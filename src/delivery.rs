use crate::{
    audio_io::{self, Result, Stereo},
    dsp::{self, Oversampler},
    resample,
};
use serde::{Deserialize, Serialize};
use std::{path::Path, str::FromStr};

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Float32,
    Pcm16,
    #[default]
    Pcm24,
}

impl FromStr for Format {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s {
            "float32" => Ok(Self::Float32),
            "pcm16" => Ok(Self::Pcm16),
            "pcm24" => Ok(Self::Pcm24),
            _ => Err("Format must be float32, pcm16, or pcm24".into()),
        }
    }
}

/// Write `audio`, at `rate`, as a new WAV file at `path` in `format`.
pub fn write(path: &Path, audio: &[Stereo], rate: u32, format: Format, seed: u64) -> Result<()> {
    validate_export(audio, format)?;
    let mut file = std::io::BufWriter::new(
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?,
    );
    write_to(&mut file, audio, rate, format, seed)
}

fn validate_export(audio: &[Stereo], format: Format) -> Result<()> {
    let bound = match format {
        Format::Float32 => f64::INFINITY,
        Format::Pcm16 => 1.0 - 2.0 / 32768.0,
        Format::Pcm24 => 1.0 - 2.0 / 8388608.0,
    };
    if audio
        .iter()
        .flatten()
        .any(|x| !x.is_finite() || (*x as f64).abs() >= bound)
    {
        return Err("Export requires finite audio with dither headroom for integer PCM".into());
    }
    Ok(())
}

/// Write `audio`, at `rate`, as a WAV stream in `format`. Integer formats
/// receive TPDF dither from `seed`.
pub fn write_to(
    writer: &mut impl std::io::Write,
    audio: &[Stereo],
    rate: u32,
    format: Format,
    seed: u64,
) -> Result<()> {
    validate_export(audio, format)?;
    if matches!(format, Format::Float32) {
        return audio_io::write_to(writer, audio, rate);
    }
    let bits = if matches!(format, Format::Pcm16) {
        16u16
    } else {
        24
    };
    let scale = 2.0f64.powi(bits as i32 - 1);
    let mut state = seed;
    let mut uniform = || {
        state = state.wrapping_add(0x9e3779b97f4a7c15);
        let mut x = state;
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
        ((x ^ (x >> 31)) >> 11) as f64 / (1u64 << 53) as f64
    };
    audio_io::write_quantized_to(writer, audio, rate, bits, |x| {
        (x as f64 * scale + uniform() - uniform()).round() as i32
    })
}

/// A copy of a delivered master at another sample rate or word length.
#[derive(Serialize)]
pub struct Copy {
    #[serde(skip)]
    pub wav: Vec<u8>,
    pub sample_rate: u32,
    pub format: Format,
    pub frames: usize,
    /// Peak estimate of the copy before the dither, in dBTP.
    pub peak_estimate_dbtp: f64,
    /// Gain that brought the copy's peak estimate to the ceiling, in dB:
    /// zero or negative.
    pub trim_db: f64,
}

/// A WAV copy of a master at `master_rate` at `rate` and `format`
/// (`delivery.copy.v1`). The rate conversion comes first. A conversion
/// moves the samples between the peaks of the waveform, so the peak estimate
/// of the copy is measured again; a copy above `ceiling_dbtp` is turned down
/// by the excess. The dither comes last and is applied once to the copy.
pub fn copy(
    master: &[Stereo],
    master_rate: u32,
    rate: u32,
    format: Format,
    ceiling_dbtp: f64,
    seed: u64,
) -> Result<Copy> {
    if matches!(format, Format::Float32) || !ceiling_dbtp.is_finite() || ceiling_dbtp > 0.0 {
        return Err("A copy is PCM16 or PCM24 under a ceiling at or below 0 dBTP".into());
    }
    let mut audio = resample::convert(master, master_rate, rate)?;
    let peak = dsp::db(Oversampler::new(rate).peak(&audio));
    let trim_db = (ceiling_dbtp - peak).min(0.0);
    if trim_db < 0.0 {
        let gain = 10f64.powf(trim_db / 20.0) as f32;
        for sample in audio.iter_mut().flatten() {
            *sample *= gain;
        }
    }
    let mut wav = Vec::with_capacity(audio.len() * 6 + 128);
    write_to(&mut wav, &audio, rate, format, seed)?;
    Ok(Copy {
        wav,
        sample_rate: rate,
        format,
        frames: audio.len(),
        peak_estimate_dbtp: peak + trim_db,
        trim_db,
    })
}

#[derive(Serialize)]
pub struct Verification {
    pub schema: &'static str,
    pub format: Format,
    pub sample_rate: u32,
    pub frames: usize,
    pub integrated_lufs: f64,
    pub peak_estimate_dbtp: f64,
    pub maximum_sample_error: f64,
    pub verified: bool,
    pub note: &'static str,
}

/// `verify_samples` of the WAV at `path`, decoded block by block: a whole
/// decoded copy of a 30-minute delivery is 0.6 GB. `expected` is at `rate`.
pub fn verify(
    path: &Path,
    expected: &[Stereo],
    rate: u32,
    format: Format,
    ceiling: f64,
) -> Result<Verification> {
    verify_in_cores(path, expected, rate, format, ceiling, 1 << 20)
}

fn verify_in_cores(
    path: &Path,
    expected: &[Stereo],
    rate: u32,
    format: Format,
    ceiling: f64,
    core: usize,
) -> Result<Verification> {
    if !ceiling.is_finite() || expected.iter().flatten().any(|x| !x.is_finite()) {
        return Err("Delivery verification requires finite samples and ceiling".into());
    }
    let file = std::fs::File::open(path)?;
    let bytes = file.metadata()?.len();
    let oversampler = Oversampler::new(rate);
    let radius = dsp::RADIUS as usize;
    let mut meter = dsp::LoudnessMeter::new(rate);
    let (mut decoded, mut maximum_sample_error, mut peak) = (0usize, 0.0f64, 0.0f64);
    // `window[0]` is decoded frame `window_start`. Each core of the peak
    // estimate has `radius` frames of context on each side, or a file end.
    let (mut window, mut window_start, mut next_core) = (Vec::new(), 0usize, 0usize);
    let sample_rate = audio_io::read_blocks(file, bytes, |_, block| {
        let Some(reference) = expected.get(decoded..decoded + block.len()) else {
            return Err("Decoded delivery duration or rate differs".into());
        };
        maximum_sample_error = reference
            .iter()
            .flatten()
            .zip(block.iter().flatten())
            .map(|(a, b)| (*a as f64 - *b as f64).abs())
            .fold(maximum_sample_error, f64::max);
        meter.push(block);
        window.extend_from_slice(block);
        decoded += block.len();
        while window_start + window.len() >= next_core + core + radius {
            let start = next_core - window_start;
            peak = peak.max(oversampler.peak_core(&window, start..start + core));
            next_core += core;
            let keep_from = next_core - radius - window_start;
            window.drain(..keep_from);
            window_start += keep_from;
        }
        Ok(())
    })?;
    if sample_rate != rate || decoded != expected.len() {
        return Err("Decoded delivery duration or rate differs".into());
    }
    if next_core < decoded {
        let start = next_core - window_start;
        peak = peak.max(oversampler.peak_core(&window, start..window.len()));
    }
    let error_bound = match format {
        Format::Float32 => 0.0,
        Format::Pcm16 => 1.501 / 32768.0,
        Format::Pcm24 => 1.501 / 8388608.0,
    };
    let peak_estimate_dbtp = dsp::db(peak);
    let integrated_lufs = meter
        .finish()
        .ok_or("Decoded delivery is below the loudness gate")?;
    if maximum_sample_error > error_bound || peak_estimate_dbtp > ceiling + 1e-6 {
        return Err("Decoded delivery failed sample-error or peak verification".into());
    }
    Ok(Verification { schema:"mastering.delivery.1", format, sample_rate:rate, frames:decoded, integrated_lufs,
        peak_estimate_dbtp, maximum_sample_error, verified:true,
        note:"Decoded WAV verification with own meter and 4x peak estimator; formal conformance remains open" })
}

/// Verify `decoded`, a decoded delivery at `sample_rate`, against `expected`
/// at `rate`.
pub fn verify_samples(
    decoded: &[Stereo],
    sample_rate: u32,
    expected: &[Stereo],
    rate: u32,
    format: Format,
    ceiling: f64,
) -> Result<Verification> {
    if sample_rate != rate || decoded.len() != expected.len() {
        return Err("Decoded delivery duration or rate differs".into());
    }
    if !ceiling.is_finite()
        || decoded
            .iter()
            .chain(expected)
            .flatten()
            .any(|x| !x.is_finite())
    {
        return Err("Delivery verification requires finite samples and ceiling".into());
    }
    let maximum_sample_error = expected
        .iter()
        .flatten()
        .zip(decoded.iter().flatten())
        .map(|(a, b)| (*a as f64 - *b as f64).abs())
        .fold(0.0, f64::max);
    let error_bound = match format {
        Format::Float32 => 0.0,
        Format::Pcm16 => 1.501 / 32768.0,
        Format::Pcm24 => 1.501 / 8388608.0,
    };
    let peak_estimate_dbtp = dsp::db(Oversampler::new(rate).peak(decoded));
    let integrated_lufs =
        dsp::loudness(decoded, rate).ok_or("Decoded delivery is below the loudness gate")?;
    if maximum_sample_error > error_bound || peak_estimate_dbtp > ceiling + 1e-6 {
        return Err("Decoded delivery failed sample-error or peak verification".into());
    }
    Ok(Verification { schema:"mastering.delivery.1", format, sample_rate:rate, frames:decoded.len(), integrated_lufs,
        peak_estimate_dbtp, maximum_sample_error, verified:true,
        note:"Decoded WAV verification with own meter and 4x peak estimator; formal conformance remains open" })
}

#[cfg(test)]
mod stream_tests {
    use super::*;

    #[test]
    fn stream_and_file_delivery_match_for_every_format() {
        let audio: Vec<Stereo> = (0..48000 * 3)
            .map(|n| {
                let phase = n as f32 * std::f32::consts::TAU / 48000.0;
                [0.19 * (phase * 220.0).sin(), 0.13 * (phase * 331.0).sin()]
            })
            .collect();
        for format in [Format::Float32, Format::Pcm16, Format::Pcm24] {
            let mut streamed = Vec::new();
            write_to(&mut streamed, &audio, 48000, format, 8675309).unwrap();
            let path = std::env::temp_dir().join(format!(
                "mastering-delivery-parity-{}-{:?}.wav",
                std::process::id(),
                format
            ));
            write(&path, &audio, 48000, format, 8675309).unwrap();
            let stored = std::fs::read(&path).unwrap();
            std::fs::remove_file(path).unwrap();
            assert_eq!(streamed, stored);
            let decoded =
                audio_io::read_stream(std::io::Cursor::new(&streamed), streamed.len() as u64)
                    .unwrap();
            assert_eq!(decoded.audio.len(), audio.len());
            assert!(verify_samples(
                &decoded.audio,
                decoded.sample_rate,
                &audio,
                48000,
                format,
                0.0
            )
            .is_ok());
        }
    }

    #[test]
    fn verification_in_cores_matches_the_whole_verification() {
        let audio: Vec<Stereo> = (0..48000 * 4 + 333)
            .map(|n| {
                let phase = n as f32 * std::f32::consts::TAU / 48000.0;
                let burst = if (n / 9000) % 3 == 0 { 0.6 } else { 0.2 };
                [
                    burst * (phase * 997.0).sin(),
                    0.3 * (phase * 11_003.0).sin(),
                ]
            })
            .collect();
        for format in [Format::Float32, Format::Pcm16, Format::Pcm24] {
            let path = std::env::temp_dir().join(format!(
                "mastering-delivery-cores-{}-{:?}.wav",
                std::process::id(),
                format
            ));
            write(&path, &audio, 48000, format, 99).unwrap();
            let decoded = audio_io::read_source(&path).unwrap();
            let whole = verify_samples(
                &decoded.audio,
                decoded.sample_rate,
                &audio,
                48000,
                format,
                0.0,
            )
            .unwrap();
            for core in [97, 4096, 65_536, 1 << 20] {
                let cores = verify_in_cores(&path, &audio, 48000, format, 0.0, core).unwrap();
                assert_eq!(cores.frames, whole.frames);
                assert_eq!(
                    cores.integrated_lufs.to_bits(),
                    whole.integrated_lufs.to_bits()
                );
                assert_eq!(
                    cores.peak_estimate_dbtp.to_bits(),
                    whole.peak_estimate_dbtp.to_bits()
                );
                assert_eq!(
                    cores.maximum_sample_error.to_bits(),
                    whole.maximum_sample_error.to_bits()
                );
            }
            assert!(verify_in_cores(&path, &audio[1..], 48000, format, 0.0, 4096).is_err());
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn public_verification_rejects_nonfinite_samples() {
        let expected = vec![[0.1, 0.2]; 48000 * 3];
        let mut decoded = expected.clone();
        decoded[20][0] = f32::NAN;
        assert!(verify_samples(&decoded, 48000, &expected, 48000, Format::Pcm24, 0.0).is_err());
        assert!(verify_samples(&expected, 48000, &decoded, 48000, Format::Pcm24, 0.0).is_err());
    }
    #[test]
    fn a_copy_changes_the_rate_and_the_word_length_under_the_ceiling() {
        let tone = |amplitude: f32| -> Vec<Stereo> {
            (0..48000 * 4)
                .map(|n| {
                    let x = amplitude * (n as f32 * std::f32::consts::TAU * 997.0 / 48000.0).sin();
                    [x, -x]
                })
                .collect()
        };
        // A tone under the ceiling keeps its level.
        let made = copy(&tone(0.5), 48000, 44100, Format::Pcm16, -1.0, 7).unwrap();
        let decoded =
            audio_io::read_stream(std::io::Cursor::new(&made.wav), made.wav.len() as u64).unwrap();
        assert_eq!(decoded.sample_rate, 44100);
        assert_eq!(decoded.audio.len(), 44100 * 4);
        assert_eq!(made.frames, decoded.audio.len());
        assert_eq!(made.trim_db, 0.0);
        assert!((made.peak_estimate_dbtp - dsp::db(0.5)).abs() < 0.05);
        // A tone above the ceiling is turned down to it.
        let made = copy(&tone(0.99), 48000, 44100, Format::Pcm24, -1.0, 7).unwrap();
        assert!(
            made.trim_db < -0.8 && made.trim_db > -1.0,
            "{}",
            made.trim_db
        );
        let decoded =
            audio_io::read_stream(std::io::Cursor::new(&made.wav), made.wav.len() as u64).unwrap();
        let peak = dsp::db(Oversampler::new(44100).peak(&decoded.audio));
        assert!(peak <= -1.0 + 0.01 && peak > -1.1, "{peak}");
        // A copy is integer PCM.
        assert!(copy(&tone(0.5), 48000, 44100, Format::Float32, -1.0, 7).is_err());
    }
}
