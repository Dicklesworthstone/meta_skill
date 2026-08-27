# Beads (beads_rust) — Repo-Local Issue Tracking

This repo uses `br` (beads_rust) for issue tracking. Issues live in `.beads/`:

- `.beads/beads.db` (SQLite; source of truth)
- `.beads/issues.jsonl` (git-friendly export)

## Quick Start

```bash
# Create
br create "Add user authentication" -t task -p 2

# List / show
br list
br show bd-123

# Work state
br update bd-123 --status in_progress
br close bd-123 --reason "Completed"

# Sync (explicit)
br sync --flush-only   # DB -> issues.jsonl (before git add/commit)
br sync --import-only  # issues.jsonl -> DB (after git pull)
```

## Notes for Agents

- Prefer `--json` output when scripting (`br list --json`, `br ready --json`).
- Use `bv --robot-triage` to pick work (never run bare `bv`).
