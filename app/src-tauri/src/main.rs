//! The Agentry desktop app: runs the local server and shows the island widget.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod window;

use std::sync::Mutex;

use agentry_core::config::Config;
use agentry_core::paths;
use agentry_server::store::Session;
use agentry_server::{init_logging, Server, ServerEvent, ServerOptions, Sessions, UiResponse};
use serde::Serialize;
use tauri::{Emitter, Manager, RunEvent};
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

#[tauri::command]
fn set_muted(muted: bool) -> Result<Config, String> {
    let home = paths::agentry_home();
    let mut config = Config::load(&home);
    config.muted = muted;
    config.save(&home).map_err(|e| e.to_string())?;
    Ok(config)
}

fn main() {
    init_logging();

    let app = tauri::Builder::default()
        .manage(AppStatus::default())
        .manage(window::Placement::default())
        .invoke_handler(tauri::generate_handler![
            server_info,
            get_sessions,
            dismiss_session,
            pin_session,
            respond_request,
            set_panel_open,
            get_config,
            set_muted,
            window::set_panel_size,
        ])
        .setup(|app| {
            if let Some(main) = app.get_webview_window("main") {
                window::place_initially(&main);
                let _ = main.show();
            }
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move { run_server(handle).await });
            Ok(())
        })
        .on_window_event(|win, event| {
            if let tauri::WindowEvent::Moved(pos) = event {
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
    loop {
        match events.recv().await {
            Ok(ServerEvent::Sessions(list)) => {
                let _ = handle.emit("agentry://sessions", &*list);
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
