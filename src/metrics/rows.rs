use crate::config::Bucket;
use crate::config::Config;
use crate::config::MetricThresholds;
use crate::metrics::{
    ComplexityMetrics, FileLocMetrics, ImplMetrics, StructuralMetrics,
    TagMetrics,
};

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
    pub impl_metrics: &'a ImplMetrics,
    pub file_loc: &'a FileLocMetrics,
    pub tag_metrics: &'a TagMetrics,
}

/// Specification for collecting an integer metric
struct IntMetricSpec<'a> {
    name: &'static str,
    distribution: &'a [(String, usize)],
    thresholds: &'a MetricThresholds,
    min_value: usize,
}

impl IntMetricSpec<'_> {
    /// Collect all entries (for global sorting mode)
    fn collect_all(&self, rows: &mut Vec<ScoredRow>) {
        for (name, value) in self.distribution {
            if *value > self.min_value {
                let bucket = self.thresholds.evaluate(*value as f64);
                rows.push(ScoredRow {
                    metric_name: self.name.to_string(),
                    symbol: name.clone(),
                    value: value.to_string(),
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
                let bucket = self.thresholds.evaluate(*value as f64);
                rows.push((
                    self.name.to_string(),
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

/// Collect metric rows for text output.
///
/// The `k` parameter limits how many rows are collected globally:
/// - If `k < 0`, all rows are collected
/// - If `k >= 0`, at most `k` rows are collected total, prioritizing by severity
///
/// Rows are selected by severity (critical first, then poor, fair, good, excellent)
/// across all metric types, then grouped by metric type for display.
///
/// The `symbol_filter` parameter, when `Some`, limits output to only the
/// matching symbol. This is used when filtering by a specific symbol ID.
pub fn collect(
    metrics: &MetricsBundle,
    config: &Config,
    k: i32,
    symbol_filter: Option<&str>,
) -> Vec<MetricRow> {
    let t = &config.thresholds;
    let m = metrics;

    // Integer metrics
    let int_specs = [
        IntMetricSpec {
            name: "fan_out",
            distribution: &m.structural.fan_out.distribution,
            thresholds: &t.fan_out_max,
            min_value: 0,
        },
        IntMetricSpec {
            name: "fan_in",
            distribution: &m.structural.fan_in.distribution,
            thresholds: &t.fan_in_max,
            min_value: 0,
        },
        IntMetricSpec {
            name: "function_size",
            distribution: &m.complexity.size.distribution,
            thresholds: &t.function_size,
            min_value: 1,
        },
        IntMetricSpec {
            name: "params",
            distribution: &m.complexity.params.distribution,
            thresholds: &t.params,
            min_value: 0,
        },
        IntMetricSpec {
            name: "cyclomatic",
            distribution: &m.complexity.cyclomatic.distribution,
            thresholds: &t.cyclomatic,
            min_value: 1,
        },
        IntMetricSpec {
            name: "methods_per_impl",
            distribution: &m.impl_metrics.methods_per_impl.distribution,
            thresholds: &t.methods_per_impl,
            min_value: 0,
        },
        IntMetricSpec {
            name: "traits_per_type",
            distribution: &m.impl_metrics.traits_per_type.distribution,
            thresholds: &t.traits_per_type,
            min_value: 0,
        },
        IntMetricSpec {
            name: "file_loc",
            distribution: &m.file_loc.distribution,
            thresholds: &t.file_loc,
            min_value: 0,
        },
    ];

    // Float metrics
    let float_specs = [FloatMetricSpec {
        name: "lcom",
        distribution: &m.impl_metrics.lcom.distribution,
        thresholds: &t.lcom,
        min_value: 0.0,
    }];

    // Handle symbol filter mode separately (no global sorting)
    if let Some(filter) = symbol_filter {
        let mut rows: Vec<MetricRow> = Vec::new();
        for spec in &int_specs {
            spec.collect_filtered(&mut rows, filter);
        }
        for spec in &float_specs {
            spec.collect_filtered(&mut rows, filter);
        }
        return rows;
    }

    // Collect all rows with severity scores
    let mut scored_rows: Vec<ScoredRow> = Vec::new();
    for spec in &int_specs {
        spec.collect_all(&mut scored_rows);
    }
    for spec in &float_specs {
        spec.collect_all(&mut scored_rows);
    }

    // Sort by severity descending (worst first)
    scored_rows.sort_by(|a, b| b.severity().cmp(&a.severity()));

    // Apply global limit
    let selected: Vec<ScoredRow> = if k < 0 {
        scored_rows
    } else {
        scored_rows.into_iter().take(k as usize).collect()
    };

    // Group by metric type to maintain display grouping
    let mut grouped: std::collections::HashMap<String, Vec<ScoredRow>> =
        std::collections::HashMap::new();
    for row in selected {
        grouped.entry(row.metric_name.clone()).or_default().push(row);
    }

    // Define metric order for consistent output
    let metric_order = [
        "fan_out",
        "fan_in",
        "function_size",
        "params",
        "cyclomatic",
        "methods_per_impl",
        "traits_per_type",
        "file_loc",
        "lcom",
    ];

    // Convert to MetricRows in metric order
    let mut rows: Vec<MetricRow> = Vec::new();
    for metric_name in &metric_order {
        if let Some(metric_rows) = grouped.remove(*metric_name) {
            for row in metric_rows {
                rows.push(row.into_row());
            }
        }
    }

    // Conceptual metrics (if tags exist)
    collect_conceptual_metrics(&mut rows, m, k);

    rows
}

/// Collect conceptual metrics from tag metrics.
fn collect_conceptual_metrics(
    rows: &mut Vec<MetricRow>,
    m: &MetricsBundle,
    k: i32,
) {
    let Some(ref conceptual) = m.tag_metrics.conceptual else {
        return;
    };

    let limit_fan_out = if k < 0 {
        conceptual.conceptual_fan_out.top.len()
    } else {
        k as usize
    };
    for (name, count) in
        conceptual.conceptual_fan_out.top.iter().take(limit_fan_out)
    {
        if *count > 1 {
            rows.push((
                "conceptual_fan_out".to_string(),
                name.clone(),
                count.to_string(),
                "-".to_string(),
            ));
        }
    }

    let limit_scatter =
        if k < 0 { conceptual.concept_scattering.len() } else { k as usize };
    for scatter in conceptual.concept_scattering.iter().take(limit_scatter) {
        if scatter.file_count > 1 {
            rows.push((
                "concept_scattering".to_string(),
                scatter.tag.clone(),
                format!("{:.2}", scatter.scatter_ratio),
                "-".to_string(),
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::Graph;
    use crate::metrics::{TagMetrics, compute};

    #[test]
    fn test_collect_empty_metrics() {
        let graph = Graph::new();
        let structural = compute(&graph);
        let complexity = ComplexityMetrics::compute(&graph);
        let impl_metrics = ImplMetrics::compute(&graph);
        let file_loc = FileLocMetrics::compute(&graph);
        let tag_metrics = TagMetrics::compute(&graph, &Default::default());
        let config = Config::default();

        let bundle = MetricsBundle {
            structural: &structural,
            complexity: &complexity,
            impl_metrics: &impl_metrics,
            file_loc: &file_loc,
            tag_metrics: &tag_metrics,
        };
        let rows = collect(&bundle, &config, -1, None);

        assert!(rows.is_empty());
    }

    #[test]
    fn test_collect_respects_global_k_limit() {
        let graph = Graph::new();
        let mut structural = compute(&graph);
        // Add fan_out entries with different severities
        // Thresholds: excellent < 3, good < 5, fair < 8, poor < 12
        structural.fan_out.distribution = vec![
            ("a".to_string(), 15), // critical
            ("b".to_string(), 10), // poor
            ("c".to_string(), 6),  // fair
        ];

        let complexity = ComplexityMetrics::compute(&graph);
        let impl_metrics = ImplMetrics::compute(&graph);
        let file_loc = FileLocMetrics::compute(&graph);
        let tag_metrics = TagMetrics::compute(&graph, &Default::default());
        let config = Config::default();

        let bundle = MetricsBundle {
            structural: &structural,
            complexity: &complexity,
            impl_metrics: &impl_metrics,
            file_loc: &file_loc,
            tag_metrics: &tag_metrics,
        };
        // With k=2, should only get 2 total rows (the worst ones)
        let rows = collect(&bundle, &config, 2, None);

        assert_eq!(rows.len(), 2);
        // Should have the critical and poor ones
        assert!(rows.iter().any(|r| r.1 == "a" && r.3 == "critical"));
        assert!(rows.iter().any(|r| r.1 == "b" && r.3 == "poor"));
    }

    #[test]
    fn test_collect_prioritizes_severity_across_metrics() {
        let graph = Graph::new();
        let mut structural = compute(&graph);
        // fan_out: fair severity (value 6, threshold < 8 for fair)
        structural.fan_out.distribution = vec![("fan_out_sym".to_string(), 6)];
        // fan_in: critical severity (value 20, threshold < 15 for poor)
        structural.fan_in.distribution = vec![("fan_in_sym".to_string(), 20)];

        let mut complexity = ComplexityMetrics::compute(&graph);
        // function_size: poor severity (value 150, threshold < 200 for poor)
        complexity.size.distribution = vec![("func_sym".to_string(), 150)];

        let impl_metrics = ImplMetrics::compute(&graph);
        let file_loc = FileLocMetrics::compute(&graph);
        let tag_metrics = TagMetrics::compute(&graph, &Default::default());
        let config = Config::default();

        let bundle = MetricsBundle {
            structural: &structural,
            complexity: &complexity,
            impl_metrics: &impl_metrics,
            file_loc: &file_loc,
            tag_metrics: &tag_metrics,
        };

        // With k=2, should get the critical fan_in and poor function_size
        // (not the fair fan_out)
        let rows = collect(&bundle, &config, 2, None);

        assert_eq!(rows.len(), 2);
        // Should have fan_in (critical) and function_size (poor)
        assert!(rows.iter().any(|r| r.0 == "fan_in" && r.3 == "critical"));
        assert!(rows.iter().any(|r| r.0 == "function_size" && r.3 == "poor"));
        // Should NOT have fan_out (fair)
        assert!(!rows.iter().any(|r| r.0 == "fan_out"));
    }

    #[test]
    fn test_collect_with_symbol_filter() {
        let graph = Graph::new();
        let mut structural = compute(&graph);
        // Add some fan_out entries
        structural.fan_out.distribution = vec![
            ("src/foo.rs::bar".to_string(), 5),
            ("src/baz.rs::qux".to_string(), 4),
            ("src/foo.rs::other".to_string(), 3),
        ];
        structural.fan_in.distribution = vec![
            ("src/foo.rs::bar".to_string(), 10),
            ("src/baz.rs::qux".to_string(), 2),
        ];

        let complexity = ComplexityMetrics::compute(&graph);
        let impl_metrics = ImplMetrics::compute(&graph);
        let file_loc = FileLocMetrics::compute(&graph);
        let tag_metrics = TagMetrics::compute(&graph, &Default::default());
        let config = Config::default();

        let bundle = MetricsBundle {
            structural: &structural,
            complexity: &complexity,
            impl_metrics: &impl_metrics,
            file_loc: &file_loc,
            tag_metrics: &tag_metrics,
        };

        // With symbol filter, should only get rows for that symbol
        let rows = collect(&bundle, &config, -1, Some("src/foo.rs::bar"));

        // Should have fan_out and fan_in for the filtered symbol
        let fan_out_rows: Vec<_> =
            rows.iter().filter(|r| r.0 == "fan_out").collect();
        let fan_in_rows: Vec<_> =
            rows.iter().filter(|r| r.0 == "fan_in").collect();

        assert_eq!(fan_out_rows.len(), 1);
        assert_eq!(fan_out_rows[0].1, "src/foo.rs::bar");
        assert_eq!(fan_in_rows.len(), 1);
        assert_eq!(fan_in_rows[0].1, "src/foo.rs::bar");
    }
}
