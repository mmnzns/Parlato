// Keyboard hook on macOS: a CGEventTap on a dedicated thread.
//
// The tap translates macOS virtual key codes (physical key positions, ANSI
// layout) into the Windows Virtual Key codes the rest of the module and the
// saved settings use, then feeds them to the shared `handle_key`. That keeps
// one hotkey format on both platforms and one copy of the trigger logic
// (hold / toggle / combos / utilities / cancel / Power Mode digits).
//
// Reference VoiceInk : HotkeyManager.swift (flagsChanged + keyCode for the
// modifier-only hotkeys) and ShortcutMonitor (active tap, returns nil to
// swallow an event).
//
// Permission: an active tap needs Accessibility. Without it
// CGEventTapCreate returns NULL; we ask macOS to show its prompt once and
// retry every few seconds, so the hotkey starts working as soon as the user
// ticks Parlato in System Settings, without a restart.

use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};
use std::time::Duration;

use core_foundation::base::TCFType;
use core_foundation::boolean::CFBoolean;
use core_foundation::dictionary::CFDictionary;
use core_foundation::runloop::{kCFRunLoopCommonModes, CFRunLoop};
use core_foundation::string::{CFString, CFStringRef};
use core_graphics::event::{
    CGEvent, CGEventTap, CGEventTapLocation, CGEventTapOptions, CGEventTapPlacement,
    CGEventTapProxy, CGEventType, CallbackResult, EventField,
};
use tracing::{debug, info, warn};

use super::{handle_key, HookContext, HotkeyEvent, HotkeyTrigger, HOOK_CONTEXT};

/// Tag put in EVENT_SOURCE_USER_DATA on the events Parlato posts itself
/// (the Cmd+V of a paste), so its own tap ignores them.
pub const PARLATO_EVENT_TAG: i64 = 0x5041_524C; // "PARL"

// Device-dependent modifier bits (IOLLEvent.h, NX_DEVICE*KEYMASK). They tell
// left from right, which the device-independent CGEventFlags cannot.
const NX_DEVICELCTLKEYMASK: u64 = 0x0000_0001;
const NX_DEVICELSHIFTKEYMASK: u64 = 0x0000_0002;
const NX_DEVICERSHIFTKEYMASK: u64 = 0x0000_0004;
const NX_DEVICELCMDKEYMASK: u64 = 0x0000_0008;
const NX_DEVICERCMDKEYMASK: u64 = 0x0000_0010;
const NX_DEVICELALTKEYMASK: u64 = 0x0000_0020;
const NX_DEVICERALTKEYMASK: u64 = 0x0000_0040;
const NX_DEVICERCTLKEYMASK: u64 = 0x0000_2000;

/// Mach port of the live tap, so the callback can re-enable it when macOS
/// disables it (callback too slow, or secure input).
static TAP_PORT: AtomicPtr<c_void> = AtomicPtr::new(std::ptr::null_mut());

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
    static kAXTrustedCheckOptionPrompt: CFStringRef;
    fn CGEventTapEnable(tap: *mut c_void, enable: bool);
}

/// Parlato : the tap thread only shows the Accessibility prompt once
/// onboarding has explained it (or onboarding is already done).
static PROMPT_ALLOWED: AtomicBool = AtomicBool::new(false);

pub fn allow_accessibility_prompt() {
    PROMPT_ALLOWED.store(true, Ordering::Release);
}

/// True when Parlato has the Accessibility permission (needed for the
/// hotkey and for pasting).
pub fn accessibility_trusted() -> bool {
    unsafe { AXIsProcessTrusted() }
}

/// Shows macOS's "Parlato would like to control this computer" prompt if
/// the permission is missing. macOS shows it at most once per app identity.
pub fn request_accessibility() -> bool {
    unsafe {
        let key = CFString::wrap_under_get_rule(kAXTrustedCheckOptionPrompt);
        let options = CFDictionary::from_CFType_pairs(&[(key, CFBoolean::true_value())]);
        AXIsProcessTrustedWithOptions(options.as_concrete_TypeRef() as *const c_void)
    }
}

pub(super) fn spawn_tap_thread() {
    let _ = std::thread::Builder::new()
        .name("parla-hotkey-tap".into())
        .spawn(run_tap_thread);
}

fn run_tap_thread() {
    let mut prompted = false;
    loop {
        match CGEventTap::new(
            CGEventTapLocation::Session,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            vec![
                CGEventType::KeyDown,
                CGEventType::KeyUp,
                CGEventType::FlagsChanged,
            ],
            on_event,
        ) {
            Ok(tap) => {
                let source = tap
                    .mach_port()
                    .create_runloop_source(0)
                    .expect("run loop source for the hotkey tap");
                TAP_PORT.store(
                    tap.mach_port().as_concrete_TypeRef() as *mut c_void,
                    Ordering::Release,
                );
                CFRunLoop::get_current().add_source(&source, unsafe { kCFRunLoopCommonModes });
                tap.enable();
                info!("macOS hotkey tap installed");
                CFRunLoop::run_current();
                warn!("macOS hotkey tap run loop exited");
                return;
            }
            Err(()) => {
                if !prompted && PROMPT_ALLOWED.load(Ordering::Acquire) {
                    warn!("macOS hotkey tap refused: Accessibility permission missing");
                    request_accessibility();
                    prompted = true;
                }
                std::thread::sleep(Duration::from_secs(2));
            }
        }
    }
}

fn on_event(_proxy: CGEventTapProxy, etype: CGEventType, event: &CGEvent) -> CallbackResult {
    let Some(ctx) = HOOK_CONTEXT.get() else {
        return CallbackResult::Keep;
    };
    if event.get_integer_value_field(EventField::EVENT_SOURCE_USER_DATA) == PARLATO_EVENT_TAG {
        return CallbackResult::Keep;
    }
    if matches!(
        etype,
        CGEventType::KeyDown | CGEventType::KeyUp | CGEventType::FlagsChanged
    ) {
        release_stale_modifiers(ctx, event.get_flags().bits());
    }
    match etype {
        CGEventType::TapDisabledByTimeout | CGEventType::TapDisabledByUserInput => {
            let port = TAP_PORT.load(Ordering::Acquire);
            if !port.is_null() {
                unsafe { CGEventTapEnable(port, true) };
                debug!("macOS hotkey tap re-enabled");
            }
            CallbackResult::Keep
        }
        CGEventType::KeyDown | CGEventType::KeyUp => {
            let code = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
            let Some(vk) = mac_keycode_to_vk(code) else {
                return CallbackResult::Keep;
            };
            let is_down = matches!(etype, CGEventType::KeyDown);
            let consumed = handle_key(ctx, vk, is_down);
            // A character typed while a modifier-only dictation key is held
            // (Option+e = e acute, Option+2 = @ on some layouts) is typing,
            // not dictation. Escape keeps its double-tap cancel meaning.
            if is_down && !consumed && vk != 0x1B {
                cancel_modifier_chord(ctx);
            }
            if consumed {
                CallbackResult::Drop
            } else {
                CallbackResult::Keep
            }
        }
        CGEventType::FlagsChanged => {
            let code = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;
            let Some((vk, mask)) = modifier_for_keycode(code) else {
                return CallbackResult::Keep;
            };
            let is_down = event.get_flags().bits() & mask != 0;
            // Modifier events are never swallowed, other apps must keep
            // seeing Shift / Option / Command state.
            let _ = handle_key(ctx, vk, is_down);
            CallbackResult::Keep
        }
        _ => CallbackResult::Keep,
    }
}

/// If a modifier-only trigger is held when another key goes down, drop that
/// press: cancel the recording it started and ignore its release.
fn cancel_modifier_chord(ctx: &HookContext) {
    let mut watched = ctx.watched.lock();
    let mut cancel = false;
    if watched.primary_pressed && matches!(watched.primary, HotkeyTrigger::Modifier { .. }) {
        watched.primary_pressed = false;
        cancel = true;
    }
    if watched.secondary_pressed && matches!(watched.secondary, HotkeyTrigger::Modifier { .. }) {
        watched.secondary_pressed = false;
        cancel = true;
    }
    drop(watched);
    if cancel {
        debug!("modifier-only hotkey used as a chord, dictation press dropped");
        let _ = ctx.tx.send(HotkeyEvent::CancelRequested {
            timestamp: std::time::Instant::now(),
        });
    }
}

const ALL_MODIFIERS: [(u32, u64); 8] = [
    (0xA2, NX_DEVICELCTLKEYMASK),
    (0xA3, NX_DEVICERCTLKEYMASK),
    (0xA4, NX_DEVICELALTKEYMASK),
    (0xA5, NX_DEVICERALTKEYMASK),
    (0xA0, NX_DEVICELSHIFTKEYMASK),
    (0xA1, NX_DEVICERSHIFTKEYMASK),
    (0x5B, NX_DEVICELCMDKEYMASK),
    (0x5C, NX_DEVICERCMDKEYMASK),
];

/// Every event carries the real modifier state. If a modifier is "down" in
/// our table but up on the keyboard (a FlagsChanged was missed while the tap
/// was disabled), release it so combos do not stay broken.
fn release_stale_modifiers(ctx: &HookContext, flags: u64) {
    let device_bits = ALL_MODIFIERS.iter().fold(0, |acc, (_, m)| acc | m);
    // Some virtual keyboards never set the device-dependent bits: then the
    // table cannot be checked, leave it alone.
    const ANY_MODIFIER: u64 = 0x00020000 | 0x00040000 | 0x00080000 | 0x00100000;
    if flags & device_bits == 0 && flags & ANY_MODIFIER != 0 {
        return;
    }
    let stale: Vec<u32> = {
        let watched = ctx.watched.lock();
        ALL_MODIFIERS
            .iter()
            .filter(|(vk, mask)| watched.key_state[*vk as usize] && flags & mask == 0)
            .map(|(vk, _)| *vk)
            .collect()
    };
    for vk in stale {
        let _ = handle_key(ctx, vk, false);
    }
}

/// Modifier key code -> (Windows VK, device-dependent flag bit).
fn modifier_for_keycode(code: u16) -> Option<(u32, u64)> {
    Some(match code {
        59 => (0xA2, NX_DEVICELCTLKEYMASK), // Left Control  -> VK_LCONTROL
        62 => (0xA3, NX_DEVICERCTLKEYMASK), // Right Control -> VK_RCONTROL
        58 => (0xA4, NX_DEVICELALTKEYMASK), // Left Option   -> VK_LMENU
        61 => (0xA5, NX_DEVICERALTKEYMASK), // Right Option  -> VK_RMENU
        56 => (0xA0, NX_DEVICELSHIFTKEYMASK), // Left Shift    -> VK_LSHIFT
        60 => (0xA1, NX_DEVICERSHIFTKEYMASK), // Right Shift   -> VK_RSHIFT
        55 => (0x5B, NX_DEVICELCMDKEYMASK), // Left Command  -> VK_LWIN
        54 => (0x5C, NX_DEVICERCMDKEYMASK), // Right Command -> VK_RWIN
        _ => return None,
    })
}

/// macOS virtual key code (kVK_*, Carbon Events.h) -> Windows VK code, for
/// the non-modifier keys a hotkey can use. Positions follow the ANSI layout,
/// the same way Windows VK letters follow the key, not the character.
fn mac_keycode_to_vk(code: u16) -> Option<u32> {
    Some(match code {
        // Letters
        0 => 0x41,  // A
        11 => 0x42, // B
        8 => 0x43,  // C
        2 => 0x44,  // D
        14 => 0x45, // E
        3 => 0x46,  // F
        5 => 0x47,  // G
        4 => 0x48,  // H
        34 => 0x49, // I
        38 => 0x4A, // J
        40 => 0x4B, // K
        37 => 0x4C, // L
        46 => 0x4D, // M
        45 => 0x4E, // N
        31 => 0x4F, // O
        35 => 0x50, // P
        12 => 0x51, // Q
        15 => 0x52, // R
        1 => 0x53,  // S
        17 => 0x54, // T
        32 => 0x55, // U
        9 => 0x56,  // V
        13 => 0x57, // W
        7 => 0x58,  // X
        16 => 0x59, // Y
        6 => 0x5A,  // Z
        // Top-row digits
        29 => 0x30,
        18 => 0x31,
        19 => 0x32,
        20 => 0x33,
        21 => 0x34,
        23 => 0x35,
        22 => 0x36,
        26 => 0x37,
        28 => 0x38,
        25 => 0x39,
        // Punctuation (VK_OEM_*)
        41 => 0xBA, // ;
        24 => 0xBB, // =
        43 => 0xBC, // ,
        27 => 0xBD, // -
        47 => 0xBE, // .
        44 => 0xBF, // /
        50 => 0xC0, // `
        33 => 0xDB, // [
        42 => 0xDC, // backslash
        30 => 0xDD, // ]
        39 => 0xDE, // '
        // Editing and navigation
        36 | 76 => 0x0D, // Return, keypad Enter
        48 => 0x09,      // Tab
        49 => 0x20,      // Space
        51 => 0x08,      // Delete (backspace)
        117 => 0x2E,     // Forward delete
        53 => 0x1B,      // Escape
        114 => 0x2D,     // Help (Insert position on full keyboards)
        115 => 0x24,     // Home
        119 => 0x23,     // End
        116 => 0x21,     // Page Up
        121 => 0x22,     // Page Down
        123 => 0x25,     // Left
        126 => 0x26,     // Up
        124 => 0x27,     // Right
        125 => 0x28,     // Down
        // Function keys
        122 => 0x70, // F1
        120 => 0x71, // F2
        99 => 0x72,  // F3
        118 => 0x73, // F4
        96 => 0x74,  // F5
        97 => 0x75,  // F6
        98 => 0x76,  // F7
        100 => 0x77, // F8
        101 => 0x78, // F9
        109 => 0x79, // F10
        103 => 0x7A, // F11
        111 => 0x7B, // F12
        105 => 0x7C, // F13
        107 => 0x7D, // F14
        113 => 0x7E, // F15
        106 => 0x7F, // F16
        64 => 0x80,  // F17
        79 => 0x81,  // F18
        80 => 0x82,  // F19
        90 => 0x83,  // F20
        // Keypad operators, Clear, ISO section key
        67 => 0x6A,  // keypad *
        69 => 0x6B,  // keypad +
        78 => 0x6D,  // keypad -
        65 => 0x6E,  // keypad .
        75 => 0x6F,  // keypad /
        71 => 0x0C,  // Clear
        10 => 0xE2,  // ISO section key (VK_OEM_102)
        // Keypad digits
        82 => 0x60,
        83 => 0x61,
        84 => 0x62,
        85 => 0x63,
        86 => 0x64,
        87 => 0x65,
        88 => 0x66,
        89 => 0x67,
        91 => 0x68,
        92 => 0x69,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::{mac_keycode_to_vk, modifier_for_keycode};

    #[test]
    fn maps_letters_digits_and_escape() {
        assert_eq!(mac_keycode_to_vk(0), Some(0x41)); // A
        assert_eq!(mac_keycode_to_vk(9), Some(0x56)); // V
        assert_eq!(mac_keycode_to_vk(29), Some(0x30)); // 0
        assert_eq!(mac_keycode_to_vk(53), Some(0x1B)); // Esc
        assert_eq!(mac_keycode_to_vk(105), Some(0x7C)); // F13
    }

    #[test]
    fn maps_right_option_and_command() {
        assert_eq!(modifier_for_keycode(61).map(|m| m.0), Some(0xA5)); // Right Option
        assert_eq!(modifier_for_keycode(54).map(|m| m.0), Some(0x5C)); // Right Command
        assert_eq!(modifier_for_keycode(63), None); // Fn: not supported yet
    }

    #[test]
    fn every_letter_is_mapped_once() {
        let mut letters: Vec<u32> = (0u16..128)
            .filter_map(mac_keycode_to_vk)
            .filter(|vk| (0x41..=0x5A).contains(vk))
            .collect();
        letters.sort();
        letters.dedup();
        assert_eq!(letters.len(), 26);
    }
}
