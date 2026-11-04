//! The stages of a prefix on a recording that arrives in spans, in order
//! from frame 0 (`Renderer::prepare`). Each stage keeps its state from one
//! span to the next: the FIR stages their FFT blocks, the filters their
//! biquad state. The output of a chain equals the output of its stages on
//! the whole recording, and the chain holds no whole signal.
use crate::{
    audio_io::{Result, Stereo},
    deesser, dsp, fir, stereo_phase, treatment, width,
};
use std::collections::VecDeque;

/// How a FIR stage forms the inputs of its kernels and its output frame.
enum Filter {
    /// The phase alignment: one kernel for each channel.
    Channels,
    /// A side gain, and the mono-safe cut that blends it in by its
    /// envelope.
    Side {
        kernels: width::SideKernels,
        cut: Option<treatment::EnvelopedBand>,
    },
    /// The de-esser cut of the sibilant band of the mid.
    Band(deesser::BandCut),
}

impl Filter {
    fn inputs(&self, frame: Stereo, inputs: &mut [f64]) {
        match self {
            Self::Channels => {
                inputs[0] = frame[0] as f64;
                inputs[1] = frame[1] as f64;
            }
            Self::Side { kernels, .. } => kernels.inputs(frame, inputs),
            Self::Band(_) => inputs[0] = deesser::mid_of(frame),
        }
    }

    /// Frame `i` after the stage, from the frame before it and the output
    /// of each kernel at it.
    fn after(&self, i: usize, before: Stereo, outputs: &[f64]) -> Stereo {
        match self {
            Self::Channels => [outputs[0] as f32, outputs[1] as f32],
            Self::Side { kernels, cut } => {
                let wet = kernels.after(before, outputs);
                match cut {
                    Some(cut) => crate::treatment_render::mono_safe_blend(cut, i, before, wet),
                    None => wet,
                }
            }
            Self::Band(band) => band.after(i, before, outputs[0]).unwrap_or(before),
        }
    }
}

struct FirStage {
    filter: Filter,
    stream: fir::Stream,
    /// The input frames that wait for their outputs, in order.
    waiting: VecDeque<Stereo>,
    inputs: Vec<f64>,
    /// The buffer of the next output: the input buffer of the push before.
    spare: Vec<Stereo>,
}

impl FirStage {
    fn new(filter: Filter, stream: fir::Stream, kernels: usize) -> Self {
        Self {
            filter,
            waiting: VecDeque::with_capacity(stream.latency() + 1),
            stream,
            inputs: vec![0.0; kernels],
            spare: Vec::new(),
        }
    }

    fn push(&mut self, frames: &[Stereo], out: &mut Vec<Stereo>) {
        let Self {
            filter,
            stream,
            waiting,
            inputs,
            ..
        } = self;
        for &frame in frames {
            filter.inputs(frame, inputs);
            waiting.push_back(frame);
            stream.push(inputs, &mut |i, outputs| {
                let before = waiting.pop_front().expect("an input for each output");
                out.push(filter.after(i, before, outputs));
            });
        }
    }

    fn finish(&mut self, out: &mut Vec<Stereo>) {
        let Self {
            filter,
            stream,
            waiting,
            ..
        } = self;
        stream.finish(&mut |i, outputs| {
            let before = waiting.pop_front().expect("an input for each output");
            out.push(filter.after(i, before, outputs));
        });
    }
}

enum Stage {
    Fir(Box<FirStage>),
    Tone(treatment::ToneStream),
    SideEq(dsp::SideEq),
}

impl Stage {
    fn push(&mut self, mut frames: Vec<Stereo>) -> Result<Vec<Stereo>> {
        match self {
            Self::Fir(stage) => {
                // The buffers pass from stage to stage, and each span reuses
                // them: a new buffer for each span raises the memory that the
                // allocator keeps. Each buffer gets the capacity of the input
                // buffer of the chain, so that it does not grow at a later
                // stage.
                let mut out = std::mem::take(&mut stage.spare);
                out.clear();
                out.reserve(frames.capacity().max(frames.len() + stage.stream.latency()));
                stage.push(&frames, &mut out);
                stage.spare = frames;
                return Ok(out);
            }
            Self::Tone(tone) => tone.process(&mut frames)?,
            Self::SideEq(eq) => eq.process(&mut frames),
        }
        Ok(frames)
    }

    fn finish(&mut self, out: &mut Vec<Stereo>) {
        if let Self::Fir(stage) = self {
            stage.finish(out);
        }
    }
}

/// Stages in order on a recording of `frames` frames.
pub(crate) struct Chain {
    frames: usize,
    stages: Vec<Stage>,
}

impl Chain {
    pub(crate) fn new(frames: usize) -> Self {
        Self {
            frames,
            stages: Vec::new(),
        }
    }

    /// `stereo_phase::Alignment::apply`.
    pub(crate) fn alignment(&mut self, alignment: &stereo_phase::Alignment, rate: u32) {
        let [left, right] = alignment.kernels(rate);
        let stream = fir::Stream::new(self.frames, &[&left, &right]);
        self.fir(Filter::Channels, stream, 2);
    }

    /// The tone and the focus envelopes of `stream`.
    pub(crate) fn tone(&mut self, stream: treatment::ToneStream) {
        self.stages.push(Stage::Tone(stream));
    }

    /// `width::SideGain::apply`.
    pub(crate) fn side_gain(&mut self, gain: &width::SideGain, rate: u32) {
        self.side(gain, None, rate);
    }

    /// `treatment_render::apply_mono_safe` from frame 0.
    pub(crate) fn mono_safe(&mut self, cuts: &[treatment::EnvelopedBand], rate: u32) {
        for cut in cuts {
            let Some(gain) = crate::treatment_render::mono_safe_gain(cut) else {
                continue;
            };
            self.side(&gain, Some(cut.clone()), rate);
        }
    }

    /// The de-esser cut of `band` from frame 0.
    pub(crate) fn band_cut(&mut self, band: deesser::BandCut) {
        let stream = fir::Stream::new(self.frames, &[&band.kernel]);
        self.fir(Filter::Band(band), stream, 1);
    }

    /// `dsp::side_eq`.
    pub(crate) fn side_eq(&mut self, bands: &[dsp::EqBand], rate: u32) {
        self.stages
            .push(Stage::SideEq(dsp::SideEq::new(bands, rate)));
    }

    fn side(&mut self, gain: &width::SideGain, cut: Option<treatment::EnvelopedBand>, rate: u32) {
        let kernels = gain.kernels(rate);
        let all = kernels.all();
        let (stream, count) = (fir::Stream::new(self.frames, &all), all.len());
        self.fir(Filter::Side { kernels, cut }, stream, count);
    }

    fn fir(&mut self, filter: Filter, stream: fir::Stream, kernels: usize) {
        let stage = FirStage::new(filter, stream, kernels);
        self.stages.push(Stage::Fir(Box::new(stage)));
    }

    /// The frames of a buffer that holds a span of `span` frames at each
    /// stage: each FIR stage gives at most its latency more frames than it
    /// takes. A buffer with this capacity passes through the chain and does
    /// not grow.
    pub(crate) fn capacity(&self, span: usize) -> usize {
        span + self
            .stages
            .iter()
            .map(|stage| match stage {
                Stage::Fir(stage) => stage.stream.latency(),
                _ => 0,
            })
            .sum::<usize>()
    }

    /// The output of the chain that the next frames of the recording
    /// complete.
    pub(crate) fn push(&mut self, mut frames: Vec<Stereo>) -> Result<Vec<Stereo>> {
        for stage in &mut self.stages {
            frames = stage.push(frames)?;
        }
        Ok(frames)
    }

    /// The rest of the output, after the last frame of the recording.
    pub(crate) fn finish(&mut self) -> Result<Vec<Stereo>> {
        let mut frames = Vec::new();
        for stage in &mut self.stages {
            frames = stage.push(frames)?;
            stage.finish(&mut frames);
        }
        Ok(frames)
    }
}
