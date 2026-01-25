# mdlr - Code Modularity Analyzer

## Quick Start

```bash
# Analyze codebase and show top opportunities per metric
mdlr check

# Analyze specific directory or file
mdlr check src/metrics
mdlr check src/main.rs

# Analyze a specific symbol (function, impl, struct, etc.)
mdlr check "src/main.rs::handle_check"
mdlr check "src/cache/store.rs::impl CacheStore"

# Show more results per metric
mdlr check -k 10

# Pretty print as aligned table
mdlr check --pretty

# List available metrics and their meanings
mdlr metrics
```

## Workflow

1. Run `mdlr check` to identify modularity issues
2. Focus on high-value opportunities (top of each metric)
3. Drill down with `mdlr check <symbol>` to get metrics for a specific unit
4. Refactor to reduce complexity, coupling, and improve cohesion
5. Run `mdlr check --save` to cache results once satisfied

## Key Metrics

- **fan_out**: Dependencies a unit has. High = too many responsibilities
- **fan_in**: Units depending on this. Very high = potential bottleneck
- **function_size**: Lines of code. High = hard to understand/test
- **cyclomatic**: Branch complexity. High = hard to test/maintain
- **lcom**: Lack of cohesion. High = impl block should be split
