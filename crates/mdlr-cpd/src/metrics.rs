use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::matching::ClonePair;
use crate::tokens::FileTokens;

/// Per-file duplication info.
#[derive(Debug, Clone)]
pub struct FileDuplication {
    pub file: PathBuf,
    /// Total lines in the file.
    pub total_lines: u32,
    /// Number of deduplicated lines that are part of any clone.
    pub duplicated_lines: u32,
    /// Duplication percentage (0.0 - 100.0).
    pub percentage: f64,
}

/// Aggregate duplication metrics across the project.
#[derive(Debug, Clone, Default)]
pub struct DuplicationMetrics {
    /// Per-file duplication percentage distribution, sorted by percentage descending.
    pub distribution: Vec<(String, usize)>,
    /// Maximum duplication percentage across all files.
    pub max: f64,
    /// Mean duplication percentage across all files with any duplication.
    pub mean: f64,
    /// 90th percentile duplication percentage.
    pub p90: f64,
    /// Total number of clone pairs found.
    pub clone_count: usize,
    /// Per-file details.
    pub files: Vec<FileDuplication>,
}

/// Compute duplication metrics from clone pairs and file token data.
///
/// `scope_files` optionally limits which files are reported in the output.
/// Clone pairs are reported if *either* side is in scope.
pub fn compute_duplication(
    clones: &[ClonePair],
    files: &[FileTokens],
    scope_files: Option<&HashSet<PathBuf>>,
) -> DuplicationMetrics {
    if files.is_empty() {
        return DuplicationMetrics::default();
    }

    // Build file -> max line map
    let mut file_max_line: HashMap<&Path, u32> = HashMap::new();
    for file in files {
        let max_line = file.tokens.last().map(|t| t.line).unwrap_or(0);
        file_max_line.insert(&file.source_path, max_line);
    }

    // Filter clones by scope: report if either side is in scope
    let in_scope_clones: Vec<&ClonePair> = if let Some(scope) = scope_files {
        clones
            .iter()
            .filter(|c| scope.contains(&c.file_a) || scope.contains(&c.file_b))
            .collect()
    } else {
        clones.iter().collect()
    };

    // Collect duplicated lines per file (deduplicated)
    let mut duplicated_lines_per_file: HashMap<&Path, HashSet<u32>> =
        HashMap::new();

    for clone in &in_scope_clones {
        // Add lines from side A (if in scope or no scope filter)
        let a_in_scope = scope_files.is_none()
            || scope_files.unwrap().contains(&clone.file_a);
        if a_in_scope {
            let lines =
                duplicated_lines_per_file.entry(&clone.file_a).or_default();
            for line in clone.start_line_a..=clone.end_line_a {
                lines.insert(line);
            }
        }

        // Add lines from side B (if in scope or no scope filter)
        let b_in_scope = scope_files.is_none()
            || scope_files.unwrap().contains(&clone.file_b);
        if b_in_scope {
            let lines =
                duplicated_lines_per_file.entry(&clone.file_b).or_default();
            for line in clone.start_line_b..=clone.end_line_b {
                lines.insert(line);
            }
        }
    }

    // Compute per-file metrics
    let mut file_duplication: Vec<FileDuplication> = Vec::new();

    // Determine which files to report
    let report_files: Vec<&Path> = if let Some(scope) = scope_files {
        files
            .iter()
            .filter(|f| scope.contains(&f.source_path))
            .map(|f| f.source_path.as_path())
            .collect()
    } else {
        files.iter().map(|f| f.source_path.as_path()).collect()
    };

    for file_path in &report_files {
        let total_lines = file_max_line.get(file_path).copied().unwrap_or(0);
        if total_lines == 0 {
            continue;
        }

        let duplicated_lines = duplicated_lines_per_file
            .get(file_path)
            .map(|s| s.len() as u32)
            .unwrap_or(0);

        let percentage =
            (duplicated_lines as f64 / total_lines as f64) * 100.0;

        file_duplication.push(FileDuplication {
            file: file_path.to_path_buf(),
            total_lines,
            duplicated_lines,
            percentage,
        });
    }

    // Sort by percentage descending
    file_duplication.sort_by(|a, b| {
        b.percentage
            .partial_cmp(&a.percentage)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Compute aggregates
    let max = file_duplication.first().map(|f| f.percentage).unwrap_or(0.0);

    let files_with_duplication: Vec<&FileDuplication> =
        file_duplication.iter().filter(|f| f.duplicated_lines > 0).collect();

    let mean = if files_with_duplication.is_empty() {
        0.0
    } else {
        let sum: f64 =
            files_with_duplication.iter().map(|f| f.percentage).sum();
        sum / files_with_duplication.len() as f64
    };

    // p90: 90th percentile of all files (not just those with duplication)
    let p90 = if file_duplication.is_empty() {
        0.0
    } else {
        let mut percentages: Vec<f64> =
            file_duplication.iter().map(|f| f.percentage).collect();
        percentages.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let idx = ((percentages.len() as f64) * 0.9).ceil() as usize;
        let idx = idx.min(percentages.len()) - 1;
        percentages[idx]
    };

    // Build distribution in the standard format: (file_path_string, duplication_pct_as_usize)
    let distribution: Vec<(String, usize)> = file_duplication
        .iter()
        .filter(|f| f.duplicated_lines > 0)
        .map(|f| {
            (
                f.file.to_string_lossy().to_string(),
                f.percentage.round() as usize,
            )
        })
        .collect();

    DuplicationMetrics {
        distribution,
        max,
        mean,
        p90,
        clone_count: in_scope_clones.len(),
        files: file_duplication,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::Token;

    fn make_file_tokens(
        path: &str,
        num_tokens: usize,
        max_line: u32,
    ) -> FileTokens {
        FileTokens {
            source_path: PathBuf::from(path),
            tokens: (0..num_tokens)
                .map(|i| Token {
                    value: "fn".to_string(),
                    line: ((i as u32) % max_line) + 1,
                    col: 0,
                })
                .collect(),
            cached_at: 1,
        }
    }

    #[test]
    fn test_no_clones() {
        let files = vec![make_file_tokens("a.rs", 100, 50)];
        let metrics = compute_duplication(&[], &files, None);
        assert_eq!(metrics.clone_count, 0);
        assert_eq!(metrics.max, 0.0);
        assert!(metrics.distribution.is_empty());
    }

    #[test]
    fn test_full_file_duplication() {
        let files = vec![
            make_file_tokens("a.rs", 100, 20),
            make_file_tokens("b.rs", 100, 20),
        ];
        let clones = vec![ClonePair {
            file_a: PathBuf::from("a.rs"),
            start_line_a: 1,
            end_line_a: 20,
            file_b: PathBuf::from("b.rs"),
            start_line_b: 1,
            end_line_b: 20,
            token_count: 100,
        }];

        let metrics = compute_duplication(&clones, &files, None);
        assert_eq!(metrics.clone_count, 1);
        // Both files should show 100% duplication
        assert!((metrics.max - 100.0).abs() < 0.01);
        assert_eq!(metrics.files.len(), 2);
    }

    #[test]
    fn test_partial_duplication() {
        let files = vec![
            make_file_tokens("a.rs", 100, 100),
            make_file_tokens("b.rs", 100, 100),
        ];
        let clones = vec![ClonePair {
            file_a: PathBuf::from("a.rs"),
            start_line_a: 1,
            end_line_a: 25,
            file_b: PathBuf::from("b.rs"),
            start_line_b: 1,
            end_line_b: 25,
            token_count: 50,
        }];

        let metrics = compute_duplication(&clones, &files, None);
        // 25 lines out of 100 = 25%
        assert!((metrics.max - 25.0).abs() < 0.01);
    }

    #[test]
    fn test_deduplicated_line_counting() {
        // Same lines involved in multiple clones should only count once
        let files = vec![
            make_file_tokens("a.rs", 200, 100),
            make_file_tokens("b.rs", 200, 100),
            make_file_tokens("c.rs", 200, 100),
        ];
        let clones = vec![
            ClonePair {
                file_a: PathBuf::from("a.rs"),
                start_line_a: 1,
                end_line_a: 20,
                file_b: PathBuf::from("b.rs"),
                start_line_b: 1,
                end_line_b: 20,
                token_count: 50,
            },
            ClonePair {
                file_a: PathBuf::from("a.rs"),
                start_line_a: 1,
                end_line_a: 20,
                file_b: PathBuf::from("c.rs"),
                start_line_b: 1,
                end_line_b: 20,
                token_count: 50,
            },
        ];

        let metrics = compute_duplication(&clones, &files, None);
        // File a has lines 1-20 duplicated (20 lines out of 100 = 20%)
        // even though it appears in two clone pairs
        let file_a = metrics
            .files
            .iter()
            .find(|f| f.file == PathBuf::from("a.rs"))
            .unwrap();
        assert_eq!(file_a.duplicated_lines, 20);
        assert!((file_a.percentage - 20.0).abs() < 0.01);
    }

    #[test]
    fn test_scope_filtering() {
        let files = vec![
            make_file_tokens("a.rs", 100, 100),
            make_file_tokens("b.rs", 100, 100),
        ];
        let clones = vec![ClonePair {
            file_a: PathBuf::from("a.rs"),
            start_line_a: 1,
            end_line_a: 50,
            file_b: PathBuf::from("b.rs"),
            start_line_b: 1,
            end_line_b: 50,
            token_count: 100,
        }];

        // Only a.rs in scope — clone should still be reported (either side in scope)
        let scope: HashSet<PathBuf> =
            [PathBuf::from("a.rs")].into_iter().collect();
        let metrics = compute_duplication(&clones, &files, Some(&scope));
        assert_eq!(metrics.clone_count, 1);
        // Only a.rs should appear in files
        assert_eq!(metrics.files.len(), 1);
        assert_eq!(metrics.files[0].file, PathBuf::from("a.rs"));
    }

    #[test]
    fn test_empty_files() {
        let metrics = compute_duplication(&[], &[], None);
        assert_eq!(metrics.clone_count, 0);
        assert!(metrics.files.is_empty());
    }
}
