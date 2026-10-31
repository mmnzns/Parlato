// Parakeet TDT, Parakeet Unified et Nemotron, sur `voice-engine-stt`
// (voice-engine v0.2.2). Le moteur prend un morceau d'au plus 40 s par appel
// (`SpeechError::TooLong` au-dela) ; Parlato garde donc son decoupage en
// fenetres (parakeet_windows.rs) pour les trois familles des que l'audio
// depasse 15 s, et chaque fenetre demande les mots horodates (`Timing::Words`).
//
// Reference VoiceInk : FluidAudio expose `AsrManager.transcribe(samples,
// decoderState)`. Ici le moteur partage encapsule le chargement ONNX, le
// spectrogramme, l'encodeur et le decodeur.
//
// Parlato : la carte graphique (DirectML sous Windows, Parakeet TDT
// seulement, reglage `use_gpu`) est lue a l'execution. Si le moteur dit
// qu'elle n'a pas servi (`fell_back`), le processeur sert jusqu'a la fermeture
// de l'application et la bulle le dit une fois (cf speech.rs).

use std::path::Path;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use tracing::info;
use voice_engine_models::{Kind, ModelHandle};
use voice_engine_stt::{ChunkOptions, Timing, Transcriber, Transcript};

use super::parakeet_windows::{transcribe_in_windows, TimedToken};
use super::speech::{Source, SpeechSlot};

#[derive(Default)]
pub struct ParakeetEngine {
    slot: SpeechSlot,
}

impl ParakeetEngine {
    /// Charge le modele (un dossier du dossier partage) s'il n'est pas deja
    /// charge. Bloquant. `threads` : `None` = la regle du moteur. La carte
    /// graphique suit la regle commune (`use_gpu`) : seul TDT la demande,
    /// DirectML sous Windows. `handle` : le verrou du dossier partage.
    pub fn ensure_loaded(
        &self,
        source: &Source,
        threads: Option<usize>,
        handle: Option<ModelHandle>,
    ) -> Result<()> {
        let gpu = crate::use_gpu::wanted_now(crate::use_gpu::runtime_for(source.kind()));
        self.slot.ensure_loaded(source, threads, gpu, handle)
    }

    /// Comme `ensure_loaded`, avec la carte demandee a la main (les tests de
    /// mesure).
    #[cfg(test)]
    pub(crate) fn ensure_loaded_with_gpu(
        &self,
        source: &Source,
        threads: Option<usize>,
        gpu: bool,
        handle: Option<ModelHandle>,
    ) -> Result<()> {
        self.slot.ensure_loaded(source, threads, gpu, handle)
    }

    /// Transcrit un buffer PCM Float32 mono 16 kHz. Bloquant (inference).
    /// `language` n'est utilise que par Nemotron (le moteur l'ignore pour les
    /// autres) : parakeet TDT v2 et Unified sont anglais uniquement, v3 detecte
    /// la langue seul.
    pub fn transcribe_samples(&self, samples: &[f32], language: Option<&str>) -> Result<String> {
        self.slot.run(|engine, kind| {
            let (text, windows) = transcribe_in_windows(samples, |slice| {
                decode_window(engine, kind, slice, language)
            })?;
            if windows > 1 {
                info!(
                    "parakeet: {:.1}s decoupees en {windows} fenetres",
                    samples.len() as f32 / 16000.0
                );
            }
            Ok(text)
        })
    }

    /// Libere la memoire (session ONNX et verrou du dossier). Utile quand
    /// l'utilisateur change de modele ou desactive la source.
    pub fn unload(&self) {
        self.slot.unload();
    }

    /// Decharge le modele s'il vient de `dir`. Avant un Delete.
    pub fn unload_if_under(&self, dir: &Path) {
        self.slot.unload_if_under(dir);
    }

    /// Ce qui a servi au dernier chargement : "cpu", "directml" ou
    /// "cpu-fallback" (carte demandee, le processeur a pris le relais).
    pub fn provider(&self) -> String {
        self.slot.provider()
    }

    /// La carte graphique a-t-elle echoue depuis le lancement de l'application ?
    pub fn gpu_failed(&self) -> bool {
        self.slot.gpu_failed()
    }

    /// Vrai une seule fois apres un repli carte graphique -> processeur : la
    /// bulle doit le dire a l'utilisateur.
    pub fn take_gpu_notice(&self) -> bool {
        self.slot.take_gpu_notice()
    }

    #[cfg(test)]
    pub fn slot(&self) -> &SpeechSlot {
        &self.slot
    }
}

#[derive(Default)]
pub struct ParakeetEngineState(pub Arc<ParakeetEngine>);

/// Une fenetre : ses mots horodates, en secondes sur l'horloge de la fenetre.
/// Nemotron : forcee sur une langue qui n'est pas celle parlee, il rend un
/// texte vide ; dans ce cas on refait un passage sans langue (detection), pour
/// qu'une phrase en anglais dictee en mode francais ne soit pas perdue.
fn decode_window(
    engine: &mut dyn Transcriber,
    kind: Kind,
    audio: &[f32],
    language: Option<&str>,
) -> Result<Vec<TimedToken>> {
    let words = window_words(engine, audio, language)?;
    let forced = language.is_some_and(|l| !l.is_empty() && l != "auto");
    if words.is_empty() && kind == Kind::Nemotron && forced {
        return window_words(engine, audio, None);
    }
    Ok(words)
}

fn window_words(
    engine: &mut dyn Transcriber,
    audio: &[f32],
    language: Option<&str>,
) -> Result<Vec<TimedToken>> {
    let opts = ChunkOptions {
        language: language.map(str::to_string),
        timing: Timing::Words,
        ..ChunkOptions::default()
    };
    let transcript = engine
        .transcribe(audio, &opts)
        .map_err(|e| anyhow!(e.to_string()))?;
    Ok(tokens_from_transcript(&transcript))
}

/// Les mots du moteur (texte, debut, fin en ms sur l'horloge de la fenetre)
/// en `TimedToken` : chaque mot ouvre un mot (espace de tete).
fn tokens_from_transcript(transcript: &Transcript) -> Vec<TimedToken> {
    transcript
        .segments
        .iter()
        .flat_map(|s| s.words.iter())
        .map(TimedToken::from_word)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use voice_engine_stt::{Segment, Word};

    fn word(text: &str, a: u64, b: u64) -> Word {
        Word {
            text: text.into(),
            start_ms: a,
            end_ms: b,
        }
    }

    #[test]
    fn engine_words_become_tokens_that_open_a_word() {
        let transcript = Transcript {
            segments: vec![
                Segment {
                    start_ms: 500,
                    end_ms: 900,
                    text: "Hello,".into(),
                    words: vec![word("Hello,", 500, 900)],
                },
                Segment {
                    start_ms: 1000,
                    end_ms: 1400,
                    text: "world.".into(),
                    words: vec![word("world.", 1000, 1400)],
                },
            ],
            language: None,
            stopped_early_at_ms: None,
        };
        let tokens = tokens_from_transcript(&transcript);
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[0].text, " Hello,");
        assert!((tokens[0].start - 0.5).abs() < 1e-6);
        assert!((tokens[1].end - 1.4).abs() < 1e-6);
        assert_eq!(
            crate::transcription::parakeet_windows::tokens_to_text(&tokens),
            "Hello, world."
        );
    }

    /// A scripted model: the first call for a forced language is empty.
    struct Scripted {
        calls: Vec<Option<String>>,
    }

    impl Transcriber for Scripted {
        fn transcribe(
            &mut self,
            _audio: &[f32],
            opts: &ChunkOptions,
        ) -> Result<Transcript, voice_engine_stt::SpeechError> {
            self.calls.push(opts.language.clone());
            assert_eq!(opts.timing, Timing::Words);
            let words = if opts.language.is_some() {
                vec![]
            } else {
                vec![word("hi", 0, 300)]
            };
            Ok(Transcript {
                segments: words
                    .iter()
                    .map(|w| Segment {
                        start_ms: w.start_ms,
                        end_ms: w.end_ms,
                        text: w.text.clone(),
                        words: vec![w.clone()],
                    })
                    .collect(),
                language: None,
                stopped_early_at_ms: None,
            })
        }

        fn backend(&self) -> voice_engine_stt::SpeechBackend {
            voice_engine_stt::SpeechBackend {
                backend: "cpu".into(),
                device: None,
                fell_back: false,
            }
        }
    }

    #[test]
    fn nemotron_retries_without_language_when_empty() {
        let mut m = Scripted { calls: vec![] };
        let t = decode_window(&mut m, Kind::Nemotron, &[0.0; 16_000], Some("fr")).unwrap();
        assert_eq!(t.len(), 1);
        assert_eq!(m.calls, vec![Some("fr".to_string()), None]);
    }

    #[test]
    fn other_models_do_not_retry() {
        let mut m = Scripted { calls: vec![] };
        let t = decode_window(&mut m, Kind::Parakeet, &[0.0; 16_000], Some("fr")).unwrap();
        assert!(t.is_empty());
        assert_eq!(m.calls.len(), 1);
        // Nemotron with no forced language is one call too.
        let mut m = Scripted { calls: vec![] };
        decode_window(&mut m, Kind::Nemotron, &[0.0; 16_000], Some("auto")).unwrap();
        assert_eq!(m.calls.len(), 1);
    }

    /// The catalog entry and folder for a smoke test:
    /// `PARLATO_PARAKEET_DIR=<dir> PARLATO_PARAKEET_KIND=unified|nemotron|tdt`,
    /// and `PARLATO_PARAKEET_ID=<catalog id>` for a TDT model other than
    /// parakeet-ultra (the folder holds the catalog's files for that id).
    fn smoke_source() -> Source {
        let dir =
            PathBuf::from(std::env::var("PARLATO_PARAKEET_DIR").expect("PARLATO_PARAKEET_DIR"));
        let id = std::env::var("PARLATO_PARAKEET_ID").unwrap_or_else(|_| {
            match std::env::var("PARLATO_PARAKEET_KIND").as_deref() {
                Ok("unified") => "parakeet-unified-en",
                Ok("nemotron") => "nemotron-3.5",
                _ => "parakeet-ultra",
            }
            .to_string()
        });
        let model = voice_engine_models::catalog::find(&id).expect("catalog id");
        Source::Catalog { model, dir }
    }

    fn words_in(text: &str) -> usize {
        text.split_whitespace().count()
    }

    /// Smoke test against a real model folder (counts and timings only, never
    /// the text): `PARLATO_PARAKEET_DIR=<dir> PARLATO_PARAKEET_KIND=unified|
    /// nemotron|tdt PARLATO_WAV=<16 kHz wav> cargo test --release --lib
    /// parakeet_smoke -- --ignored --nocapture`. Options : `PARLATO_GPU=1`,
    /// `PARLATO_LANG=fr`.
    #[test]
    #[ignore]
    fn parakeet_smoke() {
        let wav = std::env::var("PARLATO_WAV").expect("PARLATO_WAV");
        let source = smoke_source();
        let engine = ParakeetEngine::default();
        let t0 = std::time::Instant::now();
        engine
            .ensure_loaded_with_gpu(&source, None, std::env::var("PARLATO_GPU").is_ok(), None)
            .unwrap();
        let loaded_in = t0.elapsed().as_secs_f32();
        let samples = crate::transcription::speech::read_wav_as_f32(Path::new(&wav)).unwrap();
        let t1 = std::time::Instant::now();
        let lang = std::env::var("PARLATO_LANG").ok();
        let text = engine
            .transcribe_samples(&samples, lang.as_deref())
            .unwrap();
        let backend = engine.slot().backend().unwrap();
        println!(
            "--- load {loaded_in:.1}s, {:.1}s audio in {:.1}s, {} words, backend {}, fell_back {} ---",
            samples.len() as f32 / 16000.0,
            t1.elapsed().as_secs_f32(),
            words_in(&text),
            backend.backend,
            backend.fell_back
        );
        assert!(!text.trim().is_empty());
    }

    /// Fenetres sur un vrai enregistrement long (Parakeet TDT : v2, v3 ou
    /// Ultra). Une passe unique n'existe plus (le moteur refuse plus de 40 s) :
    /// on compte les fenetres, les mots et la duree, et si l'audio tient en
    /// un appel on le compare a la passe unique (nombre de mots seulement) :
    /// `PARLATO_PARAKEET_DIR=<modele tdt> PARLATO_WAV=<wav 16 kHz>
    /// cargo test --release --lib parakeet_long_audio -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn parakeet_long_audio() {
        use crate::transcription::parakeet_windows::{plan_windows, SAMPLE_RATE};
        let wav = std::env::var("PARLATO_WAV").expect("PARLATO_WAV");
        let samples = crate::transcription::speech::read_wav_as_f32(Path::new(&wav)).unwrap();
        let engine = ParakeetEngine::default();
        engine
            .ensure_loaded_with_gpu(&smoke_source(), None, false, None)
            .unwrap();
        let secs = samples.len() as f32 / SAMPLE_RATE as f32;
        if secs <= 40.0 {
            let t0 = std::time::Instant::now();
            let single = engine
                .slot()
                .run(|e, k| {
                    decode_window(e, k, &samples, None)
                        .map(|t| crate::transcription::parakeet_windows::tokens_to_text(&t))
                })
                .unwrap();
            println!(
                "=== un appel ({secs:.1}s audio, {} mots, {:.1}s)",
                words_in(&single),
                t0.elapsed().as_secs_f32()
            );
        }
        let t1 = std::time::Instant::now();
        let chunked = engine.transcribe_samples(&samples, None).unwrap();
        println!(
            "=== fenetres ({secs:.1}s audio, {} mots, {:.1}s)",
            words_in(&chunked),
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

    /// Parlato : Parakeet sur le processeur puis sur la carte graphique
    /// (DirectML sous Windows ; sur le Mac le moteur n'a pas de chemin carte
    /// pour ONNX et rapporte `fell_back`). Charge chaque variante, mesure un
    /// passage de chauffe puis trois passages, et compare les textes (egalite
    /// seulement) :
    /// `PARLATO_PARAKEET_DIR=<tdt dir> PARLATO_WAV=<wav 16 kHz>
    /// cargo test --release --lib parakeet_gpu_smoke -- --ignored --nocapture`.
    /// Option : `PARLATO_THREADS=12`.
    #[test]
    #[ignore]
    fn parakeet_gpu_smoke() {
        let wav = std::env::var("PARLATO_WAV").expect("PARLATO_WAV");
        let samples = crate::transcription::speech::read_wav_as_f32(Path::new(&wav)).unwrap();
        let threads: Option<usize> = std::env::var("PARLATO_THREADS")
            .ok()
            .and_then(|v| v.trim().parse().ok());
        let source = smoke_source();
        let secs = samples.len() as f32 / 16000.0;
        println!("audio {secs:.1}s, threads {threads:?}");

        let mut texts: Vec<(String, String)> = Vec::new();
        for (name, gpu) in [("cpu", false), ("gpu", true)] {
            // Direct engine load: this test asks for the card whatever the
            // app's policy says, to measure it.
            let t = std::time::Instant::now();
            let slot = SpeechSlot::default();
            if let Err(e) = slot.ensure_loaded(&source, threads, gpu, None) {
                println!("{name}: chargement impossible : {e}");
                continue;
            }
            let load_ms = t.elapsed().as_millis();
            let backend = slot.backend().unwrap();
            let mut times = Vec::new();
            let mut text = String::new();
            // 1 passage de chauffe puis 3 mesures.
            for _ in 0..4 {
                let st = std::time::Instant::now();
                let run = slot.run(|e, k| {
                    let (t, _) = transcribe_in_windows(&samples, |s| decode_window(e, k, s, None))?;
                    Ok(t)
                });
                match run {
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
                    "{name}: backend {}, fell_back {}, chargement {load_ms} ms, chauffe {} ms, passages {:?} ms, meilleur {best} ms ({:.1}x temps reel), {} mots",
                    backend.backend,
                    backend.fell_back,
                    times[0],
                    &times[1..],
                    secs * 1000.0 / *best as f32,
                    words_in(&text)
                );
                texts.push((name.to_string(), text));
            }
        }
        if let Some((_, reference)) = texts.first() {
            for (name, text) in &texts[1..] {
                println!("{name} identique a cpu : {}", text == reference);
            }
        }
    }

    /// Parlato : temps de Parakeet TDT selon le nombre de threads (la regle du
    /// moteur par defaut), sur 6 s puis tout le fichier :
    /// `PARLATO_PARAKEET_DIR=<tdt dir> PARLATO_WAV=<wav>
    /// cargo test --release --lib parakeet_threads_smoke -- --ignored --nocapture`.
    /// Option : `PARLATO_THREADS=4,6,9`.
    #[test]
    #[ignore]
    fn parakeet_threads_smoke() {
        let wav = std::env::var("PARLATO_WAV").expect("PARLATO_WAV");
        let all = crate::transcription::speech::read_wav_as_f32(Path::new(&wav)).unwrap();
        let short = &all[..all.len().min(6 * 16_000)];
        let threads_list: Vec<usize> = std::env::var("PARLATO_THREADS")
            .map(|v| v.split(',').filter_map(|n| n.trim().parse().ok()).collect())
            .unwrap_or_else(|_| vec![2, 4, 6, 8, 12, 16]);
        let source = smoke_source();
        for threads in threads_list {
            let engine = ParakeetEngine::default();
            engine
                .ensure_loaded_with_gpu(&source, Some(threads), false, None)
                .unwrap();
            for samples in [short, &all[..]] {
                let mut best = u128::MAX;
                let mut words = 0;
                for _ in 0..3 {
                    let st = std::time::Instant::now();
                    let text = engine.transcribe_samples(samples, None).unwrap();
                    best = best.min(st.elapsed().as_millis());
                    words = words_in(&text);
                    assert!(words > 0);
                }
                println!(
                    "threads={threads:>2} audio={:>4.1}s: best of 3 = {best} ms, {words} words",
                    samples.len() as f32 / 16000.0
                );
            }
        }
    }
}
