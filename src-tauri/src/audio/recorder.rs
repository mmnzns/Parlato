// Recorder WASAPI streaming vers WAV 16 kHz mono Int16 + callback chunks.
//
// Reference VoiceInk : VoiceInk/CoreAudioRecorder.swift
// - AUHAL + ExtAudioFile en mac, remplace par cpal + hound ici
// - Pipeline : callback device -> mono mixdown -> resample vers 16k -> Int16 clamp
//   -> ecriture WAV + emission chunk pour streaming providers
//
// Contrainte cpal : Stream est `!Send` sur certaines plateformes (WASAPI inclus).
// Le thread worker cree donc le Stream lui-meme, le garde sur sa pile, et lit les
// echantillons via un channel mpsc alimente par le callback du stream.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, StreamTrait};
use cpal::SampleFormat;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use super::device::{default_input_device, find_input_device_by_name};
use super::meters::compute_rms_peak;
use super::resampler::{float_to_int16, interleaved_to_mono, MonoResampler};
use super::{CHUNK_FRAMES, TARGET_CHANNELS, TARGET_SAMPLE_RATE};

/// Niveaux audio en dB publies vers l'UI.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct AudioMeter {
    pub rms_db: f32,
    pub peak_db: f32,
}

impl Default for AudioMeter {
    fn default() -> Self {
        Self {
            rms_db: -160.0,
            peak_db: -160.0,
        }
    }
}

/// Chunk audio Int16 mono 16 kHz expose au consommateur (streaming providers).
pub type AudioChunk = Vec<i16>;

/// Configuration d'un enregistrement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecorderConfig {
    /// Nom du peripherique. `None` = peripherique par defaut.
    pub device_name: Option<String>,
    /// Chemin de sortie du WAV (cree/ecrase au demarrage).
    pub output_path: PathBuf,
}

/// Callback pour reception des chunks audio en temps reel (streaming providers).
pub type ChunkCallback = Arc<dyn Fn(AudioChunk) + Send + Sync>;

/// Parlato : pourquoi un enregistrement s'est arrete tout seul.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecorderFailureKind {
    /// Le micro a disparu ou le flux audio a echoue en cours de route.
    MicLost,
    /// Le WAV n'a plus pu etre ecrit (disque plein).
    DiskFull,
}

impl RecorderFailureKind {
    /// Code stable montre a la personne (traduit dans `translateError.ts`).
    pub fn code(self) -> &'static str {
        match self {
            Self::MicLost => "PARLA_ERR:micLost",
            Self::DiskFull => "PARLA_ERR:diskFull",
        }
    }
}

/// Parlato : rapport envoye quand le worker s'arrete sur une erreur, une fois
/// le WAV ferme (ou abandonne).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecorderFailure {
    pub kind: RecorderFailureKind,
    /// Le WAV a ete ferme proprement et dure assez longtemps pour valoir une
    /// transcription.
    pub usable: bool,
}

/// Rappel appele (depuis le thread worker, qui se termine juste apres) quand
/// l'enregistrement s'arrete sur une erreur. Il ne doit pas attendre ce thread.
pub type FailureCallback = Arc<dyn Fn(RecorderFailure) + Send + Sync>;

/// En dessous de ~0,5 s d'audio, un enregistrement interrompu ne vaut pas
/// une transcription.
const MIN_USABLE_SAMPLES: u64 = (TARGET_SAMPLE_RATE as u64) / 2;

/// Un enregistrement interrompu vaut-il une transcription ?
fn is_usable(wav_closed: bool, samples_written: u64) -> bool {
    wav_closed && samples_written >= MIN_USABLE_SAMPLES
}

/// Handle sur un enregistrement actif.
pub struct RecorderHandle {
    stop_flag: Arc<AtomicBool>,
    meter: Arc<Mutex<AudioMeter>>,
    thread: Option<std::thread::JoinHandle<Result<()>>>,
    output_path: PathBuf,
}

impl RecorderHandle {
    pub fn current_meter(&self) -> AudioMeter {
        *self.meter.lock()
    }

    /// Arrete l'enregistrement et attend la finalisation du WAV.
    pub fn stop(mut self) -> Result<PathBuf> {
        self.stop_flag.store(true, Ordering::SeqCst);
        if let Some(handle) = self.thread.take() {
            match handle.join() {
                Ok(Ok(())) => {}
                Ok(Err(e)) => return Err(e),
                Err(_) => return Err(anyhow!("le thread d'enregistrement a panic")),
            }
        }
        Ok(self.output_path)
    }
}

pub struct AudioRecorder;

/// Parlato : issue de l'attente du signal "stream pret" du worker.
#[derive(Debug)]
enum Ready {
    Ok,
    /// Le worker a echoue pendant l'initialisation (il se termine de lui-meme).
    Failed(anyhow::Error),
    /// Pas de reponse dans le delai : le worker est abandonne.
    TimedOut,
}

/// Attend le signal du worker. Sur depassement du delai, marque le worker
/// comme abandonne et demande son arret, sans jamais l'attendre : l'ouverture
/// d'un peripherique peut rester bloquee indefiniment dans le pilote, et un
/// `join` figerait l'appelant avec elle.
fn wait_ready(
    rx: &mpsc::Receiver<Result<()>>,
    timeout: Duration,
    stop_flag: &AtomicBool,
    abandoned: &AtomicBool,
) -> Ready {
    match rx.recv_timeout(timeout) {
        Ok(Ok(())) => Ready::Ok,
        Ok(Err(e)) => Ready::Failed(e),
        Err(_) => {
            abandoned.store(true, Ordering::SeqCst);
            stop_flag.store(true, Ordering::SeqCst);
            Ready::TimedOut
        }
    }
}

/// Cote worker : annonce que le stream est pret. Faux si l'appelant a
/// abandonne l'attente (timeout) ou n'ecoute plus : le worker doit alors
/// fermer le stream et supprimer son WAV au lieu de tourner pour personne.
fn signal_ready(tx: &mpsc::Sender<Result<()>>, abandoned: &AtomicBool) -> bool {
    if abandoned.load(Ordering::SeqCst) {
        return false;
    }
    tx.send(Ok(())).is_ok()
}

impl AudioRecorder {
    /// Demarre une capture audio. Retourne apres que le thread worker a initialise
    /// le stream, pour pouvoir remonter les erreurs de setup au caller.
    ///
    /// `on_failure` est appele si l'enregistrement s'arrete tout seul plus tard
    /// (micro debranche, flux en erreur, disque plein).
    pub fn start(
        config: RecorderConfig,
        on_chunk: Option<ChunkCallback>,
        on_failure: Option<FailureCallback>,
    ) -> Result<RecorderHandle> {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let abandoned = Arc::new(AtomicBool::new(false));
        let meter = Arc::new(Mutex::new(AudioMeter::default()));

        let (ready_tx, ready_rx) = mpsc::channel::<Result<()>>();

        let worker_stop = stop_flag.clone();
        let worker_abandoned = abandoned.clone();
        let worker_meter = meter.clone();
        let output_path = config.output_path.clone();
        let config_clone = config.clone();

        let thread = std::thread::Builder::new()
            .name("parla-audio-worker".into())
            .spawn(move || -> Result<()> {
                run_worker(
                    config_clone,
                    on_chunk,
                    on_failure,
                    worker_stop,
                    worker_abandoned,
                    worker_meter,
                    ready_tx,
                )
            })?;

        // Attends que le worker signale que le stream est pret (ou echec).
        match wait_ready(&ready_rx, Duration::from_secs(5), &stop_flag, &abandoned) {
            Ready::Ok => Ok(RecorderHandle {
                stop_flag,
                meter,
                thread: Some(thread),
                output_path,
            }),
            Ready::Failed(e) => {
                // Le worker s'est arrete en erreur (retour imminent, rien ne bloque).
                let _ = thread.join();
                Err(e)
            }
            Ready::TimedOut => {
                // Parlato : pas de join. Le thread est detache ; s'il finit un
                // jour son ouverture, il voit `abandoned`, ferme le stream et
                // supprime son WAV (voir `run_worker`).
                warn!("timeout d'initialisation du recorder audio, worker abandonne");
                drop(thread);
                Err(anyhow!("timeout d'initialisation du recorder audio"))
            }
        }
    }
}

fn run_worker(
    config: RecorderConfig,
    on_chunk: Option<ChunkCallback>,
    on_failure: Option<FailureCallback>,
    stop_flag: Arc<AtomicBool>,
    abandoned: Arc<AtomicBool>,
    meter: Arc<Mutex<AudioMeter>>,
    ready_tx: mpsc::Sender<Result<()>>,
) -> Result<()> {
    // 1. Resolution peripherique.
    let device = match &config.device_name {
        Some(name) => find_input_device_by_name(name)
            .ok_or_else(|| anyhow!("peripherique audio introuvable: {name}")),
        None => default_input_device().ok_or_else(|| anyhow!("aucun peripherique audio par defaut")),
    };
    let device = match device {
        Ok(d) => d,
        Err(e) => {
            let _ = ready_tx.send(Err(anyhow!("{e}")));
            return Err(e);
        }
    };
    let device_name = device.name().unwrap_or_else(|_| "?".into());

    let supported = match device.default_input_config() {
        Ok(c) => c,
        Err(e) => {
            let e = anyhow!("default_input_config: {e}");
            let _ = ready_tx.send(Err(anyhow!("{e}")));
            return Err(e);
        }
    };
    let input_sample_rate = supported.sample_rate().0;
    let input_channels = supported.channels();
    let sample_format = supported.sample_format();

    // Parlato : l'appelant a abandonne l'attente pendant la resolution du
    // peripherique (pilote lent) : rien n'a ete cree, on s'en va.
    if abandoned.load(Ordering::SeqCst) {
        info!("Demarrage abandonne avant l'ouverture du flux");
        return Err(anyhow!("demarrage abandonne"));
    }

    info!(
        device = %device_name,
        sample_rate = input_sample_rate,
        channels = input_channels,
        format = ?sample_format,
        "Demarrage de l'enregistrement audio"
    );

    // 2. WAV writer 16 kHz mono Int16 (ref VoiceInk outputFormat L380-390).
    if let Some(parent) = config.output_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let wav_spec = hound::WavSpec {
        channels: TARGET_CHANNELS,
        sample_rate: TARGET_SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = match hound::WavWriter::create(&config.output_path, wav_spec) {
        Ok(w) => w,
        Err(e) => {
            let e = anyhow!("creation WAV {}: {e}", config.output_path.display());
            let _ = ready_tx.send(Err(anyhow!("{e}")));
            return Err(e);
        }
    };

    // 3. Channel callback audio -> worker loop.
    let (sample_tx, sample_rx) = mpsc::channel::<Vec<f32>>();
    // Parlato : leve par le callback d'erreur du flux (micro debranche...).
    let stream_lost = Arc::new(AtomicBool::new(false));

    let stream = match build_input_stream(&device, sample_format, sample_tx, stream_lost.clone()) {
        Ok(s) => s,
        Err(e) => {
            let _ = ready_tx.send(Err(anyhow!("{e}")));
            return Err(e);
        }
    };

    if let Err(e) = stream.play() {
        let e = anyhow!("stream play: {e}");
        let _ = ready_tx.send(Err(anyhow!("{e}")));
        return Err(e);
    }

    // Setup OK, on notifie le caller. S'il a abandonne l'attente entre-temps
    // (ouverture tres lente), personne ne tient ce handle : on ferme le flux
    // et on supprime le WAV vide au lieu de capturer pour personne.
    if !signal_ready(&ready_tx, &abandoned) {
        info!("Demarrage tardif d'un worker abandonne : flux ferme");
        drop(stream);
        drop(writer);
        let _ = std::fs::remove_file(&config.output_path);
        return Err(anyhow!("demarrage abandonne"));
    }

    // 4. Pipeline de traitement.
    let mut resampler = MonoResampler::new(input_sample_rate)?;
    let mut mono_buf: Vec<f32> = Vec::with_capacity(4096);
    let mut resampled: Vec<f32> = Vec::with_capacity(4096);
    let mut int16_buf: Vec<i16> = Vec::with_capacity(4096);
    let mut chunk_accum: Vec<i16> = Vec::with_capacity(CHUNK_FRAMES * 4);
    let mut last_meter_update = std::time::Instant::now();
    // Parlato : arret sur erreur (micro perdu, disque plein) et audio ecrit.
    let mut failure: Option<RecorderFailureKind> = None;
    let mut samples_written: u64 = 0;

    loop {
        let chunk = match sample_rx.recv_timeout(Duration::from_millis(100)) {
            Ok(c) => c,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if stop_flag.load(Ordering::SeqCst) {
                    break;
                }
                if stream_lost.load(Ordering::SeqCst) {
                    error!("Flux audio perdu : arret de l'enregistrement");
                    failure = Some(RecorderFailureKind::MicLost);
                    break;
                }
                continue;
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                // Le flux a lache son callback : sans demande d'arret, c'est
                // que le peripherique a disparu.
                if !stop_flag.load(Ordering::SeqCst) {
                    error!("Flux audio ferme : arret de l'enregistrement");
                    failure = Some(RecorderFailureKind::MicLost);
                }
                break;
            }
        };

        // Metering sur le buffer Float32 brut (avant resampling).
        let (rms_db, peak_db) = compute_rms_peak(&chunk);
        if last_meter_update.elapsed() >= Duration::from_millis(33) {
            *meter.lock() = AudioMeter { rms_db, peak_db };
            last_meter_update = std::time::Instant::now();
        } else {
            let mut m = meter.lock();
            m.peak_db = m.peak_db.max(peak_db);
        }

        mono_buf.clear();
        interleaved_to_mono(&chunk, input_channels, &mut mono_buf);

        resampled.clear();
        if let Err(e) = resampler.process(&mono_buf, &mut resampled) {
            warn!("Resampler err: {e}");
            continue;
        }

        int16_buf.clear();
        float_to_int16(&resampled, &mut int16_buf);

        if let Err(e) = write_samples(&mut writer, &int16_buf) {
            error!("Ecriture du WAV impossible : {e}");
            failure = Some(RecorderFailureKind::DiskFull);
            break;
        }
        samples_written += int16_buf.len() as u64;

        if let Some(cb) = on_chunk.as_ref() {
            chunk_accum.extend_from_slice(&int16_buf);
            while chunk_accum.len() >= CHUNK_FRAMES {
                let out_chunk = chunk_accum.drain(..CHUNK_FRAMES).collect::<Vec<_>>();
                cb(out_chunk);
            }
        }

        if stop_flag.load(Ordering::SeqCst) {
            break;
        }
        if stream_lost.load(Ordering::SeqCst) {
            error!("Flux audio perdu : arret de l'enregistrement");
            failure = Some(RecorderFailureKind::MicLost);
            break;
        }
    }

    // 5. Finalisation : flush resampler, ecrit le reste, ferme le WAV.
    drop(stream); // stop du stream wasapi avant flush

    // Disque plein : ecrire encore echouerait, on ne fait que fermer le WAV.
    let mut write_ok = failure != Some(RecorderFailureKind::DiskFull);
    if write_ok {
        resampled.clear();
        if let Err(e) = resampler.flush(&mut resampled) {
            warn!("Resampler flush: {e}");
        }
        int16_buf.clear();
        float_to_int16(&resampled, &mut int16_buf);
        match write_samples(&mut writer, &int16_buf) {
            Ok(()) => samples_written += int16_buf.len() as u64,
            Err(e) => {
                error!("Ecriture du WAV impossible : {e}");
                failure = Some(RecorderFailureKind::DiskFull);
                write_ok = false;
            }
        }
    }
    if let Some(cb) = on_chunk.as_ref() {
        if write_ok {
            chunk_accum.extend_from_slice(&int16_buf);
        }
        if !chunk_accum.is_empty() {
            let final_chunk: Vec<i16> = std::mem::take(&mut chunk_accum);
            cb(final_chunk);
        }
    }
    let finalized = match writer.finalize() {
        Ok(()) => true,
        Err(e) => {
            error!("Fermeture du WAV impossible : {e}");
            false
        }
    };

    *meter.lock() = AudioMeter::default();

    match failure {
        None => {
            if !finalized {
                // Arret demande mais WAV illisible : l'appelant le sait.
                return Err(anyhow!("fermeture du WAV impossible"));
            }
            debug!("Worker audio termine");
            Ok(())
        }
        Some(kind) => {
            let usable = is_usable(finalized && write_ok, samples_written);
            info!(code = kind.code(), usable, samples_written, "Enregistrement interrompu");
            if let Some(cb) = on_failure.as_ref() {
                cb(RecorderFailure { kind, usable });
            }
            Ok(())
        }
    }
}

fn write_samples(
    writer: &mut hound::WavWriter<std::io::BufWriter<std::fs::File>>,
    samples: &[i16],
) -> std::result::Result<(), hound::Error> {
    for &s in samples {
        writer.write_sample(s)?;
    }
    Ok(())
}

fn build_input_stream(
    device: &cpal::Device,
    format: SampleFormat,
    tx: mpsc::Sender<Vec<f32>>,
    lost: Arc<AtomicBool>,
) -> Result<cpal::Stream> {
    let config = device
        .default_input_config()
        .context("default_input_config failed")?;
    let stream_config: cpal::StreamConfig = config.clone().into();

    // Parlato : une erreur du flux (micro debranche) ne fait plus que se
    // logger : elle leve `lost`, que la boucle du worker lit pour arreter
    // l'enregistrement et le signaler.
    let err_fn = move || {
        let lost = lost.clone();
        move |e: cpal::StreamError| {
            error!("Erreur stream audio: {e}");
            lost.store(true, Ordering::SeqCst);
        }
    };

    let tx_f32 = tx.clone();
    let tx_i16 = tx.clone();
    let tx_u16 = tx;
    let stream = match format {
        SampleFormat::F32 => device.build_input_stream(
            &stream_config,
            move |data: &[f32], _| {
                let _ = tx_f32.send(data.to_vec());
            },
            err_fn(),
            None,
        )?,
        SampleFormat::I16 => device.build_input_stream(
            &stream_config,
            move |data: &[i16], _| {
                let buf: Vec<f32> = data.iter().map(|&s| s as f32 / 32768.0).collect();
                let _ = tx_i16.send(buf);
            },
            err_fn(),
            None,
        )?,
        SampleFormat::U16 => device.build_input_stream(
            &stream_config,
            move |data: &[u16], _| {
                let buf: Vec<f32> = data
                    .iter()
                    .map(|&s| (s as f32 - 32768.0) / 32768.0)
                    .collect();
                let _ = tx_u16.send(buf);
            },
            err_fn(),
            None,
        )?,
        other => return Err(anyhow!("format audio non supporte: {other:?}")),
    };
    Ok(stream)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wait_ready_reports_success_and_errors() {
        let stop = AtomicBool::new(false);
        let abandoned = AtomicBool::new(false);
        let (tx, rx) = mpsc::channel::<Result<()>>();
        tx.send(Ok(())).unwrap();
        assert!(matches!(
            wait_ready(&rx, Duration::from_millis(50), &stop, &abandoned),
            Ready::Ok
        ));
        tx.send(Err(anyhow!("no device"))).unwrap();
        assert!(matches!(
            wait_ready(&rx, Duration::from_millis(50), &stop, &abandoned),
            Ready::Failed(_)
        ));
        assert!(!abandoned.load(Ordering::SeqCst));
        assert!(!stop.load(Ordering::SeqCst));
    }

    #[test]
    fn a_timeout_abandons_the_worker_without_waiting_for_it() {
        let stop = AtomicBool::new(false);
        let abandoned = AtomicBool::new(false);
        // The sender stays alive and silent, like a driver stuck opening.
        let (_tx, rx) = mpsc::channel::<Result<()>>();
        let started = std::time::Instant::now();
        let outcome = wait_ready(&rx, Duration::from_millis(30), &stop, &abandoned);
        assert!(matches!(outcome, Ready::TimedOut));
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(abandoned.load(Ordering::SeqCst), "marked abandoned");
        assert!(stop.load(Ordering::SeqCst), "asked to stop");
    }

    #[test]
    fn a_late_success_from_an_abandoned_worker_is_refused() {
        // After the timeout the worker finally opens the device: it must be
        // told to shut the stream down instead of capturing for nobody.
        let abandoned = AtomicBool::new(false);
        let (tx, rx) = mpsc::channel::<Result<()>>();
        let stop = AtomicBool::new(false);
        let _ = wait_ready(&rx, Duration::from_millis(10), &stop, &abandoned);
        assert!(!signal_ready(&tx, &abandoned));
    }

    #[test]
    fn a_dropped_listener_also_refuses_the_signal() {
        let abandoned = AtomicBool::new(false);
        let (tx, rx) = mpsc::channel::<Result<()>>();
        drop(rx);
        assert!(!signal_ready(&tx, &abandoned));
    }

    #[test]
    fn a_live_listener_accepts_the_signal() {
        let abandoned = AtomicBool::new(false);
        let (tx, rx) = mpsc::channel::<Result<()>>();
        assert!(signal_ready(&tx, &abandoned));
        assert!(rx.try_recv().unwrap().is_ok());
    }

    #[test]
    fn failure_codes_match_translate_error() {
        assert_eq!(RecorderFailureKind::MicLost.code(), "PARLA_ERR:micLost");
        assert_eq!(RecorderFailureKind::DiskFull.code(), "PARLA_ERR:diskFull");
    }

    #[test]
    fn an_interrupted_recording_is_usable_only_if_closed_and_long_enough() {
        assert!(is_usable(true, MIN_USABLE_SAMPLES));
        assert!(!is_usable(true, MIN_USABLE_SAMPLES - 1), "too short");
        assert!(!is_usable(false, 10 * MIN_USABLE_SAMPLES), "WAV not closed");
    }

    #[test]
    fn write_samples_writes_every_sample() {
        let path = std::env::temp_dir().join(format!("parlato-rec-test-{}.wav", std::process::id()));
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&path, spec).unwrap();
        write_samples(&mut w, &[1, 2, 3]).unwrap();
        w.finalize().unwrap();
        let n = hound::WavReader::open(&path).unwrap().len();
        let _ = std::fs::remove_file(&path);
        assert_eq!(n, 3);
    }
}
