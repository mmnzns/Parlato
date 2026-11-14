// OCR on macOS with Apple's Vision framework.
//
// Reference VoiceInk : Services/ScreenCaptureService.swift, `extractText`:
// VNRecognizeTextRequest with recognitionLevel = .accurate and
// usesLanguageCorrection = true, observations joined with "\n". Same here.
//
// Parlato additions:
// - The recognition languages come from the user's dictation language when
//   Vision supports it (the text on screen is most likely in the language the
//   user speaks), with English as the second choice. With "auto" dictation or
//   a language Vision does not know, Vision detects the language itself.
// - Vision runs on a worker thread and the caller waits at most `TIMEOUT`.
//   The capture guard in `service.rs` is released even if Vision hangs, so one
//   stuck request cannot block every later capture.
// - The recognised text is never logged (the caller logs its length only).

use std::sync::mpsc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use objc2::rc::{autoreleasepool, Retained};
use objc2_foundation::{NSArray, NSData, NSString};
use objc2_vision::{
    VNRecognizeTextRequest, VNRequest, VNRequestTextRecognitionLevel, VNSequenceRequestHandler,
};

/// Longest wait for Vision on one window. A full-screen retina capture takes
/// well under a second with the accurate level on Apple Silicon; this only
/// guards against a hang.
const TIMEOUT: Duration = Duration::from_secs(10);

/// OCR on a PNG buffer, language chosen by Vision. Kept for parity with the
/// Windows module; `service.rs` calls `recognize_png_for` on the Mac.
#[allow(dead_code)]
pub fn recognize_png(png: &[u8]) -> Result<String> {
    recognize_png_for(png, None)
}

/// OCR on a PNG buffer. `dictation_language` is the user's dictation language
/// code ("en", "es", "fr-CA", ...), or `None` for auto-detect.
pub fn recognize_png_for(png: &[u8], dictation_language: Option<&str>) -> Result<String> {
    if png.is_empty() {
        return Err(anyhow!("empty image"));
    }
    let png = png.to_vec();
    let language = dictation_language.map(str::to_owned);
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("parlato-ocr".into())
        .spawn(move || {
            let _ = tx.send(run_vision(&png, language.as_deref()));
        })
        .map_err(|e| anyhow!("OCR thread: {e}"))?;
    match rx.recv_timeout(TIMEOUT) {
        Ok(result) => result,
        Err(_) => Err(anyhow!("OCR timed out")),
    }
}

fn run_vision(png: &[u8], dictation_language: Option<&str>) -> Result<String> {
    autoreleasepool(|_| {
        // Languages Vision can read at the accurate level. If the query
        // fails we pass none and let Vision detect the language.
        let supported = supported_languages();
        let wanted = vision_languages(dictation_language, &supported);

        let text = perform(png, &wanted);
        match text {
            // A language list Vision refuses must not lose the whole OCR:
            // retry once with automatic detection.
            Err(_) if !wanted.is_empty() => perform(png, &[]),
            other => other,
        }
    })
}

fn supported_languages() -> Vec<String> {
    let request = VNRecognizeTextRequest::new();
    request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
    // SAFETY: plain query on a fresh request object.
    match unsafe { request.supportedRecognitionLanguagesAndReturnError() } {
        Ok(list) => list.iter().map(|s| s.to_string()).collect(),
        Err(_) => Vec::new(),
    }
}

fn perform(png: &[u8], languages: &[String]) -> Result<String> {
    let request = VNRecognizeTextRequest::new();
    request.setRecognitionLevel(VNRequestTextRecognitionLevel::Accurate);
    request.setUsesLanguageCorrection(true);
    if languages.is_empty() {
        request.setAutomaticallyDetectsLanguage(true);
    } else {
        request.setAutomaticallyDetectsLanguage(false);
        let list: Vec<Retained<NSString>> =
            languages.iter().map(|l| NSString::from_str(l)).collect();
        request.setRecognitionLanguages(&NSArray::from_retained_slice(&list));
    }

    let data = NSData::with_bytes(png);
    // SAFETY: plain allocation of a request handler.
    let handler = unsafe { VNSequenceRequestHandler::new() };
    let as_request: &VNRequest = &request;
    let requests = NSArray::from_slice(&[as_request]);
    // SAFETY: both objects are alive for the call; the request was configured
    // above and the data is a PNG.
    unsafe { handler.performRequests_onImageData_error(&requests, &data) }
        .map_err(|e| anyhow!("Vision request failed: {}", e.localizedDescription()))?;

    let mut lines: Vec<String> = Vec::new();
    if let Some(results) = request.results() {
        for observation in results.iter() {
            let candidates = observation.topCandidates(1);
            if let Some(best) = candidates.firstObject() {
                let s = best.string().to_string();
                let s = s.trim();
                if !s.is_empty() {
                    lines.push(s.to_owned());
                }
            }
        }
    }
    Ok(lines.join("\n"))
}

/// Vision language codes to ask for, given the dictation language and the
/// list Vision supports. Pure, so it is unit tested. Empty = let Vision
/// detect the language.
pub(crate) fn vision_languages(dictation: Option<&str>, supported: &[String]) -> Vec<String> {
    let Some(code) = dictation.map(str::trim).filter(|c| !c.is_empty() && *c != "auto") else {
        return Vec::new();
    };
    let Some(primary) = vision_code(code) else {
        return Vec::new();
    };
    let mut out: Vec<String> = Vec::new();
    for candidate in [primary, "en-US"] {
        // With no supported list (the query failed) trust the table.
        let known = supported.is_empty()
            || supported.iter().any(|s| s.eq_ignore_ascii_case(candidate));
        if known && !out.iter().any(|o| o.eq_ignore_ascii_case(candidate)) {
            out.push(candidate.to_owned());
        }
    }
    // English alone as a fallback is no better than automatic detection.
    if out.len() == 1 && out[0].eq_ignore_ascii_case("en-US") && !primary.eq_ignore_ascii_case("en-US") {
        return Vec::new();
    }
    out
}

/// Map a dictation language code ("es", "pt-BR", "zh") to a Vision
/// recognition language. `None` when Vision has no model for it.
fn vision_code(code: &str) -> Option<&'static str> {
    let lower = code.to_ascii_lowercase().replace('_', "-");
    let base = lower.split('-').next().unwrap_or("");
    Some(match base {
        "en" => "en-US",
        "es" => "es-ES",
        "fr" => "fr-FR",
        "de" => "de-DE",
        "it" => "it-IT",
        "pt" => "pt-BR",
        "zh" => {
            if lower.contains("hant") || lower.ends_with("-tw") || lower.ends_with("-hk") {
                "zh-Hant"
            } else {
                "zh-Hans"
            }
        }
        "ja" => "ja-JP",
        "ko" => "ko-KR",
        "ru" => "ru-RU",
        "uk" => "uk-UA",
        "ar" => "ar-SA",
        "th" => "th-TH",
        "vi" => "vi-VT",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all() -> Vec<String> {
        ["en-US", "fr-FR", "es-ES", "de-DE", "pt-BR", "zh-Hans", "zh-Hant"]
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    #[test]
    fn auto_or_missing_language_lets_vision_detect() {
        assert!(vision_languages(None, &all()).is_empty());
        assert!(vision_languages(Some("auto"), &all()).is_empty());
        assert!(vision_languages(Some(""), &all()).is_empty());
    }

    #[test]
    fn dictation_language_first_english_second() {
        assert_eq!(vision_languages(Some("es"), &all()), vec!["es-ES", "en-US"]);
        assert_eq!(vision_languages(Some("fr-CA"), &all()), vec!["fr-FR", "en-US"]);
        assert_eq!(vision_languages(Some("en"), &all()), vec!["en-US"]);
        assert_eq!(vision_languages(Some("pt_BR"), &all()), vec!["pt-BR", "en-US"]);
    }

    #[test]
    fn chinese_variants() {
        assert_eq!(vision_languages(Some("zh"), &all())[0], "zh-Hans");
        assert_eq!(vision_languages(Some("zh-TW"), &all())[0], "zh-Hant");
    }

    #[test]
    fn unknown_or_unsupported_language_falls_back_to_detection() {
        assert!(vision_languages(Some("sw"), &all()).is_empty());
        // Vision on this macOS does not list Japanese: detect instead.
        assert!(vision_languages(Some("ja"), &all()).is_empty());
    }

    #[test]
    fn empty_supported_list_trusts_the_table() {
        assert_eq!(vision_languages(Some("de"), &[]), vec!["de-DE", "en-US"]);
    }
}
