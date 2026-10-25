//! One source-led job: propose, render, compare, and deliver completed versions.
use crate::{
    analysis, audibility,
    audio_io::{Result, Stereo},
    broad_tone, compressor, deesser,
    dsp::{self, EqBand},
    finishing, foundation,
    pipeline::{self, SourceAnalysis},
    selection::{self, Candidate, CandidateTrial, Gates, Signature},
    source_condition,
    source_treatments::{self, SourceIdentity},
    stem_evidence, stereo, stereo_phase,
    treatment::{FocusEnvelope, Tone},
    treatment_render::{self, Renderer, Spec},
    width, Metrics,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

/// Bind delivery dither to the decoded source, independent of WAV/SoundKit packaging.
pub fn delivery_seed(source_pcm_sha256: &str, version_id: &str) -> u64 {
    let mut hash = Sha256::new();
    hash.update(source_pcm_sha256.as_bytes());
    hash.update(version_id.as_bytes());
    u64::from_le_bytes(hash.finalize()[..8].try_into().unwrap())
}

#[cfg(test)]
mod delivery_seed_tests {
    use super::delivery_seed;

    #[test]
    fn seed_is_bound_to_decoded_source_and_version() {
        let pcm = "87ef42fa5e0a0f191fed7ef36cee2e66df88c5cb3bb88cf7339472f8f75d7697";
        assert_eq!(delivery_seed(pcm, "essential"), 13717141682437992797);
        assert_ne!(
            delivery_seed(pcm, "essential"),
            delivery_seed(pcm, "denser")
        );
    }
}

pub struct AuditoryInput<'a> {
    pub json: &'a [u8],
    pub source: SourceIdentity<'a>,
    /// Caller-verified decoding of the same original file into engine PCM.
    pub source_pcm_sha256: &'a str,
}

#[derive(Serialize)]
pub struct DensityPolicy {
    pub rule: &'static str,
    pub source_crest_p10_p50_p90_db: [f64; 3],
    pub window_contrast_db: f64,
    pub loud_passage_db: f64,
    pub base_target_lufs: f64,
    /// Target of `bold`: the base target (`set.audible-loudness.v6`). None
    /// for a source that arrives limited or that has no audibility point:
    /// `bold` then takes the base target.
    pub bold_target_lufs: Option<f64>,
    pub denser_target_lufs: Option<f64>,
    pub denser_step_lu: Option<f64>,
    /// The source arrives limited: its share of 10 ms blocks within 1 dB of
    /// its 99.9th percentile block peak is `LIMITED_PLATEAU_SHARE` or more,
    /// and those blocks spread over `LIMITED_LEVEL_SPREAD_DB` or more of
    /// level.
    pub input_limited: bool,
    /// Base target of the crest rule, which applies when the recording has
    /// no audibility point.
    pub crest_target_lufs: f64,
    /// Audibility point of peak control at the base clip budget
    /// (`set.audible-loudness.v3`), with the render at the base target among
    /// its probes.
    pub audibility: Option<audibility::Point>,
    /// Target of the track in its album (`album::levels`), in LUFS. The
    /// base and the Bold targets are at or under it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub album_target_lufs: Option<f64>,
    pub reason: &'static str,
}

/// Options of a set. The default has no album target and the de-esser
/// switch off.
#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    /// Target of the track in its album (`album::levels`), in LUFS.
    pub album_target_lufs: Option<f64>,
    /// The de-esser switch (`set.deesser.v4`). With true, the versions cut
    /// the sibilant frames of the vocal. With false, the default, the
    /// de-esser keeps its key and its measure and cuts nothing: album masters
    /// show no de-essing beyond the de-essing of their mixes (D177).
    pub deesser: bool,
}

#[cfg(test)]
mod options_tests {
    #[test]
    fn the_de_esser_is_off_by_default() {
        let options = super::Options::default();
        assert!(!options.deesser && options.album_target_lufs.is_none());
    }
}

/// Plateau share at and above which a source counts as limited. The share
/// sits between the plateau shares of unlimited recordings and those of
/// limited masters (D130).
pub const LIMITED_PLATEAU_SHARE: f64 = 0.04;
/// Level spread of the plateau blocks at and above which the plateau comes
/// from a ceiling rather than from repeated identical events. The plateau
/// blocks of a limited master spread over more level than repeated
/// identical pulses (D130).
pub const LIMITED_LEVEL_SPREAD_DB: f64 = 1.0;

#[derive(Clone, Serialize)]
pub struct Branch {
    pub id: String,
    pub label: String,
    pub rule: &'static str,
    pub basis: String,
    pub tone: Tone,
    /// Weaker tone alternatives, strongest first, tried after `tone` fails,
    /// each with the factor its tonal gate minimums are scaled by.
    pub alternatives: Vec<(String, Tone, f64)>,
    pub gates: Gates,
    pub finishing: finishing::Plan,
    pub use_foundation: bool,
}

/// Fixed version roles of one set, lightest treatment first. Every version is
/// a complete master; the IDs (`essential`, `master`, `bold`) stay stable.
/// The labels are the likeliest names until `set.version-names.v1` names the
/// delivered versions from their measured sound.
pub const ESSENTIAL_LABEL: &str = "Essential";
pub const MASTER_LABEL: &str = "Forward";
/// The label of the `master` version with the bass shelves
/// (`set.bass-version.v4`).
pub const FULL_LABEL: &str = "Full";
pub const DENSER_MASTER_LABEL: &str = "Denser Forward";
pub const DENSER_ESSENTIAL_LABEL: &str = "Denser Essential";
pub const BOLD_LABEL: &str = "Vivid";
/// The planned label of the `open` version (`open::plan`, D178).
pub const OPEN_LABEL: &str = "Open";

/// Gain-matched residual against an already delivered version, as a diagnostic.
#[derive(Clone, Serialize)]
pub struct Residual {
    pub id: String,
    pub matched_residual_db: f64,
}

#[derive(Clone, Serialize)]
pub struct Comparison {
    pub id: String,
    pub matched_residual_db: f64,
    /// True when no described measure differs by its description threshold.
    pub described_duplicate: bool,
}

#[derive(Serialize)]
pub struct Report {
    pub target_rule: &'static str,
    pub target_basis: String,
    pub limiter_rule: &'static str,
    pub limiter_basis: String,
    pub output: Metrics,
    pub output_stereo: stereo::StereoAnalysis,
    pub preservation: analysis::Preservation,
    pub gain_span_db: f64,
    pub source_constraint_diagnostics: Vec<&'static str>,
    pub diagnostic_constraints: pipeline::Constraints,
    pub chosen_candidate: usize,
    pub candidates: Vec<CandidateTrial>,
    pub render: treatment_render::Rendered,
    /// The bass of the held bands at matched loudness
    /// (`low_end::bass_level_db`).
    pub bass_level_db: Option<f64>,
    /// The change of the held bands in each section against Essential
    /// (`crate::adjust::HoldSections`); none for Essential and Denser.
    pub hold_sections: Option<crate::adjust::HoldSections>,
}

#[derive(Serialize)]
pub struct Version<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub description: &'a str,
    pub rule: &'static str,
    pub basis: &'a str,
    pub spec: &'a Spec,
    pub report: &'a Report,
    pub refinement: Option<&'a crate::refinement::Analysis>,
    /// Seconds of the render of a version that rendered beside an earlier
    /// version (`Sink::parallel_versions`); none for a render in its turn.
    #[serde(skip)]
    pub rendered_beside_seconds: Option<f64>,
}

#[derive(Serialize)]
pub struct Trial {
    pub id: String,
    pub label: String,
    pub rule: &'static str,
    pub basis: String,
    pub description: Option<String>,
    /// The measured name of a version other than Essential.
    pub measured_name: Option<String>,
    /// The measured changes behind the description, in figures, for a
    /// delivered version (`set.version-names.v1` writes the description).
    pub detail: Option<String>,
    /// The characters behind the name of a delivered version.
    pub naming: Option<crate::version_names::Named>,
    pub status: &'static str,
    pub failure: Option<String>,
    pub duplicate_of: Option<String>,
    pub comparisons: Vec<Comparison>,
    pub spec: Spec,
    pub candidate_count: usize,
    pub candidates: Vec<CandidateTrial>,
    pub report: Option<Report>,
}

#[derive(Serialize)]
pub struct Summary {
    pub schema: &'static str,
    pub validation_status: &'static str,
    pub source: SourceAnalysis,
    pub density: DensityPolicy,
    /// How the recording was captured (`source_condition`).
    pub source_condition: source_condition::SourceCondition,
    pub focus: Option<source_treatments::ProposalSet>,
    pub stem_evidence: Option<stem_evidence::Evidence>,
    pub broad_tone: broad_tone::Analysis,
    pub low_end: width::LowEnd,
    /// The low end of the `open` version (`open::plan`).
    pub open: crate::open::Open,
    pub glue: Option<compressor::Settings>,
    /// The compression styles of the versions (`set.compression.v3`).
    pub compression: Option<compressor::Plan>,
    pub duplicate_rule: &'static str,
    pub residual_diagnostic_db: f64,
    /// What a request on a delivered version needs from the plan (D171).
    pub adjust: crate::adjust::Base,
    pub version_ids: Vec<String>,
    pub trials: Vec<Trial>,
    pub work: treatment_render::Work,
}

/// A sink compares against already delivered PCM and persists each accepted
/// version before the next render. An error stops the job with earlier deliveries
/// intact. Native callers can use disk; other hosts can use their own storage.
pub trait Sink {
    fn scratch_factory(&self) -> crate::scratch::Factory {
        Box::new(crate::scratch::temporary)
    }

    /// True when the versions after Essential may render beside each other
    /// on the available cores of a native build, for a recording of up to
    /// `PARALLEL_VERSIONS_SECONDS`. Those renders keep their scratch storage
    /// in `scratch::temporary`. The events, the comparisons, the deliveries
    /// and the trials keep the order of the plan.
    fn parallel_versions(&self) -> bool {
        false
    }

    fn comparisons(&mut self, audio: &[Stereo]) -> Result<Vec<Residual>>;
    fn deliver(&mut self, version: &Version<'_>, audio: &[Stereo]) -> Result<()>;
    fn event(&mut self, _event: &'static str, _id: &str) -> Result<()> {
        Ok(())
    }
    /// The names and descriptions of the delivered versions
    /// (`set.version-names.v1`), after the last version. A version whose
    /// naming is not `distinct` is withdrawn: the sink removes it.
    fn name(&mut self, _named: &[crate::version_names::Named]) -> Result<()> {
        Ok(())
    }
}

pub fn matched_residual_db(reference: &[Stereo], candidate: &[Stereo]) -> Result<f64> {
    if reference.len() != candidate.len() || reference.is_empty() {
        return Err("Duplicate comparison requires equal nonempty frame counts".into());
    }
    let (mut cross, mut a_power, mut b_power) = (0.0, 0.0, 0.0);
    for (a, b) in reference.iter().flatten().zip(candidate.iter().flatten()) {
        if !a.is_finite() || !b.is_finite() {
            return Err("Nonfinite duplicate comparison".into());
        }
        let (a, b) = (*a as f64, *b as f64);
        cross += a * b;
        a_power += a * a;
        b_power += b * b;
    }
    // Positive gain only: polarity inversion is not the same interpretation.
    let gain = (cross / b_power.max(1e-30)).max(0.0);
    let residual: f64 = reference
        .iter()
        .flatten()
        .zip(candidate.iter().flatten())
        .map(|(a, b)| (*a as f64 - gain * *b as f64).powi(2))
        .sum();
    Ok(dsp::db((residual / a_power.max(1e-30)).sqrt()))
}

pub(crate) fn candidate_status(rule: &str, duplicate: bool) -> &'static str {
    if duplicate {
        "duplicate"
    } else if rule == "set.foundation-fallback.v1" {
        "comparison-only"
    } else {
        "retained"
    }
}

/// Residual below which two renders count as duplicates in the diagnostic
/// report. `Signature::is_duplicate_of` decides which versions are duplicates.
pub const DUPLICATE_DB: f64 = -40.0;
pub const DUPLICATE_RULE: &str = "set.described-duplicate.v1";
/// Dense versions must keep maximum limiter reduction under this bound.
pub const DENSE_LIMITER_MAXIMUM_DB: f64 = 8.0;
/// Dense versions must keep mean limiter reduction under this bound.
pub const DENSE_LIMITER_MEAN_DB: f64 = 3.0;
/// Dense versions may add this much P90 attack loss against the base.
pub const DENSE_ATTACK_LOSS_DB: f64 = 2.0;
/// Base target bounds. −20 LUFS is the AES TD1008 lower distribution level.
pub const BASE_TARGET_LUFS: (f64, f64) = (-20.0, -8.0);
/// Twelve-second window contrast (P90−P10 RMS) above which the loud
/// passages set the base target.
pub const CONTRAST_FREE_DB: f64 = 6.0;
/// Playback level streaming services normalise to. The listener hears a
/// master no louder than this, so the equal-loudness compensation uses the
/// smaller of the base target and this level.
pub const STREAMING_NORMALISATION_LUFS: f64 = -14.0;
/// Every version sits this far under the audibility point of peak control
/// (`set.audible-loudness.v6`, D206). Hot single reference masters sit at their
/// audibility points, so the margin is zero. Each version stays at or under
/// its own point (`Gates::under_own_point`). Spotify and Apple turn a louder
/// master down and add no limiting to it.
pub const AUDIBILITY_MARGIN_LU: f64 = 0.0;
/// Largest target of every version, and the cap of the Denser targets, in
/// LUFS.
pub const TARGET_MAXIMUM_LUFS: f64 = -6.5;
/// Measured rise in maximum limiter reduction per LU of denser target.
pub const DENSE_REDUCTION_PER_LU: f64 = 1.5;
/// The glue trial costs one render per set and removes no limiter work at
/// fixed loudness, so it is off (D111).
pub const GLUE_TRIAL: bool = false;
/// Under `TARGET_MAXIMUM_LUFS` (D131) a Denser version sits less than 1 LU above
/// its base and changes the band balance and the mean limiter reduction by
/// 0.05 dB or less. Loudness-normalised playback removes the level
/// difference, so no Denser version is planned (D135).
pub const DENSER_VERSIONS: bool = false;
/// Delivered true-peak ceiling for every set version (D110, D155). A
/// −1 dBTP ceiling makes each master 0.5 LU quieter at the same peak
/// control (D152).
pub const SET_CEILING_DBTP: f64 = -0.5;
/// The ceiling of the crest targets (D110). A crest target moves with the
/// delivery ceiling, so that the peak control has the same work under a
/// lower ceiling.
pub const TARGET_CEILING_DBTP: f64 = -0.5;
/// Ceiling-relative soft-clip budgets. The limiter bounds the signal this far
/// above the ceiling; the clipper shapes the remainder onto it.
pub const BASE_CLIPPER_DB: f64 = 1.5;
pub const DENSER_CLIPPER_DB: f64 = 2.5;
/// The shared foundation bells of `set.all-role-foundation.v1` change the
/// mix by 0.3 dB or less in their bands, under the 0.5 dB that a stage
/// must reach, so no set version applies them (D128).
pub const FOUNDATION_IN_VERSIONS: bool = false;
/// Stem focus side bells under this frequency are bounded to
/// `LOW_SIDE_BELL_DB` (D128).
pub const LOW_SIDE_BELL_HZ: f64 = 500.0;
pub const LOW_SIDE_BELL_DB: f64 = 1.5;
/// First drive of a base version above the loudness difference, in dB. The
/// offset sits near the final drive of a base render, so the drive search
/// starts close to its result.
pub const BASE_DRIVE_OFFSET_DB: f64 = 0.5;
/// Limiter release of every set version, in ms. Against the 35 ms of the
/// stem-derived release, 10 ms lowers the masked disturbance of peak control
/// at each clip budget, also in the bands under 510 Hz (D127).
pub const LIMITER_RELEASE_MS: f64 = 10.0;
/// True-peak correction of every set version. A trim of the whole render
/// lowers most versions by about 0.5 dB; the local correction keeps that
/// level and lowers the masked disturbance (D166).
pub const TRUE_PEAK: dsp::TruePeak = dsp::TruePeak::Local;
/// Limiter attack of the versions of a source that takes the attack, in ms
/// (D166).
pub const PEAK_ATTACK_MS: f64 = 3.0;
/// Smallest decrease of the energy attack loss P90 of the base render for
/// which a source takes the attack, in dB.
pub const PEAK_ATTACK_GAIN_DB: f64 = 0.1;

fn unchanged_finishing() -> finishing::Plan {
    finishing::Plan {
        rule: "set.unchanged.v2",
        validation_status: "provisional",
        active: false,
        dc_filter_hz: None,
        bands: Vec::new(),
        section_gain: Vec::new(),
        color_amount: 0.0,
        clipping_budget_db: 0.0,
    }
}

/// Whether the source arrives limited.
fn input_limited(source: &SourceAnalysis) -> bool {
    source.temporal.plateau_share >= LIMITED_PLATEAU_SHARE
        && source.temporal.plateau_level_spread_db >= LIMITED_LEVEL_SPREAD_DB
}

/// Initial drive of a base render at `target_lufs`. The make-up of the
/// compressor keeps the level of the mix (`set.compression.v3`).
fn base_drive_db(source: &SourceAnalysis, target_lufs: f64) -> f64 {
    target_lufs - source.input.integrated_lufs.unwrap() + BASE_DRIVE_OFFSET_DB
}

/// The compression style of a version (`set.compression.v3`): Gentle for
/// the version closest to the mix, Dense for Bold, and Punchy for the
/// other versions.
pub fn compression_style(version_id: &str) -> compressor::Style {
    match version_id {
        "essential" => compressor::Style::Gentle,
        "bold" => compressor::Style::Dense,
        _ => compressor::Style::Punchy,
    }
}

fn density(source: &SourceAnalysis, audibility: Option<audibility::Point>) -> DensityPolicy {
    let crest = source.temporal.crest_50ms_p10_p50_p90_db;
    // Peaks of the loudest 12 s windows sit near half the contrast above the
    // integrated level; placing them at the ceiling needs that much more.
    let contrast = source.temporal.window_contrast_db;
    let loud_passage_db = 0.5 * (contrast - CONTRAST_FREE_DB).max(0.0);
    // A limited source keeps its loudness, moved by the ceiling shift, and
    // gets no Denser version: its peaks already sit under a ceiling.
    let input_limited = input_limited(source);
    let crest_target = (if input_limited {
        source.input.integrated_lufs.unwrap_or(BASE_TARGET_LUFS.1)
    } else {
        -(crest[1] + loud_passage_db)
    } + SET_CEILING_DBTP
        - TARGET_CEILING_DBTP)
        .clamp(BASE_TARGET_LUFS.0, BASE_TARGET_LUFS.1);
    let eligible = !input_limited
        && crest[1] >= 6.0
        && (crest[2] - crest[0] >= 1.0 || source.temporal.events.len() >= 8);
    let step = ((crest[2] - crest[0]) * 0.5).clamp(0.5, 3.0);
    let crest_denser = (crest_target + step).min(-6.5);
    let (rule, base, denser, reason) = match (&audibility, input_limited) {
        (Some(point), false) => (
            "set.audible-loudness.v6",
            (point.lufs - AUDIBILITY_MARGIN_LU).min(TARGET_MAXIMUM_LUFS),
            eligible.then_some(crest_denser.min(point.lufs)),
            "The audibility point of peak control sets the target of every version: the output loudness at which the total masked noise-to-mask ratio of the limiter and clipper reaches 0 dB on the compressed mix. Each version sits at that point, with a -6.5 LUFS cap, and at or under the point of its own chain.",
        ),
        _ => (
            "set.source-density.v4",
            crest_target,
            eligible.then_some(crest_denser),
            "Base target is the negative of the 50 ms crest median plus half the 12 s window contrast above 6 dB, moved by the difference between the delivery ceiling and the -0.5 dBTP ceiling of that rule, bounded -20 to -8 LUFS; the denser step uses half the P90-P10 crest span, bounded 0.5-3 LU, with a -6.5 LUFS target cap and the base render's limiter headroom. Complete renders determine distinctness; these are not genre or quality targets.",
        ),
    };
    let denser = denser.filter(|d| *d > base);
    let bold = (audibility.is_some() && !input_limited).then_some(base);
    DensityPolicy {
        rule,
        source_crest_p10_p50_p90_db: crest,
        window_contrast_db: contrast,
        loud_passage_db,
        base_target_lufs: base,
        bold_target_lufs: bold,
        denser_target_lufs: denser,
        denser_step_lu: denser.map(|t| t - base),
        input_limited,
        crest_target_lufs: crest_target,
        audibility,
        album_target_lufs: None,
        reason,
    }
}

/// The branches of the set (`set.version-character.v1`, D206): Essential,
/// Full and Warm, each with the tone of its `character::Character` on top
/// of the stem focus, with its own widening. A version whose widening gives
/// the bands from 500 Hz up no gain carries the measured side cut.
fn branches(
    spectrum: &crate::Spectrum,
    focus: Option<&source_treatments::ProposalSet>,
    side_cut: Option<EqBand>,
    widening: &width::Widening,
) -> Vec<Branch> {
    let focus_proposal = focus.and_then(|f| {
        f.proposals
            .iter()
            .find(|p| p.id == "focus-combined")
            .or_else(|| f.proposals.iter().find(|p| p.id != "unchanged"))
    });
    let mut base = Tone::default();
    if let Some(p) = focus_proposal {
        base.mid.extend(p.tone.mid.iter().copied());
        base.side.extend(p.tone.side.iter().copied());
    }
    [
        ("essential", ESSENTIAL_LABEL),
        ("master", FULL_LABEL),
        ("open", OPEN_LABEL),
    ]
    .into_iter()
    .map(|(id, label)| {
        let character = crate::character::of(id).expect("a version of the set");
        let side = widening.of(id);
        let plan = crate::character::plan(spectrum, character, &base, side.as_ref());
        let mut tone = plan.tone.clone();
        if side.is_none_or(|g| g.bands_db[2..].iter().all(|x| *x <= 0.0)) {
            tone.side.extend(side_cut);
        }
        let mut basis = vec![format!(
            "{}: region moves {} from the mix {}, predicted {} at gain scale {:.3}",
            plan.rule,
            serde_json::to_string(&plan.wanted).expect("finite"),
            serde_json::to_string(&plan.mix).expect("finite"),
            serde_json::to_string(&plan.predicted).expect("finite"),
            plan.scale,
        )];
        if let Some(p) = focus_proposal {
            basis.push(format!(
                "{}: {}",
                p.rule,
                serde_json::to_string(&p.basis).expect("Validated finite proposal evidence")
            ));
        }
        Branch {
            id: id.into(),
            label: label.into(),
            rule: crate::character::RULE,
            basis: basis.join("; "),
            alternatives: vec![
                (
                    "three quarters".into(),
                    broad_tone::scaled(&tone, 0.75),
                    0.75,
                ),
                ("half".into(), broad_tone::scaled(&tone, 0.5), 0.5),
            ],
            tone,
            // Essential is the reference of the gates of the other versions;
            // its widening keeps the window bound of its allowance.
            gates: Gates {
                tonal_image: id != "essential",
                ..Gates::default()
            },
            finishing: unchanged_finishing(),
            // The shared tonal foundation of the stems, when the evidence
            // has one, is part of Essential.
            use_foundation: id == "essential" || FOUNDATION_IN_VERSIONS,
        }
    })
    .collect()
}

/// The foundation and the branch tone in one tone at `rate`.
fn with_foundation(
    mut branch: Tone,
    branch_envelopes: Vec<FocusEnvelope>,
    foundation: &foundation::Foundation,
    rate: u32,
) -> Result<(Tone, Vec<FocusEnvelope>, f64)> {
    let mut scale = 1.0;
    let tone = loop {
        let mut combined = foundation.tone.clone();
        combined.stereo.extend(branch.stereo.iter().copied());
        combined.mid.extend(branch.mid.iter().copied());
        combined.side.extend(branch.side.iter().copied());
        if combined.validate(rate).is_ok() {
            break combined;
        }
        if scale < 0.1 {
            return Ok((foundation.tone.clone(), foundation.envelopes.clone(), 0.0));
        }
        scale *= 0.9;
        for bands in [&mut branch.stereo, &mut branch.mid, &mut branch.side] {
            for band in bands {
                band.gain_db *= 0.9;
            }
        }
    };
    let mut envelopes = foundation.envelopes.clone();
    for mut envelope in branch_envelopes {
        envelope.filter_index += match envelope.domain {
            finishing::Domain::Stereo => foundation.tone.stereo.len(),
            finishing::Domain::Mid => foundation.tone.mid.len(),
            finishing::Domain::Side => foundation.tone.side.len(),
        };
        envelopes.push(envelope);
    }
    Ok((tone, envelopes, scale))
}

/// True when two tones differ in a filter.
fn json_differs(a: &Tone, b: &Tone) -> bool {
    serde_json::to_value(a).ok() != serde_json::to_value(b).ok()
}

/// A plain spec for unit tests.
#[cfg(test)]
pub(crate) fn test_spec() -> Spec {
    Spec {
        phase_alignment: None,
        finishing: unchanged_finishing(),
        tone: Tone::default(),
        focus_envelopes: Vec::new(),
        side_highpass_hz: None,
        side_gain: None,
        deesser: None,
        low_mid: None,
        low_end_hold: None,
        mono_safe: Vec::new(),
        dynamic_after_eq: Vec::new(),
        compressor: None,
        limiter: dsp::LimiterSettings::default(),
        clipper_db: BASE_CLIPPER_DB,
        ceiling_dbtp: SET_CEILING_DBTP,
        initial_drive_db: 0.0,
        target_lufs: -10.0,
        fixed_drive_db: None,
        restore_peak_tone: false,
        restoration_db: [0.0; 6],
        true_peak: TRUE_PEAK,
    }
}

#[cfg(test)]
mod foundation_plan_tests {
    use super::*;
    use crate::{
        dsp::{EqBand, Kind},
        finishing::Domain,
        foundation::{Foundation, Region},
        treatment::BlendPoint,
    };

    #[test]
    fn candidates_render_alike_only_with_the_same_reasons_gates_and_specs() {
        let candidate = Candidate {
            reason: "strongest".into(),
            spec: test_spec(),
            gates: Gates::default(),
        };
        let same = vec![candidate.clone()];
        assert!(same_candidates(
            &same,
            std::slice::from_ref(&candidate),
            None
        ));
        let mut louder = candidate.clone();
        louder.spec.target_lufs += 0.01;
        assert!(!same_candidates(&same, &[louder], None));
        let mut wider = candidate.clone();
        wider.spec.side_highpass_hz = Some(100.0);
        assert!(!same_candidates(&same, &[wider], None));
        let mut renamed = candidate.clone();
        renamed.reason = "half".into();
        assert!(!same_candidates(&same, &[renamed], None));
        let mut gated = candidate.clone();
        gated.gates.low_end = true;
        assert!(!same_candidates(&same, &[gated], None));
        assert!(!same_candidates(
            &same,
            &[candidate.clone(), candidate],
            None
        ));
    }

    #[test]
    fn vivid_steps_presence_and_treble_and_keeps_corrections() {
        // A Clear with two 4.4 dB focus bells. A scaling of the whole tone
        // pushes them past the 9 dB domain bound.
        let bell = |hz, gain_db| EqBand {
            kind: Kind::Peak,
            hz,
            q: 1.0,
            gain_db,
        };
        let lead = EqBand {
            q: broad_tone::LEAD_BELL_Q,
            ..bell(3200.0, 2.0)
        };
        let shelf = EqBand {
            kind: Kind::LowShelf,
            ..bell(150.0, -1.6)
        };
        let presence = EqBand {
            q: broad_tone::LEAD_BELL_Q,
            ..bell(3162.0, 1.0)
        };
        let spec = Spec {
            phase_alignment: None,
            finishing: unchanged_finishing(),
            tone: Tone {
                stereo: vec![shelf, presence],
                mid: vec![
                    bell(635.0, 1.5),
                    bell(252.0, 1.5),
                    bell(245.0, 4.37),
                    bell(667.0, 4.44),
                    lead,
                ],
                side: vec![bell(2016.0, 1.5), bell(4000.0, -1.2)],
            },
            focus_envelopes: Vec::new(),
            side_highpass_hz: None,
            side_gain: None,
            deesser: None,
            low_mid: None,
            low_end_hold: None,
            mono_safe: Vec::new(),
            dynamic_after_eq: Vec::new(),
            compressor: None,
            limiter: dsp::LimiterSettings::default(),
            clipper_db: BASE_CLIPPER_DB,
            ceiling_dbtp: SET_CEILING_DBTP,
            initial_drive_db: 0.0,
            target_lufs: -10.0,
            fixed_drive_db: None,
            restore_peak_tone: false,
            restoration_db: [0.0; 6],
            true_peak: TRUE_PEAK,
        };
        // Noise with a spectrum that falls by 3 dB per octave, in each band.
        let mut seed = 7u32;
        let mut state = [0.0f32; 2];
        let noise: Vec<crate::audio_io::Stereo> = (0..96_000)
            .map(|_| {
                std::array::from_fn(|c| {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    let white = (seed >> 8) as f32 / 16_777_216.0 - 0.5;
                    state[c] = 0.98 * state[c] + 0.1 * white;
                    state[c] + 0.2 * white
                })
            })
            .collect();
        let spectrum = crate::spectrum(&noise, 48000);
        let bold = bold_candidates(
            &spec,
            None,
            spec.compressor,
            None,
            &spectrum,
            24_000.0,
            false,
        )
        .into_iter()
        .next()
        .expect("Vivid exists when Clear exists");
        let json = |bands: &[EqBand]| serde_json::to_value(bands).unwrap();
        // The focus bells and the lead presence bell keep the gains of
        // Clear; the side is unchanged.
        assert_eq!(json(&bold.spec.tone.mid), json(&spec.tone.mid));
        assert_eq!(json(&bold.spec.tone.side), json(&spec.tone.side));
        // The equal-loudness shelf is a correction. The presence balance
        // bell carries the presence step, and a shelf the treble step.
        assert_eq!(bold.spec.tone.stereo[0].gain_db, -1.6);
        assert!(bold.spec.tone.stereo[1].gain_db >= 1.0);
        // The presence step goes no further than the bound of Clear plus
        // `VIVID_PRESENCE_MOVE_ABOVE_DB`.
        let ratio = |tone: &Tone| {
            let mut stereo = tone.stereo.clone();
            stereo.retain(|b| !matches!(b.kind, Kind::HighShelf));
            broad_tone::predicted_db(&spectrum, &stereo, &tone.mid, None)
        };
        let source = broad_tone::source_presence_db(&spectrum);
        let bound = broad_tone::presence_move_maximum_db(
            source,
            broad_tone::reference_presence_db(&spectrum),
        ) + broad_tone::VIVID_PRESENCE_MOVE_ABOVE_DB;
        assert!(
            ratio(&bold.spec.tone) - source <= bound + 0.01
                || bold.spec.tone.stereo[1].gain_db == 1.0,
            "{} against {bound}",
            ratio(&bold.spec.tone) - source
        );
        assert!(bold
            .spec
            .tone
            .stereo
            .iter()
            .any(|b| matches!(b.kind, Kind::HighShelf) && b.gain_db > 0.0));
        assert!(bold.gates.low_end);
        // A Clear without a presence move still gets a Vivid with one.
        let none = Spec {
            phase_alignment: None,
            tone: Tone {
                stereo: vec![shelf],
                mid: spec.tone.mid[..4].to_vec(),
                ..spec.tone.clone()
            },
            ..spec.clone()
        };
        let vivid = bold_candidates(
            &none,
            None,
            none.compressor,
            None,
            &spectrum,
            24_000.0,
            false,
        );
        assert!(vivid[0]
            .spec
            .tone
            .stereo
            .iter()
            .any(|b| b.hz == broad_tone::PRESENCE_BELL_HZ && b.gain_db > 0.0));
    }

    #[test]
    fn the_bold_of_a_dark_capture_steps_within_the_bound_of_a_capture() {
        // Presence 12 dB under the body: a dark capture.
        let centers: Vec<f64> = (0..200)
            .map(|i| 20.0 * 1000f64.powf(i as f64 / 199.0))
            .collect();
        let power: Vec<f64> = centers
            .iter()
            .map(|&hz| {
                if (broad_tone::PRESENCE_HZ.0..broad_tone::PRESENCE_HZ.1).contains(&hz) {
                    10f64.powf(-1.2)
                } else {
                    1.0
                }
            })
            .collect();
        let spectrum = crate::Spectrum {
            sample_rate: 48000,
            centers_hz: centers,
            mid_power: power.clone(),
            side_power: vec![0.0; power.len()],
            band_power: power,
            mel_power: Vec::new(),
            analyzed_frames: 1,
        };
        let presence = broad_tone::presence_balance(&spectrum, &[], &[], None, true)
            .1
            .expect("a dark capture takes a presence bell");
        let spec = Spec {
            phase_alignment: None,
            finishing: unchanged_finishing(),
            tone: Tone {
                stereo: vec![presence],
                ..Tone::default()
            },
            focus_envelopes: Vec::new(),
            side_highpass_hz: None,
            side_gain: None,
            deesser: None,
            low_mid: None,
            low_end_hold: None,
            mono_safe: Vec::new(),
            dynamic_after_eq: Vec::new(),
            compressor: None,
            limiter: dsp::LimiterSettings::default(),
            clipper_db: BASE_CLIPPER_DB,
            ceiling_dbtp: SET_CEILING_DBTP,
            initial_drive_db: 0.0,
            target_lufs: -10.0,
            fixed_drive_db: None,
            restore_peak_tone: false,
            restoration_db: [0.0; 6],
            true_peak: TRUE_PEAK,
        };
        let bell_of = |captured: bool| {
            bold_candidates(
                &spec,
                None,
                spec.compressor,
                None,
                &spectrum,
                24_000.0,
                captured,
            )
            .first()
            .and_then(|c| {
                c.spec
                    .tone
                    .stereo
                    .iter()
                    .find(|b| b.hz == broad_tone::PRESENCE_BELL_HZ)
                    .copied()
            })
        };
        let source = broad_tone::source_presence_db(&spectrum);
        let reference = broad_tone::reference_presence_db(&spectrum);
        // The bold of a capture steps past the bell of its master, within
        // the bound of a capture plus `VIVID_PRESENCE_MOVE_ABOVE_DB`.
        let captured = bell_of(true).expect("a capture bold");
        assert!(captured.gain_db > presence.gain_db);
        let moved = broad_tone::predicted_db(&spectrum, &[captured], &[], None) - source;
        assert!(
            moved
                <= crate::capture::presence_move_maximum_db(source, reference)
                    + broad_tone::VIVID_PRESENCE_MOVE_ABOVE_DB
                    + 0.01,
            "{moved}"
        );
        // With the bound of a mix, bold steps within the bound of a mix
        // plus `VIVID_PRESENCE_MOVE_ABOVE_DB` (D206), and no further than
        // the bold of a capture.
        if let Some(mix) = bell_of(false) {
            let moved = broad_tone::predicted_db(&spectrum, &[mix], &[], None) - source;
            assert!(
                moved
                    <= broad_tone::presence_move_maximum_db(source, reference)
                        + broad_tone::VIVID_PRESENCE_MOVE_ABOVE_DB
                        + 0.01,
                "{moved}"
            );
            assert!(mix.gain_db <= captured.gain_db + 1e-9);
        }
    }

    #[test]
    fn dense_target_beyond_headroom_falls_within_it() {
        // A Master at -10.03 LUFS with 6.3 dB maximum reduction has 1.7 dB
        // of headroom, 1.13 LU of target.
        let base = Spec {
            phase_alignment: None,
            finishing: unchanged_finishing(),
            tone: Tone::default(),
            focus_envelopes: Vec::new(),
            side_highpass_hz: None,
            side_gain: None,
            deesser: None,
            low_mid: None,
            low_end_hold: None,
            mono_safe: Vec::new(),
            dynamic_after_eq: Vec::new(),
            compressor: None,
            limiter: dsp::LimiterSettings::default(),
            clipper_db: BASE_CLIPPER_DB,
            ceiling_dbtp: SET_CEILING_DBTP,
            initial_drive_db: 0.0,
            target_lufs: -10.03,
            fixed_drive_db: None,
            restore_peak_tone: true,
            restoration_db: [0.0; 6],
            true_peak: TRUE_PEAK,
        };
        let template = DenseTemplate {
            target_lufs: vec![-8.79, -9.79],
            glue: None,
            clipper_db: DENSER_CLIPPER_DB,
            limiter_maximum_db: DENSE_LIMITER_MAXIMUM_DB,
            limiter_mean_db: DENSE_LIMITER_MEAN_DB,
            attack_loss_db: DENSE_ATTACK_LOSS_DB,
        };
        let targets: Vec<f64> = dense_candidates(&base, 4.5, -10.03, 6.3, &template)
            .iter()
            .map(|c| c.spec.target_lufs)
            .collect();
        assert_eq!(targets.len(), 1);
        assert!((targets[0] - (-10.03 + 1.7 / DENSE_REDUCTION_PER_LU)).abs() < 1e-9);
        assert!(dense_candidates(&base, 4.5, -10.03, 7.6, &template).is_empty());
    }

    #[test]
    fn shared_foundation_precedes_family_tone_and_shifts_its_envelope() {
        let filter = |hz| EqBand {
            kind: Kind::Peak,
            hz,
            q: 1.0,
            gain_db: 1.5,
        };
        let points = vec![
            BlendPoint {
                sample: 0,
                wet: 0.5,
            },
            BlendPoint {
                sample: 95999,
                wet: 0.5,
            },
        ];
        let foundation = Foundation {
            rule: "set.all-role-foundation.v1",
            regions: vec![Region {
                role: "bass",
                domain: Domain::Mid,
                centre_hz: 100.0,
                q: 1.0,
                positive_seconds: 12.0,
                predicted_pair_ratio_gain_db: 0.3,
                score_second_db: 3.6,
                mean_wet_duty: 0.5,
                gain_db: 1.5,
            }],
            tone: Tone {
                mid: vec![filter(100.0)],
                ..Tone::default()
            },
            envelopes: vec![FocusEnvelope {
                domain: Domain::Mid,
                filter_index: 0,
                points: points.clone(),
            }],
        };
        let branch = Tone {
            mid: vec![filter(1000.0)],
            ..Tone::default()
        };
        let (tone, envelopes, scale) = with_foundation(
            branch,
            vec![FocusEnvelope {
                domain: Domain::Mid,
                filter_index: 0,
                points,
            }],
            &foundation,
            48000,
        )
        .unwrap();
        assert_eq!(tone.mid.len(), 2);
        assert_eq!(
            envelopes.iter().map(|e| e.filter_index).collect::<Vec<_>>(),
            vec![0, 1]
        );
        assert_eq!(scale, 1.0);
        let centers: Vec<f64> = (0..200)
            .map(|i| 20.0 * 1000f64.powf(i as f64 / 199.0))
            .collect();
        let pink: Vec<f64> = centers.iter().map(|hz| 100.0 / hz).collect();
        let spectrum = crate::Spectrum {
            sample_rate: 48000,
            centers_hz: centers,
            band_power: pink.clone(),
            mid_power: pink.clone(),
            side_power: pink.iter().map(|p| p * 0.01).collect(),
            mel_power: Vec::new(),
            analyzed_frames: 1,
        };
        let audio = crate::width::tests::mix(0.2, 0.1);
        let widening = width::widening(&crate::stereo::analyze(&audio, 48000).unwrap());
        // Every version is planned, each with its own tone and the gates of
        // its stereo image; Essential carries the shared foundation.
        let branches = branches(&spectrum, None, None, &widening);
        assert_eq!(
            branches.iter().map(|b| b.id.as_str()).collect::<Vec<_>>(),
            ["essential", "master", "open"]
        );
        assert!(branches[0].use_foundation);
        for b in &branches {
            assert!(!b.tone.stereo.is_empty(), "{}", b.id);
            assert_eq!(b.gates.tonal_image, b.id != "essential", "{}", b.id);
            assert_eq!(b.alternatives.len(), 2);
        }
        assert!(json_differs(&branches[0].tone, &branches[1].tone));
        assert!(json_differs(&branches[1].tone, &branches[2].tone));
    }

    #[test]
    fn the_bass_shelves_leave_out_the_gates_of_the_held_bands() {
        // The gate of the equal-loudness bass cut: the band under 120 Hz
        // against 500 Hz to 2 kHz.
        let cut = |gain_db: f64| selection::TonalGate {
            band: 0,
            direction: -1,
            minimum_db: broad_tone::gate_minimum_db(gain_db),
            relative_to: Some(2),
            channel: selection::GateChannel::Stereo,
        };
        let presence = selection::TonalGate {
            band: 3,
            direction: 1,
            minimum_db: broad_tone::gate_minimum_db(2.0),
            relative_to: Some(2),
            channel: selection::GateChannel::Stereo,
        };
        let lead = selection::TonalGate {
            channel: selection::GateChannel::Mid,
            ..presence
        };
        // Both bands under 500 Hz move with the hold: the gate between them
        // stays.
        let low = selection::TonalGate {
            relative_to: Some(1),
            ..cut(1.0)
        };
        let candidate = |tonal: Vec<selection::TonalGate>| Candidate {
            reason: "strongest".into(),
            spec: test_spec(),
            gates: Gates {
                tonal,
                low_end: true,
                ..Gates::default()
            },
        };
        // Cuts of 0.65, 1.03 and 2 dB: the bass shelves lift the bands net
        // of the cut, and the low-end gate checks them, so no gate of the cut
        // stays.
        let shelves = crate::open::plan(&{
            let mut s = crate::Spectrum {
                sample_rate: 48000,
                centers_hz: (0..200)
                    .map(|i| 20.0 * 1000f64.powf(i as f64 / 199.0))
                    .collect(),
                band_power: Vec::new(),
                mid_power: Vec::new(),
                side_power: Vec::new(),
                mel_power: Vec::new(),
                analyzed_frames: 1,
            };
            s.band_power = vec![1.0; s.centers_hz.len()];
            s.mid_power = s.band_power.clone();
            s.side_power = vec![0.01; s.centers_hz.len()];
            s
        })
        .tone
        .stereo;
        assert_eq!(shelves.len(), 2);
        for gain_db in [-0.65, -1.03, -2.0] {
            let bass = bass_candidates(
                &[candidate(vec![cut(gain_db), presence, lead, low])],
                &shelves,
            );
            assert!(crate::open::carries_bass_shelves(&bass[0].spec.tone));
            assert_eq!(bass[0].reason, "strongest with the bass shelves");
            assert_eq!(bass[0].gates.tonal, vec![presence, lead, low]);
            assert!(bass[0].gates.low_end);
        }
        // The held bands are the two bands under 500 Hz.
        assert_eq!(
            (0..6).map(crate::low_end::lifts).collect::<Vec<_>>(),
            [true, true, false, false, false, false]
        );
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ListeningMeasurement<'a> {
    pub output: &'a Metrics,
    pub output_stereo: &'a stereo::StereoAnalysis,
    pub lufs: f64,
    pub mean_reduction_db: f64,
}

fn listening_measurement(report: &Report) -> ListeningMeasurement<'_> {
    ListeningMeasurement {
        output: &report.output,
        output_stereo: &report.output_stereo,
        lufs: report.render.lufs,
        mean_reduction_db: report.render.limiter.mean_reduction_db,
    }
}

/// `compared_with` names the Essential render in the text: Essential, or
/// the mix when Essential is not delivered because it matches the mix.
pub(crate) fn describe_measured(
    mix: pipeline::Mix<'_>,
    report: ListeningMeasurement<'_>,
    essential: Option<ListeningMeasurement<'_>>,
    compared_with: &str,
    mono_safe: bool,
) -> String {
    let lufs = report.lufs;
    let mut changes = Vec::new();
    let (reference, loudness) = essential
        .map_or((mix.stereo, mix.input.integrated_lufs.unwrap()), |r| {
            (r.output_stereo, r.lufs)
        });
    let names = [
        "bass",
        "lower mids",
        "midrange",
        "presence",
        "treble",
        "air",
    ];
    let material_a = selection::material_bands(reference);
    let material_b = selection::material_bands(report.output_stereo);
    let mut tonal: Vec<_> = reference
        .whole
        .bands
        .iter()
        .zip(&report.output_stereo.whole.bands)
        .enumerate()
        .filter_map(|(i, (a, b))| {
            let pa = a.left_power + a.right_power;
            let pb = b.left_power + b.right_power;
            if !material_a[i] || !material_b[i] || pa < 1e-20 || pb < 1e-20 {
                return None;
            }
            let delta = 10.0 * (pb / pa).log10() + loudness - lufs;
            (delta.abs() >= 0.4).then_some((i, delta))
        })
        .collect();
    tonal.sort_by(|a, b| b.1.abs().total_cmp(&a.1.abs()));
    // The two strongest bands share one sentence, and name the matched level
    // once.
    let tonal: Vec<String> = tonal
        .into_iter()
        .take(2)
        .enumerate()
        .map(|(n, (i, delta))| {
            let word = match (delta > 0.0, n == 0) {
                (true, true) => "More",
                (true, false) => "more",
                (false, true) => "Less",
                (false, false) => "less",
            };
            format!("{word} {} ({delta:+.1} dB)", names[i])
        })
        .collect();
    if !tonal.is_empty() {
        changes.push(format!("{} at matched level", tonal.join(" and ")));
    }
    let width = report.output.side_mid_db
        - essential.map_or(mix.input.side_mid_db, |r| r.output.side_mid_db);
    let mono = !mix.stereo.whole.bands.iter().any(|b| b.side_present);
    if mono
        && report
            .output_stereo
            .whole
            .bands
            .iter()
            .any(|b| b.side_present)
    {
        // 500 Hz to 12 kHz, as the width gate: the mono bass would hide it.
        let bands = &report.output_stereo.whole.bands[2..5];
        let side: f64 = bands.iter().map(|b| b.side_power).sum();
        let mid: f64 = bands.iter().map(|b| b.mid_power).sum();
        changes.push(format!(
            "Stereo width added to the mono recording ({:.1} dB side/mid, 500 Hz–12 kHz)",
            10.0 * (side / mid.max(1e-30)).log10()
        ));
    } else if width.abs() >= 0.3 && !mono {
        changes.push(format!(
            "{} stereo balance ({width:+.1} dB side/mid)",
            if width > 0.0 {
                "Broader"
            } else {
                "More centred"
            }
        ));
    }
    if mono_safe {
        changes.push("Holds the parts that cancel on a single speaker".into());
    }
    if let Some(base) = essential {
        let change = report.mean_reduction_db - base.mean_reduction_db;
        if change.abs() >= 0.15 {
            changes.push(format!(
                "{} peak compression ({change:+.1} dB average)",
                if change > 0.0 { "More" } else { "Less" }
            ));
        }
        if (lufs - base.lufs).abs() >= 0.3 {
            changes.push(format!("Delivered level {:+.1} LU", lufs - base.lufs));
        }
    }
    // The sentence for no measured change names what the version stays close
    // to, so "Compared with" does not follow it.
    let compared = essential.is_some() && !changes.is_empty();
    if changes.is_empty() {
        let louder = lufs - loudness >= 0.3;
        let limited = report.mean_reduction_db >= 0.01;
        changes.push(match (essential.is_some(), louder, limited) {
            (true, _, _) => format!(
                "Subtle change to peak shape. Tone and stereo image stay close to {compared_with}"
            ),
            (false, true, true) => {
                "Your mix's own tone and stereo image, louder and with its peaks controlled".into()
            }
            (false, true, false) => "Your mix's own tone and stereo image, louder".into(),
            (false, false, true) => {
                "Your mix's own tone, stereo image and loudness, with its peaks controlled".into()
            }
            (false, false, false) => "Your mix's own tone, stereo image and loudness".into(),
        });
    }
    format!(
        "{}. {:.1} LUFS.{}",
        changes.join(". "),
        lufs,
        if compared {
            format!(" Compared with {compared_with}.")
        } else {
            String::new()
        }
    )
}

/// Name of the mix in a duplicate screen and in descriptions.
pub const ORIGINAL_ID: &str = "original";
pub const MIX_NAME: &str = "your mix";

/// True when a version matches the mix within `selection::DISTINCT_DB` in
/// delivered level and in every measure of the duplicate screen. Such a
/// version sounds the same as the mix at any playback level and is not
/// delivered (D140).
pub fn same_as_mix(source: pipeline::Mix<'_>, signature: &Signature) -> bool {
    let lufs = source.input.integrated_lufs.unwrap_or(f64::NAN);
    let mix = Signature::new(source.input, source.stereo, lufs, 0.0);
    (signature.lufs - lufs).abs() < selection::DISTINCT_DB && signature.is_duplicate_of(&mix)
}

/// Display names describe the completed sound. Requested density or a proposed
/// filter alone is insufficient to claim its intended result in the name.
pub(crate) fn measured_label(
    spec: &Spec,
    report: ListeningMeasurement<'_>,
    essential: Option<ListeningMeasurement<'_>>,
) -> String {
    let Some(base) = essential else {
        return ESSENTIAL_LABEL.into();
    };
    let denser = report.mean_reduction_db - base.mean_reduction_db >= 0.15;
    let matching_db = base.lufs - report.lufs;
    let regions = ["bass", "low mids", "body", "presence", "treble", "air"];
    let mut focus = Vec::new();
    for (domain, filters) in [("mid", &spec.tone.mid), ("side", &spec.tone.side)] {
        for filter in filters {
            let Some(i) = stereo::BAND_EDGES_HZ
                .windows(2)
                .position(|r| filter.hz >= r[0] && filter.hz < r[1])
            else {
                continue;
            };
            let a = &base.output_stereo.whole.bands[i];
            let b = &report.output_stereo.whole.bands[i];
            if !selection::material_bands(base.output_stereo)[i]
                || !selection::material_bands(report.output_stereo)[i]
            {
                continue;
            }
            let name = if domain == "mid"
                && a.mid_present
                && b.mid_present
                && 10.0 * (b.mid_power / a.mid_power).log10() + matching_db >= 0.3
            {
                Some(format!("More centre {}", regions[i]))
            } else if domain == "side"
                && a.side_present
                && b.side_present
                && a.side_mid_db
                    .zip(b.side_mid_db)
                    .is_some_and(|(a, b)| b - a >= 0.3)
            {
                Some(format!("Broader {}", regions[i]))
            } else {
                None
            };
            if let Some(name) = name {
                if !focus.contains(&name) {
                    focus.push(name);
                }
            }
        }
    }
    let strongest_band = || {
        base.output_stereo
            .whole
            .bands
            .iter()
            .zip(&report.output_stereo.whole.bands)
            .enumerate()
            .filter_map(|(i, (a, b))| {
                if !selection::material_bands(base.output_stereo)[i]
                    || !selection::material_bands(report.output_stereo)[i]
                {
                    return None;
                }
                let change = 10.0
                    * ((b.left_power + b.right_power) / (a.left_power + a.right_power)).log10()
                    + matching_db;
                (change.abs() >= 0.5).then_some((i, change))
            })
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
            .map(|(i, change)| {
                format!(
                    "{} {}",
                    if change > 0.0 { "More" } else { "Less" },
                    regions[i]
                )
            })
    };
    let mut name = if !focus.is_empty() {
        let mut name = focus.join(" + ");
        if !spec.tone.stereo.is_empty() {
            if let Some(tonal) = strongest_band() {
                name = format!("{name} · {tonal}");
            }
        }
        name
    } else {
        let strongest = base
            .output_stereo
            .whole
            .bands
            .iter()
            .zip(&report.output_stereo.whole.bands)
            .enumerate()
            .filter_map(|(i, (a, b))| {
                if !selection::material_bands(base.output_stereo)[i]
                    || !selection::material_bands(report.output_stereo)[i]
                {
                    return None;
                }
                let change = 10.0
                    * ((b.left_power + b.right_power) / (a.left_power + a.right_power)).log10()
                    + matching_db;
                (change.abs() >= 0.5).then_some((i, change))
            })
            .max_by(|a, b| a.1.abs().total_cmp(&b.1.abs()));
        if let Some((i, change)) = strongest {
            format!(
                "{} {}",
                if change > 0.0 { "More" } else { "Less" },
                regions[i]
            )
        } else if denser {
            "Denser".into()
        } else if report.output.side_mid_db - base.output.side_mid_db >= 0.3 {
            "Broader stereo".into()
        } else if report.output.side_mid_db - base.output.side_mid_db <= -0.3 {
            "More centred".into()
        } else if !spec.tone.mid.is_empty() || !spec.tone.side.is_empty() {
            "Subtle focus variation".into()
        } else {
            "Alternate peak shape".into()
        }
    };
    if denser && name != "Denser" {
        name = format!("Denser · {name}");
    }
    name
}

/// Denser versions derive from the chosen base candidate at render time.
#[derive(Clone, Serialize, serde::Deserialize)]
pub struct DenseTemplate {
    /// Targets in order; a later target is tried when the limiter bound fails.
    pub target_lufs: Vec<f64>,
    pub glue: Option<compressor::Settings>,
    pub clipper_db: f64,
    pub limiter_maximum_db: f64,
    pub limiter_mean_db: f64,
    pub attack_loss_db: f64,
}

#[derive(Clone, Copy, Serialize, serde::Deserialize, PartialEq, Debug)]
pub enum Role {
    /// Rendered from its own candidates.
    Base,
    /// Derived from the chosen base candidate at the denser targets.
    Dense,
    /// The chosen base candidate with the presence and treble steps of
    /// `broad_tone::vivid_tone`; gated for mono compatibility and the
    /// low-end hold.
    Bold,
}

#[derive(Clone, Serialize)]
pub struct WorkerVersion {
    pub id: String,
    pub label: String,
    pub rule: &'static str,
    pub basis: String,
    pub dense: bool,
    pub role: Role,
    /// Base candidates, strongest first. Empty for a derived version.
    pub candidates: Vec<Candidate>,
    pub dense_template: Option<DenseTemplate>,
    /// Side gain of a Bold version.
    pub side_gain: Option<width::SideGain>,
    /// Compressor of a Bold version: Bold takes the Dense style in place of
    /// the compressor of its base (`set.compression.v3`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub compressor: Option<compressor::Settings>,
    /// Target of a Bold version, the level of a single; none keeps the
    /// target of its base.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_lufs: Option<f64>,
    /// Render only when the named version was not delivered.
    pub fallback_for: Option<String>,
    /// A Bold version in its own family scales this base candidate, which
    /// the base version's family is expected to choose. It renders beside
    /// the base family instead of after it; the finalizer keeps it when the
    /// base family chose that candidate, with or without the bass step, or
    /// chose none.
    pub speculative_base: Option<SpeculativeBase>,
    /// A Bold version after its base in the same family renders only when
    /// the base chose a candidate other than this one: the speculative Bold
    /// covers that candidate.
    pub unless_base_candidate: Option<usize>,
    /// The low shelves of a base version with a bass part
    /// (`set.bass-version.v4`): the shelves of `open::plan`; empty for none.
    /// The version renders the `bass_candidates` of its candidates and then,
    /// when none passes, the candidates themselves (`base_ladder`). A Bold
    /// version keeps the candidates as its base.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub bass_shelves: Vec<EqBand>,
}

/// The rule of the bass version (D164, D182).
pub const BASS_RULE: &str = "set.bass-version.v4";

/// The candidates of the bass version: each candidate with the bass shelves
/// `shelves` on its stereo tone (`set.bass-version.v4`). The low-end hold
/// counts the shelves as intended moves (`low_end::intended_db`), and the
/// low-end gate checks each section of the held bands. A tonal gate on a
/// band that the shelves move, against a band that they do not move or the
/// whole file, verifies a planned move that the shelves change. The bass
/// candidates leave such a gate out, and with it its prediction.
pub fn bass_candidates(candidates: &[Candidate], shelves: &[EqBand]) -> Vec<Candidate> {
    let held = |gate: &selection::TonalGate| {
        crate::low_end::lifts(gate.band)
            && gate
                .relative_to
                .is_none_or(|other| !crate::low_end::lifts(other))
    };
    candidates
        .iter()
        .map(|c| {
            let mut c = c.clone();
            c.spec.tone.stereo.extend(shelves.iter().copied());
            c.reason = format!("{} with the bass shelves", c.reason);
            c.gates.tonal.retain(|gate| !held(gate));
            c
        })
        .collect()
}

/// Render the candidates of a base version strongest first and keep the
/// first that passes its gates (`selection::ladder`). A version with bass
/// shelves renders its `bass_candidates`. When none passes, it renders the
/// same candidates without the shelves, and the version is delivered as
/// the plain version (`set.bass-version.v4`). With `lift`, a chosen
/// candidate with bass shelves renders again with the shelves of `sub_lift`
/// and keeps that render when it passes its gates (`set.sub-lift.v1`). A
/// request on a delivered version renders without `lift`: its candidates
/// carry the shelves of the parent, sub lift included. The trials of the
/// ladders are returned in order. An index names the same planned candidate
/// in each ladder, as `WorkerVersion::unless_base_candidate` and
/// `SpeculativeBase::candidate` do.
#[allow(clippy::too_many_arguments)]
pub fn base_ladder(
    renderer: &mut Renderer<'_>,
    input: &[Stereo],
    source: &SourceAnalysis,
    reference: Option<&selection::Reference>,
    reference_spec: Option<&Spec>,
    candidates: &[Candidate],
    bass_shelves: &[EqBand],
    lift: bool,
    mut progress: impl FnMut(usize, &str) -> Result<()>,
) -> Result<(Option<selection::Chosen>, Vec<CandidateTrial>)> {
    let mut trials = Vec::new();
    let mut chosen = None;
    let bass = bass_candidates(candidates, bass_shelves);
    let mut ladder: &[Candidate] = &bass;
    if !bass_shelves.is_empty() {
        let (bass_chosen, tried) = selection::ladder(
            renderer,
            input,
            source,
            reference,
            reference_spec,
            &bass,
            &mut progress,
        )?;
        trials = tried;
        chosen = bass_chosen;
    }
    if chosen.is_none() {
        ladder = candidates;
        let (plain_chosen, plain) = selection::ladder(
            renderer,
            input,
            source,
            reference,
            reference_spec,
            candidates,
            &mut progress,
        )?;
        trials.extend(plain);
        chosen = plain_chosen;
    }
    let Some(first) = chosen else {
        return Ok((None, trials));
    };
    let Some(lifted) = lift
        .then(|| sub_lift(source, &first, &ladder[first.index]))
        .flatten()
    else {
        return Ok((Some(first), trials));
    };
    let index = first.index;
    let (second, mut tried) =
        selection::one(renderer, input, source, reference, &lifted, |_, reason| {
            progress(index, reason)
        })?;
    for trial in &mut tried {
        trial.index = index;
    }
    trials.extend(tried);
    Ok((
        Some(match second {
            Some(mut second) => {
                second.index = index;
                second
            }
            None => first,
        }),
        trials,
    ))
}

/// The second candidate of a chosen base candidate with bass shelves
/// (`set.sub-lift.v1`). The chain after the tone, the compressor and peak
/// control above all, takes part of the low end that the shelves give. The
/// mid regions of the render (`open::mid_regions_db`) fall short of those
/// that the tone predicts on the mix; `open::lift` solves the shelves again
/// with each shortfall. The candidate keeps the gates of `planned`, the
/// candidate that was chosen, and solves its low-end hold again. None
/// without bass shelves or a shortfall of `open::LIFT_MINIMUM_DB`.
pub fn sub_lift(
    source: &SourceAnalysis,
    chosen: &selection::Chosen,
    planned: &Candidate,
) -> Option<Candidate> {
    use crate::finishing::Domain;
    if !crate::open::carries_bass_shelves(&chosen.spec.tone) {
        return None;
    }
    let rate = source.sample_rate;
    let (tone, envelopes) = chosen.spec.effective_tone();
    let predicted = crate::open::mid_regions_db(&source.spectrum, |hz| {
        selection::domain_response_db(&tone.stereo, &envelopes, Domain::Stereo, hz, rate)
            + selection::domain_response_db(&tone.mid, &envelopes, Domain::Mid, hz, rate)
    });
    let delivered = crate::open::mid_regions_db(
        &crate::spectrum(&chosen.measured.rendered.audio, rate),
        |_| 0.0,
    );
    let stereo = crate::open::lift(
        &source.spectrum,
        &chosen.spec.tone.stereo,
        (predicted.0 - delivered.0, predicted.1 - delivered.1),
    )?;
    let mut spec = chosen.spec.clone();
    spec.tone.stereo = stereo;
    spec.low_end_hold = None;
    // A lift past the response bound of `Tone::validate` is not rendered.
    spec.effective_tone().0.validate(rate).ok()?;
    Some(Candidate {
        reason: format!("{} with the sub lift", planned.reason),
        spec,
        gates: planned.gates.clone(),
    })
}

/// The label and the basis of a delivered base version, planned with bass
/// shelves when `planned_shelves` is true, whose chosen spec is `spec`. A
/// bass version whose candidates with the shelves all failed carries the
/// plain label, and its basis states the rule (`set.bass-version.v4`).
pub fn delivered_base(
    label: &str,
    basis: &str,
    planned_shelves: bool,
    spec: &Spec,
) -> (String, String) {
    if planned_shelves && !crate::open::carries_bass_shelves(&spec.tone) {
        (
            MASTER_LABEL.into(),
            format!("{basis}; {BASS_RULE}: no candidate with the bass shelves passed its gates, so the version renders its candidates without the shelves"),
        )
    } else {
        (label.into(), basis.into())
    }
}

/// The basis of a `bold` version whose base version chose no candidate. It
/// builds from the planned first candidate of the base without the bass
/// shelves, which the speculative `bold` of the cloud steps
/// (`set.bass-version.v4`).
pub fn unchosen_base_basis(basis: &str) -> String {
    format!("{basis}; {BASS_RULE}: the base version chose no candidate, so this version builds from its planned first candidate without the bass shelves")
}

#[derive(Clone, Serialize)]
pub struct SpeculativeBase {
    /// The base version, rendered by another family.
    pub version: String,
    pub candidate: usize,
    pub spec: Spec,
}

/// The chosen base spec with its measured render.
type BaseChoice = (Spec, selection::Measured);

/// Vivid candidates from the chosen Clear spec: the tone of Vivid
/// (`broad_tone::vivid_tone`) with the Vivid widening, then the same tone
/// with the widening midway between Clear's and Vivid's, then with Clear's
/// widening, then Clear's own tone with the Vivid widening. The corrections
/// (the equal-loudness shelves, the focus bells, the side cut, the low-mid
/// correction and the de-esser) stay as Clear has them; Vivid makes its own
/// low-end hold. `spectrum` is the spectrum of the source and `top_hz` the
/// top of its band (its codec cut, D142); `capture` is true for a captured
/// recording (`set.capture-presence.v1`). The filters are at the rate of the
/// spectrum. Each candidate takes `compressor`, the Dense style of the set
/// (`set.compression.v3`), and `target_lufs` when it is set, the level of a
/// single. None when Vivid would equal Clear.
pub fn bold_candidates(
    base: &Spec,
    widening: Option<width::SideGain>,
    compressor: Option<compressor::Settings>,
    target_lufs: Option<f64>,
    spectrum: &crate::Spectrum,
    top_hz: f64,
    capture: bool,
) -> Vec<Candidate> {
    let rate = spectrum.sample_rate;
    let tone = &base.tone;
    let widens = widening.is_some_and(|w| w.beside(tone, spectrum, rate) != base.side_gain);
    let mut spec = base.clone();
    spec.low_end_hold = None;
    spec.compressor = compressor;
    if let Some(target) = target_lufs {
        spec.initial_drive_db += target - spec.target_lufs;
        spec.target_lufs = target;
    }
    let clear = spec.clone();
    // The tone carries the presence and treble steps by itself, with the
    // side gain of Clear; the widening gives the width step.
    spec.tone = broad_tone::vivid_tone(
        tone,
        spectrum,
        base.side_gain.as_ref(),
        base.side_gain.as_ref(),
        widening.as_ref().or(base.side_gain.as_ref()),
        top_hz >= broad_tone::TREBLE_HZ.1,
        capture,
    );
    let moves = json_differs(&spec.tone, tone);
    if !moves && !widens {
        return Vec::new();
    }
    if spec.tone.validate(rate).is_err() {
        return Vec::new();
    }
    let gates = Gates {
        tonal_image: true,
        low_end: true,
        under_own_point: true,
        ..Gates::default()
    };
    let mut candidates = Vec::new();
    // Both widenings are family targets; the tone's side lift counts toward
    // them.
    let master_target = base.side_gain.map(|g| g.with(&base.tone, spectrum, rate));
    if widens {
        spec.side_gain = widening.and_then(|w| w.beside(&spec.tone, spectrum, rate));
    }
    candidates.push(Candidate {
        reason: "vivid tone with the vivid widening".into(),
        spec: spec.clone(),
        gates: gates.clone(),
    });
    if let (true, Some(bold)) = (widens, widening) {
        let master = master_target.unwrap_or(width::SideGain::gains([0.0; 6]));
        let mut midway = spec.clone();
        midway.side_gain = master.midway(&bold).beside(&midway.tone, spectrum, rate);
        candidates.push(Candidate {
            reason: "vivid tone with the widening midway to the vivid".into(),
            spec: midway,
            gates: gates.clone(),
        });
        // The low-end fallbacks, each without one of the two moves that can
        // take the bass: the Vivid tone with the widening of Clear, then the
        // tone of Clear with the Vivid widening.
        if moves {
            let mut narrow = spec;
            narrow.side_gain = master_target.and_then(|t| t.beside(&narrow.tone, spectrum, rate));
            candidates.push(Candidate {
                reason: "vivid tone with the clear widening".into(),
                spec: narrow,
                gates: gates.clone(),
            });
            let mut fallback = clear;
            fallback.side_gain = widening.and_then(|w| w.beside(&fallback.tone, spectrum, rate));
            candidates.push(Candidate {
                reason: "clear tone with the vivid widening".into(),
                spec: fallback,
                gates,
            });
        }
    }
    candidates
}

/// Candidates of a dense version from the chosen base spec: at each target,
/// glue then plain. Every candidate is gated on limiter maximum and mean
/// reduction and on attack loss against the base; a family with no passing
/// candidate delivers no Denser version. A target beyond the base render's
/// limiter headroom falls to the largest target within it. The search starts
/// from the base render's drive corrected for the target difference.
pub fn dense_candidates(
    base: &Spec,
    base_drive_db: f64,
    base_lufs: f64,
    base_limiter_maximum_db: f64,
    template: &DenseTemplate,
) -> Vec<Candidate> {
    let mut candidates = Vec::new();
    // The denser render adds about 1.5 dB of maximum reduction per LU of
    // target above the base's.
    let headroom = template.limiter_maximum_db - base_limiter_maximum_db;
    // A target beyond the headroom falls to the largest target within it.
    let within = base_lufs + headroom / DENSE_REDUCTION_PER_LU;
    let mut targets: Vec<f64> = Vec::new();
    for t in template.target_lufs.iter().map(|t| t.min(within)) {
        // The base render meets its target within the search tolerance, so
        // a target one 0.5 LU step above the base target stays a step.
        if t - base_lufs >= 0.5 - treatment_render::LOUDNESS_TOLERANCE_LU
            && targets.iter().all(|u| (u - t).abs() >= 0.25)
        {
            targets.push(t);
        }
    }
    for target in targets.iter() {
        let mut plain = base.clone();
        plain.target_lufs = *target;
        plain.clipper_db = template.clipper_db;
        plain.fixed_drive_db = None;
        plain.initial_drive_db = base_drive_db + (target - base_lufs);
        if let Some(glue) = template.glue {
            let mut with_glue = plain.clone();
            with_glue.compressor = Some(glue);
            candidates.push(Candidate {
                reason: format!("glue at {target:.2} LUFS"),
                spec: with_glue,
                gates: Gates {
                    limiter_maximum_db: Some(template.limiter_maximum_db),
                    limiter_mean_db: Some(template.limiter_mean_db),
                    attack_loss_db: Some(template.attack_loss_db),
                    glue: true,
                    ..Gates::default()
                },
            });
        }
        candidates.push(Candidate {
            reason: format!("plain at {target:.2} LUFS"),
            spec: plain,
            gates: Gates {
                limiter_maximum_db: Some(template.limiter_maximum_db),
                limiter_mean_db: Some(template.limiter_mean_db),
                attack_loss_db: Some(template.attack_loss_db),
                ..Gates::default()
            },
        });
    }
    candidates
}

#[derive(Serialize)]
pub struct WorkerFamily {
    pub id: String,
    /// Essential base spec at the base density; gates of this family's base
    /// version compare against its render. Absent for the essential family.
    pub reference: Option<Spec>,
    pub versions: Vec<WorkerVersion>,
}

#[derive(Serialize)]
pub struct WorkerPlan {
    pub schema: &'static str,
    /// The render rate of the source, in Hz. The sample positions of the
    /// plan are at this rate.
    pub sample_rate: u32,
    pub source_pcm_sha256: String,
    pub source_frames: usize,
    /// Reported in the finished set for the app (D142).
    pub source_condition: source_condition::SourceCondition,
    /// The planner facts of a request on a delivered version (D171).
    pub facts: crate::adjust::Facts,
    pub families: Vec<WorkerFamily>,
}

pub struct PreparedSet {
    source: SourceAnalysis,
    density: DensityPolicy,
    source_condition: source_condition::SourceCondition,
    /// The keyed side cut of the Mono-safe preset when an instrument group
    /// is anti-phase (D175): `width::mono_safe`, `adjust.mono-safe.v1`. No
    /// version carries it as made (D176).
    mono_safe: Vec<crate::treatment::EnvelopedBand>,
    phase_alignment: Option<stereo_phase::Alignment>,
    focus: Option<source_treatments::ProposalSet>,
    evidence: Option<stem_evidence::Evidence>,
    limiter: dsp::LimiterSettings,
    limiter_rule: &'static str,
    limiter_basis: String,
    proposals: Vec<Branch>,
    broad_tone: broad_tone::Analysis,
    low_end: width::LowEnd,
    side_cut: width::SideCut,
    widening: width::Widening,
    /// The low end of the `open` version (`open::plan`).
    open: crate::open::Open,
    deesser: Option<deesser::Plan>,
    /// The low-mid correction of Clear and Vivid (`set.low-mid.v1`).
    low_mid: Option<crate::treatment::EnvelopedBand>,
    glue: Option<compressor::Settings>,
    compression: Option<compressor::Plan>,
}

/// The audibility point of the base chain of `input` (`set.audible-loudness.v6`):
/// the compressor of Essential and peak control at the clip budget of the
/// base target, as `prepare` measures it, without phase alignment and stem
/// timing. None for a source that arrives limited or that has no point. The
/// album check uses it for the level of each track (`album::measure`).
pub fn audibility_point(input: &[Stereo], rate: u32) -> Result<Option<f64>> {
    let source = pipeline::analyze_source(input, rate)?;
    if input_limited(&source) {
        return Ok(None);
    }
    let mut prefix = input.to_vec();
    if let Some(c) = compressor::plan(input, None, rate) {
        compressor::process(
            &mut prefix,
            c.style(compression_style("essential")).settings,
            rate,
        )?;
    }
    let limiter = dsp::LimiterSettings {
        release_ms: LIMITER_RELEASE_MS,
        ..dsp::LimiterSettings::default()
    };
    let crest_target = density(&source, None).base_target_lufs;
    Ok(audibility::point(
        &prefix,
        base_drive_db(&source, crest_target),
        BASE_CLIPPER_DB,
        SET_CEILING_DBTP,
        limiter,
        dsp::TruePeak::Trim,
        Some(&source.temporal),
        rate,
    )?
    .map(|p| p.lufs))
}

/// Make one source-bound plan before independent treatment-family rendering.
/// `input` is the source at its render rate `rate`.
pub fn prepare(
    input: &[Stereo],
    rate: u32,
    evidence: Option<stem_evidence::Evidence>,
    auditory: Option<AuditoryInput<'_>>,
) -> Result<PreparedSet> {
    prepare_with(input, rate, evidence, auditory, Options::default())
}

/// `prepare` with `options`.
pub fn prepare_with(
    input: &[Stereo],
    rate: u32,
    evidence: Option<stem_evidence::Evidence>,
    auditory: Option<AuditoryInput<'_>>,
    options: Options,
) -> Result<PreparedSet> {
    crate::progress::step("mix");
    let source = pipeline::analyze_source(input, rate)?;
    if evidence.as_ref().is_some_and(|e| {
        e.source_pcm_sha256 != source.source_pcm_sha256 || e.source_frames != input.len()
    }) {
        return Err("Stem evidence belongs to a different source".into());
    }
    let imported_focus = auditory
        .map(|a| {
            if a.source_pcm_sha256 != source.source_pcm_sha256 {
                return Err("Auditory diagnostics are bound to different source PCM".into());
            }
            source_treatments::propose(a.json, a.source)
        })
        .transpose()?;
    let focus = imported_focus
        .as_ref()
        .or_else(|| evidence.as_ref().and_then(|e| e.auto_focus.as_ref()));
    // A source that arrives limited arrives compressed: no compressor.
    let limited = input_limited(&source);
    crate::progress::step("condition");
    let source_condition = source_condition::measure(input, &source, limited, evidence.as_ref());
    // The keyed side cut of the Mono-safe preset where an instrument group
    // cancels (`adjust.mono-safe.v1`), planned once for the facts of the set.
    let mono_safe = if source_condition.antiphase_stems.is_empty() {
        Vec::new()
    } else {
        let mut bands: Vec<usize> = source_condition
            .antiphase_stems
            .iter()
            .flat_map(|s| s.bands.iter().copied())
            .collect();
        bands.sort_unstable();
        bands.dedup();
        width::mono_safe(&source.stereo, &bands, rate)
    };
    // A source with an anti-phase band is aligned before every other stage;
    // the stages that measure the mix measure the aligned mix (D143).
    let phase_alignment = (!source_condition.antiphase_bands.is_empty())
        .then(|| stereo_phase::measure(input, rate))
        .flatten();
    let aligned = phase_alignment.as_ref().map(|a| {
        let mut audio = input.to_vec();
        a.apply(&mut audio, rate);
        audio
    });
    let mix: &[Stereo] = aligned.as_deref().unwrap_or(input);
    let compression = (!limited)
        .then(|| {
            compressor::plan(
                mix,
                evidence.as_ref().and_then(|e| {
                    compressor::Timing::from_stems(
                        e.drum_decay_p50_ms,
                        e.drum_interval_p50_ms,
                        e.bass_body_hz,
                    )
                }),
                rate,
            )
        })
        .flatten();
    let limiter = dsp::LimiterSettings {
        release_ms: LIMITER_RELEASE_MS,
        ..dsp::LimiterSettings::default()
    };
    // Without a compressor the prefix is the mix itself.
    let prefix = (!limited)
        .then(|| -> Result<std::borrow::Cow<'_, [Stereo]>> {
            Ok(match &compression {
                // The prefix of Essential, which sets the base target.
                Some(c) => {
                    let mut prefix = mix.to_vec();
                    compressor::process(
                        &mut prefix,
                        c.style(compression_style("essential")).settings,
                        rate,
                    )?;
                    prefix.into()
                }
                None => mix.into(),
            })
        })
        .transpose()?;
    // One 4x prefix serves the audibility point, the base render and the
    // attack render.
    let mut prefix_up = None;
    let audible = if let Some(prefix) = &prefix {
        crate::progress::step("audibility");
        let crest_target = density(&source, None).base_target_lufs;
        let mut up = crate::scratch::Pcm::new(crate::scratch::temporary()?);
        crate::render_blocks::upsample_into(prefix, &mut up, rate)?;
        let point = audibility::point_on(
            prefix,
            &mut up,
            base_drive_db(&source, crest_target),
            BASE_CLIPPER_DB,
            SET_CEILING_DBTP,
            limiter,
            // The loudness targets and the widening are calibrated on the peak
            // control with the trim of the whole render, so they measure with it
            // and not with `TRUE_PEAK` (D166).
            dsp::TruePeak::Trim,
            Some(&source.temporal),
            rate,
        )?;
        prefix_up = Some(up);
        point
    } else {
        None
    };
    let mut density = density(&source, audible);
    if let Some(album) = options.album_target_lufs {
        density.album_target_lufs = Some(album);
        density.base_target_lufs = density.base_target_lufs.min(album);
        density.bold_target_lufs = density.bold_target_lufs.map(|t| t.min(album));
        density.denser_target_lufs = density.denser_target_lufs.filter(|t| *t <= album);
        density.denser_step_lu = density
            .denser_target_lufs
            .map(|t| t - density.base_target_lufs);
    }
    let base_target_lufs = density.base_target_lufs;
    let base_render = match (prefix_up, &prefix, &mut density.audibility) {
        (Some(mut up), Some(prefix), Some(point)) => {
            // The base target can sit under the probes of the point; the
            // render searches its drive and joins the probes, so the attack
            // choice compares the two peak controls at matched loudness.
            crate::progress::step("level");
            let (drive_db, controlled, fit, probe) = audibility::render_at(
                &mut up,
                prefix,
                point,
                base_target_lufs,
                BASE_CLIPPER_DB,
                SET_CEILING_DBTP,
                limiter,
                dsp::TruePeak::Trim,
                Some(&source.temporal),
                rate,
            )?;
            point.probes.push(probe);
            Some((up, drive_db, controlled, fit))
        }
        _ => None,
    };
    // The widening starts from the image after the compressor and peak
    // control at the base target: peak control lowers the loud centre more
    // than the sides and widens the image by itself.
    crate::progress::step("width");
    let widening = match &base_render {
        Some((_, _, controlled, _)) => width::widening(&stereo::analyze(controlled, rate)?),
        None => match &aligned {
            Some(audio) => width::widening(&stereo::analyze(audio, rate)?),
            None => width::widening(&source.stereo),
        },
    };
    crate::progress::step("attack");
    let attack = peak_attack(
        &source_condition,
        prefix.as_deref(),
        density.audibility.as_ref(),
        base_render,
        limiter,
        rate,
    )?;
    drop(prefix);
    drop(aligned);
    // Equal-loudness compensation keeps the balance that a person heard at
    // the mix's level. A captured recording has no such level: no
    // compensation (D146).
    let delivered_gain_lu = if source_condition.capture() {
        0.0
    } else {
        density.base_target_lufs.min(STREAMING_NORMALISATION_LUFS)
            - source.input.integrated_lufs.unwrap_or(-14.0)
    };
    crate::progress::step("tone");
    let mut broad = broad_tone::analyze(
        evidence.as_ref(),
        delivered_gain_lu,
        Some(&source.spectrum),
        widening.master.as_ref(),
        source_condition.bandwidth_hz.unwrap_or(rate as f64 / 2.0),
        source_condition.device_capture(),
    );
    broad.retain_measurable(selection::material_bands(&source.stereo));
    let low_end = width::low_end(&source);
    let side_cut = width::side_cut(&source);
    let glue = compressor::glue_settings(input, rate);
    let open = crate::open::plan(&source.spectrum);
    let proposals = branches(&source.spectrum, focus, side_cut.band, &widening);
    let refinement = evidence.as_ref().and_then(|e| e.refinement.as_ref());
    if let Some(r) = refinement {
        if r.source_pcm_sha256 != source.source_pcm_sha256
            || r.stem_pcm_sha256 != evidence.as_ref().unwrap().stem_pcm_sha256
        {
            return Err("Source refinement is bound to different PCM".into());
        }
    }
    let deesser = evidence
        .as_ref()
        .and_then(|e| deesser::Plan::from_key(e.deesser_key.clone(), options.deesser));
    // The cut of the whole-track deficit is the largest cut of the dynamic
    // correction; the key of the stems sets where it applies.
    let low_mid = broad_tone::master(&broad, rate)
        .and_then(|m| m.low_mid_cut_db)
        .zip(evidence.as_ref().and_then(|e| e.low_mid.as_ref()))
        .and_then(|(cut, key)| key.correction(cut, input.len(), rate));
    let limiter_rule = "set.limiter-release.v1";
    let limiter_basis = format!(
        "{LIMITER_RELEASE_MS:.0} ms release: the lowest masked disturbance of peak control that also lowers the disturbance under 510 Hz on the confirmation mixes; set.peak-attack.v1: {}",
        attack.basis
    );
    let limiter = dsp::LimiterSettings {
        attack_ms: attack.attack_ms,
        ..limiter
    };
    Ok(PreparedSet {
        source,
        density,
        source_condition,
        mono_safe,
        phase_alignment,
        focus: focus.cloned(),
        evidence,
        limiter,
        limiter_rule,
        limiter_basis,
        proposals,
        broad_tone: broad,
        low_end,
        side_cut,
        widening,
        open,
        deesser,
        low_mid,
        glue,
        compression,
    })
}

/// The limiter attack of the versions of a source, and the reason.
struct PeakAttack {
    attack_ms: f64,
    basis: String,
}

/// The attack of the versions of a source (D166). A mono source keeps the instant
/// attack: live sets are mono, and they take their own path. Another source
/// takes `PEAK_ATTACK_MS` when peak control of its base prefix with the
/// attack and the local true-peak correction has a masked noise-to-mask
/// ratio at most that of the instant peak control with the trim of the whole
/// render at the same loudness, and keeps at least `PEAK_ATTACK_GAIN_DB`
/// more onset energy.
fn peak_attack(
    condition: &source_condition::SourceCondition,
    prefix: Option<&[Stereo]>,
    point: Option<&audibility::Point>,
    base_render: Option<BaseRender>,
    limiter: dsp::LimiterSettings,
    rate: u32,
) -> Result<PeakAttack> {
    if condition.mono {
        return Ok(PeakAttack {
            attack_ms: 0.0,
            basis: "instant attack: a mono recording, the path of live sets".into(),
        });
    }
    let (Some(prefix), Some(point), Some((mut up, drive_db, base, fit))) =
        (prefix, point, base_render)
    else {
        return Ok(PeakAttack {
            attack_ms: 0.0,
            basis: "instant attack: no base render".into(),
        });
    };
    // One temporal analysis of the prefix serves both losses. The loss of
    // the base render comes first, so the base render is freed before the
    // attack render.
    let onsets = analysis::analyze(prefix, rate);
    let base_loss = analysis::energy_attack_loss_p90_from(&onsets, &base, rate);
    drop(base);
    // The attack render needs no trim, so it starts at the drive of the
    // trimmed render less that trim. The probes of the audibility point are
    // trimmed renders of the same prefix: their masked noise-to-mask ratio at
    // the loudness of the attack render is the reference.
    let (attacked, _) = audibility::peak_control(
        &mut up,
        drive_db + fit.trim_db,
        BASE_CLIPPER_DB,
        SET_CEILING_DBTP,
        dsp::LimiterSettings {
            attack_ms: PEAK_ATTACK_MS,
            ..limiter
        },
        TRUE_PEAK,
        rate,
    )?;
    let lufs = dsp::loudness(&attacked, rate).ok_or("Missing attack render loudness")?;
    let attacked_loss = analysis::energy_attack_loss_p90_from(&onsets, &attacked, rate);
    let nmr = [
        point.nmr_at(lufs),
        audibility::peak_control_nmr_db(prefix, &attacked, rate),
    ];
    let gain = base_loss
        .zip(attacked_loss)
        .map_or(0.0, |(base, attacked)| base - attacked);
    let takes = nmr[1] <= nmr[0] && gain >= PEAK_ATTACK_GAIN_DB;
    Ok(PeakAttack {
        attack_ms: if takes { PEAK_ATTACK_MS } else { 0.0 },
        basis: format!(
            "{} attack: at {lufs:.2} LUFS, masked noise-to-mask ratio {:.2} dB with the {PEAK_ATTACK_MS:.0} ms attack and {:.2} dB with the instant attack and the trim; energy attack loss P90 lower by {gain:.2} dB",
            if takes { "3 ms" } else { "instant" },
            nmr[1],
            nmr[0],
        ),
    })
}

/// The trimmed peak control of the base prefix at the base target: the 4x
/// prefix, the drive, the render and its true-peak correction.
type BaseRender = (
    crate::scratch::Pcm,
    f64,
    Vec<Stereo>,
    crate::render_blocks::TruePeakFit,
);

impl PreparedSet {
    /// The compressor of the version `id` (`compression_style`).
    fn compressor_of(&self, id: &str) -> Option<compressor::Settings> {
        self.compression
            .as_ref()
            .map(|c| c.style(compression_style(id)).settings)
    }

    fn spec(
        &self,
        id: &str,
        tone: Tone,
        focus_envelopes: Vec<FocusEnvelope>,
        finishing: finishing::Plan,
    ) -> Spec {
        Spec {
            phase_alignment: self.phase_alignment.clone(),
            finishing,
            tone,
            focus_envelopes,
            side_highpass_hz: self.low_end.side_highpass_hz,
            side_gain: None,
            deesser: None,
            low_mid: None,
            low_end_hold: None,
            mono_safe: Vec::new(),
            dynamic_after_eq: Vec::new(),
            compressor: self.compressor_of(id),
            limiter: self.limiter,
            clipper_db: BASE_CLIPPER_DB,
            ceiling_dbtp: SET_CEILING_DBTP,
            initial_drive_db: base_drive_db(&self.source, self.density.base_target_lufs),
            target_lufs: self.density.base_target_lufs,
            fixed_drive_db: None,
            restore_peak_tone: true,
            restoration_db: [0.0; 6],
            true_peak: TRUE_PEAK,
        }
    }

    /// Merge refinement and the shared foundation into one branch tone.
    fn merged(&self, branch: &Branch, tone: &Tone) -> Result<(Tone, Vec<FocusEnvelope>, f64, f64)> {
        let refinement = self.evidence.as_ref().and_then(|e| e.refinement.as_ref());
        let foundation = self.evidence.as_ref().and_then(|e| e.foundation.as_ref());
        let (mut branch_tone, branch_envelopes, refinement_scale) =
            refinement.map_or_else(|| Ok((tone.clone(), Vec::new(), 1.0)), |r| r.apply(tone))?;
        // A side bell under 500 Hz widens the bass and the low mids: bound
        // it after the refinement sets its measured strength.
        for band in &mut branch_tone.side {
            if band.hz < LOW_SIDE_BELL_HZ {
                band.gain_db = band.gain_db.clamp(-LOW_SIDE_BELL_DB, LOW_SIDE_BELL_DB);
            }
        }
        let (tone, envelopes, character_scale) = match foundation {
            Some(f) if branch.use_foundation => {
                with_foundation(branch_tone, branch_envelopes, f, self.source.sample_rate)?
            }
            _ => (branch_tone, branch_envelopes, 1.0),
        };
        Ok((tone, envelopes, refinement_scale, character_scale))
    }

    /// The planner facts of a request on a delivered version (D171).
    pub fn facts(&self) -> crate::adjust::Facts {
        crate::adjust::Facts::of(
            &self.source,
            self.density.audibility.as_ref().map(|p| p.lufs),
            self.source_condition
                .bandwidth_hz
                .unwrap_or(self.source.sample_rate as f64 / 2.0),
            (!self.mono_safe.is_empty()).then(|| self.mono_safe.clone()),
        )
    }

    pub fn worker_plan(&self) -> Result<WorkerPlan> {
        let refinement = self.evidence.as_ref().and_then(|e| e.refinement.as_ref());
        let foundation = self.evidence.as_ref().and_then(|e| e.foundation.as_ref());
        let mut families = Vec::new();
        let essential_branch = &self.proposals[0];
        let (essential_tone, essential_envelopes, _, _) =
            self.merged(essential_branch, &essential_branch.tone)?;
        let essential_spec = self.spec(
            "essential",
            essential_tone,
            essential_envelopes,
            essential_branch.finishing.clone(),
        );
        for branch in &self.proposals {
            // Each version widens by its own gains (`set.widening.v6`).
            let family_widening = self.widening.of(&branch.id);
            let mut candidates = Vec::new();
            let mut tones: Vec<(String, Tone, Gates)> = Vec::new();
            tones.push((
                "strongest".into(),
                branch.tone.clone(),
                branch.gates.clone(),
            ));
            for (reason, tone, factor) in &branch.alternatives {
                let mut gates = branch.gates.clone();
                for gate in &mut gates.tonal {
                    gate.minimum_db = (gate.minimum_db * factor).max(0.2);
                }
                tones.push((reason.clone(), tone.clone(), gates));
            }
            let mut basis = branch.basis.clone();
            let mut noted = false;
            for (reason, tone, gates) in tones {
                let (tone, envelopes, refinement_scale, character_scale) =
                    self.merged(branch, &tone)?;
                if !noted {
                    if let Some(f) = foundation.filter(|_| branch.use_foundation) {
                        basis.push_str(&format!(
                            "; shared foundation {}: {}. Character gain scale {character_scale:.6}",
                            f.rule,
                            serde_json::to_string(&f.regions)?
                        ));
                    }
                    if refinement.is_some() && branch.id != "essential" {
                        basis.push_str(&format!("; measured strength/passage evidence: set.source-refinement.v1; refinement gain scale {refinement_scale:.6}. Full evidence is in stem_evidence.refinement; applied gains/envelopes are in spec"));
                    }
                    {
                        if let (Some(c), Some(k)) = (
                            &self.low_mid,
                            self.evidence.as_ref().and_then(|e| e.low_mid.as_ref()),
                        ) {
                            basis.push_str(&format!(
                                "; {}: mid cut of up to {:.2} dB at {:.0} Hz, Q {:.2}, in the {} of {} active lead frames where the other stems mask the lead from 250 to 500 Hz",
                                k.rule, -c.band.gain_db, c.band.hz, c.band.q, k.keyed_frames, k.active_frames
                            ));
                        }
                        let c = &self.side_cut;
                        basis.push_str(&format!(
                            "; {}: side/mid {:.2} dB at 2–8 kHz against {:.2} dB at 200 Hz–8 kHz, {}",
                            c.rule,
                            c.upper_side_mid_db,
                            c.body_side_mid_db,
                            c.band.map_or("no side cut".into(), |b| format!(
                                "side bell {:.2} dB at {:.0} Hz",
                                b.gain_db, b.hz
                            ))
                        ));
                    }
                    if self.low_end.side_highpass_hz.is_some() {
                        basis.push_str(&format!(
                            "; {}: side high-pass at {:.0} Hz, side share below 100 Hz {:.1}%",
                            self.low_end.rule,
                            width::LOW_END_MONO_HZ,
                            100.0 * self.low_end.side_share_below_100_hz
                        ));
                    }
                    if let Some(g) = family_widening {
                        basis.push_str(&format!(
                            "; {}: side gains {} dB per band under the window bound {:.2}",
                            self.widening.rule,
                            serde_json::to_string(&g.bands_db)?,
                            width::OPEN_WINDOW_CORRELATION
                        ));
                    }
                    noted = true;
                }
                let mut spec = self.spec(&branch.id, tone, envelopes, branch.finishing.clone());
                let mut gates = gates;
                // No version carries the keyed side cut of `width::mono_safe`
                // as made: on a phone speaker its movement is audible and
                // distracting (D176). The cut is the Mono-safe preset of a request.
                // Every version carries the de-esser of `set.deesser.v4` (a
                // cut only with the switch on), cuts the masked low mids and
                // holds its bass at the level of Essential.
                spec.deesser = self.deesser.clone();
                spec.low_mid = self.low_mid.clone();
                gates.low_end = true;
                // Each version stays at or under its own audibility point
                // (`set.audible-loudness.v6`).
                gates.under_own_point = true;
                if let Some(gain) = family_widening {
                    // The tone's own side lift counts toward the widening.
                    spec.side_gain =
                        gain.beside(&spec.tone, &self.source.spectrum, self.source.sample_rate);
                    // A widened version after Essential passes the mono and
                    // correlation gates against Essential.
                    gates.tonal_image = branch.id != "essential";
                }
                candidates.push(Candidate {
                    reason,
                    spec,
                    gates,
                });
            }
            // A version whose widening fails the stereo gates falls back to
            // half the widening with the strongest tone.
            if let (Some(gain), Some(first), false) = (
                family_widening,
                candidates.first(),
                branch.id == "essential",
            ) {
                let mut half = first.clone();
                half.reason = format!("{} with half the widening", first.reason);
                half.spec.side_gain = gain.half().beside(
                    &first.spec.tone,
                    &self.source.spectrum,
                    self.source.sample_rate,
                );
                candidates.push(half);
            }
            let has_master = self.proposals.iter().any(|b| b.id == "master");
            // The low shelves are part of the tone of each version
            // (`character::Character::sub_db`), and the sub lift of
            // `base_ladder` applies to them.
            let bass_shelves = Vec::new();
            let mut versions = vec![WorkerVersion {
                id: branch.id.clone(),
                label: branch.label.clone(),
                rule: branch.rule,
                basis: basis.clone(),
                dense: false,
                role: Role::Base,
                candidates,
                dense_template: None,
                side_gain: None,
                compressor: None,
                target_lufs: None,
                fallback_for: None,
                speculative_base: None,
                unless_base_candidate: None,
                bass_shelves,
            }];
            if let Some(denser) = self.density.denser_target_lufs.filter(|_| DENSER_VERSIONS) {
                let mut targets = vec![denser];
                let lower = denser - 1.0;
                if lower >= self.density.base_target_lufs + 0.5 {
                    targets.push(lower);
                }
                versions.push(WorkerVersion {
                    id: format!("dense-{}", branch.id),
                    label: if branch.id == "master" {
                        DENSER_MASTER_LABEL.into()
                    } else {
                        DENSER_ESSENTIAL_LABEL.into()
                    },
                    rule: branch.rule,
                    basis: format!("{basis}; dense version from the chosen base candidate; glue trial {} and limiter bound {:.0} dB", if self.glue.is_some() && GLUE_TRIAL {"available"} else {"off"}, DENSE_LIMITER_MAXIMUM_DB),
                    dense: true,
                    role: Role::Dense,
                    candidates: Vec::new(),
                    dense_template: Some(DenseTemplate {
                        target_lufs: targets,
                        glue: self.glue.filter(|_| GLUE_TRIAL && branch.id == "essential"),
                        clipper_db: DENSER_CLIPPER_DB,
                        limiter_maximum_db: DENSE_LIMITER_MAXIMUM_DB,
                        limiter_mean_db: DENSE_LIMITER_MEAN_DB,
                        attack_loss_db: DENSE_ATTACK_LOSS_DB,
                    }),
                    side_gain: None,
                    compressor: None,
                    target_lufs: None,
                    fallback_for: (branch.id == "essential" && has_master)
                        .then(|| "dense-master".to_string()),
                    speculative_base: None,
                    unless_base_candidate: None,
                    bass_shelves: Vec::new(),
                });
            }
            // Bold renders in its own family from the first master
            // candidate, beside the master family; the master family renders
            // Bold itself after choosing any other candidate.
            let bold = (branch.id == "master").then(|| WorkerVersion {
                id: "bold".into(),
                label: BOLD_LABEL.into(),
                rule: "set.master.v1",
                basis: format!("{basis}; the chosen clear candidate with the presence and treble steps of set.vivid-steps.v2"),
                dense: false,
                role: Role::Bold,
                candidates: Vec::new(),
                dense_template: None,
                side_gain: self.widening.bold,
                compressor: self.compressor_of("bold"),
                target_lufs: self.density.bold_target_lufs,
                fallback_for: None,
                speculative_base: None,
                unless_base_candidate: Some(0),
                bass_shelves: Vec::new(),
            });
            let speculative =
                bold.as_ref()
                    .zip(versions[0].candidates.first())
                    .map(|(bold, first)| {
                        let mut bold = bold.clone();
                        bold.unless_base_candidate = None;
                        bold.speculative_base = Some(SpeculativeBase {
                            version: branch.id.clone(),
                            candidate: 0,
                            spec: first.spec.clone(),
                        });
                        bold
                    });
            versions.extend(bold);
            families.push(WorkerFamily {
                id: branch.id.clone(),
                reference: (branch.id != "essential").then(|| essential_spec.clone()),
                versions,
            });
            if let Some(bold) = speculative {
                families.push(WorkerFamily {
                    id: "bold".into(),
                    reference: Some(essential_spec.clone()),
                    versions: vec![bold],
                });
            }
        }
        // The reference of the other families is the first candidate of
        // Essential, with every stage of Essential (D206).
        if let Some(first) = families
            .first()
            .and_then(|f: &WorkerFamily| f.versions.first())
            .and_then(|v| v.candidates.first())
            .map(|c| c.spec.clone())
        {
            for family in families.iter_mut().skip(1) {
                if family.reference.is_some() {
                    family.reference = Some(first.clone());
                }
            }
        }
        Ok(WorkerPlan {
            schema: crate::distributed::PLAN_SCHEMA,
            sample_rate: self.source.sample_rate,
            source_pcm_sha256: self.source.source_pcm_sha256.clone(),
            source_frames: self.source.source_frames,
            source_condition: self.source_condition.clone(),
            facts: self.facts(),
            families,
        })
    }
}

/// Longest recording whose versions after Essential render beside each
/// other (`Sink::parallel_versions`), in seconds. Each render holds several
/// copies of the audio; a longer recording renders its versions in turn.
pub const PARALLEL_VERSIONS_SECONDS: f64 = 300.0;

/// The ladder of a later version, rendered beside the version in its turn.
struct Ahead {
    /// The candidates that it rendered.
    candidates: Vec<Candidate>,
    result: std::result::Result<(Option<selection::Chosen>, Vec<CandidateTrial>), String>,
    /// The candidate events of the ladder, in order.
    events: Vec<(usize, String)>,
    work: treatment_render::Work,
    seconds: f64,
    /// The ladder stopped at a candidate because `stop` was set.
    stopped: bool,
}

/// A later version that can render beside the version in its turn.
struct Later {
    id: String,
    /// The base version and the candidate that a speculative Bold scales.
    scales: Option<(String, usize)>,
    candidates: Vec<Candidate>,
    role: Role,
    bass_shelves: Vec<EqBand>,
}

/// A ladder that renders ahead on its own thread, with its stop flag.
struct Pending<'scope> {
    id: String,
    scales: Option<(String, usize)>,
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread: std::thread::ScopedJoinHandle<'scope, Ahead>,
}

/// Start the ladder of `later` on a thread of `scope`.
fn spawn_later<'scope, 'env>(
    scope: &'scope std::thread::Scope<'scope, 'env>,
    input: &'env [Stereo],
    rate: u32,
    source: &'env SourceAnalysis,
    reference: std::sync::Arc<(Spec, selection::Reference)>,
    later: Later,
) -> Pending<'scope> {
    let Later {
        id,
        scales,
        candidates,
        role,
        bass_shelves,
    } = later;
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let flag = stop.clone();
    Pending {
        id,
        scales,
        stop,
        thread: scope.spawn(move || {
            render_ahead(
                input,
                rate,
                source,
                &reference,
                candidates,
                role,
                &bass_shelves,
                &flag,
            )
        }),
    }
}

/// The ladder of `candidates` against `reference`, the Essential render
/// and its spec, on a renderer of its own. The ladder stops before its next
/// render when `stop` is set.
#[allow(clippy::too_many_arguments)]
fn render_ahead(
    input: &[Stereo],
    rate: u32,
    source: &SourceAnalysis,
    reference: &(Spec, selection::Reference),
    candidates: Vec<Candidate>,
    role: Role,
    bass_shelves: &[EqBand],
    stop: &std::sync::atomic::AtomicBool,
) -> Ahead {
    let started = std::time::Instant::now();
    let mut renderer = Renderer::new(input, rate);
    renderer.work.probe_cache_budget_bytes = 0;
    let mut events = Vec::new();
    let mut stopped = false;
    let progress = |index: usize, reason: &str| {
        if stop.load(std::sync::atomic::Ordering::Relaxed) {
            stopped = true;
            return Err("Stopped: the base version renders another candidate".into());
        }
        events.push((index, reason.to_string()));
        Ok(())
    };
    let (spec, measured) = (Some(&reference.0), Some(&reference.1));
    let result = if role == Role::Base {
        base_ladder(
            &mut renderer,
            input,
            source,
            measured,
            spec,
            &candidates,
            bass_shelves,
            true,
            progress,
        )
    } else {
        selection::ladder(
            &mut renderer,
            input,
            source,
            measured,
            spec,
            &candidates,
            progress,
        )
    }
    .map_err(|e| e.to_string());
    Ahead {
        candidates,
        result,
        events,
        work: renderer.work,
        seconds: started.elapsed().as_secs_f64(),
        stopped,
    }
}

/// True when two candidate lists render alike against `reference`: the
/// same reasons, gates and warm-started specs (`selection::ladder`).
fn same_candidates(
    a: &[Candidate],
    b: &[Candidate],
    reference: Option<&selection::Reference>,
) -> bool {
    let key = |c: &Candidate| {
        serde_json::to_string(&(
            &c.reason,
            &c.gates,
            &selection::warm_start(&c.spec, reference),
        ))
        .ok()
    };
    a.len() == b.len()
        && a.iter().zip(b).all(|(x, y)| {
            let k = key(x);
            k.is_some() && k == key(y)
        })
}

/// Render every justified branch at source-derived density targets. Source facts
/// are measured once. Each version renders its candidates strongest first and
/// keeps the first that passes its gates. Described duplicates are excluded;
/// taste and preservation bounds are explicit diagnostics on returned versions.
/// `input` is the source at its render rate `rate`.
pub fn render(
    input: &[Stereo],
    rate: u32,
    evidence: Option<stem_evidence::Evidence>,
    auditory: Option<AuditoryInput<'_>>,
    sink: &mut impl Sink,
) -> Result<Summary> {
    render_with(input, rate, evidence, auditory, Options::default(), sink)
}

/// `render` with `options`.
pub fn render_with(
    input: &[Stereo],
    rate: u32,
    evidence: Option<stem_evidence::Evidence>,
    auditory: Option<AuditoryInput<'_>>,
    options: Options,
    sink: &mut impl Sink,
) -> Result<Summary> {
    sink.event("analyzing", "")?;
    let prepared = prepare_with(input, rate, evidence, auditory, options)?;
    let worker_plan = prepared.worker_plan()?;
    // The versions that render in turn; a speculative or a fallback version
    // renders only in place of another.
    crate::progress::report(crate::progress::Stage {
        stage: "plan",
        versions: worker_plan
            .families
            .iter()
            .flat_map(|f| &f.versions)
            .filter(|v| v.speculative_base.is_none() && v.fallback_for.is_none())
            .map(|v| crate::progress::PlannedVersion {
                id: v.id.clone(),
                label: v.label.clone(),
            })
            .collect(),
        ..Default::default()
    });
    let adjust = crate::adjust::Base::of_plan(&serde_json::to_value(&worker_plan)?)?;
    let PreparedSet {
        source,
        density,
        source_condition,
        focus,
        evidence,
        limiter_rule,
        limiter_basis,
        broad_tone,
        low_end,
        open,
        glue,
        compression,
        ..
    } = prepared;
    let constraints = pipeline::diagnostic_constraints();
    let refinement = evidence.as_ref().and_then(|e| e.refinement.as_ref());
    let mut renderer = Renderer::with_scratch(input, rate, sink.scratch_factory());
    // The warm-started searches of a set do not revisit a drive, and in WASM
    // the cache holds its probes in memory.
    renderer.work.probe_cache_budget_bytes = 0;
    let mut trials: Vec<Trial> = Vec::new();
    let mut version_ids = Vec::new();
    let mut sounds = Vec::new();
    let mut signatures: Vec<(String, Signature)> = Vec::new();
    let mut essential_reference: Option<(Spec, selection::Reference)> = None;
    // Families render in plan order. A fallback version waits until every
    // other family has rendered, then renders only if the version it stands
    // in for was not delivered.
    let mut deferred: Vec<(String, WorkerVersion, Option<BaseChoice>)> = Vec::new();
    let mut queue: Vec<(String, Vec<WorkerVersion>)> = worker_plan
        .families
        .into_iter()
        .map(|f| (f.id, f.versions))
        .collect();
    // On a native build the versions after Essential render beside each
    // other and deliver in their turn (`Sink::parallel_versions`).
    let parallel = sink.parallel_versions()
        && cfg!(not(target_arch = "wasm32"))
        && dsp::threads() > 1
        && input.len() as f64 <= PARALLEL_VERSIONS_SECONDS * rate as f64;
    std::thread::scope(|scope| -> Result<()> {
        let mut ahead: Vec<Pending<'_>> = Vec::new();
        // A speculative Bold waits for its base to render a first
        // candidate, and starts when that is the candidate that it scales.
        let mut waiting: Vec<(Later, std::sync::Arc<(Spec, selection::Reference)>)> = Vec::new();
        let mut spawned = false;
        while !queue.is_empty() || !deferred.is_empty() {
            let (family_id, versions, mut base_spec) = if queue.is_empty() {
                let (family_id, version, base) = deferred.remove(0);
                (family_id, vec![version], base)
            } else {
                let (family_id, versions) = queue.remove(0);
                (family_id, versions, None)
            };
            let mut base_measured: Option<selection::Measured> = None;
            // The planned first candidate of the family's base version, without
            // the bass step: the speculative Bold of the cloud scales it.
            let mut planned_base: Option<Spec> = None;
            // The index of the candidate that the base version chose.
            let mut base_index: Option<usize> = None;
            for planned in versions {
                // The set renders Bold in the family of its base. The
                // speculative Bold of the plan renders beside its base
                // (`Sink::parallel_versions`) or not at all.
                if planned.speculative_base.is_some() {
                    continue;
                }
                if let Some(dependency) = &planned.fallback_for {
                    if version_ids.contains(dependency) {
                        sink.event("not-needed", &planned.id)?;
                        continue;
                    }
                    if !queue.is_empty() {
                        deferred.push((family_id.clone(), planned, base_spec.clone()));
                        continue;
                    }
                }
                let WorkerVersion {
                    id,
                    label,
                    rule,
                    basis,
                    dense,
                    role,
                    candidates,
                    dense_template,
                    side_gain,
                    compressor,
                    target_lufs,
                    bass_shelves,
                    unless_base_candidate,
                    ..
                } = planned;
                // A bass version renders the bass candidates and then, when none
                // passes, the planned candidates (`base_ladder`); a Bold version
                // keeps the planned candidate as its base.
                let planned_candidates = candidates.clone();
                // A Bold whose base chose no candidate builds from the planned
                // first base candidate, as the speculative Bold of the cloud does
                // (`set.bass-version.v4`).
                // When the base chose the candidate that the speculative
                // Bold of the cloud scales, Bold builds from that planned
                // candidate, as in the cloud: the own point and the sub lift
                // of the base change its chosen spec.
                let bold_base = match (unless_base_candidate, base_index) {
                    (Some(scaled), Some(chosen)) if scaled == chosen => planned_base.as_ref(),
                    _ => base_spec.as_ref().map(|(b, _)| b).or(planned_base.as_ref()),
                };
                let basis = if role == Role::Bold && base_spec.is_none() {
                    unchosen_base_basis(&basis)
                } else {
                    basis
                };
                let candidates = match role {
                    Role::Base => {
                        planned_base = candidates.first().map(|c| c.spec.clone());
                        candidates
                    }
                    Role::Dense => match (&base_spec, &dense_template) {
                        (Some((base, measured)), Some(template)) => dense_candidates(
                            base,
                            measured.rendered.drive_db,
                            measured.rendered.lufs,
                            measured.rendered.limiter.maximum_reduction_db,
                            template,
                        ),
                        _ => Vec::new(),
                    },
                    Role::Bold => {
                        let candidates = bold_base
                            .map(|b| {
                                bold_candidates(
                                    b,
                                    side_gain,
                                    compressor,
                                    target_lufs,
                                    &source.spectrum,
                                    source_condition.bandwidth_hz.unwrap_or(rate as f64 / 2.0),
                                    source_condition.device_capture(),
                                )
                            })
                            .unwrap_or_default();
                        if candidates.is_empty() {
                            sink.event("no-opportunity", &id)?;
                            continue;
                        }
                        candidates
                    }
                };
                if candidates.is_empty() {
                    // A derived version with nothing to render, such as a Denser
                    // version whose targets all exceed the base's limiter headroom.
                    sink.event("no-opportunity", &id)?;
                    if let Some((base, _)) = &base_spec {
                        trials.push(Trial {
                            id,
                            label,
                            rule,
                            basis,
                            description: None,
                            measured_name: None,
                            detail: None,
                            naming: None,
                            status: "no-opportunity",
                            failure: None,
                            duplicate_of: None,
                            comparisons: Vec::new(),
                            spec: base.clone(),
                            candidate_count: 0,
                            candidates: Vec::new(),
                            report: None,
                        });
                    }
                    continue;
                }
                let candidate_count = candidates.len();
                let mut first_spec = candidates[0].spec.clone();
                if role == Role::Base {
                    first_spec.tone.stereo.extend(bass_shelves.iter().copied());
                }
                sink.event("rendering", &id)?;
                // The later versions that need only Essential render beside the
                // first version after it.
                if parallel && !spawned && family_id != "essential" {
                    spawned = true;
                    if let Some(shared) = essential_reference.clone() {
                        let shared = std::sync::Arc::new(shared);
                        for (_, later) in &queue {
                            for v in later.iter().filter(|v| v.fallback_for.is_none()) {
                                let (candidates, role) = match (&v.speculative_base, v.role) {
                                    (Some(speculative), _) => (
                                        bold_candidates(
                                            &speculative.spec,
                                            v.side_gain,
                                            v.compressor,
                                            v.target_lufs,
                                            &source.spectrum,
                                            source_condition
                                                .bandwidth_hz
                                                .unwrap_or(rate as f64 / 2.0),
                                            source_condition.device_capture(),
                                        ),
                                        Role::Bold,
                                    ),
                                    (None, Role::Base) => (v.candidates.clone(), Role::Base),
                                    _ => continue,
                                };
                                if candidates.is_empty() {
                                    continue;
                                }
                                let later = Later {
                                    id: v.id.clone(),
                                    scales: v
                                        .speculative_base
                                        .as_ref()
                                        .map(|b| (b.version.clone(), b.candidate)),
                                    candidates,
                                    role,
                                    bass_shelves: v.bass_shelves.clone(),
                                };
                                if later.scales.is_some() {
                                    waiting.push((later, shared.clone()));
                                } else {
                                    ahead.push(spawn_later(
                                        scope,
                                        input,
                                        rate,
                                        &source,
                                        shared.clone(),
                                        later,
                                    ));
                                }
                            }
                        }
                    }
                }
                // Base and Bold gates compare with Essential; dense gates compare
                // with their own base render.
                let base_reference = base_spec
                    .as_ref()
                    .filter(|_| role == Role::Dense)
                    .map(|(_, m)| selection::Reference::of(m));
                let (reference, reference_spec) = if role == Role::Dense {
                    (base_reference.as_ref(), None)
                } else {
                    (
                        essential_reference.as_ref().map(|(_, m)| m),
                        essential_reference.as_ref().map(|(s, _)| s),
                    )
                };
                let reference_low = reference
                    .filter(|_| role != Role::Dense)
                    .map(|r| (r.low_sections.clone(), r.lufs));
                // A version that rendered beside an earlier one keeps that
                // render when its candidates render alike; its events follow in
                // their turn.
                let beside = ahead
                    .iter()
                    .position(|p| p.id == id)
                    .map(|i| ahead.remove(i).thread)
                    .map(|h| h.join().unwrap_or_else(|e| std::panic::resume_unwind(e)))
                    .filter(|a| {
                        !a.stopped && same_candidates(&a.candidates, &candidates, reference)
                    });
                let mut rendered_beside = None;
                let ladder = if let Some(a) = beside {
                    for (index, reason) in &a.events {
                        sink.event("candidate", &format!("{id} {index} {reason}"))?;
                    }
                    renderer.work.add(&a.work);
                    rendered_beside = Some(a.seconds);
                    a.result.map_err(Into::into)
                } else {
                    // A speculative Bold starts when its base renders first
                    // the candidate that it scales, and stops when its base
                    // renders another: it can stand only for its own.
                    let (ahead, waiting, source) = (&mut ahead, &mut waiting, &source);
                    let progress = |index: usize, reason: &str| {
                        let mut i = 0;
                        while i < waiting.len() {
                            match &waiting[i].0.scales {
                                Some((v, c)) if *v == id => {
                                    let starts = *c == index;
                                    let (later, shared) = waiting.remove(i);
                                    if starts {
                                        ahead.push(spawn_later(
                                            scope, input, rate, source, shared, later,
                                        ));
                                    }
                                }
                                _ => i += 1,
                            }
                        }
                        for p in ahead.iter() {
                            if p.scales
                                .as_ref()
                                .is_some_and(|(v, c)| *v == id && *c != index)
                            {
                                p.stop.store(true, std::sync::atomic::Ordering::Relaxed);
                            }
                        }
                        sink.event("candidate", &format!("{id} {index} {reason}"))
                    };
                    if role == Role::Base {
                        base_ladder(
                            &mut renderer,
                            input,
                            source,
                            reference,
                            reference_spec,
                            &candidates,
                            &bass_shelves,
                            true,
                            progress,
                        )
                    } else {
                        selection::ladder(
                            &mut renderer,
                            input,
                            source,
                            reference,
                            reference_spec,
                            &candidates,
                            progress,
                        )
                    }
                };
                let result = ladder.and_then(|(chosen, candidate_trials)| {
                    let Some(chosen) = chosen else {
                        return Ok(Err(candidate_trials));
                    };
                    if chosen.measured.rendered.audio.len() != input.len()
                        || chosen.measured.rendered.peak_estimate_dbtp > chosen.spec.ceiling_dbtp + 1e-6
                    {
                        return Err("Rendered delivery length or peak failed".into());
                    }
                    let target = chosen.spec.target_lufs;
                    if family_id == "essential" && role == Role::Base {
                        essential_reference = Some((
                            chosen.spec.clone(),
                            selection::Reference::of(&chosen.measured),
                        ));
                    }
                    if role == Role::Base {
                        // The later versions of the family read the
                        // measurements of the base render; the sink keeps
                        // its audio.
                        let mut measured = chosen.measured.clone();
                        measured.rendered.audio = std::sync::Arc::new(Vec::new());
                        base_measured = Some(measured);
                    }
                    let selection::Measured { rendered, output, output_stereo, preservation, gain_span_db, low_sections, .. } = chosen.measured;
                    let bass_level_db = crate::low_end::bass_level_db(&low_sections, rendered.lufs);
                    let hold_sections = reference_low.as_ref().map(|(sections, lufs)| {
                        // The chosen trial is the last of its index: the ladder of
                        // the instant attack and the plain ladder of a bass
                        // version repeat the indices.
                        let steps = candidate_trials.iter().rev().find(|t| t.index == chosen.index).map_or(&[][..], |t| &t.low_end);
                        crate::adjust::HoldSections::of(&chosen.spec, &source.spectrum, steps, &low_sections, rendered.lufs, sections, *lufs)
                    });
                    let source_constraint_diagnostics =
                        pipeline::violations(&preservation, Some(&rendered.limiter), &constraints);
                    Ok(Ok((chosen.spec.clone(), Report {
                        target_rule: density.rule,
                        target_basis: match &density.audibility {
                            Some(point) if !density.input_limited => format!("Peak control reaches the audibility point at {:.3} LUFS, which caps the crest target {:.3} LUFS; the base target is {:.3} LUFS and the denser step {:?} LU. This {} branch targets {:.3} LUFS", point.lufs,density.crest_target_lufs,density.base_target_lufs,density.denser_step_lu,if dense {"denser"} else {"base"},target),
                            _ => format!("Source 50 ms crest median {:.3} dB sets base target {:.3} LUFS; crest span {:.3} dB supports denser step {:?} LU. This {} branch targets {:.3} LUFS", density.source_crest_p10_p50_p90_db[1],density.base_target_lufs,density.source_crest_p10_p50_p90_db[2]-density.source_crest_p10_p50_p90_db[0],density.denser_step_lu,if dense {"denser"} else {"base"},target),
                        },
                        limiter_rule,
                        limiter_basis: limiter_basis.clone(),
                        output,
                        output_stereo,
                        preservation,
                        gain_span_db,
                        source_constraint_diagnostics,
                        diagnostic_constraints: constraints.clone(),
                        chosen_candidate: chosen.index,
                        candidates: candidate_trials,
                        render: rendered,
                        bass_level_db,
                        hold_sections,
                    })))
                });
                let (spec, report) = match result {
                    Ok(Ok(chosen)) => chosen,
                    Ok(Err(candidate_trials)) => {
                        sink.event("no-opportunity", &id)?;
                        trials.push(Trial {
                            id,
                            label,
                            rule,
                            basis,
                            description: None,
                            measured_name: None,
                            detail: None,
                            naming: None,
                            status: "no-opportunity",
                            failure: None,
                            duplicate_of: None,
                            comparisons: Vec::new(),
                            spec: first_spec,
                            candidate_count,
                            candidates: candidate_trials,
                            report: None,
                        });
                        continue;
                    }
                    Err(error) => {
                        if id == "essential" {
                            return Err(error);
                        }
                        sink.event("render-failed", &id)?;
                        trials.push(Trial {
                            id,
                            label,
                            rule,
                            basis,
                            description: None,
                            measured_name: None,
                            detail: None,
                            naming: None,
                            status: "technical-failure",
                            failure: Some(error.to_string()),
                            duplicate_of: None,
                            comparisons: Vec::new(),
                            spec: first_spec,
                            candidate_count,
                            candidates: Vec::new(),
                            report: None,
                        });
                        continue;
                    }
                };
                let (label, basis) = if role == Role::Base {
                    let base = if !bass_shelves.is_empty() {
                        planned_candidates[report.chosen_candidate].spec.clone()
                    } else {
                        spec.clone()
                    };
                    base_spec = Some((base, base_measured.clone().expect("base measured")));
                    base_index = Some(report.chosen_candidate);
                    delivered_base(&label, &basis, !bass_shelves.is_empty(), &spec)
                } else {
                    (label, basis)
                };
                let signature = Signature::new(
                    &report.output,
                    &report.output_stereo,
                    report.render.lufs,
                    report.render.limiter.mean_reduction_db,
                );
                let residuals = sink.comparisons(&report.render.audio)?;
                let comparisons: Vec<Comparison> = residuals
                    .into_iter()
                    .map(|c| {
                        let described_duplicate = signatures
                            .iter()
                            .any(|(id, s)| *id == c.id && signature.is_duplicate_of(s));
                        Comparison {
                            id: c.id,
                            matched_residual_db: c.matched_residual_db,
                            described_duplicate,
                        }
                    })
                    .collect();
                let duplicate_of = if same_as_mix(source.mix(), &signature) {
                    Some(ORIGINAL_ID.to_string())
                } else {
                    comparisons
                        .iter()
                        .find(|c| c.described_duplicate)
                        .map(|c| c.id.clone())
                };
                let status = candidate_status(rule, duplicate_of.is_some());
                let essential = trials.first().and_then(|t| t.report.as_ref());
                let compared_with = if version_ids.iter().any(|v| v == "essential") {
                    ESSENTIAL_LABEL
                } else {
                    MIX_NAME
                };
                let mut description = describe_measured(
                    source.mix(),
                    listening_measurement(&report),
                    essential.map(listening_measurement),
                    compared_with,
                    !spec.mono_safe.is_empty(),
                );
                if !spec.focus_envelopes.is_empty() {
                    description.push_str(
                        " Tonal intensity follows measured local passages and eases back elsewhere.",
                    );
                }
                // Versions carry their role names. The measured name goes in
                // the trial record: the description states each change once.
                let measured_name = (id != "essential").then(|| {
                    measured_label(
                        &spec,
                        listening_measurement(&report),
                        essential.map(listening_measurement),
                    )
                });
                if status == "retained" {
                    sink.deliver(
                        &Version {
                            id: &id,
                            label: &label,
                            description: &description,
                            rule,
                            basis: &basis,
                            spec: &spec,
                            report: &report,
                            refinement,
                            rendered_beside_seconds: rendered_beside,
                        },
                        &report.render.audio,
                    )?;
                    version_ids.push(id.clone());
                    signatures.push((id.clone(), signature));
                    sounds.push(crate::version_names::sound(
                        &id,
                        source.mix(),
                        listening_measurement(&report),
                        crate::version_names::designed(&id, &spec),
                        report.bass_level_db,
                    ));
                } else {
                    sink.event(status, &id)?;
                }
                // The report keeps measurements only; the sink has persisted PCM.
                // Renderer probe ownership is separately bounded by its cache budget.
                let mut report = report;
                report.render.audio = std::sync::Arc::new(Vec::new());
                trials.push(Trial {
                    id,
                    label,
                    rule,
                    basis,
                    description: Some(description),
                    measured_name,
                    detail: None,
                    naming: None,
                    status,
                    failure: None,
                    duplicate_of,
                    comparisons,
                    spec,
                    candidate_count,
                    candidates: Vec::new(),
                    report: Some(report),
                });
            }
        }
        Ok(())
    })?;
    // The names describe the delivered versions together.
    let named = crate::version_names::name(
        &sounds,
        version_ids.first().is_some_and(|v| v == "essential"),
    );
    sink.name(&named)?;
    for n in named {
        if let Some(t) = trials
            .iter_mut()
            .find(|t| t.id == n.id && t.status == "retained")
        {
            t.label = n.name.into();
            t.detail = t.description.replace(n.description.clone());
            if !n.distinct {
                // No measured step names the version: it is withdrawn.
                t.status = "not-distinct";
                version_ids.retain(|id| *id != n.id);
            }
            t.naming = Some(n);
        }
    }
    Ok(Summary {
        schema: "mastering.set.3",
        validation_status: "provisional-source-led-listening-set",
        source,
        density,
        source_condition,
        focus,
        stem_evidence: evidence,
        broad_tone,
        low_end,
        open,
        glue,
        compression,
        duplicate_rule: DUPLICATE_RULE,
        residual_diagnostic_db: DUPLICATE_DB,
        adjust,
        version_ids,
        trials,
        work: renderer.work.clone(),
    })
}
