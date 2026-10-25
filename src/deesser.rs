//! De-esser of the Master family (`set.deesser.v4`).
//!
//! The vocal stem keys the sibilant frames: frames where the vocal has more
//! power from 5 to 10 kHz than from 1 to 4 kHz, with its total power within
//! 40 dB of its loudest frame and its 5 to 10 kHz power within 30 dB of its
//! loudest. A keyed frame is one in which the stem carries the sibilant band
//! of the mix's mid channel, and in which the vocal stem's mid has more of
//! that band than each of the drum, bass and other stems' mids. In a render,
//! the threshold is the 90% quantile of the excess of the 5 to 10 kHz band
//! over the 1 to 4 kHz band of the keyed frames (`THRESHOLD_QUANTILE`), plus
//! the threshold of the plan. In a
//! keyed frame above the threshold, the stage cuts a zero-phase 5 to 10 kHz
//! band of the mid by the excess over the threshold times the frame's vocal
//! share, by up to the largest cut of the plan. The gain moves linearly
//! between frame centres. The side is unchanged. A render whose cut removes
//! more of the band energy of the drum, bass and other stems than
//! `OTHER_LOSS_LIMIT_DB` halves the largest cut once, then cuts nothing.
//! The versions of a set cut only with the de-esser switch on
//! (`set::Options::deesser`, off by default). With the switch off, the plan
//! keeps the key and the measure and cuts nothing.
use crate::{
    audio_io::{Result, Stereo},
    fir, masking, resample,
};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

/// The sibilant band and the vocal body band, in Hz.
pub const SIBILANT_HZ: (f64, f64) = (5000.0, 10000.0);
pub const BODY_HZ: (f64, f64) = (1000.0, 4000.0);
/// Largest cut of the sibilant band in the versions of a set with the
/// de-esser switch on, in dB (`set.deesser.v4`, D209): the depth of the
/// alternative of D153. The sibilant P99 of a version against the other
/// vocal frames falls by about this cut. The tuning compared this P99 with
/// the range of the album masters of D177. With the switch off, the default,
/// the largest cut is 0 dB.
pub const MAXIMUM_CUT_DB: f64 = 3.0;
/// The stem check of a render (`set.deesser.v4`, D209): the cut frames remove at
/// most this share of the 5 to 10 kHz energy of the mid channels of the
/// drum, bass and other stems over the recording, in dB.
pub const OTHER_LOSS_LIMIT_DB: f64 = -0.3;
/// The largest cut of a plan, in dB (`Plan::validate`). A Sibilance step
/// down raises the largest cut of a plan up to this value.
pub const CUT_LIMIT_DB: f64 = 12.0;
/// The range of `Plan::threshold_db`, in dB. The reference excess is the 90%
/// quantile (`THRESHOLD_QUANTILE`), and the median excess of a vocal lies
/// several dB under it. A step down lowers the threshold to 12 dB under the
/// reference, so a Sibilance step of -4 at the full share reaches its 2 dB
/// of mean cut (`adjust::sibilance_shift`, D209).
pub const THRESHOLD_RANGE_DB: (f64, f64) = (-12.0, 6.0);
/// A keyed frame's vocal power is within this of the loudest vocal frame.
const VOCAL_ACTIVE_DB: f64 = 40.0;
/// A keyed frame's vocal sibilant power is within this of its loudest.
const SIBILANT_ACTIVE_DB: f64 = 30.0;
/// Taps of the band filter: 85 ms and 11.7 Hz resolution at 48 kHz.
const TAPS: usize = 4097;
/// Width of the band edges, in octaves.
const EDGE_OCTAVES: f64 = 1.0 / 3.0;
/// Frames under this many keyed frames give no plan.
const MINIMUM_FRAMES: usize = 20;
/// In a keyed frame, the mid channel of the vocal stem has this share or
/// more of the 5 to 10 kHz power of the mix's mid channel: the band in that
/// frame is the vocal.
pub const CARRIED_SHARE: f64 = 0.5;
/// In its active frames, the vocal stem has this share or more of the power
/// of the mix: the stem holds a vocal. The stem of a recording without a
/// vocal holds the residue of the separation, far under this share of the
/// mix (D153).
pub const VOCAL_SHARE: f64 = 0.01;

/// A keyed frame of a plan.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Debug)]
#[serde(deny_unknown_fields)]
pub struct Keyed {
    /// Frame of `masking::frames` at the rate of the render (hop
    /// `masking::HOP`).
    pub frame: u32,
    /// The vocal stem's mid 5 to 10 kHz power over the mix's, at most 1.
    /// The band filter cuts every source of the band in the frame, so the
    /// cut scales by this share.
    pub vocal_share: f64,
    /// The 5 to 10 kHz power of the mid channels of the drum, bass and other
    /// stems in the frame (`Plan::other_band_power`).
    pub other_power: f64,
}

/// The key of a vocal stem.
#[derive(Clone, Serialize, Default, PartialEq, Debug)]
pub struct Key {
    pub keyed: Vec<Keyed>,
    /// The 5 to 10 kHz power of the mid channels of the drum, bass and other
    /// stems, summed over every frame of the recording.
    pub other_band_power: f64,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Debug)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub rule: String,
    /// Keyed frames, in ascending frame order.
    pub keyed: Vec<Keyed>,
    /// The 5 to 10 kHz power of the mid channels of the drum, bass and other
    /// stems over the whole recording: the reference of the stem check.
    pub other_band_power: f64,
    pub maximum_cut_db: f64,
    /// Threshold over the reference excess of the keyed frames, in dB, within
    /// `THRESHOLD_RANGE_DB`. The Sibilance control of a request moves it
    /// (`adjust.steps.v1`).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub threshold_db: f64,
    /// The reference excess of a delivered render, in dB
    /// (`Stats::reference_excess_db`). With it, a render uses it in place of
    /// the reference of its own keyed frames and runs no stem check: a span
    /// of a delivered version cuts as the version, and a request moves from
    /// the cut of the version (`Plan::delivered`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference_excess_db: Option<f64>,
}

fn is_zero(x: &f64) -> bool {
    *x == 0.0
}

/// The result of the stem check of a render.
#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default, Debug)]
#[serde(rename_all = "snake_case")]
pub enum StemCheck {
    /// The cut of the plan passed.
    #[default]
    Passed,
    /// The cut of the plan failed and half its largest cut passed.
    Halved,
    /// Half the largest cut failed too: the render cuts nothing.
    NoCut,
    /// The plan carries the reference excess and the largest cut of a
    /// delivered render: no check.
    Fixed,
}

/// What the stage did to one render.
#[derive(Clone, Copy, Serialize, Default, Debug)]
pub struct Stats {
    pub keyed_frames: usize,
    pub cut_frames: usize,
    pub cut_p50_db: f64,
    pub cut_maximum_db: f64,
    /// Mean cut over the keyed frames, in dB: the measure of the Sibilance
    /// control (`adjust.steps.v1`).
    pub cut_mean_db: f64,
    /// The excess of the sibilant band over the body band in the keyed
    /// frames at 0, 5, … 100% of their distribution, in dB: what a Sibilance
    /// step predicts its cut from.
    pub excess_quantiles_db: [f64; EXCESS_QUANTILES],
    /// The mean vocal share of the keyed frames from each excess quantile to
    /// the next, both included.
    pub vocal_share_bins: [f64; EXCESS_QUANTILES - 1],
    /// The reference excess of the threshold, in dB: `excess_quantiles_db`
    /// at 90% (`THRESHOLD_QUANTILE`), or the reference excess of a delivered
    /// render.
    pub reference_excess_db: f64,
    /// The threshold of the cut: the reference excess and the threshold of
    /// the plan, in dB.
    pub threshold_db: f64,
    /// The largest cut after the stem check, in dB.
    pub maximum_cut_db: f64,
    /// The 5 to 10 kHz energy of the drum, bass and other stems that the
    /// cut keeps, as a share of the recording, in dB.
    pub other_loss_db: f64,
    pub stem_check: StemCheck,
}

/// Quantiles of the excess in `Stats::excess_quantiles_db`: every 5%.
pub const EXCESS_QUANTILES: usize = 21;
/// The index of the reference excess of the threshold in
/// `Stats::excess_quantiles_db`: 18, the 90% quantile (`set.deesser.v4`,
/// D209).
/// At a plan threshold of 0 dB, the stage cuts the tenth of the keyed frames
/// with the largest excess.
pub const THRESHOLD_QUANTILE: usize = (EXCESS_QUANTILES - 1) * 9 / 10;

/// Cut of a keyed frame whose excess is `over_db` above the threshold, with
/// the vocal share `share`, at most `maximum_db`.
pub fn cut_db(over_db: f64, share: f64, maximum_db: f64) -> f64 {
    if over_db > 0.0 {
        (over_db * share).min(maximum_db.max(0.0))
    } else {
        0.0
    }
}

/// Mean cut of a distribution given by its excess quantiles at even steps
/// and the mean vocal share between neighbouring quantiles, at
/// `threshold_db` with the largest cut `maximum_db`: the mean over the
/// midpoints between neighbouring quantiles.
pub fn mean_cut_over(
    quantiles: &[f64; EXCESS_QUANTILES],
    shares: &[f64; EXCESS_QUANTILES - 1],
    threshold_db: f64,
    maximum_db: f64,
) -> f64 {
    quantiles
        .windows(2)
        .zip(shares)
        .map(|(w, share)| cut_db(0.5 * (w[0] + w[1]) - threshold_db, *share, maximum_db))
        .sum::<f64>()
        / (EXCESS_QUANTILES - 1) as f64
}

fn band_power(frame: &[f64; masking::BANDS], (low, high): (f64, f64)) -> f64 {
    (0..masking::BANDS)
        .filter(|z| (low..high).contains(&masking::centre_hz(*z)))
        .map(|z| frame[z])
        .sum()
}

/// Frame powers of the mid channel of `audio` at `rate`. The frames read the
/// mid of each frame as they need it: a copy of a 5-minute mix is 115 MB.
fn mid_powers(audio: &[Stereo], rate: u32) -> Vec<[f64; masking::BANDS]> {
    let mid = |n: usize| {
        let f = audio[n];
        [(0.5 * (f[0] as f64 + f[1] as f64)) as f32; 2]
    };
    masking::frames_of(audio.len(), mid, masking::Channels::Left, rate).powers
}

/// The 5 to 10 kHz power of the mid channel of each frame of `audio` at
/// `rate`.
pub(crate) fn sibilant_powers(audio: &[Stereo], rate: u32) -> Vec<f64> {
    mid_powers(audio, rate)
        .iter()
        .map(|f| band_power(f, SIBILANT_HZ))
        .collect()
}

/// `stem` at `stem_rate` at the rate `rate`. A stem at `rate` is read in
/// place: a copy of a 30-minute stem is 0.7 GB.
fn at_rate(stem: &[Stereo], stem_rate: u32, rate: u32) -> Result<Cow<'_, [Stereo]>> {
    Ok(if stem_rate == rate {
        stem.into()
    } else {
        resample::convert(stem, stem_rate, rate)?.into()
    })
}

/// The key of `stems`, the drum, bass, other and vocal stems at
/// `stem_rate`, against the mid channel of `mix` at `rate`.
pub fn key(stems: [&[Stereo]; 4], stem_rate: u32, mix: &[Stereo], rate: u32) -> Result<Key> {
    // One stem at the render rate at a time.
    let mut others: [Vec<f64>; 3] = Default::default();
    for (role, stem) in stems[..3].iter().enumerate() {
        others[role] = sibilant_powers(&at_rate(stem, stem_rate, rate)?, rate);
    }
    let vocal = at_rate(stems[3], stem_rate, rate)?;
    let frames = masking::frames(&vocal, masking::Channels::Both, rate).powers;
    Ok(key_with_frames(&vocal, &frames, &others, mix, rate))
}

/// `key` of a vocal stem at `rate`, the rate of `mix`, with its masking
/// frames, and the mid 5 to 10 kHz power of each frame of the drum, bass and
/// other stems at `rate` (`sibilant_powers`).
pub(crate) fn key_with_frames(
    vocal: &[Stereo],
    frames: &[[f64; masking::BANDS]],
    others: &[Vec<f64>; 3],
    mix: &[Stereo],
    rate: u32,
) -> Key {
    let other_band_power = others.iter().flatten().sum();
    let total: Vec<f64> = frames.iter().map(|f| f.iter().sum()).collect();
    let loudest = total.iter().copied().fold(0.0, f64::max);
    let sibilant: Vec<f64> = frames.iter().map(|f| band_power(f, SIBILANT_HZ)).collect();
    let loudest_sibilant = sibilant.iter().copied().fold(0.0, f64::max);
    let (stem_mid, mix_mid) = (mid_powers(vocal, rate), mid_powers(mix, rate));
    // Power of the stem and of the mix in the stem's active frames.
    let active = |i: usize| total[i] > loudest * 10f64.powf(-VOCAL_ACTIVE_DB / 10.0);
    let (stem_power, mix_power) = (0..frames.len().min(mix_mid.len()))
        .filter(|i| active(*i))
        .fold((0.0, 0.0), |(s, m), i| {
            (
                s + stem_mid[i].iter().sum::<f64>(),
                m + mix_mid[i].iter().sum::<f64>(),
            )
        });
    if stem_power < VOCAL_SHARE * mix_power {
        return Key {
            keyed: Vec::new(),
            other_band_power,
        };
    }
    let other = |role: usize, i: usize| others[role].get(i).copied().unwrap_or(0.0);
    let keyed = frames
        .iter()
        .enumerate()
        .filter_map(|(i, f)| {
            let s = band_power(stem_mid.get(i)?, SIBILANT_HZ);
            let m = band_power(mix_mid.get(i)?, SIBILANT_HZ);
            // The vocal carries the band of the mix, and it is the largest
            // source of the band in the frame.
            (active(i)
                && sibilant[i] > loudest_sibilant * 10f64.powf(-SIBILANT_ACTIVE_DB / 10.0)
                && sibilant[i] > band_power(f, BODY_HZ)
                && s > 0.0
                && s >= CARRIED_SHARE * m
                && (0..3).all(|role| s > other(role, i)))
            .then(|| Keyed {
                frame: i as u32,
                vocal_share: (s / m).min(1.0),
                other_power: (0..3).map(|role| other(role, i)).sum(),
            })
        })
        .collect();
    Key {
        keyed,
        other_band_power,
    }
}

/// 10 log10 of the share of `total` that the cuts `cut` keep of the powers
/// `power`, in dB. 0 dB when `total` is 0.
fn kept_db(power: impl Iterator<Item = f64>, cut: &[f64], total: f64) -> f64 {
    if total <= 0.0 {
        return 0.0;
    }
    let removed: f64 = power
        .zip(cut)
        .map(|(p, c)| p * (1.0 - 10f64.powf(-c / 10.0)))
        .sum();
    10.0 * (1.0 - removed / total).max(1e-30).log10()
}

impl Plan {
    /// The plan of a set version from `key`. With `deesser` false, the plan
    /// keeps the key and the measure and cuts nothing.
    pub fn from_key(key: Key, deesser: bool) -> Option<Self> {
        (key.keyed.len() >= MINIMUM_FRAMES).then(|| Plan {
            rule: "set.deesser.v4".into(),
            keyed: key.keyed,
            other_band_power: key.other_band_power,
            maximum_cut_db: if deesser { MAXIMUM_CUT_DB } else { 0.0 },
            threshold_db: 0.0,
            reference_excess_db: None,
        })
    }

    pub fn validate(&self) -> Result<()> {
        let power = |p: f64| p.is_finite() && p >= 0.0;
        let valid = (0.0..=CUT_LIMIT_DB).contains(&self.maximum_cut_db)
            && (THRESHOLD_RANGE_DB.0..=THRESHOLD_RANGE_DB.1).contains(&self.threshold_db)
            && self.reference_excess_db.is_none_or(f64::is_finite)
            && power(self.other_band_power)
            && self.keyed.windows(2).all(|w| w[0].frame < w[1].frame)
            && self
                .keyed
                .iter()
                .all(|k| k.vocal_share > 0.0 && k.vocal_share <= 1.0 && power(k.other_power));
        if !valid {
            return Err("Invalid de-esser plan".into());
        }
        Ok(())
    }

    /// This plan with the reference excess and the largest cut that its
    /// render solved, from `stats`, the de-esser stats of the render report. A span
    /// of the render then cuts the same frames by the same gains, and a
    /// request moves from the cut of the render.
    pub fn delivered(&self, stats: &serde_json::Value) -> Result<Self> {
        let plan = Plan {
            reference_excess_db: Some(
                stats["reference_excess_db"]
                    .as_f64()
                    .ok_or("Missing de-esser reference excess")?,
            ),
            maximum_cut_db: stats["maximum_cut_db"]
                .as_f64()
                .ok_or("Missing de-esser cut")?,
            ..self.clone()
        };
        plan.validate()?;
        Ok(plan)
    }

    /// The cut of each keyed frame of a span from `measured`, the excess of
    /// each in dB, and the stats of the stage. The threshold is the excess at
    /// `THRESHOLD_QUANTILE`, or the reference excess of the plan, plus
    /// `threshold_db`. A plan without a reference excess runs the stem
    /// check.
    fn solve(&self, measured: &[(f64, Keyed)]) -> (Vec<f64>, Stats) {
        let mut stats = Stats {
            keyed_frames: measured.len(),
            ..Stats::default()
        };
        if !measured.is_empty() {
            let mut order: Vec<usize> = (0..measured.len()).collect();
            order.sort_by(|a, b| measured[*a].0.total_cmp(&measured[*b].0));
            let last = measured.len() - 1;
            // The nearest rank of quantile `i`.
            let index = |i: usize| (i * last + (EXCESS_QUANTILES - 1) / 2) / (EXCESS_QUANTILES - 1);
            stats.excess_quantiles_db = std::array::from_fn(|i| measured[order[index(i)]].0);
            stats.vocal_share_bins = std::array::from_fn(|b| {
                let bin = &order[index(b)..=index(b + 1)];
                bin.iter().map(|k| measured[*k].1.vocal_share).sum::<f64>() / bin.len() as f64
            });
        }
        let reference = self
            .reference_excess_db
            .unwrap_or(stats.excess_quantiles_db[THRESHOLD_QUANTILE]);
        let threshold = reference + self.threshold_db;
        let cuts = |maximum: f64| -> Vec<f64> {
            measured
                .iter()
                .map(|(e, k)| cut_db(e - threshold, k.vocal_share, maximum))
                .collect()
        };
        let kept = |cut: &[f64]| {
            kept_db(
                measured.iter().map(|(_, k)| k.other_power),
                cut,
                self.other_band_power,
            )
        };
        let mut maximum = self.maximum_cut_db;
        let mut cut = cuts(maximum);
        let mut check = if self.reference_excess_db.is_some() {
            StemCheck::Fixed
        } else {
            StemCheck::Passed
        };
        if check == StemCheck::Passed && kept(&cut) < OTHER_LOSS_LIMIT_DB {
            maximum *= 0.5;
            cut = cuts(maximum);
            check = StemCheck::Halved;
            if kept(&cut) < OTHER_LOSS_LIMIT_DB {
                maximum = 0.0;
                cut = cuts(maximum);
                check = StemCheck::NoCut;
            }
        }
        stats.reference_excess_db = reference;
        stats.threshold_db = threshold;
        stats.maximum_cut_db = maximum;
        stats.other_loss_db = kept(&cut);
        stats.stem_check = check;
        let mut positive: Vec<f64> = cut.iter().copied().filter(|c| *c > 0.0).collect();
        if !positive.is_empty() {
            positive.sort_by(f64::total_cmp);
            stats.cut_frames = positive.len();
            stats.cut_mean_db = positive.iter().sum::<f64>() / stats.keyed_frames as f64;
            stats.cut_p50_db = positive[positive.len() / 2];
            stats.cut_maximum_db = *positive.last().unwrap();
        }
        (cut, stats)
    }

    /// Cut the mid channel's sibilant band in the keyed frames of `audio`
    /// at `rate`, in place.
    pub fn apply(&self, audio: &mut [Stereo], rate: u32) -> Stats {
        self.apply_from(audio, 0, rate)
    }

    /// `apply` on the span of a recording that starts at sample `offset`.
    /// The masking frames of the span are those of the recording: the
    /// first starts at the first multiple of `masking::HOP` in the span.
    pub fn apply_from(&self, audio: &mut [Stereo], offset: usize, rate: u32) -> Stats {
        let skip = ((masking::HOP - offset % masking::HOP) % masking::HOP).min(audio.len());
        let first_frame = (offset + skip) / masking::HOP;
        let frames = mid_powers(&audio[skip..], rate);
        // The excess of each keyed frame of the span, and its frame in the
        // span.
        let mut measured = Vec::new();
        let mut at = Vec::new();
        for keyed in &self.keyed {
            let Some(k) = (keyed.frame as usize).checked_sub(first_frame) else {
                continue;
            };
            let Some(f) = frames.get(k) else {
                continue;
            };
            let excess = 10.0
                * (band_power(f, SIBILANT_HZ).max(1e-30) / band_power(f, BODY_HZ).max(1e-30))
                    .log10();
            measured.push((excess, *keyed));
            at.push(k);
        }
        let (cuts, stats) = self.solve(&measured);
        if stats.cut_frames == 0 {
            return stats;
        }
        // Cut of each frame, in dB.
        let mut cut = vec![0.0f64; frames.len()];
        for (k, c) in at.into_iter().zip(cuts) {
            cut[k] = c;
        }
        let kernel = fir::kernel(
            TAPS,
            |hz| {
                let edge = |x: f64| {
                    let t = (x / EDGE_OCTAVES + 0.5).clamp(0.0, 1.0);
                    0.5 - 0.5 * (std::f64::consts::PI * t).cos()
                };
                let low = (hz.max(1.0) / SIBILANT_HZ.0).log2();
                let high = (SIBILANT_HZ.1 / hz.max(1.0)).log2();
                edge(low) * edge(high)
            },
            rate,
        );
        let centre = |k: usize| (skip + k * masking::HOP + masking::FFT / 2) as f64;
        // In place: each output sample is emitted before any later input
        // sample is read, and f64 copies of a 30-minute mid are 0.6 GB.
        let frames = std::cell::Cell::from_mut(audio).as_slice_of_cells();
        let mid = |i: usize| {
            let f = frames[i].get();
            0.5 * (f[0] as f64 + f[1] as f64)
        };
        fir::filter_each(frames.len(), mid, &kernel, |i, band| {
            // The frame whose centre is at or before this sample.
            let k = ((i as f64 - (skip + masking::FFT / 2) as f64) / masking::HOP as f64).floor();
            let k = k.max(0.0) as usize;
            let (a, b) = (
                cut.get(k).copied().unwrap_or(0.0),
                cut.get(k + 1).copied().unwrap_or(0.0),
            );
            if a == 0.0 && b == 0.0 {
                return;
            }
            let t = ((i as f64 - centre(k)) / masking::HOP as f64).clamp(0.0, 1.0);
            let gain = 10f64.powf(-(a + (b - a) * t) / 20.0);
            let frame = frames[i].get();
            let side = 0.5 * (frame[0] as f64 - frame[1] as f64);
            let m = mid(i) + (gain - 1.0) * band;
            frames[i].set([(m + side) as f32, (m - side) as f32]);
        });
        stats
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Noise, with a burst of 7 kHz tone in frames 100 to 109.
    fn signal() -> Vec<Stereo> {
        let mut state = 0x2545_f491u32;
        let mut noise = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state as f32 / u32::MAX as f32) - 0.5
        };
        (0..400 * masking::HOP)
            .map(|i| {
                let burst = (100 * masking::HOP + masking::FFT / 2
                    ..110 * masking::HOP + masking::FFT / 2)
                    .contains(&i);
                let tone = if burst {
                    (2.0 * std::f64::consts::PI * 7000.0 * i as f64 / 48000.0).sin() as f32
                } else {
                    0.0
                };
                let n = 0.05 * noise();
                [n + tone, n + tone]
            })
            .collect()
    }

    /// The key of `vocal` against `mix`, with silent drum, bass and other
    /// stems.
    fn vocal_key(vocal: &[Stereo], mix: &[Stereo]) -> Key {
        let silent = vec![[0.0f32; 2]; vocal.len()];
        key([&silent, &silent, &silent, vocal], 48000, mix, 48000).unwrap()
    }

    fn keyed(frame: u32, vocal_share: f64, other_power: f64) -> Keyed {
        Keyed {
            frame,
            vocal_share,
            other_power,
        }
    }

    fn plan(keyed: Vec<Keyed>, other_band_power: f64) -> Plan {
        Plan {
            rule: "test".into(),
            keyed,
            other_band_power,
            maximum_cut_db: MAXIMUM_CUT_DB,
            threshold_db: 0.0,
            reference_excess_db: None,
        }
    }

    /// 21 keyed frames whose excess is 0, 1, … 20 dB, each with
    /// `other_power` and a vocal share of 1: the reference excess, at 90%,
    /// is 18 dB.
    fn ramp(other_power: f64) -> Vec<(f64, Keyed)> {
        (0..21)
            .map(|i| (i as f64, keyed(i, 1.0, other_power)))
            .collect()
    }

    /// `ramp` with an excess of 25 dB in the top two frames: the reference
    /// excess stays 18 dB, and the two frames get the largest cut.
    fn peaks(other_power: f64) -> Vec<(f64, Keyed)> {
        let mut measured = ramp(other_power);
        measured[19].0 = 25.0;
        measured[20].0 = 25.0;
        measured
    }

    #[test]
    fn mid_frames_equal_the_frames_of_a_mono_copy() {
        let audio: Vec<Stereo> = signal()
            .iter()
            .enumerate()
            .map(|(i, f)| [f[0], f[1] * (i % 7) as f32 * 0.1])
            .collect();
        let mono: Vec<Stereo> = audio
            .iter()
            .map(|f| [(0.5 * (f[0] as f64 + f[1] as f64)) as f32; 2])
            .collect();
        let bits = |frames: Vec<[f64; masking::BANDS]>| {
            frames
                .iter()
                .flatten()
                .map(|x| x.to_bits())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            bits(mid_powers(&audio, 48_000)),
            bits(masking::frames(&mono, masking::Channels::Left, 48_000).powers)
        );
    }

    #[test]
    fn frame_centres_match_the_masking_frames() {
        // An impulse at a frame centre peaks in that frame.
        let mut audio = vec![[0.0f32; 2]; 50 * masking::HOP];
        let k = 20;
        audio[k * masking::HOP + masking::FFT / 2] = [1.0, 1.0];
        let frames = masking::frames(&audio, masking::Channels::Left, 48000).powers;
        let loudest = (0..frames.len())
            .max_by(|a, b| {
                frames[*a]
                    .iter()
                    .sum::<f64>()
                    .total_cmp(&frames[*b].iter().sum())
            })
            .unwrap();
        assert_eq!(loudest, k);
    }

    #[test]
    fn keyed_burst_is_cut_and_the_rest_is_kept() {
        let input = signal();
        let key = vocal_key(&input, &input);
        assert!(
            key.keyed.iter().all(|k| (99..=111).contains(&k.frame)),
            "{key:?}"
        );
        // The stem carries the whole band of the mix.
        assert!(key.keyed.iter().all(|k| k.vocal_share == 1.0));
        // A reference excess far under each excess cuts each keyed frame by
        // the largest cut.
        let plan = Plan {
            maximum_cut_db: 6.0,
            reference_excess_db: Some(-100.0),
            ..plan(key.keyed, 0.0)
        };
        let mut output = input.clone();
        let stats = plan.apply(&mut output, 48000);
        assert!(stats.cut_frames >= 8 && (stats.cut_maximum_db - 6.0).abs() < 1e-9);
        let q = stats.excess_quantiles_db;
        assert!(q.windows(2).all(|w| w[0] <= w[1]) && q[EXCESS_QUANTILES - 1] > 0.0);
        let power = |a: &[Stereo], r: std::ops::Range<usize>| {
            a[r].iter().map(|f| (f[0] as f64).powi(2)).sum::<f64>()
        };
        let burst = 103 * masking::HOP..107 * masking::HOP;
        let cut = 10.0 * (power(&output, burst.clone()) / power(&input, burst)).log10();
        assert!((cut + 6.0).abs() < 0.5, "{cut}");
        let quiet = 200 * masking::HOP..300 * masking::HOP;
        assert!((power(&output, quiet.clone()) / power(&input, quiet) - 1.0).abs() < 1e-6);
        // The side is unchanged.
        assert!(output
            .iter()
            .zip(&input)
            .all(|(a, b)| ((a[0] - a[1]) - (b[0] - b[1])).abs() < 1e-6));
    }

    #[test]
    fn the_threshold_is_the_90_percent_excess_of_the_render() {
        assert_eq!(THRESHOLD_QUANTILE, 18);
        let mut measured = ramp(0.0);
        measured[20].0 = 25.0;
        let (cut, stats) = plan(Vec::new(), 0.0).solve(&measured);
        assert_eq!(stats.reference_excess_db, 18.0);
        assert_eq!(stats.excess_quantiles_db[THRESHOLD_QUANTILE], 18.0);
        assert_eq!(stats.threshold_db, 18.0);
        // A frame at or under the reference excess keeps its level; above
        // it, the cut is the excess over the threshold, at most 3 dB.
        assert!(cut[..=18].iter().all(|c| *c == 0.0));
        assert_eq!(&cut[19..], &[1.0, 3.0]);
        assert_eq!(stats.cut_frames, 2);
        assert!((stats.cut_mean_db - 4.0 / 21.0).abs() < 1e-12);
        // The threshold of the plan moves the threshold of the render.
        let (cut, stats) = Plan {
            threshold_db: -1.0,
            ..plan(Vec::new(), 0.0)
        }
        .solve(&measured);
        assert_eq!((stats.threshold_db, cut[17], cut[18]), (17.0, 0.0, 1.0));
        let (cut, _) = Plan {
            threshold_db: 1.0,
            ..plan(Vec::new(), 0.0)
        }
        .solve(&measured);
        assert_eq!((cut[19], cut[20]), (0.0, 3.0));
    }

    #[test]
    fn a_keyed_frame_with_half_the_band_gets_half_the_excess() {
        let mut measured = ramp(0.0);
        measured[19].1.vocal_share = 0.5;
        measured[20] = (30.0, keyed(20, 0.5, 0.0));
        let (cut, stats) = plan(Vec::new(), 0.0).solve(&measured);
        assert_eq!(cut[19], 0.5);
        // Half of 12 dB, at most the largest cut.
        assert_eq!(cut[20], 3.0);
        // The shares of the bins of the excess quantiles.
        assert_eq!(stats.vocal_share_bins[18], 0.75);
        assert_eq!(stats.vocal_share_bins[19], 0.5);
        assert_eq!(stats.vocal_share_bins[0], 1.0);
        // The bin from 18 to 19 dB cuts 0.5 dB at a share of 1, and a
        // quarter less at 0.75. The bin from 19 to 30 dB cuts 3 dB at both
        // shares.
        let mean = mean_cut_over(
            &stats.excess_quantiles_db,
            &stats.vocal_share_bins,
            stats.threshold_db,
            stats.maximum_cut_db,
        );
        let whole = mean_cut_over(&stats.excess_quantiles_db, &[1.0; 20], 18.0, 3.0);
        assert!((whole - mean - 0.25 * 0.5 / 20.0).abs() < 1e-12);
    }

    #[test]
    fn the_other_loss_is_the_energy_that_the_cut_removes() {
        let mut measured = ramp(1.0);
        measured[19].1.other_power = 4.0;
        let (cut, stats) = plan(Vec::new(), 1000.0).solve(&measured);
        let removed: f64 = measured
            .iter()
            .zip(&cut)
            .map(|((_, k), c)| k.other_power * (1.0 - 10f64.powf(-c / 10.0)))
            .sum();
        let expected = 10.0 * (1.0 - removed / 1000.0).log10();
        assert!((stats.other_loss_db - expected).abs() < 1e-12);
        assert!(stats.other_loss_db < 0.0 && stats.other_loss_db > OTHER_LOSS_LIMIT_DB);
        assert_eq!(stats.stem_check, StemCheck::Passed);
        // A recording without other stems in the band loses nothing.
        assert_eq!(plan(Vec::new(), 0.0).solve(&measured).1.other_loss_db, 0.0);
    }

    #[test]
    fn the_stem_check_halves_then_removes_the_cut() {
        // With an other power of 1 in each keyed frame, the two cuts of 3 dB
        // remove 1.00 unit in total and the two cuts of 1.5 dB remove 0.58
        // units. The limit of -0.3 dB keeps 93.3 of the 100 units of the
        // recording.
        let check = |other_power: f64| plan(Vec::new(), 100.0).solve(&peaks(other_power)).1;
        let passed = check(5.0);
        assert_eq!(passed.stem_check, StemCheck::Passed);
        assert_eq!(passed.maximum_cut_db, 3.0);
        let halved = check(9.0);
        assert_eq!(halved.stem_check, StemCheck::Halved);
        assert_eq!(halved.maximum_cut_db, 1.5);
        assert_eq!(halved.cut_maximum_db, 1.5);
        assert!(halved.other_loss_db >= OTHER_LOSS_LIMIT_DB);
        let none = check(20.0);
        assert_eq!(none.stem_check, StemCheck::NoCut);
        assert_eq!((none.maximum_cut_db, none.cut_frames), (0.0, 0));
        assert_eq!(none.other_loss_db, 0.0);
    }

    #[test]
    fn a_delivered_plan_keeps_its_reference_and_runs_no_stem_check() {
        let stats = plan(Vec::new(), 100.0).solve(&peaks(9.0)).1;
        let delivered = plan(vec![keyed(1, 1.0, 2.0)], 100.0)
            .delivered(&serde_json::to_value(stats).unwrap())
            .unwrap();
        assert_eq!(delivered.reference_excess_db, Some(18.0));
        assert_eq!(delivered.maximum_cut_db, 1.5);
        // The reference excess of the plan sets the threshold, and the stem
        // check of the plan does not run: the loss is over the limit.
        let fixed = Plan {
            reference_excess_db: Some(15.0),
            ..plan(Vec::new(), 100.0)
        };
        let (cut, stats) = fixed.solve(&ramp(5.0));
        assert_eq!(stats.stem_check, StemCheck::Fixed);
        assert_eq!(
            (stats.reference_excess_db, stats.threshold_db),
            (15.0, 15.0)
        );
        assert_eq!(stats.excess_quantiles_db[THRESHOLD_QUANTILE], 18.0);
        assert_eq!(&cut[15..], &[0.0, 1.0, 2.0, 3.0, 3.0, 3.0]);
        assert!(stats.other_loss_db < OTHER_LOSS_LIMIT_DB);
        assert!(plan(Vec::new(), 0.0)
            .delivered(&serde_json::json!({"maximum_cut_db": 3.0}))
            .is_err());
    }

    #[test]
    fn a_delivered_reference_returns_from_the_text_of_the_report() {
        // At the reference frame, the excess equals the threshold and the
        // frame keeps its level. A reference 1 ULP lower from the text of the
        // report cuts it. Without `float_roundtrip`, serde_json reads this
        // value 1 ULP off.
        let reference: f64 = 27.208359647654174;
        let text = serde_json::to_string(&serde_json::json!({
            "reference_excess_db": reference,
            "maximum_cut_db": MAXIMUM_CUT_DB,
        }))
        .unwrap();
        let stats: serde_json::Value = serde_json::from_str(&text).unwrap();
        let delivered = plan(Vec::new(), 0.0).delivered(&stats).unwrap();
        assert_eq!(
            delivered.reference_excess_db.map(f64::to_bits),
            Some(reference.to_bits())
        );
        let (cut, stats) = delivered.solve(&[(reference, keyed(1, 1.0, 0.0))]);
        assert_eq!((cut[0], stats.cut_frames), (0.0, 0));
    }

    #[test]
    fn the_switch_off_keeps_the_key_and_the_input() {
        let input = signal();
        let mut key = vocal_key(&input, &input);
        let frames = key.keyed.len();
        key.keyed
            .extend((200..220).map(|frame| keyed(frame, 1.0, 0.0)));
        // The switch is off by default.
        let plan = Plan::from_key(key.clone(), crate::set::Options::default().deesser).unwrap();
        assert_eq!(plan.keyed, key.keyed);
        assert_eq!(plan.maximum_cut_db, 0.0);
        assert_eq!(
            Plan::from_key(key, true).unwrap().maximum_cut_db,
            MAXIMUM_CUT_DB
        );
        let mut output = input.clone();
        let stats = plan.apply(&mut output, 48000);
        // The keyed frames and their excess are measured for a request; no
        // frame is cut.
        assert_eq!(stats.keyed_frames, frames + 20);
        assert_eq!(stats.cut_frames, 0);
        assert!(stats.excess_quantiles_db[EXCESS_QUANTILES - 1] > 0.0);
        assert!(output == input);
    }

    #[test]
    fn the_plan_has_one_contract() {
        let plan: Plan = serde_json::from_str(
            r#"{"rule":"set.deesser.v4","keyed":[{"frame":1,"vocal_share":0.5,"other_power":0.0}],"other_band_power":1.0,"maximum_cut_db":3.0}"#,
        )
        .unwrap();
        assert_eq!((plan.threshold_db, plan.reference_excess_db), (0.0, None));
        let encoded = serde_json::to_string(&plan).unwrap();
        assert!(!encoded.contains("threshold") && !encoded.contains("reference"));
        plan.validate().unwrap();
        // A plan with `sibilant_frames` is refused, also beside `keyed`, a
        // keyed frame takes no other field, and the plan takes no median.
        for refused in [
            r#"{"rule":"set.deesser.v3","sibilant_frames":[1],"other_band_power":1.0,"maximum_cut_db":0.0}"#,
            r#"{"rule":"set.deesser.v4","keyed":[],"sibilant_frames":[1],"other_band_power":1.0,"maximum_cut_db":3.0}"#,
            r#"{"rule":"set.deesser.v4","keyed":[{"frame":1,"vocal_share":0.5,"other_power":0.0,"x":1}],"other_band_power":1.0,"maximum_cut_db":3.0}"#,
            r#"{"rule":"set.deesser.v4","keyed":[],"other_band_power":1.0,"maximum_cut_db":3.0,"median_excess_db":1.0}"#,
        ] {
            assert!(serde_json::from_str::<Plan>(refused).is_err(), "{refused}");
        }
        for bad in [
            vec![keyed(1, 0.0, 0.0)],
            vec![keyed(1, 1.5, 0.0)],
            vec![keyed(1, f64::NAN, 0.0)],
            vec![keyed(1, 0.5, -1.0)],
            vec![keyed(1, 0.5, f64::INFINITY)],
            vec![keyed(2, 0.5, 0.0), keyed(2, 0.5, 0.0)],
        ] {
            assert!(Plan {
                keyed: bad,
                ..plan.clone()
            }
            .validate()
            .is_err());
        }
        for bad in [
            Plan {
                reference_excess_db: Some(f64::NAN),
                ..plan.clone()
            },
            Plan {
                other_band_power: -1.0,
                ..plan.clone()
            },
            Plan {
                other_band_power: f64::INFINITY,
                ..plan.clone()
            },
            Plan {
                maximum_cut_db: 12.5,
                ..plan.clone()
            },
            Plan {
                threshold_db: -12.5,
                ..plan.clone()
            },
            Plan {
                threshold_db: 6.5,
                ..plan.clone()
            },
        ] {
            assert!(bad.validate().is_err(), "{bad:?}");
        }
        // A Sibilance step down takes the threshold down to 12 dB under the
        // reference excess, with a largest cut of up to 12 dB.
        Plan {
            threshold_db: THRESHOLD_RANGE_DB.0,
            maximum_cut_db: CUT_LIMIT_DB,
            ..plan.clone()
        }
        .validate()
        .unwrap();
    }

    #[test]
    fn a_frame_is_keyed_only_when_the_stem_carries_the_band() {
        let stem = signal();
        // The mix has the stem and a louder 7 kHz tone from another source.
        let mix: Vec<Stereo> = stem
            .iter()
            .enumerate()
            .map(|(i, f)| {
                let other = 3.0 * (2.0 * std::f64::consts::PI * 7300.0 * i as f64 / 48000.0).sin();
                [f[0] + other as f32, f[1] + other as f32]
            })
            .collect();
        assert!(!vocal_key(&stem, &stem).keyed.is_empty());
        assert!(vocal_key(&stem, &mix).keyed.is_empty());
    }

    #[test]
    fn a_frame_where_a_drum_burst_holds_the_band_is_left_unkeyed() {
        let vocal = signal();
        // A drum stem with a louder 7.3 kHz burst centred on frames 100 to
        // 104. The mix keeps the vocal's share of the band, so only the
        // largest-source condition leaves those frames out.
        let drum: Vec<Stereo> = (0..vocal.len())
            .map(|i| {
                let burst = (100 * masking::HOP + masking::FFT / 2
                    ..105 * masking::HOP + masking::FFT / 2)
                    .contains(&i);
                let x = if burst {
                    3.0 * (2.0 * std::f64::consts::PI * 7300.0 * i as f64 / 48000.0).sin()
                } else {
                    0.0
                };
                [x as f32; 2]
            })
            .collect();
        let silent = vec![[0.0f32; 2]; vocal.len()];
        let frames = |key: Key| key.keyed.iter().map(|k| k.frame).collect::<Vec<_>>();
        let alone = frames(vocal_key(&vocal, &vocal));
        let with_drum = key([&drum, &silent, &silent, &vocal], 48000, &vocal, 48000).unwrap();
        assert!(with_drum.other_band_power > 0.0);
        let burst = frames(with_drum);
        assert!(alone.iter().any(|f| (100..=104).contains(f)), "{alone:?}");
        assert!(!burst.iter().any(|f| (100..=104).contains(f)), "{burst:?}");
        assert!(burst.iter().any(|f| (106..=109).contains(f)), "{burst:?}");
    }

    #[test]
    fn the_residue_of_a_separation_keys_no_frame() {
        let stem = signal();
        // The same stem, 30 dB under a mix that another source fills.
        let mut state = 0x9e37_79b9u32;
        let mix: Vec<Stereo> = stem
            .iter()
            .map(|f| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                let n = 4.0 * ((state as f32 / u32::MAX as f32) - 0.5);
                [0.03 * f[0] + n, 0.03 * f[1] + n]
            })
            .collect();
        let residue: Vec<Stereo> = stem.iter().map(|f| [0.03 * f[0], 0.03 * f[1]]).collect();
        assert!(vocal_key(&residue, &mix).keyed.is_empty());
    }
}
