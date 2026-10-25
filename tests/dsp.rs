use mastering_engine::{audio_io, dsp::*, spectrum};
use std::{
    f64::consts::PI,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicUsize, Ordering},
};

fn tone(hz: f64, amplitude: f64, frames: usize) -> Vec<audio_io::Stereo> {
    (0..frames)
        .map(|i| {
            let x = (amplitude * (2.0 * PI * hz * i as f64 / 48000.0).sin()) as f32;
            [x, x]
        })
        .collect()
}

fn close(a: f64, b: f64, tolerance: f64) {
    assert!(
        (a - b).abs() <= tolerance,
        "{a} vs {b}, tolerance {tolerance}"
    );
}

#[test]
fn loudness_calibration_and_gain() {
    let audio = tone(1000.0, 0.1, 144000);
    close(loudness(&audio, 48000).unwrap(), -20.0, 0.06);
    let doubled: Vec<_> = audio.iter().map(|x| [x[0] * 2.0, x[1] * 2.0]).collect();
    close(
        loudness(&doubled, 48000).unwrap() - loudness(&audio, 48000).unwrap(),
        db(2.0),
        1e-7,
    );
    assert!(loudness(&vec![[0.0; 2]; 144000], 48000).is_none());
    assert!(loudness(&tone(1000.0, 1e-6, 144000), 48000).is_none());
}

#[test]
fn loudness_relative_gate_excludes_quiet_sections() {
    let loud = tone(1000.0, 0.1, 480000);
    let mut joined = loud.clone();
    joined.extend(tone(1000.0, 0.001, 480000));
    close(
        loudness(&joined, 48000).unwrap(),
        loudness(&loud, 48000).unwrap(),
        0.12,
    );
}

#[test]
fn filter_response_and_render_agree() {
    let band = EqBand {
        kind: Kind::Peak,
        hz: 1800.0,
        q: 0.707,
        gain_db: -3.0,
    };
    close(band.filter(48000).response_db(1800.0, 48000), -3.0, 1e-9);
    let mut audio = tone(1800.0, 0.1, 48000);
    let before: f64 = audio[24000..].iter().map(|x| (x[0] as f64).powi(2)).sum();
    eq(&mut audio, &[band], 48000);
    let after: f64 = audio[24000..].iter().map(|x| (x[0] as f64).powi(2)).sum();
    close(db((after / before).sqrt()), -3.0, 1e-5);
    for (kind, pass, stop) in [
        (Kind::LowShelf, 1.0, 23000.0),
        (Kind::HighShelf, 23000.0, 1.0),
    ] {
        let filter = EqBand {
            kind,
            hz: 1000.0,
            q: std::f64::consts::FRAC_1_SQRT_2,
            gain_db: 3.0,
        }
        .filter(48000);
        close(filter.response_db(pass, 48000), 3.0, 1e-5);
        close(filter.response_db(stop, 48000), 0.0, 1e-5);
    }
}

#[test]
fn width_preserves_mono_and_mid() {
    let filters = [EqBand {
        kind: Kind::HighShelf,
        hz: 200.0,
        q: std::f64::consts::FRAC_1_SQRT_2,
        gain_db: 3.0,
    }];
    let original = tone(1000.0, 0.1, 48000);
    let mut mono = original.clone();
    side_eq(&mut mono, &filters, 48000);
    assert_eq!(mono, original);
    let mut stereo: Vec<_> = original.iter().map(|x| [x[0], x[1] * 0.5]).collect();
    let mid: Vec<_> = stereo.iter().map(|x| (x[0] + x[1]) * 0.5).collect();
    side_eq(&mut stereo, &filters, 48000);
    for (x, m) in stereo.iter().zip(mid) {
        close(((x[0] + x[1]) * 0.5) as f64, m as f64, 2e-8);
    }
}

#[test]
fn oversampling_preserves_dc_length_and_impulse_time() {
    let oversampler = Oversampler::new(48000);
    let dc = vec![[0.1, -0.2]; 1024];
    let up = oversampler.up(&dc);
    assert_eq!(up.len(), 4096);
    for x in &up[256..3840] {
        close(x[0] as f64, 0.1, 1e-7);
        close(x[1] as f64, -0.2, 1e-7);
    }
    let down = oversampler.down(&up);
    assert_eq!(down.len(), dc.len());
    close(down[512][0] as f64, 0.1, 1e-7);
    let mut impulse = vec![[0.0; 2]; 1024];
    impulse[512] = [1.0; 2];
    let reconstructed = oversampler.down(&oversampler.up(&impulse));
    let index = reconstructed
        .iter()
        .enumerate()
        .max_by(|a, b| a.1[0].total_cmp(&b.1[0]))
        .unwrap()
        .0;
    assert_eq!(index, 512);
}

#[test]
fn oversampling_passband_and_intersample_peaks() {
    let oversampler = Oversampler::new(48000);
    for hz in [40.0, 1000.0, 10000.0, 20000.0] {
        let audio = tone(hz, 0.2, 4800);
        let out = oversampler.down(&oversampler.up(&audio));
        let energy = |x: &[[f32; 2]]| {
            x[128..4672]
                .iter()
                .map(|v| (v[0] as f64).powi(2))
                .sum::<f64>()
        };
        close(db((energy(&out) / energy(&audio)).sqrt()), 0.0, 0.04);
    }
    let audio: Vec<_> = (0..4096)
        .map(|i| {
            let x = (PI * 0.5 * i as f64 + PI * 0.25).sin() as f32;
            [x, x]
        })
        .collect();
    let sample_peak = audio
        .iter()
        .map(|x| x[0].abs() as f64)
        .fold(0.0f64, f64::max);
    assert!(sample_peak < 0.71);
    assert!(oversampler.peak(&audio) > 0.99);
}

/// True-peak check of the 4x estimate on sines whose true peak is known.
/// A sine at `4 fs / N` for a whole N has its crests at the same positions
/// against the 4x points in each period. Over the positions within one 4x
/// step, the estimate reads at most 0.001 dB above the true peak, and at
/// most 0.001 dB under the lowest reading of an ideal 4x interpolator:
/// `cos(pi / N)` for an even N, a crest midway between two points, and
/// `cos(pi / 2N)` for an odd N, whose positive and negative crests are half
/// a step apart. At 19.2 kHz and 48 kHz (N = 10) that reading is 0.436 dB
/// under the true peak.
#[test]
fn peak_estimate_reads_sines_within_the_bound_of_its_points() {
    for rate in [44100u32, 48000] {
        let oversampler = Oversampler::new(rate);
        for n in [10usize, 11, 12, 13, 16, 24, 48, 200] {
            let period = n as f64 / 4.0;
            let (mut low, mut high) = (f64::INFINITY, f64::NEG_INFINITY);
            for step in 0..16 {
                let offset = 0.25 * step as f64 / 16.0;
                let frames = rate as usize / 2;
                let fade = 4096.0;
                let audio: Vec<audio_io::Stereo> = (0..frames)
                    .map(|i| {
                        let t = i as f64;
                        let edge = t.min((frames - 1) as f64 - t);
                        let gain = if edge < fade {
                            0.5 - 0.5 * (PI * edge / fade).cos()
                        } else {
                            1.0
                        };
                        let x = (0.5 * gain * (2.0 * PI * (t - offset) / period).cos()) as f32;
                        [x, x]
                    })
                    .collect();
                let error = db(oversampler.peak(&audio)) - db(0.5);
                low = low.min(error);
                high = high.max(error);
            }
            let ideal = db((PI / (n * (1 + n % 2)) as f64).cos());
            assert!(
                high <= 0.001,
                "{rate} Hz, N {n}: {high} dB over the true peak"
            );
            assert!(
                low >= ideal - 0.001,
                "{rate} Hz, N {n}: {low} dB against the ideal 4x reading {ideal} dB"
            );
        }
    }
}

#[test]
fn limiter_is_linked_and_respects_oversampled_ceiling() {
    let mut audio: Vec<_> = tone(1000.0, 1.5, 12000)
        .iter()
        .map(|x| [x[0], x[0] * 0.5])
        .collect();
    let stats = limit(&mut audio, 0.8, LimiterSettings::default(), 48000);
    assert!(stats.maximum_reduction_db > 5.0);
    for x in audio {
        assert!(x[0].abs() <= 0.800001);
        close(x[1] as f64, x[0] as f64 * 0.5, 1e-7);
    }
}

#[test]
fn limiter_attack_lets_the_start_of_a_short_over_pass() {
    // At the oversampled rate: overs of 0.5 ms and 30 ms, 6 dB over the
    // ceiling.
    let signal = |over: std::ops::Range<usize>| -> Vec<audio_io::Stereo> {
        (0..40_000)
            .map(|i| {
                let level = if over.contains(&i) { 2.0 } else { 0.2 };
                let x = if i % 2 == 0 { level } else { -level };
                [x, x]
            })
            .collect()
    };
    let attack = |attack_ms: f64| LimiterSettings {
        attack_ms,
        ..LimiterSettings::default()
    };
    let peak = |audio: &[audio_io::Stereo], range: std::ops::Range<usize>| {
        audio[range]
            .iter()
            .map(|x| x[0].abs() as f64)
            .fold(0.0, f64::max)
    };
    let short = 10_000..10_096;
    let mut instant = signal(short.clone());
    limit(&mut instant, 1.0, attack(0.0), 48000);
    assert!(peak(&instant, 0..40_000) <= 1.000001);
    let mut slow = signal(short.clone());
    limit(&mut slow, 1.0, attack(3.0), 48000);
    assert!(peak(&slow, short) > gain(3.0));
    // A sustained over reaches the full reduction.
    let mut sustained = signal(10_000..15_760);
    limit(&mut sustained, 1.0, attack(3.0), 48000);
    assert!(peak(&sustained, 14_800..15_760) <= gain(0.05));
}

#[test]
fn local_true_peak_correction_lowers_only_the_frames_near_an_over() {
    let os = Oversampler::new(48000);
    // Samples of a quarter-rate tone reach 71% of its true peak.
    let burst = 20_000..20_200;
    let mut audio: Vec<audio_io::Stereo> = (0..48_000)
        .map(|i| {
            let x = if burst.contains(&i) {
                0.9 * (PI * 0.5 * i as f64 + PI * 0.25).sin()
            } else {
                0.3 * (2.0 * PI * 1000.0 * i as f64 / 48000.0).sin()
            };
            [x as f32, x as f32]
        })
        .collect();
    let original = audio.clone();
    assert!(os.peak(&audio) > 0.85);
    let reduction = os.lower_peaks(&mut audio, 0.8);
    assert!(os.peak(&audio) <= 0.8 * gain(0.01), "{}", os.peak(&audio));
    assert!(reduction > 0.9 && reduction < 2.5, "{reduction}");
    assert_eq!(audio[..19_500], original[..19_500]);
    assert_eq!(audio[20_700..], original[20_700..]);
}

#[test]
fn spectrum_keeps_antiphase_energy_and_raw_mel_scaling() {
    let audio = tone(1000.0, 0.1, 144000);
    let normal = spectrum(&audio, 48000);
    let anti = spectrum(
        &audio.iter().map(|x| [x[0], -x[1]]).collect::<Vec<_>>(),
        48000,
    );
    let doubled = spectrum(
        &audio
            .iter()
            .map(|x| [x[0] * 2.0, x[1] * 2.0])
            .collect::<Vec<_>>(),
        48000,
    );
    assert_eq!(normal.band_power, anti.band_power);
    assert!(anti.mid_power.iter().all(|v| *v == 0.0));
    assert!(anti.side_power.iter().sum::<f64>() > 0.0);
    for (a, b) in normal.mel_power.iter().zip(doubled.mel_power) {
        close(b, a * 4.0, 1e-7);
    }
}

static NEXT: AtomicUsize = AtomicUsize::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "mastering-engine-test-{}-{}.wav",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        assert!(!path.exists());
        Self(path)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[test]
fn float_wav_roundtrip_and_overwrite_refusal() {
    let fixture = Fixture::new();
    let audio = tone(1000.0, 0.1, 144000);
    audio_io::write(&fixture.0, &audio, 48000).unwrap();
    assert_eq!(audio_io::read(&fixture.0).unwrap().audio, audio);
    assert!(audio_io::write(&fixture.0, &audio, 48000).is_err());
    let mut bytes = fs::read(&fixture.0).unwrap();
    bytes.truncate(bytes.len() - 1);
    fs::write(&fixture.0, bytes).unwrap();
    assert!(audio_io::read(&fixture.0).is_err());
}

#[test]
fn pcm24_sign_extension_and_nonfinite_refusal() {
    let fixture = Fixture::new();
    let samples = 144000usize;
    let mut bytes = Vec::new();
    bytes.extend(b"RIFF");
    bytes.extend((36 + samples as u32 * 6).to_le_bytes());
    bytes.extend(b"WAVEfmt ");
    bytes.extend(16u32.to_le_bytes());
    bytes.extend(1u16.to_le_bytes());
    bytes.extend(2u16.to_le_bytes());
    bytes.extend(48000u32.to_le_bytes());
    bytes.extend(288000u32.to_le_bytes());
    bytes.extend(6u16.to_le_bytes());
    bytes.extend(24u16.to_le_bytes());
    bytes.extend(b"data");
    bytes.extend((samples as u32 * 6).to_le_bytes());
    for _ in 0..samples {
        bytes.extend([0, 0, 128, 255, 255, 127]);
    }
    fs::write(&fixture.0, bytes).unwrap();
    let audio = audio_io::read(&fixture.0).unwrap().audio;
    assert_eq!(audio[0], [-1.0, 1.0 - 2f32.powi(-23)]);
    let invalid = Fixture::new();
    assert!(audio_io::write(&invalid.0, &vec![[f32::NAN; 2]; samples], 48000).is_err());
    assert!(!invalid.0.exists());
}
