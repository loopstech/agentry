//! The menu bar icon and its menu.

use agentry_core::config::Config;
use agentry_core::paths;
use tauri::image::Image;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Wry};

/// Kept so the "Quiet mode" tick can follow settings changed elsewhere.
pub struct TrayState {
    quiet: CheckMenuItem<Wry>,
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let config = Config::load(&paths::agentry_home());
    let toggle = MenuItem::with_id(app, "toggle", "Show or Hide Agentry", true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings…", true, Some("CmdOrCtrl+,"))?;
    let quiet =
        CheckMenuItem::with_id(app, "quiet", "Quiet Mode", true, config.muted, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit Agentry", true, Some("CmdOrCtrl+Q"))?;
    let menu = Menu::with_items(
        app,
        &[
            &toggle,
            &settings,
            &quiet,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    TrayIconBuilder::with_id("agentry")
        .icon(Image::from_bytes(include_bytes!(
            "../icons/tray-template.png"
        ))?)
        .icon_as_template(true)
        .tooltip("Agentry")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "toggle" => crate::toggle_island(app),
            "settings" => {
                let _ = crate::open_window(app, "settings");
            }
            "quiet" => {
                let mut config = Config::load(&paths::agentry_home());
                config.muted = !config.muted;
                let _ = crate::save_config(app.clone(), config);
            }
            "quit" => app.exit(0),
            _ => {}
        })
        .build(app)?;

    app.manage(TrayState { quiet });
    Ok(())
}

/// Reflect settings in the menu.
pub fn sync(app: &AppHandle, config: &Config) {
    if let Some(state) = app.try_state::<TrayState>() {
        let _ = state.quiet.set_checked(config.muted);
    }
}
