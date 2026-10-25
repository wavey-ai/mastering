//! Stages of a set for a host that shows them while the set renders. The
//! reports go beside the results: a stage changes no audio and no record,
//! and without a reporter `report` does nothing.
//!
//! The stages, in the order of a set with stems:
//!
//! - `stem`: one stem of the set is read and measured (`part`, `index` of
//!   `count`).
//! - `stems`: the stems are added together and compared with the mix.
//! - `focus`: the vocal and lead of the mix are found from the stems.
//! - `mix`: the loudness, spectrum, dynamics and stereo image of the mix.
//! - `condition`: how the mix was recorded: a codec cut, a capture, channels
//!   that cancel.
//! - `audibility`: peak control at a range of drives, for the loudness at
//!   which the limiter and the clipper become audible.
//! - `level`: the render of the mix at the target loudness.
//! - `width`: the stereo widening that the mix allows.
//! - `attack`: the limiter attack.
//! - `tone`: the tone of each version.
//! - `plan`: the planned versions, in render order (`versions`).
use serde::Serialize;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Debug, Default, Serialize)]
pub struct Stage {
    pub stage: &'static str,
    /// The stem of a `stem` stage: drums, bass, other or vocals.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub part: Option<&'static str>,
    /// The place of the step among the steps of its kind, from 1.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
    /// The number of steps of its kind.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub count: Option<usize>,
    /// The versions of a `plan` stage, with their planned labels.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub versions: Vec<PlannedVersion>,
}

#[derive(Clone, Debug, Serialize)]
pub struct PlannedVersion {
    pub id: String,
    pub label: String,
}

type Reporter = Box<dyn Fn(&Stage) + Send + Sync>;

static REPORTER: OnceLock<Mutex<Option<Reporter>>> = OnceLock::new();

/// Send each later stage to `reporter`. `None` stops the reports.
pub fn set_reporter(reporter: Option<Reporter>) {
    let slot = REPORTER.get_or_init(|| Mutex::new(None));
    *slot.lock().unwrap_or_else(|e| e.into_inner()) = reporter;
}

/// Give `stage` to the reporter, when the host set one.
pub fn report(stage: Stage) {
    let Some(slot) = REPORTER.get() else {
        return;
    };
    if let Ok(reporter) = slot.lock() {
        if let Some(reporter) = reporter.as_ref() {
            reporter(&stage);
        }
    }
}

/// Report the stage `stage`, which has no detail.
pub fn step(stage: &'static str) {
    report(Stage {
        stage,
        ..Stage::default()
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn a_reporter_receives_the_stages_until_it_is_removed() {
        // Other tests of this binary can report at the same time; this test
        // counts only its own stages.
        let seen = Arc::new(Mutex::new(Vec::new()));
        let sink = seen.clone();
        set_reporter(Some(Box::new(move |stage| {
            if stage.stage.starts_with("progress-test") {
                sink.lock()
                    .unwrap()
                    .push(serde_json::to_value(stage).unwrap());
            }
        })));
        step("progress-test-a");
        report(Stage {
            stage: "progress-test-b",
            part: Some("bass"),
            index: Some(2),
            count: Some(4),
            ..Stage::default()
        });
        set_reporter(None);
        step("progress-test-c");
        let seen = seen.lock().unwrap();
        assert_eq!(seen.len(), 2);
        assert_eq!(seen[0], serde_json::json!({"stage": "progress-test-a"}));
        assert_eq!(
            seen[1],
            serde_json::json!({"stage": "progress-test-b", "part": "bass", "index": 2, "count": 4})
        );
    }
}
