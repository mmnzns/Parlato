// Module screen_context : capture de la fenetre active + OCR pour remplir
// le bloc <CURRENT_WINDOW_CONTEXT> de l'enhancement.
//
// Reference VoiceInk : Services/ScreenCaptureService.swift + l'assemblage
// dans AIEnhancementService.getSystemMessage.

pub mod capture;
#[cfg_attr(target_os = "macos", path = "ocr_macos.rs")]
pub mod ocr;
pub mod service;
