//! Local measurement for the auditory spatial-overlap proposal rule (`source_treatments`).
//! The same Rust path runs in native and WASM; stems only supply evidence.
use crate::{
    analysis::percentile,
    audio_io::{Result, Stereo},
    source_treatments::{self, Band, ProposalSet},
};
use std::f64::consts::PI;

const FFT: usize = 4096;
const HOP: usize = 1024;
const BANDS: usize = 64;

struct Frame {
    mid: [f64; BANDS],
    side: [f64; BANDS],
    total: f64,
}

impl Frame {
    fn band(&self, i: usize) -> f64 {
        self.mid[i] + self.side[i]
    }
}

fn centre(i: usize) -> f64 {
    (10f64.powf((3.5 + i as f64 * 0.5) / 21.4) - 1.0) / 0.00437
}

fn measure(audio: &[Stereo], rate: u32) -> Vec<Frame> {
    let window_energy: f64 = (0..FFT)
        .map(|i| (0.5 * (1.0 - (2.0 * PI * i as f64 / FFT as f64).cos())).powi(2))
        .sum();
    let scale = 2.0 / (FFT as f64 * window_energy);
    // Truncation below 1e-12 changes measured powers far below float32 PCM
    // precision and avoids multiplying every distant FFT bin for every filter.
    let weights: Vec<Vec<(usize, f64)>> = (0..BANDS)
        .map(|i| {
            let hz = centre(i);
            let erb = 24.7 * (1.0 + 0.00437 * hz);
            (0..=FFT / 2)
                .filter_map(|bin| {
                    let frequency = bin as f64 * rate as f64 / FFT as f64;
                    let pg = 4.0 * (frequency - hz).abs() / erb;
                    let weight = (1.0 + pg) * (-pg).exp();
                    (weight >= 1e-12).then_some((bin, weight))
                })
                .collect()
        })
        .collect();
    crate::stft::stereo_frames(audio, FFT, HOP, |left, right| {
        let mut mid_power = [0.0; FFT / 2 + 1];
        let mut side_power = [0.0; FFT / 2 + 1];
        let mut total = 0.0;
        for bin in 0..=FFT / 2 {
            let edge = if bin == 0 || bin == FFT / 2 { 0.5 } else { 1.0 };
            let mid = (left[bin] + right[bin]) * 0.5;
            let side = (left[bin] - right[bin]) * 0.5;
            mid_power[bin] = mid.norm_sqr() * scale * edge;
            side_power[bin] = side.norm_sqr() * scale * edge;
            total += mid_power[bin] + side_power[bin];
        }
        let mut frame = Frame {
            mid: [0.0; BANDS],
            side: [0.0; BANDS],
            total,
        };
        for (band, weights) in weights.iter().enumerate() {
            for &(bin, weight) in weights {
                frame.mid[band] += mid_power[bin] * weight;
                frame.side[band] += side_power[bin] * weight;
            }
        }
        frame
    })
}

fn db(power: f64) -> f64 {
    10.0 * power.max(1e-30).log10()
}

fn local_delta(vm: f64, vs: f64, am: f64, ass: f64, mid: bool) -> f64 {
    let before = db((vm + vs) / (am + ass).max(1e-30));
    let gain = 10f64.powf(3.0 / 10.0);
    let (v, a) = if mid {
        (vm * gain + vs, am * gain + ass)
    } else {
        (vm + vs * gain, am + ass * gain)
    };
    db(v / a.max(1e-30)) - before
}

/// The auditory proposals of the vocal stem against the accompaniment, both
/// at `rate`. The proposed filters are at the render rate `render_rate`.
pub fn propose_from_stems(
    vocal: &[Stereo],
    accompaniment: &[Stereo],
    rate: u32,
    source_file_sha256: &str,
    source_pcm_sha256: &str,
    render_rate: u32,
) -> Result<ProposalSet> {
    if vocal.len() != accompaniment.len() || vocal.len() < FFT || ![44100, 48000].contains(&rate) {
        return Err("Auditory analysis needs aligned 44.1/48 kHz stems".into());
    }
    let v = measure(vocal, rate);
    let a = measure(accompaniment, rate);
    let totals: Vec<_> = v.iter().map(|frame| frame.total).collect();
    let gate = percentile(&totals, 0.95) * 10f64.powf(-2.5);
    let active: Vec<_> = (0..v.len())
        .filter(|&i| {
            v[i].total > gate && v[i].total > (v[i].total + a[i].total) * 0.02 && v[i].total > 1e-20
        })
        .collect();
    let active_seconds = active.len() as f64 * HOP as f64 / rate as f64;
    let total_excitation: f64 = active
        .iter()
        .map(|&i| (0..BANDS).map(|b| v[i].band(b)).sum::<f64>())
        .sum();
    let mut bands = Vec::with_capacity(BANDS);
    for band in 0..BANDS {
        let hz = centre(band);
        let valid: Vec<_> = active
            .iter()
            .copied()
            .filter(|&i| v[i].band(band) > v[i].total * 0.001)
            .collect();
        if valid.len() < 8 {
            bands.push(Band {
                hz,
                valid: false,
                eligible_seconds: None,
                vocal_excitation_share_percent: None,
                vocal_to_accompaniment_p10_p50_p90_db: None,
                local_mid_plus3_ratio_delta_db: None,
                local_side_plus3_ratio_delta_db: None,
            });
            continue;
        }
        let ratio: Vec<_> = valid
            .iter()
            .map(|&i| db(v[i].band(band) / a[i].band(band).max(1e-30)))
            .collect();
        let mid: Vec<_> = valid
            .iter()
            .map(|&i| {
                local_delta(
                    v[i].mid[band],
                    v[i].side[band],
                    a[i].mid[band],
                    a[i].side[band],
                    true,
                )
            })
            .collect();
        let side: Vec<_> = valid
            .iter()
            .map(|&i| {
                local_delta(
                    v[i].mid[band],
                    v[i].side[band],
                    a[i].mid[band],
                    a[i].side[band],
                    false,
                )
            })
            .collect();
        bands.push(Band {
            hz,
            valid: true,
            eligible_seconds: Some(valid.len() as f64 * HOP as f64 / rate as f64),
            vocal_excitation_share_percent: Some(
                100.0 * active.iter().map(|&i| v[i].band(band)).sum::<f64>()
                    / total_excitation.max(1e-30),
            ),
            vocal_to_accompaniment_p10_p50_p90_db: Some(
                [0.1, 0.5, 0.9].map(|p| percentile(&ratio, p)),
            ),
            local_mid_plus3_ratio_delta_db: Some(percentile(&mid, 0.5)),
            local_side_plus3_ratio_delta_db: Some(percentile(&side, 0.5)),
        });
    }
    source_treatments::propose_measured(
        source_file_sha256,
        source_pcm_sha256,
        active_seconds,
        bands,
        render_rate,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aligned_mid_vocal_against_side_accompaniment_proposes_mid_focus() {
        let rate = 48000;
        let frames = rate * 12;
        let vocal: Vec<Stereo> = (0..frames)
            .map(|i| {
                let x = (2.0 * PI * 700.0 * i as f64 / rate as f64).sin() as f32 * 0.02;
                [x, x]
            })
            .collect();
        let accompaniment: Vec<Stereo> = (0..frames)
            .map(|i| {
                let x = (2.0 * PI * 700.0 * i as f64 / rate as f64).sin() as f32 * 0.04;
                [x, -x]
            })
            .collect();
        let hash = "0".repeat(64);
        let proposals =
            propose_from_stems(&vocal, &accompaniment, rate, &hash, &hash, 48000).unwrap();
        assert!(proposals
            .proposals
            .iter()
            .any(|p| p.id.starts_with("focus-mid-")));
        assert!(!proposals
            .proposals
            .iter()
            .any(|p| p.id.starts_with("focus-side-")));
        assert_eq!(proposals.proposals[0].id, "unchanged");
    }
}
