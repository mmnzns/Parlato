// Parlato : l'IA locale, dans l'assistant IA du moteur (voice-engine v0.2.2).
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
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use tauri::{AppHandle, Manager};
use tracing::{info, warn};
use voice_engine_client::{EngineError, Limits, LlmClient, LlmConfig, Outcome};
use voice_engine_models::ModelHandle;
use voice_engine_protocol::Completion;

use crate::models::store::ModelStore;

/// Parlato : duree maximale d'une generation (hors chargement du modele).
/// Au-dela, le nettoyage est abandonne et le texte brut est colle, au lieu
/// de laisser la dictee en attente sans fin sur une machine lente. Les
/// limites du client valent pour toute requete (un chargement de 9B peut
/// durer plus longtemps) : cette limite-ci est donc surveillee ici et
/// arrete la requete par `cancel`.
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
    slot: Mutex<Option<Loaded>>,
    /// Compte les `unload` : un chargement parti avant un `unload` n'est pas
    /// note comme charge.
    epoch: std::sync::atomic::AtomicU64,
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
    /// La derniere reponse ou le dernier chargement a dit `fell_back`.
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

/// Faut-il demander la carte graphique ? Provisoire : l'unite 5 le remplace
/// par `use_gpu`. Mac : Metal. Windows : le processeur, pour l'instant.
pub fn gpu_request() -> bool {
    cfg!(target_os = "macos")
}

fn config_for(cpu: PathBuf, gpu: Option<PathBuf>) -> LlmConfig {
    LlmConfig {
        cpu_helper: cpu,
        gpu_helper: gpu,
        env: vec![],
        gpu_env: vec![],
        gpu_failures: None,
        gpu_key: format!("parlato-{}", env!("CARGO_PKG_VERSION")),
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
        EngineError::Crashed => "llama.cpp: the AI helper stopped",
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
        EngineError::Crashed => "crashed",
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
            client: Arc::new(LlmClient::new(config)),
            store,
            slot: Mutex::new(None),
            epoch: std::sync::atomic::AtomicU64::new(0),
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

    /// Demande la carte graphique ? Oui sur le Mac, tant qu'elle n'a pas
    /// echoue pendant cette session.
    fn gpu_wanted(&self) -> bool {
        gpu_request() && !self.gpu_failed.load(Ordering::Relaxed)
    }

    /// Ce qu'une reponse du moteur dit de la carte graphique : un repli est
    /// signale une seule fois, puis le processeur sert jusqu'a la fermeture.
    fn note_backend(&self, gpu_asked: bool, backend: &str, fell_back: bool) {
        *self.last_backend.lock() = Some(if backend.is_empty() {
            "cpu".into()
        } else {
            backend.to_string()
        });
        self.last_fell_back.store(fell_back, Ordering::Relaxed);
        if gpu_asked && fell_back {
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

    /// Vrai une seule fois apres un repli carte -> processeur : la bulle doit
    /// le dire a l'utilisateur.
    pub fn take_gpu_notice(&self) -> bool {
        self.gpu_notice_pending.swap(false, Ordering::Relaxed)
    }

    /// Ou tourne (ou tournerait) le modele : "metal", "vulkan" ou "cpu". Ce que
    /// le moteur a dit en dernier ; avant tout chargement, ce qui sera demande.
    pub fn backend(&self) -> String {
        if let Some(b) = self.last_backend.lock().clone() {
            return b;
        }
        if self.gpu_wanted() {
            "metal".into()
        } else {
            "cpu".into()
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
                self.note_backend(gpu, &out.backend.backend, out.fell_back);
                info!(
                    seconds = out.value.seconds,
                    wall_ms = started.elapsed().as_millis() as u64,
                    n_ctx,
                    backend = %out.backend.backend,
                    fell_back = out.fell_back,
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
        // La limite de generation : un fil qui arrete la requete passe 60 s.
        let fired = Arc::new(AtomicBool::new(false));
        let (stop, wait) = mpsc::channel::<()>();
        let watchdog = {
            let client = self.client.clone();
            let fired = fired.clone();
            std::thread::Builder::new()
                .name("parlato-llm-limit".into())
                .spawn(move || {
                    if wait.recv_timeout(GENERATION_LIMIT) == Err(RecvTimeoutError::Timeout) {
                        fired.store(true, Ordering::SeqCst);
                        client.cancel();
                    }
                })
                .ok()
        };
        self.generating.store(true, Ordering::SeqCst);
        let started = Instant::now();
        let result = self.client.complete(request, &mut |_| {});
        self.generating.store(false, Ordering::SeqCst);
        drop(stop);
        if let Some(w) = watchdog {
            let _ = w.join();
        }
        match result {
            Ok(Outcome {
                value,
                backend,
                fell_back,
            }) => {
                self.note_backend(gpu, &backend.backend, fell_back);
                info!(
                    wall_ms = started.elapsed().as_millis() as u64,
                    prompt_tokens = value.prompt_tokens,
                    output_tokens = value.output_tokens,
                    seconds = value.seconds,
                    truncated = value.truncated,
                    backend = %backend.backend,
                    fell_back,
                    "Reponse d'IA locale"
                );
                Ok(value.text)
            }
            Err(EngineError::Cancelled) if fired.load(Ordering::SeqCst) => Err(CompleteError::Timeout),
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
        warn!(op = what, kind = error_kind(e), "IA locale en echec");
        anyhow!(error_label(e))
    }
}

enum CompleteError {
    Engine(EngineError),
    Timeout,
}

// -- Reglages et demarrage --------------------------------------------------------

/// Le modele que la configuration actuelle veut en memoire : fournisseur
/// `llamacpp`, nettoyage actif, un modele choisi et present. None sinon.
fn wanted(app: &AppHandle, ai: &LocalAi) -> Option<(ModelRef, u32)> {
    use super::providers::llamacpp as llcp;
    let sel = super::service::get_selection(app)?;
    if sel.provider_id != "llamacpp" || !super::prompts::is_enhancement_enabled(app) {
        return None;
    }
    let id = llcp::get_selected_gguf(app)?;
    let model = ai.resolve(&id).ok()?;
    Some((model, llcp::get_context_size(app)))
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
            let Some((model, n_ctx)) = wanted(&app, &ai) else {
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
    fn gpu_request_follows_the_platform() {
        assert_eq!(gpu_request(), cfg!(target_os = "macos"));
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
            EngineError::Crashed,
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
        ai.note_backend(false, "cpu", true);
        assert!(!ai.take_gpu_notice());
        assert_eq!(ai.backend(), "cpu");
        // The card was asked for and the processor ran.
        ai.note_backend(true, "cpu", true);
        assert!(ai.take_gpu_notice());
        assert!(!ai.take_gpu_notice());
        assert!(!ai.gpu_wanted());
        // A second fall back (it cannot happen once the card is off) says nothing more.
        ai.note_backend(true, "cpu", true);
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
