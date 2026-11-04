use crate::audio_io::{Result, Stereo};
use std::f64::consts::PI;

pub const RATES: [u32; 6] = [44100, 48000, 88200, 96000, 176400, 192000];

/// `convert` of `input`, in the buffer of `input`: the conversion holds no
/// second copy of the recording. A buffer whose capacity holds the converted
/// frames does not grow (`converted_frames`).
pub fn convert_owned(
    input: Vec<Stereo>,
    source_rate: u32,
    target_rate: u32,
) -> Result<Vec<Stereo>> {
    if !RATES.contains(&source_rate)
        || !RATES.contains(&target_rate)
        || input.iter().flatten().any(|x| !x.is_finite())
    {
        return Err("Resampling requires finite stereo and a supported sample rate".into());
    }
    if source_rate == target_rate {
        return Ok(input);
    }
    Ok(Kernel::new(input.len(), source_rate, target_rate).convert_in_place(input))
}

pub fn convert(input: &[Stereo], source_rate: u32, target_rate: u32) -> Result<Vec<Stereo>> {
    if !RATES.contains(&source_rate)
        || !RATES.contains(&target_rate)
        || input.iter().flatten().any(|x| !x.is_finite())
    {
        return Err("Resampling requires finite stereo and a supported sample rate".into());
    }
    if source_rate == target_rate {
        return Ok(input.to_vec());
    }
    let kernel = Kernel::new(input.len(), source_rate, target_rate);
    let mut output = vec![[0.0f32; 2]; kernel.frames];
    fill(&mut output, |start, part| {
        kernel.fill_from(Input::whole(input), start, part)
    });
    Ok(output)
}

/// `input` at `source_rate` at the rate `target_rate`. A recording at
/// `target_rate` is read in place: a copy of a 30-minute stem is 0.7 GB.
pub fn at_rate(
    input: &[Stereo],
    source_rate: u32,
    target_rate: u32,
) -> Result<std::borrow::Cow<'_, [Stereo]>> {
    Ok(if source_rate == target_rate {
        input.into()
    } else {
        convert(input, source_rate, target_rate)?.into()
    })
}

/// Frames of `frames` frames at `source_rate` after `convert` to
/// `target_rate`.
pub fn converted_frames(frames: usize, source_rate: u32, target_rate: u32) -> usize {
    if source_rate == target_rate {
        return frames;
    }
    (frames as f64 * target_rate as f64 / source_rate as f64).round() as usize
}

/// Frames of each span of `convert_stored`.
pub(crate) const CONVERT_SPAN: usize = 1 << 19;

/// The recording in `input` at `source_rate`, converted to `target_rate` in
/// spans, in new scratch storage: frame `i` is frame `i` of `convert`
/// (D245, D250).
pub(crate) fn convert_stored(
    input: &mut impl crate::scratch::Frames,
    source_rate: u32,
    target_rate: u32,
) -> Result<crate::scratch::Pcm> {
    let at_rate = Windowed::new(input.len(), source_rate, target_rate)?;
    let mut stored = crate::scratch::Pcm::new(crate::scratch::temporary()?);
    let (mut window, mut output) = (Vec::new(), Vec::new());
    for start in (0..at_rate.frames()).step_by(CONVERT_SPAN) {
        let range = start..(start + CONVERT_SPAN).min(at_rate.frames());
        let source = at_rate.source_range(range.clone());
        input.read(source.start, source.len(), &mut window)?;
        output.clear();
        output.resize(range.len(), [0.0; 2]);
        at_rate.fill_from(&window, source.start, range.start, &mut output);
        stored.push(&output)?;
    }
    Ok(stored)
}

/// `convert` of a recording, read one frame at a time: a reader that takes
/// each frame once holds no converted copy.
pub struct Converted<'a> {
    input: &'a [Stereo],
    kernel: Option<Kernel>,
}

impl<'a> Converted<'a> {
    pub fn new(input: &'a [Stereo], source_rate: u32, target_rate: u32) -> Result<Self> {
        if !RATES.contains(&source_rate)
            || !RATES.contains(&target_rate)
            || input.iter().flatten().any(|x| !x.is_finite())
        {
            return Err("Resampling requires finite stereo and a supported sample rate".into());
        }
        let kernel = (source_rate != target_rate)
            .then(|| Kernel::new(input.len(), source_rate, target_rate));
        Ok(Self { input, kernel })
    }

    /// The frames of the converted recording.
    pub fn frames(&self) -> usize {
        self.kernel.as_ref().map_or(self.input.len(), |k| k.frames)
    }

    /// Frame `i` of the converted recording: frame `i` of `convert`.
    pub fn at(&self, i: usize) -> Stereo {
        match &self.kernel {
            Some(kernel) => kernel.at(Input::whole(self.input), i),
            None => self.input[i],
        }
    }

    /// `at` of each frame from `start` into `output`.
    pub fn fill_from(&self, start: usize, output: &mut [Stereo]) {
        match &self.kernel {
            Some(kernel) => kernel.fill_from(Input::whole(self.input), start, output),
            None => output.copy_from_slice(&self.input[start..start + output.len()]),
        }
    }
}

/// `convert` of a recording of a known length, from windows of its frames:
/// a reader of a stored recording holds no copy of the recording.
pub(crate) struct Windowed {
    kernel: Option<Kernel>,
    frames: usize,
}

impl Windowed {
    pub(crate) fn new(frames: usize, source_rate: u32, target_rate: u32) -> Result<Self> {
        if !RATES.contains(&source_rate) || !RATES.contains(&target_rate) {
            return Err("Resampling requires a supported sample rate".into());
        }
        let kernel =
            (source_rate != target_rate).then(|| Kernel::new(frames, source_rate, target_rate));
        Ok(Self { kernel, frames })
    }

    /// The frames of the converted recording.
    pub(crate) fn frames(&self) -> usize {
        self.kernel.as_ref().map_or(self.frames, |k| k.frames)
    }

    /// The source frames that the converted frames `range` read.
    pub(crate) fn source_range(&self, range: std::ops::Range<usize>) -> std::ops::Range<usize> {
        match &self.kernel {
            Some(kernel) if !range.is_empty() => {
                let radius = kernel.radius as usize;
                let first = kernel.base(range.start).saturating_sub(radius);
                first..(kernel.base(range.end - 1) + radius + 1).min(self.frames)
            }
            _ => range,
        }
    }

    /// The converted frames from `start` into `output`, from `window`, the
    /// source frames from frame `offset`. `window` holds the `source_range`
    /// of the frames of `output`.
    pub(crate) fn fill_from(
        &self,
        window: &[Stereo],
        offset: usize,
        start: usize,
        output: &mut [Stereo],
    ) {
        match &self.kernel {
            Some(kernel) => {
                let input = Input {
                    frames: window,
                    offset,
                    total: self.frames,
                };
                fill(output, |k, part| kernel.fill_from(input, start + k, part));
            }
            None => output.copy_from_slice(&window[start - offset..start - offset + output.len()]),
        }
    }
}

/// The source frames of a conversion: `frames` holds the frames from frame
/// `offset` of a recording of `total` frames.
#[derive(Clone, Copy)]
struct Input<'a> {
    frames: &'a [Stereo],
    offset: usize,
    total: usize,
}

impl<'a> Input<'a> {
    fn whole(frames: &'a [Stereo]) -> Self {
        Self {
            frames,
            offset: 0,
            total: frames.len(),
        }
    }

    /// The `count` frames from frame `start` of the recording.
    #[inline]
    fn slice(&self, start: usize, count: usize) -> &'a [Stereo] {
        &self.frames[start - self.offset..start - self.offset + count]
    }

    /// Frame `n` of the recording, or none outside it.
    #[inline]
    fn get(&self, n: usize) -> Option<&'a Stereo> {
        (n < self.total).then(|| &self.frames[n - self.offset])
    }
}

/// The windowed-sinc conversion of a recording of a known length between
/// two different rates.
struct Kernel {
    rows: Vec<(Vec<f64>, f64)>,
    radius: isize,
    p: u64,
    q: u64,
    frames: usize,
}

impl Kernel {
    fn new(input_frames: usize, source_rate: u32, target_rate: u32) -> Self {
        let ratio = target_rate as f64 / source_rate as f64;
        let frames = (input_frames as f64 * ratio).round() as usize;
        let cutoff = ratio.min(1.0) * 0.94;
        let radius = (48.0 / cutoff).ceil() as isize;
        // Output sample i sits at i·q/p source samples for the reduced ratio p/q.
        // Its fractional phase repeats every p samples, so one row of windowed
        // sinc taps per phase serves the whole signal.
        let gcd = |mut a: u64, mut b: u64| {
            while b != 0 {
                (a, b) = (b, a % b);
            }
            a
        };
        let common = gcd(target_rate as u64, source_rate as u64);
        let (p, q) = (target_rate as u64 / common, source_rate as u64 / common);
        let rows: Vec<(Vec<f64>, f64)> = (0..p)
            .map(|phase| {
                let offset = phase as f64 / p as f64;
                let taps: Vec<f64> = (-radius..=radius)
                    .map(|j| {
                        let t = j as f64 - offset;
                        let w = t / radius as f64;
                        if w.abs() > 1.0 {
                            return 0.0;
                        }
                        let sinc = if (cutoff * t).abs() < 1e-12 {
                            1.0
                        } else {
                            (PI * cutoff * t).sin() / (PI * cutoff * t)
                        };
                        cutoff * sinc * (0.42 + 0.5 * (PI * w).cos() + 0.08 * (2.0 * PI * w).cos())
                    })
                    .collect();
                let total = taps.iter().sum();
                (taps, total)
            })
            .collect();
        Self {
            rows,
            radius,
            p,
            q,
            frames,
        }
    }

    /// The input frame under output frame `i`.
    fn base(&self, i: usize) -> usize {
        (i as u64 * self.q / self.p) as usize
    }

    /// Output frame `i` of `input`.
    #[inline]
    fn at(&self, input: Input, i: usize) -> Stereo {
        let radius = self.radius;
        let numerator = i as u64 * self.q;
        let base = (numerator / self.p) as isize;
        let (taps, total) = &self.rows[(numerator % self.p) as usize];
        let first = base - radius;
        let mut frame = [0.0; 2];
        if first >= 0 && (base + radius) < input.total as isize {
            for (coefficient, x) in taps.iter().zip(input.slice(first as usize, taps.len())) {
                frame[0] += coefficient * x[0] as f64;
                frame[1] += coefficient * x[1] as f64;
            }
        } else {
            for (j, coefficient) in taps.iter().enumerate() {
                if let Some(x) = usize::try_from(first + j as isize)
                    .ok()
                    .and_then(|n| input.get(n))
                {
                    frame[0] += coefficient * x[0] as f64;
                    frame[1] += coefficient * x[1] as f64;
                }
            }
        }
        frame.map(|x| (x / total) as f32)
    }

    /// Whether the taps of output frame `i` are all in `input`.
    fn inside(&self, input: Input, i: usize) -> bool {
        let base = self.base(i) as isize;
        base >= self.radius && base + self.radius < input.total as isize
    }

    /// `at` of each output frame from `start` into `output`. Runs of
    /// `WINDOWS` frames whose taps are all in `input` take one pass over
    /// the taps, so the chains of additions of the frames run beside each
    /// other. Each frame keeps the order of its additions (D241).
    fn fill_from(&self, input: Input, start: usize, output: &mut [Stereo]) {
        const WINDOWS: usize = 4;
        let mut k = 0;
        while k < output.len() {
            let i = start + k;
            // `base` grows with the frame, so the first and the last frame
            // of a run bound the others.
            if k + WINDOWS <= output.len()
                && self.inside(input, i)
                && self.inside(input, i + WINDOWS - 1)
            {
                output[k..k + WINDOWS].copy_from_slice(&self.at_windows::<WINDOWS>(input, i));
                k += WINDOWS;
            } else {
                output[k] = self.at(input, i);
                k += 1;
            }
        }
    }

    /// `at` of the `M` output frames from `i`, whose taps are all in
    /// `input`, in one pass over the taps.
    #[inline(always)]
    fn at_windows<const M: usize>(&self, input: Input, i: usize) -> [Stereo; M] {
        let radius = self.radius as usize;
        let n = 2 * radius + 1;
        let rows: [(&[f64], f64, &[Stereo]); M] = std::array::from_fn(|j| {
            let numerator = (i + j) as u64 * self.q;
            let base = (numerator / self.p) as usize;
            let (taps, total) = &self.rows[(numerator % self.p) as usize];
            (&taps[..n], *total, input.slice(base - radius, n))
        });
        let mut frames = [[0.0f64; 2]; M];
        for t in 0..n {
            for (frame, (taps, _, samples)) in frames.iter_mut().zip(&rows) {
                let (coefficient, x) = (taps[t], samples[t]);
                frame[0] += coefficient * x[0] as f64;
                frame[1] += coefficient * x[1] as f64;
            }
        }
        std::array::from_fn(|j| frames[j].map(|x| (x / rows[j].1) as f32))
    }

    /// The output of `audio` in the buffer of `audio`, one block of output
    /// frames at a time. A block reads only input frames that no earlier
    /// block overwrote: the blocks go from the end when the output is
    /// longer than the input, and from the start when it is shorter.
    fn convert_in_place(&self, mut audio: Vec<Stereo>) -> Vec<Stereo> {
        const BLOCK: usize = 1 << 20;
        let (inputs, outputs) = (audio.len(), self.frames);
        let radius = self.radius as usize;
        let mut block = vec![[0.0f32; 2]; BLOCK.min(outputs)];
        if outputs > inputs {
            audio.resize(outputs, [0.0; 2]);
            let mut end = outputs;
            while end > 0 {
                // The block under this one reads up to base(start - 1) +
                // radius, which must be under `start`.
                let mut start = end.saturating_sub(BLOCK);
                while start > 0 && self.base(start - 1) + radius >= start {
                    start -= 1;
                }
                if block.len() < end - start {
                    block.resize(end - start, [0.0; 2]);
                }
                let input = Input::whole(&audio[..inputs]);
                fill(&mut block[..end - start], |k, part| {
                    self.fill_from(input, start + k, part)
                });
                audio[start..end].copy_from_slice(&block[..end - start]);
                end = start;
            }
        } else {
            let mut start = 0;
            while start < outputs {
                // The block over this one reads from base(end) - radius,
                // which must be at or over `end`.
                let mut end = (start + BLOCK).min(outputs);
                while end < outputs && self.base(end) < end + radius {
                    end += 1;
                }
                if block.len() < end - start {
                    block.resize(end - start, [0.0; 2]);
                }
                let input = Input::whole(&audio[..inputs]);
                fill(&mut block[..end - start], |k, part| {
                    self.fill_from(input, start + k, part)
                });
                audio[start..end].copy_from_slice(&block[..end - start]);
                start = end;
            }
            audio.truncate(outputs);
            audio.shrink_to_fit();
        }
        audio
    }
}

/// Fill `output` with `part(start, frames)`, which writes the frames from
/// `start`. Each frame is independent, so contiguous parts are filled on
/// the available cores in place.
fn fill(output: &mut [Stereo], part: impl Fn(usize, &mut [Stereo]) + Sync) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let threads = crate::dsp::threads()
            .min(8)
            .min(output.len() / 65536)
            .max(1);
        if threads > 1 {
            let size = output.len().div_ceil(threads);
            std::thread::scope(|scope| {
                for (k, chunk) in output.chunks_mut(size).enumerate() {
                    let part = &part;
                    scope.spawn(move || part(k * size, chunk));
                }
            });
            return;
        }
    }
    part(0, output);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_converted_reader_gives_the_samples_of_convert() {
        let input: Vec<Stereo> = (0..44_100 + 333)
            .map(|i| {
                let t = i as f32 / 44_100.0;
                [(t * 3_000.0).sin(), 0.4 * (t * 11_000.0).cos()]
            })
            .collect();
        for (from, to) in [(44_100, 48_000), (48_000, 44_100), (48_000, 48_000)] {
            let expected = convert(&input, from, to).unwrap();
            let reader = Converted::new(&input, from, to).unwrap();
            assert_eq!(reader.frames(), expected.len());
            assert!((0..reader.frames())
                .flat_map(|i| reader.at(i))
                .map(|x| x.to_bits())
                .eq(expected.iter().flatten().map(|x| x.to_bits())));
        }
    }

    #[test]
    fn the_frames_of_a_pass_equal_each_frame_bit_for_bit() {
        let input: Vec<Stereo> = (0..20_011)
            .map(|i| {
                let t = i as f32 / 44_100.0;
                [(t * 2_345.0).sin(), 0.3 * (t * 13_000.0).cos()]
            })
            .collect();
        for (from, to) in [
            (44_100, 48_000),
            (48_000, 44_100),
            (96_000, 48_000),
            (44_100, 96_000),
            (192_000, 44_100),
        ] {
            let reader = Converted::new(&input, from, to).unwrap();
            let frames = reader.frames();
            for (start, len) in [(0, frames), (0, 70), (3, 1_024), (57, 9), (frames - 81, 81)] {
                let mut output = vec![[0.0f32; 2]; len];
                reader.fill_from(start, &mut output);
                assert!(
                    (start..start + len)
                        .flat_map(|i| reader.at(i))
                        .map(|x| x.to_bits())
                        .eq(output.iter().flatten().map(|x| x.to_bits())),
                    "{from} {to} {start} {len}"
                );
            }
        }
    }

    /// Windows of a recording give the frames of `convert` (D245).
    #[test]
    fn windows_of_a_recording_give_the_samples_of_convert() {
        let input: Vec<Stereo> = (0..150_011)
            .map(|i| {
                let t = i as f32 / 44_100.0;
                [(t * 2_345.0).sin(), 0.3 * (t * 13_000.0).cos()]
            })
            .collect();
        for (from, to) in [
            (44_100, 48_000),
            (48_000, 44_100),
            (96_000, 48_000),
            (44_100, 96_000),
            (48_000, 48_000),
        ] {
            let expected = convert(&input, from, to).unwrap();
            let windowed = Windowed::new(input.len(), from, to).unwrap();
            let frames = windowed.frames();
            assert_eq!(frames, expected.len());
            for (start, len) in [
                (0, frames),
                (0, 70),
                (3, 1_024),
                (57, 9),
                (5_000, 140_000.min(frames - 5_000)),
                (frames - 81, 81),
            ] {
                let source = windowed.source_range(start..start + len);
                let mut output = vec![[0.0f32; 2]; len];
                windowed.fill_from(&input[source.clone()], source.start, start, &mut output);
                assert!(
                    output
                        .iter()
                        .flatten()
                        .map(|x| x.to_bits())
                        .eq(expected[start..start + len]
                            .iter()
                            .flatten()
                            .map(|x| x.to_bits())),
                    "{from} {to} {start} {len}"
                );
            }
        }
    }

    #[test]
    fn a_conversion_in_place_gives_the_samples_of_convert() {
        for frames in [100, 3_333, (1 << 20) + 4_321, 3 * (1 << 20) - 7] {
            let input: Vec<Stereo> = (0..frames)
                .map(|i| {
                    let t = i as f32 / 44_100.0;
                    [(t * 1_234.0).sin(), 0.5 * (t * 9_876.0).cos()]
                })
                .collect();
            for (from, to) in [
                (44_100, 48_000),
                (48_000, 44_100),
                (96_000, 48_000),
                (44_100, 44_100),
            ] {
                let expected = convert(&input, from, to).unwrap();
                let mut owned = input.clone();
                owned.reserve_exact(converted_frames(frames, from, to).saturating_sub(frames));
                let actual = convert_owned(owned, from, to).unwrap();
                assert_eq!(actual.len(), converted_frames(frames, from, to));
                assert!(
                    actual
                        .iter()
                        .flatten()
                        .map(|x| x.to_bits())
                        .eq(expected.iter().flatten().map(|x| x.to_bits())),
                    "{frames} {from} {to}"
                );
            }
        }
    }
}
