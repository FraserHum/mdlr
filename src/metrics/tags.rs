use crate::cache::SemanticTags;
use crate::graph::Graph;
use std::collections::HashMap;

/// Metrics computed from semantic tags
#[derive(Debug, Clone)]
pub struct TagMetrics {
    /// Total number of units
    pub total_units: usize,
    /// Number of units with at least one semantic tag
    pub tagged_units: usize,
    /// Tag coverage as a percentage (0.0 to 1.0)
    pub tag_coverage: f64,
    /// Distribution by namespace (namespace -> count of units)
    pub namespace_distribution: HashMap<String, usize>,
    /// Per-namespace breakdown (namespace -> (value -> count))
    pub namespace_values: HashMap<String, HashMap<String, usize>>,
}

impl TagMetrics {
    /// Compute tag metrics from graph and semantic tags
    pub fn compute(graph: &Graph, tags: &SemanticTags) -> Self {
        let total_units = graph.units.len();

        // Count units that have tags
        let tagged_units = graph
            .units
            .iter()
            .filter(|u| !tags.get_tags(&u.id).is_empty())
            .count();

        let tag_coverage = if total_units > 0 {
            tagged_units as f64 / total_units as f64
        } else {
            0.0
        };

        // Build namespace distributions
        let mut namespace_distribution: HashMap<String, usize> = HashMap::new();
        let mut namespace_values: HashMap<String, HashMap<String, usize>> = HashMap::new();

        for unit in &graph.units {
            for tag in tags.get_tags(&unit.id) {
                if let Some((namespace, value)) = tag.split_once(':') {
                    *namespace_distribution.entry(namespace.to_string()).or_insert(0) += 1;

                    let values = namespace_values
                        .entry(namespace.to_string())
                        .or_default();
                    *values.entry(value.to_string()).or_insert(0) += 1;
                }
            }
        }

        Self {
            total_units,
            tagged_units,
            tag_coverage,
            namespace_distribution,
            namespace_values,
        }
    }

    /// Check if there are any tags
    pub fn has_tags(&self) -> bool {
        self.tagged_units > 0
    }
}

impl std::fmt::Display for TagMetrics {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Tag Coverage")?;
        writeln!(f, "============")?;
        writeln!(f)?;
        writeln!(
            f,
            "Coverage: {:.1}% ({}/{} units tagged)",
            self.tag_coverage * 100.0,
            self.tagged_units,
            self.total_units
        )?;

        if !self.namespace_distribution.is_empty() {
            writeln!(f)?;
            writeln!(f, "By Namespace:")?;

            let mut namespaces: Vec<_> = self.namespace_distribution.iter().collect();
            namespaces.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));

            for (namespace, count) in namespaces {
                writeln!(f, "  {}: {} units", namespace, count)?;

                if let Some(values) = self.namespace_values.get(namespace) {
                    let mut values_vec: Vec<_> = values.iter().collect();
                    values_vec.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));

                    for (value, vcount) in values_vec.iter().take(5) {
                        writeln!(f, "    {}:{} ({})", namespace, value, vcount)?;
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{Span, Unit, UnitKind};
    use std::path::PathBuf;

    fn make_unit(id: &str) -> Unit {
        Unit {
            id: id.to_string(),
            kind: UnitKind::Function,
            file: PathBuf::from("test.rs"),
            span: Span {
                start_line: 1,
                start_col: 0,
                end_line: 10,
                end_col: 0,
            },
            reads: vec![],
            writes: vec![],
            calls: vec![],
            tags: vec![],
        }
    }

    #[test]
    fn test_empty_tags() {
        let mut graph = Graph::new();
        graph.add_unit(make_unit("a"));
        graph.add_unit(make_unit("b"));

        let tags = SemanticTags::new();
        let metrics = TagMetrics::compute(&graph, &tags);

        assert_eq!(metrics.total_units, 2);
        assert_eq!(metrics.tagged_units, 0);
        assert_eq!(metrics.tag_coverage, 0.0);
        assert!(!metrics.has_tags());
    }

    #[test]
    fn test_partial_coverage() {
        let mut graph = Graph::new();
        graph.add_unit(make_unit("a"));
        graph.add_unit(make_unit("b"));
        graph.add_unit(make_unit("c"));
        graph.add_unit(make_unit("d"));

        let mut tags = SemanticTags::new();
        tags.add_tag("a", "domain:auth").unwrap();
        tags.add_tag("b", "domain:billing").unwrap();

        let metrics = TagMetrics::compute(&graph, &tags);

        assert_eq!(metrics.total_units, 4);
        assert_eq!(metrics.tagged_units, 2);
        assert_eq!(metrics.tag_coverage, 0.5);
        assert!(metrics.has_tags());
        assert_eq!(*metrics.namespace_distribution.get("domain").unwrap(), 2);
    }

    #[test]
    fn test_namespace_values() {
        let mut graph = Graph::new();
        graph.add_unit(make_unit("a"));
        graph.add_unit(make_unit("b"));

        let mut tags = SemanticTags::new();
        tags.add_tag("a", "domain:auth").unwrap();
        tags.add_tag("a", "layer:api").unwrap();
        tags.add_tag("b", "domain:auth").unwrap();

        let metrics = TagMetrics::compute(&graph, &tags);

        let domain_values = metrics.namespace_values.get("domain").unwrap();
        assert_eq!(*domain_values.get("auth").unwrap(), 2);

        let layer_values = metrics.namespace_values.get("layer").unwrap();
        assert_eq!(*layer_values.get("api").unwrap(), 1);
    }
}
