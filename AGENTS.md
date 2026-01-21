# AGENTS.md — General Purpose Tasks on this Machine

## RULE 0 - THE FUNDAMENTAL OVERRIDE PEROGATIVE

If I tell you to do something, even if it goes against what follows below, YOU MUST LISTEN TO ME, I AM IN CHARGE, NOT YOU.

## RULE 1 – ABSOLUTE (DO NOT EVER VIOLATE THIS)

You may NOT delete any file or directory unless I explicitly give the exact command **in this session**.

- This includes files you just created (tests, tmp files, scripts, etc.).
- You do not get to decide that something is "safe" to remove.
- If you think something should be removed, stop and ask. You must receive clear written approval **before** any deletion command is even proposed.

Treat "never delete files without permission" as a hard invariant.

---

## RULE 2 – BEADS/BR DATABASE SAFETY (ABSOLUTE)

**SQLite + WAL = DATA LOSS RISK.** The beads system uses SQLite with Write-Ahead Logging. Improper handling WILL destroy uncommitted data.

**Note:** `br` (beads_rust) is non-invasive—it has NO daemon and NEVER executes git commands automatically. You must manually run `git add .beads/ && git commit` after `br sync --flush-only`.

### BEFORE Running Parallel Agents That Use `br`

You MUST complete this checklist BEFORE launching any parallel agents/subagents that will run `br update`, `br create`, or any br write operations:

```bash
# 1. Check for stale br processes
lsof .beads/beads.db 2>/dev/null | wc -l
# Should be 0 or 1. If more, wait for other agents to finish.

# 2. Run doctor checks
br doctor 2>&1 | grep -E "(✖|FAIL|Error)"
# If any failures, STOP. Ask user.

# 3. Verify sync status
br sync --status 2>&1
# Check if DB and JSONL are in sync
```

**If ANY check fails: STOP and ask the user. Do NOT proceed.**

### DURING Parallel Agent Work

- **FLUSH AFTER EACH BATCH**: Run `br sync --flush-only` after each agent completes
- **COMMIT PERIODICALLY**: Run `git add .beads/ && git commit` to persist changes
- **Monitor for failures**: If any `br` command fails, STOP ALL AGENTS

### FORBIDDEN ACTIONS (Will Destroy Data)

1. **NEVER kill processes holding `.beads/beads.db`**
   - The WAL may contain uncommitted transactions

2. **NEVER delete or modify these files manually:**
   - `.beads/beads.db`
   - `.beads/beads.db-wal`
   - `.beads/beads.db-shm`

3. **NEVER run `rm .beads/beads.db*` to "fix" issues**

### When `br sync` Fails

**STOP IMMEDIATELY. Ask the user.** Do not attempt to:
- Kill processes
- Delete database files
- Delete WAL files

The correct response is: "br sync is failing with [error]. I need your guidance before proceeding."

### Recovery After Disaster

If data was lost, check these locations for recovery:
- Agent output files: `/tmp/claude/-data-projects-*/tasks/*.output`
- Git history: `.beads/issues.jsonl` is git-tracked and may have recoverable versions

---

### IRREVERSIBLE GIT & FILESYSTEM ACTIONS

Absolutely forbidden unless I give the **exact command and explicit approval** in the same message:

- `git reset --hard`
- `git clean -fd`
- `rm -rf`
- Any command that can delete or overwrite code/data

Rules:

1. If you are not 100% sure what a command will delete, do not propose or run it. Ask first.
2. Prefer safe tools: `git status`, `git diff`, `git stash`, copying to backups, etc.
3. After approval, restate the command verbatim, list what it will affect, and wait for confirmation.
4. When a destructive command is run, record in your response:
   - The exact user text authorizing it
   - The command run
   - When you ran it

If that audit trail is missing, then you must act as if the operation never happened.

---
## ast-grep vs ripgrep

**Use `ast-grep` when structure matters.** It parses code and matches AST nodes, so results ignore comments/strings, understand syntax, and can safely rewrite code.

- Refactors/codemods: rename APIs, change patterns
- Policy checks: enforce patterns across a repo

**Use `ripgrep` when text is enough.** Fastest way to grep literals/regex.

- Recon: find strings, TODOs, config values
- Pre-filter: narrow candidates before precise pass

**Go-specific examples:**

```bash
# Find all error returns without wrapping
ast-grep run -l Go -p 'return err'

# Find all fmt.Println (should use structured logging)
ast-grep run -l Go -p 'fmt.Println($$$)'

# Quick grep for a function name
rg -n 'func.*LoadConfig' -t go

# Combine: find files then match precisely
rg -l -t go 'sync.Mutex' | xargs ast-grep run -l Go -p 'mu.Lock()'
```

---

## Morph Warp Grep — AI-Powered Code Search

**Use `mcp__morph-mcp__warp_grep` for exploratory "how does X work?" questions.** An AI search agent automatically expands your query into multiple search patterns, greps the codebase, reads relevant files, and returns precise line ranges.

**Use `ripgrep` for targeted searches.** When you know exactly what you're looking for.

| Scenario | Tool |
|----------|------|
| "How is graph analysis implemented?" | `warp_grep` |
| "Where is PageRank computed?" | `warp_grep` |
| "Find all uses of `NewAnalyzer`" | `ripgrep` |
| "Rename function across codebase" | `ast-grep` |

**warp_grep usage:**
```
mcp__morph-mcp__warp_grep(
  repoPath: "/path/to/beads_viewer",
  query: "How does the correlation package detect orphan commits?"
)
```

**Anti-patterns:**
- ❌ Using `warp_grep` to find a known function name → use `ripgrep`
- ❌ Using `ripgrep` to understand architecture → use `warp_grep`

---

## UBS Quick Reference

UBS = "Ultimate Bug Scanner" — static analysis for catching bugs early.

**Golden Rule:** `ubs <changed-files>` before every commit. Exit 0 = safe. Exit >0 = fix & re-run.

```bash
ubs file.go file2.go                    # Specific files (< 1s)
ubs $(git diff --name-only --cached)    # Staged files
ubs --only=go pkg/                      # Go files only
ubs .                                   # Whole project
```

**Output Format:**
```
⚠️  Category (N errors)
    file.go:42:5 – Issue description
    💡 Suggested fix
Exit code: 1
```

**Fix Workflow:**
1. Read finding → understand the issue
2. Navigate `file:line:col` → view context
3. Verify real issue (not false positive)
4. Fix root cause
5. Re-run `ubs <file>` → exit 0
6. Commit

**Bug Severity (Go-specific):**
- **Critical**: nil dereference, division by zero, race conditions, resource leaks
- **Important**: error handling, type assertions without check
- **Contextual**: TODO/FIXME, unused variables


---

### cass — Cross-Agent Search

`cass` indexes prior agent conversations (Claude Code, Codex, Cursor, Gemini, ChatGPT, etc.) so we can reuse solved problems.

Rules:

- Never run bare `cass` (TUI). Always use `--robot` or `--json`.

Examples:

```bash
cass health
cass search "authentication error" --robot --limit 5
cass view /path/to/session.jsonl -n 42 --json
cass expand /path/to/session.jsonl -n 42 -C 3 --json
cass capabilities --json
cass robot-docs guide
```

Tips:

- Use `--fields minimal` for lean output.
- Filter by agent with `--agent`.
- Use `--days N` to limit to recent history.

stdout is data-only, stderr is diagnostics; exit code 0 means success.

Treat cass as a way to avoid re-solving problems other agents already handled.

## Learnings & Troubleshooting (Dec 5, 2025)

### Next.js 16 Middleware Deprecation

**CRITICAL**: Next.js 16 deprecates `middleware.ts` in favor of `proxy.ts`.

- The middleware file is now `src/proxy.ts` (NOT `src/middleware.ts`)
- The exported function is `proxy()` (NOT `middleware()`)
- DO NOT restore or recreate `src/middleware.ts` - it will cause deprecation warnings
- If you see both files, delete `middleware.ts` and keep only `proxy.ts`

- **Tooling Issues**:
  - `mcp-agent-mail` CLI is currently missing from the environment path. Cannot register or check mail.
  - `drizzle-kit generate` may fail with `TypeError: sql2.toQuery is not a function` when `pgPolicy` is used with `sql` template literals in the schema file.
- **Workarounds**:
  - If `drizzle-kit generate` fails on `pgPolicy`, remove the policy definitions from `schema.ts` and implement RLS via raw SQL migrations or manual migration files.
  - Always provide `--name` to `drizzle-kit generate` to avoid interactive prompts.

## Landing the Plane (Session Completion)

**When ending a work session**, you MUST complete ALL steps below. Work is NOT complete until `git push` succeeds.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **PUSH TO REMOTE** - This is MANDATORY:
   ```bash
   git pull --rebase
   br sync --flush-only    # Export beads to JSONL (no git ops)
   git add .beads/         # Stage beads changes
   git add <other files>   # Stage code changes
   git commit -m "..."     # Commit everything
   git push
   git status  # MUST show "up to date with origin"
   ```
5. **Clean up** - Clear stashes, prune remote branches
6. **Verify** - All changes committed AND pushed
7. **Hand off** - Provide context for next session

**CRITICAL RULES:**
- Work is NOT complete until `git push` succeeds
- NEVER stop before pushing - that leaves work stranded locally
- NEVER say "ready to push when you are" - YOU must push
- If push fails, resolve and retry until it succeeds

---

## Memory System: cass-memory

The Cass Memory System (cm) is a tool for giving agents an effective memory based on the ability to quickly search across previous coding agent sessions across an array of different coding agent tools (e.g., Claude Code, Codex, Gemini-CLI, Cursor, etc) and projects (and even across multiple machines, optionally) and then reflect on what they find and learn in new sessions to draw out useful lessons and takeaways; these lessons are then stored and can be queried and retrieved later, much like how human memory works.

The `cm onboard` command guides you through analyzing historical sessions and extracting valuable rules.

### Quick Start

```bash
# 1. Check status and see recommendations
cm onboard status

# 2. Get sessions to analyze (filtered by gaps in your playbook)
cm onboard sample --fill-gaps

# 3. Read a session with rich context
cm onboard read /path/to/session.jsonl --template

# 4. Add extracted rules (one at a time or batch)
cm playbook add "Your rule content" --category "debugging"
# Or batch add:
cm playbook add --file rules.json

# 5. Mark session as processed
cm onboard mark-done /path/to/session.jsonl
```

Before starting complex tasks, retrieve relevant context:

```bash
cm context "<task description>" --json
```

This returns:
- **relevantBullets**: Rules that may help with your task
- **antiPatterns**: Pitfalls to avoid
- **historySnippets**: Past sessions that solved similar problems
- **suggestedCassQueries**: Searches for deeper investigation

### Protocol

1. **START**: Run `cm context "<task>" --json` before non-trivial work
2. **WORK**: Reference rule IDs when following them (e.g., "Following b-8f3a2c...")
3. **FEEDBACK**: Leave inline comments when rules help/hurt:
   - `// [cass: helpful b-xyz] - reason`
   - `// [cass: harmful b-xyz] - reason`
4. **END**: Just finish your work. Learning happens automatically.

### Key Flags

| Flag | Purpose |
|------|---------|
| `--json` | Machine-readable JSON output (required!) |
| `--limit N` | Cap number of rules returned |
| `--no-history` | Skip historical snippets for faster response |

stdout = data only, stderr = diagnostics. Exit 0 = success.

---

## xf — X Archive Search

Ultra-fast local search for X (Twitter) data archives. Parses `window.YTD.*` JavaScript format from X data exports. Sub-millisecond full-text search via Tantivy + SQLite storage.

### Core Workflow

```bash
# 1. Index archive (one-time, ~5-30 seconds)
xf index ~/x-archive
xf index ~/x-archive --force          # Rebuild from scratch
xf index ~/x-archive --only tweet,dm  # Index specific types
xf index ~/x-archive --skip grok      # Skip specific types

# 2. Search
xf search "machine learning"          # Search all indexed content
xf search "meeting" --types dm        # DMs only
xf search "rust async" --types tweet  # Tweets only
xf search "article" --types like      # Liked tweets only
xf search "claude" --types grok       # Grok conversations only

Search Syntax

xf search "exact phrase"              # Phrase match (quotes matter)
xf search "rust AND async"            # Boolean AND
xf search "python OR javascript"      # Boolean OR
xf search "python NOT snake"          # Exclusion
xf search "rust*"                     # Wildcard prefix

Key Flags

--format json                         # Machine-readable output (use this!)
--format csv                          # Spreadsheet export
--limit 50                            # Results count (default: 20)
--offset 20                           # Pagination
--context                             # Full DM conversation thread (--types dm only)
--since "2024-01-01"                  # Date filter (supports natural language)
--until "last week"                   # Date filter
--sort date|date_desc|relevance|engagement

Other Commands

xf stats                              # Archive overview (counts, date range)
xf stats --detailed                   # Full analytics (temporal, engagement, content)
xf stats --format json                # Machine-readable stats
xf tweet <id>                         # Show specific tweet by ID
xf tweet <id> --engagement            # Include engagement metrics
xf list tweets --limit 20             # Browse indexed tweets
xf list dms                           # Browse DM conversations
xf doctor                             # Health checks (archive, DB, index)
xf shell                              # Interactive REPL

Data Types

tweet (your posts), like (liked tweets), dm (direct messages), grok (AI chats), follower, following, block, mute

Storage

- Database: ~/.local/share/xf/xf.db (override: XF_DB env)
- Index: ~/.local/share/xf/xf_index/ (override: XF_INDEX env)
- Archive format: Expects data/ directory with tweets.js, like.js, direct-messages.js, etc.

Notes

- First search after restart may be slower (index loading). Subsequent searches <1ms.
- --context only works with --types dm — shows full conversation around matches.
- All data stays local. No network access.

---

## ru Quick Reference for AI Agents

Syncs GitHub repos to local projects directory (clone missing, pull updates, detect conflicts).

```bash
ru sync                    # Sync all repos
ru sync --dry-run          # Preview only
ru sync -j4 --autostash    # Parallel + auto-stash
ru status --no-fetch       # Quick local status
ru list --paths            # Repo paths (stdout)
```

**Automation:** `--non-interactive --json` (json→stdout, human→stderr)

**Exit:** 0=ok | 1=partial | 2=conflicts | 3=system | 4=bad args | 5=interrupted (`--resume`)

**Critical:**
- Never create worktrees/clones in projects dir → use `/tmp/`
- Never parse human output → use `--json`

<!-- bv-agent-instructions-v1 -->

---

## Beads Workflow Integration

This project uses [beads_rust](https://github.com/Dicklesworthstone/beads_rust) (`br`) for issue tracking. Issues are stored in `.beads/` and tracked in git.

**Important:** `br` is non-invasive—it NEVER executes git commands. After `br sync --flush-only`, you must manually run `git add .beads/ && git commit`.

### Essential Commands

```bash
# View issues (launches TUI - avoid in automated sessions)
bv

# CLI commands for agents (use these instead)
br ready              # Show issues ready to work (no blockers)
br list --status=open # All open issues
br show <id>          # Full issue details with dependencies
br create --title="..." --type=task --priority=2
br update <id> --status=in_progress
br close <id> --reason="Completed"
br close <id1> <id2>  # Close multiple issues at once
br sync --flush-only  # Export to JSONL (NO git operations)
```

### Workflow Pattern

1. **Start**: Run `br ready` to find actionable work
2. **Claim**: Use `br update <id> --status=in_progress`
3. **Work**: Implement the task
4. **Complete**: Use `br close <id>`
5. **Sync**: Run `br sync --flush-only` then manually commit

### Key Concepts

- **Dependencies**: Issues can block other issues. `br ready` shows only unblocked work.
- **Priority**: P0=critical, P1=high, P2=medium, P3=low, P4=backlog (use numbers, not words)
- **Types**: task, bug, feature, epic, question, docs
- **Blocking**: `br dep add <issue> <depends-on>` to add dependencies

### Session Protocol

**Before ending any session, run this checklist:**

```bash
git status              # Check what changed
git add <files>         # Stage code changes
br sync --flush-only    # Export beads to JSONL
git add .beads/         # Stage beads changes
git commit -m "..."     # Commit everything together
git push                # Push to remote
```

### Best Practices

- Check `br ready` at session start to find available work
- Update status as you work (in_progress → closed)
- Create new issues with `br create` when you discover tasks
- Use descriptive titles and set appropriate priority/type
- Always `br sync --flush-only && git add .beads/` before ending session

### CRITICAL: Parallel Agent Safety

**READ RULE 2 ABOVE BEFORE RUNNING PARALLEL AGENTS.**

When running multiple agents that use `br update`:

1. **Pre-flight checks are MANDATORY** (see Rule 2 checklist)
2. **Flush and commit after EACH agent completes** - not at the end
3. **If br sync fails: STOP ALL WORK and ask user**

```bash
# WRONG - may lose data if something fails at end
for agent in 1 2 3 4 5 6; do
  run_agent $agent  # Each does br update
done
br sync --flush-only  # If this fails, work not persisted to JSONL

# RIGHT - flush and commit after each batch
run_agent 1
br sync --flush-only && git add .beads/ && git commit -m "Agent 1 work"
run_agent 2
br sync --flush-only && git add .beads/ && git commit -m "Agent 2 work"
# ... etc
```

**The SQLite WAL can hold uncommitted data. Always flush to JSONL and commit to git to persist changes.**

<!-- end-bv-agent-instructions -->

---

## ms — Meta Skill CLI

`ms` is a complete skill management platform—store skills, search them, track their effectiveness, package them for sharing, and integrate them natively with AI agents. Skills can come from hand-written files, CASS session mining, bundles, or guided workflows.

### Core Commands

```bash
ms init                           # Initialize (--global for ~/.ms/)
ms index                          # Index all skills from configured paths
ms index --watch                  # Background file watcher
ms search "query"                 # Hybrid search (BM25 + embeddings + RRF)
ms search --robot                 # JSON output for automation
ms load <skill> --level overview  # Levels: minimal|overview|standard|full|complete
ms load <skill> --pack 2000       # Token-packed slices within budget
ms suggest --cwd .                # Context-aware recommendations
ms build --from-cass "topic"      # Mine sessions → generate skill
ms bundle create my-skills        # Package for sharing
ms doctor                         # Health checks (--fix auto-repairs)
ms sync                           # Git + SQLite dual persistence sync
ms mcp serve                      # MCP server for AI agent integration
ms security scan                  # ACIP prompt injection detection
ms safety check "<command>"       # DCG command safety classification
ms evidence show <skill>          # Provenance tracking
```

### Key Concepts

| Concept | Description |
|---------|-------------|
| **Progressive Disclosure** | minimal (~100 tokens) → full (variable) based on need |
| **Token Packing** | Constrained optimization: slices selected by utility within budget |
| **Skill Layers** | system < global < project < session (higher overrides lower) |
| **Dual Persistence** | SQLite for queries, Git for audit/sync |
| **Robot Mode** | `--robot` flag: stdout=JSON, stderr=diagnostics, exit 0=success |
| **Hash Embeddings** | FNV-1a based, 384 dims, no ML dependency |
| **Thompson Sampling** | Bandit algorithm learns from usage to optimize suggestions |

### Skill Building from CASS

```bash
# Find topics with sufficient sessions
ms coverage --min-sessions 5

# Single-shot extraction
ms build --from-cass "error handling" --since "7 days"

# Mark sessions for skill extraction
ms mark <session> --exemplary --topics "debugging,rust"
ms mark <session> --anti-pattern --reason "wrong approach"
```

### Integration Points

- **CASS**: Source of session transcripts for mining
- **BV/Beads**: `ms graph` delegates to bv for PageRank, betweenness, cycles
- **MCP Server**: `ms mcp serve` for native agent tool-use (search, load, evidence, list, show, doctor)

### Safety Systems

- **ACIP**: Prompt injection detection with trust boundaries and quarantine
- **DCG**: Command safety tiers (Safe/Caution/Danger/Critical)
- **Path Policy**: Symlink escape prevention, path traversal guards
- **Secret Scanner**: Entropy-based detection with automatic redaction
