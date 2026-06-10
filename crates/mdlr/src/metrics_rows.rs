//! Metric row collection for CLI output.

use crate::cache::Ignores;
use crate::config::{Bucket, Config, MetricThresholds, TwoSidedThresholds};
use mdlr_cpd::DuplicationMetrics;
use mdlr_metrics::{
    ComplexityMetrics, CoverageMetrics, FileLocMetrics, HubInfo,
    SortDirection, StructMetrics, StructuralMetrics,
};
use std::collections::HashMap;

/// A metric row: (metric_name, symbol, value, bucket)
pub type MetricRow = (String, String, String, String);

/// Internal representation with bucket for sorting
struct ScoredRow {
    metric_name: String,
    symbol: String,
    value: String,
    bucket: Bucket,
}

impl ScoredRow {
    /// Convert to MetricRow for output
    fn into_row(self) -> MetricRow {
        (self.metric_name, self.symbol, self.value, self.bucket.to_string())
    }

    /// Severity score for sorting (higher = worse)
    fn severity(&self) -> u8 {
        match self.bucket {
            Bucket::Excellent => 0,
            Bucket::Good => 1,
            Bucket::Fair => 2,
            Bucket::Poor => 3,
            Bucket::Critical => 4,
        }
    }
}

/// Bundle of all computed metrics for collection
pub struct MetricsBundle<'a> {
    pub structural: &'a StructuralMetrics,
    pub complexity: &'a ComplexityMetrics,
    pub struct_metrics: &'a StructMetrics,
    pub file_loc: &'a FileLocMetrics,
    pub duplication: &'a DuplicationMetrics,
    /// Present iff the user passed `--cov`.
    pub coverage: Option<&'a CoverageMetrics>,
}

/// Specification for collecting an integer metric.
///
/// `direction` controls both the threshold evaluation (which end of the
/// range is "worse") and which side of `boring_threshold` to keep:
/// - `Desc`: keep entries with `value > boring_threshold` (boring = small).
/// - `Asc`:  keep entries with `value < boring_threshold` (boring = large).
struct IntMetricSpec<'a> {
    name: &'static str,
    distribution: &'a [(String, usize)],
    thresholds: &'a MetricThresholds,
    boring_threshold: usize,
    direction: SortDirection,
}

impl IntMetricSpec<'_> {
    fn is_interesting(&self, value: usize) -> bool {
        match self.direction {
            SortDirection::Desc => value > self.boring_threshold,
            SortDirection::Asc => value < self.boring_threshold,
        }
    }

    fn bucket_for(&self, value: usize) -> Bucket {
        match self.direction {
            SortDirection::Desc => self.thresholds.evaluate(value as f64),
            SortDirection::Asc => self.thresholds.evaluate_asc(value as f64),
        }
    }

    fn format_value(&self, value: usize) -> String {
        match self.name {
            "line_cov" => format!("{value}%"),
            _ => value.to_string(),
        }
    }

    /// Collect all entries (for global sorting mode)
    fn collect_all(&self, rows: &mut Vec<ScoredRow>) {
        for (name, value) in self.distribution {
            if self.is_interesting(*value) {
                let bucket = self.bucket_for(*value);
                rows.push(ScoredRow {
                    metric_name: self.name.to_string(),
                    symbol: name.clone(),
                    value: self.format_value(*value),
                    bucket,
                });
            }
        }
    }

    /// Collect with per-metric limit (for symbol filter mode)
    fn collect_filtered(
        &self,
        rows: &mut Vec<MetricRow>,
        symbol_filter: &str,
    ) {
        for (name, value) in self.distribution.iter() {
            if name == symbol_filter && self.is_interesting(*value) {
                let bucket = self.bucket_for(*value);
                rows.push((
                    self.name.to_string(),
                    name.clone(),
                    self.format_value(*value),
                    bucket.to_string(),
                ));
            }
        }
    }
}

/// Specification for collecting `function_size`, the only two-sided metric:
/// both extremes are bad. The high side always applies; the low side applies
/// only to units with exactly one visible caller (`fan_in == 1`) — the
/// single-caller pass-through case where "inline into the caller" is
/// well-defined. `fan_in == 0` (callers unknown to the graph: trait dispatch,
/// pub API, entry points) and `fan_in >= 2` (shared helpers) are exempt and
/// evaluated against the high side only.
struct TwoSidedSizeSpec<'a> {
    distribution: &'a [(String, usize)],
    thresholds: &'a TwoSidedThresholds,
    fan_in: HashMap<&'a str, usize>,
}

impl TwoSidedSizeSpec<'_> {
    fn low_side_applies(&self, symbol: &str) -> bool {
        self.fan_in.get(symbol).copied().unwrap_or(0) == 1
    }

    /// Boring = 1-liners that are exempt from the low side (the high-side
    /// `value > 1` rule that applied before the metric became two-sided).
    fn is_interesting(&self, symbol: &str, value: usize) -> bool {
        value > 1 || self.low_side_applies(symbol)
    }

    fn bucket_for(&self, symbol: &str, value: usize) -> Bucket {
        if self.low_side_applies(symbol) {
            self.thresholds.evaluate(value as f64)
        } else {
            self.thresholds.high.evaluate(value as f64)
        }
    }

    /// Collect all entries (for global sorting mode)
    fn collect_all(&self, rows: &mut Vec<ScoredRow>) {
        for (name, value) in self.distribution {
            if self.is_interesting(name, *value) {
                rows.push(ScoredRow {
                    metric_name: "function_size".to_string(),
                    symbol: name.clone(),
                    value: value.to_string(),
                    bucket: self.bucket_for(name, *value),
                });
            }
        }
    }

    /// Collect with per-metric limit (for symbol filter mode)
    fn collect_filtered(
        &self,
        rows: &mut Vec<MetricRow>,
        symbol_filter: &str,
    ) {
        for (name, value) in self.distribution.iter() {
            if name == symbol_filter && self.is_interesting(name, *value) {
                rows.push((
                    "function_size".to_string(),
                    name.clone(),
                    value.to_string(),
                    self.bucket_for(name, *value).to_string(),
                ));
            }
        }
    }
}

/// Specification for collecting fan_in metric with hub filtering
/// Only includes units that are hubs (high fan_in AND high fan_out)
struct HubFilteredFanInSpec<'a> {
    distribution: &'a [(String, usize)],
    thresholds: &'a MetricThresholds,
    hubs: &'a HashMap<String, HubInfo>,
}

impl HubFilteredFanInSpec<'_> {
    /// Collect only hub entries (for global sorting mode)
    fn collect_all(&self, rows: &mut Vec<ScoredRow>) {
        for (name, value) in self.distribution {
            // Only include if this unit is a hub
            if self.hubs.contains_key(name) {
                let bucket = self.thresholds.evaluate(*value as f64);
                rows.push(ScoredRow {
                    metric_name: "fan_in".to_string(),
                    symbol: name.clone(),
                    value: value.to_string(),
                    bucket,
                });
            }
        }
    }

    /// Collect with per-metric limit (for symbol filter mode)
    /// In symbol filter mode, always show the value regardless of hub status
    fn collect_filtered(
        &self,
        rows: &mut Vec<MetricRow>,
        symbol_filter: &str,
    ) {
        for (name, value) in self.distribution.iter() {
            if name == symbol_filter {
                let bucket = self.thresholds.evaluate(*value as f64);
                rows.push((
                    "fan_in".to_string(),
                    name.clone(),
                    value.to_string(),
                    bucket.to_string(),
                ));
            }
        }
    }
}

/// Specification for collecting a float metric
struct FloatMetricSpec<'a> {
    name: &'static str,
    distribution: &'a [(String, f64)],
    thresholds: &'a MetricThresholds,
    min_value: f64,
}

impl FloatMetricSpec<'_> {
    /// Collect all entries (for global sorting mode)
    fn collect_all(&self, rows: &mut Vec<ScoredRow>) {
        for (name, value) in self.distribution {
            if *value > self.min_value {
                let bucket = self.thresholds.evaluate(*value);
                rows.push(ScoredRow {
                    metric_name: self.name.to_string(),
                    symbol: name.clone(),
                    value: format!("{:.2}", value),
                    bucket,
                });
            }
        }
    }

    /// Collect with per-metric limit (for symbol filter mode)
    fn collect_filtered(
        &self,
        rows: &mut Vec<MetricRow>,
        symbol_filter: &str,
    ) {
        for (name, value) in self.distribution.iter() {
            if name == symbol_filter && *value > self.min_value {
                let bucket = self.thresholds.evaluate(*value);
                rows.push((
                    self.name.to_string(),
                    name.clone(),
                    format!("{:.2}", value),
                    bucket.to_string(),
                ));
            }
        }
    }
}

/// Bundled metric specifications for collection
struct MetricSpecs<'a> {
    int_specs: Vec<IntMetricSpec<'a>>,
    function_size_spec: Option<TwoSidedSizeSpec<'a>>,
    fan_in_spec: Option<HubFilteredFanInSpec<'a>>,
    lcom_spec: Option<IntMetricSpec<'a>>,
    float_specs: Vec<FloatMetricSpec<'a>>,
}

impl<'a> MetricSpecs<'a> {
    fn new(m: &'a MetricsBundle, config: &'a Config) -> Self {
        let t = &config.thresholds;
        let mut int_specs = vec![
            IntMetricSpec {
                name: "fan_out",
                distribution: &m.structural.fan_out.distribution,
                thresholds: &t.fan_out_max,
                boring_threshold: 0,
                direction: SortDirection::Desc,
            },
            IntMetricSpec {
                name: "params",
                distribution: &m.complexity.params.distribution,
                thresholds: &t.params,
                boring_threshold: 0,
                direction: SortDirection::Desc,
            },
            IntMetricSpec {
                name: "cyclomatic",
                distribution: &m.complexity.cyclomatic.distribution,
                thresholds: &t.cyclomatic,
                boring_threshold: 1,
                direction: SortDirection::Desc,
            },
            IntMetricSpec {
                name: "cognitive",
                distribution: &m.complexity.cognitive.distribution,
                thresholds: &t.cognitive,
                boring_threshold: 1,
                direction: SortDirection::Desc,
            },
            IntMetricSpec {
                name: "max_scope",
                distribution: &m.complexity.max_scope.distribution,
                thresholds: &t.max_scope,
                boring_threshold: 0,
                direction: SortDirection::Desc,
            },
            IntMetricSpec {
                name: "methods_per_struct",
                distribution: &m
                    .struct_metrics
                    .methods_per_struct
                    .distribution,
                thresholds: &t.methods_per_struct,
                boring_threshold: 0,
                direction: SortDirection::Desc,
            },
            IntMetricSpec {
                name: "file_loc",
                distribution: &m.file_loc.distribution,
                thresholds: &t.file_loc,
                boring_threshold: 0,
                direction: SortDirection::Desc,
            },
            IntMetricSpec {
                name: "duplication_pct",
                distribution: &m.duplication.distribution,
                thresholds: &t.duplication_pct,
                boring_threshold: 0,
                direction: SortDirection::Desc,
            },
        ];
        if let Some(cov) = m.coverage {
            // Skip 100% covered. Use 101 so a Unit at exactly 100% drops out
            // but everything below stays in.
            int_specs.push(IntMetricSpec {
                name: "line_cov",
                distribution: &cov.line_cov.distribution,
                thresholds: &t.line_cov,
                boring_threshold: 100,
                direction: SortDirection::Asc,
            });
            if cov.has_branches {
                int_specs.push(IntMetricSpec {
                    name: "uncov_branches",
                    distribution: &cov.uncov_branches.distribution,
                    thresholds: &t.uncov_branches,
                    boring_threshold: 0,
                    direction: SortDirection::Desc,
                });
            }
        }
        // Disabling is output-control: drop specs for disabled metrics so they
        // never reach text rows or the symbol view.
        int_specs.retain(|spec| !config.is_disabled(spec.name));

        let function_size_spec =
            (!config.is_disabled("function_size")).then(|| TwoSidedSizeSpec {
                distribution: &m.complexity.size.distribution,
                thresholds: &t.function_size,
                fan_in: m
                    .structural
                    .fan_in
                    .distribution
                    .iter()
                    .map(|(id, v)| (id.as_str(), *v))
                    .collect(),
            });
        let fan_in_spec =
            (!config.is_disabled("fan_in")).then(|| HubFilteredFanInSpec {
                distribution: &m.structural.fan_in.distribution,
                thresholds: &t.fan_in_max,
                hubs: &m.structural.hubs,
            });
        let lcom_spec = (!config.is_disabled("lcom")).then(|| IntMetricSpec {
            name: "lcom",
            distribution: &m.struct_metrics.lcom.distribution,
            thresholds: &t.lcom,
            boring_threshold: 0,
            direction: SortDirection::Desc,
        });

        MetricSpecs {
            int_specs,
            function_size_spec,
            fan_in_spec,
            lcom_spec,
            float_specs: vec![],
        }
    }

    fn collect_filtered(&self, filter: &str) -> Vec<MetricRow> {
        let mut rows = Vec::new();
        for spec in &self.int_specs {
            spec.collect_filtered(&mut rows, filter);
        }
        if let Some(function_size_spec) = &self.function_size_spec {
            function_size_spec.collect_filtered(&mut rows, filter);
        }
        if let Some(fan_in_spec) = &self.fan_in_spec {
            fan_in_spec.collect_filtered(&mut rows, filter);
        }
        if let Some(lcom_spec) = &self.lcom_spec {
            lcom_spec.collect_filtered(&mut rows, filter);
        }
        for spec in &self.float_specs {
            spec.collect_filtered(&mut rows, filter);
        }
        rows
    }

    fn collect_all_scored(&self) -> Vec<ScoredRow> {
        let mut rows = Vec::new();
        for spec in &self.int_specs {
            spec.collect_all(&mut rows);
        }
        if let Some(function_size_spec) = &self.function_size_spec {
            function_size_spec.collect_all(&mut rows);
        }
        if let Some(fan_in_spec) = &self.fan_in_spec {
            fan_in_spec.collect_all(&mut rows);
        }
        if let Some(lcom_spec) = &self.lcom_spec {
            lcom_spec.collect_all(&mut rows);
        }
        for spec in &self.float_specs {
            spec.collect_all(&mut rows);
        }
        rows
    }
}

/// Canonical metric display order
const METRIC_ORDER: &[&str] = &[
    "fan_out",
    "fan_in",
    "function_size",
    "params",
    "cyclomatic",
    "cognitive",
    "max_scope",
    "methods_per_struct",
    "file_loc",
    "duplication_pct",
    "lcom",
    "line_cov",
    "uncov_branches",
];

/// Sort scored rows by severity, apply limit, then group by metric in canonical order.
fn sort_and_group(mut scored_rows: Vec<ScoredRow>, k: i32) -> Vec<MetricRow> {
    scored_rows.sort_by(|a, b| b.severity().cmp(&a.severity()));

    let selected: Vec<ScoredRow> = if k < 0 {
        scored_rows
    } else {
        scored_rows.into_iter().take(k as usize).collect()
    };

    let mut grouped: HashMap<String, Vec<ScoredRow>> = HashMap::new();
    for row in selected {
        grouped.entry(row.metric_name.clone()).or_default().push(row);
    }

    let mut rows = Vec::new();
    for metric_name in METRIC_ORDER {
        if let Some(metric_rows) = grouped.remove(*metric_name) {
            for row in metric_rows {
                rows.push(row.into_row());
            }
        }
    }
    rows
}

/// Collect metric rows for text output.
///
/// The `k` parameter limits how many rows are collected globally:
/// - If `k < 0`, all rows are collected
/// - If `k >= 0`, at most `k` rows are collected total, prioritizing by severity
///
/// Rows are selected by severity (critical first, then poor, fair, good, excellent)
/// across all metric types, then grouped by metric type for display.
pub fn collect_metric_rows(
    metrics: &MetricsBundle,
    config: &Config,
    k: i32,
    symbol_filter: Option<&str>,
    ignores: &Ignores,
) -> Vec<MetricRow> {
    let specs = MetricSpecs::new(metrics, config);

    if let Some(filter) = symbol_filter {
        let mut rows = specs.collect_filtered(filter);
        rows.retain(|(metric, symbol, _, _)| {
            !ignores.is_ignored(symbol, metric)
        });
        return rows;
    }

    let mut scored_rows = specs.collect_all_scored();
    scored_rows
        .retain(|row| !ignores.is_ignored(&row.symbol, &row.metric_name));

    sort_and_group(scored_rows, k)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec_with<'a>(
        distribution: &'a [(String, usize)],
        thresholds: &'a TwoSidedThresholds,
        fan_in: &[(&'a str, usize)],
    ) -> TwoSidedSizeSpec<'a> {
        TwoSidedSizeSpec {
            distribution,
            thresholds,
            fan_in: fan_in.iter().copied().collect(),
        }
    }

    #[test]
    fn low_side_flags_only_single_caller_units() {
        let thresholds = Config::default().thresholds.function_size;
        let distribution = vec![
            ("pass_through".to_string(), 1),
            ("trait_impl".to_string(), 1),
            ("shared_getter".to_string(), 1),
            ("small_single_caller".to_string(), 3),
            ("big".to_string(), 250),
        ];
        let spec = spec_with(
            &distribution,
            &thresholds,
            &[
                ("pass_through", 1),
                // trait_impl absent from fan_in map -> fan_in 0, exempt
                ("shared_getter", 30),
                ("small_single_caller", 1),
                ("big", 1),
            ],
        );

        let mut rows = Vec::new();
        spec.collect_all(&mut rows);
        let by_symbol: HashMap<&str, Bucket> =
            rows.iter().map(|r| (r.symbol.as_str(), r.bucket)).collect();

        // fan_in == 1: low side applies.
        assert_eq!(by_symbol["pass_through"], Bucket::Poor);
        assert_eq!(by_symbol["small_single_caller"], Bucket::Fair);
        // Exempt 1-liners are boring and produce no row at all.
        assert!(!by_symbol.contains_key("trait_impl"));
        assert!(!by_symbol.contains_key("shared_getter"));
        // The high side is unaffected by the gate.
        assert_eq!(by_symbol["big"], Bucket::Critical);
    }
}
