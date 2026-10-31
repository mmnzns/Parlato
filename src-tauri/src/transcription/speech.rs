// Parlato : les moteurs de parole, sur `voice-engine-stt` (voice-engine v0.2.2).
//
// Un `SpeechSlot` garde UN modele charge (`Box<dyn Transcriber>`), le verrou
// du dossier de modeles partage (`ModelHandle`) tant qu'il est charge, et
// l'etat de la carte graphique : un echec (le moteur dit `fell_back`) fait
// passer ce moteur sur le processeur jusqu'a la fermeture de l'application,
// avec un avis a dire une fois dans la bulle. Whisper et la famille Parakeet
// ont chacun leur slot (whisper.rs, parakeet.rs) ; la detection de voix est
// dans vad.rs.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use tracing::{info, warn};
use voice_engine_models::{catalog, Kind, ModelHandle, ModelInfo, Status};
use voice_engine_stt::threads::{default_threads, logical_cores, ThreadKind};
use voice_engine_stt::{whisper_engine, LoadOptions, SpeechBackend, Transcriber};

use crate::models::imports;
use crate::models::store::ModelStore;

// ---------------------------------------------------------------------------
// Threads
// ---------------------------------------------------------------------------

/// Threads logiques de l'ordinateur.
pub fn max_threads() -> i32 {
    logical_cores() as i32
}

/// Ce que "automatique" utilise sur le processeur : la regle du moteur (les
/// trois quarts des threads, de 2 a 12). Sert a l'affichage du reglage.
pub fn auto_threads() -> i32 {
    default_threads(ThreadKind::Processor, logical_cores()) as i32
}

/// Le reglage `speech_threads` (0 = automatique) en option du moteur :
/// automatique = `None`, la regle du moteur.
pub fn threads_option(setting: usize) -> Option<usize> {
    (setting > 0).then_some(setting)
}

// ---------------------------------------------------------------------------
// Carte graphique (provisoire : l'unite 5 la remplace par `use_gpu`)
// ---------------------------------------------------------------------------

/// Faut-il demander la carte graphique pour ce modele ? Le seul endroit qui
/// decide, pour que l'unite 5 le remplace.
/// - Mac : Whisper sur Metal ; les modeles ONNX n'ont pas de chemin carte
///   graphique et le moteur dirait `fell_back` : jamais demande.
/// - Windows : Whisper sur le processeur (pas de Vulkan dans ce processus) ;
///   Parakeet TDT suit le reglage `parakeet_gpu` (DirectML) ; Unified et
///   Nemotron sur le processeur.
pub fn gpu_request(kind: Kind, parakeet_gpu_setting: bool) -> bool {
    match kind {
        Kind::Whisper => cfg!(target_os = "macos"),
        Kind::Parakeet => cfg!(windows) && parakeet_gpu_setting,
        _ => false,
    }
}

/// Ce build sait-il utiliser la carte graphique pour Parakeet ? (le reglage
/// `parakeet_gpu` n'a de sens que si oui : DirectML sous Windows.)
pub fn gpu_available() -> bool {
    cfg!(windows)
}

// ---------------------------------------------------------------------------
// Lecture d'un WAV
// ---------------------------------------------------------------------------

pub fn read_wav_as_f32(path: &Path) -> Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let bits = spec.bits_per_sample;
            let max = (1i64 << (bits - 1)) as f32;
            reader
                .samples::<i32>()
                .map(|s| s.map(|v| v as f32 / max).map_err(anyhow::Error::from))
                .collect::<Result<Vec<_>>>()?
        }
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .map(|s| s.map_err(anyhow::Error::from))
            .collect::<Result<Vec<_>>>()?,
    };

    // Si le WAV est stereo, on mixdown en moyenne.
    let samples = if spec.channels > 1 {
        let ch = spec.channels as usize;
        samples
            .chunks_exact(ch)
            .map(|f| f.iter().sum::<f32>() / ch as f32)
            .collect()
    } else {
        samples
    };

    Ok(samples)
}

// ---------------------------------------------------------------------------
// Ce qu'on charge
// ---------------------------------------------------------------------------

/// Un modele de parole installe.
#[derive(Debug, Clone)]
pub enum Source {
    /// Un modele du catalogue du moteur, dans son dossier du dossier partage.
    Catalog {
        model: &'static ModelInfo,
        dir: PathBuf,
    },
    /// Un fichier Whisper importe (`imported:<stem>`, `Models/imported/`).
    Imported { file: PathBuf },
}

impl Source {
    /// Le modele `id`, seulement s'il est completement la.
    pub fn for_id(store: &ModelStore, id: &str) -> Option<Source> {
        if let Some(model) = catalog::find(id) {
            return match store.shared().status(id) {
                Status::Installed { dir, .. } => Some(Source::Catalog { model, dir }),
                _ => None,
            };
        }
        if id.starts_with(imports::WHISPER_IMPORT_PREFIX) {
            return imports::whisper_import_path(&store.whisper_dir(), id)
                .map(|file| Source::Imported { file });
        }
        None
    }

    /// Le dossier (catalogue) ou le fichier (import) charge.
    pub fn path(&self) -> &Path {
        match self {
            Source::Catalog { dir, .. } => dir,
            Source::Imported { file } => file,
        }
    }

    pub fn kind(&self) -> Kind {
        match self {
            Source::Catalog { model, .. } => model.kind,
            Source::Imported { .. } => Kind::Whisper,
        }
    }

    fn load(&self, opts: LoadOptions) -> Result<Box<dyn Transcriber>> {
        match self {
            Source::Catalog { model, dir } => voice_engine_stt::load(model, dir, opts),
            Source::Imported { file } => whisper_engine::WhisperEngine::load(file, opts)
                .map(|e| Box::new(e) as Box<dyn Transcriber>),
        }
        .map_err(|e| anyhow!(e.to_string()))
    }
}

struct Loaded {
    path: PathBuf,
    source: Source,
    threads: Option<usize>,
    /// La carte graphique etait demandee a ce chargement (recharge si le
    /// reglage change). Faux apres un repli sur le processeur.
    gpu_asked: bool,
    /// Une transcription a deja reussi sur cette session.
    verified: bool,
    // Le moteur est libere avant le verrou du dossier.
    engine: Box<dyn Transcriber>,
    backend: SpeechBackend,
    _handle: Option<ModelHandle>,
}

/// Un modele charge, avec l'etat de la carte graphique de ce moteur.
pub struct SpeechSlot {
    current: Mutex<Option<Loaded>>,
    /// La carte graphique a echoue pendant cette session de l'application.
    gpu_failed: AtomicBool,
    /// Un repli est a signaler une fois dans la bulle.
    gpu_notice_pending: AtomicBool,
    /// Ce que le dernier chargement a rapporte ("cpu", "metal", "directml" ou
    /// "cpu-fallback").
    last_backend: Mutex<String>,
}

impl Default for SpeechSlot {
    fn default() -> Self {
        Self {
            current: Mutex::new(None),
            gpu_failed: AtomicBool::new(false),
            gpu_notice_pending: AtomicBool::new(false),
            last_backend: Mutex::new("cpu".into()),
        }
    }
}

impl SpeechSlot {
    /// Charge le modele si ce n'est pas deja lui (meme dossier, memes threads,
    /// meme demande de carte). Bloquant. `handle` : le verrou du dossier
    /// partage, garde tant que le modele est charge (rien pour un import).
    /// `threads` : `None` = la regle du moteur. `gpu` : voir `gpu_request`.
    pub fn ensure_loaded(
        &self,
        source: &Source,
        threads: Option<usize>,
        gpu: bool,
        handle: Option<ModelHandle>,
    ) -> Result<()> {
        let want_gpu = gpu && !self.gpu_failed();
        let mut guard = self.current.lock();
        if let Some(cur) = guard.as_mut() {
            if cur.path == source.path() && cur.threads == threads && cur.gpu_asked == want_gpu {
                if handle.is_some() {
                    cur._handle = handle;
                }
                return Ok(());
            }
        }
        // Libere l'ancien (et son verrou) avant de charger le nouveau.
        *guard = None;
        *guard = Some(self.load_new(source, threads, want_gpu, handle)?);
        Ok(())
    }

    fn load_new(
        &self,
        source: &Source,
        threads: Option<usize>,
        gpu: bool,
        handle: Option<ModelHandle>,
    ) -> Result<Loaded> {
        let started = std::time::Instant::now();
        let engine = source.load(LoadOptions { gpu, threads })?;
        let backend = engine.backend();
        let mut gpu_asked = gpu;
        if gpu && backend.fell_back {
            // La carte graphique a ete demandee et n'a pas servi : le
            // processeur a pris le relais (le moteur ne dit pas pourquoi).
            warn!(
                kind = ?source.kind(),
                "carte graphique impossible, repli sur le processeur"
            );
            self.note_gpu_failure();
            gpu_asked = false;
        }
        self.set_last_backend(&backend);
        info!(
            asked = if gpu { "gpu" } else { "cpu" },
            used = %backend.backend,
            fell_back = backend.fell_back,
            load_ms = started.elapsed().as_millis() as u64,
            "Modele de parole charge"
        );
        Ok(Loaded {
            path: source.path().to_path_buf(),
            source: source.clone(),
            threads,
            gpu_asked,
            verified: false,
            engine,
            backend,
            _handle: handle,
        })
    }

    fn set_last_backend(&self, backend: &SpeechBackend) {
        *self.last_backend.lock() = if self.gpu_failed() {
            "cpu-fallback".into()
        } else {
            backend.backend.clone()
        };
    }

    /// Une operation sur le modele charge, avec sa famille. Parlato : si la
    /// premiere operation d'une session sur la carte graphique echoue, le
    /// modele est recharge sur le processeur et l'operation refaite une fois ;
    /// si le processeur reussit, c'est bien la carte qui est en cause.
    pub fn run<R>(&self, mut op: impl FnMut(&mut dyn Transcriber, Kind) -> Result<R>) -> Result<R> {
        let mut guard = self.current.lock();
        let loaded = guard
            .as_mut()
            .ok_or_else(|| anyhow!("aucun modele de parole charge"))?;
        let kind = loaded.source.kind();
        let on_gpu =
            loaded.gpu_asked && !loaded.backend.fell_back && loaded.backend.backend != "cpu";
        let first_gpu_run = on_gpu && !loaded.verified;
        match op(loaded.engine.as_mut(), kind) {
            Ok(r) => {
                loaded.verified = true;
                Ok(r)
            }
            Err(err) if first_gpu_run => {
                warn!("transcription sur la carte graphique en echec ({err}), nouvel essai sur le processeur");
                let Some(old) = guard.take() else {
                    return Err(err);
                };
                let (source, threads, handle) = (old.source.clone(), old.threads, old._handle);
                drop(old.engine);
                let mut cpu = self.load_new(&source, threads, false, handle)?;
                let result = op(cpu.engine.as_mut(), kind);
                if result.is_ok() {
                    // Le processeur a reussi la ou la carte a echoue. Sinon
                    // l'erreur vient de l'audio : on reessaiera la carte.
                    cpu.verified = true;
                    self.note_gpu_failure();
                    self.set_last_backend(&cpu.backend);
                }
                *guard = Some(cpu);
                result
            }
            Err(err) => Err(err),
        }
    }

    /// Libere le modele et son verrou.
    pub fn unload(&self) -> bool {
        self.current.lock().take().is_some()
    }

    /// Decharge le modele s'il se trouve dans (ou est) `path`. Avant un Delete.
    pub fn unload_if_under(&self, path: &Path) -> bool {
        let here = self
            .current
            .lock()
            .as_ref()
            .is_some_and(|c| c.path.starts_with(path));
        here && self.unload()
    }

    /// Ce qui a servi au dernier chargement : "cpu", "metal", "directml" ou
    /// "cpu-fallback" (carte demandee, le processeur a pris le relais).
    pub fn provider(&self) -> String {
        self.last_backend.lock().clone()
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

    pub(crate) fn note_gpu_failure(&self) {
        self.gpu_failed.store(true, Ordering::Relaxed);
        self.gpu_notice_pending.store(true, Ordering::Relaxed);
        *self.last_backend.lock() = "cpu-fallback".into();
    }

    /// Ou tourne le modele charge (None si rien n'est charge).
    #[cfg(test)]
    pub fn backend(&self) -> Option<SpeechBackend> {
        self.current.lock().as_ref().map(|c| c.backend.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_threads_are_none() {
        assert_eq!(threads_option(0), None);
        assert_eq!(threads_option(6), Some(6));
    }

    #[test]
    fn auto_threads_follow_the_engine_rule() {
        let auto = auto_threads();
        assert!((2..=12).contains(&auto));
        assert!(auto <= max_threads().max(2));
    }

    #[test]
    fn graphics_card_policy_until_use_gpu() {
        // The ONNX models other than Parakeet TDT never ask for the card.
        assert!(!gpu_request(Kind::ParakeetUnified, true));
        assert!(!gpu_request(Kind::Nemotron, true));
        assert!(!gpu_request(Kind::Vad, true));
        // Whisper: Metal on the Mac only. Parakeet: the setting, Windows only.
        assert_eq!(gpu_request(Kind::Whisper, false), cfg!(target_os = "macos"));
        assert!(!gpu_request(Kind::Parakeet, false));
        assert_eq!(gpu_request(Kind::Parakeet, true), cfg!(windows));
    }

    #[test]
    fn a_failure_is_noted_once_and_named() {
        let slot = SpeechSlot::default();
        assert_eq!(slot.provider(), "cpu");
        assert!(!slot.gpu_failed());
        assert!(!slot.take_gpu_notice());
        slot.note_gpu_failure();
        assert!(slot.gpu_failed());
        assert_eq!(slot.provider(), "cpu-fallback");
        assert!(slot.take_gpu_notice());
        assert!(!slot.take_gpu_notice());
    }

    #[test]
    fn nothing_loaded_is_an_error_and_unload_is_harmless() {
        let slot = SpeechSlot::default();
        assert!(slot.run(|_, _| Ok(())).is_err());
        assert!(!slot.unload());
        assert!(!slot.unload_if_under(Path::new("/nowhere")));
        assert!(slot.backend().is_none());
    }

    #[test]
    fn a_missing_import_is_not_a_source() {
        let tmp = tempfile::tempdir().unwrap();
        let store = crate::models::store::tests::temp_store(tmp.path());
        assert!(Source::for_id(&store, "imported:nope").is_none());
        assert!(Source::for_id(&store, "whisper-tiny").is_none());
        crate::models::store::tests::install(&store, "whisper-tiny");
        let source = Source::for_id(&store, "whisper-tiny").unwrap();
        assert_eq!(source.kind(), Kind::Whisper);
        assert!(source.path().ends_with("whisper-tiny"));
    }
}
