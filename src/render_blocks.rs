//! Exact offline peak control with bounded working blocks.
//!
//! One forward stream drives the limiter, the ceiling-relative clipper, and the
//! decimator. The limiter emits gains with a fixed lookahead lag, so a short
//! ring of driven frames is the only full-rate state. Statistics keep forward
//! order and match `dsp::limit` followed by `finishing::clip` exactly.
use crate::{
    audio_io::{Result, Stereo},
    dsp::{self, Limiter, LimiterSettings, LimiterStats, LimiterStatsAccumulator, Oversampler},
    finishing::PeakStats,
};
use serde::Serialize;
use std::collections::VecDeque;

const BLOCK: usize = 65_536;
const TRACE_BINS: usize = crate::waveform::BINS;

#[derive(Clone, Serialize)]
pub struct LimiterTraceBin {
    pub start_sample: usize,
    pub end_sample: usize,
    pub mean_reduction_db: f64,
    pub maximum_reduction_db: f64,
    /// Largest driven sample before the limiter, in dBFS at 4× rate.
    pub input_peak_db: f64,
    /// Largest sample after the limiter and clipper, in dBFS at 4× rate.
    pub output_peak_db: f64,
    /// Largest soft-clip reduction in the bin, in dB.
    pub maximum_clip_db: f64,
    /// Share of the bin's frames that the clipper shaped.
    pub clipped_fraction: f64,
}

/// Per-bin accumulators of one traced render.
#[derive(Clone, Default)]
struct TraceAccumulator {
    mean: f64,
    maximum: f64,
    input_peak: f64,
    output_peak: f64,
    clip: f64,
    clipped: usize,
    count: usize,
}

#[derive(Clone, Serialize)]
pub struct LimiterTrace {
    pub schema: &'static str,
    pub sample_rate: usize,
    pub input_frames: usize,
    pub bins: Vec<LimiterTraceBin>,
}

struct BlockRender {
    audio: Vec<Stereo>,
    stats: LimiterStats,
    clipping: Option<PeakStats>,
    trace: Option<LimiterTrace>,
    /// The accumulators of the measured frames, for a segmented render.
    parts: Parts,
}

/// Statistics of the measured frames of one block render, before they are
/// finished, so that consecutive segments can merge them in order.
#[derive(Clone)]
struct Parts {
    stats: LimiterStatsAccumulator,
    clip_maximum_db: f64,
    clipped: usize,
    trace: Option<Vec<TraceAccumulator>>,
}

struct RenderOptions {
    /// The base rate of the recording; the input is its 4x signal.
    rate: u32,
    drive_db: f64,
    clipper_db: f64,
    ceiling: f64,
    settings: LimiterSettings,
    block: usize,
    collect_trace: bool,
    /// The 4x frames of the input whose statistics count, the position of
    /// the first of them in the whole recording, and the 4x frame count of
    /// the whole recording, which sets the trace bins. `None` measures the
    /// whole input.
    measure: Option<(std::ops::Range<usize>, usize, usize)>,
}

pub(crate) fn render(
    input: &mut impl crate::scratch::Frames,
    drive_db: f64,
    clipper_db: f64,
    ceiling: f64,
    settings: LimiterSettings,
    rate: u32,
) -> Result<(Vec<Stereo>, LimiterStats, Option<PeakStats>, LimiterTrace)> {
    let result = render_with_block_trace(
        input,
        RenderOptions {
            rate,
            drive_db,
            clipper_db,
            ceiling,
            settings,
            block: BLOCK,
            collect_trace: true,
            measure: None,
        },
    )?;
    Ok((
        result.audio,
        result.stats,
        result.clipping,
        result.trace.expect("Trace was requested"),
    ))
}

/// Write the 4x prefix of `shaped`, audio at `rate`, to `up`, upsampled on
/// the available cores in order; each 4x frame depends only on the shaped
/// prefix. Returns the largest 4x sample.
pub(crate) fn upsample_into(
    shaped: &[Stereo],
    up: &mut crate::scratch::Pcm,
    rate: u32,
) -> Result<f64> {
    let os = Oversampler::new(rate);
    let mut peak = 0.0f64;
    const CHUNK: usize = 1 << 16;
    // A group spans the cores of a native build. The browser engine has one
    // thread: a group of one chunk holds 2 MiB of 4x frames, not 16 MiB.
    const GROUP: usize = if cfg!(target_arch = "wasm32") { 1 } else { 8 };
    for group in (0..shaped.len()).step_by(CHUNK * GROUP) {
        let end = (group + CHUNK * GROUP).min(shaped.len());
        let blocks = dsp::chunked(end - group, CHUNK, |range| {
            let mut block = Vec::with_capacity(range.len() * 4);
            let mut peak = 0.0f64;
            for base in group + range.start..group + range.end {
                for frame in os.at_all(shaped, base) {
                    for x in frame {
                        peak = peak.max(x.abs() as f64);
                    }
                    block.push(frame);
                }
            }
            (block, peak)
        });
        for (block, block_peak) in blocks {
            peak = peak.max(block_peak);
            for piece in block.chunks(16_384) {
                up.push(piece)?;
            }
        }
    }
    Ok(peak)
}

/// 4x frames from which peak control of a recording at `rate` renders in
/// parallel segments (`render_segmented`): one segment. A recording of one
/// segment or less renders as one stream. `MASTERING_SEGMENTED_MIN_SECONDS`
/// overrides it for measurement.
pub(crate) fn segmented_min_frames(rate: u32) -> usize {
    static SECONDS: std::sync::OnceLock<Option<f64>> = std::sync::OnceLock::new();
    SECONDS
        .get_or_init(|| {
            std::env::var("MASTERING_SEGMENTED_MIN_SECONDS")
                .ok()
                .and_then(|v| v.parse::<f64>().ok())
        })
        .map_or(segment(rate), |s| (s * 4.0 * rate as f64) as usize)
}

/// The true-peak correction of a decimated render.
#[derive(Clone, Copy, Default, Serialize)]
pub struct TruePeakFit {
    /// Largest local reduction of `TruePeak::Local`, in dB.
    pub local_db: f64,
    /// Gain of the whole render after the local reduction, in dB: zero or
    /// negative.
    pub trim_db: f64,
}

/// Bring the true peak of `audio`, a render at the base rate `rate`, to
/// `ceiling_dbtp` with `true_peak`. A 0.001 dB margin keeps the f32-scaled
/// peak at or below the ceiling after rounding. Returns the true peak after
/// the correction, in dBTP.
pub(crate) fn fit_true_peak(
    audio: &mut [Stereo],
    ceiling_dbtp: f64,
    true_peak: dsp::TruePeak,
    rate: u32,
) -> (f64, TruePeakFit) {
    let os = Oversampler::new(rate);
    let target = ceiling_dbtp - 0.001;
    let mut fit = TruePeakFit::default();
    let peak = if true_peak == dsp::TruePeak::Local {
        // The local gain aims 0.001 dB under the target, so the trim seldom
        // follows it. One scan gives the peak and the frames over that
        // ceiling, and the peak after the local gain measures again only
        // where the gain changed the interpolated samples.
        let ceiling = dsp::gain(target - 0.001);
        let mut scan = os.peak_scan(audio, ceiling);
        if dsp::db(scan.peak) > target {
            let overs = std::mem::take(&mut scan.overs);
            let (local_db, changed) = os.lower_peaks_from(audio, ceiling, overs);
            fit.local_db = local_db;
            dsp::db(os.peak_after(audio, &scan, &changed))
        } else {
            dsp::db(scan.peak)
        }
    } else {
        dsp::db(os.peak(audio))
    };
    fit.trim_db = (target - peak).min(0.0);
    crate::treatment_render::scale(audio, fit.trim_db);
    (peak + fit.trim_db, fit)
}

/// Peak control of `input`, the 4x signal of a recording at `rate`:
/// `render_segmented` above `segmented_min_frames` 4x frames, otherwise
/// `render`.
pub(crate) fn render_peak_control(
    input: &mut impl crate::scratch::Frames,
    drive_db: f64,
    clipper_db: f64,
    ceiling: f64,
    settings: LimiterSettings,
    rate: u32,
) -> Result<(Vec<Stereo>, LimiterStats, Option<PeakStats>, LimiterTrace)> {
    if input.len() > segmented_min_frames(rate) {
        render_segmented(input, drive_db, clipper_db, ceiling, settings, rate)
    } else {
        render(input, drive_db, clipper_db, ceiling, settings, rate)
    }
}

/// 4x frames of the core of one segment of a segmented render at `rate`:
/// 60 s.
pub(crate) fn segment(rate: u32) -> usize {
    4 * rate as usize * 60
}
/// 4x frames that a segment renders before its core at `rate`: 1 s. The
/// limiter's release reaches 1e-6 dB from a 20 dB reduction in 0.17 s, and
/// its smoothing and lookahead span about 1 ms.
fn segment_roll(rate: u32) -> usize {
    4 * rate as usize
}
/// 4x frames that a segment renders after its core: more than the
/// limiter's lag and the decimator's reach.
const SEGMENT_TAIL: usize = 16_384;

/// The render of `render` in segments of `segment` 4x frames, on the
/// available cores. Each segment starts `segment_roll` frames early and ends
/// `SEGMENT_TAIL` frames late, so the limiter and the decimator reach the
/// core in the state of a whole-file render; only the core's samples and
/// statistics are kept. The limiter's moving averages carry rounding from
/// the whole history, so a sample can differ from `render` in its last bit.
pub(crate) fn render_segmented(
    input: &mut impl crate::scratch::Frames,
    drive_db: f64,
    clipper_db: f64,
    ceiling: f64,
    settings: LimiterSettings,
    rate: u32,
) -> Result<(Vec<Stereo>, LimiterStats, Option<PeakStats>, LimiterTrace)> {
    render_in_segments(
        input,
        drive_db,
        clipper_db,
        ceiling,
        settings,
        segment(rate),
        rate,
    )
}

fn render_in_segments(
    input: &mut impl crate::scratch::Frames,
    drive_db: f64,
    clipper_db: f64,
    ceiling: f64,
    settings: LimiterSettings,
    segment: usize,
    rate: u32,
) -> Result<(Vec<Stereo>, LimiterStats, Option<PeakStats>, LimiterTrace)> {
    assert!(segment > 0 && segment.is_multiple_of(4));
    let frames = input.len();
    if !frames.is_multiple_of(4) {
        return Err("Segmented render needs whole base-rate frames".into());
    }
    let cores: Vec<std::ops::Range<usize>> = (0..frames)
        .step_by(segment)
        .map(|start| start..(start + segment).min(frames))
        .collect();
    let threads = dsp::threads();
    let mut output = Vec::with_capacity(frames / 4);
    let mut parts: Option<Parts> = None;
    let mut clipper_used = false;
    let bounds = |core: &std::ops::Range<usize>| {
        (
            core.start.saturating_sub(segment_roll(rate)),
            (core.end + SEGMENT_TAIL).min(frames),
        )
    };
    let options = |core: &std::ops::Range<usize>, start: usize| RenderOptions {
        rate,
        drive_db,
        clipper_db,
        ceiling,
        settings,
        block: BLOCK,
        collect_trace: true,
        measure: Some((core.start - start..core.end - start, core.start, frames)),
    };
    for batch in cores.chunks(threads) {
        let results: Vec<(usize, BlockRender)> = if let [core] = batch {
            // A segment that renders alone reads its frames from the input in
            // place: a copy of a 61 s segment at 4x is 94 MB at 48 kHz.
            let (start, end) = bounds(core);
            let mut window = crate::scratch::Window {
                frames: &mut *input,
                start,
                len: end - start,
            };
            vec![(
                start,
                render_with_block_trace(&mut window, options(core, start))?,
            )]
        } else {
            // The scratch input is read in order; the segments render in
            // parallel.
            let mut inputs = Vec::with_capacity(batch.len());
            for core in batch {
                let (start, end) = bounds(core);
                let mut samples = Vec::with_capacity(end - start);
                input.read(start, end - start, &mut samples)?;
                inputs.push((start, samples));
            }
            let render_one = |core: &std::ops::Range<usize>,
                              start: usize,
                              samples: &Vec<Stereo>| {
                render_with_block_trace(&mut crate::scratch::Slice(samples), options(core, start))
                    .map_err(|e| e.to_string())
            };
            let rendered: Vec<std::result::Result<BlockRender, String>> =
                std::thread::scope(|scope| {
                    let handles: Vec<_> = batch
                        .iter()
                        .zip(&inputs)
                        .map(|(core, (start, samples))| {
                            let render_one = &render_one;
                            scope.spawn(move || render_one(core, *start, samples))
                        })
                        .collect();
                    handles
                        .into_iter()
                        .map(|h| h.join().expect("segment thread"))
                        .collect()
                });
            let mut results = Vec::with_capacity(batch.len());
            for ((start, _), render) in inputs.iter().zip(rendered) {
                let render = render.map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
                results.push((*start, render));
            }
            results
        };
        for (core, (start, render)) in batch.iter().zip(results) {
            clipper_used = render.clipping.is_some();
            let first = (core.start - start) / 4;
            let last = (core.end - start) / 4;
            output.extend_from_slice(&render.audio[first..last]);
            match &mut parts {
                None => parts = Some(render.parts),
                Some(all) => {
                    all.stats.merge(&render.parts.stats);
                    all.clip_maximum_db = all.clip_maximum_db.max(render.parts.clip_maximum_db);
                    all.clipped += render.parts.clipped;
                    if let (Some(a), Some(b)) = (&mut all.trace, &render.parts.trace) {
                        for (x, y) in a.iter_mut().zip(b) {
                            x.mean += y.mean;
                            x.maximum = x.maximum.max(y.maximum);
                            x.input_peak = x.input_peak.max(y.input_peak);
                            x.output_peak = x.output_peak.max(y.output_peak);
                            x.clip = x.clip.max(y.clip);
                            x.clipped += y.clipped;
                            x.count += y.count;
                        }
                    }
                }
            }
        }
    }
    let parts = parts.ok_or("Empty segmented render")?;
    let clipping = clipper_used.then(|| PeakStats {
        maximum_reduction_db: parts.clip_maximum_db,
        affected_frames_percent: 100.0 * parts.clipped as f64 / frames.max(1) as f64,
    });
    let output_frames = output.len();
    let level = |peak: f64| dsp::db(peak.max(1e-6));
    let trace = LimiterTrace {
        schema: "mastering.limiter-trace.2",
        sample_rate: rate as usize,
        input_frames: output_frames,
        bins: parts
            .trace
            .expect("trace collected")
            .iter()
            .enumerate()
            .map(|(index, bin)| LimiterTraceBin {
                start_sample: crate::waveform::bin_start(index, output_frames),
                end_sample: crate::waveform::bin_start(index + 1, output_frames),
                mean_reduction_db: bin.mean / bin.count.max(1) as f64,
                maximum_reduction_db: bin.maximum,
                input_peak_db: level(bin.input_peak),
                output_peak_db: level(bin.output_peak),
                maximum_clip_db: bin.clip,
                clipped_fraction: bin.clipped as f64 / bin.count.max(1) as f64,
            })
            .collect(),
    };
    Ok((output, parts.stats.finish(rate), clipping, trace))
}

#[cfg(test)]
fn render_with_block(
    input: &mut impl crate::scratch::Frames,
    drive_db: f64,
    clipper_db: f64,
    ceiling: f64,
    settings: LimiterSettings,
    block: usize,
) -> Result<(Vec<Stereo>, LimiterStats, Option<PeakStats>)> {
    let result = render_with_block_trace(
        input,
        RenderOptions {
            rate: 48000,
            drive_db,
            clipper_db,
            ceiling,
            settings,
            block,
            collect_trace: false,
            measure: None,
        },
    )?;
    Ok((result.audio, result.stats, result.clipping))
}

fn render_with_block_trace(
    input: &mut impl crate::scratch::Frames,
    RenderOptions {
        rate,
        drive_db,
        clipper_db,
        ceiling,
        settings,
        block,
        collect_trace,
        measure,
    }: RenderOptions,
) -> Result<BlockRender> {
    assert!(block > 0);
    let frames = input.len();
    let (measured, measure_offset, trace_frames) = measure.unwrap_or((0..frames, 0, frames));
    let gain = dsp::gain(drive_db) as f32;
    if !gain.is_finite() || !(0.0..=6.0).contains(&clipper_db) {
        return Err("Invalid blocked peak-control input".into());
    }
    settings.validate()?;
    let clipper = dsp::SoftClip::new(ceiling, clipper_db);
    let mut limiter = Limiter::new(ceiling * dsp::gain(clipper_db), settings, rate);
    let mut pending: VecDeque<Stereo> = VecDeque::with_capacity(limiter.lag() + 1);
    let os = Oversampler::new(rate);
    let radius = os.down_radius();
    let mut window = Vec::with_capacity(block + radius * 2 + 4);
    let mut window_start = 0;
    let mut produced = 0;
    let mut base = 0;
    let mut output = Vec::with_capacity(frames.div_ceil(4));
    let mut stats = LimiterStatsAccumulator::default();
    let mut clipping = PeakStats::default();
    let mut clipped = 0usize;
    let mut trace = collect_trace.then(|| vec![TraceAccumulator::default(); TRACE_BINS]);
    let mut samples = Vec::with_capacity(block);
    let mut emit =
        |g: f32, driven: Stereo, window: &mut Vec<Stereo>, produced: &mut usize| -> Result<()> {
            let counted = measured.contains(produced);
            let reduction = if counted {
                stats.push(g)
            } else if g == 1.0 {
                -0.0
            } else {
                -dsp::db(g as f64)
            };
            let mut frame = driven.map(|x| x * g);
            let (mut affected, mut clip) = (false, 0.0f64);
            if let Some(clipper) = &clipper {
                for x in &mut frame {
                    let value = *x as f64;
                    if !value.is_finite() {
                        return Err("Nonfinite limited sample".into());
                    }
                    let (shaped, reduction) = clipper.shape(value);
                    clip = clip.max(reduction);
                    affected |= reduction > 0.0;
                    *x = shaped as f32;
                }
                if counted {
                    clipping.maximum_reduction_db = clipping.maximum_reduction_db.max(clip);
                    clipped += usize::from(affected);
                }
            }
            if let (Some(bins), true) = (&mut trace, counted) {
                let position = measure_offset + *produced - measured.start;
                let bin = &mut bins[crate::waveform::bin_of(position, trace_frames)];
                bin.mean += reduction;
                bin.maximum = bin.maximum.max(reduction);
                bin.input_peak = bin
                    .input_peak
                    .max(driven[0].abs().max(driven[1].abs()) as f64);
                bin.output_peak = bin
                    .output_peak
                    .max(frame[0].abs().max(frame[1].abs()) as f64);
                bin.clip = bin.clip.max(clip);
                bin.clipped += usize::from(affected);
                bin.count += 1;
            }
            window.push(frame);
            *produced += 1;
            Ok(())
        };
    let decimate = |window: &mut Vec<Stereo>,
                    window_start: &mut usize,
                    produced: usize,
                    base: &mut usize,
                    output: &mut Vec<Stereo>| {
        // The output samples whose taps are all in the window, decimated on
        // the available cores in order.
        let mut last = *base;
        while last < frames && (last + radius < produced || produced == frames) {
            last += 4;
        }
        let count = (last - *base) / 4;
        let (window_ref, first, start) = (&*window, *base, *window_start);
        for chunk in dsp::chunked(count, 1 << 12, |range| {
            range
                .map(|k| os.down_at_window(window_ref, start, frames, first + 4 * k))
                .collect::<Vec<_>>()
        }) {
            output.extend(chunk);
        }
        *base = last;
        let retain = base.saturating_sub(radius).min(produced);
        let discard = retain - *window_start;
        window.copy_within(discard.., 0);
        window.truncate(window.len() - discard);
        *window_start = retain;
    };
    for start in (0..frames).step_by(block) {
        input.read(start, block.min(frames - start), &mut samples)?;
        for &sample in &samples {
            let driven = sample.map(|x| x * gain);
            if driven.iter().any(|x| !x.is_finite()) {
                return Err("Nonfinite driven sample".into());
            }
            pending.push_back(driven);
            let peak = driven[0].abs().max(driven[1].abs()) as f64;
            if let Some(g) = limiter.push(peak) {
                let frame = pending.pop_front().expect("pending frame");
                emit(g, frame, &mut window, &mut produced)?;
            }
        }
        decimate(
            &mut window,
            &mut window_start,
            produced,
            &mut base,
            &mut output,
        );
    }
    for g in limiter.flush(frames) {
        let frame = pending.pop_front().expect("pending frame");
        emit(g, frame, &mut window, &mut produced)?;
    }
    debug_assert!(pending.is_empty());
    decimate(
        &mut window,
        &mut window_start,
        produced,
        &mut base,
        &mut output,
    );
    clipping.affected_frames_percent = 100.0 * clipped as f64 / frames.max(1) as f64;
    let parts = Parts {
        stats: stats.clone(),
        clip_maximum_db: clipping.maximum_reduction_db,
        clipped,
        trace: trace.clone(),
    };
    let stats = stats.finish(rate);
    let trace = trace.map(|bins| {
        let output_frames = output.len();
        let level = |peak: f64| dsp::db(peak.max(1e-6));
        LimiterTrace {
            schema: "mastering.limiter-trace.2",
            sample_rate: rate as usize,
            input_frames: output_frames,
            bins: bins
                .iter()
                .enumerate()
                .map(|(index, bin)| LimiterTraceBin {
                    start_sample: crate::waveform::bin_start(index, output_frames),
                    end_sample: crate::waveform::bin_start(index + 1, output_frames),
                    mean_reduction_db: bin.mean / bin.count.max(1) as f64,
                    maximum_reduction_db: bin.maximum,
                    input_peak_db: level(bin.input_peak),
                    output_peak_db: level(bin.output_peak),
                    maximum_clip_db: bin.clip,
                    clipped_fraction: bin.clipped as f64 / bin.count.max(1) as f64,
                })
                .collect(),
        }
    });
    Ok(BlockRender {
        audio: output,
        stats,
        clipping: clipper.map(|_| clipping),
        trace,
        parts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn limiter_trace_follows_the_selected_render_and_matches_whole_song_stats() {
        let input: Vec<Stereo> = (0..192_000)
            .map(|index| {
                let level = if index < 48_000 { 1.5 } else { 0.1 };
                [level, -level]
            })
            .collect();
        let (audio, stats, _, trace) = render(
            &mut crate::scratch::Slice(&input),
            0.0,
            0.0,
            0.79,
            LimiterSettings::default(),
            48000,
        )
        .unwrap();
        assert_eq!(trace.schema, "mastering.limiter-trace.2");
        assert!(trace.bins[0].input_peak_db > trace.bins[0].output_peak_db);
        assert_eq!(trace.input_frames, audio.len());
        assert_eq!(trace.bins.len(), TRACE_BINS);
        assert!(trace.bins[0].mean_reduction_db > 1.0);
        assert!(trace.bins[TRACE_BINS - 1].mean_reduction_db < 0.01);
        let maximum = trace
            .bins
            .iter()
            .map(|bin| bin.maximum_reduction_db)
            .fold(0.0, f64::max);
        assert!((maximum - stats.maximum_reduction_db).abs() < 1e-6);
        let mean = trace
            .bins
            .iter()
            .map(|bin| bin.mean_reduction_db * (bin.end_sample - bin.start_sample) as f64)
            .sum::<f64>()
            / audio.len() as f64;
        assert!((mean - stats.mean_reduction_db).abs() < 0.01);
    }

    #[test]
    fn block_boundaries_preserve_samples_and_forward_statistics_exactly() {
        for len in [0, 1, 191, 385, 4117, 20003] {
            let input: Vec<_> = (0..len)
                .map(|i| {
                    let a =
                        ((i as f64 * 0.079).sin() * if i % 523 < 90 { 1.8 } else { 0.1 }) as f32;
                    [a, -a * 0.73]
                })
                .collect();
            for slow_release_ms in [0.0, 200.0] {
                for budget in [0.0, 0.7, 2.5] {
                    let settings = LimiterSettings {
                        release_ms: 57.905930920433576,
                        pre_attack_ms: 1.0,
                        hold_ms: 12.0,
                        slow_release_ms,
                        attack_ms: 0.0,
                    };
                    let mut reference = input.clone();
                    crate::treatment_render::scale(&mut reference, 7.3);
                    let stats =
                        dsp::limit(&mut reference, 0.79 * dsp::gain(budget), settings, 48000);
                    let clip = (budget > 0.0)
                        .then(|| crate::finishing::clip(&mut reference, 0.79, budget).unwrap());
                    let reference = Oversampler::new(48000).down(&reference);
                    for block in [1, 127, 192, 509, BLOCK] {
                        let (actual, actual_stats, actual_clip) = render_with_block(
                            &mut crate::scratch::Slice(&input),
                            7.3,
                            budget,
                            0.79,
                            settings,
                            block,
                        )
                        .unwrap();
                        assert!(actual.iter().flatten().map(|x|x.to_bits()).eq(reference.iter().flatten().map(|x|x.to_bits())),"PCM differs: len={len}, block={block}, slow={slow_release_ms}, budget={budget:?}");
                        assert_eq!(
                            serde_json::to_value(actual_stats).unwrap(),
                            serde_json::to_value(&stats).unwrap()
                        );
                        assert_eq!(
                            serde_json::to_value(actual_clip).unwrap(),
                            serde_json::to_value(clip).unwrap()
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn ceiling_holds_exactly_on_the_oversampled_signal() {
        let input: Vec<Stereo> = (0..96_000)
            .map(|i| {
                let t = i as f64 / 192_000.0;
                let burst = if i % 9_600 < 400 { 3.0 } else { 0.6 };
                let a = ((2.0 * std::f64::consts::PI * 55.0 * t).sin() * burst) as f32;
                [a, a * 0.8]
            })
            .collect();
        for budget in [0.0, 2.0] {
            let mut audio = input.clone();
            let ceiling = 0.7 * dsp::gain(budget);
            dsp::limit(&mut audio, ceiling, LimiterSettings::default(), 48000);
            let peak = audio.iter().flatten().fold(0.0f32, |a, &b| a.max(b.abs()));
            assert!(
                peak as f64 <= ceiling + 1e-6,
                "peak {peak} ceiling {ceiling}"
            );
            if budget > 0.0 {
                crate::finishing::clip(&mut audio, 0.7, budget).unwrap();
                let peak = audio.iter().flatten().fold(0.0f32, |a, &b| a.max(b.abs()));
                assert!(peak as f64 <= 0.7 + 1e-6);
            }
        }
    }
    #[test]
    fn the_true_peak_fit_matches_its_exhaustive_passes() {
        let mut state = 0x9e37_79b9_7f4a_7c15u64;
        let mut next = || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((state >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
        };
        let audio: Vec<Stereo> = (0..48_000 * 3)
            .map(|i| {
                let level = if (i / 7_000) % 3 == 0 { 0.95 } else { 0.4 };
                let x = (level * next()).clamp(-0.95, 0.95);
                [x, (0.8 * x + 0.2 * level * next()).clamp(-0.95, 0.95)]
            })
            .collect();
        for (ceiling_dbtp, true_peak) in [
            (-0.5, dsp::TruePeak::Local),
            (-1.0, dsp::TruePeak::Local),
            (-0.5, dsp::TruePeak::Trim),
            (3.0, dsp::TruePeak::Local),
        ] {
            // The passes of the fit before the scan.
            let os = Oversampler::new(48_000);
            let target = ceiling_dbtp - 0.001;
            let mut expected = audio.clone();
            let mut peak = dsp::db(os.peak(&expected));
            let mut local_db = 0.0;
            if true_peak == dsp::TruePeak::Local && peak > target {
                local_db = os.lower_peaks(&mut expected, dsp::gain(target - 0.001));
                peak = dsp::db(os.peak(&expected));
            }
            let trim_db = (target - peak).min(0.0);
            crate::treatment_render::scale(&mut expected, trim_db);
            let mut actual = audio.clone();
            let (fitted, fit) = fit_true_peak(&mut actual, ceiling_dbtp, true_peak, 48_000);
            assert_eq!(fitted.to_bits(), (peak + trim_db).to_bits());
            assert_eq!(fit.local_db.to_bits(), local_db.to_bits());
            assert_eq!(fit.trim_db.to_bits(), trim_db.to_bits());
            assert!(actual
                .iter()
                .flatten()
                .map(|x| x.to_bits())
                .eq(expected.iter().flatten().map(|x| x.to_bits())));
        }
    }

    #[test]
    fn a_window_renders_as_its_copy() {
        let input: Vec<Stereo> = (0..4 * 48_000 * 3)
            .map(|i| {
                let t = i as f64 / 192_000.0;
                let level = if (t - 1.7).abs() < 0.3 { 1.6 } else { 0.4 };
                let x = level * (2.0 * std::f64::consts::PI * 330.0 * t).sin();
                [x as f32, (0.7 * x) as f32]
            })
            .collect();
        let (start, len) = (4 * 48_000 + 3_008, 4 * 48_000 + 12_000);
        let options = || RenderOptions {
            rate: 48_000,
            drive_db: 3.0,
            clipper_db: 1.5,
            ceiling: 0.8,
            settings: LimiterSettings::default(),
            block: 4_096,
            collect_trace: true,
            measure: Some((2_000..len - 2_000, start + 2_000, input.len())),
        };
        let window = render_with_block_trace(
            &mut crate::scratch::Window {
                frames: &mut crate::scratch::Slice(&input),
                start,
                len,
            },
            options(),
        )
        .unwrap();
        let copy = render_with_block_trace(
            &mut crate::scratch::Slice(&input[start..start + len]),
            options(),
        )
        .unwrap();
        assert!(window.audio.iter().flatten().map(|x| x.to_bits()).eq(copy
            .audio
            .iter()
            .flatten()
            .map(|x| x.to_bits())));
        assert_eq!(
            serde_json::to_value(&window.stats).unwrap(),
            serde_json::to_value(&copy.stats).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&window.trace).unwrap(),
            serde_json::to_value(&copy.trace).unwrap()
        );
    }

    #[test]
    fn a_segmented_render_matches_the_whole_render() {
        // Seven seconds at 4x with bursts across each 2 s segment boundary.
        let input: Vec<Stereo> = (0..4 * 48_000 * 7)
            .map(|i| {
                let t = i as f64 / 192_000.0;
                let burst = (t % 2.0 - 1.98).abs() < 0.05 || (t - 3.3).abs() < 0.4;
                let level = if burst { 1.6 } else { 0.3 };
                let x = level * (2.0 * std::f64::consts::PI * 220.0 * t).sin();
                [x as f32, (0.8 * x) as f32]
            })
            .collect();
        let settings = LimiterSettings {
            release_ms: 10.0,
            ..LimiterSettings::default()
        };
        let (whole, stats, clipping, trace) = render(
            &mut crate::scratch::Slice(&input),
            2.0,
            1.5,
            0.8,
            settings,
            48000,
        )
        .unwrap();
        let (parts, part_stats, part_clipping, part_trace) = render_in_segments(
            &mut crate::scratch::Slice(&input),
            2.0,
            1.5,
            0.8,
            settings,
            4 * 48_000 * 2,
            48000,
        )
        .unwrap();
        assert_eq!(whole.len(), parts.len());
        let worst = whole
            .iter()
            .flatten()
            .zip(parts.iter().flatten())
            .map(|(a, b)| (a - b).abs())
            .fold(0.0f32, f32::max);
        assert!(worst < 1e-6, "{worst}");
        assert_eq!(stats.maximum_reduction_db, part_stats.maximum_reduction_db);
        assert_eq!(
            stats.longest_run_above_one_db_ms,
            part_stats.longest_run_above_one_db_ms
        );
        assert!((stats.mean_reduction_db - part_stats.mean_reduction_db).abs() < 1e-9);
        let (a, b) = (clipping.unwrap(), part_clipping.unwrap());
        assert_eq!(a.affected_frames_percent, b.affected_frames_percent);
        assert_eq!(a.maximum_reduction_db, b.maximum_reduction_db);
        for (x, y) in trace.bins.iter().zip(&part_trace.bins) {
            assert_eq!(x.start_sample, y.start_sample);
            assert_eq!(x.maximum_reduction_db, y.maximum_reduction_db);
            assert!((x.mean_reduction_db - y.mean_reduction_db).abs() < 1e-9);
        }
        println!("largest sample difference {worst:e}");
    }
}
