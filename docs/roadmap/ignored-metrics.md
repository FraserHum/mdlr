# Ignored Metrics

This document explains metrics that have been intentionally suppressed as false positives or accepted design decisions.

## LCOM (Lack of Cohesion of Methods)

LCOM measures whether methods in a struct share field access. A score of 1.0 means no methods share any fields. However, this metric produces false positives for common Rust patterns.

### Data/Metrics Structs with Constructors

These structs follow the pattern of computing values in a constructor and storing results. The constructor sets fields but doesn't "read" them in the traditional sense, causing LCOM to report low cohesion.

| Symbol | Reason |
|--------|--------|
| `mdlr-metrics::display::MetricThresholds` | Constructor (`default`) sets all fields; `evaluate` reads all fields. LCOM doesn't recognize this as cohesive. |
| `mdlr-metrics::file_loc::FileLocMetrics` | `compute` and `from_counts` are constructors that set fields. No accessor methods exist because fields are public. |
| `mdlr-metrics::tags::ConceptualMetrics` | Metrics struct computed from graph data. Constructor sets fields; fields accessed directly by consumers. |
| `mdlr-metrics::tags::TagMetrics` | Same pattern as `ConceptualMetrics`. |
| `mdlr::config::types::Bucket` | Enum with display implementation. Methods don't share "fields" because it's an enum. |
| `mdlr::config::types::ThresholdsConfig` | Configuration struct loaded from file. Constructor sets fields; fields accessed directly. |

### Builder/Factory Patterns

These structs have multiple constructors or factory methods that don't read fields, combined with a few methods that do the actual work.

| Symbol | Reason |
|--------|--------|
| `mdlr::walk::SourceWalker` | Simple struct with 1 field. `new` constructs, `walk` uses the field. Only 2 methods, so LCOM is 1.0 despite correct design. |
| `mdlr-extract-rust::extractor::RustExtractor` | Multiple constructors (`new`, `discover`, `new_without_context`) plus helper methods. Only `extract_source` and `resolution_context` read the field. |
| `mdlr-extract-rust::resolve::cargo::CargoWorkspace` | Three constructors (`discover`, `from_cargo_files`, `from_manifest`) and two accessors (`find_crate`, `crate_names`). The `root` field is only accessed externally. |

### Service/Store Structs

These structs manage multiple related resources where different methods operate on different subsets of fields by design.

| Symbol | Reason |
|--------|--------|
| `mdlr::cache::store::CacheStore` | Manages cache directory, index file, and tags files. Methods like `load_entry`/`save_entry` use `cache_dir`, while `load_index`/`save_index` use `index_path`. This separation is intentional. |

## Methods Per Struct

| Symbol | Value | Reason |
|--------|-------|--------|
| `mdlr-extract-rust::resolve::resolve::ResolutionContext` | 32 | Methods are already split across 3 files by responsibility (core resolution, call resolution, import resolution). Further splitting would require significant architectural changes. The struct is cohesive (LCOM 0.73) - methods share state appropriately. |

## Recommendations

When evaluating LCOM warnings in the future, consider:

1. **Is it a data struct?** Structs that primarily hold computed data with public fields often have low cohesion scores because their purpose is data storage, not behavior.

2. **Does it have multiple constructors?** Factory patterns naturally have methods that don't share field access.

3. **Is it a service with multiple resources?** Services that manage related but distinct resources (files, caches, connections) may have methods that operate on different subsets of fields.

4. **Are methods already logically grouped?** If methods are split into separate impl blocks across files by responsibility, the design may already be as modular as practical.
