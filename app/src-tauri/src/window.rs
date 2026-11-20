//! The island window: placed at the top centre of the screen, kept anchored
//! there as it grows and shrinks, and remembered where the user drags it.
//! On a screen with a notch (and with notch mode on) it's attached to the
//! notch instead, flush with the top of the screen.
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
/// Room kept below a dropped island for its open panel.
const MIN_PANEL_ROOM: f64 = 240.0;
/// The panel is never squeezed below this.
const MIN_HEIGHT: f64 = 120.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Anchor {
    center_x: f64,
    top: f64,
}

/// A screen's notch, in global logical points (origin at the top left of
/// the main screen, y downwards — the same space as window positions).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Notch {
    /// The screen's left edge, top edge and width.
    pub screen_x: f64,
    pub screen_y: f64,
    pub screen_width: f64,
    /// The notch's left edge, width and height (the height is the menu bar's).
    pub x: f64,
    pub width: f64,
    pub height: f64,
}

impl Notch {
    /// A notch of `width` × `height` centred at the top of a screen (every
    /// MacBook notch is centred).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    pub fn centred(
        screen_x: f64,
        screen_y: f64,
        screen_width: f64,
        width: f64,
        height: f64,
    ) -> Option<Notch> {
        let sane = width > 0.0 && height > 0.0 && width < screen_width && height < 200.0;
        sane.then(|| Notch {
            screen_x,
            screen_y,
            screen_width,
            x: screen_x + (screen_width - width) / 2.0,
            width,
            height,
        })
    }

    fn anchor(&self) -> Anchor {
        Anchor {
            center_x: self.x + self.width / 2.0,
            top: self.screen_y,
        }
    }

    fn on_screen(&self, x: f64) -> bool {
        x >= self.screen_x && x < self.screen_x + self.screen_width
    }
}

/// The notch the island should attach to: the one on the screen the island
/// would otherwise be on (its saved spot, or the main screen).
pub fn pick_notch(notches: &[Notch], island_x: f64) -> Option<Notch> {
    notches.iter().copied().find(|n| n.on_screen(island_x))
}

#[derive(Debug, Clone, Copy)]
struct Layout {
    anchor: Anchor,
    width: f64,
    /// The height the UI last asked for (it's capped to the screen).
    wanted_height: f64,
    /// Top-left we last moved the window to.
    placed_at: (f64, f64),
}

/// The window's placement, managed as Tauri state.
#[derive(Default)]
pub struct Placement {
    layout: Mutex<Option<Layout>>,
    save_generation: AtomicU64,
    /// Notches on the connected screens, measured at launch.
    notches: Mutex<Vec<Notch>>,
    /// The notch the island is attached to, if any.
    attached: Mutex<Option<Notch>>,
}

impl Placement {
    pub fn set_notches(&self, notches: Vec<Notch>) {
        if !notches.is_empty() {
            tracing::info!(?notches, "screen notch found");
        }
        *self.notches.lock().unwrap() = notches;
    }

    pub fn attached_notch(&self) -> Option<Notch> {
        *self.attached.lock().unwrap()
    }
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

/// The monitor holding the anchor point (its whole area, menu bar included).
fn monitor_for(win: &WebviewWindow, a: Anchor) -> Option<Monitor> {
    win.available_monitors().ok()?.into_iter().find(|m| {
        let s = m.scale_factor();
        let (x, y) = (m.position().x as f64 / s, m.position().y as f64 / s);
        let (w, h) = (m.size().width as f64 / s, m.size().height as f64 / s);
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

/// Where the island goes when it's not on a notch: its saved spot if that's
/// still on a screen, else the top centre of the main screen.
fn free_anchor(win: &WebviewWindow) -> Option<Anchor> {
    load_anchor()
        .filter(|a| monitor_for(win, *a).is_some())
        .or_else(|| default_anchor(win))
}

/// The window's height for `wanted`, when its top is at `top` on a screen
/// whose usable area ends at `work_bottom`.
fn fit_height(wanted: f64, top: f64, work_bottom: f64) -> f64 {
    wanted
        .min((work_bottom - top - BOTTOM_GAP).max(MIN_HEIGHT))
        .max(1.0)
}

/// Keep a dropped island's window on its screen (`work` is the usable area:
/// x, y, width, height): wholly inside it sideways, and high enough to leave
/// the open panel room below.
fn keep_on_screen(a: Anchor, width: f64, work: (f64, f64, f64, f64)) -> Anchor {
    let (x, y, w, h) = work;
    let center_x = if w >= width {
        a.center_x.clamp(x + width / 2.0, x + w - width / 2.0)
    } else {
        x + w / 2.0
    };
    let max_top = (y + h - BOTTOM_GAP - MIN_PANEL_ROOM).max(y);
    Anchor {
        center_x,
        top: a.top.clamp(y, max_top),
    }
}

fn apply(
    win: &WebviewWindow,
    placement: &Placement,
    anchor: Anchor,
    width: f64,
    height: f64,
    wanted_height: f64,
) {
    let x = anchor.center_x - width / 2.0;
    *placement.layout.lock().unwrap() = Some(Layout {
        anchor,
        width,
        wanted_height,
        placed_at: (x, anchor.top),
    });
    let _ = win.set_size(LogicalSize::new(width, height));
    let _ = win.set_position(LogicalPosition::new(x, anchor.top));
}

/// Place the island: on the notch of its screen when `notch_mode` is on and
/// that screen has one, else at its free spot. Returns the notch it's
/// attached to, if any. Safe to call again when the setting changes.
pub fn place(win: &WebviewWindow, notch_mode: bool) -> Option<Notch> {
    let placement = win.state::<Placement>();
    let free = free_anchor(win)?;
    let notch = if notch_mode {
        pick_notch(&placement.notches.lock().unwrap(), free.center_x)
    } else {
        None
    };
    *placement.attached.lock().unwrap() = notch;
    let anchor = notch.map(|n| n.anchor()).unwrap_or(free);

    let (Ok(scale), Ok(size)) = (win.scale_factor(), win.outer_size()) else {
        return notch;
    };
    let size = size.to_logical::<f64>(scale);
    let wanted = placement
        .layout
        .lock()
        .unwrap()
        .map(|l| l.wanted_height)
        .unwrap_or(size.height);
    apply(win, &placement, anchor, size.width, size.height, wanted);
    notch
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
    let Some(mut anchor) = anchor else {
        return height;
    };
    let wanted = height;
    let work = monitor_for(&window, anchor).map(|m| logical_work_area(&m));
    // Floating: keep the whole window on screen (this also fixes a position
    // saved before it was kept on screen). On a notch it stays put.
    if let (Some(work), None) = (work, placement.attached_notch()) {
        anchor = keep_on_screen(anchor, width.max(1.0), work);
    }
    let height = work
        .map(|(_, y, _, h)| fit_height(wanted, anchor.top, y + h))
        .unwrap_or(wanted.max(1.0));
    apply(&window, &placement, anchor, width.max(1.0), height, wanted);
    height
}

/// Called on every window move. Moves we caused are ignored; a move by the
/// user (dragging) sets a new anchor, saved once dragging settles. On a notch
/// the island stays put.
pub fn on_moved(win: &Window, position: PhysicalPosition<i32>) {
    let app = win.app_handle();
    let Ok(scale) = win.scale_factor() else {
        return;
    };
    let pos = position.to_logical::<f64>(scale);
    let placement = app.state::<Placement>();
    if placement.attached_notch().is_some() {
        return;
    }
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
    let label = win.label().to_string();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(SAVE_DELAY).await;
        if app
            .state::<Placement>()
            .save_generation
            .load(Ordering::SeqCst)
            != gen
        {
            return;
        }
        let anchor = match app.get_webview_window(&label) {
            Some(w) => settle(&w, anchor).unwrap_or(anchor),
            None => anchor,
        };
        save_anchor(anchor);
    });
}

/// The window's new size, for the UI.
#[derive(Clone, Copy, Serialize)]
struct WindowSize {
    width: f64,
    height: f64,
}

/// After a drag: keep the window on its screen and refit its height to the
/// room below it, so the open panel doesn't run off the screen. Tells the UI
/// when the height changes. Returns the anchor it ended up at.
fn settle(win: &WebviewWindow, dropped: Anchor) -> Option<Anchor> {
    use tauri::Emitter;
    let placement = win.state::<Placement>();
    let layout = (*placement.layout.lock().unwrap())?;
    let work = logical_work_area(&monitor_for(win, dropped)?);
    let anchor = keep_on_screen(dropped, layout.width, work);
    let height = fit_height(layout.wanted_height, anchor.top, work.1 + work.3);
    let current = win
        .outer_size()
        .ok()?
        .to_logical::<f64>(win.scale_factor().ok()?)
        .height;
    if anchor != dropped || (height - current).abs() > 0.5 {
        apply(
            win,
            &placement,
            anchor,
            layout.width,
            height,
            layout.wanted_height,
        );
        let _ = win.app_handle().emit(
            "agentry://window-size",
            WindowSize {
                width: layout.width,
                height,
            },
        );
    }
    Some(anchor)
}

#[cfg(test)]
mod tests {
    use super::*;

    // A 14" MacBook Pro built-in screen at the origin, and an external
    // screen to its right.
    fn builtin() -> Notch {
        Notch::centred(0.0, 0.0, 1512.0, 186.0, 32.0).unwrap()
    }

    // A 1512×982 screen whose usable area starts below a 32 pt menu bar.
    const WORK: (f64, f64, f64, f64) = (0.0, 32.0, 1512.0, 950.0);

    #[test]
    fn height_fits_the_room_below() {
        // Plenty of room: what was asked for.
        assert_eq!(fit_height(600.0, 40.0, 982.0), 600.0);
        // Dragged low: down to the bottom gap.
        assert_eq!(fit_height(2000.0, 500.0, 982.0), 982.0 - 500.0 - BOTTOM_GAP);
        // Never below the minimum.
        assert_eq!(fit_height(2000.0, 950.0, 982.0), MIN_HEIGHT);
    }

    #[test]
    fn dropped_island_stays_on_screen() {
        let width = 548.0;
        let ok = Anchor {
            center_x: 756.0,
            top: 40.0,
        };
        assert_eq!(keep_on_screen(ok, width, WORK), ok);
        // Too far left or right: the whole window comes back on screen.
        let left = keep_on_screen(
            Anchor {
                center_x: 30.0,
                top: 40.0,
            },
            width,
            WORK,
        );
        assert_eq!(left.center_x, width / 2.0);
        let right = keep_on_screen(
            Anchor {
                center_x: 1500.0,
                top: 40.0,
            },
            width,
            WORK,
        );
        assert_eq!(right.center_x, 1512.0 - width / 2.0);
        // Too low: moved up to leave room for the panel.
        let low = keep_on_screen(
            Anchor {
                center_x: 756.0,
                top: 900.0,
            },
            width,
            WORK,
        );
        assert_eq!(low.top, 982.0 - BOTTOM_GAP - MIN_PANEL_ROOM);
        // Above the usable area (over the menu bar): moved down to it.
        let high = keep_on_screen(
            Anchor {
                center_x: 756.0,
                top: 0.0,
            },
            width,
            WORK,
        );
        assert_eq!(high.top, 32.0);
    }

    #[test]
    fn notch_is_centred_and_anchors_at_the_top() {
        let n = builtin();
        assert_eq!(n.x, 663.0);
        assert_eq!(
            n.anchor(),
            Anchor {
                center_x: 756.0,
                top: 0.0
            }
        );
    }

    #[test]
    fn rejects_nonsense_measurements() {
        assert!(Notch::centred(0.0, 0.0, 1512.0, 0.0, 32.0).is_none());
        assert!(Notch::centred(0.0, 0.0, 1512.0, 186.0, 0.0).is_none());
        assert!(Notch::centred(0.0, 0.0, 1512.0, 2000.0, 32.0).is_none());
    }

    #[test]
    fn picks_the_notch_on_the_islands_screen() {
        let notches = [builtin()];
        // Island on the built-in screen: attach.
        assert_eq!(pick_notch(&notches, 756.0), Some(builtin()));
        // Island dragged to an external screen on the right: no notch there.
        assert_eq!(pick_notch(&notches, 2500.0), None);
        // No notch at all.
        assert_eq!(pick_notch(&[], 756.0), None);
        // An external screen at negative x (left of the main screen).
        let left = Notch::centred(-1920.0, -200.0, 1920.0, 200.0, 38.0).unwrap();
        assert_eq!(pick_notch(&[builtin(), left], -900.0), Some(left));
    }
}
