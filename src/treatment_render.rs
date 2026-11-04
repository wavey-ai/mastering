//! Complete treatment rendering shared by the automatic sets and the presets.
use crate::{
    audio_io::{Result, Stereo},
    compressor, dsp, finishing, render_blocks, search, treatment,
};
use serde::Serialize;
use serde_json::json;
use std::sync::{Arc, Mutex};

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

/// The frames of a render: in memory, or in a scratch file from its peak
/// control to its delivery for a long recording (D253). A clone shares the
/// frames; the threads of a set share them (`set::render`).
#[derive(Clone, Default)]
pub struct RenderAudio(Kept);

#[derive(Clone, Default)]
enum Kept {
    /// No frames: a report whose frames are freed.
    #[default]
    Empty,
    Held(Arc<Vec<Stereo>>),
    Stored(Arc<Mutex<crate::scratch::Pcm>>),
}

impl RenderAudio {
    /// The frames `audio` in memory.
    pub fn held(audio: Vec<Stereo>) -> Self {
        Self(Kept::Held(Arc::new(audio)))
    }

    /// The frames of `pcm`, a scratch file.
    fn stored(pcm: Arc<Mutex<crate::scratch::Pcm>>) -> Self {
        Self(Kept::Stored(pcm))
    }

    pub fn len(&self) -> usize {
        use crate::scratch::Frames;
        match &self.0 {
            Kept::Empty => 0,
            Kept::Held(audio) => audio.len(),
            Kept::Stored(pcm) => lock(pcm).len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The frames in memory; `None` for a stored render.
    pub fn in_memory(&self) -> Option<&[Stereo]> {
        match &self.0 {
            Kept::Empty => Some(&[]),
            Kept::Held(audio) => Some(audio),
            Kept::Stored(_) => None,
        }
    }

    /// The frames are in a scratch file.
    pub fn is_stored(&self) -> bool {
        matches!(self.0, Kept::Stored(_))
    }

    /// The frames in memory. A stored render is read from its file.
    pub fn load(&self) -> Result<Arc<Vec<Stereo>>> {
        use crate::scratch::Frames;
        match &self.0 {
            Kept::Empty => Ok(Arc::default()),
            Kept::Held(audio) => Ok(audio.clone()),
            Kept::Stored(pcm) => {
                let mut pcm = lock(pcm);
                let (frames, mut audio) = (pcm.len(), Vec::new());
                pcm.read(0, frames, &mut audio)?;
                Ok(Arc::new(audio))
            }
        }
    }

    /// A reader of the frames.
    pub(crate) fn frames(&self) -> RenderFrames<'_> {
        use crate::scratch::{Place, Slice};
        match &self.0 {
            Kept::Empty => Place::Memory(Slice(&[])),
            Kept::Held(audio) => Place::Memory(Slice(audio)),
            Kept::Stored(pcm) => Place::File(lock(pcm)),
        }
    }
}

/// A scratch file of a render. A reader that fails leaves the file
/// unchanged, so a poisoned lock keeps its frames.
fn lock(pcm: &Mutex<crate::scratch::Pcm>) -> std::sync::MutexGuard<'_, crate::scratch::Pcm> {
    pcm.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl PartialEq for RenderAudio {
    fn eq(&self, other: &Self) -> bool {
        matches!((self.load(), other.load()), (Ok(a), Ok(b)) if a == b)
    }
}

impl std::fmt::Debug for RenderAudio {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "RenderAudio({} frames)", self.len())
    }
}

/// The frames of a `RenderAudio`.
pub(crate) type RenderFrames<'a> =
    crate::scratch::Place<'a, std::sync::MutexGuard<'a, crate::scratch::Pcm>>;

#[derive(Clone, Serialize)]
pub struct Rendered {
    #[serde(skip)]
    pub audio: RenderAudio,
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
    /// The drive response of the delivered render of a version, which its
    /// family measures once (D281).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub drive_response: Option<DriveResponse>,
}

/// The change of the output loudness of peak control of the prefix of a
/// render from a drive change of `shift_db` up and down and of `far_db` up,
/// and the loudness that peak control takes at the drive of the render
/// (D281, D283). A preview solves the drive of a new target from it, the
/// change of each section of the low-end hold (D282) and the restoring
/// gains (D284).
#[derive(Clone, Debug, Serialize)]
pub struct DriveResponse {
    pub rule: &'static str,
    pub shift_db: f64,
    /// Loudness change at the drive `shift_db` up, in LU.
    pub up_lu: f64,
    /// Loudness change at the drive `shift_db` down, in LU.
    pub down_lu: f64,
    pub far_db: f64,
    /// Loudness change at the drive `far_db` up, in LU.
    pub far_lu: f64,
    /// The loudness of the prefix with the drive and the true-peak trim,
    /// less the output loudness, in LU.
    pub loss_lu: f64,
    /// The change of each held band in each section of the hold
    /// (`low_end::sections`) against the output loudness, at the drive
    /// `shift_db` down, in dB.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sections_down_db: Vec<[f64; 2]>,
    /// The same change at the drive `shift_db` up.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sections_up_db: Vec<[f64; 2]>,
    /// The same change without peak control: the prefix against the render.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sections_free_db: Vec<[f64; 2]>,
    /// The same change at the drive `far_db` up.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sections_far_db: Vec<[f64; 2]>,
    /// The local band change of the restoration (`local_change`) at the
    /// drive `shift_db` down, less the local band change at the drive, in
    /// dB (D284).
    pub local_down_db: [f64; 6],
    /// The same change at the drive `shift_db` up.
    pub local_up_db: [f64; 6],
    /// The same change at the drive `far_db` up.
    pub local_far_db: [f64; 6],
    /// The same change without peak control.
    pub local_free_db: [f64; 6],
    /// Gain of the restoring filter of 20–120 Hz of the shelf model: peak
    /// control at the drive of the prefix with that filter, in dB.
    pub shelf_db: f64,
    /// Loudness change of the shelf model, in LU.
    pub shelf_lu: f64,
    /// The local band change of the shelf model, less the local band change
    /// at the drive, in dB.
    pub shelf_local_db: [f64; 6],
    /// The change of each held band in each section of the shelf model, as
    /// `sections_down_db`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sections_shelf_db: Vec<[f64; 2]>,
    /// The loudness that the restoring filter of the shelf model adds to
    /// the prefix without peak control, in LU (D287).
    pub shelf_free_lu: f64,
    /// The local band change of the shelf model without peak control, less
    /// the local band change at the drive, in dB (D287).
    pub shelf_free_local_db: [f64; 6],
    /// The change of each held band in each section of the shelf model
    /// without peak control, as `sections_down_db` (D287).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub sections_shelf_free_db: Vec<[f64; 2]>,
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
    /// Gains of the restoring filters of the first render, whose change is
    /// `measured_change_db` (D285).
    pub measured_gains_db: [f64; 6],
    pub filters: Vec<dsp::EqBand>,
    /// Change from the prefix to the delivered output after restoration.
    pub delivered_change_db: [f64; 6],
    /// Renders of the version: the first and one for each pass.
    pub renders: usize,
    /// Gains of the restoring filters of the delivered render, per band.
    pub gains_db: [f64; 6],
}

/// Transform of the window bands: a quarter of a restoration window.
pub(crate) const WINDOW_FFT: usize = RESTORATION_WINDOW / 4;

/// The band powers and the K-weighted power of blocks of `WINDOW_FFT`
/// frames, as `WindowScan` adds them to its windows.
pub(crate) struct BlockScan {
    /// The band and the K-weighting power of each bin.
    bins: Vec<(Option<usize>, f64)>,
    stft: crate::stft::StereoStft,
}

impl BlockScan {
    pub(crate) fn new(rate: u32) -> Self {
        let edges = crate::stereo::BAND_EDGES_HZ;
        let bins = (0..=WINDOW_FFT / 2)
            .map(|bin| {
                let hz = bin as f64 * rate as f64 / WINDOW_FFT as f64;
                (
                    edges.windows(2).position(|e| hz >= e[0] && hz < e[1]),
                    dsp::k_weighting_power(hz, rate),
                )
            })
            .collect();
        Self {
            bins,
            stft: crate::stft::StereoStft::new(WINDOW_FFT, WINDOW_FFT),
        }
    }

    /// Add the power of each band of `block`, `WINDOW_FFT` frames, to
    /// `power`, its K-weighted power to `level`, and the K-weighted power of
    /// each band to `weighted`.
    pub(crate) fn add(
        &mut self,
        block: &[Stereo],
        power: &mut [f64; 6],
        level: &mut f64,
        weighted: &mut [f64; 6],
    ) {
        if let Some((l, r)) = self.stft.add_with(WINDOW_FFT, |i| block[i]) {
            for (bin, (band, weight)) in self.bins.iter().enumerate() {
                let p = l[bin].norm_sqr() + r[bin].norm_sqr();
                *level += p * weight;
                if let Some(i) = band {
                    power[*i] += p;
                    weighted[*i] += p * weight;
                }
            }
        }
    }
}

/// `WindowBands::measure` of a signal of a known length whose frames arrive
/// in spans.
pub(crate) struct WindowScan {
    scan: BlockScan,
    /// The frames of the block in progress.
    block: Vec<Stereo>,
    /// The index of the next block, and the blocks of the complete windows.
    index: usize,
    blocks: usize,
    bands: WindowBands,
}

impl WindowScan {
    /// The scan of a signal of `frames` frames at `rate`.
    pub(crate) fn new(frames: usize, rate: u32) -> Self {
        let windows = frames / RESTORATION_WINDOW;
        Self {
            scan: BlockScan::new(rate),
            block: Vec::with_capacity(WINDOW_FFT),
            index: 0,
            blocks: windows * 4,
            bands: WindowBands {
                bands: vec![[0.0; 6]; windows],
                weighted: vec![0.0; windows],
                weighted_bands: [0.0; 6],
            },
        }
    }

    /// Add the next frames of the signal.
    pub(crate) fn push(&mut self, mut audio: &[Stereo]) {
        while self.index < self.blocks && !audio.is_empty() {
            let take = (WINDOW_FFT - self.block.len()).min(audio.len());
            self.block.extend_from_slice(&audio[..take]);
            audio = &audio[take..];
            if self.block.len() == WINDOW_FFT {
                self.add_block();
            }
        }
    }

    fn add_block(&mut self) {
        let (index, bands) = (self.index, &mut self.bands);
        self.scan.add(
            &self.block,
            &mut bands.bands[index / 4],
            &mut bands.weighted[index / 4],
            &mut bands.weighted_bands,
        );
        self.block.clear();
        self.index += 1;
    }

    pub(crate) fn finish(self) -> WindowBands {
        self.bands
    }
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
        let mut scan = WindowScan::new(audio.len(), rate);
        scan.push(audio);
        scan.finish()
    }

    /// `measure` of the frames of `audio`, read in blocks of 65,536 frames:
    /// a stored render (D253).
    pub(crate) fn measure_read(audio: &mut impl crate::scratch::Frames, rate: u32) -> Result<Self> {
        const BLOCK: usize = 65_536;
        let frames = audio.len();
        let mut scan = WindowScan::new(frames, rate);
        let mut buffer = Vec::new();
        for start in (0..frames).step_by(BLOCK) {
            scan.push(audio.window(start, BLOCK.min(frames - start), &mut buffer)?);
        }
        Ok(scan.finish())
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
pub(crate) fn restoring_filter(band: usize, gain_db: f64) -> dsp::EqBand {
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
    let used: Vec<usize> = (0..6).filter(|&i| material[i]).collect();
    let mut gains = [0.0f64; 6];
    if used.iter().all(|&i| restoration_target(change[i]) == 0.0) {
        return gains;
    }
    // Responses per dB of gain are nearly linear at these gains; solve the
    // square system over the material bands by Gaussian elimination.
    let n = used.len();
    let mut a: Vec<Vec<f64>> = used
        .iter()
        .map(|&band| {
            let mut row: Vec<f64> = used
                .iter()
                .map(|&f| restoring_response_db(f, band, rate))
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

/// The mean log-frequency response at `rate` of the restoring filter of
/// band `filter` at 1 dB in band `band`, in dB.
pub(crate) fn restoring_response_db(filter: usize, band: usize, rate: u32) -> f64 {
    let edges = crate::stereo::BAND_EDGES_HZ;
    let (low, high) = (edges[band].log2(), edges[band + 1].min(18000.0).log2());
    let biquad = restoring_filter(filter, 1.0).filter(rate);
    (0..32)
        .map(|k| {
            biquad.response_db(
                2f64.powf(low + (high - low) * (k as f64 + 0.5) / 32.0),
                rate,
            )
        })
        .sum::<f64>()
        / 32.0
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
    /// The 4x prefix. A prefix of a long recording keeps none, and its 4x
    /// frames come from `shaped` (`render_blocks::upsampled_min_frames`).
    up: Option<crate::scratch::Pcm>,
    /// The processed prefix at the base rate (`Renderer::prefix`), for the
    /// audibility check of a candidate (`selection::ladder`).
    shaped: std::cell::RefCell<crate::scratch::Pcm>,
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

impl Prepared {
    /// The prefix at the base rate and its 4x frames at `rate`.
    fn frames(
        &mut self,
        rate: u32,
    ) -> (
        &std::cell::RefCell<crate::scratch::Pcm>,
        render_blocks::Up<'_, crate::scratch::Shared<'_>>,
    ) {
        let up = match &mut self.up {
            Some(up) => render_blocks::Up::Stored(up),
            None => render_blocks::Up::Upsampled(Box::new(render_blocks::Upsampled::new(
                crate::scratch::Shared(&self.shaped),
                rate,
            ))),
        };
        (&self.shaped, up)
    }
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
    pcm: Arc<Mutex<crate::scratch::Pcm>>,
}
impl Stored {
    /// The render with its frames in memory, or in its scratch file with
    /// `stored` (D253).
    fn load(self, stored: bool) -> Result<Rendered> {
        use crate::scratch::Frames;
        if stored {
            return Ok(Rendered {
                audio: RenderAudio::stored(self.pcm),
                ..self.report
            });
        }
        let mut pcm = lock(&self.pcm);
        let frames = pcm.len();
        let mut audio = Vec::with_capacity(frames);
        let mut block = Vec::new();
        for start in (0..frames).step_by(8192) {
            pcm.read(start, 8192.min(frames - start), &mut block)?;
            audio.extend_from_slice(&block);
        }
        Ok(Rendered {
            audio: RenderAudio::held(audio),
            ..self.report
        })
    }
}

/// The PCM of a source at its render rate, for a renderer.
pub enum RenderSource<'a> {
    /// Frames that the caller keeps.
    Borrowed(&'a [Stereo]),
    /// Frames that the renderer moves to a scratch file and frees (D232).
    Owned(Vec<Stereo>),
    /// Frames in a scratch file from the read, which the renderer takes
    /// (D254).
    Stored(crate::audio_io::StoredSource),
    /// Frames in a scratch file that the caller keeps (D257).
    Lent(&'a mut crate::audio_io::StoredSource),
}

impl RenderSource<'_> {
    /// The frames of the source.
    pub fn len(&self) -> usize {
        match self {
            RenderSource::Borrowed(audio) => audio.len(),
            RenderSource::Owned(audio) => audio.len(),
            RenderSource::Stored(source) => source.frames(),
            RenderSource::Lent(source) => source.frames(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// A reader of the frames.
    pub(crate) fn frames(&mut self) -> crate::scratch::Place<'_, &mut crate::scratch::Pcm> {
        use crate::scratch::{Place, Slice};
        match self {
            RenderSource::Borrowed(audio) => Place::Memory(Slice(audio)),
            RenderSource::Owned(audio) => Place::Memory(Slice(audio)),
            RenderSource::Stored(source) => Place::File(&mut source.audio),
            RenderSource::Lent(source) => Place::File(&mut source.audio),
        }
    }

    /// The source for one more reader, which leaves this source to the
    /// caller: the frames in memory, or the scratch file (D257).
    pub fn lend(&mut self) -> RenderSource<'_> {
        match self {
            RenderSource::Borrowed(audio) => RenderSource::Borrowed(audio),
            RenderSource::Owned(audio) => RenderSource::Borrowed(audio),
            RenderSource::Stored(source) => RenderSource::Lent(source),
            RenderSource::Lent(source) => RenderSource::Lent(source),
        }
    }
}

/// The source of a renderer at its render rate: the frames of the caller,
/// or a scratch file that holds them.
enum Input<'a> {
    Whole(&'a [Stereo]),
    Stored(crate::scratch::Pcm),
    Lent(&'a mut crate::scratch::Pcm),
}

/// One immutable source, one prepared prefix, and bounded shared probe storage.
/// Completed versions are delivered by the caller; they do not accumulate here.
pub struct Renderer<'a> {
    /// The source, which the prepare of each prefix copies.
    input: std::cell::RefCell<Input<'a>>,
    /// The frames of the source.
    frames: usize,
    /// Each sample of the source is finite.
    finite: bool,
    /// The render rate of the source, in Hz.
    rate: u32,
    /// Prefix loudness of the reference whose drive the specs start from.
    /// The first search of a render moves its initial drive by the prefix
    /// loudness difference, so a candidate with a louder tone starts lower.
    pub reference_prefix_lufs: Option<f64>,
    /// Most recently used last. The renderer keeps one prefix: in ten
    /// measured runs, one reused prefix was the older of two (D230).
    prepared: Vec<Prepared>,
    /// Frames from which a prepared prefix keeps no 4x frames
    /// (`render_blocks::upsampled_min_frames`).
    upsampled_min_frames: usize,
    /// Frames from which a render stays in its scratch file
    /// (`render_blocks::stored_render_min_frames`).
    stored_render_min_frames: usize,
    scratch: crate::scratch::Factory,
    pub work: Work,
    /// The time limit of the worker, for its candidate ladders.
    pub deadline: Option<crate::selection::Deadline>,
    /// The trials of the ladders of a worker that can continue in another
    /// invocation, and the trials that it takes from the invocation before.
    pub trials: Option<crate::selection::TrialRecord>,
}

const PREFIX_SLOTS: usize = 1;

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
        // The section-gain points of the stage are frames of the whole
        // recording, and its color takes the peak of the whole recording.
        if shaped.len() != frames {
            return Err("A span render has no finishing stage".into());
        }
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
        dsp::side_eq(&mut shaped, &[side_highpass(hz)], rate);
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
    // A render of the whole recording gives the hold shift of the
    // compressor (D280).
    let whole = offset == 0 && shaped.len() == frames;
    let compressor = spec
        .compressor
        .map(|s| match whole {
            true => compressor::process_with_hold_shift(&mut shaped, s, rate),
            false => compressor::process(&mut shaped, s, rate),
        })
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

/// The side high-pass of `Spec::side_highpass_hz` at `hz`.
fn side_highpass(hz: f64) -> dsp::EqBand {
    dsp::EqBand {
        kind: dsp::Kind::HighPass,
        hz,
        q: std::f64::consts::FRAC_1_SQRT_2,
        gain_db: 0.0,
    }
}

/// `frames` at the end of `pcm`, in blocks.
fn push_blocks(pcm: &mut crate::scratch::Pcm, frames: &[Stereo]) -> Result<()> {
    for block in frames.chunks(65_536) {
        pcm.push(block)?;
    }
    Ok(())
}

/// The prepared prefix of `Renderer::prepare_streamed`.
struct StreamedPrefix {
    base: crate::scratch::Pcm,
    up: Option<crate::scratch::Pcm>,
    lufs: Option<f64>,
    deesser: Option<crate::deesser::Stats>,
    compressor: Option<compressor::Stats>,
    windows: WindowBands,
    tone_temporal: crate::analysis::Temporal,
}

/// The measures of the streamed prefix after its last filter before the
/// restoration: the window bands, then the restoration, then the onsets.
struct PrefixMeasure {
    windows: WindowScan,
    restoration: dsp::Eq,
    temporal: crate::analysis::Scan,
    /// Whether the compressor input must be finite.
    check: bool,
    nonfinite: bool,
}

impl PrefixMeasure {
    fn push(&mut self, audio: &mut [Stereo]) {
        self.windows.push(audio);
        self.restoration.process(audio);
        self.temporal.push(audio);
        if self.check {
            self.nonfinite |= audio.iter().flatten().any(|v| !v.is_finite());
        }
    }
}

/// The outputs of the streamed prefix: its loudness, and its frames at the
/// base rate and, below `render_blocks::upsampled_min_frames`, at 4x.
struct PrefixOut {
    meter: dsp::LoudnessMeter,
    up: Option<(render_blocks::Upsample, crate::scratch::Pcm)>,
    base: crate::scratch::Pcm,
}

impl PrefixOut {
    fn push(&mut self, audio: &[Stereo]) -> Result<()> {
        self.meter.push(audio);
        if let Some((upsample, up)) = &mut self.up {
            upsample.push(audio, up)?;
        }
        push_blocks(&mut self.base, audio)
    }
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
    for cut in cuts {
        let Some(gain) = mono_safe_gain(cut) else {
            continue;
        };
        gain.apply_each(audio, rate, |i, frame, wet_frame| {
            mono_safe_blend(cut, offset + i, frame, wet_frame)
        });
    }
}

/// The side gain of the mono-safe `cut`: its gain in its stereo analysis
/// band. None when its frequency is outside the bands.
pub(crate) fn mono_safe_gain(cut: &treatment::EnvelopedBand) -> Option<crate::width::SideGain> {
    let edges = &crate::stereo::BAND_EDGES_HZ;
    let band = (0..6).find(|b| cut.band.hz >= edges[*b] && cut.band.hz < edges[b + 1])?;
    let mut gains = [0.0; 6];
    gains[band] = cut.band.gain_db;
    Some(crate::width::SideGain::gains(gains))
}

/// `frame`, frame `sample` of the recording, blended toward `wet_frame`,
/// the frame after the side gain of `cut`, by the wet share of its
/// envelope.
pub(crate) fn mono_safe_blend(
    cut: &treatment::EnvelopedBand,
    sample: usize,
    mut frame: Stereo,
    wet_frame: Stereo,
) -> Stereo {
    let wet = cut.wet_at(sample) as f32;
    if wet > 0.0 {
        for c in 0..2 {
            frame[c] += wet * (wet_frame[c] - frame[c]);
        }
    }
    frame
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

/// Audio in a scratch file of a renderer (`Renderer::spill`).
pub(crate) struct Spilled(crate::scratch::Pcm);

impl Spilled {
    /// Read the audio back into `audio`.
    pub(crate) fn restore(mut self, audio: &mut RenderAudio) -> Result<()> {
        use crate::scratch::Frames;
        let mut read = Vec::new();
        let frames = self.0.len();
        self.0.read(0, frames, &mut read)?;
        *audio = RenderAudio::held(read);
        Ok(())
    }
}

impl<'a> Renderer<'a> {
    /// A renderer of `input`, at the render rate `rate`.
    pub fn new(input: &'a [Stereo], rate: u32) -> Self {
        Self::with_scratch(input, rate, Box::new(crate::scratch::temporary))
    }

    pub fn with_scratch(input: &'a [Stereo], rate: u32, scratch: crate::scratch::Factory) -> Self {
        let finite = input.iter().flatten().all(|x| x.is_finite());
        Self::with_input(Input::Whole(input), input.len(), finite, rate, scratch)
    }

    /// A renderer of `source`. An owned source moves to a scratch file of
    /// `scratch`, and its buffer is freed.
    pub fn of_source(
        source: RenderSource<'a>,
        rate: u32,
        scratch: crate::scratch::Factory,
    ) -> Result<Self> {
        match source {
            RenderSource::Borrowed(audio) => Ok(Self::with_scratch(audio, rate, scratch)),
            RenderSource::Owned(audio) => Self::with_stored(&audio, rate, scratch),
            RenderSource::Stored(mut source) => {
                let frames = source.frames();
                let finite = crate::scratch::all_finite(&mut source.audio)?;
                Ok(Self::with_input(
                    Input::Stored(source.audio),
                    frames,
                    finite,
                    rate,
                    scratch,
                ))
            }
            RenderSource::Lent(source) => {
                let frames = source.frames();
                let finite = crate::scratch::all_finite(&mut source.audio)?;
                Ok(Self::with_input(
                    Input::Lent(&mut source.audio),
                    frames,
                    finite,
                    rate,
                    scratch,
                ))
            }
        }
    }

    /// A renderer of `input` that holds it in a scratch file of `scratch`.
    fn with_stored(
        input: &[Stereo],
        rate: u32,
        mut scratch: crate::scratch::Factory,
    ) -> Result<Self> {
        let mut stored = crate::scratch::Pcm::new(scratch()?);
        push_blocks(&mut stored, input)?;
        let finite = input.iter().flatten().all(|x| x.is_finite());
        Ok(Self::with_input(
            Input::Stored(stored),
            input.len(),
            finite,
            rate,
            scratch,
        ))
    }

    fn with_input(
        input: Input<'a>,
        frames: usize,
        finite: bool,
        rate: u32,
        scratch: crate::scratch::Factory,
    ) -> Self {
        Self {
            input: std::cell::RefCell::new(input),
            frames,
            finite,
            rate,
            reference_prefix_lufs: None,
            prepared: Vec::new(),
            upsampled_min_frames: render_blocks::upsampled_min_frames(rate),
            stored_render_min_frames: render_blocks::stored_render_min_frames(rate),
            scratch,
            work: Work {
                probe_cache_budget_bytes: 128 * 1024 * 1024,
                ..Work::default()
            },
            deadline: None,
            trials: None,
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
                let change = local_change(
                    &reference,
                    &WindowBands::measure_read(&mut first.audio.frames(), self.rate)?,
                );
                Restoration {
                    rule: "set.peak-tone-restoration.v4",
                    window_samples: RESTORATION_WINDOW,
                    material: reference.material(),
                    measured_change_db: change,
                    measured_gains_db: spec.restoration_db,
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
            restoration.delivered_change_db = local_change(
                &reference,
                &WindowBands::measure_read(&mut current.audio.frames(), self.rate)?,
            );
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
            envelope.validate(&tone, self.frames)?;
        }
        if self.frames < 3 * self.rate as usize
            || !self.finite
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
        let audio = match &mut *self.input.borrow_mut() {
            Input::Whole(input) => input.to_vec(),
            Input::Stored(stored) => {
                let mut audio = Vec::new();
                crate::scratch::Frames::read(stored, 0, self.frames, &mut audio)?;
                audio
            }
            Input::Lent(stored) => {
                let mut audio = Vec::new();
                crate::scratch::Frames::read(*stored, 0, self.frames, &mut audio)?;
                audio
            }
        };
        shape_span(
            audio,
            spec,
            tone,
            focus_envelopes,
            0,
            self.frames,
            self.rate,
        )
    }

    /// The frames of `audio` in a scratch file of the renderer, until
    /// `Spilled::restore` reads them back; `audio` is left empty. None, with
    /// `audio` unchanged, when another reference shares it or when its
    /// frames are in a scratch file.
    pub(crate) fn spill(&mut self, audio: &mut RenderAudio) -> Result<Option<Spilled>> {
        let held = match std::mem::take(&mut audio.0) {
            Kept::Held(held) => held,
            other => {
                audio.0 = other;
                return Ok(None);
            }
        };
        let owned = match Arc::try_unwrap(held) {
            Ok(owned) => owned,
            Err(shared) => {
                *audio = RenderAudio(Kept::Held(shared));
                return Ok(None);
            }
        };
        let mut pcm = crate::scratch::Pcm::new((self.scratch)()?);
        push_blocks(&mut pcm, &owned)?;
        Ok(Some(Spilled(pcm)))
    }

    /// The drive response of `rendered`, a render of `spec` (D281, D282,
    /// D283, D284, D287): peak control of its prefix at its drive, at the
    /// drive `render_blocks::DRIVE_RESPONSE_SHIFT_DB` up and down and at the
    /// drive `render_blocks::DRIVE_RESPONSE_FAR_DB` up, the prefix without
    /// it, and the prefix with the restoring filter of 20–120 Hz at
    /// `render_blocks::DRIVE_RESPONSE_SHELF_DB`, with peak control at its
    /// drive and without it (`render_blocks::drive_loudness`). The local
    /// band changes take the window bands of the prefix before its restoring
    /// filters, as the restoration does. The prefix of a delivered render is
    /// the prefix of its spec with the restoring gains of its restoration. A
    /// renderer that does not hold the prefix prepares it.
    pub fn drive_response(
        &mut self,
        spec: &Spec,
        rendered: &Rendered,
    ) -> Result<Option<DriveResponse>> {
        let Some(prefix_lufs) = rendered.prefix_lufs else {
            return Ok(None);
        };
        let rate = self.rate;
        let index = self.prepared_index(spec)?;
        let reference = self.prepared[index].windows.clone();
        let (_, mut up) = self.prepared[index].frames(rate);
        let loudness = render_blocks::drive_loudness(
            &mut up,
            rendered.drive_db,
            spec.clipper_db,
            dsp::gain(spec.ceiling_dbtp - 0.1),
            spec.limiter,
            rate,
        )?;
        Ok(loudness.map(|models| {
            let [at, up, down, far, shelf, free, shelf_free] = models.lufs;
            let part = |k: usize| models.sections_db.iter().map(|s| s[k]).collect();
            let local = models.windows.map(|w| local_change(&reference, &w));
            let against =
                |k: usize| -> [f64; 6] { std::array::from_fn(|b| local[k][b] - local[0][b]) };
            DriveResponse {
                rule: "set.drive-response.v5",
                shift_db: render_blocks::DRIVE_RESPONSE_SHIFT_DB,
                up_lu: up - at,
                down_lu: down - at,
                far_db: render_blocks::DRIVE_RESPONSE_FAR_DB,
                far_lu: far - at,
                loss_lu: prefix_lufs + rendered.drive_db + rendered.true_peak.trim_db
                    - rendered.lufs,
                sections_down_db: part(0),
                sections_up_db: part(1),
                sections_free_db: part(2),
                sections_far_db: part(3),
                local_down_db: against(2),
                local_up_db: against(1),
                local_far_db: against(3),
                local_free_db: against(4),
                shelf_db: render_blocks::DRIVE_RESPONSE_SHELF_DB,
                shelf_lu: shelf - at,
                shelf_local_db: against(5),
                sections_shelf_db: part(4),
                shelf_free_lu: shelf_free - free,
                shelf_free_local_db: against(6),
                sections_shelf_free_db: part(5),
            }
        }))
    }

    /// The prepared prefix of `spec` in scratch storage: its frames at the
    /// base rate and at 4x. A renderer that does not hold it prepares it.
    pub(crate) fn prepared_frames(
        &mut self,
        spec: &Spec,
    ) -> Result<(
        &std::cell::RefCell<crate::scratch::Pcm>,
        render_blocks::Up<'_, crate::scratch::Shared<'_>>,
    )> {
        let index = self.prepared_index(spec)?;
        Ok(self.prepared[index].frames(self.rate))
    }

    /// The index of the prepared prefix of `spec`. A renderer that does not
    /// hold it prepares it.
    fn prepared_index(&mut self, spec: &Spec) -> Result<usize> {
        let key = prefix_key(spec);
        match self.prepared.iter().position(|p| p.key == key) {
            Some(index) => Ok(index),
            None => {
                self.prepare(spec)?;
                Ok(self.prepared.len() - 1)
            }
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
            // The whole prefix gives the errors of an invalid compressor in
            // the order of its stages.
            if !spec.finishing.active
                && spec.dynamic_after_eq.is_empty()
                && spec.compressor.is_none_or(|s| s.validate().is_ok())
            {
                let streamed = self.prepare_streamed(spec, &tone, &focus_envelopes)?;
                self.prepared.push(Prepared {
                    key,
                    up: streamed.up,
                    shaped: std::cell::RefCell::new(streamed.base),
                    finishing: None,
                    dynamic_after_eq: Vec::new(),
                    compressor: streamed.compressor,
                    deesser: streamed.deesser,
                    color: None,
                    windows: streamed.windows,
                    lufs: streamed.lufs,
                    tone_temporal: Some(streamed.tone_temporal),
                    probes: Vec::new(),
                    probe_bytes: 0,
                });
                self.work.prepared_prefixes += 1;
                return Ok(());
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
            push_blocks(&mut base, &shaped)?;
            drop(shaped);
            let color = if spec.finishing.active {
                Some(up.color(spec.finishing.color_amount, peak)?)
            } else {
                None
            };
            self.prepared.push(Prepared {
                key,
                up: Some(up),
                shaped: std::cell::RefCell::new(base),
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

    /// Frames `start..start + count` of the source, into `out`.
    fn read_input(&self, start: usize, count: usize, out: &mut Vec<Stereo>) -> Result<()> {
        match &mut *self.input.borrow_mut() {
            Input::Whole(input) => {
                out.clear();
                out.extend_from_slice(&input[start..start + count]);
            }
            Input::Stored(stored) => crate::scratch::Frames::read(stored, start, count, out)?,
            Input::Lent(stored) => crate::scratch::Frames::read(*stored, start, count, out)?,
        }
        Ok(())
    }

    /// The outputs of `shape` and of `prepare` for `spec`, from the source
    /// in pieces. Each stage of `spec` streams: the finishing stage is
    /// inactive and `spec` has no dynamic bands. The de-esser measures the
    /// whole signal before it cuts, so the signal before it goes to scratch
    /// storage. The compressor holds about two section windows of frames
    /// (`compressor::Stream`). A pass holds one piece of frames, and the
    /// 4x stream one span (`render_blocks::Upsample`), not the recording.
    fn prepare_streamed(
        &mut self,
        spec: &Spec,
        tone: &treatment::Tone,
        focus_envelopes: &[treatment::FocusEnvelope],
    ) -> Result<StreamedPrefix> {
        use crate::{prefix_stream::Chain, scratch::Frames};
        let (frames, rate) = (self.frames, self.rate);
        // The frames of a read. The buffers of a pass hold a piece, and only
        // the 4x stream holds a span: buffers of a span at each stage raised
        // the peak memory footprint of the sets (D243).
        let piece = 1 << 16;
        let mut chain = Chain::new(frames);
        if let Some(alignment) = &spec.phase_alignment {
            chain.alignment(alignment, rate);
        }
        chain.tone(tone.stream(focus_envelopes, 0, frames, rate)?);
        if let Some(gain) = &spec.side_gain {
            chain.side_gain(gain, rate);
        }
        chain.mono_safe(&spec.mono_safe, rate);
        // Each buffer has room for the largest output of its pass, and the
        // passes reuse the buffers from span to span. A buffer that grows,
        // or a new buffer for each span, raises the memory that the
        // allocator keeps.
        let mut block = Vec::with_capacity(chain.capacity(piece));
        let (mut before_cut, deesser) = match &spec.deesser {
            None => (None, None),
            Some(plan) => {
                let mut stored = crate::scratch::Pcm::new((self.scratch)()?);
                let mut mids = crate::deesser::MidPowers::new(frames, rate);
                // Each piece reads into the buffer of the piece before it.
                let mut store = |audio: Vec<Stereo>| -> Result<Vec<Stereo>> {
                    mids.push(&audio);
                    push_blocks(&mut stored, &audio)?;
                    Ok(audio)
                };
                for start in (0..frames).step_by(piece) {
                    self.read_input(start, piece.min(frames - start), &mut block)?;
                    block = store(chain.push(std::mem::take(&mut block))?)?;
                }
                store(chain.finish()?)?;
                let (cut, stats) = plan.cuts(&mids.finish(), 0);
                chain = Chain::new(frames);
                if stats.cut_frames > 0 {
                    chain.band_cut(crate::deesser::BandCut::new(cut, 0, rate));
                }
                (Some(stored), Some(stats))
            }
        };
        if let Some(hz) = spec.side_highpass_hz {
            chain.side_eq(&[side_highpass(hz)], rate);
        }
        let capacity = chain.capacity(piece);
        block.reserve(capacity.saturating_sub(block.len()));
        let compress = spec.compressor.filter(|s| compressor::changes(*s));
        let lag = compress.map_or(0, |s| compressor::Stream::lag(s, rate));
        let mut measure = PrefixMeasure {
            windows: WindowScan::new(frames, rate),
            restoration: dsp::Eq::new(&filters_of(&spec.restoration_db), rate),
            temporal: crate::analysis::Scan::new(frames, rate),
            check: spec.compressor.is_some(),
            nonfinite: false,
        };
        let up = if frames < self.upsampled_min_frames {
            Some((
                render_blocks::Upsample::new(frames, rate, capacity + lag),
                crate::scratch::Pcm::new((self.scratch)()?),
            ))
        } else {
            None
        };
        let mut out = PrefixOut {
            meter: dsp::LoudnessMeter::new(rate),
            up,
            base: crate::scratch::Pcm::new((self.scratch)()?),
        };
        let mut compressor = compress
            .map(|s| compressor::Stream::new(s, rate, frames, capacity).with_hold_shift(rate));
        let mut compressed = Vec::with_capacity(capacity + lag);
        let mut deliver = |mut audio: Vec<Stereo>| -> Result<Vec<Stereo>> {
            measure.push(&mut audio);
            match &mut compressor {
                Some(compressor) => {
                    compressed.clear();
                    compressor.push(&audio, &mut compressed);
                    out.push(&compressed)?;
                }
                None => out.push(&audio)?,
            }
            Ok(audio)
        };
        for start in (0..frames).step_by(piece) {
            let count = piece.min(frames - start);
            match &mut before_cut {
                Some(stored) => stored.read(start, count, &mut block)?,
                None => self.read_input(start, count, &mut block)?,
            }
            block = deliver(chain.push(std::mem::take(&mut block))?)?;
        }
        deliver(chain.finish()?)?;
        drop(before_cut);
        let PrefixMeasure {
            windows,
            temporal,
            nonfinite,
            ..
        } = measure;
        if nonfinite {
            return Err("Compressor input must be finite".into());
        }
        let compressor = match compressor {
            Some(compressor) => {
                compressed.clear();
                let stats = compressor.finish(&mut compressed);
                let rest = compressed;
                out.push(&rest)?;
                Some(stats)
            }
            None => spec.compressor.map(|_| compressor::Stats::default()),
        };
        let PrefixOut { meter, up, base } = out;
        let up = up.map(|(upsample, up)| {
            upsample.finish();
            up
        });
        Ok(StreamedPrefix {
            base,
            up,
            lufs: meter.finish(),
            deesser,
            compressor,
            windows: windows.finish(),
            tone_temporal: temporal.finish(),
        })
    }

    /// Solve the drive for the most recent prefix, which `prepare` made.
    fn search(&mut self, spec: &Spec) -> Result<Rendered> {
        let stored = self.frames >= self.stored_render_min_frames;
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
            let mut pcm = crate::scratch::Pcm::new(scratch()?);
            let (limiter, clipping, limiter_trace, peak_estimate_dbtp, true_peak, lufs, loss) =
                if stored {
                    // The render goes to its scratch file in pieces, and its
                    // measures read it there (D253).
                    let (limiter, clipping, limiter_trace) =
                        crate::render_blocks::render_peak_control_into(
                            &mut prepared.frames(rate).1,
                            drive,
                            spec.clipper_db,
                            dsp::gain(spec.ceiling_dbtp - 0.1),
                            spec.limiter,
                            rate,
                            &mut pcm,
                        )?;
                    let (peak_estimate_dbtp, true_peak) = render_blocks::fit_true_peak_stored(
                        &mut pcm,
                        spec.ceiling_dbtp,
                        spec.true_peak,
                        rate,
                    )?;
                    let mut meter = dsp::LoudnessMeter::new(rate);
                    let blocks =
                        crate::analysis::blocks_read_with(&mut pcm, rate, |span| meter.push(span))?;
                    let loss = prepared
                        .tone_temporal
                        .as_ref()
                        .and_then(|t| crate::analysis::energy_attack_loss_p90_of(t, &blocks));
                    let lufs = meter.finish().ok_or("Missing output loudness")?;
                    (
                        limiter,
                        clipping,
                        limiter_trace,
                        peak_estimate_dbtp,
                        true_peak,
                        lufs,
                        loss,
                    )
                } else {
                    let (mut audio, limiter, clipping, limiter_trace) =
                        crate::render_blocks::render_peak_control(
                            &mut prepared.frames(rate).1,
                            drive,
                            spec.clipper_db,
                            dsp::gain(spec.ceiling_dbtp - 0.1),
                            spec.limiter,
                            rate,
                        )?;
                    if audio.iter().flatten().any(|x| !x.is_finite()) {
                        return Err("Nonfinite rendered samples".into());
                    }
                    let (peak_estimate_dbtp, true_peak) = render_blocks::fit_true_peak(
                        &mut audio,
                        spec.ceiling_dbtp,
                        spec.true_peak,
                        rate,
                    );
                    let lufs = dsp::loudness(&audio, rate).ok_or("Missing output loudness")?;
                    let loss = prepared.tone_temporal.as_ref().and_then(|t| {
                        crate::analysis::energy_attack_loss_p90_from(t, &audio, rate)
                    });
                    for block in audio.chunks(8192) {
                        pcm.push(block)?;
                    }
                    (
                        limiter,
                        clipping,
                        limiter_trace,
                        peak_estimate_dbtp,
                        true_peak,
                        lufs,
                        loss,
                    )
                };
            let bytes = crate::scratch::Frames::len(&pcm) * std::mem::size_of::<Stereo>();
            let energy_attack_loss_p90_db = loss;
            let report = Rendered {
                audio: RenderAudio::default(),
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
                drive_response: None,
            };
            let result = Stored {
                report,
                pcm: Arc::new(Mutex::new(pcm)),
            };
            if prepared.probe_bytes + bytes <= work.probe_cache_budget_bytes {
                prepared.probes.push((key, result.clone()));
                prepared.probe_bytes += bytes;
            }
            Ok((lufs, result))
        };
        if let Some(drive) = spec.fixed_drive_db {
            return render(drive)?.1.load(stored);
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
        let mut output = output.load(stored)?;
        output.search = Some(search);
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Kicks, tones, a sibilant burst every second and wide noise, `frames`
    /// frames at 48 kHz.
    fn material(frames: usize) -> Vec<Stereo> {
        let mut state = 0x9e37_79b9u32;
        let mut noise = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state as f64 / u32::MAX as f64) * 2.0 - 1.0
        };
        (0..frames)
            .map(|i| {
                let t = i as f64 / 48000.0;
                let since_hit = (i % 24_000) as f64 / 48000.0;
                let kick =
                    0.6 * (-since_hit / 0.06).exp() * (std::f64::consts::TAU * 55.0 * t).sin();
                let level = 0.5 + 0.4 * (std::f64::consts::TAU * 0.07 * t).sin();
                let body = level * 0.1 * (std::f64::consts::TAU * 1500.0 * t).sin();
                let ess = if i % 48_000 < 6_000 {
                    0.15 * (std::f64::consts::TAU * 7000.0 * t).sin()
                } else {
                    0.0
                };
                let (a, b) = (0.05 * noise(), 0.05 * noise());
                [
                    (kick + body + ess + a) as f32,
                    (kick + 0.8 * body + ess + b) as f32,
                ]
            })
            .collect()
    }

    fn bits(values: impl IntoIterator<Item = f64>) -> Vec<u64> {
        values.into_iter().map(f64::to_bits).collect()
    }

    fn frames_of(pcm: &mut crate::scratch::Pcm) -> Vec<Stereo> {
        use crate::scratch::Frames;
        let mut frames = Vec::new();
        let count = pcm.len();
        pcm.read(0, count, &mut frames).unwrap();
        frames
    }

    /// The streamed prepare of `spec` on `input` equals `shape` and the
    /// whole-signal outputs of `prepare`, bit for bit.
    fn assert_streamed_prepare_is_exact(
        spec: &Spec,
        input: &[Stereo],
    ) -> (Option<crate::deesser::Stats>, Option<compressor::Stats>) {
        let mut renderer = Renderer::with_scratch(input, 48000, Box::new(crate::scratch::memory));
        let (tone, envelopes) = renderer.validated(spec).unwrap();
        let whole = renderer.shape(spec, &tone, &envelopes).unwrap();
        let mut streamed = renderer.prepare_streamed(spec, &tone, &envelopes).unwrap();
        let flat = |frames: &[Stereo]| bits(frames.iter().flatten().map(|&v| v as f64));
        assert_eq!(flat(&frames_of(&mut streamed.base)), flat(&whole.audio));
        let mut up = crate::scratch::Pcm::new(crate::scratch::memory().unwrap());
        render_blocks::upsample_into(&whole.audio, &mut up, 48000).unwrap();
        assert_eq!(
            flat(&frames_of(streamed.up.as_mut().unwrap())),
            flat(&frames_of(&mut up))
        );
        assert_eq!(
            streamed.lufs.map(f64::to_bits),
            dsp::loudness(&whole.audio, 48000).map(f64::to_bits)
        );
        let windows = |w: &WindowBands| {
            let mut all = bits(w.bands.iter().flatten().copied());
            all.extend(bits(w.weighted.iter().copied()));
            all.extend(bits(w.weighted_bands));
            all
        };
        assert_eq!(windows(&streamed.windows), windows(&whole.windows));
        let temporal = |t: &crate::analysis::Temporal| {
            (
                serde_json::to_string(t).unwrap(),
                bits(t.blocks.iter().flat_map(|b| [b.energy, b.peak])),
                t.activity_floor.to_bits(),
            )
        };
        assert_eq!(
            temporal(&streamed.tone_temporal),
            temporal(whole.tone_temporal.as_ref().unwrap())
        );
        assert_eq!(
            serde_json::to_string(&streamed.compressor).unwrap(),
            serde_json::to_string(&whole.compressor).unwrap()
        );
        assert_eq!(
            serde_json::to_string(&streamed.deesser).unwrap(),
            serde_json::to_string(&whole.deesser).unwrap()
        );
        (whole.deesser, whole.compressor)
    }

    fn bypass_spec() -> Spec {
        Spec {
            phase_alignment: None,
            finishing: finishing::Plan {
                rule: "test",
                validation_status: "test",
                active: false,
                dc_filter_hz: None,
                bands: Vec::new(),
                section_gain: Vec::new(),
                color_amount: 0.0,
                clipping_budget_db: 0.0,
            },
            tone: treatment::Tone::default(),
            focus_envelopes: Vec::new(),
            side_highpass_hz: None,
            side_gain: None,
            deesser: None,
            low_mid: None,
            low_end_hold: None,
            mono_safe: Vec::new(),
            dynamic_after_eq: Vec::new(),
            compressor: None,
            limiter: dsp::LimiterSettings::default(),
            clipper_db: 0.0,
            ceiling_dbtp: -1.0,
            initial_drive_db: 6.0,
            target_lufs: -10.0,
            fixed_drive_db: Some(6.0),
            restore_peak_tone: false,
            restoration_db: [0.0; 6],
            true_peak: dsp::TruePeak::Trim,
        }
    }

    fn band(kind: dsp::Kind, hz: f64, q: f64, gain_db: f64) -> dsp::EqBand {
        dsp::EqBand {
            kind,
            hz,
            q,
            gain_db,
        }
    }

    /// Every stage of the streamed prefix.
    fn full_spec(frames: usize) -> Spec {
        let point = |sample: usize, wet: f64| treatment::BlendPoint { sample, wet };
        let mut spec = bypass_spec();
        spec.phase_alignment = Some(crate::stereo_phase::Alignment {
            points: vec![(200.0, 30.0), (2000.0, 150.0), (8000.0, 120.0)],
        });
        spec.tone = treatment::Tone {
            stereo: vec![
                band(dsp::Kind::LowShelf, 120.0, 0.7, 2.0),
                band(dsp::Kind::Peak, 3000.0, 1.2, -1.5),
            ],
            mid: vec![band(dsp::Kind::Peak, 400.0, 1.0, 1.0)],
            side: vec![band(dsp::Kind::HighShelf, 6000.0, 0.7, 1.5)],
        };
        spec.focus_envelopes = vec![treatment::FocusEnvelope {
            domain: finishing::Domain::Stereo,
            filter_index: 1,
            points: vec![
                point(0, 0.0),
                point(frames / 3, 1.0),
                point(frames - 1, 0.4),
            ],
        }];
        spec.side_gain = Some(crate::width::SideGain {
            bands_db: [0.0, 1.0, 2.0, 2.0, 1.5, 1.0],
            decorrelated_db: [None, None, Some(-12.0), Some(-10.0), None, None],
            mid_db: [0.0, 0.0, 0.0, 0.5, 0.0, 0.0],
        });
        spec.mono_safe = vec![treatment::EnvelopedBand {
            rule: crate::width::MONO_SAFE_RULE.into(),
            domain: finishing::Domain::Side,
            band: band(dsp::Kind::Peak, 3464.1, 0.87, -6.0),
            points: vec![
                point(0, 0.0),
                point(frames / 2, 1.0),
                point(frames - 1, 1.0),
            ],
        }];
        spec.deesser = Some(crate::deesser::Plan {
            rule: "test".into(),
            keyed: (0..(frames / crate::masking::HOP) as u32)
                .step_by(3)
                .map(|frame| crate::deesser::Keyed {
                    frame,
                    vocal_share: 1.0,
                    other_power: 0.0,
                })
                .collect(),
            other_band_power: 1.0,
            maximum_cut_db: crate::deesser::MAXIMUM_CUT_DB,
            threshold_db: 0.0,
            reference_excess_db: None,
        });
        spec.side_highpass_hz = Some(90.0);
        spec.restoration_db = [0.5, 0.0, -0.4, 0.3, 0.0, 0.6];
        spec.compressor = Some(compressor::Settings {
            threshold_dbfs: -6.0,
            ratio: 2.0,
            knee_db: 6.0,
            attack_ms: 10.0,
            release_ms: 120.0,
            detector_rms_ms: 5.0,
            sidechain_highpass_hz: 80.0,
            maximum_reduction_db: 6.0,
            section_ms: 3000.0,
            section_makeup: true,
        });
        spec
    }

    #[test]
    fn the_streamed_prefix_equals_the_whole_prefix() {
        // Two and a half spans, and a recording shorter than one span.
        for frames in [2 * render_blocks::UPSAMPLE_SPAN + 262_147, 160_003] {
            let input = material(frames);
            let full = full_spec(frames);
            let (deesser, compressor) = assert_streamed_prepare_is_exact(&full, &input);
            assert!(deesser.unwrap().cut_frames > 0);
            let compressor = compressor.unwrap();
            assert!(compressor.maximum_reduction_db > 1.0 && compressor.maximum_makeup_db > 0.0);
            // The tone alone, with no envelope.
            let mut plain = bypass_spec();
            plain.tone = full.tone.clone();
            plain.restoration_db = full.restoration_db;
            assert_streamed_prepare_is_exact(&plain, &input);
            // A de-esser whose reference cuts no frame, a compressor with no
            // make-up, and a side gain with no decorrelated mid.
            let mut variant = full.clone();
            variant.phase_alignment = None;
            variant.focus_envelopes.clear();
            variant.side_gain = Some(crate::width::SideGain::gains([
                0.0, 1.0, 2.0, 2.0, 1.0, 0.0,
            ]));
            if let Some(plan) = &mut variant.deesser {
                plan.reference_excess_db = Some(60.0);
            }
            if let Some(settings) = &mut variant.compressor {
                settings.section_makeup = false;
            }
            let (deesser, compressor) = assert_streamed_prepare_is_exact(&variant, &input);
            assert_eq!(deesser.unwrap().cut_frames, 0);
            assert!(compressor.unwrap().maximum_reduction_db > 1.0);
            // A compressor that leaves the signal, and no de-esser.
            variant.deesser = None;
            if let Some(settings) = &mut variant.compressor {
                settings.ratio = 1.0;
            }
            assert_streamed_prepare_is_exact(&variant, &input);
        }
    }

    /// A renderer that keeps no 4x frames of its prefix gives the 4x
    /// frames and the render of a renderer that keeps them (D251).
    #[test]
    fn a_prefix_without_its_4x_frames_gives_the_same_render() {
        use crate::scratch::Frames;
        // Longer than one segment, so peak control renders in parallel
        // segments.
        let frames = render_blocks::segment(48000) / 4 + 100_003;
        let input = material(frames);
        let spec = full_spec(frames);
        let render = |upsampled_min_frames: usize| {
            let mut renderer =
                Renderer::with_scratch(&input, 48000, Box::new(crate::scratch::temporary));
            renderer.upsampled_min_frames = upsampled_min_frames;
            let rendered = renderer.render(&spec).unwrap();
            let (_, mut up) = renderer.prepared_frames(&spec).unwrap();
            let mut frames = Vec::new();
            let count = up.len();
            up.read(0, count, &mut frames).unwrap();
            let kept = matches!(up, render_blocks::Up::Stored(_));
            (rendered, frames, kept)
        };
        let (stored, stored_up, kept) = render(usize::MAX);
        let (upsampled, upsampled_up, upsampled_kept) = render(0);
        assert!(kept && !upsampled_kept);
        let flat = |frames: &[Stereo]| bits(frames.iter().flatten().map(|&v| v as f64));
        assert_eq!(flat(&upsampled_up), flat(&stored_up));
        assert_eq!(
            flat(&upsampled.audio.load().unwrap()),
            flat(&stored.audio.load().unwrap())
        );
        assert_eq!(
            serde_json::to_value(&upsampled).unwrap(),
            serde_json::to_value(&stored).unwrap()
        );
    }

    /// The audibility point of the prepared prefix of a lent source, with
    /// the temporal analysis of the source in blocks, is the point of the
    /// whole prefix (D257).
    #[test]
    fn the_point_of_a_prepared_prefix_is_the_point_of_the_whole_prefix() {
        let frames = render_blocks::segment(48000) / 4 + 100_003;
        let input = material(frames);
        let spec = full_spec(frames);
        let whole = crate::audibility::point(
            &Renderer::new(&input, 48000).prefix(&spec).unwrap(),
            3.0,
            spec.clipper_db,
            spec.ceiling_dbtp,
            spec.limiter,
            spec.true_peak,
            Some(&crate::analysis::analyze(&input, 48000)),
            48000,
        )
        .unwrap();
        let mut stored = crate::audio_io::StoredSource::from_blocks(|each| {
            for block in input.chunks(65_536) {
                each(block)?;
            }
            Ok(48000)
        })
        .unwrap();
        let mut source = RenderSource::Lent(&mut stored);
        let mut scan = crate::analysis::Scan::new(frames, 48000);
        crate::scratch::for_each_block(&mut source.frames(), |block| {
            scan.push(block);
            Ok(())
        })
        .unwrap();
        let temporal = scan.finish();
        let mut renderer =
            Renderer::of_source(source.lend(), 48000, Box::new(crate::scratch::temporary)).unwrap();
        let (prefix, mut up) = renderer.prepared_frames(&spec).unwrap();
        let prepared = crate::audibility::point_on(
            &mut crate::audibility::Prefix::Shared(crate::scratch::Shared(prefix)),
            &mut up,
            3.0,
            spec.clipper_db,
            spec.ceiling_dbtp,
            spec.limiter,
            spec.true_peak,
            Some(&temporal),
            48000,
        )
        .unwrap();
        assert!(whole.is_some());
        assert_eq!(
            serde_json::to_value(&prepared).unwrap(),
            serde_json::to_value(&whole).unwrap()
        );
    }

    /// A render of a long recording stays in its scratch file from its peak
    /// control. Its frames, its report and the measures of the candidate
    /// are those of the render in memory, with the drive search, the
    /// restoring passes and the local true-peak correction (D253).
    #[test]
    fn a_stored_render_gives_the_render_in_memory() {
        // Longer than one segment, so peak control renders in parallel
        // segments.
        let frames = render_blocks::segment(48000) / 4 + 100_003;
        let input = material(frames);
        let mut spec = full_spec(frames);
        spec.fixed_drive_db = None;
        spec.restore_peak_tone = true;
        spec.true_peak = dsp::TruePeak::Local;
        let source = crate::pipeline::analyze_source(&input, 48000).unwrap();
        let render = |stored_render_min_frames: usize| {
            let mut renderer =
                Renderer::with_scratch(&input, 48000, Box::new(crate::scratch::temporary));
            renderer.stored_render_min_frames = stored_render_min_frames;
            let rendered = renderer.render(&spec).unwrap();
            let stored = rendered.audio.is_stored();
            let frames = rendered.audio.load().unwrap();
            let report = serde_json::to_value(&rendered).unwrap();
            let measured = crate::selection::measure(&source, rendered).unwrap();
            let measures = serde_json::json!([
                measured.output,
                measured.output_stereo,
                measured.preservation,
                measured.gain_span_db,
                measured.low_sections,
            ]);
            (frames, report, measures, stored)
        };
        let (held, held_report, held_measures, held_stored) = render(usize::MAX);
        let (stored, stored_report, stored_measures, stored_stored) = render(0);
        assert!(!held_stored && stored_stored);
        assert!(held_report["restoration"]["renders"].as_u64().unwrap() > 1);
        let flat = |frames: &[Stereo]| bits(frames.iter().flatten().map(|&v| v as f64));
        assert_eq!(flat(&stored), flat(&held));
        assert_eq!(stored_report, held_report);
        assert_eq!(stored_measures, held_measures);
    }

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
