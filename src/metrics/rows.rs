use crate::config::Config;
use crate::config::MetricThresholds;
use crate::metrics::{
    ComplexityMetrics, FileLocMetrics, ImplMetrics, StructuralMetrics,
    TagMetrics,
};

/// A metric row: (metric_name, symbol, value, bucket)
pub type MetricRow = (String, String, String, String);

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
    fn collect(
        &self,
        rows: &mut Vec<MetricRow>,
        limit: usize,
        symbol_filter: Option<&str>,
    ) {
        let iter: Box<dyn Iterator<Item = &(String, usize)>> =
            if let Some(filter) = symbol_filter {
                // When filtering, only include the matching symbol
                Box::new(
                    self.distribution
                        .iter()
                        .filter(move |(name, _)| name == filter),
                )
            } else {
                Box::new(self.distribution.iter().take(limit))
            };

        for (name, value) in iter {
            if *value > self.min_value {
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
    fn collect(
        &self,
        rows: &mut Vec<MetricRow>,
        limit: usize,
        symbol_filter: Option<&str>,
    ) {
        let iter: Box<dyn Iterator<Item = &(String, f64)>> =
            if let Some(filter) = symbol_filter {
                // When filtering, only include the matching symbol
                Box::new(
                    self.distribution
                        .iter()
                        .filter(move |(name, _)| name == filter),
                )
            } else {
                Box::new(self.distribution.iter().take(limit))
            };

        for (name, value) in iter {
            if *value > self.min_value {
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
/// The `k` parameter limits how many rows are collected per metric:
/// - If `k < 0`, all rows are collected
/// - If `k >= 0`, at most `k` rows are collected per metric
///
/// The `symbol_filter` parameter, when `Some`, limits output to only the
/// matching symbol. This is used when filtering by a specific symbol ID.
pub fn collect(
    metrics: &MetricsBundle,
    config: &Config,
    k: i32,
    symbol_filter: Option<&str>,
) -> Vec<MetricRow> {
    let mut rows: Vec<MetricRow> = Vec::new();
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

    for spec in &int_specs {
        let limit = if k < 0 { spec.distribution.len() } else { k as usize };
        spec.collect(&mut rows, limit, symbol_filter);
    }

    // Float metrics
    let float_specs = [FloatMetricSpec {
        name: "lcom",
        distribution: &m.impl_metrics.lcom.distribution,
        thresholds: &t.lcom,
        min_value: 0.0,
    }];

    for spec in &float_specs {
        let limit = if k < 0 { spec.distribution.len() } else { k as usize };
        spec.collect(&mut rows, limit, symbol_filter);
    }

    // Conceptual metrics (if tags exist) - only collect if not filtering by symbol
    if symbol_filter.is_none() {
        collect_conceptual_metrics(&mut rows, m, k);
    }

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
    fn test_collect_respects_k_limit() {
        let graph = Graph::new();
        let mut structural = compute(&graph);
        // Add some fan_out entries
        structural.fan_out.distribution = vec![
            ("a".to_string(), 5),
            ("b".to_string(), 4),
            ("c".to_string(), 3),
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
        // With k=2, should only get 2 fan_out rows
        let rows = collect(&bundle, &config, 2, None);

        let fan_out_rows: Vec<_> =
            rows.iter().filter(|r| r.0 == "fan_out").collect();
        assert_eq!(fan_out_rows.len(), 2);
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
