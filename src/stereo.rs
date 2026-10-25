use crate::audio_io::{Result, Stereo};
use crate::stft::StereoStft;
use serde::Serialize;
use std::f64::consts::PI;

pub const FFT_SIZE: usize = 8192;
pub const HOP_SIZE: usize = 2048;
pub const BAND_EDGES_HZ: [f64; 7] = [20.0, 120.0, 500.0, 2000.0, 6000.0, 12000.0, 20000.0];
const BANDS: usize = BAND_EDGES_HZ.len() - 1;
const ABSOLUTE_FLOOR: f64 = 1e-12;
const RELATIVE_FLOOR: f64 = 1e-6;
/// A window whose correlation is under this counts as negative: its
/// channels cancel in part when summed.
pub const NEGATIVE_CORRELATION: f64 = -0.25;
const DEEP_MONO_LOSS: f64 = 0.01;
const MIN_COHERENCE_FRAMES: usize = 4;
const MIN_PHASE_RESULTANT: f64 = 0.1;

#[derive(Clone, Debug, Serialize)]
pub struct BandMetrics {
    pub active: bool,
    pub left_present: bool,
    pub right_present: bool,
    pub mid_present: bool,
    pub side_present: bool,
    pub left_power: f64,
    pub right_power: f64,
    pub mid_power: f64,
    pub side_power: f64,
    /// Left plus right power weighted by `broad_tone::equal_loudness_weight`.
    pub weighted_power: f64,
    pub side_fraction: Option<f64>,
    pub side_mid_db: Option<f64>,
    pub right_left_db: Option<f64>,
    pub correlation: Option<f64>,
    pub mono_fraction: Option<f64>,
    pub mono_fold_down_db: Option<f64>,
    pub coherence: Option<f64>,
    pub phase_resultant: Option<f64>,
    pub cross_phase_degrees: Option<f64>,
}

#[derive(Clone, Debug, Serialize)]
pub struct StereoWindow {
    pub start_sample: usize,
    pub end_sample: usize,
    pub support_start_sample: usize,
    pub support_end_sample: usize,
    pub spectral_frames: usize,
    pub padded_frames: usize,
    pub bands: Vec<BandMetrics>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Distribution {
    pub count: usize,
    pub min: f64,
    pub p10: f64,
    pub p50: f64,
    pub p90: f64,
    pub max: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct BandSummary {
    pub active_windows: usize,
    pub active_window_seconds: f64,
    pub correlation: Option<Distribution>,
    pub side_fraction: Option<Distribution>,
    pub coherence: Option<Distribution>,
    pub mono_fold_down_db: Option<Distribution>,
    pub negative_correlation_window_seconds: f64,
    pub longest_negative_correlation_window_run_seconds: f64,
    pub deep_mono_loss_window_seconds: f64,
    pub worst_correlation_window_start_sample: Option<usize>,
}

#[derive(Clone, Debug, Serialize)]
pub struct StereoAnalysis {
    pub schema: &'static str,
    pub sample_rate: usize,
    pub input_frames: usize,
    pub fft_size: usize,
    pub hop_size: usize,
    pub band_edges_hz: [f64; 7],
    pub absolute_power_floor: f64,
    pub relative_power_floor: f64,
    pub activity_power_floor: f64,
    pub minimum_coherence_frames: usize,
    pub minimum_phase_resultant: f64,
    pub negative_correlation_threshold: f64,
    pub deep_mono_loss_fraction: f64,
    pub short_window_samples: usize,
    pub long_window_samples: usize,
    pub whole: StereoWindow,
    pub short_windows: Vec<StereoWindow>,
    pub long_windows: Vec<StereoWindow>,
    pub short_summary: Vec<BandSummary>,
    pub long_summary: Vec<BandSummary>,
}

#[derive(Clone, Copy, Default)]
struct Bin {
    left: f64,
    right: f64,
    cross_re: f64,
    cross_im: f64,
}

#[derive(Default)]
struct RawBand {
    left: f64,
    right: f64,
    weighted: f64,
    cross_re: f64,
    cross_im: f64,
    cross_magnitude_sum: f64,
    coherence: Option<f64>,
}

struct RawWindow {
    start: usize,
    end: usize,
    support_start: usize,
    support_end: usize,
    frames: usize,
    padded_frames: usize,
    bands: Vec<RawBand>,
}

/// `broad_tone::equal_loudness_weight` of each FFT bin at `rate`.
fn bin_weights(rate: usize) -> Vec<f64> {
    (0..=FFT_SIZE / 2)
        .map(|i| crate::broad_tone::equal_loudness_weight(i as f64 * rate as f64 / FFT_SIZE as f64))
        .collect()
}

struct Accumulator {
    bins: Vec<Bin>,
    frames: usize,
    padded_frames: usize,
    support_start: usize,
    support_end: usize,
}

impl Accumulator {
    fn new() -> Self {
        Self {
            bins: vec![Bin::default(); FFT_SIZE / 2 + 1],
            frames: 0,
            padded_frames: 0,
            support_start: usize::MAX,
            support_end: 0,
        }
    }

    fn add(&mut self, frame: &[Bin], start: usize, end: usize, padded: bool) {
        for (sum, bin) in self.bins.iter_mut().zip(frame) {
            sum.left += bin.left;
            sum.right += bin.right;
            sum.cross_re += bin.cross_re;
            sum.cross_im += bin.cross_im;
        }
        self.frames += 1;
        self.padded_frames += usize::from(padded);
        self.support_start = self.support_start.min(start);
        self.support_end = end;
    }

    /// `weights` are the `bin_weights` of `rate`.
    fn finish(&self, start: usize, end: usize, weights: &[f64], rate: usize) -> RawWindow {
        let mut bands = Vec::with_capacity(BANDS);
        for edges in BAND_EDGES_HZ.windows(2) {
            let low = (edges[0] * FFT_SIZE as f64 / rate as f64).ceil() as usize;
            let high = (edges[1] * FFT_SIZE as f64 / rate as f64).ceil() as usize;
            let mut band = RawBand::default();
            let (mut weighted_coherence, mut weight_sum) = (0.0, 0.0);
            let weights = &weights[low..high];
            for (b, w) in self.bins[low..high].iter().zip(weights) {
                band.left += b.left;
                band.right += b.right;
                band.weighted += (b.left + b.right) * w;
                band.cross_re += b.cross_re;
                band.cross_im += b.cross_im;
                band.cross_magnitude_sum += b.cross_re.hypot(b.cross_im);
                let weight = (b.left * b.right).sqrt();
                if weight > 0.0 {
                    let coherence = ((b.cross_re.powi(2) + b.cross_im.powi(2))
                        / (b.left * b.right))
                        .clamp(0.0, 1.0);
                    weighted_coherence += weight * coherence;
                    weight_sum += weight;
                }
            }
            if self.frames >= MIN_COHERENCE_FRAMES && weight_sum > 0.0 {
                band.coherence = Some((weighted_coherence / weight_sum).clamp(0.0, 1.0));
            }
            let count = self.frames.max(1) as f64;
            band.left /= count;
            band.right /= count;
            band.weighted /= count;
            band.cross_re /= count;
            band.cross_im /= count;
            band.cross_magnitude_sum /= count;
            bands.push(band);
        }
        RawWindow {
            start,
            end,
            support_start: self.support_start.min(end),
            support_end: self.support_end,
            frames: self.frames,
            padded_frames: self.padded_frames,
            bands,
        }
    }
}

fn measure(raw: RawWindow, floor: f64) -> StereoWindow {
    let bands = raw
        .bands
        .into_iter()
        .map(|b| {
            let stereo = (b.left + b.right) * 0.5;
            let mid = ((b.left + b.right + 2.0 * b.cross_re) * 0.25).max(0.0);
            let side = ((b.left + b.right - 2.0 * b.cross_re) * 0.25).max(0.0);
            let active = stereo >= floor;
            let left_present = b.left >= floor;
            let right_present = b.right >= floor;
            let mid_present = mid >= floor;
            let side_present = side >= floor;
            let paired = active && left_present && right_present;
            let coherence = b.coherence.filter(|_| paired);
            let resultant = if paired && b.cross_magnitude_sum > floor {
                Some((b.cross_re.hypot(b.cross_im) / b.cross_magnitude_sum).clamp(0.0, 1.0))
            } else {
                None
            };
            BandMetrics {
                active,
                left_present,
                right_present,
                mid_present,
                side_present,
                left_power: b.left,
                right_power: b.right,
                mid_power: mid,
                side_power: side,
                weighted_power: b.weighted,
                side_fraction: active.then(|| (side / stereo).clamp(0.0, 1.0)),
                side_mid_db: (active && mid_present && side_present)
                    .then(|| 10.0 * (side / mid).log10()),
                right_left_db: paired.then(|| 10.0 * (b.right / b.left).log10()),
                correlation: paired
                    .then(|| (b.cross_re / (b.left * b.right).sqrt()).clamp(-1.0, 1.0)),
                mono_fraction: active.then(|| (mid / stereo).clamp(0.0, 1.0)),
                mono_fold_down_db: (active && mid_present)
                    .then(|| 10.0 * (mid / stereo).min(1.0).log10()),
                coherence,
                phase_resultant: resultant,
                cross_phase_degrees: resultant
                    .filter(|r| *r >= MIN_PHASE_RESULTANT && coherence.is_some())
                    .map(|_| b.cross_im.atan2(b.cross_re).to_degrees()),
            }
        })
        .collect();
    StereoWindow {
        start_sample: raw.start,
        end_sample: raw.end,
        support_start_sample: raw.support_start,
        support_end_sample: raw.support_end,
        spectral_frames: raw.frames,
        padded_frames: raw.padded_frames,
        bands,
    }
}

fn distribution(mut values: Vec<f64>) -> Option<Distribution> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let quantile = |p: f64| {
        let index = p * (values.len() - 1) as f64;
        let low = index.floor() as usize;
        let high = index.ceil() as usize;
        values[low] + (values[high] - values[low]) * index.fract()
    };
    Some(Distribution {
        count: values.len(),
        min: values[0],
        p10: quantile(0.1),
        p50: quantile(0.5),
        p90: quantile(0.9),
        max: values[values.len() - 1],
    })
}

fn summarize(windows: &[StereoWindow], rate: usize) -> Vec<BandSummary> {
    (0..BANDS)
        .map(|band| {
            let mut summary = BandSummary {
                active_windows: 0,
                active_window_seconds: 0.0,
                correlation: None,
                side_fraction: None,
                coherence: None,
                mono_fold_down_db: None,
                negative_correlation_window_seconds: 0.0,
                longest_negative_correlation_window_run_seconds: 0.0,
                deep_mono_loss_window_seconds: 0.0,
                worst_correlation_window_start_sample: None,
            };
            let (mut run, mut worst) = (0.0f64, f64::INFINITY);
            for w in windows {
                let b = &w.bands[band];
                let seconds = (w.end_sample - w.start_sample) as f64 / rate as f64;
                if b.active {
                    summary.active_windows += 1;
                    summary.active_window_seconds += seconds;
                }
                if let Some(correlation) = b.correlation {
                    if correlation < worst {
                        worst = correlation;
                        summary.worst_correlation_window_start_sample = Some(w.start_sample);
                    }
                }
                if b.correlation.is_some_and(|c| c < NEGATIVE_CORRELATION) {
                    run += seconds;
                    summary.negative_correlation_window_seconds += seconds;
                    summary.longest_negative_correlation_window_run_seconds = summary
                        .longest_negative_correlation_window_run_seconds
                        .max(run);
                } else {
                    run = 0.0;
                }
                if b.mono_fraction.is_some_and(|m| m < DEEP_MONO_LOSS) {
                    summary.deep_mono_loss_window_seconds += seconds;
                }
            }
            let collect = |get: fn(&BandMetrics) -> Option<f64>| {
                distribution(windows.iter().filter_map(|w| get(&w.bands[band])).collect())
            };
            summary.correlation = collect(|b| b.correlation);
            summary.side_fraction = collect(|b| b.side_fraction);
            summary.coherence = collect(|b| b.coherence);
            summary.mono_fold_down_db = collect(|b| b.mono_fold_down_db);
            summary
        })
        .collect()
}

/// The stereo analysis of `audio` at `rate`.
pub fn analyze(audio: &[Stereo], rate: u32) -> Result<StereoAnalysis> {
    if audio.is_empty() || audio.iter().flatten().any(|v| !v.is_finite()) {
        return Err("Stereo analysis requires nonempty finite stereo samples".into());
    }
    let rate = rate as usize;
    let weights = bin_weights(rate);
    let short_size = rate / 2;
    let long_size = rate * 3;
    let window_power: Vec<_> = (0..FFT_SIZE)
        .map(|i| (0.5 * (1.0 - (2.0 * PI * i as f64 / FFT_SIZE as f64).cos())).powi(2))
        .collect();
    let (mut whole, mut short, mut long) =
        (Accumulator::new(), Accumulator::new(), Accumulator::new());
    let (mut short_index, mut long_index) = (0, 0);
    let (mut short_raw, mut long_raw) = (Vec::new(), Vec::new());
    let centers: Vec<usize> = (0..audio.len())
        .step_by(HOP_SIZE)
        .chain(std::iter::once(audio.len() - 1).filter(|last| last % HOP_SIZE != 0))
        .collect();
    // Each frame is one whole FFT window, so frames transform on the
    // available cores; the accumulators take them in order. The browser
    // engine has one thread: a batch of one range holds 4 MB of frames, not
    // 34 MB.
    const BATCH: usize = if cfg!(target_arch = "wasm32") {
        64
    } else {
        512
    };
    for batch in centers.chunks(BATCH) {
        let frames = crate::dsp::chunked(batch.len(), 64, |range| {
            let mut stft = StereoStft::new(FFT_SIZE, FFT_SIZE);
            let (mut left, mut right) = (vec![0.0; FFT_SIZE], vec![0.0; FFT_SIZE]);
            batch[range]
                .iter()
                .map(|&center| {
                    left.fill(0.0);
                    right.fill(0.0);
                    let start = center.saturating_sub(FFT_SIZE / 2);
                    let end = (center + FFT_SIZE / 2).min(audio.len());
                    let offset = (FFT_SIZE / 2).saturating_sub(center);
                    let mut available_window_power = 0.0;
                    for (i, sample) in audio[start..end].iter().enumerate() {
                        left[offset + i] = sample[0];
                        right[offset + i] = sample[1];
                        available_window_power += window_power[offset + i];
                    }
                    let (a, b) = stft
                        .add_with(FFT_SIZE, |i| [left[i], right[i]])
                        .expect("Complete FFT input");
                    // Only positive, non-Nyquist bins enter the analysis bands.
                    let scale = 2.0 / (FFT_SIZE as f64 * available_window_power);
                    let frame: Vec<Bin> = (0..FFT_SIZE / 2 + 1)
                        .map(|k| {
                            let cross = a[k] * b[k].conj();
                            Bin {
                                left: a[k].norm_sqr() * scale,
                                right: b[k].norm_sqr() * scale,
                                cross_re: cross.re * scale,
                                cross_im: cross.im * scale,
                            }
                        })
                        .collect();
                    (start, end, frame)
                })
                .collect::<Vec<_>>()
        });
        for (&center, (start, end, frame)) in batch.iter().zip(frames.into_iter().flatten()) {
            let next_short = center / short_size;
            if next_short != short_index {
                short_raw.push(short.finish(
                    short_index * short_size,
                    next_short * short_size,
                    &weights,
                    rate,
                ));
                short = Accumulator::new();
                short_index = next_short;
            }
            let next_long = center / long_size;
            if next_long != long_index {
                long_raw.push(long.finish(
                    long_index * long_size,
                    next_long * long_size,
                    &weights,
                    rate,
                ));
                long = Accumulator::new();
                long_index = next_long;
            }
            let padded = end - start < FFT_SIZE;
            whole.add(&frame, start, end, padded);
            short.add(&frame, start, end, padded);
            long.add(&frame, start, end, padded);
        }
    }
    short_raw.push(short.finish(short_index * short_size, audio.len(), &weights, rate));
    long_raw.push(long.finish(long_index * long_size, audio.len(), &weights, rate));
    let peak_power = short_raw
        .iter()
        .map(|w| {
            w.bands
                .iter()
                .map(|b| (b.left + b.right) * 0.5)
                .sum::<f64>()
        })
        .fold(0.0f64, f64::max);
    let floor = ABSOLUTE_FLOOR.max(peak_power * RELATIVE_FLOOR);
    let short_windows: Vec<_> = short_raw.into_iter().map(|w| measure(w, floor)).collect();
    let long_windows: Vec<_> = long_raw.into_iter().map(|w| measure(w, floor)).collect();
    Ok(StereoAnalysis {
        schema: "mastering.stereo.1",
        sample_rate: rate,
        input_frames: audio.len(),
        fft_size: FFT_SIZE,
        hop_size: HOP_SIZE,
        band_edges_hz: BAND_EDGES_HZ,
        absolute_power_floor: ABSOLUTE_FLOOR,
        relative_power_floor: RELATIVE_FLOOR,
        activity_power_floor: floor,
        minimum_coherence_frames: MIN_COHERENCE_FRAMES,
        minimum_phase_resultant: MIN_PHASE_RESULTANT,
        negative_correlation_threshold: NEGATIVE_CORRELATION,
        deep_mono_loss_fraction: DEEP_MONO_LOSS,
        short_window_samples: short_size,
        long_window_samples: long_size,
        whole: measure(whole.finish(0, audio.len(), &weights, rate), floor),
        short_summary: summarize(&short_windows, rate),
        long_summary: summarize(&long_windows, rate),
        short_windows,
        long_windows,
    })
}

#[cfg(test)]
mod parallel_tests {
    use super::*;

    // The analysis before its frames transformed on several cores.
    fn analyze_sequential(audio: &[Stereo]) -> Result<StereoAnalysis> {
        if audio.is_empty() || audio.iter().flatten().any(|v| !v.is_finite()) {
            return Err("Stereo analysis requires nonempty finite 48 kHz stereo samples".into());
        }
        let rate = 48000;
        let weights = bin_weights(rate);
        let short_size = rate / 2;
        let long_size = rate * 3;
        let mut stft = StereoStft::new(FFT_SIZE, FFT_SIZE);
        let window_power: Vec<_> = (0..FFT_SIZE)
            .map(|i| (0.5 * (1.0 - (2.0 * PI * i as f64 / FFT_SIZE as f64).cos())).powi(2))
            .collect();
        let (mut left, mut right) = (vec![0.0; FFT_SIZE], vec![0.0; FFT_SIZE]);
        let mut frame = vec![Bin::default(); FFT_SIZE / 2 + 1];
        let (mut whole, mut short, mut long) =
            (Accumulator::new(), Accumulator::new(), Accumulator::new());
        let (mut short_index, mut long_index) = (0, 0);
        let (mut short_raw, mut long_raw) = (Vec::new(), Vec::new());
        let centers = (0..audio.len())
            .step_by(HOP_SIZE)
            .chain(std::iter::once(audio.len() - 1).filter(|last| last % HOP_SIZE != 0));
        for center in centers {
            let next_short = center / short_size;
            if next_short != short_index {
                short_raw.push(short.finish(
                    short_index * short_size,
                    next_short * short_size,
                    &weights,
                    rate,
                ));
                short = Accumulator::new();
                short_index = next_short;
            }
            let next_long = center / long_size;
            if next_long != long_index {
                long_raw.push(long.finish(
                    long_index * long_size,
                    next_long * long_size,
                    &weights,
                    rate,
                ));
                long = Accumulator::new();
                long_index = next_long;
            }
            left.fill(0.0);
            right.fill(0.0);
            let start = center.saturating_sub(FFT_SIZE / 2);
            let end = (center + FFT_SIZE / 2).min(audio.len());
            let offset = (FFT_SIZE / 2).saturating_sub(center);
            let mut available_window_power = 0.0;
            for (i, sample) in audio[start..end].iter().enumerate() {
                left[offset + i] = sample[0];
                right[offset + i] = sample[1];
                available_window_power += window_power[offset + i];
            }
            let (a, b) = stft
                .add_with(FFT_SIZE, |i| [left[i], right[i]])
                .expect("Complete FFT input");
            // Only positive, non-Nyquist bins enter the analysis bands.
            let scale = 2.0 / (FFT_SIZE as f64 * available_window_power);
            for (k, bin) in frame.iter_mut().enumerate() {
                let cross = a[k] * b[k].conj();
                *bin = Bin {
                    left: a[k].norm_sqr() * scale,
                    right: b[k].norm_sqr() * scale,
                    cross_re: cross.re * scale,
                    cross_im: cross.im * scale,
                };
            }
            let padded = end - start < FFT_SIZE;
            whole.add(&frame, start, end, padded);
            short.add(&frame, start, end, padded);
            long.add(&frame, start, end, padded);
        }
        short_raw.push(short.finish(short_index * short_size, audio.len(), &weights, rate));
        long_raw.push(long.finish(long_index * long_size, audio.len(), &weights, rate));
        let peak_power = short_raw
            .iter()
            .map(|w| {
                w.bands
                    .iter()
                    .map(|b| (b.left + b.right) * 0.5)
                    .sum::<f64>()
            })
            .fold(0.0f64, f64::max);
        let floor = ABSOLUTE_FLOOR.max(peak_power * RELATIVE_FLOOR);
        let short_windows: Vec<_> = short_raw.into_iter().map(|w| measure(w, floor)).collect();
        let long_windows: Vec<_> = long_raw.into_iter().map(|w| measure(w, floor)).collect();
        Ok(StereoAnalysis {
            schema: "mastering.stereo.1",
            sample_rate: rate,
            input_frames: audio.len(),
            fft_size: FFT_SIZE,
            hop_size: HOP_SIZE,
            band_edges_hz: BAND_EDGES_HZ,
            absolute_power_floor: ABSOLUTE_FLOOR,
            relative_power_floor: RELATIVE_FLOOR,
            activity_power_floor: floor,
            minimum_coherence_frames: MIN_COHERENCE_FRAMES,
            minimum_phase_resultant: MIN_PHASE_RESULTANT,
            negative_correlation_threshold: NEGATIVE_CORRELATION,
            deep_mono_loss_fraction: DEEP_MONO_LOSS,
            short_window_samples: short_size,
            long_window_samples: long_size,
            whole: measure(whole.finish(0, audio.len(), &weights, rate), floor),
            short_summary: summarize(&short_windows, rate),
            long_summary: summarize(&long_windows, rate),
            short_windows,
            long_windows,
        })
    }

    #[test]
    fn frames_on_several_cores_match_the_sequential_analysis() {
        let mut state = 0x2545_f491u32;
        for len in [7_000, 48_000 * 2 + 777, 48_000 * 12 + 5] {
            let audio: Vec<Stereo> = (0..len)
                .map(|_| {
                    let mut next = || {
                        state ^= state << 13;
                        state ^= state >> 17;
                        state ^= state << 5;
                        state as f32 / u32::MAX as f32 - 0.5
                    };
                    [next(), 0.5 * next()]
                })
                .collect();
            assert_eq!(
                serde_json::to_string(&analyze(&audio, 48000).unwrap()).unwrap(),
                serde_json::to_string(&analyze_sequential(&audio).unwrap()).unwrap(),
                "{len}"
            );
        }
    }
}
