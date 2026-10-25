use mastering_engine::{
    audio_io::Stereo,
    stereo::{self, BandMetrics, StereoAnalysis},
};
use std::f64::consts::PI;

const SR: usize = 48000;

fn tone(seconds: usize, hz: f64, right_gain: f64, right_phase: f64) -> Vec<Stereo> {
    (0..seconds * SR)
        .map(|i| {
            let phase = 2.0 * PI * hz * i as f64 / SR as f64;
            [
                (0.1 * phase.sin()) as f32,
                (0.1 * right_gain * (phase + right_phase).sin()) as f32,
            ]
        })
        .collect()
}

fn measured(audio: &[Stereo]) -> StereoAnalysis {
    stereo::analyze(audio, 48000).unwrap()
}

fn band(report: &StereoAnalysis, hz: f64) -> &BandMetrics {
    let index = report
        .band_edges_hz
        .windows(2)
        .position(|e| hz >= e[0] && hz < e[1])
        .unwrap();
    &report.whole.bands[index]
}

fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{actual} vs {expected}, tolerance {tolerance}"
    );
}

#[test]
fn mono_has_calibrated_power_without_fabricated_side_ratio() {
    let report = measured(&tone(3, 1000.0, 1.0, 0.0));
    let b = band(&report, 1000.0);
    assert!(b.active && b.left_present && b.right_present && b.mid_present);
    assert!(!b.side_present);
    close(b.left_power, 0.005, 0.00002);
    close(b.mid_power, 0.005, 0.00002);
    assert_eq!(b.side_power, 0.0);
    assert_eq!(b.side_mid_db, None);
    close(b.side_fraction.unwrap(), 0.0, 1e-12);
    close(b.correlation.unwrap(), 1.0, 1e-12);
    close(b.coherence.unwrap(), 1.0, 1e-12);
    close(b.cross_phase_degrees.unwrap(), 0.0, 1e-12);
    close(b.mono_fold_down_db.unwrap(), 0.0, 1e-12);
}

#[test]
fn opposite_polarity_is_coherent_but_disappears_in_mono() {
    let report = measured(&tone(3, 1000.0, -1.0, 0.0));
    let b = band(&report, 1000.0);
    assert!(b.active && b.side_present && !b.mid_present);
    close(b.correlation.unwrap(), -1.0, 1e-12);
    close(b.coherence.unwrap(), 1.0, 1e-12);
    close(b.cross_phase_degrees.unwrap().abs(), 180.0, 1e-12);
    assert_eq!(b.mono_fraction, Some(0.0));
    assert_eq!(b.mono_fold_down_db, None);
    assert_eq!(b.side_mid_db, None);
    let s = &report.short_summary[2];
    close(s.negative_correlation_window_seconds, 3.0, 1e-12);
    close(
        s.longest_negative_correlation_window_run_seconds,
        3.0,
        1e-12,
    );
    close(s.deep_mono_loss_window_seconds, 3.0, 1e-12);
}

#[test]
fn quadrature_and_delay_do_not_masquerade_as_decorrelation() {
    let report = measured(&tone(3, 1000.0, 1.0, -PI / 2.0));
    let b = band(&report, 1000.0);
    close(b.correlation.unwrap(), 0.0, 0.001);
    close(b.coherence.unwrap(), 1.0, 0.001);
    close(b.cross_phase_degrees.unwrap(), 90.0, 0.1);
    close(b.side_fraction.unwrap(), 0.5, 0.001);
    close(b.mono_fold_down_db.unwrap(), -3.0103, 0.01);

    let report = measured(&tone(3, 1000.0, 1.0, -PI));
    let b = band(&report, 1000.0);
    close(b.coherence.unwrap(), 1.0, 0.001);
    assert!(b.mono_fraction.unwrap() < 1e-8);
}

#[test]
fn single_channel_keeps_power_but_has_no_pair_statistics() {
    let report = measured(&tone(3, 1000.0, 0.0, 0.0));
    let b = band(&report, 1000.0);
    assert!(b.active && b.left_present && !b.right_present);
    close(b.mid_power, b.left_power / 4.0, 1e-12);
    close(b.side_power, b.left_power / 4.0, 1e-12);
    close(b.side_mid_db.unwrap(), 0.0, 1e-12);
    close(b.mono_fold_down_db.unwrap(), -3.0103, 0.0001);
    assert_eq!(b.correlation, None);
    assert_eq!(b.coherence, None);
    assert_eq!(b.cross_phase_degrees, None);
    assert_eq!(b.right_left_db, None);
}

#[test]
fn silent_and_subfloor_bands_remain_explicitly_unavailable() {
    for audio in [
        vec![[0.0; 2]; SR],
        tone(1, 1000.0, 1.0, 0.0)
            .into_iter()
            .map(|f| [f[0] * 1e-7, f[1] * 1e-7])
            .collect(),
    ] {
        let report = measured(&audio);
        for b in &report.whole.bands {
            assert!(!b.active);
            assert_eq!(b.correlation, None);
            assert_eq!(b.side_fraction, None);
            assert_eq!(b.coherence, None);
        }
        for summary in &report.short_summary {
            assert_eq!(summary.active_windows, 0);
            assert!(summary.correlation.is_none());
            assert_eq!(summary.worst_correlation_window_start_sample, None);
        }
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("NaN") && !json.contains("Infinity"));
    }
}

#[test]
fn channel_gain_and_swapping_have_known_results() {
    let audio = tone(3, 1000.0, 0.5, -PI / 3.0);
    let report = measured(&audio);
    let b = band(&report, 1000.0);
    close(b.right_left_db.unwrap(), -6.0206, 0.01);
    close(b.correlation.unwrap(), 0.5, 0.002);
    close(b.cross_phase_degrees.unwrap(), 60.0, 0.1);
    let swapped: Vec<_> = audio.iter().map(|f| [f[1], f[0]]).collect();
    let other = measured(&swapped);
    let other = band(&other, 1000.0);
    close(
        other.right_left_db.unwrap(),
        -b.right_left_db.unwrap(),
        1e-12,
    );
    close(
        other.cross_phase_degrees.unwrap(),
        -b.cross_phase_degrees.unwrap(),
        1e-12,
    );
    close(other.correlation.unwrap(), b.correlation.unwrap(), 1e-12);
    close(
        other.side_fraction.unwrap(),
        b.side_fraction.unwrap(),
        1e-12,
    );
    close(other.coherence.unwrap(), b.coherence.unwrap(), 1e-12);
    let quieter: Vec<_> = audio.iter().map(|f| [f[0] * 0.25, f[1] * 0.25]).collect();
    let quiet = measured(&quieter);
    let quiet = band(&quiet, 1000.0);
    close(quiet.left_power, b.left_power / 16.0, 1e-12);
    close(
        quiet.side_fraction.unwrap(),
        b.side_fraction.unwrap(),
        1e-12,
    );
    close(quiet.coherence.unwrap(), b.coherence.unwrap(), 1e-12);
}

#[test]
fn bass_and_treble_keep_separate_spatial_measurements() {
    let audio: Vec<_> = (0..3 * SR)
        .map(|i| {
            let t = i as f64 / SR as f64;
            let bass = 0.1 * (2.0 * PI * 60.0 * t).sin();
            let treble = 0.1 * (2.0 * PI * 8000.0 * t).sin();
            [(bass + treble) as f32, (bass - treble) as f32]
        })
        .collect();
    let report = measured(&audio);
    assert!(band(&report, 60.0).correlation.unwrap() > 0.999);
    assert!(band(&report, 8000.0).correlation.unwrap() < -0.999);
    assert!(band(&report, 60.0).side_fraction.unwrap() < 0.001);
    assert!(band(&report, 8000.0).side_fraction.unwrap() > 0.999);
}

#[test]
fn spatial_transitions_and_silence_break_negative_runs() {
    let mut audio = tone(2, 1000.0, -1.0, 0.0);
    audio.extend(vec![[0.0; 2]; 2 * SR]);
    audio.extend(tone(2, 1000.0, -1.0, 0.0));
    audio.extend(tone(2, 1000.0, 1.0, 0.0));
    let report = measured(&audio);
    assert_eq!(report.short_windows.len(), 16);
    assert_eq!(report.long_windows.len(), 3);
    let summary = &report.short_summary[2];
    assert!(summary.negative_correlation_window_seconds >= 4.0);
    assert!(summary.longest_negative_correlation_window_run_seconds <= 2.5);
    assert!(summary.correlation.as_ref().unwrap().min < -0.99);
    assert!(summary.correlation.as_ref().unwrap().max > 0.99);
    assert!(report.short_windows[7].bands[2]
        .correlation
        .is_some_and(|v| v < 0.0));
    assert!(!report.short_windows[5].bands[2].active);
}

fn noise(frames: usize) -> Vec<Stereo> {
    let mut state = 1979u64;
    let mut value = || {
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
        ((state >> 32) as u32 as f64 / u32::MAX as f64 * 0.2 - 0.1) as f32
    };
    (0..frames).map(|_| [value(), value()]).collect()
}

#[test]
fn independent_noise_has_low_coherence_after_time_averaging() {
    let report = measured(&noise(6 * SR));
    for b in &report.whole.bands {
        assert!(b.active);
        assert!(b.correlation.unwrap().abs() < 0.05);
        assert!(b.coherence.unwrap() < 0.12, "{b:?}");
        close(b.side_fraction.unwrap(), 0.5, 0.03);
    }
}

#[test]
fn narrow_and_wide_mid_side_sources_have_known_energy_ratios() {
    let original = noise(6 * SR);
    for side_gain in [0.1f64, 0.6, 1.4] {
        let audio: Vec<_> = original
            .iter()
            .map(|f| {
                let mid = f[0] as f64;
                let side = f[1] as f64 * side_gain;
                [(mid + side) as f32, (mid - side) as f32]
            })
            .collect();
        let report = measured(&audio);
        let b = band(&report, 1000.0);
        close(b.side_mid_db.unwrap(), 20.0 * side_gain.log10(), 0.15);
        close(
            b.side_fraction.unwrap(),
            side_gain.powi(2) / (1.0 + side_gain.powi(2)),
            0.01,
        );
        close(
            b.correlation.unwrap(),
            (1.0 - side_gain.powi(2)) / (1.0 + side_gain.powi(2)),
            0.02,
        );
    }
}

#[test]
fn delayed_noise_retains_bin_coherence_despite_phase_rotation() {
    let original = noise(6 * SR);
    let delay = 24;
    let audio: Vec<_> = original
        .iter()
        .enumerate()
        .map(|(i, f)| {
            [
                f[0],
                if i >= delay {
                    original[i - delay][0]
                } else {
                    0.0
                },
            ]
        })
        .collect();
    let report = measured(&audio);
    for b in &report.whole.bands {
        assert!(b.coherence.unwrap() > 0.99, "{b:?}");
    }
    assert!(band(&report, 8000.0).correlation.unwrap().abs() < 0.05);
    assert!(band(&report, 8000.0).phase_resultant.unwrap() < 0.1);
    assert_eq!(band(&report, 8000.0).cross_phase_degrees, None);
}

#[test]
fn boundaries_and_short_inputs_are_reported_without_padding_the_source() {
    assert!(stereo::analyze(&[], 48000).is_err());
    assert!(stereo::analyze(&[[f32::NAN, 0.0]], 48000).is_err());
    assert!(stereo::analyze(&[[0.0, f32::INFINITY]], 48000).is_err());
    let report = measured(&[[0.1, -0.1]]);
    assert_eq!(report.whole.spectral_frames, 1);
    assert_eq!(report.whole.padded_frames, 1);
    assert!(report.whole.bands.iter().all(|b| b.coherence.is_none()));
    for length in [24000, 24001, 144001] {
        let mut audio = vec![[0.0; 2]; length];
        audio[0] = [0.1, 0.1];
        audio[length - 1] = [0.1, -0.1];
        let copy = audio.clone();
        let report = measured(&audio);
        assert_eq!(audio, copy);
        assert_eq!(report.input_frames, length);
        assert_eq!(report.short_windows.len(), length.div_ceil(24000));
        assert_eq!(report.long_windows.len(), length.div_ceil(144000));
        assert_eq!(report.whole.support_start_sample, 0);
        assert_eq!(report.whole.support_end_sample, length);
        assert!(report.whole.bands.iter().any(|b| b.active));
        let mut end = 0;
        for w in &report.short_windows {
            assert_eq!(w.start_sample, end);
            assert!(w.end_sample - w.start_sample <= 24000);
            assert!(w.spectral_frames > 0);
            end = w.end_sample;
        }
        assert_eq!(end, length);
    }
}

#[test]
fn reports_are_repeatable_and_cover_finite_extreme_inputs() {
    let audio = tone(1, 1000.0, 0.75, 0.2);
    let a = serde_json::to_value(measured(&audio)).unwrap();
    let b = serde_json::to_value(measured(&audio)).unwrap();
    assert_eq!(a, b);
    let extreme = vec![[f32::MAX, -f32::MAX]; 100];
    let report = measured(&extreme);
    assert!(report.whole.bands.iter().all(|b| b.side_power.is_finite()));
    let json = serde_json::to_string(&report).unwrap();
    assert!(!json.contains("NaN") && !json.contains("Infinity"));
}

#[test]
fn a_presence_band_under_dominant_bass_is_material_by_its_weighted_share() {
    // A dark live recording: the 2–6 kHz band holds 0.2% of the bass band's
    // raw power, which the equal-loudness weighting lifts above 1%.
    let audio: Vec<Stereo> = (0..3 * SR)
        .map(|i| {
            let t = i as f64 / SR as f64;
            let x = 0.3 * (2.0 * PI * 70.0 * t).sin() + 0.015 * (2.0 * PI * 3000.0 * t).sin();
            [x as f32, x as f32]
        })
        .collect();
    let report = measured(&audio);
    let bass = band(&report, 70.0);
    let presence = band(&report, 3000.0);
    assert!(presence.left_power < 0.01 * bass.left_power);
    let weighted: f64 = report.whole.bands.iter().map(|b| b.weighted_power).sum();
    assert!(presence.weighted_power >= 0.01 * weighted);
    let material = mastering_engine::selection::material_bands(&report);
    assert!(material[0] && material[3]);
    assert!(!material[4] && !material[5]);
}
