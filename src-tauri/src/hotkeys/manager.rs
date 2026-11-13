// Manager hotkey - machine a etats qui orchestre la relation touche -> action.
//
// Reference VoiceInk :
// - Features/Shortcuts/Coordination/RecordingShortcutManager.swift
//   (RecordingShortcutModeHandler : toggle / pushToTalk / hybrid, seuil
//   hybride 0.5 s, cooldown 0.5 s).
// - Features/Shortcuts/Coordination/RecorderPanelShortcutManager.swift
//   (double-Echap 1.5 s, astuce "Press Esc again to cancel" affichee une
//   seule fois, raccourci d'annulation personnalise qui remplace le
//   double-Echap quand il est configure).
//
// Modes :
//  - Toggle      : tap ou hold -> toggle recorder.
//  - PushToTalk  : down -> start, up -> stop.
//  - Hybrid      : si press > 500 ms : PTT (stop au release). Si tap court : toggle on + hands-free.
//
// Double-tap (Parlato 0.16, reglage "Double-tap to keep recording", actif par
// defaut) : en Hold-to-talk, un tap court (< 250 ms) ne coupe plus tout de
// suite, il attend 300 ms. Un deuxieme appui dans ce delai convertit
// l'enregistrement en cours en hands-free (le prochain appui l'arrete) ; sans
// deuxieme appui, l'arret part a la fin du delai (`on_tick`). Le demarrage a
// la premiere pression n'est jamais retarde. Toggle et Hybrid font deja du
// hands-free sur un tap : ils ne changent pas.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::mpsc::RecvTimeoutError;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

use super::keyboard_hook::{HotkeyEvent, HotkeySlot, UtilityAction};

/// Seuil exact VoiceInk `hybridPressThreshold` : 0.5 s.
const HYBRID_PRESS_THRESHOLD: Duration = Duration::from_millis(500);
/// Cooldown anti-rebond apres une action, VoiceInk `shortcutPressCooldown`.
const ACTION_COOLDOWN: Duration = Duration::from_millis(500);
/// Fenetre pour double-tap Escape (VoiceInk `escapeDoublePressThreshold`).
const ESC_DOUBLE_TAP_WINDOW: Duration = Duration::from_millis(1500);
/// Parlato 0.16 : delai entre le relachement du premier tap et le deuxieme
/// appui pour compter un double-tap.
pub const DOUBLE_TAP_WINDOW: Duration = Duration::from_millis(300);
/// Parlato 0.16 : un appui plus court que ca est un "tap" (au-dela, c'est
/// quelqu'un qui parle en maintenant la touche).
pub const TAP_MAX_PRESS: Duration = Duration::from_millis(250);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HotkeyMode {
    Toggle,
    PushToTalk,
    #[default]
    Hybrid,
}

/// Action derivee par le manager et consommee par le reste de l'app.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyAction {
    StartRecording,
    StopRecording,
    CancelRecording,
    /// Transition en mode hands-free (toggle on, restera jusqu'a stop explicite).
    EnterHandsFree,
    /// Selection directe du Nieme profil Power Mode active (Alt+chiffre).
    SelectPowerMode(usize),
    /// Premier Echap pendant un enregistrement : l'UI peut afficher
    /// "Appuyez encore sur Echap pour annuler" (VoiceInk
    /// showEscapeConfirmationHintIfNeeded). Aucun effet sur l'enregistrement.
    EscapeHint,
    /// Raccourci utilitaire global (copier / coller / reessayer / historique).
    Utility(UtilityAction),
}

/// Premier tap court en Hold-to-talk : l'arret est en attente d'un deuxieme tap.
#[derive(Debug, Clone, Copy)]
struct PendingTap {
    slot: HotkeySlot,
    /// Fin de la fenetre (relachement du tap + DOUBLE_TAP_WINDOW).
    deadline: Instant,
}

#[derive(Default)]
struct State {
    is_recording: bool,
    is_hands_free: bool,
    key_down_since: Option<Instant>,
    last_action_at: Option<Instant>,
    esc_first_press_at: Option<Instant>,
    /// Parlato 0.16 : arret differe en attente d'un double-tap.
    pending_tap: Option<PendingTap>,
    /// Parlato 0.16 : session passee en hands-free par un double-tap en
    /// Hold-to-talk (le prochain appui l'arrete).
    dt_hands_free: bool,
    /// Parlato 0.16 : le relachement qui suit le 2e tap ne doit pas arreter.
    ignore_release: bool,
}

impl State {
    /// Efface tout ce qui concerne le double-tap (fin de session).
    fn clear_double_tap(&mut self) {
        self.pending_tap = None;
        self.dt_hands_free = false;
        self.ignore_release = false;
    }
}

pub struct HotkeyManager {
    state: Mutex<State>,
    /// Mode applique a chaque slot. Stockes derriere un Mutex pour etre
    /// modifiable a chaud quand l'utilisateur change la config dans
    /// Settings sans avoir a recreer le manager.
    modes: Mutex<HotkeyModes>,
    /// Vrai quand un raccourci d'annulation personnalise est configure :
    /// le double-Echap est alors desactive (VoiceInk : `.recorderPanelEscape`
    /// n'est enregistre que si `ShortcutStore.shortcut(for: .cancelRecorder)`
    /// est nil).
    custom_cancel: AtomicBool,
    /// Parlato 0.16 : reglage "Double-tap to keep recording".
    double_tap: AtomicBool,
}

#[derive(Debug, Clone, Copy)]
struct HotkeyModes {
    primary: HotkeyMode,
    secondary: HotkeyMode,
}

impl HotkeyManager {
    pub fn with_modes(mode_primary: HotkeyMode, mode_secondary: HotkeyMode) -> Self {
        Self {
            state: Mutex::new(State::default()),
            modes: Mutex::new(HotkeyModes {
                primary: mode_primary,
                secondary: mode_secondary,
            }),
            custom_cancel: AtomicBool::new(false),
            double_tap: AtomicBool::new(true),
        }
    }

    /// Active / desactive le double-tap (reglage, applique a chaud). Un tap
    /// deja en attente d'un deuxieme appui se termine normalement.
    pub fn set_double_tap(&self, enabled: bool) {
        self.double_tap.store(enabled, Ordering::Relaxed);
    }

    pub fn update_modes(&self, primary: HotkeyMode, secondary: HotkeyMode) {
        let mut g = self.modes.lock();
        g.primary = primary;
        g.secondary = secondary;
    }

    /// Active / desactive le double-Echap selon qu'un raccourci
    /// d'annulation personnalise est configure.
    pub fn set_custom_cancel(&self, enabled: bool) {
        self.custom_cancel.store(enabled, Ordering::Relaxed);
        if enabled {
            self.state.lock().esc_first_press_at = None;
        }
    }

    /// Notifie le manager que l'enregistrement a reellement demarre ou s'est arrete.
    pub fn mark_recording_state(&self, recording: bool) {
        let mut state = self.state.lock();
        state.is_recording = recording;
        if !recording {
            state.is_hands_free = false;
            state.key_down_since = None;
            state.esc_first_press_at = None;
            state.clear_double_tap();
        }
    }

    /// Un enregistrement a ete demarre hors hotkey (menu tray, UI). Il est
    /// par construction hands-free : la prochaine pression du raccourci
    /// principal doit l'arreter, comme apres un tap court en mode hybride.
    pub fn mark_started_externally(&self) {
        let mut state = self.state.lock();
        state.is_recording = true;
        state.is_hands_free = true;
        state.key_down_since = None;
    }

    /// Vrai si le manager considere un enregistrement en cours.
    #[cfg(test)]
    pub fn is_recording(&self) -> bool {
        self.state.lock().is_recording
    }

    /// Transforme un evenement hook en action metier. None si rien a faire.
    pub fn handle_event(&self, event: HotkeyEvent) -> Option<HotkeyAction> {
        match event {
            HotkeyEvent::Pressed { slot, timestamp } => self.on_pressed(slot, timestamp),
            HotkeyEvent::Released { slot, timestamp } => self.on_released(slot, timestamp),
            HotkeyEvent::EscapePressed { timestamp } => self.on_escape(timestamp),
            // Selection Power Mode : pass-through direct, sans passer par la
            // machine a etats toggle/PTT/hybrid ni le cooldown (le gating est
            // fait en amont par le hook via POWER_SHORTCUT_COUNT).
            HotkeyEvent::SelectPowerMode { index } => {
                Some(HotkeyAction::SelectPowerMode(index))
            }
            // Raccourcis utilitaires : pass-through, independants de l'etat
            // d'enregistrement (VoiceInk handleGlobalShortcut au keyUp).
            HotkeyEvent::Utility { action } => Some(HotkeyAction::Utility(action)),
            HotkeyEvent::CancelRequested { timestamp } => self.on_cancel_requested(timestamp),
        }
    }

    fn mode_for(&self, slot: HotkeySlot) -> HotkeyMode {
        let g = self.modes.lock();
        match slot {
            HotkeySlot::Primary => g.primary,
            HotkeySlot::Secondary => g.secondary,
        }
    }

    fn on_pressed(&self, slot: HotkeySlot, timestamp: Instant) -> Option<HotkeyAction> {
        let mut state = self.state.lock();

        // Parlato 0.16 : deuxieme appui d'un double-tap. Avant le cooldown :
        // le premier appui a lance l'enregistrement moins de 500 ms plus tot.
        if let Some(pending) = state.pending_tap {
            if pending.slot == slot {
                state.pending_tap = None;
                if state.is_recording {
                    if timestamp <= pending.deadline {
                        state.is_hands_free = true;
                        state.dt_hands_free = true;
                        state.ignore_release = true;
                        state.key_down_since = Some(timestamp);
                        state.last_action_at = Some(timestamp);
                        info!("PTT: double-tap, hands-free");
                        return Some(HotkeyAction::EnterHandsFree);
                    }
                    // Fenetre depassee (evenement arrive en retard) : le tap
                    // attendu etait seul, on l'arrete maintenant.
                    state.is_recording = false;
                    state.last_action_at = Some(pending.deadline - DOUBLE_TAP_WINDOW);
                    info!("PTT: stop (tap seul, deuxieme appui trop tard)");
                    return Some(HotkeyAction::StopRecording);
                }
            }
        }

        // Cooldown.
        if let Some(last) = state.last_action_at {
            if timestamp.duration_since(last) < ACTION_COOLDOWN {
                debug!("Cooldown actif, press ignore");
                return None;
            }
        }

        state.key_down_since = Some(timestamp);

        let mode = self.mode_for(slot);
        match mode {
            HotkeyMode::PushToTalk => {
                if !state.is_recording {
                    state.is_recording = true;
                    state.last_action_at = Some(timestamp);
                    info!("PTT: start");
                    Some(HotkeyAction::StartRecording)
                } else if state.dt_hands_free {
                    // Parlato 0.16 : session hands-free issue d'un double-tap,
                    // un appui l'arrete.
                    state.is_recording = false;
                    state.is_hands_free = false;
                    state.clear_double_tap();
                    state.last_action_at = Some(timestamp);
                    info!("PTT: stop (exit double-tap hands-free)");
                    Some(HotkeyAction::StopRecording)
                } else {
                    None
                }
            }
            HotkeyMode::Toggle | HotkeyMode::Hybrid => {
                // VoiceInk handleKeyDown : si hands-free actif, un press coupe la session.
                if state.is_hands_free && state.is_recording {
                    state.is_recording = false;
                    state.is_hands_free = false;
                    state.last_action_at = Some(timestamp);
                    info!("Toggle: stop (exit hands-free)");
                    Some(HotkeyAction::StopRecording)
                } else if !state.is_recording {
                    state.is_recording = true;
                    state.last_action_at = Some(timestamp);
                    info!("Toggle/Hybrid: start");
                    Some(HotkeyAction::StartRecording)
                } else {
                    None
                }
            }
        }
    }

    fn on_released(&self, slot: HotkeySlot, timestamp: Instant) -> Option<HotkeyAction> {
        let mut state = self.state.lock();
        let pressed_at = state.key_down_since.take();
        let press_duration = pressed_at
            .map(|t| timestamp.duration_since(t))
            .unwrap_or_default();

        let mode = self.mode_for(slot);
        match mode {
            HotkeyMode::Toggle => {
                // VoiceInk handleKeyUp : release en Toggle arme le hands-free
                // pour que la prochaine press coupe la session. Sans ca, un
                // "toggle" pur ne pourrait jamais s'eteindre via press.
                if state.is_recording {
                    state.is_hands_free = true;
                }
                None
            }
            HotkeyMode::PushToTalk => {
                // Parlato 0.16 : relachement qui suit le 2e tap d'un double-tap.
                if state.ignore_release {
                    state.ignore_release = false;
                    return None;
                }
                if state.is_recording {
                    // Parlato 0.16 : un tap court n'arrete pas tout de suite,
                    // il attend un eventuel deuxieme tap (sauf session lancee
                    // hors raccourci, deja hands-free : comportement inchange).
                    if self.double_tap.load(Ordering::Relaxed)
                        && !state.is_hands_free
                        && pressed_at.is_some()
                        && press_duration < TAP_MAX_PRESS
                    {
                        state.pending_tap = Some(PendingTap {
                            slot,
                            deadline: timestamp + DOUBLE_TAP_WINDOW,
                        });
                        debug!("PTT: tap court, arret differe (double-tap possible)");
                        return None;
                    }
                    state.is_recording = false;
                    state.last_action_at = Some(timestamp);
                    info!("PTT: stop");
                    Some(HotkeyAction::StopRecording)
                } else {
                    None
                }
            }
            HotkeyMode::Hybrid => {
                // VoiceInk handleKeyUp : hold long = PTT, tap court = hands-free.
                if press_duration >= HYBRID_PRESS_THRESHOLD && state.is_recording {
                    state.is_recording = false;
                    state.last_action_at = Some(timestamp);
                    info!(duration_ms = press_duration.as_millis() as u64, "Hybrid: stop (PTT)");
                    Some(HotkeyAction::StopRecording)
                } else if state.is_recording {
                    state.is_hands_free = true;
                    info!(
                        duration_ms = press_duration.as_millis() as u64,
                        "Hybrid: hands-free activated"
                    );
                    Some(HotkeyAction::EnterHandsFree)
                } else {
                    None
                }
            }
        }
    }

    /// Parlato 0.16 : fin de la fenetre du double-tap en attente, ou None.
    /// `dispatch_loop` s'en sert comme delai d'attente de `recv_timeout`.
    pub fn next_deadline(&self) -> Option<Instant> {
        self.state.lock().pending_tap.map(|p| p.deadline)
    }

    /// Parlato 0.16 : appele quand `next_deadline` est atteinte. Si aucun
    /// deuxieme tap n'est venu, arrete l'enregistrement du tap seul.
    pub fn on_tick(&self, now: Instant) -> Option<HotkeyAction> {
        let mut state = self.state.lock();
        let pending = state.pending_tap?;
        if now < pending.deadline {
            return None;
        }
        state.pending_tap = None;
        if !state.is_recording {
            return None;
        }
        state.is_recording = false;
        // Meme cooldown qu'avant : compte depuis le relachement du tap.
        state.last_action_at = Some(pending.deadline - DOUBLE_TAP_WINDOW);
        info!("PTT: stop (tap seul)");
        Some(HotkeyAction::StopRecording)
    }

    fn on_escape(&self, timestamp: Instant) -> Option<HotkeyAction> {
        // Un raccourci d'annulation personnalise remplace le double-Echap.
        if self.custom_cancel.load(Ordering::Relaxed) {
            return None;
        }
        let mut state = self.state.lock();
        if !state.is_recording {
            return None;
        }
        match state.esc_first_press_at {
            Some(first) if timestamp.duration_since(first) <= ESC_DOUBLE_TAP_WINDOW => {
                state.esc_first_press_at = None;
                state.is_recording = false;
                state.is_hands_free = false;
                state.clear_double_tap();
                state.last_action_at = Some(timestamp);
                info!("Double ESC: cancel recording");
                Some(HotkeyAction::CancelRecording)
            }
            _ => {
                state.esc_first_press_at = Some(timestamp);
                debug!("ESC pressed once, press again within 1500ms to cancel");
                Some(HotkeyAction::EscapeHint)
            }
        }
    }

    fn on_cancel_requested(&self, timestamp: Instant) -> Option<HotkeyAction> {
        let mut state = self.state.lock();
        if !state.is_recording {
            return None;
        }
        state.esc_first_press_at = None;
        state.is_recording = false;
        state.is_hands_free = false;
        state.clear_double_tap();
        state.last_action_at = Some(timestamp);
        info!("Custom cancel shortcut: cancel recording");
        Some(HotkeyAction::CancelRecording)
    }
}

/// Boucle de traitement : consomme les evenements hook et retourne les actions.
/// A faire tourner dans un thread dedie.
///
/// Parlato 0.16 : quand un tap attend un deuxieme appui (double-tap), la
/// boucle attend au plus jusqu'a la fin de la fenetre, puis `on_tick`.
pub fn dispatch_loop<F>(
    rx: mpsc::Receiver<HotkeyEvent>,
    manager: std::sync::Arc<HotkeyManager>,
    mut on_action: F,
) where
    F: FnMut(HotkeyAction) + Send + 'static,
{
    loop {
        let event = match manager.next_deadline() {
            None => match rx.recv() {
                Ok(event) => Some(event),
                Err(_) => break,
            },
            Some(deadline) => {
                let wait = deadline.saturating_duration_since(Instant::now());
                match rx.recv_timeout(wait) {
                    Ok(event) => Some(event),
                    Err(RecvTimeoutError::Timeout) => None,
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
        };
        let action = match event {
            Some(event) => manager.handle_event(event),
            None => manager.on_tick(Instant::now()),
        };
        if let Some(action) = action {
            on_action(action);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SLOT: HotkeySlot = HotkeySlot::Primary;

    fn pressed(t: Instant) -> HotkeyEvent {
        HotkeyEvent::Pressed { slot: SLOT, timestamp: t }
    }
    fn released(t: Instant) -> HotkeyEvent {
        HotkeyEvent::Released { slot: SLOT, timestamp: t }
    }
    fn esc(t: Instant) -> HotkeyEvent {
        HotkeyEvent::EscapePressed { timestamp: t }
    }

    #[test]
    fn toggle_press_starts_then_stops() {
        let m = HotkeyManager::with_modes(HotkeyMode::Toggle, HotkeyMode::Toggle);
        let t0 = Instant::now();
        assert_eq!(m.handle_event(pressed(t0)), Some(HotkeyAction::StartRecording));
        assert_eq!(m.handle_event(released(t0 + Duration::from_millis(100))), None);
        // Cooldown : second press doit etre ignore pendant 500ms.
        assert_eq!(
            m.handle_event(pressed(t0 + Duration::from_millis(300))),
            None,
            "cooldown actif"
        );
        // Apres cooldown : stop.
        assert_eq!(
            m.handle_event(pressed(t0 + Duration::from_millis(700))),
            Some(HotkeyAction::StopRecording)
        );
    }

    #[test]
    fn toggle_release_is_noop() {
        let m = HotkeyManager::with_modes(HotkeyMode::Toggle, HotkeyMode::Toggle);
        let t0 = Instant::now();
        m.handle_event(pressed(t0));
        // En toggle, le release ne doit jamais produire d'action.
        assert_eq!(m.handle_event(released(t0 + Duration::from_secs(5))), None);
    }

    #[test]
    fn ptt_press_starts_release_stops() {
        let m = HotkeyManager::with_modes(HotkeyMode::PushToTalk, HotkeyMode::PushToTalk);
        // The 0.15 rule (a short tap stops at once); with double-tap on, see
        // the double-tap tests below.
        m.set_double_tap(false);
        let t0 = Instant::now();
        assert_eq!(m.handle_event(pressed(t0)), Some(HotkeyAction::StartRecording));
        assert_eq!(
            m.handle_event(released(t0 + Duration::from_millis(200))),
            Some(HotkeyAction::StopRecording)
        );
    }

    #[test]
    fn hybrid_long_press_is_ptt() {
        let m = HotkeyManager::with_modes(HotkeyMode::Hybrid, HotkeyMode::Hybrid);
        let t0 = Instant::now();
        m.handle_event(pressed(t0));
        // Hold >= 500ms : comportement PTT, release = stop.
        assert_eq!(
            m.handle_event(released(t0 + Duration::from_millis(600))),
            Some(HotkeyAction::StopRecording)
        );
    }

    #[test]
    fn hybrid_short_press_enters_hands_free() {
        let m = HotkeyManager::with_modes(HotkeyMode::Hybrid, HotkeyMode::Hybrid);
        let t0 = Instant::now();
        m.handle_event(pressed(t0));
        // Tap court (<500ms) : active hands-free au release.
        assert_eq!(
            m.handle_event(released(t0 + Duration::from_millis(100))),
            Some(HotkeyAction::EnterHandsFree)
        );
    }

    #[test]
    fn hybrid_hands_free_next_press_stops() {
        let m = HotkeyManager::with_modes(HotkeyMode::Hybrid, HotkeyMode::Hybrid);
        let t0 = Instant::now();
        m.handle_event(pressed(t0));
        m.handle_event(released(t0 + Duration::from_millis(100)));
        // Un nouveau press (apres cooldown) doit terminer la session hands-free.
        assert_eq!(
            m.handle_event(pressed(t0 + Duration::from_millis(700))),
            Some(HotkeyAction::StopRecording)
        );
    }

    #[test]
    fn secondary_slot_uses_its_own_mode() {
        // Primary = Toggle, Secondary = PushToTalk : verify les deux slots
        // sont independants.
        let m = HotkeyManager::with_modes(HotkeyMode::Toggle, HotkeyMode::PushToTalk);
        m.set_double_tap(false);
        let t0 = Instant::now();
        // Press secondary -> PTT start
        assert_eq!(
            m.handle_event(HotkeyEvent::Pressed { slot: HotkeySlot::Secondary, timestamp: t0 }),
            Some(HotkeyAction::StartRecording)
        );
        // Release secondary apres 100ms -> PTT stop
        assert_eq!(
            m.handle_event(HotkeyEvent::Released {
                slot: HotkeySlot::Secondary,
                timestamp: t0 + Duration::from_millis(100),
            }),
            Some(HotkeyAction::StopRecording)
        );
    }

    #[test]
    fn select_power_mode_passes_through_index() {
        // SelectPowerMode contourne la machine a etats (toggle/PTT) et le
        // cooldown : il retourne toujours l'action avec l'index, quel que
        // soit l'etat courant du manager.
        let m = HotkeyManager::with_modes(HotkeyMode::Hybrid, HotkeyMode::Hybrid);
        assert_eq!(
            m.handle_event(HotkeyEvent::SelectPowerMode { index: 0 }),
            Some(HotkeyAction::SelectPowerMode(0))
        );
        assert_eq!(
            m.handle_event(HotkeyEvent::SelectPowerMode { index: 9 }),
            Some(HotkeyAction::SelectPowerMode(9))
        );
    }

    #[test]
    fn utility_passes_through_regardless_of_state() {
        let m = HotkeyManager::with_modes(HotkeyMode::Hybrid, HotkeyMode::Hybrid);
        let t0 = Instant::now();
        assert_eq!(
            m.handle_event(HotkeyEvent::Utility {
                action: UtilityAction::CopyLastTranscription
            }),
            Some(HotkeyAction::Utility(UtilityAction::CopyLastTranscription))
        );
        // Meme pendant un enregistrement.
        m.handle_event(pressed(t0));
        assert_eq!(
            m.handle_event(HotkeyEvent::Utility {
                action: UtilityAction::OpenHistory
            }),
            Some(HotkeyAction::Utility(UtilityAction::OpenHistory))
        );
    }

    #[test]
    fn double_esc_cancels_when_recording() {
        let m = HotkeyManager::with_modes(HotkeyMode::Toggle, HotkeyMode::Toggle);
        let t0 = Instant::now();
        m.handle_event(pressed(t0));
        // Premier ESC : pas d'annulation, mais l'UI recoit l'astuce
        // (VoiceInk showEscapeConfirmationHintIfNeeded).
        assert_eq!(
            m.handle_event(esc(t0 + Duration::from_millis(100))),
            Some(HotkeyAction::EscapeHint)
        );
        // Second ESC dans la fenetre : cancel.
        assert_eq!(
            m.handle_event(esc(t0 + Duration::from_millis(500))),
            Some(HotkeyAction::CancelRecording)
        );
    }

    #[test]
    fn single_esc_does_not_cancel_when_not_recording() {
        let m = HotkeyManager::with_modes(HotkeyMode::Toggle, HotkeyMode::Toggle);
        let t0 = Instant::now();
        // Pas d'enregistrement en cours : ESC ne fait rien.
        assert_eq!(m.handle_event(esc(t0)), None);
        assert_eq!(m.handle_event(esc(t0 + Duration::from_millis(500))), None);
    }

    #[test]
    fn double_esc_outside_window_does_not_cancel() {
        let m = HotkeyManager::with_modes(HotkeyMode::Toggle, HotkeyMode::Toggle);
        let t0 = Instant::now();
        m.handle_event(pressed(t0));
        m.handle_event(esc(t0 + Duration::from_millis(100)));
        // Second ESC 2s plus tard : hors fenetre (1500ms), pas de cancel,
        // mais il re-arme la fenetre (nouvelle astuce).
        assert_eq!(
            m.handle_event(esc(t0 + Duration::from_millis(2100))),
            Some(HotkeyAction::EscapeHint)
        );
    }

    #[test]
    fn custom_cancel_disables_escape_and_cancels_when_recording() {
        let m = HotkeyManager::with_modes(HotkeyMode::Toggle, HotkeyMode::Toggle);
        m.set_custom_cancel(true);
        let t0 = Instant::now();
        // Pas d'enregistrement : le raccourci custom ne fait rien.
        assert_eq!(
            m.handle_event(HotkeyEvent::CancelRequested { timestamp: t0 }),
            None
        );
        m.handle_event(pressed(t0));
        // Echap est ignore (ni astuce ni annulation).
        assert_eq!(m.handle_event(esc(t0 + Duration::from_millis(100))), None);
        assert_eq!(m.handle_event(esc(t0 + Duration::from_millis(300))), None);
        // Le raccourci custom annule.
        assert_eq!(
            m.handle_event(HotkeyEvent::CancelRequested {
                timestamp: t0 + Duration::from_millis(400)
            }),
            Some(HotkeyAction::CancelRecording)
        );
        assert!(!m.is_recording());
    }

    #[test]
    fn external_start_is_stopped_by_next_press() {
        // Enregistrement demarre depuis le tray : la prochaine pression du
        // raccourci doit l'arreter directement (session hands-free).
        let m = HotkeyManager::with_modes(HotkeyMode::Hybrid, HotkeyMode::Hybrid);
        m.mark_started_externally();
        assert!(m.is_recording());
        let t0 = Instant::now();
        assert_eq!(m.handle_event(pressed(t0)), Some(HotkeyAction::StopRecording));
    }

    #[test]
    fn mark_recording_state_resets_hands_free() {
        let m = HotkeyManager::with_modes(HotkeyMode::Hybrid, HotkeyMode::Hybrid);
        let t0 = Instant::now();
        m.handle_event(pressed(t0));
        m.handle_event(released(t0 + Duration::from_millis(100)));
        // Hands-free etait actif. Simule un stop externe.
        m.mark_recording_state(false);
        // Un press immediat ne doit rien produire car state cleared et cooldown inactif.
        // En realite le cooldown reste actif mais le state est reset.
        assert_eq!(
            m.handle_event(pressed(t0 + Duration::from_millis(700))),
            Some(HotkeyAction::StartRecording),
            "apres reset state, un nouveau cycle commence"
        );
    }

    // -- Double-tap (0.16) ---------------------------------------------------

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn ptt() -> HotkeyManager {
        HotkeyManager::with_modes(HotkeyMode::PushToTalk, HotkeyMode::PushToTalk)
    }

    #[test]
    fn ptt_start_is_never_delayed_by_double_tap() {
        let m = ptt();
        let t0 = Instant::now();
        assert_eq!(m.handle_event(pressed(t0)), Some(HotkeyAction::StartRecording));
        assert!(m.is_recording());
    }

    #[test]
    fn ptt_single_tap_stops_when_the_window_ends() {
        let m = ptt();
        let t0 = Instant::now();
        m.handle_event(pressed(t0));
        // Short tap: the stop waits for a possible second tap.
        assert_eq!(m.handle_event(released(t0 + ms(100))), None);
        assert!(m.is_recording());
        assert_eq!(m.next_deadline(), Some(t0 + ms(100) + DOUBLE_TAP_WINDOW));
        // Too early: nothing.
        assert_eq!(m.on_tick(t0 + ms(399)), None);
        assert!(m.is_recording());
        // Window over: stop.
        assert_eq!(m.on_tick(t0 + ms(400)), Some(HotkeyAction::StopRecording));
        assert!(!m.is_recording());
        assert_eq!(m.next_deadline(), None);
        // Once only.
        assert_eq!(m.on_tick(t0 + ms(900)), None);
    }

    #[test]
    fn ptt_double_tap_converts_to_hands_free() {
        let m = ptt();
        let t0 = Instant::now();
        assert_eq!(m.handle_event(pressed(t0)), Some(HotkeyAction::StartRecording));
        assert_eq!(m.handle_event(released(t0 + ms(80))), None);
        // Second tap 150 ms after the release, inside the 500 ms cooldown of
        // the start: it must not be eaten by the cooldown.
        assert_eq!(
            m.handle_event(pressed(t0 + ms(230))),
            Some(HotkeyAction::EnterHandsFree)
        );
        assert!(m.is_recording());
        assert_eq!(m.next_deadline(), None, "no stop pending any more");
        // Releasing the second tap does not stop.
        assert_eq!(m.handle_event(released(t0 + ms(300))), None);
        assert!(m.is_recording());
        // Nothing stops it by itself.
        assert_eq!(m.on_tick(t0 + ms(5000)), None);
        // Tap again to stop.
        assert_eq!(
            m.handle_event(pressed(t0 + ms(6000))),
            Some(HotkeyAction::StopRecording)
        );
        assert!(!m.is_recording());
        // Its release is a no-op, and a new cycle starts normally.
        assert_eq!(m.handle_event(released(t0 + ms(6050))), None);
        assert_eq!(
            m.handle_event(pressed(t0 + ms(7000))),
            Some(HotkeyAction::StartRecording)
        );
    }

    #[test]
    fn ptt_double_tap_window_edges() {
        // Second press exactly at the end of the window still counts.
        let m = ptt();
        let t0 = Instant::now();
        m.handle_event(pressed(t0));
        m.handle_event(released(t0 + ms(100)));
        assert_eq!(
            m.handle_event(pressed(t0 + ms(100) + DOUBLE_TAP_WINDOW)),
            Some(HotkeyAction::EnterHandsFree)
        );
        // One millisecond later it is too late: the lone tap stops, the late
        // press is not a second tap.
        let m = ptt();
        m.handle_event(pressed(t0));
        m.handle_event(released(t0 + ms(100)));
        assert_eq!(
            m.handle_event(pressed(t0 + ms(401))),
            Some(HotkeyAction::StopRecording)
        );
        assert!(!m.is_recording());
    }

    #[test]
    fn ptt_tap_length_limit() {
        // 249 ms is a tap, 250 ms is someone talking.
        let m = ptt();
        let t0 = Instant::now();
        m.handle_event(pressed(t0));
        assert_eq!(m.handle_event(released(t0 + ms(249))), None);
        assert!(m.next_deadline().is_some());
        let m = ptt();
        m.handle_event(pressed(t0));
        assert_eq!(
            m.handle_event(released(t0 + TAP_MAX_PRESS)),
            Some(HotkeyAction::StopRecording)
        );
        assert_eq!(m.next_deadline(), None);
    }

    #[test]
    fn ptt_hold_is_unchanged_with_double_tap_on() {
        let m = ptt();
        let t0 = Instant::now();
        assert_eq!(m.handle_event(pressed(t0)), Some(HotkeyAction::StartRecording));
        assert_eq!(
            m.handle_event(released(t0 + ms(2000))),
            Some(HotkeyAction::StopRecording)
        );
        assert_eq!(m.next_deadline(), None);
    }

    #[test]
    fn double_tap_off_keeps_the_old_hold_to_talk() {
        let m = ptt();
        m.set_double_tap(false);
        let t0 = Instant::now();
        assert_eq!(m.handle_event(pressed(t0)), Some(HotkeyAction::StartRecording));
        // A 100 ms tap stops at once, as in 0.15.
        assert_eq!(
            m.handle_event(released(t0 + ms(100))),
            Some(HotkeyAction::StopRecording)
        );
        assert_eq!(m.next_deadline(), None);
    }

    #[test]
    fn toggle_and_hybrid_are_unchanged_by_double_tap() {
        for mode in [HotkeyMode::Toggle, HotkeyMode::Hybrid] {
            let m = HotkeyManager::with_modes(mode, mode);
            let t0 = Instant::now();
            assert_eq!(m.handle_event(pressed(t0)), Some(HotkeyAction::StartRecording));
            let rel = m.handle_event(released(t0 + ms(80)));
            match mode {
                HotkeyMode::Hybrid => assert_eq!(rel, Some(HotkeyAction::EnterHandsFree)),
                _ => assert_eq!(rel, None),
            }
            // A quick second tap does not stop the recording (cooldown, as
            // before) and nothing is pending.
            assert_eq!(m.handle_event(pressed(t0 + ms(230))), None, "{mode:?}");
            // (Hybrid repeats EnterHandsFree on that release, as in 0.15.)
            let rel2 = m.handle_event(released(t0 + ms(300)));
            match mode {
                HotkeyMode::Hybrid => assert_eq!(rel2, Some(HotkeyAction::EnterHandsFree)),
                _ => assert_eq!(rel2, None),
            }
            assert!(m.is_recording());
            assert_eq!(m.next_deadline(), None);
            // Later tap stops, as before.
            assert_eq!(
                m.handle_event(pressed(t0 + ms(900))),
                Some(HotkeyAction::StopRecording),
                "{mode:?}"
            );
        }
    }

    #[test]
    fn cancel_and_external_stop_clear_a_pending_tap() {
        let m = ptt();
        let t0 = Instant::now();
        m.handle_event(pressed(t0));
        m.handle_event(released(t0 + ms(100)));
        assert!(m.next_deadline().is_some());
        // Custom cancel shortcut.
        assert_eq!(
            m.handle_event(HotkeyEvent::CancelRequested { timestamp: t0 + ms(150) }),
            Some(HotkeyAction::CancelRecording)
        );
        assert_eq!(m.next_deadline(), None);
        assert_eq!(m.on_tick(t0 + ms(1000)), None);

        // Recording ended by something else (error, tray).
        let m = ptt();
        m.handle_event(pressed(t0));
        m.handle_event(released(t0 + ms(100)));
        m.mark_recording_state(false);
        assert_eq!(m.next_deadline(), None);
        assert_eq!(m.on_tick(t0 + ms(1000)), None);
        // A press after that is a fresh start, not a "second tap".
        assert_eq!(
            m.handle_event(pressed(t0 + ms(1200))),
            Some(HotkeyAction::StartRecording)
        );
    }

    #[test]
    fn double_escape_clears_a_pending_tap() {
        let m = ptt();
        let t0 = Instant::now();
        m.handle_event(pressed(t0));
        m.handle_event(released(t0 + ms(100)));
        m.handle_event(esc(t0 + ms(120)));
        assert_eq!(
            m.handle_event(esc(t0 + ms(200))),
            Some(HotkeyAction::CancelRecording)
        );
        assert_eq!(m.next_deadline(), None);
    }

    #[test]
    fn hands_free_from_double_tap_is_cancelled_by_cancel_shortcut() {
        let m = ptt();
        let t0 = Instant::now();
        m.handle_event(pressed(t0));
        m.handle_event(released(t0 + ms(80)));
        m.handle_event(pressed(t0 + ms(200)));
        m.handle_event(released(t0 + ms(260)));
        assert_eq!(
            m.handle_event(HotkeyEvent::CancelRequested { timestamp: t0 + ms(3000) }),
            Some(HotkeyAction::CancelRecording)
        );
        // The next press starts a new PTT recording, not a stop.
        assert_eq!(
            m.handle_event(pressed(t0 + ms(4000))),
            Some(HotkeyAction::StartRecording)
        );
    }

    #[test]
    fn other_slot_press_does_not_consume_the_pending_tap() {
        let m = HotkeyManager::with_modes(HotkeyMode::PushToTalk, HotkeyMode::PushToTalk);
        let t0 = Instant::now();
        m.handle_event(pressed(t0));
        m.handle_event(released(t0 + ms(100)));
        // The secondary key pressed meanwhile is not the second tap of the
        // primary one: the recording is neither converted nor restarted.
        assert_eq!(
            m.handle_event(HotkeyEvent::Pressed {
                slot: HotkeySlot::Secondary,
                timestamp: t0 + ms(200),
            }),
            None
        );
        assert!(m.next_deadline().is_some());
        assert_eq!(m.on_tick(t0 + ms(400)), Some(HotkeyAction::StopRecording));
    }

    #[test]
    fn tray_started_session_keeps_the_old_ptt_release() {
        // A session started from the tray is hands-free: a short press of the
        // PTT key still stops it at release, with no waiting.
        let m = ptt();
        m.mark_started_externally();
        let t0 = Instant::now();
        assert_eq!(m.handle_event(pressed(t0)), None);
        assert_eq!(
            m.handle_event(released(t0 + ms(100))),
            Some(HotkeyAction::StopRecording)
        );
    }

    #[test]
    fn stale_ignore_release_does_not_survive_the_session() {
        let m = ptt();
        let t0 = Instant::now();
        m.handle_event(pressed(t0));
        m.handle_event(released(t0 + ms(80)));
        m.handle_event(pressed(t0 + ms(200))); // double-tap, release never comes
        m.mark_recording_state(false); // session ends otherwise
        // New session: a normal hold stops at release.
        assert_eq!(
            m.handle_event(pressed(t0 + ms(3000))),
            Some(HotkeyAction::StartRecording)
        );
        assert_eq!(
            m.handle_event(released(t0 + ms(4000))),
            Some(HotkeyAction::StopRecording)
        );
    }

    #[test]
    fn dispatch_loop_stops_a_lone_tap_after_the_window() {
        use std::sync::Arc;
        let manager = Arc::new(ptt());
        let (tx, rx) = mpsc::channel();
        let (out_tx, out_rx) = mpsc::channel();
        let m2 = manager.clone();
        let handle = std::thread::spawn(move || {
            dispatch_loop(rx, m2, move |a| {
                let _ = out_tx.send((Instant::now(), a));
            });
        });
        let t0 = Instant::now();
        tx.send(HotkeyEvent::Pressed { slot: SLOT, timestamp: t0 }).unwrap();
        tx.send(HotkeyEvent::Released { slot: SLOT, timestamp: t0 }).unwrap();
        let (_, first) = out_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(first, HotkeyAction::StartRecording);
        let (at, second) = out_rx.recv_timeout(Duration::from_secs(2)).unwrap();
        assert_eq!(second, HotkeyAction::StopRecording);
        assert!(at.duration_since(t0) >= ms(250), "waited for the window");
        drop(tx);
        handle.join().unwrap();
    }

    #[test]
    fn dispatch_loop_keeps_recording_after_a_double_tap() {
        use std::sync::Arc;
        let manager = Arc::new(ptt());
        let (tx, rx) = mpsc::channel();
        let (out_tx, out_rx) = mpsc::channel();
        let m2 = manager.clone();
        let handle = std::thread::spawn(move || {
            dispatch_loop(rx, m2, move |a| {
                let _ = out_tx.send(a);
            });
        });
        let t0 = Instant::now();
        tx.send(HotkeyEvent::Pressed { slot: SLOT, timestamp: t0 }).unwrap();
        tx.send(HotkeyEvent::Released { slot: SLOT, timestamp: t0 + ms(50) }).unwrap();
        tx.send(HotkeyEvent::Pressed { slot: SLOT, timestamp: t0 + ms(150) }).unwrap();
        tx.send(HotkeyEvent::Released { slot: SLOT, timestamp: t0 + ms(200) }).unwrap();
        assert_eq!(out_rx.recv_timeout(Duration::from_secs(2)).unwrap(), HotkeyAction::StartRecording);
        assert_eq!(out_rx.recv_timeout(Duration::from_secs(2)).unwrap(), HotkeyAction::EnterHandsFree);
        // No stop arrives by itself.
        assert!(out_rx.recv_timeout(ms(600)).is_err());
        drop(tx);
        handle.join().unwrap();
    }
}
