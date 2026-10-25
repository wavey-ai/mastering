//! Broad tonal moves derived from the recording itself: the change in
//! perceived balance when the master plays louder than the mix (ISO 226
//! equal-loudness contours), and the vocal's masking by the rest of the
//! arrangement measured from aligned stems. Each target comes from the
//! recording.
use crate::{
    dsp::{EqBand, Kind},
    selection::{GateChannel, Gates, TonalGate},
    stem_evidence::{Band, Evidence},
    treatment::Tone,
};
use serde::Serialize;

/// Q of the presence bells: the lead presence bell on the mid channel and
/// the presence balance bell on the stereo mix. `is_character` finds them
/// by this value.
pub const LEAD_BELL_Q: f64 = 0.7;
/// Largest single move and largest summed response, in dB.
pub const MAXIMUM_DB: f64 = 2.0;
pub const RESPONSE_DB: f64 = 3.0;
/// Smallest proposed move; smaller evidence stays unchanged.
pub const MINIMUM_DB: f64 = 0.5;
/// Assumed monitoring level of the mix, in phon. The master is heard
/// `delivered_gain_lu` above it.
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
/// a bass line and the parity premise does not hold; above 2 kHz the
/// centroid is clamped so presence stays below 10 kHz.
pub const LEAD_CENTROID_HZ: (f64, f64) = (500.0, 2000.0);
/// The vocal keeps fixed regions: 1 kHz reproduces 250–500, 500–2k, 2–5k.
pub const VOCAL_CENTROID_HZ: f64 = 1000.0;
/// Bands whose centers decide an instrumental lead's centroid.
const CENTROID_BAND_HZ: (f64, f64) = (250.0, 5000.0);

/// A moved band must change against Essential at matched level by this
/// much: 30% of the filter gain, at least 0.2 dB.
pub fn gate_minimum_db(gain_db: f64) -> f64 {
    (0.3 * gain_db.abs()).max(0.2)
}

#[derive(Clone, Copy, Serialize, PartialEq, Debug)]
pub enum Move {
    /// Low shelf that returns perceived bass at the delivered level to the
    /// mix's balance.
    LevelBass,
    /// High shelf from the same contours.
    LevelAir,
    /// Presence lift where the lead's share of the arrangement is lower in
    /// presence than in its body.
    LeadPresence,
    /// Low-mid cut where the lead's share is lower in the low mids than in
    /// its body.
    LeadLowMid,
    /// Presence lift on the stereo mix where the mix's presence is lower
    /// against its body than in a balance of equal loudness per band.
    PresenceBalance,
    /// High shelf on the stereo mix that returns the air against presence
    /// of the mix after the presence moves (`set.air-balance.v1`).
    AirBalance,
    /// Low shelves on the stereo mix that move the sub against the bass and
    /// the bass against the body of a captured recording into the range of
    /// the album masters (`set.capture-low-band.v1`).
    CaptureLowBand,
}

/// Class of a tone move (`set.move-class.v1`).
#[derive(Clone, Copy, Serialize, PartialEq, Debug)]
pub enum Class {
    /// A move that gives a version its sound: presence, treble, width and
    /// density. Vivid steps it (`vivid_tone`).
    Character,
    /// A move that repairs a fault of the mix or of the playback level. The
    /// engine measures it once for each recording, and each version that
    /// carries it uses the same move.
    Correction,
}

impl Move {
    /// The lead bells act on the mid channel; the other moves on the stereo
    /// mix.
    pub fn on_mid(self) -> bool {
        matches!(self, Move::LeadPresence | Move::LeadLowMid)
    }

    /// The equal-loudness shelves follow the delivered level, which the
    /// versions share; the low-mid cut follows the masking of the lead.
    /// The presence moves are the character of the brighter versions.
    pub fn class(self) -> Class {
        match self {
            Move::LevelBass | Move::LevelAir | Move::LeadLowMid | Move::CaptureLowBand => {
                Class::Correction
            }
            Move::LeadPresence | Move::PresenceBalance | Move::AirBalance => Class::Character,
        }
    }
}

/// True for a filter of a static tone that is a character move: a presence
/// bell (`Kind::Peak` at `LEAD_BELL_Q`). The equal-loudness shelves, the
/// focus bells of the stems and the side cut are corrections. The low-mid
/// correction and the low-end hold are separate fields of the render spec.
pub fn is_character(band: &EqBand) -> bool {
    matches!(band.kind, Kind::Peak) && band.q == LEAD_BELL_Q
}

#[derive(Clone, Serialize)]
pub struct Proposal {
    pub kind: Move,
    pub measured_db: f64,
    pub gain_db: f64,
    pub filters: Vec<EqBand>,
    pub gate: Vec<TonalGate>,
}

#[derive(Clone, Serialize)]
pub struct EqualLoudness {
    pub reference_phon: f64,
    pub delivered_gain_lu: f64,
    /// Compensation gain per ISO 226 frequency, in dB.
    pub gain_by_hz: Vec<(f64, f64)>,
    pub low_shelf_db: f64,
    pub high_shelf_db: f64,
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

#[derive(Clone, Serialize)]
pub struct Masking {
    pub lead: Lead,
    pub low_mid_hz: (f64, f64),
    pub body_hz: (f64, f64),
    pub presence_hz: (f64, f64),
    pub share_low_mid_db: f64,
    pub share_body_db: f64,
    pub share_presence_db: f64,
    pub presence_deficit_db: f64,
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
    /// Change of the ratio that the lead bells and shelves give by
    /// themselves.
    pub other_moves_db: f64,
}

#[derive(Clone, Serialize)]
pub struct Analysis {
    pub rule: &'static str,
    pub equal_loudness: Option<EqualLoudness>,
    pub masking: Option<Masking>,
    pub presence_balance: Option<PresenceBalance>,
    pub proposals: Vec<Proposal>,
    /// Moves not proposed because a band of their gate has no material
    /// energy in the source, so the gate cannot measure them (D144).
    pub unmeasurable: Vec<Move>,
    /// The stages of a captured recording (`capture::Capture`); absent for
    /// a mix.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capture: Option<crate::capture::Capture>,
}

impl Analysis {
    pub fn none() -> Self {
        Self {
            rule: "set.broad-tone.v3",
            equal_loudness: None,
            masking: None,
            presence_balance: None,
            proposals: Vec::new(),
            unmeasurable: Vec::new(),
            capture: None,
        }
    }

    /// Keep only the proposals whose gate bands are material in the source
    /// (`selection::material_bands`). A gate without a measurement fails
    /// every candidate, so an unmeasurable move would stop the whole version.
    pub fn retain_measurable(&mut self, material: [bool; 6]) {
        let measurable = |p: &Proposal| {
            p.gate
                .iter()
                .all(|g| material[g.band] && g.relative_to.is_none_or(|r| material[r]))
        };
        self.unmeasurable = self
            .proposals
            .iter()
            .filter(|p| !measurable(p))
            .map(|p| p.kind)
            .collect();
        self.proposals.retain(measurable);
    }
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

/// Compensation that keeps the perceived balance of the mix when the master
/// plays `delivered_gain_lu` louder: the SPL change each frequency needs for
/// the same loudness increase as 1 kHz, minus the change every frequency gets.
pub fn equal_loudness(delivered_gain_lu: f64) -> EqualLoudness {
    let gain_by_hz: Vec<(f64, f64)> = ISO226
        .iter()
        .map(|&(hz, af, lu, tf)| {
            let needed = contour_spl(REFERENCE_PHON + delivered_gain_lu, af, lu, tf)
                - contour_spl(REFERENCE_PHON, af, lu, tf);
            (hz, needed - delivered_gain_lu)
        })
        .collect();
    let mean = |low: f64, high: f64| {
        let values: Vec<f64> = gain_by_hz
            .iter()
            .filter(|(hz, _)| *hz >= low && *hz <= high)
            .map(|(_, g)| *g)
            .collect();
        values.iter().sum::<f64>() / values.len().max(1) as f64
    };
    EqualLoudness {
        reference_phon: REFERENCE_PHON,
        delivered_gain_lu,
        low_shelf_db: mean(40.0, 125.0).clamp(-MAXIMUM_DB, MAXIMUM_DB),
        high_shelf_db: mean(6300.0, 12500.0).clamp(-MAXIMUM_DB, MAXIMUM_DB),
        gain_by_hz,
    }
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
    presence_body_db(&spectrum.centers_hz, |i| {
        10f64.powf(contour_at(spectrum.centers_hz[i], REFERENCE_PHON) / 10.0)
    })
}

/// Presence against body of the source spectrum, in dB.
pub fn source_presence_db(spectrum: &crate::Spectrum) -> f64 {
    presence_body_db(&spectrum.centers_hz, |i| {
        spectrum.mid_power[i] + spectrum.side_power[i]
    })
}
/// The presence balance bell: centred on the presence region on a log
/// scale, as wide as the lead bells.
pub const PRESENCE_BELL_HZ: f64 = 3162.0;

/// SPL of the `phon` equal-loudness contour at `hz`, interpolated on a log
/// frequency scale between the ISO 226 frequencies.
fn contour_at(hz: f64, phon: f64) -> f64 {
    let points: Vec<(f64, f64)> = ISO226
        .iter()
        .map(|&(f, af, lu, tf)| (f.ln(), contour_spl(phon, af, lu, tf)))
        .collect();
    let x = hz.clamp(20.0, 12500.0).ln();
    points
        .windows(2)
        .find(|w| x >= w[0].0 && x <= w[1].0)
        .map_or(points.last().unwrap().1, |w| {
            w[0].1 + (w[1].1 - w[0].1) * (x - w[0].0) / (w[1].0 - w[0].0)
        })
}

/// Power weight of `hz` at the `REFERENCE_PHON` monitoring level, against
/// 1 kHz: the inverse of the equal-loudness contour, as A- and B-weighting
/// are inverse contours at lower levels.
pub fn equal_loudness_weight(hz: f64) -> f64 {
    10f64.powf((contour_at(1000.0, REFERENCE_PHON) - contour_at(hz, REFERENCE_PHON)) / 10.0)
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

fn shelf(kind: Kind, hz: f64, gain_db: f64) -> EqBand {
    EqBand {
        kind,
        hz,
        q: std::f64::consts::FRAC_1_SQRT_2,
        gain_db,
    }
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
    let (low_mid_hz, body_hz, presence_hz) =
        ((c / 4.0, c / 2.0), (c / 2.0, 2.0 * c), (2.0 * c, 5.0 * c));
    let share = |(low, high): (f64, f64)| {
        mean_share_db(evidence, low, high, |b| {
            b.role_active_power_shares[lead.index]
        })
    };
    let (low_mid, body, presence) = (share(low_mid_hz), share(body_hz), share(presence_hz));
    Some(Masking {
        lead,
        low_mid_hz,
        body_hz,
        presence_hz,
        share_low_mid_db: low_mid,
        share_body_db: body,
        share_presence_db: presence,
        presence_deficit_db: body - presence,
        low_mid_deficit_db: body - low_mid,
    })
}

/// Index of the `stereo::BAND_EDGES_HZ` band that contains `hz`.
fn band_index(hz: f64) -> usize {
    let edges = crate::stereo::BAND_EDGES_HZ;
    edges
        .windows(2)
        .position(|e| hz >= e[0] && hz < e[1])
        .unwrap_or(edges.len() - 2)
}

/// Gate a bell at `bell_hz` against the band that holds the lead's centroid.
/// When both fall in one band, the reference is the next band toward the
/// body: above the bell for a low-mid cut, below it for a presence lift.
/// Gate of a lead bell, measured on the mid channel that carries it.
fn region_gate(bell_hz: f64, centroid_hz: f64, gain_db: f64) -> Vec<TonalGate> {
    let band = band_index(bell_hz);
    let mut relative_to = band_index(centroid_hz);
    if relative_to == band {
        relative_to = if bell_hz < centroid_hz {
            band + 1
        } else {
            band - 1
        };
    }
    let mut gates = gate(band, relative_to, gain_db);
    for g in &mut gates {
        g.channel = GateChannel::Mid;
    }
    gates
}

fn gate(band: usize, relative_to: usize, gain_db: f64) -> Vec<TonalGate> {
    vec![TonalGate {
        band,
        direction: if gain_db > 0.0 { 1 } else { -1 },
        minimum_db: gate_minimum_db(gain_db),
        relative_to: Some(relative_to),
        channel: GateChannel::Stereo,
    }]
}

/// Propose the recording's own moves. `delivered_gain_lu` is the base target
/// minus the source loudness. `capture` is true for a captured recording
/// (`source_condition::SourceCondition::device_capture`): its presence follows
/// `set.capture-presence.v1` and its low band `set.capture-low-band.v1`.
pub fn analyze(
    evidence: Option<&Evidence>,
    delivered_gain_lu: f64,
    spectrum: Option<&crate::Spectrum>,
    side: Option<&crate::width::SideGain>,
    top_hz: f64,
    capture: bool,
) -> Analysis {
    let mut proposals = Vec::new();
    let equal = equal_loudness(delivered_gain_lu.max(0.0));
    if equal.low_shelf_db.abs() >= MINIMUM_DB {
        let gain = equal.low_shelf_db;
        proposals.push(Proposal {
            kind: Move::LevelBass,
            measured_db: gain,
            gain_db: gain,
            filters: vec![shelf(Kind::LowShelf, 150.0, gain)],
            gate: gate(0, 2, gain),
        });
    }
    if equal.high_shelf_db.abs() >= MINIMUM_DB {
        let gain = equal.high_shelf_db;
        proposals.push(Proposal {
            kind: Move::LevelAir,
            measured_db: gain,
            gain_db: gain,
            filters: vec![shelf(Kind::HighShelf, 6000.0, gain)],
            gate: gate(5, 3, gain),
        });
    }
    // The low band of a captured recording: low shelves to the range of the
    // album masters. They precede the presence moves, which count them. The
    // shelves act on the held bands, so the low-end gate checks them in each
    // section with the hold, as it checks the bass shelves of a bass version
    // (`set.bass-version.v4`); a tonal gate on one band would also measure
    // the other shelf.
    let capture = capture
        .then(|| spectrum.map(crate::capture::Capture::of))
        .flatten();
    if let Some(c) = &capture {
        let band = &c.low_band;
        let shelves = band.shelves();
        if !shelves.is_empty() {
            let (gain, measured) = if band.sub_db.abs() >= band.bass_db.abs() {
                (band.sub_db, band.wanted_sub_minus_bass_db)
            } else {
                (band.bass_db, band.wanted_bass_minus_body_db)
            };
            proposals.push(Proposal {
                kind: Move::CaptureLowBand,
                measured_db: measured,
                gain_db: gain,
                filters: shelves,
                gate: Vec::new(),
            });
        }
    }
    let masking = evidence.and_then(masking);
    if let Some(m) = &masking {
        // Restore the lead's share to parity with its body; the Bold version
        // goes past parity and Essential stays at the mix. The whole tone's
        // presence against body stays within its bound
        // (`presence_move_maximum_db`), so the lead bell takes no more than
        // the bound leaves after the shelves and the widening.
        let c = m.lead.centroid_hz;
        let mut presence = m.presence_deficit_db.clamp(0.0, MAXIMUM_DB);
        if let Some(spectrum) = spectrum.filter(|_| presence >= MINIMUM_DB) {
            let stereo: Vec<EqBand> = proposals.iter().flat_map(|p| p.filters.clone()).collect();
            let source = source_presence_db(spectrum);
            let bound = move_maximum_db(source, reference_presence_db(spectrum), capture.is_some());
            let moved = |gain: f64| {
                predicted_db(spectrum, &stereo, &[bell(3.2 * c, LEAD_BELL_Q, gain)], side) - source
            };
            if moved(presence) > bound {
                let (mut low, mut high) = (0.0, presence);
                for _ in 0..40 {
                    let middle = 0.5 * (low + high);
                    if moved(middle) > bound {
                        high = middle;
                    } else {
                        low = middle;
                    }
                }
                presence = low;
            }
        }
        if presence >= MINIMUM_DB {
            proposals.push(Proposal {
                kind: Move::LeadPresence,
                measured_db: m.presence_deficit_db,
                gain_db: presence,
                filters: vec![bell(3.2 * c, LEAD_BELL_Q, presence)],
                gate: region_gate(3.2 * c, c, presence),
            });
        }
        // The low-mid cut is dynamic: it applies in the frames where the
        // other stems mask the lead (`low_mid::key`), up to the cut that the
        // whole-track deficit gives. It has no static gate.
        let low_mid = -m.low_mid_deficit_db.clamp(0.0, MAXIMUM_DB);
        let key = evidence
            .and_then(|e| e.low_mid.as_ref())
            .filter(|k| k.lead_role == m.lead.index);
        if let Some(key) = key.filter(|_| low_mid.abs() >= MINIMUM_DB) {
            let hz = key.centre_hz;
            proposals.push(Proposal {
                kind: Move::LeadLowMid,
                measured_db: m.low_mid_deficit_db,
                gain_db: low_mid,
                filters: vec![bell(hz, crate::low_mid::q(hz), low_mid)],
                gate: Vec::new(),
            });
        }
    }
    // The presence balance counts the static moves above toward its
    // target; the dynamic low-mid cut is off in the frames without masking.
    let presence_balance = spectrum.map(|spectrum| {
        let (stereo, mid): (Vec<EqBand>, Vec<EqBand>) = {
            let (lead, other): (Vec<&Proposal>, Vec<&Proposal>) = proposals
                .iter()
                .filter(|p| p.kind != Move::LeadLowMid)
                .partition(|p| p.kind.on_mid());
            (
                other.iter().flat_map(|p| p.filters.clone()).collect(),
                lead.iter().flat_map(|p| p.filters.clone()).collect(),
            )
        };
        let (balance, filter) = presence_balance(spectrum, &stereo, &mid, side, capture.is_some());
        if let Some(filter) = filter {
            proposals.push(Proposal {
                kind: Move::PresenceBalance,
                measured_db: balance.reference_db - balance.source_db,
                gain_db: filter.gain_db,
                filters: vec![filter],
                gate: gate(
                    band_index(PRESENCE_BELL_HZ),
                    band_index(PRESENCE_BELL_HZ) - 1,
                    filter.gain_db,
                ),
            });
        }
        balance
    });
    // The air balance: the presence moves lower the air against presence
    // at matched loudness; a high shelf returns the balance of the mix, so
    // the top of the mix keeps its own proportion. The shelf follows a
    // prediction and has no gate: the air band of a full-range mix holds
    // under 1% of the power of its strongest band (D139). A source whose
    // codec cuts the air band gets no shelf.
    if let Some(spectrum) = spectrum.filter(|_| top_hz >= AIR_HZ.1) {
        let (lead, other): (Vec<&Proposal>, Vec<&Proposal>) = proposals
            .iter()
            .filter(|p| p.kind != Move::LeadLowMid)
            .partition(|p| p.kind.on_mid());
        let stereo: Vec<EqBand> = other.iter().flat_map(|p| p.filters.clone()).collect();
        let mid: Vec<EqBand> = lead.iter().flat_map(|p| p.filters.clone()).collect();
        let target = shaped_ratio_db(spectrum, &[], &[], None, AIR_HZ, PRESENCE_HZ);
        let before = shaped_ratio_db(spectrum, &stereo, &mid, side, AIR_HZ, PRESENCE_HZ);
        if let Some(g) =
            air_gain(spectrum, &stereo, &mid, side, target).filter(|g| *g >= MINIMUM_DB)
        {
            proposals.push(Proposal {
                kind: Move::AirBalance,
                measured_db: target - before,
                gain_db: g,
                filters: vec![air_shelf(g)],
                gate: Vec::new(),
            });
        }
    }
    Analysis {
        rule: "set.broad-tone.v6",
        equal_loudness: Some(equal),
        masking,
        presence_balance,
        proposals,
        unmeasurable: Vec::new(),
        capture,
    }
}

/// The joint tonal proposal: every move in one tone, bounded in summed
/// response.
#[derive(Clone, Serialize)]
pub struct Master {
    pub moves: Vec<Move>,
    /// The static moves.
    pub tone: Tone,
    /// Largest cut of the dynamic low-mid correction, in dB, when the
    /// recording has one.
    pub low_mid_cut_db: Option<f64>,
    pub gates: Gates,
}

/// The tone of the master from `analysis`, with filters at `rate`.
pub fn master(analysis: &Analysis, rate: u32) -> Option<Master> {
    if analysis.proposals.is_empty() {
        return None;
    }
    // The lead bells go on the mid channel, where a centred lead gains on
    // the side content of the rest; a stereo bell lifts the lead and the
    // rest alike. The equal-loudness shelves stay on the stereo mix (D128).
    let statics: Vec<&Proposal> = analysis
        .proposals
        .iter()
        .filter(|p| p.kind != Move::LeadLowMid)
        .collect();
    let filters: Vec<EqBand> = statics.iter().flat_map(|p| p.filters.clone()).collect();
    let (lead, other): (Vec<&Proposal>, Vec<&Proposal>) =
        statics.iter().partition(|p| p.kind.on_mid());
    let mut tone = Tone {
        stereo: other.iter().flat_map(|p| p.filters.clone()).collect(),
        mid: lead.iter().flat_map(|p| p.filters.clone()).collect(),
        ..Tone::default()
    };
    // The summed response bound applies to the mid channel, which carries
    // both the shelves and the bells.
    let peak = (0..256)
        .map(|i| {
            let hz = 20.0 * 1000f64.powf(i as f64 / 255.0);
            filters
                .iter()
                .map(|b| b.filter(rate).response_db(hz, rate))
                .sum::<f64>()
                .abs()
        })
        .fold(0.0, f64::max);
    // A captured recording keeps the summed response of its presence bell
    // (`set.capture-presence.v1`).
    let largest = if analysis.capture.is_some() {
        crate::capture::PRESENCE_BELL_MAXIMUM_DB
    } else {
        RESPONSE_DB
    };
    if peak > largest {
        tone = scaled(&tone, largest / peak);
    }
    Some(Master {
        moves: analysis.proposals.iter().map(|p| p.kind).collect(),
        tone,
        low_mid_cut_db: analysis
            .proposals
            .iter()
            .find(|p| p.kind == Move::LeadLowMid)
            .map(|p| -p.gain_db),
        gates: Gates {
            tonal: analysis
                .proposals
                .iter()
                .flat_map(|p| p.gate.clone())
                .collect(),
            tonal_image: true,
            ..Gates::default()
        },
    })
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

    #[test]
    fn lead_bells_go_on_the_mid_channel_and_shelves_on_the_mix() {
        let mut analysis = Analysis::none();
        let proposal = |kind, filter: EqBand| Proposal {
            kind,
            measured_db: filter.gain_db,
            gain_db: filter.gain_db,
            filters: vec![filter],
            gate: Vec::new(),
        };
        analysis.proposals = vec![
            proposal(Move::LevelBass, shelf(Kind::LowShelf, 150.0, -0.8)),
            proposal(Move::LeadPresence, bell(3200.0, LEAD_BELL_Q, 1.5)),
            proposal(Move::LeadLowMid, bell(350.0, 1.4, -1.0)),
        ];
        let master = master(&analysis, 48000).unwrap();
        assert_eq!(master.tone.stereo.len(), 1);
        assert!(matches!(master.tone.stereo[0].kind, Kind::LowShelf));
        // The low-mid cut is dynamic and stays out of the static tone.
        assert_eq!(master.tone.mid.len(), 1);
        assert!(master.tone.mid.iter().all(|b| b.q == LEAD_BELL_Q));
        assert_eq!(master.low_mid_cut_db, Some(1.0));
        assert!(master.tone.side.is_empty());
        assert_eq!(Move::LeadLowMid.class(), Class::Correction);
        assert_eq!(Move::LevelBass.class(), Class::Correction);
        assert_eq!(Move::LeadPresence.class(), Class::Character);
        assert!(is_character(&master.tone.mid[0]));
        assert!(!is_character(&master.tone.stereo[0]));
    }

    #[test]
    fn the_air_follows_the_presence_lift() {
        // Noise through two one-pole low-passes at about 380 Hz: its
        // spectrum falls by 12 dB per octave, so its presence is low
        // against its body in log-width bands.
        let mut seed = 11u32;
        let mut state = [[0.0f32; 2]; 2];
        let noise: Vec<crate::audio_io::Stereo> = (0..96_000)
            .map(|_| {
                std::array::from_fn(|c| {
                    seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                    let white = (seed >> 8) as f32 / 16_777_216.0 - 0.5;
                    state[0][c] = 0.95 * state[0][c] + 0.05 * white;
                    state[1][c] = 0.95 * state[1][c] + 0.05 * state[0][c];
                    state[1][c]
                })
            })
            .collect();
        let spectrum = crate::spectrum(&noise, 48000);
        let analysis = analyze(None, 0.0, Some(&spectrum), None, 24_000.0, false);
        let kinds: Vec<Move> = analysis.proposals.iter().map(|p| p.kind).collect();
        assert!(kinds.contains(&Move::PresenceBalance), "{kinds:?}");
        assert!(kinds.contains(&Move::AirBalance), "{kinds:?}");
        assert_eq!(Move::AirBalance.class(), Class::Character);
        // With every static move, the air against presence is that of the
        // source, or the shelf is at its largest gain.
        let stereo: Vec<EqBand> = analysis
            .proposals
            .iter()
            .flat_map(|p| p.filters.clone())
            .collect();
        let source = shaped_ratio_db(&spectrum, &[], &[], None, AIR_HZ, PRESENCE_HZ);
        let after = shaped_ratio_db(&spectrum, &stereo, &[], None, AIR_HZ, PRESENCE_HZ);
        let shelf = analysis
            .proposals
            .iter()
            .find(|p| p.kind == Move::AirBalance)
            .unwrap()
            .gain_db;
        assert!(
            (after - source).abs() < 0.01 || shelf == MAXIMUM_DB,
            "{after} {source}"
        );
    }

    /// A spectrum with the sub (under 80 Hz), the bass (80 to 250 Hz) and
    /// the presence at the given levels against the rest, in dB.
    fn regions(sub_db: f64, bass_db: f64, presence_db: f64) -> crate::Spectrum {
        let centers: Vec<f64> = (0..200)
            .map(|i| 20.0 * 1000f64.powf(i as f64 / 199.0))
            .collect();
        let power: Vec<f64> = centers
            .iter()
            .map(|&hz| {
                let db = if hz < crate::open::BASS_HZ.0 {
                    sub_db
                } else if hz < crate::open::BASS_HZ.1 {
                    bass_db
                } else if (PRESENCE_HZ.0..PRESENCE_HZ.1).contains(&hz) {
                    presence_db
                } else {
                    0.0
                };
                10f64.powf(db / 10.0)
            })
            .collect();
        crate::Spectrum {
            sample_rate: 48000,
            centers_hz: centers,
            mid_power: power.clone(),
            side_power: vec![0.0; power.len()],
            band_power: power,
            mel_power: Vec::new(),
            analyzed_frames: 1,
        }
    }

    fn proposal(analysis: &Analysis, kind: Move) -> Option<&Proposal> {
        analysis.proposals.iter().find(|p| p.kind == kind)
    }

    #[test]
    fn a_dark_capture_takes_a_larger_presence_move_than_a_mix() {
        // Presence 12 dB under the body; the low band inside the range of
        // the album masters.
        let spectrum = regions(7.0, 5.0, -12.0);
        let mix = analyze(None, 0.0, Some(&spectrum), None, 24_000.0, false);
        let capture = analyze(None, 0.0, Some(&spectrum), None, 24_000.0, true);
        let gain = |a: &Analysis| proposal(a, Move::PresenceBalance).unwrap().gain_db;
        assert!(gain(&mix) <= RESPONSE_DB);
        assert!(gain(&capture) > gain(&mix) + 0.5, "{}", gain(&capture));
        assert!(gain(&capture) <= crate::capture::PRESENCE_BELL_MAXIMUM_DB);
        // The capture moves presence against body to the darkest album
        // master, or as far as its largest bell reaches.
        let source = source_presence_db(&spectrum);
        let filters = &proposal(&capture, Move::PresenceBalance).unwrap().filters;
        let moved = predicted_db(&spectrum, filters, &[], None) - source;
        let wanted = PRESENCE_OUTCOME_MINIMUM_DB - source;
        assert!(
            (moved - wanted).abs() < 0.05
                || gain(&capture) == crate::capture::PRESENCE_BELL_MAXIMUM_DB,
            "{moved} against {wanted}"
        );
        // The summed response bound of a capture keeps the bell.
        let tone = master(&capture, 48000).unwrap().tone;
        assert_eq!(tone.stereo[0].gain_db, gain(&capture));
        assert!(serde_json::to_value(&mix).unwrap().get("capture").is_none());
        assert!(proposal(&capture, Move::CaptureLowBand).is_none());
    }

    #[test]
    fn a_thin_capture_takes_low_shelves_and_a_mix_does_not() {
        // The sub 6 dB under the bass; presence at the body level.
        let spectrum = regions(-1.0, 5.0, 0.0);
        let mix = analyze(None, 0.0, Some(&spectrum), None, 24_000.0, false);
        let capture = analyze(None, 0.0, Some(&spectrum), None, 24_000.0, true);
        assert!(proposal(&mix, Move::CaptureLowBand).is_none());
        let low = proposal(&capture, Move::CaptureLowBand).unwrap();
        assert!(low
            .filters
            .iter()
            .all(|b| matches!(b.kind, Kind::LowShelf) && b.gain_db > 0.0));
        assert!(low.gate.is_empty());
        assert_eq!(Move::CaptureLowBand.class(), Class::Correction);
        assert!(!Move::CaptureLowBand.on_mid());
        assert!(proposal(&capture, Move::PresenceBalance).is_none());
    }

    #[test]
    fn louder_playback_compensates_bass_more_than_treble() {
        let e = equal_loudness(4.0);
        assert!(
            e.low_shelf_db < 0.0,
            "bass gains loudness faster: {}",
            e.low_shelf_db
        );
        assert!(e.low_shelf_db.abs() > e.high_shelf_db.abs());
        let zero = equal_loudness(0.0);
        assert!(zero.low_shelf_db.abs() < 1e-9);
    }
    #[test]
    fn a_move_whose_gate_band_is_not_material_is_not_proposed() {
        let mut analysis = analyze(None, 20.0, None, None, 24_000.0, false);
        assert!(analysis
            .proposals
            .iter()
            .any(|p| matches!(p.kind, Move::LevelAir)));
        // The air band (12 to 20 kHz) carries no material energy.
        analysis.retain_measurable([true, true, true, true, true, false]);
        assert!(!analysis
            .proposals
            .iter()
            .any(|p| matches!(p.kind, Move::LevelAir)));
        assert!(analysis
            .unmeasurable
            .iter()
            .any(|m| matches!(m, Move::LevelAir)));
        assert!(analysis
            .proposals
            .iter()
            .any(|p| matches!(p.kind, Move::LevelBass)));
    }
}
