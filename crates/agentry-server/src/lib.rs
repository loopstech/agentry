//! Agentry's local HTTP server.
//!
//! Binds to 127.0.0.1 on a random free port, publishes the port in
//! `~/.agentry/port` and requires the token from `~/.agentry/token` on every
//! request. The Tauri app and the headless dev server both run it.
//!
//! Hook events feed the session [`store`]; every change is broadcast as a
//! fresh snapshot. Approval requests are still answered straight away with
//! "use the terminal"; the approval broker comes in Phase 3.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub mod store;

use agentry_core::protocol::{Decision, Envelope, EventSummary};
use agentry_core::{client, paths, TOKEN_HEADER, VERSION};
use anyhow::{bail, Context};
use axum::extract::{DefaultBodyLimit, Request, State};
use axum::http::StatusCode;
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde_json::json;
use tokio::sync::{broadcast, oneshot};
use tokio::task::JoinHandle;

use crate::store::{Session, Store};

/// How often expired sessions and helpers are cleared out.
const TICK_INTERVAL: Duration = Duration::from_secs(5);

/// Hook payloads include tool input and output, which can be large (a Write
/// of a big file). Generous, but bounded.
const MAX_BODY_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, Clone)]
pub struct ServerOptions {
    /// Agentry's directory (token and port files).
    pub home: PathBuf,
}

impl Default for ServerOptions {
    fn default() -> Self {
        ServerOptions {
            home: paths::agentry_home(),
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
}

/// Shared access to the sessions, for the UI's actions.
#[derive(Clone)]
pub struct Sessions {
    store: Arc<Mutex<Store>>,
    events: broadcast::Sender<ServerEvent>,
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
        let home = opts.home;
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
        };
        let state = AppState {
            token,
            events: events.clone(),
            sessions: sessions.clone(),
        };
        let app = Router::new()
            .route("/health", get(health))
            .route("/sessions", get(list_sessions))
            .route("/event", post(event))
            .route("/request", post(request))
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
    publish(&state, envelope);
    // Phase 1: no approval UI yet, so hand every request back to the terminal.
    Json(Decision::Terminal)
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
