//! macOS-specific window and app behaviour. No-ops elsewhere.

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

/// Let the island show on every Space and on top of full-screen apps.
///
/// tao's "visible on all workspaces" only sets CanJoinAllSpaces; floating over
/// full-screen apps also needs FullScreenAuxiliary and a higher window level.
pub fn float_everywhere(win: &WebviewWindow) {
    #[cfg(target_os = "macos")]
    {
        use objc2::msg_send;
        use objc2::runtime::AnyObject;

        // NSWindowCollectionBehavior bits.
        const CAN_JOIN_ALL_SPACES: usize = 1 << 0;
        const STATIONARY: usize = 1 << 4;
        const IGNORES_CYCLE: usize = 1 << 6;
        const FULL_SCREEN_AUXILIARY: usize = 1 << 8;
        // NSStatusWindowLevel: above full-screen apps and normal windows.
        const STATUS_WINDOW_LEVEL: isize = 25;

        let Ok(ptr) = win.ns_window() else { return };
        // SAFETY: `ns_window()` returns the window's live NSWindow; this runs
        // on the main thread (setup), as AppKit requires.
        unsafe {
            let Some(ns_window) = (ptr as *mut AnyObject).as_ref() else {
                return;
            };
            let current: usize = msg_send![ns_window, collectionBehavior];
            let wanted =
                current | CAN_JOIN_ALL_SPACES | STATIONARY | IGNORES_CYCLE | FULL_SCREEN_AUXILIARY;
            let _: () = msg_send![ns_window, setCollectionBehavior: wanted];
            let _: () = msg_send![ns_window, setLevel: STATUS_WINDOW_LEVEL];
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = win;
}

/// Show or hide the Dock icon. Without it Agentry lives in the menu bar.
pub fn set_dock_icon(app: &AppHandle, show: bool) {
    #[cfg(target_os = "macos")]
    {
        use tauri::ActivationPolicy;
        let policy = if show {
            ActivationPolicy::Regular
        } else {
            ActivationPolicy::Accessory
        };
        if let Err(e) = app.set_activation_policy(policy) {
            tracing::warn!("couldn't change the Dock icon: {e}");
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, show);
}
