use mastering_engine::{audio_io::Stereo, pipeline, source_condition};
use rustfft::{num_complex::Complex, FftPlanner};

const RATE: f64 = 48000.0;
const BLOCK: usize = 1 << 16;

/// Ten seconds of noise with `gain(hz)` as its magnitude response, built in
/// the frequency domain with random phases and repeated.
fn noise(gain: impl Fn(f64) -> f64, seed: u32) -> Vec<f64> {
    let mut state = seed;
    let mut random = || {
        state ^= state << 13;
        state ^= state >> 17;
        state ^= state << 5;
        state as f64 / u32::MAX as f64
    };
    let mut spectrum = vec![Complex::new(0.0, 0.0); BLOCK];
    for k in 1..BLOCK / 2 {
        let hz = k as f64 * RATE / BLOCK as f64;
        let phase = 2.0 * std::f64::consts::PI * random();
        let value = Complex::from_polar(gain(hz), phase);
        spectrum[k] = value;
        spectrum[BLOCK - k] = value.conj();
    }
    FftPlanner::new()
        .plan_fft_inverse(BLOCK)
        .process(&mut spectrum);
    let block: Vec<f64> = spectrum.iter().map(|c| c.re).collect();
    let peak = block.iter().fold(0.0f64, |m, x| m.max(x.abs()));
    (0..10 * RATE as usize)
        .map(|i| 0.3 * block[i % BLOCK] / peak)
        .collect()
}

fn stereo(left: &[f64], right: &[f64]) -> Vec<Stereo> {
    left.iter()
        .zip(right)
        .map(|(l, r)| [*l as f32, *r as f32])
        .collect()
}

/// `x` with a gain of `db`.
fn gain(x: &[f64], db: f64) -> Vec<f64> {
    x.iter().map(|v| v * 10f64.powf(db / 20.0)).collect()
}

/// Pink-like noise: power falls 3 dB per octave.
fn pink(hz: f64) -> f64 {
    1.0 / hz.max(20.0).sqrt()
}

#[test]
fn the_dc_offset_of_each_channel_is_reported() {
    let left = noise(pink, 3);
    let right = noise(pink, 5);
    // The left channel carries an offset of 0.01, −40 dBFS; the right none.
    let offset: Vec<f64> = left.iter().map(|x| x + 0.01).collect();
    let input = stereo(&offset, &right);
    let source = pipeline::analyze_source(&input, RATE as u32).unwrap();
    let condition = source_condition::measure(&input, &source, false, None);
    assert!(
        (condition.dc_offset_dbfs[0] + 40.0).abs() < 0.2,
        "{:?}",
        condition.dc_offset_dbfs
    );
    assert!(
        condition.dc_offset_dbfs[1] < -60.0,
        "{:?}",
        condition.dc_offset_dbfs
    );
    assert_eq!(
        source_condition::dc_offset_dbfs(&[[0.0, 0.0]; 4]),
        [source_condition::DC_FLOOR_DBFS; 2]
    );
}

#[test]
fn a_codec_low_pass_is_found_and_a_gentle_roll_off_is_not() {
    let cut = noise(|hz| if hz < 16000.0 { pink(hz) } else { 0.0 }, 7);
    let audio = stereo(&cut, &cut);
    let found = source_condition::bandwidth_hz(&audio, 48000).expect("a brick-wall cutoff");
    assert!((found - 16000.0).abs() < 1000.0, "{found}");

    // A 12 dB per octave roll-off from 10 kHz, as a mix filter.
    let gentle = noise(|hz| pink(hz) / (1.0 + (hz / 10000.0).powi(2)), 7);
    assert_eq!(
        source_condition::bandwidth_hz(&stereo(&gentle, &gentle), 48000),
        None
    );
}

#[test]
fn a_mono_source_and_anti_phase_highs_are_reported() {
    let x = noise(pink, 11);
    let mono = stereo(&x, &x);
    let source = pipeline::analyze_source(&mono, 48000).unwrap();
    let condition = source_condition::measure(&mono, &source, false, None);
    assert!(condition.mono);
    assert!(condition.antiphase_bands.is_empty());

    // The right channel inverts the content above 6 kHz.
    let low = noise(|hz| if hz < 6000.0 { pink(hz) } else { 0.0 }, 13);
    let high = noise(|hz| if hz >= 6000.0 { pink(hz) } else { 0.0 }, 17);
    let left: Vec<f64> = low.iter().zip(&high).map(|(l, h)| l + h).collect();
    let right: Vec<f64> = low.iter().zip(&high).map(|(l, h)| l - h).collect();
    let inverted = stereo(&left, &right);
    let source = pipeline::analyze_source(&inverted, 48000).unwrap();
    let condition = source_condition::measure(&inverted, &source, false, None);
    assert!(!condition.mono);
    assert_eq!(condition.antiphase_bands, vec![4, 5]);
}

#[test]
fn phase_alignment_turns_inverted_highs_back_and_keeps_the_magnitudes() {
    use mastering_engine::{stereo, stereo_phase};
    let low = noise(|hz| if hz < 6000.0 { pink(hz) } else { 0.0 }, 13);
    let high = noise(|hz| if hz >= 6000.0 { pink(hz) } else { 0.0 }, 17);
    let left: Vec<f64> = low.iter().zip(&high).map(|(l, h)| l + h).collect();
    let right: Vec<f64> = low.iter().zip(&high).map(|(l, h)| l - h).collect();
    let mut audio = stereo(&left, &right);
    let before = stereo::analyze(&audio, 48000).unwrap();
    let alignment = stereo_phase::measure(&audio, 48000).expect("inverted highs");
    alignment.validate().unwrap();
    alignment.apply(&mut audio, 48000);
    let after = stereo::analyze(&audio, 48000).unwrap();
    for i in [4, 5] {
        assert!(before.whole.bands[i].correlation.unwrap() < -0.9);
        assert!(after.whole.bands[i].correlation.unwrap() > 0.9);
    }
    for (a, b) in before.whole.bands.iter().zip(&after.whole.bands) {
        if a.left_power > 1e-9 {
            assert!((10.0 * (b.left_power / a.left_power).log10()).abs() < 0.2);
            assert!((10.0 * (b.right_power / a.right_power).log10()).abs() < 0.2);
        }
    }
    // The low band keeps its phase.
    assert!(after.whole.bands[1].correlation.unwrap() > 0.99);
}

#[test]
fn phase_alignment_leaves_in_phase_and_uncorrelated_stereo_alone() {
    use mastering_engine::stereo_phase;
    let x = noise(pink, 11);
    assert!(stereo_phase::measure(&stereo(&x, &x), 48000).is_none());
    let y = noise(pink, 29);
    assert!(stereo_phase::measure(&stereo(&x, &y), 48000).is_none());
}

#[test]
fn a_device_recording_counts_as_captured_and_a_full_band_stereo_mix_does_not() {
    let condition = |audio: &[Stereo]| {
        let source = pipeline::analyze_source(audio, 48000).unwrap();
        source_condition::measure(audio, &source, false, None)
    };
    let x = noise(pink, 11);
    let y = noise(pink, 29);
    assert!(condition(&stereo(&x, &x)).capture());
    assert!(condition(&stereo(&x, &x)).device_capture());
    assert!(!condition(&stereo(&x, &y)).capture());
    assert!(!condition(&stereo(&x, &y)).device_capture());
    let cut = noise(|hz| if hz < 16000.0 { pink(hz) } else { 0.0 }, 31);
    let cut_right = noise(|hz| if hz < 16000.0 { pink(hz) } else { 0.0 }, 37);
    // A codec low-pass at −14.6 LUFS, the level of a mix: a lossy copy of a
    // studio mix.
    let lossy = condition(&stereo(&gain(&cut, 6.0), &gain(&cut_right, 6.0)));
    assert!(lossy.capture());
    assert!(!lossy.input_quiet);
    assert!(!lossy.device_capture());
    // The same recording 30 dB down: a quiet device recording.
    let quiet = |x: &[f64]| {
        x.iter()
            .map(|v| v * 10f64.powf(-30.0 / 20.0))
            .collect::<Vec<_>>()
    };
    let device = condition(&stereo(&quiet(&cut), &quiet(&cut_right)));
    assert!(device.input_quiet);
    assert!(device.device_capture());
}

#[test]
fn a_reported_condition_gives_the_device_capture_of_the_condition() {
    let condition = |audio: &[Stereo]| {
        let source = pipeline::analyze_source(audio, 48000).unwrap();
        source_condition::measure(audio, &source, false, None)
    };
    let x = noise(pink, 11);
    let y = noise(pink, 29);
    let cut = noise(|hz| if hz < 16000.0 { pink(hz) } else { 0.0 }, 31);
    let cut_right = noise(|hz| if hz < 16000.0 { pink(hz) } else { 0.0 }, 37);
    let quiet = |x: &[f64]| {
        x.iter()
            .map(|v| v * 10f64.powf(-30.0 / 20.0))
            .collect::<Vec<_>>()
    };
    for audio in [
        stereo(&x, &x),
        stereo(&x, &y),
        stereo(&cut, &cut_right),
        stereo(&quiet(&cut), &quiet(&cut_right)),
    ] {
        let measured = condition(&audio);
        let report = serde_json::to_value(&measured).unwrap();
        assert_eq!(
            source_condition::reported_device_capture(&report),
            measured.device_capture()
        );
    }
}
