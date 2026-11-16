//! Jump to the window a session runs in.
//!
//! `plan` decides what to do from the terminal details the hook recorded
//! (pure, testable anywhere); `run` carries it out. macOS uses AppleScript and
//! `open`; tmux works on any platform.

use std::collections::BTreeMap;
use std::process::Command;

/// What the hook recorded about where a session runs.
pub struct Where<'a> {
    pub env: &'a BTreeMap<String, String>,
    pub tty: Option<&'a str>,
    pub cwd: &'a str,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    AppleScript(String),
    /// Program and arguments.
    Run(Vec<String>),
    /// `open <url>`
    OpenUrl(String),
    /// `open -b <bundle id>`
    ActivateApp(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Precision {
    /// The exact tab or pane.
    Exact,
    /// The right project window.
    Window,
    /// Just brings the app forward.
    App,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// Human name, e.g. "iTerm2".
    pub app: &'static str,
    pub precision: Precision,
    pub actions: Vec<Action>,
}

const ITERM: &str = "com.googlecode.iterm2";
const TERMINAL: &str = "com.apple.Terminal";

/// Editors whose integrated terminal or Claude Code extension sets
/// `TERM_PROGRAM=vscode`, keyed by bundle id → (name, URL scheme).
const EDITORS: &[(&str, &str, &str)] = &[
    ("com.microsoft.VSCode", "VS Code", "vscode"),
    (
        "com.microsoft.VSCodeInsiders",
        "VS Code Insiders",
        "vscode-insiders",
    ),
    ("com.todesktop.230313mzl4w4u92", "Cursor", "cursor"),
    ("com.exafunction.windsurf", "Windsurf", "windsurf"),
    ("com.vscodium", "VSCodium", "vscodium"),
];

fn safe(value: &str, extra: &str) -> Option<String> {
    (!value.is_empty()
        && value.len() <= 200
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || extra.contains(c)))
    .then(|| value.to_owned())
}

fn bundle_id(env: &BTreeMap<String, String>) -> Option<String> {
    env.get("__CFBundleIdentifier").and_then(|b| safe(b, ".-_"))
}

/// Percent-encode a path for a `scheme://file/<path>` URL.
fn url_path(path: &str) -> String {
    path.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

pub fn plan(w: &Where) -> Option<Plan> {
    let env = w.env;
    let get = |k: &str| env.get(k).map(String::as_str);
    let term = get("TERM_PROGRAM").unwrap_or("");

    // tmux: select the pane, then bring the outer terminal forward.
    if let (Some(tmux), Some(pane)) = (get("TMUX"), get("TMUX_PANE").and_then(|p| safe(p, "%"))) {
        let socket = tmux.split(',').next().filter(|s| s.starts_with('/'))?;
        let socket = safe(socket, "/._-")?;
        let tmux = |cmd: &str| {
            Action::Run(vec![
                "tmux".into(),
                "-S".into(),
                socket.clone(),
                cmd.into(),
                "-t".into(),
                pane.clone(),
            ])
        };
        let mut actions = vec![tmux("select-window"), tmux("select-pane")];
        if let Some(b) = bundle_id(env) {
            actions.push(Action::ActivateApp(b));
        }
        return Some(Plan {
            app: "tmux",
            precision: Precision::Exact,
            actions,
        });
    }

    if term == "iTerm.app" {
        let uuid = get("ITERM_SESSION_ID")
            .and_then(|s| s.split(':').nth(1))
            .and_then(|u| safe(u, "-"));
        let actions = match uuid {
            Some(u) => vec![Action::AppleScript(format!(
                r#"tell application id "{ITERM}"
  repeat with w in windows
    repeat with t in tabs of w
      repeat with s in sessions of t
        if unique ID of s is "{u}" then
          select w
          tell t to select
          tell s to select
          activate
          return "ok"
        end if
      end repeat
    end repeat
  end repeat
  activate
end tell
return "app""#
            ))],
            None => vec![Action::ActivateApp(ITERM.into())],
        };
        return Some(Plan {
            app: "iTerm2",
            precision: Precision::Exact,
            actions,
        });
    }

    if term == "Apple_Terminal" {
        let tty = w.tty.and_then(|t| safe(t, "/")).map(|t| {
            if t.starts_with("/dev/") {
                t
            } else {
                format!("/dev/{t}")
            }
        });
        let actions = match tty {
            Some(t) => vec![Action::AppleScript(format!(
                r#"tell application id "{TERMINAL}"
  repeat with w in windows
    repeat with t in tabs of w
      if tty of t is "{t}" then
        set selected of t to true
        set index of w to 1
        activate
        return "ok"
      end if
    end repeat
  end repeat
  activate
end tell
return "app""#
            ))],
            None => vec![Action::ActivateApp(TERMINAL.into())],
        };
        return Some(Plan {
            app: "Terminal",
            precision: Precision::Exact,
            actions,
        });
    }

    if let Some(id) = get("KITTY_WINDOW_ID").and_then(|i| safe(i, "")) {
        return Some(Plan {
            app: "kitty",
            precision: Precision::Exact,
            actions: vec![
                Action::Run(vec![
                    "kitty".into(),
                    "@".into(),
                    "focus-window".into(),
                    "--match".into(),
                    format!("id:{id}"),
                ]),
                Action::ActivateApp("net.kovidgoyal.kitty".into()),
            ],
        });
    }

    if let Some(pane) = get("WEZTERM_PANE").and_then(|p| safe(p, "")) {
        return Some(Plan {
            app: "WezTerm",
            precision: Precision::Exact,
            actions: vec![
                Action::Run(vec![
                    "wezterm".into(),
                    "cli".into(),
                    "activate-pane".into(),
                    "--pane-id".into(),
                    pane,
                ]),
                Action::ActivateApp("com.github.wez.wezterm".into()),
            ],
        });
    }

    if term == "ghostty" || get("GHOSTTY_RESOURCES_DIR").is_some() {
        return Some(Plan {
            app: "Ghostty",
            precision: Precision::App,
            actions: vec![Action::ActivateApp("com.mitchellh.ghostty".into())],
        });
    }
    if term == "WarpTerminal" || get("WARP_SESSION_ID").is_some() {
        return Some(Plan {
            app: "Warp",
            precision: Precision::App,
            actions: vec![Action::ActivateApp("dev.warp.Warp-Stable".into())],
        });
    }

    // VS Code and its forks: the integrated terminal sets TERM_PROGRAM=vscode;
    // the Claude Code extension runs inside the editor's own process.
    let bundle = bundle_id(env);
    let editor = bundle
        .as_deref()
        .and_then(|b| EDITORS.iter().find(|(id, _, _)| *id == b))
        .or_else(|| (get("CURSOR_TRACE_ID").is_some()).then_some(&EDITORS[2]))
        .or_else(|| (term == "vscode" || get("VSCODE_PID").is_some()).then_some(&EDITORS[0]));
    if let Some((_, name, scheme)) = editor {
        if w.cwd.starts_with('/') {
            return Some(Plan {
                app: name,
                precision: Precision::Window,
                actions: vec![Action::OpenUrl(format!(
                    "{scheme}://file{}",
                    url_path(w.cwd)
                ))],
            });
        }
    }

    bundle.map(|b| Plan {
        app: "the app",
        precision: Precision::App,
        actions: vec![Action::ActivateApp(b)],
    })
}

/// Carry out a plan. Returns a short description for the UI.
pub fn run(plan: &Plan) -> Result<String, String> {
    let mut exact = plan.precision == Precision::Exact;
    for action in &plan.actions {
        match action {
            Action::Run(argv) => {
                let ok = Command::new(&argv[0])
                    .args(&argv[1..])
                    .output()
                    .is_ok_and(|o| o.status.success());
                if !ok {
                    exact = false; // e.g. kitty remote control is off; still bring the app forward
                }
            }
            Action::AppleScript(script) => {
                let out = mac_only(Command::new("osascript").arg("-e").arg(script))?;
                if String::from_utf8_lossy(&out.stdout).trim() != "ok" {
                    exact = false;
                }
            }
            Action::OpenUrl(url) => {
                mac_only(Command::new("open").arg(url))?;
            }
            Action::ActivateApp(bundle) => {
                mac_only(Command::new("open").arg("-b").arg(bundle))?;
            }
        }
    }
    Ok(match (plan.precision, exact) {
        (Precision::Exact, true) => format!("Switched to the session in {}", plan.app),
        (Precision::Window, _) => format!("Opened the project in {}", plan.app),
        _ => format!("Brought {} forward", plan.app),
    })
}

fn mac_only(cmd: &mut Command) -> Result<std::process::Output, String> {
    if !cfg!(target_os = "macos") {
        return Err("Jumping to this app is only supported on macOS for now".into());
    }
    let out = cmd.output().map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(out)
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn plan_for(pairs: &[(&str, &str)], tty: Option<&str>, cwd: &str) -> Option<Plan> {
        plan(&Where {
            env: &env(pairs),
            tty,
            cwd,
        })
    }

    #[test]
    fn iterm_selects_the_session() {
        let p = plan_for(
            &[
                ("TERM_PROGRAM", "iTerm.app"),
                ("ITERM_SESSION_ID", "w0t1p0:4C2A-11EF-9B1D"),
            ],
            None,
            "/w",
        )
        .unwrap();
        assert_eq!((p.app, p.precision), ("iTerm2", Precision::Exact));
        let Action::AppleScript(s) = &p.actions[0] else {
            panic!()
        };
        assert!(s.contains(r#"if unique ID of s is "4C2A-11EF-9B1D""#));
    }

    #[test]
    fn terminal_app_matches_tty() {
        let p = plan_for(&[("TERM_PROGRAM", "Apple_Terminal")], Some("ttys004"), "/w").unwrap();
        let Action::AppleScript(s) = &p.actions[0] else {
            panic!()
        };
        assert!(s.contains(r#"if tty of t is "/dev/ttys004""#));
        // No tty: just activate Terminal.
        let p = plan_for(&[("TERM_PROGRAM", "Apple_Terminal")], None, "/w").unwrap();
        assert_eq!(p.actions, vec![Action::ActivateApp(TERMINAL.into())]);
    }

    #[test]
    fn rejects_values_that_could_inject_script() {
        let p = plan_for(
            &[
                ("TERM_PROGRAM", "iTerm.app"),
                ("ITERM_SESSION_ID", r#"w0:abc" & do shell script "x"#),
            ],
            None,
            "/w",
        )
        .unwrap();
        assert_eq!(p.actions, vec![Action::ActivateApp(ITERM.into())]);
        let p = plan_for(
            &[("TERM_PROGRAM", "Apple_Terminal")],
            Some("ttys0\" & x"),
            "/w",
        )
        .unwrap();
        assert_eq!(p.actions, vec![Action::ActivateApp(TERMINAL.into())]);
        assert!(plan_for(&[("__CFBundleIdentifier", "com.x\"; rm")], None, "/w").is_none());
    }

    #[test]
    fn editors_open_the_project_window() {
        let p = plan_for(
            &[
                ("TERM_PROGRAM", "vscode"),
                ("__CFBundleIdentifier", "com.todesktop.230313mzl4w4u92"),
            ],
            None,
            "/Users/me/my app",
        )
        .unwrap();
        assert_eq!((p.app, p.precision), ("Cursor", Precision::Window));
        assert_eq!(
            p.actions,
            vec![Action::OpenUrl("cursor://file/Users/me/my%20app".into())]
        );
        // The VS Code extension: no TERM_PROGRAM, just the editor's bundle id.
        let p = plan_for(
            &[("__CFBundleIdentifier", "com.microsoft.VSCode")],
            None,
            "/w/api",
        )
        .unwrap();
        assert_eq!(
            p.actions,
            vec![Action::OpenUrl("vscode://file/w/api".into())]
        );
    }

    #[test]
    fn other_terminals() {
        let p = plan_for(&[("KITTY_WINDOW_ID", "3")], None, "/w").unwrap();
        assert_eq!(
            p.actions[0],
            Action::Run(vec![
                "kitty".into(),
                "@".into(),
                "focus-window".into(),
                "--match".into(),
                "id:3".into()
            ])
        );
        let p = plan_for(&[("WEZTERM_PANE", "12")], None, "/w").unwrap();
        assert_eq!(p.app, "WezTerm");
        let p = plan_for(&[("TERM_PROGRAM", "ghostty")], None, "/w").unwrap();
        assert_eq!((p.app, p.precision), ("Ghostty", Precision::App));
        assert!(plan_for(&[], None, "/w").is_none());
    }

    #[test]
    fn tmux_inside_iterm() {
        let p = plan_for(
            &[
                ("TMUX", "/private/tmp/tmux-501/default,1234,0"),
                ("TMUX_PANE", "%7"),
                ("TERM_PROGRAM", "tmux"),
                ("__CFBundleIdentifier", ITERM),
            ],
            None,
            "/w",
        )
        .unwrap();
        assert_eq!(p.app, "tmux");
        assert_eq!(
            p.actions[0],
            Action::Run(vec![
                "tmux".into(),
                "-S".into(),
                "/private/tmp/tmux-501/default".into(),
                "select-window".into(),
                "-t".into(),
                "%7".into()
            ])
        );
        assert_eq!(p.actions[2], Action::ActivateApp(ITERM.into()));
    }

    /// Really switches tmux windows (skipped when tmux isn't installed).
    #[test]
    fn tmux_jump_really_switches_window() {
        if Command::new("tmux").arg("-V").output().is_err() {
            eprintln!("tmux not installed; skipping");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("sock");
        let sock = sock.to_str().unwrap();
        let tmux = |args: &[&str]| {
            let out = Command::new("tmux")
                .arg("-S")
                .arg(sock)
                .args(args)
                .output()
                .unwrap();
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        tmux(&["new-session", "-d", "-s", "t", "-x", "80", "-y", "20"]);
        let first_pane = tmux(&["display-message", "-p", "-t", "t:0", "#{pane_id}"]);
        tmux(&["new-window", "-t", "t"]);
        assert_eq!(
            tmux(&["display-message", "-p", "-t", "t", "#{window_index}"]),
            "1"
        );

        let env = env(&[("TMUX", &format!("{sock},1,0")), ("TMUX_PANE", &first_pane)]);
        let p = plan(&Where {
            env: &env,
            tty: None,
            cwd: "/w",
        })
        .unwrap();
        assert_eq!(run(&p).unwrap(), "Switched to the session in tmux");
        assert_eq!(
            tmux(&["display-message", "-p", "-t", "t", "#{window_index}"]),
            "0"
        );
        tmux(&["kill-server"]);
    }
}
