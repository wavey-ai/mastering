pub mod adjust;
pub mod album;
#[cfg(feature = "alloc-stats")]
pub mod alloc_stats;
pub mod analysis;
pub mod audibility;
pub mod audio_io;
pub mod auditory;
pub mod bench;
pub mod broad_tone;
pub mod capture;
pub mod character;
pub mod compressor;
pub mod deesser;
pub mod delivery;
pub mod distributed;
pub mod dsp;
pub mod excerpt;
pub mod finishing;
pub mod fir;
pub mod foundation;
pub mod imaging;
pub mod low_end;
pub mod low_mid;
pub mod masking;
pub mod open;
pub mod pipeline;
pub mod progress;
pub mod refinement;
mod render_blocks;
pub mod resample;
pub mod scratch;
pub mod search;
pub mod selection;
pub mod set;
pub mod source_condition;
pub mod source_treatments;
pub mod stem_evidence;
pub mod stereo;
pub mod stereo_phase;
pub mod stft;
pub mod treatment;
pub mod treatment_render;
pub mod version_names;
pub mod waveform;
pub mod width;

use audio_io::Stereo;
use dsp::db;
use mel_spec::mel::SparseMelFilterbank;
use serde::Serialize;
use stft::StereoStft;

#[derive(Clone, Serialize)]
pub struct Metrics {
    pub integrated_lufs: Option<f64>,
    pub sample_peak_dbfs: f64,
    pub rms_dbfs: f64,
    pub crest_db: f64,
    pub side_mid_db: f64,
    pub lr_cosine: f64,
    pub full_scale_channel_samples: usize,
}

pub fn metrics(audio: &[Stereo], rate: u32) -> Metrics {
    let (mut peak, mut energy, mut mid, mut side, mut left, mut right, mut cross) =
        (0.0f64, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let mut clipped = 0;
    for frame in audio {
        let [l, r] = frame.map(|v| v as f64);
        peak = peak.max(l.abs()).max(r.abs());
        energy += l * l + r * r;
        left += l * l;
        right += r * r;
        cross += l * r;
        mid += ((l + r) * 0.5).powi(2);
        side += ((l - r) * 0.5).powi(2);
        clipped += frame
            .iter()
            .filter(|&&v| v.abs() >= 1.0 - 2f32.powi(-23))
            .count();
    }
    let rms = db((energy / (audio.len().max(1) * 2) as f64).sqrt());
    Metrics {
        integrated_lufs: dsp::loudness(audio, rate),
        sample_peak_dbfs: db(peak),
        rms_dbfs: rms,
        crest_db: db(peak) - rms,
        side_mid_db: db((side.max(1e-30) / mid.max(1e-30)).sqrt()),
        lr_cosine: cross / (left * right).sqrt().max(1e-30),
        full_scale_channel_samples: clipped,
    }
}

#[derive(Clone, Serialize)]
pub struct Spectrum {
    /// The rate of the measured audio, in Hz.
    pub sample_rate: u32,
    pub centers_hz: Vec<f64>,
    pub band_power: Vec<f64>,
    pub mid_power: Vec<f64>,
    pub side_power: Vec<f64>,
    pub mel_power: Vec<f64>,
    pub analyzed_frames: usize,
}

/// The long-term spectrum of `audio` at `rate`.
pub fn spectrum(audio: &[Stereo], rate: u32) -> Spectrum {
    let nfft = 8192;
    let hop = 2048;
    let mut power = vec![0.0; nfft / 2 + 1];
    let mut mid = power.clone();
    let mut side = power.clone();
    let mut frames = 0;
    // A spectrogram holds only its last `nfft` samples. Each range of hops
    // transforms on its own core after the `nfft / hop - 1` hops before it,
    // which yield no frame; the sums take the frames in order.
    let hops = audio.len() / hop;
    let warm = nfft / hop - 1;
    // A batch spans the cores of a native build. The browser engine has one
    // thread: a batch of one range holds 8 MB of spectra, not 67 MB.
    const BATCH: usize = if cfg!(target_arch = "wasm32") {
        64
    } else {
        512
    };
    for batch in (0..hops).step_by(BATCH) {
        let batch_end = (batch + BATCH).min(hops);
        let spectra = dsp::chunked(batch_end - batch, 64, |range| {
            let range = batch + range.start..batch + range.end;
            let mut stft = StereoStft::new(nfft, hop);
            let mut out = Vec::new();
            for index in range.start.saturating_sub(warm)..range.end {
                let chunk = &audio[index * hop..(index + 1) * hop];
                if let Some((a, b)) = stft.add_with(hop, |i| chunk[i]) {
                    if index >= range.start {
                        out.push((a.to_vec(), b.to_vec()));
                    }
                }
            }
            out
        });
        for (a, b) in spectra.into_iter().flatten() {
            for k in 0..power.len() {
                power[k] += (a[k].norm_sqr() + b[k].norm_sqr()) * 0.5;
                mid[k] += ((a[k] + b[k]) * 0.5).norm_sqr();
                side[k] += ((a[k] - b[k]) * 0.5).norm_sqr();
            }
            frames += 1;
        }
    }
    for array in [&mut power, &mut mid, &mut side] {
        for value in array {
            *value /= frames.max(1) as f64;
        }
    }
    let filters = SparseMelFilterbank::from_mel(
        rate as f64,
        nfft,
        128,
        Some(20.0),
        Some(20000.0),
        false,
        true,
    );
    let mut mel_power = vec![0.0; 128];
    filters.project_power_f64(&power, &mut mel_power);
    let edges: Vec<f64> = (0..=24)
        .map(|i| 30.0 * 600f64.powf(i as f64 / 24.0))
        .collect();
    let integrate = |values: &[f64]| -> Vec<f64> {
        edges
            .windows(2)
            .map(|bounds| {
                values
                    .iter()
                    .enumerate()
                    .filter(|(i, _)| {
                        let hz = *i as f64 * rate as f64 / nfft as f64;
                        hz >= bounds[0] && hz < bounds[1]
                    })
                    .map(|(_, &v)| v)
                    .sum()
            })
            .collect()
    };
    Spectrum {
        sample_rate: rate,
        centers_hz: edges.windows(2).map(|e| (e[0] * e[1]).sqrt()).collect(),
        band_power: integrate(&power),
        mid_power: integrate(&mid),
        side_power: integrate(&side),
        mel_power,
        analyzed_frames: frames,
    }
}

#[cfg(test)]
mod spectrum_tests {
    use super::*;
    // The spectrum before its hops transformed on several cores.
    fn spectrum_sequential(audio: &[Stereo]) -> Spectrum {
        let nfft = 8192;
        let hop = 2048;
        let mut stft = StereoStft::new(nfft, hop);
        let mut power = vec![0.0; nfft / 2 + 1];
        let mut mid = power.clone();
        let mut side = power.clone();
        let mut frames = 0;
        for chunk in audio.chunks_exact(hop) {
            if let Some((a, b)) = stft.add_with(hop, |i| chunk[i]) {
                for k in 0..power.len() {
                    power[k] += (a[k].norm_sqr() + b[k].norm_sqr()) * 0.5;
                    mid[k] += ((a[k] + b[k]) * 0.5).norm_sqr();
                    side[k] += ((a[k] - b[k]) * 0.5).norm_sqr();
                }
                frames += 1;
            }
        }
        for array in [&mut power, &mut mid, &mut side] {
            for value in array {
                *value /= frames.max(1) as f64;
            }
        }
        let filters = SparseMelFilterbank::from_mel(
            48000.0,
            nfft,
            128,
            Some(20.0),
            Some(20000.0),
            false,
            true,
        );
        let mut mel_power = vec![0.0; 128];
        filters.project_power_f64(&power, &mut mel_power);
        let edges: Vec<f64> = (0..=24)
            .map(|i| 30.0 * 600f64.powf(i as f64 / 24.0))
            .collect();
        let integrate = |values: &[f64]| -> Vec<f64> {
            edges
                .windows(2)
                .map(|bounds| {
                    values
                        .iter()
                        .enumerate()
                        .filter(|(i, _)| {
                            let hz = *i as f64 * 48000.0 / nfft as f64;
                            hz >= bounds[0] && hz < bounds[1]
                        })
                        .map(|(_, &v)| v)
                        .sum()
                })
                .collect()
        };
        Spectrum {
            sample_rate: 48000,
            centers_hz: edges.windows(2).map(|e| (e[0] * e[1]).sqrt()).collect(),
            band_power: integrate(&power),
            mid_power: integrate(&mid),
            side_power: integrate(&side),
            mel_power,
            analyzed_frames: frames,
        }
    }

    #[test]
    fn hops_on_several_cores_match_the_sequential_spectrum() {
        let mut state = 0x9e37_79b9u32;
        for len in [5_000, 48_000 * 3 + 1_234, 48_000 * 11] {
            let audio: Vec<Stereo> = (0..len)
                .map(|_| {
                    let mut next = || {
                        state ^= state << 13;
                        state ^= state >> 17;
                        state ^= state << 5;
                        state as f32 / u32::MAX as f32 - 0.5
                    };
                    [next(), 0.25 * next()]
                })
                .collect();
            assert_eq!(
                serde_json::to_string(&spectrum(&audio, 48000)).unwrap(),
                serde_json::to_string(&spectrum_sequential(&audio)).unwrap(),
                "{len}"
            );
        }
    }
}
