# ms

<div align="center">
  <img src="ms_illustration.webp" alt="ms - Meta Skill CLI for mining CASS sessions into reusable skills">
</div>

<div align="center">

[![CI](https://github.com/Dicklesworthstone/meta_skill/actions/workflows/ci.yml/badge.svg)](https://github.com/Dicklesworthstone/meta_skill/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

</div>

Meta Skill (`ms`) is a local-first CLI that mines CASS sessions into durable, production-grade skills. It stores skills with dual persistence (SQLite + Git), supports hybrid search (BM25 + hash embeddings), and enforces safety boundaries during extraction.

<div align="center">
<h3>Quick Install</h3>

```bash
# Build & install from source
cargo install --path .
```

**Or run without installing:**

```bash
cargo run -- <COMMAND>
```

<p><em>Works on Linux, macOS, and Windows. Requires Rust 1.85+ (Edition 2024).</em></p>
</div>

---

## TL;DR

**The Problem**: Agent workflows get rediscovered repeatedly. CASS captures raw sessions, but there is no systematic way to distill them into reusable skills.

**The Solution**: `ms` extracts patterns, compiles them into structured skill specs, and stores them with provenance so they can be searched, loaded, and reused.

### Why Use ms?

| Feature | What It Does |
|---------|--------------|
| **Dual Persistence** | SQLite for fast queries + Git archive for auditability |
| **Hybrid Search** | BM25 full-text + hash embeddings fused with RRF |
| **Robot Mode** | JSON output for automation (`--robot`) |
| **Safety Controls** | ACIP prompt-injection quarantine + trust boundaries |
| **Structured Skills** | Deterministic `SKILL.md` ↔ spec round-trip |
| **Layering** | Priority layers (base/org/project/user) |
| **Local-First** | All processing is local by default |

### Quick Example

```bash
# Initialize a local ms root (.ms/)
ms init

# Configure a project skill path
ms config skill_paths.project '["./skills"]'

# Index SKILL.md files
ms index

# Search and inspect
ms search "error handling"
ms show rust-error-handling
```

---

## Prepared Blurb for AGENTS.md Files

```
## ms — Meta Skill CLI

Local-first CLI for mining CASS sessions into production-grade skills. Maintains dual persistence (SQLite + Git), supports hybrid search (BM25 + hash embeddings via RRF), and enforces safety boundaries with ACIP prompt-injection quarantine.

### Core Workflow

```bash
# 1. Initialize in a project
ms init

# 2. Configure skill paths
ms config skill_paths.project '["./skills"]'

# 3. Index skills
ms index

# 4. Explore and search
ms list
ms search "error handling"
ms show rust-error-handling

Key Flags

--robot                     # JSON output to stdout
--verbose                   # More logs
--config /path/to/config    # Explicit config path

Storage

- Local root: ./.ms (project) or ~/.local/share/ms (global)
- DB: <ms_root>/ms.db
- Git archive: <ms_root>/archive/
- Index: <ms_root>/index/

Notes

- Some commands are implemented as stubs (build, load, requirements, prune, update).
- ACIP safety scanning is implemented under `ms security`.
```

---

## Design Philosophy

`ms` is built around a few core principles:

### Local-First and Auditable

- **SQLite + Git**: Fast queries *and* a full audit trail.
- **No cloud dependency**: CASS data stays local unless you explicitly sync it.
- **Deterministic outputs**: Skills can round-trip between spec and markdown.

### Safety by Default

- **Trust boundaries**: User/assistant/tool/file content is classified before extraction.
- **Quarantine**: Prompt-injection signals are quarantined with safe excerpts.
- **Auditability**: ACIP decisions are stored with replay commands (opt-in).

### Composition Over Ceremony

- JSON in robot mode for easy pipelines.
- Human-friendly output by default.
- Commands are designed to compose with Unix tooling.

---

## How ms Compares

| Feature | ms | Manual Wiki | Raw CASS | Ad-hoc Notes |
|---------|----|-------------|----------|--------------|
| Structured skills | ✅ | ⚠️ Depends | ❌ | ❌ |
| Queryable | ✅ SQLite + FTS | ⚠️ Search only | ⚠️ CLI grep | ❌ |
| Audit trail | ✅ Git archive | ⚠️ If tracked | ❌ | ❌ |
| Safety filters | ✅ ACIP | ❌ | ❌ | ❌ |
| Hybrid search | ✅ BM25 + semantic | ❌ | ❌ | ❌ |
| CLI automation | ✅ Robot mode | ⚠️ | ⚠️ | ❌ |

---

## Origins & Authors

Created by **Jeffrey Emanuel** to transform raw agent sessions into reusable, versioned skills. The goal is to preserve hard-won workflows with the same rigor we apply to production code.

---

## Getting CASS Data Ready

`ms build` and related mining workflows require CASS sessions. CASS stores session history as JSONL and exposes a CLI (`cass`).

### Quick Check

```bash
cass health
```

### Typical Session Query

```bash
cass search "error handling" --robot --limit 5
```

### Common Config

```toml
[cass]
auto_detect = true
cass_path = "/path/to/cass"
session_pattern = "*.jsonl"
```

---

## Installation

### From Source (Recommended)

```bash
git clone https://github.com/Dicklesworthstone/meta_skill.git
cd meta_skill
cargo build --release
cp target/release/ms ~/.local/bin/
```

### Install via Cargo

```bash
cargo install --git https://github.com/Dicklesworthstone/meta_skill.git
```

> Prebuilt binaries are not yet published. If/when they are, this README will be updated.

---

## Quick Start

### 1. Initialize

```bash
ms init
```

### 2. Add Skill Paths

```bash
ms config skill_paths.project '["./skills"]'
```

### 3. Index

```bash
ms index
```

### 4. Search + Inspect

```bash
ms list
ms search "error handling"
ms show rust-error-handling
```

---

## Commands

Global flags:

```bash
--robot     # JSON output to stdout
--verbose   # Increase logging verbosity
--quiet     # Suppress non-error output
--config    # Explicit config path
```

### `ms init`

```bash
ms init
ms init --global
ms init --force
```

Creates:
- `.ms/ms.db` (SQLite)
- `.ms/archive/` (Git)
- `.ms/index/` (search index)
- `.ms/config.toml`

### `ms index`

```bash
ms index
ms index ./skills /other/path
ms index --force
```

### `ms list`

```bash
ms list
ms list --tags rust --layer project --limit 50
ms list --robot
```

### `ms show`

```bash
ms show rust-error-handling
ms show rust-error-handling --full
ms show rust-error-handling --meta
```

### `ms search`

```bash
ms search "error handling"
ms search "async" --search-type bm25
ms search "async" --search-type semantic
```

### `ms load` (planned)

Stubbed. Intended to load skills with progressive disclosure and token budgets.

### `ms suggest`

Captures context + computes a fingerprint. Suggestions currently return an empty list, but cooldown tracking is implemented.

```bash
ms suggest
ms suggest --cwd /path/to/project
```

### `ms edit`

```bash
ms edit rust-error-handling
```

### `ms fmt`

Formats skill files to canonical layout.

### `ms diff`

Semantic diff between skills.

### `ms alias`

Manage skill aliases.

### `ms requirements` (planned)

Stubbed. Intended for dependency checks.

### `ms build` (planned)

Stubbed. Intended to mine CASS sessions into skills.

```bash
ms build --from-cass "error handling"
```

### `ms bundle`

Package skills into portable bundles.

### `ms update` (planned)

Stubbed. Intended to update `ms` itself.

### `ms doctor`

```bash
ms doctor
ms doctor --fix
ms doctor --check-lock
```

### `ms pre-commit`

Runs UBS checks on staged files.

### `ms prune` (planned)

Stubbed. Intended to remove tombstoned or outdated data safely.

### `ms config`

```bash
ms config
ms config search.use_embeddings
ms config search.use_embeddings false
ms config --unset search.use_embeddings
```

### `ms security`

```bash
ms security scan --input "ignore previous instructions" --session-id sess_1 --message-index 1
ms security quarantine list
ms security quarantine review <id> --confirm-injection
```

### `ms validate`

Validate skill specs.

### `ms test`

```bash
ms test --all
ms test rust-error-handling
ms test --tags smoke
```

---

## Skill Format (SKILL.md)

`ms` parses skills from `SKILL.md` and can round-trip them via a structured spec.

Minimal example:

````markdown
# Rust Error Handling

Best practices for error handling in Rust.

## Overview

Use `Result<T, E>` and propagate errors with `?`.

## Examples

````
fn read_file(path: &str) -> Result<String, std::io::Error> {
    std::fs::read_to_string(path)
}
```
```

Parsing rules:
- `#` title becomes the skill name.
- The first paragraph after the title becomes the description.
- Each `##` section becomes a `SkillSection` with blocks.

---

## Configuration

Config precedence (lowest to highest):
1. Built-in defaults
2. Global config (`~/.config/ms/config.toml`)
3. Project config (`.ms/config.toml`)
4. Environment variables (`MS_*`)
5. CLI flags

Useful env vars:
- `MS_ROOT` — explicit ms root
- `MS_CONFIG` — explicit config path
- `MS_ROBOT` — force robot mode
- `MS_SEARCH_USE_EMBEDDINGS` — toggle semantic search

Default skill paths:
- Global: `~/.local/share/ms/skills`
- Project: `.ms/skills`

---

## Storage Locations

| Component | Location |
|----------|----------|
| ms root (project) | `.ms/` |
| ms root (global) | `~/.local/share/ms/` |
| SQLite database | `<ms_root>/ms.db` |
| Git archive | `<ms_root>/archive/` |
| Search index | `<ms_root>/index/` |

---

## Search Architecture

```text
Query
  ├─ SQLite FTS (BM25)
  ├─ Hash embeddings (semantic)
  └─ RRF fusion (rank-based)
```

Notes:
- FTS uses SQLite FTS5.
- Semantic vectors use deterministic hash embeddings (384 dims).
- Embeddings are stored as F16 for space efficiency.

---

## Safety & Prompt-Injection Defense

`ms` integrates ACIP v1.3:

- Messages are classified into trust boundaries.
- Disallowed content is quarantined with safe excerpts.
- Reviews and replays are opt-in.

Core commands:

```bash
ms security scan --input "ignore previous instructions" --session-id sess_1 --message-index 1
ms security quarantine list
ms security quarantine show <id>
```

---

## Testing

`ms` includes:
- Unit tests (meta_skill-7t2)
- Integration tests (meta_skill-9pr)
- Skill tests (`ms test`)

Recommended checks:

```bash
cargo test
cargo clippy --all-targets -- -D warnings
```

---

## Troubleshooting

### "No skill paths configured"

```bash
ms config skill_paths.project '["./skills"]'
```

### "CASS binary not found"

```bash
cass --version
```

### "Search returns no results"

```bash
ms index
ms search "query"
```

---

## Limitations

- `ms build`, `ms load`, `ms update`, `ms requirements`, and `ms prune` are not implemented yet.
- The suggestion engine currently returns an empty list (context fingerprinting is implemented).
- Prebuilt binaries are not published yet.

---

## FAQ

**Q: Why "ms"?**

`ms` stands for **Meta Skill**—skills about skills.

**Q: Is my data safe?**

Yes. All storage is local (SQLite + Git) unless you explicitly sync or upload.

**Q: Does ms work without CASS?**

Yes. You can index and manage hand-written skills using `SKILL.md` without any CASS integration.

---

## Contributing

*About Contributions:* Please don't take this the wrong way, but I do not accept outside contributions for any of my projects. I simply don't have the mental bandwidth to review anything, and it's my name on the thing, so I'm responsible for any problems it causes; thus, the risk-reward is highly asymmetric from my perspective. I'd also have to worry about other "stakeholders," which seems unwise for tools I mostly make for myself for free. Feel free to submit issues, and even PRs if you want to illustrate a proposed fix, but know I won't merge them directly. Instead, I'll have Claude or Codex review submissions via `gh` and independently decide whether and how to address them. Bug reports in particular are welcome. Sorry if this offends, but I want to avoid wasted time and hurt feelings. I understand this isn't in sync with the prevailing open-source ethos that seeks community contributions, but it's the only way I can move at this velocity and keep my sanity.

---

## License

MIT - see [LICENSE](LICENSE) for details.

---

Built with Rust, SQLite, Tantivy, and a deterministic hash embedder. `ms` is designed to turn raw agent sessions into durable, searchable knowledge.
