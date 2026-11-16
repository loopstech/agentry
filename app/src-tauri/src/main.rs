//! The Agentry desktop app: runs the local server and shows the island widget.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod extras;
mod focus;
mod macos;
mod sounds;
mod tray;
mod window;

use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use agentry_core::config::Config;
use agentry_core::settings::{self as hook_settings, HOOKS};
use agentry_core::{paths, HOOK_BIN_NAME};
use agentry_server::store::Session;
use agentry_server::{init_logging, Server, ServerEvent, ServerOptions, Sessions, UiResponse};
use serde::Serialize;
use tauri::{Emitter, Manager, RunEvent, WebviewUrl, WebviewWindowBuilder};
use tokio::sync::broadcast::error::RecvError;

#[derive(Default)]
struct AppStatus(Mutex<ServerInfo>);

#[derive(Default, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ServerInfo {
    port: Option<u16>,
    error: Option<String>,
    #[serde(skip)]
    sessions: Option<Sessions>,
}

#[tauri::command]
fn server_info(status: tauri::State<'_, AppStatus>) -> ServerInfo {
    status.0.lock().unwrap().clone()
}

#[tauri::command]
fn get_sessions(status: tauri::State<'_, AppStatus>) -> Vec<Session> {
    let sessions = status.0.lock().unwrap().sessions.clone();
    sessions.map(|s| s.snapshot()).unwrap_or_default()
}

#[tauri::command]
fn dismiss_session(status: tauri::State<'_, AppStatus>, id: String) -> bool {
    let sessions = status.0.lock().unwrap().sessions.clone();
    sessions.is_some_and(|s| s.dismiss(&id))
}

#[tauri::command]
fn pin_session(status: tauri::State<'_, AppStatus>, id: String, pinned: bool) -> bool {
    let sessions = status.0.lock().unwrap().sessions.clone();
    sessions.is_some_and(|s| s.set_pinned(&id, pinned))
}

/// Answer a pending approval or question. Errors are user-readable, e.g. when
/// the request was already answered in the terminal.
#[tauri::command]
fn respond_request(
    status: tauri::State<'_, AppStatus>,
    request_id: String,
    response: serde_json::Value,
) -> Result<(), String> {
    let response: UiResponse = serde_json::from_value(response).map_err(|e| e.to_string())?;
    let sessions = status.0.lock().unwrap().sessions.clone();
    sessions
        .ok_or("Agentry isn't ready yet")?
        .respond(&request_id, &response)
}

/// Bring the session's terminal tab or editor window forward.
#[tauri::command]
async fn jump_to_session(
    status: tauri::State<'_, AppStatus>,
    id: String,
) -> Result<String, String> {
    let sessions = status.0.lock().unwrap().sessions.clone();
    let session = sessions
        .ok_or("Agentry isn't ready yet")?
        .snapshot()
        .into_iter()
        .find(|s| s.id == id)
        .ok_or("That session is gone")?;
    tauri::async_runtime::spawn_blocking(move || {
        let w = focus::Where {
            env: &session.terminal.env,
            tty: session.terminal.tty.as_deref(),
            cwd: &session.cwd,
        };
        let plan = focus::plan(&w).ok_or_else(|| {
            if session.backfilled {
                format!(
                    "This session started before Agentry, so its window isn't known yet. \
                     It will be once the session does something. It's in {}.",
                    session.cwd
                )
            } else {
                format!(
                    "Agentry doesn't know which window this session is in. It's in {}.",
                    session.cwd
                )
            }
        })?;
        focus::run(&plan)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Recent transcript steps for the detail view.
#[tauri::command]
async fn get_transcript(
    status: tauri::State<'_, AppStatus>,
    id: String,
) -> Result<Vec<agentry_server::transcript::Step>, String> {
    let sessions = status
        .0
        .lock()
        .unwrap()
        .sessions
        .clone()
        .ok_or("Agentry isn't ready yet")?;
    tauri::async_runtime::spawn_blocking(move || sessions.transcript(&id, 50))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
fn set_panel_open(status: tauri::State<'_, AppStatus>, open: bool) {
    if let Some(s) = status.0.lock().unwrap().sessions.clone() {
        s.set_panel_open(open);
    }
}

#[tauri::command]
fn get_config() -> Config {
    Config::load(&paths::agentry_home())
}

/// Save settings, apply them, and tell every window straight away.
#[tauri::command]
fn save_config(app: tauri::AppHandle, config: Config) -> Result<Config, String> {
    let config = config.sanitized();
    config
        .save(&paths::agentry_home())
        .map_err(|e| e.to_string())?;
    let shortcut = apply_config(&app, &config);
    let _ = app.emit("agentry://config", &config);
    shortcut.map(|()| config)
}

/// Side effects of settings outside the webviews. Returns a shortcut error,
/// if the shortcut couldn't be registered.
fn apply_config(app: &tauri::AppHandle, config: &Config) -> Result<(), String> {
    tray::sync(app, config);
    macos::set_dock_icon(app, config.general.dock_icon);
    apply_notch_mode(app, config.appearance.notch);
    extras::apply_shortcut(app, config)
}

/// The notch setting as last applied, so other settings changes don't move
/// the island.
#[derive(Default)]
struct NotchMode(Mutex<Option<bool>>);

/// Attach the island to the notch or free it, when the setting changes.
fn apply_notch_mode(app: &tauri::AppHandle, enabled: bool) {
    let state = app.state::<NotchMode>();
    let mut last = state.0.lock().unwrap();
    if *last == Some(enabled) {
        return;
    }
    *last = Some(enabled);
    if let Some(main) = app.get_webview_window("main") {
        let notch = window::place(&main, enabled);
        let _ = app.emit("agentry://layout", layout_of(notch));
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct IslandLayout {
    /// Set when the island is attached to a notch: its size in points.
    notch: Option<NotchSize>,
}

#[derive(Clone, Copy, Serialize)]
struct NotchSize {
    width: f64,
    height: f64,
}

fn layout_of(notch: Option<window::Notch>) -> IslandLayout {
    IslandLayout {
        notch: notch.map(|n| NotchSize {
            width: n.width,
            height: n.height,
        }),
    }
}

#[tauri::command]
fn get_layout(placement: tauri::State<'_, window::Placement>) -> IslandLayout {
    layout_of(placement.attached_notch())
}

/// Show or hide the island (menu bar icon).
pub fn toggle_island(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        if w.is_visible().unwrap_or(true) {
            let _ = w.hide();
        } else {
            let _ = w.show();
        }
    }
}

/// Open or close the panel (global shortcut). Brings the island back if it
/// was hidden, and focuses it so ⌘↵ / ⌘⌫ work.
pub fn toggle_panel(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
    let _ = app.emit("agentry://toggle-panel", ());
}

/// Open the Settings window on `page` ("settings" or "welcome").
pub fn open_window(app: &tauri::AppHandle, page: &str) -> Result<(), String> {
    if let Some(w) = app.get_webview_window("settings") {
        let _ = w.show();
        let _ = w.unminimize();
        return w.set_focus().map_err(|e| e.to_string());
    }
    let title = if page == "welcome" {
        "Welcome to Agentry"
    } else {
        "Agentry Settings"
    };
    WebviewWindowBuilder::new(
        app,
        "settings",
        WebviewUrl::App(format!("index.html#{page}").into()),
    )
    .title(title)
    .inner_size(620.0, 720.0)
    .min_inner_size(480.0, 480.0)
    .center()
    .build()
    .map(|_| ())
    .map_err(|e| e.to_string())
}

#[tauri::command]
fn set_muted(app: tauri::AppHandle, muted: bool) -> Result<Config, String> {
    let mut config = Config::load(&paths::agentry_home());
    config.muted = muted;
    save_config(app, config)
}

/// The Settings window's "Test" buttons.
#[tauri::command]
fn play_sound(cue: sounds::Cue) {
    let config = Config::load(&paths::agentry_home());
    sounds::play(cue, config.appearance.theme, config.sounds.volume.max(0.2));
}

#[tauri::command]
fn open_settings(app: tauri::AppHandle) -> Result<(), String> {
    open_window(&app, "settings")
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HooksStatus {
    /// The settings file checked (user scope).
    settings_file: String,
    /// Agentry hook entries in it.
    installed: usize,
    expected: usize,
    /// Hook programs those entries point at that don't exist.
    missing_programs: Vec<String>,
    /// The hook program this app would install, if found.
    hook_program: Option<String>,
    claude_version: Option<String>,
}

/// The agentry-hook program next to the app's own executable.
fn bundled_hook() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe = exe.canonicalize().unwrap_or(exe);
    Some(exe.with_file_name(HOOK_BIN_NAME)).filter(|p| p.is_file())
}

#[tauri::command]
async fn hooks_status() -> Result<HooksStatus, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let file = paths::user_settings_file();
        let settings = hook_settings::read_settings(&file).map_err(|e| e.to_string())?;
        let entries = hook_settings::list_agentry_hooks(&settings);
        let mut missing: Vec<String> = entries
            .iter()
            .map(|(_, c)| hook_settings::hook_command_path(c))
            .filter(|p| !p.is_file())
            .map(|p| p.display().to_string())
            .collect();
        missing.sort();
        missing.dedup();
        let claude_version = std::process::Command::new("claude")
            .arg("--version")
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());
        Ok(HooksStatus {
            settings_file: file.display().to_string(),
            installed: entries.len(),
            expected: HOOKS.len(),
            missing_programs: missing,
            hook_program: bundled_hook().map(|p| p.display().to_string()),
            claude_version,
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn install_hooks() -> Result<String, String> {
    let hook = bundled_hook().ok_or("Can't find the agentry-hook program next to the app")?;
    let change = hook_settings::install(&paths::user_settings_file(), &hook, HOOKS)
        .map_err(|e| e.to_string())?;
    Ok(match change.backup {
        Some(b) => format!(
            "Installed. Your previous settings were backed up to {}",
            b.display()
        ),
        None => "Installed.".into(),
    })
}

#[tauri::command]
fn uninstall_hooks() -> Result<String, String> {
    let change =
        hook_settings::uninstall(&paths::user_settings_file()).map_err(|e| e.to_string())?;
    Ok(if change.removed == 0 {
        "There were no Agentry hooks to remove.".into()
    } else {
        "Removed.".into()
    })
}

/// Push settings changed outside the app (e.g. `agentry config`) to the
/// windows.
fn watch_config(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let file = Config::file(&paths::agentry_home());
        let modified = || std::fs::metadata(&file).and_then(|m| m.modified()).ok();
        let mut last: Option<SystemTime> = modified();
        loop {
            std::thread::sleep(Duration::from_secs(1));
            let now = modified();
            if now != last {
                last = now;
                let config = Config::load(&paths::agentry_home());
                // Window, tray and shortcut changes belong on the main thread.
                let handle = app.clone();
                let c = config.clone();
                let _ = app.run_on_main_thread(move || {
                    if let Err(e) = apply_config(&handle, &c) {
                        tracing::warn!("{e}");
                    }
                });
                let _ = app.emit("agentry://config", config);
            }
        }
    });
}

fn main() {
    init_logging();

    let app = tauri::Builder::default()
        // Must be first: launching Agentry again just brings this one forward.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            toggle_panel(app);
        }))
        .plugin(extras::shortcut_plugin())
        .plugin(extras::autostart_plugin())
        .manage(AppStatus::default())
        .manage(window::Placement::default())
        .manage(NotchMode::default())
        .invoke_handler(tauri::generate_handler![
            server_info,
            get_sessions,
            dismiss_session,
            pin_session,
            respond_request,
            jump_to_session,
            get_transcript,
            set_panel_open,
            get_config,
            save_config,
            set_muted,
            play_sound,
            open_settings,
            hooks_status,
            install_hooks,
            uninstall_hooks,
            extras::get_launch_at_login,
            extras::set_launch_at_login,
            extras::check_updates,
            extras::restart_app,
            extras::install_cli,
            extras::finish_welcome,
            window::set_panel_size,
            get_layout,
        ])
        .setup(|app| {
            let config = Config::load(&paths::agentry_home());
            if let Some(main) = app.get_webview_window("main") {
                // Measured once at launch (needs the main thread).
                app.state::<window::Placement>()
                    .set_notches(macos::notches());
                window::place(&main, config.appearance.notch);
                *app.state::<NotchMode>().0.lock().unwrap() = Some(config.appearance.notch);
                macos::float_everywhere(&main);
                let _ = main.show();
            }
            if let Err(e) = tray::create(app.handle()) {
                tracing::warn!("couldn't create the menu bar icon: {e}");
            }
            if let Err(e) = apply_config(app.handle(), &config) {
                tracing::warn!("{e}");
            }
            if extras::updates_configured(app.handle()) {
                app.handle().plugin(extras::updater_plugin())?;
                extras::schedule_update_checks(app.handle().clone());
            }
            extras::maybe_welcome(app.handle());
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move { run_server(handle).await });
            watch_config(app.handle().clone());
            Ok(())
        })
        .on_window_event(|win, event| {
            // Only the island's moves are the user dragging it.
            if let (tauri::WindowEvent::Moved(pos), "main") = (event, win.label()) {
                window::on_moved(win, *pos);
            }
        })
        .build(tauri::generate_context!())
        .expect("failed to build the Agentry app");

    app.run(|handle, event| {
        if let RunEvent::Exit = event {
            // Remove the port file so hooks fail fast after we quit.
            if let Some(port) = handle.state::<AppStatus>().0.lock().unwrap().port {
                paths::remove_port(&paths::agentry_home(), port);
            }
        }
    });
}

async fn run_server(handle: tauri::AppHandle) {
    let status = handle.state::<AppStatus>();
    let server = match Server::start(ServerOptions::default()).await {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("could not start the server: {e:#}");
            status.0.lock().unwrap().error = Some(format!("{e:#}"));
            let _ = handle.emit("agentry://server", status.0.lock().unwrap().clone());
            return;
        }
    };
    {
        let mut info = status.0.lock().unwrap();
        info.port = Some(server.port);
        info.sessions = Some(server.sessions());
    }
    let _ = handle.emit("agentry://server", status.0.lock().unwrap().clone());

    let mut events = server.subscribe();
    let mut cues = sounds::CueTracker::default();
    loop {
        match events.recv().await {
            Ok(ServerEvent::Sessions(list)) => {
                let _ = handle.emit("agentry://sessions", &*list);
                let new_cues = cues.observe(&list);
                if !new_cues.is_empty() {
                    let config = Config::load(&paths::agentry_home());
                    // "Needs you" matters more; play one sound at a time.
                    if let Some(cue) = new_cues.first() {
                        sounds::play_if_enabled(*cue, &config);
                    }
                }
            }
            Ok(ServerEvent::RequestAdded {
                session_id,
                open_panel,
            }) => {
                let _ = handle.emit(
                    "agentry://request",
                    serde_json::json!({ "sessionId": session_id, "openPanel": open_panel }),
                );
            }
            Ok(ServerEvent::Hook { .. }) => {}
            Err(RecvError::Lagged(n)) => {
                // Snapshots are complete, so catching up just means sending the latest.
                tracing::warn!("UI fell behind by {n} updates; resending");
                let _ = handle.emit("agentry://sessions", server.sessions().snapshot());
            }
            Err(RecvError::Closed) => break,
        }
    }
    server.shutdown().await;
}
