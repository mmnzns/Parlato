// Whisper local, sur `voice-engine-stt` (voice-engine v0.3.2) : Metal sur le
// Mac, le processeur sous Windows (pas de Vulkan dans ce processus).
//
// Reference VoiceInk : VoiceInk/Transcription/Core/Whisper/LibWhisper.swift
// Reglages de decodage : ceux de `Timing::Segments` du moteur (glouton, pas de
// contexte entre deux appels, blancs et sons non verbaux supprimes), langue
// "auto" quand aucune n'est donnee. La VAD Silero est appliquee en amont
// (cf vad.rs) ; le texte est le dernier etage, les etapes suivantes (filtre,
// remplacement de mots, IA) sont dans le pipeline, comme VoiceInk.

use std::path::Path;

use anyhow::{Context, Result};
use tracing::info;
use voice_engine_models::ModelHandle;
use voice_engine_stt::{join_segments, ChunkOptions, Timing};

use super::speech::{self, Source, SpeechSlot};

/// Whisper charge : reutilisable entre transcriptions, thread-safe.
#[derive(Default)]
pub struct WhisperEngine {
    slot: SpeechSlot,
}

impl WhisperEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Charge (ou recharge) le modele si necessaire. `threads` : `None` = la
    /// regle du moteur. `handle` : le verrou du dossier partage, garde tant
    /// que le modele est charge (vide pour un modele importe).
    pub fn ensure_loaded(
        &self,
        source: &Source,
        threads: Option<usize>,
        handle: Option<ModelHandle>,
    ) -> Result<()> {
        // La regle commune (`use_gpu`) : Metal sur le Mac si le reglage est
        // actif, le processeur ailleurs.
        let gpu = crate::use_gpu::wanted_now(crate::use_gpu::Runtime::Whisper);
        self.slot.ensure_loaded(source, threads, gpu, handle)
    }

    /// Transcrit un buffer Float32 mono 16 kHz deja decode. `language` : le
    /// code de la langue ("fr"), `None` detecte. `vocabulary` : mots a
    /// favoriser (prompt initial de Whisper), vide si aucun.
    pub fn transcribe_samples(
        &self,
        samples: &[f32],
        language: Option<&str>,
        vocabulary: &[String],
    ) -> Result<String> {
        let opts = ChunkOptions {
            language: language.map(str::to_string),
            timing: Timing::Segments,
            vocabulary: vocabulary.to_vec(),
            ..ChunkOptions::default()
        };
        self.slot.run(|engine, _| {
            let transcript = engine
                .transcribe(samples, &opts)
                .map_err(|e| anyhow::anyhow!(e.to_string()))?;
            Ok(join_segments(&transcript.segments))
        })
    }

    /// Transcrit un fichier WAV 16 kHz mono.
    pub fn transcribe_wav(
        &self,
        wav_path: &Path,
        language: Option<&str>,
        vocabulary: &[String],
    ) -> Result<String> {
        let samples = speech::read_wav_as_f32(wav_path)
            .with_context(|| format!("lecture WAV {}", wav_path.display()))?;
        self.transcribe_samples(&samples, language, vocabulary)
    }

    /// Libere le modele si un modele est charge : la RAM redescend de
    /// l'equivalent du .bin (150 Mo pour base jusqu'a 3 Go pour large-v3).
    /// Utilise apres chaque pipeline pour ne pas garder le modele resident
    /// entre deux dictees (parite VoiceInk WhisperModelManager.unloadModel).
    pub fn unload(&self) {
        if self.slot.unload() {
            info!("Modele Whisper decharge");
        }
    }

    /// Decharge le modele s'il se trouve dans (ou est) `path`. Avant un Delete.
    /// Vrai si un modele a ete decharge.
    pub fn unload_if_under(&self, path: &Path) -> bool {
        let unloaded = self.slot.unload_if_under(path);
        if unloaded {
            info!("Modele Whisper decharge");
        }
        unloaded
    }

    /// La carte graphique a-t-elle echoue depuis le lancement ?
    pub fn gpu_failed(&self) -> bool {
        self.slot.gpu_failed()
    }

    /// La personne a rallume la carte graphique : oublie l'echec.
    pub fn retry_gpu(&self) {
        self.slot.retry_gpu();
    }

    /// Vrai une seule fois apres un repli carte graphique -> processeur.
    pub fn take_gpu_notice(&self) -> bool {
        self.slot.take_gpu_notice()
    }
}

/// Le prompt libre de la commande `transcribe_wav` en liste de termes pour
/// `vocabulary` : coupe aux virgules, points-virgules et retours a la ligne,
/// espaces retires, vides jetes. Un prompt sans separateur reste un seul terme.
pub fn vocabulary_from_prompt(prompt: &str) -> Vec<String> {
    prompt
        .split([',', ';', '\n', '\r'])
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use voice_engine_stt::whisper_engine::WhisperEngine as EngineWhisper;
    use voice_engine_stt::{LoadOptions, Segment};

    fn segment(text: &str, starts_word: bool) -> Segment {
        Segment {
            start_ms: 0,
            end_ms: 0,
            text: text.into(),
            words: vec![],
            starts_word,
        }
    }

    #[test]
    fn segments_are_joined_by_the_engine_with_single_spaces() {
        assert_eq!(join_segments(&[]), "");
        assert_eq!(
            join_segments(&[segment("Hello there.", true)]),
            "Hello there."
        );
        assert_eq!(
            join_segments(&[
                segment("Hello there.", true),
                segment("How are you?", true),
                segment("", true),
                segment("Fine.", true),
            ]),
            "Hello there. How are you? Fine."
        );
    }

    #[test]
    fn segments_that_do_not_start_a_word_are_not_spaced() {
        // Whisper in Chinese or Japanese: the raw segment text had no leading space.
        assert_eq!(
            join_segments(&[segment("你好。", true), segment("再见。", false)]),
            "你好。再见。"
        );
    }

    #[test]
    fn prompt_becomes_a_vocabulary_list() {
        assert_eq!(
            vocabulary_from_prompt("Parlato, Mogo; Talaan\nEnsayo"),
            vec!["Parlato", "Mogo", "Talaan", "Ensayo"]
        );
        assert_eq!(vocabulary_from_prompt("  ,, ; "), Vec::<String>::new());
        assert_eq!(vocabulary_from_prompt(""), Vec::<String>::new());
        assert_eq!(
            vocabulary_from_prompt("a medical report"),
            vec!["a medical report"]
        );
    }

    /// Parlato : temps de Whisper selon le nombre de threads, sur les 6
    /// premieres secondes (dictee courte) puis sur tout le fichier. Ne
    /// montre que des durees et des nombres, jamais le texte.
    /// `PARLATO_WHISPER_MODEL=<ggml-*.bin> PARLATO_WAV=<16 kHz wav>
    /// cargo test --release --lib whisper_threads_smoke -- --ignored --nocapture`.
    /// Options : `PARLATO_THREADS=8,12,16`, `PARLATO_SHORT_ONLY=1` pour les
    /// gros modeles et `PARLATO_GPU=1` pour Metal.
    #[test]
    #[ignore]
    fn whisper_threads_smoke() {
        let model = std::env::var("PARLATO_WHISPER_MODEL").expect("PARLATO_WHISPER_MODEL");
        let wav = std::env::var("PARLATO_WAV").expect("PARLATO_WAV");
        let all = speech::read_wav_as_f32(Path::new(&wav)).unwrap();
        let gpu = std::env::var("PARLATO_GPU").is_ok();
        let threads_list: Vec<usize> = std::env::var("PARLATO_THREADS")
            .map(|v| v.split(',').filter_map(|n| n.trim().parse().ok()).collect())
            .unwrap_or_else(|_| vec![2, 4, 6, 8, 12, 16]);
        let short = &all[..all.len().min(6 * 16_000)];
        let clips: Vec<&[f32]> = if std::env::var("PARLATO_SHORT_ONLY").is_ok() {
            vec![short]
        } else {
            vec![short, &all[..]]
        };
        for &threads in &threads_list {
            let t0 = std::time::Instant::now();
            let mut engine = EngineWhisper::load(
                Path::new(&model),
                LoadOptions {
                    gpu,
                    threads: Some(threads),
                },
            )
            .unwrap();
            let backend = voice_engine_stt::Transcriber::backend(&engine);
            println!(
                "threads={threads:>2}: load {} ms, backend {}, fell_back {}",
                t0.elapsed().as_millis(),
                backend.backend,
                backend.fell_back()
            );
            let opts = ChunkOptions {
                language: Some("en".into()),
                timing: Timing::Segments,
                ..ChunkOptions::default()
            };
            for samples in &clips {
                let mut best = u128::MAX;
                let mut words = 0;
                for _ in 0..2 {
                    let st = std::time::Instant::now();
                    let t = voice_engine_stt::Transcriber::transcribe(&mut engine, samples, &opts)
                        .unwrap();
                    best = best.min(st.elapsed().as_millis());
                    words = t
                        .segments
                        .iter()
                        .map(|s| s.text.split_whitespace().count())
                        .sum();
                    assert!(words > 0);
                }
                println!(
                    "threads={threads:>2} audio={:>4.1}s: best of 2 = {best} ms, {words} words",
                    samples.len() as f32 / 16000.0
                );
            }
        }
    }
}
