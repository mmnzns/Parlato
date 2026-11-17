// Logs every Objective-C exception before it is thrown (macOS).
//
// Why: an Objective-C exception that reaches tao's event loop cannot be
// caught by Rust, so the app aborts ("Rust cannot catch foreign exceptions")
// and the crash report only shows `abort()`, never the exception itself.
// 0.16.0 to 0.17.0 crashed that way a few seconds after some dictations, and
// the reason had to be inferred. With this hook the next such crash leaves
// the exception's name and reason in parlato.log.
//
// The runtime calls the preprocessor for every `objc_exception_throw`,
// including exceptions that AppKit catches itself, so a logged exception is
// not by itself a crash. The reason is cut short; AppKit reasons describe API
// misuse (selector, index, key path), not what the person dictated.

use objc2::runtime::AnyObject;
use objc2::msg_send;

type Preprocessor = unsafe extern "C" fn(*mut AnyObject) -> *mut AnyObject;

extern "C" {
    fn objc_setExceptionPreprocessor(f: Preprocessor) -> Option<Preprocessor>;
}

/// The preprocessor that was installed before ours (normally none).
static PREVIOUS: std::sync::OnceLock<Option<Preprocessor>> = std::sync::OnceLock::new();

/// Longest part of an exception reason written to the log.
const MAX_REASON_CHARS: usize = 200;

/// Installs the logging hook. Safe to call more than once (later calls do
/// nothing).
pub fn install() {
    PREVIOUS.get_or_init(|| {
        // SAFETY: `preprocess` matches the runtime's preprocessor signature
        // and returns the exception unchanged.
        unsafe { objc_setExceptionPreprocessor(preprocess) }
    });
}

fn shorten(reason: &str) -> String {
    let mut out: String = reason.chars().take(MAX_REASON_CHARS).collect();
    if reason.chars().count() > MAX_REASON_CHARS {
        out.push('…');
    }
    out
}

/// Text of an NSString, or an empty string.
///
/// SAFETY: `s` is null or an NSString.
unsafe fn ns_string(s: *mut AnyObject) -> String {
    if s.is_null() {
        return String::new();
    }
    let utf8: *const std::os::raw::c_char = msg_send![s, UTF8String];
    if utf8.is_null() {
        return String::new();
    }
    std::ffi::CStr::from_ptr(utf8).to_string_lossy().into_owned()
}

unsafe extern "C" fn preprocess(exception: *mut AnyObject) -> *mut AnyObject {
    // Never let anything escape from here: a panic would itself abort.
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        if exception.is_null() {
            return;
        }
        // Only NSException (and subclasses) answer -name / -reason.
        let is_ns_exception: bool = {
            let cls = objc2::class!(NSException);
            msg_send![exception, isKindOfClass: cls]
        };
        if !is_ns_exception {
            tracing::warn!("Objective-C exception thrown (not an NSException)");
            return;
        }
        let name_obj: *mut AnyObject = msg_send![exception, name];
        let reason_obj: *mut AnyObject = msg_send![exception, reason];
        let name = ns_string(name_obj);
        let reason = shorten(&ns_string(reason_obj));
        let main_thread = objc2_foundation::MainThreadMarker::new().is_some();
        tracing::warn!(%name, %reason, main_thread, "Objective-C exception thrown");
    }));
    match PREVIOUS.get().copied().flatten() {
        Some(previous) => previous(exception),
        None => exception,
    }
}

#[cfg(test)]
mod tests {
    use super::{shorten, MAX_REASON_CHARS};

    #[test]
    fn long_reasons_are_cut_and_marked() {
        let long = "x".repeat(MAX_REASON_CHARS + 50);
        let out = shorten(&long);
        assert_eq!(out.chars().count(), MAX_REASON_CHARS + 1);
        assert!(out.ends_with('…'));
        assert_eq!(shorten("short"), "short");
    }
}
