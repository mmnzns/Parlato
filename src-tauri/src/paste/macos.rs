// Paste on macOS: post Cmd+V with CGEvent.
//
// Reference VoiceInk : CursorPaster.swift (CGEvent virtual key 0x09 with
// .maskCommand, posted at .cghidEventTap). Needs the Accessibility
// permission; without it macOS drops the events silently. VoiceInk's
// AppleScript fallback for non-QWERTY layouts is not ported: key code 9 is
// the physical V position, which is V on QWERTY, AZERTY and QWERTZ.

use std::sync::atomic::{AtomicI32, Ordering};
use std::thread::sleep;
use std::time::Duration;

use anyhow::{anyhow, Result};
use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};
use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use tracing::debug;

const KVK_ANSI_V: u16 = 9;

/// Process id of the app the user was typing in when recording started.
/// 0 = none recorded.
static TARGET_PID: AtomicI32 = AtomicI32::new(0);

/// Remembers the frontmost app (unless it is Parlato itself), so the paste
/// goes back there even if the user clicked the recorder pill meanwhile.
/// Reference VoiceInk : the recorder is a non-activating NSPanel; Parlato's
/// pill is a normal window, so it re-activates the target app instead.
pub fn remember_frontmost() {
    let Some(app) = NSWorkspace::sharedWorkspace().frontmostApplication() else {
        return;
    };
    let pid = app.processIdentifier();
    if pid > 0 && pid != std::process::id() as i32 {
        TARGET_PID.store(pid, Ordering::Relaxed);
    }
}

/// Brings the remembered app back to the front if Parlato took focus.
/// Returns true if it had to switch apps (the caller waits a moment).
pub fn restore_frontmost() -> bool {
    let pid = TARGET_PID.load(Ordering::Relaxed);
    if pid <= 0 {
        return false;
    }
    let ours = NSRunningApplication::currentApplication();
    if !ours.isActive() {
        return false;
    }
    let Some(target) = NSRunningApplication::runningApplicationWithProcessIdentifier(pid) else {
        return false;
    };
    #[allow(deprecated)]
    let ok = target.activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps);
    if ok {
        // Give macOS time to move keyboard focus before Cmd+V.
        sleep(Duration::from_millis(120));
        debug!(pid, "re-activated the target app before pasting");
    }
    ok
}

pub fn send_cmd_v() -> Result<()> {
    if !crate::hotkeys::keyboard_hook::accessibility_trusted() {
        return Err(anyhow!("Accessibility permission missing, cannot paste"));
    }
    let source = CGEventSource::new(CGEventSourceStateID::HIDSystemState)
        .map_err(|_| anyhow!("CGEventSource"))?;
    let down = CGEvent::new_keyboard_event(source.clone(), KVK_ANSI_V, true)
        .map_err(|_| anyhow!("CGEvent key down"))?;
    down.set_flags(CGEventFlags::CGEventFlagCommand);
    let up = CGEvent::new_keyboard_event(source, KVK_ANSI_V, false)
        .map_err(|_| anyhow!("CGEvent key up"))?;
    up.set_flags(CGEventFlags::CGEventFlagCommand);
    // Parlato's own hotkey tap must not react to this Cmd+V.
    for ev in [&down, &up] {
        ev.set_integer_value_field(
            core_graphics::event::EventField::EVENT_SOURCE_USER_DATA,
            crate::hotkeys::keyboard_hook::PARLATO_EVENT_TAG,
        );
    }
    down.post(CGEventTapLocation::HID);
    up.post(CGEventTapLocation::HID);
    debug!("Cmd+V posted via CGEvent");
    Ok(())
}
