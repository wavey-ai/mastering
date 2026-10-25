use mastering_engine::{
    audio_io::Stereo,
    dsp::{EqBand, Kind},
    finishing::Domain,
    refinement,
    source_treatments::{Policy, Proposal, ProposalSet},
    stem_evidence,
    treatment::{BlendPoint, FocusEnvelope, Tone},
};

fn material(mid_fraction: f64, decay: f64) -> (Vec<Stereo>, [Vec<Stereo>; 4]) {
    let stems = std::array::from_fn(|role| {
        (0..24 * 48000)
            .map(|i| {
                let t = i as f64 / 48000.0;
                let sine = |hz: f64| (std::f64::consts::TAU * hz * t).sin();
                let pulse = (-((i % 24000) as f64 / 48000.0) / decay).exp();
                match role {
                    0 => [(0.1 * pulse * sine(120.0)) as f32; 2],
                    1 => [(0.03 * sine(70.0)) as f32; 2],
                    2 => {
                        let m = 0.14 * mid_fraction * sine(690.0);
                        let s = 0.14 * (1.0 - mid_fraction) * sine(730.0);
                        [(m + s) as f32, (m - s) as f32]
                    }
                    _ => {
                        let level = if (8.0..12.0).contains(&t) || t >= 20.0 {
                            0.0
                        } else {
                            0.035
                        };
                        [(level * sine(710.0)) as f32; 2]
                    }
                }
            })
            .collect::<Vec<_>>()
    });
    let source = (0..stems[0].len())
        .map(|i| [0, 1].map(|c| stems.iter().map(|s| s[i][c]).sum()))
        .collect();
    (source, stems)
}

fn proposals() -> ProposalSet {
    ProposalSet {
        schema: "test",
        rule: "test",
        validation_status: "test",
        source_file_sha256: "a".repeat(64),
        analysis_sha256: "b".repeat(64),
        policy: Policy::default(),
        notes: Vec::new(),
        proposals: vec![Proposal {
            id: "focus".into(),
            rule: "test",
            rationale: "test",
            basis: Vec::new(),
            tone: Tone {
                mid: vec![EqBand {
                    kind: Kind::Peak,
                    hz: 700.0,
                    q: 1.0,
                    gain_db: 3.0,
                }],
                ..Tone::default()
            },
        }],
    }
}

#[test]
fn strength_and_passages_follow_measured_stem_distribution() {
    let mut strengths = Vec::new();
    for fraction in [0.1, 0.7] {
        let (source, stems) = material(fraction, 0.06);
        let before = stem_evidence::fingerprint(&source);
        let raw = stems.each_ref().map(Vec::as_slice);
        let evidence = stem_evidence::analyze(&source, raw, 48000, 48000).unwrap();
        let result =
            refinement::analyze(&source, raw, 48000, &evidence, Some(&proposals()), 48000).unwrap();
        let focus = &result.focus[0];
        assert_eq!(focus.probes.len(), 9);
        assert!((0.0..=6.0).contains(&focus.filter.gain_db));
        assert!(focus.selected_ratio_improvement_db.unwrap() > 0.1);
        assert!(focus.envelope.iter().any(|p| p.wet == 0.0));
        assert!(focus.envelope.iter().any(|p| p.wet > 0.5));
        assert!(focus
            .envelope
            .iter()
            .filter(|p| (21 * 48000..23 * 48000).contains(&p.sample))
            .all(|p| p.wet == 0.0));
        strengths.push(focus.filter.gain_db);
        assert_eq!(stem_evidence::fingerprint(&source), before);
        let (tone, envelopes, scale) = result.apply(&proposals().proposals[0].tone).unwrap();
        assert_eq!(scale, 1.0);
        assert_eq!(envelopes.len(), 1);
        let mut rendered = source.clone();
        tone.apply_with_envelopes(&mut rendered, &envelopes, 48000)
            .unwrap();
        assert!(rendered.iter().flatten().all(|x| x.is_finite()));
        assert!(rendered[21 * 48000..23 * 48000] == source[21 * 48000..23 * 48000]);
        assert!(rendered != source);
    }
    assert!(
        (strengths[0] - strengths[1]).abs() > 0.2,
        "spatial distribution must change strength: {strengths:?}"
    );
}

#[test]
fn timing_tracks_drum_decay_and_retains_study_timing_without_evidence() {
    let mut times = Vec::new();
    for decay in [0.025, 0.1] {
        let (source, stems) = material(0.3, decay);
        let mut evidence =
            stem_evidence::analyze(&source, stems.each_ref().map(Vec::as_slice), 48000, 48000)
                .unwrap();
        let timing = refinement::timing(&evidence);
        assert!(evidence.drum_decay_events >= 16);
        assert!((25.0..=160.0).contains(&timing.selected_release_ms));
        assert!(timing.selected_release_ms >= timing.bass_two_cycles_ms.unwrap());
        assert!(timing.selected_release_ms <= timing.measured_interval_ms.unwrap() * 0.5);
        times.push(timing.selected_release_ms);
        evidence.usable = false;
        assert_eq!(refinement::timing(&evidence).selected_release_ms, 50.0);
    }
    assert!(
        times[1] > times[0] + 20.0,
        "different decay evidence must change recovery: {times:?}"
    );
}

#[test]
fn stale_or_unreliable_stems_cannot_supply_refinements() {
    let (source, mut stems) = material(0.3, 0.06);
    let evidence =
        stem_evidence::analyze(&source, stems.each_ref().map(Vec::as_slice), 48000, 48000).unwrap();
    stems[3][0][0] += 0.001;
    assert!(refinement::analyze(
        &source,
        stems.each_ref().map(Vec::as_slice),
        48000,
        &evidence,
        Some(&proposals()),
        48000,
    )
    .is_err());
    for frame in &mut stems[3] {
        for sample in frame {
            *sample *= 3.0;
        }
    }
    let evidence =
        stem_evidence::analyze(&source, stems.each_ref().map(Vec::as_slice), 48000, 48000).unwrap();
    assert!(!evidence.usable);
    let result = refinement::analyze(
        &source,
        stems.each_ref().map(Vec::as_slice),
        48000,
        &evidence,
        Some(&proposals()),
        48000,
    )
    .unwrap();
    assert!(result.focus.is_empty());
    assert_eq!(result.timing.selected_release_ms, 50.0);
}

#[test]
fn passage_filters_preserve_dry_audio_and_validate_envelopes() {
    let (source, _) = material(0.3, 0.06);
    let tone = proposals().proposals.remove(0).tone;
    let envelope = FocusEnvelope {
        domain: Domain::Mid,
        filter_index: 0,
        points: vec![
            BlendPoint {
                sample: 0,
                wet: 0.0,
            },
            BlendPoint {
                sample: source.len() - 1,
                wet: 0.0,
            },
        ],
    };
    let mut audio = source.clone();
    tone.apply_with_envelopes(&mut audio, std::slice::from_ref(&envelope), 48000)
        .unwrap();
    assert!(audio == source);
    let mut invalid = envelope.clone();
    invalid.points[1].wet = f64::NAN;
    assert!(tone
        .apply_with_envelopes(&mut audio, &[invalid], 48000)
        .is_err());
    assert!(tone
        .apply_with_envelopes(&mut audio, &[envelope.clone(), envelope], 48000)
        .is_err());
}

#[test]
fn a_locally_unreliable_passage_cannot_keep_focus_active() {
    let (source, mut stems) = material(0.3, 0.06);
    for frame in &mut stems[2][..4800] {
        for sample in frame {
            *sample *= 2.0;
        }
    }
    let raw = stems.each_ref().map(Vec::as_slice);
    let evidence = stem_evidence::analyze(&source, raw, 48000, 48000).unwrap();
    assert!(
        evidence.usable,
        "global sum can pass despite local corruption"
    );
    let result =
        refinement::analyze(&source, raw, 48000, &evidence, Some(&proposals()), 48000).unwrap();
    assert!(!result.passages[0].reliable);
    assert!(result.passages[1].reliable);
    let envelope = &result.focus[0].envelope;
    assert!(!envelope.is_empty());
    assert_eq!(
        envelope.iter().find(|p| p.sample == 48000).unwrap().wet,
        0.0
    );
}
