// OCR on macOS. Phase 0 placeholder (see "Mac port plan" in CLAUDE.md):
// always fails, so screen context is skipped.
//
// Reference VoiceInk : Vision.framework VNRecognizeTextRequest
// (recognitionLevel=.accurate, usesLanguageCorrection=true). Phase 4
// implements it.

use anyhow::{anyhow, Result};

pub fn recognize_png(_png: &[u8]) -> Result<String> {
    Err(anyhow!("OCR is not implemented on macOS yet"))
}
