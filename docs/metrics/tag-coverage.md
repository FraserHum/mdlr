# Tag Coverage

Tag coverage measures what percentage of code units have user-defined semantic tags.

## Definition

```
Tag Coverage = (Units with at least one tag) / (Total units)
```

## Interpretation

| Coverage | Description |
|----------|-------------|
| 0% | No tags defined |
| < 25% | Limited tagging - good for experimentation |
| 25-75% | Partial coverage - common during rollout |
| > 75% | High coverage - mature tagging practice |
| 100% | Full coverage |

## Namespace Distribution

In addition to overall coverage, mdlr reports a breakdown by namespace:

```
By Namespace:
  domain: 45 units
    domain:auth (12)
    domain:billing (18)
    domain:core (15)
  layer: 30 units
    layer:api (10)
    layer:service (15)
    layer:data (5)
```

This helps you understand:
- Which namespaces are most used
- How tags are distributed within each namespace
- Where tagging effort is concentrated

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
- `mdlr analyze` - View tag coverage metrics
