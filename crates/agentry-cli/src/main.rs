//! `agentry`: install and remove Claude Code hooks, check the app is running,
//! and diagnose problems, without opening the app.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Stdio};
use std::time::{Duration, Instant};

use agentry_core::settings::{self, HOOKS};
use agentry_core::{client, paths, HOOK_BIN_NAME, VERSION};
use anyhow::{anyhow, Context, Result};
use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "agentry",
    version,
    about = "Agentry: see and steer every Claude Code session from one widget"
)]
struct Cli {
    #[command(subcommand)]
    command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Install or remove Agentry's Claude Code hooks.
    Hooks {
        #[command(subcommand)]
        action: HooksCmd,
    },
    /// Show whether the Agentry app is running.
    Status,
    /// Check hooks, the app and Claude Code, and explain any problems.
    Doctor,
    /// Print the Agentry version.
    Version,
}

#[derive(Subcommand)]
enum HooksCmd {
    /// Add Agentry's hooks to Claude Code settings (backs the file up first).
    Install {
        #[command(flatten)]
        scope: Scope,
        /// Path to the agentry-hook program. Defaults to the one next to this command.
        #[arg(long, value_name = "PATH")]
        hook_path: Option<PathBuf>,
    },
    /// Remove Agentry's hooks from Claude Code settings, leaving everything else.
    Uninstall {
        #[command(flatten)]
        scope: Scope,
    },
}

#[derive(Args)]
struct Scope {
    /// Use <DIR>/.claude/settings.local.json instead of your user settings
    /// (DIR defaults to the current directory).
    #[arg(long, value_name = "DIR", num_args = 0..=1, default_missing_value = ".")]
    project: Option<PathBuf>,
}

impl Scope {
    fn file(&self) -> Result<PathBuf> {
        Ok(match &self.project {
            Some(dir) => {
                let dir = dir
                    .canonicalize()
                    .with_context(|| format!("no such directory: {}", dir.display()))?;
                paths::project_settings_file(&dir)
            }
            None => paths::user_settings_file(),
        })
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let result = match cli.command {
        Cmd::Hooks {
            action: HooksCmd::Install { scope, hook_path },
        } => install(&scope, hook_path),
        Cmd::Hooks {
            action: HooksCmd::Uninstall { scope },
        } => uninstall(&scope),
        Cmd::Status => status(),
        Cmd::Doctor => doctor(),
        Cmd::Version => {
            println!("agentry {VERSION}");
            Ok(true)
        }
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::FAILURE
        }
    }
}

/// The agentry-hook program shipped next to this command.
fn default_hook_path() -> Result<PathBuf> {
    let exe = std::env::current_exe().context("finding the agentry command's location")?;
    let exe = exe.canonicalize().unwrap_or(exe);
    let candidate = exe.with_file_name(HOOK_BIN_NAME);
    if candidate.is_file() {
        Ok(candidate)
    } else {
        Err(anyhow!(
            "couldn't find {HOOK_BIN_NAME} next to {} — pass --hook-path",
            exe.display()
        ))
    }
}

fn install(scope: &Scope, hook_path: Option<PathBuf>) -> Result<bool> {
    let hook = match hook_path {
        Some(p) => p
            .canonicalize()
            .with_context(|| format!("no such file: {}", p.display()))?,
        None => default_hook_path()?,
    };
    if !is_executable(&hook) {
        return Err(anyhow!("{} is not an executable file", hook.display()));
    }
    let file = scope.file()?;
    let change = settings::install(&file, &hook, HOOKS)?;
    println!(
        "Installed {} Agentry hooks in {}",
        change.added,
        change.file.display()
    );
    if change.removed > 0 {
        println!("  (replaced {} earlier Agentry entries)", change.removed);
    }
    if let Some(b) = change.backup {
        println!("  backup: {}", b.display());
    }
    println!("  hook:   {}", hook.display());
    let other = if scope.project.is_some() {
        Some(paths::user_settings_file())
    } else {
        None
    };
    if let Some(other) = other {
        if settings::read_settings(&other)
            .is_ok_and(|s| !settings::list_agentry_hooks(&s).is_empty())
        {
            println!(
                "Warning: Agentry hooks are also in {}; sessions in this project would send every event twice.",
                other.display()
            );
        }
    }
    println!("Running Claude Code sessions pick this up on their next start.");
    Ok(true)
}

fn uninstall(scope: &Scope) -> Result<bool> {
    let file = scope.file()?;
    let change = settings::uninstall(&file)?;
    if change.removed == 0 {
        println!("No Agentry hooks in {}; nothing to do.", file.display());
    } else {
        println!(
            "Removed {} Agentry hooks from {}",
            change.removed,
            file.display()
        );
        if let Some(b) = change.backup {
            println!("  backup: {}", b.display());
        }
    }
    Ok(true)
}

/// `Some(version)` if the app answers `/health`.
fn app_health() -> Option<(u16, String)> {
    let (port, token) = paths::read_connection(&paths::agentry_home())?;
    let r = client::request("GET", port, &token, "/health", None, Duration::from_secs(1))?;
    if r.status != 200 {
        return None;
    }
    let v: serde_json::Value = serde_json::from_str(&r.body).ok()?;
    Some((port, v.get("version")?.as_str()?.to_string()))
}

fn status() -> Result<bool> {
    let Some((port, version)) = app_health() else {
        println!("Agentry is not running. Hooks pass straight through to Claude Code.");
        return Ok(false);
    };
    println!("Agentry {version} is running on 127.0.0.1:{port}");
    let sessions = fetch_sessions().unwrap_or_default();
    if sessions.is_empty() {
        println!("No Claude Code sessions right now.");
        return Ok(true);
    }
    println!();
    for s in &sessions {
        let get = |k: &str| s.get(k).and_then(|v| v.as_str()).unwrap_or("");
        let status = match get("status") {
            "needs_permission" | "needs_answer" | "needs_terminal" => "NEEDS YOU",
            "working" => "working",
            "idle" => "ready",
            "error" => "error",
            "done" => "done",
            other => other,
        };
        let progress = s
            .get("progress")
            .and_then(|p| {
                Some(format!(
                    " {}/{}",
                    p.get("done")?.as_u64()?,
                    p.get("total")?.as_u64()?
                ))
            })
            .unwrap_or_default();
        println!(
            "  {status:<9}  {}  ({}){progress}",
            get("title"),
            get("project")
        );
        println!("             {}", get("subtitle"));
        for h in s
            .get("helpers")
            .and_then(|h| h.as_array())
            .into_iter()
            .flatten()
        {
            let hget = |k: &str| h.get(k).and_then(|v| v.as_str()).unwrap_or("");
            println!("             └ {}: {}", hget("agentType"), hget("subtitle"));
        }
    }
    Ok(true)
}

fn fetch_sessions() -> Option<Vec<serde_json::Value>> {
    let (port, token) = paths::read_connection(&paths::agentry_home())?;
    let r = client::request(
        "GET",
        port,
        &token,
        "/sessions",
        None,
        Duration::from_secs(1),
    )?;
    serde_json::from_str(&r.body).ok()
}

struct Report {
    failures: usize,
}

impl Report {
    fn ok(&self, msg: impl AsRef<str>) {
        println!("  ✓ {}", msg.as_ref());
    }
    fn warn(&self, msg: impl AsRef<str>) {
        println!("  ! {}", msg.as_ref());
    }
    fn fail(&mut self, msg: impl AsRef<str>) {
        self.failures += 1;
        println!("  ✗ {}", msg.as_ref());
    }
}

fn doctor() -> Result<bool> {
    let mut r = Report { failures: 0 };
    println!("Agentry {VERSION}");

    println!("\nClaude Code");
    match claude_version() {
        Some(v) => r.ok(format!("claude {v}")),
        None => r.warn("`claude` not found on PATH (fine if you only use the editor extensions)"),
    }

    println!("\nHooks");
    let user = paths::user_settings_file();
    let in_user = check_settings_file(&mut r, &user, true);
    let project = paths::project_settings_file(&std::env::current_dir()?);
    if project.exists() && check_settings_file(&mut r, &project, false) && in_user {
        r.warn(format!(
            "hooks are installed in both {} and {} — every event is sent twice; \
             remove one with `agentry hooks uninstall [--project]`",
            user.display(),
            project.display()
        ));
    }

    println!("\nApp");
    let home = paths::agentry_home();
    check_token(&mut r, &home);
    match app_health() {
        Some((port, v)) => r.ok(format!("app {v} answering on 127.0.0.1:{port}")),
        None if paths::port_file(&home).exists() => r.warn(
            "port file exists but the app isn't answering (it may have crashed); hooks fail open",
        ),
        None => r.warn("app not running; hooks fail open until you start it"),
    }

    println!();
    if r.failures == 0 {
        println!("No problems found.");
    } else {
        println!("{} problem(s) found.", r.failures);
    }
    Ok(r.failures == 0)
}

/// Checks one settings file; returns whether it has Agentry hooks.
fn check_settings_file(r: &mut Report, file: &Path, required: bool) -> bool {
    let shown = file.display();
    let settings = match settings::read_settings(file) {
        Ok(s) => s,
        Err(e) => {
            r.fail(format!("{shown}: {e}"));
            return false;
        }
    };
    let entries = settings::list_agentry_hooks(&settings);
    if entries.is_empty() {
        if required {
            r.fail(format!(
                "no Agentry hooks in {shown} — run `agentry hooks install`"
            ));
        }
        return false;
    }
    let missing: Vec<&str> = HOOKS
        .iter()
        .filter(|s| !entries.iter().any(|(e, _)| e == s.event))
        .map(|s| s.event)
        .collect();
    if missing.is_empty() {
        r.ok(format!("{} hooks registered in {shown}", entries.len()));
    } else {
        r.fail(format!(
            "{shown} is missing hooks for {} — run `agentry hooks install`",
            missing.join(", ")
        ));
    }
    let mut paths: Vec<PathBuf> = entries
        .iter()
        .map(|(_, c)| settings::hook_command_path(c))
        .collect();
    paths.sort();
    paths.dedup();
    for p in paths {
        if is_executable(&p) {
            r.ok(format!("hook program {}", p.display()));
        } else {
            r.fail(format!(
                "hook program {} is missing (the app moved?) — run `agentry hooks install` again",
                p.display()
            ));
        }
    }
    true
}

fn check_token(r: &mut Report, home: &Path) {
    let token = paths::token_file(home);
    if !token.exists() {
        r.warn(format!(
            "{} not created yet (the app creates it on first start)",
            token.display()
        ));
        return;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        match std::fs::metadata(&token) {
            Ok(m) if m.permissions().mode() & 0o077 == 0 => {
                r.ok(format!("{} is private", token.display()))
            }
            Ok(_) => r.fail(format!(
                "{} is readable by other users — run: chmod 600 {}",
                token.display(),
                token.display()
            )),
            Err(e) => r.fail(format!("{}: {e}", token.display())),
        }
    }
}

fn is_executable(p: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(p).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        p.is_file()
    }
}

/// `claude --version`, with a timeout in case it hangs.
fn claude_version() -> Option<String> {
    let mut child = Command::new("claude")
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        match child.try_wait().ok()? {
            Some(_) => break,
            None if Instant::now() > deadline => {
                let _ = child.kill();
                return None;
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
    let out = child.wait_with_output().ok()?;
    let text = String::from_utf8(out.stdout).ok()?;
    let v = text.trim().to_string();
    (!v.is_empty()).then_some(v)
}
