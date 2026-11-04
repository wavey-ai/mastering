//! Zero-phase FIR filtering of a whole signal by FFT convolution. The engine
//! renders offline, so a symmetric kernel centred on each sample has no phase
//! shift and no latency.
use rustfft::{num_complex::Complex, FftPlanner};

/// FFT length of the block convolution.
const FFT: usize = 16384;

/// The symmetric impulse response of `taps` (odd) samples whose magnitude
/// response is `response(hz)` at `rate`, Hann-windowed, centred at
/// `taps / 2`.
pub fn kernel(taps: usize, response: impl Fn(f64) -> f64, rate: u32) -> Vec<f64> {
    let n = 4 * (taps - 1);
    let mut spectrum: Vec<Complex<f64>> = (0..n)
        .map(|k| Complex::new(response(k.min(n - k) as f64 * rate as f64 / n as f64), 0.0))
        .collect();
    FftPlanner::new().plan_fft_inverse(n).process(&mut spectrum);
    let half = taps / 2;
    (0..taps)
        .map(|i| {
            let window =
                0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / (taps - 1) as f64).cos();
            spectrum[(i + n - half) % n].re / n as f64 * window
        })
        .collect()
}

/// The antisymmetric impulse response of `taps` (odd) samples whose response
/// is `amplitude(hz)` with a phase of +90° where `sign(hz)` is positive and
/// −90° where it is negative, at `rate`, Hann-windowed, centred at
/// `taps / 2`. Its output has no correlation with its input at any
/// frequency.
pub fn quadrature_kernel(
    taps: usize,
    amplitude: impl Fn(f64) -> f64,
    sign: impl Fn(f64) -> f64,
    rate: u32,
) -> Vec<f64> {
    let n = 4 * (taps - 1);
    let mut spectrum: Vec<Complex<f64>> = (0..n)
        .map(|k| {
            if k == 0 || 2 * k == n {
                return Complex::new(0.0, 0.0);
            }
            let hz = k.min(n - k) as f64 * rate as f64 / n as f64;
            let value = amplitude(hz) * sign(hz).signum();
            // Conjugate symmetry keeps the impulse response real.
            Complex::new(0.0, if 2 * k < n { value } else { -value })
        })
        .collect();
    FftPlanner::new().plan_fft_inverse(n).process(&mut spectrum);
    let half = taps / 2;
    (0..taps)
        .map(|i| {
            let window =
                0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / (taps - 1) as f64).cos();
            spectrum[(i + n - half) % n].re / n as f64 * window
        })
        .collect()
}

/// `signal` filtered by the centred `kernel`, at the same length.
pub fn filter(signal: &[f64], kernel: &[f64]) -> Vec<f64> {
    let mut out = vec![0.0; signal.len()];
    filter_each(
        signal.len(),
        |i| signal[i],
        kernel,
        |i, value| out[i] = value,
    );
    out
}

/// `filter` of the `len` samples that `sample` reads, with each output
/// sample passed to `emit` in index order. It holds one FFT block, not the
/// signal: a 30-minute signal is 0.7 GB as f64. `sample` reads ahead of the
/// last emitted index by less than one FFT block.
pub fn filter_each(
    len: usize,
    sample: impl Fn(usize) -> f64,
    kernel: &[f64],
    mut emit: impl FnMut(usize, f64),
) {
    filter_each_many(
        len,
        &[kernel],
        |i, input| input[0] = sample(i),
        |i, output| emit(i, output[0]),
    );
}

/// `filter_each` of several kernels of one length in one pass.
/// `sample(i, inputs)` sets the input of each kernel at `i`, once for each
/// `i` and in index order. `emit(i, outputs)` gets the output of each kernel
/// at `i`, in index order, after the last call of `sample` that output `i`
/// reads, and `sample` reads no index up to `i` after it. Thus `emit` can
/// write sample `i` of the signal that `sample` reads. Each output equals the
/// output of `filter_each` with its kernel.
pub fn filter_each_many(
    len: usize,
    kernels: &[&[f64]],
    mut sample: impl FnMut(usize, &mut [f64]),
    mut emit: impl FnMut(usize, &[f64]),
) {
    let mut stream = Stream::new(len, kernels);
    let mut input = vec![0.0f64; kernels.len()];
    for i in 0..len {
        sample(i, &mut input);
        stream.push(&input, &mut emit);
    }
    stream.finish(&mut emit);
}

/// `filter_each_many` in push form: the inputs of each index go in by
/// `push`, in index order, and `emit` gets each output once it is final, in
/// index order. A chain of stages passes each block of frames through each
/// stream in turn and holds no whole signal.
pub struct Stream {
    len: usize,
    taps: usize,
    block: usize,
    forward: std::sync::Arc<dyn rustfft::Fft<f64>>,
    inverse: std::sync::Arc<dyn rustfft::Fft<f64>>,
    responses: Vec<Vec<Complex<f64>>>,
    /// `pending[n][k]` holds full sample `start + k` of kernel `n`.
    pending: Vec<Vec<f64>>,
    outputs: Vec<f64>,
    /// The inputs of the block that starts at `start`, `filled` of them.
    inputs: Vec<Vec<f64>>,
    filled: usize,
    start: usize,
    buffer: Vec<Complex<f64>>,
    spectrum: Vec<Complex<f64>>,
    scratch: Vec<Complex<f64>>,
}

impl Stream {
    /// A stream of `len` inputs through each of `kernels`, of one odd length.
    pub fn new(len: usize, kernels: &[&[f64]]) -> Self {
        let taps = kernels[0].len();
        assert!(kernels.iter().all(|k| k.len() == taps));
        let block = FFT - taps + 1;
        let mut planner = FftPlanner::new();
        let (forward, inverse) = (planner.plan_fft_forward(FFT), planner.plan_fft_inverse(FFT));
        let responses = kernels
            .iter()
            .map(|kernel| {
                let mut response: Vec<Complex<f64>> = kernel
                    .iter()
                    .map(|h| Complex::new(*h, 0.0))
                    .chain(std::iter::repeat(Complex::new(0.0, 0.0)))
                    .take(FFT)
                    .collect();
                forward.process(&mut response);
                response
            })
            .collect();
        // Out of place with one scratch: in place, rustfft allocates its
        // scratch on each call, transforms into it and copies the result
        // back.
        let scratch = vec![
            Complex::new(0.0, 0.0);
            forward
                .get_outofplace_scratch_len()
                .max(inverse.get_outofplace_scratch_len())
        ];
        Self {
            len,
            taps,
            block,
            forward,
            inverse,
            responses,
            pending: vec![vec![0.0f64; FFT]; kernels.len()],
            outputs: vec![0.0f64; kernels.len()],
            inputs: vec![vec![0.0f64; block]; kernels.len()],
            filled: 0,
            start: 0,
            buffer: vec![Complex::new(0.0, 0.0); FFT],
            spectrum: vec![Complex::new(0.0, 0.0); FFT],
            scratch,
        }
    }

    /// Frames between the last input and the last final output: the outputs
    /// of the block in progress wait for its last input, and each output
    /// reads `taps / 2` inputs after it.
    pub fn latency(&self) -> usize {
        self.block + self.taps / 2
    }

    /// Add the input of each kernel at the next index.
    pub fn push(&mut self, input: &[f64], emit: &mut impl FnMut(usize, &[f64])) {
        assert!(self.start + self.filled < self.len, "input past the stream");
        for (inputs, x) in self.inputs.iter_mut().zip(input) {
            inputs[self.filled] = *x;
        }
        self.filled += 1;
        if self.filled == self.block || self.start + self.filled == self.len {
            self.transform(emit);
        }
    }

    /// Emit the outputs after the last input.
    pub fn finish(&mut self, emit: &mut impl FnMut(usize, &[f64])) {
        assert!(self.start >= self.len, "inputs missing from the stream");
        let full = self.len + self.taps;
        self.emit_full(self.start, full.saturating_sub(self.start), emit);
    }

    fn transform(&mut self, emit: &mut impl FnMut(usize, &[f64])) {
        // Full convolution of `len + taps` samples, emitted shifted back by
        // the kernel centre.
        let full = self.len + self.taps;
        let (start, count) = (self.start, self.filled);
        for ((inputs, response), pending) in self
            .inputs
            .iter()
            .zip(&self.responses)
            .zip(&mut self.pending)
        {
            for (k, b) in self.buffer.iter_mut().enumerate() {
                *b = Complex::new(if k < count { inputs[k] } else { 0.0 }, 0.0);
            }
            self.forward.process_outofplace_with_scratch(
                &mut self.buffer,
                &mut self.spectrum,
                &mut self.scratch,
            );
            for (b, h) in self.spectrum.iter_mut().zip(response) {
                *b *= h;
            }
            self.inverse.process_outofplace_with_scratch(
                &mut self.spectrum,
                &mut self.buffer,
                &mut self.scratch,
            );
            for (k, b) in self.buffer.iter().enumerate() {
                if start + k < full {
                    pending[k] += b.re / FFT as f64;
                }
            }
        }
        // Later blocks start at `start + block`: the samples before it are
        // final.
        self.emit_full(start, self.block.min(full - start), emit);
        let block = self.block;
        for pending in &mut self.pending {
            pending.copy_within(block.., 0);
            pending[FFT - block..].fill(0.0);
        }
        self.start += block;
        self.filled = 0;
    }

    fn emit_full(&mut self, start: usize, count: usize, emit: &mut impl FnMut(usize, &[f64])) {
        let half = self.taps / 2;
        for k in 0..count {
            let j = start + k;
            if j >= half && j - half < self.len {
                for (output, pending) in self.outputs.iter_mut().zip(&self.pending) {
                    *output = pending[k];
                }
                emit(j - half, &self.outputs);
            }
        }
    }
}

/// Mean power gain of `kernel`, a filter at `rate`, between each pair of
/// `edges`, in dB, from a 65536-point spectrum.
pub fn band_power_db(kernel: &[f64], edges: &[f64], rate: u32) -> Vec<f64> {
    let n = 65536;
    let mut spectrum: Vec<Complex<f64>> = kernel
        .iter()
        .map(|h| Complex::new(*h, 0.0))
        .chain(std::iter::repeat(Complex::new(0.0, 0.0)))
        .take(n)
        .collect();
    FftPlanner::new().plan_fft_forward(n).process(&mut spectrum);
    edges
        .windows(2)
        .map(|e| {
            let bins: Vec<f64> = (1..n / 2)
                .filter(|k| {
                    let hz = *k as f64 * rate as f64 / n as f64;
                    hz >= e[0] && hz < e[1]
                })
                .map(|k| spectrum[k].norm_sqr())
                .collect();
            10.0 * (bins.iter().sum::<f64>() / bins.len().max(1) as f64)
                .max(1e-30)
                .log10()
        })
        .collect()
}

/// The impulse response of `taps` (odd) samples with unit magnitude and the
/// phase `phase(hz)` in radians at `rate`, Hann-windowed, centred at
/// `taps / 2`.
pub fn phase_kernel(taps: usize, phase: impl Fn(f64) -> f64, rate: u32) -> Vec<f64> {
    let n = 4 * (taps - 1);
    let mut spectrum: Vec<Complex<f64>> = (0..n)
        .map(|k| {
            if k == 0 || 2 * k == n {
                return Complex::new(1.0, 0.0);
            }
            let theta = phase(k.min(n - k) as f64 * rate as f64 / n as f64);
            // Conjugate symmetry keeps the impulse response real.
            Complex::from_polar(1.0, if 2 * k < n { theta } else { -theta })
        })
        .collect();
    FftPlanner::new().plan_fft_inverse(n).process(&mut spectrum);
    let half = taps / 2;
    (0..taps)
        .map(|i| {
            let window =
                0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / (taps - 1) as f64).cos();
            spectrum[(i + n - half) % n].re / n as f64 * window
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter_whole(signal: &[f64], kernel: &[f64]) -> Vec<f64> {
        let taps = kernel.len();
        let half = taps / 2;
        let block = FFT - taps + 1;
        let mut planner = FftPlanner::new();
        let (forward, inverse) = (planner.plan_fft_forward(FFT), planner.plan_fft_inverse(FFT));
        let mut response: Vec<Complex<f64>> = kernel
            .iter()
            .map(|h| Complex::new(*h, 0.0))
            .chain(std::iter::repeat(Complex::new(0.0, 0.0)))
            .take(FFT)
            .collect();
        forward.process(&mut response);
        // Full convolution, then the output shifted back by the kernel centre.
        let mut out = vec![0.0f64; signal.len() + taps];
        let mut buffer = vec![Complex::new(0.0, 0.0); FFT];
        for start in (0..signal.len()).step_by(block) {
            let end = (start + block).min(signal.len());
            for (k, b) in buffer.iter_mut().enumerate() {
                *b = Complex::new(
                    if k < end - start {
                        signal[start + k]
                    } else {
                        0.0
                    },
                    0.0,
                );
            }
            forward.process(&mut buffer);
            for (b, h) in buffer.iter_mut().zip(&response) {
                *b *= h;
            }
            inverse.process(&mut buffer);
            for (k, b) in buffer.iter().enumerate() {
                if start + k < out.len() {
                    out[start + k] += b.re / FFT as f64;
                }
            }
        }
        out.drain(..half);
        out.truncate(signal.len());
        out
    }

    #[test]
    fn the_streamed_filter_matches_the_whole_filter() {
        let kernel = kernel(1023, |hz| 1.0 / (1.0 + hz / 2000.0), 48000);
        for len in [1, 500, FFT - 1022, FFT - 1021, 3 * FFT + 17] {
            let signal: Vec<f64> = (0..len)
                .map(|i| ((i * 7919) % 1009) as f64 / 1009.0 - 0.5)
                .collect();
            assert_eq!(
                filter(&signal, &kernel),
                filter_whole(&signal, &kernel),
                "len {len}"
            );
        }
    }

    #[test]
    fn one_pass_of_several_kernels_in_place_matches_each_filter() {
        // The second input reads the signal that `emit` writes, as the side
        // gain of `width::SideGain::apply_each` does.
        let low = kernel(1023, |hz| 1.0 / (1.0 + hz / 2000.0), 48000);
        let high = kernel(1023, |hz| hz / (hz + 500.0), 48000);
        for len in [1, 500, FFT - 1022, FFT - 1021, 3 * FFT + 17] {
            let signal: Vec<f64> = (0..len)
                .map(|i| ((i * 7919) % 1009) as f64 / 1009.0 - 0.5)
                .collect();
            let other: Vec<f64> = signal.iter().map(|x| x * x - 0.25).collect();
            let expected: Vec<f64> = filter(&other, &low)
                .iter()
                .zip(filter(&signal, &high))
                .map(|(a, b)| a + b)
                .collect();
            let mut written = signal.clone();
            let cells = std::cell::Cell::from_mut(written.as_mut_slice()).as_slice_of_cells();
            let mut last = None;
            filter_each_many(
                len,
                &[&low, &high],
                |i, inputs| {
                    assert!(last.is_none_or(|l| i == l + 1));
                    last = Some(i);
                    let x = cells[i].get();
                    inputs[0] = x * x - 0.25;
                    inputs[1] = x;
                },
                |i, outputs| cells[i].set(outputs[0] + outputs[1]),
            );
            assert_eq!(written, expected, "len {len}");
        }
    }
}
