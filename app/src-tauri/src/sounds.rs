//! Sound cues: a session starts waiting on you, or finishes a turn.
//!
//! Played natively with macOS's built-in system sounds (`afplay`), so they
//! work even when the webview hasn't been clicked yet (browsers block audio
//! until then). A no-op on other platforms.

use std::collections::HashMap;

use agentry_core::config::{Config, Theme};
use agentry_server::store::{Session, Status};
use serde::Deserialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cue {
    NeedsYou,
    Finished,
}

/// Notices status changes worth a sound between session snapshots.
#[derive(Default)]
pub struct CueTracker {
    last: HashMap<String, Status>,
}

impl CueTracker {
    /// Cues for this snapshot, at most one of each kind.
    pub fn observe(&mut self, sessions: &[Session]) -> Vec<Cue> {
        let mut cues = Vec::new();
        let mut next = HashMap::with_capacity(sessions.len());
        for s in sessions {
            let before = self.last.get(&s.id).copied();
            if !s.backfilled {
                if s.status.needs_user()
                    && !before.is_some_and(Status::needs_user)
                    && !cues.contains(&Cue::NeedsYou)
                {
                    cues.push(Cue::NeedsYou);
                }
                if s.status == Status::Idle
                    && before == Some(Status::Working)
                    && !cues.contains(&Cue::Finished)
                {
                    cues.push(Cue::Finished);
                }
            }
            next.insert(s.id.clone(), s.status);
        }
        self.last = next;
        cues
    }
}

/// The system sound for a cue in a theme.
pub fn sound_name(cue: Cue, theme: Theme) -> &'static str {
    match (theme, cue) {
        (Theme::Mono, Cue::NeedsYou) => "Tink",
        (Theme::Mono, Cue::Finished) => "Purr",
        (Theme::Crew, Cue::NeedsYou) => "Pop",
        (Theme::Crew, Cue::Finished) => "Hero",
        (Theme::Terminal, Cue::NeedsYou) => "Morse",
        (Theme::Terminal, Cue::Finished) => "Bottle",
        (Theme::Paper, Cue::NeedsYou) => "Ping",
        (Theme::Paper, Cue::Finished) => "Blow",
        (Theme::Pixel, Cue::NeedsYou) => "Funk",
        (Theme::Pixel, Cue::Finished) => "Frog",
        (Theme::Glass, Cue::NeedsYou) => "Glass",
        (Theme::Glass, Cue::Finished) => "Submarine",
    }
}

/// Play a cue if the user's settings allow it.
pub fn play_if_enabled(cue: Cue, config: &Config) {
    if config.muted || !config.sounds.enabled || config.sounds.volume <= 0.0 {
        return;
    }
    play(cue, config.appearance.theme, config.sounds.volume);
}

/// Play a cue regardless of settings (the Settings window's "Test" button).
pub fn play(cue: Cue, theme: Theme, volume: f32) {
    let name = sound_name(cue, theme);
    if cfg!(target_os = "macos") {
        let path = format!("/System/Library/Sounds/{name}.aiff");
        if std::path::Path::new(&path).exists() {
            let _ = std::process::Command::new("afplay")
                .args(["-v", &format!("{:.2}", volume.clamp(0.0, 1.0))])
                .arg(path)
                .spawn();
        }
    } else {
        tracing::debug!(sound = name, "sound cue (only played on macOS)");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentry_core::protocol::{Envelope, HookContext};
    use agentry_server::store::Store;
    use serde_json::json;

    fn snapshot(store: &mut Store, t: u64, payload: serde_json::Value) -> Vec<Session> {
        store.apply(
            &Envelope {
                payload,
                context: HookContext::default(),
            },
            t,
        );
        store.snapshot()
    }

    #[test]
    fn cues_on_waiting_and_finishing() {
        let mut store = Store::new();
        let mut cues = CueTracker::default();
        let prompt = json!({"hook_event_name": "UserPromptSubmit", "session_id": "a", "cwd": "/w/a", "prompt": "go"});
        assert!(cues.observe(&snapshot(&mut store, 1, prompt)).is_empty());

        let ask =
            json!({"hook_event_name": "PermissionRequest", "session_id": "a", "tool_name": "Bash"});
        assert_eq!(
            cues.observe(&snapshot(&mut store, 2, ask.clone())),
            vec![Cue::NeedsYou]
        );
        // Still waiting: no repeat.
        assert!(cues.observe(&snapshot(&mut store, 3, ask)).is_empty());

        let tool = json!({"hook_event_name": "PostToolUse", "session_id": "a", "tool_name": "Bash", "tool_input": {}});
        assert!(cues.observe(&snapshot(&mut store, 4, tool)).is_empty());
        let stop = json!({"hook_event_name": "Stop", "session_id": "a"});
        assert_eq!(
            cues.observe(&snapshot(&mut store, 5, stop.clone())),
            vec![Cue::Finished]
        );
        assert!(cues.observe(&snapshot(&mut store, 6, stop)).is_empty());
    }

    #[test]
    fn no_cues_for_sessions_picked_up_at_launch() {
        let mut store = Store::new();
        store.backfill("b", "/w/b", "/t/b.jsonl", None, 1);
        let mut cues = CueTracker::default();
        assert!(cues.observe(&store.snapshot()).is_empty());
    }

    #[test]
    fn settings_gate_sounds() {
        // Only checks the gate doesn't panic; playback itself is macOS-only.
        let c = Config {
            muted: true,
            ..Default::default()
        };
        play_if_enabled(Cue::NeedsYou, &c);
        assert_eq!(sound_name(Cue::Finished, Theme::Crew), "Hero");
        // Each theme has its own pair of sounds.
        let pairs: std::collections::HashSet<_> = Theme::ALL
            .into_iter()
            .map(|t| (sound_name(Cue::NeedsYou, t), sound_name(Cue::Finished, t)))
            .collect();
        assert_eq!(pairs.len(), Theme::ALL.len());
    }
}
