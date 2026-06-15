//! JSON output formatting for the CLI.

use mdlr_metrics::{
    BucketedFanMetrics, BucketedValue, ComplexityMetrics, CoverageMetrics,
    FileLocMetrics, MainSequenceMetrics, StructMetrics,
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
        "cognitive": {
            "max": complexity.cognitive.max,
            "mean": complexity.cognitive.mean,
            "p90": complexity.cognitive.p90,
            "distribution": distribution_json(&complexity.cognitive.distribution, "complexity"),
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

/// Build JSON for C# main sequence distance by directory module.
pub fn build_main_sequence_json(
    main_sequence: &MainSequenceMetrics,
    include_refactor_pressure: bool,
    include_refactor_target: bool,
    include_refactor_priority: bool,
) -> serde_json::Value {
    let distance_distribution: Vec<_> = main_sequence
        .distance
        .distribution
        .iter()
        .map(|(module, value)| {
            serde_json::json!({
                "module": module,
                "main_sequence_distance": value,
            })
        })
        .collect();

    let refactor_pressure_distribution: Vec<_> = main_sequence
        .refactor_pressure
        .distribution
        .iter()
        .map(|(module, value)| {
            serde_json::json!({
                "module": module,
                "main_sequence_refactor_pressure": value,
            })
        })
        .collect();

    let refactor_target_distribution: Vec<_> = main_sequence
        .target_score
        .distribution
        .iter()
        .map(|(module, value)| {
            serde_json::json!({
                "module": module,
                "refactor_target_score": value,
            })
        })
        .collect();
    let refactor_priority_distribution: Vec<_> = main_sequence
        .priority_score
        .distribution
        .iter()
        .map(|(module, value)| {
            serde_json::json!({
                "module": module,
                "refactor_priority_score": value,
            })
        })
        .collect();

    let modules: Vec<_> = main_sequence
        .modules
        .iter()
        .map(|module| {
            let mut detail = serde_json::json!({
                "module": module.id,
                "abstractness": module.abstractness,
                "instability": module.instability,
                "distance": module.distance,
                "ca": module.ca,
                "ce": module.ce,
                "type_count": module.type_count,
                "abstract_type_count": module.abstract_type_count,
                "zone": module.zone,
                "project_paths": module.project_paths,
                "explicit_test_project": module.explicit_test_project,
                "reachable_from_executable": module.reachable_from_executable,
            });
            if include_refactor_pressure {
                detail["architecture_priority"] =
                    serde_json::json!(module.architecture_priority);
                detail["implementation_complexity"] =
                    serde_json::json!(module.implementation_complexity);
                detail["refactor_pressure"] =
                    serde_json::json!(module.refactor_pressure);
            }
            if include_refactor_target {
                detail["refactor_payoff"] =
                    serde_json::json!(module.refactor_payoff);
                detail["refactor_effort"] =
                    serde_json::json!(module.refactor_effort);
                detail["refactor_target_score"] =
                    serde_json::json!(module.refactor_target_score);
            }
            if include_refactor_priority {
                detail["project_context_weight"] =
                    serde_json::json!(module.project_context_weight);
                detail["refactor_priority_score"] =
                    serde_json::json!(module.refactor_priority_score);
            }
            detail
        })
        .collect();

    let mut output = serde_json::json!({
        "distance": {
            "max": main_sequence.distance.max,
            "mean": main_sequence.distance.mean,
            "p90": main_sequence.distance.p90,
            "distribution": distance_distribution,
        },
        "modules": modules,
    });
    if include_refactor_pressure {
        output["refactor_pressure"] = serde_json::json!({
            "max": main_sequence.refactor_pressure.max,
            "mean": main_sequence.refactor_pressure.mean,
            "p90": main_sequence.refactor_pressure.p90,
            "distribution": refactor_pressure_distribution,
        });
    }
    if include_refactor_target {
        output["refactor_target_score"] = serde_json::json!({
            "max": main_sequence.target_score.max,
            "mean": main_sequence.target_score.mean,
            "p90": main_sequence.target_score.p90,
            "distribution": refactor_target_distribution,
        });
    }
    if include_refactor_priority {
        output["refactor_priority_score"] = serde_json::json!({
            "max": main_sequence.priority_score.max,
            "mean": main_sequence.priority_score.mean,
            "p90": main_sequence.priority_score.p90,
            "distribution": refactor_priority_distribution,
        });
    }
    output
}
