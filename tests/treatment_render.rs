use mastering_engine::{
    dsp::{self, EqBand, Kind},
    finishing,
    treatment::Tone,
    treatment_render::{Renderer, Spec},
};

#[test]
fn reuse_is_exact_and_changed_tone_limiter_or_ceiling_cannot_reuse_wrong_audio() {
    let input: Vec<_> = (0..144000)
        .map(|i| {
            let t = i as f64 / 48000.0;
            let x = (0.3
                * (std::f64::consts::TAU * 613.0 * t).sin()
                * (0.4 + 0.6 * (-((i % 6000) as f64) / 400.0).exp())) as f32;
            [x, x * 0.4]
        })
        .collect();
    let spec = Spec {
        phase_alignment: None,
        finishing: finishing::Plan {
            rule: "test.bypass",
            validation_status: "test",
            active: false,
            dc_filter_hz: None,
            bands: Vec::new(),
            section_gain: Vec::new(),
            color_amount: 0.0,
            clipping_budget_db: 0.0,
        },
        tone: Tone::default(),
        focus_envelopes: Vec::new(),
        side_highpass_hz: None,
        side_gain: None,
        deesser: None,
        low_mid: None,
        low_end_hold: None,
        mono_safe: Vec::new(),
        dynamic_after_eq: Vec::new(),
        compressor: None,
        limiter: dsp::LimiterSettings::default(),
        clipper_db: 0.0,
        ceiling_dbtp: -1.0,
        initial_drive_db: 12.0,
        target_lufs: -10.0,
        fixed_drive_db: Some(12.0),
        restore_peak_tone: false,
        restoration_db: [0.0; 6],
        true_peak: dsp::TruePeak::Trim,
    };
    let mut reused = Renderer::new(&input, 48000);
    let first = reused.render(&spec).unwrap();
    let memory = Renderer::with_scratch(&input, 48000, Box::new(mastering_engine::scratch::memory))
        .render(&spec)
        .unwrap();
    assert_eq!(first.audio, memory.audio);
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(&memory).unwrap()
    );
    let again = reused.render(&spec).unwrap();
    assert!(first.audio == again.audio);
    assert!(first.limiter.maximum_reduction_db > 1.0);
    assert_eq!(reused.work.rendered_probes, 1);
    assert_eq!(reused.work.reused_probes, 1);
    for change in 0..5 {
        let mut changed = spec.clone();
        match change {
            0 => changed.ceiling_dbtp = -0.5,
            1 => changed.limiter.release_ms = 120.0,
            2 => changed.limiter.attack_ms = 3.0,
            _ => changed.tone.mid.push(EqBand {
                kind: Kind::Peak,
                hz: 613.0,
                q: 0.7,
                gain_db: 3.0,
            }),
        }
        if change == 4 {
            changed
                .focus_envelopes
                .push(mastering_engine::treatment::FocusEnvelope {
                    domain: finishing::Domain::Mid,
                    filter_index: 0,
                    points: vec![
                        mastering_engine::treatment::BlendPoint {
                            sample: 0,
                            wet: 0.5,
                        },
                        mastering_engine::treatment::BlendPoint {
                            sample: input.len() - 1,
                            wet: 0.5,
                        },
                    ],
                });
        }
        let cached = reused.render(&changed).unwrap();
        let fresh = Renderer::without_probe_cache(&input, 48000)
            .render(&changed)
            .unwrap();
        assert!(
            cached.audio == fresh.audio,
            "cached and fresh audio must match"
        );
        assert_eq!(
            serde_json::to_value(&cached).unwrap(),
            serde_json::to_value(&fresh).unwrap()
        );
        assert!(
            cached.audio != first.audio,
            "changed active processing must change audio"
        );
    }
    assert_eq!(reused.work.prepared_prefixes, 3);
    assert_eq!(reused.work.rendered_probes, 6);
}

#[test]
fn local_true_peak_lowers_the_gain_only_near_its_overs() {
    // A clipped 10 kHz tone: the decimation removes its third harmonic, and
    // its true peak rises over the ceiling.
    let input: Vec<[f32; 2]> = (0..144_000)
        .map(|i| {
            let t = i as f64 / 48000.0;
            let since_hit = (i % 24_000) as f64 / 48000.0;
            let kick = 0.8 * (-since_hit / 0.06).exp() * (std::f64::consts::TAU * 55.0 * t).sin();
            let x = (kick + 0.25 * (std::f64::consts::TAU * 10_000.0 * t).sin()) as f32;
            [x, x * 0.9]
        })
        .collect();
    let mut spec = Spec {
        phase_alignment: None,
        finishing: finishing::Plan {
            rule: "test",
            validation_status: "test",
            active: false,
            dc_filter_hz: None,
            bands: Vec::new(),
            section_gain: Vec::new(),
            color_amount: 0.0,
            clipping_budget_db: 0.0,
        },
        tone: Tone::default(),
        focus_envelopes: Vec::new(),
        side_highpass_hz: None,
        side_gain: None,
        deesser: None,
        low_mid: None,
        low_end_hold: None,
        mono_safe: Vec::new(),
        dynamic_after_eq: Vec::new(),
        compressor: None,
        limiter: dsp::LimiterSettings::default(),
        clipper_db: 1.5,
        ceiling_dbtp: -0.5,
        initial_drive_db: 12.0,
        target_lufs: -8.0,
        fixed_drive_db: Some(12.0),
        restore_peak_tone: false,
        restoration_db: [0.0; 6],
        true_peak: dsp::TruePeak::Trim,
    };
    let trim = Renderer::new(&input, 48000).render(&spec).unwrap();
    spec.true_peak = dsp::TruePeak::Local;
    let local = Renderer::new(&input, 48000).render(&spec).unwrap();
    assert!(trim.true_peak.trim_db < -0.01, "{}", trim.true_peak.trim_db);
    assert!(local.true_peak.local_db > 0.0);
    assert!(local.true_peak.trim_db > trim.true_peak.trim_db);
    assert!(local.lufs > trim.lufs);
    for rendered in [&trim, &local] {
        assert!(rendered.peak_estimate_dbtp <= spec.ceiling_dbtp);
    }
}

#[test]
fn restoration_reports_the_local_rule_and_does_not_widen_the_change() {
    // Low bursts on every half second under steady upper tones, driven into
    // the limiter.
    let input: Vec<[f32; 2]> = (0..480_000)
        .map(|i| {
            let t = i as f64 / 48000.0;
            let since_hit = (i % 24_000) as f64 / 48000.0;
            let kick = 0.8 * (-since_hit / 0.06).exp() * (std::f64::consts::TAU * 55.0 * t).sin();
            let upper = 0.08 * (std::f64::consts::TAU * 3000.0 * t).sin()
                + 0.05 * (std::f64::consts::TAU * 700.0 * t).sin();
            let x = (kick + upper) as f32;
            [x, x * 0.9]
        })
        .collect();
    let spec = Spec {
        phase_alignment: None,
        finishing: finishing::Plan {
            rule: "test",
            validation_status: "test",
            active: false,
            dc_filter_hz: None,
            bands: Vec::new(),
            section_gain: Vec::new(),
            color_amount: 0.0,
            clipping_budget_db: 0.0,
        },
        tone: Tone::default(),
        focus_envelopes: Vec::new(),
        side_highpass_hz: None,
        side_gain: None,
        deesser: None,
        low_mid: None,
        low_end_hold: None,
        mono_safe: Vec::new(),
        dynamic_after_eq: Vec::new(),
        compressor: None,
        limiter: dsp::LimiterSettings::default(),
        clipper_db: 1.5,
        ceiling_dbtp: -0.5,
        initial_drive_db: 8.0,
        target_lufs: -8.0,
        fixed_drive_db: None,
        restore_peak_tone: true,
        restoration_db: [0.0; 6],
        true_peak: dsp::TruePeak::Trim,
    };
    let rendered = Renderer::new(&input, 48000).render(&spec).unwrap();
    assert!(rendered.limiter.maximum_reduction_db > 1.0);
    let restoration = rendered.restoration.expect("restoration report");
    assert_eq!(restoration.rule, "set.peak-tone-restoration.v4");
    assert!(restoration.renders >= 1 && restoration.renders <= 3);
    assert_eq!(restoration.window_samples, 16_384);
    for band in 0..6 {
        if restoration.material[band] {
            let (measured, delivered) = (
                restoration.measured_change_db[band],
                restoration.delivered_change_db[band],
            );
            assert!(
                delivered.abs() <= measured.abs().max(0.1) + 0.05,
                "band {band}: measured {measured} delivered {delivered}"
            );
        }
    }
    assert!((rendered.lufs - spec.target_lufs).abs() < 0.1);
}
