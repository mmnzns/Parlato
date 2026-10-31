// VAD (Voice Activity Detection) Silero via whisper-rs.
//
// Reference VoiceInk :
// - Transcription/Core/Whisper/VADModelManager.swift : bundle du fichier
//   ggml-silero-v5.1.2.bin dans l'app.
// - Transcription/Core/Whisper/LibWhisper.swift L73-85 : activation via
//   UserDefault IsVADEnabled + params threshold 0.50, min_speech 250 ms,
//   min_silence 100 ms, max_speech +inf, speech_pad 30 ms, samples_overlap 0.1.
//
// Sur whisper-rs 0.16 la VAD est un contexte SEPARE (WhisperVadContext) qui
// retourne des WhisperVadSegment. On segmente d'abord puis on transcrit chaque
// segment, identique a l'effet du vad_model_path passe a FullParams cote mac.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use tracing::{debug, info, warn};
use voice_engine_models::ModelHandle;
use whisper_rs::{WhisperVadContext, WhisperVadContextParams, WhisperVadParams, WhisperVadSegment};

/// Contexte VAD charge (reutilisable entre transcriptions).
pub struct VadEngine {
    inner: Arc<Mutex<Option<WhisperVadContext>>>,
    loaded_path: Arc<Mutex<Option<PathBuf>>>,
    /// Parlato : verrou du dossier de modeles partage tant que le modele est charge.
    handle: Arc<Mutex<Option<ModelHandle>>>,
}

impl Default for VadEngine {
    fn default() -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
            loaded_path: Arc::new(Mutex::new(None)),
            handle: Arc::new(Mutex::new(None)),
        }
    }
}

impl VadEngine {
    /// Libere le contexte VAD (Silero ONNX charge via whisper-rs). ~20 Mo
    /// resident habituellement. Appele apres chaque pipeline pour reclaim.
    pub fn unload(&self) {
        let had_model = self.inner.lock().take().is_some();
        *self.loaded_path.lock() = None;
        *self.handle.lock() = None;
        if had_model {
            info!("Modele VAD decharge");
        }
    }

    /// Garde `handle` tant que le modele est charge (remplace le precedent).
    pub fn hold(&self, handle: Option<ModelHandle>) {
        *self.handle.lock() = handle;
    }

    /// Decharge le modele s'il se trouve dans `dir`. Avant un Delete.
    pub fn unload_if_under(&self, dir: &Path) {
        let loaded = self.loaded_path.lock().clone();
        if loaded.is_some_and(|l| l.starts_with(dir)) {
            self.unload();
        }
    }

    pub fn load(&self, model_path: &Path) -> Result<()> {
        {
            let loaded = self.loaded_path.lock();
            if loaded.as_deref() == Some(model_path) {
                return Ok(());
            }
        }
        info!(path = %model_path.display(), "Chargement du modele VAD Silero");
        let mut params = WhisperVadContextParams::default();
        params.set_n_threads(vad_threads());
        params.set_use_gpu(false);
        let path_str = model_path
            .to_str()
            .ok_or_else(|| anyhow!("chemin VAD non-UTF8"))?;
        let ctx = WhisperVadContext::new(path_str, params)
            .map_err(|e| anyhow!("WhisperVadContext: {e}"))?;

        *self.inner.lock() = Some(ctx);
        *self.loaded_path.lock() = Some(model_path.to_path_buf());
        Ok(())
    }

    /// Decoupe un buffer mono f32 16 kHz en segments de parole.
    pub fn segments(&self, samples: &[f32]) -> Result<Vec<WhisperVadSegment>> {
        let mut guard = self.inner.lock();
        let ctx = guard
            .as_mut()
            .ok_or_else(|| anyhow!("VAD non initialise"))?;
        let params = default_vad_params();
        let st = Instant::now();
        let iter = ctx
            .segments_from_samples(params, samples)
            .map_err(|e| anyhow!("VAD segments_from_samples: {e}"))?;
        // WhisperVadSegments est un iterateur qui yield des WhisperVadSegment.
        let segs: Vec<WhisperVadSegment> = iter.collect();
        debug!(
            duration_ms = st.elapsed().as_millis() as u64,
            segments = segs.len(),
            "VAD segmentation terminee"
        );
        Ok(segs)
    }
}

/// Parametres repliquant exactement VoiceInk LibWhisper.swift L78-85.
/// Parlato : verifie le 2026-10-16, ce sont aussi les valeurs par defaut de
/// whisper.cpp ; on les pose quand meme pour qu'une mise a jour du binding
/// ne les change pas sans qu'on le voie.
fn default_vad_params() -> WhisperVadParams {
    let mut params = WhisperVadParams::new();
    params.set_threshold(0.5);
    params.set_min_speech_duration(250);
    params.set_min_silence_duration(100);
    params.set_max_speech_duration(f32::MAX);
    params.set_speech_pad(30);
    params.set_samples_overlap(0.1);
    params
}

/// Parlato : Silero traite l'audio par fenetres de 32 ms, des graphes minuscules.
/// Avec tous les threads (16 sur un Ryzen 8 coeurs), la synchronisation coute
/// bien plus que le calcul : 32,8 s au lieu de 0,5 s pour 49 s d'audio
/// (`vad_threads_smoke`, 2026-10-05), et parfois 50 s quand le PC est occupe.
/// 2 threads est le plus rapide mesure.
fn vad_threads() -> i32 {
    num_cpus_physical().min(2)
}

fn num_cpus_physical() -> i32 {
    std::thread::available_parallelism()
        .map(|n| n.get() as i32)
        .unwrap_or(4)
}

/// Applique la VAD sur un WAV 16 kHz mono et retourne les plages de samples
/// (start, end) a transcrire. Ces plages ont deja subi le padding de
/// `speech_pad_ms` en interne par whisper.cpp.
/// Echantillons du WAV + plages (start, end) a transcrire.
pub type VadOutput = (Vec<f32>, Vec<(usize, usize)>);

pub fn run_vad_on_wav(engine: &VadEngine, wav_path: &Path) -> Result<VadOutput> {
    let samples = super::whisper::read_wav_as_f32(wav_path)?;
    let segs = engine.segments(&samples)?;
    // Les segments de whisper.cpp sont en centisecondes cote start/end.
    let ranges: Vec<(usize, usize)> = segs
        .into_iter()
        .map(|WhisperVadSegment { start, end }| {
            let start_sample = ((start / 100.0) * 16_000.0) as usize;
            let end_sample = ((end / 100.0) * 16_000.0) as usize;
            (
                start_sample.min(samples.len()),
                end_sample.min(samples.len()),
            )
        })
        .filter(|(s, e)| e > s)
        .collect();
    if ranges.is_empty() {
        warn!("VAD n'a detecte aucune parole");
    }
    Ok((samples, ranges))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Parlato : temps de la VAD selon le nombre de threads.
    /// `PARLATO_VAD_MODEL=<ggml-silero.bin> PARLATO_WAV=<16 kHz wav>
    /// cargo test --release --lib vad_threads_smoke -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn vad_threads_smoke() {
        let model = std::env::var("PARLATO_VAD_MODEL").expect("PARLATO_VAD_MODEL");
        let wav = std::env::var("PARLATO_WAV").expect("PARLATO_WAV");
        let samples = crate::transcription::whisper::read_wav_as_f32(Path::new(&wav)).unwrap();
        println!("audio: {:.1} s", samples.len() as f32 / 16000.0);
        for threads in [1, 2, 4, num_cpus_physical()] {
            let mut params = WhisperVadContextParams::default();
            params.set_n_threads(threads);
            params.set_use_gpu(false);
            let mut ctx = WhisperVadContext::new(&model, params).unwrap();
            let mut best = u128::MAX;
            for _ in 0..3 {
                let st = Instant::now();
                let n = ctx
                    .segments_from_samples(default_vad_params(), &samples)
                    .unwrap()
                    .count();
                best = best.min(st.elapsed().as_millis());
                assert!(n > 0);
            }
            println!("threads={threads:>2}: best of 3 = {best} ms");
        }
    }
}
