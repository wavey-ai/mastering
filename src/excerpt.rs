//! Render of a span of a version from the fixed state of a render.
//!
//! A span render keeps the drive, the restoring gains and the true-peak trim
//! of a delivered render, and runs the chain on the span with context on
//! each side. The envelopes and the de-esser frames take their positions in
//! the recording, and the de-esser keeps the reference excess and the
//! largest cut of the delivered render (`deesser::Plan::delivered`). With the state
//! of a delivered version, the kept frames equal the same frames of its
//! render, apart from the last bit.
use crate::{
    audio_io::{Result, Stereo},
    dsp::{self, Oversampler},
    render_blocks,
    treatment_render::{self, span_grid, Spec},
};
use serde_json::Value;
use std::ops::Range;

/// Seconds of context before the kept frames: the 4.5 s in which the
/// compressor reaches the state of the whole render, and the half window
/// of its section make-up (`compressor::MASTERING_SECTION_MS`, 2 s). On the
/// test signal, 2.5 s of state gives a difference of -98 dBFS, 3.75 s gives
/// -157 dBFS, and 4.4 s gives none.
pub const CONTEXT_BEFORE_SECONDS: f64 = 6.5;
/// Seconds of context after the kept frames: the half window of the
/// compressor section level and the half window of its section make-up
/// (2 s each), and 0.5 s for the limiter lookahead and the decimator.
pub const CONTEXT_AFTER_SECONDS: f64 = 4.5;

/// Frames of context before and after the kept frames at `rate`.
pub fn context(rate: u32) -> (usize, usize) {
    let frames = |seconds: f64| (seconds * rate as f64) as usize;
    (
        frames(CONTEXT_BEFORE_SECONDS),
        frames(CONTEXT_AFTER_SECONDS),
    )
}

/// The state of a delivered render that a span render keeps.
#[derive(Clone, Copy, Debug)]
pub struct Fixed {
    pub drive_db: f64,
    /// Gain of the whole render after the true-peak correction, in dB.
    pub trim_db: f64,
}

/// The render report of a version record (`render` of a family result, or
/// `report.render` of a set manifest).
fn render_report(record: &Value) -> &Value {
    if record["render"].is_object() {
        &record["render"]
    } else {
        &record["report"]["render"]
    }
}

impl Fixed {
    pub fn of(record: &Value) -> Result<Self> {
        let render = render_report(record);
        Ok(Self {
            drive_db: render["drive_db"].as_f64().ok_or("Missing drive")?,
            trim_db: render["true_peak"]["trim_db"].as_f64().unwrap_or(0.0),
        })
    }
}

/// The spec of a version record with the restoring gains and the de-esser
/// cut of its delivered render.
pub fn delivered_spec(record: &Value) -> Result<Spec> {
    let mut spec = request_spec(record)?;
    if let Some(gains) = render_report(record)["restoration"]["gains_db"].as_array() {
        for (g, value) in spec.restoration_db.iter_mut().zip(gains) {
            *g = value.as_f64().ok_or("Invalid restoring gain")?;
        }
    }
    Ok(spec)
}

/// The spec that a request on a version record starts from: its spec with
/// the reference excess and the largest cut of the de-esser of its delivered
/// render (`deesser::Plan::delivered`), so that the request moves from the
/// delivered cut.
pub fn request_spec(record: &Value) -> Result<Spec> {
    let mut spec = crate::distributed::decode_spec(&record["spec"])?;
    if let Some(plan) = &mut spec.deesser {
        *plan = plan.delivered(&render_report(record)["deesser"])?;
    }
    Ok(spec)
}

/// The source frames that a render of `keep` reads in a recording of
/// `frames` frames at `rate`: the context on each side, from a start on
/// `span_grid`.
pub fn bounds(keep: &Range<usize>, frames: usize, rate: u32) -> Result<Range<usize>> {
    if keep.start >= keep.end || keep.end > frames {
        return Err("The span is outside the recording".into());
    }
    let ((before, after), grid) = (context(rate), span_grid(rate));
    let start = keep.start.saturating_sub(before) / grid * grid;
    Ok(start..(keep.end + after).min(frames))
}

/// Frames `keep` of the render of `spec` with the state `fixed`, from
/// `source`, the frames `bounds(keep, frames, rate)` of the source
/// recording at `rate`.
pub fn render_span(
    source: &[Stereo],
    frames: usize,
    spec: &Spec,
    fixed: Fixed,
    keep: Range<usize>,
    rate: u32,
) -> Result<Vec<Stereo>> {
    let read = bounds(&keep, frames, rate)?;
    if source.len() != read.len() {
        return Err("The span source has the wrong length".into());
    }
    if !spec.dynamic_after_eq.is_empty() {
        return Err("A span render has no dynamic bands".into());
    }
    let prefix = treatment_render::prefix_span(source, spec, read.start, frames, rate)?;
    let mut up = crate::scratch::Pcm::new(crate::scratch::memory()?);
    render_blocks::upsample_into(&prefix, &mut up, rate)?;
    drop(prefix);
    let (mut audio, ..) = render_blocks::render(
        &mut up,
        fixed.drive_db,
        spec.clipper_db,
        dsp::gain(spec.ceiling_dbtp - 0.1),
        spec.limiter,
        rate,
    )?;
    // As `render_blocks::fit_true_peak`, with the trim of the whole render.
    let target = spec.ceiling_dbtp - 0.001;
    let os = Oversampler::new(rate);
    if spec.true_peak == dsp::TruePeak::Local && dsp::db(os.peak(&audio)) > target {
        os.lower_peaks(&mut audio, dsp::gain(target - 0.001));
    }
    treatment_render::scale(&mut audio, fixed.trim_db);
    Ok(audio[keep.start - read.start..keep.end - read.start].to_vec())
}

/// `render_span` from the whole source recording at `rate`.
pub fn render(
    input: &[Stereo],
    spec: &Spec,
    fixed: Fixed,
    keep: Range<usize>,
    rate: u32,
) -> Result<Vec<Stereo>> {
    let read = bounds(&keep, input.len(), rate)?;
    render_span(&input[read], input.len(), spec, fixed, keep, rate)
}

/// Largest absolute sample difference of two renders, in dBFS.
pub fn difference_dbfs(a: &[Stereo], b: &[Stereo]) -> f64 {
    let largest = a
        .iter()
        .flatten()
        .zip(b.iter().flatten())
        .map(|(x, y)| (*x as f64 - *y as f64).abs())
        .fold(0.0f64, f64::max);
    dsp::db(largest.max(1e-12))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::treatment::{BlendPoint, FocusEnvelope, Tone};

    /// Noise in two-second passages with a 7 kHz burst, at `rate`.
    fn source(frames: usize, rate: u32) -> Vec<Stereo> {
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut noise = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 11) as f64 / (1u64 << 53) as f64 - 0.5
        };
        (0..frames)
            .map(|i| {
                let t = i as f64 / rate as f64;
                let level = if (i / (2 * rate as usize)).is_multiple_of(2) {
                    0.5
                } else {
                    0.15
                };
                let kick = (2.0 * std::f64::consts::PI * 55.0 * t).sin()
                    * (-((t * 2.0).fract()) * 12.0).exp();
                let burst = if (5.0..5.3).contains(&t) {
                    0.2 * (2.0 * std::f64::consts::PI * 7000.0 * t).sin()
                } else {
                    0.0
                };
                let l = level * (noise() + kick) + burst;
                let r = level * (noise() + kick) + burst;
                [l as f32, r as f32]
            })
            .collect()
    }

    fn spec(frames: usize) -> Spec {
        let plan: Value = serde_json::from_str(include_str!("../tests/data/adjust-parent.json"))
            .expect("parent spec");
        let mut spec = crate::distributed::decode_spec(&plan).expect("decoded");
        spec.low_end_hold = None;
        spec.low_mid = None;
        spec.phase_alignment = None;
        spec.tone = Tone {
            stereo: vec![dsp::EqBand {
                kind: dsp::Kind::LowShelf,
                hz: 150.0,
                q: std::f64::consts::FRAC_1_SQRT_2,
                gain_db: 1.5,
            }],
            mid: vec![dsp::EqBand {
                kind: dsp::Kind::Peak,
                hz: 3162.0,
                q: 0.7,
                gain_db: 1.0,
            }],
            side: Vec::new(),
        };
        spec.focus_envelopes = vec![FocusEnvelope {
            domain: crate::finishing::Domain::Mid,
            filter_index: 0,
            points: vec![
                BlendPoint {
                    sample: 0,
                    wet: 0.2,
                },
                BlendPoint {
                    sample: frames / 2,
                    wet: 1.0,
                },
                BlendPoint {
                    sample: frames - 1,
                    wet: 0.4,
                },
            ],
        }];
        // Two groups of keyed frames: 35 frames from 4.6 s at 48 kHz (5.0 s
        // at 44.1 kHz), which hold the burst at 5 s, and 30 frames of noise
        // from 12.8 s at 48 kHz (13.9 s at 44.1 kHz). The spans from 0 to
        // 8 s read the first group only, and the span from 12.75 to 15 s
        // reads the second group only. The top 10% of the whole render are
        // frames of the burst, so each span solves a reference excess of its
        // own that is different from that of the whole render.
        if let Some(plan) = &mut spec.deesser {
            plan.keyed = (215..250)
                .chain(600..630)
                .map(|frame| crate::deesser::Keyed {
                    frame,
                    vocal_share: if frame < 600 { 0.8 } else { 0.6 },
                    other_power: 1e-3,
                })
                .collect();
            plan.other_band_power = 1.0;
        }
        spec.restoration_db = [0.3, 0.0, -0.2, 0.0, 0.4, 0.0];
        spec.restore_peak_tone = false;
        spec
    }

    #[test]
    fn a_span_equals_the_same_frames_of_the_whole_render() {
        for rate in crate::audio_io::RENDER_RATES {
            let second = rate as usize;
            let frames = 20 * second;
            let input = source(frames, rate);
            let mut spec = spec(frames);
            assert!(spec.compressor.is_some() && spec.deesser.is_some());
            let whole = treatment_render::Renderer::new(&input, rate)
                .render(&spec)
                .unwrap();
            let fixed = Fixed {
                drive_db: whole.drive_db,
                trim_db: whole.true_peak.trim_db,
            };
            // The span renders keep the de-esser cut of the whole render,
            // as `delivered_spec` gives it.
            let stats = whole.deesser.unwrap();
            assert!(
                stats.cut_frames > 0 && stats.maximum_cut_db > 0.0,
                "{stats:?}"
            );
            assert_eq!(stats.keyed_frames, 65);
            let plan = spec.deesser.clone().unwrap();
            let delivered = plan
                .delivered(&serde_json::to_value(stats).unwrap())
                .unwrap();
            assert_eq!(
                delivered.reference_excess_db,
                Some(stats.reference_excess_db)
            );
            let solved = spec.clone();
            spec.deesser = Some(delivered);
            // The second span starts off the grid of the masking frames.
            for keep in [
                4 * second..8 * second,
                4 * second + 777..8 * second,
                0..2 * second,
                12 * second + 3 * second / 4..15 * second,
                frames - second..frames,
            ] {
                let span = render(&input, &spec, fixed, keep.clone(), rate).unwrap();
                let difference = difference_dbfs(&span, &whole.audio[keep.clone()]);
                assert!(difference < -138.0, "{rate} {keep:?}: {difference} dBFS");
            }
            // A span that solves the reference excess of its own keyed frames
            // cuts other gains.
            for keep in [
                4 * second..8 * second,
                12 * second + 3 * second / 4..15 * second,
            ] {
                let span = render(&input, &solved, fixed, keep.clone(), rate).unwrap();
                let difference = difference_dbfs(&span, &whole.audio[keep.clone()]);
                assert!(difference > -100.0, "{rate} {keep:?}: {difference} dBFS");
            }
        }
    }

    #[test]
    fn a_span_starts_on_the_grid() {
        for rate in crate::audio_io::RENDER_RATES {
            let (before, after) = context(rate);
            let read = bounds(&(300_000..400_000), 1_000_000, rate).unwrap();
            assert_eq!(read.start % span_grid(rate), 0);
            assert!(read.start <= 300_000 - before);
            assert_eq!(read.end, 400_000 + after);
            assert!(bounds(&(10..10), 100, rate).is_err());
            assert!(bounds(&(90..110), 100, rate).is_err());
        }
    }
}
