//! Gates and the candidate ladder: render the strongest candidate of a
//! version first and keep the first one that passes its gates.
use crate::{
    analysis,
    audio_io::{Result, Stereo},
    dsp::db,
    imaging,
    pipeline::SourceAnalysis,
    stereo,
    treatment_render::{Rendered, Renderer, Spec, RESTORATION_PASSES},
};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Matched-level change required in one whole-file stereo band.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
pub struct TonalGate {
    /// Index into `stereo::BAND_EDGES_HZ` bands.
    pub band: usize,
    /// Sign of the intended change: 1 for a lift, -1 for a cut.
    pub direction: i8,
    pub minimum_db: f64,
    /// When set, the gate measures the change of `band` minus the change of
    /// this band, so a tilt is judged by its end-to-end difference.
    #[serde(default)]
    pub relative_to: Option<usize>,
    /// The channel whose band power the gate measures. A move on the mid
    /// channel is judged on the mid channel, where a side move elsewhere in
    /// the tone does not offset it (D128).
    #[serde(default)]
    pub channel: GateChannel,
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Debug, Default)]
pub enum GateChannel {
    #[default]
    Stereo,
    Mid,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug, Default)]
pub struct Gates {
    pub tonal: Vec<TonalGate>,
    /// Compare center, mono, correlation and balance against the source with
    /// the tonal-aware checks.
    pub tonal_image: bool,
    /// Compare the stereo image against the source with the width checks.
    pub image: bool,
    /// Mean side/mid rise over 500 Hz–12 kHz against the source, in dB.
    pub minimum_width_db: Option<f64>,
    /// Absolute bound on maximum limiter reduction.
    pub limiter_maximum_db: Option<f64>,
    /// Absolute bound on mean limiter reduction.
    #[serde(default)]
    pub limiter_mean_db: Option<f64>,
    /// Bound on P90 attack loss added against the reference render.
    #[serde(default)]
    pub attack_loss_db: Option<f64>,
    /// The candidate must beat the next alternative on three-second gain span
    /// without adding more than 0.25 dB of attack loss.
    pub glue: bool,
    /// Hold the bass of each 15 s section within `low_end::LIMIT_DB` of the
    /// reference at matched loudness (`set.low-end.v3`).
    #[serde(default)]
    pub low_end: bool,
    /// Report the additional mean limiter reduction and do not gate on it.
    /// A request on a delivered version gives it as a warning (D171).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub workload_as_warning: bool,
    /// Keep the delivered loudness at or under the audibility point of the
    /// prefix of the candidate (`audibility::point`, D131): each version of
    /// a set (D206).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub under_own_point: bool,
}

/// One candidate of a version. A glue candidate is judged against the
/// candidate that follows it, which must be its plain counterpart.
#[derive(Clone, Serialize)]
pub struct Candidate {
    pub reason: String,
    pub spec: Spec,
    pub gates: Gates,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CandidateTrial {
    pub index: usize,
    pub reason: String,
    pub passed: bool,
    pub checks: Vec<imaging::Check>,
    #[serde(deserialize_with = "number_or_nan")]
    pub lufs: f64,
    #[serde(deserialize_with = "number_or_nan")]
    pub limiter_maximum_db: f64,
    #[serde(deserialize_with = "number_or_nan")]
    pub gain_span_db: f64,
    /// P90 attack loss against the mix (`analysis::compare`). It counts the
    /// tone of the version (D164).
    pub attack_loss_p90_db: Option<f64>,
    /// P90 loss of onset energy against the prefix before the compressor:
    /// the loss of the dynamics stages (`Rendered::energy_attack_loss_p90_db`).
    pub energy_attack_loss_p90_db: Option<f64>,
    /// The renders of the low-end hold, in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub low_end: Vec<crate::low_end::Step>,
}

/// A number of a record, where a trial without a render writes `null` for
/// NaN.
fn number_or_nan<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<f64, D::Error> {
    Ok(Option::<f64>::deserialize(deserializer)?.unwrap_or(f64::NAN))
}

/// The time limit of a worker (`selection.time-limit.v1`). A candidate of a
/// ladder starts only when the longest candidate of the worker so far can
/// end before `end`. After the first candidate that cannot start, each
/// ladder of the worker stops at its first candidate.
pub struct Deadline {
    end: Instant,
    /// The longest candidate of the worker so far.
    longest: Duration,
    /// The start of the candidate that runs.
    started: Option<Instant>,
    expired: bool,
}

impl Deadline {
    pub fn new(end: Instant) -> Self {
        Self {
            end,
            longest: Duration::ZERO,
            started: None,
            expired: false,
        }
    }

    /// End the candidate that runs, and start the next candidate. The check
    /// fails when the time that is left is shorter than the longest
    /// candidate, or when no time is left.
    fn start(&mut self) -> imaging::Check {
        self.start_at(Instant::now())
    }

    fn start_at(&mut self, now: Instant) -> imaging::Check {
        self.finish_at(now);
        let left = self.end.saturating_duration_since(now);
        let check = imaging::Check::range(
            "selection.time_limit",
            "Seconds left in the time limit of the worker, against the longest candidate of the worker",
            "s",
            Some(if self.expired { 0.0 } else { left.as_secs_f64() }),
            Some(self.longest.as_secs_f64().max(1e-3)),
            None,
        );
        if check.passed {
            self.started = Some(now);
        } else {
            self.expired = true;
        }
        check
    }

    /// End the candidate that runs.
    fn finish(&mut self) {
        self.finish_at(Instant::now());
    }

    fn finish_at(&mut self, now: Instant) {
        if let Some(started) = self.started.take() {
            self.longest = self.longest.max(now - started);
        }
    }
}

impl Deadline {
    /// Whether a candidate could not start.
    pub fn expired(&self) -> bool {
        self.expired
    }
}

/// The trials of the candidate ladders of a worker, by ladder key, and the
/// failed trials of an earlier invocation of the worker. A ladder takes a
/// failed trial of its key in place of the render of its candidate when the
/// trial has the index and the reason of the candidate. A ladder that
/// starts at a candidate on a new renderer gives the trials of the whole
/// ladder from that candidate, so the trials are the trials of an
/// invocation without a time limit.
#[derive(Default)]
pub struct TrialRecord {
    key: String,
    trials: Vec<(String, CandidateTrial)>,
    earlier: Vec<(String, Vec<CandidateTrial>)>,
    replay: std::collections::HashMap<String, std::collections::VecDeque<CandidateTrial>>,
}

impl TrialRecord {
    /// A record that replays `earlier`: for each key, the trials of an
    /// earlier invocation in order.
    pub fn replaying(earlier: Vec<(String, Vec<CandidateTrial>)>) -> Self {
        let earlier: Vec<_> = earlier
            .into_iter()
            .map(|(key, trials)| (key, replayable(trials)))
            .collect();
        Self {
            key: String::new(),
            trials: Vec::new(),
            replay: earlier
                .iter()
                .map(|(key, trials)| (key.clone(), trials.clone().into()))
                .collect(),
            earlier,
        }
    }

    /// The key of the ladders that run next: a version, or the empty key
    /// for the reference.
    pub fn key(&mut self, key: &str) {
        self.key = key.to_owned();
    }

    /// For each key, the failed trials before the first trial that passed
    /// or met the time limit: the trials that a continuation takes in place
    /// of renders. A key whose ladders did not run keeps the trials of the
    /// earlier invocation. The keys of the earlier invocation come first,
    /// then the other keys in order of their first trial.
    pub fn replayable(&self) -> Vec<(String, Vec<CandidateTrial>)> {
        let mut keys: Vec<&str> = self.earlier.iter().map(|(key, _)| key.as_str()).collect();
        for (key, _) in &self.trials {
            if !keys.contains(&key.as_str()) {
                keys.push(key);
            }
        }
        keys.into_iter()
            .map(|key| {
                let trials: Vec<CandidateTrial> = self
                    .trials
                    .iter()
                    .filter(|(k, _)| k == key)
                    .map(|(_, t)| t.clone())
                    .collect();
                let trials = match self.earlier.iter().find(|(k, _)| k == key) {
                    Some((_, earlier)) if trials.is_empty() => earlier.clone(),
                    _ => replayable(trials),
                };
                (key.to_owned(), trials)
            })
            .collect()
    }

    fn log(&mut self, trials: &[CandidateTrial]) {
        let key = &self.key;
        self.trials
            .extend(trials.iter().map(|t| (key.clone(), t.clone())));
    }

    /// The earlier trial of `candidate`, the candidate at `index` of a
    /// ladder whose next candidate is `next`. A glue candidate renders the
    /// next candidate as its plain alternative, and the ladder keeps that
    /// render for the next candidate, so a glue candidate is replayed only
    /// with its next candidate. A trial that does not match ends the replay
    /// of the key.
    fn replayed(
        &mut self,
        index: usize,
        candidate: &Candidate,
        next: Option<&Candidate>,
    ) -> Option<CandidateTrial> {
        let queue = self.replay.get_mut(&self.key)?;
        let matches = |trial: Option<&CandidateTrial>, index: usize, candidate: &Candidate| {
            trial.is_some_and(|t| t.index == index && t.reason == candidate.reason)
        };
        if !matches(queue.front(), index, candidate) {
            queue.clear();
            return None;
        }
        if candidate.gates.glue {
            if let Some(next) = next {
                if !matches(queue.get(1), index + 1, next) {
                    queue.clear();
                    return None;
                }
            }
        }
        queue.pop_front()
    }
}

/// The failed trials before the first trial that passed or met the time
/// limit.
fn replayable(trials: Vec<CandidateTrial>) -> Vec<CandidateTrial> {
    trials
        .into_iter()
        .take_while(|t| !t.passed && !t.checks.iter().any(|c| c.id == "selection.time_limit"))
        .collect()
}

/// A rendered candidate with its measurements.
#[derive(Clone)]
pub struct Measured {
    pub rendered: Rendered,
    pub output: crate::Metrics,
    pub output_stereo: stereo::StereoAnalysis,
    /// The measures of the controls on the render (`adjust::measure`),
    /// from the spectrum of the stereo analysis (D330).
    pub measures: crate::adjust::Delivered,
    pub preservation: analysis::Preservation,
    pub gain_span_db: f64,
    /// The tone and envelopes of the rendered spec, for gate predictions.
    pub tone: Option<(crate::treatment::Tone, Vec<crate::treatment::FocusEnvelope>)>,
    /// Bass power of each 15 s section (`low_end::sections`).
    pub low_sections: crate::low_end::Sections,
}

pub struct Chosen {
    pub index: usize,
    pub spec: Spec,
    pub measured: Measured,
}

/// What the gates, the warm start and the low-end hold of a version read
/// from its reference render: the Essential render of the set, or the base
/// render of a Denser version.
#[derive(Clone)]
pub struct Reference {
    pub drive_db: f64,
    pub lufs: f64,
    /// Integrated loudness of the prefix of the render.
    pub prefix_lufs: Option<f64>,
    /// Gains of the restoring filters of the render, when it restored.
    pub restoration_db: Option<[f64; 6]>,
    /// Mean limiter reduction, in dB.
    pub mean_reduction_db: f64,
    /// P90 attack loss against the mix (`analysis::compare`).
    pub attack_loss_p90_db: Option<f64>,
    /// The whole-file bands of the stereo analysis of the output.
    pub bands: Vec<stereo::BandMetrics>,
    /// Bass power of each 15 s section (`low_end::sections`).
    pub low_sections: crate::low_end::Sections,
    /// The tone and envelopes of the rendered spec, for gate predictions.
    pub tone: Option<(crate::treatment::Tone, Vec<crate::treatment::FocusEnvelope>)>,
}

/// `x` as the 16 hex digits of its bits: a JSON value that keeps every bit.
fn encode(x: f64) -> serde_json::Value {
    format!("{:016x}", x.to_bits()).into()
}

fn encode_option(x: Option<f64>) -> serde_json::Value {
    x.map_or(serde_json::Value::Null, encode)
}

/// The number of `encode`.
fn decode(v: &serde_json::Value) -> Result<f64> {
    let digits = v
        .as_str()
        .filter(|d| d.len() == 16)
        .ok_or("A reference value is not exact")?;
    Ok(f64::from_bits(u64::from_str_radix(digits, 16)?))
}

fn decode_option(v: &serde_json::Value) -> Result<Option<f64>> {
    if v.is_null() {
        Ok(None)
    } else {
        decode(v).map(Some)
    }
}

impl Reference {
    /// The record of the reference with each number bit for bit, without
    /// the tone, which the spec of the reference gives.
    pub fn record(&self) -> serde_json::Value {
        let bands: Vec<serde_json::Value> = self
            .bands
            .iter()
            .map(|b| {
                serde_json::json!({
                    "active": b.active, "left_present": b.left_present,
                    "right_present": b.right_present, "mid_present": b.mid_present,
                    "side_present": b.side_present,
                    "left_power": encode(b.left_power), "right_power": encode(b.right_power),
                    "mid_power": encode(b.mid_power), "side_power": encode(b.side_power),
                    "weighted_power": encode(b.weighted_power),
                    "side_fraction": encode_option(b.side_fraction),
                    "side_mid_db": encode_option(b.side_mid_db),
                    "right_left_db": encode_option(b.right_left_db),
                    "correlation": encode_option(b.correlation),
                    "mono_fraction": encode_option(b.mono_fraction),
                    "mono_fold_down_db": encode_option(b.mono_fold_down_db),
                    "coherence": encode_option(b.coherence),
                    "phase_resultant": encode_option(b.phase_resultant),
                    "cross_phase_degrees": encode_option(b.cross_phase_degrees),
                })
            })
            .collect();
        let sections = &self.low_sections;
        serde_json::json!({
            "drive_db": encode(self.drive_db),
            "lufs": encode(self.lufs),
            "prefix_lufs": encode_option(self.prefix_lufs),
            "restoration_db": self.restoration_db.map(|g| g.map(encode).to_vec()),
            "mean_reduction_db": encode(self.mean_reduction_db),
            "attack_loss_p90_db": encode_option(self.attack_loss_p90_db),
            "bands": bands,
            "low_sections": {
                "starts": sections.starts, "ends": sections.ends,
                "power_db": sections.power_db.iter().map(|p| p.map(encode).to_vec()).collect::<Vec<_>>(),
            },
        })
    }

    /// The reference of `record` (`Reference::record`), with the tone of its
    /// spec.
    pub fn from_record(
        record: &serde_json::Value,
        tone: Option<(crate::treatment::Tone, Vec<crate::treatment::FocusEnvelope>)>,
    ) -> Result<Self> {
        let flag = |b: &serde_json::Value, key: &str| {
            b[key].as_bool().ok_or("A reference band flag is missing")
        };
        let bands = record["bands"]
            .as_array()
            .ok_or("Missing reference bands")?
            .iter()
            .map(|b| {
                Ok(stereo::BandMetrics {
                    active: flag(b, "active")?,
                    left_present: flag(b, "left_present")?,
                    right_present: flag(b, "right_present")?,
                    mid_present: flag(b, "mid_present")?,
                    side_present: flag(b, "side_present")?,
                    left_power: decode(&b["left_power"])?,
                    right_power: decode(&b["right_power"])?,
                    mid_power: decode(&b["mid_power"])?,
                    side_power: decode(&b["side_power"])?,
                    weighted_power: decode(&b["weighted_power"])?,
                    side_fraction: decode_option(&b["side_fraction"])?,
                    side_mid_db: decode_option(&b["side_mid_db"])?,
                    right_left_db: decode_option(&b["right_left_db"])?,
                    correlation: decode_option(&b["correlation"])?,
                    mono_fraction: decode_option(&b["mono_fraction"])?,
                    mono_fold_down_db: decode_option(&b["mono_fold_down_db"])?,
                    coherence: decode_option(&b["coherence"])?,
                    phase_resultant: decode_option(&b["phase_resultant"])?,
                    cross_phase_degrees: decode_option(&b["cross_phase_degrees"])?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let sections = &record["low_sections"];
        let positions = |key: &str| -> Result<Vec<usize>> {
            sections[key]
                .as_array()
                .ok_or("Missing reference sections")?
                .iter()
                .map(|v| {
                    v.as_u64()
                        .map(|x| x as usize)
                        .ok_or_else(|| "A reference section is not a position".into())
                })
                .collect()
        };
        let power_db = sections["power_db"]
            .as_array()
            .ok_or("Missing reference section powers")?
            .iter()
            .map(|p| Ok([decode(&p[0])?, decode(&p[1])?]))
            .collect::<Result<Vec<_>>>()?;
        let restoration_db = match record["restoration_db"].as_array() {
            Some(gains) if gains.len() == 6 => {
                let mut out = [0.0; 6];
                for (o, g) in out.iter_mut().zip(gains) {
                    *o = decode(g)?;
                }
                Some(out)
            }
            Some(_) => return Err("Reference restoring gains need six bands".into()),
            None => None,
        };
        if bands.len() != 6 {
            return Err("A reference needs six bands".into());
        }
        Ok(Self {
            drive_db: decode(&record["drive_db"])?,
            lufs: decode(&record["lufs"])?,
            prefix_lufs: decode_option(&record["prefix_lufs"])?,
            restoration_db,
            mean_reduction_db: decode(&record["mean_reduction_db"])?,
            attack_loss_p90_db: decode_option(&record["attack_loss_p90_db"])?,
            bands,
            low_sections: crate::low_end::Sections {
                starts: positions("starts")?,
                ends: positions("ends")?,
                power_db,
                side_db: Vec::new(),
                low_db: Vec::new(),
            },
            tone,
        })
    }

    /// The reference measurements of `m`.
    pub fn of(m: &Measured) -> Self {
        Self {
            drive_db: m.rendered.drive_db,
            lufs: m.rendered.lufs,
            prefix_lufs: m.rendered.prefix_lufs,
            restoration_db: m.rendered.restoration.as_ref().map(|r| r.gains_db),
            mean_reduction_db: m.rendered.limiter.mean_reduction_db,
            attack_loss_p90_db: m.preservation.attack_loss_p90_db,
            bands: m.output_stereo.whole.bands.clone(),
            low_sections: m.low_sections.clone(),
            tone: m.tone.clone(),
        }
    }
}

/// The level of each 3 s window of `audio` at `rate`, at 1 s steps.
pub(crate) fn window_levels(audio: &[Stereo], rate: u32) -> Vec<f64> {
    window_levels_read(&mut crate::scratch::Slice(audio), rate).expect("frames of a slice")
}

/// Steps of 1 s of `window_levels` in each span of a stored recording.
const LEVEL_SPAN_STEPS: usize = 64;

/// `window_levels` of the frames of `audio`, read one span of steps at a
/// time.
pub(crate) fn window_levels_read(
    audio: &mut impl crate::scratch::Frames,
    rate: u32,
) -> Result<Vec<f64>> {
    let (window, hop) = (3 * rate as usize, rate as usize);
    let count = audio.len().saturating_sub(window).div_ceil(hop);
    let mut levels = Vec::with_capacity(count);
    if count == 0 {
        return Ok(levels);
    }
    // Window k sums the steps k, k + 1 and k + 2. Each step adds its frames
    // to the sums of its three windows in one pass: each sum adds the same
    // values in the same order as a pass for each window, and the three
    // sums are independent chains (D310).
    let steps = count + 2;
    let mut sums = [0.0f64; 3];
    let mut buffer = Vec::new();
    for first in (0..steps).step_by(LEVEL_SPAN_STEPS) {
        let last = (first + LEVEL_SPAN_STEPS).min(steps);
        let frames = audio.window(first * hop, (last - first) * hop, &mut buffer)?;
        for (step, frames) in (first..last).zip(frames.chunks(hop)) {
            let [mut a, mut b, mut c] = [sums[1], sums[2], 0.0];
            for f in frames {
                let energy = (f[0] as f64).powi(2) + (f[1] as f64).powi(2);
                a += energy;
                b += energy;
                c += energy;
            }
            if step >= 2 {
                levels.push(db((a / (2.0 * window as f64)).sqrt()));
            }
            sums = [a, b, c];
        }
    }
    Ok(levels)
}

/// P10–P90 span of three-second output-minus-source level over active
/// windows. Both are at `rate`.
pub fn gain_span_db(source: &[Stereo], output: &[Stereo], activity_floor: f64, rate: u32) -> f64 {
    gain_span_of(
        &window_levels(source, rate),
        &window_levels(output, rate),
        activity_floor,
    )
}

/// `gain_span_db` with the window levels of the source and of the output.
fn gain_span_of(source_levels: &[f64], output_levels: &[f64], activity_floor: f64) -> f64 {
    let floor = db(activity_floor.sqrt());
    let a = source_levels;
    let b = output_levels;
    let gains: Vec<f64> = a
        .iter()
        .zip(b)
        .filter(|(s, _)| **s > floor)
        .map(|(s, o)| o - s)
        .collect();
    if gains.is_empty() {
        return 0.0;
    }
    analysis::percentile(&gains, 0.9) - analysis::percentile(&gains, 0.1)
}

/// The measures of `rendered` against the source of `source`.
pub fn measure(source: &SourceAnalysis, rendered: Rendered) -> Result<Measured> {
    let rate = source.sample_rate;
    // Each measure reads the frames in spans, so a stored render stays in
    // its scratch file (D253). One pass gives the metrics and the blocks.
    let mut frames = rendered.audio.frames();
    let mut metrics = crate::MetricsScan::new(rate);
    let blocks = analysis::blocks_read_with(&mut frames, rate, |span| metrics.push(span))?;
    let output = metrics.finish();
    let (output_stereo, spectrum) = stereo::analyze_with_spectrum_read(&mut frames, rate)?;
    let measures = crate::adjust::measures_of(&spectrum);
    let preservation = analysis::compare_blocks(&source.temporal, &blocks);
    let gain_span_db = gain_span_of(
        &source.window_levels_db,
        &window_levels_read(&mut frames, rate)?,
        source.temporal.activity_floor,
    );
    let low_sections = crate::low_end::sections_read(&mut frames, rate)?;
    drop(frames);
    Ok(Measured {
        rendered,
        output,
        output_stereo,
        measures,
        preservation,
        gain_span_db,
        tone: None,
        low_sections,
    })
}

/// Skip a candidate whose predicted tonal change misses a gate minimum by
/// more than `PREDICTION_MARGIN_DB`: the margin covers the largest error of
/// the prediction against the measured change.
pub const PREDICTION_SKIP: bool = true;
pub const PREDICTION_MARGIN_DB: f64 = 0.3;

/// Response in dB of a tone domain at `hz` at `rate`. A filter with an
/// envelope counts with its mean wet share.
pub(crate) fn domain_response_db(
    bands: &[crate::dsp::EqBand],
    envelopes: &[crate::treatment::FocusEnvelope],
    domain: crate::finishing::Domain,
    hz: f64,
    rate: u32,
) -> f64 {
    bands
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let wet = envelopes
                .iter()
                .find(|e| {
                    std::mem::discriminant(&e.domain) == std::mem::discriminant(&domain)
                        && e.filter_index == i
                })
                .map_or(1.0, |e| {
                    let span = (e.points.last().unwrap().sample - e.points[0].sample).max(1) as f64;
                    e.points
                        .windows(2)
                        .map(|w| 0.5 * (w[0].wet + w[1].wet) * (w[1].sample - w[0].sample) as f64)
                        .sum::<f64>()
                        / span
                });
            let linear = 10f64.powf(b.filter(rate).response_db(hz, rate) / 20.0);
            20.0 * (1.0 + wet * (linear - 1.0)).abs().max(1e-9).log10()
        })
        .sum()
}

/// Predicted gate change of a candidate tone against the reference tone,
/// from the source spectrum at matched K-weighted loudness. Peak control is
/// left out: the restoration returns each render to its prefix balance.
pub fn predicted_tonal_change_db(
    source: &SourceAnalysis,
    candidate: &(crate::treatment::Tone, Vec<crate::treatment::FocusEnvelope>),
    reference: &(crate::treatment::Tone, Vec<crate::treatment::FocusEnvelope>),
    gate: &TonalGate,
) -> Option<f64> {
    use crate::finishing::Domain;
    let s = &source.spectrum;
    let rate = source.sample_rate;
    let edges = stereo::BAND_EDGES_HZ;
    // Band power and K-weighted total of a tone, per channel of the gate.
    let shaped =
        |(tone, envelopes): &(crate::treatment::Tone, Vec<crate::treatment::FocusEnvelope>)| {
            let mut bands = [0.0f64; 6];
            let mut weighted = 0.0;
            for (i, &hz) in s.centers_hz.iter().enumerate() {
                let stereo = domain_response_db(&tone.stereo, envelopes, Domain::Stereo, hz, rate);
                let mid = s.mid_power[i]
                    * 10f64.powf(
                        (stereo + domain_response_db(&tone.mid, envelopes, Domain::Mid, hz, rate))
                            / 10.0,
                    );
                let side = s.side_power[i]
                    * 10f64.powf(
                        (stereo
                            + domain_response_db(&tone.side, envelopes, Domain::Side, hz, rate))
                            / 10.0,
                    );
                weighted += crate::dsp::k_weighting_power(hz, rate) * (mid + side);
                if let Some(b) = edges.windows(2).position(|e| hz >= e[0] && hz < e[1]) {
                    bands[b] += match gate.channel {
                        GateChannel::Stereo => mid + side,
                        GateChannel::Mid => mid,
                    };
                }
            }
            (bands, weighted)
        };
    let ((cb, cw), (rb, rw)) = (shaped(candidate), shaped(reference));
    let change = |band: usize| -> Option<f64> {
        (cb[band] > 0.0 && rb[band] > 0.0)
            .then(|| db(cb[band] / rb[band]) / 2.0 - db(cw / rw) / 2.0)
    };
    let c = change(gate.band)?;
    let c = match gate.relative_to {
        Some(other) => c - change(other)?,
        None => c,
    };
    Some(c * gate.direction as f64)
}

/// A band carries material energy when it is active and holds at least
/// `MATERIAL_BAND_FRACTION` of the strongest band's power, or of the total
/// power weighted by the equal-loudness contour at the monitoring level.
/// The weighted share counts a presence band that the bass outweighs in raw
/// power. Gates, descriptions and the duplicate screen ignore other bands.
pub const MATERIAL_BAND_FRACTION: f64 = 0.01;

pub fn material_bands(analysis: &stereo::StereoAnalysis) -> [bool; 6] {
    material_of(&analysis.whole.bands)
}

/// `material_bands` of the whole-file bands `bands`.
fn material_of(bands: &[stereo::BandMetrics]) -> [bool; 6] {
    let powers: Vec<f64> = bands.iter().map(|b| b.left_power + b.right_power).collect();
    let strongest = powers.iter().copied().fold(0.0, f64::max);
    let weighted: f64 = bands.iter().map(|b| b.weighted_power).sum();
    std::array::from_fn(|i| {
        let band = &bands[i];
        band.active
            && (powers[i] >= MATERIAL_BAND_FRACTION * strongest
                || band.weighted_power >= MATERIAL_BAND_FRACTION * weighted)
    })
}

/// Matched-level change of one whole-file stereo band against the reference,
/// when the band carries material energy in both renders.
fn band_change_db(
    reference: &Reference,
    m: &Measured,
    band: usize,
    channel: GateChannel,
) -> Option<f64> {
    if !(material_of(&reference.bands)[band] && material_bands(&m.output_stereo)[band]) {
        return None;
    }
    let a = &reference.bands[band];
    let b = &m.output_stereo.whole.bands[band];
    let power = |x: &stereo::BandMetrics| match channel {
        GateChannel::Stereo => x.left_power + x.right_power,
        GateChannel::Mid => x.mid_power,
    };
    let (pa, pb) = (power(a), power(b));
    if pa < 1e-20 || pb < 1e-20 {
        return None;
    }
    Some(10.0 * (pb / pa).log10() + reference.lufs - m.rendered.lufs)
}

/// Gate checks of one candidate. Tonal, image, width and workload gates
/// compare with `reference`, the Essential render at the same density; the
/// glue gate compares with `plain`, the same candidate without the compressor.
fn evaluate(
    gates: &Gates,
    m: &Measured,
    reference: Option<&Reference>,
    plain: Option<&Measured>,
) -> Vec<imaging::Check> {
    let mut checks = Vec::new();
    let needs_reference = !gates.tonal.is_empty()
        || gates.tonal_image
        || gates.image
        || gates.minimum_width_db.is_some();
    if needs_reference && reference.is_none() {
        checks.push(imaging::Check::range(
            "selection.reference",
            "Light reference render available",
            "boolean",
            Some(0.0),
            Some(1.0),
            None,
        ));
        return checks;
    }
    if let Some(r) = reference {
        for gate in &gates.tonal {
            let change = band_change_db(r, m, gate.band, gate.channel).and_then(|c| {
                let c = match gate.relative_to {
                    Some(other) => c - band_change_db(r, m, other, gate.channel)?,
                    None => c,
                };
                Some(c * gate.direction as f64)
            });
            checks.push(imaging::Check::range(
                "selection.tonal_change",
                "Matched-level change in the moved band against Light, in the intended direction",
                "dB",
                change,
                Some(gate.minimum_db),
                None,
            ));
        }
        if gates.tonal_image || gates.image {
            // A wide side lift or a mid focus lift changes the center share of
            // its band by construction. The gates bound what mono playback
            // loses and keep every band positively correlated. A symmetric
            // stereo-domain EQ cannot change channel balance; the per-channel
            // clipper can, so balance is not a gate.
            let (mut fold, mut correlation, mut negative) = (0.0f64, 0.0f64, 0usize);
            for i in 1..6 {
                let a = &r.bands[i];
                let b = &m.output_stereo.whole.bands[i];
                if !(a.active && b.active) {
                    continue;
                }
                if let (Some(x), Some(y)) = (a.mono_fold_down_db, b.mono_fold_down_db) {
                    fold = fold.max(x - y);
                }
                if let (Some(x), Some(y)) = (a.correlation, b.correlation) {
                    correlation = correlation.max(x - y);
                    if x >= 0.0 && y < 0.0 {
                        negative += 1;
                    }
                }
            }
            checks.push(imaging::Check::range(
                "selection.mono_fold_down",
                "Largest extra mono fold-down loss in a whole-file band against Light",
                "dB",
                Some(fold),
                None,
                Some(WIDTH_MONO_LOSS_DB),
            ));
            checks.push(imaging::Check::range(
                "selection.correlation_drop",
                "Largest whole-file band correlation drop against Light",
                "correlation",
                Some(correlation),
                None,
                Some(WIDTH_CORRELATION_DROP),
            ));
            checks.push(imaging::Check::range(
                "selection.negative_bands",
                "Whole-file bands turned negatively correlated",
                "bands",
                Some(negative as f64),
                None,
                Some(0.0),
            ));
        }
        if (gates.tonal_image || gates.image) && !gates.workload_as_warning {
            checks.push(imaging::Check::range(
                "selection.mean_workload",
                "Additional mean limiter reduction against Light",
                "dB",
                Some(m.rendered.limiter.mean_reduction_db - r.mean_reduction_db),
                None,
                Some(WORKLOAD_MEAN_DB),
            ));
        }
        if let Some(minimum) = gates.minimum_width_db {
            let rises: Vec<f64> = (2..5)
                .filter_map(|i| {
                    let a = r.bands[i].side_mid_db?;
                    let b = m.output_stereo.whole.bands[i].side_mid_db?;
                    Some(b - a)
                })
                .collect();
            let mean = (!rises.is_empty()).then(|| rises.iter().sum::<f64>() / rises.len() as f64);
            checks.push(imaging::Check::range(
                "selection.width_gain",
                "Mean side/mid rise over 500 Hz–12 kHz against Light",
                "dB",
                mean,
                Some(minimum),
                None,
            ));
        }
    }
    if let Some(bound) = gates.limiter_maximum_db {
        checks.push(imaging::Check::range(
            "selection.limiter_maximum",
            "Maximum limiter reduction",
            "dB",
            Some(m.rendered.limiter.maximum_reduction_db),
            None,
            Some(bound),
        ));
    }
    if let Some(bound) = gates.limiter_mean_db {
        checks.push(imaging::Check::range(
            "selection.limiter_mean",
            "Mean limiter reduction",
            "dB",
            Some(m.rendered.limiter.mean_reduction_db),
            None,
            Some(bound),
        ));
    }
    if let Some(bound) = gates.attack_loss_db {
        checks.push(imaging::Check::range(
            "selection.attack_loss",
            "Additional P90 attack loss against the base render",
            "dB",
            reference
                .and_then(|r| {
                    m.preservation
                        .attack_loss_p90_db
                        .zip(r.attack_loss_p90_db)
                        .map(|(a, b)| a - b)
                })
                .or(Some(0.0)),
            None,
            Some(bound),
        ));
    }
    if gates.glue {
        match plain {
            Some(p) => {
                checks.push(imaging::Check::range(
                    "selection.glue_relief",
                    "Maximum limiter reduction removed by the compressor",
                    "dB",
                    Some(
                        p.rendered.limiter.maximum_reduction_db
                            - m.rendered.limiter.maximum_reduction_db,
                    ),
                    Some(GLUE_RELIEF_DB),
                    None,
                ));
                checks.push(imaging::Check::range(
                    "selection.glue_attack",
                    "Additional attack loss over the plain alternative",
                    "dB",
                    m.preservation
                        .attack_loss_p90_db
                        .zip(p.preservation.attack_loss_p90_db)
                        .map(|(a, b)| a - b)
                        .or(Some(0.0)),
                    None,
                    Some(0.25),
                ));
            }
            None => checks.push(imaging::Check::range(
                "selection.glue_reference",
                "Plain alternative available for comparison",
                "boolean",
                Some(0.0),
                Some(1.0),
                None,
            )),
        }
    }
    checks
}

/// Additional mean limiter reduction a tonal or width candidate may add.
pub const WORKLOAD_MEAN_DB: f64 = 0.3;
/// Extra mono fold-down loss a width candidate may add in any band.
pub const WIDTH_MONO_LOSS_DB: f64 = 1.5;
/// Largest whole-file band correlation drop a width candidate may add.
pub const WIDTH_CORRELATION_DROP: f64 = crate::width::WIDEN_CORRELATION_DROP;
/// Maximum limiter reduction the glue compressor must remove to be kept.
pub const GLUE_RELIEF_DB: f64 = 0.3;

/// `spec` with the drive of the reference render, corrected for the target
/// difference, and the restoring gains of the reference render. A related
/// candidate lands within a probe or two. The compressor and peak control
/// change the tone of each version of a set in nearly the same way, so the
/// restoration of a version starts from that of the reference render
/// (Essential).
pub(crate) fn warm_start(spec: &Spec, reference: Option<&Reference>) -> Spec {
    match reference {
        Some(r) => Spec {
            initial_drive_db: r.drive_db + (spec.target_lufs - r.lufs),
            restoration_db: r.restoration_db.unwrap_or(spec.restoration_db),
            ..spec.clone()
        },
        None => spec.clone(),
    }
}

/// A render after the low-end hold.
pub struct Held {
    pub spec: Spec,
    pub measured: Measured,
    pub steps: Vec<crate::low_end::Step>,
    /// Lowest change of a held band against the reference, less the
    /// intended move, in dB.
    pub worst_db: Option<f64>,
    /// Renders after the first.
    pub renders: usize,
}

/// Hold the bass of a rendered `spec` at each of `targets` (the bass
/// sections and the integrated loudness of a reference render), by up to
/// `low_end::ITERATIONS` hold steps (`set.low-end.v3`), and finish its
/// peak-tone restoration in the same renders. Each render takes the next
/// hold step while the hold fails and the next restoration pass while the
/// render before leaves a change (`treatment_render::next_gains`), up to
/// `RESTORATION_PASSES` passes. `measured` is a render of `spec` with no
/// restoration pass.
#[allow(clippy::too_many_arguments)]
pub fn hold_low_end(
    renderer: &mut Renderer<'_>,
    source: &SourceAnalysis,
    targets: &[(&crate::low_end::Sections, f64)],
    reference_tone: Option<&crate::treatment::Tone>,
    mut spec: Spec,
    mut measured: Measured,
    mut progress: impl FnMut() -> Result<()>,
) -> Result<Held> {
    let sections = targets[0].0;
    // The hold keeps each held band at the intended move of the static low
    // shelves against those of the reference.
    let intended = crate::low_end::intended_against_db(&spec, reference_tone, &source.spectrum);
    // The lowest change against any target, per section and band.
    let changes_of = |m: &Measured| {
        let all: Vec<Vec<[Option<f64>; 2]>> = targets
            .iter()
            .map(|(t, lufs)| {
                crate::low_end::changes(&m.low_sections, m.rendered.lufs, t, *lufs, intended)
            })
            .collect();
        (0..all.iter().map(Vec::len).min().unwrap_or(0))
            .map(|s| {
                std::array::from_fn(|b| all.iter().filter_map(|c| c[s][b]).min_by(f64::total_cmp))
            })
            .collect::<Vec<[Option<f64>; 2]>>()
    };
    let mut changes = changes_of(&measured);
    let mut steps = Vec::new();
    let mut previous: Option<(crate::low_end::Step, Vec<[Option<f64>; 2]>)> = None;
    let first = measured
        .rendered
        .restoration
        .as_ref()
        .map(|r| (r.measured_change_db, r.measured_gains_db, r.renders));
    let (mut renders, mut passes) = (0, 0);
    loop {
        // The restoring gains of the render that the hold measures.
        let current = measured
            .rendered
            .restoration
            .as_ref()
            .map_or(spec.restoration_db, |r| r.gains_db);
        let gains = if passes < RESTORATION_PASSES {
            measured
                .rendered
                .restoration
                .as_ref()
                .and_then(|r| crate::treatment_render::next_gains(r, source.sample_rate))
        } else {
            None
        };
        // The hold plans against the next render: the changes of this
        // render with the shift of the restoration pass that it takes.
        let shift = gains.map_or([0.0; 2], |g| {
            crate::low_end::restoration_shift_db(&source.spectrum, &current, &g)
        });
        let planned: Vec<[Option<f64>; 2]> = changes
            .iter()
            .map(|c| std::array::from_fn(|b| c[b].map(|x| x + shift[b])))
            .collect();
        let hold = if !crate::low_end::passes(&planned) && steps.len() < crate::low_end::ITERATIONS
        {
            crate::low_end::next(
                &spec,
                &crate::low_end::shaped_spectrum(&source.spectrum, &spec),
                sections,
                &planned,
                previous.as_ref().map(|(p, c)| (p, c.as_slice())),
                source.source_frames,
            )
        } else {
            None
        };
        // A step whose bells take the tone past the response bound of
        // `Tone::validate` is not taken.
        let hold =
            hold.filter(|(next, _)| next.effective_tone().0.validate(source.sample_rate).is_ok());
        if hold.is_none() && gains.is_none() {
            break;
        }
        let (mut next, step) = match hold {
            Some((next, step)) => (next, Some(step)),
            None => (spec.clone(), None),
        };
        next.restoration_db = gains.unwrap_or(current);
        if gains.is_some() {
            passes += 1;
        }
        progress()?;
        // The render starts at the drive of the render before; `render_once`
        // corrects it by the change of the prefix loudness.
        let mut start = next.clone();
        start.initial_drive_db = measured.rendered.drive_db
            + measured
                .rendered
                .prefix_lufs
                .zip(renderer.reference_prefix_lufs)
                .map_or(0.0, |(p, r)| p - r);
        // The next render takes the place of this one, whose audio is not
        // read again.
        measured.rendered.audio = Default::default();
        let mut held = measure(source, renderer.render_passes(&start, 0)?)?;
        held.tone = Some(next.effective_tone());
        renders += 1;
        changes = changes_of(&held);
        spec = next;
        measured = held;
        if let Some(step) = step {
            steps.push(step.clone());
            // The measured rise of the step is the change against the plan.
            previous = Some((step, planned));
        }
    }
    if let (Some(r), Some((change, gains, before))) = (&mut measured.rendered.restoration, first) {
        if renders > 0 {
            r.measured_change_db = change;
            r.measured_gains_db = gains;
            r.renders = before + renders;
        }
    }
    Ok(Held {
        spec,
        measured,
        steps,
        worst_db: crate::low_end::worst_db(&changes),
        renders,
    })
}

/// Render candidates in order and keep the first that passes its gates.
/// Returns `None` when every candidate fails. `reference_spec` is the spec
/// of `reference`, for the fallback to the instant attack.
pub fn ladder(
    renderer: &mut Renderer<'_>,
    source: &SourceAnalysis,
    reference: Option<&Reference>,
    reference_spec: Option<&Spec>,
    candidates: &[Candidate],
    mut progress: impl FnMut(usize, &str) -> Result<()>,
) -> Result<(Option<Chosen>, Vec<CandidateTrial>)> {
    renderer.reference_prefix_lufs = reference.and_then(|r| r.prefix_lufs);
    let mut result = ladder_inner(renderer, source, reference, candidates, true, &mut progress);
    let expired = renderer.deadline.as_ref().is_some_and(|d| d.expired);
    // The limiter attack must not cost a version (`set.peak-attack.v1`):
    // when no candidate passes with it, the reference and the candidates
    // render again with the instant attack. The attack lets the start of a
    // loud low onset reach the clipper, which lowers more bass in a version
    // with more bass, so a move against the reference can shrink.
    if let (Ok((None, trials)), false) = (&result, expired) {
        // A version that builds on a fallen-back base has no attack, but its
        // reference can have it.
        if candidates.iter().any(|c| c.spec.limiter.attack_ms > 0.0)
            || reference_spec.is_some_and(|s| s.limiter.attack_ms > 0.0)
        {
            let earlier = trials.clone();
            let instant_reference = match reference_spec {
                Some(spec) if spec.limiter.attack_ms > 0.0 => {
                    let mut spec = spec.clone();
                    spec.limiter.attack_ms = 0.0;
                    renderer.reference_prefix_lufs = None;
                    let mut measured = measure(source, renderer.render(&spec)?)?;
                    measured.tone = Some(spec.effective_tone());
                    // The gates compare the reference's measurements, not its
                    // audio.
                    Some(Reference::of(&measured))
                }
                _ => reference.cloned(),
            };
            let instant: Vec<Candidate> = candidates
                .iter()
                .map(|c| {
                    let mut c = c.clone();
                    c.spec.limiter.attack_ms = 0.0;
                    c.reason = format!("{} with the instant attack", c.reason);
                    c
                })
                .collect();
            renderer.reference_prefix_lufs = instant_reference.as_ref().and_then(|r| r.prefix_lufs);
            result = ladder_inner(
                renderer,
                source,
                instant_reference.as_ref(),
                &instant,
                true,
                &mut progress,
            )
            .map(|(chosen, later)| (chosen, earlier.into_iter().chain(later).collect()));
        }
    }
    renderer.reference_prefix_lufs = None;
    result
}

/// Render one `candidate` against `reference` and keep it when it passes
/// its gates, without the fallback of the instant attack of `ladder`. The
/// render starts at the drive and the restoring gains of `from`, an earlier
/// render of the version, and corrects the drive by the change of the
/// prefix loudness from `from` (D204).
pub(crate) fn one(
    renderer: &mut Renderer<'_>,
    source: &SourceAnalysis,
    reference: Option<&Reference>,
    candidate: &Candidate,
    from: &Rendered,
    progress: impl FnMut(usize, &str) -> Result<()>,
) -> Result<(Option<Chosen>, Vec<CandidateTrial>)> {
    let mut started = candidate.clone();
    started.spec.initial_drive_db = from.drive_db;
    if let Some(r) = &from.restoration {
        started.spec.restoration_db = r.gains_db;
    }
    renderer.reference_prefix_lufs = from.prefix_lufs;
    let result = ladder_inner(
        renderer,
        source,
        reference,
        std::slice::from_ref(&started),
        false,
        progress,
    );
    renderer.reference_prefix_lufs = None;
    result
}

fn ladder_inner(
    renderer: &mut Renderer<'_>,
    source: &SourceAnalysis,
    reference: Option<&Reference>,
    candidates: &[Candidate],
    warm: bool,
    progress: impl FnMut(usize, &str) -> Result<()>,
) -> Result<(Option<Chosen>, Vec<CandidateTrial>)> {
    let result = ladder_candidates(renderer, source, reference, candidates, warm, progress);
    if let Some(deadline) = &mut renderer.deadline {
        deadline.finish();
    }
    if let (Some(record), Ok((_, trials))) = (&mut renderer.trials, &result) {
        record.log(trials);
    }
    result
}

/// The candidates in order, until one passes its gates. With `warm`, each
/// render starts at the drive and the restoring gains of `reference`
/// (`warm_start`); without it, at those of the candidate.
fn ladder_candidates(
    renderer: &mut Renderer<'_>,
    source: &SourceAnalysis,
    reference: Option<&Reference>,
    candidates: &[Candidate],
    warm: bool,
    mut progress: impl FnMut(usize, &str) -> Result<()>,
) -> Result<(Option<Chosen>, Vec<CandidateTrial>)> {
    let start = |spec: &Spec| {
        if warm {
            warm_start(spec, reference)
        } else {
            spec.clone()
        }
    };
    let mut trials = Vec::new();
    let mut prepared: Option<(usize, Measured)> = None;
    let mut index = 0;
    while index < candidates.len() {
        let candidate = &candidates[index];
        if let Some(trial) = renderer
            .trials
            .as_mut()
            .and_then(|r| r.replayed(index, candidate, candidates.get(index + 1)))
        {
            trials.push(trial);
            index += 1;
            continue;
        }
        let tone = candidate.spec.effective_tone();
        let predictions: Vec<imaging::Check> = match reference.and_then(|r| r.tone.as_ref()) {
            Some(reference_tone) => candidate
                .gates
                .tonal
                .iter()
                .map(|gate| {
                    let predicted = predicted_tonal_change_db(source, &tone, reference_tone, gate);
                    imaging::Check::range(
                        "selection.tonal_prediction",
                        "Predicted matched-level change in the moved band against Light, from the tones and the source spectrum",
                        "dB",
                        predicted,
                        Some(gate.minimum_db - PREDICTION_MARGIN_DB),
                        None,
                    )
                })
                .collect(),
            None => Vec::new(),
        };
        if PREDICTION_SKIP && !candidate.gates.glue && predictions.iter().any(|c| !c.passed) {
            trials.push(CandidateTrial {
                index,
                reason: candidate.reason.clone(),
                passed: false,
                checks: predictions,
                lufs: f64::NAN,
                limiter_maximum_db: f64::NAN,
                gain_span_db: f64::NAN,
                attack_loss_p90_db: None,
                energy_attack_loss_p90_db: None,
                low_end: Vec::new(),
            });
            index += 1;
            continue;
        }
        if let Some(check) = renderer
            .deadline
            .as_mut()
            .map(Deadline::start)
            .filter(|c| !c.passed)
        {
            trials.push(CandidateTrial {
                index,
                reason: candidate.reason.clone(),
                passed: false,
                checks: vec![check],
                lufs: f64::NAN,
                limiter_maximum_db: f64::NAN,
                gain_span_db: f64::NAN,
                attack_loss_p90_db: None,
                energy_attack_loss_p90_db: None,
                low_end: Vec::new(),
            });
            return Ok((None, trials));
        }
        progress(index, &candidate.reason)?;
        // A held candidate finishes its restoration in the renders of the
        // hold.
        let holds = candidate.gates.low_end && reference.is_some();
        let passes = if holds { 0 } else { RESTORATION_PASSES };
        let measured = match prepared.take() {
            Some((i, m)) if i == index => m,
            _ => measure(
                source,
                renderer.render_passes(&start(&candidate.spec), passes)?,
            )?,
        };
        let plain = if candidate.gates.glue && index + 1 < candidates.len() {
            progress(index + 1, &candidates[index + 1].reason)?;
            let r = measure(
                source,
                renderer.render(&start(&candidates[index + 1].spec))?,
            )?;
            prepared = Some((index + 1, r));
            prepared.as_ref().map(|(_, m)| m)
        } else {
            None
        };
        let mut measured = measured;
        measured.tone = Some(tone);
        let mut checks = evaluate(&candidate.gates, &measured, reference, plain);
        let mut passed = checks.iter().all(|c| c.passed);
        let mut spec = start(&candidate.spec);
        if holds && !passed {
            // The gates decide on the restored render.
            let rendered = renderer.restore(&spec, measured.rendered, RESTORATION_PASSES)?;
            if let Some(r) = &rendered.restoration {
                spec.restoration_db = r.gains_db;
            }
            measured = measure(source, rendered)?;
            measured.tone = Some(candidate.spec.effective_tone());
            checks = evaluate(&candidate.gates, &measured, reference, plain);
            passed = checks.iter().all(|c| c.passed);
        }
        let mut steps = Vec::new();
        if let (true, true, Some(r)) = (passed, candidate.gates.low_end, reference) {
            let held = hold_low_end(
                renderer,
                source,
                &[(&r.low_sections, r.lufs)],
                r.tone.as_ref().map(|(t, _)| t),
                spec,
                measured,
                || {
                    progress(
                        index,
                        &format!("{} with the low-end hold", candidate.reason),
                    )
                },
            )?;
            spec = held.spec;
            measured = held.measured;
            steps = held.steps;
            if held.renders > 0 {
                checks = evaluate(&candidate.gates, &measured, reference, plain);
            }
            checks.push(imaging::Check::range(
                "selection.low_end",
                "Lowest change of a held bass band in a 15 s section against Natural at matched loudness, less the equal-loudness bass move of the version",
                "dB",
                held.worst_db.or(Some(0.0)),
                Some(-crate::low_end::LIMIT_DB),
                None,
            ));
            passed = checks.iter().all(|c| c.passed);
        }
        if passed && candidate.gates.under_own_point {
            // Peak control of the prefix of the candidate, with the restoring
            // gains of the render, at the drive of the render. A ratio at or
            // under the threshold passes with this one probe; a ratio above
            // it starts the search of the point of the prefix.
            let mut prefix_spec = spec.clone();
            if let Some(r) = &measured.rendered.restoration {
                prefix_spec.restoration_db = r.gains_db;
            }
            // The prefix and its 4x frames stay in the scratch storage of
            // the renderer, and the ratios read the prefix in spans (D228).
            // The render is peak control of this prefix at its drive, so its
            // output gives the ratios of the probe without another render.
            let at_drive = {
                let mut prefix = crate::scratch::Shared(renderer.prepared_frames(&prefix_spec)?.0);
                (measured.rendered.audio.len() == crate::scratch::Frames::len(&prefix))
                    .then(|| {
                        crate::audibility::peak_control_ratios_of(
                            &mut prefix,
                            &mut measured.rendered.audio.frames(),
                            source.sample_rate,
                        )
                    })
                    .transpose()?
                    .map(|ratios| crate::audibility::Probe {
                        drive_db: measured.rendered.drive_db,
                        lufs: measured.rendered.lufs,
                        nmr_db: ratios.total_db,
                        loud_nmr_db: ratios.loud_db,
                        attack_loss_db: measured.preservation.attack_loss_p90_db,
                    })
            };
            // The render waits in scratch while the probes of the prefix
            // render.
            let spilled = if at_drive.is_none_or(|p| p.excess_db() > 0.0) {
                renderer.spill(&mut measured.rendered.audio)?
            } else {
                None
            };
            let (prefix, mut up) = renderer.prepared_frames(&prefix_spec)?;
            let mut prefix = crate::audibility::Prefix::Shared(crate::scratch::Shared(prefix));
            let at_drive = match at_drive {
                Some(probe) => probe,
                None => crate::audibility::probe_on(
                    &mut prefix,
                    &mut up,
                    measured.rendered.drive_db,
                    prefix_spec.clipper_db,
                    prefix_spec.ceiling_dbtp,
                    prefix_spec.limiter,
                    prefix_spec.true_peak,
                    Some(&source.temporal),
                    source.sample_rate,
                )?,
            };
            let point = if at_drive.excess_db() <= 0.0 {
                None
            } else {
                crate::audibility::point_on(
                    &mut prefix,
                    &mut up,
                    measured.rendered.drive_db,
                    prefix_spec.clipper_db,
                    prefix_spec.ceiling_dbtp,
                    prefix_spec.limiter,
                    prefix_spec.true_peak,
                    Some(&source.temporal),
                    source.sample_rate,
                )?
            };
            // A render at the point replaces the render of the candidate.
            let replaced = point
                .as_ref()
                .is_some_and(|p| measured.rendered.lufs - p.lufs > 0.0);
            if let (Some(spilled), false) = (spilled, replaced) {
                spilled.restore(&mut measured.rendered.audio)?;
            }
            checks.push(imaging::Check::range(
                "selection.own_nmr",
                "Masked noise-to-mask ratio of peak control on the prefix of the candidate at its drive",
                "dB",
                Some(at_drive.nmr_db),
                None,
                None,
            ));
            checks.push(imaging::Check::range(
                "selection.own_loud_nmr",
                "Masked noise-to-mask ratio of the loudest 5% of the frames of peak control on the prefix of the candidate at its drive",
                "dB",
                Some(at_drive.loud_nmr_db),
                None,
                None,
            ));
            if let Some(point) = point {
                let over = measured.rendered.lufs - point.lufs;
                if over > 0.0 {
                    // A candidate above its own point renders again at it,
                    // a loudness-search tolerance under, and keeps its gates
                    // (`set.audible-loudness.v6`, D206).
                    let mut at_point = spec.clone();
                    at_point.target_lufs =
                        point.lufs - crate::treatment_render::LOUDNESS_TOLERANCE_LU;
                    at_point.fixed_drive_db = None;
                    at_point.initial_drive_db = measured.rendered.drive_db - over;
                    measured = measure(source, renderer.render(&at_point)?)?;
                    measured.tone = Some(candidate.spec.effective_tone());
                    if let Some(r) = &measured.rendered.restoration {
                        at_point.restoration_db = r.gains_db;
                    }
                    spec = at_point;
                    let kept: Vec<imaging::Check> = checks
                        .iter()
                        .filter(|c| {
                            c.id == "selection.low_end"
                                || c.id == "selection.own_nmr"
                                || c.id == "selection.own_loud_nmr"
                        })
                        .cloned()
                        .collect();
                    checks = evaluate(&candidate.gates, &measured, reference, plain);
                    checks.extend(kept);
                }
                checks.push(imaging::Check::range(
                    "selection.own_point",
                    "Audibility point of the prefix of the candidate less its delivered loudness",
                    "LU",
                    Some(point.lufs - measured.rendered.lufs),
                    Some(-crate::treatment_render::LOUDNESS_TOLERANCE_LU),
                    None,
                ));
            }
            passed = checks.iter().all(|c| c.passed);
        }
        // Predictions are reported beside the measured gates; they do not
        // decide a rendered candidate.
        checks.extend(predictions.into_iter().map(|mut c| {
            c.passed = true;
            c
        }));
        trials.push(CandidateTrial {
            index,
            reason: candidate.reason.clone(),
            passed,
            checks,
            lufs: measured.rendered.lufs,
            limiter_maximum_db: measured.rendered.limiter.maximum_reduction_db,
            gain_span_db: measured.gain_span_db,
            attack_loss_p90_db: measured.preservation.attack_loss_p90_db,
            energy_attack_loss_p90_db: measured.rendered.energy_attack_loss_p90_db,
            low_end: steps,
        });
        if passed {
            let chosen = Chosen {
                index,
                spec,
                measured,
            };
            return Ok((Some(chosen), trials));
        }
        index += 1;
    }
    Ok((None, trials))
}

/// Smallest difference that makes a version distinct, in dB.
pub const DISTINCT_DB: f64 = 0.5;

/// Measured summary of a delivered version for the duplicate screen.
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Signature {
    pub lufs: f64,
    pub band_power: [f64; 6],
    pub band_active: [bool; 6],
    pub side_mid_db: f64,
    pub mean_reduction_db: f64,
}

impl Signature {
    pub fn new(
        output: &crate::Metrics,
        output_stereo: &stereo::StereoAnalysis,
        lufs: f64,
        mean_reduction_db: f64,
    ) -> Self {
        let bands = &output_stereo.whole.bands;
        let material = material_bands(output_stereo);
        Self {
            lufs,
            band_power: std::array::from_fn(|i| bands[i].left_power + bands[i].right_power),
            band_active: material,
            side_mid_db: output.side_mid_db,
            mean_reduction_db,
        }
    }

    /// True when no difference reaches `DISTINCT_DB` at matched loudness: in
    /// any band, in side/mid, or in mean limiter reduction. The delivered
    /// level alone does not make a version distinct: loudness-normalised
    /// playback removes it (D135).
    pub fn is_duplicate_of(&self, other: &Signature) -> bool {
        let matching = other.lufs - self.lufs;
        for i in 0..6 {
            if !(self.band_active[i] && other.band_active[i]) {
                continue;
            }
            if self.band_power[i] < 1e-20 || other.band_power[i] < 1e-20 {
                continue;
            }
            let delta = 10.0 * (self.band_power[i] / other.band_power[i]).log10() + matching;
            if delta.abs() >= DISTINCT_DB {
                return false;
            }
        }
        (self.side_mid_db - other.side_mid_db).abs() < DISTINCT_DB
            && (self.mean_reduction_db - other.mean_reduction_db).abs() < DISTINCT_DB
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_levels_of_one_pass_equal_a_sum_for_each_window() {
        // The sum of each window alone, as before D310.
        let reference = |audio: &[Stereo], rate: u32| -> Vec<f64> {
            let (window, hop) = (3 * rate as usize, rate as usize);
            (0..audio.len().saturating_sub(window))
                .step_by(hop)
                .map(|start| {
                    let energy = audio[start..start + window]
                        .iter()
                        .map(|f| (f[0] as f64).powi(2) + (f[1] as f64).powi(2))
                        .sum::<f64>()
                        / (2.0 * window as f64);
                    db(energy.sqrt())
                })
                .collect()
        };
        let bits = |levels: Vec<f64>| levels.into_iter().map(f64::to_bits).collect::<Vec<_>>();
        let mut seed = 1u32;
        for rate in [1u32, 7, 10, 441] {
            let r = rate as usize;
            for frames in [
                0,
                1,
                3 * r - 1,
                3 * r,
                3 * r + 1,
                4 * r,
                4 * r + 1,
                70 * r + 3,
                140 * r,
            ] {
                let audio: Vec<Stereo> = (0..frames)
                    .map(|i| {
                        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                        let x = (seed >> 8) as f32 / (1 << 24) as f32 - 0.5;
                        // Three silent seconds of each ten seconds.
                        if (3..6).contains(&((i / r) % 10)) {
                            [0.0, 0.0]
                        } else {
                            [x, 0.1 - 0.7 * x]
                        }
                    })
                    .collect();
                assert_eq!(
                    bits(window_levels(&audio, rate)),
                    bits(reference(&audio, rate)),
                    "rate {rate}, {frames} frames"
                );
            }
        }
    }

    #[test]
    fn a_candidate_starts_when_the_longest_candidate_can_end_in_time() {
        let start = Instant::now();
        let at = |s: u64| start + Duration::from_secs(s);
        let mut deadline = Deadline::new(at(100));
        // The first candidate has no estimate.
        assert!(deadline.start_at(at(0)).passed);
        // 30 s left 70 s.
        assert!(deadline.start_at(at(30)).passed);
        // 20 s, and the longest stays 30 s: 50 s are left.
        assert!(deadline.start_at(at(50)).passed);
        deadline.finish_at(at(60));
        let check = deadline.start_at(at(75));
        assert!(!check.passed);
        assert_eq!(check.id, "selection.time_limit");
        assert_eq!(check.measured, Some(25.0));
        assert_eq!(check.minimum, Some(30.0));
        // After a candidate that cannot start, no candidate starts.
        let mut later = Deadline::new(at(100));
        assert!(later.start_at(at(0)).passed);
        assert!(!later.start_at(at(100)).passed);
        assert!(!later.start_at(at(0)).passed);
    }
}
