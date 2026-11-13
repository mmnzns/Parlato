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
//
// The second half of this file makes the pill and its popover a NON-ACTIVATING
// PANEL that floats over full-screen apps, like VoiceInk's MiniRecorderPanel
// (as remembered from VoiceInk, not re-read from its source):
//   NSPanel, styleMask [.nonactivatingPanel, ...], isFloatingPanel = true,
//   level = .floating, collectionBehavior = [.canJoinAllSpaces,
//   .fullScreenAuxiliary, .stationary, .ignoresCycle], hidesOnDeactivate =
//   false, canBecomeKey = false, canBecomeMain = false.
// Parlato uses the same values (level 3 for the pill, 4 for the popover so it
// stays above the pill). A regular (Dock icon) app's plain NSWindow is not
// allowed over another app's full-screen Space; a non-activating NSPanel with
// fullScreenAuxiliary is. Tauri/tao creates an NSWindow subclass ("TaoWindow"),
// so we turn it into an NSPanel subclass at run time, the way tauri-nspanel
// does, with a safety check (see `panel_class`).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;

use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
use objc2::{class, msg_send, sel};
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

// -- Non-activating panel over full-screen apps -----------------------------

/// NSWindowStyleMaskNonactivatingPanel.
const STYLE_NONACTIVATING_PANEL: usize = 1 << 7;
/// canJoinAllSpaces | stationary | ignoresCycle | fullScreenAuxiliary.
const COLLECTION_BEHAVIOR: usize = (1 << 0) | (1 << 4) | (1 << 6) | (1 << 8);
/// NSFloatingWindowLevel (VoiceInk `.floating`).
const LEVEL_FLOATING: isize = 3;

/// Whether the pill may become the key window (take keyboard input). Off for
/// the recorder: the app being typed into must keep the keyboard. A feature
/// that needs typing in the pill (editing the text before pasting) turns it on
/// only while its editor is open, with `set_accepts_key`.
static ACCEPTS_KEY: AtomicBool = AtomicBool::new(false);

/// Used by the "Review before pasting" editor (review.rs): on while it is open.
pub fn set_accepts_key(accepts: bool) {
    ACCEPTS_KEY.store(accepts, Ordering::SeqCst);
}

/// Makes `window` the key window WITHOUT activating the app (tao's own
/// `set_focus` also calls activateIgnoringOtherApps, which would pull Parlato
/// in front of the app being typed into and can switch Spaces). Only works on
/// a panel made by `make_floating_panel` while `set_accepts_key(true)`.
pub fn make_key(window: &WebviewWindow) {
    let w = window.clone();
    let _ = window.run_on_main_thread(move || {
        let Ok(ptr) = w.ns_window() else {
            return;
        };
        let obj = ptr as *mut AnyObject;
        if obj.is_null() {
            return;
        }
        // SAFETY: `ptr` is this window's NSWindow, alive while `w` is held,
        // and we are on the main thread.
        unsafe {
            let _: () = msg_send![obj, makeKeyAndOrderFront: std::ptr::null::<AnyObject>()];
        }
    });
}

extern "C" fn can_become_key(_this: &AnyObject, _cmd: Sel) -> Bool {
    Bool::new(ACCEPTS_KEY.load(Ordering::SeqCst))
}

extern "C" fn can_become_main(_this: &AnyObject, _cmd: Sel) -> Bool {
    Bool::NO
}

/// An NSPanel subclass with the same memory layout as tao's `TaoWindow`
/// (NSWindow + one BOOL ivar named `focusable`). Switching a window's class
/// to it is only sound when both instance sizes are equal, so that is checked
/// and `None` is returned (and the caller falls back to level + collection
/// behaviour + non-focusable) if a macOS release ever changes NSPanel's size.
fn panel_class(window_class: &AnyClass) -> Option<&'static AnyClass> {
    static CLASS: OnceLock<Option<&'static AnyClass>> = OnceLock::new();
    *CLASS.get_or_init(|| {
        let mut decl = ClassBuilder::new(c"ParlatoPillPanel", class!(NSPanel))?;
        decl.add_ivar::<Bool>(c"focusable");
        // SAFETY: the signatures match `-canBecomeKeyWindow` / `-canBecomeMainWindow`.
        unsafe {
            decl.add_method(
                sel!(canBecomeKeyWindow),
                can_become_key as extern "C" fn(_, _) -> _,
            );
            decl.add_method(
                sel!(canBecomeMainWindow),
                can_become_main as extern "C" fn(_, _) -> _,
            );
        }
        let class = decl.register();
        (class.instance_size() == window_class.instance_size()).then_some(class)
    })
}

/// Makes `window` a non-activating, floating panel that appears over
/// full-screen apps on every Space. `above_pill` raises the level one step
/// (the popover). Safe to call more than once on the same window. The work
/// runs on the main thread; it is queued before any later `show()`.
pub fn make_floating_panel(window: &WebviewWindow, above_pill: bool) {
    // Fallback and extra safety: tao's own "may not become key" flag.
    let _ = window.set_focusable(false);
    let level = LEVEL_FLOATING + isize::from(above_pill);
    let w = window.clone();
    let _ = window.run_on_main_thread(move || {
        let Ok(ptr) = w.ns_window() else {
            return;
        };
        let obj = ptr as *mut AnyObject;
        if obj.is_null() {
            return;
        }
        // SAFETY: `ptr` is this window's NSWindow, alive while `w` is held,
        // and we are on the main thread, where AppKit windows are edited.
        unsafe {
            let current: &AnyClass = (*obj).class();
            let swapped = match panel_class(current) {
                Some(panel) => {
                    if !std::ptr::eq(current, panel) {
                        objc2::ffi::object_setClass(obj.cast(), (panel as *const AnyClass).cast());
                    }
                    true
                }
                None => false,
            };
            if swapped {
                let mask: usize = msg_send![obj, styleMask];
                let _: () = msg_send![obj, setStyleMask: mask | STYLE_NONACTIVATING_PANEL];
                let _: () = msg_send![obj, setFloatingPanel: Bool::YES];
                let _: () = msg_send![obj, setHidesOnDeactivate: Bool::NO];
            }
            let _: () = msg_send![obj, setLevel: level];
            let _: () = msg_send![obj, setCollectionBehavior: COLLECTION_BEHAVIOR];
        }
        tracing::debug!(label = w.label(), "recorder window configured as floating panel");
    });
}
