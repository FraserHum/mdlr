# Metrics Overview

mdlr computes structural metrics that help you understand the modularity and coupling characteristics of your codebase.

## Available Metrics

| Metric | Description |
|--------|-------------|
| [DAG Density](dag-density.md) | How connected the dependency graph is relative to a minimal tree |
| [Fan-In](fan-in.md) | How many units depend on each unit |
| [Fan-Out](fan-out.md) | How many units each unit depends on |

## How Metrics Are Computed

1. **Graph extraction**: Source files are parsed using tree-sitter to identify code units (functions, structs, traits, etc.)

2. **Edge detection**: Relationships between units are identified (calls, reads, writes)

3. **Metric computation**: Structural metrics are calculated from the graph

## Using Metrics

Metrics are most useful when:

- **Tracking trends over time**: Is coupling increasing or decreasing?
- **Comparing modules**: Which parts of the codebase are most interconnected?
- **Identifying hotspots**: Which units are critical hubs?
- **Guiding refactoring**: Where should you focus decoupling efforts?

See [Interpreting Results](interpreting-results.md) for guidance on what the numbers mean.
