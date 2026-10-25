//! Simultaneous and forward masking of stereo audio per critical band.
//!
//! The model follows the FFT ear model of ITU-R BS.1387 (PEAQ) in its
//! framing (2048-point Hann frames, 1024-point hop) and forward
//! masking, and Johnston (1988) in its spreading and tonality offset:
//!
//! - Critical bands of 1 Bark (Zwicker and Terhardt).
//! - The band energies of the masker spread with the Schroeder function.
//! - An offset of α(14.5 + z) + (1 − α)5.5 dB under the spread energy, with
//!   the tonality α from the spectral flatness (0 dB noise-like, −60 dB
//!   tonal) of the bins up to 20 kHz.
//! - Forward masking with time constants from 8 ms (high bands) to 30 ms.
//! - The threshold in quiet (Terhardt), with a full-scale sine at 92 dB SPL.
use crate::audio_io::Stereo;
use rustfft::num_complex::Complex;

/// Frame length and hop in samples.
pub const FFT: usize = 2048;
pub const HOP: usize = 1024;
/// Critical bands: whole Bark numbers 0 to 24.
pub const BANDS: usize = 25;
/// Level of a full-scale sine, in dB SPL (the PEAQ listening level).
pub const FULL_SCALE_SPL: f64 = 92.0;
/// Highest frequency of the spectral flatness. Content above it is
/// inaudible, and its level depends on the sample rate and on the filters
/// and codecs that the mix went through (D174).
pub const FLATNESS_MAX_HZ: f64 = 20_000.0;

/// Critical-band number of a frequency (Zwicker and Terhardt).
pub fn bark(hz: f64) -> f64 {
    13.0 * (0.00076 * hz).atan() + 3.5 * (hz / 7500.0).powi(2).atan()
}

/// Centre frequency of band `z`: the inverse of `bark` at z + 0.5.
pub fn centre_hz(z: usize) -> f64 {
    let (mut low, mut high) = (0.0, 24000.0);
    for _ in 0..50 {
        let mid = 0.5 * (low + high);
        if bark(mid) < z as f64 + 0.5 {
            low = mid;
        } else {
            high = mid;
        }
    }
    0.5 * (low + high)
}

/// Threshold in quiet in dB SPL (Terhardt 1979).
pub fn threshold_in_quiet(hz: f64) -> f64 {
    let k = hz.max(20.0) / 1000.0;
    3.64 * k.powf(-0.8) - 6.5 * (-0.6 * (k - 3.3).powi(2)).exp() + 1e-3 * k.powi(4)
}

/// Spreading function between critical bands as a power ratio (Schroeder,
/// as used by Johnston 1988). `dz` is maskee minus masker in Bark.
pub fn spreading(dz: f64) -> f64 {
    let x = dz + 0.474;
    10f64.powf((15.81 + 7.5 * x - 17.5 * (1.0 + x * x).sqrt()) / 10.0)
}

/// Band powers of the audio per frame, and the spectral flatness of each
/// frame in dB.
#[derive(Clone, Default)]
pub struct Frames {
    pub powers: Vec<[f64; BANDS]>,
    pub flatness_db: Vec<f64>,
}

/// Band powers of one channel, or of both channels summed, per frame of
/// `audio` at `rate`.
pub fn frames(audio: &[Stereo], channels: Channels, rate: u32) -> Frames {
    frames_of(audio.len(), |n| audio[n], channels, rate)
}

/// `frames` of the `length` frames at `rate` that `sample` gives by index.
pub fn frames_of(
    length: usize,
    sample: impl Fn(usize) -> Stereo + Sync,
    channels: Channels,
    rate: u32,
) -> Frames {
    let measure = frame_measure(channels, rate);
    let (flatness_db, powers) = crate::stft::stereo_frames_of(length, FFT, HOP, sample, &measure)
        .into_iter()
        .unzip();
    Frames {
        powers,
        flatness_db,
    }
}

/// `frames_of` of the two signals of `length` frames at `rate` that `sample`
/// gives by index, in one pass over the frames.
pub fn frame_pairs_of(
    length: usize,
    sample: impl Fn(usize) -> [Stereo; 2] + Sync,
    channels: Channels,
    rate: u32,
) -> (Frames, Frames) {
    let measure = frame_measure(channels, rate);
    let (first, second) = crate::stft::stereo_frame_pairs_of(length, FFT, HOP, sample, &measure);
    let frames = |measured: Vec<Measured>| {
        let (flatness_db, powers) = measured.into_iter().unzip();
        Frames {
            powers,
            flatness_db,
        }
    };
    (frames(first), frames(second))
}

/// The spectral flatness in dB and the band powers of one frame.
type Measured = (f64, [f64; BANDS]);

/// The `Measured` of one frame of `channels` at `rate`, from its left and
/// right spectra.
fn frame_measure(
    channels: Channels,
    rate: u32,
) -> impl Fn(&[Complex<f64>], &[Complex<f64>]) -> Measured + Sync {
    let bands: Vec<usize> = (0..=FFT / 2)
        .map(|bin| (bark(bin as f64 * rate as f64 / FFT as f64) as usize).min(BANDS - 1))
        .collect();
    let flat_bins = ((FLATNESS_MAX_HZ * FFT as f64 / rate as f64).floor() as usize).min(FFT / 2);
    move |a, b| {
        let mut p = [0.0; BANDS];
        let (mut log_sum, mut sum, mut count) = (0.0f64, 0.0f64, 0usize);
        for (bin, band) in bands.iter().enumerate().skip(1) {
            let power = match channels {
                Channels::Both => a[bin].norm_sqr() + b[bin].norm_sqr(),
                Channels::Left => a[bin].norm_sqr(),
                Channels::Right => b[bin].norm_sqr(),
            };
            p[*band] += power;
            if bin <= flat_bins {
                log_sum += power.max(1e-30).ln();
                sum += power;
                count += 1;
            }
        }
        let geometric = (log_sum / count as f64).exp();
        (
            10.0 * (geometric / (sum / count as f64).max(1e-30)).log10(),
            p,
        )
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Channels {
    Both,
    Left,
    Right,
}

/// Band power of a full-scale 1 kHz sine for `channels`, the reference of
/// the threshold in quiet, at `rate`.
fn full_scale(channels: Channels, rate: u32) -> f64 {
    let sine: Vec<Stereo> = (0..4 * FFT)
        .map(|i| {
            let x = (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / rate as f64).sin() as f32;
            [x, x]
        })
        .collect();
    frames(&sine, channels, rate)
        .powers
        .last()
        .map_or(1.0, |p| p.iter().sum::<f64>())
}

/// Masking threshold per frame and band from the masker's frames. The
/// threshold is the larger of the simultaneous threshold, the forward
/// masking of earlier frames and the threshold in quiet. The frames are of
/// audio at `rate`.
pub fn thresholds(masker: &Frames, channels: Channels, rate: u32) -> Vec<[f64; BANDS]> {
    let smoothing: Vec<f64> = (0..BANDS)
        .map(|z| {
            let tau = 0.008 + 100.0 / centre_hz(z) * (0.030 - 0.008);
            (-(HOP as f64 / rate as f64) / tau).exp()
        })
        .collect();
    // `spreading(z - m)` for each pair of bands, taken once.
    let gains: [[f64; BANDS]; BANDS] =
        std::array::from_fn(|z| std::array::from_fn(|m| spreading(z as f64 - m as f64)));
    // Spreading gain of a flat spectrum, so the spread energy keeps its level.
    let normal: Vec<f64> = gains.iter().map(|row| row.iter().sum()).collect();
    let reference = full_scale(channels, rate);
    let quiet: Vec<f64> = (0..BANDS)
        .map(|z| reference * 10f64.powf((threshold_in_quiet(centre_hz(z)) - FULL_SCALE_SPL) / 10.0))
        .collect();
    let mut forward = [0.0f64; BANDS];
    masker
        .powers
        .iter()
        .zip(&masker.flatness_db)
        .map(|(p, sfm)| {
            let tonality = (sfm / -60.0).clamp(0.0, 1.0);
            std::array::from_fn(|z| {
                let spread: f64 = (0..BANDS).map(|m| p[m] * gains[z][m]).sum::<f64>() / normal[z];
                let offset = tonality * (14.5 + z as f64) + (1.0 - tonality) * 5.5;
                let simultaneous = spread * 10f64.powf(-offset / 10.0);
                forward[z] = smoothing[z] * forward[z] + (1.0 - smoothing[z]) * simultaneous;
                simultaneous.max(forward[z]).max(quiet[z])
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(hz: f64, amplitude: f64, frames: usize) -> Vec<Stereo> {
        (0..frames)
            .map(|i| {
                let x = (amplitude * (2.0 * std::f64::consts::PI * hz * i as f64 / 48000.0).sin())
                    as f32;
                [x, x]
            })
            .collect()
    }

    #[test]
    fn a_tone_masks_its_own_band_most() {
        let masker = frames(&tone(1000.0, 0.5, 20 * HOP), Channels::Both, 48000);
        let threshold = thresholds(&masker, Channels::Both, 48000);
        let last = threshold.last().unwrap();
        let own = bark(1000.0) as usize;
        assert!(last[own] > last[own + 4] * 100.0, "{:?}", last);
        assert!(last[own] > last[own.saturating_sub(4)] * 100.0);
        // The threshold sits under the masker's energy in its band.
        assert!(last[own] < masker.powers.last().unwrap()[own]);
    }

    #[test]
    fn forward_masking_decays_after_the_masker_stops() {
        let mut audio = tone(1000.0, 0.5, 20 * HOP);
        audio.extend(vec![[0.0f32; 2]; 20 * HOP]);
        let masker = frames(&audio, Channels::Both, 48000);
        let threshold = thresholds(&masker, Channels::Both, 48000);
        let own = bark(1000.0) as usize;
        let at_stop = threshold[19][own];
        let later = threshold[22][own];
        let silent = threshold[threshold.len() - 1][own];
        assert!(
            at_stop > later && later > silent,
            "{at_stop} {later} {silent}"
        );
    }

    #[test]
    fn flatness_ignores_content_above_20_khz() {
        // White noise, and the same noise with the band above 20 kHz
        // removed by a brick-wall filter in the frequency domain.
        let rate = 48_000;
        let n = 1 << 16;
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        let mut noise = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            (seed >> 11) as f64 / (1u64 << 53) as f64 - 0.5
        };
        let full: Vec<f64> = (0..n).map(|_| noise()).collect();
        let mut spectrum: Vec<rustfft::num_complex::Complex<f64>> =
            full.iter().map(|&x| x.into()).collect();
        let mut planner = rustfft::FftPlanner::new();
        planner.plan_fft_forward(n).process(&mut spectrum);
        let cut = (20_500.0 * n as f64 / rate as f64) as usize;
        for (bin, value) in spectrum.iter_mut().enumerate() {
            if bin.min(n - bin) > cut {
                *value = 0.0.into();
            }
        }
        planner.plan_fft_inverse(n).process(&mut spectrum);
        let stereo = |x: &[f64]| -> Vec<Stereo> { x.iter().map(|&v| [v as f32; 2]).collect() };
        let cut_audio: Vec<f64> = spectrum.iter().map(|v| v.re / n as f64).collect();
        let mean = |f: &Frames| {
            let inner = &f.flatness_db[4..f.flatness_db.len() - 4];
            inner.iter().sum::<f64>() / inner.len() as f64
        };
        let a = mean(&frames(&stereo(&full), Channels::Both, rate));
        let b = mean(&frames(&stereo(&cut_audio), Channels::Both, rate));
        assert!(a > -6.0, "{a}");
        assert!((a - b).abs() < 0.1, "{a} {b}");
    }

    #[test]
    fn silence_leaves_the_threshold_in_quiet() {
        let masker = frames(&vec![[0.0f32; 2]; 10 * HOP], Channels::Both, 48000);
        let threshold = thresholds(&masker, Channels::Both, 48000);
        let last = threshold.last().unwrap();
        // The ear is most sensitive near 3 to 4 kHz.
        let sensitive = bark(3500.0) as usize;
        assert!(last[sensitive] < last[0] && last[sensitive] < last[BANDS - 1]);
    }
}
