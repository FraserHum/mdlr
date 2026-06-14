//! Handlers for the `mdlr ignore` command.

use anyhow::{Result, bail};

use crate::cache::{CacheStore, IgnoresStore};
use crate::find_project_root;
use std::io::IsTerminal;
use std::path::Path;

/// Valid metric names that can be ignored
const VALID_METRICS: &[&str] = &[
    "fan_in",
    "fan_out",
    "refactor_priority_score",
    "refactor_target_score",
    "main_sequence_refactor_pressure",
    "function_size",
    "params",
    "cyclomatic",
    "max_scope",
    "methods_per_struct",
    "lcom",
    "file_loc",
];

pub fn handle_ignore(
    metric: Option<String>,
    symbol: Option<String>,
    remove: bool,
    list: bool,
    explicit_root: Option<&Path>,
) -> Result<()> {
    if !std::io::stdin().is_terminal() {
        bail!("mdlr ignore is only available in interactive (human) mode");
    }

    let root = find_project_root(Path::new("."), explicit_root);
    let store = CacheStore::open(&root)?;
    let ignores_store = store.ignores();

    if list {
        return handle_ignore_list(&ignores_store);
    }

    let metric = metric.ok_or_else(|| {
        anyhow::anyhow!(
            "Metric name is required. Valid metrics: {}",
            VALID_METRICS.join(", ")
        )
    })?;

    // Validate metric name
    if !VALID_METRICS.contains(&metric.as_str()) {
        bail!(
            "Unknown metric '{}'. Valid metrics: {}",
            metric,
            VALID_METRICS.join(", ")
        );
    }

    let symbol =
        symbol.ok_or_else(|| anyhow::anyhow!("Symbol ID is required."))?;

    if remove {
        handle_ignore_remove(&ignores_store, &metric, &symbol)
    } else {
        handle_ignore_add(&ignores_store, &metric, &symbol)
    }
}

fn handle_ignore_list(store: &IgnoresStore) -> Result<()> {
    let ignores = store.load_ignores()?;

    if ignores.is_empty() {
        println!("No ignores configured.");
        return Ok(());
    }

    // Collect and sort for consistent output
    let mut entries: Vec<_> = ignores.ignores.iter().collect();
    entries.sort_by_key(|(symbol, _)| *symbol);

    for (symbol, metrics) in entries {
        for metric in metrics {
            println!("{}\t{}", metric, symbol);
        }
    }

    Ok(())
}

fn handle_ignore_add(
    store: &IgnoresStore,
    metric: &str,
    symbol: &str,
) -> Result<()> {
    let mut ignores = store.load_ignores()?;

    if ignores.is_ignored(symbol, metric) {
        println!("Already ignoring {} for {}", metric, symbol);
        return Ok(());
    }

    ignores.add(symbol, metric);
    store.save_ignores(&ignores)?;
    println!("Ignoring {} for {}", metric, symbol);

    Ok(())
}

fn handle_ignore_remove(
    store: &IgnoresStore,
    metric: &str,
    symbol: &str,
) -> Result<()> {
    let mut ignores = store.load_ignores()?;

    if !ignores.remove(symbol, metric) {
        println!("No ignore found for {} on {}", metric, symbol);
        return Ok(());
    }

    store.save_ignores(&ignores)?;
    println!("Removed ignore for {} on {}", metric, symbol);

    Ok(())
}
