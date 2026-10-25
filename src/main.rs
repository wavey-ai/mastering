use mastering_engine::{
    adjust, analysis,
    audio_io::{self, Result},
    delivery, distributed,
    dsp::{db, Oversampler},
    metrics, set, source_treatments, spectrum, stem_evidence, stereo,
};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    time::Instant,
};

const USAGE: &str = "master set INPUT.wav NEW_OUTPUT_DIRECTORY [--format pcm24|pcm16|float32] [--stems DIRECTORY] [--auditory-analysis ANALYSIS.json] [--album-lufs N] [--deesser true|false]\nmaster set-plan INPUT.wav NEW_PLAN.json [--stems DIRECTORY] [--auditory-analysis ANALYSIS.json]\nmaster render-family INPUT.wav PLAN.json FAMILY_ID NEW_OUTPUT_DIRECTORY\nmaster adjust INPUT.wav PLAN.json VERSIONS.json PARENT_ID NEW_OUTPUT_DIRECTORY [--id ID] [--preset streaming|club|radio|mono|mono-safe] [--bass N] [--presence N] [--air N] [--sibilance N] [--loudness N] [--width N] [--compression N] [--ceiling -N]\nmaster inspect INPUT.wav";

fn write_json(path: &Path, record: &impl serde::Serialize) -> Result<()> {
    let json = serde_json::to_string_pretty(record)?;
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    writeln!(file, "{json}")?;
    Ok(())
}

fn digest(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

type SetOptions = (
    delivery::Format,
    Option<PathBuf>,
    Option<PathBuf>,
    set::Options,
);

fn set_options(args: impl Iterator<Item = String>) -> Result<SetOptions> {
    let mut format = delivery::Format::default();
    let mut stems = None;
    let mut auditory = None;
    let mut options = set::Options::default();
    let mut args = args;
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("Missing value for {flag}"))?;
        match flag.as_str() {
            "--format" => format = value.parse()?,
            "--stems" => stems = Some(PathBuf::from(value)),
            "--auditory-analysis" => auditory = Some(PathBuf::from(value)),
            "--album-lufs" => options.album_target_lufs = Some(value.parse()?),
            "--deesser" => options.deesser = value.parse()?,
            _ => return Err(format!("Unknown set option {flag}").into()),
        }
    }
    Ok((format, stems, auditory, options))
}

fn set_plan_options(
    args: impl Iterator<Item = String>,
) -> Result<(Option<PathBuf>, Option<PathBuf>)> {
    let mut stems = None;
    let mut auditory = None;
    let mut args = args;
    while let Some(flag) = args.next() {
        let value = args
            .next()
            .ok_or_else(|| format!("Missing value for {flag}"))?;
        match flag.as_str() {
            "--stems" => stems = Some(PathBuf::from(value)),
            "--auditory-analysis" => auditory = Some(PathBuf::from(value)),
            _ => return Err(format!("Unknown set-plan option {flag}").into()),
        }
    }
    Ok((stems, auditory))
}

fn make_set_plan(
    input: &Path,
    output: &Path,
    stems: Option<&Path>,
    auditory_path: Option<&Path>,
) -> Result<()> {
    if output.exists() {
        return Err("Set plan exists; choose a new path".into());
    }
    let input_hash = digest(input)?;
    let source = audio_io::read_source(input)?;
    let source_rate = source.sample_rate;
    let source_frames = source.audio.len();
    let source = source.at_render_rate()?;
    let (audio, rate) = (source.audio, source.sample_rate);
    if digest(input)? != input_hash {
        return Err("Original changed while reading".into());
    }
    let pcm_hash = stem_evidence::fingerprint(&audio);
    let auditory_json = auditory_path.map(fs::read).transpose()?;
    let auditory = auditory_json.as_ref().map(|json| set::AuditoryInput {
        json,
        source_pcm_sha256: &pcm_hash,
        source: source_treatments::SourceIdentity {
            file_sha256: &input_hash,
            sample_rate: source_rate,
            frames: source_frames,
        },
    });
    let focus = auditory
        .as_ref()
        .map(|a| {
            source_treatments::propose(
                a.json,
                source_treatments::SourceIdentity {
                    file_sha256: a.source.file_sha256,
                    sample_rate: a.source.sample_rate,
                    frames: a.source.frames,
                },
            )
        })
        .transpose()?;
    let evidence = stems
        .map(|directory| {
            stem_evidence::analyze_set(
                &audio,
                rate,
                |role| audio_io::read_source(&directory.join(format!("{role}.wav"))),
                focus.as_ref(),
                &input_hash,
            )
        })
        .transpose()?;
    let plan = set::prepare(&audio, rate, evidence, auditory)?.worker_plan()?;
    if digest(input)? != input_hash {
        return Err("Original changed during planning".into());
    }
    write_json(output, &plan)
}

struct NativeSetSink<'a> {
    directory: &'a Path,
    /// The render rate of the set, in Hz.
    rate: u32,
    format: delivery::Format,
    track_name: String,
    input_hash: String,
    pcm_hash: String,
    renderer_hash: String,
    progress: fs::File,
    delivered: Vec<serde_json::Value>,
    started: Instant,
    trial_started: Option<Instant>,
}

fn filename_prefix(value: &str, maximum_bytes: usize) -> &str {
    let mut end = value.len().min(maximum_bytes);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    &value[..end]
}

impl set::Sink for NativeSetSink<'_> {
    fn parallel_versions(&self) -> bool {
        true
    }
    fn event(&mut self, event: &'static str, id: &str) -> Result<()> {
        if event == "rendering" {
            self.trial_started = Some(Instant::now());
        }
        writeln!(
            self.progress,
            "{}",
            serde_json::json!({"event":event,"id":id,"elapsed_seconds":self.started.elapsed().as_secs_f64()})
        )?;
        self.progress.sync_data()?;
        if !id.is_empty() {
            eprintln!("{event}: {id}");
        }
        Ok(())
    }
    fn comparisons(&mut self, audio: &[audio_io::Stereo]) -> Result<Vec<set::Residual>> {
        self.delivered
            .iter()
            .map(|version| {
                let reference =
                    audio_io::read_source(&self.directory.join(version["wav"].as_str().unwrap()))?;
                if reference.sample_rate != self.rate {
                    return Err("A delivered version differs in sample rate".into());
                }
                Ok(set::Residual {
                    id: version["id"].as_str().unwrap().into(),
                    matched_residual_db: set::matched_residual_db(&reference.audio, audio)?,
                })
            })
            .collect()
    }
    fn deliver(&mut self, version: &set::Version<'_>, audio: &[audio_io::Stereo]) -> Result<()> {
        // A version that rendered beside an earlier one adds the seconds of
        // that render to those of its turn.
        let render_measure_compare_seconds = self
            .trial_started
            .map(|t| t.elapsed().as_secs_f64() + version.rendered_beside_seconds.unwrap_or(0.0));
        let delivery_started = Instant::now();
        let stem = format!(
            "{} [{}] - {}",
            filename_prefix(&version.label.to_uppercase(), 80),
            version.id,
            filename_prefix(&self.track_name, 96)
        );
        let wav_name = format!("{stem}.wav");
        let report_name = format!("{stem}.json");
        let output = self.directory.join(&wav_name);
        let seed = set::delivery_seed(&self.pcm_hash, version.id);
        delivery::write(&output, audio, self.rate, self.format, seed)?;
        let verification = delivery::verify(
            &output,
            audio,
            self.rate,
            self.format,
            version.spec.ceiling_dbtp,
        )?;
        let output_hash = digest(&output)?;
        let timing = serde_json::json!({"render_measure_compare_seconds":render_measure_compare_seconds,
            "wav_write_verify_hash_seconds":delivery_started.elapsed().as_secs_f64()});
        write_json(
            &self.directory.join(&report_name),
            &serde_json::json!({
                "schema":"mastering.set-version-delivery.2","id":version.id,"label":version.label,
                "description":version.description,"rule":version.rule,"basis":version.basis,
                "input_sha256":self.input_hash,"source_pcm_sha256":self.pcm_hash,"renderer_sha256":self.renderer_hash,
                "output_sha256":output_hash,"delivery":verification,"spec":version.spec,"report":version.report,
                "refinement":version.refinement,"timing":timing
            }),
        )?;
        self.delivered.push(serde_json::json!({
            "id":version.id,"label":version.label,"description":version.description,"rule":version.rule,
            "basis":version.basis,"wav":wav_name,"report":report_name,"output_sha256":output_hash,"delivery":verification,"timing":timing
        }));
        self.event("version-complete", version.id)?;
        Ok(())
    }
    fn name(&mut self, named: &[mastering_engine::version_names::Named]) -> Result<()> {
        for n in named.iter().filter(|n| !n.distinct) {
            // A withdrawn version leaves the set with its files.
            if let Some(i) = self.delivered.iter().position(|v| v["id"] == n.id.as_str()) {
                let entry = self.delivered.remove(i);
                fs::remove_file(self.directory.join(entry["wav"].as_str().unwrap()))?;
                fs::remove_file(self.directory.join(entry["report"].as_str().unwrap()))?;
            }
        }
        for n in named.iter().filter(|n| n.distinct) {
            let Some(entry) = self.delivered.iter_mut().find(|v| v["id"] == n.id.as_str()) else {
                continue;
            };
            let stem = format!(
                "{} [{}] - {}",
                filename_prefix(&n.name.to_uppercase(), 80),
                n.id,
                filename_prefix(&self.track_name, 96)
            );
            let (wav_name, report_name) = (format!("{stem}.wav"), format!("{stem}.json"));
            let old_report = self.directory.join(entry["report"].as_str().unwrap());
            let mut report: serde_json::Value = serde_json::from_slice(&fs::read(&old_report)?)?;
            let detail = report["description"].clone();
            report["label"] = n.name.into();
            report["description"] = n.description.clone().into();
            report["detail"] = detail.clone();
            report["naming"] = serde_json::to_value(n)?;
            fs::rename(
                self.directory.join(entry["wav"].as_str().unwrap()),
                self.directory.join(&wav_name),
            )?;
            fs::remove_file(&old_report)?;
            write_json(&self.directory.join(&report_name), &report)?;
            entry["label"] = n.name.into();
            entry["description"] = n.description.clone().into();
            entry["detail"] = detail;
            entry["wav"] = wav_name.into();
            entry["report"] = report_name.into();
        }
        Ok(())
    }
}

fn render_set(
    input: &Path,
    directory: &Path,
    format: delivery::Format,
    stems: Option<&Path>,
    auditory_path: Option<&Path>,
    options: set::Options,
) -> Result<()> {
    use set::Sink;
    if directory.exists() {
        return Err("Set output directory exists; choose a new directory".into());
    }
    let started = Instant::now();
    let input_hash = digest(input)?;
    let source = audio_io::read_source(input)?;
    let source_rate = source.sample_rate;
    let source_frames = source.audio.len();
    let source = source.at_render_rate()?;
    let (audio, rate) = (source.audio, source.sample_rate);
    if digest(input)? != input_hash {
        return Err("Original changed while reading".into());
    }
    let pcm_hash = stem_evidence::fingerprint(&audio);
    let auditory_json = auditory_path.map(fs::read).transpose()?;
    let auditory = auditory_json.as_ref().map(|json| set::AuditoryInput {
        json,
        source_pcm_sha256: &pcm_hash,
        source: source_treatments::SourceIdentity {
            file_sha256: &input_hash,
            sample_rate: source_rate,
            frames: source_frames,
        },
    });
    // Reject mismatched optional diagnostics before creating any output.
    let focus = if let Some(a) = &auditory {
        Some(source_treatments::propose(
            a.json,
            source_treatments::SourceIdentity {
                file_sha256: a.source.file_sha256,
                sample_rate: a.source.sample_rate,
                frames: a.source.frames,
            },
        )?)
    } else {
        None
    };
    let evidence = stems
        .map(|p| {
            stem_evidence::analyze_set(
                &audio,
                rate,
                |role| audio_io::read_source(&p.join(format!("{role}.wav"))),
                focus.as_ref(),
                &input_hash,
            )
        })
        .transpose()?;
    let renderer_hash = digest(&std::env::current_exe()?)?;
    let preparation_seconds = started.elapsed().as_secs_f64();
    fs::create_dir(directory)?;
    let mut sink = NativeSetSink {
        directory,
        rate,
        format,
        track_name: input
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("mix")
            .into(),
        input_hash: input_hash.clone(),
        pcm_hash: pcm_hash.clone(),
        renderer_hash: renderer_hash.clone(),
        progress: fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join("progress.jsonl"))?,
        delivered: Vec::new(),
        started,
        trial_started: None,
    };
    sink.event("started", "")?;
    let result = (|| -> Result<()> {
        // Preserve the exact source bytes for the audition deck, including native rate.
        let mut original = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join("ORIGINAL.wav"))?;
        std::io::copy(&mut fs::File::open(input)?, &mut original)?;
        original.sync_all()?;
        if digest(&directory.join("ORIGINAL.wav"))? != input_hash {
            return Err("Original copy fingerprint failed".into());
        }
        let summary = set::render_with(&audio, rate, evidence, auditory, options, &mut sink)?;
        if digest(input)? != input_hash {
            return Err("Original changed during rendering".into());
        }
        write_json(&directory.join("analysis-and-trials.json"), &summary)?;
        let mut page = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(directory.join("index.html"))?;
        page.write_all(include_bytes!("../scripts/set-review.html"))?;
        write_json(
            &directory.join("set.json"),
            &serde_json::json!({
                "schema":"mastering.native-set-delivery.2","status":"complete",
                "input_file":input,"input_sha256":input_hash,"source_sample_rate":source_rate,"source_frames":source_frames,"sample_rate":rate,
                "source_pcm_sha256":summary.source.source_pcm_sha256,"renderer_sha256":renderer_hash,
                "render_seconds":started.elapsed().as_secs_f64(),"preparation_seconds":preparation_seconds,"versions":sink.delivered,
                "original":{"id":"original","label":"Original","wav":"ORIGINAL.wav","integrated_lufs":summary.source.input.integrated_lufs,"sha256":input_hash},
                "evidence":"analysis-and-trials.json","trial_count":summary.trials.len(),
                "density":summary.density,"source_condition":summary.source_condition,"work":summary.work
            }),
        )?;
        sink.event("complete", "")?;
        println!(
            "{}",
            serde_json::json!({"versions":sink.delivered.len(),"output":directory,"seconds":started.elapsed().as_secs_f64()})
        );
        Ok(())
    })();
    if let Err(error) = &result {
        let _ = writeln!(
            sink.progress,
            "{}",
            serde_json::json!({"event":"failed","reason":error.to_string()})
        );
        let _ = sink.progress.sync_data();
    }
    result
}

fn run() -> Result<()> {
    let mut args = std::env::args().skip(1);
    let command = args.next().ok_or(USAGE)?;
    if command == "--help" || command == "help" {
        println!("{USAGE}");
        return Ok(());
    }
    if command == "set" {
        let input = PathBuf::from(args.next().ok_or("Missing input WAV")?);
        let directory = PathBuf::from(args.next().ok_or("Missing set output directory")?);
        let (format, stems, auditory, options) = set_options(args)?;
        return render_set(
            &input,
            &directory,
            format,
            stems.as_deref(),
            auditory.as_deref(),
            options,
        );
    }
    if command == "set-plan" {
        let input = PathBuf::from(args.next().ok_or("Missing input WAV")?);
        let output = PathBuf::from(args.next().ok_or("Missing set plan JSON")?);
        let (stems, auditory) = set_plan_options(args)?;
        return make_set_plan(&input, &output, stems.as_deref(), auditory.as_deref());
    }
    if command == "render-family" {
        let input = PathBuf::from(args.next().ok_or("Missing input WAV")?);
        let plan = PathBuf::from(args.next().ok_or("Missing worker plan JSON")?);
        let family = args.next().ok_or("Missing family ID")?;
        let output = PathBuf::from(args.next().ok_or("Missing family output directory")?);
        if args.next().is_some() || output.exists() {
            return Err("Render-family requires a new output directory".into());
        }
        let source = audio_io::read(&input)?;
        let plan_json = fs::read(plan)?;
        fs::create_dir(&output)?;
        let result = distributed::render_family(
            &source.audio,
            source.sample_rate,
            &plan_json,
            &family,
            &output,
        )?;
        write_json(&output.join("family.json"), &result)?;
        println!("{}", serde_json::to_string(&result)?);
        return Ok(());
    }
    if command == "adjust" {
        let input = PathBuf::from(args.next().ok_or("Missing input WAV")?);
        let plan = PathBuf::from(args.next().ok_or("Missing worker plan JSON")?);
        let versions = PathBuf::from(args.next().ok_or("Missing versions JSON")?);
        let parent = args.next().ok_or("Missing parent version ID")?;
        let output = PathBuf::from(args.next().ok_or("Missing adjust output directory")?);
        if output.exists() {
            return Err("Adjust requires a new output directory".into());
        }
        let mut request = adjust::Request {
            id: format!("{parent}-adjusted"),
            parent,
            steps: adjust::Steps::default(),
            preset: None,
        };
        while let Some(flag) = args.next() {
            let value = args.next().ok_or("Missing option value")?;
            let steps = &mut request.steps;
            let slot = match flag.as_str() {
                "--id" => {
                    request.id = value;
                    continue;
                }
                "--preset" => {
                    request.preset = Some(adjust::Preset::parse(&value).ok_or("Unknown preset")?);
                    continue;
                }
                "--bass" => &mut steps.bass,
                "--presence" => &mut steps.presence,
                "--air" => &mut steps.air,
                "--sibilance" => &mut steps.sibilance,
                "--loudness" => &mut steps.loudness,
                "--width" => &mut steps.width,
                "--compression" => &mut steps.compression,
                "--ceiling" => &mut steps.ceiling,
                _ => return Err(USAGE.into()),
            };
            *slot = value.parse().map_err(|_| "A step is a whole number")?;
        }
        let source = audio_io::read(&input)?;
        let plan: serde_json::Value = serde_json::from_slice(&fs::read(plan)?)?;
        let base = adjust::Base::of_plan(&plan)?;
        let versions_dir = versions.parent().unwrap_or(Path::new(".")).to_path_buf();
        let versions: serde_json::Value = serde_json::from_slice(&fs::read(versions)?)?;
        fs::create_dir(&output)?;
        let started = Instant::now();
        if source.sample_rate != base.sample_rate() {
            return Err("The plan belongs to another render rate".into());
        }
        let mut result = adjust::render(
            &source.audio,
            &base,
            &versions,
            &versions_dir,
            &request,
            &output,
        )?;
        result["seconds"] = started.elapsed().as_secs_f64().into();
        write_json(&output.join("adjust.json"), &result)?;
        let mut summary = result.clone();
        summary["version"] = serde_json::Value::Null;
        summary["candidates"] = serde_json::Value::Null;
        println!("{}", serde_json::to_string_pretty(&summary)?);
        return Ok(());
    }
    if command != "inspect" {
        return Err(USAGE.into());
    }
    let input = PathBuf::from(args.next().ok_or("Missing input WAV")?);
    if args.next().is_some() {
        return Err("inspect takes only an input WAV".into());
    }
    let source = audio_io::read_source(&input)?;
    let source_rate = source.sample_rate;
    let source_frames = source.audio.len();
    let source = source.at_render_rate()?;
    let (audio, rate) = (source.audio, source.sample_rate);
    let hash = digest(&input)?;
    println!(
        "{}",
        serde_json::to_string_pretty(
            &serde_json::json!({"input_file":input,"input_sha256":hash,"source_sample_rate":source_rate,"source_frames":source_frames,"analysis_sample_rate":rate,
        "metrics":metrics(&audio, rate),"spectrum":spectrum(&audio, rate),"temporal":analysis::analyze(&audio, rate),
        "stereo":stereo::analyze(&audio, rate)?,
        "peak_estimate_dbtp":db(Oversampler::new(rate).peak(&audio))})
        )?
    );
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
