# CLI Reference

## Global Options

| Option | Description |
|--------|-------------|
| `-h, --help` | Print help |
| `-V, --version` | Print version |

## Commands

### check

Run analysis and display metrics.

```bash
mdlr check [target] [-k <count>] [--pretty] [--format <format>] [-A]
```

| Option | Default | Description |
|--------|---------|-------------|
| `target` | `.` | Path (file/directory) or fully qualified symbol ID to analyze |
| `-k` | `3` | Max opportunities to show per metric (-1 for all) |
| `--pretty` | false | Pretty print as aligned table |
| `--format` | `text` | Output format: `text` or `json` |
| `-A, --all` | false | Analyze all files even when on a branch |

By default, `check` uses **diff mode** on branches (only analyzing files changed since main/master) and analyzes all files when on main/master. Use `-A` to force analyzing all files when on a branch.

Running `check` extracts all files and writes results to the cache.

**Examples:**

```bash
# Analyze (diff mode on branches, all files on main/master)
mdlr check

# Force all files even when on a branch
mdlr check -A

# Analyze specific directory
mdlr check ./src/metrics

# Analyze specific file
mdlr check ./src/main.rs

# Analyze a specific function
mdlr check "src/main.rs::handle_check"

# Analyze a method in an impl block
mdlr check "src/cache/store.rs::impl CacheStore::load_entry"

# Analyze an impl block
mdlr check "src/cache/store.rs::impl CacheStore"

# Show all opportunities (not just top 3)
mdlr check -k -1

# Pretty-printed table output
mdlr check --pretty

# JSON output for scripting
mdlr check --format json
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
