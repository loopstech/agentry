//! Type a message into the terminal a session runs in, as if the user had
//! typed it there and pressed Return.
//!
//! Claude Code's hooks can't start a turn, so this is the only way to message
//! a session from the widget. It works where the terminal can type into one
//! exact pane: tmux, iTerm2 and WezTerm. Like `focus`, `plan` is pure and
//! testable anywhere; `run` carries it out.

use std::process::Command;
use std::time::Duration;

use crate::focus::{safe, Where};

/// Longest message, in characters.
pub const MAX_MESSAGE: usize = 4000;

/// Pause between the text and Return, so Claude Code doesn't take the burst
/// of keys for a paste and put the Return in the message.
const SUBMIT_DELAY: Duration = Duration::from_millis(150);

#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// Program and arguments.
    Run(Vec<String>),
    /// An AppleScript with a `run argv` handler and its arguments. The
    /// message is passed as an argument, never spliced into the script.
    AppleScript {
        script: String,
        args: Vec<String>,
    },
    Pause,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// Human name, e.g. "iTerm2".
    pub app: &'static str,
    pub steps: Vec<Step>,
}

/// The message as one line of typed text: control characters removed and
/// line breaks turned into spaces (a newline would submit early). `None` if
/// nothing is left.
pub fn clean(text: &str) -> Option<String> {
    let one_line: String = text
        .chars()
        .map(|c| {
            if c == '\n' || c == '\r' || c == '\t' {
                ' '
            } else {
                c
            }
        })
        .filter(|c| !c.is_control())
        .collect();
    let trimmed = one_line.trim();
    (!trimmed.is_empty()).then(|| trimmed.chars().take(MAX_MESSAGE).collect())
}

const ITERM_SCRIPT: &str = r#"on run argv
  set u to item 1 of argv
  set msg to item 2 of argv
  tell application id "com.googlecode.iterm2"
    repeat with w in windows
      repeat with t in tabs of w
        repeat with s in sessions of t
          if unique ID of s is u then
            tell s to write text msg newline no
            delay 0.15
            tell s to write text "" newline yes
            return "ok"
          end if
        end repeat
      end repeat
    end repeat
  end tell
  return "missing"
end run"#;

/// How to type `text` (already `clean`ed) into the session's pane, or why
/// it can't be done.
pub fn plan(w: &Where, text: &str) -> Result<Plan, String> {
    let get = |k: &str| w.env.get(k).map(String::as_str);

    if let (Some(tmux), Some(pane)) = (get("TMUX"), get("TMUX_PANE").and_then(|p| safe(p, "%"))) {
        let socket = tmux
            .split(',')
            .next()
            .filter(|s| s.starts_with('/'))
            .and_then(|s| safe(s, "/._-"))
            .ok_or("Agentry couldn't find this session's tmux server")?;
        let send = |rest: &[&str]| {
            let mut argv: Vec<String> = Vec::from(
                [
                    "tmux",
                    "-S",
                    socket.as_str(),
                    "send-keys",
                    "-t",
                    pane.as_str(),
                ]
                .map(String::from),
            );
            argv.extend(rest.iter().map(|s| s.to_string()));
            Step::Run(argv)
        };
        return Ok(Plan {
            app: "tmux",
            steps: vec![send(&["-l", "--", text]), Step::Pause, send(&["Enter"])],
        });
    }

    if get("TERM_PROGRAM") == Some("iTerm.app") {
        let uuid = get("ITERM_SESSION_ID")
            .and_then(|s| s.split(':').nth(1))
            .and_then(|u| safe(u, "-"))
            .ok_or("Agentry doesn't know which iTerm2 session this is")?;
        return Ok(Plan {
            app: "iTerm2",
            steps: vec![Step::AppleScript {
                script: ITERM_SCRIPT.into(),
                args: vec![uuid, text.to_owned()],
            }],
        });
    }

    if let Some(pane) = get("WEZTERM_PANE").and_then(|p| safe(p, "")) {
        let send = |what: &str| {
            Step::Run(Vec::from(
                [
                    "wezterm",
                    "cli",
                    "send-text",
                    "--pane-id",
                    pane.as_str(),
                    "--no-paste",
                    "--",
                    what,
                ]
                .map(String::from),
            ))
        };
        return Ok(Plan {
            app: "WezTerm",
            steps: vec![send(text), Step::Pause, send("\r")],
        });
    }

    Err(
        "Messaging from Agentry works for sessions in tmux, iTerm2 or WezTerm. \
         Use ↗ to go to this one."
            .into(),
    )
}

/// Whether Claude Code is still running on `tty` (e.g. `ttys004`), so typed
/// text reaches Claude and not the shell it was started from.
pub fn claude_on_tty(tty: &str) -> bool {
    let Some(tty) = safe(tty.trim_start_matches("/dev/"), "/") else {
        return false;
    };
    let Ok(out) = Command::new("ps")
        .args(["-o", "comm=", "-t", &tty])
        .output()
    else {
        return false;
    };
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .any(|l| is_claude_command(l.trim()))
}

/// A process name that's Claude Code: `claude`, the native build (named after
/// its version, e.g. `2.1.325`), or `node` running the npm package.
fn is_claude_command(comm: &str) -> bool {
    let name = comm.rsplit('/').next().unwrap_or(comm);
    name == "claude"
        || name == "node"
        || (name.contains('.')
            && name
                .split('.')
                .all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())))
}

/// Carry out a plan. Returns a short description for the UI.
pub fn run(plan: &Plan) -> Result<String, String> {
    let failed = || format!("Couldn't type into {}", plan.app);
    for step in &plan.steps {
        match step {
            Step::Run(argv) => {
                let out = Command::new(&argv[0])
                    .args(&argv[1..])
                    .output()
                    .map_err(|_| failed())?;
                if !out.status.success() {
                    let err = String::from_utf8_lossy(&out.stderr).trim().to_string();
                    return Err(if err.is_empty() {
                        failed()
                    } else {
                        format!("{}: {err}", failed())
                    });
                }
            }
            Step::AppleScript { script, args } => {
                if !cfg!(target_os = "macos") {
                    return Err("Messaging iTerm2 needs macOS".into());
                }
                let out = Command::new("osascript")
                    .arg("-e")
                    .arg(script)
                    .args(args)
                    .output()
                    .map_err(|_| failed())?;
                let said = String::from_utf8_lossy(&out.stdout);
                if !out.status.success() || said.trim() != "ok" {
                    return Err(format!("{}: the session's tab wasn't found", failed()));
                }
            }
            Step::Pause => std::thread::sleep(SUBMIT_DELAY),
        }
    }
    Ok(format!("Sent to Claude in {}", plan.app))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn env(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    fn plan_for(pairs: &[(&str, &str)], text: &str) -> Result<Plan, String> {
        plan(
            &Where {
                env: &env(pairs),
                tty: None,
                cwd: "/w",
            },
            text,
        )
    }

    #[test]
    fn cleans_messages_to_one_line() {
        assert_eq!(
            clean("  fix it\nthen test\r\n").as_deref(),
            Some("fix it then test")
        );
        assert_eq!(clean("a\u{1b}[2Jb\u{7}").as_deref(), Some("a[2Jb"));
        assert_eq!(clean(" \n\t "), None);
        assert_eq!(
            clean(&"x".repeat(5000)).unwrap().chars().count(),
            MAX_MESSAGE
        );
    }

    #[test]
    fn tmux_types_literally_then_presses_enter() {
        let p = plan_for(
            &[
                ("TMUX", "/private/tmp/tmux-501/default,1234,0"),
                ("TMUX_PANE", "%7"),
            ],
            "-n; rm -rf ~ $(x)",
        )
        .unwrap();
        let base = [
            "tmux",
            "-S",
            "/private/tmp/tmux-501/default",
            "send-keys",
            "-t",
            "%7",
        ];
        let with =
            |rest: &[&str]| Step::Run(base.iter().chain(rest).map(|s| s.to_string()).collect());
        assert_eq!(
            p.steps,
            vec![
                with(&["-l", "--", "-n; rm -rf ~ $(x)"]),
                Step::Pause,
                with(&["Enter"])
            ]
        );
    }

    #[test]
    fn iterm_passes_the_message_as_an_argument() {
        let msg = r#"say "hi" & do shell script "x""#;
        let p = plan_for(
            &[
                ("TERM_PROGRAM", "iTerm.app"),
                ("ITERM_SESSION_ID", "w0t1p0:4C2A-11EF"),
            ],
            msg,
        )
        .unwrap();
        let Step::AppleScript { script, args } = &p.steps[0] else {
            panic!()
        };
        assert_eq!(script, ITERM_SCRIPT);
        assert_eq!(args, &vec!["4C2A-11EF".to_string(), msg.to_string()]);
        // An unsafe session id is refused rather than used.
        assert!(plan_for(
            &[
                ("TERM_PROGRAM", "iTerm.app"),
                ("ITERM_SESSION_ID", "w0:\" & x")
            ],
            "hi"
        )
        .is_err());
    }

    #[test]
    fn wezterm_and_unsupported_terminals() {
        let p = plan_for(&[("WEZTERM_PANE", "12")], "hi").unwrap();
        assert_eq!(p.app, "WezTerm");
        assert_eq!(p.steps.len(), 3);
        assert!(plan_for(&[("TERM_PROGRAM", "Apple_Terminal")], "hi").is_err());
        assert!(plan_for(&[("TERM_PROGRAM", "vscode")], "hi").is_err());
        assert!(plan_for(&[], "hi").is_err());
    }

    #[test]
    fn recognises_claude_processes() {
        assert!(is_claude_command("claude"));
        assert!(is_claude_command(
            "/Users/me/.local/share/claude/versions/2.1.325"
        ));
        assert!(is_claude_command("node"));
        assert!(!is_claude_command("-zsh"));
        assert!(!is_claude_command("bash"));
        assert!(!is_claude_command("vim"));
        assert!(!is_claude_command("1."));
    }

    /// Really types into a tmux pane (skipped when tmux isn't installed).
    #[test]
    fn tmux_message_really_arrives() {
        if Command::new("tmux").arg("-V").output().is_err() {
            eprintln!("tmux not installed; skipping");
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let sock = dir.path().join("sock");
        let sock = sock.to_str().unwrap();
        let out = dir.path().join("out");
        let tmux = |args: &[&str]| {
            let o = Command::new("tmux")
                .args(["-f", "/dev/null", "-S", sock])
                .args(args)
                .output()
                .unwrap();
            String::from_utf8_lossy(&o.stdout).trim().to_string()
        };
        // The pane reads one line into a file, standing in for Claude Code.
        let reader = format!("head -n 1 > '{}'; sleep 5", out.display());
        tmux(&[
            "new-session",
            "-d",
            "-s",
            "t",
            "-x",
            "80",
            "-y",
            "20",
            "sh",
            "-c",
            &reader,
        ]);
        let pane = tmux(&["display-message", "-p", "-t", "t:", "#{pane_id}"]);
        let msg = "fix the $HOME bug; don't `run` anything";
        let p = plan_for(
            &[("TMUX", &format!("{sock},1,0")), ("TMUX_PANE", &pane)],
            msg,
        )
        .unwrap();
        assert_eq!(run(&p).unwrap(), "Sent to Claude in tmux");
        let mut got = String::new();
        for _ in 0..100 {
            got = std::fs::read_to_string(&out).unwrap_or_default();
            if got.ends_with('\n') {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        tmux(&["kill-server"]);
        assert_eq!(got, format!("{msg}\n"));
    }
}
