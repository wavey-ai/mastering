//! Low-end hold of Clear and Vivid against Natural (`set.low-end.v3`).
//!
//! The versions share one loudness target. The presence lift and the
//! widening of Clear and Vivid raise their loudness, so at the same target
//! their bass is lower than the bass of Natural; a correction or a side cut
//! can also take bass. After the render, the engine measures each 15 s
//! section of the version against Natural at matched integrated loudness,
//! from 40 to 120 Hz and from 120 to 250 Hz. In a section where a band is
//! more than `LIMIT_DB` below Natural:
//!
//! 1. The low-mid correction decreases in that section by the share of the
//!    deficit that it causes.
//! 2. One bell for each held band returns the rest to `TARGET_DB`, by the
//!    amount that each section needs. The envelope has that amount at the
//!    centre of the section and moves to the amount of the next section over
//!    the distance between their centres. A bell has at most
//!    `HOLD_MAXIMUM_DB`.
//!
//! The lift of a bass bell adds loudness, and at the same target the
//! loudness match takes it from every band: the bells change the bass
//! against the rest of the version and keep the balance of the other bands
//! (`set.low-end.v3`, D185). The hold returns a band to the middle of the
//! allowed distance and no further. It lifts no held band above Natural by
//! more than `LIMIT_DB`; a band already within `SPILL_DB` of that limit, or
//! past it, rises by at most `SPILL_DB` (D183). A version that stays below
//! after `ITERATIONS` renders fails its check, and its ladder continues:
//! Vivid falls back to the tone of Clear.
use crate::{
    audio_io::Stereo,
    dsp::{self, EqBand, Kind},
    finishing::Domain,
    treatment::{BlendPoint, EnvelopedBand},
    treatment_render::Spec,
};
use serde::{Deserialize, Serialize};

/// The hold of one version: the factor of the low-mid correction in each
/// changed section and the hold bells.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hold {
    pub rule: String,
    /// Start sample, end sample and factor of the correction in each
    /// section, or none when the correction keeps its strength.
    pub correction_factors: Vec<(usize, usize, f64)>,
    /// The bells of the held bands that the hold uses.
    pub bands: Vec<EnvelopedBand>,
}

impl Hold {
    /// `correction` with the factor of the sections on its points. The
    /// factor moves from the centre of one section to the centre of the next
    /// on a raised cosine, as the hold bells do.
    pub fn scaled(&self, correction: &EnvelopedBand) -> EnvelopedBand {
        let mut c = correction.clone();
        if self.correction_factors.is_empty() {
            return c;
        }
        let points: Vec<BlendPoint> = self
            .correction_factors
            .iter()
            .map(|&(start, end, f)| BlendPoint {
                sample: (start + end) / 2,
                wet: f,
            })
            .collect();
        for p in &mut c.points {
            p.wet *= glide(&points, p.sample);
        }
        c
    }

    /// Validate the hold of a version.
    pub fn validate(&self) -> crate::audio_io::Result<()> {
        let maximum = HOLD_MAXIMUM_DB;
        if self
            .correction_factors
            .iter()
            .any(|(start, end, f)| start >= end || !(0.0..=1.0).contains(f))
            || self.bands.len() > BANDS_HZ.len()
            || self
                .bands
                .iter()
                .any(|b| !matches!(b.band.kind, Kind::Peak) || b.band.gain_db.abs() > maximum)
        {
            return Err("Invalid low-end hold".into());
        }
        Ok(())
    }
}

/// The rule of the hold.
pub const RULE: &str = "set.low-end.v3";
pub const SECTION_SECONDS: f64 = 15.0;
/// The held bands, in Hz.
pub const BANDS_HZ: [(f64, f64); 2] = [(40.0, 120.0), (120.0, 250.0)];

/// True when band `band` of `stereo::BAND_EDGES_HZ` overlaps a held band:
/// the hold moves it.
pub fn lifts(band: usize) -> bool {
    let edges = crate::stereo::BAND_EDGES_HZ;
    BANDS_HZ
        .iter()
        .any(|&(low, high)| edges[band] < high && edges[band + 1] > low)
}
/// Largest distance of a held band from Natural, in dB.
pub const LIMIT_DB: f64 = 0.5;
/// The hold returns a failing band to this change against Natural: the
/// middle of the allowed distance.
pub const TARGET_DB: f64 = -0.5 * LIMIT_DB;
/// Largest rise of the hold in a held band that is already within this of
/// the limit, or beyond it, in dB: half the limit, under the smallest band
/// change that the set counts as audible (0.5 dB). Another move of the
/// version can put a band past the limit, such as the widened low mids of
/// a side-heavy outro; the bell of the band below it still lifts (D183).
pub const SPILL_DB: f64 = 0.5 * LIMIT_DB;
/// Largest gain of a hold bell, as of every broad tone move
/// (`broad_tone::MAXIMUM_DB`).
pub const HOLD_MAXIMUM_DB: f64 = crate::broad_tone::MAXIMUM_DB;

/// Largest gain of a hold bell of `spec`: `HOLD_MAXIMUM_DB`. The low
/// shelves of a bass version are static moves that the hold counts as
/// intended (`intended_db`), so the bells carry only the correction
/// (`set.bass-version.v4`).
pub fn bell_maximum_db(_spec: &Spec) -> f64 {
    HOLD_MAXIMUM_DB
}

/// The hold bell of held band `b` at `HOLD_MAXIMUM_DB`: centred on the band
/// on a log scale, with the band as its bandwidth. One shelf for both bands
/// cannot lift one band alone: a 150 Hz shelf raises 120 to 250 Hz by about
/// 0.1 dB per dB at matched loudness.
pub fn hold_band(b: usize) -> EqBand {
    hold_band_at(b, HOLD_MAXIMUM_DB)
}

/// The hold bell of held band `b` at `gain_db`.
pub fn hold_band_at(b: usize, gain_db: f64) -> EqBand {
    let (low, high) = BANDS_HZ[b];
    let hz = (low * high).sqrt();
    EqBand {
        kind: Kind::Peak,
        hz,
        q: hz / (high - low),
        gain_db,
    }
}
/// A section counts for a band when the band in Natural is within this of
/// its loudest section, and when the section is half a section long or
/// longer.
pub const ACTIVE_DB: f64 = 30.0;
/// Renders of the hold after the first render.
pub const ITERATIONS: usize = 2;

fn section_frames(rate: u32) -> usize {
    (SECTION_SECONDS * rate as f64) as usize
}

/// Band power of each section of a render, in dB.
#[derive(Clone, Serialize, Default)]
pub struct Sections {
    pub starts: Vec<usize>,
    pub ends: Vec<usize>,
    pub power_db: Vec<[f64; 2]>,
}

/// Stereo power of the held bands over all sections of a render against
/// its integrated loudness `lufs`, in dB: the bass that the hold lifts, at
/// matched loudness. Only differences between renders of one source carry
/// meaning (`set.version-names.v1`).
pub fn bass_level_db(sections: &Sections, lufs: f64) -> Option<f64> {
    let power: f64 = sections
        .power_db
        .iter()
        .flatten()
        .map(|p| 10f64.powf(p / 10.0))
        .sum();
    (power > 0.0 && lufs.is_finite()).then(|| 10.0 * power.log10() - lufs)
}

/// Stereo power of the held bands in each section of `audio` at `rate`.
pub fn sections(audio: &[Stereo], rate: u32) -> Sections {
    let span = section_frames(rate);
    let mut planner = realfft::RealFftPlanner::<f64>::new();
    let mut result = Sections::default();
    let mut start = 0;
    while start < audio.len() {
        let end = (start + span).min(audio.len());
        if end - start < span / 2 {
            break;
        }
        let n = end - start;
        let fft = planner.plan_fft_forward(n);
        let mut power = [0.0f64; 2];
        for channel in 0..2 {
            let mut input: Vec<f64> = audio[start..end]
                .iter()
                .map(|f| f[channel] as f64)
                .collect();
            let mut output = fft.make_output_vec();
            fft.process(&mut input, &mut output)
                .expect("section length");
            for (bin, value) in output.iter().enumerate() {
                let hz = bin as f64 * rate as f64 / n as f64;
                for (b, (low, high)) in BANDS_HZ.iter().enumerate() {
                    if hz >= *low && hz < *high {
                        power[b] += value.norm_sqr();
                    }
                }
            }
        }
        result.starts.push(start);
        result.ends.push(end);
        result
            .power_db
            .push(power.map(|p| 10.0 * (p / n as f64).max(1e-30).log10()));
        start = end;
    }
    result
}

/// The change of each held band that the static tone of `spec` intends: the
/// low shelves of the static tone, the equal-loudness bass shelf
/// (`broad_tone::Move::LevelBass`), the shelves of a captured recording
/// (`broad_tone::Move::CaptureLowBand`) and the bass shelves of a bass
/// version (`set.bass-version.v4`), at matched loudness. The hold keeps the
/// version there, not at Natural.
pub fn intended_db(spec: &Spec, spectrum: &crate::Spectrum) -> [f64; 2] {
    tone_intended_db(&spec.tone, spectrum)
}

fn tone_intended_db(tone: &crate::treatment::Tone, spectrum: &crate::Spectrum) -> [f64; 2] {
    tone.stereo
        .iter()
        .filter(|b| matches!(b.kind, Kind::LowShelf))
        .map(|b| predicted_db(spectrum, b, Domain::Stereo))
        .fold([0.0; 2], |a, p| [a[0] + p[0], a[1] + p[1]])
}

/// `intended_db` of `spec` against a reference render with the tone
/// `reference`: the change of each held band beyond the low shelves of the
/// reference. Each version of a set has its own low shelves, Essential
/// included (`set.version-character.v1`).
pub fn intended_against_db(
    spec: &Spec,
    reference: Option<&crate::treatment::Tone>,
    spectrum: &crate::Spectrum,
) -> [f64; 2] {
    let version = intended_db(spec, spectrum);
    let reference = reference.map_or([0.0; 2], |t| tone_intended_db(t, spectrum));
    [version[0] - reference[0], version[1] - reference[1]]
}

/// Change of each held band of `version` against `natural` in each section
/// at matched loudness, less the `intended` change, in dB. `None` for a
/// section that does not count.
pub fn changes(
    version: &Sections,
    version_lufs: f64,
    natural: &Sections,
    natural_lufs: f64,
    intended: [f64; 2],
) -> Vec<[Option<f64>; 2]> {
    let loudest: [f64; 2] = std::array::from_fn(|b| {
        natural
            .power_db
            .iter()
            .map(|p| p[b])
            .fold(f64::NEG_INFINITY, f64::max)
    });
    (0..natural.power_db.len().min(version.power_db.len()))
        .map(|s| {
            std::array::from_fn(|b| {
                (natural.power_db[s][b] >= loudest[b] - ACTIVE_DB).then(|| {
                    version.power_db[s][b]
                        - natural.power_db[s][b]
                        - (version_lufs - natural_lufs)
                        - intended[b]
                })
            })
        })
        .collect()
}

/// The rise that returns each band of a section under `-LIMIT_DB` to
/// `TARGET_DB`, in dB; 0 for a band that passes.
fn needed(change: &[Option<f64>; 2]) -> [f64; 2] {
    change.map(|c| c.filter(|c| *c < -LIMIT_DB).map_or(0.0, |c| TARGET_DB - c))
}

pub fn passes(changes: &[[Option<f64>; 2]]) -> bool {
    changes.iter().all(|c| needed(c) == [0.0, 0.0])
}

pub fn worst_db(changes: &[[Option<f64>; 2]]) -> Option<f64> {
    changes
        .iter()
        .flatten()
        .flatten()
        .copied()
        .min_by(f64::total_cmp)
}

/// Change of each held band, and of the K-weighted loudness, when `band`
/// acts at full strength on `domain`, from the source spectrum, in dB.
pub fn effect(spectrum: &crate::Spectrum, band: &EqBand, domain: Domain) -> ([f64; 2], f64) {
    let (mut held, mut moved) = ([0.0f64; 2], [0.0f64; 2]);
    let (mut weighted, mut weighted_moved) = (0.0, 0.0);
    let rate = spectrum.sample_rate;
    let filter = band.filter(rate);
    for (i, &hz) in spectrum.centers_hz.iter().enumerate() {
        let gain = 10f64.powf(filter.response_db(hz, rate) / 10.0);
        let (mid, side) = (spectrum.mid_power[i], spectrum.side_power[i]);
        let after = match domain {
            Domain::Stereo => (mid + side) * gain,
            Domain::Mid => mid * gain + side,
            Domain::Side => mid + side * gain,
        };
        let k = dsp::k_weighting_power(hz, rate);
        weighted += k * (mid + side);
        weighted_moved += k * after;
        for (b, (low, high)) in BANDS_HZ.iter().enumerate() {
            if hz >= *low && hz < *high {
                held[b] += mid + side;
                moved[b] += after;
            }
        }
    }
    let loudness = 10.0 * (weighted_moved / weighted.max(1e-30)).log10();
    (
        std::array::from_fn(|b| {
            if held[b] > 0.0 {
                10.0 * (moved[b] / held[b]).log10()
            } else {
                0.0
            }
        }),
        loudness,
    )
}

/// `spectrum` after the static tone and the widening of `spec`. The bells of
/// the hold act on this spectrum: a bass shelf makes the bass a larger share
/// of the loudness, so the loudness match takes more of each lift than the
/// source spectrum predicts (D185).
pub fn shaped_spectrum(spectrum: &crate::Spectrum, spec: &Spec) -> crate::Spectrum {
    let tone = spec.mean_tone();
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
    let mut shaped = spectrum.clone();
    for i in 0..shaped.centers_hz.len() {
        let hz = shaped.centers_hz[i];
        let stereo = response(&tone.stereo, hz);
        let widening = spec
            .side_gain
            .as_ref()
            .map_or(1.0, |g| 10f64.powf(g.response_db(hz) / 10.0));
        shaped.mid_power[i] *= stereo * response(&tone.mid, hz);
        // The fold of a preset raises the mid (`adjust::folded`).
        if let Some(gain) = spec.side_gain.as_ref().filter(|g| g.has_mid_gain()) {
            shaped.mid_power[i] *= gain.mid_power(hz);
        }
        shaped.side_power[i] *= stereo * response(&tone.side, hz) * widening;
        shaped.band_power[i] = shaped.mid_power[i] + shaped.side_power[i];
    }
    shaped
}

/// Predicted change of each held band at matched K-weighted loudness when
/// `band` acts at full strength on `domain`.
pub fn predicted_db(spectrum: &crate::Spectrum, band: &EqBand, domain: Domain) -> [f64; 2] {
    let (held, loudness) = effect(spectrum, band, domain);
    held.map(|h| h - loudness)
}

/// The value at `sample` of points that a raised cosine joins, constant
/// before the first point and after the last.
fn glide(points: &[BlendPoint], sample: usize) -> f64 {
    let i = points.partition_point(|p| p.sample <= sample);
    if i == 0 {
        return points[0].wet;
    }
    if i == points.len() {
        return points[i - 1].wet;
    }
    let (a, b) = (&points[i - 1], &points[i]);
    let t = (sample - a.sample) as f64 / (b.sample - a.sample).max(1) as f64;
    a.wet + (0.5 - 0.5 * (std::f64::consts::PI * t).cos()) * (b.wet - a.wet)
}

/// Hold envelope from one wet share per section: a point at the centre of
/// each section, joined by raised cosines. A change from one section to the
/// next takes the whole distance between their centres, so it does not
/// sound as a step at a section boundary, which follows no event in the
/// music.
fn hold_points(sections: &Sections, wet: &[f64], frames: usize) -> Vec<BlendPoint> {
    let mut points = vec![BlendPoint {
        sample: 0,
        wet: wet.first().copied().unwrap_or(0.0),
    }];
    for ((&start, &end), &share) in sections.starts.iter().zip(&sections.ends).zip(wet) {
        let sample = (start + end) / 2;
        if sample > points.last().unwrap().sample && sample < frames - 1 {
            points.push(BlendPoint { sample, wet: share });
        }
    }
    points.push(BlendPoint {
        sample: frames - 1,
        wet: wet.last().copied().unwrap_or(0.0),
    });
    points
}

/// State of the hold between renders.
#[derive(Clone, Serialize)]
pub struct Step {
    pub render: usize,
    pub worst_db: Option<f64>,
    pub failing_sections: Vec<usize>,
    /// Factor of the low-mid correction in each section.
    pub correction_factor: Vec<f64>,
    /// Wet share of each hold bell in each section.
    pub hold_wet: Vec<[f64; 2]>,
    /// Wet share that this step added.
    pub added: Vec<[f64; 2]>,
    /// Predicted change of each held band from this step at matched
    /// loudness, in dB: the rise of its bells and of the reduced correction
    /// in the section, less the loudness offset of all its lifts
    /// (`loudness_offset_db`).
    pub expected_db: Vec<[f64; 2]>,
    /// The loudness offset of the lifts of this step, in dB.
    pub offset_db: f64,
    /// The changes that the step answers (`changes`): the first step's are
    /// the changes of the version without a hold.
    pub changes_db: Vec<[Option<f64>; 2]>,
}

/// Additional wet shares `w >= 0` with `gain · w` at least `need` in each
/// band that needs a rise.
fn solve(gain: [[f64; 2]; 2], need: [f64; 2]) -> [f64; 2] {
    let det = gain[0][0] * gain[1][1] - gain[0][1] * gain[1][0];
    if det.abs() > 1e-9 {
        let w = [
            (need[0] * gain[1][1] - need[1] * gain[0][1]) / det,
            (need[1] * gain[0][0] - need[0] * gain[1][0]) / det,
        ];
        if w[0] >= 0.0 && w[1] >= 0.0 {
            return w;
        }
    }
    // One bell: the smallest share that lifts every band that needs it.
    let single = |f: usize| {
        (0..2)
            .filter(|&b| need[b] > 0.0)
            .map(|b| {
                if gain[b][f] > 1e-3 {
                    need[b] / gain[b][f]
                } else {
                    f64::INFINITY
                }
            })
            .fold(0.0, f64::max)
    };
    let (a, b) = (single(0), single(1));
    if a <= b {
        [a, 0.0]
    } else {
        [0.0, b]
    }
}

/// The change of each held band in one section when each hold bell of
/// `spec` acts there at full strength, before the loudness match:
/// `[band][bell]`; and the K-weighted loudness, in dB, that each bell adds
/// to the whole file when it acts everywhere (`set.low-end.v3`).
pub fn bell_gains(spec: &Spec, spectrum: &crate::Spectrum) -> ([[f64; 2]; 2], [f64; 2]) {
    let bell_db = bell_maximum_db(spec);
    let bells = [0, 1].map(|f| effect(spectrum, &hold_band_at(f, bell_db), Domain::Stereo));
    (
        std::array::from_fn(|b| std::array::from_fn(|f| bells[f].0[b])),
        bells.map(|(_, loudness)| loudness),
    )
}

/// The change of each held band in a section when the wet shares of the
/// hold bells rise by `add` there, before the loudness match, with the
/// gains of `bell_gains`.
pub fn rise(gains: &[[f64; 2]; 2], add: [f64; 2]) -> [f64; 2] {
    std::array::from_fn(|b| (0..2).map(|f| add[f] * gains[b][f]).sum())
}

/// The fall of every section at matched loudness, in dB, when the wet
/// shares of the hold bells rise by `added` in each section: the loudness
/// that the lifts add to the file, each section weighted by its share of
/// the duration (`shares`). The loudness match takes it from every band of
/// every section.
pub fn loudness_offset_db(loudness: &[f64; 2], added: &[[f64; 2]], shares: &[f64]) -> f64 {
    added
        .iter()
        .zip(shares)
        .map(|(a, share)| share * (a[0] * loudness[0] + a[1] * loudness[1]))
        .sum()
}

/// The measured fall of the bands that the step `previous` left alone, against
/// its loudness offset: the median over the bands whose own predicted rise
/// was under 0.02 dB, from 0.5 to 3. None when the step had no offset or no
/// such band. `before` holds the changes that the step answered.
fn measured_offset_scale(
    previous: &Step,
    before: &[[Option<f64>; 2]],
    changes: &[[Option<f64>; 2]],
) -> Option<f64> {
    if previous.offset_db < 0.01 {
        return None;
    }
    let mut ratios: Vec<f64> = (0..changes
        .len()
        .min(before.len())
        .min(previous.expected_db.len()))
        .flat_map(|s| (0..2).map(move |b| (s, b)))
        .filter(|&(s, b)| (previous.expected_db[s][b] + previous.offset_db).abs() < 0.02)
        .filter_map(|(s, b)| Some((before[s][b]? - changes[s][b]?) / previous.offset_db))
        .collect();
    if ratios.is_empty() {
        return None;
    }
    ratios.sort_by(f64::total_cmp);
    Some(ratios[ratios.len() / 2].clamp(0.5, 3.0))
}

/// The hold of `spec` with the wet share `wet` of each hold bell in each of
/// `sections` and the factor of the low-mid correction in each section.
pub fn hold_of(
    spec: &Spec,
    sections: &Sections,
    wet: &[[f64; 2]],
    factor: &[f64],
    frames: usize,
) -> Hold {
    let bell_db = bell_maximum_db(spec);
    Hold {
        rule: RULE.into(),
        correction_factors: if factor.iter().any(|f| *f < 1.0) {
            (0..wet.len())
                .map(|s| (sections.starts[s], sections.ends[s], factor[s]))
                .collect()
        } else {
            Vec::new()
        },
        bands: (0..2)
            .filter(|&f| wet.iter().any(|w| w[f] > 0.0))
            .map(|f| EnvelopedBand {
                rule: RULE.into(),
                domain: Domain::Stereo,
                band: hold_band_at(f, bell_db),
                points: hold_points(
                    sections,
                    &wet.iter().map(|w| w[f]).collect::<Vec<_>>(),
                    frames,
                ),
            })
            .collect(),
    }
}

/// The next spec of the hold, from the changes of the last render. `previous`
/// is the previous step with the changes before it: the measured rise of its
/// hold scales the prediction. None when the hold can give no more.
pub fn next(
    spec: &Spec,
    spectrum: &crate::Spectrum,
    sections: &Sections,
    changes: &[[Option<f64>; 2]],
    previous: Option<(&Step, &[[Option<f64>; 2]])>,
    frames: usize,
) -> Option<(Spec, Step)> {
    let count = changes.len();
    let mut factor = previous.map_or(vec![1.0; count], |(p, _)| p.correction_factor.clone());
    let mut wet = previous.map_or(vec![[0.0; 2]; count], |(p, _)| p.hold_wet.clone());
    let mut added = vec![[0.0; 2]; count];
    let mut lifted = vec![[0.0; 2]; count];
    let (gains, loudness) = bell_gains(spec, spectrum);
    // After a render, the fall of the bands that the step left alone scales
    // the loudness that each lift adds, and the rest of the change of each
    // band scales the gains of its bells.
    let offset_scale = previous
        .and_then(|(p, before)| measured_offset_scale(p, before, changes))
        .unwrap_or(1.0);
    let loudness = loudness.map(|l| l * offset_scale);
    let shares: Vec<f64> = (0..count)
        .map(|s| (sections.ends[s] - sections.starts[s]) as f64 / frames.max(1) as f64)
        .collect();
    let correction = spec.low_mid.as_ref();
    let correction_gain = correction.map(|c| predicted_db(spectrum, &c.band, c.domain));
    let mut changed = false;
    // The lifts add loudness, and the loudness match lowers every section by
    // it. A second pass lifts the sections that this fall takes under the
    // limit.
    let mut offset = 0.0;
    for _ in 0..2 {
        for s in 0..count {
            let now: [Option<f64>; 2] =
                std::array::from_fn(|b| changes[s][b].map(|c| c + lifted[s][b] - offset));
            let mut need = needed(&now);
            if need == [0.0, 0.0] {
                continue;
            }
            // 1. The share of the deficit that the correction causes here:
            // the planned correction at its factor in this section.
            if let (Some(c), Some(g), None) = (correction, correction_gain, previous) {
                let w = c.mean_wet(sections.starts[s], sections.ends[s]) * factor[s];
                let mut fraction = 0.0f64;
                for b in 0..2 {
                    let caused = -(w * g[b]).min(0.0);
                    if need[b] > 0.0 && caused > 1e-3 {
                        fraction = fraction.max((need[b] / caused).min(1.0));
                    }
                }
                if fraction > 0.0 {
                    for b in 0..2 {
                        need[b] = (need[b] + fraction * w * g[b]).max(0.0);
                        lifted[s][b] -= fraction * w * g[b];
                    }
                    factor[s] *= 1.0 - fraction;
                    changed = true;
                }
            }
            // 2. The hold bells return the rest. After a render, the
            // measured rise of each band in the section, with the loudness
            // offset taken back, against its prediction scales the gains.
            let mut gain = gains;
            if let Some((p, before)) = previous {
                for b in 0..2 {
                    let local = p.expected_db[s][b] + p.offset_db;
                    if let (Some(now), Some(then)) = (changes[s][b], before[s][b]) {
                        if local > 0.05 {
                            let measured = now - then + offset_scale * p.offset_db;
                            let k = (measured / local).clamp(0.25, 4.0);
                            gain[b] = gain[b].map(|g| g * k);
                        }
                    }
                }
            }
            let free = [1.0 - wet[s][0], 1.0 - wet[s][1]];
            let mut step = solve(gain, need);
            for f in 0..2 {
                step[f] = step[f].min(free[f]).max(0.0);
            }
            let rise = rise(&gain, step);
            // The hold lifts no held band of the section above the reference
            // by more than the limit, and a band already near or past the
            // limit by no more than `SPILL_DB`.
            let mut scale = 1.0f64;
            for b in 0..2 {
                if let (Some(c), true) = (now[b], rise[b] > 1e-6) {
                    let room = (LIMIT_DB - c).max(SPILL_DB);
                    scale = scale.min((room / rise[b]).clamp(0.0, 1.0));
                }
            }
            for f in 0..2 {
                let add = step[f] * scale;
                if add > 1e-3 {
                    wet[s][f] += add;
                    added[s][f] += add;
                    changed = true;
                }
            }
            for b in 0..2 {
                lifted[s][b] += rise[b] * scale;
            }
        }
        offset = loudness_offset_db(&loudness, &added, &shares);
    }
    if !changed {
        return None;
    }
    let expected: Vec<[f64; 2]> = lifted
        .iter()
        .map(|l| [l[0] - offset, l[1] - offset])
        .collect();
    let mut next = spec.clone();
    next.low_end_hold = Some(hold_of(spec, sections, &wet, &factor, frames));
    let failing_sections = (0..count)
        .filter(|&s| needed(&changes[s]) != [0.0, 0.0])
        .collect();
    Some((
        next,
        Step {
            render: previous.map_or(1, |(p, _)| p.render + 1),
            worst_db: worst_db(changes),
            failing_sections,
            correction_factor: factor,
            hold_wet: wet,
            added,
            expected_db: expected,
            offset_db: offset,
            changes_db: changes.to_vec(),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(hz: f64, amplitude: f32, frames: usize) -> Vec<Stereo> {
        (0..frames)
            .map(|i| {
                let x =
                    amplitude * (2.0 * std::f64::consts::PI * hz * i as f64 / 48000.0).sin() as f32;
                [x, x]
            })
            .collect()
    }

    #[test]
    fn a_section_below_natural_fails_and_a_matched_one_passes() {
        let frames = 3 * section_frames(48000);
        let natural = tone(80.0, 0.5, frames);
        let mut version = natural.clone();
        // The second section of the version is 1 dB lower.
        for f in &mut version[section_frames(48000)..2 * section_frames(48000)] {
            *f = f.map(|x| x * 10f32.powf(-1.0 / 20.0));
        }
        let (a, b) = (sections(&natural, 48000), sections(&version, 48000));
        assert_eq!(a.starts.len(), 3);
        let changes = changes(&b, -14.0, &a, -14.0, [0.0; 2]);
        assert!((changes[1][0].unwrap() + 1.0).abs() < 0.01);
        assert!(changes[0][0].unwrap().abs() < 0.01);
        // The 120–250 Hz band of an 80 Hz tone is under the activity floor.
        assert!(!passes(&changes));
        assert!(passes(&changes[..1]));
    }

    #[test]
    fn the_correction_factor_glides_between_section_centres() {
        let span = section_frames(48000);
        let hold = Hold {
            rule: "test".into(),
            correction_factors: vec![
                (0, span, 1.0),
                (span, 2 * span, 0.0),
                (2 * span, 3 * span, 1.0),
            ],
            bands: Vec::new(),
        };
        let correction = EnvelopedBand {
            rule: "test".into(),
            domain: Domain::Mid,
            band: EqBand {
                kind: Kind::Peak,
                hz: 300.0,
                q: 1.4,
                gain_db: -1.0,
            },
            points: (0..=30)
                .map(|k| BlendPoint {
                    sample: k * span / 10,
                    wet: 1.0,
                })
                .collect(),
        };
        let scaled = hold.scaled(&correction);
        let at = |sample: usize| {
            scaled
                .points
                .iter()
                .find(|p| p.sample == sample)
                .unwrap()
                .wet
        };
        // Full at the centre of section 0, none at the centre of section 1,
        // half way at the boundary between them.
        assert!((at(span / 2) - 1.0).abs() < 1e-9);
        assert!(at(3 * span / 2).abs() < 1e-9);
        assert!((at(span) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn the_hold_applies_only_in_failing_sections() {
        let frames = 3 * section_frames(48000);
        let s = sections(&tone(80.0, 0.5, frames), 48000);
        // The mix has bass, body and presence.
        let (bass, body, presence) = (
            tone(80.0, 0.3, frames),
            tone(1000.0, 0.2, frames),
            tone(3000.0, 0.2, frames),
        );
        let mix: Vec<Stereo> = (0..frames)
            .map(|i| [0, 1].map(|c| bass[i][c] + body[i][c] + presence[i][c]))
            .collect();
        let spectrum = crate::spectrum(&mix, 48000);
        let changes = vec![[Some(0.0), None], [Some(-0.9), None], [Some(-0.1), None]];
        let spec = crate::set::test_spec();
        let (next, step) = next(&spec, &spectrum, &s, &changes, None, frames).unwrap();
        let hold = next.low_end_hold.unwrap();
        // Only the 40 to 120 Hz band fails: only its bell is used, and the
        // loudness match takes its loudness from every band.
        assert_eq!(hold.bands.len(), 1);
        assert!(hold.bands[0].band.gain_db > 0.0);
        let hold = &hold.bands[0];
        assert_eq!(hold.band.hz, (40.0f64 * 120.0).sqrt());
        assert_eq!(step.failing_sections, vec![1]);
        assert_eq!(step.hold_wet[0], [0.0, 0.0]);
        assert!(step.hold_wet[1][0] > 0.1 && step.hold_wet[1][0] <= 1.0);
        // The first half of the passing section 0 has no hold; the hold has
        // its full share at the centre of section 1.
        let centre = |k: usize| (s.starts[k] + s.ends[k]) / 2;
        assert!(hold.mean_wet(s.starts[0], centre(0)) < 1e-9);
        let peak = hold.mean_wet(centre(1) - 4800, centre(1) + 4800);
        assert!((peak - step.hold_wet[1][0]).abs() < 0.01, "{peak}");
    }

    #[test]
    fn a_band_past_the_limit_takes_a_small_rise_from_the_lift_below_it() {
        let frames = 3 * section_frames(48000);
        // The mix has bass, low mids, body and presence.
        let parts = [
            tone(80.0, 0.3, frames),
            tone(180.0, 0.2, frames),
            tone(1000.0, 0.2, frames),
            tone(3000.0, 0.2, frames),
        ];
        let mix: Vec<Stereo> = (0..frames)
            .map(|i| [0, 1].map(|c| parts.iter().map(|p| p[i][c]).sum()))
            .collect();
        let s = sections(&mix, 48000);
        let spectrum = crate::spectrum(&mix, 48000);
        // Section 1: the bass is short and the low mids are past the limit.
        let changes = vec![
            [Some(0.0), Some(0.0)],
            [Some(-0.66), Some(1.9)],
            [Some(0.0), Some(0.0)],
        ];
        let spec = crate::set::test_spec();
        let (_, step) = next(&spec, &spectrum, &s, &changes, None, frames).unwrap();
        assert_eq!(step.failing_sections, vec![1]);
        assert!(step.hold_wet[1][0] > 0.0, "{:?}", step.hold_wet);
        let [bass, low_mids] = step.expected_db[1];
        assert!(bass > 0.0, "{bass}");
        assert!(low_mids <= SPILL_DB + 1e-9, "{low_mids}");
    }

    #[test]
    fn the_loudness_match_takes_the_lift_from_every_section() {
        let frames = 3 * section_frames(48000);
        let (bass, body) = (tone(80.0, 0.3, frames), tone(1000.0, 0.2, frames));
        let mix: Vec<Stereo> = (0..frames)
            .map(|i| [0, 1].map(|c| bass[i][c] + body[i][c]))
            .collect();
        let spectrum = crate::spectrum(&mix, 48000);
        let s = sections(&mix, 48000);
        let spec = crate::set::test_spec();
        let (gains, loudness) = bell_gains(&spec, &spectrum);
        let (held, added) = effect(&spectrum, &hold_band(0), Domain::Stereo);
        assert_eq!(gains[0][0], held[0]);
        assert!(loudness[0] > 0.0 && (loudness[0] - added).abs() < 1e-12);
        // Section 1 alone fails. Its bell lifts it to the target, and the
        // loudness match lowers every section by the loudness that the lift
        // adds to the file, a third of the bell's loudness at its share.
        let changes = vec![[Some(0.0), None], [Some(-0.9), None], [Some(-0.1), None]];
        let (_, step) = next(&spec, &spectrum, &s, &changes, None, frames).unwrap();
        let offset = loudness_offset_db(&loudness, &step.added, &[1.0 / 3.0; 3]);
        assert!(
            offset > 0.0 && offset < loudness[0] * step.added[1][0],
            "{offset}"
        );
        assert!((step.expected_db[0][0] + offset).abs() < 1e-9);
        assert!((-0.9 + step.expected_db[1][0] - (TARGET_DB - offset)).abs() < 1e-9);
        assert_eq!(
            rise(&gains, step.added[1])[0],
            step.expected_db[1][0] + offset
        );
    }

    #[test]
    fn the_fall_of_the_bands_left_alone_scales_the_loudness_offset() {
        let step = Step {
            render: 1,
            worst_db: Some(-0.9),
            failing_sections: vec![1],
            correction_factor: vec![1.0; 3],
            hold_wet: vec![[0.0; 2], [0.4, 0.0], [0.0; 2]],
            added: vec![[0.0; 2], [0.4, 0.0], [0.0; 2]],
            expected_db: vec![[-0.1, -0.1], [0.55, 0.05], [-0.1, -0.1]],
            offset_db: 0.1,
            changes_db: Vec::new(),
        };
        let before = vec![
            [Some(0.0), Some(0.0)],
            [Some(-0.9), Some(0.0)],
            [Some(-0.1), None],
        ];
        // The bands left alone fell by twice the predicted offset.
        let after = vec![
            [Some(-0.2), Some(-0.2)],
            [Some(-0.4), Some(0.0)],
            [Some(-0.3), None],
        ];
        assert_eq!(measured_offset_scale(&step, &before, &after), Some(2.0));
        let none = Step {
            offset_db: 0.0,
            ..step.clone()
        };
        assert_eq!(measured_offset_scale(&none, &before, &after), None);
    }

    #[test]
    fn a_bass_shelf_raises_the_loudness_that_a_bell_adds() {
        let frames = 3 * section_frames(48000);
        let (bass, body) = (tone(80.0, 0.3, frames), tone(1000.0, 0.2, frames));
        let mix: Vec<Stereo> = (0..frames)
            .map(|i| [0, 1].map(|c| bass[i][c] + body[i][c]))
            .collect();
        let spectrum = crate::spectrum(&mix, 48000);
        let mut spec = crate::set::test_spec();
        spec.tone.stereo.push(EqBand {
            kind: Kind::LowShelf,
            hz: 90.0,
            q: std::f64::consts::FRAC_1_SQRT_2,
            gain_db: 3.0,
        });
        // The shelf makes the bass a larger share of the loudness, so the
        // loudness match takes more of the bell's lift.
        let (_, source) = bell_gains(&spec, &spectrum);
        let (_, shaped) = bell_gains(&spec, &shaped_spectrum(&spectrum, &spec));
        assert!(shaped[0] > source[0], "{shaped:?} {source:?}");
    }
}
