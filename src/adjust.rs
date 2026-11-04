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
/// The smallest level increment that a listener detects in wideband noise
/// whose level is high for 1.5 s and low for 4.5 s: ΔI/I = 0.1, or 0.41 dB
/// (Miller 1947). The duplicate screen of a request counts a change of a
/// requested control from this increment (D279): a render delivers one step
/// within a few hundredths of a dB of `DISTINCT_DB`, on each side of it.
pub const DETECTABLE_DB: f64 = 0.41;
/// Largest number of steps of a control in each direction.
pub const MAXIMUM_STEPS: i32 = 4;
/// Range of the loudness target, in LUFS: the lowest base target of the set
/// and the cap of a Denser target.
pub const LOUDNESS_RANGE_LUFS: (f64, f64) = (crate::set::BASE_TARGET_LUFS.0, -6.5);
/// Corner of the Bass shelf, in Hz.
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
/// Move of the compressor threshold for each Compression step on a parent
/// whose render report has no level distribution of its compressor, in dB.
/// At 2:1 it moves the gain reduction above the knee by one step.
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

    /// True when `share` of the steps changes a requested control by
    /// `DETECTABLE_DB` or more. Each share keeps the ceiling.
    pub fn detectable_at(&self, share: f64) -> bool {
        let moved = [
            self.bass,
            self.presence,
            self.air,
            self.sibilance,
            self.loudness,
            self.width,
            self.compression,
        ]
        .iter()
        .any(|s| s.abs() as f64 * STEP_DB * share >= DETECTABLE_DB);
        moved || self.ceiling.abs() as f64 * STEP_DB >= DETECTABLE_DB
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
    /// The level distribution of the parent's compressor, when its render
    /// report holds it.
    pub compressor: Option<Compression>,
}

/// The level of the compressor of a render over its threshold
/// (`compressor::Stats`).
#[derive(Clone, Copy, Debug)]
pub struct Compression {
    /// `compressor::Stats::over_quantiles_db`.
    pub over_db: [f64; crate::compressor::OVER_QUANTILES],
    /// `compressor::Stats::mean_reduction_db`.
    pub mean_reduction_db: f64,
}

impl Compression {
    /// The compressor level of `stats`, the compressor stats of a render
    /// report.
    fn of(stats: &Value) -> Option<Self> {
        Some(Self {
            over_db: serde_json::from_value(stats["over_quantiles_db"].clone()).ok()?,
            mean_reduction_db: stats["mean_reduction_db"].as_f64()?,
        })
    }
}

/// Range of the shift of the compressor level that `compression_shift`
/// searches, in dB: the span of the bins of `compressor::Stats`.
const COMPRESSION_SHIFT_DB: f64 = 48.0;

/// The shift of the compressor level of `compression` at which the mean
/// static reduction with `settings` is `target_db`. None when no shift
/// within `COMPRESSION_SHIFT_DB` gives it.
fn level_shift(
    compression: &Compression,
    settings: crate::compressor::Settings,
    target_db: f64,
) -> Option<f64> {
    let mean =
        |shift| crate::compressor::mean_reduction_over(&compression.over_db, settings, shift);
    let (mut low, mut high) = (-COMPRESSION_SHIFT_DB, COMPRESSION_SHIFT_DB);
    // The mean reduction rises with the shift.
    if !(mean(low) < target_db && target_db < mean(high)) {
        return None;
    }
    for _ in 0..60 {
        let middle = 0.5 * (low + high);
        if mean(middle) < target_db {
            low = middle;
        } else {
            high = middle;
        }
    }
    Some(0.5 * (low + high))
}

/// The move of the compressor threshold of `settings`, down for a positive
/// value, that changes the mean reduction of the parent by `change_db`
/// (D279). The attack and the release raise the mean reduction over the
/// mean of the static curve. The mean reduction of the parent is the mean
/// static reduction of its level distribution shifted by an offset, and a
/// step moves the threshold from that offset. None when no move gives the
/// change.
fn compression_shift(
    compression: &Compression,
    settings: crate::compressor::Settings,
    change_db: f64,
) -> Option<f64> {
    if change_db == 0.0 {
        return Some(0.0);
    }
    let offset = level_shift(compression, settings, compression.mean_reduction_db)?;
    let moved = level_shift(
        compression,
        settings,
        compression.mean_reduction_db + change_db,
    )?;
    Some(moved - offset)
}

/// The change of the held bands of each section of the parent that the
/// compressor of `spec` makes against the compressor of `delivered`, from
/// `stats`, the compressor stats of the parent (D280). The change at a move
/// of the level is the change of the hold shift of the same direction,
/// times the change of the mean reduction of the move over the change of
/// the hold shift, both from the model of `compression_shift`. None when
/// the parent has no hold shift, or when the request changes another
/// setting of the compressor.
fn compression_hold_db(delivered: &Spec, spec: &Spec, stats: &Value) -> Option<Vec<[f64; 2]>> {
    let (before, after) = (delivered.compressor?, spec.compressor?);
    let shift = before.threshold_dbfs - after.threshold_dbfs;
    let moved = crate::compressor::Settings {
        threshold_dbfs: before.threshold_dbfs,
        ..after
    };
    if shift == 0.0 || serde_json::to_value(moved).ok()? != serde_json::to_value(before).ok()? {
        return None;
    }
    let hold: crate::compressor::HoldShift =
        serde_json::from_value(stats["hold_shift"].clone()).ok()?;
    let compression = Compression::of(stats)?;
    let offset = level_shift(&compression, before, compression.mean_reduction_db)?;
    let change = |x: f64| {
        crate::compressor::mean_reduction_over(&compression.over_db, before, offset + x)
            - crate::compressor::mean_reduction_over(&compression.over_db, before, offset)
    };
    let side = usize::from(shift > 0.0);
    let reference = change([-hold.shift_db, hold.shift_db][side]);
    if reference.abs() < 1e-9 {
        return None;
    }
    let factor = change(shift) / reference;
    Some(
        hold.change_db
            .iter()
            .map(|c| c[side].map(|v| v * factor))
            .collect(),
    )
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
            compressor: Compression::of(&render["compressor"]),
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
    let [mid, side] = shaped_parts(spectrum, tone, side, i);
    mid + side
}

/// The mid and side parts of `shaped_power`.
fn shaped_parts(
    spectrum: &BandSpectrum,
    tone: &Tone,
    side: Option<&crate::width::SideGain>,
    i: usize,
) -> [f64; 2] {
    let gains = tone_gains(tone, side, spectrum.centers_hz[i], spectrum.sample_rate);
    [
        gains[0] * spectrum.mid_power[i],
        gains[1] * spectrum.side_power[i],
    ]
}

/// The power gain of the mid and of the side at `hz` of `tone` with the
/// side gain `side`, at `rate`.
fn tone_gains(tone: &Tone, side: Option<&crate::width::SideGain>, hz: f64, rate: u32) -> [f64; 2] {
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
    [
        stereo * response(&tone.mid) * mid_gain,
        stereo * response(&tone.side) * widening,
    ]
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
    measure_read(&mut crate::scratch::Slice(audio), rate).expect("frames of a slice")
}

/// `measure` of the frames of `audio`, read one batch of hops at a time
/// (D255).
pub(crate) fn measure_read(
    audio: &mut impl crate::scratch::Frames,
    rate: u32,
) -> Result<Delivered> {
    Ok(measures_of(&crate::spectrum_read(audio, rate)?))
}

/// `measure` of the audio whose spectrum (`spectrum_read`) is `spectrum`.
pub(crate) fn measures_of(spectrum: &crate::Spectrum) -> Delivered {
    let spectrum = BandSpectrum::of(spectrum);
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
        Self::of_read(
            &mut crate::scratch::Slice(audio),
            lufs,
            mean_reduction_db,
            rate,
        )
    }

    /// `of` of the frames of `audio`, read in blocks and batches (D257).
    pub(crate) fn of_read(
        audio: &mut impl crate::scratch::Frames,
        lufs: f64,
        mean_reduction_db: f64,
        rate: u32,
    ) -> Result<Self> {
        Ok(Self {
            signature: Signature::new(
                &crate::metrics_read(audio, rate)?,
                &crate::stereo::analyze_read(audio, rate)?,
                lufs,
                mean_reduction_db,
            ),
            measures: measure_read(audio, rate)?,
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
    /// The side share of the power of each held band in each section of
    /// the version (D282). Empty in a record of an earlier engine.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub side_share: Vec<[f64; 2]>,
    /// The power of each held band in each section of the version against
    /// its integrated loudness (`low_end::Sections::power_db`), in dB: the
    /// weight of the section in the loudness that a hold bell adds (D282).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub power_db: Vec<[f64; 2]>,
    /// The power of the mid and of the side in each sub-band of the held
    /// bands in each section of the version (`low_end::Sections::low_db`,
    /// D288). Empty in a record of an earlier engine.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub low_db: Vec<Vec<[f64; 2]>>,
}

impl HoldSections {
    /// The sections of a version with the spec `spec`, the hold steps
    /// `steps` of its chosen candidate, and its sections and those of the
    /// reference, whose tone is `reference_tone`. The intended move is that
    /// of the hold: the static low shelves of the version against those of
    /// the reference (D280).
    #[allow(clippy::too_many_arguments)]
    pub fn of(
        spec: &Spec,
        spectrum: &Spectrum,
        steps: &[crate::low_end::Step],
        version: &crate::low_end::Sections,
        version_lufs: f64,
        reference: &crate::low_end::Sections,
        reference_lufs: f64,
        reference_tone: Option<&crate::treatment::Tone>,
    ) -> Self {
        let intended = crate::low_end::intended_against_db(spec, reference_tone, spectrum);
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
            side_share: version.side_shares().unwrap_or_default(),
            power_db: version
                .power_db
                .iter()
                .map(|p| p.map(|p| p - version_lufs))
                .collect(),
            low_db: version.low_db.clone(),
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
    // The duplicate screen refuses a share that moves no requested control
    // by the detectable increment (D279).
    if !steps.detectable_at(share) {
        return Err("The share changes each control by less than the detectable increment".into());
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
    // A Compression step changes the mean reduction by 0.5 dB: the builder
    // solves the move of the threshold from the level distribution of the
    // parent's compressor (D279). A parent without it moves the threshold
    // by `COMPRESSION_THRESHOLD_DB` for each step.
    if let Some(settings) = &mut spec.compressor {
        let shift = match available.compressor {
            Some(compression) if steps.compression != 0 => {
                compression_shift(&compression, *settings, change(steps.compression))
                    .ok_or("The request is outside the range of the compressor")?
            }
            _ => steps.compression as f64 * COMPRESSION_THRESHOLD_DB * share,
        };
        settings.threshold_dbfs -= shift;
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
    // A share whose level leaves the loudness range is left out, as a share
    // outside the range of the filters is: the result then reports the steps
    // that it delivers.
    spec.target_lufs = parent.target_lufs + change(steps.loudness);
    if steps.loudness != 0
        && !(LOUDNESS_RANGE_LUFS.0..=LOUDNESS_RANGE_LUFS.1).contains(&spec.target_lufs)
    {
        return Err("The request is outside the loudness range".into());
    }
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
    /// The reference record of the set (`distributed::REFERENCE_SCHEMA`):
    /// the measurements of the render that the `essential` version chose. A
    /// request takes its reference from the record, as the families of the
    /// set do (D217). A base of a set made before D217 has none, and its
    /// requests render the `essential` spec.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub record: Option<Value>,
}

/// The field of the Essential version record of a cloud manifest that holds
/// the reference record of the set (D270). The base of a manifest keeps no
/// record: the browser engine of 1.14.0 refuses a base with `record`, and it
/// takes each version record as it comes.
pub const VERSION_REFERENCE_FIELD: &str = "reference_record";

impl Base {
    /// The base with the reference record of the set from `versions`, the
    /// version records of a cloud manifest, when it has none (D270). A record
    /// without its chosen spec takes the spec of the Essential version
    /// record, the spec that the manifest omits from it (D271).
    pub fn with_versions_record(mut self, versions: &[Value]) -> Self {
        if self.record.is_none() {
            self.record = versions
                .iter()
                .find(|v| v["id"] == "essential")
                .and_then(|v| {
                    let mut record = v[VERSION_REFERENCE_FIELD].clone();
                    let fields = record.as_object_mut()?;
                    if !fields.contains_key(crate::distributed::CHOSEN_SPEC_FIELD) {
                        fields.insert(
                            crate::distributed::CHOSEN_SPEC_FIELD.into(),
                            v["spec"].clone(),
                        );
                    }
                    Some(record)
                });
        }
        self
    }

    /// The base of a worker plan of a set.
    pub fn of_plan(set_plan: &Value) -> Result<Self> {
        if set_plan["schema"] != crate::distributed::PLAN_SCHEMA
            && set_plan["schema"] != crate::distributed::FULL_PLAN_SCHEMA
        {
            return Err("Unsupported worker plan schema".into());
        }
        let families = set_plan["families"]
            .as_array()
            .ok_or("Worker plan without families")?;
        // Each family after `essential` names the `essential` spec as its
        // reference; the `essential` family renders it as its first
        // candidate.
        let mut reference = families
            .iter()
            .find_map(|f| (!f["reference"].is_null()).then(|| f["reference"].clone()))
            .or_else(|| {
                families
                    .iter()
                    .find(|f| f["id"] == "essential")
                    .map(|f| f["versions"][0]["candidates"][0]["spec"].clone())
            })
            .ok_or("Worker plan without an essential spec")?;
        if let Some(shared) = set_plan["shared"].as_array() {
            crate::distributed::expand_spec(&mut reference, shared)?;
        }
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
            record: None,
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
    // Each spec shares its keys as the plan takes it, in the order of
    // `distributed::share_specs`, so the plan holds one copy of each key
    // (D278).
    let mut shared = Vec::new();
    let mut reference = base.reference.clone();
    crate::distributed::share_spec(&mut reference, &mut shared);
    let candidates = candidates
        .iter()
        .map(|c| {
            let mut spec = serde_json::to_value(&c.spec)?;
            crate::distributed::share_spec(&mut spec, &mut shared);
            Ok(json!({
                "reason": c.reason,
                "spec": spec,
                "gates": c.gates,
            }))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut plan = json!({
        "schema": crate::distributed::PLAN_SCHEMA,
        "sample_rate": base.sample_rate(),
        "source_pcm_sha256": base.source_pcm_sha256,
        "source_frames": base.source_frames,
        "source_condition": base.source_condition,
        "facts": base.facts,
        "families": [{
            "id": id,
            "reference": reference,
            "versions": [{
                "id": id,
                "rule": RULE,
                "basis": request.basis(parent_id)?,
                "role": "Base",
                "label": "",
                // The parent spec holds the sub lift of the parent.
                "lifted": true,
                "candidates": candidates,
            }],
        }],
    });
    plan["shared"] = Value::Array(shared);
    Ok(plan)
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
    /// The programme loudness and the true-peak ceiling of radio (EBU R 128
    /// s3), with the fold of the Mono preset (`adjust.radio.v1`).
    Radio,
    /// The fold: each band takes back the side/mid of the mix, and its mid
    /// takes the power of the side that the widening adds
    /// (`adjust.mono.v1`).
    Mono,
}

impl Preset {
    pub const ALL: [Preset; 4] = [
        Preset::MonoSafe,
        Preset::Streaming,
        Preset::Radio,
        Preset::Mono,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Preset::MonoSafe => "Mono-safe",
            Preset::Streaming => "Streaming",
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
            Preset::Radio => "radio",
            Preset::Mono => "mono",
        }
    }

    /// The rule of the preset.
    pub fn rule(self) -> &'static str {
        match self {
            Preset::MonoSafe => crate::width::MONO_SAFE_RULE,
            Preset::Streaming => "adjust.streaming.v1",
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

/// What a medium preset sets on a version.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Medium {
    pub target_lufs: f64,
    pub ceiling_dbtp: f64,
    /// The side that the widening of the version adds goes to the mid
    /// (`folded`).
    pub fold: bool,
}

/// The settings of a medium preset on `parent`, or None for a preset that
/// answers a condition of the mix.
pub fn medium(parent: &Spec, preset: Preset) -> Option<Medium> {
    match preset {
        Preset::MonoSafe => None,
        Preset::Streaming => Some(Medium {
            target_lufs: parent.target_lufs.min(STREAMING_LUFS),
            ceiling_dbtp: parent.ceiling_dbtp.min(CODEC_CEILING_DBTP),
            fold: false,
        }),
        Preset::Radio => Some(Medium {
            target_lufs: RADIO_LUFS,
            ceiling_dbtp: parent.ceiling_dbtp.min(RADIO_CEILING_DBTP),
            fold: true,
        }),
        Preset::Mono => Some(Medium {
            target_lufs: parent.target_lufs,
            ceiling_dbtp: parent.ceiling_dbtp,
            fold: true,
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

/// The spec of `medium` on `parent` at `share` of its fold. The ceiling and
/// a published level stay in full: they are delivery conditions of the
/// medium. The low-end hold of the parent is removed: the render solves it
/// again.
fn medium_spec(parent: &Spec, medium: Medium, spectrum: &BandSpectrum, share: f64) -> Result<Spec> {
    let mut spec = parent.clone();
    spec.target_lufs = medium.target_lufs;
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
    /// mono sum of a band for Mono, and any of them for Radio.
    pub fn serves(&self, preset: Preset) -> bool {
        let mono = self.mono_db.iter().any(|m| distinct(*m));
        match preset {
            Preset::Streaming => distinct(self.loudness_lu) || distinct(self.ceiling_db),
            Preset::Radio => distinct(self.loudness_lu) || distinct(self.ceiling_db) || mono,
            Preset::Mono => mono,
            Preset::MonoSafe => false,
        }
    }
}

pub fn medium_change(parent: &Spec, preset: Preset, facts: &Facts) -> Option<MediumChange> {
    let medium = medium(parent, preset)?;
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
    if let Some(medium) = medium(parent, preset) {
        if !offers(parent, preset, facts) {
            return Err(format!(
                "The {} preset changes this version by less than {} dB",
                preset.name(),
                crate::selection::DISTINCT_DB
            )
            .into());
        }
        let shares: &[f64] = if medium.fold && folded(parent, &facts.spectrum, 1.0).is_some() {
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
/// none: the range that the panel shows before a request. A step whose first
/// share moves its control no further than a smaller step in the same
/// direction is null too: it gives the same change (D279). A gate of the
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
        let mut shares = Vec::new();
        for step in span {
            let mut request = serde_json::to_value(Steps::default())?;
            request[control] = step.into();
            let request: Steps = serde_json::from_value(request)?;
            let share = SHARES
                .into_iter()
                .find(|share| spec_at(&spec, &request, *share, &facts.spectrum, available).is_ok());
            shares.push((step, share));
        }
        // Each share keeps the ceiling, so each Ceiling step moves further.
        if control != "ceiling" {
            shares.sort_by_key(|(step, _)| step.abs());
            let mut furthest = [0.0f64; 2];
            for (step, share) in &mut shares {
                let reach = &mut furthest[usize::from(*step > 0)];
                match *share {
                    Some(s) if step.abs() as f64 * s > *reach + 1e-9 => {
                        *reach = step.abs() as f64 * s;
                    }
                    _ => *share = None,
                }
            }
        }
        for (step, share) in shares {
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

    /// True when this sound, the result of `steps`, is the same as
    /// `other`: the signatures are duplicates (`Signature::is_duplicate_of`)
    /// at a loudness within `DISTINCT_DB`, and no control measure, the
    /// compressor reduction, the de-esser cut or the ceiling differs by
    /// `DISTINCT_DB` or more. A request asks for one of these, so each one
    /// makes a version distinct, unlike in the versions of a set (D135). The
    /// loudness or the measure of a control that `steps` moves counts from
    /// `DETECTABLE_DB` (D279).
    pub fn is_duplicate_of(&self, other: &Sound, steps: &Steps) -> bool {
        let limit = |step: i32| {
            if step == 0 {
                crate::selection::DISTINCT_DB
            } else {
                DETECTABLE_DB
            }
        };
        let (a, b) = (&self.measures, &other.measures);
        self.signature.is_duplicate_of(&other.signature)
            && [
                (self.signature.lufs - other.signature.lufs, steps.loudness),
                (a.bass_db - b.bass_db, steps.bass),
                (a.presence_db - b.presence_db, steps.presence),
                (a.air_db - b.air_db, steps.air),
                (a.width_db - b.width_db, steps.width),
                (self.compressor_db - other.compressor_db, steps.compression),
                (self.deesser_cut_db - other.deesser_cut_db, steps.sibilance),
                (self.ceiling_dbtp - other.ceiling_dbtp, steps.ceiling),
            ]
            .iter()
            .all(|(d, step)| d.abs() < limit(*step))
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
fn mono_fold_down_db(
    audio: &mut impl crate::scratch::Frames,
    rate: u32,
) -> Result<[Option<f64>; 6]> {
    let image = crate::stereo::analyze_read(audio, rate)?;
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
    input: &mut impl crate::scratch::Frames,
    rendered: &mut impl crate::scratch::Frames,
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
/// render rate, the rate of `facts`. The measures read the source and the
/// render in blocks and batches, and the render waits in scratch storage
/// while they read it (D257).
#[allow(clippy::too_many_arguments)]
pub fn finalize(
    input: &mut crate::treatment_render::RenderSource<'_>,
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
        let temporal = {
            let mut scan = crate::analysis::Scan::new(input.len(), rate);
            crate::scratch::for_each_block(&mut input.frames(), |block| {
                scan.push(block);
                Ok(())
            })?;
            scan.finish()
        };
        // The prefix and its 4x frames wait in scratch storage, and the
        // probes read them in spans (D257).
        let mut renderer =
            crate::treatment_render::Renderer::of_source(input.lend(), rate, store.scratch())?;
        let (prefix, mut up) = renderer.prepared_frames(&spec)?;
        crate::audibility::point_on(
            &mut crate::audibility::Prefix::Shared(crate::scratch::Shared(prefix)),
            &mut up,
            version.drive_db,
            spec.clipper_db,
            spec.ceiling_dbtp,
            spec.limiter,
            spec.true_peak,
            Some(&temporal),
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
    let mut rendered = store.read_stored(&version.raw_wav_file, rate)?;
    let adjusted = VersionFacts::of_read(&mut rendered.audio, version.lufs, mean, rate)?;
    // The Mono-safe preset states the mono fold-down loss of its bands, in
    // the mix and in the result.
    let mono_safe = match (request.preset, facts.mono_safe.as_ref()) {
        (Some(Preset::MonoSafe), Some(bells)) if !bells.is_empty() => {
            let edges = &crate::stereo::BAND_EDGES_HZ;
            let bands: Vec<usize> = bells
                .iter()
                .filter_map(|b| (0..6).find(|i| b.band.hz >= edges[*i] && b.band.hz < edges[i + 1]))
                .collect();
            fn loss(
                audio: &mut impl crate::scratch::Frames,
                bands: &[usize],
                rate: u32,
            ) -> Result<f64> {
                let image = crate::stereo::analyze_read(audio, rate)?;
                let values: Vec<f64> = bands
                    .iter()
                    .filter_map(|&b| image.whole.bands[b].mono_fold_down_db)
                    .collect();
                Ok(values.iter().sum::<f64>() / values.len().max(1) as f64)
            }
            let first = &bells[0].points;
            let wet =
                first.iter().filter(|p| p.wet > 0.5).count() as f64 / first.len().max(1) as f64;
            Some(json!({
                "bands": bands,
                "low_hz": bands.iter().map(|&b| edges[b]).fold(f64::INFINITY, f64::min),
                "high_hz": bands.iter().map(|&b| edges[b + 1]).fold(0.0, f64::max),
                "song_share": wet,
                "mono_loss_mix_db": loss(&mut input.frames(), &bands, rate)?,
                "mono_loss_db": loss(&mut rendered.audio, &bands, rate)?,
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
            &mut input.frames(),
            &mut rendered.audio,
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
            .find(|o| sound.is_duplicate_of(&o.sound, steps))
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
        "work": family.work,
    }))
}

/// Mid and side power of the held bands (`low_end::BANDS_HZ`) and
/// K-weighted power of the stored spectrum after the tone of `spec` with
/// its side gain.
fn held_power(spectrum: &BandSpectrum, spec: &Spec) -> ([[f64; 2]; 2], f64) {
    let tone = spec.mean_tone();
    let side = spec.side_gain.as_ref();
    let (mut held, mut weighted) = ([[0.0f64; 2]; 2], 0.0);
    for (i, &hz) in spectrum.centers_hz.iter().enumerate() {
        let parts = shaped_parts(spectrum, &tone, side, i);
        weighted += dsp::k_weighting_power(hz, spectrum.sample_rate) * (parts[0] + parts[1]);
        for (b, (low, high)) in crate::low_end::BANDS_HZ.iter().enumerate() {
            if (*low..*high).contains(&hz) {
                held[b][0] += parts[0];
                held[b][1] += parts[1];
            }
        }
    }
    (held, weighted)
}

/// The change of each held band in each section that peak control makes
/// when the input of peak control of the parent `delivered` takes the drive
/// change of `spec` against its ceiling (`DriveResponse`, D282). Peak
/// control scales with its ceiling (D281). Empty without the sections of a
/// drive response.
fn peak_control_hold_db(delivered: &Spec, spec: &Spec, response: &Value) -> Vec<[f64; 2]> {
    peak_control_sections_db(response, input_change_db(delivered, spec, response))
}

/// The change of each held band in each section that peak control makes
/// when its input moves by `x` against its ceiling (D282). Empty without
/// the sections of a drive response.
fn peak_control_sections_db(response: &Value, x: f64) -> Vec<[f64; 2]> {
    let Some(curve) = Response::of(response) else {
        return Vec::new();
    };
    let sections = |key: &str| -> Vec<[f64; 2]> {
        serde_json::from_value(response[key].clone()).unwrap_or_default()
    };
    let (down, up, free, far) = (
        sections("sections_down_db"),
        sections("sections_up_db"),
        sections("sections_free_db"),
        sections("sections_far_db"),
    );
    if down.len() != up.len() || down.len() != free.len() {
        return Vec::new();
    }
    (0..down.len())
        .map(|s| {
            std::array::from_fn(|b| {
                let far = (far.len() == down.len()).then(|| far[s][b]);
                curve.section_change_db(down[s][b], up[s][b], free[s][b], far, x)
            })
        })
        .collect()
}

/// The change of the input of peak control against its ceiling from the
/// parent `delivered` to `spec`: the drive change that moves the output
/// loudness by the target change less the ceiling change (D281).
fn input_change_db(delivered: &Spec, spec: &Spec, response: &Value) -> f64 {
    let lu =
        spec.target_lufs - delivered.target_lufs - (spec.ceiling_dbtp - delivered.ceiling_dbtp);
    drive_change_db(response, lu)
}

/// The restoring gains of the final render of a request on the parent
/// record `parent`, whose delivered spec is `delivered`, when the request
/// moves the input of peak control by `x` against its ceiling (D284). The
/// final render starts from the restoring gains of the reference render,
/// Essential (`selection::warm_start`). A version after Essential started
/// from them too, so the first render of the request takes the gains of the
/// first render of the parent and leaves the change that it measured, with
/// the local band change of peak control at `x` (`local_change_db`, D285).
/// A request on Essential starts from its delivered gains and the change
/// that they leave. Each restoration pass takes the gains
/// of `treatment_render::next_gains`, and the change that it leaves moves
/// by the change of the shelf model for its gain step of 20–120 Hz and by
/// the response of the restoring filter of each other band. Without the
/// local band changes of a drive response, the gains of the parent.
fn predicted_restoration(
    parent: &Value,
    delivered: &Spec,
    x: f64,
    shelves: f64,
    rate: u32,
) -> Restored {
    let report = render_report(parent);
    let (restoration, response) = (&report["restoration"], &report["drive_response"]);
    let six =
        |key: &str| -> Option<[f64; 6]> { serde_json::from_value(response[key].clone()).ok() };
    let measured = || -> Option<_> {
        Some((
            serde_json::from_value::<[f64; 6]>(restoration["delivered_change_db"].clone()).ok()?,
            serde_json::from_value::<[bool; 6]>(restoration["material"].clone()).ok()?,
            [
                six("local_down_db")?,
                six("local_up_db")?,
                six("local_far_db")?,
                six("local_free_db")?,
            ],
            six("shelf_local_db")?,
            response["shelf_db"].as_f64().filter(|g| *g > 0.0)?,
            response["far_db"].as_f64()?,
            Response::of(response)?,
        ))
    };
    let Some((left, material, [down, up, far, free], shelf, shelf_db, far_db, curve)) = measured()
    else {
        return Restored {
            start: delivered.restoration_db,
            passes: Vec::new(),
        };
    };
    let first = || -> Option<([f64; 6], [f64; 6])> {
        Some((
            serde_json::from_value(restoration["measured_gains_db"].clone()).ok()?,
            serde_json::from_value(restoration["measured_change_db"].clone()).ok()?,
        ))
    };
    let (start, left) = match first() {
        Some(first) if parent["id"] != "essential" => first,
        _ => (delivered.restoration_db, left),
    };
    // The change of 40–120 Hz from the tone of the request takes the change
    // of peak control of the shelf model at the same loudness of its input
    // (D287).
    let taken = match (
        six("shelf_free_local_db"),
        response["shelf_free_lu"].as_f64(),
    ) {
        (Some(shelf_free), Some(louder)) => std::array::from_fn(|b| {
            shelves
                * (shelf[b] - shelf_free[b] + free[b]
                    - local_change_db(&curve, down[b], up[b], free[b], far[b], far_db, louder))
        }),
        _ => [0.0; 6],
    };
    let change: [f64; 6] = std::array::from_fn(|b| {
        left[b] + local_change_db(&curve, down[b], up[b], free[b], far[b], far_db, x) + taken[b]
    });
    let mut r = crate::treatment_render::Restoration {
        rule: "preview",
        window_samples: crate::treatment_render::RESTORATION_WINDOW,
        material,
        measured_change_db: change,
        measured_gains_db: start,
        filters: Vec::new(),
        delivered_change_db: change,
        renders: 1,
        gains_db: start,
    };
    let mut passes = Vec::new();
    for _ in 0..crate::treatment_render::RESTORATION_PASSES {
        let Some(total) = crate::treatment_render::next_gains(&r, rate) else {
            break;
        };
        let step: [f64; 6] = std::array::from_fn(|f| total[f] - r.gains_db[f]);
        for (b, change) in r.delivered_change_db.iter_mut().enumerate() {
            *change += step[0] * shelf[b] / shelf_db
                + (1..6)
                    .map(|f| step[f] * crate::treatment_render::restoring_response_db(f, b, rate))
                    .sum::<f64>();
        }
        r.gains_db = total;
        passes.push(total);
    }
    Restored { start, passes }
}

/// The restoring gains of the first render of a request and of each
/// restoration pass of its final render (`predicted_restoration`).
#[derive(Clone, Debug, PartialEq)]
struct Restored {
    start: [f64; 6],
    passes: Vec<[f64; 6]>,
}

impl Restored {
    /// The restoring gains of the delivered render.
    fn gains(&self) -> [f64; 6] {
        self.passes.last().copied().unwrap_or(self.start)
    }
}

/// The restoration of a render with the hold, for its hold steps
/// (`hold_steps`, D286): the change of each held band in each section from
/// the delivered gains of the parent to the gains of the first render, and
/// for each pass, the shift that the final render plans for it
/// (`low_end::restoration_shift_db`) and the change of each section that it
/// gives (`restoration_hold_db`).
#[derive(Default)]
struct HoldPasses {
    first: Vec<[f64; 2]>,
    passes: Vec<([f64; 2], Vec<[f64; 2]>)>,
}

impl HoldPasses {
    /// From the delivered gains `delivered`, the gains `start` of the first
    /// render and the gains of each pass.
    fn of(
        delivered: &[f64; 6],
        start: &[f64; 6],
        passes: &[[f64; 6]],
        response: &Value,
        spectrum: &crate::Spectrum,
        count: usize,
    ) -> Self {
        let mut from = *start;
        let passes = passes
            .iter()
            .map(|to| {
                let pass = (
                    crate::low_end::restoration_shift_db(spectrum, &from, to),
                    restoration_hold_db(&from, to, response, spectrum, count),
                );
                from = *to;
                pass
            })
            .collect();
        Self {
            first: restoration_hold_db(delivered, start, response, spectrum, count),
            passes,
        }
    }
}

/// The hold of the final render (`selection::hold_low_end`) of `spec` from
/// the changes of the held bands of its first render, `changes`, and its
/// restoration passes (D286). Each step plans against the changes of the
/// render before it plus the shift that the final render plans for the
/// pass that the step takes. The render of a step adds the predicted change
/// of the step and the change of its pass. A step whose bells take the tone
/// past the response bound of `Tone::validate` is not taken. With `renders`,
/// the steps stop after that many renders after the first. The rise of
/// 40–120 Hz of each step takes `take`, the change of peak control per dB of
/// 40–120 Hz in each section (`shelf_take_db`, D290).
#[allow(clippy::too_many_arguments)]
fn hold_steps(
    spec: &Spec,
    facts: &Facts,
    low: &crate::low_end::Sections,
    changes: &[[Option<f64>; 2]],
    restoration: &HoldPasses,
    renders: Option<usize>,
    take: &[[f64; 2]],
    frames: usize,
) -> Option<crate::low_end::Step> {
    let mut spec = spec.clone();
    let mut measured = changes.to_vec();
    let mut previous: Option<(crate::low_end::Step, Vec<[Option<f64>; 2]>)> = None;
    let mut steps = 0;
    for k in 0..renders.unwrap_or(usize::MAX) {
        let pass = restoration.passes.get(k);
        let shift = pass.map_or([0.0; 2], |p| p.0);
        let planned: Vec<[Option<f64>; 2]> = measured
            .iter()
            .map(|c| std::array::from_fn(|b| c[b].map(|x| x + shift[b])))
            .collect();
        // The final render plans each step on the spectrum after the tone of
        // the version (`low_end::shaped_spectrum`).
        let hold = if !crate::low_end::passes(&planned) && steps < crate::low_end::ITERATIONS {
            crate::low_end::next(
                &spec,
                &shaped_spectrum(&facts.spectrum, &spec),
                low,
                &planned,
                previous.as_ref().map(|(p, c)| (p, c.as_slice())),
                frames,
            )
            .filter(|(next, _)| {
                next.effective_tone()
                    .0
                    .validate(facts.spectrum.sample_rate)
                    .is_ok()
            })
        } else {
            None
        };
        if hold.is_none() && pass.is_none() {
            break;
        }
        for (s, c) in measured.iter_mut().enumerate() {
            for (b, x) in c.iter_mut().enumerate() {
                if let Some(x) = x {
                    *x += pass.and_then(|p| p.1.get(s)).map_or(0.0, |r| r[b]);
                    if let Some((_, step)) = &hold {
                        // The bells of 40–120 Hz take the change of peak
                        // control of the shelf model (D290).
                        let rise = step.expected_db[s][0] + step.offset_db;
                        *x += step.expected_db[s][b] + take.get(s).map_or(0.0, |t| rise * t[b]);
                    }
                }
            }
        }
        if let Some((next, step)) = hold {
            spec = next;
            previous = Some((step, planned));
            steps += 1;
        }
    }
    previous.map(|(step, _)| step)
}

/// The local band change of peak control of a band at the input change `x`
/// against its ceiling, from its changes `down` and `up` at the drive
/// `shift` down and up, `free`, its change without peak control, and `far`,
/// its change at the drive `far_db` up, against the drive of the render
/// (`treatment_render::DriveResponse`, D284): linear between the models,
/// and over the drive `far_db` up, along the line from the drive `shift` up.
/// Under the drive `shift` down, the change left to `free` falls with the
/// loss of peak control, as `Response::section_change_db`.
fn local_change_db(
    curve: &Response,
    down: f64,
    up: f64,
    free: f64,
    far: f64,
    far_db: f64,
    x: f64,
) -> f64 {
    let shift = curve.shift;
    if x > shift && far_db > shift {
        return up + (far - up) * (x - shift) / (far_db - shift);
    }
    if x >= 0.0 {
        return up * x / shift;
    }
    if x >= -shift {
        return -down * x / shift;
    }
    match curve.tail {
        Some((_, rate)) => free + (down - free) * (rate * (x + shift)).exp(),
        None => down,
    }
}

/// The change of each held band in each section against the loudness that
/// the restoring gains `to` add against the gains `from` (D284):
/// for the gain change of 20–120 Hz, the change of the section in the shelf
/// model of the drive response, after peak control, and for the other
/// bands, the change from the source spectrum, as the final render plans a
/// restoration pass (`low_end::restoration_shift_db`). Without the sections
/// of the shelf model, the change from the source spectrum in each of the
/// `count` sections.
fn restoration_hold_db(
    from: &[f64; 6],
    to: &[f64; 6],
    response: &Value,
    spectrum: &crate::Spectrum,
    count: usize,
) -> Vec<[f64; 2]> {
    let (from, to) = (*from, *to);
    if from == to {
        return Vec::new();
    }
    let shelf: Vec<[f64; 2]> =
        serde_json::from_value(response["sections_shelf_db"].clone()).unwrap_or_default();
    let shelf_db = response["shelf_db"].as_f64().filter(|g| *g > 0.0);
    match shelf_db.filter(|_| shelf.len() == count) {
        Some(shelf_db) => {
            // The other bands from the source spectrum, with the gain of
            // 20–120 Hz of the parent.
            let mut rest = to;
            rest[0] = from[0];
            let other = crate::low_end::restoration_shift_db(spectrum, &from, &rest);
            shelf
                .iter()
                .map(|s| std::array::from_fn(|b| (to[0] - from[0]) * s[b] / shelf_db + other[b]))
                .collect()
        }
        None => vec![crate::low_end::restoration_shift_db(spectrum, &from, &to); count],
    }
}

/// The gains of the mid and of the side of each sub-band of the held bands
/// (`low_end::low_band`) from the tone of `parent` to that of `candidate`,
/// at `rate` (D288).
fn low_gains(parent: &Spec, candidate: &Spec, rate: u32) -> Vec<[f64; 2]> {
    let (tone_a, tone_b) = (parent.mean_tone(), candidate.mean_tone());
    (0..crate::low_end::BANDS_HZ.len() * crate::low_end::LOW_SPLITS)
        .map(|k| {
            let hz = crate::low_end::low_band_hz(k);
            let a = tone_gains(&tone_a, parent.side_gain.as_ref(), hz, rate);
            let b = tone_gains(&tone_b, candidate.side_gain.as_ref(), hz, rate);
            [b[0] / a[0].max(1e-30), b[1] / a[1].max(1e-30)]
        })
        .collect()
}

/// The change of each held band of a section whose sub-bands hold the
/// power `low` of the mid and of the side, in dB, under the gains `gains`
/// of each sub-band (`low_gains`, D288).
fn section_tone_db(low: &[[f64; 2]], gains: &[[f64; 2]]) -> [f64; 2] {
    let splits = crate::low_end::LOW_SPLITS;
    std::array::from_fn(|b| {
        let (mut before, mut after) = (0.0, 0.0);
        for (power, gain) in low.iter().zip(gains).skip(b * splits).take(splits) {
            let power = power.map(|p| 10f64.powf(p / 10.0));
            before += power[0] + power[1];
            after += power[0] * gain[0] + power[1] * gain[1];
        }
        10.0 * (after.max(1e-30) / before.max(1e-30)).log10()
    })
}

/// The change of 40–120 Hz of the restoring filter of 20–120 Hz of the
/// shelf model of the drive response, from `spectrum`, in dB.
fn shelf_raise_db(response: &Value, spectrum: &Spectrum) -> Option<f64> {
    let shelf_db = response["shelf_db"].as_f64().filter(|g| *g > 0.0)?;
    let filter = crate::treatment_render::restoring_filter(0, shelf_db);
    let (held, _) = crate::low_end::effect(spectrum, &filter, crate::finishing::Domain::Stereo);
    (held[0] > 0.0).then_some(held[0])
}

/// The change of each held band in each section that peak control makes
/// per dB of 40–120 Hz at its input, at the same loudness of its input
/// (D287): the change of the section in the shelf model, less its change in
/// the shelf model without peak control and plus its change without peak
/// control, less the change of peak control in the section when its input
/// takes the loudness that the shelf adds to the prefix, over the change of
/// 40–120 Hz of the shelf. Empty without these models.
fn shelf_take_db(response: &Value, spectrum: &Spectrum, count: usize) -> Vec<[f64; 2]> {
    let sections = |key: &str| -> Vec<[f64; 2]> {
        serde_json::from_value(response[key].clone()).unwrap_or_default()
    };
    let (shelf, shelf_free, free) = (
        sections("sections_shelf_db"),
        sections("sections_shelf_free_db"),
        sections("sections_free_db"),
    );
    let louder = response["shelf_free_lu"]
        .as_f64()
        .map(|x| peak_control_sections_db(response, x))
        .unwrap_or_default();
    match shelf_raise_db(response, spectrum) {
        Some(raise) if [shelf.len(), shelf_free.len(), free.len(), louder.len()] == [count; 4] => {
            (0..count)
                .map(|s| {
                    std::array::from_fn(|b| {
                        (shelf[s][b] - shelf_free[s][b] + free[s][b] - louder[s][b]) / raise
                    })
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

/// The change of 40–120 Hz from the tone of `spec` against that of the
/// parent `delivered`, without their holds, over the change of 40–120 Hz of
/// the shelf model (D287); 0 without the shelf model.
fn tone_shelves(delivered: &Spec, spec: &Spec, response: &Value, facts: &Facts) -> f64 {
    let Some(raise) = shelf_raise_db(response, &shaped_spectrum(&facts.spectrum, delivered)) else {
        return 0.0;
    };
    let power = |s: &Spec| {
        let mut s = s.clone();
        s.low_end_hold = None;
        let held = held_power(&facts.spectrum, &s).0[0];
        held[0] + held[1]
    };
    10.0 * (power(spec).max(1e-30) / power(delivered).max(1e-30)).log10() / raise
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
/// difference of the two holds to the parent's hold. The predicted change
/// of a section is:
///
/// - the change of the tone and the width of its mid and its side, by the
///   side share of the section (D282);
/// - `compression`, the change of the compressor in each section
///   (`compression_hold_db`);
/// - `peak_control`, the change of peak control in each section
///   (`peak_control_hold_db`, D282);
/// - the change of the restoring gains of the first render in each section
///   (`HoldPasses::first`, D284, D286).
///
/// The hold steps take the restoration passes of the final render of the
/// request, `request`, and of the render of the parent, `restored`
/// (`hold_steps`, D286). The steps of the parent stop after the renders of
/// its hold (`Restoration::renders`): on track-d, the changes of its
/// sections pass the hold by 0.003 dB, and a second step of the model of
/// its hold would take back the wets of the request in four sections.
#[allow(clippy::too_many_arguments)]
fn predicted_hold(
    delivered: &Spec,
    spec: &Spec,
    sections: &HoldSections,
    compression: &[[f64; 2]],
    peak_control: &[[f64; 2]],
    request: &HoldPasses,
    restored: &HoldPasses,
    renders: Option<usize>,
    take: &[[f64; 2]],
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
    let shared: [f64; 2] = std::array::from_fn(|b| {
        -10.0 * (weighted_b.max(1e-30) / weighted_a.max(1e-30)).log10()
            - (intended_b[b] - intended_a[b])
    });
    // The gains of the mid and the side of each held band, and the side
    // share of the stored spectrum for a record without the shares of its
    // sections.
    let gain = |b: usize, part: usize| held_b[b][part].max(1e-30) / held_a[b][part].max(1e-30);
    let mix_share: [f64; 2] =
        std::array::from_fn(|b| held_a[b][1] / (held_a[b][0] + held_a[b][1]).max(1e-30));
    let low_gains = low_gains(&parent, &candidate, facts.spectrum.sample_rate);
    let changes: Vec<[Option<f64>; 2]> = sections
        .free_change_db
        .iter()
        .enumerate()
        .map(|(s, c)| {
            let compressor = compression.get(s).copied().unwrap_or([0.0; 2]);
            let share = sections.side_share.get(s).copied().unwrap_or(mix_share);
            let level = peak_control.get(s).copied().unwrap_or([0.0; 2]);
            // The tone of each section from its own spectrum of the held
            // bands (D288), or from the stored spectrum with the side share
            // of the section.
            let tone: [f64; 2] = match sections
                .low_db
                .get(s)
                .filter(|l| l.len() == low_gains.len())
            {
                Some(low) => section_tone_db(low, &low_gains),
                None => std::array::from_fn(|b| {
                    10.0 * ((1.0 - share[b]) * gain(b, 0) + share[b] * gain(b, 1))
                        .max(1e-30)
                        .log10()
                }),
            };
            // Peak control takes a share of the change of 40–120 Hz (D287).
            let taken = take.get(s).copied().unwrap_or([0.0; 2]);
            std::array::from_fn(|b| {
                let first = request.first.get(s).map_or(0.0, |r| r[b]);
                c[b].map(|c| {
                    c + tone[b] + shared[b] + compressor[b] + level[b] + first + tone[0] * taken[b]
                })
            })
        })
        .collect();
    let low = crate::low_end::Sections {
        starts: sections.starts.clone(),
        ends: sections.ends.clone(),
        ..Default::default()
    };
    // The steps from the parent's changes give the parent's hold, as its
    // render did. The preview keeps the parent's wets and adds the
    // difference that the request makes to the steps.
    let first: Vec<[Option<f64>; 2]> = sections
        .free_change_db
        .iter()
        .enumerate()
        .map(|(s, c)| {
            std::array::from_fn(|b| c[b].map(|x| x + restored.first.get(s).map_or(0.0, |r| r[b])))
        })
        .collect();
    let (before, after) = (
        hold_steps(
            &parent, facts, &low, &first, restored, renders, take, frames,
        ),
        hold_steps(
            &candidate, facts, &low, &changes, request, None, take, frames,
        ),
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

/// The drive response of a render (D281): the quadratic through the
/// loudness changes at the drive `shift` up and down, and its tail under
/// the drive `shift` down. Under that drive, the loss of peak control left
/// at it, `left`, falls away exponentially at `rate`, from the slope of the
/// quadratic there, and the output approaches the drive change plus the
/// loss at the drive of the render (`loss`). Over the drive `shift` up, the
/// quadratic through the changes at the drive `shift` up and at the drive
/// of the far model (D283).
struct Response {
    shift: f64,
    up: f64,
    down: f64,
    loss: f64,
    slope: f64,
    curve: f64,
    /// `left` and `rate`, when the loss left at the drive `shift` down
    /// falls from a slope under 1.
    tail: Option<(f64, f64)>,
    /// The drive of the far model, and the slope and the curve of the
    /// quadratic over the drive `shift` up, when the quadratic rises at the
    /// drive `shift` up.
    far: Option<(f64, f64, f64)>,
}

impl Response {
    /// The response of a render report, or None without one.
    fn of(response: &Value) -> Option<Self> {
        let (Some(shift), Some(up), Some(down), Some(loss)) = (
            response["shift_db"].as_f64(),
            response["up_lu"].as_f64(),
            response["down_lu"].as_f64(),
            response["loss_lu"].as_f64(),
        ) else {
            return None;
        };
        let slope = (up - down) / (2.0 * shift);
        let curve = (up + down) / (2.0 * shift * shift);
        if !(slope > 0.1 && curve.is_finite() && loss.is_finite()) {
            return None;
        }
        let left = loss - shift - down;
        let start = slope - 2.0 * curve * shift;
        let tail = (left > 0.0 && start < 1.0).then(|| (left, (1.0 - start) / left));
        let far = match (response["far_db"].as_f64(), response["far_lu"].as_f64()) {
            (Some(far), Some(lu)) if far > shift => {
                let (slope, curve) = through(shift, up, far, lu);
                (slope > 0.0 && slope + 2.0 * curve * shift > 0.0).then_some((far, slope, curve))
            }
            _ => None,
        };
        Some(Self {
            shift,
            up,
            down,
            loss,
            slope,
            curve,
            tail,
            far,
        })
    }

    /// The change of a held band of a section against the output loudness
    /// at the input change `x` of peak control against its ceiling, from
    /// its changes `down` and `up` at the drive `shift` down and up, `free`,
    /// its change without peak control (D282), and `far`, its change at the
    /// drive of the far model (D283). From the drive `shift` down upward,
    /// the quadratic through the two changes, and over the drive `shift` up,
    /// the quadratic through the changes up and far. Under the drive
    /// `shift` down, the change left to `free` falls with the loss of peak
    /// control.
    fn section_change_db(&self, down: f64, up: f64, free: f64, far: Option<f64>, x: f64) -> f64 {
        if let (Some((at, _, _)), Some(far), true) = (self.far, far, x > self.shift) {
            let (slope, curve) = through(self.shift, up, at, far);
            return slope * x + curve * x * x;
        }
        if x >= -self.shift {
            let slope = (up - down) / (2.0 * self.shift);
            let curve = (up + down) / (2.0 * self.shift * self.shift);
            return slope * x + curve * x * x;
        }
        match self.tail {
            Some((_, rate)) => free + (down - free) * (rate * (x + self.shift)).exp(),
            None => down,
        }
    }
}

/// The slope and the curve of the quadratic through 0 at 0, `a` at `x` and
/// `b` at `y`.
fn through(x: f64, a: f64, y: f64, b: f64) -> (f64, f64) {
    let curve = (b / y - a / x) / (y - x);
    (a / x - curve * x, curve)
}

/// The drive change of peak control that moves its output loudness by `lu`
/// at a fixed ceiling, from `response`, the drive response of the render
/// (D281, D283, `Response`). Without a response, the output follows the
/// drive dB for dB.
fn drive_change_db(response: &Value, lu: f64) -> f64 {
    let Some(Response {
        shift,
        up,
        down,
        loss,
        slope,
        curve,
        tail,
        far,
    }) = Response::of(response)
    else {
        return lu;
    };
    let (slope, curve) = match far {
        Some((_, far_slope, far_curve)) if lu > up => (far_slope, far_curve),
        _ => (slope, curve),
    };
    if lu < down {
        let Some((left, rate)) = tail else {
            return lu - down - shift;
        };
        let change = |x: f64| x + loss - left * (rate * (x + shift)).exp();
        let (mut low, mut high) = (lu - loss - 1.0, -shift);
        for _ in 0..100 {
            let middle = 0.5 * (low + high);
            if change(middle) < lu {
                low = middle;
            } else {
                high = middle;
            }
        }
        return 0.5 * (low + high);
    }
    // The root of curve x^2 + slope x = lu next to lu / slope. Past the top
    // of the curve, the drive of the top.
    let discriminant = slope * slope + 4.0 * curve * lu;
    if discriminant < 0.0 {
        return -slope / (2.0 * curve);
    }
    2.0 * lu / (slope + discriminant.sqrt())
}

/// The drive of the render of `spec` on the prefix of a parent render of
/// `delivered` at `drive_db`, with `prefix_db`, the loudness change of the
/// prefix, and `missed_lu`, the output loudness of the parent render less
/// its target. Peak control scales with its ceiling: a ceiling change moves
/// the output by the same change at the drive that moves by it, so the
/// drive solves the target change less the ceiling change at the parent's
/// ratio of drive to ceiling (D281). The drive response is the response at
/// the output of the parent, so the target change starts from that output
/// (D289).
fn solved_drive_db(
    delivered: &Spec,
    spec: &Spec,
    drive_db: f64,
    prefix_db: f64,
    missed_lu: f64,
    response: &Value,
) -> f64 {
    let ceiling_db = spec.ceiling_dbtp - delivered.ceiling_dbtp;
    let target_db = spec.target_lufs - delivered.target_lufs - missed_lu;
    drive_db + ceiling_db - prefix_db + drive_change_db(response, target_db - ceiling_db)
}

/// The output loudness of the render of `parent` less the target of its
/// drive search, or 0 without them (D289).
fn missed_lu(parent: &Value) -> f64 {
    let report = render_report(parent);
    match (
        report["lufs"].as_f64(),
        report["search"]["target_lufs"].as_f64(),
    ) {
        (Some(lufs), Some(target)) => lufs - target,
        _ => 0.0,
    }
}

/// The hold of the preview of `spec` on `parent`, whose delivered spec is
/// `delivered`: the hold steps of the final render (`predicted_hold`) from
/// the sections that the parent stores, or the hold of the parent.
fn request_hold(
    parent: &Value,
    delivered: &Spec,
    spec: &Spec,
    restored: &Restored,
    facts: &Facts,
) -> Result<Option<crate::low_end::Hold>> {
    let Some(sections) = HoldSections::of_record(parent) else {
        return Ok(delivered.low_end_hold.clone());
    };
    let frames = parent["delivery"]["frames"]
        .as_u64()
        .or_else(|| parent["verification"]["frames"].as_u64())
        .ok_or("Missing frame count")? as usize;
    let report = render_report(parent);
    let compression =
        compression_hold_db(delivered, spec, &report["compressor"]).unwrap_or_default();
    let response = &report["drive_response"];
    let peak_control = peak_control_hold_db(delivered, spec, response);
    let (spectrum, count) = (facts.spectrum.spectrum(), sections.free_change_db.len());
    let request = HoldPasses::of(
        &delivered.restoration_db,
        &restored.start,
        &restored.passes,
        response,
        &spectrum,
        count,
    );
    // The render of the parent: its first render, and one pass to its
    // delivered gains.
    let parent_first: Option<[f64; 6]> =
        serde_json::from_value(report["restoration"]["measured_gains_db"].clone()).ok();
    let parent_passes = match parent_first.filter(|g| *g != delivered.restoration_db) {
        Some(first) => HoldPasses::of(
            &delivered.restoration_db,
            &first,
            &[delivered.restoration_db],
            response,
            &spectrum,
            count,
        ),
        None => HoldPasses::default(),
    };
    let take = shelf_take_db(
        response,
        &shaped_spectrum(&facts.spectrum, delivered),
        count,
    );
    Ok(predicted_hold(
        delivered,
        spec,
        &sections,
        &compression,
        &peak_control,
        &request,
        &parent_passes,
        // The renders of the parent after its first render.
        report["restoration"]["renders"]
            .as_u64()
            .map(|r| (r as usize).saturating_sub(1)),
        &take,
        facts,
        frames,
    ))
}

/// The tone of `spec` for the loudness of its prefix: the mean tone, with
/// each hold bell at its mean wet in each section of `sections`, weighted by
/// the power of its held band in the section. A bell in a quiet section
/// adds less loudness than its share of the time: on track-g, Radio holds
/// the outro, whose bass is 15 dB under that of the other sections (D282).
/// Without the powers, the mean tone.
fn loudness_tone(spec: &Spec, sections: Option<&HoldSections>) -> Tone {
    let Some(sections) = sections.filter(|s| s.power_db.len() == s.starts.len()) else {
        return spec.mean_tone();
    };
    let mut spec = spec.clone();
    for band in spec.low_end_hold.iter_mut().flat_map(|h| &mut h.bands) {
        let Some(f) = (0..crate::low_end::BANDS_HZ.len())
            .find(|f| crate::low_end::hold_band(*f).hz == band.band.hz)
        else {
            continue;
        };
        let (mut wet, mut weight) = (0.0, 0.0);
        for (s, power) in sections.power_db.iter().enumerate() {
            let energy = 10f64.powf(power[f] / 10.0);
            wet += energy * band.mean_wet(sections.starts[s], sections.ends[s]);
            weight += energy;
        }
        if weight > 0.0 {
            band.points = vec![crate::treatment::BlendPoint {
                sample: 0,
                wet: wet / weight,
            }];
        }
    }
    spec.mean_tone()
}

/// The loudness that the restoring gains of `spec` add to its prefix
/// against the restoring gains of `delivered`, from the stored spectrum
/// after the tone of `spec` for its loudness (`loudness_tone`, D284). The
/// loudness of the tone takes the spectrum without the restoring filters:
/// with the filters of the parent, the preview drive of Bass −2 rises by
/// 0.019 to 0.022 dB against the final drive on track-c and track-g.
fn restoration_loudness_db(
    spectrum: &BandSpectrum,
    spec: &Spec,
    delivered: &Spec,
    sections: Option<&HoldSections>,
) -> f64 {
    if spec.restoration_db == delivered.restoration_db {
        return 0.0;
    }
    let tone = loudness_tone(spec, sections);
    let loudness = |gains: &[f64; 6]| {
        let mut tone = tone.clone();
        tone.stereo
            .extend(crate::treatment_render::filters_of(gains));
        predicted_loudness_db(spectrum, &tone, spec.side_gain.as_ref())
    };
    loudness(&spec.restoration_db) - loudness(&delivered.restoration_db)
}

/// The preview of a preset on `parent`: the first candidate of the preset,
/// with the restoring gains of `predicted_restoration` and the hold of
/// `request_hold`, as `preview`.
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
    let response = &render_report(parent)["drive_response"];
    let restored = predicted_restoration(
        parent,
        &delivered,
        input_change_db(&delivered, &spec, response),
        tone_shelves(&delivered, &spec, response, facts),
        facts.spectrum.sample_rate,
    );
    spec.restoration_db = restored.gains();
    spec.low_end_hold = request_hold(parent, &delivered, &spec, &restored, facts)?;
    // The drive follows the level and the ceiling of the preset, less the
    // predicted change of the loudness of the prefix, as in `preview`.
    let sections = HoldSections::of_record(parent);
    let loudness = |s: &Spec| {
        predicted_loudness_db(
            &facts.spectrum,
            &loudness_tone(s, sections.as_ref()),
            s.side_gain.as_ref(),
        )
    };
    let drive_db = solved_drive_db(
        &delivered,
        &spec,
        fixed.drive_db,
        loudness(&spec) - loudness(&delivered)
            + restoration_loudness_db(&facts.spectrum, &spec, &delivered, sections.as_ref()),
        missed_lu(parent),
        &render_report(parent)["drive_response"],
    );
    Ok((spec, crate::excerpt::Fixed { drive_db, ..fixed }))
}

/// The spec and the fixed state of the preview of `steps` on `parent`, a
/// delivered version record.
/// The preview keeps the true-peak trim of the parent. It predicts the
/// three things that the final render solves again from the stored
/// spectrum and the drive response of the parent:
///
/// - The restoring gains: the restoration passes of the final render from
///   the local band change of peak control at the drive change
///   (`predicted_restoration`, D284).
/// - The low-end hold: the hold steps of the final render (`predicted_hold`) from
///   the sections that the parent stores, with the change of the compressor
///   in each section (D280), the side share of each section and the change
///   of peak control in each section (D282), and the change of the
///   restoring gains (D284), or the hold of the parent.
/// - The drive: the drive of the parent with the change of the target and of
///   the ceiling, less the predicted change of the loudness of the prefix:
///   the tone with the hold and the restoring filters, the width, and the
///   change of the compressor reduction. The drive response of the parent gives the drive change of
///   a loudness change (`solved_drive_db`, D281).
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
    let response = &render_report(parent)["drive_response"];
    let restored = predicted_restoration(
        parent,
        &delivered,
        input_change_db(&delivered, &spec, response),
        tone_shelves(&delivered, &spec, response, facts),
        spectrum.sample_rate,
    );
    spec.restoration_db = restored.gains();
    spec.low_end_hold = request_hold(parent, &delivered, &spec, &restored, facts)?;
    let sections = HoldSections::of_record(parent);
    let loudness = |s: &Spec| {
        predicted_loudness_db(
            spectrum,
            &loudness_tone(s, sections.as_ref()),
            s.side_gain.as_ref(),
        )
    };
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
    // A change of the prefix loudness acts as a change of the drive.
    let prefix_db = loudness(&spec) - loudness(&delivered)
        + restoration_loudness_db(spectrum, &spec, &delivered, sections.as_ref())
        - compression_db;
    let drive_db = solved_drive_db(
        &delivered,
        &spec,
        fixed.drive_db,
        prefix_db,
        missed_lu(parent),
        &render_report(parent)["drive_response"],
    );
    Ok((spec, crate::excerpt::Fixed { drive_db, ..fixed }))
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

/// The reference record of an earlier manifest (`adjust.record`) or of the
/// family result of `essential` (`reference_record`).
fn earlier_record(versions: &Value) -> Option<&Value> {
    [&versions["adjust"]["record"], &versions["reference_record"]]
        .into_iter()
        .find(|r| !r.is_null())
}

/// Whether `versions` holds a reference record for the requests on it: in
/// its Essential version record (D270), in `adjust.record` or in
/// `reference_record`. A request on a versions record without one renders
/// the `essential` spec as its reference.
pub fn holds_reference(versions: &Value) -> bool {
    earlier_record(versions).is_some()
        || versions["versions"]
            .as_array()
            .into_iter()
            .flatten()
            .any(|v| v["id"] == "essential" && !v[VERSION_REFERENCE_FIELD].is_null())
}

/// Render a request as version `id` into `output_dir`, from the base of the
/// set and the record of its delivered versions: a set manifest, or a
/// family result whose versions carry no stored facts. The facts of a
/// version without them come from its WAV beside the versions record. A
/// base without a reference record takes the record of the versions record:
/// the Essential version record of a cloud manifest (D270), `adjust.record`
/// of an earlier manifest, or `reference_record` of the family result of
/// `essential`. `input` is the source at the render rate of the set.
pub fn render(
    input: crate::treatment_render::RenderSource<'_>,
    base: &Base,
    versions: &Value,
    versions_dir: &std::path::Path,
    request: &Request,
    output_dir: &std::path::Path,
) -> Result<Value> {
    // The specs of a cloud manifest share their keys (D276). The request
    // puts the values back in the records that it reads: the parent, the
    // Essential version and the reference record (D278).
    let shared = crate::distributed::shared_values(versions);
    let expand = |record: &Value| crate::distributed::expand_record(record, shared);
    let parent = &expand(parent_record(versions, &request.parent)?)?;
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
    let essential = versions["versions"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|v| v["id"] == "essential")
        .map(expand)
        .collect::<Result<Vec<_>>>()?;
    let mut base = base.clone().with_versions_record(&essential);
    drop(essential);
    if base.record.is_none() {
        base.record = earlier_record(versions).map(expand).transpose()?;
    }
    let (result, plan) = render_in(
        input,
        &base,
        parent,
        &parent_facts,
        &others,
        request,
        &mut crate::distributed::Directory(output_dir),
    )?;
    std::fs::write(
        output_dir.join("adjust.plan.json"),
        serde_json::to_vec_pretty(&plan)?,
    )?;
    Ok(result)
}

/// Render `request` on `parent` as one version into `store`, and finish it
/// (`finalize`). Returns the result and the worker plan of the request.
/// The family takes its reference from the reference record of the base
/// when the record is of its source and its reference spec (D217).
pub fn render_in(
    input: crate::treatment_render::RenderSource<'_>,
    base: &Base,
    parent: &Value,
    parent_facts: &VersionFacts,
    others: &[Other],
    request: &Request,
    store: &mut impl crate::distributed::Store,
) -> Result<(Value, Value)> {
    render_continued(
        input,
        base,
        parent,
        parent_facts,
        others,
        request,
        store,
        None,
        None,
    )
    .map(|(result, plan, _)| (result, plan))
}

/// `render_in` whose candidate ladders keep to `deadline`, and that
/// continues the request of `earlier` (D258). A version that stops at the
/// deadline gives the result `time-limit` and the record of its
/// continuation (`distributed::Resume`).
#[allow(clippy::too_many_arguments)]
pub fn render_continued(
    mut input: crate::treatment_render::RenderSource<'_>,
    base: &Base,
    parent: &Value,
    parent_facts: &VersionFacts,
    others: &[Other],
    request: &Request,
    store: &mut impl crate::distributed::Store,
    deadline: Option<std::time::Instant>,
    earlier: Option<&crate::distributed::Resume>,
) -> Result<(Value, Value, Option<crate::distributed::Resume>)> {
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
                None,
            ))
        }
    };
    let plan_json = serde_json::to_vec(&plan)?;
    // The family and the measures of the result read the source in turn
    // (D257).
    let (family, resume) = crate::distributed::render_family_continued(
        input.lend(),
        base.sample_rate(),
        &plan_json,
        &request.id,
        &mut WithReference {
            store,
            record: base.record.as_ref().map(serde_json::to_vec).transpose()?,
        },
        deadline,
        earlier,
        |_, _| Ok(()),
    )?;
    // A version that stopped at the deadline chose no candidate; another
    // invocation continues it from the record.
    if let (true, Some(resume)) = (family.versions.is_empty(), resume) {
        return Ok((
            json!({
                "schema": RESULT_SCHEMA,
                "rule": RULE,
                "parent": request.parent,
                "id": request.id,
                "steps": request.steps,
                "preset": request.preset,
                "delivered": false,
                "reason": "time-limit",
                "family": family,
            }),
            plan,
            Some(resume),
        ));
    }
    let result = finalize(
        &mut input,
        &base.facts,
        parent,
        parent_facts,
        others,
        request,
        &family,
        store,
    )?;
    Ok((result, plan, None))
}

/// The files of a request in `store`, with the reference record of its set.
struct WithReference<'a, S> {
    store: &'a mut S,
    record: Option<Vec<u8>>,
}

impl<S: crate::distributed::Store> crate::distributed::Store for WithReference<'_, S> {
    fn deliver(
        &mut self,
        name: &str,
        audio: &crate::treatment_render::RenderAudio,
        rate: u32,
        seed: u64,
        ceiling_dbtp: f64,
    ) -> Result<(String, crate::delivery::Verification)> {
        self.store.deliver(name, audio, rate, seed, ceiling_dbtp)
    }

    fn raw(
        &mut self,
        name: &str,
        audio: &crate::treatment_render::RenderAudio,
        rate: u32,
    ) -> Result<String> {
        self.store.raw(name, audio, rate)
    }

    fn read(&mut self, name: &str, rate: u32) -> Result<Vec<Stereo>> {
        self.store.read(name, rate)
    }

    fn read_stored(&mut self, name: &str, rate: u32) -> Result<crate::audio_io::StoredSource> {
        self.store.read_stored(name, rate)
    }

    fn scratch(&self) -> crate::scratch::Factory {
        self.store.scratch()
    }

    fn directory(&self) -> Option<&std::path::Path> {
        self.store.directory()
    }

    fn reference(&mut self) -> Result<Option<Vec<u8>>> {
        Ok(self.record.clone())
    }
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
        compressor: None,
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
        // Streaming and Radio move its level and its ceiling. The widening
        // of 1.5 dB on a side 10 dB under the mid moves the mono sum by less
        // than 0.5 dB.
        for (preset, offered) in [("streaming", true), ("radio", true), ("mono", false)] {
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
    fn the_range_leaves_out_a_step_that_moves_no_further() {
        let plan: Value = serde_json::from_str(include_str!("../tests/data/adjust-parent.json"))
            .expect("parent spec");
        let settings: crate::compressor::Settings =
            serde_json::from_value(plan["compressor"].clone()).unwrap();
        // A parent that reduces by 1.4 dB: three steps down are outside the
        // range of the compressor at the full share, and four steps down at
        // the full share and at three quarters.
        let over_db: [f64; crate::compressor::OVER_QUANTILES] =
            std::array::from_fn(|i| -8.0 + 0.6 * i as f64);
        let stats = json!({"over_quantiles_db": over_db, "mean_reduction_db": 1.4});
        let parent = json!({"spec": plan,
            "render": {"compressor": stats, "deesser": deesser_stats()}});
        let facts = Facts {
            rule: FACTS_RULE.into(),
            spectrum: spectrum(),
            audibility_lufs: None,
            bandwidth_hz: 24_000.0,
            mix: signature(-14.0, 1.0),
            mono_safe: None,
        };
        let compression = Compression::of(&parent["render"]["compressor"]).unwrap();
        assert!(compression_shift(&compression, settings, -1.5).is_none());
        assert!(compression_shift(&compression, settings, -1.5 * 0.75).is_some());
        let r = range(&parent, &facts).unwrap();
        assert_eq!(r["compression"]["-2"], 1.0);
        assert_eq!(r["compression"]["-3"], 0.75);
        // Half of four steps down is two steps, which the step of two
        // steps down gives.
        assert!(r["compression"]["-4"].is_null());
        for step in 1..=MAXIMUM_STEPS {
            assert_eq!(r["compression"][step.to_string()], 1.0, "{step}");
        }
        assert_eq!(r["ceiling"]["-3"], 1.0);
    }

    /// The stored sections count the low shelves of a version against those
    /// of the reference, as the low-end hold does (D280).
    #[test]
    fn the_hold_sections_count_the_shelves_against_the_reference() {
        let mut spec = parent();
        spec.low_end_hold = None;
        spec.tone.stereo.push(bass_shelf(3.0));
        let full = spectrum().spectrum();
        let sections = |power: f64| crate::low_end::Sections {
            starts: vec![0],
            ends: vec![720_000],
            power_db: vec![[power, power]],
            side_db: Vec::new(),
            low_db: Vec::new(),
        };
        let of = |tone: Option<&crate::treatment::Tone>| {
            HoldSections::of(
                &spec,
                &full,
                &[],
                &sections(1.0),
                -10.0,
                &sections(0.0),
                -10.0,
                tone,
            )
        };
        // With the shelves of the reference, the rise of the bands is the
        // change.
        let held = of(Some(&spec.tone));
        assert!((held.free_change_db[0][0].unwrap() - 1.0).abs() < 1e-9);
        assert_eq!(held.change_db, held.free_change_db);
        // Without them, the shelves of the version are intended.
        let intended = crate::low_end::intended_db(&spec, &full);
        assert!(intended[0] > 1.0, "{intended:?}");
        let alone = of(None);
        assert!((alone.free_change_db[0][0].unwrap() - (1.0 - intended[0])).abs() < 1e-9);
    }

    /// A Compression step down that lowers the held bands of a section past
    /// the limit of the hold gives the preview a hold in that section, and
    /// a step up gives none (D280).
    #[test]
    fn the_preview_of_a_compression_step_takes_the_hold_of_each_section() {
        let mut plan: Value =
            serde_json::from_str(include_str!("../tests/data/adjust-parent.json"))
                .expect("parent spec");
        plan["low_end_hold"] = Value::Null;
        let over_db: [f64; crate::compressor::OVER_QUANTILES] =
            std::array::from_fn(|i| -8.0 + 0.6 * i as f64);
        let span = crate::low_end::section_frames(48_000);
        let sections = HoldSections {
            starts: (0..4).map(|s| s * span).collect(),
            ends: (1..=4).map(|s| s * span).collect(),
            change_db: vec![[Some(-0.3), Some(0.0)]; 4],
            free_change_db: vec![[Some(-0.3), Some(0.0)]; 4],
            side_share: Vec::new(),
            power_db: Vec::new(),
            low_db: Vec::new(),
        };
        // Less reduction lowers the first band of the first section, and
        // more reduction raises it.
        let mut change_db = vec![[[0.0; 2]; 2]; 4];
        change_db[0] = [[-0.4, 0.0], [0.4, 0.0]];
        let shift = crate::compressor::HoldShift {
            shift_db: crate::compressor::HOLD_SHIFT_DB,
            reduction_db: [-0.8, 0.8],
            change_db,
        };
        let record = |shift: Option<&crate::compressor::HoldShift>| {
            json!({"spec": plan, "hold_sections": sections, "delivery": {"frames": 4 * span},
                "render": {"drive_db": 3.0, "true_peak": {"trim_db": -0.1},
                    "deesser": deesser_stats(),
                    "compressor": {"over_quantiles_db": over_db, "mean_reduction_db": 1.4,
                        "hold_shift": shift}}})
        };
        let facts = preset_facts(None);
        let steps = |compression| Steps {
            compression,
            ..Steps::default()
        };
        let delivered = crate::excerpt::delivered_spec(&record(Some(&shift))).unwrap();
        let (down, _) = preview(&record(Some(&shift)), &steps(-2), &facts).unwrap();
        let moved = compression_hold_db(
            &delivered,
            &down,
            &record(Some(&shift))["render"]["compressor"],
        )
        .unwrap();
        assert!(moved[0][0] < -0.2, "{moved:?}");
        assert_eq!(moved[3], [0.0, 0.0]);
        let hold = down.low_end_hold.expect("a hold");
        let bell = &hold.bands[0];
        assert!(bell.mean_wet(0, span) > 0.0);
        assert_eq!(bell.mean_wet(3 * span, 4 * span), 0.0);
        let (up, _) = preview(&record(Some(&shift)), &steps(2), &facts).unwrap();
        assert!(up.low_end_hold.is_none());
        // Without the hold shift, the preview keeps the hold of the parent.
        let (down, _) = preview(&record(None), &steps(-2), &facts).unwrap();
        assert!(down.low_end_hold.is_none());
    }

    /// The record of the parent fixture with four sections of free changes
    /// of 0.2 dB, whose hold sections and drive response `with` sets.
    fn sectioned_parent(plan: &Value, with: impl Fn(&mut HoldSections, &mut Value)) -> Value {
        let span = crate::low_end::section_frames(48_000);
        let mut sections = HoldSections {
            starts: (0..4).map(|s| s * span).collect(),
            ends: (1..=4).map(|s| s * span).collect(),
            change_db: vec![[Some(0.2), Some(0.2)]; 4],
            free_change_db: vec![[Some(0.2), Some(0.2)]; 4],
            side_share: Vec::new(),
            power_db: Vec::new(),
            low_db: Vec::new(),
        };
        let mut response =
            json!({"shift_db": 1.0, "up_lu": 0.86, "down_lu": -0.91, "loss_lu": 0.3});
        with(&mut sections, &mut response);
        json!({"spec": plan, "hold_sections": sections, "delivery": {"frames": 4 * span},
            "render": {"drive_db": 3.0, "true_peak": {"trim_db": -0.1},
                "deesser": deesser_stats(), "drive_response": response}})
    }

    #[test]
    fn the_section_change_follows_the_drive_response() {
        let response = Response::of(&json!({"shift_db": 1.0, "up_lu": 0.86,
            "down_lu": -0.91, "loss_lu": 0.3}))
        .unwrap();
        let change = |x| response.section_change_db(0.2, -0.25, 0.5, None, x);
        // The quadratic through the changes at the drive 1 dB down and up.
        assert!((change(1.0) + 0.25).abs() < 1e-12);
        assert!((change(-1.0) - 0.2).abs() < 1e-12);
        assert_eq!(change(0.0), 0.0);
        assert!((change(2.0) - (2.0 * -0.225 + 4.0 * -0.025)).abs() < 1e-12);
        // Under 1 dB down, the change moves from the change at 1 dB down to
        // the change without peak control, as the loss of peak control falls.
        assert!((change(-1.0 - 1e-9) - 0.2).abs() < 1e-6);
        let (a, b) = (change(-3.0), change(-6.0));
        assert!(a > 0.2 && b > a && b < 0.5, "{a} {b}");
        assert!((change(-200.0) - 0.5).abs() < 1e-6);
        // With the far model, over 1 dB up, the quadratic through the changes
        // at 1 dB and at 3 dB up. Under 1 dB up, the far change counts for
        // nothing.
        let far = Response::of(&json!({"shift_db": 1.0, "up_lu": 0.86,
            "down_lu": -0.91, "loss_lu": 0.3, "far_db": 3.0, "far_lu": 2.3}))
        .unwrap();
        let change = |x| far.section_change_db(0.2, -0.25, 0.5, Some(-0.6), x);
        assert!((change(3.0) + 0.6).abs() < 1e-12);
        assert!((change(1.0) + 0.25).abs() < 1e-12);
        let (slope, curve) = through(1.0, -0.25, 3.0, -0.6);
        assert!((change(2.0) - (2.0 * slope + 4.0 * curve)).abs() < 1e-12);
        assert!(
            change(2.0) > -0.25 * 2.0 - 0.025 * 4.0 + 0.05,
            "{}",
            change(2.0)
        );
        assert_eq!(
            change(0.5),
            response.section_change_db(0.2, -0.25, 0.5, None, 0.5)
        );
        assert_eq!(
            change(-3.0),
            response.section_change_db(0.2, -0.25, 0.5, None, -3.0)
        );
    }

    #[test]
    fn the_preview_of_a_preset_takes_the_peak_control_of_each_section() {
        let mut plan: Value =
            serde_json::from_str(include_str!("../tests/data/adjust-parent.json"))
                .expect("parent spec");
        plan["low_end_hold"] = Value::Null;
        let span = crate::low_end::section_frames(48_000);
        let facts = preset_facts(Some(-7.0));
        // Peak control leaves the last section, a quiet outro, as it is:
        // without peak control, it is 1.5 dB quieter against the loudness.
        // Streaming takes most of the loss of peak control away, and the
        // outro falls under the limit of the hold.
        let record = sectioned_parent(&plan, |s, response| {
            response["sections_down_db"] =
                json!([[0.1, 0.1], [0.1, 0.1], [0.1, 0.1], [-0.6, -0.6]]);
            response["sections_up_db"] =
                json!([[-0.1, -0.1], [-0.1, -0.1], [-0.1, -0.1], [0.6, 0.6]]);
            response["sections_free_db"] =
                json!([[0.2, 0.2], [0.2, 0.2], [0.2, 0.2], [-1.5, -1.5]]);
            s.power_db = vec![[0.0, 0.0], [0.0, 0.0], [0.0, 0.0], [-20.0, -20.0]];
        });
        let delivered = crate::excerpt::delivered_spec(&record).unwrap();
        let (spec, _) = preview_preset(&record, Preset::Streaming, &facts).unwrap();
        let moved = peak_control_hold_db(&delivered, &spec, &record["render"]["drive_response"]);
        assert!(moved[3][0] < -0.7 && moved[3][0] > -1.5, "{moved:?}");
        assert!(moved[0][0] > 0.1 && moved[0][0] < 0.2, "{moved:?}");
        let hold = spec.low_end_hold.as_ref().expect("a hold");
        let bell = &hold.bands[0];
        assert_eq!(bell.mean_wet(0, span), 0.0);
        assert!(bell.mean_wet(3 * span, 4 * span) > 0.2);
        // The bell of the outro adds the loudness of the bass of the outro,
        // 20 dB under that of the other sections, and not its share of the
        // time.
        let gain = |tone: &Tone| {
            tone.stereo
                .iter()
                .find(|b| b.hz == crate::low_end::hold_band(0).hz)
                .map_or(0.0, |b| b.gain_db)
        };
        let sections = HoldSections::of_record(&record).unwrap();
        let wet = (0..4)
            .map(|s| {
                10f64.powf(sections.power_db[s][0] / 10.0)
                    * bell.mean_wet(sections.starts[s], sections.ends[s])
            })
            .sum::<f64>()
            / (3.0 + 0.01);
        let (weighted, timed) = (
            gain(&loudness_tone(&spec, Some(&sections))),
            gain(&spec.mean_tone()),
        );
        assert!((weighted - bell.band.gain_db * wet).abs() < 1e-9);
        assert!(timed > 0.1 && weighted < 0.2 * timed, "{weighted} {timed}");
        // Without the sections of the drive response, no section falls.
        let record = sectioned_parent(&plan, |_, _| {});
        let (spec, _) = preview_preset(&record, Preset::Streaming, &facts).unwrap();
        assert!(spec.low_end_hold.is_none());
    }

    #[test]
    fn the_preview_of_a_fold_takes_the_side_share_of_each_section() {
        let mut plan: Value =
            serde_json::from_str(include_str!("../tests/data/adjust-parent.json"))
                .expect("parent spec");
        plan["low_end_hold"] = Value::Null;
        let span = crate::low_end::section_frames(48_000);
        let facts = preset_facts(Some(-7.0));
        // The parent widens 120 Hz and up. The fold takes the side gain back,
        // and the last section holds 90% of its low mids in the side.
        plan["side_gain"]["bands_db"] = json!([0.0, 4.0, 4.0, 4.0, 4.0, 4.0]);
        let record = sectioned_parent(&plan, |s, _| {
            s.side_share = vec![[0.0, 0.0], [0.0, 0.0], [0.0, 0.0], [0.0, 0.9]];
        });
        let (spec, _) = preview_preset(&record, Preset::Mono, &facts).unwrap();
        let hold = spec.low_end_hold.as_ref().expect("a hold");
        let bell = hold
            .bands
            .iter()
            .find(|b| b.band.hz == crate::low_end::hold_band(1).hz)
            .expect("a bell of the low mids");
        assert_eq!(bell.mean_wet(0, span), 0.0);
        assert!(bell.mean_wet(3 * span, 4 * span) > 0.2);
        // With the side share of the mix, each section takes one change.
        let record = sectioned_parent(&plan, |_, _| {});
        let (spec, _) = preview_preset(&record, Preset::Mono, &facts).unwrap();
        assert!(spec.low_end_hold.is_none());
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
    fn a_versions_record_holds_the_reference_of_each_form() {
        let record = json!({"schema": crate::distributed::REFERENCE_SCHEMA});
        assert!(!holds_reference(&json!({"versions": [{"id": "master"}]})));
        assert!(!holds_reference(&json!({
            "versions": [{"id": "master", "reference_record": record}]
        })));
        assert!(holds_reference(&json!({
            "versions": [{"id": "essential", "reference_record": record}]
        })));
        assert!(holds_reference(&json!({"adjust": {"record": record}})));
        assert!(holds_reference(&json!({"reference_record": record})));
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
    fn the_drive_change_follows_the_drive_response() {
        let response = json!({"shift_db": 1.0, "up_lu": 0.86, "down_lu": -0.91, "loss_lu": 0.3});
        assert!((drive_change_db(&response, 0.86) - 1.0).abs() < 1e-9);
        assert!((drive_change_db(&response, -0.91) + 1.0).abs() < 1e-9);
        assert_eq!(drive_change_db(&Value::Null, 0.7), 0.7);
        let even = json!({"shift_db": 1.0, "up_lu": 1.0, "down_lu": -1.0, "loss_lu": 0.0});
        assert!((drive_change_db(&even, 0.7) - 0.7).abs() < 1e-12);
        assert!((drive_change_db(&even, -6.0) + 6.0).abs() < 1e-12);
        // Past the top of the curve, the drive of the top.
        let flat = json!({"shift_db": 1.0, "up_lu": 0.2, "down_lu": -0.9, "loss_lu": 0.3});
        assert!((drive_change_db(&flat, 0.5) - 0.55 / 0.7).abs() < 1e-9);
        // Under the drive 1 dB down, the loss of 0.21 LU left there falls
        // away: the curve joins the quadratic, and a large fall gives back
        // nearly the whole loss of 0.3 LU.
        assert!((drive_change_db(&response, -0.91 - 1e-9) + 1.0).abs() < 1e-6);
        let x = drive_change_db(&response, -10.0);
        let rate = (1.0 - (0.885 + 0.05)) / 0.21;
        let change = x + 0.3 - 0.21 * (rate * (x + 1.0)).exp();
        assert!((change + 10.0).abs() < 1e-9, "{x}");
        assert!(x < -10.28 && x > -10.3, "{x}");
        // Over the change at the drive 1 dB up, the quadratic through the
        // changes at 1 dB and at 3 dB up; past its top, the drive of the top.
        let far = json!({"shift_db": 1.0, "up_lu": 0.86, "down_lu": -0.91, "loss_lu": 0.3,
            "far_db": 3.0, "far_lu": 2.3});
        assert!((drive_change_db(&far, 0.86) - 1.0).abs() < 1e-9);
        assert!((drive_change_db(&far, 2.3) - 3.0).abs() < 1e-9);
        assert_eq!(drive_change_db(&far, 0.5), drive_change_db(&response, 0.5));
        assert_eq!(
            drive_change_db(&far, -10.0),
            drive_change_db(&response, -10.0)
        );
        let x = drive_change_db(&far, 1.6);
        let (slope, curve) = through(1.0, 0.86, 3.0, 2.3);
        assert!((slope * x + curve * x * x - 1.6).abs() < 1e-9, "{x}");
        let falling = json!({"shift_db": 1.0, "up_lu": 0.86, "down_lu": -0.91, "loss_lu": 0.3,
            "far_db": 3.0, "far_lu": 0.9});
        assert!((drive_change_db(&falling, 1.5) - 1.14 / 0.56).abs() < 1e-9);
    }

    #[test]
    fn the_preview_replays_the_restoration_passes_of_the_drive_change() {
        let plan: Value = serde_json::from_str(include_str!("../tests/data/adjust-parent.json"))
            .expect("parent spec");
        let mut response = json!({"shift_db": 1.0, "up_lu": 0.86, "down_lu": -0.91,
            "loss_lu": 0.3, "far_db": 3.0, "far_lu": 2.3,
            "local_down_db": [0.3, 0.0, 0.0, 0.0, 0.0, 0.0],
            "local_up_db": [-0.6, 0.0, 0.0, 0.0, 0.0, 0.0],
            "local_far_db": [-1.4, 0.0, 0.0, 0.0, 0.0, 0.0],
            "local_free_db": [0.8, 0.0, 0.0, 0.0, 0.0, 0.0],
            "shelf_db": 1.0, "shelf_lu": 0.25,
            "shelf_local_db": [0.4, -0.2, -0.2, -0.2, -0.2, -0.2]});
        let record = |response: &Value| {
            json!({"spec": plan, "render": {"drive_db": 3.0, "true_peak": {"trim_db": -0.1},
                "deesser": deesser_stats(), "drive_response": response,
                "restoration": {"delivered_change_db": [0.05, 0.0, 0.0, 0.0, 0.0, 0.0],
                    "material": [true, false, false, false, false, false],
                    "gains_db": [0.2, 0.0, 0.0, 0.0, 0.0, 0.0]}}})
        };
        let parent = record(&response);
        let delivered = crate::excerpt::delivered_spec(&parent).unwrap();
        assert_eq!(delivered.restoration_db[0], 0.2);
        // At the drive, the change of the parent stays inside the dead zone.
        assert_eq!(
            predicted_restoration(&parent, &delivered, 0.0, 0.0, 48000),
            Restored {
                start: delivered.restoration_db,
                passes: Vec::new()
            }
        );
        // At 1 dB up, the first render loses 0.55 dB of 20–120 Hz. Each pass
        // returns the change less the dead zone, and its step returns the
        // change of the shelf model.
        let r00 = crate::treatment_render::restoring_response_db(0, 0, 48000);
        let first = (0.55 - 0.1) / r00;
        let second = (0.55 - 0.4 * first - 0.1) / r00;
        let restored = predicted_restoration(&parent, &delivered, 1.0, 0.0, 48000);
        assert_eq!(restored.start, delivered.restoration_db);
        assert_eq!(restored.passes.len(), 2);
        assert!((restored.passes[0][0] - (0.2 + first)).abs() < 1e-9);
        let gains = restored.gains();
        assert!(
            (gains[0] - (0.2 + first + second)).abs() < 1e-9,
            "{gains:?}"
        );
        assert_eq!(gains[1..], [0.0; 5]);
        // A version after Essential starts from the gains of its first
        // render and the change that it measured; Essential starts from its
        // delivered gains.
        let mut after = parent.clone();
        after["id"] = json!("master");
        after["render"]["restoration"]["measured_gains_db"] = json!([0.1, 0.0, 0.0, 0.0, 0.0, 0.0]);
        after["render"]["restoration"]["measured_change_db"] =
            json!([-0.02, 0.0, 0.0, 0.0, 0.0, 0.0]);
        assert_eq!(
            predicted_restoration(&after, &delivered, 0.0, 0.0, 48000).gains(),
            [0.1, 0.0, 0.0, 0.0, 0.0, 0.0]
        );
        let from_first = predicted_restoration(&after, &delivered, 1.0, 0.0, 48000).gains();
        let first = (0.62 - 0.1) / r00;
        let second = (0.62 - 0.4 * first - 0.1) / r00;
        assert!(
            (from_first[0] - (0.1 + first + second)).abs() < 1e-9,
            "{from_first:?}"
        );
        after["id"] = json!("essential");
        assert_eq!(
            predicted_restoration(&after, &delivered, 1.0, 0.0, 48000),
            restored
        );
        // A tone step that raises 40–120 Hz by four units of the shelf
        // takes the change of peak control of the shelf model, 0.4 − 1.3 +
        // 0.8 = −0.1 dB a unit, less the change of peak control when its
        // input takes the 0.3 LU that the shelf adds, −0.6 × 0.3 (D287).
        let mut shelved = response.clone();
        shelved["shelf_free_local_db"] = json!([1.3, -0.1, -0.2, -0.2, -0.2, -0.2]);
        shelved["shelf_free_lu"] = json!(0.3);
        let change = 0.05 + 4.0 * (0.4 - 1.3 + 0.8 + 0.6 * 0.3);
        let first = -(change - 0.1) / r00;
        let second = -(change + 0.4 * first - 0.1) / r00;
        let toned = predicted_restoration(&record(&shelved), &delivered, 0.0, 4.0, 48000).gains();
        assert!(
            (toned[0] - (0.2 + first + second)).abs() < 1e-9,
            "{toned:?}"
        );
        // Without the shelf model without peak control, or without its
        // loudness, the step keeps the gains of the parent.
        shelved["shelf_free_lu"] = Value::Null;
        assert_eq!(
            predicted_restoration(&record(&shelved), &delivered, 0.0, 4.0, 48000).gains(),
            delivered.restoration_db
        );
        assert_eq!(
            predicted_restoration(&parent, &delivered, 0.0, 4.0, 48000).gains(),
            delivered.restoration_db
        );
        // Without the local band changes, the gains of the parent.
        response["local_up_db"] = Value::Null;
        let old = record(&response);
        assert_eq!(
            predicted_restoration(&old, &delivered, 1.0, 0.0, 48000).gains(),
            delivered.restoration_db
        );
    }

    #[test]
    fn the_hold_plans_each_step_with_the_shift_of_its_pass() {
        let facts = preset_facts(None);
        let spec = parent();
        let span = crate::low_end::section_frames(48_000);
        let low = crate::low_end::Sections {
            starts: (0..4).map(|s| s * span).collect(),
            ends: (1..=4).map(|s| s * span).collect(),
            ..Default::default()
        };
        let changes = vec![[Some(-0.8), Some(-0.8)]; 4];
        let shifted = |c: &[[Option<f64>; 2]], d: [f64; 2]| -> Vec<[Option<f64>; 2]> {
            c.iter()
                .map(|c| std::array::from_fn(|b| c[b].map(|x| x + d[b])))
                .collect()
        };
        let passes = HoldPasses {
            first: Vec::new(),
            passes: vec![
                ([0.3, 0.0], vec![[0.05, 0.0]; 4]),
                ([0.1, 0.0], vec![[0.1, 0.0]; 4]),
            ],
        };
        let held = hold_steps(&spec, &facts, &low, &changes, &passes, None, &[], 4 * span)
            .expect("a hold");
        // The first step plans with the shift of the first pass.
        let planned = shifted(&changes, [0.3, 0.0]);
        let shaped = shaped_spectrum(&facts.spectrum, &spec);
        let (next, first) =
            crate::low_end::next(&spec, &shaped, &low, &planned, None, 4 * span).expect("a step");
        // The first pass gives less than its shift. The second step measures
        // the change of the first step and of the first pass, and plans with
        // the shift of the second pass.
        let measured: Vec<[Option<f64>; 2]> = changes
            .iter()
            .enumerate()
            .map(|(s, c)| {
                std::array::from_fn(|b| {
                    c[b].map(|x| x + [0.05, 0.0][b] + first.expected_db[s][b] + [0.1, 0.0][b])
                })
            })
            .collect();
        assert!(!crate::low_end::passes(&measured));
        let (_, second) = crate::low_end::next(
            &next,
            &shaped_spectrum(&facts.spectrum, &next),
            &low,
            &measured,
            Some((&first, &planned)),
            4 * span,
        )
        .expect("a second step");
        assert_eq!(held.changes_db, second.changes_db);
        assert_eq!(held.hold_wet, second.hold_wet);
        // The bells of the first step take the change of peak control of
        // the shelf model in each section, per dB of their rise of 40–120 Hz
        // (D290).
        let take = vec![[0.0, -0.1]; 4];
        let taken = hold_steps(
            &spec,
            &facts,
            &low,
            &changes,
            &passes,
            None,
            &take,
            4 * span,
        )
        .expect("a hold");
        for (s, (a, b)) in taken.changes_db.iter().zip(&held.changes_db).enumerate() {
            let rise = first.expected_db[s][0] + first.offset_db;
            assert_eq!(a[0], b[0]);
            if let (Some(a), Some(b)) = (a[1], b[1]) {
                assert!((a - b + 0.1 * rise).abs() < 1e-12, "{s} {a} {b} {rise}");
            }
        }
    }

    #[test]
    fn the_hold_takes_the_shelf_model_for_the_restoring_gain_of_the_bass() {
        let spectrum = spectrum().spectrum();
        let delivered = parent();
        let mut spec = delivered.clone();
        spec.restoration_db[0] += 0.3;
        spec.restoration_db[1] -= 0.1;
        let response = json!({"shelf_db": 1.0,
            "sections_shelf_db": [[0.2, -0.05], [0.1, -0.02]]});
        let changes = restoration_hold_db(
            &delivered.restoration_db,
            &spec.restoration_db,
            &response,
            &spectrum,
            2,
        );
        // The other bands take the change from the source spectrum.
        let mut rest = spec.restoration_db;
        rest[0] = delivered.restoration_db[0];
        let other =
            crate::low_end::restoration_shift_db(&spectrum, &delivered.restoration_db, &rest);
        assert!(other[1] < 0.0, "{other:?}");
        for (change, shelf) in changes.iter().zip([[0.2, -0.05], [0.1, -0.02]]) {
            for b in 0..2 {
                assert!((change[b] - (0.3 * shelf[b] + other[b])).abs() < 1e-12);
            }
        }
        // Without the shelf model, each section takes the change from the
        // source spectrum, and the same gains change nothing.
        let source = crate::low_end::restoration_shift_db(
            &spectrum,
            &delivered.restoration_db,
            &spec.restoration_db,
        );
        assert_eq!(
            restoration_hold_db(
                &delivered.restoration_db,
                &spec.restoration_db,
                &Value::Null,
                &spectrum,
                2
            ),
            vec![source; 2]
        );
        let gains = delivered.restoration_db;
        assert!(restoration_hold_db(&gains, &gains, &response, &spectrum, 2).is_empty());
    }

    #[test]
    fn a_tone_step_takes_the_change_of_peak_control_of_the_shelf_model() {
        let facts = preset_facts(None);
        let mut delivered = parent();
        delivered.low_end_hold = None;
        let mut spec = delivered.clone();
        spec.tone.stereo.push(dsp::EqBand {
            kind: dsp::Kind::LowShelf,
            hz: BASS_SHELF_HZ,
            q: std::f64::consts::FRAC_1_SQRT_2,
            gain_db: 1.5,
        });
        let response = json!({"shift_db": 1.0, "up_lu": 0.86, "down_lu": -0.91,
            "loss_lu": 0.3, "far_db": 3.0, "far_lu": 2.3, "shelf_db": 1.0,
            "shelf_free_lu": 0.3,
            "sections_down_db": [[0.1, 0.05], [0.05, 0.02]],
            "sections_up_db": [[-0.1, -0.05], [-0.04, -0.02]],
            "sections_shelf_db": [[0.3, -0.05], [0.5, -0.1]],
            "sections_shelf_free_db": [[0.6, -0.1], [0.6, -0.12]],
            "sections_free_db": [[0.1, 0.0], [0.05, 0.0]]});
        let spectrum = shaped_spectrum(&facts.spectrum, &delivered);
        let raise = shelf_raise_db(&response, &spectrum).expect("a raise");
        assert!(raise > 0.6 && raise < 1.0, "{raise}");
        // Each section takes the change of the shelf model less the rise of
        // the shelf without peak control, and less the change of peak
        // control at the 0.3 LU that the shelf adds to its input, per dB of
        // 40–120 Hz.
        let take = shelf_take_db(&response, &spectrum, 2);
        let louder = peak_control_sections_db(&response, 0.3);
        assert!(louder[0][0] < 0.0, "{louder:?}");
        assert_eq!(take.len(), 2);
        for ((t, l), raw) in take.iter().zip(&louder).zip([[-0.2, 0.05], [-0.05, 0.02]]) {
            for b in 0..2 {
                assert!((t[b] - (raw[b] - l[b]) / raise).abs() < 1e-12, "{take:?}");
            }
        }
        // A record without the shelf model without peak control or its
        // loudness, or with other sections, gives no take.
        for key in ["sections_shelf_free_db", "shelf_free_lu"] {
            let mut old = response.clone();
            old[key] = Value::Null;
            assert!(shelf_take_db(&old, &spectrum, 2).is_empty());
        }
        assert!(shelf_take_db(&response, &spectrum, 3).is_empty());
        // The bass shelf of the request counts in units of the shelf.
        let held = |s: &Spec| {
            let h = held_power(&facts.spectrum, s).0[0];
            h[0] + h[1]
        };
        let shelves = tone_shelves(&delivered, &spec, &response, &facts);
        assert!(
            (shelves - 10.0 * (held(&spec) / held(&delivered)).log10() / raise).abs() < 1e-12,
            "{shelves}"
        );
        assert!(shelves > 1.0, "{shelves}");
        assert_eq!(tone_shelves(&delivered, &delivered, &response, &facts), 0.0);
        assert_eq!(tone_shelves(&delivered, &spec, &Value::Null, &facts), 0.0);
    }

    #[test]
    fn the_tone_of_a_section_follows_its_own_spectrum() {
        let delivered = parent();
        let mut spec = delivered.clone();
        spec.tone.stereo.push(dsp::EqBand {
            kind: dsp::Kind::LowShelf,
            hz: BASS_SHELF_HZ,
            q: std::f64::consts::FRAC_1_SQRT_2,
            gain_db: 1.5,
        });
        let gains = low_gains(&delivered, &spec, 48000);
        let splits = crate::low_end::LOW_SPLITS;
        assert_eq!(gains.len(), 2 * splits);
        // The shelf lifts the bottom of 40–120 Hz more than its top.
        assert!(gains[0][0] > gains[splits - 1][0] && gains[splits - 1][0] > 1.0);
        // A section with its bass at the bottom of the band takes more of
        // the shelf than a section with its bass at the top (D288).
        let at = |k: usize| -> Vec<[f64; 2]> {
            (0..2 * splits)
                .map(|j| {
                    if j == k || j == splits {
                        [0.0, -10.0]
                    } else {
                        [-60.0, -70.0]
                    }
                })
                .collect()
        };
        let (low, high) = (
            section_tone_db(&at(0), &gains),
            section_tone_db(&at(splits - 1), &gains),
        );
        assert!(low[0] > high[0] + 0.2, "{low:?} {high:?}");
        assert!((low[1] - high[1]).abs() < 1e-9, "{low:?} {high:?}");
        // The same tone changes nothing.
        let same = section_tone_db(&at(0), &low_gains(&delivered, &delivered, 48000));
        assert!(same.iter().all(|x| x.abs() < 1e-12), "{same:?}");
    }

    #[test]
    fn the_restoring_gains_add_their_loudness_to_the_prefix() {
        let spectrum = spectrum();
        let delivered = parent();
        assert_eq!(
            restoration_loudness_db(&spectrum, &delivered, &delivered, None),
            0.0
        );
        let mut spec = delivered.clone();
        spec.restoration_db[0] += 0.4;
        let raised = restoration_loudness_db(&spectrum, &spec, &delivered, None);
        // The tone of the request with each set of restoring filters.
        let loudness = |gains: &[f64; 6]| {
            let mut tone = loudness_tone(&spec, None);
            tone.stereo
                .extend(crate::treatment_render::filters_of(gains));
            predicted_loudness_db(&spectrum, &tone, spec.side_gain.as_ref())
        };
        let moved = loudness(&spec.restoration_db) - loudness(&delivered.restoration_db);
        assert!(
            raised > 0.0 && (raised - moved).abs() < 1e-12,
            "{raised} {moved}"
        );
    }

    #[test]
    fn the_local_change_follows_the_drive_models() {
        let curve = Response::of(&json!({"shift_db": 1.0, "up_lu": 0.86,
            "down_lu": -0.91, "loss_lu": 0.3}))
        .unwrap();
        let local = |x| local_change_db(&curve, 0.3, -0.6, 0.8, -1.4, 3.0, x);
        assert_eq!(local(0.0), 0.0);
        assert!((local(1.0) + 0.6).abs() < 1e-12);
        assert!((local(2.0) + 1.0).abs() < 1e-12);
        assert!((local(3.0) + 1.4).abs() < 1e-12);
        assert!((local(4.0) + 1.8).abs() < 1e-12);
        assert!((local(0.5) + 0.3).abs() < 1e-12);
        assert!((local(-0.5) - 0.15).abs() < 1e-12);
        assert!((local(-1.0) - 0.3).abs() < 1e-12);
        // Under the drive 1 dB down, the change approaches the change
        // without peak control.
        let rate: f64 = (1.0 - (0.885 + 0.05)) / 0.21;
        assert!((local(-3.0) - (0.8 - 0.5 * (-2.0 * rate).exp())).abs() < 1e-12);
        assert!((local(-40.0) - 0.8).abs() < 1e-3);
    }

    #[test]
    fn the_preview_of_a_ceiling_step_solves_the_drive_from_the_response() {
        let plan: Value = serde_json::from_str(include_str!("../tests/data/adjust-parent.json"))
            .expect("parent spec");
        let record = |response: Value| {
            json!({"spec": plan, "render": {"drive_db": 3.0, "true_peak": {"trim_db": -0.1},
                "deesser": deesser_stats(), "drive_response": response}})
        };
        let facts = preset_facts(Some(-7.0));
        let steps = Steps {
            ceiling: -2,
            ..Steps::default()
        };
        // The lower ceiling takes 1 LU at the drive of the parent, and the
        // drive gives it back on the curve of the response.
        let response = json!({"shift_db": 1.0, "up_lu": 0.86, "down_lu": -0.91, "loss_lu": 0.3});
        let (spec, fixed) = preview(&record(response.clone()), &steps, &facts).unwrap();
        let ceiling = plan["ceiling_dbtp"].as_f64().unwrap();
        assert!((spec.ceiling_dbtp - (ceiling - 1.0)).abs() < 1e-9);
        let moved = fixed.drive_db - 3.0 + 1.0;
        assert!(
            (0.885 * moved - 0.025 * moved * moved - 1.0).abs() < 1e-9,
            "{moved}"
        );
        // Without a response, the drive follows the target dB for dB.
        let (_, fixed) = preview(&record(Value::Null), &steps, &facts).unwrap();
        assert!((fixed.drive_db - 3.0).abs() < 1e-9, "{}", fixed.drive_db);
        // Streaming lowers the level by more than the drive 1 dB down: the
        // drive gives back more of the loss of peak control than the 0.09 LU
        // of the drive 1 dB down, and less than the whole 0.3 LU.
        let target = plan["target_lufs"].as_f64().unwrap();
        let (spec, fixed) =
            preview_preset(&record(response.clone()), Preset::Streaming, &facts).unwrap();
        let (_, plain) = preview_preset(&record(Value::Null), Preset::Streaming, &facts).unwrap();
        let moved = STREAMING_LUFS - target - (spec.ceiling_dbtp - ceiling);
        let given_back = plain.drive_db - fixed.drive_db;
        assert!(moved < -1.0, "{moved}");
        assert!(
            given_back > 0.09 && given_back < 0.3,
            "{moved} {given_back}"
        );
    }

    #[test]
    fn the_drive_solve_starts_from_the_output_of_the_parent() {
        let plan: Value = serde_json::from_str(include_str!("../tests/data/adjust-parent.json"))
            .expect("parent spec");
        let target = plan["target_lufs"].as_f64().unwrap();
        let record = |search: Value| {
            json!({"spec": plan, "render": {"drive_db": 3.0, "true_peak": {"trim_db": -0.1},
                "deesser": deesser_stats(), "lufs": target - 0.03, "search": search}})
        };
        let facts = preset_facts(Some(-7.0));
        let steps = Steps {
            loudness: 2,
            ..Steps::default()
        };
        let (_, plain) = preview(&record(Value::Null), &steps, &facts).unwrap();
        // The parent ends 0.03 LU under its target, and the drive takes the
        // 0.03 LU with the step.
        let (_, missed) = preview(&record(json!({"target_lufs": target})), &steps, &facts).unwrap();
        assert!(
            (missed.drive_db - plain.drive_db - 0.03).abs() < 1e-9,
            "{} {}",
            missed.drive_db,
            plain.drive_db
        );
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
        // From −7 LUFS, two steps up reach −6.5 LUFS only at half of the
        // steps, and four steps up leave the range at each share.
        let (s, mut p) = (spectrum(), parent());
        p.target_lufs = -7.0;
        let steps = |loudness| Steps {
            loudness,
            ..Steps::default()
        };
        let reasons: Vec<String> = candidates(&p, &steps(2), &s, ALL)
            .unwrap()
            .into_iter()
            .map(|c| c.reason)
            .collect();
        assert_eq!(reasons, [share_reason(0.5)]);
        let spec = spec_at(&p, &steps(2), 0.5, &s, ALL).unwrap();
        assert_eq!(spec.target_lufs, LOUDNESS_RANGE_LUFS.1);
        assert!(candidates(&p, &steps(4), &s, ALL).is_err());
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
    fn a_share_under_the_detectable_increment_is_left_out() {
        let (s, p) = (spectrum(), parent());
        let reasons = |steps: Steps| -> Vec<String> {
            candidates(&p, &steps, &s, ALL)
                .unwrap()
                .into_iter()
                .map(|c| c.reason)
                .collect()
        };
        // Three quarters of one step is 0.375 dB, under 0.41 dB.
        let one = Steps {
            air: 1,
            ..Steps::default()
        };
        assert_eq!(reasons(one), [share_reason(1.0)]);
        let two = Steps {
            air: 2,
            ..Steps::default()
        };
        assert_eq!(reasons(two).len(), SHARES.len());
        // Each share keeps the ceiling.
        let ceiling = Steps {
            ceiling: -1,
            ..Steps::default()
        };
        assert_eq!(reasons(ceiling).len(), SHARES.len());
        assert!(one.detectable_at(1.0) && !one.detectable_at(0.75));
    }

    #[test]
    fn a_compression_step_moves_the_mean_reduction_by_one_step() {
        let p = parent();
        let settings = p.compressor.unwrap();
        // Levels from 8 dB under to 4 dB over the threshold: across the
        // knee of 6 dB.
        let over_db = std::array::from_fn(|i| -8.0 + 0.6 * i as f64);
        let mean = |shift| crate::compressor::mean_reduction_over(&over_db, settings, shift);
        let compression = Compression {
            over_db,
            mean_reduction_db: 1.5,
        };
        let offset = level_shift(&compression, settings, 1.5).unwrap();
        assert!((mean(offset) - 1.5).abs() < 1e-9);
        for steps in [-2, -1, 1, 2, 4] {
            let change = steps as f64 * STEP_DB;
            let shift = compression_shift(&compression, settings, change).unwrap();
            assert!(
                (mean(offset + shift) - 1.5 - change).abs() < 1e-9,
                "{steps}"
            );
            // In the knee, a dB of threshold moves the reduction by less
            // than half a dB.
            assert!(shift.abs() > 2.0 * change.abs(), "{steps}: {shift}");
        }
        // The parent reduces by 1.5 dB, so three steps down are outside the
        // range of the compressor.
        assert!(compression_shift(&compression, settings, -1.5).is_none());
        let available = Availability {
            compressor: Some(compression),
            ..ALL
        };
        let steps = Steps {
            compression: 1,
            ..Steps::default()
        };
        let c = candidates(&p, &steps, &spectrum(), available).unwrap();
        let moved = settings.threshold_dbfs - c[0].spec.compressor.unwrap().threshold_dbfs;
        let solved = compression_shift(&compression, settings, STEP_DB).unwrap();
        assert!((moved - solved).abs() < 1e-12);
        let down = Steps {
            compression: -3,
            ..Steps::default()
        };
        let range = candidates(&p, &down, &spectrum(), available);
        assert_eq!(
            range.map(|c| c.len()).map_err(|e| e.to_string()),
            Ok(2),
            "three quarters and half of three steps down are within the range"
        );
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
        let none = Steps::default();
        assert!(sound(-9.1, 1.0).is_duplicate_of(&o.sound, &none));
        assert!(!sound(-8.0, 10f64.powf(0.1)).is_duplicate_of(&o.sound, &none));
        assert!(!sound(-9.0, 1.3).is_duplicate_of(&o.sound, &none));
        // The air band is not material in the signature; its measure counts.
        let mut air = sound(-9.0, 1.0);
        air.measures.air_db -= 2.0;
        assert!(!air.is_duplicate_of(&o.sound, &none));
        let mut compressed = sound(-9.0, 1.0);
        compressed.compressor_db += 1.2;
        assert!(!compressed.is_duplicate_of(&o.sound, &none));
        let mut ceiling = sound(-9.0, 1.0);
        ceiling.ceiling_dbtp = -1.0;
        assert!(!ceiling.is_duplicate_of(&o.sound, &none));
    }

    #[test]
    fn a_requested_control_counts_from_the_detectable_increment() {
        let o = other("Forward");
        let one = |set: fn(&mut Steps)| {
            let mut steps = Steps::default();
            set(&mut steps);
            steps
        };
        // One Air step measured 0.477 to 0.492 dB on the eight album mixes.
        let mut air = sound(-9.0, 1.0);
        air.measures.air_db += 0.48;
        assert!(air.is_duplicate_of(&o.sound, &Steps::default()));
        assert!(!air.is_duplicate_of(&o.sound, &one(|s| s.air = 1)));
        assert!(air.is_duplicate_of(&o.sound, &one(|s| s.width = 1)));
        air.measures.air_db = o.sound.measures.air_db + 0.40;
        assert!(air.is_duplicate_of(&o.sound, &one(|s| s.air = 1)));
        let quieter = sound(-9.45, 1.0);
        assert!(quieter.is_duplicate_of(&o.sound, &Steps::default()));
        assert!(!quieter.is_duplicate_of(&o.sound, &one(|s| s.loudness = -1)));
        let mut compressed = sound(-9.0, 1.0);
        compressed.compressor_db += 0.45;
        assert!(!compressed.is_duplicate_of(&o.sound, &one(|s| s.compression = 1)));
        let mut wider = sound(-9.0, 1.0);
        wider.measures.width_db += 0.45;
        assert!(!wider.is_duplicate_of(&o.sound, &one(|s| s.width = 1)));
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
