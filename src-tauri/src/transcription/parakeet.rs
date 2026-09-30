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

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Result};
use parking_lot::Mutex;
use parakeet_rs::{Nemotron, ParakeetTDT, ParakeetUnified, Transcriber};

use super::parakeet_model_manager::ParakeetKind;

/// Parlato : une famille de modele = un type parakeet-rs.
enum Engine {
    Tdt(ParakeetTDT),
    Unified(ParakeetUnified),
    Nemotron(Nemotron),
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
            ParakeetKind::Nemotron => Engine::Nemotron(
                Nemotron::from_pretrained(model_dir, cfg)
                    .map_err(|e| anyhow!("nemotron load: {e:?}"))?,
            ),
        };
        *guard = Some(Loaded {
            path: model_dir.to_path_buf(),
            engine,
        });
        Ok(())
    }

    /// Transcrit un buffer PCM Float32 mono 16 kHz. Bloquant (inference).
    /// `language` n'est utilise que par Nemotron : parakeet TDT v2 et Unified
    /// sont anglais uniquement, v3 detecte la langue automatiquement.
    pub fn transcribe_samples(&self, samples: &[f32], language: Option<&str>) -> Result<String> {
        let mut guard = self.current.lock();
        let loaded = guard
            .as_mut()
            .ok_or_else(|| anyhow!("modele Parakeet non charge"))?;
        // Pas de timestamps pour l'usage dictee : on veut juste le texte.
        // parakeet-rs attend un Vec<f32>. Une copie est inevitable ici.
        let result = match &mut loaded.engine {
            Engine::Tdt(e) => e.transcribe_samples(samples.to_vec(), 16000, 1, None),
            Engine::Unified(e) => e.transcribe_samples(samples.to_vec(), 16000, 1, None),
            Engine::Nemotron(e) => return transcribe_nemotron(e, samples, language),
        }
        .map_err(|e| anyhow!("parakeet transcribe: {e:?}"))?;
        Ok(result.text)
    }

    /// Parlato : apercu en direct (Parakeet Unified seulement). Remet l'etat
    /// de flux a zero ; sans effet si le modele charge n'est pas Unified.
    pub fn stream_reset(&self) {
        if let Some(Loaded {
            engine: Engine::Unified(e),
            ..
        }) = self.current.lock().as_mut()
        {
            e.reset();
        }
    }

    /// Parlato : ajoute un morceau d'audio (f32 mono 16 kHz) au flux et
    /// renvoie la transcription cumulee. None si le modele charge n'est pas
    /// Unified (ou plus charge). Le passage final reste `transcribe_samples`,
    /// qui remet lui-meme l'etat de flux a zero.
    pub fn stream_chunk(&self, samples: &[f32]) -> Option<Result<String>> {
        let mut guard = self.current.lock();
        let Some(Loaded {
            engine: Engine::Unified(e),
            ..
        }) = guard.as_mut()
        else {
            return None;
        };
        Some(
            e.transcribe_chunk(samples)
                .map(|_| e.get_transcript())
                .map_err(|err| anyhow!("parakeet unified stream: {err:?}")),
        )
    }

    /// Libere la memoire (ONNX session). Utile quand l'utilisateur change de
    /// modele ou desactive la source.
    pub fn unload(&self) {
        *self.current.lock() = None;
    }
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
        engine.ensure_loaded(Path::new(&dir), kind).unwrap();
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

    /// Parlato : apercu en direct Unified. Donne le WAV par morceaux de
    /// 200 ms comme pendant une dictee et verifie que le flux tient le temps
    /// reel : `PARLATO_PARAKEET_DIR=<unified dir> PARLATO_WAV=<wav>
    /// cargo test --release --lib unified_stream_smoke -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn unified_stream_smoke() {
        let dir = std::env::var("PARLATO_PARAKEET_DIR").expect("PARLATO_PARAKEET_DIR");
        let wav = std::env::var("PARLATO_WAV").expect("PARLATO_WAV");
        let engine = ParakeetEngine::new();
        engine
            .ensure_loaded(Path::new(&dir), ParakeetKind::Unified)
            .unwrap();
        let samples = crate::transcription::whisper::read_wav_as_f32(Path::new(&wav)).unwrap();
        engine.stream_reset();
        let t0 = std::time::Instant::now();
        let mut last = String::new();
        let mut slowest = 0f32;
        for (i, chunk) in samples.chunks(3200).enumerate() {
            let t = std::time::Instant::now();
            let text = engine.stream_chunk(chunk).unwrap().unwrap();
            slowest = slowest.max(t.elapsed().as_secs_f32());
            if text != last {
                println!("[{:5.1}s audio] {text}", (i + 1) as f32 * 0.2);
                last = text;
            }
        }
        let stream_secs = t0.elapsed().as_secs_f32();
        let audio_secs = samples.len() as f32 / 16000.0;
        let offline = engine.transcribe_samples(&samples, None).unwrap();
        println!(
            "--- {audio_secs:.1}s audio streamed in {stream_secs:.1}s (slowest call {slowest:.2}s)\n\
             offline: {offline}"
        );
        assert!(!last.is_empty());
        assert!(
            stream_secs < audio_secs,
            "le flux ne tient pas le temps reel"
        );
    }
}
