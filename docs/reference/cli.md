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
mdlr check [target] [--save] [-k <count>] [--pretty] [--format <format>]
```

| Option | Default | Description |
|--------|---------|-------------|
| `target` | `.` | Path (file/directory) or fully qualified symbol ID to analyze |
| `--save` | false | Save extraction results to cache |
| `-k` | `3` | Max opportunities to show per metric (-1 for all) |
| `--pretty` | false | Pretty print as aligned table |
| `--format` | `text` | Output format: `text` or `json` |

By default, `check` is **read-only** and does not modify the cache. This makes it idempotent - running it twice produces the same output. Use `--save` to:
- Persist extraction results to cache
- Commit any staged tag changes

When a filter is specified (path or symbol), `--save` only saves entries matching that filter.

**Examples:**

```bash
# Analyze current directory (read-only)
mdlr check

# Analyze and save results to cache
mdlr check --save

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

**Staging Workflow:**

Tag changes are **staged** rather than immediately committed. This allows you to review changes before persisting them:

1. `mdlr tag <symbol> --add <tag>` stages the addition
2. `mdlr check` shows metrics with staged changes overlaid
3. `mdlr check --save` commits staged changes to the main tags file

```bash
# Stage some tag changes
mdlr tag compute --add domain:metrics
mdlr tag handle_request --add layer:api

# Review with check (shows staged changes)
mdlr check

# Commit when satisfied
mdlr check --save
```

**Storage:**

- Main tags: `.mdlr/tags.json`
- Staged changes: `.mdlr/tags.staged.json` (deleted after commit)
- Tags persist across re-extraction
