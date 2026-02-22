# ms

<div align="center">
  <img src="ms_illustration.webp" alt="ms - Meta Skill CLI: A local-first skill management platform">
</div>

<div align="center">

[![CI](https://github.com/Dicklesworthstone/meta_skill/actions/workflows/ci.yml/badge.svg)](https://github.com/Dicklesworthstone/meta_skill/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT%2BOpenAI%2FAnthropic%20Rider-blue.svg)](./LICENSE)

</div>

**Meta Skill (`ms`)** is a local-first skill management platform that turns operational knowledge into structured, searchable, reusable artifacts. It provides dual persistence (SQLite + Git), hybrid search (lexical + semantic), adaptive suggestions with bandit optimization, multi-layer security (prompt injection defense + command safety gates), dependency graph analysis, provenance tracking, and native AI agent integration via MCP.

<div align="center">
<h3>Quick Install</h3>

```bash
cargo install --path .
```

**Or run without installing:**

```bash
cargo run -- <COMMAND>
```

<p><em>Works on Linux, macOS, and Windows. Requires Rust 1.85+ (Edition 2024).</em></p>
</div>

---

## What ms Actually Is

ms is not just a tool for extracting skills from AI sessions. It's a **complete skill management platform** with these core capabilities:

| Capability | What It Provides |
|-----------|------------------|
| **Skill Storage** | Dual persistence with SQLite for queries and Git for audit trails |
| **Semantic Search** | Hybrid BM25 + hash embeddings fused with Reciprocal Rank Fusion |
| **Adaptive Suggestions** | UCB bandit algorithm that learns from feedback to optimize recommendations |
| **Security** | ACIP prompt injection defense + DCG destructive command safety gates |
| **Graph Analysis** | Dependency insights via bv: cycles, bottlenecks, PageRank, execution plans |
| **Provenance** | Evidence tracking linking skills back to source sessions |
| **Multi-Machine Sync** | Git-based synchronization with conflict resolution strategies |
| **Bundle Distribution** | Portable skill packages with checksums and safe updates |
| **Effectiveness Loop** | Feedback, outcomes, experiments, and quality scoring as first-class data |
| **AI Agent Integration** | MCP server exposing skills as native tools for Claude, Codex, and others |
| **Anti-Pattern Mining** | Automatic detection of failure patterns from session history |
| **Token Packing** | Constrained optimization to fit skills within context budgets |
| **CASS Memory** | Integration with cm for playbook rules and historical context |

Skills can come from anywhere: hand-written `SKILL.md` files, mined from CASS sessions, imported from bundles, or generated through guided workflows. The CASS integration is one input method, not the defining feature.

---

## Quick Example

```bash
# Initialize a local ms root (.ms/)
ms init

# Configure skill paths
ms config skill_paths.project '["./skills"]'

# Index SKILL.md files
ms index

# Search and inspect
ms search "error handling"
ms show rust-error-handling

# Get context-aware suggestions
ms suggest

# Analyze skill dependencies
ms graph insights

# Start MCP server for AI agent integration
ms mcp serve
```

---

## Why This Architecture

### Dual Persistence: Speed + Accountability

Every skill is stored twice:

- **SQLite** for fast queries, filtering, full-text search, and metadata operations
- **Git archive** for immutable history, diffs, and audit trails

This mirrors how production systems balance operational speed with accountability. SQLite handles the "what do I need right now?" questions in milliseconds. Git answers "what happened and why?" when you need provenance.

The split also enables resilience: if the database corrupts, skills can be rebuilt from Git. If Git history is unavailable, the database still serves queries. Neither persistence layer is privileged—they serve different needs equally well.

### Hash Embeddings: Semantic Search Without Dependencies

Semantic search matters because keywords fail on phrasing differences. "Error handling" and "exception management" should match. But external embedding models create dependencies, latency, and reproducibility problems.

ms uses **deterministic hash embeddings**: FNV-1a based, 384 dimensions, computed locally. The same text produces the same vector on any machine, any time, with no model downloads or API calls. Combined with BM25 lexical search and RRF fusion, this gives both precision (exact matches) and recall (conceptual matches) without external dependencies.

Why RRF (Reciprocal Rank Fusion)? Because merging by raw scores is unstable—BM25 scores and embedding similarities have different scales and distributions. RRF uses rank positions instead, which normalizes the signals and produces stable, predictable rankings regardless of query type.

### Bandit Optimization: Learning What Works

Static ranking systems can't adapt. The suggestion engine uses a **Thompson sampling bandit** that learns from feedback:

- Each signal type (BM25 score, semantic similarity, recency, feedback rating, etc.) is an "arm"
- Success/failure feedback updates beta distributions for each arm
- UCB exploration bonus prevents premature convergence
- Context modifiers adjust weights based on project type, time of day, and other factors

Over time, the system learns which signals matter for your workflow. A team that values recency will see recency weighted higher. A codebase where semantic matches outperform keywords will shift accordingly. This isn't magic—it's a well-understood algorithm applied to a problem that benefits from continuous learning.

### Multi-Layer Security: Defense in Depth

AI-assisted workflows create new attack surfaces. ms implements defense at multiple layers:

**ACIP (Agent Content Injection Prevention)**: Classifies content by trust boundary (user/assistant/tool/file) and quarantines prompt injection attempts. Disallowed content is stored with safe excerpts for review, not silently dropped. Replay is opt-in and requires explicit acknowledgment.

**DCG (Destructive Command Guard)**: Evaluates shell commands before execution. Commands are classified into tiers (Safe/Caution/Danger/Critical) with configurable approval requirements. Dangerous commands can require verbatim approval through environment variables.

**Path Policy**: Prevents symlink escapes and directory traversal attacks. All file operations validate paths against allowed roots.

**Secret Scanning**: Detects and redacts credentials, API keys, and PII before content enters the system.

### Graph Analysis via bv: Use the Right Tool

Skill dependencies form a graph. Rather than implement graph algorithms from scratch, ms converts skills to a beads-style JSONL format and delegates to `bv` (beads_viewer):

- **PageRank** identifies keystone skills that anchor many others
- **Betweenness centrality** finds bottlenecks that block progress
- **Cycle detection** surfaces circular dependencies
- **Critical path analysis** generates execution plans with parallel tracks
- **HITS algorithm** distinguishes authoritative skills from hubs

This is the same philosophy as using SQLite for queries: use battle-tested tools for what they do best, keep the core focused. bv has the graph algorithms; ms has the skill semantics.

### Effectiveness as Data, Not Anecdotes

Usage feedback, outcomes, experiments, and quality scores are stored as structured data:

- **Feedback**: Explicit signals (helpful/unhelpful, ratings, comments) linked to skills
- **Outcomes**: Success/failure markers for suggestions that were acted upon
- **Experiments**: A/B test records tracking variant performance
- **Quality Scores**: Session quality signals (tests passed, clear resolution, backtracking, abandonment)

This transforms "this skill seems useful" into measurable evidence. The bandit learns from it. Prune commands can use it. Graph analysis can weight by it. Quality signals inform which sessions are worth mining.

### MCP Server: Native AI Agent Integration

Rather than force AI agents to parse CLI output, ms exposes a proper MCP (Model Context Protocol) server:

```bash
ms mcp serve              # stdio transport for Claude Code
ms mcp serve --port 8080  # HTTP transport for other integrations
```

The server exposes six tools that agents can call directly:
- `search`: Query skills with hybrid search
- `load`: Retrieve skill content with progressive disclosure
- `evidence`: Get provenance for a skill
- `list`: Enumerate available skills
- `show`: Full skill details
- `doctor`: Health check

This means Claude, Codex, and other MCP-aware agents can use ms as a native tool, not a string-parsing exercise.

---

## Skill Sources

Skills can enter the system through multiple paths:

### 1. Hand-Written SKILL.md Files

Write skills directly as markdown:

````markdown
# Rust Error Handling

Best practices for error handling in Rust projects.

## Overview

Use `Result<T, E>` and propagate errors with `?`. Define custom error types for domain logic.

## Examples

```rust
fn read_config(path: &str) -> Result<Config, ConfigError> {
    let contents = std::fs::read_to_string(path)?;
    toml::from_str(&contents).map_err(ConfigError::Parse)
}
```

## Guidelines

- Prefer `thiserror` for library errors, `anyhow` for application errors
- Always include context when wrapping errors
- Use `expect()` only when panic is the correct response
````

### 2. CASS Session Mining

Extract patterns from prior AI agent sessions:

```bash
# Single-shot extraction
ms build --from-cass "error handling" --since "7 days"

# Guided workflow with checkpoints
ms build --guided --from-cass "authentication"
```

The extraction pipeline:
1. Searches CASS for relevant sessions
2. Applies quality filters (clear resolution, tests passed, no backtracking)
3. Extracts patterns with uncertainty quantification
4. Synthesizes into structured skill format
5. Links evidence back to source sessions

### 3. Bundle Import

Install pre-packaged skill sets:

```bash
ms install https://example.com/team-skills.msb
ms bundle install ./local-bundle.msb
```

Bundles are verified with checksums and per-file hashes. Updates are gated by local modification detection so user edits are not overwritten by surprise.

### 4. Multi-Machine Sync

Pull skills from configured remotes:

```bash
ms remote add origin git@github.com:team/skills.git --remote-type git --auth ssh
ms sync
```

Sync with JeffreysPrompts Premium Cloud:

```bash
ms remote add jfp https://pro.jeffreysprompts.com/api/ms/sync \
  --remote-type jfp-cloud \
  --auth token \
  --token-env JFP_CLOUD_TOKEN
ms sync
```

---

## Core Commands

Global flags work with all commands:

```bash
--robot     # JSON output to stdout (for automation)
--verbose   # Increase logging verbosity
--quiet     # Suppress non-error output
--config    # Explicit config path
```

### Initialization and Configuration

```bash
ms init                              # Create .ms/ in current directory
ms init --global                     # Create in ~/.local/share/ms/
ms config                            # Show current config
ms config skill_paths.project '["./skills"]'
ms config search.use_embeddings true
```

### Indexing and Discovery

```bash
ms index                             # Index all configured skill paths
ms index ./skills /other/path        # Index specific paths
ms list                              # List all indexed skills
ms list --tags rust --layer project  # Filter by tags/layer
ms show rust-error-handling          # Full skill details
ms show rust-error-handling --meta   # Metadata only
```

### Search

```bash
ms search "error handling"           # Hybrid search (BM25 + semantic + RRF)
ms search "async" --search-type bm25 # Lexical only
ms search "async" --search-type semantic  # Semantic only
```

### Loading and Suggestions

```bash
ms load rust-error-handling --level overview  # Progressive disclosure
ms load rust-error-handling --pack 2000       # Token-constrained packing
ms load rust-error-handling --pack 800 --contract debug   # Contracted packing (debug/refactor/learn/quickref/codegen)
ms suggest                           # Context-aware recommendations
ms suggest --cwd /path/to/project    # Explicit context
```

### Context-Aware Auto-Loading

Automatically load relevant skills based on your current project context:

```bash
ms load --auto                       # Auto-detect and load relevant skills
ms load --auto --threshold 0.5       # Only load skills scoring above 0.5
ms load --auto --dry-run             # Preview what would be loaded
ms load --auto --confirm             # Prompt before loading each skill
```

Auto-loading analyzes your project to determine relevant skills:

- **Project detection**: Identifies Rust, Node.js, Python, Go, etc. from marker files
- **File patterns**: Matches recently modified files against skill file patterns
- **Tool detection**: Checks for installed tools (cargo, npm, pip, etc.)
- **Context signals**: Scans file content for framework/library patterns

Skills can specify context requirements in their frontmatter:

```yaml
---
name: rust-error-handling
context:
  project_types: [rust]
  file_patterns: ["*.rs"]
  tools: [cargo, rustc]
  signals:
    - pattern: "use thiserror"
      weight: 0.8
---
```

Configuration options in `config.toml`:

```toml
[auto_load]
learning_enabled = true      # Enable bandit learning from feedback
exploration_rate = 0.1       # Rate of exploration for new skills
bandit_blend = 0.3           # Blend factor for learned vs computed scores
cold_start_threshold = 10    # Min uses before trusting learned weights
persist_state = true         # Save bandit state between sessions
```

### Pack Contracts

Pack contracts let you persist custom packing rules (required groups, weights, max-per-group)
and reuse them across sessions.

```bash
ms contract list                             # Show built-in + custom contracts
ms contract create debug-lite \
  --description "Slim debug pack" \
  --required pitfalls,rules \
  --group-weight pitfalls:2.0 \
  --group-weight rules:1.2

ms load rust-error-handling --pack 800 --contract debug   # Built-in preset
# (Custom contracts are persisted for future use and can be listed via ms contract list.)
```

### Templates and Authoring

```bash
ms template list                     # Discover curated templates
ms template show debugging           # Preview template markdown
ms template apply debugging \
  --id debug-rust-builds \
  --name "Debug Rust Builds" \
  --description "Diagnose Rust build failures and compiler errors." \
  --tag rust,build                   # Create a skill from a template
```

### Graph Analysis

Analyze skill dependencies via bv (beads_viewer):

```bash
ms graph insights                    # Full analysis (cycles, keystones, bottlenecks)
ms graph plan                        # Execution plan with parallel tracks
ms graph triage                      # Best next picks
ms graph export --format mermaid     # Export as mermaid/dot/json
ms graph cycles --limit 10           # Show dependency cycles
ms graph keystones --limit 10        # Top PageRank skills
ms graph bottlenecks --limit 10      # Top betweenness skills
ms graph health                      # Label health summary
```

### Security

```bash
# Prompt injection defense (ACIP)
ms security status                   # ACIP health check
ms security scan --input "text" --session-id sess_1
ms security quarantine list          # Review quarantined content
ms security quarantine show <id>
ms security quarantine review <id> --confirm-injection
ms security quarantine replay <id> --i-understand-the-risks

# Command safety (DCG)
ms safety status                     # DCG availability
ms safety log --limit 20             # Recent safety decisions
ms safety check "rm -rf /tmp"        # Test command classification
```

### Effectiveness Tracking

```bash
# Feedback
ms feedback add rust-error-handling --positive --comment "saved hours"
ms feedback add rust-error-handling --rating 4
ms feedback list --skill rust-error-handling

# Outcomes
ms outcome rust-error-handling --success
ms outcome rust-error-handling --failure

# Experiments
ms experiment create rust-error-handling --variant control --variant concise
ms experiment list
ms experiment status <experiment-id> --metric task_success
ms experiment assign <experiment-id> --context ./context.json
ms experiment load <experiment-id> --context ./context.json --pack 800 --contract debug
ms experiment record <experiment-id> control --metric task_success=true
ms experiment conclude <experiment-id> --winner control
ms load rust-error-handling --experiment-id <experiment-id> --variant-id control
```

Metrics and outcomes:
- Use `--metric key=value` pairs on `ms experiment record`. Values can be booleans, numbers, or strings.
- Success is inferred from the metric key you select (default: `task_success`), where:
  - `true` / `success` / numeric > 0.5 => success
  - `false` / `failure` / numeric <= 0.5 => failure
- `ms experiment status` aggregates assignment and outcome counts per variant and reports a simple two-proportion z-test p-value.

Robot payloads:
- `ms experiment load --robot` returns the usual `ms load` JSON plus an `experiment` block:
  - `experiment.id`, `experiment.metric`, `experiment.variant`, and the assignment `event`.

# Bandit state
```bash
ms bandit stats                      # Current arm weights
ms bandit reset                      # Reset learning
```

### Evidence and Provenance

```bash
ms evidence show rust-error-handling           # All evidence for skill
ms evidence show rust-error-handling --rule overview --excerpts
ms evidence list --limit 100
ms evidence export --format dot --output graph.dot
```

### Anti-Patterns

```bash
ms antipatterns mine --from-cass "authentication"  # Mine failure patterns
ms antipatterns list                 # All detected anti-patterns
ms antipatterns show <id>            # Details with linked skills
ms antipatterns link <pattern-id> <skill-id>      # Manual linking
```

### Cross-Project Learning

```bash
ms cross-project summary                      # Sessions by project
ms cross-project summary --query "error"      # Filter sessions with CASS query
ms cross-project summary --top 10             # Top N projects

ms cross-project patterns                     # Aggregate patterns across projects
ms cross-project patterns --min-occurrences 3 --min-projects 2
ms cross-project patterns --query "rust"      # Pattern mining scoped by query

ms cross-project gaps                         # Patterns with weak/no skill matches
ms cross-project gaps --min-score 1.0         # Treat low-scoring matches as gaps
```

### Bundles and Distribution

```bash
ms bundle create my-bundle --from-dir ./skills
ms bundle install ./my-bundle.msb
ms bundle list
ms bundle show my-bundle
ms bundle conflicts                  # Check for local modifications
ms bundle update --check             # Preview updates
ms bundle update my-bundle --force   # Apply with backup
```

### Multi-Machine Sync

```bash
ms remote add origin /path/to/archive --type filesystem
ms remote add origin https://github.com/user/skills.git --type git --auth token --token-env GIT_TOKEN
ms remote add origin git@github.com:user/skills.git --type git --auth ssh --ssh-key ~/.ssh/id_rsa
ms remote list
ms sync                              # Bidirectional sync
ms sync origin --dry-run             # Preview changes
ms sync --status                     # Current sync state
ms conflicts list                    # Unresolved conflicts
ms conflicts resolve <skill> --strategy prefer-local --apply
ms machine info                      # Machine identity
```

#### RU (Repo Updater) Backend

If you use `ru` for repo sync, configure it in `config.toml`:

```toml
[ru]
enabled = true
ru_path = "/usr/local/bin/ru" # optional
skill_repos = ["org/skills", "org/internal-skills@main"]
auto_index = true
parallel = 4
```

### CASS Memory Integration

```bash
ms cm status                         # CM availability
ms cm context "implement auth"       # Get relevant playbook context
ms cm rules --category debugging     # List playbook rules
ms cm similar "error handling"       # Find similar rules
```

### MCP Server

Expose skills as native tools for AI agents:

```bash
ms mcp serve                         # Start MCP server (stdio transport)
ms mcp serve --port 8080             # HTTP transport
```

### Maintenance

```bash
ms doctor                            # Health checks
ms doctor --fix                      # Auto-repair issues
ms backup create                     # Snapshot ms state
ms backup list                       # List backups
ms backup restore --latest --approve # Restore latest snapshot
ms fmt                               # Normalize skill formatting
ms diff skill-a skill-b              # Semantic diff
ms migrate                           # Upgrade skill spec versions
ms prune list                        # List prunable data
ms prune analyze                     # Analyze pruning candidates
ms prune proposals                   # Propose merge/deprecate actions
ms prune proposals --emit-beads      # Emit beads issues for proposals
ms prune review                      # Interactive proposal review
ms prune apply merge:a,b --approve   # Apply a proposal (merge/deprecate/split)
ms prune purge all --older-than 30 --approve
ms validate rust-error-handling      # Schema validation
ms validate rust-error-handling --ubs  # With static analysis
ms test rust-error-handling          # Run skill tests
ms update --check                    # Check for CLI updates
```

---

## Storage Architecture

```
.ms/
├── ms.db           # SQLite database (queries, metadata, search)
├── archive/        # Git repository (audit trail, history)
├── index/          # Tantivy search index
├── backups/        # Backup snapshots
├── sync/           # Sync state and remote caches
└── config.toml     # Local configuration
```

Global storage: `~/.local/share/ms/`

The separation is intentional:
- **ms.db**: Fast reads, transactions, FTS5, concurrent access
- **archive/**: Immutable history, blame, diff, merge
- **index/**: Tantivy for sub-millisecond full-text search

---

## Search Architecture

```
Query
  ├── BM25 (SQLite FTS5)     → Keyword precision
  ├── Hash Embeddings (384d) → Semantic recall
  └── RRF Fusion             → Stable ranking
```

Neither signal dominates. RRF (Reciprocal Rank Fusion) merges results by rank position rather than raw scores, which stabilizes rankings across different query types.

Hash embeddings use FNV-1a hashing to project tokens into a fixed-dimension space. No model weights, no API calls, fully deterministic. The same text produces the same embedding on any machine.

---

## Security Model

### Trust Boundaries (ACIP)

Content is classified by source:
- **User**: Direct human input (highest trust)
- **Assistant**: AI-generated content (high trust)
- **Tool**: External tool output (medium trust)
- **File**: File contents (variable trust)

Injection patterns are detected across boundaries. Suspicious content is quarantined with safe excerpts, not silently dropped. The quarantine system:
1. Records the detection context (session, message index, content hash)
2. Stores a safe excerpt for review
3. Logs the classification decision
4. Allows review/replay with explicit acknowledgment

### Command Safety (DCG)

Shell commands are evaluated before execution:
- **Safe**: No restrictions (ls, cat, echo)
- **Caution**: Logged, allowed (git commit, npm install)
- **Danger**: Requires acknowledgment (rm -r, chmod)
- **Critical**: Requires verbatim approval (rm -rf /, format)

Approval works via environment variable:
```bash
MS_APPROVE_COMMAND="rm -rf /tmp/test" ms build ...
```

---

## Skill Format

Skills are stored as `SKILL.md` with deterministic round-tripping to a canonical spec:

````markdown
# Skill Name

Description paragraph.

## Overview

High-level explanation.

## Examples

```language
code examples
```

## Guidelines

- Rule 1
- Rule 2
````

Parsing rules:
- `#` title → skill name
- First paragraph → description
- `##` sections → structured sections with blocks
- Code fences → typed code blocks with metadata

The spec can be serialized to JSON for tooling:
```bash
ms show skill-name --robot
```

---

## Configuration

Config precedence (lowest to highest):
1. Built-in defaults
2. Global config (`~/.config/ms/config.toml`)
3. Project config (`.ms/config.toml`)
4. Environment variables (`MS_*`)
5. CLI flags

Key environment variables:
- `MS_ROOT` — explicit ms root
- `MS_CONFIG` — explicit config path
- `MS_ROBOT` — force robot mode
- `MS_SEARCH_USE_EMBEDDINGS` — toggle semantic search

---

## Prepared Blurb for AGENTS.md Files

````
## ms — Meta Skill CLI

Local-first skill management platform with dual persistence (SQLite + Git), hybrid search (BM25 + hash embeddings via RRF), multi-layer security (ACIP injection defense + DCG command safety), adaptive suggestions (Thompson sampling bandit), and native AI agent integration (MCP server).

### Core Workflow

```bash
ms init                              # Initialize
ms config skill_paths.project '["./skills"]'
ms index                             # Index skills
ms search "query"                    # Hybrid search
ms suggest                           # Context-aware recommendations
ms load skill-name --pack 2000       # Token-constrained loading
```

### For AI Agents

```bash
ms mcp serve                         # Start MCP server
ms search "query" --robot            # JSON output
ms load skill --level overview --robot
```

### Key Commands

| Command | Purpose |
|---------|---------|
| `ms search` | Hybrid search (BM25 + semantic) |
| `ms suggest` | Context-aware suggestions with bandit optimization |
| `ms load` | Progressive disclosure with token packing |
| `ms graph` | Dependency analysis via bv |
| `ms security` | ACIP prompt injection defense |
| `ms safety` | DCG command safety gates |
| `ms evidence` | Provenance tracking |
| `ms antipatterns` | Failure pattern detection |
| `ms template` | Curated authoring templates |
| `ms bundle` | Portable skill packages |
| `ms backup` | Snapshot and restore ms state |
| `ms sync` | Multi-machine synchronization |
| `ms mcp` | MCP server for AI agents |
````

---

## How ms Compares

| Feature | ms | Manual Wiki | Raw CASS | Ad-hoc Notes |
|---------|----|-------------|----------|--------------|
| Structured skills | ✅ | ⚠️ Depends | ❌ | ❌ |
| Queryable | ✅ SQLite + FTS | ⚠️ Search only | ⚠️ CLI grep | ❌ |
| Audit trail | ✅ Git archive | ⚠️ If tracked | ❌ | ❌ |
| Safety filters | ✅ ACIP + DCG | ❌ | ❌ | ❌ |
| Hybrid search | ✅ BM25 + semantic | ❌ | ❌ | ❌ |
| CLI automation | ✅ Robot mode | ⚠️ | ⚠️ | ❌ |
| AI agent native | ✅ MCP server | ❌ | ❌ | ❌ |
| Effectiveness data | ✅ Bandit + feedback | ❌ | ❌ | ❌ |

---

## Troubleshooting

### "No skill paths configured"

```bash
ms config skill_paths.project '["./skills"]'
```

### "bv is not available"

Graph commands require bv (beads_viewer):

```bash
# Install beads_viewer
cargo install --git https://github.com/Dicklesworthstone/beads_viewer
```

### "Search returns no results"

```bash
ms index          # Re-index skills
ms doctor --fix   # Check for issues
```

### "ACIP not enabled"

```bash
ms security status
ms config security.acip.enabled true
```

---

## Origins

Created by **Jeffrey Emanuel** to systematize operational knowledge with the same rigor applied to production code. The goal: turn hard-won workflows into durable, searchable, reusable artifacts that improve over time through measured feedback.

---

## Contributing

*About Contributions:* Please don't take this the wrong way, but I do not accept outside contributions for any of my projects. I simply don't have the mental bandwidth to review anything, and it's my name on the thing, so I'm responsible for any problems it causes; thus, the risk-reward is highly asymmetric from my perspective. I'd also have to worry about other "stakeholders," which seems unwise for tools I mostly make for myself for free. Feel free to submit issues, and even PRs if you want to illustrate a proposed fix, but know I won't merge them directly. Instead, I'll have Claude or Codex review submissions via `gh` and independently decide whether and how to address them. Bug reports in particular are welcome. Sorry if this offends, but I want to avoid wasted time and hurt feelings.

---

## License

MIT License (with OpenAI/Anthropic Rider) — see [LICENSE](LICENSE) for details.

---

Built with Rust, SQLite, Tantivy, and deterministic hash embeddings. Local-first by design, safety-aware by default.
