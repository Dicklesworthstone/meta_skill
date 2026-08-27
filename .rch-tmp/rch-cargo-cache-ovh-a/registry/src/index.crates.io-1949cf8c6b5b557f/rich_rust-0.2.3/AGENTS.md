# AGENTS.md — rich_rust

> Guidelines for AI coding agents working in this Rust codebase.

---

## RULE 0 - THE FUNDAMENTAL OVERRIDE PREROGATIVE

If I tell you to do something, even if it goes against what follows below, YOU MUST LISTEN TO ME. I AM IN CHARGE, NOT YOU.

---

## RULE NUMBER 1: NO FILE DELETION

**YOU ARE NEVER ALLOWED TO DELETE A FILE WITHOUT EXPRESS PERMISSION.** Even a new file that you yourself created, such as a test code file. You have a horrible track record of deleting critically important files or otherwise throwing away tons of expensive work. As a result, you have permanently lost any and all rights to determine that a file or folder should be deleted.

**YOU MUST ALWAYS ASK AND RECEIVE CLEAR, WRITTEN PERMISSION BEFORE EVER DELETING A FILE OR FOLDER OF ANY KIND.**

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

- **Edition:** Rust 2024 (nightly required — see `rust-toolchain.toml`)
- **Dependency versions:** Explicit versions for stability
- **Configuration:** Cargo.toml (single crate, not a workspace)
- **Unsafe code:** Forbidden (`#![forbid(unsafe_code)]`)

### Key Dependencies

| Crate | Purpose |
|-------|---------|
| `crossterm` | Terminal manipulation and capability detection |
| `unicode-width` | Correct calculation of text width (CJK, emoji) |
| `bitflags` | Efficient text attribute management via bitflags |
| `regex` + `fancy-regex` | Markup parsing and Rich-parity highlighters (look-around) |
| `lru` | LRU caching for style and ANSI code generation |
| `smallvec` | Stack-allocated small vectors for performance |
| `once_cell` | Lazy static initialization |
| `num-rational` | Exact fraction arithmetic for ratio distribution |
| `os_pipe` + `stdio-override` | Cross-platform stdout/stderr redirection for Live displays |
| `log` + `time` | Logging integration with time formatting |
| `syntect` | Syntax highlighting (optional `syntax` feature) |
| `pulldown-cmark` | Markdown rendering (optional `markdown` feature) |
| `serde` + `serde_json` | JSON rendering (optional `json` feature) |
| `backtrace` | Automatic traceback rendering (optional `backtrace` feature) |
| `tracing` + `tracing-subscriber` | Tracing integration (optional `tracing` feature) |

### Release Profile

The release build optimizes for binary size:

```toml
[profile.release]
opt-level = "z"     # Optimize for size
lto = true          # Link-time optimization
codegen-units = 1   # Single codegen unit for better optimization
panic = "abort"     # Smaller binary, no unwinding overhead
strip = true        # Remove debug symbols
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

Every module includes inline `#[cfg(test)]` unit tests alongside the implementation. Tests must cover:
- Happy path
- Edge cases (empty input, max values, boundary conditions)
- Error conditions

Integration and end-to-end tests live in the `tests/` directory. Property-based fuzz tests use `proptest`.

### Unit Tests

```bash
# Run all tests
cargo test

# Run with output
cargo test -- --nocapture

# Run tests with all features enabled
cargo test --all-features

# Run specific test module
cargo test --test e2e_table
cargo test --test property_tests
cargo test --test conformance_test --features conformance_test
```

### Test Categories

| Location | Focus Areas |
|----------|-------------|
| `src/**` (inline) | Per-module unit tests for style, color, segment, text, console, measure, markup, box, theme, highlighter, cells, ansi |
| `tests/e2e_*` | End-to-end rendering: tables, panels, markdown, syntax, JSON, text, styles, hyperlinks, width algorithms, live displays, interactive, input limits, export |
| `tests/property_tests.rs` | proptest-based fuzzing for style parsing, color round-trips, markup, text operations |
| `tests/fuzz_*.rs` | Fuzz tests for markup parser and style parser edge cases |
| `tests/conformance_*.rs` | Python Rich parity conformance tests against reference fixtures |
| `tests/golden_test.rs` | Golden file snapshot tests for visual regression detection |
| `tests/regression_tests.rs` | Targeted regression tests for previously-discovered bugs |
| `tests/thread_safety.rs` | Concurrent access, mutex poison recovery, `Send + Sync` verification |
| `tests/demo_showcase_*.rs` | Showcase and demo harness tests for all renderable components |
| `benches/render_bench.rs` | Criterion benchmarks for rendering throughput |

### Test Fixtures

- `tests/golden/` — Golden file snapshots for visual regression testing
- `tests/snapshots/` — insta snapshot files for renderable output
- `tests/conformance/` — Python Rich reference fixtures for parity testing
- `tests/common/` — Shared test utilities
- `tests/perf_baselines.json` — Performance baseline data

### Examples

The `examples/` directory contains usage examples that also serve as visual verification:

```bash
cargo run --example basic
cargo run --example tables
cargo run --example tree
cargo run --example progress
cargo run --example text_styling
cargo run --example phase2_demo
```

---

## Third-Party Library Usage

If you aren't 100% sure how to use a third-party library, **SEARCH ONLINE** to find the latest documentation and current best practices.

---

## rich_rust — This Project

**This is the project you're working on.** `rich_rust` is a Rust port of Python's `rich` library for beautiful terminal output. It provides styled text, tables, panels, progress bars, trees, syntax highlighting, markdown rendering, and more with an ergonomic API.

### What It Does

Renders rich, styled terminal output with automatic color detection, markup parsing, and a full suite of high-level renderables (tables, panels, trees, progress bars, syntax, markdown, JSON). A single `Console` struct coordinates all rendering, export (HTML/SVG), and I/O.

### Architecture

```
Markup String ─→ Parser ─→ Text (styled spans) ─→ Segment[] ─→ Console ─→ ANSI Output
                                                       ▲
Renderable (Table, Panel, Tree, etc.) ────────────────┘

Console
├── Color detection (4-bit, 8-bit, TrueColor, auto-downgrade)
├── Terminal size detection
├── Theme + Style resolution
├── Highlighter pipeline
├── Export: HTML, SVG
└── Live display / Interactive (Status, Prompt, Pager)
```

- **Console:** Central coordinator for options, rendering, export, and I/O.
- **Renderables:** High-level components (Table, Panel, Tree, etc.) that implement the `Renderable` trait.
- **Segments:** Atomic units of styled text — the universal rendering currency.
- **Text:** Rich text with overlapping style spans, wrapping, justification, and highlighting.
- **Style/Color:** Full CSS-like styling with automatic terminal color downgrading.
- **Measurement:** Protocol for calculating width requirements.

### Project Structure

```
rich_rust/
├── Cargo.toml                          # Single-crate package (not a workspace)
├── rust-toolchain.toml                 # Nightly Rust 2024
├── src/
│   ├── lib.rs                          # Crate root, module exports, prelude
│   ├── console.rs                      # Console struct — central entry point
│   ├── style.rs                        # Style, Attributes, markup-based styling
│   ├── color.rs                        # Color types (4-bit, 8-bit, TrueColor), themes
│   ├── text.rs                         # Text with styled spans, wrapping, justification
│   ├── segment.rs                      # Atomic rendering unit (text + style)
│   ├── ansi.rs                         # ANSI escape code decoding
│   ├── box.rs                          # Box-drawing characters
│   ├── cells.rs                        # Unicode cell width calculation
│   ├── measure.rs                      # Width measurement protocol
│   ├── markup/                         # Markup parser ([bold red]...[/])
│   ├── theme.rs                        # Theme system (named styles)
│   ├── highlighter.rs                  # Regex-based text highlighting
│   ├── emoji.rs                        # Emoji lookup and replacement
│   ├── filesize.rs                     # Human-readable file size formatting
│   ├── terminal.rs                     # Terminal detection and capabilities
│   ├── protocol.rs                     # RichCast trait (Python __rich__ parity)
│   ├── sync.rs                         # Mutex poison recovery utilities
│   ├── logging.rs                      # RichLogger + RichTracingLayer
│   ├── live.rs                         # Live display (auto-refresh)
│   ├── interactive.rs                  # Status spinners, Prompt, Pager
│   └── renderables/
│       ├── mod.rs                      # Renderable trait + re-exports
│       ├── table.rs                    # Table, Column, Row, Cell
│       ├── panel.rs                    # Panel (boxed content with title)
│       ├── tree.rs                     # Tree (hierarchical guide lines)
│       ├── progress.rs                 # ProgressBar, Spinner, download columns
│       ├── rule.rs                     # Horizontal divider Rule
│       ├── columns.rs                  # Multi-column text layout
│       ├── align.rs                    # Text alignment utilities
│       ├── layout.rs                   # Layout splitter (ratio/fixed regions)
│       ├── group.rs                    # Combine multiple renderables
│       ├── padding.rs                  # Padding dimensions
│       ├── constrain.rs               # Width constraining wrapper
│       ├── control.rs                  # Terminal control codes
│       ├── emoji.rs                    # Single emoji renderable
│       ├── pretty.rs                   # Pretty-print / Inspect
│       ├── traceback.rs               # Stack trace rendering
│       ├── syntax.rs                   # Syntax highlighting (feature: syntax)
│       ├── markdown.rs                 # Markdown rendering (feature: markdown)
│       └── json.rs                     # JSON rendering (feature: json)
├── examples/                           # Usage examples (basic, tables, tree, etc.)
├── tests/                              # Integration, e2e, property, fuzz, conformance tests
├── benches/                            # Criterion benchmarks
└── src/default_styles.tsv              # Default theme style definitions
```

### Key Files

| File | Purpose |
|------|---------|
| `src/lib.rs` | Crate root, module exports, `prelude` module |
| `src/console.rs` | `Console` struct — central entry point for printing, logging, export (HTML/SVG), Live displays |
| `src/style.rs` | `Style`, `Attributes` — CSS-like styling with bold, italic, colors, hyperlinks |
| `src/color.rs` | `Color`, `ColorSystem`, `ColorTriplet`, `TerminalTheme` — 4/8/24-bit color with auto-downgrade |
| `src/text.rs` | `Text`, `Span` — Rich text with overlapping spans, wrapping, justification, truncation |
| `src/segment.rs` | `Segment`, `ControlCode` — atomic rendering unit (text + style) |
| `src/cells.rs` | Unicode cell width calculation (CJK, emoji, zero-width) |
| `src/measure.rs` | `Measurement` — width measurement protocol for renderables |
| `src/markup/` | Markup parser (`[bold red]...[/]`) |
| `src/theme.rs` | `Theme` — named style registry with stack-based scoping |
| `src/highlighter.rs` | `Highlighter`, `RegexHighlighter`, `ReprHighlighter` — automatic text highlighting |
| `src/live.rs` | `Live` — auto-refreshing live display with transient/permanent rendering |
| `src/interactive.rs` | `Status` (spinners), `Prompt`, `Pager` |
| `src/logging.rs` | `RichLogger`, `RichTracingLayer` — rich-formatted logging |
| `src/renderables/table.rs` | `Table`, `Column`, `Row`, `Cell` — auto-sizing columns, borders, formatting |
| `src/renderables/panel.rs` | `Panel` — boxed content with title/subtitle |
| `src/renderables/tree.rs` | `Tree`, `TreeNode`, `TreeGuides` — hierarchical data with guide lines |
| `src/renderables/progress.rs` | `ProgressBar`, `Spinner` — visual progress indicators |
| `src/renderables/layout.rs` | `Layout`, `LayoutSplitter` — ratio/fixed region splitting |

### Feature Flags

```toml
[features]
default = []
syntax = ["syntect"]                              # Syntax highlighting via syntect
markdown = ["pulldown-cmark"]                     # Markdown rendering via pulldown-cmark
json = ["serde_json", "serde"]                    # JSON formatting with syntax highlighting
tracing = ["dep:tracing", "dep:tracing-subscriber"]  # Tracing integration
backtrace = ["dep:backtrace"]                     # Automatic traceback rendering
full = ["syntax", "markdown", "json", "backtrace"]   # All rendering features
conformance_test = ["full"]                       # Enable conformance test suite
showcase = ["full", "tracing"]                    # Full features + tracing for demos
```

### Core Types Quick Reference

| Type | Purpose |
|------|---------|
| `Console` | Central entry point — printing, logging, export (HTML/SVG), Live, color detection |
| `Style` | Visual attributes: fg/bg color, bold, italic, underline, strikethrough, hyperlinks |
| `Color` | Terminal color: 4-bit ANSI, 8-bit (256), 24-bit TrueColor with auto-downgrade |
| `ColorSystem` | Color capability level: `Standard` (4-bit), `EightBit`, `TrueColor`, `Windows` |
| `Text` | Rich text with overlapping `Span`s, wrapping, justification, truncation, highlight |
| `Span` | A style applied to a character range within `Text` |
| `Segment` | Atomic rendering unit: text content + optional `Style` |
| `Renderable` | Core trait — `fn render(&self, console, options) -> Vec<Segment>` |
| `Measurement` | Min/max width measurement for renderable layout |
| `Theme` | Named style registry with push/pop stack scoping |
| `Table` | Auto-sizing table with columns, rows, cells, borders, and formatting |
| `Panel` | Boxed content with optional title/subtitle and border style |
| `Tree` / `TreeNode` | Hierarchical tree display with configurable guide characters |
| `ProgressBar` / `Spinner` | Visual progress indicators with customizable bar styles |
| `Layout` / `LayoutSplitter` | Ratio-based and fixed-width region splitting |
| `Live` | Auto-refreshing live display (transient rendering, thread-safe updates) |
| `Status` | Spinner-based status indicator for long-running operations |
| `Highlighter` | Trait for automatic text highlighting (regex-based, repr, null) |
| `RichLogger` | `log` crate integration with rich formatting |

### Key Design Decisions

- **Segment-based rendering pipeline** — all renderables produce `Vec<Segment>`, which Console converts to ANSI
- **Automatic color downgrading** — TrueColor styles gracefully degrade to 256-color or 4-bit based on terminal capabilities
- **Markup parser** with `[bold red]...[/]` syntax for inline styling (full Python Rich parity)
- **Mutex poison recovery** — all internal mutexes use poison recovery so threads survive panics in other threads
- **Thread-safe Console** — `Console` is `Send + Sync`; safe to share via `Arc<Console>` across threads
- **LRU caching** for style parsing and ANSI code generation to avoid repeated computation
- **`#![forbid(unsafe_code)]`** — zero unsafe code in the entire crate
- **Renderable trait** — extensible: implement `Renderable` to create custom components
- **Python Rich conformance testing** — fixtures-based tests verify output parity with the Python library
- **Optional features** — heavy dependencies (syntect, pulldown-cmark, serde) are feature-gated to keep the default build lean

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
⚠️  Category (N errors)
    file.rs:42:5 – Issue description
    💡 Suggested fix
Exit code: 1
```

Parse: `file:line:col` → location | 💡 → how to fix | Exit 0/1 → pass/fail

### Fix Workflow

1. Read finding → category + fix suggestion
2. Navigate `file:line:col` → view context
3. Verify real issue (not false positive)
4. Fix root cause (not symptom)
5. Re-run `ubs <file>` → exit 0
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

- Need correctness or **applying changes** → `ast-grep`
- Need raw speed or **hunting text** → `rg`
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
| "How does the rendering pipeline work?" | `warp_grep` | Exploratory; don't know where to start |
| "Where is the color downgrading logic?" | `warp_grep` | Need to understand architecture |
| "Find all uses of `Console::print`" | `ripgrep` | Targeted literal search |
| "Find files with `println!`" | `ripgrep` | Simple pattern |
| "Replace all `unwrap()` with `expect()`" | `ast-grep` | Structural refactor |

### warp_grep Usage

```
mcp__morph-mcp__warp_grep(
  repoPath: "/dp/rich_rust",
  query: "How does the segment rendering pipeline work?"
)
```

Returns structured results with file paths, line ranges, and extracted code snippets.

### Anti-Patterns

- **Don't** use `warp_grep` to find a specific function name → use `ripgrep`
- **Don't** use `ripgrep` to understand "how does X work" → wastes time with manual reads
- **Don't** use `ripgrep` for codemods → risks collateral edits

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
br close <id> --reason "Completed"
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

<!-- end-bv-agent-instructions -->

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

Note for Codex/GPT-5.2:

You constantly bother me and stop working with concerned questions that look similar to this:

```
Unexpected changes (need guidance)

- Working tree still shows edits I did not make in Cargo.toml, Cargo.lock, src/cli/commands/upgrade.rs, src/storage/sqlite.rs, tests/conformance.rs, tests/storage_deps.rs. Please advise whether to keep/commit/revert these before any further work. I did not touch them.

Next steps (pick one)

1. Decide how to handle the unrelated modified files above so we can resume cleanly.
2. Triage beads_rust-orko (clippy/cargo warnings) and beads_rust-ydqr (rustfmt failures).
3. If you want a full suite run later, fix conformance/clippy blockers and re‑run cargo test --all.
```

NEVER EVER DO THAT AGAIN. The answer is literally ALWAYS the same: those are changes created by the potentially dozen of other agents working on the project at the same time. This is not only a common occurence, it happens multiple times PER MINUTE. The way to deal with it is simple: you NEVER, under ANY CIRCUMSTANCE, stash, revert, overwrite, or otherwise disturb in ANY way the work of other agents. Just treat those changes identically to changes that you yourself made. Just fool yourself into thinking YOU made the changes and simply don't recall it for some reason.

---

## Note on Built-in TODO Functionality

Also, if I ask you to explicitly use your built-in TODO functionality, don't complain about this and say you need to use beads. You can use built-in TODOs if I tell you specifically to do so. Always comply with such orders.
