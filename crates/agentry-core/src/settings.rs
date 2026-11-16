//! Adding Agentry's hooks to a Claude Code `settings.json`, and taking them
//! out again, without touching anything else in the file.
//!
//! Agentry's entries are recognised by their command: a hook whose command's
//! program is a file named `agentry-hook`. That also finds stale entries left
//! by an app that has since moved.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::HOOK_BIN_NAME;

/// Seconds Claude Code waits on a blocking hook before giving up. The app
/// decides the real wait (the user's fallback timeout); this is a safety net.
pub const BLOCKING_TIMEOUT_SECS: u64 = 3600;
/// Timeout for fire-and-forget hooks. They run with `async: true`, so Claude
/// doesn't wait on them at all.
pub const ASYNC_TIMEOUT_SECS: u64 = 10;
/// Backups kept per settings file.
pub const BACKUPS_KEPT: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Runs in the background; Claude never waits.
    Async,
    /// Claude waits for the hook's answer (approvals, questions).
    Blocking,
}

#[derive(Debug, Clone, Copy)]
pub struct HookSpec {
    pub event: &'static str,
    pub matcher: Option<&'static str>,
    pub mode: Mode,
}

const fn spec(event: &'static str, matcher: Option<&'static str>, mode: Mode) -> HookSpec {
    HookSpec {
        event,
        matcher,
        mode,
    }
}

/// Every hook Agentry registers.
pub const HOOKS: &[HookSpec] = &[
    spec("SessionStart", None, Mode::Async),
    spec("UserPromptSubmit", None, Mode::Async),
    spec("PreToolUse", Some("AskUserQuestion"), Mode::Blocking),
    spec("PermissionRequest", Some("*"), Mode::Blocking),
    spec("PostToolUse", Some("*"), Mode::Async),
    spec("PostToolUseFailure", Some("*"), Mode::Async),
    spec("TaskCreated", None, Mode::Async),
    spec("TaskCompleted", None, Mode::Async),
    spec("SubagentStart", None, Mode::Async),
    spec("SubagentStop", None, Mode::Async),
    spec("Notification", None, Mode::Async),
    spec("Stop", None, Mode::Async),
    spec("StopFailure", None, Mode::Async),
    spec("SessionEnd", None, Mode::Async),
];

#[derive(Debug)]
pub enum SettingsError {
    Io(io::Error),
    /// The file exists but isn't JSON we can safely edit.
    Invalid(String),
}

impl std::fmt::Display for SettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SettingsError::Io(e) => write!(f, "{e}"),
            SettingsError::Invalid(m) => write!(f, "{m}"),
        }
    }
}

impl std::error::Error for SettingsError {}

impl From<io::Error> for SettingsError {
    fn from(e: io::Error) -> Self {
        SettingsError::Io(e)
    }
}

/// Quote a path for the shell Claude Code runs hook commands in.
pub fn shell_quote(path: &Path) -> String {
    let s = path.to_string_lossy();
    let safe = s
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "/._-+:@%,".contains(c));
    if safe && !s.is_empty() {
        s.into_owned()
    } else {
        format!("'{}'", s.replace('\'', r"'\''"))
    }
}

/// The program a hook command runs, with simple shell quoting removed.
fn command_program(command: &str) -> String {
    let c = command.trim_start();
    if let Some(rest) = c.strip_prefix('\'') {
        // 'a'\''b' style: join quoted segments until an unquoted space.
        let mut out = String::new();
        let mut chars = rest.chars().peekable();
        let mut quoted = true;
        while let Some(ch) = chars.next() {
            match (quoted, ch) {
                (true, '\'') => quoted = false,
                (false, '\\') => {
                    if let Some(n) = chars.next() {
                        out.push(n)
                    }
                }
                (false, '\'') => quoted = true,
                (false, c) if c.is_whitespace() => break,
                (_, c) => out.push(c),
            }
        }
        out
    } else if let Some(rest) = c.strip_prefix('"') {
        rest.split('"').next().unwrap_or("").to_string()
    } else {
        c.split_whitespace().next().unwrap_or("").to_string()
    }
}

/// True if this hook entry belongs to Agentry.
pub fn is_agentry_hook(hook: &Value) -> bool {
    let Some(cmd) = hook.get("command").and_then(Value::as_str) else {
        return false;
    };
    let program = command_program(cmd);
    Path::new(&program).file_name().and_then(|n| n.to_str()) == Some(HOOK_BIN_NAME)
}

/// Remove every Agentry hook entry. Returns how many were removed. Groups and
/// event keys that become empty because of the removal are dropped; ones that
/// were already empty are left alone.
pub fn remove_agentry_hooks(settings: &mut Value) -> usize {
    let Some(obj) = settings.as_object_mut() else {
        return 0;
    };
    let Some(hooks) = obj.get_mut("hooks").and_then(Value::as_object_mut) else {
        return 0;
    };

    let mut removed = 0;
    let mut empty_events = Vec::new();
    for (event, groups) in hooks.iter_mut() {
        let Some(groups) = groups.as_array_mut() else {
            continue;
        };
        let mut touched = false;
        groups.retain_mut(|group| {
            let Some(list) = group.get_mut("hooks").and_then(Value::as_array_mut) else {
                return true;
            };
            let before = list.len();
            list.retain(|h| !is_agentry_hook(h));
            let n = before - list.len();
            removed += n;
            touched |= n > 0;
            !(n > 0 && list.is_empty())
        });
        if touched && groups.is_empty() {
            empty_events.push(event.clone());
        }
    }
    for e in empty_events {
        hooks.remove(&e);
    }
    if removed > 0 && hooks.is_empty() {
        obj.remove("hooks");
    }
    removed
}

/// Add Agentry's hooks (replacing any earlier Agentry entries) for `hook_path`.
pub fn add_agentry_hooks(
    settings: &mut Value,
    hook_path: &Path,
    specs: &[HookSpec],
) -> Result<(), SettingsError> {
    if settings.is_null() {
        *settings = json!({});
    }
    remove_agentry_hooks(settings);
    let obj = settings
        .as_object_mut()
        .ok_or_else(|| SettingsError::Invalid("settings file is not a JSON object".into()))?;
    let hooks = obj
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()));
    let hooks = hooks
        .as_object_mut()
        .ok_or_else(|| SettingsError::Invalid("\"hooks\" in settings is not an object".into()))?;

    let command = shell_quote(hook_path);
    for s in specs {
        let mut hook = json!({ "type": "command", "command": command });
        match s.mode {
            Mode::Async => {
                hook["async"] = json!(true);
                hook["timeout"] = json!(ASYNC_TIMEOUT_SECS);
            }
            Mode::Blocking => hook["timeout"] = json!(BLOCKING_TIMEOUT_SECS),
        }
        let mut group = Map::new();
        if let Some(m) = s.matcher {
            group.insert("matcher".into(), json!(m));
        }
        group.insert("hooks".into(), json!([hook]));

        let list = hooks.entry(s.event).or_insert_with(|| json!([]));
        let list = list.as_array_mut().ok_or_else(|| {
            SettingsError::Invalid(format!("\"hooks.{}\" in settings is not a list", s.event))
        })?;
        list.push(Value::Object(group));
    }
    Ok(())
}

/// Agentry hook entries currently in `settings`: (event, command) pairs.
pub fn list_agentry_hooks(settings: &Value) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let Some(hooks) = settings.get("hooks").and_then(Value::as_object) else {
        return out;
    };
    for (event, groups) in hooks {
        for group in groups.as_array().into_iter().flatten() {
            for h in group
                .get("hooks")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if is_agentry_hook(h) {
                    let cmd = h
                        .get("command")
                        .and_then(Value::as_str)
                        .unwrap_or("")
                        .to_string();
                    out.push((event.clone(), cmd));
                }
            }
        }
    }
    out
}

/// The program path of an Agentry hook command, unquoted.
pub fn hook_command_path(command: &str) -> PathBuf {
    PathBuf::from(command_program(command))
}

/// Read a settings file. A missing or empty file is an empty object.
pub fn read_settings(path: &Path) -> Result<Value, SettingsError> {
    match fs::read_to_string(path) {
        Ok(text) if text.trim().is_empty() => Ok(json!({})),
        Ok(text) => serde_json::from_str(&text).map_err(|e| {
            SettingsError::Invalid(format!(
                "{} is not valid JSON ({e}); fix it or move it aside first",
                path.display()
            ))
        }),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(json!({})),
        Err(e) => Err(e.into()),
    }
}

/// Copy `path` to `<path>.agentry-backup-<unix-seconds>` and keep only the
/// newest [`BACKUPS_KEPT`]. Returns the backup path, or `None` if there was
/// nothing to back up.
pub fn backup(path: &Path) -> io::Result<Option<PathBuf>> {
    if !path.exists() {
        return Ok(None);
    }
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("settings.json");
    let prefix = format!("{name}.agentry-backup-");
    let dest = path.with_file_name(format!("{prefix}{ts}"));
    fs::copy(path, &dest)?;

    if let Some(dir) = path.parent() {
        let mut backups: Vec<PathBuf> = fs::read_dir(dir)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(&prefix))
            })
            .collect();
        backups.sort();
        let excess = backups.len().saturating_sub(BACKUPS_KEPT);
        for old in &backups[..excess] {
            let _ = fs::remove_file(old);
        }
    }
    Ok(Some(dest))
}

/// Write settings as pretty JSON (2-space indent, trailing newline),
/// atomically, keeping the file's existing permissions.
pub fn write_settings(path: &Path, settings: &Value) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let mut text = serde_json::to_string_pretty(settings).map_err(io::Error::other)?;
    text.push('\n');
    let tmp = path.with_extension(format!("agentry-tmp-{}", std::process::id()));
    fs::write(&tmp, text)?;
    if let Ok(meta) = fs::metadata(path) {
        let _ = fs::set_permissions(&tmp, meta.permissions());
    }
    fs::rename(&tmp, path)
}

/// What an install or uninstall did.
#[derive(Debug)]
pub struct Change {
    pub file: PathBuf,
    pub backup: Option<PathBuf>,
    pub added: usize,
    pub removed: usize,
}

/// Install Agentry's hooks into `file` (backing it up first).
pub fn install(file: &Path, hook_path: &Path, specs: &[HookSpec]) -> Result<Change, SettingsError> {
    let mut settings = read_settings(file)?;
    let removed = list_agentry_hooks(&settings).len();
    add_agentry_hooks(&mut settings, hook_path, specs)?;
    let backup = backup(file)?;
    write_settings(file, &settings)?;
    Ok(Change {
        file: file.to_path_buf(),
        backup,
        added: specs.len(),
        removed,
    })
}

/// Remove Agentry's hooks from `file`. Leaves the file untouched (and makes
/// no backup) when there's nothing to remove.
pub fn uninstall(file: &Path) -> Result<Change, SettingsError> {
    let mut settings = read_settings(file)?;
    let removed = remove_agentry_hooks(&mut settings);
    if removed == 0 {
        return Ok(Change {
            file: file.to_path_buf(),
            backup: None,
            added: 0,
            removed: 0,
        });
    }
    let backup = backup(file)?;
    write_settings(file, &settings)?;
    Ok(Change {
        file: file.to_path_buf(),
        backup,
        added: 0,
        removed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user_settings() -> Value {
        json!({
            "model": "opus",
            "permissions": {"allow": ["Bash(ls:*)"]},
            "hooks": {
                "PreToolUse": [
                    {"matcher": "Bash", "hooks": [{"type": "command", "command": "~/bin/guard.sh"}]}
                ],
                "Stop": [{"hooks": [{"type": "command", "command": "say done"}]}]
            },
            "statusLine": {"type": "command", "command": "~/status.sh"}
        })
    }

    const HOOK: &str = "/Applications/Agentry.app/Contents/MacOS/agentry-hook";

    #[test]
    fn install_then_uninstall_restores_original() {
        for original in [json!({}), user_settings(), json!({"hooks": {}})] {
            let mut s = original.clone();
            add_agentry_hooks(&mut s, Path::new(HOOK), HOOKS).unwrap();
            assert_eq!(list_agentry_hooks(&s).len(), HOOKS.len());
            assert_eq!(remove_agentry_hooks(&mut s), HOOKS.len());
            if original == json!({"hooks": {}}) {
                // We removed only what we added; an emptied "hooks" goes away.
                assert_eq!(s, json!({}));
            } else {
                assert_eq!(s, original);
            }
        }
    }

    #[test]
    fn install_keeps_user_hooks_and_is_idempotent() {
        let mut s = user_settings();
        add_agentry_hooks(&mut s, Path::new(HOOK), HOOKS).unwrap();
        add_agentry_hooks(&mut s, Path::new(HOOK), HOOKS).unwrap();
        assert_eq!(list_agentry_hooks(&s).len(), HOOKS.len());
        let pre = s["hooks"]["PreToolUse"].as_array().unwrap();
        assert_eq!(pre[0]["hooks"][0]["command"], "~/bin/guard.sh");
        assert_eq!(pre[1]["matcher"], "AskUserQuestion");
        assert_eq!(s["hooks"]["Stop"][0]["hooks"][0]["command"], "say done");
        assert_eq!(s["model"], "opus");
    }

    #[test]
    fn entry_shapes() {
        let mut s = json!({});
        add_agentry_hooks(&mut s, Path::new(HOOK), HOOKS).unwrap();
        assert_eq!(
            s["hooks"]["PermissionRequest"][0],
            json!({"matcher": "*", "hooks": [{"type": "command", "command": HOOK, "timeout": 3600}]})
        );
        assert_eq!(
            s["hooks"]["SessionStart"][0],
            json!({"hooks": [{"type": "command", "command": HOOK, "async": true, "timeout": 10}]})
        );
    }

    #[test]
    fn reinstall_from_new_location_replaces_stale_entries() {
        let mut s = json!({});
        add_agentry_hooks(&mut s, Path::new("/old/place/agentry-hook"), HOOKS).unwrap();
        add_agentry_hooks(
            &mut s,
            Path::new("/Applications/My Apps/Agentry.app/agentry-hook"),
            HOOKS,
        )
        .unwrap();
        let hooks = list_agentry_hooks(&s);
        assert_eq!(hooks.len(), HOOKS.len());
        assert!(hooks
            .iter()
            .all(|(_, c)| c == "'/Applications/My Apps/Agentry.app/agentry-hook'"));
    }

    #[test]
    fn mixed_group_keeps_user_hook() {
        let mut s = json!({"hooks": {"Stop": [{"hooks": [
            {"type": "command", "command": "say done"},
            {"type": "command", "command": "/x/agentry-hook"}
        ]}]}});
        assert_eq!(remove_agentry_hooks(&mut s), 1);
        assert_eq!(
            s,
            json!({"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "say done"}]}]}})
        );
    }

    #[test]
    fn recognises_agentry_commands_only() {
        let h = |c: &str| json!({"type": "command", "command": c});
        assert!(is_agentry_hook(&h("/a/b/agentry-hook")));
        assert!(is_agentry_hook(&h("'/a b/it'\\''s/agentry-hook'")));
        assert!(is_agentry_hook(&h("\"/a b/agentry-hook\" --flag")));
        assert!(is_agentry_hook(&h("agentry-hook")));
        assert!(!is_agentry_hook(&h("/a/b/agentry-hook-old")));
        assert!(!is_agentry_hook(&h("echo agentry-hook")));
        assert!(!is_agentry_hook(
            &json!({"type": "prompt", "prompt": "agentry-hook"})
        ));
    }

    #[test]
    fn quoting_round_trips() {
        for p in [
            "/usr/local/bin/agentry-hook",
            "/Users/a b/it's/agentry-hook",
        ] {
            let q = shell_quote(Path::new(p));
            assert_eq!(hook_command_path(&q), PathBuf::from(p));
        }
    }

    #[test]
    fn files_backups_and_bad_json() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("settings.json");

        // Missing file: install creates it, no backup.
        let c = install(&file, Path::new(HOOK), HOOKS).unwrap();
        assert!(c.backup.is_none());
        let c = uninstall(&file).unwrap();
        assert_eq!(c.removed, HOOKS.len());
        assert!(c.backup.is_some());
        assert_eq!(read_settings(&file).unwrap(), json!({}));

        // Nothing to remove: no write, no backup.
        let c = uninstall(&file).unwrap();
        assert_eq!(c.removed, 0);
        assert!(c.backup.is_none());

        // Backups are capped.
        for _ in 0..8 {
            install(&file, Path::new(HOOK), HOOKS).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        let n = fs::read_dir(dir.path())
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .contains("agentry-backup")
            })
            .count();
        assert_eq!(n, BACKUPS_KEPT);

        // Invalid JSON is refused and left untouched.
        fs::write(&file, "{ not json").unwrap();
        assert!(matches!(
            install(&file, Path::new(HOOK), HOOKS),
            Err(SettingsError::Invalid(_))
        ));
        assert_eq!(fs::read_to_string(&file).unwrap(), "{ not json");
    }

    #[test]
    fn preserves_key_order() {
        let text = r#"{"zeta": 1, "alpha": 2, "hooks": {"PreCompact": []}, "mid": 3}"#;
        let original: Value = serde_json::from_str(text).unwrap();
        let mut s = original.clone();
        add_agentry_hooks(&mut s, Path::new(HOOK), HOOKS).unwrap();
        remove_agentry_hooks(&mut s);
        let keys: Vec<_> = s.as_object().unwrap().keys().cloned().collect();
        assert_eq!(keys, ["zeta", "alpha", "hooks", "mid"]);
        assert_eq!(s, original);
    }
}
