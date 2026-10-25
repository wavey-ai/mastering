//! One complete treatment family from a source-bound set plan.
use crate::{
    audio_io::{Result, Stereo},
    delivery,
    dsp::LimiterSettings,
    finishing, pipeline, set, stem_evidence, stereo,
    treatment::Tone,
    treatment_render::{Renderer, Spec, Work},
    waveform,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{fs, io::Read, path::Path, time::Instant};

/// Schema of a worker plan.
pub const PLAN_SCHEMA: &str = "mastering.worker-plan.8";
/// Schema of the result of one family render.
pub const FAMILY_SCHEMA: &str = "mastering.worker-family.2";
/// Schema of the manifest of a finished set.
pub const MANIFEST_SCHEMA: &str = "mastering.distributed-set.4";
/// Schema of the reference record of a plan (`Store::publish_reference`).
pub const REFERENCE_SCHEMA: &str = "mastering.reference-render.1";

/// A version ID: ASCII letters, digits and hyphens, 1 to 80 bytes.
pub fn valid_version_id(id: &str) -> bool {
    (1..=80).contains(&id.len()) && id.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'-')
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputPlan {
    schema: String,
    sample_rate: u32,
    source_pcm_sha256: String,
    source_frames: usize,
    source_condition: Value,
    facts: crate::adjust::Facts,
    families: Vec<InputFamily>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InputFamily {
    id: String,
    reference: Option<Value>,
    versions: Vec<InputVersion>,
}

#[derive(Deserialize)]
struct InputVersion {
    id: String,
    rule: String,
    basis: String,
    role: set::Role,
    candidates: Vec<InputCandidate>,
    dense_template: Option<Value>,
    #[serde(default)]
    side_gain: Option<crate::width::SideGain>,
    #[serde(default)]
    compressor: Option<crate::compressor::Settings>,
    #[serde(default)]
    target_lufs: Option<f64>,
    #[serde(default)]
    fallback_for: Option<String>,
    #[serde(default)]
    label: String,
    #[serde(default)]
    speculative_base: Option<InputSpeculativeBase>,
    #[serde(default)]
    unless_base_candidate: Option<usize>,
    #[serde(default)]
    bass_shelves: Vec<crate::dsp::EqBand>,
    /// The candidates carry the shelves of a delivered version, its sub lift
    /// included (`adjust::worker_plan`), so the version renders no second
    /// sub lift.
    #[serde(default)]
    lifted: bool,
}

#[derive(Deserialize)]
struct InputSpeculativeBase {
    version: String,
    candidate: usize,
    spec: Value,
}

#[derive(Deserialize)]
struct InputCandidate {
    reason: String,
    spec: Value,
    gates: crate::selection::Gates,
}

#[derive(Serialize)]
pub struct OutputVersion {
    pub id: String,
    /// The chosen candidate's spec.
    pub spec: Value,
    pub chosen_candidate: usize,
    pub candidates: Vec<crate::selection::CandidateTrial>,
    pub gain_span_db: f64,
    pub wav_file: String,
    pub wav_sha256: String,
    pub raw_wav_file: String,
    pub raw_wav_sha256: String,
    pub lufs: f64,
    pub drive_db: f64,
    pub render: Value,
    pub verification: delivery::Verification,
    pub waveform: waveform::Waveform,
    pub signature: Value,
    /// The bass of the held bands at matched loudness
    /// (`low_end::bass_level_db`).
    pub bass_level_db: Option<f64>,
    /// The change of the held bands in each section against the reference
    /// (`crate::adjust::HoldSections`); none without a reference.
    pub hold_sections: Option<crate::adjust::HoldSections>,
}

fn chosen_signature(
    rendered: &crate::treatment_render::Rendered,
    output: &crate::Metrics,
    output_stereo: &stereo::StereoAnalysis,
) -> crate::selection::Signature {
    crate::selection::Signature::new(
        output,
        output_stereo,
        rendered.lufs,
        rendered.limiter.mean_reduction_db,
    )
}

/// A version whose candidate ladder chose no candidate. The manifest gives
/// its trials and failed gates as the reason that the version is absent.
#[derive(Serialize)]
pub struct UnchosenVersion {
    pub id: String,
    pub candidates: Vec<crate::selection::CandidateTrial>,
    pub failed_gates: Vec<FailedGates>,
}

/// The failed checks of one trial of an unchosen version.
#[derive(Serialize)]
pub struct FailedGates {
    /// The planned candidate of the trial (`CandidateTrial::index`).
    pub candidate: usize,
    pub reason: String,
    /// `imaging::Check::id` of each failed check, in the order of the checks.
    pub gates: Vec<&'static str>,
}

impl UnchosenVersion {
    fn of(id: String, candidates: Vec<crate::selection::CandidateTrial>) -> Self {
        let failed_gates = candidates
            .iter()
            .map(|trial| FailedGates {
                candidate: trial.index,
                reason: trial.reason.clone(),
                gates: trial
                    .checks
                    .iter()
                    .filter(|check| !check.passed)
                    .map(|check| check.id)
                    .collect(),
            })
            .collect();
        Self {
            id,
            candidates,
            failed_gates,
        }
    }
}

#[derive(Serialize)]
pub struct FamilyResult {
    pub schema: &'static str,
    pub family_id: String,
    pub source_pcm_sha256: String,
    pub source_frames: usize,
    pub seconds: f64,
    pub versions: Vec<OutputVersion>,
    /// The versions that rendered candidates and chose none.
    pub unchosen: Vec<UnchosenVersion>,
    pub work: Work,
}

fn decode_alignment(raw: &Value) -> Result<Option<crate::stereo_phase::Alignment>> {
    Ok(if raw["phase_alignment"].is_null() {
        None
    } else {
        Some(serde_json::from_value(raw["phase_alignment"].clone())?)
    })
}

/// The spec of a worker plan or of a delivered version, from its JSON.
pub fn decode_spec(raw: &Value) -> Result<Spec> {
    let inactive = &raw["finishing"];
    if inactive["rule"] != "set.unchanged.v2"
        || inactive["validation_status"] != "provisional"
        || inactive["active"] != false
        || inactive["dc_filter_hz"] != Value::Null
        || !inactive["bands"].as_array().is_some_and(Vec::is_empty)
        || !inactive["section_gain"]
            .as_array()
            .is_some_and(Vec::is_empty)
        || inactive["color_amount"] != 0.0
        || inactive["clipping_budget_db"] != 0.0
    {
        return Err("Unsupported set processor in worker plan".into());
    }
    Ok(Spec {
        phase_alignment: decode_alignment(raw)?,
        finishing: finishing::Plan {
            rule: "set.unchanged.v2",
            validation_status: "provisional",
            active: false,
            dc_filter_hz: None,
            bands: Vec::new(),
            section_gain: Vec::new(),
            color_amount: 0.0,
            clipping_budget_db: 0.0,
        },
        tone: serde_json::from_value::<Tone>(raw["tone"].clone())?,
        focus_envelopes: serde_json::from_value(raw["focus_envelopes"].clone())?,
        side_highpass_hz: raw["side_highpass_hz"].as_f64(),
        side_gain: if raw["side_gain"].is_null() {
            None
        } else {
            Some(serde_json::from_value(raw["side_gain"].clone())?)
        },
        deesser: if raw["deesser"].is_null() {
            None
        } else {
            Some(serde_json::from_value(raw["deesser"].clone())?)
        },
        low_mid: serde_json::from_value(raw["low_mid"].clone())?,
        low_end_hold: serde_json::from_value(raw["low_end_hold"].clone())?,
        mono_safe: if raw["mono_safe"].is_null() {
            Vec::new()
        } else {
            serde_json::from_value(raw["mono_safe"].clone())?
        },
        dynamic_after_eq: serde_json::from_value(raw["dynamic_after_eq"].clone())?,
        compressor: if raw["compressor"].is_null() {
            None
        } else {
            let settings: crate::compressor::Settings =
                serde_json::from_value(raw["compressor"].clone())?;
            settings.validate()?;
            Some(settings)
        },
        limiter: LimiterSettings {
            release_ms: raw["limiter"]["release_ms"]
                .as_f64()
                .ok_or("Missing limiter release")?,
            pre_attack_ms: raw["limiter"]["pre_attack_ms"]
                .as_f64()
                .ok_or("Missing limiter pre-attack")?,
            hold_ms: raw["limiter"]["hold_ms"]
                .as_f64()
                .ok_or("Missing limiter hold")?,
            slow_release_ms: raw["limiter"]["slow_release_ms"]
                .as_f64()
                .ok_or("Missing limiter sustained release")?,
            attack_ms: raw["limiter"]["attack_ms"]
                .as_f64()
                .ok_or("Missing limiter attack")?,
        },
        clipper_db: raw["clipper_db"].as_f64().ok_or("Missing clipper budget")?,
        ceiling_dbtp: raw["ceiling_dbtp"].as_f64().ok_or("Missing ceiling")?,
        true_peak: serde_json::from_value(raw["true_peak"].clone())?,
        initial_drive_db: raw["initial_drive_db"]
            .as_f64()
            .ok_or("Missing initial drive")?,
        target_lufs: raw["target_lufs"].as_f64().ok_or("Missing target")?,
        fixed_drive_db: raw["fixed_drive_db"].as_f64(),
        restore_peak_tone: raw["restore_peak_tone"]
            .as_bool()
            .ok_or("Missing peak-tone restoration")?,
        restoration_db: if raw["restoration_db"].is_null() {
            [0.0; 6]
        } else {
            serde_json::from_value(raw["restoration_db"].clone())?
        },
    })
}

fn file_sha256(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut block = [0u8; 65_536];
    loop {
        let count = file.read(&mut block)?;
        if count == 0 {
            break;
        }
        hash.update(&block[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

/// Where a family render keeps its files.
pub trait Store {
    /// Write the delivered PCM24 file `name` of `audio`, at `rate`, with the
    /// dither `seed`, decode it and verify it against `ceiling_dbtp`.
    /// Returns its SHA-256 and the verification.
    fn deliver(
        &mut self,
        name: &str,
        audio: &[Stereo],
        rate: u32,
        seed: u64,
        ceiling_dbtp: f64,
    ) -> Result<(String, delivery::Verification)>;
    /// Write the unquantized render `name` of `audio` at `rate`. Returns its
    /// SHA-256.
    fn raw(&mut self, name: &str, audio: &[Stereo], rate: u32) -> Result<String>;
    /// Read the render `name` back; an error unless it is at `rate`.
    fn read(&mut self, name: &str, rate: u32) -> Result<Vec<Stereo>>;
    /// Scratch storage of the renderer.
    fn scratch(&self) -> crate::scratch::Factory {
        Box::new(crate::scratch::temporary)
    }
    /// The directory of the files, for the diagnostic files of
    /// `MASTER_PREFIX_WAV`.
    fn directory(&self) -> Option<&Path> {
        None
    }
    /// Give `record`, the reference record of the plan
    /// (`REFERENCE_SCHEMA`), to the families that compare with the
    /// reference. The Essential family makes it from its first render.
    fn publish_reference(&mut self, _record: &[u8]) -> Result<()> {
        Ok(())
    }
    /// The reference record of the plan from the Essential family, when the
    /// host passes it between families; the call can wait for it. With
    /// `None`, or with a record without a reference, the family renders the
    /// reference itself.
    fn reference(&mut self) -> Result<Option<Vec<u8>>> {
        Ok(None)
    }
}

/// The SHA-256 of a spec of a worker plan, from its JSON.
fn spec_sha256(spec: &Value) -> Result<String> {
    Ok(format!("{:x}", Sha256::digest(serde_json::to_vec(spec)?)))
}

/// The reference record of `reference`, the render of `spec` from the plan
/// of the source `source_pcm_sha256`; with no reference, a record that
/// tells the families to render it.
fn reference_record(
    source_pcm_sha256: &str,
    spec: &Value,
    reference: Option<(&crate::selection::Reference, &Spec)>,
) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(&serde_json::json!({
        "schema": REFERENCE_SCHEMA,
        "source_pcm_sha256": source_pcm_sha256,
        "spec_sha256": spec_sha256(spec)?,
        "reference": reference.map(|(r, _)| r.record()),
        // The spec that Essential chooses: the low-end hold, the sub lift
        // and the own point can change it. The fallback to the instant
        // attack renders it again (`selection::ladder`).
        "chosen_spec": reference.map(|(_, s)| serde_json::to_value(s)).transpose()?,
    }))?)
}

/// The reference of `record` when it is the reference render of `spec`
/// (its plan JSON `value`) for the source `source_pcm_sha256`.
fn shared_reference(
    record: &[u8],
    source_pcm_sha256: &str,
    value: &Value,
    spec: &Spec,
) -> Option<(crate::selection::Reference, Spec)> {
    let record: Value = serde_json::from_slice(record).ok()?;
    if record["schema"] != REFERENCE_SCHEMA
        || record["source_pcm_sha256"] != source_pcm_sha256
        || record["spec_sha256"].as_str() != spec_sha256(value).ok().as_deref()
        || record["reference"].is_null()
    {
        return None;
    }
    let chosen = match record.get("chosen_spec").filter(|c| !c.is_null()) {
        Some(c) => decode_spec(c).ok()?,
        None => spec.clone(),
    };
    let reference = crate::selection::Reference::from_record(
        &record["reference"],
        Some(chosen.effective_tone()),
    )
    .ok()?;
    Some((reference, chosen))
}

/// Files in a directory.
pub struct Directory<'a>(pub &'a Path);

impl Store for Directory<'_> {
    fn deliver(
        &mut self,
        name: &str,
        audio: &[Stereo],
        rate: u32,
        seed: u64,
        ceiling_dbtp: f64,
    ) -> Result<(String, delivery::Verification)> {
        let path = self.0.join(name);
        delivery::write(&path, audio, rate, delivery::Format::Pcm24, seed)?;
        let verification =
            delivery::verify(&path, audio, rate, delivery::Format::Pcm24, ceiling_dbtp)?;
        Ok((file_sha256(&path)?, verification))
    }

    fn raw(&mut self, name: &str, audio: &[Stereo], rate: u32) -> Result<String> {
        let path = self.0.join(name);
        delivery::write(&path, audio, rate, delivery::Format::Float32, 0)?;
        file_sha256(&path)
    }

    fn read(&mut self, name: &str, rate: u32) -> Result<Vec<Stereo>> {
        let source = crate::audio_io::read_source(&self.0.join(name))?;
        if source.sample_rate != rate {
            return Err("A render differs in sample rate".into());
        }
        Ok(source.audio)
    }

    fn directory(&self) -> Option<&Path> {
        Some(self.0)
    }
}

pub fn render_family(
    input: &[Stereo],
    rate: u32,
    plan_json: &[u8],
    family_id: &str,
    output_dir: &Path,
) -> Result<FamilyResult> {
    render_family_with_callback(input, rate, plan_json, family_id, output_dir, |_, _, _| {
        Ok(())
    })
}

/// Notify the host when a verified candidate is complete, before rendering
/// the next candidate. Only retained Essential-family candidates are suitable
/// for early listening; final labels still come from `finalize_set`.
pub fn render_family_with_callback(
    input: &[Stereo],
    rate: u32,
    plan_json: &[u8],
    family_id: &str,
    output_dir: &Path,
    mut completed: impl FnMut(&OutputVersion, &Path, bool) -> Result<()>,
) -> Result<FamilyResult> {
    render_family_in(
        input,
        rate,
        plan_json,
        family_id,
        &mut Directory(output_dir),
        |version, eligible| completed(version, output_dir, eligible),
    )
}

/// `render_family_with_callback` with the files in `store`. `input` is the
/// source at its render rate `rate`.
pub fn render_family_in(
    input: &[Stereo],
    rate: u32,
    plan_json: &[u8],
    family_id: &str,
    store: &mut impl Store,
    mut completed: impl FnMut(&OutputVersion, bool) -> Result<()>,
) -> Result<FamilyResult> {
    // The browser engine has no clock (`wasm32-unknown-unknown`).
    #[cfg(not(target_arch = "wasm32"))]
    let started = Some(Instant::now());
    #[cfg(target_arch = "wasm32")]
    let started: Option<Instant> = None;
    let plan: InputPlan = serde_json::from_slice(plan_json)?;
    if plan.schema != PLAN_SCHEMA {
        return Err("Unsupported worker plan schema".into());
    }
    if plan.sample_rate != rate {
        return Err("Worker plan belongs to another render rate".into());
    }
    plan.facts.validate()?;
    if input.len() != plan.source_frames
        || stem_evidence::fingerprint(input) != plan.source_pcm_sha256
    {
        return Err("Worker plan belongs to different decoded source PCM".into());
    }
    // The spec that the other families compare with: the Essential spec.
    let shared_spec = plan.families.iter().find_map(|f| f.reference.clone());
    // The Essential version, whose chosen render is the reference render:
    // its candidates, its bass shelves and whether it renders a sub lift.
    let essential = plan
        .families
        .first()
        .and_then(|f| f.versions.first())
        .map(|v| {
            let candidates: Vec<(String, Value, crate::selection::Gates)> = v
                .candidates
                .iter()
                .map(|c| (c.reason.clone(), c.spec.clone(), c.gates.clone()))
                .collect();
            (candidates, v.bass_shelves.clone(), !v.lifted)
        });
    let family = plan
        .families
        .into_iter()
        .find(|f| f.id == family_id)
        .ok_or("Unknown worker family")?;
    if family.versions.is_empty() || family.versions.len() > 3 {
        return Err("Worker family requires one to three versions".into());
    }
    let source = pipeline::analyze_source(input, rate)?;
    // The top of the band of the source: its codec cut, when it has one.
    let plan_top_hz = plan.source_condition["bandwidth_hz"]
        .as_f64()
        .unwrap_or(rate as f64 / 2.0);
    let plan_capture = crate::source_condition::reported_device_capture(&plan.source_condition);
    let mut renderer = Renderer::with_scratch(input, rate, store.scratch());
    // As in `set::render`: a family's searches do not revisit a drive.
    renderer.work.probe_cache_budget_bytes = 0;
    let reference_spec = family.reference.as_ref().map(decode_spec).transpose()?;
    // The reference render of the Essential family serves the other
    // families when the host passes it; otherwise the family renders it.
    // The spec of the reference is the spec that Essential chooses, as in
    // `set::render`.
    let (reference, reference_spec) = match (&family.reference, &reference_spec) {
        (Some(value), Some(spec)) => {
            let (reference, chosen) = match store
                .reference()?
                .and_then(|record| shared_reference(&record, &plan.source_pcm_sha256, value, spec))
            {
                Some(shared) => shared,
                None => {
                    // The reference is the render that the Essential version
                    // chooses, as in `set::render`: the low-end hold, the sub
                    // lift and the own point can change it.
                    let essential = essential.as_ref().filter(|(candidates, _, _)| {
                        candidates
                            .first()
                            .is_some_and(|(_, first, _)| first == value)
                    });
                    let chosen = match essential {
                        Some((planned, bass_shelves, lift)) => {
                            let candidates = planned
                                .iter()
                                .map(|(reason, spec, gates)| {
                                    Ok(crate::selection::Candidate {
                                        reason: reason.clone(),
                                        spec: decode_spec(spec)?,
                                        gates: gates.clone(),
                                    })
                                })
                                .collect::<Result<Vec<_>>>()?;
                            set::base_ladder(
                                &mut renderer,
                                input,
                                &source,
                                None,
                                None,
                                &candidates,
                                bass_shelves,
                                *lift,
                                |_, _| Ok(()),
                            )?
                            .0
                            .map(|c| (c.measured, c.spec))
                        }
                        None => None,
                    };
                    let (measured, chosen) = match chosen {
                        Some(chosen) => chosen,
                        None => {
                            let mut measured =
                                crate::selection::measure(input, &source, renderer.render(spec)?)?;
                            measured.tone = Some(spec.effective_tone());
                            (measured, spec.clone())
                        }
                    };
                    // The gates compare the reference's measurements, not its
                    // audio.
                    (crate::selection::Reference::of(&measured), chosen)
                }
            };
            (Some(reference), Some(chosen))
        }
        _ => (None, reference_spec),
    };
    // The Essential family gives its first render to the other families as
    // their reference, once.
    let mut publish = family.reference.is_none() && shared_spec.is_some();
    let mut outputs = Vec::new();
    let mut unchosen = Vec::new();
    let mut base_spec: Option<(Spec, crate::selection::Measured)> = None;
    let mut base_candidate: Option<usize> = None;
    for version in family.versions {
        if !valid_version_id(&version.id) {
            return Err("Invalid worker version ID".into());
        }
        let mut planned_candidates: Vec<crate::selection::Candidate> = Vec::new();
        let candidates: Vec<crate::selection::Candidate> = match version.role {
            set::Role::Dense => {
                let template: set::DenseTemplate = serde_json::from_value(
                    version
                        .dense_template
                        .clone()
                        .ok_or("Dense version without template")?,
                )?;
                match &base_spec {
                    Some((base, measured)) => set::dense_candidates(
                        base,
                        measured.rendered.drive_db,
                        measured.rendered.lufs,
                        measured.rendered.limiter.maximum_reduction_db,
                        &template,
                    ),
                    None => continue,
                }
            }
            set::Role::Bold => {
                let base = match (&base_spec, &version.speculative_base) {
                    // The speculative Bold covers the candidate it scales.
                    (Some(_), _) if version.unless_base_candidate == base_candidate => continue,
                    (Some((base, _)), _) => base.clone(),
                    (None, Some(speculative)) => decode_spec(&speculative.spec)?,
                    (None, None) => continue,
                };
                let candidates = set::bold_candidates(
                    &base,
                    version.side_gain,
                    version.compressor,
                    version.target_lufs,
                    &source.spectrum,
                    plan_top_hz,
                    plan_capture,
                );
                if candidates.is_empty() {
                    continue;
                }
                candidates
            }
            set::Role::Base => {
                planned_candidates = version
                    .candidates
                    .iter()
                    .map(|c| {
                        Ok(crate::selection::Candidate {
                            reason: c.reason.clone(),
                            spec: decode_spec(&c.spec)?,
                            gates: c.gates.clone(),
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                // A bass version renders the bass candidates and then, when
                // none passes, the planned candidates (`set::base_ladder`);
                // a Bold version keeps the planned candidate as its base.
                planned_candidates.clone()
            }
        };
        if candidates.is_empty() {
            continue;
        }
        let base_reference = base_spec
            .as_ref()
            .filter(|_| version.role == set::Role::Dense)
            .map(|(_, m)| crate::selection::Reference::of(m));
        let laddered = match version.role {
            set::Role::Base => set::base_ladder(
                &mut renderer,
                input,
                &source,
                reference.as_ref(),
                reference_spec.as_ref(),
                &candidates,
                &version.bass_shelves,
                !version.lifted,
                |_, _| Ok(()),
            ),
            set::Role::Dense => crate::selection::ladder(
                &mut renderer,
                input,
                &source,
                base_reference.as_ref(),
                None,
                &candidates,
                |_, _| Ok(()),
            ),
            set::Role::Bold => crate::selection::ladder(
                &mut renderer,
                input,
                &source,
                reference.as_ref(),
                reference_spec.as_ref(),
                &candidates,
                |_, _| Ok(()),
            ),
        };
        if publish && version.role == set::Role::Base {
            // The chosen render of the Essential version is the reference
            // render of the other families when its first candidate is
            // their reference spec.
            publish = false;
            let first = laddered.as_ref().ok().and_then(|(chosen, _)| {
                chosen
                    .as_ref()
                    .filter(|_| version.candidates.first().map(|c| &c.spec) == shared_spec.as_ref())
            });
            let shared = first.map(|c| (crate::selection::Reference::of(&c.measured), &c.spec));
            if let Some(spec) = &shared_spec {
                store.publish_reference(&reference_record(
                    &plan.source_pcm_sha256,
                    spec,
                    shared.as_ref().map(|(r, s)| (r, *s)),
                )?)?;
            }
        }
        let (chosen, candidate_trials) = laddered?;
        let Some(chosen) = chosen else {
            unchosen.push(UnchosenVersion::of(version.id, candidate_trials));
            continue;
        };
        let spec = chosen.spec;
        let chosen_measured = chosen.measured;
        let bass_level_db = crate::low_end::bass_level_db(
            &chosen_measured.low_sections,
            chosen_measured.rendered.lufs,
        );
        let hold_sections = reference.as_ref().map(|r| {
            crate::adjust::HoldSections::of(
                &spec,
                &source.spectrum,
                // The chosen trial is the last of its index: the ladder of
                // the instant attack and the plain ladder of a bass version
                // repeat the indices.
                candidate_trials
                    .iter()
                    .rev()
                    .find(|t| t.index == chosen.index)
                    .map_or(&[][..], |t| &t.low_end),
                &chosen_measured.low_sections,
                chosen_measured.rendered.lufs,
                &r.low_sections,
                r.lufs,
            )
        });
        let crate::selection::Measured {
            rendered,
            output: chosen_output,
            output_stereo: chosen_stereo,
            gain_span_db,
            ..
        } = chosen_measured.clone();
        if rendered.audio.len() != input.len()
            || rendered.peak_estimate_dbtp > spec.ceiling_dbtp + 1e-6
        {
            return Err("Worker render length or peak failed".into());
        }
        if version.role == set::Role::Base {
            // Later versions use the base's measurements, not its audio.
            let mut base = chosen_measured.clone();
            base.rendered.audio = Default::default();
            let base_tone = if !version.bass_shelves.is_empty() {
                planned_candidates[chosen.index].spec.clone()
            } else {
                spec.clone()
            };
            base_spec = Some((base_tone, base));
            base_candidate = Some(chosen.index);
        }
        let id = version.id;
        // A speculative Bold and the base family's own Bold may both
        // render; their files have different names.
        let stem = if version.speculative_base.is_some() {
            format!("{id}-speculative")
        } else {
            id.clone()
        };
        let wav_file = format!("{stem}.wav");
        let (wav_sha256, verification) = store.deliver(
            &wav_file,
            &rendered.audio,
            rate,
            crate::set::delivery_seed(&plan.source_pcm_sha256, &id),
            spec.ceiling_dbtp,
        )?;
        // Final cross-family comparisons use the unquantized render, while
        // references are the already delivered PCM24 files, exactly as set::render.
        let raw_wav_file = format!("{stem}.raw.wav");
        let raw_wav_sha256 = store.raw(&raw_wav_file, &rendered.audio, rate)?;
        // With `MASTER_PREFIX_WAV` set, the store keeps the prefix of the
        // delivered render, so that a diagnostic of peak control can measure
        // the render against it.
        if let Some(output_dir) = store
            .directory()
            .filter(|_| std::env::var_os("MASTER_PREFIX_WAV").is_some())
        {
            let mut delivered = spec.clone();
            if let Some(restoration) = &rendered.restoration {
                delivered.restoration_db = restoration.gains_db;
            }
            delivery::write(
                &output_dir.join(format!("{stem}.prefix.wav")),
                &renderer.prefix(&delivered)?,
                rate,
                delivery::Format::Float32,
                0,
            )?;
            // The signal before the compressor, for a diagnostic of the
            // compressor.
            delivered.compressor = None;
            delivery::write(
                &output_dir.join(format!("{stem}.precomp.wav")),
                &renderer.prefix(&delivered)?,
                rate,
                delivery::Format::Float32,
                0,
            )?;
            // The same signal without the de-esser, for a diagnostic of the
            // de-esser.
            if delivered.deesser.take().is_some() {
                delivery::write(
                    &output_dir.join(format!("{stem}.precomp-undeessed.wav")),
                    &renderer.prefix(&delivered)?,
                    rate,
                    delivery::Format::Float32,
                    0,
                )?;
            }
        }
        // A fallback version's fate depends on another family, which the
        // worker cannot see; the finalizer decides it.
        let preview_eligible = if version.fallback_for.is_some() {
            false
        } else if family.id == "essential" && id == "essential" {
            // A Light that matches the mix is not delivered (D140); the finalizer
            // would remove a preview of it.
            set::candidate_status(&version.rule, false) == "retained"
                && !set::same_as_mix(
                    source.mix(),
                    &chosen_signature(&rendered, &chosen_output, &chosen_stereo),
                )
        } else if family.id == "essential" && id == "dense-essential" {
            let essential = outputs
                .iter()
                .find(|o: &&OutputVersion| o.id == "essential");
            let duplicate = essential.is_some_and(|e: &OutputVersion| {
                serde_json::from_value::<crate::selection::Signature>(e.signature.clone())
                    .map(|s| {
                        chosen_signature(&rendered, &chosen_output, &chosen_stereo)
                            .is_duplicate_of(&s)
                    })
                    .unwrap_or(false)
            });
            set::candidate_status(&version.rule, duplicate) == "retained"
        } else {
            false
        };
        let signature = chosen_signature(&rendered, &chosen_output, &chosen_stereo);
        let output = OutputVersion {
            id,
            spec: serde_json::to_value(&spec)?,
            chosen_candidate: chosen.index,
            candidates: candidate_trials,
            gain_span_db,
            wav_file,
            wav_sha256,
            raw_wav_file,
            raw_wav_sha256,
            lufs: rendered.lufs,
            drive_db: rendered.drive_db,
            render: serde_json::to_value(&rendered)?,
            verification,
            waveform: waveform::analyze(&rendered.audio, rate),
            signature: serde_json::to_value(&signature)?,
            bass_level_db,
            hold_sections,
        };
        completed(&output, preview_eligible)?;
        outputs.push(output);
    }
    // The other families render the reference themselves when the Essential
    // family rendered no first version.
    if let (true, Some(spec)) = (publish, &shared_spec) {
        store.publish_reference(&reference_record(&plan.source_pcm_sha256, spec, None)?)?;
    }
    Ok(FamilyResult {
        schema: FAMILY_SCHEMA,
        family_id: family.id,
        source_pcm_sha256: plan.source_pcm_sha256,
        source_frames: input.len(),
        seconds: started.map_or(0.0, |s| s.elapsed().as_secs_f64()),
        versions: outputs,
        unchosen,
        work: renderer.work,
    })
}

/// The record of version `id` in the `unchosen` versions of a family result.
fn unchosen_version<'r>(result: &'r Value, id: &str) -> Option<&'r Value> {
    result["unchosen"]
        .as_array()?
        .iter()
        .find(|version| version["id"] == id)
}

/// The trial of a planned version that the set does not deliver. A version
/// whose ladder chose no candidate (`unchosen`) keeps the trials and the
/// failed gates of that ladder.
fn undelivered(planned: &InputVersion, status: &str, unchosen: Option<&Value>) -> Value {
    let mut trial = serde_json::json!({"id":planned.id,"label":planned.label,"rule":planned.rule,
        "basis":planned.basis,"status":status,"duplicate_of":Value::Null,"comparisons":[]});
    if let Some(version) = unchosen {
        trial["candidates"] = version["candidates"].clone();
        trial["failed_gates"] = version["failed_gates"].clone();
    }
    trial
}

/// Complete a parallel set in the same plan order and with the same measured
/// naming and gain-matched duplicate rules as the normal set renderer.
/// `objects` contains the verified family WAVs and their unquantized renders.
/// `input` is the source at its render rate `rate`.
pub fn finalize_set(
    input: &[Stereo],
    rate: u32,
    plan_json: &[u8],
    family_results: &[Value],
    objects: &Path,
) -> Result<Value> {
    let plan: InputPlan = serde_json::from_slice(plan_json)?;
    if plan.schema != PLAN_SCHEMA
        || plan.source_frames != input.len()
        || plan.source_pcm_sha256 != stem_evidence::fingerprint(input)
        || family_results.len() != plan.families.len()
    {
        return Err("Finalizer plan, source, or family count differs".into());
    }
    // The finalizer reads the metrics and the stereo image of the source.
    let (mix_input, mix_stereo) = pipeline::measure_mix(input, rate)?;
    let source = pipeline::Mix {
        input: &mix_input,
        stereo: &mix_stereo,
    };
    let mut accepted: Vec<(String, std::path::PathBuf, crate::selection::Signature)> = Vec::new();
    let mut trials = Vec::new();
    let mut versions = Vec::new();
    let mut sounds = Vec::new();
    let mut essential: Option<(crate::Metrics, stereo::StereoAnalysis, f64, f64)> = None;
    // Versions the workers rendered; a fallback stands in for one of them
    // only when it is absent. Duplicate screening happens after this point,
    // so a rendered version counts as delivered here.
    let delivered: Vec<String> = family_results
        .iter()
        .filter_map(|r| r["versions"].as_array())
        .flatten()
        .filter_map(|c| c["id"].as_str().map(String::from))
        .collect();
    for (family, result) in plan.families.iter().zip(family_results) {
        let candidates = result["versions"]
            .as_array()
            .ok_or("Missing family versions")?;
        let unchosen = result["unchosen"]
            .as_array()
            .ok_or("Missing unchosen family versions")?;
        if result["schema"] != FAMILY_SCHEMA
            || result["family_id"] != family.id
            || result["source_pcm_sha256"] != plan.source_pcm_sha256
            || result["source_frames"] != plan.source_frames
            || candidates.len() + unchosen.len() > family.versions.len()
        {
            return Err("Family marker differs from the source-bound plan".into());
        }
        for planned in &family.versions {
            let id = &planned.id;
            let mut basis = planned.basis.clone();
            let rendered_by = |family_id: &Value| {
                family_results.iter().any(|r| {
                    r["family_id"] == *family_id
                        && r["versions"]
                            .as_array()
                            .is_some_and(|v| v.iter().any(|c| c["id"] == *id))
                })
            };
            // A Bold after its base renders only for a base candidate that
            // the speculative Bold does not cover; when absent, the
            // speculative Bold stands for it.
            if planned.unless_base_candidate.is_some() && !rendered_by(&result["family_id"]) {
                continue;
            }
            if let Some(speculative) = &planned.speculative_base {
                let other_family = family_results
                    .iter()
                    .filter(|r| r["family_id"] != result["family_id"])
                    .any(|r| {
                        r["versions"]
                            .as_array()
                            .is_some_and(|v| v.iter().any(|c| c["id"] == *id))
                    });
                if other_family {
                    continue;
                }
                let base_chosen = family_results
                    .iter()
                    .filter_map(|r| r["versions"].as_array())
                    .flatten()
                    .find(|c| c["id"] == speculative.version.as_str())
                    .and_then(|c| c["chosen_candidate"].as_u64());
                // The speculative Bold stands when the base chose the
                // candidate that it steps, with or without the bass shelves,
                // or chose none (`set.bass-version.v4`).
                match base_chosen {
                    Some(chosen) if chosen != speculative.candidate as u64 => {
                        // The base family rendered its own Bold for that
                        // candidate; its ladder gives the reason.
                        let own = family_results
                            .iter()
                            .filter(|r| r["family_id"] != result["family_id"])
                            .find_map(|r| unchosen_version(r, id));
                        trials.push(undelivered(planned, "no-opportunity", own));
                        continue;
                    }
                    Some(_) => {}
                    None => basis = set::unchosen_base_basis(&basis),
                }
            }
            if planned
                .fallback_for
                .as_deref()
                .is_some_and(|dependency| delivered.iter().any(|d| d == dependency))
            {
                trials.push(undelivered(planned, "not-needed", None));
                continue;
            }
            let Some(candidate) = candidates.iter().find(|c| c["id"] == *id) else {
                trials.push(undelivered(
                    planned,
                    "no-opportunity",
                    unchosen_version(result, id),
                ));
                continue;
            };
            let stem = if planned.speculative_base.is_some() {
                format!("{id}-speculative")
            } else {
                id.clone()
            };
            let wav_file = format!("{stem}.wav");
            let raw_file = format!("{stem}.raw.wav");
            if candidate["wav_file"] != wav_file || candidate["raw_wav_file"] != raw_file {
                return Err("Family candidate differs from the plan".into());
            }
            let delivered_path = objects.join(&wav_file);
            let raw_path = objects.join(&raw_file);
            if candidate["wav_sha256"].as_str() != Some(file_sha256(&delivered_path)?.as_str())
                || candidate["raw_wav_sha256"].as_str() != Some(file_sha256(&raw_path)?.as_str())
            {
                return Err("Family candidate hash differs from stored audio".into());
            }
            // The delivered WAV gives only the waveform; it is freed before
            // the raw render and the references are read.
            let delivered_waveform = {
                let delivered = crate::audio_io::read_source(&delivered_path)?;
                if delivered.sample_rate != rate || delivered.audio.len() != input.len() {
                    return Err("Family candidate duration or rate differs".into());
                }
                waveform::analyze(&delivered.audio, rate)
            };
            let rendered = crate::audio_io::read_source(&raw_path)?;
            if rendered.sample_rate != rate || rendered.audio.len() != input.len() {
                return Err("Family candidate duration or rate differs".into());
            }
            let output = crate::metrics(&rendered.audio, rate);
            let output_stereo = stereo::analyze(&rendered.audio, rate)?;
            let measures = crate::adjust::measure(&rendered.audio, rate);
            let lufs = candidate["render"]["lufs"]
                .as_f64()
                .ok_or("Missing rendered level")?;
            let mean_reduction_db = candidate["render"]["limiter"]["mean_reduction_db"]
                .as_f64()
                .ok_or("Missing rendered peak compression")?;
            let signature =
                crate::selection::Signature::new(&output, &output_stereo, lufs, mean_reduction_db);
            let view = set::ListeningMeasurement {
                output: &output,
                output_stereo: &output_stereo,
                lufs,
                mean_reduction_db,
            };
            let base =
                essential
                    .as_ref()
                    .map(|(output, output_stereo, lufs, mean_reduction_db)| {
                        set::ListeningMeasurement {
                            output,
                            output_stereo,
                            lufs: *lufs,
                            mean_reduction_db: *mean_reduction_db,
                        }
                    });
            let spec = decode_spec(&candidate["spec"])?;
            let (label, basis) = if planned.role == set::Role::Base {
                set::delivered_base(
                    &planned.label,
                    &basis,
                    !planned.bass_shelves.is_empty(),
                    &spec,
                )
            } else {
                (planned.label.clone(), basis)
            };
            let compared_with = if accepted.iter().any(|(id, ..)| id == "essential") {
                set::ESSENTIAL_LABEL
            } else {
                set::MIX_NAME
            };
            let mut description = set::describe_measured(
                source,
                view,
                base,
                compared_with,
                !spec.mono_safe.is_empty(),
            );
            let measured_name =
                (*id != "essential").then(|| set::measured_label(&spec, view, base));
            if !spec.focus_envelopes.is_empty() {
                description.push_str(
                    " Tonal intensity follows measured local passages and eases back elsewhere.",
                );
            }
            let comparisons = accepted.iter().map(|(reference_id, path, reference_signature)| {
                let reference = crate::audio_io::read_source(path)?;
                Ok(serde_json::json!({"id":reference_id,
                    "matched_residual_db":set::matched_residual_db(&reference.audio, &rendered.audio)?,
                    "described_duplicate":signature.is_duplicate_of(reference_signature)}))
            }).collect::<Result<Vec<Value>>>()?;
            let duplicate_of = if set::same_as_mix(source, &signature) {
                Some(set::ORIGINAL_ID)
            } else {
                comparisons
                    .iter()
                    .find(|c| c["described_duplicate"] == true)
                    .and_then(|c| c["id"].as_str())
            };
            let status = set::candidate_status(&planned.rule, duplicate_of.is_some());
            trials.push(
                serde_json::json!({"id":id,"label":label,"description":description,
                "measured_name":measured_name,
                "rule":planned.rule,"basis":basis,
                "status":status,"chosen_candidate":candidate["chosen_candidate"],
                "candidates":candidate["candidates"],
                "duplicate_of":duplicate_of,"comparisons":comparisons}),
            );
            if status == "retained" {
                versions.push(serde_json::json!({
                    "schema":"mastering.web-set-version.1","id":id,"label":label,
                    "description":description,"rule":planned.rule,"basis":basis,
                    "source_pcm_sha256":plan.source_pcm_sha256,
                    "output_sha256":candidate["wav_sha256"],"delivery":candidate["verification"],
                    "spec":candidate["spec"],"report":{"render":candidate["render"],
                        "output":output},
                    "waveform":delivered_waveform,
                    "wav_file":wav_file,
                    "signature":signature,
                    "measures":measures,
                    "hold_sections":candidate["hold_sections"],
                }));
                accepted.push((id.clone(), delivered_path, signature));
                sounds.push(crate::version_names::sound(
                    id,
                    source,
                    view,
                    crate::version_names::designed(id, &spec),
                    candidate["bass_level_db"].as_f64(),
                ));
            }
            if essential.is_none() {
                essential = Some((output, output_stereo, lufs, mean_reduction_db));
            }
        }
    }
    // The names describe the delivered versions together.
    let named = crate::version_names::name(
        &sounds,
        versions.first().is_some_and(|v| v["id"] == "essential"),
    );
    for n in &named {
        let naming = serde_json::to_value(n)?;
        for record in versions
            .iter_mut()
            .chain(trials.iter_mut().filter(|t| t["status"] == "retained"))
        {
            if record["id"] == n.id.as_str() {
                record["detail"] = record["description"].clone();
                record["label"] = n.name.into();
                record["description"] = n.description.clone().into();
                record["naming"] = naming.clone();
                if !n.distinct {
                    record["status"] = "not-distinct".into();
                }
            }
        }
    }
    // A version that no measured step names is not delivered.
    versions.retain(|v| v["status"] != "not-distinct");
    for v in &mut versions {
        if let Some(fields) = v.as_object_mut() {
            fields.remove("status");
        }
    }
    let adjust = crate::adjust::Base::of_plan(&serde_json::from_slice(plan_json)?)?;
    Ok(
        serde_json::json!({"schema":MANIFEST_SCHEMA,"sample_rate":plan.sample_rate,
        "source_pcm_sha256":plan.source_pcm_sha256,"source_frames":plan.source_frames,
        "source_condition":plan.source_condition,"adjust":adjust,
        "versions":versions,"trials":trials,"duplicate_threshold_db":set::DUPLICATE_DB}),
    )
}
