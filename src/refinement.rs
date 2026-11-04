//! Source-specific focus amounts, peak recovery and the passage share of
//! each focus bell. Stems are measurement inputs only. Candidate output
//! always uses the stereo mix.
use crate::{
    analysis::percentile,
    audio_io::{Result, Stereo},
    dsp::{pair, Biquad, EqBand},
    finishing::{self, Domain},
    resample,
    scratch::{Frames, Slice},
    source_treatments::ProposalSet,
    stem_evidence::{self, Evidence},
    treatment::Tone,
};
use serde::{Deserialize, Serialize};

/// Seconds of a passage.
const PASSAGE_SECONDS: usize = 2;
const GAINS: [f64; 9] = [0.0, 0.75, 1.5, 2.25, 3.0, 3.75, 4.5, 5.25, 6.0];
/// Frames of each read of the vocal for the accompaniment.
const SPAN: usize = 65_536;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Timing {
    #[serde(deserialize_with = "crate::static_text::text")]
    pub rule: &'static str,
    pub selected_release_ms: f64,
    pub measured_decay_ms: Option<f64>,
    pub measured_interval_ms: Option<f64>,
    pub bass_two_cycles_ms: Option<f64>,
    pub reason: String,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
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

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Probe {
    pub gain_db: f64,
    pub median_ratio_improvement_db: f64,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Focus {
    #[serde(deserialize_with = "crate::static_text::text")]
    pub rule: &'static str,
    pub domain: Domain,
    /// The delivered bell.
    pub filter: EqBand,
    pub eligible_seconds: f64,
    pub probes: Vec<Probe>,
    /// The gain of `set.focus-strength.v1`, before the passage share.
    pub strength_gain_db: Option<f64>,
    pub selected_ratio_improvement_db: Option<f64>,
    #[serde(deserialize_with = "crate::static_text::text")]
    pub passage_rule: &'static str,
    /// The mean wet share of the passages, weighted by their frames.
    pub mean_wet: Option<f64>,
    pub reason: String,
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Analysis {
    #[serde(deserialize_with = "crate::static_text::text")]
    pub rule: &'static str,
    #[serde(deserialize_with = "crate::static_text::text")]
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
/// at `rate`, after the bell of each of `bells`, in one read of `vocal`. Each
/// bell keeps its own filters, so its powers are those of a read for that
/// bell alone.
fn band_powers(
    vocal: &mut impl Frames,
    accompaniment: &mut impl Frames,
    bells: &[(Domain, EqBand)],
    rate: u32,
) -> Result<Vec<Vec<[f64; 2]>>> {
    let window = PASSAGE_SECONDS * rate as usize;
    let filters: Vec<_> = bells
        .iter()
        .map(|(_, filter)| {
            (
                filter.filter(rate),
                finishing::bandpass(filter.hz, filter.q, rate),
            )
        })
        .collect();
    let mut states = vec![[[0.0; 2]; 6]; bells.len()];
    let mut powers = vec![vec![[0.0; 2]; accompaniment.len().div_ceil(window)]; bells.len()];
    if bells.is_empty() {
        return Ok(powers);
    }
    let (mut span, mut other) = (Vec::new(), Vec::new());
    for (passage, start) in (0..accompaniment.len()).step_by(window).enumerate() {
        let end = (start + window).min(accompaniment.len());
        vocal.read(start, end - start, &mut span)?;
        accompaniment.read(start, end - start, &mut other)?;
        for ((((domain, _), &(eq, bp)), states), powers) in
            bells.iter().zip(&filters).zip(&mut states).zip(&mut powers)
        {
            let audio = (&span[..], &other[..]);
            powers[passage] = passage_powers(audio, *domain, eq, bp, states);
        }
    }
    Ok(powers)
}

/// The band powers of one passage of the vocal and the accompaniment after
/// the bell `eq` in `domain` and the band `bp`. `states` holds the state of
/// each filter at the end of the passage before: the bell of the vocal and
/// of the accompaniment, then the band of the mid and of the side of the
/// vocal and of the accompaniment.
// The filters take their coefficients from `eq` and `bp`, and the function
// compiles on its own (D234). The vocal and the accompaniment take the same
// filters, so lane 0 of each pair holds the vocal and lane 1 the
// accompaniment: three filters of two lanes take the place of six (D321).
#[inline(never)]
fn passage_powers(
    (vocal, accompaniment): (&[Stereo], &[Stereo]),
    domain: Domain,
    eq: Biquad,
    bp: Biquad,
    states: &mut [[f64; 2]; 6],
) -> [f64; 2] {
    let mut eq = pair::Biquad::new(eq, [states[0], states[1]]);
    let mut bp_mid = pair::Biquad::new(bp, [states[2], states[4]]);
    let mut bp_side = pair::Biquad::new(bp, [states[3], states[5]]);
    let half = pair::splat(0.5);
    let mut powers = pair::splat(0.0);
    for (v, a) in vocal.iter().zip(accompaniment) {
        let l = pair::new(v[0] as f64, a[0] as f64);
        let r = pair::new(v[1] as f64, a[1] as f64);
        let (mut m, mut s) = (
            pair::mul(pair::add(l, r), half),
            pair::mul(pair::sub(l, r), half),
        );
        match domain {
            Domain::Mid => m = eq.tick(m),
            Domain::Side => s = eq.tick(s),
            Domain::Stereo => unreachable!("Focus proposals use mid or side"),
        }
        let m = bp_mid.tick(m);
        let s = bp_side.tick(s);
        powers = pair::add(powers, pair::add(pair::mul(m, m), pair::mul(s, s)));
    }
    let ([eq_vocal, eq_accompaniment], [mid_vocal, mid_accompaniment]) =
        (eq.states(), bp_mid.states());
    let [side_vocal, side_accompaniment] = bp_side.states();
    *states = [
        eq_vocal,
        eq_accompaniment,
        mid_vocal,
        side_vocal,
        mid_accompaniment,
        side_accompaniment,
    ];
    pair::values(powers)
}

fn ratio(p: [f64; 2]) -> f64 {
    10.0 * (p[0].max(1e-30) / p[1].max(1e-30)).log10()
}

/// The measures of one bell of `focus` between its rounds.
struct Measuring {
    domain: Domain,
    original: EqBand,
    base: Vec<[f64; 2]>,
    eligible: Vec<usize>,
    result: Focus,
    /// The bell takes the next round.
    open: bool,
}

/// The focus of each of `bells` (`set.focus-strength.v1`). Each round reads
/// `vocal` once for the bells that take it: the bell at 0 dB, the probes,
/// the bell at its strength and the delivered bell (D234).
fn focus(
    vocal: &mut impl Frames,
    accompaniment: &mut impl Frames,
    passages: &[Passage],
    bells: &[(Domain, EqBand)],
    rate: u32,
) -> Result<Vec<Focus>> {
    let at = |bell: &Measuring, gain_db| {
        (
            bell.domain,
            EqBand {
                gain_db,
                ..bell.original
            },
        )
    };
    let zero: Vec<_> = bells
        .iter()
        .map(|&(domain, original)| {
            (
                domain,
                EqBand {
                    gain_db: 0.0,
                    ..original
                },
            )
        })
        .collect();
    let bases = band_powers(vocal, accompaniment, &zero, rate)?;
    let mut measuring: Vec<Measuring> = bells
        .iter()
        .zip(bases)
        .map(|(&(domain, original), base)| {
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
            Measuring {
                domain,
                original,
                base,
                eligible,
                open: eligible_seconds >= 10.0,
                result: Focus {
                    rule: "set.focus-strength.v1",
                    domain,
                    filter: original,
                    eligible_seconds,
                    probes: Vec::new(),
                    strength_gain_db: None,
                    selected_ratio_improvement_db: None,
                    passage_rule: "set.focus-passages.v2",
                    mean_wet: None,
                    reason:
                        "Retain the study strength: insufficient reliable vocal-overlap passages"
                            .into(),
                },
            }
        })
        .collect();
    let probes: Vec<_> = measuring
        .iter()
        .filter(|b| b.open)
        .flat_map(|b| GAINS.map(|gain| at(b, gain)))
        .collect();
    let mut afters = band_powers(vocal, accompaniment, &probes, rate)?.into_iter();
    for bell in measuring.iter_mut().filter(|b| b.open) {
        for gain in GAINS {
            let after = afters.next().unwrap();
            let changes: Vec<_> = bell
                .eligible
                .iter()
                .map(|&i| ratio(after[i]) - ratio(bell.base[i]))
                .collect();
            bell.result.probes.push(Probe {
                gain_db: gain,
                median_ratio_improvement_db: percentile(&changes, 0.5),
            });
        }
        let result = &mut bell.result;
        result.selected_ratio_improvement_db = result
            .probes
            .iter()
            .find(|p| p.gain_db == bell.original.gain_db)
            .map(|p| p.median_ratio_improvement_db);
        let maximum = result
            .probes
            .iter()
            .map(|p| p.median_ratio_improvement_db)
            .fold(0.0, f64::max);
        if maximum < 0.1 {
            result.reason = "Retain the study strength: the exact-filter diagnostic provides insufficient positive strength evidence".into();
            bell.open = false;
            continue;
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
        result.strength_gain_db =
            Some(pair[0].gain_db + fraction * (pair[1].gain_db - pair[0].gain_db));
    }
    let strengths: Vec<_> = measuring
        .iter()
        .filter(|b| b.open)
        .map(|b| at(b, b.result.strength_gain_db.unwrap()))
        .collect();
    let mut selected = band_powers(vocal, accompaniment, &strengths, rate)?.into_iter();
    for bell in measuring.iter_mut().filter(|b| b.open) {
        let selected = selected.next().unwrap();
        let (base, strength) = (&bell.base, bell.result.strength_gain_db.unwrap());
        // The share of each passage that needs the bell: none outside supported
        // vocal-overlap passages, and the deficit against the P75 deficit inside
        // them.
        let needs: Vec<_> = bell.eligible.iter().map(|&i| -ratio(base[i])).collect();
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
        let frames: Vec<f64> = passages
            .iter()
            .map(|p| (p.end_sample - p.start_sample) as f64)
            .collect();
        let mean_wet = wet.iter().zip(&frames).map(|(w, f)| w * f).sum::<f64>()
            / frames.iter().sum::<f64>().max(1.0);
        bell.result.mean_wet = Some(mean_wet);
        bell.result.filter.gain_db =
            static_gain_db(bell.domain, bell.original.hz, strength, mean_wet);
    }
    let delivered: Vec<_> = measuring
        .iter()
        .filter(|b| b.open)
        .map(|b| (b.domain, b.result.filter))
        .collect();
    let mut delivered = band_powers(vocal, accompaniment, &delivered, rate)?.into_iter();
    for bell in measuring.iter_mut().filter(|b| b.open) {
        let delivered = delivered.next().unwrap();
        let result = &mut bell.result;
        let improvements: Vec<_> = bell
            .eligible
            .iter()
            .map(|&i| ratio(delivered[i]) - ratio(bell.base[i]))
            .collect();
        result.selected_ratio_improvement_db = Some(percentile(&improvements, 0.5));
        result.reason = format!("{:.3} dB reaches approximately 80% of the measured 0–6 dB exact-filter response; at the mean passage share {:.3}, the static bell is {:.3} dB with a median overlap-ratio change of {:.3} dB in {:.1} reliable seconds. Diagnostic before limiting, not a quality or audibility score", result.strength_gain_db.unwrap(), result.mean_wet.unwrap(), result.filter.gain_db, result.selected_ratio_improvement_db.unwrap(), result.eligible_seconds);
    }
    Ok(measuring.into_iter().map(|b| b.result).collect())
}

/// The gain of a static bell whose centre response is the mean blend of the
/// dry mix and the bell at `strength_db`, with `mean_wet` the share of the
/// bell (D220). A side bell under `set::LOW_SIDE_BELL_HZ` takes its bound
/// before the blend, as the versions carried it with its envelope (D128).
pub(crate) fn static_gain_db(domain: Domain, hz: f64, strength_db: f64, mean_wet: f64) -> f64 {
    let bound = if matches!(domain, Domain::Side) && hz < crate::set::LOW_SIDE_BELL_HZ {
        crate::set::LOW_SIDE_BELL_DB
    } else {
        f64::INFINITY
    };
    let gain = 10f64.powf(strength_db.clamp(-bound, bound) / 20.0);
    20.0 * (1.0 + mean_wet * (gain - 1.0)).log10()
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

/// Frames of a block of `focus_change_db`.
const CHANGE_BLOCK: usize = 8192;

/// The change that `band` alone makes to `input` at `rate`, on the mid or
/// the side channel of `domain`, in dB: the power of that channel from
/// `hz / √2` to `hz × √2`, after the bell against before, over the
/// 8192-frame blocks of the recording (D128, D220, `set.focus-change.v2`).
/// None for the stereo domain or a recording with no power in the band.
pub fn focus_change_db(input: &[Stereo], domain: Domain, band: EqBand, rate: u32) -> Option<f64> {
    focus_change_db_read(&mut Slice(input), domain, band, rate).expect("frames of a slice")
}

/// Blocks of `CHANGE_BLOCK` frames in each span of `focus_change_db_read`.
const CHANGE_SPAN_BLOCKS: usize = 16;

/// `focus_change_db` of the frames of `input`, read 16 blocks at a time
/// (D250).
pub(crate) fn focus_change_db_read(
    input: &mut impl Frames,
    domain: Domain,
    band: EqBand,
    rate: u32,
) -> Result<Option<f64>> {
    let channel: fn(Stereo) -> f64 = match domain {
        Domain::Mid => |[l, r]| 0.5 * (l as f64 + r as f64),
        Domain::Side => |[l, r]| 0.5 * (l as f64 - r as f64),
        Domain::Stereo => return Ok(None),
    };
    let bins = (1..=CHANGE_BLOCK / 2).filter(|&bin| {
        let hz = bin as f64 * rate as f64 / CHANGE_BLOCK as f64;
        hz >= band.hz / std::f64::consts::SQRT_2 && hz < band.hz * std::f64::consts::SQRT_2
    });
    let (Some(first), Some(last)) = (bins.clone().next(), bins.max()) else {
        return Ok(None);
    };
    let mut filter = band.filter(rate);
    let mut stft = crate::stft::StereoStft::new(CHANGE_BLOCK, CHANGE_BLOCK);
    let mut pairs = vec![[0.0f32; 2]; CHANGE_BLOCK];
    let (mut before, mut after) = (0.0, 0.0);
    // The whole blocks of the input, as `chunks_exact`.
    let frames = input.len() / CHANGE_BLOCK * CHANGE_BLOCK;
    let span = CHANGE_SPAN_BLOCKS * CHANGE_BLOCK;
    let mut read = Vec::new();
    for start in (0..frames).step_by(span) {
        let part = input.window(start, span.min(frames - start), &mut read)?;
        for block in part.chunks_exact(CHANGE_BLOCK) {
            for (&frame, pair) in block.iter().zip(&mut pairs) {
                let dry = channel(frame);
                *pair = [dry as f32, filter.tick(dry) as f32];
            }
            if let Some((dry, output)) = stft.add_with(CHANGE_BLOCK, |i| pairs[i]) {
                before += dry[first..=last].iter().map(|c| c.norm_sqr()).sum::<f64>();
                after += output[first..=last]
                    .iter()
                    .map(|c| c.norm_sqr())
                    .sum::<f64>();
            }
        }
    }
    Ok((before > 0.0).then(|| 10.0 * (after / before).log10()))
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
    analyze_matched(
        &mut Slice(input),
        &mut Slice(&vocal),
        evidence,
        proposals,
        &mut crate::scratch::Pcm::temporary_of(&reconstructed)?,
        rate,
    )
}

/// `analyze_owned` of the PCM that `evidence` was measured from, with the
/// vocal stem at the render rate `rate`. `input`, `vocal` and the stored
/// stem sum `reconstructed` are read one passage at a time. The sum becomes
/// the accompaniment in place: each frame adds the negated vocal, and
/// `x + -y` is `x - y` (D246).
pub(crate) fn analyze_matched(
    input: &mut impl Frames,
    vocal: &mut impl Frames,
    evidence: &Evidence,
    proposals: Option<&ProposalSet>,
    reconstructed: &mut crate::scratch::Pcm,
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
    let frames = vocal.len();
    if frames.abs_diff(input.len()) > 1 || reconstructed.len() != frames {
        return Err("Refinement stem geometry differs".into());
    }
    let mut span = Vec::new();
    for start in (0..frames).step_by(SPAN) {
        let count = SPAN.min(frames - start);
        vocal.read(start, count, &mut span)?;
        for voice in &mut span {
            *voice = voice.map(|x| -x);
        }
        reconstructed.add_at(start, &span)?;
    }
    // The accompaniment, cut or padded with zeros to the frames of `input`.
    let mut accompaniment = Resized {
        frames: reconstructed,
        len: input.len(),
    };
    let mut vocal = Resized {
        frames: vocal,
        len: input.len(),
    };
    let window = PASSAGE_SECONDS * rate as usize;
    let (mut passage, mut other) = (Vec::new(), Vec::new());
    for start in (0..input.len()).step_by(window) {
        let end = (start + window).min(input.len());
        input.read(start, end - start, &mut passage)?;
        vocal.read(start, end - start, &mut span)?;
        accompaniment.read(start, end - start, &mut other)?;
        let (mut xx, mut yy, mut xy, mut residual, mut v, mut a) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        for i in start..end {
            for c in 0..2 {
                let x = passage[i - start][c] as f64;
                let voice = span[i - start][c] as f64;
                let y = voice + other[i - start][c] as f64;
                xx += x * x;
                yy += y * y;
                xy += x * y;
                residual += (x - y).powi(2);
                v += voice.powi(2) * 0.5;
                a += (other[i - start][c] as f64).powi(2) * 0.5;
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
        let mut bells: Vec<(Domain, EqBand)> = Vec::new();
        for proposal in &proposals.proposals {
            for (domain, filters) in [
                (Domain::Mid, &proposal.tone.mid),
                (Domain::Side, &proposal.tone.side),
            ] {
                for filter in filters {
                    if !bells.iter().any(|(d, f)| {
                        same_domain(*d, domain) && f.hz == filter.hz && f.q == filter.q
                    }) {
                        bells.push((domain, *filter));
                    }
                }
            }
        }
        result.focus = focus(
            &mut vocal,
            &mut accompaniment,
            &result.passages,
            &bells,
            rate,
        )?;
    }
    Ok(result)
}

/// The frames of `frames`, cut or padded with zeros to `len` frames.
struct Resized<'a, F: Frames> {
    frames: &'a mut F,
    len: usize,
}

impl<F: Frames> Frames for Resized<'_, F> {
    fn len(&self) -> usize {
        self.len
    }

    fn read(&mut self, start: usize, count: usize, output: &mut Vec<Stereo>) -> Result<()> {
        if start > self.len || count > self.len - start {
            return Err("Resized read exceeds its frame count".into());
        }
        let stored = self.frames.len().saturating_sub(start).min(count);
        if stored > 0 {
            self.frames.read(start, stored, output)?;
        } else {
            output.clear();
        }
        output.resize(count, [0.0; 2]);
        Ok(())
    }
}

fn same_domain(a: Domain, b: Domain) -> bool {
    std::mem::discriminant(&a) == std::mem::discriminant(&b)
}

impl Analysis {
    /// Fit measured amounts to a complete proposal. Combined boosts share
    /// the existing 9 dB response bound; their gains scale together.
    pub fn apply(&self, tone: &Tone) -> Result<(Tone, f64)> {
        let mut output = tone.clone();
        for (domain, bands) in [
            (Domain::Mid, &mut output.mid),
            (Domain::Side, &mut output.side),
        ] {
            for band in bands.iter_mut() {
                if let Some(focus) = self.focus.iter().find(|f| {
                    same_domain(f.domain, domain) && f.filter.hz == band.hz && f.filter.q == band.q
                }) {
                    band.gain_db = focus.filter.gain_db;
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
        Ok((output, scale))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsp::Kind;

    /// The filters of two lanes give the powers and the states of a scalar
    /// filter for each role, bit for bit, over passages in turn (D321).
    #[test]
    fn the_filters_of_two_lanes_equal_a_filter_for_each_role() {
        let vocal = noise(4 * 48_000 + 11);
        let accompaniment: Vec<Stereo> = noise(4 * 48_000 + 11)
            .iter()
            .rev()
            .map(|&[l, r]| [0.7 * r, l])
            .collect();
        let bell = EqBand {
            kind: Kind::Peak,
            hz: 3_150.0,
            gain_db: -2.5,
            q: 1.4,
        };
        let (eq, bp) = (
            bell.filter(48_000),
            finishing::bandpass(bell.hz, bell.q, 48_000),
        );
        for domain in [Domain::Mid, Domain::Side] {
            let mut states = [[0.0; 2]; 6];
            let mut scalar = [eq, eq, bp, bp, bp, bp];
            for (v, a) in vocal.chunks(48_000).zip(accompaniment.chunks(48_000)) {
                let powers = passage_powers((v, a), domain, eq, bp, &mut states);
                let mut expected = [0.0f64; 2];
                for frames in v.iter().zip(a) {
                    for (role, frame) in [frames.0, frames.1].iter().enumerate() {
                        let [l, r] = frame.map(f64::from);
                        let (mut m, mut s) = ((l + r) * 0.5, (l - r) * 0.5);
                        match domain {
                            Domain::Mid => m = scalar[role].tick(m),
                            _ => s = scalar[role].tick(s),
                        }
                        let m = scalar[2 + role * 2].tick(m);
                        let s = scalar[3 + role * 2].tick(s);
                        expected[role] += m * m + s * s;
                    }
                }
                assert_eq!(powers.map(f64::to_bits), expected.map(f64::to_bits));
                assert_eq!(
                    states.map(|state| state.map(f64::to_bits)),
                    scalar.map(|filter| filter.state().map(f64::to_bits))
                );
            }
        }
    }

    /// Deterministic noise with a side half as loud as the mid.
    fn noise(frames: usize) -> Vec<Stereo> {
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 11) as f64 / (1u64 << 53) as f64 - 0.5
        };
        (0..frames)
            .map(|_| {
                let (m, s) = (0.2 * next(), 0.1 * next());
                [(m + s) as f32, (m - s) as f32]
            })
            .collect()
    }

    fn bell(gain_db: f64) -> EqBand {
        EqBand {
            kind: Kind::Peak,
            hz: 1000.0,
            q: 2.0,
            gain_db,
        }
    }

    #[test]
    fn a_static_focus_bell_blends_its_bounded_gain_by_its_share() {
        let full = static_gain_db(Domain::Mid, 680.0, 4.66, 1.0);
        assert!((full - 4.66).abs() < 1e-9);
        assert_eq!(static_gain_db(Domain::Mid, 680.0, 4.66, 0.0), 0.0);
        // The centre response of the mean blend: 1 + 0.372 (10^(4.66/20) - 1).
        let blend = static_gain_db(Domain::Mid, 680.0, 4.66, 0.372);
        assert!((blend - 2.03).abs() < 0.01, "{blend}");
        // A side bell under 500 Hz takes the 1.5 dB bound before the blend.
        let side = static_gain_db(Domain::Side, 335.0, 4.0, 0.5);
        let bounded = 20.0 * (1.0 + 0.5 * (10f64.powf(1.5 / 20.0) - 1.0)).log10();
        assert!((side - bounded).abs() < 1e-9, "{side}");
        assert!(static_gain_db(Domain::Side, 605.0, 4.0, 0.5) > side + 1.0);
    }

    #[test]
    fn the_change_of_a_focus_bell_is_its_change_over_the_recording() {
        let rate = 48_000;
        let frames = 64 * CHANGE_BLOCK;
        let input = noise(frames);
        let full = focus_change_db(&input, Domain::Mid, bell(3.0), rate).unwrap();
        assert!(full > 0.5 && full < 3.0, "{full}");
        let flat = focus_change_db(&input, Domain::Mid, bell(0.0), rate).unwrap();
        assert!(flat.abs() < 1e-6, "{flat}");
        let silent = vec![[0.0; 2]; frames];
        assert!(focus_change_db(&silent, Domain::Mid, bell(3.0), rate).is_none());
        assert!(focus_change_db(&input, Domain::Stereo, bell(3.0), rate).is_none());
    }

    /// The change of a focus bell over a mix in scratch storage is its
    /// change over the mix in memory (D250).
    #[test]
    fn the_change_of_a_focus_bell_over_a_stored_mix_is_that_of_the_mix() {
        let rate = 48_000;
        // Two spans and a part, and a partial block at the end.
        let input = noise(40 * CHANGE_BLOCK + 777);
        let mut stored = crate::scratch::Pcm::temporary_of(&input).unwrap();
        for domain in [Domain::Mid, Domain::Side] {
            let whole = focus_change_db(&input, domain, bell(3.0), rate).map(f64::to_bits);
            let read = focus_change_db_read(&mut stored, domain, bell(3.0), rate).unwrap();
            assert!(whole.is_some());
            assert_eq!(read.map(f64::to_bits), whole);
        }
    }

    /// Stems of 24 s at 48 kHz in the form of `tests/refinement.rs`: a drum
    /// pulse, a bass, an other part near 700 Hz and a vocal at 710 Hz that
    /// rests from 8 to 12 s and from 20 s.
    fn material() -> (Vec<Stereo>, [Vec<Stereo>; 4]) {
        let stems: [Vec<Stereo>; 4] = std::array::from_fn(|role| {
            (0..24 * 48000)
                .map(|i| {
                    let t = i as f64 / 48000.0;
                    let sine = |hz: f64| (std::f64::consts::TAU * hz * t).sin();
                    let pulse = (-((i % 24000) as f64 / 48000.0) / 0.06).exp();
                    match role {
                        0 => [(0.1 * pulse * sine(120.0)) as f32; 2],
                        1 => [(0.03 * sine(70.0)) as f32; 2],
                        2 => {
                            let (m, s) = (0.042 * sine(690.0), 0.098 * sine(730.0));
                            [(m + s) as f32, (m - s) as f32]
                        }
                        _ => {
                            let rests = (8.0..12.0).contains(&t) || t >= 20.0;
                            [(if rests { 0.0 } else { 0.035 * sine(710.0) }) as f32; 2]
                        }
                    }
                })
                .collect()
        });
        let mix = (0..stems[0].len())
            .map(|i| [0, 1].map(|c| stems.iter().map(|s| s[i][c]).sum()))
            .collect();
        (mix, stems)
    }

    fn proposals(bells: &[(Domain, EqBand)]) -> ProposalSet {
        let of = |domain| {
            bells
                .iter()
                .filter(|(d, _)| same_domain(*d, domain))
                .map(|(_, band)| *band)
                .collect()
        };
        ProposalSet {
            schema: "test",
            rule: "test",
            validation_status: "test",
            source_file_sha256: "a".repeat(64),
            analysis_sha256: "b".repeat(64),
            policy: crate::source_treatments::Policy::default(),
            notes: Vec::new(),
            proposals: vec![crate::source_treatments::Proposal {
                id: "focus".into(),
                rule: "test",
                rationale: "test",
                basis: Vec::new(),
                tone: Tone {
                    mid: of(Domain::Mid),
                    side: of(Domain::Side),
                    ..Tone::default()
                },
            }],
        }
    }

    /// Each bell of a round gives the focus of a round of its own, and a
    /// vocal in scratch storage gives the analysis of a vocal in memory
    /// (D234).
    #[test]
    fn each_bell_of_a_round_gives_the_focus_of_a_round_of_its_own() {
        let (mix, stems) = material();
        let raw = stems.each_ref().map(Vec::as_slice);
        let evidence = stem_evidence::analyze(&mix, raw, 48000, 48000).unwrap();
        assert!(evidence.usable);
        let sum: Vec<Stereo> = (0..mix.len())
            .map(|i| [0, 1].map(|c| stems.iter().map(|s| s[i][c]).sum()))
            .collect();
        let reconstructed = resample::convert(&sum, 48000, 48000).unwrap();
        let bell = |hz: f64, q: f64, gain_db: f64| EqBand {
            kind: Kind::Peak,
            hz,
            q,
            gain_db,
        };
        let bells = [
            (Domain::Mid, bell(700.0, 1.0, 3.0)),
            (Domain::Mid, bell(710.0, 2.0, 2.0)),
            (Domain::Side, bell(730.0, 1.0, 2.0)),
        ];
        let analysis = |bells: &[(Domain, EqBand)]| {
            analyze_owned(
                &mix,
                crate::audio_io::Source {
                    audio: stems[3].clone(),
                    sample_rate: 48000,
                },
                &evidence,
                Some(&proposals(bells)),
                reconstructed.clone(),
                48000,
            )
            .unwrap()
        };
        let together = analysis(&bells);
        let alone: Vec<_> = bells
            .iter()
            .map(|b| serde_json::to_value(&analysis(&[*b]).focus[0]).unwrap())
            .collect();
        assert_eq!(
            serde_json::to_value(&together.focus).unwrap(),
            serde_json::Value::from(alone)
        );
        assert!(
            together
                .focus
                .iter()
                .filter(|f| f.mean_wet.is_some())
                .count()
                >= 2
        );
        let mut vocal = crate::scratch::Pcm::temporary_of(&stems[3]).unwrap();
        let stored = analyze_matched(
            &mut Slice(&mix),
            &mut vocal,
            &evidence,
            Some(&proposals(&bells)),
            &mut crate::scratch::Pcm::temporary_of(&reconstructed).unwrap(),
            48000,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(stored).unwrap(),
            serde_json::to_value(together).unwrap()
        );
    }

    /// The stored sum of the stems becomes the accompaniment in place: the
    /// power of each passage is the power of the sum minus the vocal, frame
    /// by frame (D246).
    #[test]
    fn the_accompaniment_in_place_is_the_sum_minus_the_vocal() {
        let (mix, stems) = material();
        let raw = stems.each_ref().map(Vec::as_slice);
        let evidence = stem_evidence::analyze(&mix, raw, 48000, 48000).unwrap();
        let sum: Vec<Stereo> = (0..mix.len())
            .map(|i| [0, 1].map(|c| stems.iter().map(|s| s[i][c]).sum()))
            .collect();
        let mut stored = crate::scratch::Pcm::temporary_of(&sum).unwrap();
        let analysis = analyze_matched(
            &mut Slice(&mix),
            &mut Slice(&stems[3]),
            &evidence,
            None,
            &mut stored,
            48000,
        )
        .unwrap();
        assert_eq!(analysis.passages.len(), 12);
        for passage in &analysis.passages {
            let mut power = 0.0;
            for i in passage.start_sample..passage.end_sample {
                for c in 0..2 {
                    power += ((sum[i][c] - stems[3][i][c]) as f64).powi(2) * 0.5;
                }
            }
            assert_eq!(passage.accompaniment_power, power);
        }
    }
}
