# PLAN_TO_MAKE_METASKILL_CLI.md

> **Project Codename:** `ms` (meta_skill)
> **Architecture Pattern:** Follow `/data/projects/xf` exactly
> **Primary Innovation:** Mining CASS sessions to generate production-quality skills
> **Plan Version:** 2026-01-13.39 (Section 39 + architectural enhancements)

---

## 0. Background Information (Self-Contained Context)

This section provides complete context for understanding this plan. Another LLM or human reader should be able to fully evaluate and improve this plan using only the information contained herein.

### 0.1 The AI Coding Agent Landscape (2025-2026)

AI coding agents are LLM-powered tools that autonomously write, modify, and debug code. Unlike simple code completion (GitHub Copilot circa 2022), modern agents can:

- Execute shell commands and observe output
- Read and write files across entire codebases
- Make multi-file changes to implement features
- Run tests, interpret failures, and fix issues
- Use external tools via function calling / tool use

**Major AI Coding Agents (as of early 2026):**

| Agent | Provider | Interface | Key Characteristics |
|-------|----------|-----------|---------------------|
| **Claude Code** | Anthropic | CLI (terminal) | Anthropic's official agentic CLI for Claude. Uses tool-use API. |
| **Codex CLI** | OpenAI | CLI | OpenAI's terminal agent, similar architecture to Claude Code |
| **Gemini CLI** | Google | CLI | Google's agent, supports Gemini models |
| **Cursor** | Cursor Inc | IDE | VS Code fork with deeply integrated AI |
| **Aider** | Open Source | CLI | Python-based, supports multiple models |
| **Continue** | Open Source | IDE Plugin | VS Code/JetBrains plugin |

These agents share a common interaction pattern:
1. User provides a natural language request
2. Agent reasons about the task
3. Agent uses tools (file read/write, bash, search) to accomplish the task
4. Agent presents results and awaits next instruction

**Session transcripts** are the complete logs of these interactions—every user message, agent response, tool call, and tool result. A single coding session might be 10,000-100,000+ tokens.

### 0.2 What Are Claude Code Skills?

**Skills** are Claude Code's mechanism for extending agent capabilities with domain-specific knowledge. A skill is a markdown file (conventionally `SKILL.md`) that gets injected into the agent's context when relevant. In ms, `SKILL.md` is a compiled view; the source-of-truth is `SkillSpec`.

**Why skills exist:** LLMs have general knowledge but lack specific knowledge about:
- Your company's coding conventions
- Your project's architecture decisions
- Specialized workflows (deployment, review processes)
- Tool-specific expertise (your CLI tools, your APIs)
- Lessons learned from past debugging sessions

**Skill file structure:**

[Diagram omitted: workflow/system diagram.]
- 11 line(s); see surrounding text for step labels.

**Session Segmentation (Phase-Aware Mining):**
- Segment sessions into phases: recon → hypothesis → change → validation → regression fix → wrap-up.
- Use tool-call boundaries + language markers to avoid phase bleed.

[Code block omitted: Rust enums: SessionPhase.]

**Pattern IR (Typed Intermediate Representation):**
- Compile extracted patterns into typed IR before synthesis (e.g., `CommandRecipe`,
  `DiagnosticDecisionTree`, `Invariant`, `Pitfall`, `PromptMacro`, `RefactorPlaybook`,
  `ChecklistItem`).
- Normalize commands, filepaths, tool names, and error signatures for deterministic dedupe.

**SKILL.md anatomy:**

[Code block omitted: example block (lang='markdown', 50 line(s)).]

**Conditional blocks:** The `::: block` syntax allows version-specific content. At load time, `ms` evaluates predicates against the project environment (package.json, Cargo.toml, etc.) and strips blocks whose conditions evaluate false. The agent never sees irrelevant version-specific content.

**How skills are loaded:** Claude Code discovers skills from configured paths (e.g., `~/.claude/skills/`, `.claude/skills/` in projects). When a user invokes a skill (explicitly or via auto-suggestion), its content is injected into the conversation context.

**The token budget problem:** Each skill consumes context window tokens. A 500-line skill might use 3,000-5,000 tokens. With context windows of 128K-200K tokens and complex codebases already consuming much of that, skill loading must be strategic.

### 0.3 What Is CASS (Coding Agent Session Search)?

**CASS** (Coding Agent Session Search) is a Rust CLI tool that indexes and searches across historical coding agent sessions from multiple tools. It solves the problem: "I know I solved this before, but I can't find where."

**What CASS indexes:**
- Claude Code sessions (`.jsonl` files in `~/.claude/projects/`)
- Codex CLI sessions
- Gemini CLI sessions
- Cursor sessions (exported)
- ChatGPT code-heavy conversations (exported)
- Custom agent transcripts

**Key CASS capabilities:**

[Code block omitted: shell commands using cass.]

**Robot mode:** All CASS commands support `--robot` for machine-readable JSON output. This is critical for programmatic integration—ms will call CASS as a subprocess and parse its JSON output.

**CASS search technology:**
- **Lexical search:** Tantivy (Rust port of Lucene) for BM25 full-text search
- **Semantic search:** Hash-based embeddings (no ML model required)
- **Hybrid fusion:** Reciprocal Rank Fusion (RRF) combines both rankings

**Session structure:** A session is a sequence of messages:
[Code block omitted: JSON payload; keys include role, content, tool_calls, tool_call_id.]

**Why CASS matters for ms:** CASS contains thousands of solved problems. When an agent successfully debugged an auth issue, that solution is in CASS. When a deployment workflow was refined over 10 sessions, the evolution is in CASS. ms can mine this to generate skills automatically.

### 0.4 What Is xf (X Archive Search)?

**xf** is a Rust CLI tool for searching personal X (Twitter) data archives. It's the architectural template for ms because it exemplifies the exact patterns we want:

**xf's technical stack:**
- **Language:** Rust (Edition 2024, ~23,000 LOC)
- **CLI framework:** clap with derive macros
- **Storage:** SQLite with WAL mode, FTS5 virtual tables
- **Search:** Tantivy BM25 + hash-based embeddings + RRF fusion
- **Async runtime:** Tokio
- **Output:** Human-readable by default, `--format json` for automation
- **Auto-update:** Self-updating binary via GitHub Releases

**Why follow xf exactly:**
1. **Proven patterns:** xf is battle-tested on real workloads
2. **Same author:** The ms author created xf, so patterns are familiar
3. **Same constraints:** Both are local-first CLI tools with search + indexing
4. **Code reuse:** Many modules can be adapted directly

**Key xf patterns to adopt:**

| Pattern | xf Implementation | ms Adaptation |
|---------|-------------------|---------------|
| Hybrid search | BM25 + hash embeddings + RRF | Same, for skill search |
| Hash embeddings | FNV-1a hash, 384 dimensions | Same, no ML dependency |
| SQLite storage | WAL mode, PRAGMA tuning | Same, for skill registry |
| Robot mode | `--format json` flag | `--robot` flag |
| Auto-update | GitHub Releases + SHA256 | Same mechanism |
| Indexing | Background thread, progress bar | Same UX |

**Hash embeddings explained:** Instead of using a neural network (BERT, etc.) to generate embeddings, xf uses a deterministic hash function:

1. Tokenize text into words
2. Hash each word with FNV-1a
3. Use hash bits to determine which embedding dimensions to increment/decrement
4. Normalize the resulting vector

This provides ~80-90% of neural embedding quality with zero dependencies, instant speed, and perfect reproducibility.

### 0.5 What Is mcp_agent_mail (Agent Mail)?

**Agent Mail** is a coordination system for multi-agent workflows. It provides:
- **Message passing:** Agents send messages to each other
- **File reservations:** Advisory locks to prevent edit conflicts
- **Project identity:** Agents know they're working on the same project

**The dual persistence pattern:** Agent Mail writes data to both SQLite (for queries) and Git (for auditability):

[Diagram omitted: workflow/system diagram.]
- 15 line(s); see surrounding text for step labels.

**Why ms adopts this:** Skills benefit from both:
- **SQLite:** Fast search, usage tracking, quality scores
- **Git:** Version history, collaborative editing, sync across machines

**Two-Phase Commit (2PC):** To prevent drift between SQLite and Git, ms uses a
lightweight two-phase commit for all write operations.

**File reservation pattern:** When an agent wants to edit a file, it requests a reservation:
[Code block omitted: shell commands using agent_mail.]

ms can use similar reservations for skill editing to prevent conflicts.

### 0.6 What Is NTM (Named Tmux Manager)?

**NTM** is a Go CLI that transforms tmux into a multi-agent command center. It spawns and orchestrates multiple AI coding agents in parallel.

**Why NTM matters for ms:**
1. **Multi-agent skill loading:** When NTM spawns agents, each needs appropriate skills
2. **Skill coordination:** Multiple agents shouldn't redundantly load same skills
3. **Context rotation:** As agents exhaust context, skills must transfer to fresh agents

**NTM agent types:**
[Code block omitted: shell commands using ntm.]

**Integration point:** ms should provide:
[Code block omitted: ms CLI examples; commands include load, suggest]

### 0.7 What Is BV (Beads Viewer) and the Beads System?

**Beads** is a lightweight issue/task tracking system designed for AI agent workflows. Unlike Jira/Linear, beads are:
- **File-based:** Stored in `.beads/` directory
- **Git-native:** Tracked in version control
- **Agent-friendly:** Simple enough for agents to read/write

**Bead structure:**
[Code block omitted: YAML example; keys include id, title, type, status, priority, created, ....]

**BV (Beads Viewer)** is the CLI for interacting with beads:
[Code block omitted: shell commands using bd.]

**Beads Viewer Integration (bv):**
- Prefer `bv --robot-*` flags for deterministic JSON (triage, plan, graph, insights).
- Use `bv --robot-triage` as the single entry point for actionable, dependency-aware picks.
- Avoid bare `bv` in automation (it launches interactive TUI).

**Why this matters for ms:** Skills can be tracked as beads. A skill-building session could be:
[Code block omitted: ms CLI examples; commands include build]

### 0.8 The Agent Flywheel Ecosystem

The **Agent Flywheel** is an integrated suite of tools that compound AI agent effectiveness:

[Diagram omitted: workflow/system diagram.]
- 40 line(s); see surrounding text for step labels.

**Other flywheel tools:**

| Tool | Purpose |
|------|---------|
| **CM** (Cass Memory) | Procedural memory—learns rules from session analysis |
| **DCG** (Destructive Command Guard) | Safety system blocking dangerous commands |
| **ACIP** | Prompt injection defense framework (primary for Section 5.17) |
| **UBS** (Ultimate Bug Scanner) | Static analysis for catching bugs pre-commit |
| **BV** (Beads Viewer) | Graph-aware Beads triage + dependency insights |
| **RU** (Repo Updater) | Syncs GitHub repos, handles updates |
| **XF** (X Archive Search) | Twitter archive search (architectural template) |

### 0.9 Token Efficiency: Why It Matters

**The context window constraint:** LLMs have finite context windows:
- Claude 3.5 Sonnet: 200K tokens
- GPT-4 Turbo: 128K tokens
- Gemini 1.5 Pro: 2M tokens (but performance degrades at scale)

**A typical coding session consumes:**
- System prompt: 2,000-5,000 tokens
- Project context (AGENTS.md, etc.): 3,000-10,000 tokens
- Codebase reading: 10,000-50,000+ tokens
- Conversation history: Grows throughout session
- **Skills:** Variable, 500-5,000 tokens each

**Token density imperative:** Skills must maximize information per token:

| Bad (Verbose) | Good (Token-Dense) |
|---------------|-------------------|
| "When you are working with Git and you need to..." | "Git operations:" |
| "It's important to remember that..." | (Just state the fact) |
| "Here's an example of how you might..." | "Example:" |
| Repeating information across sections | Single source of truth |
| Generic advice applicable anywhere | Specific, actionable guidance |

**Progressive disclosure solves this:** Don't load entire 5,000-token skills when 500 tokens suffice:

| Level | Tokens | Content |
|-------|--------|---------|
| Minimal | ~100 | Name + one-line description |
| Overview | ~500 | + Section headings, key points |
| Standard | ~1,500 | + Main content, truncated examples |
| Full | Variable | Complete SKILL.md |
| Complete | Variable | + scripts/ + references/ |

### 0.10 Robot Mode: The AI-Native CLI Pattern

Every CLI tool in the flywheel ecosystem supports **robot mode**—machine-readable JSON output for programmatic consumption.

**Why robot mode is essential:**

1. **Agent tool use:** LLMs can reliably parse JSON, not human-formatted tables
2. **Composability:** Tools can call other tools and process results
3. **Stability:** JSON schemas are versioned; human output changes frequently
4. **Completeness:** JSON includes metadata humans wouldn't want to see

**Robot mode convention:**

[Code block omitted: ms CLI examples; commands include search]

**Robot mode rules:**
- stdout = data only (valid JSON)
- stderr = diagnostics, progress, errors
- Exit code 0 = success, non-zero = failure
- Schema documented and versioned

### 0.11 The Problem ms Solves

**Current state (without ms):**

1. **Skill creation is manual:** Writing skills (spec + compiled SKILL.md) from scratch requires:
   - Remembering what patterns worked
   - Articulating tacit knowledge explicitly
   - Formatting correctly for Claude Code
   - Testing and iterating

2. **Skills are scattered:** Stored in various places:
   - `~/.claude/skills/`
   - `.claude/skills/` in projects
   - Random GitHub repos
   - Local directories

3. **No discovery:** No way to search skills by content or find related skills

4. **No sharing infrastructure:** Sharing skills requires manual file copying

5. **No learning loop:** Successful coding sessions don't automatically improve skills

**Future state (with ms):**

1. **Skill creation from history:**
   ```bash
   ms build --from-cass "how I debug memory leaks"
   # Mines sessions, extracts patterns, generates skill draft
   # Interactive refinement until satisfied
   # Published skill captures real-world patterns
   ```

2. **Unified registry:**
   ```bash
   ms index  # Discovers all skills from configured paths
   ms search "async error"  # Finds relevant skills instantly
   ```

3. **Context-aware suggestions:**
   ```bash
   ms suggest --cwd /data/projects/rust-api
   # Suggests skills based on project type, current files, recent commands
   ```

4. **Sharing infrastructure:**
   ```bash
   ms bundle create my-rust-skills --tags rust
   ms bundle publish my-rust-skills --repo user/skill-bundle
   # Others install with:
   ms bundle install user/skill-bundle
   ```

5. **Learning flywheel:**
   - Agents work → Sessions recorded by CASS
   - ms mines CASS → Skills generated
   - Skills loaded → Agents work better
   - Better work → Better sessions → Repeat

### 0.12 Key Terminology Reference

| Term | Definition |
|------|------------|
| **Skill** | A SKILL.md file with optional scripts/references that extends agent capabilities |
| **Session** | Complete log of agent-user interaction (tool calls, responses, etc.) |
| **CASS** | Coding Agent Session Search—indexes and searches sessions |
| **xf** | X archive search CLI—architectural template for ms |
| **Agent Mail** | Coordination system for multi-agent messaging and file reservations |
| **NTM** | Named Tmux Manager—spawns and orchestrates multiple agents |
| **BV/Beads** | Lightweight issue tracking designed for agent workflows |
| **Robot mode** | JSON output mode for programmatic tool consumption |
| **RRF** | Reciprocal Rank Fusion—combines multiple ranking signals |
| **Progressive disclosure** | Revealing skill content incrementally based on need |
| **Token density** | Information per token—high density = efficient skills |
| **Dual persistence** | Writing to both SQLite (queries) and Git (audit) |
| **Hash embeddings** | Deterministic embeddings without ML models |
| **FTS5** | SQLite's full-text search extension |
| **Tantivy** | Rust search library (Lucene port) for BM25 |

### 0.13 Reference Implementations

This plan draws from three production codebases:

**1. xf (X Archive Search)** — `/data/projects/xf`
- ~23,000 LOC Rust
- SQLite + Tantivy + hash embeddings
- Auto-update via GitHub Releases
- Robot mode JSON output
- *Contribution:* Overall architecture, search, CLI patterns

**2. CASS (Coding Agent Session Search)** — `/data/projects/coding_agent_session_search`
- Multi-agent session indexing
- Hybrid search with RRF fusion
- Robot mode API
- *Contribution:* Session mining, pattern extraction integration

**3. Agent Mail** — `/data/projects/mcp_agent_mail`
- SQLite + Git dual persistence
- File reservation system
- MCP protocol implementation
- *Contribution:* Dual persistence pattern, coordination patterns

### 0.14 Success Criteria for This Plan

This plan should enable:

1. **Complete understanding:** A reader unfamiliar with these tools can evaluate the plan
2. **Implementation guidance:** An LLM can implement ms following this plan
3. **Architecture decisions:** Trade-offs are documented with rationale
4. **Testable milestones:** Each phase has clear deliverables
5. **Extension points:** Future features have designated integration points

### 0.15 Real-World Motivating Example: The UI/UX Polish Session

This example from an actual session illustrates exactly what ms should extract and generalize:

[Code block omitted: example block (lang='n/a', 16 line(s)).]

**What ms extracts from this session:**

| Specific Instance | Generalized Pattern | Skill Category |
|-------------------|---------------------|----------------|
| "Added handleScroll() call on mount" | Check initial state on component mount | React Patterns |
| "Added prefersReducedMotion check" | Always respect `prefers-reduced-motion` | Accessibility |
| "Added aria-hidden on decorative icons" | Decorative elements need aria-hidden | Accessibility |
| "transition: all is too broad" | Use specific transition properties | CSS Best Practices |
| "Deep codebase audit" command | Systematic audit workflow | Review Methodology |

**The transformation:**
1. **Specific:** "I fixed this exact bug in hero.tsx"
2. **General:** "When reviewing React/Next.js UIs, always check for these 8 categories of issues"
3. **Skill:** A complete "nextjs-ui-polish" skill with checklists, examples, and THE EXACT PROMPTS

---

## 1. Vision & Philosophy

### 1.1 The Core Insight

Skills are crystallized wisdom from successful coding sessions. Instead of manually writing skills from scratch, we can **mine existing session history** to extract patterns that actually worked. CASS already indexes thousands of coding sessions—ms transforms that raw material into polished, production-ready skills.

[Diagram omitted: workflow/system diagram.]
- 10 line(s); see surrounding text for step labels.

### 1.2 What ms Does

1. **Index** - Discover and catalog all skills across configured paths
2. **Load** - Progressive disclosure of skill content to agents
3. **Suggest** - Context-aware skill recommendations
4. **Bundle** - Package skills for sharing (GitHub, local, enterprise)
5. **Build** - The killer feature: generate skills from CASS sessions
6. **Maintain** - Auto-update, versioning, deprecation tracking

### 1.3 Design Principles

| Principle | Implementation |
|-----------|----------------|
| **Follow xf exactly** | Same Rust patterns, same crate choices, same file layout |
| **SQLite + Git dual persistence** | From mcp_agent_mail: structured data in SQLite, human-readable in Git |
| **Robot mode everywhere** | Every command has `--robot` JSON output for automation |
| **Progressive disclosure** | Skills reveal depth as needed, not all at once |
| **Idempotent operations** | Safe to run any command multiple times |
| **Offline-first** | Full functionality without network, sync when available |

---

## 2. Architecture Overview

### 2.1 High-Level Components

[Diagram omitted: workflow/system diagram.]
- 24 line(s); see surrounding text for step labels.

### 2.2 Data Flow

[Diagram omitted: workflow/system diagram.]
- 31 line(s); see surrounding text for step labels.

### 2.3 File Layout (Following xf Pattern)

[Diagram omitted: workflow/system diagram.]
- 81 line(s); see surrounding text for step labels.

**Runtime Artifacts:**
- `.ms/skillpack.bin` (or per-skill pack objects) caches parsed spec, slices,
  embeddings, and predicate analysis for low-latency load/suggest.
- Markdown remains a compiled view; runtime uses the pack by default.

---

## 3. Core Data Models

### 3.1 Skill Structure

[Code block omitted: Rust structs: Skill, SkillSpec, SkillSectionSpec; enums: SkillBlockSpec, Platform, NetworkRequirement; fns: default, default_required; fields: id, metadata, body, assets, source, computed.]

### 3.2 SQLite Schema

[Code block omitted: SQL schema; tables include skills, skill_aliases, skill_embeddings, skill_packs, skill_slices, ...; indexes=10, triggers=3.]

### 3.3 Git Archive Structure (Human-Readable Persistence)

[Diagram omitted: workflow/system diagram.]
- 45 line(s); see surrounding text for step labels.

### 3.4 Dependency Graph and Resolution

Skills declare dependencies (`requires`), capabilities (`provides`), and environment requirements
(platforms, tools, env vars) in metadata.
ms builds a dependency graph to resolve load order, detect cycles, and auto-load prerequisites.

[Code block omitted: Rust structs: DependencyGraph, DependencyEdge, ResolvedDependencyPlan; enums: DependencyLoadMode; fns: resolve; fields: nodes, edges, skill_id, depends_on, ordered, missing.]

Default behavior: `ms load` uses `DependencyLoadMode::Auto` (load dependencies
at `overview` disclosure, root skill at the requested level).

#### 3.4.1 Skill Aliases and Deprecation

Renames are inevitable. ms preserves backward compatibility by maintaining
alias mappings (old id → canonical id) and surfacing deprecations with explicit
replacements.

[Code block omitted: Rust structs: AliasResolver, AliasResolution; fns: resolve; fields: db, canonical_id, alias_type, replaced_by.]

**Behavior:**
- `ms load legacy-id` resolves to canonical skill and emits a warning if deprecated.
- `ms search` and `ms suggest` exclude deprecated skills by default unless explicitly requested.
- If `deprecated.replaced_by` is set, ms highlights the replacement in output.
- Indexing upserts alias records from `metadata.aliases` and from `deprecated.replaced_by`
  (alias_type = `deprecated`), and rejects collisions with existing skill ids.

### 3.5 Layering and Conflict Resolution

Skills can exist in multiple layers. Higher layers override lower layers when
conflicts occur.

Layer order (default):
[Code block omitted: example block (lang='n/a', 1 line(s)).]

**Layered Skill Registry:**

[Code block omitted: Rust structs: LayeredRegistry, ResolvedSkill, ConflictDetail; enums: ConflictStrategy, MergeStrategy, ConflictResolution; fns: effective; fields: layers, registries, skill, conflicts, section, higher_layer.]

**Resolution Rules:**
- If only one layer provides a skill, use it directly.
- If multiple layers provide the same skill id:
  - Prefer higher layer by default
  - If both edit the same section, record a conflict detail
  - If conflict strategy is `interactive`, require explicit choice
  - If merge strategy is `prefer_sections`, keep higher-layer rules/pitfalls but
    append or preserve lower-layer examples/references when non-identical

**Conflict Auto-Diff and Merge Policies:**

To reduce manual resolution, ms computes section-level diffs and applies a
merge policy before falling back to interactive mode.

[Code block omitted: Rust structs: ConflictMerger; fns: resolve, section_diff, merge_sections; fields: higher, lower, strategy, merge_strategy, skill, conflicts.]

When conflicts remain, ms surfaces a guided diff in `ms resolve` showing the
exact section differences and suggested merges.

**Block-Level Overlays:**

Beyond whole-skill overrides, higher layers can provide **overlay files** that patch
specific block IDs without copying the entire skill. This enables surgical policy
additions and reduces duplication/drift.

[Code block omitted: Rust structs: SkillOverlay; enums: OverlayOp; fns: apply_overlays; fields: skill_id, layer, operations, OverlayOp.]

**Overlay File Format:**

Overlays are stored in the layer's skill directory as `skill.overlay.json`:

[Code block omitted: JSON payload; keys include skill_id, operations, type, block_id, content, items, ....]

**Benefits:**

- **No duplication:** Org/user layers don't copy entire skills
- **Drift prevention:** Base skill updates propagate automatically
- **Surgical policy:** Add compliance rules without rewriting
- **Clear provenance:** Each block records which layer modified it

### 3.6 Skill Spec and Deterministic Compilation

SKILL.md is a rendered artifact. The source-of-truth is a structured `SkillSpec`
that can be deterministically compiled into SKILL.md. This ensures reproducible
output, stable diffs, and safe automated edits.

[Code block omitted: Rust structs: SkillCompiler; enums: CompileTarget; fns: compile, validate.]

By default, `ms build` outputs `skill.spec.json`, then compiles it to SKILL.md.
SKILL.md is always generated; direct edits are blocked by default.

**Round-Trip Editing (Spec ↔ Markdown):**
- `ms edit <skill>` opens a structured view, parses edits back into `SkillSpec`,
  and re-renders `SKILL.md` deterministically.
- `ms edit --import-markdown` (or `ms repair`) can ingest Markdown diffs into
  spec with warnings and a provenance note, but remains opt-in.
- The compiler emits `spec.lens.json` to map block IDs to byte ranges so edits
  can be attributed to the correct spec blocks.
- If parsing fails, `--allow-lossy` permits a best-effort import with warnings.
- `ms fmt` re-renders from spec; `ms diff --semantic` compares spec blocks.

**Agent Adapters (Multi-Target Compile):**
- `ms compile --target claude|openai|cursor|generic-md`
- Same `SkillSpec`, different frontmatter and optional tool-call hints.

**Semantic Diff Everywhere:**
- `ms review <skill>` shows spec-level changes grouped by rule type.
- Conflict resolution and bundle updates default to semantic diffs.

**Runtime Skillpack Cache:**
- On `ms index`, emit `.ms/skillpack.bin` (or per-skill pack objects) containing:
  parsed spec, pre-tokenized slices, embeddings, predicate pre-analysis, and
  provenance pointers for low-latency `ms suggest/load`.

### 3.7 Two-Phase Commit for Dual Persistence

All writes that touch both SQLite and Git are wrapped in a lightweight two-phase
commit to avoid split-brain states.

[Code block omitted: Rust structs: TxManager, TxRecord; fns: write_skill; fields: db, git, tx_dir, id, entity_type, entity_id.]

Recovery is automatic on startup and via `ms doctor --fix`.

### 3.7.1 Global File Locking

While SQLite handles internal concurrency with WAL mode, the dual-persistence
pattern (SQLite + Git) requires coordination when multiple `ms` processes run
concurrently (e.g., parallel agent invocations, IDE background indexer + CLI).

**Optional Single-Writer Daemon (`msd`):**
- Holds hot indices/caches in memory and serializes writes.
- CLI becomes a thin client when daemon is running (lower p95 latency).

[Code block omitted: Rust structs: GlobalLock; fns: acquire, try_acquire, acquire_timeout; fields: lock_file, lock_path, std.]

**Locked TxManager:**

[Code block omitted: Rust fns: write_skill_locked, write_skills_batch.]

**Lock behavior by command:**

| Command | Lock Type | Rationale |
|---------|-----------|-----------|
| `ms index` | Exclusive | Bulk writes to both stores |
| `ms load` | None (read-only) | SQLite WAL handles read concurrency |
| `ms search` | None (read-only) | FTS queries are read-only |
| `ms suggest` | None (read-only) | Query-only operation |
| `ms edit` | Exclusive | Modifies SkillSpec, re-renders SKILL.md, updates SQLite |
| `ms mine` | Exclusive | Writes new skills |
| `ms calibrate` | Exclusive | Updates rule strengths |
| `ms doctor --fix` | Exclusive | May modify both stores |

**Diagnostics:**

[Code block omitted: ms CLI examples; commands include doctor]

The lock file includes a JSON payload with holder PID and timestamp, enabling
stale lock detection (process no longer running) and diagnostics.

---

## 4. CLI Command Reference

### 4.1 Core Commands

[Code block omitted: ms CLI examples; commands include deps, diff, edit, evidence, index, init, load, requirements...]

### 4.2 Build Commands (CASS Integration)

[Code block omitted: ms CLI examples; commands include build, compile, uncertainties]

### 4.3 Bundle Commands

[Code block omitted: ms CLI examples; commands include bundle]

### 4.4 Maintenance Commands

[Code block omitted: ms CLI examples; commands include doctor, load, prune, simulate, stale, stats, test, update]

### 4.5 Robot Mode (Comprehensive Specification)

Following the xf pattern exactly, robot mode provides machine-readable JSON output for all operations. This enables tight integration with orchestration tools (NTM, BV) and other agents.

**Core Protocol:**

[Diagram omitted: workflow/system diagram.]
- 18 line(s); see surrounding text for step labels.

**Robot Mode Commands:**

[Code block omitted: ms CLI examples; commands include build, doctor, list, load, review, search, show, stats...]

**Output Schemas:**

[Code block omitted: Rust structs: RobotResponse, StatusResponse, RegistryStatus; enums: RobotStatus; fields: status, timestamp, version, data, warnings, registry.]

**Error Response Format:**

[Code block omitted: JSON payload; keys include status, error, code, message, timestamp, version, ....]

**Integration Examples:**

[Code block omitted: ms CLI examples; commands include build]

### 4.6 Doctor Command

The `doctor` command performs comprehensive health checks on the ms installation, following best practices from xf and other Rust CLI tools.

[Code block omitted: ms CLI examples; commands include doctor]

**Check Categories:**

[Code block omitted: Rust structs: DoctorReport, CheckResult; enums: CheckCategory, HealthStatus; fields: checks, overall_status, auto_fixable, check_id, category, status.]

**Checks Performed:**

[Diagram omitted: workflow/system diagram.]
- 94 line(s); see surrounding text for step labels.

**Output Example:**

[Code block omitted: ms CLI examples; commands include ]

### 4.7 Shell Integration

Shell integration provides aliases, completions, and environment setup.

[Code block omitted: ms CLI examples; commands include ]

**Generated Shell Functions:**

[Code block omitted: ms CLI examples; commands include build, load]

**Shell Completions:**

[Code block omitted: shell commands using _ms(), local, commands=(, 'search:Search, 'list:List, 'show:Show, 'alias:Manage, 'requirements:Check....]

### 4.8 MCP Server Mode

Beyond CLI, ms provides a **Model Context Protocol (MCP) server** for native agent
tool-use integration. This eliminates subprocess overhead, PATH issues, JSON parsing
brittleness, and platform differences.

**Why MCP matters:** CLI + JSON parsing works but is brittle. MCP is the native
interface for agent tool calling. Every modern agent (Claude Code, Codex CLI, Cursor)
can consume ms via MCP with dramatically less friction.

**Server Commands:**

[Code block omitted: ms CLI examples; commands include mcp]

**MCP Tool Definitions:**

[Code block omitted: Rust structs: MsSearch, MsSuggest, MsLoad; fields: query, filters, limit, context, budget_tokens, skill_id.]

**Server Architecture:**

[Code block omitted: Rust structs: McpServer; fns: serve_stdio, serve_tcp; fields: registry, cache, protocol, tokio.]

**Benefits over CLI:**

| Aspect | CLI Mode | MCP Mode |
|--------|----------|----------|
| Latency | ~50-100ms subprocess | ~1-5ms in-process |
| Caching | Per-invocation | Shared across requests |
| Streaming | Not supported | Partial results supported |
| Error handling | Exit codes + stderr | Structured error responses |
| Type safety | JSON schema drift risk | Schema-validated tools |

**Claude Code Integration:**

[Code block omitted: JSON payload; keys include ms, command, args, env.]

---

## 5. CASS Integration Deep Dive

### 5.1 The Mining Pipeline

[Diagram omitted: workflow/system diagram.]
- 68 line(s); see surrounding text for step labels.

### 5.2 Pattern Types

[Code block omitted: Rust structs: ExtractedPattern; enums: PatternType; fields: commands, frequency, contexts, language, code, purpose.]

**Pattern IR (Typed Intermediate Representation):**

[Code block omitted: Rust enums: PatternIR.]

### 5.3 CASS Client Implementation

[Code block omitted: Rust structs: CassClient, FingerprintCache; fns: search, get_session, incremental_sessions; fields: cass_bin, data_dir, fingerprint_cache, serde_json, db.]

### 5.4 Interactive Build Session Flow

[Diagram omitted: workflow/system diagram.]
- 62 line(s); see surrounding text for step labels.

### 5.5 The Guided Iterative Mode (Hours-Long Autonomous Skill Generation)

This is a **killer feature**: ms can run autonomously for hours, systematically mining your session history to produce a comprehensive skill library tailored to YOUR approach.

**The Problem It Solves:**
- Manual skill creation is tedious and incomplete
- You've solved thousands of problems but captured none of them
- Your personal patterns and preferences aren't documented anywhere
- Starting from scratch means rediscovering solutions you already found

**The Vision:**

[Diagram omitted: workflow/system diagram.]
- 37 line(s); see surrounding text for step labels.

**Shared State Machine (Guided vs Autonomous):**
- Guided mode and autonomous mode share the same state machine.
- Autonomous = guided with zero user input; guided = autonomous with checkpoints.
- One recovery path reduces drift and improves reliability.

**Steady-State Detection:**

From your planning-workflow skill, we adopt the "iterate until steady state" pattern:

[Code block omitted: Rust structs: SteadyStateDetector; enums: SteadyStateResult; fns: is_steady, canonical_embedding; fields: min_iterations, similarity_threshold, max_token_delta, max_quality_delta, min_evidence_coverage, max_no_improvement_iters.]

**Autonomous Quality Rubric:**

The guided mode self-critiques each draft against this rubric:

| Criterion | Weight | Check |
|-----------|--------|-------|
| **Token Density** | 25% | Information per token exceeds threshold |
| **Actionability** | 25% | Contains concrete commands/code, not just advice |
| **Structure** | 20% | Has CRITICAL RULES, examples, troubleshooting |
| **Specificity** | 15% | References YOUR patterns, not generic wisdom |
| **Completeness** | 15% | Covers topic without obvious gaps |

**Interactive Checkpoints:**

Even in autonomous mode, ms pauses for user input at key moments:

[Code block omitted: Rust enums: CheckpointTrigger.]

**CLI Interface:**

[Code block omitted: ms CLI examples; commands include build]

### 5.6 Specific-to-General Transformation Algorithm

This is the core intellectual innovation: extracting universal patterns ("inner truths") from specific instances.
The same pipeline is applied to counter-examples to produce "Avoid / When NOT to use" rules.

**The Transformation Pipeline:**

[Diagram omitted: workflow/system diagram.]
- 52 line(s); see surrounding text for step labels.

**Optional LLM-Assisted Refinement (Pluggable):**
- If configured, a local model critiques the candidate generalization for overreach,
  ambiguous scope, or missing counter-examples.
- Critique summaries are stored with the uncertainty item so humans can adjudicate.
- If no model is available, the pipeline remains heuristic-only.

**The Algorithm:**

[Code block omitted: Rust structs: SpecificToGeneralTransformer, RefinementCritique; fns: critique, transform, extract_structure; fields: cass, embedder, uncertainty_queue, refiner, min_instances, confidence_threshold.]

**Generalization Confidence Scoring:**

[Code block omitted: Rust structs: GeneralizationValidation, CounterExample; enums: CounterExampleReason; fns: compute; fields: coverage, predictive_power, coherence, specificity, confidence, counterexamples.]

### 5.7 Skill Deduplication and Personalization

**No Redundancy Across Skills:**

[Diagram omitted: workflow/system diagram.]
- 47 line(s); see surrounding text for step labels.

**Implementation:**

[Code block omitted: Rust structs: SkillDeduplicator; fns: check_overlap, recommend_action; fields: embedder, registry, semantic_threshold, uniqueness_threshold, existing_skill, semantic_similarity.]

**Personalization ("Tailored to YOUR Approach"):**

[Code block omitted: Rust structs: PersonalizationEngine, StyleProfile; fns: build_from_sessions, personalize; fields: style_profile, tool_preferences, naming_conventions, prompt_patterns, indentation, comment_style.]

### 5.8 Tech Stack Detection and Specialization

Different tech stacks require different skills. ms auto-detects your project's stack:

[Code block omitted: Rust structs: TechStackDetector; enums: TechStack; fns: detect, suggest_for_stack; fields: indicators, secondary, confidence, TechStack.]

**Toolchain Detection and Drift:**

[Code block omitted: Rust structs: ProjectToolchain, ToolchainDetector, ToolchainMismatch; fns: detect, detect_toolchain_mismatches; fields: node, rust, go, nextjs, react, tool.]

**Stack-Specific Mining:**

[Code block omitted: ms CLI examples; commands include build]

### 5.9 The Meta Skill Concept

The **meta skill** is a special skill that guides AI agents in using `ms` itself. This creates a recursive self-improvement loop where agents use skills to build better skills.

#### The Core Insight

[Diagram omitted: workflow/system diagram.]
- 19 line(s); see surrounding text for step labels.

#### The Meta Skill Content

[Code block omitted: example block (lang='markdown', 16 line(s)).]
# What topics have enough sessions for skill extraction?
ms coverage --min-sessions 5

# Find pattern clusters in session history
ms analyze --cluster --min-cluster-size 3

# What skills already exist?
ms list --format=coverage
[Code block omitted: example block (lang='n/a', 1 line(s)).]
# Guided interactive build (recommended)
ms build --guided --topic "UI/UX fixes"

# Single-shot extraction from recent sessions
ms build --from-cass "error handling" --since "7 days" --output draft.md

# Hours-long autonomous generation
ms build --guided --duration 4h --checkpoint-interval 30m
[Code block omitted: example block (lang='n/a', 8 line(s)).]
# Add to your skill registry
ms add ./draft-skill/

# Update skill index
ms index --refresh

# Verify skill works
ms suggest "scenario that should trigger this skill"
[Code block omitted: example block (lang='n/a', 7 line(s)).]
Specific Session Example           General Pattern
─────────────────────────────────────────────────────────
"Fixed aria-hidden on SVG" ────► "Decorative elements need aria-hidden"
"Added motion-reduce class" ────► "All animations need reduced-motion support"
"Changed transition-all" ────► "Use specific transition properties"
[Code block omitted: example block (lang='n/a', 2 line(s)).]
# What topics have sessions but no skills?
ms coverage --show-gaps

# What skill categories are underrepresented?
ms stats --by-category

# Suggest next skill to build based on session frequency
ms next --suggest-build
[Code block omitted: example block (lang='n/a', 1 line(s)).]
# 1. I've done many UI/UX fix sessions recently
ms analyze --topic "UI fixes" --days 30
# Output: Found 23 sessions with 156 extractable patterns

# 2. Start guided build
ms build --guided --topic "UI/UX fixes" --stack nextjs-react

# 3. Interactive session begins...
# - ms presents pattern clusters
# - I approve/reject/refine each
# - Draft skill emerges

# 4. Validate and integrate
ms overlap ./draft-skill/  # Check for duplicates
ms validate ./draft-skill/ # Best practices check
ms add ./draft-skill/      # Add to registry
[Code block omitted: empty or placeholder block.]

#### Meta Skill Generation Algorithm

[Code block omitted: Rust structs: MetaSkillGenerator, MetaSkillMetrics; fns: analyze_meta_usage, self_improve; fields: cass, ms_registry, meta_skill_version, total_uses, success_rate, common_errors.]

#### The Self-Improvement Loop

[Diagram omitted: workflow/system diagram.]
- 24 line(s); see surrounding text for step labels.

**CLI Commands for Meta Skill:**

[Code block omitted: ms CLI examples; commands include meta]

### 5.10 Long-Running Autonomous Generation with Checkpointing

The user's vision emphasizes hours-long autonomous skill generation sessions. This requires robust checkpointing, recovery, and progress tracking.

#### The Long-Running Session Problem

[Diagram omitted: workflow/system diagram.]
- 17 line(s); see surrounding text for step labels.

#### Checkpoint Architecture

[Code block omitted: Rust structs: CheckpointManager, GenerationCheckpoint, SkillInProgress; enums: GenerationPhase; fns: save, load_latest, resume; fields: checkpoint_dir, checkpoint_interval, max_checkpoints, id, build_id, sequence.]

#### Autonomous Generation Orchestrator

[Code block omitted: Rust structs: AutonomousOrchestrator, AutonomousConfig; fns: default, run, report_progress; fields: cass, transformer, checkpoint_mgr, deduplicator, quality_scorer, config.]

**CLI Commands:**

[Code block omitted: ms CLI examples; commands include build]

**Progress Output Example:**

[Code block omitted: example block (lang='n/a', 38 line(s)).]

### 5.11 Session Marking for Skill Mining

Allow users to mark sessions during or after completion as good candidates for skill extraction. This creates explicit training data for skill generation.

#### The Session Marking Problem

[Diagram omitted: workflow/system diagram.]
- 15 line(s); see surrounding text for step labels.

#### Marking Data Model

[Code block omitted: Rust structs: SessionMark, SessionHighlight, SessionMarkStore; enums: MarkType, HighlightType; fns: mark, get_for_topic, get_exemplary; fields: session_id, session_path, mark_type, topics, tech_stack, quality_rating.]

#### CLI Commands for Session Marking

[Code block omitted: ms CLI examples; commands include mark, marks]

#### Integration with Skill Building

[Code block omitted: Rust structs: MarkedSessionBuilder; fns: build_from_marked; fields: cass, mark_store, transformer, topic, opts.]

**Example Workflow:**

[Code block omitted: shell commands using $, --reason, --quality, Exemplary, ★★★★★, ★★★★☆, Ignored, xyz789.]

Anti-pattern markings are treated as counter-examples and flow into a dedicated
"Avoid / When NOT to use" section during draft generation.

### 5.12 Evidence and Provenance Graph

Evidence links are first-class: every rule in a generated skill should be traceable back
to concrete session evidence. ms builds a lightweight provenance graph that connects:

[Code block omitted: example block (lang='n/a', 1 line(s)).]

This makes skills auditable, merge-safe, and self-correcting.

**Provenance Compression (Pointer + Fetch):**
- Level 0: hash pointers + message ranges (cheap default)
- Level 1: minimal redacted excerpt for quick review
- Level 2: expandable context fetched from CASS on demand

**Provenance Graph Model:**

[Code block omitted: Rust structs: ProvenanceGraph, ProvNode, ProvEdge; enums: ProvNodeType; fields: nodes, edges, id, node_type, label, from.]

**CLI Examples:**

[Code block omitted: ms CLI examples; commands include evidence]

**Actionable Evidence Navigation:**

Provenance is only valuable if humans can quickly validate and refine rules.
ms provides direct jump-to-source workflows that call CASS to expand context.

[Code block omitted: Rust structs: EvidenceNavigator, ExpandedEvidence, ExpandedEvidenceItem; fns: expand_evidence, cache_evidence; fields: cass_client, evidence_cache, skill_id, rule_id, context_lines, session_id.]

**Jump-to-Source CLI:**

[Code block omitted: ms CLI examples; commands include evidence]

### 5.13 Redaction and Privacy Guard

All CASS transcripts pass through a redaction pipeline before pattern extraction.
This prevents secrets, tokens, and PII from ever entering generated skills,
evidence excerpts, or provenance graphs.

**Reassembly Resistance:**
- Redaction assigns stable `secret_id` values so multiple partial excerpts cannot
  be combined to reconstruct a secret across rules/evidence.
- High-risk secret types are blocked from excerpt storage entirely.

**Redaction Report Model:**

[Code block omitted: Rust structs: RedactionReport, RedactionFinding, RedactionLocation; enums: RedactionKind, SecretType, RedactionRisk; fields: session_id, findings, redacted_tokens, risk_level, created_at, kind.]

**Redactor Interface:**

[Code block omitted: Rust structs: Redactor; fns: redact; fields: rules, allowlist, min_entropy.]

**CLI Examples:**

[Code block omitted: ms CLI examples; commands include build, doctor]

**Taint Tracking Through Mining Pipeline:**

Beyond binary redaction, ms tracks **taint labels** through the entire extraction →
clustering → synthesis pipeline. This ensures unsafe provenance never leaks into
high-leverage artifacts (prompts, rules, scripts).

[Code block omitted: Rust structs: TaintSet, TaintedSnippet, TaintTracker; enums: TaintSource; fns: is_safe_for_prompt, is_safe_for_evidence, classify_message; fields: sources, propagated_from, content, taint, source_location.]

**Taint Policy Enforcement:**

[Code block omitted: Rust structs: TaintPolicy; fns: validate_block; fields: BlockType.]

**CLI Integration:**

[Code block omitted: ms CLI examples; commands include build, doctor, evidence]

### 5.14 Anti-Pattern Mining and Counter-Examples

Great skills include what *not* to do. ms extracts anti-patterns from failure
signals, marked anti-pattern sessions, and explicit “wrong” fixes in transcripts.
These are presented as a dedicated "Avoid / When NOT to use" section and sliced
as `Pitfall` blocks for token packing.

**Symmetric Counterexample Pipeline:**
- Counterexamples are first-class patterns: extraction → clustering → synthesis → packing.
- Link each anti-pattern to the positive rule it constrains (conditionalization).

**Anti-Pattern Extraction Sources:**
- Session marks with `MarkType::AntiPattern`
- Failure outcomes from the effectiveness loop
- Phrases indicating incorrect or insecure approaches

**Draft Integration (example):**

[Code block omitted: appendix/example (lang='n/a', 5 line(s)).]

### 5.15 Active-Learning Uncertainty Queue

When generalization confidence is too low, ms does not discard the pattern. Instead,
it queues the candidate for targeted evidence gathering. This turns "maybe" patterns
into high-quality rules with minimal extra effort.

**Precision Loop (Active Learning):**
- Generate 3–7 targeted CASS queries per uncertainty (positive, negative, boundary).
- Auto-run when idle or via `ms uncertainties --mine` and stop on confidence threshold.

**Uncertainty Queue Flow:**

[Code block omitted: example block (lang='n/a', 1 line(s)).]

**Queue Interface:**

[Code block omitted: Rust structs: UncertaintyItem, DecisionBoundary, UncertaintyQueue; enums: MissingSignal, ResolutionCheck; fns: enqueue, list_pending, resolve; fields: id, pattern_candidate, reason, confidence, suggested_queries, status.]

**CLI Examples:**

[Code block omitted: ms CLI examples; commands include build, uncertainties]

### 5.16 Session Quality Scoring

Not all sessions are equally useful. ms scores sessions for signal quality and
filters out low-quality transcripts before pattern extraction.

**Session Quality Model:**

[Code block omitted: Rust structs: SessionQuality; fns: compute; fields: session_id, score, signals.]

**Usage:**
- Default threshold: `cass.min_session_quality`
- Use `--min-session-quality` to override per build
- Marked sessions (exemplary) get a quality bonus

---

### 5.17 Prompt Injection Defense

ms filters prompt-injection content before pattern extraction. Any session messages
that attempt to override system rules or instruct the agent to ignore constraints
are quarantined and excluded by default.

**Primary Defense: ACIP Integration (v1.3 recommended):**
- Use **ACIP** (`/data/projects/acip`, version 1.3) as the canonical injection
  defense framework, not a bespoke prompt filter.
- Support three deployment modes: direct inclusion, checker-model gate, or hybrid
  audit mode with `ACIP_AUDIT_MODE=ENABLED` tags for operator visibility.
- Pin the ACIP version in config and emit provenance so skills can cite the
  exact defense version in effect during mining.

**Forensic Quarantine Playback:**
- Store snippet hash, minimal safe excerpt, triggered rule, and replay command.
- Replay requires explicit user invocation to expand context from CASS.

**Injection Report Model:**

[Code block omitted: Rust structs: InjectionReport, InjectionFinding; enums: InjectionSeverity; fields: session_id, findings, severity, created_at, pattern, message_index.]

**CLI Examples:**

[Code block omitted: ms CLI examples; commands include build, doctor]

---

### 5.18 Safety Invariant Layer (No Destructive Ops)

ms enforces a hard invariant: destructive filesystem or git operations are never
executed without explicit, verbatim approval. This mirrors the global agent
rules and prevents ms from becoming a footgun.

**Primary Enforcement: DCG Integration:**
- Integrate **Destructive Command Guard (DCG)** (`/data/projects/destructive_command_guard`)
  as the primary runtime guard for destructive commands.
- Leverage DCG’s pack system, heredoc/inline script scanning, explain mode, and
  fail‑open design rather than re‑implementing command semantics in ms.

**Safety Policy Model:**

Safety classification is **effect-based**, not command-string-based. Rather than
pattern-matching on strings like `rm` or `git reset`, we classify by the semantic
effect of what the command does. This is more robust because:
- `rm -rf /` and `find . -delete` have the same effect (file deletion)
- A command with harmless flags (e.g., `rm -i`) is safer than one without
- Novel commands get correct classification based on what they do

**Non-Removable Policy Lenses:**
- Compile critical policies into `Policy` slices and include them via
  `MandatoryPredicate::OfType(SliceType::Policy)` in pack constraints.
- `MandatoryPredicate::Always` is reserved for global invariants (rare).
- Packer fails closed if policy slices are omitted under any pack budget.

[Code block omitted: Rust structs: SafetyPolicy, ApprovalRequest; enums: CommandEffect, SafetyTier, DestructiveOpsPolicy; fns: to_tier; fields: CommandEffect, destructive_ops, require_verbatim_approval, tombstone_deletes, command, effect.]

**Behavior:**
- Destructive commands (delete/overwrite/reset) are blocked by default.
- In robot mode, ms returns `approval_required` with the exact approve hint.
- In human mode, ms prompts for the exact verbatim command string.
- In ms-managed directories, deletions become **tombstones** (content-addressed
  markers); actual pruning is only performed when explicitly invoked.

**Robot Approval Example:**

[Code block omitted: JSON payload; keys include status, approval_required, approve_command, tier, reason, timestamp, ....]

---

## 6. Progressive Disclosure System

### 6.1 Disclosure Levels

[Code block omitted: Rust enums: DisclosureLevel; fns: token_budget; fields: DisclosureLevel.]

### 6.2 Disclosure Logic

[Code block omitted: Rust structs: TokenBudget; enums: DisclosurePlan, PackMode; fns: disclose, disclose_level, disclose_packed; fields: DisclosurePlan, DisclosureLevel, frontmatter, body, scripts, references.]

### 6.3 Context-Aware Disclosure

[Code block omitted: Rust fns: optimal_disclosure; fields: skill, context, mode, max_per_group, DisclosurePlan.]

**Disclosure Context (partial):**

[Code block omitted: Rust structs: DisclosureContext; fields: explicit_level, pack_budget, pack_mode, max_per_group, remaining_tokens, usage_history.]

### 6.4 Micro-Slicing and Token Packing

To maximize signal per token, ms pre-slices skills into atomic blocks (rules,
commands, examples, pitfalls). A packer then selects the highest-utility slices
that fit within a token budget.

**Slice Generation Heuristics:**

- One slice per rule, command block, example, checklist, pitfall, or policy invariant
- Preserve section headings by attaching them to the first slice in the section
- Estimate tokens per slice using a fast tokenizer heuristic
- Assign utility score from quality signals + usage frequency + evidence coverage
- Propagate tags from skill metadata and block annotations into slices

**Token Packer (Constrained Optimization):**

The packer treats slice selection as a constrained optimization problem, not just
greedy selection. This ensures predictable coverage, safer packs, and stable behavior.

**Constraints:**
- Total tokens ≤ budget
- Dependencies satisfied before dependents
- Coverage quotas (e.g., at least 1 from "critical-rules" group)
- Max per group (avoid over-representing one category)
- Risk tier constraints (always include safety warnings)

**Objective:** Maximize total utility with diminishing returns per group.

**Injection-Time Optimization:**
- Apply novelty penalties vs. already-loaded slices in the current prompt/context.
- Boost missing facets (e.g., pitfalls/validation) based on task fingerprint.

**Pack Contracts:**
- `DebugContract`, `RefactorContract`, etc. define mandatory groups/slices.
- Packer fails closed if contract cannot be satisfied within budget.

[Code block omitted: Rust structs: PackConstraints, CoverageQuota, ConstrainedPacker; enums: MandatorySlice, MandatoryPredicate, PackError; fns: pack, matches_mandatory, seed_required_coverage; fields: budget, max_per_group, required_coverage, excluded_groups, max_improvement_passes, mandatory_slices.]

**CLI Example:**

[Code block omitted: ms CLI examples; commands include load]

### 6.5 Conditional Block Predicates

Skills often contain version-specific or environment-specific content. Rather than
maintaining separate skills or relying on the agent to reason about versions,
ms supports **block-level predicates** that strip irrelevant content at load time.

**Markdown Syntax:**

[Code block omitted: example block (lang='markdown', 3 line(s)).]

**Predicate Types:**

| Predicate | Example | Evaluates |
|-----------|---------|-----------|
| `package:<name> <op> <version>` | `package:next >= 16.0.0` | package.json / Cargo.toml version |
| `tool:<name> <op> <version>` | `tool:node >= 18.0.0` | Installed tool version |
| `rust:edition <op> <year>` | `rust:edition == 2021` | Cargo.toml rust edition |
| `env:<var>` | `env:CI` | Environment variable presence |
| `file:<pattern>` | `file:src/middleware.ts` | File/glob existence |

**Operators:** `==`, `!=`, `<`, `<=`, `>`, `>=` (semver-aware for versions)

**Evaluation Flow:**

[Code block omitted: Rust fns: evaluate_predicate, filter_slices_by_predicates; fields: PredicateType, glob, slices, ctx.]

**Why This Matters:**

The agent *cannot* hallucinate using deprecated patterns because those patterns
are physically absent from its context window. This directly addresses the
version drift problem (e.g., Next.js middleware.ts vs proxy.ts) mentioned in
AGENTS.md without requiring separate skills or complex agent reasoning.

**CLI Example:**

[Code block omitted: ms CLI examples; commands include load]

### 6.6 Meta-Skills: Composed Slice Bundles

Agents rarely need a single skill—they need **task kits** combining slices from
multiple related skills. Meta-skills are first-class compositions that persist
and evolve.

**Why Meta-Skills:**

| Without Meta-Skills | With Meta-Skills |
|---------------------|------------------|
| `ms load nextjs-ui && ms load a11y && ms load react-patterns` | `ms load frontend-polish` |
| Manual coordination of 4+ skills | Single load, optimal packing |
| Repeated setup per session | Battle-tested bundle |

**Data Model:**

[Code block omitted: Rust structs: MetaSkill, MetaSkillSliceRef; enums: PinStrategy; fields: id, name, description, slices, pin_strategy, validated_at.]

**CLI Commands:**

[Code block omitted: ms CLI examples; commands include load, meta]

**Resolution and Packing:**

[Code block omitted: Rust fns: resolve, load_packed.]

**Use Cases:**

- **NTM integration:** Define meta-skills per bead type (e.g., `ui-polish-bead`, `api-refactor-bead`)
- **Onboarding:** Ship `team-standards` meta-skill bundling all org-required rules
- **Tech stack kits:** `rust-cli-complete`, `nextjs-fullstack`, `go-microservice`

---

## 7. Search & Suggestion Engine

### 7.1 Hybrid Search (Following xf Pattern)

[Code block omitted: Rust structs: HybridSearcher; fns: search; fields: tantivy_index, embedding_index, rrf_k, query, filters, limit.]

**Alias + Deprecation Handling:**
- If the query exactly matches a skill alias, ms resolves to the canonical skill id.
- Deprecated skills are filtered out by default (use `--include-deprecated` to show them).

### 7.2 Context-Aware Suggestion

[Code block omitted: Rust structs: Suggester; fns: suggest, trigger_boost, explain_result; fields: searcher, registry, requirements, bandit, skill, signals.]

When `--for-ntm` is used, `ms suggest` returns `swarm_plan` in robot mode so
each agent can load a complementary slice pack instead of duplicating content.

**Bandit-Weighted Signal Selection:**
- A contextual bandit learns per-project weighting over signals (bm25, embeddings,
  triggers, freshness, project match) using usage/outcome rewards.
- Replaces static tuning with adaptive, self-optimizing retrieval.

[Code block omitted: Rust structs: SignalBandit; fields: arms, prior.]

**Suggestion Context (partial):**

[Code block omitted: Rust structs: SuggestionContext; fields: cwd, current_file, recent_commands, query, pack_budget, explain.]

**Requirement-aware suggestions:**

[Code block omitted: Rust structs: EnvironmentSnapshot, RequirementStatus, RequirementChecker; enums: NetworkStatus; fns: is_satisfied, summary, check; fields: platform, tools, env_vars, network, platform_ok, missing_tools.]

**Collective Pack Planning (Swarm / NTM):**

[Code block omitted: Rust structs: SwarmContext, SwarmPlan, AgentPack; enums: PackObjective, SwarmRole; fns: plan_swarm_packs; fields: agent_count, budget_per_agent, objective, replicate_pitfalls, agents, total_tokens.]

### 7.2.1 Context Fingerprints & Suggestion Cooldowns

To prevent `ms suggest` from spamming the same skills repeatedly when context hasn't meaningfully changed, we compute a **context fingerprint** and maintain a cooldown cache.

[Code block omitted: Rust structs: ContextFingerprint; fns: compute, differs_from, fingerprint_hash; fields: repo_root, git_head, diff_hash, open_files_hash, recent_commands_hash.]

**Cooldown Cache:**

[Code block omitted: Rust structs: SuggestionCooldownCache, CooldownEntry; fns: load, save, should_suppress; fields: entries, max_entries, skill_ids, suggested_at, fingerprint, std.]

**Integration with Suggester:**

[Code block omitted: Rust structs: CooldownConfig, SuggestionResult; fns: suggest_with_cooldown, default; fields: context, cooldown_config, suggestions, suppressed, reason, fingerprint.]

**CLI flags:**

[Code block omitted: ms CLI examples; commands include suggest]

This mechanism prevents suggestion spam in tight loops (e.g., IDE integrations calling `ms suggest` on every keystroke) while still responding to meaningful context changes like new commits, file edits, or command history.

### 7.3 Hash-Based Embeddings (From xf)

[Code block omitted: Rust fns: hash_embedding.]

### 7.3.1 Pluggable Embedding Backends

Hash embeddings are the default (fast, deterministic, zero dependencies). For
higher semantic fidelity, ms supports an optional local ML embedder.

[Code block omitted: Rust structs: HashEmbedder, LocalMlEmbedder; fns: embed, dims; fields: dims.]

**Selection Rules:**
- Default: `HashEmbedder`
- If `embeddings.backend = "local"` and model available → `LocalMlEmbedder`
- Fallback to hash if local model missing

### 7.4 Skill Quality Scoring Algorithm

Quality scoring determines which skills are most worth surfacing to agents. This section details the multi-factor scoring algorithm, including provenance (evidence coverage and confidence).

[Code block omitted: Rust structs: QualityScorer, QualityWeights, QualityScore; fns: default, score, score_structure; fields: weights, usage_tracker, toolchain_detector, project_path, structure_weight, content_weight.]

**Quality Issue Types:**

[Code block omitted: Rust enums: QualityIssue.]

**CLI Integration:**

[Code block omitted: ms CLI examples; commands include quality]

**Quality-Based Filtering:**

[Code block omitted: Rust fns: filter_by_quality; fields: skills, min_score, scorer.]

---

### 7.5 Skill Pruning & Evolution

As the registry grows, ms must keep skills lean and current without destructive
deletions. Pruning is **proposal-first**: identify candidates, suggest merges
or deprecations, and require explicit confirmation before applying changes.

**Signals:**
- Low recent usage (e.g., <5 uses in 30 days)
- Low quality score (e.g., <0.3)
- High similarity to another skill (e.g., >= 0.8)
- Persistent toolchain mismatch or stale indicators

**Actions (non-destructive by default):**
- Propose merge: "Combine X and Y into Z" with auto-generated draft
- Propose deprecate: mark as deprecated + replacement alias
- Propose split: break overly broad skills into focused children
- Emit BV beads for review and scheduling

**CLI Example:**

[Code block omitted: ms CLI examples; commands include prune]

## 8. Bundle & Distribution System

### 8.1 Bundle Format

[Code block omitted: YAML example; keys include name, version, channel, description, author, license, ....]

### 8.2 GitHub Integration

[Code block omitted: Rust fns: publish_to_github; fields: bundle, config, description, private, tag_name, name.]

### 8.3 Installation Flow

[Diagram omitted: workflow/system diagram.]
- 29 line(s); see surrounding text for step labels.

### 8.4 Sharing with Local Modification Safety

The sharing system allows one-URL distribution of all your skills while preserving local customizations when upstream changes arrive.

#### The Three-Tier Storage Model

[Diagram omitted: workflow/system diagram.]
- 27 line(s); see surrounding text for step labels.

#### Local Modification Data Model

[Code block omitted: Rust structs: LocalModification, ConflictInfo; enums: SkillSyncStatus, Resolution; fields: skill_id, upstream_bundle, base_version, patch, created_at, updated_at.]

#### The Sync Engine

[Code block omitted: Rust structs: SyncEngine; fns: sync, create_modification, backup_local_mods; fields: upstream_dir, local_mods_dir, merged_dir, backup_dir, skill_id, new_content.]

#### One-URL Sharing

Share all your skills (including local modifications) via a single URL:

[Code block omitted: Rust fns: generate_share_url; fields: skills, local_mods, config, version, created_at, local_modifications.]

**CLI Commands:**

[Code block omitted: ms CLI examples; commands include share]

#### Sync Status Dashboard

[Diagram omitted: workflow/system diagram.]
- 27 line(s); see surrounding text for step labels.

#### Conflict Resolution Workflow

[Code block omitted: shell commands using $, Syncing, ✓, ⚠, Conflict, Section:, Upstream, -....]

#### Automatic Backup Schedule

[Code block omitted: Rust structs: BackupConfig; fns: default; fields: retention_count, backup_on_sync, backup_on_modify, scheduled_interval.]

**Backup Commands:**

[Code block omitted: ms CLI examples; commands include backup]

### 8.5 Multi-Machine Synchronization

Following the xf pattern for distributed archive access across multiple development machines.

#### 8.5.1 Machine Identity

[Code block omitted: Rust structs: MachineIdentity; fns: generate, load_or_create; fields: machine_id, machine_name, sync_timestamps, std.]

#### 8.5.2 Sync State Tracking

[Code block omitted: Rust structs: SyncState, SkillSyncState, RemoteConfig; enums: SkillSyncStatus, RemoteType, SyncDirection; fields: skill_states, remotes, last_full_sync, skill_id, local_modified, remote_modified.]

#### 8.5.3 Conflict Resolution

[Code block omitted: Rust structs: ConflictResolver, ConflictInfo, SkillVersion; enums: ConflictStrategy, ConflictType, Resolution; fns: resolve, attempt_three_way_merge; fields: default_strategy, skill_strategies, skill_id, local_version, remote_version, base_version.]

#### 8.5.4 Sync Engine

[Code block omitted: Rust structs: SyncEngine, SyncReport; fns: sync, quick_sync, watch_and_sync; fields: machine_identity, sync_state, conflict_resolver, skills_db, SyncChange, Utc.]

#### 8.5.5 CLI Commands

[Code block omitted: ms CLI examples; commands include conflicts, remote, sync]

#### 8.5.6 Robot Mode for Multi-Machine

[Code block omitted: ms CLI examples; commands include --robot-resolve, --robot-sync]

#### 8.5.7 Sync Configuration

[Code block omitted: TOML config; sections include machine, sync, remotes.origin, remotes.origin.auth, remotes.backup, remotes.work, ....]

---

## 9. Auto-Update System (Following xf Pattern)

### 9.1 Update Check

[Code block omitted: Rust structs: Updater, UpdateInfo; fns: check, install; fields: current_version, github_repo, binary_name, version, download_url, release_notes.]

### 9.2 Release Workflow

[Code block omitted: YAML example; keys include name, on, push, tags, jobs, build, ....]

---

## 10. Configuration System

### 10.1 Config File Structure

[Code block omitted: TOML config; sections include general, compiler, cache, bandit, disclosure, pack_contracts, ....]

### 10.2 Project-Local Config

[Code block omitted: TOML config; sections include project, triggers.]

---

## 11. Implementation Phases

### Phase 1: Foundation

[Diagram omitted: workflow/system diagram.]
- 17 line(s); see surrounding text for step labels.

### Phase 2: Search

[Diagram omitted: workflow/system diagram.]
- 14 line(s); see surrounding text for step labels.

### Phase 3: Disclosure & Suggestions

[Diagram omitted: workflow/system diagram.]
- 17 line(s); see surrounding text for step labels.

### Phase 4: CASS Integration

[Diagram omitted: workflow/system diagram.]
- 16 line(s); see surrounding text for step labels.

### Phase 5: Bundles & Distribution

[Diagram omitted: workflow/system diagram.]
- 15 line(s); see surrounding text for step labels.

### Phase 6: Polish & Auto-Update

[Diagram omitted: workflow/system diagram.]
- 17 line(s); see surrounding text for step labels.

**Reordered Phasing (Hard Invariants First):**
1. Spec-only editing + compilation + semantic diff
2. Index + skillpack artifacts + fast suggest/load
3. Provenance compression + taint/reassembly resistance
4. Mining pipeline + Pattern IR
5. Swarm orchestration + bandit scoring
6. TUI polish + bundles + auto-update

---

## 12. Dependencies (Cargo.toml)

[Code block omitted: TOML config; sections include package, bin, dependencies, dev-dependencies, profile.release.]

---

## 13. Key Design Decisions

### 13.1 Why Hash Embeddings Instead of ML Models

| ML Embeddings | Hash Embeddings |
|---------------|-----------------|
| Requires model download (100MB+) | Zero dependencies |
| GPU/CPU inference overhead | Pure CPU, instant |
| Version lock-in | Always reproducible |
| Network dependency for updates | Fully offline |
| Black box | Transparent algorithm |

The hash embedding approach from xf provides 80-90% of ML embedding quality for skill matching, with none of the operational complexity.
For teams that need higher semantic fidelity, ms supports an **optional local ML embedder**
that is still offline and fully opt-in.

### 13.2 Why SQLite + Git Dual Persistence

[Diagram omitted: workflow/system diagram.]
- 17 line(s); see surrounding text for step labels.

**Two-Phase Commit for Consistency**

To avoid partial writes (SQLite updated but Git not, or vice versa), ms wraps every
write in a two-phase commit (2PC) protocol with a durable write-ahead record.

[Code block omitted: example block (lang='n/a', 10 line(s)).]

This makes dual persistence crash-safe and idempotent.

### 13.3 Why Interactive Build Over Fully Automated

The iterative, interactive build process is essential because:

1. **Quality requires judgment** - Not all extracted patterns are good
2. **Context is king** - Human knows what's actually useful
3. **Refinement improves output** - Each iteration focuses the skill
4. **Learning opportunity** - User sees what patterns exist in their sessions

Fully automated mode (`--auto`) is available for pipelines, but interactive is the default for quality.

---

## 14. Future Extensions

### 14.1 Planned Features

| Feature | Description | Priority |
|---------|-------------|----------|
| **Skill composition** | Combine multiple skills into workflows | P1 |
| **Team sharing** | Enterprise-grade skill distribution | P1 |
| **Skill analytics** | Usage patterns, effectiveness metrics | P2 |
| **IDE plugins** | VS Code, JetBrains integration | P2 |
| **Multi-agent coordination** | Skill assignment to agent swarms | P2 |
| **Semantic versioning** | Track skill changes, migrations | P3 |
| **Skill testing** | Validate skills against example scenarios | P3 |
| **Skill bounties** | Prioritize requests with credits/bounties | P3 |

### 14.2 Integration Points

[Diagram omitted: workflow/system diagram.]
- 24 line(s); see surrounding text for step labels.

---

## 15. Success Metrics

### 15.1 Technical Metrics

| Metric | Target |
|--------|--------|
| Indexing speed | 1000 skills/second |
| Search latency | <50ms p99 |
| Memory usage | <100MB idle |
| Binary size | <20MB stripped |
| Build session start | <2 seconds |

### 15.2 User Experience Metrics

| Metric | Target |
|--------|--------|
| Skill suggestion relevance | >80% useful |
| Build session completion rate | >60% |
| Bundle installation success | >95% |
| Time to first useful skill | <5 minutes |

---

## 16. Appendix: THE EXACT PROMPTS

### 16.1 Pattern Extraction Prompt

[Code block omitted: example block (lang='n/a', 17 line(s)).]

### 16.2 Draft Generation Prompt

[Code block omitted: example block (lang='n/a', 17 line(s)).]

### 16.3 Refinement Prompt

[Code block omitted: example block (lang='n/a', 13 line(s)).]

---

## 17. Getting Started

[Code block omitted: appendix/example (lang='bash', 17 line(s)).]

---

## 18. Testing Strategy

### 18.1 Testing Philosophy

Following Rust best practices with comprehensive coverage across unit, integration, and property-based tests.

**UBS (Ultimate Bug Scanner) Integration:**
- Integrate `/data/projects/ultimate_bug_scanner` as a required quality gate.
- Run `ubs` on changed files before commits and during CI; surface findings in `ms doctor`.
- Prefer machine-readable outputs (JSON/SARIF) for automation and bead creation.

**Testing Beads Coverage:**
- Create dedicated beads for unit tests, integration tests, E2E scripts, and benchmarks.
- Treat testing beads as first-class blockers in planning/triage.

[Code block omitted: Rust types/logic.]

### 18.2 Unit Tests

[Code block omitted: Rust fns: test_fnv1a_deterministic, test_embedding_dimensions, test_embedding_normalization; fields: name, description, id, content.]
example code
[Code block omitted: example block (lang='n/a', 19 line(s)).]

### 18.3 Integration Tests

[Code block omitted: Rust fns: test_init_creates_config, test_index_skills_directory, test_search_returns_results; fields: Command, std, name, description.]

### 18.4 Property-Based Tests

[Code block omitted: Rust fns: test_skill_id_generation_unique, test_embedding_always_normalized, test_search_never_panics; fields: name, description.]

### 18.5 Snapshot Tests

[Code block omitted: Rust fns: test_skill_disclosure_minimal, test_skill_disclosure_full, test_robot_status_output.]

### 18.6 Benchmark Tests

[Code block omitted: Rust fns: bench_hash_embedding, bench_search, bench_rrf_fusion; fields: BenchmarkId.]

### 18.7 Test Fixtures and Helpers

[Code block omitted: Rust structs: TestFixture; fns: new, with_indexed_skills, with_mock_cass; fields: temp_dir, config_dir, skills_dir, db, index, std.]

### 18.8 CI Integration

[Code block omitted: appendix/example (lang='yaml', 67 line(s)).]

### 18.9 Skill Tests

Skills can include executable tests to validate correctness. Tests are stored
under `tests/` and run via `ms test`.

**Test Format (YAML):**

[Code block omitted: YAML example; keys include name, skill, steps, - load_skill, - run, - assert, ....]

**Runner Contract:**
- `load_skill` injects the selected disclosure
- `run` executes a command or script
- `assert` checks stdout/stderr patterns or file outputs

**CLI:**

[Code block omitted: ms CLI examples; commands include test]

**Extended Test Types:**

Beyond basic schema/script tests, ms supports **retrieval tests** and **packing tests**
to enable regression testing of search quality and token efficiency.

[Code block omitted: YAML example; keys include name, skill, type, tests, - context, cwd, ....]

[Code block omitted: YAML example; keys include name, skill, type, tests, - budget, contract, ....]

**Test Harness Implementation:**

[Code block omitted: Rust structs: RetrievalTest, PackingTest, SkillTestHarness; fns: run_retrieval_test, run_packing_test; fields: name, skill, query, context, expect, budget.]

**CI Integration:**

[Code block omitted: ms CLI examples; commands include test]

---

### 18.10 Skill Simulation Sandbox

Simulate a skill end-to-end in a controlled workspace before publishing. This
catches broken commands, missing assumptions, and brittle steps without touching
real projects.

**Behavior:**
- Create a temporary workspace with mock files/tools
- Execute skill steps (or mapped test steps) in order
- Capture stdout/stderr and compare against expected assertions
- Emit a simulation report and optional example transcript

**CLI:**

[Code block omitted: ms CLI examples; commands include simulate]

---

## 19. Skill Templates Library

### 19.1 Template System Overview

Pre-built templates for common skill patterns, enabling rapid skill creation with best practices baked in.

[Code block omitted: Rust structs: TemplateLibrary, SkillTemplate, TemplateStructure; enums: TemplateCategory, ContentType; fields: templates, custom_templates_dir, id, name, description, category.]

### 19.2 Built-in Templates

#### 19.2.1 Workflow Template

[Code block omitted: example block (lang='markdown', 12 line(s)).]
{{step_1_code}}
[Code block omitted: example block (lang='n/a', 3 line(s)).]
{{decision_point}} ?
├── YES → {{yes_action}}
└── NO → {{no_action}}
[Code block omitted: example block (lang='n/a', 10 line(s)).]

#### 19.2.2 Checklist Template

[Code block omitted: example block (lang='markdown', 31 line(s)).]

#### 19.2.3 Debugging Template

[Code block omitted: example block (lang='markdown', 8 line(s)).]
{{diagnostic_command}}
[Code block omitted: example block (lang='n/a', 6 line(s)).]
{{fix_code}}
[Code block omitted: example block (lang='n/a', 6 line(s)).]
{{symptom}}
├── Check: {{check_1}}
│   ├── PASS → {{next_check}}
│   └── FAIL → {{cause_1}} → {{fix_1}}
└── Check: {{check_2}}
    └── FAIL → {{cause_2}} → {{fix_2}}
[Code block omitted: example block (lang='n/a', 2 line(s)).]

#### 19.2.4 Integration Template

[Code block omitted: example block (lang='markdown', 10 line(s)).]
{{setup_commands}}
[Code block omitted: example block (lang='n/a', 1 line(s)).]
{{config_example}}
[Code block omitted: example block (lang='n/a', 2 line(s)).]
{{operation_1_command}}
[Code block omitted: example block (lang='n/a', 1 line(s)).]
{{operation_2_command}}
[Code block omitted: example block (lang='n/a', 11 line(s)).]

#### 19.2.5 Pattern Template

[Code block omitted: example block (lang='markdown', 13 line(s)).]
{{pattern_structure}}
[Code block omitted: example block (lang='n/a', 2 line(s)).]
{{basic_implementation}}
[Code block omitted: example block (lang='n/a', 1 line(s)).]
{{advanced_implementation}}
[Code block omitted: example block (lang='n/a', 3 line(s)).]
{{variation_1_code}}
[Code block omitted: example block (lang='n/a', 7 line(s)).]

### 19.3 Template CLI Commands

[Code block omitted: ms CLI examples; commands include template]

### 19.4 Template Instantiation Engine

[Code block omitted: Rust structs: TemplateEngine; fns: new, instantiate, interactive_instantiate; fields: templates, handlebars, template_id, values, name, pattern.]

### 19.5 Template Discovery from Sessions

[Diagram omitted: workflow/system diagram.]
- 57 line(s); see surrounding text for step labels.

### 19.6 Template Validation

[Code block omitted: Rust structs: TemplateValidator; fns: validate; fields: rules.]

---

## 20. Agent Mail Integration for Multi-Agent Skill Coordination

### 20.1 Overview

The `ms` CLI integrates with the Agent Mail MCP server to enable multi-agent skill coordination. When multiple agents work on the same project, they need to:

1. **Share discovered patterns** in real-time
2. **Coordinate skill generation** to avoid duplication
3. **Request skills** from other agents who may have relevant expertise
4. **Notify** when new skills are ready for use

[Diagram omitted: workflow/system diagram.]
- 25 line(s); see surrounding text for step labels.

### 20.2 Agent Mail Client Integration

[Code block omitted: Rust structs: AgentMailClient, SkillRequestBounty; enums: SkillRequestUrgency; fns: register_skill_builder, announce_build_start, announce_bounty; fields: project_key, agent_name, mcp_endpoint, topics, topic, estimated_duration.]

**Reservation-Aware Editing (Fallback):**
- If Agent Mail is unavailable, ms provides a local reservation mechanism with
  compatible semantics (path/glob, TTL, exclusive/shared).
- When Agent Mail is available, ms bridges to it transparently.

### 20.3 Coordination Protocol

[Diagram omitted: workflow/system diagram.]
- 26 line(s); see surrounding text for step labels.

### 20.4 CLI Commands with Agent Mail

[Code block omitted: ms CLI examples; commands include build, inbox, request, respond, subscribe]

### 20.5 Pattern Sharing Between Agents

[Code block omitted: Rust structs: PatternSharer; fns: share_relevant_patterns, receive_patterns; fields: mail_client, local_patterns, recipient, topic, source_agent, patterns.]

### 20.6 Multi-Agent Skill Swarm

When building skills at scale with multiple agents (via NTM), coordinate using this pattern:

[Code block omitted: Rust structs: SkillSwarm; fns: distribute_topics, find_best_agent; fields: agents, topic_allocator.]

---

## 21. Interactive Build TUI Experience

### 21.1 TUI Layout

The interactive build experience uses a rich terminal UI for guided skill generation:

[Diagram omitted: workflow/system diagram.]
- 52 line(s); see surrounding text for step labels.

### 21.2 TUI Components

[Diagram omitted: workflow/system diagram.]
- 135 line(s); see surrounding text for step labels.

### 21.3 TUI Navigation and Actions

[Code block omitted: Rust structs: BuildDialogs; fns: handle_key, toggle_current_pattern, accept_pattern; fields: KeyCode, TuiAction, title, fields, EditField, placeholder.]

### 21.4 Real-Time Draft Generation

[Code block omitted: Rust structs: LiveDraftGenerator; fns: regenerate_preview, estimate_quality; fields: transformer, debounce, last_generation, selected_patterns, current_draft, content.]

---

## 22. Skill Effectiveness Feedback Loop

### 22.1 Overview

Track whether skills actually help agents accomplish their tasks. This data improves skill quality scores and informs future skill generation.
When multiple variants exist, ms can run A/B experiments to select the most effective version.

**Slice-Level Experiments:**
- Experiments can target individual slices (rule wording, example blocks) while keeping
  the rest of the skill constant for faster convergence.

[Diagram omitted: workflow/system diagram.]
- 24 line(s); see surrounding text for step labels.

### 22.2 Usage Tracking

[Code block omitted: Rust structs: EffectivenessTracker, SkillExperiment, ExperimentVariant; enums: ExperimentScope, AllocationStrategy, ExperimentStatus; fns: record_skill_load, analyze_session_outcome, infer_outcome; fields: db, cass, id, skill_id, scope, scope_id.]

### 22.3 Feedback Collection

[Code block omitted: Rust structs: FeedbackCollector; fns: collect_feedback_interactive, infer_feedback; fields: tracker, positives, improvements, helpful_sections, confusing_sections, rating.]

### 22.4 Quality Score Updates

[Code block omitted: Rust structs: QualityUpdater; fns: update_quality, generate_improvements; fields: scorer, tracker, db, suggestion_type, priority, evidence.]

### 22.4.1 A/B Skill Experiments

When multiple versions of a skill exist (e.g., different wording, structure, or
examples), ms can run A/B experiments to empirically determine the more effective
variant. Results feed back into quality scoring and can automatically promote the
winning version.

[Code block omitted: Rust structs: ExperimentRunner, ExperimentResult, VariantStats; fns: create_experiment, assign_variant, evaluate; fields: tracker, db, skill_id, variants, allocation, id.]

### 22.5 CLI Commands for Effectiveness

[Diagram omitted: workflow/system diagram.]
- 29 line(s); see surrounding text for step labels.

---

## 23. Cross-Project Learning and Coverage Analysis

### 23.1 Overview

Learn from sessions across multiple projects to build more comprehensive skills and identify coverage gaps.

**CM (cass-memory) Integration:**
- Integrate `/data/projects/cass_memory_system` as a shared procedural memory layer.
- Unify rule IDs, confidence decay, and anti-pattern promotion across ms and CM.
- Provide import/export bridges so CM playbooks and ms skills reinforce each other.

[Diagram omitted: workflow/system diagram.]
- 32 line(s); see surrounding text for step labels.

### 23.2 Cross-Project Pattern Extraction

[Code block omitted: Rust structs: CrossProjectAnalyzer, ProjectInfo, UniversalPattern; fns: find_universal_patterns, find_tech_specific_patterns, normalize_pattern; fields: cass, projects, path, name, tech_stack, session_count.]

### 23.3 Coverage Gap Analysis

[Code block omitted: Rust structs: CoverageAnalyzer, KnowledgeGraph, GraphNode; enums: NodeType, EdgeRelation, SkillSuggestion; fns: find_gaps, batch_compute_coverage, get_or_build_skill_index; fields: cass, skill_registry, search, nodes, edges, id.]

### 23.4 CLI Commands for Coverage

[Diagram omitted: workflow/system diagram.]
- 38 line(s); see surrounding text for step labels.

---

## 24. Error Recovery and Resilience

### 24.1 Overview

Robust error handling for long-running autonomous skill generation, including network failures, LLM errors, and system interruptions.

[Diagram omitted: workflow/system diagram.]
- 15 line(s); see surrounding text for step labels.

### 24.2 Error Taxonomy and Retryability Classification

All errors in `ms` are classified by their retryability to prevent wasteful retry attempts and surface permanent failures immediately.

[Code block omitted: Rust enums: MsError, RetryDecision; fns: retry_policy, exit_code, hint; fields: provider, retry_after, MsError, RetryDecision.]

### 24.3 Retry System

[Code block omitted: Rust structs: RetryConfig, RetryExecutor; fns: default, execute; fields: max_retries, initial_delay, max_delay, backoff_multiplier, jitter, config.]

### 24.3 Rate Limit Handler

[Code block omitted: Rust structs: RateLimitHandler, RateLimitState; fns: should_wait, update_from_headers, parse_reset_timestamp; fields: limits, queue, provider, requests_remaining, tokens_remaining, reset_at.]

### 24.4 Checkpoint Recovery

[Code block omitted: Rust structs: CheckpointRecovery, RecoverableSession, RecoveryOption; enums: RecoveryAction, DataLoss; fns: find_recoverable, is_recoverable, analyze_recovery_options; fields: checkpoint_dir, recovery_options, name, description, action, data_loss.]

### 24.5 Graceful Degradation

[Code block omitted: Rust structs: GracefulDegradation, HealthEndpoints, HealthStatus; fns: default, execute_with_fallback, health_check; fields: cass_available, network_available, cache, health_endpoints, cass, llm_providers.]

### 24.6 CLI Commands for Recovery

[Code block omitted: ms CLI examples; commands include build]

---

## 25. Skill Versioning and Migration System

### 25.1 Overview

Track skill versions semantically and provide migration paths when skills evolve.

[Diagram omitted: workflow/system diagram.]
- 22 line(s); see surrounding text for step labels.

### 25.2 Version Data Model

[Code block omitted: Rust structs: SkillVersion, BreakingChange, VersionHistory; enums: MigrationAction; fns: migration_path, generate_migration_steps; fields: version, changelog, breaking_changes, migration_from, created_at, author.]

### 25.3 Version Tracking

[Code block omitted: SQL schema; tables include skill_versions, installed_skills; indexes=2, triggers=0.]

[Code block omitted: Rust structs: VersionManager; enums: BumpType; fns: create_version, get_latest_available, get_installed; fields: db, git, skill_id, bump_type, changelog, breaking_changes.]

### 25.4 Migration Runner

[Code block omitted: Rust structs: MigrationRunner, MigrationPlan, MigrationResult; fns: check_migration_needed, run_migration, execute_step; fields: version_manager, from, to, migrations, manual_steps, success.]

### 25.5 CLI Commands for Versioning

[Code block omitted: ms CLI examples; commands include version]

---

## 26. Real-World Pattern Mining: CASS Insights

This section documents actual patterns discovered by mining CASS sessions. These represent the "inner truths" that `ms build` should extract and transform into skills.

### 26.1 Discovered Skill Candidates

#### Pattern 1: UI Polish Checklist (from brenner_bot sessions)

**Source Sessions:** `/home/ubuntu/.claude/projects/-data-projects-brenner-bot/agent-a9a6d6d.jsonl`

**Recurring Categories:**
[Diagram omitted: workflow/system diagram.]
- 20 line(s); see surrounding text for step labels.

**Report Format (from sessions):**
[Code block omitted: example block (lang='n/a', 3 line(s)).]

**Inner Truth → Skill:**
[Code block omitted: YAML example; keys include name, description, tags.]

---

#### Pattern 2: Iterative Convergence (from automated_plan_reviser_pro)

**Source Sessions:** `/home/ubuntu/.claude/projects/-data-projects-automated-plan-reviser-pro/`

**The Convergence Pattern:**
> "Specifications improve through multiple iterations like numerical optimization converging to steady state"

**Round Progression Heuristics:**
[Code block omitted: Rust structs: ConvergenceProfile; fns: default; fields: round_expectations, rounds, label, expected_changes, focus_areas.]

**Steady-State Detection:**
[Code block omitted: Rust fns: detect_steady_state; fields: round_outputs, threshold, SteadyStateResult, final_round, current_delta, estimated_rounds_remaining.]

---

#### Pattern 3: Brenner Principles Extraction (from brenner_bot)

**Methodology Pattern:**
Sessions reveal extraction of "AppliedPrinciples" from specific instances:

[Code block omitted: Rust structs: AppliedPrinciple; fns: extract_principles; fields: name, explanation, source_line, confidence, session_content, principle_keywords.]

**Inner Truth:** Domain expertise can be encoded as keyword → principle mappings, then extracted from sessions automatically.

---

#### Pattern 4: Accessibility Standards (multi-project)

**Recurring Pattern Across Sessions:**
[Code block omitted: example block (lang='typescript', 12 line(s)).]

---

### 26.2 Pattern-to-Skill Transformation Examples

#### Example: UI Polish → Generated Skill

**Input (aggregated from 15+ sessions):**
- 47 instances of touch-manipulation additions
- 23 instances of active:scale-* additions
- 18 instances of focus-visible corrections
- 12 instances of aria-label additions
- 8 instances of useReducedMotion additions

**Generated Skill Draft:**

[Code block omitted: example block (lang='markdown', 62 line(s)).]

---

### 26.3 Cluster Analysis Insights

The CASS searches revealed natural clustering:

| Cluster | Sessions | Key Terms | Potential Skill |
|---------|----------|-----------|-----------------|
| UI Polish | 15+ | touch-manipulation, focus-visible, aria | `ui-polish-nextjs` |
| Accessibility | 12+ | reduced-motion, aria-label, a11y | `react-accessibility` |
| Iterative Refinement | 8+ | rounds, convergence, steady-state | `iterative-spec-refinement` |
| Code Review | 10+ | fresh eyes, systematic, checklist | `code-review-methodology` |
| Error Handling | 7+ | try-catch, Result, error boundary | `error-handling-patterns` |

---

### 26.4 CASS Query Patterns for Skill Mining

**Effective queries discovered:**

[Code block omitted: shell commands using cass.]

**Query expansion strategy:**
1. Start with exact phrase: `"inner truth"`
2. Expand to component terms: `inner`, `truth`, `abstract`
3. Add synonyms: `general`, `principles`, `universal`
4. Add domain context: `pattern`, `extract`, `lesson`

---

### 26.5 Inner Truth Extraction Algorithm

Based on session analysis, here's the refined extraction algorithm:

[Code block omitted: Rust structs: InnerTruthExtractor; fns: default, extract; fields: generalization_markers, specificity_markers, min_pattern_occurrences, occurrences, sessions, confidence.]

---

### 26.6 Future CASS Integration Enhancements

Based on mining experience, these CASS features would improve skill generation:

1. **Semantic clustering API**: `cass cluster --by-topic --limit 10`
2. **Cross-session patterns**: `cass patterns --min-occurrences 3`
3. **Project filtering**: `cass search "query" --workspace /data/projects/brenner_bot`
4. **Time-range filtering**: `cass search "query" --since "2025-12-01"`
5. **Agent filtering**: `cass search "query" --agent claude_code`

---

## 27. Appendix: Raw CASS Mining Results

### A.1 UI Polish Session Excerpts

**Session:** `agent-a9a6d6d.jsonl` (brenner_bot)
**Key Finding:** Comprehensive UI polish report format

[Code block omitted: example block (lang='n/a', 10 line(s)).]

### A.2 Iterative Refinement Session Excerpts

**Session:** automated_plan_reviser_pro exploration
**Key Finding:** Round progression pattern

[Code block omitted: example block (lang='n/a', 4 line(s)).]

### A.3 Accessibility Pattern Excerpts

**Multi-project recurring pattern:**
[Code block omitted: example block (lang='tsx', 7 line(s)).]

---

## Section 28: The Brenner Method for Skill Extraction

*CASS Mining Deep Dive: brenner_bot methodology (P0 bead: meta_skill-4d7)*

### 28.1 Core Insight: Reverse-Engineering Cognitive Architectures

The brenner_bot project provides a methodology for extracting **actionable skills** from CASS sessions. Key insight: **don't summarize—extract the generative grammar**.

> "This is not a summary... It is an attempt to **reverse-engineer the cognitive architecture** that generated those contributions—to find the generative grammar of his thinking."

**Application to meta_skill:** We're not looking for "what happened"—we're looking for **repeatable cognitive moves** that made work successful. These become skills.

### 28.2 The Two Axioms for Skill Extraction

#### Axiom 1: Effective Coding Has a Generative Grammar
Code changes are *generated* by cognitive moves that can be identified and formalized.

#### Axiom 2: Understanding = Ability to Reproduce
A skill is valid only if you can **execute it on new problems**.

### 28.3 The Brenner Loop for Skill Extraction

[Code block omitted: example block (lang='n/a', 8 line(s)).]

### 28.4 Skill Tags (Operator Algebra)

| Tag | Description |
|-----|-------------|
| ProblemSelection | How to pick what to work on |
| HypothesisSlate | Explicit enumeration of approaches |
| ThirdAlternative | Both approaches could be wrong |
| IterativeRefinement | Multi-round improvement |
| RuthlessKill | Abandoning failing approaches |
| Quickie | Pilot experiments to de-risk |
| MaterializationInstinct | "What would I see if true?" |
| InnerTruth | The generalizable principle |

### 28.5 Key Methodological Insights

1. **Seven-Cycle Log Paper Test**: If improvement isn't obvious, skill needs refinement
2. **Multi-Model Triangulation**: Extract from multiple angles, keep convergent patterns
3. **Don't Worry Hypothesis**: Document gaps, don't block on secondary concerns
4. **Exception Quarantine**: Collect failures first, look for patterns before patching

### 28.6 Beads for Further CASS Mining

| Bead | P | Topic |
|------|---|-------|
| meta_skill-4d7 | P0 | Inner Truth/Abstract Principles ✓ |
| meta_skill-hzg | P1 | APR Iterative Refinement |
| meta_skill-897 | P1 | Optimization Patterns |
| meta_skill-z2r | P1 | Performance Profiling |
| meta_skill-dag | P2 | Error Handling |
| meta_skill-f8s | P2 | CI/CD Automation |

### 28.7 Interactive TUI Wizard: `ms mine --guided`

The Brenner extraction loop becomes operable through an interactive TUI that guides users from "some sessions" to "skill + tests" in one flow.

[Diagram omitted: workflow/system diagram.]
- 15 line(s); see surrounding text for step labels.

#### CLI Interface

[Code block omitted: ms CLI examples; commands include mine]

#### TUI Screens

**Screen 1: Session Selection**
[Diagram omitted: workflow/system diagram.]
- 16 line(s); see surrounding text for step labels.

**Screen 2: Cognitive Move Extraction**
[Diagram omitted: workflow/system diagram.]
- 19 line(s); see surrounding text for step labels.

**Screen 3: Third-Alternative Guard**
[Diagram omitted: workflow/system diagram.]
- 18 line(s); see surrounding text for step labels.

**Screen 4: Skill Formalization (Live Editor)**
[Diagram omitted: workflow/system diagram.]
- 26 line(s); see surrounding text for step labels.

**Screen 5: Materialization Test**
[Diagram omitted: workflow/system diagram.]
- 21 line(s); see surrounding text for step labels.

#### Wizard Output Artifacts

On completion, the wizard produces:

[Diagram omitted: workflow/system diagram.]
- 6 line(s); see surrounding text for step labels.

#### Implementation

[Code block omitted: Rust structs: BrennerWizard; enums: WizardState; fns: run, resume; fields: state, sessions, moves, skill_draft, test_results, WizardState.]

---

## Section 29: APR Iterative Refinement Patterns

*CASS Mining Deep Dive: automated_plan_reviser_pro methodology (P1 bead: meta_skill-hzg)*

### 29.1 The Numerical Optimizer Analogy

The APR project reveals a powerful insight: **iterative specification refinement follows the same dynamics as numerical optimization**.

> "It very much reminds me of a numerical optimizer gradually converging on a steady state after wild swings in the initial iterations."

**Application to meta_skill:** When building skills through CASS mining, expect early iterations to produce wild swings (major restructures, foundational changes). Later iterations converge on stable formulations. Don't judge early work—judge the convergence trajectory.

### 29.2 The Convergence Pattern

Refinement progresses through predictable phases:

[Diagram omitted: workflow/system diagram.]
- 9 line(s); see surrounding text for step labels.

| Phase | Rounds | Focus |
|-------|--------|-------|
| **Major Fixes** | 1-3 | Security gaps, architectural flaws, fundamental issues |
| **Architecture** | 4-7 | Interface improvements, component boundaries |
| **Refinement** | 8-12 | Edge cases, optimizations, nuanced handling |
| **Polishing** | 13+ | Final abstractions, converging on steady state |

**Key insight:** In early rounds, reviewers focus on "putting out fires." Once major issues are addressed, they can apply "considerable intellectual energies on nuanced particulars."

### 29.3 Convergence Analytics Algorithm

APR implements a quantitative convergence detector using three weighted signals:

[Code block omitted: example block (lang='n/a', 1 line(s)).]

| Signal | Weight | What It Measures |
|--------|--------|------------------|
| **Output Size Trend** | 35% | Are responses getting shorter? Early rounds produce lengthy analyses; convergence shows as more focused, briefer feedback |
| **Change Velocity** | 35% | Is the rate of change slowing? Measured by comparing delta sizes between consecutive rounds |
| **Content Similarity** | 30% | Are successive rounds becoming more similar? Uses word-level overlap to detect stabilization |

**Interpretation:**
- **Score ≥ 0.75**: High confidence of convergence. The specification is stabilizing.
- **Score 0.50-0.74**: Moderate convergence. Significant work remains but progress is visible.
- **Score < 0.50**: Low convergence. Still in early iteration phase with major changes likely.

**Application to meta_skill:** When refining skills through CASS mining, track these metrics:
1. Are extracted patterns getting shorter/tighter?
2. Is the rate of changes to skill definitions slowing?
3. Are multi-model extractions converging on similar formulations?

### 29.4 Grounded Abstraction Principle

> "Every few rounds, including the implementation document keeps abstract specifications grounded in concrete reality."

**Pattern:** Every 3-4 rounds of abstract refinement, ground the work in concrete implementation:

[Diagram omitted: workflow/system diagram.]
- 19 line(s); see surrounding text for step labels.

**Application to meta_skill:** When extracting skills from CASS sessions, periodically test them:
- Can the skill actually be loaded and executed?
- Does the skill produce expected outputs?
- Do agents understand and apply the skill correctly?

### 29.5 Reliability Features for Long Operations

APR implements several reliability patterns for expensive operations:

#### Pre-Flight Validation
Check all preconditions before starting expensive work:
[Code block omitted: example block (lang='n/a', 5 line(s)).]

**Application to meta_skill:** Before running expensive CASS operations:
- Verify index is up-to-date
- Check disk space for embeddings
- Validate query parameters
- Confirm output paths writable

#### Auto-Retry with Exponential Backoff
[Code block omitted: example block (lang='n/a', 4 line(s)).]

**Application to meta_skill:** Retry transient failures (network, rate limits) with increasing delays.

#### Session Locking
Prevent concurrent operations that could cause corruption:
- File-based locks with timestamp
- Automatic stale lock cleanup
- Clear error messages on lock conflict

### 29.6 Dual Interface Pattern

APR serves two audiences with the same codebase:

| Audience | Interface | Features |
|----------|-----------|----------|
| **Humans** | Beautiful TUI | gum styling, interactive wizards, progress indicators, notifications |
| **Machines** | Robot Mode JSON | Structured output, semantic error codes, pre-flight validation |

[Code block omitted: Rust structs: OutputMode; fields: human, robot.]

**Semantic Error Codes:**
- `ok` - Success
- `not_configured` - No configuration found
- `not_found` - Resource doesn't exist
- `validation_failed` - Preconditions not met
- `dependency_missing` - Required dependency unavailable

### 29.7 Audit Trail Principle

Every operation creates artifacts:
- Output files saved to versioned directories
- Git integration for history
- Logs for debugging
- Metrics for analysis

**Application to meta_skill:**
- Every CASS mining session produces artifacts in `.ms_cache/`
- Extracted skills tracked in Git
- Operation logs preserved for debugging
- Convergence metrics stored for analysis

### 29.8 Design Principles Summary

| Principle | Description |
|-----------|-------------|
| **Iterative Convergence** | Like numerical optimization—expect wild swings early, convergence late |
| **Grounded Abstraction** | Periodically ground abstract work in concrete implementation |
| **Audit Trail** | Every operation creates artifacts; history is preserved |
| **Graceful Degradation** | Fallbacks for missing dependencies (gum → ANSI, global → npx) |
| **Dual Interface** | Beautiful for humans, structured for machines |
| **Secure by Default** | No credential storage, checksum verification, atomic operations |

### 29.9 Updated Beads Table

| Bead | P | Topic | Status |
|------|---|-------|--------|
| meta_skill-4d7 | P0 | Inner Truth/Abstract Principles | ✓ Complete |
| meta_skill-hzg | P1 | APR Iterative Refinement | ✓ Complete |
| meta_skill-897 | P1 | Optimization Patterns | ✓ Complete |
| meta_skill-z2r | P1 | Performance Profiling | ✓ Complete |
| meta_skill-aku | P1 | Security Vulnerability Assessment | ✓ Complete |
| meta_skill-dag | P2 | Error Handling | ✓ Complete |
| meta_skill-f8s | P2 | CI/CD Automation | ✓ Complete |
| meta_skill-hax | P2 | Caching/Memoization | ✓ Complete |
| meta_skill-36x | P2 | Debugging Workflows | ✓ Complete |
| meta_skill-avs | P2 | Refactoring Patterns | ✓ Complete |
| meta_skill-cbx | P2 | Testing Patterns | ✓ Complete |
| meta_skill-6st | P2 | REST API Design | ✓ Complete |

## Section 30: Performance Profiling Patterns

*Source: CASS mining of local coding agent sessions - performance analysis workflows*

### 30.1 Introduction

Performance profiling is critical for building efficient CLI tools. This section synthesizes patterns from real-world performance optimization sessions, covering methodology, tooling, benchmarking, and specific optimization techniques.

### 30.2 Performance Analysis Methodology

A comprehensive performance analysis examines multiple dimensions:

#### Hot Path Identification
Focus analysis on the most frequently executed code:
- Query execution and caching
- Vector/similarity search operations
- Full-text index operations
- I/O pipelines (indexing, storage)
- Database operations
- Parser/connector code

#### Inefficiency Pattern Checklist

| Pattern | Description | Detection |
|---------|-------------|-----------|
| **N+1 Queries** | Fetching in loops | Profile shows repeated DB calls |
| **Unnecessary Allocations** | `String::new()`, `Vec::new()` in hot loops | Heap profiling |
| **Repeated Serialization** | serde overhead in loops | CPU profiling |
| **Linear Scans** | O(n) where hash/binary search works | Code review |
| **Lock Contention** | Mutex/RwLock blocking | Contention profiling |
| **Unbounded Collections** | Growing without limits | Memory profiling |
| **Missing Early Termination** | No short-circuiting | Code review |
| **Redundant Computation** | Same calculation repeated | Memoization analysis |
| **String Operations** | Could use interning | Allocation profiling |
| **Iterator Overhead** | Intermediate collections | Inspect `.collect()` calls |
| **Cache Misses** | Poor memory locality | `perf stat` cache metrics |

### 30.3 Algorithm and Data Structure Opportunities

#### Data Structure Upgrades

| Current | Opportunity | Use Case |
|---------|-------------|----------|
| HashMap | Trie | Prefix operations, autocomplete |
| Linear scan | Bloom filter | Fast negative lookups |
| Range queries | Interval tree | Time-range filtering |
| LRU cache | ARC/LFU | Frequency-biased caching |
| Deduplication | Union-find | Graph-based dedup |
| Cumulative ops | Prefix sums | Running totals |
| Sorting | Priority queue | Top-K selection |

#### Top-K Selection Strategies

[Code block omitted: Rust types/logic.]

### 30.4 SIMD and Vectorization

#### Memory Layout Considerations

| Layout | Description | SIMD Friendly |
|--------|-------------|---------------|
| **AoS** | Array of Structs: `[{x,y,z}, {x,y,z}]` | ❌ Poor |
| **SoA** | Struct of Arrays: `{xs: [], ys: [], zs: []}` | ✅ Excellent |

[Code block omitted: Rust structs: VectorIndex; fields: xs, ys, zs.]

#### SIMD Dot Product Pattern

[Code block omitted: Rust fns: dot_product_simd.]

#### Quantization (F16 Storage)

[Code block omitted: Rust fns: quantize_vector, dequantize_vector.]

### 30.5 Criterion Benchmark Patterns

#### Basic Benchmark Structure

[Code block omitted: Rust fns: bench_operation.]

#### Batched Benchmarks (Setup/Teardown Separation)

[Code block omitted: Rust fields: BatchSize.]

#### Benchmark Groups for Comparison

[Code block omitted: Rust fns: bench_scaling.]

#### Parallel vs Sequential Comparison

[Code block omitted: Rust fns: bench_parallelization.]

### 30.6 Profiling Build Configuration

#### Cargo Profile for Profiling

[Code block omitted: TOML config; sections include profile.profiling.]

#### Profiling Workflow

[Code block omitted: shell commands using RUSTFLAGS="-C, perf, cargo.]

### 30.7 I/O and Serialization Optimization

#### Memory-Mapped Files

[Code block omitted: Rust fns: mmap_read.]

#### JSON Parsing Optimization

[Code block omitted: Rust types/logic.]

### 30.8 Cache Design Patterns

#### LRU Cache with TTL

[Code block omitted: Rust structs: TtlCache; fns: get, insert; fields: cache, ttl.]

#### Fast Hash for Cache Keys

[Code block omitted: Rust types/logic.]

### 30.9 Parallel Processing Patterns

#### Rayon Work-Stealing

[Code block omitted: Rust fields: rayon.]

#### Chunked Processing

[Code block omitted: Rust types/logic.]

### 30.10 Application to meta_skill

| Pattern | Application |
|---------|-------------|
| **Hot Path Analysis** | Profile skill extraction, template rendering, search operations |
| **SIMD** | Vectorize embedding comparisons for semantic skill matching |
| **Criterion Benchmarks** | Measure extraction throughput, template performance |
| **Memory-Mapped Files** | Large session file processing |
| **LRU Cache** | Cache parsed sessions, rendered templates |
| **Parallel Processing** | Batch skill extraction across multiple sessions |
| **Profiling Profile** | Enable flamegraph generation for performance debugging |

### 30.11 Benchmark Results Reference

Real-world improvements observed in CASS codebase:

| Optimization | Before | After | Improvement |
|--------------|--------|-------|-------------|
| Sequential → Parallel search | ~63-135ms | ~2.04ms | **30-50x** |
| Scalar → SIMD dot product | baseline | faster | **4-8x** (typical) |
| HashMap → FxHashMap | baseline | faster | **10-30%** (small keys) |
| String → &str where possible | allocating | zero-copy | **significant** |

### 30.12 Profiling Checklist

Before optimization:
- [ ] Establish baseline measurements with criterion
- [ ] Identify actual hot paths (don't guess)
- [ ] Profile under realistic workloads

During optimization:
- [ ] Make one change at a time
- [ ] Measure after each change
- [ ] Verify correctness with tests

After optimization:
- [ ] Document the improvement with benchmarks
- [ ] Add regression tests if performance is critical
- [ ] Consider adding benchmark to CI

---

*Plan version: 1.8.0*
*Created: 2026-01-13*
*Updated: 2026-01-13*
*Author: Claude Opus 4.5*

## Section 31: Optimization Patterns and Methodology

*Source: CASS mining from optimization sessions across multiple codebases*

This section captures systematic optimization methodologies and specific optimization patterns discovered through CASS analysis of real-world performance work.

### 31.1 Optimization Methodology Framework

Before attempting any optimization, follow this disciplined methodology:

#### A) Baseline Establishment

[Code block omitted: shell commands using cargo, time.]

**Key Principle**: Never optimize without knowing your starting point.

#### B) Profile Before Proposing

[Code block omitted: shell commands using cargo, DHAT=1, strace, perf.]

**Anti-pattern**: Optimizing based on intuition rather than profiling data.

#### C) Equivalence Oracle

Define explicit verification criteria before making changes:

[Code block omitted: Rust structs: OptimizationOracle; fns: verify; fields: golden_outputs, invariants, tolerance.]

#### D) Isomorphism Proof Per Change

Every optimization diff must include proof that outputs cannot change:

[Code block omitted: Rust types/logic.]

#### E) Opportunity Matrix

Rank optimizations by expected value:

| Opportunity | Impact (1-5) | Confidence (1-5) | Effort (1-5) | Score |
|-------------|--------------|------------------|--------------|-------|
| Replace Vec with SmallVec for N<8 | 3 | 5 | 1 | 15.0 |
| Parallelize with Rayon | 4 | 4 | 2 | 8.0 |
| Switch to FxHashMap | 2 | 5 | 1 | 10.0 |
| Implement SIMD dot product | 4 | 3 | 4 | 3.0 |
| Memory-map large files | 5 | 4 | 3 | 6.7 |

**Formula**: Score = (Impact × Confidence) / Effort

#### F) Minimal Diffs

One performance lever per commit:

[Code block omitted: example block (lang='n/a', 2 line(s)).]

Benefits:
- Easier to measure individual impact
- Easier to bisect regressions
- Easier to revert if problems arise

#### G) Regression Guardrails

Add benchmark thresholds to CI:

[Code block omitted: Rust fns: benchmark_critical_path.]

[Code block omitted: YAML example; keys include - name, run.]

### 31.2 Memory Optimization Patterns

#### Zero-Copy Pattern

[Code block omitted: Rust fns: process_data; fields: Cow.]

#### Buffer Reuse Pattern

[Code block omitted: Rust structs: BufferPool; fns: acquire, release; fields: buffers, buffer_size.]

#### String Interning

[Code block omitted: Rust structs: StringInterner; fns: intern; fields: strings, Arc.]

#### Copy-on-Write (Cow) Pattern

[Code block omitted: Rust structs: SkillConfig; fns: default_static, with_name; fields: name, template, tags.]

#### Structure of Arrays (SoA) vs Array of Structures (AoS)

[Code block omitted: Rust structs: SkillAoS, SkillSoA, SkillHybrid; fields: skills, names, descriptions, tags, hot, cold.]

### 31.3 Algorithm and Data Structure Optimizations

#### Trie for Prefix Matching

[Code block omitted: Rust structs: TrieNode; fns: insert, find_prefix_matches; fields: children, is_end, value.]

#### Bloom Filter for Membership Testing

[Code block omitted: Rust structs: BloomFilter; fns: insert, may_contain; fields: bits, num_hashes.]

#### Interval Tree for Range Queries

[Code block omitted: Rust structs: IntervalTree, IntervalNode; fns: query_overlapping; fields: root, interval, max_end, value, left, right.]

#### Segment Tree with Lazy Propagation

[Code block omitted: Rust structs: SegmentTree; fns: range_update, range_query, push_down; fields: tree, lazy, n.]

### 31.4 Advanced Algorithmic Techniques

> **Speculative Section**: The techniques below (Convex Hull Trick, Matrix Exponentiation)
> are included for completeness but are unlikely to be needed for typical CLI tool workloads.
> These are competitive programming techniques that apply to specific mathematical structures.
> Profile before implementing - premature optimization with these patterns adds complexity
> with no benefit if the problem structure doesn't match.

#### Convex Hull Trick for DP Optimization

[Code block omitted: Rust structs: ConvexHullTrick; fns: add_line, query; fields: lines.]

#### Matrix Exponentiation for Linear Recurrences

[Code block omitted: Rust fns: matrix_mult, matrix_pow, fibonacci.]

#### FFT/NTT for Polynomial Multiplication

[Code block omitted: Rust fns: ntt, mod_pow.]

### 31.5 Lazy Evaluation Patterns

#### Lazy Iterator Chains

[Code block omitted: Rust fns: process_skills.]

#### Lazy Loading with OnceCell

[Code block omitted: Rust structs: LazySkillIndex; fns: new, get; fields: path, index, SkillIndex.]

#### Deferred Computation Pattern

[Code block omitted: Rust structs: Deferred; fns: new, get, is_computed; fields: cell, init.]

### 31.6 Memoization with Invalidation

#### Time-Based Cache Invalidation

[Code block omitted: Rust structs: TimedCache; fns: get, insert, evict_expired; fields: entries, ttl.]

#### Version-Based Invalidation

[Code block omitted: Rust structs: VersionedCache, FileCache; fns: get, invalidate; fields: value, cached_version, F, path, cache.]

#### Dependency-Based Invalidation

[Code block omitted: Rust structs: DependencyCache, CacheEntry; fns: invalidate, set_dependency; fields: entries, dependencies, value, valid.]

### 31.7 I/O Optimization Patterns

#### Scatter-Gather I/O

[Code block omitted: Rust fns: write_multiple.]

#### Buffered I/O with Controlled Flushing

[Code block omitted: Rust structs: BatchedWriter; fns: write_item; fields: inner, writes_since_flush, flush_interval.]

#### Async I/O for Concurrent Operations

[Code block omitted: Rust fns: read_all_files.]

### 31.8 Precomputation Patterns

#### Lookup Tables

[Code block omitted: Rust structs: LookupTable; fns: new, to_hex; fields: byte_to_hex.]

#### Compile-Time Computation

[Code block omitted: Rust fns: compute_factorial, factorial.]

#### Static Initialization with LazyLock

[Code block omitted: Rust fns: extract_skill; fields: Regex.]

### 31.9 N+1 Query Elimination

#### Batch Loading Pattern

[Code block omitted: Rust structs: SkillRepository; fns: get_skills_with_tags_bad, get_skills_with_tags_good; fields: db, tags.]

#### DataLoader Pattern

[Code block omitted: Rust structs: DataLoader; fns: load, execute_batch; fields: load_fn, cache, pending.]

### 31.10 Application to meta_skill

| Pattern | Application |
|---------|-------------|
| **Zero-Copy** | Parse session files without copying string data |
| **Buffer Reuse** | Reuse buffers when processing multiple sessions |
| **String Interning** | Deduplicate skill names and tag names |
| **Trie** | Fast prefix matching for skill/command autocomplete |
| **Bloom Filter** | Quick "no match" checks before expensive extraction |
| **Lazy Loading** | Load skill definitions on demand |
| **Memoization** | Cache extracted skills per session file |
| **Batch Loading** | Load all skills for displayed results in one pass |
| **Precomputation** | Pre-compile regex patterns, build lookup tables |
| **Convex Hull** | Optimize ranking/scoring DP if applicable |

### 31.11 Optimization Decision Flowchart

[Diagram omitted: workflow/system diagram.]
- 27 line(s); see surrounding text for step labels.

### 31.12 Optimization Checklist

Before optimizing:
- [ ] Establish golden outputs / equivalence oracle
- [ ] Profile under realistic workload
- [ ] Identify actual bottleneck (don't guess)
- [ ] Calculate opportunity score: (Impact × Confidence) / Effort

During optimization:
- [ ] One change at a time
- [ ] Document isomorphism proof for each change
- [ ] Verify tests still pass
- [ ] Measure improvement

After optimization:
- [ ] Add regression benchmark
- [ ] Document the optimization for future maintainers
- [ ] Consider if optimization adds complexity worth the gain

## Section 32: Security Vulnerability Assessment Patterns

*Source: CASS mining from security audits and vulnerability assessments across multiple codebases*

This section captures systematic security vulnerability assessment methodologies and specific security patterns discovered through CASS analysis of real-world security work.

### 32.1 Security Audit Methodology Framework

#### Systematic Security Review Process

[Code block omitted: Rust structs: SecurityFinding; enums: AuditPhase, Severity; fields: title, severity, file_path, line_number, description, proof_of_concept.]

#### Attack Surface Mapping Checklist

[Code block omitted: example block (lang='markdown', 29 line(s)).]

### 32.2 OWASP-Aligned Vulnerability Categories

#### A01: Broken Access Control

[Code block omitted: Rust fns: verify_authorization; fields: user, resource, action, AccessType.]

#### A02: Cryptographic Failures

[Code block omitted: Rust fns: hash_password, encrypt_data, bad_derive_nonce; fields: argon2, key, plaintext, aad, msg.]

#### A03: Injection

[Code block omitted: Rust fns: get_user_by_email, safe_execute_command, escape_shell_arg; fields: id, email, name, allowed_commands, command, args.]

#### A04: Insecure Design

[Code block omitted: Rust structs: SecureSession; fns: new, validate; fields: id, user_id, created_at, expires_at, last_activity, ip_address.]

#### A05: Security Misconfiguration

[Code block omitted: Rust structs: SecurityConfig; fns: validate, load_secret; fields: cors_origins, rate_limit, tls, secrets, log, std.]

### 32.3 Input Validation Patterns

#### Path Traversal Prevention

[Code block omitted: Rust fns: validate_path, sanitize_filename; fields: Component.]

#### XSS Prevention

[Code block omitted: Rust fns: escape_html, csp_header, sanitize_html; fields: ammonia.]

### 32.4 Authentication Security Patterns

#### JWT Token Management

[Code block omitted: Rust structs: AccessTokenClaims, TokenPair; fns: generate_access_token, validate_access_token, refresh_tokens; fields: sub, email, tier, iss, aud, iat.]

#### OAuth Security

[Code block omitted: Rust structs: PkceChallenge; fns: validate_redirect_url, generate, verify; fields: code_verifier, code_challenge, code_challenge_method.]

### 32.5 Rate Limiting and DoS Protection

#### IP-Based Rate Limiting

[Code block omitted: Rust structs: RateLimiter, RateLimitEntry; fns: check, extract_client_ip; fields: limits, max_requests, window, max_entries, count, window_start.]

#### ReDoS (Regex Denial of Service) Protection

[Code block omitted: Rust structs: SafeRegex; fns: new, is_match; fields: inner, max_input_len.]

### 32.6 Secret Management

#### Environment Variable Security

[Code block omitted: Rust structs: Secret; fns: from_env, expose, validate_required_secrets; fields: std.]

#### API Key Best Practices

[Code block omitted: Rust structs: ApiClient; fns: bad_api_call, good_api_call, from_env; fields: reqwest, base_url, secret.]

### 32.7 Command Execution Security

#### Safe Command Execution Patterns

[Code block omitted: Rust structs: CommandExecutor; fns: execute, validate_argument, analyze_heredoc; fields: allowed_commands, allowed_cwd, command, args, cwd, Command.]

### 32.8 Security Audit Report Template

[Code block omitted: example block (lang='markdown', 9 line(s)).]
// Vulnerable code snippet
[Code block omitted: example block (lang='n/a', 3 line(s)).]
// Fixed code snippet
[Code block omitted: example block (lang='n/a', 26 line(s)).]

### 32.9 Application to meta_skill

| Security Area | Application |
|---------------|-------------|
| **Input Validation** | Validate skill file paths, template inputs, user queries |
| **Path Traversal** | Protect skill repository access, session file access |
| **Command Injection** | Safe execution of skill commands, template rendering |
| **Secret Management** | API keys for external services, embedding model credentials |
| **Authentication** | User sessions for skill customization (if applicable) |
| **Rate Limiting** | Prevent abuse of skill extraction endpoints |
| **Crypto** | Secure storage of user preferences, session encryption |

### 32.10 Security Checklist

Before deployment:
- [ ] All user inputs validated and sanitized
- [ ] SQL queries parameterized (no string interpolation)
- [ ] Command execution whitelisted and argument-validated
- [ ] Path traversal attacks prevented
- [ ] XSS outputs properly escaped
- [ ] Secrets loaded from environment, not hardcoded
- [ ] Rate limiting implemented on public endpoints
- [ ] Authentication tokens properly validated
- [ ] HTTPS enforced in production
- [ ] Security headers configured (CSP, HSTS, X-Frame-Options)
- [ ] Error messages don't leak sensitive information
- [ ] Logging doesn't include secrets or PII
- [ ] Dependencies scanned for known vulnerabilities

---

## Section 33: Error Handling Patterns and Methodology

*CASS-mined insights on robust error handling in Rust applications*

### 33.1 Error Handling Philosophy

Error handling in Rust differs fundamentally from exceptions in other languages. The key principles:

1. **Errors are values** - `Result<T, E>` makes errors explicit and composable
2. **Fail loudly, recover gracefully** - Errors should be visible but recoverable
3. **Context over raw messages** - Error chains explain *why*, not just *what*
4. **Match error types to boundaries** - Different error types for different layers

[Diagram omitted: workflow/system diagram.]
- 8 line(s); see surrounding text for step labels.

### 33.2 The thiserror and anyhow Dichotomy

**thiserror** is for library code - create specific, matchable error types:

[Code block omitted: Rust enums: SkillError; fns: from; fields: SkillError.]

**anyhow** is for application code - rich context chains without ceremony:

[Code block omitted: Rust fns: load_skill.]

### 33.3 Structured CLI Error Types

For CLI applications, create a structured error type that maps to exit codes:

[Code block omitted: Rust structs: CliError; fns: usage, not_found, network; fields: code, kind, message, hint, retryable, serde_json.]

### 33.4 Error Taxonomy Patterns

For protocol or API libraries, define a comprehensive error taxonomy:

[Code block omitted: Rust structs: FcpError; fns: protocol, auth, rate_limited; fields: code, message, retryable, retry_after_ms, details, ai_recovery_hint.]

### 33.5 Error Context Chaining

Build rich error chains that explain the full failure path:

[Code block omitted: Rust fns: execute_skill, from; fields: TemplateError, SkillError, line, reason.]

### 33.6 Error Recovery Patterns

Implement retry logic with exponential backoff:

[Code block omitted: Rust structs: RetryConfig, CircuitBreaker; enums: CircuitState; fns: default, with_retry, new; fields: max_retries, initial_delay, max_delay, backoff_factor, jitter, config.]

### 33.7 Panic vs Result Guidelines

**When to use panic (via `unwrap`, `expect`, `unreachable!`):**

[Code block omitted: Rust fns: pop_from_non_empty_stack, test_parsing; fields: Regex, State.]

**When to use Result (proper error handling):**

[Code block omitted: Rust fns: parse_config, read_settings, fetch_data; fields: toml, reqwest.]

### 33.8 Error Boundary Patterns

For systems with multiple error domains, create clear boundaries:

[Code block omitted: Rust enums: LibraryError, AppError; fns: from, display; fields: LibraryError, message, hint, AppError.]

### 33.9 Error Logging Best Practices

[Code block omitted: Rust fns: save_file, handle_error, log_structured_error; fields: std, AppError, tracing.]

### 33.10 Application to meta_skill

| Error Category | Pattern | Example |
|----------------|---------|---------|
| **Skill Loading** | `thiserror` enum | `SkillError::NotFound`, `SkillError::ParseFailed` |
| **CLI Interface** | `CliError` struct | Exit codes, hints, retryable flags |
| **Template Rendering** | `anyhow` context | Rich failure chains |
| **External APIs** | `FcpError` taxonomy | Error codes, retry hints |
| **Network Operations** | Retry with backoff | `with_retry()` function |
| **Service Stability** | Circuit breaker | Prevent cascading failures |

### 33.11 Error Handling Checklist

Before shipping error handling:
- [ ] All public API functions return `Result` (not `Option` for errors)
- [ ] Error types are appropriate for the layer (library vs application)
- [ ] Error messages are user-friendly (no raw technical jargon)
- [ ] Errors include actionable hints where possible
- [ ] Sensitive information is not leaked in error messages
- [ ] Errors are logged with appropriate severity
- [ ] Retryable errors are clearly marked
- [ ] Exit codes follow Unix conventions
- [ ] Error chains preserve the full context
- [ ] Panics only occur for programming errors, not user input
- [ ] Circuit breakers protect against cascading failures
- [ ] Retry logic includes jitter and backoff

## Section 34: Testing Patterns and Methodology

### 34.1 Testing Philosophy

#### The "NO Mocks" Principle

**Source**: CASS mining of brenner_bot testing landscape analysis

The observed philosophy is: **"NO mocks - test real implementations with real data fixtures."**

[Code block omitted: example block (lang='n/a', 9 line(s)).]

**When to mock**:
1. **Animations**: Mock framer-motion to avoid flaky timing-dependent tests
2. **External APIs**: Stub fetch only when testing HTTP client behavior, not when testing business logic
3. **Time-dependent operations**: Use fakeable clocks, not mocked functions

**When NOT to mock**:
1. File system operations - use `t.TempDir()` (Go) or `mkdtempSync()` (JS)
2. Database operations - use in-memory SQLite or real temp databases
3. Internal functions - if you need to mock internal functions, the design needs refactoring

### 34.2 Test Organization Patterns

#### JavaScript/TypeScript (Vitest/Jest/Bun)

[Code block omitted: example block (lang='typescript', 49 line(s)).]

#### Go Table-Driven Tests

[Code block omitted: example block (lang='go', 36 line(s)).]

#### Test File Naming Conventions

| Language | Pattern | Example |
|----------|---------|---------|
| **TypeScript** | `*.test.ts`, `*.test.tsx` | `copy.test.ts`, `Button.test.tsx` |
| **Go** | `*_test.go` | `evaluator_test.go` |
| **Rust** | `mod tests` in same file, or `/tests/*.rs` | `mod tests { ... }` |
| **Bash** | `*.bats` (BATS framework) | `test_utils.bats` |

### 34.3 Test Fixture Patterns

#### Real Filesystem Fixtures

[Code block omitted: example block (lang='typescript', 32 line(s)).]

#### Go Test Fixtures with t.TempDir()

[Code block omitted: example block (lang='go', 18 line(s)).]

#### Environment Variable Isolation

[Code block omitted: example block (lang='typescript', 14 line(s)).]

### 34.4 Property-Based Testing

#### Rust with proptest

**Source**: CASS mining of destructive_command_guard property tests

[Code block omitted: Rust fns: command_strategy, normalization_is_idempotent, evaluation_is_deterministic.]

#### Key Property Test Categories

| Property | What it tests | Example assertion |
|----------|---------------|-------------------|
| **Idempotence** | `f(f(x)) == f(x)` | Normalization, formatting |
| **Determinism** | Same input → same output | Evaluation, parsing |
| **Safety** | Never panics on any input | Error handling |
| **Bounds** | Handles edge sizes gracefully | Large inputs, empty inputs |
| **Commutativity** | Order doesn't matter | Set operations |
| **Invertibility** | `decode(encode(x)) == x` | Serialization |

### 34.5 Test Coverage Analysis

#### Comprehensive Coverage Report Pattern

**Source**: CASS mining of Go codebase coverage analysis

[Code block omitted: example block (lang='markdown', 25 line(s)).]

### 34.6 Snapshot Testing

#### Vitest/Jest Snapshot Pattern

[Code block omitted: example block (lang='typescript', 15 line(s)).]

#### Managing Snapshot Updates

[Code block omitted: shell commands using bun, git.]

### 34.7 E2E Testing Patterns

#### Playwright Configuration

**Source**: CASS mining of brenner_bot E2E test infrastructure

[Code block omitted: example block (lang='typescript', 41 line(s)).]

#### E2E Test Structure

[Code block omitted: example block (lang='typescript', 25 line(s)).]

### 34.8 BATS Framework for Shell Testing

**Source**: CASS mining of APR BATS test infrastructure

#### Test Helper Structure

[Code block omitted: shell commands using load, setup_test_environment(), export, mkdir, }, teardown_test_environment(), rm, capture_streams()....]

#### Custom Assertions

[Code block omitted: shell commands using assert_stderr_only(), assert, }, assert_stdout_only(), assert_valid_json(), echo, assert_success, assert_json_value()....]

#### Unit Test Example

[Code block omitted: shell commands using setup(), load, setup_test_environment, source, }, teardown(), teardown_test_environment, @test....]

### 34.9 Real Clipboard Testing

**Source**: CASS mining of jeffreysprompts.com copy command tests

[Code block omitted: example block (lang='typescript', 66 line(s)).]

### 34.10 Test Harness Pattern

**Source**: CASS mining of Go testutil.Harness pattern

[Code block omitted: example block (lang='go', 66 line(s)).]

### 34.11 CI Integration Patterns

#### JUnit XML Output for CI

[Code block omitted: shell commands using set, SCRIPT_DIR="$(cd, cd, preflight_check(), echo, if, exit, fi....]

#### GitHub Actions Integration

[Code block omitted: YAML example; keys include name, on, jobs, test, runs-on, steps, ....]

### 34.12 Application to meta_skill

| Test Type | Pattern | Example |
|-----------|---------|---------|
| **Unit Tests** | Table-driven, real fixtures | Skill parsing, template rendering |
| **Integration Tests** | Real filesystem, temp dirs | Skill installation, config loading |
| **Property Tests** | proptest invariants | Template normalization, path handling |
| **E2E Tests** | CLI subprocess | Full workflow: search → select → copy |
| **Snapshot Tests** | Output verification | Rendered skill content |

### 34.13 Testing Checklist

Before shipping tests:
- [ ] Tests use real implementations, not mocks (except for animations/network)
- [ ] File operations use temp directories (`t.TempDir()` or `mkdtempSync()`)
- [ ] Environment variables are isolated and restored
- [ ] Tests are deterministic (no reliance on timing, random values, or external state)
- [ ] Property tests cover invariants (idempotence, determinism, safety)
- [ ] Table-driven tests cover edge cases systematically
- [ ] E2E tests run on multiple browsers/platforms if applicable
- [ ] CI produces JUnit/TAP output for test reporting
- [ ] Flaky tests are marked or fixed (use `it.skip` with explanation)
- [ ] Test coverage gaps are documented and prioritized
- [ ] Snapshot tests are reviewed when updated
- [ ] Tests are organized by type (unit/, integration/, e2e/)

---

## Section 35: CI/CD Automation Patterns

**Source**: CASS mining of repo_updater, apr, jeffreysprompts_premium, flywheel_gateway, and destructive_command_guard CI/CD implementations

### 35.1 GitHub Actions Workflow Architecture

#### Workflow File Organization

**Source**: CASS mining of production GitHub Actions setups

| Workflow File | Purpose | Trigger |
|---------------|---------|---------|
| `ci.yml` | Continuous integration (lint, test, build) | push, pull_request, workflow_dispatch |
| `release.yml` | Release automation with artifacts | `tags: ['v*']` |
| `deploy.yml` | Production deployment | `tags: ['v*']` or manual |
| `e2e.yml` | Full E2E test suite | push to main |
| `dependabot.yml` | Automated dependency updates | schedule |

[Code block omitted: YAML example; keys include name, on, push, branches, pull_request, workflow_dispatch, ....]

### 35.2 Job Dependencies and Ordering

#### Dependency Graph Patterns

[Diagram omitted: workflow/system diagram.]
- 5 line(s); see surrounding text for step labels.

[Code block omitted: YAML example; keys include jobs, lint, runs-on, test, needs, build, ....]

#### Conditional Execution

[Code block omitted: YAML example; keys include jobs, deploy, runs-on, if, steps, - name, ....]

### 35.3 Release Automation

#### Tag-Triggered Releases

**Source**: CASS mining of repo_updater release workflow

[Code block omitted: YAML example; keys include name, on, push, tags, permissions, contents, ....]

### 35.4 Version Management Patterns

#### Dual Version Storage

**Source**: CASS mining of repo_updater version management

[Code block omitted: shell commands using 1.2.1, VERSION="1.2.1", get_version(), local, script_dir="$(dirname, if, cat, else....]

#### Semantic Version Comparison

[Code block omitted: shell commands using version_gt(), local, IFS='.', for, if, return, elif, fi....]

### 35.5 Matrix Testing Strategies

#### Multi-OS Matrix

[Code block omitted: YAML example; keys include jobs, test, strategy, matrix, os, node-version, ....]

#### Browser Matrix for E2E

**Source**: CASS mining of jeffreysprompts_premium E2E workflow

[Code block omitted: YAML example; keys include jobs, e2e, strategy, matrix, browser, include, ....]

### 35.6 Container Image Pipelines

#### Multi-Stage Dockerfile with CI

**Source**: CASS mining of flywheel_gateway tenant container pipeline

[Code block omitted: example block (lang='dockerfile', 13 line(s)).]

[Code block omitted: YAML example; keys include name, on, push, tags, pull_request, paths, ....]

### 35.7 Artifact Management

#### Upload and Download Patterns

[Code block omitted: YAML example; keys include jobs, build, runs-on, steps, - uses, - run, ....]

#### Caching Dependencies

[Code block omitted: YAML example; keys include jobs, test, runs-on, steps, - uses, with, ....]

### 35.8 Automated Dependency Updates

#### Dependabot Configuration

[Code block omitted: YAML example; keys include version, updates, - package-ecosystem, directory, schedule, interval, ....]

### 35.9 Pre-Commit Hook Integration

#### Installing Pre-Commit Hooks

**Source**: CASS mining of destructive_command_guard hook patterns

[Code block omitted: YAML example; keys include jobs, lint, runs-on, steps, - uses, - name, ....]

[Code block omitted: YAML example; keys include repos, - repo, rev, hooks, - id, name, ....]

### 35.10 Deployment Workflows

#### Vercel Deployment

**Source**: CASS mining of jeffreysprompts_premium deploy workflow

[Code block omitted: YAML example; keys include name, on, push, tags, jobs, deploy, ....]

### 35.11 Quality Gates

#### Comprehensive Quality Pipeline

[Code block omitted: YAML example; keys include jobs, quality, runs-on, steps, - uses, - name, ....]

### 35.12 Self-Update Mechanisms

#### CLI Self-Update Pattern

**Source**: CASS mining of apr self-update implementation

[Code block omitted: shell commands using RELEASE_URL="https://github.com/owner/repo/releases/latest/download", update_self(), local, temp_dir=$(mktemp, echo, if, rm, return....]

### 35.13 Application to meta_skill

| CI/CD Component | Pattern | meta_skill Application |
|-----------------|---------|------------------------|
| **CI Pipeline** | Multi-job with dependencies | lint → test → build |
| **Matrix Testing** | OS × Runtime | ubuntu + macos, Node 20/22 |
| **Release** | Tag-triggered | `v*` creates GitHub Release with checksums |
| **Versioning** | Dual storage | VERSION file + CLI --version |
| **Quality Gates** | Lint, type, format, test, build | All must pass before merge |
| **Caching** | Dependency cache | Bun cache by lockfile hash |
| **Artifacts** | Test results, build output | 14-day retention |

### 35.14 CI/CD Checklist

Before shipping CI/CD:
- [ ] CI runs on push and pull_request to main
- [ ] Jobs have appropriate dependencies (`needs:`)
- [ ] Matrix strategy covers target platforms
- [ ] Tests run in TAP/JUnit format for reporting
- [ ] Artifacts uploaded with sensible retention
- [ ] Dependencies cached by lockfile hash
- [ ] Release workflow validates version consistency
- [ ] Checksums generated and published with releases
- [ ] Quality gates include lint, type check, format, test, build
- [ ] Deployment includes smoke tests/health checks
- [ ] Dependabot configured for dependencies and actions
- [ ] Pre-commit hooks run in CI
- [ ] Bundle size monitoring if applicable
- [ ] Container images scanned for vulnerabilities

---

## Section 36: Caching and Memoization Patterns

**Source**: CASS mining of beads_viewer, xf, coding_agent_session_search, and related optimization sessions

### 36.1 Caching Philosophy

#### When to Cache

| Scenario | Cache Strategy | Example |
|----------|----------------|---------|
| **Computed on demand, used multiple times** | Lazy accessor with memoization | `TriageContext.ActionableIssues()` |
| **Expensive computation, stable inputs** | Hash-keyed persistent cache | Graph metrics, embeddings |
| **Hot path, sub-millisecond latency required** | In-memory with TTL | API responses, search results |
| **Large dataset, memory-limited** | LRU eviction | File caches, database query results |
| **One-time initialization, immutable after** | `OnceLock` / `sync.Once` | Configuration, static indices |

#### Caching Anti-Patterns

[Code block omitted: example block (lang='n/a', 5 line(s)).]

### 36.2 Lazy Initialization Patterns

#### Rust: OnceLock for Static Lazy Values

**Source**: CASS mining of xf VectorIndex cache

[Code block omitted: Rust fns: get_vector_index; fields: VectorIndex, std.]

**When to use**:
- Configuration that's expensive to compute
- Indices loaded on first access
- Runtime feature flags

#### Go: sync.Once for Thread-Safe Initialization

[Code block omitted: example block (lang='go', 24 line(s)).]

#### TypeScript: Lazy Accessor Pattern

[Code block omitted: example block (lang='typescript', 23 line(s)).]

### 36.3 TriageContext Pattern: Unified Lazy Caching

**Source**: CASS mining of beads_viewer TriageContext implementation

This pattern provides a context object that lazily computes and caches multiple related values, avoiding redundant computation in complex workflows.

#### Go Implementation

[Code block omitted: appendix/example (lang='go', 113 line(s)).]

#### Key Design Points

| Aspect | Implementation | Rationale |
|--------|----------------|-----------|
| **Lazy computation** | Check `computed` flag before work | Avoid redundant expensive calls |
| **Lookup optimization** | Build `Set` from `Slice` on first access | O(1) membership tests |
| **Thread safety** | Optional mutex via constructor | Single-threaded hot paths stay fast |
| **Internal methods** | `*Internal` methods don't acquire locks | Avoid deadlock from nested calls |
| **Cycle detection** | `visiting` map parameter | Handle graph cycles gracefully |
| **Reset capability** | Clear all cached state | Reuse context across operations |

### 36.4 Heap-Based Top-K Collectors

**Source**: CASS mining of beads_viewer topk utility and cass vector search

For selecting the top K items from a stream or large collection, a min-heap is more efficient than full sorting: O(n log k) vs O(n log n).

#### Go Generic Implementation

[Code block omitted: example block (lang='go', 100 line(s)).]

#### Rust BinaryHeap Implementation

[Code block omitted: Rust structs: TopKCollector, ScoredEntry; fns: partial_cmp, cmp, new; fields: k, heap, score, item.]

#### Complexity Comparison

| Operation | sort.Slice | Heap-based Top-K |
|-----------|------------|------------------|
| Time | O(n log n) | O(n log k) |
| Space | O(n) | O(k) |
| Streaming | No (need all items) | Yes (process items as they arrive) |

**Benchmark results** (from beads_viewer): ~15x faster for k=10 on n=10,000 items

### 36.5 LRU Cache with Disk Persistence

**Source**: CASS mining of beads_viewer cache.go and codex LRU discussions

#### Go Implementation

[Code block omitted: appendix/example (lang='go', 143 line(s)).]

### 36.6 In-Memory Cache with TTL

**Source**: CASS mining of beads_viewer GlobalCache pattern

[Code block omitted: example block (lang='go', 91 line(s)).]

### 36.7 SIMD-Optimized Dot Product

**Source**: CASS mining of xf and cass vector search implementations

[Code block omitted: Rust fns: dot_product_simd, dot_product, dot_product_scalar; fields: std.]

### 36.8 Parallel K-NN Search with Thread-Local Heaps

**Source**: CASS mining of cass vector index parallel search

[Code block omitted: Rust fns: search_top_k, search_top_k_sequential, search_top_k_parallel; fields: BinaryHeap, doc_id.]

### 36.9 Cache-Efficient Data Layout (Struct of Arrays)

**Source**: CASS mining of cass vector index memory layout

[Code block omitted: Rust structs: VectorIndex, VectorRow; enums: VectorStorage; fns: vector_slab_offset_bytes, align_up; fields: rows, vectors, message_id, created_at_ms, agent_id, workspace_id.]

**Benefits of SoA Layout**:
| Aspect | Array of Structs (AoS) | Struct of Arrays (SoA) |
|--------|------------------------|------------------------|
| **Cache utilization** | Poor (loads unused fields) | Excellent (loads only needed data) |
| **SIMD friendliness** | Poor (scattered data) | Excellent (contiguous data) |
| **Memory bandwidth** | Wasteful | Efficient |
| **Prefetching** | Unpredictable | Sequential access patterns |

### 36.10 Hash-Based Content Deduplication

**Source**: CASS mining of xf and cass embedding deduplication

[Code block omitted: Rust fns: content_hash, get_or_compute; fields: content, compute, F.]

### 36.11 Cache Invalidation Strategies

| Strategy | Use Case | Implementation |
|----------|----------|----------------|
| **TTL-based** | Time-sensitive data | Check `time.Since(computedAt) > ttl` |
| **Hash-based** | Content-derived values | Compare `storedHash != currentHash` |
| **Event-driven** | Reactive systems | Publish invalidation on data change |
| **LRU eviction** | Memory-bounded caches | Remove least-recently-used entries |
| **Version-based** | Schema migrations | Store version, invalidate on mismatch |
| **Manual** | User-triggered | Explicit `cache.Invalidate(key)` |

### 36.12 Application to meta_skill

| Component | Caching Strategy | Pattern |
|-----------|------------------|---------|
| **Skill index** | Lazy initialization | `OnceLock` / `sync.Once` for singleton |
| **Rendered templates** | Content hash deduplication | Hash template + variables |
| **Search results** | LRU with TTL | Top-10 recent queries |
| **Parsed YAML** | TriageContext pattern | Lazy accessor on skill struct |
| **Vector embeddings** | Disk cache with hash | Avoid re-computing unchanged skills |
| **API responses** | In-memory TTL | 5-minute cache for list endpoints |

### 36.13 Caching Checklist

Before implementing caching:
- [ ] Identify the computation bottleneck (profile first)
- [ ] Determine cache key granularity (too broad = cache misses, too narrow = explosion)
- [ ] Choose appropriate TTL for data freshness requirements
- [ ] Implement invalidation strategy (stale data is worse than no caching)
- [ ] Set memory bounds (LRU eviction or max entries)
- [ ] Add hash-based staleness detection for derived values
- [ ] Consider thread safety requirements (single-threaded vs concurrent)
- [ ] Use lazy initialization for expensive one-time setup
- [ ] For hot paths, consider SIMD and cache-line alignment
- [ ] For large datasets, consider parallel processing with thread-local heaps
- [ ] Test cache behavior under memory pressure
- [ ] Add metrics for cache hit rate monitoring

---

## Section 37: Debugging Workflows and Methodologies

**Source**: CASS mining of brenner_bot, coding_agent_session_search, coding_agent_account_manager, mcp_agent_mail, fix_my_documents_backend, and agentic_coding_flywheel_setup debugging sessions

### 37.1 Debugging Philosophy

#### The Systematic Approach

Effective debugging follows a methodical process rather than random experimentation:

| Phase | Action | Outcome |
|-------|--------|---------|
| **1. Reproduce** | Create minimal, reliable reproduction | Consistent failure on demand |
| **2. Isolate** | Narrow scope to smallest unit | Single function or data path |
| **3. Hypothesize** | Form testable theory | "If X, then Y should happen" |
| **4. Verify** | Test hypothesis with evidence | Log output, debugger, or test |
| **5. Fix** | Apply minimal change | Targeted correction |
| **6. Validate** | Confirm fix works | Tests pass, behavior correct |
| **7. Prevent** | Add regression test | Future protection |

#### Debugging Anti-Patterns

[Code block omitted: example block (lang='n/a', 6 line(s)).]

### 37.2 Systematic Code Review for Bug Classes

**Source**: CASS mining of coding_agent_account_manager sync package review

#### Race Condition Hunting

[Code block omitted: example block (lang='go', 14 line(s)).]

**Race Condition Detection Checklist:**
- [ ] Map access from multiple goroutines → needs mutex
- [ ] Pointer/slice assignment without sync → data race
- [ ] Check-then-act without lock → TOCTOU vulnerability
- [ ] Shared mutable state in struct → needs sync primitives

#### Go Race Detector Usage

[Code block omitted: shell commands using go.]

**Example race condition fix:**

[Code block omitted: example block (lang='go', 15 line(s)).]

### 37.3 Error Handling Issue Detection

**Source**: CASS mining of coding_agent_account_manager ssh.go review

#### Error Handling Bug Patterns

| Pattern | Issue | Fix |
|---------|-------|-----|
| **Swallowed error** | `if err != nil { /* ignore */ }` | Log or propagate |
| **Missing defer Close** | Resource opened but not closed on error | Add `defer f.Close()` after open |
| **Half-handled error** | Error checked but not all paths covered | Complete error path coverage |
| **Silent fallback** | Error replaced with default without logging | Log original error before fallback |

[Code block omitted: example block (lang='go', 13 line(s)).]

#### Resource Leak Detection

[Code block omitted: example block (lang='go', 20 line(s)).]

### 37.4 Performance Debugging Methodology

**Source**: CASS mining of beads_viewer pkg/ui performance analysis

#### Profiling Hot Paths

**Step 1: Identify the hot path**
[Code block omitted: shell commands using go.]

**Step 2: Measure allocation pressure**

| Allocation Source | Count/Frame | Impact |
|-------------------|-------------|--------|
| `Renderer.NewStyle()` | 16 per item | High - 800 allocs at 50 items |
| `fmt.Sprintf()` | 6 per item | Medium - string allocations |
| `append()` to slice | 8-12 per item | Low with pre-allocation |

**Step 3: Apply targeted fixes**

[Code block omitted: example block (lang='go', 23 line(s)).]

### 37.5 N+1 Query Pattern Detection

**Source**: CASS mining of mcp_agent_mail app.py N+1 analysis

#### Identifying N+1 Patterns

[Code block omitted: example block (lang='python', 14 line(s)).]

[Code block omitted: example block (lang='python', 10 line(s)).]

#### N+1 Detection Checklist

- [ ] Loop containing database query → batch outside loop
- [ ] Repeated function calls with single ID → batch with list
- [ ] ORM lazy loading in loop → eager load with joins
- [ ] HTTP request per item → batch API call

### 37.6 Test Failure Debugging

**Source**: CASS mining of coding_agent_session_search cli.rs test debugging

#### Analyzing Test Failures

[Code block omitted: Rust fns: truncate_for_markdown.]

#### Test Debugging Workflow

[Code block omitted: shell commands using cargo, fn, let, eprintln!("Input:, eprintln!("Result:, eprintln!("Result, assert_eq!(result,, }....]

### 37.7 Comprehensive Investigation Report Format

**Source**: CASS mining of mcp_agent_mail manifest validation investigation

When debugging complex issues, use a structured report format:

[Code block omitted: example block (lang='markdown', 9 line(s)).]
// Current problematic code
[Code block omitted: example block (lang='n/a', 1 line(s)).]
// Corrected code
[Code block omitted: example block (lang='n/a', 11 line(s)).]

### 37.8 Print Debugging Best Practices

**Source**: CASS mining of coding_agent_session_search CLI normalization debugging

#### Strategic Debug Output

[Code block omitted: Rust fns: normalize_args.]

#### Structured Logging for Debugging

[Code block omitted: example block (lang='go', 22 line(s)).]

[Code block omitted: example block (lang='python', 15 line(s)).]

### 37.9 Concurrency Debugging

**Source**: CASS mining of mcp_agent_mail rate limit debugging

#### Detecting Race Conditions in Async Code

[Code block omitted: example block (lang='python', 27 line(s)).]

#### Deadlock Prevention

[Code block omitted: example block (lang='go', 25 line(s)).]

### 37.10 Timeout and Context Deadline Debugging

**Source**: CASS mining of coding_agent_account_manager script test handling

[Code block omitted: example block (lang='go', 26 line(s)).]

### 37.11 Debugging Checklist by Bug Type

#### Crash/Panic Debugging
- [ ] Check for nil pointer dereference
- [ ] Check for out-of-bounds array/slice access
- [ ] Check for division by zero
- [ ] Check for stack overflow (deep recursion)
- [ ] Check for race conditions (use `-race` flag)
- [ ] Enable stack traces (`RUST_BACKTRACE=1` or equivalent)

#### Memory Leak Debugging
- [ ] Check for unclosed file handles
- [ ] Check for unclosed database connections
- [ ] Check for unclosed HTTP response bodies
- [ ] Check for growing maps without cleanup
- [ ] Check for goroutines that never exit
- [ ] Profile with memory profiler

#### Performance Debugging
- [ ] Profile CPU usage to find hot spots
- [ ] Profile memory allocation
- [ ] Check for N+1 query patterns
- [ ] Check for O(n²) algorithms
- [ ] Check for unnecessary allocations in loops
- [ ] Check for synchronous I/O blocking async code

#### Logic Bug Debugging
- [ ] Add assertions for preconditions
- [ ] Log input/output at function boundaries
- [ ] Check boundary conditions (empty, one, many)
- [ ] Check error handling paths
- [ ] Use debugger to step through logic
- [ ] Write failing test first

### 37.12 Application to meta_skill

| Debugging Area | Pattern | meta_skill Application |
|----------------|---------|------------------------|
| **Race conditions** | Run with `-race` flag | Test skill loading concurrency |
| **N+1 queries** | Batch lookups | Load related skills together |
| **Performance** | Profile hot paths | Optimize skill rendering |
| **Test failures** | Structured investigation | Categorize by severity |
| **Error handling** | Check all paths | Ensure cleanup on failure |
| **Logging** | Structured with context | Add skill_id to all logs |

### 37.13 Debugging Workflow Checklist

Before diving into debugging:
- [ ] Can you reproduce the bug reliably?
- [ ] Do you have a minimal reproduction case?
- [ ] Have you checked error logs?
- [ ] Have you checked recent code changes (git bisect)?

During investigation:
- [ ] Have you isolated the failing component?
- [ ] Have you formed a testable hypothesis?
- [ ] Have you verified hypothesis with evidence?
- [ ] Have you checked related code for same issue?

After fixing:
- [ ] Does the fix address root cause (not just symptom)?
- [ ] Have you added a regression test?
- [ ] Have you removed debug code?
- [ ] Have you documented the fix in commit message?

## Section 38: Refactoring Patterns and Methodology

*Source: CASS mining of local coding agent sessions - refactoring workflows, clippy-driven improvements, code modernization*

### 38.1 Introduction

Refactoring is the disciplined technique of restructuring existing code without changing its external behavior. This section synthesizes patterns from real-world refactoring sessions across Rust, Go, Python, and TypeScript projects.

**Key Principle**: Refactoring should be incremental, testable, and driven by concrete signals (linter warnings, performance data, maintainability concerns) rather than aesthetic preferences.

### 38.2 Linter-Driven Refactoring (Clippy Workflow)

One of the most effective refactoring triggers is linter feedback. The clippy workflow demonstrates systematic improvement:

#### 38.2.1 The Clippy Fix Cycle

[Code block omitted: shell commands using cargo.]

#### 38.2.2 Common Clippy Fixes

| Lint | Issue | Fix Pattern |
|------|-------|-------------|
| `format_push_string` | `push_str(&format!(...))` | Use `write!(buf, ...)` instead |
| `manual_let_else` | Match that can be `let else` | Convert to `let Some(x) = expr else { return }` |
| `needless_pass_by_value` | Owned type passed but not consumed | Take `&T` instead of `T` |
| `cast_possible_truncation` | `u128` to `u64` without check | Add explicit truncation handling |
| `single_match` | Match with one arm + wildcard | Convert to `if let` |
| `map_unwrap_or` | `.map(...).unwrap_or(...)` | Use `.map_or(default, \|x\| ...)` |
| `too_many_lines` | Function exceeds line limit | Extract helper functions |

#### 38.2.3 Example: Fresh Eyes Review

From a real session (destructive_command_guard):

[Code block omitted: Rust types/logic.]

### 38.3 Dead Code Removal

#### 38.3.1 Detection Strategies

1. **Compiler warnings**: `#[warn(dead_code)]` in Rust, unused import warnings
2. **IDE analysis**: Gray/faded code indicating unused symbols
3. **Search verification**: `rg "symbol_name"` to confirm no usages
4. **Comment archaeology**: Check if code is referenced only in comments

#### 38.3.2 Safe Removal Process

[Code block omitted: shell commands using rg, cargo.]

#### 38.3.3 Example: Orphaned File Detection

From a real session (brenner_bot):

[Code block omitted: example block (lang='n/a', 4 line(s)).]

**Key Pattern**: Always flag orphaned files explicitly rather than silently removing them.

### 38.4 Unused Variable Handling

#### 38.4.1 The Underscore Convention

[Code block omitted: example block (lang='typescript', 6 line(s)).]

#### 38.4.2 Rust-Specific Patterns

[Code block omitted: Rust fns: unused_callback, handler.]

### 38.5 Function Extraction

#### 38.5.1 When to Extract

| Signal | Action |
|--------|--------|
| Function exceeds ~50 lines | Extract logical subsections |
| Repeated code blocks | Extract shared helper |
| Deep nesting (>3 levels) | Extract inner logic |
| Comments explaining "what" | Code should be self-documenting via function name |
| Multiple responsibilities | One function = one purpose |

#### 38.5.2 Extraction Process

1. **Identify boundaries**: Find natural cut points (after setup, before cleanup, between phases)
2. **Name the concept**: The function name should explain "what", not "how"
3. **Extract with parameters**: Pass only what's needed, return only what's used
4. **Test both caller and extracted function**

#### 38.5.3 Example: Too Many Lines Fix

From clippy warning in dcg:

[Code block omitted: Rust fns: run_command.]

### 38.6 Code Organization Patterns

#### 38.6.1 Module Structure (Rust Example)

From beads_viewer architecture:

[Diagram omitted: workflow/system diagram.]
- 12 line(s); see surrounding text for step labels.

**Key Principles**:
1. **Clear separation of concerns**: loader, analysis, UI, export are independent
2. **Flat-ish structure**: Avoid deep nesting of modules
3. **Test files colocated**: `foo_test.go` next to `foo.go`
4. **Shared types in `model/`**: Prevents circular dependencies

#### 38.6.2 Layered Architecture

[Diagram omitted: workflow/system diagram.]
- 9 line(s); see surrounding text for step labels.

### 38.7 Consistency Improvements

#### 38.7.1 Pattern Normalization

From mcp_agent_mail code review:

[Code block omitted: example block (lang='python', 6 line(s)).]

**Impact**: Cache key consistency improved (4 different path formats → 1 cache entry)

#### 38.7.2 Error Handling Consistency

[Code block omitted: example block (lang='go', 13 line(s)).]

### 38.8 Defensive Refactoring

#### 38.8.1 Redundant But Safe Checks

From code review:

[Code block omitted: example block (lang='python', 7 line(s)).]

**Principle**: Accept minor redundancy when it protects against future regressions.

#### 38.8.2 Array Mutation Prevention

From brenner_bot bug fix:

[Code block omitted: example block (lang='typescript', 5 line(s)).]

### 38.9 Type System Improvements

#### 38.9.1 Strengthening Types

[Code block omitted: Rust enums: Status; fns: set_status.]

#### 38.9.2 Narrowing Generic Constraints

[Code block omitted: Rust fns: process.]

### 38.10 Refactoring Triggers

| Trigger | Response |
|---------|----------|
| Clippy/linter warnings | Fix systematically (see 38.2) |
| Failing tests after change | Understand coupling, extract shared logic |
| Performance bottleneck | Profile first, then optimize hot path |
| New feature difficult to add | Refactor to make feature easy, then add |
| Bug in complex function | Simplify first, then fix |
| Code review feedback | Address systematically |
| Upgrade to new language edition | Run migration tools, then fix manually |

### 38.11 Refactoring Anti-Patterns

#### 38.11.1 Big Bang Refactoring

**Problem**: Attempting massive restructuring in one commit  
**Solution**: Incremental changes, each tested and committed

#### 38.11.2 Refactoring Without Tests

**Problem**: Changing code without test coverage  
**Solution**: Write characterization tests first, then refactor

#### 38.11.3 Premature Abstraction

**Problem**: Creating abstractions for single use cases  
**Solution**: Wait for repetition (Rule of Three)

#### 38.11.4 Aesthetic-Only Changes

**Problem**: Reformatting working code for "cleanliness"  
**Solution**: Only refactor when there's a concrete benefit (performance, maintainability, bug fix)

### 38.12 Application to meta_skill

| Refactoring Area | Pattern | meta_skill Application |
|------------------|---------|------------------------|
| **Linter compliance** | Clippy workflow | Run `cargo clippy` in CI, fix before merge |
| **Dead code** | Detection + removal | Flag unused skills, orphaned templates |
| **Function extraction** | Too-many-lines fix | Keep skill handlers focused |
| **Module structure** | Clear separation | skills/, templates/, search/, cli/ |
| **Type safety** | Enum over strings | SkillType, TemplateKind as enums |
| **Consistency** | Normalize paths | Consistent skill path resolution |

### 38.13 Refactoring Workflow Checklist

Before refactoring:
- [ ] Do you have test coverage for the code being changed?
- [ ] Is there a concrete trigger (lint, perf, bug, feature)?
- [ ] Have you identified the smallest useful change?

During refactoring:
- [ ] Are you making one logical change per commit?
- [ ] Are tests passing after each change?
- [ ] Are you avoiding behavior changes (pure restructuring)?

After refactoring:
- [ ] Does the code pass all linters?
- [ ] Are there any new warnings?
- [ ] Is the code more readable/maintainable?
- [ ] Have you updated documentation if needed?

### 38.14 Tool Reference

| Task | Tool | Command |
|------|------|---------|
| Rust linting | clippy | `cargo clippy --all-targets -- -D warnings` |
| Rust auto-fix | clippy | `cargo clippy --fix --allow-dirty` |
| Go linting | golangci-lint | `golangci-lint run` |
| Python linting | ruff | `ruff check .` |
| TypeScript linting | eslint | `eslint --fix .` |
| Dead code (Rust) | cargo | `cargo build 2>&1 \| grep "warning: unused"` |
| Symbol search | ripgrep | `rg "symbol_name" --type rust` |
| AST refactoring | ast-grep | `ast-grep run -p 'old_pattern' -r 'new_pattern'` |

---

## Section 39: REST API Design Patterns

*CASS Research: Mined from flywheel_gateway, flywheel_private, jeffreysprompts_premium projects*

### 39.1 Overview

REST API design in agentic systems requires careful attention to schema validation, error taxonomies, authentication flows, and pagination. This section captures patterns observed across production implementations where AI agents both consume and expose APIs.

**Key Themes from CASS Mining:**
- Zod-based runtime validation with OpenAPI generation
- Structured error taxonomies with AI-friendly hints
- OAuth 2.0 Device Code flow for headless agents
- Cursor-based pagination for streaming results
- Idempotency middleware for safe retries
- Semantic HTTP status codes

### 39.2 Schema Validation Architecture

#### 39.2.1 Zod as the Single Source of Truth

A production gateway with 25+ route files and 133+ schemas demonstrates the pattern:

[Code block omitted: example block (lang='typescript', 30 line(s)).]

#### 39.2.2 Schema Categories

| Category | Count | Purpose | Example |
|----------|-------|---------|---------|
| **Request Validation** | 46 | Validate POST/PUT bodies | CreateJobSchema, SendMessageSchema |
| **Query/Filter** | 19 | Validate GET query params | ListReposQuerySchema, SearchQuerySchema |
| **Discriminated Union** | 3 | Type-safe polymorphism | StepConfigSchema, BudgetStrategySchema |
| **Enum** | 6 | Constrained string sets | ProviderSchema, ProfileStatusSchema |
| **Configuration** | 8 | Complex nested configs | UpdateConfigSchema, RateCardSchema |

#### 39.2.3 OpenAPI Generation

[Code block omitted: example block (lang='typescript', 28 line(s)).]

**Exposed Endpoints:**
- `GET /openapi.json` - Raw OpenAPI 3.1 specification
- `GET /docs` - Swagger UI interactive documentation
- `GET /redoc` - ReDoc documentation

### 39.3 Error Taxonomy

#### 39.3.1 Structured Error Codes

A production error taxonomy with 55+ codes across 7 categories:

[Code block omitted: example block (lang='typescript', 109 line(s)).]

#### 39.3.2 Error Response Format

[Code block omitted: example block (lang='typescript', 29 line(s)).]

#### 39.3.3 HTTP Status Code Semantics

| Status | When to Use | Example Scenario |
|--------|-------------|------------------|
| **200 OK** | Successful GET, successful update returning data | Fetch reservation details |
| **201 Created** | Resource created, return with Location header | Create new reservation |
| **202 Accepted** | Async operation started, will complete later | Start long-running job |
| **204 No Content** | Successful DELETE, update with no body needed | Delete reservation |
| **400 Bad Request** | Malformed JSON, validation failure | Invalid schema |
| **401 Unauthorized** | Missing or invalid token | Expired JWT |
| **403 Forbidden** | Valid token but insufficient permissions | Wrong scope |
| **404 Not Found** | Resource doesn't exist | Unknown reservation ID |
| **409 Conflict** | State conflict, duplicate key | File already reserved |
| **410 Gone** | Resource permanently deleted | Purged job history |
| **422 Unprocessable** | Valid JSON but business rule violation | Invalid state transition |
| **429 Too Many Requests** | Rate limited, include Retry-After | Burst limit exceeded |
| **500 Internal Error** | Unexpected server error | Unhandled exception |
| **502 Bad Gateway** | Upstream service error | LLM API failure |
| **503 Unavailable** | Service temporarily unavailable | Database maintenance |

### 39.4 Authentication Patterns

#### 39.4.1 OAuth 2.0 Device Code Flow (RFC 8628)

For headless agents without browser access:

[Code block omitted: example block (lang='typescript', 30 line(s)).]

#### 39.4.2 Token Management

[Code block omitted: example block (lang='typescript', 30 line(s)).]

#### 39.4.3 Security Analysis

| Component | Implementation | Security Property |
|-----------|----------------|-------------------|
| Device code | 32-byte random (256-bit) | Unguessable, safe for polling |
| User code | 8-char from 20-char alphabet | 25.8 bits entropy, human-friendly |
| Access token | JWT, 15-min expiry | Short exposure window |
| Refresh token | Opaque, single-use, rotation | Replay detection, family revocation |
| PKCE | Required for public clients | Prevents authorization code interception |

### 39.5 Pagination Patterns

#### 39.5.1 Cursor-Based Pagination

Preferred over offset-based for stability with concurrent modifications:

[Code block omitted: example block (lang='typescript', 41 line(s)).]

#### 39.5.2 Cursor Encoding

[Code block omitted: example block (lang='typescript', 12 line(s)).]

#### 39.5.3 Pagination Comparison

| Aspect | Offset-Based | Cursor-Based |
|--------|--------------|--------------|
| **Stability** | Items shift with inserts/deletes | Stable position |
| **Performance** | O(offset) for large offsets | O(1) seek |
| **Random access** | Supported (page 50) | Not supported |
| **Implementation** | Simple | More complex |
| **Use when** | Static data, UI with page numbers | Dynamic data, infinite scroll |

### 39.6 Idempotency Middleware

#### 39.6.1 Purpose

Ensures safe retries for non-idempotent operations (POST, DELETE with side effects):

[Code block omitted: example block (lang='typescript', 64 line(s)).]

#### 39.6.2 Client Usage

[Code block omitted: example block (lang='typescript', 11 line(s)).]

### 39.7 Route Organization

#### 39.7.1 Hono-Based Route Structure

[Code block omitted: example block (lang='typescript', 33 line(s)).]

#### 39.7.2 Route File Organization

[Diagram omitted: workflow/system diagram.]
- 32 line(s); see surrounding text for step labels.

### 39.8 Request/Response Patterns

#### 39.8.1 Standard Response Helpers

[Code block omitted: example block (lang='typescript', 33 line(s)).]

#### 39.8.2 Validation Error Transformation

[Code block omitted: example block (lang='typescript', 18 line(s)).]

### 39.9 API Versioning Strategies

#### 39.9.1 URL Path Versioning

[Code block omitted: example block (lang='typescript', 5 line(s)).]

#### 39.9.2 Header-Based Versioning

[Code block omitted: example block (lang='typescript', 16 line(s)).]

#### 39.9.3 Versioning Decision Matrix

| Strategy | Pros | Cons | Use When |
|----------|------|------|----------|
| **URL Path** | Clear, cacheable, visible in logs | URL changes between versions | Breaking changes, public APIs |
| **Header** | URL stable, granular | Hidden, harder to cache | Internal APIs, minor changes |
| **Query Param** | Easy to switch | Pollutes URLs, caching issues | Rarely recommended |

### 39.10 Rate Limiting

#### 39.10.1 Multi-Tier Rate Limiting

[Code block omitted: example block (lang='typescript', 31 line(s)).]

### 39.11 Application to meta_skill

| Pattern | meta_skill Application |
|---------|------------------------|
| **Zod Schemas** | Validate skill invocation parameters, template variables |
| **OpenAPI Generation** | Auto-generate API docs for HTTP-based skill registry |
| **Error Taxonomy** | Structured errors for skill execution failures |
| **Cursor Pagination** | List skills, search results, template listings |
| **Idempotency** | Safe skill installation/uninstallation |
| **Auth** | Skill marketplace authentication, API keys |
| **Versioning** | Skill version constraints, API evolution |

### 39.12 REST API Checklist

Before deploying an API endpoint:

**Schema & Validation:**
- [ ] Request body validated with Zod schema
- [ ] Query parameters validated with coercion
- [ ] Schema registered in OpenAPI registry
- [ ] Response schema documented

**Error Handling:**
- [ ] Uses error codes from taxonomy
- [ ] Includes aiHint for AI consumers
- [ ] Validation errors include field details
- [ ] Request ID included in error response

**HTTP Semantics:**
- [ ] Correct status code (201 for create, 204 for delete)
- [ ] Location header for created resources
- [ ] Proper Content-Type headers

**Pagination & Performance:**
- [ ] List endpoints use cursor-based pagination
- [ ] Reasonable default and max limits
- [ ] hasMore indicator in response

**Security:**
- [ ] Auth middleware applied
- [ ] Rate limiting configured
- [ ] Idempotency key support for mutations

### 39.13 Anti-Patterns

#### 39.13.1 Avoid: Offset Pagination for Mutable Data

[Code block omitted: example block (lang='typescript', 4 line(s)).]

#### 39.13.2 Avoid: Generic Error Responses

[Code block omitted: example block (lang='typescript', 11 line(s)).]

#### 39.13.3 Avoid: Inconsistent Naming

[Code block omitted: example block (lang='typescript', 8 line(s)).]

#### 39.13.4 Avoid: Overloaded Endpoints

[Code block omitted: example block (lang='typescript', 6 line(s)).]
