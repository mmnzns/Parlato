// Mini-recorder placement on macOS: the screen under the mouse and its visible
// frame (menu bar and Dock excluded), in global POINTS with a top-left origin.
//
// Why this exists, two problems with Tauri's physical-pixel monitor API:
// - Mixed DPI: tao builds each monitor's physical rectangle as its points
//   multiplied by THAT monitor's own scale factor, so physical coordinates are
//   not one consistent global space when screens have different scales. And
//   `set_position(PhysicalPosition)` converts back to points using the scale of
//   the window's CURRENT monitor, not the target's. A pill last shown on a 1x
//   screen and targeted at a 2x screen therefore lands off every screen.
//   `cursor_position()` / `monitor_from_point()` share the same ambiguity.
// - `Monitor::work_area()` (tauri-runtime-wry monitor/macos.rs) takes the SIZE
//   of NSScreen.visibleFrame but only shifts the X origin; the menu bar height
//   is lost.
// So we ask AppKit directly (NSEvent mouse location, NSScreen frames), which
// is one consistent space in points, and position the window in logical units.
//
// Reference VoiceInk : MiniRecorderPanel.swift positions the panel from
// NSScreen.visibleFrame (`screen.visibleFrame.minY + padding`).

use objc2_app_kit::{NSEvent, NSScreen};
use objc2_foundation::MainThreadMarker;
use tauri::WebviewWindow;

use crate::mini_recorder::PxRect;

/// Frame and visible frame (global points, top-left origin) of the screen
/// containing the mouse, or of the main screen when none matches. Returns
/// `(frame, visible_frame)`; `None` when there is no screen at all.
pub fn screen_under_mouse(window: &WebviewWindow) -> Option<(PxRect, PxRect)> {
    if let Some(mtm) = MainThreadMarker::new() {
        return compute(mtm);
    }
    // NSScreen is main-thread only: hop there and wait for the answer.
    let (tx, rx) = std::sync::mpsc::channel();
    window
        .run_on_main_thread(move || {
            let result = MainThreadMarker::new().and_then(compute);
            let _ = tx.send(result);
        })
        .ok()?;
    rx.recv_timeout(std::time::Duration::from_millis(500))
        .ok()
        .flatten()
}

fn compute(mtm: MainThreadMarker) -> Option<(PxRect, PxRect)> {
    let screens = NSScreen::screens(mtm);
    // The first screen is the one with the menu bar; its height is the Y flip
    // between Cocoa (bottom-left origin) and top-left origin.
    let primary_h = screens.firstObject()?.frame().size.height;
    let mouse = NSEvent::mouseLocation();
    let chosen = screens
        .iter()
        .find(|s| {
            let f = s.frame();
            mouse.x >= f.origin.x
                && mouse.x < f.origin.x + f.size.width
                && mouse.y >= f.origin.y
                && mouse.y < f.origin.y + f.size.height
        })
        .or_else(|| screens.firstObject())?;
    let to_rect = |r: objc2_foundation::NSRect| PxRect {
        x: r.origin.x.round() as i32,
        y: (primary_h - (r.origin.y + r.size.height)).round() as i32,
        w: r.size.width.round() as i32,
        h: r.size.height.round() as i32,
    };
    Some((to_rect(chosen.frame()), to_rect(chosen.visibleFrame())))
}
