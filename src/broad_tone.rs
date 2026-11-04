//! Broad tonal measures of the recording: the lead of the arrangement and
//! its masking from aligned stems, the presence and air balances of the
//! source spectrum against ISO 226 equal-loudness contours, and the presence
//! and treble steps of Vivid. Each target comes from the recording.
use crate::{
    dsp::{EqBand, Kind},
    stem_evidence::{Band, Evidence},
    treatment::Tone,
};
use serde::Serialize;

pub const RULE: &str = "set.broad-tone.v7";
/// Q of the presence bells: the presence bell of each version (`character`)
/// and the presence balance bell.
pub const LEAD_BELL_Q: f64 = 0.7;
/// Largest single move, in dB.
pub const MAXIMUM_DB: f64 = 2.0;
/// Largest gain of the presence balance bell of a mix, in dB.
pub const RESPONSE_DB: f64 = 3.0;
/// Smallest proposed move; smaller evidence stays unchanged.
pub const MINIMUM_DB: f64 = 0.5;
/// Assumed monitoring level, in phon: the level of the equal-loudness
/// balance (D134).
pub const REFERENCE_PHON: f64 = 75.0;
/// Lead activity required for masking moves, in seconds.
pub const MASKING_MINIMUM_SECONDS: f64 = 10.0;
/// The vocal leads by precedence when active for this fraction of the
/// frames where any role is active.
pub const VOCAL_LEAD_FRACTION: f64 = 0.15;
/// Band whose share decides an instrumental lead.
pub const LEAD_BAND_HZ: (f64, f64) = (500.0, 5000.0);
/// Smallest mean share of `LEAD_BAND_HZ` for an instrumental lead.
pub const LEAD_MINIMUM_SHARE: f64 = 0.35;
/// The lead's share must exceed the runner-up's by this much.
pub const LEAD_MARGIN_DB: f64 = 1.5;
/// Energy centroid bounds for an instrumental lead. Below 500 Hz the lead is
/// a bass line; above 2 kHz the centroid is clamped.
pub const LEAD_CENTROID_HZ: (f64, f64) = (500.0, 2000.0);
/// The vocal keeps fixed regions: 1 kHz gives low mids of 250–500 Hz and a
/// body of 500 Hz–2 kHz.
pub const VOCAL_CENTROID_HZ: f64 = 1000.0;
/// Bands whose centers decide an instrumental lead's centroid.
const CENTROID_BAND_HZ: (f64, f64) = (250.0, 5000.0);

/// A moved band must change against Essential at matched level by this
/// much: 30% of the filter gain, at least 0.2 dB.
pub fn gate_minimum_db(gain_db: f64) -> f64 {
    (0.3 * gain_db.abs()).max(0.2)
}

#[derive(Clone, Serialize)]
pub struct Lead {
    pub role: &'static str,
    pub index: usize,
    pub active_seconds: f64,
    /// Mean share of `LEAD_BAND_HZ` over all frames, per role, in dB.
    pub band_share_db: [f64; 4],
    /// Log-frequency energy centroid of the lead over 250 Hz–5 kHz, or
    /// `VOCAL_CENTROID_HZ` for the vocal.
    pub centroid_hz: f64,
    pub by_precedence: bool,
}

/// The lead's share of the arrangement in its low mids and in its body.
#[derive(Clone, Serialize)]
pub struct Masking {
    pub lead: Lead,
    pub low_mid_hz: (f64, f64),
    pub body_hz: (f64, f64),
    pub share_low_mid_db: f64,
    pub share_body_db: f64,
    pub low_mid_deficit_db: f64,
}

/// The mix's presence against its body, and the balance of equal loudness
/// per band (`set.presence-balance.v2`).
#[derive(Clone, Serialize)]
pub struct PresenceBalance {
    /// Presence (2 to 5 kHz) against body (500 Hz to 2 kHz): mean band power
    /// of the source spectrum, in dB.
    pub source_db: f64,
    /// The same ratio for bands of equal loudness at `REFERENCE_PHON`
    /// (ISO 226), in dB.
    pub reference_db: f64,
    /// Change of the ratio that the proposal aims at: the shortfall to the
    /// reference, at most `presence_move_maximum_db`.
    pub target_move_db: f64,
    /// Change of the ratio that the other filters give by themselves.
    pub other_moves_db: f64,
}

/// The lead of a set and the depth of its low-mid correction
/// (`set.broad-tone.v7`).
#[derive(Clone, Serialize)]
pub struct Analysis {
    pub rule: &'static str,
    pub masking: Option<Masking>,
    /// Largest cut of the dynamic low-mid correction, in dB (`low_mid_cut_db`).
    pub low_mid_cut_db: Option<f64>,
}

// ISO 226:2003 parameters: frequency, alpha_f, L_u, T_f.
const ISO226: [(f64, f64, f64, f64); 29] = [
    (20.0, 0.532, -31.6, 78.5),
    (25.0, 0.506, -27.2, 68.7),
    (31.5, 0.480, -23.0, 59.5),
    (40.0, 0.455, -19.1, 51.1),
    (50.0, 0.432, -15.9, 44.0),
    (63.0, 0.409, -13.0, 37.5),
    (80.0, 0.387, -10.3, 31.5),
    (100.0, 0.367, -8.1, 26.5),
    (125.0, 0.349, -6.2, 22.1),
    (160.0, 0.330, -4.5, 17.9),
    (200.0, 0.315, -3.1, 14.4),
    (250.0, 0.301, -2.0, 11.4),
    (315.0, 0.288, -1.1, 8.6),
    (400.0, 0.276, -0.4, 6.2),
    (500.0, 0.267, 0.0, 4.4),
    (630.0, 0.259, 0.3, 3.0),
    (800.0, 0.253, 0.5, 2.2),
    (1000.0, 0.250, 0.0, 2.4),
    (1250.0, 0.246, -2.7, 3.5),
    (1600.0, 0.244, -4.1, 1.7),
    (2000.0, 0.243, -1.0, -1.3),
    (2500.0, 0.243, 1.7, -4.2),
    (3150.0, 0.243, 2.5, -6.0),
    (4000.0, 0.242, 1.2, -5.4),
    (5000.0, 0.242, -2.1, -1.5),
    (6300.0, 0.245, -7.1, 6.0),
    (8000.0, 0.254, -11.2, 12.6),
    (10000.0, 0.271, -10.7, 13.9),
    (12500.0, 0.301, -3.1, 12.3),
];

/// Sound pressure level of the `phon` equal-loudness contour at one
/// ISO 226 frequency.
fn contour_spl(phon: f64, af: f64, lu: f64, tf: f64) -> f64 {
    let a = 4.47e-3 * (10f64.powf(0.025 * phon) - 1.15)
        + (0.4 * 10f64.powf((tf + lu) / 10.0 - 9.0)).powf(af);
    (10.0 / af) * a.log10() - lu + 94.0
}

/// Presence and body regions of the presence balance, in Hz.
pub const PRESENCE_HZ: (f64, f64) = (2000.0, 5000.0);
pub const BODY_HZ: (f64, f64) = (500.0, 2000.0);
/// Largest move of presence against body of the static tone of `master`,
/// in dB: the P75 move of the album pairs (`set.presence-balance.v2`, D180).
/// An album pair is the mix and the master of one album by one engineer
/// (D177). The
/// pairs move it in both directions.
pub const PRESENCE_MOVE_MAXIMUM_DB: f64 = 0.94;
/// Largest move of presence against body of the tone of Vivid above the
/// bound of `master`, in dB: the P90 move of the pairs (D206).
pub const VIVID_PRESENCE_MOVE_ABOVE_DB: f64 = 1.42;
/// Presence against body of the darkest album master, in dB. A mix darker
/// than every album master lies outside the outcomes of the pairs: its
/// master moves toward the equal-loudness balance up to this outcome, by at
/// most `MAXIMUM_DB`.
pub const PRESENCE_OUTCOME_MINIMUM_DB: f64 = -9.10;

/// Largest move of presence against body of the static tone of `master` for
/// a source at `source_db` against the equal-loudness balance
/// `reference_db`, in dB: the P75 move of the pairs, or the move to the
/// darkest album master where the source is darker than every album master,
/// and never past the equal-loudness balance (D134). Never negative.
pub fn presence_move_maximum_db(source_db: f64, reference_db: f64) -> f64 {
    PRESENCE_MOVE_MAXIMUM_DB
        .max((PRESENCE_OUTCOME_MINIMUM_DB - source_db).min(MAXIMUM_DB))
        .min(reference_db - source_db)
        .max(0.0)
}

/// The largest move of presence against body of the static tone of
/// `master`: `capture::presence_move_maximum_db` for a captured recording
/// (`set.capture-presence.v1`), otherwise `presence_move_maximum_db`.
fn move_maximum_db(source_db: f64, reference_db: f64, capture: bool) -> f64 {
    if capture {
        crate::capture::presence_move_maximum_db(source_db, reference_db)
    } else {
        presence_move_maximum_db(source_db, reference_db)
    }
}

/// Presence against body of bands of equal loudness at `REFERENCE_PHON`, on
/// the band centres of `spectrum`, in dB (D134).
pub fn reference_presence_db(spectrum: &crate::Spectrum) -> f64 {
    let contour = Contour::new(REFERENCE_PHON);
    presence_body_db(&spectrum.centers_hz, |i| {
        10f64.powf(contour.at(spectrum.centers_hz[i]) / 10.0)
    })
}

/// Presence against body of the source spectrum, in dB.
pub fn source_presence_db(spectrum: &crate::Spectrum) -> f64 {
    presence_body_db(&spectrum.centers_hz, |i| {
        spectrum.mid_power[i] + spectrum.side_power[i]
    })
}
/// The presence bell: centred on the presence region on a log scale, with
/// the Q `LEAD_BELL_Q`.
pub const PRESENCE_BELL_HZ: f64 = 3162.0;

/// The `phon` equal-loudness contour: the log frequency and the SPL of each
/// ISO 226 frequency.
struct Contour(Vec<(f64, f64)>);

impl Contour {
    fn new(phon: f64) -> Self {
        Self(
            ISO226
                .iter()
                .map(|&(f, af, lu, tf)| (f.ln(), contour_spl(phon, af, lu, tf)))
                .collect(),
        )
    }

    /// SPL at `hz`, interpolated on a log frequency scale between the ISO 226
    /// frequencies.
    fn at(&self, hz: f64) -> f64 {
        let x = hz.clamp(20.0, 12500.0).ln();
        self.0
            .windows(2)
            .find(|w| x >= w[0].0 && x <= w[1].0)
            .map_or(self.0.last().unwrap().1, |w| {
                w[0].1 + (w[1].1 - w[0].1) * (x - w[0].0) / (w[1].0 - w[0].0)
            })
    }
}

/// Power weight of each of `hz` at the `REFERENCE_PHON` monitoring level,
/// against 1 kHz: the inverse of the equal-loudness contour, as A- and
/// B-weighting are inverse contours at lower levels.
pub fn equal_loudness_weights(hz: impl IntoIterator<Item = f64>) -> Vec<f64> {
    // One contour for the weights: a contour for each weight took the SPL
    // of each ISO 226 frequency twice (D316).
    let contour = Contour::new(REFERENCE_PHON);
    let reference = contour.at(1000.0);
    hz.into_iter()
        .map(|hz| 10f64.powf((reference - contour.at(hz)) / 10.0))
        .collect()
}

/// Presence against body of per-band powers, in dB: the ratio of the mean
/// band power in each region.
fn presence_body_db(centers: &[f64], power: impl Fn(usize) -> f64) -> f64 {
    region_ratio_db(centers, power, PRESENCE_HZ, BODY_HZ)
}

/// Region `a` against region `b` of per-band powers, in dB: the ratio of
/// the mean band power in each region.
fn region_ratio_db(
    centers: &[f64],
    power: impl Fn(usize) -> f64,
    a: (f64, f64),
    b: (f64, f64),
) -> f64 {
    let mean = |(low, high): (f64, f64)| {
        let v: Vec<f64> = (0..centers.len())
            .filter(|&i| centers[i] >= low && centers[i] < high)
            .map(&power)
            .collect();
        v.iter().sum::<f64>() / v.len().max(1) as f64
    };
    10.0 * (mean(a).max(1e-30) / mean(b).max(1e-30)).log10()
}

/// Region `a` against region `b` of the source after `stereo` filters on
/// both channels, `mid` filters on the mid channel and the widening `side`
/// gain.
pub(crate) fn shaped_ratio_db(
    spectrum: &crate::Spectrum,
    stereo: &[EqBand],
    mid: &[EqBand],
    side: Option<&crate::width::SideGain>,
    a: (f64, f64),
    b: (f64, f64),
) -> f64 {
    let rate = spectrum.sample_rate;
    let response = |filters: &[EqBand], hz: f64| {
        10f64.powf(
            filters
                .iter()
                .map(|b| b.filter(rate).response_db(hz, rate))
                .sum::<f64>()
                / 10.0,
        )
    };
    region_ratio_db(
        &spectrum.centers_hz,
        |i| {
            let hz = spectrum.centers_hz[i];
            let s = response(stereo, hz);
            let widening = side.map_or(1.0, |g| 10f64.powf(g.response_db(hz) / 10.0));
            spectrum.mid_power[i] * s * response(mid, hz) + spectrum.side_power[i] * s * widening
        },
        a,
        b,
    )
}

/// Air region of the air balance, in Hz.
pub const AIR_HZ: (f64, f64) = (8000.0, 16000.0);
/// Corner of the air balance shelf, in Hz.
pub const AIR_SHELF_HZ: f64 = 8000.0;

/// The air balance shelf at `gain_db`.
pub fn air_shelf(gain_db: f64) -> EqBand {
    EqBand {
        kind: Kind::HighShelf,
        hz: AIR_SHELF_HZ,
        q: SHELF_Q,
        gain_db,
    }
}

fn is_air_shelf(b: &EqBand) -> bool {
    matches!(b.kind, Kind::HighShelf) && b.hz == AIR_SHELF_HZ && b.q == SHELF_Q
}

/// The gain of the air shelf, from 0 to `MAXIMUM_DB`, that returns the air
/// against presence of `stereo`, `mid` and `side` to `target_db`. None when
/// the tone keeps that balance by itself.
fn air_gain(
    spectrum: &crate::Spectrum,
    stereo: &[EqBand],
    mid: &[EqBand],
    side: Option<&crate::width::SideGain>,
    target_db: f64,
) -> Option<f64> {
    let at = |g: f64| {
        let mut with: Vec<EqBand> = stereo
            .iter()
            .filter(|b| !is_air_shelf(b))
            .copied()
            .collect();
        if g > 0.0 {
            with.push(air_shelf(g));
        }
        shaped_ratio_db(spectrum, &with, mid, side, AIR_HZ, PRESENCE_HZ)
    };
    if at(0.0) >= target_db {
        return None;
    }
    if at(MAXIMUM_DB) <= target_db {
        return Some(MAXIMUM_DB);
    }
    let (mut low, mut high) = (0.0, MAXIMUM_DB);
    for _ in 0..40 {
        let middle = 0.5 * (low + high);
        if at(middle) < target_db {
            low = middle;
        } else {
            high = middle;
        }
    }
    Some(high)
}

/// Treble region of the step from Clear to Vivid, in Hz.
pub const TREBLE_HZ: (f64, f64) = (6000.0, 12000.0);
/// Smallest step of presence against body from Clear to Vivid, in dB:
/// twice the duplicate threshold (`selection::DISTINCT_DB`).
pub const VIVID_PRESENCE_STEP_DB: f64 = 2.0 * crate::selection::DISTINCT_DB;
/// Smallest step of treble against body from Clear to Vivid, in dB: twice
/// the duplicate threshold, as the presence step (D206).
pub const VIVID_TREBLE_STEP_DB: f64 = 2.0 * crate::selection::DISTINCT_DB;
/// Corner of the treble shelf of Vivid, in Hz.
pub const VIVID_TREBLE_SHELF_HZ: f64 = 6000.0;
/// Largest gain of the presence balance bell of Vivid, in dB.
pub const VIVID_PRESENCE_MAXIMUM_DB: f64 = 6.0;

/// The tone of Vivid from the tone of Clear (`set.vivid-steps.v2`). The lead
/// bells keep the gains of Clear: the lead keeps its parity with its body,
/// and the mid does not rise against the side. The presence balance bell
/// has the gain that puts the predicted presence against body
/// `VIVID_PRESENCE_STEP_DB` above Clear, at least the gain of Clear, and no
/// further than `VIVID_PRESENCE_MOVE_ABOVE_DB` above the presence bound of
/// `master` (`presence_move_maximum_db`) from the source: the P90 side of
/// the album pairs (D180). Where the source has the treble, a
/// high shelf puts the predicted treble against body `VIVID_TREBLE_STEP_DB`
/// above Clear; `treble_band` is true when the source has the 6 to 12 kHz
/// band (no codec cut under 12 kHz). `clear_side` and `vivid_side` are the side gains of the
/// predictions; the set passes the side gain of Clear for both, so the
/// steps come from the tone and not from the widening. `bound_side` is the
/// widening of Vivid: the bound applies to the whole tone with it, because
/// a wider side carries more presence against body. `capture` is true for
/// a captured recording, whose bound of `master` is
/// `capture::presence_move_maximum_db` (`set.capture-presence.v1`).
pub fn vivid_tone(
    clear: &Tone,
    spectrum: &crate::Spectrum,
    clear_side: Option<&crate::width::SideGain>,
    vivid_side: Option<&crate::width::SideGain>,
    bound_side: Option<&crate::width::SideGain>,
    treble_band: bool,
    capture: bool,
) -> Tone {
    let ratio = |t: &Tone, side, a, b| shaped_ratio_db(spectrum, &t.stereo, &t.mid, side, a, b);
    let is_presence =
        |b: &EqBand| matches!(b.kind, Kind::Peak) && b.hz == PRESENCE_BELL_HZ && b.q == LEAD_BELL_Q;
    let with_gain =
        |tone: &Tone, gain: f64, is: &dyn Fn(&EqBand) -> bool, make: &dyn Fn(f64) -> EqBand| {
            let mut t = tone.clone();
            match t.stereo.iter().position(is) {
                Some(i) => t.stereo[i].gain_db = gain,
                None => t.stereo.push(make(gain)),
            }
            t
        };
    // The smallest gain in [low, high] whose ratio reaches `target`.
    let solve = |low: f64, high: f64, ratio_at: &dyn Fn(f64) -> f64, target: f64| {
        if ratio_at(low) >= target {
            return low;
        }
        if ratio_at(high) <= target {
            return high;
        }
        let (mut lo, mut hi) = (low, high);
        for _ in 0..40 {
            let middle = 0.5 * (lo + hi);
            if ratio_at(middle) < target {
                lo = middle;
            } else {
                hi = middle;
            }
        }
        hi
    };
    let presence_bell = |g| bell(PRESENCE_BELL_HZ, LEAD_BELL_Q, g);
    let clear_gain = clear
        .stereo
        .iter()
        .find(|b| is_presence(b))
        .map_or(0.0, |b| b.gain_db);
    let source = source_presence_db(spectrum);
    let bound = source
        + move_maximum_db(source, reference_presence_db(spectrum), capture)
        + VIVID_PRESENCE_MOVE_ABOVE_DB;
    let target =
        (ratio(clear, clear_side, PRESENCE_HZ, BODY_HZ) + VIVID_PRESENCE_STEP_DB).min(bound);
    let presence_at = |g: f64| {
        ratio(
            &with_gain(clear, g, &is_presence, &presence_bell),
            vivid_side,
            PRESENCE_HZ,
            BODY_HZ,
        )
    };
    let gain = solve(clear_gain, VIVID_PRESENCE_MAXIMUM_DB, &presence_at, target);
    let mut tone = with_gain(clear, gain, &is_presence, &presence_bell);
    // Where Clear has an air shelf, Vivid solves it again for its own
    // presence: the air against presence of Clear stays.
    if clear.stereo.iter().any(is_air_shelf) {
        let target = ratio(clear, clear_side, AIR_HZ, PRESENCE_HZ);
        let g = air_gain(spectrum, &tone.stereo, &tone.mid, vivid_side, target)
            .unwrap_or(0.0)
            .max(
                clear
                    .stereo
                    .iter()
                    .find(|b| is_air_shelf(b))
                    .map_or(0.0, |b| b.gain_db),
            );
        if let Some(b) = tone.stereo.iter_mut().find(|b| is_air_shelf(b)) {
            b.gain_db = g;
        }
    }
    if treble_band {
        let target = ratio(clear, clear_side, TREBLE_HZ, BODY_HZ) + VIVID_TREBLE_STEP_DB;
        let is_treble = |b: &EqBand| {
            matches!(b.kind, Kind::HighShelf) && b.hz == VIVID_TREBLE_SHELF_HZ && b.q == SHELF_Q
        };
        let treble_shelf = |g| EqBand {
            kind: Kind::HighShelf,
            hz: VIVID_TREBLE_SHELF_HZ,
            q: SHELF_Q,
            gain_db: g,
        };
        let base = tone.clone();
        let treble_at = |g: f64| {
            ratio(
                &with_gain(&base, g, &is_treble, &treble_shelf),
                vivid_side,
                TREBLE_HZ,
                BODY_HZ,
            )
        };
        if treble_at(0.0) < target {
            let g = solve(0.0, MAXIMUM_DB, &treble_at, target);
            tone = with_gain(&base, g, &is_treble, &treble_shelf);
        }
    }
    // The whole tone, with the shelves and the widening of Vivid, stays on
    // the P90 side of the pairs: the presence balance bell gives back what
    // goes past the bound, down to the gain of Clear.
    let fixed = tone.clone();
    let bounded = |g: f64| {
        ratio(
            &with_gain(&fixed, g, &is_presence, &presence_bell),
            bound_side,
            PRESENCE_HZ,
            BODY_HZ,
        )
    };
    if gain > clear_gain && bounded(gain) > bound {
        let (mut low, mut high) = (clear_gain, gain);
        if bounded(low) >= bound {
            high = low;
        } else {
            for _ in 0..40 {
                let middle = 0.5 * (low + high);
                if bounded(middle) > bound {
                    high = middle;
                } else {
                    low = middle;
                }
            }
            high = low;
        }
        tone = with_gain(&fixed, high, &is_presence, &presence_bell);
    }
    tone
}

/// Q of the shelves.
const SHELF_Q: f64 = std::f64::consts::FRAC_1_SQRT_2;

/// Presence against body of the source after `stereo` filters on both
/// channels, `mid` filters on the mid channel and the widening `side` gain.
pub(crate) fn predicted_db(
    spectrum: &crate::Spectrum,
    stereo: &[EqBand],
    mid: &[EqBand],
    side: Option<&crate::width::SideGain>,
) -> f64 {
    shaped_ratio_db(spectrum, stereo, mid, side, PRESENCE_HZ, BODY_HZ)
}

/// The presence balance of the source and the stereo bell that moves the
/// whole tone's presence against body toward the equal-loudness balance,
/// by at most `presence_move_maximum_db`. `stereo` and `mid` are the other
/// moves' filters and `side` the widening. The bell gain is at most
/// `RESPONSE_DB`: a broad bell also lifts part of the body, so its gain
/// exceeds the move it gives. For a captured recording (`capture`), the
/// move is at most `capture::presence_move_maximum_db` and the bell gain
/// at most `capture::PRESENCE_BELL_MAXIMUM_DB` (`set.capture-presence.v1`).
pub fn presence_balance(
    spectrum: &crate::Spectrum,
    stereo: &[EqBand],
    mid: &[EqBand],
    side: Option<&crate::width::SideGain>,
    capture: bool,
) -> (PresenceBalance, Option<EqBand>) {
    let source_db = source_presence_db(spectrum);
    // Equal loudness per band: each band's power follows the contour.
    let reference_db = reference_presence_db(spectrum);
    let target_move_db = move_maximum_db(source_db, reference_db, capture);
    let other_moves_db = predicted_db(spectrum, stereo, mid, side) - source_db;
    let balance = PresenceBalance {
        source_db,
        reference_db,
        target_move_db,
        other_moves_db,
    };
    if target_move_db - other_moves_db < MINIMUM_DB {
        return (balance, None);
    }
    // The bell gain whose whole-tone move meets the target, by bisection.
    let moved = |gain: f64| {
        let mut with = stereo.to_vec();
        with.push(bell(PRESENCE_BELL_HZ, LEAD_BELL_Q, gain));
        predicted_db(spectrum, &with, mid, side) - source_db
    };
    let largest = if capture {
        crate::capture::PRESENCE_BELL_MAXIMUM_DB
    } else {
        RESPONSE_DB
    };
    let (mut low, mut high) = (0.0, largest);
    if moved(high) <= target_move_db {
        return (balance, Some(bell(PRESENCE_BELL_HZ, LEAD_BELL_Q, high)));
    }
    for _ in 0..40 {
        let middle = 0.5 * (low + high);
        if moved(middle) < target_move_db {
            low = middle;
        } else {
            high = middle;
        }
    }
    let gain = 0.5 * (low + high);
    (
        balance,
        (gain >= MINIMUM_DB).then(|| bell(PRESENCE_BELL_HZ, LEAD_BELL_Q, gain)),
    )
}

fn bell(hz: f64, q: f64, gain_db: f64) -> EqBand {
    EqBand {
        kind: Kind::Peak,
        hz,
        q,
        gain_db,
    }
}

/// Mean of `select(band)` over the one-third-octave bands whose centers
/// fall in [low, high), in dB.
fn mean_share_db(evidence: &Evidence, low: f64, high: f64, select: impl Fn(&Band) -> f64) -> f64 {
    let shares: Vec<f64> = evidence
        .bands
        .iter()
        .filter(|b| b.center_hz >= low && b.center_hz < high)
        .map(select)
        .collect();
    10.0 * (shares.iter().sum::<f64>() / shares.len().max(1) as f64)
        .max(1e-6)
        .log10()
}

/// Log-frequency energy centroid of a role over `CENTROID_BAND_HZ`.
fn centroid_hz(evidence: &Evidence, role: usize) -> f64 {
    let (num, den) = evidence
        .bands
        .iter()
        .filter(|b| b.center_hz >= CENTROID_BAND_HZ.0 && b.center_hz < CENTROID_BAND_HZ.1)
        .fold((0.0, 0.0), |(n, d), b| {
            let w = b.role_energy_fractions[role];
            (n + w * b.center_hz.log2(), d + w)
        });
    2f64.powf(num / den.max(1e-30))
}

/// The lead role. The vocal takes precedence when active for at least
/// `MASKING_MINIMUM_SECONDS` and `VOCAL_LEAD_FRACTION` of the active frames.
/// Otherwise the role with the highest mean share of `LEAD_BAND_HZ` leads
/// when it is not drums, holds `LEAD_MINIMUM_SHARE`, beats the runner-up by
/// `LEAD_MARGIN_DB`, is active for `MASKING_MINIMUM_SECONDS` and has a
/// centroid at or above `LEAD_CENTROID_HZ.0`.
pub fn lead(evidence: &Evidence) -> Option<Lead> {
    let band_share_db: [f64; 4] = std::array::from_fn(|role| {
        mean_share_db(evidence, LEAD_BAND_HZ.0, LEAD_BAND_HZ.1, |b| {
            b.all_activity_power_shares[role]
        })
    });
    let vocal = evidence.vocal_active_seconds;
    if vocal >= MASKING_MINIMUM_SECONDS
        && vocal >= VOCAL_LEAD_FRACTION * evidence.any_active_seconds
    {
        return Some(Lead {
            role: evidence.roles[3],
            index: 3,
            active_seconds: vocal,
            band_share_db,
            centroid_hz: VOCAL_CENTROID_HZ,
            by_precedence: true,
        });
    }
    let mut order: Vec<usize> = (0..4).collect();
    order.sort_by(|&a, &b| band_share_db[b].total_cmp(&band_share_db[a]));
    let (best, second) = (order[0], order[1]);
    if best == 0
        || band_share_db[best] < 10.0 * LEAD_MINIMUM_SHARE.log10()
        || band_share_db[best] - band_share_db[second] < LEAD_MARGIN_DB
        || evidence.role_active_seconds[best] < MASKING_MINIMUM_SECONDS
    {
        return None;
    }
    let centroid = if best == 3 {
        VOCAL_CENTROID_HZ
    } else {
        centroid_hz(evidence, best)
    };
    if centroid < LEAD_CENTROID_HZ.0 {
        return None;
    }
    Some(Lead {
        role: evidence.roles[best],
        index: best,
        active_seconds: evidence.role_active_seconds[best],
        band_share_db,
        centroid_hz: centroid.min(LEAD_CENTROID_HZ.1),
        by_precedence: false,
    })
}

pub fn masking(evidence: &Evidence) -> Option<Masking> {
    if !evidence.usable {
        return None;
    }
    let lead = lead(evidence)?;
    let c = lead.centroid_hz;
    let (low_mid_hz, body_hz) = ((c / 4.0, c / 2.0), (c / 2.0, 2.0 * c));
    let share = |(low, high): (f64, f64)| {
        mean_share_db(evidence, low, high, |b| {
            b.role_active_power_shares[lead.index]
        })
    };
    let (low_mid, body) = (share(low_mid_hz), share(body_hz));
    Some(Masking {
        lead,
        low_mid_hz,
        body_hz,
        share_low_mid_db: low_mid,
        share_body_db: body,
        low_mid_deficit_db: body - low_mid,
    })
}

/// The lead and its masking from the stems (`masking`), and the largest
/// cut of the dynamic low-mid correction (`low_mid_cut_db`).
pub fn analyze(evidence: Option<&Evidence>) -> Analysis {
    let masking = evidence.and_then(masking);
    let low_mid_cut_db = masking
        .as_ref()
        .and_then(|m| low_mid_cut_db(m, evidence.and_then(|e| e.low_mid.as_ref())));
    Analysis {
        rule: RULE,
        masking,
        low_mid_cut_db,
    }
}

/// Largest cut of the low-mid correction, in dB: the lead's share deficit
/// from its body to its low mids, from `MINIMUM_DB` to `MAXIMUM_DB`. The cut
/// applies in the frames where the other stems mask the lead
/// (`low_mid::key`), so it needs a key of the lead role.
fn low_mid_cut_db(masking: &Masking, key: Option<&crate::low_mid::Key>) -> Option<f64> {
    let cut = masking.low_mid_deficit_db.clamp(0.0, MAXIMUM_DB);
    (cut >= MINIMUM_DB && key.is_some_and(|k| k.lead_role == masking.lead.index)).then_some(cut)
}

/// Scale every filter gain of a tone by `factor`.
pub fn scaled(tone: &Tone, factor: f64) -> Tone {
    let scale = |bands: &Vec<EqBand>| -> Vec<EqBand> {
        bands
            .iter()
            .map(|b| EqBand {
                gain_db: b.gain_db * factor,
                ..*b
            })
            .collect()
    };
    Tone {
        stereo: scale(&tone.stereo),
        mid: scale(&tone.mid),
        side: scale(&tone.side),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The weights of one contour equal the weights of a contour for each
    /// frequency (D316).
    #[test]
    fn the_weights_of_one_contour_equal_the_weights_of_a_contour_for_each() {
        let hz: Vec<f64> = (0..=4_096)
            .map(|i| i as f64 * 44_100.0 / 8_192.0)
            .chain([5.0, 20.0, 1_000.0, 12_500.0, 30_000.0])
            .collect();
        let weights = equal_loudness_weights(hz.iter().copied());
        assert_eq!(weights.len(), hz.len());
        for (weight, &hz) in weights.iter().zip(&hz) {
            let at = |hz| Contour::new(REFERENCE_PHON).at(hz);
            let each = 10f64.powf((at(1000.0) - at(hz)) / 10.0);
            assert_eq!(weight.to_bits(), each.to_bits());
        }
    }

    fn masking(index: usize, low_mid_deficit_db: f64) -> Masking {
        Masking {
            lead: Lead {
                role: "vocals",
                index,
                active_seconds: 60.0,
                band_share_db: [0.0; 4],
                centroid_hz: VOCAL_CENTROID_HZ,
                by_precedence: true,
            },
            low_mid_hz: (250.0, 500.0),
            body_hz: (500.0, 2000.0),
            share_low_mid_db: -low_mid_deficit_db,
            share_body_db: 0.0,
            low_mid_deficit_db,
        }
    }

    fn key(lead_role: usize) -> crate::low_mid::Key {
        crate::low_mid::Key {
            rule: "set.low-mid.v1",
            lead_role,
            active_frames: 100,
            keyed_frames: 20,
            excess_db: vec![(354.0, 1.0)],
            centre_hz: 354.0,
            depth_db: Vec::new(),
        }
    }

    #[test]
    fn the_low_mid_cut_is_the_deficit_of_the_lead_within_its_bounds() {
        let vocal = 3;
        assert_eq!(
            low_mid_cut_db(&masking(vocal, 1.2), Some(&key(vocal))),
            Some(1.2)
        );
        assert_eq!(
            low_mid_cut_db(&masking(vocal, 3.5), Some(&key(vocal))),
            Some(MAXIMUM_DB)
        );
        // A deficit under the smallest move, a key of another role, or no
        // key: no cut.
        assert_eq!(
            low_mid_cut_db(&masking(vocal, 0.4), Some(&key(vocal))),
            None
        );
        assert_eq!(low_mid_cut_db(&masking(vocal, 1.2), Some(&key(2))), None);
        assert_eq!(low_mid_cut_db(&masking(vocal, 1.2), None), None);
        assert_eq!(
            low_mid_cut_db(&masking(vocal, -1.0), Some(&key(vocal))),
            None
        );
    }

    #[test]
    fn a_set_without_stems_has_no_lead_and_no_cut() {
        let analysis = analyze(None);
        assert!(analysis.masking.is_none());
        assert!(analysis.low_mid_cut_db.is_none());
        assert_eq!(analysis.rule, RULE);
    }
}
