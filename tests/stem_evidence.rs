use mastering_engine::{audio_io::Stereo, set, stem_evidence};

#[test]
fn set_analysis_derives_focus_from_aligned_stems_without_external_json() {
    use mastering_engine::audio_io::Source;
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
    let source: Vec<_> = stems[2]
        .iter()
        .zip(&stems[3])
        .map(|(a, b)| [a[0] + b[0], a[1] + b[1]])
        .collect();
    let evidence = stem_evidence::analyze_set(
        &source,
        48000,
        |role| {
            let i = ["drums", "bass", "other", "vocals"]
                .iter()
                .position(|x| *x == role)
                .unwrap();
            Ok(Source {
                audio: std::mem::take(&mut stems[i]),
                sample_rate: rate,
            })
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
    use mastering_engine::{audio_io::Source, refinement, resample};
    for rate in [44100, 48000] {
        for silent in [false, true] {
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
                |role| {
                    let index = ["drums", "bass", "other", "vocals"]
                        .iter()
                        .position(|x| *x == role)
                        .unwrap();
                    roles.push(role);
                    Ok(Source {
                        audio: std::mem::take(&mut stems[index]),
                        sample_rate: rate,
                    })
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
