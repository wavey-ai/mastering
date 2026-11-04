//! A short-time Fourier transform with the arithmetic of mel_spec's
//! `Spectrogram`: the same Hann window, hop buffer and FFT plan, so each
//! frame is bit-identical. It reuses its buffers; mel_spec allocates three
//! buffers for each frame.
use realfft::{RealFftPlanner, RealToComplex};
use rustfft::{num_complex::Complex, Fft, FftPlanner};
use std::{f64::consts::PI, sync::Arc};

pub struct Stft {
    fft: Arc<dyn Fft<f64>>,
    size: usize,
    hop: usize,
    filled: u64,
    hop_buf: Vec<f64>,
    window: Vec<f64>,
    input: Vec<Complex<f64>>,
    frame: Vec<Complex<f64>>,
    scratch: Vec<Complex<f64>>,
}

impl Stft {
    pub fn new(size: usize, hop: usize) -> Self {
        let fft = FftPlanner::new().plan_fft_forward(size);
        let scratch = vec![Complex::new(0.0, 0.0); fft.get_outofplace_scratch_len()];
        Self {
            fft,
            size,
            hop,
            filled: 0,
            hop_buf: vec![0.0; size],
            window: (0..size)
                .map(|i| 0.5 * (1.0 - f64::cos((2.0 * PI * i as f64) / size as f64)))
                .collect(),
            input: vec![Complex::new(0.0, 0.0); size],
            frame: vec![Complex::new(0.0, 0.0); size],
            scratch,
        }
    }

    /// Append `samples` (at most one hop, zero-padded to a hop) and return
    /// the spectrum of the last `size` samples once `size` have arrived.
    pub fn add(&mut self, samples: &[f32]) -> Option<&[Complex<f64>]> {
        self.add_with(samples.len(), |i| samples[i])
    }

    /// `add` of `count` samples that `sample` reads by index.
    pub fn add_with(
        &mut self,
        count: usize,
        sample: impl Fn(usize) -> f32,
    ) -> Option<&[Complex<f64>]> {
        assert!(count <= self.hop, "frames must be <= hop_size");
        let (size, hop) = (self.size, self.hop);
        self.hop_buf.copy_within(hop.., 0);
        for (i, value) in self.hop_buf[size - hop..].iter_mut().enumerate() {
            *value = if i < count { sample(i) as f64 } else { 0.0 };
        }
        self.filled = self.filled.wrapping_add(count as u64);
        if self.filled < size as u64 {
            return None;
        }
        for ((c, x), w) in self.input.iter_mut().zip(&self.hop_buf).zip(&self.window) {
            *c = Complex::new(x * w, 0.0);
        }
        // In place, rustfft transforms out of place into its scratch and
        // copies the result back; out of place skips the copy.
        self.fft.process_outofplace_with_scratch(
            &mut self.input,
            &mut self.frame,
            &mut self.scratch,
        );
        Some(&self.frame)
    }
}

/// Bins 0 to `size / 2` of the left and the right spectrum.
pub type StereoSpectra<'a> = (&'a [Complex<f64>], &'a [Complex<f64>]);

/// The transforms of a left and a right channel, each by a real-input FFT,
/// with the Hann window, hop buffer and frame count of `Stft`. It returns
/// bins 0 to `size / 2` of each channel, equal to those of `Stft` to
/// rounding, in half the work. Equal channels give equal spectra and opposite
/// channels opposite spectra, exactly.
pub struct StereoStft {
    fft: Arc<dyn RealToComplex<f64>>,
    size: usize,
    hop: usize,
    filled: u64,
    hop_buf: Vec<[f64; 2]>,
    window: Vec<f64>,
    input: Vec<f64>,
    scratch: Vec<Complex<f64>>,
    left: Vec<Complex<f64>>,
    right: Vec<Complex<f64>>,
}

impl StereoStft {
    pub fn new(size: usize, hop: usize) -> Self {
        let fft = RealFftPlanner::new().plan_fft_forward(size);
        Self {
            size,
            hop,
            filled: 0,
            hop_buf: vec![[0.0; 2]; size],
            window: (0..size)
                .map(|i| 0.5 * (1.0 - f64::cos((2.0 * PI * i as f64) / size as f64)))
                .collect(),
            input: fft.make_input_vec(),
            scratch: fft.make_scratch_vec(),
            left: fft.make_output_vec(),
            right: fft.make_output_vec(),
            fft,
        }
    }

    /// Append `count` frames (at most one hop, zero-padded to a hop) that
    /// `frame` reads by index, and return the left and right spectra of the
    /// last `size` frames once `size` have arrived.
    pub fn add_with(
        &mut self,
        count: usize,
        frame: impl Fn(usize) -> crate::audio_io::Stereo,
    ) -> Option<StereoSpectra<'_>> {
        assert!(count <= self.hop, "frames must be <= hop_size");
        let (size, hop) = (self.size, self.hop);
        self.hop_buf.copy_within(hop.., 0);
        for (i, value) in self.hop_buf[size - hop..].iter_mut().enumerate() {
            *value = if i < count {
                frame(i).map(f64::from)
            } else {
                [0.0; 2]
            };
        }
        self.filled = self.filled.wrapping_add(count as u64);
        if self.filled < size as u64 {
            return None;
        }
        for (c, output) in [(0, &mut self.left), (1, &mut self.right)] {
            for ((x, frame), w) in self.input.iter_mut().zip(&self.hop_buf).zip(&self.window) {
                *x = frame[c] * w;
            }
            self.fft
                .process_with_scratch(&mut self.input, output, &mut self.scratch)
                .expect("buffer lengths from the plan");
        }
        Some((&self.left, &self.right))
    }
}

/// The frames of the sequential loop
///
/// ```text
/// for hop in audio.chunks_exact(hop) {
///     if let (Some(l), Some(r)) = (left.add(left_of(hop)), right.add(right_of(hop))) {
///         frames.push(frame(l, r));
///     }
/// }
/// ```
///
/// computed on the available cores. A transform holds only its last `size`
/// samples, so each range of hops starts after the hops before it that fill
/// all but one hop of the window; those yield no frame.
pub fn stereo_frames<T: Send>(
    audio: &[crate::audio_io::Stereo],
    size: usize,
    hop: usize,
    frame: impl Fn(&[Complex<f64>], &[Complex<f64>]) -> T + Sync,
) -> Vec<T> {
    stereo_frames_of(audio.len(), size, hop, |n| audio[n], frame)
}

/// `stereo_frames` of the `length` frames that `sample` gives by index. A
/// caller that derives each frame from other audio holds no copy of the
/// derived audio.
pub fn stereo_frames_of<T: Send>(
    length: usize,
    size: usize,
    hop: usize,
    sample: impl Fn(usize) -> crate::audio_io::Stereo + Sync,
    frame: impl Fn(&[Complex<f64>], &[Complex<f64>]) -> T + Sync,
) -> Vec<T> {
    let mut frames = Vec::with_capacity(frame_count(length / hop, size, hop));
    stereo_frames_each(length, size, hop, sample, frame, |value| frames.push(value));
    frames
}

/// Hops in each range of the frames of a thread. Each range starts its
/// transform `size / hop` hops early, so its frames do not depend on the
/// ranges before it.
const RANGE: usize = 256;

/// Ranges of each thread in a batch of `stereo_frames_each`.
const BATCH: usize = 4;

/// The frames of `stereo_frames_of`, in order, to `sink`. The frames of a
/// batch of ranges compute on the available cores, and `sink` takes them
/// before the next batch: the frames of all ranges at once, beside their
/// joined copy, held each frame twice.
pub fn stereo_frames_each<T: Send>(
    length: usize,
    size: usize,
    hop: usize,
    sample: impl Fn(usize) -> crate::audio_io::Stereo + Sync,
    frame: impl Fn(&[Complex<f64>], &[Complex<f64>]) -> T + Sync,
    sink: impl FnMut(T),
) {
    stereo_frames_in(0..length / hop, size, hop, sample, frame, sink);
}

/// The frames `hops` of `stereo_frames_each`, to `sink`. When `hops.start`
/// is a multiple of `RANGE`, they equal those frames of the whole signal.
/// `sample` reads the frames of `reads(hops, size, hop)`.
fn stereo_frames_in<T: Send>(
    hops: std::ops::Range<usize>,
    size: usize,
    hop: usize,
    sample: impl Fn(usize) -> crate::audio_io::Stereo + Sync,
    frame: impl Fn(&[Complex<f64>], &[Complex<f64>]) -> T + Sync,
    mut sink: impl FnMut(T),
) {
    let warm = size.div_ceil(hop) - 1;
    let batch = RANGE * BATCH * crate::dsp::threads().min(8);
    for first in (hops.start..hops.end).step_by(batch) {
        let parts = crate::dsp::chunked(batch.min(hops.end - first), RANGE, |part| {
            let range = first + part.start..first + part.end;
            let mut stft = StereoStft::new(size, hop);
            let mut out = Vec::with_capacity(range.len());
            for index in range.start.saturating_sub(warm)..range.end {
                let start = index * hop;
                if let Some((l, r)) = stft.add_with(hop, |i| sample(start + i)) {
                    if index >= range.start {
                        out.push(frame(l, r));
                    }
                }
            }
            out
        });
        for part in parts {
            part.into_iter().for_each(&mut sink);
        }
    }
}

/// The frames of `stereo_frames_each` of the frames of `frames`, each
/// through `map`, read in spans of one range for each thread: a span holds
/// 4.2 MB of frames for each thread at a hop of 1,024 frames.
pub(crate) fn stereo_frames_read<T: Send>(
    frames: &mut impl crate::scratch::Frames,
    size: usize,
    hop: usize,
    map: impl Fn(crate::audio_io::Stereo) -> crate::audio_io::Stereo + Sync,
    frame: impl Fn(&[Complex<f64>], &[Complex<f64>]) -> T + Sync,
    mut sink: impl FnMut(T),
) -> crate::audio_io::Result<()> {
    let hops = frames.len() / hop;
    let span = RANGE * crate::dsp::threads().min(8);
    let mut buffer = Vec::new();
    for first in (0..hops).step_by(span) {
        let range = first..(first + span).min(hops);
        let reads = reads(range.clone(), size, hop);
        let window = frames.window(reads.start, reads.len(), &mut buffer)?;
        let sample = |n: usize| map(window[n - reads.start]);
        stereo_frames_in(range, size, hop, sample, &frame, &mut sink);
    }
    Ok(())
}

/// The sample frames that `stereo_frames_in` reads for `hops`.
fn reads(hops: std::ops::Range<usize>, size: usize, hop: usize) -> std::ops::Range<usize> {
    let warm = size.div_ceil(hop) - 1;
    hops.start.saturating_sub(warm) * hop..hops.end * hop
}

/// The frames that `hops` hops give: the first `size / hop - 1` hops only
/// fill the window.
pub fn frame_count(hops: usize, size: usize, hop: usize) -> usize {
    hops.saturating_sub(size.div_ceil(hop) - 1)
}

/// `stereo_frames_of` of the two signals of `length` frames that `sample`
/// gives by index, in one pass: each pair of frames is formed once for both
/// transforms. Signal `k` takes the frame function `frame[k]`.
pub fn stereo_frame_pairs_of<T: Send>(
    length: usize,
    size: usize,
    hop: usize,
    sample: impl Fn(usize) -> [crate::audio_io::Stereo; 2] + Sync,
    frame: [impl Fn(&[Complex<f64>], &[Complex<f64>]) -> T + Sync; 2],
) -> (Vec<T>, Vec<T>) {
    let read = |start: usize, pairs: &mut [[crate::audio_io::Stereo; 2]]| {
        for (i, pair) in pairs.iter_mut().enumerate() {
            *pair = sample(start + i);
        }
    };
    stereo_frame_pairs_in(0..length / hop, size, hop, read, frame)
}

/// Hops in each part of `stereo_frame_pairs_in`. Each part starts its
/// transforms `size / hop` hops early, so its frames do not depend on the
/// parts before it.
pub const PAIR_RANGE: usize = RANGE;

/// The frames `hops` of `stereo_frame_pairs_of`. When `hops.start` is a
/// multiple of `PAIR_RANGE`, they equal those frames of the whole signal.
/// `read(start, pairs)` writes the pairs of frames from `start` into
/// `pairs`, `hop` pairs at a time, within `pair_reads(hops, size, hop)`.
pub fn stereo_frame_pairs_in<T: Send>(
    hops: std::ops::Range<usize>,
    size: usize,
    hop: usize,
    read: impl Fn(usize, &mut [[crate::audio_io::Stereo; 2]]) + Sync,
    frame: [impl Fn(&[Complex<f64>], &[Complex<f64>]) -> T + Sync; 2],
) -> (Vec<T>, Vec<T>) {
    let (mut first, mut second) = (
        Vec::with_capacity(hops.len()),
        Vec::with_capacity(hops.len()),
    );
    stereo_frame_pairs_each(hops, size, hop, read, frame, |a, b| {
        first.extend(a);
        second.extend(b);
    });
    (first, second)
}

/// The frames of `stereo_frame_pairs_in`, in order, to `sink`, which takes
/// the frames of the two signals of each part. The parts of a batch compute
/// on the available cores, as in `stereo_frames_each`.
pub fn stereo_frame_pairs_each<T: Send>(
    hops: std::ops::Range<usize>,
    size: usize,
    hop: usize,
    read: impl Fn(usize, &mut [[crate::audio_io::Stereo; 2]]) + Sync,
    frame: [impl Fn(&[Complex<f64>], &[Complex<f64>]) -> T + Sync; 2],
    mut sink: impl FnMut(Vec<T>, Vec<T>),
) {
    let warm = size.div_ceil(hop) - 1;
    let batch = PAIR_RANGE * BATCH * crate::dsp::threads().min(8);
    for first in (0..hops.len()).step_by(batch) {
        let parts = crate::dsp::chunked(batch.min(hops.len() - first), PAIR_RANGE, |part| {
            let range = hops.start + first + part.start..hops.start + first + part.end;
            let mut stfts = [StereoStft::new(size, hop), StereoStft::new(size, hop)];
            let mut pairs = vec![[[0.0f32; 2]; 2]; hop];
            let mut out = (
                Vec::with_capacity(range.len()),
                Vec::with_capacity(range.len()),
            );
            for index in range.start.saturating_sub(warm)..range.end {
                read(index * hop, &mut pairs);
                if let Some((l, r)) = stfts[0].add_with(hop, |i| pairs[i][0]) {
                    if index >= range.start {
                        out.0.push(frame[0](l, r));
                    }
                }
                if let Some((l, r)) = stfts[1].add_with(hop, |i| pairs[i][1]) {
                    if index >= range.start {
                        out.1.push(frame[1](l, r));
                    }
                }
            }
            out
        });
        for (a, b) in parts {
            sink(a, b);
        }
    }
}

/// The sample frames that `stereo_frame_pairs_in` reads for `hops`.
pub fn pair_reads(hops: std::ops::Range<usize>, size: usize, hop: usize) -> std::ops::Range<usize> {
    reads(hops, size, hop)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mel_spec::stft::Spectrogram;

    #[test]
    fn paired_frames_equal_the_frames_of_each_signal() {
        let mut state = 0x3c6e_f372u32;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as f32 / u32::MAX as f32 - 0.5
        };
        let pairs: Vec<[crate::audio_io::Stereo; 2]> = (0..1024 * 600 + 77)
            .map(|_| [[next(), next()], [next(), 0.1 * next()]])
            .collect();
        let bits = |l: &[Complex<f64>], r: &[Complex<f64>]| -> Vec<u64> {
            l.iter()
                .chain(r)
                .flat_map(|c| [c.re.to_bits(), c.im.to_bits()])
                .collect()
        };
        // A hop of 16 frames gives several batches of ranges.
        for (size, hop) in [(2048, 1024), (4096, 1024), (64, 16)] {
            let (first, second) =
                stereo_frame_pairs_of(pairs.len(), size, hop, |n| pairs[n], [bits; 2]);
            let one = |k: usize| stereo_frames_of(pairs.len(), size, hop, |n| pairs[n][k], bits);
            assert_eq!(first, one(0), "{size} {hop}");
            assert_eq!(second, one(1), "{size} {hop}");
        }
    }

    #[test]
    fn frames_on_several_cores_match_the_sequential_loop() {
        let mut state = 0x9e37_79b9u32;
        let audio: Vec<crate::audio_io::Stereo> = (0..1024 * 700 + 333)
            .map(|_| {
                let mut next = || {
                    state ^= state << 13;
                    state ^= state >> 17;
                    state ^= state << 5;
                    state as f32 / u32::MAX as f32 - 0.5
                };
                [next(), next()]
            })
            .collect();
        // A hop of 16 frames gives several batches of ranges.
        for (size, hop) in [(4096, 1024), (2048, 1024), (8192, 4096), (64, 16)] {
            let bits = |l: &[Complex<f64>], r: &[Complex<f64>]| -> Vec<u64> {
                l.iter()
                    .chain(r)
                    .flat_map(|c| [c.re.to_bits(), c.im.to_bits()])
                    .collect()
            };
            let mut stft = StereoStft::new(size, hop);
            let mut expected = Vec::new();
            for samples in audio.chunks_exact(hop) {
                if let Some((l, r)) = stft.add_with(hop, |i| samples[i]) {
                    expected.push(bits(l, r));
                }
            }
            assert_eq!(
                stereo_frames(&audio, size, hop, bits),
                expected,
                "{size} {hop}"
            );
            // The frames of a stored signal, read in spans (D245).
            let mut stored = crate::scratch::Pcm::temporary_of(&audio).unwrap();
            let mut read = Vec::new();
            stereo_frames_read(&mut stored, size, hop, |x| x, bits, |f| read.push(f)).unwrap();
            assert_eq!(read, expected, "{size} {hop}");
        }
    }

    #[test]
    fn stereo_frames_match_each_channel_to_rounding() {
        let mut state = 0x6a09_e667u32;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as f32 / u32::MAX as f32 - 0.5
        };
        for (size, hop) in [(2048, 1024), (8192, 2048), (8192, 8192), (4096, 1024)] {
            let mut stereo = StereoStft::new(size, hop);
            let (mut left, mut right) = (Stft::new(size, hop), Stft::new(size, hop));
            for n in 0..40 {
                let count = if n == 39 { hop / 3 } else { hop };
                // A quiet right channel, so its error relative to its own
                // level shows.
                let samples: Vec<crate::audio_io::Stereo> =
                    (0..count).map(|_| [next(), 1e-3 * next()]).collect();
                let l: Vec<f32> = samples.iter().map(|x| x[0]).collect();
                let r: Vec<f32> = samples.iter().map(|x| x[1]).collect();
                let (el, er) = (left.add(&l).map(<[_]>::to_vec), right.add(&r));
                let actual = stereo.add_with(count, |i| samples[i]);
                assert_eq!(el.is_some(), actual.is_some());
                if let (Some(el), Some(er), Some((al, ar))) = (el, er, actual) {
                    for (expected, actual) in [(&el[..], al), (er, ar)] {
                        let scale = expected[..=size / 2]
                            .iter()
                            .map(|c| c.norm())
                            .fold(0.0, f64::max);
                        for (x, y) in expected.iter().zip(actual) {
                            assert!((x - y).norm() <= 1e-12 * scale, "{size} {hop}");
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn equal_and_opposite_channels_give_exactly_equal_and_opposite_spectra() {
        let mut state = 0xbb67_ae85u32;
        let samples: Vec<f32> = (0..8192)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                state as f32 / u32::MAX as f32 - 0.5
            })
            .collect();
        for sign in [1.0f32, -1.0] {
            let mut stft = StereoStft::new(4096, 1024);
            for hop in samples.as_chunks::<1024>().0 {
                if let Some((l, r)) = stft.add_with(1024, |i| [hop[i], sign * hop[i]]) {
                    for (l, r) in l.iter().zip(r) {
                        // Equal as numbers: a negated zero bin is −0.
                        assert_eq!(*l, r * sign as f64);
                    }
                }
            }
        }
    }

    #[test]
    fn frames_match_mel_spec_bit_for_bit() {
        let mut state = 0x2545_f491u32;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            state as f32 / u32::MAX as f32 - 0.5
        };
        for (size, hop) in [(2048, 1024), (8192, 2048), (8192, 8192), (4096, 4096)] {
            let (mut ours, mut theirs) = (Stft::new(size, hop), Spectrogram::new(size, hop));
            for n in 0..40 {
                // A short final hop is zero-padded by both.
                let count = if n == 39 { hop / 3 } else { hop };
                let samples: Vec<f32> = (0..count).map(|_| next()).collect();
                let expected = theirs.add(&samples);
                let actual = ours.add(&samples);
                assert_eq!(expected.is_some(), actual.is_some());
                if let (Some(e), Some(a)) = (expected, actual) {
                    for (x, y) in e.iter().zip(a) {
                        assert_eq!(
                            (x.re.to_bits(), x.im.to_bits()),
                            (y.re.to_bits(), y.im.to_bits())
                        );
                    }
                }
            }
        }
    }
}
