//! The Open version: the low end and the width of the reference album
//! masters of D177 (`set.open.v3`, D178). The tone lifts the sub against the bass and
//! the bass against the body by the median move of the eight mix/master
//! pairs, each lift bounded where the mix already reaches the largest
//! outcome of the pairs. The gain of each shelf is solved so that its
//! predicted region move on the spectrum of the mix equals the wanted move.
//! The width is the width pattern of the version
//! (`character::Character::width_db`) under each band's allowance and the
//! pairs' largest outcome (`width::widening`), and the target is the base
//! target, which sits at the level of the album masters.
use crate::{
    dsp::{EqBand, Kind},
    treatment::Tone,
    Spectrum,
};
use serde::Serialize;

pub const RULE: &str = "set.open.v3";
/// The regions of the low end that the pairs measure, in Hz.
pub const SUB_HZ: (f64, f64) = (30.0, 80.0);
pub const BASS_HZ: (f64, f64) = (80.0, 250.0);
pub const BODY_HZ: (f64, f64) = (500.0, 2000.0);
/// Median move of the sub against the bass over the eight album masters,
/// in dB.
pub const SUB_MOVE_DB: f64 = 1.4;
/// Median move of the bass against the body over the eight album masters.
pub const BASS_MOVE_DB: f64 = 1.2;
/// Largest outcome of the pairs: a lift stops where the mix already
/// reaches it.
pub const SUB_MINUS_BASS_MAXIMUM_DB: f64 = 5.74;
pub const BASS_MINUS_BODY_MAXIMUM_DB: f64 = 9.67;
/// Largest gain of a shelf, in dB.
pub const SHELF_MAXIMUM_DB: f64 = 4.0;
/// Corner of the low shelf that lifts the sub against the bass.
pub const SUB_SHELF_HZ: f64 = 90.0;
/// Corner of the low shelf that lifts the bass against the body.
pub const BASS_SHELF_HZ: f64 = 300.0;

/// The low-end plan of the Open version of a mix.
#[derive(Clone, Serialize)]
pub struct Open {
    pub rule: &'static str,
    /// The regions of the mix, in dB.
    pub sub_minus_bass_db: f64,
    pub bass_minus_body_db: f64,
    /// The gains of the shelves, in dB.
    pub sub_db: f64,
    pub bass_db: f64,
    /// The predicted moves of the regions under the tone, in dB.
    pub predicted_sub_minus_bass_db: f64,
    pub predicted_bass_minus_body_db: f64,
    pub tone: Tone,
}

/// Mean band power of the spectrum over the bands whose centres fall in
/// `[low, high)`, with the response of `bands` applied, in dB.
fn region_db(spectrum: &Spectrum, bands: &[EqBand], (low, high): (f64, f64)) -> f64 {
    let rate = spectrum.sample_rate;
    let values: Vec<f64> = spectrum
        .centers_hz
        .iter()
        .zip(&spectrum.band_power)
        .filter(|(c, _)| (low..high).contains(c))
        .map(|(c, p)| {
            let response: f64 = bands
                .iter()
                .map(|b| b.filter(rate).response_db(*c, rate))
                .sum();
            p * 10f64.powf(response / 10.0)
        })
        .collect();
    if values.is_empty() {
        return 0.0;
    }
    10.0 * (values.iter().sum::<f64>() / values.len() as f64 + 1e-30).log10()
}

/// The sub against the bass and the bass against the body of the spectrum
/// under `bands`, in dB.
pub(crate) fn regions_db(spectrum: &Spectrum, bands: &[EqBand]) -> (f64, f64) {
    let sub = region_db(spectrum, bands, SUB_HZ);
    let bass = region_db(spectrum, bands, BASS_HZ);
    let body = region_db(spectrum, bands, BODY_HZ);
    (sub - bass, bass - body)
}

/// Rule of the second render of a version with bass shelves.
pub const LIFT_RULE: &str = "set.sub-lift.v1";
/// Smallest shortfall of a region that takes the lift, in dB: the dead zone
/// of the restoration (`treatment_render::RESTORATION_DEAD_ZONE_DB`).
pub const LIFT_MINIMUM_DB: f64 = 0.1;

/// The sub against the bass and the bass against the body of the mid power
/// of `spectrum` under the response `response_db` at each centre, in dB.
/// The mid holds the low end that the compressor and peak control take; a
/// side gain of a version leaves it.
pub fn mid_regions_db(spectrum: &Spectrum, response_db: impl Fn(f64) -> f64) -> (f64, f64) {
    let region = |(low, high): (f64, f64)| {
        let values: Vec<f64> = spectrum
            .centers_hz
            .iter()
            .zip(&spectrum.mid_power)
            .filter(|(c, _)| (low..high).contains(*c))
            .map(|(c, p)| p * 10f64.powf(response_db(*c) / 10.0))
            .collect();
        if values.is_empty() {
            return 0.0;
        }
        10.0 * (values.iter().sum::<f64>() / values.len() as f64 + 1e-30).log10()
    };
    let (sub, bass, body) = (region(SUB_HZ), region(BASS_HZ), region(BODY_HZ));
    (sub - bass, bass - body)
}

/// The stereo bands `stereo` of a version with the bass shelves solved
/// again (`set.sub-lift.v1`). `short` is the move of each region that the
/// first render of the version fell short of the move that its tone
/// predicts. Each shelf of the plan takes the shortfall of its region on top
/// of its predicted move, within the largest outcome of the pairs and
/// `SHELF_MAXIMUM_DB`; a shelf only rises. A region without a shelf takes no
/// lift. None when `stereo` carries no bass shelf or no region with a shelf
/// falls short by `LIFT_MINIMUM_DB`. The other bands keep their order, and
/// the shelves follow them.
pub fn lift(spectrum: &Spectrum, stereo: &[EqBand], short: (f64, f64)) -> Option<Vec<EqBand>> {
    let gain_of = |hz: f64| {
        stereo
            .iter()
            .find(|b| is_bass_shelf(b) && b.hz == hz)
            .map(|b| b.gain_db)
    };
    let (sub_was, bass_was) = (gain_of(SUB_SHELF_HZ), gain_of(BASS_SHELF_HZ));
    let want = (
        sub_was.map_or(0.0, |_| short.0.max(0.0)),
        bass_was.map_or(0.0, |_| short.1.max(0.0)),
    );
    if want.0 < LIFT_MINIMUM_DB && want.1 < LIFT_MINIMUM_DB {
        return None;
    }
    let others: Vec<EqBand> = stereo
        .iter()
        .copied()
        .filter(|b| !is_bass_shelf(b))
        .collect();
    let now = regions_db(spectrum, stereo);
    let target = (
        (now.0 + want.0).min(SUB_MINUS_BASS_MAXIMUM_DB.max(now.0)),
        (now.1 + want.1).min(BASS_MINUS_BODY_MAXIMUM_DB.max(now.1)),
    );
    // A shelf that cuts keeps its cut: the lift only raises it from there.
    let shelves = |hz: f64, gain: f64| -> Vec<EqBand> {
        (gain.abs() > 1e-3)
            .then(|| shelf(hz, gain))
            .into_iter()
            .collect()
    };
    let with = |extra: Vec<EqBand>| -> Vec<EqBand> {
        let mut bands = others.clone();
        bands.extend(extra);
        bands
    };
    let (mut sub_db, mut bass_db) = (sub_was.unwrap_or(0.0), bass_was.unwrap_or(0.0));
    for _ in 0..4 {
        if let Some(was) = bass_was {
            let bands = with(shelves(SUB_SHELF_HZ, sub_db));
            let given = regions_db(spectrum, &bands).1;
            bass_db = solve(spectrum, &bands, BASS_SHELF_HZ, target.1 - given, |r| r.1).max(was);
        }
        if let Some(was) = sub_was {
            let bands = with(shelves(BASS_SHELF_HZ, bass_db));
            let given = regions_db(spectrum, &bands).0;
            sub_db = solve(spectrum, &bands, SUB_SHELF_HZ, target.0 - given, |r| r.0).max(was);
        }
    }
    let mut lifted = with(shelves(BASS_SHELF_HZ, bass_db));
    lifted.extend(shelves(SUB_SHELF_HZ, sub_db));
    Some(lifted)
}

/// The gain of a shelf at `hz` whose predicted move of `measure` is
/// `wanted` dB over `bands`: the move of a 1 dB shelf scaled, in two
/// passes, bounded by `SHELF_MAXIMUM_DB` in each direction. A cut of the
/// shelf of a version that cuts its bass (`character::FULL`) is a negative
/// `wanted`.
fn solve(
    spectrum: &Spectrum,
    bands: &[EqBand],
    hz: f64,
    wanted: f64,
    measure: impl Fn((f64, f64)) -> f64,
) -> f64 {
    if wanted.abs() <= 1e-9 {
        return 0.0;
    }
    let before = measure(regions_db(spectrum, bands));
    let mut gain = wanted;
    for _ in 0..2 {
        let mut with = bands.to_vec();
        with.push(shelf(hz, gain));
        let moved = measure(regions_db(spectrum, &with)) - before;
        if moved * wanted.signum() <= 1e-6 {
            return 0.0;
        }
        gain = (gain * wanted / moved).clamp(-SHELF_MAXIMUM_DB, SHELF_MAXIMUM_DB);
    }
    gain
}

pub(crate) fn shelf(hz: f64, gain_db: f64) -> EqBand {
    EqBand {
        kind: Kind::LowShelf,
        hz,
        q: std::f64::consts::FRAC_1_SQRT_2,
        gain_db,
    }
}

/// True for a low shelf of `plan`: a low shelf at `SUB_SHELF_HZ` or
/// `BASS_SHELF_HZ` with the Q of the plan. The bass version of `master`
/// carries the shelves of the plan (`set.bass-version.v4`).
pub fn is_bass_shelf(band: &EqBand) -> bool {
    matches!(band.kind, Kind::LowShelf)
        && (band.hz == SUB_SHELF_HZ || band.hz == BASS_SHELF_HZ)
        && band.q == std::f64::consts::FRAC_1_SQRT_2
}

/// True when `tone` carries a low shelf of `plan` on the stereo mix.
pub fn carries_bass_shelves(tone: &Tone) -> bool {
    tone.stereo.iter().any(is_bass_shelf)
}

/// The plan from the long-term spectrum of the mix. Each shelf moves both
/// regions, so the two gains are solved together: each shelf takes the
/// wanted move less what the other shelf gives its region, in turns.
pub fn plan(spectrum: &Spectrum) -> Open {
    let base = regions_db(spectrum, &[]);
    let (sub_minus_bass_db, bass_minus_body_db) = base;
    let wanted_sub = SUB_MOVE_DB
        .min(SUB_MINUS_BASS_MAXIMUM_DB - sub_minus_bass_db)
        .max(0.0);
    let wanted_bass = BASS_MOVE_DB
        .min(BASS_MINUS_BODY_MAXIMUM_DB - bass_minus_body_db)
        .max(0.0);
    let (mut sub_db, mut bass_db) = (0.0, 0.0);
    let shelves = |hz: f64, gain: f64| -> Vec<EqBand> {
        (gain > 0.0).then(|| shelf(hz, gain)).into_iter().collect()
    };
    for _ in 0..4 {
        let others = shelves(SUB_SHELF_HZ, sub_db);
        let given = regions_db(spectrum, &others).1 - base.1;
        // The shelves of the plan only rise.
        bass_db = solve(spectrum, &others, BASS_SHELF_HZ, wanted_bass - given, |r| {
            r.1
        })
        .max(0.0);
        let others = shelves(BASS_SHELF_HZ, bass_db);
        let given = regions_db(spectrum, &others).0 - base.0;
        sub_db = solve(spectrum, &others, SUB_SHELF_HZ, wanted_sub - given, |r| r.0).max(0.0);
    }
    let mut stereo = shelves(BASS_SHELF_HZ, bass_db);
    stereo.extend(shelves(SUB_SHELF_HZ, sub_db));
    let after = regions_db(spectrum, &stereo);
    Open {
        rule: RULE,
        sub_minus_bass_db,
        bass_minus_body_db,
        sub_db,
        bass_db,
        predicted_sub_minus_bass_db: after.0 - sub_minus_bass_db,
        predicted_bass_minus_body_db: after.1 - bass_minus_body_db,
        tone: Tone {
            stereo,
            ..Tone::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spectrum(sub_db: f64, bass_db: f64) -> Spectrum {
        let centers: Vec<f64> = (0..200)
            .map(|i| 20.0 * 1000f64.powf(i as f64 / 199.0))
            .collect();
        let power: Vec<f64> = centers
            .iter()
            .map(|&hz| {
                let db = if hz < BASS_HZ.0 {
                    sub_db
                } else if hz < BASS_HZ.1 {
                    bass_db
                } else {
                    0.0
                };
                10f64.powf(db / 10.0)
            })
            .collect();
        Spectrum {
            sample_rate: 48000,
            centers_hz: centers,
            mid_power: power.clone(),
            side_power: power.iter().map(|p| p * 0.01).collect(),
            band_power: power,
            mel_power: Vec::new(),
            analyzed_frames: 1,
        }
    }

    #[test]
    fn a_flat_low_end_takes_the_full_moves() {
        let open = plan(&spectrum(0.0, 0.0));
        // The shelves deliver the wanted region moves within 0.05 dB, with
        // gains above the moves, since a shelf's corner lifts its region
        // in part.
        assert!((open.predicted_sub_minus_bass_db - SUB_MOVE_DB).abs() < 0.05);
        assert!((open.predicted_bass_minus_body_db - BASS_MOVE_DB).abs() < 0.05);
        assert!(open.sub_db > SUB_MOVE_DB && open.sub_db <= SHELF_MAXIMUM_DB);
        // The sub shelf lifts part of the bass region, so the bass shelf
        // takes less than the move.
        assert!(open.bass_db > 0.0 && open.bass_db < BASS_MOVE_DB);
        assert_eq!(open.tone.stereo.len(), 2);
    }

    #[test]
    fn the_lift_adds_the_shortfall_of_each_region_with_a_shelf() {
        let s = spectrum(0.0, 0.0);
        let open = plan(&s);
        let mut stereo = vec![EqBand {
            kind: Kind::Peak,
            hz: 3000.0,
            q: 1.0,
            gain_db: 1.0,
        }];
        stereo.extend(open.tone.stereo.iter().copied());
        let before = regions_db(&s, &stereo);
        let lifted = lift(&s, &stereo, (0.5, 0.3)).expect("both regions fall short");
        assert_eq!(lifted[0].hz, 3000.0);
        assert_eq!(lifted.len(), stereo.len());
        let after = regions_db(&s, &lifted);
        assert!(
            (after.0 - before.0 - 0.5).abs() < 0.05,
            "{after:?} {before:?}"
        );
        assert!(
            (after.1 - before.1 - 0.3).abs() < 0.05,
            "{after:?} {before:?}"
        );
        // A shortfall under the minimum, or a region that the render
        // delivered in full, takes no lift.
        assert!(lift(&s, &stereo, (0.05, -0.4)).is_none());
        // Without the shelves of the plan there is nothing to lift.
        assert!(lift(&s, &stereo[..1], (1.0, 1.0)).is_none());
    }

    #[test]
    fn the_lift_keeps_the_largest_outcome_and_raises_only_a_planned_shelf() {
        // The sub sits at the largest outcome: no sub shelf in the plan.
        let s = spectrum(12.0, 6.0);
        let open = plan(&s);
        let lifted = lift(&s, &open.tone.stereo, (2.0, 0.4)).expect("the bass falls short");
        assert!(lifted.iter().all(|b| b.hz != SUB_SHELF_HZ));
        assert!(lifted[0].gain_db > open.bass_db);
        // Each shelf stays under its bound.
        let flat = spectrum(0.0, 0.0);
        let open = plan(&flat);
        let lifted = lift(&flat, &open.tone.stereo, (20.0, 0.0)).unwrap();
        assert!(lifted.iter().all(|b| b.gain_db <= SHELF_MAXIMUM_DB));
    }

    #[test]
    fn mid_regions_follow_the_response() {
        let s = spectrum(0.0, 0.0);
        let flat = mid_regions_db(&s, |_| 0.0);
        let lifted = mid_regions_db(&s, |hz| if hz < SUB_HZ.1 { 2.0 } else { 0.0 });
        assert!((lifted.0 - flat.0 - 2.0).abs() < 1e-9);
        assert!((lifted.1 - flat.1).abs() < 1e-9);
    }

    #[test]
    fn a_mix_at_the_outcome_bound_takes_no_lift() {
        // The sub sits 6 dB over the bass: past the largest outcome.
        let open = plan(&spectrum(12.0, 6.0));
        assert_eq!(open.sub_db, 0.0);
        // The bass shelf alone moves the sub against the bass a little.
        assert!(open.predicted_sub_minus_bass_db.abs() < 0.2);
        assert!(open.bass_db > 0.0);
        assert_eq!(open.tone.stereo.len(), 1);
    }
}
