//! Test signals and tone measurements.
use crate::{
    audio_io::{Result, Stereo},
    dsp,
};
use serde::Serialize;
use std::f64::consts::TAU;

pub fn tone(hz: f64, amplitude: f64, frames: usize, rate: u32) -> Vec<Stereo> {
    (0..frames)
        .map(|i| {
            let x = (amplitude * (TAU * hz * i as f64 / rate as f64).sin()) as f32;
            [x, x]
        })
        .collect()
}

/// Six seconds of seeded noise bursts at 48 kHz for compressor selection tests.
/// Coherent-window projection returns peak amplitude, averaged by channel power.
pub fn amplitude(audio: &[Stereo], hz: f64, rate: u32) -> Result<f64> {
    let cycles = hz * audio.len() as f64 / rate as f64;
    if audio.len() < 4
        || rate == 0
        || !hz.is_finite()
        || hz <= 0.0
        || hz >= rate as f64 * 0.5
        || (cycles - cycles.round()).abs() > 1e-7
        || audio.iter().flatten().any(|x| !x.is_finite())
    {
        return Err(
            "Spectral probe requires finite audio and coherent whole cycles below Nyquist".into(),
        );
    }
    let mut real = [0.0; 2];
    let mut imaginary = [0.0; 2];
    for (i, frame) in audio.iter().enumerate() {
        let phase = TAU * hz * i as f64 / rate as f64;
        for c in 0..2 {
            real[c] += frame[c] as f64 * phase.cos();
            imaginary[c] += frame[c] as f64 * phase.sin();
        }
    }
    Ok((real
        .iter()
        .zip(imaginary)
        .map(|(r, i)| r * r + i * i)
        .sum::<f64>()
        * 0.5)
        .sqrt()
        * 2.0
        / audio.len() as f64)
}

#[derive(Serialize)]
pub struct ToneResult {
    pub fundamental_hz: f64,
    pub fundamental_peak_dbfs: f64,
    pub harmonic_amplitudes_dbfs: Vec<f64>,
    pub thd_db: Option<f64>,
}

pub fn harmonics(audio: &[Stereo], hz: f64, rate: u32) -> Result<ToneResult> {
    let fundamental = amplitude(audio, hz, rate)?;
    if fundamental < 1e-12 {
        return Err("No measurable fundamental for harmonic ratio".into());
    }
    let mut components = Vec::new();
    let mut distortion = 0.0;
    for order in 2..=10 {
        if hz * order as f64 >= rate as f64 * 0.5 {
            break;
        }
        let a = amplitude(audio, hz * order as f64, rate)?;
        distortion += a * a;
        components.push(dsp::db(a));
    }
    Ok(ToneResult {
        fundamental_hz: hz,
        fundamental_peak_dbfs: dsp::db(fundamental),
        thd_db: (!components.is_empty()).then(|| dsp::db(distortion.sqrt() / fundamental)),
        harmonic_amplitudes_dbfs: components,
    })
}
