//! Measurements of supplied separation estimates. No model runs in the engine
//! and no separated waveform is substituted for the original stereo mix.
use crate::{
    analysis::percentile,
    audio_io::{Result, Stereo},
    foundation::{self, SpatialBands},
    resample,
};
use serde::Serialize;
use sha2::{Digest, Sha256};

const FFT: usize = 8192;
const HOP: usize = 4096;
const BANDS: usize = 29;

#[derive(Clone, Serialize)]
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
#[derive(Clone, Serialize, Debug)]
pub struct StemImage {
    pub role: &'static str,
    pub bands: Vec<StemImageBand>,
}

#[derive(Clone, Serialize, Debug)]
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
    let image = crate::stereo::analyze(audio, rate)?;
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

#[derive(Clone, Serialize)]
pub struct Evidence {
    pub rule: &'static str,
    pub validation_status: &'static str,
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
    #[serde(skip_serializing)]
    pub auto_focus: Option<crate::source_treatments::ProposalSet>,
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
}

pub fn fingerprint(audio: &[Stereo]) -> String {
    // The little-endian samples in 64 KiB pieces: the same bytes as one
    // sample at a time, without a hash call for each sample.
    let mut hash = Sha256::new();
    let mut bytes = Vec::with_capacity(8192 * 8);
    for frames in audio.chunks(8192) {
        bytes.clear();
        for sample in frames.iter().flatten() {
            bytes.extend_from_slice(&sample.to_le_bytes());
        }
        hash.update(&bytes);
    }
    format!("{:x}", hash.finalize())
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
    let bin_bands: Vec<_> = (0..=FFT / 2)
        .map(|bin| {
            let hz = bin as f64 * rate as f64 / FFT as f64;
            let index = (3.0 * (hz / 25.0).log2()).floor();
            (index >= 0.0 && index < BANDS as f64).then_some(index as usize)
        })
        .collect();
    crate::stft::stereo_frames(audio, FFT, HOP, |l, r| {
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
    })
    .into_iter()
    .unzip()
}

/// Drum envelope peaks with a local rise and a measured 6 dB decay. Timing
/// comes from the estimated drum waveform, not sparse onsets in the full mix.
fn drum_decay(audio: &[Stereo], rate: u32) -> (usize, Option<f64>, Option<[f64; 3]>, Option<f64>) {
    let block = rate as usize / 100;
    let levels: Vec<_> = audio
        .chunks_exact(block)
        .map(|frames| {
            let p = frames
                .iter()
                .flatten()
                .map(|&v| (v as f64).powi(2))
                .sum::<f64>()
                / (frames.len() * 2) as f64;
            10.0 * p.max(1e-30).log10()
        })
        .collect();
    let gate = percentile(&levels, 0.95) - 15.0;
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
    let (mut evidence, reconstructed) = finish_evidence(
        input,
        Observations {
            rate: stem_rate,
            frames: stems[0].len(),
            hashes: stems.map(fingerprint),
            powers,
            spatial,
            drum: drum_decay(stems[0], stem_rate),
            images,
        },
        reconstructed,
        rate,
    )?;
    if evidence.usable {
        // The masking frames of each stem serve the low-mid key and the
        // de-esser key; the mid band powers of the drum, bass and other
        // stems serve the de-esser key.
        let mut roles = Vec::with_capacity(4);
        let mut others: [Vec<f64>; 3] = Default::default();
        let mut converted = Vec::new();
        for (role, stem) in stems.iter().enumerate() {
            let at_rate = at_render_rate(stem, stem_rate, rate, &mut converted)?;
            let frames =
                crate::masking::frames(at_rate, crate::masking::Channels::Both, rate).powers;
            if role < 3 {
                others[role] = crate::deesser::sibilant_powers(at_rate, rate);
            } else {
                evidence.deesser_key =
                    crate::deesser::key_with_frames(at_rate, &frames, &others, input, rate);
            }
            roles.push(frames);
        }
        evidence.low_mid = low_mid_key(&evidence, input, &roles, rate);
    }
    Ok((evidence, reconstructed))
}

/// A stem at `stem_rate` at the render rate `rate`. A stem at another rate
/// converts into `converted`: in the browser engine, frames that convert
/// each sample as they read it take more CPU time for the set.
fn at_render_rate<'a>(
    stem: &'a [Stereo],
    stem_rate: u32,
    rate: u32,
    converted: &'a mut Vec<Stereo>,
) -> Result<&'a [Stereo]> {
    if stem_rate == rate {
        return Ok(stem);
    }
    resample::convert_into(stem, stem_rate, rate, converted)?;
    Ok(converted)
}

/// The low-mid key of the lead role against the power sum of the other
/// roles, with the tonality of the mix. `input` and the frames of the roles
/// are at `rate`.
fn low_mid_key(
    evidence: &Evidence,
    input: &[Stereo],
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
    let mix = crate::masking::frames(input, crate::masking::Channels::Both, rate);
    crate::low_mid::key(
        &roles[lead.index],
        &others,
        &mix.flatness_db,
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

/// Measure each raw stem and release it before loading the next role.
/// `input` is at the render rate `rate`.
pub fn analyze_sequential(
    input: &[Stereo],
    rate: u32,
    mut load: impl FnMut(&'static str) -> Result<crate::audio_io::Source>,
    proposals: Option<&crate::source_treatments::ProposalSet>,
    refine: bool,
) -> Result<Evidence> {
    analyze_sequential_inner(input, rate, &mut load, proposals, refine, None)
}

/// Set analysis uses the same local auditory measurement on native and WASM.
/// Imported diagnostics still take precedence when explicitly supplied.
/// `input` is at the render rate `rate`.
pub fn analyze_set(
    input: &[Stereo],
    rate: u32,
    mut load: impl FnMut(&'static str) -> Result<crate::audio_io::Source>,
    proposals: Option<&crate::source_treatments::ProposalSet>,
    source_file_sha256: &str,
) -> Result<Evidence> {
    analyze_sequential_inner(
        input,
        rate,
        &mut load,
        proposals,
        true,
        Some(source_file_sha256),
    )
}

fn analyze_sequential_inner(
    input: &[Stereo],
    rate: u32,
    load: &mut impl FnMut(&'static str) -> Result<crate::audio_io::Source>,
    proposals: Option<&crate::source_treatments::ProposalSet>,
    refine: bool,
    auto_source_hash: Option<&str>,
) -> Result<Evidence> {
    crate::audio_io::check_render_rate(rate)?;
    if input.len() < 3 * rate as usize || input.iter().flatten().any(|x| !x.is_finite()) {
        return Err("Stem evidence requires finite source PCM".into());
    }
    let mut observations = Observations {
        rate: 0,
        frames: 0,
        hashes: std::array::from_fn(|_| String::new()),
        powers: Vec::new(),
        spatial: Vec::new(),
        drum: (0, None, None, None),
        images: Vec::new(),
    };
    let mut sum: Vec<Stereo> = Vec::new();
    let mut vocal = None;
    let mut deesser_key = crate::deesser::Key::default();
    // The mid 5 to 10 kHz power of each frame of the drum, bass and other
    // stems at the render rate, for the de-esser key.
    let mut others: [Vec<f64>; 3] = Default::default();
    let mut role_masking = Vec::with_capacity(4);
    // The stems at the render rate share one buffer: a buffer freed after
    // each stem leaves holes that later allocations split.
    let mut converted: Vec<Stereo> = Vec::new();
    let mut auto_focus = None;
    for (role, name) in ["drums", "bass", "other", "vocals"].into_iter().enumerate() {
        crate::progress::report(crate::progress::Stage {
            stage: "stem",
            part: Some(name),
            index: Some(role + 1),
            count: Some(4),
            ..Default::default()
        });
        let stem = load(name)?;
        if role == 0 {
            observations.rate = stem.sample_rate;
            observations.frames = stem.audio.len();
            let expected =
                (stem.audio.len() as f64 * rate as f64 / stem.sample_rate as f64).round() as usize;
            if !resample::RATES.contains(&stem.sample_rate)
                || stem.audio.is_empty()
                || expected.abs_diff(input.len()) > 1
            {
                return Err("Stem duration or sample rate does not match the original mix".into());
            }
            // The sum converts to the render rate in its own buffer
            // (`resample::convert_owned`).
            sum.reserve_exact(
                resample::converted_frames(stem.audio.len(), stem.sample_rate, rate)
                    .max(stem.audio.len()),
            );
            sum.resize(stem.audio.len(), [-0.0; 2]);
        }
        if stem.sample_rate != observations.rate
            || stem.audio.len() != observations.frames
            || stem.audio.iter().flatten().any(|x| !x.is_finite())
        {
            return Err("Stem evidence requires finite, aligned estimates at one rate".into());
        }
        observations.hashes[role] = fingerprint(&stem.audio);
        let (powers, spatial) = band_frames(&stem.audio, observations.rate);
        observations.powers.push(powers);
        observations.spatial.push(spatial);
        observations
            .images
            .push(stem_image(name, &stem.audio, observations.rate)?);
        if role == 0 {
            observations.drum = drum_decay(&stem.audio, observations.rate);
        }
        // The vocal at the render rate and its masking frames serve the
        // low-mid key, the de-esser key and the refinement.
        let mut vocal_at_rate = None;
        if role == 3 {
            // The vocal takes the buffer of the earlier conversions.
            let resampled = if observations.rate != rate {
                resample::convert_into(&stem.audio, observations.rate, rate, &mut converted)?;
                Some(std::mem::take(&mut converted))
            } else {
                None
            };
            let at_rate = resampled.as_deref().unwrap_or(&stem.audio);
            let frames =
                crate::masking::frames(at_rate, crate::masking::Channels::Both, rate).powers;
            deesser_key = crate::deesser::key_with_frames(at_rate, &frames, &others, input, rate);
            role_masking.push(frames);
            vocal_at_rate = Some(resampled);
        } else {
            let at_rate = at_render_rate(&stem.audio, observations.rate, rate, &mut converted)?;
            role_masking
                .push(crate::masking::frames(at_rate, crate::masking::Channels::Both, rate).powers);
            others[role] = crate::deesser::sibilant_powers(at_rate, rate);
        }
        if role == 3 && proposals.is_none() {
            if let Some(source_hash) = auto_source_hash {
                auto_focus = Some(crate::auditory::propose_from_stems(
                    &stem.audio,
                    &sum,
                    observations.rate,
                    source_hash,
                    &fingerprint(input),
                    rate,
                )?);
            }
        }
        for (total, frame) in sum.iter_mut().zip(&stem.audio) {
            for channel in 0..2 {
                total[channel] += frame[channel];
            }
        }
        if let (Some(resampled), true) = (vocal_at_rate, refine) {
            vocal = Some(resampled.unwrap_or(stem.audio));
        }
    }
    crate::progress::step("stems");
    let reconstructed = resample::convert_owned(sum, observations.rate, rate)?;
    let (mut evidence, reconstructed) = finish_evidence(input, observations, reconstructed, rate)?;
    if evidence.usable {
        evidence.auto_focus = auto_focus;
        evidence.deesser_key = deesser_key;
        evidence.low_mid = low_mid_key(&evidence, input, &role_masking, rate);
    }
    drop(role_masking);
    if refine {
        crate::progress::step("focus");
        evidence.refinement = Some(crate::refinement::analyze_matched(
            input,
            if evidence.usable {
                vocal.unwrap()
            } else {
                Vec::new()
            },
            &evidence,
            proposals.or(evidence.auto_focus.as_ref()),
            reconstructed,
            rate,
        )?);
    }
    Ok(evidence)
}

/// The evidence of `observations` against `input`; `input` and
/// `reconstructed` are at the render rate `render_rate`.
fn finish_evidence(
    input: &[Stereo],
    observations: Observations,
    reconstructed: Vec<Stereo>,
    render_rate: u32,
) -> Result<(Evidence, Vec<Stereo>)> {
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
    for (a, b) in input.iter().zip(&reconstructed) {
        for c in 0..2 {
            let (x, y) = (a[c] as f64, b[c] as f64);
            xx += x * x;
            yy += y * y;
            xy += x * y;
            residual += (x - y).powi(2);
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
        foundation::propose(&spatial, input, &reconstructed, rate, render_rate)?
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
    Ok((
        Evidence {
            rule: "stem-evidence.2",
            validation_status: "engineering-hypothesis",
            roles: ["drums", "bass", "other", "vocals"],
            source_pcm_sha256: fingerprint(input),
            source_frames: input.len(),
            stem_sample_rate: rate,
            stem_frames: frames,
            stem_pcm_sha256: hashes,
            reconstruction_pcm_sha256: fingerprint(&reconstructed),
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
        },
        reconstructed,
    ))
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
