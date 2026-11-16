//! The Agentry desktop app.
//!
//! Phase 1: runs the local server, logs every hook event to the console and
//! shows them in a plain debug window. The widget UI arrives in Phase 2.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Mutex;

use agentry_core::paths;
use agentry_server::{init_logging, Server, ServerEvent, ServerOptions};
use serde::Serialize;
use tauri::{Emitter, Manager, RunEvent};
use tokio::sync::broadcast::error::RecvError;

#[derive(Default)]
struct AppStatus(Mutex<ServerInfo>);

#[derive(Default, Clone, Serialize)]
struct ServerInfo {
    port: Option<u16>,
    error: Option<String>,
}

#[tauri::command]
fn server_info(status: tauri::State<'_, AppStatus>) -> ServerInfo {
    status.0.lock().unwrap().clone()
}

fn main() {
    init_logging();

    let app = tauri::Builder::default()
        .manage(AppStatus::default())
        .invoke_handler(tauri::generate_handler![server_info])
        .setup(|app| {
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move { run_server(handle).await });
            Ok(())
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
    status.0.lock().unwrap().port = Some(server.port);
    let _ = handle.emit("agentry://server", status.0.lock().unwrap().clone());

    let mut events = server.subscribe();
    loop {
        match events.recv().await {
            Ok(ServerEvent::Hook { summary, .. }) => {
                let _ = handle.emit("agentry://event", &summary);
            }
            Err(RecvError::Lagged(n)) => tracing::warn!("UI fell behind; skipped {n} events"),
            Err(RecvError::Closed) => break,
        }
    }
    server.shutdown().await;
}
