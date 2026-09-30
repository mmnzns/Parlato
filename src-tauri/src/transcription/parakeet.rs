// Parlato : gere aussi `parakeet_rs::ParakeetUnified` (Parakeet Unified EN),
// selon le ParakeetKind de la variante.
//
// Wrapper autour de `parakeet_rs::ParakeetTDT` pour reproduire l'API
// publique de WhisperEngine (load paresseux + transcribe_samples).
//
// Reference VoiceInk : FluidAudio expose `AsrManager.transcribe(samples,
// decoderState)`. Ici on s'appuie sur parakeet-rs qui encapsule deja le
// chargement ONNX, le mel spectrogram, l'encodeur, le joint net et le
// decoder TDT. On expose juste un handle avec rechargement paresseux si
// le repertoire change.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use parakeet_rs::{ParakeetTDT, ParakeetUnified, Transcriber};

use super::parakeet_model_manager::ParakeetKind;

/// Parlato : une famille de modele = un type parakeet-rs.
enum Engine {
    Tdt(ParakeetTDT),
    Unified(ParakeetUnified),
}

struct Loaded {
    path: PathBuf,
    engine: Engine,
}

pub struct ParakeetEngine {
    current: Mutex<Option<Loaded>>,
}

impl ParakeetEngine {
    pub fn new() -> Self {
        Self {
            current: Mutex::new(None),
        }
    }

    /// Charge le modele (repertoire contenant config.json + *.onnx + vocab.txt)
    /// si ce n'est pas deja celui-ci qui est charge. Bloquant.
    pub fn ensure_loaded(&self, model_dir: &Path, kind: ParakeetKind) -> Result<()> {
        let mut guard = self.current.lock();
        if let Some(cur) = guard.as_ref() {
            if cur.path == model_dir {
                return Ok(());
            }
        }
        // Libere l'ancien avant de charger le nouveau.
        *guard = None;

        // Selection de l'execution provider ONNX. parakeet-rs prend un
        // `Option<ExecutionConfig>` ; `None` retombe sur `ExecutionProvider::Cpu`
        // (le `#[default]` de l'enum). Il faut donc lui passer explicitement le
        // provider GPU, sinon compiler avec `cuda-onnx` / `directml-onnx` ne
        // fait qu'exposer le variant sans jamais l'utiliser (inference CPU).
        // Chaque provider GPU retombe automatiquement sur CPU s'il echoue a
        // s'initialiser (parakeet-rs enchaine [GPU, CPU.error_on_failure()]).
        #[cfg(feature = "cuda-onnx")]
        let cfg = Some(
            parakeet_rs::ExecutionConfig::new()
                .with_execution_provider(parakeet_rs::ExecutionProvider::Cuda),
        );
        #[cfg(all(not(feature = "cuda-onnx"), feature = "directml-onnx"))]
        let cfg = Some(
            parakeet_rs::ExecutionConfig::new()
                .with_execution_provider(parakeet_rs::ExecutionProvider::DirectML),
        );
        #[cfg(all(not(feature = "cuda-onnx"), not(feature = "directml-onnx")))]
        let cfg: Option<parakeet_rs::ExecutionConfig> = None;

        let engine = match kind {
            ParakeetKind::Tdt => Engine::Tdt(
                ParakeetTDT::from_pretrained(model_dir, cfg)
                    .map_err(|e| anyhow!("parakeet load: {e:?}"))?,
            ),
            ParakeetKind::Unified => Engine::Unified(
                ParakeetUnified::from_pretrained(model_dir, cfg)
                    .map_err(|e| anyhow!("parakeet unified load: {e:?}"))?,
            ),
        };
        *guard = Some(Loaded {
            path: model_dir.to_path_buf(),
            engine,
        });
        Ok(())
    }

    /// Transcrit un buffer PCM Float32 mono 16 kHz. Bloquant (inference).
    /// `language` est accepte mais non-utilise : parakeet TDT v2 est anglais
    /// uniquement, v3 detecte la langue automatiquement.
    pub fn transcribe_samples(&self, samples: &[f32], _language: Option<&str>) -> Result<String> {
        let mut guard = self.current.lock();
        let loaded = guard
            .as_mut()
            .ok_or_else(|| anyhow!("modele Parakeet non charge"))?;
        // Pas de timestamps pour l'usage dictee : on veut juste le texte.
        // parakeet-rs attend un Vec<f32>. Une copie est inevitable ici.
        let result = match &mut loaded.engine {
            Engine::Tdt(e) => e.transcribe_samples(samples.to_vec(), 16000, 1, None),
            Engine::Unified(e) => e.transcribe_samples(samples.to_vec(), 16000, 1, None),
        }
        .map_err(|e| anyhow!("parakeet transcribe: {e:?}"))?;
        Ok(result.text)
    }

    /// Libere la memoire (ONNX session). Utile quand l'utilisateur change de
    /// modele ou desactive la source.
    pub fn unload(&self) {
        *self.current.lock() = None;
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

    /// Smoke test against a real model directory :
    /// `PARLATO_PARAKEET_DIR=<dir> PARLATO_PARAKEET_KIND=unified|tdt
    /// PARLATO_WAV=<16 kHz wav> cargo test --lib parakeet_smoke -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn parakeet_smoke() {
        let dir = std::env::var("PARLATO_PARAKEET_DIR").expect("PARLATO_PARAKEET_DIR");
        let wav = std::env::var("PARLATO_WAV").expect("PARLATO_WAV");
        let kind = match std::env::var("PARLATO_PARAKEET_KIND").as_deref() {
            Ok("unified") => ParakeetKind::Unified,
            _ => ParakeetKind::Tdt,
        };
        let engine = ParakeetEngine::new();
        let t0 = std::time::Instant::now();
        engine.ensure_loaded(Path::new(&dir), kind).unwrap();
        let loaded_in = t0.elapsed().as_secs_f32();
        let samples = crate::transcription::whisper::read_wav_as_f32(Path::new(&wav)).unwrap();
        let t1 = std::time::Instant::now();
        let text = engine.transcribe_samples(&samples, None).unwrap();
        println!(
            "--- load {loaded_in:.1}s, {:.1}s audio in {:.1}s ---\n{text}\n---",
            samples.len() as f32 / 16000.0,
            t1.elapsed().as_secs_f32()
        );
        assert!(!text.trim().is_empty());
    }
}
