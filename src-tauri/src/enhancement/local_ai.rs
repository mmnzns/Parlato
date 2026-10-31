// Parlato : l'IA locale, dans l'assistant IA du moteur (voice-engine v0.3.1).
//
// Un `LocalAi` possede UN `LlmClient` : le client lance l'assistant
// (`parlato-llm`, le sidecar Tauri a cote de l'executable), le garde tant que
// l'application tourne (jamais de sortie sur inactivite), le relance s'il
// meurt et traduit chaque echec en `EngineError` (on ne lit jamais le texte
// d'un message). Le modele charge, le verrou du dossier partage
// (`ModelHandle`) et l'etat de la carte graphique sont ici.
//
// Cycle de vie du modele :
//   - au lancement, et au debut de chaque enregistrement, il se charge en
//     arriere-plan quand le fournisseur est `llamacpp` (preload_if_wanted) ;
//   - apres une dictee il reste charge pour "Garder les modeles prets"
//     (`model_keep_alive_secs`, meme delai que la parole : pipeline.rs appelle
//     `unload` avec les modeles de parole) ;
//   - changement de modele, suppression, reglages : `unload` puis rechargement.
//
// Les logs ne portent que des tailles et des durees : jamais le texte dicte,
// le prompt ou la reponse.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use tauri::{AppHandle, Manager};
use tracing::{info, warn};
use voice_engine_client::{EngineError, Limits, LlmClient, LlmConfig, Outcome};
use voice_engine_models::ModelHandle;
use voice_engine_protocol::{Completion, Fallback};

use crate::models::store::ModelStore;

/// Parlato : duree maximale d'une generation (hors chargement du modele).
/// Au-dela, le nettoyage est abandonne et le texte brut est colle, au lieu
/// de laisser la dictee en attente sans fin sur une machine lente. Les
/// limites du client valent pour toute requete (un chargement de 9B peut
/// durer plus longtemps) : cette limite-ci est donnee a `complete_within`, qui
/// la compte une fois le modele charge.
pub const GENERATION_LIMIT: Duration = Duration::from_secs(60);

/// Le nom du sidecar (`externalBin` de tauri.conf.json, sans le triplet).
const HELPER_NAME: &str = "parlato-llm";
/// Windows x64 : l'assistant Vulkan, essaye en premier quand il est la.
const HELPER_VULKAN_NAME: &str = "parlato-llm-vulkan";

/// Le modele a charger : son id et le chemin du fichier.
#[derive(Debug, Clone)]
pub struct ModelRef {
    pub id: String,
    pub path: PathBuf,
}

struct Loaded {
    id: String,
    path: PathBuf,
    n_ctx: u32,
    /// La carte graphique demandee pour ce chargement (apres un eventuel repli).
    gpu: bool,
    // Le verrou du dossier partage : rien pour un modele importe.
    _handle: Option<ModelHandle>,
}

pub struct LocalAi {
    client: Arc<LlmClient>,
    /// None dans les tests : aucun verrou de dossier.
    store: Option<Arc<ModelStore>>,
    /// Une carte graphique peut etre essayee (cf `card_path`).
    card_path: bool,
    slot: Mutex<Option<Loaded>>,
    /// Compte les `unload` : un chargement parti avant un `unload` n'est pas
    /// note comme charge.
    epoch: std::sync::atomic::AtomicU64,
    /// L'assistant a ete lance (`start_helper`) pendant ce lancement.
    helper_started: AtomicBool,
    /// La carte graphique a echoue pendant cette session de l'application.
    gpu_failed: AtomicBool,
    /// Un repli est a signaler une fois dans la bulle.
    gpu_notice_pending: AtomicBool,
    /// Ce que le dernier chargement ou la derniere reponse a rapporte
    /// ("metal", "vulkan" ou "cpu").
    last_backend: Mutex<Option<String>>,
    /// Une generation tourne : seule celle-la est annulee par `cancel`, pas un
    /// chargement en arriere-plan.
    generating: AtomicBool,
    /// La derniere reponse ou le dernier chargement a rapporte un repli
    /// (n'importe lequel, pas seulement un echec de la carte).
    last_fell_back: AtomicBool,
}

static LOCAL_AI: OnceLock<Arc<LocalAi>> = OnceLock::new();

/// Le `LocalAi` de l'application (None avant `init`).
pub fn local_ai() -> Option<Arc<LocalAi>> {
    LOCAL_AI.get().cloned()
}

// -- Assistant et carte graphique ---------------------------------------------

/// Ou sont les assistants : a cote de l'executable. En `tauri dev` et en
/// build debug Tauri copie le sidecar a cote du binaire de debug.
pub fn helper_paths(dir: &Path, windows_x64: bool) -> (PathBuf, Option<PathBuf>) {
    let ext = if cfg!(windows) { ".exe" } else { "" };
    let cpu = dir.join(format!("{HELPER_NAME}{ext}"));
    let gpu = windows_x64
        .then(|| dir.join(format!("{HELPER_VULKAN_NAME}{ext}")))
        .filter(|p| p.is_file());
    (cpu, gpu)
}

/// L'assistant peut-il essayer une carte graphique ? Mac : oui, c'est le meme
/// assistant (Metal). Windows : seulement avec l'assistant Vulkan ; sans lui
/// le client n'a que l'assistant processeur (`NoCardHelper`, silencieux) : on
/// ne demande alors pas la carte, ce qui garde `backend()` juste.
fn card_path(vulkan_helper: bool, mac: bool) -> bool {
    mac || vulkan_helper
}

/// Ce que le moteur retient d'un echec de la carte graphique (`gpu_key`) : la
/// version de Parlato et, sous Windows, le nom de la carte et la version de son
/// pilote, pour qu'une mise a jour du pilote reessaie une carte qui a echoue.
/// Ce qu'on ne sait pas (pas de carte, erreur, `None`) s'ecrit `unknown`, donc
/// la cle reste stable d'un lancement a l'autre. Une seule ligne (le moteur la
/// garde telle quelle dans un fichier). Mac : la version seule, comme avant.
pub fn gpu_key(version: &str, windows: bool, card: Option<&str>, driver: Option<&str>) -> String {
    let base = format!("parlato-{version}");
    if !windows {
        return base;
    }
    let part = |v: Option<&str>| {
        let one_line = v.unwrap_or_default().split_whitespace().collect::<Vec<_>>().join(" ");
        if one_line.is_empty() { "unknown".to_string() } else { one_line }
    };
    format!("{base} | {} | driver {}", part(card), part(driver))
}

/// La cle de cette installation. Windows : cherche la carte dediee (DXGI, sans
/// charger de modele ni lancer d'assistant, assez leger pour le demarrage) ;
/// une erreur ou un panic vaut "carte inconnue".
fn current_gpu_key() -> String {
    let version = env!("CARGO_PKG_VERSION");
    if !cfg!(windows) {
        return gpu_key(version, false, None, None);
    }
    let found = std::panic::catch_unwind(|| {
        let card = voice_engine_stt::discrete_card()?;
        let name = crate::use_gpu::card_name(&card.description, &card.name);
        Some((name, voice_engine_stt::driver_version(&card)))
    })
    .ok()
    .flatten();
    match found {
        Some((name, driver)) => gpu_key(version, true, Some(&name), driver.as_deref()),
        None => gpu_key(version, true, None, None),
    }
}

fn config_for(cpu: PathBuf, gpu: Option<PathBuf>) -> LlmConfig {
    LlmConfig {
        cpu_helper: cpu,
        gpu_helper: gpu,
        env: vec![],
        gpu_env: vec![],
        gpu_failures: None,
        gpu_key: current_gpu_key(),
        // Jamais de sortie sur inactivite : l'assistant vit avec l'application.
        idle_secs: None,
        limits: Limits::default(),
    }
}

/// La configuration du client pour cette installation : les assistants a cote
/// de l'executable.
pub fn default_config() -> LlmConfig {
    let dir = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .unwrap_or_default();
    let (cpu, gpu) = helper_paths(&dir, cfg!(all(windows, target_arch = "x86_64")));
    config_for(cpu, gpu)
}

// -- Erreurs ---------------------------------------------------------------------

/// Le texte d'une erreur du moteur, d'apres son GENRE (jamais le message de
/// l'assistant). Le prefixe `llama.cpp:` garde le comportement existant du
/// service : une erreur locale n'est pas rejouee, le texte brut est colle.
/// Aucun de ces textes ne contient "timeout" (ce serait un delai a rejouer).
pub fn error_label(e: &EngineError) -> &'static str {
    match e {
        EngineError::ModelMissing { .. } => "llama.cpp: model file missing",
        EngineError::OutOfMemory => "llama.cpp: out of memory",
        EngineError::PromptTooLong { .. } => "llama.cpp: prompt too long for the context size",
        EngineError::NoChatTemplate => "llama.cpp: the model has no chat template",
        EngineError::Cancelled => "llama.cpp: cancelled",
        EngineError::GpuFailed { .. } => "llama.cpp: graphics card failed",
        EngineError::BadRequest => "llama.cpp: bad request",
        EngineError::Internal { .. } => "llama.cpp: internal error",
        EngineError::Crashed { .. } => "llama.cpp: the AI helper stopped",
        EngineError::VersionMismatch { .. } => "llama.cpp: the AI helper is from another version",
        EngineError::TimedOut => "llama.cpp: the AI helper did not answer in time",
        EngineError::CouldNotStart { .. } => "llama.cpp: the AI helper could not start",
        EngineError::BadSchema { .. }
        | EngineError::BadModel
        | EngineError::TooLong { .. }
        | EngineError::Unsupported { .. } => "llama.cpp: request refused",
    }
}

fn error_kind(e: &EngineError) -> &'static str {
    match e {
        EngineError::ModelMissing { .. } => "model_missing",
        EngineError::OutOfMemory => "out_of_memory",
        EngineError::PromptTooLong { .. } => "prompt_too_long",
        EngineError::NoChatTemplate => "no_chat_template",
        EngineError::Cancelled => "cancelled",
        EngineError::GpuFailed { .. } => "gpu_failed",
        EngineError::BadRequest => "bad_request",
        EngineError::Internal { .. } => "internal",
        EngineError::Crashed { .. } => "crashed",
        EngineError::VersionMismatch { .. } => "version_mismatch",
        EngineError::TimedOut => "timed_out",
        EngineError::CouldNotStart { .. } => "could_not_start",
        _ => "other",
    }
}

// -- LocalAi ---------------------------------------------------------------------

impl LocalAi {
    pub fn new(config: LlmConfig, store: Option<Arc<ModelStore>>) -> Self {
        Self {
            card_path: card_path(config.gpu_helper.is_some(), cfg!(target_os = "macos")),
            client: Arc::new(LlmClient::new(config)),
            store,
            slot: Mutex::new(None),
            epoch: std::sync::atomic::AtomicU64::new(0),
            helper_started: AtomicBool::new(false),
            gpu_failed: AtomicBool::new(false),
            gpu_notice_pending: AtomicBool::new(false),
            last_backend: Mutex::new(None),
            generating: AtomicBool::new(false),
            last_fell_back: AtomicBool::new(false),
        }
    }

    /// Le fichier du modele `id` (un modele du catalogue installe, ou un
    /// import), seulement quand il est la en entier.
    pub fn resolve(&self, id: &str) -> Result<ModelRef> {
        let store = self
            .store
            .as_ref()
            .ok_or_else(|| anyhow!("llama.cpp: no model store"))?;
        let path = store
            .path_for_id(id)
            .ok_or_else(|| anyhow!("modele GGUF non telecharge: {id}"))?;
        Ok(ModelRef {
            id: id.to_string(),
            path,
        })
    }

    /// Demande la carte graphique ? La regle commune (`use_gpu`), si une
    /// carte peut etre essayee, tant qu'elle n'a pas echoue pendant cette
    /// session.
    fn gpu_wanted(&self) -> bool {
        self.card_path
            && crate::use_gpu::wanted_now(crate::use_gpu::Runtime::LocalAi)
            && !self.gpu_failed.load(Ordering::Relaxed)
    }

    /// Ce qu'une reponse du moteur dit de la carte graphique : un echec de la
    /// carte (`CardFailed`, `CardOutOfMemory`) est signale une seule fois, puis
    /// le processeur sert jusqu'a ce que la personne rallume la carte. Les
    /// autres replis (`NoCardPath`, `NoCardHelper`, `RememberedFailure`) sont
    /// silencieux.
    fn note_backend(&self, gpu_asked: bool, backend: &str, fallback: Option<Fallback>) {
        *self.last_backend.lock() = Some(if backend.is_empty() {
            "cpu".into()
        } else {
            backend.to_string()
        });
        self.last_fell_back
            .store(fallback.is_some(), Ordering::Relaxed);
        if gpu_asked && crate::use_gpu::card_failed(fallback) {
            self.note_gpu_failure();
        }
    }

    /// La derniere requete a-t-elle dit que la carte graphique a echoue ?
    #[cfg(test)]
    pub fn last_fell_back(&self) -> bool {
        self.last_fell_back.load(Ordering::Relaxed)
    }

    fn note_gpu_failure(&self) {
        if !self.gpu_failed.swap(true, Ordering::Relaxed) {
            self.gpu_notice_pending.store(true, Ordering::Relaxed);
            warn!("carte graphique impossible pour l'IA locale, repli sur le processeur");
        }
    }

    /// La carte graphique a-t-elle echoue depuis le lancement ?
    pub fn gpu_failed(&self) -> bool {
        self.gpu_failed.load(Ordering::Relaxed)
    }

    /// Vrai une seule fois apres un repli carte -> processeur : la bulle doit
    /// le dire a l'utilisateur.
    pub fn take_gpu_notice(&self) -> bool {
        self.gpu_notice_pending.swap(false, Ordering::Relaxed)
    }

    /// La personne a rallume la carte graphique : le client du moteur oublie
    /// l'echec (`retry_gpu` attend une requete en cours, d'ou le fil), puis
    /// l'echec de Parlato s'efface. Rien n'est redemarre : le prochain
    /// chargement ou la prochaine requete essaie la carte.
    pub fn retry_gpu_in_background(self: &Arc<Self>) {
        let me = self.clone();
        let _ = std::thread::Builder::new()
            .name("parlato-llm-retry-gpu".into())
            .spawn(move || {
                me.client.retry_gpu();
                me.gpu_failed.store(false, Ordering::Relaxed);
                me.gpu_notice_pending.store(false, Ordering::Relaxed);
                info!("carte graphique de l'IA locale : nouvel essai demande");
            });
    }

    /// Ou tourne (ou tournerait) le modele : "metal", "vulkan" ou "cpu". Ce que
    /// le moteur a dit en dernier ; avant tout chargement, ce qui sera demande.
    pub fn backend(&self) -> String {
        if let Some(b) = self.last_backend.lock().clone() {
            return b;
        }
        match (self.gpu_wanted(), cfg!(target_os = "macos")) {
            (true, true) => "metal".into(),
            (true, false) => "vulkan".into(),
            _ => "cpu".into(),
        }
    }

    /// Lance l'assistant sans rien charger (une seule fois par lancement ; le
    /// client le relance seul s'il meurt). Bloquant : a appeler sur un fil.
    pub fn start_helper(&self) {
        if self.helper_started.swap(true, Ordering::SeqCst) {
            return;
        }
        let started = Instant::now();
        match self.client.devices() {
            Ok(d) => info!(
                wall_ms = started.elapsed().as_millis() as u64,
                devices = d.devices.len(),
                "assistant d'IA locale lance"
            ),
            Err(e) => {
                self.helper_started.store(false, Ordering::SeqCst);
                warn!(
                    error = error_kind(&e),
                    "lancement de l'assistant d'IA locale impossible"
                );
            }
        }
    }

    /// Charge le modele si ce n'est pas deja lui (meme fichier, meme contexte,
    /// meme demande de carte). Bloquant. Rend les secondes de chargement (0 si
    /// deja charge).
    pub fn ensure_loaded(&self, model: &ModelRef, n_ctx: u32) -> Result<f32> {
        let gpu = self.gpu_wanted();
        {
            let guard = self.slot.lock();
            if let Some(cur) = guard.as_ref() {
                if cur.path == model.path && cur.n_ctx == n_ctx && cur.gpu == gpu {
                    return Ok(0.0);
                }
            }
        }
        // Le verrou du dossier partage (rien pour un import), pris avant le
        // chargement : un Delete d'une autre application ne l'efface plus.
        let handle = self.store.as_ref().and_then(|s| s.hold(&model.id));
        let epoch = self.epoch.load(Ordering::SeqCst);
        let started = Instant::now();
        let path = model.path.to_string_lossy().into_owned();
        match self.client.load(&path, n_ctx, gpu, &mut |_| {}) {
            Ok(out) => {
                self.note_backend(gpu, &out.backend.backend, out.fallback);
                info!(
                    seconds = out.value.seconds,
                    wall_ms = started.elapsed().as_millis() as u64,
                    n_ctx,
                    backend = %out.backend.backend,
                    fallback = ?out.fallback,
                    "Modele d'IA locale charge"
                );
                if self.epoch.load(Ordering::SeqCst) == epoch {
                    *self.slot.lock() = Some(Loaded {
                        id: model.id.clone(),
                        path: model.path.clone(),
                        n_ctx,
                        gpu: self.gpu_wanted(),
                        _handle: handle,
                    });
                }
                Ok(out.value.seconds)
            }
            Err(e) => Err(self.failed("load", gpu, &e)),
        }
    }

    /// Genere la reponse a un couple system / user. Bloquant. Charge le modele
    /// au besoin. Thinking coupe, sortie gloutonne (temperature 0), `max_tokens`
    /// et contexte donnes. Garde `clean_output` en filet de securite.
    pub fn complete(
        &self,
        model: &ModelRef,
        n_ctx: u32,
        max_tokens: u32,
        system: &str,
        user: &str,
    ) -> Result<String> {
        self.ensure_loaded(model, n_ctx)?;
        let mut gpu = self.gpu_wanted();
        let mut retried = false;
        loop {
            let request = Completion {
                model_path: model.path.to_string_lossy().into_owned(),
                n_ctx,
                system: system.trim().to_string(),
                user: user.trim().to_string(),
                json_schema: None,
                max_tokens,
                // Glouton : une sortie repetable pour le nettoyage.
                temperature: 0.0,
                gpu,
                // Tous les modeles du catalogue "pensent" : on veut la reponse
                // tout de suite.
                thinking: false,
                thinking_budget: None,
                seed: None,
            };
            match self.run_completion(request, gpu) {
                Ok(text) => return Ok(super::providers::llamacpp::clean_output(&text)),
                // La carte a echoue sans que rien soit fait : une fois sur le
                // processeur, sans attendre la dictee suivante.
                Err(CompleteError::Engine(EngineError::GpuFailed { .. })) if gpu && !retried => {
                    retried = true;
                    self.note_gpu_failure();
                    gpu = false;
                }
                Err(CompleteError::Engine(e)) => return Err(self.failed("complete", gpu, &e)),
                Err(CompleteError::Timeout) => {
                    return Err(anyhow!(
                        "llama.cpp: generation stopped after {} s",
                        GENERATION_LIMIT.as_secs()
                    ))
                }
            }
        }
    }

    fn run_completion(
        &self,
        request: Completion,
        gpu: bool,
    ) -> std::result::Result<String, CompleteError> {
        self.generating.store(true, Ordering::SeqCst);
        let started = Instant::now();
        // La limite de generation, comptee par le moteur une fois le modele
        // charge. Depassee, il arrete l'assistant (le modele est decharge).
        let result = self
            .client
            .complete_within(GENERATION_LIMIT, request, &mut |_| {});
        self.generating.store(false, Ordering::SeqCst);
        match result {
            Ok(Outcome {
                value,
                backend,
                fallback,
            }) => {
                self.note_backend(gpu, &backend.backend, fallback);
                info!(
                    wall_ms = started.elapsed().as_millis() as u64,
                    prompt_tokens = value.prompt_tokens,
                    output_tokens = value.output_tokens,
                    seconds = value.seconds,
                    truncated = value.truncated,
                    backend = %backend.backend,
                    fallback = ?fallback,
                    "Reponse d'IA locale"
                );
                Ok(value.text)
            }
            Err(EngineError::TimedOut) => {
                // L'assistant a ete arrete : le modele n'est plus charge.
                self.slot.lock().take();
                if started.elapsed() >= GENERATION_LIMIT {
                    Err(CompleteError::Timeout)
                } else {
                    // Une limite du client (demarrage, silence), pas la notre.
                    Err(CompleteError::Engine(EngineError::TimedOut))
                }
            }
            Err(e) => Err(CompleteError::Engine(e)),
        }
    }

    /// Annule la generation en cours (le bouton Annuler de la bulle). Ne fait
    /// rien si aucune generation ne tourne : un chargement en arriere-plan
    /// n'est pas touche. Le modele reste charge.
    pub fn cancel(&self) {
        if self.generating.load(Ordering::SeqCst) {
            info!("generation d'IA locale annulee");
            self.client.cancel();
        }
    }

    /// Libere la memoire du modele (l'assistant continue de tourner) et rend le
    /// verrou du dossier partage. Bloque tant qu'une requete tourne.
    pub fn unload(&self) {
        self.epoch.fetch_add(1, Ordering::SeqCst);
        let had = self.slot.lock().take().is_some();
        if let Err(e) = self.client.unload() {
            warn!(error = error_kind(&e), "dechargement de l'IA locale impossible");
        } else if had {
            info!("modele d'IA locale decharge");
        }
    }

    /// `unload` sur un fil : pour l'interface et les minuteries, qui ne
    /// doivent pas attendre une requete en cours.
    pub fn unload_in_background(self: &Arc<Self>) {
        let me = self.clone();
        let _ = std::thread::Builder::new()
            .name("parlato-llm-unload".into())
            .spawn(move || me.unload());
    }

    /// Le modele `id` est-il charge ?
    pub fn is_loaded(&self, id: &str) -> bool {
        self.slot.lock().as_ref().is_some_and(|l| l.id == id)
    }

    /// A la fermeture de l'application : arrete l'assistant.
    pub fn shutdown(&self) {
        self.cancel();
        self.slot.lock().take();
        self.client.shutdown();
        info!("assistant d'IA locale arrete");
    }

    /// Un echec du moteur : note la carte graphique, journalise le genre
    /// (jamais le message) et rend le texte d'apres le genre.
    fn failed(&self, what: &str, gpu_asked: bool, e: &EngineError) -> anyhow::Error {
        if gpu_asked && matches!(e, EngineError::GpuFailed { .. }) {
            self.note_gpu_failure();
        }
        if let EngineError::Crashed {
            exit_code,
            signal,
            detail,
        } = e
        {
            // Le moteur garantit que `detail` (5 dernieres lignes d'erreur de
            // l'assistant) ne porte aucun texte de la personne.
            warn!(op = what, ?exit_code, ?signal, %detail, "l'assistant d'IA locale s'est arrete");
        }
        warn!(op = what, kind = error_kind(e), "IA locale en echec");
        anyhow!(error_label(e))
    }
}

enum CompleteError {
    Engine(EngineError),
    Timeout,
}

// -- Reglages et demarrage --------------------------------------------------------

/// Ce que le lancement (et chaque rechargement) doit faire pour l'IA locale.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct StartupPlan {
    /// Lancer l'assistant (`parlato-llm`) : des que le fournisseur est
    /// `llamacpp`. Le processus est leger ; seul le chargement du modele est long.
    pub start_helper: bool,
    /// Charger le modele choisi : fournisseur `llamacpp`, nettoyage actif, un
    /// modele choisi et present dans le dossier.
    pub load_model: bool,
}

/// La decision, pure : `provider` est le fournisseur enregistre, `enabled` le
/// reglage "nettoyage IA" (dans `parla.prompts.json`), `selected` le modele
/// choisi (deja migre), `installed` s'il est present en entier.
pub(crate) fn startup_plan(
    provider: Option<&str>,
    enabled: bool,
    selected: Option<&str>,
    installed: bool,
) -> StartupPlan {
    let local = provider == Some("llamacpp");
    StartupPlan {
        start_helper: local,
        load_model: local && enabled && selected.is_some_and(|s| !s.is_empty()) && installed,
    }
}

/// Le plan de la configuration actuelle, et le modele a charger s'il y en a un.
fn wanted(app: &AppHandle, ai: &LocalAi) -> (StartupPlan, Option<(ModelRef, u32)>) {
    use super::providers::llamacpp as llcp;
    let provider = super::service::get_selection(app).map(|s| s.provider_id);
    let enabled = super::prompts::is_enhancement_enabled(app);
    let selected = llcp::get_selected_gguf(app);
    let model = selected.as_deref().and_then(|id| ai.resolve(id).ok());
    let plan = startup_plan(
        provider.as_deref(),
        enabled,
        selected.as_deref(),
        model.is_some(),
    );
    let load = if plan.load_model {
        model.map(|m| (m, llcp::get_context_size(app)))
    } else {
        None
    };
    (plan, load)
}

/// Cree le `LocalAi` de l'application (le dossier de modeles est ouvert) et
/// charge le modele choisi en arriere-plan.
pub fn init(app: &AppHandle) -> Arc<LocalAi> {
    let store = app
        .try_state::<crate::models::ModelStoreState>()
        .map(|s| s.0.clone());
    let ai = Arc::new(LocalAi::new(default_config(), store));
    let _ = LOCAL_AI.set(ai.clone());
    info!("assistant d'IA locale pret a demarrer");
    preload_if_wanted(app);
    ai
}

/// Charge en arriere-plan le modele que la configuration veut, sans bloquer
/// l'appelant : au lancement, au debut d'un enregistrement (le chargement
/// recouvre l'enregistrement), apres un changement de modele ou de reglage.
pub fn preload_if_wanted(app: &AppHandle) {
    let Some(ai) = local_ai() else { return };
    let app = app.clone();
    let _ = std::thread::Builder::new()
        .name("parlato-llm-preload".into())
        .spawn(move || {
            let (plan, load) = wanted(&app, &ai);
            if plan.start_helper {
                ai.start_helper();
            }
            let Some((model, n_ctx)) = load else {
                info!(
                    helper = plan.start_helper,
                    "IA locale : pas de modele a charger"
                );
                return;
            };
            if let Err(e) = ai.ensure_loaded(&model, n_ctx) {
                warn!(error = %e, "chargement de l'IA locale en arriere-plan impossible");
            }
        });
}

/// Annule la generation locale en cours (Annuler dans la bulle).
pub fn cancel_generation() {
    if let Some(ai) = local_ai() {
        ai.cancel();
    }
}

/// A la fermeture de l'application.
pub fn shutdown() {
    if let Some(ai) = local_ai() {
        ai.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(
        provider: Option<&str>,
        enabled: bool,
        sel: Option<&str>,
        installed: bool,
    ) -> (bool, bool) {
        let p = startup_plan(provider, enabled, sel, installed);
        (p.start_helper, p.load_model)
    }

    #[test]
    fn the_helper_starts_whenever_the_built_in_ai_is_the_provider() {
        // Meme sans nettoyage actif, sans modele choisi ou sans fichier.
        assert_eq!(plan(Some("llamacpp"), false, None, false), (true, false));
        assert_eq!(plan(Some("llamacpp"), true, None, false), (true, false));
        assert_eq!(
            plan(Some("llamacpp"), true, Some("granite-4.2-3b"), false),
            (true, false)
        );
    }

    #[test]
    fn the_model_loads_only_when_enabled_chosen_and_installed() {
        assert_eq!(
            plan(Some("llamacpp"), true, Some("granite-4.2-3b"), true),
            (true, true)
        );
        assert_eq!(
            plan(Some("llamacpp"), false, Some("granite-4.2-3b"), true),
            (true, false)
        );
        assert_eq!(plan(Some("llamacpp"), true, Some(""), true), (true, false));
        assert_eq!(plan(Some("llamacpp"), true, None, true), (true, false));
    }

    #[test]
    fn another_provider_or_none_starts_nothing() {
        for p in [Some("openai"), Some("ollama"), Some(""), None] {
            assert_eq!(
                plan(p, true, Some("granite-4.2-3b"), true),
                (false, false),
                "{p:?}"
            );
        }
    }

    #[test]
    fn helper_sits_next_to_the_executable() {
        let dir = Path::new("/app/bin");
        let (cpu, gpu) = helper_paths(dir, false);
        assert_eq!(cpu.parent(), Some(dir));
        assert!(cpu.file_name().unwrap().to_str().unwrap().starts_with("parlato-llm"));
        assert_eq!(gpu, None);
        // A Vulkan helper is used only when the file exists.
        let (_, gpu) = helper_paths(dir, true);
        assert_eq!(gpu, None);
        let tmp = tempfile::tempdir().unwrap();
        let (cpu, gpu) = helper_paths(tmp.path(), true);
        assert_eq!(gpu, None);
        let vulkan = tmp
            .path()
            .join(format!("parlato-llm-vulkan{}", if cfg!(windows) { ".exe" } else { "" }));
        std::fs::write(&vulkan, b"").unwrap();
        let (cpu2, gpu) = helper_paths(tmp.path(), true);
        assert_eq!(cpu, cpu2);
        assert_eq!(gpu, Some(vulkan));
    }

    #[test]
    fn the_helper_never_exits_on_idle() {
        let cfg = default_config();
        assert_eq!(cfg.idle_secs, None);
    }

    #[test]
    fn the_mac_key_stays_the_version_alone() {
        assert_eq!(gpu_key("0.14.1", false, None, None), "parlato-0.14.1");
        // Whatever is passed, the Mac does not fold a card in.
        assert_eq!(
            gpu_key("0.14.1", false, Some("Apple M3 Pro"), Some("1.0")),
            "parlato-0.14.1"
        );
    }

    #[test]
    fn the_windows_key_has_the_version_the_card_and_the_driver() {
        assert_eq!(
            gpu_key("0.14.1", true, Some("NVIDIA GeForce RTX 5070 Ti"), Some("32.0.15.6614")),
            "parlato-0.14.1 | NVIDIA GeForce RTX 5070 Ti | driver 32.0.15.6614"
        );
    }

    #[test]
    fn a_new_driver_a_new_card_or_a_new_version_gives_a_new_key() {
        let key = |v: &str, c: &str, d: &str| gpu_key(v, true, Some(c), Some(d));
        let base = key("0.14.1", "Card A", "32.0.1");
        assert_ne!(base, key("0.14.1", "Card A", "32.0.2"));
        assert_ne!(base, key("0.14.1", "Card B", "32.0.1"));
        assert_ne!(base, key("0.15.0", "Card A", "32.0.1"));
        assert_eq!(base, key("0.14.1", "Card A", "32.0.1"));
    }

    #[test]
    fn what_is_not_known_is_unknown_and_the_key_stays_stable() {
        let unknown = "parlato-0.14.1 | unknown | driver unknown";
        assert_eq!(gpu_key("0.14.1", true, None, None), unknown);
        assert_eq!(gpu_key("0.14.1", true, Some(""), Some("  ")), unknown);
        assert_eq!(
            gpu_key("0.14.1", true, Some("Card A"), None),
            "parlato-0.14.1 | Card A | driver unknown"
        );
        // Asked twice, the same answer.
        assert_eq!(gpu_key("0.14.1", true, None, None), gpu_key("0.14.1", true, None, None));
    }

    #[test]
    fn the_key_is_one_line() {
        let k = gpu_key("0.14.1", true, Some("Card\n  A\r\n"), Some("32.0\t.1"));
        assert!(!k.contains('\n') && !k.contains('\r') && !k.contains('\t'));
        assert_eq!(k, "parlato-0.14.1 | Card A | driver 32.0 .1");
    }

    #[test]
    fn the_config_carries_the_key_of_this_install() {
        let key = default_config().gpu_key;
        assert!(key.starts_with(&format!("parlato-{}", env!("CARGO_PKG_VERSION"))));
        assert_eq!(key.contains(" | "), cfg!(windows));
    }

    #[test]
    fn the_card_is_only_tried_where_a_card_helper_exists() {
        // Mac: the one helper is the Metal one. Windows: the Vulkan helper or nothing.
        assert!(card_path(false, true));
        assert!(card_path(true, true));
        assert!(card_path(true, false));
        assert!(!card_path(false, false));
    }

    #[test]
    fn a_processor_only_install_never_asks_for_the_card() {
        // No Vulkan helper file (Windows) = nothing to try: no request, so no
        // fallback notice for a missing file.
        let ai = LocalAi::new(config_for(PathBuf::from("/nowhere"), None), None);
        assert_eq!(ai.card_path, cfg!(target_os = "macos"));
        if !cfg!(target_os = "macos") {
            assert!(!ai.gpu_wanted());
            assert_eq!(ai.backend(), "cpu");
        }
    }

    #[test]
    fn labels_never_carry_the_helper_message_or_a_timeout_word() {
        let all = [
            EngineError::ModelMissing { path: "/secret".into() },
            EngineError::OutOfMemory,
            EngineError::PromptTooLong { prompt_tokens: 1, n_ctx: 1, max_tokens: 1 },
            EngineError::NoChatTemplate,
            EngineError::Cancelled,
            EngineError::GpuFailed { fell_back: true },
            EngineError::BadRequest,
            EngineError::Internal { message: "private words".into() },
            EngineError::Crashed { exit_code: Some(1), signal: None, detail: "secret words".into() },
            EngineError::VersionMismatch { helper: 1, app: 2 },
            EngineError::TimedOut,
            EngineError::CouldNotStart { message: "/some/path".into() },
            EngineError::BadModel,
        ];
        for e in &all {
            let label = error_label(e);
            assert!(label.starts_with("llama.cpp: "), "{label}");
            assert!(!label.to_lowercase().contains("timeout"), "{label}");
            assert!(!label.contains("secret") && !label.contains("private") && !label.contains("/some"));
        }
    }

    #[test]
    fn a_fallback_is_reported_once_and_pins_the_processor() {
        let ai = LocalAi::new(config_for("/nowhere".into(), None), None);
        // Nothing asked of the card: nothing to report.
        ai.note_backend(false, "cpu", None);
        assert!(!ai.take_gpu_notice());
        assert_eq!(ai.backend(), "cpu");
        // Asked, but the model has no card path or the build no card helper,
        // or the failure was told before: silent, no failure is remembered.
        for silent in [
            Fallback::NoCardPath,
            Fallback::NoCardHelper,
            Fallback::RememberedFailure,
        ] {
            ai.note_backend(true, "cpu", Some(silent));
            assert!(!ai.take_gpu_notice(), "{silent:?}");
            assert!(!ai.gpu_failed(), "{silent:?}");
        }
        // The card was asked for, failed, and the processor ran.
        ai.note_backend(true, "cpu", Some(Fallback::CardFailed));
        assert!(ai.take_gpu_notice());
        assert!(!ai.take_gpu_notice());
        assert!(!ai.gpu_wanted());
        // A second fall back (it cannot happen once the card is off) says nothing more.
        ai.note_backend(true, "cpu", Some(Fallback::CardOutOfMemory));
        assert!(!ai.take_gpu_notice());
    }

    #[test]
    fn turning_the_card_back_on_forgets_the_failure_without_a_restart() {
        let ai = Arc::new(LocalAi::new(config_for("/nowhere".into(), None), None));
        ai.note_gpu_failure();
        assert!(ai.gpu_failed());
        ai.retry_gpu_in_background();
        // The client waits for a request in progress, so it runs on a thread.
        let mut cleared = false;
        for _ in 0..50 {
            if !ai.gpu_failed() {
                cleared = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(cleared, "the failure was not forgotten");
        assert!(!ai.take_gpu_notice());
    }

    #[test]
    fn a_missing_helper_is_an_error_not_a_hang() {
        let ai = LocalAi::new(config_for("/nowhere/parlato-llm".into(), None), None);
        let model = ModelRef { id: "x".into(), path: "/nowhere/m.gguf".into() };
        let err = ai.ensure_loaded(&model, 512).unwrap_err().to_string();
        assert_eq!(err, "llama.cpp: the AI helper could not start");
        // The same through a completion; unload and shutdown are harmless.
        assert!(ai.complete(&model, 512, 32, "s", "u").is_err());
        ai.unload();
        ai.shutdown();
        ai.cancel();
    }

    /// A scripted helper that speaks the protocol: lines in, events out.
    #[cfg(unix)]
    fn fake_helper(dir: &Path, complete_reply: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("fake-llm");
        let pid_file = dir.join("helper.pid").display().to_string();
        let script = format!(
            r#"#!/bin/sh
echo $$ > "{pid_file}"
while IFS= read -r line; do
  case "$line" in
    *'"op":"ping"'*) printf '%s\n' '{{"event":"ready","backend":"cpu","protocol":2}}';;
    *'"op":"configure"'*) printf '%s\n' '{{"event":"configured","idle_secs":null}}';;
    *'"op":"load"'*) printf '%s\n' '{{"event":"loaded","seconds":0.5,"n_ctx_train":4096,"backend":"metal"}}';;
    *'"op":"unload"'*) printf '%s\n' '{{"event":"unloaded"}}';;
    *'"op":"complete"'*) printf '%s\n' '{complete_reply}';;
  esac
done
"#
        );
        std::fs::write(&path, script).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[cfg(unix)]
    #[test]
    fn load_complete_and_unload_through_a_scripted_helper() {
        let tmp = tempfile::tempdir().unwrap();
        let reply = r#"{"event":"done","text":"<think>x</think>\nHello there.","prompt_tokens":10,"output_tokens":3,"seconds":0.1,"truncated":false,"backend":"metal"}"#;
        let helper = fake_helper(tmp.path(), reply);
        let ai = LocalAi::new(config_for(helper, None), None);
        let model = ModelRef { id: "m".into(), path: tmp.path().join("m.gguf") };
        assert_eq!(ai.ensure_loaded(&model, 4096).unwrap(), 0.5);
        assert!(ai.is_loaded("m"));
        // Already loaded with the same settings: no second load.
        assert_eq!(ai.ensure_loaded(&model, 4096).unwrap(), 0.0);
        // The reply goes through clean_output.
        assert_eq!(ai.complete(&model, 4096, 128, " system ", " user ").unwrap(), "Hello there.");
        assert_eq!(ai.backend(), "metal");
        ai.unload();
        assert!(!ai.is_loaded("m"));
        // Shutdown stops the helper process.
        let pid: u32 = std::fs::read_to_string(tmp.path().join("helper.pid"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        let alive = |pid: u32| {
            std::process::Command::new("kill")
                .args(["-0", &pid.to_string()])
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|s| s.success())
        };
        assert!(alive(pid));
        ai.shutdown();
        let mut gone = false;
        for _ in 0..50 {
            if !alive(pid) {
                gone = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(gone, "the helper is still running after shutdown");
    }

    #[cfg(unix)]
    #[test]
    fn a_helper_error_is_mapped_by_kind() {
        let tmp = tempfile::tempdir().unwrap();
        let reply = r#"{"event":"error","message":"private words","error":{"kind":"no_chat_template"}}"#;
        let helper = fake_helper(tmp.path(), reply);
        let ai = LocalAi::new(config_for(helper, None), None);
        let model = ModelRef { id: "m".into(), path: tmp.path().join("m.gguf") };
        let err = ai.complete(&model, 4096, 128, "s", "u").unwrap_err().to_string();
        assert_eq!(err, "llama.cpp: the model has no chat template");
        ai.shutdown();
    }
}
