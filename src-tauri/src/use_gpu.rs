// Parlato : le reglage "Utiliser la carte graphique" (`use_gpu`), un seul pour
// la parole et l'IA locale.
//
// - La regle (`wants_card`) est une fonction pure : systeme, sorte de modele
//   et reglage. Parole et IA locale n'ont pas d'autre endroit ou decider.
// - Le defaut quand la personne n'a jamais choisi : Mac = oui ; Windows = oui
//   seulement si le PC a une carte graphique dediee
//   (`voice_engine_stt::discrete_card`, DXGI, jusqu'a 10 s), cherchee une fois
//   par lancement sur un fil, jamais bloquante : en attendant, non. Le defaut
//   n'est PAS ecrit dans le fichier (une carte ajoutee plus tard compte) ;
//   seul un choix de la personne l'est.
// - Migration, une fois : `parakeet_gpu == true` devient `use_gpu = true`,
//   puis `parakeet_gpu` est supprime.
//
// Journal : le nom de la carte, rien d'autre.

use parking_lot::Mutex;
use serde_json::{Map, Value};
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;
use tracing::{info, warn};
use voice_engine_models::Kind;

const STORE_FILE: &str = "parla.settings.json";
/// Le reglage (bool). Absent tant que la personne n'a pas touche l'interrupteur.
pub const KEY: &str = "use_gpu";
/// L'ancien reglage de Parakeet, migre puis supprime.
pub const OLD_KEY: &str = "parakeet_gpu";

// ---------------------------------------------------------------------------
// La regle
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Mac,
    Windows,
    Other,
}

impl Os {
    pub const fn current() -> Os {
        if cfg!(target_os = "macos") {
            Os::Mac
        } else if cfg!(windows) {
            Os::Windows
        } else {
            Os::Other
        }
    }
}

/// Ce qui demande la carte graphique.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Runtime {
    Whisper,
    /// Parakeet TDT (v2, v3, Ultra).
    ParakeetTdt,
    /// Parakeet Unified, Nemotron, voix : le catalogue n'a pas de chemin carte.
    OtherOnnx,
    LocalAi,
}

pub fn runtime_for(kind: Kind) -> Runtime {
    match kind {
        Kind::Whisper => Runtime::Whisper,
        Kind::Parakeet => Runtime::ParakeetTdt,
        _ => Runtime::OtherOnnx,
    }
}

/// Faut-il demander la carte graphique ? `use_gpu` est la valeur effective du
/// reglage (choix ou defaut).
/// - Reglage coupe : jamais.
/// - Mac : Whisper (Metal) et l'IA locale. Les modeles ONNX n'ont pas de
///   chemin carte : le moteur dirait `fell_back`, on ne les demande jamais.
/// - Windows : Parakeet TDT (DirectML) et l'IA locale (assistant Vulkan s'il
///   est la). Whisper reste sur le processeur dans ce processus (cette
///   version) ; Unified et Nemotron n'ont pas de chemin carte.
pub fn wants_card(os: Os, runtime: Runtime, use_gpu: bool) -> bool {
    use Runtime::*;
    use_gpu
        && matches!(
            (os, runtime),
            (Os::Mac, Whisper | LocalAi) | (Os::Windows, ParakeetTdt | LocalAi)
        )
}

/// La regle, pour ce PC et le reglage effectif du moment.
pub fn wanted_now(runtime: Runtime) -> bool {
    wants_card(Os::current(), runtime, enabled())
}

// ---------------------------------------------------------------------------
// Le defaut
// ---------------------------------------------------------------------------

/// Ce que la recherche de carte a dit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Card {
    /// Pas encore de reponse (jusqu'a 10 s).
    Checking,
    /// Une carte dediee, par son nom.
    Found(String),
    /// Aucune carte, ou une erreur.
    Absent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    Mac,
    DiscreteCard,
    NoCard,
    Checking,
    SetByPerson,
}

impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Reason::Mac => "mac",
            Reason::DiscreteCard => "discrete_card",
            Reason::NoCard => "no_card",
            Reason::Checking => "checking",
            Reason::SetByPerson => "set_by_person",
        }
    }
}

/// La valeur effective du reglage et pourquoi. `stored` : le choix de la
/// personne s'il y en a un.
pub fn resolve(stored: Option<bool>, os: Os, card: &Card) -> (bool, Reason) {
    if let Some(v) = stored {
        return (v, Reason::SetByPerson);
    }
    match os {
        Os::Mac => (true, Reason::Mac),
        Os::Windows => match card {
            Card::Checking => (false, Reason::Checking),
            Card::Found(_) => (true, Reason::DiscreteCard),
            Card::Absent => (false, Reason::NoCard),
        },
        Os::Other => (false, Reason::NoCard),
    }
}

/// Le nom de la carte d'un `Device` du moteur (ce que le pilote lui donne).
pub fn card_name(description: &str, name: &str) -> String {
    let d = description.trim();
    if d.is_empty() { name.trim() } else { d }.to_string()
}

// ---------------------------------------------------------------------------
// Migration
// ---------------------------------------------------------------------------

/// Migre `parakeet_gpu` dans `map` (les cles du fichier de reglages). Vrai si
/// quelque chose a change. Une fois : l'ancienne cle disparait.
/// - `parakeet_gpu == true` : `use_gpu = true`, sauf si la personne a deja un
///   `use_gpu` (son choix gagne).
/// - sinon : `use_gpu` reste absent, le defaut s'applique.
pub fn migrate(map: &mut Map<String, Value>) -> bool {
    let Some(old) = map.remove(OLD_KEY) else {
        return false;
    };
    if old.as_bool() == Some(true) && !map.contains_key(KEY) {
        map.insert(KEY.into(), Value::Bool(true));
    }
    true
}

/// Applique `migrate` au fichier de reglages.
pub fn migrate_store(app: &AppHandle) {
    let Ok(store) = app.store(STORE_FILE) else {
        return;
    };
    let mut keys = Map::new();
    for k in [OLD_KEY, KEY] {
        if let Some(v) = store.get(k) {
            keys.insert(k.to_string(), v);
        }
    }
    if !migrate(&mut keys) {
        return;
    }
    store.delete(OLD_KEY);
    if let Some(v) = keys.get(KEY) {
        store.set(KEY, v.clone());
    }
    if let Err(e) = store.save() {
        warn!("migration de parakeet_gpu : enregistrement impossible : {e}");
    } else {
        info!("reglage parakeet_gpu migre vers use_gpu");
    }
}

// ---------------------------------------------------------------------------
// L'etat de l'application
// ---------------------------------------------------------------------------

struct State {
    /// Le choix de la personne (None = jamais choisi).
    stored: Option<bool>,
    card: Card,
}

static STATE: Mutex<State> = Mutex::new(State {
    stored: None,
    card: Card::Checking,
});

/// La valeur effective maintenant : ce que la parole et l'IA locale lisent.
pub fn enabled() -> bool {
    snapshot().0
}

/// (valeur effective, pourquoi, choix, nom de la carte).
pub fn snapshot() -> (bool, Reason, Option<String>) {
    let s = STATE.lock();
    let (on, why) = resolve(s.stored, Os::current(), &s.card);
    let name = match &s.card {
        Card::Found(n) => Some(n.clone()),
        _ => None,
    };
    (on, why, name)
}

/// Au lancement : migre l'ancien reglage, lit le choix, lance la recherche de
/// carte sur un fil (Windows). A appeler avant tout chargement de modele.
pub fn init(app: &AppHandle) {
    migrate_store(app);
    let stored = app
        .store(STORE_FILE)
        .ok()
        .and_then(|s| s.get(KEY))
        .and_then(|v| v.as_bool());
    STATE.lock().stored = stored;
    if Os::current() != Os::Windows {
        STATE.lock().card = Card::Absent;
        return;
    }
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("parlato-card-probe".into())
        .spawn(move || {
            let before = enabled();
            let card = probe();
            match &card {
                Card::Found(name) => info!(card = %name, "carte graphique dediee trouvee"),
                _ => info!("pas de carte graphique dediee"),
            }
            STATE.lock().card = card;
            if enabled() != before {
                // Le defaut vient de changer : l'IA locale se recharge avec la
                // carte ; la parole le fait a la prochaine dictee.
                crate::enhancement::local_ai::preload_if_wanted(&app);
            }
        });
    if spawned.is_err() {
        STATE.lock().card = Card::Absent;
    }
}

/// Windows, DXGI et DXCore : aucun modele charge, aucun assistant lance.
/// Une erreur ou un panic vaut "pas de carte".
fn probe() -> Card {
    match std::panic::catch_unwind(voice_engine_stt::discrete_card) {
        Ok(Some(d)) => Card::Found(card_name(&d.description, &d.name)),
        _ => Card::Absent,
    }
}

/// Le choix de la personne : ecrit dans le fichier et pris en compte tout de
/// suite. Les modeles charges sont decharges par l'appelant.
pub fn set(app: &AppHandle, enabled: bool) -> anyhow::Result<()> {
    let store = app.store(STORE_FILE)?;
    store.set(KEY, Value::Bool(enabled));
    store.save()?;
    STATE.lock().stored = Some(enabled);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const KINDS: [Runtime; 4] = [
        Runtime::Whisper,
        Runtime::ParakeetTdt,
        Runtime::OtherOnnx,
        Runtime::LocalAi,
    ];

    #[test]
    fn off_means_off_everywhere() {
        for os in [Os::Mac, Os::Windows, Os::Other] {
            for k in KINDS {
                assert!(!wants_card(os, k, false), "{os:?} {k:?}");
            }
        }
    }

    #[test]
    fn the_rule_for_every_system_and_kind_when_on() {
        use Runtime::*;
        let table = [
            (Os::Mac, Whisper, true),
            (Os::Mac, ParakeetTdt, false),
            (Os::Mac, OtherOnnx, false),
            (Os::Mac, LocalAi, true),
            (Os::Windows, Whisper, false),
            (Os::Windows, ParakeetTdt, true),
            (Os::Windows, OtherOnnx, false),
            (Os::Windows, LocalAi, true),
            (Os::Other, Whisper, false),
            (Os::Other, ParakeetTdt, false),
            (Os::Other, OtherOnnx, false),
            (Os::Other, LocalAi, false),
        ];
        for (os, k, want) in table {
            assert_eq!(wants_card(os, k, true), want, "{os:?} {k:?}");
        }
    }

    #[test]
    fn model_kinds_map_to_runtimes() {
        assert_eq!(runtime_for(Kind::Whisper), Runtime::Whisper);
        assert_eq!(runtime_for(Kind::Parakeet), Runtime::ParakeetTdt);
        assert_eq!(runtime_for(Kind::ParakeetUnified), Runtime::OtherOnnx);
        assert_eq!(runtime_for(Kind::Nemotron), Runtime::OtherOnnx);
        assert_eq!(runtime_for(Kind::Vad), Runtime::OtherOnnx);
    }

    #[test]
    fn default_on_the_mac_is_on() {
        assert_eq!(resolve(None, Os::Mac, &Card::Absent), (true, Reason::Mac));
        assert_eq!(resolve(None, Os::Mac, &Card::Checking), (true, Reason::Mac));
    }

    #[test]
    fn default_on_windows_follows_the_card_and_waits_as_off() {
        let name = Card::Found("NVIDIA GeForce RTX 5070 Ti".into());
        assert_eq!(resolve(None, Os::Windows, &name), (true, Reason::DiscreteCard));
        assert_eq!(resolve(None, Os::Windows, &Card::Absent), (false, Reason::NoCard));
        assert_eq!(resolve(None, Os::Windows, &Card::Checking), (false, Reason::Checking));
        assert_eq!(resolve(None, Os::Other, &name), (false, Reason::NoCard));
    }

    #[test]
    fn a_choice_beats_the_default_and_the_card() {
        let name = Card::Found("RTX".into());
        assert_eq!(resolve(Some(false), Os::Windows, &name), (false, Reason::SetByPerson));
        assert_eq!(resolve(Some(true), Os::Windows, &Card::Absent), (true, Reason::SetByPerson));
        assert_eq!(resolve(Some(false), Os::Mac, &Card::Absent), (false, Reason::SetByPerson));
        assert_eq!(Reason::SetByPerson.as_str(), "set_by_person");
        assert_eq!(Reason::DiscreteCard.as_str(), "discrete_card");
        assert_eq!(Reason::NoCard.as_str(), "no_card");
        assert_eq!(Reason::Checking.as_str(), "checking");
        assert_eq!(Reason::Mac.as_str(), "mac");
    }

    #[test]
    fn the_card_is_named_by_its_description() {
        assert_eq!(card_name(" NVIDIA GeForce RTX 5070 Ti ", "DirectML1"), "NVIDIA GeForce RTX 5070 Ti");
        assert_eq!(card_name("", "DirectML1"), "DirectML1");
    }

    fn map(v: Value) -> Map<String, Value> {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn migration_true_becomes_use_gpu_true_and_the_old_key_goes() {
        let mut m = map(json!({ "parakeet_gpu": true, "other": 1 }));
        assert!(migrate(&mut m));
        assert_eq!(m, map(json!({ "use_gpu": true, "other": 1 })));
    }

    #[test]
    fn migration_false_leaves_use_gpu_unset() {
        let mut m = map(json!({ "parakeet_gpu": false }));
        assert!(migrate(&mut m));
        assert_eq!(m, map(json!({})));
        let mut m = map(json!({ "parakeet_gpu": "yes" }));
        assert!(migrate(&mut m));
        assert!(!m.contains_key(KEY) && !m.contains_key(OLD_KEY));
    }

    #[test]
    fn migration_without_the_old_key_does_nothing() {
        let mut m = map(json!({}));
        assert!(!migrate(&mut m));
        assert!(m.is_empty());
        let mut m = map(json!({ "use_gpu": false }));
        assert!(!migrate(&mut m));
        assert_eq!(m, map(json!({ "use_gpu": false })));
    }

    #[test]
    fn migration_never_overrides_a_choice_and_runs_once() {
        let mut m = map(json!({ "parakeet_gpu": true, "use_gpu": false }));
        assert!(migrate(&mut m));
        assert_eq!(m, map(json!({ "use_gpu": false })));
        // A second start finds nothing to do.
        assert!(!migrate(&mut m));
    }
}
