//! End-to-end: the real `agentry-hook` binary against the real server.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use agentry_server::{Server, ServerEvent, ServerOptions};
use serde_json::json;
use tokio::sync::broadcast::Receiver;

fn run_hook(home: &Path, stdin: &str, extra_env: &[(&str, &str)]) -> (Output, Duration) {
    let start = Instant::now();
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_agentry-hook"));
    cmd.env("AGENTRY_HOME", home)
        .env_remove("AGENTRY_DISABLE")
        .env("TERM_PROGRAM", "iTerm.app")
        .env("ITERM_SESSION_ID", "w0t0p0:ABC")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().expect("spawn agentry-hook");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    let out = child.wait_with_output().unwrap();
    (out, start.elapsed())
}

async fn hook(home: &Path, payload: serde_json::Value) -> (Output, Duration) {
    let home = home.to_path_buf();
    tokio::task::spawn_blocking(move || run_hook(&home, &payload.to_string(), &[]))
        .await
        .unwrap()
}

async fn next_event(rx: &mut Receiver<ServerEvent>) -> ServerEvent {
    tokio::time::timeout(Duration::from_secs(5), rx.recv())
        .await
        .expect("no event within 5s")
        .unwrap()
}

fn assert_silent_success(out: &Output) {
    assert!(out.status.success(), "hook exited with {:?}", out.status);
    assert!(
        out.stdout.is_empty(),
        "unexpected stdout: {}",
        String::from_utf8_lossy(&out.stdout)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn events_reach_the_server_with_context() {
    let dir = tempfile::tempdir().unwrap();
    let server = Server::start(ServerOptions {
        home: dir.path().into(),
    })
    .await
    .unwrap();
    let mut rx = server.subscribe();

    let (out, _) = hook(
        dir.path(),
        json!({"hook_event_name": "SessionStart", "session_id": "sess-1", "cwd": "/tmp/proj",
               "source": "startup", "transcript_path": "/tmp/t.jsonl"}),
    )
    .await;
    assert_silent_success(&out);

    let ServerEvent::Hook { summary, envelope } = next_event(&mut rx).await;
    assert_eq!(summary.event, "SessionStart");
    assert_eq!(summary.session_id.as_deref(), Some("sess-1"));
    assert!(!summary.blocking);
    assert_eq!(
        envelope.context.env.get("TERM_PROGRAM").map(String::as_str),
        Some("iTerm.app")
    );
    assert_eq!(
        envelope
            .context
            .env
            .get("ITERM_SESSION_ID")
            .map(String::as_str),
        Some("w0t0p0:ABC")
    );
    assert!(envelope.context.ppid.is_some());
    assert_eq!(envelope.payload["source"], "startup");

    // Subagent tool events carry agent info.
    let (out, _) = hook(
        dir.path(),
        json!({"hook_event_name": "PostToolUse", "session_id": "sess-1", "tool_name": "Read",
               "tool_input": {"file_path": "/x"}, "agent_id": "a1", "agent_type": "Explore"}),
    )
    .await;
    assert_silent_success(&out);
    let ServerEvent::Hook { summary, .. } = next_event(&mut rx).await;
    assert_eq!(summary.agent_type.as_deref(), Some("Explore"));
    assert_eq!(summary.tool_name.as_deref(), Some("Read"));

    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn blocking_requests_fall_back_to_terminal_in_phase_1() {
    let dir = tempfile::tempdir().unwrap();
    let server = Server::start(ServerOptions {
        home: dir.path().into(),
    })
    .await
    .unwrap();
    let mut rx = server.subscribe();

    for payload in [
        json!({"hook_event_name": "PermissionRequest", "session_id": "s", "tool_name": "Bash",
               "tool_input": {"command": "rm -rf dist"}}),
        json!({"hook_event_name": "PreToolUse", "session_id": "s", "tool_name": "AskUserQuestion",
               "tool_input": {"questions": []}}),
    ] {
        let (out, _) = hook(dir.path(), payload).await;
        assert_silent_success(&out);
        let ServerEvent::Hook { summary, .. } = next_event(&mut rx).await;
        assert!(summary.blocking);
    }
    server.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn fails_open_when_app_is_not_running() {
    let dir = tempfile::tempdir().unwrap();
    let permission =
        json!({"hook_event_name": "PermissionRequest", "tool_name": "Bash", "tool_input": {}});

    // Never started: no port or token file.
    let (out, took) = hook(dir.path(), permission.clone()).await;
    assert_silent_success(&out);
    assert!(took < Duration::from_millis(500), "took {took:?}");

    // Started then stopped: shutdown removes the port file.
    let server = Server::start(ServerOptions {
        home: dir.path().into(),
    })
    .await
    .unwrap();
    server.shutdown().await;
    assert!(!dir.path().join("port").exists());

    // Crashed: a stale port file pointing at nothing.
    let free = std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port();
    std::fs::write(dir.path().join("port"), free.to_string()).unwrap();
    let (out, took) = hook(dir.path(), permission).await;
    assert_silent_success(&out);
    assert!(took < Duration::from_millis(500), "took {took:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn rejects_wrong_token_and_bad_input_silently() {
    let dir = tempfile::tempdir().unwrap();
    let server = Server::start(ServerOptions {
        home: dir.path().into(),
    })
    .await
    .unwrap();
    let mut rx = server.subscribe();

    // Garbage and non-hook input: exit 0, nothing sent.
    for input in ["", "not json", "[]", r#"{"no_event": true}"#] {
        let home = dir.path().to_path_buf();
        let (out, _) = tokio::task::spawn_blocking(move || run_hook(&home, input, &[]))
            .await
            .unwrap();
        assert_silent_success(&out);
    }

    // Disabled by env var: nothing sent.
    let home = dir.path().to_path_buf();
    let (out, _) = tokio::task::spawn_blocking(move || {
        run_hook(
            &home,
            r#"{"hook_event_name":"Stop"}"#,
            &[("AGENTRY_DISABLE", "1")],
        )
    })
    .await
    .unwrap();
    assert_silent_success(&out);

    // Wrong token: the server refuses it, the hook stays silent.
    std::fs::write(dir.path().join("token"), "x".repeat(64)).unwrap();
    let (out, _) = hook(
        dir.path(),
        json!({"hook_event_name": "PermissionRequest", "tool_name": "Bash"}),
    )
    .await;
    assert_silent_success(&out);

    assert!(
        tokio::time::timeout(Duration::from_millis(300), rx.recv())
            .await
            .is_err(),
        "no event should have reached the server"
    );
    drop(server);
}

#[tokio::test(flavor = "multi_thread")]
async fn second_instance_refuses_to_start() {
    let dir = tempfile::tempdir().unwrap();
    let first = Server::start(ServerOptions {
        home: dir.path().into(),
    })
    .await
    .unwrap();
    let second = Server::start(ServerOptions {
        home: dir.path().into(),
    })
    .await;
    assert!(second.is_err());
    first.shutdown().await;
    // Once the first is gone, a new one can start.
    let third = Server::start(ServerOptions {
        home: dir.path().into(),
    })
    .await
    .unwrap();
    third.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn hook_is_fast() {
    let dir = tempfile::tempdir().unwrap();
    let server = Server::start(ServerOptions {
        home: dir.path().into(),
    })
    .await
    .unwrap();
    let payload = json!({"hook_event_name": "PostToolUse", "session_id": "s", "tool_name": "Read", "tool_input": {}});
    let mut times = Vec::new();
    for _ in 0..20 {
        let (out, took) = hook(dir.path(), payload.clone()).await;
        assert_silent_success(&out);
        times.push(took);
    }
    times.sort();
    let median = times[times.len() / 2];
    eprintln!(
        "hook round trip: median {median:?}, max {:?}",
        times.last().unwrap()
    );
    // Generous bound for slow CI; typical is a few milliseconds.
    assert!(median < Duration::from_millis(100), "median {median:?}");
    server.shutdown().await;
}
