# CLI Reference

## Global Options

| Option | Description |
|--------|-------------|
| `-h, --help` | Print help |
| `-V, --version` | Print version |

## Commands

### session

Manage analysis sessions.

#### `session new <name>`

Create a new analysis session.

```bash
mdlr session new my-project
```

#### `session list`

List all sessions.

```bash
mdlr session list
```

#### `session show <name>`

Show session details including targets and graph size.

```bash
mdlr session show my-project
```

#### `session delete <name>`

Delete a session.

```bash
mdlr session delete my-project
```

---

### target

Manage analysis targets within a session.

#### `target add <path> --session <name>`

Add a target to analyze. Targets can be:
- Directories (recursively analyzed)
- Files
- Specific objects using `file::name` syntax

```bash
# Add a directory
mdlr target add ./src --session my-project

# Add a specific file
mdlr target add ./src/main.rs --session my-project

# Add a specific object
mdlr target add ./src/lib.rs::MyStruct --session my-project
```

#### `target list --session <name>`

List all targets in a session.

```bash
mdlr target list --session my-project
```

#### `target clear --session <name>`

Remove all targets from a session.

```bash
mdlr target clear --session my-project
```

---

### analyze

Run analysis on a session and display metrics.

```bash
mdlr analyze --session <name> [--format <format>]
```

| Option | Default | Description |
|--------|---------|-------------|
| `--session` | required | Session name |
| `--format` | `text` | Output format: `text` or `json` |

**Examples:**

```bash
# Human-readable output
mdlr analyze --session my-project

# JSON output for scripting
mdlr analyze --session my-project --format json
```

---

### export

Export the graph from a session.

```bash
mdlr export --session <name> [--format <format>]
```

| Option | Default | Description |
|--------|---------|-------------|
| `--session` | required | Session name |
| `--format` | `json` | Output format: `text` or `json` |

**Examples:**

```bash
# Export as JSON
mdlr export --session my-project --format json > graph.json

# Human-readable list
mdlr export --session my-project --format text
```
