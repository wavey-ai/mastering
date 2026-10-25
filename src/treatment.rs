//! Explicit treatments: the tone of a version, its enveloped corrections
//! and its dynamic bands, on the DSP of the mastering chain.
use crate::{
    analysis::percentile,
    audio_io::{Result, Stereo},
    dsp::{self, EqBand},
    finishing::{self, Band, Detector, Domain},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct BlendPoint {
    pub sample: usize,
    pub wet: f64,
}

/// Slow, source-derived focus movement. Filter state runs continuously, including
/// dry passages. Mixing dry/filtered audio avoids changing recursive coefficients.
#[derive(Clone, Serialize, Deserialize)]
pub struct FocusEnvelope {
    pub domain: Domain,
    pub filter_index: usize,
    pub points: Vec<BlendPoint>,
}

impl FocusEnvelope {
    pub fn validate(&self, tone: &Tone, frames: usize) -> Result<()> {
        let bands = match self.domain {
            Domain::Stereo => &tone.stereo,
            Domain::Mid => &tone.mid,
            Domain::Side => &tone.side,
        };
        if self.filter_index >= bands.len()
            || self.points.len() < 2
            || self.points[0].sample != 0
            || self.points.last().unwrap().sample != frames.saturating_sub(1)
            || self.points.iter().any(|p| !(0.0..=1.0).contains(&p.wet))
            || self.points.windows(2).any(|p| p[1].sample <= p[0].sample)
        {
            return Err("Invalid source focus envelope".into());
        }
        Ok(())
    }
}

/// One filter whose wet share follows an envelope: a correction that the
/// engine measures once and applies only where its condition holds.
#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct EnvelopedBand {
    pub rule: String,
    pub domain: Domain,
    pub band: EqBand,
    pub points: Vec<BlendPoint>,
}

impl EnvelopedBand {
    /// Mean wet share over samples `start..end`, from the points.
    pub fn mean_wet(&self, start: usize, end: usize) -> f64 {
        let step = ((end - start) / 256).max(1);
        let samples: Vec<f64> = (start..end).step_by(step).map(|s| self.wet_at(s)).collect();
        samples.iter().sum::<f64>() / samples.len().max(1) as f64
    }

    /// The wet share at `sample`: a raised cosine between the points, as
    /// `Tone::apply_with_envelopes` applies it.
    pub fn wet_at(&self, sample: usize) -> f64 {
        if self.points.len() < 2 {
            return self.points.first().map_or(1.0, |p| p.wet);
        }
        let i = self
            .points
            .partition_point(|p| p.sample <= sample)
            .clamp(1, self.points.len() - 1);
        let (a, b) = (&self.points[i - 1], &self.points[i]);
        let position =
            (sample.saturating_sub(a.sample)) as f64 / (b.sample - a.sample).max(1) as f64;
        let smooth = 0.5 - 0.5 * (std::f64::consts::PI * position.clamp(0.0, 1.0)).cos();
        a.wet + smooth * (b.wet - a.wet)
    }
}

/// The tone with the enveloped corrections appended to their domains, and
/// the envelopes of all of them.
pub fn with_enveloped(
    tone: &Tone,
    envelopes: &[FocusEnvelope],
    bands: &[&EnvelopedBand],
) -> (Tone, Vec<FocusEnvelope>) {
    let mut tone = tone.clone();
    let mut envelopes = envelopes.to_vec();
    for b in bands {
        let domain = match b.domain {
            Domain::Stereo => &mut tone.stereo,
            Domain::Mid => &mut tone.mid,
            Domain::Side => &mut tone.side,
        };
        domain.push(b.band);
        envelopes.push(FocusEnvelope {
            domain: b.domain,
            filter_index: domain.len() - 1,
            points: b.points.clone(),
        });
    }
    (tone, envelopes)
}

/// Largest number of filters in one domain of a tone.
pub const MAXIMUM_FILTERS: usize = 12;

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tone {
    pub stereo: Vec<EqBand>,
    pub mid: Vec<EqBand>,
    pub side: Vec<EqBand>,
}

impl Tone {
    /// Apply the tone to `audio` at `rate`, each enveloped filter at the wet
    /// share of its envelope.
    pub fn apply_with_envelopes(
        &self,
        audio: &mut [Stereo],
        envelopes: &[FocusEnvelope],
        rate: u32,
    ) -> Result<()> {
        let frames = audio.len();
        self.apply_with_envelopes_from(audio, envelopes, 0, frames, rate)
    }

    /// `apply_with_envelopes` on the span of a recording of `frames` frames
    /// that starts at frame `offset`: the envelopes take their wet share at
    /// the position in the recording.
    pub fn apply_with_envelopes_from(
        &self,
        audio: &mut [Stereo],
        envelopes: &[FocusEnvelope],
        offset: usize,
        frames: usize,
        rate: u32,
    ) -> Result<()> {
        self.validate(rate)?;
        if envelopes.is_empty() {
            return self.apply(audio, rate);
        }
        if offset + audio.len() > frames {
            return Err("Treatment span outside the recording".into());
        }
        for (i, envelope) in envelopes.iter().enumerate() {
            envelope.validate(self, frames)?;
            if envelopes[..i].iter().any(|other| {
                std::mem::discriminant(&other.domain) == std::mem::discriminant(&envelope.domain)
                    && other.filter_index == envelope.filter_index
            }) {
                return Err("Repeated source focus envelope".into());
            }
        }
        if audio.iter().flatten().any(|x| !x.is_finite()) {
            return Err("Nonfinite treatment input".into());
        }
        for (domain, bands) in [
            (Domain::Stereo, &self.stereo),
            (Domain::Mid, &self.mid),
            (Domain::Side, &self.side),
        ] {
            for (index, band) in bands.iter().enumerate() {
                let envelope = envelopes.iter().find(|e| {
                    std::mem::discriminant(&e.domain) == std::mem::discriminant(&domain)
                        && e.filter_index == index
                });
                let mut filters = [band.filter(rate), band.filter(rate)];
                let mut segment = 0;
                for (index, frame) in audio.iter_mut().enumerate() {
                    let sample = offset + index;
                    let wet = envelope.map_or(1.0, |e| {
                        while segment + 2 < e.points.len() && sample > e.points[segment + 1].sample
                        {
                            segment += 1;
                        }
                        let a = &e.points[segment];
                        let b = &e.points[segment + 1];
                        let position = (sample - a.sample) as f64 / (b.sample - a.sample) as f64;
                        let smooth = 0.5 - 0.5 * (std::f64::consts::PI * position).cos();
                        a.wet + smooth * (b.wet - a.wet)
                    });
                    let [l, r] = frame.map(f64::from);
                    let blend = |dry: f64, processed: f64| {
                        if wet == 1.0 {
                            processed
                        } else {
                            dry + wet * (processed - dry)
                        }
                    };
                    let mid = (l + r) * 0.5;
                    let side = (l - r) * 0.5;
                    let output = match domain {
                        Domain::Stereo => {
                            [blend(l, filters[0].tick(l)), blend(r, filters[1].tick(r))]
                        }
                        Domain::Mid => {
                            let m = blend(mid, filters[0].tick(mid));
                            [m + side, m - side]
                        }
                        Domain::Side => {
                            let s = blend(side, filters[0].tick(side));
                            [mid + s, mid - s]
                        }
                    };
                    if wet != 0.0 {
                        *frame = output.map(|x| x as f32);
                    }
                }
            }
        }
        Ok(())
    }

    /// Check the filter count, each filter, and the response of each domain
    /// at `rate`.
    pub fn validate(&self, rate: u32) -> Result<()> {
        for bands in [&self.stereo, &self.mid, &self.side] {
            // The stereo domain of a held version carries up to three broad
            // moves, four restoring filters and three hold bells.
            if bands.len() > MAXIMUM_FILTERS {
                return Err("Too many explicit filters in one domain".into());
            }
            for b in bands {
                if matches!(b.kind, dsp::Kind::HighPass)
                    || !(20.0..=18000.0).contains(&b.hz)
                    || !(0.3..=4.0).contains(&b.q)
                    || !(-6.0..=6.0).contains(&b.gain_db)
                {
                    return Err("Invalid experimental filter".into());
                }
            }
            for i in 0..256 {
                let hz = 20.0 * 1000f64.powf(i as f64 / 255.0);
                let response: f64 = bands
                    .iter()
                    .map(|b| b.filter(rate).response_db(hz, rate))
                    .sum();
                if !response.is_finite() || response.abs() > 9.0 {
                    return Err("Experimental domain response exceeds 9 dB".into());
                }
            }
        }
        Ok(())
    }

    /// Apply the tone to `audio` at `rate`.
    pub fn apply(&self, audio: &mut [Stereo], rate: u32) -> Result<()> {
        self.validate(rate)?;
        if audio.iter().flatten().any(|x| !x.is_finite()) {
            return Err("Nonfinite treatment input".into());
        }
        dsp::eq(audio, &self.stereo, rate);
        if !self.mid.is_empty() {
            finishing::eq_domain(audio, &self.mid, Domain::Mid, rate);
        }
        dsp::side_eq(audio, &self.side, rate);
        Ok(())
    }
}

#[derive(Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DynamicProbe {
    pub hz: f64,
    pub q: f64,
    pub domain: Domain,
    pub maximum_cut_db: f64,
    pub threshold_percentile: f64,
    pub threshold_offset_db: f64,
}

/// Calibrate the exact existing absolute band-power detector on the signal at
/// the insertion point. Its 5 ms smoothing and max-channel linkage match DSP.
/// The percentile and the offset are explicit parameters of the probe.
/// `audio` is at `rate`.
pub fn measure_dynamic(audio: &[Stereo], spec: DynamicProbe, rate: u32) -> Result<Band> {
    if !(20.0..=18000.0).contains(&spec.hz)
        || !(0.3..=4.0).contains(&spec.q)
        || !(0.0..=3.0).contains(&spec.maximum_cut_db)
        || !(0.5..=0.99).contains(&spec.threshold_percentile)
        || !(-12.0..=0.0).contains(&spec.threshold_offset_db)
        || audio.iter().flatten().any(|v| !v.is_finite())
    {
        return Err("Invalid dynamic treatment probe".into());
    }
    let mut filters = [finishing::bandpass(spec.hz, spec.q, rate); 2];
    let smoothing = (-1.0f64 / (rate as f64 * 0.005)).exp();
    // The 10 ms blocks of the detector level.
    let block = rate as usize / 100;
    let (mut full, mut power) = (0.0f64, 0.0f64);
    let mut levels = Vec::new();
    let mut ratios = Vec::new();
    for (i, frame) in audio.iter().enumerate() {
        let [l, r] = frame.map(f64::from);
        let signal = match spec.domain {
            Domain::Stereo => [l, r],
            Domain::Mid => [(l + r) * 0.5; 2],
            Domain::Side => [(l - r) * 0.5, (r - l) * 0.5],
        };
        let band = [filters[0].tick(signal[0]), filters[1].tick(signal[1])];
        full = smoothing * full + (1.0 - smoothing) * signal[0].powi(2).max(signal[1].powi(2));
        power = smoothing * power + (1.0 - smoothing) * band[0].powi(2).max(band[1].powi(2));
        if i % block == block - 1 && full > 1e-10 {
            levels.push(10.0 * power.max(1e-30).log10());
            ratios.push(10.0 * (power.max(1e-30) / full).log10());
        }
    }
    if levels.len() < 100 {
        return Err("Insufficient active signal for dynamic calibration".into());
    }
    Ok(Band {
        id: "treatment.measured-dynamic",
        hz: spec.hz,
        q: spec.q,
        domain: spec.domain,
        ratio_p50_p95_db: [percentile(&ratios, 0.5), percentile(&ratios, 0.95)],
        level_p50_p95_dbfs: [percentile(&levels, 0.5), percentile(&levels, 0.95)],
        detector: Detector::AbsoluteBandPower,
        active_blocks: levels.len(),
        static_cut_db: 0.0,
        dynamic_threshold_db: percentile(&levels, spec.threshold_percentile) + spec.threshold_offset_db,
        maximum_dynamic_cut_db: spec.maximum_cut_db,
        reason: "Measured at the treatment insertion point with the existing 5 ms linked band-power detector",
    })
}
