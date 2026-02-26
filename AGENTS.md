# AGENTS.md — meta_skill

> Guidelines for AI coding agents working in this Rust codebase.

---

## RULE 0 - THE FUNDAMENTAL OVERRIDE PREROGATIVE

If I tell you to do something, even if it goes against what follows below, YOU MUST LISTEN TO ME. I AM IN CHARGE, NOT YOU.

---

## RULE NUMBER 1: NO FILE DELETION

**YOU ARE NEVER ALLOWED TO DELETE A FILE WITHOUT EXPRESS PERMISSION.** Even a new file that you yourself created, such as a test code file. You have a horrible track record of deleting critically important files or otherwise throwing away tons of expensive work. As a result, you have permanently lost any and all rights to determine that a file or folder should be deleted.

**YOU MUST ALWAYS ASK AND RECEIVE CLEAR, WRITTEN PERMISSION BEFORE EVER DELETING A FILE OR FOLDER OF ANY KIND.**

---

## RULE 2 - BEADS/BR DATABASE SAFETY (ABSOLUTE)

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

## Irreversible Git & Filesystem Actions — DO NOT EVER BREAK GLASS

1. **Absolutely forbidden commands:** `git reset --hard`, `git clean -fd`, `rm -rf`, or any command that can delete or overwrite code/data must never be run unless the user explicitly provides the exact command and states, in the same message, that they understand and want the irreversible consequences.
2. **No guessing:** If there is any uncertainty about what a command might delete or overwrite, stop immediately and ask the user for specific approval. "I think it's safe" is never acceptable.
3. **Safer alternatives first:** When cleanup or rollbacks are needed, request permission to use non-destructive options (`git status`, `git diff`, `git stash`, copying to backups) before ever considering a destructive command.
4. **Mandatory explicit plan:** Even after explicit user authorization, restate the command verbatim, list exactly what will be affected, and wait for a confirmation that your understanding is correct. Only then may you execute it—if anything remains ambiguous, refuse and escalate.
5. **Document the confirmation:** When running any approved destructive command, record (in the session notes / final response) the exact user text that authorized it, the command actually run, and the execution time. If that record is absent, the operation did not happen.

---

## Git Branch: ONLY Use `main`, NEVER `master`

**The default branch is `main`. The `master` branch exists only for legacy URL compatibility.**

- **All work happens on `main`** — commits, PRs, feature branches all merge to `main`
- **Never reference `master` in code or docs** — if you see `master` anywhere, it's a bug that needs fixing
- **The `master` branch must stay synchronized with `main`** — after pushing to `main`, also push to `master`:
  ```bash
  git push origin main:master
  ```

**If you see `master` referenced anywhere:**
1. Update it to `main`
2. Ensure `master` is synchronized: `git push origin main:master`

---

## Toolchain: Rust & Cargo

We only use **Cargo** in this project, NEVER any other package manager.

- **Edition:** Rust 2024 (nightly required)
- **Minimum Rust version:** 1.85
- **Dependency versions:** Explicit versions for stability
- **Configuration:** Single-crate project (not a workspace)
- **Unsafe code:** Denied (`#[deny(unsafe_code)]`), with `#[allow(unsafe_code)]` permitted in test modules for `env::set_var`/`remove_var`

### Async Runtime: Tokio

This project uses **Tokio** for all async/concurrent operations.

- `tokio` with `rt-multi-thread`, `macros`, `fs`, `io-util`, `time`, `signal` features
- HTTP via `reqwest` (rustls backend, blocking + json features)

### Key Dependencies

| Crate | Purpose |
|-------|---------|
| `clap` + `clap_complete` | CLI argument parsing with derive macros and shell completions |
| `tokio` | Async runtime (multi-threaded) |
| `rusqlite` | SQLite database (bundled, modern_sqlite) |
| `tantivy` | Full-text BM25 search engine |
| `git2` | Git operations (vendored-openssl for musl cross-compilation) |
| `half` + `wide` | IEEE 754 f16 floats and portable SIMD for vector operations |
| `ring` | Cryptographic hashing |
| `ratatui` + `crossterm` | Terminal UI rendering |
| `rich_rust` | Rich terminal output formatting |
| `colored` + `indicatif` + `console` | Terminal colors, progress bars, console utilities |
| `reqwest` | HTTP client (rustls, blocking + json) |
| `rayon` | Data parallelism (CPU-bound work) |
| `serde` + `serde_json` + `serde_yaml` + `toml` | Serialization (JSON, YAML, TOML) |
| `toon_rust` | TOON format support |
| `tracing` + `tracing-subscriber` | Structured logging with env-filter, fmt, ansi, json |
| `thiserror` + `anyhow` | Ergonomic error types and ad-hoc errors |
| `chrono` | Date/time handling |
| `uuid` | UUID v4 generation |
| `regex` + `memchr` | High-performance text processing |
| `unicode-normalization` | NFC text canonicalization |
| `walkdir` + `glob` | Filesystem traversal and glob matching |
| `sha2` + `hex` + `base64` | Hashing and encoding utilities |
| `keyring` | Secure credential storage |
| `insta` | Snapshot testing (dev) |
| `criterion` | Benchmarking (dev) |
| `proptest` | Property-based testing (dev) |
| `httpmock` | HTTP mocking for tests (dev) |
| `rand` + `rand_distr` | Random number generation and distributions |
| `lru` | LRU cache |
| `semver` | Semantic versioning |
| `itertools` | Iterator combinators |

### Release Profile

The release build optimizes for binary size (this is a CLI tool):

```toml
[profile.release]
opt-level = "z"       # Optimize for size
lto = true            # Link-time optimization
codegen-units = 1     # Single codegen unit for better optimization
panic = "abort"       # Abort on panic (smaller binary)
strip = true          # Remove debug symbols
```

Additional profiles:

```toml
[profile.dev]
opt-level = 1         # Some optimization in dev for faster iteration

[profile.profiling]
inherits = "release"
debug = true          # Debug symbols for flamegraph/perf
strip = false         # Keep symbols for profiling
```

---

## Code Editing Discipline

### No Script-Based Changes

**NEVER** run a script that processes/changes code files in this repo. Brittle regex-based transformations create far more problems than they solve.

- **Always make code changes manually**, even when there are many instances
- For many simple changes: use parallel subagents
- For subtle/complex changes: do them methodically yourself

### No File Proliferation

If you want to change something or add a feature, **revise existing code files in place**.

**NEVER** create variations like:
- `mainV2.rs`
- `main_improved.rs`
- `main_enhanced.rs`

New files are reserved for **genuinely new functionality** that makes zero sense to include in any existing file. The bar for creating new files is **incredibly high**.

---

## Backwards Compatibility

We do not care about backwards compatibility—we're in early development with no users. We want to do things the **RIGHT** way with **NO TECH DEBT**.

- Never create "compatibility shims"
- Never create wrapper functions for deprecated APIs
- Just fix the code directly

---

## Compiler Checks (CRITICAL)

**After any substantive code changes, you MUST verify no errors were introduced:**

```bash
# Check for compiler errors and warnings (all targets)
cargo check --all-targets

# Check for clippy lints (pedantic + nursery are enabled)
cargo clippy --all-targets -- -D warnings

# Verify formatting
cargo fmt --check
```

If you see errors, **carefully understand and resolve each issue**. Read sufficient context to fix them the RIGHT way.

---

## Testing

### Testing Policy

This project includes inline `#[cfg(test)]` unit tests alongside the implementation. Tests must cover:
- Happy path
- Edge cases (empty input, max values, boundary conditions)
- Error conditions

Integration tests live in the `tests/` directory. Property-based tests use `proptest`.

### Unit Tests

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run tests for a specific module
cargo test search::
cargo test storage::
cargo test bundler::
cargo test security::
```

### Test Categories

| Directory | Focus Areas |
|-----------|-------------|
| `tests/unit/` | Module-level unit tests |
| `tests/integration/` | Cross-module integration tests |
| `tests/e2e/` | End-to-end CLI tests |
| `tests/properties/` | Property-based tests (proptest) |
| `tests/fixtures/` | Test data and configuration fixtures |
| `tests/snapshots/` | Insta snapshot test expectations |
| `benches/` | Performance benchmarks (criterion) |

---

## Third-Party Library Usage

If you aren't 100% sure how to use a third-party library, **SEARCH ONLINE** to find the latest documentation and current best practices.

---

## meta_skill — This Project

**This is the project you're working on.** `ms` (Meta Skill) is a complete skill management platform — store skills, search them, track their effectiveness, package them for sharing, and integrate them natively with AI agents. Skills can come from hand-written files, CASS session mining, bundles, or guided workflows.

### What It Does

Mines CASS (Cross-Agent Search System) sessions to generate Claude Code skills. Provides hybrid search (BM25 + hash embeddings + RRF), progressive disclosure (minimal to full detail levels), token packing, Thompson Sampling for suggestions, dual persistence (SQLite + Git), MCP server integration, and security scanning (ACIP prompt injection detection, DCG command safety).

### Project Structure

```
meta_skill/
├── Cargo.toml                    # Package manifest
├── src/
│   ├── main.rs                   # Entry point
│   ├── app.rs                    # Application orchestration
│   ├── lib.rs                    # Library root
│   ├── config.rs                 # Configuration (56KB — comprehensive)
│   ├── error.rs.backup           # Error type backup
│   ├── simulation.rs             # Simulation engine (24KB)
│   ├── templates.rs              # Template rendering
│   ├── cli/                      # CLI command definitions and dispatch
│   ├── core/                     # Core types, traits, skill model
│   ├── search/                   # Hybrid search (BM25 + embeddings + RRF)
│   ├── storage/                  # SQLite + Git dual persistence
│   ├── sync/                     # Git synchronization
│   ├── bundler/                  # Skill packaging for sharing
│   ├── cass/                     # CASS session mining and extraction
│   ├── context/                  # Context-aware recommendations
│   ├── suggestions/              # Thompson Sampling suggestion engine
│   ├── security/                 # ACIP injection detection, DCG safety, secret scanning
│   ├── auth/                     # Authentication and keyring
│   ├── tui/                      # Ratatui terminal UI
│   ├── output/                   # Output formatting (JSON, human, rich)
│   ├── meta_skills/              # Meta-skill definitions and management
│   ├── quality/                  # Quality metrics and tracking
│   ├── lint/                     # Skill linting and validation
│   ├── import/                   # Skill import from various sources
│   ├── graph/                    # Dependency graph (delegates to bv)
│   ├── beads/                    # Beads integration
│   ├── agent_detection/          # Coding agent session detection
│   ├── agent_mail/               # MCP Agent Mail integration
│   ├── cm/                       # Cass Memory integration
│   ├── skill_md/                 # Skill markdown parsing
│   ├── dedup/                    # Deduplication
│   ├── antipatterns/             # Anti-pattern detection
│   ├── utils/                    # Shared utilities
│   ├── updater/                  # Self-update mechanism
│   ├── testing/                  # Test helpers
│   └── test_utils/               # Additional test utilities
├── tests/                        # Integration, E2E, property, and snapshot tests
├── benches/                      # Criterion benchmarks
└── .ms/                          # Local skill store and config
```

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
ms build --from-cass "topic"      # Mine sessions -> generate skill
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
| **Progressive Disclosure** | minimal (~100 tokens) to full (variable) based on need |
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

---

## MCP Agent Mail — Multi-Agent Coordination

A mail-like layer that lets coding agents coordinate asynchronously via MCP tools and resources. Provides identities, inbox/outbox, searchable threads, and advisory file reservations with human-auditable artifacts in Git.

### Why It's Useful

- **Prevents conflicts:** Explicit file reservations (leases) for files/globs
- **Token-efficient:** Messages stored in per-project archive, not in context
- **Quick reads:** `resource://inbox/...`, `resource://thread/...`

### Same Repository Workflow

1. **Register identity:**
   ```
   ensure_project(project_key=<abs-path>)
   register_agent(project_key, program, model)
   ```

2. **Reserve files before editing:**
   ```
   file_reservation_paths(project_key, agent_name, ["src/**"], ttl_seconds=3600, exclusive=true)
   ```

3. **Communicate with threads:**
   ```
   send_message(..., thread_id="FEAT-123")
   fetch_inbox(project_key, agent_name)
   acknowledge_message(project_key, agent_name, message_id)
   ```

4. **Quick reads:**
   ```
   resource://inbox/{Agent}?project=<abs-path>&limit=20
   resource://thread/{id}?project=<abs-path>&include_bodies=true
   ```

### Macros vs Granular Tools

- **Prefer macros for speed:** `macro_start_session`, `macro_prepare_thread`, `macro_file_reservation_cycle`, `macro_contact_handshake`
- **Use granular tools for control:** `register_agent`, `file_reservation_paths`, `send_message`, `fetch_inbox`, `acknowledge_message`

### Common Pitfalls

- `"from_agent not registered"`: Always `register_agent` in the correct `project_key` first
- `"FILE_RESERVATION_CONFLICT"`: Adjust patterns, wait for expiry, or use non-exclusive reservation
- **Auth errors:** If JWT+JWKS enabled, include bearer token with matching `kid`

---

## Beads (br) — Dependency-Aware Issue Tracking

Beads provides a lightweight, dependency-aware issue database and CLI (`br` - beads_rust) for selecting "ready work," setting priorities, and tracking status. It complements MCP Agent Mail's messaging and file reservations.

**Important:** `br` is non-invasive—it NEVER runs git commands automatically. You must manually commit changes after `br sync --flush-only`.

### Conventions

- **Single source of truth:** Beads for task status/priority/dependencies; Agent Mail for conversation and audit
- **Shared identifiers:** Use Beads issue ID (e.g., `br-123`) as Mail `thread_id` and prefix subjects with `[br-123]`
- **Reservations:** When starting a task, call `file_reservation_paths()` with the issue ID in `reason`

### Typical Agent Flow

1. **Pick ready work (Beads):**
   ```bash
   br ready --json  # Choose highest priority, no blockers
   ```

2. **Reserve edit surface (Mail):**
   ```
   file_reservation_paths(project_key, agent_name, ["src/**"], ttl_seconds=3600, exclusive=true, reason="br-123")
   ```

3. **Announce start (Mail):**
   ```
   send_message(..., thread_id="br-123", subject="[br-123] Start: <title>", ack_required=true)
   ```

4. **Work and update:** Reply in-thread with progress

5. **Complete and release:**
   ```bash
   br close 123 --reason "Completed"
   br sync --flush-only  # Export to JSONL (no git operations)
   ```
   ```
   release_file_reservations(project_key, agent_name, paths=["src/**"])
   ```
   Final Mail reply: `[br-123] Completed` with summary

### Mapping Cheat Sheet

| Concept | Value |
|---------|-------|
| Mail `thread_id` | `br-###` |
| Mail subject | `[br-###] ...` |
| File reservation `reason` | `br-###` |
| Commit messages | Include `br-###` for traceability |

---

## bv — Graph-Aware Triage Engine

bv is a graph-aware triage engine for Beads projects (`.beads/beads.jsonl`). It computes PageRank, betweenness, critical path, cycles, HITS, eigenvector, and k-core metrics deterministically.

**Scope boundary:** bv handles *what to work on* (triage, priority, planning). For agent-to-agent coordination (messaging, work claiming, file reservations), use MCP Agent Mail.

**CRITICAL: Use ONLY `--robot-*` flags. Bare `bv` launches an interactive TUI that blocks your session.**

### The Workflow: Start With Triage

**`bv --robot-triage` is your single entry point.** It returns:
- `quick_ref`: at-a-glance counts + top 3 picks
- `recommendations`: ranked actionable items with scores, reasons, unblock info
- `quick_wins`: low-effort high-impact items
- `blockers_to_clear`: items that unblock the most downstream work
- `project_health`: status/type/priority distributions, graph metrics
- `commands`: copy-paste shell commands for next steps

```bash
bv --robot-triage        # THE MEGA-COMMAND: start here
bv --robot-next          # Minimal: just the single top pick + claim command
```

### Command Reference

**Planning:**
| Command | Returns |
|---------|---------|
| `--robot-plan` | Parallel execution tracks with `unblocks` lists |
| `--robot-priority` | Priority misalignment detection with confidence |

**Graph Analysis:**
| Command | Returns |
|---------|---------|
| `--robot-insights` | Full metrics: PageRank, betweenness, HITS, eigenvector, critical path, cycles, k-core, articulation points, slack |
| `--robot-label-health` | Per-label health: `health_level`, `velocity_score`, `staleness`, `blocked_count` |
| `--robot-label-flow` | Cross-label dependency: `flow_matrix`, `dependencies`, `bottleneck_labels` |
| `--robot-label-attention [--attention-limit=N]` | Attention-ranked labels |

**History & Change Tracking:**
| Command | Returns |
|---------|---------|
| `--robot-history` | Bead-to-commit correlations |
| `--robot-diff --diff-since <ref>` | Changes since ref: new/closed/modified issues, cycles |

**Other:**
| Command | Returns |
|---------|---------|
| `--robot-burndown <sprint>` | Sprint burndown, scope changes, at-risk items |
| `--robot-forecast <id\|all>` | ETA predictions with dependency-aware scheduling |
| `--robot-alerts` | Stale issues, blocking cascades, priority mismatches |
| `--robot-suggest` | Hygiene: duplicates, missing deps, label suggestions |
| `--robot-graph [--graph-format=json\|dot\|mermaid]` | Dependency graph export |
| `--export-graph <file.html>` | Interactive HTML visualization |

### Scoping & Filtering

```bash
bv --robot-plan --label backend              # Scope to label's subgraph
bv --robot-insights --as-of HEAD~30          # Historical point-in-time
bv --recipe actionable --robot-plan          # Pre-filter: ready to work
bv --recipe high-impact --robot-triage       # Pre-filter: top PageRank
bv --robot-triage --robot-triage-by-track    # Group by parallel work streams
bv --robot-triage --robot-triage-by-label    # Group by domain
```

### Understanding Robot Output

**All robot JSON includes:**
- `data_hash` — Fingerprint of source beads.jsonl
- `status` — Per-metric state: `computed|approx|timeout|skipped` + elapsed ms
- `as_of` / `as_of_commit` — Present when using `--as-of`

**Two-phase analysis:**
- **Phase 1 (instant):** degree, topo sort, density
- **Phase 2 (async, 500ms timeout):** PageRank, betweenness, HITS, eigenvector, cycles

### jq Quick Reference

```bash
bv --robot-triage | jq '.quick_ref'                        # At-a-glance summary
bv --robot-triage | jq '.recommendations[0]'               # Top recommendation
bv --robot-plan | jq '.plan.summary.highest_impact'        # Best unblock target
bv --robot-insights | jq '.status'                         # Check metric readiness
bv --robot-insights | jq '.Cycles'                         # Circular deps (must fix!)
```

---

## UBS — Ultimate Bug Scanner

**Golden Rule:** `ubs <changed-files>` before every commit. Exit 0 = safe. Exit >0 = fix & re-run.

### Commands

```bash
ubs file.rs file2.rs                    # Specific files (< 1s) — USE THIS
ubs $(git diff --name-only --cached)    # Staged files — before commit
ubs --only=rust,toml src/               # Language filter (3-5x faster)
ubs --ci --fail-on-warning .            # CI mode — before PR
ubs .                                   # Whole project (ignores target/, Cargo.lock)
```

### Output Format

```
Warning  Category (N errors)
    file.rs:42:5 - Issue description
    Suggested fix
Exit code: 1
```

Parse: `file:line:col` -> location | Suggested fix -> how to fix | Exit 0/1 -> pass/fail

### Fix Workflow

1. Read finding -> category + fix suggestion
2. Navigate `file:line:col` -> view context
3. Verify real issue (not false positive)
4. Fix root cause (not symptom)
5. Re-run `ubs <file>` -> exit 0
6. Commit

### Bug Severity

- **Critical (always fix):** Memory safety, use-after-free, data races, SQL injection
- **Important (production):** Unwrap panics, resource leaks, overflow checks
- **Contextual (judgment):** TODO/FIXME, println! debugging

---

## RCH — Remote Compilation Helper

RCH offloads `cargo build`, `cargo test`, `cargo clippy`, and other compilation commands to a fleet of 8 remote Contabo VPS workers instead of building locally. This prevents compilation storms from overwhelming csd when many agents run simultaneously.

**RCH is installed at `~/.local/bin/rch` and is hooked into Claude Code's PreToolUse automatically.** Most of the time you don't need to do anything if you are Claude Code — builds are intercepted and offloaded transparently.

To manually offload a build:
```bash
rch exec -- cargo build --release
rch exec -- cargo test
rch exec -- cargo clippy
```

Quick commands:
```bash
rch doctor                    # Health check
rch workers probe --all       # Test connectivity to all 8 workers
rch status                    # Overview of current state
rch queue                     # See active/waiting builds
```

If rch or its workers are unavailable, it fails open — builds run locally as normal.

**Note for Codex/GPT-5.2:** Codex does not have the automatic PreToolUse hook, but you can (and should) still manually offload compute-intensive compilation commands using `rch exec -- <command>`. This avoids local resource contention when multiple agents are building simultaneously.

---

## ast-grep vs ripgrep

**Use `ast-grep` when structure matters.** It parses code and matches AST nodes, ignoring comments/strings, and can **safely rewrite** code.

- Refactors/codemods: rename APIs, change import forms
- Policy checks: enforce patterns across a repo
- Editor/automation: LSP mode, `--json` output

**Use `ripgrep` when text is enough.** Fastest way to grep literals/regex.

- Recon: find strings, TODOs, log lines, config values
- Pre-filter: narrow candidate files before ast-grep

### Rule of Thumb

- Need correctness or **applying changes** -> `ast-grep`
- Need raw speed or **hunting text** -> `rg`
- Often combine: `rg` to shortlist files, then `ast-grep` to match/modify

### Rust Examples

```bash
# Find structured code (ignores comments)
ast-grep run -l Rust -p 'fn $NAME($$$ARGS) -> $RET { $$$BODY }'

# Find all unwrap() calls
ast-grep run -l Rust -p '$EXPR.unwrap()'

# Quick textual hunt
rg -n 'println!' -t rust

# Combine speed + precision
rg -l -t rust 'unwrap\(' | xargs ast-grep run -l Rust -p '$X.unwrap()' --json
```

---

## Morph Warp Grep — AI-Powered Code Search

**Use `mcp__morph-mcp__warp_grep` for exploratory "how does X work?" questions.** An AI agent expands your query, greps the codebase, reads relevant files, and returns precise line ranges with full context.

**Use `ripgrep` for targeted searches.** When you know exactly what you're looking for.

**Use `ast-grep` for structural patterns.** When you need AST precision for matching/rewriting.

### When to Use What

| Scenario | Tool | Why |
|----------|------|-----|
| "How does the search engine work?" | `warp_grep` | Exploratory; don't know where to start |
| "Where is token packing implemented?" | `warp_grep` | Need to understand architecture |
| "Find all uses of `SkillStore::search`" | `ripgrep` | Targeted literal search |
| "Find files with `println!`" | `ripgrep` | Simple pattern |
| "Replace all `unwrap()` with `expect()`" | `ast-grep` | Structural refactor |

### warp_grep Usage

```
mcp__morph-mcp__warp_grep(
  repoPath: "/dp/meta_skill",
  query: "How does the Thompson Sampling suggestion engine work?"
)
```

Returns structured results with file paths, line ranges, and extracted code snippets.

### Anti-Patterns

- **Don't** use `warp_grep` to find a specific function name -> use `ripgrep`
- **Don't** use `ripgrep` to understand "how does X work" -> wastes time with manual reads
- **Don't** use `ripgrep` for codemods -> risks collateral edits

---

## Memory System: cass-memory

The Cass Memory System (cm) is a tool for giving agents an effective memory based on the ability to quickly search across previous coding agent sessions across an array of different coding agent tools (e.g., Claude Code, Codex, Gemini-CLI, Cursor, etc.) and projects (and even across multiple machines, optionally) and then reflect on what they find and learn in new sessions to draw out useful lessons and takeaways; these lessons are then stored and can be queried and retrieved later, much like how human memory works.

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
```

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

**Automation:** `--non-interactive --json` (json->stdout, human->stderr)

**Exit:** 0=ok | 1=partial | 2=conflicts | 3=system | 4=bad args | 5=interrupted (`--resume`)

**Critical:**
- Never create worktrees/clones in projects dir -> use `/tmp/`
- Never parse human output -> use `--json`

---

## Learnings & Troubleshooting

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
- Update status as you work (in_progress -> closed)
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

## Landing the Plane (Session Completion)

**When ending a work session**, you MUST complete ALL steps below.

**MANDATORY WORKFLOW:**

1. **File issues for remaining work** - Create issues for anything that needs follow-up
2. **Run quality gates** (if code changed) - Tests, linters, builds
3. **Update issue status** - Close finished work, update in-progress items
4. **Sync beads** - `br sync --flush-only` to export to JSONL
5. **Hand off** - Provide context for next session

---

## cass — Cross-Agent Session Search

`cass` indexes prior agent conversations (Claude Code, Codex, Cursor, Gemini, ChatGPT, etc.) so we can reuse solved problems.

**Rules:** Never run bare `cass` (TUI). Always use `--robot` or `--json`.

### Examples

```bash
cass health
cass search "async runtime" --robot --limit 5
cass view /path/to/session.jsonl -n 42 --json
cass expand /path/to/session.jsonl -n 42 -C 3 --json
cass capabilities --json
cass robot-docs guide
```

### Tips

- Use `--fields minimal` for lean output
- Filter by agent with `--agent`
- Use `--days N` to limit to recent history

stdout is data-only, stderr is diagnostics; exit code 0 means success.

Treat cass as a way to avoid re-solving problems other agents already handled.

---

## Note for Codex/GPT-5.2

You constantly bother me and stop working with concerned questions that look similar to this:

```
Unexpected changes (need guidance)

- Working tree still shows edits I did not make in Cargo.toml, Cargo.lock, src/main.rs, src/patterns.rs. Please advise whether to keep/commit/revert these before any further work. I did not touch them.

Next steps (pick one)

1. Decide how to handle the unrelated modified files above so we can resume cleanly.
```

NEVER EVER DO THAT AGAIN. The answer is literally ALWAYS the same: those are changes created by the potentially dozen of other agents working on the project at the same time. This is not only a common occurence, it happens multiple times PER MINUTE. The way to deal with it is simple: you NEVER, under ANY CIRCUMSTANCE, stash, revert, overwrite, or otherwise disturb in ANY way the work of other agents. Just treat those changes identically to changes that you yourself made. Just fool yourself into thinking YOU made the changes and simply don't recall it for some reason.

---

## Note on Built-in TODO Functionality

Also, if I ask you to explicitly use your built-in TODO functionality, don't complain about this and say you need to use beads. You can use built-in TODOs if I tell you specifically to do so. Always comply with such orders.
