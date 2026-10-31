// Whisper local, sur `voice-engine-stt` (voice-engine v0.2.2) : Metal sur le
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
use voice_engine_stt::{ChunkOptions, Timing};

use super::speech::{self, gpu_request, Source, SpeechSlot};

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
        self.slot.ensure_loaded(
            source,
            threads,
            gpu_request(voice_engine_models::Kind::Whisper, false),
            handle,
        )
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
            let texts: Vec<&str> = transcript
                .segments
                .iter()
                .map(|s| s.text.as_str())
                .collect();
            Ok(join_segments(&texts))
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
    pub fn unload_if_under(&self, path: &Path) {
        if self.slot.unload_if_under(path) {
            info!("Modele Whisper decharge");
        }
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

/// Les segments de Whisper (deja nettoyes par le moteur) en un seul texte :
/// un espace entre deux segments, sauf entre deux caracteres d'une ecriture
/// qui s'ecrit sans espaces (chinois, japonais).
pub fn join_segments(segments: &[&str]) -> String {
    let mut out = String::new();
    for text in segments.iter().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let (Some(last), Some(first)) = (out.chars().last(), text.chars().next()) {
            if !(unspaced_script(last) || unspaced_script(first)) {
                out.push(' ');
            }
        }
        out.push_str(text);
    }
    out
}

/// Ecritures sans espace entre les mots : han, hiragana, katakana et la
/// ponctuation pleine chasse qui va avec.
fn unspaced_script(c: char) -> bool {
    matches!(c,
        '\u{3000}'..='\u{303F}'   // ponctuation CJK
        | '\u{3040}'..='\u{30FF}' // hiragana, katakana
        | '\u{3400}'..='\u{4DBF}' // han (extension A)
        | '\u{4E00}'..='\u{9FFF}' // han
        | '\u{FF00}'..='\u{FFEF}' // pleine chasse
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use voice_engine_stt::whisper_engine::WhisperEngine as EngineWhisper;
    use voice_engine_stt::LoadOptions;

    #[test]
    fn segments_are_joined_with_single_spaces() {
        assert_eq!(join_segments(&[]), "");
        assert_eq!(join_segments(&["Hello there."]), "Hello there.");
        assert_eq!(
            join_segments(&["Hello there.", "  How are you? ", "", "Fine."]),
            "Hello there. How are you? Fine."
        );
    }

    #[test]
    fn chinese_and_japanese_segments_are_not_spaced() {
        assert_eq!(join_segments(&["你好。", "再见。"]), "你好。再见。");
        assert_eq!(join_segments(&["今日は", "晴れです"]), "今日は晴れです");
        // A Latin word next to Han keeps no extra space either side of the script change.
        assert_eq!(join_segments(&["ok", "你好"]), "ok你好");
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
                backend.fell_back
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
