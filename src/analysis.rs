use crate::{audio_io::Stereo, dsp::db};
use serde::Serialize;

pub fn percentile(values: &[f64], fraction: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let index = fraction.clamp(0.0, 1.0) * (sorted.len() - 1) as f64;
    let lo = index.floor() as usize;
    let hi = index.ceil() as usize;
    sorted[lo] + (sorted[hi] - sorted[lo]) * (index - lo as f64)
}

#[derive(Clone)]
pub struct Block {
    pub energy: f64,
    pub peak: f64,
}

/// The 10 ms blocks of `audio` at `rate`.
pub fn blocks(audio: &[Stereo], rate: u32) -> Vec<Block> {
    audio
        .chunks(rate as usize / 100)
        .map(|chunk| {
            let mut energy = 0.0;
            let mut peak = 0.0f64;
            for frame in chunk {
                for &v in frame {
                    energy += (v as f64).powi(2);
                    peak = peak.max(v.abs() as f64);
                }
            }
            Block {
                energy: energy / (chunk.len() * 2) as f64,
                peak,
            }
        })
        .collect()
}

fn mean_energy(blocks: &[Block]) -> f64 {
    blocks.iter().map(|b| b.energy).sum::<f64>() / blocks.len().max(1) as f64
}

fn crest(blocks: &[Block]) -> f64 {
    db(blocks.iter().map(|b| b.peak).fold(0.0f64, f64::max)) - db(mean_energy(blocks).sqrt())
}

fn attack_contrast(blocks: &[Block], start: usize) -> f64 {
    let attack = blocks[start..start + 3]
        .iter()
        .map(|b| b.peak)
        .fold(0.0f64, f64::max);
    db(attack) - db(mean_energy(&blocks[start + 5..start + 20]).sqrt())
}

#[derive(Clone, Serialize)]
pub struct Event {
    pub start_seconds: f64,
    pub attack_sustain_db: f64,
    pub rise_db: f64,
}

#[derive(Clone, Serialize)]
pub struct Window {
    pub start_seconds: f64,
    pub duration_seconds: f64,
    pub rms_dbfs: f64,
    pub crest_db: f64,
    pub active: bool,
}

#[derive(Clone, Serialize)]
pub struct Temporal {
    pub crest_50ms_p10_p50_p90_db: [f64; 3],
    pub events: Vec<Event>,
    pub median_event_gap_seconds: Option<f64>,
    pub windows: Vec<Window>,
    pub window_contrast_db: f64,
    pub dc_per_channel: [f64; 2],
    pub flat_top_runs: usize,
    pub clipped_channel_samples: usize,
    /// Share of active 10 ms blocks whose peak is within 1 dB of the 99.9th
    /// percentile block peak. A limiter holds many blocks under its ceiling.
    pub plateau_share: f64,
    /// P90 minus P10 of the RMS of those blocks, in dB. A limiter holds
    /// blocks of different level under one ceiling; repeated identical hits
    /// share the peak and the level.
    pub plateau_level_spread_db: f64,
    pub analysis_note: &'static str,
    #[serde(skip)]
    pub blocks: Vec<Block>,
    #[serde(skip)]
    pub activity_floor: f64,
}

pub fn analyze(audio: &[Stereo], rate: u32) -> Temporal {
    let blocks = blocks(audio, rate);
    let maximum = blocks.iter().map(|b| b.energy).fold(0.0f64, f64::max);
    let floor = (maximum * 1e-4).max(1e-10);
    let crests: Vec<f64> = blocks
        .chunks(5)
        .filter(|b| b.len() == 5 && mean_energy(b) > floor)
        .map(crest)
        .collect();
    let mut events = Vec::new();
    let mut next = 5;
    for i in 5..blocks.len().saturating_sub(20) {
        let prior = mean_energy(&blocks[i - 5..i]);
        let rise = db((blocks[i].energy / prior.max(1e-20)).sqrt());
        if i >= next && blocks[i].energy > floor.max(1e-6) && rise >= 5.0 {
            events.push(Event {
                start_seconds: i as f64 / 100.0,
                attack_sustain_db: attack_contrast(&blocks, i),
                rise_db: rise,
            });
            next = i + 10;
        }
    }
    let gaps: Vec<f64> = events
        .windows(2)
        .map(|v| v[1].start_seconds - v[0].start_seconds)
        .collect();
    let mut windows = Vec::new();
    for (i, chunk) in blocks.chunks(1200).enumerate() {
        let duration = ((audio.len() as f64 / rate as f64) - i as f64 * 12.0).min(12.0);
        windows.push(Window {
            start_seconds: i as f64 * 12.0,
            duration_seconds: duration,
            rms_dbfs: db(mean_energy(chunk).sqrt()),
            crest_db: crest(chunk),
            active: duration >= 3.0 && mean_energy(chunk) > floor * 10.0,
        });
    }
    let levels: Vec<f64> = windows
        .iter()
        .filter(|w| w.active)
        .map(|w| w.rms_dbfs)
        .collect();
    let mut dc = [0.0; 2];
    let mut clipped = 0;
    let mut flat = 0;
    let mut lengths = [0usize; 2];
    for (i, frame) in audio.iter().enumerate() {
        for c in 0..2 {
            dc[c] += frame[c] as f64;
            clipped += usize::from(frame[c].abs() >= 1.0 - 2f32.powi(-23));
            if i > 0 && frame[c].abs() > 0.5 && (frame[c] - audio[i - 1][c]).abs() <= 2f32.powi(-23)
            {
                lengths[c] += 1;
                if lengths[c] == 2 {
                    flat += 1;
                }
            } else {
                lengths[c] = 0;
            }
        }
    }
    for v in &mut dc {
        *v /= audio.len().max(1) as f64;
    }
    let peaks: Vec<f64> = blocks
        .iter()
        .filter(|b| b.energy > floor)
        .map(|b| db(b.peak))
        .collect();
    let plateau = percentile(&peaks, 0.999);
    let plateau_share =
        peaks.iter().filter(|p| **p >= plateau - 1.0).count() as f64 / peaks.len().max(1) as f64;
    let plateau_levels: Vec<f64> = blocks
        .iter()
        .filter(|b| b.energy > floor && db(b.peak) >= plateau - 1.0)
        .map(|b| db(b.energy.sqrt()))
        .collect();
    let plateau_level_spread_db = if plateau_levels.len() > 2 {
        percentile(&plateau_levels, 0.9) - percentile(&plateau_levels, 0.1)
    } else {
        0.0
    };
    Temporal { crest_50ms_p10_p50_p90_db: [percentile(&crests, 0.1), percentile(&crests, 0.5), percentile(&crests, 0.9)],
        median_event_gap_seconds: (!gaps.is_empty()).then(|| percentile(&gaps, 0.5)), events, windows,
        window_contrast_db: percentile(&levels, 0.9)-percentile(&levels, 0.1),
        dc_per_channel: dc, flat_top_runs: flat, clipped_channel_samples: clipped, plateau_share, plateau_level_spread_db,
        analysis_note: "10 ms stereo-power blocks; 50 ms crest; 30 ms peak against 50–200 ms sustain; fixed 12 s windows, not musical section labels",
        blocks, activity_floor: floor }
}

/// P90 of the loss of onset energy of `output` against `reference`, in dB:
/// for each onset of `reference`, the energy of the first three 10 ms blocks
/// against that of blocks 5 to 19 (D126). Clipping a short peak keeps most
/// of this energy; a limiter gain over the onset removes it.
pub fn energy_attack_loss_p90_db(
    reference: &[Stereo],
    output: &[Stereo],
    rate: u32,
) -> Option<f64> {
    energy_attack_loss_p90_from(&analyze(reference, rate), output, rate)
}

/// `energy_attack_loss_p90_db` with the reference already analyzed: its
/// onsets and its 10 ms blocks come from `reference`.
pub fn energy_attack_loss_p90_from(
    reference: &Temporal,
    output: &[Stereo],
    rate: u32,
) -> Option<f64> {
    let (a, b) = (&reference.blocks, blocks(output, rate));
    let contrast = |blocks: &[Block], start: usize| {
        let mean = |range: std::ops::Range<usize>| {
            blocks[range.clone()].iter().map(|b| b.energy).sum::<f64>() / range.len() as f64
        };
        10.0 * (mean(start..start + 3).max(1e-30) / mean(start + 5..start + 20).max(1e-30)).log10()
    };
    let losses: Vec<f64> = reference
        .events
        .iter()
        .map(|e| (e.start_seconds * 100.0).round() as usize)
        .filter(|&start| start + 20 <= a.len().min(b.len()))
        .map(|start| contrast(a, start) - contrast(&b, start))
        .collect();
    (!losses.is_empty()).then(|| percentile(&losses, 0.9))
}

#[derive(Clone, Serialize)]
pub struct Preservation {
    pub crest_loss_p90_db: f64,
    pub attack_loss_p90_db: Option<f64>,
    pub event_count: usize,
    pub window_contrast_loss_db: f64,
    pub window_level_change_span_db: f64,
}

pub fn compare(input: &Temporal, output: &[Stereo], rate: u32) -> Preservation {
    let out = blocks(output, rate);
    let crest_losses: Vec<f64> = input
        .blocks
        .chunks(5)
        .zip(out.chunks(5))
        .filter(|(a, b)| a.len() == 5 && b.len() == 5 && mean_energy(a) > input.activity_floor)
        .map(|(a, b)| crest(a) - crest(b))
        .collect();
    let attack_losses: Vec<f64> = input
        .events
        .iter()
        .filter_map(|e| {
            let start = (e.start_seconds * 100.0).round() as usize;
            (start + 20 <= out.len()).then(|| e.attack_sustain_db - attack_contrast(&out, start))
        })
        .collect();
    let mut original_levels = Vec::new();
    let mut new_levels = Vec::new();
    let mut gains = Vec::new();
    for (window, chunk) in input.windows.iter().zip(out.chunks(1200)) {
        if window.active {
            let value = db(mean_energy(chunk).sqrt());
            original_levels.push(window.rms_dbfs);
            new_levels.push(value);
            gains.push(value - window.rms_dbfs);
        }
    }
    let contrast = |v: &[f64]| percentile(v, 0.9) - percentile(v, 0.1);
    Preservation {
        crest_loss_p90_db: percentile(&crest_losses, 0.9).max(0.0),
        attack_loss_p90_db: (!attack_losses.is_empty())
            .then(|| percentile(&attack_losses, 0.9).max(0.0)),
        event_count: attack_losses.len(),
        window_contrast_loss_db: (contrast(&original_levels) - contrast(&new_levels)).max(0.0),
        window_level_change_span_db: contrast(&gains),
    }
}

/// The level of each block of `hop` frames in dB, over both channels.
fn block_levels_db(audio: &[Stereo], hop: usize) -> Vec<f64> {
    audio
        .chunks_exact(hop)
        .map(|chunk| {
            let energy: f64 = chunk
                .iter()
                .map(|f| (f[0] as f64).powi(2) + (f[1] as f64).powi(2))
                .sum();
            db((energy / (2 * hop) as f64).sqrt()).max(-100.0)
        })
        .collect()
}

/// Each level less the mean of the `window` levels centred on it: the onset
/// envelope, which a gain change of the whole file leaves as it is.
fn onset_envelope(levels: &[f64], window: usize) -> Vec<f64> {
    let mut prefix = vec![0.0; levels.len() + 1];
    for (i, l) in levels.iter().enumerate() {
        prefix[i + 1] = prefix[i] + l;
    }
    (0..levels.len())
        .map(|i| {
            let lo = i.saturating_sub(window / 2);
            let hi = (i + window / 2 + 1).min(levels.len());
            levels[i] - (prefix[hi] - prefix[lo]) / (hi - lo) as f64
        })
        .collect()
}

/// The lag in `b` of the series `a`, within `lags`, with the greatest
/// normalized correlation over at least `minimum` overlapping points. The
/// correlation of lag `l` pairs `a[i]` with `b[i + l]`.
fn best_lag(a: &[f64], b: &[f64], lags: std::ops::RangeInclusive<isize>, minimum: usize) -> isize {
    let squares = |v: &[f64]| {
        let mut prefix = vec![0.0; v.len() + 1];
        for (i, x) in v.iter().enumerate() {
            prefix[i + 1] = prefix[i] + x * x;
        }
        prefix
    };
    let (sa, sb) = (squares(a), squares(b));
    let mut best = (0isize, f64::NEG_INFINITY);
    for lag in lags {
        let start = (-lag).max(0) as usize;
        let end = (a.len() as isize).min(b.len() as isize - lag).max(0) as usize;
        if end <= start || end - start < minimum {
            continue;
        }
        let product: f64 = (start..end)
            .map(|i| a[i] * b[(i as isize + lag) as usize])
            .sum();
        let (bs, be) = (
            (start as isize + lag) as usize,
            (end as isize + lag) as usize,
        );
        let energy = (sa[end] - sa[start]) * (sb[be] - sb[bs]);
        let score = product.abs() / energy.max(1e-30).sqrt();
        if score > best.1 {
            best = (lag, score);
        }
    }
    best.0
}

/// The frame of `comparison` at which frame 0 of `source` lies, so that
/// `comparison[offset + i]` is the same moment as `source[i]`. The offset
/// comes from the onset envelopes (the 10 ms levels in dB less their 1 s
/// moving mean), correlated within ±`maximum_seconds`, then from the 1 ms
/// levels within ±20 ms of that, then from the waveforms within ±1 ms over
/// eight 0.5 s windows. A negative offset places frame 0 of the comparison
/// after frame 0 of the source. A gain change, a limiter or a polarity
/// inversion of the comparison leaves the offset as it is.
pub fn align_offset(
    source: &[Stereo],
    comparison: &[Stereo],
    rate: u32,
    maximum_seconds: f64,
) -> isize {
    let coarse_hop = rate as usize / 100;
    let fine_hop = rate as usize / 1000;
    let coarse = |audio: &[Stereo]| onset_envelope(&block_levels_db(audio, coarse_hop), 100);
    let limit = (maximum_seconds * 100.0).round() as isize;
    let coarse_lag = best_lag(
        &coarse(source),
        &coarse(comparison),
        -limit..=limit,
        300.min(source.len() / coarse_hop / 2),
    );
    let fine = |audio: &[Stereo]| onset_envelope(&block_levels_db(audio, fine_hop), 1000);
    let fine_lag = best_lag(
        &fine(source),
        &fine(comparison),
        coarse_lag * 10 - 20..=coarse_lag * 10 + 20,
        3000.min(source.len() / fine_hop / 2),
    );
    let mono =
        |audio: &[Stereo]| -> Vec<f64> { audio.iter().map(|f| (f[0] + f[1]) as f64).collect() };
    let (a, b) = (mono(source), mono(comparison));
    let guess = fine_lag * fine_hop as isize;
    let window = rate as usize / 2;
    let overlap_start = (-guess).max(0) as usize;
    let overlap_end = (a.len() as isize).min(b.len() as isize - guess).max(0) as usize;
    if overlap_end <= overlap_start + 2 * window {
        return guess;
    }
    let span = overlap_end - overlap_start - window;
    let mut best = (guess, f64::NEG_INFINITY);
    for lag in guess - fine_hop as isize..=guess + fine_hop as isize {
        let mut score = 0.0;
        for k in 0..8 {
            let start = overlap_start + span * (2 * k + 1) / 16;
            let (bs, be) = (start as isize + lag, (start + window) as isize + lag);
            if bs < 0 || be as usize > b.len() {
                continue;
            }
            let (x, y) = (&a[start..start + window], &b[bs as usize..be as usize]);
            let product: f64 = x.iter().zip(y).map(|(p, q)| p * q).sum();
            let energy =
                x.iter().map(|p| p * p).sum::<f64>() * y.iter().map(|q| q * q).sum::<f64>();
            score += product.abs() / energy.max(1e-30).sqrt();
        }
        if score > best.1 {
            best = (lag, score);
        }
    }
    best.0
}

#[cfg(test)]
mod align_tests {
    use super::*;

    /// Twenty seconds of decaying noise bursts at intervals of 0.3 to 0.9 s
    /// and levels of 0.1 to 0.6, over a low noise floor, from a fixed linear
    /// congruential sequence.
    fn bursts(rate: u32) -> Vec<Stereo> {
        let mut state = 12345u64;
        let mut noise = move || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            ((state >> 33) as f64 / (1u64 << 31) as f64) * 2.0 - 1.0
        };
        let frames = 20 * rate as usize;
        let mut gains = vec![0.001; frames];
        let mut start = 0;
        while start < frames {
            let level = 0.35 + 0.25 * noise();
            let burst = (rate as usize / 20).min(frames - start);
            for (i, gain) in gains[start..start + burst].iter_mut().enumerate() {
                *gain = level * (-(i as f64 / rate as f64) * 40.0).exp();
            }
            start += (rate as f64 * (0.6 + 0.3 * noise())) as usize;
        }
        gains
            .into_iter()
            .map(|gain| {
                let v = (gain * noise()) as f32;
                [v, v * 0.8]
            })
            .collect()
    }

    #[test]
    fn a_delayed_quieter_copy_gives_its_delay() {
        let rate = 48_000;
        let source = bursts(rate);
        let mut comparison = vec![[0.0f32; 2]; 59_232];
        comparison.extend(source.iter().map(|f| [f[0] * 0.5, f[1] * 0.5]));
        assert_eq!(align_offset(&source, &comparison, rate, 5.0), 59_232);
    }

    #[test]
    fn a_trimmed_inverted_copy_gives_a_negative_offset() {
        let rate = 48_000;
        let source = bursts(rate);
        let comparison: Vec<Stereo> = source[30_000..].iter().map(|f| [-f[0], -f[1]]).collect();
        assert_eq!(align_offset(&source, &comparison, rate, 5.0), -30_000);
    }
}
