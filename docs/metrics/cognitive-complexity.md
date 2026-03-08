# Cognitive Complexity

Cognitive complexity measures how difficult a function is to **understand**, using the SonarSource formulation. Unlike cyclomatic complexity which counts independent paths, cognitive complexity penalizes nesting depth — deeply nested code costs more than flat code with the same number of branches.

## How It Differs from Cyclomatic Complexity

| Code pattern | Cyclomatic | Cognitive |
|---|---|---|
| 3 sequential `if` statements | 4 | 3 |
| `if > if > if` (nested) | 4 | 6 |
| `while > for > if > if` | 5 | 14 |

The key insight: cyclomatic complexity treats all branches equally, but nested branches are harder to reason about because you must hold more context in your head.

## Scoring Rules

### Increments (+1 each)

- `if`, `else if`, `else`
- `match` / `switch` (the whole statement, not per-arm)
- `for`, `while`, `loop`
- `catch` / `?` (try operator)
- `&&` / `||` (logical operators)
- `break` / `continue` to a label

### Nesting penalty (+nesting_depth)

Applied in addition to the +1 increment for:

- `if`, `match` / `switch`
- `for`, `while`, `loop`
- Ternary expressions

Closures/lambdas increase the nesting depth for their body but don't add to the score themselves.

### Not counted

- `else` in an `else if` chain (the `if` already counted)
- Desugared match expressions (e.g., `for` loop desugaring)

The cost of a construct = `1 (inherent) + nesting_depth (structural penalty)`.

## Thresholds

| Bucket | Value |
|--------|-------|
| Excellent | < 5 |
| Good | < 10 |
| Fair | < 15 |
| Poor | < 25 |
| Critical | >= 25 |

## Example

```rust
fn process(items: &[Item]) -> Result<()> {     // nesting = 0
    for item in items {                         // +1 +0 = 1, nesting -> 1
        if item.is_valid() {                    // +1 +1 = 2, nesting -> 2
            match item.kind {                   // +1 +2 = 3, nesting -> 3
                Kind::A => {
                    if item.needs_update() {    // +1 +3 = 4, nesting -> 4
                        // ...
                    }
                }
                Kind::B => { /* ... */ }
            }
        } else {                                // +1 (else)
            // ...
        }
    }
    Ok(())
}
// Cognitive complexity: 1 + 2 + 3 + 4 + 1 = 11
// Cyclomatic complexity: 5 (for + if + match(2 arms) + if)
```

## When to Act

High cognitive complexity often indicates:

- Deeply nested control flow that should be flattened (early returns, guard clauses)
- Functions doing too many things (extract sub-functions)
- Complex conditional logic that could be simplified or extracted into named predicates
