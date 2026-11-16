//! `agentry-hook`: Claude Code runs this for every registered hook event.
//!
//! It reads the event JSON on stdin and forwards it to the Agentry app on
//! 127.0.0.1. For approvals and questions it waits for the app's answer and
//! prints the decision JSON Claude Code expects.
//!
//! It always fails open: whatever goes wrong (app not running, bad input,
//! timeouts), it exits 0 without printing anything, so Claude Code carries on
//! exactly as if Agentry weren't installed.

use std::io::Read;
use std::process::Command;
use std::time::Duration;

use agentry_core::client;
use agentry_core::paths;
use agentry_core::protocol::{self, Decision, Envelope, HookContext, CONTEXT_ENV_VARS};
use serde_json::Value;

/// How long to wait for the app to take a fire-and-forget event.
const EVENT_READ_TIMEOUT: Duration = Duration::from_secs(2);
/// Upper bound on waiting for an answer. The app enforces the user's own,
/// shorter fallback timeout; Claude Code's hook timeout is the outer limit.
const REQUEST_READ_TIMEOUT: Duration = Duration::from_secs(3700);

fn main() {
    if std::env::var_os("AGENTRY_DISABLE").is_some_and(|v| !v.is_empty() && v != "0") {
        drain_stdin();
        return;
    }
    if let Some(output) = run() {
        println!("{output}");
    }
}

fn drain_stdin() {
    let _ = std::io::copy(&mut std::io::stdin().lock(), &mut std::io::sink());
}

/// Returns the JSON to print on stdout, if any.
fn run() -> Option<String> {
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input).ok()?;
    let payload: Value = serde_json::from_str(&input).ok()?;
    let event = protocol::event_name(&payload)?.to_owned();
    let blocking = protocol::is_blocking(&payload);

    let (port, token) = paths::read_connection(&paths::agentry_home())?;

    // Looking up the TTY spawns `ps`, so only do it when the session's
    // terminal is first learned or might be needed right away.
    let want_tty = blocking || matches!(event.as_str(), "SessionStart" | "UserPromptSubmit");
    let envelope = Envelope {
        payload,
        context: gather_context(want_tty),
    };
    let body = serde_json::to_string(&envelope).ok()?;

    if !blocking {
        client::request(
            "POST",
            port,
            &token,
            "/event",
            Some(&body),
            EVENT_READ_TIMEOUT,
        );
        return None;
    }

    let response = client::request(
        "POST",
        port,
        &token,
        "/request",
        Some(&body),
        REQUEST_READ_TIMEOUT,
    )?;
    if response.status != 200 {
        return None;
    }
    let decision: Decision = serde_json::from_str(&response.body).ok()?;
    protocol::hook_output(&event, &decision).map(|v| v.to_string())
}

fn gather_context(want_tty: bool) -> HookContext {
    let ppid = parent_pid();
    let env = CONTEXT_ENV_VARS
        .iter()
        .filter_map(|k| {
            std::env::var(k)
                .ok()
                .filter(|v| !v.is_empty())
                .map(|v| (k.to_string(), v))
        })
        .collect();
    let tty = if want_tty {
        ppid.and_then(tty_of)
    } else {
        None
    };
    HookContext {
        hook_version: agentry_core::VERSION.into(),
        ppid,
        tty,
        env,
    }
}

#[cfg(unix)]
fn parent_pid() -> Option<u32> {
    Some(std::os::unix::process::parent_id())
}

#[cfg(not(unix))]
fn parent_pid() -> Option<u32> {
    None
}

/// Controlling terminal of `pid`, e.g. `ttys004`. `None` for no terminal.
fn tty_of(pid: u32) -> Option<String> {
    let out = Command::new("ps")
        .args(["-o", "tty=", "-p", &pid.to_string()])
        .output()
        .ok()?;
    let tty = String::from_utf8(out.stdout).ok()?.trim().to_string();
    match tty.as_str() {
        "" | "?" | "??" => None,
        _ => Some(tty),
    }
}
