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
    masking::{self, Channels, Measures, BANDS},
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
        .map(|i| fit.residual(prefix[i], output[i], i))
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
                Self::gain(&prefix[range.clone()], &output[range])
            })
            .collect();
        Self { gains, block }
    }

    /// `of` of a prefix and an output in scratch storage, read 64 blocks
    /// at a time.
    fn of_frames(
        prefix: &mut impl scratch::Frames,
        output: &mut impl scratch::Frames,
        block: usize,
    ) -> Result<Self> {
        let blocks = prefix.len().min(output.len()) / block;
        let mut gains = Vec::with_capacity(blocks);
        let (mut read, mut rendered) = (Vec::new(), Vec::new());
        for first in (0..blocks).step_by(64) {
            let count = 64.min(blocks - first);
            prefix.read(first * block, count * block, &mut read)?;
            let output = output.window(first * block, count * block, &mut rendered)?;
            for (x, y) in read.chunks(block).zip(output.chunks(block)) {
                gains.push(Self::gain(x, y));
            }
        }
        Ok(Self { gains, block })
    }

    /// The least-squares gain of the frames `prefix` in the frames `output`.
    fn gain(prefix: &[Stereo], output: &[Stereo]) -> f64 {
        let (mut xy, mut xx) = (0.0f64, 0.0f64);
        for (x, y) in prefix.iter().zip(output) {
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
    }

    /// Samples of the residual: a whole number of blocks.
    fn len(&self) -> usize {
        self.gains.len() * self.block
    }

    /// Sample `i` of the residual of the output against the prefix, from
    /// their frames `x` and `y` at `i`, with the gain interpolated between
    /// the block centres.
    #[inline]
    fn residual(&self, x: Stereo, y: Stereo, i: usize) -> Stereo {
        let (gains, block) = (&self.gains, self.block);
        let blocks = gains.len();
        let centre = |b: usize| (b * block + block / 2) as f64;
        let position = (i as f64 - block as f64 / 2.0) / block as f64;
        let b = (position.floor().max(0.0) as usize).min(blocks.saturating_sub(2));
        let t = ((i as f64 - centre(b)) / block as f64).clamp(0.0, 1.0);
        let g = gains[b] + (gains[(b + 1).min(blocks - 1)] - gains[b]) * t;
        std::array::from_fn(|c| (y[c] as f64 - g * x[c] as f64) as f32)
    }

    /// The reference and the disturbance of the ratio at frame `i`, from the
    /// frames `x` of the prefix and `y` of the output.
    #[inline]
    fn pair(&self, x: Stereo, y: Stereo, i: usize) -> [Stereo; 2] {
        let r = self.residual(x, y, i);
        [[y[0] - r[0], y[1] - r[1]], r]
    }
}

/// PEAQ total noise-to-mask ratio of `disturbance` in `reference`, in dB,
/// over the frames within `QUIET_DB` of the loudest frame. Both are audio at
/// `rate`.
pub fn total_nmr_db(reference: &[Stereo], disturbance: &[Stereo], rate: u32) -> f64 {
    let n = reference.len().min(disturbance.len());
    let masker = masking::frames(
        &reference[..n],
        Channels::Both,
        Measures::PowersAndFlatness,
        rate,
    );
    let noise = masking::frames(&disturbance[..n], Channels::Both, Measures::Powers, rate);
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

/// The measures of the frames of the masker and of the noise of a ratio.
/// The threshold takes the flatness of the masker.
const MASKER_AND_NOISE: [Measures; 2] = [Measures::PowersAndFlatness, Measures::Powers];

/// The noise-to-mask ratios of the frames `noise` under the frames `masker`
/// of audio at `rate`.
fn nmr_of(masker: &masking::Frames, noise: &masking::Frames, rate: u32) -> Ratios {
    let mut ratios = FrameRatios::new(rate);
    ratios.push(masker, noise);
    ratios.finish()
}

/// The band power of the masker and the noise-to-mask ratio of each frame,
/// from consecutive frames. A frame takes 16 bytes, against 416 bytes for
/// its masking frames (D238).
struct FrameRatios {
    thresholds: masking::Thresholds,
    frames: Vec<(f64, f64)>,
}

impl FrameRatios {
    fn new(rate: u32) -> Self {
        Self {
            thresholds: masking::Thresholds::new(Channels::Both, rate),
            frames: Vec::new(),
        }
    }

    /// Add the frames `noise` under the next frames `masker`.
    fn push(&mut self, masker: &masking::Frames, noise: &masking::Frames) {
        for ((p, sfm), q) in masker
            .powers
            .iter()
            .zip(&masker.flatness_db)
            .zip(&noise.powers)
        {
            let threshold = self.thresholds.next(p, *sfm);
            let ratio = (0..BANDS)
                .map(|z| q[z] / threshold[z].max(1e-30))
                .sum::<f64>()
                / BANDS as f64;
            self.frames.push((p.iter().sum::<f64>(), ratio));
        }
    }

    /// The ratios over the frames within `QUIET_DB` of the loudest frame.
    fn finish(self) -> Ratios {
        let loudest = self.frames.iter().map(|f| f.0).fold(0.0, f64::max);
        let ratios: Vec<f64> = self
            .frames
            .iter()
            .filter(|(p, _)| *p >= loudest * 10f64.powf(-QUIET_DB / 10.0))
            .map(|(_, ratio)| *ratio)
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
    let pair = |i: usize| fit.pair(prefix[i], output[i], i);
    let (masker, noise) =
        masking::frame_pairs_of(fit.len(), pair, Channels::Both, MASKER_AND_NOISE, rate);
    nmr_of(&masker, &noise, rate)
}

/// Hops of the masking frames in each span of a prefix in scratch storage:
/// 1.05 million frames, 8.4 MB. A span of 8.4 million frames grew the
/// linear memory of the browser engine by 64 MiB (D235).
const SPAN_HOPS: usize = 4 * crate::stft::PAIR_RANGE;

/// `peak_control_ratios` of a prefix and an output in scratch storage. The
/// masking frames come from consecutive spans of the prefix and the
/// output, so a long prefix or render is not held in memory; they equal
/// the frames of the whole prefix. Each span gives the ratio of each of its
/// frames (D235, D248).
pub(crate) fn peak_control_ratios_of(
    prefix: &mut impl scratch::Frames,
    output: &mut impl scratch::Frames,
    rate: u32,
) -> Result<Ratios> {
    let fit = GainFit::of_frames(prefix, output, GAIN_BLOCK)?;
    let hops = fit.len() / masking::HOP;
    let mut ratios = FrameRatios::new(rate);
    let (mut span, mut rendered) = (Vec::new(), Vec::new());
    for start in (0..hops).step_by(SPAN_HOPS) {
        let part = start..(start + SPAN_HOPS).min(hops);
        let reads = masking::pair_reads(part.clone());
        prefix.read(reads.start, reads.len(), &mut span)?;
        let output = output.window(reads.start, reads.len(), &mut rendered)?;
        let pair = |i: usize| fit.pair(span[i - reads.start], output[i - reads.start], i);
        let (masker, noise) =
            masking::frame_pairs_in(part, pair, Channels::Both, MASKER_AND_NOISE, rate);
        ratios.push(&masker, &noise);
    }
    Ok(ratios.finish())
}

/// The prefix of a point search: in memory, or in scratch storage, where
/// the ratios read it in spans.
pub(crate) enum Prefix<'a> {
    Whole(&'a [Stereo]),
    /// A prefix that its 4x frames can also read (D251).
    Shared(scratch::Shared<'a>),
}

impl Prefix<'_> {
    /// `peak_control_ratios` of `output` against the prefix.
    pub(crate) fn ratios(
        &mut self,
        output: &mut impl scratch::Frames,
        rate: u32,
    ) -> Result<Ratios> {
        match self {
            Prefix::Whole(prefix) => {
                peak_control_ratios_of(&mut scratch::Slice(prefix), output, rate)
            }
            Prefix::Shared(prefix) => peak_control_ratios_of(prefix, output, rate),
        }
    }
}

/// Peak control of the set on the 4x oversampled prefix `up` at one drive,
/// trimmed to the ceiling, at the base rate `rate`. The render goes to a new
/// scratch file (D248).
pub(crate) fn peak_control(
    up: &mut impl scratch::Frames,
    drive_db: f64,
    clipper_db: f64,
    ceiling_dbtp: f64,
    limiter: LimiterSettings,
    true_peak: TruePeak,
    rate: u32,
) -> Result<(scratch::Pcm, render_blocks::TruePeakFit)> {
    let mut audio = scratch::Pcm::new(scratch::temporary()?);
    render_blocks::render_peak_control_audio_into(
        up,
        drive_db,
        clipper_db,
        dsp::gain(ceiling_dbtp - 0.1),
        limiter,
        rate,
        &mut audio,
    )?;
    let (_, fit) = render_blocks::fit_true_peak_stored(&mut audio, ceiling_dbtp, true_peak, rate)?;
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
    up: &mut impl scratch::Frames,
    prefix: &mut Prefix<'_>,
    point: &Point,
    target_lufs: f64,
    clipper_db: f64,
    ceiling_dbtp: f64,
    limiter: LimiterSettings,
    true_peak: TruePeak,
    mix: Option<&crate::analysis::Temporal>,
    rate: u32,
) -> Result<(f64, scratch::Pcm, render_blocks::TruePeakFit, Probe)> {
    let mut drive_db = point.drive_for(target_lufs);
    let mut last: Option<(f64, f64)> = None;
    for render in 1..=RENDERS {
        let (mut audio, fit) = peak_control(
            up,
            drive_db,
            clipper_db,
            ceiling_dbtp,
            limiter,
            true_peak,
            rate,
        )?;
        let lufs = dsp::loudness_read(&mut audio, rate)?.ok_or("Missing output loudness")?;
        if (lufs - target_lufs).abs() <= RENDER_TOLERANCE_LU || render == RENDERS {
            let ratios = prefix.ratios(&mut audio, rate)?;
            let probe = Probe::with_ratios(ratios, &mut audio, drive_db, mix, rate)?;
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
        Self::with_ratios(
            peak_control_ratios(prefix, audio, rate),
            &mut scratch::Slice(audio),
            drive_db,
            mix,
            rate,
        )
    }

    /// `of` with the ratios of `audio` against the prefix.
    fn with_ratios(
        ratios: Ratios,
        audio: &mut impl scratch::Frames,
        drive_db: f64,
        mix: Option<&crate::analysis::Temporal>,
        rate: u32,
    ) -> Result<Self> {
        let lufs = dsp::loudness_read(audio, rate)?.ok_or("Missing output loudness")?;
        let attack_loss_db = match mix {
            Some(m) => {
                let blocks = crate::analysis::blocks_read(audio, rate)?;
                crate::analysis::compare_blocks(m, &blocks).attack_loss_p90_db
            }
            None => None,
        };
        Ok(Probe {
            drive_db,
            lufs,
            nmr_db: ratios.total_db,
            loud_nmr_db: ratios.loud_db,
            attack_loss_db,
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
        &mut Prefix::Whole(prefix),
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
    probe_on(
        &mut Prefix::Whole(prefix),
        &mut up,
        drive_db,
        clipper_db,
        ceiling_dbtp,
        limiter,
        true_peak,
        mix,
        rate,
    )
}

/// `probe` on `up`, the 4x prefix of `prefix`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn probe_on(
    prefix: &mut Prefix,
    up: &mut impl scratch::Frames,
    drive_db: f64,
    clipper_db: f64,
    ceiling_dbtp: f64,
    limiter: LimiterSettings,
    true_peak: TruePeak,
    mix: Option<&crate::analysis::Temporal>,
    rate: u32,
) -> Result<Probe> {
    let (mut audio, _) = peak_control(
        up,
        drive_db,
        clipper_db,
        ceiling_dbtp,
        limiter,
        true_peak,
        rate,
    )?;
    let ratios = prefix.ratios(&mut audio, rate)?;
    Probe::with_ratios(ratios, &mut audio, drive_db, mix, rate)
}

/// `point` on `up`, the 4x prefix of `prefix` (`render_blocks::upsample_into`).
#[allow(clippy::too_many_arguments)]
pub(crate) fn point_on(
    prefix: &mut Prefix,
    up: &mut impl scratch::Frames,
    initial_drive_db: f64,
    clipper_db: f64,
    ceiling_dbtp: f64,
    limiter: LimiterSettings,
    true_peak: TruePeak,
    mix: Option<&crate::analysis::Temporal>,
    rate: u32,
) -> Result<Option<Point>> {
    let mut probe = |drive_db: f64| -> Result<Probe> {
        probe_on(
            prefix,
            up,
            drive_db,
            clipper_db,
            ceiling_dbtp,
            limiter,
            true_peak,
            mix,
            rate,
        )
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
    fn the_ratios_of_a_stored_prefix_equal_those_of_the_whole_prefix() {
        // Longer than one span of `SPAN_HOPS`, and not a whole number of
        // gain blocks or hops.
        let frames = SPAN_HOPS * masking::HOP + 3 * GAIN_BLOCK + 777;
        let prefix: Vec<Stereo> = (0..frames)
            .map(|i| {
                let t = i as f64 / 48_000.0;
                let x = 0.5 * (2.0 * std::f64::consts::PI * 220.0 * t).sin()
                    + 0.2 * (2.0 * std::f64::consts::PI * 3_100.0 * t).sin() * (0.3 * t).cos();
                [x as f32, (0.8 * x) as f32]
            })
            .collect();
        let output: Vec<Stereo> = prefix
            .iter()
            .map(|f| f.map(|x| (1.3 * x).clamp(-0.6, 0.6)))
            .collect();
        let mut stored = scratch::Pcm::new(scratch::memory().unwrap());
        for block in prefix.chunks(65_536) {
            stored.push(block).unwrap();
        }
        let whole = peak_control_ratios(&prefix, &output, 48_000);
        // A stored render gives the ratios of the render in memory (D248).
        let mut rendered = scratch::Pcm::new(scratch::memory().unwrap());
        rendered.push(&output).unwrap();
        let stored = std::cell::RefCell::new(stored);
        for spans in [
            Prefix::Shared(scratch::Shared(&stored))
                .ratios(&mut scratch::Slice(&output), 48_000)
                .unwrap(),
            Prefix::Shared(scratch::Shared(&stored))
                .ratios(&mut rendered, 48_000)
                .unwrap(),
            Prefix::Whole(&prefix)
                .ratios(&mut rendered, 48_000)
                .unwrap(),
        ] {
            assert_eq!(whole.total_db.to_bits(), spans.total_db.to_bits());
            assert_eq!(whole.loud_db.to_bits(), spans.loud_db.to_bits());
        }
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
