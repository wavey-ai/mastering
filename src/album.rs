//! The album check, the running order and the album levels
//! (`album.order.v2`, `album.levels.v2`, D207).
//!
//! The check measures one track at a time and keeps a summary of each:
//! a host that masters an album on a device holds one track in memory at a
//! time. The order and the levels come from the summaries alone.
use crate::{
    audio_io::{Result, Stereo},
    dsp,
};
use serde::{Deserialize, Serialize};

pub const RULE: &str = "album.order.v2";
pub const LEVELS_RULE: &str = "album.levels.v2";

/// Length of the loudest excerpt on which the check measures the
/// audibility point and the image, in seconds.
pub const EXCERPT_SECONDS: f64 = 60.0;
/// Largest level step between adjacent tracks of the album levels, in LU.
/// The masters of a reference album step 0.2 to 0.9 LU between adjacent
/// tracks, except into the last track (D177).
pub const LEVEL_STEP_LU: f64 = 1.0;
/// Largest level rise into the last track, in LU: the last track of a
/// reference album closes 2.3 LU above the track before it (D177).
pub const CLOSER_RISE_LU: f64 = 2.5;
/// Largest cut of the album levels under the point of a track, in LU. A
/// larger step between two tracks stays: the masters of a reference album
/// step up to 2.3 LU (D177), and one quiet track does not lower the whole album.
pub const LEVEL_CUT_MAX_LU: f64 = 1.0;
/// Smallest lead of the correlation of the key over the next key for a
/// clear key.
pub const KEY_CLEAR_MARGIN: f64 = 0.05;
/// Two tracks whose crests differ by this much or less are equally dense,
/// in dB.
pub const DENSE_TIE_DB: f64 = 0.25;
/// Largest length of a vinyl side, in seconds: 22 minutes of a 12-inch
/// record at 33 1/3 rpm.
pub const VINYL_SIDE_SECONDS: f64 = 22.0 * 60.0;
/// Width tier edges of the side/mid of the mix from 120 Hz to 12 kHz, in dB.
pub const WIDTH_TIER_EDGES_DB: [f64; 3] = [-14.0, -11.0, -8.0];

/// The summary of one track.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Track {
    pub id: String,
    pub seconds: f64,
    /// Integrated loudness of the mix, in LUFS.
    pub lufs: f64,
    /// Audibility point of the base chain of the whole track, in LUFS: the
    /// point of the loudest excerpt less the loudness of the excerpt above
    /// the track. None for a track that arrives limited.
    pub point_lufs: Option<f64>,
    /// 50 ms crest at the median, in dB.
    pub crest_db: f64,
    /// Side/mid from 120 Hz to 12 kHz, in dB.
    pub width_db: f64,
    /// 1 (narrow) to 4 (wide), from `WIDTH_TIER_EDGES_DB`.
    pub width_tier: u8,
    /// Side/mid from 12 to 20 kHz, in dB.
    pub treble_side_db: Option<f64>,
    pub tempo_bpm: f64,
    /// Peak of the normalized onset autocorrelation at the tempo, 0 to 1.
    pub tempo_strength: f64,
    /// Pitch class of the key, 0 for C.
    pub key: u8,
    pub minor: bool,
    /// Correlation of the chroma with the key profile, -1 to 1.
    pub key_strength: f64,
    /// The key with the next highest correlation.
    #[serde(default)]
    pub second_key: Option<Key>,
    /// True when the key leads `second_key` by `KEY_CLEAR_MARGIN` or more.
    /// A summary without a `second_key` field is clear.
    #[serde(default = "clear")]
    pub key_clear: bool,
}

fn clear() -> bool {
    true
}

/// One key and its correlation with the chroma of a track.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Key {
    /// Pitch class, 0 for C.
    pub key: u8,
    pub minor: bool,
    /// Correlation of the chroma with the key profile, -1 to 1.
    pub strength: f64,
}

impl Track {
    /// The level of the track in the album: its point, or its loudness for
    /// a track that arrives limited.
    pub fn level_lufs(&self) -> f64 {
        self.point_lufs.unwrap_or(self.lufs)
    }
}

/// The summary of `audio`, a track at `rate`.
pub fn measure(id: &str, audio: &[Stereo], rate: u32) -> Result<Track> {
    if audio.len() < rate as usize {
        return Err("A track of an album is one second or longer".into());
    }
    let seconds = audio.len() as f64 / rate as f64;
    let lufs = dsp::loudness(audio, rate).ok_or("The track is silent")?;
    let excerpt = loudest(audio, rate, EXCERPT_SECONDS);
    let excerpt_lufs = dsp::loudness(excerpt, rate).unwrap_or(lufs);
    let source = crate::pipeline::analyze_source(excerpt, rate)?;
    let point_lufs =
        crate::set::audibility_point(excerpt, rate)?.map(|p| p - (excerpt_lufs - lufs));
    let bands = &source.stereo.whole.bands;
    let ratio = |range: std::ops::RangeInclusive<usize>| {
        let (side, mid) = range
            .filter_map(|i| bands.get(i))
            .filter(|b| b.active)
            .fold((0.0, 0.0), |(s, m), b| (s + b.side_power, m + b.mid_power));
        (mid > 0.0 && side > 0.0).then(|| 10.0 * (side / mid).log10())
    };
    let width_db = ratio(1..=4).unwrap_or(-60.0);
    let width_tier = 1 + WIDTH_TIER_EDGES_DB
        .iter()
        .filter(|edge| width_db >= **edge)
        .count() as u8;
    let (tempo_bpm, tempo_strength) = tempo(audio, rate);
    let [first, second] = keys_of(audio, rate);
    Ok(Track {
        id: id.into(),
        seconds,
        lufs,
        point_lufs,
        crest_db: source.temporal.crest_50ms_p10_p50_p90_db[1],
        width_db,
        width_tier,
        treble_side_db: ratio(5..=5),
        tempo_bpm,
        tempo_strength,
        key: first.key,
        minor: first.minor,
        key_strength: first.strength,
        second_key: Some(second),
        key_clear: first.strength - second.strength >= KEY_CLEAR_MARGIN,
    })
}

/// The loudest `seconds` of `audio`, in 1 s steps.
fn loudest(audio: &[Stereo], rate: u32, seconds: f64) -> &[Stereo] {
    let length = (seconds * rate as f64) as usize;
    if audio.len() <= length {
        return audio;
    }
    let step = rate as usize;
    let blocks: Vec<f64> = audio
        .chunks(step)
        .map(|c| c.iter().map(|f| (f[0] * f[0] + f[1] * f[1]) as f64).sum())
        .collect();
    let span = length / step;
    let mut sum: f64 = blocks[..span].iter().sum();
    let (mut best, mut at) = (sum, 0);
    for i in span..blocks.len() {
        sum += blocks[i] - blocks[i - span];
        if sum > best {
            best = sum;
            at = i + 1 - span;
        }
    }
    let start = (at * step).min(audio.len() - length);
    &audio[start..start + length]
}

/// Magnitude spectra of the mono sum of `audio` in Hann frames of `n` at
/// `hop`. `each` receives the index and the magnitudes of each frame.
fn spectra(audio: &[Stereo], n: usize, hop: usize, mut each: impl FnMut(usize, &[f64])) {
    use realfft::RealFftPlanner;
    let fft = RealFftPlanner::<f64>::new().plan_fft_forward(n);
    let window: Vec<f64> = (0..n)
        .map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / n as f64).cos())
        .collect();
    let mut input = fft.make_input_vec();
    let mut output = fft.make_output_vec();
    let mut magnitude = vec![0.0; n / 2 + 1];
    let mut start = 0;
    let mut index = 0;
    while start + n <= audio.len() {
        for (x, (f, w)) in input
            .iter_mut()
            .zip(audio[start..start + n].iter().zip(&window))
        {
            *x = 0.5 * (f[0] + f[1]) as f64 * w;
        }
        if fft.process(&mut input, &mut output).is_ok() {
            for (m, bin) in magnitude.iter_mut().zip(&output) {
                *m = bin.norm();
            }
            each(index, &magnitude);
        }
        start += hop;
        index += 1;
    }
}

/// Frame length of the spectra at `rate` for a frame of `n` at 22.05 kHz.
fn frame(n: usize, rate: u32) -> usize {
    n * (rate as usize / 22_050).max(1)
}

/// The felt tempo prior: a log-normal weight centered at 120 BPM with a
/// width of half an octave. Most songs have a felt tempo from 80 to 160 BPM.
fn felt(bpm: f64) -> f64 {
    (-0.5 * ((bpm / 120.0).log2() / 0.5).powi(2)).exp()
}

/// Tempo of `audio` in BPM and the strength of its autocorrelation peak:
/// the spectral flux of the log magnitude in 10 ms steps, autocorrelated
/// from 60 to 190 BPM. Of the three strongest peaks, the one with the
/// highest strength times `felt`.
fn tempo(audio: &[Stereo], rate: u32) -> (f64, f64) {
    let n = frame(2048, rate);
    let hop = (rate / 100) as usize;
    // Full scale of a Hann frame of a sine.
    let reference = n as f64 / 4.0;
    let mut previous: Vec<f64> = Vec::new();
    let mut onset: Vec<f64> = Vec::new();
    spectra(audio, n, hop, |_, magnitude| {
        let log: Vec<f64> = magnitude
            .iter()
            .map(|m| (1000.0 * m / reference).ln_1p())
            .collect();
        if !previous.is_empty() {
            onset.push(
                log.iter()
                    .zip(&previous)
                    .map(|(a, b)| (a - b).max(0.0))
                    .sum(),
            );
        }
        previous = log;
    });
    if onset.len() < 400 {
        return (0.0, 0.0);
    }
    let mean = onset.iter().sum::<f64>() / onset.len() as f64;
    for x in &mut onset {
        *x -= mean;
    }
    let lags: Vec<usize> = ((6000.0 / 190.0) as usize..=(6000.0 / 60.0) as usize).collect();
    let corr: Vec<f64> = lags
        .iter()
        .map(|&lag| {
            onset[..onset.len() - lag]
                .iter()
                .zip(&onset[lag..])
                .map(|(a, b)| a * b)
                .sum()
        })
        .collect();
    let top = corr.iter().fold(1e-12f64, |m, c| m.max(c.abs()));
    let mut peaks: Vec<usize> = (1..corr.len() - 1)
        .filter(|&i| corr[i] >= corr[i - 1] && corr[i] > corr[i + 1])
        .collect();
    peaks.sort_by(|a, b| corr[*b].total_cmp(&corr[*a]));
    peaks.truncate(3);
    peaks
        .iter()
        .map(|&i| (6000.0 / lags[i] as f64, (corr[i] / top).clamp(0.0, 1.0)))
        .max_by(|a, b| (a.1 * felt(a.0)).total_cmp(&(b.1 * felt(b.0))))
        .unwrap_or((0.0, 0.0))
}

const MAJOR: [f64; 12] = [
    6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88,
];
const MINOR: [f64; 12] = [
    6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17,
];

/// Key of `audio`: a 12-bin chroma of the spectral peaks from 55 Hz to
/// 2 kHz, correlated with the Krumhansl major and minor profiles. A peak is
/// a bin above its two neighbors: the peaks keep the tones of the mix and
/// drop most of the broadband energy of the drums. Returns the pitch class,
/// the two keys of the highest correlation.
fn keys_of(audio: &[Stereo], rate: u32) -> [Key; 2] {
    let n = frame(8192, rate);
    let pitch: Vec<Option<usize>> = (0..n / 2 + 1)
        .map(|k| {
            let hz = k as f64 * rate as f64 / n as f64;
            (55.0..=2000.0)
                .contains(&hz)
                .then(|| ((12.0 * (hz / 440.0).log2()).round() as i64 + 9).rem_euclid(12) as usize)
        })
        .collect();
    let mut chroma = [0.0f64; 12];
    spectra(audio, n, 2 * n, |_, m| {
        for k in 1..m.len() - 1 {
            if let Some(p) = pitch[k] {
                if m[k] > m[k - 1] && m[k] > m[k + 1] {
                    chroma[p] += m[k];
                }
            }
        }
    });
    let correlate = |a: &[f64; 12], b: &[f64]| {
        let (ma, mb) = (a.iter().sum::<f64>() / 12.0, b.iter().sum::<f64>() / 12.0);
        let (mut num, mut da, mut db) = (0.0, 0.0, 0.0);
        for i in 0..12 {
            num += (a[i] - ma) * (b[i] - mb);
            da += (a[i] - ma).powi(2);
            db += (b[i] - mb).powi(2);
        }
        if da * db > 0.0 {
            num / (da * db).sqrt()
        } else {
            0.0
        }
    };
    let mut keys: Vec<Key> = (0..12u8)
        .flat_map(|tonic| [(tonic, false), (tonic, true)])
        .map(|(tonic, minor)| {
            let profile = if minor { &MINOR } else { &MAJOR };
            let rotated: Vec<f64> = (0..12)
                .map(|i| profile[(i + 12 - tonic as usize) % 12])
                .collect();
            Key {
                key: tonic,
                minor,
                strength: correlate(&chroma, &rotated),
            }
        })
        .collect();
    keys.sort_by(|a, b| b.strength.total_cmp(&a.strength));
    [keys[0], keys[1]]
}

/// Options of the order.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct Options {
    /// The id of the track that the user marked as the single.
    #[serde(default)]
    pub single: Option<String>,
    /// Two sides of a record (`VINYL_SIDE_SECONDS`).
    #[serde(default)]
    pub vinyl: bool,
}

/// What puts a track in its position. The page gives each one its words.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Reason {
    /// The first track: the single, or the loudest and densest.
    Opens { single: bool },
    /// The densest track at position 2 or 3.
    EarlyPeak,
    /// The last track: the longest or the most distinct.
    Closes,
    /// The first track of a vinyl side.
    OpensSide { side: u8 },
    /// The last track of a vinyl side.
    EndsSide { side: u8 },
}

/// The step from the track before to a track.
#[derive(Clone, Debug, Serialize)]
pub struct Step {
    /// Level change, in LU.
    pub level_lu: f64,
    /// Width tier change.
    pub width_tiers: i8,
    /// Key relation of the two keys.
    pub key: KeyStep,
    /// Tempo ratio of the two felt tempos, from 1.
    pub tempo_ratio: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum KeyStep {
    Same,
    Relative,
    Parallel,
    Fifth,
    SemitoneUp,
    Tritone,
    Other,
}

/// One track of the order.
#[derive(Clone, Debug, Serialize)]
pub struct Position {
    pub id: String,
    /// 1 or 2 on a record, None otherwise.
    pub side: Option<u8>,
    pub reasons: Vec<Reason>,
    /// The step from the track before; None for the first track.
    pub step: Option<Step>,
}

#[derive(Clone, Debug, Serialize)]
pub struct Order {
    pub rule: &'static str,
    pub positions: Vec<Position>,
    pub cost: f64,
    /// The cost of each rule: opener, early peak, level, width, key, tempo,
    /// closer, vinyl.
    pub costs: [f64; 8],
    /// Length of each side of a record, in seconds.
    pub sides_seconds: Vec<f64>,
    /// True when each order of the tracks was scored.
    pub exhaustive: bool,
}

/// The key relation from the key of track `a` to the key of track `b`.
pub fn key_step(a: &Track, b: &Track) -> KeyStep {
    relation((a.key, a.minor), (b.key, b.minor))
}

/// The keys of `track` that the order considers: its key, and its second
/// key when the key is not clear.
fn candidates(track: &Track) -> Vec<(u8, bool)> {
    let mut keys = vec![(track.key, track.minor)];
    if !track.key_clear {
        keys.extend(track.second_key.map(|k| (k.key, k.minor)));
    }
    keys
}

fn relation((a_key, a_minor): (u8, bool), (b_key, b_minor): (u8, bool)) -> KeyStep {
    let up = (b_key as i32 - a_key as i32).rem_euclid(12);
    match (a_minor == b_minor, up) {
        (true, 0) => KeyStep::Same,
        (false, 0) => KeyStep::Parallel,
        // A minor key and the major key three semitones above it.
        (false, 3) if a_minor => KeyStep::Relative,
        (false, 9) if !a_minor => KeyStep::Relative,
        (true, 5) | (true, 7) => KeyStep::Fifth,
        (_, 1) => KeyStep::SemitoneUp,
        (_, 6) => KeyStep::Tritone,
        _ => KeyStep::Other,
    }
}

fn key_cost(step: KeyStep) -> f64 {
    match step {
        KeyStep::Same | KeyStep::Relative => 0.0,
        KeyStep::Fifth | KeyStep::SemitoneUp => 0.5,
        KeyStep::Parallel => 1.0,
        KeyStep::Other => 1.5,
        KeyStep::Tritone => 3.0,
    }
}

/// The ratio of the felt tempos of two tracks, from 1: a tempo and its
/// half or double count as one tempo.
pub fn tempo_ratio(a: f64, b: f64) -> f64 {
    if a <= 0.0 || b <= 0.0 {
        return 1.0;
    }
    [b, 2.0 * b, 0.5 * b]
        .iter()
        .map(|x| if *x > a { x / a } else { a / x })
        .fold(f64::INFINITY, f64::min)
}

fn step(a: &Track, b: &Track) -> Step {
    Step {
        level_lu: b.level_lufs() - a.level_lufs(),
        width_tiers: b.width_tier as i8 - a.width_tier as i8,
        key: key_step(a, b),
        tempo_ratio: tempo_ratio(a.tempo_bpm, b.tempo_bpm),
    }
}

/// Where a two-sided record splits `order`: the first index of side 2 that
/// makes the sides most equal within `VINYL_SIDE_SECONDS`.
fn split(order: &[usize], tracks: &[Track]) -> usize {
    let total: f64 = order.iter().map(|&i| tracks[i].seconds).sum();
    let mut best = (f64::INFINITY, order.len() / 2);
    let mut a = 0.0;
    for (k, &i) in order.iter().enumerate().take(order.len().saturating_sub(1)) {
        a += tracks[i].seconds;
        let b = total - a;
        let over = (a - VINYL_SIDE_SECONDS).max(0.0) + (b - VINYL_SIDE_SECONDS).max(0.0);
        let score = (a - b).abs() + 1000.0 * over;
        if score < best.0 {
            best = (score, k + 1);
        }
    }
    best.1
}

/// Album facts that the costs compare a track with.
struct Album {
    loudest: f64,
    /// The tracks within `DENSE_TIE_DB` of the lowest crest.
    densest: Vec<usize>,
    longest: f64,
    distinct: Vec<f64>,
}

impl Album {
    fn of(tracks: &[Track]) -> Self {
        let loudest = tracks
            .iter()
            .map(Track::level_lufs)
            .fold(f64::NEG_INFINITY, f64::max);
        let lowest = tracks
            .iter()
            .map(|t| t.crest_db)
            .fold(f64::INFINITY, f64::min);
        let densest = (0..tracks.len())
            .filter(|&i| tracks[i].crest_db <= lowest + DENSE_TIE_DB)
            .collect();
        let longest = tracks.iter().map(|t| t.seconds).fold(0.0, f64::max);
        // The distance of each track from the album mean in level, width,
        // crest and tempo, each in units of its spread.
        let features = |t: &Track| [t.level_lufs(), t.width_db, t.crest_db, t.tempo_bpm];
        let n = tracks.len().max(1) as f64;
        let mean: [f64; 4] =
            std::array::from_fn(|k| tracks.iter().map(|t| features(t)[k]).sum::<f64>() / n);
        let spread: [f64; 4] = std::array::from_fn(|k| {
            (tracks
                .iter()
                .map(|t| (features(t)[k] - mean[k]).powi(2))
                .sum::<f64>()
                / n)
                .sqrt()
                .max(1e-6)
        });
        let distinct = tracks
            .iter()
            .map(|t| {
                (0..4)
                    .map(|k| ((features(t)[k] - mean[k]) / spread[k]).powi(2))
                    .sum::<f64>()
                    .sqrt()
            })
            .collect();
        Album {
            loudest,
            densest,
            longest,
            distinct,
        }
    }
}

/// The cost of each rule for `order`, a permutation of the indices of
/// `tracks`: opener, early peak, level, width, key, tempo, closer, vinyl.
fn costs(order: &[usize], tracks: &[Track], album: &Album, options: &Options) -> [f64; 8] {
    let n = order.len();
    let mut total = [0.0; 8];
    let first = &tracks[order[0]];
    // 1. Opener.
    match options.single.as_deref() {
        Some(single) if tracks.iter().any(|t| t.id == single) => {
            if first.id != single {
                total[0] += 10.0;
            }
        }
        _ => total[0] += 0.5 * (album.loudest - first.level_lufs()),
    }
    // 2. Early peak.
    if n >= 4 && !order[1..3].iter().any(|i| album.densest.contains(i)) {
        total[1] += 2.0;
    }
    // 3 to 6. The steps between adjacent tracks.
    for (k, pair) in order.windows(2).enumerate() {
        let (a, b) = (&tracks[pair[0]], &tracks[pair[1]]);
        let s = step(a, b);
        let into_closer = k + 2 == n;
        let free = if into_closer && s.level_lu > 0.0 {
            CLOSER_RISE_LU
        } else {
            LEVEL_STEP_LU
        };
        total[2] += 3.0 * (s.level_lu.abs() - free).max(0.0);
        // The masters of a reference album step two width tiers at one track (D177),
        // so width and tempo steps only break ties.
        total[3] += 0.5 * (s.width_tiers.unsigned_abs() as f64 - 1.0).max(0.0);
        // A key that is not clear takes the relation of the lower cost.
        let key = candidates(a)
            .into_iter()
            .flat_map(|x| {
                candidates(b)
                    .into_iter()
                    .map(move |y| key_cost(relation(x, y)))
            })
            .fold(f64::INFINITY, f64::min);
        total[4] += key * a.key_strength.min(b.key_strength).clamp(0.0, 1.0);
        total[5] += 2.0 * (s.tempo_ratio - 1.25).max(0.0);
    }
    // 7. Closer.
    if n >= 3 {
        let last = order[n - 1];
        let most_distinct = album
            .distinct
            .iter()
            .enumerate()
            .filter(|(i, _)| options.single.as_deref() != Some(tracks[*i].id.as_str()))
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, _)| i);
        if tracks[last].seconds < album.longest && most_distinct != Some(last) {
            total[6] += 1.5;
        }
    }
    // 8. Vinyl.
    if options.vinyl && n >= 2 {
        let k = split(order, tracks);
        for side in [&order[..k], &order[k..]] {
            let (open, end) = (&tracks[side[0]], &tracks[*side.last().unwrap()]);
            // A side opens loud, bright and wide and ends quiet, dark and
            // narrow.
            let load = |t: &Track| {
                t.level_lufs() + 0.25 * t.treble_side_db.unwrap_or(-30.0) + t.width_tier as f64
            };
            if side.len() >= 2 {
                total[7] += 0.5 * (load(end) - load(open)).max(0.0);
            }
        }
    }
    total
}

fn cost(order: &[usize], tracks: &[Track], album: &Album, options: &Options) -> f64 {
    costs(order, tracks, album, options).iter().sum()
}

/// The running order of `tracks` with the lowest cost: each order for
/// ten tracks or fewer, and for more, the order from the best opener with
/// the next track of the lowest cost, improved by swaps of two tracks.
pub fn order(tracks: &[Track], options: &Options) -> Result<Order> {
    if tracks.is_empty() {
        return Err("An album has one track or more".into());
    }
    let album = Album::of(tracks);
    let n = tracks.len();
    let mut best: Vec<usize> = (0..n).collect();
    let mut best_cost = cost(&best, tracks, &album, options);
    let exhaustive = n <= 10;
    if exhaustive {
        // Heap's algorithm over each permutation.
        let mut p: Vec<usize> = (0..n).collect();
        let mut c = vec![0usize; n];
        let mut i = 0;
        while i < n {
            if c[i] < i {
                if i % 2 == 0 {
                    p.swap(0, i);
                } else {
                    p.swap(c[i], i);
                }
                let value = cost(&p, tracks, &album, options);
                if value < best_cost - 1e-12 {
                    best_cost = value;
                    best.clone_from(&p);
                }
                c[i] += 1;
                i = 0;
            } else {
                c[i] = 0;
                i += 1;
            }
        }
    } else {
        loop {
            let mut improved = false;
            for a in 0..n {
                for b in a + 1..n {
                    best.swap(a, b);
                    let value = cost(&best, tracks, &album, options);
                    if value < best_cost - 1e-12 {
                        best_cost = value;
                        improved = true;
                    } else {
                        best.swap(a, b);
                    }
                }
            }
            if !improved {
                break;
            }
        }
    }
    Ok(describe(
        &best, tracks, &album, options, best_cost, exhaustive,
    ))
}

/// The order of the tracks with `ids`, in that order: the order of the user.
pub fn order_of(tracks: &[Track], ids: &[String], options: &Options) -> Result<Order> {
    let order: Vec<usize> = ids
        .iter()
        .map(|id| {
            tracks
                .iter()
                .position(|t| &t.id == id)
                .ok_or_else(|| format!("No track {id} in the album"))
        })
        .collect::<std::result::Result<_, _>>()?;
    if order.len() != tracks.len() {
        return Err("The order names each track of the album once".into());
    }
    let album = Album::of(tracks);
    let value = cost(&order, tracks, &album, options);
    Ok(describe(&order, tracks, &album, options, value, false))
}

fn describe(
    order: &[usize],
    tracks: &[Track],
    album: &Album,
    options: &Options,
    cost: f64,
    exhaustive: bool,
) -> Order {
    let n = order.len();
    let k = (options.vinyl && n >= 2).then(|| split(order, tracks));
    let positions = order
        .iter()
        .enumerate()
        .map(|(p, &i)| {
            let t = &tracks[i];
            let mut reasons = Vec::new();
            if p == 0 {
                reasons.push(Reason::Opens {
                    single: options.single.as_deref() == Some(t.id.as_str()),
                });
            }
            if (p == 1 || p == 2) && album.densest.contains(&i) && n >= 4 {
                reasons.push(Reason::EarlyPeak);
            }
            if p + 1 == n && n >= 3 {
                reasons.push(Reason::Closes);
            }
            let side = k.map(|k| if p < k { 1 } else { 2 });
            if let Some(k) = k {
                if p == k {
                    reasons.push(Reason::OpensSide { side: 2 });
                }
                if p + 1 == k {
                    reasons.push(Reason::EndsSide { side: 1 });
                }
            }
            Position {
                id: t.id.clone(),
                side,
                reasons,
                step: (p > 0).then(|| step(&tracks[order[p - 1]], t)),
            }
        })
        .collect();
    let sides_seconds = match k {
        Some(k) => vec![
            order[..k].iter().map(|&i| tracks[i].seconds).sum(),
            order[k..].iter().map(|&i| tracks[i].seconds).sum(),
        ],
        None => Vec::new(),
    };
    Order {
        rule: RULE,
        positions,
        cost,
        costs: costs(order, tracks, album, options),
        sides_seconds,
        exhaustive,
    }
}

/// The target of one track of the album.
#[derive(Clone, Debug, Serialize)]
pub struct Level {
    pub id: String,
    pub target_lufs: f64,
    /// The audibility point of the track less its target, in LU: the level
    /// that the album leaves unused.
    pub under_point_lu: f64,
    /// This target less the target of the track before, in LU; None for
    /// the first track.
    pub step_lu: Option<f64>,
    /// True for a track that is mastered: its target is its delivered
    /// loudness.
    pub delivered: bool,
}

/// The album levels of `tracks` in the order `ids`, with no track mastered.
pub fn levels(tracks: &[Track], ids: &[String]) -> Result<Vec<Level>> {
    levels_with(tracks, ids, &std::collections::BTreeMap::new())
}

/// The album levels of `tracks` in the order `ids` (`album.levels.v2`).
/// Each track starts at its own audibility point; a mastered track in
/// `delivered` starts at its delivered loudness and keeps it. The louder of
/// two adjacent tracks then comes down to `LEVEL_STEP_LU` above the quieter,
/// or `CLOSER_RISE_LU` for a rise into the last track, by at most
/// `LEVEL_CUT_MAX_LU` under its point. No track rises above its point.
pub fn levels_with(
    tracks: &[Track],
    ids: &[String],
    delivered: &std::collections::BTreeMap<String, f64>,
) -> Result<Vec<Level>> {
    let order: Vec<&Track> = ids
        .iter()
        .map(|id| {
            tracks
                .iter()
                .find(|t| &t.id == id)
                .ok_or_else(|| format!("No track {id} in the album"))
        })
        .collect::<std::result::Result<_, _>>()?;
    let n = order.len();
    let fixed: Vec<Option<f64>> = order
        .iter()
        .map(|t| delivered.get(&t.id).copied())
        .collect();
    let mut target: Vec<f64> = order
        .iter()
        .zip(&fixed)
        .map(|(t, f)| f.unwrap_or(t.level_lufs()))
        .collect();
    let floor: Vec<f64> = order
        .iter()
        .zip(&fixed)
        .map(|(t, f)| f.unwrap_or(t.level_lufs() - LEVEL_CUT_MAX_LU))
        .collect();
    // Each pass only lowers a target, and no target falls under its floor,
    // so the passes end.
    let lower = |target: &mut Vec<f64>, k: usize, to: f64| {
        let to = to.max(floor[k]);
        if fixed[k].is_none() && target[k] > to + 1e-9 {
            target[k] = to;
            true
        } else {
            false
        }
    };
    loop {
        let mut changed = false;
        for k in 0..n.saturating_sub(1) {
            let rise_free = if k + 2 == n {
                CLOSER_RISE_LU
            } else {
                LEVEL_STEP_LU
            };
            if target[k] - target[k + 1] > LEVEL_STEP_LU + 1e-9 {
                let to = target[k + 1] + LEVEL_STEP_LU;
                changed |= lower(&mut target, k, to);
            }
            if target[k + 1] - target[k] > rise_free + 1e-9 {
                let to = target[k] + rise_free;
                changed |= lower(&mut target, k + 1, to);
            }
        }
        if !changed {
            break;
        }
    }
    Ok((0..n)
        .map(|k| Level {
            id: order[k].id.clone(),
            target_lufs: target[k],
            under_point_lu: order[k].level_lufs() - target[k],
            step_lu: (k > 0).then(|| target[k] - target[k - 1]),
            delivered: fixed[k].is_some(),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(
        id: &str,
        level: f64,
        tier: u8,
        key: u8,
        minor: bool,
        bpm: f64,
        seconds: f64,
    ) -> Track {
        Track {
            id: id.into(),
            seconds,
            lufs: level - 6.0,
            point_lufs: Some(level),
            crest_db: 8.0,
            width_db: -12.0,
            width_tier: tier,
            treble_side_db: Some(-12.0),
            tempo_bpm: bpm,
            tempo_strength: 0.8,
            key,
            minor,
            key_strength: 0.8,
            second_key: None,
            key_clear: true,
        }
    }

    #[test]
    fn the_key_relations_follow_the_circle_of_fifths() {
        let c_major = track("a", -8.0, 2, 0, false, 120.0, 200.0);
        let a_minor = track("b", -8.0, 2, 9, true, 120.0, 200.0);
        let g_major = track("c", -8.0, 2, 7, false, 120.0, 200.0);
        let f_sharp = track("d", -8.0, 2, 6, false, 120.0, 200.0);
        assert_eq!(key_step(&c_major, &a_minor), KeyStep::Relative);
        assert_eq!(key_step(&a_minor, &c_major), KeyStep::Relative);
        assert_eq!(key_step(&c_major, &g_major), KeyStep::Fifth);
        assert_eq!(key_step(&c_major, &f_sharp), KeyStep::Tritone);
    }

    #[test]
    fn a_half_time_tempo_is_the_same_tempo() {
        assert!((tempo_ratio(65.0, 130.0) - 1.0).abs() < 1e-9);
        assert!((tempo_ratio(97.0, 105.0) - 105.0 / 97.0).abs() < 1e-9);
    }

    #[test]
    fn the_single_opens_and_each_order_is_scored() {
        let tracks = vec![
            track("quiet", -11.0, 1, 0, false, 120.0, 150.0),
            track("single", -8.0, 3, 5, true, 97.0, 190.0),
            track("dense", -7.5, 3, 0, true, 105.0, 220.0),
            track("long", -9.0, 2, 7, false, 110.0, 300.0),
        ];
        let options = Options {
            single: Some("single".into()),
            vinyl: false,
        };
        let order = order(&tracks, &options).unwrap();
        assert!(order.exhaustive);
        assert_eq!(order.positions[0].id, "single");
        assert_eq!(
            order.positions[0].reasons,
            vec![Reason::Opens { single: true }]
        );
        // The cost of the chosen order is the least of each order.
        let ids: Vec<String> = order.positions.iter().map(|p| p.id.clone()).collect();
        let mine = order_of(&tracks, &ids, &options).unwrap();
        assert!((mine.cost - order.cost).abs() < 1e-9);
    }

    #[test]
    fn a_record_splits_into_two_sides_within_the_side_limit() {
        let tracks: Vec<Track> = (0..8)
            .map(|i| {
                track(
                    &format!("t{i}"),
                    -9.0 - 0.2 * i as f64,
                    2,
                    0,
                    false,
                    110.0,
                    200.0,
                )
            })
            .collect();
        let order = order(
            &tracks,
            &Options {
                single: None,
                vinyl: true,
            },
        )
        .unwrap();
        assert_eq!(order.sides_seconds, vec![800.0, 800.0]);
        assert!(order.positions.iter().take(4).all(|p| p.side == Some(1)));
        assert!(order.positions[4]
            .reasons
            .contains(&Reason::OpensSide { side: 2 }));
    }

    #[test]
    fn album_levels_cut_each_track_by_one_lu_at_most() {
        let tracks = vec![
            track("a", -7.2, 2, 0, false, 120.0, 200.0),
            track("b", -9.5, 2, 0, false, 120.0, 200.0),
            track("c", -8.0, 2, 0, false, 120.0, 200.0),
            track("d", -7.0, 2, 0, false, 120.0, 200.0),
        ];
        let ids: Vec<String> = ["a", "b", "c", "d"].iter().map(|s| s.to_string()).collect();
        let levels = levels(&tracks, &ids).unwrap();
        let t: Vec<f64> = levels.iter().map(|l| l.target_lufs).collect();
        for l in &levels {
            assert!(l.under_point_lu >= -1e-9 && l.under_point_lu <= LEVEL_CUT_MAX_LU + 1e-9);
        }
        // The quiet track lowers its neighbors by 1 LU at most, and the step
        // that stays is reported.
        assert!((t[0] - -8.2).abs() < 1e-9);
        assert!((levels[1].step_lu.unwrap() - -1.3).abs() < 1e-9);
        assert!((t[2] - -8.5).abs() < 1e-9);
        assert_eq!(t[3], -7.0);
        assert!(levels[0].step_lu.is_none());
    }

    #[test]
    fn a_mastered_track_keeps_its_loudness_and_its_neighbors_follow_it() {
        let tracks = vec![
            track("a", -8.0, 2, 0, false, 120.0, 200.0),
            track("b", -8.0, 2, 0, false, 120.0, 200.0),
            track("c", -8.0, 2, 0, false, 120.0, 200.0),
        ];
        let ids: Vec<String> = ["a", "b", "c"].iter().map(|s| s.to_string()).collect();
        let delivered = std::collections::BTreeMap::from([("a".to_string(), -9.6)]);
        let levels = levels_with(&tracks, &ids, &delivered).unwrap();
        assert!(levels[0].delivered && !levels[1].delivered);
        assert_eq!(levels[0].target_lufs, -9.6);
        assert!((levels[1].target_lufs - -8.6).abs() < 1e-9);
        assert_eq!(levels[2].target_lufs, -8.0);
    }

    #[test]
    fn a_key_that_is_not_clear_takes_the_relation_of_the_lower_cost() {
        let mut a = track("a", -8.0, 2, 0, false, 120.0, 200.0);
        let b = track("b", -8.0, 2, 7, false, 120.0, 200.0);
        let album = Album::of(&[a.clone(), b.clone()]);
        let options = Options::default();
        let fifth = costs(&[0, 1], &[a.clone(), b.clone()], &album, &options)[4];
        a.key_clear = false;
        a.second_key = Some(Key {
            key: 7,
            minor: false,
            strength: 0.78,
        });
        let unclear = costs(&[0, 1], &[a, b], &album, &options)[4];
        assert!(fifth > 0.0);
        assert_eq!(unclear, 0.0);
    }
}
