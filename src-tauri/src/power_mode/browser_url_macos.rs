// Address of the frontmost browser on macOS, for pairing Power modes with
// websites.
//
// Reference VoiceInk : PowerMode/BrowserURLService.swift. It asks the browser
// for the URL of the active tab with an AppleScript, one script per browser
// family (Safari: `URL of front document`; Chromium family and Arc: `URL of
// active tab of front window`). Same here, through `/usr/bin/osascript`.
//
// Why a child process and not NSAppleScript in this process: a hung browser or
// an unanswered permission prompt must never be able to stick one of our
// threads for good, and a child can be killed. macOS attributes the Apple
// events of a child process to the app that started it (the "responsible
// process"), so the Automation prompt names Parlato.
//
// Timing (the contract with the dictation start):
// - The lookup runs on its own thread. `prefetch` starts it at the very start
//   of a recording, `lookup` (called when the Power mode is chosen) waits for
//   it at most until `WAIT` after it started. So dictation is delayed by 400 ms
//   at worst, usually by nothing, and a slow lookup only means "no website
//   match this time".
// - The first lookup per browser raises macOS's Automation prompt ("Parlato
//   wants to control Safari"). The script blocks until the user answers, far
//   longer than `WAIT`, so that dictation falls back to the app match and the
//   next one has the answer. The child is killed after `PROCESS_LIMIT`.
// - Only configurations that have a website trigger cause a lookup at all, so
//   nobody who does not use website pairing ever sees the prompt.
//
// Privacy: only the HOST leaves this module (for matching and logs). The path
// and query of the address (tokens, document ids) are dropped right here.
//
// Firefox (and Zen, which is Firefox-based) have no AppleScript support for
// tabs. They are read through the Accessibility API instead (see
// browser_ax_macos.rs): Parlato already holds that permission for the hotkey
// and for pasting, so there is no extra prompt and no Automation entry. Same
// thread, same `WAIT`, same fall-back to the app match.

use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::{Condvar, Mutex};
use tauri::AppHandle;
use tracing::{info, warn};

use super::active_window::ActiveWindow;
use super::config::PowerModeConfig;

/// Longest dictation start waits for the address, counted from the moment the
/// lookup started.
const WAIT: Duration = Duration::from_millis(400);
/// A lookup that started this recently is reused instead of starting another.
const REUSE_WINDOW: Duration = Duration::from_secs(3);
/// The script is killed after this, for example when the Automation prompt is
/// ignored.
const PROCESS_LIMIT: Duration = Duration::from_secs(60);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Script {
    /// `URL of front document` (Safari, Safari Technology Preview).
    Safari,
    /// `URL of active tab of front window` (Chromium family, Arc).
    Chromium,
    /// Accessibility API: the address bar of the focused window (Firefox
    /// family). No AppleScript, no Automation permission.
    Accessibility,
}

struct Browser {
    /// Bundle identifier, as macOS spells it.
    bundle: &'static str,
    /// Name shown in the Power modes hint.
    name: &'static str,
    script: Script,
}

const fn b(bundle: &'static str, name: &'static str, script: Script) -> Browser {
    Browser { bundle, name, script }
}

const BROWSERS: &[Browser] = &[
    b("com.apple.Safari", "Safari", Script::Safari),
    b("com.apple.SafariTechnologyPreview", "Safari Technology Preview", Script::Safari),
    b("com.google.Chrome", "Google Chrome", Script::Chromium),
    b("com.google.Chrome.beta", "Google Chrome Beta", Script::Chromium),
    b("com.google.Chrome.dev", "Google Chrome Dev", Script::Chromium),
    b("com.google.Chrome.canary", "Google Chrome Canary", Script::Chromium),
    b("company.thebrowser.Browser", "Arc", Script::Chromium),
    b("com.brave.Browser", "Brave", Script::Chromium),
    b("com.brave.Browser.beta", "Brave Beta", Script::Chromium),
    b("com.brave.Browser.nightly", "Brave Nightly", Script::Chromium),
    b("com.microsoft.edgemac", "Microsoft Edge", Script::Chromium),
    b("com.microsoft.edgemac.Beta", "Microsoft Edge Beta", Script::Chromium),
    b("com.microsoft.edgemac.Dev", "Microsoft Edge Dev", Script::Chromium),
    b("com.microsoft.edgemac.Canary", "Microsoft Edge Canary", Script::Chromium),
    b("com.vivaldi.Vivaldi", "Vivaldi", Script::Chromium),
    b("com.operasoftware.Opera", "Opera", Script::Chromium),
    b("com.operasoftware.OperaGX", "Opera GX", Script::Chromium),
    b("org.chromium.Chromium", "Chromium", Script::Chromium),
    b("org.mozilla.firefox", "Firefox", Script::Accessibility),
    b("org.mozilla.firefoxdeveloperedition", "Firefox Developer Edition", Script::Accessibility),
    b("org.mozilla.nightly", "Firefox Nightly", Script::Accessibility),
    b("app.zen-browser.zen", "Zen", Script::Accessibility),
];

fn browser_for(bundle_lower: &str) -> Option<&'static Browser> {
    BROWSERS
        .iter()
        .find(|b| b.bundle.eq_ignore_ascii_case(bundle_lower))
}

/// Is this bundle identifier (any case) a browser whose address can be read?
pub fn is_supported(bundle: &str) -> bool {
    browser_for(bundle).is_some()
}

fn script_source(browser: &Browser) -> String {
    let what = match browser.script {
        Script::Safari => "URL of front document",
        Script::Chromium => "URL of active tab of front window",
        // Never run through osascript; see `start`.
        Script::Accessibility => return String::new(),
    };
    // `application id` names the app by bundle identifier, so a localized or
    // renamed app still resolves, and it never launches a different app.
    format!("tell application id \"{}\" to return {}", browser.bundle, what)
}

/// The host of a page address, lower case, without scheme, credentials, port,
/// path or query. `None` for addresses with no host (about:blank, file://,
/// chrome://newtab, an empty tab).
pub(crate) fn host_of(url: &str) -> Option<String> {
    let url = url.trim();
    let (scheme, rest) = url.split_once("://")?;
    if !(scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")) {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit('@').next().unwrap_or("");
    let host = if host_port.starts_with('[') {
        // IPv6 literal: keep the brackets, drop the port after them.
        host_port.split_once(']').map(|(h, _)| format!("{h}]"))?
    } else {
        host_port.split(':').next().unwrap_or("").to_string()
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Outcome {
    Host(String),
    /// The browser answered but has no page with an address (new tab, no window).
    NoPage,
    /// macOS Automation says no (error -1743).
    NotAuthorized,
    Failed,
}

fn classify(success: bool, stdout: &str, stderr: &str) -> Outcome {
    if success {
        return match host_of(stdout) {
            Some(h) => Outcome::Host(h),
            None => Outcome::NoPage,
        };
    }
    if stderr.contains("-1743") || stderr.to_ascii_lowercase().contains("not authorized") {
        return Outcome::NotAuthorized;
    }
    Outcome::Failed
}

/// True when some enabled Power mode is paired with a website.
pub fn any_site_triggers(configs: &[PowerModeConfig]) -> bool {
    configs
        .iter()
        .any(|c| c.is_enabled && c.url_triggers.iter().any(|t| !t.url.trim().is_empty()))
}

// -- State -------------------------------------------------------------------

struct Shared {
    done: Mutex<Option<Outcome>>,
    cv: Condvar,
}

struct InFlight {
    bundle: &'static str,
    started: Instant,
    shared: Arc<Shared>,
}

static CURRENT: Mutex<Option<InFlight>> = Mutex::new(None);
/// A script is running (maybe stuck on the permission prompt): do not pile up
/// more of them.
static BUSY: AtomicBool = AtomicBool::new(false);
/// The browser macOS last refused Automation for, until a lookup succeeds.
static DENIED: Mutex<Option<&'static str>> = Mutex::new(None);

/// Name of the browser Parlato was refused Automation access to, if the last
/// lookup was refused. Drives the hint in the Power modes editor.
pub fn denied_browser() -> Option<&'static str> {
    *DENIED.lock()
}

fn record_status(browser: &'static Browser, outcome: &Outcome) {
    match outcome {
        Outcome::NotAuthorized => {
            *DENIED.lock() = Some(browser.name);
            warn!(browser = browser.name, "Browser address: Automation not allowed (-1743)");
        }
        Outcome::Host(_) | Outcome::NoPage => {
            let mut d = DENIED.lock();
            if d.is_some() {
                *d = None;
            }
        }
        Outcome::Failed => {}
    }
}

// -- Lookup ------------------------------------------------------------------

/// Start (or reuse) a lookup for `browser`. `None` when one is already stuck.
fn start(browser: &'static Browser, pid: u32) -> Option<(Instant, Arc<Shared>)> {
    let mut cur = CURRENT.lock();
    if let Some(c) = cur.as_ref() {
        if c.bundle == browser.bundle && c.started.elapsed() < REUSE_WINDOW {
            return Some((c.started, c.shared.clone()));
        }
    }
    if BUSY.swap(true, Ordering::SeqCst) {
        return None;
    }
    let shared = Arc::new(Shared {
        done: Mutex::new(None),
        cv: Condvar::new(),
    });
    let started = Instant::now();
    *cur = Some(InFlight {
        bundle: browser.bundle,
        started,
        shared: shared.clone(),
    });
    let worker_shared = shared.clone();
    let spawned = std::thread::Builder::new()
        .name("parlato-browser-url".into())
        .spawn(move || {
            let outcome = match browser.script {
                Script::Accessibility => match super::browser_ax_macos::read_displayed_address(pid)
                    .as_deref()
                    .and_then(super::browser_ax_macos::host_of_displayed)
                {
                    Some(h) => Outcome::Host(h),
                    None => Outcome::Failed,
                },
                _ => run_osascript(browser),
            };
            // The Automation hint is about AppleScript browsers only.
            if browser.script != Script::Accessibility {
                record_status(browser, &outcome);
            }
            if let Outcome::Host(host) = &outcome {
                // The host only, never the path or query.
                info!(browser = browser.name, host = %host, "Browser address read");
            }
            *worker_shared.done.lock() = Some(outcome);
            worker_shared.cv.notify_all();
            BUSY.store(false, Ordering::SeqCst);
        });
    if spawned.is_err() {
        BUSY.store(false, Ordering::SeqCst);
        *cur = None;
        return None;
    }
    Some((started, shared))
}

fn run_osascript(browser: &Browser) -> Outcome {
    let mut child = match Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(script_source(browser))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => {
            warn!("osascript: {e}");
            return Outcome::Failed;
        }
    };
    let deadline = Instant::now() + PROCESS_LIMIT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Outcome::Failed;
                }
                std::thread::sleep(Duration::from_millis(15));
            }
            Err(_) => return Outcome::Failed,
        }
    };
    let mut out = String::new();
    let mut err = String::new();
    if let Some(mut s) = child.stdout.take() {
        let _ = s.read_to_string(&mut out);
    }
    if let Some(mut s) = child.stderr.take() {
        let _ = s.read_to_string(&mut err);
    }
    classify(status.success(), &out, &err)
}

/// Called at the very start of a recording: if some Power mode is paired with
/// a website and the frontmost app is a supported browser, start reading its
/// address now, in parallel with the rest of the start-up.
pub fn prefetch(app: &AppHandle) {
    let configs = super::config::load_all(app).unwrap_or_default();
    if !any_site_triggers(&configs) {
        return;
    }
    if let Ok(active) = super::active_window::foreground_window() {
        if let Some(browser) = browser_for(&active.exe_name) {
            let _ = start(browser, active.pid);
        }
    }
}

/// The host of the page open in the frontmost browser, waiting at most `WAIT`
/// (counted from when the lookup started) for the browser to answer.
pub fn lookup(active: &ActiveWindow) -> Option<String> {
    let browser = browser_for(&active.exe_name)?;
    let (started, shared) = start(browser, active.pid)?;
    let deadline = (started + WAIT).max(Instant::now() + Duration::from_millis(100));
    let mut done = shared.done.lock();
    while done.is_none() {
        if shared.cv.wait_until(&mut done, deadline).timed_out() {
            break;
        }
    }
    match done.clone() {
        Some(Outcome::Host(h)) => Some(h),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_only_from_addresses() {
        assert_eq!(host_of("https://Mail.Google.com/mail/u/0/#inbox").as_deref(), Some("mail.google.com"));
        assert_eq!(host_of("http://user:pw@example.com:8080/a?b=c").as_deref(), Some("example.com"));
        assert_eq!(host_of("https://example.com.").as_deref(), Some("example.com"));
        assert_eq!(host_of("  https://github.com\n").as_deref(), Some("github.com"));
        assert_eq!(host_of("https://[::1]:3000/x").as_deref(), Some("[::1]"));
    }

    #[test]
    fn no_host_for_internal_pages() {
        assert_eq!(host_of(""), None);
        assert_eq!(host_of("missing value"), None);
        assert_eq!(host_of("about:blank"), None);
        assert_eq!(host_of("chrome://newtab/"), None);
        assert_eq!(host_of("favorites://"), None);
        assert_eq!(host_of("file:///Users/me/doc.pdf"), None);
        assert_eq!(host_of("https:///path"), None);
    }

    #[test]
    fn supported_browsers_by_bundle_id_any_case() {
        for id in [
            "com.apple.safari",
            "com.apple.safaritechnologypreview",
            "com.google.chrome",
            "com.google.chrome.canary",
            "company.thebrowser.browser",
            "com.brave.browser",
            "com.microsoft.edgemac",
            "com.vivaldi.vivaldi",
            "com.operasoftware.opera",
            "org.chromium.chromium",
            "org.mozilla.firefox",
            "org.mozilla.firefoxdeveloperedition",
            "org.mozilla.nightly",
            "app.zen-browser.zen",
        ] {
            assert!(is_supported(id), "{id}");
        }
        assert!(!is_supported("org.mozilla.fennec"));
        assert!(!is_supported("com.apple.textedit"));
        assert!(!is_supported(""));
    }

    #[test]
    fn firefox_family_is_read_through_accessibility() {
        for id in ["org.mozilla.firefox", "org.mozilla.firefoxdeveloperedition", "org.mozilla.nightly", "app.zen-browser.zen"] {
            assert_eq!(browser_for(id).unwrap().script, Script::Accessibility, "{id}");
        }
        assert_eq!(browser_for("com.apple.safari").unwrap().script, Script::Safari);
        // Every table entry is unique and lower-case matchable.
        let mut seen = std::collections::HashSet::new();
        for b in BROWSERS {
            assert!(seen.insert(b.bundle.to_ascii_lowercase()), "{}", b.bundle);
        }
    }

    #[test]
    fn scripts_name_the_app_by_bundle_id() {
        let safari = browser_for("com.apple.safari").unwrap();
        assert_eq!(
            script_source(safari),
            "tell application id \"com.apple.Safari\" to return URL of front document"
        );
        let arc = browser_for("company.thebrowser.browser").unwrap();
        assert_eq!(
            script_source(arc),
            "tell application id \"company.thebrowser.Browser\" to return URL of active tab of front window"
        );
    }

    #[test]
    fn classifies_osascript_results() {
        assert_eq!(
            classify(true, "https://github.com/foo/bar?x=1\n", ""),
            Outcome::Host("github.com".into())
        );
        assert_eq!(classify(true, "missing value\n", ""), Outcome::NoPage);
        assert_eq!(
            classify(false, "", "execution error: Not authorized to send Apple events to Safari. (-1743)"),
            Outcome::NotAuthorized
        );
        assert_eq!(
            classify(false, "", "execution error: Safari got an error: Can\u{2019}t get document 1. (-1728)"),
            Outcome::Failed
        );
    }

    #[test]
    fn lookups_only_when_a_site_is_paired() {
        use crate::power_mode::config::UrlTrigger;
        let mut c = PowerModeConfig::new("a".into(), "".into());
        assert!(!any_site_triggers(&[c.clone()]));
        c.url_triggers = vec![UrlTrigger { id: "t".into(), url: "github.com".into() }];
        assert!(any_site_triggers(&[c.clone()]));
        c.is_enabled = false;
        assert!(!any_site_triggers(&[c]));
    }
}
