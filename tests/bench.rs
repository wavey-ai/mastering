use mastering_engine::{
    bench,
    dsp::{self, Oversampler},
    finishing,
};

#[test]
fn coherent_projection_has_known_amplitude_and_rejects_leakage() {
    let mut tone = bench::tone(1000.0, 0.5, 48000, 48000);
    for x in &mut tone {
        x[1] = -x[1];
    }
    assert!((bench::amplitude(&tone, 1000.0, 48000).unwrap() - 0.5).abs() < 1e-7);
    assert!(bench::amplitude(&tone[..47001], 1000.0, 48000).is_err());
    assert!(bench::amplitude(&tone, 24000.0, 48000).is_err());
    assert!(
        bench::harmonics(&tone, 1000.0, 48000)
            .unwrap()
            .thd_db
            .unwrap()
            < -120.0
    );
    assert!(
        bench::harmonics(&bench::tone(18000.0, 0.5, 48000, 48000), 18000.0, 48000)
            .unwrap()
            .thd_db
            .is_none()
    );
}

#[test]
fn harmonic_meter_matches_the_cubic_transfer_function() {
    let mut tone = bench::tone(1000.0, 0.5, 48000, 48000);
    let amount = 0.02;
    finishing::color(&mut tone, amount).unwrap();
    let fundamental = 0.5 * (1.0 - 3.0 * amount / 4.0);
    let third = 0.5 * amount / 4.0;
    assert!((bench::amplitude(&tone, 1000.0, 48000).unwrap() - fundamental).abs() < 1e-7);
    assert!((bench::amplitude(&tone, 3000.0, 48000).unwrap() - third).abs() < 1e-7);
    assert!(
        (bench::harmonics(&tone, 1000.0, 48000)
            .unwrap()
            .thd_db
            .unwrap()
            - dsp::db(third / fundamental))
        .abs()
            < 0.001
    );
}

#[test]
fn two_tone_intermodulation_and_modulation_sidebands_have_known_values() {
    let mut audio = bench::tone(1000.0, 0.2, 48000, 48000);
    for (a, b) in audio.iter_mut().zip(bench::tone(1700.0, 0.2, 48000, 48000)) {
        for c in 0..2 {
            a[c] += b[c];
            a[c] = a[c] - 0.1 * a[c].powi(3);
        }
    }
    let expected = 0.1 * 3.0 * 0.2f64.powi(3) / 4.0;
    for hz in [300.0, 2400.0, 3700.0, 4400.0] {
        assert!((bench::amplitude(&audio, hz, 48000).unwrap() - expected).abs() < 1e-7);
    }
    let mut am = bench::tone(4000.0, 0.5, 48000, 48000);
    for (i, x) in am.iter_mut().enumerate() {
        let g = 1.0 + 0.1 * (std::f64::consts::TAU * 50.0 * i as f64 / 48000.0).sin();
        for v in x {
            *v *= g as f32;
        }
    }
    for hz in [3950.0, 4050.0] {
        assert!((bench::amplitude(&am, hz, 48000).unwrap() - 0.025).abs() < 1e-7);
    }
}

#[test]
fn oversampling_reduces_a_known_cubic_alias_product() {
    let input = bench::tone(18000.0, 0.5, 48000, 48000);
    let mut native = input.clone();
    finishing::color(&mut native, 0.02).unwrap();
    let os = Oversampler::new(48000);
    let mut up = os.up(&input);
    finishing::color(&mut up, 0.02).unwrap();
    let output = os.down(&up);
    let native_alias = bench::amplitude(&native[12000..36000], 6000.0, 48000).unwrap();
    let oversampled_alias = bench::amplitude(&output[12000..36000], 6000.0, 48000).unwrap();
    assert!(dsp::db(oversampled_alias / native_alias) < -45.0);
}
