use std::process::Command;

#[test]
fn help_lists_commands() {
    let result = Command::new(env!("CARGO_BIN_EXE_master"))
        .arg("--help")
        .output()
        .unwrap();
    assert!(result.status.success());
    let text = String::from_utf8(result.stdout).unwrap();
    for command in [
        "master set ",
        "master set-plan ",
        "master render-family ",
        "master adjust ",
        "master inspect ",
    ] {
        assert!(text.contains(command));
    }
    assert!(!text.contains("--reference"));
    // Width is automatic in a set; only a request on a delivered version
    // changes it (D171).
    for line in text.lines().filter(|l| !l.starts_with("master adjust ")) {
        assert!(!line.contains("--width"), "{line}");
    }
    assert!(text.contains("--width N"));
}

#[test]
fn set_cli_writes_verified_versions_and_refuses_replacement() {
    let root = std::env::temp_dir().join(format!(
        "master-set-cli-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&root).unwrap();
    struct Cleanup(std::path::PathBuf);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let cleanup = Cleanup(root);
    let input = cleanup.0.join("mix.wav");
    let output = cleanup.0.join("masters");
    let audio = mastering_engine::bench::tone(997.0, 0.1, 144000, 48000);
    mastering_engine::audio_io::write(&input, &audio, 48000).unwrap();
    let original_bytes = std::fs::read(&input).unwrap();
    let render = || {
        Command::new(env!("CARGO_BIN_EXE_master"))
            .arg("set")
            .arg(&input)
            .arg(&output)
            .output()
            .unwrap()
    };
    let first = render();
    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let manifest: serde_json::Value =
        serde_json::from_slice(&std::fs::read(output.join("set.json")).unwrap()).unwrap();
    assert_eq!(manifest["schema"], "mastering.native-set-delivery.2");
    assert_eq!(manifest["status"], "complete");
    let versions = manifest["versions"].as_array().unwrap();
    assert!(!versions.is_empty());
    assert_eq!(versions[0]["id"], "essential");
    assert!(output.join("index.html").is_file());
    assert!(output.join("analysis-and-trials.json").is_file());
    assert_eq!(
        std::fs::read(output.join("ORIGINAL.wav")).unwrap(),
        original_bytes
    );
    for version in versions {
        let wav = output.join(version["wav"].as_str().unwrap());
        let report = output.join(version["report"].as_str().unwrap());
        assert!(wav.is_file() && report.is_file());
        assert_eq!(version["delivery"]["verified"], true);
        let detail: serde_json::Value =
            serde_json::from_slice(&std::fs::read(report).unwrap()).unwrap();
        assert_eq!(detail["id"], version["id"]);
        assert_eq!(detail["schema"], "mastering.set-version-delivery.2");
        assert_eq!(detail["source_pcm_sha256"], manifest["source_pcm_sha256"]);
    }
    assert!(std::fs::read_to_string(output.join("progress.jsonl"))
        .unwrap()
        .contains("\"event\":\"complete\""));
    assert_eq!(std::fs::read(&input).unwrap(), original_bytes);
    let again = render();
    assert!(!again.status.success());
    assert!(String::from_utf8_lossy(&again.stderr).contains("directory exists"));
    assert_eq!(std::fs::read(&input).unwrap(), original_bytes);
}

#[test]
fn set_cli_rejects_sound_controls_before_reading_input() {
    let result = Command::new(env!("CARGO_BIN_EXE_master"))
        .args(["set", "missing.wav", "new-masters", "--character", "warm"])
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("Unknown set option --character"));
}

#[test]
fn inspect_exposes_stereo_measurements_without_changing_source() {
    let path =
        std::env::temp_dir().join(format!("mastering-stereo-cli-{}.wav", std::process::id()));
    assert!(!path.exists());
    struct Source(std::path::PathBuf);
    impl Drop for Source {
        fn drop(&mut self) {
            let _ = std::fs::remove_file(&self.0);
        }
    }
    let source = Source(path);
    let audio: Vec<_> = (0..144000)
        .map(|i| {
            let x = (0.1 * (std::f64::consts::TAU * 1000.0 * i as f64 / 48000.0).sin()) as f32;
            [x, -x]
        })
        .collect();
    mastering_engine::audio_io::write(&source.0, &audio, 48000).unwrap();
    let before = std::fs::read(&source.0).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_master"))
        .arg("inspect")
        .arg(&source.0)
        .output()
        .unwrap();
    assert!(result.status.success(), "{:?}", result.stderr);
    let json: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    let report = &json["stereo"];
    assert_eq!(report["schema"], "mastering.stereo.1");
    assert_eq!(report["short_windows"].as_array().unwrap().len(), 6);
    assert_eq!(report["whole"]["bands"][2]["correlation"], -1.0);
    assert_eq!(report["whole"]["bands"][2]["mono_fraction"], 0.0);
    assert_eq!(std::fs::read(&source.0).unwrap(), before);
}

#[test]
fn invalid_delivery_options_fail_before_reading_sources() {
    for args in [
        vec!["set", "missing.wav", "new-masters", "--format", "mp3"],
        vec![
            "set-plan",
            "missing.wav",
            "new-plan.json",
            "--format",
            "pcm24",
        ],
        vec!["inspect", "missing.wav", "--format", "pcm24"],
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_master"))
            .args(args)
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(!String::from_utf8_lossy(&result.stderr).contains("No such file"));
    }
}
