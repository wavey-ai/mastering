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
    let mut envelopes = Envelopes::new(audio.len(), rate);
    envelopes.push(audio);
    envelopes.finish()
}

/// `analyze` of the frames of `audio`, read in blocks: a stored render
/// (D253).
pub(crate) fn analyze_read(
    audio: &mut impl crate::scratch::Frames,
    rate: u32,
) -> crate::audio_io::Result<Waveform> {
    let mut envelopes = Envelopes::new(audio.len(), rate);
    crate::scratch::for_each_block(audio, |block| {
        envelopes.push(block);
        Ok(())
    })?;
    Ok(envelopes.finish())
}

/// The envelopes of a signal of `frames` frames at `rate`, from its blocks
/// in order. The bins are those of `analyze` on the whole signal.
pub struct Envelopes {
    frames: usize,
    rate: u32,
    next: usize,
    peaks: [f32; BINS],
    energy: [f64; BINS],
    counts: [usize; BINS],
}

impl Envelopes {
    pub fn new(frames: usize, rate: u32) -> Self {
        Self {
            frames,
            rate,
            next: 0,
            peaks: [0.0; BINS],
            energy: [0.0; BINS],
            counts: [0; BINS],
        }
    }

    /// Add the next frames of the signal. The frames pushed stay within
    /// `frames`.
    pub fn push(&mut self, mut block: &[Stereo]) {
        // The frames of one bin in one run, with the sums of the bin in
        // locals: the bin of each frame took a 64-bit division, and the
        // sums a load and a store (D318). Frame `i` is in bin `k` when
        // `bin_start(k) <= i < bin_start(k + 1)`. The peak of a frame comes
        // first, so the running peak takes one `max` for each frame.
        while !block.is_empty() {
            let bin = bin_of(self.next, self.frames);
            let length = (bin_start(bin + 1, self.frames) - self.next).min(block.len());
            let (run, rest) = block.split_at(length);
            let (mut peak, mut energy) = (self.peaks[bin], self.energy[bin]);
            for &[left, right] in run {
                // The running peak is not NaN, so a comparison gives the
                // value of `max` (D332).
                peak = crate::dsp::larger(peak, left.abs().max(right.abs()));
                energy += (left as f64 * left as f64 + right as f64 * right as f64) * 0.5;
            }
            (self.peaks[bin], self.energy[bin]) = (peak, energy);
            self.counts[bin] += length;
            self.next += length;
            block = rest;
        }
    }

    pub fn finish(self) -> Waveform {
        let frames = self.frames;
        let bins = (0..BINS)
            .map(|index| Bin {
                start_sample: bin_start(index, frames),
                end_sample: bin_start(index + 1, frames),
                peak: self.peaks[index],
                rms: (self.energy[index] / self.counts[index].max(1) as f64).sqrt() as f32,
            })
            .collect();
        Waveform {
            schema: "mastering.waveform.1",
            sample_rate: self.rate as usize,
            input_frames: frames,
            amplitude_scale: "linear-full-scale",
            peak_definition: "maximum absolute left or right sample",
            rms_definition: "sqrt(mean((left^2 + right^2) / 2))",
            bins,
        }
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

    /// The runs of one bin equal the bins of each frame, bit for bit, for
    /// blocks of any length (D318).
    #[test]
    fn runs_of_one_bin_equal_the_bin_of_each_frame() {
        let mut state = 0x9e37_79b9u32;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as f32 / u32::MAX as f32 - 0.5
        };
        for frames in [1, 383, 384, 385, 48_000 * 3 + 17] {
            let audio: Vec<Stereo> = (0..frames).map(|_| [next(), next()]).collect();
            let mut each = Envelopes::new(frames, 48_000);
            for (i, &[left, right]) in audio.iter().enumerate() {
                let bin = bin_of(i, frames);
                each.peaks[bin] = each.peaks[bin].max(left.abs()).max(right.abs());
                each.energy[bin] += (left as f64 * left as f64 + right as f64 * right as f64) * 0.5;
                each.counts[bin] += 1;
            }
            let bits = |e: &Envelopes| -> Vec<u64> {
                e.peaks
                    .iter()
                    .map(|p| p.to_bits() as u64)
                    .chain(e.energy.iter().map(|x| x.to_bits()))
                    .chain(e.counts.iter().map(|&c| c as u64))
                    .collect()
            };
            for block in [1, 7, 4_096, frames] {
                let mut runs = Envelopes::new(frames, 48_000);
                audio.chunks(block).for_each(|b| runs.push(b));
                assert_eq!(runs.next, frames);
                assert_eq!(bits(&runs), bits(&each), "{frames} {block}");
            }
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
    fn envelopes_of_blocks_are_those_of_the_whole_signal() {
        let audio: Vec<Stereo> = (0..48_000 + 77)
            .map(|n| {
                let x = (n as f32 * 0.013).sin() * (1.0 + (n / 5000) as f32) / 12.0;
                [x, -0.5 * x]
            })
            .collect();
        let whole = serde_json::to_string(&analyze(&audio, 48000)).unwrap();
        for size in [1, 383, 4096, audio.len()] {
            let mut envelopes = Envelopes::new(audio.len(), 48000);
            audio.chunks(size).for_each(|block| envelopes.push(block));
            assert_eq!(serde_json::to_string(&envelopes.finish()).unwrap(), whole);
        }
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
