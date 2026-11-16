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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    Mono,
    Crew,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Appearance {
    pub theme: Theme,
    /// "#rrggbb", or `None` for the theme's own accent.
    pub accent: Option<String>,
    /// Open the panel when the pointer rests on the pill.
    pub hover_to_open: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Sounds {
    pub enabled: bool,
    /// 0.0–1.0
    pub volume: f32,
}

impl Default for Sounds {
    fn default() -> Self {
        Sounds {
            enabled: true,
            volume: 0.6,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub approvals: Approvals,
    pub appearance: Appearance,
    pub sounds: Sounds,
    /// Quiet mode: no sounds, and every prompt goes to the terminal.
    pub muted: bool,
}

/// `#rgb` or `#rrggbb` → normalised `#rrggbb`.
pub fn parse_hex_color(v: &str) -> Option<String> {
    let h = v.trim().strip_prefix('#')?;
    let full = match h.len() {
        3 => h.chars().flat_map(|c| [c, c]).collect::<String>(),
        6 => h.to_string(),
        _ => return None,
    };
    full.chars()
        .all(|c| c.is_ascii_hexdigit())
        .then(|| format!("#{}", full.to_ascii_lowercase()))
}

impl Config {
    pub fn file(home: &Path) -> PathBuf {
        home.join("config.json")
    }

    /// Load settings; a missing or unreadable file gives the defaults.
    pub fn load(home: &Path) -> Config {
        std::fs::read_to_string(Self::file(home))
            .ok()
            .and_then(|t| serde_json::from_str::<Config>(&t).ok())
            .unwrap_or_default()
            .sanitized()
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
            self.approvals.tools.as_ref().is_none_or(|t| {
                t.iter().any(|x| {
                    x.eq_ignore_ascii_case(tool)
                        || (x.eq_ignore_ascii_case("mcp") && tool.starts_with("mcp__"))
                })
            })
        };
        mode_ok && kind_ok
    }

    /// Clamp values a hand-edited file might have out of range.
    pub fn sanitized(mut self) -> Config {
        self.sounds.volume = if self.sounds.volume.is_finite() {
            self.sounds.volume.clamp(0.0, 1.0)
        } else {
            0.6
        };
        self.appearance.accent = self.appearance.accent.as_deref().and_then(parse_hex_color);
        self
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
        ("appearance.theme", "mono | crew"),
        (
            "appearance.accent",
            "a colour such as #ff7a45, or \"default\"",
        ),
        (
            "appearance.hoverToOpen",
            "true | false — open the panel on hover",
        ),
        ("sounds.enabled", "true | false"),
        ("sounds.volume", "0–100"),
        (
            "muted",
            "true | false — quiet mode: no sounds, everything goes to the terminal",
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
            "appearance.theme" => match self.appearance.theme {
                Theme::Mono => "mono".into(),
                Theme::Crew => "crew".into(),
            },
            "appearance.accent" => self
                .appearance
                .accent
                .clone()
                .unwrap_or_else(|| "default".into()),
            "appearance.hoverToOpen" => self.appearance.hover_to_open.to_string(),
            "sounds.enabled" => self.sounds.enabled.to_string(),
            "sounds.volume" => ((self.sounds.volume * 100.0).round() as u32).to_string(),
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
            "appearance.theme" => {
                self.appearance.theme = match v.to_ascii_lowercase().as_str() {
                    "mono" => Theme::Mono,
                    "crew" => Theme::Crew,
                    _ => return Err("appearance.theme must be mono or crew".into()),
                }
            }
            "appearance.accent" => {
                self.appearance.accent = if v.eq_ignore_ascii_case("default") || v.is_empty() {
                    None
                } else {
                    Some(parse_hex_color(v).ok_or("appearance.accent must be a colour like #ff7a45, or \"default\"")?)
                }
            }
            "appearance.hoverToOpen" => self.appearance.hover_to_open = boolean()?,
            "sounds.enabled" => self.sounds.enabled = boolean()?,
            "sounds.volume" => {
                let n: f32 = v.parse().map_err(|_| "sounds.volume must be 0–100".to_string())?;
                if !(0.0..=100.0).contains(&n) {
                    return Err("sounds.volume must be 0–100".into());
                }
                self.sounds.volume = n / 100.0;
            }
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
    fn appearance_and_sounds() {
        let mut c = Config::default();
        assert_eq!((c.appearance.theme, c.sounds.enabled), (Theme::Mono, true));
        c.set("appearance.theme", "Crew").unwrap();
        c.set("appearance.accent", "#F0a").unwrap();
        c.set("sounds.volume", "35").unwrap();
        assert_eq!(c.get("appearance.theme").unwrap(), "crew");
        assert_eq!(c.get("appearance.accent").unwrap(), "#ff00aa");
        assert_eq!(c.get("sounds.volume").unwrap(), "35");
        assert!(c.set("appearance.accent", "red").is_err());
        assert!(c.set("sounds.volume", "300").is_err());
        c.set("appearance.accent", "default").unwrap();
        assert_eq!(c.appearance.accent, None);

        // Hand-edited nonsense is cleaned up on load.
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            Config::file(dir.path()),
            r#"{"appearance":{"accent":"javascript:alert(1)"},"sounds":{"volume":7}}"#,
        )
        .unwrap();
        let c = Config::load(dir.path());
        assert_eq!((c.appearance.accent, c.sounds.volume), (None, 1.0));
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
        c.set("approvals.tools", "Write,mcp").unwrap();
        assert!(c.widget_handles("mcp__github__create_issue", false, true));
        assert!(!c.widget_handles("Bash", false, true));
        c.set("approvals.tools", "Write").unwrap();
        c.set("approvals.mode", "widget_when_visible").unwrap();
        assert!(!c.widget_handles("Write", false, false));
        assert!(c.widget_handles("Write", false, true));
        c.set("muted", "true").unwrap();
        assert!(!c.widget_handles("Write", false, true));
    }
}
