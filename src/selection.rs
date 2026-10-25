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
    /// prefix of the candidate (`audibility::point`, D131): the Club preset
    /// (D197).
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

#[derive(Clone, Serialize)]
pub struct CandidateTrial {
    pub index: usize,
    pub reason: String,
    pub passed: bool,
    pub checks: Vec<imaging::Check>,
    pub lufs: f64,
    pub limiter_maximum_db: f64,
    pub gain_span_db: f64,
    /// P90 attack loss against the mix (`analysis::compare`). It counts the
    /// tone of the version (D164).
    pub attack_loss_p90_db: Option<f64>,
    /// P90 loss of onset energy against the prefix before the compressor:
    /// the loss of the dynamics stages (`Rendered::energy_attack_loss_p90_db`).
    pub energy_attack_loss_p90_db: Option<f64>,
    /// The renders of the low-end hold, in order.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub low_end: Vec<crate::low_end::Step>,
}

/// A rendered candidate with its measurements.
#[derive(Clone)]
pub struct Measured {
    pub rendered: Rendered,
    pub output: crate::Metrics,
    pub output_stereo: stereo::StereoAnalysis,
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
fn window_levels(audio: &[Stereo], rate: u32) -> Vec<f64> {
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
}

/// P10–P90 span of three-second output-minus-source level over active
/// windows. Both are at `rate`.
pub fn gain_span_db(source: &[Stereo], output: &[Stereo], activity_floor: f64, rate: u32) -> f64 {
    let floor = db(activity_floor.sqrt());
    let a = window_levels(source, rate);
    let b = window_levels(output, rate);
    let gains: Vec<f64> = a
        .iter()
        .zip(&b)
        .filter(|(s, _)| **s > floor)
        .map(|(s, o)| o - s)
        .collect();
    if gains.is_empty() {
        return 0.0;
    }
    analysis::percentile(&gains, 0.9) - analysis::percentile(&gains, 0.1)
}

pub fn measure(input: &[Stereo], source: &SourceAnalysis, rendered: Rendered) -> Result<Measured> {
    let rate = source.sample_rate;
    let output = crate::metrics(&rendered.audio, rate);
    let output_stereo = stereo::analyze(&rendered.audio, rate)?;
    let preservation = analysis::compare(&source.temporal, &rendered.audio, rate);
    let gain_span_db = gain_span_db(input, &rendered.audio, source.temporal.activity_floor, rate);
    let low_sections = crate::low_end::sections(&rendered.audio, rate);
    Ok(Measured {
        rendered,
        output,
        output_stereo,
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
    input: &[Stereo],
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
    // The change of each held band at matched loudness that a restoration
    // pass from `from` to `to` gains adds, from the source spectrum.
    let restoration_shift = |from: &[f64; 6], to: &[f64; 6]| -> [f64; 2] {
        let sum = |gains: &[f64; 6]| {
            crate::treatment_render::filters_of(gains).iter().fold(
                [0.0f64; 2],
                |mut total, filter| {
                    let (held, loudness) = crate::low_end::effect(
                        &source.spectrum,
                        filter,
                        crate::finishing::Domain::Stereo,
                    );
                    for b in 0..2 {
                        total[b] += held[b] - loudness;
                    }
                    total
                },
            )
        };
        let (before, after) = (sum(from), sum(to));
        [after[0] - before[0], after[1] - before[1]]
    };
    let mut changes = changes_of(&measured);
    let mut steps = Vec::new();
    let mut previous: Option<(crate::low_end::Step, Vec<[Option<f64>; 2]>)> = None;
    let first = measured
        .rendered
        .restoration
        .as_ref()
        .map(|r| (r.measured_change_db, r.renders));
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
        let shift = gains.map_or([0.0; 2], |g| restoration_shift(&current, &g));
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
                input.len(),
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
        measured.rendered.audio = std::sync::Arc::new(Vec::new());
        let mut held = measure(input, source, renderer.render_passes(&start, 0)?)?;
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
    if let (Some(r), Some((change, before))) = (&mut measured.rendered.restoration, first) {
        if renders > 0 {
            r.measured_change_db = change;
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
    input: &[Stereo],
    source: &SourceAnalysis,
    reference: Option<&Reference>,
    reference_spec: Option<&Spec>,
    candidates: &[Candidate],
    mut progress: impl FnMut(usize, &str) -> Result<()>,
) -> Result<(Option<Chosen>, Vec<CandidateTrial>)> {
    renderer.reference_prefix_lufs = reference.and_then(|r| r.prefix_lufs);
    let mut result = ladder_inner(
        renderer,
        input,
        source,
        reference,
        candidates,
        &mut progress,
    );
    // The limiter attack must not cost a version (`set.peak-attack.v1`):
    // when no candidate passes with it, the reference and the candidates
    // render again with the instant attack. The attack lets the start of a
    // loud low onset reach the clipper, which lowers more bass in a version
    // with more bass, so a move against the reference can shrink.
    if let Ok((None, trials)) = &result {
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
                    let mut measured = measure(input, source, renderer.render(&spec)?)?;
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
                input,
                source,
                instant_reference.as_ref(),
                &instant,
                &mut progress,
            )
            .map(|(chosen, later)| (chosen, earlier.into_iter().chain(later).collect()));
        }
    }
    renderer.reference_prefix_lufs = None;
    result
}

/// Render one `candidate` against `reference` and keep it when it passes
/// its gates, without the fallback of the instant attack of `ladder`.
pub(crate) fn one(
    renderer: &mut Renderer<'_>,
    input: &[Stereo],
    source: &SourceAnalysis,
    reference: Option<&Reference>,
    candidate: &Candidate,
    progress: impl FnMut(usize, &str) -> Result<()>,
) -> Result<(Option<Chosen>, Vec<CandidateTrial>)> {
    renderer.reference_prefix_lufs = reference.and_then(|r| r.prefix_lufs);
    let result = ladder_inner(
        renderer,
        input,
        source,
        reference,
        std::slice::from_ref(candidate),
        progress,
    );
    renderer.reference_prefix_lufs = None;
    result
}

fn ladder_inner(
    renderer: &mut Renderer<'_>,
    input: &[Stereo],
    source: &SourceAnalysis,
    reference: Option<&Reference>,
    candidates: &[Candidate],
    mut progress: impl FnMut(usize, &str) -> Result<()>,
) -> Result<(Option<Chosen>, Vec<CandidateTrial>)> {
    let mut trials = Vec::new();
    let mut prepared: Option<(usize, Measured)> = None;
    let mut index = 0;
    while index < candidates.len() {
        let candidate = &candidates[index];
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
        progress(index, &candidate.reason)?;
        // A held candidate finishes its restoration in the renders of the
        // hold.
        let holds = candidate.gates.low_end && reference.is_some();
        let passes = if holds { 0 } else { RESTORATION_PASSES };
        let measured = match prepared.take() {
            Some((i, m)) if i == index => m,
            _ => measure(
                input,
                source,
                renderer.render_passes(&warm_start(&candidate.spec, reference), passes)?,
            )?,
        };
        let plain = if candidate.gates.glue && index + 1 < candidates.len() {
            progress(index + 1, &candidates[index + 1].reason)?;
            let r = measure(
                input,
                source,
                renderer.render(&warm_start(&candidates[index + 1].spec, reference))?,
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
        let mut spec = warm_start(&candidate.spec, reference);
        if holds && !passed {
            // The gates decide on the restored render.
            let rendered = renderer.restore(&spec, measured.rendered, RESTORATION_PASSES)?;
            if let Some(r) = &rendered.restoration {
                spec.restoration_db = r.gains_db;
            }
            measured = measure(input, source, rendered)?;
            measured.tone = Some(candidate.spec.effective_tone());
            checks = evaluate(&candidate.gates, &measured, reference, plain);
            passed = checks.iter().all(|c| c.passed);
        }
        let mut steps = Vec::new();
        if let (true, true, Some(r)) = (passed, candidate.gates.low_end, reference) {
            let held = hold_low_end(
                renderer,
                input,
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
            let prefix = renderer.prepared_prefix(&prefix_spec)?;
            // The render is peak control of this prefix at its drive, so its
            // output gives the ratios of the probe without another render.
            let at_drive = if measured.rendered.audio.len() == prefix.len() {
                let ratios = crate::audibility::peak_control_ratios(
                    &prefix,
                    &measured.rendered.audio,
                    source.sample_rate,
                );
                crate::audibility::Probe {
                    drive_db: measured.rendered.drive_db,
                    lufs: measured.rendered.lufs,
                    nmr_db: ratios.total_db,
                    loud_nmr_db: ratios.loud_db,
                    attack_loss_db: measured.preservation.attack_loss_p90_db,
                }
            } else {
                crate::audibility::probe(
                    &prefix,
                    measured.rendered.drive_db,
                    prefix_spec.clipper_db,
                    prefix_spec.ceiling_dbtp,
                    prefix_spec.limiter,
                    prefix_spec.true_peak,
                    Some(&source.temporal),
                    source.sample_rate,
                )?
            };
            let point = if at_drive.excess_db() <= 0.0 {
                None
            } else {
                crate::audibility::point(
                    &prefix,
                    measured.rendered.drive_db,
                    prefix_spec.clipper_db,
                    prefix_spec.ceiling_dbtp,
                    prefix_spec.limiter,
                    prefix_spec.true_peak,
                    Some(&source.temporal),
                    source.sample_rate,
                )?
            };
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
                    measured = measure(input, source, renderer.render(&at_point)?)?;
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
