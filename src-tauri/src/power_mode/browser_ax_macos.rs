// Address of Firefox (and Firefox-based browsers) on macOS, read through the
// Accessibility API. Firefox has no AppleScript support for tabs, so
// browser_url_macos.rs cannot ask it. Parlato already holds the Accessibility
// permission (hotkey and paste), so this adds no prompt.
//
// Windows equivalent: browser_url.rs finds the browser's Edit control through
// UI Automation (Firefox labels it "Search with <engine> or enter address").
//
// Walk: AXUIElementCreateApplication(pid) -> AXFocusedWindow -> breadth-first
// search of the toolbar area for the address bar -> AXValue. The search is
// bounded (depth, element count, time) and never descends into web content
// (AXWebArea), which can be huge.
//
// Guards: when the address bar has keyboard focus the user is typing in it, so
// its text is not a page address: nothing is returned. Text that does not look
// like an address (spaces, no dot) is dropped too.
//
// Privacy: only the HOST leaves this module. The value is never logged.
// Nothing here writes to the browser (no attribute is set except our own
// messaging timeout on our handle).

use std::collections::VecDeque;
use std::ffi::c_void;
use std::time::{Duration, Instant};

use core_foundation::array::CFArray;
use core_foundation::base::{CFType, CFTypeRef, TCFType};
use core_foundation::boolean::CFBoolean;
use core_foundation::string::{CFString, CFStringRef};

type AXError = i32;
const AX_SUCCESS: AXError = 0;

#[link(name = "ApplicationServices", kind = "framework")]
extern "C" {
    fn AXIsProcessTrusted() -> bool;
    fn AXUIElementCreateApplication(pid: i32) -> CFTypeRef;
    fn AXUIElementCopyAttributeValue(
        element: CFTypeRef,
        attribute: CFStringRef,
        value: *mut CFTypeRef,
    ) -> AXError;
    fn AXUIElementSetMessagingTimeout(element: CFTypeRef, seconds: f32) -> AXError;
}

/// How deep below the window the search goes.
const MAX_DEPTH: usize = 9;
/// How many elements the search looks at.
const MAX_ELEMENTS: usize = 500;
/// Time budget of the whole walk (the caller waits 400 ms at most).
const BUDGET: Duration = Duration::from_millis(300);
/// Each single Accessibility call to Firefox may take this long at most.
const CALL_TIMEOUT_SECS: f32 = 0.12;

// -- Pure parts ---------------------------------------------------------------

/// The host in what Firefox shows in its address bar: with a scheme
/// (`https://example.com/x`) or without (`example.com/x`, Firefox hides
/// `https://`). `None` for search text (spaces), for internal pages
/// (`about:`, `moz-extension://`) and for anything without a dot.
pub(super) fn host_of_displayed(value: &str) -> Option<String> {
    let v = value.trim();
    if v.is_empty() || v.chars().any(char::is_whitespace) {
        return None;
    }
    if v.contains("://") {
        // The shared extractor accepts http(s) only.
        let host = super::browser_url_macos::host_of(v)?;
        return looks_like_host(&host).then_some(host);
    }
    let authority = v.split(['/', '?', '#']).next().unwrap_or("");
    let host_port = authority.rsplit('@').next().unwrap_or("");
    let host = host_port.split(':').next().unwrap_or("");
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    looks_like_host(&host).then_some(host)
}

fn looks_like_host(host: &str) -> bool {
    if host.starts_with('[') {
        return true; // IPv6 literal, from the scheme path
    }
    host.contains('.')
        && !host.starts_with('.')
        && host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.' || c == '_')
}

/// What the search needs to know about one element.
#[derive(Debug, Default, Clone)]
pub(super) struct Probe {
    pub role: String,
    pub identifier: String,
    pub description: String,
    pub placeholder: String,
    pub in_toolbar: bool,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(super) enum Match {
    No,
    /// First editable text field of the toolbar, used when nothing better turns up.
    Weak,
    /// Firefox's own address-bar id or label.
    Strong,
}

fn is_text_field(role: &str) -> bool {
    role == "AXTextField" || role == "AXComboBox"
}

pub(super) fn classify_probe(p: &Probe) -> Match {
    if !is_text_field(&p.role) {
        return Match::No;
    }
    if p.identifier == "urlbar-input" {
        return Match::Strong;
    }
    let label = format!("{} {}", p.description, p.placeholder).to_lowercase();
    // "Search with Google or enter address" and localized variants all keep
    // the word for address; the separate search bar says "Search" only.
    let address_words = [
        "address",
        "dirección",
        "direccion",
        "adresse",
        "indirizzo",
        "endereço",
        "endereco",
        "adres",
        "адрес",
        "アドレス",
        "地址",
        "주소",
    ];
    if address_words.iter().any(|w| label.contains(w)) {
        return Match::Strong;
    }
    if p.in_toolbar && !p.identifier.to_lowercase().contains("search") {
        return Match::Weak;
    }
    Match::No
}

// -- Accessibility calls ------------------------------------------------------

fn copy_attr(el: &CFType, name: &str) -> Option<CFType> {
    let attr = CFString::new(name);
    let mut out: CFTypeRef = std::ptr::null();
    let err = unsafe {
        AXUIElementCopyAttributeValue(el.as_CFTypeRef(), attr.as_concrete_TypeRef(), &mut out)
    };
    if err != AX_SUCCESS || out.is_null() {
        return None;
    }
    // Copy rule: we own `out`.
    Some(unsafe { CFType::wrap_under_create_rule(out) })
}

fn string_attr(el: &CFType, name: &str) -> String {
    copy_attr(el, name)
        .and_then(|v| v.downcast_into::<CFString>())
        .map(|s| s.to_string())
        .unwrap_or_default()
}

fn children(el: &CFType) -> Vec<CFType> {
    let Some(v) = copy_attr(el, "AXChildren") else {
        return Vec::new();
    };
    let Some(arr) = v.downcast_into::<CFArray<*const c_void>>() else {
        return Vec::new();
    };
    arr.iter()
        .map(|p| unsafe { CFType::wrap_under_get_rule(*p as CFTypeRef) })
        .collect()
}

fn is_focused(el: &CFType) -> bool {
    copy_attr(el, "AXFocused")
        .and_then(|v| v.downcast_into::<CFBoolean>())
        .map(bool::from)
        .unwrap_or(false)
}

/// The displayed address of the focused window of process `pid`, or `None`.
/// Costs a few ms in the normal case and never more than `BUDGET` plus one
/// call timeout.
pub(super) fn read_displayed_address(pid: u32) -> Option<String> {
    if pid == 0 || !unsafe { AXIsProcessTrusted() } {
        return None;
    }
    let started = Instant::now();
    let app_ref = unsafe { AXUIElementCreateApplication(pid as i32) };
    if app_ref.is_null() {
        return None;
    }
    let app = unsafe { CFType::wrap_under_create_rule(app_ref) };
    unsafe { AXUIElementSetMessagingTimeout(app.as_CFTypeRef(), CALL_TIMEOUT_SECS) };
    let window = copy_attr(&app, "AXFocusedWindow")?;
    let field = find_address_bar(&window, started)?;

    // The user is typing in the address bar: that text is not the page.
    if is_focused(&field) {
        return None;
    }
    let value = copy_attr(&field, "AXValue")?.downcast_into::<CFString>()?;
    host_of_displayed(&value.to_string())
}

fn find_address_bar(window: &CFType, started: Instant) -> Option<CFType> {
    let mut queue: VecDeque<(CFType, usize, bool)> = VecDeque::new();
    queue.push_back((window.clone(), 0, false));
    let mut seen = 0usize;
    let mut weak: Option<CFType> = None;
    while let Some((el, depth, in_toolbar)) = queue.pop_front() {
        if seen >= MAX_ELEMENTS || started.elapsed() >= BUDGET {
            break;
        }
        seen += 1;
        let role = string_attr(&el, "AXRole");
        let in_toolbar = in_toolbar || role == "AXToolbar";
        if is_text_field(&role) {
            let probe = Probe {
                identifier: string_attr(&el, "AXIdentifier"),
                description: string_attr(&el, "AXDescription"),
                placeholder: string_attr(&el, "AXPlaceholderValue"),
                role: role.clone(),
                in_toolbar,
            };
            match classify_probe(&probe) {
                Match::Strong => return Some(el),
                Match::Weak if weak.is_none() => weak = Some(el.clone()),
                _ => {}
            }
        }
        // Web content is huge and is never the address bar.
        if depth < MAX_DEPTH && role != "AXWebArea" {
            for c in children(&el) {
                queue.push_back((c, depth + 1, in_toolbar));
            }
        }
    }
    weak
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_from_displayed_values() {
        for (v, want) in [
            ("https://example.com/path?q=1", "example.com"),
            ("http://user:pw@example.com:8080/a", "example.com"),
            ("example.com", "example.com"),
            ("example.com/", "example.com"),
            ("Mail.Google.com/mail/u/0/#inbox", "mail.google.com"),
            ("www.example.com", "www.example.com"),
            ("https://www.example.com/", "www.example.com"),
            ("example.com:3000/x", "example.com"),
            ("  github.com \n", "github.com"),
            ("192.168.1.10:8080/admin", "192.168.1.10"),
        ] {
            assert_eq!(host_of_displayed(v).as_deref(), Some(want), "{v}");
        }
    }

    #[test]
    fn no_host_for_search_text_and_internal_pages() {
        for v in [
            "",
            "   ",
            "weather in paris",
            "example.com login page",
            "localhost",
            "localhost:3000",
            "about:preferences",
            "about:blank",
            "moz-extension://abc/page.html",
            "file:///Users/me/a.pdf",
            "view-source:https://example.com",
            ".com",
            "hello/world.txt",
            "https:///x",
        ] {
            assert_eq!(host_of_displayed(v), None, "{v}");
        }
    }

    fn probe(role: &str, id: &str, desc: &str, ph: &str, toolbar: bool) -> Probe {
        Probe {
            role: role.into(),
            identifier: id.into(),
            description: desc.into(),
            placeholder: ph.into(),
            in_toolbar: toolbar,
        }
    }

    #[test]
    fn recognises_the_address_bar() {
        assert_eq!(
            classify_probe(&probe("AXTextField", "urlbar-input", "", "", false)),
            Match::Strong
        );
        assert_eq!(
            classify_probe(&probe(
                "AXComboBox",
                "",
                "Search with Google or enter address",
                "",
                false
            )),
            Match::Strong
        );
        assert_eq!(
            classify_probe(&probe(
                "AXTextField",
                "",
                "",
                "Buscar con Google o introducir dirección",
                false
            )),
            Match::Strong
        );
        assert_eq!(
            classify_probe(&probe(
                "AXTextField",
                "",
                "",
                "Rechercher avec Google ou saisir une adresse",
                false
            )),
            Match::Strong
        );
        // Lenient fallback: a text field in the toolbar.
        assert_eq!(
            classify_probe(&probe("AXTextField", "x", "", "", true)),
            Match::Weak
        );
        // The separate search bar and page fields are not it.
        assert_eq!(
            classify_probe(&probe("AXTextField", "searchbar-input", "", "", true)),
            Match::No
        );
        assert_eq!(
            classify_probe(&probe("AXTextField", "q", "", "", false)),
            Match::No
        );
        assert_eq!(
            classify_probe(&probe("AXButton", "urlbar-input", "", "", true)),
            Match::No
        );
    }
}
