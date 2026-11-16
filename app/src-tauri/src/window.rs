//! The island window: placed at the top centre of the screen, kept anchored
//! there as it grows and shrinks, and remembered where the user drags it.
//!
//! The anchor (the window's top-centre point, in logical pixels) is owned
//! here rather than re-derived from the window's geometry on each resize:
//! resizes can arrive in quick succession, before the window system has
//! applied the previous size and position, and re-deriving would drift.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use agentry_core::paths;
use serde::{Deserialize, Serialize};
use tauri::{
    LogicalPosition, LogicalSize, Manager, Monitor, PhysicalPosition, WebviewWindow, Window,
};

/// Gap between the menu bar and the island.
const TOP_GAP: f64 = 8.0;
/// Space kept free below the panel when it's as tall as the screen allows.
const BOTTOM_GAP: f64 = 24.0;
/// Wait for dragging to settle before writing the position to disk.
const SAVE_DELAY: Duration = Duration::from_millis(600);
/// A move event this close to where we put the window is our own doing.
const OWN_MOVE_TOLERANCE: f64 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Anchor {
    center_x: f64,
    top: f64,
}

#[derive(Debug, Clone, Copy)]
struct Layout {
    anchor: Anchor,
    width: f64,
    /// Top-left we last moved the window to.
    placed_at: (f64, f64),
}

/// The window's placement, managed as Tauri state.
#[derive(Default)]
pub struct Placement {
    layout: Mutex<Option<Layout>>,
    save_generation: AtomicU64,
}

fn prefs_file() -> PathBuf {
    paths::agentry_home().join("window.json")
}

fn load_anchor() -> Option<Anchor> {
    serde_json::from_str(&std::fs::read_to_string(prefs_file()).ok()?).ok()
}

fn save_anchor(a: Anchor) {
    if let Ok(text) = serde_json::to_string(&a) {
        let _ = std::fs::create_dir_all(paths::agentry_home());
        let _ = std::fs::write(prefs_file(), text);
    }
}

/// A monitor's usable area (below the menu bar) in logical pixels:
/// (x, y, width, height).
fn logical_work_area(m: &Monitor) -> (f64, f64, f64, f64) {
    let s = m.scale_factor();
    let wa = m.work_area();
    (
        wa.position.x as f64 / s,
        wa.position.y as f64 / s,
        wa.size.width as f64 / s,
        wa.size.height as f64 / s,
    )
}

fn monitor_for(win: &WebviewWindow, a: Anchor) -> Option<Monitor> {
    win.available_monitors().ok()?.into_iter().find(|m| {
        let (x, y, w, h) = logical_work_area(m);
        a.center_x >= x && a.center_x <= x + w && a.top >= y - 1.0 && a.top <= y + h
    })
}

fn default_anchor(win: &WebviewWindow) -> Option<Anchor> {
    let m = win
        .primary_monitor()
        .ok()
        .flatten()
        .or_else(|| win.current_monitor().ok().flatten())?;
    let (x, y, w, _) = logical_work_area(&m);
    Some(Anchor {
        center_x: x + w / 2.0,
        top: y + TOP_GAP,
    })
}

fn apply(win: &WebviewWindow, placement: &Placement, anchor: Anchor, width: f64, height: f64) {
    let x = anchor.center_x - width / 2.0;
    *placement.layout.lock().unwrap() = Some(Layout {
        anchor,
        width,
        placed_at: (x, anchor.top),
    });
    let _ = win.set_size(LogicalSize::new(width, height));
    let _ = win.set_position(LogicalPosition::new(x, anchor.top));
}

/// Put the window at its saved spot if that's still on a screen, else at the
/// top centre of the main screen.
pub fn place_initially(win: &WebviewWindow) {
    let anchor = load_anchor()
        .filter(|a| monitor_for(win, *a).is_some())
        .or_else(|| default_anchor(win));
    let (Some(anchor), Ok(scale), Ok(size)) = (anchor, win.scale_factor(), win.outer_size()) else {
        return;
    };
    let size = size.to_logical::<f64>(scale);
    apply(
        win,
        &win.state::<Placement>(),
        anchor,
        size.width,
        size.height,
    );
}

/// Resize to fit the UI's content, keeping the anchor fixed and the window on
/// screen. Returns the height actually applied.
#[tauri::command]
pub fn set_panel_size(
    window: WebviewWindow,
    placement: tauri::State<'_, Placement>,
    width: f64,
    height: f64,
) -> f64 {
    let anchor = placement
        .layout
        .lock()
        .unwrap()
        .map(|l| l.anchor)
        .or_else(|| default_anchor(&window));
    let Some(anchor) = anchor else { return height };
    let max_height = monitor_for(&window, anchor)
        .map(|m| {
            let (_, y, _, h) = logical_work_area(&m);
            (y + h - anchor.top - BOTTOM_GAP).max(120.0)
        })
        .unwrap_or(height);
    let height = height.min(max_height).max(1.0);
    apply(&window, &placement, anchor, width.max(1.0), height);
    height
}

/// Called on every window move. Moves we caused are ignored; a move by the
/// user (dragging) sets a new anchor, saved once dragging settles.
pub fn on_moved(win: &Window, position: PhysicalPosition<i32>) {
    let app = win.app_handle();
    let Ok(scale) = win.scale_factor() else {
        return;
    };
    let pos = position.to_logical::<f64>(scale);
    let placement = app.state::<Placement>();
    let anchor = {
        let mut layout = placement.layout.lock().unwrap();
        let Some(l) = layout.as_mut() else { return };
        if (pos.x - l.placed_at.0).abs() <= OWN_MOVE_TOLERANCE
            && (pos.y - l.placed_at.1).abs() <= OWN_MOVE_TOLERANCE
        {
            return;
        }
        l.anchor = Anchor {
            center_x: pos.x + l.width / 2.0,
            top: pos.y,
        };
        l.placed_at = (pos.x, pos.y);
        l.anchor
    };

    let gen = placement.save_generation.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SAVE_DELAY).await;
        if app
            .state::<Placement>()
            .save_generation
            .load(Ordering::SeqCst)
            == gen
        {
            save_anchor(anchor);
        }
    });
}
