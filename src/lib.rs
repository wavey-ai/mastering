pub mod adjust;
pub mod album;
#[cfg(feature = "alloc-stats")]
pub mod alloc_stats;
pub mod analysis;
pub mod audibility;
pub mod audio_io;
pub mod auditory;
pub mod bench;
pub mod broad_tone;
pub mod capture;
pub mod character;
pub mod compressor;
pub mod deesser;
pub mod delivery;
pub mod distributed;
pub mod dsp;
pub mod excerpt;
pub mod finishing;
pub mod fir;
pub mod foundation;
pub mod imaging;
pub mod low_end;
pub mod low_mid;
pub mod masking;
pub mod open;
pub mod pipeline;
mod prefix_stream;
pub mod progress;
pub mod refinement;
mod render_blocks;
pub mod resample;
pub mod scratch;
pub mod search;
pub mod selection;
pub mod set;
pub mod source_condition;
pub mod source_treatments;
mod static_text;
pub mod stem_evidence;
pub mod stereo;
pub mod stereo_phase;
pub mod stft;
pub mod treatment;
pub mod treatment_render;
pub mod version_names;
pub mod waveform;
pub mod width;

use audio_io::Stereo;
use dsp::db;
use mel_spec::mel::SparseMelFilterbank;
use rustfft::num_complex::Complex;
use serde::Serialize;
use stft::StereoStft;

#[derive(Clone, Serialize)]
pub struct Metrics {
    pub integrated_lufs: Option<f64>,
    pub sample_peak_dbfs: f64,
    pub rms_dbfs: f64,
    pub crest_db: f64,
    pub side_mid_db: f64,
    pub lr_cosine: f64,
    pub full_scale_channel_samples: usize,
}

pub fn metrics(audio: &[Stereo], rate: u32) -> Metrics {
    let mut scan = MetricsScan::new(rate);
    scan.push(audio);
    scan.finish()
}

/// `metrics` of the frames of `audio`, read in blocks (D255).
pub(crate) fn metrics_read(
    audio: &mut impl scratch::Frames,
    rate: u32,
) -> audio_io::Result<Metrics> {
    let mut scan = MetricsScan::new(rate);
    scratch::for_each_block(audio, |block| {
        scan.push(block);
        Ok(())
    })?;
    Ok(scan.finish())
}

/// `metrics` of a signal whose frames arrive in blocks of any length.
pub(crate) struct MetricsScan {
    /// The peak, then the sums of the energy, the left, right, cross, mid
    /// and side powers.
    sums: [f64; 7],
    clipped: usize,
    frames: usize,
    meter: dsp::LoudnessMeter,
}

impl MetricsScan {
    pub(crate) fn new(rate: u32) -> Self {
        Self {
            sums: [0.0; 7],
            clipped: 0,
            frames: 0,
            meter: dsp::LoudnessMeter::new(rate),
        }
    }

    /// Add the next frames of the signal.
    pub(crate) fn push(&mut self, audio: &[Stereo]) {
        // In locals, the sums stay in registers.
        let [mut peak, mut energy, mut left, mut right, mut cross, mut mid, mut side] = self.sums;
        let mut clipped = self.clipped;
        for frame in audio {
            let [l, r] = frame.map(|v| v as f64);
            // The peak of the frame first, with `max`, which passes a NaN by
            // in any order (D319). The running peak is not NaN, so a
            // comparison gives the value of `max` (D332).
            peak = dsp::larger(peak, l.abs().max(r.abs()));
            energy += l * l + r * r;
            left += l * l;
            right += r * r;
            cross += l * r;
            mid += ((l + r) * 0.5).powi(2);
            side += ((l - r) * 0.5).powi(2);
            clipped += frame
                .iter()
                .filter(|&&v| v.abs() >= 1.0 - 2f32.powi(-23))
                .count();
        }
        self.sums = [peak, energy, left, right, cross, mid, side];
        self.clipped = clipped;
        self.frames += audio.len();
        self.meter.push(audio);
    }

    pub(crate) fn finish(self) -> Metrics {
        let [peak, energy, left, right, cross, mid, side] = self.sums;
        let rms = db((energy / (self.frames.max(1) * 2) as f64).sqrt());
        Metrics {
            integrated_lufs: self.meter.finish(),
            sample_peak_dbfs: db(peak),
            rms_dbfs: rms,
            crest_db: db(peak) - rms,
            side_mid_db: db((side.max(1e-30) / mid.max(1e-30)).sqrt()),
            lr_cosine: cross / (left * right).sqrt().max(1e-30),
            full_scale_channel_samples: self.clipped,
        }
    }
}

#[derive(Clone, Serialize)]
pub struct Spectrum {
    /// The rate of the measured audio, in Hz.
    pub sample_rate: u32,
    pub centers_hz: Vec<f64>,
    pub band_power: Vec<f64>,
    pub mid_power: Vec<f64>,
    pub side_power: Vec<f64>,
    pub mel_power: Vec<f64>,
    pub analyzed_frames: usize,
}

/// The long-term spectrum of `audio` at `rate`.
pub fn spectrum(audio: &[Stereo], rate: u32) -> Spectrum {
    spectrum_read(&mut scratch::Slice(audio), rate).expect("frames of a slice")
}

/// Frames of each transform of the spectrum.
pub(crate) const SPECTRUM_FFT: usize = 8192;
/// Frames between the transforms of the spectrum.
pub(crate) const SPECTRUM_HOP: usize = 2048;

/// `spectrum` of the frames of `audio`, read one batch of hops at a time.
pub(crate) fn spectrum_read(
    audio: &mut impl scratch::Frames,
    rate: u32,
) -> audio_io::Result<Spectrum> {
    let (nfft, hop) = (SPECTRUM_FFT, SPECTRUM_HOP);
    let mut sums = SpectrumSums::new();
    // A spectrogram holds only its last `nfft` samples. Each range of hops
    // transforms on its own core after the `nfft / hop - 1` hops before it,
    // which yield no frame; the sums take the frames in order.
    let hops = audio.len() / hop;
    let warm = nfft / hop - 1;
    // A batch has one range of 64 hops for each core of `dsp::chunked`, and
    // each range holds 8.4 MB of spectra (D240).
    let batch_hops = 64 * dsp::threads().min(8);
    let mut buffer = Vec::new();
    for batch in (0..hops).step_by(batch_hops) {
        let batch_end = (batch + batch_hops).min(hops);
        // The hops of the batch and the warm-up hops before it.
        let first = batch.saturating_sub(warm) * hop;
        let span = audio.window(first, batch_end * hop - first, &mut buffer)?;
        let spectra = dsp::chunked(batch_end - batch, 64, |range| {
            let range = batch + range.start..batch + range.end;
            let mut stft = StereoStft::new(nfft, hop);
            let mut out = Vec::new();
            for index in range.start.saturating_sub(warm)..range.end {
                let chunk = &span[index * hop - first..(index + 1) * hop - first];
                if let Some((a, b)) = stft.add_with(hop, |i| chunk[i]) {
                    if index >= range.start {
                        out.push(SpectrumSums::terms(a, b));
                    }
                }
            }
            out
        });
        for terms in spectra.into_iter().flatten() {
            sums.add(&terms);
        }
    }
    Ok(sums.finish(rate))
}

/// The sums of the long-term spectrum over its frames, in the order of the
/// frames.
pub(crate) struct SpectrumSums {
    power: Vec<f64>,
    mid: Vec<f64>,
    side: Vec<f64>,
    frames: usize,
}

impl SpectrumSums {
    pub(crate) fn new() -> Self {
        let power = vec![0.0; SPECTRUM_FFT / 2 + 1];
        Self {
            mid: power.clone(),
            side: power.clone(),
            power,
            frames: 0,
        }
    }
    /// The power, the mid power and the side power of each bin of the left
    /// spectrum `a` and the right spectrum `b` of one frame.
    pub(crate) fn terms(a: &[Complex<f64>], b: &[Complex<f64>]) -> Vec<[f64; 3]> {
        a.iter()
            .zip(b)
            .map(|(&a, &b)| {
                [
                    (a.norm_sqr() + b.norm_sqr()) * 0.5,
                    ((a + b) * 0.5).norm_sqr(),
                    ((a - b) * 0.5).norm_sqr(),
                ]
            })
            .collect()
    }
    /// Adds the `terms` of the next frame.
    pub(crate) fn add(&mut self, terms: &[[f64; 3]]) {
        for (k, &[power, mid, side]) in terms.iter().enumerate() {
            self.power[k] += power;
            self.mid[k] += mid;
            self.side[k] += side;
        }
        self.frames += 1;
    }
    /// The spectrum of the frames at `rate`.
    pub(crate) fn finish(self, rate: u32) -> Spectrum {
        let nfft = SPECTRUM_FFT;
        let Self {
            mut power,
            mut mid,
            mut side,
            frames,
        } = self;
        for array in [&mut power, &mut mid, &mut side] {
            for value in array {
                *value /= frames.max(1) as f64;
            }
        }
        let filters = SparseMelFilterbank::from_mel(
            rate as f64,
            nfft,
            128,
            Some(20.0),
            Some(20000.0),
            false,
            true,
        );
        let mut mel_power = vec![0.0; 128];
        filters.project_power_f64(&power, &mut mel_power);
        let edges: Vec<f64> = (0..=24)
            .map(|i| 30.0 * 600f64.powf(i as f64 / 24.0))
            .collect();
        let integrate = |values: &[f64]| -> Vec<f64> {
            edges
                .windows(2)
                .map(|bounds| {
                    values
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| {
                            let hz = *i as f64 * rate as f64 / nfft as f64;
                            hz >= bounds[0] && hz < bounds[1]
                        })
                        .map(|(_, &v)| v)
                        .sum()
                })
                .collect()
        };
        Spectrum {
            sample_rate: rate,
            centers_hz: edges.windows(2).map(|e| (e[0] * e[1]).sqrt()).collect(),
            band_power: integrate(&power),
            mid_power: integrate(&mid),
            side_power: integrate(&side),
            mel_power,
            analyzed_frames: frames,
        }
    }
}

#[cfg(test)]
mod metrics_tests {
    use super::*;

    /// The peak of each frame first gives the sums of two `max` for each
    /// frame, bit for bit, with NaN and silent frames among the frames
    /// (D319).
    #[test]
    fn the_peak_of_each_frame_first_gives_the_same_sums() {
        let mut state = 0x2545_f491u32;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as f32 / u32::MAX as f32 * 2.2 - 1.1
        };
        let mut audio: Vec<Stereo> = (0..48_000 * 4 + 7).map(|_| [next(), next()]).collect();
        audio[100] = [f32::NAN, 0.5];
        audio[200] = [0.25, f32::NAN];
        audio[300] = [0.0, -0.0];
        let mut peak = 0.0f64;
        for frame in &audio {
            let [l, r] = frame.map(|v| v as f64);
            peak = peak.max(l.abs()).max(r.abs());
        }
        let mut scan = MetricsScan::new(48_000);
        audio.chunks(4_099).for_each(|block| scan.push(block));
        assert_eq!(scan.sums[0].to_bits(), peak.to_bits());
    }
}

#[cfg(test)]
mod spectrum_tests {
    use super::*;
    // The spectrum before its hops transformed on several cores.
    fn spectrum_sequential(audio: &[Stereo]) -> Spectrum {
        let nfft = 8192;
        let hop = 2048;
        let mut stft = StereoStft::new(nfft, hop);
        let mut power = vec![0.0; nfft / 2 + 1];
        let mut mid = power.clone();
        let mut side = power.clone();
        let mut frames = 0;
        for chunk in audio.chunks_exact(hop) {
            if let Some((a, b)) = stft.add_with(hop, |i| chunk[i]) {
                for k in 0..power.len() {
                    power[k] += (a[k].norm_sqr() + b[k].norm_sqr()) * 0.5;
                    mid[k] += ((a[k] + b[k]) * 0.5).norm_sqr();
                    side[k] += ((a[k] - b[k]) * 0.5).norm_sqr();
                }
                frames += 1;
            }
        }
        for array in [&mut power, &mut mid, &mut side] {
            for value in array {
                *value /= frames.max(1) as f64;
            }
        }
        let filters = SparseMelFilterbank::from_mel(
            48000.0,
            nfft,
            128,
            Some(20.0),
            Some(20000.0),
            false,
            true,
        );
        let mut mel_power = vec![0.0; 128];
        filters.project_power_f64(&power, &mut mel_power);
        let edges: Vec<f64> = (0..=24)
            .map(|i| 30.0 * 600f64.powf(i as f64 / 24.0))
            .collect();
        let integrate = |values: &[f64]| -> Vec<f64> {
            edges
                .windows(2)
                .map(|bounds| {
                    values
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| {
                            let hz = *i as f64 * 48000.0 / nfft as f64;
                            hz >= bounds[0] && hz < bounds[1]
                        })
                        .map(|(_, &v)| v)
                        .sum()
                })
                .collect()
        };
        Spectrum {
            sample_rate: 48000,
            centers_hz: edges.windows(2).map(|e| (e[0] * e[1]).sqrt()).collect(),
            band_power: integrate(&power),
            mid_power: integrate(&mid),
            side_power: integrate(&side),
            mel_power,
            analyzed_frames: frames,
        }
    }

    #[test]
    fn hops_on_several_cores_match_the_sequential_spectrum() {
        let mut state = 0x9e37_79b9u32;
        for len in [5_000, 48_000 * 3 + 1_234, 48_000 * 11] {
            let audio: Vec<Stereo> = (0..len)
                .map(|_| {
                    let mut next = || {
                        state ^= state << 13;
                        state ^= state >> 17;
                        state ^= state << 5;
                        state as f32 / u32::MAX as f32 - 0.5
                    };
                    [next(), 0.25 * next()]
                })
                .collect();
            assert_eq!(
                serde_json::to_string(&spectrum(&audio, 48000)).unwrap(),
                serde_json::to_string(&spectrum_sequential(&audio)).unwrap(),
                "{len}"
            );
            // A stored recording gives the spectrum of the recording (D247).
            let mut stored = scratch::Pcm::temporary_of(&audio).unwrap();
            assert_eq!(
                serde_json::to_string(&spectrum_read(&mut stored, 48000).unwrap()).unwrap(),
                serde_json::to_string(&spectrum(&audio, 48000)).unwrap(),
                "{len}"
            );
        }
    }
}
