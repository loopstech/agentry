//! Approvals and questions, end to end: the real `agentry-hook` binary
//! blocks on the real server until the "widget" (Sessions::respond, the same
//! call the UI makes) answers, then prints what Claude Code reads.

use std::io::Write;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use agentry_server::store::Status;
use agentry_server::{Server, ServerOptions, Sessions, UiResponse};
use serde_json::{json, Value};

fn spawn_hook(home: &Path, payload: &Value) -> Child {
    let mut child = Command::new(env!("CARGO_BIN_EXE_agentry-hook"))
        .env("AGENTRY_HOME", home)
        .env_remove("AGENTRY_DISABLE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(payload.to_string().as_bytes())
        .unwrap();
    child
}

/// Wait for the hook to exit; returns its stdout parsed as JSON (or Null when
/// it printed nothing) and how long it took.
async fn finish(child: Child) -> (Value, Duration) {
    let start = Instant::now();
    let out = tokio::task::spawn_blocking(move || child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert!(out.status.success());
    let text = String::from_utf8(out.stdout).unwrap();
    let v = if text.trim().is_empty() {
        Value::Null
    } else {
        serde_json::from_str(&text).unwrap()
    };
    (v, start.elapsed())
}

/// The id of the first request pending for `session`, once it appears.
async fn pending_id(sessions: &Sessions, session: &str) -> String {
    for _ in 0..200 {
        if let Some(s) = sessions.snapshot().into_iter().find(|s| s.id == session) {
            if let Some(p) = s.pending.first() {
                return p.id.clone();
            }
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("no pending request appeared for {session}");
}

async fn start(dir: &Path, timeout: Option<Duration>) -> Server {
    Server::start(ServerOptions {
        home: dir.into(),
        request_timeout: timeout,
    })
    .await
    .unwrap()
}

fn permission(session: &str, command: &str) -> Value {
    json!({
        "hook_event_name": "PermissionRequest", "session_id": session, "cwd": "/w/web",
        "tool_name": "Bash", "tool_input": {"command": command, "description": "Install"},
        "permission_suggestions": [
            {"type": "addRules", "rules": [{"toolName": "Bash", "ruleContent": "pnpm add:*"}],
             "behavior": "allow", "destination": "localSettings"}
        ]
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn allow_from_the_widget() {
    let dir = tempfile::tempdir().unwrap();
    let server = start(dir.path(), None).await;
    let child = spawn_hook(dir.path(), &permission("s1", "pnpm add zod"));

    let id = pending_id(&server.sessions(), "s1").await;
    let card = server.sessions().snapshot().remove(0);
    assert_eq!(card.status, Status::NeedsPermission);
    assert_eq!(card.subtitle, "Wants to run: pnpm add zod");
    let p = &card.pending[0];
    assert_eq!(p.detail.as_deref(), Some("pnpm add zod"));
    assert_eq!(
        p.always_allow_label.as_deref(),
        Some("Always allow Bash(pnpm add:*) in this project")
    );
    assert!(p.expires_at.is_some());

    server.sessions().respond(&id, &UiResponse::Allow).unwrap();
    let (out, _) = finish(child).await;
    assert_eq!(
        out,
        json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": {"behavior": "allow"}}})
    );
    let card = server.sessions().snapshot().remove(0);
    assert_eq!((card.status, card.pending.len()), (Status::Working, 0));
    assert_eq!(card.subtitle, "Approved — carrying on");

    // Answering twice is refused, not a crash.
    assert!(server.sessions().respond(&id, &UiResponse::Allow).is_err());
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn deny_with_a_note_and_stop() {
    let dir = tempfile::tempdir().unwrap();
    let server = start(dir.path(), None).await;
    let child = spawn_hook(dir.path(), &permission("s1", "rm -rf dist"));
    let id = pending_id(&server.sessions(), "s1").await;

    server
        .sessions()
        .respond(
            &id,
            &UiResponse::Deny {
                note: Some("use pnpm clean".into()),
                stop: true,
            },
        )
        .unwrap();
    let (out, _) = finish(child).await;
    assert_eq!(
        out,
        json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": {
            "behavior": "deny",
            "message": "The user denied this from Agentry. Their note: use pnpm clean",
            "interrupt": true}}})
    );
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn always_allow_sends_back_the_suggested_rule() {
    let dir = tempfile::tempdir().unwrap();
    let server = start(dir.path(), None).await;
    let payload = permission("s1", "pnpm add zod");
    let child = spawn_hook(dir.path(), &payload);
    let id = pending_id(&server.sessions(), "s1").await;

    server
        .sessions()
        .respond(&id, &UiResponse::AlwaysAllow)
        .unwrap();
    let (out, _) = finish(child).await;
    assert_eq!(
        out["hookSpecificOutput"]["decision"],
        json!({"behavior": "allow", "updatedPermissions": payload["permission_suggestions"]})
    );
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn answer_a_question() {
    let dir = tempfile::tempdir().unwrap();
    let server = start(dir.path(), None).await;
    let questions = json!([
        {"question": "Which version bump?", "header": "Version",
         "options": [{"label": "Patch"}, {"label": "Minor"}, {"label": "Major"}], "multiSelect": false},
        {"question": "Which platforms?", "header": "Platforms",
         "options": [{"label": "macOS"}, {"label": "Linux"}], "multiSelect": true}
    ]);
    let child = spawn_hook(
        dir.path(),
        &json!({"hook_event_name": "PreToolUse", "session_id": "s1", "cwd": "/w/docs",
                "tool_name": "AskUserQuestion", "tool_input": {"questions": questions}}),
    );
    let id = pending_id(&server.sessions(), "s1").await;
    let card = server.sessions().snapshot().remove(0);
    assert_eq!(card.status, Status::NeedsAnswer);
    assert_eq!(card.pending[0].questions.len(), 2);
    assert!(card.pending[0].questions[1].multi_select);

    // Wrong kind of action, and empty answers, are refused.
    assert!(server.sessions().respond(&id, &UiResponse::Allow).is_err());
    assert!(server
        .sessions()
        .respond(
            &id,
            &UiResponse::Answer {
                answers: Default::default()
            }
        )
        .is_err());

    let answers = [
        ("Which version bump?", "Minor"),
        ("Which platforms?", "macOS, Linux"),
    ]
    .into_iter()
    .map(|(q, a)| (q.to_string(), a.to_string()))
    .collect();
    server
        .sessions()
        .respond(&id, &UiResponse::Answer { answers })
        .unwrap();
    let (out, _) = finish(child).await;
    assert_eq!(
        out,
        json!({"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "allow",
            "updatedInput": {"questions": questions,
                "answers": {"Which platforms?": "macOS, Linux", "Which version bump?": "Minor"}}}})
    );
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn timeout_hands_back_to_the_terminal() {
    let dir = tempfile::tempdir().unwrap();
    let server = start(dir.path(), Some(Duration::from_millis(600))).await;
    let child = spawn_hook(dir.path(), &permission("s1", "make deploy"));
    pending_id(&server.sessions(), "s1").await;

    let (out, took) = finish(child).await;
    assert_eq!(
        out,
        Value::Null,
        "no output: Claude Code shows its own prompt"
    );
    assert!(took < Duration::from_secs(3), "took {took:?}");
    let card = server.sessions().snapshot().remove(0);
    assert_eq!(
        (card.status, card.pending.len()),
        (Status::NeedsTerminal, 0)
    );
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn terminal_button_hands_back_immediately() {
    let dir = tempfile::tempdir().unwrap();
    let server = start(dir.path(), None).await;
    let child = spawn_hook(dir.path(), &permission("s1", "make deploy"));
    let id = pending_id(&server.sessions(), "s1").await;
    server
        .sessions()
        .respond(&id, &UiResponse::Terminal)
        .unwrap();
    let (out, _) = finish(child).await;
    assert_eq!(out, Value::Null);
    assert_eq!(
        server.sessions().snapshot()[0].status,
        Status::NeedsTerminal
    );
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn settings_decide_who_answers() {
    let dir = tempfile::tempdir().unwrap();
    let server = start(dir.path(), None).await;
    let write = |c: &str| std::fs::write(dir.path().join("config.json"), c).unwrap();

    // Muted: straight to the terminal.
    write(r#"{"muted": true}"#);
    let (out, took) = finish(spawn_hook(dir.path(), &permission("s1", "ls"))).await;
    assert_eq!(out, Value::Null);
    assert!(took < Duration::from_secs(1));

    // Tool not in the list: terminal. Questions still come to the widget.
    write(r#"{"approvals": {"tools": ["Write"]}}"#);
    let (out, _) = finish(spawn_hook(dir.path(), &permission("s1", "ls"))).await;
    assert_eq!(out, Value::Null);

    // Only while the panel is open.
    write(r#"{"approvals": {"mode": "widget_when_visible"}}"#);
    let (out, _) = finish(spawn_hook(dir.path(), &permission("s1", "ls"))).await;
    assert_eq!(out, Value::Null);
    server.sessions().set_panel_open(true);
    let child = spawn_hook(dir.path(), &permission("s1", "ls"));
    let id = pending_id(&server.sessions(), "s1").await;
    server.sessions().respond(&id, &UiResponse::Allow).unwrap();
    assert_eq!(
        finish(child).await.0["hookSpecificOutput"]["decision"]["behavior"],
        "allow"
    );
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn session_moving_on_releases_the_hook() {
    let dir = tempfile::tempdir().unwrap();
    let server = start(dir.path(), None).await;
    let child = spawn_hook(dir.path(), &permission("s1", "make deploy"));
    pending_id(&server.sessions(), "s1").await;

    // e.g. the user interrupted Claude and typed a new prompt.
    let (_, _) = finish(spawn_hook(
        dir.path(),
        &json!({"hook_event_name": "UserPromptSubmit", "session_id": "s1", "prompt": "never mind"}),
    ))
    .await;
    let (out, took) = finish(child).await;
    assert_eq!(out, Value::Null);
    assert!(took < Duration::from_secs(2));
    assert!(server.sessions().snapshot()[0].pending.is_empty());
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn two_sessions_wait_independently() {
    let dir = tempfile::tempdir().unwrap();
    let server = start(dir.path(), None).await;
    let a = spawn_hook(dir.path(), &permission("a", "echo a"));
    let b = spawn_hook(dir.path(), &permission("b", "echo b"));
    let ida = pending_id(&server.sessions(), "a").await;
    let idb = pending_id(&server.sessions(), "b").await;
    server
        .sessions()
        .respond(
            &idb,
            &UiResponse::Deny {
                note: None,
                stop: false,
            },
        )
        .unwrap();
    server.sessions().respond(&ida, &UiResponse::Allow).unwrap();
    assert_eq!(
        finish(a).await.0["hookSpecificOutput"]["decision"]["behavior"],
        "allow"
    );
    assert_eq!(
        finish(b).await.0["hookSpecificOutput"]["decision"]["behavior"],
        "deny"
    );
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn killed_hook_clears_the_card() {
    // Claude Code kills the hook if the user interrupts. Whether the server
    // notices the closed connection right away is checked here.
    let dir = tempfile::tempdir().unwrap();
    let server = start(dir.path(), None).await;
    let mut child = spawn_hook(dir.path(), &permission("s1", "sleep 100"));
    pending_id(&server.sessions(), "s1").await;
    child.kill().unwrap();
    let _ = child.wait();

    let mut cleared = false;
    for _ in 0..40 {
        if server.sessions().snapshot()[0].pending.is_empty() {
            cleared = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(
        cleared,
        "pending request should clear when the hook goes away"
    );
    server.shutdown().await;
}
