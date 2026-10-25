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
    // Full convolution of `len + taps` samples, emitted shifted back by the
    // kernel centre. `pending[k]` holds full sample `start + k`.
    let full = len + taps;
    let mut emit_full = |j: usize, value: f64| {
        if j >= half && j - half < len {
            emit(j - half, value);
        }
    };
    let mut pending = vec![0.0f64; FFT];
    // Out of place with one scratch: in place, rustfft allocates its scratch
    // on each call, transforms into it and copies the result back.
    let mut buffer = vec![Complex::new(0.0, 0.0); FFT];
    let mut spectrum = vec![Complex::new(0.0, 0.0); FFT];
    let mut scratch = vec![
        Complex::new(0.0, 0.0);
        forward
            .get_outofplace_scratch_len()
            .max(inverse.get_outofplace_scratch_len())
    ];
    let mut start = 0;
    while start < len {
        let end = (start + block).min(len);
        for (k, b) in buffer.iter_mut().enumerate() {
            *b = Complex::new(
                if k < end - start {
                    sample(start + k)
                } else {
                    0.0
                },
                0.0,
            );
        }
        forward.process_outofplace_with_scratch(&mut buffer, &mut spectrum, &mut scratch);
        for (b, h) in spectrum.iter_mut().zip(&response) {
            *b *= h;
        }
        inverse.process_outofplace_with_scratch(&mut spectrum, &mut buffer, &mut scratch);
        for (k, b) in buffer.iter().enumerate() {
            if start + k < full {
                pending[k] += b.re / FFT as f64;
            }
        }
        // Later blocks start at `start + block`: the samples before it are final.
        for (k, value) in pending.iter().enumerate().take(block.min(full - start)) {
            emit_full(start + k, *value);
        }
        pending.copy_within(block.., 0);
        pending[FFT - block..].fill(0.0);
        start += block;
    }
    for (k, value) in pending.iter().enumerate().take(full.saturating_sub(start)) {
        emit_full(start + k, *value);
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
}
