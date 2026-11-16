//! Replays real Claude Code sessions (captured from 2.1.325, then sanitised)
//! through the session store.

use agentry_core::protocol::{Envelope, HookContext};
use agentry_server::store::{Status, Store};
use serde_json::Value;

/// Events Agentry actually registers for (the captures recorded every
/// PreToolUse; Agentry only asks for AskUserQuestion's).
fn registered(p: &Value) -> bool {
    match p["hook_event_name"].as_str() {
        Some("PreToolUse") => p["tool_name"] == "AskUserQuestion",
        _ => true,
    }
}

/// What the card showed after one event.
struct Frame {
    event: String,
    status: Status,
    subtitle: String,
    progress: Option<(usize, usize)>,
    /// (agent type, subtitle, done)
    helpers: Vec<(String, String, bool)>,
}

fn replay(fixture: &str) -> Vec<Frame> {
    let text = std::fs::read_to_string(format!(
        "{}/tests/fixtures/{fixture}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap();
    let mut store = Store::new();
    let mut frames = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let payload: Value = serde_json::from_str(line).unwrap();
        if !registered(&payload) {
            continue;
        }
        let event = payload["hook_event_name"].as_str().unwrap().to_owned();
        store.apply(
            &Envelope {
                payload,
                context: HookContext::default(),
            },
            i as u64,
        );
        let s = &store.snapshot()[0];
        frames.push(Frame {
            event,
            status: s.status,
            subtitle: s.subtitle.clone(),
            progress: s.progress.as_ref().map(|p| (p.done, p.total)),
            helpers: s
                .helpers
                .iter()
                .map(|h| (h.agent_type.clone(), h.subtitle.clone(), h.done))
                .collect(),
        });
    }
    frames
}

#[test]
fn task_session_shows_live_progress() {
    let frames = replay("tasks.jsonl");
    for f in &frames {
        eprintln!(
            "{:<18} {:<14} {:<28} {:?}",
            f.event,
            format!("{:?}", f.status),
            f.subtitle,
            f.progress
        );
    }

    // Progress only ever reflects real task state, and ends complete.
    let progress: Vec<(usize, usize)> = frames.iter().filter_map(|f| f.progress).collect();
    assert_eq!(progress.first(), Some(&(0, 1)));
    assert_eq!(progress.last(), Some(&(3, 3)));
    assert!(
        progress.windows(2).all(|w| w[1].1 >= w[0].1),
        "total never shrinks here"
    );

    // While a task is in progress, the subtitle is its activeForm.
    let subtitles: Vec<&str> = frames.iter().map(|f| f.subtitle.as_str()).collect();
    for expected in ["Writing a.txt", "Writing b.txt", "Counting files"] {
        assert!(
            subtitles.contains(&expected),
            "missing subtitle {expected:?} in {subtitles:?}"
        );
    }

    let last = frames.last().unwrap();
    assert_eq!(last.status, Status::Done);
    let stop = frames.iter().rev().find(|f| f.event == "Stop").unwrap();
    assert_eq!(
        (stop.status, stop.subtitle.as_str()),
        (Status::Idle, "Finished — ready for you")
    );
}

#[test]
fn subagent_session_nests_helpers_and_flags_permission() {
    let frames = replay("subagent.jsonl");
    for f in &frames {
        eprintln!(
            "{:<18} {:<14} {:<34} {:?}",
            f.event,
            format!("{:?}", f.status),
            f.subtitle,
            f.helpers
        );
    }

    // Only one session: the subagent's events belong to the parent.
    let start = frames.iter().find(|f| f.event == "SubagentStart").unwrap();
    assert_eq!(
        start.helpers,
        vec![("Explore".to_string(), "Starting…".to_string(), false)]
    );

    let during = frames
        .iter()
        .find(|f| f.event == "PostToolUse" && !f.helpers.is_empty() && !f.helpers[0].2)
        .unwrap();
    assert_eq!(during.helpers[0].1, "List files in current directory");

    let stop = frames.iter().find(|f| f.event == "SubagentStop").unwrap();
    assert!(stop.helpers[0].2, "helper marked done");

    let perm = frames
        .iter()
        .find(|f| f.event == "PermissionRequest")
        .unwrap();
    assert_eq!(
        (perm.status, perm.subtitle.as_str()),
        (Status::NeedsTerminal, "Wants to read /etc/hostname")
    );

    let end = frames.iter().find(|f| f.event == "Stop").unwrap();
    assert_eq!(end.status, Status::Idle);
}
