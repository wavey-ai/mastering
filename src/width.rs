//! Low-end mono and the side cut from the long-term source spectrum.
use crate::{
    dsp::{EqBand, Kind},
    pipeline::SourceAnalysis,
};
use serde::Serialize;

/// Side content below this frequency is removed when its share is large enough.
pub const LOW_END_MONO_HZ: f64 = 100.0;
/// Side share of total power below `LOW_END_MONO_HZ` that triggers the stage.
pub const LOW_END_SIDE_SHARE: f64 = 0.02;
/// Side bell of the side cut, at the centre of 2 to 8 kHz on a log scale.
pub const SIDE_CUT_HZ: f64 = 4000.0;
pub const SIDE_CUT_Q: f64 = 0.7;
/// Largest side cut, in dB.
pub const SIDE_CUT_MAXIMUM_DB: f64 = 3.0;
/// Excess of the upper side/mid over the body side/mid under which the side
/// stays unchanged, in dB.
pub const SIDE_CUT_DEAD_ZONE_DB: f64 = 0.5;

#[derive(Clone, Serialize)]
pub struct LowEnd {
    pub rule: &'static str,
    pub side_share_below_100_hz: f64,
    pub side_highpass_hz: Option<f64>,
}

pub fn low_end(source: &SourceAnalysis) -> LowEnd {
    let s = &source.spectrum;
    let (mut side, mut total) = (0.0, 0.0);
    for (i, c) in s.centers_hz.iter().enumerate() {
        if *c < LOW_END_MONO_HZ {
            side += s.side_power[i];
            total += s.band_power[i];
        }
    }
    let share = if total > 0.0 { side / total } else { 0.0 };
    LowEnd {
        rule: "set.low-end-mono.v1",
        side_share_below_100_hz: share,
        side_highpass_hz: (share >= LOW_END_SIDE_SHARE).then_some(LOW_END_MONO_HZ),
    }
}

/// Side treatment from the source's own image: the side/mid from 2 to
/// 8 kHz against the side/mid from 200 Hz to 8 kHz.
#[derive(Clone, Serialize)]
pub struct SideCut {
    pub rule: &'static str,
    pub upper_side_mid_db: f64,
    pub body_side_mid_db: f64,
    /// A side bell that returns the upper side/mid to the body side/mid,
    /// when the upper side is wider by the dead zone or more. The side is
    /// never lifted.
    pub band: Option<EqBand>,
}

/// Side/mid over the spectrum bands whose centres fall in `[low, high)`,
/// with each band's side power changed by `response(centre)` in dB.
fn side_mid_db(
    spectrum: &crate::Spectrum,
    low: f64,
    high: f64,
    response: impl Fn(f64) -> f64,
) -> Option<f64> {
    let (mut side, mut mid) = (0.0, 0.0);
    for (i, c) in spectrum.centers_hz.iter().enumerate() {
        if (low..high).contains(c) {
            side += spectrum.side_power[i] * 10f64.powf(response(*c) / 10.0);
            mid += spectrum.mid_power[i];
        }
    }
    (side > 0.0 && mid > 0.0).then(|| 10.0 * (side / mid).log10())
}

pub fn side_cut(source: &SourceAnalysis) -> SideCut {
    let s = &source.spectrum;
    let upper = side_mid_db(s, 2000.0, 8000.0, |_| 0.0);
    let body = side_mid_db(s, 200.0, 8000.0, |_| 0.0);
    let band = upper.zip(body).and_then(|(upper, body)| {
        let excess = upper - body;
        if excess < SIDE_CUT_DEAD_ZONE_DB {
            return None;
        }
        let target = excess.min(SIDE_CUT_MAXIMUM_DB);
        let bell = |gain_db: f64| EqBand {
            kind: Kind::Peak,
            hz: SIDE_CUT_HZ,
            q: SIDE_CUT_Q,
            gain_db,
        };
        // The cut whose response lowers the upper side/mid by `target`,
        // by bisection over the band centres.
        let rate = source.sample_rate;
        let lowered = |gain_db: f64| {
            let filter = bell(gain_db).filter(rate);
            upper
                - side_mid_db(s, 2000.0, 8000.0, |hz| filter.response_db(hz, rate)).unwrap_or(upper)
        };
        let (mut low, mut high) = (-2.0 * SIDE_CUT_MAXIMUM_DB, 0.0);
        for _ in 0..40 {
            let gain = 0.5 * (low + high);
            if lowered(gain) > target {
                low = gain;
            } else {
                high = gain;
            }
        }
        Some(bell(0.5 * (low + high)))
    });
    SideCut {
        rule: "set.side-cut.v1",
        upper_side_mid_db: upper.unwrap_or(f64::NAN),
        body_side_mid_db: body.unwrap_or(f64::NAN),
        band,
    }
}

/// Largest correlation drop a band may take from the widening, as the
/// selection gate (`selection::WIDTH_CORRELATION_DROP`). A reference album
/// master drops two midrange bands by 0.31 and 0.32; with the margin, the
/// plan allows 0.35.
pub const WIDEN_CORRELATION_DROP: f64 = 0.40;
/// Largest extra mono fold-down loss of a band, in dB, as the selection gate
/// (`selection::WIDTH_MONO_LOSS_DB`).
pub const WIDEN_MONO_LOSS_DB: f64 = 1.5;
/// Correlation that the P10 of the 0.5 s windows of a band keeps. Under 0,
/// the channels cancel in mono in those passages.
pub const WIDEN_WINDOW_CORRELATION: f64 = 0.0;
/// The widening plan stays this far inside both correlation bounds, so the
/// compressor and peak control of the render do not take a band past the
/// gate. It is about one just-noticeable correlation difference from a
/// correlation near 1 (Gabriel and Colburn 1981).
pub const WIDEN_CORRELATION_MARGIN: f64 = 0.05;

/// Side/mid power ratio of a correlation, for uncorrelated mid and side of
/// equal channel levels: ρ = (1 − r) / (1 + r).
fn side_ratio(correlation: f64) -> f64 {
    (1.0 - correlation) / (1.0 + correlation)
}

/// Largest side gain of one stereo analysis band, in dB, that keeps the
/// whole-file correlation drop, the P10 window correlation and the extra
/// mono fold-down loss within their bounds. Negative when the P10 window
/// correlation is already under its bound; the widening then leaves the
/// band as it is (`widening`).
pub fn side_allowance_db(whole_correlation: f64, window_correlation_p10: f64) -> f64 {
    side_allowance_within_db(
        whole_correlation,
        window_correlation_p10,
        WIDEN_WINDOW_CORRELATION,
    )
}

/// `side_allowance_db` with `window_correlation` as the bound of the P10
/// window correlation.
pub fn side_allowance_within_db(
    whole_correlation: f64,
    window_correlation_p10: f64,
    window_correlation: f64,
) -> f64 {
    let r = side_ratio(whole_correlation.clamp(-0.99, 0.999));
    let limits = [
        side_ratio(
            (whole_correlation - WIDEN_CORRELATION_DROP + WIDEN_CORRELATION_MARGIN).max(-0.99),
        ) / r,
        side_ratio(window_correlation + WIDEN_CORRELATION_MARGIN)
            / side_ratio(window_correlation_p10.clamp(-0.99, 0.999)),
        ((1.0 + r) * 10f64.powf(WIDEN_MONO_LOSS_DB / 10.0) - 1.0) / r,
    ];
    let k = limits.into_iter().fold(f64::INFINITY, f64::min);
    (10.0 * k.log10()).max(-WIDEN_MAXIMUM_DB)
}

/// Side gain allowance of each band of a stereo analysis, in dB
/// (`stereo::BAND_EDGES_HZ`). Inactive bands get none.
pub fn side_allowances_db(image: &crate::stereo::StereoAnalysis) -> Vec<f64> {
    side_allowances_within_db(image, WIDEN_WINDOW_CORRELATION)
}

/// `side_allowances_db` with `window_correlation` as the bound of the P10
/// window correlation of each band.
pub fn side_allowances_within_db(
    image: &crate::stereo::StereoAnalysis,
    window_correlation: f64,
) -> Vec<f64> {
    image
        .whole
        .bands
        .iter()
        .zip(&image.short_summary)
        .map(|(band, summary)| {
            match (band.active, band.correlation, summary.correlation.as_ref()) {
                (true, Some(whole), Some(windows)) => {
                    side_allowance_within_db(whole, windows.p10, window_correlation)
                }
                _ => 0.0,
            }
        })
        .collect()
}

/// Bound of the P10 window correlation of a band of the Open version: the
/// bound of every version (`WIDEN_WINDOW_CORRELATION`). A mono fold-down and
/// a phone speaker sum the channels, and the reference album masters that
/// end a low-mid band under 0 do so from the polarity of a passage, an
/// anti-phase mix or a period widening (D183, D186).
pub const OPEN_WINDOW_CORRELATION: f64 = WIDEN_WINDOW_CORRELATION;
/// Largest side/mid of the eight reference album masters in each band, in
/// dB. The Open widening of a band stops where the mix already reaches it:
/// the mix/master pairs give no evidence beyond their outcomes.
pub const OPEN_OUTCOME_MAXIMUM_DB: [f64; 6] = [-13.8, -1.4, -0.1, 1.1, -2.6, -4.0];
/// Taps of the zero-phase side gain filter: 85 ms and 11.7 Hz resolution at
/// 48 kHz.
pub const SIDE_GAIN_TAPS: usize = 4097;
/// Width of the transition between two bands, in octaves.
pub const SIDE_GAIN_TRANSITION_OCTAVES: f64 = 1.0 / 3.0;

/// Zero-phase gain of the side channel in each stereo analysis band, and of
/// the mid channel for the fold of a preset (`adjust::folded`). With no mid
/// gain, only the side changes, so the mono sum stays exact. The filters
/// have no phase shift, so no image moves at a band edge.
#[derive(Clone, Copy, Serialize, serde::Deserialize, PartialEq, Debug)]
#[serde(deny_unknown_fields)]
pub struct SideGain {
    /// Side gain of each band of `stereo::BAND_EDGES_HZ`, in dB.
    pub bands_db: [f64; 6],
    /// Level of the decorrelated mid that the side receives in each band,
    /// against the mid, in dB. None adds no decorrelated mid to the band.
    pub decorrelated_db: [Option<f64>; 6],
    /// Mid gain of each band, in dB. The Mono and Radio presets give a band
    /// the power of the side that the widening of the version adds. Each
    /// version of a set has no mid gain.
    #[serde(default, skip_serializing_if = "is_flat")]
    pub mid_db: [f64; 6],
}

/// True when each gain is 0 dB.
fn is_flat(gains: &[f64; 6]) -> bool {
    gains.iter().all(|g| *g == 0.0)
}

/// Gain at `hz` of the band gains `bands_db`, in dB: the gains joined by
/// raised-cosine transitions on a log-frequency scale, centred on each band
/// edge.
fn joined_db(bands_db: &[f64; 6], hz: f64) -> f64 {
    let edges = &crate::stereo::BAND_EDGES_HZ;
    let half = SIDE_GAIN_TRANSITION_OCTAVES / 2.0;
    let mut gain = bands_db[0];
    for (i, pair) in bands_db.windows(2).enumerate() {
        let x = ((hz.max(1.0) / edges[i + 1]).log2() + half) / (2.0 * half);
        let t = if x <= 0.0 {
            0.0
        } else if x >= 1.0 {
            1.0
        } else {
            0.5 - 0.5 * (std::f64::consts::PI * x).cos()
        };
        gain += t * (pair[1] - pair[0]);
    }
    gain
}

impl SideGain {
    pub fn validate(&self) -> crate::audio_io::Result<()> {
        if self
            .bands_db
            .iter()
            .chain(&self.mid_db)
            .any(|g| !g.is_finite() || !(-24.0..=WIDEN_MAXIMUM_DB).contains(g))
            || self
                .decorrelated_db
                .iter()
                .flatten()
                .any(|d| !d.is_finite() || !(-30.0..=0.0).contains(d))
        {
            return Err("Invalid side gain".into());
        }
        Ok(())
    }

    /// The side gains and no decorrelated mid.
    pub fn gains(bands_db: [f64; 6]) -> SideGain {
        SideGain {
            bands_db,
            decorrelated_db: [None; 6],
            mid_db: [0.0; 6],
        }
    }

    /// Half the widening: half of each side gain in dB and half the power of
    /// the decorrelated mid.
    pub fn half(&self) -> SideGain {
        SideGain {
            bands_db: self.bands_db.map(|g| 0.5 * g),
            decorrelated_db: self
                .decorrelated_db
                .map(|d| d.map(|d| d - 10.0 * 2f64.log10())),
            mid_db: self.mid_db.map(|g| 0.5 * g),
        }
    }

    /// The widening midway between `self` and `other`: the mean side gain in
    /// dB and the mean power of the decorrelated mid.
    pub fn midway(&self, other: &SideGain) -> SideGain {
        SideGain {
            bands_db: std::array::from_fn(|i| 0.5 * (self.bands_db[i] + other.bands_db[i])),
            decorrelated_db: std::array::from_fn(|i| {
                let power = |d: Option<f64>| d.map_or(0.0, |d| 10f64.powf(d / 10.0));
                let mean = 0.5 * (power(self.decorrelated_db[i]) + power(other.decorrelated_db[i]));
                (mean > 0.0).then(|| 10.0 * mean.log10())
            }),
            mid_db: std::array::from_fn(|i| 0.5 * (self.mid_db[i] + other.mid_db[i])),
        }
    }

    /// True when a band has a mid gain.
    pub fn has_mid_gain(&self) -> bool {
        !is_flat(&self.mid_db)
    }

    /// Mid gain at `hz`, in dB: the band gains joined as in `response_db`.
    pub fn mid_response_db(&self, hz: f64) -> f64 {
        joined_db(&self.mid_db, hz)
    }

    /// Power factor of the mid gain at `hz`: exactly 1 without a mid gain.
    pub fn mid_power(&self, hz: f64) -> f64 {
        if self.has_mid_gain() {
            10f64.powf(self.mid_response_db(hz) / 10.0)
        } else {
            1.0
        }
    }

    /// Amplitude of the decorrelated mid at `hz`, against the mid, for audio
    /// at `rate`: the band amplitudes joined by the transitions of
    /// `response_db`. A band with no decorrelated mid has an amplitude of 0.
    pub fn decorrelated_amplitude(&self, hz: f64, rate: u32) -> f64 {
        let loss = decorrelation_loss_db(rate);
        let mut amplitude = [0.0; 6];
        for (i, d) in self.decorrelated_db.iter().enumerate() {
            amplitude[i] = d.map_or(0.0, |d| 10f64.powf((d - loss[i]) / 20.0));
        }
        let edges = &crate::stereo::BAND_EDGES_HZ;
        let half = SIDE_GAIN_TRANSITION_OCTAVES / 2.0;
        let mut value = amplitude[0];
        for (i, pair) in amplitude.windows(2).enumerate() {
            let x = ((hz.max(1.0) / edges[i + 1]).log2() + half) / (2.0 * half);
            let t = if x <= 0.0 {
                0.0
            } else if x >= 1.0 {
                1.0
            } else {
                0.5 - 0.5 * (std::f64::consts::PI * x).cos()
            };
            value += t * (pair[1] - pair[0]);
        }
        value
    }

    /// Gain at `hz`, in dB: the band gains joined by raised-cosine
    /// transitions on a log-frequency scale, centred on each band edge.
    pub fn response_db(&self, hz: f64) -> f64 {
        joined_db(&self.bands_db, hz)
    }

    /// Apply the gain to the side channel of `audio`, at `rate`, in place,
    /// by FFT convolution with the zero-phase kernel, and add the
    /// decorrelated mid to the side. Without a mid gain only the side
    /// changes, so the mono sum stays exact. A mid gain filters the mid with
    /// a zero-phase kernel of the same length; the decorrelated mid comes
    /// from the mid before that gain.
    pub fn apply(&self, audio: &mut [crate::audio_io::Stereo], rate: u32) {
        let mid_gain = self.has_mid_gain().then(|| {
            let kernel = crate::fir::kernel(
                SIDE_GAIN_TAPS,
                |hz| 10f64.powf(self.mid_response_db(hz) / 20.0),
                rate,
            );
            // A mid of f32 samples: the output is f32, and a whole f64 copy
            // of a 30-minute recording is 0.6 GB.
            let mut mid = vec![0.0f32; audio.len()];
            crate::fir::filter_each(
                audio.len(),
                |i| 0.5 * (audio[i][0] as f64 + audio[i][1] as f64),
                &kernel,
                |i, value| mid[i] = value as f32,
            );
            mid
        });
        let kernel = crate::fir::kernel(
            SIDE_GAIN_TAPS,
            |hz| 10f64.powf(self.response_db(hz) / 20.0),
            rate,
        );
        // The filters read the side and the mid from `audio` as they go:
        // whole f64 copies of a 30-minute recording are 0.6 GB each.
        let mut out = vec![0.0f64; audio.len()];
        crate::fir::filter_each(
            audio.len(),
            |i| 0.5 * (audio[i][0] as f64 - audio[i][1] as f64),
            &kernel,
            |i, value| out[i] = value,
        );
        if self.decorrelated_db.iter().any(Option::is_some) {
            let kernel = crate::fir::quadrature_kernel(
                SIDE_GAIN_TAPS,
                |hz| self.decorrelated_amplitude(hz, rate),
                decorrelation_sign,
                rate,
            );
            crate::fir::filter_each(
                audio.len(),
                |i| 0.5 * (audio[i][0] as f64 + audio[i][1] as f64),
                &kernel,
                |i, value| out[i] += value,
            );
        }
        match mid_gain {
            None => {
                for (frame, s) in audio.iter_mut().zip(out) {
                    let mid = 0.5 * (frame[0] as f64 + frame[1] as f64);
                    *frame = [(mid + s) as f32, (mid - s) as f32];
                }
            }
            Some(mids) => {
                for ((frame, s), mid) in audio.iter_mut().zip(out).zip(mids) {
                    let mid = mid as f64;
                    *frame = [(mid + s) as f32, (mid - s) as f32];
                }
            }
        }
    }
}

/// Mean response of `bands` at `rate` in each stereo analysis band, in dB,
/// at 16 points spaced evenly on a log scale.
pub fn band_means_db(bands: &[EqBand], rate: u32) -> [f64; 6] {
    let edges = &crate::stereo::BAND_EDGES_HZ;
    let filters: Vec<_> = bands.iter().map(|b| b.filter(rate)).collect();
    std::array::from_fn(|i| {
        let (low, high) = (edges[i].max(20.0), edges[i + 1].min(18000.0));
        (0..16)
            .map(|k| {
                let hz = low * (high / low).powf((k as f64 + 0.5) / 16.0);
                filters.iter().map(|f| f.response_db(hz, rate)).sum::<f64>()
            })
            .sum::<f64>()
            / 16.0
    })
}

/// The side/mid change of each stereo analysis band that a tone gives the
/// long-term `spectrum`, in dB, with the tone at its widest: side lifts and
/// mid cuts in full, side cuts and mid lifts not at all, because a focus
/// envelope can hold them back in a passage; stereo filters in full, because
/// a lift where a band is mono raises its mid more than its side. The
/// change is weighted by the spectrum, because the mean response of a
/// filter over a band (`band_means_db`) counts a narrow side bell where the
/// band has almost no side as width.
pub fn tone_width_db(
    tone: &crate::treatment::Tone,
    spectrum: &crate::Spectrum,
    rate: u32,
) -> [f64; 6] {
    let edges = &crate::stereo::BAND_EDGES_HZ;
    let filters = |bands: &[EqBand], keep: fn(f64) -> bool| -> Vec<crate::dsp::Biquad> {
        bands
            .iter()
            .filter(|b| keep(b.gain_db))
            .map(|b| b.filter(rate))
            .collect()
    };
    let side = filters(&tone.side, |g| g > 0.0);
    let mid = filters(&tone.mid, |g| g < 0.0);
    let stereo = filters(&tone.stereo, |_| true);
    let power = |filters: &[crate::dsp::Biquad], hz: f64| -> f64 {
        10f64.powf(filters.iter().map(|f| f.response_db(hz, rate)).sum::<f64>() / 10.0)
    };
    let (mut before, mut after) = ([[0.0f64; 2]; 6], [[0.0f64; 2]; 6]);
    for (i, &hz) in spectrum.centers_hz.iter().enumerate() {
        let Some(b) = (0..6).find(|b| hz >= edges[*b] && hz < edges[*b + 1]) else {
            continue;
        };
        let common = power(&stereo, hz);
        let (m, s) = (spectrum.mid_power[i], spectrum.side_power[i]);
        before[b][0] += m;
        before[b][1] += s;
        after[b][0] += m * common * power(&mid, hz);
        after[b][1] += s * common * power(&side, hz);
    }
    std::array::from_fn(|b| {
        let ratio = |p: [f64; 2]| 10.0 * (p[1].max(1e-30) / p[0].max(1e-30)).log10();
        if before[b][0] > 0.0 && before[b][1] > 0.0 {
            ratio(after[b]) - ratio(before[b])
        } else {
            0.0
        }
    })
}

impl SideGain {
    /// The widening that, with the tone's own width change on `spectrum`
    /// (`tone_width_db`), changes each band's side/mid by at most this gain.
    /// The widening never cuts. The tone is at `rate`.
    pub fn beside(
        &self,
        tone: &crate::treatment::Tone,
        spectrum: &crate::Spectrum,
        rate: u32,
    ) -> Option<SideGain> {
        let widest = tone_width_db(tone, spectrum, rate);
        let net = SideGain {
            bands_db: std::array::from_fn(|i| {
                if self.bands_db[i] < 0.0 {
                    // A narrowing band narrows by its gain whatever the tone.
                    return self.bands_db[i];
                }
                (self.bands_db[i] - widest[i]).clamp(0.0, WIDEN_MAXIMUM_DB)
            }),
            // The decorrelated mid follows the mid after the tone, so the
            // tone does not change its level against the mid.
            decorrelated_db: self.decorrelated_db,
            mid_db: self.mid_db,
        };
        (net.bands_db.iter().any(|g| *g != 0.0) || net.decorrelated_db.iter().any(Option::is_some))
            .then_some(net)
    }

    /// The side/mid change of this widening with the tone's own on
    /// `spectrum`, at `rate`.
    pub fn with(
        &self,
        tone: &crate::treatment::Tone,
        spectrum: &crate::Spectrum,
        rate: u32,
    ) -> SideGain {
        let widest = tone_width_db(tone, spectrum, rate);
        SideGain {
            bands_db: std::array::from_fn(|i| {
                if self.bands_db[i] < 0.0 {
                    self.bands_db[i]
                } else {
                    self.bands_db[i] + widest[i]
                }
            }),
            decorrelated_db: self.decorrelated_db,
            mid_db: self.mid_db,
        }
    }
}

/// Share of the allowed side power under which the side gain hands the rest
/// to the decorrelated mid: the source's own side, lifted by
/// `WIDEN_MAXIMUM_DB`, carries less than half of the allowed width. A mono
/// or nearly mono band then receives its width from the mid.
pub const DECORRELATE_SHARE: f64 = 0.5;

/// Sign of the 90° phase of the decorrelated mid at `hz`: it alternates in
/// each equivalent rectangular bandwidth (Glasberg and Moore 1990), so each
/// auditory filter hears one phase, half of them leading on the left and
/// half on the right, and the image keeps its centre.
pub fn decorrelation_sign(hz: f64) -> f64 {
    let erb_number = 21.4 * (1.0 + 0.00437 * hz).log10();
    if erb_number.floor() as i64 % 2 == 0 {
        1.0
    } else {
        -1.0
    }
}

/// Power gain of the decorrelation kernel in each band at a planned level of
/// 0 dB, in dB. The window of the kernel smooths each sign change of the
/// phase, and the ERBs are narrow at low frequencies, so the 120 to 500 Hz
/// band loses about 1.8 dB. The amplitude of each band is raised by its loss.
/// The kernel is at `rate`.
pub fn decorrelation_loss_db(rate: u32) -> [f64; 6] {
    let measure = || -> [f64; 6] {
        let kernel = crate::fir::quadrature_kernel(
            SIDE_GAIN_TAPS,
            |hz| {
                if hz >= crate::stereo::BAND_EDGES_HZ[1] {
                    1.0
                } else {
                    0.0
                }
            },
            decorrelation_sign,
            rate,
        );
        let measured = crate::fir::band_power_db(&kernel, &crate::stereo::BAND_EDGES_HZ, rate);
        // The band under 120 Hz receives no decorrelated mid.
        std::array::from_fn(|i| if i == 0 { 0.0 } else { measured[i].min(0.0) })
    };
    // Each render rate measures its kernel once.
    static AT_44100: std::sync::OnceLock<[f64; 6]> = std::sync::OnceLock::new();
    static AT_48000: std::sync::OnceLock<[f64; 6]> = std::sync::OnceLock::new();
    match rate {
        44100 => *AT_44100.get_or_init(measure),
        48000 => *AT_48000.get_or_init(measure),
        _ => measure(),
    }
}

/// Level of the decorrelated mid, against the mid, in dB, that fills a band
/// whose desired side gain `desired_db` the side gain cannot reach. The band's
/// side/mid power ratio is `ratio`. None when the side gain reaches
/// `DECORRELATE_SHARE` of the desired side power.
pub fn decorrelated_db(ratio: f64, desired_db: f64) -> Option<f64> {
    let desired = ratio * 10f64.powf(desired_db / 10.0);
    let reached = ratio * 10f64.powf(desired_db.min(WIDEN_MAXIMUM_DB) / 10.0);
    (reached < DECORRELATE_SHARE * desired).then(|| 10.0 * (desired - reached).log10())
}

/// Rule of the Mono-safe preset.
pub const MONO_SAFE_RULE: &str = "adjust.mono-safe.v1";

/// The side bells of the Mono-safe preset: for each band in `bands`, a side
/// bell at the gain that brings the band's P10 window correlation to the
/// window bound, keyed to the 0.5 s windows of `image` whose correlation is
/// negative and to the window on each side of them. A band whose gain is
/// under `WIDEN_MINIMUM_DB` gets no bell. `image` is the stereo analysis of
/// the mix at `rate`.
pub fn mono_safe(
    image: &crate::stereo::StereoAnalysis,
    bands: &[usize],
    rate: u32,
) -> Vec<crate::treatment::EnvelopedBand> {
    let edges = &crate::stereo::BAND_EDGES_HZ;
    let frames = image.input_frames;
    let _ = rate;
    bands
        .iter()
        .filter(|&&b| b > 0 && b + 1 < edges.len())
        .filter_map(|&b| {
            let p10 = image.short_summary[b].correlation.as_ref()?.p10;
            let gain = (10.0
                * (side_ratio(WIDEN_WINDOW_CORRELATION + WIDEN_CORRELATION_MARGIN)
                    / side_ratio(p10.clamp(-0.99, 0.999)))
                .log10())
            .clamp(-WIDEN_MAXIMUM_DB, 0.0);
            if gain > -WIDEN_MINIMUM_DB {
                return None;
            }
            let negative: Vec<bool> = image
                .short_windows
                .iter()
                .map(|w| w.bands[b].active && w.bands[b].correlation.is_some_and(|c| c < 0.0))
                .collect();
            if !negative.iter().any(|n| *n) {
                return None;
            }
            // The window on each side of a negative window takes the cut
            // too, so a short gap between two does not open and close it.
            let wet: Vec<f64> = (0..negative.len())
                .map(|i| {
                    let around = negative[i]
                        || (i > 0 && negative[i - 1])
                        || negative.get(i + 1).copied().unwrap_or(false);
                    if around {
                        1.0
                    } else {
                        0.0
                    }
                })
                .collect();
            let mut points = vec![crate::treatment::BlendPoint {
                sample: 0,
                wet: wet[0],
            }];
            for (w, share) in image.short_windows.iter().zip(&wet) {
                let sample = (w.start_sample + w.end_sample) / 2;
                if sample > points.last().unwrap().sample && sample < frames - 1 {
                    points.push(crate::treatment::BlendPoint {
                        sample,
                        wet: *share,
                    });
                }
            }
            points.push(crate::treatment::BlendPoint {
                sample: frames - 1,
                wet: *wet.last().unwrap(),
            });
            let (low, high) = (edges[b], edges[b + 1]);
            let hz = (low * high).sqrt();
            Some(crate::treatment::EnvelopedBand {
                rule: MONO_SAFE_RULE.into(),
                domain: crate::finishing::Domain::Side,
                band: EqBand {
                    kind: Kind::Peak,
                    hz,
                    q: hz / (high - low),
                    gain_db: gain,
                },
                points,
            })
        })
        .collect()
}

/// A family widening under this gain in every band is not applied, in dB:
/// a stage must change the mix by 0.5 dB or more.
pub const WIDEN_MINIMUM_DB: f64 = 0.5;
/// Largest side gain or cut of a band, in dB: the side gain of a stereo
/// imager at its full width.
pub const WIDEN_MAXIMUM_DB: f64 = 6.0;

/// Side widening of each version from the source image
/// (`set.widening.v6`, D206): the width pattern of the version
/// (`character::Character::width_db`), each band with a side in the mix
/// bounded by its allowance under `OPEN_WINDOW_CORRELATION` and by
/// `outcome_room_db`, at least 0 and at most `WIDEN_MAXIMUM_DB`.
#[derive(Clone, Serialize)]
pub struct Widening {
    pub rule: &'static str,
    /// Largest side gain of each stereo analysis band under
    /// `OPEN_WINDOW_CORRELATION`, in dB.
    pub allowances_db: Vec<f64>,
    /// Room from the side/mid of the mix to `OPEN_OUTCOME_MAXIMUM_DB` in
    /// each band, in dB.
    pub outcome_room_db: Vec<f64>,
    pub essential_db: Vec<f64>,
    pub master_db: Vec<f64>,
    pub bold_db: Vec<f64>,
    pub open_db: Vec<f64>,
    /// Level of the decorrelated mid of each band of each version, in dB
    /// against the mid (`decorrelated_db`).
    pub essential_decorrelated_db: Vec<Option<f64>>,
    pub master_decorrelated_db: Vec<Option<f64>>,
    pub bold_decorrelated_db: Vec<Option<f64>>,
    pub open_decorrelated_db: Vec<Option<f64>>,
    pub essential: Option<SideGain>,
    pub master: Option<SideGain>,
    pub bold: Option<SideGain>,
    pub open: Option<SideGain>,
}

impl Widening {
    /// The side gain of the version `id`.
    pub fn of(&self, id: &str) -> Option<SideGain> {
        match id {
            "essential" => self.essential,
            "master" => self.master,
            "bold" => self.bold,
            "open" => self.open,
            _ => None,
        }
    }
}

/// The widening from `image`: the stereo analysis of the mix after the
/// compressor and peak control at the base target, whose peak control
/// already widens the image, or of the source when it arrives limited.
pub fn widening(image: &crate::stereo::StereoAnalysis) -> Widening {
    // A negative allowance (a band whose quietest-correlated passages are
    // already under the bound) gives no widening and no cut: the width of
    // the mix is the mix engineer's choice. A mix can be wide and out of
    // phase in passages by design, and a master that narrows it changes
    // that design.
    let allowances = side_allowances_within_db(image, OPEN_WINDOW_CORRELATION);
    let outcome_room_db: Vec<f64> = image
        .whole
        .bands
        .iter()
        .enumerate()
        .map(|(i, b)| {
            b.side_mid_db
                .map_or(0.0, |s| OPEN_OUTCOME_MAXIMUM_DB[i] - s)
        })
        .collect();
    // The side/mid ratio that the allowance starts from.
    let ratios: Vec<Option<f64>> = image
        .whole
        .bands
        .iter()
        .map(|b| {
            b.active
                .then_some(b.correlation)
                .flatten()
                .map(|c| side_ratio(c.clamp(-0.99, 0.999)))
        })
        .collect();
    let version = |id: &str| -> (Vec<f64>, Vec<Option<f64>>, Option<SideGain>) {
        let pattern = crate::character::of(id).map_or([0.0; 6], |c| c.width_db);
        // A band with no side, as in a mono source, takes its width from
        // the decorrelated mid up to its allowance under the correlation
        // and mono fold-down bounds of D141; the band under 120 Hz of such a
        // source stays mono.
        let desired: Vec<f64> = image
            .whole
            .bands
            .iter()
            .enumerate()
            .map(|(i, band)| {
                if band.side_present {
                    pattern[i].min(allowances[i]).min(outcome_room_db[i])
                } else if i == 0 {
                    0.0
                } else {
                    allowances[i]
                }
                .max(0.0)
            })
            .collect();
        let decorrelated: Vec<Option<f64>> = desired
            .iter()
            .zip(&ratios)
            .map(|(d, r)| r.and_then(|r| decorrelated_db(r, *d)))
            .collect();
        let db: Vec<f64> = desired.iter().map(|d| d.min(WIDEN_MAXIMUM_DB)).collect();
        let gain = (db.iter().any(|t| *t >= WIDEN_MINIMUM_DB)
            || decorrelated.iter().any(Option::is_some))
        .then(|| SideGain {
            bands_db: std::array::from_fn(|i| db[i]),
            decorrelated_db: std::array::from_fn(|i| decorrelated[i]),
            mid_db: [0.0; 6],
        });
        (db, decorrelated, gain)
    };
    let (essential_db, essential_decorrelated_db, essential) = version("essential");
    let (master_db, master_decorrelated_db, master) = version("master");
    let (bold_db, bold_decorrelated_db, bold) = version("bold");
    let (open_db, open_decorrelated_db, open) = version("open");
    Widening {
        rule: "set.widening.v6",
        allowances_db: allowances,
        outcome_room_db,
        essential_db,
        master_db,
        bold_db,
        open_db,
        essential_decorrelated_db,
        master_decorrelated_db,
        bold_decorrelated_db,
        open_decorrelated_db,
        essential,
        master,
        bold,
        open,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// Noise whose side share is `upper_side` from 2 to 8 kHz and
    /// `body_side` elsewhere, as amplitude ratios of side to mid.
    pub(crate) fn mix(upper_side: f32, body_side: f32) -> Vec<crate::audio_io::Stereo> {
        let mut state = 0x2545_f491u32;
        let mut noise = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state as f32 / u32::MAX as f32) * 2.0 - 1.0
        };
        // Two uncorrelated noises, split at about 2 kHz by a one-pole filter.
        let (mut low_mid, mut low_side) = (0.0f32, 0.0f32);
        let a = (-2.0 * std::f32::consts::PI * 2000.0 / 48000.0).exp();
        (0..10 * 48000)
            .map(|_| {
                let (m, s) = (noise(), noise());
                low_mid = (1.0 - a) * m + a * low_mid;
                low_side = (1.0 - a) * s + a * low_side;
                // The body is 12 dB louder than the upper band, as in a mix.
                let mid = 0.1 * (4.0 * low_mid + (m - low_mid));
                let side = 0.1 * (4.0 * body_side * low_side + upper_side * (s - low_side));
                [mid + side, mid - side]
            })
            .collect()
    }

    #[test]
    fn a_wide_upper_side_is_cut_toward_the_body() {
        let source = crate::pipeline::analyze_source(&mix(0.8, 0.3), 48000).unwrap();
        let cut = side_cut(&source);
        let excess = cut.upper_side_mid_db - cut.body_side_mid_db;
        assert!(excess > 1.0, "{excess}");
        let band = cut.band.expect("the upper side is wider than the body");
        assert!(band.gain_db < -1.0 && band.gain_db >= -2.0 * SIDE_CUT_MAXIMUM_DB);
    }

    #[test]
    fn each_version_widens_by_its_pattern_within_the_bounds_of_the_mix() {
        // A side 20 dB under the mid in the body: narrow and correlated, as
        // the mixes of the pairs are, with room to their outcome.
        let narrow = widening(&crate::stereo::analyze(&mix(0.2, 0.1), 48000).unwrap());
        for id in ["essential", "master", "bold", "open"] {
            let pattern = crate::character::of(id).unwrap().width_db;
            let db = match id {
                "essential" => &narrow.essential_db,
                "master" => &narrow.master_db,
                "bold" => &narrow.bold_db,
                _ => &narrow.open_db,
            };
            assert!(narrow.of(id).is_some(), "{id}");
            for i in 0..6 {
                let bound = pattern[i]
                    .min(narrow.allowances_db[i])
                    .min(narrow.outcome_room_db[i])
                    .clamp(0.0, WIDEN_MAXIMUM_DB);
                assert_eq!(db[i], bound, "{id} band {i}");
            }
        }
        for i in 0..6 {
            assert!(narrow.essential_db[i] <= narrow.master_db[i]);
            assert!(narrow.master_db[i] <= narrow.bold_db[i]);
        }
        // A side 4 dB under the mid in the body: wider than any album master
        // under 120 Hz, so that band has no room in any version.
        let wide = widening(&crate::stereo::analyze(&mix(0.2, 0.6), 48000).unwrap());
        assert!(wide.outcome_room_db[0] < 0.0, "{:?}", wide.outcome_room_db);
        for db in [
            &wide.essential_db,
            &wide.master_db,
            &wide.bold_db,
            &wide.open_db,
        ] {
            assert_eq!(db[0], 0.0);
        }
    }

    #[test]
    fn a_narrow_upper_side_is_not_lifted() {
        let source = crate::pipeline::analyze_source(&mix(0.2, 0.6), 48000).unwrap();
        let cut = side_cut(&source);
        assert!(cut.upper_side_mid_db < cut.body_side_mid_db);
        assert!(cut.band.is_none());
    }

    #[test]
    fn the_open_allowance_keeps_the_window_bound_of_every_version() {
        assert_eq!(
            side_allowance_within_db(0.63, 0.26, OPEN_WINDOW_CORRELATION),
            side_allowance_db(0.63, 0.26)
        );
        // A band with negative passages has no allowance in any version.
        assert!(side_allowance_within_db(0.3, -0.1, OPEN_WINDOW_CORRELATION) < 0.0);
    }

    #[test]
    fn allowance_follows_the_tightest_bound() {
        // A narrow band: the correlation drop binds.
        let narrow = side_allowance_db(0.9, 0.8);
        assert!((narrow - 10.0 * (side_ratio(0.55) / side_ratio(0.9)).log10()).abs() < 1e-9);
        // A band whose P10 window correlation is near the floor: the window
        // bound binds, with the margin.
        let windows = side_allowance_db(0.63, 0.26);
        assert!((windows - 10.0 * (side_ratio(0.05) / side_ratio(0.26)).log10()).abs() < 1e-9);
        // A band with negative passages has a negative allowance: the gain
        // that would bring its P10 window correlation to the bound.
        let narrow = side_allowance_db(0.3, -0.1);
        assert!((narrow - 10.0 * (side_ratio(0.05) / side_ratio(-0.1)).log10()).abs() < 1e-9);
        assert!(narrow < 0.0);
    }

    /// A long-term spectrum whose 120 to 500 Hz band has ten times the
    /// power under 250 Hz. `wide_low` puts the side under 250 Hz, else
    /// above it.
    fn two_part_spectrum(wide_low: bool) -> crate::Spectrum {
        let centers: Vec<f64> = (0..200)
            .map(|i| 20.0 * 1000f64.powf(i as f64 / 199.0))
            .collect();
        let mid: Vec<f64> = centers
            .iter()
            .map(|&hz| if hz < 250.0 { 10.0 } else { 1.0 })
            .collect();
        let side: Vec<f64> = centers
            .iter()
            .map(|&hz| if (hz < 250.0) == wide_low { 0.5 } else { 0.001 })
            .collect();
        crate::Spectrum {
            sample_rate: 48000,
            band_power: mid.iter().zip(&side).map(|(m, s)| m + s).collect(),
            centers_hz: centers,
            mid_power: mid,
            side_power: side,
            mel_power: Vec::new(),
            analyzed_frames: 1,
        }
    }

    #[test]
    fn the_tone_width_follows_the_spectrum_not_the_filter_mean() {
        let bell = |hz: f64, gain_db: f64, q: f64| EqBand {
            kind: Kind::Peak,
            hz,
            q,
            gain_db,
        };
        // A narrow side bell at 335 Hz where the band has almost no side:
        // the mean response over the band counts it, the spectrum does not.
        let side_bell = crate::treatment::Tone {
            side: vec![bell(335.0, 1.5, 2.0)],
            ..Default::default()
        };
        let mean = band_means_db(&side_bell.side, 48000)[1];
        let widened = tone_width_db(&side_bell, &two_part_spectrum(true), 48000)[1];
        assert!(mean > 0.3, "{mean}");
        assert!(
            widened >= 0.0 && widened < 0.5 * mean,
            "{widened} against {mean}"
        );
        // A stereo lift at 173 Hz, where the band is mono, raises the mid
        // of the band more than its side: the band narrows.
        let stereo_lift = crate::treatment::Tone {
            stereo: vec![bell(173.0, 3.5, 1.33)],
            ..Default::default()
        };
        let narrowed = tone_width_db(&stereo_lift, &two_part_spectrum(false), 48000)[1];
        assert!(narrowed < -0.5, "{narrowed}");
        // A side cut and a mid lift count for nothing.
        let narrowing = crate::treatment::Tone {
            side: vec![bell(335.0, -1.5, 2.0)],
            mid: vec![bell(335.0, 1.5, 2.0)],
            ..Default::default()
        };
        assert_eq!(
            tone_width_db(&narrowing, &two_part_spectrum(true), 48000),
            [0.0; 6]
        );
    }

    #[test]
    fn a_band_past_the_correlation_bound_is_neither_widened_nor_cut() {
        // Noise whose upper side outweighs the mid in every other second:
        // the upper bands have negative passages, the body does not.
        let mut audio = mix(0.6, 0.1);
        for (i, frame) in audio.iter_mut().enumerate() {
            if (i / 48000) % 2 == 1 {
                let (mid, side) = (0.5 * (frame[0] + frame[1]), 0.5 * (frame[0] - frame[1]));
                *frame = [0.3 * mid + 2.0 * side, 0.3 * mid - 2.0 * side];
            }
        }
        let w = widening(&crate::stereo::analyze(&audio, 48000).unwrap());
        let a = &w.allowances_db;
        assert!(a[2].min(a[3]) < 0.0, "{a:?}");
        // A band past the bound keeps the width of the mix in every version;
        // no version cuts a band.
        for db in [&w.essential_db, &w.master_db, &w.bold_db, &w.open_db] {
            for i in 0..6 {
                assert!(
                    db[i] >= 0.0 && db[i] <= a[i].max(0.0) + 1e-9,
                    "{db:?} {a:?}"
                );
            }
        }
    }

    #[test]
    fn mono_safe_cuts_the_side_only_where_the_channels_cancel() {
        // The upper side outweighs the mid in every other two seconds.
        let mut audio = mix(0.6, 0.1);
        for (i, frame) in audio.iter_mut().enumerate() {
            if (i / 96000) % 2 == 1 {
                let (mid, side) = (0.5 * (frame[0] + frame[1]), 0.5 * (frame[0] - frame[1]));
                *frame = [0.3 * mid + 2.0 * side, 0.3 * mid - 2.0 * side];
            }
        }
        let image = crate::stereo::analyze(&audio, 48000).unwrap();
        let bells = mono_safe(&image, &[1, 2, 3], 48000);
        // The upper band (2 to 6 kHz) has negative windows; the low mids and
        // the body do not.
        assert!(bells
            .iter()
            .all(|b| b.band.gain_db < 0.0 && b.band.gain_db >= -WIDEN_MAXIMUM_DB));
        assert!(
            bells.iter().any(|b| (b.band.hz - 3464.1).abs() < 1.0),
            "{bells:?}"
        );
        assert!(bells
            .iter()
            .all(|b| (b.band.hz - 244.9).abs() > 1.0 && (b.band.hz - 1000.0).abs() > 1.0));
        let bell = bells
            .iter()
            .find(|b| (b.band.hz - 3464.1).abs() < 1.0)
            .unwrap();
        // The odd two-second blocks are wet, with one window on each side;
        // the middle of an even block is dry.
        let wet_at = |second: f64| {
            let sample = (second * 48000.0) as usize;
            let i = bell.points.partition_point(|p| p.sample <= sample);
            bell.points[i.saturating_sub(1)].wet
        };
        assert_eq!(wet_at(7.0), 1.0, "{:?}", bell.points);
        assert_eq!(wet_at(5.0), 0.0, "{:?}", bell.points);
        assert_eq!(bell.points.first().unwrap().sample, 0);
        assert_eq!(bell.points.last().unwrap().sample, image.input_frames - 1);
    }

    #[test]
    fn side_gain_meets_band_targets_and_keeps_the_mono_sum() {
        let gain = SideGain::gains([0.0, 3.0, 4.0, 4.0, 3.0, 2.0]);
        let edges = &crate::stereo::BAND_EDGES_HZ;
        for i in 1..6 {
            // Mean over the band on a log scale.
            let (low, high) = (edges[i], edges[i + 1].min(18000.0));
            let mean = (0..32)
                .map(|k| gain.response_db(low * (high / low).powf((k as f64 + 0.5) / 32.0)))
                .sum::<f64>()
                / 32.0;
            assert!((mean - gain.bands_db[i]).abs() < 0.3, "band {i}: {mean}");
        }
        let mut state = 0x2545_f491u32;
        let mut noise = || {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            (state as f32 / u32::MAX as f32) - 0.5
        };
        let input: Vec<crate::audio_io::Stereo> = (0..96000).map(|_| [noise(), noise()]).collect();
        let mut output = input.clone();
        gain.apply(&mut output, 48000);
        for (a, b) in input.iter().zip(&output) {
            assert!(((a[0] + a[1]) - (b[0] + b[1])).abs() < 1e-5);
        }
        // A 1 kHz side sine gains 4 dB with no delay.
        let sine: Vec<crate::audio_io::Stereo> = (0..48000)
            .map(|i| {
                let x =
                    (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / 48000.0).sin() as f32 * 0.1;
                [x, -x]
            })
            .collect();
        let mut lifted = sine.clone();
        gain.apply(&mut lifted, 48000);
        let rms = |a: &[crate::audio_io::Stereo]| {
            a[12000..36000]
                .iter()
                .map(|f| (f[0] as f64).powi(2))
                .sum::<f64>()
        };
        let lift = 10.0 * (rms(&lifted) / rms(&sine)).log10();
        assert!((lift - 4.0).abs() < 0.1, "{lift}");
        // Zero phase: the lifted sine stays in phase with the input.
        let dot: f64 = sine[12000..36000]
            .iter()
            .zip(&lifted[12000..36000])
            .map(|(a, b)| a[0] as f64 * b[0] as f64)
            .sum();
        assert!(dot / (rms(&sine) * rms(&lifted)).sqrt() > 0.9999);
    }
    /// White noise on both channels: a mono recording.
    fn mono() -> Vec<crate::audio_io::Stereo> {
        let mut state = 0x2545_f491u32;
        (0..10 * 48000)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                let x = ((state as f32 / u32::MAX as f32) * 2.0 - 1.0) * 0.1;
                [x, x]
            })
            .collect()
    }

    #[test]
    fn a_mono_image_takes_its_width_from_the_mid_and_a_stereo_image_does_not() {
        let mono = widening(&crate::stereo::analyze(&mono(), 48000).unwrap());
        let master = mono.master.expect("a mono image widens");
        assert_eq!(master.decorrelated_db[0], None);
        for d in &master.decorrelated_db[1..] {
            // The correlation drop of 0.40 less the margin binds: side/mid
            // of (1 - 0.65) / (1 + 0.65).
            let d = d.expect("each band above 120 Hz");
            assert!((d - 10.0 * side_ratio(0.65).log10()).abs() < 0.1, "{d}");
        }
        let stereo = widening(&crate::stereo::analyze(&mix(0.3, 0.3), 48000).unwrap());
        assert!(stereo.master_decorrelated_db.iter().all(Option::is_none));
        assert!(stereo.bold_decorrelated_db.iter().all(Option::is_none));
        // The side gain reaches half of the desired side power 3 dB above
        // its bound.
        assert_eq!(decorrelated_db(0.01, WIDEN_MAXIMUM_DB + 2.9), None);
        assert!(decorrelated_db(0.01, WIDEN_MAXIMUM_DB + 3.1).is_some());
    }

    #[test]
    fn the_decorrelated_mid_keeps_the_mono_sum_the_balance_and_the_centre() {
        let input = mono();
        let mut output = input.clone();
        let mut gain = SideGain::gains([0.0; 6]);
        gain.decorrelated_db = [
            None,
            Some(-7.5),
            Some(-7.5),
            Some(-7.5),
            Some(-7.5),
            Some(-7.5),
        ];
        gain.apply(&mut output, 48000);
        for (a, b) in input.iter().zip(&output) {
            assert!(((a[0] + a[1]) - (b[0] + b[1])).abs() < 1e-6);
        }
        let image = crate::stereo::analyze(&output, 48000).unwrap();
        for band in &image.whole.bands[2..5] {
            // The side holds -7.5 dB of the mid, with no correlation to it,
            // so the channels keep equal levels.
            assert!(
                (band.side_mid_db.unwrap() + 7.5).abs() < 0.3,
                "{:?}",
                band.side_mid_db
            );
            assert!(band.right_left_db.unwrap().abs() < 0.05);
            let expected = (1.0 - side_ratio(0.70)) / (1.0 + side_ratio(0.70));
            assert!((band.correlation.unwrap() - expected).abs() < 0.03);
        }
        assert!(image.whole.bands[0].side_mid_db.is_none_or(|d| d < -30.0));
    }

    #[test]
    fn a_mid_gain_raises_the_mono_sum_of_its_band_and_keeps_the_side() {
        let input = mix(0.3, 0.3);
        let flat = SideGain::gains([0.0; 6]);
        assert!(!flat.has_mid_gain());
        assert_eq!(flat.mid_power(1000.0), 1.0);
        // A side gain without a mid gain serializes without `mid_db`.
        let json = serde_json::to_value(flat).unwrap();
        assert!(json.get("mid_db").is_none());
        assert_eq!(serde_json::from_value::<SideGain>(json).unwrap(), flat);
        let mut gain = flat;
        gain.mid_db[2] = 2.0;
        gain.validate().unwrap();
        let mut output = input.clone();
        gain.apply(&mut output, 48000);
        let (before, after) = (
            crate::stereo::analyze(&input, 48000).unwrap(),
            crate::stereo::analyze(&output, 48000).unwrap(),
        );
        let db = |a: f64, b: f64| 10.0 * (a / b).log10();
        let (a, b) = (&after.whole.bands, &before.whole.bands);
        // The mid of 500 Hz to 2 kHz rises by the gain; its side stays.
        assert!((db(a[2].mid_power, b[2].mid_power) - 2.0).abs() < 0.2);
        assert!(db(a[2].side_power, b[2].side_power).abs() < 0.05);
        // The bands beside it move by less than the transitions give.
        assert!(db(a[4].mid_power, b[4].mid_power).abs() < 0.05);
        assert!(db(a[1].mid_power, b[1].mid_power) < 1.0);
        gain.mid_db[2] = 7.0;
        assert!(gain.validate().is_err());
    }
}
