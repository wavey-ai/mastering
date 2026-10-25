//! Stages of a captured recording: the presence move
//! (`set.capture-presence.v1`, D200) and the low band
//! (`set.capture-low-band.v1`, D201). They apply only to a source that
//! `source_condition::SourceCondition::device_capture` reports. A device set
//! the balance of a captured recording, not a person (D146), so the bounds
//! that keep the balance of a mix give way to the outcome range of the album
//! masters of D177.
use crate::{
    broad_tone,
    dsp::{EqBand, Kind},
    open, Spectrum,
};
use serde::Serialize;

pub const PRESENCE_RULE: &str = "set.capture-presence.v1";
pub const LOW_BAND_RULE: &str = "set.capture-low-band.v1";

/// Largest gain of the presence balance bell of a captured recording, and
/// largest summed response of the static tone of its `master`, in dB: the
/// largest presence bell of the engine, that of `bold`
/// (`broad_tone::VIVID_PRESENCE_MAXIMUM_DB`). A mix keeps
/// `broad_tone::RESPONSE_DB`.
pub const PRESENCE_BELL_MAXIMUM_DB: f64 = broad_tone::VIVID_PRESENCE_MAXIMUM_DB;

/// Largest move of presence against body of the static tone of `master` for
/// a captured recording at `source_db` against the equal-loudness balance
/// `reference_db`, in dB: the move to the darkest album master
/// (`broad_tone::PRESENCE_OUTCOME_MINIMUM_DB`) where the recording is darker,
/// otherwise the P75 move of the pairs, and never past the equal-loudness
/// balance (D134). The move follows the deficit to the reference beyond the
/// album outcomes one for one. A mix takes at most `broad_tone::MAXIMUM_DB`
/// of that move (`broad_tone::presence_move_maximum_db`).
pub fn presence_move_maximum_db(source_db: f64, reference_db: f64) -> f64 {
    broad_tone::PRESENCE_MOVE_MAXIMUM_DB
        .max(broad_tone::PRESENCE_OUTCOME_MINIMUM_DB - source_db)
        .min(reference_db - source_db)
        .max(0.0)
}

/// Smallest outcome of the album masters of D177 in sub (30 to 80 Hz)
/// against bass (80 to 250 Hz), in dB. `open::SUB_MINUS_BASS_MAXIMUM_DB` is
/// the largest.
pub const SUB_MINUS_BASS_MINIMUM_DB: f64 = 0.59;
/// Smallest outcome of the album masters in bass against body (500 Hz to
/// 2 kHz), in dB. `open::BASS_MINUS_BODY_MAXIMUM_DB` is the largest.
pub const BASS_MINUS_BODY_MINIMUM_DB: f64 = 2.78;

/// Corner of the low shelf that moves the sub against the bass, in Hz: the
/// top of the sub region (`open::SUB_HZ`). The shelves of the bass version
/// keep their own corners (`open::SUB_SHELF_HZ`, `open::BASS_SHELF_HZ`), so
/// `open::is_bass_shelf`, which the sub lift of D204 (`open::lift`) and the
/// name Full of the bass version of D182 read, finds only them.
pub const SUB_SHELF_HZ: f64 = open::SUB_HZ.1;
/// Corner of the low shelf that moves the bass against the body, in Hz: the
/// top of the bass region (`open::BASS_HZ`).
pub const BASS_SHELF_HZ: f64 = open::BASS_HZ.1;

/// The low band of a captured recording (`set.capture-low-band.v1`).
#[derive(Clone, Serialize, Debug)]
pub struct LowBand {
    pub rule: &'static str,
    /// The regions of the recording (`open::SUB_HZ`, `open::BASS_HZ`,
    /// `open::BODY_HZ`), in dB.
    pub sub_minus_bass_db: f64,
    pub bass_minus_body_db: f64,
    /// The distance of each measure to the outcome range of the album
    /// masters: positive under the range, negative over it, 0 inside.
    pub wanted_sub_minus_bass_db: f64,
    pub wanted_bass_minus_body_db: f64,
    /// The gains of the low shelves at `SUB_SHELF_HZ` and
    /// `BASS_SHELF_HZ`, in dB; 0 for a shelf under
    /// `broad_tone::MINIMUM_DB`.
    pub sub_db: f64,
    pub bass_db: f64,
    /// The predicted moves of the regions under the shelves, in dB.
    pub predicted_sub_minus_bass_db: f64,
    pub predicted_bass_minus_body_db: f64,
}

impl LowBand {
    /// The shelves of the plan.
    pub fn shelves(&self) -> Vec<EqBand> {
        [(BASS_SHELF_HZ, self.bass_db), (SUB_SHELF_HZ, self.sub_db)]
            .into_iter()
            .filter(|(_, gain)| *gain != 0.0)
            .map(|(hz, gain)| shelf(hz, gain))
            .collect()
    }
}

fn shelf(hz: f64, gain_db: f64) -> EqBand {
    EqBand {
        kind: Kind::LowShelf,
        hz,
        q: std::f64::consts::FRAC_1_SQRT_2,
        gain_db,
    }
}

/// The distance of `value` to the range `low..=high`: positive under it,
/// negative over it.
fn to_range(value: f64, low: f64, high: f64) -> f64 {
    if value < low {
        low - value
    } else if value > high {
        high - value
    } else {
        0.0
    }
}

/// The gain of a shelf at `hz` whose predicted move of `measure` over
/// `others` is `wanted` dB, either sign, within `open::SHELF_MAXIMUM_DB`.
fn solve(
    spectrum: &Spectrum,
    others: &[EqBand],
    hz: f64,
    wanted: f64,
    measure: impl Fn((f64, f64)) -> f64,
) -> f64 {
    if wanted == 0.0 {
        return 0.0;
    }
    let before = measure(open::regions_db(spectrum, others));
    let mut gain = wanted;
    for _ in 0..2 {
        let mut with = others.to_vec();
        with.push(shelf(hz, gain));
        let moved = measure(open::regions_db(spectrum, &with)) - before;
        if moved * wanted <= 1e-9 {
            return 0.0;
        }
        gain = (gain * wanted / moved).clamp(-open::SHELF_MAXIMUM_DB, open::SHELF_MAXIMUM_DB);
    }
    gain
}

/// The low band of a captured recording from its long-term spectrum. Where
/// the sub against the bass or the bass against the body lies outside the
/// range of the album masters, a low shelf moves it to the nearest end of
/// the range, in either direction, by at most `open::SHELF_MAXIMUM_DB`. The
/// two shelves move both regions, so their gains are solved together, as in
/// `open::plan`.
pub fn low_band(spectrum: &Spectrum) -> LowBand {
    let base = open::regions_db(spectrum, &[]);
    let (sub_minus_bass_db, bass_minus_body_db) = base;
    let wanted_sub = to_range(
        sub_minus_bass_db,
        SUB_MINUS_BASS_MINIMUM_DB,
        open::SUB_MINUS_BASS_MAXIMUM_DB,
    );
    let wanted_bass = to_range(
        bass_minus_body_db,
        BASS_MINUS_BODY_MINIMUM_DB,
        open::BASS_MINUS_BODY_MAXIMUM_DB,
    );
    let shelves = |hz: f64, gain: f64| -> Vec<EqBand> {
        (gain != 0.0).then(|| shelf(hz, gain)).into_iter().collect()
    };
    // A shelf moves only in the direction of its own region: the move that
    // the other shelf gives a region inside the range stays.
    let directed = |wanted: f64, given: f64| {
        if wanted > 0.0 {
            (wanted - given).max(0.0)
        } else if wanted < 0.0 {
            (wanted - given).min(0.0)
        } else {
            0.0
        }
    };
    let (mut sub_db, mut bass_db) = (0.0, 0.0);
    for _ in 0..4 {
        let others = shelves(SUB_SHELF_HZ, sub_db);
        let given = open::regions_db(spectrum, &others).1 - base.1;
        bass_db = solve(
            spectrum,
            &others,
            BASS_SHELF_HZ,
            directed(wanted_bass, given),
            |r| r.1,
        );
        let others = shelves(BASS_SHELF_HZ, bass_db);
        let given = open::regions_db(spectrum, &others).0 - base.0;
        sub_db = solve(
            spectrum,
            &others,
            SUB_SHELF_HZ,
            directed(wanted_sub, given),
            |r| r.0,
        );
    }
    // A shelf under the smallest move of a stage is not proposed.
    let keep = |g: f64| {
        if g.abs() >= broad_tone::MINIMUM_DB {
            g
        } else {
            0.0
        }
    };
    let (sub_db, bass_db) = (keep(sub_db), keep(bass_db));
    let mut stereo = shelves(BASS_SHELF_HZ, bass_db);
    stereo.extend(shelves(SUB_SHELF_HZ, sub_db));
    let after = open::regions_db(spectrum, &stereo);
    LowBand {
        rule: LOW_BAND_RULE,
        sub_minus_bass_db,
        bass_minus_body_db,
        wanted_sub_minus_bass_db: wanted_sub,
        wanted_bass_minus_body_db: wanted_bass,
        sub_db,
        bass_db,
        predicted_sub_minus_bass_db: after.0 - sub_minus_bass_db,
        predicted_bass_minus_body_db: after.1 - bass_minus_body_db,
    }
}

/// The stages of a captured recording in the broad tone analysis of a set:
/// the rule and the bound of the presence move, and the low band.
#[derive(Clone, Serialize, Debug)]
pub struct Capture {
    pub presence_rule: &'static str,
    /// Presence against body of the recording and of the equal-loudness
    /// balance, and the largest move of `master` (`presence_move_maximum_db`),
    /// in dB.
    pub presence_source_db: f64,
    pub presence_reference_db: f64,
    pub presence_move_maximum_db: f64,
    /// The largest gain of the presence balance bell
    /// (`PRESENCE_BELL_MAXIMUM_DB`).
    pub presence_bell_maximum_db: f64,
    pub low_band: LowBand,
}

impl Capture {
    pub fn of(spectrum: &Spectrum) -> Self {
        let source = broad_tone::source_presence_db(spectrum);
        let reference = broad_tone::reference_presence_db(spectrum);
        Self {
            presence_rule: PRESENCE_RULE,
            presence_source_db: source,
            presence_reference_db: reference,
            presence_move_maximum_db: presence_move_maximum_db(source, reference),
            presence_bell_maximum_db: PRESENCE_BELL_MAXIMUM_DB,
            low_band: low_band(spectrum),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A spectrum with the sub, the bass and the rest at the given levels, in
    /// dB, and the presence at `presence_db` against the body.
    fn spectrum(sub_db: f64, bass_db: f64, presence_db: f64) -> Spectrum {
        let centers: Vec<f64> = (0..200)
            .map(|i| 20.0 * 1000f64.powf(i as f64 / 199.0))
            .collect();
        let power: Vec<f64> = centers
            .iter()
            .map(|&hz| {
                let db = if hz < open::BASS_HZ.0 {
                    sub_db
                } else if hz < open::BASS_HZ.1 {
                    bass_db
                } else if (broad_tone::PRESENCE_HZ.0..broad_tone::PRESENCE_HZ.1).contains(&hz) {
                    presence_db
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
            side_power: vec![0.0; power.len()],
            band_power: power,
            mel_power: Vec::new(),
            analyzed_frames: 1,
        }
    }

    #[test]
    fn a_dark_capture_moves_to_the_darkest_album_master() {
        let reference = -3.3;
        // 8.46 dB under the reference and darker than every album master.
        // A mix stops at 2 dB.
        assert!((presence_move_maximum_db(-11.76, reference) - 2.66).abs() < 1e-9);
        assert_eq!(
            broad_tone::presence_move_maximum_db(-11.76, reference),
            broad_tone::MAXIMUM_DB
        );
        // Inside the album outcomes the capture keeps the P75 move of a mix.
        assert_eq!(
            presence_move_maximum_db(-8.66, reference),
            broad_tone::presence_move_maximum_db(-8.66, reference)
        );
        // At or past the equal-loudness balance there is no move.
        assert_eq!(presence_move_maximum_db(-3.22, reference), 0.0);
        assert_eq!(presence_move_maximum_db(-2.69, reference), 0.0);
    }

    #[test]
    fn a_thin_sub_takes_a_lift_to_the_album_range() {
        // The sub 6 dB under the bass: 6.6 dB under the smallest outcome.
        let band = low_band(&spectrum(0.0, 6.0, 0.0));
        assert!(band.sub_minus_bass_db < SUB_MINUS_BASS_MINIMUM_DB);
        assert!(band.wanted_sub_minus_bass_db > 6.0);
        // The shelf stops at its largest gain.
        assert_eq!(band.sub_db, open::SHELF_MAXIMUM_DB);
        assert!(band.predicted_sub_minus_bass_db > 1.0);
        // The bass against the body is inside the range: no bass shelf.
        assert_eq!(band.bass_db, 0.0);
        assert_eq!(band.shelves().len(), 1);
    }

    #[test]
    fn the_shelves_of_a_capture_are_not_the_shelves_of_the_bass_version() {
        let band = low_band(&spectrum(13.0, 12.0, 0.0));
        let thin = low_band(&spectrum(0.0, 6.0, 0.0));
        let shelves: Vec<EqBand> = band.shelves().into_iter().chain(thin.shelves()).collect();
        assert!(!shelves.is_empty());
        assert!(shelves.iter().all(|b| !open::is_bass_shelf(b)));
    }

    #[test]
    fn a_low_band_inside_the_album_range_takes_no_shelf() {
        // Sub 2 dB over the bass, bass 5 dB over the body.
        let band = low_band(&spectrum(7.0, 5.0, 0.0));
        assert_eq!(band.wanted_sub_minus_bass_db, 0.0);
        assert_eq!(band.wanted_bass_minus_body_db, 0.0);
        assert!(band.shelves().is_empty());
    }

    #[test]
    fn a_heavy_bass_takes_a_cut_to_the_album_range() {
        // Bass 12 dB over the body: 2.3 dB over the largest outcome.
        let band = low_band(&spectrum(13.0, 12.0, 0.0));
        assert!(band.wanted_bass_minus_body_db < -2.0);
        assert!(band.bass_db < 0.0);
        assert!(band.predicted_bass_minus_body_db < -1.5);
    }
}
