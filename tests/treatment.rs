use mastering_engine::{
    dsp::{EqBand, Kind},
    finishing::{self, Domain},
    treatment::{measure_dynamic, DynamicProbe, Tone},
};

fn fixture(hz: f64, seconds: usize) -> Vec<[f32; 2]> {
    (0..48000 * seconds)
        .map(|i| {
            let x = (0.1 * (2.0 * std::f64::consts::PI * hz * i as f64 / 48000.0).sin()) as f32;
            [x, x * 0.4]
        })
        .collect()
}

#[test]
fn explicit_domain_treatments_preserve_the_other_component_and_identity() {
    let input = fixture(600.0, 2);
    let mut identity = input.clone();
    Tone::default().apply(&mut identity, 48000).unwrap();
    assert_eq!(identity, input);
    let band = EqBand {
        kind: Kind::Peak,
        hz: 600.0,
        q: 0.7,
        gain_db: 3.0,
    };
    for mid in [false, true] {
        let tone = if mid {
            Tone {
                mid: vec![band],
                ..Tone::default()
            }
        } else {
            Tone {
                side: vec![band],
                ..Tone::default()
            }
        };
        let mut output = input.clone();
        tone.apply(&mut output, 48000).unwrap();
        let (mut before, mut after) = (0.0, 0.0);
        for (a, b) in input[48000..].iter().zip(&output[48000..]) {
            let keep = if mid { -1.0 } else { 1.0 };
            assert!(((a[0] + keep * a[1]) - (b[0] + keep * b[1])).abs() < 3e-8);
            before += (a[0] - keep * a[1]).powi(2) as f64;
            after += (b[0] - keep * b[1]).powi(2) as f64;
        }
        assert!((10.0 * (after / before).log10() - 3.0).abs() < 0.01);
    }
}

#[test]
fn dynamic_calibration_tracks_gain_and_responds_to_loud_band_activity() {
    let mut input = fixture(90.0, 4);
    for (i, frame) in input.iter_mut().enumerate() {
        if i < 48000 * 2 {
            for x in frame {
                *x *= 0.1;
            }
        }
    }
    let spec = DynamicProbe {
        hz: 90.0,
        q: 0.8,
        domain: Domain::Stereo,
        maximum_cut_db: 3.0,
        threshold_percentile: 0.9,
        threshold_offset_db: -4.0,
    };
    let band = measure_dynamic(&input, spec, 48000).unwrap();
    let scaled: Vec<_> = input.iter().map(|a| a.map(|x| x * 2.0)).collect();
    let second = measure_dynamic(&scaled, spec, 48000).unwrap();
    assert!((second.dynamic_threshold_db - band.dynamic_threshold_db - 6.0206).abs() < 0.001);
    let original = input.clone();
    let stats = finishing::dynamic_band(&mut input, &band, 48000).unwrap();
    assert!(stats.maximum_reduction_db > 1.5 && stats.maximum_reduction_db <= 3.0);
    let quiet_error = original[24000..48000]
        .iter()
        .zip(&input[24000..48000])
        .flat_map(|(a, b)| a.iter().zip(b))
        .map(|(a, b)| (*a - *b).abs())
        .fold(0.0f32, f32::max);
    assert!(quiet_error < 1e-6);
}

#[test]
fn invalid_or_excessive_filters_fail_before_mutating_source() {
    let input = fixture(600.0, 1);
    for gain in [f64::NAN, 7.0] {
        let mut output = input.clone();
        let tone = Tone {
            mid: vec![EqBand {
                kind: Kind::Peak,
                hz: 600.0,
                q: 0.7,
                gain_db: gain,
            }],
            ..Tone::default()
        };
        assert!(tone.apply(&mut output, 48000).is_err());
        assert_eq!(input, output);
    }
    let tone = Tone {
        side: vec![
            EqBand {
                kind: Kind::Peak,
                hz: 600.0,
                q: 0.7,
                gain_db: 6.0
            };
            2
        ],
        ..Tone::default()
    };
    assert!(tone.validate(48000).is_err());
}
