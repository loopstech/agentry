//! Global shortcut, launch at login, updates and the command-line tool.

use std::path::{Path, PathBuf};

use agentry_core::config::Config;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_autostart::ManagerExt as _;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tauri_plugin_updater::UpdaterExt;

// ---------- Global shortcut ----------

pub fn shortcut_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_global_shortcut::Builder::new()
        .with_handler(|app, _shortcut, event| {
            if event.state == ShortcutState::Pressed {
                crate::toggle_panel(app);
            }
        })
        .build()
}

/// (Re)register the shortcut from settings. Returns a readable error.
pub fn apply_shortcut(app: &AppHandle, config: &Config) -> Result<(), String> {
    let gs = app.global_shortcut();
    let _ = gs.unregister_all();
    let Some(text) = config.general.shortcut.as_deref() else {
        return Ok(());
    };
    let shortcut: Shortcut = text
        .parse()
        .map_err(|e| format!("“{text}” isn't a shortcut Agentry understands ({e})"))?;
    gs.register(shortcut)
        .map_err(|e| format!("Couldn't use {text}; another app may already use it ({e})"))
}

// ---------- Launch at login ----------

pub fn autostart_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry> {
    tauri_plugin_autostart::init(tauri_plugin_autostart::MacosLauncher::LaunchAgent, None)
}

#[tauri::command]
pub fn get_launch_at_login(app: AppHandle) -> bool {
    app.autolaunch().is_enabled().unwrap_or(false)
}

#[tauri::command]
pub fn set_launch_at_login(app: AppHandle, enabled: bool) -> Result<bool, String> {
    let manager = app.autolaunch();
    if enabled {
        manager.enable()
    } else {
        manager.disable()
    }
    .map_err(|e| e.to_string())?;
    Ok(manager.is_enabled().unwrap_or(enabled))
}

// ---------- Updates ----------

/// Whether this build was configured for updates (release builds are; local
/// builds have no signing key and no update feed).
pub fn updates_configured(app: &AppHandle) -> bool {
    app.config().plugins.0.contains_key("updater")
}

pub fn updater_plugin() -> tauri::plugin::TauriPlugin<tauri::Wry, tauri_plugin_updater::Config> {
    tauri_plugin_updater::Builder::new().build()
}

/// Look for an update; when `install`, download and install it. The new
/// version runs after a restart.
pub async fn check_for_update(app: &AppHandle, install: bool) -> Result<String, String> {
    if !updates_configured(app) {
        return Err(
            "Updates aren't set up in this build (it wasn't built by the release workflow).".into(),
        );
    }
    let updater = app.updater().map_err(|e| e.to_string())?;
    let Some(update) = updater
        .check()
        .await
        .map_err(|e| format!("Couldn't check for updates: {e}"))?
    else {
        return Ok(format!(
            "Agentry {} is up to date.",
            app.package_info().version
        ));
    };
    let version = update.version.clone();
    if !install {
        return Ok(format!("Agentry {version} is available."));
    }
    update
        .download_and_install(|_, _| {}, || {})
        .await
        .map_err(|e| format!("Couldn't install the update: {e}"))?;
    let _ = app.emit("agentry://update-installed", &version);
    Ok(format!(
        "Agentry {version} is installed. Restart to use it."
    ))
}

#[tauri::command]
pub async fn check_updates(app: AppHandle) -> Result<String, String> {
    check_for_update(&app, true).await
}

#[tauri::command]
pub fn restart_app(app: AppHandle) {
    app.restart();
}

/// Check now, then once a day, if the user wants automatic updates.
pub fn schedule_update_checks(app: AppHandle) {
    if !updates_configured(&app) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        loop {
            let config = Config::load(&agentry_core::paths::agentry_home());
            if config.general.auto_update {
                match check_for_update(&app, true).await {
                    Ok(msg) => tracing::info!("{msg}"),
                    Err(e) => tracing::warn!("{e}"),
                }
            }
            tokio::time::sleep(std::time::Duration::from_secs(24 * 60 * 60)).await;
        }
    });
}

// ---------- Command-line tool ----------

/// The `agentry` command next to the app's own executable.
fn bundled_cli() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe = exe.canonicalize().unwrap_or(exe);
    Some(exe.with_file_name("agentry")).filter(|p| p.is_file())
}

fn on_path(dir: &Path) -> bool {
    std::env::var_os("PATH").is_some_and(|p| std::env::split_paths(&p).any(|d| d == dir))
}

/// Link `agentry` into /usr/local/bin if writable, else ~/.local/bin.
#[tauri::command]
pub fn install_cli() -> Result<String, String> {
    let cli = bundled_cli().ok_or("The agentry command wasn't found inside the app")?;
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or("No home folder")?;
    let system = PathBuf::from("/usr/local/bin");
    let dir = if writable(&system) {
        system
    } else {
        home.join(".local/bin")
    };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let link = dir.join("agentry");
    link_to(&cli, &link)?;
    let mut msg = format!("Installed: {} → {}", link.display(), cli.display());
    if !on_path(&dir) {
        msg.push_str(&format!(". Add {} to your PATH to use it.", dir.display()));
    }
    Ok(msg)
}

fn writable(dir: &Path) -> bool {
    let probe = dir.join(format!(".agentry-write-test-{}", std::process::id()));
    let ok = std::fs::write(&probe, b"").is_ok();
    let _ = std::fs::remove_file(&probe);
    ok
}

/// Create or replace a symlink, but never overwrite a real file.
fn link_to(target: &Path, link: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(link) {
        Ok(m) if m.file_type().is_symlink() => {
            std::fs::remove_file(link).map_err(|e| e.to_string())?
        }
        Ok(_) => {
            return Err(format!(
                "{} already exists and isn't a link; leaving it alone",
                link.display()
            ))
        }
        Err(_) => {}
    }
    #[cfg(unix)]
    return std::os::unix::fs::symlink(target, link).map_err(|e| e.to_string());
    #[cfg(not(unix))]
    return Err("Only supported on macOS and Linux".into());
}

/// Show the welcome screen on first launch.
pub fn maybe_welcome(app: &AppHandle) {
    if !Config::load(&agentry_core::paths::agentry_home()).onboarded {
        let _ = crate::open_window(app, "welcome");
    }
}

#[tauri::command]
pub fn finish_welcome(app: AppHandle) -> Result<(), String> {
    let mut config = Config::load(&agentry_core::paths::agentry_home());
    config.onboarded = true;
    crate::save_config(app.clone(), config)?;
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.close();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_but_never_overwrites_files() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("agentry-real");
        std::fs::write(&target, "x").unwrap();
        let link = dir.path().join("agentry");
        link_to(&target, &link).unwrap();
        link_to(&target, &link).unwrap(); // replacing our own link is fine
        assert_eq!(std::fs::read_link(&link).unwrap(), target);

        let file = dir.path().join("taken");
        std::fs::write(&file, "mine").unwrap();
        assert!(link_to(&target, &file).is_err());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "mine");
    }
}
