use mastering_engine::{
    analysis::{self, Preservation},
    dsp::{self, LimiterStats},
    pipeline,
};
use std::f64::consts::PI;

fn signal(seconds: usize) -> Vec<[f32; 2]> {
    (0..seconds * 48000)
        .map(|i| {
            let phase = (i % 24000) as f64 / 48000.0;
            let amplitude = 0.008 + 0.75 * (-phase / 0.008).exp();
            let value = (amplitude * (2.0 * PI * 997.0 * i as f64 / 48000.0).sin()) as f32;
            [value, value * 0.8]
        })
        .collect()
}

#[test]
fn temporal_measurements_detect_attacks_and_are_gain_invariant() {
    let input = signal(12);
    let analyzed = analysis::analyze(&input, 48000);
    assert!(analyzed.events.len() >= 16);
    let attenuated: Vec<_> = input.iter().map(|x| [x[0] * 0.25, x[1] * 0.25]).collect();
    let change = analysis::compare(&analyzed, &attenuated, 48000);
    assert!(change.crest_loss_p90_db < 1e-5);
    assert!(change.attack_loss_p90_db.unwrap() < 1e-5);
    assert!(change.window_level_change_span_db < 1e-5);
    let damaged: Vec<_> = input
        .iter()
        .map(|x| [x[0].clamp(-0.05, 0.05), x[1].clamp(-0.05, 0.05)])
        .collect();
    let change = analysis::compare(&analyzed, &damaged, 48000);
    assert!(change.attack_loss_p90_db.unwrap() > 6.0);
}

#[test]
fn sustained_audio_is_not_labeled_as_onsets() {
    let input: Vec<_> = (0..48000 * 4)
        .map(|i| {
            let v = (0.1 * (2.0 * PI * 1000.0 * i as f64 / 48000.0).sin()) as f32;
            [v, v]
        })
        .collect();
    assert!(analysis::analyze(&input, 48000).events.is_empty());
}

#[test]
fn independent_preservation_failures_are_reported() {
    let bad = Preservation {
        crest_loss_p90_db: 10.0,
        attack_loss_p90_db: Some(10.0),
        event_count: 10,
        window_contrast_loss_db: 4.0,
        window_level_change_span_db: 5.0,
    };
    let stats = LimiterStats {
        maximum_reduction_db: 20.0,
        samples_reduced_percent: 100.0,
        frames_above_one_db_percent: 100.0,
        mean_reduction_db: 10.0,
        longest_run_above_one_db_ms: 4000.0,
    };
    let failures = pipeline::violations(&bad, Some(&stats), &pipeline::diagnostic_constraints());
    assert_eq!(failures.len(), 7);
}

#[test]
fn sustained_workload_is_rejected_even_without_large_peak_reduction() {
    let unchanged = Preservation {
        crest_loss_p90_db: 0.0,
        attack_loss_p90_db: Some(0.0),
        event_count: 10,
        window_contrast_loss_db: 0.0,
        window_level_change_span_db: 0.0,
    };
    let stats = LimiterStats {
        maximum_reduction_db: 3.0,
        mean_reduction_db: 2.0,
        frames_above_one_db_percent: 40.0,
        samples_reduced_percent: 100.0,
        longest_run_above_one_db_ms: 400.0,
    };
    let failures = pipeline::violations(
        &unchanged,
        Some(&stats),
        &pipeline::diagnostic_constraints(),
    );
    assert_eq!(failures, vec!["limit.mean_reduction"]);
}

#[test]
fn window_checks_detect_changed_long_term_balance() {
    let input: Vec<_> = (0..48000 * 36)
        .map(|i| {
            let a = if i < 48000 * 12 {
                0.02
            } else if i < 48000 * 24 {
                0.08
            } else {
                0.2
            };
            let v = (a * (2.0 * PI * 997.0 * i as f64 / 48000.0).sin()) as f32;
            [v, v]
        })
        .collect();
    let temporal = analysis::analyze(&input, 48000);
    let damaged: Vec<_> = input
        .iter()
        .enumerate()
        .map(|(i, x)| {
            let g = if i < 48000 * 12 {
                4.0
            } else if i < 48000 * 24 {
                1.0
            } else {
                0.4
            };
            [x[0] * g, x[1] * g]
        })
        .collect();
    let check = analysis::compare(&temporal, &damaged, 48000);
    assert!(check.window_contrast_loss_db > 10.0);
    assert!(check.window_level_change_span_db > 10.0);
}

#[test]
fn limiter_activity_uses_a_meaningful_one_db_threshold() {
    let mut audio = vec![[2.0, 1.0]; 19200];
    let stats = dsp::limit(&mut audio, 1.0, dsp::LimiterSettings::default(), 48000);
    assert!((stats.frames_above_one_db_percent - 100.0).abs() < 1e-6);
    assert!((stats.mean_reduction_db - 6.0206).abs() < 0.001);
    assert!((stats.longest_run_above_one_db_ms - 100.0).abs() < 1e-6);
}
