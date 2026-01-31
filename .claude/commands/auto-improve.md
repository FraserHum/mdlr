# Auto-Improve

Use mdlr to identify and improve modularity issues in the codebase.

## mdlr Reference

### Quick Start

```bash
# Analyze codebase and show top opportunities per metric
mdlr check

# Analyze specific directory or file
mdlr check src/metrics
mdlr check src/main.rs

# Analyze a specific symbol by fully qualified crate name
mdlr check "mdlr::handle_check"
mdlr check "mdlr::cache::store::CacheStore"

# Show more results per metric
mdlr check -k 10

# Pretty print as aligned table
mdlr check --pretty

# List available metrics and their meanings
mdlr metrics ls

# Get details about a specific metric including thresholds
mdlr metrics get cyclomatic
```

### Key Metrics

- **fan_out**: Dependencies a unit has. High = too many responsibilities
- **fan_in**: Units depending on this. Very high = potential bottleneck
- **function_size**: Lines of code in a function. High = hard to understand/test
- **file_loc**: Lines of code in a file. High = hard to navigate/maintain
- **cyclomatic**: Branch complexity. High = hard to test/maintain
- **lcom**: Lack of cohesion. High = struct should be split
- **methods_per_struct**: Methods in a struct. High = too many responsibilities

## Steps

1. Run `mdlr check` to identify modularity issues
2. Focus on high-value opportunities (top of each metric)
3. Drill down with `mdlr check <symbol>` to get metrics for a specific unit
4. Create a plan and consider alternatives before making changes
5. Follow the plan to make the suggested improvements to the codebase
6. Ensure all existing tests continue to pass by running `cargo test`
7. Update or add tests as needed to cover your changes
8. If you add a new metric, CLI command, or language support, update the relevant documentation as specified in CLAUDE.md

## Important: Consider Architecture

When evaluating alternatives, consider the **big picture architecture** of the codebase. A function or file may be large or complex due to deliberate architectural decisions, not just organic growth.

Before proposing to split a large function or restructure code:

- **Understand why it's structured this way** - Is there a design pattern, performance reason, or domain constraint?
- **Evaluate if splitting helps or hurts** - Sometimes a large function that does one thing coherently is better than scattered pieces
- **Consider downstream impact** - Will this change ripple through the codebase in unexpected ways?
- **Look for root causes** - A high metric might be a symptom of a deeper architectural issue that splitting won't solve

The goal is improved modularity, not just lower numbers. A refactor that fragments related logic or introduces unnecessary indirection is worse than leaving well-structured but large code alone.
