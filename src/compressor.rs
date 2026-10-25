use crate::{
    analysis::percentile,
    audio_io::{Result, Stereo},
    dsp::gain,
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
}

fn coefficient(ms: f64, rate: u32) -> f64 {
    (-1.0 / (rate as f64 * ms * 0.001)).exp()
}

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
    let levels: Vec<_> = audio
        .iter()
        .enumerate()
        .filter_map(|(i, &frame)| {
            let level = detector.tick(frame);
            ((i + 1) % block == 0 || i + 1 == audio.len()).then_some(level)
        })
        .collect();
    let floor = (levels.iter().copied().fold(-300.0, f64::max) - 40.0).max(-100.0);
    let active: Vec<_> = levels.iter().copied().filter(|&v| v > floor).collect();
    if active.len() < 100 {
        return None;
    }
    settings.threshold_dbfs = percentile(&active, 0.7);
    Some(settings)
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
    fn new(
        audio: &[Stereo],
        settings: Settings,
        temporal: &crate::analysis::Temporal,
        rate: u32,
    ) -> Self {
        let block = section_block(rate);
        let section = section_levels(audio, settings, rate);
        let mut detector = Detector::new(settings, rate);
        let steps = audio.len().div_ceil(block) * SEARCH_STEPS;
        let (mut relative, mut energy) = (Vec::with_capacity(steps), Vec::with_capacity(steps));
        let mut level = -300.0;
        for (b, chunk) in audio.chunks(block).enumerate() {
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
        Self {
            relative,
            energy,
            onsets: temporal
                .events
                .iter()
                .map(|e| (e.start_seconds * 100.0).round() as usize)
                .collect(),
            active: temporal.windows.iter().map(|w| w.active).collect(),
            half: section_half(settings),
        }
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
    base.validate().ok()?;
    let temporal = crate::analysis::analyze(audio, rate);
    if temporal.windows.iter().filter(|w| w.active).count() < 2 {
        return None;
    }
    let search = Search::new(audio, base, &temporal, rate);
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
    Some(Plan {
        rule: MASTERING_RULE,
        punchy_attack_ms,
        attack_source,
        punch_by_attack,
        styles,
    })
}

/// The Punchy style of `plan`: the compression of the `master` version.
pub fn mastering(audio: &[Stereo], timing: Option<Timing>, rate: u32) -> Option<Mastering> {
    plan(audio, timing, rate).map(|p| p.style(Style::Punchy).clone())
}

/// Positive attenuation in dB, before attack/release smoothing.
pub fn static_reduction_db(level_dbfs: f64, settings: Settings) -> f64 {
    let over = level_dbfs - settings.threshold_dbfs;
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
    if settings.section_ms == 0.0 {
        return Vec::new();
    }
    let mut detector = Detector::new(settings, rate);
    let block = section_block(rate);
    let mut blocks = Vec::with_capacity(audio.len().div_ceil(block));
    for chunk in audio.chunks(block) {
        let sum: f64 = chunk.iter().map(|&f| detector.instant(f)).sum();
        blocks.push(sum / chunk.len() as f64);
    }
    let half = section_half(settings);
    let mut prefix = Vec::with_capacity(blocks.len() + 1);
    prefix.push(0.0f64);
    for b in &blocks {
        prefix.push(prefix.last().unwrap() + b);
    }
    (0..blocks.len())
        .map(|i| {
            let (a, b) = (i.saturating_sub(half), (i + half + 1).min(blocks.len()));
            10.0 * ((prefix[b] - prefix[a]) / (b - a) as f64)
                .max(1e-30)
                .log10()
        })
        .collect()
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
        .map(|i| {
            let range = i.saturating_sub(half)..(i + half + 1).min(input.len());
            let a: f64 = input[range.clone()].iter().sum();
            let b: f64 = output[range].iter().sum();
            if a > 0.0 && b > 0.0 {
                10.0 * (a / b).log10()
            } else {
                0.0
            }
        })
        .collect()
}

/// Section make-up of `settings` on `audio`, per 10 ms block, in dB.
fn section_makeup(audio: &[Stereo], section: &[f64], settings: Settings, rate: u32) -> Vec<f64> {
    let block = section_block(rate);
    let mut detector = Detector::new(settings, rate);
    let attack = coefficient(settings.attack_ms, rate);
    let release = coefficient(settings.release_ms, rate);
    let blocks = audio.len().div_ceil(block);
    let (mut input, mut output) = (vec![0.0f64; blocks], vec![0.0f64; blocks]);
    let mut reduction = 0.0;
    for (i, &frame) in audio.iter().enumerate() {
        let target = reduction_at(detector.tick(frame), i, section, settings, rate);
        let smoothing = if target > reduction { attack } else { release };
        reduction = smoothing * reduction + (1.0 - smoothing) * target;
        let power = (frame[0] as f64).powi(2) + (frame[1] as f64).powi(2);
        input[i / block] += power;
        output[i / block] += power * gain(-2.0 * reduction);
    }
    window_makeup(&input, &output, section_half(settings))
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

/// Static reduction at sample `index`, with the threshold relative to the
/// section level when `section` is not empty.
fn reduction_at(level: f64, index: usize, section: &[f64], settings: Settings, rate: u32) -> f64 {
    match section.get(index / section_block(rate)) {
        Some(section) => {
            let mut local = settings;
            local.threshold_dbfs = section + settings.threshold_dbfs;
            static_reduction_db(level, local)
        }
        None => static_reduction_db(level, settings),
    }
}

/// Compress `audio`, at `rate`, with `settings`.
pub fn process(audio: &mut [Stereo], settings: Settings, rate: u32) -> Result<Stats> {
    settings.validate()?;
    if audio.iter().flatten().any(|v| !v.is_finite()) {
        return Err("Compressor input must be finite".into());
    }
    if settings.ratio == 1.0 || settings.maximum_reduction_db == 0.0 || audio.is_empty() {
        return Ok(Stats::default());
    }
    let section = section_levels(audio, settings, rate);
    let makeup = if settings.section_makeup {
        section_makeup(audio, &section, settings, rate)
    } else {
        Vec::new()
    };
    let block = section_block(rate);
    let mut detector = Detector::new(settings, rate);
    let attack = coefficient(settings.attack_ms, rate);
    let release = coefficient(settings.release_ms, rate);
    let mut reduction = 0.0;
    let mut sum = 0.0;
    let mut active = 0usize;
    let mut stats = Stats::default();
    for (i, frame) in audio.iter_mut().enumerate() {
        let target = reduction_at(detector.tick(*frame), i, &section, settings, rate);
        let smoothing = if target > reduction { attack } else { release };
        reduction = smoothing * reduction + (1.0 - smoothing) * target;
        let g = gain(makeup_at(&makeup, i, block) - reduction) as f32;
        frame[0] *= g;
        frame[1] *= g;
        if reduction > stats.maximum_reduction_db {
            stats.maximum_reduction_db = reduction;
            stats.maximum_reduction_sample = i;
        }
        sum += reduction;
        active += usize::from(reduction > 0.25);
    }
    stats.mean_reduction_db = sum / audio.len() as f64;
    stats.frames_above_quarter_db_percent = 100.0 * active as f64 / audio.len() as f64;
    if !makeup.is_empty() {
        stats.mean_makeup_db = makeup.iter().sum::<f64>() / makeup.len() as f64;
        stats.maximum_makeup_db = makeup.iter().copied().fold(0.0, f64::max);
    }
    Ok(stats)
}
