# CLI Reference

## Global Options

| Option | Description |
|--------|-------------|
| `-h, --help` | Print help |
| `-V, --version` | Print version |

## Commands

### todo

Show files that need analysis.

```bash
mdlr todo [path] [--all] [--format <format>]
```

| Option | Default | Description |
|--------|---------|-------------|
| `path` | `.` | Directory to check |
| `--all` | false | Also show files with untagged units |
| `--format` | `text` | Output format: `text` or `json` |

**Examples:**

```bash
# Check current directory
mdlr todo

# Check specific directory
mdlr todo ./src

# Include files with untagged units
mdlr todo --all

# JSON output for scripting
mdlr todo --format json
```

---

### analyze

Run analysis on a directory and display metrics.

```bash
mdlr analyze [path] [--force] [--format <format>]
```

| Option | Default | Description |
|--------|---------|-------------|
| `path` | `.` | Directory to analyze |
| `--force` | false | Force re-analysis of all files |
| `--format` | `text` | Output format: `text` or `json` |

**Examples:**

```bash
# Analyze current directory (incremental)
mdlr analyze

# Analyze specific directory
mdlr analyze ./my-project

# Force full re-analysis
mdlr analyze --force

# JSON output for scripting
mdlr analyze --format json
```

---

### export

Export the graph from cached analysis.

```bash
mdlr export [path] [--format <format>]
```

| Option | Default | Description |
|--------|---------|-------------|
| `path` | `.` | Directory to export from |
| `--format` | `json` | Output format: `text` or `json` |

**Examples:**

```bash
# Export as JSON
mdlr export > graph.json

# Export specific directory
mdlr export ./my-project --format json

# Human-readable list
mdlr export --format text
```

---

### ls

List symbols (units) in a file or directory.

```bash
mdlr ls [path] [--kind <kind>] [--format <format>]
```

| Option | Default | Description |
|--------|---------|-------------|
| `path` | `.` | Directory or file to list symbols from |
| `--kind` | - | Filter by unit kind: `function`, `struct`, `trait`, `impl`, `module` |
| `--format` | `text` | Output format: `text` or `json` |

**Examples:**

```bash
# List all symbols in current directory
mdlr ls

# List only functions
mdlr ls --kind function

# List symbols from specific directory
mdlr ls ./src

# JSON output for scripting
mdlr ls --format json
```

**Output columns:**

| Column | Description |
|--------|-------------|
| ID | Unique symbol identifier |
| Kind | Type of unit (Function, Struct, etc.) |
| File | Source file path |
| Start-End | Line number range |
| Tags | Semantic tags (if any) |

---

### get

Get the content of a symbol.

```bash
mdlr get <symbol> [--format <format>]
```

| Option | Default | Description |
|--------|---------|-------------|
| `symbol` | required | Symbol ID to retrieve (from `mdlr ls`) |
| `--format` | `text` | Output format: `text` or `json` |

**Examples:**

```bash
# Get a function's source code
mdlr get compute

# Get symbol as JSON
mdlr get compute --format json
```

---

### tag

Manage semantic tags on symbols.

```bash
mdlr tag <symbol> --add <tag> [--add <tag>...]
mdlr tag <symbol> --remove <tag>
mdlr tag <symbol> --clear
mdlr tag <symbol>
mdlr tag --list [--format <format>]
```

| Option | Default | Description |
|--------|---------|-------------|
| `symbol` | - | Symbol ID to tag (required unless `--list` is used) |
| `--add` | - | Add a tag (can be used multiple times) |
| `--remove` | - | Remove a specific tag |
| `--clear` | false | Remove all tags from the symbol |
| `--list` | false | List all semantic tags in the project |
| `--format` | `text` | Output format: `text` or `json` |

**Tag Format:**

Tags use a namespaced convention: `namespace:value`

| Namespace | Example Values | Use Case |
|-----------|----------------|----------|
| `domain` | `auth`, `billing`, `core` | Business domain categorization |
| `layer` | `api`, `service`, `data` | Architectural layer |
| `complexity` | `high`, `low` | Complexity annotations |
| `status` | `deprecated`, `experimental` | Lifecycle status |

**Examples:**

```bash
# Add a tag
mdlr tag compute --add domain:metrics

# Add multiple tags
mdlr tag compute --add domain:metrics --add layer:core

# Remove a tag
mdlr tag compute --remove layer:core

# Clear all tags
mdlr tag compute --clear

# Show tags for a symbol
mdlr tag compute

# List all tags in project
mdlr tag --list

# List tags as JSON
mdlr tag --list --format json
```

**Notes:**

- Tags are stored in `.mdlr/tags.json` and persist across re-extraction
- Tag coverage metrics are shown in `mdlr analyze` output when tags exist
