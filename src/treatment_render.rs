//! Complete treatment rendering shared by the automatic sets and the presets.
use crate::{
    audio_io::{Result, Stereo},
    compressor, dsp, finishing, render_blocks, search, treatment,
};
use serde::Serialize;
use serde_json::json;
use std::{cell::RefCell, rc::Rc, sync::Arc};

#[derive(Clone, Serialize)]
pub struct Spec {
    /// Phase alignment of the channels before every other stage
    /// (`set.phase-alignment.v1`). `None` keeps the source phase.
    pub phase_alignment: Option<crate::stereo_phase::Alignment>,
    pub finishing: finishing::Plan,
    pub tone: treatment::Tone,
    pub focus_envelopes: Vec<treatment::FocusEnvelope>,
    /// Second-order Butterworth high-pass on the side channel after tone.
    /// `None` keeps the source low-end image.
    pub side_highpass_hz: Option<f64>,
    /// Zero-phase side gain per band after tone (`set.widening.v6`). `None`
    /// keeps the side level.
    pub side_gain: Option<crate::width::SideGain>,
    /// Keyed cut of the mid channel's sibilant band after tone and width
    /// (`set.deesser.v4`).
    pub deesser: Option<crate::deesser::Plan>,
    /// Low-mid correction of the mid channel keyed by the stems, after the
    /// tone (`set.low-mid.v1`). Each version that carries it uses the same
    /// correction.
    pub low_mid: Option<treatment::EnvelopedBand>,
    /// The hold of the bass of the version at the level of Natural in the
    /// sections where the version falls below it (`set.low-end.v3`).
    pub low_end_hold: Option<crate::low_end::Hold>,
    /// Side cuts keyed to the passages whose channels cancel, one per
    /// anti-phase band of an instrument group (`width::mono_safe`,
    /// `width::MONO_SAFE_RULE`). Each is rendered as the zero-phase side filter
    /// of the widening (`width::SideGain`) with the band's gain, blended by
    /// its envelope after the side gain (`apply_mono_safe`); the bell gives
    /// the band and the gain. Empty for a version that keeps the image of
    /// the mix.
    #[serde(default)]
    pub mono_safe: Vec<treatment::EnvelopedBand>,
    pub dynamic_after_eq: Vec<treatment::DynamicProbe>,
    pub compressor: Option<compressor::Settings>,
    pub limiter: dsp::LimiterSettings,
    /// Ceiling-relative soft-clip budget in dB. The limiter bounds the signal
    /// this far above the ceiling; the clipper maps the remainder onto it.
    pub clipper_db: f64,
    pub ceiling_dbtp: f64,
    /// Correction of a true peak above the ceiling after the decimation.
    pub true_peak: dsp::TruePeak,
    pub initial_drive_db: f64,
    pub target_lufs: f64,
    pub fixed_drive_db: Option<f64>,
    /// Measure the tonal change that the compressor and peak control cause
    /// inside each restoration window at matched K-weighted level, and render
    /// again with broad filters that return the balance before the
    /// compressor, in up to `RESTORATION_PASSES` passes. See
    /// `set.peak-tone-restoration.v4`.
    pub restore_peak_tone: bool,
    /// Gains of the restoring filters of each `stereo::BAND_EDGES_HZ` band,
    /// applied after the restoration measures the prefix and before the
    /// compressor. The restoration starts from these gains.
    pub restoration_db: [f64; 6],
}

impl Spec {
    /// The tone with the low-mid correction on the mid channel and the
    /// low-end hold on the stereo mix, and their envelopes.
    pub fn effective_tone(&self) -> (treatment::Tone, Vec<treatment::FocusEnvelope>) {
        let low_mid = self.low_mid.as_ref().map(|c| {
            self.low_end_hold
                .as_ref()
                .map_or(c.clone(), |h| h.scaled(c))
        });
        let hold = self.low_end_hold.iter().flat_map(|h| &h.bands);
        let bands: Vec<&treatment::EnvelopedBand> = low_mid.iter().chain(hold).collect();
        treatment::with_enveloped(&self.tone, &self.focus_envelopes, &bands)
    }

    /// The effective tone with each enveloped filter at its mean wet share
    /// over the recording, as a gain: the static tone that predictions from
    /// a long-term spectrum use.
    pub fn mean_tone(&self) -> treatment::Tone {
        let (mut tone, envelopes) = self.effective_tone();
        for envelope in &envelopes {
            let points = &envelope.points;
            let span =
                points.last().map_or(0, |p| p.sample) - points.first().map_or(0, |p| p.sample);
            // A raised cosine between two points has the mean of the two.
            let mean = if span == 0 {
                points.first().map_or(1.0, |p| p.wet)
            } else {
                points
                    .windows(2)
                    .map(|w| 0.5 * (w[0].wet + w[1].wet) * (w[1].sample - w[0].sample) as f64)
                    .sum::<f64>()
                    / span as f64
            };
            let bands = match envelope.domain {
                finishing::Domain::Stereo => &mut tone.stereo,
                finishing::Domain::Mid => &mut tone.mid,
                finishing::Domain::Side => &mut tone.side,
            };
            if let Some(band) = bands.get_mut(envelope.filter_index) {
                band.gain_db *= mean;
            }
        }
        tone
    }
}

#[derive(Clone, Serialize)]
pub struct DynamicReport {
    pub probe: treatment::DynamicProbe,
    pub measured_band: finishing::Band,
    pub stats: finishing::BandStats,
}

#[derive(Clone, Serialize)]
pub struct Rendered {
    /// The rendered audio, shared between the threads of a set
    /// (`set::render`).
    #[serde(skip)]
    pub audio: Arc<Vec<Stereo>>,
    pub lufs: f64,
    pub peak_estimate_dbtp: f64,
    /// The true-peak correction after the decimation.
    pub true_peak: render_blocks::TruePeakFit,
    pub drive_db: f64,
    pub limiter: dsp::LimiterStats,
    pub limiter_trace: render_blocks::LimiterTrace,
    pub clipping: Option<finishing::PeakStats>,
    pub finishing: Option<finishing::Stats>,
    pub dynamic_after_eq: Vec<DynamicReport>,
    pub compressor: Option<compressor::Stats>,
    pub deesser: Option<crate::deesser::Stats>,
    pub color: Option<finishing::PeakStats>,
    pub search: Option<search::Report>,
    pub restoration: Option<Restoration>,
    /// Integrated loudness of the prefix the render was made from.
    #[serde(default)]
    pub prefix_lufs: Option<f64>,
    /// P90 loss of onset energy of the render against the prefix before the
    /// compressor (`analysis::energy_attack_loss_p90_from`, D126), in dB: the
    /// loss that the compressor and peak control cause. The loss against the
    /// mix (`analysis::compare`) also counts the tone of the version (D164).
    pub energy_attack_loss_p90_db: Option<f64>,
}

/// A render meets its loudness target within this distance, in LU.
pub const LOUDNESS_TOLERANCE_LU: f64 = 0.03;

/// Largest restoring filter gain, in dB.
pub const RESTORATION_MAXIMUM_DB: f64 = 1.5;
/// Restoring renders after the first render. One pass returns about half
/// of the measured change: the limiter of the second render acts on the
/// restored tone.
pub const RESTORATION_PASSES: usize = 2;
/// Each measured band change shrinks by this much before it is restored, in
/// dB. The correction is continuous in the change: a change inside the dead
/// zone stays, and a larger change keeps this residual. A hard threshold
/// gives two versions with nearly equal changes different corrections.
pub const RESTORATION_DEAD_ZONE_DB: f64 = 0.1;
/// Restoring filters under this gain are dropped, in dB.
const RESTORATION_FILTER_FLOOR_DB: f64 = 0.05;
/// Smallest share of the prefix's K-weighted power for a band to receive a
/// restoring filter. The K-weighting matches the level reference of the
/// measured change, so a band counts by its share of loudness, not of raw
/// power, which the bass dominates.
const RESTORATION_MATERIAL_FRACTION: f64 = 0.01;

/// Samples in one restoration window: four 4096-point blocks, 0.34 s at
/// 48 kHz and 0.37 s at 44.1 kHz. A window is short enough to hold one
/// passage, so a level change between passages does not enter the measured
/// tone change.
pub const RESTORATION_WINDOW: usize = 4 * 4096;
/// Windows more than 40 dB under the loudest window carry no weight.
const RESTORATION_QUIET_FRACTION: f64 = 1e-4;

/// Tonal change from the prefix before the compressor to the delivered output
/// inside each restoration window, at the window's matched K-weighted level,
/// per `stereo::BAND_EDGES_HZ` band, and the filters that return it.
#[derive(Clone, Serialize)]
pub struct Restoration {
    pub rule: &'static str,
    pub window_samples: usize,
    pub material: [bool; 6],
    /// Change from the prefix before the compressor to the first render, in
    /// dB: the change of the compressor and of peak control that the
    /// starting gains (`Spec::restoration_db`) leave.
    pub measured_change_db: [f64; 6],
    pub filters: Vec<dsp::EqBand>,
    /// Change from the prefix to the delivered output after restoration.
    pub delivered_change_db: [f64; 6],
    /// Renders of the version: the first and one for each pass.
    pub renders: usize,
    /// Gains of the restoring filters of the delivered render, per band.
    pub gains_db: [f64; 6],
}

/// Power per `stereo::BAND_EDGES_HZ` band and K-weighted energy in each
/// complete restoration window, both channels.
#[derive(Clone, Default)]
pub struct WindowBands {
    pub bands: Vec<[f64; 6]>,
    /// K-weighted power of each window, from the same windowed blocks as the
    /// band powers.
    pub weighted: Vec<f64>,
    /// K-weighted power of each band over the complete windows.
    pub weighted_bands: [f64; 6],
}

impl WindowBands {
    /// The window bands of `audio` at `rate`.
    pub fn measure(audio: &[Stereo], rate: u32) -> Self {
        const FFT: usize = RESTORATION_WINDOW / 4;
        let edges = crate::stereo::BAND_EDGES_HZ;
        let bins: Vec<(Option<usize>, f64)> = (0..=FFT / 2)
            .map(|bin| {
                let hz = bin as f64 * rate as f64 / FFT as f64;
                (
                    edges.windows(2).position(|e| hz >= e[0] && hz < e[1]),
                    dsp::k_weighting_power(hz, rate),
                )
            })
            .collect();
        let mut stft = crate::stft::StereoStft::new(FFT, FFT);
        let windows = audio.len() / RESTORATION_WINDOW;
        let mut bands = vec![[0.0; 6]; windows];
        let mut weighted = vec![0.0; windows];
        let mut weighted_bands = [0.0; 6];
        for (index, block) in audio
            .as_chunks::<FFT>()
            .0
            .iter()
            .take(windows * 4)
            .enumerate()
        {
            if let Some((l, r)) = stft.add_with(FFT, |i| block[i]) {
                let (power, level) = (&mut bands[index / 4], &mut weighted[index / 4]);
                for (bin, (band, weight)) in bins.iter().enumerate() {
                    let p = l[bin].norm_sqr() + r[bin].norm_sqr();
                    *level += p * weight;
                    if let Some(i) = band {
                        power[*i] += p;
                        weighted_bands[*i] += p * weight;
                    }
                }
            }
        }
        Self {
            bands,
            weighted,
            weighted_bands,
        }
    }

    /// Band power summed over the windows.
    pub fn total(&self) -> [f64; 6] {
        std::array::from_fn(|i| self.bands.iter().map(|b| b[i]).sum())
    }

    /// Bands that hold at least `RESTORATION_MATERIAL_FRACTION` of the
    /// K-weighted power. Shares are compared on a 1e-6 grid, like the
    /// changes in `local_change`.
    pub fn material(&self) -> [bool; 6] {
        let total: f64 = self.weighted_bands.iter().sum();
        std::array::from_fn(|i| {
            total > 0.0
                && quantize(self.weighted_bands[i] / total, 1e6)
                    >= quantize(RESTORATION_MATERIAL_FRACTION, 1e6)
        })
    }
}

/// `value` rounded to a grid of `1 / scale`. FFT rounding differs between
/// native and WASM builds; decisions on a coarse grid keep both renders
/// identical.
pub(crate) fn quantize(value: f64, scale: f64) -> f64 {
    (value * scale).round() / scale
}

/// Band change from `before` to `after` inside each window with the window's
/// K-weighted level change removed. The mean weights each window by the
/// band's power before the change, so a band counts where it is present.
/// Changes are rounded to 0.0001 dB.
pub fn local_change(before: &WindowBands, after: &WindowBands) -> [f64; 6] {
    let windows = before
        .bands
        .len()
        .min(after.bands.len())
        .min(before.weighted.len())
        .min(after.weighted.len());
    let loudest = before.weighted[..windows]
        .iter()
        .copied()
        .fold(0.0, f64::max);
    let mut sums = [(0.0f64, 0.0f64); 6];
    for w in 0..windows {
        let (level_in, level_out) = (before.weighted[w], after.weighted[w]);
        if level_in <= loudest * RESTORATION_QUIET_FRACTION || level_out <= 0.0 {
            continue;
        }
        let level = 10.0 * (level_out / level_in).log10();
        for (i, sum) in sums.iter_mut().enumerate() {
            let (power_in, power_out) = (before.bands[w][i], after.bands[w][i]);
            if power_in > 0.0 && power_out > 0.0 {
                sum.0 += power_in * (10.0 * (power_out / power_in).log10() - level);
                sum.1 += power_in;
            }
        }
    }
    std::array::from_fn(|i| {
        if sums[i].1 > 0.0 {
            quantize(sums[i].0 / sums[i].1, 1e4)
        } else {
            0.0
        }
    })
}

/// One broad filter per band: shelves at the outer edges, bells at the
/// geometric centres with a bandwidth near the band's width.
fn restoring_filter(band: usize, gain_db: f64) -> dsp::EqBand {
    let (kind, hz, q) = match band {
        0 => (dsp::Kind::LowShelf, 120.0, std::f64::consts::FRAC_1_SQRT_2),
        1 => (dsp::Kind::Peak, 245.0, 0.67),
        2 => (dsp::Kind::Peak, 1000.0, 0.67),
        3 => (dsp::Kind::Peak, 3464.0, 0.9),
        4 => (dsp::Kind::Peak, 8485.0, 1.41),
        _ => (
            dsp::Kind::HighShelf,
            12000.0,
            std::f64::consts::FRAC_1_SQRT_2,
        ),
    };
    dsp::EqBand {
        kind,
        hz,
        q,
        gain_db,
    }
}

/// The response that restores one measured band change: the change shrunk
/// toward zero by the dead zone, with the opposite sign.
pub fn restoration_target(change: f64) -> f64 {
    -change.signum() * (change.abs() - RESTORATION_DEAD_ZONE_DB).max(0.0)
}

/// Filters at `rate` whose mean log-frequency response in each material
/// band equals `restoration_target` of its change. Bands inside the dead
/// zone keep a zero target, so the neighbouring filters do not move them.
/// Gains are bounded.
pub fn restoring_filters(change: &[f64; 6], material: &[bool; 6], rate: u32) -> Vec<dsp::EqBand> {
    filters_of(&restoring_gains(change, material, rate))
}

/// The restoring filter of each band with a gain of at least
/// `RESTORATION_FILTER_FLOOR_DB`.
pub fn filters_of(gains: &[f64; 6]) -> Vec<dsp::EqBand> {
    (0..6)
        .filter(|&band| gains[band].abs() >= RESTORATION_FILTER_FLOOR_DB)
        .map(|band| restoring_filter(band, gains[band]))
        .collect()
}

/// Gains of the restoration pass after `r`, a restoration of a render at
/// `rate`: the gains of `r` with those that return the change it leaves.
/// None when no band moves by `RESTORATION_FILTER_FLOOR_DB` or more.
pub fn next_gains(r: &Restoration, rate: u32) -> Option<[f64; 6]> {
    let step = restoring_gains(&r.delivered_change_db, &r.material, rate);
    let total: [f64; 6] = std::array::from_fn(|band| {
        (r.gains_db[band] + step[band]).clamp(-RESTORATION_MAXIMUM_DB, RESTORATION_MAXIMUM_DB)
    });
    (0..6)
        .any(|band| (total[band] - r.gains_db[band]).abs() >= RESTORATION_FILTER_FLOOR_DB)
        .then_some(total)
}

/// Gain of the restoring filter of each band (`restoring_filters`) at
/// `rate`; 0 for a band that is not material.
fn restoring_gains(change: &[f64; 6], material: &[bool; 6], rate: u32) -> [f64; 6] {
    let edges = crate::stereo::BAND_EDGES_HZ;
    let used: Vec<usize> = (0..6).filter(|&i| material[i]).collect();
    let mut gains = [0.0f64; 6];
    if used.iter().all(|&i| restoration_target(change[i]) == 0.0) {
        return gains;
    }
    let mean_response = |filter: &dsp::EqBand, band: usize| -> f64 {
        let (low, high) = (edges[band].log2(), edges[band + 1].min(18000.0).log2());
        let biquad = filter.filter(rate);
        (0..32)
            .map(|k| {
                biquad.response_db(
                    2f64.powf(low + (high - low) * (k as f64 + 0.5) / 32.0),
                    rate,
                )
            })
            .sum::<f64>()
            / 32.0
    };
    // Responses per dB of gain are nearly linear at these gains; solve the
    // square system over the material bands by Gaussian elimination.
    let n = used.len();
    let mut a: Vec<Vec<f64>> = used
        .iter()
        .map(|&band| {
            let mut row: Vec<f64> = used
                .iter()
                .map(|&f| mean_response(&restoring_filter(f, 1.0), band))
                .collect();
            row.push(restoration_target(change[band]));
            row
        })
        .collect();
    for col in 0..n {
        let pivot = (col..n)
            .max_by(|&x, &y| a[x][col].abs().total_cmp(&a[y][col].abs()))
            .unwrap();
        a.swap(col, pivot);
        if a[col][col].abs() < 1e-9 {
            return gains;
        }
        let pivot_row = a[col].clone();
        for (row, values) in a.iter_mut().enumerate() {
            if row != col {
                let factor = values[col] / pivot_row[col];
                for (value, p) in values.iter_mut().zip(&pivot_row).skip(col) {
                    *value -= factor * p;
                }
            }
        }
    }
    for (k, &band) in used.iter().enumerate() {
        gains[band] = (a[k][n] / a[k][k]).clamp(-RESTORATION_MAXIMUM_DB, RESTORATION_MAXIMUM_DB);
    }
    gains
}

#[derive(Default, Clone, Serialize)]
pub struct Work {
    pub prepared_prefixes: usize,
    pub reused_prefixes: usize,
    pub rendered_probes: usize,
    pub reused_probes: usize,
    pub probe_cache_budget_bytes: usize,
}

impl Work {
    /// Add the counts of `other`, the work of another renderer of the same
    /// set.
    pub fn add(&mut self, other: &Work) {
        self.prepared_prefixes += other.prepared_prefixes;
        self.reused_prefixes += other.reused_prefixes;
        self.rendered_probes += other.rendered_probes;
        self.reused_probes += other.reused_probes;
    }
}

struct Prepared {
    key: String,
    up: crate::scratch::Pcm,
    /// The processed prefix at the base rate (`Renderer::prefix`), for the
    /// audibility check of a candidate (`selection::ladder`).
    shaped: crate::scratch::Pcm,
    finishing: Option<finishing::Stats>,
    dynamic_after_eq: Vec<DynamicReport>,
    compressor: Option<compressor::Stats>,
    deesser: Option<crate::deesser::Stats>,
    color: Option<finishing::PeakStats>,
    /// Window band powers and loudness of the processed prefix before drive.
    windows: WindowBands,
    lufs: Option<f64>,
    /// Onsets and blocks of the prefix before the compressor.
    tone_temporal: Option<crate::analysis::Temporal>,
    probes: Vec<([u64; 9], Stored)>,
    probe_bytes: usize,
}

/// The key of the prepared prefix of `spec`: each field of the spec that
/// changes the input of the drive.
fn prefix_key(spec: &Spec) -> String {
    json!({"phase_alignment":spec.phase_alignment,"finishing":spec.finishing,"tone":spec.tone,"focus_envelopes":spec.focus_envelopes,
        "side_highpass_hz":spec.side_highpass_hz,"side_gain":spec.side_gain,"deesser":spec.deesser,
        "low_mid":spec.low_mid,"low_end_hold":spec.low_end_hold,"mono_safe":spec.mono_safe,"restoration_db":spec.restoration_db,
        "dynamic_after_eq":spec.dynamic_after_eq,"compressor":spec.compressor})
    .to_string()
}

/// The processed prefix at the base rate, and the reports of its stages.
struct Shaped {
    audio: Vec<Stereo>,
    finishing: Option<finishing::Stats>,
    dynamic_after_eq: Vec<DynamicReport>,
    compressor: Option<compressor::Stats>,
    deesser: Option<crate::deesser::Stats>,
    windows: WindowBands,
    /// Onsets and blocks of the whole prefix before the compressor
    /// (`analysis::analyze`); none for a span.
    tone_temporal: Option<crate::analysis::Temporal>,
}

#[derive(Clone)]
struct Stored {
    report: Rendered,
    pcm: Rc<RefCell<crate::scratch::Pcm>>,
}
impl Stored {
    fn load(self) -> Result<Rendered> {
        use crate::scratch::Frames;
        let mut pcm = self.pcm.borrow_mut();
        let frames = pcm.len();
        let mut audio = Vec::with_capacity(frames);
        let mut block = Vec::new();
        for start in (0..frames).step_by(8192) {
            pcm.read(start, 8192.min(frames - start), &mut block)?;
            audio.extend_from_slice(&block);
        }
        Ok(Rendered {
            audio: Arc::new(audio),
            ..self.report
        })
    }
}

/// One immutable source, one prepared prefix, and bounded shared probe storage.
/// Completed versions are delivered by the caller; they do not accumulate here.
pub struct Renderer<'a> {
    input: &'a [Stereo],
    /// The render rate of `input`, in Hz.
    rate: u32,
    /// Prefix loudness of the reference whose drive the specs start from.
    /// The first search of a render moves its initial drive by the prefix
    /// loudness difference, so a candidate with a louder tone starts lower.
    pub reference_prefix_lufs: Option<f64>,
    /// Most recently used last. Two prefixes let glue and plain candidates
    /// alternate without re-preparing.
    prepared: Vec<Prepared>,
    scratch: crate::scratch::Factory,
    pub work: Work,
}

const PREFIX_SLOTS: usize = 2;

pub fn scale(audio: &mut [Stereo], db: f64) {
    let gain = dsp::gain(db) as f32;
    for frame in audio {
        for sample in frame {
            *sample *= gain;
        }
    }
}

/// The prefix of `spec` on `shaped`, the span at `rate` of a recording of
/// `frames` frames that starts at frame `offset`. A span other than the
/// whole recording starts on `span_grid` (`crate::excerpt`).
fn shape_span(
    mut shaped: Vec<Stereo>,
    spec: &Spec,
    tone: &treatment::Tone,
    focus_envelopes: &[treatment::FocusEnvelope],
    offset: usize,
    frames: usize,
    rate: u32,
) -> Result<Shaped> {
    if !offset.is_multiple_of(span_grid(rate)) {
        return Err("A span of a recording starts on the span grid".into());
    }
    if let Some(alignment) = &spec.phase_alignment {
        alignment.apply(&mut shaped, rate);
    }
    let finishing = if spec.finishing.active {
        Some(finishing::process(&mut shaped, &spec.finishing, rate)?)
    } else {
        None
    };
    tone.apply_with_envelopes_from(&mut shaped, focus_envelopes, offset, frames, rate)?;
    if let Some(gain) = &spec.side_gain {
        gain.apply(&mut shaped, rate);
    }
    apply_mono_safe(&mut shaped, &spec.mono_safe, offset, rate);
    let deesser = spec
        .deesser
        .as_ref()
        .map(|p| p.apply_from(&mut shaped, offset, rate));
    if let Some(hz) = spec.side_highpass_hz {
        dsp::side_eq(
            &mut shaped,
            &[dsp::EqBand {
                kind: dsp::Kind::HighPass,
                hz,
                q: std::f64::consts::FRAC_1_SQRT_2,
                gain_db: 0.0,
            }],
            rate,
        );
    }
    let mut dynamic_after_eq = Vec::new();
    for probe in &spec.dynamic_after_eq {
        // The dynamic bands measure the whole recording.
        if shaped.len() != frames {
            return Err("A span render has no dynamic bands".into());
        }
        let measured_band = treatment::measure_dynamic(&shaped, *probe, rate)?;
        let stats = finishing::dynamic_band(&mut shaped, &measured_band, rate)?;
        dynamic_after_eq.push(DynamicReport {
            probe: *probe,
            measured_band,
            stats,
        });
    }
    let windows = WindowBands::measure(&shaped, rate);
    dsp::eq(&mut shaped, &filters_of(&spec.restoration_db), rate);
    // The reference of the onset-energy loss of a render: the tone of the
    // version without its dynamics stages, so that a tone that raises the
    // sustain of an onset does not count as attack loss (D164).
    let tone_temporal = (shaped.len() == frames).then(|| crate::analysis::analyze(&shaped, rate));
    let compressor = spec
        .compressor
        .map(|s| compressor::process(&mut shaped, s, rate))
        .transpose()?;
    Ok(Shaped {
        audio: shaped,
        finishing,
        dynamic_after_eq,
        compressor,
        deesser,
        windows,
        tone_temporal,
    })
}

/// The keyed side cuts of `width::MONO_SAFE_RULE` on `audio`, a span of the
/// recording that starts at frame `offset`: for each cut, the zero-phase
/// side filter of the widening at the cut's gain in the cut's band, blended
/// into the audio by the wet share of its envelope at each frame. The filter
/// has third-octave band edges, so a cut in one band moves the band above
/// by little. The skirt of a bell moves that band by about 1 dB.
pub fn apply_mono_safe(
    audio: &mut [Stereo],
    cuts: &[treatment::EnvelopedBand],
    offset: usize,
    rate: u32,
) {
    let edges = &crate::stereo::BAND_EDGES_HZ;
    for cut in cuts {
        let Some(band) = (0..6).find(|b| cut.band.hz >= edges[*b] && cut.band.hz < edges[b + 1])
        else {
            continue;
        };
        let mut gains = [0.0; 6];
        gains[band] = cut.band.gain_db;
        let mut filtered = audio.to_vec();
        crate::width::SideGain::gains(gains).apply(&mut filtered, rate);
        for (i, (frame, wet_frame)) in audio.iter_mut().zip(&filtered).enumerate() {
            let wet = cut.wet_at(offset + i) as f32;
            if wet > 0.0 {
                for c in 0..2 {
                    frame[c] += wet * (wet_frame[c] - frame[c]);
                }
            }
        }
    }
}

/// Grid of the start of a span render at `rate`, in frames: one compressor
/// section block (10 ms), so that the section levels meet those of the
/// whole recording. The de-esser places its masking frames at their
/// positions in the recording (`deesser::Plan::apply_from`).
pub fn span_grid(rate: u32) -> usize {
    compressor::section_block(rate)
}

/// The prefix of `spec` on frames `offset..offset + span.len()` of the
/// recording `frames` frames long, from `span`, the source over those
/// frames. The section level of the compressor and the de-esser frames
/// need context on each side of the frames that a caller keeps
/// (`crate::excerpt`).
pub fn prefix_span(
    span: &[Stereo],
    spec: &Spec,
    offset: usize,
    frames: usize,
    rate: u32,
) -> Result<Vec<Stereo>> {
    let (tone, focus_envelopes) = spec.effective_tone();
    tone.validate(rate)?;
    for envelope in &focus_envelopes {
        envelope.validate(&tone, frames)?;
    }
    Ok(shape_span(
        span.to_vec(),
        spec,
        &tone,
        &focus_envelopes,
        offset,
        frames,
        rate,
    )?
    .audio)
}

impl<'a> Renderer<'a> {
    /// A renderer of `input`, at the render rate `rate`.
    pub fn new(input: &'a [Stereo], rate: u32) -> Self {
        Self::with_scratch(input, rate, Box::new(crate::scratch::temporary))
    }

    pub fn with_scratch(input: &'a [Stereo], rate: u32, scratch: crate::scratch::Factory) -> Self {
        Self {
            input,
            rate,
            reference_prefix_lufs: None,
            prepared: Vec::new(),
            scratch,
            work: Work {
                probe_cache_budget_bytes: 128 * 1024 * 1024,
                ..Work::default()
            },
        }
    }

    /// Useful for identity checks and environments with tighter memory budgets.
    pub fn without_probe_cache(input: &'a [Stereo], rate: u32) -> Self {
        let mut renderer = Self::new(input, rate);
        renderer.work.probe_cache_budget_bytes = 0;
        renderer
    }

    /// The render rate of the input, in Hz.
    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Render a spec. With `restore_peak_tone`, render once, measure the
    /// band change from the prefix to the output inside each restoration
    /// window at matched level, and render again with the restoring filters
    /// in up to `RESTORATION_PASSES` passes.
    pub fn render(&mut self, spec: &Spec) -> Result<Rendered> {
        self.render_passes(spec, RESTORATION_PASSES)
    }

    /// `render` with at most `passes` restoration passes. With no pass, the
    /// render reports the change that it leaves (`next_gains`).
    pub fn render_passes(&mut self, spec: &Spec, passes: usize) -> Result<Rendered> {
        let first = self.render_once(spec)?;
        self.restore_latest(spec, first, passes)
    }

    /// Continue the restoration of `first`, a render of `spec`, by at most
    /// `passes` passes.
    pub fn restore(&mut self, spec: &Spec, first: Rendered, passes: usize) -> Result<Rendered> {
        self.prepare(spec)?;
        self.restore_latest(spec, first, passes)
    }

    /// `restore` of a render of the most recent prefix.
    fn restore_latest(&mut self, spec: &Spec, first: Rendered, passes: usize) -> Result<Rendered> {
        if !spec.restore_peak_tone {
            return Ok(first);
        }
        let prefix = self.prepared.last().expect("prepared prefix");
        let Some(prefix_lufs) = prefix.lufs else {
            return Ok(first);
        };
        let reference = prefix.windows.clone();
        let mut restoration = match &first.restoration {
            Some(r) => r.clone(),
            None => {
                let change =
                    local_change(&reference, &WindowBands::measure(&first.audio, self.rate));
                Restoration {
                    rule: "set.peak-tone-restoration.v4",
                    window_samples: RESTORATION_WINDOW,
                    material: reference.material(),
                    measured_change_db: change,
                    filters: filters_of(&spec.restoration_db),
                    delivered_change_db: change,
                    renders: 1,
                    gains_db: spec.restoration_db,
                }
            }
        };
        // Each pass adds the filters that return the change left by the
        // render before it, at the drive of that render corrected by the
        // loudness that the filters add to the prefix.
        let (mut current, mut current_lufs) = (first, prefix_lufs);
        for _ in 0..passes {
            let Some(total) = next_gains(&restoration, self.rate) else {
                break;
            };
            let mut restored = spec.clone();
            restored.restoration_db = total;
            restored.initial_drive_db = current.drive_db;
            self.prepare(&restored)?;
            let lufs = self
                .prepared
                .last()
                .and_then(|p| p.lufs)
                .unwrap_or(current_lufs);
            restored.initial_drive_db -= lufs - current_lufs;
            drop(current);
            current = self.search(&restored)?;
            current_lufs = lufs;
            restoration.filters = filters_of(&total);
            restoration.gains_db = total;
            restoration.renders += 1;
            restoration.delivered_change_db =
                local_change(&reference, &WindowBands::measure(&current.audio, self.rate));
        }
        Ok(Rendered {
            restoration: Some(restoration),
            ..current
        })
    }

    fn render_once(&mut self, spec: &Spec) -> Result<Rendered> {
        self.prepare(spec)?;
        let prefix = self.prepared.last().and_then(|p| p.lufs);
        match (self.reference_prefix_lufs, prefix) {
            (Some(reference), Some(prefix)) => {
                let mut corrected = spec.clone();
                corrected.initial_drive_db -= prefix - reference;
                self.search(&corrected)
            }
            _ => self.search(spec),
        }
    }

    /// The tone and focus envelopes of `spec`, after its checks.
    fn validated(&self, spec: &Spec) -> Result<(treatment::Tone, Vec<treatment::FocusEnvelope>)> {
        let (tone, focus_envelopes) = spec.effective_tone();
        crate::audio_io::check_render_rate(self.rate)?;
        tone.validate(self.rate)?;
        for envelope in &focus_envelopes {
            envelope.validate(&tone, self.input.len())?;
        }
        if self.input.len() < 3 * self.rate as usize
            || self.input.iter().flatten().any(|x| !x.is_finite())
            || spec.limiter.validate().is_err()
            || !(0.0..=6.0).contains(&spec.clipper_db)
            || !(-3.0..=-0.1).contains(&spec.ceiling_dbtp)
            || !spec.initial_drive_db.is_finite()
            || !(-60.0..=-3.0).contains(&spec.target_lufs)
            || spec.fixed_drive_db.is_some_and(|d| !d.is_finite())
            || spec.dynamic_after_eq.len() > 4
            || spec
                .side_highpass_hz
                .is_some_and(|hz| !(20.0..=300.0).contains(&hz))
        {
            return Err("Invalid complete treatment render input or settings".into());
        }
        if let Some(settings) = spec.compressor {
            settings.validate()?;
        }
        if let Some(gain) = &spec.side_gain {
            gain.validate()?;
        }
        if let Some(alignment) = &spec.phase_alignment {
            alignment.validate()?;
        }
        if let Some(plan) = &spec.deesser {
            plan.validate()?;
        }
        if let Some(hold) = &spec.low_end_hold {
            hold.validate()?;
        }
        if spec.mono_safe.iter().any(|b| {
            !matches!(b.domain, finishing::Domain::Side)
                || !matches!(b.band.kind, crate::dsp::Kind::Peak)
                || !(-crate::width::WIDEN_MAXIMUM_DB..=0.0).contains(&b.band.gain_db)
        }) {
            return Err("Invalid mono-safe bells".into());
        }
        Ok((tone, focus_envelopes))
    }

    /// The processed prefix of `spec` at the base rate: the input after
    /// each stage before the drive. The prefix of a delivered render is
    /// the prefix of its spec with the restoring gains of its restoration.
    pub fn prefix(&self, spec: &Spec) -> Result<Vec<Stereo>> {
        let (tone, focus_envelopes) = self.validated(spec)?;
        Ok(self.shape(spec, &tone, &focus_envelopes)?.audio)
    }

    fn shape(
        &self,
        spec: &Spec,
        tone: &treatment::Tone,
        focus_envelopes: &[treatment::FocusEnvelope],
    ) -> Result<Shaped> {
        shape_span(
            self.input.to_vec(),
            spec,
            tone,
            focus_envelopes,
            0,
            self.input.len(),
            self.rate,
        )
    }

    /// `prefix`, read from the prepared prefix of `spec` when the renderer
    /// holds one.
    pub fn prepared_prefix(&mut self, spec: &Spec) -> Result<Vec<Stereo>> {
        use crate::scratch::Frames;
        let key = prefix_key(spec);
        match self.prepared.iter_mut().find(|p| p.key == key) {
            Some(prepared) => {
                let mut audio = Vec::new();
                let frames = prepared.shaped.len();
                prepared.shaped.read(0, frames, &mut audio)?;
                Ok(audio)
            }
            None => self.prefix(spec),
        }
    }

    fn prepare(&mut self, spec: &Spec) -> Result<()> {
        let (tone, focus_envelopes) = self.validated(spec)?;
        let key = prefix_key(spec);
        if let Some(position) = self.prepared.iter().position(|p| p.key == key) {
            let hit = self.prepared.remove(position);
            self.prepared.push(hit);
            self.work.reused_prefixes += 1;
        } else {
            if self.prepared.len() >= PREFIX_SLOTS {
                self.prepared.remove(0);
            }
            let Shaped {
                audio: shaped,
                finishing,
                dynamic_after_eq,
                compressor,
                deesser,
                windows,
                tone_temporal,
            } = self.shape(spec, &tone, &focus_envelopes)?;
            let lufs = dsp::loudness(&shaped, self.rate);
            let mut up = crate::scratch::Pcm::new((self.scratch)()?);
            let peak = crate::render_blocks::upsample_into(&shaped, &mut up, self.rate)?;
            let mut base = crate::scratch::Pcm::new((self.scratch)()?);
            for block in shaped.chunks(65_536) {
                base.push(block)?;
            }
            drop(shaped);
            let color = if spec.finishing.active {
                Some(up.color(spec.finishing.color_amount, peak)?)
            } else {
                None
            };
            self.prepared.push(Prepared {
                key,
                up,
                shaped: base,
                finishing,
                dynamic_after_eq,
                compressor,
                deesser,
                color,
                windows,
                lufs,
                tone_temporal,
                probes: Vec::new(),
                probe_bytes: 0,
            });
            self.work.prepared_prefixes += 1;
        }
        Ok(())
    }

    /// Solve the drive for the most recent prefix, which `prepare` made.
    fn search(&mut self, spec: &Spec) -> Result<Rendered> {
        let prepared = self.prepared.last_mut().unwrap();
        let rate = self.rate;
        let scratch = &mut self.scratch;
        let work = &mut self.work;
        // The spec's initial drive is a drive for this prefix: the set adds
        // the compression depth to the base guess, and the Denser version
        // and the restoring render start from a drive that already holds it.
        let initial = spec.initial_drive_db;
        let mut render = |drive: f64| -> Result<(f64, Stored)> {
            let key = [
                drive.to_bits(),
                spec.ceiling_dbtp.to_bits(),
                spec.clipper_db.to_bits(),
                spec.limiter.release_ms.to_bits(),
                spec.limiter.pre_attack_ms.to_bits(),
                spec.limiter.hold_ms.to_bits(),
                spec.limiter.slow_release_ms.to_bits(),
                spec.limiter.attack_ms.to_bits(),
                spec.true_peak as u64,
            ];
            if let Some((_, cached)) = prepared.probes.iter().find(|(k, _)| *k == key) {
                work.reused_probes += 1;
                return Ok((cached.report.lufs, cached.clone()));
            }
            work.rendered_probes += 1;
            let (mut audio, limiter, clipping, limiter_trace) =
                crate::render_blocks::render_peak_control(
                    &mut prepared.up,
                    drive,
                    spec.clipper_db,
                    dsp::gain(spec.ceiling_dbtp - 0.1),
                    spec.limiter,
                    rate,
                )?;
            if audio.iter().flatten().any(|x| !x.is_finite()) {
                return Err("Nonfinite rendered samples".into());
            }
            let (peak_estimate_dbtp, true_peak) =
                render_blocks::fit_true_peak(&mut audio, spec.ceiling_dbtp, spec.true_peak, rate);
            let lufs = dsp::loudness(&audio, rate).ok_or("Missing output loudness")?;
            let energy_attack_loss_p90_db = prepared
                .tone_temporal
                .as_ref()
                .and_then(|t| crate::analysis::energy_attack_loss_p90_from(t, &audio, rate));
            let bytes = audio.len() * std::mem::size_of::<Stereo>();
            let mut pcm = crate::scratch::Pcm::new(scratch()?);
            for block in audio.chunks(8192) {
                pcm.push(block)?;
            }
            drop(audio);
            let report = Rendered {
                audio: Arc::new(Vec::new()),
                lufs,
                peak_estimate_dbtp,
                true_peak,
                drive_db: drive,
                limiter,
                limiter_trace,
                clipping,
                finishing: prepared.finishing.clone(),
                dynamic_after_eq: prepared.dynamic_after_eq.clone(),
                compressor: prepared.compressor.clone(),
                deesser: prepared.deesser,
                color: prepared.color,
                search: None,
                restoration: None,
                prefix_lufs: prepared.lufs,
                energy_attack_loss_p90_db,
            };
            let result = Stored {
                report,
                pcm: Rc::new(RefCell::new(pcm)),
            };
            if prepared.probe_bytes + bytes <= work.probe_cache_budget_bytes {
                prepared.probes.push((key, result.clone()));
                prepared.probe_bytes += bytes;
            }
            Ok((lufs, result))
        };
        if let Some(drive) = spec.fixed_drive_db {
            return render(drive)?.1.load();
        }
        let (output, search) = search::solve(
            initial,
            initial + 12.0,
            spec.target_lufs,
            search::Policy {
                maximum_evaluations: 8,
                tolerance_lu: LOUDNESS_TOLERANCE_LU,
                ..search::Policy::default()
            },
            render,
        )?;
        let mut output = output.load()?;
        output.search = Some(search);
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mono_safe_cut_stays_in_its_band_and_its_passages() {
        // Wide noise: uncorrelated left and right.
        let mut state = 0x1234_5678u32;
        let mut noise = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state as f32 / u32::MAX as f32) * 2.0 - 1.0
        };
        let frames = 10 * 48000;
        let audio: Vec<Stereo> = (0..frames)
            .map(|_| [0.3 * noise(), 0.3 * noise()])
            .collect();
        // A cut of 6 dB in the 2 to 6 kHz band, wet in the second half.
        let cut = treatment::EnvelopedBand {
            rule: crate::width::MONO_SAFE_RULE.into(),
            domain: finishing::Domain::Side,
            band: dsp::EqBand {
                kind: dsp::Kind::Peak,
                hz: 3464.1,
                q: 0.87,
                gain_db: -6.0,
            },
            points: vec![
                treatment::BlendPoint {
                    sample: 0,
                    wet: 0.0,
                },
                treatment::BlendPoint {
                    sample: frames / 2 - 2400,
                    wet: 0.0,
                },
                treatment::BlendPoint {
                    sample: frames / 2 + 2400,
                    wet: 1.0,
                },
                treatment::BlendPoint {
                    sample: frames - 1,
                    wet: 1.0,
                },
            ],
        };
        let mut cut_audio = audio.clone();
        apply_mono_safe(&mut cut_audio, &[cut], 0, 48000);
        let image = |a: &[Stereo]| crate::stereo::analyze(a, 48000).unwrap();
        let (dry_first, dry_second) = (
            image(&audio[..frames / 2 - 4800]),
            image(&audio[frames / 2 + 4800..]),
        );
        let (first, second) = (
            image(&cut_audio[..frames / 2 - 4800]),
            image(&cut_audio[frames / 2 + 4800..]),
        );
        let side_mid =
            |i: &crate::stereo::StereoAnalysis, b: usize| i.whole.bands[b].side_mid_db.unwrap();
        // The dry half is untouched.
        for b in 1..6 {
            assert!(
                (side_mid(&first, b) - side_mid(&dry_first, b)).abs() < 0.05,
                "band {b}"
            );
        }
        // The wet half: the band falls by about the cut, the bands beside it
        // by little.
        let change = |b: usize| side_mid(&second, b) - side_mid(&dry_second, b);
        assert!(change(3) < -4.5, "{}", change(3));
        assert!(change(2).abs() < 0.6, "{}", change(2));
        assert!(change(4).abs() < 0.6, "{}", change(4));
        assert!(change(1).abs() < 0.2 && change(5).abs() < 0.2);
    }

    fn tone(hz: f64, amplitude: f64, index: usize) -> f32 {
        (amplitude * (2.0 * std::f64::consts::PI * hz * index as f64 / 48000.0).sin()) as f32
    }

    #[test]
    fn local_change_ignores_a_level_change_between_passages() {
        // A quiet passage holds a tone in each band above the bass; a loud
        // passage adds the bass. The output lowers the loud passage by 3 dB,
        // as a limiter does.
        let half = 6 * RESTORATION_WINDOW;
        let prefix: Vec<Stereo> = (0..2 * half)
            .map(|i| {
                let bass = if i < half { 0.0 } else { tone(60.0, 0.5, i) };
                let upper: f32 = [250.0, 1000.0, 3500.0, 8000.0, 14000.0]
                    .iter()
                    .map(|&hz| tone(hz, 0.03, i))
                    .sum();
                let x = bass + upper;
                [x, x]
            })
            .collect();
        let gain = dsp::gain(-3.0) as f32;
        let output: Vec<Stereo> = prefix
            .iter()
            .enumerate()
            .map(|(i, f)| if i < half { *f } else { f.map(|x| x * gain) })
            .collect();
        let (before, after) = (
            WindowBands::measure(&prefix, 48000),
            WindowBands::measure(&output, 48000),
        );
        let local = local_change(&before, &after);
        assert!(local.iter().all(|c| c.abs() < 0.05), "{local:?}");
        // The whole-song measure reads the same level change as a tone change.
        let (a, b) = (before.total(), after.total());
        let lufs = dsp::loudness(&output, 48000).unwrap() - dsp::loudness(&prefix, 48000).unwrap();
        let whole = |i: usize| 10.0 * (b[i] / a[i]).log10() - lufs;
        assert!(whole(2) - whole(0) > 1.0, "{} {}", whole(0), whole(2));
    }

    #[test]
    fn local_change_weights_windows_by_the_band_power() {
        // The bass drops 1 dB where it is loud and 3 dB where it is quiet.
        // The power weighting follows the loud passage.
        let half = 6 * RESTORATION_WINDOW;
        let mix = |loud_db: f64, quiet_db: f64| -> Vec<Stereo> {
            (0..2 * half)
                .map(|i| {
                    let bass = if i < half {
                        0.5 * dsp::gain(loud_db)
                    } else {
                        0.05 * dsp::gain(quiet_db)
                    };
                    let x = tone(60.0, bass, i) + tone(1000.0, 0.05, i);
                    [x, x]
                })
                .collect()
        };
        let change = local_change(
            &WindowBands::measure(&mix(0.0, 0.0), 48000),
            &WindowBands::measure(&mix(-1.0, -3.0), 48000),
        );
        // Power weighting gives about -1.1 dB; equal weighting about -2.0 dB.
        assert!(change[0] - change[2] > -1.5, "{change:?}");
    }

    #[test]
    fn local_change_skips_windows_far_under_the_loudest() {
        // A passage 60 dB down changes its tone; the loud passage does not.
        let half = 6 * RESTORATION_WINDOW;
        let mix = |quiet_bass: f64| -> Vec<Stereo> {
            (0..2 * half)
                .map(|i| {
                    let x = if i < half {
                        tone(60.0, 0.5, i) + tone(1000.0, 0.05, i)
                    } else {
                        tone(60.0, 0.0005 * quiet_bass, i) + tone(1000.0, 0.00005, i)
                    };
                    [x, x]
                })
                .collect()
        };
        let change = local_change(
            &WindowBands::measure(&mix(1.0), 48000),
            &WindowBands::measure(&mix(4.0), 48000),
        );
        assert!(change.iter().all(|c| c.abs() < 0.01), "{change:?}");
    }

    #[test]
    fn material_bands_follow_the_k_weighted_share() {
        // A weak 8 kHz tone holds under 1% of the raw power and over 1% of
        // the K-weighted power; the empty top band stays out.
        let audio: Vec<Stereo> = (0..6 * RESTORATION_WINDOW)
            .map(|i| {
                let x = tone(60.0, 0.5, i) + tone(1000.0, 0.2, i) + tone(8000.0, 0.035, i);
                [x, x]
            })
            .collect();
        let windows = WindowBands::measure(&audio, 48000);
        let total = windows.total();
        assert!(total[4] < 0.01 * total.iter().sum::<f64>());
        let material = windows.material();
        assert!(material[0] && material[2] && material[4] && !material[5]);
    }

    #[test]
    fn local_change_measures_a_tone_change_inside_passages() {
        let length = 12 * RESTORATION_WINDOW;
        let mix = |bass_db: f64| -> Vec<Stereo> {
            let bass = 0.5 * dsp::gain(bass_db);
            (0..length)
                .map(|i| {
                    let x = tone(60.0, bass, i) + tone(1000.0, 0.05, i);
                    [x, x]
                })
                .collect()
        };
        let local = local_change(
            &WindowBands::measure(&mix(0.0), 48000),
            &WindowBands::measure(&mix(-1.0), 48000),
        );
        assert!(
            (local[0] - local[2] + 1.0).abs() < 0.05,
            "bass {} mids {}",
            local[0],
            local[2]
        );
    }

    #[test]
    fn restoring_filters_cancel_the_measured_band_change() {
        let change = [-0.8, 0.5, 0.4, -0.3, 0.1, 0.0];
        let material = [true, true, true, true, true, false];
        let filters = restoring_filters(&change, &material, 48000);
        assert!(filters
            .iter()
            .all(|f| f.gain_db.abs() <= RESTORATION_MAXIMUM_DB));
        let edges = crate::stereo::BAND_EDGES_HZ;
        for band in 0..5 {
            let (low, high) = (edges[band].log2(), edges[band + 1].log2());
            let response: f64 = (0..32)
                .map(|k| {
                    let hz = 2f64.powf(low + (high - low) * (k as f64 + 0.5) / 32.0);
                    filters
                        .iter()
                        .map(|f| f.filter(48000).response_db(hz, 48000))
                        .sum::<f64>()
                })
                .sum::<f64>()
                / 32.0;
            // Changes shrink by the dead zone; small ones stay at zero.
            let wanted = restoration_target(change[band]);
            assert!(
                (response - wanted).abs() < 0.1,
                "band {band}: {response} against {wanted}"
            );
        }
        assert!(restoring_filters(&[0.1; 6], &[true; 6], 48000).is_empty());
    }

    #[test]
    fn restoration_is_continuous_in_the_measured_change() {
        // Two versions whose body change differs by 0.02 dB get nearly the
        // same correction, on either side of any threshold.
        let response = |change: [f64; 6]| -> f64 {
            restoring_filters(&change, &[true, true, true, true, false, false], 48000)
                .iter()
                .map(|f| f.filter(48000).response_db(1000.0, 48000))
                .sum()
        };
        let low = response([-0.4, 0.1, 0.19, 0.05, 0.0, 0.0]);
        let high = response([-0.4, 0.1, 0.21, 0.05, 0.0, 0.0]);
        assert!((low - high).abs() < 0.05, "{low} {high}");
        assert!(restoration_target(0.05) == 0.0 && restoration_target(-0.6) > 0.49);
    }
}
