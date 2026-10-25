use mastering_engine::{
    audio_io::{self, Result, Stereo},
    compressor, delivery, distributed,
    dsp::{self, EqBand, Kind, LimiterSettings, Oversampler},
    finishing, resample,
    set::{self, Residual, Sink, Version},
    treatment::Tone,
    treatment_render::{Renderer, Spec, WindowBands},
};
use std::f64::consts::TAU;

fn tones(rate: u32, seconds: usize, partials: &[(f64, f64)]) -> Vec<Stereo> {
    (0..rate as usize * seconds)
        .map(|i| {
            let t = i as f64 / rate as f64;
            let x: f64 = partials
                .iter()
                .map(|(hz, a)| a * (TAU * hz * t).sin())
                .sum();
            [x as f32, x as f32]
        })
        .collect()
}

/// The material of `tests/set.rs`, at 48 kHz.
fn signal(seconds: usize) -> Vec<Stereo> {
    let mut seed = 19u32;
    (0..48_000 * seconds)
        .map(|i| {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let noise = ((seed >> 8) as f64 / 16777216.0 - 0.5) * 0.12;
            let t = i as f64 / 48_000.0;
            let pulse = 0.65 * (-(t % 0.25) / (700.0 / 48000.0)).exp() * (TAU * 120.0 * t).sin();
            let mid = noise + pulse + 0.05 * (TAU * 700.0 * t).sin();
            let side = 0.035 * (TAU * 970.0 * t).sin();
            [(mid + side) as f32, (mid - side) as f32]
        })
        .collect()
}

#[test]
fn each_source_rate_renders_at_the_rate_of_its_family() {
    for (source, render) in [
        (44_100, 44_100),
        (88_200, 44_100),
        (176_400, 44_100),
        (48_000, 48_000),
        (96_000, 48_000),
        (192_000, 48_000),
    ] {
        assert_eq!(audio_io::render_rate(source).unwrap(), render);
    }
    assert!(audio_io::render_rate(32_000).is_err());
    let path = std::env::temp_dir().join(format!("source-rate-{}.wav", std::process::id()));
    audio_io::write(&path, &tones(88_200, 3, &[(1000.0, 0.1)]), 88_200).unwrap();
    let source = audio_io::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    assert_eq!(source.sample_rate, 44_100);
    assert_eq!(source.audio.len(), 3 * 44_100);
}

#[test]
fn loudness_is_the_same_at_each_render_rate() {
    // A 997 Hz sine at full scale on one channel reads -3.01 LUFS (ITU-R
    // BS.1770).
    for rate in audio_io::RENDER_RATES {
        let mut sine = tones(rate, 5, &[(997.0, 1.0)]);
        for frame in &mut sine {
            frame[1] = 0.0;
        }
        let lufs = dsp::loudness(&sine, rate).unwrap();
        assert!((lufs + 3.01).abs() < 0.01, "{rate}: {lufs}");
    }
    let partials = [
        (40.0, 0.2),
        (100.0, 0.2),
        (1000.0, 0.1),
        (5000.0, 0.05),
        (10_000.0, 0.03),
        (16_000.0, 0.02),
    ];
    let [a, b] = audio_io::RENDER_RATES.map(|rate| dsp::loudness(&tones(rate, 5, &partials), rate));
    assert!((a.unwrap() - b.unwrap()).abs() < 0.01, "{a:?} {b:?}");
    for hz in [20.0, 100.0, 1000.0, 4000.0, 10_000.0, 16_000.0] {
        let [a, b] = audio_io::RENDER_RATES.map(|rate| dsp::k_weighting_power(hz, rate));
        assert!(dsp::db(a / b).abs() < 0.05, "{hz} Hz");
    }
}

#[test]
fn peak_control_passes_the_audible_band_at_each_rate() {
    for rate in audio_io::RENDER_RATES {
        let os = Oversampler::new(rate);
        for hz in [1000.0, 16_000.0, 20_000.0] {
            let tone = tones(rate, 1, &[(hz, 0.5)]);
            let round_trip = os.down(&os.up(&tone));
            let level = |audio: &[Stereo]| {
                let middle = &audio[rate as usize / 4..3 * rate as usize / 4];
                (middle.iter().map(|f| (f[0] as f64).powi(2)).sum::<f64>() / middle.len() as f64)
                    .sqrt()
            };
            let change = dsp::db(level(&round_trip) / level(&tone));
            assert!(change.abs() < 0.05, "{rate} Hz rate, {hz} Hz: {change} dB");
            // The 4x samples of the steady tone do not read above it.
            let up = os.up(&tone);
            let peak = up[up.len() / 4..3 * up.len() / 4]
                .iter()
                .map(|f| f[0].abs() as f64)
                .fold(0.0, f64::max);
            assert!(
                dsp::db(peak / 0.5) < 0.05,
                "{rate} Hz rate, {hz} Hz: peak {peak}"
            );
        }
    }
}

#[test]
fn time_constants_follow_the_rate() {
    // A 50 ms burst 6 dB over the ceiling at the 4x rate.
    let runs = audio_io::RENDER_RATES.map(|rate| {
        let frames = 4 * rate as usize;
        let burst = 4 * rate as usize / 20;
        let mut audio: Vec<Stereo> = (0..frames)
            .map(|i| {
                let level = if (frames / 4..frames / 4 + burst).contains(&i) {
                    1.0
                } else {
                    0.1
                };
                [level, level]
            })
            .collect();
        dsp::limit(&mut audio, 0.5, LimiterSettings::default(), rate).longest_run_above_one_db_ms
    });
    assert!((runs[0] - runs[1]).abs() < 0.1, "{runs:?}");
    // A compressor with a 30 ms attack reaches the same reduction 30 ms
    // after a step at each rate.
    let settings = compressor::Settings {
        threshold_dbfs: -20.0,
        ratio: 2.0,
        knee_db: 0.0,
        attack_ms: 30.0,
        release_ms: 300.0,
        detector_rms_ms: 10.0,
        sidechain_highpass_hz: 0.0,
        maximum_reduction_db: 12.0,
        section_ms: 0.0,
        section_makeup: false,
    };
    let gains = audio_io::RENDER_RATES.map(|rate| {
        let mut audio: Vec<Stereo> = (0..rate as usize)
            .map(|i| {
                if i < rate as usize / 2 {
                    [0.01; 2]
                } else {
                    [0.5; 2]
                }
            })
            .collect();
        compressor::process(&mut audio, settings, rate).unwrap();
        audio[rate as usize / 2 + rate as usize * 3 / 100][0] as f64
    });
    assert!(dsp::db(gains[0] / gains[1]).abs() < 0.05, "{gains:?}");
}

#[derive(Default)]
struct MemorySink {
    audio: Vec<(String, Vec<Stereo>)>,
}
impl Sink for MemorySink {
    fn comparisons(&mut self, audio: &[Stereo]) -> Result<Vec<Residual>> {
        self.audio
            .iter()
            .map(|(id, reference)| {
                Ok(Residual {
                    id: id.clone(),
                    matched_residual_db: set::matched_residual_db(reference, audio)?,
                })
            })
            .collect()
    }
    fn deliver(&mut self, version: &Version<'_>, audio: &[Stereo]) -> Result<()> {
        assert!(version.report.render.peak_estimate_dbtp <= version.spec.ceiling_dbtp);
        self.audio.push((version.id.into(), audio.to_vec()));
        Ok(())
    }
}

#[test]
fn a_set_renders_at_44_1_khz_and_its_plan_keeps_the_rate() {
    let rate = 44_100;
    let input = resample::convert(&signal(3), 48_000, rate).unwrap();
    let plan = set::prepare(&input, rate, None, None)
        .unwrap()
        .worker_plan()
        .unwrap();
    assert_eq!(plan.sample_rate, rate);
    let plan_json = serde_json::to_vec(&plan).unwrap();
    let mut sink = MemorySink::default();
    let summary = set::render(&input, rate, None, None, &mut sink).unwrap();
    assert_eq!(summary.source.sample_rate, rate);
    let (_, essential) = &sink.audio[0];
    assert_eq!(essential.len(), input.len());
    // A delivered 44.1 kHz file passes its verification.
    let directory = std::env::temp_dir().join(format!("source-rate-set-{}", std::process::id()));
    std::fs::create_dir(&directory).unwrap();
    let path = directory.join("essential.wav");
    delivery::write(&path, essential, rate, delivery::Format::Pcm24, 7).unwrap();
    let verified = delivery::verify(
        &path,
        essential,
        rate,
        delivery::Format::Pcm24,
        set::SET_CEILING_DBTP,
    )
    .unwrap();
    assert_eq!(verified.sample_rate, rate);
    // A worker renders the plan at its own rate only, and its essential
    // version is the set's.
    assert!(
        distributed::render_family(&input, 48_000, &plan_json, "essential", &directory).is_err()
    );
    let family = directory.join("family");
    std::fs::create_dir(&family).unwrap();
    let result =
        distributed::render_family(&input, rate, &plan_json, "essential", &family).unwrap();
    let delivered = audio_io::read_source(&family.join(&result.versions[0].raw_wav_file)).unwrap();
    assert_eq!(delivered.sample_rate, rate);
    assert_eq!(&delivered.audio, essential);
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn the_chain_gives_the_same_result_at_each_rate() {
    // One spec with a tone, the compressor, the limiter, the clipper and the
    // peak-tone restoration, on the same material at each rate.
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
        tone: Tone {
            stereo: vec![
                EqBand {
                    kind: Kind::LowShelf,
                    hz: 150.0,
                    q: std::f64::consts::FRAC_1_SQRT_2,
                    gain_db: 1.5,
                },
                EqBand {
                    kind: Kind::Peak,
                    hz: 3162.0,
                    q: 0.7,
                    gain_db: 2.0,
                },
            ],
            ..Tone::default()
        },
        focus_envelopes: Vec::new(),
        side_highpass_hz: Some(100.0),
        side_gain: None,
        deesser: None,
        low_mid: None,
        low_end_hold: None,
        mono_safe: Vec::new(),
        dynamic_after_eq: Vec::new(),
        compressor: Some(compressor::Settings {
            threshold_dbfs: -24.0,
            ratio: 2.0,
            knee_db: 6.0,
            attack_ms: 30.0,
            release_ms: 300.0,
            detector_rms_ms: 10.0,
            sidechain_highpass_hz: 60.0,
            maximum_reduction_db: 6.0,
            section_ms: 0.0,
            section_makeup: false,
        }),
        limiter: LimiterSettings {
            release_ms: set::LIMITER_RELEASE_MS,
            ..LimiterSettings::default()
        },
        clipper_db: set::BASE_CLIPPER_DB,
        ceiling_dbtp: set::SET_CEILING_DBTP,
        initial_drive_db: 6.0,
        target_lufs: -11.0,
        fixed_drive_db: None,
        restore_peak_tone: true,
        restoration_db: [0.0; 6],
        true_peak: set::TRUE_PEAK,
    };
    let at_48k = signal(3);
    let at_44k = resample::convert(&at_48k, 48_000, 44_100).unwrap();
    let rendered = [(&at_44k, 44_100), (&at_48k, 48_000)]
        .map(|(audio, rate)| Renderer::new(audio, rate).render(&spec).unwrap());
    for r in &rendered {
        assert!((r.lufs - spec.target_lufs).abs() <= 0.03);
        assert!(r.peak_estimate_dbtp <= spec.ceiling_dbtp);
    }
    // The band balance at matched loudness and the limiter work agree. The
    // 48 kHz render is measured at 44.1 kHz, on the same frequency grid.
    let converted = resample::convert(&rendered[1].audio, 48_000, 44_100).unwrap();
    let [a, b] = [&rendered[0].audio[..], &converted[..]]
        .map(|audio| WindowBands::measure(audio, 44_100).total());
    let level = |bands: &[f64; 6]| bands.iter().sum::<f64>();
    for band in 0..5 {
        let change = dsp::db((a[band] / level(&a)).sqrt()) - dsp::db((b[band] / level(&b)).sqrt());
        assert!(change.abs() < 0.1, "band {band}: {change} dB");
    }
    let limiter = rendered.each_ref().map(|r| r.limiter.mean_reduction_db);
    assert!((limiter[0] - limiter[1]).abs() < 0.05, "{limiter:?}");
}
