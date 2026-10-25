//! Measured range checks that the set reports for each candidate.
use serde::Serialize;

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
