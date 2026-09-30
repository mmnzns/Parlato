// Parlato : logs to the console and to a file, so a user can send a log
// when something goes wrong (a bundled app has no visible console).
//
// Location: macOS ~/Library/Logs/com.craftconceptsdigital.parlato/parlato.log,
// Windows %LOCALAPPDATA%\com.craftconceptsdigital.parlato\logs\parlato.log
// (the same folders Tauri's app_log_dir uses). Resolved by hand because the
// subscriber starts before the Tauri app exists.
//
// Size: on start, a log over 5 MB is moved to parlato.old.log, so at most
// about 10 MB is kept. Logs hold lengths and timings, never dictated text.

use std::fs::{self, File, OpenOptions};
use std::path::PathBuf;
use std::sync::Mutex;

use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{fmt, EnvFilter};

const IDENTIFIER: &str = "com.craftconceptsdigital.parlato";
const MAX_BYTES: u64 = 5 * 1024 * 1024;

pub fn log_dir() -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join("Library/Logs").join(IDENTIFIER))
    }
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join(IDENTIFIER).join("logs"))
    }
    #[cfg(not(any(target_os = "macos", windows)))]
    {
        None
    }
}

fn open_log_file() -> Option<File> {
    let dir = log_dir()?;
    fs::create_dir_all(&dir).ok()?;
    let path = dir.join("parlato.log");
    if fs::metadata(&path).map(|m| m.len() > MAX_BYTES).unwrap_or(false) {
        let _ = fs::rename(&path, dir.join("parlato.old.log"));
    }
    OpenOptions::new().create(true).append(true).open(path).ok()
}

pub fn init() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,parla=debug"));
    let file_layer = open_log_file()
        .map(|f| fmt::layer().with_ansi(false).with_writer(Mutex::new(f)));
    tracing_subscriber::registry()
        .with(filter)
        .with(fmt::layer())
        .with(file_layer)
        .init();
}
