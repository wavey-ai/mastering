//! Names and descriptions of the delivered versions of a set from their
//! measured sound (`set.version-names.v1`).
//!
//! Each name is a character of the version: what it has more of than the
//! version before it, at matched loudness. A version keeps the character
//! that its rule gives it, such as Full for the bass version
//! (`set.bass-version.v4`), when that character measures `STEP_DB` or more.
//! The version closest to the mix takes the character it has more of than
//! every other version, or `CLOSEST_NAME`. Each name occurs once in a set.

use crate::selection;

pub const RULE: &str = "set.version-names.v1";

/// The name of the version closest to the mix when no measured character
/// sets it apart from the other versions.
pub const CLOSEST_NAME: &str = "Essential";

/// Every name that a version can take.
pub const NAMES: [&str; 11] = [
    "Warm",
    "Smooth",
    "Full",
    "Forward",
    "Bright",
    "Airy",
    "Wide",
    "Focused",
    "Vivid",
    "Open",
    CLOSEST_NAME,
];

/// Smallest measured step that a name states, in dB (`selection::DISTINCT_DB`).
pub const STEP_DB: f64 = selection::DISTINCT_DB;

/// The measured sound of one delivered version against the mix.
#[derive(Clone, Debug)]
pub struct Sound {
    pub id: String,
    /// Change of each `stereo::BAND_EDGES_HZ` band against the mix at
    /// matched loudness, in dB. `None` for a band that is not material in
    /// the mix or in the version.
    pub bands_db: [Option<f64>; 6],
    /// Side-to-mid change against the mix, in dB. `None` for a mono mix.
    pub width_db: Option<f64>,
    /// The bass of the held bands at matched loudness
    /// (`low_end::bass_level_db`).
    pub bass_level_db: Option<f64>,
    /// The character that the version's rule gives it.
    pub designed: Option<Character>,
}

/// The character that a version's rule designs: Open for the `open`
/// version (`set.open.v3`), Full for a version whose tone carries the bass
/// shelves (`open::carries_bass_shelves`, `set.bass-version.v4`).
pub fn designed(id: &str, spec: &crate::treatment_render::Spec) -> Option<Character> {
    if id == "open" {
        Some(Character::Open)
    } else if crate::open::carries_bass_shelves(&spec.tone) {
        Some(Character::Full)
    } else {
        None
    }
}

/// The measured sound of a version from its delivered measurements.
pub(crate) fn sound(
    id: &str,
    source: crate::pipeline::Mix<'_>,
    version: crate::set::ListeningMeasurement<'_>,
    designed: Option<Character>,
    bass_level_db: Option<f64>,
) -> Sound {
    let mix_lufs = source.input.integrated_lufs.unwrap_or(version.lufs);
    let (mix, out) = (source.stereo, version.output_stereo);
    let (material_mix, material_out) = (
        selection::material_bands(mix),
        selection::material_bands(out),
    );
    let bands_db = std::array::from_fn(|i| {
        let (a, b) = (&mix.whole.bands[i], &out.whole.bands[i]);
        let (pa, pb) = (a.left_power + a.right_power, b.left_power + b.right_power);
        (material_mix[i] && material_out[i] && pa > 1e-20 && pb > 1e-20)
            .then(|| 10.0 * (pb / pa).log10() + mix_lufs - version.lufs)
    });
    let mono = !mix.whole.bands.iter().any(|b| b.side_present);
    Sound {
        id: id.into(),
        bands_db,
        width_db: (!mono).then_some(version.output.side_mid_db - source.input.side_mid_db),
        bass_level_db,
        designed,
    }
}

/// A character name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Character {
    Warm,
    Smooth,
    Full,
    Forward,
    Bright,
    Airy,
    Wide,
    Focused,
    Vivid,
    /// More bass and a wider bass and low midrange together, in the manner
    /// of the reference album masters of D177 (`set.open.v3`).
    Open,
}

const CHARACTERS: [Character; 10] = [
    Character::Warm,
    Character::Smooth,
    Character::Full,
    Character::Forward,
    Character::Bright,
    Character::Airy,
    Character::Wide,
    Character::Focused,
    Character::Vivid,
    Character::Open,
];

impl Character {
    pub fn name(self) -> &'static str {
        match self {
            Character::Warm => "Warm",
            Character::Smooth => "Smooth",
            Character::Full => "Full",
            Character::Forward => "Forward",
            Character::Bright => "Bright",
            Character::Airy => "Airy",
            Character::Wide => "Wide",
            Character::Focused => "Focused",
            Character::Vivid => "Vivid",
            Character::Open => "Open",
        }
    }

    /// What the version has most of in the set, for the version closest to
    /// the mix.
    fn most(self) -> &'static str {
        match self {
            Character::Warm => "the most body and warmth",
            Character::Smooth => "the gentlest top end",
            Character::Full => "the most bass",
            Character::Forward => "the vocal and lead furthest forward",
            Character::Bright => "the most treble",
            Character::Airy => "the most air",
            Character::Wide => "the widest stereo image",
            Character::Focused => "the most centred stereo image",
            Character::Vivid => "the most presence, treble and width",
            Character::Open => "the most bass and the widest low end",
        }
    }

    /// What the version adds to the version before it.
    fn step(self) -> &'static str {
        match self {
            Character::Warm => "adds body and warmth",
            Character::Smooth => "softens the top end",
            Character::Full => "adds bass",
            Character::Forward => "brings the vocal and lead forward",
            Character::Bright => "adds treble and sparkle",
            Character::Airy => "adds air",
            Character::Wide => "widens the stereo image",
            Character::Focused => "tightens and centres the stereo image",
            Character::Vivid => "adds presence, treble and width together",
            Character::Open => "adds bass and opens the bass and low mids",
        }
    }

    /// The name of a version with more of the character than a version
    /// before it that has the character's name.
    fn more(self) -> &'static str {
        match self {
            Character::Warm => "Warmer",
            Character::Smooth => "Smoother",
            Character::Full => "Fuller",
            Character::Forward => "More forward",
            Character::Bright => "Brighter",
            Character::Airy => "Airier",
            Character::Wide => "Wider",
            Character::Focused => "More focused",
            Character::Vivid => "More vivid",
            Character::Open => "More open",
        }
    }

    /// What the version adds to the version before it, when both have the
    /// character: the start of "… than <the version before>".
    fn further(self) -> &'static str {
        match self {
            Character::Warm => "adds more body and warmth",
            Character::Smooth => "softens the top end more",
            Character::Full => "adds more bass",
            Character::Forward => "brings the vocal and lead further forward",
            Character::Bright => "adds more treble and sparkle",
            Character::Airy => "adds more air",
            Character::Wide => "widens the stereo image more",
            Character::Focused => "centres the stereo image more",
            Character::Vivid => "adds more presence, treble and width",
            Character::Open => "adds more bass and opens the low end more",
        }
    }

    /// The measured value of the character, in dB; `None` when the bands it
    /// needs are not measured.
    fn value(self, s: &Sound) -> Option<f64> {
        let b = |i: usize| s.bands_db[i];
        match self {
            // The held bands of the low-end hold, 40 to 250 Hz; the band
            // from 20 Hz holds sub-bass that the hold does not lift.
            Character::Full => s.bass_level_db.or(b(0)),
            Character::Warm => Some(b(1)? - b(3)?),
            Character::Smooth => Some(-(b(3)? + b(4)?) / 2.0),
            // Presence at matched loudness: a body cut of the low-end hold
            // raises presence against body without a presence lift.
            Character::Forward => b(3),
            Character::Bright => b(4),
            Character::Airy => b(5),
            Character::Wide => s.width_db,
            Character::Focused => s.width_db.map(|w| -w),
            Character::Vivid | Character::Open => None,
        }
    }

    /// A width name describes a version that is wider, or narrower, than
    /// the mix.
    fn holds(self, s: &Sound) -> bool {
        match self {
            Character::Wide | Character::Open => s.width_db.is_some_and(|w| w >= STEP_DB),
            Character::Focused => s.width_db.is_some_and(|w| w <= -STEP_DB),
            _ => true,
        }
    }
}

/// The step of `character` from `before` to `version`, in dB. For Vivid,
/// the smallest of the presence, treble and width steps; for Open, the
/// smaller of the bass and width steps.
fn step(character: Character, version: &Sound, before: &Sound) -> Option<f64> {
    if character == Character::Vivid {
        let presence = version.bands_db[3]? - before.bands_db[3]?;
        let treble = version.bands_db[4]? - before.bands_db[4]?;
        let width = version.width_db? - before.width_db?;
        return Some(presence.min(treble).min(width));
    }
    if character == Character::Open {
        let bass = Character::Full.value(version)? - Character::Full.value(before)?;
        let width = version.width_db? - before.width_db?;
        return Some(bass.min(width));
    }
    Some(character.value(version)? - character.value(before)?)
}

/// The margin of `character` of the first version over every other
/// version, in dB.
fn margin(character: Character, first: &Sound, others: &[Sound]) -> Option<f64> {
    let own = character.value(first)?;
    others
        .iter()
        .map(|o| character.value(o).map(|v| own - v))
        .try_fold(f64::INFINITY, |m, d| d.map(|d| m.min(d)))
        .filter(|m| m.is_finite())
}

/// The name and the description of one version.
#[derive(Clone, Debug, PartialEq, serde::Serialize)]
pub struct Named {
    pub id: String,
    pub name: &'static str,
    pub description: String,
    /// The characters behind the name and the description, with their
    /// measured steps in dB.
    pub characters: Vec<(&'static str, f64)>,
    /// False for a later version with no step of `STEP_DB` or more in any
    /// character over the version before it: the set does not deliver it,
    /// because no name or description would tell a listener what it
    /// changes.
    pub distinct: bool,
}

/// Name the delivered versions of a set, in delivery order. `closest`
/// is true when the first version is the version closest to the mix.
pub fn name(versions: &[Sound], closest: bool) -> Vec<Named> {
    // Every candidate name of every version with its measured step.
    let mut candidates: Vec<(f64, usize, Character)> = Vec::new();
    for (i, v) in versions.iter().enumerate() {
        for c in CHARACTERS {
            if !c.holds(v) {
                continue;
            }
            let score = if i == 0 {
                if matches!(c, Character::Vivid | Character::Open) || versions.len() < 2 {
                    continue;
                }
                margin(c, v, &versions[1..])
            } else {
                step(c, v, &versions[i - 1])
            };
            if let Some(score) = score {
                candidates.push((score, i, c));
            }
        }
    }
    candidates.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut chosen: Vec<Option<(Character, f64)>> = vec![None; versions.len()];
    let mut used = Vec::new();
    // A version keeps its designed character when it measures `STEP_DB` or
    // more, before the largest steps of the other versions.
    for &(score, i, c) in &candidates {
        if score >= STEP_DB
            && versions[i].designed == Some(c)
            && chosen[i].is_none()
            && !used.contains(&c)
        {
            chosen[i] = Some((c, score));
            used.push(c);
        }
    }
    let mut assign =
        |minimum: f64, later_only: bool, chosen: &mut Vec<Option<(Character, f64)>>| {
            for &(score, i, c) in &candidates {
                if score >= minimum
                    && chosen[i].is_none()
                    && !used.contains(&c)
                    && !(later_only && i == 0)
                {
                    chosen[i] = Some((c, score));
                    used.push(c);
                }
            }
        };
    assign(STEP_DB, false, &mut chosen);
    // A later version whose largest step of `STEP_DB` or more is in a
    // character that a version before it holds takes the comparative name
    // of that character.
    let mut further: Vec<Option<(Character, f64)>> = vec![None; versions.len()];
    let mut taken: Vec<Character> = Vec::new();
    for &(score, i, c) in &candidates {
        if i > 0
            && score >= STEP_DB
            && chosen[i].is_none()
            && further[i].is_none()
            && used.contains(&c)
            && !taken.contains(&c)
        {
            further[i] = Some((c, score));
            taken.push(c);
        }
    }
    let names: Vec<&'static str> = (0..versions.len())
        .map(|i| match (chosen[i], further[i]) {
            (Some((c, _)), _) => c.name(),
            (None, Some((c, _))) => c.more(),
            (None, None) => CLOSEST_NAME,
        })
        .collect();
    versions
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let main = chosen[i];
            // The second character of the description: the next step of
            // `STEP_DB` or more of the same version.
            let extra = candidates
                .iter()
                .find(|&&(score, j, c)| {
                    j == i && score >= STEP_DB && main.is_none_or(|(m, _)| m != c)
                })
                .map(|&(score, _, c)| (c, score));
            let mut characters = Vec::new();
            let mut distinct = true;
            let description = if i == 0 {
                let mut text = if closest {
                    "Closest to your mix".to_string()
                } else {
                    "The version with the least change".to_string()
                };
                let count = ["two", "three", "four"]
                    .get(versions.len().saturating_sub(2))
                    .copied()
                    .unwrap_or("versions");
                if let Some((c, score)) = main {
                    text.push_str(&format!(", with {} of the {count}", c.most()));
                    characters.push((c.name(), score));
                    if let Some((e, s)) = extra {
                        text.push_str(&format!(", and {}", e.most()));
                        characters.push((e.name(), s));
                    }
                }
                text + "."
            } else if let Some((c, score)) = main {
                characters.push((c.name(), score));
                let mut text = capitalised(c.step());
                if let Some((e, s)) = extra {
                    text.push_str(&format!(", and {}", e.step()));
                    characters.push((e.name(), s));
                }
                text + "."
            } else if let Some((c, score)) = further[i] {
                characters.push((c.name(), score));
                format!("{} than {}.", capitalised(c.further()), names[i - 1])
            } else {
                distinct = false;
                String::new()
            };
            Named {
                id: v.id.clone(),
                name: names[i],
                description,
                characters,
                distinct,
            }
        })
        .collect()
}

fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map_or(String::new(), |c| c.to_uppercase().chain(chars).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn version(id: &str, bands: [f64; 6], width: f64) -> Sound {
        Sound {
            id: id.into(),
            bands_db: bands.map(Some),
            width_db: Some(width),
            bass_level_db: None,
            designed: None,
        }
    }

    #[test]
    fn each_version_takes_its_largest_step_and_each_name_occurs_once() {
        let natural = version("essential", [0.0; 6], 0.0);
        let full = version("master", [1.2, 0.8, -0.6, 0.0, 0.0, 0.0], 0.2);
        let vivid = version("bold", [0.0, -0.2, -0.5, 2.0, 1.6, 1.0], 1.4);
        let named = name(&[natural, full, vivid], true);
        assert_eq!(named[1].name, "Full");
        assert_eq!(named[1].description, "Adds bass, and adds body and warmth.");
        assert_ne!(named[2].name, named[1].name);
        assert!(["Forward", "Bright", "Wide", "Vivid"].contains(&named[2].name));
        assert!(named[0].description.starts_with("Closest to your mix"));
    }

    #[test]
    fn the_description_of_the_closest_version_counts_every_version() {
        let natural = version("essential", [0.0; 6], 0.0);
        let full = version("master", [1.0, 0.5, 0.0, -0.3, -1.0, -1.0], 0.2);
        let forward = version("bold", [-0.2, 0.0, 0.0, 1.5, -0.6, -0.6], 1.0);
        let open = version("open", [1.0, 1.0, 0.5, -0.2, -0.8, -0.8], 2.0);
        let named = name(&[natural, full, forward, open], true);
        assert!(
            named[0].description.contains("the most treble of the four"),
            "{}",
            named[0].description
        );
    }

    #[test]
    fn the_closest_version_without_a_character_keeps_the_mix_name() {
        let natural = version("essential", [0.0; 6], 0.0);
        let full = version("master", [1.5, 0.6, -0.3, 0.2, 0.0, 0.0], 0.0);
        let bright = version("bold", [0.0, -0.2, -0.4, 1.2, 1.8, 1.0], 0.3);
        let named = name(&[natural, full, bright], true);
        assert_eq!(named[0].name, CLOSEST_NAME);
        assert_eq!(named[0].description, "Closest to your mix.");
    }

    #[test]
    fn the_bass_version_keeps_its_name_when_its_bass_step_is_distinct() {
        let natural = version("essential", [0.0; 6], 0.0);
        let mut bass = version("master", [0.7, 0.4, -0.5, 0.4, 0.2, 0.2], 2.0);
        bass.designed = Some(Character::Full);
        let vivid = version("bold", [0.0, -0.2, -0.5, 2.5, 1.8, 1.0], 2.6);
        let named = name(&[natural.clone(), bass.clone(), vivid.clone()], true);
        // The width step (2.0 dB) is larger, but the bass step is distinct.
        assert_eq!(named[1].name, "Full");
        assert!(named[1].description.starts_with("Adds bass"));
        // A bass step under `STEP_DB` gives the name of the largest step.
        bass.bands_db[0] = Some(0.3);
        let named = name(&[natural, bass, vivid], true);
        assert_eq!(named[1].name, "Wide");
    }

    #[test]
    fn a_version_with_more_of_a_taken_character_takes_its_comparative_name() {
        // The third version has 1.1 dB more presence than Forward and a
        // little less bass.
        let warm = version("essential", [0.7, 0.3, -0.6, 0.0, 0.0, 0.0], 0.0);
        let forward = version("master", [-0.4, -0.2, -0.5, 2.4, 0.1, 0.0], -0.4);
        let more = version("bold", [-0.6, -0.3, -0.6, 3.5, 0.2, 0.0], -0.4);
        let named = name(&[warm, forward, more], true);
        assert_eq!(named[1].name, "Forward");
        assert_eq!(named[2].name, "More forward");
        assert_eq!(
            named[2].description,
            "Brings the vocal and lead further forward than Forward."
        );
        assert!(named.iter().all(|n| n.distinct));
    }

    #[test]
    fn a_version_that_no_step_names_is_not_distinct() {
        let natural = version("essential", [0.0; 6], 0.0);
        let full = version("master", [1.2, 0.8, -0.6, 0.0, 0.0, 0.0], 0.2);
        // Less of everything than Full by under a step: no name says what
        // it changes.
        let unnamed = version("bold", [1.0, 0.7, -0.4, 0.1, 0.1, 0.1], 0.3);
        let named = name(&[natural, full, unnamed], true);
        assert!(named[0].distinct && named[1].distinct);
        assert!(!named[2].distinct);
        assert!(named[2].description.is_empty());
    }

    #[test]
    fn a_width_name_needs_a_change_against_the_mix() {
        let natural = version("essential", [0.0; 6], -0.2);
        let narrower = version("master", [0.0, 0.0, 0.0, 0.3, 0.2, 0.0], -2.0);
        let named = name(&[natural.clone(), narrower], true);
        assert_eq!(named[1].name, "Focused");
        let wider_than_before = version("master", [0.0; 6], 0.2);
        let named = name(&[natural, wider_than_before], true);
        assert_ne!(named[1].name, "Wide");
    }
}
