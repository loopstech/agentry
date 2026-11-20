//! Click-through for the island's transparent area.
//!
//! The island's window stays the size of the open panel, so expanding and
//! collapsing is a CSS animation inside it rather than a native resize (which
//! can't be kept in step with what the web view draws). Most of that window is
//! transparent, so it must not swallow clicks meant for the menu bar or the
//! apps underneath: the UI reports the rectangle the shape occupies, and a
//! light poll of the cursor makes the window ignore the mouse whenever the
//! cursor is outside it.

use std::sync::Mutex;
use std::time::Duration;

use tauri::{AppHandle, Manager, WebviewWindow};

/// How often the cursor is checked. Fast enough that the island responds as
/// soon as the pointer reaches it.
const POLL: Duration = Duration::from_millis(25);

/// A rectangle in the window, in CSS (logical) pixels from its top left.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Rect {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x < self.x + self.width && y >= self.y && y < self.y + self.height
    }
}

/// Where the shape is. `None` until the UI reports it; the whole window
/// takes the mouse until then.
#[derive(Default)]
pub struct HitArea(Mutex<Option<Rect>>);

#[tauri::command]
pub fn set_hit_area(area: tauri::State<'_, HitArea>, x: f64, y: f64, width: f64, height: f64) {
    *area.0.lock().unwrap() = Some(Rect {
        x,
        y,
        width: width.max(0.0),
        height: height.max(0.0),
    });
}

/// Whether the window should take the mouse with the cursor at `cursor`
/// (global logical pixels) and the window's top left at `origin` (the same).
pub fn wants_mouse(area: Option<Rect>, origin: (f64, f64), cursor: (f64, f64)) -> bool {
    match area {
        None => true,
        Some(r) => r.contains(cursor.0 - origin.0, cursor.1 - origin.1),
    }
}

/// Start watching the cursor for the window labelled `label`.
pub fn watch(app: AppHandle, label: &'static str) {
    std::thread::spawn(move || {
        let mut taking: Option<bool> = None;
        loop {
            std::thread::sleep(POLL);
            let Some(win) = app.get_webview_window(label) else {
                continue;
            };
            if !win.is_visible().unwrap_or(false) {
                continue;
            }
            let Some((origin, cursor)) = positions(&app, &win) else {
                continue;
            };
            let area = *app.state::<HitArea>().0.lock().unwrap();
            let want = wants_mouse(area, origin, cursor);
            if taking != Some(want) && win.set_ignore_cursor_events(!want).is_ok() {
                taking = Some(want);
            }
        }
    });
}

/// The window's top left and the cursor, both in global logical pixels.
fn positions(app: &AppHandle, win: &WebviewWindow) -> Option<((f64, f64), (f64, f64))> {
    let cursor = app.cursor_position().ok()?;
    // The cursor comes back scaled by the primary screen's factor (tao on
    // macOS), the window's position by its own screen's.
    let primary = app.primary_monitor().ok()??.scale_factor();
    let scale = win.scale_factor().ok()?;
    let pos = win.outer_position().ok()?;
    Some((
        (pos.x as f64 / scale, pos.y as f64 / scale),
        (cursor.x / primary, cursor.y / primary),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_the_mouse_only_over_the_shape() {
        let area = Some(Rect {
            x: 24.0,
            y: 0.0,
            width: 330.0,
            height: 32.0,
        });
        let origin = (600.0, 0.0);
        assert!(wants_mouse(area, origin, (700.0, 10.0)));
        assert!(
            !wants_mouse(area, origin, (610.0, 10.0)),
            "left of the shape"
        );
        assert!(!wants_mouse(area, origin, (700.0, 40.0)), "below the shape");
        assert!(
            !wants_mouse(area, origin, (954.0, 10.0)),
            "right edge is outside"
        );
    }

    #[test]
    fn whole_window_until_the_ui_reports() {
        assert!(wants_mouse(None, (0.0, 0.0), (5000.0, 5000.0)));
    }
}
