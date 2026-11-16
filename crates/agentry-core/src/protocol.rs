//! What the hook sends to the app, what the app answers, and how an answer is
//! turned into the JSON Claude Code expects on the hook's stdout.
//!
//! Output shapes follow https://code.claude.com/docs/en/hooks (checked
//! 2026-11-16). Keep all Claude Code payload knowledge in this crate.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// Body of `POST /event` and `POST /request`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Envelope {
    /// The hook payload exactly as Claude Code wrote it to stdin.
    pub payload: Value,
    pub context: HookContext,
}

/// Facts the hook adds that Claude Code doesn't send. Used later to find the
/// session's terminal window.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct HookContext {
    pub hook_version: String,
    /// Parent of the hook process (Claude Code, or the shell it ran us in).
    pub ppid: Option<u32>,
    /// Controlling terminal of the parent, e.g. `ttys004`.
    pub tty: Option<String>,
    /// Selected terminal/editor environment variables.
    pub env: BTreeMap<String, String>,
}

/// Environment variables that identify the terminal or editor a session runs in.
pub const CONTEXT_ENV_VARS: &[&str] = &[
    "TERM_PROGRAM",
    "TERM_PROGRAM_VERSION",
    "TERM_SESSION_ID",
    "ITERM_SESSION_ID",
    "KITTY_WINDOW_ID",
    "WEZTERM_PANE",
    "GHOSTTY_RESOURCES_DIR",
    "WARP_SESSION_ID",
    "VSCODE_PID",
    "VSCODE_GIT_IPC_HANDLE",
    "CURSOR_TRACE_ID",
    "__CFBundleIdentifier",
    "TMUX",
    "TMUX_PANE",
];

/// The app's answer to `POST /request`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "decision", rename_all = "snake_case")]
pub enum Decision {
    Allow {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        updated_input: Option<Value>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        updated_permissions: Option<Value>,
    },
    Deny {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        message: Option<String>,
        #[serde(default)]
        interrupt: bool,
    },
    /// Let Claude Code show its normal prompt in the terminal.
    Terminal,
}

/// Name of the hook event in a payload, e.g. `"PreToolUse"`.
pub fn event_name(payload: &Value) -> Option<&str> {
    payload.get("hook_event_name")?.as_str()
}

pub fn tool_name(payload: &Value) -> Option<&str> {
    payload.get("tool_name")?.as_str()
}

/// Events the hook must wait on: permission prompts, and questions Claude asks
/// via `AskUserQuestion`. Everything else is fire-and-forget.
pub fn is_blocking(payload: &Value) -> bool {
    match event_name(payload) {
        Some("PermissionRequest") => true,
        Some("PreToolUse") => tool_name(payload) == Some("AskUserQuestion"),
        _ => false,
    }
}

/// Turn the app's decision into the JSON printed on the hook's stdout.
/// `None` means print nothing, so Claude Code falls back to its own prompt.
pub fn hook_output(event: &str, decision: &Decision) -> Option<Value> {
    match (event, decision) {
        (_, Decision::Terminal) => None,

        (
            "PermissionRequest",
            Decision::Allow {
                updated_input,
                updated_permissions,
            },
        ) => {
            let mut d = json!({ "behavior": "allow" });
            if let Some(i) = updated_input {
                d["updatedInput"] = i.clone();
            }
            if let Some(p) = updated_permissions {
                d["updatedPermissions"] = p.clone();
            }
            Some(
                json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": d } }),
            )
        }
        ("PermissionRequest", Decision::Deny { message, interrupt }) => {
            let mut d = json!({ "behavior": "deny" });
            if let Some(m) = message {
                d["message"] = json!(m);
            }
            if *interrupt {
                d["interrupt"] = json!(true);
            }
            Some(
                json!({ "hookSpecificOutput": { "hookEventName": "PermissionRequest", "decision": d } }),
            )
        }

        ("PreToolUse", Decision::Allow { updated_input, .. }) => {
            let mut o = json!({ "hookEventName": "PreToolUse", "permissionDecision": "allow" });
            if let Some(i) = updated_input {
                o["updatedInput"] = i.clone();
            }
            Some(json!({ "hookSpecificOutput": o }))
        }
        ("PreToolUse", Decision::Deny { message, .. }) => Some(json!({
            "hookSpecificOutput": {
                "hookEventName": "PreToolUse",
                "permissionDecision": "deny",
                "permissionDecisionReason": message.clone().unwrap_or_else(|| "Denied from Agentry".into()),
            }
        })),

        _ => None,
    }
}

/// A short, safe description of an event for logs and the debug view. Never
/// includes tool input or output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSummary {
    pub event: String,
    pub session_id: Option<String>,
    pub cwd: Option<String>,
    pub tool_name: Option<String>,
    pub agent_id: Option<String>,
    pub agent_type: Option<String>,
    pub notification_type: Option<String>,
    pub blocking: bool,
    pub received_at_ms: u64,
}

impl EventSummary {
    pub fn from_payload(payload: &Value, received_at_ms: u64) -> Self {
        let s = |k: &str| payload.get(k).and_then(Value::as_str).map(str::to_owned);
        EventSummary {
            event: s("hook_event_name").unwrap_or_else(|| "Unknown".into()),
            session_id: s("session_id"),
            cwd: s("cwd"),
            tool_name: s("tool_name"),
            agent_id: s("agent_id"),
            agent_type: s("agent_type"),
            notification_type: s("notification_type"),
            blocking: is_blocking(payload),
            received_at_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocking_only_for_permission_and_questions() {
        assert!(is_blocking(
            &json!({"hook_event_name": "PermissionRequest", "tool_name": "Bash"})
        ));
        assert!(is_blocking(
            &json!({"hook_event_name": "PreToolUse", "tool_name": "AskUserQuestion"})
        ));
        assert!(!is_blocking(
            &json!({"hook_event_name": "PreToolUse", "tool_name": "Bash"})
        ));
        assert!(!is_blocking(
            &json!({"hook_event_name": "PostToolUse", "tool_name": "AskUserQuestion"})
        ));
        assert!(!is_blocking(&json!({"hook_event_name": "Stop"})));
        assert!(!is_blocking(&json!({})));
    }

    #[test]
    fn terminal_prints_nothing() {
        assert_eq!(hook_output("PermissionRequest", &Decision::Terminal), None);
        assert_eq!(hook_output("PreToolUse", &Decision::Terminal), None);
    }

    #[test]
    fn permission_allow_and_deny_shapes() {
        let allow = hook_output(
            "PermissionRequest",
            &Decision::Allow {
                updated_input: None,
                updated_permissions: Some(json!([{"type": "addRules"}])),
            },
        )
        .unwrap();
        assert_eq!(
            allow,
            json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest",
                "decision": {"behavior": "allow", "updatedPermissions": [{"type": "addRules"}]}}})
        );

        let deny = hook_output(
            "PermissionRequest",
            &Decision::Deny {
                message: Some("use pnpm".into()),
                interrupt: true,
            },
        )
        .unwrap();
        assert_eq!(
            deny,
            json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest",
                "decision": {"behavior": "deny", "message": "use pnpm", "interrupt": true}}})
        );
    }

    #[test]
    fn question_answer_shape() {
        let input = json!({"questions": [{"question": "Which?", "options": [{"label": "A"}]}], "answers": {"Which?": "A"}});
        let out = hook_output(
            "PreToolUse",
            &Decision::Allow {
                updated_input: Some(input.clone()),
                updated_permissions: None,
            },
        )
        .unwrap();
        assert_eq!(
            out,
            json!({"hookSpecificOutput": {"hookEventName": "PreToolUse", "permissionDecision": "allow", "updatedInput": input}})
        );
    }

    #[test]
    fn decision_wire_format() {
        let d: Decision = serde_json::from_str(r#"{"decision":"terminal"}"#).unwrap();
        assert_eq!(d, Decision::Terminal);
        let d: Decision = serde_json::from_str(r#"{"decision":"deny","message":"no"}"#).unwrap();
        assert_eq!(
            d,
            Decision::Deny {
                message: Some("no".into()),
                interrupt: false
            }
        );
        assert_eq!(
            serde_json::to_value(Decision::Terminal).unwrap(),
            json!({"decision": "terminal"})
        );
    }

    #[test]
    fn summary_omits_tool_input() {
        let p = json!({"hook_event_name": "PostToolUse", "session_id": "s1", "tool_name": "Write",
                       "tool_input": {"content": "secret"}, "agent_type": "Explore"});
        let s = serde_json::to_string(&EventSummary::from_payload(&p, 1)).unwrap();
        assert!(!s.contains("secret"));
        assert!(s.contains("Explore"));
    }
}
