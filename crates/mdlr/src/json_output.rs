//! JSON output formatting for the CLI.

use mdlr_metrics::{
    BucketedFanMetrics, BucketedValue, ComplexityMetrics, CoverageMetrics,
    FileLocMetrics, StructMetrics,
};

/// Build JSON for a bucketed metric value
pub fn build_bucketed_json(metric: &BucketedValue) -> serde_json::Value {
    serde_json::json!({
        "value": metric.value,
        "bucket": metric.bucket,
    })
}

/// Build JSON for fan metrics (fan_in/fan_out with max/mean and distribution)
pub fn build_fan_metrics_json(
    metrics: &BucketedFanMetrics,
    distribution: &[(String, usize)],
) -> serde_json::Value {
    serde_json::json!({
        "max": {
            "value": metrics.max.value as usize,
            "bucket": metrics.max.bucket,
        },
        "mean": {
            "value": metrics.mean.value,
            "bucket": metrics.mean.bucket,
        },
        "distribution": distribution_json(distribution, "count"),
    })
}

fn distribution_json(
    distribution: &[(String, usize)],
    value_key: &str,
) -> Vec<serde_json::Value> {
    distribution
        .iter()
        .map(|(id, val)| serde_json::json!({"id": id, value_key: val}))
        .collect()
}

/// Build JSON for complexity metrics with distributions
pub fn build_complexity_json(
    complexity: &ComplexityMetrics,
) -> serde_json::Value {
    serde_json::json!({
        "size": {
            "max": complexity.size.max,
            "mean": complexity.size.mean,
            "p90": complexity.size.p90,
            "distribution": distribution_json(&complexity.size.distribution, "lines"),
        },
        "params": {
            "max": complexity.params.max,
            "mean": complexity.params.mean,
            "distribution": distribution_json(&complexity.params.distribution, "count"),
        },
        "cyclomatic": {
            "max": complexity.cyclomatic.max,
            "mean": complexity.cyclomatic.mean,
            "p90": complexity.cyclomatic.p90,
            "distribution": distribution_json(&complexity.cyclomatic.distribution, "complexity"),
        },
        "max_scope": {
            "max": complexity.max_scope.max,
            "mean": complexity.max_scope.mean,
            "p90": complexity.max_scope.p90,
            "distribution": distribution_json(&complexity.max_scope.distribution, "lines"),
        },
    })
}

/// Build JSON for struct metrics with distributions
pub fn build_struct_json(struct_metrics: &StructMetrics) -> serde_json::Value {
    serde_json::json!({
        "methods_per_struct": {
            "max": struct_metrics.methods_per_struct.max,
            "mean": struct_metrics.methods_per_struct.mean,
            "p90": struct_metrics.methods_per_struct.p90,
            "distribution": distribution_json(&struct_metrics.methods_per_struct.distribution, "count"),
        },
        "lcom": {
            "max": struct_metrics.lcom.max,
            "mean": struct_metrics.lcom.mean,
            "distribution": distribution_json(&struct_metrics.lcom.distribution, "lcom4"),
        },
    })
}

/// Build JSON for coverage metrics. `uncov_branches` is omitted when the
/// input lcov had no BRDA records.
pub fn build_coverage_json(cov: &CoverageMetrics) -> serde_json::Value {
    let line_dist: Vec<_> = cov
        .line_cov
        .distribution
        .iter()
        .map(|(id, pct)| serde_json::json!({"id": id, "line_cov_pct": pct}))
        .collect();
    let mut out = serde_json::json!({
        "line_cov": {
            "max": cov.line_cov.max,
            "mean": cov.line_cov.mean,
            "p90": cov.line_cov.p90,
            "distribution": line_dist,
        },
        "has_branches": cov.has_branches,
        "units_analyzed": cov.units_analyzed,
        "units_without_data": cov.units_without_data,
    });
    if cov.has_branches {
        let br_dist: Vec<_> = cov
            .uncov_branches
            .distribution
            .iter()
            .map(|(id, n)| serde_json::json!({"id": id, "uncov_branches": n}))
            .collect();
        out["uncov_branches"] = serde_json::json!({
            "max": cov.uncov_branches.max,
            "mean": cov.uncov_branches.mean,
            "p90": cov.uncov_branches.p90,
            "distribution": br_dist,
        });
    }
    out
}

/// Build JSON for file_loc metrics with distribution
pub fn build_file_loc_json(file_loc: &FileLocMetrics) -> serde_json::Value {
    let distribution: Vec<_> = file_loc
        .distribution
        .iter()
        .map(|(file, lines)| serde_json::json!({"file": file, "lines": lines}))
        .collect();

    serde_json::json!({
        "max": file_loc.max,
        "mean": file_loc.mean,
        "p90": file_loc.p90,
        "total": file_loc.total,
        "distribution": distribution,
    })
}
