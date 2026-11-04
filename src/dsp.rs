use crate::audio_io::Stereo;
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

pub fn db(value: f64) -> f64 {
    20.0 * value.max(1e-15).log10()
}
/// 10^(value / 20) as e^(value ln 10 / 20): in the browser engine, `exp`
/// takes a quarter of the time of `pow` (D291).
pub fn gain(value: f64) -> f64 {
    (value * (std::f64::consts::LN_10 / 20.0)).exp()
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
    /// The state of the filter after its last tick.
    pub fn state(&self) -> [f64; 2] {
        self.state
    }
    /// The filter with the coefficients of `self` and the state `state`.
    pub fn with_state(self, state: [f64; 2]) -> Self {
        Self { state, ..self }
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

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Debug)]
pub enum Kind {
    Peak,
    LowShelf,
    HighShelf,
    /// Second-order high-pass. `gain_db` is unused and must be zero.
    HighPass,
}

#[derive(Clone, Copy, PartialEq, Serialize, Deserialize, Debug)]
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
    Eq::new(bands, rate).process(audio);
}

/// `eq` in spans: each filter keeps its state from one span to the next.
pub struct Eq(Vec<[Biquad; 2]>);

impl Eq {
    pub fn new(bands: &[EqBand], rate: u32) -> Self {
        Self(bands.iter().map(|band| [band.filter(rate); 2]).collect())
    }
    /// Filter the next frames of the signal in place.
    pub fn process(&mut self, audio: &mut [Stereo]) {
        for state in &mut self.0 {
            // In a local, the state of the filters stays in registers.
            let mut filters = *state;
            for frame in audio.iter_mut() {
                for c in 0..2 {
                    frame[c] = filters[c].tick(frame[c] as f64) as f32;
                }
            }
            *state = filters;
        }
    }
}

pub fn side_eq(audio: &mut [Stereo], bands: &[EqBand], rate: u32) {
    SideEq::new(bands, rate).process(audio);
}

/// `side_eq` in spans: each filter keeps its state from one span to the
/// next.
pub struct SideEq(Vec<Biquad>);

impl SideEq {
    pub fn new(bands: &[EqBand], rate: u32) -> Self {
        Self(bands.iter().map(|band| band.filter(rate)).collect())
    }
    /// Filter the side of the next frames of the signal in place. Without a
    /// band, the frames stay as they are.
    pub fn process(&mut self, audio: &mut [Stereo]) {
        if self.0.is_empty() {
            return;
        }
        let mut filters = std::mem::take(&mut self.0);
        for frame in audio {
            let mid = (frame[0] as f64 + frame[1] as f64) * 0.5;
            let mut side = (frame[0] as f64 - frame[1] as f64) * 0.5;
            for filter in &mut filters {
                side = filter.tick(side);
            }
            *frame = [(mid + side) as f32, (mid - side) as f32];
        }
        self.0 = filters;
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
    /// The interpolator at the points of `PEAK_POINTS` in each sample
    /// interval that are not 4x points. With `bank`, the true-peak estimate
    /// reads `PEAK_POINTS` points in each interval (D211).
    between: Vec<[f64; TAPS]>,
    down: Vec<f64>,
    /// The same taps in f32, each repeated for the left and the right
    /// sample, for the interior loops.
    bank_pairs: [Vec<f32>; 4],
    between_pairs: Vec<Vec<f32>>,
    down_pairs: Vec<f32>,
    /// The central taps (`CENTER_FIRST`, `CENTER_TAPS`) in f32, each with
    /// 16 lanes: the tap of each point of `bank` and `between`, in order,
    /// for the left and the right sample. The sums of a frame run in the
    /// lanes, with no sum across lanes (D292).
    center: [[f32; 16]; CENTER_TAPS],
    /// The largest absolute sum of the f32 taps of a point outside the
    /// central taps, plus 1e-4 of the absolute sum of all its f32 taps. A
    /// computed point is at most its computed central sum plus this times
    /// the largest sample under its taps: the margin is more than the
    /// rounding of the f32 sums of 97 and 18 products, at most 115 × 2^−24
    /// of the absolute sum of the products (D292).
    tail_gain: f64,
    /// The taps of the 11 frames before the central taps and of the 11
    /// frames after them, in the lanes of `center`. With the central taps
    /// they are the 40 wide taps from `WIDE_FIRST` (D303).
    sides: [[[f32; 16]; SIDE_TAPS]; 2],
    /// `tail_gain` of the wide taps. The margin is more than the rounding
    /// of the f32 sums of 97 and 40 products, at most 137 × 2^−24 of the
    /// absolute sum of the products (D303).
    wide_tail_gain: f64,
}

/// The first and the count of the central taps of the bound of an
/// interpolated point: the taps from 8 frames before the frame to 9 frames
/// after it, around the points from the frame to the next frame (D292).
const CENTER_FIRST: usize = RADIUS as usize - 8;
const CENTER_TAPS: usize = 18;
/// The first and the count of the wide taps of the bound of a group whose
/// central bound is over the limit: the taps from 19 frames before the
/// frame to 20 frames after it (D303).
const WIDE_FIRST: usize = RADIUS as usize - 19;
const WIDE_TAPS: usize = 40;
/// The wide taps on each side of the central taps.
const SIDE_TAPS: usize = CENTER_FIRST - WIDE_FIRST;

/// Points of the true-peak estimate in each sample interval: the 4x points
/// of peak control and the points between them (D211).
pub const PEAK_POINTS: usize = 8;
impl Oversampler {
    /// The 4x interpolator and decimator of audio at `rate`.
    pub fn new(rate: u32) -> Self {
        // The cutoff as a share of the Nyquist frequency of the base rate,
        // and of the 4x rate.
        let cutoff = OVERSAMPLER_CUTOFF_HZ / (rate as f64 / 2.0);
        let down_cutoff = cutoff / 4.0;
        // The taps of the point at `offset` of a sample interval.
        let taps = |offset: f64| {
            let mut coefficients = [0.0; TAPS];
            for (i, coefficient) in coefficients.iter_mut().enumerate() {
                let t = i as f64 - RADIUS as f64 - offset;
                *coefficient = cutoff * sinc(cutoff * t) * window(t / RADIUS as f64);
            }
            let total: f64 = coefficients.iter().sum();
            for v in &mut coefficients {
                *v /= total;
            }
            coefficients
        };
        let bank: [[f64; TAPS]; 4] = std::array::from_fn(|phase| taps(phase as f64 / 4.0));
        let between: Vec<[f64; TAPS]> = (0..PEAK_POINTS)
            .filter(|k| k % (PEAK_POINTS / 4) != 0)
            .map(|k| taps(k as f64 / PEAK_POINTS as f64))
            .collect();
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
        let points: Vec<[f32; TAPS]> = bank
            .iter()
            .chain(&between)
            .map(|taps| taps.map(|c| c as f32))
            .collect();
        // The taps of tap `first + i` of each point, for the left and the
        // right sample.
        let lanes = |first: usize| {
            let points = &points;
            move |i: usize| -> [f32; 16] { std::array::from_fn(|lane| points[lane / 2][first + i]) }
        };
        let center = std::array::from_fn(lanes(CENTER_FIRST));
        let sides = [
            std::array::from_fn(lanes(WIDE_FIRST)),
            std::array::from_fn(lanes(CENTER_FIRST + CENTER_TAPS)),
        ];
        let tail_gain_outside = |window: std::ops::Range<usize>| {
            points
                .iter()
                .map(|taps| {
                    let (mut total, mut tail) = (0.0f64, 0.0f64);
                    for (i, &c) in taps.iter().enumerate() {
                        total += (c as f64).abs();
                        if !window.contains(&i) {
                            tail += (c as f64).abs();
                        }
                    }
                    tail + 1e-4 * total
                })
                .fold(0.0, f64::max)
        };
        let tail_gain = tail_gain_outside(CENTER_FIRST..CENTER_FIRST + CENTER_TAPS);
        let wide_tail_gain = tail_gain_outside(WIDE_FIRST..WIDE_FIRST + WIDE_TAPS);
        Self {
            bank_pairs: std::array::from_fn(|phase| pairs(&bank[phase])),
            between_pairs: between.iter().map(|taps| pairs(taps)).collect(),
            down_pairs: pairs(&down),
            center,
            tail_gain,
            sides,
            wide_tail_gain,
            bank,
            between,
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
    sum_lanes(acc)
}

/// The left and the right sums of the 16 lanes of `dot_pairs`, in order.
#[inline(always)]
fn sum_lanes(acc: [f32; 16]) -> Stereo {
    let (mut left, mut right) = (0.0f32, 0.0f32);
    for j in (0..16).step_by(2) {
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
    acc.map(sum_lanes)
}

/// Vectors of four f32 lanes for `dot_pairs_windows`. Each multiply and
/// add rounds as the scalar `*` and `+` of each lane.
#[cfg(target_arch = "aarch64")]
mod vector {
    use std::arch::aarch64::{
        float32x4_t, vabsq_f32, vaddq_f32, vbslq_f32, vcgtq_f32, vdupq_n_f32, vld1q_f32, vmulq_f32,
        vst1q_f32,
    };
    pub(super) type Lanes = float32x4_t;
    // NEON is part of the aarch64 base: the arithmetic below is safe on
    // every aarch64 target.
    /// Four windows hold 16 sums in the 32 NEON registers.
    pub(super) const WINDOWS: usize = 4;
    #[inline(always)]
    pub(super) fn zero() -> Lanes {
        unsafe { vdupq_n_f32(0.0) }
    }
    /// # Safety
    /// `values` points to four readable f32 values.
    #[inline(always)]
    pub(super) unsafe fn load(values: *const f32) -> Lanes {
        unsafe { vld1q_f32(values) }
    }
    #[inline(always)]
    pub(super) fn add(a: Lanes, b: Lanes) -> Lanes {
        unsafe { vaddq_f32(a, b) }
    }
    #[inline(always)]
    pub(super) fn mul(a: Lanes, b: Lanes) -> Lanes {
        unsafe { vmulq_f32(a, b) }
    }
    /// In each lane, the absolute value of `b` where it is over `a`, and
    /// `a` elsewhere, also where `b` is NaN: `f32::max` of `a` and `|b|`
    /// for an `a` that is not NaN.
    #[inline(always)]
    pub(super) fn max_abs(a: Lanes, b: Lanes) -> Lanes {
        unsafe {
            let b = vabsq_f32(b);
            vbslq_f32(vcgtq_f32(b, a), b, a)
        }
    }
    #[inline(always)]
    pub(super) fn store(lanes: Lanes, values: &mut [f32; 4]) {
        unsafe { vst1q_f32(values.as_mut_ptr(), lanes) }
    }
}

/// Vectors of four f32 lanes for `dot_pairs_windows`. Each multiply and
/// add rounds as the scalar `*` and `+` of each lane.
#[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
mod vector {
    use std::arch::wasm32::{
        f32x4_abs, f32x4_add, f32x4_mul, f32x4_pmax, f32x4_splat, v128, v128_load, v128_store,
    };
    pub(super) type Lanes = v128;
    /// Two windows hold 8 sums: with the taps they fit in the 16 vector
    /// registers of an x86 host.
    pub(super) const WINDOWS: usize = 2;
    #[inline(always)]
    pub(super) fn zero() -> Lanes {
        f32x4_splat(0.0)
    }
    /// # Safety
    /// `values` points to four readable f32 values.
    #[inline(always)]
    pub(super) unsafe fn load(values: *const f32) -> Lanes {
        unsafe { v128_load(values as *const v128) }
    }
    #[inline(always)]
    pub(super) fn add(a: Lanes, b: Lanes) -> Lanes {
        f32x4_add(a, b)
    }
    #[inline(always)]
    pub(super) fn mul(a: Lanes, b: Lanes) -> Lanes {
        f32x4_mul(a, b)
    }
    /// In each lane, the absolute value of `b` where it is over `a`, and
    /// `a` elsewhere, also where `b` is NaN: `f32::max` of `a` and `|b|`
    /// for an `a` that is not NaN. `pmax(a, b)` is `a < b ? b : a`.
    #[inline(always)]
    pub(super) fn max_abs(a: Lanes, b: Lanes) -> Lanes {
        f32x4_pmax(a, f32x4_abs(b))
    }
    #[inline(always)]
    pub(super) fn store(lanes: Lanes, values: &mut [f32; 4]) {
        unsafe { v128_store(values.as_mut_ptr() as *mut v128, lanes) }
    }
}

/// `dot_pairs` of `taps` against `M` windows of `samples` that start `STEP`
/// values apart. Each window keeps the 16 lane sums of
/// `dot_pairs` and their order, so each result equals a call of
/// `dot_pairs`. The windows share each load of the taps. The sums of one
/// window form one chain of additions, and the core runs the chains of the
/// windows beside each other. The loads, multiplies and adds are explicit:
/// the loops of the same order that LLVM vectorized ran 0.8 to 1.5 times as
/// fast as `dot_pairs`, as the form of the source changed.
#[cfg(any(
    target_arch = "aarch64",
    all(target_arch = "wasm32", target_feature = "simd128")
))]
#[inline(always)]
fn dot_pairs_windows<const M: usize, const STEP: usize>(
    taps: &[f32],
    samples: &[f32],
) -> [Stereo; M] {
    const LANES: usize = 16;
    let n = taps.len();
    assert!(samples.len() >= n + (M - 1) * STEP);
    let chunks = n / LANES;
    let mut sums = [[vector::zero(); 4]; M];
    for k in 0..chunks {
        // SAFETY: `LANES * (k + 1)` is at most `n`, and the assertion holds
        // the last window within `samples`.
        let t: [vector::Lanes; 4] =
            std::array::from_fn(|q| unsafe { vector::load(taps.as_ptr().add(LANES * k + 4 * q)) });
        for (m, sums) in sums.iter_mut().enumerate() {
            let x = unsafe { samples.as_ptr().add(m * STEP + LANES * k) };
            for (q, sum) in sums.iter_mut().enumerate() {
                *sum = vector::add(
                    *sum,
                    vector::mul(t[q], unsafe { vector::load(x.add(4 * q)) }),
                );
            }
        }
    }
    let mut acc = sums.map(|sums| {
        let mut acc = [0.0f32; LANES];
        for (lanes, values) in sums.into_iter().zip(acc.as_chunks_mut::<4>().0) {
            vector::store(lanes, values);
        }
        acc
    });
    for i in LANES * chunks..n {
        for (m, acc) in acc.iter_mut().enumerate() {
            acc[i % LANES] += taps[i] * samples[m * STEP + i];
        }
    }
    acc.map(sum_lanes)
}

/// The larger of `a` and `b`, where `a` is not NaN and neither value is
/// -0.0: the value of `max` with one comparison and one select. `max` and
/// the comparison pass a NaN in `b` by. In WASM, `max` also tests each
/// value for NaN (D327, D329).
#[inline(always)]
pub(crate) fn larger<T: PartialOrd>(a: T, b: T) -> T {
    if b > a {
        b
    } else {
        a
    }
}

/// The smaller of `a` and `b` under the conditions of `larger`: the value
/// of `min` (D329).
#[inline(always)]
pub(crate) fn smaller<T: PartialOrd>(a: T, b: T) -> T {
    if b < a {
        b
    } else {
        a
    }
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

/// The interpolated point of `input` at `base` with `taps`, and `pairs`,
/// the same taps in interleaved f32.
#[inline(always)]
fn point(input: &[Stereo], base: usize, pairs: &[f32], taps: &[f64; TAPS]) -> Stereo {
    let radius = RADIUS as usize;
    if base >= radius && base + radius < input.len() {
        return dot_pairs(pairs, input[base - radius..=base + radius].as_flattened());
    }
    let mut out = [0.0f64; 2];
    for (i, coefficient) in taps.iter().enumerate() {
        let index = base as isize + i as isize - RADIUS;
        if let Some(frame) = usize::try_from(index).ok().and_then(|n| input.get(n)) {
            out[0] += *coefficient * frame[0] as f64;
            out[1] += *coefficient * frame[1] as f64;
        }
    }
    out.map(|v| v as f32)
}

/// Adds to `sums` the products of `taps` and the frames of `input` from tap
/// `first` of the frame at `base`: each lane of each tap holds the tap of
/// one point for the left or the right sample.
#[inline(always)]
fn add_lane_sums<const N: usize>(
    taps: &[[f32; 16]; N],
    input: &[Stereo],
    base: usize,
    first: usize,
    sums: &mut [f32; 16],
) {
    let samples = &input[base - RADIUS as usize + first..][..N];
    for (taps, frame) in taps.iter().zip(samples) {
        let x: [f32; 16] = std::array::from_fn(|lane| frame[lane % 2]);
        for lane in 0..16 {
            sums[lane] += taps[lane] * x[lane];
        }
    }
}

/// The largest absolute value of the lanes of the sums of each frame.
#[inline(always)]
fn lanes_peak<const M: usize>(sums: &[[f32; 16]; M]) -> f32 {
    let mut peaks = [0.0f32; 16];
    for sums in sums {
        // A comparison, where `f32::max` would keep the lanes from running
        // in vectors: the samples are finite.
        for lane in 0..16 {
            let sum = sums[lane].abs();
            peaks[lane] = if sum > peaks[lane] { sum } else { peaks[lane] };
        }
    }
    peaks.into_iter().fold(0.0, larger)
}

impl Oversampler {
    pub(crate) fn at(&self, input: &[Stereo], base: usize, phase: usize) -> Stereo {
        point(input, base, &self.bank_pairs[phase], &self.bank[phase])
    }
    /// `interpolated_peak` of each frame of `frames`, in order, given to
    /// `each`. Frames whose taps are all in the signal go `vector::WINDOWS`
    /// at a time, and their windows share each load of the taps
    /// (`dot_pairs_windows`, D239). `skip` holds the largest sample under
    /// the taps of the frames and a limit: a group of frames whose taps are
    /// all in the signal and whose bound is at most the limit skips `each`,
    /// as none of its points is above the limit. The bound is that of
    /// `group_bound` (D292, D303). Returns the largest bound of the skipped
    /// groups, or −1 when the call skipped none.
    #[inline(always)]
    fn each_interpolated_peak(
        &self,
        input: &[Stereo],
        frames: std::ops::Range<usize>,
        skip: Option<(f64, f64)>,
        mut each: impl FnMut(usize, f32),
    ) -> f64 {
        let radius = RADIUS as usize;
        let (near, limit) = skip.unwrap_or((0.0, -1.0));
        let mut skipped = -1.0f64;
        let mut base = frames.start;
        while base < frames.end {
            #[cfg(any(
                target_arch = "aarch64",
                all(target_arch = "wasm32", target_feature = "simd128")
            ))]
            {
                const M: usize = vector::WINDOWS;
                if base + M <= frames.end && base >= radius && base + M - 1 + radius < input.len() {
                    if let Some(bound) = self.group_bound::<M>(input, base, near, limit) {
                        skipped = skipped.max(bound);
                        base += M;
                        continue;
                    }
                    let samples = input[base - radius..=base + M - 1 + radius].as_flattened();
                    let mut peaks = [0.0f32; M];
                    for taps in self.bank_pairs.iter().chain(&self.between_pairs) {
                        let points = dot_pairs_windows::<M, 2>(taps, samples);
                        for (peak, point) in peaks.iter_mut().zip(points) {
                            *peak = point.iter().fold(*peak, |peak, &x| larger(peak, x.abs()));
                        }
                    }
                    for (k, peak) in peaks.into_iter().enumerate() {
                        each(base + k, peak);
                    }
                    base += M;
                    continue;
                }
            }
            if base >= radius && base + radius < input.len() {
                if let Some(bound) = self.group_bound::<1>(input, base, near, limit) {
                    skipped = skipped.max(bound);
                    base += 1;
                    continue;
                }
            }
            each(base, self.interpolated_peak(input, base));
            base += 1;
        }
        skipped
    }
    /// The bound of the points of the `M` frames from `base`, whose taps
    /// are all in `input`, when it is at most `limit`. `near` is the largest
    /// sample under their taps. The bound is the largest absolute central
    /// sum (`center`) plus `tail_gain` times `near` (D292). Over the limit,
    /// the sums of `sides` extend the central sums to the wide taps, and
    /// the bound is the largest absolute wide sum plus `wide_tail_gain`
    /// times `near` (D303).
    #[inline(always)]
    fn group_bound<const M: usize>(
        &self,
        input: &[Stereo],
        base: usize,
        near: f64,
        limit: f64,
    ) -> Option<f64> {
        let wide_tail = near * self.wide_tail_gain;
        if wide_tail > limit {
            return None;
        }
        let mut sums = [[0.0f32; 16]; M];
        for (k, sums) in sums.iter_mut().enumerate() {
            add_lane_sums(&self.center, input, base + k, CENTER_FIRST, sums);
        }
        let bound = lanes_peak(&sums) as f64 + near * self.tail_gain;
        if bound <= limit {
            return Some(bound);
        }
        for (k, sums) in sums.iter_mut().enumerate() {
            add_lane_sums(&self.sides[0], input, base + k, WIDE_FIRST, sums);
            let after = CENTER_FIRST + CENTER_TAPS;
            add_lane_sums(&self.sides[1], input, base + k, after, sums);
        }
        let bound = lanes_peak(&sums) as f64 + wide_tail;
        (bound <= limit).then_some(bound)
    }
    /// The largest absolute value of the `PEAK_POINTS` interpolated points of
    /// both channels at `base`: the 4x points and the points between them.
    #[inline(always)]
    fn interpolated_peak(&self, input: &[Stereo], base: usize) -> f32 {
        let radius = RADIUS as usize;
        let largest = |frames: [Stereo; 2], peak: f32| {
            frames
                .iter()
                .flatten()
                .fold(peak, |peak, &sample| peak.max(sample.abs()))
        };
        if base >= radius && base + radius < input.len() {
            let samples = input[base - radius..=base + radius].as_flattened();
            let mut peak = largest(
                dot_pairs_two(&self.bank_pairs[0], &self.bank_pairs[1], samples),
                0.0,
            );
            peak = largest(
                dot_pairs_two(&self.bank_pairs[2], &self.bank_pairs[3], samples),
                peak,
            );
            for two in self.between_pairs.chunks_exact(2) {
                peak = largest(dot_pairs_two(&two[0], &two[1], samples), peak);
            }
            return peak;
        }
        let banks = self.bank_pairs.iter().zip(&self.bank);
        banks
            .chain(self.between_pairs.iter().zip(&self.between))
            .flat_map(|(pairs, taps)| point(input, base, pairs, taps))
            .fold(0.0f32, |peak, sample| peak.max(sample.abs()))
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
        self.peak_core_from(input, core, 0.0)
    }
    /// `peak_core` whose chunks start from `floor`, at most the peak of the
    /// signal: a block whose bound is at most `floor` holds no sample that
    /// raises the peak of the signal. Returns at least `floor`. With the
    /// sample peak of the signal as `floor`, each core measures the blocks
    /// that `peak` of the whole signal measures (D248).
    pub(crate) fn peak_core_from(
        &self,
        input: &[Stereo],
        core: std::ops::Range<usize>,
        floor: f64,
    ) -> f64 {
        let mut peak = sample_peak(&input[core.clone()]).max(floor);
        if core.is_empty() {
            return peak;
        }
        // An interpolated sample is at most the largest absolute tap sum times
        // the largest input sample under the taps. A block whose bound cannot
        // exceed the peak found so far holds no larger sample, so the result
        // equals the exhaustive search.
        let gain = self.bound_gain();
        // Each chunk starts from the sample peak, so its skipped blocks and
        // its maximum do not depend on the other chunks.
        let sample_peak = peak;
        let chunk_peaks = chunked(core.len(), CHUNK, |range| {
            let range = range.start + core.start..range.end + core.start;
            let near = BlockPeaks::near(input, range.clone());
            let mut peak = sample_peak;
            for start in range.clone().step_by(PEAK_BLOCK) {
                let bound = near.bound(start);
                if bound * gain <= peak {
                    continue;
                }
                self.each_interpolated_peak(
                    input,
                    start..(start + PEAK_BLOCK).min(range.end),
                    Some((bound, peak)),
                    |_, x| {
                        peak = larger(peak, x as f64);
                    },
                );
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
        self.peak_scan_core(input, 0..input.len(), ceiling, 0.0)
    }
    /// `peak_scan` of the frames of `input` in `core`, whose chunks start
    /// from `floor` as in `peak_core_from`. Each core position needs
    /// `RADIUS` frames of `input` on each side, or the end of the signal.
    /// The overs are frames of `input`, and the blocks are blocks of the
    /// core (D248).
    pub(crate) fn peak_scan_core(
        &self,
        input: &[Stereo],
        core: std::ops::Range<usize>,
        ceiling: f64,
        floor: f64,
    ) -> PeakScan {
        let sample_peak = sample_peak(&input[core.clone()]).max(floor);
        if core.is_empty() {
            return PeakScan {
                peak: sample_peak,
                overs: Vec::new(),
                blocks: Vec::new(),
            };
        }
        let gain = self.bound_gain();
        let parts = chunked(core.len(), CHUNK, |range| {
            let range = range.start + core.start..range.end + core.start;
            let near = BlockPeaks::near(input, range.clone());
            let mut peak = sample_peak;
            let mut blocks = Vec::with_capacity(range.len().div_ceil(PEAK_BLOCK));
            let mut overs = Vec::new();
            for start in range.clone().step_by(PEAK_BLOCK) {
                let bound = near.bound(start);
                let limit = peak.min(ceiling);
                if bound * gain <= limit {
                    blocks.push(-bound_f32(bound * gain));
                    continue;
                }
                let mut largest = 0.0f32;
                let block = start..(start + PEAK_BLOCK).min(range.end);
                // A skipped frame holds no sample above the ceiling when
                // each sample under the taps is at most the limit.
                let skip = (bound <= limit).then_some((bound, limit));
                let skipped =
                    self.each_interpolated_peak(input, block, skip, |base, interpolated| {
                        // The samples of a render are finite (D329).
                        largest = larger(largest, interpolated);
                        let frame_peak = larger(
                            larger(input[base][0].abs(), input[base][1].abs()),
                            interpolated,
                        ) as f64;
                        if frame_peak > ceiling {
                            overs.push((base, ceiling / frame_peak));
                        }
                    });
                peak = peak.max(largest as f64);
                blocks.push(if skipped < 0.0 {
                    largest
                } else {
                    -bound_f32(skipped.max(largest as f64))
                });
            }
            (peak, blocks, overs)
        });
        let mut scan = PeakScan {
            peak: sample_peak,
            overs: Vec::new(),
            blocks: Vec::with_capacity(parts.len()),
        };
        for (peak, blocks, overs) in parts {
            scan.peak = scan.peak.max(peak);
            scan.blocks.push(blocks);
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
        self.peak_after_core(audio, 0..audio.len(), scan, changed, 0.0)
    }

    /// `peak_after` of the frames of `audio` in `core`, from `scan`, the
    /// `peak_scan_core` of the same core, whose chunks start from `floor`
    /// as in `peak_core_from`. The ranges of `changed` are frames of
    /// `audio` (D248).
    pub(crate) fn peak_after_core(
        &self,
        audio: &[Stereo],
        core: std::ops::Range<usize>,
        scan: &PeakScan,
        changed: &[std::ops::Range<usize>],
        floor: f64,
    ) -> f64 {
        let sample_peak = sample_peak(&audio[core.clone()]).max(floor);
        if core.is_empty() {
            return sample_peak;
        }
        let mut fresh = vec![false; core.len().div_ceil(PEAK_BLOCK)];
        for range in changed {
            let (start, end) = (range.start.max(core.start), range.end.min(core.end));
            if start < end {
                let blocks =
                    (start - core.start) / PEAK_BLOCK..(end - core.start).div_ceil(PEAK_BLOCK);
                fresh[blocks].fill(true);
            }
        }
        let gain = self.bound_gain();
        let chunk_peaks = chunked(core.len(), CHUNK, |range| {
            let range = range.start + core.start..range.end + core.start;
            let near = BlockPeaks::near(audio, range.clone());
            let blocks = &scan.blocks[(range.start - core.start) / CHUNK];
            let mut peak = sample_peak;
            for start in range.clone().step_by(PEAK_BLOCK) {
                let measured = blocks[(start - range.start) / PEAK_BLOCK];
                let kept = !fresh[(start - core.start) / PEAK_BLOCK];
                if kept && measured >= 0.0 {
                    peak = peak.max(measured as f64);
                    continue;
                }
                let bound = near.bound(start);
                if bound * gain <= peak || (kept && -measured as f64 <= peak) {
                    continue;
                }
                self.each_interpolated_peak(
                    audio,
                    start..(start + PEAK_BLOCK).min(range.end),
                    Some((bound, peak)),
                    |_, x| {
                        peak = larger(peak, x as f64);
                    },
                );
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
            .chain(&self.between)
            .map(|taps| taps.iter().map(|c| c.abs()).sum::<f64>())
            .fold(0.0, f64::max)
            * (1.0 + 1e-6)
    }

    /// The frames of `audio` in `range` whose true peak is above `ceiling`,
    /// each with the gain that brings its true peak to `ceiling`.
    pub(crate) fn overs(
        &self,
        audio: &[Stereo],
        range: std::ops::Range<usize>,
        ceiling: f64,
    ) -> Vec<(usize, f64)> {
        let gain = self.bound_gain();
        let radius = RADIUS as usize;
        let offset = range.start;
        chunked(range.len(), CHUNK, |chunk| {
            let mut found = Vec::new();
            let chunk = chunk.start + offset..chunk.end + offset;
            for start in chunk.clone().step_by(PEAK_BLOCK) {
                let end = (start + PEAK_BLOCK).min(chunk.end);
                // As in `peak_core`: no frame of a block whose bound is
                // under the ceiling is over it.
                let bound = sample_peak(
                    &audio[start.saturating_sub(radius)..(end + radius).min(audio.len())],
                );
                if bound * gain <= ceiling {
                    continue;
                }
                let skip = (bound <= ceiling).then_some((bound, ceiling));
                self.each_interpolated_peak(audio, start..end, skip, |base, interpolated| {
                    let peak = audio[base][0]
                        .abs()
                        .max(audio[base][1].abs())
                        .max(interpolated) as f64;
                    if peak > ceiling {
                        found.push((base, ceiling / peak));
                    }
                });
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

    /// `down_at_window` at `count` bases from `first`, 4 frames apart. The
    /// bases whose taps are all in the signal go `vector::WINDOWS` at a
    /// time through `dot_pairs_windows`.
    pub(crate) fn down_range_at_window(
        &self,
        input: &[Stereo],
        window_start: usize,
        total_frames: usize,
        first: usize,
        count: usize,
    ) -> Vec<Stereo> {
        let mut output = Vec::with_capacity(count);
        let mut k = 0;
        while k < count {
            let base = first + 4 * k;
            #[cfg(any(
                target_arch = "aarch64",
                all(target_arch = "wasm32", target_feature = "simd128")
            ))]
            {
                const M: usize = vector::WINDOWS;
                let radius = DOWN_RADIUS as usize;
                let last = base + 4 * (M - 1);
                if k + M <= count && base >= radius && last + radius < total_frames {
                    let (low, high) = (base - radius - window_start, last + radius - window_start);
                    output.extend(dot_pairs_windows::<M, 8>(
                        &self.down_pairs,
                        input[low..=high].as_flattened(),
                    ));
                    k += M;
                    continue;
                }
            }
            output.push(self.down_at_window(input, window_start, total_frames, base));
            k += 1;
        }
        output
    }

    pub(crate) fn down_radius(&self) -> usize {
        DOWN_RADIUS as usize
    }
}

/// Frames of a block of the bound of the peak estimate.
const PEAK_BLOCK: usize = 32;

/// The true-peak scan of a render (`Oversampler::peak_scan`).
pub(crate) struct PeakScan {
    /// `Oversampler::peak` of the render.
    pub(crate) peak: f64,
    /// The frames whose true peak is above the ceiling of the scan, each with
    /// the gain that brings its true peak to the ceiling.
    pub(crate) overs: Vec<(usize, f64)>,
    /// The largest absolute interpolated sample of each block of
    /// `PEAK_BLOCK` frames, or the negative of a bound of its interpolated
    /// samples for a block whose points the scan did not each measure, for
    /// each chunk of `CHUNK` frames (D292).
    blocks: Vec<Vec<f32>>,
}

/// The smallest f32 that is at least `value`.
fn bound_f32(value: f64) -> f32 {
    let rounded = value as f32;
    if (rounded as f64) < value {
        rounded.next_up()
    } else {
        rounded
    }
}

/// The largest absolute sample of `input`.
pub(crate) fn sample_peak(input: &[Stereo]) -> f64 {
    peak_abs(input.as_flattened()) as f64
}

/// The largest absolute value of `values`, or 0: the fold of `f32::max`
/// from 0. `max` takes the larger value and passes a NaN by, so the order
/// of the fold leaves the result the same, and lanes of 16 values take the
/// values of each step (D308). The f32 result as f64 is the fold of the
/// values as f64.
pub(crate) fn peak_abs(values: &[f32]) -> f32 {
    #[allow(unused_mut)]
    let (mut peak, mut values) = (0.0f32, values);
    #[cfg(any(
        target_arch = "aarch64",
        all(target_arch = "wasm32", target_feature = "simd128")
    ))]
    {
        const LANES: usize = 16;
        let chunks = values.len() / LANES;
        let mut lanes = [vector::zero(); 4];
        for k in 0..chunks {
            for (q, lane) in lanes.iter_mut().enumerate() {
                // SAFETY: `LANES * (k + 1)` is at most the count of values.
                let x = unsafe { vector::load(values.as_ptr().add(LANES * k + 4 * q)) };
                *lane = vector::max_abs(*lane, x);
            }
        }
        let mut all = [0.0f32; LANES];
        for (lane, out) in lanes.into_iter().zip(all.as_chunks_mut::<4>().0) {
            vector::store(lane, out);
        }
        peak = all.into_iter().fold(peak, f32::max);
        values = &values[LANES * chunks..];
    }
    values.iter().fold(peak, |a, &b| a.max(b.abs()))
}

/// Frames of each chunk of the true-peak scans, a multiple of
/// `PEAK_BLOCK`. The chunks run on the available cores.
const CHUNK: usize = 1 << 16;

/// The largest absolute sample of each block of `PEAK_BLOCK` frames of a
/// signal near a range of frames: the blocks that hold a frame within
/// `RADIUS` frames of the range.
struct BlockPeaks {
    /// The block of the first peak.
    first: usize,
    /// The index of the last block of the signal.
    last: usize,
    peaks: Vec<f32>,
}
impl BlockPeaks {
    /// The block peaks of `input` near the blocks of frames that start
    /// `PEAK_BLOCK` frames apart from the start of `range`, a range with
    /// frames.
    fn near(input: &[Stereo], range: std::ops::Range<usize>) -> Self {
        let radius = RADIUS as usize;
        let last = (input.len() - 1) / PEAK_BLOCK;
        let first = range.start.saturating_sub(radius) / PEAK_BLOCK;
        let final_start = range.start + (range.len() - 1) / PEAK_BLOCK * PEAK_BLOCK;
        let end = ((final_start + PEAK_BLOCK - 1 + radius) / PEAK_BLOCK).min(last) + 1;
        let frames = &input[first * PEAK_BLOCK..(end * PEAK_BLOCK).min(input.len())];
        // The largest absolute f32 sample is an f32.
        let peaks = frames
            .chunks(PEAK_BLOCK)
            .map(|block| sample_peak(block) as f32)
            .collect();
        Self { first, last, peaks }
    }
    /// The largest absolute sample under the taps of the block of frames
    /// that starts at `start`: the blocks within `RADIUS` frames of it.
    fn bound(&self, start: usize) -> f64 {
        let radius = RADIUS as usize;
        let first = start.saturating_sub(radius) / PEAK_BLOCK;
        let last = ((start + PEAK_BLOCK - 1 + radius) / PEAK_BLOCK).min(self.last);
        self.peaks[first - self.first..=last - self.first]
            .iter()
            .fold(0.0, |bound, &peak| f64::max(bound, peak as f64))
    }
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
    /// The last gain under 1 that `push` took and its reduction in dB: a
    /// quarter of the gains of a render repeat the gain before them (D294).
    last: (f32, f64),
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
            last: (f32::NAN, 0.0),
        }
    }
}

impl LimiterStatsAccumulator {
    /// Returns the reduction in dB for the pushed gain.
    pub fn push(&mut self, gain: f32) -> f64 {
        let g = gain as f64;
        self.minimum = smaller(self.minimum, g);
        // -db(1.0) is -0.0; most samples carry no reduction.
        let reduction = if g == 1.0 {
            -0.0
        } else if gain == self.last.0 {
            self.last.1
        } else {
            let reduction = -db(g);
            self.last = (gain, reduction);
            reduction
        };
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

/// Running maximum, or running minimum with `MAXIMUM` false, over a
/// trailing window of samples (monotonic deque). The deque holds at most
/// one entry for each sample of the window and the new one, so it never
/// grows after `new`.
struct RunningExtremum<const MAXIMUM: bool> {
    deque: std::collections::VecDeque<(usize, f64)>,
    window: usize,
}

impl<const MAXIMUM: bool> RunningExtremum<MAXIMUM> {
    fn new(window: usize) -> Self {
        let window = window.max(1);
        Self {
            deque: std::collections::VecDeque::with_capacity(window + 1),
            window,
        }
    }
    // A call in the frame loop of the limiter kept the state of the loop
    // in memory around the call (D312).
    #[inline(always)]
    fn push(&mut self, index: usize, value: f64) -> f64 {
        while self
            .deque
            .back()
            .is_some_and(|b| if MAXIMUM { b.1 <= value } else { b.1 >= value })
        {
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

/// Three moving averages of one length in cascade, each with a running sum,
/// in one ring. Each average takes the output of the one before it. Rings
/// start at unity gain (D300).
struct Smoothing {
    ring: Vec<[f64; 3]>,
    position: usize,
    sums: [f64; 3],
}

impl Smoothing {
    fn new(length: usize) -> Self {
        let length = length.max(1);
        Self {
            ring: vec![[1.0; 3]; length],
            position: 0,
            sums: [length as f64; 3],
        }
    }
    fn fill(&mut self, value: f64) {
        self.ring.fill([value; 3]);
        self.sums = [value * self.ring.len() as f64; 3];
    }
    fn push(&mut self, mut value: f64) -> f64 {
        let length = self.ring.len() as f64;
        let slot = &mut self.ring[self.position];
        for (old, sum) in slot.iter_mut().zip(&mut self.sums) {
            let previous = std::mem::replace(old, value);
            *sum += value - previous;
            value = *sum / length;
        }
        self.position = (self.position + 1) % self.ring.len();
        value
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
    sustain: RunningExtremum<false>,
    lookahead: RunningExtremum<true>,
    smoothing: Smoothing,
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
            sustain: RunningExtremum::new((SUSTAIN_WINDOW_MS * per_ms).round() as usize),
            lookahead: RunningExtremum::new(3 * window),
            smoothing: Smoothing::new(window),
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
    #[inline(always)]
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
                larger(self.fast_db, self.slow_db)
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
            self.smoothing.fill(g);
        }
        g = self.smoothing.push(g);
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
        let y = self.level(a);
        (y.copysign(x), db(a / y))
    }
    /// The shaped sample of `shape` alone.
    pub fn shaped(&self, x: f64) -> f64 {
        let a = x.abs();
        if a <= self.knee_start {
            return x;
        }
        self.level(a).copysign(x)
    }
    /// The shaped magnitude of `a`, a magnitude above the knee.
    fn level(&self, a: f64) -> f64 {
        let d = (a - self.knee_start).min(self.width);
        let y = self.knee_start + d - d * d / (2.0 * self.width);
        y.min(self.ceiling)
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

/// `loudness` of the frames of `audio`, read in blocks of 65,536 frames.
pub(crate) fn loudness_read(
    audio: &mut impl crate::scratch::Frames,
    rate: u32,
) -> crate::audio_io::Result<Option<f64>> {
    const BLOCK: usize = 65_536;
    let frames = audio.len();
    let mut meter = LoudnessMeter::new(rate);
    let mut buffer = Vec::new();
    for start in (0..frames).step_by(BLOCK) {
        meter.push(audio.window(start, BLOCK.min(frames - start), &mut buffer)?);
    }
    Ok(meter.finish())
}

/// Two f64 lanes. Each multiply, add and subtract rounds as the scalar
/// operation of each lane, so a filter of two lanes gives the outputs of two
/// scalar filters (D321).
pub(crate) mod pair {
    #[cfg(target_arch = "aarch64")]
    mod lanes {
        use std::arch::aarch64::{
            float64x2_t, vaddq_f64, vdupq_n_f64, vgetq_lane_f64, vld1q_f64, vmulq_f64, vsubq_f64,
        };
        pub type Pair = float64x2_t;
        // NEON is part of the aarch64 base: the arithmetic below is safe on
        // every aarch64 target.
        #[inline(always)]
        pub fn new(first: f64, second: f64) -> Pair {
            let values = [first, second];
            unsafe { vld1q_f64(values.as_ptr()) }
        }
        #[inline(always)]
        pub fn splat(value: f64) -> Pair {
            unsafe { vdupq_n_f64(value) }
        }
        #[inline(always)]
        pub fn add(a: Pair, b: Pair) -> Pair {
            unsafe { vaddq_f64(a, b) }
        }
        #[inline(always)]
        pub fn sub(a: Pair, b: Pair) -> Pair {
            unsafe { vsubq_f64(a, b) }
        }
        #[inline(always)]
        pub fn mul(a: Pair, b: Pair) -> Pair {
            unsafe { vmulq_f64(a, b) }
        }
        #[inline(always)]
        pub fn values(pair: Pair) -> [f64; 2] {
            unsafe { [vgetq_lane_f64::<0>(pair), vgetq_lane_f64::<1>(pair)] }
        }
    }

    #[cfg(all(target_arch = "wasm32", target_feature = "simd128"))]
    mod lanes {
        use std::arch::wasm32::{
            f64x2, f64x2_add, f64x2_extract_lane, f64x2_mul, f64x2_splat, f64x2_sub, v128,
        };
        pub type Pair = v128;
        #[inline(always)]
        pub fn new(first: f64, second: f64) -> Pair {
            f64x2(first, second)
        }
        #[inline(always)]
        pub fn splat(value: f64) -> Pair {
            f64x2_splat(value)
        }
        #[inline(always)]
        pub fn add(a: Pair, b: Pair) -> Pair {
            f64x2_add(a, b)
        }
        #[inline(always)]
        pub fn sub(a: Pair, b: Pair) -> Pair {
            f64x2_sub(a, b)
        }
        #[inline(always)]
        pub fn mul(a: Pair, b: Pair) -> Pair {
            f64x2_mul(a, b)
        }
        #[inline(always)]
        pub fn values(pair: Pair) -> [f64; 2] {
            [f64x2_extract_lane::<0>(pair), f64x2_extract_lane::<1>(pair)]
        }
    }

    #[cfg(not(any(
        target_arch = "aarch64",
        all(target_arch = "wasm32", target_feature = "simd128")
    )))]
    mod lanes {
        pub type Pair = [f64; 2];
        #[inline(always)]
        pub fn new(first: f64, second: f64) -> Pair {
            [first, second]
        }
        #[inline(always)]
        pub fn splat(value: f64) -> Pair {
            [value; 2]
        }
        #[inline(always)]
        pub fn add(a: Pair, b: Pair) -> Pair {
            [a[0] + b[0], a[1] + b[1]]
        }
        #[inline(always)]
        pub fn sub(a: Pair, b: Pair) -> Pair {
            [a[0] - b[0], a[1] - b[1]]
        }
        #[inline(always)]
        pub fn mul(a: Pair, b: Pair) -> Pair {
            [a[0] * b[0], a[1] * b[1]]
        }
        #[inline(always)]
        pub fn values(pair: Pair) -> [f64; 2] {
            pair
        }
    }

    pub(crate) use lanes::{add, mul, new, splat, sub, values, Pair};

    /// `dsp::Biquad` of two lanes with the same coefficients.
    #[derive(Clone, Copy)]
    pub(crate) struct Biquad {
        b: [Pair; 3],
        a: [Pair; 2],
        state: [Pair; 2],
    }

    impl Biquad {
        /// `filter` with the state `states[0]` in lane 0 and `states[1]` in
        /// lane 1.
        #[inline(always)]
        pub(crate) fn new(filter: super::Biquad, states: [[f64; 2]; 2]) -> Self {
            Self {
                b: filter.b.map(splat),
                a: filter.a.map(splat),
                state: [0, 1].map(|k| new(states[0][k], states[1][k])),
            }
        }
        /// `dsp::Biquad::tick` of each lane.
        #[inline(always)]
        pub(crate) fn tick(&mut self, x: Pair) -> Pair {
            let y = add(mul(self.b[0], x), self.state[0]);
            self.state[0] = add(sub(mul(self.b[1], x), mul(self.a[0], y)), self.state[1]);
            self.state[1] = sub(mul(self.b[2], x), mul(self.a[1], y));
            y
        }
        /// The state of the filter of each lane.
        #[inline(always)]
        pub(crate) fn states(&self) -> [[f64; 2]; 2] {
            let [first, second] = self.state.map(values);
            [[first[0], second[0]], [first[1], second[1]]]
        }
    }
}

/// Integrated loudness of audio that arrives in blocks; the blocks may have
/// any length.
pub struct LoudnessMeter {
    /// The K-weighting stages of the left channel in lane 0 and of the right
    /// channel in lane 1 (D322).
    shelf: pair::Biquad,
    highpass: pair::Biquad,
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
            shelf: pair::Biquad::new(shelf, [[0.0; 2]; 2]),
            highpass: pair::Biquad::new(highpass, [[0.0; 2]; 2]),
            bucket: rate as usize / 10,
            buckets: Vec::new(),
            energy: 0.0,
            frames: 0,
        }
    }

    pub fn push(&mut self, audio: &[Stereo]) {
        // The filters and the sums are locals of the call. The frame that
        // ends the bucket takes the place of a remainder for each frame: it
        // is the first multiple of `bucket` after the frames so far (D322).
        let (mut shelf, mut highpass) = (self.shelf, self.highpass);
        let (mut energy, mut frames) = (self.energy, self.frames);
        let bucket = self.bucket;
        let mut end = match bucket {
            0 => usize::MAX,
            _ => (frames / bucket + 1) * bucket,
        };
        for frame in audio {
            let x = pair::new(frame[0] as f64, frame[1] as f64);
            let [left, right] = pair::values(highpass.tick(shelf.tick(x)));
            energy += left * left;
            energy += right * right;
            frames += 1;
            if frames == end {
                self.buckets.push(energy / bucket as f64);
                energy = 0.0;
                end += bucket;
            }
        }
        (self.shelf, self.highpass) = (shelf, highpass);
        (self.energy, self.frames) = (energy, frames);
    }

    /// The mean energies of the 100 ms buckets so far, which the meter
    /// gives up. `gated_loudness` of the buckets of consecutive spans is
    /// the loudness of the whole.
    pub fn take_buckets(&mut self) -> Vec<f64> {
        std::mem::take(&mut self.buckets)
    }

    pub fn finish(self) -> Option<f64> {
        gated_loudness(&self.buckets)
    }
}

/// Integrated loudness of the mean energies of consecutive 100 ms buckets
/// of K-weighted audio, with the gates of ITU-R BS.1770.
pub fn gated_loudness(buckets: &[f64]) -> Option<f64> {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `larger` and `smaller` equal `max` and `min` when the first value is
    /// not NaN and neither value is -0.0, also with NaN as the second
    /// value, bit for bit (D327, D329).
    #[test]
    fn larger_and_smaller_equal_max_and_min() {
        let values = [
            f64::NEG_INFINITY,
            -2.5,
            0.0,
            1e-300,
            0.5,
            1.0,
            3.0,
            f64::MAX,
            f64::INFINITY,
        ];
        for &a in &values {
            for &b in values.iter().chain(&[f64::NAN]) {
                assert_eq!(larger(a, b).to_bits(), a.max(b).to_bits(), "{a} {b}");
                assert_eq!(smaller(a, b).to_bits(), a.min(b).to_bits(), "{a} {b}");
                let (a, b) = (a as f32, b as f32);
                assert_eq!(larger(a, b).to_bits(), a.max(b).to_bits(), "{a} {b}");
                assert_eq!(smaller(a, b).to_bits(), a.min(b).to_bits(), "{a} {b}");
            }
        }
    }

    /// The meter in pairs gives the buckets and the energy of a scalar
    /// filter for each channel, bit for bit, over blocks of any length
    /// (D322).
    #[test]
    fn the_meter_in_pairs_equals_a_filter_for_each_channel() {
        let mut state = 0x2545_f491u32;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as f32 / u32::MAX as f32 * 1.6 - 0.8
        };
        let audio: Vec<Stereo> = (0..48_000 * 3 + 4_801).map(|_| [next(), next()]).collect();
        for rate in [44_100, 48_000] {
            let (shelf, highpass) = k_weighting(rate);
            let (mut shelves, mut highpasses) = ([shelf; 2], [highpass; 2]);
            let bucket = rate as usize / 10;
            let (mut buckets, mut energy) = (Vec::new(), 0.0f64);
            for (n, frame) in audio.iter().enumerate() {
                for (c, sample) in frame.iter().enumerate() {
                    let filtered = highpasses[c].tick(shelves[c].tick(*sample as f64));
                    energy += filtered * filtered;
                }
                if (n + 1).is_multiple_of(bucket) {
                    buckets.push(energy / bucket as f64);
                    energy = 0.0;
                }
            }
            for block in [1, 997, 4_800, 65_536] {
                let mut meter = LoudnessMeter::new(rate);
                audio.chunks(block).for_each(|frames| meter.push(frames));
                assert_eq!(meter.energy.to_bits(), energy.to_bits());
                assert_eq!(
                    meter
                        .take_buckets()
                        .iter()
                        .map(|v| v.to_bits())
                        .collect::<Vec<_>>(),
                    buckets.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
                );
            }
        }
    }

    #[test]
    fn the_peak_in_lanes_equals_the_fold_of_each_value() {
        // NaN, infinity and both zeros, at each place of a lane and of the
        // values after the lanes.
        let specials = [
            f32::NAN,
            -0.0,
            0.0,
            f32::INFINITY,
            -f32::INFINITY,
            -1.5,
            1e-40,
        ];
        for count in 0..50 {
            for (k, &special) in specials.iter().enumerate() {
                for at in 0..count.max(1) {
                    let mut values: Vec<f32> = (0..count)
                        .map(|i| ((i * 37 + k * 11) % 23) as f32 * 0.1 - 1.1)
                        .collect();
                    if at < count {
                        values[at] = special;
                    }
                    let fold = values.iter().fold(0.0f64, |a, &b| a.max(b.abs() as f64));
                    assert_eq!((peak_abs(&values) as f64).to_bits(), fold.to_bits());
                }
            }
        }
        let nans = [f32::NAN; 40];
        assert_eq!(peak_abs(&nans).to_bits(), 0.0f32.to_bits());
    }

    #[test]
    fn the_smoothing_equals_three_moving_averages_in_cascade() {
        // Three rings with their own positions and sums, as the limiter had
        // them before D300.
        let length = 7;
        let mut rings = [vec![1.0f64; length], vec![1.0; length], vec![1.0; length]];
        let mut sums = [length as f64; 3];
        let mut smoothing = Smoothing::new(length);
        let mut seed = 1u64;
        for n in 0..1_000 {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let mut value = (seed >> 11) as f64 / (1u64 << 53) as f64;
            if n == 3 {
                smoothing.fill(value);
                for (ring, sum) in rings.iter_mut().zip(&mut sums) {
                    ring.fill(value);
                    *sum = value * length as f64;
                }
            }
            let smoothed = smoothing.push(value);
            for (ring, sum) in rings.iter_mut().zip(&mut sums) {
                let old = std::mem::replace(&mut ring[n % length], value);
                *sum += value - old;
                value = *sum / length as f64;
            }
            assert_eq!(smoothed.to_bits(), value.to_bits(), "{n}");
        }
    }

    #[test]
    fn the_reduction_of_a_repeated_gain_is_its_reduction() {
        let mut stats = LimiterStatsAccumulator::default();
        for g in [0.5f32, 0.5, 1.0, 0.5, 0.25, 0.25, 0.5, 1.0, 1.0, 0.25] {
            let expected = if g == 1.0 { -0.0 } else { -db(g as f64) };
            assert_eq!(stats.push(g).to_bits(), expected.to_bits(), "{g}");
        }
    }

    #[test]
    fn the_gain_of_a_level_is_ten_to_its_twentieth() {
        // The limiter emits 1.0 for a reduction of -0.0 (D291).
        assert_eq!(gain(0.0), 1.0);
        assert_eq!(gain(-0.0), 1.0);
        for i in -1200..=400 {
            let x = i as f64 * 0.1;
            let ten = 10f64.powf(x / 20.0);
            assert!((gain(x) - ten).abs() <= 1e-14 * ten, "{x} dB");
        }
    }

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

    /// Loud band-limited audio: eight tones from 40 Hz to 12 kHz whose
    /// levels change each 2,000 frames, with peaks near 0.95.
    fn tones(frames: usize, seed: u64) -> Vec<Stereo> {
        let mut state = seed;
        let mut next = || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (state >> 11) as f64 / (1u64 << 53) as f64
        };
        let tones: Vec<[f64; 3]> = (0..8)
            .map(|_| [40.0 * 300f64.powf(next()), 2.0 * PI * next(), next()])
            .collect();
        let levels: Vec<f64> = (0..frames / 2_000 + 1)
            .map(|_| 0.5 + 0.5 * next())
            .collect();
        let total: f64 = tones.iter().map(|t| t[2]).sum();
        (0..frames)
            .map(|i| {
                let t = i as f64 / 48_000.0;
                let level = 0.95 * levels[i / 2_000] / total;
                let at = |shift: f64| {
                    tones
                        .iter()
                        .map(|[hz, phase, a]| a * (2.0 * PI * hz * t + phase + shift).sin())
                        .sum::<f64>()
                        * level
                };
                [at(0.0) as f32, at(0.3) as f32]
            })
            .collect()
    }

    /// The peak of `audio` and its frames above `ceiling`, each with the
    /// gain that brings it to the ceiling, from the points of each frame.
    fn exhaustive(os: &Oversampler, audio: &[Stereo], ceiling: f64) -> (f64, Vec<(usize, f64)>) {
        let mut peak = 0.0f64;
        let mut overs = Vec::new();
        for (base, frame) in audio.iter().enumerate() {
            let frame_peak = frame[0]
                .abs()
                .max(frame[1].abs())
                .max(os.interpolated_peak(audio, base)) as f64;
            peak = peak.max(frame_peak);
            if frame_peak > ceiling {
                overs.push((base, ceiling / frame_peak));
            }
        }
        (peak, overs)
    }

    #[test]
    fn a_point_is_at_most_its_central_sum_and_its_tail_bound() {
        let os = Oversampler::new(48_000);
        let radius = RADIUS as usize;
        // The largest absolute central or wide sum of the frame at `base`.
        let sums = |audio: &[Stereo], base: usize, wide: bool| {
            let mut sums = [[0.0f32; 16]; 1];
            add_lane_sums(&os.center, audio, base, CENTER_FIRST, &mut sums[0]);
            if wide {
                add_lane_sums(&os.sides[0], audio, base, WIDE_FIRST, &mut sums[0]);
                let after = CENTER_FIRST + CENTER_TAPS;
                add_lane_sums(&os.sides[1], audio, base, after, &mut sums[0]);
            }
            lanes_peak(&sums) as f64
        };
        let windows = [
            (
                CENTER_FIRST..CENTER_FIRST + CENTER_TAPS,
                os.tail_gain,
                false,
            ),
            (WIDE_FIRST..WIDE_FIRST + WIDE_TAPS, os.wide_tail_gain, true),
        ];
        assert!(os.wide_tail_gain < os.tail_gain);
        // Samples of the sign of each tail tap of each point, with the
        // samples of the window at zero: the largest tail of that point.
        for (window, gain, wide) in windows.clone() {
            for (point, taps) in os.bank.iter().chain(&os.between).enumerate() {
                let mut audio = vec![[0.0f32; 2]; TAPS];
                for (i, frame) in audio.iter_mut().enumerate() {
                    if !window.contains(&i) {
                        let x = if taps[i] < 0.0 { -0.99 } else { 0.99 };
                        *frame = [x, -x];
                    }
                }
                let bound = sums(&audio, radius, wide) + 0.99 * gain;
                assert!(
                    os.interpolated_peak(&audio, radius) as f64 <= bound,
                    "{point} {wide}"
                );
            }
        }
        for audio in [tones(20_000, 3), bursts(20_000, 4)] {
            // The skip applies to the loud tones.
            if audio[0] == tones(1, 3)[0] {
                let mut measured = 0;
                let skipped = os.each_interpolated_peak(
                    &audio,
                    0..audio.len(),
                    Some((sample_peak(&audio), 0.99)),
                    |_, _| measured += 1,
                );
                assert!(skipped >= 0.0 && measured < audio.len() / 2, "{measured}");
            }
            for base in radius..audio.len() - radius {
                let near = sample_peak(&audio[base - radius..=base + radius]);
                let point = os.interpolated_peak(&audio, base) as f64;
                for (_, gain, wide) in windows.clone() {
                    let bound = sums(&audio, base, wide) + near * gain;
                    assert!(point <= bound, "{base} {wide}");
                }
                if let Some(bound) = os.group_bound::<1>(&audio, base, near, point) {
                    assert!(point <= bound, "{base}");
                }
            }
        }
    }

    #[test]
    fn the_scans_of_loud_tones_equal_the_points_of_each_frame() {
        for rate in [44_100, 48_000] {
            let os = Oversampler::new(rate);
            for (frames, seed) in [(70_001, 2u64), (65_536 * 2 + 31, 5)] {
                let audio = tones(frames, seed);
                let (peak, _) = exhaustive(&os, &audio, f64::INFINITY);
                assert_eq!(os.peak(&audio).to_bits(), peak.to_bits(), "{rate} {seed}");
                for ceiling in [0.8, 0.9, peak * 0.999, peak] {
                    let (_, expected) = exhaustive(&os, &audio, ceiling);
                    let scan = os.peak_scan(&audio, ceiling);
                    assert_eq!(scan.peak.to_bits(), peak.to_bits());
                    // The central sums skip points in the scan.
                    assert!(scan.blocks.iter().flatten().any(|&b| b < 0.0));
                    for overs in [scan.overs.clone(), os.overs(&audio, 0..frames, ceiling)] {
                        assert_eq!(overs.len(), expected.len(), "{rate} {seed} {ceiling}");
                        for (a, b) in overs.iter().zip(&expected) {
                            assert_eq!((a.0, a.1.to_bits()), (b.0, b.1.to_bits()));
                        }
                    }
                    let mut lowered = audio.clone();
                    let mut scan = scan;
                    let overs = std::mem::take(&mut scan.overs);
                    let (_, changed) = os.lower_peaks_from(&mut lowered, ceiling, overs);
                    let (after, _) = exhaustive(&os, &lowered, f64::INFINITY);
                    assert_eq!(
                        os.peak_after(&lowered, &scan, &changed).to_bits(),
                        after.to_bits(),
                        "{rate} {seed} {ceiling}"
                    );
                }
            }
        }
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
    fn the_peak_of_a_core_is_the_largest_sample_and_interpolated_point() {
        let os = Oversampler::new(44_100);
        let audio = bursts(140_003, 13);
        for core in [0..audio.len(), 5..70_000, 65_530..65_541, 100_017..140_003] {
            let expected = core.clone().fold(0.0f64, |peak, base| {
                let frame = audio[base][0].abs().max(audio[base][1].abs());
                peak.max(frame.max(os.interpolated_peak(&audio, base)) as f64)
            });
            assert_eq!(
                os.peak_core(&audio, core.clone()).to_bits(),
                expected.to_bits(),
                "{core:?}"
            );
        }
    }

    #[test]
    fn the_windows_of_the_interpolated_peaks_equal_each_frame_bit_for_bit() {
        let os = Oversampler::new(48_000);
        let audio = bursts(20_011, 5);
        for frames in [
            0..audio.len(),
            0..60,
            47..53,
            3..10_003,
            19_950..audio.len(),
        ] {
            let mut peaks = Vec::new();
            os.each_interpolated_peak(&audio, frames.clone(), None, |base, peak| {
                peaks.push((base, peak.to_bits()))
            });
            let expected: Vec<_> = frames
                .map(|base| (base, os.interpolated_peak(&audio, base).to_bits()))
                .collect();
            assert_eq!(peaks, expected);
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
    fn the_decimation_of_a_range_equals_the_whole_decimation_bit_for_bit() {
        let os = Oversampler::new(48_000);
        for frames in [0, 5, 385, 386, 401, 1_000, 4_117] {
            let input = bursts(frames, frames as u64);
            let whole = os.down(&input);
            for (start, step) in [(0, 1), (0, 6), (8, 7), (12, 4_096)] {
                let window_start = start.min(frames.saturating_sub(DOWN_RADIUS as usize));
                let bases = (0..frames).step_by(4).collect::<Vec<_>>();
                let mut actual = Vec::new();
                let mut k = 0;
                while k < bases.len() {
                    // Bases whose taps start before the window take the frames before it.
                    let count = step.min(bases.len() - k);
                    let window = if bases[k] >= window_start + DOWN_RADIUS as usize {
                        window_start
                    } else {
                        0
                    };
                    actual.extend(os.down_range_at_window(
                        &input[window..],
                        window,
                        frames,
                        bases[k],
                        count,
                    ));
                    k += count;
                }
                assert_eq!(actual.len(), whole.len());
                for (n, (a, b)) in actual.iter().zip(&whole).enumerate() {
                    assert_eq!(
                        a.map(f32::to_bits),
                        b.map(f32::to_bits),
                        "{frames} {step} {n}"
                    );
                }
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
