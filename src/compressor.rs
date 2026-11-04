use crate::{
    analysis::percentile,
    audio_io::{Result, Stereo},
    dsp::gain,
    scratch::{Frames, Slice},
};
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub threshold_dbfs: f64,
    pub ratio: f64,
    pub knee_db: f64,
    pub attack_ms: f64,
    pub release_ms: f64,
    pub detector_rms_ms: f64,
    pub sidechain_highpass_hz: f64,
    pub maximum_reduction_db: f64,
    /// Centred window of the section level, in ms. When it is above zero,
    /// `threshold_dbfs` is relative to the detector power averaged over this
    /// window, so quiet and loud sections are compressed alike.
    #[serde(default)]
    pub section_ms: f64,
    /// Give back the energy that the gain reduction removes over the
    /// centred section window, so that each section keeps the level that it
    /// has at the input (`set.compression.v3`). It needs a section window.
    #[serde(default)]
    pub section_makeup: bool,
}

impl Settings {
    pub fn validate(&self) -> Result<()> {
        for (value, low, high) in [
            (self.threshold_dbfs, -160.0, 120.0),
            (self.ratio, 1.0, 20.0),
            (self.knee_db, 0.0, 24.0),
            (self.attack_ms, 0.1, 1000.0),
            (self.release_ms, 1.0, 5000.0),
            (self.detector_rms_ms, 1.0, 1000.0),
            (self.sidechain_highpass_hz, 0.0, 5000.0),
            (self.maximum_reduction_db, 0.0, 24.0),
        ] {
            if !(low..=high).contains(&value) {
                return Err("Invalid compressor setting".into());
            }
        }
        if !(self.section_ms == 0.0 || (1000.0..=30000.0).contains(&self.section_ms))
            || (self.section_makeup && self.section_ms == 0.0)
        {
            return Err("Invalid compressor setting".into());
        }
        Ok(())
    }
}

#[derive(Clone, Default, Debug, Serialize)]
pub struct Stats {
    pub maximum_reduction_db: f64,
    pub mean_reduction_db: f64,
    pub frames_above_quarter_db_percent: f64,
    pub maximum_reduction_sample: usize,
    /// Mean and largest section make-up, in dB (`Settings::section_makeup`).
    pub mean_makeup_db: f64,
    pub maximum_makeup_db: f64,
    /// The detector level over the threshold of the frames at 0, 5, … 100%
    /// of their distribution, in dB, to `OVER_STEP_DB`: what a Compression
    /// step predicts its reduction from (`adjust.steps.v1`, D279). None
    /// when the compressor leaves the signal as it is.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub over_quantiles_db: Option<[f64; OVER_QUANTILES]>,
    /// The change of the held bands of each section with the threshold
    /// moved (D280). The render of a whole recording gives it
    /// (`process_with_hold_shift`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hold_shift: Option<HoldShift>,
}

/// Quantiles of `Stats::over_quantiles_db`: every 5%.
pub const OVER_QUANTILES: usize = 21;
/// Width of a bin of the distribution of `Stats::over_quantiles_db`, in dB.
pub const OVER_STEP_DB: f64 = 0.05;
/// The bins of `Stats::over_quantiles_db` span this level under and over
/// the threshold, in dB: twice the largest knee and the largest reduction
/// at a ratio of 2. A level outside the span counts in its end bin.
const OVER_SPAN_DB: f64 = 48.0;
const OVER_BINS: usize = (2.0 * OVER_SPAN_DB / OVER_STEP_DB) as usize;

fn over_bin(over_db: f64) -> usize {
    ((over_db + OVER_SPAN_DB) / OVER_STEP_DB).clamp(0.0, (OVER_BINS - 1) as f64) as usize
}

/// The quantiles of `Stats::over_quantiles_db` from the counts of the bins:
/// the centre of the bin of each quantile, to 0.001 dB.
fn over_quantiles(bins: &[u64]) -> Option<[f64; OVER_QUANTILES]> {
    let total: u64 = bins.iter().sum();
    if total == 0 {
        return None;
    }
    let mut out = [0.0; OVER_QUANTILES];
    let (mut bin, mut below) = (0, 0u64);
    for (q, value) in out.iter_mut().enumerate() {
        // The rank of the quantile among the frames, from 0.
        let rank = ((total - 1) as f64 * q as f64 / (OVER_QUANTILES - 1) as f64).round() as u64;
        while below + bins[bin] <= rank {
            below += bins[bin];
            bin += 1;
        }
        let centre = (bin as f64 + 0.5) * OVER_STEP_DB - OVER_SPAN_DB;
        *value = (centre * 1000.0).round() / 1000.0;
    }
    Some(out)
}

/// Mean static reduction of a distribution given by its quantiles of the
/// level over the threshold (`Stats::over_quantiles_db`), with each level
/// `shift_db` higher: the mean over the midpoints between neighbouring
/// quantiles, in dB.
pub fn mean_reduction_over(
    quantiles: &[f64; OVER_QUANTILES],
    settings: Settings,
    shift_db: f64,
) -> f64 {
    quantiles
        .windows(2)
        .map(|w| reduction_over_db(0.5 * (w[0] + w[1]) + shift_db, settings))
        .sum::<f64>()
        / (OVER_QUANTILES - 1) as f64
}

/// Move of the threshold of the two shadow gains of `HoldShift`, in dB:
/// about one and a half Compression steps (D279, D280).
pub const HOLD_SHIFT_DB: f64 = 2.0;

/// The change of each held band of the low-end hold (`low_end::BANDS_HZ`)
/// in each section of `low_end::SECTION_SECONDS` against the power of the
/// section, which the section make-up keeps, when the level of the
/// compressor moves `shift_db` down and up (D280). A level `shift_db` up is the threshold `shift_db` down. The
/// preview of a Compression step predicts the low-end hold of the final
/// render from it (`adjust::preview`).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct HoldShift {
    pub shift_db: f64,
    /// The change of the mean reduction with the level down and up, in dB.
    pub reduction_db: [f64; 2],
    /// The change of each held band in each section with the level down and
    /// up, in dB.
    pub change_db: Vec<[[f64; 2]; 2]>,
}

/// Rate of the decimated signal of `HoldShadow`, in Hz: six times the top
/// of the held bands.
const HOLD_RATE_HZ: f64 = 1500.0;

/// The shadow gains and the section powers of `HoldShift` in the gain pass.
/// Each shadow gain follows the detector of the compressor with its own
/// reduction, and takes the make-up of the compressor. The shadow takes the
/// mean of each block of `factor` frames: the frames of a block are 1 ms or
/// less, under the attack, the release and the detector window. Each
/// section of the decimated signal after each gain gives the power of each
/// held band from its spectrum, as `low_end::sections` gives it.
struct HoldShadow {
    /// Frames in a section, and in a block.
    span: usize,
    factor: usize,
    rate: f64,
    /// The smoothing coefficients of one frame.
    attack: f64,
    release: f64,
    reductions: [f64; 2],
    reduction_sums: [f64; 2],
    /// The block in progress: its first frame, the sum of each channel, the
    /// power, the sum of the gain of the compressor and the frames.
    start: usize,
    sums: [f64; 2],
    power: f64,
    gain: f64,
    count: usize,
    /// The level over the threshold and the reduction of the last frame.
    over: f64,
    reduction: f64,
    /// The section in progress: the mean of each channel and the gain of the
    /// compressor and of each shadow, for each block, and the power after
    /// each gain.
    section: usize,
    blocks: Vec<[f64; 5]>,
    raw: [f64; 3],
    /// For each section, the power of each held band after each gain, and
    /// the power after each gain.
    done: Vec<([[f64; 2]; 3], [f64; 3])>,
    planner: realfft::RealFftPlanner<f64>,
}

impl HoldShadow {
    fn new(settings: Settings, rate: u32) -> Self {
        let factor = ((rate as f64 / HOLD_RATE_HZ).floor() as usize).max(1);
        Self {
            span: crate::low_end::section_frames(rate),
            factor,
            rate: rate as f64 / factor as f64,
            attack: coefficient(settings.attack_ms, rate),
            release: coefficient(settings.release_ms, rate),
            reductions: [0.0; 2],
            reduction_sums: [0.0; 2],
            start: 0,
            sums: [0.0; 2],
            power: 0.0,
            gain: 0.0,
            count: 0,
            over: 0.0,
            reduction: 0.0,
            section: 0,
            blocks: Vec::new(),
            raw: [0.0; 3],
            done: Vec::new(),
            planner: realfft::RealFftPlanner::new(),
        }
    }

    /// Add the next frame, the input `frame` of the gain pass, with its level
    /// over the threshold, the gain of the compressor and its reduction.
    fn push(&mut self, frame: Stereo, over: f64, gain: f64, reduction: f64, settings: Settings) {
        let (l, r) = (frame[0] as f64, frame[1] as f64);
        self.sums[0] += l;
        self.sums[1] += r;
        self.power += l * l + r * r;
        self.gain += gain;
        self.count += 1;
        (self.over, self.reduction) = (over, reduction);
        if self.count == self.factor {
            self.end_block(settings);
        }
    }

    /// Close the block in progress: each shadow gain moves over its frames
    /// to the reduction of the level of its last frame.
    fn end_block(&mut self, settings: Settings) {
        let n = self.count as f64;
        let mut gains = [self.gain / n; 3];
        for (k, shift) in [-HOLD_SHIFT_DB, HOLD_SHIFT_DB].into_iter().enumerate() {
            let target = reduction_over_db(self.over + shift, settings);
            let r = &mut self.reductions[k];
            let smoothing = if target > *r {
                self.attack
            } else {
                self.release
            };
            let smoothing = smoothing.powi(self.count as i32);
            *r = smoothing * *r + (1.0 - smoothing) * target;
            self.reduction_sums[k] += *r * n;
            gains[k + 1] *= gain(self.reduction - *r);
        }
        let section = self.start / self.span;
        if section != self.section {
            self.end_section();
            self.section = section;
        }
        self.blocks.push([
            self.sums[0] / n,
            self.sums[1] / n,
            gains[0],
            gains[1],
            gains[2],
        ]);
        for (raw, g) in self.raw.iter_mut().zip(gains) {
            *raw += self.power * g * g;
        }
        self.start += self.count;
        (self.sums, self.power, self.gain, self.count) = ([0.0; 2], 0.0, 0.0, 0);
    }

    /// Close the section in progress: the power of each held band after
    /// each gain, from the spectrum of each channel.
    fn end_section(&mut self) {
        let n = self.blocks.len();
        if n == 0 {
            return;
        }
        let fft = self.planner.plan_fft_forward(n);
        let (mut input, mut output) = (fft.make_input_vec(), fft.make_output_vec());
        let mut bands = [[0.0f64; 2]; 3];
        for (k, band) in bands.iter_mut().enumerate() {
            for c in 0..2 {
                for (x, b) in input.iter_mut().zip(&self.blocks) {
                    *x = b[c] * b[2 + k];
                }
                fft.process(&mut input, &mut output)
                    .expect("section length");
                for (bin, value) in output.iter().enumerate() {
                    let hz = bin as f64 * self.rate / n as f64;
                    for (b, (low, high)) in crate::low_end::BANDS_HZ.iter().enumerate() {
                        if hz >= *low && hz < *high {
                            band[b] += value.norm_sqr();
                        }
                    }
                }
            }
        }
        self.done.push((bands, self.raw));
        self.blocks.clear();
        self.raw = [0.0; 3];
    }

    /// The `HoldShift` of a signal of `frames` frames whose mean reduction
    /// is `mean_reduction_db`. The make-up of each shadow gives back the
    /// power that its reduction removes from each section, as the section
    /// make-up of the compressor gives it back over its window.
    fn finish(mut self, frames: usize, settings: Settings, mean_reduction_db: f64) -> HoldShift {
        if self.count > 0 {
            self.end_block(settings);
        }
        self.end_section();
        let round = |v: f64| (v * 1000.0).round() / 1000.0;
        let ratio_db = |a: f64, b: f64| {
            if a > 0.0 && b > 0.0 {
                10.0 * (a / b).log10()
            } else {
                0.0
            }
        };
        HoldShift {
            shift_db: HOLD_SHIFT_DB,
            reduction_db: std::array::from_fn(|k| {
                round(self.reduction_sums[k] / frames as f64 - mean_reduction_db)
            }),
            change_db: self
                .done
                .iter()
                .map(|(bands, raw)| {
                    std::array::from_fn(|k| {
                        let back = ratio_db(raw[k + 1], raw[0]);
                        std::array::from_fn(|b| {
                            round(ratio_db(bands[k + 1][b], bands[0][b]) - back)
                        })
                    })
                })
                .collect(),
        }
    }
}

fn coefficient(ms: f64, rate: u32) -> f64 {
    (-1.0 / (rate as f64 * ms * 0.001)).exp()
}

#[derive(Clone, Copy)]
struct Detector {
    hp_b: f64,
    hp_a: f64,
    highpass: bool,
    previous_input: [f64; 2],
    previous_output: [f64; 2],
    rms_coefficient: f64,
    power: f64,
}

impl Detector {
    fn new(settings: Settings, rate: u32) -> Self {
        let k = (PI * settings.sidechain_highpass_hz / rate as f64).tan();
        Self {
            hp_b: 1.0 / (1.0 + k),
            hp_a: (k - 1.0) / (k + 1.0),
            highpass: settings.sidechain_highpass_hz > 0.0,
            previous_input: [0.0; 2],
            previous_output: [0.0; 2],
            rms_coefficient: coefficient(settings.detector_rms_ms, rate),
            power: 0.0,
        }
    }

    fn tick(&mut self, frame: Stereo) -> f64 {
        let power = self.instant(frame);
        self.power = self.rms_coefficient * self.power + (1.0 - self.rms_coefficient) * power;
        10.0 * self.power.max(1e-30).log10()
    }

    /// Linked peak power of the filtered frame.
    fn instant(&mut self, frame: Stereo) -> f64 {
        let mut power: f64 = 0.0;
        for (c, input) in frame.into_iter().enumerate() {
            let input = input as f64;
            let filtered = if self.highpass {
                self.hp_b * (input - self.previous_input[c]) - self.hp_a * self.previous_output[c]
            } else {
                input
            };
            self.previous_input[c] = input;
            self.previous_output[c] = filtered;
            power = power.max(filtered * filtered);
        }
        power
    }
}

/// Slow linked glue settings for the set: 1.4:1, 3 dB knee, 30 ms attack,
/// 300 ms release, at most 2 dB. The threshold is the 70th percentile of the
/// active detector level. Returns `None` without enough active signal.
/// `audio` is at `rate`.
pub fn glue_settings(audio: &[Stereo], rate: u32) -> Option<Settings> {
    glue_settings_read(&mut Slice(audio), rate).expect("frames of a slice")
}

/// Frames of each span of `glue_settings_read`.
const GLUE_SPAN: usize = 1 << 16;

/// `glue_settings` of the frames of `audio`, read one span at a time
/// (D250).
pub(crate) fn glue_settings_read(audio: &mut impl Frames, rate: u32) -> Result<Option<Settings>> {
    let mut settings = Settings {
        threshold_dbfs: 0.0,
        ratio: 1.4,
        knee_db: 3.0,
        attack_ms: 30.0,
        release_ms: 300.0,
        detector_rms_ms: 10.0,
        sidechain_highpass_hz: 0.0,
        maximum_reduction_db: 2.0,
        section_ms: 0.0,
        section_makeup: false,
    };
    let mut detector = Detector::new(settings, rate);
    let block = section_block(rate);
    let frames = audio.len();
    let mut levels = Vec::with_capacity(frames.div_ceil(block));
    let mut span = Vec::new();
    for start in (0..frames).step_by(GLUE_SPAN) {
        let part = audio.window(start, GLUE_SPAN.min(frames - start), &mut span)?;
        for (i, &frame) in (start..).zip(part) {
            let level = detector.tick(frame);
            if (i + 1) % block == 0 || i + 1 == frames {
                levels.push(level);
            }
        }
    }
    let floor = (levels.iter().copied().fold(-300.0, f64::max) - 40.0).max(-100.0);
    let active: Vec<_> = levels.iter().copied().filter(|&v| v > floor).collect();
    if active.len() < 100 {
        return Ok(None);
    }
    settings.threshold_dbfs = percentile(&active, 0.7);
    Ok(Some(settings))
}

/// Rule of the mastering compression of a set.
pub const MASTERING_RULE: &str = "set.compression.v3";
/// Centred window of the section level that the threshold follows, and over
/// which the make-up gives back the energy of the gain reduction, in ms.
pub const MASTERING_SECTION_MS: f64 = 4000.0;
/// Attacks that `set.compression.v3` tries for the punch of the onsets, in
/// ms. The range spans the attacks of 20 to 50 ms, at which the punch of a
/// mix is usually largest.
pub const PUNCH_ATTACKS_MS: [f64; 8] = [10.0, 15.0, 20.0, 30.0, 40.0, 50.0, 70.0, 100.0];
/// Share of the largest punch that the attack of Punchy keeps. Near its
/// largest value the punch changes little, and a slower attack moves the
/// gain less after each onset.
pub const PUNCH_SHARE: f64 = 0.95;
/// Onsets that the punch search needs. With fewer, the attack is that of
/// the stem timing, or 30 ms.
pub const PUNCH_ONSETS: usize = 8;
/// Longest attack of Gentle, in ms.
pub const GENTLE_ATTACK_MAXIMUM_MS: f64 = 150.0;
/// Shortest attack of Dense, in ms.
pub const DENSE_ATTACK_MINIMUM_MS: f64 = 10.0;
/// Steps of the gain search per 10 ms section block: steps of about 1 ms.
const SEARCH_STEPS: usize = 10;

/// Compressor timing from the stems: attack from the drum decay, release
/// from the drum interval, detector high-pass at the bass body frequency.
/// `set.compression.v3` takes the release and the high-pass; the attack is
/// the attack of a recording with too few onsets for the punch search.
#[derive(Clone, Copy, Serialize)]
pub struct Timing {
    pub attack_ms: f64,
    pub release_ms: f64,
    pub detector_highpass_hz: f64,
}

impl Timing {
    /// Timing from the stem measurements, or `None` when one is missing.
    pub fn from_stems(
        drum_decay_p50_ms: Option<f64>,
        drum_interval_p50_ms: Option<f64>,
        bass_body_hz: Option<f64>,
    ) -> Option<Self> {
        Some(Self {
            attack_ms: drum_decay_p50_ms?.clamp(10.0, 50.0),
            release_ms: drum_interval_p50_ms?.clamp(100.0, 600.0),
            detector_highpass_hz: bass_body_hz?.clamp(40.0, 150.0),
        })
    }
}

/// Compression style of a version (`set.compression.v3`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Style {
    /// 1 dB at twice the attack of Punchy: the onsets of the mix pass, and
    /// the gain moves least.
    Gentle,
    /// 1.5 dB at the attack that gives the onsets their punch.
    Punchy,
    /// 2 dB at half the attack of Punchy: more density and more movement of
    /// the gain.
    Dense,
}

impl Style {
    pub const ALL: [Style; 3] = [Style::Gentle, Style::Punchy, Style::Dense];

    /// Mean gain reduction of the style, in dB.
    pub fn depth_db(self) -> f64 {
        match self {
            Style::Gentle => 1.0,
            Style::Punchy => 1.5,
            Style::Dense => 2.0,
        }
    }

    /// Attack of the style from the attack of Punchy, in ms.
    pub fn attack_ms(self, punchy_ms: f64) -> f64 {
        match self {
            Style::Gentle => (2.0 * punchy_ms).min(GENTLE_ATTACK_MAXIMUM_MS),
            Style::Punchy => punchy_ms,
            Style::Dense => (0.5 * punchy_ms).max(DENSE_ATTACK_MINIMUM_MS),
        }
    }
}

/// Compression of one style for one recording (`set.compression.v3`).
#[derive(Clone, Serialize)]
pub struct Mastering {
    pub rule: &'static str,
    pub style: Style,
    pub settings: Settings,
    /// `stems` when the release and the detector high-pass come from the
    /// stems, otherwise `default`.
    pub timing_source: &'static str,
    /// Mean gain reduction over the recording before the make-up, in dB.
    pub mean_reduction_db: f64,
    /// Median change of the onset contrast after the make-up: the energy of
    /// the first 30 ms of each onset against that of 50 to 200 ms after it
    /// (D126), in dB. A positive change gives the onsets more punch. None
    /// without onsets.
    pub punch_db: Option<f64>,
    /// Median rise of the gain reduction from the 50 ms before each onset
    /// to 50 to 200 ms after it, in dB. None without onsets.
    pub movement_db: Option<f64>,
    /// Change of the window contrast (`analysis::analyze`) after the
    /// make-up, in dB.
    pub window_contrast_change_db: f64,
}

/// The compression styles of one recording (`set.compression.v3`).
#[derive(Clone, Serialize)]
pub struct Plan {
    pub rule: &'static str,
    /// The attack of Punchy, in ms.
    pub punchy_attack_ms: f64,
    /// `punch` when the punch search set the attack of Punchy, otherwise
    /// `stems` or `default`.
    pub attack_source: &'static str,
    /// The punch of each tried attack at the depth of Punchy, as
    /// `[attack ms, punch dB]`.
    pub punch_by_attack: Vec<[f64; 2]>,
    /// Gentle, Punchy and Dense, in this order.
    pub styles: Vec<Mastering>,
}

impl Plan {
    pub fn style(&self, style: Style) -> &Mastering {
        self.styles
            .iter()
            .find(|m| m.style == style)
            .expect("a plan has each style")
    }
}

/// The recording at the resolution of the search of `set.compression.v3`.
struct Search {
    /// Detector level less the section level at the end of each step, dB.
    relative: Vec<f32>,
    /// Input energy of each step.
    energy: Vec<f64>,
    /// Onsets of `analysis::analyze`, as indices of 10 ms blocks.
    onsets: Vec<usize>,
    /// Active 12 s windows of `analysis::analyze`.
    active: Vec<bool>,
    /// Half the section window, in 10 ms blocks.
    half: usize,
}

/// Measures of one setting in the search.
struct Outcome {
    punch_db: Option<f64>,
    movement_db: Option<f64>,
    window_contrast_change_db: f64,
}

impl Search {
    /// The search of the frames of `audio`, read twice in spans of
    /// `SPAN_BLOCKS` section blocks: for the section levels, then for the
    /// detector.
    fn new(
        audio: &mut impl Frames,
        settings: Settings,
        temporal: &crate::analysis::Temporal,
        rate: u32,
    ) -> Result<Self> {
        let block = section_block(rate);
        let (frames, span) = (audio.len(), SPAN_BLOCKS * block);
        let mut buffer = Vec::new();
        let mut blocks = SectionBlocks::new(settings, rate, frames);
        for start in (0..frames).step_by(span) {
            blocks.push(audio.window(start, span.min(frames - start), &mut buffer)?);
        }
        let section = blocks.finish();
        let mut detector = Detector::new(settings, rate);
        let steps = frames.div_ceil(block) * SEARCH_STEPS;
        let (mut relative, mut energy) = (Vec::with_capacity(steps), Vec::with_capacity(steps));
        let mut level = -300.0;
        for first in (0..frames).step_by(span) {
            let part = audio.window(first, span.min(frames - first), &mut buffer)?;
            for (i, chunk) in part.chunks(block).enumerate() {
                let b = first / block + i;
                let mut start = 0;
                for step in 0..SEARCH_STEPS {
                    let end = (step + 1) * chunk.len() / SEARCH_STEPS;
                    let mut sum = 0.0;
                    for &frame in &chunk[start..end] {
                        level = detector.tick(frame);
                        sum += (frame[0] as f64).powi(2) + (frame[1] as f64).powi(2);
                    }
                    relative.push((level - section[b]) as f32);
                    energy.push(sum);
                    start = end;
                }
            }
        }
        Ok(Self {
            relative,
            energy,
            onsets: temporal
                .events
                .iter()
                .map(|e| (e.start_seconds * 100.0).round() as usize)
                .collect(),
            active: temporal.windows.iter().map(|w| w.active).collect(),
            half: section_half(settings),
        })
    }

    /// The gain reduction of each step and its mean, in dB. The steps are
    /// about 1 ms long.
    fn gains(&self, settings: Settings, out: &mut Vec<f32>) -> f64 {
        let attack = (-1.0 / settings.attack_ms).exp();
        let release = (-1.0 / settings.release_ms).exp();
        let mut reduction = 0.0f64;
        let mut sum = 0.0;
        out.clear();
        for &relative in &self.relative {
            let target = static_reduction_db(relative as f64, settings);
            let smoothing = if target > reduction { attack } else { release };
            reduction = smoothing * reduction + (1.0 - smoothing) * target;
            sum += reduction;
            out.push(reduction as f32);
        }
        sum / self.relative.len().max(1) as f64
    }

    /// The threshold offset of `settings` that gives a mean reduction of
    /// `depth` dB, to 0.01 dB.
    fn offset_for(&self, mut settings: Settings, depth: f64, scratch: &mut Vec<f32>) -> f64 {
        let (mut low, mut high) = (-40.0, 40.0);
        for _ in 0..24 {
            settings.threshold_dbfs = 0.5 * (low + high);
            if self.gains(settings, scratch) > depth {
                low = settings.threshold_dbfs;
            } else {
                high = settings.threshold_dbfs;
            }
        }
        crate::treatment_render::quantize(0.5 * (low + high), 100.0)
    }

    fn outcome(&self, gains: &[f32]) -> Outcome {
        let blocks = self.relative.len() / SEARCH_STEPS;
        let (mut input, mut output, mut reduction) =
            (vec![0.0; blocks], vec![0.0; blocks], vec![0.0; blocks]);
        for (i, (&e, &g)) in self.energy.iter().zip(gains).enumerate() {
            let b = i / SEARCH_STEPS;
            input[b] += e;
            output[b] += e * gain(-2.0 * g as f64);
            reduction[b] += g as f64 / SEARCH_STEPS as f64;
        }
        let makeup = window_makeup(&input, &output, self.half);
        let after: Vec<f64> = output
            .iter()
            .zip(&makeup)
            .map(|(o, m)| o * gain(2.0 * m))
            .collect();
        let mean = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
        // The 12 s windows of `analysis::analyze`.
        let contrast = |energy: &[f64]| {
            let levels: Vec<f64> = energy
                .chunks(1200)
                .zip(&self.active)
                .filter(|(_, a)| **a)
                .map(|(e, _)| 10.0 * mean(e).max(1e-30).log10())
                .collect();
            percentile(&levels, 0.9) - percentile(&levels, 0.1)
        };
        let onset = |e: &[f64], s: usize| {
            10.0 * (mean(&e[s..s + 3]).max(1e-30) / mean(&e[s + 5..s + 20]).max(1e-30)).log10()
        };
        let punch: Vec<f64> = self
            .onsets
            .iter()
            .filter(|&&s| s + 20 <= blocks)
            .map(|&s| onset(&after, s) - onset(&input, s))
            .collect();
        let movement: Vec<f64> = self
            .onsets
            .iter()
            .filter(|&&s| s >= 5 && s + 20 <= blocks)
            .map(|&s| mean(&reduction[s + 5..s + 20]) - mean(&reduction[s - 5..s]))
            .collect();
        Outcome {
            punch_db: (!punch.is_empty()).then(|| percentile(&punch, 0.5)),
            movement_db: (!movement.is_empty()).then(|| percentile(&movement, 0.5)),
            window_contrast_change_db: contrast(&after) - contrast(&input),
        }
    }
}

/// Linked 2:1 compression before peak control in three styles
/// (`set.compression.v3`). The threshold follows the section level (a
/// centred 4 s window of detector power), and the make-up gives back the
/// energy that the gain reduction removes over the same window, so that
/// each section keeps the level that it has in the mix. The attack of
/// Punchy is the slowest attack of `PUNCH_ATTACKS_MS` that keeps
/// `PUNCH_SHARE` of the largest punch at its depth; Gentle and Dense take
/// their attacks from it (`Style::attack_ms`). Each style's offset gives its
/// mean reduction (`Style::depth_db`). The stem timing gives the release
/// and the detector high-pass; without it, the release is 300 ms and the
/// detector full-band. Returns `None` when the recording has fewer than two
/// active 12 s windows. `audio` is at `rate`.
pub fn plan(audio: &[Stereo], timing: Option<Timing>, rate: u32) -> Option<Plan> {
    let temporal = crate::analysis::analyze(audio, rate);
    plan_with(&mut Slice(audio), timing, &temporal, rate).expect("frames of a slice")
}

/// Section blocks of each span of `Search::new`: 60 s.
const SPAN_BLOCKS: usize = 6000;

/// `plan` of the frames of `audio`, whose temporal analysis is `temporal`.
pub(crate) fn plan_with(
    audio: &mut impl Frames,
    timing: Option<Timing>,
    temporal: &crate::analysis::Temporal,
    rate: u32,
) -> Result<Option<Plan>> {
    let base = Settings {
        threshold_dbfs: 0.0,
        ratio: 2.0,
        knee_db: 6.0,
        attack_ms: 30.0,
        release_ms: timing.map_or(300.0, |t| t.release_ms),
        detector_rms_ms: 10.0,
        sidechain_highpass_hz: timing.map_or(0.0, |t| t.detector_highpass_hz),
        maximum_reduction_db: 6.0,
        section_ms: MASTERING_SECTION_MS,
        section_makeup: true,
    };
    if base.validate().is_err() || temporal.windows.iter().filter(|w| w.active).count() < 2 {
        return Ok(None);
    }
    let search = Search::new(audio, base, temporal, rate)?;
    let mut scratch = Vec::new();
    let mut punch_by_attack = Vec::new();
    if search.onsets.len() >= PUNCH_ONSETS {
        for attack_ms in PUNCH_ATTACKS_MS {
            let mut settings = base;
            settings.attack_ms = attack_ms;
            settings.threshold_dbfs =
                search.offset_for(settings, Style::Punchy.depth_db(), &mut scratch);
            search.gains(settings, &mut scratch);
            if let Some(punch) = search.outcome(&scratch).punch_db {
                punch_by_attack.push([attack_ms, punch]);
            }
        }
    }
    let best = punch_by_attack.iter().map(|p| p[1]).fold(0.0, f64::max);
    let (punchy_attack_ms, attack_source) = if best > 0.0 {
        let attack = punch_by_attack
            .iter()
            .filter(|p| p[1] >= PUNCH_SHARE * best)
            .map(|p| p[0])
            .fold(0.0, f64::max);
        (attack, "punch")
    } else {
        match timing {
            Some(t) => (t.attack_ms, "stems"),
            None => (30.0, "default"),
        }
    };
    let styles = Style::ALL
        .iter()
        .map(|&style| {
            let mut settings = base;
            settings.attack_ms = style.attack_ms(punchy_attack_ms);
            settings.threshold_dbfs = search.offset_for(settings, style.depth_db(), &mut scratch);
            let mean = search.gains(settings, &mut scratch);
            let outcome = search.outcome(&scratch);
            Mastering {
                rule: MASTERING_RULE,
                style,
                settings,
                timing_source: if timing.is_some() { "stems" } else { "default" },
                mean_reduction_db: mean,
                punch_db: outcome.punch_db,
                movement_db: outcome.movement_db,
                window_contrast_change_db: outcome.window_contrast_change_db,
            }
        })
        .collect();
    Ok(Some(Plan {
        rule: MASTERING_RULE,
        punchy_attack_ms,
        attack_source,
        punch_by_attack,
        styles,
    }))
}

/// The Punchy style of `plan`: the compression of the `master` version.
pub fn mastering(audio: &[Stereo], timing: Option<Timing>, rate: u32) -> Option<Mastering> {
    plan(audio, timing, rate).map(|p| p.style(Style::Punchy).clone())
}

/// Positive attenuation in dB, before attack/release smoothing.
pub fn static_reduction_db(level_dbfs: f64, settings: Settings) -> f64 {
    reduction_over_db(level_dbfs - settings.threshold_dbfs, settings)
}

/// `static_reduction_db` of a level `over` dB over the threshold.
pub fn reduction_over_db(over: f64, settings: Settings) -> f64 {
    let slope = 1.0 - 1.0 / settings.ratio;
    let reduction = if settings.knee_db == 0.0 {
        slope * over.max(0.0)
    } else if over <= -settings.knee_db * 0.5 {
        0.0
    } else if over >= settings.knee_db * 0.5 {
        slope * over
    } else {
        slope * (over + settings.knee_db * 0.5).powi(2) / (2.0 * settings.knee_db)
    };
    reduction.clamp(0.0, settings.maximum_reduction_db)
}

/// Samples per section-level block at `rate`: 10 ms.
pub fn section_block(rate: u32) -> usize {
    rate as usize / 100
}

/// Section level per 10 ms block: the detector power averaged over a centred
/// window of `settings.section_ms`, in dB. Empty when the setting is zero.
fn section_levels(audio: &[Stereo], settings: Settings, rate: u32) -> Vec<f64> {
    let mut blocks = SectionBlocks::new(settings, rate, audio.len());
    blocks.push(audio);
    blocks.finish()
}

/// `section_levels` of a signal whose frames arrive in spans.
struct SectionBlocks {
    settings: Settings,
    detector: Detector,
    block: usize,
    /// The detector sum and the frames of the block in progress.
    sum: f64,
    count: usize,
    blocks: Vec<f64>,
}

impl SectionBlocks {
    /// The section levels of a signal of `frames` frames at `rate`.
    fn new(settings: Settings, rate: u32, frames: usize) -> Self {
        let block = section_block(rate);
        let blocks = if settings.section_ms == 0.0 {
            0
        } else {
            frames.div_ceil(block)
        };
        Self {
            settings,
            detector: Detector::new(settings, rate),
            block,
            sum: -0.0,
            count: 0,
            blocks: Vec::with_capacity(blocks),
        }
    }

    /// Add the next frames of the signal.
    fn push(&mut self, audio: &[Stereo]) {
        if self.settings.section_ms == 0.0 {
            return;
        }
        // The loops of this module keep their state in locals: through
        // `self`, each frame loads and stores it.
        let (mut detector, mut sum, mut count) = (self.detector, self.sum, self.count);
        for &frame in audio {
            sum += detector.instant(frame);
            count += 1;
            if count == self.block {
                self.blocks.push(sum / count as f64);
                (sum, count) = (-0.0, 0);
            }
        }
        (self.detector, self.sum, self.count) = (detector, sum, count);
    }

    /// The section level of each block of the signal.
    fn finish(mut self) -> Vec<f64> {
        if self.settings.section_ms == 0.0 {
            return Vec::new();
        }
        if self.count > 0 {
            self.blocks.push(self.sum / self.count as f64);
        }
        section_levels_of(&self.blocks, self.settings)
    }
}

/// The section level of each block from `blocks`, the block means of the
/// detector.
fn section_levels_of(blocks: &[f64], settings: Settings) -> Vec<f64> {
    let half = section_half(settings);
    let mut prefix = Vec::with_capacity(blocks.len() + 1);
    prefix.push(0.0f64);
    for b in blocks {
        prefix.push(prefix.last().unwrap() + b);
    }
    (0..blocks.len())
        .map(|i| section_level(&prefix, half, i, blocks.len()))
        .collect()
}

/// The section level of block `i` of `blocks` blocks, from `prefix`, the
/// sums of the block means before each block from block 0.
fn section_level(prefix: &[f64], half: usize, i: usize, blocks: usize) -> f64 {
    let (a, b) = (i.saturating_sub(half), (i + half + 1).min(blocks));
    10.0 * ((prefix[b] - prefix[a]) / (b - a) as f64)
        .max(1e-30)
        .log10()
}

/// Half the centred section window of `settings`, in 10 ms blocks.
fn section_half(settings: Settings) -> usize {
    (settings.section_ms / 20.0).round() as usize
}

/// Make-up of each 10 ms block, in dB: the input energy over the centred
/// window of `half` blocks on each side against the energy after the gain
/// reduction. Each window sums its own blocks, so that a span of a
/// recording gives the make-up of the whole recording.
fn window_makeup(input: &[f64], output: &[f64], half: usize) -> Vec<f64> {
    (0..input.len())
        .map(|i| window_makeup_at(input, output, half, i))
        .collect()
}

/// The make-up of block `i` of `window_makeup`.
fn window_makeup_at(input: &[f64], output: &[f64], half: usize, i: usize) -> f64 {
    let range = i.saturating_sub(half)..(i + half + 1).min(input.len());
    let a: f64 = input[range.clone()].iter().sum();
    let b: f64 = output[range].iter().sum();
    if a > 0.0 && b > 0.0 {
        10.0 * (a / b).log10()
    } else {
        0.0
    }
}

/// Section make-up of `settings` on `audio`, per 10 ms block, in dB.
fn section_makeup(audio: &[Stereo], section: &[f64], settings: Settings, rate: u32) -> Vec<f64> {
    let mut makeup = Makeup::new(settings, rate, audio.len());
    makeup.push(audio, section);
    makeup.finish()
}

/// `section_makeup` of a signal of a known length whose frames arrive in
/// spans.
struct Makeup {
    settings: Settings,
    rate: u32,
    block: usize,
    detector: Detector,
    attack: f64,
    release: f64,
    reduction: f64,
    /// The index of the next frame.
    index: usize,
    input: Vec<f64>,
    output: Vec<f64>,
}

impl Makeup {
    /// The make-up of a signal of `frames` frames.
    fn new(settings: Settings, rate: u32, frames: usize) -> Self {
        let block = section_block(rate);
        let blocks = frames.div_ceil(block);
        Self {
            settings,
            rate,
            block,
            detector: Detector::new(settings, rate),
            attack: coefficient(settings.attack_ms, rate),
            release: coefficient(settings.release_ms, rate),
            reduction: 0.0,
            index: 0,
            input: vec![0.0f64; blocks],
            output: vec![0.0f64; blocks],
        }
    }

    /// Add the next frames of the signal, with `section`, the section levels
    /// of the whole signal or of its blocks up to the last of these frames.
    fn push(&mut self, audio: &[Stereo], section: &[f64]) {
        self.push_each(audio, section, |_| {});
    }

    /// `push`, with the level over the threshold and the reduction of each
    /// frame to `each`. The gain pass of `Compress` gives the same values.
    fn push_each(&mut self, audio: &[Stereo], section: &[f64], mut each: impl FnMut([f64; 2])) {
        let (settings, rate, block) = (self.settings, self.rate, self.block);
        let (attack, release) = (self.attack, self.release);
        let (mut detector, mut reduction) = (self.detector, self.reduction);
        let mut input = std::mem::take(&mut self.input);
        let mut output = std::mem::take(&mut self.output);
        for (i, &frame) in (self.index..).zip(audio) {
            let over = over_at(detector.tick(frame), i, section, settings, rate);
            let target = reduction_over_db(over, settings);
            let smoothing = if target > reduction { attack } else { release };
            reduction = smoothing * reduction + (1.0 - smoothing) * target;
            each([over, reduction]);
            let power = (frame[0] as f64).powi(2) + (frame[1] as f64).powi(2);
            input[i / block] += power;
            output[i / block] += power * gain(-2.0 * reduction);
        }
        (self.detector, self.reduction) = (detector, reduction);
        (self.input, self.output) = (input, output);
        self.index += audio.len();
    }

    fn finish(self) -> Vec<f64> {
        window_makeup(&self.input, &self.output, section_half(self.settings))
    }

    /// The make-up of block `b`, once the frames of the blocks of its
    /// window are in.
    fn at(&self, b: usize) -> f64 {
        window_makeup_at(&self.input, &self.output, section_half(self.settings), b)
    }
}

/// Make-up at sample `index`: linear between the centres of the 10 ms
/// blocks of `block` samples.
fn makeup_at(makeup: &[f64], index: usize, block: usize) -> f64 {
    let Some(&last) = makeup.last() else {
        return 0.0;
    };
    let x = (index as f64 + 0.5) / block as f64 - 0.5;
    if x <= 0.0 {
        return makeup[0];
    }
    let b = x.floor() as usize;
    if b + 1 >= makeup.len() {
        return last;
    }
    makeup[b] + (x - b as f64) * (makeup[b + 1] - makeup[b])
}

/// The level over the threshold at sample `index`, in dB, with the
/// threshold relative to the section level when `section` is not empty.
fn over_at(level: f64, index: usize, section: &[f64], settings: Settings, rate: u32) -> f64 {
    match section.get(index / section_block(rate)) {
        Some(section) => level - (section + settings.threshold_dbfs),
        None => level - settings.threshold_dbfs,
    }
}

/// Compress `audio`, at `rate`, with `settings`.
pub fn process(audio: &mut [Stereo], settings: Settings, rate: u32) -> Result<Stats> {
    process_with(audio, settings, rate, false)
}

/// `process` on a whole recording, with the `Stats::hold_shift` of the
/// recording.
pub fn process_with_hold_shift(
    audio: &mut [Stereo],
    settings: Settings,
    rate: u32,
) -> Result<Stats> {
    process_with(audio, settings, rate, true)
}

fn process_with(
    audio: &mut [Stereo],
    settings: Settings,
    rate: u32,
    hold_shift: bool,
) -> Result<Stats> {
    settings.validate()?;
    if audio.iter().flatten().any(|v| !v.is_finite()) {
        return Err("Compressor input must be finite".into());
    }
    if !changes(settings) || audio.is_empty() {
        return Ok(Stats::default());
    }
    let section = section_levels(audio, settings, rate);
    let makeup = if settings.section_makeup {
        section_makeup(audio, &section, settings, rate)
    } else {
        Vec::new()
    };
    let mut compress = Compress::new(settings, rate);
    if hold_shift {
        compress.hold = Some(HoldShadow::new(settings, rate));
    }
    compress.process(audio, &section, &makeup);
    Ok(compress.finish(audio.len(), &makeup))
}

/// Whether `process` with `settings` changes the signal.
pub(crate) fn changes(settings: Settings) -> bool {
    settings.ratio != 1.0 && settings.maximum_reduction_db != 0.0
}

/// The gain pass of `process` on a signal whose frames arrive in spans.
struct Compress {
    settings: Settings,
    rate: u32,
    block: usize,
    detector: Detector,
    attack: f64,
    release: f64,
    reduction: f64,
    sum: f64,
    active: usize,
    /// The index of the next frame.
    index: usize,
    stats: Stats,
    /// The frames in each bin of the level over the threshold
    /// (`Stats::over_quantiles_db`).
    over_bins: Vec<u64>,
    hold: Option<HoldShadow>,
}

impl Compress {
    fn new(settings: Settings, rate: u32) -> Self {
        Self {
            settings,
            rate,
            block: section_block(rate),
            detector: Detector::new(settings, rate),
            attack: coefficient(settings.attack_ms, rate),
            release: coefficient(settings.release_ms, rate),
            reduction: 0.0,
            sum: 0.0,
            active: 0,
            index: 0,
            stats: Stats::default(),
            over_bins: vec![0; OVER_BINS],
            hold: None,
        }
    }

    /// Compress the next frames of the signal in place, with the section
    /// levels and the make-up of the whole signal, or of its blocks up to
    /// the block after the last of these frames.
    fn process(&mut self, audio: &mut [Stereo], section: &[f64], makeup: &[f64]) {
        let (settings, rate) = (self.settings, self.rate);
        let (attack, release) = (self.attack, self.release);
        let (mut detector, mut reduction) = (self.detector, self.reduction);
        let frames = (self.index..).zip(audio.iter_mut()).map(|(i, frame)| {
            let over = over_at(detector.tick(*frame), i, section, settings, rate);
            let target = reduction_over_db(over, settings);
            let smoothing = if target > reduction { attack } else { release };
            reduction = smoothing * reduction + (1.0 - smoothing) * target;
            (frame, [over, reduction])
        });
        self.apply(frames, makeup);
        (self.detector, self.reduction) = (detector, reduction);
    }

    /// `process` with the level over the threshold and the reduction of
    /// each frame from `passes`, the values of `Makeup::push_each` on the
    /// same frames.
    fn process_stored(&mut self, audio: &mut [Stereo], passes: &[[f64; 2]], makeup: &[f64]) {
        self.apply(audio.iter_mut().zip(passes.iter().copied()), makeup);
    }

    /// Apply the gain of each frame of `frames`, with its level over the
    /// threshold and its reduction, and add the frame to the stats.
    fn apply<'a>(
        &mut self,
        frames: impl Iterator<Item = (&'a mut Stereo, [f64; 2])>,
        makeup: &[f64],
    ) {
        let (settings, block) = (self.settings, self.block);
        let (mut sum, mut active, mut stats) = (self.sum, self.active, self.stats.clone());
        let over_bins = &mut self.over_bins;
        let mut hold = self.hold.as_mut();
        let mut index = self.index;
        for (frame, [over, reduction]) in frames {
            let i = index;
            index += 1;
            over_bins[over_bin(over)] += 1;
            let g = gain(makeup_at(makeup, i, block) - reduction);
            if let Some(hold) = &mut hold {
                hold.push(*frame, over, g, reduction, settings);
            }
            let g = g as f32;
            frame[0] *= g;
            frame[1] *= g;
            if reduction > stats.maximum_reduction_db {
                stats.maximum_reduction_db = reduction;
                stats.maximum_reduction_sample = i;
            }
            sum += reduction;
            active += usize::from(reduction > 0.25);
        }
        (self.sum, self.active, self.stats) = (sum, active, stats);
        self.index = index;
    }

    /// The stats of a signal of `frames` frames.
    fn finish(self, frames: usize, makeup: &[f64]) -> Stats {
        let mut stats = self.stats;
        stats.mean_reduction_db = self.sum / frames as f64;
        stats.frames_above_quarter_db_percent = 100.0 * self.active as f64 / frames as f64;
        stats.over_quantiles_db = over_quantiles(&self.over_bins);
        stats.hold_shift = self
            .hold
            .map(|hold| hold.finish(frames, self.settings, stats.mean_reduction_db));
        if !makeup.is_empty() {
            stats.mean_makeup_db = makeup.iter().sum::<f64>() / makeup.len() as f64;
            stats.maximum_makeup_db = makeup.iter().copied().fold(0.0, f64::max);
        }
        stats
    }
}

/// Add the sum of the block means to `prefix`, with the mean of the next
/// block.
fn add_mean(prefix: &mut Vec<f64>, mean: f64) {
    prefix.push(prefix.last().unwrap() + mean);
}

/// `process` on a signal of `frames` frames whose frames arrive in spans,
/// in one pass. The section level of a block reads `section_half` blocks on
/// each side of it, and the make-up of a block the reductions of as many
/// blocks on each side, so the gain of a frame waits for the frames of
/// about two section windows after it. The stream holds those frames, the
/// level over the threshold and the reduction of each of them that the
/// make-up pass gives, and a few values for each block: 13 MB at 30
/// minutes in spans of 65,536 frames.
pub(crate) struct Stream {
    settings: Settings,
    block: usize,
    frames: usize,
    blocks: usize,
    half: usize,
    /// The section detector, and the sum and the frames of its block in
    /// progress.
    detector: Detector,
    sum: f64,
    count: usize,
    /// The sums of the block means before each block, from block 0.
    prefix: Vec<f64>,
    section: Vec<f64>,
    makeup: Option<Makeup>,
    levels: Vec<f64>,
    compress: Compress,
    /// The frames from the next frame of the gain pass.
    pending: std::collections::VecDeque<Stereo>,
    /// The level over the threshold and the reduction of each frame from
    /// the next frame of the gain pass, from the make-up pass.
    passes: std::collections::VecDeque<[f64; 2]>,
    received: usize,
}

impl Stream {
    /// The compressor of a signal of `frames` frames at `rate`, in spans
    /// of at most `span` frames, for `settings` that `changes`.
    pub(crate) fn new(settings: Settings, rate: u32, frames: usize, span: usize) -> Self {
        let block = section_block(rate);
        let blocks = frames.div_ceil(block);
        let pending = span + Self::lag(settings, rate);
        let sections = if settings.section_ms == 0.0 {
            0
        } else {
            blocks
        };
        let mut prefix = Vec::with_capacity(sections + 1);
        prefix.push(0.0f64);
        Self {
            settings,
            block,
            frames,
            blocks,
            half: section_half(settings),
            detector: Detector::new(settings, rate),
            sum: -0.0,
            count: 0,
            prefix,
            section: Vec::with_capacity(sections),
            makeup: settings
                .section_makeup
                .then(|| Makeup::new(settings, rate, frames)),
            levels: Vec::with_capacity(if settings.section_makeup { blocks } else { 0 }),
            compress: Compress::new(settings, rate),
            pending: std::collections::VecDeque::with_capacity(pending),
            passes: std::collections::VecDeque::with_capacity(if settings.section_makeup {
                pending
            } else {
                0
            }),
            received: 0,
        }
    }

    /// The stream with the `Stats::hold_shift` of the recording.
    pub(crate) fn with_hold_shift(mut self, rate: u32) -> Self {
        self.compress.hold = Some(HoldShadow::new(self.settings, rate));
        self
    }

    /// The most frames that wait for the gain pass after a push: the
    /// blocks of the section window after a frame, and as many again for
    /// its make-up, with a block for the block in progress and for the
    /// make-up of the block after the frame.
    pub(crate) fn lag(settings: Settings, rate: u32) -> usize {
        let half = section_half(settings);
        let blocks = match (settings.section_ms == 0.0, settings.section_makeup) {
            (true, _) => 0,
            (false, false) => half + 2,
            (false, true) => 2 * half + 4,
        };
        blocks * section_block(rate)
    }

    /// Add the next frames of the signal, and append the compressed frames
    /// that they complete to `out`.
    pub(crate) fn push(&mut self, audio: &[Stereo], out: &mut Vec<Stereo>) {
        if self.settings.section_ms != 0.0 {
            let (mut detector, mut sum, mut count) = (self.detector, self.sum, self.count);
            for &frame in audio {
                sum += detector.instant(frame);
                count += 1;
                if count == self.block {
                    add_mean(&mut self.prefix, sum / count as f64);
                    (sum, count) = (-0.0, 0);
                }
            }
            (self.detector, self.sum, self.count) = (detector, sum, count);
        }
        self.pending.extend(audio);
        self.received += audio.len();
        self.advance(out);
    }

    /// Append the rest of the compressed frames to `out`, and give the
    /// stats of the signal.
    pub(crate) fn finish(mut self, out: &mut Vec<Stereo>) -> Stats {
        if self.settings.section_ms != 0.0 && self.count > 0 {
            add_mean(&mut self.prefix, self.sum / self.count as f64);
        }
        self.advance(out);
        assert_eq!(
            self.compress.index, self.frames,
            "frames missing from the stream"
        );
        self.compress.finish(self.frames, &self.levels)
    }

    /// Run each pass over the frames whose inputs are in.
    fn advance(&mut self, out: &mut Vec<Stereo>) {
        let (block, blocks, half) = (self.block, self.blocks, self.half);
        // The frames before `ready` have their section levels.
        let ready = if self.settings.section_ms == 0.0 {
            self.received
        } else {
            let means = self.prefix.len() - 1;
            while self.section.len() < blocks
                && means >= (self.section.len() + half + 1).min(blocks)
            {
                let i = self.section.len();
                self.section
                    .push(section_level(&self.prefix, half, i, blocks));
            }
            (self.section.len() * block).min(self.received)
        };
        // The frames before `ready` also have the make-up of their block
        // and of the block after it (`makeup_at`).
        let ready = match &mut self.makeup {
            None => ready,
            Some(makeup) => {
                let start = self.compress.index;
                let range = makeup.index - start..ready - start;
                let (front, back) = self.pending.as_slices();
                let split = front.len();
                let passes = &mut self.passes;
                for frames in [
                    &front[range.start.min(split)..range.end.min(split)],
                    &back[range.start.max(split) - split..range.end.max(split) - split],
                ] {
                    makeup.push_each(frames, &self.section, |pass| passes.push_back(pass));
                }
                while self.levels.len() < blocks
                    && makeup.index >= ((self.levels.len() + half + 1) * block).min(self.frames)
                {
                    let b = self.levels.len();
                    self.levels.push(makeup.at(b));
                }
                if self.levels.len() == blocks {
                    ready
                } else {
                    ready.min(self.levels.len().saturating_sub(1) * block)
                }
            }
        };
        let count = ready.saturating_sub(self.compress.index);
        if self.makeup.is_some() {
            // The gain pass takes the values of the make-up pass, in the
            // runs of frames that are in one piece in both rings.
            let mut done = 0;
            while done < count {
                let (front, back) = self.pending.as_mut_slices();
                let frames = match front.get_mut(done..) {
                    Some(frames) if !frames.is_empty() => frames,
                    _ => &mut back[done - front.len()..],
                };
                let (front, back) = self.passes.as_slices();
                let passes = match front.get(done..) {
                    Some(passes) if !passes.is_empty() => passes,
                    _ => &back[done - front.len()..],
                };
                let n = (count - done).min(frames.len()).min(passes.len());
                let frames = &mut frames[..n];
                self.compress
                    .process_stored(frames, &passes[..n], &self.levels);
                out.extend_from_slice(frames);
                done += n;
            }
            self.passes.drain(..count);
        } else {
            let (front, back) = self.pending.as_mut_slices();
            let split = count.min(front.len());
            for done in [&mut front[..split], &mut back[..count - split]] {
                self.compress.process(done, &self.section, &self.levels);
                out.extend_from_slice(done);
            }
        }
        self.pending.drain(..count);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bursts of a 200 Hz tone at levels that change every 1.3 s, with a
    /// quiet passage.
    fn signal(frames: usize) -> Vec<Stereo> {
        (0..frames)
            .map(|i| {
                let t = i as f64 / 48000.0;
                let level = [0.9, 0.2, 0.6, 0.05, 0.8][(i / 62_400) % 5];
                let burst = 0.4 + 0.6 * (-((i % 9_600) as f64) / 1_200.0).exp();
                let x = level * burst * (std::f64::consts::TAU * 200.0 * t).sin();
                [x as f32, (0.7 * x) as f32]
            })
            .collect()
    }

    /// The plan of a stored recording, read in spans of 60 s, equals the
    /// plan of the recording in memory (D247).
    #[test]
    fn a_stored_recording_gives_the_plan_of_the_recording() {
        let audio = signal(48_000 * 75 + 77);
        let whole = plan(&audio, None, 48000).expect("a plan");
        let temporal = crate::analysis::analyze(&audio, 48000);
        let mut stored = crate::scratch::Pcm::temporary_of(&audio).unwrap();
        let read = plan_with(&mut stored, None, &temporal, 48000)
            .unwrap()
            .expect("a plan");
        assert_eq!(
            serde_json::to_value(read).unwrap(),
            serde_json::to_value(whole).unwrap()
        );
    }

    /// The glue settings of a stored recording, read in spans, equal those
    /// of the recording in memory (D250).
    #[test]
    fn a_stored_recording_gives_the_glue_settings_of_the_recording() {
        let audio = signal(3 * GLUE_SPAN + 4_321);
        let whole = glue_settings(&audio, 48000).expect("settings");
        let mut stored = crate::scratch::Pcm::temporary_of(&audio).unwrap();
        let read = glue_settings_read(&mut stored, 48000)
            .unwrap()
            .expect("settings");
        assert_eq!(
            read.threshold_dbfs.to_bits(),
            whole.threshold_dbfs.to_bits()
        );
        assert_eq!(
            serde_json::to_value(read).unwrap(),
            serde_json::to_value(whole).unwrap()
        );
    }

    /// The hold shift of a recording gives the change of the held bands of
    /// each section of two runs of the compressor with the threshold moved
    /// down and up, against the power of the section (D280).
    #[test]
    fn the_hold_shift_gives_the_held_bands_of_a_moved_threshold() {
        let rate = 48_000;
        // Steady tones at 60 and 180 Hz under bursts of 2 kHz at levels
        // that change every 1.3 s: the bursts move the gain, and the gain
        // moves the tones.
        let audio: Vec<Stereo> = (0..rate as usize * 40)
            .map(|i| {
                let t = i as f64 / rate as f64;
                let level = [0.9, 0.2, 0.6, 0.05, 0.8][(i / 62_400) % 5];
                let burst = (-((i % 9_600) as f64) / 1_200.0).exp();
                let x = 0.2 * (std::f64::consts::TAU * 60.0 * t).sin()
                    + 0.1 * (std::f64::consts::TAU * 180.0 * t).sin()
                    + level * burst * (std::f64::consts::TAU * 2_000.0 * t).sin();
                [x as f32, (0.8 * x) as f32]
            })
            .collect();
        let settings = Settings {
            threshold_dbfs: -2.0,
            ratio: 2.0,
            knee_db: 6.0,
            attack_ms: 50.0,
            release_ms: 270.0,
            detector_rms_ms: 10.0,
            sidechain_highpass_hz: 70.7,
            maximum_reduction_db: 6.0,
            section_ms: 4000.0,
            section_makeup: true,
        };
        let mut base = audio.clone();
        let stats = process_with_hold_shift(&mut base, settings, rate).unwrap();
        assert!(process(&mut audio.clone(), settings, rate)
            .unwrap()
            .hold_shift
            .is_none());
        let shift = stats.hold_shift.expect("a hold shift");
        let sections = |audio: &[Stereo]| {
            let held = crate::low_end::sections(audio, rate);
            let span = crate::low_end::section_frames(rate);
            let power: Vec<f64> = audio
                .chunks(span)
                .map(|c| {
                    c.iter()
                        .map(|f| (f[0] as f64).powi(2) + (f[1] as f64).powi(2))
                        .sum()
                })
                .collect();
            (held, power)
        };
        let (held, power) = sections(&base);
        assert_eq!(shift.change_db.len(), 3);
        assert_eq!(held.power_db.len(), 3);
        for (k, level) in [-HOLD_SHIFT_DB, HOLD_SHIFT_DB].into_iter().enumerate() {
            let mut moved = audio.clone();
            let s = Settings {
                threshold_dbfs: settings.threshold_dbfs - level,
                ..settings
            };
            let moved_stats = process(&mut moved, s, rate).unwrap();
            let reduction = moved_stats.mean_reduction_db - stats.mean_reduction_db;
            assert!((shift.reduction_db[k] - reduction).abs() < 0.01, "{k}");
            let (moved_held, moved_power) = sections(&moved);
            for s in 0..3 {
                let back = 10.0 * (moved_power[s] / power[s]).log10();
                for b in 0..2 {
                    let real = moved_held.power_db[s][b] - held.power_db[s][b] - back;
                    let shadow = shift.change_db[s][k][b];
                    assert!(
                        (shadow - real).abs() < 0.01,
                        "{k} {s} {b}: {shadow} against {real}"
                    );
                }
            }
            // More reduction lowers the bursts, and the make-up raises the tone.
            assert!(
                shift.change_db[0][k][0] * level > 0.0,
                "{k} {:?}",
                shift.change_db[0]
            );
        }
    }

    /// The quantiles of the level over the threshold give the mean static
    /// reduction of the frames (D279).
    #[test]
    fn the_level_quantiles_give_the_mean_static_reduction() {
        let settings = Settings {
            threshold_dbfs: -2.0,
            ratio: 2.0,
            knee_db: 6.0,
            attack_ms: 30.0,
            release_ms: 170.0,
            detector_rms_ms: 10.0,
            sidechain_highpass_hz: 56.0,
            maximum_reduction_db: 6.0,
            section_ms: 4000.0,
            section_makeup: true,
        };
        // One frame in each bin from 10 dB under to 6 dB over the
        // threshold.
        let mut bins = vec![0u64; OVER_BINS];
        let span = over_bin(-10.0)..over_bin(6.0);
        for b in span.clone() {
            bins[b] = 1;
        }
        let q = over_quantiles(&bins).unwrap();
        assert!((q[0] + 10.0 - 0.5 * OVER_STEP_DB).abs() < 1e-9);
        assert!((q[OVER_QUANTILES - 1] - 6.0 + 0.5 * OVER_STEP_DB).abs() < 1e-9);
        assert!(q.windows(2).all(|w| w[0] < w[1]));
        let centre = |b: usize| (b as f64 + 0.5) * OVER_STEP_DB - OVER_SPAN_DB;
        let exact = span
            .clone()
            .map(|b| reduction_over_db(centre(b), settings))
            .sum::<f64>()
            / span.len() as f64;
        assert!((mean_reduction_over(&q, settings, 0.0) - exact).abs() < 0.02);
        assert!(over_quantiles(&vec![0; OVER_BINS]).is_none());
        // A render that compresses holds the quantiles, and one that leaves
        // the signal as it is holds none.
        let mut audio = signal(48_000 * 5);
        let stats = process(
            &mut audio,
            Settings {
                threshold_dbfs: -14.0,
                ..settings
            },
            48000,
        )
        .unwrap();
        let held = stats.over_quantiles_db.expect("quantiles");
        assert!(held.windows(2).all(|w| w[0] <= w[1]));
        let none = Settings {
            ratio: 1.0,
            ..settings
        };
        assert!(process(&mut audio, none, 48000)
            .unwrap()
            .over_quantiles_db
            .is_none());
    }

    #[test]
    fn the_stream_equals_the_compressor_of_the_whole_signal() {
        let settings = Settings {
            threshold_dbfs: -14.0,
            ratio: 4.0,
            knee_db: 6.0,
            attack_ms: 10.0,
            release_ms: 150.0,
            detector_rms_ms: 5.0,
            sidechain_highpass_hz: 80.0,
            maximum_reduction_db: 8.0,
            section_ms: 3000.0,
            section_makeup: true,
        };
        let frames = 1_000_003;
        let input = signal(frames);
        for settings in [
            settings,
            Settings {
                section_makeup: false,
                ..settings
            },
            Settings {
                section_ms: 0.0,
                section_makeup: false,
                ..settings
            },
        ] {
            let mut whole = input.clone();
            let stats = process(&mut whole, settings, 48000).unwrap();
            assert!(stats.maximum_reduction_db > 1.0);
            assert!(!settings.section_makeup || stats.maximum_makeup_db > 0.5);
            let held = process_with_hold_shift(&mut input.clone(), settings, 48000).unwrap();
            assert!(held.hold_shift.is_some());
            for span in [1, 4_799, 65_536, frames] {
                // The spans of 4,799 frames also give the hold shift.
                let hold = span == 4_799;
                let mut stream = Stream::new(settings, 48000, frames, span);
                if hold {
                    stream = stream.with_hold_shift(48000);
                }
                let mut out = Vec::new();
                for part in input.chunks(span) {
                    stream.push(part, &mut out);
                    assert!(stream.pending.len() <= Stream::lag(settings, 48000));
                }
                let streamed = stream.finish(&mut out);
                let bits =
                    |a: &[Stereo]| a.iter().flatten().map(|v| v.to_bits()).collect::<Vec<_>>();
                assert_eq!(bits(&out), bits(&whole), "span {span}");
                assert_eq!(
                    serde_json::to_string(&streamed).unwrap(),
                    serde_json::to_string(if hold { &held } else { &stats }).unwrap()
                );
            }
        }
    }
}
