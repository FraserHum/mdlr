# Tag Coverage & Conceptual Metrics

Semantic tags enable conceptual analysis of your codebase, measuring not just what code does structurally, but what business domains and architectural layers it belongs to.

## Tag Coverage

Tag coverage measures what percentage of code units have user-defined semantic tags.

```
Tag Coverage = (Units with at least one tag) / (Total units)
```

| Coverage | Description |
|----------|-------------|
| 0% | No tags defined |
| < 25% | Limited tagging - good for experimentation |
| 25-75% | Partial coverage - common during rollout |
| > 75% | High coverage - mature tagging practice |
| 100% | Full coverage |

## Conceptual Metrics

When tags exist, mdlr computes additional metrics to identify architectural issues:

### Conceptual Fan-Out

Measures how many concepts (tags) each unit touches. High conceptual fan-out indicates a function might be doing too much.

```
Conceptual Fan-Out (tags per unit):
  max=3, mean=1.5

  Potential conceptual overload:
    process_order (3 concepts)
    handle_request (2 concepts)
```

**Interpretation:**
- **1 concept**: Focused, single-responsibility
- **2 concepts**: May be a coordinator or boundary function
- **3+ concepts**: Likely doing too much, consider refactoring

### Concept Scattering

Measures how spread out each concept is across files. High scatter indicates a concept is not cohesive.

```
Concept Scattering (high = spread across files):
  domain:auth - 12 units across 8 files (ratio: 0.67)
  domain:billing - 5 units across 1 file (ratio: 0.20)
```

**Interpretation:**
- **Ratio near 0**: Highly cohesive (many units in few files)
- **Ratio near 1**: Highly scattered (each unit in its own file)
- **Ratio > 0.5**: Consider consolidating related code

### Cross-Concept Coupling

Measures edges (calls) that cross between different concepts within the same namespace.

```
Cross-Concept Coupling:
  5/20 edges cross concept boundaries (25.0%)

  domain:
    auth <-> billing (3 edges)
    auth <-> user (2 edges)
```

**Interpretation:**
- **Low ratio (< 20%)**: Clean domain boundaries
- **High ratio (> 50%)**: Concepts are tightly coupled
- **Specific pairs**: Shows which concepts interact most

## Example Analysis

```
Semantic Tags
=============

Coverage: 45.0% (90/200 units tagged)

By Namespace:
  domain: 85 units
    domain:auth (30)
    domain:billing (25)
    domain:core (30)
  layer: 60 units
    layer:api (20)
    layer:service (25)
    layer:data (15)

Conceptual Fan-Out (tags per unit):
  max=3, mean=1.2

  Potential conceptual overload:
    handle_checkout (3 concepts)

Concept Scattering (high = spread across files):
  domain:auth - 30 units across 15 files (ratio: 0.50)

Cross-Concept Coupling:
  12/50 edges cross concept boundaries (24.0%)

  domain:
    auth <-> billing (5 edges)
```

This analysis reveals:
1. `handle_checkout` touches 3 domains - consider splitting
2. Auth code is scattered across 15 files - consider consolidating
3. Auth and billing are coupled via 5 edges - may need an abstraction layer

## Use Cases

### Domain Mapping

Track which business domains are represented in code:

```bash
mdlr tag login --add domain:auth
mdlr tag signup --add domain:auth
mdlr tag process_payment --add domain:billing
```

### Architectural Analysis

Identify layer distribution:

```bash
mdlr tag handle_request --add layer:api
mdlr tag validate_user --add layer:service
mdlr tag query_users --add layer:data
```

### Technical Debt

Mark code that needs attention:

```bash
mdlr tag old_parser --add status:deprecated
mdlr tag new_feature --add status:experimental
```

## Tag Storage

Tags are stored in `.mdlr/tags.json` and persist independently from extracted units. This means:

1. Tags survive source file changes
2. Tags can be version-controlled
3. Tags can be shared across team members

## Related Commands

- `mdlr tag --list` - View all tags
- `mdlr ls` - See tags alongside symbols
- `mdlr analyze` - View tag coverage and conceptual metrics
