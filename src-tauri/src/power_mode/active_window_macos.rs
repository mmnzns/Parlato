// Active window detection on macOS. Phase 0 placeholder (see "Mac port plan"
// in CLAUDE.md): same shape as active_window.rs so the rest of Power Mode
// compiles, but it never detects an app, so no Power Mode ever activates.
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

pub fn foreground_window() -> Result<ActiveWindow> {
    Err(anyhow!("active window detection is not implemented on macOS yet"))
}

/// Browsers are matched by bundle identifier on macOS. Empty until Phase 4.
pub const BROWSER_EXES: &[&str] = &[];

pub fn is_browser(active: &ActiveWindow) -> bool {
    BROWSER_EXES.iter().any(|e| *e == active.exe_name)
}
