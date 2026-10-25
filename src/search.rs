use crate::audio_io::Result;
use serde::Serialize;

/// Smallest loudness reversal that stops the search as nonmonotonic.
const NONMONOTONIC_LU: f64 = 0.1;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Policy {
    pub maximum_evaluations: usize,
    pub tolerance_lu: f64,
    pub maximum_step_db: f64,
    pub minimum_step_db: f64,
    pub downward_range_db: f64,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            maximum_evaluations: 8,
            tolerance_lu: 0.05,
            maximum_step_db: 1.5,
            minimum_step_db: 0.005,
            downward_range_db: 12.0,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct Probe {
    pub drive_db: f64,
    pub output_lufs: f64,
    pub error_lu: f64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Report {
    pub rule: &'static str,
    pub policy: Policy,
    pub target_lufs: f64,
    pub minimum_drive_db: f64,
    pub maximum_drive_db: f64,
    pub stop_reason: &'static str,
    pub selected_probe: usize,
    pub probes: Vec<Probe>,
}

/// The callback measures the complete output, including final peak trim.
pub fn solve<T>(
    initial_drive: f64,
    maximum_drive: f64,
    target_lufs: f64,
    policy: Policy,
    mut render: impl FnMut(f64) -> Result<(f64, T)>,
) -> Result<(T, Report)> {
    if !initial_drive.is_finite()
        || !maximum_drive.is_finite()
        || !target_lufs.is_finite()
        || !(1..=64).contains(&policy.maximum_evaluations)
        || [
            policy.tolerance_lu,
            policy.maximum_step_db,
            policy.minimum_step_db,
            policy.downward_range_db,
        ]
        .iter()
        .any(|v| !v.is_finite() || *v <= 0.0)
        || policy.minimum_step_db > policy.maximum_step_db
    {
        return Err("Invalid loudness search settings".into());
    }
    let mut drive = initial_drive.min(maximum_drive);
    let minimum_drive = drive - policy.downward_range_db;
    if !minimum_drive.is_finite() {
        return Err("Invalid loudness search bounds".into());
    }
    let mut report = Report {
        rule: "loudness.search.v1",
        policy,
        target_lufs,
        minimum_drive_db: minimum_drive,
        maximum_drive_db: maximum_drive,
        stop_reason: "evaluation-limit",
        selected_probe: 0,
        probes: Vec::new(),
    };
    let mut best = None;
    let mut below: Option<Probe> = None;
    let mut above: Option<Probe> = None;
    // Illinois weights: the end of the bracket that stays for two steps has
    // its error halved, so the interpolation cannot stall at one end.
    let (mut below_weight, mut above_weight) = (1.0f64, 1.0f64);
    let mut last_side = 0i8;
    for _ in 0..policy.maximum_evaluations {
        let (lufs, output) = render(drive)?;
        let error = target_lufs - lufs;
        if !lufs.is_finite() || !error.is_finite() {
            return Err("Nonfinite loudness search measurement".into());
        }
        let probe = Probe {
            drive_db: drive,
            output_lufs: lufs,
            error_lu: error,
        };
        let better = report.probes.get(report.selected_probe).is_none_or(|old| {
            error.abs() < old.error_lu.abs()
                || (error.abs() == old.error_lu.abs() && drive < old.drive_db)
        });
        if better {
            report.selected_probe = report.probes.len();
            best = Some(output);
        }
        // Hold and lookahead make loudness locally uneven in drive by a few
        // hundredths of an LU; only a clear reversal stops the search.
        let reversal = policy.tolerance_lu.max(NONMONOTONIC_LU);
        let nonmonotonic = report.probes.iter().any(|old| {
            (drive > old.drive_db && lufs < old.output_lufs - reversal)
                || (drive < old.drive_db && lufs > old.output_lufs + reversal)
        });
        report.probes.push(probe);
        if error.abs() <= policy.tolerance_lu {
            report.stop_reason = "target-matched";
            break;
        }
        if nonmonotonic {
            report.stop_reason = "nonmonotonic-response";
            break;
        }
        let side = if error > 0.0 { 1 } else { -1 };
        if side == 1 {
            below = Some(probe);
            below_weight = 1.0;
            if last_side == 1 {
                above_weight *= 0.5;
            }
        } else {
            above = Some(probe);
            above_weight = 1.0;
            if last_side == -1 {
                below_weight *= 0.5;
            }
        }
        last_side = side;
        let proposed = if let (Some(low), Some(high)) = (below, above) {
            // Inside a bracket, interpolate the drive at the target between
            // its ends (regula falsi), and keep strictly inside the bracket.
            let (el, eh) = (low.error_lu * below_weight, high.error_lu * above_weight);
            let t = (el / (el - eh)).clamp(0.05, 0.95);
            low.drive_db + t * (high.drive_db - low.drive_db)
        } else {
            let slope = report.probes.iter().rev().nth(1).map_or(1.0, |old| {
                ((lufs - old.output_lufs) / (drive - old.drive_db)).clamp(0.1, 4.0)
            });
            drive + (error / slope).clamp(-policy.maximum_step_db, policy.maximum_step_db)
        };
        let next = proposed
            .clamp(
                drive - policy.maximum_step_db,
                drive + policy.maximum_step_db,
            )
            .clamp(minimum_drive, maximum_drive);
        if (next - drive).abs() < policy.minimum_step_db {
            report.stop_reason = if drive == maximum_drive && error > 0.0 {
                "upper-drive-bound"
            } else if drive == minimum_drive && error < 0.0 {
                "lower-drive-bound"
            } else {
                "drive-resolution"
            };
            break;
        }
        drive = next;
    }
    Ok((best.unwrap(), report))
}
