mod branches;
mod calls;
mod cognitive;
mod field_access;
mod path_util;
mod scopes;
mod visitor;
mod walk;

use anyhow::{Context, Result};
use clap::Parser;
use ra_ap_hir::{attach_db, Crate, Semantics};
use ra_ap_load_cargo::{
    load_workspace_at, LoadCargoConfig, ProcMacroServerChoice,
};
use ra_ap_project_model::{CargoConfig, RustLibSource};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

/// Cached extraction data for a single source file.
/// Matches the `FileCacheEntry` format from `crates/mdlr/src/cache/types.rs`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct FileCacheEntry {
    source_path: PathBuf,
    units: Vec<mdlr_core::Unit>,
    cached_at: u64,
}

/// mdlr-extract-rust: Rust unit extraction via rust-analyzer.
///
/// Uses ra_ap_* crates (rust-analyzer's published APIs) to load and analyze
/// Rust workspaces, extracting unit information with full type resolution.
#[derive(Parser, Debug)]
#[command(name = "mdlr-extract-rust")]
struct Cli {
    /// Path to the workspace Cargo.toml
    #[arg(long)]
    manifest_path: Option<PathBuf>,

    /// Output directory for per-file JSON results (mirrors source tree structure)
    #[arg(long)]
    output: Option<PathBuf>,

    /// Package names to extract from (if empty, extracts from all workspace members)
    #[arg(long)]
    package: Vec<String>,

    /// Generation ID to stamp on all cache entries (used for stale-entry filtering)
    #[arg(long)]
    generation_id: Option<u64>,
}

fn main() {
    if let Err(e) = run() {
        eprintln!("mdlr-extract-rust: {e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();

    let manifest_path =
        cli.manifest_path.as_ref().context("--manifest-path is required")?;
    let output_dir = cli.output.as_ref().context("--output is required")?;

    let manifest_path = manifest_path.canonicalize().with_context(|| {
        format!("Failed to resolve manifest path: {}", manifest_path.display())
    })?;

    let workspace_dir = manifest_path
        .parent()
        .context("manifest path has no parent directory")?;

    // Load the workspace using rust-analyzer's infrastructure
    let cargo_config = CargoConfig {
        sysroot: Some(RustLibSource::Discover),
        ..CargoConfig::default()
    };
    let load_config = LoadCargoConfig {
        load_out_dirs_from_check: true,
        with_proc_macro_server: ProcMacroServerChoice::None,
        prefill_caches: false,
    };

    let (db, vfs, _proc_macro) = load_workspace_at(
        workspace_dir,
        &cargo_config,
        &load_config,
        &|_msg| {},
    )
    .context("Failed to load workspace")?;

    let cwd = std::env::current_dir().unwrap_or_default();

    // Determine which crates to extract
    let target_packages: HashSet<String> = if !cli.package.is_empty() {
        cli.package.iter().cloned().collect()
    } else {
        HashSet::new()
    };

    // Wrap all semantic analysis in attach_db — required for the trait solver's TLS.
    let units_by_file = attach_db(&db, || {
        let sema = Semantics::new(&db);

        let all_crates = Crate::all(&db);
        let target_crates: Vec<Crate> = all_crates
            .into_iter()
            .filter(|krate| {
                let name = krate
                    .display_name(&db)
                    .map(|n| n.to_string())
                    .unwrap_or_default();

                let normalized_name = name.replace('-', "_");
                if target_packages.is_empty() {
                    is_local_crate(&db, krate, &vfs, &cwd)
                } else {
                    target_packages.iter().any(|pkg| {
                        let normalized_pkg = pkg.replace('-', "_");
                        normalized_pkg == normalized_name
                    })
                }
            })
            .collect();

        visitor::extract_units(&db, &sema, &vfs, &target_crates, &cwd)
    });

    let timestamp = cli.generation_id.unwrap_or_else(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0)
    });

    for (source_path, units) in units_by_file {
        let entry = FileCacheEntry {
            source_path: PathBuf::from(&source_path),
            units,
            cached_at: timestamp,
        };

        let mut output_file = output_dir.join(&source_path);
        output_file.set_extension("json");

        if let Some(parent) = output_file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        match serde_json::to_string_pretty(&entry) {
            Ok(json) => {
                if let Err(e) = std::fs::write(&output_file, json) {
                    eprintln!(
                        "Failed to write output for {}: {}",
                        source_path, e
                    );
                }
            }
            Err(e) => {
                eprintln!(
                    "Failed to serialize output for {}: {}",
                    source_path, e
                );
            }
        }
    }

    Ok(())
}

/// Check if a crate has source files under the current working directory.
/// This is a heuristic for detecting workspace members vs external dependencies.
fn is_local_crate(
    db: &ra_ap_ide_db::RootDatabase,
    krate: &Crate,
    vfs: &ra_ap_vfs::Vfs,
    cwd: &std::path::Path,
) -> bool {
    let root_file = krate.root_file(db);
    let vfs_path = vfs.file_path(root_file);
    if let Some(abs_path) = vfs_path.as_path() {
        let file_path: &std::path::Path = abs_path.as_ref();
        file_path.starts_with(cwd)
    } else {
        false
    }
}
