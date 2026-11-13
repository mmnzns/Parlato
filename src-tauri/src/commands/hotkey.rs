// Commandes Tauri pour configurer les raccourcis globaux a chaud.
//
// Etat persiste sous parla.settings.json -> "hotkey" ->
//   { primary, secondary, actions: { copy_last_transcription, ... } }.
// Au boot, lib.rs::setup_hotkeys lit la config et installe le hook avec ces
// triggers + modes. A chaque set_hotkey_config :
//   1. on valide (conflits, touches reservees),
//   2. on persiste dans le store,
//   3. on appelle keyboard_hook::update_watched / update_utilities pour
//      swap les triggers surveilles sans re-installer le hook,
//   4. on appelle HotkeyManager::update_modes / set_custom_cancel,
//   5. on reconstruit le menu tray (accelerateurs affiches).
// Pas de redemarrage requis.
//
// Reference VoiceInk : Features/Shortcuts/State/ShortcutStore.swift
// (une cle UserDefaults par action), ShortcutValidator.swift (conflits
// entre actions, touche seule interdite hors F-keys, Option+chiffre reserve
// a la selection de mode).

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::{command, AppHandle, State};
use tauri_plugin_store::StoreExt;

use crate::hotkeys::keyboard_hook::{self, HotkeyOption, HotkeyTrigger, UtilityAction};
use crate::hotkeys::manager::{HotkeyManager, HotkeyMode};
use crate::hotkeys::reserved;

const STORE_FILE: &str = "parla.settings.json";
const KEY_HOTKEY: &str = "hotkey";

/// Wrapper Tauri State pour partager l'Arc<HotkeyManager> entre setup et
/// commandes. Le manager lui-meme est interieurement Sync (Mutex).
pub struct HotkeyManagerState(pub Arc<HotkeyManager>);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeyConfig {
    pub primary: HotkeySlotConfig,
    pub secondary: HotkeySlotConfig,
    /// Raccourcis additionnels (VoiceInk "Additional Shortcuts"). Absent
    /// des configs < 0.6.0 : serde(default) = tout a `None`.
    #[serde(default)]
    pub actions: UtilityShortcuts,
    /// Parlato 0.16 : "Double-tap to keep recording". Absent des configs
    /// < 0.16 : actif par defaut.
    #[serde(default = "default_double_tap")]
    pub double_tap: bool,
}

fn default_double_tap() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HotkeySlotConfig {
    pub trigger: HotkeyTrigger,
    pub mode: HotkeyMode,
}

/// Un trigger par action utilitaire, plus le raccourci d'annulation.
/// Tous `None` par defaut, comme VoiceInk ("Not Set").
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct UtilityShortcuts {
    /// Ajout Parla (VoiceInk n'a qu'un accelerateur de menu Shift+Cmd+C).
    pub copy_last_transcription: HotkeyTrigger,
    /// VoiceInk "Paste Last Transcription (Original)".
    pub paste_last_transcription: HotkeyTrigger,
    /// VoiceInk "Paste Last Transcription (Enhanced)".
    pub paste_last_enhancement: HotkeyTrigger,
    /// VoiceInk "Retry Last Transcription".
    pub retry_last_transcription: HotkeyTrigger,
    /// VoiceInk "Open History Window" (Parla : fenetre principale, onglet
    /// Historique).
    pub open_history: HotkeyTrigger,
    /// VoiceInk "Cancel Recording". `None` = double-Echap par defaut.
    pub cancel_recording: HotkeyTrigger,
}

impl UtilityShortcuts {
    pub fn trigger_for(&self, action: UtilityAction) -> HotkeyTrigger {
        match action {
            UtilityAction::CopyLastTranscription => self.copy_last_transcription,
            UtilityAction::PasteLastTranscription => self.paste_last_transcription,
            UtilityAction::PasteLastEnhancement => self.paste_last_enhancement,
            UtilityAction::RetryLastTranscription => self.retry_last_transcription,
            UtilityAction::OpenHistory => self.open_history,
        }
    }

    /// Paires (action, trigger) pour le hook clavier.
    pub fn as_utility_list(&self) -> Vec<(UtilityAction, HotkeyTrigger)> {
        UtilityAction::ALL
            .iter()
            .map(|a| (*a, self.trigger_for(*a)))
            .collect()
    }
}

impl HotkeyConfig {
    pub fn defaults() -> Self {
        Self {
            primary: HotkeySlotConfig {
                trigger: HotkeyTrigger::default_primary(),
                mode: HotkeyMode::Hybrid,
            },
            secondary: HotkeySlotConfig {
                trigger: HotkeyTrigger::None,
                mode: HotkeyMode::Hybrid,
            },
            actions: UtilityShortcuts::default(),
            double_tap: true,
        }
    }
}

/// Lit la config depuis le store. Defaults appliques par champ : si la
/// cle est absente OU si le format a change (deserialize fail), on retombe
/// sur Right Alt + Hybrid.
pub fn load(app: &AppHandle) -> HotkeyConfig {
    let Some(store) = app.store(STORE_FILE).ok() else {
        return HotkeyConfig::defaults();
    };
    let Some(value) = store.get(KEY_HOTKEY) else {
        return HotkeyConfig::defaults();
    };
    serde_json::from_value(value).unwrap_or_else(|err| {
        tracing::warn!("hotkey config invalide, fallback default: {err}");
        HotkeyConfig::defaults()
    })
}

#[command]
pub fn get_hotkey_config(app: AppHandle) -> HotkeyConfig {
    load(&app)
}

#[command]
pub fn set_hotkey_config(
    app: AppHandle,
    state: State<HotkeyManagerState>,
    config: HotkeyConfig,
) -> Result<(), String> {
    validate(&config)?;
    persist(&app, &config)?;
    apply_live(&state.0, &config);
    crate::tray::refresh(&app);
    Ok(())
}

/// Remet les deux raccourcis d'enregistrement a leur valeur par defaut
/// (Right Alt + Hybrid, pas de secondaire). Les raccourcis additionnels
/// sont conserves : chacun a son propre bouton d'effacement dans l'UI.
#[command]
pub fn reset_hotkey_config(
    app: AppHandle,
    state: State<HotkeyManagerState>,
) -> Result<HotkeyConfig, String> {
    let mut cfg = HotkeyConfig::defaults();
    cfg.actions = load(&app).actions;
    persist(&app, &cfg)?;
    apply_live(&state.0, &cfg);
    crate::tray::refresh(&app);
    Ok(cfg)
}

fn persist(app: &AppHandle, cfg: &HotkeyConfig) -> Result<(), String> {
    let store = app.store(STORE_FILE).map_err(|e| e.to_string())?;
    let value = serde_json::to_value(cfg).map_err(|e| e.to_string())?;
    store.set(KEY_HOTKEY, value);
    store.save().map_err(|e| e.to_string())
}

pub fn apply_live(manager: &Arc<HotkeyManager>, cfg: &HotkeyConfig) {
    keyboard_hook::update_watched(cfg.primary.trigger, cfg.secondary.trigger);
    keyboard_hook::update_utilities(cfg.actions.as_utility_list(), cfg.actions.cancel_recording);
    manager.update_modes(cfg.primary.mode, cfg.secondary.mode);
    manager.set_double_tap(cfg.double_tap);
    manager.set_custom_cancel(!matches!(cfg.actions.cancel_recording, HotkeyTrigger::None));
}

// -- Validation (VoiceInk ShortcutValidator) --------------------------------

/// Identifiant stable d'une action pour les messages d'erreur, traduit
/// cote frontend (`errors.shortcutConflict` + `hotkey.additional.<id>`).
fn action_id(action: UtilityAction) -> &'static str {
    match action {
        UtilityAction::CopyLastTranscription => "copyLast",
        UtilityAction::PasteLastTranscription => "pasteLast",
        UtilityAction::PasteLastEnhancement => "pasteLastEnhanced",
        UtilityAction::RetryLastTranscription => "retryLast",
        UtilityAction::OpenHistory => "openHistory",
    }
}

/// Touches acceptees seules (sans modificateur) pour les raccourcis
/// utilitaires (avales globalement) : F1-F24, Pause, PrintScreen, Insert,
/// Delete, Home, End, PageUp/Down. Miroir de STANDALONE_VKS dans
/// src/components/HotkeyRecorder.tsx et de la regle VoiceInk
/// `isFunctionKeyCode` (F-keys autorisees sans modificateur). Les
/// declencheurs d'enregistrement (primary/secondary) acceptent n'importe
/// quelle touche seule sauf Echap (voir `validate`).
fn standalone_allowed(vk: u32) -> bool {
    matches!(vk, 0x13 | 0x21 | 0x22 | 0x23 | 0x24 | 0x2C | 0x2D | 0x2E) || (0x70..=0x87).contains(&vk)
}

/// Verifie la config avant persistance. Codes d'erreur stables :
///  - `PARLA_ERR:shortcutNeedsModifier:<action>` : touche textuelle seule
///    pour un raccourci utilitaire.
///  - `PARLA_ERR:shortcutReserved:<action>` : Alt+chiffre (selection de
///    profil Power Mode pendant l'enregistrement).
///  - `PARLA_ERR:shortcutConflict:<action a>:<action b>` : meme trigger
///    utilise deux fois.
///
/// Parlato 0.16 :
///  - `PARLA_ERR:shortcutSystem:<action>:<id>` : raccourci deja utilise par le
///    systeme (voir hotkeys/reserved.rs), `errors.systemShortcut.<id>`.
///  - `PARLA_ERR:shortcutEscNeedsCancel:<action>` : Echap comme touche de
///    dictee seulement avec un autre raccourci d'annulation.
///  - `PARLA_ERR:shortcutTwoModifiers:<action>` : "deux modifiers" avec moins
///    de deux modifiers.
///  - `PARLA_ERR:shortcutDictationOnly:<action>` : touche seule ou modifiers
///    seuls pour une action autre que la dictee.
///  - `PARLA_ERR:shortcutUnsupported:<action>` : touche absente de cette
///    plateforme (Left Win / Menu sur Mac, Fn sur Windows).
///  - `PARLA_ERR:shortcutCapsHold:<action>` : Caps Lock en Hold-to-talk sur Mac.
pub fn validate(cfg: &HotkeyConfig) -> Result<(), String> {
    validate_on(cfg, cfg!(target_os = "macos"))
}

/// `validate` pour une plateforme donnee (les tests couvrent les deux).
fn validate_on(cfg: &HotkeyConfig, mac: bool) -> Result<(), String> {
    let mut entries: Vec<(&'static str, HotkeyTrigger)> = vec![
        ("primary", cfg.primary.trigger),
        ("secondary", cfg.secondary.trigger),
        ("cancelRecording", cfg.actions.cancel_recording),
    ];
    for a in UtilityAction::ALL {
        entries.push((action_id(a), cfg.actions.trigger_for(a)));
    }
    let entries: Vec<(&'static str, HotkeyTrigger)> = entries
        .into_iter()
        .filter(|(_, t)| !matches!(t, HotkeyTrigger::None))
        .collect();

    for (name, trigger) in &entries {
        // Declencheurs d'enregistrement : n'importe quelle touche seule (non
        // avalee par le listener).
        let is_record_trigger = matches!(*name, "primary" | "secondary");
        match trigger {
            HotkeyTrigger::Combo {
                vk,
                ctrl,
                alt,
                shift,
                win,
                ..
            } => {
                let has_modifier = *ctrl || *alt || *shift || *win;
                // Echap seul est accepte pour l'annulation (VoiceInk
                // ShortcutValidator : `.cancelRecorder` + kVK_Escape sans
                // flags) et, depuis 0.16, pour la dictee (voir plus bas).
                let escape_for_cancel = *name == "cancelRecording" && *vk == 0x1B;
                let allowed_alone = if is_record_trigger {
                    true
                } else {
                    standalone_allowed(*vk) || escape_for_cancel
                };
                if !has_modifier && !allowed_alone {
                    return Err(format!("PARLA_ERR:shortcutNeedsModifier:{name}"));
                }
                let is_top_row_digit = (0x30..=0x39).contains(vk);
                if *alt && !*ctrl && !*shift && !*win && is_top_row_digit {
                    return Err(format!("PARLA_ERR:shortcutReserved:{name}"));
                }
                let reserved = if mac {
                    reserved::reserved_mac(*vk, *ctrl, *alt, *shift, *win)
                } else {
                    reserved::reserved_windows(*vk, *ctrl, *alt, *shift, *win)
                };
                if let Some(id) = reserved {
                    return Err(format!("PARLA_ERR:shortcutSystem:{name}:{id}"));
                }
                // Echap seul comme touche de dictee : le double-Echap qui
                // annule l'enregistrement ne peut plus exister (chaque Echap
                // serait aussi un appui de dictee), il faut donc un autre
                // raccourci d'annulation.
                if is_record_trigger && *vk == 0x1B && !has_modifier {
                    let other_cancel = !matches!(cfg.actions.cancel_recording, HotkeyTrigger::None)
                        && !is_plain_escape(cfg.actions.cancel_recording);
                    if !other_cancel {
                        return Err(format!("PARLA_ERR:shortcutEscNeedsCancel:{name}"));
                    }
                }
            }
            HotkeyTrigger::Modifier { option } => {
                if *option == HotkeyOption::None {
                    return Err(format!("PARLA_ERR:shortcutNeedsModifier:{name}"));
                }
                let is_new_key = matches!(
                    option,
                    HotkeyOption::LeftWin
                        | HotkeyOption::CapsLock
                        | HotkeyOption::Menu
                        | HotkeyOption::Fn
                );
                if is_new_key && !is_record_trigger {
                    return Err(format!("PARLA_ERR:shortcutDictationOnly:{name}"));
                }
                let supported = match option {
                    HotkeyOption::LeftWin | HotkeyOption::Menu => !mac,
                    HotkeyOption::Fn => mac,
                    _ => true,
                };
                if !supported {
                    return Err(format!("PARLA_ERR:shortcutUnsupported:{name}"));
                }
                // Caps Lock arrive sur Mac comme un seul evenement par appui :
                // pas de "maintenir pour parler".
                if mac && *option == HotkeyOption::CapsLock {
                    let mode = if *name == "primary" {
                        cfg.primary.mode
                    } else {
                        cfg.secondary.mode
                    };
                    if mode == HotkeyMode::PushToTalk {
                        return Err(format!("PARLA_ERR:shortcutCapsHold:{name}"));
                    }
                }
            }
            HotkeyTrigger::Modifiers {
                ctrl,
                alt,
                shift,
                win,
                ..
            } => {
                if !is_record_trigger {
                    return Err(format!("PARLA_ERR:shortcutDictationOnly:{name}"));
                }
                let count = [*ctrl, *alt, *shift, *win].iter().filter(|m| **m).count();
                if count < 2 {
                    return Err(format!("PARLA_ERR:shortcutTwoModifiers:{name}"));
                }
                // Mac : Commande + Maj sont dans trop de raccourcis (captures
                // d'ecran, Annuler / Retablir...).
                if mac && *win && *shift && !*ctrl && !*alt {
                    return Err(format!("PARLA_ERR:shortcutSystem:{name}:cmdShift"));
                }
            }
            HotkeyTrigger::None => {}
        }
    }

    for (i, (name_a, trig_a)) in entries.iter().enumerate() {
        for (name_b, trig_b) in entries.iter().skip(i + 1) {
            if trig_a == trig_b {
                return Err(format!("PARLA_ERR:shortcutConflict:{name_a}:{name_b}"));
            }
        }
    }
    Ok(())
}

fn is_plain_escape(trigger: HotkeyTrigger) -> bool {
    matches!(
        trigger,
        HotkeyTrigger::Combo {
            vk: 0x1B,
            ctrl: false,
            alt: false,
            shift: false,
            win: false,
            ..
        }
    )
}

/// Liste exhaustive des modifier-only options exposees au frontend pour
/// peupler le picker. L'ordre est aligne avec ce que VoiceInk montre dans
/// son Settings (Right Option / Left Option / Right Control / etc.).
#[command]
pub fn list_hotkey_options() -> Vec<HotkeyOptionInfo> {
    vec![
        HotkeyOptionInfo { id: HotkeyOption::None, vk: None },
        HotkeyOptionInfo { id: HotkeyOption::RightAlt, vk: HotkeyOption::RightAlt.vk() },
        HotkeyOptionInfo { id: HotkeyOption::LeftAlt, vk: HotkeyOption::LeftAlt.vk() },
        HotkeyOptionInfo { id: HotkeyOption::RightCtrl, vk: HotkeyOption::RightCtrl.vk() },
        HotkeyOptionInfo { id: HotkeyOption::LeftCtrl, vk: HotkeyOption::LeftCtrl.vk() },
        HotkeyOptionInfo { id: HotkeyOption::RightWin, vk: HotkeyOption::RightWin.vk() },
        HotkeyOptionInfo { id: HotkeyOption::RightShift, vk: HotkeyOption::RightShift.vk() },
        HotkeyOptionInfo { id: HotkeyOption::LeftShift, vk: HotkeyOption::LeftShift.vk() },
        HotkeyOptionInfo { id: HotkeyOption::LeftWin, vk: HotkeyOption::LeftWin.vk() },
        HotkeyOptionInfo { id: HotkeyOption::CapsLock, vk: HotkeyOption::CapsLock.vk() },
        HotkeyOptionInfo { id: HotkeyOption::Menu, vk: HotkeyOption::Menu.vk() },
        HotkeyOptionInfo { id: HotkeyOption::Fn, vk: HotkeyOption::Fn.vk() },
    ]
}

#[derive(Debug, Serialize)]
pub struct HotkeyOptionInfo {
    pub id: HotkeyOption,
    pub vk: Option<u32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hotkeys::keyboard_hook::Sides;

    fn combo(vk: u32, ctrl: bool, alt: bool, shift: bool) -> HotkeyTrigger {
        HotkeyTrigger::Combo {
            vk,
            ctrl,
            alt,
            shift,
            win: false,
            sides: Sides::default(),
        }
    }

    fn with_win(vk: u32, ctrl: bool, alt: bool, shift: bool) -> HotkeyTrigger {
        HotkeyTrigger::Combo {
            vk,
            ctrl,
            alt,
            shift,
            win: true,
            sides: Sides::default(),
        }
    }

    fn chord(ctrl: bool, alt: bool, shift: bool, win: bool) -> HotkeyTrigger {
        HotkeyTrigger::Modifiers {
            ctrl,
            alt,
            shift,
            win,
            sides: Sides::default(),
        }
    }

    fn single(option: HotkeyOption) -> HotkeyTrigger {
        HotkeyTrigger::Modifier { option }
    }

    fn primary(trigger: HotkeyTrigger) -> HotkeyConfig {
        let mut cfg = HotkeyConfig::defaults();
        cfg.primary.trigger = trigger;
        cfg
    }

    const WIN: bool = false;
    const MAC: bool = true;

    #[test]
    fn defaults_are_valid_and_legacy_json_loads() {
        assert!(validate(&HotkeyConfig::defaults()).is_ok());
        // Config 0.5.x sans champ `actions`.
        let legacy = serde_json::json!({
            "primary": { "trigger": { "kind": "modifier", "option": "rightAlt" }, "mode": "hybrid" },
            "secondary": { "trigger": { "kind": "none" }, "mode": "hybrid" }
        });
        let cfg: HotkeyConfig = serde_json::from_value(legacy).expect("legacy config");
        assert!(matches!(cfg.actions.copy_last_transcription, HotkeyTrigger::None));
        assert!(matches!(cfg.actions.cancel_recording, HotkeyTrigger::None));
        // 0.16: a config without the double-tap setting has it on.
        assert!(cfg.double_tap);
        // And it can be switched off and kept.
        let mut off = HotkeyConfig::defaults();
        off.double_tap = false;
        let back: HotkeyConfig =
            serde_json::from_value(serde_json::to_value(&off).unwrap()).unwrap();
        assert!(!back.double_tap);
    }

    #[test]
    fn rejects_conflict_between_actions() {
        let mut cfg = HotkeyConfig::defaults();
        cfg.actions.copy_last_transcription = combo(0x43, true, false, true);
        cfg.actions.paste_last_transcription = combo(0x43, true, false, true);
        let err = validate(&cfg).unwrap_err();
        assert_eq!(err, "PARLA_ERR:shortcutConflict:copyLast:pasteLast");
    }

    #[test]
    fn rejects_conflict_with_recording_trigger() {
        let mut cfg = HotkeyConfig::defaults();
        cfg.primary.trigger = combo(0x52, true, true, false); // Ctrl+Alt+R
        cfg.actions.retry_last_transcription = combo(0x52, true, true, false);
        let err = validate(&cfg).unwrap_err();
        assert_eq!(err, "PARLA_ERR:shortcutConflict:primary:retryLast");
    }

    #[test]
    fn rejects_reserved_alt_digit_and_bare_letter() {
        let mut cfg = HotkeyConfig::defaults();
        cfg.actions.open_history = combo(0x31, false, true, false); // Alt+1
        assert_eq!(
            validate(&cfg).unwrap_err(),
            "PARLA_ERR:shortcutReserved:openHistory"
        );
        let mut cfg = HotkeyConfig::defaults();
        cfg.actions.open_history = combo(0x48, false, false, false); // H seul
        assert_eq!(
            validate(&cfg).unwrap_err(),
            "PARLA_ERR:shortcutNeedsModifier:openHistory"
        );
    }

    #[test]
    fn accepts_function_key_alone_and_escape_for_cancel() {
        let mut cfg = HotkeyConfig::defaults();
        cfg.actions.copy_last_transcription = combo(0x7C, false, false, false); // F13
        cfg.actions.cancel_recording = combo(0x1B, false, false, false); // Esc
        assert!(validate(&cfg).is_ok());
        // Esc seul n'est pas accepte pour une autre action.
        let mut cfg = HotkeyConfig::defaults();
        cfg.actions.open_history = combo(0x1B, false, false, false);
        assert!(validate(&cfg).is_err());
    }

    #[test]
    fn record_triggers_accept_any_single_key() {
        for vk in [0x52u32, 0x20, 0x31, 0xBE, 0x60, 0x25, 0x5D] {
            let mut cfg = HotkeyConfig::defaults();
            cfg.primary.trigger = combo(vk, false, false, false);
            assert!(validate(&cfg).is_ok(), "primary vk {vk:#x}");
            let mut cfg = HotkeyConfig::defaults();
            cfg.secondary.trigger = combo(vk, false, false, false);
            assert!(validate(&cfg).is_ok(), "secondary vk {vk:#x}");
        }
    }

    #[test]
    fn escape_as_dictation_key_needs_another_cancel_shortcut() {
        let esc = combo(0x1B, false, false, false);
        // No cancel shortcut: the double-Esc would clash.
        for name in ["primary", "secondary"] {
            let mut cfg = HotkeyConfig::defaults();
            if name == "primary" {
                cfg.primary.trigger = esc;
            } else {
                cfg.secondary.trigger = esc;
            }
            assert_eq!(
                validate(&cfg).unwrap_err(),
                format!("PARLA_ERR:shortcutEscNeedsCancel:{name}")
            );
        }
        // Cancel shortcut that is Esc itself: same problem.
        let mut cfg = HotkeyConfig::defaults();
        cfg.primary.trigger = esc;
        cfg.actions.cancel_recording = esc;
        assert_eq!(
            validate(&cfg).unwrap_err(),
            "PARLA_ERR:shortcutEscNeedsCancel:primary"
        );
        // A different cancel shortcut makes it fine.
        let mut cfg = HotkeyConfig::defaults();
        cfg.primary.trigger = esc;
        cfg.actions.cancel_recording = combo(0x58, true, true, false); // Ctrl+Alt+X
        assert!(validate(&cfg).is_ok());
        // Clearing that cancel shortcut while Esc is the dictation key is refused.
        cfg.actions.cancel_recording = HotkeyTrigger::None;
        assert!(validate(&cfg).is_err());
        // Utility shortcuts still refuse a bare Esc.
        let mut cfg = HotkeyConfig::defaults();
        cfg.actions.open_history = esc;
        assert_eq!(
            validate(&cfg).unwrap_err(),
            "PARLA_ERR:shortcutNeedsModifier:openHistory"
        );
    }

    #[test]
    fn record_trigger_single_key_keeps_alt_digit_and_conflict_rules() {
        let mut cfg = HotkeyConfig::defaults();
        cfg.primary.trigger = combo(0x31, false, true, false); // Alt+1
        assert_eq!(
            validate(&cfg).unwrap_err(),
            "PARLA_ERR:shortcutReserved:primary"
        );
        let mut cfg = HotkeyConfig::defaults();
        cfg.primary.trigger = combo(0x52, false, false, false);
        cfg.secondary.trigger = combo(0x52, false, false, false);
        assert_eq!(
            validate(&cfg).unwrap_err(),
            "PARLA_ERR:shortcutConflict:primary:secondary"
        );
    }

    #[test]
    fn utility_actions_still_refuse_text_keys_alone() {
        for vk in [0x48u32, 0x20, 0x31, 0xBE] {
            let mut cfg = HotkeyConfig::defaults();
            cfg.actions.paste_last_transcription = combo(vk, false, false, false);
            assert_eq!(
                validate(&cfg).unwrap_err(),
                "PARLA_ERR:shortcutNeedsModifier:pasteLast",
                "vk {vk:#x}"
            );
        }
        let mut cfg = HotkeyConfig::defaults();
        cfg.actions.cancel_recording = combo(0x48, false, false, false);
        assert_eq!(
            validate(&cfg).unwrap_err(),
            "PARLA_ERR:shortcutNeedsModifier:cancelRecording"
        );
        let mut cfg = HotkeyConfig::defaults();
        cfg.actions.cancel_recording = combo(0x1B, false, false, false);
        assert!(validate(&cfg).is_ok());
    }

    // -- Reserved system shortcuts (0.16) -------------------------------------

    #[test]
    fn windows_system_shortcuts_are_refused_with_their_id() {
        let cases = [
            (with_win(0x4C, false, false, false), "winL"),
            (with_win(0x44, false, false, false), "winD"),
            (with_win(0x45, false, false, false), "winE"),
            (with_win(0x52, false, false, false), "winR"),
            (with_win(0x49, false, false, false), "winI"),
            (with_win(0x56, false, false, false), "winV"),
            (with_win(0xBE, false, false, false), "winPeriod"),
            (with_win(0x09, false, false, false), "winTab"),
            (with_win(0x53, false, false, true), "winShiftS"),
            (combo(0x09, false, true, false), "altTab"),
            (combo(0x73, false, true, false), "altF4"),
            (combo(0x2E, true, true, false), "ctrlAltDel"),
            (combo(0x1B, true, false, true), "ctrlShiftEsc"),
            (combo(0x43, true, false, false), "ctrlC"),
            (combo(0x56, true, false, false), "ctrlV"),
            (combo(0x58, true, false, false), "ctrlX"),
            (combo(0x5A, true, false, false), "ctrlZ"),
            (combo(0x59, true, false, false), "ctrlY"),
            (combo(0x41, true, false, false), "ctrlA"),
            (combo(0x53, true, false, false), "ctrlS"),
        ];
        for (trigger, id) in cases {
            let err = validate_on(&primary(trigger), WIN).unwrap_err();
            assert_eq!(err, format!("PARLA_ERR:shortcutSystem:primary:{id}"));
            // The same on any other shortcut of the card.
            let mut cfg = HotkeyConfig::defaults();
            cfg.actions.open_history = trigger;
            assert_eq!(
                validate_on(&cfg, WIN).unwrap_err(),
                format!("PARLA_ERR:shortcutSystem:openHistory:{id}")
            );
        }
    }

    #[test]
    fn mac_system_shortcuts_are_refused_with_their_id() {
        let cases = [
            (with_win(0x20, false, false, false), "cmdSpace"),
            (with_win(0x09, false, false, false), "cmdTab"),
            (with_win(0x33, false, false, true), "cmdShift3"),
            (with_win(0x34, false, false, true), "cmdShift4"),
            (with_win(0x35, false, false, true), "cmdShift5"),
            (with_win(0x51, true, false, false), "ctrlCmdQ"),
            (with_win(0x1B, false, true, false), "cmdOptEsc"),
        ];
        for (trigger, id) in cases {
            assert_eq!(
                validate_on(&primary(trigger), MAC).unwrap_err(),
                format!("PARLA_ERR:shortcutSystem:primary:{id}")
            );
        }
    }

    #[test]
    fn near_misses_of_system_shortcuts_are_allowed() {
        for trigger in [
            combo(0x43, true, false, true),         // Ctrl+Shift+C
            combo(0x52, true, true, false),         // Ctrl+Alt+R
            with_win(0x4C, false, true, false),     // Win+Alt+L
            combo(0x7C, false, false, false),       // F13
            combo(0x09, true, false, false),        // Ctrl+Tab
            combo(0x53, false, true, false),        // Alt+S
        ] {
            assert!(validate_on(&primary(trigger), WIN).is_ok(), "{trigger:?}");
        }
        for trigger in [
            combo(0x20, true, true, false),         // Control+Option+Space
            with_win(0x20, false, true, false),     // Command+Option+Space
            with_win(0x51, false, false, false),    // Command+Q (blocked in the UI)
        ] {
            assert!(validate_on(&primary(trigger), MAC).is_ok(), "{trigger:?}");
        }
        // A Windows shortcut is not reserved on the Mac and the other way round.
        assert!(validate_on(&primary(combo(0x43, true, false, false)), MAC).is_ok());
        assert!(validate_on(&primary(with_win(0x20, false, false, false)), WIN).is_ok());
    }

    #[test]
    fn system_shortcut_check_comes_after_the_older_rules() {
        // Alt+1 is still "reserved" (power modes), a bare letter on a utility
        // still needs a modifier, a conflict is still a conflict.
        let mut cfg = HotkeyConfig::defaults();
        cfg.actions.open_history = combo(0x31, false, true, false);
        assert_eq!(
            validate_on(&cfg, WIN).unwrap_err(),
            "PARLA_ERR:shortcutReserved:openHistory"
        );
        let mut cfg = HotkeyConfig::defaults();
        cfg.actions.copy_last_transcription = combo(0x48, false, false, false);
        assert_eq!(
            validate_on(&cfg, WIN).unwrap_err(),
            "PARLA_ERR:shortcutNeedsModifier:copyLast"
        );
    }

    // -- Two modifiers, single keys (0.16) ------------------------------------

    #[test]
    fn two_modifiers_alone_for_dictation_only() {
        assert!(validate_on(&primary(chord(true, false, false, true)), WIN).is_ok()); // Ctrl+Win
        assert!(validate_on(&primary(chord(true, true, false, false)), WIN).is_ok()); // Ctrl+Alt
        assert!(validate_on(&primary(chord(true, true, false, false)), MAC).is_ok()); // Control+Option
        let mut cfg = HotkeyConfig::defaults();
        cfg.secondary.trigger = chord(true, true, false, false);
        assert!(validate_on(&cfg, WIN).is_ok());
        // One modifier is not a chord.
        assert_eq!(
            validate_on(&primary(chord(true, false, false, false)), WIN).unwrap_err(),
            "PARLA_ERR:shortcutTwoModifiers:primary"
        );
        assert_eq!(
            validate_on(&primary(chord(false, false, false, false)), WIN).unwrap_err(),
            "PARLA_ERR:shortcutTwoModifiers:primary"
        );
        // Not for the additional shortcuts.
        let mut cfg = HotkeyConfig::defaults();
        cfg.actions.open_history = chord(true, true, false, false);
        assert_eq!(
            validate_on(&cfg, WIN).unwrap_err(),
            "PARLA_ERR:shortcutDictationOnly:openHistory"
        );
        let mut cfg = HotkeyConfig::defaults();
        cfg.actions.cancel_recording = chord(true, true, false, false);
        assert_eq!(
            validate_on(&cfg, WIN).unwrap_err(),
            "PARLA_ERR:shortcutDictationOnly:cancelRecording"
        );
    }

    #[test]
    fn command_plus_shift_alone_is_refused_on_the_mac_only() {
        assert_eq!(
            validate_on(&primary(chord(false, false, true, true)), MAC).unwrap_err(),
            "PARLA_ERR:shortcutSystem:primary:cmdShift"
        );
        assert!(validate_on(&primary(chord(false, false, true, true)), WIN).is_ok());
        // With Control or Option added it is another combination.
        assert!(validate_on(&primary(chord(true, false, true, true)), MAC).is_ok());
        assert!(validate_on(&primary(chord(false, true, true, true)), MAC).is_ok());
        assert!(validate_on(&primary(chord(true, false, false, true)), MAC).is_ok());
    }

    #[test]
    fn single_keys_by_platform() {
        // Windows: Left Win, Caps Lock, Menu. Not Fn.
        for option in [HotkeyOption::LeftWin, HotkeyOption::CapsLock, HotkeyOption::Menu] {
            assert!(validate_on(&primary(single(option)), WIN).is_ok(), "{option:?}");
        }
        assert_eq!(
            validate_on(&primary(single(HotkeyOption::Fn)), WIN).unwrap_err(),
            "PARLA_ERR:shortcutUnsupported:primary"
        );
        // Mac: Caps Lock and Fn. Not Left Command alone, not Menu.
        assert!(validate_on(&primary(single(HotkeyOption::CapsLock)), MAC).is_ok());
        assert!(validate_on(&primary(single(HotkeyOption::Fn)), MAC).is_ok());
        for option in [HotkeyOption::LeftWin, HotkeyOption::Menu] {
            assert_eq!(
                validate_on(&primary(single(option)), MAC).unwrap_err(),
                "PARLA_ERR:shortcutUnsupported:primary",
                "{option:?}"
            );
        }
        // The 0.15 single keys are valid everywhere, as before.
        for option in [
            HotkeyOption::RightAlt,
            HotkeyOption::LeftAlt,
            HotkeyOption::RightCtrl,
            HotkeyOption::LeftCtrl,
            HotkeyOption::RightWin,
            HotkeyOption::RightShift,
            HotkeyOption::LeftShift,
        ] {
            assert!(validate_on(&primary(single(option)), WIN).is_ok());
            assert!(validate_on(&primary(single(option)), MAC).is_ok());
        }
        assert_eq!(
            validate_on(&primary(single(HotkeyOption::None)), WIN).unwrap_err(),
            "PARLA_ERR:shortcutNeedsModifier:primary"
        );
    }

    #[test]
    fn new_single_keys_are_for_dictation_only() {
        for option in [HotkeyOption::CapsLock, HotkeyOption::LeftWin, HotkeyOption::Fn, HotkeyOption::Menu] {
            let mut cfg = HotkeyConfig::defaults();
            cfg.actions.retry_last_transcription = single(option);
            assert_eq!(
                validate_on(&cfg, WIN).unwrap_err(),
                "PARLA_ERR:shortcutDictationOnly:retryLast",
                "{option:?}"
            );
        }
    }

    #[test]
    fn caps_lock_on_the_mac_cannot_be_hold_to_talk() {
        let mut cfg = primary(single(HotkeyOption::CapsLock));
        cfg.primary.mode = HotkeyMode::PushToTalk;
        assert_eq!(
            validate_on(&cfg, MAC).unwrap_err(),
            "PARLA_ERR:shortcutCapsHold:primary"
        );
        // Windows can swallow the key, so hold-to-talk works there.
        assert!(validate_on(&cfg, WIN).is_ok());
        for mode in [HotkeyMode::Toggle, HotkeyMode::Hybrid] {
            cfg.primary.mode = mode;
            assert!(validate_on(&cfg, MAC).is_ok());
        }
        // The secondary slot is checked with its own mode.
        let mut cfg = HotkeyConfig::defaults();
        cfg.secondary.trigger = single(HotkeyOption::CapsLock);
        cfg.secondary.mode = HotkeyMode::PushToTalk;
        assert_eq!(
            validate_on(&cfg, MAC).unwrap_err(),
            "PARLA_ERR:shortcutCapsHold:secondary"
        );
    }

    #[test]
    fn conflict_check_still_sees_new_trigger_kinds() {
        let mut cfg = primary(chord(true, true, false, false));
        cfg.secondary.trigger = chord(true, true, false, false);
        assert_eq!(
            validate_on(&cfg, WIN).unwrap_err(),
            "PARLA_ERR:shortcutConflict:primary:secondary"
        );
        let mut cfg = primary(single(HotkeyOption::CapsLock));
        cfg.secondary.trigger = single(HotkeyOption::CapsLock);
        assert!(validate_on(&cfg, WIN).is_err());
    }

    #[test]
    fn options_list_includes_the_new_keys() {
        let ids: Vec<_> = list_hotkey_options().into_iter().map(|o| o.id).collect();
        for option in [
            HotkeyOption::LeftWin,
            HotkeyOption::CapsLock,
            HotkeyOption::Menu,
            HotkeyOption::Fn,
        ] {
            assert!(ids.contains(&option), "{option:?}");
        }
    }
}
