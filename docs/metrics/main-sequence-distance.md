# Main Sequence Distance and Refactor Priority Score

`main_sequence_distance` is a C#-only directory module metric. It groups `.cs`
units by parent directory and reports how far each module is from the main
sequence:

```text
D = |A + I - 1|
```

The CLI displays `D * 100`, rounded to percentage points.

`main_sequence_refactor_pressure` remains available as an architecture-pressure
diagnostic. It combines raw main-sequence distance with coupling and
implementation complexity so a large distance-80 zone-of-pain module can rank
ahead of a tiny distance-100 module.

`refactor_target_score` is the unweighted graph-derived target score. It
combines behavior complexity, coordination complexity, architecture pressure,
and estimated effort. It does not use path/name role heuristics or
active-gameplay boosts.

`refactor_priority_score` is the default actionable ranking score shown in text
output. It applies conservative C# project-context weighting to
`refactor_target_score` using guaranteed project facts only: explicitly detected
test projects are discounted, modules in projects reachable from non-test
executable projects are modestly boosted, and unknown modules stay neutral.

## Components

| Value | Meaning |
|-------|---------|
| `A` / abstractness | Abstract C# types divided by all C# types in the module |
| `I` / instability | Efferent coupling divided by total coupling: `Ce / (Ca + Ce)` |
| `Ca` | Count of distinct other C# directory modules that call this module |
| `Ce` | Count of distinct other C# directory modules this module calls |
| `D` / `distance` | Distance from the main sequence, rounded 0-100 |
| `architecture_priority` | Weighted distance, coupling, and type-count score |
| `implementation_complexity` | Weighted cognitive complexity, LOC, and LCOM4 score |
| `refactor_pressure` | Weighted architecture-pressure diagnostic |
| `refactor_payoff` | Weighted behavior complexity, coordination complexity, and architecture pressure |
| `refactor_effort` | Estimated refactor effort from LOC, files, types, methods, and incoming coupling |
| `refactor_target_score` | Effort-adjusted payoff score before project-context weighting |
| `project_context_weight` | Conservative multiplier from guaranteed C# project facts |
| `refactor_priority_score` | Weighted score used by text output |

C# types are classes, interfaces, structs, records, and record structs. Enums
are excluded. Interfaces and abstract classes/records count as abstract types.

## Zones

| Zone | Condition | Interpretation |
|------|-----------|----------------|
| `balanced` | `D < 0.30` | The module is near the main sequence |
| `zone_of_pain` | `D >= 0.30` and `A + I < 1` | Concrete and stable; changes may be costly |
| `zone_of_uselessness` | `D >= 0.30` and `A + I >= 1` | Abstract and unstable; abstractions may not be anchored |

## Output

Text output reports `refactor_priority_score` rows and shows only modules with
at least one cross-module dependency (`Ca + Ce > 0`) so isolated directories do
not become false-positive refactor prompts.

JSON output includes every C# module under `metrics.main_sequence.modules`,
including dependency-free modules, with `abstractness`, `instability`, `ca`,
`ce`, `type_count`, `abstract_type_count`, `distance`, `zone`,
`architecture_priority`, `implementation_complexity`, `refactor_pressure`,
`refactor_payoff`, `refactor_effort`, `refactor_target_score`,
`project_paths`, `explicit_test_project`, `reachable_from_executable`,
`project_context_weight`, and `refactor_priority_score`. It also includes
parallel `distance`, `refactor_pressure`, `refactor_target_score`, and
`refactor_priority_score` distributions.

Set `disabled_metrics: [refactor_priority_score]` to suppress priority text rows
and remove the priority distribution plus priority fields from JSON. Set
`disabled_metrics: [refactor_target_score]` to remove the unweighted target
distribution plus target/payoff/effort fields from JSON. Raw distance and
pressure details remain. Set
`disabled_metrics: [main_sequence_refactor_pressure]` to remove pressure fields
from JSON while leaving target output available.

## Refactor Pressure Formula

Scores are computed against the full project before diff or display filtering.
Weights are fixed in v1.

```text
norm_log(value, max) = if max == 0 then 0 else ln(1 + value) / ln(1 + max)
```

`architecture_priority` uses raw distance, zone-specific coupling, and module
type count. Zone-of-pain modules use `Ca`; zone-of-uselessness modules use
`Ce`; balanced modules use `max(Ca, Ce)`.

```text
architecture_priority =
  round(100 * (
    0.50 * distance / 100 +
    0.35 * norm_log(coupling_basis, max_coupling_basis) +
    0.15 * norm_log(type_count, max_type_count)
  ))
```

`implementation_complexity` uses the largest cognitive complexity in the
module, total module LOC, and the largest `LCOM4 - 1` in the module.

```text
implementation_complexity =
  round(100 * (
    0.50 * norm_log(max_cognitive, max_module_max_cognitive) +
    0.30 * norm_log(total_loc, max_module_total_loc) +
    0.20 * norm_log(max_lcom4 - 1, max_module_lcom4_minus_1)
  ))

refactor_pressure =
  round(100 * (
    0.60 * architecture_priority / 100 +
    0.40 * implementation_complexity / 100
  ))
```

## Refactor Target Score Formula

Scores are computed from graph data only: unit kind, spans, calls, reads/writes,
tags, parameters, branches, max scope, cognitive complexity, parent links, and
module-level `Ca`/`Ce`.

```text
behavior_complexity =
  round(100 * (
    0.40 * norm_log(max_cognitive, max_module_max_cognitive) +
    0.25 * norm_log(total_cognitive, max_module_total_cognitive) +
    0.20 * norm_log(max_scope, max_module_max_scope) +
    0.15 * norm_log(write_count, max_module_write_count)
  ))

coordination_complexity =
  round(100 * (
    0.45 * norm_log(Ce, max_module_ce) +
    0.25 * norm_log(call_count, max_module_call_count) +
    0.20 * norm_log(method_count, max_module_method_count) +
    0.10 * norm_log(file_count, max_module_file_count)
  ))

refactor_payoff =
  round(
    0.45 * behavior_complexity +
    0.30 * coordination_complexity +
    0.25 * refactor_pressure
  )

refactor_effort =
  round(100 * (
    0.35 * norm_log(total_loc, max_module_total_loc) +
    0.20 * norm_log(file_count, max_module_file_count) +
    0.20 * norm_log(type_count, max_module_type_count) +
    0.15 * norm_log(method_count, max_module_method_count) +
    0.10 * norm_log(Ca, max_module_ca)
  ))

refactor_target_score =
  round(refactor_payoff * (1.10 - 0.40 * refactor_effort / 100))
```

## Refactor Priority Score Formula

Project context is based only on C# project metadata emitted by
`mdlr-extract-csharp`. Missing or stale project facts are neutral.

```text
project_context_weight =
  0.95 if all known owning projects are explicit test projects
  1.05 if any known owning project is reachable from a non-test executable project
  1.00 otherwise

refactor_priority_score =
  clamp_0_100(round(refactor_target_score * project_context_weight))
```

Explicit test projects are projects with `IsTestProject=true` or a known test
package reference such as `Microsoft.NET.Test.Sdk`, `xunit`, `xunit.v3`,
`NUnit`, or `MSTest`. Executable reachability starts from non-test
`Exe`/`WinExe` projects and follows project references.

## Thresholds

`main_sequence_distance`, `main_sequence_refactor_pressure`,
`refactor_target_score`, and `refactor_priority_score` use the same 0-100
higher-is-worse buckets.

| Bucket | Score |
|--------|-------|
| excellent | < 15 |
| good | < 30 |
| fair | < 50 |
| poor | < 75 |
| critical | >= 75 |
