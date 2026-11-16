//! Agentry's local HTTP server.
//!
//! Binds to 127.0.0.1 on a random free port, publishes the port in
//! `~/.agentry/port` and requires the token from `~/.agentry/token` on every
//! request. The Tauri app and the headless dev server both run it.
//!
//! Hook events feed the session [`store`]; every change is broadcast as a
//! fresh snapshot. Blocking requests (permissions, questions) are held open
//! until the user answers in the widget, the timeout passes, or the user's
//! settings send them to the terminal.

use std::collections::{BTreeMap, HashMap};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub mod store;

use agentry_core::config::Config;
use agentry_core::protocol::{tool_name, Decision, Envelope, EventSummary};
use agentry_core::{client, paths, random_id, TOKEN_HEADER, VERSION};
use anyhow::{bail, Context};
use axum::extract::{DefaultBodyLimit, Path as UrlPath, Request, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::{broadcast, oneshot};
use tokio::task::JoinHandle;

use crate::store::{Outcome, Pending, PendingKind, Session, Store};

/// How often expired sessions and helpers are cleared out.
const TICK_INTERVAL: Duration = Duration::from_secs(5);

/// Hook payloads include tool input and output, which can be large (a Write
/// of a big file). Generous, but bounded.
const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ServerOptions {
    /// Agentry's directory (token, port and config files).
    pub home: PathBuf,
    /// Overrides the configured approval timeout. For tests.
    pub request_timeout: Option<Duration>,
}

impl Default for ServerOptions {
    fn default() -> Self {
        ServerOptions {
            home: paths::agentry_home(),
            request_timeout: None,
        }
    }
}

/// What the server tells listeners (the UI, the dev server's log).
#[derive(Debug, Clone)]
pub enum ServerEvent {
    /// A hook event arrived. `envelope` holds the full payload, in memory only.
    Hook {
        summary: EventSummary,
        envelope: Box<Envelope>,
    },
    /// The sessions changed; this is the full list in display order.
    Sessions(Arc<Vec<Session>>),
    /// A request is now waiting in the widget. `open_panel` is the user's
    /// "open the panel when a request arrives" setting.
    RequestAdded {
        session_id: String,
        open_panel: bool,
    },
}

/// What the user chose in the widget.
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum UiResponse {
    Allow,
    /// Allow, and apply the rule Claude Code suggested.
    AlwaysAllow,
    Deny {
        /// Sent to Claude as the reason.
        #[serde(default)]
        note: Option<String>,
        /// Also stop Claude's current turn.
        #[serde(default)]
        stop: bool,
    },
    /// Hand the request back to Claude Code's own prompt.
    Terminal,
    /// Answers to `AskUserQuestion`: question text → chosen label(s), with
    /// several labels joined by ", ".
    Answer {
        answers: BTreeMap<String, String>,
    },
}

/// Turn the user's choice into the hook's decision.
pub fn decide(req: &Pending, response: &UiResponse) -> Result<(Decision, Outcome), String> {
    let deny_message = |note: &Option<String>, what: &str| match note
        .as_deref()
        .map(str::trim)
        .filter(|n| !n.is_empty())
    {
        Some(n) => format!("{what} Their note: {n}"),
        None => what.to_owned(),
    };
    match (req.kind, response) {
        (_, UiResponse::Terminal) => Ok((Decision::Terminal, Outcome::ToTerminal)),
        (PendingKind::Permission, UiResponse::Allow) => Ok((
            Decision::Allow {
                updated_input: None,
                updated_permissions: None,
            },
            Outcome::Allowed,
        )),
        (PendingKind::Permission, UiResponse::AlwaysAllow) => Ok((
            Decision::Allow {
                updated_input: None,
                updated_permissions: (!req.suggestions.is_empty())
                    .then(|| Value::Array(req.suggestions.clone())),
            },
            Outcome::Allowed,
        )),
        (PendingKind::Permission, UiResponse::Deny { note, stop }) => Ok((
            Decision::Deny {
                message: Some(deny_message(note, "The user denied this from Agentry.")),
                interrupt: *stop,
            },
            Outcome::Denied,
        )),
        (PendingKind::Question, UiResponse::Answer { answers }) => {
            if answers.is_empty() || answers.values().all(|a| a.trim().is_empty()) {
                return Err("choose an answer first".into());
            }
            let mut input = req.tool_input.clone();
            if !input.is_object() {
                input = json!({});
            }
            input["answers"] = json!(answers);
            Ok((
                Decision::Allow {
                    updated_input: Some(input),
                    updated_permissions: None,
                },
                Outcome::Answered,
            ))
        }
        (PendingKind::Question, UiResponse::Deny { note, stop }) => Ok((
            Decision::Deny {
                message: Some(deny_message(
                    note,
                    "The user chose not to answer this question in Agentry.",
                )),
                interrupt: *stop,
            },
            Outcome::Denied,
        )),
        _ => Err("that action doesn't fit this request".into()),
    }
}

/// Shared access to the sessions, for the UI's actions.
#[derive(Clone)]
pub struct Sessions {
    store: Arc<Mutex<Store>>,
    events: broadcast::Sender<ServerEvent>,
    /// Hooks waiting for an answer, by request id.
    waiters: Arc<Mutex<HashMap<String, oneshot::Sender<Decision>>>>,
    /// Whether the widget's panel is open (for "widget only while visible").
    panel_open: Arc<AtomicBool>,
}

impl Sessions {
    pub fn snapshot(&self) -> Vec<Session> {
        self.store.lock().unwrap().snapshot()
    }

    /// Hide a session until it's prompted again.
    pub fn dismiss(&self, id: &str) -> bool {
        self.update(|s| s.dismiss(id))
    }

    pub fn set_pinned(&self, id: &str, pinned: bool) -> bool {
        self.update(|s| s.set_pinned(id, pinned))
    }

    fn apply(&self, envelope: &Envelope) -> bool {
        self.update(|s| s.apply(envelope, now_ms()))
    }

    pub fn set_panel_open(&self, open: bool) {
        self.panel_open.store(open, Ordering::SeqCst);
    }

    pub fn panel_open(&self) -> bool {
        self.panel_open.load(Ordering::SeqCst)
    }

    /// Answer a pending request with the user's choice.
    pub fn respond(&self, request_id: &str, response: &UiResponse) -> Result<(), String> {
        let decision = {
            let mut store = self.store.lock().unwrap();
            let req = store.pending(request_id).ok_or(
                "this request is no longer waiting (it was answered or handed to the terminal)",
            )?;
            let (decision, outcome) = decide(req, response)?;
            store.resolve(request_id, outcome, now_ms());
            let _ = self
                .events
                .send(ServerEvent::Sessions(Arc::new(store.snapshot())));
            decision
        };
        let waiter = self.waiters.lock().unwrap().remove(request_id);
        match waiter.map(|tx| tx.send(decision).is_ok()) {
            Some(true) => Ok(()),
            _ => Err("Claude Code stopped waiting for this request".into()),
        }
    }

    /// Hand a request back to the terminal if it's still pending (timeout,
    /// or the hook went away).
    fn expire(&self, request_id: &str) {
        self.update(|s| {
            s.resolve(request_id, Outcome::ToTerminal, now_ms())
                .is_some()
        });
        self.waiters.lock().unwrap().remove(request_id);
    }

    /// Release hooks whose requests the store dropped.
    fn release_cancelled(&self) {
        let ids = self.store.lock().unwrap().take_cancelled();
        let mut waiters = self.waiters.lock().unwrap();
        for id in ids {
            if let Some(tx) = waiters.remove(&id) {
                let _ = tx.send(Decision::Terminal);
            }
        }
    }

    fn tick(&self) -> bool {
        self.update(|s| s.tick(now_ms()))
    }

    /// Run `f` on the store and broadcast a snapshot if it changed anything.
    fn update(&self, f: impl FnOnce(&mut Store) -> bool) -> bool {
        let snapshot = {
            let mut store = self.store.lock().unwrap();
            if !f(&mut store) {
                return false;
            }
            store.snapshot()
        };
        let _ = self.events.send(ServerEvent::Sessions(Arc::new(snapshot)));
        true
    }
}

#[derive(Clone)]
struct AppState {
    home: PathBuf,
    request_timeout: Option<Duration>,
    token: String,
    events: broadcast::Sender<ServerEvent>,
    sessions: Sessions,
}

pub struct Server {
    pub port: u16,
    home: PathBuf,
    events: broadcast::Sender<ServerEvent>,
    sessions: Sessions,
    shutdown: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
    ticker: Option<JoinHandle<()>>,
}

impl Server {
    /// Start listening. Fails if another Agentry instance is already serving.
    pub async fn start(opts: ServerOptions) -> anyhow::Result<Server> {
        let home = opts.home.clone();
        if let Some((port, token)) = paths::read_connection(&home) {
            let alive = tokio::task::spawn_blocking(move || {
                client::request(
                    "GET",
                    port,
                    &token,
                    "/health",
                    None,
                    Duration::from_millis(500),
                )
                .is_some_and(|r| r.status == 200)
            })
            .await
            .unwrap_or(false);
            if alive {
                bail!("Agentry is already running on port {port}");
            }
        }

        let token = paths::ensure_token(&home).context("creating the token file")?;
        let listener = tokio::net::TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0)))
            .await
            .context("binding to 127.0.0.1")?;
        let port = listener.local_addr()?.port();

        let (events, _) = broadcast::channel(1024);
        let sessions = Sessions {
            store: Arc::new(Mutex::new(Store::new())),
            events: events.clone(),
            waiters: Arc::new(Mutex::new(HashMap::new())),
            panel_open: Arc::new(AtomicBool::new(false)),
        };
        let state = AppState {
            home: home.clone(),
            request_timeout: opts.request_timeout,
            token,
            events: events.clone(),
            sessions: sessions.clone(),
        };
        let app = Router::new()
            .route("/health", get(health))
            .route("/sessions", get(list_sessions))
            .route("/event", post(event))
            .route("/request", post(request))
            .route("/requests/{id}", post(respond))
            .layer(middleware::from_fn_with_state(state.clone(), require_token))
            .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
            .with_state(state);

        let (tx, rx) = oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            let serve = axum::serve(listener, app).with_graceful_shutdown(async {
                let _ = rx.await;
            });
            if let Err(e) = serve.await {
                tracing::error!("server stopped: {e}");
            }
        });

        let ticker = tokio::spawn({
            let sessions = sessions.clone();
            async move {
                let mut interval = tokio::time::interval(TICK_INTERVAL);
                loop {
                    interval.tick().await;
                    sessions.tick();
                }
            }
        });

        paths::write_port(&home, port).context("writing the port file")?;
        tracing::info!(port, home = %home.display(), "Agentry listening on 127.0.0.1");
        Ok(Server {
            port,
            home,
            events,
            sessions,
            shutdown: Some(tx),
            task: Some(task),
            ticker: Some(ticker),
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ServerEvent> {
        self.events.subscribe()
    }

    pub fn sessions(&self) -> Sessions {
        self.sessions.clone()
    }

    /// Stop serving and remove the port file so hooks fail fast.
    pub async fn shutdown(mut self) {
        self.stop();
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }

    fn stop(&mut self) {
        if let Some(t) = self.ticker.take() {
            t.abort();
        }
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
            paths::remove_port(&self.home, self.port);
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}

async fn require_token(State(state): State<AppState>, req: Request, next: Next) -> Response {
    let given = req
        .headers()
        .get(TOKEN_HEADER)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    if !constant_time_eq(given.as_bytes(), state.token.as_bytes()) {
        return (StatusCode::UNAUTHORIZED, "missing or wrong token").into_response();
    }
    next.run(req).await
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

async fn health() -> Json<serde_json::Value> {
    Json(json!({ "ok": true, "app": "agentry", "version": VERSION }))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

async fn list_sessions(State(state): State<AppState>) -> Json<Vec<Session>> {
    Json(state.sessions.snapshot())
}

fn publish(state: &AppState, envelope: Envelope) -> EventSummary {
    let summary = EventSummary::from_payload(&envelope.payload, now_ms());
    tracing::info!(
        event = %summary.event,
        session = %summary.session_id.as_deref().map(short).unwrap_or("-"),
        tool = summary.tool_name.as_deref().unwrap_or("-"),
        agent = summary.agent_type.as_deref().unwrap_or("-"),
        notification = summary.notification_type.as_deref().unwrap_or("-"),
        blocking = summary.blocking,
        tty = envelope.context.tty.as_deref().unwrap_or("-"),
        term = envelope.context.env.get("TERM_PROGRAM").map(String::as_str).unwrap_or("-"),
        "hook event"
    );
    // Field names only (never values), to spot Claude Code payload changes.
    if tracing::enabled!(tracing::Level::DEBUG) {
        if let Some(obj) = envelope.payload.as_object() {
            let keys: Vec<&str> = obj.keys().map(String::as_str).collect();
            tracing::debug!(event = %summary.event, fields = %keys.join(","), "payload fields");
        }
    }
    state.sessions.apply(&envelope);
    state.sessions.release_cancelled();
    let _ = state.events.send(ServerEvent::Hook {
        summary: summary.clone(),
        envelope: Box::new(envelope),
    });
    summary
}

fn short(id: &str) -> &str {
    id.get(..8).unwrap_or(id)
}

async fn event(State(state): State<AppState>, Json(envelope): Json<Envelope>) -> StatusCode {
    publish(&state, envelope);
    StatusCode::NO_CONTENT
}

async fn request(State(state): State<AppState>, Json(envelope): Json<Envelope>) -> Json<Decision> {
    let payload = envelope.payload.clone();
    publish(&state, envelope);

    let config = Config::load(&state.home);
    let tool = tool_name(&payload).unwrap_or("");
    let is_question = tool == "AskUserQuestion";
    let Some(session_id) = payload.get("session_id").and_then(Value::as_str) else {
        return Json(Decision::Terminal);
    };
    if !config.widget_handles(tool, is_question, state.sessions.panel_open()) {
        return Json(Decision::Terminal);
    }

    let id = random_id();
    let now = now_ms();
    let timeout = state
        .request_timeout
        .unwrap_or(Duration::from_secs(config.timeout_secs()));
    let expires_at = now + timeout.as_millis() as u64;
    let pending = Pending::from_payload(
        &payload,
        id.clone(),
        now,
        Some(expires_at),
        config.approvals.always_allow,
    );
    let (tx, rx) = oneshot::channel();
    state
        .sessions
        .waiters
        .lock()
        .unwrap()
        .insert(id.clone(), tx);
    if !state
        .sessions
        .update(|s| s.add_pending(session_id, pending, now))
    {
        state.sessions.waiters.lock().unwrap().remove(&id);
        return Json(Decision::Terminal);
    }
    let _ = state.events.send(ServerEvent::RequestAdded {
        session_id: session_id.to_owned(),
        open_panel: config.approvals.auto_open,
    });
    tracing::info!(request = %id, tool, timeout_secs = timeout.as_secs(), "waiting for the user");

    // If the hook disconnects, this future is dropped; the guard then hands
    // the request back so the card doesn't wait forever.
    let _guard = ExpireOnDrop {
        sessions: state.sessions.clone(),
        id: id.clone(),
    };
    let decision = match tokio::time::timeout(timeout, rx).await {
        Ok(Ok(decision)) => decision,
        Ok(Err(_)) => Decision::Terminal,
        Err(_) => {
            tracing::info!(request = %id, "no answer in time; handing back to the terminal");
            Decision::Terminal
        }
    };
    tracing::info!(request = %id, decision = decision_name(&decision), "answered");
    Json(decision)
}

fn decision_name(d: &Decision) -> &'static str {
    match d {
        Decision::Allow {
            updated_input: Some(_),
            ..
        } => "answer",
        Decision::Allow { .. } => "allow",
        Decision::Deny { .. } => "deny",
        Decision::Terminal => "terminal",
    }
}

struct ExpireOnDrop {
    sessions: Sessions,
    id: String,
}

impl Drop for ExpireOnDrop {
    fn drop(&mut self) {
        self.sessions.expire(&self.id);
    }
}

/// `POST /requests/{id}`: answer a pending request (the same as the widget's
/// buttons; used by tests and scripts).
async fn respond(
    State(state): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Json(response): Json<UiResponse>,
) -> Response {
    match state.sessions.respond(&id, &response) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => (StatusCode::CONFLICT, Json(json!({ "error": e }))).into_response(),
    }
}

/// Log to stderr, `info` by default, overridable with `RUST_LOG`.
pub fn init_logging() {
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}
