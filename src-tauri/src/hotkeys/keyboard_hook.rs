// Hook clavier bas niveau (WH_KEYBOARD_LL) configurable a chaud.
//
// Cree un thread dedie qui installe le hook et pompe une boucle de messages.
// Les evenements KeyDown / KeyUp des touches surveillees sont pousses dans un
// channel mpsc consomme par le HotkeyManager. Le hook vit jusqu'a la fin du
// processus mais sa table de touches surveillees (`watched`) peut etre mise
// a jour a la volee via `update_watched(...)`, sans re-installer le hook.
//
// Reference VoiceInk : HotkeyManager.swift L133-144 keyCodes modifiers macOS
// + selectedHotkey1 / selectedHotkey2 changeables a chaud.
//
// Equivalents Windows (Virtual Key Codes) :
//   rightOption  -> Right Alt   = VK_RMENU    0xA5
//   leftOption   -> Left Alt    = VK_LMENU    0xA4
//   leftControl  -> Left Ctrl   = VK_LCONTROL 0xA2
//   rightControl -> Right Ctrl  = VK_RCONTROL 0xA3
//   rightCommand -> Right Win   = VK_RWIN     0x5C
//   rightShift   -> Right Shift = VK_RSHIFT   0xA1
//   leftShift    -> Left Shift  = VK_LSHIFT   0xA0
//
// Trois sortes de triggers supportes :
//   - None     : disable ce slot (l'utilisateur n'a pas configure de hotkey).
//   - Modifier : touche modifier seule (ex Right Alt) - retro-compat 0.1.x.
//   - Combo    : combinaison libre type Ctrl+Alt+R, F13, etc.
//
// Parlato 0.16 ajoute :
//   - Combo avec `sides` : cote gauche / droit de chaque modifier (absent =
//     l'un ou l'autre, comme avant ; non ecrit dans le fichier de reglages).
//   - Modifiers : deux modifiers ou plus sans touche finale (Ctrl+Win).
//     Pressed quand le dernier descend (ensemble exact de modifiers),
//     Released quand l'un d'eux remonte. Si une autre touche (hors Echap)
//     descend pendant que c'est tenu : CancelRequested, et pas de Released.
//   - Modifier { LeftWin | CapsLock | Menu | Fn } : Left Win (Windows ; masque
//     du menu Demarrer par une touche 0xE8 injectee, ignoree par le hook),
//     Caps Lock (Windows : avale, ne bascule pas ; Mac : un evenement par
//     appui, impossible a avaler), Menu (Windows ; avale), Fn / Globe (Mac,
//     VK prive 0xFF). Seuls les nouveaux options sont avales / masques : les
//     anciens triggers gardent exactement leur comportement.
//   - Echap seul comme touche de dictee (le double-Echap est alors remplace
//     par un raccourci d'annulation personnalise, impose par validate()).

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
#[cfg_attr(not(windows), allow(unused_imports))] // debug/error/warn: Windows hook only
use tracing::{debug, error, info, warn};

#[cfg(windows)]
use windows::Win32::Foundation::{LPARAM, LRESULT, WPARAM};
#[cfg(windows)]
use windows::Win32::UI::WindowsAndMessaging::{
    CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
    UnhookWindowsHookEx, KBDLLHOOKSTRUCT, MSG, WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP,
    WM_SYSKEYDOWN, WM_SYSKEYUP,
};

#[cfg(target_os = "macos")]
#[path = "keyboard_hook_macos.rs"]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{
    accessibility_trusted, allow_accessibility_prompt, request_accessibility, PARLATO_EVENT_TAG,
};

/// Touches modifier surveillees individuellement (legacy enum, conserve pour
/// retro-compatibilite avec les tests et la config 0.1.x ou seul un modifier
/// pouvait etre choisi).
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HotkeyOption {
    #[default]
    None,
    RightAlt,
    LeftAlt,
    LeftCtrl,
    RightCtrl,
    RightWin,
    RightShift,
    LeftShift,
    // Parlato 0.16 : single keys added to the picker. LeftWin and Menu are
    // Windows only, Fn (the Globe key) is Mac only, CapsLock is on both.
    LeftWin,
    CapsLock,
    Menu,
    Fn,
}

/// Parlato 0.16 : the Fn / Globe key has no Windows Virtual Key code. The
/// Mac tap gives it this private code (never sent by the Windows hook, where
/// `HotkeyOption::Fn` has no VK at all).
pub const VK_FN: u32 = 0xFF;

impl HotkeyOption {
    pub fn vk(self) -> Option<u32> {
        match self {
            HotkeyOption::None => None,
            HotkeyOption::LeftWin => Some(0x5B),
            HotkeyOption::CapsLock => Some(0x14),
            HotkeyOption::Menu => Some(0x5D),
            HotkeyOption::Fn => {
                if cfg!(target_os = "macos") {
                    Some(VK_FN)
                } else {
                    None
                }
            }
            HotkeyOption::RightAlt => Some(0xA5),
            HotkeyOption::LeftAlt => Some(0xA4),
            HotkeyOption::LeftCtrl => Some(0xA2),
            HotkeyOption::RightCtrl => Some(0xA3),
            HotkeyOption::RightWin => Some(0x5C),
            HotkeyOption::RightShift => Some(0xA1),
            HotkeyOption::LeftShift => Some(0xA0),
        }
    }
}

/// Parlato 0.16 : which physical key of a modifier pair a trigger needs.
/// `Any` (the default, and what every config saved before 0.16 means) accepts
/// either the left or the right key.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Side {
    #[default]
    Any,
    Left,
    Right,
}

/// Parlato 0.16 : the side asked for each modifier of a trigger. All `Any`
/// is not written to the settings file, so a config without sides stays
/// byte for byte what it was.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sides {
    pub ctrl: Side,
    pub alt: Side,
    pub shift: Side,
    pub win: Side,
}

impl Sides {
    pub fn is_any(&self) -> bool {
        *self == Sides::default()
    }
}

/// Definition d'un trigger pour un slot (primary ou secondary). Sert a
/// representer les cas user : pas de trigger, modifier-only, combo libre,
/// ou (0.16) deux modifiers ou plus sans touche finale.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum HotkeyTrigger {
    #[default]
    None,
    Modifier { option: HotkeyOption },
    Combo {
        vk: u32,
        ctrl: bool,
        alt: bool,
        shift: bool,
        win: bool,
        /// Parlato 0.16 : left / right side of each modifier. Absent in old
        /// configs = either side.
        #[serde(default, skip_serializing_if = "Sides::is_any")]
        sides: Sides,
    },
    /// Parlato 0.16 : two or more modifiers alone (Ctrl+Win, Ctrl+Option).
    /// Fires when the last one goes down, released when any goes up. Only
    /// for the dictation slots (see `commands::hotkey::validate`).
    Modifiers {
        ctrl: bool,
        alt: bool,
        shift: bool,
        win: bool,
        #[serde(default, skip_serializing_if = "Sides::is_any")]
        sides: Sides,
    },
}

impl HotkeyTrigger {
    /// Default Parla 0.1.x : Right Alt en modifier seul.
    pub fn default_primary() -> Self {
        HotkeyTrigger::Modifier {
            option: HotkeyOption::RightAlt,
        }
    }
}

/// Slot identifiant qui a fire l'evenement (utilise par HotkeyManager pour
/// appliquer le mode toggle/PTT/hybrid associe).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeySlot {
    Primary,
    Secondary,
}

/// Actions utilitaires declenchables par un raccourci global, en plus des
/// deux slots d'enregistrement.
///
/// Reference VoiceInk : Features/Shortcuts/Models/ShortcutAction.swift
/// `globalUtilityActions` (pasteLastTranscription, pasteLastEnhancement,
/// retryLastTranscription, openHistoryWindow). `CopyLastTranscription` est
/// un ajout Parla : VoiceInk n'expose "Copy Last Transcription" que comme
/// accelerateur de menu (Shift+Cmd+C, actif menu ouvert uniquement), pas
/// comme raccourci global.
///
/// Une action utilitaire fire au KeyDown et l'evenement clavier est
/// consomme (VoiceInk ShortcutMonitor renvoie nil au tap CGEvent), sinon
/// Ctrl+Shift+C serait aussi recu par l'application cible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UtilityAction {
    CopyLastTranscription,
    PasteLastTranscription,
    PasteLastEnhancement,
    RetryLastTranscription,
    OpenHistory,
}

impl UtilityAction {
    pub const ALL: [UtilityAction; 5] = [
        UtilityAction::CopyLastTranscription,
        UtilityAction::PasteLastTranscription,
        UtilityAction::PasteLastEnhancement,
        UtilityAction::RetryLastTranscription,
        UtilityAction::OpenHistory,
    ];
}

#[derive(Debug, Clone)]
pub enum HotkeyEvent {
    /// Le trigger surveille a ete presse (transition KeyDown).
    Pressed {
        slot: HotkeySlot,
        timestamp: Instant,
    },
    /// Le trigger surveille a ete relache (transition KeyUp).
    Released {
        slot: HotkeySlot,
        timestamp: Instant,
    },
    /// La touche Escape a ete pressee (pour le double-tap cancel).
    EscapePressed { timestamp: Instant },
    /// Alt+chiffre : selection directe du Nieme profil Power Mode active
    /// (index 0-base). Reference VoiceInk MiniRecorderShortcutManager
    /// selectPowerMode1..10 = Option+1..0. Pas de timestamp : la selection
    /// n'a pas de logique de timing (pas de cooldown, une fire par pression
    /// physique grace au gate !was_down cote hook).
    SelectPowerMode { index: usize },
    /// Un raccourci utilitaire (coller / copier / reessayer / historique)
    /// a ete presse. Fire une fois par pression physique, evenement consomme.
    Utility { action: UtilityAction },
    /// Le raccourci d'annulation personnalise a ete presse pendant un
    /// enregistrement (remplace le double-Echap quand il est configure).
    /// Reference VoiceInk RecorderPanelShortcutManager `.cancelRecorder`.
    CancelRequested { timestamp: Instant },
}

/// Vrai pendant un enregistrement (hotkey ou tray). Sert au hook a n'armer
/// le raccourci d'annulation personnalise que lorsque le recorder est
/// visible (VoiceInk : `visibleRecorderMonitor` demarre/stoppe avec
/// `isRecorderPanelVisible`).
static RECORDING_ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn set_recording_active(active: bool) {
    RECORDING_ACTIVE.store(active, Ordering::Relaxed);
}

pub fn is_recording_active() -> bool {
    RECORDING_ACTIVE.load(Ordering::Relaxed)
}

/// Nombre de profils Power Mode selectionnables via Alt+chiffre pendant que
/// le mini-recorder est visible. 0 = raccourcis desactives (aucune touche
/// Alt+chiffre n'est capturee). Mis a jour au start/stop d'un enregistrement.
///
/// Reference VoiceInk : MiniRecorderShortcutManager n'enregistre les
/// shortcuts selectPowerMode1..10 que pendant la visibilite du recorder et
/// uniquement pour les configurations activees (enabledConfigurations).
static POWER_SHORTCUT_COUNT: AtomicUsize = AtomicUsize::new(0);

/// Active (n>0) ou desactive (n=0) la capture des raccourcis Alt+chiffre de
/// selection Power Mode, en fixant le nombre de profils adressables.
pub fn set_power_shortcut_count(n: usize) {
    POWER_SHORTCUT_COUNT.store(n, Ordering::Relaxed);
}

/// Mappe un VK de chiffre de la rangee du haut vers un index de profil.
/// 1->0, 2->1, ... 9->8, 0->9 (comme VoiceInk .one..-.zero). Le pave
/// numerique (0x60-0x69) est volontairement ignore pour ne pas interferer
/// avec les Alt-codes Windows.
fn power_mode_digit_index(vk: u32) -> Option<usize> {
    match vk {
        0x31..=0x39 => Some((vk - 0x31) as usize), // '1'..'9' -> 0..8
        0x30 => Some(9),                            // '0' -> 10e profil
        _ => None,
    }
}

#[allow(dead_code)] // certains champs uniquement utilises sous cfg(windows)
struct WatchedKeys {
    primary: HotkeyTrigger,
    secondary: HotkeyTrigger,
    /// Etat courant (down/up) pour chaque VK (0..255). Utilise par les
    /// triggers Combo qui doivent verifier les modifiers presents.
    key_state: [bool; 256],
    /// Slot actuellement en train de fire (au hold). Utile pour emettre
    /// le bon Released meme si un Combo se "casse" car un modifier est
    /// relache avant le VK final.
    primary_pressed: bool,
    secondary_pressed: bool,
    /// Timestamp du dernier LCtrl DOWN - filtre AltGr.
    last_lctrl_down: Option<Instant>,
    /// Indique qu'un RAlt DOWN a ete drop comme AltGr -> il faut drop
    /// le RAlt UP correspondant.
    altgr_in_progress: bool,
    /// Raccourcis utilitaires globaux (action -> trigger). Les entrees
    /// `HotkeyTrigger::None` sont filtrees a la mise a jour.
    utilities: Vec<(UtilityAction, HotkeyTrigger)>,
    /// Raccourci d'annulation personnalise. `None` = comportement par
    /// defaut double-Echap (gere par HotkeyManager via EscapePressed).
    cancel: HotkeyTrigger,
    /// VK final d'un combo utilitaire / cancel consomme au KeyDown : le
    /// KeyUp correspondant est consomme aussi pour que l'app cible ne
    /// voie jamais une touche "relachee sans avoir ete pressee".
    consumed_vk: Option<u32>,
}

#[cfg(any(windows, target_os = "macos"))]
struct HookContext {
    tx: mpsc::Sender<HotkeyEvent>,
    watched: Mutex<WatchedKeys>,
}

#[cfg(any(windows, target_os = "macos"))]
static HOOK_CONTEXT: OnceLock<HookContext> = OnceLock::new();

/// Installe le hook clavier bas niveau dans un thread dedie.
/// Retourne le receiver des evenements. Le hook vit jusqu'a la fin du
/// processus.
///
/// La table des touches surveillees (`HOOK_CONTEXT`) est initialisee de
/// maniere synchrone AVANT de lancer le thread, pour qu'un
/// `update_watched` / `update_utilities` appele juste apres ne tombe pas
/// dans le vide pendant que le thread demarre.
pub fn install_hook(
    primary: HotkeyTrigger,
    secondary: HotkeyTrigger,
    utilities: Vec<(UtilityAction, HotkeyTrigger)>,
    cancel: HotkeyTrigger,
) -> mpsc::Receiver<HotkeyEvent> {
    let (tx, rx) = mpsc::channel();

    #[cfg(any(windows, target_os = "macos"))]
    {
        let _ = HOOK_CONTEXT.set(HookContext {
            tx,
            watched: Mutex::new(WatchedKeys {
                primary,
                secondary,
                key_state: [false; 256],
                primary_pressed: false,
                secondary_pressed: false,
                last_lctrl_down: None,
                altgr_in_progress: false,
                utilities: utilities
                    .into_iter()
                    .filter(|(_, t)| !matches!(t, HotkeyTrigger::None))
                    .collect(),
                cancel,
                consumed_vk: None,
            }),
        });
        #[cfg(windows)]
        let _ = std::thread::Builder::new()
            .name("parla-hotkey-hook".into())
            .spawn(run_hook_thread);
        #[cfg(target_os = "macos")]
        macos::spawn_tap_thread();
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (tx, primary, secondary, utilities, cancel);
    }

    rx
}

/// Met a jour les triggers surveilles sans re-installer le hook. Appele
/// quand l'utilisateur change son raccourci dans Settings.
pub fn update_watched(primary: HotkeyTrigger, secondary: HotkeyTrigger) {
    #[cfg(any(windows, target_os = "macos"))]
    {
        if let Some(ctx) = HOOK_CONTEXT.get() {
            let mut g = ctx.watched.lock();
            // Reset les flags pressed pour eviter qu'un Released orphan
            // ne fire avec un nouveau slot.
            g.primary_pressed = false;
            g.secondary_pressed = false;
            g.primary = primary;
            g.secondary = secondary;
            info!(
                primary = ?primary,
                secondary = ?secondary,
                "Hotkeys mises a jour a chaud"
            );
        }
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (primary, secondary);
    }
}

/// Met a jour les raccourcis utilitaires et le raccourci d'annulation
/// personnalise sans re-installer le hook. Meme contrat que
/// `update_watched` : appele au boot et a chaque sauvegarde de la config.
pub fn update_utilities(utilities: Vec<(UtilityAction, HotkeyTrigger)>, cancel: HotkeyTrigger) {
    #[cfg(any(windows, target_os = "macos"))]
    {
        if let Some(ctx) = HOOK_CONTEXT.get() {
            let mut g = ctx.watched.lock();
            g.consumed_vk = None;
            g.utilities = utilities
                .into_iter()
                .filter(|(_, t)| !matches!(t, HotkeyTrigger::None))
                .collect();
            g.cancel = cancel;
            info!(
                utilities = g.utilities.len(),
                cancel = ?g.cancel,
                "Raccourcis utilitaires mis a jour a chaud"
            );
        }
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (utilities, cancel);
    }
}

#[cfg(windows)]
fn run_hook_thread() {
    let hook = unsafe { SetWindowsHookExW(WH_KEYBOARD_LL, Some(low_level_proc), None, 0) };
    let hook = match hook {
        Ok(h) => h,
        Err(e) => {
            error!("SetWindowsHookExW a echoue: {e:?}");
            return;
        }
    };

    debug!("Hook WH_KEYBOARD_LL installe");

    unsafe {
        let mut msg = MSG::default();
        loop {
            let ret = GetMessageW(&mut msg, None, 0, 0);
            match ret.0 {
                -1 => {
                    error!("GetMessageW a retourne -1");
                    break;
                }
                0 => break,
                _ => {
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }
        }
        if let Err(e) = UnhookWindowsHookEx(hook) {
            warn!("UnhookWindowsHookEx: {e:?}");
        }
    }

    debug!("Thread hook clavier termine");
}

#[cfg(windows)]
unsafe extern "system" fn low_level_proc(
    n_code: i32,
    w_param: WPARAM,
    l_param: LPARAM,
) -> LRESULT {
    if n_code >= 0 {
        let info = &*(l_param.0 as *const KBDLLHOOKSTRUCT);
        // Parlato 0.16 : the Start menu masking key press is ours, ignore it
        // (and never take the table lock for it: SendInput may call this
        // hook again before the first call has returned).
        if info.dwExtraInfo == MASK_TAG {
            return CallNextHookEx(None, n_code, w_param, l_param);
        }
        let vk = info.vkCode;
        let message = w_param.0 as u32;
        let is_down = message == WM_KEYDOWN || message == WM_SYSKEYDOWN;
        let is_up = message == WM_KEYUP || message == WM_SYSKEYUP;

        if is_down || is_up {
            if let Some(ctx) = HOOK_CONTEXT.get() {
                if handle_key(ctx, vk, is_down) {
                    // Evenement consomme (Alt+chiffre Power Mode) : on ne le
                    // propage pas a l'app cible, sinon le chiffre serait tape
                    // dans le document sous le curseur.
                    return LRESULT(1);
                }
            }
        }
    }
    CallNextHookEx(None, n_code, w_param, l_param)
}

/// Fenetre de detection AltGr : Windows synthetise un LCtrl DOWN 0-1ms
/// avant chaque RAlt DOWN sur AZERTY/QWERTZ. 5ms est large vs le delai
/// observable et bien sous ce qu'un humain peut faire a la main.
#[cfg(any(windows, target_os = "macos"))]
const ALTGR_GATE: Duration = Duration::from_millis(5);

const VK_LCONTROL: u32 = 0xA2;
const VK_RCONTROL: u32 = 0xA3;
const VK_LMENU: u32 = 0xA4;
const VK_RMENU: u32 = 0xA5;
const VK_LSHIFT: u32 = 0xA0;
const VK_RSHIFT: u32 = 0xA1;
const VK_LWIN: u32 = 0x5B;
const VK_RWIN: u32 = 0x5C;
const VK_ESCAPE: u32 = 0x1B;

///
/// Retourne `true` si l'evenement doit etre consomme (non propage a l'app
/// cible). C'est le cas pour Alt+chiffre de selection Power Mode, pour les
/// raccourcis utilitaires (coller / copier / reessayer / historique) et pour
/// le raccourci d'annulation personnalise ; les triggers record et Escape
/// retournent `false` pour preserver le comportement historique (l'app
/// recoit bien la touche).
#[cfg(any(windows, target_os = "macos"))]
fn handle_key(ctx: &HookContext, vk: u32, is_down: bool) -> bool {
    let now = Instant::now();
    let mut watched = ctx.watched.lock();
    let idx = vk as usize & 0xFF;
    let was_down = watched.key_state[idx];
    watched.key_state[idx] = is_down;

    if vk == VK_LCONTROL && is_down {
        watched.last_lctrl_down = Some(now);
    }

    // Filtre AltGr : RAlt synthetise apres LCtrl en moins de 5ms.
    if vk == VK_RMENU {
        if is_down {
            let is_altgr = watched
                .last_lctrl_down
                .map(|t| now.duration_since(t) < ALTGR_GATE)
                .unwrap_or(false);
            if is_altgr {
                watched.altgr_in_progress = true;
                return false;
            }
        } else if watched.altgr_in_progress {
            watched.altgr_in_progress = false;
            return false;
        }
    }

    // KeyUp du VK final d'un combo consomme au KeyDown : consomme aussi.
    if !is_down && watched.consumed_vk == Some(vk) {
        watched.consumed_vk = None;
        return true;
    }

    // Raccourci d'annulation personnalise : uniquement pendant un
    // enregistrement (VoiceInk : monitor actif tant que le recorder est
    // visible). Quand il est configure, il remplace le double-Echap.
    if is_down && !was_down && is_recording_active() {
        let mods_now = current_modifiers(&watched.key_state);
        if !matches!(watched.cancel, HotkeyTrigger::None)
            && trigger_matches(watched.cancel, vk, mods_now, &watched.key_state)
        {
            watched.consumed_vk = Some(vk);
            let _ = ctx.tx.send(HotkeyEvent::CancelRequested { timestamp: now });
            return true;
        }
    }

    // Raccourcis utilitaires globaux (VoiceInk ShortcutMonitor : evenement
    // supprime quand un raccourci matche). Une fire par pression physique.
    if is_down && !was_down && !watched.utilities.is_empty() {
        let mods_now = current_modifiers(&watched.key_state);
        let hit = watched
            .utilities
            .iter()
            .find(|(_, t)| trigger_matches(*t, vk, mods_now, &watched.key_state))
            .map(|(a, _)| *a);
        if let Some(action) = hit {
            watched.consumed_vk = Some(vk);
            let _ = ctx.tx.send(HotkeyEvent::Utility { action });
            return true;
        }
    }

    // Escape pour double-tap cancel - independant des triggers.
    if vk == VK_ESCAPE && is_down && !was_down {
        let _ = ctx.tx.send(HotkeyEvent::EscapePressed { timestamp: now });
        // Parlato 0.16 : a plain Esc used as a dictation key must still reach
        // the slot matching below (validate() only allows it when a custom
        // cancel shortcut replaces the double-Esc, so the manager ignores the
        // EscapePressed above). Any other Esc keeps returning here, as before.
        if !is_plain_escape_trigger(watched.primary) && !is_plain_escape_trigger(watched.secondary)
        {
            return false;
        }
    }

    // Selection Power Mode par Alt+chiffre pendant que le mini-recorder est
    // visible (POWER_SHORTCUT_COUNT > 0). VoiceInk : selectPowerMode1..10 =
    // Option+1..0. On ne capture QUE Alt seul (sans Ctrl/Shift/Win) pour ne
    // pas voler AltGr+chiffre (AZERTY, Ctrl est down) ni les combos
    // applicatifs, et on laisse passer si le chiffre correspond deja a un
    // hotkey record configure (pour ne pas hijacker le raccourci de l'user).
    let count = POWER_SHORTCUT_COUNT.load(Ordering::Relaxed);
    if count > 0 && is_down && !was_down {
        if let Some(index) = power_mode_digit_index(vk) {
            let mods = current_modifiers(&watched.key_state);
            let is_alt_only = mods.alt && !mods.ctrl && !mods.shift && !mods.win;
            let clashes_trigger = trigger_matches(watched.primary, vk, mods, &watched.key_state)
                || trigger_matches(watched.secondary, vk, mods, &watched.key_state);
            if is_alt_only && !clashes_trigger && index < count {
                let _ = ctx.tx.send(HotkeyEvent::SelectPowerMode { index });
                return true;
            }
        }
    }

    // Match du VK contre les deux slots. Important : on teste primary
    // ET secondary, on n'arrete pas au premier match (l'utilisateur
    // peut configurer deux triggers qui partagent un VK terminal mais
    // avec des modifiers differents - cas rare mais autorise).
    let primary = watched.primary;
    let secondary = watched.secondary;
    let mods = current_modifiers(&watched.key_state);

    // Parlato 0.16 : a modifier-only trigger that is held (two modifiers, or
    // Left Win) when a normal key goes down is the start of an ordinary
    // shortcut (Ctrl+Win+Right, Win+E), not dictation: drop the press and
    // cancel the recording it started. The older single-modifier triggers
    // keep their behaviour (on the Mac the tap does the same for them, see
    // keyboard_hook_macos.rs).
    if is_down && !is_modifier_like(vk) && vk != VK_ESCAPE {
        let mut cancel = false;
        if watched.primary_pressed && cancels_on_chord(primary) {
            watched.primary_pressed = false;
            cancel = true;
        }
        if watched.secondary_pressed && cancels_on_chord(secondary) {
            watched.secondary_pressed = false;
            cancel = true;
        }
        if cancel {
            debug!("chord started with a modifier-only hotkey, dictation press dropped");
            let _ = ctx.tx.send(HotkeyEvent::CancelRequested { timestamp: now });
        }
    }

    let primary_matches = trigger_matches(primary, vk, mods, &watched.key_state);
    let secondary_matches = trigger_matches(secondary, vk, mods, &watched.key_state);

    // Pour un Combo, un Pressed fire UNIQUEMENT quand le VK final tombe
    // ET tous les modifiers requis sont down. Un Released fire quand le
    // VK final remonte OU n'importe quel modifier requis remonte
    // (sinon on serait bloque en hold).

    if primary_matches && is_down && !watched.primary_pressed {
        watched.primary_pressed = true;
        let _ = ctx.tx.send(HotkeyEvent::Pressed {
            slot: HotkeySlot::Primary,
            timestamp: now,
        });
        if needs_start_menu_mask(primary) {
            mask_start_menu();
        }
    } else if watched.primary_pressed && release_check(primary, vk, is_down, &watched.key_state) {
        watched.primary_pressed = false;
        let _ = ctx.tx.send(HotkeyEvent::Released {
            slot: HotkeySlot::Primary,
            timestamp: now,
        });
    }

    if secondary_matches && is_down && !watched.secondary_pressed {
        watched.secondary_pressed = true;
        let _ = ctx.tx.send(HotkeyEvent::Pressed {
            slot: HotkeySlot::Secondary,
            timestamp: now,
        });
        if needs_start_menu_mask(secondary) {
            mask_start_menu();
        }
    } else if watched.secondary_pressed && release_check(secondary, vk, is_down, &watched.key_state) {
        watched.secondary_pressed = false;
        let _ = ctx.tx.send(HotkeyEvent::Released {
            slot: HotkeySlot::Secondary,
            timestamp: now,
        });
    }

    // Aucun evenement Power Mode consomme : on laisse la touche se propager.
    // Parlato 0.16 : sauf Caps Lock / Menu utilises comme touche de dictee
    // (Windows : evite le basculement des majuscules / le menu contextuel).
    swallows_key(primary, secondary, vk)
}

/// Parlato : noms des modifiers selon la plateforme (Option / Command sur
/// macOS), dans l'ordre Ctrl, Alt, Shift, Win.
#[cfg(not(target_os = "macos"))]
const MODIFIER_NAMES: [&str; 4] = ["Ctrl", "Alt", "Shift", "Win"];
#[cfg(target_os = "macos")]
const MODIFIER_NAMES: [&str; 4] = ["Control", "Option", "Shift", "Command"];

#[cfg(not(target_os = "macos"))]
fn option_name(option: HotkeyOption) -> Option<&'static str> {
    Some(match option {
        HotkeyOption::None => return None,
        HotkeyOption::RightAlt => "Right Alt",
        HotkeyOption::LeftAlt => "Left Alt",
        HotkeyOption::LeftCtrl => "Left Ctrl",
        HotkeyOption::RightCtrl => "Right Ctrl",
        HotkeyOption::RightWin => "Right Win",
        HotkeyOption::RightShift => "Right Shift",
        HotkeyOption::LeftShift => "Left Shift",
        HotkeyOption::LeftWin => "Left Win",
        HotkeyOption::CapsLock => "Caps Lock",
        HotkeyOption::Menu => "Menu",
        HotkeyOption::Fn => "Fn",
    })
}

#[cfg(target_os = "macos")]
fn option_name(option: HotkeyOption) -> Option<&'static str> {
    Some(match option {
        HotkeyOption::None => return None,
        HotkeyOption::RightAlt => "Right Option",
        HotkeyOption::LeftAlt => "Left Option",
        HotkeyOption::LeftCtrl => "Left Control",
        HotkeyOption::RightCtrl => "Right Control",
        HotkeyOption::RightWin => "Right Command",
        HotkeyOption::RightShift => "Right Shift",
        HotkeyOption::LeftShift => "Left Shift",
        HotkeyOption::LeftWin => "Left Command",
        HotkeyOption::CapsLock => "Caps Lock",
        HotkeyOption::Menu => "Menu",
        HotkeyOption::Fn => "Fn",
    })
}

/// Parlato 0.16 : name of one modifier of a chord, with its side when the
/// trigger asks for one ("Left Ctrl"). `kind` is 0 ctrl, 1 alt, 2 shift, 3 win.
fn modifier_label(kind: usize, side: Side) -> String {
    let option = match (kind, side) {
        (0, Side::Left) => Some(HotkeyOption::LeftCtrl),
        (0, Side::Right) => Some(HotkeyOption::RightCtrl),
        (1, Side::Left) => Some(HotkeyOption::LeftAlt),
        (1, Side::Right) => Some(HotkeyOption::RightAlt),
        (2, Side::Left) => Some(HotkeyOption::LeftShift),
        (2, Side::Right) => Some(HotkeyOption::RightShift),
        (3, Side::Left) => Some(HotkeyOption::LeftWin),
        (3, Side::Right) => Some(HotkeyOption::RightWin),
        _ => None,
    };
    match option.and_then(option_name) {
        Some(name) => name.to_string(),
        None => MODIFIER_NAMES[kind].to_string(),
    }
}

fn chord_labels(required: [bool; 4], sides: &Sides) -> Vec<String> {
    (0..4)
        .filter(|k| required[*k])
        .map(|k| modifier_label(k, side_of(sides, k)))
        .collect()
}

/// Libelle lisible d'un trigger, pour le menu tray et les badges UI.
/// Format Windows : "Ctrl+Shift+C", "F13", "Right Alt" (macOS : "Control",
/// "Option", "Command"). `None` pour un trigger `None`.
pub fn trigger_label(trigger: HotkeyTrigger) -> Option<String> {
    match trigger {
        HotkeyTrigger::None => None,
        HotkeyTrigger::Modifier { option } => option_name(option).map(str::to_string),
        HotkeyTrigger::Combo {
            vk,
            ctrl,
            alt,
            shift,
            win,
            sides,
        } => {
            let mut parts = chord_labels([ctrl, alt, shift, win], &sides);
            parts.push(vk_name(vk));
            Some(parts.join("+"))
        }
        HotkeyTrigger::Modifiers {
            ctrl,
            alt,
            shift,
            win,
            sides,
        } => Some(chord_labels([ctrl, alt, shift, win], &sides).join("+")),
    }
}

/// Nom d'une touche a partir de son Virtual Key code. Aligne sur `vkLabel`
/// de src/components/HotkeyRecorder.tsx (noms anglais) pour que le tray et
/// les reglages affichent le meme libelle.
pub fn vk_name(vk: u32) -> String {
    // Parlato : Mac keyboard names for the keys that differ.
    #[cfg(target_os = "macos")]
    {
        let mac = match vk {
            0x0D => Some("Return"),
            0x08 => Some("Delete"),
            0x2E => Some("Forward Delete"),
            0x2D => Some("Help"),
            0x0C => Some("Clear"),
            _ => None,
        };
        if let Some(name) = mac {
            return name.to_string();
        }
    }
    let fixed = match vk {
        0x08 => Some("Backspace"),
        0x09 => Some("Tab"),
        0x0D => Some("Enter"),
        0x13 => Some("Pause"),
        0x14 => Some("Caps Lock"),
        0x1B => Some("Esc"),
        0x20 => Some("Space"),
        0x21 => Some("Page Up"),
        0x22 => Some("Page Down"),
        0x23 => Some("End"),
        0x24 => Some("Home"),
        0x25 => Some("Left"),
        0x26 => Some("Up"),
        0x27 => Some("Right"),
        0x28 => Some("Down"),
        0x2C => Some("Print Screen"),
        0x2D => Some("Insert"),
        0x2E => Some("Delete"),
        0x5D => Some("Menu"),
        0x90 => Some("Num Lock"),
        0x91 => Some("Scroll Lock"),
        0xAD => Some("Volume Mute"),
        0xAE => Some("Volume Down"),
        0xAF => Some("Volume Up"),
        0xB0 => Some("Next Track"),
        0xB1 => Some("Previous Track"),
        0xB3 => Some("Play/Pause"),
        0xBA => Some(";"),
        0xBB => Some("="),
        0xBC => Some(","),
        0xBD => Some("-"),
        0xBE => Some("."),
        0xBF => Some("/"),
        0xC0 => Some("`"),
        0xDB => Some("["),
        0xDC => Some("\\"),
        0xDD => Some("]"),
        0xDE => Some("'"),
        0xE2 => Some("<>"),
        0x6A => Some("Num *"),
        0x6B => Some("Num +"),
        0x6D => Some("Num -"),
        0x6E => Some("Num ."),
        0x6F => Some("Num /"),
        _ => None,
    };
    if let Some(f) = fixed {
        return f.to_string();
    }
    if (0x30..=0x39).contains(&vk) || (0x41..=0x5A).contains(&vk) {
        return char::from_u32(vk).map(|c| c.to_string()).unwrap_or_default();
    }
    if (0x70..=0x87).contains(&vk) {
        return format!("F{}", vk - 0x70 + 1);
    }
    if (0x60..=0x69).contains(&vk) {
        return format!("Num {}", vk - 0x60);
    }
    format!("VK_{:02X}", vk)
}

/// Etat courant des modifiers (lus depuis `key_state`).
#[derive(Debug, Clone, Copy)]
struct Modifiers {
    ctrl: bool,
    alt: bool,
    shift: bool,
    win: bool,
}

fn current_modifiers(key_state: &[bool; 256]) -> Modifiers {
    Modifiers {
        ctrl: key_state[VK_LCONTROL as usize] || key_state[VK_RCONTROL as usize],
        alt: key_state[VK_LMENU as usize] || key_state[VK_RMENU as usize],
        shift: key_state[VK_LSHIFT as usize] || key_state[VK_RSHIFT as usize],
        win: key_state[VK_LWIN as usize] || key_state[VK_RWIN as usize],
    }
}

/// Parlato 0.16 : index of a modifier VK in (ctrl, alt, shift, win), or None.
fn modifier_kind(vk: u32) -> Option<usize> {
    match vk {
        VK_LCONTROL | VK_RCONTROL => Some(0),
        VK_LMENU | VK_RMENU => Some(1),
        VK_LSHIFT | VK_RSHIFT => Some(2),
        VK_LWIN | VK_RWIN => Some(3),
        _ => None,
    }
}

fn is_modifier_vk(vk: u32) -> bool {
    modifier_kind(vk).is_some()
}

/// Modifier-like keys that do not make a chord "a shortcut with a key":
/// the four modifiers, Caps Lock and the Mac Fn key.
fn is_modifier_like(vk: u32) -> bool {
    is_modifier_vk(vk) || vk == 0x14 || vk == VK_FN
}

/// (left, right) VK of each modifier, in (ctrl, alt, shift, win) order.
const MODIFIER_PAIRS: [(u32, u32); 4] = [
    (VK_LCONTROL, VK_RCONTROL),
    (VK_LMENU, VK_RMENU),
    (VK_LSHIFT, VK_RSHIFT),
    (VK_LWIN, VK_RWIN),
];

fn side_of(sides: &Sides, kind: usize) -> Side {
    match kind {
        0 => sides.ctrl,
        1 => sides.alt,
        2 => sides.shift,
        _ => sides.win,
    }
}

/// Is modifier `kind` held, on the side asked for? `Left` needs the left key
/// down and the right one up (and the other way round), `Any` either.
fn side_satisfied(side: Side, key_state: &[bool; 256], kind: usize) -> bool {
    let (l, r) = MODIFIER_PAIRS[kind];
    let (l, r) = (key_state[l as usize], key_state[r as usize]);
    match side {
        Side::Any => l || r,
        Side::Left => l && !r,
        Side::Right => r && !l,
    }
}

/// Every required modifier is held on the side asked for (extra modifiers
/// are not looked at here). With all sides `Any` this is exactly the old
/// "required modifier still down" test.
fn required_held(required: [bool; 4], sides: &Sides, key_state: &[bool; 256]) -> bool {
    (0..4).all(|k| !required[k] || side_satisfied(side_of(sides, k), key_state, k))
}

/// Teste si un trigger se declenche pour ce VK + les modifiers actifs.
fn trigger_matches(
    trigger: HotkeyTrigger,
    vk: u32,
    mods: Modifiers,
    key_state: &[bool; 256],
) -> bool {
    match trigger {
        HotkeyTrigger::None => false,
        HotkeyTrigger::Modifier { option } => option.vk() == Some(vk),
        HotkeyTrigger::Combo {
            vk: combo_vk,
            ctrl,
            alt,
            shift,
            win,
            sides,
        } => {
            // Pour matcher un combo : le VK final doit correspondre ET
            // tous les modifiers requis doivent etre down. Les modifiers
            // SANS check (ex pas requis) doivent ne pas etre presents
            // pour eviter des matches accidentels (Ctrl+R != Ctrl+Alt+R).
            // Parlato 0.16 : si un cote est demande (gauche / droite), la
            // touche de ce cote doit etre celle qui est enfoncee.
            combo_vk == vk
                && ctrl == mods.ctrl
                && alt == mods.alt
                && shift == mods.shift
                && win == mods.win
                && (sides.is_any() || required_held([ctrl, alt, shift, win], &sides, key_state))
        }
        // Parlato 0.16 : the event must be a modifier going down that is part
        // of the chord, and the modifiers held must be exactly the chord.
        HotkeyTrigger::Modifiers {
            ctrl,
            alt,
            shift,
            win,
            sides,
        } => match modifier_kind(vk) {
            Some(kind) => {
                [ctrl, alt, shift, win][kind]
                    && ctrl == mods.ctrl
                    && alt == mods.alt
                    && shift == mods.shift
                    && win == mods.win
                    && required_held([ctrl, alt, shift, win], &sides, key_state)
            }
            None => false,
        },
    }
}

/// Verifie si un evenement correspond a un release du trigger en cours.
/// Pour un Modifier : release du VK lui-meme. Pour un Combo : release
/// du VK final OU release d'un des modifiers requis. Pour deux modifiers
/// (0.16) : release de l'un d'eux.
fn release_check(
    trigger: HotkeyTrigger,
    vk: u32,
    is_down: bool,
    key_state: &[bool; 256],
) -> bool {
    if is_down {
        return false;
    }
    match trigger {
        HotkeyTrigger::None => false,
        HotkeyTrigger::Modifier { option } => option.vk() == Some(vk),
        HotkeyTrigger::Combo {
            vk: combo_vk,
            ctrl,
            alt,
            shift,
            win,
            sides,
        } => {
            if combo_vk == vk {
                return true;
            }
            // Si l'un des modifiers requis remonte, le combo casse.
            !required_held([ctrl, alt, shift, win], &sides, key_state)
        }
        HotkeyTrigger::Modifiers {
            ctrl,
            alt,
            shift,
            win,
            sides,
        } => is_modifier_vk(vk) && !required_held([ctrl, alt, shift, win], &sides, key_state),
    }
}

/// Parlato 0.16 : plain Esc (no modifier) as a dictation key.
fn is_plain_escape_trigger(trigger: HotkeyTrigger) -> bool {
    matches!(
        trigger,
        HotkeyTrigger::Combo {
            vk: VK_ESCAPE,
            ctrl: false,
            alt: false,
            shift: false,
            win: false,
            ..
        }
    )
}

/// Parlato 0.16 : triggers made only of modifiers, for which a normal key
/// pressed while they are held means "this was the start of a shortcut".
fn cancels_on_chord(trigger: HotkeyTrigger) -> bool {
    matches!(
        trigger,
        HotkeyTrigger::Modifiers { .. }
            | HotkeyTrigger::Modifier {
                option: HotkeyOption::LeftWin
            }
    )
}

/// Parlato 0.16 : a trigger holding the Windows key alone would open the
/// Start menu on release, so Windows gets a masking key press (see
/// `mask_start_menu`). Only the triggers new in 0.16: Right Win keeps its
/// 0.15 behaviour.
fn needs_start_menu_mask(trigger: HotkeyTrigger) -> bool {
    matches!(
        trigger,
        HotkeyTrigger::Modifier {
            option: HotkeyOption::LeftWin
        } | HotkeyTrigger::Modifiers { win: true, .. }
    )
}

/// Parlato 0.16 : Caps Lock and the Menu key, when they are a dictation key,
/// are swallowed (Windows) so Caps Lock does not change the case and the Menu
/// key does not open a context menu. Only the new `Modifier` options: a Menu
/// key saved as a custom combo keeps passing through, as before.
fn swallows_key(primary: HotkeyTrigger, secondary: HotkeyTrigger, vk: u32) -> bool {
    [primary, secondary].into_iter().any(|t| {
        matches!(
            t,
            HotkeyTrigger::Modifier {
                option: HotkeyOption::CapsLock | HotkeyOption::Menu
            }
        ) && matches!(t, HotkeyTrigger::Modifier { option } if option.vk() == Some(vk))
    })
}

/// Tag put in `dwExtraInfo` of the key presses Parlato injects itself, so the
/// hook ignores them.
#[cfg(windows)]
const MASK_TAG: usize = 0x5041_524C; // "PARL"

/// Parlato 0.16 (Windows): tap the unassigned key 0xE8 while the Windows key
/// is down. Windows only opens the Start menu when the Win key goes up
/// without any other key in between, so this keeps Left Win usable as a
/// dictation key (the same trick AutoHotkey uses).
#[cfg(windows)]
fn mask_start_menu() {
    use windows::Win32::UI::Input::KeyboardAndMouse::{
        SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP,
        VIRTUAL_KEY,
    };

    fn ki(flags: KEYBD_EVENT_FLAGS) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(0xE8),
                    wScan: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: MASK_TAG,
                },
            },
        }
    }
    let inputs = [ki(KEYBD_EVENT_FLAGS(0)), ki(KEYEVENTF_KEYUP)];
    let sent = unsafe { SendInput(&inputs, std::mem::size_of::<INPUT>() as i32) };
    if sent as usize != inputs.len() {
        debug!("masque du menu Demarrer : SendInput {sent}/{}", inputs.len());
    }
}

#[cfg(all(not(windows), target_os = "macos"))]
fn mask_start_menu() {}

#[cfg(test)]
mod tests {
    use super::{
        cancels_on_chord, is_plain_escape_trigger, needs_start_menu_mask, power_mode_digit_index,
        release_check, swallows_key, trigger_label, trigger_matches, vk_name, HotkeyOption,
        HotkeyTrigger, Modifiers, Side, Sides,
    };
    #[cfg(target_os = "macos")]
    use super::VK_FN;

    const LCTRL: u32 = 0xA2;
    const RCTRL: u32 = 0xA3;
    const LALT: u32 = 0xA4;
    const RALT: u32 = 0xA5;
    const LSHIFT: u32 = 0xA0;
    const LWIN: u32 = 0x5B;
    const RWIN: u32 = 0x5C;

    fn state(down: &[u32]) -> [bool; 256] {
        let mut s = [false; 256];
        for vk in down {
            s[*vk as usize] = true;
        }
        s
    }

    fn mods(s: &[bool; 256]) -> Modifiers {
        super::current_modifiers(s)
    }

    fn combo(vk: u32, ctrl: bool, alt: bool, shift: bool, win: bool) -> HotkeyTrigger {
        HotkeyTrigger::Combo {
            vk,
            ctrl,
            alt,
            shift,
            win,
            sides: Sides::default(),
        }
    }

    fn chord(ctrl: bool, alt: bool, shift: bool, win: bool, sides: Sides) -> HotkeyTrigger {
        HotkeyTrigger::Modifiers {
            ctrl,
            alt,
            shift,
            win,
            sides,
        }
    }

    /// Does `trigger` fire when `vk` goes down with `down` (including `vk`) held?
    fn fires(trigger: HotkeyTrigger, vk: u32, down: &[u32]) -> bool {
        let s = state(down);
        trigger_matches(trigger, vk, mods(&s), &s)
    }

    #[test]
    fn trigger_label_formats_combo_and_modifier() {
        let c = combo(0x43, true, false, true, false);
        #[cfg(not(target_os = "macos"))]
        assert_eq!(trigger_label(c).as_deref(), Some("Ctrl+Shift+C"));
        #[cfg(target_os = "macos")]
        assert_eq!(trigger_label(c).as_deref(), Some("Control+Shift+C"));
        let f13 = combo(0x7C, false, false, false, false);
        assert_eq!(trigger_label(f13).as_deref(), Some("F13"));
        let m = HotkeyTrigger::Modifier {
            option: HotkeyOption::RightAlt,
        };
        #[cfg(not(target_os = "macos"))]
        assert_eq!(trigger_label(m).as_deref(), Some("Right Alt"));
        #[cfg(target_os = "macos")]
        assert_eq!(trigger_label(m).as_deref(), Some("Right Option"));
        assert_eq!(trigger_label(HotkeyTrigger::None), None);
    }

    #[test]
    fn trigger_label_names_sides_and_chords() {
        let left_ctrl_k = HotkeyTrigger::Combo {
            vk: 0x4B,
            ctrl: true,
            alt: false,
            shift: false,
            win: false,
            sides: Sides {
                ctrl: Side::Left,
                ..Sides::default()
            },
        };
        #[cfg(not(target_os = "macos"))]
        {
            assert_eq!(trigger_label(left_ctrl_k).as_deref(), Some("Left Ctrl+K"));
            assert_eq!(
                trigger_label(chord(true, false, false, true, Sides::default())).as_deref(),
                Some("Ctrl+Win")
            );
            let right_alt_left_ctrl = chord(
                true,
                true,
                false,
                false,
                Sides {
                    ctrl: Side::Left,
                    alt: Side::Right,
                    ..Sides::default()
                },
            );
            assert_eq!(
                trigger_label(right_alt_left_ctrl).as_deref(),
                Some("Left Ctrl+Right Alt")
            );
        }
        #[cfg(target_os = "macos")]
        {
            assert_eq!(trigger_label(left_ctrl_k).as_deref(), Some("Left Control+K"));
            assert_eq!(
                trigger_label(chord(true, true, false, false, Sides::default())).as_deref(),
                Some("Control+Option")
            );
        }
    }

    #[test]
    fn vk_name_covers_common_keys() {
        assert_eq!(vk_name(0x41), "A");
        assert_eq!(vk_name(0x30), "0");
        assert_eq!(vk_name(0x70), "F1");
        assert_eq!(vk_name(0x87), "F24");
        assert_eq!(vk_name(0x20), "Space");
        // 0.16: the names match vkLabel in HotkeyRecorder.tsx.
        assert_eq!(vk_name(0xBA), ";");
        assert_eq!(vk_name(0xBF), "/");
        assert_eq!(vk_name(0xDC), "\\");
        assert_eq!(vk_name(0xE2), "<>");
        assert_eq!(vk_name(0x6A), "Num *");
        assert_eq!(vk_name(0x65), "Num 5");
        assert_eq!(vk_name(0x14), "Caps Lock");
        assert_eq!(vk_name(0x21), "Page Up");
        assert_eq!(vk_name(0x2C), "Print Screen");
        assert_eq!(vk_name(0x5D), "Menu");
        assert_eq!(vk_name(0xAF), "Volume Up");
        // Unknown codes keep the raw name.
        assert_eq!(vk_name(0xE8), "VK_E8");
    }

    #[test]
    fn digit_index_maps_top_row_1_to_9() {
        // '1'..'9' (VK 0x31..0x39) -> index 0..8 (VoiceInk selectPowerMode1..9).
        for (vk, expected) in (0x31u32..=0x39).zip(0usize..) {
            assert_eq!(power_mode_digit_index(vk), Some(expected));
        }
    }

    #[test]
    fn digit_index_maps_zero_to_tenth_slot() {
        // '0' (VK 0x30) adresse le 10e profil -> index 9 (selectPowerMode10).
        assert_eq!(power_mode_digit_index(0x30), Some(9));
    }

    #[test]
    fn digit_index_ignores_numpad_and_other_keys() {
        // Pave numerique (0x60-0x69) ignore pour ne pas capter les Alt-codes.
        assert_eq!(power_mode_digit_index(0x60), None); // Numpad 0
        assert_eq!(power_mode_digit_index(0x69), None); // Numpad 9
        // Lettres et autres VK non concernes.
        assert_eq!(power_mode_digit_index(0x41), None); // 'A'
        assert_eq!(power_mode_digit_index(0x1B), None); // Escape
    }

    // -- Old behaviour, unchanged ---------------------------------------

    #[test]
    fn single_modifier_trigger_matches_only_its_own_key() {
        let t = HotkeyTrigger::Modifier {
            option: HotkeyOption::RightAlt,
        };
        assert!(fires(t, RALT, &[RALT]));
        assert!(!fires(t, LALT, &[LALT]));
        // Extra modifiers do not matter for a single-modifier trigger (0.15).
        assert!(fires(t, RALT, &[LCTRL, RALT]));
    }

    #[test]
    fn combo_requires_the_exact_modifier_set() {
        let t = combo(0x52, true, true, false, false); // Ctrl+Alt+R
        assert!(fires(t, 0x52, &[LCTRL, LALT, 0x52]));
        assert!(fires(t, 0x52, &[RCTRL, RALT, 0x52]));
        assert!(!fires(t, 0x52, &[LCTRL, 0x52]));
        assert!(!fires(t, 0x52, &[LCTRL, LALT, LSHIFT, 0x52]));
        assert!(!fires(t, 0x53, &[LCTRL, LALT, 0x53]));
    }

    #[test]
    fn combo_without_sides_accepts_either_side() {
        let t = combo(0x4B, true, false, false, false); // Ctrl+K, any side
        assert!(fires(t, 0x4B, &[LCTRL, 0x4B]));
        assert!(fires(t, 0x4B, &[RCTRL, 0x4B]));
        assert!(fires(t, 0x4B, &[LCTRL, RCTRL, 0x4B]));
    }

    #[test]
    fn old_config_json_loads_and_saves_unchanged() {
        // A combo saved by 0.15 has no `sides`.
        let old = serde_json::json!({
            "kind": "combo", "vk": 82, "ctrl": true, "alt": true, "shift": false, "win": false
        });
        let t: HotkeyTrigger = serde_json::from_value(old.clone()).expect("old combo");
        assert_eq!(t, combo(82, true, true, false, false));
        // Written back byte for byte: no `sides` key when every side is Any.
        assert_eq!(serde_json::to_value(t).unwrap(), old);
        let m: HotkeyTrigger =
            serde_json::from_value(serde_json::json!({"kind": "modifier", "option": "rightAlt"}))
                .unwrap();
        assert_eq!(
            m,
            HotkeyTrigger::Modifier {
                option: HotkeyOption::RightAlt
            }
        );
    }

    // -- Left / right sides ---------------------------------------------

    #[test]
    fn combo_with_a_side_needs_that_side_only() {
        let left = HotkeyTrigger::Combo {
            vk: 0x4B,
            ctrl: true,
            alt: false,
            shift: false,
            win: false,
            sides: Sides {
                ctrl: Side::Left,
                ..Sides::default()
            },
        };
        assert!(fires(left, 0x4B, &[LCTRL, 0x4B]));
        assert!(!fires(left, 0x4B, &[RCTRL, 0x4B]));
        // Both Ctrl keys down: not "the left one".
        assert!(!fires(left, 0x4B, &[LCTRL, RCTRL, 0x4B]));
        let right = HotkeyTrigger::Combo {
            vk: 0x4B,
            ctrl: true,
            alt: false,
            shift: false,
            win: false,
            sides: Sides {
                ctrl: Side::Right,
                ..Sides::default()
            },
        };
        assert!(fires(right, 0x4B, &[RCTRL, 0x4B]));
        assert!(!fires(right, 0x4B, &[LCTRL, 0x4B]));
    }

    #[test]
    fn sides_serialize_only_when_set_and_round_trip() {
        let t = HotkeyTrigger::Combo {
            vk: 0x4B,
            ctrl: true,
            alt: false,
            shift: false,
            win: false,
            sides: Sides {
                ctrl: Side::Right,
                ..Sides::default()
            },
        };
        let v = serde_json::to_value(t).unwrap();
        assert_eq!(v["sides"]["ctrl"], "right");
        assert_eq!(v["sides"]["alt"], "any");
        let back: HotkeyTrigger = serde_json::from_value(v).unwrap();
        assert_eq!(back, t);
        // A different side is a different trigger (conflict detection).
        assert_ne!(t, combo(0x4B, true, false, false, false));
    }

    #[test]
    fn combo_release_follows_the_side() {
        let left = HotkeyTrigger::Combo {
            vk: 0x4B,
            ctrl: true,
            alt: false,
            shift: false,
            win: false,
            sides: Sides {
                ctrl: Side::Left,
                ..Sides::default()
            },
        };
        // Left Ctrl up while Right Ctrl is down: the combo is over.
        assert!(release_check(left, LCTRL, false, &state(&[RCTRL, 0x4B])));
        // Final key up.
        assert!(release_check(left, 0x4B, false, &state(&[LCTRL])));
        // Unrelated key up: still held.
        assert!(!release_check(left, 0x41, false, &state(&[LCTRL, 0x4B])));
        // Old combo (any side): release of the held Ctrl only when none is left.
        let any = combo(0x4B, true, false, false, false);
        assert!(!release_check(any, LCTRL, false, &state(&[RCTRL, 0x4B])));
        assert!(release_check(any, LCTRL, false, &state(&[0x4B])));
        // Never a release on a key down.
        assert!(!release_check(any, 0x4B, true, &state(&[LCTRL, 0x4B])));
    }

    // -- Two modifiers alone --------------------------------------------

    #[test]
    fn chord_fires_when_the_second_modifier_goes_down() {
        let t = chord(true, false, false, true, Sides::default()); // Ctrl+Win
        // First modifier alone: not yet.
        assert!(!fires(t, LCTRL, &[LCTRL]));
        // Second modifier completes it, in either order, either side.
        assert!(fires(t, LWIN, &[LCTRL, LWIN]));
        assert!(fires(t, LCTRL, &[LWIN, LCTRL]));
        assert!(fires(t, RWIN, &[RCTRL, RWIN]));
    }

    #[test]
    fn chord_needs_exactly_its_modifiers() {
        let t = chord(true, false, false, true, Sides::default());
        // A third modifier: it is another shortcut (Ctrl+Shift+Win).
        assert!(!fires(t, LWIN, &[LCTRL, LSHIFT, LWIN]));
        // Wrong pair.
        assert!(!fires(t, LALT, &[LCTRL, LALT]));
        // A non-modifier key never completes a chord.
        assert!(!fires(t, 0x41, &[LCTRL, LWIN, 0x41]));
    }

    #[test]
    fn chord_sides_are_respected() {
        let t = chord(
            true,
            true,
            false,
            false,
            Sides {
                ctrl: Side::Left,
                alt: Side::Right,
                ..Sides::default()
            },
        );
        assert!(fires(t, RALT, &[LCTRL, RALT]));
        assert!(!fires(t, LALT, &[LCTRL, LALT]));
        assert!(!fires(t, RALT, &[RCTRL, RALT]));
    }

    #[test]
    fn chord_is_released_when_either_modifier_goes_up() {
        let t = chord(true, true, false, false, Sides::default()); // Ctrl+Alt
        assert!(release_check(t, LCTRL, false, &state(&[LALT])));
        assert!(release_check(t, LALT, false, &state(&[LCTRL])));
        // Both still held after an unrelated modifier goes up: keep holding.
        assert!(!release_check(t, LSHIFT, false, &state(&[LCTRL, LALT])));
        // Other Ctrl key still down: Ctrl is still held.
        assert!(!release_check(t, LCTRL, false, &state(&[RCTRL, LALT])));
        // A normal key going up never ends the chord.
        assert!(!release_check(t, 0x41, false, &state(&[LCTRL, LALT])));
        assert!(!release_check(t, LCTRL, true, &state(&[LCTRL, LALT])));
    }

    #[test]
    fn chord_json_round_trip() {
        let t = chord(true, false, false, true, Sides::default());
        let v = serde_json::to_value(t).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"kind": "modifiers", "ctrl": true, "alt": false, "shift": false, "win": true})
        );
        assert_eq!(serde_json::from_value::<HotkeyTrigger>(v).unwrap(), t);
    }

    #[test]
    fn chord_cancel_and_start_menu_predicates() {
        assert!(cancels_on_chord(chord(true, true, false, false, Sides::default())));
        assert!(cancels_on_chord(HotkeyTrigger::Modifier {
            option: HotkeyOption::LeftWin
        }));
        // The 0.15 single modifiers keep their behaviour on Windows.
        for option in [
            HotkeyOption::RightAlt,
            HotkeyOption::RightWin,
            HotkeyOption::LeftShift,
            HotkeyOption::CapsLock,
            HotkeyOption::Menu,
        ] {
            assert!(!cancels_on_chord(HotkeyTrigger::Modifier { option }), "{option:?}");
        }
        assert!(!cancels_on_chord(combo(0x52, true, false, false, false)));
        // Start menu mask: only the Windows-key triggers added in 0.16.
        assert!(needs_start_menu_mask(HotkeyTrigger::Modifier {
            option: HotkeyOption::LeftWin
        }));
        assert!(needs_start_menu_mask(chord(true, false, false, true, Sides::default())));
        assert!(!needs_start_menu_mask(chord(true, true, false, false, Sides::default())));
        assert!(!needs_start_menu_mask(HotkeyTrigger::Modifier {
            option: HotkeyOption::RightWin
        }));
        assert!(!needs_start_menu_mask(combo(0x45, false, false, false, true)));
    }

    // -- Single keys added in 0.16 --------------------------------------

    #[test]
    fn new_single_keys_have_their_vk() {
        assert_eq!(HotkeyOption::LeftWin.vk(), Some(0x5B));
        assert_eq!(HotkeyOption::CapsLock.vk(), Some(0x14));
        assert_eq!(HotkeyOption::Menu.vk(), Some(0x5D));
        #[cfg(target_os = "macos")]
        assert_eq!(HotkeyOption::Fn.vk(), Some(VK_FN));
        // No Fn on Windows: it never matches a key.
        #[cfg(not(target_os = "macos"))]
        assert_eq!(HotkeyOption::Fn.vk(), None);
        let t = HotkeyTrigger::Modifier {
            option: HotkeyOption::LeftWin,
        };
        assert!(fires(t, LWIN, &[LWIN]));
        assert!(!fires(t, RWIN, &[RWIN]));
        // JSON names used by the UI.
        for (option, name) in [
            (HotkeyOption::LeftWin, "leftWin"),
            (HotkeyOption::CapsLock, "capsLock"),
            (HotkeyOption::Menu, "menu"),
            (HotkeyOption::Fn, "fn"),
        ] {
            assert_eq!(serde_json::to_value(option).unwrap(), name);
        }
    }

    #[test]
    fn caps_lock_and_menu_are_swallowed_only_as_the_new_options() {
        let caps = HotkeyTrigger::Modifier {
            option: HotkeyOption::CapsLock,
        };
        let menu = HotkeyTrigger::Modifier {
            option: HotkeyOption::Menu,
        };
        let none = HotkeyTrigger::None;
        assert!(swallows_key(caps, none, 0x14));
        assert!(swallows_key(none, caps, 0x14));
        assert!(swallows_key(menu, none, 0x5D));
        // Not for another key, and not for the other slot's key.
        assert!(!swallows_key(caps, none, 0x5D));
        assert!(!swallows_key(menu, none, 0x14));
        // A Menu key saved as a custom combo (0.15) keeps passing through.
        assert!(!swallows_key(combo(0x5D, false, false, false, false), none, 0x5D));
        // Other single keys are never swallowed.
        let ralt = HotkeyTrigger::Modifier {
            option: HotkeyOption::RightAlt,
        };
        assert!(!swallows_key(ralt, none, RALT));
        assert!(!swallows_key(none, none, 0x14));
    }

    #[test]
    fn plain_escape_detection() {
        assert!(is_plain_escape_trigger(combo(0x1B, false, false, false, false)));
        assert!(!is_plain_escape_trigger(combo(0x1B, true, false, false, false)));
        assert!(!is_plain_escape_trigger(combo(0x20, false, false, false, false)));
        assert!(!is_plain_escape_trigger(HotkeyTrigger::None));
    }

    // -- handle_key end to end (the hook table, without the OS) -----------

    #[cfg(any(windows, target_os = "macos"))]
    mod flow {
        use super::*;
        use crate::hotkeys::keyboard_hook::{
            handle_key, HookContext, HotkeyEvent, HotkeySlot, WatchedKeys,
        };
        use parking_lot::Mutex;
        use std::sync::mpsc;

        fn context(
            primary: HotkeyTrigger,
            secondary: HotkeyTrigger,
        ) -> (HookContext, mpsc::Receiver<HotkeyEvent>) {
            let (tx, rx) = mpsc::channel();
            let ctx = HookContext {
                tx,
                watched: Mutex::new(WatchedKeys {
                    primary,
                    secondary,
                    key_state: [false; 256],
                    primary_pressed: false,
                    secondary_pressed: false,
                    last_lctrl_down: None,
                    altgr_in_progress: false,
                    utilities: Vec::new(),
                    cancel: HotkeyTrigger::None,
                    consumed_vk: None,
                }),
            };
            (ctx, rx)
        }

        /// Short names of what the hook sent since the last call.
        fn drain(rx: &mpsc::Receiver<HotkeyEvent>) -> Vec<&'static str> {
            let mut out = Vec::new();
            while let Ok(e) = rx.try_recv() {
                out.push(match e {
                    HotkeyEvent::Pressed {
                        slot: HotkeySlot::Primary,
                        ..
                    } => "pressed",
                    HotkeyEvent::Pressed {
                        slot: HotkeySlot::Secondary,
                        ..
                    } => "pressed2",
                    HotkeyEvent::Released {
                        slot: HotkeySlot::Primary,
                        ..
                    } => "released",
                    HotkeyEvent::Released {
                        slot: HotkeySlot::Secondary,
                        ..
                    } => "released2",
                    HotkeyEvent::EscapePressed { .. } => "escape",
                    HotkeyEvent::CancelRequested { .. } => "cancel",
                    HotkeyEvent::SelectPowerMode { .. } => "power",
                    HotkeyEvent::Utility { .. } => "utility",
                });
            }
            out
        }

        #[test]
        fn chord_press_and_release() {
            let (ctx, rx) = context(chord(true, true, false, false, Sides::default()), HotkeyTrigger::None);
            assert!(!handle_key(&ctx, LCTRL, true));
            assert!(drain(&rx).is_empty());
            assert!(!handle_key(&ctx, LALT, true));
            assert_eq!(drain(&rx), ["pressed"]);
            // Auto-repeat of the held modifier does not fire again.
            handle_key(&ctx, LALT, true);
            assert!(drain(&rx).is_empty());
            // Releasing either one releases the dictation key.
            handle_key(&ctx, LALT, false);
            assert_eq!(drain(&rx), ["released"]);
            handle_key(&ctx, LCTRL, false);
            assert!(drain(&rx).is_empty());
            // Pressing again starts a new cycle.
            handle_key(&ctx, LCTRL, true);
            handle_key(&ctx, LALT, true);
            assert_eq!(drain(&rx), ["pressed"]);
        }

        #[test]
        fn chord_with_a_normal_key_is_cancelled_and_not_released() {
            let (ctx, rx) = context(chord(true, true, false, false, Sides::default()), HotkeyTrigger::None);
            handle_key(&ctx, LCTRL, true);
            handle_key(&ctx, LALT, true);
            assert_eq!(drain(&rx), ["pressed"]);
            // Ctrl+Alt+A is a shortcut, not dictation.
            assert!(!handle_key(&ctx, 0x41, true));
            assert_eq!(drain(&rx), ["cancel"]);
            handle_key(&ctx, 0x41, false);
            handle_key(&ctx, LALT, false);
            handle_key(&ctx, LCTRL, false);
            assert!(drain(&rx).is_empty(), "no orphan release after a cancel");
        }

        #[test]
        fn escape_while_a_chord_is_held_keeps_the_double_escape_meaning() {
            let (ctx, rx) = context(chord(true, true, false, false, Sides::default()), HotkeyTrigger::None);
            handle_key(&ctx, LCTRL, true);
            handle_key(&ctx, LALT, true);
            drain(&rx);
            handle_key(&ctx, 0x1B, true);
            assert_eq!(drain(&rx), ["escape"]);
        }

        #[test]
        fn old_single_modifier_is_not_cancelled_by_this_code_path() {
            // The 0.15 Right Alt trigger: a key pressed while it is held does
            // nothing here (the Mac tap handles that case separately).
            let (ctx, rx) = context(
                HotkeyTrigger::Modifier {
                    option: HotkeyOption::LeftAlt,
                },
                HotkeyTrigger::None,
            );
            handle_key(&ctx, LALT, true);
            assert_eq!(drain(&rx), ["pressed"]);
            handle_key(&ctx, 0x41, true);
            assert!(drain(&rx).is_empty());
            handle_key(&ctx, LALT, false);
            assert_eq!(drain(&rx), ["released"]);
        }

        #[test]
        fn old_combo_still_fires_and_releases() {
            let (ctx, rx) = context(combo(0x52, true, true, false, false), HotkeyTrigger::None);
            handle_key(&ctx, LCTRL, true);
            handle_key(&ctx, LALT, true);
            assert!(drain(&rx).is_empty());
            assert!(!handle_key(&ctx, 0x52, true), "the dictation key is not swallowed");
            assert_eq!(drain(&rx), ["pressed"]);
            handle_key(&ctx, 0x52, false);
            assert_eq!(drain(&rx), ["released"]);
        }

        #[test]
        fn left_ctrl_combo_ignores_right_ctrl() {
            let left = HotkeyTrigger::Combo {
                vk: 0x4B,
                ctrl: true,
                alt: false,
                shift: false,
                win: false,
                sides: Sides {
                    ctrl: Side::Left,
                    ..Sides::default()
                },
            };
            let (ctx, rx) = context(left, HotkeyTrigger::None);
            handle_key(&ctx, RCTRL, true);
            handle_key(&ctx, 0x4B, true);
            assert!(drain(&rx).is_empty());
            handle_key(&ctx, 0x4B, false);
            handle_key(&ctx, RCTRL, false);
            handle_key(&ctx, LCTRL, true);
            handle_key(&ctx, 0x4B, true);
            assert_eq!(drain(&rx), ["pressed"]);
        }

        #[test]
        fn plain_escape_as_dictation_key_reaches_the_slot() {
            let (ctx, rx) = context(combo(0x1B, false, false, false, false), HotkeyTrigger::None);
            assert!(!handle_key(&ctx, 0x1B, true));
            // The manager ignores the EscapePressed (custom cancel is set).
            assert_eq!(drain(&rx), ["escape", "pressed"]);
            handle_key(&ctx, 0x1B, false);
            assert_eq!(drain(&rx), ["released"]);
        }

        #[test]
        fn escape_with_a_modifier_still_only_sends_the_escape_event() {
            // Ctrl+Esc as a dictation combo never worked (the Esc branch
            // returns first); 0.16 keeps that.
            let (ctx, rx) = context(combo(0x1B, true, false, false, false), HotkeyTrigger::None);
            handle_key(&ctx, LCTRL, true);
            handle_key(&ctx, 0x1B, true);
            assert_eq!(drain(&rx), ["escape"]);
        }

        #[test]
        fn escape_is_just_escape_when_it_is_not_a_trigger() {
            let (ctx, rx) = context(combo(0x52, true, true, false, false), HotkeyTrigger::None);
            assert!(!handle_key(&ctx, 0x1B, true));
            assert_eq!(drain(&rx), ["escape"]);
        }

        #[test]
        fn caps_lock_trigger_fires_and_is_swallowed() {
            let (ctx, rx) = context(
                HotkeyTrigger::Modifier {
                    option: HotkeyOption::CapsLock,
                },
                HotkeyTrigger::None,
            );
            assert!(handle_key(&ctx, 0x14, true), "down swallowed");
            assert_eq!(drain(&rx), ["pressed"]);
            assert!(handle_key(&ctx, 0x14, true), "auto-repeat swallowed");
            assert!(handle_key(&ctx, 0x14, false), "up swallowed");
            assert_eq!(drain(&rx), ["released"]);
            // Other keys are untouched.
            assert!(!handle_key(&ctx, 0x41, true));
        }

        #[test]
        fn caps_lock_is_not_swallowed_when_it_is_not_the_dictation_key() {
            let (ctx, rx) = context(
                HotkeyTrigger::Modifier {
                    option: HotkeyOption::RightAlt,
                },
                HotkeyTrigger::None,
            );
            assert!(!handle_key(&ctx, 0x14, true));
            assert!(!handle_key(&ctx, 0x14, false));
            assert!(drain(&rx).is_empty());
        }

        #[test]
        fn secondary_slot_chord_uses_its_own_events() {
            let (ctx, rx) = context(
                combo(0x52, true, false, false, false),
                chord(true, false, true, false, Sides::default()),
            );
            handle_key(&ctx, LCTRL, true);
            handle_key(&ctx, LSHIFT, true);
            assert_eq!(drain(&rx), ["pressed2"]);
            handle_key(&ctx, LSHIFT, false);
            assert_eq!(drain(&rx), ["released2"]);
        }
    }
}
