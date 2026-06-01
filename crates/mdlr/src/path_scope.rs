//! Shared path-based scoping for filtering source files by a CLI path argument.
//!
//! Both `check` and `ls` accept a path that should restrict which source files
//! are considered. This module owns the classification (file vs. directory) and
//! the match logic; callers decide what a non-existent path means.

use std::path::{Path, PathBuf};

/// A resolved, canonicalized path filter: a single file or a directory subtree.
pub enum PathScope {
    /// Match exactly one file.
    File(PathBuf),
    /// Match any file under this directory.
    Directory(PathBuf),
}

impl PathScope {
    /// Resolve `path` (relative to `cwd`) into a scope.
    ///
    /// Returns `None` if the path does not exist — callers decide what that
    /// means (e.g. `ls` errors, `check` falls back to a symbol-ID filter).
    pub fn classify(path: &Path, cwd: &Path) -> Option<PathScope> {
        let resolved = if path.is_absolute() {
            path.to_path_buf()
        } else {
            cwd.join(path)
        };

        if !resolved.exists() {
            return None;
        }

        let canonical = resolved.canonicalize().unwrap_or(resolved);
        if canonical.is_file() {
            Some(PathScope::File(canonical))
        } else {
            Some(PathScope::Directory(canonical))
        }
    }

    /// Does `file_path` (an absolute path yielded by the source walker) fall
    /// within this scope?
    pub fn matches(&self, file_path: &Path) -> bool {
        match self {
            PathScope::File(p) => file_path == p,
            PathScope::Directory(p) => file_path.starts_with(p),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn classify_directory() {
        let temp = TempDir::new().unwrap();
        let sub = temp.path().join("src");
        fs::create_dir_all(&sub).unwrap();

        let scope =
            PathScope::classify(Path::new("src"), temp.path()).unwrap();
        let canonical_sub = sub.canonicalize().unwrap();
        assert!(
            matches!(scope, PathScope::Directory(ref p) if *p == canonical_sub)
        );
        assert!(scope.matches(&canonical_sub.join("main.rs")));
        assert!(!scope.matches(&temp.path().join("other.rs")));
    }

    #[test]
    fn classify_file() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("main.rs");
        fs::write(&file, "fn main() {}").unwrap();

        let scope =
            PathScope::classify(Path::new("main.rs"), temp.path()).unwrap();
        let canonical_file = file.canonicalize().unwrap();
        assert!(
            matches!(scope, PathScope::File(ref p) if *p == canonical_file)
        );
        assert!(scope.matches(&canonical_file));
        assert!(!scope.matches(&temp.path().join("other.rs")));
    }

    #[test]
    fn classify_nonexistent_is_none() {
        let temp = TempDir::new().unwrap();
        assert!(
            PathScope::classify(Path::new("missing"), temp.path()).is_none()
        );
    }

    #[test]
    fn classify_absolute_path() {
        let temp = TempDir::new().unwrap();
        let file = temp.path().join("main.rs");
        fs::write(&file, "fn main() {}").unwrap();

        // cwd is irrelevant for an absolute path.
        let scope =
            PathScope::classify(&file, Path::new("/nonexistent")).unwrap();
        assert!(matches!(scope, PathScope::File(_)));
    }
}
