use mastering_engine::{
    bench,
    dsp::{self, EqBand, Kind, Oversampler},
    finishing::{self, Band, Domain, GainPoint},
};

fn band() -> Band {
    Band {
        id: "test",
        hz: 1000.0,
        q: 0.8,
        domain: Domain::Stereo,
        ratio_p50_p95_db: [-12.0, -2.0],
        level_p50_p95_dbfs: [-30.0, -20.0],
        detector: finishing::Detector::RelativeBandPower,
        active_blocks: 100,
        static_cut_db: 0.0,
        dynamic_threshold_db: -12.0,
        maximum_dynamic_cut_db: 0.75,
        reason: "Synthetic fixture",
    }
}

#[test]
fn mid_and_side_eq_preserve_the_opposite_component() {
    let input: Vec<_> = bench::tone(1000.0, 0.3, 48000, 48000)
        .iter()
        .zip(bench::tone(3000.0, 0.1, 48000, 48000))
        .map(|(m, s)| [m[0] + s[0], m[0] - s[0]])
        .collect();
    let filters = [EqBand {
        kind: Kind::Peak,
        hz: 1000.0,
        q: 0.8,
        gain_db: -0.5,
    }];
    for domain in [Domain::Mid, Domain::Side] {
        let mut output = input.clone();
        finishing::eq_domain(&mut output, &filters, domain, 48000);
        for (a, b) in input.iter().zip(output) {
            let error = match domain {
                Domain::Mid => (a[0] - a[1]) - (b[0] - b[1]),
                _ => (a[0] + a[1]) - (b[0] + b[1]),
            };
            assert!(error.abs() < 1e-7);
        }
    }
}

#[test]
fn dynamic_band_is_bounded_linked_and_exact_when_bypassed() {
    let input: Vec<_> = bench::tone(1000.0, 0.5, 144000, 48000)
        .iter()
        .map(|x| [x[0], x[0] * 0.5])
        .collect();
    let mut bypass = input.clone();
    finishing::dynamic_band(
        &mut bypass,
        &Band {
            maximum_dynamic_cut_db: 0.0,
            ..band()
        },
        48000,
    )
    .unwrap();
    assert_eq!(input, bypass);
    let mut output = input.clone();
    let stats = finishing::dynamic_band(&mut output, &band(), 48000).unwrap();
    assert!(stats.maximum_reduction_db > 0.74 && stats.maximum_reduction_db <= 0.75);
    assert!(output.iter().all(|x| (x[1] - x[0] * 0.5).abs() < 1e-7));
    let reduction = dsp::db(
        bench::amplitude(&output[48000..96000], 1000.0, 48000).unwrap()
            / bench::amplitude(&input[48000..96000], 1000.0, 48000).unwrap(),
    );
    assert!((reduction + 0.75).abs() < 0.01);
    let mut scaled: Vec<_> = input.iter().map(|x| x.map(|s| s * 0.25)).collect();
    finishing::dynamic_band(&mut scaled, &band(), 48000).unwrap();
    assert!(scaled
        .iter()
        .flatten()
        .zip(output.iter().flatten())
        .all(|(a, b)| (*a - *b * 0.25).abs() < 1e-7));
}

#[test]
fn section_gain_is_continuous_bounded_and_preserves_silence() {
    let mut audio = vec![[0.5, 0.25]; 48000];
    let points = [
        GainPoint {
            sample: 0,
            gain_db: 0.0,
        },
        GainPoint {
            sample: 24000,
            gain_db: 0.25,
        },
        GainPoint {
            sample: 47999,
            gain_db: 0.0,
        },
    ];
    finishing::automate_gain(&mut audio, &points).unwrap();
    assert_eq!(audio[0], [0.5, 0.25]);
    assert_eq!(audio[47999], [0.5, 0.25]);
    assert!(audio.windows(2).all(|w| (w[0][0] - w[1][0]).abs() < 1e-5));
    assert!(audio
        .iter()
        .all(|f| f[0] <= 0.5 * dsp::gain(0.25) as f32 + 1e-7));
    let mut silence = vec![[0.0; 2]; 48000];
    finishing::automate_gain(&mut silence, &points).unwrap();
    assert!(silence.iter().flatten().all(|x| *x == 0.0));
    let before = audio.clone();
    assert!(finishing::automate_gain(
        &mut audio,
        &[GainPoint {
            sample: 0,
            gain_db: f64::NAN
        }]
    )
    .is_err());
    assert_eq!(audio, before);
}

#[test]
fn dc_cleanup_and_planned_bypass_are_measurable() {
    let mut input = bench::tone(1000.0, 0.1, 144000, 48000);
    for frame in &mut input {
        frame[0] += 0.02;
        frame[1] += 0.02;
    }
    let mut plan = finishing::Plan {
        rule: "test.dc",
        validation_status: "test",
        active: false,
        dc_filter_hz: Some(3.0),
        bands: Vec::new(),
        section_gain: Vec::new(),
        color_amount: 0.0,
        clipping_budget_db: 0.0,
    };
    let mut bypass = input.clone();
    finishing::process(&mut bypass, &plan, 48000).unwrap();
    assert_eq!(input, bypass);
    plan.active = true;
    finishing::process(&mut bypass, &plan, 48000).unwrap();
    let mean = bypass[48000..].iter().map(|x| x[0] as f64).sum::<f64>() / 96000.0;
    assert!(mean.abs() < 1e-5);
}

#[test]
fn nonlinear_stages_bypass_bound_peaks_and_swap_channels() {
    let input: Vec<_> = bench::tone(1000.0, 0.7, 48000, 48000)
        .iter()
        .zip(bench::tone(1700.0, 0.3, 48000, 48000))
        .map(|(a, b)| [a[0], b[0]])
        .collect();
    let mut bypass = input.clone();
    finishing::color(&mut bypass, 0.0).unwrap();
    finishing::clip(&mut bypass, 1.0, 0.0).unwrap();
    assert_eq!(input, bypass);
    let mut a = input.clone();
    let mut b: Vec<_> = input.iter().map(|x| [x[1], x[0]]).collect();
    finishing::color(&mut a, 0.015).unwrap();
    finishing::color(&mut b, 0.015).unwrap();
    let peak = a
        .iter()
        .flatten()
        .fold(0.0f64, |m, &x| m.max(x.abs() as f64));
    let stats = finishing::clip(&mut a, peak * dsp::gain(-0.35), 0.35).unwrap();
    finishing::clip(&mut b, peak * dsp::gain(-0.35), 0.35).unwrap();
    assert!(stats.maximum_reduction_db <= 0.3501 && stats.maximum_reduction_db > 0.0);
    assert!(a.iter().zip(b).all(|(a, b)| a[0] == b[1] && a[1] == b[0]));
    assert!(a
        .iter()
        .flatten()
        .zip(input.iter().flatten())
        .all(|(a, b)| a.abs() <= b.abs() + 1e-7));
    assert!(finishing::color(&mut a, f64::NAN).is_err());
}

#[test]
fn adaptive_limiter_preserves_ceiling_linking_and_has_distinct_recovery() {
    let mut input = bench::tone(1000.0, 0.05, 96000, 48000);
    for (i, x) in input.iter_mut().enumerate() {
        if (24000..33600).contains(&i) {
            x[0] *= 16.0;
        }
        x[1] = x[0] * 0.5;
    }
    let os = Oversampler::new(48000);
    let mut fixed = os.up(&input);
    let mut adaptive = fixed.clone();
    dsp::limit(&mut fixed, 0.4, dsp::LimiterSettings::default(), 48000);
    dsp::limit(
        &mut adaptive,
        0.4,
        dsp::LimiterSettings {
            slow_release_ms: 200.0,
            ..dsp::LimiterSettings::default()
        },
        48000,
    );
    assert!(adaptive.iter().flatten().all(|x| x.abs() <= 0.400001));
    assert!(adaptive.iter().all(|x| (x[1] - x[0] * 0.5).abs() < 1e-7));
    assert!(adaptive[140000..180000]
        .iter()
        .zip(&fixed[140000..180000])
        .any(|(a, b)| (a[0] - b[0]).abs() > 1e-4));
}

#[test]
fn absolute_band_control_reduces_loud_peaks_but_leaves_quiet_band_activity() {
    let settings = Band {
        detector: finishing::Detector::AbsoluteBandPower,
        dynamic_threshold_db: -25.0,
        maximum_dynamic_cut_db: 2.0,
        ..band()
    };
    let mut quiet = bench::tone(1000.0, 0.01, 144000, 48000);
    let original = quiet.clone();
    let stats = finishing::dynamic_band(&mut quiet, &settings, 48000).unwrap();
    assert_eq!(stats.maximum_reduction_db, 0.0);
    assert_eq!(quiet, original);
    let mut loud: Vec<_> = bench::tone(1000.0, 0.5, 144000, 48000)
        .iter()
        .map(|x| [x[0], x[0] * 0.5])
        .collect();
    let stats = finishing::dynamic_band(&mut loud, &settings, 48000).unwrap();
    assert!(stats.maximum_reduction_db > 1.99 && stats.maximum_reduction_db <= 2.0);
    assert!(loud.iter().all(|x| (x[1] - x[0] * 0.5).abs() < 1e-7));
}
