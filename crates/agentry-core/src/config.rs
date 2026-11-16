//! User settings, in `~/.agentry/config.json`. Written by the app (and
//! `agentry config`), read by the server on every approval request so changes
//! apply immediately.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    /// The widget answers first; the terminal takes over after the timeout.
    #[default]
    WidgetFirst,
    /// Only show status; every prompt goes to the terminal.
    TerminalOnly,
    /// The widget answers only while its panel is open.
    WidgetWhenVisible,
}

impl ApprovalMode {
    pub const ALL: [(&'static str, ApprovalMode); 3] = [
        ("widget_first", ApprovalMode::WidgetFirst),
        ("terminal_only", ApprovalMode::TerminalOnly),
        ("widget_when_visible", ApprovalMode::WidgetWhenVisible),
    ];
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Approvals {
    pub mode: ApprovalMode,
    /// Seconds before handing a request back to the terminal. `None` = never.
    pub timeout_secs: Option<u64>,
    /// Tools the widget handles. `None` = all tools.
    pub tools: Option<Vec<String>>,
    /// Offer "Always allow" when Claude Code suggests a rule.
    pub always_allow: bool,
    /// Answer `AskUserQuestion` from the widget.
    pub answer_questions: bool,
    /// Open the panel when a new request arrives.
    pub auto_open: bool,
}

impl Default for Approvals {
    fn default() -> Self {
        Approvals {
            mode: ApprovalMode::WidgetFirst,
            timeout_secs: Some(DEFAULT_TIMEOUT_SECS),
            tools: None,
            always_allow: true,
            answer_questions: true,
            auto_open: true,
        }
    }
}

pub const DEFAULT_TIMEOUT_SECS: u64 = 90;
pub const MIN_TIMEOUT_SECS: u64 = 15;
/// Claude Code's hook timeout is 3600 s; answer well before it gives up.
pub const MAX_TIMEOUT_SECS: u64 = 3500;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub approvals: Approvals,
    /// Quiet mode: no sounds, and every prompt goes to the terminal.
    pub muted: bool,
}

impl Config {
    pub fn file(home: &Path) -> PathBuf {
        home.join("config.json")
    }

    /// Load settings; a missing or unreadable file gives the defaults.
    pub fn load(home: &Path) -> Config {
        std::fs::read_to_string(Self::file(home))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, home: &Path) -> std::io::Result<()> {
        std::fs::create_dir_all(home)?;
        let mut text = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        text.push('\n');
        crate::paths::write_private(&Self::file(home), text.as_bytes())
    }

    /// The effective wait before handing a request to the terminal.
    pub fn timeout_secs(&self) -> u64 {
        self.approvals
            .timeout_secs
            .map(|t| t.clamp(MIN_TIMEOUT_SECS, MAX_TIMEOUT_SECS))
            .unwrap_or(MAX_TIMEOUT_SECS)
    }

    /// Whether the widget should handle a request for `tool`.
    pub fn widget_handles(&self, tool: &str, is_question: bool, panel_open: bool) -> bool {
        if self.muted {
            return false;
        }
        let mode_ok = match self.approvals.mode {
            ApprovalMode::WidgetFirst => true,
            ApprovalMode::TerminalOnly => false,
            ApprovalMode::WidgetWhenVisible => panel_open,
        };
        let kind_ok = if is_question {
            self.approvals.answer_questions
        } else {
            self.approvals
                .tools
                .as_ref()
                .is_none_or(|t| t.iter().any(|x| x.eq_ignore_ascii_case(tool)))
        };
        mode_ok && kind_ok
    }

    /// Settable keys for `agentry config`, with a description.
    pub const KEYS: &'static [(&'static str, &'static str)] = &[
        (
            "approvals.mode",
            "widget_first | terminal_only | widget_when_visible",
        ),
        (
            "approvals.timeout",
            "seconds before the terminal takes over (15–3500), or \"never\"",
        ),
        (
            "approvals.tools",
            "\"all\", or a comma-separated list such as Bash,Write,Edit",
        ),
        (
            "approvals.alwaysAllow",
            "true | false — offer \"Always allow\"",
        ),
        (
            "approvals.answerQuestions",
            "true | false — answer Claude's questions in the widget",
        ),
        (
            "approvals.autoOpen",
            "true | false — open the panel when a request arrives",
        ),
        (
            "muted",
            "true | false — quiet mode: everything goes to the terminal",
        ),
    ];

    pub fn get(&self, key: &str) -> Option<String> {
        let a = &self.approvals;
        Some(match key {
            "approvals.mode" => ApprovalMode::ALL
                .iter()
                .find(|(_, m)| *m == a.mode)?
                .0
                .to_string(),
            "approvals.timeout" => a.timeout_secs.map_or("never".into(), |t| t.to_string()),
            "approvals.tools" => a.tools.as_ref().map_or("all".into(), |t| t.join(",")),
            "approvals.alwaysAllow" => a.always_allow.to_string(),
            "approvals.answerQuestions" => a.answer_questions.to_string(),
            "approvals.autoOpen" => a.auto_open.to_string(),
            "muted" => self.muted.to_string(),
            _ => return None,
        })
    }

    pub fn set(&mut self, key: &str, value: &str) -> Result<(), String> {
        let v = value.trim();
        let boolean = || match v.to_ascii_lowercase().as_str() {
            "true" | "on" | "yes" | "1" => Ok(true),
            "false" | "off" | "no" | "0" => Ok(false),
            _ => Err(format!("{key} must be true or false")),
        };
        let a = &mut self.approvals;
        match key {
            "approvals.mode" => {
                a.mode = ApprovalMode::ALL
                    .iter()
                    .find(|(n, _)| *n == v)
                    .map(|(_, m)| *m)
                    .ok_or_else(|| "approvals.mode must be one of: widget_first, terminal_only, widget_when_visible".to_string())?
            }
            "approvals.timeout" => {
                a.timeout_secs = if v.eq_ignore_ascii_case("never") {
                    None
                } else {
                    let n: u64 = v.parse().map_err(|_| "approvals.timeout must be a number of seconds or \"never\"".to_string())?;
                    if !(MIN_TIMEOUT_SECS..=MAX_TIMEOUT_SECS).contains(&n) {
                        return Err(format!("approvals.timeout must be between {MIN_TIMEOUT_SECS} and {MAX_TIMEOUT_SECS}"));
                    }
                    Some(n)
                }
            }
            "approvals.tools" => {
                a.tools = if v.eq_ignore_ascii_case("all") || v.is_empty() {
                    None
                } else {
                    Some(v.split(',').map(|t| t.trim().to_string()).filter(|t| !t.is_empty()).collect())
                }
            }
            "approvals.alwaysAllow" => a.always_allow = boolean()?,
            "approvals.answerQuestions" => a.answer_questions = boolean()?,
            "approvals.autoOpen" => a.auto_open = boolean()?,
            "muted" => self.muted = boolean()?,
            _ => return Err(format!("unknown setting {key:?}")),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let c = Config::load(dir.path());
        assert_eq!(c, Config::default());
        assert_eq!(c.timeout_secs(), 90);

        let mut c = c;
        c.set("approvals.timeout", "never").unwrap();
        c.set("approvals.tools", "Bash, Write").unwrap();
        c.set("muted", "on").unwrap();
        c.save(dir.path()).unwrap();
        let back = Config::load(dir.path());
        assert_eq!(back, c);
        assert_eq!(back.get("approvals.timeout").unwrap(), "never");
        assert_eq!(back.get("approvals.tools").unwrap(), "Bash,Write");
        assert_eq!(back.timeout_secs(), MAX_TIMEOUT_SECS);
    }

    #[test]
    fn partial_files_fill_in_defaults() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            Config::file(dir.path()),
            r#"{"approvals":{"mode":"terminal_only"}}"#,
        )
        .unwrap();
        let c = Config::load(dir.path());
        assert_eq!(c.approvals.mode, ApprovalMode::TerminalOnly);
        assert!(c.approvals.always_allow);
        assert_eq!(c.approvals.timeout_secs, Some(90));
    }

    #[test]
    fn validation() {
        let mut c = Config::default();
        assert!(c.set("approvals.timeout", "5").is_err());
        assert!(c.set("approvals.mode", "sometimes").is_err());
        assert!(c.set("muted", "maybe").is_err());
        assert!(c.set("nope", "1").is_err());
        assert_eq!(c, Config::default());
    }

    #[test]
    fn who_answers() {
        let mut c = Config::default();
        assert!(c.widget_handles("Bash", false, false));
        c.set("approvals.tools", "Write").unwrap();
        assert!(!c.widget_handles("Bash", false, true));
        assert!(c.widget_handles("write", false, true));
        assert!(
            c.widget_handles("AskUserQuestion", true, true),
            "tool list doesn't apply to questions"
        );
        c.set("approvals.mode", "widget_when_visible").unwrap();
        assert!(!c.widget_handles("Write", false, false));
        assert!(c.widget_handles("Write", false, true));
        c.set("muted", "true").unwrap();
        assert!(!c.widget_handles("Write", false, true));
    }
}
