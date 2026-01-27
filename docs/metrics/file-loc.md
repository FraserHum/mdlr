# File Lines of Code

## Definition

File LOC (Lines of Code) measures the total number of lines in each source file by finding the maximum end line of any unit within that file.

## Reported Values

| Metric | Description |
|--------|-------------|
| max | Largest file by line count |
| mean | Average lines per file |
| p90 | 90th percentile file size |
| total | Total lines across all files |
| distribution | List of files sorted by LOC (descending) |

## Interpretation

**High file LOC indicates:**
- Large files that may be hard to navigate
- Potential code organization issues
- Files with too many responsibilities
- Difficulty in code review and maintenance

**Low file LOC indicates:**
- Well-decomposed files
- Focused, single-purpose modules
- Easier to understand and maintain

## Example

```
mdlr check

metric      symbol                value
file_loc    src/resolve/resolve.rs   1119
file_loc    src/main.rs              1062
file_loc    src/extract/rust.rs       780
```

## Guidelines

| File LOC | Interpretation |
|----------|----------------|
| < 200 | Small, focused file |
| 200-500 | Normal size |
| 500-800 | Getting large, consider splitting |
| > 800 | Large file - review for opportunities to decompose |

## What To Do

**Large files should be reviewed for:**
- Multiple unrelated concerns that could be separated
- Helper functions that could move to utility modules
- Types that could be extracted to their own files
- Clear module boundaries within the file

**Consider splitting when:**
- The file has multiple distinct sections
- You find yourself scrolling a lot
- Code review is difficult due to size
- Different parts change independently
