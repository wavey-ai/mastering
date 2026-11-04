use mastering_engine::{audio_io::Stereo, set, stem_evidence};

#[test]
fn set_analysis_derives_focus_from_aligned_stems_without_external_json() {
    let rate = 48000u32;
    let mut stems: [Vec<Stereo>; 4] =
        std::array::from_fn(|_| Vec::with_capacity(rate as usize * 12));
    for i in 0..rate as usize * 12 {
        let wave = (std::f64::consts::TAU * 700.0 * i as f64 / rate as f64).sin() as f32;
        stems[0].push([0.0, 0.0]);
        stems[1].push([0.0, 0.0]);
        stems[2].push([0.04 * wave, -0.04 * wave]);
        stems[3].push([0.02 * wave, 0.02 * wave]);
    }
    let mut source: Vec<_> = stems[2]
        .iter()
        .zip(&stems[3])
        .map(|(a, b)| [a[0] + b[0], a[1] + b[1]])
        .collect();
    let evidence = stem_evidence::analyze_set(
        &mut source,
        48000,
        |role, each| {
            let i = ["drums", "bass", "other", "vocals"]
                .iter()
                .position(|x| *x == role)
                .unwrap();
            for block in std::mem::take(&mut stems[i]).chunks(65_536) {
                each(rate, block)?;
            }
            Ok(())
        },
        None,
        &"0".repeat(64),
    )
    .unwrap();
    assert!(evidence.usable);
    assert!(evidence
        .auto_focus
        .as_ref()
        .unwrap()
        .proposals
        .iter()
        .any(|proposal| proposal.id.starts_with("focus-mid-")));
    assert!(evidence.refinement.is_some());
}

fn material(treble_hz: f64) -> (Vec<Stereo>, [Vec<Stereo>; 4]) {
    let stems = std::array::from_fn(|role| {
        (0..6 * 48000)
            .map(|i| {
                let t = i as f64 / 48000.0;
                let burst = if i % 24000 < 1200 { 1.0 } else { 0.0 };
                let sine = |hz: f64| (std::f64::consts::TAU * hz * t).sin();
                let x = match role {
                    0 => 0.025 * sine(170.0) * burst,
                    1 => 0.025 * sine(75.0),
                    2 => 0.015 * sine(1700.0),
                    _ => 0.03 * sine(700.0) + 0.14 * sine(treble_hz) * burst,
                } as f32;
                [x, x]
            })
            .collect::<Vec<_>>()
    });
    let mix = (0..stems[0].len())
        .map(|i| [0, 1].map(|c| stems.iter().map(|s| s[i][c]).sum()))
        .collect();
    (mix, stems)
}

#[test]
fn sequential_stems_match_retained_stems_and_reconstruction_exactly() {
    use mastering_engine::{refinement, resample};
    for rate in [44100, 48000] {
        // The blocks of a decoder: of one frame count, of one frame count
        // with a remainder, and the whole stem in one block.
        for (silent, block) in [(false, 4_096), (false, 7_001), (true, usize::MAX)] {
            let (_, mut stems) = material(9000.0);
            for stem in &mut stems {
                stem.truncate(3 * rate as usize);
                if silent {
                    stem.fill([-0.0; 2]);
                }
            }
            let sum: Vec<_> = (0..stems[0].len())
                .map(|i| [0, 1].map(|c| stems.iter().map(|s| s[i][c]).sum()))
                .collect();
            let source = resample::convert(&sum, rate, 48000).unwrap();
            let (mut expected, reconstructed) = stem_evidence::analyze_with_reconstruction(
                &source,
                stems.each_ref().map(Vec::as_slice),
                rate,
                48000,
            )
            .unwrap();
            expected.refinement = Some(
                refinement::analyze_with_reconstruction(
                    &source,
                    stems.each_ref().map(Vec::as_slice),
                    rate,
                    &expected,
                    None,
                    &reconstructed,
                    48000,
                )
                .unwrap(),
            );
            let mut roles = Vec::new();
            let actual = stem_evidence::analyze_sequential(
                &source,
                48000,
                |role, each| {
                    let index = ["drums", "bass", "other", "vocals"]
                        .iter()
                        .position(|x| *x == role)
                        .unwrap();
                    roles.push(role);
                    for frames in stems[index].chunks(block) {
                        each(rate, frames)?;
                    }
                    Ok(())
                },
                None,
                true,
            )
            .unwrap();
            assert_eq!(roles, ["drums", "bass", "other", "vocals"]);
            assert_eq!(
                serde_json::to_value(actual).unwrap(),
                serde_json::to_value(expected).unwrap()
            );
        }
    }
}

#[test]
fn measured_vocal_treble_changes_the_proposed_frequency_without_remixing() {
    for hz in [6200.0, 9000.0] {
        let (source, stems) = material(hz);
        let original = source.clone();
        let evidence =
            stem_evidence::analyze(&source, stems.each_ref().map(Vec::as_slice), 48000, 48000)
                .unwrap();
        assert!(evidence.usable);
        assert!(evidence.sum_residual_relative_db < -100.0);
        let index = evidence.vocal_transient_band.unwrap();
        assert!(evidence.bands[index].low_hz <= hz && evidence.bands[index].high_hz > hz);
        assert_eq!(source, original);
    }
}

#[test]
fn changed_gains_or_alignment_cannot_supply_usable_stem_evidence() {
    let (source, mut stems) = material(9000.0);
    for frame in &mut stems[3] {
        for x in frame {
            *x *= 2.0;
        }
    }
    let changed =
        stem_evidence::analyze(&source, stems.each_ref().map(Vec::as_slice), 48000, 48000).unwrap();
    assert!(!changed.usable);
    assert_eq!(changed.vocal_transient_band, None);
    let (source, mut stems) = material(9000.0);
    for stem in &mut stems {
        stem.rotate_right(240);
    }
    let shifted =
        stem_evidence::analyze(&source, stems.each_ref().map(Vec::as_slice), 48000, 48000).unwrap();
    assert!(!shifted.usable);
}

#[test]
fn bad_geometry_and_evidence_from_another_mix_are_rejected() {
    let (source, mut stems) = material(9000.0);
    let evidence =
        stem_evidence::analyze(&source, stems.each_ref().map(Vec::as_slice), 48000, 48000).unwrap();
    let mut other = source.clone();
    other[100][0] *= 0.9;
    assert!(set::prepare(&other, 48000, Some(evidence), None).is_err());
    stems[0].pop();
    assert!(
        stem_evidence::analyze(&source, stems.each_ref().map(Vec::as_slice), 48000, 48000).is_err()
    );
    stems[0].push([f32::NAN; 2]);
    assert!(
        stem_evidence::analyze(&source, stems.each_ref().map(Vec::as_slice), 48000, 48000).is_err()
    );
}

#[test]
fn drums_are_not_mislabeled_as_vocal_treble() {
    let (source, mut stems) = material(9000.0);
    stems.swap(0, 3);
    let evidence =
        stem_evidence::analyze(&source, stems.each_ref().map(Vec::as_slice), 48000, 48000).unwrap();
    assert_eq!(evidence.vocal_transient_band, None);
}

#[test]
fn drum_timing_and_bass_period_refine_release_even_without_vocals() {
    let mut releases = Vec::new();
    for bass_hz in [50.0, 100.0] {
        let stems: [Vec<Stereo>; 4] = std::array::from_fn(|role| {
            (0..6 * 48000)
                .map(|i| {
                    let t = i as f64 / 48000.0;
                    let drum_phase = (i % 9600) as f64 / 48000.0;
                    let bass_phase = (i % 48000) as f64 / 48000.0;
                    let x = match role {
                        0 => {
                            0.12 * (-drum_phase / 0.008).exp()
                                * (std::f64::consts::TAU * 1500.0 * t).sin()
                        }
                        1 => {
                            (0.02 + 0.15 * (-bass_phase / 0.2).exp())
                                * (std::f64::consts::TAU * bass_hz * t).sin()
                        }
                        2 => 0.015 * (std::f64::consts::TAU * 1700.0 * t).sin(),
                        _ => 0.0,
                    } as f32;
                    [x, x * 0.8]
                })
                .collect()
        });
        let input: Vec<Stereo> = (0..stems[0].len())
            .map(|i| [0, 1].map(|c| stems.iter().map(|s| s[i][c]).sum()))
            .collect();
        let evidence =
            stem_evidence::analyze(&input, stems.each_ref().map(Vec::as_slice), 48000, 48000)
                .unwrap();
        assert!(evidence.usable);
        assert!(evidence.vocal_active_seconds < 0.1);
        assert!(evidence.drum_decay_events >= 16);
        let release = evidence.limiter_release_ms.unwrap();
        assert!(release >= 2000.0 / evidence.bass_body_hz.unwrap());
        releases.push(release);
    }
    assert!(releases[0] > releases[1]);
}

#[test]
fn a_plan_from_stored_evidence_is_the_plan_from_the_evidence() {
    let (mut source, stems) = material(9000.0);
    let original = source.clone();
    let evidence = stem_evidence::analyze_set(
        &mut source,
        48000,
        |role, each| {
            let i = ["drums", "bass", "other", "vocals"]
                .iter()
                .position(|x| *x == role)
                .unwrap();
            for block in stems[i].chunks(65_536) {
                each(48000, block)?;
            }
            Ok(())
        },
        None,
        &"0".repeat(64),
    )
    .unwrap();
    assert!(evidence.auto_focus.is_some() && evidence.refinement.is_some());
    let bytes = evidence.stored().unwrap().unwrap();
    let read = stem_evidence::Evidence::from_stored(&bytes).unwrap();
    assert!(read == evidence);
    let plan = |evidence| {
        serde_json::to_vec(
            &set::prepare(&original, 48000, Some(evidence), None)
                .unwrap()
                .worker_plan()
                .unwrap(),
        )
        .unwrap()
    };
    assert_eq!(plan(read), plan(evidence.clone()));
    // The stored evidence keeps the masked depth of each frame of the
    // low-mid key, which the evidence of a plan record leaves out.
    let mut low_mid = evidence.clone();
    low_mid.low_mid = Some(mastering_engine::low_mid::Key {
        rule: "set.low-mid.v1",
        lead_role: 3,
        active_frames: 3,
        keyed_frames: 2,
        excess_db: vec![(315.0, 2.5)],
        centre_hz: 330.0,
        depth_db: vec![0.0, 1.25, 0.1],
    });
    let bytes = low_mid.stored().unwrap().unwrap();
    assert!(stem_evidence::Evidence::from_stored(&bytes).unwrap() == low_mid);
    // A non-finite number reads back as another value.
    let mut other = evidence;
    other.sum_source_cosine = f64::NAN;
    assert!(other.stored().unwrap().is_none());
}

/// A worker plan holds each distinct key of its specs once, and its
/// families are the families of the plan with the keys in each spec (D266).
#[test]
fn a_plan_with_shared_keys_gives_the_families_of_the_full_plan() {
    use mastering_engine::{adjust, distributed};
    let (mut source, stems) = material(9000.0);
    let original = source.clone();
    let evidence = stem_evidence::analyze_set(
        &mut source,
        48000,
        |role, each| {
            let i = ["drums", "bass", "other", "vocals"]
                .iter()
                .position(|x| *x == role)
                .unwrap();
            for block in stems[i].chunks(65_536) {
                each(48000, block)?;
            }
            Ok(())
        },
        None,
        &"0".repeat(64),
    )
    .unwrap();
    let plan = set::prepare(&original, 48000, Some(evidence), None)
        .unwrap()
        .worker_plan()
        .unwrap();
    let full = serde_json::to_value(&plan).unwrap();
    let shared = plan.to_json().unwrap();
    assert!(!shared["shared"].as_array().unwrap().is_empty());
    let (full_json, shared_json) = (
        serde_json::to_vec(&full).unwrap(),
        serde_json::to_vec(&shared).unwrap(),
    );
    assert!(shared_json.len() < full_json.len());
    let mut expanded = shared.clone();
    distributed::expand_specs(&mut expanded).unwrap();
    assert_eq!(expanded, full);
    // The base of ADJUST takes the same reference from the shared plan, from
    // the full plan and from a plan of a set made before D266.
    let mut before = full.clone();
    before["schema"] = distributed::FULL_PLAN_SCHEMA.into();
    let reference = adjust::Base::of_plan(&full).unwrap().reference;
    assert_eq!(adjust::Base::of_plan(&shared).unwrap().reference, reference);
    assert_eq!(adjust::Base::of_plan(&before).unwrap().reference, reference);
    let directory = std::env::temp_dir().join(format!(
        "shared-plan-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let family = |plan_json: &[u8], name: &str| {
        let output = directory.join(name);
        std::fs::create_dir_all(&output).unwrap();
        distributed::render_family(&original, 48000, plan_json, "essential", &output).map(
            |result| {
                let mut result = serde_json::to_value(result).unwrap();
                result["seconds"] = serde_json::Value::Null;
                result
            },
        )
    };
    assert_eq!(
        family(&shared_json, "shared").unwrap(),
        family(&full_json, "full").unwrap()
    );
    // A spec that names a value outside the shared values is an error.
    let mut broken = shared;
    broken["shared"] = serde_json::json!([]);
    assert!(family(&serde_json::to_vec(&broken).unwrap(), "broken").is_err());
    std::fs::remove_dir_all(&directory).unwrap();
}

/// A set manifest holds each distinct key of its specs once, and an ADJUST
/// request on it gives the result of the manifest with the keys in each
/// spec (D276).
#[test]
fn a_manifest_with_shared_keys_gives_the_adjust_result_of_the_full_manifest() {
    use mastering_engine::{adjust, distributed, treatment_render::RenderSource};
    let (mut source, stems) = material(9000.0);
    let original = source.clone();
    let evidence = stem_evidence::analyze_set(
        &mut source,
        48000,
        |role, each| {
            let i = ["drums", "bass", "other", "vocals"]
                .iter()
                .position(|x| *x == role)
                .unwrap();
            for block in stems[i].chunks(65_536) {
                each(48000, block)?;
            }
            Ok(())
        },
        None,
        &"0".repeat(64),
    )
    .unwrap();
    let plan = set::prepare(&original, 48000, Some(evidence), None)
        .unwrap()
        .worker_plan()
        .unwrap();
    let plan_value = plan.to_json().unwrap();
    let plan_json = serde_json::to_vec(&plan_value).unwrap();
    let directory = std::env::temp_dir().join(format!(
        "shared-manifest-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).unwrap();
    let results: Vec<serde_json::Value> = plan
        .families
        .iter()
        .map(|family| {
            serde_json::to_value(
                distributed::render_family(&original, 48000, &plan_json, &family.id, &directory)
                    .unwrap(),
            )
            .unwrap()
        })
        .collect();
    let manifest = distributed::finalize_set(
        RenderSource::Borrowed(&original),
        48000,
        &plan_json,
        &results,
        &directory,
    )
    .unwrap();
    assert_eq!(manifest["schema"], distributed::MANIFEST_SCHEMA);
    let shared = manifest["shared"].as_array().unwrap();
    assert!(!shared.is_empty());
    let mut full = manifest.clone();
    distributed::expand_manifest(&mut full).unwrap();
    assert!(full.get("shared").is_none());
    let versions = full["versions"].as_array().unwrap();
    assert!(versions.len() > 1);
    for version in versions {
        assert!(
            version["spec"]["deesser"]["keyed"].is_array(),
            "{}",
            version["id"]
        );
    }
    assert!(full["adjust"]["reference"]["deesser"]["keyed"].is_array());
    // Each version and the base hold the same keys once.
    let mut again = full.clone();
    distributed::share_manifest(&mut again);
    assert_eq!(again, manifest);
    // Each version and the reference of the base name the de-esser key.
    let names = ["versions", "adjust"]
        .map(|field| {
            serde_json::to_string(&manifest[field])
                .unwrap()
                .matches("{\"shared\":")
                .count()
        })
        .iter()
        .sum::<usize>();
    assert!(
        names > versions.len(),
        "{names} names of {} values",
        shared.len()
    );
    assert!(
        serde_json::to_vec(&manifest).unwrap().len() < serde_json::to_vec(&full).unwrap().len()
    );
    let base = adjust::Base::of_plan(&plan_value).unwrap();
    let request = adjust::Request {
        parent: versions[1]["id"].as_str().unwrap().into(),
        id: "streaming".into(),
        steps: Default::default(),
        preset: Some(adjust::Preset::Streaming),
    };
    let render = |name: &str, versions: &serde_json::Value| {
        let output = directory.join(name);
        std::fs::create_dir(&output).unwrap();
        let mut result = adjust::render(
            RenderSource::Borrowed(&original),
            &base,
            versions,
            &directory,
            &request,
            &output,
        )
        .unwrap();
        let wav =
            std::fs::read(output.join(result["version"]["wav_file"].as_str().unwrap())).unwrap();
        result["seconds"] = serde_json::Value::Null;
        (result, wav)
    };
    // The request takes the same reference record from both manifests, so
    // it renders the same work.
    assert_eq!(render("shared", &manifest), render("full", &full));
    // The request puts the values back only in the records that it reads
    // (D278), and each record is then the record of the full manifest.
    for (record, expanded) in manifest["versions"]
        .as_array()
        .unwrap()
        .iter()
        .zip(versions)
    {
        assert_eq!(
            &distributed::expand_record(record, shared).unwrap(),
            expanded
        );
    }
    // The plan of a request shares each spec as it takes it, and is the
    // plan of the full specs with their keys shared (D278).
    let parent = adjust::parent_record(&full, &request.parent).unwrap();
    let request_plan = adjust::plan(&base, parent, &request).unwrap();
    assert!(!request_plan["shared"].as_array().unwrap().is_empty());
    let mut again = request_plan.clone();
    distributed::expand_specs(&mut again).unwrap();
    distributed::share_specs(&mut again);
    assert_eq!(again, request_plan);
    // A spec that names a value outside the shared values is an error.
    let mut broken = manifest;
    broken["shared"] = serde_json::json!([]);
    let output = directory.join("broken");
    std::fs::create_dir(&output).unwrap();
    assert!(adjust::render(
        RenderSource::Borrowed(&original),
        &base,
        &broken,
        &directory,
        &request,
        &output,
    )
    .is_err());
    assert!(distributed::expand_manifest(&mut broken).is_err());
    std::fs::remove_dir_all(&directory).unwrap();
}

/// The files of a family in a directory, with the reference record of the
/// plan passed by hand.
struct RecordStore<'a> {
    files: mastering_engine::distributed::Directory<'a>,
    published: Option<Vec<u8>>,
    given: Option<Vec<u8>>,
}

impl mastering_engine::distributed::Store for RecordStore<'_> {
    fn deliver(
        &mut self,
        name: &str,
        audio: &mastering_engine::treatment_render::RenderAudio,
        rate: u32,
        seed: u64,
        ceiling_dbtp: f64,
    ) -> mastering_engine::audio_io::Result<(String, mastering_engine::delivery::Verification)>
    {
        self.files.deliver(name, audio, rate, seed, ceiling_dbtp)
    }
    fn raw(
        &mut self,
        name: &str,
        audio: &mastering_engine::treatment_render::RenderAudio,
        rate: u32,
    ) -> mastering_engine::audio_io::Result<String> {
        self.files.raw(name, audio, rate)
    }
    fn read(&mut self, name: &str, rate: u32) -> mastering_engine::audio_io::Result<Vec<Stereo>> {
        self.files.read(name, rate)
    }
    fn publish_reference(&mut self, record: &[u8]) -> mastering_engine::audio_io::Result<()> {
        self.published = Some(record.to_vec());
        Ok(())
    }
    fn reference(&mut self) -> mastering_engine::audio_io::Result<Option<Vec<u8>>> {
        Ok(self.given.clone())
    }
}

/// A family of a plan with shared keys decodes its specs without copies of
/// the keys, and hashes each spec with its keys in place. The reference
/// record of the Essential family is then the record of the full plan, and
/// the other families take it (D277).
#[test]
fn a_family_of_a_plan_with_shared_keys_takes_the_reference_record_of_the_full_plan() {
    use mastering_engine::distributed;
    let (mut source, stems) = material(9000.0);
    let original = source.clone();
    let evidence = stem_evidence::analyze_set(
        &mut source,
        48000,
        |role, each| {
            let i = ["drums", "bass", "other", "vocals"]
                .iter()
                .position(|x| *x == role)
                .unwrap();
            for block in stems[i].chunks(65_536) {
                each(48000, block)?;
            }
            Ok(())
        },
        None,
        &"0".repeat(64),
    )
    .unwrap();
    let plan = set::prepare(&original, 48000, Some(evidence), None)
        .unwrap()
        .worker_plan()
        .unwrap();
    let shared = plan.to_json().unwrap();
    assert!(!shared["shared"].as_array().unwrap().is_empty());
    let (full_json, shared_json) = (
        serde_json::to_vec(&plan).unwrap(),
        serde_json::to_vec(&shared).unwrap(),
    );
    let directory = std::env::temp_dir().join(format!(
        "shared-record-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let render = |plan_json: &[u8], family: &str, given: Option<Vec<u8>>, name: &str| {
        let output = directory.join(name);
        std::fs::create_dir_all(&output).unwrap();
        let mut store = RecordStore {
            files: distributed::Directory(&output),
            published: None,
            given,
        };
        let result = distributed::render_family_in(
            &original,
            48000,
            plan_json,
            family,
            &mut store,
            None,
            |_, _| Ok(()),
        )
        .unwrap();
        let mut result = serde_json::to_value(result).unwrap();
        result["seconds"] = serde_json::Value::Null;
        (result, store.published)
    };
    let (essential, record) = render(&shared_json, "essential", None, "essential-shared");
    let (full_essential, full_record) = render(&full_json, "essential", None, "essential-full");
    assert_eq!(essential, full_essential);
    let record = record.expect("the Essential family publishes its first render");
    assert_eq!(Some(&record), full_record.as_ref());
    let parsed: serde_json::Value = serde_json::from_slice(&record).unwrap();
    assert!(!parsed["reference"].is_null());
    assert_eq!(essential["reference_record"], parsed);
    let mut compared = 0;
    for family in plan.families.iter().skip(1) {
        let (own, _) = render(
            &shared_json,
            &family.id,
            None,
            &format!("{}-own", family.id),
        );
        let (given, _) = render(
            &shared_json,
            &family.id,
            Some(record.clone()),
            &format!("{}-given", family.id),
        );
        // The family that takes the record renders no reference.
        assert!(
            given["work"]["prepared_prefixes"].as_u64() < own["work"]["prepared_prefixes"].as_u64(),
            "{}",
            family.id
        );
        let (full_given, _) = render(
            &full_json,
            &family.id,
            Some(record.clone()),
            &format!("{}-full", family.id),
        );
        assert_eq!(given, full_given, "{}", family.id);
        compared += 1;
    }
    assert!(compared > 0);
    std::fs::remove_dir_all(&directory).unwrap();
}
