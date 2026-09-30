// Module Power Mode : configurations contextuelles qui switch prompts,
// providers et source de transcription selon l'application (ou l'URL)
// active au moment de l'enregistrement.
//
// Reference VoiceInk : VoiceInk/PowerMode/*.

pub mod active_window;
pub mod browser_url;
pub mod config;
pub mod installed_apps;
pub mod matcher;
pub mod session;
