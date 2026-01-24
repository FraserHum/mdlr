# mdlr - Code Modularity Analyzer

## Quick Start

```bash
# Analyze codebase and show top opportunities per metric
mdlr check

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
3. Refactor to reduce complexity, coupling, and improve cohesion
4. Re-run `mdlr check` to verify improvements

## Key Metrics

- **fan_out**: Dependencies a unit has. High = too many responsibilities
- **fan_in**: Units depending on this. Very high = potential bottleneck
- **function_size**: Lines of code. High = hard to understand/test
- **cyclomatic**: Branch complexity. High = hard to test/maintain
- **lcom**: Lack of cohesion. High = impl block should be split
