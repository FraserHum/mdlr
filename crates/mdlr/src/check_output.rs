//! Text and JSON formatting for the `mdlr check` command. Consumes the
//! `ComputedMetrics` produced in [`crate::check`] and renders it, honoring
//! disabled metrics and per-symbol filtering.

use anyhow::Result;
use std::io::Write;

use crate::cache::CacheStore;
use crate::check::{CheckFilter, ComputedMetrics, ScopeInfo};
use crate::check_scope::describe_scope;
use crate::cli::OutputFormat;
use crate::config;
use crate::display_scope::DisplayScope;
use crate::json_output::{
    build_bucketed_json, build_complexity_json, build_fan_metrics_json,
    build_file_loc_json, build_main_sequence_json, build_struct_json,
};
use crate::metrics_rows::{
    MetricSpecs, MetricsBundle, RowSelection, collect_metric_rows,
};
use mdlr_metrics::{BucketedMetrics, Thresholds};

/// Bundle the computed metrics for row collection.
fn metrics_bundle(computed: &ComputedMetrics) -> MetricsBundle<'_> {
    MetricsBundle {
        structural: &computed.structural,
        complexity: &computed.complexity,
        struct_metrics: &computed.struct_metrics,
        main_sequence: &computed.main_sequence,
        file_loc: &computed.file_loc,
        duplication: &computed.duplication,
        coverage: computed.coverage.as_ref(),
    }
}

/// Everything `render` needs beyond the computed metrics and config.
pub(crate) struct RenderArgs<'a> {
    pub format: OutputFormat,
    pub k: i32,
    pub pretty: bool,
    pub entry_count: usize,
    pub filter: &'a CheckFilter,
    pub scope: Option<&'a DisplayScope>,
}

/// Render `check` results in the requested output format.
pub(crate) fn render(
    computed: &ComputedMetrics,
    config: &config::Config,
    args: &RenderArgs,
    store: &CacheStore,
) -> anyhow::Result<()> {
    let scope_info = describe_scope(args.filter, args.scope);
    match args.format {
        OutputFormat::Text => format_text_output(
            computed,
            config,
            &TextOptions {
                k: args.k,
                pretty: args.pretty,
                filter: args.filter,
                scope: &scope_info,
            },
            store,
        ),
        OutputFormat::Json => {
            format_json_output(computed, config, args, &scope_info)
        }
    }
}

/// How rows should be selected for this run's filter.
fn row_selection<'a>(filter: &'a CheckFilter, k: i32) -> RowSelection<'a> {
    match filter {
        CheckFilter::Symbol(s) => RowSelection::Symbol(s.as_str()),
        _ => RowSelection::Top(k),
    }
}

/// Presentation options for `format_text_output`.
struct TextOptions<'a> {
    k: i32,
    pretty: bool,
    filter: &'a CheckFilter,
    scope: &'a ScopeInfo,
}

/// Format and print text output
fn format_text_output(
    computed: &ComputedMetrics,
    config: &config::Config,
    opts: &TextOptions,
    store: &CacheStore,
) -> Result<()> {
    // Diff mode switches scope silently on git state; always say which scope
    // this run reported on.
    println!("scope: {}", opts.scope.description);

    let bundle = metrics_bundle(computed);
    let ignores = store.ignores().load_ignores().unwrap_or_default();
    let rows = collect_metric_rows(
        &bundle,
        config,
        row_selection(opts.filter, opts.k),
        &ignores,
    );

    if opts.pretty {
        let mut tw = tabwriter::TabWriter::new(vec![]);
        writeln!(tw, "metric\tsymbol\tvalue\tbucket")?;
        for (metric, symbol, value, bucket) in &rows {
            writeln!(tw, "{}\t{}\t{}\t{}", metric, symbol, value, bucket)?;
        }
        tw.flush()?;
        print!("{}", String::from_utf8_lossy(&tw.into_inner()?));
    } else {
        println!("metric\tsymbol\tvalue\tbucket");
        for (metric, symbol, value, bucket) in &rows {
            println!("{}\t{}\t{}\t{}", metric, symbol, value, bucket);
        }
    }

    let partial_count =
        computed.graph.units.iter().filter(|u| u.partial).count();
    if partial_count > 0 {
        eprintln!(
            "warning: {} unit(s) have partial extraction (compilation errors prevented full analysis)",
            partial_count
        );
    }

    Ok(())
}

/// Format and print JSON output
fn format_json_output(
    computed: &ComputedMetrics,
    config: &config::Config,
    args: &RenderArgs,
    scope: &ScopeInfo,
) -> Result<()> {
    // When filtering by symbol, output specific metrics for that symbol
    if let CheckFilter::Symbol(symbol_id) = args.filter {
        let output = build_symbol_json(computed, config, symbol_id);
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    let partial_count =
        computed.graph.units.iter().filter(|u| u.partial).count();

    let output = serde_json::json!({
        "scope": {
            "mode": scope.mode,
            "description": scope.description,
        },
        "files": {
            "extracted": args.entry_count,
        },
        "units": computed.graph.units.len(),
        "partial_units": partial_count,
        "edges": computed.graph.edges.len(),
        "metrics": build_metrics_json(computed, config),
    });
    println!("{}", serde_json::to_string_pretty(&output)?);

    Ok(())
}

/// Assemble the full `metrics` JSON object, then drop any disabled metrics.
fn build_metrics_json(
    computed: &ComputedMetrics,
    config: &config::Config,
) -> serde_json::Value {
    // Bucket the structural summary with the user's configured thresholds.
    let t = &config.thresholds;
    let thresholds = Thresholds {
        dag_density: t.dag_density.clone(),
        fan_in_max: t.fan_in_max.clone(),
        fan_in_mean: t.fan_in_mean.clone(),
        fan_out_max: t.fan_out_max.clone(),
        fan_out_mean: t.fan_out_mean.clone(),
    };
    let bucketed =
        BucketedMetrics::from_metrics(&computed.structural, &thresholds);

    let duplication_json = serde_json::json!({
        "max": computed.duplication.max,
        "mean": computed.duplication.mean,
        "p90": computed.duplication.p90,
        "clone_count": computed.duplication.clone_count,
        "distribution": computed.duplication.distribution.iter()
            .map(|(unit, pct)| serde_json::json!({"unit": unit, "duplication_pct": pct}))
            .collect::<Vec<_>>(),
    });

    let mut metrics = serde_json::json!({
        "dag_density": build_bucketed_json(&bucketed.dag_density),
        "fan_in": build_fan_metrics_json(&bucketed.fan_in, &computed.structural.fan_in.distribution),
        "fan_out": build_fan_metrics_json(&bucketed.fan_out, &computed.structural.fan_out.distribution),
        "complexity": build_complexity_json(&computed.complexity),
        "struct": build_struct_json(&computed.struct_metrics),
        "main_sequence": build_main_sequence_json(
            &computed.main_sequence,
            !config.is_disabled("main_sequence_refactor_pressure"),
            !config.is_disabled("refactor_target_score"),
            !config.is_disabled("refactor_priority_score"),
        ),
        "file_loc": build_file_loc_json(&computed.file_loc),
        "duplication": duplication_json,
    });
    if let Some(cov) = computed.coverage.as_ref() {
        metrics["coverage"] = crate::json_output::build_coverage_json(cov);
    }

    prune_disabled_metrics(metrics.as_object_mut().expect("object"), config);
    metrics
}

/// Remove disabled metrics from the assembled `metrics` object: top-level keys
/// outright, and individual fields inside the composite `complexity`/`struct`/
/// `coverage` objects (dropping a composite entirely once it is emptied).
fn prune_disabled_metrics(
    metrics: &mut serde_json::Map<String, serde_json::Value>,
    config: &config::Config,
) {
    // (metric name, top-level JSON key)
    const TOP_LEVEL: &[(&str, &str)] = &[
        ("dag_density", "dag_density"),
        ("fan_in", "fan_in"),
        ("fan_out", "fan_out"),
        ("main_sequence_distance", "main_sequence"),
        ("file_loc", "file_loc"),
        ("duplication_pct", "duplication"),
    ];
    // (metric name, composite parent key, sub-field key)
    const NESTED: &[(&str, &str, &str)] = &[
        ("function_size", "complexity", "size"),
        ("params", "complexity", "params"),
        ("cyclomatic", "complexity", "cyclomatic"),
        ("cognitive", "complexity", "cognitive"),
        ("max_scope", "complexity", "max_scope"),
        ("methods_per_struct", "struct", "methods_per_struct"),
        ("lcom", "struct", "lcom"),
        ("line_cov", "coverage", "line_cov"),
        ("uncov_branches", "coverage", "uncov_branches"),
    ];

    for (metric, key) in TOP_LEVEL {
        if config.is_disabled(metric) {
            metrics.remove(*key);
        }
    }
    for (metric, parent, sub) in NESTED {
        if config.is_disabled(metric) {
            if let Some(obj) =
                metrics.get_mut(*parent).and_then(|v| v.as_object_mut())
            {
                obj.remove(*sub);
            }
        }
    }

    if config.is_disabled("refactor_target_score")
        && let Some(main_sequence) =
            metrics.get_mut("main_sequence").and_then(|v| v.as_object_mut())
    {
        main_sequence.remove("refactor_target_score");
        if let Some(modules) =
            main_sequence.get_mut("modules").and_then(|v| v.as_array_mut())
        {
            for module in modules {
                if let Some(module) = module.as_object_mut() {
                    module.remove("refactor_payoff");
                    module.remove("refactor_effort");
                    module.remove("refactor_target_score");
                }
            }
        }
    }
    if config.is_disabled("refactor_priority_score")
        && let Some(main_sequence) =
            metrics.get_mut("main_sequence").and_then(|v| v.as_object_mut())
    {
        main_sequence.remove("refactor_priority_score");
        if let Some(modules) =
            main_sequence.get_mut("modules").and_then(|v| v.as_array_mut())
        {
            for module in modules {
                if let Some(module) = module.as_object_mut() {
                    module.remove("project_context_weight");
                    module.remove("refactor_priority_score");
                }
            }
        }
    }
    for parent in ["complexity", "struct", "coverage"] {
        let empty = metrics
            .get(parent)
            .and_then(|v| v.as_object())
            .is_some_and(|o| o.is_empty());
        if empty {
            metrics.remove(parent);
        }
    }
}

/// Look up a symbol's value in a distribution.
fn find_value(dist: &[(String, usize)], symbol_id: &str) -> Option<usize> {
    dist.iter().find(|(n, _)| n == symbol_id).map(|(_, v)| *v)
}

/// Build JSON output for a specific symbol. Reuses the [`MetricSpecs`]
/// registry so the symbol view and the text rows read the same
/// distributions and thresholds. Unlike text rows, every metric with a
/// value for the symbol is shown (no boring/hub filtering).
fn build_symbol_json(
    computed: &ComputedMetrics,
    config: &config::Config,
    symbol_id: &str,
) -> serde_json::Value {
    let bundle = metrics_bundle(computed);
    let specs = MetricSpecs::new(&bundle, config);
    let mut metrics = serde_json::Map::new();
    let mut insert = |name: &str, value: usize, bucket: config::Bucket| {
        metrics.insert(
            name.to_string(),
            serde_json::json!({ "value": value, "bucket": bucket.to_string() }),
        );
    };

    for spec in &specs.int_specs {
        if let Some(value) = find_value(spec.distribution, symbol_id) {
            insert(spec.name, value, spec.bucket_for(value));
        }
    }
    if let Some(spec) = &specs.fan_in_spec
        && let Some(value) = find_value(spec.distribution, symbol_id)
    {
        insert("fan_in", value, spec.thresholds.evaluate(value as f64));
    }
    if let Some(spec) = &specs.function_size_spec
        && let Some(value) = find_value(spec.distribution, symbol_id)
    {
        insert("function_size", value, spec.bucket_for(symbol_id, value));
    }

    let is_partial =
        computed.graph.units.iter().any(|u| u.id == symbol_id && u.partial);

    let mut output = serde_json::json!({
        "symbol": symbol_id,
        "metrics": metrics
    });
    if is_partial {
        output["partial"] = serde_json::json!(true);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::Ignores;
    use crate::metrics_rows::{RowSelection, collect_metric_rows};
    use mdlr_core::{Edge, EdgeKind, Graph, Span, Unit, UnitKind};
    use mdlr_metrics::{
        ComplexityMetrics, FileLocMetrics, MainSequenceMetrics, StructMetrics,
        compute as compute_structural,
    };
    use std::path::PathBuf;

    fn unit(id: &str, kind: UnitKind, file: &str, tags: &[&str]) -> Unit {
        Unit {
            id: id.to_string(),
            kind,
            file: PathBuf::from(file),
            span: Span {
                start_line: 1,
                start_col: 0,
                end_line: 3,
                end_col: 0,
            },
            reads: vec![],
            writes: vec![],
            calls: vec![],
            tags: tags.iter().map(|s| s.to_string()).collect(),
            params: 0,
            branches: 0,
            max_scope_lines: 0,
            parent: None,
            cognitive_complexity: 0,
            partial: false,
        }
    }

    fn type_unit(id: &str, file: &str, tags: &[&str]) -> Unit {
        unit(id, UnitKind::Struct, file, tags)
    }

    fn method_unit(id: &str, file: &str) -> Unit {
        unit(id, UnitKind::Method, file, &[])
    }

    fn method_unit_with_metrics(
        id: &str,
        file: &str,
        start_line: usize,
        end_line: usize,
        cognitive_complexity: usize,
    ) -> Unit {
        let mut unit = method_unit(id, file);
        unit.span.start_line = start_line;
        unit.span.end_line = end_line;
        unit.cognitive_complexity = cognitive_complexity;
        unit
    }

    fn computed_from_graph(graph: Graph) -> ComputedMetrics {
        ComputedMetrics {
            structural: compute_structural(&graph),
            complexity: ComplexityMetrics::compute(&graph),
            struct_metrics: StructMetrics::compute(&graph),
            main_sequence: MainSequenceMetrics::compute(&graph),
            file_loc: FileLocMetrics::compute(&graph),
            duplication: mdlr_cpd::DuplicationMetrics::default(),
            coverage: None,
            graph,
        }
    }

    fn computed_with_main_sequence_edge() -> ComputedMetrics {
        let mut graph = Graph::new();
        graph.add_unit(type_unit(
            "src/api/A.cs::Api.IFoo",
            "src/api/A.cs",
            &["interface"],
        ));
        graph.add_unit(type_unit(
            "src/impl/B.cs::Impl.Foo",
            "src/impl/B.cs",
            &["class"],
        ));
        graph.add_unit(method_unit(
            "src/api/A.cs::Api.IFoo::Run()",
            "src/api/A.cs",
        ));
        graph.add_unit(method_unit(
            "src/impl/B.cs::Impl.Foo::Run()",
            "src/impl/B.cs",
        ));
        graph.add_edge(Edge {
            from: "src/impl/B.cs::Impl.Foo::Run()".to_string(),
            to: "src/api/A.cs::Api.IFoo::Run()".to_string(),
            kind: EdgeKind::Calls,
        });

        computed_from_graph(graph)
    }

    fn computed_with_pressure_ranking() -> ComputedMetrics {
        let mut graph = Graph::new();
        graph.add_unit(type_unit(
            "core/t0.cs::Core.T0",
            "core/t0.cs",
            &["class", "abstract"],
        ));
        for i in 1..5 {
            graph.add_unit(type_unit(
                &format!("core/t{i}.cs::Core.T{i}"),
                &format!("core/t{i}.cs"),
                &["class"],
            ));
        }
        graph.add_unit(method_unit_with_metrics(
            "core/t0.cs::Core.T0::Run()",
            "core/t0.cs",
            1,
            200,
            30,
        ));

        graph.add_unit(type_unit(
            "tiny/t.cs::Tiny.T",
            "tiny/t.cs",
            &["class"],
        ));
        graph.add_unit(method_unit_with_metrics(
            "tiny/t.cs::Tiny.T::Run()",
            "tiny/t.cs",
            1,
            5,
            1,
        ));

        for i in 0..5 {
            graph.add_unit(type_unit(
                &format!("caller{i}/c.cs::Caller{i}.C"),
                &format!("caller{i}/c.cs"),
                &["class"],
            ));
            graph.add_unit(method_unit(
                &format!("caller{i}/c.cs::Caller{i}.C::Run()"),
                &format!("caller{i}/c.cs"),
            ));
            graph.add_edge(Edge {
                from: format!("caller{i}/c.cs::Caller{i}.C::Run()"),
                to: "core/t0.cs::Core.T0::Run()".to_string(),
                kind: EdgeKind::Calls,
            });
        }

        graph.add_unit(type_unit(
            "tinycaller/c.cs::TinyCaller.C",
            "tinycaller/c.cs",
            &["class"],
        ));
        graph.add_unit(method_unit(
            "tinycaller/c.cs::TinyCaller.C::Run()",
            "tinycaller/c.cs",
        ));
        graph.add_edge(Edge {
            from: "tinycaller/c.cs::TinyCaller.C::Run()".to_string(),
            to: "tiny/t.cs::Tiny.T::Run()".to_string(),
            kind: EdgeKind::Calls,
        });

        computed_from_graph(graph)
    }

    #[test]
    fn metrics_json_includes_main_sequence_details() {
        let computed = computed_with_main_sequence_edge();
        let json = build_metrics_json(&computed, &config::Config::default());

        assert!(json["complexity"].get("cognitive").is_some());

        let main_sequence = &json["main_sequence"];
        let modules = main_sequence["modules"].as_array().unwrap();
        let api = modules
            .iter()
            .find(|module| module["module"] == "src/api")
            .unwrap();
        assert_eq!(api["abstract_type_count"], serde_json::json!(1));
        assert_eq!(api["zone"], serde_json::json!("balanced"));
        assert_eq!(api["ca"], serde_json::json!(1));
        assert_eq!(api["ce"], serde_json::json!(0));
        assert_eq!(api["distance"], serde_json::json!(0));
        assert!(api.get("architecture_priority").is_some());
        assert!(api.get("implementation_complexity").is_some());
        assert!(api.get("refactor_pressure").is_some());
        assert!(api.get("refactor_payoff").is_some());
        assert!(api.get("refactor_effort").is_some());
        assert!(api.get("refactor_target_score").is_some());
        assert_eq!(api["project_paths"], serde_json::json!([]));
        assert_eq!(api["project_context_weight"], serde_json::json!(1.0));
        assert!(api.get("refactor_priority_score").is_some());
        assert!(main_sequence.get("refactor_pressure").is_some());
        assert!(main_sequence.get("refactor_target_score").is_some());
        assert!(main_sequence.get("refactor_priority_score").is_some());
    }

    #[test]
    fn disabled_main_sequence_distance_removes_json_object() {
        let computed = computed_with_main_sequence_edge();
        let config = config::Config {
            disabled_metrics: vec!["main_sequence_distance".to_string()],
            ..Default::default()
        };
        let json = build_metrics_json(&computed, &config);

        assert!(json.get("main_sequence").is_none());
    }

    #[test]
    fn disabled_main_sequence_refactor_pressure_prunes_pressure_only() {
        let computed = computed_with_main_sequence_edge();
        let config = config::Config {
            disabled_metrics: vec![
                "main_sequence_refactor_pressure".to_string(),
            ],
            ..Default::default()
        };
        let json = build_metrics_json(&computed, &config);
        let main_sequence = &json["main_sequence"];
        let modules = main_sequence["modules"].as_array().unwrap();
        let api = modules
            .iter()
            .find(|module| module["module"] == "src/api")
            .unwrap();

        assert!(main_sequence.get("distance").is_some());
        assert!(main_sequence.get("refactor_pressure").is_none());
        assert!(main_sequence.get("refactor_target_score").is_some());
        assert!(main_sequence.get("refactor_priority_score").is_some());
        assert!(api.get("distance").is_some());
        assert!(api.get("architecture_priority").is_none());
        assert!(api.get("implementation_complexity").is_none());
        assert!(api.get("refactor_pressure").is_none());
        assert!(api.get("refactor_target_score").is_some());
        assert!(api.get("refactor_priority_score").is_some());
    }

    #[test]
    fn disabled_refactor_target_score_prunes_target_only() {
        let computed = computed_with_main_sequence_edge();
        let config = config::Config {
            disabled_metrics: vec!["refactor_target_score".to_string()],
            ..Default::default()
        };
        let json = build_metrics_json(&computed, &config);
        let main_sequence = &json["main_sequence"];
        let modules = main_sequence["modules"].as_array().unwrap();
        let api = modules
            .iter()
            .find(|module| module["module"] == "src/api")
            .unwrap();

        assert!(main_sequence.get("distance").is_some());
        assert!(main_sequence.get("refactor_pressure").is_some());
        assert!(main_sequence.get("refactor_target_score").is_none());
        assert!(main_sequence.get("refactor_priority_score").is_some());
        assert!(api.get("distance").is_some());
        assert!(api.get("refactor_pressure").is_some());
        assert!(api.get("refactor_payoff").is_none());
        assert!(api.get("refactor_effort").is_none());
        assert!(api.get("refactor_target_score").is_none());
        assert!(api.get("refactor_priority_score").is_some());
    }

    #[test]
    fn disabled_refactor_priority_score_prunes_priority_only() {
        let computed = computed_with_main_sequence_edge();
        let config = config::Config {
            disabled_metrics: vec!["refactor_priority_score".to_string()],
            ..Default::default()
        };
        let json = build_metrics_json(&computed, &config);
        let main_sequence = &json["main_sequence"];
        let modules = main_sequence["modules"].as_array().unwrap();
        let api = modules
            .iter()
            .find(|module| module["module"] == "src/api")
            .unwrap();

        assert!(main_sequence.get("refactor_target_score").is_some());
        assert!(main_sequence.get("refactor_priority_score").is_none());
        assert!(api.get("refactor_target_score").is_some());
        assert!(api.get("project_context_weight").is_none());
        assert!(api.get("refactor_priority_score").is_none());
    }

    #[test]
    fn disabled_cognitive_removes_complexity_json_field() {
        let computed = computed_with_main_sequence_edge();
        let config = config::Config {
            disabled_metrics: vec!["cognitive".to_string()],
            ..Default::default()
        };
        let json = build_metrics_json(&computed, &config);

        assert!(json["complexity"].get("cognitive").is_none());
        assert!(json["complexity"].get("cyclomatic").is_some());
    }

    #[test]
    fn dependency_free_main_sequence_modules_do_not_emit_text_rows() {
        let mut computed = computed_with_main_sequence_edge();
        computed.graph.edges.clear();
        computed.structural = compute_structural(&computed.graph);
        computed.main_sequence = MainSequenceMetrics::compute(&computed.graph);

        let bundle = metrics_bundle(&computed);
        let rows = collect_metric_rows(
            &bundle,
            &config::Config::default(),
            RowSelection::Top(-1),
            &Ignores::default(),
        );

        assert!(
            rows.iter()
                .all(|(metric, _, _, _)| metric != "refactor_priority_score")
        );
    }

    #[test]
    fn text_rows_use_priority_score_order_not_raw_distance_or_pressure() {
        let computed = computed_with_pressure_ranking();
        let bundle = metrics_bundle(&computed);
        let rows = collect_metric_rows(
            &bundle,
            &config::Config::default(),
            RowSelection::Top(-1),
            &Ignores::default(),
        );
        let target_rows: Vec<_> = rows
            .iter()
            .filter(|(metric, _, _, _)| metric == "refactor_priority_score")
            .collect();

        assert_eq!(target_rows[0].1, "core");
        assert!(target_rows.iter().any(|(_, module, _, _)| module == "tiny"));

        let core = computed
            .main_sequence
            .modules
            .iter()
            .find(|module| module.id == "core")
            .unwrap();
        let tiny = computed
            .main_sequence
            .modules
            .iter()
            .find(|module| module.id == "tiny")
            .unwrap();
        assert!(core.distance < tiny.distance);
        assert!(core.refactor_pressure > tiny.refactor_pressure);
        assert!(core.refactor_target_score > tiny.refactor_target_score);
    }
}
