//! Source measurement and the diagnostic preservation constraints of the set path.
use crate::{
    analysis::{self, Preservation, Temporal},
    audio_io::{Result, Stereo},
    dsp::LimiterStats,
    metrics, spectrum, stem_evidence, stereo, waveform, Metrics, Spectrum,
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

/// The metrics of `input`, after the checks of `analyze_source`.
fn checked_metrics(input: &[Stereo], rate: u32) -> Result<Metrics> {
    crate::audio_io::check_render_rate(rate)?;
    if input.len() < 3 * rate as usize || input.iter().flatten().any(|v| !v.is_finite()) {
        return Err("At least three seconds of finite stereo audio are required".into());
    }
    let input_metrics = metrics(input, rate);
    input_metrics
        .integrated_lufs
        .ok_or("Input is below the loudness gate")?;
    Ok(input_metrics)
}

/// The metrics and the stereo image of `input` at `rate`, as
/// `analyze_source` measures them: the parts that the finalizer reads.
pub fn measure_mix(input: &[Stereo], rate: u32) -> Result<(Metrics, stereo::StereoAnalysis)> {
    let input_metrics = checked_metrics(input, rate)?;
    Ok((input_metrics, stereo::analyze(input, rate)?))
}

/// The measurements of `input`, stereo audio at the render rate `rate`.
pub fn analyze_source(input: &[Stereo], rate: u32) -> Result<SourceAnalysis> {
    let input_metrics = checked_metrics(input, rate)?;
    Ok(SourceAnalysis {
        sample_rate: rate,
        source_pcm_sha256: stem_evidence::fingerprint(input),
        source_frames: input.len(),
        input: input_metrics,
        spectrum: spectrum(input, rate),
        temporal: analysis::analyze(input, rate),
        stereo: stereo::analyze(input, rate)?,
        waveform: waveform::analyze(input, rate),
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
