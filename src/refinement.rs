//! Source-specific focus amounts, peak recovery and slow passage envelopes.
//! Stems are measurement inputs only. Candidate output always uses the stereo mix.
use crate::{
    analysis::percentile,
    audio_io::{Result, Stereo},
    dsp::EqBand,
    finishing::{self, Domain},
    resample,
    source_treatments::ProposalSet,
    stem_evidence::{self, Evidence},
    treatment::{BlendPoint, FocusEnvelope, Tone},
};
use serde::Serialize;

/// Seconds of a passage.
const PASSAGE_SECONDS: usize = 2;
const GAINS: [f64; 9] = [0.0, 0.75, 1.5, 2.25, 3.0, 3.75, 4.5, 5.25, 6.0];

#[derive(Clone, Serialize)]
pub struct Timing {
    pub rule: &'static str,
    pub selected_release_ms: f64,
    pub measured_decay_ms: Option<f64>,
    pub measured_interval_ms: Option<f64>,
    pub bass_two_cycles_ms: Option<f64>,
    pub reason: String,
}

#[derive(Clone, Serialize)]
pub struct Passage {
    pub start_sample: usize,
    pub end_sample: usize,
    pub reconstruction_residual_db: f64,
    pub reconstruction_cosine: f64,
    pub reliable: bool,
    pub vocal_active: bool,
    pub vocal_power: f64,
    pub accompaniment_power: f64,
}

#[derive(Clone, Serialize)]
pub struct Probe {
    pub gain_db: f64,
    pub median_ratio_improvement_db: f64,
}

#[derive(Clone, Serialize)]
pub struct Focus {
    pub rule: &'static str,
    pub domain: Domain,
    pub filter: EqBand,
    pub eligible_seconds: f64,
    pub probes: Vec<Probe>,
    pub selected_ratio_improvement_db: Option<f64>,
    pub envelope_rule: &'static str,
    pub envelope: Vec<BlendPoint>,
    pub reason: String,
}

#[derive(Clone, Serialize)]
pub struct Analysis {
    pub rule: &'static str,
    pub validation_status: &'static str,
    /// The render rate of the passages and the filters, in Hz.
    pub sample_rate: u32,
    pub source_pcm_sha256: String,
    pub stem_pcm_sha256: [String; 4],
    pub timing: Timing,
    pub passages: Vec<Passage>,
    pub focus: Vec<Focus>,
}

pub fn timing(evidence: &Evidence) -> Timing {
    let cycles = evidence.bass_body_hz.map(|hz| 2000.0 / hz);
    let valid = evidence.usable && evidence.drum_decay_events >= 16;
    let measured = valid.then_some(evidence.drum_decay_p50_ms).flatten();
    let interval = valid.then_some(evidence.drum_interval_p50_ms).flatten();
    // Match an exponential recovery time constant to the observed 6 dB decay.
    // Two bass cycles set the floor of the release and half the median drum interval sets its ceiling.
    let selected = measured.zip(interval).map(|(decay, interval)| {
        let floor = cycles.unwrap_or(25.0).clamp(25.0, 160.0);
        let ceiling = (interval * 0.5).clamp(floor, 160.0);
        (decay / (6.0 * std::f64::consts::LN_10 / 20.0)).clamp(floor, ceiling)
    });
    Timing {
        rule: "set.stem-recovery.v1",
        selected_release_ms: selected.unwrap_or(50.0),
        measured_decay_ms: measured,
        measured_interval_ms: interval,
        bass_two_cycles_ms: cycles,
        reason: selected.map_or_else(
            || "Retain the 50 ms study timing: insufficient reliable drum decay/interval evidence".into(),
            |ms| format!("{ms:.3} ms from measured six-dB drum decay, within 25–160 ms; two bass cycles set the floor and half the median event interval sets the ceiling, with the bass floor taking precedence if they conflict"),
        ),
    }
}

/// The band powers of each passage of the vocal and the accompaniment, both
/// at `rate`, after `filter`.
fn band_powers(
    vocal: &[Stereo],
    accompaniment: &[Stereo],
    domain: Domain,
    filter: EqBand,
    rate: u32,
) -> Vec<[f64; 2]> {
    let window = PASSAGE_SECONDS * rate as usize;
    let mut eq = [filter.filter(rate); 2];
    let mut bp = [finishing::bandpass(filter.hz, filter.q, rate); 4];
    let mut powers = vec![[0.0; 2]; vocal.len().div_ceil(window)];
    for (i, (v, a)) in vocal.iter().zip(accompaniment).enumerate() {
        for (role, frame) in [v, a].iter().enumerate() {
            let [l, r] = frame.map(f64::from);
            let (mut m, mut s) = ((l + r) * 0.5, (l - r) * 0.5);
            match domain {
                Domain::Mid => m = eq[role].tick(m),
                Domain::Side => s = eq[role].tick(s),
                Domain::Stereo => unreachable!("Focus proposals use mid or side"),
            }
            let m = bp[role * 2].tick(m);
            let s = bp[role * 2 + 1].tick(s);
            powers[i / window][role] += m * m + s * s;
        }
    }
    powers
}

fn ratio(p: [f64; 2]) -> f64 {
    10.0 * (p[0].max(1e-30) / p[1].max(1e-30)).log10()
}

fn focus(
    vocal: &[Stereo],
    accompaniment: &[Stereo],
    passages: &[Passage],
    domain: Domain,
    original: EqBand,
    rate: u32,
) -> Focus {
    let base = band_powers(
        vocal,
        accompaniment,
        domain,
        EqBand {
            gain_db: 0.0,
            ..original
        },
        rate,
    );
    let eligible: Vec<_> = passages
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            (p.reliable
                && p.vocal_active
                && base[i][0] > p.vocal_power * 0.001
                && ratio(base[i]) <= 0.0)
                .then_some(i)
        })
        .collect();
    let eligible_seconds = eligible
        .iter()
        .map(|&i| (passages[i].end_sample - passages[i].start_sample) as f64 / rate as f64)
        .sum();
    let mut result = Focus {
        rule: "set.focus-strength.v1",
        domain,
        filter: original,
        eligible_seconds,
        probes: Vec::new(),
        selected_ratio_improvement_db: None,
        envelope_rule: "set.focus-passages.v1",
        envelope: Vec::new(),
        reason: "Retain the study strength: insufficient reliable vocal-overlap passages".into(),
    };
    if eligible_seconds < 10.0 {
        return result;
    }
    for gain in GAINS {
        let after = band_powers(
            vocal,
            accompaniment,
            domain,
            EqBand {
                gain_db: gain,
                ..original
            },
            rate,
        );
        let changes: Vec<_> = eligible
            .iter()
            .map(|&i| ratio(after[i]) - ratio(base[i]))
            .collect();
        result.probes.push(Probe {
            gain_db: gain,
            median_ratio_improvement_db: percentile(&changes, 0.5),
        });
    }
    result.selected_ratio_improvement_db = result
        .probes
        .iter()
        .find(|p| p.gain_db == original.gain_db)
        .map(|p| p.median_ratio_improvement_db);
    let maximum = result
        .probes
        .iter()
        .map(|p| p.median_ratio_improvement_db)
        .fold(0.0, f64::max);
    if maximum < 0.1 {
        result.reason = "Retain the study strength: the exact-filter diagnostic provides insufficient positive strength evidence".into();
        return result;
    }
    // Smallest interpolated gain reaching 80% of the measured bounded response.
    // The actual selected filter is measured again, rather than reporting interpolation.
    let target = maximum * 0.8;
    let pair = result
        .probes
        .windows(2)
        .find(|p| p[1].median_ratio_improvement_db >= target)
        .unwrap();
    let fraction = ((target - pair[0].median_ratio_improvement_db)
        / (pair[1].median_ratio_improvement_db - pair[0].median_ratio_improvement_db))
        .clamp(0.0, 1.0);
    result.filter.gain_db = pair[0].gain_db + fraction * (pair[1].gain_db - pair[0].gain_db);
    let selected = band_powers(vocal, accompaniment, domain, result.filter, rate);
    let improvements: Vec<_> = eligible
        .iter()
        .map(|&i| ratio(selected[i]) - ratio(base[i]))
        .collect();
    result.selected_ratio_improvement_db = Some(percentile(&improvements, 0.5));
    result.reason = format!("{:.3} dB reaches approximately 80% of the measured 0–6 dB exact-filter response; achieved median overlap-ratio change {:.3} dB in {:.1} reliable seconds. Diagnostic before limiting, not a quality or audibility score", result.filter.gain_db, result.selected_ratio_improvement_db.unwrap(), eligible_seconds);
    // Turn down only the focus contribution, not the entire mix, outside supported
    // vocal-overlap passages. Slow raised-cosine transitions preserve filter state.
    let needs: Vec<_> = eligible.iter().map(|&i| -ratio(base[i])).collect();
    let reference_need = percentile(&needs, 0.75).max(1.0);
    let wet: Vec<_> = passages
        .iter()
        .enumerate()
        .map(|(i, p)| {
            if !p.reliable
                || !p.vocal_active
                || base[i][0] <= p.vocal_power * 0.001
                || ratio(selected[i]) - ratio(base[i]) < 0.05
            {
                return 0.0;
            }
            (-ratio(base[i]) / reference_need).clamp(0.0, 1.0)
        })
        .collect();
    let variation = percentile(&wet, 0.9) - percentile(&wet, 0.1);
    if variation >= 0.2 {
        result.envelope.push(BlendPoint {
            sample: 0,
            wet: wet[0],
        });
        for (p, &amount) in passages.iter().zip(&wet) {
            let sample = (p.start_sample + p.end_sample) / 2;
            if sample > 0 && sample < vocal.len() - 1 {
                result.envelope.push(BlendPoint {
                    sample,
                    wet: amount,
                });
            }
        }
        result.envelope.push(BlendPoint {
            sample: vocal.len() - 1,
            wet: *wet.last().unwrap(),
        });
    }
    result
}

/// Analyze once while the host has aligned raw estimates in memory. Existing
/// source focus regions provide hypotheses; actual band-filter responses calibrate
/// their strength. No stem or reconstructed sum becomes a rendered master.
/// The stems are at `stem_rate` and `input` at the render rate `rate`.
pub fn analyze(
    input: &[Stereo],
    stems: [&[Stereo]; 4],
    stem_rate: u32,
    evidence: &Evidence,
    proposals: Option<&ProposalSet>,
    rate: u32,
) -> Result<Analysis> {
    validate_binding(input, stems, stem_rate, evidence)?;
    let sum: Vec<Stereo> = (0..stems[0].len())
        .map(|i| [0, 1].map(|c| stems.iter().map(|s| s[i][c]).sum()))
        .collect();
    let reconstructed = resample::convert(&sum, stem_rate, rate)?;
    analyze_with_reconstruction(
        input,
        stems,
        stem_rate,
        evidence,
        proposals,
        &reconstructed,
        rate,
    )
}

fn validate_binding(
    input: &[Stereo],
    stems: [&[Stereo]; 4],
    rate: u32,
    evidence: &Evidence,
) -> Result<()> {
    if input.len() != evidence.source_frames
        || stem_evidence::fingerprint(input) != evidence.source_pcm_sha256
        || rate != evidence.stem_sample_rate
        || stems.map(stem_evidence::fingerprint) != evidence.stem_pcm_sha256
    {
        return Err("Refinement evidence does not match source/stem PCM".into());
    }
    Ok(())
}

/// `analyze` with the stem sum at the render rate `rate`.
pub fn analyze_with_reconstruction(
    input: &[Stereo],
    stems: [&[Stereo]; 4],
    stem_rate: u32,
    evidence: &Evidence,
    proposals: Option<&ProposalSet>,
    reconstructed: &[Stereo],
    rate: u32,
) -> Result<Analysis> {
    validate_binding(input, stems, stem_rate, evidence)?;
    analyze_owned(
        input,
        crate::audio_io::Source {
            audio: stems[3].to_vec(),
            sample_rate: stem_rate,
        },
        evidence,
        proposals,
        reconstructed.to_vec(),
        rate,
    )
}

pub(crate) fn analyze_owned(
    input: &[Stereo],
    raw_vocal: crate::audio_io::Source,
    evidence: &Evidence,
    proposals: Option<&ProposalSet>,
    reconstructed: Vec<Stereo>,
    rate: u32,
) -> Result<Analysis> {
    if input.len() != evidence.source_frames
        || stem_evidence::fingerprint(input) != evidence.source_pcm_sha256
        || raw_vocal.sample_rate != evidence.stem_sample_rate
        || stem_evidence::fingerprint(&raw_vocal.audio) != evidence.stem_pcm_sha256[3]
    {
        return Err("Refinement evidence does not match source/stem PCM".into());
    }
    if stem_evidence::fingerprint(&reconstructed) != evidence.reconstruction_pcm_sha256 {
        return Err("Refinement reconstruction fingerprint differs".into());
    }
    let vocal = if evidence.usable {
        resample::convert_owned(raw_vocal.audio, raw_vocal.sample_rate, rate)?
    } else {
        Vec::new()
    };
    analyze_matched(input, vocal, evidence, proposals, reconstructed, rate)
}

/// `analyze_owned` of the PCM that `evidence` was measured from, with the
/// vocal stem at the render rate `rate`.
pub(crate) fn analyze_matched(
    input: &[Stereo],
    vocal: Vec<Stereo>,
    evidence: &Evidence,
    proposals: Option<&ProposalSet>,
    mut reconstructed: Vec<Stereo>,
    rate: u32,
) -> Result<Analysis> {
    let mut result = Analysis {
        rule: "set.source-refinement.v1",
        validation_status: "provisional-requires-listening",
        sample_rate: rate,
        source_pcm_sha256: evidence.source_pcm_sha256.clone(),
        stem_pcm_sha256: evidence.stem_pcm_sha256.clone(),
        timing: timing(evidence),
        passages: Vec::new(),
        focus: Vec::new(),
    };
    if !evidence.usable {
        return Ok(result);
    }
    let mut vocal = vocal;
    let frames = vocal.len();
    if frames.abs_diff(input.len()) > 1 || reconstructed.len() != frames {
        return Err("Refinement stem geometry differs".into());
    }
    for (frame, voice) in reconstructed.iter_mut().zip(&vocal) {
        for channel in 0..2 {
            frame[channel] -= voice[channel];
        }
    }
    let mut accompaniment = reconstructed;
    vocal.reserve_exact(input.len().saturating_sub(vocal.len()));
    accompaniment.reserve_exact(input.len().saturating_sub(accompaniment.len()));
    vocal.resize(input.len(), [0.0; 2]);
    accompaniment.resize(input.len(), [0.0; 2]);
    let window = PASSAGE_SECONDS * rate as usize;
    for start in (0..input.len()).step_by(window) {
        let end = (start + window).min(input.len());
        let (mut xx, mut yy, mut xy, mut residual, mut v, mut a) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        for i in start..end {
            for c in 0..2 {
                let x = input[i][c] as f64;
                let y = vocal[i][c] as f64 + accompaniment[i][c] as f64;
                xx += x * x;
                yy += y * y;
                xy += x * y;
                residual += (x - y).powi(2);
                v += (vocal[i][c] as f64).powi(2) * 0.5;
                a += (accompaniment[i][c] as f64).powi(2) * 0.5;
            }
        }
        let residual_db = 10.0 * (residual / xx.max(1e-30)).max(1e-30).log10();
        let cosine = xy / (xx * yy).sqrt().max(1e-30);
        result.passages.push(Passage {
            start_sample: start,
            end_sample: end,
            reconstruction_residual_db: residual_db,
            reconstruction_cosine: cosine,
            reliable: xx / (end - start) as f64 > 1e-10 && residual_db <= -18.0 && cosine >= 0.98,
            vocal_active: false,
            vocal_power: v,
            accompaniment_power: a,
        });
    }
    let levels: Vec<_> = result
        .passages
        .iter()
        .map(|p| p.vocal_power / (p.end_sample - p.start_sample) as f64)
        .collect();
    let gate = percentile(&levels, 0.95) * 10f64.powf(-2.5);
    for (p, level) in result.passages.iter_mut().zip(levels) {
        p.vocal_active =
            level > gate && p.vocal_power > (p.vocal_power + p.accompaniment_power) * 0.02;
    }
    if let Some(proposals) = proposals {
        for proposal in &proposals.proposals {
            for (domain, filters) in [
                (Domain::Mid, &proposal.tone.mid),
                (Domain::Side, &proposal.tone.side),
            ] {
                for filter in filters {
                    if result.focus.iter().any(|f| {
                        same_domain(f.domain, domain)
                            && f.filter.hz == filter.hz
                            && f.filter.q == filter.q
                    }) {
                        continue;
                    }
                    result.focus.push(focus(
                        &vocal,
                        &accompaniment,
                        &result.passages,
                        domain,
                        *filter,
                        rate,
                    ));
                }
            }
        }
    }
    Ok(result)
}

fn same_domain(a: Domain, b: Domain) -> bool {
    std::mem::discriminant(&a) == std::mem::discriminant(&b)
}

impl Analysis {
    /// Fit measured amounts and passage envelopes to a complete proposal. Combined
    /// boosts share the existing 9 dB response bound; their gains scale together.
    pub fn apply(&self, tone: &Tone) -> Result<(Tone, Vec<FocusEnvelope>, f64)> {
        let mut output = tone.clone();
        let mut envelopes = Vec::new();
        for (domain, bands) in [
            (Domain::Mid, &mut output.mid),
            (Domain::Side, &mut output.side),
        ] {
            for (index, band) in bands.iter_mut().enumerate() {
                if let Some(shape) = self.focus.iter().find(|f| {
                    same_domain(f.domain, domain) && f.filter.hz == band.hz && f.filter.q == band.q
                }) {
                    band.gain_db = shape.filter.gain_db;
                    if !shape.envelope.is_empty() {
                        envelopes.push(FocusEnvelope {
                            domain,
                            filter_index: index,
                            points: shape.envelope.clone(),
                        });
                    }
                }
            }
        }
        let mut scale = 1.0;
        while output.validate(self.sample_rate).is_err() && scale > 0.1 {
            scale *= 0.95;
            for bands in [&mut output.mid, &mut output.side] {
                for band in bands {
                    band.gain_db *= 0.95;
                }
            }
        }
        output.validate(self.sample_rate)?;
        Ok((output, envelopes, scale))
    }
}
