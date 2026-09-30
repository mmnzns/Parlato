// Permissions on macOS (TCC). Parlato needs:
// - Microphone: to record. Checked with AVCaptureDevice; cpal still lists
//   devices when access is denied and then records silence, so counting
//   devices is not enough.
// - Accessibility: for the global shortcut (event tap) and for pasting.
//
// Reference VoiceInk : Views/PermissionsView.swift and
// OnboardingPermissionsView.swift (AVCaptureDevice.authorizationStatus,
// AXIsProcessTrusted, links to x-apple.systempreferences panes).

use objc2_av_foundation::{AVAuthorizationStatus, AVCaptureDevice, AVMediaTypeAudio};

use super::permissions::PermissionState;
use crate::hotkeys::keyboard_hook::{accessibility_trusted, request_accessibility};

pub const PANE_MICROPHONE: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone";
pub const PANE_ACCESSIBILITY: &str =
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility";

fn mic_status() -> AVAuthorizationStatus {
    unsafe {
        match AVMediaTypeAudio {
            Some(media) => AVCaptureDevice::authorizationStatusForMediaType(media),
            None => AVAuthorizationStatus::NotDetermined,
        }
    }
}

pub fn microphone_state() -> PermissionState {
    let status = mic_status();
    let (ok, label, hint) = if status == AVAuthorizationStatus::Authorized {
        (true, "permissions.mac.micAllowed", None)
    } else if status == AVAuthorizationStatus::NotDetermined {
        (false, "permissions.mac.micNotAsked", Some("permissions.mac.micNotAskedHint"))
    } else {
        (false, "permissions.mac.micDenied", Some("permissions.mac.micDeniedHint"))
    };
    PermissionState {
        ok,
        label_key: label.into(),
        label_args: None,
        hint_key: hint.map(String::from),
        diagnostic: None,
    }
}

/// Shows macOS's microphone prompt if the user has never answered it.
/// Returns immediately; the answer arrives asynchronously and the UI
/// re-checks. When access was already denied macOS shows nothing, so the
/// UI opens System Settings instead.
pub fn request_microphone() {
    if mic_status() != AVAuthorizationStatus::NotDetermined {
        return;
    }
    let handler = block2::RcBlock::new(|granted: objc2::runtime::Bool| {
        tracing::info!(granted = granted.as_bool(), "microphone permission answered");
    });
    unsafe {
        if let Some(media) = AVMediaTypeAudio {
            AVCaptureDevice::requestAccessForMediaType_completionHandler(media, &handler);
        }
    }
}

pub fn accessibility_state() -> PermissionState {
    let ok = accessibility_trusted();
    PermissionState {
        ok,
        label_key: if ok {
            "permissions.mac.accessibilityAllowed"
        } else {
            "permissions.mac.accessibilityMissing"
        }
        .into(),
        label_args: None,
        hint_key: (!ok).then(|| "permissions.mac.accessibilityHint".into()),
        diagnostic: None,
    }
}

/// Shows macOS's Accessibility prompt (once per app identity) and allows the
/// hotkey thread to prompt again later if needed.
pub fn request_accessibility_access() -> bool {
    crate::hotkeys::keyboard_hook::allow_accessibility_prompt();
    request_accessibility()
}
