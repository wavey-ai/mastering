use crate::{
    audio_io::{self, Result, Stereo},
    dsp::{self, Oversampler},
    resample,
};
use serde::{Deserialize, Serialize};
use std::{io::Read, path::Path, str::FromStr};

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
    write_frames(path, &mut crate::scratch::Slice(audio), rate, format, seed)
}

/// `write` of the frames of `audio`, read in pieces: a stored render
/// (D253).
pub(crate) fn write_frames(
    path: &Path,
    audio: &mut impl crate::scratch::Frames,
    rate: u32,
    format: Format,
    seed: u64,
) -> Result<()> {
    validate_export(audio, format)?;
    let mut file = std::io::BufWriter::new(
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?,
    );
    write_valid_to(&mut file, audio, rate, format, seed)
}

fn validate_export(audio: &mut impl crate::scratch::Frames, format: Format) -> Result<()> {
    let bound = match format {
        Format::Float32 => f64::INFINITY,
        Format::Pcm16 => 1.0 - 2.0 / 32768.0,
        Format::Pcm24 => 1.0 - 2.0 / 8388608.0,
    };
    let mut valid = true;
    crate::scratch::for_each_block(audio, |block| {
        valid &= block
            .iter()
            .flatten()
            .all(|x| x.is_finite() && (*x as f64).abs() < bound);
        Ok(())
    })?;
    if !valid {
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
    let mut audio = crate::scratch::Slice(audio);
    validate_export(&mut audio, format)?;
    write_valid_to(writer, &mut audio, rate, format, seed)
}

/// `write_to` of the frames of the render `audio`. The frames of a stored
/// render are read in pieces (D253).
pub fn write_render_to(
    writer: &mut impl std::io::Write,
    audio: &crate::treatment_render::RenderAudio,
    rate: u32,
    format: Format,
    seed: u64,
) -> Result<()> {
    let mut frames = audio.frames();
    validate_export(&mut frames, format)?;
    write_valid_to(writer, &mut frames, rate, format, seed)
}

/// `write_to` of the frames of `audio`, which `validate_export` passed.
fn write_valid_to(
    writer: &mut impl std::io::Write,
    audio: &mut impl crate::scratch::Frames,
    rate: u32,
    format: Format,
    seed: u64,
) -> Result<()> {
    if matches!(format, Format::Float32) {
        return audio_io::write_f32_to(writer, audio, rate);
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

/// Verify the WAV at `path` against `expected` at `rate` (`verify_stream`).
pub fn verify(
    path: &Path,
    expected: &[Stereo],
    rate: u32,
    format: Format,
    ceiling: f64,
) -> Result<Verification> {
    verify_frames(
        path,
        &mut crate::scratch::Slice(expected),
        rate,
        format,
        ceiling,
    )
}

/// `verify` against the frames of `expected`, read in blocks: a stored
/// render (D253).
pub(crate) fn verify_frames(
    path: &Path,
    expected: &mut impl crate::scratch::Frames,
    rate: u32,
    format: Format,
    ceiling: f64,
) -> Result<Verification> {
    let (file, bytes) = audio_io::open(path)?;
    verify_in_cores(
        file,
        bytes,
        expected,
        rate,
        format,
        ceiling,
        1 << 20,
        |_| {},
    )
}

/// Verify the WAV of `bytes` bytes that `reader` gives against `expected` at
/// `rate`: the sample error, the loudness and the 4x peak estimate of the
/// decoded samples. The WAV is decoded block by block: a whole decoded copy
/// of a 30-minute delivery is 0.6 GB. `each` receives each decoded block in
/// order, inside the duration of `expected`.
pub fn verify_stream(
    reader: impl Read,
    bytes: u64,
    expected: &[Stereo],
    rate: u32,
    format: Format,
    ceiling: f64,
    each: impl FnMut(&[Stereo]),
) -> Result<Verification> {
    verify_in_cores(
        reader,
        bytes,
        &mut crate::scratch::Slice(expected),
        rate,
        format,
        ceiling,
        1 << 20,
        each,
    )
}

#[allow(clippy::too_many_arguments)]
fn verify_in_cores(
    reader: impl Read,
    bytes: u64,
    expected: &mut impl crate::scratch::Frames,
    rate: u32,
    format: Format,
    ceiling: f64,
    core: usize,
    mut each: impl FnMut(&[Stereo]),
) -> Result<Verification> {
    if !ceiling.is_finite() || !crate::scratch::all_finite(expected)? {
        return Err("Delivery verification requires finite samples and ceiling".into());
    }
    let oversampler = Oversampler::new(rate);
    let radius = dsp::RADIUS as usize;
    let mut meter = dsp::LoudnessMeter::new(rate);
    let (mut decoded, mut maximum_sample_error, mut peak) = (0usize, 0.0f64, 0.0f64);
    // `window[0]` is decoded frame `window_start`. Each core of the peak
    // estimate has `radius` frames of context on each side, or a file end.
    let (mut window, mut window_start, mut next_core) = (Vec::new(), 0usize, 0usize);
    let frames = expected.len();
    let mut reference = Vec::new();
    let sample_rate = audio_io::read_blocks(reader, bytes, |_, block| {
        if block.len() > frames - decoded {
            return Err("Decoded delivery duration or rate differs".into());
        }
        let reference = expected.window(decoded, block.len(), &mut reference)?;
        each(block);
        maximum_sample_error = reference
            .iter()
            .flatten()
            .zip(block.iter().flatten())
            .map(|(a, b)| (*a as f64 - *b as f64).abs())
            // The samples are finite, so a comparison gives the value of
            // `max` (D332).
            .fold(maximum_sample_error, dsp::larger);
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
    if sample_rate != rate || decoded != frames {
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
            let mut blocks = Vec::new();
            let verified = verify_stream(
                std::io::Cursor::new(&streamed),
                streamed.len() as u64,
                &audio,
                48000,
                format,
                0.0,
                |block| blocks.extend_from_slice(block),
            )
            .unwrap();
            assert_eq!(verified.frames, audio.len());
            assert_eq!(blocks, decoded.audio);
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
            let maximum_sample_error = audio
                .iter()
                .flatten()
                .zip(decoded.audio.iter().flatten())
                .map(|(a, b)| (*a as f64 - *b as f64).abs())
                .fold(0.0f64, f64::max);
            let peak = dsp::db(Oversampler::new(48000).peak(&decoded.audio));
            let lufs = dsp::loudness(&decoded.audio, 48000).unwrap();
            let in_cores = |expected: &[Stereo], core| {
                let (file, bytes) = audio_io::open(&path).unwrap();
                verify_in_cores(
                    file,
                    bytes,
                    &mut crate::scratch::Slice(expected),
                    48000,
                    format,
                    0.0,
                    core,
                    |_| {},
                )
            };
            for core in [97, 4096, 65_536, 1 << 20] {
                let cores = in_cores(&audio, core).unwrap();
                assert_eq!(cores.frames, decoded.audio.len());
                assert_eq!(cores.integrated_lufs.to_bits(), lufs.to_bits());
                assert_eq!(cores.peak_estimate_dbtp.to_bits(), peak.to_bits());
                assert_eq!(
                    cores.maximum_sample_error.to_bits(),
                    maximum_sample_error.to_bits()
                );
            }
            // A stored render gives the file and the verification of the
            // frames in memory (D253).
            let mut stored = crate::scratch::Pcm::temporary_of(&audio).unwrap();
            let copy = path.with_extension("stored.wav");
            write_frames(&copy, &mut stored, 48000, format, 99).unwrap();
            assert_eq!(std::fs::read(&copy).unwrap(), std::fs::read(&path).unwrap());
            std::fs::remove_file(copy).unwrap();
            let (file, bytes) = audio_io::open(&path).unwrap();
            let from_stored =
                verify_in_cores(file, bytes, &mut stored, 48000, format, 0.0, 4096, |_| {})
                    .unwrap();
            assert_eq!(
                serde_json::to_value(&from_stored).unwrap(),
                serde_json::to_value(in_cores(&audio, 4096).unwrap()).unwrap()
            );
            assert!(in_cores(&audio[1..], 4096).is_err());
            let mut longer = audio.clone();
            longer.push([0.0, 0.0]);
            assert!(in_cores(&longer, 4096).is_err());
            std::fs::remove_file(path).unwrap();
        }
    }

    #[test]
    fn public_verification_rejects_nonfinite_samples() {
        let expected = vec![[0.1, 0.2]; 48000 * 3];
        let mut wav = Vec::new();
        write_to(&mut wav, &expected, 48000, Format::Float32, 0).unwrap();
        let check = |wav: &[u8], expected: &[Stereo], ceiling| {
            let bytes = wav.len() as u64;
            verify_stream(
                wav,
                bytes,
                expected,
                48000,
                Format::Float32,
                ceiling,
                |_| {},
            )
        };
        assert!(check(&wav, &expected, 0.0).is_ok());
        assert!(check(&wav, &expected, f64::NAN).is_err());
        let mut nonfinite = expected.clone();
        nonfinite[20][0] = f32::NAN;
        assert!(check(&wav, &nonfinite, 0.0).is_err());
        // The samples of the float32 WAV end the file, 8 bytes to a frame.
        let sample = wav.len() - (expected.len() - 20) * 8;
        wav[sample..sample + 4].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(check(&wav, &expected, 0.0).is_err());
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
