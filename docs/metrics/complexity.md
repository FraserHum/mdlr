# Complexity Metrics

Complexity metrics measure how complex individual functions are, helping identify functions that may be doing too much.

## Metrics

### Function Size

Lines of code per function, computed from the span (start line to end line).

| Statistic | Description |
|-----------|-------------|
| max | Largest function in lines |
| mean | Average function size |
| p90 | 90th percentile size |

**Default thresholds:**

| Bucket | Threshold |
|--------|-----------|
| Excellent | < 20 lines |
| Good | < 50 lines |
| Fair | < 100 lines |
| Poor | < 200 lines |
| Critical | >= 200 lines |

### Parameter Count

Number of parameters per function. Self/&self/&mut self parameters are not counted.

| Statistic | Description |
|-----------|-------------|
| max | Most parameters on any function |
| mean | Average parameter count |

**Default thresholds:**

| Bucket | Threshold |
|--------|-----------|
| Excellent | < 3 params |
| Good | < 5 params |
| Fair | < 7 params |
| Poor | < 10 params |
| Critical | >= 10 params |

### Cyclomatic Complexity

Measures the number of independent paths through a function. Higher values indicate more complex control flow.

Counts:
- `if` expressions (+1 each)
- `match` arms (+1 per arm beyond the first)
- `while`, `for`, `loop` expressions (+1 each)
- `&&` and `||` operators (+1 each)

| Statistic | Description |
|-----------|-------------|
| max | Highest complexity in any function |
| mean | Average complexity |
| p90 | 90th percentile complexity |

**Default thresholds:**

| Bucket | Threshold |
|--------|-----------|
| Excellent | < 5 |
| Good | < 10 |
| Fair | < 20 |
| Poor | < 30 |
| Critical | >= 30 |

### Max Scope Lines

Measures the largest single scope block within each function. While `function_size` measures the overall length, `max_scope` catches functions where a single block (if body, match arm, loop body, closure) dominates — suggesting that block should be extracted into its own function.

| Statistic | Description |
|-----------|-------------|
| max | Largest scope block across all functions |
| mean | Average max scope size |
| p90 | 90th percentile max scope size |

**Default thresholds:**

| Bucket | Threshold |
|--------|-----------|
| Excellent | < 15 lines |
| Good | < 30 lines |
| Fair | < 50 lines |
| Poor | < 100 lines |
| Critical | >= 100 lines |

## Example Output

```
Complexity Metrics
==================

Function Size: max=294 lines, mean=17.3, p90=28
Parameters:    max=6, mean=0.9
Cyclomatic:    max=22, mean=2.5, p90=5

Most Complex Functions:
  handle_check (cc=22, lines=294, params=3)
  handle_tag (cc=17, lines=108, params=6)
  count_branches_recursive (cc=14, lines=41, params=2)

Largest Functions:
  handle_check (294 lines)
  handle_tag (108 lines)
```

## Interpretation

- **High function size**: Consider breaking into smaller, focused functions
- **Many parameters**: Consider using a struct/builder pattern
- **High cyclomatic complexity**: Consider extracting conditional logic into separate functions
- **High max scope**: Extract the oversized block into a separate function

## Configuration

```yaml
thresholds:
  function_size:
    excellent: 20
    good: 50
    fair: 100
    poor: 200

  params:
    excellent: 3
    good: 5
    fair: 7
    poor: 10

  cyclomatic:
    excellent: 5
    good: 10
    fair: 20
    poor: 30

  max_scope:
    excellent: 15
    good: 30
    fair: 50
    poor: 100
```
