use mdlr_core::{Graph, UnitKind};
use std::collections::HashMap;

/// Complexity metrics for functions and methods
#[derive(Debug, Clone)]
pub struct ComplexityMetrics {
    /// Function/method size in lines of code
    pub size: DistributionMetrics,
    /// Parameter counts
    pub params: ParamMetrics,
    /// Cyclomatic complexity (branches + 1)
    pub cyclomatic: DistributionMetrics,
    /// Cognitive complexity (nesting-aware, SonarSource formulation)
    pub cognitive: DistributionMetrics,
    /// Largest single scope block within each function
    pub max_scope: DistributionMetrics,
}

/// Sort direction for a distribution: which end of the value range is "worse".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SortDirection {
    /// Higher values are worse (e.g. cyclomatic complexity). Worst-first = descending.
    #[default]
    Desc,
    /// Lower values are worse (e.g. line coverage %). Worst-first = ascending.
    Asc,
}

/// A distribution of usize values with summary statistics.
///
/// `distribution` is sorted worst-first per `sort_direction`, so consumers can
/// take the first N entries to get the top-N worst symbols regardless of which
/// end of the range is "bad".
#[derive(Debug, Clone)]
pub struct DistributionMetrics {
    pub max: usize,
    pub mean: f64,
    pub p90: usize,
    /// Entries sorted worst-first per `sort_direction`
    pub distribution: Vec<(String, usize)>,
    /// Which end of the value range is "worse"
    pub sort_direction: SortDirection,
}

#[derive(Debug, Clone)]
pub struct ParamMetrics {
    pub max: usize,
    pub mean: f64,
    /// Functions/methods sorted by param count descending
    pub distribution: Vec<(String, usize)>,
}

impl ComplexityMetrics {
    #[tracing::instrument(name = "compute_complexity", skip_all)]
    pub fn compute(graph: &Graph) -> Self {
        Self::compute_with_progress(graph, |_| {})
    }

    pub fn compute_with_progress(
        graph: &Graph,
        on_progress: impl Fn(usize),
    ) -> Self {
        let mut sizes: HashMap<String, usize> = HashMap::new();
        let mut params: HashMap<String, usize> = HashMap::new();
        let mut cyclomatic: HashMap<String, usize> = HashMap::new();
        let mut cognitive: HashMap<String, usize> = HashMap::new();
        let mut max_scope: HashMap<String, usize> = HashMap::new();

        for (i, unit) in graph.units.iter().enumerate() {
            on_progress(i);
            // Only compute complexity for functions and methods
            if unit.kind != UnitKind::Function && unit.kind != UnitKind::Method
            {
                continue;
            }

            // Size from span
            let size =
                unit.span.end_line.saturating_sub(unit.span.start_line) + 1;
            sizes.insert(unit.id.clone(), size);

            // Parameter count (from unit.params if available)
            params.insert(unit.id.clone(), unit.params);

            // Cyclomatic complexity (from unit.branches if available)
            // Cyclomatic = branches + 1
            cyclomatic.insert(unit.id.clone(), unit.branches + 1);

            // Cognitive complexity
            cognitive.insert(unit.id.clone(), unit.cognitive_complexity);

            // Max scope lines
            max_scope.insert(unit.id.clone(), unit.max_scope_lines);
        }

        Self {
            size: DistributionMetrics::from_counts(sizes),
            params: ParamMetrics::from_counts(params),
            cyclomatic: DistributionMetrics::from_counts(cyclomatic),
            cognitive: DistributionMetrics::from_counts(cognitive),
            max_scope: DistributionMetrics::from_counts(max_scope),
        }
    }

    /// Check if there are any functions/methods to report on
    pub fn has_functions(&self) -> bool {
        !self.size.distribution.is_empty()
    }
}

impl DistributionMetrics {
    /// Build a distribution where higher values are worse (default).
    pub fn from_counts(counts: HashMap<String, usize>) -> Self {
        Self::from_counts_with_direction(counts, SortDirection::Desc)
    }

    /// Build a distribution with an explicit sort direction.
    pub fn from_counts_with_direction(
        counts: HashMap<String, usize>,
        sort_direction: SortDirection,
    ) -> Self {
        let mut distribution: Vec<_> = counts.into_iter().collect();
        match sort_direction {
            SortDirection::Desc => distribution
                .sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0))),
            SortDirection::Asc => distribution
                .sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.0.cmp(&b.0))),
        }

        let mut metrics =
            Self { max: 0, mean: 0.0, p90: 0, distribution, sort_direction };
        metrics.rebuild_aggregates();
        metrics
    }

    /// Drop every entry whose id is not in `keep`, then rebuild the
    /// aggregates so they describe the retained set. Used by diff-mode
    /// display scoping.
    pub fn retain_ids(&mut self, keep: &std::collections::HashSet<String>) {
        self.distribution.retain(|(id, _)| keep.contains(id));
        self.rebuild_aggregates();
    }

    /// Recompute max/mean/p90 from the current distribution.
    fn rebuild_aggregates(&mut self) {
        let values: Vec<usize> =
            self.distribution.iter().map(|(_, v)| *v).collect();
        self.max = values.iter().copied().max().unwrap_or(0);
        self.mean = if values.is_empty() {
            0.0
        } else {
            values.iter().sum::<usize>() as f64 / values.len() as f64
        };
        self.p90 = p90_boundary(values, self.sort_direction);
    }
}

/// The boundary cutting off the worst 10% of `values`, regardless of
/// direction: for `Desc` (higher = worse) that's the 90th percentile; for
/// `Asc` (lower = worse) it's the 10th. So the worst-10% threshold is always
/// reported on the metric's "worse" side of the distribution.
pub fn p90_boundary(
    mut values: Vec<usize>,
    direction: SortDirection,
) -> usize {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    let percentile = match direction {
        SortDirection::Desc => 0.9,
        SortDirection::Asc => 0.1,
    };
    let idx = (values.len() as f64 * percentile).ceil() as usize;
    values[idx.max(1).min(values.len()) - 1]
}

impl ParamMetrics {
    fn from_counts(counts: HashMap<String, usize>) -> Self {
        if counts.is_empty() {
            return Self { max: 0, mean: 0.0, distribution: vec![] };
        }

        let max = counts.values().copied().max().unwrap_or(0);
        let sum: usize = counts.values().sum();
        let mean = sum as f64 / counts.len() as f64;

        let mut distribution: Vec<_> = counts.into_iter().collect();
        distribution.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        Self { max, mean, distribution }
    }
}

/// Look up a value by name in a distribution, returning 0 if not found.
fn lookup(distribution: &[(String, usize)], name: &str) -> usize {
    distribution.iter().find(|(n, _)| n == name).map(|(_, v)| *v).unwrap_or(0)
}

impl std::fmt::Display for ComplexityMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Complexity Metrics")?;
        writeln!(f, "==================")?;
        writeln!(f)?;

        writeln!(
            f,
            "Function Size: max={} lines, mean={:.1}, p90={}",
            self.size.max, self.size.mean, self.size.p90
        )?;
        writeln!(
            f,
            "Parameters:    max={}, mean={:.1}",
            self.params.max, self.params.mean
        )?;
        writeln!(
            f,
            "Cyclomatic:    max={}, mean={:.1}, p90={}",
            self.cyclomatic.max, self.cyclomatic.mean, self.cyclomatic.p90
        )?;
        writeln!(
            f,
            "Cognitive:     max={}, mean={:.1}, p90={}",
            self.cognitive.max, self.cognitive.mean, self.cognitive.p90
        )?;
        writeln!(
            f,
            "Max Scope:     max={} lines, mean={:.1}, p90={}",
            self.max_scope.max, self.max_scope.mean, self.max_scope.p90
        )?;

        self.fmt_complex_functions(f)?;
        self.fmt_largest_functions(f)
    }
}

impl ComplexityMetrics {
    fn fmt_complex_functions(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        let complex: Vec<_> = self
            .cyclomatic
            .distribution
            .iter()
            .filter(|(_, c)| *c > 1)
            .take(10)
            .collect();
        if complex.is_empty() {
            return Ok(());
        }
        writeln!(f)?;
        writeln!(f, "Most Complex Functions:")?;
        for (name, complexity) in complex {
            writeln!(
                f,
                "  {} (cc={}, lines={}, params={})",
                name,
                complexity,
                lookup(&self.size.distribution, name),
                lookup(&self.params.distribution, name),
            )?;
        }
        Ok(())
    }

    fn fmt_largest_functions(
        &self,
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        let large: Vec<_> = self
            .size
            .distribution
            .iter()
            .filter(|(_, s)| *s > 20)
            .take(10)
            .collect();
        if large.is_empty() {
            return Ok(());
        }
        writeln!(f)?;
        writeln!(f, "Largest Functions:")?;
        for (name, size) in large {
            writeln!(f, "  {} ({} lines)", name, size)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdlr_core::{Span, Unit};
    use std::path::PathBuf;

    fn make_function(
        id: &str,
        start: usize,
        end: usize,
        params: usize,
        branches: usize,
    ) -> Unit {
        Unit {
            id: id.to_string(),
            kind: UnitKind::Function,
            file: PathBuf::from("test.rs"),
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
            params,
            branches,
            max_scope_lines: 0,
            parent: None,
            cognitive_complexity: 0,
            partial: false,
        }
    }

    #[test]
    fn test_size_metrics() {
        let mut graph = Graph::new();
        graph.add_unit(make_function("small", 1, 5, 0, 0)); // 5 lines
        graph.add_unit(make_function("medium", 10, 30, 2, 3)); // 21 lines
        graph.add_unit(make_function("large", 40, 100, 5, 10)); // 61 lines

        let metrics = ComplexityMetrics::compute(&graph);

        assert_eq!(metrics.size.max, 61);
        assert_eq!(metrics.size.distribution[0].0, "large");
        assert_eq!(metrics.size.distribution[0].1, 61);
    }

    #[test]
    fn test_param_metrics() {
        let mut graph = Graph::new();
        graph.add_unit(make_function("no_params", 1, 5, 0, 0));
        graph.add_unit(make_function("some_params", 10, 15, 3, 0));
        graph.add_unit(make_function("many_params", 20, 25, 7, 0));

        let metrics = ComplexityMetrics::compute(&graph);

        assert_eq!(metrics.params.max, 7);
        assert_eq!(metrics.params.distribution[0].0, "many_params");
    }

    fn make_function_with_cognitive(
        id: &str,
        branches: usize,
        cognitive: usize,
    ) -> Unit {
        let mut u = make_function(id, 1, 10, 0, branches);
        u.cognitive_complexity = cognitive;
        u
    }

    #[test]
    fn test_cognitive_metrics() {
        let mut graph = Graph::new();
        graph.add_unit(make_function_with_cognitive("flat", 3, 3)); // 3 flat ifs
        graph.add_unit(make_function_with_cognitive("nested", 3, 9)); // 3 nested ifs (1+0 + 1+1 + 1+2 = 6... but say 9)
        graph.add_unit(make_function_with_cognitive("simple", 0, 0));

        let metrics = ComplexityMetrics::compute(&graph);

        assert_eq!(metrics.cognitive.max, 9);
        assert_eq!(metrics.cognitive.distribution[0].0, "nested");
        assert_eq!(metrics.cognitive.distribution[0].1, 9);
    }

    #[test]
    fn test_cyclomatic_metrics() {
        let mut graph = Graph::new();
        graph.add_unit(make_function("simple", 1, 5, 0, 0)); // cc=1
        graph.add_unit(make_function("branchy", 10, 30, 0, 5)); // cc=6
        graph.add_unit(make_function("complex", 40, 100, 0, 15)); // cc=16

        let metrics = ComplexityMetrics::compute(&graph);

        assert_eq!(metrics.cyclomatic.max, 16);
        assert_eq!(metrics.cyclomatic.distribution[0].0, "complex");
        assert_eq!(metrics.cyclomatic.distribution[0].1, 16);
    }
}
