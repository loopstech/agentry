//! macOS-specific window and app behaviour. No-ops elsewhere.

// The window constants and helpers below are only used on macOS (and tested
// everywhere).
#![cfg_attr(not(target_os = "macos"), allow(dead_code))]

use tauri::{AppHandle, WebviewWindow};

use crate::window::Notch;

/// The notches of the connected screens. Must run on the main thread (it
/// returns nothing otherwise, and nothing on other platforms).
///
/// A screen has a notch when its safe area starts below the top and it
/// reports unobscured areas either side of the camera housing. The notch's
/// width is the gap between those areas, and its height is the safe-area
/// inset (the menu bar's height on such screens).
pub fn notches() -> Vec<Notch> {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::NSScreen;

        let Some(mtm) = MainThreadMarker::new() else {
            return Vec::new();
        };
        let screens = NSScreen::screens(mtm);
        if screens.count() == 0 {
            return Vec::new();
        }
        // AppKit puts the origin at the bottom left of the main screen;
        // window positions use the top left.
        let main_height = screens.objectAtIndex(0).frame().size.height;
        let mut out = Vec::new();
        for i in 0..screens.count() {
            let screen = screens.objectAtIndex(i);
            let frame = screen.frame();
            let top_inset = screen.safeAreaInsets().top;
            let left = screen.auxiliaryTopLeftArea();
            let right = screen.auxiliaryTopRightArea();
            if top_inset <= 0.0 || left.size.width <= 0.0 || right.size.width <= 0.0 {
                continue;
            }
            // The same in screen or global coordinates, so no conversion needed.
            let width = right.origin.x - (left.origin.x + left.size.width);
            let top = main_height - (frame.origin.y + frame.size.height);
            if let Some(n) = Notch::centred(frame.origin.x, top, frame.size.width, width, top_inset)
            {
                out.push(n);
            }
        }
        out
    }
    #[cfg(not(target_os = "macos"))]
    Vec::new()
}

/// The island's window level: just above the menu bar
/// (NSMainMenuWindowLevel + 3, as other notch apps use), so it shows over the
/// menu bar, normal windows and full-screen apps.
const ISLAND_LEVEL: isize = 24 + 3;

// NSWindowCollectionBehavior bits.
const CAN_JOIN_ALL_SPACES: usize = 1 << 0;
const MOVE_TO_ACTIVE_SPACE: usize = 1 << 1;
const STATIONARY: usize = 1 << 4;
const IGNORES_CYCLE: usize = 1 << 6;
const FULL_SCREEN_PRIMARY: usize = 1 << 7;
const FULL_SCREEN_AUXILIARY: usize = 1 << 8;
const FULL_SCREEN_NONE: usize = 1 << 9;

/// The island's Space behaviour, from the window's current one: on every
/// Space (including other apps' full-screen Spaces), left alone by Mission
/// Control and ⌘` cycling. Clears the bits AppKit treats as conflicting.
fn island_behaviour(current: usize) -> usize {
    (current & !(MOVE_TO_ACTIVE_SPACE | FULL_SCREEN_PRIMARY | FULL_SCREEN_NONE))
        | CAN_JOIN_ALL_SPACES
        | STATIONARY
        | IGNORES_CYCLE
        | FULL_SCREEN_AUXILIARY
}

/// Keep the island showing whichever app is in front and on every Space,
/// including over full-screen apps. Call once, at launch, on the main thread.
///
/// tao's "visible on all workspaces" only sets CanJoinAllSpaces; floating over
/// full-screen apps also needs FullScreenAuxiliary and a higher window level.
/// Switching apps or Spaces can still leave the island behind, so this also
/// watches for both and re-applies the settings each time, bringing the
/// island back to the front if it's showing.
pub fn float_everywhere(win: &WebviewWindow) {
    #[cfg(target_os = "macos")]
    {
        use tauri::Manager;
        reassert_float(win, false);
        watch_app_and_space_changes(win.app_handle().clone());
    }
    #[cfg(not(target_os = "macos"))]
    let _ = win;
}

/// Re-apply the island's level and Space behaviour. With `to_front`, also
/// order it to the front if it's visible (a hidden island stays hidden; the
/// app isn't activated). Must run on the main thread.
pub fn reassert_float(win: &WebviewWindow, to_front: bool) {
    #[cfg(target_os = "macos")]
    {
        use objc2::msg_send;
        use objc2::runtime::{AnyObject, Bool};

        let Ok(ptr) = win.ns_window() else { return };
        // SAFETY: `ns_window()` returns the window's live NSWindow, and callers
        // run on the main thread, as AppKit requires.
        unsafe {
            let Some(ns_window) = (ptr as *mut AnyObject).as_ref() else {
                return;
            };
            let current: usize = msg_send![ns_window, collectionBehavior];
            let level: isize = msg_send![ns_window, level];
            let wanted = island_behaviour(current);
            if to_front && (current != wanted || level != ISLAND_LEVEL) {
                tracing::warn!(
                    "the island's window level ({level}) or Space behaviour ({current:#x}) changed; restoring"
                );
            }
            if current != wanted {
                let _: () = msg_send![ns_window, setCollectionBehavior: wanted];
            }
            if level != ISLAND_LEVEL {
                let _: () = msg_send![ns_window, setLevel: ISLAND_LEVEL];
            }
            if to_front {
                let visible: Bool = msg_send![ns_window, isVisible];
                if visible.as_bool() {
                    let _: () = msg_send![ns_window, orderFrontRegardless];
                }
            }
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (win, to_front);
}

/// Re-assert the island whenever another app becomes active or the active
/// Space changes (for example, into a full-screen app).
#[cfg(target_os = "macos")]
fn watch_app_and_space_changes(app: AppHandle) {
    use std::ptr::NonNull;

    use block2::RcBlock;
    use objc2_app_kit::{
        NSWorkspace, NSWorkspaceActiveSpaceDidChangeNotification,
        NSWorkspaceDidActivateApplicationNotification,
    };
    use objc2_foundation::{NSNotification, NSOperationQueue};
    use tauri::Manager;

    let center = NSWorkspace::sharedWorkspace().notificationCenter();
    let queue = NSOperationQueue::mainQueue();
    // SAFETY: reading AppKit's notification-name constants.
    let names = unsafe {
        [
            NSWorkspaceActiveSpaceDidChangeNotification,
            NSWorkspaceDidActivateApplicationNotification,
        ]
    };
    for name in names {
        let app = app.clone();
        let block = RcBlock::new(move |_: NonNull<NSNotification>| {
            if let Some(win) = app.get_webview_window("main") {
                reassert_float(&win, true);
            }
        });
        // SAFETY: no object filter; the block runs on the main queue, which
        // `reassert_float` needs.
        let observer = unsafe {
            center.addObserverForName_object_queue_usingBlock(
                Some(name),
                None,
                Some(&queue),
                &block,
            )
        };
        // Observe for the life of the app.
        std::mem::forget(observer);
    }
}

/// Show or hide the Dock icon. Without it Agentry lives in the menu bar.
/// Safe to call from any thread.
pub fn set_dock_icon(app: &AppHandle, show: bool) {
    #[cfg(target_os = "macos")]
    {
        // Only on a change: switching the activation policy makes the Dock
        // icon flicker, and this runs on every settings change.
        static LAST: std::sync::Mutex<Option<bool>> = std::sync::Mutex::new(None);
        if LAST.lock().unwrap().replace(show) == Some(show) {
            return;
        }
        let handle = app.clone();
        let run = app.run_on_main_thread(move || {
            use tauri::{ActivationPolicy, Manager};
            let main = handle.get_webview_window("main");
            if show {
                // As Electron does for windows that float over full-screen
                // apps: settle the island's Space behaviour while Agentry is
                // an accessory app, then become a regular app with a Dock icon.
                let _ = handle.set_activation_policy(ActivationPolicy::Accessory);
                if let Some(w) = &main {
                    reassert_float(w, false);
                }
            }
            let policy = if show {
                ActivationPolicy::Regular
            } else {
                ActivationPolicy::Accessory
            };
            if let Err(e) = handle.set_activation_policy(policy) {
                tracing::warn!("couldn't change the Dock icon: {e}");
            }
            if let Some(w) = &main {
                reassert_float(w, true);
            }
        });
        if let Err(e) = run {
            tracing::warn!("couldn't change the Dock icon: {e}");
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, show);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn island_behaviour_joins_all_spaces_and_full_screen_apps() {
        let b = island_behaviour(0);
        for bit in [
            CAN_JOIN_ALL_SPACES,
            STATIONARY,
            IGNORES_CYCLE,
            FULL_SCREEN_AUXILIARY,
        ] {
            assert_ne!(b & bit, 0);
        }
    }

    #[test]
    fn island_behaviour_clears_conflicting_bits_and_keeps_others() {
        let managed = 1 << 2;
        let b = island_behaviour(
            MOVE_TO_ACTIVE_SPACE | FULL_SCREEN_PRIMARY | FULL_SCREEN_NONE | managed,
        );
        assert_eq!(
            b & (MOVE_TO_ACTIVE_SPACE | FULL_SCREEN_PRIMARY | FULL_SCREEN_NONE),
            0
        );
        assert_ne!(b & managed, 0);
        assert_eq!(island_behaviour(b), b);
    }
}
