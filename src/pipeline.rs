//! Source measurement and the diagnostic preservation constraints of the set path.
use crate::{
    analysis::{self, Preservation, Temporal},
    audio_io::{Result, Stereo},
    dsp::LimiterStats,
    stem_evidence, stereo, waveform, Metrics, Spectrum,
};
use serde::Serialize;

/// Measurements of one exact stereo input at the render rate.
#[derive(Clone, Serialize)]
pub struct SourceAnalysis {
    /// The render rate of the input, in Hz.
    pub sample_rate: u32,
    pub source_pcm_sha256: String,
    pub source_frames: usize,
    pub input: Metrics,
    pub spectrum: Spectrum,
    pub temporal: Temporal,
    pub stereo: stereo::StereoAnalysis,
    pub waveform: waveform::Waveform,
    /// The level of each 3 s window of the input, at 1 s steps: the source
    /// side of the gain span of each render (`selection::measure`).
    #[serde(skip)]
    pub window_levels_db: Vec<f64>,
}

/// The measurements of a mix that the duplicate screen, the descriptions
/// and the names compare a version with: its metrics and its stereo image.
#[derive(Clone, Copy)]
pub struct Mix<'a> {
    pub input: &'a Metrics,
    pub stereo: &'a stereo::StereoAnalysis,
}

impl SourceAnalysis {
    /// The metrics and the stereo image of the source.
    pub fn mix(&self) -> Mix<'_> {
        Mix {
            input: &self.input,
            stereo: &self.stereo,
        }
    }
}

/// The metrics and the stereo image of `input` at `rate`, as
/// `analyze_source` measures them: the parts that the finalizer reads.
pub fn measure_mix(input: &[Stereo], rate: u32) -> Result<(Metrics, stereo::StereoAnalysis)> {
    measure_mix_read(&mut crate::scratch::Slice(input), rate)
}

/// `measure_mix` of the frames of `input`. One pass over its blocks gives
/// the finite check and the metrics, and the stereo analysis reads the
/// frames again in batches (D255).
pub(crate) fn measure_mix_read(
    input: &mut impl crate::scratch::Frames,
    rate: u32,
) -> Result<(Metrics, stereo::StereoAnalysis)> {
    const SHORT: &str = "At least three seconds of finite stereo audio are required";
    crate::audio_io::check_render_rate(rate)?;
    if input.len() < 3 * rate as usize {
        return Err(SHORT.into());
    }
    let mut scan = crate::MetricsScan::new(rate);
    let mut finite = true;
    crate::scratch::for_each_block(input, |block| {
        finite &= block.iter().flatten().all(|v| v.is_finite());
        scan.push(block);
        Ok(())
    })?;
    if !finite {
        return Err(SHORT.into());
    }
    let input_metrics = scan.finish();
    input_metrics
        .integrated_lufs
        .ok_or("Input is below the loudness gate")?;
    Ok((input_metrics, stereo::analyze_read(input, rate)?))
}

/// The measurements of `input`, stereo audio at the render rate `rate`.
pub fn analyze_source(input: &[Stereo], rate: u32) -> Result<SourceAnalysis> {
    analyze_source_read(&mut crate::scratch::Slice(input), rate)
}

/// Frames of each block of `analyze_source_read`.
const SOURCE_BLOCK: usize = 65_536;

/// `analyze_source` of the frames of `input`. One pass over blocks of
/// `SOURCE_BLOCK` frames gives the fingerprint, the metrics, the temporal
/// analysis and the waveform. The stereo analysis with the spectrum and the
/// window levels each read the frames again in spans.
pub(crate) fn analyze_source_read(
    input: &mut impl crate::scratch::Frames,
    rate: u32,
) -> Result<SourceAnalysis> {
    const SHORT: &str = "At least three seconds of finite stereo audio are required";
    crate::audio_io::check_render_rate(rate)?;
    let frames = input.len();
    if frames < 3 * rate as usize {
        return Err(SHORT.into());
    }
    let mut hash = stem_evidence::Fingerprint::default();
    let mut metrics = crate::MetricsScan::new(rate);
    let mut temporal = analysis::Scan::new(frames, rate);
    let mut envelopes = waveform::Envelopes::new(frames, rate);
    let mut buffer = Vec::new();
    for start in (0..frames).step_by(SOURCE_BLOCK) {
        let block = input.window(start, SOURCE_BLOCK.min(frames - start), &mut buffer)?;
        if block.iter().flatten().any(|v| !v.is_finite()) {
            return Err(SHORT.into());
        }
        hash.take(block);
        metrics.push(block);
        temporal.push(block);
        envelopes.push(block);
    }
    let input_metrics = metrics.finish();
    input_metrics
        .integrated_lufs
        .ok_or("Input is below the loudness gate")?;
    let (stereo, spectrum) = stereo::analyze_with_spectrum_read(input, rate)?;
    Ok(SourceAnalysis {
        sample_rate: rate,
        source_pcm_sha256: hash.finish(),
        source_frames: frames,
        input: input_metrics,
        spectrum,
        temporal: temporal.finish(),
        stereo,
        waveform: envelopes.finish(),
        window_levels_db: crate::selection::window_levels_read(input, rate)?,
    })
}

#[derive(Clone, Serialize)]
pub struct Constraints {
    pub maximum_reduction_db: f64,
    pub mean_reduction_db: f64,
    pub crest_loss_p90_db: f64,
    pub attack_loss_p90_db: f64,
    pub window_contrast_loss_db: f64,
    pub window_level_change_span_db: f64,
    pub frames_above_one_db_percent: f64,
}

/// Diagnostic limits for punch 0.7 and intensity 0.8. The set reports these
/// limits; they do not select a version.
pub fn diagnostic_constraints() -> Constraints {
    let punch: f64 = 0.7;
    let intensity: f64 = 0.8;
    Constraints {
        maximum_reduction_db: 12.0 * intensity * (1.0 - 0.2 * punch),
        mean_reduction_db: 4.5 - 4.0 * punch,
        crest_loss_p90_db: 3.7 - 3.0 * punch,
        attack_loss_p90_db: 8.4 - 7.8 * punch,
        window_contrast_loss_db: 2.4 - 2.0 * punch,
        window_level_change_span_db: 2.9 - 2.2 * punch,
        frames_above_one_db_percent: (125.0 - 100.0 * punch).clamp(25.0, 95.0),
    }
}

pub fn violations(
    p: &Preservation,
    limiter: Option<&LimiterStats>,
    c: &Constraints,
) -> Vec<&'static str> {
    let mut reasons = Vec::new();
    if p.crest_loss_p90_db > c.crest_loss_p90_db {
        reasons.push("preserve.crest");
    }
    if p.event_count >= 8
        && p.attack_loss_p90_db
            .is_some_and(|v| v > c.attack_loss_p90_db)
    {
        reasons.push("preserve.attack");
    }
    if p.window_contrast_loss_db > c.window_contrast_loss_db {
        reasons.push("preserve.contrast");
    }
    if p.window_level_change_span_db > c.window_level_change_span_db {
        reasons.push("preserve.window_balance");
    }
    if let Some(limiter) = limiter {
        if limiter.maximum_reduction_db > c.maximum_reduction_db + 1e-4 {
            reasons.push("limit.maximum_reduction");
        }
        if limiter.mean_reduction_db > c.mean_reduction_db {
            reasons.push("limit.mean_reduction");
        }
        if limiter.frames_above_one_db_percent > c.frames_above_one_db_percent {
            reasons.push("limit.sustained_reduction");
        }
    }
    reasons
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A stored mix gives the source analysis and the condition of the mix
    /// in memory, with its window levels and its 10 ms blocks (D247).
    #[test]
    fn a_stored_mix_gives_the_measures_of_the_mix() {
        // 70 s and 311 frames: two spans of window levels, and a last block
        // shorter than `SOURCE_BLOCK`.
        let frames = 48_000 * 70 + 311;
        let mut state = 0x2545_f491u32;
        let audio: Vec<Stereo> = (0..frames)
            .map(|i| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                let noise = state as f32 / u32::MAX as f32 - 0.5;
                let level = [0.6, 0.1, 0.4][(i / 100_000) % 3];
                [level * noise, level * (0.5 * noise + 0.1)]
            })
            .collect();
        let whole = analyze_source(&audio, 48_000).unwrap();
        let mut stored = crate::scratch::Pcm::temporary_of(&audio).unwrap();
        let read = analyze_source_read(&mut stored, 48_000).unwrap();
        assert_eq!(
            serde_json::to_value(&read).unwrap(),
            serde_json::to_value(&whole).unwrap()
        );
        assert_eq!(read.window_levels_db.len(), 68);
        assert_eq!(read.window_levels_db, whole.window_levels_db);
        let blocks = |s: &SourceAnalysis| -> Vec<[f64; 2]> {
            s.temporal
                .blocks
                .iter()
                .map(|b| [b.energy, b.peak])
                .collect()
        };
        assert_eq!(blocks(&read), blocks(&whole));
        let condition =
            crate::source_condition::measure_read(&mut stored, &read, false, None).unwrap();
        assert_eq!(
            serde_json::to_value(condition).unwrap(),
            serde_json::to_value(crate::source_condition::measure(
                &audio, &whole, false, None
            ))
            .unwrap()
        );
    }
}
