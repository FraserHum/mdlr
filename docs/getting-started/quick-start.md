# Quick Start

## Basic Workflow

1. **Create a session** to hold your analysis state:
   ```bash
   mdlr session new my-project
   ```

2. **Add targets** to analyze (directories, files, or specific objects):
   ```bash
   mdlr target add ./src --session my-project
   ```

3. **Run analysis** to extract the graph and compute metrics:
   ```bash
   mdlr analyze --session my-project
   ```

4. **Export the graph** for further processing:
   ```bash
   mdlr export --session my-project --format json
   ```

## Example Session

```bash
# Create a new session
$ mdlr session new demo
Created session 'demo'

# Add the src directory
$ mdlr target add ./src --session demo
Added target './src' to session 'demo'

# Run analysis
$ mdlr analyze --session demo
Analysis for session 'demo'

Graph: 87 units, 36 edges

Structural Metrics
==================

DAG Density: 0.419

Fan-In:  max=4, mean=0.43
Fan-Out: max=6, mean=0.43

Top Fan-Out:
  extract_from_node (6)
  main (4)
  build_graph (3)

Top Fan-In:
  get_node_name (4)
  node_span (4)
  compute (3)

# Clean up when done
$ mdlr session delete demo
Deleted session 'demo'
```

## Targeting Specific Objects

You can target specific code objects using the `file::name` syntax:

```bash
# Analyze only MyStruct and related code
mdlr target add ./src/lib.rs::MyStruct --session my-project
```
