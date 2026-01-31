use ignore::WalkBuilder;
use std::path::{Path, PathBuf};

/// Walker for traversing files in a project, respecting .gitignore.
pub struct SourceWalker {
    root: PathBuf,
}

impl SourceWalker {
    pub fn new(root: &Path) -> Self {
        Self { root: root.to_path_buf() }
    }

    /// Walk the source tree, yielding paths to all files.
    /// Respects .gitignore but includes hidden files (except .git directory).
    pub fn walk(&self) -> impl Iterator<Item = PathBuf> {
        WalkBuilder::new(&self.root)
            .hidden(false) // Include hidden files (e.g., .gitignore, .cargo/config.toml)
            .git_ignore(true) // Respect .gitignore
            .git_global(true) // Respect global gitignore
            .git_exclude(true) // Respect .git/info/exclude
            .filter_entry(|entry| {
                // Exclude .git directory
                entry.file_name() != ".git"
            })
            .build()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry.file_type().map(|ft| ft.is_file()).unwrap_or(false)
            })
            .map(|entry| entry.into_path())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_walk_finds_all_files() {
        let temp = TempDir::new().unwrap();
        let src_dir = temp.path().join("src");
        fs::create_dir_all(&src_dir).unwrap();

        fs::write(src_dir.join("main.rs"), "fn main() {}").unwrap();
        fs::write(src_dir.join("lib.rs"), "pub mod foo;").unwrap();
        fs::write(src_dir.join("readme.txt"), "not code").unwrap();

        let walker = SourceWalker::new(temp.path());
        let files: Vec<_> = walker.walk().collect();

        assert_eq!(files.len(), 3);
        assert!(files.iter().any(|p| p.ends_with("main.rs")));
        assert!(files.iter().any(|p| p.ends_with("lib.rs")));
        assert!(files.iter().any(|p| p.ends_with("readme.txt")));
    }

    #[test]
    fn test_walk_respects_gitignore() {
        use std::process::Command;

        let temp = TempDir::new().unwrap();
        let src_dir = temp.path().join("src");
        let target_dir = temp.path().join("target");
        fs::create_dir_all(&src_dir).unwrap();
        fs::create_dir_all(&target_dir).unwrap();

        // Initialize git repo so .gitignore is recognized
        Command::new("git")
            .args(["init"])
            .current_dir(temp.path())
            .output()
            .expect("git init failed");

        fs::write(temp.path().join(".gitignore"), "target/\n").unwrap();
        fs::write(src_dir.join("main.rs"), "fn main() {}").unwrap();
        fs::write(target_dir.join("debug.rs"), "fn debug() {}").unwrap();

        let walker = SourceWalker::new(temp.path());
        let files: Vec<_> = walker.walk().collect();

        // main.rs and .gitignore should be found, target/ is ignored
        assert_eq!(files.len(), 2);
        assert!(files.iter().any(|p| p.ends_with("main.rs")));
        assert!(files.iter().any(|p| p.ends_with(".gitignore")));
        assert!(!files.iter().any(|p| p.to_string_lossy().contains("target")));
    }
}
