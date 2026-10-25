//! Absolute stereo peak and RMS envelopes for a shared-scale comparison view.
//! These are source/output measurements, not a playback gain or normalization.

use crate::audio_io::Stereo;
use serde::Serialize;

pub const BINS: usize = 384;

#[derive(Clone, Serialize)]
pub struct Bin {
    pub start_sample: usize,
    pub end_sample: usize,
    pub peak: f32,
    pub rms: f32,
}

#[derive(Clone, Serialize)]
pub struct Waveform {
    pub schema: &'static str,
    pub sample_rate: usize,
    pub input_frames: usize,
    pub amplitude_scale: &'static str,
    pub peak_definition: &'static str,
    pub rms_definition: &'static str,
    pub bins: Vec<Bin>,
}

/// Bin of frame `index` among `BINS` equal parts of `frames`. The product is
/// formed in 64 bits: on wasm32 it passes 32 bits from 233 s of 48 kHz frames,
/// and from 58 s of 4x frames.
pub(crate) fn bin_of(index: usize, frames: usize) -> usize {
    (index as u64 * BINS as u64 / frames as u64) as usize
}

/// First frame of bin `index` among `BINS` equal parts of `frames`.
pub(crate) fn bin_start(index: usize, frames: usize) -> usize {
    (index as u64 * frames as u64).div_ceil(BINS as u64) as usize
}

/// The envelopes of `audio` at `rate`.
pub fn analyze(audio: &[Stereo], rate: u32) -> Waveform {
    let frames = audio.len();
    let mut peaks = [0.0f32; BINS];
    let mut energy = [0.0f64; BINS];
    let mut counts = [0usize; BINS];
    for (index, &[left, right]) in audio.iter().enumerate() {
        let bin = bin_of(index, frames);
        peaks[bin] = peaks[bin].max(left.abs()).max(right.abs());
        energy[bin] += (left as f64 * left as f64 + right as f64 * right as f64) * 0.5;
        counts[bin] += 1;
    }
    let bins = (0..BINS)
        .map(|index| Bin {
            start_sample: bin_start(index, frames),
            end_sample: bin_start(index + 1, frames),
            peak: peaks[index],
            rms: (energy[index] / counts[index].max(1) as f64).sqrt() as f32,
        })
        .collect();
    Waveform {
        schema: "mastering.waveform.1",
        sample_rate: rate as usize,
        input_frames: frames,
        amplitude_scale: "linear-full-scale",
        peak_definition: "maximum absolute left or right sample",
        rms_definition: "sqrt(mean((left^2 + right^2) / 2))",
        bins,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_keep_one_absolute_scale() {
        let source = analyze(&vec![[0.2, -0.2]; 960], 48000);
        let master = analyze(&vec![[0.8, -0.8]; 960], 48000);
        assert_eq!(source.bins.len(), BINS);
        assert_eq!(master.bins.len(), BINS);
        for (original, mastered) in source.bins.iter().zip(&master.bins) {
            assert!((mastered.peak / original.peak - 4.0).abs() < 1e-5);
            assert!((mastered.rms / original.rms - 4.0).abs() < 1e-5);
        }
    }

    #[test]
    fn bins_of_a_long_render_use_products_above_32_bits() {
        // Five minutes of 4x frames: a 32-bit product wraps after 58 s and
        // puts the rest of the song in the first bins again.
        let frames = 4 * 48_000 * 300;
        assert_eq!(bin_of(frames - 1, frames), BINS - 1);
        assert_eq!(bin_of(frames / 2, frames), BINS / 2);
        assert_eq!(bin_start(BINS, frames), frames);
        assert_eq!(bin_start(BINS / 2, frames), frames / 2);
    }

    #[test]
    fn stereo_rms_and_peak_have_distinct_meanings() {
        let waveform = analyze(&vec![[1.0, 0.0]; 768], 48000);
        assert_eq!(waveform.bins[0].peak, 1.0);
        assert!((waveform.bins[0].rms as f64 - 2.0f64.sqrt().recip()).abs() < 1e-6);
        assert_eq!(waveform.bins[0].start_sample, 0);
        assert_eq!(waveform.bins.last().unwrap().end_sample, 768);
    }
}
