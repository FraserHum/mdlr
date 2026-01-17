# Graph Structure

mdlr builds a directed graph representing the structure and dependencies in your code.

## Units

Units are the nodes in the graph. Each unit represents a code entity.

### Unit Types

| Kind | Description |
|------|-------------|
| `Function` | A function or method |
| `Struct` | A struct definition |
| `Trait` | A trait definition |
| `Impl` | An impl block |
| `Module` | A module (planned) |

### Unit Properties

| Property | Description |
|----------|-------------|
| `id` | Qualified name (e.g., `module::function`) |
| `kind` | Unit type (Function, Struct, etc.) |
| `file` | Source file path |
| `span` | Location in source (line/column) |
| `reads` | Entities consumed (future) |
| `writes` | Entities produced (future) |
| `calls` | Other units invoked |
| `tags` | Domain labels (future) |

### Example Unit (JSON)

```json
{
  "id": "extract::rust::extract_function",
  "kind": "Function",
  "file": "./src/extract/rust.rs",
  "span": {
    "start_line": 107,
    "start_col": 0,
    "end_line": 121,
    "end_col": 1
  },
  "reads": [],
  "writes": [],
  "calls": ["get_node_name", "extract_calls", "node_span"],
  "tags": []
}
```

## Edges

Edges represent relationships between units.

### Edge Types

| Kind | Description |
|------|-------------|
| `Calls` | Function/method invocation |
| `Reads` | Data consumption (future) |
| `Writes` | Data production (future) |

### Edge Properties

| Property | Description |
|----------|-------------|
| `from` | Source unit ID |
| `to` | Target unit ID |
| `kind` | Relationship type |

### Example Edge (JSON)

```json
{
  "from": "extract_function",
  "to": "get_node_name",
  "kind": "Calls"
}
```

## Full Graph Example

```json
{
  "units": [
    {
      "id": "main",
      "kind": "Function",
      "file": "./src/main.rs",
      "span": { "start_line": 10, "start_col": 0, "end_line": 20, "end_col": 1 },
      "reads": [],
      "writes": [],
      "calls": ["handle_session", "handle_target"],
      "tags": []
    },
    {
      "id": "handle_session",
      "kind": "Function",
      "file": "./src/main.rs",
      "span": { "start_line": 22, "start_col": 0, "end_line": 48, "end_col": 1 },
      "reads": [],
      "writes": [],
      "calls": [],
      "tags": []
    }
  ],
  "edges": [
    {
      "from": "main",
      "to": "handle_session",
      "kind": "Calls"
    }
  ]
}
```

## ID Naming Convention

Unit IDs are qualified names based on their location:

| Location | ID Format |
|----------|-----------|
| Top-level function | `function_name` |
| Function in module | `module::function_name` |
| Nested module | `outer::inner::function_name` |
| Impl block | `impl TypeName` or `impl Trait for TypeName` |
