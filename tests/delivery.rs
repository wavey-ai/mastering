use mastering_engine::{
    audio_io, bench,
    delivery::{self, Format},
    dsp, resample,
};
use std::{
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};
static NEXT: AtomicU64 = AtomicU64::new(0);
struct Directory(PathBuf);
impl Directory {
    fn new() -> Self {
        let p = std::env::temp_dir().join(format!(
            "master-delivery-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&p).unwrap();
        Self(p)
    }
}
impl Drop for Directory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn integer_and_float_exports_round_trip_and_refuse_overwrite() {
    let dir = Directory::new();
    let audio = bench::tone(1000.0, 0.5, 144000, 48000);
    for (i, format) in [Format::Float32, Format::Pcm16, Format::Pcm24]
        .into_iter()
        .enumerate()
    {
        let p = dir.0.join(format!("{i}.wav"));
        delivery::write(&p, &audio, 48000, format, 123).unwrap();
        let result = delivery::verify(&p, &audio, 48000, format, -1.0).unwrap();
        assert!(result.verified);
        assert_eq!(result.frames, audio.len());
        assert!(result.peak_estimate_dbtp < -5.9);
        let before = std::fs::read(&p).unwrap();
        assert!(delivery::write(&p, &audio, 48000, format, 123).is_err());
        assert_eq!(std::fs::read(&p).unwrap(), before);
        let q = dir.0.join(format!("{i}-repeat.wav"));
        delivery::write(&q, &audio, 48000, format, 123).unwrap();
        assert_eq!(std::fs::read(&q).unwrap(), before);
    }
}

#[test]
fn dither_has_zero_mean_finite_energy_and_independent_channels() {
    let dir = Directory::new();
    let p = dir.0.join("silence.wav");
    let audio = vec![[0.0; 2]; 144000];
    delivery::write(&p, &audio, 48000, Format::Pcm16, 94).unwrap();
    let decoded = audio_io::read(&p).unwrap().audio;
    let n = decoded.len() as f64;
    let mean = decoded.iter().map(|x| x[0] as f64 * 32768.0).sum::<f64>() / n;
    let power = decoded
        .iter()
        .map(|x| (x[0] as f64 * 32768.0).powi(2))
        .sum::<f64>()
        / n;
    let cross = decoded
        .iter()
        .map(|x| x[0] as f64 * x[1] as f64 * 32768.0f64.powi(2))
        .sum::<f64>()
        / n;
    assert!(mean.abs() < 0.01);
    assert!((power - 0.25).abs() < 0.01);
    assert!(cross.abs() < 0.01);
}

#[test]
fn unsafe_or_corrupted_exports_fail_verification() {
    let dir = Directory::new();
    let p = dir.0.join("unsafe.wav");
    assert!(delivery::write(&p, &vec![[1.0; 2]; 144000], 48000, Format::Pcm24, 0).is_err());
    assert!(!p.exists());
    let source = bench::tone(1000.0, 0.5, 144000, 48000);
    delivery::write(&p, &source, 48000, Format::Float32, 0).unwrap();
    assert!(delivery::verify(&p, &source, 48000, Format::Float32, -12.0).is_err());
    let other = bench::tone(1000.0, 0.25, 144000, 48000);
    assert!(delivery::verify(&p, &other, 48000, Format::Float32, -1.0).is_err());
}

#[test]
fn resampling_preserves_duration_dc_and_low_frequency_amplitude() {
    for rate in resample::RATES {
        let input = bench::tone(1000.0, 0.5, rate as usize, rate);
        let output = resample::convert(&input, rate, 48000).unwrap();
        assert_eq!(output.len(), 48000);
        assert!(
            (bench::amplitude(&output[12000..36000], 1000.0, 48000).unwrap() - 0.5).abs() < 2e-5
        );
        if rate == 48000 {
            assert_eq!(input, output);
        }
    }
    let dc = resample::convert(&vec![[0.2, -0.1]; 44100], 44100, 48000).unwrap();
    assert!(dc[200..47000]
        .iter()
        .all(|x| (x[0] - 0.2).abs() < 1e-6 && (x[1] + 0.1).abs() < 1e-6));
}

#[test]
fn downsampling_rejects_above_nyquist_tones_and_keeps_impulse_time() {
    let input = bench::tone(30000.0, 0.5, 96000, 96000);
    let output = resample::convert(&input, 96000, 48000).unwrap();
    assert!(
        dsp::db(bench::amplitude(&output[12000..36000], 18000.0, 48000).unwrap() / 0.5) < -70.0
    );
    let mut impulse = vec![[0.0; 2]; 96000];
    impulse[48000] = [1.0; 2];
    let output = resample::convert(&impulse, 96000, 48000).unwrap();
    let peak = output
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a[0].abs().total_cmp(&b[0].abs()))
        .unwrap()
        .0;
    assert_eq!(peak, 24000);
    assert!(resample::convert(&impulse, 12345, 48000).is_err());
}

#[test]
fn soundkit_input_preserves_all_supported_sample_precisions_across_byte_chunks() {
    use soundkit::wav::{WavSampleFormat, WavStreamEncoder};
    let dir = Directory::new();
    let frames = 144_001;
    for (name, format, bits) in [
        ("pcm16", WavSampleFormat::I16, 16),
        ("pcm24", WavSampleFormat::I24, 24),
        ("pcm32", WavSampleFormat::I32, 32),
    ] {
        let scale = 2.0f64.powi(bits - 1);
        let values = [-(scale as i64) as i32, -1, 0, 1, (scale as i64 - 1) as i32];
        let planar: Vec<_> = (0..frames * 2).map(|i| values[i % values.len()]).collect();
        let mut encoder = WavStreamEncoder::new(format, 48000, 2, frames as u64).unwrap();
        let mut bytes = encoder.header().to_vec();
        bytes.extend(match format {
            WavSampleFormat::I16 => encoder
                .push_planar_i16(
                    &planar.iter().map(|&x| x as i16).collect::<Vec<_>>(),
                    frames,
                )
                .unwrap(),
            WavSampleFormat::I24 => encoder.push_planar_i24(&planar, frames).unwrap(),
            _ => encoder.push_planar_i32(&planar, frames).unwrap(),
        });
        encoder.finish().unwrap();
        let path = dir.0.join(format!("{name}.wav"));
        std::fs::write(&path, bytes).unwrap();
        let decoded = audio_io::read_source(&path).unwrap();
        assert_eq!(decoded.audio.len(), frames);
        for (i, actual) in decoded.audio.iter().enumerate() {
            for channel in 0..2 {
                let expected = (planar[channel * frames + i] as f64 / scale) as f32;
                assert_eq!(
                    actual[channel].to_bits(),
                    expected.to_bits(),
                    "{name}/{i}/{channel}"
                );
            }
        }
    }
    let audio: Vec<_> = (0..frames)
        .map(|i| [f32::from_bits((i % 0x7f_ffff) as u32), -0.0])
        .collect();
    let path = dir.0.join("float32.wav");
    audio_io::write(&path, &audio, 48000).unwrap();
    let decoded = audio_io::read_source(&path).unwrap();
    assert!(decoded
        .audio
        .iter()
        .flatten()
        .map(|x| x.to_bits())
        .eq(audio.iter().flatten().map(|x| x.to_bits())));
}
