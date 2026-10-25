//! How the recording was captured, measured from the audio
//! (`set.source-condition.v4`). The set and the cloud manifest report the
//! condition; phase alignment (`stereo_phase`, D143), the tone stage (D146)
//! and the capture stages (`capture`, D200, D201) read it.
use crate::{audio_io::Stereo, pipeline::SourceAnalysis};
use rustfft::{num_complex::Complex, FftPlanner};
use serde::Serialize;

pub const RULE: &str = "set.source-condition.v4";
/// The DC offset of a channel reads this level when the channel's mean is
/// smaller, in dBFS.
pub const DC_FLOOR_DBFS: f64 = -200.0;
/// A fall of this many dB within a sixth of an octave above 8 kHz is the
/// low-pass of a lossy codec. The threshold lies between the roll-off of a
/// full-band mix and the steeper fall of a phone or a codec low-pass.
pub const CUTOFF_FALL_DB: f64 = 30.0;
/// A source arrives quiet at this integrated loudness or less, in LUFS. A
/// device recording that carries only a codec low-pass arrives below this
/// level; a studio mix and its AAC or MP3 copy arrive above it. A file at the
/// −23 LUFS of EBU R 128 is not quiet.
pub const QUIET_LUFS: f64 = -24.0;
/// The cutoff search covers 8 to 20 kHz, the top of the audible band. A
/// sample rate of 44.1 kHz ends at 22 kHz and does not count.
const CUTOFF_SEARCH_HZ: (f64, f64) = (8000.0, 20000.0);
/// A band is anti-phase when the phase between the channels is beyond this
/// angle, with this phase resultant or more, and the whole-file correlation
/// is this value or less: the band cancels in mono, and the offset holds
/// over the recording. A mix with an intact stereo image keeps a
/// correlation well above this value in each band.
pub const ANTIPHASE_DEGREES: f64 = 90.0;
pub const ANTIPHASE_RESULTANT: f64 = 0.5;
pub const ANTIPHASE_CORRELATION: f64 = -0.5;
/// A stem is anti-phase in a band when this share or more of its active
/// 0.5 s windows have a negative correlation, its whole-file phase is
/// beyond `STEM_ANTIPHASE_DEGREES` with a resultant of
/// `STEM_ANTIPHASE_RESULTANT` or more, and every other stem that is active
/// in the band is in phase (within `STEM_INPHASE_DEGREES`). The window share
/// marks the stem because an in-phase section of the stem keeps its
/// whole-file correlation above `ANTIPHASE_CORRELATION`.
pub const STEM_ANTIPHASE_SHARE: f64 = 0.5;
pub const STEM_ANTIPHASE_DEGREES: f64 = 135.0;
pub const STEM_ANTIPHASE_RESULTANT: f64 = 0.3;
pub const STEM_INPHASE_DEGREES: f64 = 45.0;
/// A stem counts in a band when it is active for this share of the
/// recording's active windows or more.
pub const STEM_ACTIVE_SHARE: f64 = 0.1;
const FFT: usize = 8192;

#[derive(Clone, Serialize, Debug)]
pub struct SourceCondition {
    pub rule: &'static str,
    /// The channels carry no side signal.
    pub mono: bool,
    /// The frequency of a codec low-pass, in Hz.
    pub bandwidth_hz: Option<f64>,
    /// The source arrives limited (D130).
    pub input_limited: bool,
    /// The source arrives at `QUIET_LUFS` or less (D200, D201).
    pub input_quiet: bool,
    /// Stereo analysis bands (`stereo::BAND_EDGES_HZ`) that are anti-phase.
    pub antiphase_bands: Vec<usize>,
    /// Stems whose channels cancel while the other stems are in phase: an
    /// inverted or wide instrument group from the mix stage. The engine
    /// reports it and keeps the width of the mix (D132).
    pub antiphase_stems: Vec<AntiphaseStem>,
    /// The DC offset of the left and the right channel: the mean of the
    /// channel over the recording against full scale, in dBFS, at least
    /// `DC_FLOOR_DBFS`. The engine reports it and changes no processing for
    /// it (D142, D187).
    pub dc_offset_dbfs: [f64; 2],
}

/// The mean of each channel of `input` against full scale, in dBFS, at
/// least `DC_FLOOR_DBFS`.
pub fn dc_offset_dbfs(input: &[Stereo]) -> [f64; 2] {
    let n = input.len().max(1) as f64;
    std::array::from_fn(|c| {
        let mean = input.iter().map(|f| f[c] as f64).sum::<f64>() / n;
        (20.0 * mean.abs().max(1e-300).log10()).max(DC_FLOOR_DBFS)
    })
}

/// One instrument group whose channels cancel (`antiphase_stems`).
#[derive(Clone, Serialize, Debug, PartialEq)]
pub struct AntiphaseStem {
    pub role: &'static str,
    /// Stereo analysis bands in which the stem is anti-phase.
    pub bands: Vec<usize>,
    /// Start of the first and end of the last negative window over those
    /// bands, in seconds.
    pub from_seconds: f64,
    pub to_seconds: f64,
    /// Largest share of negative windows over the bands.
    pub negative_share: f64,
    /// Whole-file cross-spectrum phase of the band with the largest share.
    pub cross_phase_degrees: f64,
}

impl SourceCondition {
    /// A device captured the recording: it is mono, carries a codec
    /// low-pass, or has an anti-phase band. Its balance and level come from
    /// the device, not from a person who listened at that level. The tone
    /// stage reads it (D146); the capture stages read `device_capture`.
    pub fn capture(&self) -> bool {
        self.mono || self.bandwidth_hz.is_some() || !self.antiphase_bands.is_empty()
    }

    /// The capture stages apply (D200, D201): the recording is mono, has an
    /// anti-phase band, or carries a codec low-pass and arrives quiet
    /// (`input_quiet`) and unlimited. An MP3 or AAC copy of a studio mix
    /// carries the low-pass at the level of the mix, so it keeps the bounds
    /// of a mix.
    pub fn device_capture(&self) -> bool {
        device_capture(
            self.mono,
            !self.antiphase_bands.is_empty(),
            self.bandwidth_hz.is_some(),
            self.input_quiet,
            self.input_limited,
        )
    }
}

fn device_capture(mono: bool, antiphase: bool, low_pass: bool, quiet: bool, limited: bool) -> bool {
    mono || antiphase || (low_pass && quiet && !limited)
}

/// `SourceCondition::device_capture` of a reported condition, as a worker
/// plan carries it (`set::WorkerPlan::source_condition`).
pub fn reported_device_capture(report: &serde_json::Value) -> bool {
    let flag = |key: &str| report[key].as_bool().unwrap_or(false);
    device_capture(
        flag("mono"),
        report["antiphase_bands"]
            .as_array()
            .is_some_and(|b| !b.is_empty()),
        report["bandwidth_hz"].as_f64().is_some(),
        flag("input_quiet"),
        flag("input_limited"),
    )
}

/// The stems whose channels cancel in a band while another stem active in
/// that band is in phase, from the stem images of the evidence. A band in
/// `mix_antiphase` (the anti-phase bands of the mix, D142) is left out: the
/// whole mix cancels there, and phase alignment (`stereo_phase`, D143)
/// answers it. In a
/// live recording whose mix is anti-phase in the high bands, one stem alone
/// can be active in the top band and would otherwise be reported.
pub fn antiphase_stems(
    images: &[crate::stem_evidence::StemImage],
    mix_antiphase: &[usize],
) -> Vec<AntiphaseStem> {
    let Some(bands) = images.first().map(|i| i.bands.len()) else {
        return Vec::new();
    };
    let longest: Vec<f64> = (0..bands)
        .map(|b| {
            images
                .iter()
                .map(|i| i.bands[b].active_seconds)
                .fold(0.0, f64::max)
        })
        .collect();
    let active = |band: &crate::stem_evidence::StemImageBand, b: usize| {
        band.active_seconds >= STEM_ACTIVE_SHARE * longest[b] && band.active_seconds > 0.0
    };
    let share = |band: &crate::stem_evidence::StemImageBand| {
        if band.active_seconds > 0.0 {
            band.negative_seconds / band.active_seconds
        } else {
            0.0
        }
    };
    let anti = |band: &crate::stem_evidence::StemImageBand, b: usize| {
        active(band, b)
            && share(band) >= STEM_ANTIPHASE_SHARE
            && band
                .cross_phase_degrees
                .is_some_and(|p| p.abs() >= STEM_ANTIPHASE_DEGREES)
            && band
                .phase_resultant
                .is_some_and(|r| r >= STEM_ANTIPHASE_RESULTANT)
    };
    let in_phase = |band: &crate::stem_evidence::StemImageBand, b: usize| {
        !active(band, b)
            || band
                .cross_phase_degrees
                .is_none_or(|p| p.abs() < STEM_INPHASE_DEGREES)
    };
    images
        .iter()
        .enumerate()
        .filter_map(|(s, image)| {
            let found: Vec<usize> = (0..bands)
                .filter(|&b| {
                    !mix_antiphase.contains(&b)
                        && anti(&image.bands[b], b)
                        && images
                            .iter()
                            .enumerate()
                            .any(|(o, other)| o != s && active(&other.bands[b], b))
                        && images
                            .iter()
                            .enumerate()
                            .all(|(o, other)| o == s || in_phase(&other.bands[b], b))
                })
                .collect();
            let largest = *found
                .iter()
                .max_by(|&&x, &&y| share(&image.bands[x]).total_cmp(&share(&image.bands[y])))?;
            Some(AntiphaseStem {
                role: image.role,
                from_seconds: found
                    .iter()
                    .filter_map(|&b| image.bands[b].negative_from_seconds)
                    .fold(f64::INFINITY, f64::min),
                to_seconds: found
                    .iter()
                    .filter_map(|&b| image.bands[b].negative_to_seconds)
                    .fold(0.0, f64::max),
                negative_share: share(&image.bands[largest]),
                cross_phase_degrees: image.bands[largest].cross_phase_degrees.unwrap_or(0.0),
                bands: found,
            })
        })
        .collect()
}

pub fn measure(
    input: &[Stereo],
    source: &SourceAnalysis,
    input_limited: bool,
    evidence: Option<&crate::stem_evidence::Evidence>,
) -> SourceCondition {
    let bands = &source.stereo.whole.bands;
    let antiphase_bands: Vec<usize> = bands
        .iter()
        .enumerate()
        .filter(|(_, b)| {
            b.correlation.is_some_and(|c| c <= ANTIPHASE_CORRELATION)
                && b.phase_resultant.is_some_and(|r| r >= ANTIPHASE_RESULTANT)
                && b.cross_phase_degrees
                    .is_some_and(|p| p.abs() >= ANTIPHASE_DEGREES)
        })
        .map(|(i, _)| i)
        .collect();
    SourceCondition {
        rule: RULE,
        mono: !bands.iter().any(|b| b.side_present),
        bandwidth_hz: bandwidth_hz(input, source.sample_rate),
        input_limited,
        input_quiet: source
            .input
            .integrated_lufs
            .is_none_or(|lufs| lufs <= QUIET_LUFS),
        antiphase_stems: evidence.filter(|e| e.usable).map_or_else(Vec::new, |e| {
            antiphase_stems(&e.stem_images, &antiphase_bands)
        }),
        antiphase_bands,
        dc_offset_dbfs: dc_offset_dbfs(input),
    }
}

/// The codec low-pass of `input`: the centre of the largest fall of the mono
/// spectrum within a sixth of an octave from 8 to 20 kHz, when the fall is
/// `CUTOFF_FALL_DB` or more. `input` is at `rate`.
pub fn bandwidth_hz(input: &[Stereo], rate: u32) -> Option<f64> {
    let rate = rate as f64;
    let fft = FftPlanner::new().plan_fft_forward(FFT);
    let window: Vec<f64> = (0..FFT)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / FFT as f64).cos())
        .collect();
    let mut power = vec![0.0f64; FFT / 2 + 1];
    let mut buffer = vec![Complex::new(0.0, 0.0); FFT];
    let mut scratch = vec![Complex::new(0.0, 0.0); fft.get_inplace_scratch_len()];
    // Every third block: the long-term spectrum at a third of the work.
    for start in (0..input.len().saturating_sub(FFT)).step_by(3 * FFT) {
        for (k, b) in buffer.iter_mut().enumerate() {
            let f = input[start + k];
            *b = Complex::new(0.5 * (f[0] as f64 + f[1] as f64) * window[k], 0.0);
        }
        fft.process_with_scratch(&mut buffer, &mut scratch);
        for (p, b) in power.iter_mut().zip(&buffer) {
            *p += b.norm_sqr();
        }
    }
    let level = |hz: f64| {
        let (low, high) = (hz * 2f64.powf(-1.0 / 24.0), hz * 2f64.powf(1.0 / 24.0));
        let bins: Vec<f64> = (0..power.len())
            .filter(|&k| {
                let f = k as f64 * rate / FFT as f64;
                f >= low && f < high
            })
            .map(|k| power[k])
            .collect();
        10.0 * (bins.iter().sum::<f64>() / bins.len().max(1) as f64)
            .max(1e-30)
            .log10()
    };
    let (low, high) = CUTOFF_SEARCH_HZ;
    let top = high * 2f64.powf(-1.0 / 6.0);
    let steps = 60;
    (0..steps)
        .map(|i| low * (top / low).powf(i as f64 / (steps - 1) as f64))
        .map(|hz| (level(hz) - level(hz * 2f64.powf(1.0 / 6.0)), hz))
        .filter(|(fall, _)| *fall >= CUTOFF_FALL_DB)
        .max_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, hz)| hz * 2f64.powf(1.0 / 12.0))
}

#[cfg(test)]
mod device_capture_tests {
    use super::*;

    fn condition(
        mono: bool,
        bandwidth_hz: Option<f64>,
        quiet: bool,
        limited: bool,
    ) -> SourceCondition {
        SourceCondition {
            rule: RULE,
            mono,
            bandwidth_hz,
            input_limited: limited,
            input_quiet: quiet,
            antiphase_bands: Vec::new(),
            antiphase_stems: Vec::new(),
            dc_offset_dbfs: [DC_FLOOR_DBFS; 2],
        }
    }

    #[test]
    fn a_lossy_copy_of_a_mix_is_a_capture_for_the_tone_stage_only() {
        // An MP3 of a studio mix: a codec low-pass at the level of the mix.
        let lossy_mix = condition(false, Some(17639.0), false, false);
        assert!(lossy_mix.capture());
        assert!(!lossy_mix.device_capture());
        // A quiet and unlimited recording with a codec low-pass: a phone.
        assert!(condition(false, Some(17639.0), true, false).device_capture());
        // A quiet master keeps its limiter, so it is a mix.
        assert!(!condition(false, Some(17639.0), true, true).device_capture());
        // A quiet full-band mix is no capture.
        assert!(!condition(false, None, true, false).device_capture());
    }

    #[test]
    fn a_mono_or_anti_phase_recording_is_a_device_capture_at_any_level() {
        assert!(condition(true, Some(16482.0), false, true).device_capture());
        assert!(condition(true, None, false, false).device_capture());
        let mut antiphase = condition(false, Some(18124.0), false, false);
        antiphase.antiphase_bands = vec![4, 5];
        assert!(antiphase.device_capture());
    }

    #[test]
    fn a_reported_condition_gives_the_device_capture_of_the_condition() {
        for (mono, bandwidth, quiet, limited) in [
            (false, Some(17639.0), false, false),
            (false, Some(17639.0), true, false),
            (false, Some(17639.0), true, true),
            (true, None, false, true),
            (false, None, true, false),
        ] {
            let measured = condition(mono, bandwidth, quiet, limited);
            let report = serde_json::to_value(&measured).unwrap();
            assert_eq!(reported_device_capture(&report), measured.device_capture());
        }
    }
}

#[cfg(test)]
mod antiphase_stem_tests {
    use super::*;
    use crate::stem_evidence::{StemImage, StemImageBand};

    fn band(phase: f64, negative_share: f64, active: f64) -> StemImageBand {
        StemImageBand {
            correlation: Some(if phase.abs() > 90.0 { -0.3 } else { 0.8 }),
            cross_phase_degrees: Some(phase),
            phase_resultant: Some(0.5),
            active_seconds: active,
            negative_seconds: negative_share * active,
            longest_negative_run_seconds: 10.0,
            negative_from_seconds: (negative_share > 0.0).then_some(45.0),
            negative_to_seconds: (negative_share > 0.0).then_some(172.0),
        }
    }

    fn stem(role: &'static str, phases: [f64; 6], share: f64, active: f64) -> StemImage {
        StemImage {
            role,
            bands: phases.iter().map(|p| band(*p, share, active)).collect(),
        }
    }

    #[test]
    fn one_inverted_stem_among_in_phase_stems_is_reported() {
        let images = vec![
            stem("drums", [0.0; 6], 0.0, 170.0),
            stem("bass", [0.0; 6], 0.0, 170.0),
            stem("other", [0.0, 0.0, -176.0, -179.0, 10.0, 5.0], 0.75, 170.0),
            stem("vocals", [0.0, 0.0, -10.0, 14.0, 0.0, 0.0], 0.1, 120.0),
        ];
        let found = antiphase_stems(&images, &[]);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].role, "other");
        assert_eq!(found[0].bands, vec![2, 3]);
        assert_eq!(found[0].from_seconds, 45.0);
        assert_eq!(found[0].to_seconds, 172.0);
        assert!((found[0].negative_share - 0.75).abs() < 1e-9);
    }

    #[test]
    fn a_whole_mix_out_of_phase_is_not_one_stem() {
        // Every stem anti-phase: a bus fault, which `antiphase_bands` reports
        // on the mix (D142).
        let images = vec![
            stem("drums", [-178.0; 6], 0.9, 170.0),
            stem("bass", [-178.0; 6], 0.9, 170.0),
            stem("other", [-178.0; 6], 0.9, 170.0),
            stem("vocals", [-178.0; 6], 0.9, 170.0),
        ];
        assert!(antiphase_stems(&images, &[]).is_empty());
    }

    #[test]
    fn a_band_the_mix_cancels_in_or_with_no_other_stem_is_not_reported() {
        // The mix is anti-phase in bands 4 and 5 (D142): the alignment
        // answers it, not a stem report.
        let images = vec![
            stem("drums", [0.0, 0.0, 0.0, 0.0, 170.0, 169.0], 1.0, 170.0),
            stem("bass", [0.0; 6], 0.0, 170.0),
        ];
        assert!(antiphase_stems(&images, &[4, 5]).is_empty());
        // No other stem is active in the band: nothing to be out of phase
        // with.
        let mut lone = stem("drums", [0.0, 0.0, 0.0, 0.0, 0.0, 169.0], 1.0, 170.0);
        lone.bands[5].active_seconds = 170.0;
        let mut quiet = stem("bass", [0.0; 6], 0.0, 170.0);
        quiet.bands[5].active_seconds = 0.0;
        assert!(antiphase_stems(&[lone, quiet], &[]).is_empty());
    }

    #[test]
    fn a_short_or_mild_share_is_not_reported() {
        let images = vec![
            stem("drums", [0.0; 6], 0.0, 170.0),
            stem("other", [0.0, 0.0, -176.0, -179.0, 0.0, 0.0], 0.3, 170.0),
        ];
        assert!(antiphase_stems(&images, &[]).is_empty());
        // A quiet stem that is barely active in the band does not count.
        let images = vec![
            stem("drums", [0.0; 6], 0.0, 170.0),
            stem("other", [0.0, 0.0, -176.0, -179.0, 0.0, 0.0], 0.9, 5.0),
        ];
        assert!(antiphase_stems(&images, &[]).is_empty());
    }
}
