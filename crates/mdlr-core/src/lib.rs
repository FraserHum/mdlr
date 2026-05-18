pub mod graph;

pub use graph::{
    Edge, EdgeKind, Graph, Span, Unit, UnitKind, build, build_with_progress,
};

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Cached extraction data for a single source file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileCacheEntry {
    pub source_path: PathBuf,
    pub units: Vec<Unit>,
    pub cached_at: u64,
}

/// Build a cache file path by appending `.suffix` to the source file name.
///
/// Appends instead of replacing so files that share a stem but differ in
/// extension (e.g. `test.spec.ts` vs `test.spec.tsx`) don't collide.
pub fn cache_file_path(
    cache_dir: &Path,
    source_rel: &Path,
    suffix: &str,
) -> PathBuf {
    let mut path = cache_dir.join(source_rel);
    let new_name = match path.file_name() {
        Some(name) => format!("{}.{}", name.to_string_lossy(), suffix),
        None => format!(".{}", suffix),
    };
    path.set_file_name(new_name);
    path
}
