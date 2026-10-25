use mastering_engine::{analysis, pipeline, stem_evidence};
use std::f64::consts::PI;

fn signal() -> Vec<[f32; 2]> {
    (0..4 * 48000)
        .map(|i| {
            let phase = i as f64 / 48000.0;
            let pulse = if i % 12000 < 1200 { 0.25 } else { 0.0 };
            let left = (0.15 * (2.0 * PI * 440.0 * phase).sin() + pulse) as f32;
            let right = (0.12 * (2.0 * PI * 440.0 * phase).sin() - pulse * 0.25) as f32;
            [left, right]
        })
        .collect()
}

#[test]
fn source_analysis_is_repeatable() {
    let input = signal();
    let source = pipeline::analyze_source(&input, 48000).unwrap();
    assert_eq!(source.source_frames, input.len());
    assert_eq!(source.source_pcm_sha256, stem_evidence::fingerprint(&input));
    assert_eq!(
        serde_json::to_value(&source).unwrap(),
        serde_json::to_value(pipeline::analyze_source(&input, 48000).unwrap()).unwrap()
    );
}

#[test]
fn changed_pcm_changes_the_source_fingerprint() {
    let input = signal();
    let source = pipeline::analyze_source(&input, 48000).unwrap();
    let mut changed = input.clone();
    changed[9000][0] += 0.001;
    assert_ne!(
        source.source_pcm_sha256,
        pipeline::analyze_source(&changed, 48000)
            .unwrap()
            .source_pcm_sha256
    );
}

#[test]
fn energy_attack_loss_counts_a_lowered_onset_and_not_a_clipped_peak() {
    // A 30 ms hit of a 200 Hz tone every 0.5 s over a quiet sustain.
    let hits: Vec<[f32; 2]> = (0..4 * 48000)
        .map(|i| {
            let level = if i % 24000 < 1440 { 0.8 } else { 0.1 };
            let x = (level * (2.0 * PI * 200.0 * i as f64 / 48000.0).sin()) as f32;
            [x, x]
        })
        .collect();
    assert_eq!(
        analysis::energy_attack_loss_p90_db(&hits, &hits, 48000),
        Some(0.0)
    );
    // A gain of -6 dB over each hit, as of a limiter.
    let lowered: Vec<[f32; 2]> = hits
        .iter()
        .enumerate()
        .map(|(i, f)| {
            if i % 24000 < 1440 {
                f.map(|x| x * 0.5)
            } else {
                *f
            }
        })
        .collect();
    let loss = analysis::energy_attack_loss_p90_db(&hits, &lowered, 48000).unwrap();
    assert!((loss - 6.02).abs() < 0.3, "{loss}");
    // A clip of the top 1 dB of each hit.
    let clipped: Vec<[f32; 2]> = hits
        .iter()
        .map(|f| f.map(|x| x.clamp(-0.71, 0.71)))
        .collect();
    assert!(analysis::energy_attack_loss_p90_db(&hits, &clipped, 48000).unwrap() < 1.0);
}
