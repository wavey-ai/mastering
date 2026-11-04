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

/// Centre frequency of band `z`, a band under `BANDS`: the inverse of
/// `bark` at z + 0.5. The bisection of each band runs once, and the
/// band powers of each frame read the table (D296).
pub fn centre_hz(z: usize) -> f64 {
    static CENTRES: std::sync::OnceLock<[f64; BANDS]> = std::sync::OnceLock::new();
    CENTRES.get_or_init(|| std::array::from_fn(bisect_centre_hz))[z]
}

/// `centre_hz` of band `z` by 50 bisection steps of `bark`.
fn bisect_centre_hz(z: usize) -> f64 {
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
    /// Empty when the frames measure `Measures::Powers`.
    pub flatness_db: Vec<f64>,
}

/// The measures of each frame of `Frames`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Measures {
    /// The band powers.
    Powers,
    /// The band powers and the spectral flatness. The flatness takes the
    /// log of the power of each bin up to `FLATNESS_MAX_HZ` (D317).
    PowersAndFlatness,
}

/// Band powers of one channel, or of both channels summed, per frame of
/// `audio` at `rate`, with the flatness of each frame if `measures` holds it.
pub fn frames(audio: &[Stereo], channels: Channels, measures: Measures, rate: u32) -> Frames {
    frames_of(audio.len(), |n| audio[n], channels, measures, rate)
}

/// `frames` of the `length` frames at `rate` that `sample` gives by index.
pub fn frames_of(
    length: usize,
    sample: impl Fn(usize) -> Stereo + Sync,
    channels: Channels,
    measures: Measures,
    rate: u32,
) -> Frames {
    let measure = frame_measure(channels, measures, rate);
    let mut frames = Frames::with_capacity(crate::stft::frame_count(length / HOP, FFT, HOP));
    crate::stft::stereo_frames_each(length, FFT, HOP, sample, &measure, |measured| {
        frames.push(measured)
    });
    frames
}

/// `frames` of the frames of `frames`, each through `map`, read in spans.
pub(crate) fn frames_read(
    frames: &mut impl crate::scratch::Frames,
    map: impl Fn(Stereo) -> Stereo + Sync,
    channels: Channels,
    measures: Measures,
    rate: u32,
) -> crate::audio_io::Result<Frames> {
    let measure = frame_measure(channels, measures, rate);
    let mut measured =
        Frames::with_capacity(crate::stft::frame_count(frames.len() / HOP, FFT, HOP));
    crate::stft::stereo_frames_read(frames, FFT, HOP, map, &measure, |m| measured.push(m))?;
    Ok(measured)
}

/// `frames_of` of the two signals of `length` frames at `rate` that `sample`
/// gives by index, in one pass over the frames. Each signal takes its
/// `measures`.
pub fn frame_pairs_of(
    length: usize,
    sample: impl Fn(usize) -> [Stereo; 2] + Sync,
    channels: Channels,
    measures: [Measures; 2],
    rate: u32,
) -> (Frames, Frames) {
    frame_pairs_in(0..length / HOP, sample, channels, measures, rate)
}

/// `frame_pairs_of` with `read(start, pairs)`, which writes the pairs of
/// frames from `start` into `pairs`, `HOP` pairs at a time.
pub fn frame_pairs_read_of(
    length: usize,
    read: impl Fn(usize, &mut [[Stereo; 2]]) + Sync,
    channels: Channels,
    measures: [Measures; 2],
    rate: u32,
) -> (Frames, Frames) {
    frame_pairs_read_in(0..length / HOP, read, channels, measures, rate)
}

/// The frames `hops` of `frame_pairs_of`. When `hops.start` is a multiple
/// of `stft::PAIR_RANGE`, they equal those frames of the whole signal, so
/// the frames of a long signal can come from consecutive spans of it.
/// `sample` reads the frames of `pair_reads(hops)`.
pub fn frame_pairs_in(
    hops: std::ops::Range<usize>,
    sample: impl Fn(usize) -> [Stereo; 2] + Sync,
    channels: Channels,
    measures: [Measures; 2],
    rate: u32,
) -> (Frames, Frames) {
    let read = |start: usize, pairs: &mut [[Stereo; 2]]| {
        for (i, pair) in pairs.iter_mut().enumerate() {
            *pair = sample(start + i);
        }
    };
    frame_pairs_read_in(hops, read, channels, measures, rate)
}

/// `frame_pairs_in` with the reader of `frame_pairs_read_of`.
fn frame_pairs_read_in(
    hops: std::ops::Range<usize>,
    read: impl Fn(usize, &mut [[Stereo; 2]]) + Sync,
    channels: Channels,
    measures: [Measures; 2],
    rate: u32,
) -> (Frames, Frames) {
    let measure = measures.map(|measures| frame_measure(channels, measures, rate));
    let mut frames = [(); 2].map(|_| Frames::with_capacity(hops.len()));
    crate::stft::stereo_frame_pairs_each(hops, FFT, HOP, read, measure, |a, b| {
        a.into_iter().for_each(|measured| frames[0].push(measured));
        b.into_iter().for_each(|measured| frames[1].push(measured));
    });
    let [first, second] = frames;
    (first, second)
}

impl Frames {
    pub(crate) fn with_capacity(frames: usize) -> Self {
        Self {
            powers: Vec::with_capacity(frames),
            flatness_db: Vec::with_capacity(frames),
        }
    }

    fn push(&mut self, (flatness_db, powers): Measured) {
        self.flatness_db.extend(flatness_db);
        self.powers.push(powers);
    }

    /// The frames of `later` after these frames.
    pub(crate) fn append(&mut self, later: Frames) {
        self.flatness_db.extend(later.flatness_db);
        self.powers.extend(later.powers);
    }
}

/// The sample frames that `frame_pairs_in` reads for `hops`.
pub fn pair_reads(hops: std::ops::Range<usize>) -> std::ops::Range<usize> {
    crate::stft::pair_reads(hops, FFT, HOP)
}

/// The spectral flatness in dB, if measured, and the band powers of one
/// frame.
type Measured = (Option<f64>, [f64; BANDS]);

/// The `Measured` of `measures` of one frame of `channels` at `rate`, from
/// its left and right spectra.
fn frame_measure(
    channels: Channels,
    measures: Measures,
    rate: u32,
) -> impl Fn(&[Complex<f64>], &[Complex<f64>]) -> Measured + Sync {
    // The runs of consecutive bins from bin 1 with one band. The sum of a
    // band continues in a local over each run, with the same additions in
    // the same order as a sum in `p` for each bin (D323).
    let mut runs: Vec<(usize, std::ops::Range<usize>)> = Vec::new();
    for bin in 1..=FFT / 2 {
        let band = (bark(bin as f64 * rate as f64 / FFT as f64) as usize).min(BANDS - 1);
        match runs.last_mut() {
            Some((last, bins)) if *last == band => bins.end = bin + 1,
            _ => runs.push((band, bin..bin + 1)),
        }
    }
    // The loop starts at bin 1, so the band powers alone take no log.
    let flat_bins = match measures {
        Measures::Powers => 0,
        Measures::PowersAndFlatness => {
            ((FLATNESS_MAX_HZ * FFT as f64 / rate as f64).floor() as usize).min(FFT / 2)
        }
    };
    move |a, b| {
        let mut p = [0.0; BANDS];
        let (mut log_sum, mut sum, mut count) = (0.0f64, 0.0f64, 0usize);
        for (band, bins) in &runs {
            let mut total = p[*band];
            for bin in bins.clone() {
                let power = match channels {
                    Channels::Both => a[bin].norm_sqr() + b[bin].norm_sqr(),
                    Channels::Left => a[bin].norm_sqr(),
                    Channels::Right => b[bin].norm_sqr(),
                };
                total += power;
                if bin <= flat_bins {
                    log_sum += power.max(1e-30).ln();
                    sum += power;
                    count += 1;
                }
            }
            p[*band] = total;
        }
        let flatness = (measures == Measures::PowersAndFlatness).then(|| {
            let geometric = (log_sum / count as f64).exp();
            10.0 * (geometric / (sum / count as f64).max(1e-30)).log10()
        });
        (flatness, p)
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
    frames(&sine, channels, Measures::Powers, rate)
        .powers
        .last()
        .map_or(1.0, |p| p.iter().sum::<f64>())
}

/// Masking threshold per frame and band from the masker's frames. The
/// threshold is the larger of the simultaneous threshold, the forward
/// masking of earlier frames and the threshold in quiet. The frames are of
/// audio at `rate`.
pub fn thresholds(masker: &Frames, channels: Channels, rate: u32) -> Vec<[f64; BANDS]> {
    let mut thresholds = Thresholds::new(channels, rate);
    masker
        .powers
        .iter()
        .zip(&masker.flatness_db)
        .map(|(p, sfm)| thresholds.next(p, *sfm))
        .collect()
}

/// The masking thresholds of `thresholds` frame by frame, with the forward
/// masking of the frames before.
pub struct Thresholds {
    smoothing: [f64; BANDS],
    /// `spreading(z - m)` for each pair of bands, taken once.
    gains: [[f64; BANDS]; BANDS],
    /// Spreading gain of a flat spectrum, so the spread energy keeps its level.
    normal: [f64; BANDS],
    quiet: [f64; BANDS],
    forward: [f64; BANDS],
}

impl Thresholds {
    pub fn new(channels: Channels, rate: u32) -> Self {
        let smoothing = std::array::from_fn(|z| {
            let tau = 0.008 + 100.0 / centre_hz(z) * (0.030 - 0.008);
            (-(HOP as f64 / rate as f64) / tau).exp()
        });
        let gains: [[f64; BANDS]; BANDS] =
            std::array::from_fn(|z| std::array::from_fn(|m| spreading(z as f64 - m as f64)));
        let normal = std::array::from_fn(|z| gains[z].iter().sum());
        let reference = full_scale(channels, rate);
        let quiet = std::array::from_fn(|z| {
            reference * 10f64.powf((threshold_in_quiet(centre_hz(z)) - FULL_SCALE_SPL) / 10.0)
        });
        Self {
            smoothing,
            gains,
            normal,
            quiet,
            forward: [0.0; BANDS],
        }
    }

    /// The threshold of the next frame, from its band powers `p` and its
    /// spectral flatness `sfm` in dB.
    pub fn next(&mut self, p: &[f64; BANDS], sfm: f64) -> [f64; BANDS] {
        let tonality = (sfm / -60.0).clamp(0.0, 1.0);
        std::array::from_fn(|z| {
            let spread: f64 =
                (0..BANDS).map(|m| p[m] * self.gains[z][m]).sum::<f64>() / self.normal[z];
            let offset = tonality * (14.5 + z as f64) + (1.0 - tonality) * 5.5;
            let simultaneous = spread * 10f64.powf(-offset / 10.0);
            self.forward[z] =
                self.smoothing[z] * self.forward[z] + (1.0 - self.smoothing[z]) * simultaneous;
            simultaneous.max(self.forward[z]).max(self.quiet[z])
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The runs of one band give the band powers and the flatness of a sum
    /// in `p` for each bin, bit for bit (D323).
    #[test]
    fn the_runs_of_a_band_equal_a_sum_for_each_bin() {
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
        for rate in [44_100, 48_000] {
            for channels in [Channels::Both, Channels::Left, Channels::Right] {
                for measures in [Measures::Powers, Measures::PowersAndFlatness] {
                    let bands: Vec<usize> = (0..=FFT / 2)
                        .map(|bin| {
                            (bark(bin as f64 * rate as f64 / FFT as f64) as usize).min(BANDS - 1)
                        })
                        .collect();
                    let flat_bins = match measures {
                        Measures::Powers => 0,
                        Measures::PowersAndFlatness => {
                            ((FLATNESS_MAX_HZ * FFT as f64 / rate as f64).floor() as usize)
                                .min(FFT / 2)
                        }
                    };
                    let (a, b) = (&spectra[0], &spectra[1]);
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
                    let flatness = (measures == Measures::PowersAndFlatness).then(|| {
                        let geometric = (log_sum / count as f64).exp();
                        10.0 * (geometric / (sum / count as f64).max(1e-30)).log10()
                    });
                    let (measured_flatness, measured) =
                        frame_measure(channels, measures, rate)(a, b);
                    assert_eq!(measured.map(f64::to_bits), p.map(f64::to_bits));
                    assert_eq!(
                        measured_flatness.map(f64::to_bits),
                        flatness.map(f64::to_bits)
                    );
                }
            }
        }
    }

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
    fn the_centre_of_each_band_is_its_bisection() {
        for z in 0..BANDS {
            assert_eq!(centre_hz(z).to_bits(), bisect_centre_hz(z).to_bits(), "{z}");
            assert!((bark(centre_hz(z)) - (z as f64 + 0.5)).abs() < 1e-9);
        }
    }

    #[test]
    fn a_tone_masks_its_own_band_most() {
        let masker = frames(
            &tone(1000.0, 0.5, 20 * HOP),
            Channels::Both,
            Measures::PowersAndFlatness,
            48000,
        );
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
        let masker = frames(&audio, Channels::Both, Measures::PowersAndFlatness, 48000);
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
        let flatness = Measures::PowersAndFlatness;
        let a = mean(&frames(&stereo(&full), Channels::Both, flatness, rate));
        let b = mean(&frames(&stereo(&cut_audio), Channels::Both, flatness, rate));
        assert!(a > -6.0, "{a}");
        assert!((a - b).abs() < 0.1, "{a} {b}");
    }

    #[test]
    fn silence_leaves_the_threshold_in_quiet() {
        let masker = frames(
            &vec![[0.0f32; 2]; 10 * HOP],
            Channels::Both,
            Measures::PowersAndFlatness,
            48000,
        );
        let threshold = thresholds(&masker, Channels::Both, 48000);
        let last = threshold.last().unwrap();
        // The ear is most sensitive near 3 to 4 kHz.
        let sensitive = bark(3500.0) as usize;
        assert!(last[sensitive] < last[0] && last[sensitive] < last[BANDS - 1]);
    }

    /// Frames of the band powers alone equal the frames with the flatness
    /// in their band powers, bit for bit (D317).
    #[test]
    fn the_powers_alone_equal_the_powers_with_the_flatness() {
        let mut state = 0x2545_f491u32;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as f32 / u32::MAX as f32 - 0.5
        };
        let pairs: Vec<[Stereo; 2]> = (0..40 * HOP + 77)
            .map(|_| [[next(), next()], [0.1 * next(), next()]])
            .collect();
        let bits = |f: &Frames| -> Vec<u64> {
            f.powers
                .iter()
                .flatten()
                .chain(&f.flatness_db)
                .map(|x| x.to_bits())
                .collect()
        };
        let one = |k: usize, measures| {
            frames_of(
                pairs.len(),
                |n| pairs[n][k],
                Channels::Both,
                measures,
                48000,
            )
        };
        let flat = one(0, Measures::PowersAndFlatness);
        let powers = one(0, Measures::Powers);
        assert!(powers.flatness_db.is_empty());
        assert_eq!(flat.flatness_db.len(), flat.powers.len());
        assert_eq!(
            bits(&powers),
            bits(&Frames {
                flatness_db: Vec::new(),
                ..flat.clone()
            })
        );
        let measures = [Measures::PowersAndFlatness, Measures::Powers];
        let (first, second) =
            frame_pairs_of(pairs.len(), |n| pairs[n], Channels::Both, measures, 48000);
        assert_eq!(bits(&first), bits(&flat));
        assert_eq!(bits(&second), bits(&one(1, Measures::Powers)));
    }
}
