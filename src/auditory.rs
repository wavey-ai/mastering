//! Local measurement for the auditory spatial-overlap proposal rule (`source_treatments`).
//! The same Rust path runs in native and WASM; stems only supply evidence.
use crate::{
    analysis::percentile,
    audio_io::{Result, Stereo},
    source_treatments::{self, Band, ProposalSet},
};
use rustfft::num_complex::Complex;
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
    crate::stft::stereo_frames(audio, FFT, HOP, frame_measure(rate))
}

/// The auditory frame of the left and right spectra of one STFT frame at
/// `rate`.
fn frame_measure(rate: u32) -> impl Fn(&[Complex<f64>], &[Complex<f64>]) -> Frame + Sync {
    let window_energy: f64 = (0..FFT)
        .map(|i| (0.5 * (1.0 - (2.0 * PI * i as f64 / FFT as f64).cos())).powi(2))
        .sum();
    let scale = 2.0 / (FFT as f64 * window_energy);
    // Truncation below 1e-12 changes measured powers far below float32 PCM
    // precision and avoids multiplying every distant FFT bin for every filter.
    // The weight falls with the distance from the centre, so the bins of a
    // band are one run: the first bin and the weight of each bin of the run.
    let weights: Vec<(usize, Vec<f64>)> = band_weights(rate)
        .into_iter()
        .map(|bins| {
            let first = bins.first().map_or(0, |&(bin, _)| bin);
            assert!(
                bins.iter()
                    .enumerate()
                    .all(|(k, &(bin, _))| bin == first + k),
                "The bins of an auditory band are one run"
            );
            (first, bins.into_iter().map(|(_, weight)| weight).collect())
        })
        .collect();
    move |left: &[Complex<f64>], right: &[Complex<f64>]| {
        // The mid and the side power of each bin side by side, so the two
        // sums of a band take one vector operation for each bin (D307).
        let mut power = [[0.0f64; 2]; FFT / 2 + 1];
        let mut total = 0.0;
        for (bin, power) in power.iter_mut().enumerate() {
            let edge = if bin == 0 || bin == FFT / 2 { 0.5 } else { 1.0 };
            let mid = (left[bin] + right[bin]) * 0.5;
            let side = (left[bin] - right[bin]) * 0.5;
            *power = [
                mid.norm_sqr() * scale * edge,
                side.norm_sqr() * scale * edge,
            ];
            total += power[0] + power[1];
        }
        let mut frame = Frame {
            mid: [0.0; BANDS],
            side: [0.0; BANDS],
            total,
        };
        // Four bands side by side over the bins that each of them holds:
        // each band adds its bins in order, and the four chains of additions
        // run beside each other (D324).
        let (groups, rest) = weights.as_chunks::<GROUP>();
        let mut band = 0;
        for group in groups {
            let common = group.iter().map(|(_, w)| w.len()).min().unwrap_or(0);
            let heads = group
                .each_ref()
                .map(|(first, weights)| (&power[*first..*first + common], &weights[..common]));
            let mut sums = [[0.0f64; 2]; GROUP];
            for k in 0..common {
                for (sums, (power, weights)) in sums.iter_mut().zip(&heads) {
                    sums[0] += power[k][0] * weights[k];
                    sums[1] += power[k][1] * weights[k];
                }
            }
            for (sums, (first, weights)) in sums.iter_mut().zip(group) {
                for (power, &weight) in power[*first + common..].iter().zip(&weights[common..]) {
                    sums[0] += power[0] * weight;
                    sums[1] += power[1] * weight;
                }
                [frame.mid[band], frame.side[band]] = *sums;
                band += 1;
            }
        }
        for (first, weights) in rest {
            let mut sums = [0.0f64; 2];
            for (power, &weight) in power[*first..].iter().zip(weights) {
                sums[0] += power[0] * weight;
                sums[1] += power[1] * weight;
            }
            [frame.mid[band], frame.side[band]] = sums;
            band += 1;
        }
        frame
    }
}

/// Bands of `frame_measure` side by side.
const GROUP: usize = 4;

/// The bins of each band at `rate` whose weight is 1e-12 or more, with
/// their weights.
fn band_weights(rate: u32) -> Vec<Vec<(usize, f64)>> {
    (0..BANDS)
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
        .collect()
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

/// The auditory frames of one stem, for `propose_from_measured`.
pub struct Measured {
    frames: Vec<Frame>,
    len: usize,
    rate: u32,
}

/// The auditory frames of a stem at `rate`.
pub fn measure_stem(audio: &[Stereo], rate: u32) -> Result<Measured> {
    if audio.len() < FFT || ![44100, 48000].contains(&rate) {
        return Err(MISALIGNED.into());
    }
    Ok(Measured {
        frames: measure(audio, rate),
        len: audio.len(),
        rate,
    })
}

/// `measure_stem` of the frames of `frames`, read in spans.
pub(crate) fn measure_stem_read(
    frames: &mut impl crate::scratch::Frames,
    rate: u32,
) -> Result<Measured> {
    let len = frames.len();
    if len < FFT || ![44100, 48000].contains(&rate) {
        return Err(MISALIGNED.into());
    }
    let mut measured = Vec::with_capacity(crate::stft::frame_count(len / HOP, FFT, HOP));
    crate::stft::stereo_frames_read(
        frames,
        FFT,
        HOP,
        |x| x,
        frame_measure(rate),
        |f| measured.push(f),
    )?;
    Ok(Measured {
        frames: measured,
        len,
        rate,
    })
}

const MISALIGNED: &str = "Auditory analysis needs aligned 44.1/48 kHz stems";

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
    if vocal.len() != accompaniment.len() {
        return Err(MISALIGNED.into());
    }
    propose_from_measured(
        &measure_stem(vocal, rate)?,
        &measure_stem(accompaniment, rate)?,
        source_file_sha256,
        source_pcm_sha256,
        render_rate,
    )
}

/// `propose_from_stems` of the frames of the vocal stem and of the
/// accompaniment.
pub fn propose_from_measured(
    vocal: &Measured,
    accompaniment: &Measured,
    source_file_sha256: &str,
    source_pcm_sha256: &str,
    render_rate: u32,
) -> Result<ProposalSet> {
    if vocal.len != accompaniment.len || vocal.rate != accompaniment.rate {
        return Err(MISALIGNED.into());
    }
    let rate = vocal.rate;
    let (v, a) = (&vocal.frames, &accompaniment.frames);
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

    /// The bands side by side give the sums of one band at a time, bit for
    /// bit (D324).
    #[test]
    fn the_bands_side_by_side_equal_one_band_at_a_time() {
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 11) as f64 / (1u64 << 53) as f64 - 0.5
        };
        let spectra: Vec<Vec<Complex<f64>>> = (0..2)
            .map(|_| {
                (0..=FFT / 2)
                    .map(|_| Complex::new(next(), next()))
                    .collect()
            })
            .collect();
        let (left, right) = (&spectra[0], &spectra[1]);
        for rate in [44_100, 48_000] {
            let frame = frame_measure(rate)(left, right);
            let window_energy: f64 = (0..FFT)
                .map(|i| (0.5 * (1.0 - (2.0 * PI * i as f64 / FFT as f64).cos())).powi(2))
                .sum();
            let scale = 2.0 / (FFT as f64 * window_energy);
            for (band, bins) in band_weights(rate).into_iter().enumerate() {
                let mut sums = [0.0f64; 2];
                for (bin, weight) in bins {
                    let edge = if bin == 0 || bin == FFT / 2 { 0.5 } else { 1.0 };
                    let mid = (left[bin] + right[bin]) * 0.5;
                    let side = (left[bin] - right[bin]) * 0.5;
                    sums[0] += mid.norm_sqr() * scale * edge * weight;
                    sums[1] += side.norm_sqr() * scale * edge * weight;
                }
                assert_eq!(
                    frame.mid[band].to_bits(),
                    sums[0].to_bits(),
                    "{rate} {band}"
                );
                assert_eq!(
                    frame.side[band].to_bits(),
                    sums[1].to_bits(),
                    "{rate} {band}"
                );
            }
        }
    }

    #[test]
    fn the_runs_of_the_bands_give_the_sums_of_the_weighted_bins() {
        // The frame of each band from its pairs of a bin and a weight, as
        // the measure took it before D307.
        for rate in [44100, 48000] {
            let measure = frame_measure(rate);
            let weights = band_weights(rate);
            let spectrum = |seed: f64| -> Vec<Complex<f64>> {
                (0..=FFT / 2)
                    .map(|bin| {
                        let x = bin as f64 + seed;
                        Complex::new((x * 0.37).sin() * 1e3 / (1.0 + x), (x * 0.11).cos())
                    })
                    .collect()
            };
            let (left, right) = (spectrum(0.0), spectrum(0.5));
            let frame = measure(&left, &right);
            let (mid, side) = measure_pairs(&left, &right, &weights);
            let bits = |v: &[f64]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
            assert_eq!(bits(&frame.mid), bits(&mid));
            assert_eq!(bits(&frame.side), bits(&side));
        }
    }

    fn measure_pairs(
        left: &[Complex<f64>],
        right: &[Complex<f64>],
        weights: &[Vec<(usize, f64)>],
    ) -> (Vec<f64>, Vec<f64>) {
        let window_energy: f64 = (0..FFT)
            .map(|i| (0.5 * (1.0 - (2.0 * PI * i as f64 / FFT as f64).cos())).powi(2))
            .sum();
        let scale = 2.0 / (FFT as f64 * window_energy);
        let power = |bin: usize, sign: f64| {
            let edge = if bin == 0 || bin == FFT / 2 { 0.5 } else { 1.0 };
            ((left[bin] + right[bin] * sign) * 0.5).norm_sqr() * scale * edge
        };
        let (mut mid, mut side) = (vec![0.0; BANDS], vec![0.0; BANDS]);
        for (band, weights) in weights.iter().enumerate() {
            for &(bin, weight) in weights {
                mid[band] += power(bin, 1.0) * weight;
                side[band] += power(bin, -1.0) * weight;
            }
        }
        (mid, side)
    }

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
