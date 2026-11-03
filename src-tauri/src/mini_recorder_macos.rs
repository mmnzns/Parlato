// Mini-recorder placement on macOS: the visible frame of a screen (menu bar
// and Dock excluded), in the same coordinates as Tauri's monitor rectangles.
//
// Why this exists: Tauri's `Monitor::work_area()` on macOS (tauri-runtime-wry
// monitor/macos.rs) takes the SIZE of NSScreen.visibleFrame but only shifts
// the X origin; the Y origin stays at the top of the screen. NSScreen's
// coordinates are bottom-left based, so the menu bar height is lost and the
// computed bottom edge ends up one menu-bar height (~34 pt) above the real
// Dock top. Here the rectangle is converted properly.
//
// Reference VoiceInk : MiniRecorderPanel.swift positions the panel from
// NSScreen.visibleFrame (`screen.visibleFrame.minY + padding`).

use objc2_app_kit::NSScreen;
use objc2_foundation::MainThreadMarker;
use tauri::WebviewWindow;

use crate::mini_recorder::PxRect;

/// Visible frame (physical pixels, top-left origin, global desktop
/// coordinates) of the screen whose frame is `screen` (a Tauri monitor
/// rectangle). `None` when no NSScreen matches.
pub fn visible_frame(window: &WebviewWindow, screen: PxRect, scale: f64) -> Option<PxRect> {
    if let Some(mtm) = MainThreadMarker::new() {
        return compute(mtm, screen, scale);
    }
    // NSScreen is main-thread only: hop there and wait for the answer.
    let (tx, rx) = std::sync::mpsc::channel();
    window
        .run_on_main_thread(move || {
            let result = MainThreadMarker::new().and_then(|mtm| compute(mtm, screen, scale));
            let _ = tx.send(result);
        })
        .ok()?;
    rx.recv_timeout(std::time::Duration::from_millis(500))
        .ok()
        .flatten()
}

fn compute(mtm: MainThreadMarker, screen: PxRect, scale: f64) -> Option<PxRect> {
    let screens = NSScreen::screens(mtm);
    // The first screen is the one with the menu bar; its height is the Y flip
    // between Cocoa (bottom-left origin) and Tauri (top-left origin).
    let primary_h = screens.firstObject()?.frame().size.height;
    let want_x = screen.x as f64 / scale;
    let want_y = screen.y as f64 / scale;
    for s in screens.iter() {
        let frame = s.frame();
        let top = primary_h - (frame.origin.y + frame.size.height);
        if (frame.origin.x - want_x).abs() > 1.5 || (top - want_y).abs() > 1.5 {
            continue;
        }
        let vis = s.visibleFrame();
        let vis_top = primary_h - (vis.origin.y + vis.size.height);
        return Some(PxRect {
            x: (vis.origin.x * scale).round() as i32,
            y: (vis_top * scale).round() as i32,
            w: (vis.size.width * scale).round() as i32,
            h: (vis.size.height * scale).round() as i32,
        });
    }
    None
}
