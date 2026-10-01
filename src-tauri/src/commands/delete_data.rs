// Parlato : "Delete all Parlato data" (Settings). Removes everything Parlato
// stored on this computer, so removing the app leaves nothing behind:
// downloaded models, history and recordings, settings, logs, caches, start
// at login and saved account keys (plus, on macOS, its privacy permissions).
//
// Folders are deleted by a small helper that waits for Parlato to exit:
// Windows cannot delete files that are still open (loaded models, the
// history database, the log file), and deleting after exit also stops
// Parlato from recreating a file on its way out. Safety: only folders named
// exactly after Parlato's identifier are ever passed to the helper, as
// separate arguments (never pasted into a shell string).

use std::path::PathBuf;

use tauri::{command, AppHandle, Manager};
use tracing::{info, warn};

const IDENTIFIER: &str = "com.craftconceptsdigital.parlato";

/// Every folder Parlato writes to, deduplicated. On Windows the log and
/// web view folders live inside the local data folder.
fn data_folders(app: &AppHandle) -> Vec<PathBuf> {
    let paths = app.path();
    let mut dirs: Vec<PathBuf> = [
        paths.app_config_dir(),
        paths.app_data_dir(),
        paths.app_local_data_dir(),
        paths.app_cache_dir(),
        paths.app_log_dir(),
    ]
    .into_iter()
    .flatten()
    .collect();
    // WKWebView keeps the window's storage in ~/Library/WebKit/<identifier>.
    #[cfg(target_os = "macos")]
    if let Some(home) = std::env::var_os("HOME") {
        dirs.push(PathBuf::from(home).join("Library/WebKit").join(IDENTIFIER));
    }
    dirs.retain(|d| is_parlato_folder(d));
    dirs.sort();
    dirs.dedup();
    dirs
}

/// Guard: an absolute path whose last component is Parlato's identifier.
fn is_parlato_folder(path: &std::path::Path) -> bool {
    path.is_absolute()
        && path.file_name().and_then(|n| n.to_str()) == Some(IDENTIFIER)
        && path.components().count() > 3
}

#[command]
pub fn delete_all_data(app: AppHandle) -> Result<(), String> {
    info!("Delete all Parlato data: requested");

    // 1. Start at login.
    {
        use tauri_plugin_autostart::ManagerExt;
        if let Err(e) = app.autolaunch().disable() {
            warn!("delete data: autostart disable: {e}");
        }
    }

    // 2. Saved account keys.
    for provider in crate::services::api_keys::PROVIDERS {
        if let Err(e) = crate::services::api_keys::delete_api_key(provider) {
            warn!("delete data: key {provider}: {e}");
        }
    }

    // 3. macOS privacy permissions (Accessibility, Microphone...), so the
    // System Settings lists no longer show Parlato. No admin rights needed.
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("/usr/bin/tccutil")
            .args(["reset", "All", IDENTIFIER])
            .status();
    }

    // 4. Folders, after Parlato has exited.
    let folders = data_folders(&app);
    info!(count = folders.len(), "Delete all Parlato data: removing folders after exit");
    spawn_remover(&folders).map_err(|e| e.to_string())?;

    app.exit(0);
    Ok(())
}

#[cfg(unix)]
fn spawn_remover(folders: &[PathBuf]) -> std::io::Result<()> {
    use std::process::{Command, Stdio};
    let pid = std::process::id().to_string();
    // $1 = Parlato's pid, then the folders. Wait for the pid to be gone.
    Command::new("/bin/sh")
        .arg("-c")
        .arg(r#"pid="$1"; shift; while kill -0 "$pid" 2>/dev/null; do sleep 0.2; done; rm -rf -- "$@""#)
        .arg("sh")
        .arg(pid)
        .args(folders)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

#[cfg(windows)]
fn spawn_remover(folders: &[PathBuf]) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    // PowerShell single-quoted literals: a quote inside is doubled.
    let list = folders
        .iter()
        .map(|f| format!("'{}'", f.to_string_lossy().replace('\'', "''")))
        .collect::<Vec<_>>()
        .join(",");
    // Retry for up to 10 s: WebView2 helper processes can hold files in the
    // local data folder for a moment after Parlato itself has exited.
    let script = format!(
        "$p = @({list}); \
         Wait-Process -Id {pid} -Timeout 60 -ErrorAction SilentlyContinue; \
         for ($i = 0; $i -lt 10; $i++) {{ \
           Start-Sleep -Milliseconds 1000; \
           $left = @($p | Where-Object {{ Test-Path -LiteralPath $_ }}); \
           if ($left.Count -eq 0) {{ break }}; \
           Remove-Item -LiteralPath $left -Recurse -Force -ErrorAction SilentlyContinue \
         }}",
        pid = std::process::id()
    );
    Command::new("powershell.exe")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW | DETACHED_PROCESS)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::is_parlato_folder;
    use std::path::Path;

    #[cfg(unix)]
    #[test]
    fn only_parlato_folders_pass_the_guard() {
        assert!(is_parlato_folder(Path::new(
            "/Users/a/Library/Application Support/com.craftconceptsdigital.parlato"
        )));
        assert!(!is_parlato_folder(Path::new("/Users/a/Library/Application Support")));
        assert!(!is_parlato_folder(Path::new("/com.craftconceptsdigital.parlato")));
        assert!(!is_parlato_folder(Path::new("com.craftconceptsdigital.parlato")));
        assert!(!is_parlato_folder(Path::new("/Users/a/Library/Logs/other.app")));
    }

    #[cfg(windows)]
    #[test]
    fn only_parlato_folders_pass_the_guard() {
        assert!(is_parlato_folder(Path::new(
            r"C:\Users\a\AppData\Roaming\com.craftconceptsdigital.parlato"
        )));
        assert!(is_parlato_folder(Path::new(
            r"C:\Users\a\AppData\Local\com.craftconceptsdigital.parlato"
        )));
        assert!(!is_parlato_folder(Path::new(r"C:\Users\a\AppData\Local")));
        assert!(!is_parlato_folder(Path::new(r"C:\com.craftconceptsdigital.parlato")));
        assert!(!is_parlato_folder(Path::new("com.craftconceptsdigital.parlato")));
    }
}
