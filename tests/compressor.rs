use mastering_engine::{
    analysis,
    compressor::{self, Settings},
    dsp,
};
use std::f64::consts::PI;

fn settings() -> Settings {
    Settings {
        threshold_dbfs: -20.0,
        ratio: 2.0,
        knee_db: 4.0,
        attack_ms: 30.0,
        release_ms: 120.0,
        detector_rms_ms: 10.0,
        sidechain_highpass_hz: 0.0,
        maximum_reduction_db: 12.0,
        section_ms: 0.0,
        section_makeup: false,
    }
}

fn tone(hz: f64, seconds: usize) -> Vec<[f32; 2]> {
    (0..seconds * 48000)
        .map(|i| {
            let v = (0.5 * (2.0 * PI * hz * i as f64 / 48000.0).sin()) as f32;
            [v, v * 0.5]
        })
        .collect()
}

fn bursts() -> Vec<[f32; 2]> {
    let mut state = 431u64;
    (0..48000 * 5)
        .map(|i| {
            let mut noise = 0.0;
            for _ in 0..4 {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                noise += (state >> 32) as u32 as f64 / u32::MAX as f64 - 0.5;
            }
            let phase = (i % 24000) as f64 / 48000.0;
            let v = (noise * (0.015 + 0.2 * (-phase / 0.07).exp())) as f32;
            [v, v * 0.8]
        })
        .collect()
}

#[test]
fn knee_has_expected_ratio_continuity_and_cap() {
    let s = settings();
    assert_eq!(compressor::static_reduction_db(-23.0, s), 0.0);
    assert!((compressor::static_reduction_db(-20.0, s) - 0.25).abs() < 1e-10);
    assert!((compressor::static_reduction_db(-10.0, s) - 5.0).abs() < 1e-10);
    for level in [-22.0, -18.0] {
        assert!(
            (compressor::static_reduction_db(level - 1e-6, s)
                - compressor::static_reduction_db(level + 1e-6, s))
            .abs()
                < 1e-5
        );
    }
    assert_eq!(compressor::static_reduction_db(100.0, s), 12.0);
    let hard = Settings { knee_db: 0.0, ..s };
    assert_eq!(compressor::static_reduction_db(-20.0, hard), 0.0);
    assert_eq!(compressor::static_reduction_db(-10.0, hard), 5.0);
}

#[test]
fn bypass_and_below_threshold_are_exact_and_silence_is_finite() {
    let input = tone(997.0, 1);
    for s in [
        Settings {
            ratio: 1.0,
            ..settings()
        },
        Settings {
            maximum_reduction_db: 0.0,
            ..settings()
        },
        Settings {
            threshold_dbfs: 12.0,
            ..settings()
        },
    ] {
        let mut output = input.clone();
        let stats = compressor::process(&mut output, s, 48000).unwrap();
        assert_eq!(input, output);
        assert_eq!(stats.maximum_reduction_db, 0.0);
    }
    let mut silence = vec![[0.0; 2]; 48000];
    assert_eq!(
        compressor::process(&mut silence, settings(), 48000)
            .unwrap()
            .maximum_reduction_db,
        0.0
    );
    assert!(silence.iter().all(|f| *f == [0.0; 2]));
    assert_eq!(
        compressor::process(&mut [], settings(), 48000)
            .unwrap()
            .mean_reduction_db,
        0.0
    );
}

#[test]
fn linked_gain_preserves_channel_ratio_polarity_and_swap_symmetry() {
    let input = tone(997.0, 2);
    let mut output = input.clone();
    let stats = compressor::process(&mut output, settings(), 48000).unwrap();
    assert!(stats.mean_reduction_db > 4.0);
    assert!(output
        .iter()
        .zip(&input)
        .all(|(o, i)| o[1] == o[0] * 0.5 && o[0].abs() <= i[0].abs()));
    let mut swapped: Vec<_> = input.iter().map(|f| [f[1], f[0]]).collect();
    compressor::process(&mut swapped, settings(), 48000).unwrap();
    assert!(output.iter().zip(swapped).all(|(a, b)| *a == [b[1], b[0]]));
    let mut opposite: Vec<_> = input.iter().map(|f| [f[0], -f[0]]).collect();
    compressor::process(&mut opposite, settings(), 48000).unwrap();
    assert!(opposite.iter().all(|f| f[0] == -f[1]));
    let mut one_sided: Vec<_> = input.iter().map(|f| [f[0], 0.0]).collect();
    compressor::process(&mut one_sided, settings(), 48000).unwrap();
    assert!(one_sided.iter().all(|f| f[1] == 0.0));
}

#[test]
fn detector_highpass_reduces_bass_triggering_without_filtering_audio() {
    let input = tone(40.0, 3);
    let mut full = input.clone();
    let mut weighted = input.clone();
    let a = compressor::process(&mut full, settings(), 48000).unwrap();
    let b = compressor::process(
        &mut weighted,
        Settings {
            sidechain_highpass_hz: 120.0,
            ..settings()
        },
        48000,
    )
    .unwrap();
    assert!(a.mean_reduction_db > b.mean_reduction_db + 3.0);
    let mut bypass = input.clone();
    compressor::process(
        &mut bypass,
        Settings {
            threshold_dbfs: 12.0,
            sidechain_highpass_hz: 120.0,
            ..settings()
        },
        48000,
    )
    .unwrap();
    assert_eq!(input, bypass);
}

#[test]
fn steady_state_matches_the_transfer_curve_and_has_no_makeup_gain() {
    let input = vec![[0.5, 0.25]; 48000 * 3];
    let mut output = input.clone();
    compressor::process(&mut output, settings(), 48000).unwrap();
    let expected = 0.5 * dsp::gain(-compressor::static_reduction_db(dsp::db(0.5), settings()));
    assert!((output.last().unwrap()[0] as f64 - expected).abs() < 1e-6);
    assert!(output[0][0] > output[48000][0]);
    assert_eq!(input.len(), output.len());
}

#[test]
fn attack_and_release_follow_the_selected_time_constants() {
    let mut input = vec![[0.5; 2]; 48000];
    input.extend(vec![[0.01; 2]; 48000]);
    let mut fast = input.clone();
    let mut slow = input.clone();
    compressor::process(
        &mut fast,
        Settings {
            attack_ms: 1.0,
            release_ms: 30.0,
            ..settings()
        },
        48000,
    )
    .unwrap();
    compressor::process(
        &mut slow,
        Settings {
            attack_ms: 100.0,
            release_ms: 500.0,
            ..settings()
        },
        48000,
    )
    .unwrap();
    assert!(fast[2400][0] < slow[2400][0]);
    assert!(fast[60000][0] > slow[60000][0]);
    assert!(fast[95999][0] > 0.0099);
}

#[test]
fn invalid_inputs_fail_before_modifying_samples() {
    for s in [
        Settings {
            ratio: f64::NAN,
            ..settings()
        },
        Settings {
            attack_ms: 0.0,
            ..settings()
        },
        Settings {
            sidechain_highpass_hz: 24000.0,
            ..settings()
        },
        Settings {
            threshold_dbfs: f64::INFINITY,
            ..settings()
        },
    ] {
        let mut audio = vec![[0.25; 2]; 12];
        assert!(compressor::process(&mut audio, s, 48000).is_err());
        assert_eq!(audio, vec![[0.25; 2]; 12]);
    }
    let mut audio = vec![[0.25; 2], [f32::NAN, 0.0]];
    assert!(compressor::process(&mut audio, settings(), 48000).is_err());
    assert_eq!(audio[0], [0.25; 2]);
}

#[test]
fn reduction_is_bounded_and_extreme_finite_samples_remain_finite() {
    let mut audio = vec![[f32::MAX, -f32::MAX]; 48000];
    let stats = compressor::process(
        &mut audio,
        Settings {
            maximum_reduction_db: 1.5,
            ..settings()
        },
        48000,
    )
    .unwrap();
    assert!(stats.maximum_reduction_db <= 1.5 + 1e-10);
    assert!(audio.iter().flatten().all(|v| v.is_finite()));
}

#[test]
fn compressor_processing_is_repeatable_and_gain_covariant() {
    let input = bursts();
    let mut a = input.clone();
    let mut b = input.clone();
    compressor::process(&mut a, settings(), 48000).unwrap();
    compressor::process(&mut b, settings(), 48000).unwrap();
    assert_eq!(a, b);
    let mut quieter: Vec<_> = input.iter().map(|f| [f[0] * 0.5, f[1] * 0.5]).collect();
    compressor::process(
        &mut quieter,
        Settings {
            threshold_dbfs: settings().threshold_dbfs - dsp::db(2.0),
            ..settings()
        },
        48000,
    )
    .unwrap();
    assert!(a
        .iter()
        .zip(quieter)
        .all(|(x, y)| (x[0] * 0.5 - y[0]).abs() < 1e-7));
    assert!(analysis::analyze(&input, 48000).events.len() >= 8);
}

/// Noise bursts every 0.5 s; `level(second)` scales each second.
fn sections(seconds: usize, level: impl Fn(usize) -> f64) -> Vec<[f32; 2]> {
    let mut state = 977u64;
    (0..48000 * seconds)
        .map(|i| {
            let mut noise = 0.0;
            for _ in 0..4 {
                state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
                noise += (state >> 32) as u32 as f64 / u32::MAX as f64 - 0.5;
            }
            let phase = (i % 24000) as f64 / 48000.0;
            let v = (level(i / 48000) * noise * (0.05 + 0.3 * (-phase / 0.07).exp())) as f32;
            [v, v * 0.8]
        })
        .collect()
}

fn window_contrast(audio: &[[f32; 2]]) -> f64 {
    analysis::analyze(audio, 48000).window_contrast_db
}

fn energy_db(audio: &[[f32; 2]]) -> f64 {
    10.0 * audio
        .iter()
        .map(|f| (f[0] as f64).powi(2) + (f[1] as f64).powi(2))
        .sum::<f64>()
        .log10()
}

#[test]
fn mastering_compression_takes_the_depth_of_each_style_on_steady_material() {
    let audio = sections(72, |_| 1.0);
    let plan = compressor::plan(&audio, None, 48000).expect("steady material is compressed");
    assert_eq!(plan.rule, "set.compression.v3");
    assert_eq!(plan.attack_source, "punch");
    assert!(compressor::PUNCH_ATTACKS_MS.contains(&plan.punchy_attack_ms));
    let best = plan
        .punch_by_attack
        .iter()
        .map(|p| p[1])
        .fold(0.0, f64::max);
    // Punchy takes the slowest attack that keeps the share of the punch.
    for p in &plan.punch_by_attack {
        if p[0] > plan.punchy_attack_ms {
            assert!(p[1] < compressor::PUNCH_SHARE * best, "{p:?}");
        }
    }
    for style in compressor::Style::ALL {
        let chosen = plan.style(style);
        assert_eq!(chosen.timing_source, "default");
        assert!(chosen.settings.section_makeup);
        assert_eq!(chosen.settings.section_ms, compressor::MASTERING_SECTION_MS);
        assert_eq!(
            chosen.settings.attack_ms,
            style.attack_ms(plan.punchy_attack_ms)
        );
        assert!((chosen.mean_reduction_db - style.depth_db()).abs() < 0.05);
        // The search runs in steps of about 1 ms; the render agrees.
        let mut output = audio.clone();
        let stats = compressor::process(&mut output, chosen.settings, 48000).unwrap();
        assert!(
            (stats.mean_reduction_db - chosen.mean_reduction_db).abs() < 0.05,
            "{style:?} {} {}",
            stats.mean_reduction_db,
            chosen.mean_reduction_db
        );
        // The make-up keeps the level of the input.
        assert!((energy_db(&output) - energy_db(&audio)).abs() < 0.1);
        assert!(stats.mean_makeup_db > 0.5);
    }
    let attack = |s| plan.style(s).settings.attack_ms;
    use compressor::Style::*;
    assert!(attack(Gentle) >= attack(Punchy) && attack(Punchy) >= attack(Dense));
}

#[test]
fn section_makeup_keeps_the_contrast_of_the_sections() {
    // Quiet and loud 12 s sections, 12 dB apart.
    let audio = sections(96, |second| if (second / 12) % 2 == 0 { 0.25 } else { 1.0 });
    let before = window_contrast(&audio);
    assert!(before > 10.0);
    let plan = compressor::plan(&audio, None, 48000).expect("sections are compressed");
    for style in compressor::Style::ALL {
        let chosen = plan.style(style);
        let mut output = audio.clone();
        compressor::process(&mut output, chosen.settings, 48000).unwrap();
        // The 4 s window straddles each 12 dB step for 2 s, where the make-up
        // of the loud side reaches the quiet side.
        let change = window_contrast(&output) - before;
        assert!(change.abs() < 0.5, "{style:?} {change}");
        assert!((change - chosen.window_contrast_change_db).abs() < 0.1);
    }
}

#[test]
fn section_makeup_gives_back_what_the_reduction_removes() {
    // A loud section with hits beside a quiet steady section: the hits take
    // more reduction, and the make-up gives the section its level back.
    let mut audio = sections(30, |_| 1.0);
    for frame in audio.iter_mut().take(48000 * 12) {
        *frame = [0.02, 0.016];
    }
    let settings = Settings {
        threshold_dbfs: -3.0,
        ratio: 2.0,
        knee_db: 6.0,
        attack_ms: 20.0,
        release_ms: 200.0,
        detector_rms_ms: 10.0,
        sidechain_highpass_hz: 0.0,
        maximum_reduction_db: 6.0,
        section_ms: 4000.0,
        section_makeup: false,
    };
    let mut plain = audio.clone();
    compressor::process(&mut plain, settings, 48000).unwrap();
    let mut kept = audio.clone();
    let stats = compressor::process(
        &mut kept,
        Settings {
            section_makeup: true,
            ..settings
        },
        48000,
    )
    .unwrap();
    let range = |a: &[[f32; 2]], from: usize, to: usize| energy_db(&a[from * 48000..to * 48000]);
    // The middle of the loud section, away from the window edges.
    let (input, without, with) = (
        range(&audio, 16, 28),
        range(&plain, 16, 28),
        range(&kept, 16, 28),
    );
    assert!(input - without > 1.0, "{input} {without}");
    assert!((input - with).abs() < 0.1, "{input} {with}");
    assert!(stats.maximum_makeup_db > 1.0);
    // The make-up rises over the window, with no step.
    let largest_step = kept
        .windows(2)
        .zip(audio.windows(2))
        .filter(|(_, a)| a[0][0].abs() > 1e-3 && a[1][0].abs() > 1e-3)
        .map(|(k, a)| {
            ((k[1][0] / a[1][0]) as f64 / (k[0][0] / a[0][0]) as f64)
                .ln()
                .abs()
        })
        .fold(0.0, f64::max);
    assert!(largest_step < 0.01, "{largest_step}");
}

#[test]
fn a_span_with_context_takes_the_makeup_of_the_whole_recording() {
    let audio = sections(40, |second| if (second / 8) % 2 == 0 { 0.3 } else { 1.0 });
    let settings = compressor::plan(&audio, None, 48000)
        .unwrap()
        .style(compressor::Style::Punchy)
        .settings;
    let mut whole = audio.clone();
    compressor::process(&mut whole, settings, 48000).unwrap();
    let (before, after) = mastering_engine::excerpt::context(48000);
    let keep = 20 * 48000..24 * 48000;
    let span = keep.start - before..keep.end + after;
    let mut part = audio[span.clone()].to_vec();
    compressor::process(&mut part, settings, 48000).unwrap();
    let kept = &part[before..before + keep.len()];
    let difference = kept
        .iter()
        .zip(&whole[keep])
        .map(|(a, b)| (a[0] - b[0]).abs().max((a[1] - b[1]).abs()))
        .fold(0.0f32, f32::max);
    assert!(difference < 1e-6, "{difference}");
}

#[test]
fn mastering_compression_needs_active_windows() {
    assert!(compressor::plan(&sections(10, |_| 1.0), None, 48000).is_none());
    assert!(compressor::plan(&vec![[0.0f32; 2]; 48000 * 60], None, 48000).is_none());
}

#[test]
fn stem_timing_is_bounded_and_complete() {
    let timing = compressor::Timing::from_stems(Some(5.0), Some(1000.0), Some(30.0)).unwrap();
    assert_eq!(
        (
            timing.attack_ms,
            timing.release_ms,
            timing.detector_highpass_hz
        ),
        (10.0, 600.0, 40.0)
    );
    let timing = compressor::Timing::from_stems(Some(30.0), Some(340.0), Some(70.7)).unwrap();
    assert_eq!(
        (
            timing.attack_ms,
            timing.release_ms,
            timing.detector_highpass_hz
        ),
        (30.0, 340.0, 70.7)
    );
    assert!(compressor::Timing::from_stems(None, Some(340.0), Some(70.7)).is_none());
    let plan = compressor::plan(&sections(72, |_| 1.0), Some(timing), 48000).unwrap();
    for chosen in &plan.styles {
        assert_eq!(chosen.timing_source, "stems");
        assert_eq!(chosen.settings.sidechain_highpass_hz, 70.7);
        assert_eq!(chosen.settings.release_ms, 340.0);
    }
}

#[test]
fn style_attacks_follow_the_attack_of_punchy() {
    use compressor::Style::*;
    assert_eq!(Gentle.attack_ms(30.0), 60.0);
    assert_eq!(
        Gentle.attack_ms(100.0),
        compressor::GENTLE_ATTACK_MAXIMUM_MS
    );
    assert_eq!(Dense.attack_ms(30.0), 15.0);
    assert_eq!(Dense.attack_ms(10.0), compressor::DENSE_ATTACK_MINIMUM_MS);
    assert_eq!(Punchy.attack_ms(40.0), 40.0);
    assert!(Gentle.depth_db() < Punchy.depth_db() && Punchy.depth_db() < Dense.depth_db());
}

#[test]
fn section_threshold_compresses_loud_and_quiet_sections_alike() {
    // Quiet and loud 12 s sections, 12 dB apart.
    let audio = sections(96, |second| if (second / 12) % 2 == 0 { 0.25 } else { 1.0 });
    let plan = compressor::plan(&audio, None, 48000).expect("sections are compressed");
    let chosen = plan.style(compressor::Style::Punchy);
    // The 4 s section window straddles each 12 dB step for 2 s.
    assert!(
        chosen.mean_reduction_db >= 1.0,
        "{}",
        chosen.mean_reduction_db
    );
    // The reduction without the make-up, in the middle 8 s of each section.
    let mut output = audio.clone();
    compressor::process(
        &mut output,
        Settings {
            section_makeup: false,
            ..chosen.settings
        },
        48000,
    )
    .unwrap();
    let section_reduction = |start: usize| {
        let range = (start + 2) * 48000..(start + 10) * 48000;
        let (a, b): (f64, f64) = audio[range.clone()]
            .iter()
            .zip(&output[range])
            .map(|(x, y)| ((x[0] as f64).powi(2), (y[0] as f64).powi(2)))
            .fold((0.0, 0.0), |s, v| (s.0 + v.0, s.1 + v.1));
        10.0 * (a / b).log10()
    };
    let (quiet, loud) = (section_reduction(24), section_reduction(36));
    assert!(quiet > 1.0 && loud > 1.0, "{quiet} {loud}");
    assert!((quiet - loud).abs() < 0.3, "{quiet} {loud}");
}

#[test]
fn fixed_threshold_settings_are_unchanged_by_the_section_field() {
    let audio = bursts();
    let mut a = audio.clone();
    let mut b = audio.clone();
    let stats_a = compressor::process(&mut a, settings(), 48000).unwrap();
    let json = serde_json::to_value(settings()).unwrap();
    let mut without = json.as_object().unwrap().clone();
    without.remove("section_ms");
    let decoded: Settings = serde_json::from_value(without.into()).unwrap();
    assert_eq!(decoded.section_ms, 0.0);
    let stats_b = compressor::process(&mut b, decoded, 48000).unwrap();
    assert_eq!(a, b);
    assert_eq!(stats_a.mean_reduction_db, stats_b.mean_reduction_db);
    assert!(!decoded.section_makeup);
    let mut invalid = settings();
    invalid.section_ms = 500.0;
    assert!(invalid.validate().is_err());
    let mut invalid = settings();
    invalid.section_makeup = true;
    assert!(invalid.validate().is_err());
}
