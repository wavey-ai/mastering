//! Alignment of a consistent phase offset between the channels
//! (`set.phase-alignment.v1`). A device or a transfer can turn one channel
//! against the other over a frequency region; the region then cancels in
//! mono. The stage applies only when the source condition reports an
//! anti-phase band (D142), and turns the left channel by minus half and the
//! right channel by plus half of the measured phase where the phase is
//! consistent and beyond 90°. Magnitudes stay the same (D143).
use crate::audio_io::Stereo;
use rustfft::num_complex::Complex;
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

pub const RULE: &str = "set.phase-alignment.v1";
/// Smallest phase resultant of a frequency that the stage turns. Incoherent
/// stereo has a resultant near 0; a phone recording with a channel turned
/// by the device holds 0.54 to 0.94 over the faulty region.
pub const RESULTANT: f64 = 0.5;
/// The turn rises from none at 60° to the full phase at 90°: a band beyond
/// 90° subtracts in mono.
pub const START_DEGREES: f64 = 60.0;
pub const FULL_DEGREES: f64 = 90.0;
/// Taps of the alignment kernels: as the side gain kernel.
pub const TAPS: usize = 4097;
const FFT: usize = 8192;
/// Points per octave of the stored turn.
const POINTS_PER_OCTAVE: f64 = 12.0;

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(deny_unknown_fields)]
pub struct Alignment {
    /// The phase of the left channel against the right that the stage
    /// removes, in degrees, at each frequency in Hz, ascending.
    pub points: Vec<(f64, f64)>,
}

impl Alignment {
    pub fn validate(&self) -> crate::audio_io::Result<()> {
        let ordered = self.points.windows(2).all(|w| w[1].0 > w[0].0);
        if self.points.len() < 2
            || !ordered
            || self.points.iter().any(|(hz, deg)| {
                !hz.is_finite() || !deg.is_finite() || *hz <= 0.0 || deg.abs() > 360.0
            })
        {
            return Err("Invalid phase alignment".into());
        }
        Ok(())
    }

    /// The turn at `hz`, in radians, interpolated on a log-frequency scale.
    pub fn radians(&self, hz: f64) -> f64 {
        let first = self.points[0];
        let last = self.points[self.points.len() - 1];
        let deg = if hz <= first.0 {
            first.1
        } else if hz >= last.0 {
            last.1
        } else {
            let i = self.points.partition_point(|p| p.0 <= hz) - 1;
            let (a, b) = (self.points[i], self.points[i + 1]);
            let t = (hz / a.0).ln() / (b.0 / a.0).ln();
            a.1 + t * (b.1 - a.1)
        };
        deg.to_radians()
    }

    /// Turn the left channel of `audio`, at `rate`, by minus half and the
    /// right channel by plus half of the measured phase, by FFT convolution.
    pub fn apply(&self, audio: &mut [Stereo], rate: u32) {
        let left = crate::fir::phase_kernel(TAPS, |hz| -0.5 * self.radians(hz), rate);
        let right = crate::fir::phase_kernel(TAPS, |hz| 0.5 * self.radians(hz), rate);
        // In place: each output sample is emitted before any later input
        // sample is read, and f64 copies of a 30-minute channel are 0.6 GB.
        let frames = std::cell::Cell::from_mut(audio).as_slice_of_cells();
        for (c, kernel) in [(0, &left), (1, &right)] {
            crate::fir::filter_each(
                frames.len(),
                |i| frames[i].get()[c] as f64,
                kernel,
                |i, value| {
                    let mut frame = frames[i].get();
                    frame[c] = value as f32;
                    frames[i].set(frame);
                },
            );
        }
    }
}

/// The alignment of `input`: the phase of its whole-file cross-spectrum,
/// smoothed over a sixth of an octave and unwrapped along frequency,
/// weighted where the phase is consistent and beyond `START_DEGREES`. None
/// when no frequency qualifies. `input` is at `rate`.
pub fn measure(input: &[Stereo], rate: u32) -> Option<Alignment> {
    let rate = rate as f64;
    let window: Vec<f64> = (0..FFT)
        .map(|i| 0.5 - 0.5 * (2.0 * PI * i as f64 / FFT as f64).cos())
        .collect();
    let bins = FFT / 2 + 1;
    let mut cross = vec![Complex::new(0.0, 0.0); bins];
    let mut magnitude = vec![0.0f64; bins];
    // A real-input transform of each channel.
    let fft = realfft::RealFftPlanner::new().plan_fft_forward(FFT);
    let (mut samples, mut scratch) = (fft.make_input_vec(), fft.make_scratch_vec());
    let (mut a, mut b) = (fft.make_output_vec(), fft.make_output_vec());
    for start in (0..input.len().saturating_sub(FFT)).step_by(FFT / 2) {
        for (c, output) in [(0, &mut a), (1, &mut b)] {
            for (k, x) in samples.iter_mut().enumerate() {
                *x = input[start + k][c] as f64 * window[k];
            }
            fft.process_with_scratch(&mut samples, output, &mut scratch)
                .expect("buffer lengths from the plan");
        }
        for k in 0..bins {
            let c = a[k] * b[k].conj();
            cross[k] += c;
            magnitude[k] += c.norm();
        }
    }
    let hz = |k: usize| k as f64 * rate / FFT as f64;
    let points_hz: Vec<f64> = {
        let (low, high) = (20.0f64, 20000.0f64);
        let count = ((high / low).log2() * POINTS_PER_OCTAVE).floor() as usize;
        (0..=count)
            .map(|i| low * 2f64.powf(i as f64 / POINTS_PER_OCTAVE))
            .collect()
    };
    let smoothed: Vec<(f64, f64)> = points_hz
        .iter()
        .map(|&f| {
            let (low, high) = (f * 2f64.powf(-1.0 / 12.0), f * 2f64.powf(1.0 / 12.0));
            let (mut c, mut m) = (Complex::new(0.0, 0.0), 0.0);
            for k in 1..bins {
                if hz(k) >= low && hz(k) < high {
                    c += cross[k];
                    m += magnitude[k];
                }
            }
            let resultant = if m > 0.0 { c.norm() / m } else { 0.0 };
            (c.arg(), resultant)
        })
        .collect();
    let weights: Vec<f64> = smoothed
        .iter()
        .map(|&(phase, resultant)| {
            let beyond = ((phase.abs().to_degrees() - START_DEGREES)
                / (FULL_DEGREES - START_DEGREES))
                .clamp(0.0, 1.0);
            if resultant >= RESULTANT {
                0.5 - 0.5 * (PI * beyond).cos()
            } else {
                0.0
            }
        })
        .collect();
    // Unwrap within each region that the stage turns, so a turn near ±180°
    // stays continuous; outside the regions the phase has no weight.
    let mut points = Vec::with_capacity(smoothed.len());
    let mut previous: Option<f64> = None;
    for ((&f, &(phase, _)), &weight) in points_hz.iter().zip(&smoothed).zip(&weights) {
        let mut turn = phase;
        if weight > 0.0 {
            if let Some(p) = previous {
                while turn - p > PI {
                    turn -= 2.0 * PI;
                }
                while turn - p < -PI {
                    turn += 2.0 * PI;
                }
            }
            previous = Some(turn);
        } else {
            previous = None;
        }
        points.push((f, (turn * weight).to_degrees()));
    }
    points
        .iter()
        .any(|(_, deg)| deg.abs() > 1.0)
        .then_some(Alignment { points })
}
