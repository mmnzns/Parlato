// Parlato 0.16 : shortcuts the operating system already uses.
//
// A dictation key is not swallowed and the other shortcuts are, so a
// shortcut the system or every app relies on (Win+L, Ctrl+C, Cmd+Space...)
// would either break that function or start a dictation every time it is
// used. `reserved_id` names the function; `commands::hotkey::validate` turns
// the id into `PARLA_ERR:shortcutSystem:<action>:<id>`, and the UI shows the
// `errors.systemShortcut.<id>` sentence ("Win+L already locks your computer").
//
// The lists are deliberately short: only shortcuts documented by Microsoft
// and Apple that everybody has. `win` means the Windows key on Windows and
// the Command key on the Mac, `alt` the Alt / Option key. The match is on the
// exact modifier set (Ctrl+C is reserved, Ctrl+Shift+C is not).

/// Windows: (vk, ctrl, alt, shift, win, id).
const WINDOWS: &[(u32, bool, bool, bool, bool, &str)] = &[
    (0x4C, false, false, false, true, "winL"),      // Win+L lock
    (0x44, false, false, false, true, "winD"),      // Win+D show desktop
    (0x45, false, false, false, true, "winE"),      // Win+E File Explorer
    (0x52, false, false, false, true, "winR"),      // Win+R Run
    (0x49, false, false, false, true, "winI"),      // Win+I Settings
    (0x56, false, false, false, true, "winV"),      // Win+V clipboard history
    (0xBE, false, false, false, true, "winPeriod"), // Win+. emoji panel
    (0x09, false, false, false, true, "winTab"),    // Win+Tab Task View
    (0x53, false, false, true, true, "winShiftS"),  // Win+Shift+S screenshot
    (0x09, false, true, false, false, "altTab"),    // Alt+Tab
    (0x73, false, true, false, false, "altF4"),     // Alt+F4
    (0x2E, true, true, false, false, "ctrlAltDel"), // Ctrl+Alt+Del
    (0x1B, true, false, true, false, "ctrlShiftEsc"), // Ctrl+Shift+Esc
    (0x43, true, false, false, false, "ctrlC"),
    (0x56, true, false, false, false, "ctrlV"),
    (0x58, true, false, false, false, "ctrlX"),
    (0x5A, true, false, false, false, "ctrlZ"),
    (0x59, true, false, false, false, "ctrlY"),
    (0x41, true, false, false, false, "ctrlA"),
    (0x53, true, false, false, false, "ctrlS"),
];

/// Mac: (vk, ctrl, alt, shift, win = Command, id).
const MAC: &[(u32, bool, bool, bool, bool, &str)] = &[
    (0x20, false, false, false, true, "cmdSpace"),   // Spotlight
    (0x09, false, false, false, true, "cmdTab"),     // app switcher
    (0x33, false, false, true, true, "cmdShift3"),   // screenshot
    (0x34, false, false, true, true, "cmdShift4"),   // screenshot selection
    (0x35, false, false, true, true, "cmdShift5"),   // screenshot toolbar
    (0x51, true, false, false, true, "ctrlCmdQ"),    // lock screen
    (0x1B, false, true, false, true, "cmdOptEsc"),   // Force Quit
];

fn find(
    table: &'static [(u32, bool, bool, bool, bool, &'static str)],
    vk: u32,
    ctrl: bool,
    alt: bool,
    shift: bool,
    win: bool,
) -> Option<&'static str> {
    table
        .iter()
        .find(|(v, c, a, s, w, _)| *v == vk && *c == ctrl && *a == alt && *s == shift && *w == win)
        .map(|(.., id)| *id)
}

/// Id of the Windows function that uses this combination, if any.
pub fn reserved_windows(vk: u32, ctrl: bool, alt: bool, shift: bool, win: bool) -> Option<&'static str> {
    find(WINDOWS, vk, ctrl, alt, shift, win)
}

/// Id of the macOS function that uses this combination, if any.
pub fn reserved_mac(vk: u32, ctrl: bool, alt: bool, shift: bool, win: bool) -> Option<&'static str> {
    find(MAC, vk, ctrl, alt, shift, win)
}

/// Every reserved id (both platforms), for the locale parity tests.
#[cfg(test)]
pub fn all_ids() -> Vec<&'static str> {
    WINDOWS.iter().chain(MAC.iter()).map(|(.., id)| *id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_list_matches_exact_modifier_sets() {
        assert_eq!(reserved_windows(0x4C, false, false, false, true), Some("winL"));
        assert_eq!(reserved_windows(0x53, false, false, true, true), Some("winShiftS"));
        assert_eq!(reserved_windows(0x09, false, true, false, false), Some("altTab"));
        assert_eq!(reserved_windows(0x2E, true, true, false, false), Some("ctrlAltDel"));
        assert_eq!(reserved_windows(0x1B, true, false, true, false), Some("ctrlShiftEsc"));
        for (vk, id) in [(0x43, "ctrlC"), (0x56, "ctrlV"), (0x58, "ctrlX"), (0x5A, "ctrlZ"), (0x59, "ctrlY"), (0x41, "ctrlA"), (0x53, "ctrlS")] {
            assert_eq!(reserved_windows(vk, true, false, false, false), Some(id));
        }
        // Another modifier set is a different shortcut.
        assert_eq!(reserved_windows(0x43, true, false, true, false), None); // Ctrl+Shift+C
        assert_eq!(reserved_windows(0x4C, false, false, true, true), None); // Win+Shift+L
        assert_eq!(reserved_windows(0x52, false, false, false, false), None); // R alone
        assert_eq!(reserved_windows(0x52, true, true, false, false), None); // Ctrl+Alt+R
    }

    #[test]
    fn mac_list_matches_exact_modifier_sets() {
        assert_eq!(reserved_mac(0x20, false, false, false, true), Some("cmdSpace"));
        assert_eq!(reserved_mac(0x09, false, false, false, true), Some("cmdTab"));
        for (vk, id) in [(0x33, "cmdShift3"), (0x34, "cmdShift4"), (0x35, "cmdShift5")] {
            assert_eq!(reserved_mac(vk, false, false, true, true), Some(id));
        }
        assert_eq!(reserved_mac(0x51, true, false, false, true), Some("ctrlCmdQ"));
        assert_eq!(reserved_mac(0x1B, false, true, false, true), Some("cmdOptEsc"));
        // Control+Option+Space (the docs example) and Cmd+Option+Space are free.
        assert_eq!(reserved_mac(0x20, true, true, false, false), None);
        assert_eq!(reserved_mac(0x20, false, true, false, true), None);
        assert_eq!(reserved_mac(0x51, false, false, false, true), None); // Cmd+Q: blocked elsewhere
    }

    #[test]
    fn ids_are_unique_per_platform_and_across() {
        let mut ids = all_ids();
        let n = ids.len();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), n);
    }

    #[test]
    fn windows_shortcuts_do_not_leak_to_the_mac_and_back() {
        // Ctrl+C is a Windows entry; on the Mac Ctrl+C is not reserved here.
        assert_eq!(reserved_mac(0x43, true, false, false, false), None);
        // Cmd+Space is a Mac entry; on Windows Win+Space is not in the list.
        assert_eq!(reserved_windows(0x20, false, false, false, true), None);
    }
}
