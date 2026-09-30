// Parlato : conversion d'un fichier audio ou video quelconque en WAV
// 16 kHz mono 16 bits, le format que toute la chaine de transcription
// attend (whisper, parakeet, VAD et upload cloud lisent ce WAV tel quel,
// sans resampling).
//
// Pas d'equivalent direct cote VoiceInk (AudioTranscriptionService passe
// par AVFoundation). Ici : decodage via symphonia (mp3, m4a/aac, alac,
// flac, ogg/vorbis, wav, aiff, caf, piste audio des mp4/mkv), mixdown
// mono et resampling via le meme MonoResampler que l'enregistreur.

use std::path::Path;

use anyhow::{anyhow, Result};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymphoniaError;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use crate::audio::resampler::{float_to_int16, interleaved_to_mono, MonoResampler};
use crate::audio::TARGET_SAMPLE_RATE;

/// Duree maximale acceptee (2 h, comme la maquette). Au-dela, les samples
/// f32 en memoire (~460 Mo pour 2 h) deviennent deraisonnables.
pub const MAX_DURATION_SECS: u64 = 2 * 60 * 60;

/// Decode `input` et l'ecrit en WAV 16 kHz mono 16 bits dans `output`.
/// Retourne la duree en secondes.
pub fn convert_to_wav(input: &Path, output: &Path) -> Result<f64> {
    let samples = decode_to_mono_16k(input)?;
    if samples.is_empty() {
        anyhow::bail!("PARLA_ERR:fileNoAudio");
    }

    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: TARGET_SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut pcm = Vec::with_capacity(samples.len());
    float_to_int16(&samples, &mut pcm);
    let mut writer = hound::WavWriter::create(output, spec)?;
    for s in pcm {
        writer.write_sample(s)?;
    }
    writer.finalize()?;

    Ok(samples.len() as f64 / TARGET_SAMPLE_RATE as f64)
}

/// Decode la premiere piste audio du fichier en f32 mono 16 kHz.
pub fn decode_to_mono_16k(path: &Path) -> Result<Vec<f32>> {
    let file = std::fs::File::open(path)?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|_| anyhow!("PARLA_ERR:fileUnsupported"))?;
    let mut format = probed.format;

    let track = format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| anyhow!("PARLA_ERR:fileNoAudio"))?;
    let track_id = track.id;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|_| anyhow!("PARLA_ERR:fileUnsupported"))?;

    let max_samples = (MAX_DURATION_SECS * TARGET_SAMPLE_RATE as u64) as usize;
    let mut resampler: Option<MonoResampler> = None;
    let mut mono: Vec<f32> = Vec::new();
    let mut out: Vec<f32> = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            // Fin de flux : symphonia signale l'EOF par une IoError.
            Err(SymphoniaError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                break
            }
            Err(SymphoniaError::ResetRequired) => break,
            Err(e) => return Err(e.into()),
        };
        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            // Trame corrompue : on la saute, comme les lecteurs audio.
            Err(SymphoniaError::DecodeError(_)) => continue,
            Err(e) => return Err(e.into()),
        };

        let spec = *decoded.spec();
        let channels = spec.channels.count() as u16;
        let rs = match resampler.as_mut() {
            Some(r) => r,
            None => resampler.insert(MonoResampler::new(spec.rate)?),
        };

        let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        buf.copy_interleaved_ref(decoded);

        mono.clear();
        interleaved_to_mono(buf.samples(), channels, &mut mono);
        rs.process(&mono, &mut out)?;

        if out.len() > max_samples {
            anyhow::bail!("PARLA_ERR:fileTooLong");
        }
    }

    if let Some(rs) = resampler.as_mut() {
        rs.flush(&mut out)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un WAV 44.1 kHz stereo d'une seconde doit ressortir en 16 kHz mono,
    /// ~1 s, avec le signal preserve (pas un buffer de zeros).
    #[test]
    fn converts_44k_stereo_wav_to_16k_mono() {
        let dir = std::env::temp_dir().join(format!("parlato-audio-file-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("in.wav");
        let output = dir.join("out.wav");

        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 44_100,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut w = hound::WavWriter::create(&input, spec).unwrap();
        for i in 0..44_100u32 {
            let t = i as f32 / 44_100.0;
            let s = ((t * 440.0 * std::f32::consts::TAU).sin() * 0.5 * i16::MAX as f32) as i16;
            w.write_sample(s).unwrap();
            w.write_sample(s).unwrap();
        }
        w.finalize().unwrap();

        let secs = convert_to_wav(&input, &output).unwrap();
        assert!((secs - 1.0).abs() < 0.02, "duree inattendue: {secs}");

        let reader = hound::WavReader::open(&output).unwrap();
        let out_spec = reader.spec();
        assert_eq!(out_spec.channels, 1);
        assert_eq!(out_spec.sample_rate, TARGET_SAMPLE_RATE);
        let peak = reader
            .into_samples::<i16>()
            .map(|s| s.unwrap().unsigned_abs())
            .max()
            .unwrap();
        assert!(peak > 10_000, "signal perdu, pic = {peak}");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn rejects_non_audio_file() {
        let dir = std::env::temp_dir().join(format!("parlato-audio-file-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let input = dir.join("notes.txt");
        std::fs::write(&input, "not audio").unwrap();
        let err = convert_to_wav(&input, &dir.join("out.wav")).unwrap_err();
        assert!(err.to_string().starts_with("PARLA_ERR:"), "{err}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
