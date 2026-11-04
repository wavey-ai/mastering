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
#[derive(Clone, Copy, Default)]
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
    /// Whether the frames of `measure` count in the limiter statistics, the
    /// clipping and the trace. A render whose caller keeps the audio alone
    /// skips them (D295).
    statistics: bool,
    /// The 4x frames of the input whose statistics count, the position of
    /// the first of them in the whole recording, and the 4x frame count of
    /// the whole recording, which sets the trace bins. `None` measures the
    /// whole input.
    measure: Option<(std::ops::Range<usize>, usize, usize)>,
    /// The base-rate frame of the render that the first frame of the output
    /// holds. The render decimates only the frames that the output holds.
    keep: usize,
}

pub(crate) fn render(
    input: &mut impl crate::scratch::Frames,
    drive_db: f64,
    clipper_db: f64,
    ceiling: f64,
    settings: LimiterSettings,
    rate: u32,
) -> Result<(Vec<Stereo>, LimiterStats, Option<PeakStats>, LimiterTrace)> {
    let mut audio = vec![[0.0; 2]; input.len().div_ceil(4)];
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
            statistics: true,
            measure: None,
            keep: 0,
        },
        &mut audio,
    )?;
    Ok((
        audio,
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
            for base in group + range.start..group + range.end {
                block.extend(os.at_all(shaped, base));
            }
            let peak = dsp::sample_peak(&block);
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

/// Frames of a span of a signal that `Upsample` takes at a time: the frames
/// of one group of `upsample_into`.
pub(crate) const UPSAMPLE_SPAN: usize = if cfg!(target_arch = "wasm32") {
    1 << 16
} else {
    1 << 19
};

/// `upsample_into` of a signal of `frames` frames whose frames arrive in
/// pieces. The stream upsamples a span of `UPSAMPLE_SPAN` frames at a time
/// on the available cores. Each 4x frame reads `dsp::RADIUS` frames on each
/// side, so the stream holds the frames of the next span and the frames
/// before it that its 4x frames read.
pub(crate) struct Upsample {
    os: Oversampler,
    frames: usize,
    /// Frames of the signal from frame `start`.
    window: Vec<Stereo>,
    start: usize,
    /// The next frame to upsample.
    next: usize,
    peak: f64,
}

impl Upsample {
    /// The stream of a signal of `frames` frames at `rate`, in pieces of at
    /// most `piece` frames.
    pub(crate) fn new(frames: usize, rate: u32, piece: usize) -> Self {
        Self {
            os: Oversampler::new(rate),
            frames,
            window: Vec::with_capacity(UPSAMPLE_SPAN + piece + 2 * dsp::RADIUS as usize),
            start: 0,
            next: 0,
            peak: 0.0,
        }
    }

    /// Add the next frames of the signal, and write the 4x frames that
    /// they complete to `up`, on the available cores in order.
    pub(crate) fn push(&mut self, piece: &[Stereo], up: &mut crate::scratch::Pcm) -> Result<()> {
        const CHUNK: usize = 1 << 16;
        let radius = dsp::RADIUS as usize;
        self.window.extend_from_slice(piece);
        let received = self.start + self.window.len();
        // A 4x frame near the end of the signal reads to its end.
        let end = if received == self.frames {
            received
        } else {
            received.saturating_sub(radius)
        };
        if end == self.frames || end >= self.next + UPSAMPLE_SPAN {
            let (os, window, start, next) = (&self.os, &self.window, self.start, self.next);
            // The window holds `radius` frames on each side of each frame
            // inside the signal, or the end of the signal, so `at_all` reads
            // the frames of the whole signal.
            let blocks = dsp::chunked(end - next, CHUNK, |range| {
                let mut block = Vec::with_capacity(range.len() * 4);
                for base in next + range.start..next + range.end {
                    block.extend(os.at_all(window, base - start));
                }
                let peak = dsp::sample_peak(&block);
                (block, peak)
            });
            for (block, block_peak) in blocks {
                self.peak = self.peak.max(block_peak);
                for piece in block.chunks(16_384) {
                    up.push(piece)?;
                }
            }
            self.next = end;
        }
        let keep = self.next.saturating_sub(radius).max(self.start);
        self.window.drain(..keep - self.start);
        self.start = keep;
        Ok(())
    }

    /// The largest 4x sample of the signal.
    pub(crate) fn finish(self) -> f64 {
        assert_eq!(self.next, self.frames, "frames missing from the stream");
        self.peak
    }
}

/// Base-rate frames from which a prepared prefix of a recording at `rate`
/// keeps no 4x frames in scratch storage: 30 minutes. Its 4x frames come
/// from its frames at the base rate as they are read (`Upsampled`), and a
/// 4x prefix is four times the storage of the recording.
/// `MASTERING_UPSAMPLED_MIN_SECONDS` overrides it for measurement.
pub(crate) fn upsampled_min_frames(rate: u32) -> usize {
    static SECONDS: std::sync::OnceLock<Option<f64>> = std::sync::OnceLock::new();
    SECONDS
        .get_or_init(|| {
            std::env::var("MASTERING_UPSAMPLED_MIN_SECONDS")
                .ok()
                .and_then(|v| v.parse::<f64>().ok())
        })
        .map_or(rate as usize * 1800, |s| (s * rate as f64) as usize)
}

/// Base-rate frames from which a candidate render of a recording at `rate`
/// stays in scratch storage from its peak control to its delivery: 30
/// minutes (D253). A render in memory is one buffer of the recording.
/// `MASTERING_STORED_RENDER_MIN_SECONDS` overrides it for measurement.
pub(crate) fn stored_render_min_frames(rate: u32) -> usize {
    static SECONDS: std::sync::OnceLock<Option<f64>> = std::sync::OnceLock::new();
    SECONDS
        .get_or_init(|| {
            std::env::var("MASTERING_STORED_RENDER_MIN_SECONDS")
                .ok()
                .and_then(|v| v.parse::<f64>().ok())
        })
        .map_or(rate as usize * 1800, |s| (s * rate as f64) as usize)
}

/// Base-rate frames that `Upsampled` upsamples at a time.
const UPSAMPLED_SPAN: usize = 1 << 14;

/// The 4x frames of the signal in `base`, at `rate`, computed from the
/// frames of `base` as they are read: frame `i` is frame `i` of
/// `upsample_into` of the signal. Each 4x frame reads `dsp::RADIUS` frames
/// of `base` on each side, or the end of the signal (D251).
pub(crate) struct Upsampled<F: crate::scratch::Frames> {
    base: F,
    os: Oversampler,
    /// The frames of `base` that the last span read.
    window: Vec<Stereo>,
    /// The 4x frames from 4x frame `start`.
    span: Vec<Stereo>,
    start: usize,
}

impl<F: crate::scratch::Frames> Upsampled<F> {
    pub(crate) fn new(base: F, rate: u32) -> Self {
        Self {
            base,
            os: Oversampler::new(rate),
            window: Vec::new(),
            span: Vec::new(),
            start: 0,
        }
    }

    /// The 4x frames of the span of base-rate frames that holds 4x frame
    /// `at`.
    fn fill(&mut self, at: usize) -> Result<()> {
        let radius = dsp::RADIUS as usize;
        let frames = self.base.len();
        let first = at / 4;
        let end = (first + UPSAMPLED_SPAN).min(frames);
        let read = first.saturating_sub(radius)..(end + radius).min(frames);
        self.base.read(read.start, read.len(), &mut self.window)?;
        self.span.clear();
        for base in first..end {
            self.span
                .extend(self.os.at_all(&self.window, base - read.start));
        }
        self.start = 4 * first;
        Ok(())
    }
}

impl<F: crate::scratch::Frames> crate::scratch::Frames for Upsampled<F> {
    fn len(&self) -> usize {
        4 * self.base.len()
    }

    fn read(&mut self, start: usize, count: usize, output: &mut Vec<Stereo>) -> Result<()> {
        if start > self.len() || count > self.len() - start {
            return Err("Upsampled read exceeds its frame count".into());
        }
        output.clear();
        output.reserve_exact(count);
        let mut at = start;
        while at < start + count {
            if at < self.start || at >= self.start + self.span.len() {
                self.fill(at)?;
            }
            let offset = at - self.start;
            let take = (self.span.len() - offset).min(start + count - at);
            output.extend_from_slice(&self.span[offset..offset + take]);
            at += take;
        }
        Ok(())
    }
}

/// The 4x frames of a prepared prefix: frames in scratch storage, or the
/// frames of the prefix at the base rate, upsampled as they are read
/// (D251).
pub(crate) enum Up<'a, F: crate::scratch::Frames> {
    Stored(&'a mut crate::scratch::Pcm),
    Upsampled(Box<Upsampled<F>>),
}

impl<F: crate::scratch::Frames> crate::scratch::Frames for Up<'_, F> {
    fn len(&self) -> usize {
        match self {
            Up::Stored(up) => up.len(),
            Up::Upsampled(up) => up.len(),
        }
    }

    fn read(&mut self, start: usize, count: usize, output: &mut Vec<Stereo>) -> Result<()> {
        match self {
            Up::Stored(up) => up.read(start, count, output),
            Up::Upsampled(up) => up.read(start, count, output),
        }
    }
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

/// Drive change of each side of a drive response, in dB.
pub const DRIVE_RESPONSE_SHIFT_DB: f64 = 1.0;
/// Drive change of the far model of a drive response, in dB. One step of
/// Loudness or Peak level raises the drive by up to 2.87 dB on the eight
/// mixes (D283).
pub const DRIVE_RESPONSE_FAR_DB: f64 = 3.0;
/// Gain of the restoring filter of 20–120 Hz of the shelf model of a drive
/// response, in dB (D284).
pub const DRIVE_RESPONSE_SHELF_DB: f64 = 1.0;

/// Peak control at the base rate at one drive, for `drive_loudness`.
struct Shadow<'a> {
    gain: f32,
    limiter: Limiter,
    pending: VecDeque<Stereo>,
    clipper: Option<&'a dsp::SoftClip>,
}

impl Shadow<'_> {
    /// Push the base-rate frame `value` with its peak. Returns the frame
    /// `lag` frames earlier once it is available.
    fn push(&mut self, value: Stereo, peak: f32) -> Option<Stereo> {
        self.pending.push_back(value.map(|x| x * self.gain));
        let g = self.limiter.push((peak * self.gain) as f64)?;
        Some(self.limited(g))
    }

    fn limited(&mut self, g: f32) -> Stereo {
        let frame = self.pending.pop_front().expect("pending frame");
        frame.map(|x| {
            let x = x * g;
            self.clipper.map_or(x, |c| c.shape(x as f64).0 as f32)
        })
    }
}

/// The base-rate models of peak control of `drive_loudness`.
pub(crate) struct DriveModels {
    /// The integrated loudness at the drive, at the drive
    /// `DRIVE_RESPONSE_SHIFT_DB` up, at the drive `DRIVE_RESPONSE_SHIFT_DB`
    /// down, at the drive `DRIVE_RESPONSE_FAR_DB` up, of the shelf model
    /// (D284), of the prefix without peak control, and of the shelf model
    /// without peak control (D287).
    pub lufs: [f64; 7],
    /// The change of each held band (`low_end::BANDS_HZ`) in each section
    /// (`low_end::sections`) against the integrated loudness, from the model
    /// at the drive: at the drive `DRIVE_RESPONSE_SHIFT_DB` down, at the
    /// drive `DRIVE_RESPONSE_SHIFT_DB` up, without peak control (D282), and
    /// at the drive `DRIVE_RESPONSE_FAR_DB` up (D283), of the shelf model
    /// (D284), and of the shelf model without peak control (D287), in dB.
    pub sections_db: Vec<[[f64; 2]; 6]>,
    /// The window bands of the restoration (`treatment_render::WindowBands`)
    /// at the drive, at the drive `DRIVE_RESPONSE_SHIFT_DB` up and down, at
    /// the drive `DRIVE_RESPONSE_FAR_DB` up, without peak control, of the
    /// shelf model (D284), and of the shelf model without peak control
    /// (D287).
    pub windows: [crate::treatment_render::WindowBands; 7],
}

/// Rate of the decimated signal of `HeldSections`, in Hz: six times the top
/// of the held bands, as `compressor::HoldShadow`.
const HELD_RATE_HZ: f64 = 1500.0;

/// The block sums of the frames of one model in frame order: the block,
/// the sum of each channel, and the frames. The blocks of a section start
/// at the section, and each holds `factor` frames, or fewer at the end of
/// the section.
struct Blocks {
    span: usize,
    factor: usize,
    per_section: usize,
    /// The frame after the block in progress.
    end: usize,
    sums: Vec<(usize, [f64; 2], u32)>,
}

impl Blocks {
    fn new(rate: u32) -> Self {
        let span = crate::low_end::section_frames(rate);
        let factor = ((rate as f64 / HELD_RATE_HZ).floor() as usize).max(1);
        Self {
            span,
            factor,
            per_section: span.div_ceil(factor),
            end: 0,
            sums: Vec::new(),
        }
    }

    /// Add frame `i` of the recording, the frame after the last one added.
    fn push(&mut self, i: usize, frame: Stereo) {
        if self.sums.is_empty() || i >= self.end {
            let (section, k) = (i / self.span, i % self.span / self.factor);
            let start = section * self.span;
            self.end = (start + (k + 1) * self.factor).min(start + self.span);
            self.sums
                .push((section * self.per_section + k, [0.0; 2], 0));
        }
        let (_, sum, count) = self.sums.last_mut().expect("a block");
        sum[0] += frame[0] as f64;
        sum[1] += frame[1] as f64;
        *count += 1;
    }
}

/// The power of each held band in each section of one model, from the
/// spectrum of the means of its blocks (`Blocks`). The decimated changes of
/// the models at 1 dB up and down and without peak control differ from
/// those of the full spectrum by 0.009 dB or less on three mixes (D282).
struct HeldSections {
    span: usize,
    per_section: usize,
    frames: usize,
    rate: f64,
    section: usize,
    means: Vec<([f64; 2], u32)>,
    done: Vec<[f64; 2]>,
    planner: realfft::RealFftPlanner<f64>,
}

impl HeldSections {
    fn new(rate: u32, frames: usize) -> Self {
        let blocks = Blocks::new(rate);
        Self {
            span: blocks.span,
            per_section: blocks.per_section,
            frames,
            rate: rate as f64 / blocks.factor as f64,
            section: 0,
            means: Vec::new(),
            done: Vec::new(),
            planner: realfft::RealFftPlanner::new(),
        }
    }

    /// Add the block sums of the next frames of the model.
    fn add(&mut self, blocks: &Blocks) {
        for &(block, sum, count) in &blocks.sums {
            let (section, k) = (block / self.per_section, block % self.per_section);
            if section != self.section {
                self.end_section();
                self.section = section;
            }
            if self.means.len() <= k {
                self.means.resize(k + 1, ([0.0; 2], 0));
            }
            let (s, c) = &mut self.means[k];
            s[0] += sum[0];
            s[1] += sum[1];
            *c += count;
        }
    }

    /// Close the section in progress. A section counts when it is half a
    /// section long or longer, as in `low_end::sections`.
    fn end_section(&mut self) {
        let start = self.section * self.span;
        let end = (start + self.span).min(self.frames);
        let n = self.means.len();
        if n == 0 || end <= start || end - start < self.span / 2 {
            self.means.clear();
            return;
        }
        let fft = self.planner.plan_fft_forward(n);
        let (mut input, mut output) = (fft.make_input_vec(), fft.make_output_vec());
        let mut power = [0.0f64; 2];
        for c in 0..2 {
            for (x, (sum, count)) in input.iter_mut().zip(&self.means) {
                *x = sum[c] / (*count).max(1) as f64;
            }
            fft.process(&mut input, &mut output)
                .expect("section length");
            for (bin, value) in output.iter().enumerate() {
                let hz = bin as f64 * self.rate / n as f64;
                for (b, (low, high)) in crate::low_end::BANDS_HZ.iter().enumerate() {
                    if hz >= *low && hz < *high {
                        power[b] += value.norm_sqr();
                    }
                }
            }
        }
        self.done
            .push(power.map(|p| 10.0 * (p / n as f64).max(1e-30).log10()));
        self.means.clear();
    }

    fn finish(mut self) -> Vec<[f64; 2]> {
        self.end_section();
        self.done
    }
}

/// Base-rate models of peak control of `input`, the 4x signal of a
/// recording at `rate`: at `drive_db`, at the drive
/// `DRIVE_RESPONSE_SHIFT_DB` up and down and at the drive
/// `DRIVE_RESPONSE_FAR_DB` up, the prefix without peak control (D281,
/// D282, D283), and the shelf model: at `drive_db`, of the prefix with the
/// restoring filter of 20–120 Hz at `DRIVE_RESPONSE_SHELF_DB` (D284), and
/// without peak control (D287). Each
/// base-rate frame takes the largest of its four
/// 4x samples as its peak and its first 4x sample as its value. The shelf
/// model filters the values at the base rate and moves the largest 4x
/// sample of each channel of a frame by the change of its value. The changes
/// of this model and of the same model at 4x differ by 0.005 LU or less on
/// the eight mixes. The models run in segments on the available cores, as
/// `render_segmented`. Returns `None` for a silent prefix.
pub(crate) fn drive_loudness(
    input: &mut impl crate::scratch::Frames,
    drive_db: f64,
    clipper_db: f64,
    ceiling: f64,
    settings: LimiterSettings,
    rate: u32,
) -> Result<Option<DriveModels>> {
    shadow_loudness(
        input,
        drive_db,
        clipper_db,
        ceiling,
        settings,
        segment(rate),
        rate,
    )
}

/// `drive_loudness` in segments of `segment` 4x frames.
/// Each segment starts `segment_roll` frames early and ends `DRIVE_TAIL`
/// frames late, so the limiters and the loudness meters reach the core in
/// the state of a whole-file pass. The meters keep the 100 ms blocks of
/// each core, and the sections take the block sums of each core in order.
/// Each segment gives the window bands of the blocks that start in its
/// core.
fn shadow_loudness(
    input: &mut impl crate::scratch::Frames,
    drive_db: f64,
    clipper_db: f64,
    ceiling: f64,
    settings: LimiterSettings,
    segment: usize,
    rate: u32,
) -> Result<Option<DriveModels>> {
    let frames = input.len();
    let shifts = [
        0.0,
        DRIVE_RESPONSE_SHIFT_DB,
        -DRIVE_RESPONSE_SHIFT_DB,
        DRIVE_RESPONSE_FAR_DB,
    ];
    let roll = segment_roll(rate);
    let bucket = 4 * (rate as usize / 10);
    let block = crate::treatment_render::WINDOW_FFT;
    // The blocks of the complete restoration windows.
    let blocks = frames / 4 / crate::treatment_render::RESTORATION_WINDOW
        * (crate::treatment_render::RESTORATION_WINDOW / block);
    let shelf = [crate::treatment_render::restoring_filter(
        0,
        DRIVE_RESPONSE_SHELF_DB,
    )];
    if !frames.is_multiple_of(4)
        || !rate.is_multiple_of(20)
        || !segment.is_multiple_of(bucket)
        || !roll.is_multiple_of(bucket)
        || shifts.iter().any(|s| !dsp::gain(drive_db + s).is_finite())
    {
        return Err("Invalid drive response input".into());
    }
    settings.validate()?;
    let clipper = dsp::SoftClip::new(ceiling, clipper_db);
    let threshold = ceiling * dsp::gain(clipper_db);
    let cores: Vec<std::ops::Range<usize>> = (0..frames)
        .step_by(segment)
        .map(|start| start..(start + segment).min(frames))
        .collect();
    // The meter blocks, the block sums and the window blocks of the core of
    // each model over one segment, from its base-rate frames from 4x frame
    // `start`: the four drives, the prefix, the shelf model, then the shelf
    // model without peak control. The shelf
    // filters the base-rate frames from the start of the segment, and moves
    // the largest 4x sample of each channel of a frame by the change of the
    // frame.
    let shadow_segment =
        |core: &std::ops::Range<usize>, start: usize, values: &[Stereo], peaks: &[Stereo]| {
            let skip = (core.start - start) / 4;
            let keep = (core.end - start) / 4;
            let first = (core.start / 4).div_ceil(block).min(blocks);
            let last = (core.end / 4).div_ceil(block).min(blocks);
            let model = |shift: Option<f64>, shelved: bool| {
                let mut meter = dsp::LoudnessMeter::new(rate);
                let mut sections = Blocks::new(rate);
                let mut chunk = Vec::with_capacity(BLOCK);
                let mut scan = crate::treatment_render::BlockScan::new(rate);
                let mut windows = Vec::with_capacity(last - first);
                let mut pending = Vec::with_capacity(block);
                let mut emitted = 0;
                let mut emit = |frame: Stereo| {
                    if emitted < keep {
                        chunk.push(frame);
                        if chunk.len() == BLOCK {
                            meter.push(&chunk);
                            chunk.clear();
                        }
                        if emitted >= skip {
                            sections.push(start / 4 + emitted, frame);
                        }
                    }
                    let i = start / 4 + emitted;
                    if (first * block..last * block).contains(&i) {
                        pending.push(frame);
                        if pending.len() == block {
                            let mut bands = ([0.0; 6], 0.0, [0.0; 6]);
                            scan.add(&pending, &mut bands.0, &mut bands.1, &mut bands.2);
                            windows.push((i / block, bands));
                            pending.clear();
                        }
                    }
                    emitted += 1;
                };
                match shift {
                    Some(shift) => {
                        let limiter = Limiter::new(threshold, settings, rate / 4);
                        let mut shadow = Shadow {
                            gain: dsp::gain(drive_db + shift) as f32,
                            pending: VecDeque::with_capacity(limiter.lag() + 1),
                            limiter,
                            clipper: clipper.as_ref(),
                        };
                        let mut eq = shelved.then(|| dsp::Eq::new(&shelf, rate));
                        let mut buffer = Vec::new();
                        for (part, part_peaks) in values.chunks(BLOCK).zip(peaks.chunks(BLOCK)) {
                            let filtered: &[Stereo] = match eq.as_mut() {
                                Some(eq) => {
                                    buffer.clear();
                                    buffer.extend_from_slice(part);
                                    eq.process(&mut buffer);
                                    &buffer
                                }
                                None => part,
                            };
                            for ((value, plain), peak) in filtered.iter().zip(part).zip(part_peaks)
                            {
                                let peak = (peak[0] + (value[0] - plain[0]))
                                    .abs()
                                    .max((peak[1] + (value[1] - plain[1])).abs());
                                if let Some(frame) = shadow.push(*value, peak) {
                                    emit(frame);
                                }
                            }
                        }
                        // A segment that reads the last frame takes the gains
                        // of the frames that the lag holds.
                        if start + 4 * values.len() == frames {
                            for g in shadow.limiter.flush(values.len()) {
                                emit(shadow.limited(g));
                            }
                        }
                    }
                    None if shelved => {
                        let mut eq = dsp::Eq::new(&shelf, rate);
                        let mut buffer = Vec::with_capacity(BLOCK);
                        for part in values.chunks(BLOCK) {
                            buffer.clear();
                            buffer.extend_from_slice(part);
                            eq.process(&mut buffer);
                            buffer.iter().for_each(|v| emit(*v));
                        }
                    }
                    None => values.iter().for_each(|v| emit(*v)),
                }
                meter.push(&chunk);
                let mut buckets = meter.take_buckets();
                buckets.drain(..skip / (rate as usize / 10));
                (buckets, sections, windows)
            };
            [
                model(Some(shifts[0]), false),
                model(Some(shifts[1]), false),
                model(Some(shifts[2]), false),
                model(Some(shifts[3]), false),
                model(None, false),
                model(Some(shifts[0]), true),
                model(None, true),
            ]
        };
    let mut buckets: [Vec<f64>; 7] = Default::default();
    let mut held: [HeldSections; 7] = std::array::from_fn(|_| HeldSections::new(rate, frames / 4));
    let windows = blocks * block / crate::treatment_render::RESTORATION_WINDOW;
    let mut bands: [crate::treatment_render::WindowBands; 7] =
        std::array::from_fn(|_| crate::treatment_render::WindowBands {
            bands: vec![[0.0; 6]; windows],
            weighted: vec![0.0; windows],
            weighted_bands: [0.0; 6],
        });
    let per_window = crate::treatment_render::RESTORATION_WINDOW / block;
    #[cfg(not(target_arch = "wasm32"))]
    let threads = dsp::threads().min(8);
    #[cfg(target_arch = "wasm32")]
    let threads = 1;
    for batch in cores.chunks(threads) {
        let mut segments = Vec::with_capacity(batch.len());
        for core in batch {
            let start = core.start.saturating_sub(roll);
            let end = (core.end + DRIVE_TAIL).min(frames);
            segments.push((start, base_frames(input, start..end)?));
        }
        let parts = dsp::chunked(batch.len(), 1, |range| {
            let (start, (values, peaks)) = &segments[range.start];
            shadow_segment(&batch[range.start], *start, values, peaks)
        });
        for part in parts {
            for (((all, sections), windows), (part, blocks, measured)) in
                buckets.iter_mut().zip(&mut held).zip(&mut bands).zip(part)
            {
                all.extend(part);
                sections.add(&blocks);
                for (k, (power, level, weighted)) in measured {
                    let w = k / per_window;
                    for b in 0..6 {
                        windows.bands[w][b] += power[b];
                        windows.weighted_bands[b] += weighted[b];
                    }
                    windows.weighted[w] += level;
                }
            }
        }
    }
    let [Some(at), Some(up), Some(down), Some(far), Some(free), Some(shelved), Some(shelf_free)] =
        buckets.map(|b| dsp::gated_loudness(&b))
    else {
        return Ok(None);
    };
    let [at_db, up_db, down_db, far_db, free_db, shelf_db, shelf_free_db] =
        held.map(HeldSections::finish);
    let change = |model: &[f64; 2], lufs: f64, base: &[f64; 2]| -> [f64; 2] {
        std::array::from_fn(|b| model[b] - lufs - (base[b] - at))
    };
    let sections_db = (0..at_db.len())
        .map(|s| {
            [
                change(&down_db[s], down, &at_db[s]),
                change(&up_db[s], up, &at_db[s]),
                change(&free_db[s], free, &at_db[s]),
                change(&far_db[s], far, &at_db[s]),
                change(&shelf_db[s], shelved, &at_db[s]),
                change(&shelf_free_db[s], shelf_free, &at_db[s]),
            ]
        })
        .collect();
    Ok(Some(DriveModels {
        lufs: [at, up, down, far, shelved, free, shelf_free],
        sections_db,
        windows: bands,
    }))
}

/// The base-rate frames of 4x frames `range` of `input`: the first 4x sample
/// of each frame, and the 4x sample of each channel of the frame with the
/// largest magnitude.
fn base_frames(
    input: &mut impl crate::scratch::Frames,
    range: std::ops::Range<usize>,
) -> Result<(Vec<Stereo>, Vec<Stereo>)> {
    let mut samples = Vec::with_capacity(BLOCK);
    let (mut values, mut peaks) = (
        Vec::with_capacity(range.len() / 4),
        Vec::with_capacity(range.len() / 4),
    );
    for start in range.clone().step_by(BLOCK) {
        input.read(start, BLOCK.min(range.end - start), &mut samples)?;
        for group in samples.chunks_exact(4) {
            if group.iter().flatten().any(|x| !x.is_finite()) {
                return Err("Nonfinite prefix sample".into());
            }
            values.push(group[0]);
            let mut peak = [0.0f32; 2];
            for x in group {
                for c in 0..2 {
                    if x[c].abs() > peak[c].abs() {
                        peak[c] = x[c];
                    }
                }
            }
            peaks.push(peak);
        }
    }
    Ok((values, peaks))
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

/// Frames of the core of each span of a true-peak fit of a stored render.
const PEAK_SPAN: usize = 1 << 20;
/// Frames of a stored render on each side of the core of a span: the
/// radius of the interpolator, rounded up to a multiple of the blocks of
/// the true-peak scans, so the blocks of each span are blocks of the
/// whole render.
const PEAK_MARGIN: usize = 64;
/// Frames that the passes of `Oversampler::lower_peaks_from` read or change
/// beyond the first and the last frame over the ceiling, with a margin:
/// each pass adds a span of two local radii and a range of one radius of
/// the interpolator, 144 frames, to its frames over the ceiling.
const LOWER_MARGIN: usize = 2048;

/// The spans of a stored render of `frames` frames: the first frame of
/// each window and the core of the span in the window.
fn peak_spans(frames: usize) -> impl Iterator<Item = (usize, std::ops::Range<usize>)> {
    (0..frames).step_by(PEAK_SPAN).map(move |start| {
        let core = start..(start + PEAK_SPAN).min(frames);
        let first = core.start.saturating_sub(PEAK_MARGIN);
        (first, core)
    })
}

/// The frames of the window of the span with `core` that starts at
/// `first`, read into `window`. Returns the core in the window.
fn read_span(
    audio: &mut crate::scratch::Pcm,
    first: usize,
    core: &std::ops::Range<usize>,
    window: &mut Vec<Stereo>,
) -> Result<std::ops::Range<usize>> {
    let end = (core.end + PEAK_MARGIN).min(crate::scratch::Frames::len(audio));
    crate::scratch::Frames::read(audio, first, end - first, window)?;
    Ok(core.start - first..core.end - first)
}

/// The largest absolute sample of a stored render.
fn sample_peak_stored(audio: &mut crate::scratch::Pcm) -> Result<f64> {
    let mut peak = 0.0f64;
    let mut span = Vec::new();
    let frames = crate::scratch::Frames::len(audio);
    for start in (0..frames).step_by(PEAK_SPAN) {
        crate::scratch::Frames::read(audio, start, PEAK_SPAN.min(frames - start), &mut span)?;
        peak = peak.max(dsp::sample_peak(&span));
    }
    Ok(peak)
}

/// `fit_true_peak` of a render in scratch storage, read one span at a time
/// with the frames of the interpolator on each side. The scans of each
/// span start from the sample peak of the render, so each measures the
/// blocks and gives the peak of the scan of the whole render.
/// `TruePeak::Local` lowers each group of frames over the ceiling in a
/// window that holds each frame its passes read or change (D248).
pub(crate) fn fit_true_peak_stored(
    audio: &mut crate::scratch::Pcm,
    ceiling_dbtp: f64,
    true_peak: dsp::TruePeak,
    rate: u32,
) -> Result<(f64, TruePeakFit)> {
    let os = Oversampler::new(rate);
    let target = ceiling_dbtp - 0.001;
    let frames = crate::scratch::Frames::len(audio);
    let mut fit = TruePeakFit::default();
    let mut window = Vec::new();
    let floor = sample_peak_stored(audio)?;
    let mut peak = floor;
    let peak = if true_peak == dsp::TruePeak::Local {
        let ceiling = dsp::gain(target - 0.001);
        let (mut overs, mut scans) = (Vec::new(), Vec::new());
        for (first, core) in peak_spans(frames) {
            let local = read_span(audio, first, &core, &mut window)?;
            let mut scan = os.peak_scan_core(&window, local, ceiling, floor);
            peak = peak.max(scan.peak);
            overs.extend(
                scan.overs
                    .drain(..)
                    .map(|(frame, gain)| (frame + first, gain)),
            );
            scans.push(scan);
        }
        if dsp::db(peak) > target {
            let (local_db, changed) = lower_stored(&os, audio, ceiling, overs)?;
            fit.local_db = local_db;
            let floor = sample_peak_stored(audio)?;
            let mut after = floor;
            for ((first, core), scan) in peak_spans(frames).zip(&scans) {
                let local = read_span(audio, first, &core, &mut window)?;
                let end = first + window.len();
                let changed: Vec<_> = changed
                    .iter()
                    .filter(|range| range.start < end && range.end > first)
                    .map(|range| range.start.max(first) - first..range.end.min(end) - first)
                    .collect();
                after = after.max(os.peak_after_core(&window, local, scan, &changed, floor));
            }
            dsp::db(after)
        } else {
            dsp::db(peak)
        }
    } else {
        for (first, core) in peak_spans(frames) {
            let local = read_span(audio, first, &core, &mut window)?;
            peak = peak.max(os.peak_core_from(&window, local, floor));
        }
        dsp::db(peak)
    };
    fit.trim_db = (target - peak).min(0.0);
    let gain = dsp::gain(fit.trim_db) as f32;
    if gain != 1.0 {
        for start in (0..frames).step_by(PEAK_SPAN) {
            let count = PEAK_SPAN.min(frames - start);
            crate::scratch::Frames::read(audio, start, count, &mut window)?;
            crate::treatment_render::scale(&mut window, fit.trim_db);
            audio.write_at(start, &window)?;
        }
    }
    Ok((peak + fit.trim_db, fit))
}

/// `Oversampler::lower_peaks_from` of a stored render. The frames over the
/// ceiling form groups whose gaps exceed two margins; each group lowers in
/// a window of the render from one margin before its first frame to one
/// margin after its last. Returns the largest reduction in dB, and the
/// ranges of frames whose interpolated samples the passes changed.
fn lower_stored(
    os: &Oversampler,
    audio: &mut crate::scratch::Pcm,
    ceiling: f64,
    overs: Vec<(usize, f64)>,
) -> Result<(f64, Vec<std::ops::Range<usize>>)> {
    let frames = crate::scratch::Frames::len(audio);
    let mut largest = 0.0f64;
    let mut changed = Vec::new();
    let mut window = Vec::new();
    let mut first = 0;
    while first < overs.len() {
        let mut last = first;
        while last + 1 < overs.len() && overs[last + 1].0 - overs[last].0 <= 2 * LOWER_MARGIN {
            last += 1;
        }
        let start = overs[first].0.saturating_sub(LOWER_MARGIN);
        let end = (overs[last].0 + LOWER_MARGIN + 1).min(frames);
        crate::scratch::Frames::read(audio, start, end - start, &mut window)?;
        let group = overs[first..=last]
            .iter()
            .map(|&(frame, gain)| (frame - start, gain))
            .collect();
        let (reduction, ranges) = os.lower_peaks_from(&mut window, ceiling, group);
        largest = largest.max(reduction);
        changed.extend(
            ranges
                .into_iter()
                .map(|range| range.start + start..range.end + start),
        );
        audio.write_at(start, &window)?;
        first = last + 1;
    }
    Ok((largest, changed))
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

/// `render_peak_control` with the render in `output`, a new scratch file.
/// A segmented render stores the cores of each batch of segments in turn,
/// so it holds one batch of cores (D248). A render with a nonfinite sample
/// is an error.
pub(crate) fn render_peak_control_into(
    input: &mut impl crate::scratch::Frames,
    drive_db: f64,
    clipper_db: f64,
    ceiling: f64,
    settings: LimiterSettings,
    rate: u32,
    output: &mut crate::scratch::Pcm,
) -> Result<(LimiterStats, Option<PeakStats>, LimiterTrace)> {
    if input.len() > segmented_min_frames(rate) {
        render_in_segments(
            input,
            drive_db,
            clipper_db,
            ceiling,
            settings,
            segment(rate),
            rate,
            true,
            &mut RenderOutput::Stored {
                stored: output,
                batch: Vec::new(),
            },
        )
    } else {
        let (audio, stats, clipping, trace) =
            render(input, drive_db, clipper_db, ceiling, settings, rate)?;
        finite(&audio)?;
        output.push(&audio)?;
        Ok((stats, clipping, trace))
    }
}

/// The audio of `render_peak_control_into` alone: the render skips the
/// limiter statistics, the clipping and the trace (D295).
pub(crate) fn render_peak_control_audio_into(
    input: &mut impl crate::scratch::Frames,
    drive_db: f64,
    clipper_db: f64,
    ceiling: f64,
    settings: LimiterSettings,
    rate: u32,
    output: &mut crate::scratch::Pcm,
) -> Result<()> {
    if input.len() > segmented_min_frames(rate) {
        render_in_segments(
            input,
            drive_db,
            clipper_db,
            ceiling,
            settings,
            segment(rate),
            rate,
            false,
            &mut RenderOutput::Stored {
                stored: output,
                batch: Vec::new(),
            },
        )?;
    } else {
        let (audio, ..) = render(input, drive_db, clipper_db, ceiling, settings, rate)?;
        finite(&audio)?;
        output.push(&audio)?;
    }
    Ok(())
}

/// The output of a segmented render: the whole render in memory, or a
/// scratch file that takes the cores of each batch of segments.
enum RenderOutput<'a> {
    Whole(Vec<Stereo>),
    Stored {
        stored: &'a mut crate::scratch::Pcm,
        batch: Vec<Stereo>,
    },
}

impl RenderOutput<'_> {
    /// The places of the base-rate frames `first..end` of the render.
    fn places(&mut self, first: usize, end: usize) -> &mut [Stereo] {
        match self {
            RenderOutput::Whole(output) => {
                output.resize(end, [0.0; 2]);
                &mut output[first..]
            }
            RenderOutput::Stored { batch, .. } => {
                batch.clear();
                batch.resize(end - first, [0.0; 2]);
                batch
            }
        }
    }

    /// Store the frames of the last `places`.
    fn commit(&mut self) -> Result<()> {
        if let RenderOutput::Stored { stored, batch } = self {
            finite(batch)?;
            stored.push(batch)?;
        }
        Ok(())
    }
}

/// An error for a render with a nonfinite sample.
fn finite(audio: &[Stereo]) -> Result<()> {
    if audio.iter().flatten().any(|x| !x.is_finite()) {
        return Err("Nonfinite rendered samples".into());
    }
    Ok(())
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
/// 4x frames that a segment of the drive models renders after its core:
/// `SEGMENT_TAIL` and one block of the window bands (D284).
const DRIVE_TAIL: usize = SEGMENT_TAIL + 4 * crate::treatment_render::WINDOW_FFT;

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
    let mut output = RenderOutput::Whole(Vec::with_capacity(input.len() / 4));
    let (stats, clipping, trace) = render_in_segments(
        input,
        drive_db,
        clipper_db,
        ceiling,
        settings,
        segment(rate),
        rate,
        true,
        &mut output,
    )?;
    let RenderOutput::Whole(audio) = output else {
        unreachable!("a whole output")
    };
    Ok((audio, stats, clipping, trace))
}

#[allow(clippy::too_many_arguments)]
fn render_in_segments(
    input: &mut impl crate::scratch::Frames,
    drive_db: f64,
    clipper_db: f64,
    ceiling: f64,
    settings: LimiterSettings,
    segment: usize,
    rate: u32,
    statistics: bool,
    output: &mut RenderOutput<'_>,
) -> Result<(LimiterStats, Option<PeakStats>, LimiterTrace)> {
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
        statistics,
        measure: Some((core.start - start..core.end - start, core.start, frames)),
        keep: (core.start - start) / 4,
    };
    for batch in cores.chunks(threads) {
        // Each core renders into its place in the output: a buffer of the
        // whole segment at 48 kHz is 23 MB (D236).
        let first = batch[0].start / 4;
        let places = output
            .places(first, batch[batch.len() - 1].end / 4)
            .chunks_mut(segment / 4);
        let results: Vec<BlockRender> = if let [core] = batch {
            // A segment that renders alone reads its frames from the input in
            // place: a copy of a 61 s segment at 4x is 94 MB at 48 kHz.
            let (start, end) = bounds(core);
            let mut window = crate::scratch::Window {
                frames: &mut *input,
                start,
                len: end - start,
            };
            let place = places.into_iter().next().expect("place of the core");
            vec![render_with_block_trace(
                &mut window,
                options(core, start),
                place,
            )?]
        } else {
            // The segments render in parallel, and this thread reads the
            // frames of each from the input in blocks: a copy of a 61 s
            // segment at 4x is 94 MB at 48 kHz (D237).
            let (requests, served) = std::sync::mpsc::channel();
            let rendered: Vec<std::result::Result<BlockRender, String>> =
                std::thread::scope(|scope| {
                    let handles: Vec<_> = batch
                        .iter()
                        .zip(places)
                        .map(|(core, place)| {
                            let (start, end) = bounds(core);
                            let mut frames =
                                crate::scratch::Remote::new(requests.clone(), start, end - start);
                            let options = &options;
                            scope.spawn(move || {
                                render_with_block_trace(&mut frames, options(core, start), place)
                                    .map_err(|e| e.to_string())
                            })
                        })
                        .collect();
                    drop(requests);
                    crate::scratch::serve(&mut *input, served);
                    handles
                        .into_iter()
                        .map(|h| h.join().expect("segment thread"))
                        .collect()
                });
            let mut results = Vec::with_capacity(batch.len());
            for render in rendered {
                let render = render.map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;
                results.push(render);
            }
            results
        };
        output.commit()?;
        for render in results {
            clipper_used = render.clipping.is_some();
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
    let output_frames = frames / 4;
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
    Ok((parts.stats.finish(rate), clipping, trace))
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
    let mut audio = vec![[0.0; 2]; input.len().div_ceil(4)];
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
            statistics: true,
            measure: None,
            keep: 0,
        },
        &mut audio,
    )?;
    Ok((audio, result.stats, result.clipping))
}

// The input is a trait object, so one compiled loop serves each kind of
// input. With a loop for each kind, the plan of track-e took 2.7% more
// instructions (D237).
fn render_with_block_trace(
    input: &mut dyn crate::scratch::Frames,
    RenderOptions {
        rate,
        drive_db,
        clipper_db,
        ceiling,
        settings,
        block,
        collect_trace,
        statistics,
        measure,
        keep,
    }: RenderOptions,
    output: &mut [Stereo],
) -> Result<BlockRender> {
    assert!(block > 0);
    let frames = input.len();
    let (measured, measure_offset, trace_frames) = measure.unwrap_or((0..frames, 0, frames));
    let measured = if statistics { measured } else { 0..0 };
    let gain = dsp::gain(drive_db) as f32;
    if !gain.is_finite() || !(0.0..=6.0).contains(&clipper_db) {
        return Err("Invalid blocked peak-control input".into());
    }
    settings.validate()?;
    let clipper = dsp::SoftClip::new(ceiling, clipper_db);
    let mut limiter = Limiter::new(ceiling * dsp::gain(clipper_db), settings, rate);
    let mut pending: VecDeque<Stereo> = VecDeque::with_capacity(limiter.lag() + block + 1);
    let os = Oversampler::new(rate);
    let radius = os.down_radius();
    let mut window = Vec::with_capacity(block + radius * 2 + 4);
    let mut window_start = 0;
    let mut produced = 0;
    let mut base = 0;
    let mut stats = LimiterStatsAccumulator::default();
    let mut clipping = PeakStats::default();
    let mut clipped = 0usize;
    let mut trace = collect_trace.then(|| vec![TraceAccumulator::default(); TRACE_BINS]);
    // The trace bin of the last counted frame and the first position of the
    // next bin. The positions of the counted frames increase by one, so the
    // frames of a bin come in one run. The first counted frame takes its
    // bin from `bin_of` (D318, D333).
    let mut trace_at = (0usize, 0usize);
    let mut samples = Vec::with_capacity(block);
    // The frames of a block go through `emit` in one call with the gains of
    // the block. The accumulators of the frames are locals of the call, and
    // a call for each frame kept them in memory (D298).
    let mut emit = |gains: &[f32],
                    pending: &mut VecDeque<Stereo>,
                    window: &mut Vec<Stereo>,
                    produced: &mut usize|
     -> Result<()> {
        let mut acc = std::mem::take(&mut stats);
        let (mut clip_maximum, mut clipped_frames) = (clipping.maximum_reduction_db, clipped);
        // The accumulators of the bin of the run in a local: in the vector,
        // each counted frame took a load and a store of each one (D333).
        let (mut trace_bin, mut next_bin) = trace_at;
        let mut run = trace
            .as_ref()
            .map_or_else(TraceAccumulator::default, |bins| bins[trace_bin]);
        let mut at = *produced;
        window.reserve(gains.len());
        for (&g, driven) in gains.iter().zip(pending.drain(..gains.len())) {
            let counted = measured.contains(&at);
            let reduction = if counted { acc.push(g) } else { 0.0 };
            let mut frame = driven.map(|x| x * g);
            let (mut affected, mut clip) = (false, 0.0f64);
            if let Some(clipper) = &clipper {
                for x in &mut frame {
                    let value = *x as f64;
                    if !value.is_finite() {
                        return Err("Nonfinite limited sample".into());
                    }
                    if counted {
                        let (shaped, reduction) = clipper.shape(value);
                        clip = dsp::larger(clip, reduction);
                        affected |= reduction > 0.0;
                        *x = shaped as f32;
                    } else {
                        *x = clipper.shaped(value) as f32;
                    }
                }
                if counted {
                    clip_maximum = dsp::larger(clip_maximum, clip);
                    clipped_frames += usize::from(affected);
                }
            }
            if let (Some(bins), true) = (&mut trace, counted) {
                let position = measure_offset + at - measured.start;
                if position >= next_bin {
                    bins[trace_bin] = run;
                    trace_bin = crate::waveform::bin_of(position, trace_frames);
                    next_bin = crate::waveform::bin_start(trace_bin + 1, trace_frames);
                    run = bins[trace_bin];
                }
                run.mean += reduction;
                // The maximum starts at 0.0 and a reduction is not NaN, so a
                // comparison gives the value of `max` (D333).
                run.maximum = dsp::larger(run.maximum, reduction);
                run.input_peak = dsp::larger(
                    run.input_peak,
                    dsp::larger(driven[0].abs(), driven[1].abs()) as f64,
                );
                run.output_peak = dsp::larger(
                    run.output_peak,
                    dsp::larger(frame[0].abs(), frame[1].abs()) as f64,
                );
                run.clip = dsp::larger(run.clip, clip);
                run.clipped += usize::from(affected);
                run.count += 1;
            }
            window.push(frame);
            at += 1;
        }
        stats = acc;
        (clipping.maximum_reduction_db, clipped) = (clip_maximum, clipped_frames);
        if let Some(bins) = &mut trace {
            bins[trace_bin] = run;
        }
        trace_at = (trace_bin, next_bin);
        *produced = at;
        Ok(())
    };
    let decimate = |window: &mut Vec<Stereo>,
                    window_start: &mut usize,
                    produced: usize,
                    base: &mut usize,
                    output: &mut [Stereo]| {
        // The output samples whose taps are all in the window, decimated on
        // the available cores in order.
        let mut last = *base;
        while last < frames && (last + radius < produced || produced == frames) {
            last += 4;
        }
        let held = keep..keep + output.len();
        let first = (*base / 4).clamp(held.start, held.end);
        let end = (last / 4).clamp(held.start, held.end);
        let (window_ref, start) = (&*window, *window_start);
        let mut at = first - keep;
        for chunk in dsp::chunked(end - first, 1 << 12, |range| {
            os.down_range_at_window(
                window_ref,
                start,
                frames,
                4 * (first + range.start),
                range.len(),
            )
        }) {
            output[at..at + chunk.len()].copy_from_slice(&chunk);
            at += chunk.len();
        }
        *base = last;
        let retain = base.saturating_sub(radius).min(produced);
        let discard = retain - *window_start;
        window.copy_within(discard.., 0);
        window.truncate(window.len() - discard);
        *window_start = retain;
    };
    // The drive, the check and the queue take the block in whole passes,
    // and the limiter takes one frame at a time. A queue and a vector of
    // gains in memory took a load and a store of their lengths at each
    // frame around the call of the limiter (D305).
    let mut gains = vec![0.0f32; block];
    for start in (0..frames).step_by(block) {
        input.read(start, block.min(frames - start), &mut samples)?;
        let mut finite = true;
        for sample in &mut samples {
            *sample = sample.map(|x| x * gain);
            finite &= sample[0].is_finite() & sample[1].is_finite();
        }
        if !finite {
            return Err("Nonfinite driven sample".into());
        }
        pending.extend(&samples);
        let mut count = 0;
        for driven in &samples {
            let peak = dsp::larger(driven[0].abs(), driven[1].abs()) as f64;
            if let Some(g) = limiter.push(peak) {
                gains[count] = g;
                count += 1;
            }
        }
        emit(&gains[..count], &mut pending, &mut window, &mut produced)?;
        decimate(&mut window, &mut window_start, produced, &mut base, output);
    }
    emit(
        &limiter.flush(frames),
        &mut pending,
        &mut window,
        &mut produced,
    )?;
    debug_assert!(pending.is_empty());
    decimate(&mut window, &mut window_start, produced, &mut base, output);
    clipping.affected_frames_percent = 100.0 * clipped as f64 / frames.max(1) as f64;
    let parts = Parts {
        stats: stats.clone(),
        clip_maximum_db: clipping.maximum_reduction_db,
        clipped,
        trace: trace.clone(),
    };
    let stats = stats.finish(rate);
    let trace = trace.map(|bins| {
        let output_frames = frames.div_ceil(4);
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
    fn a_stored_render_gives_the_true_peak_fit_of_the_render_in_memory() {
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut next = || {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((state >> 40) as f32 / (1u64 << 24) as f32) * 2.0 - 1.0
        };
        // Loud passages every 21,000 frames, one across the end of the
        // first span, and short bursts 3,000 frames apart: groups of
        // frames over the ceiling that are near or far from each other.
        let audio: Vec<Stereo> = (0..PEAK_SPAN + 300_311)
            .map(|i| {
                let loud = (i / 7_000) % 3 == 0
                    || (PEAK_SPAN - 3_000..PEAK_SPAN + 3_000).contains(&i)
                    || ((400_000..500_000).contains(&i) && i % 3_000 < 50);
                let level = if loud { 0.95 } else { 0.4 };
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
            let mut expected = audio.clone();
            let (peak, fit) = fit_true_peak(&mut expected, ceiling_dbtp, true_peak, 48_000);
            let mut stored = crate::scratch::Pcm::new(crate::scratch::memory().unwrap());
            stored.push(&audio).unwrap();
            let (stored_peak, stored_fit) =
                fit_true_peak_stored(&mut stored, ceiling_dbtp, true_peak, 48_000).unwrap();
            assert_eq!(stored_peak.to_bits(), peak.to_bits());
            assert_eq!(stored_fit.local_db.to_bits(), fit.local_db.to_bits());
            assert_eq!(stored_fit.trim_db.to_bits(), fit.trim_db.to_bits());
            let mut read = Vec::new();
            crate::scratch::Frames::read(&mut stored, 0, audio.len(), &mut read).unwrap();
            assert!(read
                .iter()
                .flatten()
                .map(|x| x.to_bits())
                .eq(expected.iter().flatten().map(|x| x.to_bits())));
            // The measures of the stored render read it in spans (D248).
            assert_eq!(
                dsp::loudness_read(&mut stored, 48_000)
                    .unwrap()
                    .map(f64::to_bits),
                dsp::loudness(&expected, 48_000).map(f64::to_bits)
            );
            let blocks = crate::analysis::blocks(&expected, 48_000);
            let spans = crate::analysis::blocks_read(&mut stored, 48_000).unwrap();
            assert_eq!(spans.len(), blocks.len());
            assert!(spans.iter().zip(&blocks).all(|(a, b)| {
                a.energy.to_bits() == b.energy.to_bits() && a.peak.to_bits() == b.peak.to_bits()
            }));
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
        let options = |keep| RenderOptions {
            rate: 48_000,
            drive_db: 3.0,
            clipper_db: 1.5,
            ceiling: 0.8,
            settings: LimiterSettings::default(),
            block: 4_096,
            collect_trace: true,
            statistics: true,
            measure: Some((2_000..len - 2_000, start + 2_000, input.len())),
            keep,
        };
        // The window holds 40,000 of the 51,000 frames of its render.
        let mut held = vec![[0.0; 2]; 40_000];
        let window = render_with_block_trace(
            &mut crate::scratch::Window {
                frames: &mut crate::scratch::Slice(&input),
                start,
                len,
            },
            options(3_000),
            &mut held,
        )
        .unwrap();
        let mut whole = vec![[0.0; 2]; len / 4];
        let copy = render_with_block_trace(
            &mut crate::scratch::Slice(&input[start..start + len]),
            options(0),
            &mut whole,
        )
        .unwrap();
        assert!(held
            .iter()
            .flatten()
            .map(|x| x.to_bits())
            .eq(whole[3_000..43_000].iter().flatten().map(|x| x.to_bits())));
        assert_eq!(
            serde_json::to_value(&window.stats).unwrap(),
            serde_json::to_value(&copy.stats).unwrap()
        );
        assert_eq!(
            serde_json::to_value(&window.trace).unwrap(),
            serde_json::to_value(&copy.trace).unwrap()
        );
    }

    /// The trace bin of each counted frame is the bin of its position
    /// (`waveform::bin_of`), for blocks of any length and a measure that
    /// starts inside a longer signal (D333).
    #[test]
    fn the_trace_bins_of_the_counted_frames_follow_their_positions() {
        let input: Vec<Stereo> = (0..4 * 48_000 * 2)
            .map(|i| {
                let x = (0.9 * (i as f64 * 0.013).sin()) as f32;
                [x, -0.6 * x]
            })
            .collect();
        let len = input.len();
        let (offset, total) = (123_457, 3 * len + 11);
        let mut expected = vec![0usize; TRACE_BINS];
        for position in offset..offset + len - 3_000 {
            expected[crate::waveform::bin_of(position, total)] += 1;
        }
        assert!(expected.iter().filter(|&&count| count > 0).count() > 100);
        for block in [1, 997, 4_096] {
            let mut output = vec![[0.0; 2]; len / 4];
            let render = render_with_block_trace(
                &mut crate::scratch::Slice(&input),
                RenderOptions {
                    rate: 48_000,
                    drive_db: 3.0,
                    clipper_db: 1.5,
                    ceiling: 0.8,
                    settings: LimiterSettings::default(),
                    block,
                    collect_trace: true,
                    statistics: true,
                    measure: Some((1_000..len - 2_000, offset, total)),
                    keep: 0,
                },
                &mut output,
            )
            .unwrap();
            let counts: Vec<usize> = render
                .parts
                .trace
                .expect("a trace")
                .iter()
                .map(|bin| bin.count)
                .collect();
            assert_eq!(counts, expected, "{block}");
        }
    }

    /// Reads of the 4x frames of a signal from its frames at the base rate
    /// give the frames of `upsample_into` (D251).
    #[test]
    fn upsampled_reads_give_the_frames_of_upsample_into() {
        use crate::scratch::Frames;
        let frames = 3 * UPSAMPLED_SPAN + 77;
        let signal: Vec<Stereo> = (0..frames)
            .map(|i| {
                let t = i as f32;
                [0.8 * (t * 0.0137).sin(), 0.6 * (t * 0.0071).cos()]
            })
            .collect();
        let mut up = crate::scratch::Pcm::new(crate::scratch::memory().unwrap());
        upsample_into(&signal, &mut up, 48_000).unwrap();
        let mut whole = Vec::new();
        up.read(0, up.len(), &mut whole).unwrap();
        let bits = |frames: &[Stereo]| -> Vec<[u32; 2]> {
            frames.iter().map(|f| f.map(f32::to_bits)).collect()
        };
        let mut upsampled = Upsampled::new(crate::scratch::Slice(&signal), 48_000);
        assert_eq!(upsampled.len(), whole.len());
        let mut read = Vec::new();
        for (start, count) in [
            (0, 5),
            (3, 4 * UPSAMPLED_SPAN + 9),
            (4 * UPSAMPLED_SPAN - 2, 7),
            (2, 1),
            (whole.len() - 13, 13),
            (0, whole.len()),
        ] {
            upsampled.read(start, count, &mut read).unwrap();
            assert_eq!(bits(&read), bits(&whole[start..start + count]), "{start}");
        }
        assert!(upsampled.read(whole.len() - 3, 4, &mut read).is_err());
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
        let mut output = RenderOutput::Whole(Vec::new());
        let (part_stats, part_clipping, part_trace) = render_in_segments(
            &mut crate::scratch::Slice(&input),
            2.0,
            1.5,
            0.8,
            settings,
            4 * 48_000 * 2,
            48000,
            true,
            &mut output,
        )
        .unwrap();
        let RenderOutput::Whole(parts) = output else {
            unreachable!("a whole output")
        };
        // The cores of each batch in scratch storage are the cores in
        // memory (D248).
        let mut stored = crate::scratch::Pcm::new(crate::scratch::temporary().unwrap());
        render_in_segments(
            &mut crate::scratch::Slice(&input),
            2.0,
            1.5,
            0.8,
            settings,
            4 * 48_000 * 2,
            48000,
            true,
            &mut RenderOutput::Stored {
                stored: &mut stored,
                batch: Vec::new(),
            },
        )
        .unwrap();
        let mut read = Vec::new();
        let frames = crate::scratch::Frames::len(&stored);
        crate::scratch::Frames::read(&mut stored, 0, frames, &mut read).unwrap();
        assert_eq!(read, parts);
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

    #[test]
    fn a_render_without_statistics_has_the_audio_of_a_render_with_them() {
        // Five seconds at 4x with bursts above the knee of the clipper.
        let input: Vec<Stereo> = (0..4 * 48_000 * 5)
            .map(|i| {
                let t = i as f64 / 192_000.0;
                let level = if (t % 1.5 - 1.2).abs() < 0.2 {
                    1.8
                } else {
                    0.4
                };
                let x = level * (2.0 * std::f64::consts::PI * 180.0 * t).sin();
                [x as f32, (0.7 * x) as f32]
            })
            .collect();
        let audio = |statistics| {
            let mut output = RenderOutput::Whole(Vec::new());
            render_in_segments(
                &mut crate::scratch::Slice(&input),
                2.0,
                1.5,
                0.8,
                LimiterSettings::default(),
                4 * 48_000 * 2,
                48000,
                statistics,
                &mut output,
            )
            .unwrap();
            let RenderOutput::Whole(audio) = output else {
                unreachable!("a whole output")
            };
            audio
        };
        let bits = |frames: &[Stereo]| -> Vec<[u32; 2]> {
            frames.iter().map(|f| f.map(f32::to_bits)).collect()
        };
        let (with, without) = (audio(true), audio(false));
        assert_eq!(with.len(), input.len() / 4);
        assert_eq!(bits(&with), bits(&without));
    }

    /// Bursts over a steady tone: `frames` frames of the 4x signal of
    /// 48 kHz.
    fn response_signal(frames: usize) -> Vec<Stereo> {
        (0..frames)
            .map(|i| {
                let t = i as f64 / 192_000.0;
                let level = if i % 76_800 < 9_600 { 1.6 } else { 0.25 };
                let tone = (2.0 * std::f64::consts::PI * 110.0 * t).sin()
                    + 0.5 * (2.0 * std::f64::consts::PI * 1700.0 * t).sin();
                let a = (level * tone / 1.5) as f32;
                [a, a * 0.8]
            })
            .collect()
    }

    #[test]
    fn a_segmented_drive_loudness_matches_the_whole_pass() {
        for rate in [44_100, 48_000] {
            // 4x frames of one 100 ms block of the meter. The last segment
            // of 40 blocks holds 100 base-rate frames, fewer than the lag of
            // the limiter.
            let block = 4 * rate as usize / 10;
            let input = response_signal(40 * block + 400);
            let loudness = |segment| {
                shadow_loudness(
                    &mut crate::scratch::Slice(&input),
                    3.0,
                    1.0,
                    0.79,
                    LimiterSettings::default(),
                    segment,
                    rate,
                )
                .unwrap()
                .unwrap()
            };
            let whole = loudness(41 * block).lufs;
            for segment in [10 * block, 15 * block, 40 * block] {
                let parts = loudness(segment).lufs;
                for (a, b) in parts.iter().zip(&whole) {
                    assert!(
                        (a - b).abs() < 1e-6,
                        "{rate} {segment}: {parts:?} {whole:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_drive_models_give_the_change_of_each_section() {
        for rate in [44_100, 48_000] {
            // A loud first section of bursts, and a quiet second section of
            // the tone alone, at 4x frames of the rate.
            let span = 4 * crate::low_end::section_frames(rate);
            let mut input = response_signal(2 * span);
            for (i, f) in input.iter_mut().enumerate().skip(span) {
                let t = i as f64 / (4 * rate) as f64;
                let tone = (2.0 * std::f64::consts::PI * 110.0 * t).sin()
                    + 0.5 * (2.0 * std::f64::consts::PI * 1700.0 * t).sin();
                let a = (0.25 * tone / 1.5) as f32;
                *f = [a, a * 0.8];
            }
            let models = |drive, segment| {
                shadow_loudness(
                    &mut crate::scratch::Slice(&input),
                    drive,
                    1.0,
                    0.79,
                    LimiterSettings::default(),
                    segment,
                    rate,
                )
                .unwrap()
                .unwrap()
                .sections_db
            };
            // The sections of segments that cut a section match those of one
            // pass.
            let bucket = 4 * rate as usize / 10;
            let whole = models(3.0, 2 * span);
            assert_eq!(whole.len(), 2);
            for segment in [7 * 10 * bucket, 40 * 10 * bucket] {
                let parts = models(3.0, segment);
                for (a, b) in parts
                    .iter()
                    .flatten()
                    .flatten()
                    .zip(whole.iter().flatten().flatten())
                {
                    assert!(
                        (a - b).abs() < 1e-9,
                        "{rate} {segment}: {parts:?} {whole:?}"
                    );
                }
            }
            // Peak control takes bass from the loud section. A lower drive
            // gives it back, and the quiet section loses against the
            // loudness; without peak control, more.
            let [loud, quiet] = [whole[0], whole[1]];
            assert!(loud[0][0] > 0.0 && loud[1][0] < 0.0 && loud[2][0] > loud[0][0]);
            assert!(quiet[0][0] < 0.0 && quiet[1][0] > 0.0 && quiet[2][0] < quiet[0][0]);
            // The far model takes more than the model at 1 dB up.
            assert!(loud[3][0] < loud[1][0] && quiet[3][0] > quiet[1][0]);
            // Under the threshold, no model of the drive changes a section,
            // and the shelf raises the band of the 110 Hz tone. Peak control
            // takes part of that rise from the loud section.
            let free = models(-20.0, 2 * span);
            for change in free.iter().flat_map(|s| &s[..4]).flatten() {
                assert!(change.abs() < 1e-3, "{change}");
            }
            for s in &free {
                assert!(s[4][0] > 0.1 && s[4][0] < 1.0, "{:?}", s[4]);
            }
            assert!(
                loud[4][0] > 0.0 && loud[4][0] < free[0][4][0] - 0.05,
                "{loud:?}"
            );
            // The shelf model without peak control gives the rise of the
            // shelf model under the threshold (D287). In the loud section,
            // the change of the shelf model less its rise without peak
            // control gives the share that peak control takes.
            for s in &free {
                assert!((s[5][0] - s[4][0]).abs() < 1e-3, "{s:?}");
            }
            let taken = loud[4][0] - (loud[5][0] - loud[2][0]);
            assert!(taken < -0.05, "{loud:?}");
            assert!(
                (loud[5][0] - loud[2][0] - free[0][5][0]).abs() < 1e-3,
                "{loud:?}"
            );
        }
    }

    #[test]
    fn the_drive_models_give_the_window_bands_of_each_model() {
        let rate = 48_000;
        let input = response_signal(4 * (3 * rate as usize + 1234));
        let models = |drive, segment| {
            shadow_loudness(
                &mut crate::scratch::Slice(&input),
                drive,
                1.0,
                0.79,
                LimiterSettings::default(),
                segment,
                rate,
            )
            .unwrap()
            .unwrap()
        };
        let bucket = 4 * rate as usize / 10;
        let whole = models(3.0, 40 * bucket).windows;
        assert_eq!(whole[0].bands.len(), 8);
        // The prefix without peak control has the window bands of its
        // base-rate frames.
        let base: Vec<Stereo> = input.chunks_exact(4).map(|g| g[0]).collect();
        let measured = crate::treatment_render::WindowBands::measure(&base, rate);
        let close = |a: &crate::treatment_render::WindowBands,
                     b: &crate::treatment_render::WindowBands,
                     tolerance: f64| {
            assert_eq!(a.bands.len(), b.bands.len());
            for (x, y) in a.bands.iter().flatten().zip(b.bands.iter().flatten()) {
                assert!((x - y).abs() <= tolerance * y.abs().max(1e-12), "{x} {y}");
            }
            for (x, y) in a.weighted.iter().zip(&b.weighted) {
                assert!((x - y).abs() <= tolerance * y.abs(), "{x} {y}");
            }
        };
        close(&whole[4], &measured, 1e-12);
        // The shelf model without peak control has the window bands of its
        // base-rate frames through the shelf (D287).
        let mut shelved = base.clone();
        dsp::Eq::new(
            &[crate::treatment_render::restoring_filter(
                0,
                DRIVE_RESPONSE_SHELF_DB,
            )],
            rate,
        )
        .process(&mut shelved);
        close(
            &whole[6],
            &crate::treatment_render::WindowBands::measure(&shelved, rate),
            1e-9,
        );
        // Segments that cut the blocks give the window bands of one pass.
        // The limiter's moving averages carry rounding from the whole
        // history (`render_segmented`).
        for segment in [7 * bucket, 13 * bucket] {
            let parts = models(3.0, segment).windows;
            close(&parts[4], &whole[4], 1e-12);
            for (a, b) in parts.iter().zip(&whole) {
                close(a, b, 1e-6);
            }
        }
        // Under the threshold, the shelf model raises the band of the 110 Hz
        // tone and keeps the band of the 1700 Hz tone.
        let quiet = models(-20.0, 40 * bucket);
        let [at, shelf] = [&quiet.windows[0], &quiet.windows[5]];
        for (a, s) in at.bands.iter().zip(&shelf.bands) {
            let db = |b: usize| 10.0 * (s[b] / a[b]).log10();
            assert!(db(0) > 0.3 && db(0) < 1.0, "{}", db(0));
            assert!(db(2).abs() < 0.02, "{}", db(2));
        }
        assert!(quiet.lufs[4] > quiet.lufs[0]);
    }

    #[test]
    fn the_drive_loudness_gives_the_loudness_changes_of_the_render() {
        let input = response_signal(768_000);
        let settings = LimiterSettings::default();
        // The changes up, down and far up from the drive.
        let changes = |drive| {
            let [at, up, down, far, ..] = drive_loudness(
                &mut crate::scratch::Slice(&input),
                drive,
                1.0,
                0.79,
                settings,
                48000,
            )
            .unwrap()
            .unwrap()
            .lufs;
            [up - at, down - at, far - at]
        };
        // Under the threshold, the loudness follows the drive.
        let quiet = changes(-20.0);
        assert!((quiet[0] - 1.0).abs() < 1e-3, "{quiet:?}");
        assert!((quiet[1] + 1.0).abs() < 1e-3, "{quiet:?}");
        assert!((quiet[2] - 3.0).abs() < 1e-3, "{quiet:?}");
        // Under peak control, the base-rate model gives the loudness changes
        // of the render at 4x.
        let loudness = |drive| {
            let (audio, ..) = render(
                &mut crate::scratch::Slice(&input),
                drive,
                1.0,
                0.79,
                settings,
                48000,
            )
            .unwrap();
            dsp::loudness(&audio, 48000).unwrap()
        };
        let (model, at) = (changes(3.0), loudness(3.0));
        let render = [loudness(4.0) - at, loudness(2.0) - at, loudness(6.0) - at];
        println!("model {model:?}, render {render:?}");
        assert!(model[0] < 0.9, "{model:?}");
        for (a, b) in model.iter().zip(&render) {
            assert!((a - b).abs() < 0.005, "{model:?} {render:?}");
        }
    }
}
