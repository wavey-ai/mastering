//! Measurements of supplied separation estimates. No model runs in the engine
//! and no separated waveform is substituted for the original stereo mix.
use crate::{
    analysis::percentile,
    audio_io::{Result, Stereo},
    foundation::{self, SpatialBands},
    resample,
    scratch::{self, Frames},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const FFT: usize = 8192;
const HOP: usize = 4096;
const BANDS: usize = 29;
/// Frames of each read of a mix in scratch storage.
const BLOCK: usize = 65_536;

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Band {
    pub low_hz: f64,
    pub high_hz: f64,
    pub center_hz: f64,
    pub vocal_energy_fraction: f64,
    pub role_energy_fractions: [f64; 4],
    pub all_activity_power_shares: [f64; 4],
    /// Powers of estimated stems are not additive powers of independent sources.
    pub estimated_power_shares: [f64; 4],
    /// Role r's share of total estimated power over the frames where r is
    /// active, indexed by r. Index 3 equals `estimated_power_shares[3]`.
    pub role_active_power_shares: [f64; 4],
    pub vocal_relative_p50_p95_db: [f64; 2],
}

/// Largest residual of the stem sum against the mix, in dB (D138).
pub const STEM_SUM_RESIDUAL_DB: f64 = -15.0;

/// The stereo image of one stem per stereo analysis band
/// (`stereo::BAND_EDGES_HZ`), for the source condition of an anti-phase
/// instrument group (`source_condition::antiphase_stems`).
#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct StemImage {
    #[serde(deserialize_with = "crate::static_text::text")]
    pub role: &'static str,
    pub bands: Vec<StemImageBand>,
}

#[derive(Clone, PartialEq, Serialize, Deserialize, Debug)]
pub struct StemImageBand {
    /// Whole-file left/right correlation, cross-spectrum phase and its
    /// resultant (`stereo::BandMetrics`).
    pub correlation: Option<f64>,
    pub cross_phase_degrees: Option<f64>,
    pub phase_resultant: Option<f64>,
    /// Seconds of active 0.5 s windows, and of those whose correlation is
    /// under the negative threshold of `stereo`.
    pub active_seconds: f64,
    pub negative_seconds: f64,
    pub longest_negative_run_seconds: f64,
    /// Start of the first and end of the last negative window, in seconds.
    pub negative_from_seconds: Option<f64>,
    pub negative_to_seconds: Option<f64>,
}

/// The stereo image of a stem at `rate`.
pub fn stem_image(role: &'static str, audio: &[Stereo], rate: u32) -> Result<StemImage> {
    image_of(role, crate::stereo::analyze(audio, rate)?, rate)
}

/// `stem_image` of the frames of `frames`, read in spans.
fn stem_image_read(role: &'static str, frames: &mut impl Frames, rate: u32) -> Result<StemImage> {
    image_of(role, crate::stereo::analyze_read(frames, rate)?, rate)
}

fn image_of(
    role: &'static str,
    image: crate::stereo::StereoAnalysis,
    rate: u32,
) -> Result<StemImage> {
    let bands = image
        .whole
        .bands
        .iter()
        .zip(&image.short_summary)
        .enumerate()
        .map(|(b, (whole, summary))| {
            let negative: Vec<&crate::stereo::StereoWindow> = image
                .short_windows
                .iter()
                .filter(|w| {
                    w.bands[b].active
                        && w.bands[b]
                            .correlation
                            .is_some_and(|c| c < crate::stereo::NEGATIVE_CORRELATION)
                })
                .collect();
            let seconds = |sample: usize| sample as f64 / rate as f64;
            StemImageBand {
                correlation: whole.correlation,
                cross_phase_degrees: whole.cross_phase_degrees,
                phase_resultant: whole.phase_resultant,
                active_seconds: summary.active_window_seconds,
                negative_seconds: summary.negative_correlation_window_seconds,
                longest_negative_run_seconds: summary
                    .longest_negative_correlation_window_run_seconds,
                negative_from_seconds: negative.first().map(|w| seconds(w.start_sample)),
                negative_to_seconds: negative.last().map(|w| seconds(w.end_sample)),
            }
        })
        .collect();
    Ok(StemImage { role, bands })
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    #[serde(deserialize_with = "crate::static_text::text")]
    pub rule: &'static str,
    #[serde(deserialize_with = "crate::static_text::text")]
    pub validation_status: &'static str,
    #[serde(deserialize_with = "crate::static_text::four")]
    pub roles: [&'static str; 4],
    pub source_pcm_sha256: String,
    pub source_frames: usize,
    pub stem_sample_rate: u32,
    pub stem_frames: usize,
    pub stem_pcm_sha256: [String; 4],
    pub reconstruction_pcm_sha256: String,
    pub sum_residual_relative_db: f64,
    pub sum_source_cosine: f64,
    pub usable: bool,
    #[serde(deserialize_with = "crate::static_text::text")]
    pub reason: &'static str,
    pub vocal_active_seconds: f64,
    /// Seconds each role is active: above its P95 minus 25 dB and above 2%
    /// of total estimated power.
    pub role_active_seconds: [f64; 4],
    /// Seconds where at least one role is active.
    pub any_active_seconds: f64,
    pub bands: Vec<Band>,
    pub vocal_transient_band: Option<usize>,
    pub bass_peak_band: Option<usize>,
    pub drum_decay_events: usize,
    pub drum_decay_p50_ms: Option<f64>,
    pub bass_body_hz: Option<f64>,
    pub limiter_release_ms: Option<f64>,
    pub drum_decay_p10_p50_p90_ms: Option<[f64; 3]>,
    pub drum_interval_p50_ms: Option<f64>,
    pub refinement: Option<crate::refinement::Analysis>,
    /// The de-esser key of the vocal stem against the drum, bass and other
    /// stems at the render rate (`deesser::key`).
    pub deesser_key: crate::deesser::Key,
    /// Frames where the other stems mask the lead stem in the low mids
    /// (`low_mid::key`).
    pub low_mid: Option<crate::low_mid::Key>,
    pub foundation: Option<foundation::Foundation>,
    /// The stereo image of each stem (`stem_image`), in role order.
    pub stem_images: Vec<StemImage>,
    #[serde(skip)]
    pub auto_focus: Option<crate::source_treatments::ProposalSet>,
}

/// The evidence of a plan in two invocations (D263), with the fields that
/// the evidence of a plan record leaves out.
#[derive(Serialize)]
struct StoredRef<'a> {
    evidence: &'a Evidence,
    low_mid_depth_db: Option<&'a [f32]>,
    auto_focus: Option<&'a crate::source_treatments::ProposalSet>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "'de: 'static"))]
struct Stored {
    evidence: Evidence,
    low_mid_depth_db: Option<Vec<f32>>,
    auto_focus: Option<crate::source_treatments::ProposalSet>,
}

impl Evidence {
    /// The share of the estimated stem power from 40 to 120 Hz that the
    /// drums and bass stems hold: the mean over the evidence bands centred
    /// in that range. `None` when the evidence is not usable.
    pub fn rhythm_low_share(&self) -> Option<f64> {
        let bands: Vec<&Band> = self
            .bands
            .iter()
            .filter(|b| (40.0..120.0).contains(&b.center_hz))
            .collect();
        (self.usable && !bands.is_empty()).then(|| {
            bands
                .iter()
                .map(|b| b.estimated_power_shares[0] + b.estimated_power_shares[1])
                .sum::<f64>()
                / bands.len() as f64
        })
    }

    /// The evidence as JSON for a later invocation of the plan (D263), or
    /// `None` when the JSON reads back as other evidence. A non-finite
    /// number reads back as another value.
    pub fn stored(&self) -> Result<Option<Vec<u8>>> {
        let bytes = serde_json::to_vec(&StoredRef {
            evidence: self,
            low_mid_depth_db: self.low_mid.as_ref().map(|key| key.depth_db.as_slice()),
            auto_focus: self.auto_focus.as_ref(),
        })?;
        let same = Self::from_stored(&bytes).is_ok_and(|read| read == *self);
        Ok(same.then_some(bytes))
    }

    /// The evidence of `stored`.
    pub fn from_stored(bytes: &[u8]) -> Result<Self> {
        // The `&'static str` fields read their texts into `static_text`. A
        // reader lends no text of the input to the evidence.
        let mut input = serde_json::Deserializer::from_reader(bytes);
        let Stored {
            mut evidence,
            low_mid_depth_db,
            auto_focus,
        } = Stored::deserialize(&mut input)?;
        input.end()?;
        if let (Some(key), Some(depth_db)) = (evidence.low_mid.as_mut(), low_mid_depth_db) {
            key.depth_db = depth_db;
        }
        evidence.auto_focus = auto_focus;
        Ok(evidence)
    }
}

pub fn fingerprint(audio: &[Stereo]) -> String {
    let mut hash = Fingerprint::default();
    hash.take(audio);
    hash.finish()
}

/// `fingerprint` of the frames of `audio`, read in blocks of 65,536 frames.
pub(crate) fn fingerprint_read(audio: &mut impl Frames) -> Result<String> {
    const BLOCK: usize = 65_536;
    let frames = audio.len();
    let mut hash = Fingerprint::default();
    let mut block = Vec::new();
    for start in (0..frames).step_by(BLOCK) {
        hash.take(audio.window(start, BLOCK.min(frames - start), &mut block)?);
    }
    Ok(hash.finish())
}

/// `fingerprint` of a recording that arrives in blocks.
#[derive(Default)]
pub(crate) struct Fingerprint {
    hash: Sha256,
    bytes: Vec<u8>,
}

impl Fingerprint {
    pub(crate) fn take(&mut self, audio: &[Stereo]) {
        // The little-endian samples in 64 KiB pieces: the same bytes as one
        // sample at a time, without a hash call for each sample.
        for frames in audio.chunks(8192) {
            self.bytes.clear();
            for sample in frames.iter().flatten() {
                self.bytes.extend_from_slice(&sample.to_le_bytes());
            }
            self.hash.update(&self.bytes);
        }
    }

    pub(crate) fn finish(self) -> String {
        format!("{:x}", self.hash.finalize())
    }
}

/// Frames where `role` is above its P95 minus 25 dB and above 2% of total
/// estimated power, in ascending order.
fn active_frames(powers: &[Vec<[f64; BANDS]>], role: usize) -> Vec<usize> {
    let role_power: Vec<f64> = powers[role].iter().map(|b| b.iter().sum()).collect();
    let gate = percentile(&role_power, 0.95) * 10f64.powf(-2.5);
    role_power
        .iter()
        .enumerate()
        .filter_map(|(i, &v)| {
            let total: f64 = powers.iter().map(|p| p[i].iter().sum::<f64>()).sum();
            (v > gate && v > total * 0.02 && v > 1e-20).then_some(i)
        })
        .collect()
}

fn band_frames(audio: &[Stereo], rate: u32) -> (Vec<[f64; BANDS]>, Vec<SpatialBands>) {
    band_frames_read(&mut scratch::Slice(audio), rate).expect("frames of a slice")
}

/// `band_frames` of the frames of `frames`, read in spans.
fn band_frames_read(
    frames: &mut impl Frames,
    rate: u32,
) -> Result<(Vec<[f64; BANDS]>, Vec<SpatialBands>)> {
    let bin_bands: Vec<_> = (0..=FFT / 2)
        .map(|bin| {
            let hz = bin as f64 * rate as f64 / FFT as f64;
            let index = (3.0 * (hz / 25.0).log2()).floor();
            (index >= 0.0 && index < BANDS as f64).then_some(index as usize)
        })
        .collect();
    let count = crate::stft::frame_count(frames.len() / HOP, FFT, HOP);
    let (mut powers, mut spatial) = (Vec::with_capacity(count), Vec::with_capacity(count));
    let frame = |l: &[rustfft::num_complex::Complex<f64>],
                 r: &[rustfft::num_complex::Complex<f64>]| {
        let mut bands = [0.0; BANDS];
        let mut domains = SpatialBands {
            mid: [0.0; BANDS],
            side: [0.0; BANDS],
        };
        for (bin, band) in bin_bands.iter().enumerate() {
            if let Some(index) = band {
                let mid = (l[bin] + r[bin]) * 0.5;
                let side = (l[bin] - r[bin]) * 0.5;
                domains.mid[*index] += mid.norm_sqr();
                domains.side[*index] += side.norm_sqr();
                bands[*index] += mid.norm_sqr() + side.norm_sqr();
            }
        }
        (bands, domains)
    };
    crate::stft::stereo_frames_read(
        frames,
        FFT,
        HOP,
        |x| x,
        frame,
        |(bands, domains)| {
            powers.push(bands);
            spatial.push(domains);
        },
    )?;
    Ok((powers, spatial))
}

/// Drum envelope peaks with a local rise and a measured 6 dB decay. Timing
/// comes from the estimated drum waveform, not sparse onsets in the full mix.
fn drum_decay(audio: &[Stereo], rate: u32) -> DrumDecay {
    let mut levels = Levels::new(rate);
    levels.take(audio);
    decay_of(&levels.levels)
}

/// The level of each whole 10 ms block of a recording at `rate`, in dB, as
/// the recording arrives in blocks of any length (`drum_decay`).
struct Levels {
    block: usize,
    /// The power and the frames of the block in progress.
    power: f64,
    frames: usize,
    levels: Vec<f64>,
}

impl Levels {
    fn new(rate: u32) -> Self {
        Self {
            block: rate as usize / 100,
            power: 0.0,
            frames: 0,
            levels: Vec::new(),
        }
    }

    fn take(&mut self, audio: &[Stereo]) {
        for frame in audio {
            for &v in frame {
                self.power += (v as f64).powi(2);
            }
            self.frames += 1;
            if self.frames == self.block {
                let p = self.power / (self.block * 2) as f64;
                self.levels.push(10.0 * p.max(1e-30).log10());
                (self.power, self.frames) = (0.0, 0);
            }
        }
    }
}

/// `drum_decay` of the 10 ms levels `levels`.
fn decay_of(levels: &[f64]) -> DrumDecay {
    let gate = percentile(levels, 0.95) - 15.0;
    let mut decays = Vec::new();
    let mut intervals = Vec::new();
    let mut last = 0;
    for i in 10..levels.len().saturating_sub(30) {
        if i - last < 10
            || levels[i] < gate
            || levels[i] < levels[i - 1]
            || levels[i] <= levels[i + 1]
            || levels[i]
                - levels[i - 8..i]
                    .iter()
                    .copied()
                    .fold(f64::INFINITY, f64::min)
                < 3.0
        {
            continue;
        }
        if let Some(offset) = (1..=30).find(|&j| levels[i + j] <= levels[i] - 6.0) {
            if !decays.is_empty() {
                intervals.push((i - last) as f64 * 10.0);
            }
            decays.push(offset as f64 * 10.0);
            last = i;
        }
    }
    (
        decays.len(),
        (decays.len() >= 16).then(|| percentile(&decays, 0.5)),
        (decays.len() >= 16).then(|| [0.1, 0.5, 0.9].map(|p| percentile(&decays, p))),
        (decays.len() >= 16).then(|| percentile(&intervals, 0.5)),
    )
}

/// Stems must be unnormalized, aligned drums/bass/other/vocals from one split.
/// One frame of endpoint rounding is allowed after sample-rate conversion.
/// The stems are at `stem_rate` and `input` at the render rate `rate`.
pub fn analyze(
    input: &[Stereo],
    stems: [&[Stereo]; 4],
    stem_rate: u32,
    rate: u32,
) -> Result<Evidence> {
    Ok(analyze_with_reconstruction(input, stems, stem_rate, rate)?.0)
}

/// `analyze`, and the sum of the stems at the render rate `rate`.
pub fn analyze_with_reconstruction(
    input: &[Stereo],
    stems: [&[Stereo]; 4],
    stem_rate: u32,
    rate: u32,
) -> Result<(Evidence, Vec<Stereo>)> {
    crate::audio_io::check_render_rate(rate)?;
    if input.len() < 3 * rate as usize
        || input.iter().flatten().any(|v| !v.is_finite())
        || !resample::RATES.contains(&stem_rate)
        || stems[0].is_empty()
        || stems
            .iter()
            .any(|s| s.len() != stems[0].len() || s.iter().flatten().any(|v| !v.is_finite()))
    {
        return Err(
            "Stem evidence requires finite, aligned stereo estimates at one supported rate".into(),
        );
    }
    let expected = (stems[0].len() as f64 * rate as f64 / stem_rate as f64).round() as usize;
    if expected.abs_diff(input.len()) > 1 {
        return Err("Stem duration does not match the original mix".into());
    }
    let sum: Vec<Stereo> = (0..stems[0].len())
        .map(|i| [0, 1].map(|c| stems.iter().map(|s| s[i][c]).sum()))
        .collect();
    let reconstructed = resample::convert(&sum, stem_rate, rate)?;
    let mut powers = Vec::with_capacity(4);
    let mut spatial = Vec::with_capacity(4);
    let mut images = Vec::with_capacity(4);
    for (stem, role) in stems.iter().zip(["drums", "bass", "other", "vocals"]) {
        let (bands, domains) = band_frames(stem, stem_rate);
        powers.push(bands);
        spatial.push(domains);
        images.push(stem_image(role, stem, stem_rate)?);
    }
    let mut evidence = finish_evidence(
        &mut scratch::Slice(input),
        fingerprint(input),
        Observations {
            rate: stem_rate,
            frames: stems[0].len(),
            hashes: stems.map(fingerprint),
            powers,
            spatial,
            drum: drum_decay(stems[0], stem_rate),
            images,
        },
        &mut scratch::Slice(&reconstructed),
        rate,
    )?;
    if evidence.usable {
        // The masking frames of each stem serve the low-mid key and the
        // de-esser key; the mid band powers of the drum, bass and other
        // stems serve the de-esser key.
        let mut roles = Vec::with_capacity(4);
        let mut others: [Vec<f64>; 3] = Default::default();
        for (role, stem) in stems.iter().enumerate() {
            let at_rate = resample::at_rate(stem, stem_rate, rate)?;
            let frames = crate::masking::frames(
                &at_rate,
                crate::masking::Channels::Both,
                crate::masking::Measures::Powers,
                rate,
            )
            .powers;
            if role < 3 {
                others[role] = crate::deesser::sibilant_powers(&at_rate, rate);
            } else {
                evidence.deesser_key = crate::deesser::key_with_frames(
                    &crate::deesser::mid_powers(&at_rate, rate),
                    &frames,
                    &others,
                    &crate::deesser::mid_powers(input, rate),
                );
            }
            roles.push(frames);
        }
        evidence.low_mid = low_mid_key(&evidence, || mix_flatness(input, rate), &roles, rate);
    }
    Ok((evidence, reconstructed))
}

/// The spectral flatness of each masking frame of `input` at `rate`.
fn mix_flatness(input: &[Stereo], rate: u32) -> Vec<f64> {
    crate::masking::frames(
        input,
        crate::masking::Channels::Both,
        crate::masking::Measures::PowersAndFlatness,
        rate,
    )
    .flatness_db
}

/// The low-mid key of the lead role against the power sum of the other
/// roles, with the tonality of the mix (`mix_flatness`). The frames of the
/// roles are at the rate of the mix, `rate`.
fn low_mid_key(
    evidence: &Evidence,
    mix_flatness: impl FnOnce() -> Vec<f64>,
    roles: &[Vec<[f64; crate::masking::BANDS]>],
    rate: u32,
) -> Option<crate::low_mid::Key> {
    let lead = crate::broad_tone::lead(evidence)?;
    let frames = roles.iter().map(Vec::len).min()?;
    let others: Vec<[f64; crate::masking::BANDS]> = (0..frames)
        .map(|t| {
            std::array::from_fn(|z| {
                (0..roles.len())
                    .filter(|&r| r != lead.index)
                    .map(|r| roles[r][t][z])
                    .sum()
            })
        })
        .collect();
    crate::low_mid::key(
        &roles[lead.index],
        &others,
        &mix_flatness(),
        lead.index,
        rate,
    )
}

type DrumDecay = (usize, Option<f64>, Option<[f64; 3]>, Option<f64>);
struct Observations {
    rate: u32,
    frames: usize,
    hashes: [String; 4],
    powers: Vec<Vec<[f64; BANDS]>>,
    spatial: Vec<Vec<SpatialBands>>,
    drum: DrumDecay,
    images: Vec<StemImage>,
}

/// Measure each raw stem in turn. `input` is at the render rate `rate`.
/// `load(role, each)` gives the frames of the stem of `role` to `each`,
/// block by block, with the sample rate of the stem. Each stem waits in
/// scratch storage, and its measures read it in spans (D245).
pub fn analyze_sequential(
    input: &[Stereo],
    rate: u32,
    mut load: impl FnMut(&'static str, &mut Blocks<'_>) -> Result<()>,
    proposals: Option<&crate::source_treatments::ProposalSet>,
    refine: bool,
) -> Result<Evidence> {
    analyze_sequential_inner(Mix::Whole(input), rate, &mut load, proposals, refine, None)
}

/// Set analysis uses the same local auditory measurement on native and WASM.
/// Imported diagnostics still take precedence when explicitly supplied.
/// `input` is at the render rate `rate`, and `load` is that of
/// `analyze_sequential`. While the stems load, `input` waits in scratch
/// storage (`scratch::temporary`) and holds no frames; it holds the mix
/// again when the analysis returns.
pub fn analyze_set(
    input: &mut Vec<Stereo>,
    rate: u32,
    mut load: impl FnMut(&'static str, &mut Blocks<'_>) -> Result<()>,
    proposals: Option<&crate::source_treatments::ProposalSet>,
    source_file_sha256: &str,
) -> Result<Evidence> {
    analyze_sequential_inner(
        Mix::Held(input),
        rate,
        &mut load,
        proposals,
        true,
        Some(source_file_sha256),
    )
}

/// `analyze_set` of a mix in scratch storage (D250).
pub fn analyze_set_stored(
    mix: &mut crate::audio_io::StoredSource,
    mut load: impl FnMut(&'static str, &mut Blocks<'_>) -> Result<()>,
    proposals: Option<&crate::source_treatments::ProposalSet>,
    source_file_sha256: &str,
) -> Result<Evidence> {
    let rate = mix.sample_rate;
    analyze_sequential_inner(
        Mix::Stored(&mut mix.audio),
        rate,
        &mut load,
        proposals,
        true,
        Some(source_file_sha256),
    )
}

/// The mix of a sequential stem analysis.
enum Mix<'a> {
    Whole(&'a [Stereo]),
    /// The mix of the caller, which waits in scratch storage while the stems
    /// load.
    Held(&'a mut Vec<Stereo>),
    /// A mix in scratch storage.
    Stored(&'a mut scratch::Pcm),
}

/// The frames of a `Mix` after the measures that read the whole mix.
enum MixFrames<'a> {
    Whole(scratch::Slice<'a>),
    Held {
        stored: scratch::Pcm,
        audio: &'a mut Vec<Stereo>,
    },
    Stored(&'a mut scratch::Pcm),
}

impl<'a> Mix<'a> {
    /// The measures of the whole mix at `rate`, and an error for a mix
    /// shorter than three seconds or with a sample that is not finite. A
    /// stored mix reads in blocks and spans (D250).
    fn measures(&mut self, rate: u32) -> Result<MixMeasures> {
        let input = match self {
            Mix::Whole(audio) => &audio[..],
            Mix::Held(audio) => &audio[..],
            Mix::Stored(stored) => return measures_read(stored, rate),
        };
        if input.len() < 3 * rate as usize || input.iter().flatten().any(|x| !x.is_finite()) {
            return Err("Stem evidence requires finite source PCM".into());
        }
        Ok(MixMeasures {
            frames: input.len(),
            pcm_sha256: fingerprint(input),
            mid: crate::deesser::mid_powers(input, rate),
            flatness: mix_flatness(input, rate),
        })
    }

    /// The frames of the mix. A held mix moves to scratch storage, and its
    /// buffer is freed. In the browser engine the buffer waits empty for
    /// the mix to read back into it (`MixFrames::restore`).
    fn frames(self) -> Result<MixFrames<'a>> {
        Ok(match self {
            Mix::Whole(audio) => MixFrames::Whole(scratch::Slice(audio)),
            Mix::Held(audio) => {
                let stored = scratch::Pcm::temporary_of(audio)?;
                // The linear memory of a module never shrinks, and the
                // allocations of the stem stages split a freed buffer, so
                // that the restored mix would grow the linear memory by one
                // buffer (D235, D246).
                if cfg!(target_arch = "wasm32") {
                    audio.clear();
                } else {
                    *audio = Vec::new();
                }
                MixFrames::Held { stored, audio }
            }
            Mix::Stored(stored) => MixFrames::Stored(stored),
        })
    }
}

/// `Mix::measures` of a mix in scratch storage: one pass over blocks of
/// 65,536 frames for the checks and the fingerprint, and the masking frames
/// of the mid channel and of the mix in spans.
fn measures_read(mix: &mut scratch::Pcm, rate: u32) -> Result<MixMeasures> {
    const BLOCK: usize = 65_536;
    let frames = mix.len();
    if frames < 3 * rate as usize {
        return Err("Stem evidence requires finite source PCM".into());
    }
    let mut hash = Fingerprint::default();
    let mut block = Vec::new();
    for start in (0..frames).step_by(BLOCK) {
        mix.read(start, BLOCK.min(frames - start), &mut block)?;
        if block.iter().flatten().any(|x| !x.is_finite()) {
            return Err("Stem evidence requires finite source PCM".into());
        }
        hash.take(&block);
    }
    let mid = crate::masking::frames_read(
        mix,
        |x| [crate::deesser::mid(x); 2],
        crate::masking::Channels::Left,
        crate::masking::Measures::Powers,
        rate,
    )?
    .powers;
    let flatness = crate::masking::frames_read(
        mix,
        |x| x,
        crate::masking::Channels::Both,
        crate::masking::Measures::PowersAndFlatness,
        rate,
    )?
    .flatness_db;
    Ok(MixMeasures {
        frames,
        pcm_sha256: hash.finish(),
        mid,
        flatness,
    })
}

impl MixFrames<'_> {
    /// Read a held mix back into the buffer of the caller.
    fn restore(self) -> Result<()> {
        if let MixFrames::Held { mut stored, audio } = self {
            let frames = stored.len();
            stored.read(0, frames, audio)?;
        }
        Ok(())
    }
}

impl Frames for MixFrames<'_> {
    fn len(&self) -> usize {
        match self {
            MixFrames::Whole(audio) => audio.len(),
            MixFrames::Held { stored, .. } => stored.len(),
            MixFrames::Stored(stored) => stored.len(),
        }
    }

    fn read(&mut self, start: usize, count: usize, output: &mut Vec<Stereo>) -> Result<()> {
        match self {
            MixFrames::Whole(audio) => audio.read(start, count, output),
            MixFrames::Held { stored, .. } => stored.read(start, count, output),
            MixFrames::Stored(stored) => stored.read(start, count, output),
        }
    }
}

/// The measures of the whole mix that the stem analysis uses.
struct MixMeasures {
    frames: usize,
    pcm_sha256: String,
    /// The mid band powers of the de-esser key (`deesser::mid_powers`).
    mid: Vec<[f64; crate::masking::BANDS]>,
    /// The tonality of the low-mid key (`mix_flatness`).
    flatness: Vec<f64>,
}

/// The receiver of the blocks of a stem: the sample rate of the stem and
/// its next frames.
pub type Blocks<'a> = dyn FnMut(u32, &[Stereo]) -> Result<()> + 'a;

fn analyze_sequential_inner(
    mix: Mix<'_>,
    rate: u32,
    load: &mut impl FnMut(&'static str, &mut Blocks<'_>) -> Result<()>,
    proposals: Option<&crate::source_treatments::ProposalSet>,
    refine: bool,
    auto_source_hash: Option<&str>,
) -> Result<Evidence> {
    crate::audio_io::check_render_rate(rate)?;
    let mut mix = mix;
    let measures = mix.measures(rate)?;
    let mut input = mix.frames()?;
    let evidence = analyze_stems(
        &mut input,
        measures,
        rate,
        load,
        proposals,
        refine,
        auto_source_hash,
    );
    input.restore()?;
    evidence
}

/// The stem loop of `analyze_sequential_inner`, with the frames of the mix
/// and its `measures`.
fn analyze_stems(
    input: &mut MixFrames<'_>,
    measures: MixMeasures,
    rate: u32,
    load: &mut impl FnMut(&'static str, &mut Blocks<'_>) -> Result<()>,
    proposals: Option<&crate::source_treatments::ProposalSet>,
    refine: bool,
    auto_source_hash: Option<&str>,
) -> Result<Evidence> {
    let MixMeasures {
        frames: mix_frames,
        pcm_sha256,
        mid: mix_mid,
        flatness,
    } = measures;
    let mut observations = Observations {
        rate: 0,
        frames: 0,
        hashes: std::array::from_fn(|_| String::new()),
        powers: Vec::new(),
        spatial: Vec::new(),
        drum: (0, None, None, None),
        images: Vec::new(),
    };
    // The sum of the stems waits in scratch storage, and each stem adds to it
    // as it decodes (D234): the drum stem starts the sum.
    let mut sum: Option<scratch::Pcm> = None;
    // With an automatic focus, the accompaniment gives its auditory frames
    // at the end of the other stage.
    let auto_source_hash = auto_source_hash.filter(|_| proposals.is_none());
    let mut accompaniment: Option<crate::auditory::Measured> = None;
    // The vocal at the render rate, in scratch storage for the refinement.
    let mut vocal: Option<scratch::Pcm> = None;
    let mut deesser_key = crate::deesser::Key::default();
    // The mid 5 to 10 kHz power of each frame of the drum, bass and other
    // stems at the render rate, for the de-esser key.
    let mut others: [Vec<f64>; 3] = Default::default();
    let mut role_masking = Vec::with_capacity(4);
    let mut auto_focus = None;
    for (role, name) in ["drums", "bass", "other", "vocals"].into_iter().enumerate() {
        crate::progress::report(crate::progress::Stage {
            stage: "stem",
            part: Some(name),
            index: Some(role + 1),
            count: Some(4),
            ..Default::default()
        });
        let mut reader = StemReader {
            role,
            rate: (role > 0).then_some(observations.rate),
            frames: 0,
            limit: if role == 0 {
                usize::MAX
            } else {
                observations.frames
            },
            stored: scratch::Pcm::new(scratch::temporary()?),
            hash: Fingerprint::default(),
            levels: None,
            sum: sum.as_mut(),
        };
        load(name, &mut |stem_rate, block| reader.take(stem_rate, block))?;
        let StemReader {
            rate: stem_rate,
            frames,
            stored: mut stem,
            hash,
            levels,
            ..
        } = reader;
        let stem_rate = stem_rate.unwrap_or(0);
        if role == 0 {
            observations.rate = stem_rate;
            observations.frames = frames;
            let expected = (frames as f64 * rate as f64 / stem_rate as f64).round() as usize;
            if !resample::RATES.contains(&stem_rate)
                || frames == 0
                || expected.abs_diff(mix_frames) > 1
            {
                return Err("Stem duration or sample rate does not match the original mix".into());
            }
        }
        if stem_rate != observations.rate || frames != observations.frames {
            return Err("Stem evidence requires finite, aligned estimates at one rate".into());
        }
        observations.hashes[role] = hash.finish();
        let (powers, spatial) = band_frames_read(&mut stem, stem_rate)?;
        observations.powers.push(powers);
        observations.spatial.push(spatial);
        observations
            .images
            .push(stem_image_read(name, &mut stem, stem_rate)?);
        if let Some(levels) = levels {
            observations.drum = decay_of(&levels.levels);
        }
        if role < 3 {
            let (frames, mid) = masking_pairs(&mut stem, stem_rate, rate)?;
            role_masking.push(frames.powers);
            others[role] = crate::deesser::sibilant_of(&mid.powers);
        } else if let (Some(source_hash), Some(frames)) = (auto_source_hash, accompaniment.take()) {
            let vocal_frames = crate::auditory::measure_stem_read(&mut stem, stem_rate)?;
            auto_focus = Some(crate::auditory::propose_from_measured(
                &vocal_frames,
                &frames,
                source_hash,
                &pcm_sha256,
                rate,
            )?);
        }
        if role == 0 {
            sum = Some(stem);
        } else if role == 2 && auto_source_hash.is_some() {
            let stored = sum.as_mut().unwrap();
            accompaniment = Some(crate::auditory::measure_stem_read(stored, stem_rate)?);
        } else if role == 3 {
            // The vocal at the render rate and its masking frames serve the
            // low-mid key, the de-esser key and the refinement.
            let mut at_rate = if stem_rate == rate {
                stem
            } else {
                resample::convert_stored(&mut stem, stem_rate, rate)?
            };
            let frames = crate::masking::frames_read(
                &mut at_rate,
                |x| x,
                crate::masking::Channels::Both,
                crate::masking::Measures::Powers,
                rate,
            )?
            .powers;
            let stem_mid = crate::masking::frames_read(
                &mut at_rate,
                |x| [crate::deesser::mid(x); 2],
                crate::masking::Channels::Left,
                crate::masking::Measures::Powers,
                rate,
            )?
            .powers;
            deesser_key = crate::deesser::key_with_frames(&stem_mid, &frames, &others, &mix_mid);
            role_masking.push(frames);
            if refine {
                vocal = Some(at_rate);
            }
        }
    }
    crate::progress::step("stems");
    // The sum stays in scratch storage at the render rate (D246). The sum
    // at the rate of the stems is freed after the conversion.
    let sum = sum.ok_or("Stem evidence requires four stems")?;
    let mut reconstructed = if observations.rate == rate {
        sum
    } else {
        let mut sum = sum;
        resample::convert_stored(&mut sum, observations.rate, rate)?
    };
    let mut evidence = finish_evidence(input, pcm_sha256, observations, &mut reconstructed, rate)?;
    if evidence.usable {
        evidence.auto_focus = auto_focus;
        evidence.deesser_key = deesser_key;
        evidence.low_mid = low_mid_key(&evidence, || flatness, &role_masking, rate);
    }
    drop(role_masking);
    if refine {
        crate::progress::step("focus");
        // With `refine`, the vocal stage stored the vocal.
        evidence.refinement = Some(crate::refinement::analyze_matched(
            input,
            vocal.as_mut().unwrap(),
            &evidence,
            proposals.or(evidence.auto_focus.as_ref()),
            &mut reconstructed,
            rate,
        )?);
    }
    Ok(evidence)
}

/// The receiver of the blocks of one stem. It stores the stem, and it
/// takes the measures that read each frame once: the fingerprint, the
/// 10 ms levels of the drums and the sum of the stems.
struct StemReader<'a> {
    role: usize,
    /// The sample rate of the stem: for each stem after the first, the
    /// rate of the first.
    rate: Option<u32>,
    frames: usize,
    /// The frames of the first stem, for each stem after it.
    limit: usize,
    stored: scratch::Pcm,
    hash: Fingerprint,
    levels: Option<Levels>,
    /// The sum of the stems before this one.
    sum: Option<&'a mut scratch::Pcm>,
}

impl StemReader<'_> {
    fn take(&mut self, rate: u32, block: &[Stereo]) -> Result<()> {
        if self.role == 0 && self.rate.is_none() {
            if !resample::RATES.contains(&rate) {
                return Err("Stem duration or sample rate does not match the original mix".into());
            }
            self.rate = Some(rate);
            self.levels = Some(Levels::new(rate));
        }
        if self.rate != Some(rate)
            || block.len() > self.limit - self.frames
            || block.iter().flatten().any(|x| !x.is_finite())
        {
            return Err("Stem evidence requires finite, aligned estimates at one rate".into());
        }
        self.hash.take(block);
        if let Some(levels) = self.levels.as_mut() {
            levels.take(block);
        }
        // The first stem is the sum: -0.0 + x is x.
        if let Some(sum) = self.sum.as_mut() {
            sum.add_at(self.frames, block)?;
        }
        self.stored.push(block)?;
        self.frames += block.len();
        Ok(())
    }
}

/// The masking frames of the stem in `stem` at `stem_rate`, at the render
/// rate `rate`, and those of its mid channel with a silent right channel:
/// its power in both channels is the power of the mid channel
/// (`deesser::sibilant_powers`). The stem converts to `rate` one span of
/// frames at a time. The frames hold the band powers (D317).
fn masking_pairs(
    stem: &mut impl Frames,
    stem_rate: u32,
    rate: u32,
) -> Result<(crate::masking::Frames, crate::masking::Frames)> {
    use crate::masking;
    let at_rate = resample::Windowed::new(stem.len(), stem_rate, rate)?;
    let hops = at_rate.frames() / masking::HOP;
    let count = crate::stft::frame_count(hops, masking::FFT, masking::HOP);
    let mut pairs = [(); 2].map(|_| masking::Frames::with_capacity(count));
    // Each span starts at a multiple of `stft::PAIR_RANGE`, so its frames
    // equal those frames of the whole stem.
    let span = crate::stft::PAIR_RANGE * crate::dsp::threads().min(8);
    let (mut window, mut frames) = (Vec::new(), Vec::new());
    for first in (0..hops).step_by(span) {
        let range = first..(first + span).min(hops);
        let reads = masking::pair_reads(range.clone());
        let source = at_rate.source_range(reads.clone());
        stem.read(source.start, source.len(), &mut window)?;
        frames.clear();
        frames.resize(reads.len(), [0.0; 2]);
        at_rate.fill_from(&window, source.start, reads.start, &mut frames);
        let sample = |n: usize| {
            let frame = frames[n - reads.start];
            [frame, [crate::deesser::mid(frame), 0.0]]
        };
        let powers = [masking::Measures::Powers; 2];
        let (a, b) = masking::frame_pairs_in(range, sample, masking::Channels::Both, powers, rate);
        pairs[0].append(a);
        pairs[1].append(b);
    }
    let [a, b] = pairs;
    Ok((a, b))
}

/// The evidence of `observations` against `input`, whose fingerprint is
/// `source_pcm_sha256`; `input` and the sum of the stems `reconstructed`
/// are at the render rate `render_rate`. Each is read in blocks.
fn finish_evidence(
    input: &mut impl Frames,
    source_pcm_sha256: String,
    observations: Observations,
    reconstructed: &mut impl Frames,
    render_rate: u32,
) -> Result<Evidence> {
    let Observations {
        rate,
        frames,
        hashes,
        powers,
        spatial,
        drum,
        images,
    } = observations;
    let (mut xx, mut yy, mut xy, mut residual) = (0.0, 0.0, 0.0, 0.0);
    let compared = input.len().min(reconstructed.len());
    let mut hash = Fingerprint::default();
    let (mut block, mut sum) = (Vec::new(), Vec::new());
    for start in (0..reconstructed.len()).step_by(BLOCK) {
        reconstructed.read(start, BLOCK.min(reconstructed.len() - start), &mut sum)?;
        hash.take(&sum);
        let count = BLOCK.min(compared.saturating_sub(start));
        input.read(start, count, &mut block)?;
        for (a, b) in block.iter().zip(&sum[..count]) {
            for c in 0..2 {
                let (x, y) = (a[c] as f64, b[c] as f64);
                xx += x * x;
                yy += y * y;
                xy += x * y;
                residual += (x - y).powi(2);
            }
        }
    }
    let residual_db = 10.0 * (residual / xx.max(1e-30)).max(1e-30).log10();
    let cosine = xy / (xx * yy).sqrt().max(1e-30);
    // The cosine bound rejects stems from another mix or out of alignment.
    // The residual bound admits the coding error of the cloud's 192 kbps
    // Opus stems (about -17.6 dB); lossless stems reach about -31 dB.
    let aligned = xx > 1e-12 && residual_db <= STEM_SUM_RESIDUAL_DB && cosine >= 0.98;
    let vocal_power: Vec<f64> = powers[3].iter().map(|b| b.iter().sum()).collect();
    let role_active: [Vec<usize>; 4] = std::array::from_fn(|role| active_frames(&powers, role));
    let active = role_active[3].clone();
    let seconds = |n: usize| n as f64 * HOP as f64 / rate as f64;
    let active_seconds = seconds(active.len());
    let role_active_seconds: [f64; 4] =
        std::array::from_fn(|role| seconds(role_active[role].len()));
    let any_active_seconds = seconds(
        (0..powers[0].len())
            .filter(|i| {
                role_active
                    .iter()
                    .any(|frames| frames.binary_search(i).is_ok())
            })
            .count(),
    );
    let vocal_total: f64 = active.iter().map(|&i| vocal_power[i]).sum();
    let all_energy: [[f64; BANDS]; 4] = std::array::from_fn(|role| {
        std::array::from_fn(|b| powers[role].iter().map(|frame| frame[b]).sum())
    });
    let role_totals: [f64; 4] = all_energy.map(|bands| bands.iter().sum::<f64>());
    let bands: Vec<_> = (0..BANDS)
        .map(|b| {
            let energy: [f64; 4] =
                std::array::from_fn(|role| active.iter().map(|&i| powers[role][i][b]).sum());
            let total = energy.iter().sum::<f64>().max(1e-30);
            let ratios: Vec<_> = active
                .iter()
                .map(|&i| 10.0 * (powers[3][i][b] / vocal_power[i]).max(1e-30).log10())
                .collect();
            let low = 25.0 * 2f64.powf(b as f64 / 3.0);
            let high = 25.0 * 2f64.powf((b + 1) as f64 / 3.0);
            Band {
                low_hz: low,
                high_hz: high,
                center_hz: (low * high).sqrt(),
                vocal_energy_fraction: energy[3] / vocal_total.max(1e-30),
                role_energy_fractions: std::array::from_fn(|role| {
                    all_energy[role][b] / role_totals[role].max(1e-30)
                }),
                all_activity_power_shares: std::array::from_fn(|role| {
                    all_energy[role][b] / all_energy.iter().map(|r| r[b]).sum::<f64>().max(1e-30)
                }),
                estimated_power_shares: energy.map(|p| p / total),
                role_active_power_shares: std::array::from_fn(|role| {
                    let frames = &role_active[role];
                    let own: f64 = frames.iter().map(|&i| powers[role][i][b]).sum();
                    let all: f64 = frames
                        .iter()
                        .map(|&i| (0..4).map(|r| powers[r][i][b]).sum::<f64>())
                        .sum();
                    own / all.max(1e-30)
                }),
                vocal_relative_p50_p95_db: [percentile(&ratios, 0.5), percentile(&ratios, 0.95)],
            }
        })
        .collect();
    let usable = aligned;
    let foundation = if usable {
        foundation::propose(&spatial, input, reconstructed, rate, render_rate)?
    } else {
        None
    };
    // Locate intermittent vocal-associated treble; presence/body remain evidence,
    // since their mere energy does not establish a need for brightening or a cut.
    let vocal_transient_band = (usable && active_seconds >= 3.0)
        .then(|| {
            bands
                .iter()
                .enumerate()
                .filter(|(_, b)| {
                    let [p50, p95] = b.vocal_relative_p50_p95_db;
                    (5000.0..=11000.0).contains(&b.center_hz)
                        && b.estimated_power_shares[3] >= 0.5
                        && p95 - p50 > 6.0
                        && p95 > -20.0
                })
                .max_by(|(_, a), (_, b)| {
                    a.vocal_energy_fraction.total_cmp(&b.vocal_energy_fraction)
                })
                .map(|(i, _)| i)
        })
        .flatten();
    let bass_body = bands
        .iter()
        .enumerate()
        .filter(|(_, b)| {
            (35.0..=200.0).contains(&b.center_hz) && b.all_activity_power_shares[1] >= 0.5
        })
        .max_by(|(_, a), (_, b)| a.role_energy_fractions[1].total_cmp(&b.role_energy_fractions[1]));
    let bass_body_hz = usable
        .then(|| bass_body.map(|(_, b)| b.center_hz))
        .flatten();
    let bass_peak_band = usable.then(|| bass_body.map(|(i, _)| i)).flatten();
    let (drum_decay_events, drum_decay_p50_ms, drum_decay_p10_p50_p90_ms, drum_interval_p50_ms) =
        drum;
    // Allow recovery between hits, with at least two cycles of the dominant
    // estimated bass body. The bounds are a timing heuristic, not a THD bound.
    let limiter_release_ms = if usable {
        drum_decay_p50_ms.map(|decay| {
            (decay * 0.5)
                .max(bass_body_hz.map_or(35.0, |hz| 2000.0 / hz))
                .clamp(35.0, 80.0)
        })
    } else {
        None
    };
    Ok(Evidence {
        rule: "stem-evidence.2",
        validation_status: "engineering-hypothesis",
        roles: ["drums", "bass", "other", "vocals"],
        source_pcm_sha256,
        source_frames: input.len(),
        stem_sample_rate: rate,
        stem_frames: frames,
        stem_pcm_sha256: hashes,
        reconstruction_pcm_sha256: hash.finish(),
        sum_residual_relative_db: residual_db,
        sum_source_cosine: cosine,
        usable,
        reason: if !aligned {
            "Stem sum does not sufficiently match the original mix; no stem-informed processing proposed"
        } else {
            "Aligned estimates inform drum recovery, bass headroom and vocal-band trials on the original mix"
        },
        vocal_active_seconds: active_seconds,
        role_active_seconds,
        any_active_seconds,
        bands,
        vocal_transient_band,
        bass_peak_band,
        drum_decay_events,
        drum_decay_p50_ms,
        bass_body_hz,
        limiter_release_ms,
        drum_decay_p10_p50_p90_ms,
        drum_interval_p50_ms,
        refinement: None,
        deesser_key: crate::deesser::Key::default(),
        low_mid: None,
        foundation,
        stem_images: images,
        auto_focus: None,
    })
}

#[cfg(test)]
mod fingerprint_tests {
    use super::*;

    #[test]
    fn the_fingerprint_hashes_each_sample_in_order() {
        let audio: Vec<Stereo> = (0..20_001)
            .map(|i| [i as f32 * 1e-4 - 1.0, -(i as f32).sin()])
            .collect();
        let mut hash = Sha256::new();
        for sample in audio.iter().flatten() {
            hash.update(sample.to_le_bytes());
        }
        assert_eq!(fingerprint(&audio), format!("{:x}", hash.finalize()));
        assert_eq!(fingerprint(&[]), format!("{:x}", Sha256::new().finalize()));
    }
}

#[cfg(test)]
mod held_mix_tests {
    use super::*;

    /// A mix of 6.3 s at 48 kHz and its four stems: a drum burst, a bass, an
    /// other part and a vocal with a treble burst.
    fn material() -> (Vec<Stereo>, [Vec<Stereo>; 4]) {
        let frames = 6 * 48_000 + 12_345;
        let stems: [Vec<Stereo>; 4] = std::array::from_fn(|role| {
            (0..frames)
                .map(|i| {
                    let t = i as f64 / 48_000.0;
                    let burst = if i % 24_000 < 1_200 { 1.0 } else { 0.0 };
                    let sine = |hz: f64| (std::f64::consts::TAU * hz * t).sin();
                    let x = match role {
                        0 => 0.025 * sine(170.0) * burst,
                        1 => 0.025 * sine(75.0),
                        2 => 0.015 * sine(1_700.0),
                        _ => 0.03 * sine(700.0) + 0.14 * sine(7_000.0) * burst,
                    };
                    [x as f32, (0.8 * x) as f32]
                })
                .collect()
        });
        let mix = (0..frames)
            .map(|i| [0, 1].map(|c| stems.iter().map(|s| s[i][c]).sum()))
            .collect();
        (mix, stems)
    }

    /// The evidence of `stems` at `rates`, each in blocks of `block` frames.
    fn in_blocks(
        mix: Mix<'_>,
        stems: &[Vec<Stereo>; 4],
        block: usize,
        rates: [u32; 4],
    ) -> Result<Evidence> {
        analyze_sequential_inner(
            mix,
            48_000,
            &mut |role, each| {
                let index = ["drums", "bass", "other", "vocals"]
                    .iter()
                    .position(|r| *r == role)
                    .unwrap();
                for frames in stems[index].chunks(block) {
                    each(rates[index], frames)?;
                }
                Ok(())
            },
            None,
            true,
            Some(&"0".repeat(64)),
        )
    }

    fn evidence(mix: Mix<'_>, stems: &[Vec<Stereo>; 4]) -> Evidence {
        in_blocks(mix, stems, 10_007, [48_000; 4]).unwrap()
    }

    #[test]
    fn a_held_mix_gives_the_evidence_of_the_whole_mix_and_returns_to_its_caller() {
        let (mix, stems) = material();
        let whole = evidence(Mix::Whole(&mix), &stems);
        let mut held = mix.clone();
        let from_scratch = evidence(Mix::Held(&mut held), &stems);
        assert!(whole.usable && whole.refinement.is_some());
        assert_eq!(
            serde_json::to_value(from_scratch).unwrap(),
            serde_json::to_value(whole).unwrap()
        );
        assert_eq!(fingerprint(&held), fingerprint(&mix));
        assert_eq!(held.len(), mix.len());
    }

    /// A mix in scratch storage gives the evidence of the mix in memory, and
    /// a stored mix with a non-finite sample gives an error (D250).
    #[test]
    fn a_stored_mix_gives_the_evidence_of_the_whole_mix() {
        let (mut mix, stems) = material();
        let whole = serde_json::to_value(evidence(Mix::Whole(&mix), &stems)).unwrap();
        let mut stored = scratch::Pcm::temporary_of(&mix).unwrap();
        let from_storage = evidence(Mix::Stored(&mut stored), &stems);
        assert_eq!(serde_json::to_value(from_storage).unwrap(), whole);
        mix[70_001][0] = f32::NAN;
        let mut stored = scratch::Pcm::temporary_of(&mix).unwrap();
        assert!(in_blocks(Mix::Stored(&mut stored), &stems, 10_007, [48_000; 4]).is_err());
    }

    /// A stem gives one evidence in blocks of any length, and a stem with
    /// another rate, another length or a non-finite sample gives an error
    /// (D245).
    #[test]
    fn stems_in_blocks_of_any_length_give_one_evidence() {
        let (mix, stems) = material();
        // The evidence and its automatic focus, which the evidence does not
        // serialize.
        let json = |evidence: Evidence| {
            let focus = serde_json::to_value(&evidence.auto_focus).unwrap();
            (serde_json::to_value(evidence).unwrap(), focus)
        };
        let whole = json(in_blocks(Mix::Whole(&mix), &stems, usize::MAX, [48_000; 4]).unwrap());
        assert!(whole.0["usable"].as_bool().unwrap() && !whole.1.is_null());
        assert!(!whole.0["refinement"].is_null());
        for block in [1_024, 65_536] {
            let evidence = in_blocks(Mix::Whole(&mix), &stems, block, [48_000; 4]).unwrap();
            assert_eq!(json(evidence), whole, "{block}");
        }
        let changes: [fn(&mut [Vec<Stereo>; 4]); 4] = [
            |stems| stems[2].push([0.0; 2]),
            |stems| stems[1].truncate(stems[1].len() - 1),
            |stems| stems[0].truncate(1),
            |stems| stems[3][12_345][1] = f32::NAN,
        ];
        for change in changes {
            let mut changed = stems.clone();
            change(&mut changed);
            assert!(in_blocks(Mix::Whole(&mix), &changed, 10_007, [48_000; 4]).is_err());
        }
        let rates = [48_000, 48_000, 44_100, 48_000];
        assert!(in_blocks(Mix::Whole(&mix), &stems, 10_007, rates).is_err());
        assert!(in_blocks(Mix::Whole(&mix), &stems, 10_007, [44_000; 4]).is_err());
    }

    #[test]
    fn the_automatic_focus_and_the_reconstruction_use_the_sum_from_scratch() {
        let (mut mix, stems) = material();
        let sum_of = |roles: std::ops::Range<usize>| -> Vec<Stereo> {
            (0..mix.len())
                .map(|i| {
                    [0, 1].map(|c| {
                        roles
                            .clone()
                            .fold(-0.0, |sum, role| sum + stems[role][i][c])
                    })
                })
                .collect()
        };
        let (accompaniment, reconstruction) = (sum_of(0..3), sum_of(0..4));
        let source = fingerprint(&mix);
        let expected = crate::auditory::propose_from_stems(
            &stems[3],
            &accompaniment,
            48_000,
            &"0".repeat(64),
            &source,
            48_000,
        )
        .unwrap();
        let evidence = evidence(Mix::Held(&mut mix), &stems);
        assert!(evidence.usable);
        assert_eq!(
            evidence.reconstruction_pcm_sha256,
            fingerprint(&reconstruction)
        );
        assert_eq!(
            serde_json::to_value(evidence.auto_focus).unwrap(),
            serde_json::to_value(Some(expected)).unwrap()
        );
    }
}

#[cfg(test)]
mod stored_stem_tests {
    use super::*;

    fn stem(frames: usize) -> Vec<Stereo> {
        let mut state = 0x2545_f491u32;
        (0..frames)
            .map(|i| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                let noise = state as f32 / u32::MAX as f32 - 0.5;
                let t = i as f32 / 44_100.0;
                [0.3 * (t * 2_100.0).sin() + 0.05 * noise, 0.2 * noise]
            })
            .collect()
    }

    fn bits(audio: &[Stereo]) -> Vec<u32> {
        audio.iter().flatten().map(|x| x.to_bits()).collect()
    }

    /// A stored stem converts in spans to the frames of `resample::convert`
    /// (D245).
    #[test]
    fn a_stored_stem_converts_in_spans_to_the_frames_of_convert() {
        let audio = stem(3 * resample::CONVERT_SPAN / 2 + 1_234);
        for (from, to) in [(44_100, 48_000), (48_000, 44_100), (96_000, 48_000)] {
            let mut stored = scratch::Pcm::temporary_of(&audio).unwrap();
            let mut at_rate = resample::convert_stored(&mut stored, from, to).unwrap();
            let mut read = Vec::new();
            at_rate.read(0, at_rate.len(), &mut read).unwrap();
            assert_eq!(
                bits(&read),
                bits(&resample::convert(&audio, from, to).unwrap())
            );
        }
    }

    /// The masking frames of a stored stem, converted in spans, equal the
    /// frames of the stem converted as a whole (D245). The stem fills
    /// several spans of eight threads.
    #[test]
    fn the_masking_frames_of_a_stored_stem_equal_those_of_the_whole_stem() {
        use crate::masking;
        let audio = stem(2_200_000);
        let (from, to) = (44_100, 48_000);
        let at_rate = resample::Converted::new(&audio, from, to).unwrap();
        let (frames, mid) = masking::frame_pairs_read_of(
            at_rate.frames(),
            |start, pairs| {
                let mut frames = [[0.0f32; 2]; masking::HOP];
                let frames = &mut frames[..pairs.len()];
                at_rate.fill_from(start, frames);
                for (pair, &frame) in pairs.iter_mut().zip(&*frames) {
                    *pair = [frame, [crate::deesser::mid(frame), 0.0]];
                }
            },
            masking::Channels::Both,
            [masking::Measures::Powers; 2],
            to,
        );
        let mut stored = scratch::Pcm::temporary_of(&audio).unwrap();
        let (read, read_mid) = masking_pairs(&mut stored, from, to).unwrap();
        let powers = |frames: &masking::Frames| -> Vec<u64> {
            frames
                .powers
                .iter()
                .flatten()
                .chain(&frames.flatness_db)
                .map(|x| x.to_bits())
                .collect()
        };
        assert!(frames.powers.len() > crate::stft::PAIR_RANGE * crate::dsp::threads().min(8));
        assert_eq!(powers(&read), powers(&frames));
        assert_eq!(powers(&read_mid), powers(&mid));
    }
}
