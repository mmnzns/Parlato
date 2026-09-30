// Installed apps list on macOS. Phase 0 placeholder (see "Mac port plan" in
// CLAUDE.md): returns no apps, so the "Add app" picker only offers typing a
// name.
//
// Reference VoiceInk : AppPicker lists the .app bundles in /Applications
// (NSWorkspace). Phase 4 implements it.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InstalledApp {
    /// Display name.
    pub name: String,
    /// What Power Mode matches (bundle identifier on macOS).
    pub exe_name: String,
    /// The app is running right now.
    pub running: bool,
}

pub fn list() -> Vec<InstalledApp> {
    Vec::new()
}
