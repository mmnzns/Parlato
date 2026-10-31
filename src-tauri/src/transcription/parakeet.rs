// Parlato : gere aussi `parakeet_rs::ParakeetUnified` (Parakeet Unified EN)
// et `parakeet_rs::Nemotron` (Nemotron 3.5 ASR), selon le ParakeetKind de la
// variante.
//
// Wrapper autour de `parakeet_rs::ParakeetTDT` pour reproduire l'API
// publique de WhisperEngine (load paresseux + transcribe_samples).
//
// Reference VoiceInk : FluidAudio expose `AsrManager.transcribe(samples,
// decoderState)`. Ici on s'appuie sur parakeet-rs qui encapsule deja le
// chargement ONNX, le mel spectrogram, l'encodeur, le joint net et le
// decoder TDT. On expose juste un handle avec rechargement paresseux si
// le repertoire change.
//
// Parlato : le processeur graphique (DirectML sous Windows, CUDA si le build
// a la feature `cuda-onnx`) est un reglage de l'utilisateur, lu a l'execution
// (`parakeet_gpu`). Si le chargement ou la premiere transcription echoue sur
// le GPU, on recharge sur le processeur, on refait la transcription et on
// retient l'echec jusqu'a la fermeture de l'application.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use parakeet_rs::{
    ExecutionConfig, ExecutionProvider, Nemotron, ParakeetTDT, ParakeetUnified, Transcriber,
};
use parking_lot::Mutex;
use tracing::{info, warn};

use voice_engine_models::{Kind, ModelHandle};

/// Parlato : famille de modele, chaque famille a son type parakeet-rs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParakeetKind {
    /// Parakeet TDT v2 / v3 / Ultra (`parakeet_rs::ParakeetTDT`).
    Tdt,
    /// Parakeet Unified EN (`parakeet_rs::ParakeetUnified`).
    Unified,
    /// Nemotron 3.5 ASR multilingue (`parakeet_rs::Nemotron`).
    Nemotron,
}

impl ParakeetKind {
    /// La famille d'un modele du catalogue du moteur (None : pas un modele
    /// de la famille Parakeet).
    pub fn from_catalog(kind: Kind) -> Option<Self> {
        match kind {
            Kind::Parakeet => Some(Self::Tdt),
            Kind::ParakeetUnified => Some(Self::Unified),
            Kind::Nemotron => Some(Self::Nemotron),
            _ => None,
        }
    }
}

/// Provider GPU de ce build et son nom : DirectML sous Windows (feature
/// `directml` de parakeet-rs, activee dans Cargo.toml pour Windows), CUDA si le
/// build a la feature `cuda-onnx` (build depuis les sources), rien ailleurs.
fn gpu_provider() -> Option<(ExecutionProvider, &'static str)> {
    #[cfg(feature = "cuda-onnx")]
    {
        Some((ExecutionProvider::Cuda, "cuda"))
    }
    #[cfg(all(windows, not(feature = "cuda-onnx")))]
    {
        Some((ExecutionProvider::DirectML, "directml"))
    }
    #[cfg(not(any(windows, feature = "cuda-onnx")))]
    {
        None
    }
}

/// Ce build sait-il utiliser le GPU ? (le reglage n'a de sens que si oui)
pub fn gpu_available() -> bool {
    gpu_provider().is_some()
}

/// Parakeet TDT : le decodeur/joint tourne une fois par jeton, sur de tout
/// petits tenseurs ; sur le GPU les allers-retours coutent plus qu'ils ne
/// rapportent. Par defaut l'encodeur seul va sur le GPU. A confirmer par
/// `parakeet_gpu_smoke` (qui mesure les deux).
const JOINT_ON_GPU: bool = false;

/// Le GPU est-il a utiliser pour ce chargement ? Fonction pure, testee.
fn gpu_wanted(preference: bool, available: bool, failed: bool) -> bool {
    preference && available && !failed
}

/// Ce que `parakeet_execution_provider` rapporte pour un chargement :
/// "cpu", "directml", "cuda" ou "cpu-fallback" (GPU demande mais retombe sur
/// le processeur).
fn reported_provider(
    on_gpu: Option<&'static str>,
    preference: bool,
    available: bool,
    failed: bool,
) -> &'static str {
    match on_gpu {
        Some(name) => name,
        None if preference && available && failed => "cpu-fallback",
        None => "cpu",
    }
}

/// Parlato : une famille de modele = un type parakeet-rs.
enum Engine {
    Tdt(ParakeetTDT),
    Unified(ParakeetUnified),
    Nemotron(Nemotron),
}

struct Loaded {
    path: PathBuf,
    kind: ParakeetKind,
    /// Parlato : threads ONNX du chargement (recharge si le reglage change).
    threads: usize,
    /// Parlato : le GPU etait demande a ce chargement (recharge si le reglage
    /// change). Faux apres un repli sur le processeur.
    gpu_asked: bool,
    /// Parlato : la session tourne reellement sur le GPU (nom du provider).
    on_gpu: Option<&'static str>,
    /// Parlato : une transcription a deja reussi sur cette session.
    verified: bool,
    engine: Engine,
}

pub struct ParakeetEngine {
    current: Mutex<Option<Loaded>>,
    /// Parlato : le GPU a echoue pendant cette session de l'application.
    gpu_failed: AtomicBool,
    /// Parlato : un repli est a signaler une fois dans la bulle.
    gpu_notice_pending: AtomicBool,
    /// Parlato : provider du dernier chargement (cf `reported_provider`).
    last_provider: Mutex<&'static str>,
    /// Parlato : verrou du dossier de modeles partage tant que le modele est charge.
    handle: Mutex<Option<ModelHandle>>,
}

impl ParakeetEngine {
    pub fn new() -> Self {
        Self {
            current: Mutex::new(None),
            gpu_failed: AtomicBool::new(false),
            gpu_notice_pending: AtomicBool::new(false),
            last_provider: Mutex::new("cpu"),
            handle: Mutex::new(None),
        }
    }

    /// Garde `handle` tant que le modele est charge (remplace le precedent).
    pub fn hold(&self, handle: Option<ModelHandle>) {
        *self.handle.lock() = handle;
    }

    /// Decharge le modele s'il vient de `dir`. Avant un Delete.
    pub fn unload_if_dir(&self, dir: &Path) {
        let loaded_here = self.current.lock().as_ref().is_some_and(|c| c.path == dir);
        if loaded_here {
            self.unload();
        }
    }

    /// Provider reellement utilise au dernier chargement.
    pub fn provider(&self) -> &'static str {
        *self.last_provider.lock()
    }

    /// Le GPU a-t-il echoue depuis le lancement de l'application ?
    pub fn gpu_failed(&self) -> bool {
        self.gpu_failed.load(Ordering::Relaxed)
    }

    /// Vrai une seule fois apres un repli GPU vers processeur : la bulle doit
    /// le dire a l'utilisateur.
    pub fn take_gpu_notice(&self) -> bool {
        self.gpu_notice_pending.swap(false, Ordering::Relaxed)
    }

    fn note_gpu_failure(&self) {
        self.gpu_failed.store(true, Ordering::Relaxed);
        self.gpu_notice_pending.store(true, Ordering::Relaxed);
        *self.last_provider.lock() = "cpu-fallback";
    }

    /// Charge le modele (repertoire contenant config.json + *.onnx + vocab.txt)
    /// si ce n'est pas deja celui-ci qui est charge. Bloquant.
    /// `threads` (Parlato) : threads ONNX, 0 = automatique (cf
    /// `whisper::auto_threads`, mesures dans `parakeet_threads_smoke`).
    /// `gpu` (Parlato) : reglage "utiliser la carte graphique". Si le
    /// chargement GPU echoue, le modele est charge sur le processeur.
    pub fn ensure_loaded(
        &self,
        model_dir: &Path,
        kind: ParakeetKind,
        threads: usize,
        gpu: bool,
    ) -> Result<()> {
        let threads = if threads == 0 {
            super::whisper::auto_threads() as usize
        } else {
            threads
        };
        let available = gpu_available();
        let want_gpu = gpu_wanted(gpu, available, self.gpu_failed());
        let mut guard = self.current.lock();
        if let Some(cur) = guard.as_ref() {
            if cur.path == model_dir && cur.threads == threads && cur.gpu_asked == want_gpu {
                return Ok(());
            }
        }
        // Libere l'ancien avant de charger le nouveau.
        *guard = None;

        let started = std::time::Instant::now();
        let mut on_gpu = None;
        let mut gpu_asked = want_gpu;
        let engine = match gpu_provider().filter(|_| want_gpu) {
            Some(provider) => {
                match load_engine(model_dir, kind, threads, Some(provider), JOINT_ON_GPU) {
                    Ok(e) => {
                        on_gpu = Some(provider.1);
                        e
                    }
                    Err(err) => {
                        warn!(
                            "parakeet: chargement {} impossible ({err}), repli sur le processeur",
                            provider.1
                        );
                        self.note_gpu_failure();
                        gpu_asked = false;
                        load_engine(model_dir, kind, threads, None, false)?
                    }
                }
            }
            None => load_engine(model_dir, kind, threads, None, false)?,
        };
        let used = reported_provider(on_gpu, gpu, available, self.gpu_failed());
        *self.last_provider.lock() = used;
        info!(
            asked = if gpu { "gpu" } else { "cpu" },
            used,
            threads,
            load_ms = started.elapsed().as_millis() as u64,
            "Parakeet charge"
        );
        *guard = Some(Loaded {
            path: model_dir.to_path_buf(),
            kind,
            threads,
            gpu_asked,
            on_gpu,
            verified: false,
            engine,
        });
        Ok(())
    }

    /// Transcrit un buffer PCM Float32 mono 16 kHz. Bloquant (inference).
    /// `language` n'est utilise que par Nemotron : parakeet TDT v2 et Unified
    /// sont anglais uniquement, v3 detecte la langue automatiquement.
    /// Parlato : si la premiere transcription d'une session GPU echoue, on
    /// recharge sur le processeur et on la refait une fois.
    pub fn transcribe_samples(&self, samples: &[f32], language: Option<&str>) -> Result<String> {
        let mut guard = self.current.lock();
        let loaded = guard
            .as_mut()
            .ok_or_else(|| anyhow!("modele Parakeet non charge"))?;
        let first_gpu_run = loaded.on_gpu.is_some() && !loaded.verified;
        match run_engine(&mut loaded.engine, samples, language) {
            Ok(text) => {
                loaded.verified = true;
                Ok(text)
            }
            Err(err) if first_gpu_run => {
                warn!(
                    "parakeet: transcription GPU en echec ({err}), nouvel essai sur le processeur"
                );
                let (path, kind, threads) = (loaded.path.clone(), loaded.kind, loaded.threads);
                *guard = None;
                let engine = load_engine(&path, kind, threads, None, false)?;
                let mut cpu = Loaded {
                    path,
                    kind,
                    threads,
                    gpu_asked: false,
                    on_gpu: None,
                    verified: false,
                    engine,
                };
                let result = run_engine(&mut cpu.engine, samples, language);
                if result.is_ok() {
                    // Le processeur a reussi la ou le GPU a echoue : c'est bien le
                    // GPU qui est en cause. Sinon l'erreur vient de l'audio, on
                    // reessaiera le GPU a la prochaine dictee.
                    cpu.verified = true;
                    self.note_gpu_failure();
                }
                *guard = Some(cpu);
                result
            }
            Err(err) => Err(err),
        }
    }

    /// Libere la memoire (ONNX session). Utile quand l'utilisateur change de
    /// modele ou desactive la source.
    pub fn unload(&self) {
        *self.current.lock() = None;
        *self.handle.lock() = None;
    }
}

/// Parlato : construit la session ONNX. `gpu` = provider GPU a demander ;
/// `joint_on_gpu` : le decodeur/joint (TDT) y va aussi.
fn load_engine(
    model_dir: &Path,
    kind: ParakeetKind,
    threads: usize,
    gpu: Option<(ExecutionProvider, &'static str)>,
    joint_on_gpu: bool,
) -> Result<Engine> {
    let cpu_cfg = ExecutionConfig::new().with_intra_threads(threads);
    let cfg = match gpu {
        // ONNX Runtime exige de couper le "memory pattern" avec DirectML
        // (formes d'entree variables) ; sans effet nuisible avec CUDA.
        Some((provider, _)) => cpu_cfg
            .clone()
            .with_execution_provider(provider)
            .with_custom_configure(|b| Ok(b.with_memory_pattern(false)?)),
        None => cpu_cfg.clone(),
    };
    Ok(match kind {
        ParakeetKind::Tdt => {
            let joint_cfg = if joint_on_gpu { cfg.clone() } else { cpu_cfg };
            Engine::Tdt(
                ParakeetTDT::from_pretrained_with_joint_config(
                    model_dir,
                    Some(cfg),
                    Some(joint_cfg),
                )
                .map_err(|e| anyhow!("parakeet load: {e:?}"))?,
            )
        }
        ParakeetKind::Unified => Engine::Unified(
            ParakeetUnified::from_pretrained(model_dir, Some(cfg))
                .map_err(|e| anyhow!("parakeet unified load: {e:?}"))?,
        ),
        ParakeetKind::Nemotron => Engine::Nemotron(
            Nemotron::from_pretrained(model_dir, Some(cfg))
                .map_err(|e| anyhow!("nemotron load: {e:?}"))?,
        ),
    })
}

/// Parlato : une transcription sur la session chargee.
fn run_engine(engine: &mut Engine, samples: &[f32], language: Option<&str>) -> Result<String> {
    // Pas de timestamps pour l'usage dictee : on veut juste le texte.
    // parakeet-rs attend un Vec<f32>. Une copie est inevitable ici.
    let result = match engine {
        Engine::Tdt(e) => {
            // Plus de 15 s : fenetres qui se recouvrent (parakeet_windows),
            // une passe unique fait deriver le modele sur un long audio.
            // Unified (flux a memoire) et Nemotron (etat de flux, relance
            // de langue) gardent leur chemin : derive non constatee.
            if samples.len() > super::parakeet_windows::SINGLE_PASS_MAX_SAMPLES {
                return transcribe_tdt_windows(e, samples);
            }
            e.transcribe_samples(samples.to_vec(), 16000, 1, None)
        }
        Engine::Unified(e) => e.transcribe_samples(samples.to_vec(), 16000, 1, None),
        Engine::Nemotron(e) => return transcribe_nemotron(e, samples, language),
    }
    .map_err(|e| anyhow!("parakeet transcribe: {e:?}"))?;
    Ok(result.text)
}

/// Parlato : TDT sur un audio de plus de 15 s, fenetre par fenetre.
fn transcribe_tdt_windows(e: &mut ParakeetTDT, samples: &[f32]) -> Result<String> {
    let (text, windows) = super::parakeet_windows::transcribe_in_windows(samples, |slice| {
        e.transcribe_samples(slice.to_vec(), 16000, 1, None)
            .map(|r| r.tokens)
            .map_err(|err| anyhow!("parakeet transcribe: {err:?}"))
    })?;
    tracing::info!(
        "parakeet: {:.1}s decoupees en {windows} fenetres",
        samples.len() as f32 / 16000.0
    );
    Ok(text)
}

/// Parlato : La langue de dictee ("fr") est passee au modele, sinon "auto"
/// (le modele choisit seul). Force sur une langue qui n'est pas celle parlee,
/// Nemotron rend un texte vide : dans ce cas on refait un passage en "auto",
/// pour qu'une phrase en anglais dictee en mode francais ne soit pas perdue.
fn transcribe_nemotron(
    e: &mut Nemotron,
    samples: &[f32],
    language: Option<&str>,
) -> Result<String> {
    let text = nemotron_pass(e, samples, nemotron_lang(language))?;
    if !text.is_empty() {
        return Ok(text);
    }
    nemotron_pass(e, samples, "auto")
}

/// Un passage complet ; Nemotron garde un etat de flux entre deux appels, on
/// le remet a zero a chaque fois.
fn nemotron_pass(e: &mut Nemotron, samples: &[f32], lang: &str) -> Result<String> {
    e.reset();
    if e.set_target_lang(lang).is_err() {
        // Code inconnu du modele : on retombe sur la detection automatique.
        let _ = e.set_target_lang("auto");
    }
    let text = e
        .transcribe_audio(samples)
        .map_err(|err| anyhow!("nemotron transcribe: {err:?}"))?;
    // Le decodeur laisse parfois deux espaces entre deux phrases.
    Ok(text.split_whitespace().collect::<Vec<_>>().join(" "))
}

fn nemotron_lang(language: Option<&str>) -> &str {
    // Quelques langues n'existent chez Nemotron que sous forme "xx-YY".
    match language {
        Some("ja") => "ja-JP",
        Some("zh") => "zh-CN",
        Some("vi") => "vi-VN",
        Some("he") => "he-IL",
        Some("th") => "th-TH",
        Some("mt") => "mt-MT",
        Some(l) if !l.is_empty() => l,
        _ => "auto",
    }
}

impl Default for ParakeetEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Default)]
pub struct ParakeetEngineState(pub Arc<ParakeetEngine>);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nemotron_lang_maps_short_codes() {
        assert_eq!(nemotron_lang(Some("fr")), "fr");
        assert_eq!(nemotron_lang(Some("ja")), "ja-JP");
        assert_eq!(nemotron_lang(Some("zh")), "zh-CN");
        assert_eq!(nemotron_lang(Some("auto")), "auto");
        assert_eq!(nemotron_lang(Some("")), "auto");
        assert_eq!(nemotron_lang(None), "auto");
    }

    #[test]
    fn gpu_is_used_only_when_asked_available_and_not_failed() {
        assert!(gpu_wanted(true, true, false));
        assert!(!gpu_wanted(false, true, false));
        assert!(!gpu_wanted(true, false, false));
        // Apres un echec, plus de GPU jusqu'a la fermeture de l'application.
        assert!(!gpu_wanted(true, true, true));
    }

    #[test]
    fn reported_provider_names_what_ran() {
        assert_eq!(
            reported_provider(Some("directml"), true, true, false),
            "directml"
        );
        assert_eq!(reported_provider(Some("cuda"), true, true, false), "cuda");
        assert_eq!(reported_provider(None, false, true, false), "cpu");
        // GPU demande mais retombe sur le processeur.
        assert_eq!(reported_provider(None, true, true, true), "cpu-fallback");
        // Build sans GPU (Mac) : le reglage est ignore, ce n'est pas un repli.
        assert_eq!(reported_provider(None, true, false, false), "cpu");
    }

    #[test]
    fn engine_starts_on_the_processor_without_failure() {
        let engine = ParakeetEngine::new();
        assert_eq!(engine.provider(), "cpu");
        assert!(!engine.gpu_failed());
        assert!(!engine.take_gpu_notice());
        engine.note_gpu_failure();
        assert!(engine.gpu_failed());
        assert_eq!(engine.provider(), "cpu-fallback");
        // Le message n'est donne qu'une fois.
        assert!(engine.take_gpu_notice());
        assert!(!engine.take_gpu_notice());
    }

    /// Smoke test against a real model directory :
    /// `PARLATO_PARAKEET_DIR=<dir> PARLATO_PARAKEET_KIND=unified|nemotron|tdt
    /// PARLATO_WAV=<16 kHz wav> cargo test --lib parakeet_smoke -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn parakeet_smoke() {
        let dir = std::env::var("PARLATO_PARAKEET_DIR").expect("PARLATO_PARAKEET_DIR");
        let wav = std::env::var("PARLATO_WAV").expect("PARLATO_WAV");
        let kind = match std::env::var("PARLATO_PARAKEET_KIND").as_deref() {
            Ok("unified") => ParakeetKind::Unified,
            Ok("nemotron") => ParakeetKind::Nemotron,
            _ => ParakeetKind::Tdt,
        };
        let engine = ParakeetEngine::new();
        let t0 = std::time::Instant::now();
        engine
            .ensure_loaded(
                Path::new(&dir),
                kind,
                0,
                std::env::var("PARLATO_GPU").is_ok(),
            )
            .unwrap();
        let loaded_in = t0.elapsed().as_secs_f32();
        let samples = crate::transcription::whisper::read_wav_as_f32(Path::new(&wav)).unwrap();
        let t1 = std::time::Instant::now();
        let lang = std::env::var("PARLATO_LANG").ok();
        let text = engine
            .transcribe_samples(&samples, lang.as_deref())
            .unwrap();
        println!(
            "--- load {loaded_in:.1}s, {:.1}s audio in {:.1}s ---\n{text}\n---",
            samples.len() as f32 / 16000.0,
            t1.elapsed().as_secs_f32()
        );
        assert!(!text.trim().is_empty());
    }

    /// Compare passe unique et fenetres sur un vrai enregistrement (Parakeet
    /// TDT : v2, v3 ou Ultra) :
    /// `PARLATO_PARAKEET_DIR=<modele tdt> PARLATO_WAV=<wav 16 kHz>
    /// cargo test --release --lib parakeet_long_audio -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn parakeet_long_audio() {
        use crate::transcription::parakeet_windows::{plan_windows, SAMPLE_RATE};
        let dir = std::env::var("PARLATO_PARAKEET_DIR").expect("PARLATO_PARAKEET_DIR");
        let wav = std::env::var("PARLATO_WAV").expect("PARLATO_WAV");
        let samples = crate::transcription::whisper::read_wav_as_f32(Path::new(&wav)).unwrap();
        let engine = ParakeetEngine::new();
        engine
            .ensure_loaded(Path::new(&dir), ParakeetKind::Tdt, 0, false)
            .unwrap();
        let t0 = std::time::Instant::now();
        let single = {
            let mut g = engine.current.lock();
            match &mut g.as_mut().unwrap().engine {
                Engine::Tdt(e) => {
                    e.transcribe_samples(samples.clone(), 16000, 1, None)
                        .unwrap()
                        .text
                }
                _ => panic!("il faut un modele Parakeet TDT"),
            }
        };
        println!(
            "=== passe unique ({:.1}s audio, {} car., {:.1}s)
{single}
",
            samples.len() as f32 / SAMPLE_RATE as f32,
            single.len(),
            t0.elapsed().as_secs_f32()
        );
        let t1 = std::time::Instant::now();
        let chunked = engine.transcribe_samples(&samples, None).unwrap();
        println!(
            "=== fenetres ({} car., {:.1}s)
{chunked}
",
            chunked.len(),
            t1.elapsed().as_secs_f32()
        );
        for w in plan_windows(&samples) {
            println!(
                "fenetre {:.2}s..{:.2}s (propre des {:.2}s)",
                w.start as f32 / SAMPLE_RATE as f32,
                w.end as f32 / SAMPLE_RATE as f32,
                w.own_from as f32 / SAMPLE_RATE as f32
            );
        }
        assert!(!chunked.trim().is_empty());
    }

    /// Parlato : Parakeet TDT sur le processeur et sur la carte graphique
    /// (DirectML ; CUDA si le build a `cuda-onnx`). Charge chaque variante,
    /// mesure un passage de chauffe puis trois passages, et compare les textes :
    /// `PARLATO_PARAKEET_DIR=<tdt dir> PARLATO_WAV=<wav 16 kHz>
    /// cargo test --release --lib parakeet_gpu_smoke -- --ignored --nocapture`.
    /// Variantes : cpu, gpu (encodeur sur le GPU, joint sur le processeur,
    /// comme Parlato) et gpu+joint (les deux sur le GPU). Option :
    /// `PARLATO_THREADS=12`.
    #[test]
    #[ignore]
    fn parakeet_gpu_smoke() {
        let dir = std::env::var("PARLATO_PARAKEET_DIR").expect("PARLATO_PARAKEET_DIR");
        let wav = std::env::var("PARLATO_WAV").expect("PARLATO_WAV");
        let samples = crate::transcription::whisper::read_wav_as_f32(Path::new(&wav)).unwrap();
        let threads: usize = std::env::var("PARLATO_THREADS")
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or_else(|| crate::transcription::whisper::auto_threads() as usize);
        let gpu = gpu_provider().expect("ce build n'a pas de provider GPU");
        let secs = samples.len() as f32 / 16000.0;
        println!("audio {secs:.1}s, {threads} threads, GPU = {}", gpu.1);

        let mut texts: Vec<(String, String)> = Vec::new();
        for (name, provider, joint_on_gpu) in [
            ("cpu", None, false),
            ("gpu", Some(gpu), false),
            ("gpu+joint", Some(gpu), true),
        ] {
            let t = std::time::Instant::now();
            let mut engine = match load_engine(
                Path::new(&dir),
                ParakeetKind::Tdt,
                threads,
                provider,
                joint_on_gpu,
            ) {
                Ok(e) => e,
                Err(e) => {
                    println!("{name}: chargement impossible : {e}");
                    continue;
                }
            };
            let load_ms = t.elapsed().as_millis();
            let mut times = Vec::new();
            let mut text = String::new();
            // 1 passage de chauffe puis 3 mesures.
            for _ in 0..4 {
                let st = std::time::Instant::now();
                match run_engine(&mut engine, &samples, None) {
                    Ok(t) => text = t,
                    Err(e) => {
                        println!("{name}: transcription impossible : {e}");
                        break;
                    }
                }
                times.push(st.elapsed().as_millis());
            }
            if times.len() == 4 {
                let best = times[1..].iter().min().unwrap();
                println!(
                    "{name}: chargement {load_ms} ms, chauffe {} ms, passages {:?} ms, meilleur {best} ms ({:.1}x temps reel)",
                    times[0],
                    &times[1..],
                    secs * 1000.0 / *best as f32
                );
                println!("{name} texte ({} car.) : {text}", text.len());
                texts.push((name.to_string(), text));
            }
        }
        if let Some((_, reference)) = texts.first() {
            for (name, text) in &texts[1..] {
                println!("{name} identique a cpu : {}", text == reference);
            }
        }
    }

    /// Parlato : temps de Parakeet TDT selon `intra_threads` (4 par defaut
    /// dans parakeet-rs), sur 6 s puis tout le fichier :
    /// `PARLATO_PARAKEET_DIR=<tdt dir> PARLATO_WAV=<wav>
    /// cargo test --release --lib parakeet_threads_smoke -- --ignored --nocapture`.
    /// Option : `PARLATO_THREADS=4,6,9`.
    #[test]
    #[ignore]
    fn parakeet_threads_smoke() {
        let dir = std::env::var("PARLATO_PARAKEET_DIR").expect("PARLATO_PARAKEET_DIR");
        let wav = std::env::var("PARLATO_WAV").expect("PARLATO_WAV");
        let all = crate::transcription::whisper::read_wav_as_f32(Path::new(&wav)).unwrap();
        let short = &all[..all.len().min(6 * 16_000)];
        let threads_list: Vec<usize> = std::env::var("PARLATO_THREADS")
            .map(|v| v.split(',').filter_map(|n| n.trim().parse().ok()).collect())
            .unwrap_or_else(|_| vec![2, 4, 6, 8, 12, 16]);
        for threads in threads_list {
            let cfg = parakeet_rs::ExecutionConfig::new().with_intra_threads(threads);
            let mut m = ParakeetTDT::from_pretrained(Path::new(&dir), Some(cfg)).unwrap();
            for samples in [short, &all[..]] {
                let mut best = u128::MAX;
                for _ in 0..3 {
                    let st = std::time::Instant::now();
                    let r = m
                        .transcribe_samples(samples.to_vec(), 16000, 1, None)
                        .unwrap();
                    best = best.min(st.elapsed().as_millis());
                    assert!(!r.text.trim().is_empty());
                }
                println!(
                    "threads={threads:>2} audio={:>4.1}s: best of 3 = {best} ms",
                    samples.len() as f32 / 16000.0
                );
            }
        }
    }
}
