//! Audibility point of peak control: the loudness at which the masked
//! disturbance of the limiter and clipper reaches the masking threshold.
//!
//! The disturbance is the output minus the prefix times a gain fitted per
//! 0.34 s restoration window, so the limiter gain inside a passage stays in
//! it with the clipping. Its audibility is the PEAQ total noise-to-mask ratio
//! under the model of `masking`. A total ratio of 0 dB is the point where the
//! disturbance, averaged over the recording, equals the masking threshold.
//!
//! The attack loss against the mix holds the point under that loudness
//! (`set.audible-loudness.v6`): the gain fit removes the limiter gain riding
//! from the disturbance, and the attack loss measures it. The ratio of the
//! loudest frames is reported with each probe. The average lets a loud
//! chorus over the threshold balance quiet verses under it.
use crate::{
    audio_io::{Result, Stereo},
    dsp::{self, LimiterSettings, TruePeak},
    masking::{self, Channels, BANDS},
    render_blocks, scratch,
};
use serde::Serialize;

/// Samples per gain fit of the disturbance: one restoration window.
pub const GAIN_BLOCK: usize = 4 * 4096;
/// Frames more than this far under the loudest frame are left out, in dB.
pub const QUIET_DB: f64 = 50.0;
/// Total noise-to-mask ratio of the audibility point, in dB.
pub const AUDIBLE_NMR_DB: f64 = 0.0;
/// Share of the frames whose ratio `loud_nmr_db` reads: the loudest 5%.
pub const LOUD_SHARE: f64 = 0.05;
/// The threshold of a disturbed frame of PEAQ (ITU-R BS.1387), in dB. The
/// report compares `Probe::loud_nmr_db` with it; the point does not.
pub const LOUD_NMR_DB: f64 = 1.5;
/// Largest P90 attack loss of the point against the mix, in dB: that of
/// the hottest master of the reference mix/master pairs (D208). The
/// reference album masters lose 2.68 to 4.93 dB (D177).
pub const ATTACK_LOSS_MAX_DB: f64 = 5.45;
/// dB of noise-to-mask ratio for each dB of attack loss in the search: the
/// ratio rises 4 to 5 dB per LU and the attack loss about 0.45 dB per LU
/// (D179),
/// so the tolerance of the search holds for both.
const ATTACK_SCALE: f64 = 10.0;
/// First drive step of the search, in dB.
const FIRST_STEP_DB: f64 = 1.0;
/// Largest drive step of the search, in dB.
const LARGEST_STEP_DB: f64 = 4.0;
/// The search stops when a probe is this close to the audibility point, in
/// dB of noise-to-mask ratio. The ratio rises 4 to 5 dB per LU, so 0.25 dB
/// is about 0.05 LU.
const TOLERANCE_DB: f64 = 0.25;
/// Largest number of probes.
const PROBES: usize = 8;

/// `output` minus `prefix` times a gain fitted by least squares per block of
/// `block` samples and interpolated between block centres. The length is a
/// whole number of blocks.
pub fn gain_residual(prefix: &[Stereo], output: &[Stereo], block: usize) -> Vec<Stereo> {
    let fit = GainFit::of(prefix, output, block);
    (0..fit.len())
        .map(|i| fit.residual(prefix, output, i))
        .collect()
}

/// The gains of `gain_residual`: one least-squares gain of the prefix in the
/// output per block.
struct GainFit {
    gains: Vec<f64>,
    block: usize,
}

impl GainFit {
    fn of(prefix: &[Stereo], output: &[Stereo], block: usize) -> Self {
        let blocks = prefix.len().min(output.len()) / block;
        let gains = (0..blocks)
            .map(|b| {
                let range = b * block..(b + 1) * block;
                let (mut xy, mut xx) = (0.0f64, 0.0f64);
                for (x, y) in prefix[range.clone()].iter().zip(&output[range]) {
                    for c in 0..2 {
                        xy += x[c] as f64 * y[c] as f64;
                        xx += (x[c] as f64).powi(2);
                    }
                }
                if xx > 0.0 {
                    xy / xx
                } else {
                    0.0
                }
            })
            .collect();
        Self { gains, block }
    }

    /// Samples of the residual: a whole number of blocks.
    fn len(&self) -> usize {
        self.gains.len() * self.block
    }

    /// Sample `i` of the residual of `output` against `prefix`, with the gain
    /// interpolated between the block centres.
    #[inline]
    fn residual(&self, prefix: &[Stereo], output: &[Stereo], i: usize) -> Stereo {
        let (gains, block) = (&self.gains, self.block);
        let blocks = gains.len();
        let centre = |b: usize| (b * block + block / 2) as f64;
        let position = (i as f64 - block as f64 / 2.0) / block as f64;
        let b = (position.floor().max(0.0) as usize).min(blocks.saturating_sub(2));
        let t = ((i as f64 - centre(b)) / block as f64).clamp(0.0, 1.0);
        let g = gains[b] + (gains[(b + 1).min(blocks - 1)] - gains[b]) * t;
        std::array::from_fn(|c| (output[i][c] as f64 - g * prefix[i][c] as f64) as f32)
    }
}

/// PEAQ total noise-to-mask ratio of `disturbance` in `reference`, in dB,
/// over the frames within `QUIET_DB` of the loudest frame. Both are audio at
/// `rate`.
pub fn total_nmr_db(reference: &[Stereo], disturbance: &[Stereo], rate: u32) -> f64 {
    let n = reference.len().min(disturbance.len());
    let masker = masking::frames(&reference[..n], Channels::Both, rate);
    let noise = masking::frames(&disturbance[..n], Channels::Both, rate);
    nmr_of(&masker, &noise, rate).total_db
}

/// The noise-to-mask ratios of one disturbance, in dB.
#[derive(Clone, Copy, Debug)]
pub struct Ratios {
    /// The total ratio: the mean over the frames.
    pub total_db: f64,
    /// The ratio that the loudest `LOUD_SHARE` of the frames reach or pass.
    pub loud_db: f64,
}

/// The noise-to-mask ratios of the frames `noise` under the frames `masker`
/// of audio at `rate`.
fn nmr_of(masker: &masking::Frames, noise: &masking::Frames, rate: u32) -> Ratios {
    let thresholds = masking::thresholds(masker, Channels::Both, rate);
    let loudest = masker
        .powers
        .iter()
        .map(|p| p.iter().sum::<f64>())
        .fold(0.0, f64::max);
    let ratios: Vec<f64> = masker
        .powers
        .iter()
        .zip(&noise.powers)
        .zip(&thresholds)
        .filter(|((p, _), _)| p.iter().sum::<f64>() >= loudest * 10f64.powf(-QUIET_DB / 10.0))
        .map(|((_, q), threshold)| {
            (0..BANDS)
                .map(|z| q[z] / threshold[z].max(1e-30))
                .sum::<f64>()
                / BANDS as f64
        })
        .collect();
    let total = ratios.iter().sum::<f64>() / ratios.len().max(1) as f64;
    let mut sorted = ratios;
    sorted.sort_by(f64::total_cmp);
    let loud = sorted
        .get(((1.0 - LOUD_SHARE) * sorted.len() as f64) as usize)
        .or(sorted.last())
        .copied()
        .unwrap_or(0.0);
    let db = |x: f64| 10.0 * x.max(1e-30).log10();
    Ratios {
        total_db: db(total),
        loud_db: db(loud),
    }
}

/// Total noise-to-mask ratio of the peak-control disturbance of `output`
/// against `prefix`, in dB. Both are audio at `rate`. The disturbance is
/// `gain_residual`, and the reference of the ratio is the output less the
/// disturbance. Both are formed for each sample as the frames read it: a
/// long render holds no copy of either.
pub fn peak_control_nmr_db(prefix: &[Stereo], output: &[Stereo], rate: u32) -> f64 {
    peak_control_ratios(prefix, output, rate).total_db
}

/// The ratios of `peak_control_nmr_db`.
pub fn peak_control_ratios(prefix: &[Stereo], output: &[Stereo], rate: u32) -> Ratios {
    let fit = GainFit::of(prefix, output, GAIN_BLOCK);
    let pair = |i: usize| {
        let (y, r) = (output[i], fit.residual(prefix, output, i));
        [[y[0] - r[0], y[1] - r[1]], r]
    };
    let (masker, noise) = masking::frame_pairs_of(fit.len(), pair, Channels::Both, rate);
    nmr_of(&masker, &noise, rate)
}

/// Peak control of the set on the 4x oversampled prefix `up` at one drive,
/// trimmed to the ceiling, at the base rate `rate`.
pub(crate) fn peak_control(
    up: &mut scratch::Pcm,
    drive_db: f64,
    clipper_db: f64,
    ceiling_dbtp: f64,
    limiter: LimiterSettings,
    true_peak: TruePeak,
    rate: u32,
) -> Result<(Vec<Stereo>, render_blocks::TruePeakFit)> {
    let (mut audio, _, _, _) = render_blocks::render_peak_control(
        up,
        drive_db,
        clipper_db,
        dsp::gain(ceiling_dbtp - 0.1),
        limiter,
        rate,
    )?;
    let (_, fit) = render_blocks::fit_true_peak(&mut audio, ceiling_dbtp, true_peak, rate);
    Ok((audio, fit))
}

/// Output loudness within which `render_at` stops, in LU: the loudness
/// accuracy of the search of a version render.
pub const RENDER_TOLERANCE_LU: f64 = 0.05;
/// Largest number of renders of `render_at`.
const RENDERS: usize = 3;

/// Peak control of the set on the 4x oversampled prefix `up` at the drive
/// that gives `target_lufs`: the first drive from the probes of `point`
/// (`Point::drive_for`), then secant steps until the output is within
/// `RENDER_TOLERANCE_LU` of the target, in at most `RENDERS` renders.
/// Returns the drive, the render, its true-peak fit and its probe, whose
/// noise-to-mask ratio is that of the render against `prefix`, at the base
/// rate `rate`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn render_at(
    up: &mut scratch::Pcm,
    prefix: &[Stereo],
    point: &Point,
    target_lufs: f64,
    clipper_db: f64,
    ceiling_dbtp: f64,
    limiter: LimiterSettings,
    true_peak: TruePeak,
    mix: Option<&crate::analysis::Temporal>,
    rate: u32,
) -> Result<(f64, Vec<Stereo>, render_blocks::TruePeakFit, Probe)> {
    let mut drive_db = point.drive_for(target_lufs);
    let mut last: Option<(f64, f64)> = None;
    for render in 1..=RENDERS {
        let (audio, fit) = peak_control(
            up,
            drive_db,
            clipper_db,
            ceiling_dbtp,
            limiter,
            true_peak,
            rate,
        )?;
        let lufs = dsp::loudness(&audio, rate).ok_or("Missing output loudness")?;
        if (lufs - target_lufs).abs() <= RENDER_TOLERANCE_LU || render == RENDERS {
            let probe = Probe::of(prefix, &audio, drive_db, mix, rate)?;
            return Ok((drive_db, audio, fit, probe));
        }
        // The loudness rises by at most one LU per dB of drive; the limiter
        // and the clipper take part of each dB.
        let slope = last
            .filter(|(d, _)| (drive_db - d).abs() > 1e-9)
            .map_or(1.0, |(d, l)| ((lufs - l) / (drive_db - d)).clamp(0.2, 1.0));
        last = Some((drive_db, lufs));
        drive_db += (target_lufs - lufs) / slope;
    }
    unreachable!("the last render returns")
}

impl Point {
    /// The drive that gives `lufs`, interpolated between the two probes
    /// nearest in loudness. Under the quietest probe, the drive falls by one
    /// dB per LU: peak control does less work there, and each dB of drive
    /// tends to one LU of output.
    pub fn drive_for(&self, lufs: f64) -> f64 {
        if let Some(quietest) = self
            .probes
            .iter()
            .min_by(|a, b| a.lufs.total_cmp(&b.lufs))
            .filter(|q| lufs < q.lufs)
        {
            return quietest.drive_db + (lufs - quietest.lufs);
        }
        let mut nearest = self.probes.clone();
        nearest.sort_by(|a, b| (a.lufs - lufs).abs().total_cmp(&(b.lufs - lufs).abs()));
        let (a, b) = (nearest[0], nearest[1]);
        if (b.lufs - a.lufs).abs() < 1e-9 {
            return a.drive_db;
        }
        a.drive_db + (lufs - a.lufs) * (b.drive_db - a.drive_db) / (b.lufs - a.lufs)
    }

    /// The total masked noise-to-mask ratio at `lufs`, interpolated between
    /// the two probes nearest in loudness, in dB.
    pub fn nmr_at(&self, lufs: f64) -> f64 {
        let mut nearest = self.probes.clone();
        nearest.sort_by(|a, b| (a.lufs - lufs).abs().total_cmp(&(b.lufs - lufs).abs()));
        let (a, b) = (nearest[0], nearest[1]);
        if (b.lufs - a.lufs).abs() < 1e-9 {
            return a.nmr_db;
        }
        a.nmr_db + (lufs - a.lufs) * (b.nmr_db - a.nmr_db) / (b.lufs - a.lufs)
    }
}

/// One peak-control render of the search.
#[derive(Clone, Copy, Serialize)]
pub struct Probe {
    pub drive_db: f64,
    pub lufs: f64,
    /// The total noise-to-mask ratio.
    pub nmr_db: f64,
    /// The ratio of the loudest frames (`Ratios::loud_db`).
    pub loud_nmr_db: f64,
    /// P90 attack loss of the render against the mix; None without the
    /// analysis of the mix or without onsets.
    pub attack_loss_db: Option<f64>,
}

impl Probe {
    /// The guard of the larger excess: "total" or "attack".
    pub fn bound(&self) -> &'static str {
        if self.attack_excess_db() > self.nmr_db - AUDIBLE_NMR_DB {
            "attack"
        } else {
            "total"
        }
    }

    fn attack_excess_db(&self) -> f64 {
        self.attack_loss_db.map_or(f64::NEG_INFINITY, |a| {
            ATTACK_SCALE * (a - ATTACK_LOSS_MAX_DB)
        })
    }

    /// The probe of the render `audio` of peak control on `prefix` at
    /// `drive_db`. `mix` is the analysis of the mix for the attack loss.
    pub fn of(
        prefix: &[Stereo],
        audio: &[Stereo],
        drive_db: f64,
        mix: Option<&crate::analysis::Temporal>,
        rate: u32,
    ) -> Result<Self> {
        let ratios = peak_control_ratios(prefix, audio, rate);
        Ok(Probe {
            drive_db,
            lufs: dsp::loudness(audio, rate).ok_or("Missing output loudness")?,
            nmr_db: ratios.total_db,
            loud_nmr_db: ratios.loud_db,
            attack_loss_db: mix
                .and_then(|m| crate::analysis::compare(m, audio, rate).attack_loss_p90_db),
        })
    }

    /// How far the probe is over the point, in dB of total ratio: the
    /// larger excess of the total ratio and the scaled attack loss. The point
    /// is where it reaches 0.
    pub fn excess_db(&self) -> f64 {
        (self.nmr_db - AUDIBLE_NMR_DB).max(self.attack_excess_db())
    }
}

/// The audibility point of one prefix at one clip budget.
#[derive(Clone, Serialize)]
pub struct Point {
    /// Output loudness at an excess of 0 dB (`Probe::excess_db`),
    /// interpolated between the two probes that hold it.
    pub lufs: f64,
    pub probes: Vec<Probe>,
    /// The guard that sets the point: "total" or "attack".
    pub bound: &'static str,
}

/// Peak control of the set on `prefix` at each drive, and the output
/// loudness at which the excess reaches 0 dB (`Probe::excess_db`). `mix` is
/// the analysis of the mix for the attack loss; without it, the total ratio
/// sets the point. The search starts at `initial_drive_db` and 1 dB
/// above or under it, and then takes secant steps of at most 4 dB until a
/// probe is within `TOLERANCE_DB` of the point. The loudness comes from the
/// two nearest probes. Returns `None` when no probe comes within the
/// tolerance, for example when the ratio stops rising with drive. `prefix`
/// is audio at `rate`.
#[allow(clippy::too_many_arguments)]
pub fn point(
    prefix: &[Stereo],
    initial_drive_db: f64,
    clipper_db: f64,
    ceiling_dbtp: f64,
    limiter: LimiterSettings,
    true_peak: TruePeak,
    mix: Option<&crate::analysis::Temporal>,
    rate: u32,
) -> Result<Option<Point>> {
    // The 4x prefix lives in scratch storage, as in the renderer.
    let mut up = scratch::Pcm::new(scratch::temporary()?);
    render_blocks::upsample_into(prefix, &mut up, rate)?;
    point_on(
        prefix,
        &mut up,
        initial_drive_db,
        clipper_db,
        ceiling_dbtp,
        limiter,
        true_peak,
        mix,
        rate,
    )
}

/// One probe of `point` on `prefix` at `drive_db`: the output loudness and
/// the guards of peak control at that drive.
#[allow(clippy::too_many_arguments)]
pub fn probe(
    prefix: &[Stereo],
    drive_db: f64,
    clipper_db: f64,
    ceiling_dbtp: f64,
    limiter: LimiterSettings,
    true_peak: TruePeak,
    mix: Option<&crate::analysis::Temporal>,
    rate: u32,
) -> Result<Probe> {
    let mut up = scratch::Pcm::new(scratch::temporary()?);
    render_blocks::upsample_into(prefix, &mut up, rate)?;
    let (audio, _) = peak_control(
        &mut up,
        drive_db,
        clipper_db,
        ceiling_dbtp,
        limiter,
        true_peak,
        rate,
    )?;
    Probe::of(prefix, &audio, drive_db, mix, rate)
}

/// `point` on `up`, the 4x prefix of `prefix` (`render_blocks::upsample_into`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn point_on(
    prefix: &[Stereo],
    up: &mut scratch::Pcm,
    initial_drive_db: f64,
    clipper_db: f64,
    ceiling_dbtp: f64,
    limiter: LimiterSettings,
    true_peak: TruePeak,
    mix: Option<&crate::analysis::Temporal>,
    rate: u32,
) -> Result<Option<Point>> {
    let mut probe = |drive_db: f64| -> Result<Probe> {
        let (audio, _) = peak_control(
            up,
            drive_db,
            clipper_db,
            ceiling_dbtp,
            limiter,
            true_peak,
            rate,
        )?;
        Probe::of(prefix, &audio, drive_db, mix, rate)
    };
    let first = probe(initial_drive_db)?;
    let step = if first.excess_db() < 0.0 {
        FIRST_STEP_DB
    } else {
        -FIRST_STEP_DB
    };
    let mut probes = vec![first, probe(initial_drive_db + step)?];
    while probes.len() < PROBES {
        let (a, b) = (probes[probes.len() - 2], probes[probes.len() - 1]);
        if b.excess_db().abs() <= TOLERANCE_DB {
            break;
        }
        let slope = (b.excess_db() - a.excess_db()) / (b.drive_db - a.drive_db);
        if slope <= 0.0 || !slope.is_finite() {
            return Ok(None);
        }
        let next = (-b.excess_db() / slope).clamp(-LARGEST_STEP_DB, LARGEST_STEP_DB);
        probes.push(probe(b.drive_db + next)?);
    }
    // The two probes nearest the point in excess give the loudness.
    let mut nearest = probes.clone();
    nearest.sort_by(|a, b| a.excess_db().abs().total_cmp(&b.excess_db().abs()));
    let (a, b) = (nearest[0], nearest[1]);
    if a.excess_db().abs() > TOLERANCE_DB || (b.excess_db() - a.excess_db()).abs() < 1e-9 {
        return Ok(None);
    }
    let t = -a.excess_db() / (b.excess_db() - a.excess_db());
    let lufs = a.lufs + t * (b.lufs - a.lufs);
    Ok(Some(Point {
        lufs,
        probes,
        bound: a.bound(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point() -> Point {
        let probe = |drive_db: f64, lufs: f64, nmr_db: f64| Probe {
            drive_db,
            lufs,
            nmr_db,
            loud_nmr_db: nmr_db,
            attack_loss_db: None,
        };
        Point {
            bound: "total",
            lufs: -8.3,
            probes: vec![
                probe(6.0, -9.5, -5.6),
                probe(7.0, -8.9, -2.5),
                probe(7.8, -8.4, -0.4),
            ],
        }
    }

    #[test]
    fn the_ratio_of_a_render_equals_the_ratio_of_its_copies() {
        // A prefix and a clipped render of it, which leave a disturbance.
        let prefix: Vec<Stereo> = (0..48_000 * 4 + 777)
            .map(|i| {
                let t = i as f64 / 48_000.0;
                let x = 0.5 * (2.0 * std::f64::consts::PI * 220.0 * t).sin()
                    + 0.2 * (2.0 * std::f64::consts::PI * 3_100.0 * t).sin();
                [x as f32, (0.8 * x) as f32]
            })
            .collect();
        let output: Vec<Stereo> = prefix
            .iter()
            .map(|f| f.map(|x| (1.3 * x).clamp(-0.6, 0.6)))
            .collect();
        let residual = gain_residual(&prefix, &output, GAIN_BLOCK);
        let reference: Vec<Stereo> = output
            .iter()
            .zip(&residual)
            .map(|(y, r)| [y[0] - r[0], y[1] - r[1]])
            .collect();
        assert_eq!(
            peak_control_nmr_db(&prefix, &output, 48_000).to_bits(),
            total_nmr_db(&reference, &residual, 48_000).to_bits()
        );
    }

    #[test]
    fn the_drive_under_the_quietest_probe_falls_one_db_per_lu() {
        let p = point();
        // Between the probes: the line through the two nearest in loudness.
        assert!((p.drive_for(-9.2) - 6.5).abs() < 1e-9);
        // Under the quietest probe: one dB of drive per LU.
        assert!((p.drive_for(-11.5) - 4.0).abs() < 1e-9);
        // Above the loudest probe: the line through the two loudest.
        assert!(p.drive_for(-8.0) > 7.8);
    }
}
