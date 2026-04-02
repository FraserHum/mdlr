mod branches;
#[cfg(test)]
mod branches_test;
mod calls;
#[cfg(test)]
mod calls_test;
mod cognitive;
#[cfg(test)]
mod cognitive_test;
mod field_access;
#[cfg(test)]
mod field_access_test;
mod scopes;
#[cfg(test)]
mod scopes_test;
mod visitor;

use anyhow::{Context, Result};
use clap::Parser;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Cached extraction data for a single source file.
/// Matches the `FileCacheEntry` format from `crates/mdlr/src/cache/types.rs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileCacheEntry {
    source_path: PathBuf,
    units: Vec<mdlr_core::Unit>,
    cached_at: u64,
}

/// mdlr-extract-py: ruff-based Python unit extraction.
#[derive(Parser, Debug)]
#[command(name = "mdlr-extract-py")]
struct Cli {
    /// Root directory to scan for Python files
    #[arg(long)]
    root: PathBuf,

    /// Output directory for per-file JSON results
    #[arg(long)]
    output: PathBuf,

    /// Generation ID for stale-entry filtering
    #[arg(long)]
    generation_id: Option<u64>,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("mdlr-extract-py: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    let root = cli.root.canonicalize().with_context(|| {
        format!("Failed to resolve root path: {}", cli.root.display())
    })?;

    let timestamp = cli.generation_id.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    });

    let files = collect_files(&root)?;

    files.par_iter().for_each(|file_path| {
        if let Err(e) = process_file(file_path, &root, &cli.output, timestamp)
        {
            eprintln!("Failed to process {}: {e:#}", file_path.display());
        }
    });

    Ok(())
}

/// Collect all Python files under root, respecting .gitignore and common excludes.
fn collect_files(root: &std::path::Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();

    let walker = ignore::WalkBuilder::new(root)
        .hidden(true)
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            // Hardcoded excludes for Python ecosystem dirs
            !matches!(
                name.as_ref(),
                "__pycache__"
                    | ".venv"
                    | "venv"
                    | ".tox"
                    | "build"
                    | "dist"
                    | ".eggs"
                    | "node_modules"
            )
        })
        .build();

    for entry in walker {
        let entry = entry?;
        let path = entry.path();

        if !path.is_file() {
            continue;
        }

        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        match ext {
            "py" | "pyi" => files.push(path.to_path_buf()),
            _ => {}
        }
    }

    Ok(files)
}

/// Parse and extract units from a single Python file, writing JSON output.
fn process_file(
    file_path: &std::path::Path,
    root: &std::path::Path,
    output_dir: &std::path::Path,
    timestamp: u64,
) -> Result<()> {
    let source = std::fs::read_to_string(file_path)
        .with_context(|| format!("Failed to read {}", file_path.display()))?;

    let parsed = ruff_python_parser::parse_module(&source);

    let parsed = match parsed {
        Ok(m) => m,
        Err(_) => {
            // Parse errors — skip file
            return Ok(());
        }
    };

    let rel_path = file_path
        .strip_prefix(root)
        .unwrap_or(file_path)
        .to_string_lossy()
        .replace('\\', "/");

    let units = visitor::extract_units(parsed.suite(), &source, &rel_path);

    let entry = FileCacheEntry {
        source_path: PathBuf::from(&rel_path),
        units,
        cached_at: timestamp,
    };

    // Write to <output_dir>/<rel_path>.json
    let mut output_file = output_dir.join(&rel_path);
    output_file.set_extension("json");

    if let Some(parent) = output_file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let json = serde_json::to_string_pretty(&entry)?;
    std::fs::write(&output_file, json).with_context(|| {
        format!("Failed to write {}", output_file.display())
    })?;

    Ok(())
}
