//! Shared, source-bound tonal treatment from aligned estimated roles.
//! The estimates supply measurements only; every treatment renders the original mix.
use crate::{
    analysis::percentile,
    audio_io::{Result, Stereo},
    dsp::{EqBand, Kind},
    finishing::Domain,
    treatment::{BlendPoint, FocusEnvelope, Tone},
};
use serde::Serialize;

pub const BANDS: usize = 29;
const HOP: usize = 4096;
/// Seconds of a reliability window.
const WINDOW_SECONDS: usize = 2;
const GAIN_DB: f64 = 1.5;
const MIN_SECONDS: f64 = 10.0;
const MIN_SHARE: f64 = 0.005;
const MIN_DELTA_DB: f64 = 0.25;
const ROLES: [&str; 4] = ["drums", "bass", "other", "vocals"];

#[derive(Clone)]
pub struct SpatialBands {
    pub mid: [f64; BANDS],
    pub side: [f64; BANDS],
}

impl SpatialBands {
    fn band(&self, index: usize) -> f64 {
        self.mid[index] + self.side[index]
    }

    fn total(&self) -> f64 {
        (0..BANDS).map(|index| self.band(index)).sum()
    }
}

#[derive(Clone, Serialize)]
pub struct Region {
    pub role: &'static str,
    pub domain: Domain,
    pub centre_hz: f64,
    pub q: f64,
    pub positive_seconds: f64,
    pub predicted_pair_ratio_gain_db: f64,
    pub score_second_db: f64,
    pub mean_wet_duty: f64,
    pub gain_db: f64,
}

#[derive(Clone, Serialize)]
pub struct Foundation {
    pub rule: &'static str,
    pub regions: Vec<Region>,
    pub tone: Tone,
    pub envelopes: Vec<FocusEnvelope>,
}

struct Candidate {
    region: Region,
    wet: Vec<f64>,
}

fn db(power: f64) -> f64 {
    10.0 * power.max(1e-30).log10()
}

fn ratio_gain(role: &SpatialBands, others: &[f64; 2], index: usize, domain: Domain) -> f64 {
    let rm = role.mid[index];
    let rs = role.side[index];
    let om = others[0];
    let os = others[1];
    let before = db((rm + rs) / (om + os).max(1e-30));
    let gain = 10f64.powf(3.0 / 10.0);
    let after = match domain {
        Domain::Mid => db((rm * gain + rs) / (om * gain + os).max(1e-30)),
        Domain::Side => db((rm + rs * gain) / (om + os * gain).max(1e-30)),
        Domain::Stereo => unreachable!("Foundation probes use mid or side"),
    };
    after - before
}

fn reliable_windows(source: &[Stereo], reconstructed: &[Stereo], window: usize) -> Vec<bool> {
    source
        .chunks(window)
        .zip(reconstructed.chunks(window))
        .map(|(a, b)| {
            let (mut xx, mut yy, mut xy, mut error) = (0.0, 0.0, 0.0, 0.0);
            for (x, y) in a.iter().flatten().zip(b.iter().flatten()) {
                let (x, y) = (*x as f64, *y as f64);
                xx += x * x;
                yy += y * y;
                xy += x * y;
                error += (x - y).powi(2);
            }
            xx / a.len().max(1) as f64 > 1e-10
                && db(error / xx.max(1e-30)) <= -18.0
                && xy / (xx * yy).sqrt().max(1e-30) >= 0.98
        })
        .collect()
}

/// The window of stem frame `index` at `stem_rate`.
fn bucket(index: usize, stem_rate: u32, windows: usize) -> usize {
    ((index as u128 * HOP as u128) / (stem_rate as u128 * WINDOW_SECONDS as u128))
        .min(windows.saturating_sub(1) as u128) as usize
}

fn smooth(values: &[f64]) -> Vec<f64> {
    (0..values.len())
        .map(|i| {
            0.25 * values[i.saturating_sub(1)]
                + 0.5 * values[i]
                + 0.25 * values[(i + 1).min(values.len() - 1)]
        })
        .collect()
}

/// The envelope of `wet`, one share for each window, over `frames` samples
/// at `rate`. Each share is at the centre of its window.
fn envelope(domain: Domain, index: usize, wet: &[f64], frames: usize, rate: u32) -> FocusEnvelope {
    let mut points = vec![BlendPoint {
        sample: 0,
        wet: wet[0],
    }];
    for (i, &amount) in wet.iter().enumerate() {
        let sample = ((2 * i + 1) * (WINDOW_SECONDS / 2) * rate as usize).min(frames - 2);
        if sample > points.last().unwrap().sample {
            points.push(BlendPoint {
                sample,
                wet: amount,
            });
        }
    }
    points.push(BlendPoint {
        sample: frames - 1,
        wet: *wet.last().unwrap(),
    });
    FocusEnvelope {
        domain,
        filter_index: index,
        points,
    }
}

/// Return no treatment when source/stem geometry is unusable or no role has a
/// sustained, locally reliable, bounded spatial opportunity. `spatial` holds
/// the frames of the stems at `stem_rate`; `source` and `reconstructed` are
/// at `rate`.
pub fn propose(
    spatial: &[Vec<SpatialBands>],
    source: &[Stereo],
    reconstructed: &[Stereo],
    stem_rate: u32,
    rate: u32,
) -> Result<Option<Foundation>> {
    if spatial.len() != 4
        || spatial[0].is_empty()
        || spatial
            .iter()
            .any(|frames| frames.len() != spatial[0].len())
        || source.len().abs_diff(reconstructed.len()) > 1
        || ![44100, 48000].contains(&stem_rate)
        || crate::audio_io::check_render_rate(rate).is_err()
    {
        return Err("Invalid all-role foundation geometry".into());
    }
    let window = WINDOW_SECONDS * rate as usize;
    if source.len() < window * 2 {
        return Ok(None);
    }
    let windows = source.len().div_ceil(window);
    let mut reliable = reliable_windows(source, reconstructed, window);
    reliable.resize(windows, false);
    let frame_count = spatial[0].len();
    let frame_bucket: Vec<_> = (0..frame_count)
        .map(|i| bucket(i, stem_rate, windows))
        .collect();
    let mut bucket_frames = vec![0usize; windows];
    for &b in &frame_bucket {
        bucket_frames[b] += 1;
    }
    let totals: Vec<[f64; 4]> = (0..frame_count)
        .map(|i| std::array::from_fn(|role| spatial[role][i].total()))
        .collect();
    let mut candidates = Vec::new();
    for role in 0..4 {
        let role_levels: Vec<_> = totals.iter().map(|row| row[role]).collect();
        let gate = percentile(&role_levels, 0.95) * 10f64.powf(-2.5);
        let active: Vec<_> = (0..frame_count)
            .map(|i| {
                let level = totals[i][role];
                level > gate && level > totals[i].iter().sum::<f64>() * 0.02 && level > 1e-20
            })
            .collect();
        let excitation: f64 = (0..frame_count)
            .filter(|&i| active[i])
            .map(|i| totals[i][role])
            .sum();
        if excitation <= 1e-20 {
            continue;
        }
        let shares: [f64; BANDS] = std::array::from_fn(|band| {
            (0..frame_count)
                .filter(|&i| active[i])
                .map(|i| spatial[role][i].band(band))
                .sum::<f64>()
                / excitation
        });
        let mut best: Option<Candidate> = None;
        for domain in [Domain::Mid, Domain::Side] {
            for first in 0..BANDS - 1 {
                let low = 25.0 * 2f64.powf(first as f64 / 3.0);
                let high = 25.0 * 2f64.powf((first + 2) as f64 / 3.0);
                let centre = (low * high).sqrt();
                if !(35.0..=18000.0).contains(&centre)
                    || (matches!(domain, Domain::Side) && centre < 200.0)
                    || shares[first] < MIN_SHARE
                    || shares[first + 1] < MIN_SHARE
                {
                    continue;
                }
                let mut deltas = Vec::new();
                let mut wet_counts = vec![0usize; windows];
                for i in 0..frame_count {
                    if !active[i] || !reliable[frame_bucket[i]] {
                        continue;
                    }
                    let mut pair_delta = f64::INFINITY;
                    let mut eligible = true;
                    for band in [first, first + 1] {
                        let r = &spatial[role][i];
                        let om: f64 = (0..4)
                            .filter(|&other| other != role)
                            .map(|other| spatial[other][i].mid[band])
                            .sum();
                        let os: f64 = (0..4)
                            .filter(|&other| other != role)
                            .map(|other| spatial[other][i].side[band])
                            .sum();
                        let delta = ratio_gain(r, &[om, os], band, domain);
                        if r.band(band) <= totals[i][role] * 0.001
                            || db(r.band(band) / (om + os).max(1e-30)) > 0.0
                            || delta < MIN_DELTA_DB
                        {
                            eligible = false;
                            break;
                        }
                        pair_delta = pair_delta.min(delta);
                    }
                    if eligible {
                        deltas.push(pair_delta);
                        wet_counts[frame_bucket[i]] += 1;
                    }
                }
                let seconds = deltas.len() as f64 * HOP as f64 / stem_rate as f64;
                if seconds < MIN_SECONDS {
                    continue;
                }
                let predicted = percentile(&deltas, 0.5);
                let score = seconds * predicted;
                let wet: Vec<_> = wet_counts
                    .iter()
                    .zip(&bucket_frames)
                    .map(|(&n, &d)| n as f64 / d.max(1) as f64)
                    .collect();
                let wet = smooth(&wet);
                let candidate = Candidate {
                    region: Region {
                        role: ROLES[role],
                        domain,
                        centre_hz: centre,
                        q: (centre / (high - low)).clamp(0.5, 2.0),
                        positive_seconds: seconds,
                        predicted_pair_ratio_gain_db: predicted,
                        score_second_db: score,
                        mean_wet_duty: wet.iter().sum::<f64>() / wet.len() as f64,
                        gain_db: GAIN_DB,
                    },
                    wet,
                };
                if best.as_ref().is_none_or(|current| {
                    score > current.region.score_second_db
                        || (score == current.region.score_second_db
                            && centre < current.region.centre_hz)
                }) {
                    best = Some(candidate);
                }
            }
        }
        if let Some(candidate) = best {
            candidates.push(candidate);
        }
    }
    candidates.sort_by(|a, b| {
        b.region
            .score_second_db
            .total_cmp(&a.region.score_second_db)
            .then_with(|| a.region.role.cmp(b.region.role))
    });
    let mut selected: Vec<Candidate> = Vec::new();
    for candidate in candidates {
        if selected.iter().any(|other| {
            std::mem::discriminant(&other.region.domain)
                == std::mem::discriminant(&candidate.region.domain)
                && (other.region.centre_hz / candidate.region.centre_hz)
                    .log2()
                    .abs()
                    < 0.5
        }) {
            continue;
        }
        selected.push(candidate);
    }
    if selected.is_empty() {
        return Ok(None);
    }
    let mut foundation = Foundation {
        rule: "set.all-role-foundation.v1",
        regions: Vec::new(),
        tone: Tone::default(),
        envelopes: Vec::new(),
    };
    for candidate in selected {
        let bands = match candidate.region.domain {
            Domain::Mid => &mut foundation.tone.mid,
            Domain::Side => &mut foundation.tone.side,
            Domain::Stereo => unreachable!(),
        };
        let index = bands.len();
        bands.push(EqBand {
            kind: Kind::Peak,
            hz: candidate.region.centre_hz,
            q: candidate.region.q,
            gain_db: GAIN_DB,
        });
        foundation.envelopes.push(envelope(
            candidate.region.domain,
            index,
            &candidate.wet,
            source.len(),
            rate,
        ));
        foundation.regions.push(candidate.region);
    }
    foundation.tone.validate(rate)?;
    for envelope in &foundation.envelopes {
        envelope.validate(&foundation.tone, source.len())?;
    }
    Ok(Some(foundation))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_vocal_mid_opportunity_produces_continuous_source_bound_envelope() {
        let frames = 12 * 48000;
        let source = vec![[0.01, 0.01]; frames];
        let count = frames / HOP;
        let mut spatial = vec![
            vec![
                SpatialBands {
                    mid: [0.0; BANDS],
                    side: [0.0; BANDS],
                };
                count
            ];
            4
        ];
        let (first_roles, last_roles) = spatial.split_at_mut(2);
        for (bass, other) in first_roles[1].iter_mut().zip(&mut last_roles[0]) {
            for band in [10, 11] {
                bass.mid[band] = 1.0;
                other.side[band] = 4.0;
            }
        }
        let foundation = propose(&spatial, &source, &source, 48000, 48000)
            .unwrap()
            .unwrap();
        assert_eq!(foundation.regions.len(), 1);
        assert_eq!(foundation.regions[0].role, "bass");
        assert!(matches!(foundation.regions[0].domain, Domain::Mid));
        assert_eq!(foundation.tone.mid.len(), 1);
        assert!(foundation.tone.side.is_empty());
        assert_eq!(foundation.envelopes.len(), 1);
        assert_eq!(foundation.envelopes[0].points[0].sample, 0);
        assert_eq!(
            foundation.envelopes[0].points.last().unwrap().sample,
            frames - 1
        );
        assert!(foundation.envelopes[0].points.iter().all(|p| p.wet > 0.0));
    }

    #[test]
    fn unreliable_source_windows_keep_tone_unchanged() {
        let frames = 12 * 48000;
        let source = vec![[0.0, 0.0]; frames];
        let spatial = vec![
            vec![
                SpatialBands {
                    mid: [0.0; BANDS],
                    side: [0.0; BANDS],
                };
                frames / HOP
            ];
            4
        ];
        assert!(propose(&spatial, &source, &source, 48000, 48000)
            .unwrap()
            .is_none());
    }

    #[test]
    fn short_aligned_source_keeps_tone_unchanged() {
        let source = vec![[0.01, 0.01]; 3 * 48000];
        let spatial = vec![
            vec![
                SpatialBands {
                    mid: [0.0; BANDS],
                    side: [0.0; BANDS],
                };
                source.len() / HOP
            ];
            4
        ];
        assert!(propose(&spatial, &source, &source, 48000, 48000)
            .unwrap()
            .is_none());
    }

    #[test]
    fn separate_roles_and_bands_follow_their_own_passages() {
        let frames = 24 * 48000;
        let source = vec![[0.01, 0.01]; frames];
        let count = frames / HOP;
        let mut spatial = vec![
            vec![
                SpatialBands {
                    mid: [0.0; BANDS],
                    side: [0.0; BANDS],
                };
                count
            ];
            4
        ];
        let (first_roles, last_roles) = spatial.split_at_mut(2);
        let (other_roles, vocal_roles) = last_roles.split_at_mut(1);
        for (i, ((bass, other), vocal)) in first_roles[1]
            .iter_mut()
            .zip(&mut other_roles[0])
            .zip(&mut vocal_roles[0])
            .enumerate()
        {
            let (target, bands) = if i < count / 2 {
                (bass, [10, 11])
            } else {
                (vocal, [18, 19])
            };
            for band in bands {
                target.mid[band] = 1.0;
                other.side[band] = 4.0;
            }
        }
        let foundation = propose(&spatial, &source, &source, 48000, 48000)
            .unwrap()
            .unwrap();
        assert_eq!(foundation.regions.len(), 2);
        let bass = foundation
            .regions
            .iter()
            .position(|r| r.role == "bass")
            .unwrap();
        let vocals = foundation
            .regions
            .iter()
            .position(|r| r.role == "vocals")
            .unwrap();
        let wet_at = |region: usize, sample: usize| {
            foundation.envelopes[region]
                .points
                .iter()
                .min_by_key(|point| point.sample.abs_diff(sample))
                .unwrap()
                .wet
        };
        assert!(wet_at(bass, 4 * 48000) > 0.9);
        assert!(wet_at(bass, 20 * 48000) < 0.1);
        assert!(wet_at(vocals, 4 * 48000) < 0.1);
        assert!(wet_at(vocals, 20 * 48000) > 0.9);
    }
}
