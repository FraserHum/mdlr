use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::matching::ClonePair;
use crate::tokens::FileTokens;

/// A unit's location, used to attribute duplicated lines. The caller supplies
/// these from the extraction graph (functions, methods, structs, impls —
/// not whole-file modules, which would swallow orphan lines).
#[derive(Debug, Clone)]
pub struct UnitSpan {
    pub id: String,
    /// Relative path, same form as `FileTokens.source_path`.
    pub file: PathBuf,
    /// 1-based inclusive line span.
    pub start_line: u32,
    pub end_line: u32,
}

impl UnitSpan {
    /// One unit spanning an entire tokenized file (line 1 through the last
    /// token's line), id `<path>::all`. For callers without extraction data —
    /// primarily tests.
    pub fn whole_file(tokens: &FileTokens) -> Self {
        UnitSpan {
            id: format!("{}::all", tokens.source_path.display()),
            file: tokens.source_path.clone(),
            start_line: 1,
            end_line: tokens.tokens.last().map(|t| t.line).unwrap_or(1),
        }
    }
}

/// Aggregate duplication metrics across the project.
#[derive(Debug, Clone, Default)]
pub struct DuplicationMetrics {
    /// Per-unit duplication percentage distribution, sorted by percentage
    /// descending. Only units with any duplicated lines appear.
    pub distribution: Vec<(String, usize)>,
    /// Maximum duplication percentage across all units.
    pub max: f64,
    /// Mean duplication percentage across units with any duplication.
    pub mean: f64,
    /// 90th percentile duplication percentage across all units.
    pub p90: f64,
    /// Total number of clone pairs found.
    pub clone_count: usize,
}

/// Compute per-unit duplication metrics from clone pairs.
///
/// Each duplicated line is attributed to the innermost unit (smallest span)
/// containing it; lines outside every unit's span (duplicated imports, file
/// headers) are dropped. A unit's percentage is its attributed duplicated
/// lines over its span length.
pub fn compute_duplication(
    clones: &[ClonePair],
    units: &[UnitSpan],
) -> DuplicationMetrics {
    if units.is_empty() {
        return DuplicationMetrics {
            clone_count: clones.len(),
            ..Default::default()
        };
    }

    // Group units by file for attribution.
    let mut units_by_file: HashMap<&PathBuf, Vec<&UnitSpan>> = HashMap::new();
    for unit in units {
        units_by_file.entry(&unit.file).or_default().push(unit);
    }

    // Attribute duplicated lines (deduplicated) to innermost units.
    let mut dup_lines: HashMap<&str, HashSet<u32>> = HashMap::new();
    let mut attribute = |file: &PathBuf, start: u32, end: u32| {
        let Some(units_in_file) = units_by_file.get(file) else { return };
        for line in start..=end {
            if let Some(unit) = innermost(units_in_file, line) {
                dup_lines.entry(&unit.id).or_default().insert(line);
            }
        }
    };
    for clone in clones {
        attribute(&clone.file_a, clone.start_line_a, clone.end_line_a);
        attribute(&clone.file_b, clone.start_line_b, clone.end_line_b);
    }

    // Per-unit percentage: attributed duplicated lines / span length.
    let mut percentages: Vec<(String, f64)> = units
        .iter()
        .map(|u| {
            let span_len = (u.end_line - u.start_line + 1) as f64;
            let dup = dup_lines.get(u.id.as_str()).map_or(0, |s| s.len());
            (u.id.clone(), (dup as f64 / span_len) * 100.0)
        })
        .collect();
    percentages.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });

    let max = percentages.first().map(|(_, p)| *p).unwrap_or(0.0);

    let with_dup: Vec<f64> = percentages
        .iter()
        .filter(|(_, p)| *p > 0.0)
        .map(|(_, p)| *p)
        .collect();
    let mean = if with_dup.is_empty() {
        0.0
    } else {
        with_dup.iter().sum::<f64>() / with_dup.len() as f64
    };

    // p90: 90th percentile across all units (not just those with duplication).
    let mut sorted: Vec<f64> = percentages.iter().map(|(_, p)| *p).collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let idx = ((sorted.len() as f64) * 0.9).ceil() as usize;
    let p90 = sorted[idx.min(sorted.len()) - 1];

    let distribution: Vec<(String, usize)> = percentages
        .into_iter()
        .filter(|(_, p)| *p > 0.0)
        .map(|(id, p)| (id, p.round() as usize))
        .collect();

    DuplicationMetrics {
        distribution,
        max,
        mean,
        p90,
        clone_count: clones.len(),
    }
}

/// The unit with the smallest span containing `line`, ties broken by id.
fn innermost<'a>(
    units_in_file: &[&'a UnitSpan],
    line: u32,
) -> Option<&'a UnitSpan> {
    units_in_file
        .iter()
        .filter(|u| u.start_line <= line && line <= u.end_line)
        .min_by(|a, b| {
            (a.end_line - a.start_line)
                .cmp(&(b.end_line - b.start_line))
                .then_with(|| a.id.cmp(&b.id))
        })
        .copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit(id: &str, file: &str, start: u32, end: u32) -> UnitSpan {
        UnitSpan {
            id: id.to_string(),
            file: PathBuf::from(file),
            start_line: start,
            end_line: end,
        }
    }

    fn clone_pair(
        file_a: &str,
        a: (u32, u32),
        file_b: &str,
        b: (u32, u32),
    ) -> ClonePair {
        ClonePair {
            file_a: PathBuf::from(file_a),
            start_line_a: a.0,
            end_line_a: a.1,
            file_b: PathBuf::from(file_b),
            start_line_b: b.0,
            end_line_b: b.1,
            token_count: 50,
        }
    }

    #[test]
    fn test_no_clones() {
        let units = vec![unit("a.rs::f", "a.rs", 1, 50)];
        let metrics = compute_duplication(&[], &units);
        assert_eq!(metrics.clone_count, 0);
        assert_eq!(metrics.max, 0.0);
        assert!(metrics.distribution.is_empty());
    }

    #[test]
    fn test_fully_duplicated_unit() {
        let units = vec![
            unit("a.rs::f", "a.rs", 1, 20),
            unit("b.rs::g", "b.rs", 1, 20),
        ];
        let clones = vec![clone_pair("a.rs", (1, 20), "b.rs", (1, 20))];

        let metrics = compute_duplication(&clones, &units);
        assert_eq!(metrics.clone_count, 1);
        assert!((metrics.max - 100.0).abs() < 0.01);
        assert_eq!(metrics.distribution.len(), 2);
        assert_eq!(metrics.distribution[0].1, 100);
    }

    #[test]
    fn test_partial_unit_duplication() {
        // Clone covers lines 1-25 of a unit spanning 1-100: 25%.
        let units = vec![
            unit("a.rs::f", "a.rs", 1, 100),
            unit("b.rs::g", "b.rs", 1, 100),
        ];
        let clones = vec![clone_pair("a.rs", (1, 25), "b.rs", (1, 25))];

        let metrics = compute_duplication(&clones, &units);
        assert!((metrics.max - 25.0).abs() < 0.01);
    }

    #[test]
    fn test_deduplicated_line_counting() {
        // Same lines in multiple clone pairs only count once.
        let units = vec![
            unit("a.rs::f", "a.rs", 1, 100),
            unit("b.rs::g", "b.rs", 1, 100),
            unit("c.rs::h", "c.rs", 1, 100),
        ];
        let clones = vec![
            clone_pair("a.rs", (1, 20), "b.rs", (1, 20)),
            clone_pair("a.rs", (1, 20), "c.rs", (1, 20)),
        ];

        let metrics = compute_duplication(&clones, &units);
        let a = metrics
            .distribution
            .iter()
            .find(|(id, _)| id == "a.rs::f")
            .unwrap();
        assert_eq!(a.1, 20);
    }

    #[test]
    fn test_innermost_attribution() {
        // Clone lines inside the method attribute to the method, not the
        // enclosing impl.
        let units = vec![
            unit("a.rs::impl Foo", "a.rs", 1, 100),
            unit("a.rs::impl Foo::bar", "a.rs", 10, 29),
            unit("b.rs::g", "b.rs", 1, 20),
        ];
        let clones = vec![clone_pair("a.rs", (10, 29), "b.rs", (1, 20))];

        let metrics = compute_duplication(&clones, &units);
        let method = metrics
            .distribution
            .iter()
            .find(|(id, _)| id == "a.rs::impl Foo::bar")
            .unwrap();
        assert_eq!(method.1, 100);
        assert!(
            !metrics.distribution.iter().any(|(id, _)| id == "a.rs::impl Foo"),
            "impl got no attributed lines, so it should not appear"
        );
    }

    #[test]
    fn test_orphan_clone_lines_dropped() {
        // Clone covers lines 1-10 but the only unit starts at line 20:
        // duplicated import block produces no rows.
        let units = vec![
            unit("a.rs::f", "a.rs", 20, 40),
            unit("b.rs::g", "b.rs", 20, 40),
        ];
        let clones = vec![clone_pair("a.rs", (1, 10), "b.rs", (1, 10))];

        let metrics = compute_duplication(&clones, &units);
        assert_eq!(metrics.clone_count, 1);
        assert!(metrics.distribution.is_empty());
    }

    #[test]
    fn test_clone_straddling_unit_boundary() {
        // Clone spans lines 15-30; unit covers 20-40. Only lines 20-30
        // attribute (11 of 21 span lines = 52%).
        let units = vec![
            unit("a.rs::f", "a.rs", 20, 40),
            unit("b.rs::g", "b.rs", 1, 30),
        ];
        let clones = vec![clone_pair("a.rs", (15, 30), "b.rs", (1, 16))];

        let metrics = compute_duplication(&clones, &units);
        let a = metrics
            .distribution
            .iter()
            .find(|(id, _)| id == "a.rs::f")
            .unwrap();
        assert_eq!(a.1, 52);
    }

    #[test]
    fn test_empty_units() {
        let clones = vec![clone_pair("a.rs", (1, 10), "b.rs", (1, 10))];
        let metrics = compute_duplication(&clones, &[]);
        assert_eq!(metrics.clone_count, 1);
        assert!(metrics.distribution.is_empty());
    }
}
