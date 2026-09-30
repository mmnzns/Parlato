// Active window detection on macOS: the frontmost app via NSWorkspace, in
// the same shape as active_window.rs. Browsers are not recognised yet, so no
// URL matching on Mac (Phase 4).
//
// Reference VoiceInk : PowerMode/ActiveWindowService.swift
// (NSWorkspace.frontmostApplication + bundleIdentifier). Phase 4 implements it.

use std::path::PathBuf;

use anyhow::{anyhow, Result};

#[derive(Debug, Clone)]
pub struct ActiveWindow {
    #[allow(dead_code)]
    pub hwnd: isize,
    pub pid: u32,
    pub title: String,
    #[allow(dead_code)]
    pub exe_path: PathBuf,
    /// On macOS this will hold the app's bundle identifier.
    pub exe_name: String,
}

/// The frontmost app. `exe_name` holds its bundle identifier in lower case
/// (for example "com.apple.textedit"), the macOS equivalent of the exe name.
/// Enough for "Use everywhere else" profiles; pairing specific apps comes
/// with the installed-apps picker (Phase 4).
pub fn foreground_window() -> Result<ActiveWindow> {
    use objc2_app_kit::NSWorkspace;
    let app = NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .ok_or_else(|| anyhow!("no frontmost application"))?;
    let exe_name = app
        .bundleIdentifier()
        .map(|b| b.to_string().to_lowercase())
        .unwrap_or_default();
    let title = app.localizedName().map(|n| n.to_string()).unwrap_or_default();
    Ok(ActiveWindow {
        hwnd: 0,
        pid: app.processIdentifier().max(0) as u32,
        title,
        exe_path: PathBuf::new(),
        exe_name,
    })
}

/// Browsers are matched by bundle identifier on macOS. Empty until Phase 4.
pub const BROWSER_EXES: &[&str] = &[];

pub fn is_browser(active: &ActiveWindow) -> bool {
    BROWSER_EXES.iter().any(|e| *e == active.exe_name)
}
