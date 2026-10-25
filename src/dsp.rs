use crate::audio_io::Stereo;
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

pub fn db(value: f64) -> f64 {
    20.0 * value.max(1e-15).log10()
}
pub fn gain(value: f64) -> f64 {
    10f64.powf(value / 20.0)
}

#[derive(Clone, Copy)]
pub struct Biquad {
    pub b: [f64; 3],
    pub a: [f64; 2],
    state: [f64; 2],
}

impl Biquad {
    pub fn coefficients(b: [f64; 3], a: [f64; 2]) -> Self {
        Self {
            b,
            a,
            state: [0.0; 2],
        }
    }
    pub fn tick(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.state[0];
        self.state[0] = self.b[1] * x - self.a[0] * y + self.state[1];
        self.state[1] = self.b[2] * x - self.a[1] * y;
        y
    }
    pub fn response_db(&self, hz: f64, rate: u32) -> f64 {
        let w = 2.0 * PI * hz / rate as f64;
        let nr = self.b[0] + self.b[1] * w.cos() + self.b[2] * (2.0 * w).cos();
        let ni = -self.b[1] * w.sin() - self.b[2] * (2.0 * w).sin();
        let dr = 1.0 + self.a[0] * w.cos() + self.a[1] * (2.0 * w).cos();
        let di = -self.a[0] * w.sin() - self.a[1] * (2.0 * w).sin();
        10.0 * ((nr * nr + ni * ni) / (dr * dr + di * di))
            .max(1e-30)
            .log10()
    }
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
pub enum Kind {
    Peak,
    LowShelf,
    HighShelf,
    /// Second-order high-pass. `gain_db` is unused and must be zero.
    HighPass,
}

#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct EqBand {
    pub kind: Kind,
    pub hz: f64,
    pub q: f64,
    pub gain_db: f64,
}

impl EqBand {
    pub fn filter(&self, rate: u32) -> Biquad {
        if matches!(self.kind, Kind::HighPass) {
            let w = 2.0 * PI * self.hz / rate as f64;
            let c = w.cos();
            let alpha = w.sin() / (2.0 * self.q);
            let b = [(1.0 + c) * 0.5, -(1.0 + c), (1.0 + c) * 0.5];
            let den = [1.0 + alpha, -2.0 * c, 1.0 - alpha];
            return Biquad::coefficients(b.map(|v| v / den[0]), [den[1] / den[0], den[2] / den[0]]);
        }
        if self.gain_db.abs() < 1e-12 {
            return Biquad::coefficients([1.0, 0.0, 0.0], [0.0, 0.0]);
        }
        let a = 10f64.powf(self.gain_db / 40.0);
        let w = 2.0 * PI * self.hz / rate as f64;
        let c = w.cos();
        let alpha = w.sin() / (2.0 * self.q);
        let beta = 2.0 * a.sqrt() * alpha;
        let (b, den) = match self.kind {
            Kind::Peak => (
                [1.0 + alpha * a, -2.0 * c, 1.0 - alpha * a],
                [1.0 + alpha / a, -2.0 * c, 1.0 - alpha / a],
            ),
            Kind::LowShelf => (
                [
                    a * ((a + 1.0) - (a - 1.0) * c + beta),
                    2.0 * a * ((a - 1.0) - (a + 1.0) * c),
                    a * ((a + 1.0) - (a - 1.0) * c - beta),
                ],
                [
                    (a + 1.0) + (a - 1.0) * c + beta,
                    -2.0 * ((a - 1.0) + (a + 1.0) * c),
                    (a + 1.0) + (a - 1.0) * c - beta,
                ],
            ),
            Kind::HighPass => unreachable!("handled above"),
            Kind::HighShelf => (
                [
                    a * ((a + 1.0) + (a - 1.0) * c + beta),
                    -2.0 * a * ((a - 1.0) + (a + 1.0) * c),
                    a * ((a + 1.0) + (a - 1.0) * c - beta),
                ],
                [
                    (a + 1.0) - (a - 1.0) * c + beta,
                    2.0 * ((a - 1.0) - (a + 1.0) * c),
                    (a + 1.0) - (a - 1.0) * c - beta,
                ],
            ),
        };
        Biquad::coefficients(b.map(|v| v / den[0]), [den[1] / den[0], den[2] / den[0]])
    }
}

pub fn eq(audio: &mut [Stereo], bands: &[EqBand], rate: u32) {
    for band in bands {
        let mut filters = [band.filter(rate); 2];
        for frame in audio.iter_mut() {
            for c in 0..2 {
                frame[c] = filters[c].tick(frame[c] as f64) as f32;
            }
        }
    }
}

pub fn side_eq(audio: &mut [Stereo], bands: &[EqBand], rate: u32) {
    if bands.is_empty() {
        return;
    }
    let mut filters: Vec<_> = bands.iter().map(|band| band.filter(rate)).collect();
    for frame in audio {
        let mid = (frame[0] as f64 + frame[1] as f64) * 0.5;
        let mut side = (frame[0] as f64 - frame[1] as f64) * 0.5;
        for filter in &mut filters {
            side = filter.tick(side);
        }
        *frame = [(mid + side) as f32, (mid - side) as f32];
    }
}

pub(crate) const RADIUS: isize = 48;
/// Frames on each side of a frame over the ceiling within which the local
/// true-peak gain falls: the radius of the interpolator, 1 ms at 48 kHz.
const LOCAL_PEAK_RADIUS: usize = RADIUS as usize;
/// Passes of `Oversampler::lower_peaks`. A pass that lowers a frame changes
/// the interpolated samples near it.
const LOCAL_PEAK_PASSES: usize = 4;
const TAPS: usize = (RADIUS * 2 + 1) as usize;
const DOWN_RADIUS: isize = RADIUS * 4;
fn window(t: f64) -> f64 {
    if t.abs() > 1.0 {
        0.0
    } else {
        0.42 + 0.5 * (PI * t).cos() + 0.08 * (2.0 * PI * t).cos()
    }
}
fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 {
        1.0
    } else {
        (PI * x).sin() / (PI * x)
    }
}

/// Cutoff of the interpolator and the decimator of peak control, in Hz:
/// 0.90625 of the Nyquist frequency at 48 kHz. The cutoff is the same at
/// each render rate, so a 44.1 kHz render keeps the band to 20.5 kHz flat.
/// At 44.1 kHz, images and aliases at 24.1 kHz, which fold onto 20 kHz, are
/// 80 dB down.
pub const OVERSAMPLER_CUTOFF_HZ: f64 = 21_750.0;

pub struct Oversampler {
    bank: [[f64; TAPS]; 4],
    down: Vec<f64>,
    /// The same taps in f32, each repeated for the left and the right
    /// sample, for the interior loops.
    bank_pairs: [Vec<f32>; 4],
    down_pairs: Vec<f32>,
}
impl Oversampler {
    /// The 4x interpolator and decimator of audio at `rate`.
    pub fn new(rate: u32) -> Self {
        // The cutoff as a share of the Nyquist frequency of the base rate,
        // and of the 4x rate.
        let cutoff = OVERSAMPLER_CUTOFF_HZ / (rate as f64 / 2.0);
        let down_cutoff = cutoff / 4.0;
        let mut bank = [[0.0; TAPS]; 4];
        for (phase, coefficients) in bank.iter_mut().enumerate() {
            for (i, coefficient) in coefficients.iter_mut().enumerate() {
                let t = i as f64 - RADIUS as f64 - phase as f64 / 4.0;
                *coefficient = cutoff * sinc(cutoff * t) * window(t / RADIUS as f64);
            }
            let total: f64 = coefficients.iter().sum();
            for v in coefficients {
                *v /= total;
            }
        }
        let mut down: Vec<f64> = (-DOWN_RADIUS..=DOWN_RADIUS)
            .map(|k| {
                down_cutoff * sinc(down_cutoff * k as f64) * window(k as f64 / DOWN_RADIUS as f64)
            })
            .collect();
        let total: f64 = down.iter().sum();
        for v in &mut down {
            *v /= total;
        }
        let pairs = |taps: &[f64]| taps.iter().flat_map(|&c| [c as f32; 2]).collect();
        Self {
            bank_pairs: std::array::from_fn(|phase| pairs(&bank[phase])),
            down_pairs: pairs(&down),
            bank,
            down,
        }
    }
}

/// Interleaved taps against interleaved stereo samples in 16 f32 lanes, so
/// the loop runs in vector registers with no conversion per tap. Even lanes
/// hold the left channel and odd lanes the right.
#[inline(always)]
fn dot_pairs(taps: &[f32], samples: &[f32]) -> Stereo {
    const LANES: usize = 16;
    let n = taps.len().min(samples.len());
    let (taps, samples) = (&taps[..n], &samples[..n]);
    let mut acc = [0.0f32; LANES];
    let chunks = n / LANES;
    for k in 0..chunks {
        let (t, x) = (
            &taps[LANES * k..LANES * (k + 1)],
            &samples[LANES * k..LANES * (k + 1)],
        );
        for j in 0..LANES {
            acc[j] += t[j] * x[j];
        }
    }
    for i in LANES * chunks..n {
        acc[i % LANES] += taps[i] * samples[i];
    }
    let (mut left, mut right) = (0.0f32, 0.0f32);
    for j in (0..LANES).step_by(2) {
        left += acc[j];
        right += acc[j + 1];
    }
    [left, right]
}

/// `dot_pairs` of two tap sets against the same samples in one pass. Each
/// result sums its lanes in the order of `dot_pairs`, so it equals two calls.
#[inline(always)]
fn dot_pairs_two(first: &[f32], second: &[f32], samples: &[f32]) -> [Stereo; 2] {
    const LANES: usize = 16;
    let n = first.len().min(second.len()).min(samples.len());
    let (first, second, samples) = (&first[..n], &second[..n], &samples[..n]);
    let mut acc = [[0.0f32; LANES]; 2];
    let chunks = n / LANES;
    for k in 0..chunks {
        let range = LANES * k..LANES * (k + 1);
        let (t0, t1, x) = (
            &first[range.clone()],
            &second[range.clone()],
            &samples[range],
        );
        for j in 0..LANES {
            acc[0][j] += t0[j] * x[j];
            acc[1][j] += t1[j] * x[j];
        }
    }
    for i in LANES * chunks..n {
        acc[0][i % LANES] += first[i] * samples[i];
        acc[1][i % LANES] += second[i] * samples[i];
    }
    acc.map(|acc| {
        let (mut left, mut right) = (0.0f32, 0.0f32);
        for j in (0..LANES).step_by(2) {
            left += acc[j];
            right += acc[j + 1];
        }
        [left, right]
    })
}

/// Threads that the engine uses: the available cores, or fewer when
/// `MASTERING_THREADS` sets a smaller count for measurement. Each result is
/// the same at any count.
pub fn threads() -> usize {
    static LIMIT: std::sync::OnceLock<Option<usize>> = std::sync::OnceLock::new();
    let available = std::thread::available_parallelism().map_or(1, |n| n.get());
    LIMIT
        .get_or_init(|| {
            std::env::var("MASTERING_THREADS")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .filter(|&n| n > 0)
        })
        .map_or(available, |limit| limit.min(available))
}

/// Results of `work` over consecutive ranges of `0..length`, in range order.
/// Native builds split the ranges over the available cores; WASM builds run
/// them in order on one thread. Each range's result depends only on its
/// range, so the output is the same either way.
pub(crate) fn chunked<T: Send>(
    length: usize,
    chunk: usize,
    work: impl Fn(std::ops::Range<usize>) -> T + Sync,
) -> Vec<T> {
    let ranges: Vec<std::ops::Range<usize>> = (0..length)
        .step_by(chunk.max(1))
        .map(|start| start..(start + chunk).min(length))
        .collect();
    #[cfg(not(target_arch = "wasm32"))]
    {
        let threads = threads().min(8).min(ranges.len());
        if threads > 1 {
            let next = std::sync::atomic::AtomicUsize::new(0);
            let slots: std::sync::Mutex<Vec<Option<T>>> =
                std::sync::Mutex::new((0..ranges.len()).map(|_| None).collect());
            std::thread::scope(|scope| {
                for _ in 0..threads {
                    scope.spawn(|| loop {
                        let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                        let Some(range) = ranges.get(i) else { break };
                        let value = work(range.clone());
                        slots.lock().expect("chunk results")[i] = Some(value);
                    });
                }
            });
            return slots
                .into_inner()
                .expect("chunk results")
                .into_iter()
                .map(|v| v.expect("every chunk ran"))
                .collect();
        }
    }
    ranges.into_iter().map(work).collect()
}

impl Oversampler {
    pub(crate) fn at(&self, input: &[Stereo], base: usize, phase: usize) -> Stereo {
        let radius = RADIUS as usize;
        if base >= radius && base + radius < input.len() {
            return dot_pairs(
                &self.bank_pairs[phase],
                input[base - radius..=base + radius].as_flattened(),
            );
        }
        let mut out = [0.0f64; 2];
        for (i, coefficient) in self.bank[phase].iter().enumerate() {
            let index = base as isize + i as isize - RADIUS;
            if let Some(frame) = usize::try_from(index).ok().and_then(|n| input.get(n)) {
                out[0] += *coefficient * frame[0] as f64;
                out[1] += *coefficient * frame[1] as f64;
            }
        }
        out.map(|v| v as f32)
    }
    /// The four phases at `base`, each equal to `at`.
    #[inline(always)]
    pub(crate) fn at_all(&self, input: &[Stereo], base: usize) -> [Stereo; 4] {
        let radius = RADIUS as usize;
        if base >= radius && base + radius < input.len() {
            let samples = input[base - radius..=base + radius].as_flattened();
            let [p0, p1] = dot_pairs_two(&self.bank_pairs[0], &self.bank_pairs[1], samples);
            let [p2, p3] = dot_pairs_two(&self.bank_pairs[2], &self.bank_pairs[3], samples);
            return [p0, p1, p2, p3];
        }
        std::array::from_fn(|phase| self.at(input, base, phase))
    }
    pub fn peak(&self, input: &[Stereo]) -> f64 {
        self.peak_core(input, 0..input.len())
    }
    /// The peak of the samples of `input` in `core` and of the interpolated
    /// samples at those positions. Each core position needs `RADIUS` frames
    /// of `input` on each side, or the end of the signal: then the peaks of
    /// consecutive cores of one signal combine to its `peak`.
    pub(crate) fn peak_core(&self, input: &[Stereo], core: std::ops::Range<usize>) -> f64 {
        let mut peak = input[core.clone()]
            .iter()
            .flatten()
            .fold(0.0f64, |a, &b| a.max(b.abs() as f64));
        if core.is_empty() {
            return peak;
        }
        // An interpolated sample is at most the largest absolute tap sum times
        // the largest input sample under the taps. A block whose bound cannot
        // exceed the peak found so far holds no larger sample, so the result
        // equals the exhaustive search.
        const BLOCK: usize = 32;
        let gain = self
            .bank
            .iter()
            .map(|taps| taps.iter().map(|c| c.abs()).sum::<f64>())
            .fold(0.0, f64::max)
            * (1.0 + 1e-6);
        let radius = RADIUS as usize;
        let block_peaks: Vec<f64> = input
            .chunks(BLOCK)
            .map(|c| {
                c.iter()
                    .flatten()
                    .fold(0.0f64, |a, &b| a.max(b.abs() as f64))
            })
            .collect();
        // Each chunk starts from the sample peak, so its skipped blocks and
        // its maximum do not depend on the other chunks.
        let sample_peak = peak;
        let chunk_peaks = chunked(core.len(), 1 << 16, |range| {
            let range = range.start + core.start..range.end + core.start;
            let mut peak = sample_peak;
            for start in range.clone().step_by(BLOCK) {
                let first = start.saturating_sub(radius) / BLOCK;
                let last = ((start + BLOCK - 1 + radius) / BLOCK).min(block_peaks.len() - 1);
                let bound = block_peaks[first..=last]
                    .iter()
                    .copied()
                    .fold(0.0, f64::max);
                if bound * gain <= peak {
                    continue;
                }
                for base in start..(start + BLOCK).min(range.end) {
                    for frame in self.at_all(input, base) {
                        for sample in frame {
                            peak = peak.max(sample.abs() as f64);
                        }
                    }
                }
            }
            peak
        });
        peak = chunk_peaks.into_iter().fold(peak, f64::max);
        peak
    }
    /// `peak` of `input`, with the frames whose true peak is above
    /// `ceiling`, as the first pass of `lower_peaks` finds them, and the
    /// largest interpolated sample of each block that the scan measured. A
    /// block whose bound is at most the peak found so far and at most the
    /// ceiling holds neither a larger sample nor a frame above the ceiling,
    /// so the peak and the frames equal those of an exhaustive search.
    pub(crate) fn peak_scan(&self, input: &[Stereo], ceiling: f64) -> PeakScan {
        let sample_peak = sample_peak(input);
        if input.is_empty() {
            return PeakScan {
                peak: sample_peak,
                overs: Vec::new(),
                blocks: Vec::new(),
            };
        }
        let gain = self.bound_gain();
        let block_peaks = block_peaks(input);
        let parts = chunked(input.len(), 1 << 16, |range| {
            let mut peak = sample_peak;
            let mut blocks = Vec::with_capacity(range.len().div_ceil(PEAK_BLOCK));
            let mut overs = Vec::new();
            for start in range.clone().step_by(PEAK_BLOCK) {
                if block_bound(&block_peaks, start) * gain <= peak.min(ceiling) {
                    blocks.push(-1.0);
                    continue;
                }
                let mut largest = 0.0f32;
                for base in start..(start + PEAK_BLOCK).min(range.end) {
                    let mut interpolated = 0.0f32;
                    for frame in self.at_all(input, base) {
                        for sample in frame {
                            interpolated = interpolated.max(sample.abs());
                        }
                    }
                    largest = largest.max(interpolated);
                    let frame_peak = input[base][0]
                        .abs()
                        .max(input[base][1].abs())
                        .max(interpolated) as f64;
                    if frame_peak > ceiling {
                        overs.push((base, ceiling / frame_peak));
                    }
                }
                peak = peak.max(largest as f64);
                blocks.push(largest);
            }
            (peak, blocks, overs)
        });
        let mut scan = PeakScan {
            peak: sample_peak,
            overs: Vec::new(),
            blocks: Vec::with_capacity(input.len().div_ceil(PEAK_BLOCK)),
        };
        for (peak, blocks, overs) in parts {
            scan.peak = scan.peak.max(peak);
            scan.blocks.extend(blocks);
            scan.overs.extend(overs);
        }
        scan
    }

    /// `peak` of `audio` after `lower_peaks_from` changed the interpolated
    /// samples of the frames in `changed`, from `scan`, the scan of `audio`
    /// before the change. A block that the scan measured and that no range
    /// of `changed` meets keeps its largest interpolated sample; each other
    /// block is measured as in `peak_core`.
    pub(crate) fn peak_after(
        &self,
        audio: &[Stereo],
        scan: &PeakScan,
        changed: &[std::ops::Range<usize>],
    ) -> f64 {
        let sample_peak = sample_peak(audio);
        if audio.is_empty() {
            return sample_peak;
        }
        let mut fresh = vec![false; scan.blocks.len()];
        for range in changed {
            let blocks = range.start / PEAK_BLOCK..range.end.div_ceil(PEAK_BLOCK).min(fresh.len());
            fresh[blocks].fill(true);
        }
        let gain = self.bound_gain();
        let block_peaks = block_peaks(audio);
        let chunk_peaks = chunked(audio.len(), 1 << 16, |range| {
            let mut peak = sample_peak;
            for start in range.clone().step_by(PEAK_BLOCK) {
                let block = start / PEAK_BLOCK;
                if !fresh[block] && scan.blocks[block] >= 0.0 {
                    peak = peak.max(scan.blocks[block] as f64);
                    continue;
                }
                if block_bound(&block_peaks, start) * gain <= peak {
                    continue;
                }
                for base in start..(start + PEAK_BLOCK).min(range.end) {
                    for frame in self.at_all(audio, base) {
                        for sample in frame {
                            peak = peak.max(sample.abs() as f64);
                        }
                    }
                }
            }
            peak
        });
        chunk_peaks.into_iter().fold(sample_peak, f64::max)
    }

    /// The largest absolute tap sum of the interpolator, with a margin: an
    /// interpolated sample is at most this times the largest input sample
    /// under its taps.
    fn bound_gain(&self) -> f64 {
        self.bank
            .iter()
            .map(|taps| taps.iter().map(|c| c.abs()).sum::<f64>())
            .fold(0.0, f64::max)
            * (1.0 + 1e-6)
    }

    /// The frames of `audio` in `range` whose true peak is above `ceiling`,
    /// each with the gain that brings its true peak to `ceiling`.
    fn overs(
        &self,
        audio: &[Stereo],
        range: std::ops::Range<usize>,
        ceiling: f64,
    ) -> Vec<(usize, f64)> {
        const BLOCK: usize = 32;
        let gain = self
            .bank
            .iter()
            .map(|taps| taps.iter().map(|c| c.abs()).sum::<f64>())
            .fold(0.0, f64::max)
            * (1.0 + 1e-6);
        let radius = RADIUS as usize;
        let offset = range.start;
        chunked(range.len(), 1 << 16, |chunk| {
            let mut found = Vec::new();
            let chunk = chunk.start + offset..chunk.end + offset;
            for start in chunk.clone().step_by(BLOCK) {
                let end = (start + BLOCK).min(chunk.end);
                // As in `peak_core`: no frame of a block whose bound is
                // under the ceiling is over it.
                let bound = audio[start.saturating_sub(radius)..(end + radius).min(audio.len())]
                    .iter()
                    .flatten()
                    .fold(0.0f64, |a, &b| a.max(b.abs() as f64));
                if bound * gain <= ceiling {
                    continue;
                }
                for base in start..end {
                    let mut peak = audio[base][0].abs().max(audio[base][1].abs()) as f64;
                    for frame in self.at_all(audio, base) {
                        for sample in frame {
                            peak = peak.max(sample.abs() as f64);
                        }
                    }
                    if peak > ceiling {
                        found.push((base, ceiling / peak));
                    }
                }
            }
            found
        })
        .concat()
    }

    /// Lower the gain near each frame of `audio` whose true peak is above
    /// `ceiling`, so that its true peak comes to `ceiling`. The gain of a
    /// frame is the mean, over `LOCAL_PEAK_RADIUS` frames on each side, of
    /// the smallest required gain within the same distance. Thus it is at
    /// or under the required gain of each frame over the ceiling. It falls
    /// over the 2 ms before such a frame and recovers over the 2 ms after
    /// it. Later passes measure again where the gain changed. Returns the
    /// largest reduction in dB.
    pub fn lower_peaks(&self, audio: &mut [Stereo], ceiling: f64) -> f64 {
        let overs = self.overs(audio, 0..audio.len(), ceiling);
        self.lower_peaks_from(audio, ceiling, overs).0
    }

    /// `lower_peaks` whose first pass lowers `overs`, the frames over
    /// `ceiling` of the whole of `audio` (`peak_scan`). Returns the largest
    /// reduction in dB and the ranges of frames whose interpolated samples
    /// the passes changed.
    pub(crate) fn lower_peaks_from(
        &self,
        audio: &mut [Stereo],
        ceiling: f64,
        overs: Vec<(usize, f64)>,
    ) -> (f64, Vec<std::ops::Range<usize>>) {
        let r = LOCAL_PEAK_RADIUS;
        let width = (2 * r + 1) as f64;
        let mut largest = 0.0f64;
        let mut changed = Vec::new();
        let mut overs = overs;
        let mut ranges: Vec<std::ops::Range<usize>> = Vec::new();
        for pass in 0..LOCAL_PEAK_PASSES {
            if pass > 0 {
                overs = ranges
                    .iter()
                    .flat_map(|range| self.overs(audio, range.clone(), ceiling))
                    .collect();
            }
            let mut next: Vec<std::ops::Range<usize>> = Vec::new();
            let mut first = 0;
            while first < overs.len() {
                // The overs whose gains meet form one span.
                let mut last = first;
                while last + 1 < overs.len() && overs[last + 1].0 - overs[last].0 <= 4 * r {
                    last += 1;
                }
                let span = overs[first].0.saturating_sub(2 * r)
                    ..(overs[last].0 + 2 * r + 1).min(audio.len());
                let mut lowest = vec![1.0f64; span.len()];
                for &(frame, gain) in &overs[first..=last] {
                    let low = frame.saturating_sub(r) - span.start;
                    let high = (frame + r + 1).min(span.end) - span.start;
                    for value in &mut lowest[low..high] {
                        *value = value.min(gain);
                    }
                }
                // The mean over the 2r + 1 frames centred on each frame; a
                // frame outside the span has the gain 1.
                let at = |k: isize| {
                    usize::try_from(k)
                        .ok()
                        .and_then(|k| lowest.get(k).copied())
                        .unwrap_or(1.0)
                };
                let mut sum: f64 = (-(r as isize)..=r as isize).map(at).sum();
                for k in 0..span.len() {
                    let g = sum / width;
                    largest = largest.max(-db(g));
                    let frame = &mut audio[span.start + k];
                    frame[0] = (frame[0] as f64 * g) as f32;
                    frame[1] = (frame[1] as f64 * g) as f32;
                    sum += at((k + r + 1) as isize) - at(k as isize - r as isize);
                }
                // The interpolated samples change within the radius of the
                // span; the ranges of two spans can meet.
                let range = span.start.saturating_sub(RADIUS as usize)
                    ..(span.end + RADIUS as usize).min(audio.len());
                match next.last_mut() {
                    Some(previous) if previous.end >= range.start => previous.end = range.end,
                    _ => next.push(range),
                }
                first = last + 1;
            }
            if next.is_empty() {
                break;
            }
            changed.extend(next.iter().cloned());
            ranges = next;
        }
        (largest, changed)
    }
    pub fn up(&self, input: &[Stereo]) -> Vec<Stereo> {
        let mut out = Vec::with_capacity(input.len() * 4);
        for base in 0..input.len() {
            out.extend(self.at_all(input, base));
        }
        out
    }
    pub fn down(&self, input: &[Stereo]) -> Vec<Stereo> {
        let mut output = Vec::with_capacity(input.len() / 4);
        let radius = DOWN_RADIUS as usize;
        for base in (0..input.len()).step_by(4) {
            if base >= radius && base + radius < input.len() {
                output.push(dot_pairs(
                    &self.down_pairs,
                    input[base - radius..=base + radius].as_flattened(),
                ));
                continue;
            }
            let mut frame = [0.0f64; 2];
            for (i, coefficient) in self.down.iter().enumerate() {
                let index = base as isize + i as isize - DOWN_RADIUS;
                if let Some(x) = usize::try_from(index).ok().and_then(|n| input.get(n)) {
                    frame[0] += coefficient * x[0] as f64;
                    frame[1] += coefficient * x[1] as f64;
                }
            }
            output.push(frame.map(|v| v as f32));
        }
        output
    }

    /// Same taps and summation order as `down`, over an available input window.
    pub(crate) fn down_at_window(
        &self,
        input: &[Stereo],
        window_start: usize,
        total_frames: usize,
        base: usize,
    ) -> Stereo {
        let radius = DOWN_RADIUS as usize;
        if base >= radius && base + radius < total_frames {
            let (low, high) = (base - radius - window_start, base + radius - window_start);
            return dot_pairs(&self.down_pairs, input[low..=high].as_flattened());
        }
        let mut frame = [0.0f64; 2];
        for (i, coefficient) in self.down.iter().enumerate() {
            let index = base as isize + i as isize - DOWN_RADIUS;
            if let Ok(index) = usize::try_from(index) {
                if index < total_frames {
                    let x = input[index - window_start];
                    frame[0] += coefficient * x[0] as f64;
                    frame[1] += coefficient * x[1] as f64;
                }
            }
        }
        frame.map(|v| v as f32)
    }

    pub(crate) fn down_radius(&self) -> usize {
        DOWN_RADIUS as usize
    }
}

/// Frames of a block of the bound of the peak estimate.
const PEAK_BLOCK: usize = 32;

/// The 4x scan of a render (`Oversampler::peak_scan`).
pub(crate) struct PeakScan {
    /// `Oversampler::peak` of the render.
    pub(crate) peak: f64,
    /// The frames whose true peak is above the ceiling of the scan, each with
    /// the gain that brings its true peak to the ceiling.
    pub(crate) overs: Vec<(usize, f64)>,
    /// The largest absolute interpolated sample of each block of
    /// `PEAK_BLOCK` frames, or −1 for a block that the scan did not measure.
    blocks: Vec<f32>,
}

/// The largest absolute sample of `input`.
fn sample_peak(input: &[Stereo]) -> f64 {
    input
        .iter()
        .flatten()
        .fold(0.0f64, |a, &b| a.max(b.abs() as f64))
}

/// The largest absolute sample of each block of `PEAK_BLOCK` frames of
/// `input`.
fn block_peaks(input: &[Stereo]) -> Vec<f64> {
    input.chunks(PEAK_BLOCK).map(sample_peak).collect()
}

/// The largest absolute sample under the taps of the block of frames that
/// starts at `start`, from `block_peaks`: the blocks within `RADIUS` frames
/// of it.
fn block_bound(block_peaks: &[f64], start: usize) -> f64 {
    let radius = RADIUS as usize;
    let first = start.saturating_sub(radius) / PEAK_BLOCK;
    let last = ((start + PEAK_BLOCK - 1 + radius) / PEAK_BLOCK).min(block_peaks.len() - 1);
    block_peaks[first..=last]
        .iter()
        .copied()
        .fold(0.0, f64::max)
}

/// Oversampling factor of peak control. Peak control runs on the 4× signal.
pub const OVERSAMPLING: usize = 4;

/// Samples per millisecond of the 4× signal of audio at `rate`.
pub fn oversampled_per_ms(rate: u32) -> f64 {
    (OVERSAMPLING * rate as usize) as f64 / 1000.0
}
/// Window over which the fast stage must hold reduction before the sustained
/// stage adopts it.
const SUSTAIN_WINDOW_MS: f64 = 60.0;

#[derive(Clone, Copy, Serialize)]
pub struct LimiterSettings {
    /// Fast-stage release time constant in milliseconds, in the dB domain.
    pub release_ms: f64,
    /// Attack window in milliseconds. Three cascaded windows of this length
    /// smooth the lookahead minimum; total lookahead is three windows.
    pub pre_attack_ms: f64,
    /// Fast-stage hold after a new maximum, in milliseconds. The hold applies
    /// when another over-ceiling run ended within this time, so recurring
    /// crests of a sustained low frequency release together and an isolated
    /// click releases at once.
    pub hold_ms: f64,
    /// Sustained-stage release time constant in milliseconds. Zero disables
    /// the sustained stage.
    pub slow_release_ms: f64,
    /// Fast-stage attack time constant in milliseconds, in the dB domain,
    /// while the required reduction is above the fast stage. Zero takes each
    /// new maximum at once. A longer attack lets the start of an over pass
    /// the limiter, and the clipper takes it.
    pub attack_ms: f64,
}

impl Default for LimiterSettings {
    fn default() -> Self {
        Self {
            release_ms: 50.0,
            pre_attack_ms: 1.0,
            hold_ms: 12.0,
            slow_release_ms: 0.0,
            attack_ms: 0.0,
        }
    }
}

impl LimiterSettings {
    pub fn validate(&self) -> crate::audio_io::Result<()> {
        if !(5.0..=2000.0).contains(&self.release_ms)
            || !(0.05..=10.0).contains(&self.pre_attack_ms)
            || !(0.0..=100.0).contains(&self.hold_ms)
            || !(self.slow_release_ms == 0.0 || (20.0..=5000.0).contains(&self.slow_release_ms))
            || !(0.0..=20.0).contains(&self.attack_ms)
        {
            return Err("Invalid limiter settings".into());
        }
        Ok(())
    }
}

/// Correction of a true peak above the ceiling after the decimation from
/// 4x.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TruePeak {
    /// Lower the whole render by the excess of its highest true peak.
    Trim,
    /// Lower the gain only near each frame over the ceiling
    /// (`Oversampler::lower_peaks`), then trim what remains.
    Local,
}

#[derive(Clone, Serialize)]
pub struct LimiterStats {
    pub maximum_reduction_db: f64,
    pub samples_reduced_percent: f64,
    pub frames_above_one_db_percent: f64,
    pub mean_reduction_db: f64,
    pub longest_run_above_one_db_ms: f64,
}

/// Forward-order accumulation of limiter statistics from applied gains.
#[derive(Clone)]
pub struct LimiterStatsAccumulator {
    minimum: f64,
    sum: f64,
    reduced: usize,
    above_one: usize,
    run: usize,
    longest: usize,
    frames: usize,
    /// Frames above 1 dB from the first frame, while every frame so far is.
    leading: usize,
    leading_open: bool,
}

impl Default for LimiterStatsAccumulator {
    fn default() -> Self {
        Self {
            minimum: 1.0,
            sum: 0.0,
            reduced: 0,
            above_one: 0,
            run: 0,
            longest: 0,
            frames: 0,
            leading: 0,
            leading_open: true,
        }
    }
}

impl LimiterStatsAccumulator {
    /// Returns the reduction in dB for the pushed gain.
    pub fn push(&mut self, gain: f32) -> f64 {
        let g = gain as f64;
        self.minimum = self.minimum.min(g);
        // -db(1.0) is -0.0; most samples carry no reduction.
        let reduction = if g == 1.0 { -0.0 } else { -db(g) };
        self.sum += reduction;
        if reduction > 1.0 {
            self.above_one += 1;
            self.run += 1;
            self.longest = self.longest.max(self.run);
            if self.leading_open {
                self.leading += 1;
            }
        } else {
            self.run = 0;
            self.leading_open = false;
        }
        self.reduced += usize::from(g < 0.999);
        self.frames += 1;
        reduction
    }
    /// Append the statistics of the frames that follow this accumulator's.
    /// A run above 1 dB continues across the boundary.
    pub fn merge(&mut self, next: &Self) {
        self.longest = self.longest.max(next.longest).max(self.run + next.leading);
        self.run = if next.leading_open {
            self.run + next.run
        } else {
            next.run
        };
        if self.leading_open {
            self.leading += next.leading;
            self.leading_open = next.leading_open;
        }
        self.minimum = self.minimum.min(next.minimum);
        self.sum += next.sum;
        self.reduced += next.reduced;
        self.above_one += next.above_one;
        self.frames += next.frames;
    }
    /// The statistics of the 4× signal of audio at `rate`.
    pub fn finish(&self, rate: u32) -> LimiterStats {
        let frames = self.frames.max(1) as f64;
        LimiterStats {
            maximum_reduction_db: -db(self.minimum),
            samples_reduced_percent: 100.0 * self.reduced as f64 / frames,
            frames_above_one_db_percent: 100.0 * self.above_one as f64 / frames,
            mean_reduction_db: self.sum / frames,
            longest_run_above_one_db_ms: self.longest as f64 / oversampled_per_ms(rate),
        }
    }
}

/// Running extremum over a trailing window of samples (monotonic deque).
struct RunningExtremum {
    deque: std::collections::VecDeque<(usize, f64)>,
    window: usize,
    maximum: bool,
}

impl RunningExtremum {
    fn new(window: usize, maximum: bool) -> Self {
        Self {
            deque: std::collections::VecDeque::new(),
            window: window.max(1),
            maximum,
        }
    }
    fn push(&mut self, index: usize, value: f64) -> f64 {
        while self.deque.back().is_some_and(|b| {
            if self.maximum {
                b.1 <= value
            } else {
                b.1 >= value
            }
        }) {
            self.deque.pop_back();
        }
        self.deque.push_back((index, value));
        while self
            .deque
            .front()
            .is_some_and(|f| f.0 + self.window <= index)
        {
            self.deque.pop_front();
        }
        self.deque.front().unwrap().1
    }
}

/// Moving average with a running sum. Rings start at unity gain.
struct MovingAverage {
    ring: Vec<f64>,
    position: usize,
    sum: f64,
}

impl MovingAverage {
    fn new(length: usize) -> Self {
        let length = length.max(1);
        Self {
            ring: vec![1.0; length],
            position: 0,
            sum: length as f64,
        }
    }
    fn fill(&mut self, value: f64) {
        self.ring.fill(value);
        self.sum = value * self.ring.len() as f64;
    }
    fn push(&mut self, value: f64) -> f64 {
        let old = self.ring[self.position];
        self.ring[self.position] = value;
        self.position = (self.position + 1) % self.ring.len();
        self.sum += value - old;
        self.sum / self.ring.len() as f64
    }
}

/// Streaming stereo-linked peak limiter on the oversampled signal.
///
/// Stage order per frame: required reduction, fast stage with hold and
/// release, sustained stage that adopts reduction the fast stage has held for
/// the sustain window and releases slowly, trailing maximum over the
/// lookahead, then three cascaded moving averages. The smoothed gain for a
/// frame never exceeds its required gain, so the ceiling holds exactly at the
/// oversampled rate. Output gains lag input frames by `lag()`.
pub struct Limiter {
    ceiling: f64,
    attack: Option<f64>,
    fast_release: f64,
    slow_release: Option<f64>,
    hold_samples: usize,
    fast_db: f64,
    hold_left: usize,
    in_run: bool,
    previous_run_end: Option<usize>,
    slow_db: f64,
    sustain: RunningExtremum,
    lookahead: RunningExtremum,
    smoothing: [MovingAverage; 3],
    lookahead_samples: usize,
    pushed: usize,
    emitted: usize,
    /// The last lookahead reduction and its gain. The lookahead holds a
    /// peak's reduction while the peak is in its window.
    last_gain: (f64, f64),
}

impl Limiter {
    /// A limiter of the 4× signal of audio at `rate`.
    pub fn new(ceiling: f64, settings: LimiterSettings, rate: u32) -> Self {
        assert!(ceiling.is_finite() && ceiling > 0.0);
        settings.validate().expect("validated limiter settings");
        let per_ms = oversampled_per_ms(rate);
        let window = ((settings.pre_attack_ms * per_ms).round() as usize).max(1);
        Self {
            ceiling,
            attack: (settings.attack_ms > 0.0)
                .then(|| (-1.0 / (per_ms * settings.attack_ms)).exp()),
            fast_release: (-1.0 / (per_ms * settings.release_ms)).exp(),
            slow_release: (settings.slow_release_ms > 0.0)
                .then(|| (-1.0 / (per_ms * settings.slow_release_ms)).exp()),
            hold_samples: (settings.hold_ms * per_ms).round() as usize,
            fast_db: 0.0,
            hold_left: 0,
            in_run: false,
            previous_run_end: None,
            slow_db: 0.0,
            sustain: RunningExtremum::new((SUSTAIN_WINDOW_MS * per_ms).round() as usize, false),
            lookahead: RunningExtremum::new(3 * window, true),
            smoothing: [
                MovingAverage::new(window),
                MovingAverage::new(window),
                MovingAverage::new(window),
            ],
            lookahead_samples: 3 * window,
            pushed: 0,
            emitted: 0,
            last_gain: (f64::NAN, f64::NAN),
        }
    }

    /// Frames of delay between a pushed peak and its emitted gain.
    pub fn lag(&self) -> usize {
        self.lookahead_samples - 1
    }

    /// Push the linked peak of the next frame. Returns the gain of the frame
    /// `lag()` frames earlier once it is available.
    pub fn push(&mut self, peak: f64) -> Option<f32> {
        let required = if peak > self.ceiling {
            20.0 * (peak / self.ceiling).log10()
        } else {
            0.0
        };
        let index = self.pushed;
        if required > 0.0 {
            self.in_run = true;
        } else if self.in_run {
            self.in_run = false;
            self.previous_run_end = Some(index - 1);
        }
        if required >= self.fast_db {
            self.fast_db = match self.attack {
                Some(attack) => required - (required - self.fast_db) * attack,
                None => required,
            };
            let recurring = self
                .previous_run_end
                .is_some_and(|end| index - end <= self.hold_samples);
            self.hold_left = if recurring { self.hold_samples } else { 0 };
        } else if self.hold_left > 0 {
            self.hold_left -= 1;
        } else {
            self.fast_db *= self.fast_release;
            if self.fast_db < 1e-6 {
                self.fast_db = 0.0;
            }
        }
        let reduction = match self.slow_release {
            Some(release) => {
                let sustained = self.sustain.push(index, self.fast_db);
                if sustained >= self.slow_db {
                    self.slow_db = sustained;
                } else {
                    self.slow_db *= release;
                    if self.slow_db < 1e-6 {
                        self.slow_db = 0.0;
                    }
                }
                self.fast_db.max(self.slow_db)
            }
            None => self.fast_db,
        };
        let ahead = self.lookahead.push(index, reduction);
        self.pushed += 1;
        if self.pushed < self.lookahead_samples {
            return None;
        }
        // gain(-0.0) is 1.0 exactly.
        let mut g = if ahead == 0.0 {
            1.0
        } else if ahead == self.last_gain.0 {
            self.last_gain.1
        } else {
            let g = gain(-ahead);
            self.last_gain = (ahead, g);
            g
        };
        if self.emitted == 0 {
            // Pre-roll with the first lookahead gain so early output never
            // exceeds the required gain.
            for average in &mut self.smoothing {
                average.fill(g);
            }
        }
        for average in &mut self.smoothing {
            g = average.push(g);
        }
        self.emitted += 1;
        Some(g.clamp(0.0, 1.0) as f32)
    }

    /// Drain the gains of the last `lag()` real frames after the final push.
    pub fn flush(&mut self, real_frames: usize) -> Vec<f32> {
        let mut gains = Vec::with_capacity(self.lag());
        while self.emitted < real_frames {
            if let Some(g) = self.push(0.0) {
                gains.push(g);
            }
        }
        gains
    }
}

/// Limit `audio`, the 4× signal of audio at `rate`.
pub fn limit(
    audio: &mut [Stereo],
    ceiling: f64,
    settings: LimiterSettings,
    rate: u32,
) -> LimiterStats {
    let mut limiter = Limiter::new(ceiling, settings, rate);
    let mut gains = Vec::with_capacity(audio.len());
    for frame in audio.iter() {
        let peak = frame[0].abs().max(frame[1].abs()) as f64;
        if let Some(g) = limiter.push(peak) {
            gains.push(g);
        }
    }
    gains.extend(limiter.flush(audio.len()));
    debug_assert_eq!(gains.len(), audio.len());
    let mut stats = LimiterStatsAccumulator::default();
    for (frame, g) in audio.iter_mut().zip(gains) {
        stats.push(g);
        frame[0] *= g;
        frame[1] *= g;
    }
    stats.finish(rate)
}

/// Ceiling-relative soft clipper. Peaks up to `budget_db` above the ceiling
/// map onto the ceiling through a quadratic knee whose slope falls from one to
/// zero. The knee starts `2 - 10^(budget/20)` times the ceiling below it.
#[derive(Clone, Copy)]
pub struct SoftClip {
    ceiling: f64,
    knee_start: f64,
    width: f64,
}

impl SoftClip {
    pub fn new(ceiling: f64, budget_db: f64) -> Option<Self> {
        assert!(ceiling.is_finite() && ceiling > 0.0);
        assert!((0.0..=6.0).contains(&budget_db));
        if budget_db == 0.0 {
            return None;
        }
        let knee = gain(budget_db) - 1.0;
        Some(Self {
            ceiling,
            knee_start: ceiling * (1.0 - knee),
            width: ceiling * 2.0 * knee,
        })
    }
    /// Returns the shaped sample and its reduction in dB.
    pub fn shape(&self, x: f64) -> (f64, f64) {
        let a = x.abs();
        if a <= self.knee_start {
            return (x, 0.0);
        }
        let d = (a - self.knee_start).min(self.width);
        let y = self.knee_start + d - d * d / (2.0 * self.width);
        let y = y.min(self.ceiling);
        (y.copysign(x), db(a / y))
    }
}

/// ITU-R BS.1770 K-weighting stages at `rate`: the shelf, then the
/// high-pass. At 48 kHz the stages have the coefficients of the
/// recommendation. At another rate the bilinear transform of the same analog
/// stages gives them.
pub fn k_weighting(rate: u32) -> (Biquad, Biquad) {
    if rate == 48000 {
        return (
            Biquad::coefficients(
                [1.53512485958697, -2.69169618940638, 1.19839281085285],
                [-1.69065929318241, 0.73248077421585],
            ),
            Biquad::coefficients([1.0, -2.0, 1.0], [-1.99004745483398, 0.99007225036621]),
        );
    }
    let rate = rate as f64;
    let shelf = {
        let k = (PI * 1681.974450955533 / rate).tan();
        let q = 0.7071752369554196;
        let vh = 10f64.powf(3.999843853973347 / 20.0);
        let vb = vh.powf(0.4996667741545416);
        let a0 = 1.0 + k / q + k * k;
        Biquad::coefficients(
            [
                (vh + vb * k / q + k * k) / a0,
                2.0 * (k * k - vh) / a0,
                (vh - vb * k / q + k * k) / a0,
            ],
            [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
        )
    };
    let highpass = {
        let k = (PI * 38.13547087602444 / rate).tan();
        let q = 0.5003270373238773;
        let a0 = 1.0 + k / q + k * k;
        Biquad::coefficients(
            [1.0, -2.0, 1.0],
            [2.0 * (k * k - 1.0) / a0, (1.0 - k / q + k * k) / a0],
        )
    };
    (shelf, highpass)
}

/// Power gain of the K-weighting stages at `hz` for audio at `rate`.
pub fn k_weighting_power(hz: f64, rate: u32) -> f64 {
    let (shelf, highpass) = k_weighting(rate);
    10f64.powf((shelf.response_db(hz, rate) + highpass.response_db(hz, rate)) / 10.0)
}

/// Integrated loudness of `audio` at `rate`, in LUFS.
pub fn loudness(audio: &[Stereo], rate: u32) -> Option<f64> {
    let mut meter = LoudnessMeter::new(rate);
    meter.push(audio);
    meter.finish()
}

/// Integrated loudness of audio that arrives in blocks; the blocks may have
/// any length.
pub struct LoudnessMeter {
    shelves: [Biquad; 2],
    highpasses: [Biquad; 2],
    /// Frames in 100 ms, a quarter of a gating block.
    bucket: usize,
    buckets: Vec<f64>,
    energy: f64,
    frames: usize,
}

impl LoudnessMeter {
    /// A meter of audio at `rate`.
    pub fn new(rate: u32) -> Self {
        let (shelf, highpass) = k_weighting(rate);
        Self {
            shelves: [shelf; 2],
            highpasses: [highpass; 2],
            bucket: rate as usize / 10,
            buckets: Vec::new(),
            energy: 0.0,
            frames: 0,
        }
    }

    pub fn push(&mut self, audio: &[Stereo]) {
        for frame in audio {
            for (c, sample) in frame.iter().enumerate() {
                let filtered = self.highpasses[c].tick(self.shelves[c].tick(*sample as f64));
                self.energy += filtered * filtered;
            }
            self.frames += 1;
            if self.frames.is_multiple_of(self.bucket) {
                self.buckets.push(self.energy / self.bucket as f64);
                self.energy = 0.0;
            }
        }
    }

    pub fn finish(self) -> Option<f64> {
        let buckets = self.buckets;
        let absolute = 10f64.powf((-70.0 + 0.691) / 10.0);
        let blocks: Vec<f64> = buckets
            .windows(4)
            .map(|v| v.iter().sum::<f64>() / 4.0)
            .filter(|&v| v > absolute)
            .collect();
        if blocks.is_empty() {
            return None;
        }
        let relative = blocks.iter().sum::<f64>() / blocks.len() as f64 / 10.0;
        let included: Vec<f64> = blocks
            .into_iter()
            .filter(|&v| v > relative.max(absolute))
            .collect();
        Some(-0.691 + 10.0 * (included.iter().sum::<f64>() / included.len() as f64).log10())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Noise with loud bursts whose interpolated samples pass full scale:
    /// limited audio with overs at 4x.
    fn bursts(frames: usize, seed: u64) -> Vec<Stereo> {
        let mut state = seed;
        let mut next = || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((state >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
        };
        (0..frames)
            .map(|i| {
                let loud = (i / 3_000) % 4 == 1;
                let level = if loud { 0.97 } else { 0.35 };
                let x = (level * next()).clamp(-0.97, 0.97);
                let y = if loud {
                    (0.9 * x + 0.1 * next()).clamp(-0.97, 0.97)
                } else {
                    0.5 * x
                };
                [x, y]
            })
            .collect()
    }

    #[test]
    fn the_scan_gives_the_peak_and_the_overs_of_the_exhaustive_passes() {
        let os = Oversampler::new(48_000);
        for (frames, seed) in [(70_001, 1u64), (140_000, 7), (65_536 * 2 + 31, 99)] {
            let audio = bursts(frames, seed);
            let peak = os.peak(&audio);
            for ceiling in [0.5, 0.9, 0.96, peak * 0.999, peak, 1.5] {
                let scan = os.peak_scan(&audio, ceiling);
                assert_eq!(scan.peak.to_bits(), peak.to_bits(), "{frames} {ceiling}");
                let overs = os.overs(&audio, 0..audio.len(), ceiling);
                assert_eq!(scan.overs.len(), overs.len(), "{frames} {ceiling}");
                for (a, b) in scan.overs.iter().zip(&overs) {
                    assert_eq!((a.0, a.1.to_bits()), (b.0, b.1.to_bits()));
                }
            }
        }
    }

    #[test]
    fn the_peak_after_the_local_gain_is_the_peak_of_the_changed_audio() {
        let os = Oversampler::new(44_100);
        for (frames, seed) in [(90_000, 3u64), (200_003, 11)] {
            let audio = bursts(frames, seed);
            for ceiling in [0.8, 0.93, 0.99] {
                let mut expected = audio.clone();
                let reduction = os.lower_peaks(&mut expected, ceiling);
                let mut actual = audio.clone();
                let mut scan = os.peak_scan(&actual, ceiling);
                let overs = std::mem::take(&mut scan.overs);
                let (largest, changed) = os.lower_peaks_from(&mut actual, ceiling, overs);
                assert_eq!(largest.to_bits(), reduction.to_bits());
                assert!(actual
                    .iter()
                    .flatten()
                    .map(|x| x.to_bits())
                    .eq(expected.iter().flatten().map(|x| x.to_bits())));
                assert_eq!(
                    os.peak_after(&actual, &scan, &changed).to_bits(),
                    os.peak(&expected).to_bits(),
                    "{frames} {ceiling}"
                );
            }
        }
    }

    #[test]
    fn all_phases_equal_each_phase_bit_for_bit() {
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let input: Vec<Stereo> = (0..400)
            .map(|_| {
                std::array::from_fn(|_| {
                    state = state
                        .wrapping_mul(6_364_136_223_846_793_005)
                        .wrapping_add(1_442_695_040_888_963_407);
                    ((state >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
                })
            })
            .collect();
        let os = Oversampler::new(48000);
        for base in 0..input.len() {
            let all = os.at_all(&input, base);
            for (phase, frame) in all.iter().enumerate() {
                let one = os.at(&input, base, phase);
                assert_eq!(
                    frame.map(f32::to_bits),
                    one.map(f32::to_bits),
                    "{base} {phase}"
                );
            }
        }
    }
}
