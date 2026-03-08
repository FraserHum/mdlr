use anyhow::{Context, Result, bail};
use clap::Parser;
use std::env;
use std::io::Write;
use std::path::{Path, PathBuf};

mod cache;
mod cli;
mod config;
mod ignore_commands;
mod json_output;
mod metrics_commands;
mod metrics_rows;
mod symbol_commands;
mod timing;
mod walk;

use cache::{CacheStore, FileCacheEntry};
use cli::{Cli, Command, OutputFormat};
use json_output::{
    build_bucketed_json, build_complexity_json, build_fan_metrics_json,
    build_file_loc_json, build_struct_json,
};
use mdlr_core::{Graph, Unit, build as build_graph};
use mdlr_metrics::{
    BucketedMetrics, ComplexityMetrics, FileLocMetrics, StructMetrics,
    StructuralMetrics, Thresholds,
    compute_with_hub_thresholds as compute_structural,
};
use metrics_rows::{MetricsBundle, collect_metric_rows};
use symbol_commands::{handle_get, handle_ls};

/// Walk up from `start_dir` and find the highest directory with both `.mdlr` and `.git`.
/// Falls back to `start_dir` if none found.
pub fn find_project_root(start_dir: &Path) -> PathBuf {
    let start =
        start_dir.canonicalize().unwrap_or_else(|_| start_dir.to_path_buf());
    let mut current = start.as_path();
    let mut highest: Option<&Path> = None;

    loop {
        if current.join(".mdlr").exists() && current.join(".git").exists() {
            highest = Some(current);
        }

        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }

    highest.map(|p| p.to_path_buf()).unwrap_or(start)
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Check { target, k, pretty, format, timing } => {
            handle_check(target.as_deref(), k, pretty, format, timing)
        }
        Command::Metrics { command } => {
            metrics_commands::handle_metrics(command)
        }
        Command::Prompt => handle_prompt(),
        Command::Ls { path, kind, format } => handle_ls(&path, kind, format),
        Command::Get { symbol, format } => handle_get(&symbol, format),
        Command::Ignore { metric, symbol, remove, list } => {
            ignore_commands::handle_ignore(metric, symbol, remove, list)
        }
    }
}

fn handle_prompt() -> Result<()> {
    print!("{}", include_str!("prompt.md"));
    Ok(())
}

/// Represents what type of filter was specified
enum CheckFilter {
    /// No filter - analyze entire project
    None,
    /// Filter by file path
    File(std::path::PathBuf),
    /// Filter by directory path
    Directory(std::path::PathBuf),
    /// Filter by symbol ID
    Symbol(String),
}

/// Bundle of all computed metrics for a graph
struct ComputedMetrics {
    graph: Graph,
    structural: StructuralMetrics,
    complexity: ComplexityMetrics,
    struct_metrics: StructMetrics,
    file_loc: FileLocMetrics,
}

/// Context for the check command, bundling common resources
struct CheckContext {
    cwd: std::path::PathBuf,
    store: CacheStore,
    config: config::Config,
    /// Generation ID (unix timestamp) shared across all extractors.
    /// Cache entries with `cached_at < generation_id` are stale.
    generation_id: u64,
}

impl CheckContext {
    fn new() -> Result<Self> {
        let cwd = env::current_dir()?;
        let root = find_project_root(&cwd);
        let store = CacheStore::open(&root)?;
        let config = config::load_from_dir(store.root())?;
        let generation_id = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        Ok(CheckContext { cwd, store, config, generation_id })
    }
}

/// Parse target string into a CheckFilter
fn parse_check_filter(target: Option<&str>, cwd: &Path) -> CheckFilter {
    if let Some(target_str) = target {
        let target_path = if Path::new(target_str).is_absolute() {
            Path::new(target_str).to_path_buf()
        } else {
            cwd.join(target_str)
        };

        if target_path.exists() {
            let canonical = target_path.canonicalize().unwrap_or(target_path);
            if canonical.is_file() {
                CheckFilter::File(canonical)
            } else {
                CheckFilter::Directory(canonical)
            }
        } else {
            CheckFilter::Symbol(target_str.to_string())
        }
    } else {
        CheckFilter::None
    }
}

/// Find the `mdlr-extract-rust` binary, checking next to our own binary first.
fn find_extract_rust_binary() -> Result<PathBuf> {
    if let Ok(current_exe) = env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            let sibling = dir.join("mdlr-extract-rust");
            if sibling.exists() {
                return Ok(sibling);
            }
        }
    }
    // Check if it's on PATH
    if let Ok(output) =
        std::process::Command::new("which").arg("mdlr-extract-rust").output()
    {
        if output.status.success() {
            let path =
                String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Ok(PathBuf::from(path));
            }
        }
    }
    bail!(
        "Could not find mdlr-extract-rust binary. \
         Build it with: cargo install --path tools/mdlr-extract-rust"
    );
}

/// Shell out to `mdlr-extract-rust` to extract units from all workspace members.
///
/// Only runs if a `Cargo.toml` exists at the workspace root.
#[tracing::instrument(name = "extract", skip_all)]
fn extract_rust(store: &CacheStore, generation_id: u64) -> Result<()> {
    let workspace_root = store.root();

    // Skip if no Cargo workspace
    let manifest_path = workspace_root.join("Cargo.toml");
    if !manifest_path.exists() {
        return Ok(());
    }

    let extract_bin = find_extract_rust_binary()?;

    let status = std::process::Command::new(&extract_bin)
        .arg("--manifest-path")
        .arg(&manifest_path)
        .arg("--output")
        .arg(store.cache_dir())
        .arg("--generation-id")
        .arg(generation_id.to_string())
        .env("MDLR_QUIET_DIAGNOSTICS", "1")
        .current_dir(workspace_root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .context("Failed to run mdlr-extract-rust")?;

    if !status.success() {
        eprintln!(
            "Warning: HIR extraction had errors (results may be partial)"
        );
    }

    Ok(())
}

/// Find the `mdlr-extract-ts` binary, checking next to our own binary first.
fn find_extract_ts_binary() -> Option<PathBuf> {
    if let Ok(current_exe) = env::current_exe() {
        if let Some(dir) = current_exe.parent() {
            let sibling = dir.join("mdlr-extract-ts");
            if sibling.exists() {
                return Some(sibling);
            }
        }
    }
    if let Ok(output) =
        std::process::Command::new("which").arg("mdlr-extract-ts").output()
    {
        if output.status.success() {
            let path =
                String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                return Some(PathBuf::from(path));
            }
        }
    }
    None
}

/// Detect whether the project has TypeScript/JavaScript files.
fn has_ts_files(root: &Path) -> bool {
    // Quick check: tsconfig.json or package.json
    if root.join("tsconfig.json").exists()
        || root.join("package.json").exists()
    {
        return true;
    }
    // Fallback: look for .ts/.tsx/.js/.jsx files (shallow check)
    let walker =
        ignore::WalkBuilder::new(root).hidden(true).max_depth(Some(3)).build();
    for entry in walker.flatten() {
        if let Some(ext) = entry.path().extension().and_then(|e| e.to_str()) {
            if matches!(ext, "ts" | "tsx" | "js" | "jsx") {
                return true;
            }
        }
    }
    false
}

/// Shell out to `mdlr-extract-ts` to extract units from TS/JS files.
#[tracing::instrument(name = "extract_ts", skip_all)]
fn extract_ts(store: &CacheStore, generation_id: u64) -> Result<()> {
    let extract_bin = match find_extract_ts_binary() {
        Some(bin) => bin,
        None => return Ok(()), // silently skip if not available
    };

    let workspace_root = store.root();
    if !has_ts_files(workspace_root) {
        return Ok(());
    }

    let status = std::process::Command::new(&extract_bin)
        .arg("--root")
        .arg(workspace_root)
        .arg("--output")
        .arg(store.cache_dir())
        .arg("--generation-id")
        .arg(generation_id.to_string())
        .current_dir(workspace_root)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .context("Failed to run mdlr-extract-ts")?;

    if !status.success() {
        eprintln!(
            "Warning: TS extraction had errors (results may be partial)"
        );
    }

    Ok(())
}

/// Recursively load FileCacheEntry JSON files from a directory.
#[tracing::instrument(name = "load_cache", skip_all)]
fn load_entries_from_dir(
    dir: &Path,
    entries: &mut Vec<FileCacheEntry>,
) -> Result<()> {
    if !dir.exists() {
        return Ok(());
    }
    for item in std::fs::read_dir(dir)? {
        let item = item?;
        let path = item.path();
        if path.is_dir() {
            load_entries_from_dir(&path, entries)?;
        } else if path.extension().and_then(|e| e.to_str()) == Some("json") {
            let content =
                std::fs::read_to_string(&path).with_context(|| {
                    format!("Failed to read {}", path.display())
                })?;
            let entry: FileCacheEntry = serde_json::from_str(&content)
                .with_context(|| {
                    format!("Failed to parse {}", path.display())
                })?;
            entries.push(entry);
        }
    }
    Ok(())
}

/// Check if a file path passes the filter
fn passes_path_filter(file_path: &Path, filter: &CheckFilter) -> bool {
    match filter {
        CheckFilter::File(filter_path) => file_path == *filter_path,
        CheckFilter::Directory(filter_path) => {
            file_path.starts_with(filter_path)
        }
        CheckFilter::Symbol(_) | CheckFilter::None => true,
    }
}
/// Save cache entries based on filter type
/// Compute all metrics from units
#[tracing::instrument(name = "compute_metrics", skip_all)]
fn compute_all_metrics(
    units: Vec<Unit>,
    config: &config::Config,
) -> ComputedMetrics {
    let graph =
        tracing::info_span!("build_graph").in_scope(|| build_graph(units));
    let structural = compute_structural(
        &graph,
        config.hub.min_fan_in,
        config.hub.min_fan_out,
    );
    let complexity = ComplexityMetrics::compute(&graph);
    let struct_metrics = StructMetrics::compute(&graph);
    let file_loc = FileLocMetrics::compute(&graph);

    ComputedMetrics { graph, structural, complexity, struct_metrics, file_loc }
}

/// Extract symbol filter string from CheckFilter
fn get_symbol_filter(filter: &CheckFilter) -> Option<&str> {
    match filter {
        CheckFilter::Symbol(s) => Some(s.as_str()),
        _ => None,
    }
}

/// Format and print text output
fn format_text_output(
    computed: &ComputedMetrics,
    config: &config::Config,
    k: i32,
    pretty: bool,
    filter: &CheckFilter,
    store: &CacheStore,
) -> Result<()> {
    let bundle = MetricsBundle {
        structural: &computed.structural,
        complexity: &computed.complexity,
        struct_metrics: &computed.struct_metrics,
        file_loc: &computed.file_loc,
    };
    let symbol_filter = get_symbol_filter(filter);
    let ignores = store.ignores().load_ignores().unwrap_or_default();
    let rows =
        collect_metric_rows(&bundle, config, k, symbol_filter, &ignores);

    if pretty {
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
    extracted_count: usize,
    filter: &CheckFilter,
) -> Result<()> {
    // When filtering by symbol, output specific metrics for that symbol
    if let CheckFilter::Symbol(symbol_id) = filter {
        let output = build_symbol_json(computed, config, symbol_id);
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    let thresholds = Thresholds::default();
    let bucketed =
        BucketedMetrics::from_metrics(&computed.structural, &thresholds);

    let partial_count =
        computed.graph.units.iter().filter(|u| u.partial).count();

    let output = serde_json::json!({
        "files": {
            "extracted": extracted_count,
        },
        "units": computed.graph.units.len(),
        "partial_units": partial_count,
        "edges": computed.graph.edges.len(),
        "metrics": {
            "dag_density": build_bucketed_json(&bucketed.dag_density),
            "fan_in": build_fan_metrics_json(&bucketed.fan_in, &computed.structural.fan_in.distribution),
            "fan_out": build_fan_metrics_json(&bucketed.fan_out, &computed.structural.fan_out.distribution),
            "complexity": build_complexity_json(&computed.complexity),
            "struct": build_struct_json(&computed.struct_metrics),
            "file_loc": build_file_loc_json(&computed.file_loc),
        }
    });
    println!("{}", serde_json::to_string_pretty(&output)?);

    Ok(())
}

/// Insert a metric entry for a symbol if found in the distribution.
fn insert_symbol_metric(
    metrics: &mut serde_json::Map<String, serde_json::Value>,
    name: &str,
    distribution: &[(String, usize)],
    thresholds: &config::MetricThresholds,
    symbol_id: &str,
) {
    if let Some((_, value)) = distribution.iter().find(|(n, _)| n == symbol_id)
    {
        let bucket = thresholds.evaluate(*value as f64);
        metrics.insert(
            name.to_string(),
            serde_json::json!({ "value": value, "bucket": bucket.to_string() }),
        );
    }
}

/// Build JSON output for a specific symbol
fn build_symbol_json(
    computed: &ComputedMetrics,
    config: &config::Config,
    symbol_id: &str,
) -> serde_json::Value {
    let mut metrics = serde_json::Map::new();
    let t = &config.thresholds;

    let metric_sources: &[(
        &str,
        &[(String, usize)],
        &config::MetricThresholds,
    )] = &[
        ("fan_in", &computed.structural.fan_in.distribution, &t.fan_in_max),
        ("fan_out", &computed.structural.fan_out.distribution, &t.fan_out_max),
        (
            "function_size",
            &computed.complexity.size.distribution,
            &t.function_size,
        ),
        ("params", &computed.complexity.params.distribution, &t.params),
        (
            "cyclomatic",
            &computed.complexity.cyclomatic.distribution,
            &t.cyclomatic,
        ),
        (
            "max_scope",
            &computed.complexity.max_scope.distribution,
            &t.max_scope,
        ),
        (
            "methods_per_struct",
            &computed.struct_metrics.methods_per_struct.distribution,
            &t.methods_per_struct,
        ),
        ("lcom", &computed.struct_metrics.lcom.distribution, &t.lcom),
    ];

    for (name, distribution, thresholds) in metric_sources {
        insert_symbol_metric(
            &mut metrics,
            name,
            distribution,
            thresholds,
            symbol_id,
        );
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

/// Set up timing instrumentation if requested, returns a printer to call after work is done.
fn setup_timing(enabled: bool) -> Option<timing::TimingPrinter> {
    if !enabled {
        return None;
    }
    let (layer, printer) = timing::TimingLayer::new();
    let subscriber = tracing_subscriber::registry::Registry::default();
    use tracing_subscriber::layer::SubscriberExt;
    let subscriber = subscriber.with(layer);
    tracing::subscriber::set_global_default(subscriber)
        .expect("failed to set tracing subscriber");
    Some(printer)
}

/// Load cache entries and collect units matching the filter.
/// Entries with `cached_at < generation_id` are stale and skipped.
fn load_filtered_units(
    store: &CacheStore,
    filter: &CheckFilter,
    generation_id: u64,
) -> Result<(Vec<FileCacheEntry>, Vec<Unit>)> {
    let mut all_entries = Vec::new();
    load_entries_from_dir(&store.cache_dir(), &mut all_entries)?;

    let mut entries = Vec::new();
    let mut units = Vec::new();
    for entry in all_entries {
        if entry.cached_at < generation_id {
            continue; // stale entry from a previous extraction
        }
        let file_path = store.root().join(&entry.source_path);
        if passes_path_filter(&file_path, filter) {
            units.extend(entry.units.clone());
        }
        entries.push(entry);
    }

    Ok((entries, units))
}

/// Extract, load, validate, and compute all metrics.
fn extract_and_analyze(
    ctx: &CheckContext,
    filter: &CheckFilter,
) -> Result<(ComputedMetrics, usize)> {
    extract_rust(&ctx.store, ctx.generation_id)?;
    if let Err(e) = extract_ts(&ctx.store, ctx.generation_id) {
        eprintln!("Warning: TS extraction failed: {e:#}");
    }

    let (entries, units) =
        load_filtered_units(&ctx.store, filter, ctx.generation_id)?;

    if let CheckFilter::Symbol(symbol_id) = filter {
        if !units.iter().any(|u| u.id == *symbol_id) {
            bail!(
                "Symbol '{}' not found. Run 'mdlr ls' to see available symbols.",
                symbol_id
            );
        }
    }

    let entry_count = entries.len();
    let computed = compute_all_metrics(units, &ctx.config);
    Ok((computed, entry_count))
}

fn handle_check(
    target: Option<&str>,
    k: i32,
    pretty: bool,
    format: OutputFormat,
    timing: bool,
) -> Result<()> {
    let printer = setup_timing(timing);
    let ctx = CheckContext::new()?;
    let filter = parse_check_filter(target, &ctx.cwd);

    let (computed, entry_count) = extract_and_analyze(&ctx, &filter)?;

    let result = match format {
        OutputFormat::Text => format_text_output(
            &computed,
            &ctx.config,
            k,
            pretty,
            &filter,
            &ctx.store,
        ),
        OutputFormat::Json => {
            format_json_output(&computed, &ctx.config, entry_count, &filter)
        }
    };

    if let Some(printer) = printer {
        printer.print();
    }

    result
}
