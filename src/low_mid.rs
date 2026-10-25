//! Low-mid correction keyed by the stems (`set.low-mid.v1`).
//!
//! The other stems mask the lead stem in the low mids in some frames and not
//! in others. In each frame, the masking model (`masking::thresholds`) gives
//! the threshold that the other stems set from 250 to 500 Hz. When the lead
//! is active and its power in that region is under the threshold, the frame
//! is keyed, and the masked depth (threshold over lead power, in dB) sets the
//! cut of that frame, up to the measured static cut. Frames where the lead
//! is silent or audible above the others keep the low mids of the mix. The
//! band has a static gain of 0 dB: an envelope blends it in.
use crate::{
    dsp::{EqBand, Kind},
    finishing::Domain,
    masking,
    treatment::{BlendPoint, EnvelopedBand},
};
use serde::Serialize;

/// Region of the correction, in Hz.
pub const REGION_HZ: (f64, f64) = (250.0, 500.0);
/// Smallest Q of the correction band.
pub const MINIMUM_Q: f64 = 1.4;
/// Lowest frequency of the lower −3 dB point of the band-pass with the
/// band's centre and Q, in Hz: the correction stays above the bass.
pub const LOWER_EDGE_HZ: f64 = 200.0;
/// The lead is active in a frame when its power is within this of its 95th
/// percentile frame power, as `stem_evidence` counts role activity.
pub const ACTIVE_DB: f64 = 25.0;
/// Fewer keyed frames give no correction: 20 frames are 0.43 s at 48 kHz.
pub const MINIMUM_FRAMES: usize = 20;
/// Attack and release of the cut, in seconds. The cut follows the masking
/// of phrases, not of single notes.
pub const ATTACK_SECONDS: f64 = 0.1;
pub const RELEASE_SECONDS: f64 = 0.5;
/// Envelope points are this many masking frames apart (85 ms at 48 kHz).
const POINT_FRAMES: usize = 4;

/// The masking of the lead stem in the low mids, frame by frame.
#[derive(Clone, Serialize)]
pub struct Key {
    pub rule: &'static str,
    /// Index of the lead role in `stem_evidence::Evidence::roles`.
    pub lead_role: usize,
    pub active_frames: usize,
    pub keyed_frames: usize,
    /// Mean masked depth of the lead over the keyed frames, per masking
    /// band of the region: centre in Hz and depth in dB.
    pub excess_db: Vec<(f64, f64)>,
    /// Centre of the correction: the log-frequency centroid of the positive
    /// mean depths, in Hz.
    pub centre_hz: f64,
    /// Masked depth of each masking frame in dB, 0 when not keyed.
    #[serde(skip)]
    pub depth_db: Vec<f32>,
}

fn region() -> Vec<usize> {
    (0..masking::BANDS)
        .filter(|&z| (REGION_HZ.0..REGION_HZ.1).contains(&masking::centre_hz(z)))
        .collect()
}

/// Key the frames of `lead` (band powers of the lead stem, both channels,
/// at `rate`) against `others` (the summed band powers of the other stems),
/// with the spectral flatness of the mix for the tonality of the masker.
pub fn key(
    lead: &[[f64; masking::BANDS]],
    others: &[[f64; masking::BANDS]],
    flatness_db: &[f64],
    lead_role: usize,
    rate: u32,
) -> Option<Key> {
    let frames = lead.len().min(others.len()).min(flatness_db.len());
    if frames == 0 {
        return None;
    }
    let threshold = masking::thresholds(
        &masking::Frames {
            powers: others[..frames].to_vec(),
            flatness_db: flatness_db[..frames].to_vec(),
        },
        masking::Channels::Both,
        rate,
    );
    let total: Vec<f64> = lead[..frames].iter().map(|f| f.iter().sum()).collect();
    let mut sorted = total.clone();
    sorted.sort_by(f64::total_cmp);
    let p95 = sorted[((sorted.len() - 1) as f64 * 0.95).round() as usize];
    let gate = p95 * 10f64.powf(-ACTIVE_DB / 10.0);
    let bands = region();
    let mut depth_db = vec![0.0f32; frames];
    let mut sums = vec![0.0f64; bands.len()];
    let (mut active_frames, mut keyed_frames) = (0, 0);
    for t in 0..frames {
        if !(total[t] > gate && total[t] > 1e-20) {
            continue;
        }
        active_frames += 1;
        let (masked, own): (f64, f64) = bands
            .iter()
            .map(|&z| (threshold[t][z], lead[t][z]))
            .fold((0.0, 0.0), |(a, b), (m, l)| (a + m, b + l));
        let depth = 10.0 * (masked / own.max(1e-30)).log10();
        if depth > 0.0 {
            keyed_frames += 1;
            depth_db[t] = depth as f32;
            for (sum, &z) in sums.iter_mut().zip(&bands) {
                *sum += 10.0 * (threshold[t][z] / lead[t][z].max(1e-30)).log10();
            }
        }
    }
    if keyed_frames < MINIMUM_FRAMES {
        return None;
    }
    let excess_db: Vec<(f64, f64)> = bands
        .iter()
        .zip(&sums)
        .map(|(&z, s)| (masking::centre_hz(z), s / keyed_frames as f64))
        .collect();
    let (num, den) = excess_db.iter().fold((0.0, 0.0), |(n, d), &(hz, e)| {
        (n + e.max(0.0) * hz.log2(), d + e.max(0.0))
    });
    let centre_hz = if den > 0.0 {
        2f64.powf(num / den)
    } else {
        (REGION_HZ.0 * REGION_HZ.1).sqrt()
    }
    .clamp(REGION_HZ.0, REGION_HZ.1);
    Some(Key {
        rule: "set.low-mid.v1",
        lead_role,
        active_frames,
        keyed_frames,
        excess_db,
        centre_hz,
        depth_db,
    })
}

/// Q of the correction at `centre_hz`: `MINIMUM_Q`, or more where the lower
/// −3 dB point of that Q falls under `LOWER_EDGE_HZ`.
pub fn q(centre_hz: f64) -> f64 {
    // The lower −3 dB point of a band-pass is f0 (sqrt(1 + x²) − x) with
    // x = 1 / 2Q; it equals the edge at Q = r / (1 − r²), r = edge / f0.
    let r = (LOWER_EDGE_HZ / centre_hz).min(0.95);
    (r / (1.0 - r * r)).max(MINIMUM_Q)
}

/// Lower −3 dB point of the band-pass with `hz` and `q`.
pub fn lower_edge_hz(hz: f64, q: f64) -> f64 {
    let x = 0.5 / q;
    hz * ((1.0 + x * x).sqrt() - x)
}

impl Key {
    /// The correction band and its envelope over `frames` samples at
    /// `rate`. The wet share of a frame is its cut over `maximum_cut_db`,
    /// smoothed by the attack and the release.
    pub fn correction(
        &self,
        maximum_cut_db: f64,
        frames: usize,
        rate: u32,
    ) -> Option<EnvelopedBand> {
        if maximum_cut_db <= 0.0 || frames < 2 {
            return None;
        }
        let hop = masking::HOP as f64 / rate as f64;
        let (attack, release) = (
            (-hop / ATTACK_SECONDS).exp(),
            (-hop / RELEASE_SECONDS).exp(),
        );
        let mut level = 0.0f64;
        let wet: Vec<f64> = self
            .depth_db
            .iter()
            .map(|&d| {
                let target = (d as f64).min(maximum_cut_db) / maximum_cut_db;
                let k = if target > level { attack } else { release };
                level = k * level + (1.0 - k) * target;
                level
            })
            .collect();
        let centre = |t: usize| t * masking::HOP + masking::FFT / 2;
        let mut points = vec![BlendPoint {
            sample: 0,
            wet: wet.first().copied().unwrap_or(0.0),
        }];
        for t in (0..wet.len()).step_by(POINT_FRAMES) {
            let sample = centre(t);
            if sample > 0 && sample < frames - 1 {
                points.push(BlendPoint {
                    sample,
                    wet: (wet[t] * 1000.0).round() / 1000.0,
                });
            }
        }
        points.push(BlendPoint {
            sample: frames - 1,
            wet: points.last().unwrap().wet,
        });
        // A point between two equal neighbours adds nothing.
        let mut kept: Vec<BlendPoint> = Vec::with_capacity(points.len());
        for i in 0..points.len() {
            let interior = i > 0 && i + 1 < points.len();
            if interior && points[i].wet == points[i - 1].wet && points[i].wet == points[i + 1].wet
            {
                continue;
            }
            kept.push(points[i].clone());
        }
        let hz = self.centre_hz;
        Some(EnvelopedBand {
            rule: self.rule.into(),
            domain: Domain::Mid,
            band: EqBand {
                kind: Kind::Peak,
                hz,
                q: q(hz),
                gain_db: -maximum_cut_db,
            },
            points: kept,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_band_stays_above_the_bass() {
        for hz in [250.0, 284.0, 350.0, 500.0] {
            let q = q(hz);
            assert!(q >= MINIMUM_Q);
            assert!(lower_edge_hz(hz, q) >= LOWER_EDGE_HZ - 1e-6, "{hz} {q}");
        }
        assert_eq!(q(400.0), MINIMUM_Q);
    }

    #[test]
    fn only_frames_where_the_others_mask_the_lead_are_keyed() {
        let bands = region();
        let frame = |level: f64| {
            let mut f = [1e-12; masking::BANDS];
            for &z in &bands {
                f[z] = level;
            }
            f
        };
        // The lead is active in each frame, about 30 dB under full scale in
        // each band. The others are 20 dB above it in the first 40 frames
        // and silent in the next 160.
        let lead: Vec<_> = (0..200).map(|_| frame(1e3)).collect();
        let others: Vec<_> = (0..200)
            .map(|t| if t < 40 { frame(1e5) } else { frame(1e-12) })
            .collect();
        let key = key(&lead, &others, &vec![0.0; 200], 3, 48000).unwrap();
        assert_eq!(key.active_frames, 200);
        assert!(key.keyed_frames >= 40 && key.keyed_frames < 46);
        assert!(key.depth_db[..40].iter().all(|&d| d > 0.0));
        assert!(key.depth_db[50..].iter().all(|&d| d == 0.0));
        let band = key.correction(1.6, 200 * masking::HOP, 48000).unwrap();
        assert_eq!(band.band.gain_db, -1.6);
        // After the attack, the band is fully in.
        assert!(band.mean_wet(15 * masking::HOP, 38 * masking::HOP) > 0.9);
        // Six release time constants after the masking ends, it is out.
        assert!(band.mean_wet(180 * masking::HOP, 200 * masking::HOP) < 0.01);
    }
}
