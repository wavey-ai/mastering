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
    fill(&mut output, |i| kernel.at(input, i));
    Ok(output)
}

/// `convert` of `input` into the buffer of `output`, which a caller with
/// several recordings of one length can use for each of them.
pub fn convert_into(
    input: &[Stereo],
    source_rate: u32,
    target_rate: u32,
    output: &mut Vec<Stereo>,
) -> Result<()> {
    if !RATES.contains(&source_rate)
        || !RATES.contains(&target_rate)
        || input.iter().flatten().any(|x| !x.is_finite())
    {
        return Err("Resampling requires finite stereo and a supported sample rate".into());
    }
    output.clear();
    if source_rate == target_rate {
        output.extend_from_slice(input);
        return Ok(());
    }
    let kernel = Kernel::new(input.len(), source_rate, target_rate);
    output.resize(kernel.frames, [0.0; 2]);
    fill(output, |i| kernel.at(input, i));
    Ok(())
}

/// Frames of `frames` frames at `source_rate` after `convert` to
/// `target_rate`.
pub fn converted_frames(frames: usize, source_rate: u32, target_rate: u32) -> usize {
    if source_rate == target_rate {
        return frames;
    }
    (frames as f64 * target_rate as f64 / source_rate as f64).round() as usize
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
    fn at(&self, input: &[Stereo], i: usize) -> Stereo {
        let radius = self.radius;
        let numerator = i as u64 * self.q;
        let base = (numerator / self.p) as isize;
        let (taps, total) = &self.rows[(numerator % self.p) as usize];
        let first = base - radius;
        let mut frame = [0.0; 2];
        if first >= 0 && (base + radius) < input.len() as isize {
            for (coefficient, x) in taps.iter().zip(&input[first as usize..]) {
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
                let input = &audio[..inputs];
                fill(&mut block[..end - start], |k| self.at(input, start + k));
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
                let input = &audio[..inputs];
                fill(&mut block[..end - start], |k| self.at(input, start + k));
                audio[start..end].copy_from_slice(&block[..end - start]);
                start = end;
            }
            audio.truncate(outputs);
            audio.shrink_to_fit();
        }
        audio
    }
}

/// Write `sample(i)` to each `output[i]`. Each sample is independent, so
/// contiguous parts are filled on the available cores in place.
fn fill(output: &mut [Stereo], sample: impl Fn(usize) -> Stereo + Sync) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let threads = crate::dsp::threads()
            .min(8)
            .min(output.len() / 65536)
            .max(1);
        if threads > 1 {
            let part = output.len().div_ceil(threads);
            std::thread::scope(|scope| {
                for (k, chunk) in output.chunks_mut(part).enumerate() {
                    let sample = &sample;
                    scope.spawn(move || {
                        for (j, frame) in chunk.iter_mut().enumerate() {
                            *frame = sample(k * part + j);
                        }
                    });
                }
            });
            return;
        }
    }
    for (i, frame) in output.iter_mut().enumerate() {
        *frame = sample(i);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_conversion_into_a_used_buffer_gives_the_samples_of_convert() {
        let input: Vec<Stereo> = (0..44_100 + 333)
            .map(|i| {
                let t = i as f32 / 44_100.0;
                [(t * 2_000.0).sin(), 0.3 * (t * 13_000.0).cos()]
            })
            .collect();
        let mut buffer = vec![[1.0f32; 2]; 7];
        for (from, to) in [(44_100, 48_000), (48_000, 44_100), (48_000, 48_000)] {
            let expected = convert(&input, from, to).unwrap();
            convert_into(&input, from, to, &mut buffer).unwrap();
            assert!(buffer
                .iter()
                .flatten()
                .map(|x| x.to_bits())
                .eq(expected.iter().flatten().map(|x| x.to_bits())));
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
