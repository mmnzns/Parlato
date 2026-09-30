// Paste on macOS: post Cmd+V with CGEvent.
//
// Reference VoiceInk : CursorPaster.swift (CGEvent virtual key 0x09 with
// .maskCommand, posted at .cghidEventTap). Needs the Accessibility
// permission; without it macOS drops the events silently. VoiceInk's
// AppleScript fallback for non-QWERTY layouts is not ported: key code 9 is
// the physical V position, which is V on QWERTY, AZERTY and QWERTZ.

use anyhow::{anyhow, Result};
use core_graphics::event::{CGEvent, CGEventFlags, CGEventTapLocation};
use core_graphics::event_source::{CGEventSource, CGEventSourceStateID};
use tracing::debug;

const KVK_ANSI_V: u16 = 9;

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
    down.post(CGEventTapLocation::HID);
    up.post(CGEventTapLocation::HID);
    debug!("Cmd+V posted via CGEvent");
    Ok(())
}
