//! Requests that change a delivered version.
//!
//! A request is a number of steps on each control. The builder makes the
//! candidates of one version from the parent spec: the full steps, then
//! three quarters and half of them. `distributed::render_family` renders
//! them with `essential` as the reference, and solves the low-end hold and
//! the peak-tone restoration of each candidate again.
use crate::{
    audio_io::{Result, Stereo},
    broad_tone,
    dsp::{self, EqBand, Kind},
    selection::{Candidate, Gates, Signature},
    treatment::Tone,
    treatment_render::Spec,
    Spectrum,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// One step: the duplicate threshold of the set, in dB or LU.
pub const STEP_DB: f64 = crate::selection::DISTINCT_DB;
/// Largest number of steps of a control in each direction.
pub const MAXIMUM_STEPS: i32 = 4;
/// Range of the loudness target, in LUFS: the lowest base target of the set
/// and the cap of a Denser target.
pub const LOUDNESS_RANGE_LUFS: (f64, f64) = (crate::set::BASE_TARGET_LUFS.0, -6.5);
/// Corner of the Bass shelf, in Hz: the corner of the equal-loudness bass
/// shelf (`broad_tone::Move::LevelBass`).
pub const BASS_SHELF_HZ: f64 = 150.0;
/// Region of the Bass measure, in Hz: the two held bands of the low-end hold.
pub const BASS_HZ: (f64, f64) = (crate::low_end::BANDS_HZ[0].0, crate::low_end::BANDS_HZ[1].1);
/// Shares of the requested steps, in the order of the candidates.
pub const SHARES: [f64; 3] = [1.0, 0.75, 0.5];
/// Largest filter gain of a control, in dB (`Tone::validate`).
const MAXIMUM_GAIN_DB: f64 = 6.0;
const SHELF_Q: f64 = std::f64::consts::FRAC_1_SQRT_2;
pub const RULE: &str = "adjust.steps.v1";
/// Region of the Width measure, in Hz: the side/mid of the width gate of
/// the set (`selection::Gates::minimum_width_db`).
pub const WIDTH_HZ: (f64, f64) = (500.0, 12000.0);
/// Move of the compressor threshold for each Compression step, in dB. At
/// 2:1 it moves the gain reduction above the knee by one step.
pub const COMPRESSION_THRESHOLD_DB: f64 = 2.0 * STEP_DB;
/// Largest number of Ceiling steps down from the ceiling of the set.
pub const MAXIMUM_CEILING_STEPS: i32 = 3;

/// Steps of each control against the parent.
#[derive(Clone, Copy, Default, Serialize, Deserialize, PartialEq, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct Steps {
    pub bass: i32,
    pub presence: i32,
    pub air: i32,
    pub sibilance: i32,
    pub loudness: i32,
    /// Side/mid from 120 Hz up.
    pub width: i32,
    /// Compressor threshold, down for a positive step.
    pub compression: i32,
    /// Delivery ceiling, down from that of the set. 0 or negative.
    pub ceiling: i32,
}

impl Steps {
    pub fn validate(&self) -> Result<()> {
        let all = [
            self.bass,
            self.presence,
            self.air,
            self.sibilance,
            self.loudness,
            self.width,
            self.compression,
            self.ceiling,
        ];
        if all.iter().any(|s| s.abs() > MAXIMUM_STEPS) {
            return Err(format!("A control has at most {MAXIMUM_STEPS} steps each way").into());
        }
        if !(-MAXIMUM_CEILING_STEPS..=0).contains(&self.ceiling) {
            return Err(
                format!("The ceiling goes down by at most {MAXIMUM_CEILING_STEPS} steps").into(),
            );
        }
        if all.iter().all(|s| *s == 0) {
            return Err("A request needs at least one step".into());
        }
        Ok(())
    }
}

/// Where the source has the band of a control.
#[derive(Clone, Copy, Debug)]
pub struct Availability {
    /// The source band reaches the top of the air region
    /// (`source_condition.bandwidth_hz`).
    pub air: bool,
    /// The distribution of the keyed frames of the parent's de-esser, when
    /// it has a de-esser plan with keyed frames.
    pub sibilance: Option<Sibilance>,
    /// The parent has a compressor.
    pub compression: bool,
}

/// The keyed frames of the de-esser of a render (`deesser::Stats`).
#[derive(Clone, Copy, Debug)]
pub struct Sibilance {
    /// `deesser::Stats::excess_quantiles_db`.
    pub excess_db: [f64; crate::deesser::EXCESS_QUANTILES],
    /// `deesser::Stats::vocal_share_bins`.
    pub vocal_shares: [f64; crate::deesser::EXCESS_QUANTILES - 1],
    /// `deesser::Stats::reference_excess_db`.
    pub reference_excess_db: f64,
}

impl Sibilance {
    /// The keyed frames of `stats`, the de-esser stats of a render report.
    fn of(stats: &Value) -> Option<Self> {
        Some(Self {
            excess_db: serde_json::from_value(stats["excess_quantiles_db"].clone()).ok()?,
            vocal_shares: serde_json::from_value(stats["vocal_share_bins"].clone()).ok()?,
            reference_excess_db: stats["reference_excess_db"].as_f64()?,
        })
    }
}

impl Availability {
    /// The controls of `parent` with its render report `render`.
    pub fn of(parent: &Spec, render: &Value, top_hz: f64) -> Self {
        Self {
            air: top_hz >= broad_tone::AIR_HZ.1,
            sibilance: Sibilance::of(&render["deesser"]).filter(|_| {
                parent.deesser.is_some() && render["deesser"]["keyed_frames"].as_u64() > Some(0)
            }),
            compression: parent.compressor.is_some(),
        }
    }
}

/// Mean de-esser cut of `plan` over the keyed frames of `sibilance`, with
/// its threshold `shift_db` higher and its largest cut `shift_db` lower, at
/// most `deesser::CUT_LIMIT_DB`, in dB. The threshold is the reference excess of the parent's render (the
/// excess at `deesser::THRESHOLD_QUANTILE`, or the reference of the delivered
/// render) and the threshold of the plan (`set.deesser.v4`); each bin of the
/// excess quantiles cuts by its mean vocal share.
fn mean_cut(sibilance: &Sibilance, plan: &crate::deesser::Plan, shift_db: f64) -> f64 {
    crate::deesser::mean_cut_over(
        &sibilance.excess_db,
        &sibilance.vocal_shares,
        sibilance.reference_excess_db + plan.threshold_db + shift_db,
        shifted_cut_db(plan, shift_db),
    )
}

/// The largest cut of `plan` after a Sibilance shift of `shift_db`: lower by
/// the shift, at most `deesser::CUT_LIMIT_DB`.
fn shifted_cut_db(plan: &crate::deesser::Plan, shift_db: f64) -> f64 {
    (plan.maximum_cut_db - shift_db).min(crate::deesser::CUT_LIMIT_DB)
}

/// Range of a Sibilance shift, in dB: the range of
/// `deesser::Plan::threshold_db`, 12 dB down and 6 dB up.
const SIBILANCE_SHIFT_DB: (f64, f64) = crate::deesser::THRESHOLD_RANGE_DB;

/// The shift of the threshold and the largest cut of `plan` that changes the
/// mean cut over the keyed frames by `-change_db`: more sibilance for a
/// positive change. A step down lowers the threshold to at most 12 dB under
/// the reference excess and raises the largest cut by the same shift, to
/// at most `deesser::CUT_LIMIT_DB`. A positive change at or above the mean
/// cut takes the cut to 0 dB: the shift is the largest cut, with the
/// threshold at most 6 dB over the reference (`set.deesser.v4`, D209). After
/// steps down, a plan can have a threshold under -3 dB and a largest cut
/// over 6 dB, and this shift is then larger than 6 dB. None when no other
/// shift within `SIBILANCE_SHIFT_DB` gives the change, or for a positive
/// change on a plan that cuts nothing.
fn sibilance_shift(
    sibilance: &Sibilance,
    plan: &crate::deesser::Plan,
    change_db: f64,
) -> Option<f64> {
    if change_db == 0.0 {
        return Some(0.0);
    }
    let (down, up) = SIBILANCE_SHIFT_DB;
    let low = down.max(down - plan.threshold_db);
    let high = up.min(up - plan.threshold_db);
    let before = mean_cut(sibilance, plan, 0.0);
    if change_db > 0.0 && before <= 0.0 {
        return None;
    }
    if change_db >= before {
        return Some(plan.maximum_cut_db.min(up - plan.threshold_db));
    }
    let target = before - change_db;
    // The mean cut falls as the shift rises.
    if mean_cut(sibilance, plan, low) < target || mean_cut(sibilance, plan, high) > target {
        return None;
    }
    let (mut a, mut b) = (low, high);
    for _ in 0..40 {
        let m = 0.5 * (a + b);
        if mean_cut(sibilance, plan, m) > target {
            a = m;
        } else {
            b = m;
        }
    }
    Some(0.5 * (a + b))
}

/// The side gain of `parent` with `change_db` on each band from 120 Hz up,
/// within the range of a side gain. The band under 120 Hz and the
/// decorrelated mid stay.
pub fn widened(
    parent: Option<&crate::width::SideGain>,
    change_db: f64,
) -> Option<crate::width::SideGain> {
    if change_db == 0.0 {
        return parent.copied();
    }
    let mut side = parent
        .copied()
        .unwrap_or(crate::width::SideGain::gains([0.0; 6]));
    for g in &mut side.bands_db[1..] {
        *g = (*g + change_db).clamp(-24.0, crate::width::WIDEN_MAXIMUM_DB);
    }
    Some(side)
}

/// Bands per octave of the stored source spectrum.
pub const BANDS_PER_OCTAVE: f64 = 24.0;
/// Lower edge of the first band of the stored source spectrum, in Hz. The
/// bins under it make one band.
pub const LOWEST_BAND_HZ: f64 = 20.0;

/// The source spectrum in bands of `1 / BANDS_PER_OCTAVE` octave: the input
/// of each prediction of a request. A request on any path predicts from the
/// same stored bands, so the cloud, the browser and the native render build
/// the same candidates.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BandSpectrum {
    /// The rate of the measured audio, in Hz.
    pub sample_rate: u32,
    /// Mean frequency of the bins of each band, in Hz.
    pub centers_hz: Vec<f64>,
    /// FFT bins in each band.
    pub bins: Vec<u32>,
    /// Sum of the mid power of the bins of each band.
    pub mid_power: Vec<f64>,
    /// Sum of the side power of the bins of each band.
    pub side_power: Vec<f64>,
}

impl BandSpectrum {
    /// The bands as a spectrum of one bin each, for the predictions of the
    /// low-end hold (`low_end::effect`).
    pub fn spectrum(&self) -> Spectrum {
        Spectrum {
            sample_rate: self.sample_rate,
            centers_hz: self.centers_hz.clone(),
            band_power: self
                .mid_power
                .iter()
                .zip(&self.side_power)
                .map(|(m, s)| m + s)
                .collect(),
            mid_power: self.mid_power.clone(),
            side_power: self.side_power.clone(),
            mel_power: Vec::new(),
            analyzed_frames: 1,
        }
    }

    pub fn of(spectrum: &Spectrum) -> Self {
        let band = |hz: f64| {
            if hz < LOWEST_BAND_HZ {
                0
            } else {
                1 + (BANDS_PER_OCTAVE * (hz / LOWEST_BAND_HZ).log2()).floor() as usize
            }
        };
        let mut sums: Vec<(f64, u32, f64, f64)> = Vec::new();
        for (i, &hz) in spectrum.centers_hz.iter().enumerate() {
            if hz <= 0.0 {
                continue;
            }
            let b = band(hz);
            if sums.len() <= b {
                sums.resize(b + 1, (0.0, 0, 0.0, 0.0));
            }
            let s = &mut sums[b];
            s.0 += hz;
            s.1 += 1;
            s.2 += spectrum.mid_power[i];
            s.3 += spectrum.side_power[i];
        }
        sums.retain(|s| s.1 > 0);
        Self {
            sample_rate: spectrum.sample_rate,
            centers_hz: sums.iter().map(|s| s.0 / s.1 as f64).collect(),
            bins: sums.iter().map(|s| s.1).collect(),
            mid_power: sums.iter().map(|s| s.2).collect(),
            side_power: sums.iter().map(|s| s.3).collect(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        let n = self.centers_hz.len();
        if n == 0
            || crate::audio_io::check_render_rate(self.sample_rate).is_err()
            || self.bins.len() != n
            || self.mid_power.len() != n
            || self.side_power.len() != n
            || self.centers_hz.windows(2).any(|w| w[1] <= w[0])
            || self.bins.contains(&0)
            || self
                .mid_power
                .iter()
                .chain(&self.side_power)
                .any(|p| !p.is_finite() || *p < 0.0)
        {
            return Err("Invalid stored source spectrum".into());
        }
        Ok(())
    }
}

/// Power of the source in band `i` of `spectrum` after `tone` and the side
/// gain `side`.
fn shaped_power(
    spectrum: &BandSpectrum,
    tone: &Tone,
    side: Option<&crate::width::SideGain>,
    i: usize,
) -> f64 {
    let hz = spectrum.centers_hz[i];
    let rate = spectrum.sample_rate;
    let response = |filters: &[EqBand]| {
        10f64.powf(
            filters
                .iter()
                .map(|b| b.filter(rate).response_db(hz, rate))
                .sum::<f64>()
                / 10.0,
        )
    };
    let stereo = response(&tone.stereo);
    let widening = side.map_or(1.0, |g| 10f64.powf(g.response_db(hz) / 10.0));
    // Exactly 1 without the fold of a preset.
    let mid_gain = side.map_or(1.0, |g| g.mid_power(hz));
    stereo
        * (spectrum.mid_power[i] * response(&tone.mid) * mid_gain
            + spectrum.side_power[i] * response(&tone.side) * widening)
}

/// Mean power of the bins of `spectrum` in `region` after the tone.
fn region_power(
    spectrum: &BandSpectrum,
    tone: &Tone,
    side: Option<&crate::width::SideGain>,
    (low, high): (f64, f64),
) -> f64 {
    let (mut power, mut bins) = (0.0, 0u32);
    for i in 0..spectrum.centers_hz.len() {
        if (low..high).contains(&spectrum.centers_hz[i]) {
            power += shaped_power(spectrum, tone, side, i);
            bins += spectrum.bins[i];
        }
    }
    power / bins.max(1) as f64
}

/// The predicted measures of a control after `tone`, in dB.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
pub struct Measures {
    /// 40–250 Hz against 500 Hz–2 kHz, the reference of Presence. Against
    /// the K-weighted power of the whole band, a shelf on a mix whose
    /// loudness is mostly bass raises the reference with the bass.
    pub bass_db: f64,
    /// 2–5 kHz against 500 Hz–2 kHz.
    pub presence_db: f64,
    /// 8–16 kHz against 2–5 kHz.
    pub air_db: f64,
}

pub fn predicted(
    spectrum: &BandSpectrum,
    tone: &Tone,
    side: Option<&crate::width::SideGain>,
) -> Measures {
    let ratio = |a, b| {
        10.0 * (region_power(spectrum, tone, side, a).max(1e-30)
            / region_power(spectrum, tone, side, b).max(1e-30))
        .log10()
    };
    Measures {
        bass_db: ratio(BASS_HZ, broad_tone::BODY_HZ),
        presence_db: ratio(broad_tone::PRESENCE_HZ, broad_tone::BODY_HZ),
        air_db: ratio(broad_tone::AIR_HZ, broad_tone::PRESENCE_HZ),
    }
}

/// K-weighted power of `spectrum` after the tone and the side gain, against
/// the power without them, in dB: the predicted change of the loudness of
/// the prefix.
pub fn predicted_loudness_db(
    spectrum: &BandSpectrum,
    tone: &Tone,
    side: Option<&crate::width::SideGain>,
) -> f64 {
    let (mut after, mut before) = (0.0, 0.0);
    for (i, &hz) in spectrum.centers_hz.iter().enumerate() {
        let k = dsp::k_weighting_power(hz, spectrum.sample_rate);
        after += k * shaped_power(spectrum, tone, side, i);
        before += k * (spectrum.mid_power[i] + spectrum.side_power[i]);
    }
    10.0 * (after.max(1e-30) / before.max(1e-30)).log10()
}

/// Side/mid of `spectrum` over `WIDTH_HZ` after the side gain, in dB.
pub fn side_mid_db(spectrum: &BandSpectrum, side: Option<&crate::width::SideGain>) -> f64 {
    let (mut s, mut m) = (0.0, 0.0);
    for (i, &hz) in spectrum.centers_hz.iter().enumerate() {
        if (WIDTH_HZ.0..WIDTH_HZ.1).contains(&hz) {
            s +=
                spectrum.side_power[i] * side.map_or(1.0, |g| 10f64.powf(g.response_db(hz) / 10.0));
            m += spectrum.mid_power[i] * side.map_or(1.0, |g| g.mid_power(hz));
        }
    }
    10.0 * (s.max(1e-30) / m.max(1e-30)).log10()
}

/// The measures of the controls on a delivered version.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Delivered {
    pub bass_db: f64,
    pub presence_db: f64,
    pub air_db: f64,
    /// Side/mid over `WIDTH_HZ`.
    pub width_db: f64,
}

/// The measures of the controls on `audio` at `rate`.
pub fn measure(audio: &[Stereo], rate: u32) -> Delivered {
    let spectrum = BandSpectrum::of(&crate::spectrum(audio, rate));
    let m = predicted(&spectrum, &Tone::default(), None);
    Delivered {
        bass_db: m.bass_db,
        presence_db: m.presence_db,
        air_db: m.air_db,
        width_db: side_mid_db(&spectrum, None),
    }
}

/// What a delivered version stores for the requests on it and for the
/// duplicate screen of an adjusted version: the signature of the set
/// (`selection::Signature`) and the measures of the controls.
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct VersionFacts {
    pub signature: Signature,
    pub measures: Delivered,
}

impl VersionFacts {
    /// The facts of a render at `rate` whose loudness is `lufs` and whose
    /// mean limiter reduction is `mean_reduction_db`.
    pub fn of(audio: &[Stereo], lufs: f64, mean_reduction_db: f64, rate: u32) -> Result<Self> {
        Ok(Self {
            signature: Signature::new(
                &crate::metrics(audio, rate),
                &crate::stereo::analyze(audio, rate)?,
                lufs,
                mean_reduction_db,
            ),
            measures: measure(audio, rate),
        })
    }

    /// The facts that a version record stores (`signature`, `measures`).
    pub fn from_record(record: &Value) -> Option<Self> {
        Some(Self {
            signature: serde_json::from_value(record["signature"].clone()).ok()?,
            measures: serde_json::from_value(record["measures"].clone()).ok()?,
        })
    }
}

/// The change of the held bands of a delivered version in each 15 s section
/// against its reference at matched loudness, less its intended move: the
/// measure of the low-end hold (`low_end::changes`). The preview predicts
/// the hold of a request from it.
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct HoldSections {
    pub starts: Vec<usize>,
    pub ends: Vec<usize>,
    pub change_db: Vec<[Option<f64>; 2]>,
    /// The changes of the version without a hold: what the first step of
    /// its hold answered (`low_end::Step::changes_db`), or `change_db` when
    /// the version has no hold.
    pub free_change_db: Vec<[Option<f64>; 2]>,
}

impl HoldSections {
    /// The sections of a version with the spec `spec`, the hold steps
    /// `steps` of its chosen candidate, and its sections and those of the
    /// reference.
    pub fn of(
        spec: &Spec,
        spectrum: &Spectrum,
        steps: &[crate::low_end::Step],
        version: &crate::low_end::Sections,
        version_lufs: f64,
        reference: &crate::low_end::Sections,
        reference_lufs: f64,
    ) -> Self {
        let intended = crate::low_end::intended_db(spec, spectrum);
        let change_db =
            crate::low_end::changes(version, version_lufs, reference, reference_lufs, intended);
        let free_change_db = match (steps.first(), &spec.low_end_hold) {
            (Some(first), Some(_)) => first.changes_db.clone(),
            _ => change_db.clone(),
        };
        Self {
            starts: version.starts.clone(),
            ends: version.ends.clone(),
            change_db,
            free_change_db,
        }
    }

    /// The sections of a version record, when it stores them.
    pub fn of_record(record: &Value) -> Option<Self> {
        serde_json::from_value(record["hold_sections"].clone())
            .or_else(|_| serde_json::from_value(record["report"]["hold_sections"].clone()))
            .ok()
    }
}

pub const FACTS_RULE: &str = "adjust.facts.v1";

/// Facts of the planner that a request needs. The worker plan and the set
/// manifest store them.
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Facts {
    pub rule: String,
    pub spectrum: BandSpectrum,
    /// Audibility point of peak control on the compressed mix
    /// (`set.audible-loudness.v3`), in LUFS. A version's own point is
    /// measured when a request raises its loudness.
    pub audibility_lufs: Option<f64>,
    /// Top of the band of the source (`source_condition.bandwidth_hz`).
    pub bandwidth_hz: f64,
    /// The signature of the mix, for the duplicate screen.
    pub mix: Signature,
    /// The side bells of the Mono-safe preset (`width::mono_safe`), when the
    /// source condition reports an anti-phase instrument group (D175).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mono_safe: Option<Vec<crate::treatment::EnvelopedBand>>,
}

impl Facts {
    pub fn of(
        source: &crate::pipeline::SourceAnalysis,
        audibility_lufs: Option<f64>,
        bandwidth_hz: f64,
        mono_safe: Option<Vec<crate::treatment::EnvelopedBand>>,
    ) -> Self {
        let lufs = source.input.integrated_lufs.unwrap_or(f64::NAN);
        Self {
            rule: FACTS_RULE.into(),
            spectrum: BandSpectrum::of(&source.spectrum),
            audibility_lufs,
            bandwidth_hz,
            mix: Signature::new(&source.input, &source.stereo, lufs, 0.0),
            mono_safe,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.rule != FACTS_RULE || !self.bandwidth_hz.is_finite() {
            return Err("Unsupported planner facts".into());
        }
        self.spectrum.validate()
    }

    /// True when `signature` sounds the same as the mix (`set::same_as_mix`).
    pub fn same_as_mix(&self, signature: &Signature) -> bool {
        (signature.lufs - self.mix.lufs).abs() < crate::selection::DISTINCT_DB
            && signature.is_duplicate_of(&self.mix)
    }
}

/// One stereo filter of a control: the filter that the control moves, and
/// the filter that it adds when the parent has none.
struct Control {
    is: fn(&EqBand) -> bool,
    make: fn(f64) -> EqBand,
    measure: fn(&Measures) -> f64,
}

fn is_bass(b: &EqBand) -> bool {
    matches!(b.kind, Kind::LowShelf) && b.hz == BASS_SHELF_HZ
}

fn bass_shelf(gain_db: f64) -> EqBand {
    EqBand {
        kind: Kind::LowShelf,
        hz: BASS_SHELF_HZ,
        q: SHELF_Q,
        gain_db,
    }
}

fn is_presence(b: &EqBand) -> bool {
    matches!(b.kind, Kind::Peak)
        && b.hz == broad_tone::PRESENCE_BELL_HZ
        && b.q == broad_tone::LEAD_BELL_Q
}

fn presence_bell(gain_db: f64) -> EqBand {
    EqBand {
        kind: Kind::Peak,
        hz: broad_tone::PRESENCE_BELL_HZ,
        q: broad_tone::LEAD_BELL_Q,
        gain_db,
    }
}

fn is_air(b: &EqBand) -> bool {
    matches!(b.kind, Kind::HighShelf) && b.hz == broad_tone::AIR_SHELF_HZ && b.q == SHELF_Q
}

const BASS: Control = Control {
    is: is_bass,
    make: bass_shelf,
    measure: |m| m.bass_db,
};
const PRESENCE: Control = Control {
    is: is_presence,
    make: presence_bell,
    measure: |m| m.presence_db,
};
const AIR: Control = Control {
    is: is_air,
    make: broad_tone::air_shelf,
    measure: |m| m.air_db,
};

fn with_gain(tone: &Tone, control: &Control, gain_db: f64) -> Tone {
    let mut t = tone.clone();
    match t.stereo.iter().position(control.is) {
        Some(i) => t.stereo[i].gain_db = gain_db,
        None => t.stereo.push((control.make)(gain_db)),
    }
    t
}

/// `tone` with the filter of `control` at the gain that moves its predicted
/// measure by `change_db`. None when no gain within ±`MAXIMUM_GAIN_DB`
/// gives the change.
/// `effective` gives the tone that the render applies with a tone, such as
/// the tone with the hold bells of the parent: the prediction measures it.
fn moved(
    tone: &Tone,
    control: &Control,
    change_db: f64,
    spectrum: &BandSpectrum,
    side: Option<&crate::width::SideGain>,
    effective: &dyn Fn(&Tone) -> Tone,
) -> Option<Tone> {
    if change_db == 0.0 {
        return Some(tone.clone());
    }
    let measure = |t: &Tone| (control.measure)(&predicted(spectrum, &effective(t), side));
    let at = |g: f64| measure(&with_gain(tone, control, g));
    let target = measure(tone) + change_db;
    let (mut low, mut high) = (-MAXIMUM_GAIN_DB, MAXIMUM_GAIN_DB);
    if at(low) > target || at(high) < target {
        return None;
    }
    // The measure rises with the gain of each control filter.
    for _ in 0..40 {
        let middle = 0.5 * (low + high);
        if at(middle) < target {
            low = middle;
        } else {
            high = middle;
        }
    }
    Some(with_gain(tone, control, 0.5 * (low + high)))
}

/// The spec of `parent` with `share` of `steps`. The low-end hold of the
/// parent is removed: the render solves it again.
pub fn spec_at(
    parent: &Spec,
    steps: &Steps,
    share: f64,
    spectrum: &BandSpectrum,
    available: Availability,
) -> Result<Spec> {
    if (steps.air != 0 && !available.air)
        || (steps.sibilance != 0 && available.sibilance.is_none())
        || (steps.compression != 0 && !available.compression)
    {
        return Err("The request moves a control that this version does not have".into());
    }
    let change = |s: i32| s as f64 * STEP_DB * share;
    let side_gain = widened(parent.side_gain.as_ref(), change(steps.width));
    let side = side_gain.as_ref();
    // The parent sounds with its low-end hold and its low-mid correction,
    // and the final render solves a hold near it, so a step is predicted
    // on the tone with them: a hold that lifts the bass makes the bass a
    // larger share of the loudness, and a shelf moves the bass measure less.
    let effective = |t: &Tone| {
        let mut s = parent.clone();
        s.tone = t.clone();
        s.effective_tone().0
    };
    let mut tone = parent.tone.clone();
    for (control, s) in [
        (&BASS, steps.bass),
        (&PRESENCE, steps.presence),
        (&AIR, steps.air),
    ] {
        tone = moved(&tone, control, change(s), spectrum, side, &effective)
            .ok_or("The request is outside the range of the filters")?;
    }
    tone.validate(spectrum.sample_rate)?;
    let mut spec = parent.clone();
    spec.tone = tone;
    spec.side_gain = side_gain;
    // The final render solves the hold again and adds its bells to the
    // tone. A share whose tone leaves no room for a hold of the size of the
    // parent's is left out, so the next share renders instead.
    spec.effective_tone().0.validate(spectrum.sample_rate)?;
    spec.low_end_hold = None;
    if let Some(settings) = &mut spec.compressor {
        settings.threshold_dbfs -= steps.compression as f64 * COMPRESSION_THRESHOLD_DB * share;
        settings.validate()?;
    }
    // The ceiling is a delivery condition, so each share keeps it.
    spec.ceiling_dbtp = parent.ceiling_dbtp + steps.ceiling as f64 * STEP_DB;
    // A Sibilance step changes the mean cut over the keyed frames by 0.5 dB:
    // the threshold rises and the largest cut falls by the same shift, with
    // the largest cut at most `deesser::CUT_LIMIT_DB`.
    if let (Some(plan), Some(sibilance)) = (&mut spec.deesser, &available.sibilance) {
        let shift = sibilance_shift(sibilance, plan, change(steps.sibilance))
            .ok_or("The request is outside the range of the de-esser")?;
        plan.maximum_cut_db = shifted_cut_db(plan, shift);
        plan.threshold_db += shift;
    }
    spec.target_lufs = (parent.target_lufs + change(steps.loudness))
        .clamp(LOUDNESS_RANGE_LUFS.0, LOUDNESS_RANGE_LUFS.1);
    Ok(spec)
}

/// The reason of the candidate at `share` of the steps.
fn share_reason(share: f64) -> String {
    format!("{}% of the requested steps", (share * 100.0).round())
}

/// The share of the steps of a candidate, from its reason. A share that
/// the builder leaves out moves the later candidates up, so the index of a
/// candidate does not give its share.
fn share_of(reason: &str) -> Option<f64> {
    SHARES
        .into_iter()
        .find(|s| share_reason(*s) == reason || preset_share_reason(*s) == reason)
}

/// The candidates of a request: the full steps, then three quarters and
/// half of them. A share whose tone is not valid is left out.
pub fn candidates(
    parent: &Spec,
    steps: &Steps,
    spectrum: &BandSpectrum,
    available: Availability,
) -> Result<Vec<Candidate>> {
    steps.validate()?;
    let gates = Gates {
        tonal_image: true,
        low_end: true,
        workload_as_warning: true,
        ..Gates::default()
    };
    let mut candidates = Vec::new();
    let mut first_error = None;
    for share in SHARES {
        match spec_at(parent, steps, share, spectrum, available) {
            Ok(spec) => candidates.push(Candidate {
                reason: share_reason(share),
                spec,
                gates: gates.clone(),
            }),
            Err(e) => {
                first_error.get_or_insert(e);
            }
        }
    }
    match (candidates.is_empty(), first_error) {
        (true, Some(e)) => Err(e),
        _ => Ok(candidates),
    }
}

/// What a request needs from the plan of its set: the source, the planner
/// facts and the `essential` spec, the reference of the gates and of the
/// low-end hold.
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Base {
    pub source_pcm_sha256: String,
    pub source_frames: usize,
    pub source_condition: Value,
    pub facts: Facts,
    pub reference: Value,
}

impl Base {
    /// The base of a worker plan of a set.
    pub fn of_plan(set_plan: &Value) -> Result<Self> {
        if set_plan["schema"] != crate::distributed::PLAN_SCHEMA {
            return Err("Unsupported worker plan schema".into());
        }
        let families = set_plan["families"]
            .as_array()
            .ok_or("Worker plan without families")?;
        // Each family after `essential` names the `essential` spec as its
        // reference; the `essential` family renders it as its first
        // candidate.
        let reference = families
            .iter()
            .find_map(|f| (!f["reference"].is_null()).then(|| f["reference"].clone()))
            .or_else(|| {
                families
                    .iter()
                    .find(|f| f["id"] == "essential")
                    .map(|f| f["versions"][0]["candidates"][0]["spec"].clone())
            })
            .ok_or("Worker plan without an essential spec")?;
        let base = Self {
            source_pcm_sha256: set_plan["source_pcm_sha256"]
                .as_str()
                .ok_or("Worker plan without a source")?
                .into(),
            source_frames: set_plan["source_frames"]
                .as_u64()
                .ok_or("Worker plan without a source")? as usize,
            source_condition: set_plan["source_condition"].clone(),
            facts: facts_of(set_plan)?,
            reference,
        };
        Ok(base)
    }

    /// The render rate of the set, in Hz.
    pub fn sample_rate(&self) -> u32 {
        self.facts.spectrum.sample_rate
    }
}

/// A worker plan of one family that renders the request as version `id`.
pub fn worker_plan(
    base: &Base,
    id: &str,
    parent_id: &str,
    candidates: &[Candidate],
    request: &Request,
) -> Result<Value> {
    Ok(json!({
        "schema": crate::distributed::PLAN_SCHEMA,
        "sample_rate": base.sample_rate(),
        "source_pcm_sha256": base.source_pcm_sha256,
        "source_frames": base.source_frames,
        "source_condition": base.source_condition,
        "facts": base.facts,
        "families": [{
            "id": id,
            "reference": base.reference,
            "versions": [{
                "id": id,
                "rule": RULE,
                "basis": request.basis(parent_id)?,
                "role": "Base",
                "label": "",
                // The parent spec holds the sub lift of the parent.
                "lifted": true,
                "candidates": candidates.iter().map(|c| json!({
                    "reason": c.reason,
                    "spec": c.spec,
                    "gates": c.gates,
                })).collect::<Vec<_>>(),
            }],
        }],
    }))
}

/// The planner facts of a worker plan or a set manifest.
pub fn facts_of(record: &Value) -> Result<Facts> {
    let facts: Facts = serde_json::from_value(record["facts"].clone())
        .map_err(|_| "The set has no planner facts")?;
    facts.validate()?;
    Ok(facts)
}

/// Warnings of a delivered request.
pub fn warnings(
    lufs: f64,
    audibility_lufs: Option<f64>,
    mean_reduction_db: f64,
    parent_mean_reduction_db: f64,
) -> Vec<Value> {
    let mut warnings = Vec::new();
    if let Some(point) = audibility_lufs.filter(|p| lufs > *p) {
        warnings.push(json!({
            "id": "loudness-above-audibility-point",
            "rule": "set.audible-loudness.v3",
            "lufs": lufs,
            "audibility_lufs": point,
        }));
    }
    if mean_reduction_db - parent_mean_reduction_db > crate::selection::WORKLOAD_MEAN_DB {
        warnings.push(json!({
            "id": "limiter-workload",
            "rule": "selection.workload",
            "mean_reduction_db": mean_reduction_db,
            "parent_mean_reduction_db": parent_mean_reduction_db,
        }));
    }
    warnings
}

/// The record of `parent_id` in `versions`: a family result
/// (`mastering.worker-family.2`) or a set manifest.
pub fn parent_record<'v>(versions: &'v Value, parent_id: &str) -> Result<&'v Value> {
    versions["versions"]
        .as_array()
        .and_then(|v| v.iter().find(|r| r["id"] == parent_id))
        .ok_or_else(|| format!("No version {parent_id} in the versions record").into())
}

/// The render report of a version record: a set manifest keeps it under
/// `report`, a family result at the top.
fn render_report(record: &Value) -> &Value {
    if record["render"].is_object() {
        &record["render"]
    } else {
        &record["report"]["render"]
    }
}

/// Whether a set offers the Mono-safe preset. Off: on a phone speaker the
/// movement of the keyed cut is audible and distracting, and no form of the
/// cut gives a steady phone sound (D176). A side cut leaves the mono sum of one
/// speaker as it is, and on two speakers 14 cm apart each form of the cut
/// moves the bands from 2 to 6 kHz by 2.6 to 4.3 dB (D176).
pub const MONO_SAFE_OFFERED: bool = false;

/// A preset: a bundle of changes whose amounts the engine sets from the
/// measurements of the mix.
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Preset {
    /// A side cut per anti-phase band, keyed to the windows whose channels
    /// cancel (`width::mono_safe`). Offered when the source condition
    /// reports an anti-phase instrument group (D175).
    MonoSafe,
    /// The normalization level of the streaming services and the true-peak
    /// ceiling of a lossy codec (`adjust.streaming.v1`).
    Streaming,
    /// The audibility point of peak control, less `CLUB_MARGIN_LU` (D131), at the
    /// true-peak ceiling of a lossy codec (`adjust.club.v1`).
    Club,
    /// The programme loudness and the true-peak ceiling of radio (EBU R 128
    /// s3), with the fold of the Mono preset (`adjust.radio.v1`).
    Radio,
    /// The fold: each band takes back the side/mid of the mix, and its mid
    /// takes the power of the side that the widening adds
    /// (`adjust.mono.v1`).
    Mono,
}

impl Preset {
    pub const ALL: [Preset; 5] = [
        Preset::MonoSafe,
        Preset::Streaming,
        Preset::Club,
        Preset::Radio,
        Preset::Mono,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Preset::MonoSafe => "Mono-safe",
            Preset::Streaming => "Streaming",
            Preset::Club => "Club",
            Preset::Radio => "Radio",
            Preset::Mono => "Mono",
        }
    }

    /// The request keyword of the preset, as the cloud action and
    /// `master adjust --preset` take it.
    pub fn keyword(self) -> &'static str {
        match self {
            Preset::MonoSafe => "mono-safe",
            Preset::Streaming => "streaming",
            Preset::Club => "club",
            Preset::Radio => "radio",
            Preset::Mono => "mono",
        }
    }

    /// The rule of the preset.
    pub fn rule(self) -> &'static str {
        match self {
            Preset::MonoSafe => crate::width::MONO_SAFE_RULE,
            Preset::Streaming => "adjust.streaming.v1",
            Preset::Club => "adjust.club.v1",
            Preset::Radio => "adjust.radio.v1",
            Preset::Mono => "adjust.mono.v1",
        }
    }

    pub fn parse(keyword: &str) -> Option<Self> {
        Preset::ALL.into_iter().find(|p| p.keyword() == keyword)
    }
}

/// Loudness at which the streaming services play music, in LUFS. Spotify
/// plays music at −14 LUFS by default and turns a louder master down
/// (Spotify for Artists, "Loudness normalization"). AES TD1008 plays the
/// loudest track of an album at −14 LUFS.
pub const STREAMING_LUFS: f64 = -14.0;
/// True-peak ceiling at the input of a lossy codec, in dBTP. Spotify asks
/// for a master at −14 LUFS under −1 dBTP, Apple for at least 1 dB of
/// headroom (Apple Digital Masters), and AES TD1008 for −1 dBTP or lower.
pub const CODEC_CEILING_DBTP: f64 = -1.0;
/// Programme loudness of radio production and play-out, in LUFS (EBU R 128
/// s3).
pub const RADIO_LUFS: f64 = -23.0;
/// Largest true peak of a radio programme, in dBTP (EBU R 128, EBU R 128
/// s3).
pub const RADIO_CEILING_DBTP: f64 = -1.0;
/// Margin of the Club level under the audibility point of peak control at
/// the ceiling of the preset, in LU: the step of D131.
pub const CLUB_MARGIN_LU: f64 = 0.5;

/// What a medium preset sets on a version.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Medium {
    pub target_lufs: f64,
    pub ceiling_dbtp: f64,
    /// The side that the widening of the version adds goes to the mid
    /// (`folded`).
    pub fold: bool,
    /// The candidates take shares of the level change. The level of Club is
    /// an amount of the engine; a published level is a delivery condition.
    pub level_shares: bool,
}

/// The Club level on `parent`: the audibility point of the set (D131) at
/// the ceiling of the preset, less `CLUB_MARGIN_LU`, within the range of a
/// request and never under the level of the parent. Peak control works
/// against the ceiling, so a lower ceiling lowers the point by the same
/// amount. Without a point, the level of the parent.
pub fn club_lufs(parent: &Spec, facts: &Facts) -> f64 {
    let ceiling = parent.ceiling_dbtp.min(CODEC_CEILING_DBTP);
    facts.audibility_lufs.map_or(parent.target_lufs, |point| {
        (point + ceiling - parent.ceiling_dbtp - CLUB_MARGIN_LU)
            .min(LOUDNESS_RANGE_LUFS.1)
            .max(parent.target_lufs)
    })
}

/// The settings of a medium preset on `parent`, or None for a preset that
/// answers a condition of the mix.
pub fn medium(parent: &Spec, preset: Preset, facts: &Facts) -> Option<Medium> {
    match preset {
        Preset::MonoSafe => None,
        Preset::Streaming => Some(Medium {
            target_lufs: parent.target_lufs.min(STREAMING_LUFS),
            ceiling_dbtp: parent.ceiling_dbtp.min(CODEC_CEILING_DBTP),
            fold: false,
            level_shares: false,
        }),
        Preset::Club => Some(Medium {
            target_lufs: club_lufs(parent, facts),
            ceiling_dbtp: parent.ceiling_dbtp.min(CODEC_CEILING_DBTP),
            fold: false,
            level_shares: true,
        }),
        Preset::Radio => Some(Medium {
            target_lufs: RADIO_LUFS,
            ceiling_dbtp: parent.ceiling_dbtp.min(RADIO_CEILING_DBTP),
            fold: true,
            level_shares: false,
        }),
        Preset::Mono => Some(Medium {
            target_lufs: parent.target_lufs,
            ceiling_dbtp: parent.ceiling_dbtp,
            fold: true,
            level_shares: false,
        }),
    }
}

/// The mid and side power of each stereo analysis band of `spectrum` after
/// `tone` and the side gain `side`, with its decorrelated mid and its mid
/// gain.
fn band_parts(
    spectrum: &BandSpectrum,
    tone: &Tone,
    side: Option<&crate::width::SideGain>,
) -> [[f64; 2]; 6] {
    let rate = spectrum.sample_rate;
    let edges = &crate::stereo::BAND_EDGES_HZ;
    let response = |filters: &[EqBand], hz: f64| {
        10f64.powf(
            filters
                .iter()
                .map(|b| b.filter(rate).response_db(hz, rate))
                .sum::<f64>()
                / 10.0,
        )
    };
    let mut parts = [[0.0f64; 2]; 6];
    for (i, &hz) in spectrum.centers_hz.iter().enumerate() {
        let Some(b) = (0..6).find(|b| hz >= edges[*b] && hz < edges[*b + 1]) else {
            continue;
        };
        let stereo = response(&tone.stereo, hz);
        let mid = stereo * response(&tone.mid, hz) * spectrum.mid_power[i];
        let (widening, decorrelated, gain) = side.map_or((1.0, 0.0, 1.0), |g| {
            (
                10f64.powf(g.response_db(hz) / 10.0),
                g.decorrelated_amplitude(hz, rate).powi(2),
                g.mid_power(hz),
            )
        });
        parts[b][0] += mid * gain;
        parts[b][1] += stereo * response(&tone.side, hz) * spectrum.side_power[i] * widening
            + decorrelated * mid;
    }
    parts
}

/// The fold of `share` of the widening of `parent`: each band that the
/// widening raises takes back `share` of its side gain and of its
/// decorrelated mid, and its mid takes the side power that they add, so the
/// power of the band stays (`adjust.mono.v1`). A mono speaker plays the mid
/// alone, and a side gain leaves the mid as it is: the fold gives the mid of
/// the band the power that the stereo version holds in its added side. A
/// band whose side gain is 0 dB or a cut stays. None when the parent has no
/// widening.
pub fn folded(
    parent: &Spec,
    spectrum: &BandSpectrum,
    share: f64,
) -> Option<crate::width::SideGain> {
    let side = parent.side_gain?;
    let widened: [bool; 6] =
        std::array::from_fn(|b| side.bands_db[b] > 0.0 || side.decorrelated_db[b].is_some());
    if !widened.iter().any(|w| *w) {
        return None;
    }
    let keep = (1.0 - share).max(0.0);
    let mut fold = side;
    for (b, _) in widened.iter().enumerate().filter(|(_, w)| **w) {
        if fold.bands_db[b] > 0.0 {
            fold.bands_db[b] *= keep;
        }
        fold.decorrelated_db[b] = fold.decorrelated_db[b]
            .filter(|_| keep > 0.0)
            .map(|d| d + 10.0 * keep.log10())
            .filter(|d| *d >= -30.0);
    }
    let tone = parent.mean_tone();
    let before = band_parts(spectrum, &tone, Some(&side));
    // The transitions of the gains join the bands, so the mid gains are
    // solved together, in a few passes.
    for _ in 0..8 {
        let after = band_parts(spectrum, &tone, Some(&fold));
        for b in 0..6 {
            if !widened[b] || after[b][0] <= 0.0 {
                continue;
            }
            let wanted = before[b][0] + before[b][1] - after[b][1];
            fold.mid_db[b] = (fold.mid_db[b] + 10.0 * (wanted.max(1e-30) / after[b][0]).log10())
                .clamp(0.0, crate::width::WIDEN_MAXIMUM_DB);
        }
    }
    Some(fold)
}

/// The reason of the candidate at `share` of a preset.
fn preset_share_reason(share: f64) -> String {
    format!("{}% of the preset", (share * 100.0).round())
}

/// The spec of `medium` on `parent` at `share` of its fold, or of its level
/// change with `Medium::level_shares`. The ceiling, and a published level,
/// stay in full: they are delivery conditions of the medium. The low-end
/// hold of the parent is removed: the render solves it again.
fn medium_spec(parent: &Spec, medium: Medium, spectrum: &BandSpectrum, share: f64) -> Result<Spec> {
    let mut spec = parent.clone();
    spec.target_lufs = if medium.level_shares {
        parent.target_lufs + share * (medium.target_lufs - parent.target_lufs)
    } else {
        medium.target_lufs
    };
    spec.ceiling_dbtp = medium.ceiling_dbtp;
    if medium.fold {
        if let Some(fold) = folded(parent, spectrum, share) {
            fold.validate()?;
            spec.side_gain = Some(fold);
        }
    }
    spec.effective_tone().0.validate(spectrum.sample_rate)?;
    spec.low_end_hold = None;
    Ok(spec)
}

/// The predicted change of `preset` on `parent` in the measures that it
/// sets: the loudness, the ceiling, and the gain of the mono sum of each
/// band at matched loudness. None for a preset that answers a condition.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MediumChange {
    pub loudness_lu: f64,
    pub ceiling_db: f64,
    pub mono_db: [f64; 6],
}

/// True when a change reaches `DISTINCT_DB`, the smallest difference that
/// makes a version distinct.
fn distinct(change: f64) -> bool {
    change.abs() >= crate::selection::DISTINCT_DB - 1e-9
}

impl MediumChange {
    /// True when the change of the preset reaches `DISTINCT_DB` in a measure
    /// that serves its medium: the level or the ceiling for Streaming, the
    /// level for Club, the mono sum of a band for Mono, and any of them for
    /// Radio.
    pub fn serves(&self, preset: Preset) -> bool {
        let mono = self.mono_db.iter().any(|m| distinct(*m));
        match preset {
            Preset::Streaming => distinct(self.loudness_lu) || distinct(self.ceiling_db),
            Preset::Club => distinct(self.loudness_lu),
            Preset::Radio => distinct(self.loudness_lu) || distinct(self.ceiling_db) || mono,
            Preset::Mono => mono,
            Preset::MonoSafe => false,
        }
    }
}

pub fn medium_change(parent: &Spec, preset: Preset, facts: &Facts) -> Option<MediumChange> {
    let medium = medium(parent, preset, facts)?;
    let spectrum = &facts.spectrum;
    let mono_db = match medium.fold.then(|| folded(parent, spectrum, 1.0)).flatten() {
        Some(fold) => mono_change_db(
            spectrum,
            &parent.mean_tone(),
            parent.side_gain.as_ref(),
            Some(&fold),
        ),
        None => [0.0; 6],
    };
    Some(MediumChange {
        loudness_lu: medium.target_lufs - parent.target_lufs,
        ceiling_db: medium.ceiling_dbtp - parent.ceiling_dbtp,
        mono_db,
    })
}

/// True when `preset` changes `parent` by an amount that the duplicate
/// screen of a request counts (`Sound::is_duplicate_of`) in a measure that
/// serves its medium (`MediumChange::serves`), or, for the Mono-safe preset,
/// where its condition holds. A result that the screen calls a duplicate is
/// not delivered, so `range` offers what can deliver.
pub fn offers(parent: &Spec, preset: Preset, facts: &Facts) -> bool {
    match preset {
        Preset::MonoSafe => {
            MONO_SAFE_OFFERED
                && parent.mono_safe.is_empty()
                && facts.mono_safe.as_ref().is_some_and(|b| !b.is_empty())
        }
        _ => medium_change(parent, preset, facts).is_some_and(|c| c.serves(preset)),
    }
}

/// A request: a preset or the steps on the delivered version `parent`,
/// rendered as the new version `id`.
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub parent: String,
    pub id: String,
    #[serde(default)]
    pub steps: Steps,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preset: Option<Preset>,
}

impl Request {
    pub fn validate(&self) -> Result<()> {
        if !crate::distributed::valid_version_id(&self.id) {
            return Err("Invalid version ID".into());
        }
        match self.preset {
            Some(_) => {
                if self.steps != Steps::default() {
                    return Err("A request carries a preset or steps, not both".into());
                }
                Ok(())
            }
            None => self.steps.validate(),
        }
    }

    /// What the request asks, for the basis of its plan.
    fn basis(&self, parent_id: &str) -> Result<String> {
        Ok(match self.preset {
            Some(p) => format!("Preset {} on {parent_id}", p.keyword()),
            None => format!(
                "Steps on {parent_id}: {}",
                serde_json::to_string(&self.steps)?
            ),
        })
    }
}

/// The candidates of a preset on `parent`: the full amount, then three
/// quarters and half of it. A medium preset keeps its ceiling and a
/// published level in each candidate, so Streaming has one candidate.
pub fn preset_candidates(parent: &Spec, preset: Preset, facts: &Facts) -> Result<Vec<Candidate>> {
    let gates = Gates {
        tonal_image: true,
        low_end: true,
        workload_as_warning: true,
        ..Gates::default()
    };
    if let Some(medium) = medium(parent, preset, facts) {
        // The Club level comes from the point of `essential`. A version
        // with more low end reaches its own point sooner, so each Club
        // candidate stays under the point of its own prefix.
        let gates = Gates {
            under_own_point: preset == Preset::Club,
            ..gates
        };
        if !offers(parent, preset, facts) {
            return Err(format!(
                "The {} preset changes this version by less than {} dB",
                preset.name(),
                crate::selection::DISTINCT_DB
            )
            .into());
        }
        let shares: &[f64] = if medium.level_shares
            || (medium.fold && folded(parent, &facts.spectrum, 1.0).is_some())
        {
            &SHARES
        } else {
            &SHARES[..1]
        };
        let mut candidates = Vec::new();
        let mut first_error = None;
        for &share in shares {
            match medium_spec(parent, medium, &facts.spectrum, share) {
                Ok(spec) => candidates.push(Candidate {
                    reason: preset_share_reason(share),
                    spec,
                    gates: gates.clone(),
                }),
                Err(e) => {
                    first_error.get_or_insert(e);
                }
            }
        }
        return match (candidates.is_empty(), first_error) {
            (true, Some(e)) => Err(e),
            _ => Ok(candidates),
        };
    }
    let bells = match preset {
        Preset::MonoSafe => {
            if !MONO_SAFE_OFFERED {
                return Err("The Mono-safe preset is withdrawn".into());
            }
            if !parent.mono_safe.is_empty() {
                return Err("This version already holds the parts that cancel".into());
            }
            facts
                .mono_safe
                .as_ref()
                .filter(|b| !b.is_empty())
                .ok_or("This set has no Mono-safe preset: no instrument group is anti-phase")?
        }
        _ => return Err("A medium preset has its own candidates".into()),
    };
    let mut candidates = Vec::new();
    for share in SHARES {
        let scaled: Vec<crate::treatment::EnvelopedBand> = bells
            .iter()
            .map(|b| {
                let mut b = b.clone();
                b.band.gain_db *= share;
                b
            })
            .collect();
        let mut spec = parent.clone();
        spec.mono_safe = scaled;
        if spec
            .effective_tone()
            .0
            .validate(facts.spectrum.sample_rate)
            .is_err()
        {
            continue;
        }
        // The final render solves the hold again.
        spec.low_end_hold = None;
        candidates.push(Candidate {
            reason: share_reason(share),
            spec,
            gates: gates.clone(),
        });
    }
    if candidates.is_empty() {
        return Err("The preset does not fit the filters of this version".into());
    }
    Ok(candidates)
}

/// The candidates and the worker plan of `request` on `parent`, a version
/// record with a `spec`.
pub fn plan(base: &Base, parent: &Value, request: &Request) -> Result<Value> {
    request.validate()?;
    let facts = &base.facts;
    let spec = crate::excerpt::request_spec(parent)?;
    let candidates = match request.preset {
        Some(preset) => preset_candidates(&spec, preset, facts)?,
        None => {
            let available = Availability::of(&spec, render_report(parent), facts.bandwidth_hz);
            candidates(&spec, &request.steps, &facts.spectrum, available)?
        }
    };
    worker_plan(base, &request.id, &request.parent, &candidates, request)
}

/// For each control and step of `parent`, a version record, the first share
/// of the step that the builder offers (`SHARES`), or null when it offers
/// none: the range that the panel shows before a request. A gate of the
/// render can still reduce a share. `presets` gives, for each preset, whether
/// the version offers it (`offers`).
pub fn range(parent: &Value, facts: &Facts) -> Result<Value> {
    let spec = crate::excerpt::request_spec(parent)?;
    let available = Availability::of(&spec, render_report(parent), facts.bandwidth_hz);
    let mut out = serde_json::Map::new();
    for control in [
        "bass",
        "presence",
        "air",
        "sibilance",
        "loudness",
        "width",
        "compression",
        "ceiling",
    ] {
        let mut steps = serde_json::Map::new();
        let span: Vec<i32> = if control == "ceiling" {
            (-MAXIMUM_CEILING_STEPS..0).collect()
        } else {
            (-MAXIMUM_STEPS..=MAXIMUM_STEPS)
                .filter(|s| *s != 0)
                .collect()
        };
        for step in span {
            let mut request = serde_json::to_value(Steps::default())?;
            request[control] = step.into();
            let request: Steps = serde_json::from_value(request)?;
            let share = SHARES
                .into_iter()
                .find(|share| spec_at(&spec, &request, *share, &facts.spectrum, available).is_ok());
            steps.insert(step.to_string(), share.map_or(Value::Null, Value::from));
        }
        out.insert(control.into(), Value::Object(steps));
    }
    let mut presets = serde_json::Map::new();
    for preset in Preset::ALL {
        presets.insert(
            preset.keyword().into(),
            Value::Bool(offers(&spec, preset, facts)),
        );
    }
    out.insert("presets".into(), Value::Object(presets));
    Ok(Value::Object(out))
}

/// Another version of the set, for the duplicate screen and the name of an
/// adjusted version.
#[derive(Clone, Debug)]
pub struct Other {
    pub id: String,
    pub label: String,
    pub sound: Sound,
}

/// What the duplicate screen of a request compares: the signature of the
/// set, the measures of the controls, the mean compressor reduction, the
/// mean de-esser cut and the ceiling.
#[derive(Clone, Debug)]
pub struct Sound {
    pub signature: Signature,
    pub measures: Delivered,
    pub compressor_db: f64,
    pub deesser_cut_db: f64,
    pub ceiling_dbtp: f64,
}

impl Sound {
    /// The sound of a version record with its stored facts.
    pub fn of(record: &Value) -> Option<Self> {
        let facts = VersionFacts::from_record(record)?;
        let render = render_report(record);
        Some(Self {
            signature: facts.signature,
            measures: facts.measures,
            compressor_db: render["compressor"]["mean_reduction_db"]
                .as_f64()
                .unwrap_or(0.0),
            deesser_cut_db: render["deesser"]["cut_mean_db"].as_f64().unwrap_or(0.0),
            ceiling_dbtp: record["spec"]["ceiling_dbtp"].as_f64()?,
        })
    }

    /// True when this sound is the same as `other`: the signatures are
    /// duplicates (`Signature::is_duplicate_of`) at a loudness within
    /// `DISTINCT_DB`, and no control measure, the compressor reduction, the
    /// de-esser cut or the ceiling differs by `DISTINCT_DB` or more. A
    /// request asks for one of these, so each one makes a version distinct,
    /// unlike in the versions of a set (D135).
    pub fn is_duplicate_of(&self, other: &Sound) -> bool {
        let distinct = crate::selection::DISTINCT_DB;
        let (a, b) = (&self.measures, &other.measures);
        (self.signature.lufs - other.signature.lufs).abs() < distinct
            && self.signature.is_duplicate_of(&other.signature)
            && [
                a.bass_db - b.bass_db,
                a.presence_db - b.presence_db,
                a.air_db - b.air_db,
                a.width_db - b.width_db,
                self.compressor_db - other.compressor_db,
                self.deesser_cut_db - other.deesser_cut_db,
                self.ceiling_dbtp - other.ceiling_dbtp,
            ]
            .iter()
            .all(|d| d.abs() < distinct)
    }
}

impl Other {
    /// The version of a record with its stored facts.
    pub fn of(record: &Value) -> Option<Self> {
        Some(Self {
            id: record["id"].as_str()?.into(),
            label: record["label"].as_str()?.into(),
            sound: Sound::of(record)?,
        })
    }
}

/// A measured change of a control that a name or a description states, in
/// dB or LU.
pub const STATED_CHANGE_DB: f64 = 0.25;

/// The phrase of the largest change: the largest measured change of a
/// requested tone, width, loudness, compression or sibilance control that
/// reaches `STATED_CHANGE_DB`, otherwise the largest requested step. A
/// control that is not requested can move as a side effect, such as the Air
/// measure after a Presence step; the name states what was asked.
fn phrase(change: &Value, steps: &Steps) -> &'static str {
    let measured = [
        ("bass_db", steps.bass, "more bass", "less bass"),
        (
            "presence_db",
            steps.presence,
            "more presence",
            "less presence",
        ),
        ("air_db", steps.air, "more air", "less air"),
        ("width_db", steps.width, "wider", "narrower"),
        ("loudness_lu", steps.loudness, "louder", "quieter"),
        (
            "compression_db",
            steps.compression,
            "more compressed",
            "less compressed",
        ),
        (
            "sibilance_db",
            steps.sibilance,
            "brighter S sounds",
            "softer S sounds",
        ),
    ];
    let largest = measured
        .iter()
        .filter(|(_, step, ..)| *step != 0)
        .filter_map(|(key, _, up, down)| {
            let c = change[*key].as_f64()?;
            (c.abs() >= STATED_CHANGE_DB).then_some((c, *up, *down))
        })
        .max_by(|a, b| a.0.abs().total_cmp(&b.0.abs()));
    if let Some((c, up, down)) = largest {
        return if c > 0.0 { up } else { down };
    }
    let requested = [
        (steps.bass, "more bass", "less bass"),
        (steps.presence, "more presence", "less presence"),
        (steps.air, "more air", "less air"),
        (steps.width, "wider", "narrower"),
        (steps.loudness, "louder", "quieter"),
        (steps.compression, "more compressed", "less compressed"),
        (steps.sibilance, "brighter S sounds", "softer S sounds"),
        (steps.ceiling, "", "lower ceiling"),
    ];
    requested
        .iter()
        .filter(|(s, ..)| *s != 0)
        .max_by_key(|(s, ..)| s.abs())
        .map_or("adjusted", |(s, up, down)| if *s > 0 { up } else { down })
}

/// The name of an adjusted version: the label of its parent and its largest
/// change, with a number when another version has the name.
pub fn name(parent_label: &str, change: &Value, steps: &Steps, others: &[Other]) -> String {
    unique_name(
        format!("{parent_label} · {}", phrase(change, steps)),
        others,
    )
}

/// `base`, with a number when another version has the name.
fn unique_name(base: String, others: &[Other]) -> String {
    let taken = |n: &str| others.iter().any(|o| o.label == n);
    if !taken(&base) {
        return base;
    }
    (2..)
        .map(|i| format!("{base} {i}"))
        .find(|n| !taken(n))
        .expect("a free name")
}

/// The description of an adjusted version: each measured change that
/// reaches `STATED_CHANGE_DB`, and the ceiling.
pub fn description(parent_label: &str, change: &Value, ceiling_dbtp: f64, steps: &Steps) -> String {
    let mut parts = Vec::new();
    for (key, name, unit) in [
        ("bass_db", "bass", "dB"),
        ("presence_db", "presence", "dB"),
        ("air_db", "air", "dB"),
        ("width_db", "width", "dB"),
        ("loudness_lu", "loudness", "LU"),
        ("compression_db", "compression", "dB"),
        ("sibilance_db", "sibilance", "dB"),
    ] {
        if let Some(c) = change[key].as_f64().filter(|c| c.abs() >= STATED_CHANGE_DB) {
            parts.push(format!(
                "{name} {}{:.1} {unit}",
                if c > 0.0 { "+" } else { "−" },
                c.abs()
            ));
        }
    }
    if steps.ceiling != 0 {
        parts.push(format!("ceiling −{:.1} dBTP", -ceiling_dbtp));
    }
    if parts.is_empty() {
        return format!("Made from {parent_label}. It measures the same as {parent_label}.");
    }
    format!("Made from {parent_label}: {}.", parts.join(", "))
}

/// A frequency as "500 Hz" or "6 kHz".
fn hz_text(hz: f64) -> String {
    if hz >= 1000.0 {
        format!("{} kHz", (hz / 1000.0 * 10.0).round() / 10.0)
    } else {
        format!("{} Hz", hz.round())
    }
}

/// The description of a Mono-safe result, from its measures (`finalize`).
fn preset_description(parent_label: &str, mono_safe: &Value) -> String {
    let get = |k: &str| mono_safe[k].as_f64().unwrap_or(f64::NAN);
    format!(
        "Made from {parent_label}: narrower from {} to {} where the channels cancel, {:.0}% of the song. Mono loss {:.1} → {:.1} dB.",
        hz_text(get("low_hz")),
        hz_text(get("high_hz")),
        100.0 * get("song_share"),
        get("mono_loss_mix_db").abs(),
        get("mono_loss_db").abs()
    )
}

/// The change of the mono sum of each stereo analysis band at matched
/// loudness when the side gain `before` becomes `after`, predicted on
/// `spectrum` after `tone`, in dB. A mono speaker plays the mid, and a
/// service matches the loudness of the stereo file.
fn mono_change_db(
    spectrum: &BandSpectrum,
    tone: &Tone,
    before: Option<&crate::width::SideGain>,
    after: Option<&crate::width::SideGain>,
) -> [f64; 6] {
    let (a, b) = (
        band_parts(spectrum, tone, before),
        band_parts(spectrum, tone, after),
    );
    let loudness = predicted_loudness_db(spectrum, tone, after)
        - predicted_loudness_db(spectrum, tone, before);
    std::array::from_fn(|i| {
        if a[i][0] > 0.0 && b[i][0] > 0.0 {
            10.0 * (b[i][0] / a[i][0]).log10() - loudness
        } else {
            0.0
        }
    })
}

/// The mono fold-down loss of each stereo analysis band of `audio`, in dB.
fn mono_fold_down_db(audio: &[Stereo], rate: u32) -> Result<[Option<f64>; 6]> {
    let image = crate::stereo::analyze(audio, rate)?;
    Ok(std::array::from_fn(|b| {
        image.whole.bands[b].mono_fold_down_db
    }))
}

/// The report of a medium preset result: the delivered level, ceiling and
/// true peak against those of the parent, the change of each band at matched
/// loudness, the change of the work of peak control, and, with a fold, the
/// predicted change of the mono sum of each band and the measured mono
/// fold-down loss of the mix and of the result.
#[allow(clippy::too_many_arguments)]
fn medium_report(
    preset: Preset,
    input: &[Stereo],
    rendered: &[Stereo],
    facts: &Facts,
    parent: &Value,
    parent_facts: &VersionFacts,
    version: &crate::distributed::OutputVersion,
    adjusted: &VersionFacts,
) -> Result<Value> {
    let rate = facts.spectrum.sample_rate;
    let parent_spec = crate::distributed::decode_spec(&parent["spec"])?;
    let spec = crate::distributed::decode_spec(&version.spec)?;
    let parent_render = render_report(parent);
    let (lufs, parent_lufs) = (version.lufs, parent_facts.signature.lufs);
    let peak = version.verification.peak_estimate_dbtp;
    let parent_peak = parent["delivery"]["peak_estimate_dbtp"]
        .as_f64()
        .or_else(|| parent["verification"]["peak_estimate_dbtp"].as_f64())
        .or_else(|| parent_render["peak_estimate_dbtp"].as_f64());
    let (sa, sp) = (&adjusted.signature, &parent_facts.signature);
    let band_db: Vec<Value> = (0..6)
        .map(|b| {
            (sa.band_active[b]
                && sp.band_active[b]
                && sa.band_power[b] > 1e-20
                && sp.band_power[b] > 1e-20)
                .then(|| {
                    10.0 * (sa.band_power[b] / sp.band_power[b]).log10() - (lufs - parent_lufs)
                })
                .map_or(Value::Null, Value::from)
        })
        .collect();
    let change = |key: &[&str]| -> Option<f64> {
        let at = |render: &Value| key.iter().fold(render, |v, k| &v[*k]).as_f64();
        at(&version.render)
            .zip(at(parent_render))
            .map(|(a, b)| a - b)
    };
    let fold = spec.side_gain.filter(|g| g.has_mid_gain());
    let (mono_gain_db, mix_fold_down, result_fold_down) = match fold {
        Some(fold) => (
            Some(mono_change_db(
                &facts.spectrum,
                &parent_spec.mean_tone(),
                parent_spec.side_gain.as_ref(),
                Some(&fold),
            )),
            Some(mono_fold_down_db(input, rate)?),
            Some(mono_fold_down_db(rendered, rate)?),
        ),
        None => (None, None, None),
    };
    Ok(json!({
        "preset": preset,
        "rule": preset.rule(),
        "lufs": lufs,
        "ceiling_dbtp": spec.ceiling_dbtp,
        "peak_dbtp": peak,
        "parent_lufs": parent_lufs,
        "parent_ceiling_dbtp": parent_spec.ceiling_dbtp,
        "parent_peak_dbtp": parent_peak,
        "loudness_lu": lufs - parent_lufs,
        "ceiling_db": spec.ceiling_dbtp - parent_spec.ceiling_dbtp,
        "peak_db": parent_peak.map(|p| peak - p),
        // The peak-to-loudness ratio of EBU Tech 3401.
        "plr_db": parent_peak.map(|p| (peak - lufs) - (p - parent_lufs)),
        "band_db": band_db,
        "width_db": adjusted.measures.width_db - parent_facts.measures.width_db,
        "limiter_mean_reduction_db": change(&["limiter", "mean_reduction_db"]),
        "limiter_maximum_reduction_db": change(&["limiter", "maximum_reduction_db"]),
        "clipped_frames_percent": change(&["clipping", "affected_frames_percent"]),
        "mono_gain_db": mono_gain_db,
        "mono_fold_down_db": mix_fold_down.zip(result_fold_down)
            .map(|(mix, result)| json!({"mix": mix, "result": result})),
    }))
}

/// A level as "−14.0".
fn level_text(value: f64) -> String {
    format!("{}{:.1}", if value < 0.0 { "−" } else { "" }, value.abs())
}

/// The description of a medium preset result, from its report
/// (`medium_report`).
fn medium_description(preset: Preset, parent_label: &str, report: &Value) -> String {
    let get = |k: &str| report[k].as_f64().unwrap_or(f64::NAN);
    let mut text = format!(
        "Made from {parent_label}: {} LUFS with peaks at {} dBTP",
        level_text(get("lufs")),
        level_text(get("peak_dbtp"))
    );
    let change = get("loudness_lu");
    if change.abs() >= STATED_CHANGE_DB {
        text += &format!(
            ", {:.1} LU {} than {parent_label}",
            change.abs(),
            if change > 0.0 { "louder" } else { "quieter" }
        );
    }
    text += ".";
    if preset == Preset::Streaming && change <= -STATED_CHANGE_DB {
        text += &format!(
            " A service that plays music at {} LUFS plays it as loud as {parent_label}.",
            level_text(STREAMING_LUFS)
        );
    }
    // The bands whose mono sum the fold raises by a stated amount.
    if let Some(gains) = report["mono_gain_db"].as_array() {
        let gains: Vec<f64> = gains.iter().map(|g| g.as_f64().unwrap_or(0.0)).collect();
        let edges = &crate::stereo::BAND_EDGES_HZ;
        let raised: Vec<usize> = (0..gains.len())
            .filter(|&b| gains[b] >= STATED_CHANGE_DB)
            .collect();
        if let (Some(&low), Some(&high)) = (raised.first(), raised.last()) {
            let largest = raised.iter().map(|&b| gains[b]).fold(0.0, f64::max);
            text += &format!(
                " On a single speaker, {} to {} is up to {largest:.1} dB louder, with the width of your mix.",
                hz_text(edges[low]),
                hz_text(edges[high + 1])
            );
        } else {
            text += " It keeps the width of your mix.";
        }
    }
    text
}

/// Finish the render of a request as one version: the warnings, the
/// measured change against the parent, the duplicate screen against the
/// mix and the other versions, the name and the description. `store` holds
/// the files of `family`. `parent` is the record of the parent version and
/// `parent_facts` its stored or measured facts. `input` is the source at its
/// render rate, the rate of `facts`.
#[allow(clippy::too_many_arguments)]
pub fn finalize(
    input: &[Stereo],
    facts: &Facts,
    parent: &Value,
    parent_facts: &VersionFacts,
    others: &[Other],
    request: &Request,
    family: &crate::distributed::FamilyResult,
    store: &mut impl crate::distributed::Store,
) -> Result<Value> {
    let (parent_id, id, steps) = (request.parent.as_str(), request.id.as_str(), &request.steps);
    let rate = facts.spectrum.sample_rate;
    let parent_label = parent["label"].as_str().unwrap_or(parent_id);
    let Some(version) = family.versions.first() else {
        return Ok(json!({
            "schema": RESULT_SCHEMA,
            "rule": RULE,
            "parent": parent_id,
            "id": id,
            "steps": steps,
            "preset": request.preset,
            "delivered": false,
            "reason": "no-share-passed",
            "family": family,
        }));
    };
    let share = version
        .candidates
        .iter()
        .find(|c| c.index == version.chosen_candidate)
        .and_then(|c| share_of(&c.reason))
        .ok_or("The chosen candidate has no share of the steps")?;
    let delivered_steps = json!({
        "bass": steps.bass as f64 * share,
        "presence": steps.presence as f64 * share,
        "air": steps.air as f64 * share,
        "sibilance": steps.sibilance as f64 * share,
        "loudness": steps.loudness as f64 * share,
        "width": steps.width as f64 * share,
        "compression": steps.compression as f64 * share,
        "ceiling": steps.ceiling,
    });
    let parent_render = render_report(parent);
    let compressor_mean_db = |render: &Value| render["compressor"]["mean_reduction_db"].as_f64();
    // The audibility point of the delivered prefix, for a louder target: a
    // Loudness step up, or a preset that raises the level.
    let louder = version.spec["target_lufs"]
        .as_f64()
        .zip(parent["spec"]["target_lufs"].as_f64())
        .is_some_and(|(a, b)| a > b);
    let audibility_lufs = if steps.loudness > 0 || louder {
        let mut spec = crate::distributed::decode_spec(&version.spec)?;
        if let Some(gains) = version.render["restoration"]["gains_db"].as_array() {
            for (g, value) in spec.restoration_db.iter_mut().zip(gains) {
                *g = value.as_f64().ok_or("Invalid restoring gain")?;
            }
        }
        let prefix = crate::treatment_render::Renderer::new(input, rate).prefix(&spec)?;
        crate::audibility::point(
            &prefix,
            version.drive_db,
            spec.clipper_db,
            spec.ceiling_dbtp,
            spec.limiter,
            spec.true_peak,
            Some(&crate::analysis::analyze(input, rate)),
            rate,
        )?
        .map(|p| p.lufs)
    } else {
        None
    };
    let mean = version.render["limiter"]["mean_reduction_db"]
        .as_f64()
        .ok_or("Missing limiter reduction")?;
    let warnings = warnings(
        version.lufs,
        audibility_lufs,
        mean,
        parent_render["limiter"]["mean_reduction_db"]
            .as_f64()
            .unwrap_or(mean),
    );
    let rendered = store.read(&version.raw_wav_file, rate)?;
    let adjusted = VersionFacts::of(&rendered, version.lufs, mean, rate)?;
    // The Mono-safe preset states the mono fold-down loss of its bands, in
    // the mix and in the result.
    let mono_safe = match (request.preset, facts.mono_safe.as_ref()) {
        (Some(Preset::MonoSafe), Some(bells)) if !bells.is_empty() => {
            let edges = &crate::stereo::BAND_EDGES_HZ;
            let bands: Vec<usize> = bells
                .iter()
                .filter_map(|b| (0..6).find(|i| b.band.hz >= edges[*i] && b.band.hz < edges[i + 1]))
                .collect();
            let loss = |audio: &[Stereo]| -> Result<f64> {
                let image = crate::stereo::analyze(audio, rate)?;
                let values: Vec<f64> = bands
                    .iter()
                    .filter_map(|&b| image.whole.bands[b].mono_fold_down_db)
                    .collect();
                Ok(values.iter().sum::<f64>() / values.len().max(1) as f64)
            };
            let first = &bells[0].points;
            let wet =
                first.iter().filter(|p| p.wet > 0.5).count() as f64 / first.len().max(1) as f64;
            Some(json!({
                "bands": bands,
                "low_hz": bands.iter().map(|&b| edges[b]).fold(f64::INFINITY, f64::min),
                "high_hz": bands.iter().map(|&b| edges[b + 1]).fold(0.0, f64::max),
                "song_share": wet,
                "mono_loss_mix_db": loss(input)?,
                "mono_loss_db": loss(&rendered)?,
                "cut_db": bells.iter().map(|b| b.band.gain_db * share).fold(0.0, f64::min),
            }))
        }
        _ => None,
    };
    // A medium preset states its level, its ceiling and its changes against
    // the parent.
    let medium = match request.preset {
        Some(preset) if preset != Preset::MonoSafe => Some(medium_report(
            preset,
            input,
            &rendered,
            facts,
            parent,
            parent_facts,
            version,
            &adjusted,
        )?),
        _ => None,
    };
    drop(rendered);
    let parent_lufs = parent_facts.signature.lufs;
    let (p, a) = (&parent_facts.measures, &adjusted.measures);
    let measured = json!({
        "bass_db": a.bass_db - p.bass_db,
        "presence_db": a.presence_db - p.presence_db,
        "air_db": a.air_db - p.air_db,
        "width_db": a.width_db - p.width_db,
        "loudness_lu": version.lufs - parent_lufs,
        "compression_db": compressor_mean_db(&version.render)
            .zip(compressor_mean_db(parent_render))
            .map(|(a, b)| a - b),
        // Less de-esser cut is more sibilance.
        "sibilance_db": version.render["deesser"]["cut_mean_db"].as_f64()
            .zip(parent_render["deesser"]["cut_mean_db"].as_f64())
            .map(|(a, b)| b - a),
    });
    let compression = json!({
        "mean_reduction_db": compressor_mean_db(&version.render),
        "parent_mean_reduction_db": compressor_mean_db(parent_render),
    });
    let ceiling_dbtp = version.spec["ceiling_dbtp"]
        .as_f64()
        .ok_or("Missing ceiling")?;
    let sound = Sound {
        signature: adjusted.signature.clone(),
        measures: adjusted.measures,
        compressor_db: compressor_mean_db(&version.render).unwrap_or(0.0),
        deesser_cut_db: version.render["deesser"]["cut_mean_db"]
            .as_f64()
            .unwrap_or(0.0),
        ceiling_dbtp,
    };
    // The fold changes the mono sum of a band, which the sound of a version
    // record does not hold, so a fold that moves it by `DISTINCT_DB` or more
    // makes the result distinct from its parent.
    let folds_mono = medium
        .as_ref()
        .and_then(|m| m["mono_gain_db"].as_array())
        .is_some_and(|gains| gains.iter().any(|g| g.as_f64().is_some_and(distinct)));
    let duplicate_of = if facts.same_as_mix(&adjusted.signature) {
        Some((
            crate::set::ORIGINAL_ID.to_string(),
            crate::set::MIX_NAME.to_string(),
        ))
    } else {
        others
            .iter()
            .filter(|o| !(folds_mono && o.id == parent_id))
            .find(|o| sound.is_duplicate_of(&o.sound))
            .map(|o| (o.id.clone(), o.label.clone()))
    };
    let (label, text) = match (request.preset, &mono_safe, &medium) {
        (Some(preset), Some(m), _) => (
            unique_name(format!("{parent_label} · {}", preset.name()), others),
            preset_description(parent_label, m),
        ),
        (Some(preset), _, Some(m)) => (
            unique_name(format!("{parent_label} · {}", preset.name()), others),
            medium_description(preset, parent_label, m),
        ),
        _ => (
            name(parent_label, &measured, steps, others),
            description(parent_label, &measured, ceiling_dbtp, steps),
        ),
    };
    let record = json!({
        "schema": "mastering.web-set-version.1",
        "id": id,
        "label": label,
        "description": text,
        "rule": RULE,
        "basis": request.basis(parent_id)?,
        "source_pcm_sha256": family.source_pcm_sha256,
        "output_sha256": version.wav_sha256,
        "delivery": version.verification,
        "spec": version.spec,
        "report": {"render": version.render},
        "waveform": version.waveform,
        "wav_file": version.wav_file,
        "signature": adjusted.signature,
        "measures": adjusted.measures,
        "adjusted": {
            "parent": parent_id,
            "parent_label": parent_label,
            "steps": steps,
            "preset": request.preset,
            "mono_safe": mono_safe,
            "medium": medium,
            "delivered_share": share,
            "delivered_steps": delivered_steps,
            "measured_change": measured,
            "warnings": warnings,
        },
    });
    Ok(json!({
        "schema": RESULT_SCHEMA,
        "rule": RULE,
        "parent": parent_id,
        "id": id,
        "steps": steps,
        "preset": request.preset,
        "delivered": duplicate_of.is_none(),
        "reason": if duplicate_of.is_some() { json!("duplicate") } else { Value::Null },
        "duplicate_of": duplicate_of.as_ref().map(|(id, label)| json!({"id": id, "label": label})),
        "delivered_share": share,
        "delivered_steps": delivered_steps,
        "warnings": warnings,
        "audibility_lufs": audibility_lufs,
        "measured_change": measured,
        "medium": medium,
        "compression": compression,
        "version": record,
        "candidates": version.candidates,
        "seconds": family.seconds,
    }))
}

/// Power of the held bands (`low_end::BANDS_HZ`) and K-weighted power of
/// the stored spectrum after the tone of `spec` with its side gain.
fn held_power(spectrum: &BandSpectrum, spec: &Spec) -> ([f64; 2], f64) {
    let tone = spec.mean_tone();
    let side = spec.side_gain.as_ref();
    let (mut held, mut weighted) = ([0.0f64; 2], 0.0);
    for (i, &hz) in spectrum.centers_hz.iter().enumerate() {
        let power = shaped_power(spectrum, &tone, side, i);
        weighted += dsp::k_weighting_power(hz, spectrum.sample_rate) * power;
        for (b, (low, high)) in crate::low_end::BANDS_HZ.iter().enumerate() {
            if (*low..*high).contains(&hz) {
                held[b] += power;
            }
        }
    }
    (held, weighted)
}

/// The stored spectrum after the tone of `spec` with its side gain, at the
/// mean wet shares of its enveloped filters.
fn shaped_spectrum(spectrum: &BandSpectrum, spec: &Spec) -> Spectrum {
    crate::low_end::shaped_spectrum(&spectrum.spectrum(), spec)
}

/// The hold that the final render of `spec` solves, predicted from the
/// sections of the parent `delivered`. The final render solves the hold
/// from the changes without a hold. The preview takes the hold steps of the
/// final render (`low_end::next`) from the parent's changes without a hold
/// (`HoldSections::free_change_db`), and from them plus the predicted change
/// of the held bands from the request, less its intended move, and adds the
/// difference of the two holds to the parent's hold.
fn predicted_hold(
    delivered: &Spec,
    spec: &Spec,
    sections: &HoldSections,
    facts: &Facts,
    frames: usize,
) -> Option<crate::low_end::Hold> {
    let full = facts.spectrum.spectrum();
    let free = |s: &Spec| {
        let mut s = s.clone();
        s.low_end_hold = None;
        s
    };
    let (parent, candidate) = (free(delivered), free(spec));
    let ((held_a, weighted_a), (held_b, weighted_b)) = (
        held_power(&facts.spectrum, &parent),
        held_power(&facts.spectrum, &candidate),
    );
    let intended = |s: &Spec| crate::low_end::intended_db(s, &full);
    let (intended_a, intended_b) = (intended(&parent), intended(&candidate));
    let request: [f64; 2] = std::array::from_fn(|b| {
        10.0 * (held_b[b].max(1e-30) / held_a[b].max(1e-30)).log10()
            - 10.0 * (weighted_b.max(1e-30) / weighted_a.max(1e-30)).log10()
            - (intended_b[b] - intended_a[b])
    });
    let changes: Vec<[Option<f64>; 2]> = sections
        .free_change_db
        .iter()
        .map(|c| std::array::from_fn(|b| c[b].map(|c| c + request[b])))
        .collect();
    let low = crate::low_end::Sections {
        starts: sections.starts.clone(),
        ends: sections.ends.clone(),
        power_db: Vec::new(),
    };
    // The steps from the parent's changes give the parent's hold, as its
    // render did. The preview keeps the parent's wets and adds the
    // difference that the request makes to the steps.
    let step = |spec: &Spec, changes: &[[Option<f64>; 2]]| {
        // The final render plans each step on the spectrum after the tone of
        // the version (`low_end::shaped_spectrum`).
        let shaped = shaped_spectrum(&facts.spectrum, spec);
        let (_, first) = crate::low_end::next(spec, &shaped, &low, changes, None, frames)?;
        if crate::low_end::ITERATIONS < 2 {
            return Some(first);
        }
        // The final render measures the change of the first step and takes a
        // second step from it.
        let measured: Vec<[Option<f64>; 2]> = changes
            .iter()
            .enumerate()
            .map(|(s, c)| std::array::from_fn(|b| c[b].map(|x| x + first.expected_db[s][b])))
            .collect();
        if crate::low_end::passes(&measured) {
            return Some(first);
        }
        match crate::low_end::next(
            spec,
            &shaped,
            &low,
            &measured,
            Some((&first, changes)),
            frames,
        ) {
            Some((_, second)) => Some(second),
            None => Some(first),
        }
    };
    let (before, after) = (
        step(&parent, &sections.free_change_db),
        step(&candidate, &changes),
    );
    let wet_of = |step: &Option<crate::low_end::Step>, s: usize| {
        step.as_ref().map_or([0.0; 2], |step| step.hold_wet[s])
    };
    let bell_wet = |f: usize, s: usize| {
        let hz = crate::low_end::hold_band(f).hz;
        delivered
            .low_end_hold
            .iter()
            .flat_map(|h| &h.bands)
            .find(|b| b.band.hz == hz)
            .map_or(0.0, |b| b.mean_wet(sections.starts[s], sections.ends[s]))
    };
    let wet: Vec<[f64; 2]> = (0..sections.free_change_db.len())
        .map(|s| {
            let (a, b) = (wet_of(&before, s), wet_of(&after, s));
            std::array::from_fn(|f| (bell_wet(f, s) + b[f] - a[f]).clamp(0.0, 1.0))
        })
        .collect();
    if wet.iter().all(|w| w.iter().all(|x| *x <= 0.0)) {
        return None;
    }
    let factor = after.map_or(vec![1.0; wet.len()], |step| step.correction_factor);
    Some(crate::low_end::hold_of(
        &candidate, &low, &wet, &factor, frames,
    ))
}

/// The preview of a preset on `parent`: the first candidate of the preset,
/// with the hold and the restoring gains of the parent, as `preview`.
pub fn preview_preset(
    parent: &Value,
    preset: Preset,
    facts: &Facts,
) -> Result<(Spec, crate::excerpt::Fixed)> {
    let delivered = crate::excerpt::delivered_spec(parent)?;
    let fixed = crate::excerpt::Fixed::of(parent)?;
    let mut spec = preset_candidates(&delivered, preset, facts)?
        .into_iter()
        .next()
        .ok_or("No share of the preset")?
        .spec;
    spec.restoration_db = delivered.restoration_db;
    spec.low_end_hold = delivered.low_end_hold.clone();
    // The drive follows the level of the preset, less the predicted change
    // of the loudness of the prefix, as in `preview`.
    let loudness =
        |s: &Spec| predicted_loudness_db(&facts.spectrum, &s.mean_tone(), s.side_gain.as_ref());
    let drive_db = fixed.drive_db + (spec.target_lufs - delivered.target_lufs)
        - (loudness(&spec) - loudness(&delivered));
    Ok((
        spec,
        crate::excerpt::Fixed {
            drive_db,
            trim_db: fixed.trim_db,
        },
    ))
}

/// The spec and the fixed state of the preview of `steps` on `parent`, a
/// delivered version record.
/// The preview keeps the restoring gains and the true-peak trim of the
/// parent. It predicts the two things that the final render solves again
/// from the stored spectrum:
///
/// - The low-end hold: the hold steps of the final render (`predicted_hold`) from
///   the sections that the parent stores, or the hold of the parent.
/// - The drive: the drive of the parent with the change of the target, less
///   the predicted change of the loudness of the prefix: the tone with the
///   hold, the width, and the change of the compressor reduction.
pub fn preview(
    parent: &Value,
    steps: &Steps,
    facts: &Facts,
) -> Result<(Spec, crate::excerpt::Fixed)> {
    let delivered = crate::excerpt::delivered_spec(parent)?;
    let fixed = crate::excerpt::Fixed::of(parent)?;
    if *steps == Steps::default() {
        return Ok((delivered, fixed));
    }
    let spectrum = &facts.spectrum;
    let available = Availability::of(&delivered, render_report(parent), facts.bandwidth_hz);
    // The first share whose tone is valid, as the candidates of the final
    // render.
    let mut first_error = None;
    let mut spec = None;
    for share in SHARES {
        match spec_at(&delivered, steps, share, spectrum, available) {
            Ok(s) => {
                spec = Some(s);
                break;
            }
            Err(e) => {
                first_error.get_or_insert(e);
            }
        }
    }
    let mut spec = match (spec, first_error) {
        (Some(spec), _) => spec,
        (None, Some(e)) => return Err(e),
        (None, None) => return Err("No share of the request".into()),
    };
    spec.restoration_db = delivered.restoration_db;
    spec.low_end_hold = match HoldSections::of_record(parent) {
        Some(sections) => {
            let frames = parent["delivery"]["frames"]
                .as_u64()
                .or_else(|| parent["verification"]["frames"].as_u64())
                .ok_or("Missing frame count")? as usize;
            predicted_hold(&delivered, &spec, &sections, facts, frames)
        }
        None => delivered.low_end_hold.clone(),
    };
    let loudness = |s: &Spec| predicted_loudness_db(spectrum, &s.mean_tone(), s.side_gain.as_ref());
    // The compressor reduction: 1 - 1/ratio of each dB that the threshold
    // moves down, and a release that cannot remove more than the mean
    // reduction of the parent. The section make-up gives the reduction back
    // (`set.compression.v3`).
    let compression_db = match (&delivered.compressor, &spec.compressor) {
        (Some(_), Some(after)) if after.section_makeup => 0.0,
        (Some(before), Some(after)) => {
            let moved = (before.threshold_dbfs - after.threshold_dbfs) * (1.0 - 1.0 / after.ratio);
            let mean = render_report(parent)["compressor"]["mean_reduction_db"]
                .as_f64()
                .unwrap_or(0.0)
                .max(1e-6);
            if moved >= 0.0 {
                moved
            } else {
                -mean * (1.0 - (moved / mean).exp())
            }
        }
        _ => 0.0,
    };
    // Peak control takes the overs with the clipper, so the output loudness
    // follows the prefix loudness and the drive dB for dB.
    let prefix_db = loudness(&spec) - loudness(&delivered) - compression_db;
    let drive_db = fixed.drive_db + (spec.target_lufs - delivered.target_lufs) - prefix_db;
    Ok((
        spec,
        crate::excerpt::Fixed {
            drive_db,
            trim_db: fixed.trim_db,
        },
    ))
}

pub const RESULT_SCHEMA: &str = "mastering.adjust-result.2";

/// The versions of a record that the duplicate screen compares with, other
/// than `except`.
pub fn others(versions: &Value, except: &str) -> Vec<Other> {
    versions["versions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|r| r["id"] != except)
        .filter_map(Other::of)
        .collect()
}

/// Render a request as version `id` into `output_dir`, from the base of the
/// set and the record of its delivered versions: a set manifest, or a
/// family result whose versions carry no stored facts. The facts of a
/// version without them come from its WAV beside the versions record.
/// `input` is the source at the render rate of the set.
pub fn render(
    input: &[Stereo],
    base: &Base,
    versions: &Value,
    versions_dir: &std::path::Path,
    request: &Request,
    output_dir: &std::path::Path,
) -> Result<Value> {
    let parent = parent_record(versions, &request.parent)?;
    let stored = |record: &Value| -> Result<VersionFacts> {
        if let Some(f) = VersionFacts::from_record(record) {
            return Ok(f);
        }
        let file = record["raw_wav_file"]
            .as_str()
            .or_else(|| record["wav_file"].as_str())
            .ok_or("A version without facts or a file")?;
        let audio = crate::audio_io::read_source(&versions_dir.join(file))?;
        if audio.sample_rate != base.sample_rate() {
            return Err("A version differs from the set in sample rate".into());
        }
        let render = render_report(record);
        VersionFacts::of(
            &audio.audio,
            render["lufs"].as_f64().ok_or("Missing loudness")?,
            render["limiter"]["mean_reduction_db"]
                .as_f64()
                .ok_or("Missing limiter reduction")?,
            audio.sample_rate,
        )
    };
    let parent_facts = stored(parent)?;
    let mut others = Vec::new();
    for record in versions["versions"].as_array().into_iter().flatten() {
        if record["id"] == request.id.as_str() {
            continue;
        }
        let mut record = record.clone();
        if VersionFacts::from_record(&record).is_none() {
            let f = stored(&record)?;
            record["signature"] = serde_json::to_value(&f.signature)?;
            record["measures"] = serde_json::to_value(f.measures)?;
        }
        if record["label"].as_str().is_none_or(str::is_empty) {
            record["label"] = record["id"].clone();
        }
        others.extend(Other::of(&record));
    }
    let mut store = crate::distributed::Directory(output_dir);
    let (result, plan) = render_in(
        input,
        base,
        parent,
        &parent_facts,
        &others,
        request,
        &mut store,
    )?;
    std::fs::write(
        output_dir.join("adjust.plan.json"),
        serde_json::to_vec_pretty(&plan)?,
    )?;
    Ok(result)
}

/// Render `request` on `parent` as one version into `store`, and finish it
/// (`finalize`). Returns the result and the worker plan of the request.
pub fn render_in(
    input: &[Stereo],
    base: &Base,
    parent: &Value,
    parent_facts: &VersionFacts,
    others: &[Other],
    request: &Request,
    store: &mut impl crate::distributed::Store,
) -> Result<(Value, Value)> {
    request.validate()?;
    // A request outside the range of the builder is a result, so the app
    // can say so in its own words.
    let plan = match plan(base, parent, request) {
        Ok(plan) => plan,
        Err(error) => {
            return Ok((
                json!({
                    "schema": RESULT_SCHEMA,
                    "rule": RULE,
                    "parent": request.parent,
                    "id": request.id,
                    "steps": request.steps,
                    "preset": request.preset,
                    "delivered": false,
                    "reason": "outside-range",
                    "detail": error.to_string(),
                }),
                Value::Null,
            ))
        }
    };
    let plan_json = serde_json::to_vec(&plan)?;
    let family = crate::distributed::render_family_in(
        input,
        base.sample_rate(),
        &plan_json,
        &request.id,
        store,
        |_, _| Ok(()),
    )?;
    let result = finalize(
        input,
        &base.facts,
        parent,
        parent_facts,
        others,
        request,
        &family,
        store,
    )?;
    Ok((result, plan))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::treatment_render::Spec;

    /// A pink spectrum with side power 10 dB below the mid.
    fn bins() -> Spectrum {
        let centers_hz: Vec<f64> = (0..4097).map(|i| i as f64 * 48000.0 / 8192.0).collect();
        let mid_power: Vec<f64> = centers_hz.iter().map(|hz| 1.0 / hz.max(1.0)).collect();
        let side_power = mid_power.iter().map(|p| 0.1 * p).collect();
        Spectrum {
            sample_rate: 48000,
            band_power: mid_power.clone(),
            centers_hz,
            mid_power,
            side_power,
            mel_power: Vec::new(),
            analyzed_frames: 1,
        }
    }

    fn spectrum() -> BandSpectrum {
        BandSpectrum::of(&bins())
    }

    fn parent() -> Spec {
        let plan: Value = serde_json::from_str(include_str!("../tests/data/adjust-parent.json"))
            .expect("parent spec");
        crate::distributed::decode_spec(&plan).expect("decoded")
    }

    /// Keyed frames whose excess spreads evenly from 0 to 10 dB.
    const EXCESS: [f64; crate::deesser::EXCESS_QUANTILES] = {
        let mut q = [0.0; crate::deesser::EXCESS_QUANTILES];
        let mut i = 0;
        while i < q.len() {
            q[i] = i as f64 * 0.5;
            i += 1;
        }
        q
    };

    /// The keyed frames of `EXCESS`, each with a vocal share of 1: the
    /// reference excess, at 90%, is 9 dB.
    const SIBILANCE: Sibilance = Sibilance {
        excess_db: EXCESS,
        vocal_shares: [1.0; crate::deesser::EXCESS_QUANTILES - 1],
        reference_excess_db: EXCESS[crate::deesser::THRESHOLD_QUANTILE],
    };

    /// The de-esser stats of a render of the parent with the keyed frames of
    /// `SIBILANCE`.
    fn deesser_stats() -> Value {
        json!({"keyed_frames": 40, "excess_quantiles_db": EXCESS,
            "vocal_share_bins": SIBILANCE.vocal_shares,
            "reference_excess_db": SIBILANCE.reference_excess_db,
            "maximum_cut_db": crate::deesser::MAXIMUM_CUT_DB})
    }

    /// A plan of `set.deesser.v4` with the largest cut `maximum_cut_db`.
    fn deesser_plan(maximum_cut_db: f64) -> crate::deesser::Plan {
        crate::deesser::Plan {
            rule: "set.deesser.v4".into(),
            keyed: vec![crate::deesser::Keyed {
                frame: 1,
                vocal_share: 1.0,
                other_power: 0.0,
            }],
            other_band_power: 0.0,
            maximum_cut_db,
            threshold_db: 0.0,
            reference_excess_db: Some(SIBILANCE.reference_excess_db),
        }
    }

    const ALL: Availability = Availability {
        air: true,
        sibilance: Some(SIBILANCE),
        compression: true,
    };

    #[test]
    fn steps_are_bounded_and_not_empty() {
        assert!(Steps::default().validate().is_err());
        assert!(Steps {
            bass: 5,
            ..Steps::default()
        }
        .validate()
        .is_err());
        assert!(Steps {
            bass: -4,
            loudness: 4,
            ..Steps::default()
        }
        .validate()
        .is_ok());
    }

    #[test]
    fn each_tone_step_moves_its_prediction() {
        let (s, p) = (spectrum(), parent());
        let before = predicted(&s, &p.tone, p.side_gain.as_ref());
        for (steps, measure) in [
            (
                Steps {
                    bass: 2,
                    ..Steps::default()
                },
                (|m: &Measures| m.bass_db) as fn(&Measures) -> f64,
            ),
            (
                Steps {
                    presence: -3,
                    ..Steps::default()
                },
                |m| m.presence_db,
            ),
            (
                Steps {
                    air: 4,
                    ..Steps::default()
                },
                |m| m.air_db,
            ),
        ] {
            let spec = spec_at(&p, &steps, 1.0, &s, ALL).unwrap();
            let after = predicted(&s, &spec.tone, spec.side_gain.as_ref());
            let wanted = (steps.bass + steps.presence + steps.air) as f64 * STEP_DB;
            assert!(
                (measure(&after) - measure(&before) - wanted).abs() < 0.01,
                "{steps:?}: {} against {wanted}",
                measure(&after) - measure(&before)
            );
        }
    }

    #[test]
    fn a_step_moves_the_existing_filter() {
        let (s, p) = (spectrum(), parent());
        let spec = spec_at(
            &p,
            &Steps {
                air: 1,
                ..Steps::default()
            },
            1.0,
            &s,
            ALL,
        )
        .unwrap();
        let shelves = |t: &Tone| t.stereo.iter().filter(|b| is_air(b)).count();
        assert_eq!(shelves(&p.tone), 1);
        assert_eq!(shelves(&spec.tone), 1);
        assert_eq!(spec.tone.stereo.len(), p.tone.stereo.len());
    }

    #[test]
    fn the_candidates_take_shares_and_remove_the_hold() {
        let (s, p) = (spectrum(), parent());
        let steps = Steps {
            sibilance: -2,
            loudness: 2,
            ..Steps::default()
        };
        let c = candidates(&p, &steps, &s, ALL).unwrap();
        assert_eq!(c.len(), 3);
        // A bass step that the hold of the parent takes over the 9 dB limit
        // of a domain leaves out the full share.
        let mut held = p.clone();
        held.low_end_hold = Some(crate::low_end::Hold {
            rule: "set.low-end.v1".into(),
            correction_factors: Vec::new(),
            bands: (0..2)
                .map(|b| crate::treatment::EnvelopedBand {
                    rule: "set.low-end.v1".into(),
                    domain: crate::finishing::Domain::Stereo,
                    band: crate::low_end::hold_band_at(b, 3.5),
                    points: vec![
                        crate::treatment::BlendPoint {
                            sample: 0,
                            wet: 1.0,
                        },
                        crate::treatment::BlendPoint {
                            sample: 1,
                            wet: 1.0,
                        },
                    ],
                })
                .collect(),
        });
        let bass = Steps {
            bass: 4,
            ..Steps::default()
        };
        // Some parent bass shelf leaves room for the half share and not for
        // the full share; each offered share has room for the hold.
        let mut left_out = false;
        for gain in [0.5, 1.0, 1.5, 2.0, 2.5, 3.0] {
            let mut parent = held.clone();
            parent.tone.stereo.push(bass_shelf(gain));
            let Ok(offered) = candidates(&parent, &bass, &s, ALL) else {
                continue;
            };
            for c in &offered {
                let mut check = c.spec.clone();
                check.low_end_hold = held.low_end_hold.clone();
                check.effective_tone().0.validate(48000).unwrap();
            }
            left_out |= offered.len() < SHARES.len();
        }
        assert!(left_out);
        for (candidate, share) in c.iter().zip(SHARES) {
            assert!(candidate.spec.low_end_hold.is_none());
            assert!(candidate.gates.low_end && candidate.gates.tonal_image);
            assert!((candidate.spec.target_lufs - p.target_lufs - share).abs() < 1e-9);
            // Sibilance -2 is 1 dB more mean cut at the full share.
            let plan = candidate.spec.deesser.as_ref().unwrap();
            let before = mean_cut(&SIBILANCE, p.deesser.as_ref().unwrap(), 0.0);
            let after = mean_cut(&SIBILANCE, plan, 0.0);
            assert!((after - before - share).abs() < 1e-6, "{after} {before}");
            assert!(
                (plan.threshold_db + plan.maximum_cut_db - crate::deesser::MAXIMUM_CUT_DB).abs()
                    < 1e-9
            );
        }
    }

    #[test]
    fn a_sibilance_step_cuts_from_a_plan_without_a_cut() {
        // The plan of a version with the de-esser off cuts nothing: a step
        // down cuts from the reference threshold, and a step up has nothing
        // to give back.
        let plan = deesser_plan(0.0);
        assert_eq!(mean_cut(&SIBILANCE, &plan, 0.0), 0.0);
        let shift = sibilance_shift(&SIBILANCE, &plan, -1.0).unwrap();
        assert!(shift < 0.0);
        assert!((mean_cut(&SIBILANCE, &plan, shift) - 1.0).abs() < 1e-6);
        assert!(sibilance_shift(&SIBILANCE, &plan, 0.5).is_none());
    }

    #[test]
    fn a_sibilance_step_moves_the_mean_cut_by_half_a_db() {
        let plan = deesser_plan(crate::deesser::MAXIMUM_CUT_DB);
        // The two bins over the reference of 9 dB cut 0.25 and 0.75 dB: a
        // mean of 0.05 dB.
        let before = mean_cut(&SIBILANCE, &plan, 0.0);
        assert!((before - 0.05).abs() < 1e-12);
        // After steps down to a threshold of -4 dB and a largest cut of
        // 7 dB, the mean cut is 1.25 dB, and a step up has a cut to give
        // back.
        let stepped = crate::deesser::Plan {
            threshold_db: -4.0,
            ..deesser_plan(7.0)
        };
        assert!((mean_cut(&SIBILANCE, &stepped, 0.0) - 1.25).abs() < 1e-12);
        let half = Sibilance {
            vocal_shares: [0.5; crate::deesser::EXCESS_QUANTILES - 1],
            ..SIBILANCE
        };
        for sibilance in [SIBILANCE, half] {
            for (plan, changes) in [(&plan, &[-1.0, -0.5][..]), (&stepped, &[-0.5, 0.5][..])] {
                let before = mean_cut(&sibilance, plan, 0.0);
                for change in changes {
                    let shift = sibilance_shift(&sibilance, plan, *change).unwrap();
                    assert!((before - mean_cut(&sibilance, plan, shift) - change).abs() < 1e-6);
                }
            }
        }
        // Half the vocal share in each bin gives less cut.
        assert!(mean_cut(&half, &plan, 0.0) < before);
    }

    #[test]
    fn a_sibilance_step_up_takes_the_cut_down_to_none() {
        // A step up on the plan of a set with the switch on is larger than
        // its mean cut of 0.05 dB: it takes the largest cut to 0 dB.
        let plan = deesser_plan(crate::deesser::MAXIMUM_CUT_DB);
        let shift = sibilance_shift(&SIBILANCE, &plan, 0.5).unwrap();
        assert_eq!(shift, plan.maximum_cut_db);
        assert_eq!(mean_cut(&SIBILANCE, &plan, shift), 0.0);
        // After steps down, a step up lowers the mean cut by 0.5 dB.
        let stepped = crate::deesser::Plan {
            threshold_db: -4.0,
            ..deesser_plan(7.0)
        };
        let before = mean_cut(&SIBILANCE, &stepped, 0.0);
        let shift = sibilance_shift(&SIBILANCE, &stepped, 0.5).unwrap();
        assert!(shift > 0.0);
        assert!((mean_cut(&SIBILANCE, &stepped, shift) - (before - 0.5)).abs() < 1e-6);
        // With a threshold of -4 dB and a largest cut of 7 dB, the shift of
        // 7 dB is over the 6 dB of a step up and still takes the cut to 0 dB.
        let low = Sibilance {
            vocal_shares: [0.2; crate::deesser::EXCESS_QUANTILES - 1],
            ..SIBILANCE
        };
        let remaining = mean_cut(&low, &stepped, 0.0);
        assert!(remaining > 0.0 && remaining < 2.0, "{remaining}");
        let shift = sibilance_shift(&low, &stepped, 2.0).unwrap();
        assert_eq!(shift, 7.0);
        assert_eq!(mean_cut(&low, &stepped, shift), 0.0);
        // The threshold stays in its range: a plan whose threshold and
        // largest cut sum to more than 6 dB keeps the remainder.
        let wide = crate::deesser::Plan {
            threshold_db: -1.0,
            ..deesser_plan(8.0)
        };
        assert_eq!(sibilance_shift(&low, &wide, 20.0), Some(7.0));
        let (s, p) = (spectrum(), parent());
        let up = |parent: &Spec, sibilance| {
            spec_at(
                parent,
                &Steps {
                    sibilance,
                    ..Steps::default()
                },
                1.0,
                &s,
                ALL,
            )
        };
        // The parent cuts with the switch on: one step up takes the cut to
        // 0 dB.
        let one = up(&p, 1).unwrap().deesser.unwrap();
        assert_eq!((one.threshold_db, one.maximum_cut_db), (3.0, 0.0));
        // After steps down, one step up lowers the mean cut by 0.5 dB, and
        // four steps take it to 0 dB.
        let mut down = p.clone();
        let parent_plan = down.deesser.as_mut().unwrap();
        (parent_plan.threshold_db, parent_plan.maximum_cut_db) = (-4.0, 7.0);
        let one = up(&down, 1).unwrap().deesser.unwrap();
        assert!(one.threshold_db > -4.0 && one.maximum_cut_db < 7.0);
        assert!((mean_cut(&SIBILANCE, &one, 0.0) - (before - 0.5)).abs() < 1e-6);
        let all = up(&down, 4).unwrap().deesser.unwrap();
        assert_eq!(all.maximum_cut_db, 0.0);
        assert_eq!(mean_cut(&SIBILANCE, &all, 0.0), 0.0);
        // A step up on a plan that cuts nothing is outside the range.
        let mut off = p.clone();
        off.deesser.as_mut().unwrap().maximum_cut_db = 0.0;
        let refused = spec_at(
            &off,
            &Steps {
                sibilance: 1,
                ..Steps::default()
            },
            1.0,
            &s,
            ALL,
        );
        assert!(refused.is_err());
    }

    #[test]
    fn the_range_gives_the_first_share_of_each_step() {
        let plan: Value = serde_json::from_str(include_str!("../tests/data/adjust-parent.json"))
            .expect("parent spec");
        let parent = json!({"spec": plan, "render": {"deesser": deesser_stats()}});
        let facts = |bandwidth_hz| Facts {
            rule: FACTS_RULE.into(),
            spectrum: spectrum(),
            audibility_lufs: None,
            bandwidth_hz,
            mix: signature(-14.0, 1.0),
            mono_safe: None,
        };
        let r = range(&parent, &facts(24_000.0)).unwrap();
        assert_eq!(r["presets"]["mono-safe"], false);
        // The parent plays at −10.9 LUFS with a ceiling of −0.5 dBTP:
        // Streaming and Radio move its level and its ceiling. Without an
        // audibility point, Club keeps the level. The widening of 1.5 dB on
        // a side 10 dB under the mid moves the mono sum by less than 0.5 dB.
        for (preset, offered) in [
            ("streaming", true),
            ("club", false),
            ("radio", true),
            ("mono", false),
        ] {
            assert_eq!(r["presets"][preset], offered, "{preset}");
        }
        for control in ["bass", "presence", "air", "sibilance", "loudness", "width"] {
            assert_eq!(r[control]["1"], 1.0, "{control}");
            assert_eq!(r[control]["-1"], 1.0, "{control}");
        }
        assert_eq!(r["ceiling"]["-3"], 1.0);
        assert!(r["ceiling"]["1"].is_null());
        let narrow = range(&parent, &facts(12_000.0)).unwrap();
        assert!(narrow["air"]["1"].is_null());
    }

    /// The keyed frames of a master with a wide distribution, rounded to
    /// 0.01: the median excess is 10.67 dB under the 90% quantile.
    const WIDE: Sibilance = Sibilance {
        excess_db: [
            -28.03, -16.77, -13.38, -10.13, -7.85, -6.38, -4.66, -3.71, -2.80, -1.55, -0.54, -0.13,
            0.83, 2.05, 3.01, 4.19, 5.71, 7.50, 10.13, 13.29, 21.82,
        ],
        vocal_shares: [
            0.76, 0.80, 0.89, 0.89, 0.84, 0.88, 0.88, 0.86, 0.92, 0.92, 0.94, 0.96, 0.95, 0.97,
            0.97, 0.98, 0.99, 0.98, 0.99, 1.00,
        ],
        reference_excess_db: 10.13,
    };

    #[test]
    fn four_sibilance_steps_down_reach_a_wide_distribution() {
        // With the switch off and on, four steps down at the full share add
        // 2 dB of mean cut. The shift is more than 6 dB, and the largest cut
        // stays at or under `deesser::CUT_LIMIT_DB`.
        for maximum in [0.0, crate::deesser::MAXIMUM_CUT_DB] {
            let plan = crate::deesser::Plan {
                reference_excess_db: Some(WIDE.reference_excess_db),
                ..deesser_plan(maximum)
            };
            let before = mean_cut(&WIDE, &plan, 0.0);
            for steps in 1..=MAXIMUM_STEPS {
                let change = -STEP_DB * steps as f64;
                let shift = sibilance_shift(&WIDE, &plan, change).unwrap();
                assert!((mean_cut(&WIDE, &plan, shift) - before + change).abs() < 1e-6);
                assert!(plan.threshold_db + shift >= crate::deesser::THRESHOLD_RANGE_DB.0);
                assert!(shifted_cut_db(&plan, shift) <= crate::deesser::CUT_LIMIT_DB);
                if steps == MAXIMUM_STEPS {
                    assert!(shift < -6.0, "{maximum} {shift}");
                }
            }
        }
        // The range of a version of the set offers each step down at the
        // full share. With the switch off, it offers no step up.
        let spec: Value = serde_json::from_str(include_str!("../tests/data/adjust-parent.json"))
            .expect("parent spec");
        let facts = Facts {
            rule: FACTS_RULE.into(),
            spectrum: spectrum(),
            audibility_lufs: None,
            bandwidth_hz: 24_000.0,
            mix: signature(-14.0, 1.0),
            mono_safe: None,
        };
        for maximum in [0.0, crate::deesser::MAXIMUM_CUT_DB] {
            let stats = json!({"keyed_frames": 738, "excess_quantiles_db": WIDE.excess_db,
                "vocal_share_bins": WIDE.vocal_shares,
                "reference_excess_db": WIDE.reference_excess_db,
                "maximum_cut_db": maximum});
            let parent = json!({"spec": spec, "render": {"deesser": stats}});
            let r = range(&parent, &facts).unwrap();
            for step in 1..=MAXIMUM_STEPS {
                assert_eq!(
                    r["sibilance"][(-step).to_string()],
                    1.0,
                    "{maximum} -{step}"
                );
            }
            assert_eq!(r["sibilance"]["1"].is_null(), maximum == 0.0, "{maximum}");
        }
    }

    #[test]
    fn a_reason_gives_its_share() {
        for share in SHARES {
            assert_eq!(share_of(&share_reason(share)), Some(share));
            assert_eq!(share_of(&preset_share_reason(share)), Some(share));
        }
        assert_eq!(share_of("strongest"), None);
    }

    fn preset_facts(audibility_lufs: Option<f64>) -> Facts {
        Facts {
            rule: FACTS_RULE.into(),
            spectrum: spectrum(),
            audibility_lufs,
            bandwidth_hz: 24_000.0,
            mix: signature(-14.0, 1.0),
            mono_safe: None,
        }
    }

    /// The parent with a widening of `gain_db` from 120 to 500 Hz alone.
    fn widened_parent(gain_db: f64) -> Spec {
        let mut p = parent();
        p.side_gain = Some(crate::width::SideGain::gains([
            0.0, gain_db, 0.0, 0.0, 0.0, 0.0,
        ]));
        p
    }

    #[test]
    fn each_preset_keyword_parses_back() {
        for preset in Preset::ALL {
            assert_eq!(Preset::parse(preset.keyword()), Some(preset));
            assert_eq!(
                serde_json::to_value(preset).unwrap(),
                json!(preset.keyword())
            );
            assert!(!preset.rule().is_empty() && !preset.name().is_empty());
        }
        assert_eq!(Preset::parse("vinyl"), None);
    }

    #[test]
    fn streaming_takes_the_level_and_the_ceiling_of_the_services() {
        let mut p = parent();
        p.low_end_hold = Some(crate::low_end::Hold {
            rule: "set.low-end.v3".into(),
            correction_factors: Vec::new(),
            bands: Vec::new(),
        });
        let facts = preset_facts(Some(-7.0));
        let c = preset_candidates(&p, Preset::Streaming, &facts).unwrap();
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].spec.target_lufs, STREAMING_LUFS);
        assert_eq!(c[0].spec.ceiling_dbtp, CODEC_CEILING_DBTP);
        assert!(c[0].spec.low_end_hold.is_none());
        assert_eq!(c[0].spec.side_gain, p.side_gain);
        assert!(c[0].gates.tonal_image && c[0].gates.low_end);
        // A quieter version keeps its level and takes the ceiling.
        let mut quiet = parent();
        quiet.target_lufs = -16.0;
        let c = preset_candidates(&quiet, Preset::Streaming, &facts).unwrap();
        assert_eq!(c[0].spec.target_lufs, -16.0);
        assert_eq!(c[0].spec.ceiling_dbtp, CODEC_CEILING_DBTP);
        // At the level and the ceiling of the services, nothing changes.
        quiet.ceiling_dbtp = CODEC_CEILING_DBTP;
        assert!(!offers(&quiet, Preset::Streaming, &facts));
        assert!(preset_candidates(&quiet, Preset::Streaming, &facts).is_err());
    }

    #[test]
    fn club_takes_the_audibility_point_at_its_ceiling() {
        let p = parent();
        // The point of −7.0 LUFS at −0.5 dBTP is −7.5 LUFS at −1 dBTP, and
        // the level is `CLUB_MARGIN_LU`, the step of D131, under it.
        let c = preset_candidates(&p, Preset::Club, &preset_facts(Some(-7.0))).unwrap();
        assert!((c[0].spec.target_lufs + 8.0).abs() < 1e-9);
        // The later candidates take three quarters and half of the rise, at
        // the ceiling of the preset.
        assert_eq!(c.len(), SHARES.len());
        for (candidate, share) in c.iter().zip(SHARES) {
            let rise = -8.0 - p.target_lufs;
            assert!((candidate.spec.target_lufs - (p.target_lufs + share * rise)).abs() < 1e-9);
            assert_eq!(candidate.spec.ceiling_dbtp, CODEC_CEILING_DBTP);
        }
        // Within the range of a request, and never under the parent.
        assert_eq!(
            club_lufs(&p, &preset_facts(Some(-5.0))),
            LOUDNESS_RANGE_LUFS.1
        );
        assert_eq!(club_lufs(&p, &preset_facts(Some(-11.0))), p.target_lufs);
        assert_eq!(club_lufs(&p, &preset_facts(None)), p.target_lufs);
        // Without a point, the ceiling alone does not serve a club.
        assert!(!offers(&p, Preset::Club, &preset_facts(None)));
        assert!(offers(&p, Preset::Club, &preset_facts(Some(-7.0))));
        // Each Club candidate stays under the point of its own prefix; the
        // other presets keep their published level or the parent level.
        assert!(c.iter().all(|c| c.gates.under_own_point));
        let streaming =
            preset_candidates(&p, Preset::Streaming, &preset_facts(Some(-7.0))).unwrap();
        assert!(streaming.iter().all(|c| !c.gates.under_own_point));
    }

    #[test]
    fn the_fold_keeps_the_band_power_and_returns_the_side_of_the_mix() {
        let s = spectrum();
        let p = widened_parent(5.0);
        let fold = folded(&p, &s, 1.0).unwrap();
        assert_eq!(fold.bands_db[1], 0.0);
        assert!(fold.mid_db[1] > 0.5, "{:?}", fold.mid_db);
        assert_eq!(fold.mid_db[0], 0.0);
        let tone = p.mean_tone();
        let (before, after, mix) = (
            band_parts(&s, &tone, p.side_gain.as_ref()),
            band_parts(&s, &tone, Some(&fold)),
            band_parts(&s, &tone, None),
        );
        let db = |x: f64| 10.0 * x.log10();
        // The band keeps its power, and takes the side/mid of the mix.
        let power = |q: [f64; 2]| q[0] + q[1];
        assert!((db(power(after[1])) - db(power(before[1]))).abs() < 0.05);
        let ratio = |q: [f64; 2]| db(q[1] / q[0]);
        assert!(
            (ratio(after[1]) - (ratio(mix[1]) - fold.mid_db[1])).abs() < 0.2,
            "{} {} {}",
            ratio(after[1]),
            ratio(mix[1]),
            fold.mid_db[1]
        );
        // The mono sum of the band rises by the side power the widening adds.
        let mono = mono_change_db(&s, &tone, p.side_gain.as_ref(), Some(&fold));
        let added = db(before[1][0] + before[1][1] - mix[1][1]) - db(before[1][0]);
        assert!((mono[1] - added).abs() < 0.1, "{mono:?} {added}");
        assert!(mono[3].abs() < 0.1, "{mono:?}");
        // Half the fold keeps half the side gain.
        assert!((folded(&p, &s, 0.5).unwrap().bands_db[1] - 2.5).abs() < 1e-12);
        // A version without a widening has nothing to fold.
        let mut plain = parent();
        plain.side_gain = None;
        assert!(folded(&plain, &s, 1.0).is_none());
        assert!(!offers(&plain, Preset::Mono, &preset_facts(None)));
    }

    #[test]
    fn mono_and_radio_fold_where_the_mono_sum_changes() {
        let facts = preset_facts(None);
        let p = widened_parent(5.0);
        assert!(offers(&p, Preset::Mono, &facts));
        let c = preset_candidates(&p, Preset::Mono, &facts).unwrap();
        assert_eq!(c.len(), SHARES.len());
        assert_eq!(c[0].spec.target_lufs, p.target_lufs);
        assert_eq!(c[0].spec.ceiling_dbtp, p.ceiling_dbtp);
        let gains: Vec<f64> = c
            .iter()
            .map(|c| c.spec.side_gain.unwrap().bands_db[1])
            .collect();
        assert_eq!(gains, vec![0.0, 1.25, 2.5]);
        let radio = preset_candidates(&p, Preset::Radio, &facts).unwrap();
        assert_eq!(radio.len(), SHARES.len());
        for candidate in &radio {
            assert_eq!(candidate.spec.target_lufs, RADIO_LUFS);
            assert_eq!(candidate.spec.ceiling_dbtp, RADIO_CEILING_DBTP);
        }
        assert_eq!(radio[0].spec.side_gain, c[0].spec.side_gain);
        // A widening whose mono change stays under 0.5 dB is not offered by
        // Mono; Radio still moves the level.
        let small = widened_parent(0.5);
        assert!(!offers(&small, Preset::Mono, &facts));
        assert!(offers(&small, Preset::Radio, &facts));
        assert!(preset_candidates(&small, Preset::Mono, &facts).is_err());
    }

    #[test]
    fn a_medium_description_states_the_level_the_peak_and_the_fold() {
        let streaming = json!({"lufs": -14.0, "peak_dbtp": -1.0, "loudness_lu": -3.9});
        assert_eq!(
            medium_description(Preset::Streaming, "Full", &streaming),
            "Made from Full: −14.0 LUFS with peaks at −1.0 dBTP, 3.9 LU quieter than Full. A service that plays music at −14.0 LUFS plays it as loud as Full."
        );
        let mono = json!({"lufs": -10.0, "peak_dbtp": -0.5, "loudness_lu": 0.01,
            "mono_gain_db": [0.0, 0.72, 0.31, 0.1, 0.0, 0.0]});
        assert_eq!(
            medium_description(Preset::Mono, "Full", &mono),
            "Made from Full: −10.0 LUFS with peaks at −0.5 dBTP. On a single speaker, 120 Hz to 2 kHz is up to 0.7 dB louder, with the width of your mix."
        );
        let club = json!({"lufs": -8.2, "peak_dbtp": -1.0, "loudness_lu": 1.8});
        assert_eq!(
            medium_description(Preset::Club, "Warm", &club),
            "Made from Warm: −8.2 LUFS with peaks at −1.0 dBTP, 1.8 LU louder than Warm."
        );
    }

    #[test]
    fn the_preview_of_a_preset_takes_its_level_in_the_drive() {
        let mut plan: Value =
            serde_json::from_str(include_str!("../tests/data/adjust-parent.json"))
                .expect("parent spec");
        let record = |plan: &Value| {
            json!({"spec": plan, "render": {"drive_db": 3.0, "true_peak": {"trim_db": -0.1},
                "deesser": deesser_stats()}})
        };
        let facts = preset_facts(Some(-7.0));
        let target = plan["target_lufs"].as_f64().unwrap();
        let (spec, fixed) = preview_preset(&record(&plan), Preset::Streaming, &facts).unwrap();
        assert_eq!(spec.target_lufs, STREAMING_LUFS);
        // The tone and the width stay, so the drive moves by the level alone.
        assert!((fixed.drive_db - (3.0 + STREAMING_LUFS - target)).abs() < 1e-9);
        assert_eq!(fixed.trim_db, -0.1);
        // The fold keeps the power of each band, so the drive stays within
        // 0.1 dB.
        plan["side_gain"] = json!({"bands_db": [0.0, 5.0, 0.0, 0.0, 0.0, 0.0],
            "decorrelated_db": [null, null, null, null, null, null]});
        let (spec, fixed) = preview_preset(&record(&plan), Preset::Mono, &facts).unwrap();
        assert!(spec.side_gain.unwrap().mid_db[1] > 0.5);
        assert!((fixed.drive_db - 3.0).abs() < 0.1, "{}", fixed.drive_db);
    }

    #[test]
    fn a_missing_band_refuses_its_control() {
        let (s, p) = (spectrum(), parent());
        let steps = Steps {
            air: 1,
            ..Steps::default()
        };
        let no_air = Availability { air: false, ..ALL };
        assert!(candidates(&p, &steps, &s, no_air).is_err());
        let render = json!({"deesser": deesser_stats()});
        assert!(!Availability::of(&p, &render, 12_000.0).air);
        let all = Availability::of(&p, &render, 24_000.0);
        assert!(all.air && all.sibilance.is_some());
        assert!(Availability::of(&p, &json!({}), 24_000.0)
            .sibilance
            .is_none());
    }

    #[test]
    fn loudness_stays_in_its_range() {
        let (s, mut p) = (spectrum(), parent());
        p.target_lufs = -7.0;
        let spec = spec_at(
            &p,
            &Steps {
                loudness: 4,
                ..Steps::default()
            },
            1.0,
            &s,
            ALL,
        )
        .unwrap();
        assert_eq!(spec.target_lufs, LOUDNESS_RANGE_LUFS.1);
    }

    #[test]
    fn width_moves_the_side_from_120_hz_up() {
        let (s, p) = (spectrum(), parent());
        let steps = Steps {
            width: 3,
            ..Steps::default()
        };
        let spec = spec_at(&p, &steps, 1.0, &s, ALL).unwrap();
        let side = spec.side_gain.unwrap();
        let before = p.side_gain.map_or([0.0; 6], |g| g.bands_db);
        assert_eq!(side.bands_db[0], before[0]);
        for (b, a) in before[1..].iter().zip(&side.bands_db[1..]) {
            assert!((a - (b + 1.5).min(crate::width::WIDEN_MAXIMUM_DB)).abs() < 1e-12);
        }
        let change = side_mid_db(&s, Some(&side)) - side_mid_db(&s, p.side_gain.as_ref());
        assert!((change - 1.5).abs() < 0.01, "{change}");
        assert!(widened(None, 0.0).is_none());
        assert_eq!(widened(None, -6.0).unwrap().bands_db[1], -6.0);
    }

    #[test]
    fn compression_and_ceiling_move_their_settings() {
        let (s, p) = (spectrum(), parent());
        let threshold = p.compressor.unwrap().threshold_dbfs;
        let steps = Steps {
            compression: 2,
            ceiling: -1,
            ..Steps::default()
        };
        let c = candidates(&p, &steps, &s, ALL).unwrap();
        for (candidate, share) in c.iter().zip(SHARES) {
            let moved = candidate.spec.compressor.unwrap().threshold_dbfs;
            assert!((threshold - moved - 2.0 * share).abs() < 1e-9);
            assert!(candidate.gates.workload_as_warning);
            assert_eq!(candidate.spec.ceiling_dbtp, p.ceiling_dbtp - 0.5);
        }
        assert!(Steps {
            ceiling: 1,
            ..Steps::default()
        }
        .validate()
        .is_err());
        assert!(Steps {
            ceiling: -4,
            ..Steps::default()
        }
        .validate()
        .is_err());
        let mut plain = p.clone();
        plain.compressor = None;
        assert!(candidates(
            &plain,
            &steps,
            &s,
            Availability::of(&plain, &json!({}), 24_000.0)
        )
        .is_err());
    }

    #[test]
    fn the_bands_keep_the_power_and_the_region_means() {
        let (full, bands) = (bins(), spectrum());
        bands.validate().unwrap();
        let total = |p: &[f64]| p.iter().sum::<f64>();
        // The DC bin is left out.
        let full_mid = total(&full.mid_power) - full.mid_power[0];
        assert!((total(&bands.mid_power) / full_mid - 1.0).abs() < 1e-12);
        assert_eq!(bands.bins.iter().sum::<u32>(), 4096);
        assert!(bands.centers_hz.len() < 260, "{}", bands.centers_hz.len());
        // The side/mid of the pink spectrum is -10 dB in each band.
        assert!((side_mid_db(&bands, None) + 10.0).abs() < 1e-9);
        // Each path predicts from the parsed JSON of the stored bands.
        let json = serde_json::to_string(&bands).unwrap();
        let parsed: BandSpectrum = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.bins, bands.bins);
        for (a, b) in parsed.mid_power.iter().zip(&bands.mid_power) {
            assert!((a / b - 1.0).abs() < 1e-14);
        }
    }

    fn signature(lufs: f64, bass: f64) -> Signature {
        Signature {
            lufs,
            band_power: [bass, 1.0, 1.0, 1.0, 1.0, 1.0],
            band_active: [true, true, true, true, false, false],
            side_mid_db: -10.0,
            mean_reduction_db: 1.0,
        }
    }

    fn sound(lufs: f64, bass: f64) -> Sound {
        Sound {
            signature: signature(lufs, bass),
            measures: Delivered {
                bass_db: -6.0,
                presence_db: -3.0,
                air_db: -8.0,
                width_db: -10.0,
            },
            compressor_db: 1.5,
            deesser_cut_db: 2.0,
            ceiling_dbtp: -0.5,
        }
    }

    fn other(label: &str) -> Other {
        Other {
            id: "master".into(),
            label: label.into(),
            sound: sound(-9.0, 1.0),
        }
    }

    #[test]
    fn each_requested_change_makes_an_adjusted_version_distinct() {
        let o = other("Forward");
        assert!(sound(-9.1, 1.0).is_duplicate_of(&o.sound));
        assert!(!sound(-8.0, 10f64.powf(0.1)).is_duplicate_of(&o.sound));
        assert!(!sound(-9.0, 1.3).is_duplicate_of(&o.sound));
        // The air band is not material in the signature; its measure counts.
        let mut air = sound(-9.0, 1.0);
        air.measures.air_db -= 2.0;
        assert!(!air.is_duplicate_of(&o.sound));
        let mut compressed = sound(-9.0, 1.0);
        compressed.compressor_db += 1.2;
        assert!(!compressed.is_duplicate_of(&o.sound));
        let mut ceiling = sound(-9.0, 1.0);
        ceiling.ceiling_dbtp = -1.0;
        assert!(!ceiling.is_duplicate_of(&o.sound));
    }

    #[test]
    fn the_name_states_the_largest_change() {
        let others = vec![other("Forward · more bass")];
        let change = json!({"bass_db": 0.74, "presence_db": -0.45, "air_db": 0.1,
            "width_db": 0.0, "loudness_lu": 0.98, "compression_db": null});
        let steps = Steps {
            bass: 2,
            loudness: 2,
            ..Steps::default()
        };
        assert_eq!(name("Forward", &change, &steps, &[]), "Forward · louder");
        // The Air measure moves with a Presence step; the name states the
        // requested control.
        let presence = Steps {
            presence: -2,
            ..Steps::default()
        };
        let side_effect = json!({"presence_db": -0.96, "air_db": 1.15});
        assert_eq!(
            name("Full", &side_effect, &presence, &[]),
            "Full · less presence"
        );
        let bass = json!({"bass_db": 0.74, "loudness_lu": 0.1});
        assert_eq!(
            name("Forward", &bass, &steps, &others),
            "Forward · more bass 2"
        );
        let ceiling = Steps {
            ceiling: -2,
            ..Steps::default()
        };
        assert_eq!(
            name("Warm", &json!({}), &ceiling, &[]),
            "Warm · lower ceiling"
        );
        assert_eq!(
            description("Forward", &change, -1.0, &ceiling),
            "Made from Forward: bass +0.7 dB, presence −0.5 dB, loudness +1.0 LU, ceiling −1.0 dBTP."
        );
    }

    #[test]
    fn warnings_name_the_audibility_point_and_the_workload() {
        assert!(warnings(-9.0, Some(-9.5), 0.1, 0.1)[0]["id"] == "loudness-above-audibility-point");
        assert!(warnings(-9.0, Some(-8.5), 0.5, 0.1)[0]["id"] == "limiter-workload");
        assert!(warnings(-9.0, None, 0.1, 0.1).is_empty());
    }
}
