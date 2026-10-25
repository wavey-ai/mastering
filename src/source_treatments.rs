//! Source-led treatment proposals, before rendering or choosing versions.
//!
//! The spatial-overlap family ports the experiment of D033. It reads
//! existing aligned-stem diagnostics; it never renders or recombines stems.
use crate::{
    audio_io::Result,
    dsp::{EqBand, Kind},
    treatment::Tone,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const RULE: &str = "treatment.spatial-overlap.v1";
const BANDS: usize = 64;

/// Identity of the actual original file, verified by the caller. The hash is of
/// file bytes, not the engine's rate-converted PCM fingerprint.
pub struct SourceIdentity<'a> {
    pub file_sha256: &'a str,
    pub sample_rate: u32,
    pub frames: usize,
}

#[derive(Clone, Serialize)]
pub struct ProposalSet {
    pub schema: &'static str,
    pub rule: &'static str,
    pub validation_status: &'static str,
    pub source_file_sha256: String,
    pub analysis_sha256: String,
    pub policy: Policy,
    pub proposals: Vec<Proposal>,
    pub notes: Vec<&'static str>,
}

#[derive(Clone, Serialize)]
pub struct Policy {
    pub minimum_hz: f64,
    pub maximum_hz: f64,
    pub minimum_eligible_seconds: f64,
    pub minimum_excitation_share_percent: f64,
    pub maximum_median_vocal_accompaniment_db: f64,
    pub minimum_local_predicted_ratio_gain_db: f64,
    pub minimum_adjacent_filters: usize,
    pub local_probe_gain_db: f64,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            minimum_hz: 200.0,
            maximum_hz: 4000.0,
            minimum_eligible_seconds: 10.0,
            minimum_excitation_share_percent: 0.5,
            maximum_median_vocal_accompaniment_db: 0.0,
            minimum_local_predicted_ratio_gain_db: 0.25,
            minimum_adjacent_filters: 2,
            local_probe_gain_db: 3.0,
        }
    }
}

#[derive(Clone, Serialize)]
pub struct Proposal {
    pub id: String,
    pub rule: &'static str,
    pub rationale: &'static str,
    pub tone: Tone,
    pub basis: Vec<Region>,
}

#[derive(Clone, Serialize)]
pub struct Region {
    pub domain: &'static str,
    pub first_band: usize,
    pub last_band: usize,
    pub centre_hz: f64,
    pub measured_centre_span_hz: [f64; 2],
    pub minimum_eligible_seconds: f64,
    pub minimum_excitation_share_percent: f64,
    pub median_vocal_accompaniment_range_db: [f64; 2],
    pub predicted_median_ratio_gain_range_db: [f64; 2],
}

// Import only the source diagnostics from the existing analysis schema. Paired
// reference spectra, title, and other descriptive text have no policy role.
#[derive(Deserialize)]
struct Analysis {
    schema: String,
    centres_hz: Vec<f64>,
    sources_unchanged: bool,
    files: BTreeMap<String, Original>,
    stems: Option<Stems>,
}

#[derive(Deserialize)]
struct Original {
    sha256: String,
    sample_rate: u32,
    frames: usize,
}

#[derive(Deserialize)]
struct Stems {
    source_sha256: String,
    stem_sha256: BTreeMap<String, String>,
    source_conversion_residual_db: f64,
    reconstruction: Reconstruction,
    active_seconds: f64,
    bands: Vec<Band>,
}

#[derive(Deserialize)]
struct Reconstruction {
    residual_relative_db: f64,
    cosine: f64,
}

#[derive(Deserialize, Serialize)]
pub(crate) struct Band {
    pub(crate) hz: f64,
    pub(crate) valid: bool,
    pub(crate) eligible_seconds: Option<f64>,
    pub(crate) vocal_excitation_share_percent: Option<f64>,
    pub(crate) vocal_to_accompaniment_p10_p50_p90_db: Option<[f64; 3]>,
    pub(crate) local_mid_plus3_ratio_delta_db: Option<f64>,
    pub(crate) local_side_plus3_ratio_delta_db: Option<f64>,
}

fn is_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
}

impl Band {
    fn prediction(&self, domain: &str) -> f64 {
        if domain == "mid" {
            self.local_mid_plus3_ratio_delta_db.unwrap()
        } else {
            self.local_side_plus3_ratio_delta_db.unwrap()
        }
    }

    fn eligible(&self, domain: &str, policy: &Policy) -> bool {
        self.valid
            && (policy.minimum_hz..=policy.maximum_hz).contains(&self.hz)
            && self.eligible_seconds.unwrap() >= policy.minimum_eligible_seconds
            && self.vocal_excitation_share_percent.unwrap()
                >= policy.minimum_excitation_share_percent
            && self.vocal_to_accompaniment_p10_p50_p90_db.unwrap()[1]
                <= policy.maximum_median_vocal_accompaniment_db
            && self.prediction(domain) >= policy.minimum_local_predicted_ratio_gain_db
    }

    fn validate(&self, active_seconds: f64) -> Result<()> {
        if !self.valid {
            return Ok(());
        }
        let seconds = self.eligible_seconds.ok_or("Missing band activity")?;
        let share = self
            .vocal_excitation_share_percent
            .ok_or("Missing vocal share")?;
        let ratios = self
            .vocal_to_accompaniment_p10_p50_p90_db
            .ok_or("Missing overlap ratios")?;
        let mid = self
            .local_mid_plus3_ratio_delta_db
            .ok_or("Missing mid prediction")?;
        let side = self
            .local_side_plus3_ratio_delta_db
            .ok_or("Missing side prediction")?;
        if !seconds.is_finite()
            || !(0.0..=active_seconds + 1e-6).contains(&seconds)
            || !(0.0..=100.0).contains(&share)
            || ratios.iter().any(|v| !v.is_finite())
            || ratios.windows(2).any(|r| r[0] > r[1])
            || !(-3.000001..=3.000001).contains(&mid)
            || !(-3.000001..=3.000001).contains(&side)
        {
            return Err("Invalid auditory band measurements".into());
        }
        Ok(())
    }
}

fn validate(report: &Analysis, source: &SourceIdentity<'_>) -> Result<()> {
    let original = report
        .files
        .get("Original")
        .ok_or("Missing original identity")?;
    if report.schema != "mastering.auditory-analysis.1"
        || !report.sources_unchanged
        || !is_hash(source.file_sha256)
        || original.sha256 != source.file_sha256
        || original.frames != source.frames
        || original.sample_rate != source.sample_rate
        || ![44100, 48000].contains(&source.sample_rate)
        || source.frames < source.sample_rate as usize * 3
    {
        return Err("Auditory analysis does not match the unchanged original source".into());
    }
    if report.centres_hz.len() != BANDS
        || report.centres_hz.iter().enumerate().any(|(i, hz)| {
            let expected = (10f64.powf((3.5 + i as f64 * 0.5) / 21.4) - 1.0) / 0.00437;
            !hz.is_finite() || (hz - expected).abs() > expected * 1e-10
        })
    {
        return Err("Unsupported auditory filter grid".into());
    }
    if let Some(stems) = &report.stems {
        let duration = source.frames as f64 / source.sample_rate as f64;
        if !is_hash(&stems.source_sha256)
            || stems.stem_sha256.len() != 4
            || ["drums", "bass", "other", "vocals"].iter().any(|role| {
                !stems
                    .stem_sha256
                    .get(*role)
                    .is_some_and(|hash| is_hash(hash))
            })
            || !stems.source_conversion_residual_db.is_finite()
            || stems.source_conversion_residual_db > -35.0
            || !stems.reconstruction.residual_relative_db.is_finite()
            || stems.reconstruction.residual_relative_db > -18.0
            || !(0.98..=1.000001).contains(&stems.reconstruction.cosine)
            || !(0.0..=duration + 1e-6).contains(&stems.active_seconds)
            || stems.bands.len() != BANDS
        {
            return Err("Unreliable or incomplete aligned-stem diagnostics".into());
        }
        for (band, hz) in stems.bands.iter().zip(&report.centres_hz) {
            if !band.hz.is_finite() || (band.hz - hz).abs() > hz * 1e-10 {
                return Err("Missing, reordered or mismatched auditory bands".into());
            }
            band.validate(stems.active_seconds)?;
        }
    }
    Ok(())
}

fn region(bands: &[Band], first: usize, end: usize, domain: &'static str) -> Region {
    let group = &bands[first..end];
    let total: f64 = group
        .iter()
        .map(|b| b.vocal_excitation_share_percent.unwrap())
        .sum();
    let centre_hz = (group
        .iter()
        .map(|b| b.hz.ln() * b.vocal_excitation_share_percent.unwrap())
        .sum::<f64>()
        / total)
        .exp();
    let range = |values: Vec<f64>| {
        [
            values.iter().copied().fold(f64::INFINITY, f64::min),
            values.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        ]
    };
    Region {
        domain,
        first_band: first,
        last_band: end - 1,
        centre_hz,
        measured_centre_span_hz: [group[0].hz, group.last().unwrap().hz],
        minimum_eligible_seconds: group
            .iter()
            .map(|b| b.eligible_seconds.unwrap())
            .fold(f64::INFINITY, f64::min),
        minimum_excitation_share_percent: group
            .iter()
            .map(|b| b.vocal_excitation_share_percent.unwrap())
            .fold(f64::INFINITY, f64::min),
        median_vocal_accompaniment_range_db: range(
            group
                .iter()
                .map(|b| b.vocal_to_accompaniment_p10_p50_p90_db.unwrap()[1])
                .collect(),
        ),
        predicted_median_ratio_gain_range_db: range(
            group.iter().map(|b| b.prediction(domain)).collect(),
        ),
    }
}

fn tone(regions: &[Region], policy: &Policy) -> Tone {
    let mut tone = Tone::default();
    for region in regions {
        let filter = EqBand {
            kind: Kind::Peak,
            hz: region.centre_hz,
            q: (region.centre_hz
                / (region.measured_centre_span_hz[1] - region.measured_centre_span_hz[0]))
                .clamp(0.5, 2.0),
            gain_db: policy.local_probe_gain_db,
        };
        if region.domain == "mid" {
            tone.mid.push(filter);
        } else {
            tone.side.push(filter);
        }
    }
    tone
}

/// Build the spatial-overlap proposal family from existing diagnostics.
/// The caller verifies `source` against the file it will render. This imports
/// measurements, not external instructions or reference-master target curves.
pub fn propose(analysis_json: &[u8], source: SourceIdentity<'_>) -> Result<ProposalSet> {
    let report: Analysis = serde_json::from_slice(analysis_json)?;
    validate(&report, &source)?;
    build_proposals(
        source.file_sha256,
        format!("{:x}", Sha256::digest(analysis_json)),
        report.stems.as_ref().map(|stems| stems.bands.as_slice()),
        crate::audio_io::render_rate(source.sample_rate)?,
    )
}

/// The same proposal rule applied to local, source-bound auditory measurements.
/// The caller must first validate the aligned stems against the original PCM.
/// The proposed filters are at the render rate `rate`.
pub(crate) fn propose_measured(
    source_file_sha256: &str,
    source_pcm_sha256: &str,
    active_seconds: f64,
    bands: Vec<Band>,
    rate: u32,
) -> Result<ProposalSet> {
    if !is_hash(source_file_sha256) || !is_hash(source_pcm_sha256) || bands.len() != BANDS {
        return Err("Invalid local auditory measurement identity or grid".into());
    }
    for (i, band) in bands.iter().enumerate() {
        let hz = (10f64.powf((3.5 + i as f64 * 0.5) / 21.4) - 1.0) / 0.00437;
        if (band.hz - hz).abs() > hz * 1e-10 {
            return Err("Invalid local auditory filter grid".into());
        }
        band.validate(active_seconds)?;
    }
    let mut measurement_hash = Sha256::new();
    measurement_hash.update(source_pcm_sha256.as_bytes());
    measurement_hash.update(active_seconds.to_le_bytes());
    measurement_hash.update(serde_json::to_vec(&bands)?);
    build_proposals(
        source_file_sha256,
        format!("{:x}", measurement_hash.finalize()),
        Some(&bands),
        rate,
    )
}

/// The proposals of `bands`, with filters at the render rate `rate`.
fn build_proposals(
    source_file_sha256: &str,
    analysis_sha256: String,
    bands: Option<&[Band]>,
    rate: u32,
) -> Result<ProposalSet> {
    let policy = Policy::default();
    let mut result = ProposalSet {
        schema: "mastering.source-treatments.1",
        rule: RULE,
        validation_status: "provisional-proposals-require-complete-render-evaluation",
        source_file_sha256: source_file_sha256.to_owned(),
        analysis_sha256,
        proposals: vec![Proposal {
            id: "unchanged".into(),
            rule: "treatment.unchanged.v1",
            rationale: "Retain the original tone and spatial balance as a candidate",
            tone: Tone::default(),
            basis: Vec::new(),
        }],
        notes: Vec::new(),
        policy,
    };
    let Some(bands) = bands else {
        result
            .notes
            .push("No aligned-stem diagnostics; no spatial-overlap treatment inferred");
        return Ok(result);
    };
    let mut regions = Vec::new();
    for domain in ["mid", "side"] {
        let mut first = None;
        for i in 0..=bands.len() {
            if i < bands.len() && bands[i].eligible(domain, &result.policy) {
                first.get_or_insert(i);
            } else if let Some(start) = first.take() {
                if i - start >= result.policy.minimum_adjacent_filters {
                    regions.push(region(bands, start, i, domain));
                }
            }
        }
    }
    for region in &regions {
        let tone = tone(std::slice::from_ref(region), &result.policy);
        tone.validate(rate)?;
        result.proposals.push(Proposal {
            id: format!("focus-{}-{:02}-{:02}", region.domain, region.first_band, region.last_band),
            rule: RULE,
            rationale: "Test a localized spatial treatment where measured vocal/accompaniment differences predict a positive local ratio change",
            tone,
            basis: vec![region.clone()],
        });
    }
    if regions.len() > 1 {
        let combined = tone(&regions, &result.policy);
        if combined.validate(rate).is_ok() {
            result.proposals.push(Proposal {
                id: "focus-combined".into(),
                rule: RULE,
                rationale: "Test the combined source-derived regions; measure their interaction through the complete chain",
                tone: combined,
                basis: regions,
            });
        } else {
            result.notes.push("Combined focus exceeds treatment filter/response bounds; individual regions remain eligible");
        }
    }
    if result.proposals.len() == 1 {
        result.notes.push("No adjacent source regions support a spatial-overlap proposal under this provisional rule");
    }
    Ok(result)
}
