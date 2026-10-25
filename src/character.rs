//! The character of each version of a set (`set.version-character.v1`,
//! D206). Each version moves five tone regions of the mix by its own amount
//! and widens each stereo band by its own amount. The amounts come from the
//! single: the master of the album pairs of D177 that moves the mix most. `master`
//! takes the moves of the single, and the other versions sit around it.
//! The gain of each filter is solved so that its predicted region move on
//! the spectrum of the mix equals the wanted move. A move stops where the
//! mix reaches the bound of its region.
use crate::{
    broad_tone,
    dsp::{EqBand, Kind},
    treatment::Tone,
    width::SideGain,
    Spectrum,
};
use serde::Serialize;

pub const RULE: &str = "set.version-character.v1";

/// The regions of the measures of the album pairs, in Hz.
pub const SUB_HZ: (f64, f64) = crate::open::SUB_HZ;
pub const BASS_HZ: (f64, f64) = crate::open::BASS_HZ;
pub const LOW_MID_HZ: (f64, f64) = (250.0, 500.0);
pub const BODY_HZ: (f64, f64) = crate::open::BODY_HZ;
pub const PRESENCE_HZ: (f64, f64) = broad_tone::PRESENCE_HZ;
pub const AIR_HZ: (f64, f64) = broad_tone::AIR_HZ;

/// The moves of one version from the mix, in dB: low mid against body,
/// presence against body, air against presence, sub against bass and bass
/// against body, and the side gain of each band of
/// `stereo::BAND_EDGES_HZ`.
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Character {
    pub low_mid_db: f64,
    pub presence_db: f64,
    pub air_db: f64,
    pub sub_db: f64,
    pub bass_db: f64,
    pub width_db: [f64; 6],
    /// True when the low mid and the presence move toward the median outcome
    /// of the pairs by the size of their moves, as the album masters of D177 move
    /// them up on one mix and down on another. False when the version keeps
    /// the directions of its moves.
    pub follow_mix: bool,
}

/// The median outcome of the album pairs of D177, in dB: the direction of a
/// move that follows the mix.
pub const LOW_MID_MEDIAN_DB: f64 = 2.75;
pub const PRESENCE_MEDIAN_DB: f64 = -4.83;

/// The side gain of each band of the single against its mix, in dB.
pub const SINGLE_WIDTH_DB: [f64; 6] = [3.5, 3.6, 4.0, 4.4, 3.5, 2.1];

/// `master` (Full): the moves of the single master.
pub const FULL: Character = Character {
    low_mid_db: -2.1,
    presence_db: 1.3,
    air_db: -2.3,
    sub_db: 1.6,
    bass_db: -1.2,
    width_db: SINGLE_WIDTH_DB,
    follow_mix: true,
};
/// `essential`: half the tone moves of the single, the sub of the single
/// and three quarters of its width.
pub const ESSENTIAL: Character = Character {
    low_mid_db: -1.0,
    presence_db: 0.8,
    air_db: -1.0,
    sub_db: 1.6,
    bass_db: 0.5,
    width_db: [2.6, 2.7, 3.0, 3.3, 2.6, 1.6],
    follow_mix: true,
};
/// `open` (Warm): more low mid and bass, less air, the width of the single.
pub const WARM: Character = Character {
    low_mid_db: 1.0,
    presence_db: -0.3,
    air_db: -2.5,
    sub_db: 2.0,
    bass_db: 2.5,
    width_db: SINGLE_WIDTH_DB,
    follow_mix: false,
};
/// `bold` (Wide): the tone of Full and 1 dB more width than the single in
/// each band. The set renders it from the chosen render of Full, with the
/// presence and treble steps of `broad_tone::vivid_tone`.
pub const WIDE: Character = Character {
    low_mid_db: -2.1,
    presence_db: 1.3,
    air_db: -2.3,
    sub_db: 1.6,
    bass_db: -1.2,
    width_db: [4.5, 4.6, 5.0, 5.4, 4.5, 3.1],
    follow_mix: true,
};

/// The character of the version `id`.
pub fn of(id: &str) -> Option<Character> {
    match id {
        "essential" => Some(ESSENTIAL),
        "master" => Some(FULL),
        "open" => Some(WARM),
        "bold" => Some(WIDE),
        _ => None,
    }
}

/// Bounds of the outcome of each region, in dB. A move stops where the mix
/// reaches the bound in its direction.
/// - Presence against body: the equal-loudness balance of D134 above, the
///   darkest album master under (`broad_tone`).
/// - Sub against bass and bass against body: the largest outcome of the
///   pairs above (`open`).
/// - Low mid against body: the P25 outcome of the album pairs under, so a
///   lean mix does not get thinner; the P75 above.
/// - Air against presence: the P25 outcome of the album pairs under.
pub const PRESENCE_MAXIMUM_DB: f64 = -3.30;
pub const PRESENCE_MINIMUM_DB: f64 = -9.10;
pub const LOW_MID_MINIMUM_DB: f64 = 1.17;
pub const LOW_MID_MAXIMUM_DB: f64 = 3.45;
pub const AIR_MINIMUM_DB: f64 = -7.38;
/// Largest gain of a filter, in dB.
pub const FILTER_MAXIMUM_DB: f64 = 6.0;

/// Corner or centre of each filter, in Hz.
pub const LOW_MID_BELL_HZ: f64 = 354.0;
pub const LOW_MID_BELL_Q: f64 = 1.4;

/// The five region measures of a spectrum, in dB.
#[derive(Clone, Copy, Debug, Default, Serialize)]
pub struct Regions {
    pub low_mid_db: f64,
    pub presence_db: f64,
    pub air_db: f64,
    pub sub_db: f64,
    pub bass_db: f64,
}

impl Regions {
    fn get(&self, i: usize) -> f64 {
        [
            self.low_mid_db,
            self.presence_db,
            self.air_db,
            self.sub_db,
            self.bass_db,
        ][i]
    }
    fn from(values: [f64; 5]) -> Self {
        Regions {
            low_mid_db: values[0],
            presence_db: values[1],
            air_db: values[2],
            sub_db: values[3],
            bass_db: values[4],
        }
    }
}

/// The five regions of `spectrum` under the stereo filters `stereo` and the
/// widening `side`.
pub fn regions(
    spectrum: &Spectrum,
    stereo: &[EqBand],
    mid: &[EqBand],
    side: Option<&SideGain>,
) -> Regions {
    let ratio = |a, b| broad_tone::shaped_ratio_db(spectrum, stereo, mid, side, a, b);
    Regions {
        low_mid_db: ratio(LOW_MID_HZ, BODY_HZ),
        presence_db: ratio(PRESENCE_HZ, BODY_HZ),
        air_db: ratio(AIR_HZ, PRESENCE_HZ),
        sub_db: ratio(SUB_HZ, BASS_HZ),
        bass_db: ratio(BASS_HZ, BODY_HZ),
    }
}

/// The tone plan of one version.
#[derive(Clone, Serialize)]
pub struct Plan {
    pub rule: &'static str,
    pub character: Character,
    /// The regions of the mix.
    pub mix: Regions,
    /// The wanted moves after the bounds of the outcome.
    pub wanted: Regions,
    /// The moves that the solved filters predict.
    pub predicted: Regions,
    /// Scale of the solved gains: under 1 when the tone at full scale
    /// exceeds the response bound of `Tone::validate`.
    pub scale: f64,
    pub tone: Tone,
}

/// The filter of measure `i` at `gain_db`: the low-mid bell, the presence
/// bell of `broad_tone`, the air shelf of `broad_tone`, and the sub and bass
/// shelves of `open`, which the sub lift solves again (`set.sub-lift.v1`).
fn filter(i: usize, gain_db: f64) -> EqBand {
    match i {
        0 => EqBand {
            kind: Kind::Peak,
            hz: LOW_MID_BELL_HZ,
            q: LOW_MID_BELL_Q,
            gain_db,
        },
        1 => EqBand {
            kind: Kind::Peak,
            hz: broad_tone::PRESENCE_BELL_HZ,
            q: broad_tone::LEAD_BELL_Q,
            gain_db,
        },
        2 => broad_tone::air_shelf(gain_db),
        3 => crate::open::shelf(crate::open::SUB_SHELF_HZ, gain_db),
        _ => crate::open::shelf(crate::open::BASS_SHELF_HZ, gain_db),
    }
}

/// The wanted move of `want` from `now` within `[low, high]`: the move
/// stops at the bound in its direction, and never reverses.
fn bounded(want: f64, now: f64, low: f64, high: f64) -> f64 {
    if want >= 0.0 {
        want.min(high - now).max(0.0)
    } else {
        want.max(low - now).min(0.0)
    }
}

/// The tone that moves the regions of `spectrum` by `character`. The moves
/// are from the mix; the tone reaches them on top of `base`, the stem focus
/// of the version, with the widening `side`, as the pairs measure a master
/// against its mix. The five gains are solved in turns: each filter takes
/// the wanted move of its region less what the other filters give it.
pub fn plan(
    spectrum: &Spectrum,
    character: Character,
    base: &Tone,
    side: Option<&SideGain>,
) -> Plan {
    let mix = regions(spectrum, &[], &[], None);
    // A move that follows the mix points toward the median outcome.
    let toward = |size: f64, now: f64, median: f64| {
        if !character.follow_mix {
            size
        } else if now < median {
            size.abs()
        } else {
            -size.abs()
        }
    };
    // A single master of D206 cuts the low mid of a mix above the median and
    // leaves a mix under it: a low-mid move that follows the mix only cuts.
    let low_mid = toward(character.low_mid_db, mix.low_mid_db, LOW_MID_MEDIAN_DB);
    let low_mid = if character.follow_mix {
        low_mid.min(0.0)
    } else {
        low_mid
    };
    let presence = toward(character.presence_db, mix.presence_db, PRESENCE_MEDIAN_DB);
    let wanted = Regions {
        low_mid_db: bounded(
            low_mid,
            mix.low_mid_db,
            LOW_MID_MINIMUM_DB,
            LOW_MID_MAXIMUM_DB,
        ),
        presence_db: bounded(
            presence,
            mix.presence_db,
            PRESENCE_MINIMUM_DB,
            PRESENCE_MAXIMUM_DB,
        ),
        air_db: bounded(character.air_db, mix.air_db, AIR_MINIMUM_DB, f64::INFINITY),
        sub_db: bounded(
            character.sub_db,
            mix.sub_db,
            f64::NEG_INFINITY,
            crate::open::SUB_MINUS_BASS_MAXIMUM_DB,
        ),
        bass_db: bounded(
            character.bass_db,
            mix.bass_db,
            f64::NEG_INFINITY,
            crate::open::BASS_MINUS_BODY_MAXIMUM_DB,
        ),
    };
    let mut gains = [0.0f64; 5];
    let bands = |gains: &[f64; 5]| -> Vec<EqBand> {
        base.stereo
            .iter()
            .copied()
            .chain(
                (0..5)
                    .filter(|&i| gains[i].abs() > 1e-3)
                    .map(|i| filter(i, gains[i])),
            )
            .collect()
    };
    for _ in 0..8 {
        for i in 0..5 {
            let target = mix.get(i) + wanted.get(i);
            let measure = |g: f64| {
                let mut trial = gains;
                trial[i] = g;
                regions(spectrum, &bands(&trial), &base.mid, side).get(i)
            };
            // Each region rises with the gain of its own filter.
            let (mut low, mut high) = (-FILTER_MAXIMUM_DB, FILTER_MAXIMUM_DB);
            gains[i] = if measure(low) >= target {
                low
            } else if measure(high) <= target {
                high
            } else {
                for _ in 0..30 {
                    let middle = 0.5 * (low + high);
                    if measure(middle) < target {
                        low = middle;
                    } else {
                        high = middle;
                    }
                }
                0.5 * (low + high)
            };
        }
    }
    // Shelves and bells that add up past the response bound of a tone take
    // a common scale, in steps of 10%.
    let mut scale = 1.0;
    let tone = loop {
        let scaled: [f64; 5] = std::array::from_fn(|i| gains[i] * scale);
        let tone = Tone {
            stereo: bands(&scaled),
            ..base.clone()
        };
        if tone.validate(spectrum.sample_rate).is_ok() || scale < 0.1 {
            break tone;
        }
        scale *= 0.9;
    };
    let after = regions(spectrum, &tone.stereo, &base.mid, side);
    let predicted = Regions::from(std::array::from_fn(|i| after.get(i) - mix.get(i)));
    Plan {
        rule: RULE,
        character,
        mix,
        wanted,
        predicted,
        scale,
        tone,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A spectrum with a falling slope, as a mix has, and a small side.
    fn spectrum() -> Spectrum {
        let centers: Vec<f64> = (0..240)
            .map(|i| 20.0 * 1000f64.powf(i as f64 / 239.0))
            .collect();
        let power: Vec<f64> = centers
            .iter()
            .map(|&hz| 10f64.powf(-3.0 * (hz / 100.0).log2() / 10.0))
            .collect();
        Spectrum {
            sample_rate: 48000,
            centers_hz: centers,
            mid_power: power.clone(),
            side_power: power.iter().map(|p| p * 0.03).collect(),
            band_power: power,
            mel_power: Vec::new(),
            analyzed_frames: 1,
        }
    }

    #[test]
    fn each_version_reaches_its_moves() {
        let spectrum = spectrum();
        for (id, character) in [
            ("essential", ESSENTIAL),
            ("master", FULL),
            ("open", WARM),
            ("bold", WIDE),
        ] {
            let plan = plan(&spectrum, character, &Tone::default(), None);
            for i in 0..5 {
                assert!(
                    (plan.predicted.get(i) - plan.wanted.get(i)).abs() < 0.05,
                    "{id} region {i}: predicted {:?} wanted {:?}",
                    plan.predicted,
                    plan.wanted
                );
            }
        }
    }

    #[test]
    fn a_move_stops_at_the_bound_of_its_region() {
        assert_eq!(
            bounded(1.3, -3.5, -9.1, -3.3),
            0.2f64.clamp(-3.3 + 3.5, 1.3)
        );
        assert_eq!(bounded(1.3, -3.0, -9.1, -3.3), 0.0);
        assert_eq!(bounded(-2.1, 2.0, 1.17, 3.45), 1.17 - 2.0);
        assert_eq!(bounded(-2.1, 1.0, 1.17, 3.45), 0.0);
    }

    #[test]
    fn the_versions_differ_by_more_than_a_duplicate() {
        let spectrum = spectrum();
        let plans: Vec<Plan> = [ESSENTIAL, FULL, WARM, WIDE]
            .into_iter()
            .map(|c| plan(&spectrum, c, &Tone::default(), None))
            .collect();
        for (a, pa) in plans.iter().enumerate() {
            for pb in plans.iter().skip(a + 1) {
                let largest = (0..5)
                    .map(|i| (pa.predicted.get(i) - pb.predicted.get(i)).abs())
                    .fold(0.0, f64::max);
                let width = (0..6)
                    .map(|i| (pa.character.width_db[i] - pb.character.width_db[i]).abs())
                    .fold(0.0, f64::max);
                assert!(largest.max(width) >= 1.0, "{largest} {width}");
            }
        }
    }
}
