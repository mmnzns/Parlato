// Module Power Mode : configurations contextuelles qui switch prompts,
// providers et source de transcription selon l'application (ou l'URL)
// active au moment de l'enregistrement.
//
// Reference VoiceInk : VoiceInk/PowerMode/*.

#[cfg_attr(target_os = "macos", path = "active_window_macos.rs")]
pub mod active_window;
pub mod browser_url;
pub mod config;
#[cfg_attr(target_os = "macos", path = "installed_apps_macos.rs")]
pub mod installed_apps;
pub mod matcher;
pub mod session;
