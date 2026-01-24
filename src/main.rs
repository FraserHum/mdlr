use anyhow::{bail, Result};
use clap::Parser;
use mdlr::cache::{get_file_metadata, now_timestamp, CacheStore, FileCacheEntry, ProjectIndex};
use mdlr::cli::{Cli, Command, OutputFormat};
use mdlr::config;
use mdlr::extract::{extractor_for_path, Extractor};
use mdlr::graph::{Edge, EdgeKind, Graph, Unit, UnitKind};
use mdlr::metrics::{BucketedMetrics, MetricsDisplay, TagMetrics};
use mdlr::walk::SourceWalker;
use std::collections::HashSet;
use std::fs;
use std::path::Path;

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Todo { path, all, format } => handle_todo(&path, all, format),
        Command::Analyze { path, force, format } => handle_analyze(&path, force, format),
        Command::Export { path, format } => handle_export(&path, format),
        Command::Ls { path, kind, format } => handle_ls(&path, kind, format),
        Command::Get { symbol, format } => handle_get(&symbol, format),
        Command::Tag {
            symbol,
            add,
            remove,
            clear,
            list,
            format,
        } => handle_tag(symbol, add, remove, clear, list, format),
    }
}

fn handle_todo(path: &Path, all: bool, format: OutputFormat) -> Result<()> {
    let store = CacheStore::open(path)?;
    let index = store.load_index()?;
    let walker = SourceWalker::new(store.root());

    let mut new_files = Vec::new();
    let mut changed_files = Vec::new();
    let mut untagged_files = Vec::new();

    for file_path in walker.walk() {
        let relative = file_path
            .strip_prefix(store.root())
            .unwrap_or(&file_path)
            .to_path_buf();

        let current_meta = get_file_metadata(&file_path)?;

        match index.files.get(&relative) {
            None => {
                new_files.push(relative);
            }
            Some(cached_meta) => {
                if cached_meta.mtime != current_meta.mtime || cached_meta.size != current_meta.size
                {
                    changed_files.push(relative);
                } else if all {
                    if let Ok(Some(entry)) = store.load_entry(&file_path) {
                        if entry.units.iter().any(|u| u.tags.is_empty()) {
                            untagged_files.push(relative);
                        }
                    }
                }
            }
        }
    }

    match format {
        OutputFormat::Text => {
            let has_work = !new_files.is_empty() || !changed_files.is_empty();
            let has_untagged = !untagged_files.is_empty();

            if !has_work && !has_untagged {
                println!("All files are up to date.");
                return Ok(());
            }

            if !new_files.is_empty() {
                println!("New files ({}):", new_files.len());
                for f in &new_files {
                    println!("  {}", f.display());
                }
                println!();
            }

            if !changed_files.is_empty() {
                println!("Changed files ({}):", changed_files.len());
                for f in &changed_files {
                    println!("  {}", f.display());
                }
                println!();
            }

            if all && !untagged_files.is_empty() {
                println!("Files with untagged units ({}):", untagged_files.len());
                for f in &untagged_files {
                    println!("  {}", f.display());
                }
                println!();
            }

            let total = new_files.len() + changed_files.len();
            if total > 0 {
                println!("Run 'mdlr analyze' to update {} file(s).", total);
            }
        }
        OutputFormat::Json => {
            let output = serde_json::json!({
                "new": new_files,
                "changed": changed_files,
                "untagged": if all { untagged_files } else { vec![] },
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
    }

    Ok(())
}

fn handle_analyze(path: &Path, force: bool, format: OutputFormat) -> Result<()> {
    let store = CacheStore::open(path)?;
    let config = config::load()?;
    let walker = SourceWalker::new(store.root());

    let mut index = if force {
        ProjectIndex::default()
    } else {
        store.load_index()?
    };

    let mut all_units: Vec<Unit> = Vec::new();
    let mut extracted_count = 0;
    let mut cached_count = 0;

    for file_path in walker.walk() {
        let relative = file_path
            .strip_prefix(store.root())
            .unwrap_or(&file_path)
            .to_path_buf();

        let current_meta = get_file_metadata(&file_path)?;
        let is_stale = force
            || index
                .files
                .get(&relative)
                .map(|m| m.mtime != current_meta.mtime || m.size != current_meta.size)
                .unwrap_or(true);

        let units = if is_stale {
            if let Some(extractor) = extractor_for_path(&file_path) {
                match extract_file(&file_path, extractor.as_ref()) {
                    Ok(units) => {
                        let entry = FileCacheEntry {
                            source_path: relative.clone(),
                            mtime: current_meta.mtime,
                            size: current_meta.size,
                            units: units.clone(),
                            cached_at: now_timestamp(),
                        };
                        store.save_entry(&entry)?;
                        index.files.insert(relative, current_meta);
                        extracted_count += 1;
                        units
                    }
                    Err(e) => {
                        eprintln!("Warning: Failed to extract {}: {}", file_path.display(), e);
                        continue;
                    }
                }
            } else {
                continue;
            }
        } else {
            match store.load_entry(&file_path)? {
                Some(entry) => {
                    cached_count += 1;
                    entry.units
                }
                None => continue,
            }
        };

        all_units.extend(units);
    }

    index.last_scan = now_timestamp();
    store.save_index(&index)?;

    let graph = build_graph(all_units);
    let metrics = mdlr::metrics::compute(&graph);
    let semantic_tags = store.load_tags()?;
    let tag_metrics = TagMetrics::compute(&graph, &semantic_tags);

    match format {
        OutputFormat::Text => {
            println!("Analysis complete");
            println!();
            println!(
                "Files: {} extracted, {} from cache",
                extracted_count, cached_count
            );
            println!(
                "Graph: {} units, {} edges",
                graph.units.len(),
                graph.edges.len()
            );
            println!();
            let display = MetricsDisplay::new(&metrics, &config);
            print!("{}", display);

            if tag_metrics.has_tags() {
                println!();
                print!("{}", tag_metrics);
            }
        }
        OutputFormat::Json => {
            let bucketed = BucketedMetrics::from_metrics(&metrics, &config);

            let namespace_distribution: serde_json::Map<String, serde_json::Value> = tag_metrics
                .namespace_distribution
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::json!(v)))
                .collect();

            let namespace_values: serde_json::Map<String, serde_json::Value> = tag_metrics
                .namespace_values
                .iter()
                .map(|(ns, values)| {
                    let values_map: serde_json::Map<String, serde_json::Value> = values
                        .iter()
                        .map(|(k, v)| (k.clone(), serde_json::json!(v)))
                        .collect();
                    (ns.clone(), serde_json::Value::Object(values_map))
                })
                .collect();

            let output = serde_json::json!({
                "files": {
                    "extracted": extracted_count,
                    "cached": cached_count,
                },
                "units": graph.units.len(),
                "edges": graph.edges.len(),
                "metrics": {
                    "dag_density": {
                        "value": bucketed.dag_density.value,
                        "bucket": bucketed.dag_density.bucket,
                    },
                    "fan_in": {
                        "max": {
                            "value": bucketed.fan_in.max.value as usize,
                            "bucket": bucketed.fan_in.max.bucket,
                        },
                        "mean": {
                            "value": bucketed.fan_in.mean.value,
                            "bucket": bucketed.fan_in.mean.bucket,
                        },
                    },
                    "fan_out": {
                        "max": {
                            "value": bucketed.fan_out.max.value as usize,
                            "bucket": bucketed.fan_out.max.bucket,
                        },
                        "mean": {
                            "value": bucketed.fan_out.mean.value,
                            "bucket": bucketed.fan_out.mean.bucket,
                        },
                    },
                    "tag_coverage": {
                        "total_units": tag_metrics.total_units,
                        "tagged_units": tag_metrics.tagged_units,
                        "coverage": tag_metrics.tag_coverage,
                        "by_namespace": namespace_distribution,
                        "namespace_values": namespace_values,
                    }
                }
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
    }

    Ok(())
}

fn handle_export(path: &Path, format: OutputFormat) -> Result<()> {
    let store = CacheStore::open(path)?;
    let walker = SourceWalker::new(store.root());

    let mut all_units: Vec<Unit> = Vec::new();

    for file_path in walker.walk() {
        if let Ok(Some(entry)) = store.load_entry(&file_path) {
            all_units.extend(entry.units);
        }
    }

    let graph = build_graph(all_units);

    match format {
        OutputFormat::Json => {
            let json = mdlr::graph::serialize::to_json(&graph)?;
            println!("{}", json);
        }
        OutputFormat::Text => {
            println!("Graph");
            println!();
            println!("Units ({}):", graph.units.len());
            for unit in &graph.units {
                println!("  {} ({:?}) - {:?}", unit.id, unit.kind, unit.file);
            }
            println!();
            println!("Edges ({}):", graph.edges.len());
            for edge in &graph.edges {
                println!("  {} -> {} ({:?})", edge.from, edge.to, edge.kind);
            }
        }
    }

    Ok(())
}

fn handle_ls(path: &Path, kind_filter: Option<String>, format: OutputFormat) -> Result<()> {
    let store = CacheStore::open(path)?;
    let walker = SourceWalker::new(store.root());
    let semantic_tags = store.load_tags()?;

    let kind_filter = kind_filter.map(|k| parse_unit_kind(&k)).transpose()?;

    let mut all_units: Vec<(Unit, Vec<String>)> = Vec::new();

    for file_path in walker.walk() {
        if let Ok(Some(entry)) = store.load_entry(&file_path) {
            for unit in entry.units {
                if let Some(ref filter) = kind_filter {
                    if &unit.kind != filter {
                        continue;
                    }
                }
                let tags = semantic_tags.get_tags(&unit.id).to_vec();
                all_units.push((unit, tags));
            }
        }
    }

    match format {
        OutputFormat::Text => {
            if all_units.is_empty() {
                println!("No symbols found. Run 'mdlr analyze' first.");
                return Ok(());
            }

            println!("{:<40} {:<10} {:<30} {:>6}-{:<6} {}", "ID", "Kind", "File", "Start", "End", "Tags");
            println!("{}", "-".repeat(120));
            for (unit, tags) in &all_units {
                let kind_str = format!("{:?}", unit.kind);
                let file_str = unit.file.display().to_string();
                let tags_str = if tags.is_empty() {
                    String::new()
                } else {
                    tags.join(", ")
                };
                println!(
                    "{:<40} {:<10} {:<30} {:>6}-{:<6} {}",
                    truncate(&unit.id, 40),
                    kind_str,
                    truncate(&file_str, 30),
                    unit.span.start_line,
                    unit.span.end_line,
                    tags_str
                );
            }
            println!();
            println!("Total: {} symbols", all_units.len());
        }
        OutputFormat::Json => {
            let output: Vec<_> = all_units
                .into_iter()
                .map(|(unit, tags)| {
                    serde_json::json!({
                        "id": unit.id,
                        "kind": format!("{:?}", unit.kind),
                        "file": unit.file,
                        "span": {
                            "start_line": unit.span.start_line,
                            "end_line": unit.span.end_line,
                        },
                        "tags": tags,
                    })
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
    }

    Ok(())
}

fn handle_get(symbol: &str, format: OutputFormat) -> Result<()> {
    let store = CacheStore::open(Path::new("."))?;
    let walker = SourceWalker::new(store.root());
    let semantic_tags = store.load_tags()?;

    // Find the unit
    let mut found_unit: Option<Unit> = None;
    for file_path in walker.walk() {
        if let Ok(Some(entry)) = store.load_entry(&file_path) {
            for unit in entry.units {
                if unit.id == symbol {
                    found_unit = Some(unit);
                    break;
                }
            }
        }
        if found_unit.is_some() {
            break;
        }
    }

    let unit = match found_unit {
        Some(u) => u,
        None => bail!("Symbol '{}' not found. Run 'mdlr ls' to see available symbols.", symbol),
    };

    // Read the source file and extract the span
    let source_path = store.root().join(&unit.file);
    let source = fs::read_to_string(&source_path)?;
    let lines: Vec<&str> = source.lines().collect();

    let start_idx = unit.span.start_line.saturating_sub(1);
    let end_idx = unit.span.end_line.min(lines.len());
    let content: String = lines[start_idx..end_idx].join("\n");

    let tags = semantic_tags.get_tags(&unit.id).to_vec();

    match format {
        OutputFormat::Text => {
            println!("Symbol: {}", unit.id);
            println!("Kind: {:?}", unit.kind);
            println!("File: {}:{}-{}", unit.file.display(), unit.span.start_line, unit.span.end_line);
            if !tags.is_empty() {
                println!("Tags: {}", tags.join(", "));
            }
            println!();
            println!("{}", content);
        }
        OutputFormat::Json => {
            let output = serde_json::json!({
                "id": unit.id,
                "kind": format!("{:?}", unit.kind),
                "file": unit.file,
                "span": {
                    "start_line": unit.span.start_line,
                    "end_line": unit.span.end_line,
                },
                "tags": tags,
                "content": content,
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        }
    }

    Ok(())
}

fn handle_tag(
    symbol: Option<String>,
    add: Vec<String>,
    remove: Option<String>,
    clear: bool,
    list: bool,
    format: OutputFormat,
) -> Result<()> {
    let store = CacheStore::open(Path::new("."))?;
    let mut semantic_tags = store.load_tags()?;

    // List all tags
    if list {
        match format {
            OutputFormat::Text => {
                if semantic_tags.tags.is_empty() {
                    println!("No semantic tags defined.");
                    return Ok(());
                }
                println!("{:<40} {}", "Symbol", "Tags");
                println!("{}", "-".repeat(80));
                let mut entries: Vec<_> = semantic_tags.tags.iter().collect();
                entries.sort_by_key(|(k, _)| k.as_str());
                for (unit_id, tags) in entries {
                    println!("{:<40} {}", truncate(unit_id, 40), tags.join(", "));
                }
            }
            OutputFormat::Json => {
                println!("{}", serde_json::to_string_pretty(&semantic_tags.tags)?);
            }
        }
        return Ok(());
    }

    // Require symbol for add/remove/clear operations
    let symbol = match symbol {
        Some(s) => s,
        None => bail!("Symbol ID is required. Use 'mdlr tag --list' to see all tags, or specify a symbol."),
    };

    // Verify symbol exists
    let walker = SourceWalker::new(store.root());
    let mut symbol_exists = false;
    for file_path in walker.walk() {
        if let Ok(Some(entry)) = store.load_entry(&file_path) {
            if entry.units.iter().any(|u| u.id == symbol) {
                symbol_exists = true;
                break;
            }
        }
    }
    if !symbol_exists {
        bail!("Symbol '{}' not found. Run 'mdlr ls' to see available symbols.", symbol);
    }

    // Clear tags
    if clear {
        let removed = semantic_tags.clear_tags(&symbol);
        store.save_tags(&semantic_tags)?;
        if removed {
            println!("Cleared all tags from '{}'", symbol);
        } else {
            println!("No tags to clear on '{}'", symbol);
        }
        return Ok(());
    }

    // Remove a tag
    if let Some(ref tag) = remove {
        let removed = semantic_tags.remove_tag(&symbol, tag);
        store.save_tags(&semantic_tags)?;
        if removed {
            println!("Removed tag '{}' from '{}'", tag, symbol);
        } else {
            println!("Tag '{}' not found on '{}'", tag, symbol);
        }
        return Ok(());
    }

    // Add tags
    if !add.is_empty() {
        for tag in &add {
            semantic_tags.add_tag(&symbol, tag)?;
        }
        store.save_tags(&semantic_tags)?;
        println!("Added {} tag(s) to '{}'", add.len(), symbol);
        return Ok(());
    }

    // Show current tags for symbol
    let tags = semantic_tags.get_tags(&symbol);
    match format {
        OutputFormat::Text => {
            if tags.is_empty() {
                println!("No tags on '{}'", symbol);
            } else {
                println!("Tags on '{}': {}", symbol, tags.join(", "));
            }
        }
        OutputFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&tags)?);
        }
    }

    Ok(())
}

fn parse_unit_kind(s: &str) -> Result<UnitKind> {
    match s.to_lowercase().as_str() {
        "function" | "fn" => Ok(UnitKind::Function),
        "struct" => Ok(UnitKind::Struct),
        "module" | "mod" => Ok(UnitKind::Module),
        "trait" => Ok(UnitKind::Trait),
        "impl" => Ok(UnitKind::Impl),
        _ => bail!("Unknown unit kind '{}'. Valid kinds: function, struct, module, trait, impl", s),
    }
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

fn build_graph(units: Vec<Unit>) -> Graph {
    let mut graph = Graph::new();
    let unit_ids: HashSet<_> = units.iter().map(|u| u.id.clone()).collect();

    for unit in &units {
        for call in &unit.calls {
            if unit_ids.contains(call) {
                graph.add_edge(Edge {
                    from: unit.id.clone(),
                    to: call.clone(),
                    kind: EdgeKind::Calls,
                });
            }
        }
    }

    for unit in units {
        graph.add_unit(unit);
    }

    graph
}

fn extract_file(path: &Path, extractor: &dyn Extractor) -> Result<Vec<Unit>> {
    let source = fs::read_to_string(path)?;
    extractor.extract(&source, path)
}
