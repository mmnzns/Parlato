// VAD (Voice Activity Detection) Silero, par `voice_engine_stt::vad::VadDetector`
// (voice-engine v0.3.2, modele "vad" du catalogue).
//
// Reference VoiceInk :
// - Transcription/Core/Whisper/VADModelManager.swift : le modele Silero.
// - Transcription/Core/Whisper/LibWhisper.swift L73-85 : activation via
//   UserDefault IsVADEnabled + params threshold 0.50, min_speech 250 ms,
//   speech_pad 30 ms.
//
// Le moteur rend les plages de parole en millisecondes, sans marge (seuil 0,5,
// parole minimale 250 ms, silence minimal 300 ms ; ces reglages sont les siens,
// Parlato ne les change pas). On les convertit ici en plages d'echantillons
// avec la marge de 30 ms de VoiceInk de chaque cote.
//
// Le detecteur (moins de 1 Mo) est charge une fois et reste charge, derriere un
// `Mutex` (il est `Send`, utilise par un seul fil a la fois), avec le verrou du
// dossier partage (`ModelHandle`). Il part avec le delai "Garder les modeles
// prets", avec le Delete du modele VAD et avec "Delete all data" (`unload`,
// `unload_if_under`). La chauffe du lancement le charge, pour que la premiere
// dictee ne paie pas.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use tracing::{debug, info, warn};
use voice_engine_models::ModelHandle;
use voice_engine_stt::vad::VadDetector;

use super::speech;

const SAMPLE_RATE: u64 = 16_000;
/// Marge de VoiceInk (`speech_pad_ms`) de chaque cote d'une plage.
const SPEECH_PAD_MS: u64 = 30;

/// Un detecteur charge, avec le dossier d'ou il vient et le verrou du dossier
/// partage. L'ordre des champs est celui de la chute : le detecteur d'abord,
/// puis le verrou (un Delete d'une autre application peut alors effacer).
struct Loaded<D> {
    detector: D,
    dir: PathBuf,
    _handle: Option<ModelHandle>,
}

/// Le detecteur charge, s'il y en a un. Generique pour tester le cycle de vie
/// sans modele (les tests mettent un faux detecteur).
struct Slot<D>(Option<Loaded<D>>);

impl<D> Default for Slot<D> {
    fn default() -> Self {
        Self(None)
    }
}

impl<D> Slot<D> {
    /// Charge avec `load` si rien n'est charge ou si le dossier change (l'ancien
    /// et son verrou tombent d'abord). Rend le detecteur et `true` quand il
    /// vient d'etre charge. `handle` remplace le verrou precedent s'il y en a
    /// un ; un echec laisse la place vide.
    fn ensure(
        &mut self,
        dir: &Path,
        handle: Option<ModelHandle>,
        load: impl FnOnce(&Path) -> Result<D>,
    ) -> Result<(&mut D, bool)> {
        let same = self.0.as_ref().is_some_and(|l| l.dir == dir);
        if !same {
            self.0 = None;
            let detector = load(dir)?;
            self.0 = Some(Loaded {
                detector,
                dir: dir.to_path_buf(),
                _handle: handle,
            });
        } else if let Some(cur) = self.0.as_mut() {
            if handle.is_some() {
                cur._handle = handle;
            }
        }
        let loaded_now = !same;
        let cur = self.0.as_mut().expect("charge juste au-dessus");
        Ok((&mut cur.detector, loaded_now))
    }

    /// Vrai si quelque chose etait charge.
    fn unload(&mut self) -> bool {
        self.0.take().is_some()
    }

    /// Decharge s'il se trouve dans `dir`. Vrai si c'est le cas.
    fn unload_if_under(&mut self, dir: &Path) -> bool {
        if self.0.as_ref().is_some_and(|l| l.dir.starts_with(dir)) {
            return self.unload();
        }
        false
    }

    #[cfg(test)]
    fn is_loaded(&self) -> bool {
        self.0.is_some()
    }
}

#[derive(Default)]
pub struct VadEngine {
    slot: Mutex<Slot<VadDetector>>,
}

impl VadEngine {
    /// Oublie le detecteur et son verrou. Apres le delai "Garder les modeles
    /// prets" ; attend une detection en cours.
    pub fn unload(&self) {
        if self.slot.lock().unload() {
            info!("Modele VAD decharge");
        }
    }

    /// Decharge le detecteur s'il vient du dossier `dir`. Avant un Delete.
    pub fn unload_if_under(&self, dir: &Path) {
        if self.slot.lock().unload_if_under(dir) {
            info!("Modele VAD decharge");
        }
    }

    /// Un detecteur est-il charge ?
    #[cfg(test)]
    pub fn is_loaded(&self) -> bool {
        self.slot.lock().is_loaded()
    }

    /// Charge le detecteur si ce n'est pas deja fait. `handle` : le verrou du
    /// dossier partage, garde tant qu'il est charge. Rend la duree du
    /// chargement (zero s'il l'etait).
    pub fn ensure_loaded(&self, model_dir: &Path, handle: Option<ModelHandle>) -> Result<Duration> {
        let st = Instant::now();
        let mut slot = self.slot.lock();
        let (_, loaded_now) = slot.ensure(model_dir, handle, load_detector)?;
        if !loaded_now {
            return Ok(Duration::ZERO);
        }
        let d = st.elapsed();
        info!(load_ms = d.as_millis() as u64, "Detecteur VAD charge");
        Ok(d)
    }

    /// Les plages d'echantillons (debut, fin) de parole de `samples`, marge de
    /// 30 ms comprise. `model_dir` : le dossier du modele "vad" ; le detecteur
    /// est charge ici s'il ne l'est pas encore (`handle`, cf `ensure_loaded`).
    pub fn ranges(
        &self,
        model_dir: &Path,
        handle: Option<ModelHandle>,
        samples: &[f32],
    ) -> Result<Vec<(usize, usize)>> {
        let st = Instant::now();
        let spans = {
            let mut slot = self.slot.lock();
            let (detector, loaded_now) = slot.ensure(model_dir, handle, load_detector)?;
            if loaded_now {
                info!(load_ms = st.elapsed().as_millis() as u64, "Detecteur VAD charge");
            }
            detector.detect(samples).map_err(|e| anyhow!("VAD: {e}"))?
        };
        let ranges = spans_to_sample_ranges(&spans, samples.len());
        debug!(
            duration_ms = st.elapsed().as_millis() as u64,
            ranges = ranges.len(),
            "VAD segmentation terminee"
        );
        Ok(ranges)
    }
}

fn load_detector(dir: &Path) -> Result<VadDetector> {
    VadDetector::load(dir).map_err(|e| anyhow!("VAD: {e}"))
}

/// Parlato : 0,5 s de silence a 16 kHz pour la chauffe.
const WARM_UP_SAMPLES: usize = SAMPLE_RATE as usize / 2;

/// Faut-il chauffer la VAD au lancement ? Oui quand elle est activee et que
/// son modele est installe dans le dossier partage.
pub fn should_warm_up(vad_enabled: bool, model_installed: bool) -> bool {
    vad_enabled && model_installed
}

/// Premier usage du moteur apres le lancement : whisper.cpp initialise ses
/// backends ggml une fois pour le processus (mesure : ~6,9 s au premier appel,
/// ~72 ms ensuite, sur un clip de 30 s). On le paie ici, sur un fil, avant la
/// premiere dictee : le detecteur est charge (il reste charge) puis essaye sur
/// 0,5 s de silence. Retourne la duree totale.
pub fn warm_up(engine: &VadEngine, model_dir: &Path, handle: Option<ModelHandle>) -> Result<Duration> {
    let st = Instant::now();
    engine.ensure_loaded(model_dir, handle)?;
    engine
        .ranges(model_dir, None, &vec![0.0f32; WARM_UP_SAMPLES])
        .map(|_| st.elapsed())
}

/// Au lancement, apres l'ouverture du dossier de modeles : charge le detecteur
/// VAD sur un fil, sans bloquer le demarrage ni une dictee (une dictee qui
/// arrive pendant la chauffe paie seulement le reste). Rien si la VAD est
/// coupee ou son modele absent.
pub fn spawn_warm_up(app: &tauri::AppHandle) {
    use tauri::Manager;
    let enabled = super::pipeline::is_vad_enabled(app);
    let store = app.state::<crate::models::ModelStoreState>().0.clone();
    let engine = app.state::<crate::commands::vad::VadEngineState>().0.clone();
    let dir = match store.shared().status(crate::commands::vad::VAD_ID) {
        voice_engine_models::Status::Installed { dir, .. } => Some(dir),
        _ => None,
    };
    if !should_warm_up(enabled, dir.is_some()) {
        return;
    }
    let Some(dir) = dir else { return };
    let spawned = std::thread::Builder::new()
        .name("vad-warm-up".into())
        .spawn(move || {
            // Le verrou du dossier partage reste au detecteur jusqu'au dechargement.
            let handle = store.hold(crate::commands::vad::VAD_ID);
            match warm_up(&engine, &dir, handle) {
                Ok(d) => info!(warm_up_ms = d.as_millis() as u64, "VAD chauffee au lancement"),
                Err(e) => warn!(error = %e, "Chauffe de la VAD echouee"),
            }
        });
    if let Err(e) = spawned {
        warn!(error = %e, "Chauffe de la VAD : fil non demarre");
    }
}

/// Plages de parole en ms -> plages d'echantillons : marge de `SPEECH_PAD_MS`
/// de chaque cote, bornees a l'audio, plages qui se touchent fusionnees (la
/// marge ne doit pas dupliquer de l'audio), plages vides retirees.
pub fn spans_to_sample_ranges(spans: &[(u64, u64)], total_samples: usize) -> Vec<(usize, usize)> {
    let to_samples = |ms: u64| (ms * SAMPLE_RATE / 1000) as usize;
    let mut out: Vec<(usize, usize)> = Vec::with_capacity(spans.len());
    for &(start_ms, end_ms) in spans {
        let start = to_samples(start_ms.saturating_sub(SPEECH_PAD_MS)).min(total_samples);
        let end = to_samples(end_ms + SPEECH_PAD_MS).min(total_samples);
        if end <= start {
            continue;
        }
        match out.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => out.push((start, end)),
        }
    }
    out
}

/// Echantillons du WAV + plages (start, end) a transcrire.
pub type VadOutput = (Vec<f32>, Vec<(usize, usize)>);

/// Ce que le pipeline passe a la VAD : le moteur, le dossier du modele "vad" et
/// le verrou du dossier partage (garde par le detecteur s'il se charge).
pub type VadUse = (std::sync::Arc<VadEngine>, PathBuf, Option<ModelHandle>);

/// Applique la VAD sur un WAV 16 kHz mono et retourne les plages de samples
/// a transcrire (marge de 30 ms comprise).
pub fn run_vad_on_wav(
    engine: &VadEngine,
    model_dir: &Path,
    handle: Option<ModelHandle>,
    wav_path: &Path,
) -> Result<VadOutput> {
    let samples = speech::read_wav_as_f32(wav_path)?;
    let ranges = engine.ranges(model_dir, handle, &samples)?;
    if ranges.is_empty() {
        warn!("VAD n'a detecte aucune parole");
    }
    Ok((samples, ranges))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn spans_get_a_30_ms_margin_inside_the_audio() {
        // 1.0 s .. 2.0 s -> 0.97 s .. 2.03 s at 16 kHz.
        assert_eq!(
            spans_to_sample_ranges(&[(1000, 2000)], 16_000 * 10),
            vec![(15_520, 32_480)]
        );
        // The margin stops at the edges of the audio.
        assert_eq!(
            spans_to_sample_ranges(&[(10, 990)], 16_000),
            vec![(0, 16_000)]
        );
    }

    #[test]
    fn margins_that_touch_are_merged_not_duplicated() {
        // 40 ms apart: the two 30 ms margins overlap.
        let r = spans_to_sample_ranges(&[(1000, 2000), (2040, 3000)], 16_000 * 10);
        assert_eq!(r, vec![(15_520, 48_480)]);
        // Far apart: two ranges.
        let r = spans_to_sample_ranges(&[(1000, 2000), (4000, 5000)], 16_000 * 10);
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn warm_up_needs_vad_on_and_the_model_installed() {
        assert!(should_warm_up(true, true));
        assert!(!should_warm_up(true, false));
        assert!(!should_warm_up(false, true));
        assert!(!should_warm_up(false, false));
    }

    #[test]
    fn empty_and_backwards_spans_are_dropped() {
        assert!(spans_to_sample_ranges(&[], 16_000).is_empty());
        assert!(spans_to_sample_ranges(&[(5000, 5000)], 16_000).is_empty());
        assert!(spans_to_sample_ranges(&[(5000, 100)], 16_000).is_empty());
        // Entirely past the end of the audio.
        assert!(spans_to_sample_ranges(&[(9000, 9500)], 16_000).is_empty());
    }

    /// A stand-in for the detector: counts its loads and records when it drops.
    struct Fake {
        id: u32,
        log: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl Drop for Fake {
        fn drop(&mut self) {
            self.log.lock().unwrap().push(format!("drop {}", self.id));
        }
    }

    type Log = std::sync::Arc<std::sync::Mutex<Vec<String>>>;

    fn loader(log: &Log, loads: &std::cell::Cell<u32>) -> impl FnOnce(&Path) -> Result<Fake> {
        let log = log.clone();
        let id = loads.get() + 1;
        loads.set(id);
        move |_| {
            log.lock().unwrap().push(format!("load {id}"));
            Ok(Fake { id, log })
        }
    }

    #[test]
    fn the_detector_is_loaded_once_and_reused() {
        let (log, loads) = (Log::default(), std::cell::Cell::new(0));
        let mut slot = Slot::<Fake>::default();
        let dir = Path::new("/models/vad");
        for call in 0..3 {
            let (d, loaded_now) = slot.ensure(dir, None, loader(&log, &loads)).unwrap();
            assert_eq!(d.id, 1, "call {call}");
            assert_eq!(loaded_now, call == 0, "call {call}");
        }
        assert!(slot.is_loaded());
        assert_eq!(*log.lock().unwrap(), vec!["load 1"]);
    }

    #[test]
    fn unload_drops_the_detector_and_the_next_use_loads_again() {
        let (log, loads) = (Log::default(), std::cell::Cell::new(0));
        let mut slot = Slot::<Fake>::default();
        let dir = Path::new("/models/vad");
        assert!(!slot.unload(), "nothing to drop yet");
        slot.ensure(dir, None, loader(&log, &loads)).unwrap();
        assert!(slot.unload());
        assert!(!slot.is_loaded());
        let (d, loaded_now) = slot.ensure(dir, None, loader(&log, &loads)).unwrap();
        assert!(loaded_now);
        assert_eq!(d.id, 2);
        assert_eq!(*log.lock().unwrap(), vec!["load 1", "drop 1", "load 2"]);
    }

    #[test]
    fn a_delete_drops_the_detector_only_when_it_comes_from_that_folder() {
        let (log, loads) = (Log::default(), std::cell::Cell::new(0));
        let mut slot = Slot::<Fake>::default();
        slot.ensure(Path::new("/models/vad"), None, loader(&log, &loads)).unwrap();
        assert!(!slot.unload_if_under(Path::new("/models/whisper-tiny")));
        assert!(slot.is_loaded());
        assert!(slot.unload_if_under(Path::new("/models/vad")));
        assert!(!slot.is_loaded());
        // The folder above works too (Delete all data).
        slot.ensure(Path::new("/models/vad"), None, loader(&log, &loads)).unwrap();
        assert!(slot.unload_if_under(Path::new("/models")));
        assert_eq!(
            *log.lock().unwrap(),
            vec!["load 1", "drop 1", "load 2", "drop 2"]
        );
    }

    #[test]
    fn another_folder_reloads_and_the_old_detector_goes_first() {
        let (log, loads) = (Log::default(), std::cell::Cell::new(0));
        let mut slot = Slot::<Fake>::default();
        slot.ensure(Path::new("/a/vad"), None, loader(&log, &loads)).unwrap();
        let (d, loaded_now) = slot
            .ensure(Path::new("/b/vad"), None, loader(&log, &loads))
            .unwrap();
        assert!(loaded_now);
        assert_eq!(d.id, 2);
        assert_eq!(*log.lock().unwrap(), vec!["load 1", "drop 1", "load 2"]);
    }

    #[test]
    fn a_failed_load_leaves_nothing_loaded() {
        let (log, loads) = (Log::default(), std::cell::Cell::new(0));
        let mut slot = Slot::<Fake>::default();
        slot.ensure(Path::new("/a/vad"), None, loader(&log, &loads)).unwrap();
        let err = slot
            .ensure(Path::new("/b/vad"), None, |_| Err(anyhow!("VAD: model missing")))
            .map(|_| ())
            .unwrap_err();
        assert!(err.to_string().contains("model missing"));
        assert!(!slot.is_loaded());
    }

    #[test]
    fn the_engine_stays_empty_when_the_model_is_not_there() {
        // An empty folder: the engine says ModelMissing, nothing is kept.
        let tmp = tempfile::tempdir().unwrap();
        let engine = VadEngine::default();
        assert!(!engine.is_loaded());
        assert!(engine.ensure_loaded(tmp.path(), None).is_err());
        assert!(engine.ranges(tmp.path(), None, &[0.0; 16_000]).is_err());
        assert!(warm_up(&engine, tmp.path(), None).is_err());
        assert!(!engine.is_loaded());
        // Unloading an empty engine is harmless.
        engine.unload();
        engine.unload_if_under(tmp.path());
        assert!(!engine.is_loaded());
    }

    /// The model folder a smoke test gives the engine: `PARLATO_VAD_MODEL` is
    /// the .bin file; the engine wants a folder holding the catalog's name.
    /// Returns the folder (a temp folder with a link when the name differs).
    pub(crate) fn vad_dir_for(model: &Path) -> (PathBuf, Option<tempfile::TempDir>) {
        let name = voice_engine_models::catalog::find("vad").unwrap().files[0].name;
        if model.file_name().and_then(|n| n.to_str()) == Some(name) {
            return (model.parent().unwrap().to_path_buf(), None);
        }
        let tmp = tempfile::tempdir().unwrap();
        std::fs::hard_link(model, tmp.path().join(name))
            .or_else(|_| std::fs::copy(model, tmp.path().join(name)).map(|_| ()))
            .unwrap();
        (tmp.path().to_path_buf(), Some(tmp))
    }

    /// Parlato : temps de la VAD du moteur (detecteur charge une fois, 4 threads
    /// fixes, regle du moteur), nombre de plages seulement :
    /// `PARLATO_VAD_MODEL=<ggml-silero.bin> PARLATO_WAV=<16 kHz wav>
    /// cargo test --release --lib vad_threads_smoke -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn vad_threads_smoke() {
        let model = std::env::var("PARLATO_VAD_MODEL").expect("PARLATO_VAD_MODEL");
        let wav = std::env::var("PARLATO_WAV").expect("PARLATO_WAV");
        let samples = speech::read_wav_as_f32(Path::new(&wav)).unwrap();
        println!("audio: {:.1} s", samples.len() as f32 / 16000.0);
        let (dir, _keep) = vad_dir_for(Path::new(&model));
        let mut detector = VadDetector::load(&dir).unwrap();
        let mut best = u128::MAX;
        let mut n = 0;
        for _ in 0..3 {
            let st = Instant::now();
            n = detector.detect(&samples).unwrap().len();
            best = best.min(st.elapsed().as_millis());
        }
        println!("engine vad (loaded detector, threads fixed by the engine): best of 3 = {best} ms, {n} spans");
        assert!(n > 0);
        let engine = VadEngine::default();
        let ranges = engine.ranges(&dir, None, &samples).unwrap();
        println!("sample ranges after the 30 ms margin: {}", ranges.len());
        assert!(!ranges.is_empty());
    }

    /// Parlato : le demarrage a froid de la VAD, dans un processus neuf :
    /// le premier appel puis le second sur le meme clip, puis (avec
    /// `PARLATO_WARM=1`) la chauffe (chargement + 0,5 s de silence) avant le
    /// premier appel reel.
    /// `PARLATO_VAD_MODEL=<ggml-silero.bin> PARLATO_WAV=<16 kHz wav>
    /// [PARLATO_WARM=1] cargo test --release --lib vad_cold_start_smoke --
    /// --ignored --nocapture`.
    #[test]
    #[ignore]
    fn vad_cold_start_smoke() {
        let model = std::env::var("PARLATO_VAD_MODEL").expect("PARLATO_VAD_MODEL");
        let wav = std::env::var("PARLATO_WAV").expect("PARLATO_WAV");
        let samples = speech::read_wav_as_f32(Path::new(&wav)).unwrap();
        let (dir, _keep) = vad_dir_for(Path::new(&model));
        let engine = VadEngine::default();
        if std::env::var("PARLATO_WARM").is_ok() {
            println!(
                "warm-up (load + 0.5 s of silence): {} ms",
                warm_up(&engine, &dir, None).unwrap().as_millis()
            );
        }
        for n in 1..=2 {
            let st = Instant::now();
            let spans = engine.ranges(&dir, None, &samples).unwrap().len();
            println!(
                "call {n} on {:.1} s of audio: {} ms, {spans} spans",
                samples.len() as f32 / 16000.0,
                st.elapsed().as_millis()
            );
        }
    }
}
