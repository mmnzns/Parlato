// VAD (Voice Activity Detection) Silero, par `voice_engine_stt::vad::detect`
// (voice-engine v0.2.2, modele "vad" du catalogue).
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
// avec la marge de 30 ms de VoiceInk de chaque cote. Le moteur charge le
// modele (moins de 1 Mo) a chaque appel et ne garde rien : `VadEngine` ne
// garde que le verrou du dossier partage, comme avant, jusqu'au dechargement.

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use tracing::{debug, info, warn};
use voice_engine_models::ModelHandle;

use super::speech;

const SAMPLE_RATE: u64 = 16_000;
/// Marge de VoiceInk (`speech_pad_ms`) de chaque cote d'une plage.
const SPEECH_PAD_MS: u64 = 30;

#[derive(Default)]
pub struct VadEngine {
    /// Le dossier du modele utilise en dernier (pour `unload_if_under`).
    loaded_dir: Mutex<Option<PathBuf>>,
    /// Parlato : verrou du dossier de modeles partage tant que le modele est
    /// "charge" (jusqu'au dechargement).
    handle: Mutex<Option<ModelHandle>>,
}

impl VadEngine {
    /// Oublie le modele et son verrou. Appele apres chaque pipeline.
    pub fn unload(&self) {
        let had = self.loaded_dir.lock().take().is_some();
        *self.handle.lock() = None;
        if had {
            info!("Modele VAD decharge");
        }
    }

    /// Garde `handle` tant que le modele est charge (remplace le precedent).
    pub fn hold(&self, handle: Option<ModelHandle>) {
        *self.handle.lock() = handle;
    }

    /// Decharge le modele s'il se trouve dans `dir`. Avant un Delete.
    pub fn unload_if_under(&self, dir: &Path) {
        let here = self
            .loaded_dir
            .lock()
            .as_ref()
            .is_some_and(|l| l.starts_with(dir));
        if here {
            self.unload();
        }
    }

    /// Les plages d'echantillons (debut, fin) de parole de `samples`, marge de
    /// 30 ms comprise. `model_dir` : le dossier du modele "vad".
    pub fn ranges(&self, model_dir: &Path, samples: &[f32]) -> Result<Vec<(usize, usize)>> {
        let st = Instant::now();
        let spans =
            voice_engine_stt::vad::detect(model_dir, samples).map_err(|e| anyhow!("VAD: {e}"))?;
        *self.loaded_dir.lock() = Some(model_dir.to_path_buf());
        let ranges = spans_to_sample_ranges(&spans, samples.len());
        debug!(
            duration_ms = st.elapsed().as_millis() as u64,
            ranges = ranges.len(),
            "VAD segmentation terminee"
        );
        Ok(ranges)
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

/// Applique la VAD sur un WAV 16 kHz mono et retourne les plages de samples
/// a transcrire (marge de 30 ms comprise).
pub fn run_vad_on_wav(engine: &VadEngine, model_dir: &Path, wav_path: &Path) -> Result<VadOutput> {
    let samples = speech::read_wav_as_f32(wav_path)?;
    let ranges = engine.ranges(model_dir, &samples)?;
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
    fn empty_and_backwards_spans_are_dropped() {
        assert!(spans_to_sample_ranges(&[], 16_000).is_empty());
        assert!(spans_to_sample_ranges(&[(5000, 5000)], 16_000).is_empty());
        assert!(spans_to_sample_ranges(&[(5000, 100)], 16_000).is_empty());
        // Entirely past the end of the audio.
        assert!(spans_to_sample_ranges(&[(9000, 9500)], 16_000).is_empty());
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

    /// Parlato : temps de la VAD du moteur (4 threads fixes, regle du moteur),
    /// nombre de plages seulement :
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
        let mut best = u128::MAX;
        let mut n = 0;
        for _ in 0..3 {
            let st = Instant::now();
            n = voice_engine_stt::vad::detect(&dir, &samples).unwrap().len();
            best = best.min(st.elapsed().as_millis());
        }
        println!("engine vad (threads fixed by the engine): best of 3 = {best} ms, {n} spans");
        assert!(n > 0);
        let engine = VadEngine::default();
        let ranges = engine.ranges(&dir, &samples).unwrap();
        println!("sample ranges after the 30 ms margin: {}", ranges.len());
        assert!(!ranges.is_empty());
    }
}
