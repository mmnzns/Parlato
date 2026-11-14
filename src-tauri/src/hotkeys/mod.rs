// Module hotkeys - raccourcis globaux avec support modifier-only.
//
// Reference VoiceInk :
// - VoiceInk/HotkeyManager.swift (modes hybrid/toggle/pushToTalk, seuil 500ms,
//   debounce Fn 75ms, cooldown shortcut 500ms)
// - VoiceInk/MiniRecorderShortcutManager.swift (double-ESC 1500ms pour cancel)
//
// Sur macOS VoiceInk surveille NSEvent.flagsChanged. Sur Windows on utilise
// SetWindowsHookEx(WH_KEYBOARD_LL) pour pouvoir detecter une simple pression
// de Right Alt / Right Ctrl / Fn / etc., ce que RegisterHotKey ne sait pas faire.

pub mod keyboard_hook;
pub mod manager;
pub mod reserved;
