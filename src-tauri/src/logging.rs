// Parlato : logs to the console and to a file, so a user can send a log
// when something goes wrong (a bundled app has no visible console).
//
// Location: macOS ~/Library/Logs/com.craftconceptsdigital.parlato/parlato.log,
// Windows %LOCALAPPDATA%\com.craftconceptsdigital.parlato\logs\parlato.log
// (the same folders Tauri's app_log_dir uses). Resolved by hand because the
// subscriber starts before the Tauri app exists.
//
// Size: a log over 5 MB is moved to parlato.old.log, so at most about 10 MB
// is kept. This is checked on start and while running (the writer counts the
// bytes it writes, so a long session cannot grow the file without limit).
// Logs hold lengths and timings, never dictated text.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
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

fn open_append(path: &Path) -> io::Result<File> {
    OpenOptions::new().create(true).append(true).open(path)
}

/// Moves `path` to `old` (replacing it) when it is over `cap` bytes.
fn rotate_if_over(path: &Path, old: &Path, cap: u64) {
    if fs::metadata(path).map(|m| m.len() > cap).unwrap_or(false) {
        let _ = fs::rename(path, old);
    }
}

/// Log file that rotates while the app runs: it counts the bytes written and,
/// once past `cap`, moves the file to `old` and starts a new one. The count is
/// in memory, so a write costs one addition.
struct RotatingFile {
    file: Option<File>,
    path: PathBuf,
    old: PathBuf,
    written: u64,
    cap: u64,
}

impl RotatingFile {
    fn open(path: PathBuf, old: PathBuf, cap: u64) -> io::Result<Self> {
        rotate_if_over(&path, &old, cap);
        let file = open_append(&path)?;
        let written = file.metadata().map(|m| m.len()).unwrap_or(0);
        Ok(Self {
            file: Some(file),
            path,
            old,
            written,
            cap,
        })
    }

    fn rotate(&mut self) {
        // The handle is closed first: Windows cannot rename an open file.
        self.file = None;
        let _ = fs::rename(&self.path, &self.old);
        self.file = open_append(&self.path).ok();
        self.written = 0;
    }
}

impl Write for RotatingFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let Some(file) = self.file.as_mut() else {
            // Could not reopen after a rotation: drop the line, never fail the app.
            return Ok(buf.len());
        };
        let n = file.write(buf)?;
        self.written += n as u64;
        if self.written > self.cap {
            self.rotate();
        }
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        match self.file.as_mut() {
            Some(f) => f.flush(),
            None => Ok(()),
        }
    }
}

fn open_log_file() -> Option<RotatingFile> {
    let dir = log_dir()?;
    fs::create_dir_all(&dir).ok()?;
    RotatingFile::open(dir.join("parlato.log"), dir.join("parlato.old.log"), MAX_BYTES).ok()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("parlato-log-test-{}-{name}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_log_past_the_cap_rotates_while_writing() {
        let dir = scratch("rotate");
        let (log, old) = (dir.join("parlato.log"), dir.join("parlato.old.log"));
        let mut f = RotatingFile::open(log.clone(), old.clone(), 100).unwrap();
        for _ in 0..5 {
            f.write_all(&[b'a'; 30]).unwrap();
        }
        // 4 writes = 120 bytes > 100 -> rotated; the 5th went to the new file.
        assert_eq!(fs::metadata(&old).unwrap().len(), 120);
        assert_eq!(fs::metadata(&log).unwrap().len(), 30);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_log_under_the_cap_is_left_alone() {
        let dir = scratch("under");
        let (log, old) = (dir.join("parlato.log"), dir.join("parlato.old.log"));
        let mut f = RotatingFile::open(log.clone(), old.clone(), 1000).unwrap();
        f.write_all(&[b'a'; 500]).unwrap();
        assert!(!old.exists());
        assert_eq!(fs::metadata(&log).unwrap().len(), 500);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn rotation_replaces_an_older_old_log() {
        let dir = scratch("replace");
        let (log, old) = (dir.join("parlato.log"), dir.join("parlato.old.log"));
        fs::write(&old, b"stale").unwrap();
        let mut f = RotatingFile::open(log.clone(), old.clone(), 10).unwrap();
        f.write_all(&[b'b'; 20]).unwrap();
        assert_eq!(fs::read(&old).unwrap(), vec![b'b'; 20]);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn opening_rotates_a_log_that_is_already_too_big_and_counts_existing_bytes() {
        let dir = scratch("start");
        let (log, old) = (dir.join("parlato.log"), dir.join("parlato.old.log"));
        fs::write(&log, [b'c'; 200]).unwrap();
        let f = RotatingFile::open(log.clone(), old.clone(), 100).unwrap();
        assert!(old.exists());
        assert_eq!(f.written, 0);
        drop(f);
        // A log under the cap keeps its size in the counter.
        fs::write(&log, [b'd'; 40]).unwrap();
        let f = RotatingFile::open(log, old, 100).unwrap();
        assert_eq!(f.written, 40);
        let _ = fs::remove_dir_all(&dir);
    }
}
