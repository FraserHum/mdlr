# Fan-In

## Definition

Fan-in is the number of incoming edges to a unit - how many other units depend on or call it.

## Reported Values

| Metric | Description |
|--------|-------------|
| max | Highest fan-in of any single unit |
| mean | Average fan-in across all units |
| distribution | List of units sorted by fan-in (top 10 shown) |

## Interpretation

**High fan-in indicates:**
- The unit is widely used (high reuse)
- Changes to this unit affect many dependents
- This is a critical/core piece of the codebase
- Extra care needed when modifying - consider stability

**Low fan-in indicates:**
- The unit has few or no dependents
- May be a leaf node, entry point, or potentially dead code
- Changes have limited blast radius

## Example

A utility function `get_node_name` with fan-in of 4 is called from 4 different places. Changing its signature requires updating all 4 call sites.

```
extract_function ──→ get_node_name ←── extract_struct
                           ↑
extract_trait ─────────────┘
                           ↑
extract_impl ──────────────┘
```

## Guidelines

| Fan-In | Interpretation |
|--------|----------------|
| 0 | Entry point, dead code, or test-only |
| 1-3 | Normal, limited usage |
| 4-10 | Moderate reuse, somewhat critical |
| > 10 | High reuse, very critical - treat as stable API |

## What To Do

**High fan-in units should:**
- Have stable interfaces (avoid breaking changes)
- Be well-tested
- Be documented
- Have clear contracts

**Zero fan-in units might be:**
- Entry points (main, handlers) - expected
- Dead code - consider removing
- Test utilities - expected
