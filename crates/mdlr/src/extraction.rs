use anyhow::{Context, Result, bail};
use std::env;
use std::path::{Path, PathBuf};

use crate::cache::{CacheStore, FileCacheEntry};

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
pub fn extract_rust(store: &CacheStore, generation_id: u64) -> Result<()> {
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
pub fn extract_ts(store: &CacheStore, generation_id: u64) -> Result<()> {
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
pub fn load_entries_from_dir(
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
