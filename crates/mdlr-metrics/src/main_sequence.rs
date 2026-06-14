//! C# main sequence distance by directory module.
//!
//! Robert Martin's main sequence is `A + I = 1`, where `A` is
//! abstractness and `I` is instability. This module computes those values for
//! C# directory modules using existing type tags and resolved call edges.

use crate::DistributionMetrics;
use mdlr_core::{EdgeKind, Graph, Unit, UnitKind};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Debug, Clone)]
pub struct MainSequenceMetrics {
    pub distance: DistributionMetrics,
    pub refactor_pressure: DistributionMetrics,
    pub target_score: DistributionMetrics,
    pub priority_score: DistributionMetrics,
    pub modules: Vec<MainSequenceModule>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CSharpProjectFacts {
    pub cached_at: u64,
    pub projects: Vec<CSharpProject>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CSharpProject {
    pub project_path: String,
    #[serde(default)]
    pub source_files: Vec<String>,
    #[serde(default)]
    pub project_references: Vec<String>,
    #[serde(default)]
    pub output_type: Option<String>,
    #[serde(default)]
    pub is_test_project: Option<bool>,
    #[serde(default)]
    pub has_microsoft_net_test_sdk: bool,
    #[serde(default)]
    pub test_package_references: Vec<String>,
    #[serde(default)]
    pub explicit_test_project: bool,
    #[serde(default)]
    pub reachable_from_executable: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct MainSequenceModule {
    pub id: String,
    pub abstractness: f64,
    pub instability: f64,
    pub distance: usize,
    pub ca: usize,
    pub ce: usize,
    pub type_count: usize,
    pub abstract_type_count: usize,
    pub zone: MainSequenceZone,
    pub architecture_priority: usize,
    pub implementation_complexity: usize,
    pub refactor_pressure: usize,
    pub refactor_payoff: usize,
    pub refactor_effort: usize,
    pub refactor_target_score: usize,
    pub project_paths: Vec<String>,
    pub explicit_test_project: bool,
    pub reachable_from_executable: bool,
    pub project_context_weight: f64,
    pub refactor_priority_score: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MainSequenceZone {
    Balanced,
    ZoneOfPain,
    ZoneOfUselessness,
}

#[derive(Default)]
struct ModuleCounts {
    type_count: usize,
    abstract_type_count: usize,
    ca_modules: HashSet<String>,
    ce_modules: HashSet<String>,
    file_max_lines: HashMap<String, usize>,
    max_cognitive: usize,
    total_cognitive: usize,
    max_scope: usize,
    method_count: usize,
    call_count: usize,
    write_count: usize,
    max_lcom4: usize,
}

#[derive(Default)]
struct ModuleProjectContext {
    project_paths: Vec<String>,
    explicit_test_project: bool,
    reachable_from_executable: bool,
    project_context_weight: f64,
}

impl MainSequenceMetrics {
    #[tracing::instrument(name = "compute_main_sequence", skip_all)]
    pub fn compute(graph: &Graph) -> Self {
        Self::compute_with_project_facts(graph, None)
    }

    #[tracing::instrument(name = "compute_main_sequence", skip_all)]
    pub fn compute_with_project_facts(
        graph: &Graph,
        project_facts: Option<&CSharpProjectFacts>,
    ) -> Self {
        let mut unit_modules = HashMap::new();
        let mut modules: HashMap<String, ModuleCounts> = HashMap::new();
        let source_project_index = project_facts.map(source_project_index);

        for unit in graph.units.iter().filter(|u| is_csharp_unit(u)) {
            let module = module_for(unit);
            unit_modules.insert(unit.id.clone(), module.clone());
            let counts = modules.entry(module).or_default();
            let file_path = unit.file.to_string_lossy().to_string();
            let file_max = counts.file_max_lines.entry(file_path).or_insert(0);
            *file_max = (*file_max).max(unit.span.end_line);
            if unit.kind == UnitKind::Function || unit.kind == UnitKind::Method
            {
                counts.method_count += 1;
                counts.max_cognitive =
                    counts.max_cognitive.max(unit.cognitive_complexity);
                counts.total_cognitive += unit.cognitive_complexity;
                counts.max_scope = counts.max_scope.max(unit.max_scope_lines);
                counts.call_count += unit.calls.len();
                counts.write_count += unit.writes.len();
            }
            if is_csharp_type(unit) {
                counts.type_count += 1;
                if is_abstract_type(unit) {
                    counts.abstract_type_count += 1;
                }
            }
        }

        for edge in graph.edges.iter().filter(|e| e.kind == EdgeKind::Calls) {
            let Some(from_module) = unit_modules.get(&edge.from) else {
                continue;
            };
            let Some(to_module) = unit_modules.get(&edge.to) else {
                continue;
            };
            if from_module == to_module {
                continue;
            }
            modules
                .entry(from_module.clone())
                .or_default()
                .ce_modules
                .insert(to_module.clone());
            modules
                .entry(to_module.clone())
                .or_default()
                .ca_modules
                .insert(from_module.clone());
        }

        for (module, max_lcom4) in module_max_lcom4(graph, &unit_modules) {
            modules.entry(module).or_default().max_lcom4 = max_lcom4;
        }

        let max_coupling_basis = modules
            .values()
            .map(|counts| coupling_basis(counts))
            .max()
            .unwrap_or(0);
        let max_type_count = modules
            .values()
            .map(|counts| counts.type_count)
            .max()
            .unwrap_or(0);
        let max_module_max_cognitive = modules
            .values()
            .map(|counts| counts.max_cognitive)
            .max()
            .unwrap_or(0);
        let max_module_total_cognitive = modules
            .values()
            .map(|counts| counts.total_cognitive)
            .max()
            .unwrap_or(0);
        let max_module_max_scope =
            modules.values().map(|counts| counts.max_scope).max().unwrap_or(0);
        let max_module_write_count = modules
            .values()
            .map(|counts| counts.write_count)
            .max()
            .unwrap_or(0);
        let max_module_ce = modules
            .values()
            .map(|counts| counts.ce_modules.len())
            .max()
            .unwrap_or(0);
        let max_module_call_count = modules
            .values()
            .map(|counts| counts.call_count)
            .max()
            .unwrap_or(0);
        let max_module_method_count = modules
            .values()
            .map(|counts| counts.method_count)
            .max()
            .unwrap_or(0);
        let max_module_file_count = modules
            .values()
            .map(|counts| counts.file_max_lines.len())
            .max()
            .unwrap_or(0);
        let max_module_total_loc =
            modules.values().map(total_loc).max().unwrap_or(0);
        let max_module_ca = modules
            .values()
            .map(|counts| counts.ca_modules.len())
            .max()
            .unwrap_or(0);
        let max_module_lcom4_minus_1 = modules
            .values()
            .map(|counts| counts.max_lcom4.saturating_sub(1))
            .max()
            .unwrap_or(0);

        let mut detail: Vec<_> = modules
            .into_iter()
            .map(|(id, counts)| {
                let ca = counts.ca_modules.len();
                let ce = counts.ce_modules.len();
                let abstractness =
                    ratio(counts.abstract_type_count, counts.type_count);
                let instability = ratio(ce, ca + ce);
                let raw_distance = (abstractness + instability - 1.0).abs();
                let distance = score(raw_distance * 100.0);
                let zone = zone_for(abstractness, instability, raw_distance);
                let architecture_priority = architecture_priority(
                    distance,
                    zone,
                    ca,
                    ce,
                    counts.type_count,
                    max_coupling_basis,
                    max_type_count,
                );
                let implementation_complexity = implementation_complexity(
                    counts.max_cognitive,
                    total_loc(&counts),
                    counts.max_lcom4,
                    max_module_max_cognitive,
                    max_module_total_loc,
                    max_module_lcom4_minus_1,
                );
                let refactor_pressure = score(
                    100.0
                        * (0.60 * architecture_priority as f64 / 100.0
                            + 0.40 * implementation_complexity as f64 / 100.0),
                );
                let behavior_complexity = behavior_complexity(
                    &counts,
                    max_module_max_cognitive,
                    max_module_total_cognitive,
                    max_module_max_scope,
                    max_module_write_count,
                );
                let coordination_complexity = coordination_complexity(
                    &counts,
                    ce,
                    max_module_ce,
                    max_module_call_count,
                    max_module_method_count,
                    max_module_file_count,
                );
                let refactor_payoff = refactor_payoff(
                    behavior_complexity,
                    coordination_complexity,
                    refactor_pressure,
                );
                let refactor_effort = refactor_effort(
                    &counts,
                    ca,
                    max_module_total_loc,
                    max_module_file_count,
                    max_type_count,
                    max_module_method_count,
                    max_module_ca,
                );
                let refactor_target_score =
                    refactor_target_score(refactor_payoff, refactor_effort);
                let project_context = project_context_for(
                    &counts,
                    source_project_index.as_ref(),
                );
                let refactor_priority_score = refactor_priority_score(
                    refactor_target_score,
                    project_context.project_context_weight,
                );
                MainSequenceModule {
                    id,
                    abstractness,
                    instability,
                    distance,
                    ca,
                    ce,
                    type_count: counts.type_count,
                    abstract_type_count: counts.abstract_type_count,
                    zone,
                    architecture_priority,
                    implementation_complexity,
                    refactor_pressure,
                    refactor_payoff,
                    refactor_effort,
                    refactor_target_score,
                    project_paths: project_context.project_paths,
                    explicit_test_project: project_context
                        .explicit_test_project,
                    reachable_from_executable: project_context
                        .reachable_from_executable,
                    project_context_weight: project_context
                        .project_context_weight,
                    refactor_priority_score,
                }
            })
            .collect();
        detail.sort_by(|a, b| a.id.cmp(&b.id));

        let distance_counts =
            detail.iter().map(|m| (m.id.clone(), m.distance)).collect();
        let refactor_pressure_counts = detail
            .iter()
            .map(|m| (m.id.clone(), m.refactor_pressure))
            .collect();
        let target_score_counts = detail
            .iter()
            .map(|m| (m.id.clone(), m.refactor_target_score))
            .collect();
        let priority_score_counts = detail
            .iter()
            .map(|m| (m.id.clone(), m.refactor_priority_score))
            .collect();

        Self {
            distance: DistributionMetrics::from_counts(distance_counts),
            refactor_pressure: DistributionMetrics::from_counts(
                refactor_pressure_counts,
            ),
            target_score: DistributionMetrics::from_counts(
                target_score_counts,
            ),
            priority_score: DistributionMetrics::from_counts(
                priority_score_counts,
            ),
            modules: detail,
        }
    }

    pub fn retain_modules(&mut self, keep: &HashSet<String>) {
        self.modules.retain(|m| keep.contains(&m.id));
        let counts =
            self.modules.iter().map(|m| (m.id.clone(), m.distance)).collect();
        self.distance = DistributionMetrics::from_counts(counts);
        let counts = self
            .modules
            .iter()
            .map(|m| (m.id.clone(), m.refactor_pressure))
            .collect();
        self.refactor_pressure = DistributionMetrics::from_counts(counts);
        let counts = self
            .modules
            .iter()
            .map(|m| (m.id.clone(), m.refactor_target_score))
            .collect();
        self.target_score = DistributionMetrics::from_counts(counts);
        let counts = self
            .modules
            .iter()
            .map(|m| (m.id.clone(), m.refactor_priority_score))
            .collect();
        self.priority_score = DistributionMetrics::from_counts(counts);
    }
}

pub fn module_for(unit: &Unit) -> String {
    let parent = unit.file.parent().unwrap_or_else(|| Path::new(""));
    if parent.as_os_str().is_empty() {
        ".".to_string()
    } else {
        parent.to_string_lossy().replace('\\', "/")
    }
}

pub fn is_csharp_unit(unit: &Unit) -> bool {
    unit.file.extension().and_then(|e| e.to_str()) == Some("cs")
}

fn source_project_index(
    facts: &CSharpProjectFacts,
) -> HashMap<String, Vec<&CSharpProject>> {
    let mut index: HashMap<String, Vec<&CSharpProject>> = HashMap::new();
    for project in &facts.projects {
        for source_file in &project.source_files {
            index
                .entry(normalize_rel_path(source_file))
                .or_default()
                .push(project);
        }
    }
    index
}

fn project_context_for(
    counts: &ModuleCounts,
    source_project_index: Option<&HashMap<String, Vec<&CSharpProject>>>,
) -> ModuleProjectContext {
    let Some(source_project_index) = source_project_index else {
        return ModuleProjectContext {
            project_context_weight: 1.0,
            ..ModuleProjectContext::default()
        };
    };

    let mut projects: HashMap<String, &CSharpProject> = HashMap::new();
    for file in counts.file_max_lines.keys() {
        if let Some(file_projects) =
            source_project_index.get(&normalize_rel_path(file))
        {
            for project in file_projects {
                projects.insert(project.project_path.clone(), project);
            }
        }
    }

    if projects.is_empty() {
        return ModuleProjectContext {
            project_context_weight: 1.0,
            ..ModuleProjectContext::default()
        };
    }

    let mut project_paths: Vec<_> = projects.keys().cloned().collect();
    project_paths.sort();

    let explicit_test_project =
        projects.values().all(|project| project.explicit_test_project);
    let reachable_from_executable =
        projects.values().any(|project| project.reachable_from_executable);
    let project_context_weight = if explicit_test_project {
        0.95
    } else if reachable_from_executable {
        1.05
    } else {
        1.0
    };

    ModuleProjectContext {
        project_paths,
        explicit_test_project,
        reachable_from_executable,
        project_context_weight,
    }
}

fn normalize_rel_path(path: &str) -> String {
    path.replace('\\', "/")
}

fn is_csharp_type(unit: &Unit) -> bool {
    unit.kind == UnitKind::Struct
        && unit.tags.iter().any(|tag| {
            matches!(
                tag.as_str(),
                "class" | "interface" | "struct" | "record" | "record-struct"
            )
        })
        && !unit.tags.iter().any(|tag| tag == "enum")
}

fn is_abstract_type(unit: &Unit) -> bool {
    unit.tags.iter().any(|tag| tag == "interface" || tag == "abstract")
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 { 0.0 } else { numerator as f64 / denominator as f64 }
}

fn total_loc(counts: &ModuleCounts) -> usize {
    counts.file_max_lines.values().sum()
}

fn coupling_basis(counts: &ModuleCounts) -> usize {
    let ca = counts.ca_modules.len();
    let ce = counts.ce_modules.len();
    let abstractness = ratio(counts.abstract_type_count, counts.type_count);
    let instability = ratio(ce, ca + ce);
    let distance = (abstractness + instability - 1.0).abs();
    match zone_for(abstractness, instability, distance) {
        MainSequenceZone::ZoneOfPain => ca,
        MainSequenceZone::ZoneOfUselessness => ce,
        MainSequenceZone::Balanced => ca.max(ce),
    }
}

fn score(value: f64) -> usize {
    value.round().clamp(0.0, 100.0) as usize
}

fn norm_log(value: usize, max: usize) -> f64 {
    if max == 0 {
        0.0
    } else {
        (1.0 + value as f64).ln() / (1.0 + max as f64).ln()
    }
}

fn architecture_priority(
    distance: usize,
    zone: MainSequenceZone,
    ca: usize,
    ce: usize,
    type_count: usize,
    max_coupling_basis: usize,
    max_type_count: usize,
) -> usize {
    let coupling_basis = match zone {
        MainSequenceZone::ZoneOfPain => ca,
        MainSequenceZone::ZoneOfUselessness => ce,
        MainSequenceZone::Balanced => ca.max(ce),
    };
    score(
        100.0
            * (0.50 * distance as f64 / 100.0
                + 0.35 * norm_log(coupling_basis, max_coupling_basis)
                + 0.15 * norm_log(type_count, max_type_count)),
    )
}

fn implementation_complexity(
    max_cognitive: usize,
    total_loc: usize,
    max_lcom4: usize,
    max_module_max_cognitive: usize,
    max_module_total_loc: usize,
    max_module_lcom4_minus_1: usize,
) -> usize {
    score(
        100.0
            * (0.50 * norm_log(max_cognitive, max_module_max_cognitive)
                + 0.30 * norm_log(total_loc, max_module_total_loc)
                + 0.20
                    * norm_log(
                        max_lcom4.saturating_sub(1),
                        max_module_lcom4_minus_1,
                    )),
    )
}

fn behavior_complexity(
    counts: &ModuleCounts,
    max_module_max_cognitive: usize,
    max_module_total_cognitive: usize,
    max_module_max_scope: usize,
    max_module_write_count: usize,
) -> usize {
    score(
        100.0
            * (0.40
                * norm_log(counts.max_cognitive, max_module_max_cognitive)
                + 0.25
                    * norm_log(
                        counts.total_cognitive,
                        max_module_total_cognitive,
                    )
                + 0.20 * norm_log(counts.max_scope, max_module_max_scope)
                + 0.15 * norm_log(counts.write_count, max_module_write_count)),
    )
}

fn coordination_complexity(
    counts: &ModuleCounts,
    ce: usize,
    max_module_ce: usize,
    max_module_call_count: usize,
    max_module_method_count: usize,
    max_module_file_count: usize,
) -> usize {
    score(
        100.0
            * (0.45 * norm_log(ce, max_module_ce)
                + 0.25 * norm_log(counts.call_count, max_module_call_count)
                + 0.20
                    * norm_log(counts.method_count, max_module_method_count)
                + 0.10
                    * norm_log(
                        counts.file_max_lines.len(),
                        max_module_file_count,
                    )),
    )
}

fn refactor_payoff(
    behavior_complexity: usize,
    coordination_complexity: usize,
    refactor_pressure: usize,
) -> usize {
    score(
        0.45 * behavior_complexity as f64
            + 0.30 * coordination_complexity as f64
            + 0.25 * refactor_pressure as f64,
    )
}

fn refactor_effort(
    counts: &ModuleCounts,
    ca: usize,
    max_module_total_loc: usize,
    max_module_file_count: usize,
    max_module_type_count: usize,
    max_module_method_count: usize,
    max_module_ca: usize,
) -> usize {
    score(
        100.0
            * (0.35 * norm_log(total_loc(counts), max_module_total_loc)
                + 0.20
                    * norm_log(
                        counts.file_max_lines.len(),
                        max_module_file_count,
                    )
                + 0.20 * norm_log(counts.type_count, max_module_type_count)
                + 0.15
                    * norm_log(counts.method_count, max_module_method_count)
                + 0.10 * norm_log(ca, max_module_ca)),
    )
}

fn refactor_target_score(
    refactor_payoff: usize,
    refactor_effort: usize,
) -> usize {
    score(
        refactor_payoff as f64
            * (1.10 - 0.40 * refactor_effort as f64 / 100.0),
    )
}

fn refactor_priority_score(
    refactor_target_score: usize,
    project_context_weight: f64,
) -> usize {
    score(refactor_target_score as f64 * project_context_weight)
}

fn zone_for(
    abstractness: f64,
    instability: f64,
    distance: f64,
) -> MainSequenceZone {
    if distance < 0.30 {
        MainSequenceZone::Balanced
    } else if abstractness + instability < 1.0 {
        MainSequenceZone::ZoneOfPain
    } else {
        MainSequenceZone::ZoneOfUselessness
    }
}

fn module_max_lcom4(
    graph: &Graph,
    unit_modules: &HashMap<String, String>,
) -> HashMap<String, usize> {
    let mut methods_by_parent: HashMap<String, Vec<&Unit>> = HashMap::new();
    for unit in graph.units.iter().filter(|unit| {
        unit.kind == UnitKind::Method
            && unit.parent.is_some()
            && unit_modules.contains_key(&unit.id)
    }) {
        methods_by_parent
            .entry(unit.parent.clone().unwrap())
            .or_default()
            .push(unit);
    }

    let mut module_max = HashMap::new();
    for (parent, methods) in methods_by_parent {
        let Some(module) = unit_modules.get(&parent) else {
            continue;
        };
        let lcom4 = compute_lcom4(&methods);
        let current = module_max.entry(module.clone()).or_insert(0);
        *current = (*current).max(lcom4);
    }
    module_max
}

fn compute_lcom4(methods: &[&Unit]) -> usize {
    if methods.is_empty() {
        return 0;
    }
    if methods.len() == 1 {
        return 1;
    }

    let mut uf = UnionFind::new(methods.len());
    let mut field_to_methods: HashMap<&String, Vec<usize>> = HashMap::new();
    for (idx, method) in methods.iter().enumerate() {
        for field in method.reads.iter().chain(method.writes.iter()) {
            field_to_methods.entry(field).or_default().push(idx);
        }
    }
    for method_indices in field_to_methods.values() {
        for window in method_indices.windows(2) {
            uf.union(window[0], window[1]);
        }
    }

    let simple_name_to_idx: HashMap<&str, usize> = methods
        .iter()
        .enumerate()
        .map(|(idx, method)| (simple_name(&method.id), idx))
        .collect();
    for (idx, method) in methods.iter().enumerate() {
        for call in &method.calls {
            if let Some(&called_idx) =
                simple_name_to_idx.get(simple_name(call))
            {
                uf.union(idx, called_idx);
            }
        }
    }

    uf.count_components()
}

fn simple_name(id: &str) -> &str {
    id.rsplit("::").next().and_then(|s| s.rsplit('.').next()).unwrap_or(id)
}

struct UnionFind {
    parent: Vec<usize>,
    rank: Vec<usize>,
}

impl UnionFind {
    fn new(n: usize) -> Self {
        Self { parent: (0..n).collect(), rank: vec![0; n] }
    }

    fn find(&mut self, i: usize) -> usize {
        if self.parent[i] != i {
            self.parent[i] = self.find(self.parent[i]);
        }
        self.parent[i]
    }

    fn union(&mut self, i: usize, j: usize) {
        let ri = self.find(i);
        let rj = self.find(j);
        if ri != rj {
            if self.rank[ri] < self.rank[rj] {
                self.parent[ri] = rj;
            } else if self.rank[ri] > self.rank[rj] {
                self.parent[rj] = ri;
            } else {
                self.parent[rj] = ri;
                self.rank[ri] += 1;
            }
        }
    }

    fn count_components(&mut self) -> usize {
        let n = self.parent.len();
        let mut roots = HashSet::new();
        for i in 0..n {
            roots.insert(self.find(i));
        }
        roots.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mdlr_core::{Edge, Span};
    use std::path::PathBuf;

    fn type_unit(id: &str, file: &str, tags: &[&str]) -> Unit {
        unit(id, UnitKind::Struct, file, tags)
    }

    fn method_unit(id: &str, file: &str) -> Unit {
        unit(id, UnitKind::Method, file, &[])
    }

    fn unit(id: &str, kind: UnitKind, file: &str, tags: &[&str]) -> Unit {
        Unit {
            id: id.to_string(),
            kind,
            file: PathBuf::from(file),
            span: Span {
                start_line: 1,
                start_col: 0,
                end_line: 1,
                end_col: 0,
            },
            reads: vec![],
            writes: vec![],
            calls: vec![],
            tags: tags.iter().map(|s| s.to_string()).collect(),
            params: 0,
            branches: 0,
            max_scope_lines: 0,
            parent: None,
            cognitive_complexity: 0,
            partial: false,
        }
    }

    fn method_unit_with_metrics(
        id: &str,
        file: &str,
        start_line: usize,
        end_line: usize,
        cognitive_complexity: usize,
    ) -> Unit {
        let mut unit = method_unit(id, file);
        unit.span.start_line = start_line;
        unit.span.end_line = end_line;
        unit.cognitive_complexity = cognitive_complexity;
        unit
    }

    fn method_unit_with_details(
        id: &str,
        file: &str,
        end_line: usize,
        cognitive_complexity: usize,
        max_scope_lines: usize,
        calls: &[&str],
        writes: &[&str],
    ) -> Unit {
        let mut unit = method_unit_with_metrics(
            id,
            file,
            1,
            end_line,
            cognitive_complexity,
        );
        unit.max_scope_lines = max_scope_lines;
        unit.calls = calls.iter().map(|call| call.to_string()).collect();
        unit.writes = writes.iter().map(|write| write.to_string()).collect();
        unit
    }

    fn call(from: &str, to: &str) -> Edge {
        Edge {
            from: from.to_string(),
            to: to.to_string(),
            kind: EdgeKind::Calls,
        }
    }

    fn module<'a>(
        metrics: &'a MainSequenceMetrics,
        id: &str,
    ) -> &'a MainSequenceModule {
        metrics.modules.iter().find(|m| m.id == id).unwrap()
    }

    #[test]
    fn balanced_module_can_land_near_zero_distance() {
        let mut graph = Graph::new();
        graph.add_unit(type_unit(
            "api.cs::Api.IThing",
            "api/api.cs",
            &["interface"],
        ));
        graph.add_unit(type_unit(
            "impl.cs::Impl.Thing",
            "impl/impl.cs",
            &["class"],
        ));
        graph.add_unit(method_unit("api.cs::Api.IThing::Run()", "api/api.cs"));
        graph.add_unit(method_unit(
            "impl.cs::Impl.Thing::Run()",
            "impl/impl.cs",
        ));
        graph.add_edge(call(
            "impl.cs::Impl.Thing::Run()",
            "api.cs::Api.IThing::Run()",
        ));

        let metrics = MainSequenceMetrics::compute(&graph);
        let api = module(&metrics, "api");

        assert_eq!(api.abstract_type_count, 1);
        assert_eq!(api.type_count, 1);
        assert_eq!(api.ce, 0);
        assert_eq!(api.ca, 1);
        assert_eq!(api.zone, MainSequenceZone::Balanced);
        assert_eq!(
            metrics
                .distance
                .distribution
                .iter()
                .find(|(id, _)| id == "api")
                .map(|(_, value)| *value),
            Some(0)
        );
    }

    #[test]
    fn concrete_stable_module_is_zone_of_pain() {
        let mut graph = Graph::new();
        graph.add_unit(type_unit(
            "core/core.cs::Core.Service",
            "core/core.cs",
            &["class"],
        ));
        graph.add_unit(type_unit(
            "app/app.cs::App.Runner",
            "app/app.cs",
            &["class"],
        ));
        graph.add_unit(method_unit(
            "core/core.cs::Core.Service::Run()",
            "core/core.cs",
        ));
        graph.add_unit(method_unit(
            "app/app.cs::App.Runner::Run()",
            "app/app.cs",
        ));
        graph.add_edge(call(
            "app/app.cs::App.Runner::Run()",
            "core/core.cs::Core.Service::Run()",
        ));

        let metrics = MainSequenceMetrics::compute(&graph);
        let core = module(&metrics, "core");

        assert_eq!(core.ca, 1);
        assert_eq!(core.ce, 0);
        assert_eq!(core.abstractness, 0.0);
        assert_eq!(core.instability, 0.0);
        assert_eq!(core.zone, MainSequenceZone::ZoneOfPain);
    }

    #[test]
    fn abstract_unstable_module_is_zone_of_uselessness() {
        let mut graph = Graph::new();
        graph.add_unit(type_unit(
            "ports/p.cs::Ports.IPort",
            "ports/p.cs",
            &["interface"],
        ));
        graph.add_unit(type_unit(
            "adapters/a.cs::Adapters.Adapter",
            "adapters/a.cs",
            &["class"],
        ));
        graph.add_unit(method_unit(
            "ports/p.cs::Ports.IPort::Run()",
            "ports/p.cs",
        ));
        graph.add_unit(method_unit(
            "adapters/a.cs::Adapters.Adapter::Run()",
            "adapters/a.cs",
        ));
        graph.add_edge(call(
            "ports/p.cs::Ports.IPort::Run()",
            "adapters/a.cs::Adapters.Adapter::Run()",
        ));

        let metrics = MainSequenceMetrics::compute(&graph);
        let ports = module(&metrics, "ports");

        assert_eq!(ports.abstractness, 1.0);
        assert_eq!(ports.instability, 1.0);
        assert_eq!(ports.zone, MainSequenceZone::ZoneOfUselessness);
    }

    #[test]
    fn dependency_free_modules_are_retained_for_json_details() {
        let mut graph = Graph::new();
        graph.add_unit(type_unit(
            "isolated/solo.cs::Isolated.Solo",
            "isolated/solo.cs",
            &["class"],
        ));

        let metrics = MainSequenceMetrics::compute(&graph);
        let isolated = module(&metrics, "isolated");

        assert_eq!(isolated.ca + isolated.ce, 0);
        assert_eq!(isolated.zone, MainSequenceZone::ZoneOfPain);
        assert_eq!(metrics.modules.len(), 1);
    }

    #[test]
    fn duplicate_call_edges_count_once_per_module_pair() {
        let mut graph = Graph::new();
        graph.add_unit(type_unit("a/a.cs::A.Type", "a/a.cs", &["class"]));
        graph.add_unit(type_unit("b/b.cs::B.Type", "b/b.cs", &["class"]));
        graph.add_unit(method_unit("a/a.cs::A.Type::One()", "a/a.cs"));
        graph.add_unit(method_unit("a/a.cs::A.Type::Two()", "a/a.cs"));
        graph.add_unit(method_unit("b/b.cs::B.Type::One()", "b/b.cs"));
        graph.add_unit(method_unit("b/b.cs::B.Type::Two()", "b/b.cs"));
        graph.add_edge(call("a/a.cs::A.Type::One()", "b/b.cs::B.Type::One()"));
        graph.add_edge(call("a/a.cs::A.Type::Two()", "b/b.cs::B.Type::Two()"));
        graph.add_edge(call("a/a.cs::A.Type::One()", "b/b.cs::B.Type::Two()"));

        let metrics = MainSequenceMetrics::compute(&graph);
        let a = module(&metrics, "a");
        let b = module(&metrics, "b");

        assert_eq!(a.ce, 1);
        assert_eq!(b.ca, 1);
    }

    #[test]
    fn pressure_promotes_significant_distance_80_module_over_tiny_100() {
        let mut graph = Graph::new();
        graph.add_unit(type_unit(
            "core/t0.cs::Core.T0",
            "core/t0.cs",
            &["class", "abstract"],
        ));
        for i in 1..5 {
            graph.add_unit(type_unit(
                &format!("core/t{i}.cs::Core.T{i}"),
                &format!("core/t{i}.cs"),
                &["class"],
            ));
        }
        graph.add_unit(method_unit_with_metrics(
            "core/t0.cs::Core.T0::Run()",
            "core/t0.cs",
            1,
            200,
            30,
        ));

        graph.add_unit(type_unit(
            "tiny/t.cs::Tiny.T",
            "tiny/t.cs",
            &["class"],
        ));
        graph.add_unit(method_unit_with_metrics(
            "tiny/t.cs::Tiny.T::Run()",
            "tiny/t.cs",
            1,
            5,
            1,
        ));

        for i in 0..5 {
            graph.add_unit(type_unit(
                &format!("caller{i}/c.cs::Caller{i}.C"),
                &format!("caller{i}/c.cs"),
                &["class"],
            ));
            graph.add_unit(method_unit(
                &format!("caller{i}/c.cs::Caller{i}.C::Run()"),
                &format!("caller{i}/c.cs"),
            ));
            graph.add_edge(call(
                &format!("caller{i}/c.cs::Caller{i}.C::Run()"),
                "core/t0.cs::Core.T0::Run()",
            ));
        }
        graph.add_unit(type_unit(
            "tinycaller/c.cs::TinyCaller.C",
            "tinycaller/c.cs",
            &["class"],
        ));
        graph.add_unit(method_unit(
            "tinycaller/c.cs::TinyCaller.C::Run()",
            "tinycaller/c.cs",
        ));
        graph.add_edge(call(
            "tinycaller/c.cs::TinyCaller.C::Run()",
            "tiny/t.cs::Tiny.T::Run()",
        ));

        let metrics = MainSequenceMetrics::compute(&graph);
        let core = module(&metrics, "core");
        let tiny = module(&metrics, "tiny");

        assert_eq!(core.distance, 80);
        assert_eq!(tiny.distance, 100);
        assert!(
            core.refactor_pressure > tiny.refactor_pressure,
            "core pressure {} should outrank tiny pressure {}",
            core.refactor_pressure,
            tiny.refactor_pressure
        );
        assert_eq!(metrics.refactor_pressure.distribution[0].0, "core");
    }

    #[test]
    fn architecture_priority_uses_zone_specific_coupling_basis() {
        let mut graph = Graph::new();
        graph.add_unit(type_unit(
            "pain/p.cs::Pain.P",
            "pain/p.cs",
            &["class"],
        ));
        graph.add_unit(method_unit("pain/p.cs::Pain.P::Run()", "pain/p.cs"));
        graph.add_unit(type_unit(
            "useless/u.cs::Useless.IU",
            "useless/u.cs",
            &["interface"],
        ));
        graph.add_unit(method_unit(
            "useless/u.cs::Useless.IU::Run()",
            "useless/u.cs",
        ));

        for i in 0..5 {
            graph.add_unit(type_unit(
                &format!("in{i}/i.cs::In{i}.T"),
                &format!("in{i}/i.cs"),
                &["class"],
            ));
            graph.add_unit(method_unit(
                &format!("in{i}/i.cs::In{i}.T::Run()"),
                &format!("in{i}/i.cs"),
            ));
            graph.add_edge(call(
                &format!("in{i}/i.cs::In{i}.T::Run()"),
                "pain/p.cs::Pain.P::Run()",
            ));

            graph.add_unit(type_unit(
                &format!("out{i}/o.cs::Out{i}.T"),
                &format!("out{i}/o.cs"),
                &["class"],
            ));
            graph.add_unit(method_unit(
                &format!("out{i}/o.cs::Out{i}.T::Run()"),
                &format!("out{i}/o.cs"),
            ));
            graph.add_edge(call(
                "useless/u.cs::Useless.IU::Run()",
                &format!("out{i}/o.cs::Out{i}.T::Run()"),
            ));
        }

        let metrics = MainSequenceMetrics::compute(&graph);
        let pain = module(&metrics, "pain");
        let useless = module(&metrics, "useless");

        assert_eq!(pain.zone, MainSequenceZone::ZoneOfPain);
        assert_eq!(useless.zone, MainSequenceZone::ZoneOfUselessness);
        assert_eq!(pain.architecture_priority, useless.architecture_priority);
        assert!(pain.architecture_priority > 90);
    }

    #[test]
    fn target_score_ignores_path_names_for_identical_graph_shapes() {
        let mut graph = Graph::new();
        for (module_path, ns) in [
            ("debug/data/shape.cs", "DebugData"),
            ("gameplay/core/shape.cs", "GameplayCore"),
        ] {
            graph.add_unit(type_unit(
                &format!("{module_path}::{ns}.Shape"),
                module_path,
                &["class"],
            ));
            graph.add_unit(method_unit_with_details(
                &format!("{module_path}::{ns}.Shape::Run()"),
                module_path,
                80,
                12,
                30,
                &["Dep.One", "Dep.Two"],
                &["state"],
            ));
        }
        graph.add_unit(type_unit(
            "deps/dep.cs::Deps.Dep",
            "deps/dep.cs",
            &["class"],
        ));
        graph.add_unit(method_unit(
            "deps/dep.cs::Deps.Dep::Run()",
            "deps/dep.cs",
        ));
        graph.add_edge(call(
            "debug/data/shape.cs::DebugData.Shape::Run()",
            "deps/dep.cs::Deps.Dep::Run()",
        ));
        graph.add_edge(call(
            "gameplay/core/shape.cs::GameplayCore.Shape::Run()",
            "deps/dep.cs::Deps.Dep::Run()",
        ));

        let metrics = MainSequenceMetrics::compute(&graph);
        let debug_data = module(&metrics, "debug/data");
        let gameplay_core = module(&metrics, "gameplay/core");

        assert_eq!(debug_data.refactor_payoff, gameplay_core.refactor_payoff);
        assert_eq!(debug_data.refactor_effort, gameplay_core.refactor_effort);
        assert_eq!(
            debug_data.refactor_target_score,
            gameplay_core.refactor_target_score
        );
    }

    #[test]
    fn project_context_weight_uses_only_project_facts() {
        let mut graph = Graph::new();
        for (file, ns) in [
            ("src/Product/Service.cs", "Product"),
            ("tests/Product.Tests/ServiceTests.cs", "ProductTests"),
            ("unknown/Loose.cs", "Loose"),
        ] {
            graph.add_unit(type_unit(
                &format!("{file}::{ns}.Service"),
                file,
                &["class"],
            ));
            graph.add_unit(method_unit_with_details(
                &format!("{file}::{ns}.Service::Run()"),
                file,
                80,
                12,
                30,
                &["Dep.One", "Dep.Two"],
                &["state"],
            ));
        }
        graph.add_unit(type_unit(
            "dep/Dep.cs::Dep.Service",
            "dep/Dep.cs",
            &["class"],
        ));
        graph.add_unit(method_unit(
            "dep/Dep.cs::Dep.Service::Run()",
            "dep/Dep.cs",
        ));
        graph.add_edge(call(
            "src/Product/Service.cs::Product.Service::Run()",
            "dep/Dep.cs::Dep.Service::Run()",
        ));
        graph.add_edge(call(
            "tests/Product.Tests/ServiceTests.cs::ProductTests.Service::Run()",
            "dep/Dep.cs::Dep.Service::Run()",
        ));
        graph.add_edge(call(
            "unknown/Loose.cs::Loose.Service::Run()",
            "dep/Dep.cs::Dep.Service::Run()",
        ));
        let facts = CSharpProjectFacts {
            cached_at: 1,
            projects: vec![
                CSharpProject {
                    project_path: "src/Product/Product.csproj".to_string(),
                    source_files: vec!["src/Product/Service.cs".to_string()],
                    output_type: Some("Exe".to_string()),
                    reachable_from_executable: true,
                    ..Default::default()
                },
                CSharpProject {
                    project_path: "tests/Product.Tests/Product.Tests.csproj"
                        .to_string(),
                    source_files: vec![
                        "tests/Product.Tests/ServiceTests.cs".to_string(),
                    ],
                    is_test_project: Some(true),
                    explicit_test_project: true,
                    ..Default::default()
                },
            ],
        };

        let metrics = MainSequenceMetrics::compute_with_project_facts(
            &graph,
            Some(&facts),
        );
        let product = module(&metrics, "src/Product");
        let tests = module(&metrics, "tests/Product.Tests");
        let unknown = module(&metrics, "unknown");

        assert_eq!(product.project_context_weight, 1.05);
        assert!(product.reachable_from_executable);
        assert!(
            product.refactor_priority_score > product.refactor_target_score
        );
        assert_eq!(tests.project_context_weight, 0.95);
        assert!(tests.explicit_test_project);
        assert!(tests.refactor_priority_score < tests.refactor_target_score);
        assert_eq!(unknown.project_context_weight, 1.0);
        assert_eq!(unknown.project_paths, Vec::<String>::new());
        assert_eq!(
            unknown.refactor_priority_score,
            unknown.refactor_target_score
        );
    }

    #[test]
    fn refactor_priority_score_clamps_to_score_range() {
        assert_eq!(refactor_priority_score(95, 1.05), 100);
        assert_eq!(refactor_priority_score(60, 0.95), 57);
    }

    #[test]
    fn behavior_rich_module_outranks_low_behavior_vocabulary_module() {
        let mut graph = Graph::new();
        graph.add_unit(type_unit(
            "vocabulary/v.cs::Vocabulary.Term",
            "vocabulary/v.cs",
            &["record"],
        ));

        graph.add_unit(type_unit(
            "workflow/w.cs::Workflow.Runner",
            "workflow/w.cs",
            &["class"],
        ));
        graph.add_unit(method_unit_with_details(
            "workflow/w.cs::Workflow.Runner::Run()",
            "workflow/w.cs",
            180,
            35,
            70,
            &["Dep.One", "Dep.Two", "Dep.Three", "Dep.Four"],
            &["state", "position", "phase"],
        ));

        for i in 0..4 {
            graph.add_unit(type_unit(
                &format!("caller{i}/c.cs::Caller{i}.C"),
                &format!("caller{i}/c.cs"),
                &["class"],
            ));
            graph.add_unit(method_unit(
                &format!("caller{i}/c.cs::Caller{i}.C::Run()"),
                &format!("caller{i}/c.cs"),
            ));
            graph.add_edge(call(
                &format!("caller{i}/c.cs::Caller{i}.C::Run()"),
                "vocabulary/v.cs::Vocabulary.Term",
            ));

            graph.add_unit(type_unit(
                &format!("dep{i}/d.cs::Dep{i}.D"),
                &format!("dep{i}/d.cs"),
                &["class"],
            ));
            graph.add_unit(method_unit(
                &format!("dep{i}/d.cs::Dep{i}.D::Run()"),
                &format!("dep{i}/d.cs"),
            ));
            graph.add_edge(call(
                "workflow/w.cs::Workflow.Runner::Run()",
                &format!("dep{i}/d.cs::Dep{i}.D::Run()"),
            ));
        }

        let metrics = MainSequenceMetrics::compute(&graph);
        let vocabulary = module(&metrics, "vocabulary");
        let workflow = module(&metrics, "workflow");

        assert!(vocabulary.refactor_payoff < workflow.refactor_payoff);
        assert!(
            workflow.refactor_target_score > vocabulary.refactor_target_score,
            "workflow target {} should outrank vocabulary target {}",
            workflow.refactor_target_score,
            vocabulary.refactor_target_score
        );
    }

    #[test]
    fn effort_discount_reduces_score_without_erasing_payoff() {
        let low_effort = refactor_target_score(80, 20);
        let high_effort = refactor_target_score(80, 90);

        assert!(low_effort > high_effort);
        assert!(high_effort > 0);
    }

    #[test]
    fn log_normalization_handles_zero_maxima_and_outliers() {
        assert_eq!(norm_log(0, 0), 0.0);
        assert_eq!(norm_log(10, 0), 0.0);
        assert_eq!(norm_log(10, 10), 1.0);
        assert!(norm_log(10, 1000) > 0.0);
        assert!(norm_log(10, 1000) < 0.5);

        let mut graph = Graph::new();
        graph.add_unit(type_unit(
            "tiny/t.cs::Tiny.T",
            "tiny/t.cs",
            &["class"],
        ));
        let metrics = MainSequenceMetrics::compute(&graph);
        let tiny = module(&metrics, "tiny");
        assert!(tiny.refactor_payoff <= 100);
        assert!(tiny.refactor_effort <= 100);
        assert!(tiny.refactor_target_score <= 100);
    }
}
