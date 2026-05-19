//! Code coverage metrics derived from LCOV files.
//!
//! Coverage is attributed to Units via the innermost-containing-Span rule:
//! for each `DA:` (line) or `BRDA:` (branch) record, find the deepest Unit
//! whose span contains the line, and attribute the hit there. A line inside a
//! closure that's inside a function attributes to the closure, not the
//! function.
//!
//! Two metrics are exposed:
//! - `line_cov`: per-Unit % of attributed DA lines with `hits > 0`
//! - `uncov_branches`: per-Unit count of attributed BRDA records with `taken == 0`

use crate::complexity::{DistributionMetrics, SortDirection};
use mdlr_core::{Graph, Unit, UnitKind};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// A single DA record: line hits.
#[derive(Debug, Clone, Copy)]
pub struct LineHit {
    pub line: usize,
    pub hits: u64,
}

/// A single BRDA record: branch taken count.
#[derive(Debug, Clone, Copy)]
pub struct BranchHit {
    pub line: usize,
    pub taken: u64,
}

/// Parsed LCOV data, keyed by source file path (as the `SF:` record states).
#[derive(Debug, Default)]
pub struct LcovData {
    pub files: HashMap<PathBuf, FileCoverage>,
    /// True when at least one `BRDA:` record appeared in any input file.
    pub has_branches: bool,
}

#[derive(Debug, Default)]
pub struct FileCoverage {
    pub lines: Vec<LineHit>,
    pub branches: Vec<BranchHit>,
}

impl LcovData {
    pub fn new() -> Self {
        Self::default()
    }

    /// Parse an LCOV file and merge its records into this dataset.
    /// Hits for the same (file, line) are summed; branch records are appended.
    pub fn parse_and_merge(
        &mut self,
        path: &Path,
        repo_root: &Path,
    ) -> anyhow::Result<()> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| anyhow::anyhow!("reading {}: {e}", path.display()))?;
        self.parse_str(&content, repo_root);
        Ok(())
    }

    /// Parse LCOV text and merge into this dataset.
    /// Paths are normalized: absolute paths are kept, relative paths are
    /// resolved against `repo_root`. Both are canonicalized when possible so
    /// they match the Unit `file` paths later.
    pub fn parse_str(&mut self, content: &str, repo_root: &Path) {
        let mut current: Option<PathBuf> = None;
        for line in content.lines() {
            let line = line.trim();
            if line == "end_of_record" {
                current = None;
                continue;
            }
            let Some((tag, rest)) = line.split_once(':') else {
                continue;
            };
            match tag {
                "SF" => {
                    let p = Path::new(rest);
                    let abs = if p.is_absolute() {
                        p.to_path_buf()
                    } else {
                        repo_root.join(p)
                    };
                    let canonical = abs.canonicalize().unwrap_or(abs);
                    current = Some(canonical);
                }
                "DA" => {
                    let Some(file) = current.as_ref() else { continue };
                    let mut it = rest.split(',');
                    let (Some(line_s), Some(hits_s)) = (it.next(), it.next())
                    else {
                        continue;
                    };
                    let (Ok(line_no), Ok(hits)) =
                        (line_s.parse::<usize>(), hits_s.parse::<u64>())
                    else {
                        continue;
                    };
                    let fc = self.files.entry(file.clone()).or_default();
                    if let Some(existing) =
                        fc.lines.iter_mut().find(|l| l.line == line_no)
                    {
                        existing.hits = existing.hits.saturating_add(hits);
                    } else {
                        fc.lines.push(LineHit { line: line_no, hits });
                    }
                }
                "BRDA" => {
                    let Some(file) = current.as_ref() else { continue };
                    let mut it = rest.split(',');
                    let (Some(line_s), _, _, Some(taken_s)) =
                        (it.next(), it.next(), it.next(), it.next())
                    else {
                        continue;
                    };
                    let Ok(line_no) = line_s.parse::<usize>() else {
                        continue;
                    };
                    // BRDA taken is `-` if not instrumented; treat as 0.
                    let taken = if taken_s == "-" {
                        0
                    } else {
                        taken_s.parse().unwrap_or(0)
                    };
                    let fc = self.files.entry(file.clone()).or_default();
                    fc.branches.push(BranchHit { line: line_no, taken });
                    self.has_branches = true;
                }
                _ => {}
            }
        }
    }
}

/// Per-Unit coverage results.
#[derive(Debug, Clone)]
pub struct CoverageMetrics {
    /// Per-Unit line coverage percentage (0..=100), ascending sort.
    pub line_cov: DistributionMetrics,
    /// Per-Unit uncovered branch counts, descending sort.
    /// Only populated when LCOV contains BRDA records.
    pub uncov_branches: DistributionMetrics,
    /// True iff the LCOV input had any BRDA records.
    pub has_branches: bool,
    /// Count of analyzed Units that had zero DA records attributed.
    /// Used to decide whether to emit the "stale lcov?" warning.
    pub units_without_data: usize,
    /// Total Units analyzed for coverage.
    pub units_analyzed: usize,
    /// Number of distinct files in the parsed LCOV (post canonicalization).
    pub lcov_files_total: usize,
    /// Of the in-scope Unit files, how many had at least one matching
    /// `SF:` record in the LCOV. When this is 0 but `lcov_files_total > 0`,
    /// the LCOV exists but speaks a different path language than the graph —
    /// typically a sourcemap issue (e.g. LCOV references built `.js` while
    /// the graph holds `.ts`).
    pub lcov_files_matched: usize,
}

impl CoverageMetrics {
    /// Compute per-Unit coverage from parsed LCOV data and a graph.
    ///
    /// `scope_files`, if `Some`, restricts the metric to Units whose file
    /// (canonicalized) is in the set. Pass `None` to analyze every Unit in
    /// the graph.
    pub fn compute(
        graph: &Graph,
        lcov: &LcovData,
        repo_root: &Path,
        scope_files: Option<&std::collections::HashSet<PathBuf>>,
    ) -> Self {
        // Resolve in-scope function/method units to absolute, canonical paths.
        let mut unit_refs: Vec<(&Unit, PathBuf)> = Vec::new();
        for u in &graph.units {
            if u.kind != UnitKind::Function && u.kind != UnitKind::Method {
                continue;
            }
            let abs = if u.file.is_absolute() {
                u.file.clone()
            } else {
                repo_root.join(&u.file)
            };
            let canonical = abs.canonicalize().unwrap_or(abs);
            if let Some(scope) = scope_files {
                let scope_abs = scope.iter().any(|s| {
                    let s_abs = if s.is_absolute() {
                        s.clone()
                    } else {
                        repo_root.join(s)
                    };
                    let s_canonical = s_abs.canonicalize().unwrap_or(s_abs);
                    s_canonical == canonical
                });
                if !scope_abs {
                    continue;
                }
            }
            unit_refs.push((u, canonical));
        }

        // Group units by canonical file path so attribution is per-file work.
        let mut by_file: HashMap<&PathBuf, Vec<&(&Unit, PathBuf)>> =
            HashMap::new();
        for tup in &unit_refs {
            by_file.entry(&tup.1).or_default().push(tup);
        }

        let mut line_pct: HashMap<String, usize> = HashMap::new();
        let mut uncov: HashMap<String, usize> = HashMap::new();
        let mut units_without_data = 0;
        let units_analyzed = unit_refs.len();
        let mut lcov_files_matched = 0usize;

        for (file_path, units_in_file) in &by_file {
            let fc = lcov.files.get(file_path.as_path());
            if fc.is_some() {
                lcov_files_matched += 1;
            }

            // Bucket DA/BRDA records by innermost-containing Unit. Pre-sort
            // units in this file by span ascending start, descending end so
            // we can pick the deepest match quickly. Simpler: iterate every
            // unit and pick min-span containing the line.
            for (unit, _) in units_in_file {
                let (start, end) = (unit.span.start_line, unit.span.end_line);
                let mut total = 0usize;
                let mut hit = 0usize;
                let mut uncovered = 0usize;
                if let Some(fc) = fc {
                    for da in &fc.lines {
                        if da.line < start || da.line > end {
                            continue;
                        }
                        if !is_innermost(units_in_file, unit, da.line) {
                            continue;
                        }
                        total += 1;
                        if da.hits > 0 {
                            hit += 1;
                        }
                    }
                    for br in &fc.branches {
                        if br.line < start || br.line > end {
                            continue;
                        }
                        if !is_innermost(units_in_file, unit, br.line) {
                            continue;
                        }
                        if br.taken == 0 {
                            uncovered += 1;
                        }
                    }
                }
                if total == 0 {
                    units_without_data += 1;
                    line_pct.insert(unit.id.clone(), 0);
                } else {
                    let pct = (hit * 100) / total;
                    line_pct.insert(unit.id.clone(), pct);
                }
                if lcov.has_branches {
                    uncov.insert(unit.id.clone(), uncovered);
                }
            }
        }

        // Units in the graph whose file had no SF record at all already get
        // 0% via the `total == 0` branch above. Good.

        let line_cov = DistributionMetrics::from_counts_with_direction(
            line_pct,
            SortDirection::Asc,
        );
        let uncov_branches = DistributionMetrics::from_counts_with_direction(
            uncov,
            SortDirection::Desc,
        );

        Self {
            line_cov,
            uncov_branches,
            has_branches: lcov.has_branches,
            units_without_data,
            units_analyzed,
            lcov_files_total: lcov.files.len(),
            lcov_files_matched,
        }
    }
}

/// True if `candidate`'s span is the smallest among `units_in_file` that
/// contains `line`. Ties broken by unit id for determinism.
fn is_innermost(
    units_in_file: &[&(&Unit, PathBuf)],
    candidate: &Unit,
    line: usize,
) -> bool {
    let cand_span = candidate.span.end_line - candidate.span.start_line;
    let mut best: Option<&Unit> = None;
    let mut best_span: usize = usize::MAX;
    for (u, _) in units_in_file {
        if line < u.span.start_line || line > u.span.end_line {
            continue;
        }
        let s = u.span.end_line - u.span.start_line;
        let is_better = s < best_span
            || (s == best_span
                && best.is_none_or(|b| u.id.as_str() < b.id.as_str()));
        if is_better {
            best = Some(u);
            best_span = s;
        }
    }
    match best {
        Some(b) => b.id == candidate.id && best_span == cand_span,
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdlr_core::{Span, Unit, UnitKind};

    fn unit(id: &str, file: &str, start: usize, end: usize) -> Unit {
        Unit {
            id: id.to_string(),
            kind: UnitKind::Function,
            file: PathBuf::from(file),
            span: Span {
                start_line: start,
                start_col: 0,
                end_line: end,
                end_col: 0,
            },
            reads: vec![],
            writes: vec![],
            calls: vec![],
            tags: vec![],
            params: 0,
            branches: 0,
            max_scope_lines: 0,
            parent: None,
            cognitive_complexity: 0,
            partial: false,
        }
    }

    #[test]
    fn parses_da_and_brda_records() {
        let lcov_text = "\
SF:src/foo.rs
DA:1,1
DA:2,0
DA:3,5
BRDA:2,0,0,1
BRDA:2,0,1,0
end_of_record
";
        let mut data = LcovData::new();
        data.parse_str(lcov_text, Path::new("/tmp/repo"));
        assert!(data.has_branches);
        // Path may or may not canonicalize depending on whether /tmp/repo/src/foo.rs exists;
        // just find the single entry.
        let (_, fc) = data.files.iter().next().expect("one file");
        assert_eq!(fc.lines.len(), 3);
        assert_eq!(fc.branches.len(), 2);
    }

    /// Builds an LcovData from text via `parse_str` so the path normalization
    /// matches what `compute()` will do at lookup time.
    fn lcov_from(text: &str, repo_root: &Path) -> LcovData {
        let mut data = LcovData::new();
        data.parse_str(text, repo_root);
        data
    }

    #[test]
    fn line_coverage_is_ratio_of_attributed_da_lines() {
        let root = Path::new("/tmp/mdlr-cov-test");
        let mut graph = Graph::new();
        graph.add_unit(unit("foo", "src/foo.rs", 1, 10));
        let lcov = lcov_from(
            "SF:src/foo.rs\nDA:2,1\nDA:4,0\nDA:6,3\nDA:8,0\nend_of_record\n",
            root,
        );

        let cov = CoverageMetrics::compute(&graph, &lcov, root, None);
        // 2 of 4 lines hit = 50%
        let (id, val) = cov.line_cov.distribution.first().unwrap();
        assert_eq!(id, "foo");
        assert_eq!(*val, 50);
    }

    #[test]
    fn nested_unit_steals_lines_from_outer() {
        // Outer function spans 1..50, inner closure spans 20..30.
        // Lines inside [20, 30] should attribute to the inner unit.
        let root = Path::new("/tmp/mdlr-cov-test");
        let mut graph = Graph::new();
        graph.add_unit(unit("outer", "src/foo.rs", 1, 50));
        graph.add_unit(unit("inner", "src/foo.rs", 20, 30));
        let lcov = lcov_from(
            "SF:src/foo.rs\nDA:5,1\nDA:10,1\nDA:22,1\nDA:25,1\nend_of_record\n",
            root,
        );
        let cov = CoverageMetrics::compute(&graph, &lcov, root, None);
        let pct = |id: &str| -> usize {
            cov.line_cov
                .distribution
                .iter()
                .find(|(n, _)| n == id)
                .map(|(_, v)| *v)
                .unwrap()
        };
        assert_eq!(pct("outer"), 100);
        assert_eq!(pct("inner"), 100);
        // outer's total was only 2 (lines 5 and 10). inner's total was 2.
    }

    #[test]
    fn missing_file_records_yield_zero_pct_and_warning_count() {
        let root = Path::new("/tmp/mdlr-cov-test");
        let mut graph = Graph::new();
        graph.add_unit(unit("foo", "src/foo.rs", 1, 10));
        let lcov = LcovData::default();
        let cov = CoverageMetrics::compute(&graph, &lcov, root, None);
        assert_eq!(cov.units_analyzed, 1);
        assert_eq!(cov.units_without_data, 1);
        let (_, val) = cov.line_cov.distribution.first().unwrap();
        assert_eq!(*val, 0);
    }

    #[test]
    fn lcov_with_no_matching_files_signals_path_mismatch() {
        // Graph holds `src/foo.ts` (the kind of path mdlr's TS extractor
        // produces). LCOV references `dist/foo.js` (the kind of path you'd
        // get from running nyc against pre-built output without sourcemaps).
        // Both files have records, but their paths never overlap.
        let root = Path::new("/tmp/mdlr-cov-test");
        let mut graph = Graph::new();
        graph.add_unit(unit("foo", "src/foo.ts", 1, 10));
        let lcov =
            lcov_from("SF:dist/foo.js\nDA:2,1\nDA:3,1\nend_of_record\n", root);
        let cov = CoverageMetrics::compute(&graph, &lcov, root, None);
        assert_eq!(cov.lcov_files_total, 1);
        assert_eq!(cov.lcov_files_matched, 0);
        assert_eq!(cov.units_analyzed, 1);
        assert_eq!(cov.units_without_data, 1);
    }

    #[test]
    fn lcov_files_matched_counts_overlap_with_graph() {
        let root = Path::new("/tmp/mdlr-cov-test");
        let mut graph = Graph::new();
        graph.add_unit(unit("a", "src/a.rs", 1, 10));
        graph.add_unit(unit("b", "src/b.rs", 1, 10));
        // LCOV has records for `a` (in-graph) and `c` (not in graph).
        let lcov = lcov_from(
            "SF:src/a.rs\nDA:2,1\nend_of_record\n\
             SF:src/c.rs\nDA:2,1\nend_of_record\n",
            root,
        );
        let cov = CoverageMetrics::compute(&graph, &lcov, root, None);
        assert_eq!(cov.lcov_files_total, 2);
        // Only `src/a.rs` overlaps with graph Units.
        assert_eq!(cov.lcov_files_matched, 1);
    }

    #[test]
    fn uncov_branches_counted_when_brda_present() {
        let root = Path::new("/tmp/mdlr-cov-test");
        let mut graph = Graph::new();
        graph.add_unit(unit("foo", "src/foo.rs", 1, 10));
        let lcov = lcov_from(
            "SF:src/foo.rs\n\
             DA:2,1\n\
             BRDA:2,0,0,1\n\
             BRDA:2,0,1,0\n\
             BRDA:5,0,0,0\n\
             end_of_record\n",
            root,
        );
        let cov = CoverageMetrics::compute(&graph, &lcov, root, None);
        let (_, val) = cov.uncov_branches.distribution.first().unwrap();
        assert_eq!(*val, 2);
    }

    #[test]
    fn p90_marks_worst_10_percent_boundary_for_asc() {
        // 10 functions: nine at 100%, one at 0%. The worst-10% boundary is 0%.
        let root = Path::new("/tmp/mdlr-cov-test");
        let mut graph = Graph::new();
        for i in 0..10 {
            graph.add_unit(unit(
                &format!("f{i}"),
                &format!("src/f{i}.rs"),
                1,
                10,
            ));
        }
        let mut lcov_text = String::new();
        for i in 0..10 {
            let hits = if i == 0 { 0 } else { 1 };
            lcov_text.push_str(&format!(
                "SF:src/f{i}.rs\nDA:2,{hits}\nend_of_record\n"
            ));
        }
        let lcov = lcov_from(&lcov_text, root);
        let cov = CoverageMetrics::compute(&graph, &lcov, root, None);
        // For an Asc distribution, p90 is the value at the 10th percentile
        // (the boundary marking the worst 10% — i.e. the 0% function here).
        assert_eq!(cov.line_cov.p90, 0);
    }

    #[test]
    fn line_cov_sorts_ascending_worst_first() {
        let root = Path::new("/tmp/mdlr-cov-test");
        let mut graph = Graph::new();
        graph.add_unit(unit("a", "src/a.rs", 1, 10));
        graph.add_unit(unit("b", "src/b.rs", 1, 10));
        let lcov = lcov_from(
            "SF:src/a.rs\nDA:2,1\nDA:3,0\nend_of_record\n\
             SF:src/b.rs\nDA:2,1\nDA:3,1\nend_of_record\n",
            root,
        );
        let cov = CoverageMetrics::compute(&graph, &lcov, root, None);
        assert_eq!(cov.line_cov.sort_direction, SortDirection::Asc);
        assert_eq!(cov.line_cov.distribution[0].0, "a");
        assert_eq!(cov.line_cov.distribution[0].1, 50);
        assert_eq!(cov.line_cov.distribution[1].0, "b");
    }
}
