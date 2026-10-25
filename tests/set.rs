use mastering_engine::{
    audio_io::{Result, Stereo},
    bench, delivery, distributed,
    dsp::{EqBand, Kind},
    finishing::Domain,
    foundation::{Foundation, Region},
    set::{self, AuditoryInput, Residual, Sink, Version},
    source_treatments::SourceIdentity,
    stem_evidence,
    treatment::{BlendPoint, FocusEnvelope, Tone},
};
use serde_json::json;
use sha2::{Digest, Sha256};

#[derive(Default)]
struct MemorySink {
    audio: Vec<(String, Vec<Stereo>)>,
    descriptions: Vec<String>,
    labels: Vec<String>,
    events: Vec<(String, String)>,
    cancel_after_first: bool,
    parallel: bool,
    /// The versions that rendered beside an earlier version.
    beside: Vec<String>,
}
impl Sink for MemorySink {
    fn parallel_versions(&self) -> bool {
        self.parallel
    }
    fn comparisons(&mut self, audio: &[Stereo]) -> Result<Vec<Residual>> {
        self.audio
            .iter()
            .map(|(id, reference)| {
                Ok(Residual {
                    id: id.clone(),
                    matched_residual_db: set::matched_residual_db(reference, audio)?,
                })
            })
            .collect()
    }
    fn deliver(&mut self, version: &Version<'_>, audio: &[Stereo]) -> Result<()> {
        assert!(audio.iter().flatten().all(|x| x.is_finite()));
        assert!(version.report.render.peak_estimate_dbtp <= version.spec.ceiling_dbtp);
        self.audio.push((version.id.into(), audio.to_vec()));
        if version.rendered_beside_seconds.is_some() {
            self.beside.push(version.id.into());
        }
        self.descriptions.push(version.description.into());
        self.labels.push(version.label.into());
        self.events.push(("delivered".into(), version.id.into()));
        Ok(())
    }
    fn name(&mut self, named: &[mastering_engine::version_names::Named]) -> Result<()> {
        for n in named {
            let i = self
                .audio
                .iter()
                .position(|(id, _)| *id == n.id)
                .expect("a named version is delivered");
            self.labels[i] = n.name.into();
            self.descriptions[i] = n.description.clone();
        }
        Ok(())
    }
    fn event(&mut self, event: &'static str, id: &str) -> Result<()> {
        if self.cancel_after_first && event == "rendering" && !self.audio.is_empty() {
            return Err("Cancelled by host".into());
        }
        self.events.push((event.into(), id.into()));
        Ok(())
    }
}

fn signal(seconds: usize) -> Vec<Stereo> {
    let mut seed = 19u32;
    (0..48000 * seconds)
        .map(|i| {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = ((seed >> 8) as f64 / 16777216.0 - 0.5) * 0.12;
            let t = i as f64 / 48000.0;
            let pulse = 0.65
                * (-((i % 12000) as f64) / 700.0).exp()
                * (std::f64::consts::TAU * 120.0 * t).sin();
            let mid = noise + pulse + 0.05 * (std::f64::consts::TAU * 700.0 * t).sin();
            let side = 0.035 * (std::f64::consts::TAU * 970.0 * t).sin();
            [(mid + side) as f32, (mid - side) as f32]
        })
        .collect()
}

#[test]
fn gain_only_duplicates_and_polarity_have_different_outcomes() {
    let audio = signal(3);
    let gain: Vec<_> = audio.iter().map(|a| a.map(|x| x * 0.5)).collect();
    let inverted: Vec<_> = audio.iter().map(|a| a.map(|x| -x)).collect();
    assert!(set::matched_residual_db(&audio, &gain).unwrap() < -140.0);
    assert_eq!(set::matched_residual_db(&audio, &inverted).unwrap(), 0.0);
    assert!(set::matched_residual_db(&audio, &gain[1..]).is_err());
}

#[test]
fn worker_plan_replays_the_native_family_and_rejects_wrong_source() {
    let input = signal(3);
    let worker_plan = set::prepare(&input, 48000, None, None)
        .unwrap()
        .worker_plan()
        .unwrap();
    let plan_json = serde_json::to_vec(&worker_plan).unwrap();
    let mut sink = MemorySink::default();
    let summary = set::render(&input, 48000, None, None, &mut sink).unwrap();
    let essential_family = &worker_plan.families[0];
    assert_eq!(essential_family.id, "essential");
    for version in &essential_family.versions {
        let Some(trial) = summary.trials.iter().find(|t| t.id == version.id) else {
            // A fallback version has no trial when the family it waits on
            // delivered.
            assert!(version.fallback_for.is_some());
            continue;
        };
        if !version.dense {
            // The render sets the loudness, the restoration and the low-end
            // hold of the chosen spec: the own point can lower its target
            // (D206). A sub lift solves the low shelves of the tone again.
            let planned = |spec: &mastering_engine::treatment_render::Spec| {
                let mut spec = spec.clone();
                spec.tone
                    .stereo
                    .retain(|b| !matches!(b.kind, mastering_engine::dsp::Kind::LowShelf));
                let mut value = serde_json::to_value(&spec).unwrap();
                for field in [
                    "target_lufs",
                    "initial_drive_db",
                    "fixed_drive_db",
                    "restoration_db",
                    "low_end_hold",
                ] {
                    value.as_object_mut().unwrap().remove(field);
                }
                value
            };
            assert_eq!(planned(&trial.spec), planned(&version.candidates[0].spec));
        } else {
            assert!(version.dense_template.is_some());
        }
    }
    let directory = std::env::temp_dir().join(format!(
        "master-family-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let mut published = Vec::new();
    let result = distributed::render_family_with_callback(
        &input,
        48000,
        &plan_json,
        "essential",
        &directory,
        |version, directory, eligible| {
            assert!(directory.join(&version.wav_file).exists());
            assert_eq!(version.waveform.input_frames, input.len());
            assert_eq!(version.waveform.bins.len(), 384);
            published.push((version.id.clone(), version.wav_sha256.clone(), eligible));
            Ok(())
        },
    )
    .unwrap();
    let essential_audio: Vec<&(String, Vec<Stereo>)> = sink
        .audio
        .iter()
        .filter(|(id, _)| id == "essential" || id == "dense-essential")
        .collect();
    // The worker cannot know whether Master delivered, so it renders the
    // Denser Essential fallback; the finalizer drops it when not needed.
    assert!(result.versions.len() >= essential_audio.len());
    assert_eq!(published.len(), result.versions.len());
    assert_eq!(published[0].0, "essential");
    assert!(published[0].2);
    for (id, audio) in essential_audio {
        let version = result.versions.iter().find(|v| &v.id == id).unwrap();
        let mut expected = Vec::new();
        delivery::write_to(
            &mut expected,
            audio,
            48000,
            delivery::Format::Pcm24,
            set::delivery_seed(&worker_plan.source_pcm_sha256, id),
        )
        .unwrap();
        assert_eq!(
            version.wav_sha256,
            format!("{:x}", Sha256::digest(&expected))
        );
    }
    let mut family_results = vec![serde_json::to_value(&result).unwrap()];
    for family in worker_plan.families.iter().skip(1) {
        let other =
            distributed::render_family(&input, 48000, &plan_json, &family.id, &directory).unwrap();
        family_results.push(serde_json::to_value(other).unwrap());
    }
    let completed =
        distributed::finalize_set(&input, 48000, &plan_json, &family_results, &directory).unwrap();
    let versions = completed["versions"].as_array().unwrap();
    assert_eq!(versions.len(), summary.version_ids.len());
    for (i, version) in versions.iter().enumerate() {
        assert_eq!(version["id"], summary.version_ids[i]);
        assert_eq!(version["label"], sink.labels[i]);
        assert_eq!(version["description"], sink.descriptions[i]);
        // Every delivered version, Bold from its own family included, is
        // the native set's WAV.
        let (_, audio) = sink
            .audio
            .iter()
            .find(|(id, _)| version["id"] == id.as_str())
            .unwrap();
        let id = version["id"].as_str().unwrap();
        let mut expected = Vec::new();
        delivery::write_to(
            &mut expected,
            audio,
            48000,
            delivery::Format::Pcm24,
            set::delivery_seed(&worker_plan.source_pcm_sha256, id),
        )
        .unwrap();
        assert_eq!(
            version["output_sha256"],
            format!("{:x}", Sha256::digest(&expected)),
            "{id}"
        );
    }
    // A delivered Bold comes from its own family.
    assert!(
        !summary.version_ids.iter().any(|id| id == "bold")
            || worker_plan.families.iter().any(|f| f.id == "bold")
    );
    for (id, hash, eligible) in published {
        let final_version = versions.iter().find(|version| version["id"] == id);
        assert_eq!(eligible, final_version.is_some());
        if let Some(final_version) = final_version {
            assert_eq!(final_version["output_sha256"], hash);
        }
    }
    let mut wrong: serde_json::Value = serde_json::from_slice(&plan_json).unwrap();
    wrong["source_pcm_sha256"] = json!("0".repeat(64));
    assert!(distributed::render_family(
        &input,
        48000,
        &serde_json::to_vec(&wrong).unwrap(),
        "essential",
        &directory,
    )
    .is_err());
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn unchanged_sustained_source_does_not_force_extra_versions() {
    let input = bench::tone(997.0, 0.1, 144000, 48000);
    let original = input.clone();
    let mut sink = MemorySink::default();
    let summary = set::render(&input, 48000, None, None, &mut sink).unwrap();
    assert_eq!(summary.schema, "mastering.set.3");
    assert_eq!(summary.version_ids, vec!["essential"]);
    assert!(summary.density.denser_target_lufs.is_none());
    assert_eq!(input, original);
    assert!(summary.trials[0]
        .report
        .as_ref()
        .unwrap()
        .render
        .audio
        .is_empty());
}

#[test]
fn unchanged_foundation_fallback_is_measured_but_not_delivered_locally_or_by_finalizer() {
    let input = signal(3);
    let zero = vec![[0.0, 0.0]; input.len()];
    let foundation = Foundation {
        rule: "set.all-role-foundation.v1",
        regions: vec![Region {
            role: "bass",
            domain: Domain::Mid,
            centre_hz: 700.0,
            q: 1.0,
            positive_seconds: 12.0,
            predicted_pair_ratio_gain_db: 0.5,
            score_second_db: 6.0,
            mean_wet_duty: 1.0,
            gain_db: 6.0,
        }],
        tone: Tone {
            mid: vec![EqBand {
                kind: Kind::Peak,
                hz: 700.0,
                q: 1.0,
                gain_db: 6.0,
            }],
            ..Tone::default()
        },
        envelopes: vec![FocusEnvelope {
            domain: Domain::Mid,
            filter_index: 0,
            points: vec![
                BlendPoint {
                    sample: 0,
                    wet: 1.0,
                },
                BlendPoint {
                    sample: input.len() - 1,
                    wet: 1.0,
                },
            ],
        }],
    };
    let make_evidence = || {
        let (mut evidence, _) = stem_evidence::analyze_with_reconstruction(
            &input,
            [&input[..], &zero[..], &zero[..], &zero[..]],
            48000,
            48000,
        )
        .unwrap();
        evidence.foundation = Some(foundation.clone());
        evidence
    };
    let mut sink = MemorySink::default();
    let summary = set::render(&input, 48000, Some(make_evidence()), None, &mut sink).unwrap();
    // The plan holds no comparison-only fallback: the essential family
    // carries the foundation and no version is named after the unchanged tone.
    assert!(summary
        .trials
        .iter()
        .all(|trial| trial.id != "unchanged-tone"));
    assert!(!summary
        .version_ids
        .iter()
        .any(|id| id.contains("unchanged")));
    let plan = set::prepare(&input, 48000, Some(make_evidence()), None)
        .unwrap()
        .worker_plan()
        .unwrap();
    let plan_json = serde_json::to_vec(&plan).unwrap();
    let directory = std::env::temp_dir().join(format!(
        "master-fallback-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let mut fallback_previews = Vec::new();
    let results = plan
        .families
        .iter()
        .map(|family| {
            distributed::render_family_with_callback(
                &input,
                48000,
                &plan_json,
                &family.id,
                &directory,
                |version, _, eligible| {
                    fallback_previews.push((version.id.clone(), eligible));
                    Ok(())
                },
            )
            .unwrap()
        })
        .map(|result| serde_json::to_value(result).unwrap())
        .collect::<Vec<_>>();
    assert!(fallback_previews
        .iter()
        .any(|(id, eligible)| id == "essential" && *eligible));
    assert!(fallback_previews
        .iter()
        .all(|(id, eligible)| id != "unchanged-tone" || !eligible));
    let completed =
        distributed::finalize_set(&input, 48000, &plan_json, &results, &directory).unwrap();
    let versions = completed["versions"].as_array().unwrap();
    assert_eq!(versions.len(), summary.version_ids.len());
    assert!(versions
        .iter()
        .all(|version| version["id"] != "unchanged-tone"));
    assert!(completed["trials"]
        .as_array()
        .unwrap()
        .iter()
        .all(|trial| trial["id"] != "unchanged-tone"));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn completed_versions_are_delivered_before_later_work_and_host_can_stop() {
    let input = signal(3);
    let mut sink = MemorySink {
        cancel_after_first: true,
        ..MemorySink::default()
    };
    let result = set::render(&input, 48000, None, None, &mut sink);
    assert!(result
        .err()
        .unwrap()
        .to_string()
        .contains("Cancelled by host"));
    assert_eq!(sink.audio.len(), 1);
    assert_eq!(sink.audio[0].0, "essential");
    assert_eq!(sink.audio[0].1.len(), input.len());
}

#[test]
fn one_job_generates_renders_and_describes_source_focus_and_density() {
    let input = signal(12);
    let original = input.clone();
    let hash = "a".repeat(64);
    let pcm = stem_evidence::fingerprint(&input);
    let centres: Vec<_> = (0..64)
        .map(|i| (10f64.powf((3.5 + i as f64 * 0.5) / 21.4) - 1.0) / 0.00437)
        .collect();
    let bands:Vec<_>=centres.iter().enumerate().map(|(i,hz)|{
        if (13..=30).contains(&i) { json!({"hz":hz,"valid":true,"eligible_seconds":11.0,
            "vocal_excitation_share_percent":1.0,"vocal_to_accompaniment_p10_p50_p90_db":[-12.0,-6.0,-2.0],
            "local_mid_plus3_ratio_delta_db":0.5,"local_side_plus3_ratio_delta_db":-0.5}) }
        else {json!({"hz":hz,"valid":false})}
    }).collect();
    let report = json!({"schema":"mastering.auditory-analysis.1","sources_unchanged":true,"centres_hz":centres,
        "files":{"Original":{"sha256":hash,"sample_rate":48000,"frames":input.len()}},
        "stems":{"source_sha256":hash,"stem_sha256":{"drums":hash,"bass":hash,"other":hash,"vocals":hash},
            "source_conversion_residual_db":-80.0,"reconstruction":{"residual_relative_db":-30.0,"cosine":0.999},"active_seconds":11.5,"bands":bands}});
    let bytes = serde_json::to_vec(&report).unwrap();
    let mut sink = MemorySink::default();
    let summary = set::render(
        &input,
        48000,
        None,
        Some(AuditoryInput {
            json: &bytes,
            source_pcm_sha256: &pcm,
            source: SourceIdentity {
                file_sha256: &hash,
                sample_rate: 48000,
                frames: input.len(),
            },
        }),
        &mut sink,
    )
    .unwrap();
    // Master carries the stem focus; when its change is too small to
    // describe it is screened and Denser Essential renders as the fallback.
    let status = |id: &str| summary.trials.iter().find(|t| t.id == id).map(|t| t.status);
    assert_eq!(status("essential"), Some("retained"));
    if !set::DENSER_VERSIONS {
        // No Denser version is planned (D135).
        assert_eq!(status("dense-master"), None);
        assert_eq!(status("dense-essential"), None);
    } else {
        match status("master") {
            Some("retained") => {
                // The dense bounds can exclude every denser Master candidate of
                // this transient fixture; Denser Essential then stands in.
                if status("dense-master") == Some("retained") {
                    assert_ne!(status("dense-essential"), Some("retained"));
                } else {
                    // The same bounds can exclude the Essential fallback too, and
                    // a Denser version that differs only in level is a duplicate.
                    assert!(matches!(
                        status("dense-essential"),
                        Some("retained") | Some("no-opportunity") | Some("duplicate")
                    ));
                }
            }
            _ => assert!(matches!(
                status("dense-essential"),
                Some("retained") | Some("duplicate")
            )),
        }
    }
    assert!(summary.version_ids.len() >= 2);
    assert_eq!(summary.density.rule, "set.audible-loudness.v6");
    // Every version sits at the margin under the audibility point of peak
    // control; a Denser target sits above it and at most at the point.
    let density = &summary.density;
    let point = density.audibility.as_ref().unwrap().lufs;
    let expected = (point - set::AUDIBILITY_MARGIN_LU).min(set::TARGET_MAXIMUM_LUFS);
    assert!((density.base_target_lufs - expected).abs() < 1e-9);
    if let Some(denser) = density.denser_target_lufs {
        assert!(denser > density.base_target_lufs && denser <= point + 1e-9);
        assert!(denser <= -6.5);
    }
    let bold_target = density.bold_target_lufs.unwrap();
    assert_eq!(bold_target, density.base_target_lufs);
    for trial in summary.trials.iter().filter(|t| t.id == "bold") {
        assert!((trial.spec.target_lufs - bold_target).abs() < 1e-9);
    }
    // A denser version exists only within the base render's limiter headroom.
    for id in ["dense-master", "dense-essential"] {
        if let Some(trial) = summary
            .trials
            .iter()
            .find(|t| t.id == id && t.status == "retained")
        {
            assert!(trial.spec.target_lufs > summary.density.base_target_lufs);
        }
    }
    assert!(summary.work.prepared_prefixes >= 2);
    // Base and denser versions use different clip budgets, so their probes
    // are distinct; prefixes are still shared.
    for trial in summary.trials.iter().filter(|t| t.status == "retained") {
        assert_eq!(trial.spec.ceiling_dbtp, set::SET_CEILING_DBTP);
        assert_eq!(
            trial.spec.clipper_db,
            if trial.id.starts_with("dense-") {
                set::DENSER_CLIPPER_DB
            } else {
                set::BASE_CLIPPER_DB
            }
        );
        assert_eq!(trial.spec.limiter.hold_ms, 12.0);
        assert_eq!(trial.spec.limiter.slow_release_ms, 0.0);
        assert_eq!(trial.spec.limiter.release_ms, set::LIMITER_RELEASE_MS);
        assert_eq!(trial.spec.true_peak, set::TRUE_PEAK);
        assert!([0.0, set::PEAK_ATTACK_MS].contains(&trial.spec.limiter.attack_ms));
        assert!(trial
            .spec
            .tone
            .side
            .iter()
            .filter(|b| b.hz < set::LOW_SIDE_BELL_HZ)
            .all(|b| b.gain_db.abs() <= set::LOW_SIDE_BELL_DB));
    }
    let master = summary.trials.iter().find(|t| t.id == "master").unwrap();
    // The Master carries the stem focus, or falls back to the broad moves
    // alone when the joint tone fails its tonal gates. On this dull fixture
    // the presence balance takes its full bound and the fallback applies.
    let chosen = master.report.as_ref().map(|r| {
        let v = serde_json::to_value(r).unwrap();
        v["candidates"][v["chosen_candidate"].as_u64().unwrap() as usize]["reason"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    });
    assert!(
        !master.spec.tone.mid.is_empty() || chosen.as_deref() == Some("broad moves without focus"),
        "master carries the stem focus or its broad-move fallback: {chosen:?}"
    );
    assert!(
        summary.trials.iter().any(|t| !t
            .report
            .as_ref()
            .unwrap()
            .source_constraint_diagnostics
            .is_empty()),
        "old preservation diagnostics must not silently veto stronger versions"
    );
    // The measured detail states the figures; each change is a sentence,
    // and "at matched level" is said once.
    let details: Vec<&String> = summary
        .trials
        .iter()
        .filter(|t| t.status == "retained")
        .filter_map(|t| t.detail.as_ref())
        .collect();
    assert_eq!(details.len(), sink.labels.len());
    assert!(details.iter().all(|d| d.contains("LUFS")
        && !d.contains(';')
        && d.matches("at matched level").count() <= 1));
    // The description is plain words; the name is a measured character,
    // used once in the set.
    assert!(sink
        .descriptions
        .iter()
        .all(|d| d.ends_with('.') && !d.contains("dB")));
    assert!(sink
        .labels
        .iter()
        .all(|l| mastering_engine::version_names::NAMES.contains(&l.as_str())));
    let mut unique = sink.labels.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), sink.labels.len());
    for trial in summary
        .trials
        .iter()
        .filter(|t| t.id.starts_with("dense-") && t.status == "retained")
    {
        assert!(trial.spec.target_lufs > summary.density.base_target_lufs);
        assert_eq!(trial.spec.clipper_db, set::DENSER_CLIPPER_DB);
    }
    assert!(
        sink.labels.iter().all(|label| !label.contains("Hz")),
        "display names describe the measured result; exact filter frequencies stay in evidence"
    );
    assert_eq!(
        sink.audio
            .iter()
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>(),
        summary.version_ids
    );
    let first_delivery = sink.events.iter().position(|e| e.0 == "delivered").unwrap();
    // Each version is delivered before the next one renders.
    let second_render = sink
        .events
        .iter()
        .enumerate()
        .filter(|(_, e)| e.0 == "rendering")
        .nth(1)
        .map(|(i, _)| i)
        .unwrap();
    assert!(first_delivery < second_render);
    assert_eq!(input, original);
    let plan = set::prepare(
        &input,
        48000,
        None,
        Some(AuditoryInput {
            json: &bytes,
            source_pcm_sha256: &pcm,
            source: SourceIdentity {
                file_sha256: &hash,
                sample_rate: 48000,
                frames: input.len(),
            },
        }),
    )
    .unwrap()
    .worker_plan()
    .unwrap();
    let plan_json = serde_json::to_vec(&plan).unwrap();
    let directory = std::env::temp_dir().join(format!(
        "master-finalize-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let mut families = Vec::new();
    for family in &plan.families {
        let result =
            distributed::render_family(&input, 48000, &plan_json, &family.id, &directory).unwrap();
        families.push(serde_json::to_value(result).unwrap());
    }
    let completed =
        distributed::finalize_set(&input, 48000, &plan_json, &families, &directory).unwrap();
    let versions = completed["versions"].as_array().unwrap();
    assert_eq!(
        versions
            .iter()
            .map(|v| v["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        summary
            .version_ids
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
    );
    for (i, version) in versions.iter().enumerate() {
        assert_eq!(version["label"], sink.labels[i]);
        let id = version["id"].as_str().unwrap();
        assert_eq!(version["description"], sink.descriptions[i], "{id}");
        let (_, audio) = sink.audio.iter().find(|(v, _)| v == id).unwrap();
        let mut expected = Vec::new();
        delivery::write_to(
            &mut expected,
            audio,
            48000,
            delivery::Format::Pcm24,
            set::delivery_seed(&plan.source_pcm_sha256, id),
        )
        .unwrap();
        assert_eq!(
            version["output_sha256"],
            format!("{:x}", Sha256::digest(&expected)),
            "{id}"
        );
    }
    std::fs::remove_dir_all(directory).unwrap();
    let wrong = set::render(
        &input,
        48000,
        None,
        Some(AuditoryInput {
            json: &bytes,
            source_pcm_sha256: &hash,
            source: SourceIdentity {
                file_sha256: &hash,
                sample_rate: 48000,
                frames: input.len(),
            },
        }),
        &mut MemorySink::default(),
    );
    assert!(wrong.is_err());
}

#[test]
fn a_version_matches_the_mix_only_at_the_mix_level() {
    let input = signal(3);
    let source = mastering_engine::pipeline::analyze_source(&input, 48000).unwrap();
    let signature = |audio: &[Stereo]| {
        mastering_engine::selection::Signature::new(
            &mastering_engine::metrics(audio, 48000),
            &mastering_engine::stereo::analyze(audio, 48000).unwrap(),
            mastering_engine::dsp::loudness(audio, 48000).unwrap(),
            0.0,
        )
    };
    assert!(set::same_as_mix(source.mix(), &signature(&input)));
    // 1.6 dB louder with the same tone and image: a louder master, not the mix.
    let louder: Vec<Stereo> = input.iter().map(|a| a.map(|x| x * 1.2)).collect();
    assert!(!set::same_as_mix(source.mix(), &signature(&louder)));
}

/// A directory for the files of one test.
fn new_directory(name: &str) -> std::path::PathBuf {
    let directory = std::env::temp_dir().join(format!(
        "{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    directory
}

#[test]
fn a_bass_version_whose_bass_candidates_fail_renders_its_candidates_without_the_shelves() {
    use mastering_engine::selection::{self, GateChannel, TonalGate};
    // One 15 s section of bass at 70 Hz, low mids at 400 Hz and body at
    // 1 kHz.
    let mut seed = 23u32;
    let input: Vec<Stereo> = (0..48000 * 16)
        .map(|i| {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = ((seed >> 8) as f64 / 16777216.0 - 0.5) * 0.02;
            let t = std::f64::consts::TAU * i as f64 / 48000.0;
            let mid = 0.25 * (70.0 * t).sin() + 0.2 * (400.0 * t).sin() + 0.1 * (1000.0 * t).sin();
            let side = 0.02 * (970.0 * t).sin();
            [(mid + noise + side) as f32, (mid + noise - side) as f32]
        })
        .collect();
    let plan = set::prepare(&input, 48000, None, None)
        .unwrap()
        .worker_plan()
        .unwrap();
    let essential = plan.families[0].versions[0].candidates[0].spec.clone();
    let source = mastering_engine::pipeline::analyze_source(&input, 48000).unwrap();
    let mut renderer = mastering_engine::treatment_render::Renderer::new(&input, 48000);
    let mut measured =
        selection::measure(&input, &source, renderer.render(&essential).unwrap()).unwrap();
    measured.tone = Some(essential.effective_tone());
    let reference = selection::Reference::of(&measured);
    // The candidate is Essential with a gate that lets the band under
    // 120 Hz rise by at most 0.25 dB against the band from 120 to 500 Hz.
    // The bass shelves lift the 70 Hz bass more than the 400 Hz low mids,
    // so the gate fails with them and passes without them.
    let shelves = [
        mastering_engine::dsp::EqBand {
            kind: mastering_engine::dsp::Kind::LowShelf,
            hz: mastering_engine::open::SUB_SHELF_HZ,
            q: std::f64::consts::FRAC_1_SQRT_2,
            gain_db: 2.0,
        },
        mastering_engine::dsp::EqBand {
            kind: mastering_engine::dsp::Kind::LowShelf,
            hz: mastering_engine::open::BASS_SHELF_HZ,
            q: std::f64::consts::FRAC_1_SQRT_2,
            gain_db: 1.0,
        },
    ];
    let candidate = selection::Candidate {
        reason: "strongest".into(),
        spec: essential.clone(),
        gates: selection::Gates {
            tonal: vec![TonalGate {
                band: 0,
                direction: -1,
                minimum_db: -0.25,
                relative_to: Some(1),
                channel: GateChannel::Stereo,
            }],
            low_end: true,
            ..selection::Gates::default()
        },
    };
    let (chosen, trials) = set::base_ladder(
        &mut renderer,
        &input,
        &source,
        Some(&reference),
        Some(&essential),
        &[candidate],
        &shelves,
        // The tone of Essential carries the sub and bass moves of its
        // character (D206); this test renders no sub lift of them.
        false,
        |_, _| Ok(()),
    )
    .unwrap();
    let chosen = chosen.expect("the candidate without the shelves passes");
    // The index names the planned candidate in both ladders.
    assert_eq!(chosen.index, 0);
    // The chosen candidate has the planned tone, without the shelves.
    assert_eq!(
        serde_json::to_value(&chosen.spec.tone).unwrap(),
        serde_json::to_value(&essential.tone).unwrap()
    );
    let (last, bass) = trials.split_last().unwrap();
    assert!(last.passed && last.reason == "strongest");
    assert!(!bass.is_empty());
    for trial in bass {
        assert!(!trial.passed && trial.reason.starts_with("strongest with the bass shelves"));
        // The gate fails on the render or on its prediction from the tone.
        assert!(trial.checks.iter().any(|c| {
            (c.id == "selection.tonal_change" || c.id == "selection.tonal_prediction") && !c.passed
        }));
    }
    // The version is delivered as the plain version, and its basis says so.
    // A tone without low shelves is a plain tone; the tone of Essential
    // carries the low shelves of its character (D206).
    let mut plain = chosen.spec.clone();
    plain
        .tone
        .stereo
        .retain(|b| !matches!(b.kind, mastering_engine::dsp::Kind::LowShelf));
    let (label, basis) = set::delivered_base(set::FULL_LABEL, "planned", true, &plain);
    assert_eq!(label, set::MASTER_LABEL);
    assert!(basis.starts_with("planned; set.bass-version.v4: "));
    let (label, basis) = set::delivered_base(set::FULL_LABEL, "planned", false, &chosen.spec);
    assert_eq!(
        (label.as_str(), basis.as_str()),
        (set::FULL_LABEL, "planned")
    );
}

#[test]
fn the_finalizer_delivers_the_speculative_bold_when_master_chose_no_candidate() {
    let input = signal(3);
    let plan = set::prepare(&input, 48000, None, None)
        .unwrap()
        .worker_plan()
        .unwrap();
    let plan_json = serde_json::to_vec(&plan).unwrap();
    let directory = new_directory("master-unchosen");
    let mut results: Vec<serde_json::Value> = plan
        .families
        .iter()
        .map(|family| {
            let result =
                distributed::render_family(&input, 48000, &plan_json, &family.id, &directory)
                    .unwrap();
            serde_json::to_value(result).unwrap()
        })
        .collect();
    let index = |id: &str| plan.families.iter().position(|f| f.id == id).unwrap();
    let speculative = results[index("bold")]["versions"][0].clone();
    assert_eq!(speculative["wav_file"], "bold-speculative.wav");
    let master = results[index("master")].clone();
    // A master family in which no candidate passes: the family returns no
    // version, and its own Bold does not render.
    results[index("master")]["versions"] = json!([]);
    let completed =
        distributed::finalize_set(&input, 48000, &plan_json, &results, &directory).unwrap();
    let trial = |id: &str| {
        completed["trials"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["id"] == id)
            .cloned()
            .unwrap()
    };
    assert_eq!(trial("master")["status"], "no-opportunity");
    // The speculative Bold built from the planned first candidate without
    // the step, so it stands, and its basis says so.
    let bold = trial("bold");
    assert_ne!(bold["status"], "no-opportunity");
    assert!(bold["basis"]
        .as_str()
        .unwrap()
        .ends_with(&set::unchosen_base_basis("")));
    let versions = completed["versions"].as_array().unwrap();
    assert!(versions.iter().any(|v| v["id"] == "bold"
        && v["output_sha256"] == speculative["wav_sha256"]
        && v["wav_file"] == "bold-speculative.wav"));
    // When the master family chose another candidate, its own Bold stands
    // for it. The speculative Bold stays out, also when that Bold failed.
    results[index("master")] = master;
    results[index("master")]["versions"][0]["chosen_candidate"] = json!(1);
    let completed =
        distributed::finalize_set(&input, 48000, &plan_json, &results, &directory).unwrap();
    let bold = completed["trials"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "bold")
        .cloned()
        .unwrap();
    assert_eq!(bold["status"], "no-opportunity");
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn versions_that_render_beside_each_other_match_a_render_in_turn() {
    let input = signal(3);
    let render = |parallel: bool| {
        let mut sink = MemorySink {
            parallel,
            ..MemorySink::default()
        };
        let summary = set::render(&input, 48000, None, None, &mut sink).unwrap();
        let summary = serde_json::to_value(&summary).unwrap();
        (sink, summary)
    };
    let (in_turn, in_turn_summary) = render(false);
    let (beside, beside_summary) = render(true);
    // Essential renders first; Bold and Open render beside Master.
    let delivered: Vec<&str> = beside.audio.iter().map(|(id, _)| id.as_str()).collect();
    assert!(delivered.len() >= 3, "delivered {delivered:?}");
    assert!(in_turn.beside.is_empty());
    assert!(!beside.beside.is_empty(), "delivered {delivered:?}");
    assert!(beside
        .beside
        .iter()
        .all(|id| id != "essential" && id != "master"));
    assert_eq!(in_turn.events, beside.events);
    assert_eq!(in_turn.labels, beside.labels);
    assert_eq!(in_turn.descriptions, beside.descriptions);
    assert_eq!(in_turn_summary, beside_summary);
    assert_eq!(in_turn.audio.len(), beside.audio.len());
    for ((id, a), (other, b)) in in_turn.audio.iter().zip(&beside.audio) {
        assert_eq!(id, other);
        assert_eq!(a.len(), b.len());
        assert!(
            a.iter()
                .zip(b)
                .all(|(x, y)| x[0].to_bits() == y[0].to_bits() && x[1].to_bits() == y[1].to_bits()),
            "{id} differs"
        );
    }
}

/// The files of a family in a directory, with the reference record of the
/// plan passed by hand.
struct SharedStore<'a> {
    files: distributed::Directory<'a>,
    published: Option<Vec<u8>>,
    given: Option<Vec<u8>>,
}

impl distributed::Store for SharedStore<'_> {
    fn deliver(
        &mut self,
        name: &str,
        audio: &[Stereo],
        rate: u32,
        seed: u64,
        ceiling_dbtp: f64,
    ) -> Result<(String, delivery::Verification)> {
        self.files.deliver(name, audio, rate, seed, ceiling_dbtp)
    }
    fn raw(&mut self, name: &str, audio: &[Stereo], rate: u32) -> Result<String> {
        self.files.raw(name, audio, rate)
    }
    fn read(&mut self, name: &str, rate: u32) -> Result<Vec<Stereo>> {
        self.files.read(name, rate)
    }
    fn publish_reference(&mut self, record: &[u8]) -> Result<()> {
        self.published = Some(record.to_vec());
        Ok(())
    }
    fn reference(&mut self) -> Result<Option<Vec<u8>>> {
        Ok(self.given.clone())
    }
}

#[test]
fn a_family_with_the_shared_reference_renders_as_a_family_that_renders_it() {
    let input = signal(3);
    let plan = set::prepare(&input, 48000, None, None)
        .unwrap()
        .worker_plan()
        .unwrap();
    let plan_json = serde_json::to_vec(&plan).unwrap();
    let render = |family: &str, given: Option<Vec<u8>>, name: &str| {
        let directory = new_directory(name);
        let mut store = SharedStore {
            files: distributed::Directory(&directory),
            published: None,
            given,
        };
        let result =
            distributed::render_family_in(&input, 48000, &plan_json, family, &mut store, |_, _| {
                Ok(())
            })
            .unwrap();
        let mut files: Vec<(String, Vec<u8>)> = std::fs::read_dir(&directory)
            .unwrap()
            .map(|e| {
                let path = e.unwrap().path();
                (
                    path.file_name().unwrap().to_string_lossy().into_owned(),
                    std::fs::read(&path).unwrap(),
                )
            })
            .collect();
        files.sort();
        std::fs::remove_dir_all(&directory).unwrap();
        let mut value = serde_json::to_value(&result).unwrap();
        value["seconds"] = json!(0);
        (value, files, store.published)
    };
    let (_, _, record) = render("essential", None, "shared-essential");
    let record = record.expect("the Essential family publishes its first render");
    let parsed: serde_json::Value = serde_json::from_slice(&record).unwrap();
    assert_eq!(parsed["schema"], distributed::REFERENCE_SCHEMA);
    assert!(!parsed["reference"].is_null());
    for family in ["master", "bold", "open"] {
        if !plan.families.iter().any(|f| f.id == family) {
            continue;
        }
        let (own, own_files, published) = render(family, None, "shared-own");
        assert!(published.is_none(), "only the Essential family publishes");
        let (shared, shared_files, _) = render(family, Some(record.clone()), "shared-given");
        assert_eq!(own_files, shared_files, "{family} files");
        // The family that takes the record renders no reference.
        assert!(
            shared["work"]["prepared_prefixes"].as_u64()
                < own["work"]["prepared_prefixes"].as_u64()
        );
        // A record of another spec is not taken: the family renders the
        // reference itself.
        let mut other: serde_json::Value = parsed.clone();
        other["spec_sha256"] = json!("0".repeat(64));
        let (refused, refused_files, _) = render(
            family,
            Some(serde_json::to_vec(&other).unwrap()),
            "shared-other",
        );
        assert_eq!(refused, own, "{family} with a record of another spec");
        assert_eq!(refused_files, own_files);
        let (mut own, mut shared) = (own, shared);
        own["work"] = json!(null);
        shared["work"] = json!(null);
        assert_eq!(own, shared, "{family} result");
    }
}

#[test]
fn a_version_that_chooses_no_candidate_keeps_its_trials_and_failed_gates() {
    let input = signal(3);
    let plan = set::prepare(&input, 48000, None, None)
        .unwrap()
        .worker_plan()
        .unwrap();
    let master = plan.families.iter().position(|f| f.id == "master").unwrap();
    // A limiter bound that no render meets fails every candidate of master.
    let mut plan_value = serde_json::to_value(&plan).unwrap();
    for candidate in plan_value["families"][master]["versions"][0]["candidates"]
        .as_array_mut()
        .unwrap()
    {
        candidate["gates"]["limiter_maximum_db"] = json!(-1.0);
    }
    let plan_json = serde_json::to_vec(&plan_value).unwrap();
    let directory = new_directory("unchosen-trials");
    let results: Vec<serde_json::Value> = plan
        .families
        .iter()
        .map(|family| {
            let result =
                distributed::render_family(&input, 48000, &plan_json, &family.id, &directory)
                    .unwrap();
            serde_json::to_value(result).unwrap()
        })
        .collect();
    let family = &results[master];
    assert_eq!(family["schema"], distributed::FAMILY_SCHEMA);
    assert!(family["versions"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v["id"] != "master"));
    let unchosen = &family["unchosen"][0];
    assert_eq!(unchosen["id"], "master");
    let trials = unchosen["candidates"].as_array().unwrap();
    let failed = unchosen["failed_gates"].as_array().unwrap();
    assert!(!trials.is_empty());
    assert_eq!(trials.len(), failed.len());
    for (trial, gates) in trials.iter().zip(failed) {
        assert_eq!(trial["passed"], false);
        assert_eq!(gates["candidate"], trial["index"]);
        assert_eq!(gates["reason"], trial["reason"]);
        assert!(gates["gates"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g == "selection.limiter_maximum"));
    }
    // The other families render as before; only master is absent.
    for (index, result) in results.iter().enumerate() {
        if index != master {
            assert_eq!(result["unchosen"], json!([]));
        }
    }
    let completed =
        distributed::finalize_set(&input, 48000, &plan_json, &results, &directory).unwrap();
    let trial = completed["trials"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["id"] == "master")
        .cloned()
        .unwrap();
    assert_eq!(trial["status"], "no-opportunity");
    assert_eq!(trial["candidates"], unchosen["candidates"]);
    assert_eq!(trial["failed_gates"], unchosen["failed_gates"]);
    assert!(completed["versions"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v["id"] != "master"));
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn a_request_keeps_the_sub_lift_of_its_parent() {
    use mastering_engine::{adjust, dsp, open, selection};
    // A loud 120 Hz pulse over noise: peak control and the compressor turn
    // down the pulses, which carry the bass, so a render falls short of the
    // low end that the shelves give.
    let input = signal(8);
    let plan = set::prepare(&input, 48000, None, None)
        .unwrap()
        .worker_plan()
        .unwrap();
    let base = adjust::Base::of_plan(&serde_json::to_value(&plan).unwrap()).unwrap();
    let mut spec = plan.families[0].versions[0].candidates[0].spec.clone();
    let shelf = |hz, gain_db| dsp::EqBand {
        kind: dsp::Kind::LowShelf,
        hz,
        q: std::f64::consts::FRAC_1_SQRT_2,
        gain_db,
    };
    spec.tone.stereo.extend([
        shelf(open::SUB_SHELF_HZ, 2.0),
        shelf(open::BASS_SHELF_HZ, 1.0),
    ]);
    spec.target_lufs = -8.0;
    let candidate = selection::Candidate {
        reason: "planned".into(),
        spec,
        gates: selection::Gates::default(),
    };
    // A version of a set renders the sub lift of this candidate.
    let source = mastering_engine::pipeline::analyze_source(&input, 48000).unwrap();
    let mut renderer = mastering_engine::treatment_render::Renderer::new(&input, 48000);
    let (_, trials) = set::base_ladder(
        &mut renderer,
        &input,
        &source,
        None,
        None,
        std::slice::from_ref(&candidate),
        &[],
        true,
        |_, _| Ok(()),
    )
    .unwrap();
    assert!(trials
        .iter()
        .any(|t| t.reason == "planned with the sub lift"));
    // A request on a version with this spec keeps its shelves.
    let request = adjust::Request {
        parent: "master".into(),
        id: "master-request".into(),
        steps: adjust::Steps::default(),
        preset: None,
    };
    let request_plan = adjust::worker_plan(
        &base,
        &request.id,
        &request.parent,
        std::slice::from_ref(&candidate),
        &request,
    )
    .unwrap();
    assert_eq!(request_plan["families"][0]["versions"][0]["lifted"], true);
    let directory = new_directory("request-lift");
    let family = distributed::render_family(
        &input,
        48000,
        &serde_json::to_vec(&request_plan).unwrap(),
        &request.id,
        &directory,
    )
    .unwrap();
    let version = &family.versions[0];
    assert!(version
        .candidates
        .iter()
        .all(|t| !t.reason.ends_with("with the sub lift")));
    let gains = |spec: &mastering_engine::treatment_render::Spec| -> Vec<f64> {
        spec.tone.stereo.iter().map(|b| b.gain_db).collect()
    };
    let delivered = distributed::decode_spec(&version.spec).unwrap();
    // The plan JSON parses a gain to within one unit of the last place.
    assert_eq!(gains(&delivered).len(), gains(&candidate.spec).len());
    for (a, b) in gains(&delivered).iter().zip(gains(&candidate.spec)) {
        assert!((a - b).abs() < 1e-12, "{a} {b}");
    }
}
