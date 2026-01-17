# Session Storage

Sessions persist analysis state between CLI invocations, enabling incremental workflows.

## What's Stored

Each session contains:

| Field | Description |
|-------|-------------|
| `id` | Session name |
| `created_at` | Creation timestamp |
| `updated_at` | Last modification timestamp |
| `targets` | List of analysis targets |
| `graph` | Extracted graph (units and edges) |

## Storage Location

Sessions are stored as JSON files in the system cache directory:

| Platform | Path |
|----------|------|
| macOS | `~/Library/Caches/mdlr/sessions/` |
| Linux | `~/.cache/mdlr/sessions/` |
| Windows | `%LOCALAPPDATA%/mdlr/sessions/` |

Each session is a separate file: `{session_id}.json`

## Session Lifecycle

```bash
# Create
mdlr session new my-project
# → Creates ~/.cache/mdlr/sessions/my-project.json

# Add targets
mdlr target add ./src --session my-project
# → Updates targets in session file

# Analyze (updates graph)
mdlr analyze --session my-project
# → Parses targets, stores graph in session file

# Delete
mdlr session delete my-project
# → Removes session file
```

## Manual Access

Sessions are plain JSON and can be inspected directly:

```bash
cat ~/.cache/mdlr/sessions/my-project.json | jq .
```

## Backup and Sharing

To share a session:

```bash
# Export
cp ~/.cache/mdlr/sessions/my-project.json ./my-project-session.json

# Import (on another machine)
cp ./my-project-session.json ~/.cache/mdlr/sessions/my-project.json
```

Note: File paths in targets are stored as-is. If sharing between machines, relative paths work better than absolute paths.
