use mastering_engine::search::{self, Policy};

#[test]
fn compressed_response_converges_beyond_one_correction() {
    let (drive, report) = search::solve(0.0, 10.0, -18.0, Policy::default(), |d| {
        Ok((-20.0 + 0.35 * d, d))
    })
    .unwrap();
    assert_eq!(report.stop_reason, "target-matched");
    assert!(report.probes.len() > 2);
    assert!((-20.0 + 0.35 * drive + 18.0).abs() <= report.policy.tolerance_lu);
    assert_eq!(drive, report.probes[report.selected_probe].drive_db);
    assert!(report.probes.windows(2).all(|w| {
        (w[1].drive_db - w[0].drive_db).abs() <= report.policy.maximum_step_db + 1e-10
    }));
}

#[test]
fn overshoot_is_bracketed_and_can_reduce_drive() {
    for (initial, slope, target) in [(0.0, 4.0, -19.0), (3.0, 1.0, -20.0)] {
        let (drive, report) = search::solve(initial, 10.0, target, Policy::default(), |d| {
            Ok((-20.0 + slope * d, d))
        })
        .unwrap();
        assert_eq!(report.stop_reason, "target-matched");
        assert!((-20.0 + slope * drive - target).abs() <= report.policy.tolerance_lu);
        assert!(report
            .probes
            .windows(2)
            .any(|w| w[1].drive_db < w[0].drive_db));
    }
}

#[test]
fn bounds_and_budget_are_reported_without_claiming_infeasibility() {
    let (best, report) =
        search::solve(0.0, 1.0, -18.0, Policy::default(), |d| Ok((-20.0, d))).unwrap();
    assert_eq!(report.stop_reason, "upper-drive-bound");
    assert_eq!(best, 0.0);
    assert!(report.probes.iter().all(|p| p.drive_db <= 1.0));

    let policy = Policy {
        downward_range_db: 1.0,
        ..Policy::default()
    };
    let (_, report) = search::solve(0.0, 1.0, -30.0, policy, |d| Ok((-20.0 + d, d))).unwrap();
    assert_eq!(report.stop_reason, "lower-drive-bound");
    assert!(report.probes.iter().all(|p| p.drive_db >= -1.0));

    let policy = Policy {
        maximum_evaluations: 2,
        ..Policy::default()
    };
    let (_, report) =
        search::solve(0.0, 10.0, -18.0, policy, |d| Ok((-20.0 + 0.1 * d, d))).unwrap();
    assert_eq!(report.stop_reason, "evaluation-limit");
    assert_eq!(report.probes.len(), 2);
    assert!(report.probes[report.selected_probe].error_lu.abs() > policy.tolerance_lu);
}

#[test]
fn nonmonotonic_response_retains_best_observed_output() {
    let (best, report) =
        search::solve(0.0, 10.0, -18.0, Policy::default(), |d| Ok((-20.0 - d, d))).unwrap();
    assert_eq!(report.stop_reason, "nonmonotonic-response");
    assert_eq!(best, 0.0);
    assert_eq!(report.selected_probe, 0);
}

#[test]
fn exact_target_stops_immediately_and_search_is_repeatable() {
    let run = || search::solve(0.0, 5.0, -20.0, Policy::default(), |d| Ok((-20.0 + d, d))).unwrap();
    let (a, first) = run();
    let (b, second) = run();
    assert_eq!(a, b);
    assert_eq!(first.probes.len(), 1);
    assert_eq!(first.stop_reason, "target-matched");
    assert_eq!(
        serde_json::to_string(&first).unwrap(),
        serde_json::to_string(&second).unwrap()
    );
}

#[test]
fn invalid_settings_and_measurements_fail_explicitly() {
    for policy in [
        Policy {
            maximum_evaluations: 0,
            ..Policy::default()
        },
        Policy {
            tolerance_lu: f64::NAN,
            ..Policy::default()
        },
        Policy {
            maximum_step_db: 0.0,
            ..Policy::default()
        },
    ] {
        assert!(search::solve(
            0.0,
            1.0,
            -20.0,
            policy,
            |_| -> mastering_engine::audio_io::Result<(f64, ())> {
                panic!("Invalid settings must fail before rendering")
            }
        )
        .is_err());
    }
    assert!(search::solve(0.0, 1.0, -20.0, Policy::default(), |_| Ok((f64::NAN, ()))).is_err());
    assert!(search::solve(
        0.0,
        1.0,
        -20.0,
        Policy::default(),
        |_| -> mastering_engine::audio_io::Result<(f64, ())> { Err("Measurement failed".into()) }
    )
    .is_err());
}
