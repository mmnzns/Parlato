// System audio mute during recording.
//
// Reference VoiceInk : Infrastructure/SystemIntegration/Media/MediaController.swift
// (2.21, commit af772a81 "Fix playback and mute restoration after recording").
// Windows Core Audio API instead of Core Audio macOS. Port of Florian's
// Parla 517837b. The default render endpoint (speakers / headphones) is muted
// when the user starts dictating so notifications / music / calls don't leak
// into the microphone. A configurable delay restores the previous state when
// recording stops.
//
// Ownership model (VoiceInk 2.21) :
// - Parlato remembers the endpoint IDs it muted itself (`muted_devices`).
//   An endpoint that was already muted by the user is never added, so it is
//   never un-muted by Parlato.
// - On release, Parlato restores exactly the endpoints it muted, not the
//   current default : plugging headphones in during a dictation no longer
//   leaves the speakers muted.
// - Release runs even if the setting was turned off during the recording :
//   a mute Parlato applied is always given back.
// - Every engage() / release() starts a new session. A deferred engage that
//   wakes up after a release, or a delayed release that wakes up after a new
//   engage, sees a different session and does nothing. Ownership survives
//   rapid back-to-back recordings : the endpoint stays in `muted_devices`
//   until a release actually restores it.
//
// macOS : the `osascript` volume commands only reach the default output, so
// the endpoint ID there is one fixed value (see mute_macos.rs) and restore
// cannot follow a change of output device. Session counter, ownership and
// restore-even-if-off are the same.
//
// Toggling fails silently on devices without mute capability. Not fatal.

use std::collections::HashSet;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;
use tracing::{debug, warn};

const STORE_FILE: &str = "parla.settings.json";
const KEY_ENABLED: &str = "system_mute_enabled";
const KEY_DELAY: &str = "audio_resumption_delay_secs";

#[derive(Debug, Default)]
struct State {
    /// Endpoint IDs muted by Parlato and not restored yet.
    muted_devices: HashSet<String>,
    /// Recording session counter, bumped by every engage() and release().
    session: u64,
}

impl State {
    /// Starts a new session and returns its number.
    fn begin_session(&mut self) -> u64 {
        self.session = self.session.wrapping_add(1);
        self.session
    }

    /// False once another engage() / release() has started a newer session.
    fn is_current(&self, session: u64) -> bool {
        self.session == session
    }

    /// Un-mutes every owned endpoint with `unmute`, and forgets only those
    /// whose un-mute succeeded (a device that is unplugged stays owned and is
    /// retried by the next release).
    fn restore_owned(&mut self, mut unmute: impl FnMut(&str) -> Result<()>) {
        let owned: Vec<String> = self.muted_devices.iter().cloned().collect();
        for id in owned {
            match unmute(&id) {
                Ok(()) => {
                    self.muted_devices.remove(&id);
                    debug!("system audio restored");
                }
                Err(e) => warn!(error = %e, "unmute failed, will retry on next release"),
            }
        }
    }
}

fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(|| Mutex::new(State::default()))
}

pub fn is_enabled(app: &AppHandle) -> bool {
    app.store(STORE_FILE)
        .ok()
        .and_then(|s| s.get(KEY_ENABLED).and_then(|v| v.as_bool()))
        .unwrap_or(false)
}

pub fn set_enabled(app: &AppHandle, enabled: bool) -> Result<()> {
    let store = app.store(STORE_FILE).map_err(|e| anyhow!("store: {e}"))?;
    store.set(KEY_ENABLED, serde_json::Value::Bool(enabled));
    store.save().map_err(|e| anyhow!("store save: {e}"))
}

pub fn resumption_delay(app: &AppHandle) -> Duration {
    let secs = app
        .store(STORE_FILE)
        .ok()
        .and_then(|s| s.get(KEY_DELAY).and_then(|v| v.as_f64()))
        .unwrap_or(0.2)
        .clamp(0.0, 10.0);
    Duration::from_secs_f64(secs)
}

pub fn set_resumption_delay(app: &AppHandle, secs: f64) -> Result<()> {
    let clamped = secs.clamp(0.0, 10.0);
    let store = app.store(STORE_FILE).map_err(|e| anyhow!("store: {e}"))?;
    store.set(
        KEY_DELAY,
        serde_json::Value::Number(
            serde_json::Number::from_f64(clamped).unwrap_or(serde_json::Number::from(0)),
        ),
    );
    store.save().map_err(|e| anyhow!("store save: {e}"))
}

/// Delay before the mute actually engages after recording starts (VoiceInk's
/// `scheduleSystemMute` idea). Kept short so the mute feels responsive : when
/// system mute is on, only the first ~300ms of the start cue is heard before
/// the speakers are muted. A tap-then-stop shorter than this never mutes.
const ENGAGE_DELAY: Duration = Duration::from_millis(300);

/// What engage() must do with the current default endpoint, given whether it
/// is already muted. Pure decision, unit-tested.
fn should_take_ownership(currently_muted: bool) -> bool {
    // Already muted : either the user muted it (never ours) or Parlato muted
    // it in a previous session (still in `muted_devices`, restored later).
    !currently_muted
}

/// Engage system mute if the feature is enabled. Called on record start.
/// The actual mute is deferred by ENGAGE_DELAY and cancelled if a release()
/// (or a new engage()) starts a new session while we wait.
pub fn engage(app: &AppHandle) {
    if !is_enabled(app) {
        return;
    }
    let my_session = state().lock().begin_session();

    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(ENGAGE_DELAY).await;

        // Current default endpoint + mute state (COM read, no lock held).
        let (device_id, currently_muted) = match default_endpoint_state() {
            Ok(v) => v,
            Err(e) => {
                warn!(error = %e, "mute: cannot read current state, skipping");
                return;
            }
        };

        // Decide + act under a single lock so a concurrent release() can't be
        // overridden (which would leak a mute that never gets restored).
        let mut s = state().lock();
        if !s.is_current(my_session) {
            // Recording stopped (or restarted) during the delay - drop it.
            return;
        }
        if !should_take_ownership(currently_muted) {
            return;
        }
        match set_endpoint_muted(&device_id, true) {
            Ok(()) => {
                s.muted_devices.insert(device_id);
                debug!("system audio muted for recording");
            }
            Err(e) => warn!(error = %e, "mute: set muted failed"),
        }
    });
}

/// Release system mute after the configured delay. Called on record stop
/// / cancel, even if the setting is now off : Parlato always gives back a mute
/// it applied. Restores the endpoints Parlato muted, then forgets them.
pub fn release(app: &AppHandle) {
    let delay = resumption_delay(app);
    let my_session = {
        let mut s = state().lock();
        // New session : a deferred engage() still waiting out its
        // ENGAGE_DELAY sees a mismatch and aborts, the recording is over.
        let session = s.begin_session();
        if s.muted_devices.is_empty() {
            return;
        }
        session
    };

    tauri::async_runtime::spawn(async move {
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        let mut s = state().lock();
        if !s.is_current(my_session) {
            // A new recording started while we were waiting : it keeps the
            // ownership and its own release will restore the endpoints.
            return;
        }
        s.restore_owned(|id| set_endpoint_muted(id, false));
    });
}

#[cfg(windows)]
fn default_endpoint_state() -> Result<(String, bool)> {
    windows_audio::default_endpoint_state()
}

#[cfg(windows)]
fn set_endpoint_muted(device_id: &str, muted: bool) -> Result<()> {
    windows_audio::set_endpoint_muted(device_id, muted)
}

// Parlato : macOS calls live in mute_macos.rs.
#[cfg(target_os = "macos")]
#[path = "mute_macos.rs"]
mod macos_audio;

#[cfg(target_os = "macos")]
fn default_endpoint_state() -> Result<(String, bool)> {
    macos_audio::default_output_state()
}

#[cfg(target_os = "macos")]
fn set_endpoint_muted(device_id: &str, muted: bool) -> Result<()> {
    macos_audio::set_output_muted(device_id, muted)
}

#[cfg(not(any(windows, target_os = "macos")))]
fn default_endpoint_state() -> Result<(String, bool)> {
    Err(anyhow!("mute not implemented on this platform"))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn set_endpoint_muted(_device_id: &str, _muted: bool) -> Result<()> {
    Err(anyhow!("mute not implemented on this platform"))
}

#[cfg(windows)]
mod windows_audio {
    use anyhow::{anyhow, Result};
    use windows::core::{Interface, PCWSTR};
    use windows::Win32::Media::Audio::Endpoints::IAudioEndpointVolume;
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_APARTMENTTHREADED,
    };

    /// COM guard - scoped CoInitialize / CoUninitialize. cpal and other
    /// libraries may already have initialized COM in apartment-threaded
    /// mode on this thread. CoInitializeEx with the same mode is fine
    /// (returns S_FALSE) so we do not need to track the init count.
    struct ComGuard;

    impl ComGuard {
        fn new() -> Result<Self> {
            let hr = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
            // S_FALSE = 0x00000001 means COM was already initialized on
            // this thread - that's OK. Any other error is fatal.
            if hr.is_err() {
                return Err(anyhow!("CoInitializeEx: {hr:?}"));
            }
            Ok(Self)
        }
    }

    impl Drop for ComGuard {
        fn drop(&mut self) {
            unsafe { CoUninitialize() };
        }
    }

    fn enumerator() -> Result<IMMDeviceEnumerator> {
        unsafe {
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| anyhow!("CoCreateInstance MMDeviceEnumerator: {e}"))
        }
    }

    fn volume_of(device: &IMMDevice) -> Result<IAudioEndpointVolume> {
        let volume: IAudioEndpointVolume = unsafe {
            device
                .Activate(CLSCTX_ALL, None)
                .map_err(|e| anyhow!("IMMDevice::Activate: {e}"))?
        };
        volume
            .cast()
            .map_err(|e| anyhow!("cast IAudioEndpointVolume: {e}"))
    }

    fn device_id(device: &IMMDevice) -> Result<String> {
        unsafe {
            let raw = device
                .GetId()
                .map_err(|e| anyhow!("IMMDevice::GetId: {e}"))?;
            let id = raw.to_string();
            CoTaskMemFree(Some(raw.0 as *const _));
            id.map_err(|e| anyhow!("endpoint id utf16: {e}"))
        }
    }

    /// ID + mute state of the current default render endpoint.
    pub fn default_endpoint_state() -> Result<(String, bool)> {
        let _com = ComGuard::new()?;
        let device = unsafe {
            enumerator()?
                .GetDefaultAudioEndpoint(eRender, eConsole)
                .map_err(|e| anyhow!("GetDefaultAudioEndpoint: {e}"))?
        };
        let id = device_id(&device)?;
        let muted = unsafe {
            volume_of(&device)?
                .GetMute()
                .map_err(|e| anyhow!("GetMute: {e}"))?
        };
        Ok((id, muted.as_bool()))
    }

    /// Mute / un-mute a specific endpoint by ID (not the current default).
    pub fn set_endpoint_muted(device_id: &str, muted: bool) -> Result<()> {
        let _com = ComGuard::new()?;
        let wide: Vec<u16> = device_id.encode_utf16().chain(std::iter::once(0)).collect();
        let device = unsafe {
            enumerator()?
                .GetDevice(PCWSTR::from_raw(wide.as_ptr()))
                .map_err(|e| anyhow!("GetDevice: {e}"))?
        };
        unsafe {
            volume_of(&device)?
                .SetMute(muted, std::ptr::null())
                .map_err(|e| anyhow!("SetMute({muted}): {e}"))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn never_takes_ownership_of_an_already_muted_endpoint() {
        // VoiceInk 2.21 : "A device already muted by the user is never added
        // to our ownership."
        assert!(!should_take_ownership(true));
        assert!(should_take_ownership(false));
    }

    #[test]
    fn every_engage_and_release_starts_a_new_session() {
        let mut s = State::default();
        let engage = s.begin_session();
        assert!(s.is_current(engage));
        // A release starts a newer session : the deferred engage is stale.
        let release = s.begin_session();
        assert!(!s.is_current(engage));
        assert!(s.is_current(release));
        // A new engage makes the pending release stale (rapid back-to-back).
        let engage2 = s.begin_session();
        assert!(!s.is_current(release));
        assert!(s.is_current(engage2));
    }

    #[test]
    fn restore_gives_back_every_owned_endpoint_not_the_default() {
        // Speakers muted, then headphones became the default : the restore
        // goes by ID and reaches the speakers.
        let mut s = State::default();
        s.muted_devices.insert("speakers".to_string());
        let mut restored = Vec::new();
        s.restore_owned(|id| {
            restored.push(id.to_string());
            Ok(())
        });
        assert_eq!(restored, vec!["speakers".to_string()]);
        assert!(s.muted_devices.is_empty());
    }

    #[test]
    fn failed_unmute_keeps_ownership_for_the_next_release() {
        let mut s = State::default();
        s.muted_devices.insert("usb-dac".to_string());
        s.muted_devices.insert("speakers".to_string());
        s.restore_owned(|id| {
            if id == "usb-dac" {
                Err(anyhow!("device unplugged"))
            } else {
                Ok(())
            }
        });
        assert_eq!(s.muted_devices.len(), 1);
        assert!(s.muted_devices.contains("usb-dac"));
        // Next release, the device is back.
        s.restore_owned(|_| Ok(()));
        assert!(s.muted_devices.is_empty());
    }
}
