# Uncovered Branches

Per-function count of branch records that were never taken during the test run. Derived from `BRDA:` records in an LCOV file passed via `mdlr check --cov`.

## How It's Computed

1. The LCOV file is scanned for `BRDA:line,block,branch,taken` records.
2. For each function/method `Unit`, mdlr counts every `BRDA` record where:
   - `line` falls inside the unit's span,
   - the line is not claimed by a nested unit (same innermost-containing rule as [line coverage](line-coverage.md)),
   - `taken == 0` (or `-`, meaning the branch was never instrumented as taken).
3. `uncov_branches` is the count of those untaken branches per function.

## Gating

This metric is **omitted entirely** when the input LCOV has zero `BRDA:` records anywhere — a single hazard warning is printed in the progress area:

```
  ⚠ lcov has no BRDA records — uncov_branches omitted (re-run coverage with branch instrumentation: c8 --all, coverage run --branch, llvm-cov --branch)
```

A 0-everywhere column would be misleading: it makes branch coverage look perfect when in reality the tool simply isn't measuring it. Better to be silent and tell you why.

## Enabling Branch Coverage

| Tool              | Flag                                                             |
|-------------------|------------------------------------------------------------------|
| coverage.py       | `coverage run --branch`                                          |
| c8 / istanbul     | `c8 --all` (branch coverage is on by default in recent versions) |
| cargo-llvm-cov    | `cargo llvm-cov --branch`                                        |
| gcov / gcov2lcov  | compile with `-fprofile-arcs -ftest-coverage`, then `gcov2lcov`  |

## Sort Direction

`uncov_branches` is `SortDirection::Desc` — **higher is worse**. The distribution is sorted worst-first (largest count).

## Default Thresholds

| Bucket    | Value           |
|-----------|-----------------|
| Excellent | `< 1`           |
| Good      | `1 – 2`         |
| Fair      | `3 – 5`         |
| Poor      | `6 – 9`         |
| Critical  | `>= 10`         |

## Interpreting Results

A function with `line_cov: 100` but `uncov_branches: 5` means every line ran, but five branches always went the same way. Common causes:

- An `else` clause for an error condition you didn't simulate in tests.
- A `match` arm hit only when input has a specific shape your fixtures never produce.
- A short-circuit `||` whose right operand never evaluated.

These are the most valuable rows to investigate — they are gaps your line-level coverage hides.

## Related

- [Line Coverage](line-coverage.md) — paired metric for statement coverage
- [CLI Reference: `--cov`](../reference/cli.md#check)
