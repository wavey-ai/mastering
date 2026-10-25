use mastering_engine::source_treatments::{self, ProposalSet, SourceIdentity};
use serde_json::{json, Value};

const SOURCE_HASH: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn fixture() -> Value {
    let centres: Vec<_> = (0..64)
        .map(|i| (10f64.powf((3.5 + i as f64 * 0.5) / 21.4) - 1.0) / 0.00437)
        .collect();
    let bands: Vec<_> = centres
        .iter()
        .map(|hz| json!({"hz":hz,"valid":false}))
        .collect();
    json!({
        "schema":"mastering.auditory-analysis.1", "sources_unchanged":true,
        "centres_hz":centres,
        "files":{"Original":{"sha256":SOURCE_HASH,"sample_rate":48000,"frames":960000}},
        "stems":{
            "source_sha256":SOURCE_HASH,
            "stem_sha256":{"drums":SOURCE_HASH,"bass":SOURCE_HASH,"other":SOURCE_HASH,"vocals":SOURCE_HASH},
            "source_conversion_residual_db":-80.0,
            "reconstruction":{"residual_relative_db":-30.0,"cosine":0.999},
            "active_seconds":18.0,"bands":bands
        }
    })
}

fn eligible(report: &mut Value, indices: &[usize], domain: &str) {
    for &i in indices {
        let hz = report["centres_hz"][i].clone();
        report["stems"]["bands"][i] = json!({
            "hz":hz,"valid":true,"eligible_seconds":12.0,
            "vocal_excitation_share_percent":1.0,
            "vocal_to_accompaniment_p10_p50_p90_db":[-12.0,-6.0,-2.0],
            "local_mid_plus3_ratio_delta_db":if domain=="mid" {0.5} else {-0.5},
            "local_side_plus3_ratio_delta_db":if domain=="side" {0.5} else {-0.5}
        });
    }
}

fn propose(report: &Value) -> mastering_engine::audio_io::Result<ProposalSet> {
    source_treatments::propose(
        &serde_json::to_vec(report)?,
        SourceIdentity {
            file_sha256: SOURCE_HASH,
            sample_rate: 48000,
            frames: 960000,
        },
    )
}

#[test]
fn absent_or_unhelpful_stems_keep_only_unchanged_processing() {
    let mut report = fixture();
    assert_eq!(propose(&report).unwrap().proposals.len(), 1);
    report.as_object_mut().unwrap().remove("stems");
    let set = propose(&report).unwrap();
    assert_eq!(set.proposals.len(), 1);
    assert_eq!(set.proposals[0].id, "unchanged");
    assert!(set.proposals[0].tone.stereo.is_empty());
    assert!(set.proposals[0].tone.mid.is_empty());
    assert!(set.proposals[0].tone.side.is_empty());
    assert!(!set.notes.is_empty());
}

#[test]
fn direction_frequency_and_width_follow_the_measured_source() {
    for domain in ["mid", "side"] {
        for indices in [[12, 13, 14], [28, 29, 30]] {
            let mut report = fixture();
            eligible(&mut report, &indices, domain);
            // Nonuniform excitation must move the centre toward the stronger band.
            report["stems"]["bands"][indices[2]]["vocal_excitation_share_percent"] = json!(2.0);
            let set = propose(&report).unwrap();
            assert_eq!(set.proposals.len(), 2);
            let p = &set.proposals[1];
            let region = &p.basis[0];
            let centres: Vec<_> = indices
                .iter()
                .map(|&i| report["centres_hz"][i].as_f64().unwrap())
                .collect();
            let expected =
                ((centres[0].ln() + centres[1].ln() + 2.0 * centres[2].ln()) / 4.0).exp();
            assert!((region.centre_hz - expected).abs() < 1e-9);
            assert_eq!(region.domain, domain);
            assert_eq!(region.predicted_median_ratio_gain_range_db, [0.5, 0.5]);
            let filters = if domain == "mid" {
                &p.tone.mid
            } else {
                &p.tone.side
            };
            assert_eq!(filters.len(), 1);
            assert!((filters[0].hz - expected).abs() < 1e-9);
            assert_eq!(filters[0].gain_db, 3.0);
            assert_eq!(
                filters[0].q,
                (expected / (centres[2] - centres[0])).clamp(0.5, 2.0)
            );
            assert!(p.tone.stereo.is_empty());
        }
    }
}

#[test]
fn insufficient_activity_or_opposite_prediction_cannot_create_a_focus() {
    for field in [
        "eligible_seconds",
        "vocal_excitation_share_percent",
        "vocal_to_accompaniment_p10_p50_p90_db",
        "local_mid_plus3_ratio_delta_db",
    ] {
        let mut report = fixture();
        eligible(&mut report, &[12, 13, 14], "mid");
        for i in [12, 13, 14] {
            report["stems"]["bands"][i][field] = match field {
                "eligible_seconds" => json!(9.99),
                "vocal_excitation_share_percent" => json!(0.49),
                "vocal_to_accompaniment_p10_p50_p90_db" => json!([-2.0, 0.01, 2.0]),
                _ => json!(-0.5),
            };
        }
        assert_eq!(propose(&report).unwrap().proposals.len(), 1, "{field}");
    }
    let mut report = fixture();
    eligible(&mut report, &[12, 14], "mid");
    assert_eq!(
        propose(&report).unwrap().proposals.len(),
        1,
        "invalid intervening band breaks adjacency"
    );
}

#[test]
fn no_version_quota_and_combination_preserves_each_regions_basis() {
    let mut report = fixture();
    for group in [[8, 9], [13, 14], [18, 19], [23, 24]] {
        eligible(&mut report, &group, "mid");
    }
    let set = propose(&report).unwrap();
    assert_eq!(set.proposals.len(), 6); // unchanged, four regions, combined
    let combined = set
        .proposals
        .iter()
        .find(|p| p.id == "focus-combined")
        .unwrap();
    assert_eq!(combined.tone.mid.len(), 4);
    assert_eq!(combined.basis.len(), 4);
    let ids: std::collections::BTreeSet<_> = set.proposals.iter().map(|p| &p.id).collect();
    assert_eq!(ids.len(), set.proposals.len());
    assert_eq!(
        serde_json::to_value(&set).unwrap(),
        serde_json::to_value(propose(&report).unwrap()).unwrap()
    );
}

#[test]
fn unsafe_combination_does_not_erase_individual_opportunities() {
    let mut report = fixture();
    for first in (8..38).step_by(3) {
        eligible(&mut report, &[first, first + 1], "mid");
    }
    let set = propose(&report).unwrap();
    assert_eq!(set.proposals.len(), 11);
    assert!(set.proposals.iter().all(|p| p.id != "focus-combined"));
    assert!(set.notes.iter().any(|note| note.contains("bounds")));
}

#[test]
fn stale_sources_unreliable_stems_and_broken_band_grids_are_rejected() {
    let changes = [
        ("/files/Original/sha256", json!("b".repeat(64))),
        ("/files/Original/frames", json!(959999)),
        ("/files/Original/sample_rate", json!(44100)),
        ("/sources_unchanged", json!(false)),
        ("/schema", json!("mastering.auditory-analysis.99")),
        ("/stems/source_conversion_residual_db", json!(-34.9)),
        ("/stems/reconstruction/residual_relative_db", json!(-17.9)),
        ("/stems/reconstruction/cosine", json!(0.979)),
        ("/stems/active_seconds", json!(21.0)),
        ("/stems/stem_sha256/vocals", json!("not-a-hash")),
        ("/stems/bands/12/hz", json!(700.0)),
        ("/centres_hz/12", json!(700.0)),
        ("/stems/bands/12/eligible_seconds", json!(19.0)),
        ("/stems/bands/12/local_mid_plus3_ratio_delta_db", json!(4.0)),
        (
            "/stems/bands/12/vocal_to_accompaniment_p10_p50_p90_db",
            json!([0.0, -6.0, 2.0]),
        ),
    ];
    for (pointer, value) in changes {
        let mut report = fixture();
        eligible(&mut report, &[12, 13], "mid");
        *report.pointer_mut(pointer).unwrap() = value;
        assert!(propose(&report).is_err(), "{pointer}");
    }
    let mut report = fixture();
    report["stems"]["bands"].as_array_mut().unwrap().remove(12);
    assert!(propose(&report).is_err());
    let mut report = fixture();
    eligible(&mut report, &[12, 13], "mid");
    report["stems"]["bands"][12]
        .as_object_mut()
        .unwrap()
        .remove("eligible_seconds");
    assert!(propose(&report).is_err());
}

#[test]
fn paired_references_have_no_effect_on_source_proposals() {
    let mut report = fixture();
    eligible(&mut report, &[12, 13], "mid");
    let baseline = propose(&report).unwrap();
    report["files"]["Reference"] = json!({"sha256":"f".repeat(64),"sample_rate":48000,"frames":1000000,"mean_excitation":[999.0]});
    report["files"]["Original"]["mean_excitation"] = json!([500.0]);
    let changed = propose(&report).unwrap();
    assert_eq!(
        serde_json::to_value(baseline.proposals).unwrap(),
        serde_json::to_value(changed.proposals).unwrap()
    );
    assert_ne!(baseline.analysis_sha256, changed.analysis_sha256);
}
