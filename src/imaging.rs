//! Measured range checks that the set reports for each candidate.
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Serialize)]
pub struct Check {
    pub id: &'static str,
    pub label: &'static str,
    pub unit: &'static str,
    pub measured: Option<f64>,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub passed: bool,
    pub band_index: Option<usize>,
    pub window_start_sample: Option<usize>,
}

/// A check of a record, such as the trials of a family that continues in
/// another invocation.
impl<'de> Deserialize<'de> for Check {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Record {
            id: String,
            label: String,
            unit: String,
            measured: Option<f64>,
            minimum: Option<f64>,
            maximum: Option<f64>,
            passed: bool,
            band_index: Option<usize>,
            window_start_sample: Option<usize>,
        }
        let r = Record::deserialize(deserializer)?;
        Ok(Self {
            id: name(r.id),
            label: name(r.label),
            unit: name(r.unit),
            measured: r.measured,
            minimum: r.minimum,
            maximum: r.maximum,
            passed: r.passed,
            band_index: r.band_index,
            window_start_sample: r.window_start_sample,
        })
    }
}

/// A name of a check read from a record. The checks name a small set of
/// ids, labels and units, so each distinct name is kept once.
fn name(owned: String) -> &'static str {
    static NAMES: OnceLock<Mutex<HashSet<&'static str>>> = OnceLock::new();
    let mut names = NAMES
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    match names.get(owned.as_str()) {
        Some(&kept) => kept,
        None => {
            let kept: &'static str = Box::leak(owned.into_boxed_str());
            names.insert(kept);
            kept
        }
    }
}

impl Check {
    pub(crate) fn range(
        id: &'static str,
        label: &'static str,
        unit: &'static str,
        measured: Option<f64>,
        minimum: Option<f64>,
        maximum: Option<f64>,
    ) -> Self {
        let measured = measured.filter(|v| v.is_finite());
        Self {
            id,
            label,
            unit,
            measured,
            minimum,
            maximum,
            passed: measured.is_some_and(|v| {
                minimum.is_none_or(|low| v >= low) && maximum.is_none_or(|high| v <= high)
            }),
            band_index: None,
            window_start_sample: None,
        }
    }
}
