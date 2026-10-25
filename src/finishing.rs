use crate::{
    audio_io::{Result, Stereo},
    dsp::{self, Biquad, EqBand, Kind},
};
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Domain {
    Stereo,
    Mid,
    Side,
}

/// Filter `audio`, at `rate`, with `bands` in `domain`.
pub fn eq_domain(audio: &mut [Stereo], bands: &[EqBand], domain: Domain, rate: u32) {
    match domain {
        Domain::Stereo => dsp::eq(audio, bands, rate),
        Domain::Side => dsp::side_eq(audio, bands, rate),
        Domain::Mid => {
            let mut filters: Vec<_> = bands.iter().map(|band| band.filter(rate)).collect();
            for frame in audio {
                let mut mid = (frame[0] as f64 + frame[1] as f64) * 0.5;
                let side = (frame[0] as f64 - frame[1] as f64) * 0.5;
                for filter in &mut filters {
                    mid = filter.tick(mid);
                }
                *frame = [(mid + side) as f32, (mid - side) as f32];
            }
        }
    }
}

pub(crate) fn bandpass(hz: f64, q: f64, rate: u32) -> Biquad {
    let w = 2.0 * PI * hz / rate as f64;
    let a = w.sin() / (2.0 * q);
    Biquad::coefficients(
        [a / (1.0 + a), 0.0, -a / (1.0 + a)],
        [-2.0 * w.cos() / (1.0 + a), (1.0 - a) / (1.0 + a)],
    )
}

#[derive(Clone, Copy, Serialize)]
pub enum Detector {
    RelativeBandPower,
    AbsoluteBandPower,
}

#[derive(Clone, Serialize)]
pub struct Band {
    pub id: &'static str,
    pub hz: f64,
    pub q: f64,
    pub domain: Domain,
    pub ratio_p50_p95_db: [f64; 2],
    pub level_p50_p95_dbfs: [f64; 2],
    pub detector: Detector,
    pub active_blocks: usize,
    pub static_cut_db: f64,
    pub dynamic_threshold_db: f64,
    pub maximum_dynamic_cut_db: f64,
    pub reason: &'static str,
}

#[derive(Clone, Serialize)]
pub struct GainPoint {
    pub sample: usize,
    pub gain_db: f64,
}

#[derive(Clone, Serialize)]
pub struct Plan {
    pub rule: &'static str,
    pub validation_status: &'static str,
    pub active: bool,
    pub dc_filter_hz: Option<f64>,
    pub bands: Vec<Band>,
    pub section_gain: Vec<GainPoint>,
    pub color_amount: f64,
    pub clipping_budget_db: f64,
}

#[derive(Clone, Default, Debug, Serialize)]
pub struct BandStats {
    pub id: &'static str,
    pub maximum_reduction_db: f64,
    pub mean_reduction_db: f64,
}

#[derive(Clone, Default, Debug, Serialize)]
pub struct Stats {
    pub dc_filter_applied: bool,
    pub maximum_section_gain_db: f64,
    pub minimum_section_gain_db: f64,
    pub static_cuts: usize,
    pub dynamic_bands: Vec<BandStats>,
}

pub fn automate_gain(audio: &mut [Stereo], points: &[GainPoint]) -> Result<()> {
    if points.is_empty() {
        return Ok(());
    }
    if points
        .iter()
        .any(|p| !p.gain_db.is_finite() || p.gain_db.abs() > 0.25 || p.sample >= audio.len())
        || points.windows(2).any(|w| w[0].sample >= w[1].sample)
    {
        return Err("Invalid bounded section-gain points".into());
    }
    let mut segment = 0;
    for (i, frame) in audio.iter_mut().enumerate() {
        while segment + 1 < points.len() && points[segment + 1].sample <= i {
            segment += 1;
        }
        let a = &points[segment];
        let gain_db = if let Some(b) = points.get(segment + 1) {
            let t =
                (i.saturating_sub(a.sample) as f64 / (b.sample - a.sample) as f64).clamp(0.0, 1.0);
            a.gain_db + (b.gain_db - a.gain_db) * t
        } else {
            a.gain_db
        };
        let g = dsp::gain(gain_db) as f32;
        frame[0] *= g;
        frame[1] *= g;
    }
    Ok(())
}

/// Apply the dynamic cut of `band` to `audio` at `rate`.
pub fn dynamic_band(audio: &mut [Stereo], band: &Band, rate: u32) -> Result<BandStats> {
    if !band.hz.is_finite()
        || !(20.0..=18000.0).contains(&band.hz)
        || !(0.3..=4.0).contains(&band.q)
        || !(0.0..=3.0).contains(&band.maximum_dynamic_cut_db)
        || !band.dynamic_threshold_db.is_finite()
        || audio.iter().flatten().any(|x| !x.is_finite())
    {
        return Err("Invalid dynamic band".into());
    }
    let mut stats = BandStats {
        id: band.id,
        ..BandStats::default()
    };
    if band.maximum_dynamic_cut_db == 0.0 {
        return Ok(stats);
    }
    let mut filters = [bandpass(band.hz, band.q, rate); 2];
    let (smoothing, attack_seconds, release_seconds) = match band.detector {
        Detector::RelativeBandPower => (0.03, 0.02, 0.12),
        Detector::AbsoluteBandPower => (0.005, 0.005, 0.08),
    };
    let rate = rate as f64;
    let power_smooth = (-1.0f64 / (rate * smoothing)).exp();
    let attack = (-1.0f64 / (rate * attack_seconds)).exp();
    let release = (-1.0f64 / (rate * release_seconds)).exp();
    let (mut full_power, mut band_power, mut reduction) = (0.0f64, 0.0f64, 0.0f64);
    let n = audio.len().max(1);
    for frame in audio {
        let original = frame.map(f64::from);
        let mid = (original[0] + original[1]) * 0.5;
        let side = (original[0] - original[1]) * 0.5;
        let input = match band.domain {
            Domain::Stereo => original,
            Domain::Mid => [mid, mid],
            Domain::Side => [side, -side],
        };
        let filtered = [filters[0].tick(input[0]), filters[1].tick(input[1])];
        full_power = power_smooth * full_power
            + (1.0 - power_smooth) * input[0].powi(2).max(input[1].powi(2));
        band_power = power_smooth * band_power
            + (1.0 - power_smooth) * filtered[0].powi(2).max(filtered[1].powi(2));
        let detector_db = match band.detector {
            Detector::RelativeBandPower => {
                10.0 * (band_power.max(1e-30) / full_power.max(1e-30)).log10()
            }
            Detector::AbsoluteBandPower => 10.0 * band_power.max(1e-30).log10(),
        };
        let target = if full_power > 1e-10 {
            ((detector_db - band.dynamic_threshold_db) * 0.5)
                .clamp(0.0, band.maximum_dynamic_cut_db)
        } else {
            0.0
        };
        let a = if target > reduction { attack } else { release };
        reduction = a * reduction + (1.0 - a) * target;
        let change = dsp::gain(-reduction) - 1.0;
        for c in 0..2 {
            frame[c] = (original[c] + filtered[c] * change) as f32;
        }
        stats.maximum_reduction_db = stats.maximum_reduction_db.max(reduction);
        stats.mean_reduction_db += reduction / n as f64;
    }
    Ok(stats)
}

/// Apply `plan` to `audio` at `rate`.
pub fn process(audio: &mut [Stereo], plan: &Plan, rate: u32) -> Result<Stats> {
    let mut stats = Stats::default();
    if !plan.active {
        return Ok(stats);
    }
    if let Some(hz) = plan.dc_filter_hz {
        let k = (PI * hz / rate as f64).tan();
        let mut filters = [Biquad::coefficients(
            [1.0 / (1.0 + k), -1.0 / (1.0 + k), 0.0],
            [(k - 1.0) / (k + 1.0), 0.0],
        ); 2];
        for frame in audio.iter_mut() {
            for c in 0..2 {
                frame[c] = filters[c].tick(frame[c] as f64) as f32;
            }
        }
        stats.dc_filter_applied = true;
    }
    automate_gain(audio, &plan.section_gain)?;
    for point in &plan.section_gain {
        stats.maximum_section_gain_db = stats.maximum_section_gain_db.max(point.gain_db);
        stats.minimum_section_gain_db = stats.minimum_section_gain_db.min(point.gain_db);
    }
    for band in &plan.bands {
        if band.static_cut_db != 0.0 {
            eq_domain(
                audio,
                &[EqBand {
                    kind: Kind::Peak,
                    hz: band.hz,
                    q: band.q,
                    gain_db: band.static_cut_db,
                }],
                band.domain,
                rate,
            );
            stats.static_cuts += 1;
        }
        if band.maximum_dynamic_cut_db > 0.0 {
            stats.dynamic_bands.push(dynamic_band(audio, band, rate)?);
        }
    }
    Ok(stats)
}

#[derive(Clone, Copy, Default, Debug, Serialize)]
pub struct PeakStats {
    pub maximum_reduction_db: f64,
    pub affected_frames_percent: f64,
}

pub fn color(audio: &mut [Stereo], amount: f64) -> Result<PeakStats> {
    if !(0.0..=0.05).contains(&amount) || audio.iter().flatten().any(|x| !x.is_finite()) {
        return Err("Invalid harmonic color input".into());
    }
    if amount == 0.0 {
        return Ok(PeakStats::default());
    }
    let peak = audio
        .iter()
        .flatten()
        .map(|x| x.abs() as f64)
        .fold(0.0, f64::max);
    if peak == 0.0 {
        return Ok(PeakStats::default());
    }
    let mut affected = 0;
    let mut stats = PeakStats::default();
    for frame in audio.iter_mut() {
        let mut changed = false;
        for x in frame {
            let factor = 1.0 - amount * (*x as f64 / peak).powi(2);
            stats.maximum_reduction_db = stats.maximum_reduction_db.max(-dsp::db(factor));
            changed |= factor < 1.0 - 1e-6;
            *x = (*x as f64 * factor) as f32;
        }
        affected += usize::from(changed);
    }
    stats.affected_frames_percent = 100.0 * affected as f64 / audio.len().max(1) as f64;
    Ok(stats)
}

/// Ceiling-relative soft clipping. Peaks up to `budget_db` above the ceiling
/// map onto the ceiling; the limiter must already bound the signal there.
pub fn clip(audio: &mut [Stereo], ceiling: f64, budget_db: f64) -> Result<PeakStats> {
    if !((0.0..=6.0).contains(&budget_db) && ceiling.is_finite() && ceiling > 0.0)
        || audio.iter().flatten().any(|x| !x.is_finite())
    {
        return Err("Invalid clipping input".into());
    }
    let Some(clipper) = dsp::SoftClip::new(ceiling, budget_db) else {
        return Ok(PeakStats::default());
    };
    let mut stats = PeakStats::default();
    let mut affected = 0;
    for frame in audio.iter_mut() {
        let mut changed = false;
        for x in frame {
            let (shaped, reduction) = clipper.shape(*x as f64);
            stats.maximum_reduction_db = stats.maximum_reduction_db.max(reduction);
            changed |= reduction > 0.0;
            *x = shaped as f32;
        }
        affected += usize::from(changed);
    }
    stats.affected_frames_percent = 100.0 * affected as f64 / audio.len().max(1) as f64;
    Ok(stats)
}
