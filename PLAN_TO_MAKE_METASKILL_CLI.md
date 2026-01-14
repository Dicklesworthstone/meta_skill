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

```
my-skill/
├── SKILL.md           # Main skill content (required)
├── scripts/           # Executable helpers (optional)
│   ├── validate.sh
│   └── deploy.py
├── references/        # Reference documents (optional)
│   └── api-spec.yaml
├── tests/             # Skill tests (optional)
│   └── basic.yaml
└── assets/            # Images, templates (optional)
    └── architecture.png
```

**Session Segmentation (Phase-Aware Mining):**
- Segment sessions into phases: recon → hypothesis → change → validation → regression fix → wrap-up.
- Use tool-call boundaries + language markers to avoid phase bleed.

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionPhase {
    Recon,
    Hypothesis,
    Change,
    Validation,
    RegressionFix,
    WrapUp,
}
```

**Pattern IR (Typed Intermediate Representation):**
- Compile extracted patterns into typed IR before synthesis (e.g., `CommandRecipe`,
  `DiagnosticDecisionTree`, `Invariant`, `Pitfall`, `PromptMacro`, `RefactorPlaybook`,
  `ChecklistItem`).
- Normalize commands, filepaths, tool names, and error signatures for deterministic dedupe.

**SKILL.md anatomy:**

```markdown
---
name: my-skill-name
description: One-line description shown in skill listings
version: 1.0.0
tags: [rust, cli, deployment]
requires: [core-cli-basics, logging-standards]
provides: [rust-cli-patterns]
aliases: [legacy-cli-patterns]
requirements:
  platforms: [macos, linux]
  tools:
    - name: git
      min_version: "2.40.0"
    - name: gh
      required: false
  env:
    - GITHUB_TOKEN
  network: required
fixes:  # Error codes/patterns this skill helps resolve
  - "clippy::unwrap_used"
  - "ubs:nil_dereference"
policies:  # Machine-readable constraints for external tooling (declarative, not enforced by ms)
  - pattern_type: command
    pattern: "rm -rf"
    severity: block
    message: "Use trash-cli instead per safety-policy skill"
  - pattern_type: ast-grep
    pattern: "console.log($$$)"
    severity: warn
    message: "Use structured logger (see observability skill)"
---

# Skill Title

Brief overview of what this skill enables.

## ⚠️ CRITICAL RULES

1. NEVER do X without Y
2. ALWAYS check Z before W

## Core Content

The main instructional content...

::: block id="middleware-v15" condition="package:next < 16.0.0"
### Middleware (Next.js 15 and earlier)
Create `src/middleware.ts` for request interception...
:::

::: block id="proxy-v16" condition="package:next >= 16.0.0"
### Proxy Handler (Next.js 16+)
Create `src/proxy.ts` using the new proxy API...
:::

## Examples

Concrete examples with code...

## Troubleshooting

Common errors and resolutions...
```

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

```bash
# Search across all indexed sessions
cass search "authentication error handling" --robot --limit 10

# View a specific session
cass show /path/to/session.jsonl --robot

# Expand context around a match
cass expand /path/to/session.jsonl -n 42 -C 5 --robot

# Check CASS health and indexed content
cass health
cass stats

# Self-documenting API
cass capabilities --json
cass robot-docs guide
```

**Robot mode:** All CASS commands support `--robot` for machine-readable JSON output. This is critical for programmatic integration—ms will call CASS as a subprocess and parse its JSON output.

**CASS search technology:**
- **Lexical search:** Tantivy (Rust port of Lucene) for BM25 full-text search
- **Semantic search:** Hash-based embeddings (no ML model required)
- **Hybrid fusion:** Reciprocal Rank Fusion (RRF) combines both rankings

**Session structure:** A session is a sequence of messages:
```json
{"role": "user", "content": "Fix the auth bug"}
{"role": "assistant", "content": "I'll investigate...", "tool_calls": [...]}
{"role": "tool", "tool_call_id": "xyz", "content": "file contents..."}
{"role": "assistant", "content": "Found the issue..."}
```

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

```
┌─────────────────────────────────────────────────────────────────┐
│                    DUAL PERSISTENCE                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│  SQLite Database                    Git Archive                 │
│  ├── Fast queries                   ├── Human-readable          │
│  ├── FTS5 search                    ├── git log/blame/diff      │
│  ├── ACID transactions              ├── Branch/merge            │
│  └── Efficient storage              └── Natural sync            │
│                                                                 │
│  Write to both → Query from SQLite → Audit via Git             │
│                                                                 │
│  Recovery: If SQLite corrupted, rebuild from Git               │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

**Why ms adopts this:** Skills benefit from both:
- **SQLite:** Fast search, usage tracking, quality scores
- **Git:** Version history, collaborative editing, sync across machines

**Two-Phase Commit (2PC):** To prevent drift between SQLite and Git, ms uses a
lightweight two-phase commit for all write operations.

**File reservation pattern:** When an agent wants to edit a file, it requests a reservation:
```bash
# Agent claims exclusive access to a file path/glob
agent_mail reserve --path "src/auth/*.rs" --ttl 3600 --exclusive
```

ms can use similar reservations for skill editing to prevent conflicts.

### 0.6 What Is NTM (Named Tmux Manager)?

**NTM** is a Go CLI that transforms tmux into a multi-agent command center. It spawns and orchestrates multiple AI coding agents in parallel.

**Why NTM matters for ms:**
1. **Multi-agent skill loading:** When NTM spawns agents, each needs appropriate skills
2. **Skill coordination:** Multiple agents shouldn't redundantly load same skills
3. **Context rotation:** As agents exhaust context, skills must transfer to fresh agents

**NTM agent types:**
```bash
ntm spawn myproject --cc=3 --cod=2 --gmi=1  # 3 Claude + 2 Codex + 1 Gemini
ntm send myproject --cc "Implement auth"     # Send to all Claude agents
```

**Integration point:** ms should provide:
```bash
ms suggest --for-ntm myproject  # Skills for multi-agent session
ms suggest --for-ntm myproject --agents 6 --budget 800 --objective coverage_first
ms load ntm --level 2           # Appropriate for split attention
```

### 0.7 What Is BV (Beads Viewer) and the Beads System?

**Beads** is a lightweight issue/task tracking system designed for AI agent workflows. Unlike Jira/Linear, beads are:
- **File-based:** Stored in `.beads/` directory
- **Git-native:** Tracked in version control
- **Agent-friendly:** Simple enough for agents to read/write

**Bead structure:**
```yaml
# .beads/issues/beads-abc123.yaml
id: beads-abc123
title: Implement OAuth2 flow
type: feature
status: in_progress
priority: 2
created: 2026-01-10T14:30:00Z
assignee: GreenCastle  # Agent name
blocks: []
blocked_by: [beads-xyz789]
```

**BV (Beads Viewer)** is the CLI for interacting with beads:
```bash
bd ready           # Show unblocked issues ready to work
bd create --title "Fix auth bug" --type bug
bd update beads-abc123 --status in_progress
bd close beads-abc123
bd stats           # Project overview
```

**Beads Viewer Integration (bv):**
- Prefer `bv --robot-*` flags for deterministic JSON (triage, plan, graph, insights).
- Use `bv --robot-triage` as the single entry point for actionable, dependency-aware picks.
- Avoid bare `bv` in automation (it launches interactive TUI).

**Why this matters for ms:** Skills can be tracked as beads. A skill-building session could be:
```bash
bd create --title "Create rust-async skill from CASS" --type task
ms build --name rust-async --from-cass "async rust patterns"
bd close beads-xyz --reason "Skill published"
```

### 0.8 The Agent Flywheel Ecosystem

The **Agent Flywheel** is an integrated suite of tools that compound AI agent effectiveness:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         THE AGENT FLYWHEEL                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│                              ┌─────────┐                                    │
│                              │   NTM   │ ◄── Spawns/manages agents          │
│                              │ (spawn) │                                    │
│                              └────┬────┘                                    │
│                                   │                                         │
│                     ┌─────────────┼─────────────┐                          │
│                     ▼             ▼             ▼                          │
│               ┌─────────┐   ┌─────────┐   ┌─────────┐                      │
│               │ Claude  │   │  Codex  │   │ Gemini  │ ◄── AI agents        │
│               │  Code   │   │   CLI   │   │   CLI   │                      │
│               └────┬────┘   └────┬────┘   └────┬────┘                      │
│                    │             │             │                            │
│                    └─────────────┼─────────────┘                            │
│                                  │                                          │
│                    ┌─────────────┼─────────────┐                           │
│                    ▼             ▼             ▼                           │
│              ┌─────────┐   ┌─────────┐   ┌─────────┐                       │
│              │   BV    │   │  CASS   │   │   MS    │ ◄── THIS PROJECT      │
│              │ (tasks) │   │ (search)│   │ (skill) │                       │
│              └─────────┘   └────┬────┘   └────┬────┘                       │
│                                 │             │                             │
│                                 └──────┬──────┘                             │
│                                        │                                    │
│                               ┌────────┴────────┐                           │
│                               │   Agent Mail    │ ◄── Coordination          │
│                               │  (coordinate)   │                           │
│                               └─────────────────┘                           │
│                                                                             │
│  The Flywheel Effect:                                                       │
│  1. Agents work → Sessions recorded                                         │
│  2. CASS indexes sessions → Searchable history                             │
│  3. MS mines CASS → Skills generated                                       │
│  4. Skills loaded → Agents more effective                                  │
│  5. More effective work → Better sessions → REPEAT                         │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

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

```bash
# Human mode (default)
ms search "auth"
# Output:
# 1. oauth2-patterns (★★★★☆) - OAuth 2.0 implementation patterns
# 2. jwt-handling (★★★☆☆) - JWT creation and validation
# ...

# Robot mode
ms search "auth" --robot
# Output:
{
  "results": [
    {
      "id": "oauth2-patterns",
      "score": 0.89,
      "name": "oauth2-patterns",
      "description": "OAuth 2.0 implementation patterns",
      "token_count": 2340,
      "quality_score": 0.85,
      "tags": ["auth", "oauth", "security"]
    },
    ...
  ],
  "query": "auth",
  "total_matches": 12,
  "search_time_ms": 23
}
```

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

```
● Summary: Found and Fixed 15+ Issues

Scroll Indicator Issues (hero.tsx)
1. Initial scroll position not checked → Added handleScroll() call on mount
2. Outer animation didn't respect reduced motion → Added prefersReducedMotion check
3. Missing aria-hidden → Added aria-hidden="true" (decorative element)
4. Initial opacity mismatch → Changed initial={{ opacity: hasScrolled ? 0 : 1 }}

Accessibility Issues (aria-hidden on decorative elements)
5. SVG star icon in highlight badge → Added aria-hidden="true"
6. ArrowRight icon in "Explore all tools" link → Added aria-hidden="true"
... [26 more fixes listed] ...

CSS Issues (globals.css)
27. btn-glow-primary had conflicting transition: all → Removed
28. Button glow effects didn't respect reduced motion → Added @media block

❯ check for other similar issues!!! Use ultrathink.
● Explore (Deep codebase audit for 43+ issues)
⎿ Done (32 tool uses · 100.4k tokens · 1m 23s)
```

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

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        THE META_SKILL TRANSFORMATION                        │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│   Raw Sessions (CASS)  ──►  Pattern Mining  ──►  Skill Draft  ──►  Polish  │
│                                                                             │
│   "I solved this       ──►  "This pattern    ──►  SKILL.md     ──►  Tested │
│    problem 12 times"        appears in 80%"       + scripts/       & shared│
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

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

```
┌──────────────────────────────────────────────────────────────────────────┐
│                              META_SKILL CLI                              │
├──────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐     │
│  │   Indexer   │  │   Loader    │  │  Suggester  │  │   Builder   │     │
│  │  (Tantivy)  │  │ (Disclosure)│  │  (Context)  │  │   (CASS)    │     │
│  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘     │
│         │                │                │                │             │
│         └────────────────┴────────────────┴────────────────┘             │
│                                    │                                     │
│                           ┌────────┴────────┐                            │
│                           │   Core Engine   │                            │
│                           │ (SQLite + Git)  │                            │
│                           └────────┬────────┘                            │
│                                    │                                     │
│         ┌──────────────────────────┼──────────────────────────┐         │
│         │                          │                          │         │
│  ┌──────┴──────┐  ┌────────────────┴────────────────┐  ┌──────┴──────┐ │
│  │   Bundler   │  │         CASS Integration        │  │  Updater    │ │
│  │  (Package)  │  │  (Session Mining + Generation)  │  │  (GitHub)   │ │
│  └─────────────┘  └─────────────────────────────────┘  └─────────────┘ │
│                                                                          │
└──────────────────────────────────────────────────────────────────────────┘
```

### 2.2 Data Flow

```
                    ┌─────────────────────────────────────────┐
                    │           Skill Sources                 │
                    │  ~/.config/ms/skills/                   │
                    │  /data/projects/*/skills/               │
                    │  .claude/skills/                        │
                    │  GitHub repositories                    │
                    └─────────────────┬───────────────────────┘
                                      │
                                      ▼
                    ┌─────────────────────────────────────────┐
                    │          Skill Registry                 │
                    │  ~/.local/share/ms/registry.db          │
                    │  (SQLite with FTS5)                     │
                    └─────────────────┬───────────────────────┘
                                      │
                    ┌─────────────────┴───────────────────────┐
                    │                                         │
                    ▼                                         ▼
        ┌───────────────────────┐             ┌───────────────────────┐
        │    Full-Text Index    │             │    Vector Index       │
        │   (Tantivy BM25)      │             │   (Hash Embeddings)   │
        └───────────────────────┘             └───────────────────────┘
                    │                                         │
                    └─────────────────┬───────────────────────┘
                                      │
                                      ▼
                    ┌─────────────────────────────────────────┐
                    │          Hybrid Search (RRF)            │
                    │  Rank = 1/(K + rank_bm25) +             │
                    │         1/(K + rank_vector)             │
                    └─────────────────────────────────────────┘
```

### 2.3 File Layout (Following xf Pattern)

```
meta_skill/
├── Cargo.toml
├── Cargo.lock
├── src/
│   ├── main.rs                    # Entry point, CLI setup
│   ├── lib.rs                     # Library root
│   ├── cli/
│   │   ├── mod.rs
│   │   ├── commands/
│   │   │   ├── mod.rs
│   │   │   ├── index.rs           # ms index
│   │   │   ├── search.rs          # ms search
│   │   │   ├── load.rs            # ms load
│   │   │   ├── suggest.rs         # ms suggest
│   │   │   ├── edit.rs            # ms edit
│   │   │   ├── fmt.rs             # ms fmt
│   │   │   ├── diff.rs            # ms diff
│   │   │   ├── alias.rs           # ms alias
│   │   │   ├── requirements.rs    # ms requirements
│   │   │   ├── build.rs           # ms build (CASS integration)
│   │   │   ├── bundle.rs          # ms bundle
│   │   │   ├── update.rs          # ms update
│   │   │   ├── doctor.rs          # ms doctor
│   │   │   ├── prune.rs           # ms prune
│   │   │   ├── init.rs            # ms init
│   │   │   └── config.rs          # ms config
│   │   └── output.rs              # Robot mode, human mode formatting
│   ├── core/
│   │   ├── mod.rs
│   │   ├── skill.rs               # Skill struct and parsing
│   │   ├── registry.rs            # Skill registry management
│   │   ├── disclosure.rs          # Progressive disclosure logic
│   │   ├── safety.rs              # Destructive ops policy + approvals
│   │   ├── requirements.rs        # Environment requirement checks
│   │   ├── spec_lens.rs           # Round-trip spec ↔ markdown mapping
│   │   └── validation.rs          # Skill validation
│   ├── storage/
│   │   ├── mod.rs
│   │   ├── sqlite.rs              # SQLite operations
│   │   ├── git.rs                 # Git persistence layer
│   │   └── migrations.rs          # Schema migrations
│   ├── search/
│   │   ├── mod.rs
│   │   ├── tantivy.rs             # Full-text indexing
│   │   ├── embeddings.rs          # Embedder trait + hash embedder
│   │   ├── embeddings_local.rs    # Optional local ML embedder
│   │   ├── hybrid.rs              # RRF fusion
│   │   └── context.rs             # Context-aware ranking
│   ├── cass/
│   │   ├── mod.rs
│   │   ├── client.rs              # CASS CLI integration
│   │   ├── mining.rs              # Pattern extraction
│   │   ├── synthesis.rs           # Skill generation
│   │   └── refinement.rs          # Iterative improvement
│   ├── bundler/
│   │   ├── mod.rs
│   │   ├── package.rs             # Bundle creation
│   │   ├── github.rs              # GitHub publishing
│   │   └── install.rs             # Bundle installation
│   ├── updater/
│   │   ├── mod.rs
│   │   ├── check.rs               # Version checking
│   │   ├── download.rs            # Binary download
│   │   └── verify.rs              # SHA256 verification
│   └── utils/
│       ├── mod.rs
│       ├── fs.rs                  # Filesystem utilities
│       ├── git.rs                 # Git utilities
│       └── format.rs              # Output formatting
├── migrations/
│   ├── 001_initial_schema.sql
│   ├── 002_add_fts.sql
│   └── 003_add_vectors.sql
├── tests/
│   ├── integration/
│   └── fixtures/
├── .github/
│   └── workflows/
│       ├── ci.yml
│       └── release.yml
└── README.md
```

**Runtime Artifacts:**
- `.ms/skillpack.bin` (or per-skill pack objects) caches parsed spec, slices,
  embeddings, and predicate analysis for low-latency load/suggest.
- Markdown remains a compiled view; runtime uses the pack by default.

---

## 3. Core Data Models

### 3.1 Skill Structure

```rust
/// A complete skill with all metadata and content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Skill {
    /// Unique identifier (derived from path or explicit id)
    pub id: String,

    /// YAML frontmatter metadata
    pub metadata: SkillMetadata,

    /// Main SKILL.md body content
    pub body: String,

    /// Associated files
    pub assets: SkillAssets,

    /// Source information
    pub source: SkillSource,

    /// Computed fields
    pub computed: SkillComputed,

    /// Rule-level evidence and provenance
    pub evidence: SkillEvidenceIndex,
}

/// Deterministic source-of-truth for skill content
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSpec {
    /// Spec format version (for migrations)
    pub format_version: String,

    /// Stable skill id
    pub id: String,

    /// Frontmatter metadata
    pub metadata: SkillMetadata,

    /// Structured sections and blocks
    pub sections: Vec<SkillSectionSpec>,

    /// Associated files
    pub assets: SkillAssets,

    /// Evidence index (rule provenance)
    pub evidence: SkillEvidenceIndex,

    /// When spec was generated or updated
    pub generated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSectionSpec {
    pub title: String,
    pub level: u8,
    pub blocks: Vec<SkillBlockSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SkillBlockSpec {
    Rule { id: String, text: String },
    Command { command: String, description: Option<String> },
    Example { language: String, code: String, description: Option<String> },
    Checklist { items: Vec<String> },
    Table { headers: Vec<String>, rows: Vec<Vec<String>> },
    Prompt { prompt: String },
    Pitfall { bad: String, risk: String, fix: String },
    Note { text: String },
}

/// Mapping from compiled markdown back to spec blocks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpecLens {
    pub format_version: String,
    pub blocks: Vec<BlockLens>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockLens {
    pub block_id: String,
    pub section: String,
    pub block_type: String,
    pub byte_range: (u32, u32),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,

    #[serde(default)]
    pub version: Option<String>,

    #[serde(default)]
    pub author: Option<String>,

    #[serde(default)]
    pub tags: Vec<String>,

    #[serde(default)]
    pub aliases: Vec<String>,  // Alternate names / legacy ids

    #[serde(default)]
    pub requires: Vec<String>,  // Dependencies on other skills

    #[serde(default)]
    pub provides: Vec<String>,  // Capabilities exposed by this skill

    #[serde(default)]
    pub triggers: Vec<SkillTrigger>,  // When to suggest this skill

    #[serde(default)]
    pub priority: SkillPriority,

    #[serde(default)]
    pub deprecated: Option<DeprecationInfo>,

    #[serde(default)]
    pub toolchains: Vec<ToolchainConstraint>,  // Compatibility constraints

    #[serde(default)]
    pub requirements: SkillRequirements,  // Tooling/OS/environment requirements

    #[serde(default)]
    pub fixes: Vec<String>,  // Error codes/patterns this skill addresses (e.g., "clippy::unwrap_used", "ubs:nil_dereference")

    #[serde(default)]
    pub policies: Vec<SkillPolicy>,  // Machine-readable policy constraints for external tooling
}

/// Machine-readable policy declaration for external tool integration
/// Note: These are declarative hints, not runtime enforcement (enforcement belongs in NTM/UBS)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPolicy {
    /// Pattern type: "regex", "ast-grep", "command", "file_pattern"
    pub pattern_type: String,

    /// The pattern to match
    pub pattern: String,

    /// Severity: "block", "warn", "info"
    pub severity: String,

    /// Human-readable explanation
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeprecationInfo {
    /// When the skill was deprecated (YYYY-MM-DD)
    pub since: Option<String>,

    /// Reason for deprecation
    pub reason: String,

    /// Replacement skill id (if any)
    pub replaced_by: Option<String>,

    /// Optional sunset date after which skill should not be suggested
    pub sunset_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillTrigger {
    /// Trigger type: "command", "file_pattern", "keyword", "context"
    pub trigger_type: String,

    /// Pattern to match
    pub pattern: String,

    /// Priority boost when triggered (0.0 - 1.0)
    #[serde(default = "default_boost")]
    pub boost: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolchainConstraint {
    /// Tool or framework name (e.g., "node", "rust", "nextjs")
    pub name: String,

    /// Minimum compatible version (semver)
    pub min_version: Option<String>,

    /// Maximum compatible version (semver)
    pub max_version: Option<String>,

    /// Human-readable notes about compatibility
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SkillRequirements {
    /// Supported platforms (empty = any)
    pub platforms: Vec<Platform>,

    /// Required external tools (git, docker, gh, etc.)
    pub tools: Vec<ToolRequirement>,

    /// Required environment variables (presence only)
    pub env: Vec<String>,

    /// Network requirement (offline/online)
    pub network: NetworkRequirement,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolRequirement {
    pub name: String,
    pub min_version: Option<String>,
    pub max_version: Option<String>,
    #[serde(default = "default_required")]
    pub required: bool,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Platform {
    Any,
    Linux,
    Macos,
    Windows,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkRequirement {
    OfflineOk,
    Required,
    PreferOffline,
}

impl Default for NetworkRequirement {
    fn default() -> Self {
        NetworkRequirement::OfflineOk
    }
}

fn default_required() -> bool { true }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillAssets {
    /// Scripts in scripts/ directory
    pub scripts: Vec<ScriptFile>,

    /// Reference documents in references/ directory
    pub references: Vec<ReferenceFile>,

    /// Skill tests in tests/ directory
    pub tests: Vec<TestFile>,

    /// Other assets (images, templates, etc.)
    pub assets: Vec<AssetFile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestFile {
    pub path: PathBuf,
    pub test_type: String, // yaml | json | md
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSource {
    /// Where the skill was found
    pub path: PathBuf,

    /// Layer for conflict resolution (base, org, project, user)
    pub layer: SkillLayer,

    /// Git remote if available
    pub git_remote: Option<String>,

    /// Last commit hash if in git repo
    pub git_commit: Option<String>,

    /// Last modified timestamp
    pub modified_at: DateTime<Utc>,

    /// Content hash for change detection
    pub content_hash: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SkillLayer {
    Base,
    Org,
    Project,
    User,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillComputed {
    /// Token count estimate
    pub token_count: usize,

    /// Disclosure level summary
    pub disclosure_levels: Vec<DisclosureLevel>,

    /// Quality score (0.0 - 1.0)
    pub quality_score: f32,

    /// Embedding vector for similarity search
    pub embedding: Vec<f32>,

    /// Pre-sliced content blocks for token packing
    pub slices: SkillSliceIndex,
}

/// Precompiled runtime cache for low-latency load/suggest
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPack {
    pub skill_id: String,
    pub pack_path: PathBuf,
    pub spec_hash: String,
    pub slices_hash: String,
    pub embedding_hash: String,
    pub predicate_index_hash: String,
    pub generated_at: DateTime<Utc>,
}

/// A sliceable unit of a skill for token-aware packing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSlice {
    /// Stable slice id (rule-1, example-2, etc.)
    pub id: String,

    /// Slice type for packing heuristics
    pub slice_type: SliceType,

    /// Estimated tokens for this slice
    pub token_estimate: usize,

    /// Utility score (0.0 - 1.0), computed from usage + quality
    pub utility_score: f32,

    /// Coverage group id (e.g., "critical-rules", "workflow", "pitfalls")
    pub coverage_group: Option<String>,

    /// Optional tags for packing and filtering
    pub tags: Vec<String>,

    /// Optional dependencies on other slices (by id)
    pub requires: Vec<String>,

    /// Optional predicate condition for conditional inclusion
    /// Examples: "package:next >= 16.0.0", "rust:edition == 2021", "env:CI"
    /// When present, slice is stripped at load time if condition evaluates false
    pub condition: Option<SlicePredicate>,

    /// Content payload (markdown)
    pub content: String,
}

/// Predicate for conditional slice inclusion based on project environment
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlicePredicate {
    /// Predicate expression: "package:<name> <op> <version>" | "env:<var>" | "file:<glob>"
    pub expr: String,

    /// Pre-parsed predicate type for fast evaluation
    pub predicate_type: PredicateType,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PredicateType {
    /// Package version check: package:next >= 16.0.0
    PackageVersion { package: String, op: VersionOp, version: String },
    /// Environment variable presence: env:CI
    EnvVar { var: String },
    /// File/glob existence: file:src/middleware.ts
    FileExists { pattern: String },
    /// Rust edition check: rust:edition == 2021
    RustEdition { op: VersionOp, edition: String },
    /// Toolchain version: tool:node >= 18.0.0
    ToolVersion { tool: String, op: VersionOp, version: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum VersionOp {
    Eq,   // ==
    Ne,   // !=
    Lt,   // <
    Le,   // <=
    Gt,   // >
    Ge,   // >=
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SliceType {
    Rule,
    Command,
    Example,
    Checklist,
    Pitfall,
    Overview,
    Reference,
    Policy,   // Non-removable safety/policy invariants
}

/// Index of slices for packing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSliceIndex {
    pub slices: Vec<SkillSlice>,
    pub generated_at: DateTime<Utc>,
}

/// Pack contracts define minimal guidance guarantees for specific tasks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackContract {
    pub id: String,                   // e.g., "DebugContract"
    pub description: String,
    pub required_groups: Vec<String>, // e.g., ["critical-rules", "validation"]
    pub mandatory_slices: Vec<String>,
    pub max_per_group: Option<usize>,
}

/// Rule-level evidence index for provenance and auditing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillEvidenceIndex {
    /// Rule id -> evidence references
    pub rules: HashMap<String, Vec<EvidenceRef>>,

    /// Aggregate coverage metrics
    pub coverage: EvidenceCoverage,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRef {
    /// CASS session id
    pub session_id: String,

    /// Inclusive message range within the session (start, end)
    pub message_range: (u32, u32),

    /// Stable hash of the source snippet (after redaction)
    pub snippet_hash: String,

    /// Optional short excerpt for humans (redacted)
    pub excerpt: Option<String>,

    /// Optional pointer to archived excerpt file
    pub excerpt_path: Option<PathBuf>,

    /// Storage level for provenance compression
    pub level: EvidenceLevel,

    /// Confidence for this evidence link (0.0 - 1.0)
    pub confidence: f32,

    /// When this evidence was captured
    pub captured_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EvidenceLevel {
    Pointer,   // hash + message range only
    Excerpt,   // minimal redacted excerpt
    Expanded,  // full context available via CASS
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceCoverage {
    /// Total rules in the skill
    pub total_rules: usize,

    /// Rules with at least one evidence link
    pub rules_with_evidence: usize,

    /// Average evidence confidence across linked rules
    pub avg_confidence: f32,
}

/// Queue item for low-confidence generalizations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UncertaintyItem {
    pub id: String,
    pub pattern_candidate: ExtractedPattern,
    pub reason: String,
    pub confidence: f32,
    pub suggested_queries: Vec<String>,
    pub auto_mine_attempts: u32,
    pub last_mined_at: Option<DateTime<Utc>>,
    pub status: UncertaintyStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UncertaintyStatus {
    Pending,
    Resolved,
    Discarded,
}
```

### 3.2 SQLite Schema

```sql
-- Core skill registry
CREATE TABLE skills (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    version TEXT,
    author TEXT,

    -- Source tracking
    source_path TEXT NOT NULL,
    source_layer TEXT NOT NULL,  -- base | org | project | user
    git_remote TEXT,
    git_commit TEXT,
    content_hash TEXT NOT NULL,

    -- Content
    body TEXT NOT NULL,
    metadata_json TEXT NOT NULL,
    assets_json TEXT NOT NULL,

    -- Computed
    token_count INTEGER NOT NULL,
    quality_score REAL NOT NULL,

    -- Timestamps
    indexed_at TEXT NOT NULL,
    modified_at TEXT NOT NULL,

    -- Status
    is_deprecated INTEGER NOT NULL DEFAULT 0,
    deprecation_reason TEXT
);

-- Alternate names / legacy ids
CREATE TABLE skill_aliases (
    alias TEXT PRIMARY KEY,
    skill_id TEXT NOT NULL,
    alias_type TEXT NOT NULL, -- alias | deprecated
    created_at TEXT NOT NULL,
    FOREIGN KEY(skill_id) REFERENCES skills(id) ON DELETE CASCADE
);

CREATE INDEX idx_skill_aliases_skill ON skill_aliases(skill_id);

-- Full-text search
CREATE VIRTUAL TABLE skills_fts USING fts5(
    name,
    description,
    body,
    tags,
    content='skills',
    content_rowid='rowid'
);

-- Triggers to keep FTS in sync (INSERT, UPDATE, DELETE)
CREATE TRIGGER skills_ai AFTER INSERT ON skills BEGIN
    INSERT INTO skills_fts(rowid, name, description, body, tags)
    VALUES (NEW.rowid, NEW.name, NEW.description, NEW.body,
            (SELECT json_extract(NEW.metadata_json, '$.tags')));
END;

CREATE TRIGGER skills_ad AFTER DELETE ON skills BEGIN
    INSERT INTO skills_fts(skills_fts, rowid, name, description, body, tags)
    VALUES ('delete', OLD.rowid, OLD.name, OLD.description, OLD.body,
            (SELECT json_extract(OLD.metadata_json, '$.tags')));
END;

CREATE TRIGGER skills_au AFTER UPDATE ON skills BEGIN
    INSERT INTO skills_fts(skills_fts, rowid, name, description, body, tags)
    VALUES ('delete', OLD.rowid, OLD.name, OLD.description, OLD.body,
            (SELECT json_extract(OLD.metadata_json, '$.tags')));
    INSERT INTO skills_fts(rowid, name, description, body, tags)
    VALUES (NEW.rowid, NEW.name, NEW.description, NEW.body,
            (SELECT json_extract(NEW.metadata_json, '$.tags')));
END;

-- Vector embeddings storage
CREATE TABLE skill_embeddings (
    skill_id TEXT PRIMARY KEY REFERENCES skills(id),
    embedding BLOB NOT NULL,  -- f16 quantized, 384 dimensions
    created_at TEXT NOT NULL
);

-- Precompiled runtime skillpack cache
CREATE TABLE skill_packs (
    skill_id TEXT PRIMARY KEY REFERENCES skills(id),
    pack_path TEXT NOT NULL,
    spec_hash TEXT NOT NULL,
    slices_hash TEXT NOT NULL,
    embedding_hash TEXT NOT NULL,
    predicate_index_hash TEXT NOT NULL,
    generated_at TEXT NOT NULL
);

-- Pre-sliced content blocks for token packing
CREATE TABLE skill_slices (
    skill_id TEXT NOT NULL REFERENCES skills(id),
    slices_json TEXT NOT NULL,  -- SkillSliceIndex
    updated_at TEXT NOT NULL,
    PRIMARY KEY (skill_id)
);

-- Rule-level evidence and provenance
CREATE TABLE skill_evidence (
    skill_id TEXT NOT NULL REFERENCES skills(id),
    rule_id TEXT NOT NULL,
    evidence_json TEXT NOT NULL,   -- JSON array of EvidenceRef
    coverage_json TEXT NOT NULL,   -- EvidenceCoverage snapshot
    updated_at TEXT NOT NULL,
    PRIMARY KEY (skill_id, rule_id)
);

CREATE INDEX idx_evidence_skill ON skill_evidence(skill_id);

-- Rule strength calibration (0.0 - 1.0)
CREATE TABLE skill_rules (
    skill_id TEXT NOT NULL REFERENCES skills(id),
    rule_id TEXT NOT NULL,
    strength REAL NOT NULL DEFAULT 0.5,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (skill_id, rule_id)
);

-- Uncertainty queue for low-confidence generalizations
CREATE TABLE uncertainty_queue (
    id TEXT PRIMARY KEY,
    pattern_json TEXT NOT NULL,     -- ExtractedPattern
    reason TEXT NOT NULL,
    confidence REAL NOT NULL,
    suggested_queries TEXT NOT NULL, -- JSON array
    auto_mine_attempts INTEGER NOT NULL DEFAULT 0,
    last_mined_at TEXT,
    status TEXT NOT NULL,            -- pending | resolved | discarded
    created_at TEXT NOT NULL
);

CREATE INDEX idx_uncertainty_status ON uncertainty_queue(status);

-- Redaction reports for privacy and secret-scrubbing
CREATE TABLE redaction_reports (
    id INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL,
    report_json TEXT NOT NULL,   -- RedactionReport
    created_at TEXT NOT NULL
);

CREATE INDEX idx_redaction_session ON redaction_reports(session_id);

-- Prompt injection reports for safety filtering
CREATE TABLE injection_reports (
    id INTEGER PRIMARY KEY,
    session_id TEXT NOT NULL,
    acip_version TEXT,
    acip_mode TEXT,
    acip_audit_mode INTEGER,
    report_json TEXT NOT NULL,   -- InjectionReport
    created_at TEXT NOT NULL
);

CREATE INDEX idx_injection_session ON injection_reports(session_id);

-- Command safety events (DCG decisions + policy enforcement)
CREATE TABLE command_safety_events (
    id INTEGER PRIMARY KEY,
    session_id TEXT,
    command TEXT NOT NULL,
    dcg_version TEXT,
    dcg_pack TEXT,
    decision_json TEXT NOT NULL,  -- DcgDecision
    created_at TEXT NOT NULL
);

CREATE INDEX idx_command_safety_session ON command_safety_events(session_id);

-- Skill usage tracking
CREATE TABLE skill_usage (
    id INTEGER PRIMARY KEY,
    skill_id TEXT NOT NULL REFERENCES skills(id),
    project_path TEXT,
    used_at TEXT NOT NULL,
    disclosure_level INTEGER NOT NULL,
    context_keywords TEXT,  -- JSON array
    success_signal INTEGER,  -- 1 = worked well, 0 = didn't help, NULL = unknown
    experiment_id TEXT,
    variant_id TEXT
);

-- Skill usage events (full detail for effectiveness analysis)
CREATE TABLE skill_usage_events (
    id TEXT PRIMARY KEY,
    skill_id TEXT NOT NULL REFERENCES skills(id),
    session_id TEXT NOT NULL,
    loaded_at TEXT NOT NULL,
    disclosure_level TEXT NOT NULL,   -- JSON
    discovery_method TEXT NOT NULL,   -- JSON
    experiment_id TEXT,
    variant_id TEXT,
    outcome TEXT,                     -- JSON
    feedback TEXT                     -- JSON
);

-- Per-rule outcomes for calibration
CREATE TABLE rule_outcomes (
    id TEXT PRIMARY KEY,
    skill_id TEXT NOT NULL REFERENCES skills(id),
    rule_id TEXT NOT NULL,
    session_id TEXT NOT NULL,
    followed INTEGER NOT NULL,
    outcome TEXT NOT NULL,     -- JSON SessionOutcome
    created_at TEXT NOT NULL
);

-- UBS static analysis reports (quality gates)
CREATE TABLE ubs_reports (
    id INTEGER PRIMARY KEY,
    project_path TEXT,
    run_at TEXT NOT NULL,
    exit_code INTEGER NOT NULL,
    report_json TEXT NOT NULL      -- UbsReport
);

CREATE INDEX idx_ubs_project ON ubs_reports(project_path);

-- CM (cass-memory) rule link registry
CREATE TABLE cm_rule_links (
    id TEXT PRIMARY KEY,
    cm_rule_id TEXT NOT NULL,
    ms_rule_id TEXT NOT NULL,
    linkage_json TEXT NOT NULL,    -- CmRuleLink
    updated_at TEXT NOT NULL
);

CREATE INDEX idx_cm_rule ON cm_rule_links(cm_rule_id);

-- CM sync state (import/export checkpoints)
CREATE TABLE cm_sync_state (
    id INTEGER PRIMARY KEY,
    cm_db_path TEXT,
    last_imported_at TEXT,
    last_exported_at TEXT,
    status_json TEXT,              -- CmSyncStatus
    updated_at TEXT NOT NULL
);

-- A/B experiments for skill variants
CREATE TABLE skill_experiments (
    id TEXT PRIMARY KEY,
    skill_id TEXT NOT NULL REFERENCES skills(id),
    scope TEXT NOT NULL DEFAULT 'skill', -- skill | slice
    scope_id TEXT,                       -- slice_id if scope = slice
    variants_json TEXT NOT NULL,      -- Vec<ExperimentVariant>
    allocation_json TEXT NOT NULL,    -- AllocationStrategy
    status TEXT NOT NULL,
    started_at TEXT NOT NULL
);

-- Local reservation fallback (when Agent Mail is unavailable)
CREATE TABLE skill_reservations (
    id TEXT PRIMARY KEY,
    path_pattern TEXT NOT NULL,
    holder TEXT NOT NULL,
    exclusive INTEGER NOT NULL,
    expires_at TEXT NOT NULL,
    created_at TEXT NOT NULL
);

-- Skill relationships
CREATE TABLE skill_dependencies (
    skill_id TEXT NOT NULL REFERENCES skills(id),
    depends_on TEXT NOT NULL REFERENCES skills(id),
    PRIMARY KEY (skill_id, depends_on)
);

-- Capability index (for "provides")
CREATE TABLE skill_capabilities (
    capability TEXT NOT NULL,
    skill_id TEXT NOT NULL REFERENCES skills(id),
    PRIMARY KEY (capability, skill_id)
);

-- Build sessions (CASS integration)
CREATE TABLE build_sessions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    status TEXT NOT NULL,  -- 'draft', 'refining', 'complete', 'published'

    -- CASS queries that seeded this build
    cass_queries TEXT NOT NULL,  -- JSON array

    -- Extracted patterns
    patterns_json TEXT NOT NULL,

    -- Generated skill (in progress or complete)
    draft_skill_json TEXT,

    -- Deterministic source-of-truth
    skill_spec_json TEXT,   -- SkillSpec (structured parts)

    -- Iteration tracking
    iteration_count INTEGER NOT NULL DEFAULT 0,
    last_feedback TEXT,

    -- Timestamps
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Config store
CREATE TABLE config (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Two-phase commit transactions
CREATE TABLE tx_log (
    id TEXT PRIMARY KEY,
    entity_type TEXT NOT NULL,   -- skill | usage | config | build
    entity_id TEXT NOT NULL,
    phase TEXT NOT NULL,         -- prepare | commit | complete
    payload_json TEXT NOT NULL,
    created_at TEXT NOT NULL
);

-- CASS session fingerprints for incremental processing
CREATE TABLE cass_fingerprints (
    session_id TEXT PRIMARY KEY,
    content_hash TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Indexes
CREATE INDEX idx_skills_name ON skills(name);
CREATE INDEX idx_skills_modified ON skills(modified_at);
CREATE INDEX idx_skills_quality ON skills(quality_score DESC);
CREATE INDEX idx_usage_skill ON skill_usage(skill_id);
CREATE INDEX idx_usage_time ON skill_usage(used_at);
```

### 3.3 Git Archive Structure (Human-Readable Persistence)

```
~/.local/share/ms/archive/
├── .git/
├── skills/
│   ├── by-id/
│   │   ├── ntm/
│   │   │   ├── metadata.yaml
│   │   │   ├── skill.spec.json
│   │   │   ├── spec.lens.json
│   │   │   ├── SKILL.md
│   │   │   ├── evidence.json
│   │   │   ├── evidence/
│   │   │   │   ├── rule-1.md
│   │   │   │   └── rule-3.md
│   │   │   ├── slices.json
│   │   │   ├── tests/
│   │   │   │   └── basic.yaml
│   │   │   └── usage-log.jsonl
│   │   └── planning-workflow/
│   │       ├── metadata.yaml
│   │       ├── skill.spec.json
│   │       ├── spec.lens.json
│   │       ├── SKILL.md
│   │       ├── evidence.json
│   │       ├── slices.json
│   │       └── usage-log.jsonl
│   └── by-source/
│       └── agent_flywheel_clawdbot_skills_and_integrations/
│           └── ... (symlinks or copies)
├── builds/
│   ├── session-abc123/
│   │   ├── manifest.yaml
│   │   ├── patterns.md
│   │   ├── evidence.json
│   │   ├── redaction-report.json
│   │   ├── skill.spec.json
│   │   ├── spec.lens.json
│   │   ├── draft-v1.md
│   │   ├── draft-v2.md
│   │   └── final.md
│   └── session-def456/
│       └── ...
├── bundles/
│   └── published/
│       └── ...
└── README.md
```

### 3.4 Dependency Graph and Resolution

Skills declare dependencies (`requires`), capabilities (`provides`), and environment requirements
(platforms, tools, env vars) in metadata.
ms builds a dependency graph to resolve load order, detect cycles, and auto-load prerequisites.

```rust
#[derive(Debug, Clone)]
pub struct DependencyGraph {
    pub nodes: HashSet<String>,           // skill ids
    pub edges: Vec<DependencyEdge>,       // skill -> depends_on
}

#[derive(Debug, Clone)]
pub struct DependencyEdge {
    pub skill_id: String,
    pub depends_on: String,
}

#[derive(Debug, Clone)]
pub struct ResolvedDependencyPlan {
    pub ordered: Vec<SkillLoadPlan>,      // topo-sorted load order
    pub missing: Vec<String>,             // missing dependencies
    pub cycles: Vec<Vec<String>>,         // cycle groups
}

#[derive(Debug, Clone)]
pub struct SkillLoadPlan {
    pub skill_id: String,
    pub disclosure: DisclosurePlan,
    pub reason: String,
}

pub enum DependencyLoadMode {
    Off,
    Auto,
    Full,       // dependencies at full disclosure
    Overview,   // dependencies at overview/minimal
}

pub struct DependencyResolver {
    registry: SkillRegistry,
    max_depth: usize,
}

impl DependencyResolver {
    pub fn resolve(&self, root: &str, mode: DependencyLoadMode) -> ResolvedDependencyPlan {
        // 1) expand dependency closure (BFS with depth limit)
        // 2) detect missing skills
        // 3) detect cycles (Tarjan / DFS back-edge)
        // 4) topologically sort and assign disclosure levels
        unimplemented!()
    }
}
```

Default behavior: `ms load` uses `DependencyLoadMode::Auto` (load dependencies
at `overview` disclosure, root skill at the requested level).

#### 3.4.1 Skill Aliases and Deprecation

Renames are inevitable. ms preserves backward compatibility by maintaining
alias mappings (old id → canonical id) and surfacing deprecations with explicit
replacements.

```rust
pub struct AliasResolver {
    db: Connection,
}

#[derive(Debug, Clone)]
pub struct AliasResolution {
    pub canonical_id: String,
    pub alias_type: String,  // alias | deprecated
    pub replaced_by: Option<String>,
}

impl AliasResolver {
    pub fn resolve(&self, id_or_alias: &str) -> Option<AliasResolution> {
        // 1) If skill id exists, return canonical
        // 2) Otherwise check skill_aliases table
        // 3) Return alias metadata for UI warnings
        unimplemented!()
    }
}
```

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
```
base < org < project < user
```

**Layered Skill Registry:**

```rust
pub struct LayeredRegistry {
    pub layers: Vec<SkillLayer>, // ordered by precedence
    pub registries: HashMap<SkillLayer, SkillRegistry>,
}

impl LayeredRegistry {
    /// Return the effective skill, resolving conflicts by layer
    pub fn effective(&self, skill_id: &str) -> Result<ResolvedSkill> {
        let mut candidates = Vec::new();
        for layer in &self.layers {
            if let Some(skill) = self.registries.get(layer).and_then(|r| r.get(skill_id).ok()) {
                candidates.push(skill);
            }
        }

        resolve_conflicts(candidates, ConflictStrategy::PreferHigher)
    }
}

#[derive(Debug, Clone)]
pub struct ResolvedSkill {
    pub skill: Skill,
    pub conflicts: Vec<ConflictDetail>,
}

#[derive(Debug, Clone)]
pub struct ConflictDetail {
    pub section: String,
    pub higher_layer: SkillLayer,
    pub lower_layer: SkillLayer,
    pub resolution: ConflictResolution,
}

pub enum ConflictStrategy {
    PreferHigher,
    PreferLower,
    Interactive,
}

/// How to merge non-identical sections before falling back to strategy
pub enum MergeStrategy {
    /// Merge only when sections are non-overlapping
    Auto,
    /// Prefer higher-layer rules/pitfalls, lower-layer examples/references
    PreferSections,
}

pub enum ConflictResolution {
    UseHigher,
    UseLower,
    Merge(String), // merged section content
}
```

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

```rust
pub struct ConflictMerger;

impl ConflictMerger {
    pub fn resolve(
        &self,
        higher: &SkillSpec,
        lower: &SkillSpec,
        strategy: ConflictStrategy,
        merge_strategy: MergeStrategy,
    ) -> Result<ResolvedSkill> {
        let diffs = section_diff(higher, lower);

        // Auto-merge if changes are non-overlapping
        if matches!(merge_strategy, MergeStrategy::Auto) && diffs.non_overlapping() {
            return Ok(ResolvedSkill {
                skill: merge_sections(higher, lower)?,
                conflicts: vec![],
            });
        }

        // Prefer sections across layers to keep lower-layer examples
        if matches!(merge_strategy, MergeStrategy::PreferSections) {
            return Ok(ResolvedSkill {
                skill: merge_by_section_preference(higher, lower)?,
                conflicts: diffs.to_conflicts(SkillLayer::User, SkillLayer::Project),
            });
        }

        match strategy {
            ConflictStrategy::PreferHigher => Ok(ResolvedSkill {
                skill: higher.to_skill(),
                conflicts: diffs.to_conflicts(SkillLayer::User, SkillLayer::Project),
            }),
            ConflictStrategy::PreferLower => Ok(ResolvedSkill {
                skill: lower.to_skill(),
                conflicts: diffs.to_conflicts(SkillLayer::Project, SkillLayer::User),
            }),
            ConflictStrategy::Interactive => Err(anyhow!("Interactive resolution required")),
        }
    }
}

fn section_diff(higher: &SkillSpec, lower: &SkillSpec) -> SectionDiff { unimplemented!() }
fn merge_sections(higher: &SkillSpec, lower: &SkillSpec) -> Result<Skill> { unimplemented!() }
fn merge_by_section_preference(higher: &SkillSpec, lower: &SkillSpec) -> Result<Skill> { unimplemented!() }
```

When conflicts remain, ms surfaces a guided diff in `ms resolve` showing the
exact section differences and suggested merges.

**Block-Level Overlays:**

Beyond whole-skill overrides, higher layers can provide **overlay files** that patch
specific block IDs without copying the entire skill. This enables surgical policy
additions and reduces duplication/drift.

```rust
/// Overlay operations for surgical skill modifications
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillOverlay {
    /// Target skill id
    pub skill_id: String,

    /// Layer that provides this overlay
    pub layer: SkillLayer,

    /// Ordered list of patch operations
    pub operations: Vec<OverlayOp>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OverlayOp {
    /// Replace a block's content entirely
    ReplaceBlock { block_id: String, content: String },

    /// Delete a block
    DeleteBlock { block_id: String },

    /// Insert a new block after an existing one
    InsertAfter { after_block_id: String, new_block: SkillBlock },

    /// Append items to a checklist block
    AppendToChecklist { block_id: String, items: Vec<String> },

    /// Prepend a critical rule (inserted at top of rules section)
    PrependRule { rule: SkillBlock },

    /// Override metadata fields
    PatchMetadata { patches: HashMap<String, serde_json::Value> },
}

impl LayeredRegistry {
    /// Apply overlays from higher layers to base skill
    pub fn apply_overlays(&self, skill: &Skill, overlays: &[SkillOverlay]) -> Result<Skill> {
        let mut spec = skill.to_spec()?;

        for overlay in overlays {
            for op in &overlay.operations {
                match op {
                    OverlayOp::ReplaceBlock { block_id, content } => {
                        spec.replace_block(block_id, content)?;
                    }
                    OverlayOp::DeleteBlock { block_id } => {
                        spec.delete_block(block_id)?;
                    }
                    OverlayOp::InsertAfter { after_block_id, new_block } => {
                        spec.insert_after(after_block_id, new_block.clone())?;
                    }
                    OverlayOp::AppendToChecklist { block_id, items } => {
                        spec.append_checklist_items(block_id, items)?;
                    }
                    OverlayOp::PrependRule { rule } => {
                        spec.prepend_rule(rule.clone())?;
                    }
                    OverlayOp::PatchMetadata { patches } => {
                        spec.patch_metadata(patches)?;
                    }
                }
            }
        }

        spec.compile()
    }
}
```

**Overlay File Format:**

Overlays are stored in the layer's skill directory as `skill.overlay.json`:

```json
{
  "skill_id": "nextjs-patterns",
  "operations": [
    {
      "type": "replace_block",
      "block_id": "rule-3",
      "content": "NEVER use transition-all; prefer specific properties per new policy."
    },
    {
      "type": "append_to_checklist",
      "block_id": "checklist-pre-deploy",
      "items": ["Run compliance audit: `audit-tool check`"]
    },
    {
      "type": "prepend_rule",
      "rule": {
        "id": "org-rule-1",
        "type": "rule",
        "content": "All API routes must use the org auth middleware."
      }
    }
  ]
}
```

**Benefits:**

- **No duplication:** Org/user layers don't copy entire skills
- **Drift prevention:** Base skill updates propagate automatically
- **Surgical policy:** Add compliance rules without rewriting
- **Clear provenance:** Each block records which layer modified it

### 3.6 Skill Spec and Deterministic Compilation

SKILL.md is a rendered artifact. The source-of-truth is a structured `SkillSpec`
that can be deterministically compiled into SKILL.md. This ensures reproducible
output, stable diffs, and safe automated edits.

```rust
pub enum CompileTarget {
    Claude,
    OpenAI,
    Cursor,
    GenericMarkdown,
}

pub struct SkillCompiler;

impl SkillCompiler {
    /// Compile SkillSpec into SKILL.md (deterministic ordering)
    pub fn compile(spec: &SkillSpec, target: CompileTarget) -> Result<String> {
        // 1) render frontmatter
        // 2) render sections in order
        // 3) render blocks with stable formatting
        // 4) apply target-specific frontmatter / tool hints
        unimplemented!()
    }

    /// Validate spec schema and required sections
    pub fn validate(spec: &SkillSpec) -> Result<()> {
        // Ensure required fields, unique rule ids, no empty sections
        unimplemented!()
    }
}
```

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

```rust
pub struct TxManager {
    db: Connection,
    git: GitArchive,
    tx_dir: PathBuf, // .ms/tx/
}

impl TxManager {
    pub fn write_skill(&self, skill: &SkillSpec) -> Result<()> {
        let tx = TxRecord::prepare("skill", &skill.id, skill)?;

        // Phase 1: prepare
        self.write_tx_record(&tx)?;
        self.db_write_pending(&tx)?;

        // Phase 2: commit
        self.git_commit(&tx)?;
        self.db_mark_committed(&tx)?;
        self.cleanup_tx(&tx)?;

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TxRecord {
    pub id: String,
    pub entity_type: String,
    pub entity_id: String,
    pub phase: String,
    pub payload_json: String,
    pub created_at: DateTime<Utc>,
}
```

Recovery is automatic on startup and via `ms doctor --fix`.

### 3.7.1 Global File Locking

While SQLite handles internal concurrency with WAL mode, the dual-persistence
pattern (SQLite + Git) requires coordination when multiple `ms` processes run
concurrently (e.g., parallel agent invocations, IDE background indexer + CLI).

**Optional Single-Writer Daemon (`msd`):**
- Holds hot indices/caches in memory and serializes writes.
- CLI becomes a thin client when daemon is running (lower p95 latency).

```rust
use std::fs::{File, OpenOptions};
use std::io;
use std::path::PathBuf;

#[cfg(unix)]
use std::os::unix::fs::FileExt;

/// Advisory file lock for coordinating dual-persistence writes
pub struct GlobalLock {
    lock_file: File,
    lock_path: PathBuf,
}

impl GlobalLock {
    const LOCK_FILENAME: &'static str = ".ms/ms.lock";

    /// Acquire exclusive lock (blocking)
    pub fn acquire(ms_root: &Path) -> io::Result<Self> {
        let lock_path = ms_root.join(Self::LOCK_FILENAME);
        std::fs::create_dir_all(lock_path.parent().unwrap())?;

        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&lock_path)?;

        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = lock_file.as_raw_fd();
            // LOCK_EX = exclusive, blocks until acquired
            unsafe { libc::flock(fd, libc::LOCK_EX) };
        }

        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use winapi::um::fileapi::LockFileEx;
            use winapi::um::minwinbase::LOCKFILE_EXCLUSIVE_LOCK;
            let handle = lock_file.as_raw_handle();
            unsafe {
                let mut overlapped = std::mem::zeroed();
                LockFileEx(
                    handle as *mut _,
                    LOCKFILE_EXCLUSIVE_LOCK,
                    0,
                    !0,
                    !0,
                    &mut overlapped,
                );
            }
        }

        Ok(Self { lock_file, lock_path })
    }

    /// Try to acquire lock without blocking
    pub fn try_acquire(ms_root: &Path) -> io::Result<Option<Self>> {
        let lock_path = ms_root.join(Self::LOCK_FILENAME);
        std::fs::create_dir_all(lock_path.parent().unwrap())?;

        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(&lock_path)?;

        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = lock_file.as_raw_fd();
            // LOCK_NB = non-blocking
            let result = unsafe { libc::flock(fd, libc::LOCK_EX | libc::LOCK_NB) };
            if result != 0 {
                return Ok(None); // Lock held by another process
            }
        }

        Ok(Some(Self { lock_file, lock_path }))
    }

    /// Acquire with timeout (polling fallback for portability)
    pub fn acquire_timeout(ms_root: &Path, timeout: Duration) -> io::Result<Option<Self>> {
        let start = std::time::Instant::now();
        let poll_interval = Duration::from_millis(50);

        while start.elapsed() < timeout {
            if let Some(lock) = Self::try_acquire(ms_root)? {
                return Ok(Some(lock));
            }
            std::thread::sleep(poll_interval);
        }

        Ok(None)
    }
}

impl Drop for GlobalLock {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::unix::io::AsRawFd;
            let fd = self.lock_file.as_raw_fd();
            unsafe { libc::flock(fd, libc::LOCK_UN) };
        }

        #[cfg(windows)]
        {
            use std::os::windows::io::AsRawHandle;
            use winapi::um::fileapi::UnlockFileEx;
            let handle = self.lock_file.as_raw_handle();
            unsafe {
                let mut overlapped = std::mem::zeroed();
                UnlockFileEx(handle as *mut _, 0, !0, !0, &mut overlapped);
            }
        }
    }
}
```

**Locked TxManager:**

```rust
impl TxManager {
    /// Write skill with global lock coordination
    pub fn write_skill_locked(&self, skill: &SkillSpec) -> Result<()> {
        let _lock = GlobalLock::acquire_timeout(&self.ms_root, Duration::from_secs(30))
            .map_err(|e| anyhow!("Failed to acquire lock: {}", e))?
            .ok_or_else(|| anyhow!("Timeout waiting for global lock"))?;

        self.write_skill(skill)
        // Lock released on drop
    }

    /// Batch write with single lock acquisition
    pub fn write_skills_batch(&self, skills: &[SkillSpec]) -> Result<()> {
        let _lock = GlobalLock::acquire(&self.ms_root)?;

        for skill in skills {
            self.write_skill(skill)?;
        }

        Ok(())
    }
}
```

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

```bash
# Check lock status
ms doctor --check-lock

# Force break stale lock (with pid check)
ms doctor --break-lock

# Show lock holder
ms lock status
```

The lock file includes a JSON payload with holder PID and timestamp, enabling
stale lock detection (process no longer running) and diagnostics.

---

## 4. CLI Command Reference

### 4.1 Core Commands

```bash
# Initialize ms in current project
ms init
ms init --global  # Initialize global config only

# Index skills from configured paths
ms index
ms index --path /data/projects/agent_flywheel_clawdbot_skills_and_integrations
ms index --all  # Re-index everything
ms index --watch  # Watch for changes (daemon mode)
ms index --cass-incremental  # Only process new/changed sessions

# RU → ms index automation (multi-machine sync)
ru sync --non-interactive --json
ms index --all

# Search for skills
ms search "git workflow"
ms search "git workflow" --limit 10
ms search "error handling" --tags rust,cli
ms search "testing" --min-quality 0.7
ms search "legacy patterns" --include-deprecated
ms search "logging" --layer project  # restrict to a layer

# Load a skill (progressive disclosure)
ms load ntm
ms load ntm --level 1  # Just overview
ms load ntm --level 2  # Include key sections
ms load ntm --level 3  # Full content
ms load ntm --full     # Everything including assets
ms load ntm --pack 800 # Token-budgeted slice pack
ms load ntm --pack 800 --mode coverage_first  # Bias toward rule coverage
ms load ntm --pack 800 --mode pitfall_safe --max-per-group 2
ms load ntm --deps auto      # Load prerequisites at overview
ms load ntm --deps off       # Disable dependency auto-load
ms load ntm --deps full      # Load prerequisites at full disclosure
ms load ntm --robot    # JSON output for automation

# Suggest skills for current context
ms suggest
ms suggest --cwd /data/projects/my-rust-project
ms suggest --file src/main.rs
ms suggest --query "how to handle async errors"
ms suggest --pack 800  # Suggest packed slices within token budget
ms suggest --explain   # Include signal breakdown
ms suggest --pack 800 --mode pitfall_safe --max-per-group 2
ms suggest --include-deprecated
ms suggest --for-ntm myproject --agents 6 --budget 800 --objective coverage_first

# Show skill details
ms show ntm
ms show ntm --usage  # Include usage stats
ms show ntm --deps   # Show dependency graph
ms show ntm --layer user  # show a specific layer

# Round-trip editing (Spec ↔ Markdown)
ms edit ntm
ms edit ntm --allow-lossy  # Allow lossy edits if parse fails
ms edit ntm --import-markdown  # Repair/import Markdown into spec (opt-in)
ms fmt ntm                 # Re-render deterministically
ms diff ntm --semantic      # Spec-level diff, not raw markdown
ms review ntm               # Semantic review grouped by rule type

# Manage aliases and deprecations
ms alias list ntm
ms alias add legacy-cli-patterns ntm
ms alias resolve legacy-cli-patterns
ms alias remove legacy-cli-patterns

# Check environment requirements
ms requirements ntm
ms requirements ntm --project /data/projects/my-rust-project
ms requirements ntm --robot

# Resolve dependency order
ms deps ntm
ms deps ntm --graph --format json

# Resolve conflicts across layers
ms resolve ntm
ms resolve ntm --strategy interactive
ms resolve ntm --diff  # show section-level diffs
ms resolve ntm --merge-strategy prefer_sections

# Inspect rule-level evidence and provenance
ms evidence ntm
ms evidence ntm --rule "rule-3"
ms evidence ntm --graph  # Export provenance graph (JSON)
ms evidence ntm --timeline  # Evidence by session chronology
ms evidence ntm --open  # Open source excerpts (redacted)
```

### 4.2 Build Commands (CASS Integration)

```bash
# Start interactive skill building session
ms build
ms build --name "rust-error-handling"

# Build from specific CASS query
ms build --from-cass "error handling in rust"
ms build --from-cass "how I implemented auth" --sessions 20
ms build --from-cass "auth tokens" --redaction-report  # emit redaction report
ms build --from-cass "api keys" --no-redact  # only if you explicitly accept risk
ms build --from-cass "auth mistakes" --no-antipatterns  # skip counter-examples
ms build --from-cass "error handling" --output-spec skill.spec.json
ms build --from-cass "error handling" --min-session-quality 0.6
ms build --from-cass "auth issues" --no-injection-filter  # only if you explicitly accept risk
ms build --from-cass "error handling" --generalize heuristic
ms build --from-cass "error handling" --generalize llm --llm-critique

# Resume existing build session
ms build --resume session-abc123

# Non-interactive build (fully automated)
ms build --auto --from-cass "testing patterns" --min-confidence 0.8

# Compile a spec to SKILL.md (deterministic)
ms compile skill.spec.json --out SKILL.md
ms compile skill.spec.json --out SKILL.md --target claude
ms spec validate skill.spec.json

# Resolve low-confidence patterns
ms build --resolve-uncertainties
ms uncertainties list
ms uncertainties resolve UNK-123 --mine "error handling in rust"

# Build commands within interactive session
# (These become subcommands in interactive mode)
#   /mine <query>     - Mine more sessions
#   /patterns         - Show extracted patterns
#   /draft            - Generate skill draft
#   /spec             - Show or export SkillSpec
#   /refine           - Iterate on current draft
#   /preview          - Preview rendered skill
#   /save             - Save current state
#   /publish          - Finalize and publish
#   /abort            - Discard session
```

### 4.3 Bundle Commands

```bash
# Create a bundle from local skills
ms bundle create my-skills --skills ntm,planning-workflow,dcg
ms bundle create rust-toolkit --tags rust --min-quality 0.8

# Publish bundle to GitHub
ms bundle publish my-skills --repo user/skill-bundle
ms bundle publish my-skills --gist  # As a GitHub Gist
ms bundle publish my-skills --sign --key ~/.keys/ms_ed25519

# Install bundle from GitHub
ms bundle install user/skill-bundle
ms bundle install user/skill-bundle --skills ntm,dcg  # Specific skills only
ms bundle install user/skill-bundle --channel beta --verify

# List installed bundles
ms bundle list

# Update installed bundles
ms bundle update
ms bundle update user/skill-bundle
ms bundle update --channel beta
ms bundle verify user/skill-bundle
```

### 4.4 Maintenance Commands

```bash
# Check for updates
ms update --check
ms update  # Install update if available

# Health check
ms doctor
ms doctor --fix  # Attempt auto-fixes
ms doctor --check=transactions
ms doctor --check=security
ms doctor --check=requirements
ms doctor --check=perf
ms doctor --preflight --context /tmp/ms_ctx.json

# Prune tombstoned data (requires approval)
ms prune --scope archive

# Skill pruning/evolution proposals (non-destructive by default)
ms prune --scope skills --dry-run
ms prune --scope skills --min-uses 5 --window 30d
ms prune --scope skills --similarity 0.8 --emit-beads
ms prune --scope skills --apply --require-confirmation

# Configuration
ms config show
ms config set search.default_limit 20
ms config paths add /data/projects/skills
ms config paths list

# Stats and analytics
ms stats
ms stats --skill ntm  # Usage stats for specific skill
ms stats --period week

# Staleness and drift checks
ms stale
ms stale --project /data/projects/my-rust-project
ms stale --min-severity medium

# Skill tests
ms test ntm
ms test ntm --report junit
ms test --all

# Live preview of packed content
ms load ntm --pack 800 --preview

# Optional single-writer daemon
msd start
msd status
msd stop

# Skill simulation sandbox
ms simulate ntm
ms simulate ntm --project /data/projects/my-rust-project
ms simulate ntm --report json
```

### 4.5 Robot Mode (Comprehensive Specification)

Following the xf pattern exactly, robot mode provides machine-readable JSON output for all operations. This enables tight integration with orchestration tools (NTM, BV) and other agents.

**Core Protocol:**

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         ROBOT MODE PROTOCOL                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Input:   Command + --robot flag OR --robot-* variant                       │
│  Output:  stdout = JSON data (always)                                       │
│           stderr = diagnostics/progress (human-readable)                    │
│  Exit:    0 = success, 1 = error, 2 = not implemented                       │
│                                                                             │
│  Contract:                                                                  │
│  • stdout is ALWAYS valid JSON when --robot is used                        │
│  • Errors are returned as JSON objects with "error" field                  │
│  • Progress/diagnostics go to stderr, never stdout                         │
│  • Empty results return empty arrays [], not null                          │
│  • Timestamps are ISO 8601 UTC                                              │
│  • IDs are stable across invocations                                        │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Robot Mode Commands:**

```bash
# Global robot flags (alternative to --robot on individual commands)
ms --robot-status              # Full registry status
ms --robot-health              # Health check summary
ms --robot-suggest             # Context-aware suggestions
ms --robot-search="query"      # Search as JSON
ms --robot-build-status        # Active build sessions
ms --robot-cass-status         # CASS integration status

# Per-command robot mode
ms list --robot
ms search "query" --robot
ms show skill-id --robot
ms load skill-id --robot
ms review skill-id --robot
ms suggest --robot
ms build --robot --status
ms stats --robot
ms doctor --robot
ms sync status --robot
```

**Output Schemas:**

```rust
/// Standard robot response wrapper
#[derive(Serialize)]
pub struct RobotResponse<T> {
    /// Operation status
    pub status: RobotStatus,

    /// Timestamp of response
    pub timestamp: DateTime<Utc>,

    /// ms version
    pub version: String,

    /// Response payload (varies by command)
    pub data: T,

    /// Optional warnings (non-fatal issues)
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RobotStatus {
    Ok,
    Error { code: String, message: String },
    Partial { completed: usize, failed: usize },
}

/// --robot-status response
#[derive(Serialize)]
pub struct StatusResponse {
    pub registry: RegistryStatus,
    pub search_index: IndexStatus,
    pub cass_integration: CassStatus,
    pub active_builds: Vec<BuildSessionSummary>,
    pub config: ConfigSummary,
}

#[derive(Serialize)]
pub struct RegistryStatus {
    pub total_skills: usize,
    pub indexed_skills: usize,
    pub local_skills: usize,
    pub upstream_skills: usize,
    pub modified_skills: usize,
    pub last_index_update: Option<DateTime<Utc>>,
}

/// --robot-suggest response
#[derive(Serialize)]
pub struct SuggestResponse {
    pub context: SuggestionContext,
    pub suggestions: Vec<SuggestionItem>,
    pub swarm_plan: Option<SwarmPlan>,
    pub explain: Option<SuggestionExplain>,
}

#[derive(Serialize)]
pub struct SuggestionItem {
    pub skill_id: String,
    pub name: String,
    pub score: f32,
    pub reason: String,
    pub disclosure_level: String,
    pub token_estimate: usize,
    pub pack_budget: Option<usize>,
    pub packed_token_estimate: Option<usize>,
    pub slice_count: Option<usize>,
    pub dependencies: Vec<String>,
    pub layer: Option<String>,
    pub conflicts: Vec<String>,
    pub requirements: Option<RequirementStatus>,
    pub explanation: Option<SuggestionExplanation>,
}

#[derive(Serialize)]
pub struct SuggestionExplain {
    pub enabled: bool,
    pub signals: Vec<SuggestionSignalExplain>,
}

#[derive(Serialize)]
pub struct SuggestionExplanation {
    pub matched_triggers: Vec<String>,
    pub signal_scores: Vec<SignalScore>,
    pub rrf_components: RrfBreakdown,
}

#[derive(Serialize)]
pub struct SuggestionSignalExplain {
    pub signal_type: String,
    pub value: String,
    pub weight: f32,
}

#[derive(Serialize)]
pub struct SignalScore {
    pub signal: String,
    pub contribution: f32,
}

#[derive(Serialize)]
pub struct RrfBreakdown {
    pub bm25_rank: Option<usize>,
    pub vector_rank: Option<usize>,
    pub rrf_score: f32,
}

/// --robot-build-status response
#[derive(Serialize)]
pub struct BuildStatusResponse {
    pub active_sessions: Vec<BuildSessionDetail>,
    pub recent_completed: Vec<BuildSessionSummary>,
    pub queued_patterns: usize,
    pub queued_uncertainties: usize,
}

/// --robot requirements response
#[derive(Serialize)]
pub struct RequirementsResponse {
    pub skill_id: String,
    pub requirements: SkillRequirements,
    pub status: RequirementStatus,
    pub environment: EnvironmentSnapshot,
}

#[derive(Serialize)]
pub struct BuildSessionDetail {
    pub session_id: String,
    pub skill_name: String,
    pub state: BuildState,
    pub iteration: usize,
    pub patterns_used: usize,
    pub patterns_available: usize,
    pub started_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    pub checkpoint_path: Option<PathBuf>,
}
```

**Error Response Format:**

```json
{
  "status": {
    "error": {
      "code": "SKILL_NOT_FOUND",
      "message": "Skill 'nonexistent' not found in registry"
    }
  },
  "timestamp": "2026-01-13T15:30:00Z",
  "version": "0.1.0",
  "data": null,
  "warnings": []
}
```

**Integration Examples:**

```bash
# NTM integration: spawn agent with skills
skills=$(ms --robot-suggest | jq -r '.data.suggestions[].skill_id')
for skill in $skills; do
  content=$(ms load "$skill" --robot --level=full | jq -r '.data.content')
  # Inject into agent prompt
done

# BV integration: find skills for current bead
bead_type=$(bv show BD-123 --json | jq -r '.type')
relevant_skills=$(ms search "$bead_type" --robot | jq -r '.data.results[].skill_id')

# Automated skill generation pipeline
ms build --robot --from-cass "nextjs ui" --auto --max-iterations 10 | \
  jq -r '.data.generated_skill_path'

# Health monitoring
ms --robot-health | jq '.data.issues[] | select(.severity == "error")'
```

### 4.6 Doctor Command

The `doctor` command performs comprehensive health checks on the ms installation, following best practices from xf and other Rust CLI tools.

```bash
ms doctor              # Run all checks
ms doctor --fix        # Attempt automatic fixes
ms doctor --robot      # JSON output for automation
ms doctor --check=db   # Run specific check
```

**Check Categories:**

```rust
pub struct DoctorReport {
    pub checks: Vec<CheckResult>,
    pub overall_status: HealthStatus,
    pub auto_fixable: Vec<String>,
}

pub struct CheckResult {
    pub check_id: String,
    pub category: CheckCategory,
    pub status: HealthStatus,
    pub message: String,
    pub details: Option<String>,
    pub fix_available: bool,
    pub fix_command: Option<String>,
}

pub enum CheckCategory {
    Database,
    SearchIndex,
    Configuration,
    CassIntegration,
    Redaction,
    Safety,
    Security,
    Toolchain,
    Requirements,
    Dependencies,
    Layers,
    Transactions,
    GitArchive,
    FileSystem,
    Permissions,
    Network,
}

pub enum HealthStatus {
    Healthy,
    Warning,
    Error,
    Unknown,
}
```

**Checks Performed:**

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           DOCTOR CHECKS                                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│ DATABASE                                                                    │
│  ☐ SQLite file exists and is readable                                      │
│  ☐ Schema version is current                                               │
│  ☐ WAL mode enabled                                                        │
│  ☐ No corruption (PRAGMA integrity_check)                                  │
│  ☐ Foreign keys consistent                                                 │
│                                                                             │
│ SEARCH INDEX                                                                │
│  ☐ Tantivy index directory exists                                          │
│  ☐ Index is not corrupted                                                  │
│  ☐ Index is in sync with database (count match)                            │
│  ☐ Embeddings are computed for all skills                                  │
│                                                                             │
│ CONFIGURATION                                                               │
│  ☐ Config file exists and is valid TOML                                    │
│  ☐ All configured paths exist                                              │
│  ☐ No deprecated config keys                                               │
│  ☐ Reasonable defaults for missing values                                  │
│                                                                             │
│ TOOLCHAIN                                                                   │
│  ☐ Project toolchain detected (node/cargo/go/etc.)                         │
│  ☐ Skill compatibility constraints parsed                                  │
│  ☐ Drift check completed (skill ranges vs project versions)                │
│                                                                             │
│ REQUIREMENTS                                                                │
│  ☐ Required tools found in PATH (git/docker/gh/etc.)                        │
│  ☐ Required environment variables present                                  │
│  ☐ Platform compatibility satisfied                                         │
│                                                                             │
│ DEPENDENCIES                                                                │
│  ☐ Dependency graph builds without errors                                  │
│  ☐ No cycles detected                                                      │
│  ☐ All required skills are present                                         │
│                                                                             │
│ LAYERS                                                                      │
│  ☐ Layer paths exist (base/org/project/user)                                │
│  ☐ No conflicting skill ids without resolution                             │
│  ☐ Layer precedence order valid                                             │
│                                                                             │
│ TRANSACTIONS                                                                │
│  ☐ No pending tx_log entries                                               │
│  ☐ Orphaned tx records resolved                                            │
│                                                                             │
│ CASS INTEGRATION                                                            │
│  ☐ CASS binary found in PATH                                               │
│  ☐ CASS is responsive (cass health)                                        │
│  ☐ CASS has indexed sessions                                               │
│  ☐ Can execute CASS queries                                                │
│                                                                             │
│ REDACTION                                                                   │
│  ☐ Redaction enabled in config                                             │
│  ☐ Redaction rules loaded (built-in + custom)                              │
│  ☐ Recent redaction report available                                       │
│  ☐ No high-risk secrets leaked in last N builds                            │
│                                                                             │
│ SAFETY                                                                      │
│  ☐ Prompt-injection filter enabled                                         │
│  ☐ Quarantine directory writable                                           │
│  ☐ No high-severity injection findings in last N builds                    │
│  ☐ Destructive ops policy enforced (approval required)                     │
│                                                                             │
│ SECURITY                                                                    │
│  ☐ Bundle signature verification enabled                                   │
│  ☐ Update signature verification enabled                                   │
│  ☐ Trusted signer keys available                                           │
│  ☐ No unsigned bundles installed (unless explicitly allowed)               │
│                                                                             │
│ GIT ARCHIVE                                                                 │
│  ☐ Archive directory exists                                                │
│  ☐ Git repository initialized                                              │
│  ☐ No uncommitted changes (or warning)                                     │
│  ☐ Remote configured (if sharing enabled)                                  │
│                                                                             │
│ FILE SYSTEM                                                                 │
│  ☐ Data directory writable                                                 │
│  ☐ Sufficient disk space (>100MB free)                                     │
│  ☐ Skill directories readable                                              │
│  ☐ No orphaned files                                                       │
│                                                                             │
│ PERMISSIONS                                                                 │
│  ☐ Config file not world-readable (security)                               │
│  ☐ Database file permissions correct                                       │
│  ☐ GitHub token secure (if configured)                                     │
│                                                                             │
│ NETWORK (optional, --check-network)                                         │
│  ☐ Can reach GitHub API                                                    │
│  ☐ Upstream bundles accessible                                             │
│  ☐ Update server reachable                                                 │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Output Example:**

```
$ ms doctor

ms doctor — health check

Database
  ✓ SQLite database exists
  ✓ Schema version current (v3)
  ✓ WAL mode enabled
  ✓ Integrity check passed

Search Index
  ✓ Tantivy index exists
  ⚠ Index out of sync (42 skills in DB, 40 indexed)
    Fix: ms index --rebuild

Configuration
  ✓ Config file valid
  ✓ Skill paths exist
  ⚠ Deprecated key 'search.rrf_weight' (use 'search.rrf_k')

CASS Integration
  ✓ CASS binary found (/usr/local/bin/cass)
  ✓ CASS responsive
  ✓ 1,247 sessions indexed

Git Archive
  ✓ Archive initialized
  ⚠ 3 uncommitted changes
    Fix: ms sync commit

Overall: HEALTHY (2 warnings)
Run 'ms doctor --fix' to auto-fix 1 issue
```

### 4.7 Shell Integration

Shell integration provides aliases, completions, and environment setup.

```bash
# Initialize shell integration
ms init bash >> ~/.bashrc    # For bash
ms init zsh >> ~/.zshrc      # For zsh
ms init fish >> ~/.config/fish/config.fish  # For fish

# Or use eval
eval "$(ms init zsh)"
```

**Generated Shell Functions:**

```bash
# Core aliases
alias mss='ms search'
alias msl='ms load'
alias msg='ms suggest'
alias msb='ms build'
alias msd='ms doctor'
alias msy='ms sync'

# Quick search (outputs to clipboard)
msc() {
    ms load "$1" --level=full | pbcopy  # macOS
    # ms load "$1" --level=full | xclip -selection clipboard  # Linux
    echo "Skill '$1' copied to clipboard"
}

# Interactive skill selector (requires fzf)
msf() {
    local skill
    skill=$(ms list --robot | jq -r '.data.skills[].id' | fzf --preview 'ms show {}')
    [[ -n "$skill" ]] && ms load "$skill"
}

# Build from recent sessions
msb-recent() {
    ms build --from-cass "$(cass search --robot --limit 1 | jq -r '.sessions[0].id')"
}

# Environment variables
export MS_DATA_DIR="${XDG_DATA_HOME:-$HOME/.local/share}/ms"
export MS_CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/ms"

# Auto-suggest on directory change (optional)
_ms_auto_suggest() {
    local suggestions
    suggestions=$(ms --robot-suggest 2>/dev/null | jq -r '.data.suggestions[:3] | .[].name' 2>/dev/null)
    [[ -n "$suggestions" ]] && echo "💡 Suggested skills: $suggestions"
}

# Uncomment to enable auto-suggestions
# chpwd_functions+=(_ms_auto_suggest)
```

**Shell Completions:**

```bash
# Zsh completions (generated)
_ms() {
    local -a commands
    commands=(
        'search:Search for skills'
        'list:List all skills'
        'show:Show skill details'
        'alias:Manage skill aliases'
        'requirements:Check environment requirements'
        'edit:Edit skill with round-trip spec'
        'fmt:Format skill deterministically'
        'diff:Diff skill semantics'
        'prune:Prune tombstoned data'
        'load:Load skill content'
        'suggest:Get contextual suggestions'
        'build:Build new skill'
        'bundle:Manage skill bundles'
        'sync:Synchronize skills'
        'doctor:Health check'
        'stats:Usage statistics'
        'config:Configuration management'
        'upgrade:Check for updates'
    )

    local -a global_opts
    global_opts=(
        '--robot[Output as JSON]'
        '--help[Show help]'
        '--version[Show version]'
        '--verbose[Verbose output]'
        '--quiet[Suppress non-essential output]'
    )

    _arguments -C \
        $global_opts \
        '1:command:->command' \
        '*::arg:->args'

    case "$state" in
        command)
            _describe 'command' commands
            ;;
        args)
            case "$words[1]" in
                search)
                    _arguments \
                        '--limit[Max results]:number' \
                        '--tags[Filter by tags]:tags' \
                        '--type[Filter by type]:type:(command code workflow constraint)' \
                        '--include-deprecated[Include deprecated skills]'
                    ;;
                alias)
                    _arguments \
                        '1:action:(list add remove resolve)' \
                        '*:skill:_ms_skills'
                    ;;
                requirements)
                    _arguments \
                        '--project[Project path for environment check]:path:_files' \
                        '*:skill:_ms_skills'
                    ;;
                suggest)
                    _arguments \
                        '--cwd[Working directory]:path:_files' \
                        '--file[Current file]:file:_files' \
                        '--query[Explicit query]:query' \
                        '--pack[Token budget]:tokens' \
                        '--mode[Pack mode]:mode:(balanced coverage_first pitfall_safe)' \
                        '--max-per-group[Max slices per group]:number' \
                        '--explain[Include signal breakdown]' \
                        '--include-deprecated[Include deprecated skills]' \
                        '--for-ntm[Swarm-aware suggestions for NTM]:project' \
                        '--agents[Agent count]:number' \
                        '--budget[Token budget per agent]:tokens' \
                        '--objective[Swarm objective]:objective:(coverage_first redundancy_min safety_first)'
                    ;;
                load)
                    _arguments \
                        '--level[Disclosure level]:level:(minimal overview standard full complete)' \
                        '--format[Output format]:format:(markdown json yaml)' \
                        '*:skill:_ms_skills'
                    ;;
                edit)
                    _arguments \
                        '--allow-lossy[Allow lossy edits if parse fails]' \
                        '*:skill:_ms_skills'
                    ;;
                fmt)
                    _arguments \
                        '--check[Check formatting without changes]' \
                        '*:skill:_ms_skills'
                    ;;
                diff)
                    _arguments \
                        '--semantic[Spec-level diff]' \
                        '*:skill:_ms_skills'
                    ;;
                prune)
                    _arguments \
                        '--scope[Prune scope]:scope:(archive bundles cache)' \
                        '--approve[Verbatim approval string]:cmd'
                    ;;
                build)
                    _arguments \
                        '--from-cass[Mine from CASS sessions]:query' \
                        '--from-sessions[Specific session files]:file:_files' \
                        '--name[Skill name]:name' \
                        '--auto[Non-interactive mode]' \
                        '--iterations[Max iterations]:number'
                    ;;
            esac
            ;;
    esac
}

_ms_skills() {
    local -a skills
    skills=(${(f)"$(ms list --robot 2>/dev/null | jq -r '.data.skills[].id' 2>/dev/null)"})
    _describe 'skill' skills
}

compdef _ms ms
```

### 4.8 MCP Server Mode

Beyond CLI, ms provides a **Model Context Protocol (MCP) server** for native agent
tool-use integration. This eliminates subprocess overhead, PATH issues, JSON parsing
brittleness, and platform differences.

**Why MCP matters:** CLI + JSON parsing works but is brittle. MCP is the native
interface for agent tool calling. Every modern agent (Claude Code, Codex CLI, Cursor)
can consume ms via MCP with dramatically less friction.

**Server Commands:**

```bash
# Start MCP server (stdio mode for Claude Code integration)
ms mcp serve

# Start with TCP for multi-agent access
ms mcp serve --tcp 127.0.0.1:9847

# Health check
ms mcp health
```

**MCP Tool Definitions:**

```rust
pub mod mcp_tools {
    use crate::prelude::*;

    /// Search skills by query with optional filters
    #[derive(Serialize, Deserialize, JsonSchema)]
    pub struct MsSearch {
        pub query: String,
        #[serde(default)]
        pub filters: SearchFilters,
        #[serde(default = "default_limit")]
        pub limit: usize,
    }

    /// Get context-aware skill suggestions
    #[derive(Serialize, Deserialize, JsonSchema)]
    pub struct MsSuggest {
        pub context: SuggestContext,
        #[serde(default)]
        pub budget_tokens: Option<usize>,
    }

    /// Load skill content with optional packing
    #[derive(Serialize, Deserialize, JsonSchema)]
    pub struct MsLoad {
        pub skill_id: String,
        #[serde(default)]
        pub pack_budget: Option<usize>,
        #[serde(default)]
        pub level: Option<DisclosureLevel>,
    }

    /// Get evidence for a specific rule
    #[derive(Serialize, Deserialize, JsonSchema)]
    pub struct MsEvidence {
        pub skill_id: String,
        pub rule_id: String,
        #[serde(default)]
        pub expand_context: usize,
    }

    /// Check build status for a skill
    #[derive(Serialize, Deserialize, JsonSchema)]
    pub struct MsBuildStatus {
        pub skill_id: String,
    }

    /// Pack slices optimally for token budget
    #[derive(Serialize, Deserialize, JsonSchema)]
    pub struct MsPack {
        pub skill_ids: Vec<String>,
        pub budget_tokens: usize,
        #[serde(default)]
        pub mode: PackMode,
    }
}
```

**Server Architecture:**

```rust
pub struct McpServer {
    /// Shared registry handle (hot indices)
    registry: Arc<SkillRegistry>,

    /// LRU cache for frequent queries
    cache: Arc<RwLock<LruCache<String, CachedResult>>>,

    /// Protocol handler
    protocol: McpProtocol,
}

impl McpServer {
    pub async fn serve_stdio(&self) -> Result<()> {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        self.protocol.run(stdin, stdout, self).await
    }

    pub async fn serve_tcp(&self, addr: SocketAddr) -> Result<()> {
        let listener = TcpListener::bind(addr).await?;
        loop {
            let (stream, _) = listener.accept().await?;
            let server = self.clone();
            tokio::spawn(async move {
                let (read, write) = stream.into_split();
                server.protocol.run(read, write, &server).await
            });
        }
    }
}
```

**Benefits over CLI:**

| Aspect | CLI Mode | MCP Mode |
|--------|----------|----------|
| Latency | ~50-100ms subprocess | ~1-5ms in-process |
| Caching | Per-invocation | Shared across requests |
| Streaming | Not supported | Partial results supported |
| Error handling | Exit codes + stderr | Structured error responses |
| Type safety | JSON schema drift risk | Schema-validated tools |

**Claude Code Integration:**

```json
// ~/.claude/mcp_servers.json
{
  "ms": {
    "command": "ms",
    "args": ["mcp", "serve"],
    "env": {}
  }
}
```

---

## 5. CASS Integration Deep Dive

### 5.1 The Mining Pipeline

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         CASS MINING PIPELINE                                │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Step 1: Query CASS                                                         │
│  ┌───────────────────────────────────────────────────────────────────────┐ │
│  │  ms calls: cass search "error handling in rust" --robot --limit 50    │ │
│  │  Returns: Session metadata, file paths, relevance scores              │ │
│  │  Incremental mode: only sessions with new hashes are processed         │ │
│  └───────────────────────────────────────────────────────────────────────┘ │
│                                      │                                      │
│                                      ▼                                      │
│  Step 2: Fetch Session Content                                              │
│  ┌───────────────────────────────────────────────────────────────────────┐ │
│  │  For each relevant session:                                           │ │
│  │  - Read full transcript from CASS                                     │ │
│  │  - Redact secrets/PII (emit redaction report)                          │ │
│  │  - Extract tool calls, code blocks, user feedback                     │ │
│  │  - Identify success/failure signals                                   │ │
│  │  - Score session quality and drop low-signal sessions                  │ │
│  │  - Detect prompt-injection content and quarantine                     │ │
│  └───────────────────────────────────────────────────────────────────────┘ │
│                                      │                                      │
│                                      ▼                                      │
│  Step 3: Pattern Extraction                                                 │
│  ┌───────────────────────────────────────────────────────────────────────┐ │
│  │  Analyze sessions to find:                                            │ │
│  │  - Repeated command sequences                                         │ │
│  │  - Common file patterns touched                                       │ │
│  │  - Recurring explanations/justifications                              │ │
│  │  - Error patterns and resolutions                                     │ │
│  │  - "THE EXACT PROMPT" candidates                                      │ │
│  │  - Evidence refs (session_id, message range, snippet hash)            │ │
│  │  - Anti-patterns and counter-examples                                 │ │
│  └───────────────────────────────────────────────────────────────────────┘ │
│                                      │                                      │
│                                      ▼                                      │
│  Step 4: Pattern Clustering                                                 │
│  ┌───────────────────────────────────────────────────────────────────────┐ │
│  │  Group similar patterns:                                              │ │
│  │  - Semantic similarity (embeddings)                                   │ │
│  │  - Structural similarity (AST for code)                               │ │
│  │  - Temporal proximity (same session/day)                              │ │
│  └───────────────────────────────────────────────────────────────────────┘ │
│                                      │                                      │
│                                      ▼                                      │
│  Step 5: Draft Generation                                                   │
│  ┌───────────────────────────────────────────────────────────────────────┐ │
│  │  Generate SKILL.md structure:                                         │ │
│  │  - Title and description from patterns                                │ │
│  │  - Core content from clustered insights                               │ │
│  │  - Examples from actual session excerpts                              │ │
│  │  - Scripts from extracted code patterns                               │ │
│  │  - SkillSpec (structured source-of-truth)                             │ │
│  └───────────────────────────────────────────────────────────────────────┘ │
│                                      │                                      │
│                                      ▼                                      │
│  Step 6: Iterative Refinement                                               │
│  ┌───────────────────────────────────────────────────────────────────────┐ │
│  │  Present draft, collect feedback, improve:                            │ │
│  │  - User marks good/bad sections                                       │ │
│  │  - Mine more sessions if gaps identified                              │ │
│  │  - Regenerate with feedback incorporated                              │ │
│  │  - Recompile SKILL.md from SkillSpec                                  │ │
│  │  - Repeat until steady state                                          │ │
│  └───────────────────────────────────────────────────────────────────────┘ │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 5.2 Pattern Types

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PatternType {
    /// A specific command or sequence of commands
    CommandPattern {
        commands: Vec<String>,
        frequency: usize,
        contexts: Vec<String>,
    },

    /// A reusable code snippet
    CodePattern {
        language: String,
        code: String,
        purpose: String,
        frequency: usize,
    },

    /// An explanation or rationale that appears frequently
    ExplanationPattern {
        text: String,
        variants: Vec<String>,
        frequency: usize,
    },

    /// A decision tree or workflow
    WorkflowPattern {
        steps: Vec<WorkflowStep>,
        decision_points: Vec<DecisionPoint>,
        frequency: usize,
    },

    /// A constraint or rule that's repeatedly emphasized
    ConstraintPattern {
        rule: String,
        severity: Severity,  // Critical, Important, Recommended
        rationale: String,
        frequency: usize,
    },

    /// An error and its resolution
    ErrorResolutionPattern {
        error_signature: String,
        resolution: String,
        prevention: Option<String>,
        frequency: usize,
    },

    /// A specific prompt that gets reused
    PromptPattern {
        prompt: String,
        context: String,
        effectiveness_score: f32,
        frequency: usize,
    },

    /// What NOT to do (counter-example)
    AntiPattern {
        bad_practice: String,
        risk: String,
        safer_alternative: String,
        frequency: usize,
    },
}

/// Extracted pattern with provenance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedPattern {
    /// Stable id for deduplication and cross-referencing
    pub id: String,

    /// The classified pattern type
    pub pattern_type: PatternType,

    /// Evidence references supporting this pattern
    pub evidence: Vec<EvidenceRef>,

    /// Confidence of the pattern extraction (0.0 - 1.0)
    pub confidence: f32,
}
```

**Pattern IR (Typed Intermediate Representation):**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PatternIR {
    CommandRecipe { commands: Vec<String>, context: String },
    DiagnosticDecisionTree { nodes: Vec<DecisionNode> },
    Invariant { statement: String, severity: Severity },
    Pitfall { warning: String, counterexample: Option<String> },
    PromptMacro { template: String, variables: Vec<String> },
    RefactorPlaybook { steps: Vec<String>, safeguards: Vec<String> },
    ChecklistItem { item: String, category: String },
}
```

### 5.3 CASS Client Implementation

```rust
/// Client for interacting with CASS (coding_agent_session_search)
pub struct CassClient {
    /// Path to cass binary
    cass_bin: PathBuf,

    /// CASS data directory
    data_dir: PathBuf,

    /// Session fingerprint cache
    fingerprint_cache: FingerprintCache,
}

impl CassClient {
    /// Search sessions with the given query
    pub async fn search(&self, query: &str, limit: usize) -> Result<Vec<SessionMatch>> {
        let output = Command::new(&self.cass_bin)
            .args(["search", query, "--robot", "--limit", &limit.to_string()])
            .output()
            .await?;

        let results: CassSearchResults = serde_json::from_slice(&output.stdout)?;
        Ok(results.matches)
    }

    /// Get full session content
    pub async fn get_session(&self, session_id: &str) -> Result<Session> {
        let output = Command::new(&self.cass_bin)
            .args(["show", session_id, "--robot"])
            .output()
            .await?;

        serde_json::from_slice(&output.stdout).map_err(Into::into)
    }

    /// Incremental scan: only return sessions not seen or changed
    pub async fn incremental_sessions(&self) -> Result<Vec<SessionMatch>> {
        let output = Command::new(&self.cass_bin)
            .args(["search", "*", "--robot", "--limit", "10000"])
            .output()
            .await?;

        let results: CassSearchResults = serde_json::from_slice(&output.stdout)?;
        let mut delta = Vec::new();

        for m in results.matches {
            if self.fingerprint_cache.is_new_or_changed(&m.session_id, &m.content_hash) {
                delta.push(m);
            }
        }

        Ok(delta)
    }

    /// Get capabilities and schema
    pub async fn capabilities(&self) -> Result<CassCapabilities> {
        let output = Command::new(&self.cass_bin)
            .args(["capabilities", "--robot"])
            .output()
            .await?;

        serde_json::from_slice(&output.stdout).map_err(Into::into)
    }
}

/// Cache of session fingerprints to avoid reprocessing
pub struct FingerprintCache {
    db: Connection,
}

impl FingerprintCache {
    pub fn is_new_or_changed(&self, session_id: &str, hash: &str) -> bool {
        // Compare against cached hash
        unimplemented!()
    }

    pub fn update(&self, session_id: &str, hash: &str) -> Result<()> {
        unimplemented!()
    }
}
```

### 5.4 Interactive Build Session Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                      INTERACTIVE BUILD SESSION                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  $ ms build --name "rust-async-patterns"                                    │
│                                                                             │
│  ┌─────────────────────────────────────────────────────────────────────┐   │
│  │ 🔨 Starting skill build session: rust-async-patterns                │   │
│  │                                                                      │   │
│  │ What topic should I mine from your coding sessions?                  │   │
│  │ > async error handling in tokio                                      │   │
│  │                                                                      │   │
│  │ Mining sessions... Found 23 relevant sessions (confidence: 0.82)     │   │
│  │                                                                      │   │
│  │ Extracted patterns:                                                  │   │
│  │   ✓ 4 command patterns                                               │   │
│  │   ✓ 7 code patterns                                                  │   │
│  │   ✓ 3 workflow patterns                                              │   │
│  │   ✓ 2 constraint patterns ("NEVER use unwrap in async")              │   │
│  │                                                                      │   │
│  │ Commands:                                                            │   │
│  │   /patterns  - View extracted patterns                               │   │
│  │   /mine      - Mine more sessions                                    │   │
│  │   /draft     - Generate skill draft                                  │   │
│  │                                                                      │   │
│  │ > /draft                                                             │   │
│  │                                                                      │   │
│  │ Generating draft... Done!                                            │   │
│  │                                                                      │   │
│  │ ═══════════════════════════════════════════════════════════════════ │   │
│  │ DRAFT v1 - rust-async-patterns                                       │   │
│  │ ═══════════════════════════════════════════════════════════════════ │   │
│  │                                                                      │   │
│  │ ---                                                                  │   │
│  │ name: rust-async-patterns                                            │   │
│  │ description: Async/await patterns for Tokio-based Rust applications  │   │
│  │ ---                                                                  │   │
│  │                                                                      │   │
│  │ # Rust Async Patterns                                                │   │
│  │                                                                      │   │
│  │ ## ⚠️ CRITICAL RULES                                                 │   │
│  │                                                                      │   │
│  │ 1. NEVER use `.unwrap()` in async code - use `?` or explicit match  │   │
│  │ 2. ALWAYS cancel child tasks when parent task is dropped            │   │
│  │ ...                                                                  │   │
│  │ ═══════════════════════════════════════════════════════════════════ │   │
│  │                                                                      │   │
│  │ Rate this draft: [1-5] or /refine with feedback                      │   │
│  │ > /refine Add more examples of error propagation                     │   │
│  │                                                                      │   │
│  │ Mining more examples... Refining draft...                            │   │
│  │                                                                      │   │
│  │ [Shows updated draft v2]                                             │   │
│  │                                                                      │   │
│  │ > /publish                                                           │   │
│  │                                                                      │   │
│  │ ✓ Skill saved to ~/.config/ms/skills/rust-async-patterns/            │   │
│  │ ✓ Indexed and searchable                                             │   │
│  │ ✓ Quality score: 0.87                                                │   │
│  └─────────────────────────────────────────────────────────────────────┘   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 5.5 The Guided Iterative Mode (Hours-Long Autonomous Skill Generation)

This is a **killer feature**: ms can run autonomously for hours, systematically mining your session history to produce a comprehensive skill library tailored to YOUR approach.

**The Problem It Solves:**
- Manual skill creation is tedious and incomplete
- You've solved thousands of problems but captured none of them
- Your personal patterns and preferences aren't documented anywhere
- Starting from scratch means rediscovering solutions you already found

**The Vision:**

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                   GUIDED ITERATIVE MODE FLOW                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ms build --guided --duration 4h                                            │
│                                                                             │
│  Hour 1: Discovery                                                          │
│  ├── Scan CASS index for topic clusters                                    │
│  ├── Identify high-value session groups (error fixes, refactors, etc.)    │
│  ├── Rank by frequency × recency × success signals                         │
│  └── Present top 10 skill opportunities to user                            │
│                                                                             │
│  Hour 2-3: Generation Loop                                                  │
│  ├── For each approved opportunity:                                        │
│  │   ├── Deep mine all related sessions                                    │
│  │   ├── Extract patterns (see 5.6 algorithm)                             │
│  │   ├── Queue low-confidence patterns (see 5.15)                          │
│  │   ├── Generate draft skill                                              │
│  │   ├── Self-critique against quality rubric                              │
│  │   ├── Refine until steady-state (typically 3-6 iterations)             │
│  │   └── Add to skill queue for review                                     │
│  └── Move to next opportunity                                               │
│                                                                             │
│  Hour 4: Consolidation                                                      │
│  ├── Detect overlaps between generated skills (see 5.7)                    │
│  ├── Merge or deduplicate as needed                                        │
│  ├── Generate skill relationship graph                                     │
│  ├── Present batch for final user review                                   │
│  └── Publish approved skills to registry                                   │
│                                                                             │
│  Output:                                                                    │
│  ├── 8-15 new personalized skills                                          │
│  ├── Skill relationship documentation                                       │
│  ├── Session coverage report (% of history now captured)                   │
│  └── Recommendations for next guided session                               │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Shared State Machine (Guided vs Autonomous):**
- Guided mode and autonomous mode share the same state machine.
- Autonomous = guided with zero user input; guided = autonomous with checkpoints.
- One recovery path reduces drift and improves reliability.

**Steady-State Detection:**

From your planning-workflow skill, we adopt the "iterate until steady state" pattern:

```rust
/// Detect when refinement has reached steady state
pub struct SteadyStateDetector {
    /// Minimum iterations before considering steady
    min_iterations: usize,  // Default: 3

    /// Semantic similarity threshold for "no meaningful change"
    similarity_threshold: f32,  // Default: 0.95

    /// Maximum token delta considered "stable"
    max_token_delta: usize,  // Default: 50

    /// Maximum quality score delta considered "stable"
    max_quality_delta: f32,  // Default: 0.01

    /// Minimum evidence coverage to allow steady-state (prevents premature completion)
    min_evidence_coverage: f32,  // Default: 0.7

    /// Maximum iterations without quality improvement before forced stop
    max_no_improvement_iters: usize,  // Default: 3

    /// Maximum wall-clock time per skill (prevents pathological loops)
    max_wall_clock_per_skill: Duration,  // Default: 15 minutes
}

impl SteadyStateDetector {
    pub fn is_steady(&self, history: &[SkillDraft], start_time: Instant) -> SteadyStateResult {
        // Hard stop: wall-clock timeout
        if start_time.elapsed() > self.max_wall_clock_per_skill {
            return SteadyStateResult::ForcedStop {
                reason: "Max wall-clock time exceeded".into(),
            };
        }

        if history.len() < self.min_iterations {
            return SteadyStateResult::NotYet;
        }

        let recent = &history[history.len() - 2..];
        let prev = &recent[0];
        let curr = &recent[1];

        // Check semantic similarity (on canonical "outline + rules only" for stability)
        let similarity = cosine_similarity(
            &self.canonical_embedding(prev),
            &self.canonical_embedding(curr),
        );
        if similarity < self.similarity_threshold {
            return SteadyStateResult::NotYet;
        }

        // Check structural stability
        let token_delta = (curr.token_count as i64 - prev.token_count as i64).abs();
        if token_delta as usize > self.max_token_delta {
            return SteadyStateResult::NotYet;
        }

        // Check quality score stability (prevents false-positive "steady" on tiny edits)
        let quality_delta = (curr.quality_score - prev.quality_score).abs();
        if quality_delta > self.max_quality_delta {
            return SteadyStateResult::NotYet;
        }

        // Check evidence coverage (can't be steady if evidence is missing)
        if curr.evidence_coverage < self.min_evidence_coverage {
            return SteadyStateResult::NeedsEvidence {
                current: curr.evidence_coverage,
                required: self.min_evidence_coverage,
            };
        }

        // Check for no-improvement stall
        let recent_quality: Vec<_> = history.iter()
            .rev()
            .take(self.max_no_improvement_iters + 1)
            .map(|d| d.quality_score)
            .collect();
        if recent_quality.len() > self.max_no_improvement_iters {
            let max_recent = recent_quality.iter().cloned().fold(f32::MIN, f32::max);
            let improvement = max_recent - recent_quality.last().unwrap_or(&0.0);
            if improvement < 0.001 {
                return SteadyStateResult::ForcedStop {
                    reason: format!("No quality improvement in {} iterations", self.max_no_improvement_iters),
                };
            }
        }

        // Check section stability
        let sections_unchanged = prev.section_hashes == curr.section_hashes;

        if similarity >= self.similarity_threshold &&
           token_delta as usize <= self.max_token_delta &&
           quality_delta <= self.max_quality_delta &&
           sections_unchanged {
            SteadyStateResult::Steady
        } else {
            SteadyStateResult::NotYet
        }
    }

    /// Compute embedding on canonical representation (outline + rules only)
    /// More stable than full-document embedding
    fn canonical_embedding(&self, draft: &SkillDraft) -> Vec<f32> {
        let canonical = format!(
            "{}\n{}",
            draft.outline_text(),
            draft.rules_only_text(),
        );
        self.embedder.embed(&canonical)
    }
}

pub enum SteadyStateResult {
    NotYet,
    Steady,
    NeedsEvidence { current: f32, required: f32 },
    ForcedStop { reason: String },
}
```

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

```rust
pub enum CheckpointTrigger {
    /// Every N skills generated
    SkillBatch(usize),

    /// When confidence drops below threshold
    LowConfidence(f32),

    /// When potential merge detected
    MergeOpportunity,

    /// Scheduled interval
    TimeInterval(Duration),

    /// Coverage milestone reached
    CoverageMilestone(f32),
}
```

**CLI Interface:**

```bash
# Start guided mode with 4-hour duration
ms build --guided --duration 4h

# Start with specific focus areas
ms build --guided --focus "rust,async,error-handling" --duration 2h

# Resume interrupted guided session
ms build --guided --resume session-abc123

# Fully autonomous (no checkpoints except critical)
ms build --guided --autonomous --duration 8h

# Dry run: show what would be generated without creating
ms build --guided --dry-run --duration 1h
```

### 5.6 Specific-to-General Transformation Algorithm

This is the core intellectual innovation: extracting universal patterns ("inner truths") from specific instances.
The same pipeline is applied to counter-examples to produce "Avoid / When NOT to use" rules.

**The Transformation Pipeline:**

```
┌─────────────────────────────────────────────────────────────────────────────┐
│              SPECIFIC-TO-GENERAL TRANSFORMATION                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  SPECIFIC INSTANCE                                                          │
│  "In hero.tsx, I added handleScroll() call on mount because the scroll     │
│   indicator wasn't showing correctly when the page loaded already scrolled" │
│                                                                             │
│                        ▼ Extract Structure ▼                                │
│                                                                             │
│  STRUCTURAL PATTERN                                                         │
│  - File type: React component (*.tsx)                                       │
│  - Pattern: State initialization on mount                                   │
│  - Trigger: useEffect with empty deps                                       │
│  - Problem class: Initial state not matching DOM reality                    │
│                                                                             │
│                        ▼ Generalize ▼                                       │
│                                                                             │
│  GENERAL PRINCIPLE                                                          │
│  "When React component state depends on DOM measurements (scroll, size,     │
│   visibility), explicitly sync state on mount—don't assume default matches" │
│                                                                             │
│                        ▼ Validate ▼                                         │
│                                                                             │
│  VALIDATION                                                                 │
│  - Search CASS for similar patterns (found 7 more instances)               │
│  - Cluster by context (all are "mount-time sync" issues)                   │
│  - Confirm generalization holds for 6/7 (86% confidence)                   │
│                                                                             │
│                        ▼ Crystallize ▼                                      │
│                                                                             │
│  SKILL CONTENT                                                              │
│  "## React Mount-Time State Sync                                            │
│                                                                             │
│   **Pattern:** When state depends on DOM, sync on mount.                    │
│                                                                             │
│   **Examples:**                                                             │
│   - Scroll position → call handler in useEffect                            │
│   - Window size → measure on mount, not just on resize                     │
│   - Intersection → check initial visibility state                           │
│                                                                             │
│   **THE EXACT FIX:**                                                        │
│   ```tsx                                                                    │
│   useEffect(() => {                                                         │
│     handleScroll(); // Sync immediately on mount                            │
│     window.addEventListener('scroll', handleScroll);                        │
│     return () => window.removeEventListener('scroll', handleScroll);        │
│   }, []);                                                                   │
│   ```                                                                       │
│  "                                                                          │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Optional LLM-Assisted Refinement (Pluggable):**
- If configured, a local model critiques the candidate generalization for overreach,
  ambiguous scope, or missing counter-examples.
- Critique summaries are stored with the uncertainty item so humans can adjudicate.
- If no model is available, the pipeline remains heuristic-only.

**The Algorithm:**

```rust
/// Transform specific instances into general patterns
pub struct SpecificToGeneralTransformer {
    cass: CassClient,
    embedder: HashEmbedder,
    uncertainty_queue: UncertaintyQueue,
    refiner: Option<Box<dyn GeneralizationRefiner>>,
    min_instances: usize,       // Minimum instances to generalize (default: 3)
    confidence_threshold: f32,  // Minimum generalization confidence (default: 0.7)
}

pub trait GeneralizationRefiner {
    fn critique(&self, common: &CommonElements, cluster: &InstanceCluster) -> Result<RefinementCritique>;
}

pub struct RefinementCritique {
    pub summary: String,
    pub flags_overgeneralization: bool,
}

impl SpecificToGeneralTransformer {
    pub async fn transform(&self, instance: &SpecificInstance) -> Result<GeneralPattern> {
        // Step 1: Extract structural features
        let structure = self.extract_structure(instance)?;

        // Step 2: Find similar instances in CASS
        let similar = self.find_similar_instances(&structure).await?;

        if similar.len() < self.min_instances {
            return Err(anyhow!("Insufficient instances for generalization"));
        }

        // Step 3: Cluster by context
        let clusters = self.cluster_by_context(&similar)?;
        let primary_cluster = clusters.into_iter()
            .max_by_key(|c| c.instances.len())
            .ok_or_else(|| anyhow!("No valid clusters"))?;

        // Step 4: Extract common elements (the "inner truth")
        let common = self.extract_common_elements(&primary_cluster)?;

        // Step 5: Validate generalization
        let validation = self.validate_generalization(&common, &primary_cluster)?;

        if validation.confidence < self.confidence_threshold {
            self.queue_uncertainty(instance, &validation, &primary_cluster, None).ok();
            return Err(anyhow!("Generalization confidence too low: {}", validation.confidence));
        }

        // Step 6: Optional refinement/critique (LLM-assisted if configured)
        if let Some(refiner) = &self.refiner {
            let critique = refiner.critique(&common, &primary_cluster)?;
            if critique.flags_overgeneralization {
                self.queue_uncertainty(instance, &validation, &primary_cluster, Some(&critique)).ok();
                return Err(anyhow!("Generalization critique failed: {}", critique.summary));
            }
        }

        // Step 7: Generate general pattern
        Ok(GeneralPattern {
            principle: common.abstracted_description,
            examples: primary_cluster.instances.iter()
                .take(3)
                .map(|i| i.to_example())
                .collect(),
            applicability: common.context_conditions,
            confidence: validation.confidence,
            source_instances: similar.len(),
        })
    }

    fn extract_structure(&self, instance: &SpecificInstance) -> Result<StructuralPattern> {
        let file_type = self.detect_file_type(&instance.context)?;
        let code_pattern = self.extract_code_pattern(&instance.content)?;
        let problem_class = self.classify_problem(&instance.content)?;
        let solution_approach = self.extract_solution(&instance.content)?;

        Ok(StructuralPattern {
            file_type,
            code_pattern,
            problem_class,
            solution_approach,
        })
    }

    async fn find_similar_instances(&self, pattern: &StructuralPattern) -> Result<Vec<Instance>> {
        let query = format!(
            "{} {} {} {}",
            pattern.file_type,
            pattern.code_pattern.signature(),
            pattern.problem_class,
            pattern.solution_approach.keywords().join(" ")
        );

        let matches = self.cass.search(&query, 100).await?;

        let similar: Vec<_> = matches.into_iter()
            .filter(|m| self.is_structurally_similar(m, pattern))
            .collect();

        Ok(similar)
    }

    fn extract_common_elements(&self, cluster: &InstanceCluster) -> Result<CommonElements> {
        let mut always_present = HashSet::new();
        let mut sometimes_present = HashMap::new();

        for (i, instance) in cluster.instances.iter().enumerate() {
            let elements = self.extract_elements(instance)?;

            if i == 0 {
                always_present = elements.into_iter().collect();
            } else {
                always_present = always_present.intersection(&elements.into_iter().collect())
                    .cloned()
                    .collect();
            }

            for elem in elements {
                *sometimes_present.entry(elem).or_insert(0) += 1;
            }
        }

        let inner_truth = always_present.iter()
            .map(|e| self.abstract_element(e))
            .collect::<Result<Vec<_>>>()?;

        Ok(CommonElements {
            abstracted_description: self.synthesize_description(&inner_truth)?,
            context_conditions: self.infer_conditions(&sometimes_present, cluster.instances.len())?,
        })
    }

    fn queue_uncertainty(
        &self,
        instance: &SpecificInstance,
        validation: &GeneralizationValidation,
        cluster: &InstanceCluster,
        critique: Option<&RefinementCritique>,
    ) -> Result<()> {
        let suggested_queries = self.suggest_queries(instance, cluster)?;
        let mut reason = format!("Low confidence: {:.2}", validation.confidence);
        if let Some(c) = critique {
            reason = format!("{reason} | critique: {}", c.summary);
        }
        let item = UncertaintyItem {
            id: uuid::Uuid::new_v4().to_string(),
            pattern_candidate: instance.to_pattern_candidate(),
            reason,
            confidence: validation.confidence,
            suggested_queries,
            status: UncertaintyStatus::Pending,
            created_at: Utc::now(),
        };

        self.uncertainty_queue.enqueue(item)
    }
}
```

**Generalization Confidence Scoring:**

```rust
pub struct GeneralizationValidation {
    pub coverage: f32,           // How many instances fit the generalization
    pub predictive_power: f32,   // How well it predicts outcomes (given applicability)
    pub coherence: f32,          // Semantic coherence
    pub specificity: f32,        // Inverse of overbreadth (prevents platitudes)
    pub confidence: f32,         // Combined score
    pub counterexamples: Vec<CounterExample>,  // Instances where pattern fails
}

/// A counterexample captures why a pattern didn't apply or failed
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CounterExample {
    pub instance_id: String,
    pub failure_reason: CounterExampleReason,
    pub missing_precondition: Option<String>,
    pub suggests_refinement: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CounterExampleReason {
    PatternNotApplicable,  // Preconditions not met
    OutcomeMismatch,       // Applied but wrong outcome
    DifferentContext,      // Similar surface but different underlying situation
}

impl GeneralizationValidation {
    pub fn compute(pattern: &GeneralPattern, instances: &[Instance]) -> Self {
        let applies: Vec<_> = instances.iter()
            .filter(|i| pattern.applies_to(i))
            .collect();
        let applies_count = applies.len();

        let coverage = applies_count as f32 / instances.len() as f32;

        // IMPORTANT: predictive_power uses applies_count as denominator, not instances.len()
        // This prevents predictive power from collapsing as coverage drops
        let correct_count = applies.iter()
            .filter(|i| i.outcome == pattern.predicted_outcome())
            .count();
        let predictive_power = if applies_count > 0 {
            correct_count as f32 / applies_count as f32
        } else {
            0.0
        };

        let coherence = pattern.semantic_coherence_score();

        // Specificity penalizes overly broad patterns (platitudes that "apply to everything")
        // High coverage + low coherence = probably a platitude
        let specificity = if coverage > 0.95 && coherence < 0.5 {
            0.3  // Penalty for overbreadth
        } else {
            1.0 - (coverage * 0.2)  // Slight preference for more specific patterns
        };

        let confidence = 0.35 * coverage + 0.35 * predictive_power + 0.20 * coherence + 0.10 * specificity;

        // Collect counterexamples for "Avoid / When NOT to use" section
        let counterexamples = instances.iter()
            .filter(|i| !pattern.applies_to(i) || i.outcome != pattern.predicted_outcome())
            .map(|i| CounterExample {
                instance_id: i.id.clone(),
                failure_reason: if !pattern.applies_to(i) {
                    CounterExampleReason::PatternNotApplicable
                } else {
                    CounterExampleReason::OutcomeMismatch
                },
                missing_precondition: pattern.missing_precondition(i),
                suggests_refinement: pattern.suggest_scope_refinement(i),
            })
            .collect();

        Self { coverage, predictive_power, coherence, specificity, confidence, counterexamples }
    }
}
```

### 5.7 Skill Deduplication and Personalization

**No Redundancy Across Skills:**

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                      SKILL DEDUPLICATION SYSTEM                              │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  When generating a new skill, ms checks:                                    │
│                                                                             │
│  1. SEMANTIC OVERLAP                                                        │
│     ├── Embed new skill and all existing skills                            │
│     ├── Compute pairwise cosine similarity                                 │
│     ├── Flag pairs with similarity > 0.75                                  │
│     └── If overlap detected: MERGE, SUBSET, or DIFFERENTIATE               │
│                                                                             │
│  2. STRUCTURAL OVERLAP                                                      │
│     ├── Extract section headings from both skills                          │
│     ├── Compare section content hashes                                     │
│     ├── Identify duplicate sections                                        │
│     └── If sections match: REFERENCE instead of DUPLICATE                  │
│                                                                             │
│  3. COMMAND OVERLAP                                                         │
│     ├── Extract all command patterns from both skills                      │
│     ├── Compare command signatures                                         │
│     ├── Identify redundant commands                                        │
│     └── If commands overlap: CONSOLIDATE in one skill, reference from other│
│                                                                             │
│  Resolution Strategies:                                                     │
│                                                                             │
│  MERGE: Skills are >85% similar → combine into one                          │
│  ┌─────────────┐   ┌─────────────┐         ┌─────────────────────┐         │
│  │  Skill A    │ + │  Skill B    │   →     │  Merged Skill       │         │
│  │  (React     │   │  (React     │         │  (React State       │         │
│  │   State)    │   │   Hooks)    │         │   & Hooks)          │         │
│  └─────────────┘   └─────────────┘         └─────────────────────┘         │
│                                                                             │
│  SUBSET: One skill is proper subset → keep parent, deprecate child         │
│  ┌─────────────────────┐   ┌─────────────┐                                 │
│  │  Parent Skill       │ ⊃ │ Child Skill │  →  Deprecate child,            │
│  │  (Error Handling)   │   │ (Try-Catch) │      reference parent           │
│  └─────────────────────┘   └─────────────┘                                 │
│                                                                             │
│  DIFFERENTIATE: Overlap but different purposes → clarify scope             │
│  ┌─────────────┐   ┌─────────────┐         ┌─────────────┐ ┌─────────────┐ │
│  │  Skill A    │ ∩ │  Skill B    │   →     │  Skill A    │ │  Skill B    │ │
│  │  (API       │   │  (REST      │         │  (API Auth) │ │  (REST      │ │
│  │   Design)   │   │   Patterns) │         │  only       │ │   except    │ │
│  └─────────────┘   └─────────────┘         └─────────────┘ │   auth)     │ │
│                                                             └─────────────┘ │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Implementation:**

```rust
pub struct SkillDeduplicator {
    embedder: HashEmbedder,
    registry: SkillRegistry,
    semantic_threshold: f32,    // Default: 0.75
    uniqueness_threshold: f32,  // Default: 0.30
}

impl SkillDeduplicator {
    pub async fn check_overlap(&self, new_skill: &Skill) -> Result<OverlapReport> {
        let new_embedding = self.embedder.embed(&new_skill.body);
        let mut overlaps = Vec::new();

        for existing in self.registry.all_skills()? {
            let existing_embedding = self.registry.get_embedding(&existing.id)?;
            let similarity = cosine_similarity(&new_embedding, &existing_embedding);

            if similarity > self.semantic_threshold {
                let structural = self.analyze_structural_overlap(new_skill, &existing)?;
                overlaps.push(OverlapCandidate {
                    existing_skill: existing,
                    semantic_similarity: similarity,
                    structural_overlap: structural,
                    recommended_action: self.recommend_action(similarity, &structural),
                });
            }
        }

        Ok(OverlapReport { new_skill_id: new_skill.id.clone(), overlaps })
    }

    fn recommend_action(&self, similarity: f32, structural: &StructuralOverlap) -> OverlapAction {
        if similarity > 0.90 && structural.section_overlap > 0.80 {
            return OverlapAction::Merge;
        }
        if structural.is_subset {
            return OverlapAction::DeprecateSubset;
        }
        if similarity > 0.75 && structural.unique_content_ratio < self.uniqueness_threshold {
            return OverlapAction::Merge;
        }
        if similarity > 0.60 {
            return OverlapAction::Differentiate {
                suggested_scopes: structural.non_overlapping_topics.clone(),
            };
        }
        OverlapAction::NoAction
    }
}
```

**Personalization ("Tailored to YOUR Approach"):**

```rust
/// Track user patterns to personalize generated skills
pub struct PersonalizationEngine {
    style_profile: StyleProfile,
    tool_preferences: HashMap<String, String>,
    naming_conventions: NamingConventions,
    prompt_patterns: Vec<PromptPattern>,
}

#[derive(Debug, Clone)]
pub struct StyleProfile {
    pub indentation: IndentationStyle,
    pub comment_style: CommentStyle,
    pub error_handling: ErrorHandlingStyle,
    pub test_style: TestStyle,
    pub verbosity: Verbosity,
}

impl PersonalizationEngine {
    /// Build profile from CASS session analysis
    pub async fn build_from_sessions(&mut self, cass: &CassClient) -> Result<()> {
        let sessions = cass.search("*", 1000).await?;
        for session in sessions {
            self.analyze_session(&session)?;
        }
        self.consolidate_preferences()?;
        Ok(())
    }

    /// Apply personalization to generated skill
    pub fn personalize(&self, skill: &mut Skill) {
        // Adjust examples to match user's style
        for example in &mut skill.examples {
            example.code = self.adjust_style(&example.code);
        }

        // Use user's preferred tools in commands
        for command in &mut skill.commands {
            command.tool = self.tool_preferences
                .get(&command.tool)
                .cloned()
                .unwrap_or_else(|| command.tool.clone());
        }

        // Match verbosity preference
        if self.style_profile.verbosity == Verbosity::Concise {
            skill.body = self.condense(&skill.body);
        }

        // Include user's prompt patterns
        skill.prompts.extend(
            self.prompt_patterns.iter()
                .filter(|p| p.relevant_to(&skill.tags))
                .map(|p| p.to_skill_prompt())
        );
    }
}
```

### 5.8 Tech Stack Detection and Specialization

Different tech stacks require different skills. ms auto-detects your project's stack:

```rust
#[derive(Debug, Clone, PartialEq)]
pub enum TechStack {
    NextJsReact,
    React,
    Vue,
    Angular,
    RustCli,
    RustWeb,
    GoCli,
    GoWeb,
    Python,
    TypeScript,
    Node,
    Other(String),
}

pub struct TechStackDetector {
    indicators: HashMap<String, Vec<StackIndicator>>,
}

impl TechStackDetector {
    pub fn detect(&self, project_path: &Path) -> Result<DetectedStack> {
        let mut scores: HashMap<TechStack, f32> = HashMap::new();

        // Check package.json
        if let Ok(pkg) = self.read_package_json(project_path) {
            if pkg.dependencies.contains_key("next") {
                *scores.entry(TechStack::NextJsReact).or_default() += 5.0;
            }
            if pkg.dependencies.contains_key("react") {
                *scores.entry(TechStack::React).or_default() += 3.0;
            }
        }

        // Check Cargo.toml
        if let Ok(cargo) = self.read_cargo_toml(project_path) {
            *scores.entry(TechStack::RustCli).or_default() += 3.0;
            if cargo.dependencies.contains_key("tokio") {
                *scores.entry(TechStack::RustCli).or_default() += 2.0;
            }
            if cargo.dependencies.contains_key("actix-web") {
                *scores.entry(TechStack::RustWeb).or_default() += 5.0;
            }
        }

        // Check go.mod
        if self.file_exists(project_path, "go.mod") {
            *scores.entry(TechStack::GoCli).or_default() += 3.0;
        }

        let (primary, score) = scores.iter()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(k, v)| (k.clone(), *v))
            .unwrap_or((TechStack::Other("unknown".into()), 0.0));

        Ok(DetectedStack {
            primary,
            secondary: scores.iter()
                .filter(|(k, v)| *k != &primary && **v > 2.0)
                .map(|(k, _)| k.clone())
                .collect(),
            confidence: score / 10.0,
        })
    }

    pub fn suggest_for_stack(&self, stack: &DetectedStack) -> Vec<String> {
        match &stack.primary {
            TechStack::NextJsReact => vec![
                "nextjs-ui-polish".into(),
                "react-hooks-patterns".into(),
                "tailwind-responsive".into(),
                "accessibility-checklist".into(),
            ],
            TechStack::RustCli => vec![
                "rust-cli-patterns".into(),
                "rust-error-handling".into(),
                "rust-async-patterns".into(),
            ],
            TechStack::GoCli => vec![
                "go-cli-patterns".into(),
                "go-error-handling".into(),
            ],
            _ => vec![],
        }
    }
}
```

**Toolchain Detection and Drift:**

```rust
#[derive(Debug, Clone)]
pub struct ProjectToolchain {
    pub node: Option<String>,
    pub rust: Option<String>,
    pub go: Option<String>,
    pub nextjs: Option<String>,
    pub react: Option<String>,
}

pub struct ToolchainDetector;

impl ToolchainDetector {
    pub fn detect(&self, project_path: &Path) -> Result<ProjectToolchain> {
        Ok(ProjectToolchain {
            node: read_node_version(project_path),
            rust: read_cargo_version(project_path),
            go: read_go_version(project_path),
            nextjs: read_package_version(project_path, "next"),
            react: read_package_version(project_path, "react"),
        })
    }
}

pub struct ToolchainMismatch {
    pub tool: String,
    pub skill_range: String,
    pub project_version: String,
}

pub fn detect_toolchain_mismatches(
    skill: &Skill,
    toolchain: &ProjectToolchain,
) -> Vec<ToolchainMismatch> {
    let mut mismatches = Vec::new();

    for constraint in &skill.metadata.toolchains {
        let project_version = match constraint.name.as_str() {
            "node" => toolchain.node.clone(),
            "rust" => toolchain.rust.clone(),
            "go" => toolchain.go.clone(),
            "nextjs" => toolchain.nextjs.clone(),
            "react" => toolchain.react.clone(),
            _ => None,
        };

        if let Some(version) = project_version {
            if !version_in_range(&version, &constraint.min_version, &constraint.max_version) {
                mismatches.push(ToolchainMismatch {
                    tool: constraint.name.clone(),
                    skill_range: format!(
                        "{}..{}",
                        constraint.min_version.clone().unwrap_or_else(|| "*".into()),
                        constraint.max_version.clone().unwrap_or_else(|| "*".into())
                    ),
                    project_version: version,
                });
            }
        }
    }

    mismatches
}
```

**Stack-Specific Mining:**

```bash
# Mine sessions for NextJS/React projects specifically
ms build --from-cass "UI fixes" --stack nextjs-react

# Generate Go CLI skills from Go project sessions
ms build --guided --stack go-cli --duration 2h

# Auto-detect stack and filter sessions
ms build --from-cass "error handling" --stack auto
```

### 5.9 The Meta Skill Concept

The **meta skill** is a special skill that guides AI agents in using `ms` itself. This creates a recursive self-improvement loop where agents use skills to build better skills.

#### The Core Insight

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    THE META SKILL PHILOSOPHY                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Traditional:                                                               │
│  Human writes skills → Agent uses skills → Done                            │
│                                                                             │
│  Meta Skill:                                                                │
│  Agent sessions → CASS indexes → MS analyzes → Skills generated →          │
│  Agent uses skills → Better sessions → CASS indexes → Improved skills →    │
│  ∞ (Continuous improvement flywheel)                                       │
│                                                                             │
│  The meta skill teaches agents to:                                          │
│  1. Recognize when their sessions contain extractable patterns              │
│  2. Use ms commands to mine their own history                              │
│  3. Evaluate and refine generated skills                                   │
│  4. Identify gaps in their skill coverage                                  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### The Meta Skill Content

```markdown
---
name: ms-meta-skill
description: Guide Claude Code to use meta_skill (ms) for autonomous skill
  generation from CASS session history. Teaches iterative refinement,
  specific-to-general extraction, and skill lifecycle management.
---

# Meta Skill: Building Skills from Sessions

## When to Use This Skill

Trigger phrases:
- "turn this session into a skill"
- "extract patterns from my history"
- "what skills should I build"
- "improve my skills collection"
- "find gaps in my skills"

## Core Workflow

### 1. Discovery Phase

```bash
# What topics have enough sessions for skill extraction?
ms coverage --min-sessions 5

# Find pattern clusters in session history
ms analyze --cluster --min-cluster-size 3

# What skills already exist?
ms list --format=coverage
```

### 2. Extraction Phase

```bash
# Guided interactive build (recommended)
ms build --guided --topic "UI/UX fixes"

# Single-shot extraction from recent sessions
ms build --from-cass "error handling" --since "7 days" --output draft.md

# Hours-long autonomous generation
ms build --guided --duration 4h --checkpoint-interval 30m
```

### 3. Refinement Phase

After `ms build` generates a draft:

1. **Review** the draft skill critically
2. **Test** by using the skill in a real session
3. **Iterate** with `ms refine <skill-name>` based on usage
4. **Compile** from `skill.spec.json` to ensure deterministic output
5. **Validate** with `ms validate <skill-name>` for best practices

### 4. Integration Phase

```bash
# Add to your skill registry
ms add ./draft-skill/

# Update skill index
ms index --refresh

# Verify skill works
ms suggest "scenario that should trigger this skill"
```

## ⚠️ CRITICAL RULES

1. **Never skip the refinement phase** — First drafts are never optimal
2. **Test skills in real sessions** — The only true validation
3. **Keep skills focused** — One skill per concern
4. **Don't duplicate existing skills** — Run `ms overlap <draft>` first

## The Specific-to-General Pattern

When extracting patterns:

```
Specific Session Example           General Pattern
─────────────────────────────────────────────────────────
"Fixed aria-hidden on SVG" ────► "Decorative elements need aria-hidden"
"Added motion-reduce class" ────► "All animations need reduced-motion support"
"Changed transition-all" ────► "Use specific transition properties"
```

The key: **find the universal truth that made the specific fix necessary**.

## Coverage Gap Analysis

```bash
# What topics have sessions but no skills?
ms coverage --show-gaps

# What skill categories are underrepresented?
ms stats --by-category

# Suggest next skill to build based on session frequency
ms next --suggest-build
```

## Example: From Sessions to Skill

```bash
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
```
```

#### Meta Skill Generation Algorithm

```rust
/// The meta skill is self-referential - it teaches how to build skills
/// including how to build better versions of itself
pub struct MetaSkillGenerator {
    cass: CassClient,
    ms_registry: SkillRegistry,

    /// Meta skills evolve based on:
    /// 1. How often agents successfully use them
    /// 2. Quality of skills they help generate
    /// 3. Coverage of skill building scenarios
    meta_skill_version: SemanticVersion,
}

impl MetaSkillGenerator {
    /// Analyze how the meta skill is being used
    pub async fn analyze_meta_usage(&self) -> MetaSkillAnalysis {
        // Find sessions where ms commands were used
        let ms_sessions = self.cass.search("ms build OR ms refine OR ms guided").await;

        // Analyze what worked well
        let successful_builds = ms_sessions.iter()
            .filter(|s| s.contains_success_signals())
            .collect::<Vec<_>>();

        // Identify pain points
        let failed_builds = ms_sessions.iter()
            .filter(|s| s.contains_error_signals())
            .collect::<Vec<_>>();

        MetaSkillAnalysis {
            total_uses: ms_sessions.len(),
            success_rate: successful_builds.len() as f32 / ms_sessions.len() as f32,
            common_errors: extract_error_patterns(&failed_builds),
            improvement_opportunities: identify_gaps(&successful_builds, &failed_builds),
        }
    }

    /// Self-improve the meta skill based on usage analysis
    pub async fn self_improve(&mut self) -> Result<MetaSkillDraft> {
        let analysis = self.analyze_meta_usage().await;

        // Use the specific-to-general transformer on meta skill usage
        let transformer = SpecificToGeneralTransformer::new(self.cass.clone());

        // Extract patterns from successful meta skill uses
        let improvements = transformer
            .extract_principles(&analysis.successful_patterns())
            .await?;

        // Generate improved meta skill
        let improved_meta_skill = self.integrate_improvements(improvements)?;

        Ok(MetaSkillDraft {
            content: improved_meta_skill,
            improvements_made: analysis.improvement_opportunities,
            confidence: analysis.success_rate,
        })
    }
}

/// Track meta skill effectiveness over time
pub struct MetaSkillMetrics {
    /// Skills successfully generated using the meta skill
    pub skills_generated: usize,

    /// Average quality score of generated skills
    pub avg_quality_score: f32,

    /// How often users complete the guided flow
    pub guided_completion_rate: f32,

    /// Time from "ms build" to "ms add" (skill completion)
    pub avg_time_to_skill: Duration,
}
```

#### The Self-Improvement Loop

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    META SKILL SELF-IMPROVEMENT                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌──────────┐        ┌──────────┐        ┌──────────┐                      │
│  │ Agent    │        │  CASS    │        │   MS     │                      │
│  │ uses ms  │───────►│ indexes  │───────►│ analyzes │                      │
│  └──────────┘        │ session  │        │ patterns │                      │
│       ▲              └──────────┘        └────┬─────┘                      │
│       │                                       │                             │
│       │              ┌──────────┐             │                             │
│       │              │ Improved │             │                             │
│       └──────────────│ meta     │◄────────────┘                            │
│                      │ skill    │                                           │
│                      └──────────┘                                           │
│                                                                             │
│  Each cycle:                                                                │
│  1. Agents use meta skill to build skills                                  │
│  2. Those sessions get indexed by CASS                                     │
│  3. MS analyzes how meta skill was used                                    │
│  4. Meta skill improves based on usage patterns                            │
│  5. Better meta skill leads to better skill generation                     │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**CLI Commands for Meta Skill:**

```bash
# Check meta skill coverage
ms meta coverage

# Analyze meta skill effectiveness
ms meta analyze --days 30

# Suggest meta skill improvements
ms meta improve --dry-run

# Apply meta skill self-improvement
ms meta improve --apply

# View meta skill metrics
ms meta metrics
```

### 5.10 Long-Running Autonomous Generation with Checkpointing

The user's vision emphasizes hours-long autonomous skill generation sessions. This requires robust checkpointing, recovery, and progress tracking.

#### The Long-Running Session Problem

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    CHALLENGES IN LONG-RUNNING GENERATION                     │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Problem: Agent sessions can run for hours but:                            │
│  - LLM context windows have limits (~200K tokens)                          │
│  - Network interruptions happen                                            │
│  - Users may need to pause/resume                                          │
│  - Progress should be visible and auditable                                │
│                                                                             │
│  Solution: Checkpoint-based autonomous generation                           │
│  - Persist state every N iterations/minutes                                │
│  - Enable resume from any checkpoint                                       │
│  - Decouple discovery from generation from refinement                      │
│  - Support parallel skill generation                                       │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Checkpoint Architecture

```rust
/// Manages checkpoints for long-running skill generation
pub struct CheckpointManager {
    checkpoint_dir: PathBuf,
    checkpoint_interval: Duration,
    max_checkpoints: usize,
}

/// A checkpoint captures complete generation state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationCheckpoint {
    /// Unique checkpoint ID
    pub id: String,

    /// Build ID (stable across resumes)
    pub build_id: String,

    /// Checkpoint sequence number within build
    pub sequence: usize,

    /// When checkpoint was created
    pub created_at: DateTime<Utc>,

    /// Phase of generation
    pub phase: GenerationPhase,

    /// Skills being actively generated
    pub active_skills: Vec<SkillInProgress>,

    /// Completed skills ready for finalization
    pub completed_skills: Vec<CompletedSkillDraft>,

    /// Pattern pool (discovered but not yet used)
    pub pattern_pool: PatternPool,

    /// CASS query state (for resuming searches)
    pub cass_state: CassQueryState,

    /// Metrics at checkpoint time
    pub metrics: GenerationMetrics,

    /// Human feedback received so far
    pub feedback_history: Vec<FeedbackEvent>,

    // --- Idempotency fields for reproducibility ---

    /// Hashes of all input sessions processed so far (for dedup on resume)
    pub processed_session_hashes: Vec<String>,

    /// Config snapshot (effective config after overrides)
    pub config_snapshot: AutonomousConfig,

    /// Algorithm version (so resumes don't change semantics silently)
    pub algorithm_version: String,

    /// Random seed if any stochastic steps exist
    pub random_seed: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GenerationPhase {
    /// Discovering patterns from CASS
    Discovery {
        queries_completed: usize,
        queries_remaining: Vec<String>,
        patterns_found: usize,
    },

    /// Clustering and analyzing patterns
    Analysis {
        clusters_formed: usize,
        current_cluster: Option<String>,
    },

    /// Generating skill drafts
    Generation {
        skills_started: usize,
        skills_completed: usize,
        current_skill: Option<String>,
    },

    /// Iterative refinement
    Refinement {
        iteration: usize,
        last_delta: f32,
        steady_state_approach: bool,
    },

    /// Final validation and quality checks
    Validation,

    /// Complete (terminal state)
    Complete {
        total_skills: usize,
        total_duration: Duration,
    },
}

/// A skill currently being generated
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInProgress {
    pub name: String,
    pub tech_stack: TechStackContext,
    pub patterns_used: Vec<PatternId>,
    pub current_draft: String,
    pub iteration: usize,
    pub quality_score: f32,
    pub feedback: Vec<String>,
}

impl CheckpointManager {
    /// Save checkpoint to disk
    pub fn save(&self, checkpoint: &GenerationCheckpoint) -> Result<PathBuf> {
        let filename = format!(
            "checkpoint-{}-{:04}.json",
            checkpoint.session_id,
            checkpoint.sequence
        );
        let path = self.checkpoint_dir.join(&filename);

        // Atomic write (write to temp, then rename)
        let temp_path = path.with_extension("tmp");
        let json = serde_json::to_string_pretty(checkpoint)?;
        std::fs::write(&temp_path, &json)?;
        std::fs::rename(&temp_path, &path)?;

        // Prune old checkpoints
        self.prune_old_checkpoints()?;

        Ok(path)
    }

    /// Load most recent checkpoint for a session
    pub fn load_latest(&self, session_id: &str) -> Result<Option<GenerationCheckpoint>> {
        let pattern = format!("checkpoint-{}-*.json", session_id);
        let matches: Vec<_> = glob::glob(&self.checkpoint_dir.join(&pattern).to_string_lossy())?
            .filter_map(Result::ok)
            .collect();

        if matches.is_empty() {
            return Ok(None);
        }

        // Find highest sequence number
        let latest = matches.into_iter()
            .max_by_key(|p| {
                p.file_stem()
                    .and_then(|s| s.to_str())
                    .and_then(|s| s.rsplit('-').next())
                    .and_then(|n| n.parse::<usize>().ok())
                    .unwrap_or(0)
            })
            .unwrap();

        let content = std::fs::read_to_string(&latest)?;
        let checkpoint: GenerationCheckpoint = serde_json::from_str(&content)?;

        Ok(Some(checkpoint))
    }

    /// Resume generation from checkpoint
    pub fn resume(&self, checkpoint: &GenerationCheckpoint) -> Result<ResumedSession> {
        eprintln!(
            "Resuming from checkpoint {} (phase: {:?}, {} patterns in pool)",
            checkpoint.id,
            checkpoint.phase,
            checkpoint.pattern_pool.len()
        );

        Ok(ResumedSession {
            checkpoint: checkpoint.clone(),
            start_time: Utc::now(),
            iterations_since_resume: 0,
        })
    }
}
```

#### Autonomous Generation Orchestrator

```rust
/// Orchestrates hours-long autonomous skill generation
pub struct AutonomousOrchestrator {
    cass: CassClient,
    transformer: SpecificToGeneralTransformer,
    checkpoint_mgr: CheckpointManager,
    deduplicator: SkillDeduplicator,
    quality_scorer: QualityScorer,

    /// Configuration for autonomous operation
    config: AutonomousConfig,
}

#[derive(Debug, Clone)]
pub struct AutonomousConfig {
    /// Maximum duration for autonomous run
    pub max_duration: Duration,

    /// Checkpoint save interval
    pub checkpoint_interval: Duration,

    /// Progress report interval (to stderr)
    pub progress_interval: Duration,

    /// Maximum iterations per skill before forced completion
    pub max_iterations_per_skill: usize,

    /// Minimum quality score to accept a skill
    pub min_quality_threshold: f32,

    /// Enable parallel skill generation
    pub parallel_skills: usize,

    /// Stop if no new patterns found for this duration
    pub stall_timeout: Duration,
}

impl Default for AutonomousConfig {
    fn default() -> Self {
        Self {
            max_duration: Duration::from_secs(4 * 3600),  // 4 hours
            checkpoint_interval: Duration::from_secs(30 * 60),  // 30 minutes
            progress_interval: Duration::from_secs(5 * 60),  // 5 minutes
            max_iterations_per_skill: 15,
            min_quality_threshold: 0.7,
            parallel_skills: 2,
            stall_timeout: Duration::from_secs(30 * 60),  // 30 minutes
        }
    }
}

impl AutonomousOrchestrator {
    /// Run autonomous skill generation
    ///
    /// Note on identifiers:
    /// - `build_id`: Stable across resumes; supplied via CLI on resume
    /// - `run_id`: New each invocation (for logging/metrics)
    /// - `checkpoint_id`: Unique per checkpoint within a build
    pub async fn run(
        &self,
        topics: &[String],
        resume_build_id: Option<&str>,
    ) -> Result<AutonomousResult> {
        let run_id = uuid::Uuid::new_v4().to_string();
        let start_time = Utc::now();
        let mut checkpoint_seq = 0;

        // Determine build_id: either resume existing or start new
        let (build_id, mut state) = if let Some(existing_id) = resume_build_id {
            // Resume from existing build
            let cp = self.checkpoint_mgr.load_latest(existing_id)?
                .ok_or_else(|| anyhow!("No checkpoint found for build {}", existing_id))?;
            eprintln!("Resuming build {} from checkpoint {}", existing_id, cp.id);
            checkpoint_seq = cp.sequence;
            (existing_id.to_string(), State::from_checkpoint(cp))
        } else {
            // Start new build
            let new_build_id = uuid::Uuid::new_v4().to_string();
            eprintln!("Starting new build {}", new_build_id);
            (new_build_id, State::new(topics.to_vec()))
        };

        // Main autonomous loop
        loop {
            // Check termination conditions
            let elapsed = Utc::now() - start_time;
            if elapsed > chrono::Duration::from_std(self.config.max_duration)? {
                eprintln!("Reached max duration ({:?}), finishing up", self.config.max_duration);
                break;
            }

            // Progress report
            if should_report_progress(&state, self.config.progress_interval) {
                self.report_progress(&state, elapsed);
            }

            // Checkpoint save
            if should_save_checkpoint(&state, self.config.checkpoint_interval) {
                checkpoint_seq += 1;
                let checkpoint = state.to_checkpoint(&build_id, checkpoint_seq);
                let path = self.checkpoint_mgr.save(&checkpoint)?;
                eprintln!("Checkpoint saved: {:?} (build {})", path, build_id);
            }

            // Execute next phase
            match &state.phase {
                GenerationPhase::Discovery { .. } => {
                    self.run_discovery_step(&mut state).await?;
                }
                GenerationPhase::Analysis { .. } => {
                    self.run_analysis_step(&mut state).await?;
                }
                GenerationPhase::Generation { .. } => {
                    self.run_generation_step(&mut state).await?;
                }
                GenerationPhase::Refinement { .. } => {
                    self.run_refinement_step(&mut state).await?;
                }
                GenerationPhase::Validation => {
                    self.run_validation_step(&mut state).await?;
                }
                GenerationPhase::Complete { .. } => {
                    break;
                }
            }

            // Check for stall
            if state.time_since_progress() > self.config.stall_timeout {
                eprintln!("Generation stalled, forcing completion");
                state.force_completion();
            }
        }

        // Final checkpoint
        let final_checkpoint = state.to_checkpoint(&build_id, checkpoint_seq + 1);
        self.checkpoint_mgr.save(&final_checkpoint)?;

        Ok(AutonomousResult {
            build_id,
            run_id,
            duration: Utc::now() - start_time,
            skills_generated: state.completed_skills.len(),
            skills: state.completed_skills,
            patterns_discovered: state.pattern_pool.total_discovered(),
            patterns_used: state.pattern_pool.total_used(),
            checkpoints_created: checkpoint_seq + 1,
        })
    }

    fn report_progress(&self, state: &State, elapsed: chrono::Duration) {
        eprintln!(
            "\n━━━ Progress Report ({}) ━━━",
            format_duration(elapsed)
        );
        eprintln!(
            "  Phase: {:?}",
            state.phase
        );
        eprintln!(
            "  Patterns: {} discovered, {} used",
            state.pattern_pool.total_discovered(),
            state.pattern_pool.total_used()
        );
        eprintln!(
            "  Skills: {} in progress, {} completed",
            state.active_skills.len(),
            state.completed_skills.len()
        );
        if let Some(skill) = state.active_skills.first() {
            eprintln!(
                "  Current: \"{}\" (iter {}, quality {:.2})",
                skill.name, skill.iteration, skill.quality_score
            );
        }
        eprintln!("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
    }
}
```

**CLI Commands:**

```bash
# Start hours-long autonomous generation
ms build --autonomous --topics "ui-ux,error-handling,testing" --duration 4h

# Resume from checkpoint
ms build --resume SESSION_ID

# Resume latest checkpoint
ms build --resume-latest

# List available checkpoints
ms build --list-checkpoints

# View checkpoint details
ms build --show-checkpoint checkpoint-abc-0042.json

# Export checkpoint to shareable format
ms build --export-checkpoint checkpoint-abc-0042.json --output session.tar.gz

# Dry run: show what would be generated without actually generating
ms build --autonomous --topics "rust" --dry-run

# Set custom checkpoint interval
ms build --autonomous --topics "go" --checkpoint-interval 15m

# Run with progress output (default writes to stderr)
ms build --autonomous --topics "nextjs" --progress-interval 2m
```

**Progress Output Example:**

```
$ ms build --autonomous --topics "nextjs-ui,react-hooks" --duration 2h

Starting autonomous skill generation
  Topics: nextjs-ui, react-hooks
  Max duration: 2 hours
  Checkpoint interval: 30 minutes

Phase 1: Discovery
  Searching CASS for relevant sessions...
  Found 47 sessions matching "nextjs-ui"
  Found 31 sessions matching "react-hooks"
  Extracting patterns...

━━━ Progress Report (00:05:00) ━━━
  Phase: Discovery
  Patterns: 156 discovered, 0 used
  Skills: 0 in progress, 0 completed
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Phase 2: Analysis
  Clustering 156 patterns...
  Formed 8 clusters (min size: 3)

Phase 3: Generation
  Starting skill "nextjs-ui-accessibility"...

━━━ Progress Report (00:10:00) ━━━
  Phase: Generation
  Patterns: 156 discovered, 23 used
  Skills: 1 in progress, 0 completed
  Current: "nextjs-ui-accessibility" (iter 3, quality 0.72)
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Checkpoint saved: ~/.local/share/ms/checkpoints/checkpoint-abc123-0001.json

...

━━━ Final Report ━━━
  Duration: 01:47:32
  Skills generated: 4
    - nextjs-ui-accessibility (quality: 0.89)
    - react-hooks-patterns (quality: 0.84)
    - nextjs-performance (quality: 0.81)
    - react-state-management (quality: 0.78)
  Patterns: 156 discovered, 89 used
  Checkpoints: 4 created

Run 'ms add ~/.local/share/ms/generated/abc123/' to add skills to registry
```

### 5.11 Session Marking for Skill Mining

Allow users to mark sessions during or after completion as good candidates for skill extraction. This creates explicit training data for skill generation.

#### The Session Marking Problem

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    WHY SESSION MARKING MATTERS                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Not all sessions are equal for skill extraction:                           │
│  - Some sessions are exploratory/experimental (noisy)                       │
│  - Some sessions solve unique one-off problems (not generalizable)         │
│  - Some sessions demonstrate repeatable excellence (skill-worthy!)         │
│                                                                             │
│  Session marking lets users say:                                            │
│  "This session exemplifies how I solve X problems"                          │
│  "This session has patterns worth extracting"                              │
│  "This session should NOT be used for skill generation"                    │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Marking Data Model

```rust
/// A mark applied to a session for skill mining purposes
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMark {
    /// The session being marked
    pub session_id: String,

    /// Path to session file
    pub session_path: PathBuf,

    /// Mark type
    pub mark_type: MarkType,

    /// Topics/tags for this session
    pub topics: Vec<String>,

    /// Tech stack detected or specified
    pub tech_stack: TechStackContext,

    /// Quality assessment (1-5 stars)
    pub quality_rating: Option<u8>,

    /// Why this session was marked
    pub reason: Option<String>,

    /// When the mark was created
    pub marked_at: DateTime<Utc>,

    /// Who created the mark (agent name or "user")
    pub marked_by: String,

    /// Specific highlights within the session
    pub highlights: Vec<SessionHighlight>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MarkType {
    /// This session is excellent for skill extraction
    Exemplary,

    /// This session has some useful patterns
    Useful,

    /// This session should be ignored for skill extraction
    Ignore,

    /// This session has issues that should be learned from (anti-patterns)
    AntiPattern,
}

/// A highlighted section within a session (first-class, topic-scoped)
///
/// Highlights are first-class objects that can be independently queried,
/// filtered, and aggregated. They are topic-scoped, meaning a highlight
/// belongs to specific topics rather than inheriting all topics from
/// the parent session. This enables:
/// - Precise topic→evidence mapping during generalization
/// - Multi-topic sessions where different sections apply to different topics
/// - Efficient queries like "all highlights for 'git-workflow' topic"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionHighlight {
    /// Unique identifier for this highlight
    pub id: String,

    /// Start turn/message index
    pub start: usize,

    /// End turn/message index
    pub end: usize,

    /// Topics this specific highlight applies to (NOT inherited from session)
    /// A highlight can have a subset of session topics, or different topics entirely
    pub topics: Vec<String>,

    /// Classification of what kind of evidence this highlight provides
    pub highlight_type: HighlightType,

    /// Why this section is highlighted
    pub reason: String,

    /// Confidence that this highlight is correctly scoped (0.0-1.0)
    pub confidence: f32,

    /// Extracted pattern (if any)
    pub pattern: Option<ExtractedPattern>,

    /// Related highlights (e.g., problem→solution pairs)
    pub related_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum HighlightType {
    /// Exemplary implementation of a pattern
    Exemplar,
    /// A problem that was solved (pair with Solution)
    Problem,
    /// A solution to a problem (pair with Problem)
    Solution,
    /// An anti-pattern or mistake to avoid
    AntiPattern,
    /// A clarifying discussion or explanation
    Clarification,
    /// A specific command sequence worth extracting
    CommandSequence,
}

/// Storage for session marks
pub struct SessionMarkStore {
    db: Connection,
}

impl SessionMarkStore {
    /// Mark a session
    pub fn mark(
        &self,
        session_id: &str,
        mark_type: MarkType,
        topics: &[String],
        opts: MarkOptions,
    ) -> Result<SessionMark> {
        // Detect tech stack if not provided
        let tech_stack = opts.tech_stack.unwrap_or_else(|| {
            TechStackDetector::new().detect_from_session(session_id)
                .unwrap_or_default()
        });

        let mark = SessionMark {
            session_id: session_id.to_string(),
            session_path: self.resolve_session_path(session_id)?,
            mark_type,
            topics: topics.to_vec(),
            tech_stack,
            quality_rating: opts.quality,
            reason: opts.reason,
            marked_at: Utc::now(),
            marked_by: opts.marked_by.unwrap_or_else(|| "user".into()),
            highlights: opts.highlights,
        };

        self.save_mark(&mark)?;
        Ok(mark)
    }

    /// Get all marked sessions for a topic
    pub fn get_for_topic(&self, topic: &str) -> Result<Vec<SessionMark>> {
        self.db.query(
            "SELECT * FROM session_marks WHERE topics LIKE ?",
            params![format!("%{}%", topic)],
        )
    }

    /// Get exemplary sessions for skill generation
    pub fn get_exemplary(&self) -> Result<Vec<SessionMark>> {
        self.db.query(
            "SELECT * FROM session_marks WHERE mark_type = 'exemplary' ORDER BY quality_rating DESC",
            [],
        )
    }

    /// Filter sessions by marks for CASS queries
    pub fn filter_for_cass_query(&self, query: &CassQuery) -> CassQuery {
        let exemplary_ids = self.get_exemplary()
            .unwrap_or_default()
            .iter()
            .map(|m| m.session_id.clone())
            .collect::<Vec<_>>();

        let ignore_ids = self.db.query::<SessionMark>(
            "SELECT * FROM session_marks WHERE mark_type = 'ignore'",
            [],
        )
            .unwrap_or_default()
            .iter()
            .map(|m| m.session_id.clone())
            .collect::<Vec<_>>();

        query.clone()
            .prefer_sessions(&exemplary_ids)
            .exclude_sessions(&ignore_ids)
    }
}
```

#### CLI Commands for Session Marking

```bash
# Mark current/recent session as exemplary
ms mark --exemplary --topics "ui-ux,accessibility"

# Mark a specific session
ms mark SESSION_ID --useful --topics "error-handling" --reason "Good retry patterns"

# Mark session to be ignored
ms mark SESSION_ID --ignore --reason "Exploratory session, not production patterns"

# Mark as anti-pattern (learn what NOT to do)
ms mark SESSION_ID --anti-pattern --topics "auth" --reason "Insecure token handling"

# Add quality rating
ms mark SESSION_ID --exemplary --quality 5 --topics "testing"

# Highlight specific section of a session
ms mark SESSION_ID --highlight 45-67 --reason "Excellent error recovery pattern"

# List all marked sessions
ms marks list

# List exemplary sessions for a topic
ms marks list --exemplary --topic "react"

# Show mark details
ms marks show SESSION_ID

# Remove a mark
ms marks remove SESSION_ID

# Import marks from another machine
ms marks import ~/exported-marks.json

# Export marks for sharing
ms marks export --output marks.json
```

#### Integration with Skill Building

```rust
/// Skill builder that prioritizes marked sessions
pub struct MarkedSessionBuilder {
    cass: CassClient,
    mark_store: SessionMarkStore,
    transformer: SpecificToGeneralTransformer,
}

impl MarkedSessionBuilder {
    /// Build skill prioritizing marked sessions
    pub async fn build_from_marked(
        &self,
        topic: &str,
        opts: BuildOptions,
    ) -> Result<SkillDraft> {
        // Get explicitly marked exemplary sessions first
        let exemplary = self.mark_store.get_for_topic(topic)?
            .into_iter()
            .filter(|m| matches!(m.mark_type, MarkType::Exemplary))
            .collect::<Vec<_>>();

        // Get highlighted sections from marked sessions
        let highlighted_patterns: Vec<_> = exemplary.iter()
            .flat_map(|m| &m.highlights)
            .filter_map(|h| h.pattern.clone())
            .collect();

        // Fall back to CASS search for unmarked sessions
        let cass_query = CassQuery::new(topic)
            .prefer_sessions(&exemplary.iter().map(|m| &m.session_id).collect::<Vec<_>>())
            .exclude_ignored(&self.mark_store);

        let cass_patterns = self.cass.search(&cass_query).await?;

        // Combine marked highlights (higher weight) with discovered patterns
        let mut all_patterns = highlighted_patterns;
        all_patterns.extend(cass_patterns);

        // Apply specific-to-general transformation
        self.transformer.transform(&all_patterns, opts).await
    }
}
```

**Example Workflow:**

```bash
# After a great session fixing accessibility issues
$ ms mark --exemplary --topics "nextjs,accessibility,aria" \
    --reason "Comprehensive aria-hidden fixes across 15 components" \
    --quality 5

# After a mediocre session with lots of backtracking
$ ms mark --ignore --reason "Too much trial and error, not exemplary"

# Later, when building skills
$ ms build --guided --topic "accessibility"
# → Prioritizes the exemplary session
# → Ignores the marked-ignore session

# View what's available for a topic
$ ms marks list --topic "accessibility"
Exemplary sessions (2):
  ★★★★★ abc123 - "Comprehensive aria-hidden fixes" (nextjs, accessibility)
  ★★★★☆ def456 - "Screen reader testing patterns" (accessibility, testing)

Ignored sessions (1):
  xyz789 - "Too much trial and error, not exemplary"
```

Anti-pattern markings are treated as counter-examples and flow into a dedicated
"Avoid / When NOT to use" section during draft generation.

### 5.12 Evidence and Provenance Graph

Evidence links are first-class: every rule in a generated skill should be traceable back
to concrete session evidence. ms builds a lightweight provenance graph that connects:

```
Rule → Pattern → Session → Messages
```

This makes skills auditable, merge-safe, and self-correcting.

**Provenance Compression (Pointer + Fetch):**
- Level 0: hash pointers + message ranges (cheap default)
- Level 1: minimal redacted excerpt for quick review
- Level 2: expandable context fetched from CASS on demand

**Provenance Graph Model:**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvenanceGraph {
    pub nodes: Vec<ProvNode>,
    pub edges: Vec<ProvEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvNode {
    pub id: String,
    pub node_type: ProvNodeType,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProvNodeType {
    Skill,
    Rule,
    Pattern,
    Session,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvEdge {
    pub from: String,
    pub to: String,
    pub weight: f32,     // evidence confidence
    pub reason: String,  // short description
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceTimeline {
    pub rule_id: String,
    pub items: Vec<TimelineItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineItem {
    pub session_id: String,
    pub occurred_at: DateTime<Utc>,
    pub excerpt_path: Option<PathBuf>,
    pub confidence: f32,
}
```

**CLI Examples:**

```bash
# Show evidence for a skill (rule-level summary)
ms evidence nextjs-accessibility

# Inspect a specific rule's evidence
ms evidence nextjs-accessibility --rule "rule-7"

# Export the provenance graph
ms evidence nextjs-accessibility --graph --format json
ms evidence nextjs-accessibility --timeline
ms evidence nextjs-accessibility --open
```

**Actionable Evidence Navigation:**

Provenance is only valuable if humans can quickly validate and refine rules.
ms provides direct jump-to-source workflows that call CASS to expand context.

```rust
pub struct EvidenceNavigator {
    cass_client: CassClient,
    evidence_cache: PathBuf,  // Git archive cache for redacted excerpts
}

impl EvidenceNavigator {
    /// Jump to source session and expand context around evidence
    pub async fn expand_evidence(
        &self,
        skill_id: &str,
        rule_id: &str,
        context_lines: usize,
    ) -> Result<ExpandedEvidence> {
        let skill = self.registry.get(skill_id)?;
        let evidence = skill.evidence.get(rule_id)
            .ok_or_else(|| anyhow!("No evidence for rule: {}", rule_id))?;

        let mut expanded = Vec::new();
        for ref_item in &evidence.refs {
            // Call CASS to expand context
            let cass_result = self.cass_client
                .expand(&ref_item.session_id, ref_item.message_range.0, context_lines)
                .await?;

            expanded.push(ExpandedEvidenceItem {
                session_id: ref_item.session_id.clone(),
                message_range: ref_item.message_range,
                context_before: cass_result.before,
                matched_content: cass_result.matched,
                context_after: cass_result.after,
                session_metadata: cass_result.metadata,
            });
        }

        Ok(ExpandedEvidence {
            skill_id: skill_id.to_string(),
            rule_id: rule_id.to_string(),
            items: expanded,
        })
    }

    /// Cache redacted excerpts in Git archive for offline access
    pub fn cache_evidence(&self, skill_id: &str, rule_id: &str) -> Result<PathBuf> {
        let expanded = self.expand_evidence(skill_id, rule_id, 5).await?;
        let cache_path = self.evidence_cache
            .join("excerpts")
            .join(skill_id)
            .join(format!("{}.md", rule_id));

        std::fs::create_dir_all(cache_path.parent().unwrap())?;

        let content = render_evidence_excerpt(&expanded);
        std::fs::write(&cache_path, content)?;

        Ok(cache_path)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpandedEvidence {
    pub skill_id: String,
    pub rule_id: String,
    pub items: Vec<ExpandedEvidenceItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpandedEvidenceItem {
    pub session_id: String,
    pub message_range: (u32, u32),
    pub context_before: Vec<String>,
    pub matched_content: String,
    pub context_after: Vec<String>,
    pub session_metadata: SessionMetadata,
}
```

**Jump-to-Source CLI:**

```bash
# Expand evidence with CASS context (5 lines before/after)
ms evidence nextjs-accessibility --rule rule-7 --expand -C 5

# Open evidence in editor (caches redacted excerpt first)
ms evidence nextjs-accessibility --rule rule-7 --open-editor

# Show CASS session info for evidence
ms evidence nextjs-accessibility --rule rule-7 --cass-info

# Validate evidence still exists in CASS (detect stale refs)
ms evidence nextjs-accessibility --validate

# Refresh evidence cache from CASS
ms evidence nextjs-accessibility --refresh-cache
```

### 5.13 Redaction and Privacy Guard

All CASS transcripts pass through a redaction pipeline before pattern extraction.
This prevents secrets, tokens, and PII from ever entering generated skills,
evidence excerpts, or provenance graphs.

**Reassembly Resistance:**
- Redaction assigns stable `secret_id` values so multiple partial excerpts cannot
  be combined to reconstruct a secret across rules/evidence.
- High-risk secret types are blocked from excerpt storage entirely.

**Redaction Report Model:**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionReport {
    pub session_id: String,
    pub findings: Vec<RedactionFinding>,
    pub redacted_tokens: usize,
    pub risk_level: RedactionRisk,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionFinding {
    pub kind: RedactionKind,
    pub matched_pattern: String,
    pub snippet_hash: String,
    pub location: RedactionLocation,
    pub secret_id: Option<String>,     // stable id for reassembly resistance
    pub secret_type: Option<SecretType>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RedactionKind {
    ApiKey,
    Secret,
    AccessToken,
    Password,
    PiiEmail,
    PiiPhone,
    PiiIp,
    HighEntropy,
    CustomPattern,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SecretType {
    ApiKey,
    AccessToken,
    Email,
    Phone,
    Hostname,
    Filepath,
    CustomerData,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedactionLocation {
    pub message_index: u32,
    pub byte_start: u32,
    pub byte_end: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RedactionRisk {
    Low,
    Medium,
    High,
}
```

**Redactor Interface:**

```rust
pub struct Redactor {
    pub rules: Vec<Regex>,
    pub allowlist: Vec<Regex>,
    pub min_entropy: f32,
}

impl Redactor {
    pub fn redact(&self, input: &str) -> (String, RedactionReport) {
        // 1) apply allowlist exemptions
        // 2) regex-based redactions
        // 3) entropy-based redactions
        // 4) emit report with findings + risk
        unimplemented!()
    }
}
```

**CLI Examples:**

```bash
# Validate redaction health
ms doctor --check=redaction

# Emit redaction report for a build
ms build --from-cass "auth tokens" --redaction-report
```

**Taint Tracking Through Mining Pipeline:**

Beyond binary redaction, ms tracks **taint labels** through the entire extraction →
clustering → synthesis pipeline. This ensures unsafe provenance never leaks into
high-leverage artifacts (prompts, rules, scripts).

```rust
/// Taint sources for session content
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TaintSource {
    /// Tool output (file reads, command results) - untrusted external data
    ToolOutput,
    /// User-provided text - may contain typos, bad advice, or injection
    UserText,
    /// Contains detected secrets (post-redaction risk)
    ContainsSecret,
    /// Contains potential prompt injection patterns
    ContainsInjection,
    /// Contains PII (even if redacted, provenance is tainted)
    ContainsPii,
    /// Assistant-generated content (relatively safer, still needs verification)
    AssistantGenerated,
}

/// Taint set attached to each extracted snippet
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TaintSet {
    pub sources: HashSet<TaintSource>,
    pub propagated_from: Vec<String>,  // IDs of parent snippets
}

impl TaintSet {
    pub fn is_safe_for_prompt(&self) -> bool {
        !self.sources.contains(&TaintSource::ToolOutput)
            && !self.sources.contains(&TaintSource::ContainsInjection)
            && !self.sources.contains(&TaintSource::ContainsSecret)
    }

    pub fn is_safe_for_evidence(&self) -> bool {
        !self.sources.contains(&TaintSource::ContainsSecret)
            && !self.sources.contains(&TaintSource::ContainsPii)
    }
}

/// Taint-aware snippet extraction
pub struct TaintedSnippet {
    pub content: String,
    pub taint: TaintSet,
    pub source_location: SourceLocation,
}

pub struct TaintTracker;

impl TaintTracker {
    /// Classify message role and content to assign initial taint
    pub fn classify_message(&self, msg: &Message) -> TaintSet {
        let mut taint = TaintSet::default();

        match msg.role.as_str() {
            "tool" => { taint.sources.insert(TaintSource::ToolOutput); }
            "user" => { taint.sources.insert(TaintSource::UserText); }
            "assistant" => { taint.sources.insert(TaintSource::AssistantGenerated); }
            _ => {}
        }

        // Check for injection patterns
        if self.detect_injection_patterns(&msg.content) {
            taint.sources.insert(TaintSource::ContainsInjection);
        }

        // Check for residual secrets (post-redaction audit)
        if self.detect_secret_patterns(&msg.content) {
            taint.sources.insert(TaintSource::ContainsSecret);
        }

        taint
    }

    /// Propagate taint through synthesis (union of input taints)
    pub fn propagate(&self, inputs: &[&TaintSet]) -> TaintSet {
        let mut result = TaintSet::default();
        for input in inputs {
            result.sources.extend(&input.sources);
        }
        result
    }
}
```

**Taint Policy Enforcement:**

```rust
pub struct TaintPolicy;

impl TaintPolicy {
    /// Validate that a skill block meets taint requirements
    pub fn validate_block(&self, block: &SkillBlock, taint: &TaintSet) -> Result<()> {
        match block.block_type {
            BlockType::Rule | BlockType::Prompt => {
                if !taint.is_safe_for_prompt() {
                    return Err(anyhow!(
                        "Block '{}' has unsafe taint for prompt: {:?}",
                        block.id, taint.sources
                    ));
                }
            }
            BlockType::Evidence => {
                if !taint.is_safe_for_evidence() {
                    return Err(anyhow!(
                        "Block '{}' has unsafe taint for evidence: {:?}",
                        block.id, taint.sources
                    ));
                }
            }
            _ => {} // Examples, references have looser requirements
        }
        Ok(())
    }
}
```

**CLI Integration:**

```bash
# Check for taint policy violations
ms doctor --check=security

# Show taint provenance for a skill
ms evidence rust-patterns --show-taint

# Build with strict taint enforcement
ms build --from-cass "auth" --strict-taint
```

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

```
## Avoid / When NOT to Use

- ❌ Store tokens in localStorage (risk: XSS exfiltration)
  ✅ Use httpOnly cookies and rotate on logout

- ❌ Re-run migrations on every startup (risk: production locks)
  ✅ Gate migrations behind explicit deploy step
```

### 5.15 Active-Learning Uncertainty Queue

When generalization confidence is too low, ms does not discard the pattern. Instead,
it queues the candidate for targeted evidence gathering. This turns "maybe" patterns
into high-quality rules with minimal extra effort.

**Precision Loop (Active Learning):**
- Generate 3–7 targeted CASS queries per uncertainty (positive, negative, boundary).
- Auto-run when idle or via `ms uncertainties --mine` and stop on confidence threshold.

**Uncertainty Queue Flow:**

```
Low confidence pattern → Queue → Suggested CASS queries → Review/resolve → Promote or discard
```

**Queue Interface:**

```rust
/// An uncertainty item with actionable resolution criteria
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UncertaintyItem {
    pub id: String,
    pub pattern_candidate: PatternCandidate,
    pub reason: String,
    pub confidence: f32,
    pub suggested_queries: Vec<String>,
    pub status: UncertaintyStatus,
    pub created_at: DateTime<Utc>,

    // --- Actionable resolution criteria ---

    /// What evidence would change the decision (makes resolution deterministic)
    /// e.g., "need 2 more positive instances OR 1 strong counterexample"
    pub decision_boundary: DecisionBoundary,

    /// What signals are missing that caused low confidence
    /// e.g., "no tests-passed instances", "no user-confirmed resolution"
    pub missing_signals: Vec<MissingSignal>,

    /// Proposed narrower scope if current scope is too broad
    pub candidate_scope_refinement: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionBoundary {
    /// Minimum additional positive instances needed to promote
    pub positive_instances_needed: usize,
    /// If true, one strong counterexample would demote to discard
    pub counterexample_would_discard: bool,
    /// Target confidence threshold to promote
    pub target_confidence: f32,
    /// Human-readable description of what would resolve this
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MissingSignal {
    NoTestsPassed,
    NoUserConfirmation,
    NoClearResolution,
    InsufficientInstances { have: usize, need: usize },
    LowSemanticCoherence,
    NoCounterExamplesValidated,
}

pub struct UncertaintyQueue {
    db: Connection,
}

impl UncertaintyQueue {
    pub fn enqueue(&self, item: UncertaintyItem) -> Result<()> {
        self.db.execute(
            "INSERT INTO uncertainty_queue VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                item.id,
                serde_json::to_string(&item.pattern_candidate)?,
                item.reason,
                item.confidence,
                serde_json::to_string(&item.suggested_queries)?,
                serde_json::to_string(&item.decision_boundary)?,
                serde_json::to_string(&item.missing_signals)?,
                item.candidate_scope_refinement,
                "pending",
                item.created_at.to_rfc3339(),
            ],
        )?;
        Ok(())
    }

    pub fn list_pending(&self, limit: usize) -> Result<Vec<UncertaintyItem>> {
        self.db.query("SELECT * FROM uncertainty_queue WHERE status = 'pending' LIMIT ?", [limit])
    }

    pub fn resolve(&self, id: &str, outcome: UncertaintyStatus) -> Result<()> {
        self.db.execute(
            "UPDATE uncertainty_queue SET status = ? WHERE id = ?",
            params![format!("{:?}", outcome).to_lowercase(), id],
        )?;
        Ok(())
    }

    /// Check if new evidence satisfies the decision boundary
    pub fn check_resolution(&self, item: &UncertaintyItem, new_evidence: &NewEvidence) -> ResolutionCheck {
        let boundary = &item.decision_boundary;

        if new_evidence.positive_instances >= boundary.positive_instances_needed {
            return ResolutionCheck::Promote;
        }
        if boundary.counterexample_would_discard && new_evidence.strong_counterexamples > 0 {
            return ResolutionCheck::Discard;
        }
        if new_evidence.updated_confidence >= boundary.target_confidence {
            return ResolutionCheck::Promote;
        }

        ResolutionCheck::StillUncertain {
            progress: format!(
                "{}/{} positive instances, confidence {:.2}/{:.2}",
                new_evidence.positive_instances,
                boundary.positive_instances_needed,
                new_evidence.updated_confidence,
                boundary.target_confidence,
            ),
        }
    }
}

pub enum ResolutionCheck {
    Promote,
    Discard,
    StillUncertain { progress: String },
}
```

**CLI Examples:**

```bash
# List pending uncertain patterns
ms uncertainties list

# Resolve one by mining more evidence
ms uncertainties resolve UNK-123 --mine "react mount state sync"

# Resolve in batch (guided)
ms build --resolve-uncertainties
```

### 5.16 Session Quality Scoring

Not all sessions are equally useful. ms scores sessions for signal quality and
filters out low-quality transcripts before pattern extraction.

**Session Quality Model:**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionQuality {
    pub session_id: String,
    pub score: f32,
    pub signals: Vec<String>,
}

impl SessionQuality {
    /// Compute a quality score from observed signals
    pub fn compute(session: &Session) -> Self {
        let mut score = 0.0;
        let mut signals = Vec::new();

        // Positive signals
        if session.has_tests_passed() {
            score += 0.25; signals.push("tests_passed".into());
        }
        if session.has_clear_resolution() {
            score += 0.25; signals.push("clear_resolution".into());
        }
        if session.has_code_changes() {
            score += 0.15; signals.push("code_changes".into());
        }
        if session.has_user_confirmation() {
            score += 0.15; signals.push("user_confirmed".into());
        }

        // Negative signals
        if session.has_backtracking() {
            score -= 0.10; signals.push("backtracking".into());
        }
        if session.is_abandoned() {
            score -= 0.20; signals.push("abandoned".into());
        }

        Self { session_id: session.id.clone(), score: score.clamp(0.0, 1.0), signals }
    }
}
```

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
- Use **ACIP** from `/data/projects/acip` as the canonical injection defense framework.
- Support three modes: direct inclusion, checker-model gate, or hybrid audit mode.
- Audit mode uses `ACIP_AUDIT_MODE=ENABLED` tags for operator visibility.
- Pin ACIP version in config and store provenance alongside injection reports.

**Forensic Quarantine Playback:**
- Store snippet hash, minimal safe excerpt, triggered rule, and replay command.
- Replay requires explicit user invocation to expand context from CASS.

**Injection Report Model:**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectionReport {
    pub session_id: String,
    pub findings: Vec<InjectionFinding>,
    pub severity: InjectionSeverity,
    pub acip_version: Option<String>,
    pub acip_mode: Option<String>,
    pub acip_audit_mode: Option<bool>,
    pub created_at: DateTime<Utc>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AcipConfig {
    pub enabled: bool,
    pub version: String,     // e.g., "1.3"
    pub mode: String,        // direct | checker | hybrid
    pub audit_mode: bool,
}

pub struct InjectionGate {
    pub acip: AcipConfig,
}

impl InjectionGate {
    pub fn evaluate(&self, session: &Session) -> Result<InjectionReport> {
        // Apply ACIP as primary defense; record version/mode in report
        unimplemented!()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InjectionFinding {
    pub pattern: String,
    pub message_index: u32,
    pub snippet_hash: String,
    pub safe_excerpt: Option<String>,
    pub triggered_rule: Option<String>,
    pub replay_command: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum InjectionSeverity {
    Low,
    Medium,
    High,
}
```

**CLI Examples:**

```bash
ms doctor --check=safety
ms build --from-cass "auth issues" --no-injection-filter
```

---

### 5.18 Safety Invariant Layer (No Destructive Ops)

ms enforces a hard invariant: destructive filesystem or git operations are never
executed without explicit, verbatim approval. This mirrors the global agent
rules and prevents ms from becoming a footgun.

**Primary Enforcement: DCG Integration:**
- Integrate **Destructive Command Guard (DCG)** from `/data/projects/destructive_command_guard`
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

```rust
/// Effect-based safety classification
/// Classifies by what a command DOES, not what it looks like
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CommandEffect {
    /// No state changes (ls, cat, git status)
    ReadOnly,
    /// Creates new files/directories (touch, mkdir, git init)
    Create,
    /// Modifies existing content in place (edit, append)
    Modify,
    /// Removes files/directories (rm, rmdir)
    Delete,
    /// Overwrites existing content (cp without -n, mv, redirect >)
    Overwrite,
    /// Changes repository state (git commit, git branch)
    GitState,
    /// Rewrites history (git rebase, git reset, git filter-branch)
    GitHistoryRewrite,
    /// Network/external system interaction
    Network,
    /// Unknown effect (should be treated as dangerous)
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SafetyTier {
    Safe,      // ReadOnly, Create (in safe paths)
    Caution,   // Modify, GitState, Network
    Dangerous, // Delete, Overwrite
    Critical,  // GitHistoryRewrite, Unknown
}

impl CommandEffect {
    /// Map effect to safety tier
    pub fn to_tier(&self) -> SafetyTier {
        match self {
            CommandEffect::ReadOnly => SafetyTier::Safe,
            CommandEffect::Create => SafetyTier::Safe,
            CommandEffect::Modify => SafetyTier::Caution,
            CommandEffect::GitState => SafetyTier::Caution,
            CommandEffect::Network => SafetyTier::Caution,
            CommandEffect::Delete => SafetyTier::Dangerous,
            CommandEffect::Overwrite => SafetyTier::Dangerous,
            CommandEffect::GitHistoryRewrite => SafetyTier::Critical,
            CommandEffect::Unknown => SafetyTier::Critical,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyPolicy {
    pub destructive_ops: DestructiveOpsPolicy, // deny | confirm | allow
    pub require_verbatim_approval: bool,
    pub tombstone_deletes: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DestructiveOpsPolicy {
    Deny,
    Confirm,
    Allow,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub command: String,
    /// Classified by effect, not command string pattern
    pub effect: CommandEffect,
    pub tier: SafetyTier,
    pub reason: String,
    pub approve_hint: String,
}

/// DCG integration wrapper (preferred enforcement layer)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DcgGuard {
    pub dcg_bin: PathBuf,
    pub packs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DcgDecision {
    pub allowed: bool,
    pub reason: String,
    pub remediation: Option<String>,
    pub rule_id: Option<String>,
    pub pack: Option<String>,
    pub tier: SafetyTier,
}

impl DcgGuard {
    pub fn evaluate_command(&self, cmd: &str) -> Result<DcgDecision> {
        // Delegate to dcg explain/scan mode and translate decision
        unimplemented!()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandSafetyEvent {
    pub session_id: Option<String>,
    pub command: String,
    pub dcg_version: Option<String>,
    pub dcg_pack: Option<String>,
    pub decision: DcgDecision,
    pub created_at: DateTime<Utc>,
}
```

**Behavior:**
- Destructive commands (delete/overwrite/reset) are blocked by default.
- In robot mode, ms returns `approval_required` with the exact approve hint.
- In human mode, ms prompts for the exact verbatim command string.
- In ms-managed directories, deletions become **tombstones** (content-addressed
  markers); actual pruning is only performed when explicitly invoked.

**Robot Approval Example:**

```json
{
  "status": {
    "approval_required": true,
    "approve_command": "ms prune --approve \"ms prune --scope archive\"",
    "tier": "critical",
    "reason": "destructive operation"
  },
  "timestamp": "2026-01-13T16:00:00Z",
  "version": "0.1.0",
  "data": null,
  "warnings": []
}
```

---

## 6. Progressive Disclosure System

### 6.1 Disclosure Levels

```rust
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum DisclosureLevel {
    /// Level 0: Just name and one-line description
    /// ~50-100 tokens
    Minimal,

    /// Level 1: Name, description, key sections headers
    /// ~200-500 tokens
    Overview,

    /// Level 2: Overview + main content, no examples
    /// ~500-1500 tokens
    Standard,

    /// Level 3: Full SKILL.md content
    /// Variable, typically 1000-5000 tokens
    Full,

    /// Level 4: Full content + scripts + references
    /// Variable, can be 5000+ tokens
    Complete,
}

// Budget-driven alternative:
// Use TokenBudget + packer when an explicit token budget is provided

impl DisclosureLevel {
    pub fn token_budget(&self) -> Option<usize> {
        match self {
            DisclosureLevel::Minimal => Some(100),
            DisclosureLevel::Overview => Some(500),
            DisclosureLevel::Standard => Some(1500),
            DisclosureLevel::Full => None,
            DisclosureLevel::Complete => None,
        }
    }
}
```

### 6.2 Disclosure Logic

```rust
/// Generate content at a specified disclosure plan
pub fn disclose(skill: &Skill, plan: DisclosurePlan) -> DisclosedContent {
    match plan {
        DisclosurePlan::Pack(budget) => disclose_packed(skill, budget),
        DisclosurePlan::Level(level) => disclose_level(skill, level),
    }
}

/// Generate content at specified disclosure level
fn disclose_level(skill: &Skill, level: DisclosureLevel) -> DisclosedContent {
    match level {
        DisclosureLevel::Minimal => DisclosedContent {
            frontmatter: minimal_frontmatter(skill),
            body: None,
            scripts: vec![],
            references: vec![],
        },

        DisclosureLevel::Overview => DisclosedContent {
            frontmatter: full_frontmatter(skill),
            body: Some(extract_headings(&skill.body)),
            scripts: vec![],
            references: vec![],
        },

        DisclosureLevel::Standard => DisclosedContent {
            frontmatter: full_frontmatter(skill),
            body: Some(truncate_examples(&skill.body, 1500)),
            scripts: vec![],
            references: vec![],
        },

        DisclosureLevel::Full => DisclosedContent {
            frontmatter: full_frontmatter(skill),
            body: Some(skill.body.clone()),
            scripts: vec![],
            references: vec![],
        },

        DisclosureLevel::Complete => DisclosedContent {
            frontmatter: full_frontmatter(skill),
            body: Some(skill.body.clone()),
            scripts: skill.assets.scripts.clone(),
            references: skill.assets.references.clone(),
        },
    }
}

/// Budget-driven disclosure using pre-sliced content
fn disclose_packed(skill: &Skill, budget: TokenBudget) -> DisclosedContent {
    let packed = pack_slices(&skill.computed.slices, budget);

    DisclosedContent {
        frontmatter: minimal_frontmatter(skill),
        body: Some(packed.join("\n\n")),
        scripts: vec![],
        references: vec![],
    }
}

#[derive(Debug, Clone, Copy)]
pub enum DisclosurePlan {
    Level(DisclosureLevel),
    Pack(TokenBudget),
}

#[derive(Debug, Clone, Copy)]
pub struct TokenBudget {
    pub tokens: usize,
    pub mode: PackMode,
    pub max_per_group: usize,
}

#[derive(Debug, Clone, Copy)]
pub enum PackMode {
    Balanced,
    UtilityFirst,
    CoverageFirst,
    PitfallSafe,
}
```

### 6.3 Context-Aware Disclosure

```rust
/// Determine optimal disclosure level based on context
pub fn optimal_disclosure(
    skill: &Skill,
    context: &DisclosureContext,
) -> DisclosurePlan {
    // If explicitly requested full, give full
    if context.explicit_level.is_some() {
        return DisclosurePlan::Level(context.explicit_level.unwrap());
    }

    // If a token budget is specified, use packing
    if let Some(tokens) = context.pack_budget {
        return DisclosurePlan::Pack(TokenBudget {
            tokens,
            mode: context.pack_mode.unwrap_or(PackMode::Balanced),
            max_per_group: context.max_per_group.unwrap_or(2),
        });
    }

    // If agent has used this skill before successfully, give standard
    if context.usage_history.successful_uses > 0 {
        return DisclosurePlan::Level(DisclosureLevel::Standard);
    }

    // If remaining context budget is low, give minimal
    if context.remaining_tokens < 1000 {
        return DisclosurePlan::Level(DisclosureLevel::Minimal);
    }

    // If this is a direct request for the skill, give full
    if context.request_type == RequestType::Direct {
        return DisclosurePlan::Level(DisclosureLevel::Full);
    }

    // Default to overview for suggestions
    DisclosurePlan::Level(DisclosureLevel::Overview)
}
```

**Disclosure Context (partial):**

```rust
pub struct DisclosureContext {
    pub explicit_level: Option<DisclosureLevel>,
    pub pack_budget: Option<usize>,
    pub pack_mode: Option<PackMode>,
    pub max_per_group: Option<usize>,
    pub remaining_tokens: usize,
    pub usage_history: UsageHistory,
    pub request_type: RequestType,
}
```

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

```rust
#[derive(Debug, Clone)]
pub struct PackConstraints {
    pub budget: usize,
    pub max_per_group: usize,
    /// Required groups that MUST have at least min_count slices
    pub required_coverage: Vec<CoverageQuota>,
    /// Groups that should never be included
    pub excluded_groups: Vec<String>,
    /// Maximum improvement iterations
    pub max_improvement_passes: usize,

    // --- Packing Invariants (mandatory slices) ---

    /// Slices that MUST be included if they exist (by ID or predicate)
    /// These are not subject to utility ranking - they're hard requirements
    /// Use for: safety warnings, critical pitfalls, license notices
    pub mandatory_slices: Vec<MandatorySlice>,

    /// If true, fail the pack rather than omit mandatory slices
    /// Default: true (safe default - ensures critical content isn't silently dropped)
    pub fail_on_mandatory_omission: bool,

    /// Recent slice IDs already in context (novelty penalty)
    pub recent_slice_ids: Vec<String>,

    /// Optional pack contract enforcing minimum guidance
    pub contract: Option<PackContract>,
}

/// A mandatory slice specification
#[derive(Debug, Clone)]
pub enum MandatorySlice {
    /// Include slice by exact ID
    ById(String),
    /// Include all slices matching a predicate
    ByPredicate(MandatoryPredicate),
}

/// Predicates for mandatory slice matching
#[derive(Debug, Clone)]
pub enum MandatoryPredicate {
    /// Always include (used for non-removable policy lenses)
    Always,
    /// All slices tagged with a specific tag
    HasTag(String),
    /// All slices of a specific type
    OfType(SliceType),
    /// All slices in a specific group
    InGroup(String),
    /// Slices matching a custom filter function name
    Custom(String),
}

#[derive(Debug, Clone)]
pub struct CoverageQuota {
    pub group: String,
    pub min_count: usize,
}

pub struct ConstrainedPacker;

/// Error type for packing failures
#[derive(Debug, Clone)]
pub enum PackError {
    /// A mandatory slice couldn't fit within budget
    MandatorySliceOmitted {
        slice_id: String,
        required_tokens: usize,
        available_tokens: usize,
    },
    /// Budget too small for minimum viable pack
    InsufficientBudget { required: usize, available: usize },
}

impl ConstrainedPacker {
    pub fn pack(&self, slices: &[SkillSlice], constraints: &PackConstraints) -> Result<PackResult, PackError> {
        // Phase 1: Coverage seeding - satisfy mandatory + required groups first
        let mut selected = self.seed_required_coverage(slices, constraints)?;
        let mut remaining = constraints.budget - selected.iter().map(|s| s.token_estimate).sum::<usize>();

        // Phase 2: Greedy fill with utility density + diminishing returns
        let mut group_counts = self.count_groups(&selected);
        let candidates: Vec<_> = slices.iter()
            .filter(|s| !selected.iter().any(|x| x.id == s.id))
            .filter(|s| self.satisfies_constraints(s, &selected, constraints))
            .collect();

        for slice in self.rank_by_density(&candidates, &group_counts) {
            if slice.token_estimate > remaining {
                continue;
            }
            if !self.deps_satisfied(slice, &selected) {
                continue;
            }

            selected.push(slice.clone());
            remaining -= slice.token_estimate;
            if let Some(g) = &slice.coverage_group {
                *group_counts.entry(g.clone()).or_insert(0) += 1;
            }
        }

        // Phase 3: Local improvement - swap low-utility for high-utility
        for _ in 0..constraints.max_improvement_passes {
            if !self.try_improve(&mut selected, slices, constraints) {
                break;
            }
        }

        Ok(PackResult {
            slices: selected,
            total_tokens: constraints.budget - remaining,
            coverage_satisfied: self.check_coverage(&selected, constraints),
        })
    }

    /// Check if a slice matches a mandatory specification
    fn matches_mandatory(&self, slice: &SkillSlice, mandatory: &MandatorySlice) -> bool {
        match mandatory {
            MandatorySlice::ById(id) => &slice.id == id,
            MandatorySlice::ByPredicate(pred) => match pred {
                MandatoryPredicate::Always => true,
                MandatoryPredicate::HasTag(tag) => slice.tags.contains(tag),
                MandatoryPredicate::OfType(slice_type) => &slice.slice_type == slice_type,
                MandatoryPredicate::InGroup(group) => slice.coverage_group.as_ref() == Some(group),
                MandatoryPredicate::Custom(_) => false, // Custom predicates resolved externally
            },
        }
    }

    /// Seed required coverage groups with highest utility slices
    fn seed_required_coverage(&self, slices: &[SkillSlice], constraints: &PackConstraints) -> Result<Vec<SkillSlice>, PackError> {
        let mut selected = Vec::new();
        let mut remaining = constraints.budget;

        // PHASE 0: Include mandatory slices FIRST (packing invariants)
        // These are non-negotiable - they must be included if they exist
        for mandatory in &constraints.mandatory_slices {
            let matching: Vec<_> = slices.iter()
                .filter(|s| self.matches_mandatory(s, mandatory))
                .filter(|s| !selected.iter().any(|x| x.id == s.id))
                .collect();

            for slice in matching {
                if slice.token_estimate <= remaining {
                    selected.push(slice.clone());
                    remaining -= slice.token_estimate;
                } else if constraints.fail_on_mandatory_omission {
                    return Err(PackError::MandatorySliceOmitted {
                        slice_id: slice.id.clone(),
                        required_tokens: slice.token_estimate,
                        available_tokens: remaining,
                    });
                }
            }
        }

        // PHASE 1: Always include Overview (if not already mandatory)
        if let Some(overview) = slices.iter().find(|s| matches!(s.slice_type, SliceType::Overview)) {
            if !selected.iter().any(|x| x.id == overview.id) && overview.token_estimate <= remaining {
                selected.push(overview.clone());
                remaining -= overview.token_estimate;
            }
        }

        // PHASE 2: Satisfy required coverage quotas
        for quota in &constraints.required_coverage {
            let group_slices: Vec<_> = slices.iter()
                .filter(|s| s.coverage_group.as_ref() == Some(&quota.group))
                .filter(|s| !selected.iter().any(|x| x.id == s.id))
                .collect();

            // Sort by utility/token ratio (density)
            let mut ranked: Vec<_> = group_slices.iter()
                .map(|s| (*s, s.utility_score / s.token_estimate as f32))
                .collect();
            ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());

            let mut count = 0;
            for (slice, _) in ranked {
                if count >= quota.min_count || slice.token_estimate > remaining {
                    break;
                }
                selected.push((*slice).clone());
                remaining -= slice.token_estimate;
                count += 1;
            }
        }

        Ok(selected)
    }

    /// Rank candidates by utility density with diminishing returns per group
    fn rank_by_density<'a>(&self, candidates: &[&'a SkillSlice], group_counts: &HashMap<String, usize>) -> Vec<&'a SkillSlice> {
        let mut scored: Vec<_> = candidates.iter()
            .map(|s| {
                let base_density = s.utility_score / s.token_estimate as f32;
                // Apply diminishing returns: each additional slice in group worth less
                let group_penalty = s.coverage_group.as_ref()
                    .map(|g| group_counts.get(g).unwrap_or(&0))
                    .map(|c| 0.8_f32.powi(*c as i32))
                    .unwrap_or(1.0);
                (*s, base_density * group_penalty)
            })
            .collect();
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        scored.into_iter().map(|(s, _)| s).collect()
    }

    /// Local improvement: try swapping low-utility slice for higher-utility one
    fn try_improve(&self, selected: &mut Vec<SkillSlice>, all: &[SkillSlice], constraints: &PackConstraints) -> bool {
        let current_tokens: usize = selected.iter().map(|s| s.token_estimate).sum();
        let slack = constraints.budget - current_tokens;

        // Find lowest-utility non-required slice
        let (worst_idx, worst) = selected.iter().enumerate()
            .filter(|(_, s)| !self.is_required_for_coverage(s, selected, constraints))
            .min_by(|a, b| a.1.utility_score.partial_cmp(&b.1.utility_score).unwrap())?;

        // Find better replacement that fits
        let replacement = all.iter()
            .filter(|s| !selected.iter().any(|x| x.id == s.id))
            .filter(|s| s.token_estimate <= worst.token_estimate + slack)
            .filter(|s| s.utility_score > worst.utility_score)
            .max_by(|a, b| a.utility_score.partial_cmp(&b.utility_score).unwrap());

        if let Some(better) = replacement {
            selected.remove(worst_idx);
            selected.push(better.clone());
            return true;
        }
        false
    }
}

#[derive(Debug)]
pub struct PackResult {
    pub slices: Vec<SkillSlice>,
    pub total_tokens: usize,
    pub coverage_satisfied: bool,
}

fn estimate_packed_tokens_with_constraints(
    index: &SkillSliceIndex,
    budget: usize,
    mode: PackMode,
    max_per_group: usize,
) -> usize {
    let packed = pack_slices(index, TokenBudget {
        tokens: budget,
        mode,
        max_per_group,
    });
    packed.iter().map(|s| estimate_tokens(s)).sum()
}

fn score_slice(slice: &SkillSlice, mode: PackMode) -> f32 {
    match mode {
        PackMode::UtilityFirst => slice.utility_score,
        PackMode::CoverageFirst => match slice.slice_type {
            SliceType::Rule => slice.utility_score + 0.2,
            SliceType::Command => slice.utility_score + 0.15,
            SliceType::Example => slice.utility_score + 0.1,
            _ => slice.utility_score,
        },
        PackMode::PitfallSafe => match slice.slice_type {
            SliceType::Pitfall => slice.utility_score + 0.25,
            SliceType::Rule => slice.utility_score + 0.1,
            _ => slice.utility_score,
        },
        PackMode::Balanced => slice.utility_score,
    }
}
```

**CLI Example:**

```bash
ms load ntm --pack 800
```

### 6.5 Conditional Block Predicates

Skills often contain version-specific or environment-specific content. Rather than
maintaining separate skills or relying on the agent to reason about versions,
ms supports **block-level predicates** that strip irrelevant content at load time.

**Markdown Syntax:**

```markdown
::: block id="unique-id" condition="<predicate>"
Content only included when predicate evaluates true...
:::
```

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

```rust
pub fn evaluate_predicate(pred: &SlicePredicate, ctx: &ProjectContext) -> bool {
    match &pred.predicate_type {
        PredicateType::PackageVersion { package, op, version } => {
            ctx.package_versions
                .get(package)
                .map(|v| compare_versions(v, op, version))
                .unwrap_or(false)  // Missing package = false
        }
        PredicateType::EnvVar { var } => std::env::var(var).is_ok(),
        PredicateType::FileExists { pattern } => {
            glob::glob(&ctx.root.join(pattern).to_string_lossy())
                .map(|mut g| g.next().is_some())
                .unwrap_or(false)
        }
        PredicateType::RustEdition { op, edition } => {
            ctx.rust_edition
                .as_ref()
                .map(|e| compare_editions(e, op, edition))
                .unwrap_or(false)
        }
        PredicateType::ToolVersion { tool, op, version } => {
            detect_tool_version(tool)
                .map(|v| compare_versions(&v, op, version))
                .unwrap_or(false)
        }
    }
}

pub fn filter_slices_by_predicates(
    slices: &[SkillSlice],
    ctx: &ProjectContext,
) -> Vec<SkillSlice> {
    slices
        .iter()
        .filter(|s| {
            s.condition
                .as_ref()
                .map(|p| evaluate_predicate(p, ctx))
                .unwrap_or(true)  // No condition = always include
        })
        .cloned()
        .collect()
}
```

**Why This Matters:**

The agent *cannot* hallucinate using deprecated patterns because those patterns
are physically absent from its context window. This directly addresses the
version drift problem (e.g., Next.js middleware.ts vs proxy.ts) mentioned in
AGENTS.md without requiring separate skills or complex agent reasoning.

**CLI Example:**

```bash
# Force predicate evaluation with explicit context
ms load nextjs-routing --eval-predicates --package-version next=16.1.0

# Show which blocks would be filtered
ms load nextjs-routing --dry-run --show-filtered
```

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

```rust
/// A curated composition of slices from multiple skills
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaSkill {
    pub id: String,
    pub name: String,
    pub description: String,

    /// Ordered slice references from source skills
    pub slices: Vec<MetaSkillSliceRef>,

    /// Version pinning strategy
    pub pin_strategy: PinStrategy,

    /// When this meta-skill was last validated
    pub validated_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaSkillSliceRef {
    /// Source skill id
    pub skill_id: String,

    /// Slice id within the skill
    pub slice_id: String,

    /// Optional content hash for reproducibility (when pinned)
    pub content_hash: Option<String>,

    /// Override utility score for packing priority
    pub priority_override: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PinStrategy {
    /// Always use latest slice content
    Float,
    /// Pin to specific content hashes
    Pinned,
    /// Pin major version, float patches
    SemverFloat { major: u32 },
}
```

**CLI Commands:**

```bash
# Create a meta-skill from multiple sources
ms meta create frontend-polish \
  --from nextjs-ui-polish:rule-*,checklist-* \
  --from a11y-css:rule-1,rule-2,example-1 \
  --from react-patterns:hooks-rules

# Load meta-skill with unified packing
ms load frontend-polish --pack 1200

# Edit slice composition
ms meta edit frontend-polish --add tailwind-responsive:rule-* --remove a11y-css:example-1

# Validate all slice references still exist
ms meta doctor frontend-polish

# List meta-skills
ms meta list

# Show meta-skill composition
ms meta show frontend-polish --slices
```

**Resolution and Packing:**

```rust
impl MetaSkillLoader {
    pub fn resolve(&self, meta: &MetaSkill, registry: &SkillRegistry) -> Result<Vec<SkillSlice>> {
        let mut resolved = Vec::new();

        for slice_ref in &meta.slices {
            let skill = registry.get(&slice_ref.skill_id)?;
            let slice = skill.slices.get(&slice_ref.slice_id)
                .ok_or_else(|| anyhow!("Slice {} not found in {}", slice_ref.slice_id, slice_ref.skill_id))?;

            // Verify hash if pinned
            if let Some(expected_hash) = &slice_ref.content_hash {
                let actual_hash = hash_content(&slice.content);
                if &actual_hash != expected_hash {
                    return Err(anyhow!(
                        "Content drift detected for {}:{} (expected {}, got {})",
                        slice_ref.skill_id, slice_ref.slice_id, expected_hash, actual_hash
                    ));
                }
            }

            let mut resolved_slice = slice.clone();
            if let Some(priority) = slice_ref.priority_override {
                resolved_slice.utility_score = priority;
            }
            resolved.push(resolved_slice);
        }

        Ok(resolved)
    }

    pub fn load_packed(&self, meta: &MetaSkill, budget: usize) -> Result<String> {
        let slices = self.resolve(meta, &self.registry)?;
        let packed = pack_slices(&slices, TokenBudget { tokens: budget, ..Default::default() });
        Ok(packed.join("\n\n"))
    }
}
```

**Use Cases:**

- **NTM integration:** Define meta-skills per bead type (e.g., `ui-polish-bead`, `api-refactor-bead`)
- **Onboarding:** Ship `team-standards` meta-skill bundling all org-required rules
- **Tech stack kits:** `rust-cli-complete`, `nextjs-fullstack`, `go-microservice`

---

## 7. Search & Suggestion Engine

### 7.1 Hybrid Search (Following xf Pattern)

```rust
/// Hybrid search combining BM25 and vector similarity
pub struct HybridSearcher {
    tantivy_index: Index,
    embedding_index: VectorIndex,
    rrf_k: f32,  // Default: 60.0
}

impl HybridSearcher {
    /// Search with RRF fusion
    pub async fn search(
        &self,
        query: &str,
        filters: &SearchFilters,
        limit: usize,
    ) -> Result<Vec<SearchResult>> {
        // Run both searches in parallel
        let (bm25_results, vector_results) = tokio::join!(
            self.bm25_search(query, limit * 2),
            self.vector_search(query, limit * 2),
        );

        // Compute RRF scores
        let mut rrf_scores: HashMap<String, f32> = HashMap::new();

        for (rank, result) in bm25_results?.iter().enumerate() {
            let score = 1.0 / (self.rrf_k + rank as f32 + 1.0);
            *rrf_scores.entry(result.skill_id.clone()).or_default() += score;
        }

        for (rank, result) in vector_results?.iter().enumerate() {
            let score = 1.0 / (self.rrf_k + rank as f32 + 1.0);
            *rrf_scores.entry(result.skill_id.clone()).or_default() += score;
        }

        // Apply filters and sort
        let mut results: Vec<_> = rrf_scores
            .into_iter()
            .filter(|(id, _)| self.passes_filters(id, filters))
            .map(|(id, score)| SearchResult { skill_id: id, score })
            .collect();

        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        results.truncate(limit);

        Ok(results)
    }
}
```

**Alias + Deprecation Handling:**
- If the query exactly matches a skill alias, ms resolves to the canonical skill id.
- Deprecated skills are filtered out by default (use `--include-deprecated` to show them).

### 7.2 Context-Aware Suggestion

```rust
/// Suggest skills based on current context
pub struct Suggester {
    searcher: HybridSearcher,
    registry: SkillRegistry,
    requirements: RequirementChecker,
    bandit: Option<SignalBandit>,
}

impl Suggester {
    pub async fn suggest(&self, context: &SuggestionContext) -> Result<Vec<Suggestion>> {
        let mut signals: Vec<SuggestionSignal> = vec![];

        // Signal 1: Current directory patterns
        if let Some(cwd) = &context.cwd {
            signals.extend(self.analyze_directory(cwd).await?);
        }

        // Signal 2: Current file patterns
        if let Some(file) = &context.current_file {
            signals.extend(self.analyze_file(file).await?);
        }

        // Signal 3: Recent commands
        if !context.recent_commands.is_empty() {
            signals.extend(self.analyze_commands(&context.recent_commands)?);
        }

        // Signal 4: Explicit query
        if let Some(query) = &context.query {
            signals.push(SuggestionSignal::Query(query.clone()));
        }

        // Convert signals to search query
        let query = self.signals_to_query(&signals);

        // Search and boost by trigger matches
        let mut results = self.searcher.search(&query, &SearchFilters::default(), 20).await?;

        let pack_budget = context.pack_budget;
        let pack_mode = context.pack_mode;
        let max_per_group = context.max_per_group;
        let explain = context.explain;

        for result in &mut results {
            if let Some(resolved) = self.registry.effective(&result.skill_id).ok() {
                let skill = &resolved.skill;
                result.score *= self.trigger_boost(skill, &signals);
                result.dependencies = skill.metadata.requires.clone();
                result.layer = Some(format!("{:?}", skill.source.layer).to_lowercase());
                result.conflicts = resolved.conflicts.iter()
                    .map(|c| c.section.clone())
                    .collect();

                if skill.metadata.deprecated.is_some() && !context.include_deprecated {
                    result.score = 0.0;
                    result.reason = "deprecated".to_string();
                    continue;
                }

                let req_status = self.requirements.check(skill, context);
                result.requirements = Some(req_status.clone());
                if !req_status.is_satisfied() {
                    result.score *= 0.6; // down-rank incompatible skills
                    result.reason = req_status.summary();
                }

                if let Some(deprecation) = &skill.metadata.deprecated {
                    result.score *= 0.2; // heavily down-rank deprecated skills
                    if let Some(replacement) = &deprecation.replaced_by {
                        result.reason = format!("deprecated → use {}", replacement);
                    } else {
                        result.reason = "deprecated".to_string();
                    }
                }

                if explain {
                    result.explanation = Some(self.explain_result(skill, &signals, result.score));
                }

                if let Some(budget) = pack_budget {
                    result.disclosure_level = "pack".into();
                    result.pack_budget = Some(budget);
                    result.packed_token_estimate = Some(
                        estimate_packed_tokens_with_constraints(
                            &skill.computed.slices,
                            budget,
                            pack_mode.unwrap_or(PackMode::Balanced),
                            max_per_group.unwrap_or(2),
                        )
                    );
                }
            }
        }

        results.retain(|r| r.score > 0.0);
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());

        Ok(results.into_iter().take(5).map(Into::into).collect())
    }

    fn trigger_boost(&self, skill: &Skill, signals: &[SuggestionSignal]) -> f32 {
        let mut boost = 1.0;

        for trigger in &skill.metadata.triggers {
            for signal in signals {
                if self.matches_trigger(trigger, signal) {
                    boost *= 1.0 + trigger.boost;
                }
            }
        }

        boost
    }

    fn explain_result(
        &self,
        skill: &Skill,
        signals: &[SuggestionSignal],
        final_score: f32,
    ) -> SuggestionExplanation {
        let matched_triggers = skill.metadata.triggers.iter()
            .filter(|t| signals.iter().any(|s| self.matches_trigger(t, s)))
            .map(|t| format!("{}:{}", t.trigger_type, t.pattern))
            .collect::<Vec<_>>();

        let signal_scores = signals.iter().map(|s| SignalScore {
            signal: format!("{:?}", s),
            contribution: 0.0, // populated from search logs
        }).collect();

        SuggestionExplanation {
            matched_triggers,
            signal_scores,
            rrf_components: RrfBreakdown {
                bm25_rank: None,
                vector_rank: None,
                rrf_score: final_score,
            },
        }
    }
}
```

When `--for-ntm` is used, `ms suggest` returns `swarm_plan` in robot mode so
each agent can load a complementary slice pack instead of duplicating content.

**Bandit-Weighted Signal Selection:**
- A contextual bandit learns per-project weighting over signals (bm25, embeddings,
  triggers, freshness, project match) using usage/outcome rewards.
- Replaces static tuning with adaptive, self-optimizing retrieval.

```rust
#[derive(Debug, Clone)]
pub struct SignalBandit {
    pub arms: Vec<SignalWeighting>,
    pub prior: SignalWeighting,
}
```

**Suggestion Context (partial):**

```rust
pub struct SuggestionContext {
    pub cwd: Option<PathBuf>,
    pub current_file: Option<PathBuf>,
    pub recent_commands: Vec<String>,
    pub query: Option<String>,
    pub pack_budget: Option<usize>,
    pub explain: bool,
    pub pack_mode: Option<PackMode>,
    pub max_per_group: Option<usize>,
    pub environment: Option<EnvironmentSnapshot>,
    pub include_deprecated: bool,
    pub swarm: Option<SwarmContext>,
}
```

**Requirement-aware suggestions:**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvironmentSnapshot {
    pub platform: Platform,
    pub tools: HashMap<String, Option<String>>, // name -> version (if known)
    pub env_vars: Vec<String>,
    pub network: NetworkStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NetworkStatus {
    Online,
    Offline,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequirementStatus {
    pub platform_ok: bool,
    pub missing_tools: Vec<String>,
    pub missing_env: Vec<String>,
    pub network_ok: bool,
}

impl RequirementStatus {
    pub fn is_satisfied(&self) -> bool {
        self.platform_ok
            && self.missing_tools.is_empty()
            && self.missing_env.is_empty()
            && self.network_ok
    }

    pub fn summary(&self) -> String {
        let mut parts = vec![];
        if !self.platform_ok {
            parts.push("platform mismatch".to_string());
        }
        if !self.missing_tools.is_empty() {
            parts.push(format!("missing tools: {}", self.missing_tools.join(", ")));
        }
        if !self.missing_env.is_empty() {
            parts.push(format!("missing env: {}", self.missing_env.join(", ")));
        }
        if !self.network_ok {
            parts.push("network required".to_string());
        }
        if parts.is_empty() {
            "requirements satisfied".to_string()
        } else {
            parts.join("; ")
        }
    }
}

pub struct RequirementChecker;

impl RequirementChecker {
    pub fn check(&self, skill: &Skill, context: &SuggestionContext) -> RequirementStatus {
        let env = match &context.environment {
            Some(env) => env,
            None => {
                return RequirementStatus {
                    platform_ok: true,
                    missing_tools: vec![],
                    missing_env: vec![],
                    network_ok: true,
                };
            }
        };

        let platforms = &skill.metadata.requirements.platforms;
        let platform_ok = platforms.is_empty()
            || platforms.iter().any(|p| matches!(p, Platform::Any) || p == &env.platform);

        let missing_tools = skill.metadata.requirements.tools.iter()
            .filter(|tool| {
                tool.required && !env.tools.contains_key(&tool.name)
            })
            .map(|tool| tool.name.clone())
            .collect::<Vec<_>>();

        let missing_env = skill.metadata.requirements.env.iter()
            .filter(|var| !env.env_vars.iter().any(|v| v == *var))
            .cloned()
            .collect::<Vec<_>>();

        let network_ok = match (skill.metadata.requirements.network.clone(), &env.network) {
            (NetworkRequirement::Required, NetworkStatus::Offline) => false,
            (NetworkRequirement::PreferOffline, NetworkStatus::Online) => false,
            _ => true,
        };

        RequirementStatus {
            platform_ok,
            missing_tools,
            missing_env,
            network_ok,
        }
    }
}
```

**Collective Pack Planning (Swarm / NTM):**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmContext {
    pub agent_count: usize,
    pub budget_per_agent: usize,
    pub objective: PackObjective,
    pub replicate_pitfalls: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PackObjective {
    CoverageFirst,
    RedundancyMin,
    SafetyFirst,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SwarmRole {
    Leader,
    Worker,
    Reviewer,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmPlan {
    pub agents: Vec<AgentPack>,
    pub objective: PackObjective,
    pub total_tokens: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentPack {
    pub agent_id: usize,
    pub role: SwarmRole,
    pub slice_ids: Vec<String>,
    pub token_estimate: usize,
}

impl Suggester {
    pub fn plan_swarm_packs(
        &self,
        skill: &Skill,
        context: &SwarmContext,
    ) -> SwarmPlan {
        // Greedy assignment: maximize marginal coverage, minimize duplicate groups
        // Ensure CRITICAL RULE slices appear in >= 1 agent
        // Optionally replicate Pitfall slices across all agents
        // Role-aware packs: Leader gets decision structure + pitfalls, Workers get commands/examples,
        // Reviewer gets semantic diff + safety invariants
        unimplemented!()
    }
}
```

### 7.2.1 Context Fingerprints & Suggestion Cooldowns

To prevent `ms suggest` from spamming the same skills repeatedly when context hasn't meaningfully changed, we compute a **context fingerprint** and maintain a cooldown cache.

```rust
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;

/// Compact fingerprint of suggestion context for deduplication
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContextFingerprint {
    pub repo_root: Option<String>,
    pub git_head: Option<String>,      // HEAD commit SHA (first 8 chars)
    pub diff_hash: Option<u64>,        // hash of `git diff` output
    pub open_files_hash: u64,          // hash of sorted open file paths
    pub recent_commands_hash: u64,     // hash of last N commands
}

impl ContextFingerprint {
    pub fn compute(context: &SuggestionContext) -> Self {
        let repo_root = context.cwd.as_ref()
            .and_then(|p| find_repo_root(p))
            .map(|p| p.to_string_lossy().into_owned());

        let git_head = repo_root.as_ref()
            .and_then(|_| git_head_short().ok());

        let diff_hash = repo_root.as_ref()
            .and_then(|_| git_diff_hash().ok());

        let open_files_hash = hash_string_list(
            &context.open_files.iter()
                .map(|p| p.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        );

        let recent_commands_hash = hash_string_list(&context.recent_commands);

        Self {
            repo_root,
            git_head,
            diff_hash,
            open_files_hash,
            recent_commands_hash,
        }
    }

    /// Returns true if context has meaningfully changed
    pub fn differs_from(&self, other: &Self) -> bool {
        // Git HEAD change = definitely new context
        if self.git_head != other.git_head {
            return true;
        }
        // Diff hash change = working tree changed
        if self.diff_hash != other.diff_hash {
            return true;
        }
        // Open files changed significantly
        if self.open_files_hash != other.open_files_hash {
            return true;
        }
        // Different commands issued
        if self.recent_commands_hash != other.recent_commands_hash {
            return true;
        }
        false
    }

    pub fn fingerprint_hash(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.hash(&mut hasher);
        hasher.finish()
    }
}

fn hash_string_list(items: &[String]) -> u64 {
    let mut sorted = items.to_vec();
    sorted.sort();
    let mut hasher = DefaultHasher::new();
    for item in sorted {
        item.hash(&mut hasher);
    }
    hasher.finish()
}

fn git_head_short() -> Result<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--short=8", "HEAD"])
        .output()?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn git_diff_hash() -> Result<u64> {
    let output = std::process::Command::new("git")
        .args(["diff", "HEAD"])
        .output()?;
    let mut hasher = DefaultHasher::new();
    output.stdout.hash(&mut hasher);
    hasher.finish().pipe(Ok)
}
```

**Cooldown Cache:**

```rust
use std::time::{Duration, SystemTime};

/// Tracks recent suggestions to implement cooldowns
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestionCooldownCache {
    /// Map from fingerprint hash -> (skill_ids, timestamp)
    entries: HashMap<u64, CooldownEntry>,
    /// Maximum cache entries before LRU eviction
    max_entries: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CooldownEntry {
    skill_ids: Vec<String>,
    suggested_at: SystemTime,
    fingerprint: ContextFingerprint,
}

impl SuggestionCooldownCache {
    const CACHE_PATH: &'static str = "ms/suggest-cooldowns.json";
    const DEFAULT_COOLDOWN: Duration = Duration::from_secs(300); // 5 minutes
    const MAX_ENTRIES: usize = 100;

    pub fn load() -> Self {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from(".cache"));
        let cache_path = cache_dir.join(Self::CACHE_PATH);

        if let Ok(data) = std::fs::read_to_string(&cache_path) {
            if let Ok(cache) = serde_json::from_str(&data) {
                return cache;
            }
        }

        Self {
            entries: HashMap::new(),
            max_entries: Self::MAX_ENTRIES,
        }
    }

    pub fn save(&self) -> Result<()> {
        let cache_dir = dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from(".cache"));
        let cache_path = cache_dir.join(Self::CACHE_PATH);

        std::fs::create_dir_all(cache_path.parent().unwrap())?;
        let data = serde_json::to_string_pretty(self)?;
        std::fs::write(&cache_path, data)?;
        Ok(())
    }

    /// Check if we should suppress these suggestions due to cooldown
    pub fn should_suppress(
        &self,
        fingerprint: &ContextFingerprint,
        skill_ids: &[String],
        cooldown: Duration,
    ) -> bool {
        let fp_hash = fingerprint.fingerprint_hash();

        if let Some(entry) = self.entries.get(&fp_hash) {
            // Same fingerprint = same context, check cooldown
            if let Ok(elapsed) = entry.suggested_at.elapsed() {
                if elapsed < cooldown {
                    // Check if skills overlap significantly (>50%)
                    let overlap = skill_ids.iter()
                        .filter(|id| entry.skill_ids.contains(id))
                        .count();
                    let overlap_ratio = overlap as f32 / skill_ids.len().max(1) as f32;
                    return overlap_ratio > 0.5;
                }
            }
        }

        false
    }

    /// Record a suggestion for cooldown tracking
    pub fn record(
        &mut self,
        fingerprint: ContextFingerprint,
        skill_ids: Vec<String>,
    ) {
        let fp_hash = fingerprint.fingerprint_hash();

        // LRU eviction if at capacity
        if self.entries.len() >= self.max_entries {
            self.evict_oldest();
        }

        self.entries.insert(fp_hash, CooldownEntry {
            skill_ids,
            suggested_at: SystemTime::now(),
            fingerprint,
        });
    }

    fn evict_oldest(&mut self) {
        if let Some((oldest_key, _)) = self.entries.iter()
            .min_by_key(|(_, e)| e.suggested_at)
            .map(|(k, e)| (*k, e.clone()))
        {
            self.entries.remove(&oldest_key);
        }
    }

    /// Clear expired entries
    pub fn gc(&mut self, max_age: Duration) {
        let now = SystemTime::now();
        self.entries.retain(|_, entry| {
            entry.suggested_at.elapsed()
                .map(|e| e < max_age)
                .unwrap_or(false)
        });
    }
}
```

**Integration with Suggester:**

```rust
impl Suggester {
    pub async fn suggest_with_cooldown(
        &self,
        context: &SuggestionContext,
        cooldown_config: &CooldownConfig,
    ) -> Result<SuggestionResult> {
        let fingerprint = ContextFingerprint::compute(context);
        let mut cache = SuggestionCooldownCache::load();

        // Get raw suggestions
        let suggestions = self.suggest(context).await?;
        let skill_ids: Vec<_> = suggestions.iter()
            .map(|s| s.skill_id.clone())
            .collect();

        // Check cooldown
        let suppressed = if cooldown_config.enabled {
            cache.should_suppress(
                &fingerprint,
                &skill_ids,
                cooldown_config.duration,
            )
        } else {
            false
        };

        if suppressed {
            return Ok(SuggestionResult {
                suggestions: vec![],
                suppressed: true,
                reason: Some("Same context, suggestions on cooldown".into()),
                fingerprint: Some(fingerprint),
            });
        }

        // Record for future cooldown checks
        cache.record(fingerprint.clone(), skill_ids);
        cache.gc(Duration::from_secs(3600)); // 1 hour max age
        let _ = cache.save(); // Best effort persist

        Ok(SuggestionResult {
            suggestions,
            suppressed: false,
            reason: None,
            fingerprint: Some(fingerprint),
        })
    }
}

#[derive(Debug, Clone)]
pub struct CooldownConfig {
    pub enabled: bool,
    pub duration: Duration,
}

impl Default for CooldownConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            duration: SuggestionCooldownCache::DEFAULT_COOLDOWN,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct SuggestionResult {
    pub suggestions: Vec<Suggestion>,
    pub suppressed: bool,
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<ContextFingerprint>,
}
```

**CLI flags:**

```bash
# Disable cooldown for this invocation
ms suggest --no-cooldown

# Custom cooldown duration
ms suggest --cooldown=60s

# Show fingerprint in output (for debugging)
ms suggest --show-fingerprint

# Clear cooldown cache
ms suggest --clear-cache
```

This mechanism prevents suggestion spam in tight loops (e.g., IDE integrations calling `ms suggest` on every keystroke) while still responding to meaningful context changes like new commits, file edits, or command history.

### 7.3 Hash-Based Embeddings (From xf)

```rust
/// Generate hash-based embeddings (no ML model needed)
/// Uses FNV-1a hash with dimension reduction
pub fn hash_embedding(text: &str, dimensions: usize) -> Vec<f32> {
    let mut embedding = vec![0.0f32; dimensions];

    // Tokenize
    let tokens: Vec<&str> = text
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty() && s.len() > 2)
        .collect();

    // Hash each token and accumulate
    for token in &tokens {
        let hash = fnv1a_hash(token.as_bytes());

        // Use hash to determine dimension and sign
        for i in 0..dimensions {
            let dim_hash = fnv1a_hash(&[hash as u8, i as u8]);
            let sign = if dim_hash & 1 == 0 { 1.0 } else { -1.0 };
            let dim = (dim_hash as usize >> 1) % dimensions;
            embedding[dim] += sign;
        }
    }

    // Also hash n-grams for context
    for window in tokens.windows(2) {
        let bigram = format!("{} {}", window[0], window[1]);
        let hash = fnv1a_hash(bigram.as_bytes());

        for i in 0..dimensions {
            let dim_hash = fnv1a_hash(&[hash as u8, i as u8]);
            let sign = if dim_hash & 1 == 0 { 0.5 } else { -0.5 };
            let dim = (dim_hash as usize >> 1) % dimensions;
            embedding[dim] += sign;
        }
    }

    // L2 normalize
    let norm: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in &mut embedding {
            *x /= norm;
        }
    }

    embedding
}
```

### 7.3.1 Pluggable Embedding Backends

Hash embeddings are the default (fast, deterministic, zero dependencies). For
higher semantic fidelity, ms supports an optional local ML embedder.

```rust
pub trait Embedder {
    fn embed(&self, text: &str) -> Vec<f32>;
    fn dims(&self) -> usize;
}

pub struct HashEmbedder {
    pub dims: usize,
}

impl Embedder for HashEmbedder {
    fn embed(&self, text: &str) -> Vec<f32> {
        hash_embedding(text, self.dims)
    }
    fn dims(&self) -> usize { self.dims }
}

pub struct LocalMlEmbedder;

impl Embedder for LocalMlEmbedder {
    fn embed(&self, text: &str) -> Vec<f32> {
        // Optional feature: local model inference
        unimplemented!()
    }
    fn dims(&self) -> usize { 384 }
}
```

**Selection Rules:**
- Default: `HashEmbedder`
- If `embeddings.backend = "local"` and model available → `LocalMlEmbedder`
- Fallback to hash if local model missing

### 7.4 Skill Quality Scoring Algorithm

Quality scoring determines which skills are most worth surfacing to agents. This section details the multi-factor scoring algorithm, including provenance (evidence coverage and confidence).

```rust
/// Comprehensive skill quality scorer
pub struct QualityScorer {
    weights: QualityWeights,
    usage_tracker: UsageTracker,
    toolchain_detector: ToolchainDetector,
    project_path: Option<PathBuf>,
}

/// Configurable weights for quality factors
#[derive(Debug, Clone)]
pub struct QualityWeights {
    pub structure_weight: f32,      // Default: 0.18
    pub content_weight: f32,        // Default: 0.22
    pub effectiveness_weight: f32,  // Default: 0.25
    pub provenance_weight: f32,     // Default: 0.15
    pub safety_weight: f32,         // Default: 0.05
    pub freshness_weight: f32,      // Default: 0.10
    pub popularity_weight: f32,     // Default: 0.10
}

impl Default for QualityWeights {
    fn default() -> Self {
        Self {
            structure_weight: 0.18,
            content_weight: 0.22,
            effectiveness_weight: 0.25,
            provenance_weight: 0.15,
            safety_weight: 0.05,
            freshness_weight: 0.10,
            popularity_weight: 0.05,
        }
    }
}

/// Detailed quality breakdown
#[derive(Debug, Clone, Serialize)]
pub struct QualityScore {
    /// Overall score (0.0 - 1.0)
    pub overall: f32,

    /// Individual factor scores
    pub factors: QualityFactors,

    /// Quality issues found
    pub issues: Vec<QualityIssue>,

    /// Suggestions for improvement
    pub suggestions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct QualityFactors {
    /// Structure: proper YAML frontmatter, sections, formatting
    pub structure: f32,

    /// Content: token density, actionability, specificity
    pub content: f32,

    /// Effectiveness: usage success rate, agent feedback
    pub effectiveness: f32,

    /// Provenance: evidence coverage and confidence
    pub provenance: f32,

    /// Safety: presence and coverage of anti-patterns and pitfalls
    pub safety: f32,

    /// Freshness: recency of updates, relevance to current tools
    pub freshness: f32,

    /// Popularity: usage frequency, explicit ratings
    pub popularity: f32,
}

impl QualityScorer {
    /// Calculate comprehensive quality score
    pub fn score(&self, skill: &Skill) -> QualityScore {
        let structure = self.score_structure(skill);
        let content = self.score_content(skill);
        let effectiveness = self.score_effectiveness(skill);
        let provenance = self.score_provenance(skill);
        let safety = self.score_safety(skill);
        let freshness = self.score_freshness(skill);
        let popularity = self.score_popularity(skill);

        let overall = self.weights.structure_weight * structure.score
            + self.weights.content_weight * content.score
            + self.weights.effectiveness_weight * effectiveness.score
            + self.weights.provenance_weight * provenance.score
            + self.weights.safety_weight * safety.score
            + self.weights.freshness_weight * freshness.score
            + self.weights.popularity_weight * popularity.score;

        let mut issues = vec![];
        issues.extend(structure.issues);
        issues.extend(content.issues);
        issues.extend(effectiveness.issues);

        let mut suggestions = vec![];
        suggestions.extend(structure.suggestions);
        suggestions.extend(content.suggestions);

        QualityScore {
            overall,
            factors: QualityFactors {
                structure: structure.score,
                content: content.score,
                effectiveness: effectiveness.score,
                provenance: provenance.score,
                safety: safety.score,
                freshness: freshness.score,
                popularity: popularity.score,
            },
            issues,
            suggestions,
        }
    }

    /// Score structural quality
    fn score_structure(&self, skill: &Skill) -> FactorResult {
        let mut score = 1.0;
        let mut issues = vec![];
        let mut suggestions = vec![];

        // Check required frontmatter fields
        if skill.metadata.name.is_empty() {
            score -= 0.3;
            issues.push(QualityIssue::MissingField("name".into()));
        }

        if skill.metadata.description.is_empty() {
            score -= 0.2;
            issues.push(QualityIssue::MissingField("description".into()));
        } else if skill.metadata.description.len() < 20 {
            score -= 0.1;
            suggestions.push("Description should be at least 20 characters".into());
        }

        // Check for good section structure
        let sections = count_sections(&skill.body);
        if sections < 2 {
            score -= 0.15;
            suggestions.push("Add more sections for better organization".into());
        }

        // Check for code examples
        let code_blocks = count_code_blocks(&skill.body);
        if code_blocks == 0 {
            score -= 0.1;
            suggestions.push("Add code examples for clarity".into());
        }

        // Check for CRITICAL RULES section (bonus)
        if skill.body.contains("⚠️") || skill.body.contains("CRITICAL") {
            score += 0.05;  // Bonus for having safety constraints
        }

        FactorResult { score: score.clamp(0.0, 1.0), issues, suggestions }
    }

    /// Score content quality
    fn score_content(&self, skill: &Skill) -> FactorResult {
        let mut score = 1.0;
        let mut issues = vec![];
        let mut suggestions = vec![];

        // Token density: content length vs. substantive content ratio
        let token_count = estimate_tokens(&skill.body);
        let substantive_ratio = calculate_substantive_ratio(&skill.body);

        if substantive_ratio < 0.6 {
            score -= 0.2;
            issues.push(QualityIssue::LowDensity(substantive_ratio));
            suggestions.push("Remove filler words and generic content".into());
        }

        // Actionability: presence of commands, code, specific steps
        let actionable_content = count_actionable_elements(&skill.body);
        if actionable_content < 3 {
            score -= 0.15;
            suggestions.push("Add more actionable content (commands, code, steps)".into());
        }

        // Specificity: avoid vague language
        let vague_phrases = count_vague_phrases(&skill.body);
        if vague_phrases > 5 {
            score -= 0.1 * (vague_phrases as f32 / 10.0).min(0.3);
            suggestions.push("Replace vague phrases with specific guidance".into());
        }

        // Check for "THE EXACT PROMPT" sections (bonus for reusable prompts)
        if skill.body.contains("THE EXACT PROMPT") || skill.body.contains("```prompt") {
            score += 0.1;
        }

        // Tables for structured info (bonus)
        let tables = count_tables(&skill.body);
        if tables > 0 {
            score += 0.05 * (tables as f32).min(3.0);
        }

        FactorResult { score: score.clamp(0.0, 1.0), issues, suggestions }
    }

    /// Score effectiveness based on usage data
    fn score_effectiveness(&self, skill: &Skill) -> FactorResult {
        let mut score = 0.5;  // Start neutral if no data
        let issues = vec![];
        let suggestions = vec![];

        if let Some(usage) = self.usage_tracker.get_usage(&skill.id) {
            // Success rate: times skill led to successful outcomes
            if usage.total_uses > 0 {
                let success_rate = usage.successful_uses as f32 / usage.total_uses as f32;
                score = success_rate;
            }

            // Boost if explicitly rated positively
            if usage.positive_ratings > usage.negative_ratings {
                score += 0.1;
            }

            // Penalize if agents frequently abandon mid-skill
            if usage.abandoned_uses as f32 / usage.total_uses.max(1) as f32 > 0.3 {
                score -= 0.15;
            }
        }

        FactorResult { score: score.clamp(0.0, 1.0), issues, suggestions }
    }

    /// Score provenance based on evidence coverage and confidence
    fn score_provenance(&self, skill: &Skill) -> FactorResult {
        let mut score = 0.5;  // Neutral if no evidence yet
        let mut issues = vec![];
        let mut suggestions = vec![];

        let coverage = &skill.evidence.coverage;
        if coverage.total_rules > 0 {
            let coverage_ratio =
                coverage.rules_with_evidence as f32 / coverage.total_rules as f32;
            score = (0.7 * coverage_ratio) + (0.3 * coverage.avg_confidence);

            if coverage_ratio < 0.7 {
                issues.push(QualityIssue::LowEvidenceCoverage(coverage_ratio));
                suggestions.push("Add evidence links for more rules".into());
            }
        } else {
            suggestions.push("Add rule-level evidence to improve provenance".into());
        }

        FactorResult { score: score.clamp(0.0, 1.0), issues, suggestions }
    }

    /// Score safety based on anti-pattern and pitfall coverage
    fn score_safety(&self, skill: &Skill) -> FactorResult {
        let mut score = 0.5;  // Neutral if no data
        let mut issues = vec![];
        let mut suggestions = vec![];

        let pitfall_count = count_pitfalls(&skill.body);
        if pitfall_count == 0 {
            score -= 0.2;
            issues.push(QualityIssue::MissingAntiPatterns);
            suggestions.push("Add an 'Avoid / When NOT to use' section".into());
        } else if pitfall_count >= 2 {
            score += 0.2;
        }

        FactorResult { score: score.clamp(0.0, 1.0), issues, suggestions }
    }

    /// Score freshness and relevance
    fn score_freshness(&self, skill: &Skill) -> FactorResult {
        let mut score = 1.0;
        let mut issues = vec![];
        let mut suggestions = vec![];

        let now = Utc::now();
        let age = now - skill.metadata.updated_at;

        // Decay function: skills older than 6 months start losing points
        let months_old = age.num_days() as f32 / 30.0;
        if months_old > 6.0 {
            score -= 0.1 * ((months_old - 6.0) / 6.0).min(0.5);
        }

        // Check for deprecated patterns (version-specific content)
        let deprecated = check_deprecated_patterns(&skill.body);
        score -= 0.1 * deprecated.len() as f32;

        // Skill-level deprecation penalty
        if let Some(deprecation) = &skill.metadata.deprecated {
            score -= 0.4;
            issues.push(QualityIssue::DeprecatedSkill {
                replaced_by: deprecation.replaced_by.clone(),
            });
            if let Some(replacement) = &deprecation.replaced_by {
                suggestions.push(format!("Use '{}' instead of this deprecated skill", replacement));
            }
        }

        // Toolchain mismatch penalty (if project context available)
        if let Some(path) = &self.project_path {
            if let Ok(toolchain) = self.toolchain_detector.detect(path) {
                let mismatches = detect_toolchain_mismatches(skill, &toolchain);
                for mismatch in mismatches {
                    score -= 0.1;
                    issues.push(QualityIssue::ToolchainMismatch {
                        tool: mismatch.tool,
                        skill_range: mismatch.skill_range,
                        project_version: mismatch.project_version,
                    });
                }

                if !mismatches.is_empty() {
                    suggestions.push("Update skill for current toolchain versions".into());
                }
            }
        }

        FactorResult { score: score.clamp(0.0, 1.0), issues, suggestions }
    }

    /// Score popularity and community validation
    fn score_popularity(&self, skill: &Skill) -> FactorResult {
        let mut score = 0.5;  // Start neutral
        let issues = vec![];
        let suggestions = vec![];

        if let Some(usage) = self.usage_tracker.get_usage(&skill.id) {
            // Logarithmic scaling for usage count
            let log_uses = (usage.total_uses as f32 + 1.0).ln();
            score = (log_uses / 10.0).min(1.0);  // Cap at ~22,000 uses

            // Boost for recent activity
            if let Some(last_used) = usage.last_used {
                let days_since = (Utc::now() - last_used).num_days();
                if days_since < 7 {
                    score += 0.1;
                }
            }
        }

        // Bonus for being part of popular bundles
        if skill.bundle_count > 0 {
            score += 0.05 * (skill.bundle_count as f32).min(3.0);
        }

        FactorResult { score: score.clamp(0.0, 1.0), issues, suggestions }
    }
}
```

**Quality Issue Types:**

```rust
#[derive(Debug, Clone, Serialize)]
pub enum QualityIssue {
    /// Required field is missing
    MissingField(String),

    /// Content density is too low
    LowDensity(f32),

    /// Deprecated patterns detected
    DeprecatedPattern { pattern: String, replacement: Option<String> },

    /// Structure problem
    StructuralIssue(String),

    /// Conflicting guidance
    InternalConflict { section1: String, section2: String },

    /// Evidence coverage too low
    LowEvidenceCoverage(f32),

    /// Skill is deprecated
    DeprecatedSkill { replaced_by: Option<String> },

    /// Skill is incompatible with the project's toolchain versions
    ToolchainMismatch { tool: String, skill_range: String, project_version: String },

    /// No anti-patterns/pitfalls present
    MissingAntiPatterns,
}
```

**CLI Integration:**

```bash
# Show quality score for a skill
ms quality ntm
# → Overall: 0.87 (A)
# → Structure: 0.95 | Content: 0.90 | Effectiveness: 0.82 | Provenance: 0.88 | Safety: 0.70 | Fresh: 0.85 | Popular: 0.75

# Quality report with suggestions
ms quality ntm --verbose
# → ... detailed breakdown with suggestions ...

# Score all skills
ms quality --all --min=0.5
# → Lists skills below threshold with issues

# Quality gate for CI
ms quality --check --min=0.7
# → Exit 1 if any skill below threshold

# Staleness / toolchain drift report
ms quality --stale
ms quality --stale --project /data/projects/my-rust-project

# Quality as JSON
ms quality --robot ntm
```

**Quality-Based Filtering:**

```rust
/// Filter skills by minimum quality
pub fn filter_by_quality(
    skills: Vec<Skill>,
    min_score: f32,
    scorer: &QualityScorer,
) -> Vec<(Skill, QualityScore)> {
    skills
        .into_iter()
        .filter_map(|skill| {
            let score = scorer.score(&skill);
            if score.overall >= min_score {
                Some((skill, score))
            } else {
                None
            }
        })
        .collect()
}
```

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

```bash
# Dry run analysis
ms prune --scope skills --dry-run

# Emit BV beads for human review
ms prune --scope skills --emit-beads

# Apply only after explicit confirmation (no deletion)
ms prune --scope skills --apply --require-confirmation
```

## 8. Bundle & Distribution System

### 8.1 Bundle Format

```yaml
# bundle.yaml
name: rust-toolkit
version: 1.0.0
channel: stable
description: Essential skills for Rust development
author: Dicklesworthstone
license: MIT
homepage: https://github.com/Dicklesworthstone/rust-toolkit-skills

skills:
  - id: rust-async-patterns
    version: 1.2.0
    path: skills/rust-async-patterns/

  - id: rust-error-handling
    version: 1.1.0
    path: skills/rust-error-handling/

  - id: rust-testing
    version: 1.0.0
    path: skills/rust-testing/

dependencies:
  - bundle: core-cli-skills
    version: ">=2.0.0"

checksum: sha256:abc123...
signatures:
  - signer: "dicklesworthstone"
    key_id: "ed25519:abcd1234"
    signature: "base64:..."
```

### 8.2 GitHub Integration

```rust
/// Publish bundle to GitHub repository
pub async fn publish_to_github(
    bundle: &Bundle,
    config: &GitHubConfig,
) -> Result<PublishResult> {
    let gh = GitHubClient::new(&config.token)?;

    // Create or update repository
    let repo = if gh.repo_exists(&config.repo).await? {
        gh.get_repo(&config.repo).await?
    } else {
        gh.create_repo(&config.repo, &CreateRepoOptions {
            description: Some(&bundle.description),
            private: config.private,
            ..Default::default()
        }).await?
    };

    // Push bundle contents
    let tree = build_git_tree(bundle)?;
    let commit = gh.create_commit(&repo, &tree, "Update bundle").await?;
    gh.update_ref(&repo, "heads/main", &commit.sha).await?;

    // Create release
    let release = gh.create_release(&repo, &CreateReleaseOptions {
        tag_name: format!("v{}", bundle.version),
        name: format!("{} v{}", bundle.name, bundle.version),
        body: generate_release_notes(bundle),
        ..Default::default()
    }).await?;

    // Optionally sign bundle manifest
    if config.signing_enabled {
        sign_bundle_manifest(bundle, &config.signing_key)?;
    }

    Ok(PublishResult {
        repo_url: repo.html_url,
        release_url: release.html_url,
    })
}
```

### 8.3 Installation Flow

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        BUNDLE INSTALLATION FLOW                             │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  $ ms bundle install Dicklesworthstone/rust-toolkit-skills                  │
│                                                                             │
│  1. Resolve bundle location                                                 │
│     └─► GitHub API: GET /repos/Dicklesworthstone/rust-toolkit-skills        │
│     └─► Select release channel (stable/beta)                               │
│                                                                             │
│  2. Fetch bundle manifest                                                   │
│     └─► GET bundle.yaml from repo                                           │
│     └─► Verify checksum + signature                                         │
│                                                                             │
│  3. Check dependencies                                                      │
│     └─► Recursively resolve and install dependencies                        │
│                                                                             │
│  4. Download skills                                                         │
│     └─► Clone/pull repo to ~/.local/share/ms/bundles/                       │
│     └─► Extract skills to ~/.config/ms/skills/                              │
│                                                                             │
│  5. Index new skills                                                        │
│     └─► Update SQLite registry                                              │
│     └─► Rebuild search indexes                                              │
│                                                                             │
│  6. Report                                                                  │
│     └─► Show installed skills and any conflicts resolved                    │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 8.4 Sharing with Local Modification Safety

The sharing system allows one-URL distribution of all your skills while preserving local customizations when upstream changes arrive.

#### The Three-Tier Storage Model

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    THREE-TIER SKILL STORAGE                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ~/.local/share/ms/                                                         │
│  ├── upstream/                  # Pristine copies from remote              │
│  │   ├── bundle-a/                                                         │
│  │   │   ├── .git/              # Full git history for updates             │
│  │   │   ├── skill-1/SKILL.md                                              │
│  │   │   └── skill-2/SKILL.md                                              │
│  │   └── bundle-b/                                                         │
│  │                                                                          │
│  ├── local-mods/                # Your modifications ONLY (diffs)          │
│  │   ├── skill-1.patch          # patch -p1 compatible                     │
│  │   ├── skill-3.patch          # Your additions to upstream skills        │
│  │   └── custom-skill/          # Entirely local skills (no upstream)      │
│  │       └── SKILL.md                                                       │
│  │                                                                          │
│  └── merged/                    # Combined view (upstream + local mods)    │
│      ├── skill-1/SKILL.md       # Upstream + your patches applied          │
│      ├── skill-2/SKILL.md       # Pure upstream (no mods)                  │
│      ├── skill-3/SKILL.md       # Upstream + your patches applied          │
│      └── custom-skill/SKILL.md  # Pure local (no upstream)                 │
│                                                                             │
│  ~/.config/ms/skills/ → symlink to merged/                                  │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Local Modification Data Model

```rust
/// A modification to an upstream skill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LocalModification {
    /// The skill being modified
    pub skill_id: String,

    /// Bundle the upstream skill came from
    pub upstream_bundle: String,

    /// Upstream version when modification was made
    pub base_version: String,

    /// The patch content (unified diff format)
    pub patch: String,

    /// When the modification was created
    pub created_at: DateTime<Utc>,

    /// When the modification was last updated
    pub updated_at: DateTime<Utc>,

    /// User notes about why this modification exists
    pub reason: Option<String>,
}

/// Status of a skill relative to upstream
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SkillSyncStatus {
    /// Matches upstream exactly
    Clean,

    /// Has local modifications applied cleanly
    Modified {
        base_version: String,
        current_version: String,
    },

    /// Upstream changed, local mods may conflict
    NeedsRebase {
        base_version: String,
        upstream_version: String,
        conflicts: Vec<ConflictInfo>,
    },

    /// Pure local skill, no upstream
    LocalOnly,
}

/// Information about a merge conflict
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictInfo {
    pub section: String,
    pub upstream_change: String,
    pub local_change: String,
    pub suggested_resolution: Resolution,
}

pub enum Resolution {
    KeepUpstream,
    KeepLocal,
    Merge(String),  // Combined content
}
```

#### The Sync Engine

```rust
pub struct SyncEngine {
    upstream_dir: PathBuf,
    local_mods_dir: PathBuf,
    merged_dir: PathBuf,
    backup_dir: PathBuf,
}

impl SyncEngine {
    /// Sync upstream bundles and safely merge local modifications
    pub async fn sync(&self) -> Result<SyncReport> {
        let mut report = SyncReport::default();

        // 1. Backup local mods before any changes
        self.backup_local_mods()?;

        // 2. Update upstream repos
        for bundle in self.list_upstream_bundles()? {
            match self.update_upstream(&bundle).await {
                Ok(updates) => report.upstream_updates.push(updates),
                Err(e) => report.errors.push((bundle.clone(), e.to_string())),
            }
        }

        // 3. Reapply local modifications
        for skill in self.list_modified_skills()? {
            match self.reapply_modifications(&skill) {
                Ok(status) => report.skill_status.push((skill, status)),
                Err(e) => {
                    // Modification failed to apply - needs manual intervention
                    report.conflicts.push((skill.clone(), e.to_string()));
                    // Keep the backup version accessible
                    self.preserve_backup_version(&skill)?;
                }
            }
        }

        // 4. Rebuild merged directory
        self.rebuild_merged_dir()?;

        Ok(report)
    }

    /// Create a local modification for a skill
    pub fn create_modification(
        &self,
        skill_id: &str,
        new_content: &str,
        reason: Option<&str>,
    ) -> Result<LocalModification> {
        // Get upstream version
        let upstream = self.get_upstream_skill(skill_id)?;

        // Generate patch
        let patch = generate_unified_diff(&upstream.content, new_content)?;

        let modification = LocalModification {
            skill_id: skill_id.to_string(),
            upstream_bundle: upstream.bundle.clone(),
            base_version: upstream.version.clone(),
            patch,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            reason: reason.map(String::from),
        };

        // Save patch file
        self.save_modification(&modification)?;

        // Update merged directory
        self.apply_modification_to_merged(&modification)?;

        Ok(modification)
    }

    /// Backup all local modifications before sync
    fn backup_local_mods(&self) -> Result<PathBuf> {
        let backup_name = format!(
            "local-mods-backup-{}",
            Utc::now().format("%Y%m%d-%H%M%S")
        );
        let backup_path = self.backup_dir.join(&backup_name);

        // Copy entire local-mods directory
        copy_dir_all(&self.local_mods_dir, &backup_path)?;

        // Keep only last 10 backups
        self.prune_old_backups(10)?;

        Ok(backup_path)
    }

    /// Reapply a local modification after upstream update
    fn reapply_modifications(&self, skill_id: &str) -> Result<SkillSyncStatus> {
        let modification = self.load_modification(skill_id)?;
        let upstream = self.get_upstream_skill(skill_id)?;

        // Check if upstream version changed
        if upstream.version == modification.base_version {
            // No upstream change, just apply patch
            self.apply_patch(skill_id, &modification.patch)?;
            return Ok(SkillSyncStatus::Modified {
                base_version: modification.base_version.clone(),
                current_version: upstream.version.clone(),
            });
        }

        // Upstream changed - try to rebase patch
        match self.rebase_patch(&modification, &upstream) {
            Ok(rebased_patch) => {
                // Update modification with rebased patch
                let mut updated = modification.clone();
                updated.patch = rebased_patch;
                updated.base_version = upstream.version.clone();
                updated.updated_at = Utc::now();
                self.save_modification(&updated)?;

                self.apply_patch(skill_id, &updated.patch)?;
                Ok(SkillSyncStatus::Modified {
                    base_version: modification.base_version,
                    current_version: upstream.version,
                })
            }
            Err(conflicts) => {
                Ok(SkillSyncStatus::NeedsRebase {
                    base_version: modification.base_version,
                    upstream_version: upstream.version,
                    conflicts,
                })
            }
        }
    }
}
```

#### One-URL Sharing

Share all your skills (including local modifications) via a single URL:

```rust
/// Generate a shareable URL for all skills
pub async fn generate_share_url(
    skills: &[Skill],
    local_mods: &[LocalModification],
    config: &ShareConfig,
) -> Result<ShareUrl> {
    // Option 1: GitHub Gist (simple, no repo needed)
    if config.method == ShareMethod::Gist {
        let gist = GitHubGistClient::new(&config.token)?;

        let manifest = ShareManifest {
            version: "1.0".into(),
            created_at: Utc::now(),
            skills: skills.iter().map(|s| s.to_manifest_entry()).collect(),
            local_modifications: local_mods.clone(),
            upstream_sources: collect_upstream_sources(skills),
        };

        let gist_id = gist.create_or_update(
            "ms-skills-share",
            &serde_json::to_string_pretty(&manifest)?,
            config.private,
        ).await?;

        return Ok(ShareUrl::Gist(format!(
            "https://gist.github.com/{}/{}",
            config.username, gist_id
        )));
    }

    // Option 2: Dedicated GitHub repo
    if config.method == ShareMethod::Repository {
        let gh = GitHubClient::new(&config.token)?;

        // Ensure repo exists
        let repo = gh.ensure_repo(&config.repo_name, CreateRepoOptions {
            description: Some("My meta_skill skills collection"),
            private: config.private,
            ..Default::default()
        }).await?;

        // Push full skill contents + modifications
        let tree = build_share_tree(skills, local_mods)?;
        gh.push_tree(&repo, &tree, "Update skills").await?;

        return Ok(ShareUrl::Repository(repo.html_url));
    }

    // Option 3: JSON file export
    let export = SkillsExport {
        version: "1.0".into(),
        skills: skills.clone(),
        local_modifications: local_mods.clone(),
    };

    let json = serde_json::to_string_pretty(&export)?;
    let path = config.output_path.unwrap_or_else(|| PathBuf::from("ms-skills-export.json"));
    std::fs::write(&path, &json)?;

    Ok(ShareUrl::LocalFile(path))
}
```

**CLI Commands:**

```bash
# Share via GitHub Gist (simple)
ms share --gist
# → https://gist.github.com/username/abc123

# Share via dedicated repo
ms share --repo my-skills
# → https://github.com/username/my-skills

# Export to JSON file
ms share --export skills.json

# Import from share URL
ms import https://gist.github.com/Dicklesworthstone/abc123
ms import https://github.com/Dicklesworthstone/my-skills
ms import ./skills.json

# Show current share URL
ms share --status

# Auto-push changes to share location
ms share --auto-sync enable

# View what would be shared
ms share --dry-run
```

#### Sync Status Dashboard

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           SKILL SYNC STATUS                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  $ ms sync status                                                           │
│                                                                             │
│  Upstream Bundles (3)                                                       │
│  ├── Dicklesworthstone/rust-toolkit    v2.1.0 ✓ (up to date)              │
│  ├── Dicklesworthstone/go-patterns     v1.5.0 ⟳ (update available: 1.6.0) │
│  └── custom/my-team-skills             v3.0.0 ✓                            │
│                                                                             │
│  Skill Status (12 total)                                                    │
│  ├── 8 Clean (upstream only)                                               │
│  ├── 3 Modified (local changes)                                            │
│  │   ├── rust-async-patterns      +15/-3 lines                             │
│  │   ├── go-error-handling        +42/-0 lines (additions only)            │
│  │   └── git-workflow             +8/-8 lines                              │
│  └── 1 LocalOnly (no upstream)                                             │
│      └── my-custom-skill                                                   │
│                                                                             │
│  Local Modifications                                                        │
│  └── Last backup: 2026-01-13 14:32:00 (15 minutes ago)                     │
│                                                                             │
│  Share Status                                                               │
│  └── Auto-sync: enabled → gist:abc123 (last pushed: 2 hours ago)           │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Conflict Resolution Workflow

```bash
# Sync detects conflicts
$ ms sync
Syncing upstream bundles...
  ✓ rust-toolkit updated to v2.2.0
  ⚠ Conflict in rust-async-patterns

# View conflict details
$ ms sync conflict rust-async-patterns
Conflict in rust-async-patterns:

Section: ## Error Handling
  Upstream changed:
    - Use `anyhow::Result` for applications
    + Use `anyhow::Result` for applications
    + Use `thiserror::Error` for libraries

  Your modification:
    - Use `anyhow::Result` for applications
    + Use `eyre::Result` for better error reporting

Suggested resolution: Merge (combine both changes)

Options:
  1. Keep upstream (discard your modification)
  2. Keep local (ignore upstream changes for this section)
  3. Merge (combine: use thiserror for libraries, eyre for apps)
  4. Edit manually

# Resolve interactively
$ ms sync resolve rust-async-patterns --interactive

# Or auto-resolve with preference
$ ms sync resolve rust-async-patterns --prefer=local
$ ms sync resolve rust-async-patterns --prefer=upstream
$ ms sync resolve rust-async-patterns --prefer=merge
```

#### Automatic Backup Schedule

```rust
/// Backup configuration
pub struct BackupConfig {
    /// How many backups to keep
    pub retention_count: usize,

    /// Backup before every sync
    pub backup_on_sync: bool,

    /// Backup before every modification
    pub backup_on_modify: bool,

    /// Scheduled backup interval
    pub scheduled_interval: Option<Duration>,
}

impl Default for BackupConfig {
    fn default() -> Self {
        Self {
            retention_count: 10,
            backup_on_sync: true,
            backup_on_modify: true,
            scheduled_interval: None,  // Manual backups only by default
        }
    }
}
```

**Backup Commands:**

```bash
# Manual backup
ms backup create --reason "Before major changes"

# List backups
ms backup list
# → 2026-01-13-143200  Before major changes
# → 2026-01-13-120000  Pre-sync backup
# → 2026-01-12-180000  Pre-sync backup

# Restore from backup
ms backup restore 2026-01-13-120000

# Show diff from backup
ms backup diff 2026-01-13-143200
```

### 8.5 Multi-Machine Synchronization

Following the xf pattern for distributed archive access across multiple development machines.

**RU (repo_updater) Integration:**
- Use `/data/projects/repo_updater` as the repo‑level sync backend for skill sources.
- RU syncs GitHub repos; ms indexes/merges at the skill level.
- When RU finishes a sync, trigger `ms index` to refresh skills deterministically.
- Bead cross‑reference: `meta_skill-327` (RU integration) depends on
  `meta_skill-ujr` and `meta_skill-yu1` (blocks).

#### 8.5.1 Machine Identity

```rust
pub struct MachineIdentity {
    /// Unique identifier for this machine
    pub machine_id: String,
    /// Human-readable name (e.g., "workstation", "laptop", "server")
    pub machine_name: String,
    /// Last sync timestamp per remote
    pub sync_timestamps: HashMap<String, DateTime<Utc>>,
}

impl MachineIdentity {
    pub fn generate() -> Self {
        // Use stable machine fingerprint (hostname + some hardware IDs)
        let hostname = hostname::get().unwrap_or_default();
        let machine_id = format!("{}-{}", hostname.to_string_lossy(), uuid::Uuid::new_v4());

        Self {
            machine_id,
            machine_name: hostname.to_string_lossy().into_owned(),
            sync_timestamps: HashMap::new(),
        }
    }

    pub fn load_or_create(config_dir: &Path) -> Result<Self> {
        let identity_path = config_dir.join("machine_identity.json");

        if identity_path.exists() {
            let json = std::fs::read_to_string(&identity_path)?;
            Ok(serde_json::from_str(&json)?)
        } else {
            let identity = Self::generate();
            let json = serde_json::to_string_pretty(&identity)?;
            std::fs::write(&identity_path, json)?;
            Ok(identity)
        }
    }
}
```

#### 8.5.2 Sync State Tracking

```rust
pub struct SyncState {
    /// Per-skill sync metadata
    pub skill_states: HashMap<String, SkillSyncState>,
    /// Remote configurations
    pub remotes: Vec<RemoteConfig>,
    /// Last full sync per remote
    pub last_full_sync: HashMap<String, DateTime<Utc>>,
}

pub struct SkillSyncState {
    /// Skill ID
    pub skill_id: String,
    /// Local modification timestamp
    pub local_modified: DateTime<Utc>,
    /// Remote modification timestamps (per remote)
    pub remote_modified: HashMap<String, DateTime<Utc>>,
    /// Content hash for conflict detection
    pub content_hash: String,
    /// Sync status
    pub status: SkillSyncStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SkillSyncStatus {
    /// In sync with all remotes
    Synced,
    /// Local changes not yet pushed
    LocalAhead,
    /// Remote changes not yet pulled
    RemoteAhead,
    /// Both local and remote have changes (needs merge)
    Conflict,
    /// New skill not yet synced anywhere
    New,
    /// Skill only exists locally (not in any remote)
    LocalOnly,
}

pub struct RemoteConfig {
    /// Remote name (e.g., "origin", "github", "backup")
    pub name: String,
    /// Remote type
    pub remote_type: RemoteType,
    /// URL or path
    pub url: String,
    /// Whether to auto-sync on changes
    pub auto_sync: bool,
    /// Sync direction
    pub direction: SyncDirection,
}

#[derive(Debug, Clone)]
pub enum RemoteType {
    /// GitHub repository
    GitHub { owner: String, repo: String },
    /// Git repository (local or remote)
    Git { path: String },
    /// Network share or mounted drive
    FileSystem { path: PathBuf },
    /// Custom sync server
    Server { endpoint: String, api_key: Option<String> },
}

#[derive(Debug, Clone)]
pub enum SyncDirection {
    /// Only pull from remote
    PullOnly,
    /// Only push to remote
    PushOnly,
    /// Full bidirectional sync
    Bidirectional,
}
```

#### 8.5.3 Conflict Resolution

```rust
pub struct ConflictResolver {
    /// Default resolution strategy
    pub default_strategy: ConflictStrategy,
    /// Per-skill strategy overrides
    pub skill_strategies: HashMap<String, ConflictStrategy>,
}

#[derive(Debug, Clone)]
pub enum ConflictStrategy {
    /// Always prefer local version
    PreferLocal,
    /// Always prefer remote version
    PreferRemote,
    /// Prefer most recently modified
    PreferNewest,
    /// Keep both versions (rename one)
    KeepBoth,
    /// Interactive resolution required
    Interactive,
    /// Three-way merge (for text content)
    ThreeWayMerge,
}

pub struct ConflictInfo {
    pub skill_id: String,
    pub local_version: SkillVersion,
    pub remote_version: SkillVersion,
    pub base_version: Option<SkillVersion>,  // Common ancestor if known
    pub conflict_type: ConflictType,
}

#[derive(Debug, Clone)]
pub enum ConflictType {
    /// Both modified same sections
    ContentConflict,
    /// One side deleted, other modified
    DeleteModify,
    /// Both created skill with same ID
    CreateCreate,
    /// Metadata-only conflict (can auto-resolve)
    MetadataOnly,
}

pub struct SkillVersion {
    pub content_hash: String,
    pub modified_at: DateTime<Utc>,
    pub modified_by: String,  // Machine ID
    pub version_number: u64,
}

impl ConflictResolver {
    pub fn resolve(&self, conflict: &ConflictInfo) -> Result<Resolution> {
        let strategy = self.skill_strategies
            .get(&conflict.skill_id)
            .unwrap_or(&self.default_strategy);

        match strategy {
            ConflictStrategy::PreferLocal => Ok(Resolution::UseLocal),
            ConflictStrategy::PreferRemote => Ok(Resolution::UseRemote),
            ConflictStrategy::PreferNewest => {
                if conflict.local_version.modified_at > conflict.remote_version.modified_at {
                    Ok(Resolution::UseLocal)
                } else {
                    Ok(Resolution::UseRemote)
                }
            }
            ConflictStrategy::KeepBoth => {
                Ok(Resolution::KeepBoth {
                    local_suffix: format!("-{}", conflict.local_version.modified_by),
                    remote_suffix: "-remote".to_string(),
                })
            }
            ConflictStrategy::Interactive => {
                Err(Error::InteractiveResolutionRequired(conflict.clone()))
            }
            ConflictStrategy::ThreeWayMerge => {
                self.attempt_three_way_merge(conflict)
            }
        }
    }

    fn attempt_three_way_merge(&self, conflict: &ConflictInfo) -> Result<Resolution> {
        match &conflict.base_version {
            Some(base) => {
                // Use diff3-style merge
                let merged = three_way_merge(
                    &base.content_hash,
                    &conflict.local_version.content_hash,
                    &conflict.remote_version.content_hash,
                )?;

                if merged.has_conflicts {
                    // Merge has conflict markers, need interactive resolution
                    Err(Error::MergeConflicts(merged))
                } else {
                    Ok(Resolution::Merged(merged.content))
                }
            }
            None => {
                // No base version, fall back to interactive
                Err(Error::InteractiveResolutionRequired(conflict.clone()))
            }
        }
    }
}

pub enum Resolution {
    UseLocal,
    UseRemote,
    KeepBoth { local_suffix: String, remote_suffix: String },
    Merged(String),
}
```

#### 8.5.4 Sync Engine

```rust
pub struct SyncEngine {
    pub machine_identity: MachineIdentity,
    pub sync_state: SyncState,
    pub conflict_resolver: ConflictResolver,
    pub skills_db: Arc<SkillsDatabase>,
}

impl SyncEngine {
    /// Perform full sync with a remote
    pub async fn sync(&mut self, remote_name: &str) -> Result<SyncReport> {
        let remote = self.sync_state.remotes
            .iter()
            .find(|r| r.name == remote_name)
            .ok_or(Error::RemoteNotFound)?;

        let mut report = SyncReport::new(remote_name);

        // 1. Fetch remote state
        let remote_skills = self.fetch_remote_state(remote).await?;

        // 2. Compare with local state
        let changes = self.compute_changes(&remote_skills, remote)?;

        // 3. Handle each change type
        for change in changes {
            match change {
                SyncChange::Pull(skill_id) => {
                    self.pull_skill(&skill_id, remote).await?;
                    report.pulled.push(skill_id);
                }
                SyncChange::Push(skill_id) => {
                    if matches!(remote.direction, SyncDirection::PushOnly | SyncDirection::Bidirectional) {
                        self.push_skill(&skill_id, remote).await?;
                        report.pushed.push(skill_id);
                    }
                }
                SyncChange::Conflict(conflict_info) => {
                    match self.conflict_resolver.resolve(&conflict_info) {
                        Ok(resolution) => {
                            self.apply_resolution(&conflict_info, resolution, remote).await?;
                            report.resolved.push(conflict_info.skill_id);
                        }
                        Err(Error::InteractiveResolutionRequired(c)) => {
                            report.conflicts.push(c);
                        }
                        Err(e) => return Err(e),
                    }
                }
                SyncChange::Delete(skill_id, direction) => {
                    // Handle deletions based on sync config
                    self.handle_deletion(&skill_id, direction, remote).await?;
                    report.deleted.push(skill_id);
                }
            }
        }

        // 4. Update sync timestamps
        self.sync_state.last_full_sync.insert(
            remote_name.to_string(),
            Utc::now(),
        );

        Ok(report)
    }

    /// Quick sync - only check for changes since last sync
    pub async fn quick_sync(&mut self, remote_name: &str) -> Result<SyncReport> {
        let last_sync = self.sync_state.last_full_sync
            .get(remote_name)
            .copied()
            .unwrap_or(DateTime::UNIX_EPOCH);

        // Only fetch changes since last sync
        self.sync_since(remote_name, last_sync).await
    }

    /// Watch for changes and auto-sync
    pub async fn watch_and_sync(&mut self) -> Result<()> {
        use notify::{Watcher, RecursiveMode, watcher};
        use std::sync::mpsc::channel;

        let (tx, rx) = channel();
        let mut watcher = watcher(tx, Duration::from_secs(2))?;

        // Watch skills directories
        for dir in &self.get_skill_directories() {
            watcher.watch(dir, RecursiveMode::Recursive)?;
        }

        // Process changes
        loop {
            match rx.recv() {
                Ok(event) => {
                    if let Some(skill_id) = self.event_to_skill_id(&event) {
                        // Auto-sync to remotes with auto_sync enabled
                        for remote in &self.sync_state.remotes {
                            if remote.auto_sync {
                                self.sync_skill(&skill_id, &remote.name).await?;
                            }
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Watch error: {:?}", e);
                }
            }
        }
    }
}

pub struct SyncReport {
    pub remote: String,
    pub pulled: Vec<String>,
    pub pushed: Vec<String>,
    pub resolved: Vec<String>,
    pub conflicts: Vec<ConflictInfo>,
    pub deleted: Vec<String>,
    pub errors: Vec<String>,
    pub duration: Duration,
}

impl SyncReport {
    pub fn summary(&self) -> String {
        format!(
            "Sync with '{}': ↓{} ↑{} ⚡{} ⚠{} 🗑{}",
            self.remote,
            self.pulled.len(),
            self.pushed.len(),
            self.resolved.len(),
            self.conflicts.len(),
            self.deleted.len(),
        )
    }
}
```

#### 8.5.5 CLI Commands

```bash
# Add a remote
ms remote add origin https://github.com/user/skills.git --bidirectional
ms remote add backup /mnt/nas/skills --push-only
ms remote add work git@work.internal:team/skills.git --pull-only

# List remotes
ms remote list
# → origin    github   https://github.com/user/skills.git  bidirectional  auto-sync
# → backup    fs       /mnt/nas/skills                     push-only      manual
# → work      git      git@work.internal:team/skills.git   pull-only      auto-sync

# Remove a remote
ms remote remove backup

# Sync operations
ms sync                      # Sync all remotes
ms sync origin               # Sync specific remote
ms sync --quick              # Quick sync (changes since last sync)
ms sync --dry-run            # Show what would be synced

# Check sync status
ms sync status
# → origin:  ↓2 ↑5 ⚠1  Last sync: 2 hours ago
# → backup:  ✓ in sync  Last sync: 1 day ago
# → work:    ↓8        Last sync: 5 minutes ago

# Handle conflicts
ms conflicts list
# → skill-123  "react-patterns"  Local vs origin  Content conflict
# → skill-456  "git-workflow"    Local vs work    Both modified

ms conflicts show skill-123
# Shows diff between versions

ms conflicts resolve skill-123 --prefer-local
ms conflicts resolve skill-456 --prefer-remote
ms conflicts resolve skill-789 --merge  # Opens editor for manual merge

# Watch mode (auto-sync on changes)
ms sync watch
# → Watching for changes... (Ctrl+C to stop)
# → [14:32:15] Detected change: rust-patterns
# → [14:32:16] Pushed to origin: rust-patterns ✓

# Machine identity
ms machine info
# → Machine ID: workstation-a1b2c3d4
# → Machine Name: workstation
# → Last syncs:
# →   origin: 2026-01-13T14:30:00Z
# →   backup: 2026-01-12T18:00:00Z

ms machine rename "dev-laptop"
```

#### 8.5.6 Robot Mode for Multi-Machine

```bash
# Get sync status in JSON
ms --robot-sync-status
# {
#   "machine_id": "workstation-a1b2c3d4",
#   "remotes": [
#     {
#       "name": "origin",
#       "type": "github",
#       "url": "https://github.com/user/skills.git",
#       "direction": "bidirectional",
#       "auto_sync": true,
#       "last_sync": "2026-01-13T14:30:00Z",
#       "pending_push": 5,
#       "pending_pull": 2,
#       "conflicts": 1
#     }
#   ],
#   "overall_status": "has_conflicts"
# }

# Trigger sync via robot mode
ms --robot-sync --remote=origin
# {
#   "success": true,
#   "report": {
#     "remote": "origin",
#     "pulled": ["skill-abc", "skill-def"],
#     "pushed": ["skill-123", "skill-456", "skill-789"],
#     "resolved": [],
#     "conflicts": ["skill-999"],
#     "duration_ms": 2340
#   }
# }

# Get conflicts in JSON
ms --robot-conflicts
# {
#   "conflicts": [
#     {
#       "skill_id": "skill-999",
#       "skill_name": "react-patterns",
#       "remote": "origin",
#       "conflict_type": "content",
#       "local_modified": "2026-01-13T12:00:00Z",
#       "remote_modified": "2026-01-13T13:00:00Z",
#       "local_hash": "a1b2c3...",
#       "remote_hash": "d4e5f6..."
#     }
#   ]
# }

# Resolve via robot mode
ms --robot-resolve --skill=skill-999 --strategy=prefer-local
# {
#   "success": true,
#   "skill_id": "skill-999",
#   "resolution": "prefer_local",
#   "applied_at": "2026-01-13T14:35:00Z"
# }
```

#### 8.5.7 Sync Configuration

```toml
# ~/.config/ms/sync.toml

[machine]
name = "workstation"

[sync]
# Default conflict resolution
default_conflict_strategy = "prefer-newest"

# Auto-sync settings
auto_sync_on_change = true
auto_sync_interval_minutes = 30
sync_on_startup = true

# What to sync
sync_skills = true
sync_bundles = true
sync_config = false  # Don't sync machine-specific config

[ru]
# Repo Updater integration for multi-machine sync
enabled = true
binary = "ru"
sync_on_startup = true
sync_interval_minutes = 30

# After ru sync, trigger ms index to refresh registry
run_ms_index = true
index_scope = "all"

[remotes.origin]
url = "https://github.com/user/skills.git"
type = "github"
direction = "bidirectional"
auto_sync = true

[remotes.origin.auth]
method = "ssh"  # or "token", "oauth"

[remotes.backup]
url = "/mnt/nas/skills-backup"
type = "filesystem"
direction = "push-only"
auto_sync = false

[remotes.work]
url = "git@work.internal:team/shared-skills.git"
type = "git"
direction = "pull-only"
auto_sync = true

# Per-skill overrides
[skill_overrides."personal-workflow"]
# Never sync this skill
sync = false

[skill_overrides."team-standards"]
# Always prefer remote for this skill
conflict_strategy = "prefer-remote"
```

---

## 9. Auto-Update System (Following xf Pattern)

### 9.1 Update Check

```rust
pub struct Updater {
    current_version: Version,
    github_repo: String,  // "Dicklesworthstone/meta_skill"
    binary_name: String,
}

pub struct UpdateInfo {
    pub version: Version,
    pub download_url: Option<String>,
    pub release_notes: Option<String>,
    pub checksum_url: Option<String>,
    pub signature_url: Option<String>,
}

impl Updater {
    /// Check for new release on GitHub
    pub async fn check(&self) -> Result<Option<UpdateInfo>> {
        let url = format!(
            "https://api.github.com/repos/{}/releases/latest",
            self.github_repo
        );

        let response: GitHubRelease = reqwest::get(&url).await?.json().await?;
        let latest = Version::parse(&response.tag_name.trim_start_matches('v'))?;

        if latest > self.current_version {
            // Find binary for current platform
            let asset = response.assets.iter().find(|a| {
                a.name.contains(std::env::consts::OS) &&
                a.name.contains(std::env::consts::ARCH)
            });

            let signature_url = asset.and_then(|asset| {
                let sig_name = format!("{}.sig", asset.name);
                response.assets.iter()
                    .find(|a| a.name == sig_name)
                    .map(|a| a.browser_download_url.clone())
            });

            Ok(Some(UpdateInfo {
                version: latest,
                download_url: asset.map(|a| a.browser_download_url.clone()),
                release_notes: response.body,
                checksum_url: response.assets.iter()
                    .find(|a| a.name.ends_with(".sha256"))
                    .map(|a| a.browser_download_url.clone()),
                signature_url,
            }))
        } else {
            Ok(None)
        }
    }

    /// Download and install update
    pub async fn install(
        &self,
        info: &UpdateInfo,
        security: &SecurityConfig,
    ) -> Result<()> {
        let download_url = info.download_url.as_ref()
            .ok_or_else(|| anyhow!("No binary for this platform"))?;

        // Download binary
        println!("Downloading ms v{}...", info.version);
        let binary = reqwest::get(download_url).await?.bytes().await?;

        // Verify checksum if available
        if let Some(checksum_url) = &info.checksum_url {
            let expected = reqwest::get(checksum_url).await?.text().await?;
            let actual = sha256_hex(&binary);

            if !expected.contains(&actual) {
                return Err(anyhow!("Checksum mismatch!"));
            }
            println!("✓ Checksum verified");
        }

        // Verify signature if enabled
        if security.verify_updates {
            let signature_url = info.signature_url.as_ref()
                .ok_or_else(|| anyhow!("Missing signature for update"))?;
            let signature = reqwest::get(signature_url).await?.bytes().await?;
            verify_ed25519_signature(&binary, &signature, &security.trusted_keys)?;
            println!("✓ Signature verified");
        }

        // Replace current binary
        let current_exe = std::env::current_exe()?;
        let backup = current_exe.with_extension("old");

        std::fs::rename(&current_exe, &backup)?;
        std::fs::write(&current_exe, &binary)?;

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&current_exe, std::fs::Permissions::from_mode(0o755))?;
        }

        println!("✓ Updated to ms v{}", info.version);
        Ok(())
    }
}
```

### 9.2 Release Workflow

```yaml
# .github/workflows/release.yml
name: Release

on:
  push:
    tags:
      - 'v*'

jobs:
  build:
    strategy:
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
            artifact: ms-linux-x86_64
          - os: ubuntu-latest
            target: aarch64-unknown-linux-gnu
            artifact: ms-linux-aarch64
          - os: macos-latest
            target: x86_64-apple-darwin
            artifact: ms-macos-x86_64
          - os: macos-latest
            target: aarch64-apple-darwin
            artifact: ms-macos-aarch64

    runs-on: ${{ matrix.os }}

    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-action@stable
        with:
          targets: ${{ matrix.target }}

      - name: Build
        run: cargo build --release --target ${{ matrix.target }}

      - name: Package
        run: |
          cp target/${{ matrix.target }}/release/ms ${{ matrix.artifact }}
          sha256sum ${{ matrix.artifact }} > ${{ matrix.artifact }}.sha256

      - name: Sign
        run: |
          echo "${{ secrets.MS_SIGNING_KEY_PEM }}" > /tmp/ms_ed25519.pem
          openssl pkeyutl -sign -inkey /tmp/ms_ed25519.pem -rawin \
            -in ${{ matrix.artifact }} -out ${{ matrix.artifact }}.sig

      - name: Upload
        uses: softprops/action-gh-release@v2
        with:
          files: |
            ${{ matrix.artifact }}
            ${{ matrix.artifact }}.sha256
            ${{ matrix.artifact }}.sig
```

---

## 10. Configuration System

### 10.1 Config File Structure

```toml
# ~/.config/ms/config.toml

[general]
# Default disclosure level for suggestions
default_disclosure = "standard"

# Quality score threshold for suggestions
min_quality_score = 0.5

# Maximum skills to suggest at once
max_suggestions = 5

[compiler]
# Default compile target for SKILL.md
default_target = "generic-md"

# Block direct Markdown edits (spec-only workflow)
block_markdown_edits = true

[cache]
# Enable precompiled skillpack artifacts for low-latency load/suggest
skillpack_enabled = true

# Pack cache path (per-skill objects or monolithic file)
skillpack_path = ".ms/skillpack.bin"

[bandit]
# Adaptive weighting of suggestion signals
enabled = true

# Minimum samples before preferring a learned arm
min_samples = 50

[disclosure]
# Default token budget for packed disclosure (0 = disabled)
default_pack_budget = 800

# Packing mode: balanced | utility_first | coverage_first | pitfall_safe
default_pack_mode = "balanced"

# Max slices per coverage group (anti-bloat)
default_max_per_group = 2

[pack_contracts]
# Default contract for task types (optional)
debug = "DebugContract"
refactor = "RefactorContract"
deploy = "DeployContract"

[dependencies]
# Auto-load prerequisites on ms load (auto resolves order)
auto_load = true

# Default dependency load mode: auto | off | full | overview
default_mode = "auto"

# Default disclosure level for dependencies
default_level = "overview"

# Max dependency expansion depth
max_depth = 5

[layers]
# Layer precedence for conflict resolution: base < org < project < user
order = ["base", "org", "project", "user"]

# Conflict strategy: prefer_higher | prefer_lower | interactive
conflict_strategy = "prefer_higher"

# Merge strategy: auto | prefer_sections
merge_strategy = "auto"

# Section preferences when merge_strategy = "prefer_sections"
section_preference = { rules = "higher", pitfalls = "higher", examples = "lower", references = "lower" }

# If true, conflict details are emitted in --robot responses
emit_conflicts = true

[toolchain]
# Enable project toolchain detection for freshness scoring
detect_toolchain = true

# Projects to scan for toolchain info (first match wins)
project_roots = [
    ".",
    "..",
]

# Max allowed version drift (major version difference) before warning
max_major_drift = 1

[paths]
# Directories to scan for skills
skill_paths = [
    "~/.config/ms/skills",
    "~/.claude/skills",
    "/data/projects/agent_flywheel_clawdbot_skills_and_integrations/skills",
]

# Layered paths (override or augment skill_paths)
[paths.layers]
base = ["~/.config/ms/skills/base"]
org = ["~/.config/ms/skills/org"]
project = ["./.ms/skills"]
user = ["~/.config/ms/skills/user"]

# Exclude patterns
exclude_patterns = [
    "**/node_modules/**",
    "**/target/**",
    "**/.git/**",
]

[daemon]
# Optional single-writer daemon for cache + concurrency
enabled = false

[perf]
# Emit p50/p95/p99 latency in robot health output
emit_latency = true

[search]
# Default result limit
default_limit = 10

# RRF K parameter (higher = more equal weighting)
rrf_k = 60.0

# Embedding dimensions
embedding_dims = 384

[embeddings]
# Backend: hash | local
backend = "hash"

# Optional local model path (if backend=local)
model_path = "~/.local/share/ms/models/embeddings.onnx"

[cass]
# Path to CASS binary
binary = "cass"

# Default session limit for mining
default_session_limit = 50

# Minimum confidence for pattern extraction
min_pattern_confidence = 0.6

# Minimum session quality score (0.0 - 1.0)
min_session_quality = 0.5

# Incremental session scan (uses fingerprint cache)
incremental_scan = true

[cm]
# Path to cass-memory binary
binary = "cm"

# CM database path (for rule import/export)
db_path = "~/.local/share/cm/cm.db"

# Enable import/export bridge
import_enabled = true
export_enabled = true

[ubs]
# Path to UBS binary
binary = "ubs"

# Minimum severity to report in ms doctor
min_severity = "important"

# Report format: json | sarif
report_format = "json"

[ru]
# Path to repo_updater binary
binary = "ru"

# Enable RU integration for multi-machine sync
enabled = true

# Auto-sync cadence (minutes)
sync_interval_minutes = 30
sync_on_startup = true

# RU repo listing command (used to discover skill sources)
repo_list_cmd = "ru list --paths"

[generalization]
# Generalization engine: heuristic | llm (uses local model if available)
engine = "heuristic"

# If true, run critique round and push low-quality results to uncertainty queue
llm_critique = true

# Max time allowed for local refinement (ms)
llm_timeout_ms = 3000

[uncertainty]
# Enable uncertainty queue for low-confidence generalizations
enabled = true

# Confidence threshold below which patterns are queued
min_confidence = 0.7

# Max pending items before throttling
max_pending = 500

[prune]
# Usage window for pruning analysis (days)
window_days = 30

# Minimum uses within window to keep without review
min_uses = 5

# Similarity threshold for merge proposals
merge_similarity = 0.8

# Require explicit confirmation before applying any changes
require_confirmation = true

[privacy]
# Redaction enabled for all CASS ingestion
redaction_enabled = true

# Minimum entropy for secret-like tokens
redaction_min_entropy = 4.0

# Extra regex patterns to redact (in addition to built-ins)
redaction_patterns = [
    "(?i)api[_-]?key\\s*[:=]\\s*[A-Za-z0-9_-]{16,}",
    "(?i)secret\\s*[:=]\\s*[^\\s]+",
]

# Allowlist patterns that should not be redacted
redaction_allowlist = [
    "EXAMPLE_API_KEY",
    "TEST_TOKEN",
]

[safety]
# Detect prompt-injection content in sessions
prompt_injection_enabled = true

# ACIP integration for prompt injection defense
acip_enabled = true
acip_recommended_version = "1.3"
acip_audit_mode = false
acip_mode = "hybrid"  # direct | checker | hybrid

# DCG integration for destructive command guard
dcg_bin = "dcg"
dcg_packs = ["core", "git", "fs", "shell"]
dcg_explain_mode = true

# Regex patterns for prompt injection
prompt_injection_patterns = [
    "(?i)ignore previous instructions",
    "(?i)system prompt",
    "(?i)you are now",
    "(?i)override safety",
]

# Quarantine directory for flagged excerpts
quarantine_dir = "~/.local/share/ms/quarantine"

# Destructive operation policy: deny | confirm | allow
destructive_ops = "deny"

# Require verbatim approval for destructive ops
require_verbatim_approval = true

# Tombstone deletes in ms-managed dirs (never rm)
tombstone_deletes = true

[security]
# Verify bundle signatures on install/update
verify_bundles = true

# Verify ms self-update signatures
verify_updates = true

# Trusted signer public keys (bundles + updates)
trusted_keys = [
    "~/.config/ms/keys/dicklesworthstone.pub",
]

[build]
# Auto-save interval during interactive builds (seconds)
auto_save_interval = 60

# Maximum iterations before prompting for decision
max_iterations = 10

# Include anti-patterns/counter-examples in drafts
include_anti_patterns = true

[github]
# Default visibility for published bundles
default_visibility = "public"

# Auto-update check frequency (hours, 0 = disabled)
update_check_hours = 24

[display]
# Theme: auto, dark, light, plain
theme = "auto"

# Use icons (requires Nerd Font)
use_icons = true

# Color scheme: catppuccin, nord, dracula
color_scheme = "catppuccin"
```

### 10.2 Project-Local Config

```toml
# .ms/config.toml (in project root)

[project]
# Project-specific skill paths
skill_paths = [
    "./.claude/skills",
    "./docs/skills",
]

# Project-specific triggers
[triggers]
# Suggest rust-async-patterns when editing async code
"rust-async-patterns" = ["*.rs", "tokio", "async fn"]

# Suggest testing skill when in tests directory
"rust-testing" = ["tests/**", "#[test]"]
```

---

## 11. Implementation Phases

### Phase 1: Foundation

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ PHASE 1: FOUNDATION                                                         │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│ ☐ Project scaffold (cargo new, directory structure)                        │
│ ☐ CLI framework with clap (following xf pattern)                           │
│ ☐ SQLite storage layer with migrations                                     │
│ ☐ Git archive layer (following mcp_agent_mail pattern)                     │
│ ☐ Skill struct and YAML/Markdown parsing                                   │
│ ☐ Alias + deprecation metadata support                                     │
│ ☐ Basic CRUD operations (index, show, list)                                │
│ ☐ Robot mode output formatting                                             │
│ ☐ Config system (TOML parsing, paths)                                      │
│                                                                             │
│ Deliverable: ms index, ms list, ms show work                               │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Phase 2: Search

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ PHASE 2: SEARCH                                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│ ☐ Tantivy integration for full-text search                                 │
│ ☐ Hash-based embeddings (from xf)                                          │
│ ☐ Vector storage and similarity search                                     │
│ ☐ RRF hybrid fusion                                                        │
│ ☐ Search filters (tags, quality, etc.)                                     │
│ ☐ Search result ranking and formatting                                     │
│                                                                             │
│ Deliverable: ms search works with hybrid ranking                           │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Phase 3: Disclosure & Suggestions

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ PHASE 3: DISCLOSURE & SUGGESTIONS                                           │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│ ☐ Disclosure level system                                                  │
│ ☐ Token counting and budget management                                     │
│ ☐ Content truncation strategies                                            │
│ ☐ Context analysis (directory, files, commands)                            │
│ ☐ Trigger matching system                                                  │
│ ☐ Suggestion ranking with context boosting                                 │
│ ☐ Requirement-aware suggestions (platform/tools/env gating)                │
│ ☐ Collective pack planning for swarms (NTM)                                │
│ ☐ Usage tracking                                                           │
│                                                                             │
│ Deliverable: ms load, ms suggest work                                      │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Phase 4: CASS Integration

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ PHASE 4: CASS INTEGRATION                                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│ ☐ CASS client (subprocess, JSON parsing)                                   │
│ ☐ Session fetching and parsing                                             │
│ ☐ Pattern extraction algorithms                                            │
│ ☐ Pattern clustering and deduplication                                     │
│ ☐ Draft generation from patterns                                           │
│ ☐ Interactive build session (TUI)                                          │
│ ☐ Iterative refinement loop                                                │
│ ☐ Build session persistence                                                │
│                                                                             │
│ Deliverable: ms build --from-cass works                                    │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Phase 5: Bundles & Distribution

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ PHASE 5: BUNDLES & DISTRIBUTION                                             │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│ ☐ Bundle manifest format                                                   │
│ ☐ Bundle creation (packaging skills)                                       │
│ ☐ GitHub API integration                                                   │
│ ☐ Bundle publishing workflow                                               │
│ ☐ Bundle installation from GitHub                                          │
│ ☐ Dependency resolution                                                    │
│ ☐ Bundle update checking                                                   │
│                                                                             │
│ Deliverable: ms bundle create/publish/install work                         │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Phase 6: Polish & Auto-Update

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ PHASE 6: POLISH & AUTO-UPDATE                                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│ ☐ Auto-update system (following xf)                                        │
│ ☐ Doctor command (health checks)                                           │
│ ☐ Stats and analytics                                                      │
│ ☐ TUI polish (colors, icons, animations)                                   │
│ ☐ Comprehensive error messages                                             │
│ ☐ Shell completions                                                        │
│ ☐ Man pages                                                                │
│ ☐ CI/CD pipeline                                                           │
│ ☐ Cross-platform testing                                                   │
│                                                                             │
│ Deliverable: Production-ready release                                      │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Reordered Phasing (Hard Invariants First):**
1. Spec-only editing + compilation + semantic diff
2. Index + skillpack artifacts + fast suggest/load
3. Provenance compression + taint/reassembly resistance
4. Mining pipeline + Pattern IR
5. Swarm orchestration + bandit scoring
6. TUI polish + bundles + auto-update

---

## 12. Dependencies (Cargo.toml)

```toml
[package]
name = "meta_skill"
version = "0.1.0"
edition = "2024"
rust-version = "1.85"
authors = ["Dicklesworthstone"]
description = "Skill management and generation CLI for AI coding agents"
license = "MIT"
repository = "https://github.com/Dicklesworthstone/meta_skill"

[[bin]]
name = "ms"
path = "src/main.rs"

[dependencies]
# CLI framework
clap = { version = "4.5", features = ["derive", "env", "wrap_help"] }

# Async runtime
tokio = { version = "1.43", features = ["full"] }

# Serialization
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
serde_yaml = "0.9"
toml = "0.8"

# Database
rusqlite = { version = "0.32", features = ["bundled", "vtab", "array"] }

# Full-text search
tantivy = "0.22"

# Git operations
gix = { version = "0.67", features = ["blocking-network-client"] }

# HTTP client
reqwest = { version = "0.12", features = ["json", "rustls-tls"] }

# Error handling
anyhow = "1.0"
thiserror = "2.0"

# Logging
tracing = "0.1"
tracing-subscriber = { version = "0.3", features = ["env-filter"] }

# Terminal UI
crossterm = "0.28"
ratatui = "0.29"
indicatif = "0.17"

# Utilities
chrono = { version = "0.4", features = ["serde"] }
directories = "5.0"
glob = "0.3"
regex = "1.11"
sha2 = "0.10"
semver = { version = "1.0", features = ["serde"] }
uuid = { version = "1.11", features = ["v4", "serde"] }

# Markdown parsing
pulldown-cmark = "0.12"

[dev-dependencies]
tempfile = "3.14"
assert_cmd = "2.0"
predicates = "3.1"

[profile.release]
lto = true
codegen-units = 1
strip = true
panic = "abort"
```

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

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    DUAL PERSISTENCE RATIONALE                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│ SQLite gives us:                     Git gives us:                          │
│ • Fast queries (FTS5, indexes)       • Human-readable history              │
│ • ACID transactions                  • Blame/audit trail                   │
│ • Complex joins and aggregations     • Branch/merge for collaboration      │
│ • Efficient storage                  • Familiar tooling (git log, etc.)    │
│ • Concurrent readers                 • Natural backup/sync                 │
│                                                                             │
│ Combined: Best of both worlds                                               │
│ • Query performance when needed                                             │
│ • Human auditability always                                                 │
│ • Graceful degradation (can rebuild SQLite from Git)                       │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Two-Phase Commit for Consistency**

To avoid partial writes (SQLite updated but Git not, or vice versa), ms wraps every
write in a two-phase commit (2PC) protocol with a durable write-ahead record.

```
Phase 1 (Prepare)
  1) Write pending change to .ms/tx/<txid>.json
  2) Write to SQLite in a "pending" state

Phase 2 (Commit)
  3) Write to Git archive (commit)
  4) Mark SQLite row as "committed"
  5) Remove tx record

Recovery
  - If tx exists without Git commit: resume commit
  - If Git commit exists but SQLite pending: finalize SQLite
```

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

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        FLYWHEEL INTEGRATION                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────┐     ┌─────────┐     ┌─────────┐     ┌─────────┐              │
│  │   NTM   │────►│   MS    │────►│  CASS   │────►│   BV    │              │
│  │ (spawn) │     │ (skill) │     │ (mine)  │     │ (work)  │              │
│  └─────────┘     └─────────┘     └─────────┘     └─────────┘              │
│       │               │               │               │                    │
│       └───────────────┴───────────────┴───────────────┘                    │
│                               │                                             │
│                        ┌──────┴──────┐                                      │
│                        │ Agent Mail  │                                      │
│                        │ (coordinate)│                                      │
│                        └─────────────┘                                      │
│                                                                             │
│  Flow:                                                                      │
│  1. NTM spawns agents with skills loaded from MS                           │
│  2. Agents work on BV beads                                                │
│  3. Sessions are indexed by CASS                                           │
│  4. MS mines CASS to generate new skills                                   │
│  5. Agent Mail coordinates multi-agent skill sharing                       │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

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

```
You are analyzing coding session transcripts to extract reusable patterns.

For each session, identify:
1. COMMAND PATTERNS: Sequences of commands that solve a problem
2. CODE PATTERNS: Reusable code snippets with clear purposes
3. WORKFLOW PATTERNS: Step-by-step processes that work
4. CONSTRAINT PATTERNS: Rules that are emphasized (especially "NEVER" and "ALWAYS")
5. ERROR RESOLUTION PATTERNS: Errors encountered and how they were fixed

For each pattern, note:
- Frequency: How many times it appears across sessions
- Context: When/where it's typically used
- Effectiveness: Any signals about whether it worked well

Focus on patterns that:
- Appear multiple times
- Have clear start/end boundaries
- Are self-contained and reusable
- Include sufficient context to understand

Output as JSON with the schema provided.
```

### 16.2 Draft Generation Prompt

```
You are generating a SKILL.md file from extracted patterns.

The skill should follow this exact structure:
1. YAML frontmatter with name, description, tags
2. Brief overview (2-3 sentences)
3. ⚠️ CRITICAL RULES section if any constraint patterns exist
4. Core content organized by topic
5. Examples section with real code
6. Common mistakes/troubleshooting if error patterns exist

Guidelines:
- Be token-dense but not cryptic
- Use tables for structured information
- Include "THE EXACT PROMPT" sections for reusable prompts
- Add decision trees for complex workflows
- Never use generic filler text

Input patterns:
{patterns_json}

Generate the complete SKILL.md content.
```

### 16.3 Refinement Prompt

```
The user has provided feedback on the skill draft:

{feedback}

Current draft:
{current_draft}

Available patterns not yet used:
{unused_patterns}

Instructions:
1. Address ALL feedback points specifically
2. Incorporate relevant unused patterns if they help
3. Maintain the existing structure unless feedback requests changes
4. Keep token density high
5. Do not add generic content to fill space

Output the complete updated SKILL.md.
```

---

## 17. Getting Started

```bash
# Clone the repository
git clone https://github.com/Dicklesworthstone/meta_skill.git
cd meta_skill

# Build in release mode
cargo build --release

# Install locally
cargo install --path .

# Initialize
ms init --global

# Index your existing skills
ms index --path /data/projects/agent_flywheel_clawdbot_skills_and_integrations/skills

# Start building a skill from your sessions
ms build --name my-first-skill

# Search for skills
ms search "error handling"

# Get skill suggestions
ms suggest
```

---

## 18. Testing Strategy

### 18.1 Testing Philosophy

Following Rust best practices with comprehensive coverage across unit, integration, and property-based tests.

**UBS (Ultimate Bug Scanner) Integration:**
- Integrate `/data/projects/ultimate_bug_scanner` as a required quality gate.
- Run `ubs` on changed files before commits and during CI; surface findings in `ms doctor`.
- Prefer machine-readable outputs (JSON/SARIF) for automation and bead creation.

**UBS Data Model:**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UbsConfig {
    pub ubs_bin: PathBuf,
    pub min_severity: String,
    pub report_format: String, // json | sarif
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UbsFinding {
    pub file: String,
    pub line: u32,
    pub column: u32,
    pub category: String,
    pub message: String,
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UbsReport {
    pub exit_code: i32,
    pub findings: Vec<UbsFinding>,
}
```

**Testing Beads Coverage:**
- Create dedicated beads for unit tests, integration tests, E2E scripts, and benchmarks.
- Treat testing beads as first-class blockers in planning/triage.

```rust
// Testing configuration
// Cargo.toml
[dev-dependencies]
tempfile = "3.10"
assert_cmd = "2.0"
predicates = "3.1"
proptest = "1.4"
criterion = "0.5"
insta = "1.34"       # Snapshot testing
wiremock = "0.5"     # HTTP mocking
fake = "2.9"         # Fake data generation
test-log = "0.2"     # Logging in tests
rstest = "0.18"      # Parameterized tests
```

### 18.2 Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    // 18.2.1 Hash Embedding Tests
    mod hash_embedding {
        use super::*;

        #[test]
        fn test_fnv1a_deterministic() {
            let hasher = FnvHasher::new();
            let hash1 = hasher.hash_term("test");
            let hash2 = hasher.hash_term("test");
            assert_eq!(hash1, hash2);
        }

        #[test]
        fn test_embedding_dimensions() {
            let generator = HashEmbeddingGenerator::new(384);
            let text = "This is a test document";
            let embedding = generator.generate(text);
            assert_eq!(embedding.len(), 384);
        }

        #[test]
        fn test_embedding_normalization() {
            let generator = HashEmbeddingGenerator::new(384);
            let embedding = generator.generate("test");
            let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
            assert!((magnitude - 1.0).abs() < 0.001);  // L2 normalized
        }

        #[test]
        fn test_similar_texts_similar_embeddings() {
            let generator = HashEmbeddingGenerator::new(384);
            let emb1 = generator.generate("rust error handling patterns");
            let emb2 = generator.generate("rust error patterns and handling");
            let emb3 = generator.generate("python web frameworks");

            let sim_12 = cosine_similarity(&emb1, &emb2);
            let sim_13 = cosine_similarity(&emb1, &emb3);

            assert!(sim_12 > sim_13, "Similar texts should have higher similarity");
        }
    }

    // 18.2.2 Skill Parser Tests
    mod skill_parser {
        use super::*;
        use rstest::rstest;

        #[rstest]
        #[case("simple-skill", true)]
        #[case("skill-with-dashes", true)]
        #[case("skill_with_underscores", true)]
        #[case("", false)]
        #[case("skill with spaces", false)]
        #[case("../path/traversal", false)]
        fn test_skill_name_validation(#[case] name: &str, #[case] valid: bool) {
            assert_eq!(is_valid_skill_name(name), valid);
        }

        #[test]
        fn test_frontmatter_parsing() {
            let content = r#"---
name: test-skill
description: A test skill
---

# Test Skill Content
"#;
            let skill = SkillParser::parse(content).unwrap();
            assert_eq!(skill.name, "test-skill");
            assert_eq!(skill.description, "A test skill");
        }

        #[test]
        fn test_malformed_frontmatter() {
            let content = r#"---
name: test-skill
invalid yaml {{{{
---
"#;
            let result = SkillParser::parse(content);
            assert!(result.is_err());
            assert!(matches!(result.unwrap_err(), Error::YamlParsing(_)));
        }

        #[test]
        fn test_missing_required_fields() {
            let content = r#"---
description: Missing name field
---
"#;
            let result = SkillParser::parse(content);
            assert!(result.is_err());
            assert!(matches!(result.unwrap_err(), Error::MissingField("name")));
        }
    }

    // 18.2.3 Search Index Tests
    mod search_index {
        use super::*;
        use tempfile::tempdir;

        #[test]
        fn test_index_and_search() {
            let dir = tempdir().unwrap();
            let index = SearchIndex::create(dir.path()).unwrap();

            let skill = Skill {
                id: "test-1".to_string(),
                name: "rust-patterns".to_string(),
                description: "Common Rust programming patterns".to_string(),
                content: "Error handling with Result type...".to_string(),
                ..Default::default()
            };

            index.add(&skill).unwrap();
            index.commit().unwrap();

            let results = index.search("rust error", 10).unwrap();
            assert_eq!(results.len(), 1);
            assert_eq!(results[0].skill_id, "test-1");
        }

        #[test]
        fn test_rrf_fusion() {
            let bm25_ranks = vec![
                ("a", 1.0), ("b", 0.8), ("c", 0.6), ("d", 0.4),
            ];
            let vector_ranks = vec![
                ("c", 1.0), ("a", 0.9), ("e", 0.7), ("b", 0.5),
            ];

            let fused = rrf_fusion(&[bm25_ranks, vector_ranks], 60);

            // "a" and "c" should rank highest (appear in both)
            assert!(fused[0].0 == "a" || fused[0].0 == "c");
            assert!(fused[1].0 == "a" || fused[1].0 == "c");
        }
    }

    // 18.2.4 Quality Scorer Tests
    mod quality_scorer {
        use super::*;

        #[test]
        fn test_structure_score() {
            let scorer = QualityScorer::default();

            // Well-structured skill
            let good_skill = Skill {
                content: r#"---
name: good-skill
description: Well structured skill
---

# Good Skill

## Overview
Content here...

## Usage
More content...

```bash
example code
```
"#.to_string(),
                ..Default::default()
            };

            let structure_score = scorer.score_structure(&good_skill);
            assert!(structure_score > 0.7);
        }

        #[test]
        fn test_content_density() {
            let scorer = QualityScorer::default();

            // High-density content (minimal fluff)
            let dense = "Implement rate limiting using token bucket algorithm. \
                         Configure max_tokens and refill_rate parameters.";

            // Low-density content (lots of filler)
            let fluffy = "This is a really great and amazing skill that will help \
                          you do things in a very nice and wonderful way.";

            assert!(scorer.measure_density(dense) > scorer.measure_density(fluffy));
        }
    }
}
```

### 18.3 Integration Tests

```rust
// tests/integration/cli.rs
use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::tempdir;

#[test]
fn test_init_creates_config() {
    let dir = tempdir().unwrap();
    let config_dir = dir.path().join(".config/ms");

    Command::cargo_bin("ms")
        .unwrap()
        .args(["init", "--config-dir", config_dir.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Initialized"));

    assert!(config_dir.join("config.toml").exists());
    assert!(config_dir.join("ms.db").exists());
}

#[test]
fn test_index_skills_directory() {
    let dir = tempdir().unwrap();
    let skills_dir = dir.path().join("skills");
    std::fs::create_dir_all(&skills_dir).unwrap();

    // Create test skill
    let skill_content = r#"---
name: test-skill
description: Test skill for integration testing
---

# Test Skill
"#;
    std::fs::write(skills_dir.join("test-skill/SKILL.md"), skill_content).unwrap();

    Command::cargo_bin("ms")
        .unwrap()
        .args(["index", "--path", skills_dir.to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("Indexed 1 skill"));
}

#[test]
fn test_search_returns_results() {
    let fixture = TestFixture::with_indexed_skills(&[
        ("rust-patterns", "Common Rust programming patterns"),
        ("python-testing", "Python testing best practices"),
        ("go-concurrency", "Go concurrency patterns"),
    ]);

    Command::cargo_bin("ms")
        .unwrap()
        .args(["search", "rust programming"])
        .env("MS_CONFIG_DIR", fixture.config_dir())
        .assert()
        .success()
        .stdout(predicate::str::contains("rust-patterns"));
}

#[test]
fn test_robot_mode_json_output() {
    let fixture = TestFixture::new();

    Command::cargo_bin("ms")
        .unwrap()
        .args(["--robot-status"])
        .env("MS_CONFIG_DIR", fixture.config_dir())
        .assert()
        .success()
        .stdout(predicate::str::is_json());
}

#[test]
fn test_suggest_with_cass_integration() {
    let fixture = TestFixture::with_mock_cass();

    Command::cargo_bin("ms")
        .unwrap()
        .args(["suggest", "--limit", "5"])
        .env("MS_CONFIG_DIR", fixture.config_dir())
        .assert()
        .success();
}
```

### 18.4 Property-Based Tests

```rust
// tests/property.rs
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_skill_id_generation_unique(
        names in prop::collection::vec("[a-z][a-z0-9-]{2,30}", 100)
    ) {
        let ids: Vec<String> = names.iter()
            .map(|n| generate_skill_id(n))
            .collect();

        let unique_ids: std::collections::HashSet<_> = ids.iter().collect();
        assert_eq!(ids.len(), unique_ids.len(), "All IDs should be unique");
    }

    #[test]
    fn test_embedding_always_normalized(text in "\\PC{1,1000}") {
        let generator = HashEmbeddingGenerator::new(384);
        let embedding = generator.generate(&text);

        let magnitude: f32 = embedding.iter().map(|x| x * x).sum::<f32>().sqrt();
        prop_assert!((magnitude - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_search_never_panics(query in "\\PC{0,100}") {
        let index = TestFixture::search_index();
        // Should return empty vec for any input, never panic
        let _ = index.search(&query, 10);
    }

    #[test]
    fn test_rrf_order_independent(
        list1 in prop::collection::vec(("[a-z]{3}", 0.0f32..1.0), 0..20),
        list2 in prop::collection::vec(("[a-z]{3}", 0.0f32..1.0), 0..20),
    ) {
        let result1 = rrf_fusion(&[list1.clone(), list2.clone()], 60);
        let result2 = rrf_fusion(&[list2, list1], 60);

        // Same documents should appear (order may differ based on tie-breaking)
        let docs1: std::collections::HashSet<_> = result1.iter().map(|(d, _)| d).collect();
        let docs2: std::collections::HashSet<_> = result2.iter().map(|(d, _)| d).collect();
        prop_assert_eq!(docs1, docs2);
    }

    #[test]
    fn test_yaml_roundtrip(
        name in "[a-z][a-z0-9-]{2,20}",
        description in "\\PC{10,200}",
    ) {
        let skill = SkillMetadata {
            name: name.clone(),
            description: description.clone(),
            ..Default::default()
        };

        let yaml = serde_yaml::to_string(&skill).unwrap();
        let parsed: SkillMetadata = serde_yaml::from_str(&yaml).unwrap();

        prop_assert_eq!(skill.name, parsed.name);
        prop_assert_eq!(skill.description, parsed.description);
    }
}
```

### 18.5 Snapshot Tests

```rust
// tests/snapshots.rs
use insta::assert_snapshot;

#[test]
fn test_skill_disclosure_minimal() {
    let skill = test_fixtures::sample_skill();
    let disclosed = skill.disclose(DisclosureLevel::Minimal);
    assert_snapshot!(disclosed);
}

#[test]
fn test_skill_disclosure_full() {
    let skill = test_fixtures::sample_skill();
    let disclosed = skill.disclose(DisclosureLevel::Full);
    assert_snapshot!(disclosed);
}

#[test]
fn test_robot_status_output() {
    let state = test_fixtures::sample_state();
    let output = RobotOutput::status(&state);
    assert_snapshot!(serde_json::to_string_pretty(&output).unwrap());
}

#[test]
fn test_doctor_report_format() {
    let report = test_fixtures::sample_doctor_report();
    assert_snapshot!(report.human_format());
}

#[test]
fn test_search_results_format() {
    let results = test_fixtures::sample_search_results();
    assert_snapshot!(format_search_results(&results, OutputFormat::Table));
}
```

### 18.6 Benchmark Tests

```rust
// benches/benchmarks.rs
use criterion::{criterion_group, criterion_main, Criterion, BenchmarkId};

fn bench_hash_embedding(c: &mut Criterion) {
    let generator = HashEmbeddingGenerator::new(384);
    let texts = vec![
        "short",
        "This is a medium length text for embedding",
        &"word ".repeat(1000),  // Long text
    ];

    let mut group = c.benchmark_group("hash_embedding");
    for (i, text) in texts.iter().enumerate() {
        group.bench_with_input(
            BenchmarkId::new("generate", format!("len_{}", text.len())),
            text,
            |b, t| b.iter(|| generator.generate(t)),
        );
    }
    group.finish();
}

fn bench_search(c: &mut Criterion) {
    let fixture = TestFixture::with_skills(1000);

    let mut group = c.benchmark_group("search");
    group.bench_function("single_term", |b| {
        b.iter(|| fixture.index.search("rust", 10))
    });
    group.bench_function("phrase", |b| {
        b.iter(|| fixture.index.search("error handling patterns", 10))
    });
    group.bench_function("complex_query", |b| {
        b.iter(|| fixture.index.search("rust OR go AND patterns NOT deprecated", 10))
    });
    group.finish();
}

fn bench_rrf_fusion(c: &mut Criterion) {
    let mut group = c.benchmark_group("rrf_fusion");

    for size in [10, 100, 1000] {
        let lists = generate_ranked_lists(size);
        group.bench_with_input(
            BenchmarkId::new("fuse", size),
            &lists,
            |b, l| b.iter(|| rrf_fusion(l, 60)),
        );
    }
    group.finish();
}

fn bench_skill_parsing(c: &mut Criterion) {
    let skills = test_fixtures::various_skill_contents();

    let mut group = c.benchmark_group("skill_parsing");
    for (name, content) in skills {
        group.bench_with_input(
            BenchmarkId::new("parse", name),
            &content,
            |b, c| b.iter(|| SkillParser::parse(c)),
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_hash_embedding,
    bench_search,
    bench_rrf_fusion,
    bench_skill_parsing,
);
criterion_main!(benches);
```

### 18.7 Test Fixtures and Helpers

```rust
// tests/common/mod.rs
pub struct TestFixture {
    pub temp_dir: TempDir,
    pub config_dir: PathBuf,
    pub skills_dir: PathBuf,
    pub db: SkillsDatabase,
    pub index: SearchIndex,
}

impl TestFixture {
    pub fn new() -> Self {
        let temp_dir = tempdir().unwrap();
        let config_dir = temp_dir.path().join("config");
        let skills_dir = temp_dir.path().join("skills");

        std::fs::create_dir_all(&config_dir).unwrap();
        std::fs::create_dir_all(&skills_dir).unwrap();

        let db = SkillsDatabase::create(config_dir.join("ms.db")).unwrap();
        let index = SearchIndex::create(config_dir.join("index")).unwrap();

        Self { temp_dir, config_dir, skills_dir, db, index }
    }

    pub fn with_indexed_skills(skills: &[(&str, &str)]) -> Self {
        let fixture = Self::new();

        for (name, description) in skills {
            let skill_dir = fixture.skills_dir.join(name);
            std::fs::create_dir_all(&skill_dir).unwrap();

            let content = format!(
                "---\nname: {}\ndescription: {}\n---\n\n# {}\n",
                name, description, name
            );
            std::fs::write(skill_dir.join("SKILL.md"), content).unwrap();

            let skill = Skill {
                id: generate_skill_id(name),
                name: name.to_string(),
                description: description.to_string(),
                ..Default::default()
            };
            fixture.db.insert(&skill).unwrap();
            fixture.index.add(&skill).unwrap();
        }
        fixture.index.commit().unwrap();

        fixture
    }

    pub fn with_mock_cass() -> Self {
        let fixture = Self::new();

        // Set up mock CASS environment
        std::env::set_var("CASS_DB_PATH", fixture.temp_dir.path().join("cass.db"));

        // Create mock session data
        let mock_sessions = vec![
            MockSession {
                id: "session-1",
                agent: "claude-code",
                project: "/data/projects/test",
                messages: vec![
                    ("user", "Fix the authentication bug"),
                    ("assistant", "I'll look into the auth module..."),
                ],
            },
        ];

        MockCass::setup(&fixture.temp_dir, &mock_sessions);

        fixture
    }

    pub fn config_dir(&self) -> &str {
        self.config_dir.to_str().unwrap()
    }
}

impl Drop for TestFixture {
    fn drop(&mut self) {
        // Cleanup is automatic via TempDir
    }
}
```

### 18.8 CI Integration

```yaml
# .github/workflows/test.yml
name: Test

on:
  push:
    branches: [main]
  pull_request:

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-action@stable
        with:
          components: llvm-tools-preview

      - name: Cache cargo
        uses: Swatinem/rust-cache@v2

      - name: Run tests
        run: cargo test --all-features

      - name: Run doc tests
        run: cargo test --doc

  coverage:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-action@stable
        with:
          components: llvm-tools-preview

      - name: Install cargo-llvm-cov
        uses: taiki-e/install-action@cargo-llvm-cov

      - name: Generate coverage
        run: cargo llvm-cov --all-features --lcov --output-path lcov.info

      - name: Upload coverage
        uses: codecov/codecov-action@v4
        with:
          files: lcov.info
          fail_ci_if_error: true

  property-tests:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-action@stable

      - name: Run property tests (extended)
        run: cargo test --test property -- --test-threads=1
        env:
          PROPTEST_CASES: 10000

  benchmarks:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-action@stable

      - name: Run benchmarks
        run: cargo bench --no-run  # Just compile, don't run (too slow for CI)

  snapshots:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install Rust
        uses: dtolnay/rust-action@stable

      - name: Check snapshots
        run: cargo insta test --check
```

### 18.9 Skill Tests

Skills can include executable tests to validate correctness. Tests are stored
under `tests/` and run via `ms test`.

**Test Format (YAML):**

```yaml
# tests/basic.yaml
name: "basic"
skill: "rust-error-handling"
steps:
  - load_skill: { level: "standard" }
  - run: "cargo test -q"
  - assert:
      contains: ["error context"]
```

**Runner Contract:**
- `load_skill` injects the selected disclosure
- `run` executes a command or script
- `assert` checks stdout/stderr patterns or file outputs

**CLI:**

```bash
ms test rust-error-handling
ms test --all --report junit
```

**Extended Test Types:**

Beyond basic schema/script tests, ms supports **retrieval tests** and **packing tests**
to enable regression testing of search quality and token efficiency.

```yaml
# tests/retrieval.yaml - Verify skill appears for expected queries
name: "retrieval-accuracy"
skill: "rust-error-handling"
type: retrieval
tests:
  - context:
      cwd: "/data/projects/rust-cli"
      files: ["src/main.rs", "Cargo.toml"]
      keywords: ["Result", "anyhow"]
    query: "error handling patterns"
    expect:
      top_k: 3                    # Must appear in top 3 results
      score_min: 0.6              # Minimum relevance score

  - context:
      diff: "- panic!(\"failed\")\n+ return Err(anyhow!(\"failed\"))"
    query: null                   # Use diff-based suggestion
    expect:
      suggested: true             # Must be auto-suggested for this diff
```

```yaml
# tests/packing.yaml - Verify packing behavior
name: "packing-coverage"
skill: "rust-error-handling"
type: packing
tests:
  - budget: 500
    contract: "DebugContract"
    expect_contains:
      - "rule-1"                  # Critical rule must be included
      - "rule-2"
    expect_excludes:
      - "example-verbose"         # Long example should be cut

  - budget: 1500
    contract: "RefactorContract"
    expect_coverage_groups:
      - "critical-rules"          # All critical rules at this budget
      - "core-examples"
    expect_min_utility: 0.7       # Minimum average utility of packed slices

  - budget: 200
    recent_slice_ids: ["overview", "rule-1"]  # novelty penalty vs prior context
    expect_contains:
      - "overview"                # Overview always included
    expect_max_slices: 3          # At tight budget, only essentials
```

**Test Harness Implementation:**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetrievalTest {
    pub name: String,
    pub skill: String,
    pub query: Option<String>,
    pub context: TestContext,
    pub expect: RetrievalExpect,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackingTest {
    pub budget: usize,
    pub contract: Option<PackContract>,
    pub expect_contains: Vec<String>,
    pub expect_excludes: Vec<String>,
    pub expect_coverage_groups: Vec<String>,
    pub expect_min_utility: Option<f32>,
    pub expect_max_slices: Option<usize>,
    pub required_coverage: Vec<CoverageQuota>,
    pub excluded_groups: Vec<String>,
    pub mandatory_slices: Vec<MandatorySlice>,
    pub max_per_group: Option<usize>,
    pub recent_slice_ids: Option<Vec<String>>,
}

pub struct SkillTestHarness {
    registry: SkillRegistry,
    searcher: HybridSearcher,
    packer: ConstrainedPacker,
}

impl SkillTestHarness {
    pub fn run_retrieval_test(&self, test: &RetrievalTest) -> TestResult {
        let context = SuggestContext::from_test(&test.context);
        let results = if let Some(query) = &test.query {
            self.searcher.search(query, &context.to_filters(), 10).unwrap()
        } else {
            self.searcher.suggest(&context, 10).unwrap()
        };

        let skill_rank = results.iter().position(|r| r.skill_id == test.skill);
        let skill_score = results.iter().find(|r| r.skill_id == test.skill).map(|r| r.score);

        TestResult {
            passed: match &test.expect {
                RetrievalExpect::TopK(k) => skill_rank.map(|r| r < *k).unwrap_or(false),
                RetrievalExpect::Suggested => skill_rank.is_some(),
                RetrievalExpect::ScoreMin(min) => skill_score.map(|s| s >= *min).unwrap_or(false),
            },
            actual_rank: skill_rank,
            actual_score: skill_score,
        }
    }

    pub fn run_packing_test(&self, test: &PackingTest) -> TestResult {
        let skill = self.registry.get(&test.skill).unwrap();
        let constraints = PackConstraints {
            budget: test.budget,
            max_per_group: test.max_per_group.unwrap_or(3),
            required_coverage: test.required_coverage.clone(),
            excluded_groups: test.excluded_groups.clone(),
            max_improvement_passes: 3,
            mandatory_slices: test.mandatory_slices.clone(),
            fail_on_mandatory_omission: true,
            recent_slice_ids: test.recent_slice_ids.clone().unwrap_or_default(),
            contract: test.contract.clone(),
        };
        let packed = self.packer.pack(&skill.computed.slices, &constraints).unwrap().slices;
        let packed_ids: Vec<_> = packed.iter().map(|s| s.id.clone()).collect();

        let contains_ok = test.expect_contains.iter().all(|id| packed_ids.contains(id));
        let excludes_ok = test.expect_excludes.iter().all(|id| !packed_ids.contains(id));
        let coverage_ok = test.expect_coverage_groups.iter().all(|group| {
            packed.iter().any(|s| s.coverage_group.as_ref() == Some(group))
        });
        let max_slices_ok = test.expect_max_slices
            .map(|max| packed.len() <= max)
            .unwrap_or(true);
        let min_utility_ok = test.expect_min_utility
            .map(|min| {
                let avg = packed.iter().map(|s| s.utility_score).sum::<f32>() / packed.len().max(1) as f32;
                avg >= min
            })
            .unwrap_or(true);

        TestResult {
            passed: contains_ok && excludes_ok && coverage_ok && max_slices_ok && min_utility_ok,
            packed_slice_ids: packed_ids,
            total_tokens: packed.iter().map(|s| s.token_estimate).sum(),
        }
    }
}
```

**CI Integration:**

```bash
# Run all test types
ms test --all --report junit > test-results.xml

# Run only retrieval tests (fast, no model needed)
ms test --type retrieval

# Run packing tests with specific budget range
ms test --type packing --budget-range 200-2000
```

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

```bash
ms simulate rust-error-handling
ms simulate rust-error-handling --project /data/projects/demo
ms simulate rust-error-handling --report json
```

---

## 19. Skill Templates Library

### 19.1 Template System Overview

Pre-built templates for common skill patterns, enabling rapid skill creation with best practices baked in.

```rust
pub struct TemplateLibrary {
    pub templates: HashMap<String, SkillTemplate>,
    pub custom_templates_dir: PathBuf,
}

pub struct SkillTemplate {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: TemplateCategory,
    pub structure: TemplateStructure,
    pub placeholders: Vec<Placeholder>,
    pub examples: Vec<String>,
    pub best_for: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum TemplateCategory {
    Workflow,       // Multi-step processes
    Checklist,      // Quality gates, reviews
    Reference,      // Documentation, API guides
    Pattern,        // Design patterns, idioms
    Debugging,      // Error diagnosis, troubleshooting
    Integration,    // Tool integrations
    Custom,         // User-defined
}

pub struct TemplateStructure {
    pub sections: Vec<TemplateSection>,
    pub optional_sections: Vec<TemplateSection>,
    pub resources: Option<ResourcesTemplate>,
}

pub struct TemplateSection {
    pub name: String,
    pub heading_level: u8,
    pub content_type: ContentType,
    pub placeholder: Option<String>,
    pub example: String,
}

#[derive(Debug, Clone)]
pub enum ContentType {
    Prose,
    CodeBlock { language: String },
    Checklist,
    Table,
    DecisionTree,
    CommandSequence,
}

pub struct Placeholder {
    pub name: String,
    pub description: String,
    pub default: Option<String>,
    pub validation: Option<String>,  // Regex pattern
    pub required: bool,
}
```

### 19.2 Built-in Templates

#### 19.2.1 Workflow Template

```markdown
---
name: {{skill_name}}
description: {{description}}
---

# {{skill_name}}

## Overview

{{overview}}

## Prerequisites

{{prerequisites}}

## Workflow Steps

### Step 1: {{step_1_name}}

{{step_1_content}}

```{{language}}
{{step_1_code}}
```

### Step 2: {{step_2_name}}

{{step_2_content}}

## Decision Points

```
{{decision_point}} ?
├── YES → {{yes_action}}
└── NO → {{no_action}}
```

## Common Issues

| Issue | Cause | Solution |
|-------|-------|----------|
| {{issue_1}} | {{cause_1}} | {{solution_1}} |

## Anti-Patterns (Avoid)

- ❌ {{anti_pattern_1}} → ✅ {{anti_pattern_1_fix}}
- ❌ {{anti_pattern_2}} → ✅ {{anti_pattern_2_fix}}

## Completion Criteria

- [ ] {{criterion_1}}
- [ ] {{criterion_2}}
```

#### 19.2.2 Checklist Template

```markdown
---
name: {{skill_name}}
description: {{description}}
---

# {{skill_name}}

## Purpose

{{purpose}}

## When to Use

- {{trigger_1}}
- {{trigger_2}}

## Pre-Flight Checklist

### Required

- [ ] {{required_check_1}}
- [ ] {{required_check_2}}

### Recommended

- [ ] {{recommended_check_1}}

## Main Checklist

### {{section_1_name}}

- [ ] {{check_1_1}}
  - {{check_1_1_detail}}
- [ ] {{check_1_2}}

### {{section_2_name}}

- [ ] {{check_2_1}}
- [ ] {{check_2_2}}

## Post-Completion

- [ ] {{post_check_1}}
- [ ] {{post_check_2}}

## Quick Reference

| Check | Command | Expected |
|-------|---------|----------|
| {{check_name}} | `{{command}}` | {{expected}} |
```

#### 19.2.3 Debugging Template

```markdown
---
name: {{skill_name}}
description: {{description}}
---

# {{skill_name}}

## Symptoms

{{symptom_description}}

## Quick Diagnosis

```{{language}}
{{diagnostic_command}}
```

## Root Cause Analysis

### Most Common: {{common_cause_1}}

**Indicators:**
- {{indicator_1}}
- {{indicator_2}}

**Fix:**
```{{language}}
{{fix_code}}
```

### Less Common: {{common_cause_2}}

**Indicators:**
- {{indicator_3}}

**Fix:**
{{fix_description}}

## Decision Tree

```
{{symptom}}
├── Check: {{check_1}}
│   ├── PASS → {{next_check}}
│   └── FAIL → {{cause_1}} → {{fix_1}}
└── Check: {{check_2}}
    └── FAIL → {{cause_2}} → {{fix_2}}
```

## Prevention

{{prevention_steps}}
```

#### 19.2.4 Integration Template

```markdown
---
name: {{skill_name}}
description: {{description}}
---

# {{skill_name}}

## Tool Overview

**Tool:** {{tool_name}}
**Version:** {{version}}
**Purpose:** {{purpose}}

## Setup

```{{language}}
{{setup_commands}}
```

## Configuration

```{{config_format}}
{{config_example}}
```

## Common Operations

### {{operation_1_name}}

```{{language}}
{{operation_1_command}}
```

### {{operation_2_name}}

```{{language}}
{{operation_2_command}}
```

## Integration with Other Tools

| Tool | Integration Point | Example |
|------|-------------------|---------|
| {{tool_1}} | {{integration_1}} | `{{example_1}}` |

## Troubleshooting

### {{error_1}}

**Cause:** {{error_1_cause}}
**Fix:** {{error_1_fix}}

## Best Practices

- {{practice_1}}
- {{practice_2}}
```

#### 19.2.5 Pattern Template

```markdown
---
name: {{skill_name}}
description: {{description}}
---

# {{skill_name}}

## Pattern Intent

{{intent}}

## When to Use

- {{use_case_1}}
- {{use_case_2}}

## When NOT to Use

- {{anti_use_case_1}}

## Structure

```{{language}}
{{pattern_structure}}
```

## Implementation

### Basic Implementation

```{{language}}
{{basic_implementation}}
```

### Advanced Implementation

```{{language}}
{{advanced_implementation}}
```

## Variations

### {{variation_1_name}}

{{variation_1_description}}

```{{language}}
{{variation_1_code}}
```

## Trade-offs

| Aspect | Pro | Con |
|--------|-----|-----|
| {{aspect_1}} | {{pro_1}} | {{con_1}} |

## Related Patterns

- {{related_1}}: {{relationship_1}}
- {{related_2}}: {{relationship_2}}
```

### 19.3 Template CLI Commands

```bash
# List available templates
ms template list
# → workflow     Multi-step process documentation
# → checklist    Quality gates and review checklists
# → debugging    Error diagnosis and troubleshooting
# → integration  Tool integration guides
# → pattern      Design patterns and idioms

# Show template details
ms template show workflow
# → [Template details with placeholders]

# Create skill from template
ms template use workflow --name "deploy-workflow"
# → Interactive prompt for placeholders

# Create with pre-filled values
ms template use debugging \
    --name "react-render-issues" \
    --set symptom_description="Components not re-rendering" \
    --set language="typescript"

# Create custom template
ms template create --from-skill rust-patterns --name "rust-template"

# Import template from URL
ms template import https://github.com/user/templates/workflow.json

# Export template
ms template export workflow --output workflow.json
```

### 19.4 Template Instantiation Engine

```rust
pub struct TemplateEngine {
    pub templates: TemplateLibrary,
    pub handlebars: Handlebars<'static>,
}

impl TemplateEngine {
    pub fn new() -> Self {
        let mut handlebars = Handlebars::new();

        // Register helpers
        handlebars.register_helper("lowercase", Box::new(lowercase_helper));
        handlebars.register_helper("uppercase", Box::new(uppercase_helper));
        handlebars.register_helper("kebab", Box::new(kebab_case_helper));
        handlebars.register_helper("snake", Box::new(snake_case_helper));
        handlebars.register_helper("code_block", Box::new(code_block_helper));
        handlebars.register_helper("table_row", Box::new(table_row_helper));

        Self {
            templates: TemplateLibrary::load_builtin(),
            handlebars,
        }
    }

    pub fn instantiate(
        &self,
        template_id: &str,
        values: &HashMap<String, String>,
    ) -> Result<String> {
        let template = self.templates.get(template_id)
            .ok_or(Error::TemplateNotFound)?;

        // Validate required placeholders
        for placeholder in &template.placeholders {
            if placeholder.required && !values.contains_key(&placeholder.name) {
                return Err(Error::MissingPlaceholder(placeholder.name.clone()));
            }

            // Apply validation regex if present
            if let (Some(pattern), Some(value)) = (&placeholder.validation, values.get(&placeholder.name)) {
                let regex = Regex::new(pattern)?;
                if !regex.is_match(value) {
                    return Err(Error::InvalidPlaceholderValue {
                        name: placeholder.name.clone(),
                        pattern: pattern.clone(),
                    });
                }
            }
        }

        // Build context with defaults
        let mut context: HashMap<String, String> = template.placeholders
            .iter()
            .filter_map(|p| p.default.as_ref().map(|d| (p.name.clone(), d.clone())))
            .collect();

        // Override with provided values
        context.extend(values.clone());

        // Render template
        let rendered = self.handlebars.render_template(
            &template.structure.to_markdown(),
            &context,
        )?;

        Ok(rendered)
    }

    pub fn interactive_instantiate(
        &self,
        template_id: &str,
    ) -> Result<String> {
        let template = self.templates.get(template_id)
            .ok_or(Error::TemplateNotFound)?;

        let mut values = HashMap::new();

        // Prompt for each placeholder
        for placeholder in &template.placeholders {
            let prompt = format!(
                "{}{}: ",
                placeholder.name,
                if placeholder.required { " (required)" } else { "" }
            );

            let default = placeholder.default.as_deref().unwrap_or("");
            let value = dialoguer::Input::<String>::new()
                .with_prompt(&prompt)
                .default(default.to_string())
                .interact_text()?;

            if !value.is_empty() {
                values.insert(placeholder.name.clone(), value);
            }
        }

        self.instantiate(template_id, &values)
    }
}

// Handlebars helpers
fn code_block_helper(
    h: &Helper,
    _: &Handlebars,
    _: &Context,
    _: &mut RenderContext,
    out: &mut dyn Output,
) -> HelperResult {
    let language = h.param(0).and_then(|v| v.value().as_str()).unwrap_or("");
    let content = h.param(1).and_then(|v| v.value().as_str()).unwrap_or("");

    out.write(&format!("```{}\n{}\n```", language, content))?;
    Ok(())
}
```

### 19.5 Template Discovery from Sessions

```rust
/// Analyze sessions to discover potential new templates
pub struct TemplateDiscovery {
    pub cass: CassClient,
    pub pattern_detector: PatternDetector,
}

impl TemplateDiscovery {
    /// Find recurring structures in successful sessions
    pub async fn discover_patterns(&self, min_occurrences: usize) -> Vec<DiscoveredPattern> {
        let sessions = self.cass.search_sessions(
            "successful completion",
            SearchOptions::default().with_limit(1000),
        ).await?;

        // Extract structural patterns
        let structures: Vec<SessionStructure> = sessions
            .iter()
            .map(|s| self.pattern_detector.extract_structure(s))
            .collect();

        // Cluster similar structures
        let clusters = self.cluster_structures(&structures);

        // Return patterns that appear frequently
        clusters
            .into_iter()
            .filter(|c| c.members.len() >= min_occurrences)
            .map(|c| self.generalize_to_pattern(c))
            .collect()
    }

    /// Suggest template based on skill content
    pub fn suggest_template(&self, skill_content: &str) -> Option<&SkillTemplate> {
        let features = self.extract_features(skill_content);

        // Match against template characteristics
        let scores: Vec<(f32, &SkillTemplate)> = self.templates.templates
            .values()
            .map(|t| (self.match_score(&features, t), t))
            .collect();

        scores
            .into_iter()
            .max_by(|a, b| a.0.partial_cmp(&b.0).unwrap())
            .filter(|(score, _)| *score > 0.5)
            .map(|(_, t)| t)
    }

    fn extract_features(&self, content: &str) -> TemplateFeatures {
        TemplateFeatures {
            has_checklist: content.contains("- [ ]"),
            has_decision_tree: content.contains("├──") || content.contains("└──"),
            has_code_blocks: content.contains("```"),
            has_tables: content.contains("|---|"),
            section_count: content.matches("\n## ").count(),
            code_block_languages: self.extract_languages(content),
        }
    }
}

pub struct DiscoveredPattern {
    pub suggested_name: String,
    pub structure: TemplateStructure,
    pub example_sessions: Vec<String>,
    pub confidence: f32,
}
```

### 19.6 Template Validation

```rust
pub struct TemplateValidator {
    pub rules: Vec<ValidationRule>,
}

impl TemplateValidator {
    pub fn validate(&self, template: &SkillTemplate) -> ValidationReport {
        let mut issues = Vec::new();

        // Check required fields
        if template.name.is_empty() {
            issues.push(ValidationIssue::error("Template name is required"));
        }

        if template.description.is_empty() {
            issues.push(ValidationIssue::error("Template description is required"));
        }

        // Check placeholders
        for placeholder in &template.placeholders {
            if placeholder.required && placeholder.default.is_some() {
                issues.push(ValidationIssue::warning(format!(
                    "Placeholder '{}' is required but has default - consider making it optional",
                    placeholder.name
                )));
            }

            if let Some(pattern) = &placeholder.validation {
                if Regex::new(pattern).is_err() {
                    issues.push(ValidationIssue::error(format!(
                        "Invalid regex pattern for placeholder '{}': {}",
                        placeholder.name, pattern
                    )));
                }
            }
        }

        // Check template renders
        let test_values: HashMap<String, String> = template.placeholders
            .iter()
            .map(|p| (p.name.clone(), "TEST_VALUE".to_string()))
            .collect();

        let engine = TemplateEngine::new();
        if let Err(e) = engine.instantiate(&template.id, &test_values) {
            issues.push(ValidationIssue::error(format!(
                "Template failed to render: {}",
                e
            )));
        }

        ValidationReport { issues }
    }
}
```

---

## 20. Agent Mail Integration for Multi-Agent Skill Coordination

### 20.1 Overview

The `ms` CLI integrates with the Agent Mail MCP server to enable multi-agent skill coordination. When multiple agents work on the same project, they need to:

1. **Share discovered patterns** in real-time
2. **Coordinate skill generation** to avoid duplication
3. **Request skills** from other agents who may have relevant expertise
4. **Notify** when new skills are ready for use

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    AGENT MAIL + MS INTEGRATION                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Agent A (Building skill)          Agent Mail           Agent B (Working)   │
│  ┌─────────────────┐               ┌─────────┐        ┌─────────────────┐  │
│  │ ms build        │──────────────►│ Message │───────►│ Receives:       │  │
│  │ --from-cass     │  "Building    │ Router  │        │ "New skill:     │  │
│  │ "auth patterns" │   auth skill" │         │        │  auth-patterns" │  │
│  └─────────────────┘               └─────────┘        └─────────────────┘  │
│         │                               │                     │             │
│         │                               │                     ▼             │
│         ▼                               │             ┌─────────────────┐  │
│  ┌─────────────────┐                    │             │ ms load         │  │
│  │ Skill Draft     │                    │             │ auth-patterns   │  │
│  │ Generated       │                    │             │ --level full    │  │
│  └─────────────────┘                    │             └─────────────────┘  │
│         │                               │                                   │
│         ▼                               │                                   │
│  ┌─────────────────┐               ┌────┴────┐                             │
│  │ ms add          │──────────────►│ Skill   │◄── Other agents can now    │
│  │ ./auth-draft/   │  "Skill       │ Registry│    load this skill          │
│  └─────────────────┘   available"  └─────────┘                             │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 20.2 Agent Mail Client Integration

```rust
/// Integration with Agent Mail MCP server
pub struct AgentMailClient {
    project_key: String,
    agent_name: String,
    mcp_endpoint: String,
}

impl AgentMailClient {
    /// Register as a skill-building agent
    pub async fn register_skill_builder(
        &self,
        topics: &[String],
    ) -> Result<AgentRegistration> {
        let registration = self.mcp_call("register_agent", json!({
            "project_key": self.project_key,
            "program": "ms",
            "model": "skill-builder",
            "task_description": format!("Building skills for: {}", topics.join(", ")),
        })).await?;

        Ok(registration)
    }

    /// Announce start of skill building session
    pub async fn announce_build_start(
        &self,
        topic: &str,
        estimated_duration: Duration,
    ) -> Result<()> {
        self.broadcast_message(
            "skill-build-start",
            json!({
                "topic": topic,
                "estimated_duration_minutes": estimated_duration.as_secs() / 60,
                "builder": self.agent_name,
            }),
        ).await
    }

    /// Announce a bounty for a requested skill
    pub async fn announce_bounty(
        &self,
        topic: &str,
        bounty: SkillRequestBounty,
    ) -> Result<()> {
        self.broadcast_message(
            "skill-bounty",
            json!({
                "topic": topic,
                "bounty": bounty.amount,
                "currency": bounty.currency,
                "requester": self.agent_name,
            }),
        ).await
    }

    /// Announce skill completion
    pub async fn announce_skill_ready(
        &self,
        skill: &Skill,
        quality_score: f32,
    ) -> Result<()> {
        self.broadcast_message(
            "skill-available",
            json!({
                "skill_id": skill.id,
                "skill_name": skill.name,
                "description": skill.description,
                "quality_score": quality_score,
                "topics": skill.tags,
            }),
        ).await
    }

    /// Request a skill from other agents
    pub async fn request_skill(
        &self,
        topic: &str,
        urgency: SkillRequestUrgency,
    ) -> Result<SkillRequest> {
        self.send_message(
            "skill-request",
            "all",  // Broadcast to all agents
            json!({
                "topic": topic,
                "urgency": urgency,
                "requester": self.agent_name,
            }),
            urgency.to_importance(),
        ).await
    }

    /// Check if another agent is already building a skill for this topic
    pub async fn check_skill_in_progress(&self, topic: &str) -> Result<Option<SkillBuildStatus>> {
        let messages = self.fetch_inbox().await?;

        for msg in messages {
            if msg.msg_type == "skill-build-start" {
                if let Some(build_topic) = msg.data.get("topic") {
                    if self.topics_overlap(topic, build_topic.as_str().unwrap_or("")) {
                        return Ok(Some(SkillBuildStatus {
                            builder: msg.data["builder"].as_str().unwrap_or("").to_string(),
                            topic: build_topic.as_str().unwrap_or("").to_string(),
                            started_at: msg.timestamp,
                        }));
                    }
                }
            }
        }

        Ok(None)
    }

    /// Reserve patterns for skill building (prevent duplicate work)
    pub async fn reserve_patterns(
        &self,
        pattern_ids: &[String],
        ttl: Duration,
    ) -> Result<PatternReservation> {
        self.mcp_call("file_reservation_paths", json!({
            "project_key": self.project_key,
            "agent_name": self.agent_name,
            "paths": pattern_ids.iter()
                .map(|id| format!("patterns/{}", id))
                .collect::<Vec<_>>(),
            "ttl_seconds": ttl.as_secs(),
            "exclusive": true,
            "reason": "skill-building",
        })).await
    }

    fn topics_overlap(&self, topic1: &str, topic2: &str) -> bool {
        // Simple word overlap check
        let words1: HashSet<&str> = topic1.split_whitespace().collect();
        let words2: HashSet<&str> = topic2.split_whitespace().collect();
        let intersection: HashSet<_> = words1.intersection(&words2).collect();

        intersection.len() as f32 / words1.len().max(words2.len()) as f32 > 0.5
    }
}

#[derive(Debug, Clone)]
pub enum SkillRequestUrgency {
    /// Agent can proceed without skill but it would help
    Nice,
    /// Agent is blocked waiting for this skill
    Blocking,
    /// Critical - agent cannot proceed
    Critical,
}

/// Optional bounty for skill requests
#[derive(Debug, Clone)]
pub struct SkillRequestBounty {
    pub amount: u32,
    pub currency: String, // e.g., "credits"
}

impl SkillRequestUrgency {
    fn to_importance(&self) -> &'static str {
        match self {
            Self::Nice => "normal",
            Self::Blocking => "high",
            Self::Critical => "urgent",
        }
    }
}
```

**Reservation-Aware Editing (Fallback):**
- If Agent Mail is unavailable, ms provides a local reservation mechanism with
  compatible semantics (path/glob, TTL, exclusive/shared).
- When Agent Mail is available, ms bridges to it transparently.

### 20.3 Coordination Protocol

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    SKILL BUILDING COORDINATION PROTOCOL                      │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Before starting build:                                                     │
│  1. Check Agent Mail for "skill-build-start" messages on similar topics    │
│  2. If another agent is building, either:                                  │
│     a) Wait and subscribe to their completion notification                  │
│     b) Offer to collaborate (send patterns you've found)                   │
│     c) Build complementary skill (different aspect of same topic)          │
│  3. Reserve patterns you'll be using (via file_reservation)                │
│  4. Announce your build start                                              │
│                                                                             │
│  During build:                                                              │
│  5. Periodically check inbox for:                                          │
│     - Pattern contributions from other agents                               │
│     - Skill requests that your in-progress skill might satisfy             │
│     - Build cancellation/priority changes                                   │
│  6. Send progress updates every checkpoint                                  │
│                                                                             │
│  After build:                                                               │
│  7. Announce skill availability with quality score                         │
│  8. Release pattern reservations                                           │
│  9. Respond to any pending skill requests that this skill satisfies        │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 20.4 CLI Commands with Agent Mail

```bash
# Check if anyone is building skills for this topic
ms build --check-duplicates "auth patterns"
# → Agent "BlueLake" is building "authentication-workflow" (started 15m ago)
# → Wait for completion? [y/n]

# Build with coordination enabled (default)
ms build --guided --topic "error handling" --coordinate
# → Announcing build start to 3 agents in project...
# → Checking for pattern reservations...
# → Reserving 12 patterns...
# → Starting interactive build...

# Build without coordination (solo mode)
ms build --guided --topic "error handling" --no-coordinate

# Request a skill from the swarm
ms request "database migration patterns" --urgency blocking
# → Request sent to 5 agents
# → Waiting for responses...

# Request with bounty (prioritize & reward)
ms request "deployment workflow" --urgency blocking --bounty 200

# Check skill requests from other agents
ms inbox --requests
# → GreenCastle requests: "react testing patterns" (urgency: nice)
# → RedBear requests: "deployment workflow" (urgency: blocking)

# Respond to a skill request
ms respond GreenCastle --skill react-testing-patterns
# → Skill shared with GreenCastle

# Subscribe to skill completion
ms subscribe "auth" --timeout 30m
# → Watching for skills matching "auth"...
# → [12:34] BlueLake completed "auth-workflow" (quality: 0.87)
# → Loading skill...
```

### 20.5 Pattern Sharing Between Agents

```rust
/// Share patterns discovered during exploration
pub struct PatternSharer {
    mail_client: AgentMailClient,
    local_patterns: PatternStore,
}

impl PatternSharer {
    /// Share patterns that might help other agents
    pub async fn share_relevant_patterns(
        &self,
        recipient: &str,
        topic: &str,
    ) -> Result<usize> {
        // Find patterns relevant to topic
        let patterns = self.local_patterns.search(topic, 20)?;

        if patterns.is_empty() {
            return Ok(0);
        }

        // Package patterns for sharing
        let pattern_bundle = PatternBundle {
            source_agent: self.mail_client.agent_name.clone(),
            topic: topic.to_string(),
            patterns: patterns.iter().map(|p| p.to_shareable()).collect(),
            created_at: Utc::now(),
        };

        // Send via Agent Mail
        self.mail_client.send_message(
            "pattern-contribution",
            recipient,
            json!({
                "bundle": pattern_bundle,
                "message": format!(
                    "Found {} patterns that might help with your '{}' skill",
                    patterns.len(), topic
                ),
            }),
            "normal",
        ).await?;

        Ok(patterns.len())
    }

    /// Receive and integrate shared patterns
    pub async fn receive_patterns(&self) -> Result<Vec<Pattern>> {
        let messages = self.mail_client.fetch_inbox().await?;
        let mut received = Vec::new();

        for msg in messages {
            if msg.msg_type == "pattern-contribution" {
                if let Some(bundle) = msg.data.get("bundle") {
                    let pattern_bundle: PatternBundle = serde_json::from_value(bundle.clone())?;

                    for pattern in pattern_bundle.patterns {
                        // Validate and store
                        if self.validate_pattern(&pattern) {
                            self.local_patterns.add_external(&pattern, &pattern_bundle.source_agent)?;
                            received.push(pattern);
                        }
                    }

                    // Acknowledge receipt
                    self.mail_client.acknowledge_message(msg.id).await?;
                }
            }
        }

        Ok(received)
    }
}
```

### 20.6 Multi-Agent Skill Swarm

When building skills at scale with multiple agents (via NTM), coordinate using this pattern:

```rust
/// Orchestrate multiple agents building skills in parallel
pub struct SkillSwarm {
    agents: Vec<AgentMailClient>,
    topic_allocator: TopicAllocator,
}

impl SkillSwarm {
    /// Distribute skill building across agents
    pub async fn distribute_topics(&self, topics: &[String]) -> Result<AllocationResult> {
        let mut allocations = HashMap::new();

        for topic in topics {
            // Find best agent for this topic
            let best_agent = self.find_best_agent(topic).await?;

            // Reserve topic for agent
            self.topic_allocator.reserve(topic, &best_agent.agent_name).await?;

            allocations.insert(topic.clone(), best_agent.agent_name.clone());

            // Notify agent of assignment
            best_agent.send_message(
                "skill-assignment",
                &best_agent.agent_name,
                json!({
                    "topic": topic,
                    "priority": self.calculate_priority(topic),
                }),
                "high",
            ).await?;
        }

        Ok(AllocationResult { allocations })
    }

    async fn find_best_agent(&self, topic: &str) -> Result<&AgentMailClient> {
        // Factors for agent selection:
        // 1. Current workload (fewer in-progress builds = better)
        // 2. Past success with similar topics
        // 3. Agent's session history coverage for this topic

        let mut best = &self.agents[0];
        let mut best_score = 0.0;

        for agent in &self.agents {
            let workload = self.get_agent_workload(&agent.agent_name).await?;
            let expertise = self.get_agent_expertise(&agent.agent_name, topic).await?;

            let score = expertise * (1.0 - workload);

            if score > best_score {
                best_score = score;
                best = agent;
            }
        }

        Ok(best)
    }
}
```

---

## 21. Interactive Build TUI Experience

### 21.1 TUI Layout

The interactive build experience uses a rich terminal UI for guided skill generation:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│ META_SKILL BUILD SESSION                                      [F1:Help]     │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│ ╔═══════════════════════════════════════════════════════════════════════╗  │
│ ║ Building: "nextjs-accessibility-patterns"                              ║  │
│ ║ Phase: Pattern Review (2/4)  │  Iteration: 3  │  Quality: 0.72        ║  │
│ ║ Duration: 00:23:45           │  Patterns: 47 found, 23 used           ║  │
│ ╚═══════════════════════════════════════════════════════════════════════╝  │
│                                                                             │
│ ┌─ Pattern Clusters ─────────────────────┐ ┌─ Current Draft ─────────────┐ │
│ │ ▶ aria-hidden fixes (12 patterns)     │ │ ---                         │ │
│ │   ├─ [✓] SVG decorative elements      │ │ name: nextjs-accessibility  │ │
│ │   ├─ [✓] Icon button accessibility    │ │ description: ...            │ │
│ │   ├─ [ ] Image alt text patterns      │ │ ---                         │ │
│ │   └─ [?] Dynamic content announce     │ │                             │ │
│ │                                        │ │ # Accessibility Patterns    │ │
│ │   focus-management (8 patterns)       │ │                             │ │
│ │   ├─ [ ] Focus trap in modals         │ │ ## CRITICAL RULES           │ │
│ │   ├─ [ ] Focus restoration            │ │                             │ │
│ │   └─ [ ] Skip links                   │ │ 1. All decorative SVGs MUST │ │
│ │                                        │ │    have aria-hidden="true"  │ │
│ │   motion-reduce (6 patterns)          │ │                             │ │
│ │   └─ [ ] prefers-reduced-motion       │ │ 2. Interactive elements... │ │
│ │                                        │ │                             │ │
│ │ ──────────────────────────────────────│ │ [Preview: 847 tokens]       │ │
│ │ Unclustered (21 patterns)             │ │                             │ │
│ └────────────────────────────────────────┘ └─────────────────────────────┘ │
│                                                                             │
│ ┌─ Pattern Detail ──────────────────────────────────────────────────────┐  │
│ │ Pattern: SVG decorative elements (cluster: aria-hidden)               │  │
│ │ Confidence: 0.89  │  Occurrences: 7  │  Sessions: 4                   │  │
│ │                                                                        │  │
│ │ Specific instances:                                                    │  │
│ │ > "Fixed aria-hidden on hero SVG decoration"                          │  │
│ │ > "Added aria-hidden to spinner icon"                                  │  │
│ │ > "Marked decorative divider SVG as hidden"                           │  │
│ │                                                                        │  │
│ │ Generalized pattern:                                                   │  │
│ │ "Decorative SVG elements (icons, illustrations not conveying info)    │  │
│ │  must have aria-hidden='true' to prevent screen reader confusion"     │  │
│ │                                                                        │  │
│ │ [a]ccept  [r]eject  [e]dit  [s]kip  [m]ore examples                   │  │
│ └────────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│ ┌─ Actions ─────────────────────────────────────────────────────────────┐  │
│ │ [Space] Toggle pattern  │  [Enter] Confirm selection  │  [/] Search   │  │
│ │ [Tab] Next cluster      │  [g] Generate draft         │  [q] Save/Quit│  │
│ └────────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│ Progress: ████████████░░░░░░░░ 58%   Checkpoint: 2m ago   [c] Checkpoint   │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 21.2 TUI Components

```rust
use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Gauge, List, ListItem, Paragraph, Tabs},
    Frame,
};

/// Main build session TUI
pub struct BuildTui {
    pub state: BuildState,
    pub selected_cluster: usize,
    pub selected_pattern: usize,
    pub draft_scroll: u16,
    pub focus: TuiFocus,
}

#[derive(Debug, Clone)]
pub enum TuiFocus {
    Clusters,
    Patterns,
    Draft,
    Actions,
}

impl BuildTui {
    pub fn draw(&self, frame: &mut Frame) {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(3),   // Title bar
                Constraint::Length(4),   // Status banner
                Constraint::Min(20),     // Main content
                Constraint::Length(4),   // Pattern detail
                Constraint::Length(3),   // Actions bar
                Constraint::Length(1),   // Progress bar
            ])
            .split(frame.size());

        self.draw_title_bar(frame, chunks[0]);
        self.draw_status_banner(frame, chunks[1]);
        self.draw_main_content(frame, chunks[2]);
        self.draw_pattern_detail(frame, chunks[3]);
        self.draw_actions_bar(frame, chunks[4]);
        self.draw_progress_bar(frame, chunks[5]);
    }

    fn draw_status_banner(&self, frame: &mut Frame, area: Rect) {
        let status = format!(
            " Building: \"{}\"  │  Phase: {} ({}/{})  │  Iteration: {}  │  Quality: {:.2}",
            self.state.skill_name,
            self.state.phase_name(),
            self.state.phase_index + 1,
            self.state.total_phases,
            self.state.iteration,
            self.state.quality_score,
        );

        let style = match self.state.quality_score {
            q if q >= 0.8 => Style::default().fg(Color::Green),
            q if q >= 0.6 => Style::default().fg(Color::Yellow),
            _ => Style::default().fg(Color::Red),
        };

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .title(" Session Status ");

        let paragraph = Paragraph::new(status)
            .style(style)
            .block(block);

        frame.render_widget(paragraph, area);
    }

    fn draw_clusters_list(&self, frame: &mut Frame, area: Rect) {
        let items: Vec<ListItem> = self.state.clusters
            .iter()
            .enumerate()
            .map(|(i, cluster)| {
                let selected_count = cluster.patterns.iter()
                    .filter(|p| p.selected)
                    .count();
                let total = cluster.patterns.len();

                let prefix = if i == self.selected_cluster {
                    "▶"
                } else {
                    " "
                };

                let status_icon = if selected_count == total {
                    "✓"
                } else if selected_count > 0 {
                    "◐"
                } else {
                    "○"
                };

                ListItem::new(format!(
                    "{} {} {} ({} patterns, {} selected)",
                    prefix, status_icon, cluster.name, total, selected_count
                ))
            })
            .collect();

        let list = List::new(items)
            .block(Block::default()
                .borders(Borders::ALL)
                .title(" Pattern Clusters "))
            .highlight_style(Style::default()
                .add_modifier(Modifier::BOLD)
                .bg(Color::DarkGray));

        frame.render_widget(list, area);
    }

    fn draw_draft_preview(&self, frame: &mut Frame, area: Rect) {
        let draft = &self.state.current_draft;
        let token_count = estimate_tokens(draft);

        let block = Block::default()
            .borders(Borders::ALL)
            .title(format!(" Current Draft [{} tokens] ", token_count));

        // Syntax highlight the draft (simplified)
        let paragraph = Paragraph::new(draft.as_str())
            .block(block)
            .scroll((self.draft_scroll, 0));

        frame.render_widget(paragraph, area);
    }

    fn draw_progress_bar(&self, frame: &mut Frame, area: Rect) {
        let progress = self.state.progress_percentage();

        let checkpoint_info = match &self.state.last_checkpoint {
            Some(cp) => {
                let ago = Utc::now() - cp.timestamp;
                format!("Checkpoint: {}m ago", ago.num_minutes())
            }
            None => "No checkpoint yet".to_string(),
        };

        let gauge = Gauge::default()
            .block(Block::default())
            .gauge_style(Style::default()
                .fg(Color::Cyan)
                .bg(Color::DarkGray))
            .percent(progress as u16)
            .label(format!(
                "Progress: {}%   {}   [c] Save checkpoint",
                progress, checkpoint_info
            ));

        frame.render_widget(gauge, area);
    }
}
```

### 21.3 TUI Navigation and Actions

```rust
/// Handle keyboard input during build session
impl BuildTui {
    pub fn handle_key(&mut self, key: KeyEvent) -> TuiAction {
        match key.code {
            // Navigation
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Tab => self.cycle_focus(),
            KeyCode::BackTab => self.cycle_focus_reverse(),

            // Pattern actions
            KeyCode::Char(' ') => self.toggle_current_pattern(),
            KeyCode::Char('a') => self.accept_pattern(),
            KeyCode::Char('r') => self.reject_pattern(),
            KeyCode::Char('e') => TuiAction::EditPattern(self.current_pattern_id()),
            KeyCode::Char('s') => self.skip_pattern(),
            KeyCode::Char('m') => self.show_more_examples(),

            // Build actions
            KeyCode::Enter => self.confirm_selection(),
            KeyCode::Char('g') => TuiAction::GenerateDraft,
            KeyCode::Char('c') => TuiAction::SaveCheckpoint,

            // Search and filter
            KeyCode::Char('/') => TuiAction::OpenSearch,
            KeyCode::Char('f') => TuiAction::OpenFilter,

            // Help and quit
            KeyCode::F(1) => TuiAction::ShowHelp,
            KeyCode::Char('q') => TuiAction::ConfirmQuit,
            KeyCode::Esc => self.handle_escape(),

            _ => TuiAction::None,
        }
    }

    fn toggle_current_pattern(&mut self) -> TuiAction {
        if let Some(cluster) = self.state.clusters.get_mut(self.selected_cluster) {
            if let Some(pattern) = cluster.patterns.get_mut(self.selected_pattern) {
                pattern.selected = !pattern.selected;
                self.state.update_quality_estimate();
            }
        }
        TuiAction::Refresh
    }

    fn accept_pattern(&mut self) -> TuiAction {
        // Accept pattern and move to next
        self.toggle_current_pattern();
        self.move_selection(1);
        TuiAction::Refresh
    }
}

/// Modal dialogs for the TUI
pub struct BuildDialogs;

impl BuildDialogs {
    /// Pattern editing modal
    pub fn edit_pattern_dialog(pattern: &Pattern) -> EditDialog {
        EditDialog {
            title: format!("Edit Pattern: {}", pattern.name),
            fields: vec![
                EditField::text("Name", &pattern.name),
                EditField::multiline("Generalized Pattern", &pattern.generalized),
                EditField::tags("Tags", &pattern.tags),
                EditField::slider("Confidence", pattern.confidence, 0.0, 1.0),
            ],
        }
    }

    /// Search patterns dialog
    pub fn search_dialog() -> SearchDialog {
        SearchDialog {
            placeholder: "Search patterns...",
            filters: vec![
                SearchFilter::Dropdown("Cluster", vec!["All", "Selected", "Unselected"]),
                SearchFilter::Range("Confidence", 0.0, 1.0),
            ],
        }
    }

    /// Confirm quit with unsaved changes
    pub fn quit_confirm_dialog(unsaved_changes: bool) -> ConfirmDialog {
        if unsaved_changes {
            ConfirmDialog {
                title: "Unsaved Changes",
                message: "You have unsaved changes. Save checkpoint before quitting?",
                options: vec![
                    ("Save & Quit", DialogAction::SaveAndQuit),
                    ("Quit without saving", DialogAction::Quit),
                    ("Cancel", DialogAction::Cancel),
                ],
            }
        } else {
            ConfirmDialog {
                title: "Quit Build Session",
                message: "Exit the build session?",
                options: vec![
                    ("Quit", DialogAction::Quit),
                    ("Cancel", DialogAction::Cancel),
                ],
            }
        }
    }
}
```

### 21.4 Real-Time Draft Generation

```rust
/// Live draft generation as patterns are selected
pub struct LiveDraftGenerator {
    transformer: SpecificToGeneralTransformer,
    debounce: Duration,
    last_generation: Option<Instant>,
}

impl LiveDraftGenerator {
    /// Regenerate draft preview when selection changes
    pub async fn regenerate_preview(
        &mut self,
        selected_patterns: &[Pattern],
        current_draft: &str,
    ) -> Result<DraftPreview> {
        // Debounce rapid changes
        if let Some(last) = self.last_generation {
            if last.elapsed() < self.debounce {
                return Ok(DraftPreview::unchanged(current_draft));
            }
        }

        // Generate quick preview (not full refinement)
        let preview = self.transformer.quick_transform(selected_patterns).await?;

        self.last_generation = Some(Instant::now());

        Ok(DraftPreview {
            content: preview,
            token_count: estimate_tokens(&preview),
            quality_estimate: self.estimate_quality(&preview, selected_patterns),
            diff_from_current: generate_diff(current_draft, &preview),
        })
    }

    fn estimate_quality(&self, draft: &str, patterns: &[Pattern]) -> f32 {
        let mut score = 0.5;  // Base score

        // More patterns = potentially better coverage
        score += (patterns.len() as f32 / 50.0).min(0.2);

        // Higher confidence patterns = better quality
        let avg_confidence: f32 = patterns.iter()
            .map(|p| p.confidence)
            .sum::<f32>() / patterns.len().max(1) as f32;
        score += avg_confidence * 0.2;

        // Check for critical sections
        if draft.contains("⚠️") || draft.contains("CRITICAL") {
            score += 0.05;
        }

        // Check for code examples
        let code_blocks = draft.matches("```").count() / 2;
        score += (code_blocks as f32 * 0.02).min(0.1);

        score.clamp(0.0, 1.0)
    }
}
```

---

## 22. Skill Effectiveness Feedback Loop

### 22.1 Overview

Track whether skills actually help agents accomplish their tasks. This data improves skill quality scores and informs future skill generation.
When multiple variants exist, ms can run A/B experiments to select the most effective version.

**Slice-Level Experiments:**
- Experiments can target individual slices (rule wording, example blocks) while keeping
  the rest of the skill constant for faster convergence.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    SKILL EFFECTIVENESS FEEDBACK LOOP                         │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌──────────┐      ┌──────────┐      ┌──────────┐      ┌──────────┐       │
│  │  Skill   │─────►│  Agent   │─────►│  CASS    │─────►│  MS      │       │
│  │  Loaded  │      │  Uses It │      │  Indexes │      │ Analyzes │       │
│  └──────────┘      └──────────┘      └──────────┘      └────┬─────┘       │
│                                                              │              │
│                    ┌─────────────────────────────────────────┘              │
│                    ▼                                                        │
│  Was the task successful?                                                   │
│  ├── YES → Positive signal for skill                                       │
│  │         - Increment success count                                        │
│  │         - Boost quality score                                            │
│  │         - Extract what worked well                                       │
│  │                                                                          │
│  └── NO → Analyze failure                                                   │
│           - Was skill followed correctly?                                   │
│           - Did skill have wrong information?                               │
│           - Was skill not applicable?                                       │
│           - Update skill or flag for review                                 │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 22.2 Usage Tracking

```rust
/// Track skill usage and outcomes
pub struct EffectivenessTracker {
    db: Connection,
    cass: CassClient,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillExperiment {
    pub id: String,
    pub skill_id: String,
    pub scope: ExperimentScope,      // skill | slice
    pub scope_id: Option<String>,    // slice_id if scope = slice
    pub variants: Vec<ExperimentVariant>,
    pub allocation: AllocationStrategy,
    pub started_at: DateTime<Utc>,
    pub status: ExperimentStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExperimentScope {
    Skill,
    Slice,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExperimentVariant {
    pub variant_id: String,
    pub version: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AllocationStrategy {
    Uniform,
    Weighted(Vec<(String, f32)>),
    ThompsonSampling,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExperimentStatus {
    Running,
    Paused,
    Completed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillUsageEvent {
    /// Unique event ID
    pub id: String,

    /// The skill that was used
    pub skill_id: String,

    /// Session where skill was used
    pub session_id: String,

    /// When the skill was loaded
    pub loaded_at: DateTime<Utc>,

    /// Disclosure level used
    pub disclosure_level: DisclosureLevel,

    /// How skill was discovered
    pub discovery_method: DiscoveryMethod,

    /// Experiment id if this usage is part of an A/B test
    pub experiment_id: Option<String>,

    /// Variant id if in an experiment
    pub variant_id: Option<String>,

    /// Outcome of the session
    pub outcome: Option<SessionOutcome>,

    /// Specific feedback if any
    pub feedback: Option<SkillFeedback>,
}

/// Per-rule outcome signals for calibration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleOutcome {
    pub rule_id: String,
    pub followed: bool,
    pub outcome: SessionOutcome,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DiscoveryMethod {
    /// User explicitly requested the skill
    DirectRequest,
    /// Suggested by ms based on context
    Suggestion,
    /// Auto-loaded based on project
    AutoLoad,
    /// Recommended by another agent
    AgentRecommendation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionOutcome {
    /// Task completed successfully
    Success {
        duration: Duration,
        quality_signals: Vec<String>,  // e.g., "tests passed", "code reviewed"
    },

    /// Task completed with issues
    PartialSuccess {
        completed_aspects: Vec<String>,
        failed_aspects: Vec<String>,
    },

    /// Task not completed
    Failure {
        reason: FailureReason,
        at_step: Option<String>,  // Where in the skill did it fail
    },

    /// Session abandoned before completion
    Abandoned {
        reason: Option<String>,
        progress_percent: f32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FailureReason {
    /// Skill instructions were wrong
    IncorrectInstructions,
    /// Skill was not applicable to the situation
    NotApplicable,
    /// Skill was incomplete (missing steps)
    Incomplete,
    /// External factors (API down, permissions, etc.)
    ExternalFactors,
    /// User changed direction
    DirectionChange,
    /// Unknown
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillFeedback {
    /// Overall rating (1-5)
    pub rating: u8,

    /// What worked well
    pub positives: Vec<String>,

    /// What could be improved
    pub improvements: Vec<String>,

    /// Specific sections that helped
    pub helpful_sections: Vec<String>,

    /// Specific sections that were confusing
    pub confusing_sections: Vec<String>,
}

impl EffectivenessTracker {
    /// Record when a skill is loaded
    pub fn record_skill_load(
        &self,
        skill_id: &str,
        session_id: &str,
        level: DisclosureLevel,
        method: DiscoveryMethod,
        experiment: Option<(String, String)>, // (experiment_id, variant_id)
    ) -> Result<String> {
        let event = SkillUsageEvent {
            id: uuid::Uuid::new_v4().to_string(),
            skill_id: skill_id.to_string(),
            session_id: session_id.to_string(),
            loaded_at: Utc::now(),
            disclosure_level: level,
            discovery_method: method,
            experiment_id: experiment.as_ref().map(|(id, _)| id.clone()),
            variant_id: experiment.as_ref().map(|(_, v)| v.clone()),
            outcome: None,
            feedback: None,
        };

        self.db.execute(
            "INSERT INTO skill_usage_events VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                event.id,
                event.skill_id,
                event.session_id,
                event.loaded_at.to_rfc3339(),
                serde_json::to_string(&event.disclosure_level)?,
                serde_json::to_string(&event.discovery_method)?,
                event.experiment_id.clone(),
                event.variant_id.clone(),
                None::<String>,  // outcome
                None::<String>,  // feedback
            ],
        )?;

        Ok(event.id)
    }

    /// Analyze session to determine skill effectiveness
    pub async fn analyze_session_outcome(
        &self,
        usage_event_id: &str,
    ) -> Result<SessionOutcome> {
        let event = self.get_usage_event(usage_event_id)?;
        let session = self.cass.get_session(&event.session_id).await?;

        // Analyze session content for outcome signals
        let outcome = self.infer_outcome(&session, &event)?;

        // Update the usage event
        self.db.execute(
            "UPDATE skill_usage_events SET outcome = ? WHERE id = ?",
            params![serde_json::to_string(&outcome)?, usage_event_id],
        )?;

        Ok(outcome)
    }

    fn infer_outcome(&self, session: &Session, event: &SkillUsageEvent) -> Result<SessionOutcome> {
        let signals = self.extract_outcome_signals(session);

        // Success signals
        let success_indicators = [
            "completed", "done", "finished", "success", "passed",
            "working", "fixed", "resolved", "shipped",
        ];

        // Failure signals
        let failure_indicators = [
            "failed", "error", "broken", "doesn't work", "giving up",
            "wrong", "incorrect", "abandoned",
        ];

        let success_score: f32 = success_indicators.iter()
            .map(|s| signals.iter().filter(|sig| sig.to_lowercase().contains(s)).count() as f32)
            .sum();

        let failure_score: f32 = failure_indicators.iter()
            .map(|s| signals.iter().filter(|sig| sig.to_lowercase().contains(s)).count() as f32)
            .sum();

        // Determine outcome
        if success_score > failure_score * 2.0 {
            Ok(SessionOutcome::Success {
                duration: session.duration(),
                quality_signals: signals.into_iter()
                    .filter(|s| success_indicators.iter().any(|i| s.contains(i)))
                    .collect(),
            })
        } else if failure_score > success_score {
            Ok(SessionOutcome::Failure {
                reason: self.infer_failure_reason(&signals),
                at_step: self.find_failure_step(session, event),
            })
        } else {
            Ok(SessionOutcome::PartialSuccess {
                completed_aspects: vec![],
                failed_aspects: vec![],
            })
        }
    }

    /// Infer whether specific rules were followed and their outcomes
    pub async fn infer_rule_outcomes(
        &self,
        session: &Session,
        skill: &Skill,
    ) -> Result<Vec<RuleOutcome>> {
        // Heuristic: look for rule-linked commands, file edits, and keywords
        // to determine if a rule was followed.
        let mut outcomes = Vec::new();
        for rule in skill.evidence.rules.keys() {
            let followed = session.contains_rule_signal(rule);
            outcomes.push(RuleOutcome {
                rule_id: rule.clone(),
                followed,
                outcome: if followed {
                    SessionOutcome::Success { duration: session.duration(), quality_signals: vec![] }
                } else {
                    SessionOutcome::Failure { reason: FailureReason::Unknown, at_step: None }
                },
            });
        }
        Ok(outcomes)
    }

    /// Aggregate per-rule stats for calibration
    pub fn get_rule_stats(&self, skill_id: &str) -> Result<HashMap<String, RuleStat>> {
        // Query rule_outcomes and compute success rate per rule
        unimplemented!()
    }
}

#[derive(Debug, Clone)]
pub struct RuleStat {
    pub total: usize,
    pub success: usize,
}
```

### 22.3 Feedback Collection

```rust
/// Collect explicit feedback from agents/users
pub struct FeedbackCollector {
    tracker: EffectivenessTracker,
}

impl FeedbackCollector {
    /// Prompt for feedback after session completion
    pub fn collect_feedback_interactive(&self, skill_id: &str) -> Result<SkillFeedback> {
        println!("\n━━━ Skill Feedback: {} ━━━\n", skill_id);

        // Rating
        let rating: u8 = dialoguer::Input::new()
            .with_prompt("How helpful was this skill? (1-5)")
            .validate_with(|v: &u8| {
                if *v >= 1 && *v <= 5 { Ok(()) }
                else { Err("Rating must be 1-5") }
            })
            .interact()?;

        // Positives
        let positives: String = dialoguer::Input::new()
            .with_prompt("What worked well? (comma-separated)")
            .allow_empty(true)
            .interact()?;

        // Improvements
        let improvements: String = dialoguer::Input::new()
            .with_prompt("What could be improved? (comma-separated)")
            .allow_empty(true)
            .interact()?;

        Ok(SkillFeedback {
            rating,
            positives: parse_comma_list(&positives),
            improvements: parse_comma_list(&improvements),
            helpful_sections: vec![],
            confusing_sections: vec![],
        })
    }

    /// Infer feedback from session content (no user input needed)
    pub async fn infer_feedback(&self, session_id: &str, skill_id: &str) -> Result<SkillFeedback> {
        let session = self.tracker.cass.get_session(session_id).await?;

        // Look for feedback signals in session
        let mut feedback = SkillFeedback {
            rating: 3,  // Default neutral
            positives: vec![],
            improvements: vec![],
            helpful_sections: vec![],
            confusing_sections: vec![],
        };

        // Extract positive mentions
        for msg in &session.messages {
            if msg.contains("this helped") || msg.contains("worked great") {
                feedback.rating = feedback.rating.saturating_add(1);
                if let Some(section) = self.extract_mentioned_section(msg) {
                    feedback.helpful_sections.push(section);
                }
            }

            if msg.contains("confusing") || msg.contains("didn't work") {
                feedback.rating = feedback.rating.saturating_sub(1);
                if let Some(section) = self.extract_mentioned_section(msg) {
                    feedback.confusing_sections.push(section);
                }
            }
        }

        Ok(feedback)
    }
}
```

### 22.4 Quality Score Updates

```rust
/// Update skill quality based on effectiveness data
pub struct QualityUpdater {
    scorer: QualityScorer,
    tracker: EffectivenessTracker,
    db: Connection,
}

impl QualityUpdater {
    /// Recalculate quality score incorporating usage data
    pub fn update_quality(&self, skill_id: &str) -> Result<QualityScore> {
        let skill = self.db.get_skill(skill_id)?;
        let usage_stats = self.tracker.get_usage_stats(skill_id)?;

        let mut score = self.scorer.score(&skill);

        // Adjust effectiveness factor based on actual usage
        if usage_stats.total_uses > 5 {  // Enough data to be meaningful
            let success_rate = usage_stats.successful_uses as f32 / usage_stats.total_uses as f32;
            score.factors.effectiveness = success_rate;

            // Recalculate overall
            score.overall = self.calculate_weighted_score(&score.factors);
        }

        // Calibrate rule strengths based on outcomes
        let rule_stats = self.tracker.get_rule_stats(skill_id)?;
        for (rule_id, stats) in rule_stats {
            if stats.total >= 5 {
                let success_rate = stats.success as f32 / stats.total as f32;
                self.db.execute(
                    "UPDATE skill_rules SET strength = ? WHERE skill_id = ? AND rule_id = ?",
                    params![success_rate, skill_id, rule_id],
                )?;
            }
        }

        // Store updated score
        self.db.execute(
            "UPDATE skills SET quality_score = ?, effectiveness_score = ? WHERE id = ?",
            params![score.overall, score.factors.effectiveness, skill_id],
        )?;

        Ok(score)
    }

    /// Generate improvement suggestions from feedback
    pub fn generate_improvements(&self, skill_id: &str) -> Result<Vec<ImprovementSuggestion>> {
        let feedback_list = self.tracker.get_feedback(skill_id)?;
        let mut suggestions = Vec::new();

        // Aggregate confusing sections
        let confusing: HashMap<String, usize> = feedback_list.iter()
            .flat_map(|f| &f.confusing_sections)
            .fold(HashMap::new(), |mut acc, section| {
                *acc.entry(section.clone()).or_default() += 1;
                acc
            });

        for (section, count) in confusing {
            if count >= 3 {  // Multiple reports of same issue
                suggestions.push(ImprovementSuggestion {
                    section,
                    suggestion_type: SuggestionType::ClarifySection,
                    priority: count as f32 / feedback_list.len() as f32,
                    evidence: format!("{} users found this confusing", count),
                });
            }
        }

        // Aggregate improvement requests
        let improvements: HashMap<String, usize> = feedback_list.iter()
            .flat_map(|f| &f.improvements)
            .fold(HashMap::new(), |mut acc, imp| {
                *acc.entry(imp.clone()).or_default() += 1;
                acc
            });

        for (improvement, count) in improvements {
            if count >= 2 {
                suggestions.push(ImprovementSuggestion {
                    section: "general".to_string(),
                    suggestion_type: SuggestionType::AddContent(improvement.clone()),
                    priority: count as f32 / feedback_list.len() as f32,
                    evidence: format!("{} users requested: {}", count, improvement),
                });
            }
        }

        suggestions.sort_by(|a, b| b.priority.partial_cmp(&a.priority).unwrap());
        Ok(suggestions)
    }
}
```

### 22.4.1 A/B Skill Experiments

When multiple versions of a skill exist (e.g., different wording, structure, or
examples), ms can run A/B experiments to empirically determine the more effective
variant. Results feed back into quality scoring and can automatically promote the
winning version.

```rust
pub struct ExperimentRunner {
    tracker: EffectivenessTracker,
    db: Connection,
}

impl ExperimentRunner {
    /// Create a new experiment for a skill
    pub fn create_experiment(
        &self,
        skill_id: &str,
        variants: Vec<ExperimentVariant>,
        allocation: AllocationStrategy,
    ) -> Result<SkillExperiment> {
        let experiment = SkillExperiment {
            id: uuid::Uuid::new_v4().to_string(),
            skill_id: skill_id.to_string(),
            variants,
            allocation,
            started_at: Utc::now(),
            status: ExperimentStatus::Running,
        };

        self.db.execute(
            "INSERT INTO skill_experiments VALUES (?, ?, ?, ?, ?, ?)",
            params![
                experiment.id,
                experiment.skill_id,
                serde_json::to_string(&experiment.variants)?,
                serde_json::to_string(&experiment.allocation)?,
                "running",
                experiment.started_at.to_rfc3339(),
            ],
        )?;

        Ok(experiment)
    }

    /// Assign a variant for a given load event
    pub fn assign_variant(&self, experiment: &SkillExperiment) -> Result<(String, String)> {
        let variant = select_variant(&experiment.variants, &experiment.allocation);
        Ok((experiment.id.clone(), variant.variant_id.clone()))
    }

    /// Evaluate experiment and recommend winner
    pub fn evaluate(&self, experiment_id: &str) -> Result<ExperimentResult> {
        let stats = self.tracker.get_experiment_stats(experiment_id)?;
        Ok(ExperimentResult::from_stats(stats))
    }
}

#[derive(Debug, Clone)]
pub struct ExperimentResult {
    pub winner_variant: Option<String>,
    pub confidence: f32,
    pub stats: HashMap<String, VariantStats>,
}

#[derive(Debug, Clone)]
pub struct VariantStats {
    pub uses: usize,
    pub success_rate: f32,
    pub avg_rating: f32,
}
```

### 22.5 CLI Commands for Effectiveness

```bash
# Record skill usage
ms track load rust-patterns --session CURRENT --method suggestion

# Record outcome
ms track outcome rust-patterns --success --signals "tests passed,code reviewed"
ms track outcome rust-patterns --failure --reason "incorrect-instructions" --step "Step 3"

# Provide feedback
ms feedback rust-patterns --rating 4 \
    --positive "clear examples" \
    --improve "add error handling section"

# View effectiveness stats
ms stats effectiveness rust-patterns
# → Uses: 47  │  Success: 38 (81%)  │  Avg Rating: 4.2
# → Most helpful: "Code Examples" section
# → Needs improvement: "Troubleshooting" section

# View improvement suggestions
ms improvements rust-patterns
# → HIGH: Clarify "Error Handling" section (5 reports)
# → MED: Add more examples for async patterns (3 requests)

# Calibrate rule strengths from outcomes
ms calibrate rust-patterns

# Update quality scores with latest data
ms quality update --all
ms quality update rust-patterns

# Start an A/B experiment with two variants
ms experiment start rust-patterns \
  --variant v1 --desc "Concise rules-first layout" \
  --variant v2 --desc "Examples-first layout"

# View experiment results
ms experiment results rust-patterns
```

---

## 23. Cross-Project Learning and Coverage Analysis

### 23.1 Overview

Learn from sessions across multiple projects to build more comprehensive skills and identify coverage gaps.

**CM (cass-memory) Integration:**
- Integrate `/data/projects/cass_memory_system` as a shared procedural memory layer.
- Unify rule IDs, confidence decay, and anti-pattern promotion across ms and CM.
- Provide import/export bridges so CM playbooks and ms skills reinforce each other.

**CM Bridge Data Model:**

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CmBridgeConfig {
    pub cm_bin: PathBuf,
    pub cm_db_path: PathBuf,
    pub import_enabled: bool,
    pub export_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CmRuleLink {
    pub cm_rule_id: String,
    pub ms_rule_id: String,
    pub confidence: f32,
    pub tags: Vec<String>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CmSyncStatus {
    pub last_imported_at: Option<DateTime<Utc>>,
    pub last_exported_at: Option<DateTime<Utc>>,
    pub status: String,
}
```

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    CROSS-PROJECT LEARNING ARCHITECTURE                       │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Project A              Project B              Project C                    │
│  (NextJS)               (Rust CLI)             (Go Backend)                 │
│  ┌───────┐              ┌───────┐              ┌───────┐                   │
│  │Sessions│              │Sessions│              │Sessions│                   │
│  └───┬───┘              └───┬───┘              └───┬───┘                   │
│      │                      │                      │                        │
│      └──────────────────────┼──────────────────────┘                        │
│                             │                                               │
│                             ▼                                               │
│                    ┌────────────────┐                                       │
│                    │  Pattern Pool  │                                       │
│                    │  (CASS Index)  │                                       │
│                    └───────┬────────┘                                       │
│                            │                                                │
│         ┌──────────────────┼──────────────────┐                            │
│         ▼                  ▼                  ▼                            │
│  ┌─────────────┐   ┌─────────────┐   ┌─────────────┐                      │
│  │ Tech-Specific│   │  Universal  │   │  Workflow  │                       │
│  │   Patterns   │   │  Patterns   │   │  Patterns  │                       │
│  │  (per-stack) │   │ (all stacks)│   │ (meta-lvl) │                       │
│  └─────────────┘   └─────────────┘   └─────────────┘                      │
│                                                                             │
│  Examples:          Examples:          Examples:                            │
│  - NextJS routing   - Error handling   - Code review workflow               │
│  - Rust error types - Git workflows    - Debugging methodology              │
│  - Go concurrency   - Testing patterns - Documentation patterns             │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 23.2 Cross-Project Pattern Extraction

```rust
/// Extract patterns that appear across multiple projects
pub struct CrossProjectAnalyzer {
    cass: CassClient,
    projects: Vec<ProjectInfo>,
}

#[derive(Debug, Clone)]
pub struct ProjectInfo {
    pub path: PathBuf,
    pub name: String,
    pub tech_stack: TechStackContext,
    pub session_count: usize,
}

impl CrossProjectAnalyzer {
    /// Find patterns that appear in multiple projects
    pub async fn find_universal_patterns(&self, min_projects: usize) -> Result<Vec<UniversalPattern>> {
        let mut pattern_occurrences: HashMap<String, Vec<ProjectPattern>> = HashMap::new();

        // Collect patterns from all projects
        for project in &self.projects {
            let sessions = self.cass.get_sessions_for_project(&project.path).await?;
            let patterns = self.extract_patterns(&sessions)?;

            for pattern in patterns {
                let normalized = self.normalize_pattern(&pattern);
                pattern_occurrences
                    .entry(normalized.clone())
                    .or_default()
                    .push(ProjectPattern {
                        project: project.name.clone(),
                        tech_stack: project.tech_stack.clone(),
                        original: pattern,
                    });
            }
        }

        // Filter to patterns appearing in multiple projects
        let universal: Vec<UniversalPattern> = pattern_occurrences
            .into_iter()
            .filter(|(_, occurrences)| {
                let unique_projects: HashSet<_> = occurrences.iter()
                    .map(|o| &o.project)
                    .collect();
                unique_projects.len() >= min_projects
            })
            .map(|(normalized, occurrences)| UniversalPattern {
                normalized_pattern: normalized,
                occurrences,
                confidence: self.calculate_cross_project_confidence(&occurrences),
            })
            .collect();

        Ok(universal)
    }

    /// Identify tech-specific patterns by filtering out universal ones
    ///
    /// NOTE: Properly tracks distinct projects per pattern (not just occurrence count).
    pub async fn find_tech_specific_patterns(
        &self,
        tech_stack: &TechStackContext,
    ) -> Result<Vec<TechSpecificPattern>> {
        // Get universal patterns to exclude
        let universal = self.find_universal_patterns(3).await?;
        let universal_set: HashSet<_> = universal.iter()
            .map(|u| &u.normalized_pattern)
            .collect();

        // Get patterns for this tech stack
        let tech_projects: Vec<_> = self.projects.iter()
            .filter(|p| self.stacks_overlap(&p.tech_stack, tech_stack))
            .collect();

        // Track patterns with their source projects for correct project_count
        let mut pattern_projects: HashMap<String, HashSet<String>> = HashMap::new();
        let mut pattern_examples: HashMap<String, Pattern> = HashMap::new();

        for project in tech_projects {
            let sessions = self.cass.get_sessions_for_project(&project.path).await?;
            let patterns = self.extract_patterns(&sessions)?;

            for pattern in patterns {
                let normalized = self.normalize_pattern(&pattern);

                // Skip if it's a universal pattern
                if universal_set.contains(&normalized) {
                    continue;
                }

                // Track distinct projects per normalized pattern
                pattern_projects
                    .entry(normalized.clone())
                    .or_default()
                    .insert(project.name.clone());

                // Keep one example of the original pattern
                pattern_examples
                    .entry(normalized)
                    .or_insert(pattern);
            }
        }

        // Convert to TechSpecificPattern with correct project_count
        let tech_patterns: Vec<TechSpecificPattern> = pattern_projects
            .into_iter()
            .filter_map(|(normalized, projects)| {
                pattern_examples.get(&normalized).map(|example| {
                    TechSpecificPattern {
                        pattern: example.clone(),
                        tech_stack: tech_stack.clone(),
                        project_count: projects.len(),  // Correct: distinct project count
                    }
                })
            })
            .collect();

        // Merge similar patterns (further clustering if needed)
        Ok(self.merge_tech_patterns(tech_patterns))
    }

    /// Normalize pattern for cross-project comparison
    ///
    /// IMPORTANT: Uses pre-compiled regexes for performance (LazyLock).
    /// Normalization is idempotent: normalize(normalize(x)) == normalize(x)
    fn normalize_pattern(&self, pattern: &Pattern) -> String {
        use std::sync::LazyLock;

        // Pre-compiled regexes (compiled once, reused across calls)
        static PATH_REGEX: LazyLock<Regex> = LazyLock::new(|| {
            // Cross-platform: handles Unix and Windows paths, hyphens, dots in names
            Regex::new(r"(?:[A-Za-z]:\\|/)?[\w\-./\\]+[/\\](src|lib|pkg|internal|cmd)[/\\]").unwrap()
        });
        static JS_EXT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
            // All JS-family extensions including ESM/CJS variants
            Regex::new(r"\.(ts|tsx|js|jsx|mjs|cjs|mts|cts)").unwrap()
        });
        static RUST_EXT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"\.rs").unwrap()
        });
        static GO_EXT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"\.go").unwrap()
        });
        static PY_EXT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"\.(py|pyi)").unwrap()
        });
        static JAVA_EXT_REGEX: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r"\.(java|kt|kts)").unwrap()
        });

        // Remove project-specific details
        let mut normalized = pattern.generalized.clone();

        // Replace specific paths with placeholders (cross-platform)
        normalized = PATH_REGEX
            .replace_all(&normalized, "/PROJECT_ROOT/")
            .to_string();

        // Replace specific file extensions with type placeholders
        normalized = JS_EXT_REGEX
            .replace_all(&normalized, ".{js-family}")
            .to_string();

        normalized = RUST_EXT_REGEX
            .replace_all(&normalized, ".{rust}")
            .to_string();

        normalized = GO_EXT_REGEX
            .replace_all(&normalized, ".{go}")
            .to_string();

        normalized = PY_EXT_REGEX
            .replace_all(&normalized, ".{python}")
            .to_string();

        normalized = JAVA_EXT_REGEX
            .replace_all(&normalized, ".{jvm}")
            .to_string();

        // Lowercase for comparison
        normalized.to_lowercase()
    }
}

#[derive(Debug, Clone)]
pub struct UniversalPattern {
    pub normalized_pattern: String,
    pub occurrences: Vec<ProjectPattern>,
    pub confidence: f32,
}

#[derive(Debug, Clone)]
pub struct ProjectPattern {
    pub project: String,
    pub tech_stack: TechStackContext,
    pub original: Pattern,
}
```

### 23.3 Coverage Gap Analysis

```rust
/// Analyze what patterns exist in sessions but have no corresponding skill
pub struct CoverageAnalyzer {
    cass: CassClient,
    skill_registry: SkillRegistry,
    search: HybridSearcher,
}

/// Knowledge graph for patterns and skills
pub struct KnowledgeGraph {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone)]
pub struct GraphNode {
    pub id: String,
    pub node_type: NodeType,
    pub label: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum NodeType {
    Skill,
    Pattern,
    Topic,
    TechStack,
}

#[derive(Debug, Clone)]
pub struct GraphEdge {
    pub from: String,
    pub to: String,
    pub relation: EdgeRelation,
    pub weight: f32,
}

#[derive(Debug, Clone)]
pub enum EdgeRelation {
    AppliesTo,
    DerivedFrom,
    ConflictsWith,
    SimilarTo,
}

impl CoverageAnalyzer {
    /// Find topics with sessions but no skills
    ///
    /// OPTIMIZATION: Uses batch scoring instead of O(N×search) per-topic queries.
    /// Pre-computes skill embeddings and matches topics locally with ANN search.
    pub async fn find_gaps(&self) -> Result<Vec<CoverageGap>> {
        // Get all unique topics from sessions (cached with TTL)
        let session_topics = self.cass.get_all_topics().await?;

        // BATCH APPROACH: Pre-compute coverage scores for all topics at once
        // instead of N individual search calls
        let coverage_scores = self.batch_compute_coverage(&session_topics).await?;

        let mut gaps: Vec<CoverageGap> = session_topics
            .iter()
            .filter_map(|topic| {
                let (coverage_score, best_skill) = coverage_scores
                    .get(&topic.name)
                    .cloned()
                    .unwrap_or((0.0, None));

                if coverage_score < 0.5 {  // Threshold for "covered"
                    Some(CoverageGap {
                        topic: topic.name.clone(),
                        session_count: topic.session_count,
                        pattern_count: topic.pattern_count,
                        best_matching_skill: best_skill,
                        coverage_score,
                        priority: self.calculate_gap_priority(topic, coverage_score),
                    })
                } else {
                    None
                }
            })
            .collect();

        // Sort by priority (deterministic: stable sort)
        gaps.sort_by(|a, b| {
            b.priority.partial_cmp(&a.priority)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.topic.cmp(&b.topic))  // Tiebreaker for stability
        });

        Ok(gaps)
    }

    /// Batch compute coverage scores for all topics
    ///
    /// Instead of O(N) search calls, this:
    /// 1. Embeds all skill tags into a local index (done once, cached)
    /// 2. For each topic, does fast local ANN lookup
    async fn batch_compute_coverage(
        &self,
        topics: &[TopicStats],
    ) -> Result<HashMap<String, (f32, Option<String>)>> {
        // Build skill coverage index (cached)
        let skill_index = self.get_or_build_skill_index().await?;

        let mut results = HashMap::with_capacity(topics.len());

        for topic in topics {
            // Fast local lookup against pre-built index
            let (score, skill_id) = skill_index.best_match(&topic.name);
            results.insert(topic.name.clone(), (score, skill_id));
        }

        Ok(results)
    }

    /// Get or build cached skill coverage index
    async fn get_or_build_skill_index(&self) -> Result<SkillCoverageIndex> {
        // In practice, this would be cached with TTL and rebuilt when skills change
        let all_skills = self.skill_registry.all_skills()?;

        let mut index = SkillCoverageIndex::new();
        for skill in all_skills {
            // Index by skill tags and name for fast topic matching
            index.add_skill(&skill.id, &skill.metadata.tags, &skill.metadata.name);
        }

        Ok(index)
    }

    /// Suggest which skill to build next
    pub async fn suggest_next_skill(&self) -> Result<SkillSuggestion> {
        let gaps = self.find_gaps().await?;

        if gaps.is_empty() {
            return Ok(SkillSuggestion::NoneNeeded);
        }

        let top_gap = &gaps[0];

        // Get example patterns for this gap
        let patterns = self.cass.search_patterns(&top_gap.topic, 20).await?;

        Ok(SkillSuggestion::Build {
            topic: top_gap.topic.clone(),
            priority: top_gap.priority,
            rationale: format!(
                "{} sessions and {} patterns found with no matching skill",
                top_gap.session_count, top_gap.pattern_count
            ),
            example_patterns: patterns.into_iter().take(5).collect(),
            suggested_tech_stacks: self.infer_tech_stacks(&top_gap.topic).await?,
        })
    }

    /// Calculate priority for filling a gap
    fn calculate_gap_priority(&self, topic: &TopicStats, coverage: f32) -> f32 {
        let mut priority = 0.0;

        // More sessions = higher priority
        priority += (topic.session_count as f32).ln() / 10.0;

        // More patterns = higher confidence in value
        priority += (topic.pattern_count as f32).ln() / 15.0;

        // Recent activity boosts priority
        if let Some(last_seen) = topic.last_seen {
            let days_ago = (Utc::now() - last_seen).num_days();
            if days_ago < 7 {
                priority += 0.3;
            } else if days_ago < 30 {
                priority += 0.1;
            }
        }

        // Lower coverage = higher priority
        priority += (1.0 - coverage) * 0.5;

        priority.clamp(0.0, 1.0)
    }

    /// Build knowledge graph for coverage and discovery
    ///
    /// Uses HashMap for node deduplication - no duplicate node IDs in output.
    /// Node IDs follow stable format: `skill:<id>`, `topic:<tag>`, `pattern:<hash>`, `stack:<id>`
    pub async fn build_graph(&self) -> Result<KnowledgeGraph> {
        // Use HashMap for O(1) deduplication by node ID
        let mut nodes_map: HashMap<String, GraphNode> = HashMap::new();
        let mut edges: Vec<GraphEdge> = Vec::new();

        // Add skills as nodes
        for skill in self.skill_registry.all_skills()? {
            let skill_id = format!("skill:{}", skill.id);
            nodes_map.insert(skill_id.clone(), GraphNode {
                id: skill_id,
                node_type: NodeType::Skill,
                label: skill.metadata.name.clone(),
                tags: skill.metadata.tags.clone(),
            });
        }

        // Collect skill nodes for edge creation
        let skill_nodes: Vec<_> = nodes_map.values()
            .filter(|n| matches!(n.node_type, NodeType::Skill))
            .cloned()
            .collect();

        // Add edges from skills to topics (from tags)
        // Dedup topic nodes via HashMap - same topic from multiple skills = one node
        for node in &skill_nodes {
            for tag in &node.tags {
                let topic_id = format!("topic:{}", tag);

                // Insert topic node only if not already present (dedup)
                nodes_map.entry(topic_id.clone()).or_insert_with(|| GraphNode {
                    id: topic_id.clone(),
                    node_type: NodeType::Topic,
                    label: tag.clone(),
                    tags: vec![],
                });

                edges.push(GraphEdge {
                    from: node.id.clone(),
                    to: topic_id,
                    relation: EdgeRelation::AppliesTo,
                    weight: 0.5,
                });
            }
        }

        // Convert to sorted Vec for deterministic output (diff-stable)
        let mut nodes: Vec<GraphNode> = nodes_map.into_values().collect();
        nodes.sort_by(|a, b| a.id.cmp(&b.id));
        edges.sort_by(|a, b| (&a.from, &a.to).cmp(&(&b.from, &b.to)));

        Ok(KnowledgeGraph { nodes, edges })
    }
}

#[derive(Debug, Clone)]
pub struct CoverageGap {
    pub topic: String,
    pub session_count: usize,
    pub pattern_count: usize,
    pub best_matching_skill: Option<String>,
    pub coverage_score: f32,
    pub priority: f32,
}

pub enum SkillSuggestion {
    NoneNeeded,
    Build {
        topic: String,
        priority: f32,
        rationale: String,
        example_patterns: Vec<Pattern>,
        suggested_tech_stacks: Vec<TechStackContext>,
    },
}
```

### 23.4 CLI Commands for Coverage

```bash
# Analyze coverage gaps
ms coverage gaps
# → Gap Analysis (12 gaps found)
# →
# → HIGH PRIORITY:
# →   1. "database migrations" - 23 sessions, 0% coverage
# →   2. "API versioning" - 18 sessions, 12% coverage
# →
# → MEDIUM PRIORITY:
# →   3. "caching strategies" - 15 sessions, 34% coverage
# →   ...

# Suggest next skill to build
ms coverage suggest
# → Suggested: Build skill for "database migrations"
# →   Sessions: 23  │  Patterns: 67  │  Priority: 0.89
# →
# → Example patterns:
# →   - "Always run migrations in transaction"
# →   - "Test rollback before deploying"
# →   - "Keep migration files small and focused"
# →
# → Run: ms build --guided --topic "database migrations"

# View coverage by tech stack
ms coverage --by-stack
# → NextJS/React: 78% covered (12 skills)
# → Rust CLI: 45% covered (5 skills)
# → Go Backend: 23% covered (2 skills)

# Find cross-project patterns
ms analyze cross-project --min-projects 3
# → Universal patterns (appear in 3+ projects):
# →   1. "Error handling with context" - 5 projects
# →   2. "Retry with exponential backoff" - 4 projects
# →   3. "Configuration from environment" - 4 projects

# Build skill from cross-project patterns
ms build --from-universal "error handling"

# Build or export knowledge graph
ms graph build
ms graph export --format json
```

---

## 24. Error Recovery and Resilience

### 24.1 Overview

Robust error handling for long-running autonomous skill generation, including network failures, LLM errors, and system interruptions.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    ERROR RECOVERY ARCHITECTURE                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Error Category        │  Recovery Strategy       │  User Impact            │
│  ──────────────────────┼──────────────────────────┼─────────────────────── │
│  Network timeout       │  Retry with backoff      │  Automatic, no action  │
│  LLM rate limit        │  Queue + wait            │  Progress paused       │
│  LLM error response    │  Retry or skip pattern   │  May lose some patterns│
│  CASS unavailable      │  Use cached data         │  Stale patterns OK     │
│  Disk full             │  Prune old checkpoints   │  Warning shown         │
│  Process killed        │  Resume from checkpoint  │  Re-run with --resume  │
│  Context exhaustion    │  Summarize + continue    │  Automatic handoff     │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 24.2 Error Taxonomy and Retryability Classification

All errors in `ms` are classified by their retryability to prevent wasteful retry attempts and surface permanent failures immediately.

```rust
use thiserror::Error;

/// Unified error type with retryability classification
#[derive(Error, Debug)]
pub enum MsError {
    // === TRANSIENT (safe to retry with backoff) ===
    #[error("Network timeout: {0}")]
    NetworkTimeout(String),

    #[error("Connection refused: {0}")]
    ConnectionRefused(String),

    #[error("Rate limited by {provider}, retry after {retry_after:?}")]
    RateLimited {
        provider: String,
        retry_after: Option<Duration>,
    },

    #[error("CASS temporarily unavailable: {0}")]
    CassUnavailable(String),

    #[error("LLM service error (retriable): {0}")]
    LlmTransient(String),

    // === PERMANENT (do NOT retry) ===
    #[error("Invalid input: {0}")]
    InvalidInput(String),

    #[error("Skill not found: {0}")]
    SkillNotFound(String),

    #[error("Authentication failed: {0}")]
    AuthFailed(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Schema validation failed: {0}")]
    ValidationFailed(String),

    #[error("Corrupt checkpoint: {path}")]
    CorruptCheckpoint { path: PathBuf },

    #[error("LLM rejected request (permanent): {0}")]
    LlmPermanent(String),

    // === RESOURCE EXHAUSTION (may recover after cleanup) ===
    #[error("Disk full: {0}")]
    DiskFull(String),

    #[error("Out of memory: {0}")]
    OutOfMemory(String),

    #[error("Context window exhausted")]
    ContextExhausted,
}

/// Retry decision based on error classification
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RetryDecision {
    /// Safe to retry immediately or with backoff
    Retry { max_attempts: usize },
    /// Retry after specific delay (from rate limit headers)
    RetryAfter(Duration),
    /// Do not retry - fail immediately with clear error
    NoRetry,
    /// May retry after resource cleanup (disk, memory)
    RetryAfterCleanup,
}

impl MsError {
    /// Determine retry policy based on error type
    pub fn retry_policy(&self) -> RetryDecision {
        match self {
            // Transient errors: retry with exponential backoff
            MsError::NetworkTimeout(_) |
            MsError::ConnectionRefused(_) |
            MsError::CassUnavailable(_) |
            MsError::LlmTransient(_) => RetryDecision::Retry { max_attempts: 5 },

            // Rate limit: respect the retry-after
            MsError::RateLimited { retry_after, .. } => {
                RetryDecision::RetryAfter(retry_after.unwrap_or(Duration::from_secs(60)))
            }

            // Permanent errors: fail fast, don't waste time retrying
            MsError::InvalidInput(_) |
            MsError::SkillNotFound(_) |
            MsError::AuthFailed(_) |
            MsError::PermissionDenied(_) |
            MsError::ValidationFailed(_) |
            MsError::CorruptCheckpoint { .. } |
            MsError::LlmPermanent(_) => RetryDecision::NoRetry,

            // Resource exhaustion: might recover after cleanup
            MsError::DiskFull(_) |
            MsError::OutOfMemory(_) |
            MsError::ContextExhausted => RetryDecision::RetryAfterCleanup,
        }
    }

    /// Exit code for CLI (follows Unix conventions)
    pub fn exit_code(&self) -> i32 {
        match self {
            MsError::InvalidInput(_) | MsError::ValidationFailed(_) => 1,
            MsError::SkillNotFound(_) => 2,
            MsError::AuthFailed(_) | MsError::PermissionDenied(_) => 3,
            MsError::CassUnavailable(_) => 4,
            MsError::NetworkTimeout(_) | MsError::ConnectionRefused(_) => 5,
            MsError::RateLimited { .. } => 6,
            MsError::DiskFull(_) | MsError::OutOfMemory(_) => 7,
            MsError::CorruptCheckpoint { .. } => 8,
            MsError::LlmTransient(_) | MsError::LlmPermanent(_) => 9,
            MsError::ContextExhausted => 10,
        }
    }

    /// User-facing hint for resolving the error
    pub fn hint(&self) -> Option<&'static str> {
        match self {
            MsError::CassUnavailable(_) => Some("Check if CASS is running: `cass health`"),
            MsError::CorruptCheckpoint { .. } => Some("Try: `ms build --prune-checkpoints`"),
            MsError::DiskFull(_) => Some("Free disk space or prune checkpoints"),
            MsError::RateLimited { .. } => Some("Wait and retry, or use a different provider"),
            MsError::ContextExhausted => Some("Try with smaller input or use --resume"),
            _ => None,
        }
    }
}
```

### 24.3 Retry System

```rust
/// Configurable retry system with backoff
pub struct RetryConfig {
    pub max_retries: usize,
    pub initial_delay: Duration,
    pub max_delay: Duration,
    pub backoff_multiplier: f32,
    pub jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 5,
            initial_delay: Duration::from_secs(1),
            max_delay: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            jitter: true,
        }
    }
}

pub struct RetryExecutor {
    config: RetryConfig,
}

impl RetryExecutor {
    /// Execute operation with retry logic
    pub async fn execute<F, T, E>(&self, operation: F) -> Result<T, E>
    where
        F: Fn() -> Pin<Box<dyn Future<Output = Result<T, E>>>>,
        E: std::fmt::Debug,
    {
        let mut delay = self.config.initial_delay;
        let mut attempts = 0;

        loop {
            match operation().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    attempts += 1;

                    if attempts >= self.config.max_retries {
                        eprintln!(
                            "Operation failed after {} attempts: {:?}",
                            attempts, e
                        );
                        return Err(e);
                    }

                    eprintln!(
                        "Operation failed (attempt {}/{}), retrying in {:?}: {:?}",
                        attempts, self.config.max_retries, delay, e
                    );

                    // Apply jitter
                    let actual_delay = if self.config.jitter {
                        let jitter_factor = rand::random::<f32>() * 0.3 + 0.85; // 0.85-1.15
                        Duration::from_secs_f32(delay.as_secs_f32() * jitter_factor)
                    } else {
                        delay
                    };

                    tokio::time::sleep(actual_delay).await;

                    // Increase delay for next attempt
                    delay = Duration::from_secs_f32(
                        (delay.as_secs_f32() * self.config.backoff_multiplier)
                            .min(self.config.max_delay.as_secs_f32())
                    );
                }
            }
        }
    }
}
```

### 24.3 Rate Limit Handler

```rust
/// Handle LLM API rate limits gracefully
pub struct RateLimitHandler {
    /// Rate limit state per provider
    limits: HashMap<String, RateLimitState>,

    /// Queue of pending operations
    queue: VecDeque<QueuedOperation>,
}

#[derive(Debug)]
pub struct RateLimitState {
    pub provider: String,
    pub requests_remaining: Option<usize>,
    pub tokens_remaining: Option<usize>,
    pub reset_at: Option<DateTime<Utc>>,
    pub backoff_until: Option<DateTime<Utc>>,
}

impl RateLimitHandler {
    /// Check if we should proceed or wait
    pub fn should_wait(&self, provider: &str) -> Option<Duration> {
        if let Some(state) = self.limits.get(provider) {
            // Check explicit backoff
            if let Some(until) = state.backoff_until {
                if until > Utc::now() {
                    return Some((until - Utc::now()).to_std().unwrap_or_default());
                }
            }

            // Check requests remaining
            if let Some(remaining) = state.requests_remaining {
                if remaining == 0 {
                    if let Some(reset) = state.reset_at {
                        return Some((reset - Utc::now()).to_std().unwrap_or(Duration::from_secs(60)));
                    }
                }
            }
        }

        None
    }

    /// Update rate limit state from API response headers
    ///
    /// Handles multiple header formats used by different providers:
    /// - `x-ratelimit-reset`: Unix timestamp (seconds) OR RFC3339 datetime
    /// - `retry-after`: Seconds (integer) OR HTTP-date (RFC2616)
    pub fn update_from_headers(&mut self, provider: &str, headers: &HeaderMap) {
        let state = self.limits.entry(provider.to_string()).or_insert_with(|| {
            RateLimitState {
                provider: provider.to_string(),
                requests_remaining: None,
                tokens_remaining: None,
                reset_at: None,
                backoff_until: None,
            }
        });

        // Parse common rate limit headers
        if let Some(remaining) = headers.get("x-ratelimit-remaining") {
            state.requests_remaining = remaining.to_str().ok()
                .and_then(|s| s.parse().ok());
        }

        // x-ratelimit-reset: Try unix timestamp first (most common), then RFC3339
        if let Some(reset) = headers.get("x-ratelimit-reset") {
            if let Ok(s) = reset.to_str() {
                state.reset_at = Self::parse_reset_timestamp(s);
            }
        }

        // retry-after: Can be seconds OR HTTP-date (RFC2616)
        if let Some(retry_after) = headers.get("retry-after") {
            if let Ok(s) = retry_after.to_str() {
                let backoff_duration = Self::parse_retry_after(s);
                state.backoff_until = Some(Utc::now() + backoff_duration);
            }
        }
    }

    /// Parse x-ratelimit-reset header value
    /// Handles: unix timestamp (seconds), RFC3339 datetime
    fn parse_reset_timestamp(value: &str) -> Option<DateTime<Utc>> {
        // Try unix timestamp first (most APIs use this)
        if let Ok(secs) = value.parse::<i64>() {
            return DateTime::from_timestamp(secs, 0);
        }

        // Try RFC3339 datetime
        if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
            return Some(dt.with_timezone(&Utc));
        }

        None
    }

    /// Parse retry-after header value
    /// Handles: seconds (integer), HTTP-date (RFC2616)
    fn parse_retry_after(value: &str) -> chrono::Duration {
        // Try seconds first (most common)
        if let Ok(secs) = value.parse::<i64>() {
            return chrono::Duration::seconds(secs);
        }

        // Try HTTP-date format (e.g., "Wed, 21 Oct 2015 07:28:00 GMT")
        if let Ok(dt) = DateTime::parse_from_rfc2822(value) {
            let now = Utc::now();
            let target = dt.with_timezone(&Utc);
            if target > now {
                return target - now;
            }
        }

        // Default fallback
        chrono::Duration::seconds(60)
    }

    /// Execute with rate limit awareness
    ///
    /// IMPORTANT: Takes a closure that produces a Future, not a Future directly.
    /// This ensures the operation isn't started until after rate limit check.
    pub async fn execute_with_limits<F, Fut, T>(
        &mut self,
        provider: &str,
        operation: F,
    ) -> Result<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<T>>,
    {
        // Wait if rate limited (BEFORE creating the future)
        if let Some(wait_duration) = self.should_wait(provider) {
            eprintln!(
                "Rate limited by {}, waiting {:?}",
                provider, wait_duration
            );
            tokio::time::sleep(wait_duration).await;
        }

        // Now create and await the future
        operation().await
    }
}
```

### 24.4 Checkpoint Recovery

```rust
/// Recover from interruptions using checkpoints
pub struct CheckpointRecovery {
    checkpoint_dir: PathBuf,
}

impl CheckpointRecovery {
    /// Find most recent recoverable checkpoint
    pub fn find_recoverable(&self, session_id: Option<&str>) -> Result<Option<RecoverableSession>> {
        let checkpoints = self.list_checkpoints()?;

        // Filter by session if specified
        let candidates: Vec<_> = if let Some(sid) = session_id {
            checkpoints.into_iter()
                .filter(|cp| cp.session_id == sid)
                .collect()
        } else {
            checkpoints
        };

        if candidates.is_empty() {
            return Ok(None);
        }

        // Find most recent that's actually recoverable
        for checkpoint in candidates.into_iter().rev() {
            if self.is_recoverable(&checkpoint)? {
                return Ok(Some(RecoverableSession {
                    checkpoint,
                    recovery_options: self.analyze_recovery_options(&checkpoint)?,
                }));
            }
        }

        Ok(None)
    }

    /// Check if checkpoint is in recoverable state
    fn is_recoverable(&self, checkpoint: &CheckpointInfo) -> Result<bool> {
        // Check checkpoint file exists and is valid JSON
        let path = self.checkpoint_path(&checkpoint.id);
        if !path.exists() {
            return Ok(false);
        }

        let content = std::fs::read_to_string(&path)?;
        let parsed: Result<GenerationCheckpoint, _> = serde_json::from_str(&content);

        if parsed.is_err() {
            eprintln!("Checkpoint {} is corrupted", checkpoint.id);
            return Ok(false);
        }

        Ok(true)
    }

    /// Analyze what recovery options are available
    fn analyze_recovery_options(&self, checkpoint: &CheckpointInfo) -> Result<Vec<RecoveryOption>> {
        let cp = self.load_checkpoint(&checkpoint.id)?;
        let mut options = Vec::new();

        // Option 1: Resume from exact state
        options.push(RecoveryOption {
            name: "Resume".to_string(),
            description: format!(
                "Continue from phase {:?}, iteration {}",
                cp.phase, cp.metrics.iterations
            ),
            action: RecoveryAction::Resume,
            data_loss: DataLoss::None,
        });

        // Option 2: Restart current phase
        options.push(RecoveryOption {
            name: "Restart Phase".to_string(),
            description: format!("Restart {:?} phase with preserved patterns", cp.phase),
            action: RecoveryAction::RestartPhase,
            data_loss: DataLoss::CurrentPhaseProgress,
        });

        // Option 3: Start fresh with discovered patterns
        if !cp.pattern_pool.is_empty() {
            options.push(RecoveryOption {
                name: "Fresh with Patterns".to_string(),
                description: format!(
                    "Start new session using {} previously discovered patterns",
                    cp.pattern_pool.len()
                ),
                action: RecoveryAction::FreshWithPatterns,
                data_loss: DataLoss::AllExceptPatterns,
            });
        }

        Ok(options)
    }

    /// Execute recovery
    pub fn recover(&self, session: &RecoverableSession, option: &RecoveryOption) -> Result<RecoveredState> {
        let checkpoint = self.load_checkpoint(&session.checkpoint.id)?;

        match option.action {
            RecoveryAction::Resume => {
                Ok(RecoveredState {
                    checkpoint,
                    starting_phase: None,  // Continue current phase
                    preserved_patterns: None,  // All patterns preserved in checkpoint
                })
            }
            RecoveryAction::RestartPhase => {
                let mut cp = checkpoint;
                cp.reset_current_phase();
                Ok(RecoveredState {
                    checkpoint: cp,
                    starting_phase: None,
                    preserved_patterns: None,
                })
            }
            RecoveryAction::FreshWithPatterns => {
                Ok(RecoveredState {
                    checkpoint: GenerationCheckpoint::new_with_patterns(
                        checkpoint.pattern_pool.clone()
                    ),
                    starting_phase: Some(GenerationPhase::Analysis { clusters_formed: 0, current_cluster: None }),
                    preserved_patterns: Some(checkpoint.pattern_pool),
                })
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct RecoverableSession {
    pub checkpoint: CheckpointInfo,
    pub recovery_options: Vec<RecoveryOption>,
}

#[derive(Debug, Clone)]
pub struct RecoveryOption {
    pub name: String,
    pub description: String,
    pub action: RecoveryAction,
    pub data_loss: DataLoss,
}

#[derive(Debug, Clone)]
pub enum RecoveryAction {
    Resume,
    RestartPhase,
    FreshWithPatterns,
}

#[derive(Debug, Clone)]
pub enum DataLoss {
    None,
    CurrentPhaseProgress,
    AllExceptPatterns,
}
```

### 24.5 Graceful Degradation

```rust
use serde::{Serialize, de::DeserializeOwned};

/// Handle component failures gracefully
pub struct GracefulDegradation {
    cass_available: AtomicBool,
    network_available: AtomicBool,
    cache: DegradationCache,
    /// Configured endpoints for health checks (not hardcoded)
    health_endpoints: HealthEndpoints,
}

/// Configurable health check endpoints
#[derive(Debug, Clone)]
pub struct HealthEndpoints {
    /// CASS endpoint (required dependency)
    pub cass: Option<String>,
    /// LLM provider endpoints (optional, only if LLM features enabled)
    pub llm_providers: Vec<String>,
    /// Network check endpoint (optional, for offline detection)
    pub network_probe: Option<String>,
}

impl Default for HealthEndpoints {
    fn default() -> Self {
        Self {
            cass: None,  // Will use CassClient default
            llm_providers: vec![],
            network_probe: None,  // Optional: only check if configured
        }
    }
}

impl GracefulDegradation {
    /// Execute with fallback options
    ///
    /// NOTE: T must be Serialize + DeserializeOwned for caching to work.
    pub async fn execute_with_fallback<T>(
        &self,
        primary: impl Future<Output = Result<T>>,
        fallback: impl Future<Output = Result<T>>,
        cache_key: &str,
    ) -> Result<T>
    where
        T: Serialize + DeserializeOwned,  // Required for cache serialization
    {
        // Try primary
        match primary.await {
            Ok(result) => {
                // Cache for future fallback
                if let Ok(json) = serde_json::to_string(&result) {
                    self.cache.set(cache_key, &json);
                }
                Ok(result)
            }
            Err(e) => {
                eprintln!("Primary operation failed: {:?}, trying fallback", e);

                // Try fallback
                match fallback.await {
                    Ok(result) => Ok(result),
                    Err(fallback_err) => {
                        // Try cache
                        if let Some(cached) = self.cache.get(cache_key) {
                            eprintln!("Using cached data (may be stale)");
                            serde_json::from_str(&cached)
                                .map_err(|e| anyhow!("Cache parse error: {}", e))
                        } else {
                            Err(anyhow!(
                                "All options failed. Primary: {:?}, Fallback: {:?}",
                                e, fallback_err
                            ))
                        }
                    }
                }
            }
        }
    }

    /// Check component health and update status
    ///
    /// Probes CONFIGURED dependencies only - not hardcoded external services.
    /// Network check is optional and uses configured probe endpoint if provided.
    pub async fn health_check(&self) -> HealthStatus {
        // Check CASS (our primary dependency)
        let cass_ok = timeout(Duration::from_secs(5), async {
            CassClient::new().health_check().await.is_ok()
        }).await.unwrap_or(false);
        self.cass_available.store(cass_ok, Ordering::SeqCst);

        // Check network only if a probe endpoint is configured
        // Don't hardcode external services (like GitHub) that may be irrelevant
        let network_ok = if let Some(ref probe_url) = self.health_endpoints.network_probe {
            timeout(Duration::from_secs(5), async {
                reqwest::get(probe_url).await.is_ok()
            }).await.unwrap_or(false)
        } else {
            // No network probe configured = assume network is OK (offline-friendly)
            true
        };
        self.network_available.store(network_ok, Ordering::SeqCst);

        // Optionally check LLM providers if configured
        let llm_status: Vec<_> = futures::future::join_all(
            self.health_endpoints.llm_providers.iter().map(|url| async move {
                let ok = timeout(Duration::from_secs(3), async {
                    reqwest::get(url).await.is_ok()
                }).await.unwrap_or(false);
                (url.clone(), ok)
            })
        ).await;

        HealthStatus {
            cass_available: cass_ok,
            network_available: network_ok,
            llm_providers: llm_status,
            cache_size: self.cache.size(),
            degraded_mode: !cass_ok,  // Only CASS is required
        }
    }
}

#[derive(Debug, Clone)]
pub struct HealthStatus {
    pub cass_available: bool,
    pub network_available: bool,
    pub llm_providers: Vec<(String, bool)>,
    pub cache_size: usize,
    pub degraded_mode: bool,
}
```

### 24.6 CLI Commands for Recovery

```bash
# Check for recoverable sessions
ms build --check-recoverable
# → Found 2 recoverable sessions:
# →   1. abc123 - "nextjs-patterns" (3 hours ago, Phase: Generation)
# →   2. def456 - "rust-cli" (1 day ago, Phase: Discovery)

# Resume specific session
ms build --resume abc123
# → Resuming session abc123...
# → Recovered from checkpoint #42 (Phase: Generation, Iteration: 7)
# → Continuing...

# Resume with options
ms build --resume abc123 --recovery-option 2
# → Using recovery option: "Restart Phase"
# → Restarting Generation phase with 156 preserved patterns...

# Force fresh start (discard checkpoint)
ms build --fresh --topic "nextjs-patterns"

# View checkpoint details before recovery
ms build --show-checkpoint abc123
# → Session: abc123
# → Created: 2026-01-13T10:30:00Z
# → Phase: Generation (skill 3/5)
# → Patterns: 156 discovered, 89 used
# → Skills in progress: nextjs-routing
# → Quality estimate: 0.76

# Clean up old checkpoints
ms build --prune-checkpoints --older-than 7d
# → Removed 12 checkpoints (245 MB freed)
```

---

## 25. Skill Versioning and Migration System

### 25.1 Overview

Track skill versions semantically and provide migration paths when skills evolve.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    SKILL VERSIONING SYSTEM                                   │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  Version Format: MAJOR.MINOR.PATCH                                          │
│                                                                             │
│  MAJOR: Breaking changes                                                    │
│    - Skill renamed or reorganized                                           │
│    - Critical rules changed                                                 │
│    - Incompatible with previous usage patterns                              │
│                                                                             │
│  MINOR: New content, backward compatible                                    │
│    - New sections added                                                     │
│    - New examples added                                                     │
│    - Expanded coverage                                                      │
│                                                                             │
│  PATCH: Fixes and clarifications                                            │
│    - Typo fixes                                                             │
│    - Clarified wording                                                      │
│    - Updated external references                                            │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 25.2 Version Data Model

```rust
use semver::Version;

/// Skill version metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillVersion {
    /// Semantic version
    pub version: Version,

    /// What changed in this version
    pub changelog: String,

    /// Breaking changes (if major bump)
    pub breaking_changes: Vec<BreakingChange>,

    /// Migration available from previous version
    pub migration_from: Option<Version>,

    /// When this version was created
    pub created_at: DateTime<Utc>,

    /// Author of this version
    pub author: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BreakingChange {
    /// What changed
    pub description: String,

    /// How to migrate
    pub migration_hint: String,

    /// Sections affected
    pub affected_sections: Vec<String>,
}

/// Version history for a skill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionHistory {
    pub skill_id: String,
    pub versions: Vec<SkillVersion>,
    pub current: Version,
}

impl VersionHistory {
    /// Get migration path between versions
    pub fn migration_path(&self, from: &Version, to: &Version) -> Option<Vec<Migration>> {
        if from >= to {
            return None;  // Can't migrate backwards or to same version
        }

        let mut path = Vec::new();
        let mut current = from.clone();

        while current < *to {
            // Find next version
            let next = self.versions.iter()
                .find(|v| v.version > current && v.migration_from.as_ref() == Some(&current))
                .or_else(|| {
                    // Find any version greater than current
                    self.versions.iter()
                        .filter(|v| v.version > current)
                        .min_by(|a, b| a.version.cmp(&b.version))
                })?;

            path.push(Migration {
                from: current.clone(),
                to: next.version.clone(),
                steps: self.generate_migration_steps(&current, &next.version),
            });

            current = next.version.clone();
        }

        Some(path)
    }

    fn generate_migration_steps(&self, from: &Version, to: &Version) -> Vec<MigrationStep> {
        let from_ver = self.versions.iter().find(|v| &v.version == from);
        let to_ver = self.versions.iter().find(|v| &v.version == to);

        match (from_ver, to_ver) {
            (Some(_from), Some(to)) => {
                let mut steps = Vec::new();

                // Add steps for breaking changes
                for bc in &to.breaking_changes {
                    steps.push(MigrationStep {
                        action: MigrationAction::Update,
                        description: bc.description.clone(),
                        hint: bc.migration_hint.clone(),
                        automatic: false,
                    });
                }

                steps
            }
            _ => vec![],
        }
    }
}

#[derive(Debug, Clone)]
pub struct Migration {
    pub from: Version,
    pub to: Version,
    pub steps: Vec<MigrationStep>,
}

#[derive(Debug, Clone)]
pub struct MigrationStep {
    pub action: MigrationAction,
    pub description: String,
    pub hint: String,
    pub automatic: bool,
}

#[derive(Debug, Clone)]
pub enum MigrationAction {
    Update,           // Content changed
    Rename,           // Section renamed
    Remove,           // Section removed
    Add,              // New required section
    SplitSkill,       // Skill split into multiple
    MergeSkill,       // Multiple skills merged
}
```

### 25.3 Version Tracking

```sql
-- Database schema for versioning
-- Supports both "available" (registry) and "installed" (local) states

CREATE TABLE skill_versions (
    skill_id TEXT NOT NULL,
    version TEXT NOT NULL,           -- semver string e.g. "2.1.0"
    changelog TEXT NOT NULL,
    breaking_changes_json TEXT,       -- JSON array of BreakingChange
    migration_from TEXT,              -- Previous version this migrates from
    created_at TEXT NOT NULL,         -- RFC3339 timestamp
    author TEXT NOT NULL,
    content_hash TEXT,                -- SHA256 of skill content for integrity
    PRIMARY KEY (skill_id, version)
);

-- Track what's installed locally vs what's available in registry
CREATE TABLE installed_skills (
    skill_id TEXT PRIMARY KEY,
    installed_version TEXT NOT NULL,  -- Currently installed version
    pinned_version TEXT,              -- NULL = auto-update, non-NULL = pinned
    installed_at TEXT NOT NULL,       -- RFC3339 timestamp
    source TEXT NOT NULL,             -- 'local', 'registry', 'git:url'
    FOREIGN KEY (skill_id, installed_version) REFERENCES skill_versions(skill_id, version)
);

CREATE INDEX idx_skill_versions_skill ON skill_versions(skill_id);
CREATE INDEX idx_installed_source ON installed_skills(source);
```

```rust
/// Track and manage skill versions
///
/// Distinguishes between:
/// - **Available versions**: All versions in the registry (skill_versions table)
/// - **Installed version**: What's currently active locally (installed_skills table)
/// - **Pinned version**: Prevents auto-update (installed_skills.pinned_version)
pub struct VersionManager {
    db: Connection,
    git: GitArchive,
}

impl VersionManager {
    /// Create new version of a skill
    pub fn create_version(
        &self,
        skill_id: &str,
        bump_type: BumpType,
        changelog: &str,
        breaking_changes: Vec<BreakingChange>,
    ) -> Result<SkillVersion> {
        let history = self.get_history(skill_id)?;
        let current = &history.current;

        let new_version = match bump_type {
            BumpType::Major => Version::new(current.major + 1, 0, 0),
            BumpType::Minor => Version::new(current.major, current.minor + 1, 0),
            BumpType::Patch => Version::new(current.major, current.minor, current.patch + 1),
        };

        let version = SkillVersion {
            version: new_version.clone(),
            changelog: changelog.to_string(),
            breaking_changes,
            migration_from: Some(current.clone()),
            created_at: Utc::now(),
            author: self.get_current_author()?,
        };

        // Store in database with explicit column list (prevents schema/code mismatch)
        self.db.execute(
            "INSERT INTO skill_versions (
                skill_id, version, changelog, breaking_changes_json,
                migration_from, created_at, author
            ) VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![
                skill_id,
                version.version.to_string(),
                version.changelog,
                serde_json::to_string(&version.breaking_changes)?,
                version.migration_from.as_ref().map(|v| v.to_string()),
                version.created_at.to_rfc3339(),
                version.author,
            ],
        )?;

        // Create git tag (in transaction - rollback DB if tag fails)
        if let Err(e) = self.git.tag(
            &format!("{}-v{}", skill_id, new_version),
            &format!("Skill {} version {}\n\n{}", skill_id, new_version, changelog),
        ) {
            // Rollback: delete the version we just inserted
            self.db.execute(
                "DELETE FROM skill_versions WHERE skill_id = ? AND version = ?",
                params![skill_id, new_version.to_string()],
            )?;
            return Err(e);
        }

        Ok(version)
    }

    /// Get latest available version (what's in registry)
    pub fn get_latest_available(&self, skill_id: &str) -> Result<Option<Version>> {
        let version_str: Option<String> = self.db.query_row(
            "SELECT version FROM skill_versions WHERE skill_id = ?
             ORDER BY version DESC LIMIT 1",
            params![skill_id],
            |row| row.get(0),
        ).optional()?;

        version_str.map(|s| Version::parse(&s).map_err(Into::into)).transpose()
    }

    /// Get installed version (what's active locally)
    pub fn get_installed(&self, skill_id: &str) -> Result<Option<InstalledSkill>> {
        self.db.query_row(
            "SELECT installed_version, pinned_version, installed_at, source
             FROM installed_skills WHERE skill_id = ?",
            params![skill_id],
            |row| Ok(InstalledSkill {
                skill_id: skill_id.to_string(),
                installed_version: Version::parse(&row.get::<_, String>(0)?).unwrap(),
                pinned_version: row.get::<_, Option<String>>(1)?
                    .map(|s| Version::parse(&s).unwrap()),
                installed_at: DateTime::parse_from_rfc3339(&row.get::<_, String>(2)?)
                    .unwrap().with_timezone(&Utc),
                source: row.get(3)?,
            }),
        ).optional().map_err(Into::into)
    }

    /// Check if skill needs update (installed < available, not pinned)
    pub fn needs_update(&self, skill_id: &str) -> Result<bool> {
        let installed = self.get_installed(skill_id)?;
        let available = self.get_latest_available(skill_id)?;

        match (installed, available) {
            (Some(i), Some(a)) => {
                // Pinned skills never auto-update
                if i.pinned_version.is_some() {
                    return Ok(false);
                }
                Ok(a > i.installed_version)
            }
            _ => Ok(false),
        }
    }

    /// Pin skill to specific version (prevent auto-update)
    pub fn pin(&self, skill_id: &str, version: &Version) -> Result<()> {
        self.db.execute(
            "UPDATE installed_skills SET pinned_version = ? WHERE skill_id = ?",
            params![version.to_string(), skill_id],
        )?;
        Ok(())
    }

    /// Unpin skill (allow auto-update)
    pub fn unpin(&self, skill_id: &str) -> Result<()> {
        self.db.execute(
            "UPDATE installed_skills SET pinned_version = NULL WHERE skill_id = ?",
            params![skill_id],
        )?;
        Ok(())
    }

    /// Detect version bump type from changes
    pub fn detect_bump_type(&self, skill_id: &str, new_content: &str) -> Result<BumpType> {
        let current = self.get_current_skill(skill_id)?;
        let diff = self.compute_diff(&current.content, new_content)?;

        // Analyze diff for breaking changes
        let has_breaking = self.detect_breaking_changes(&diff);
        if !has_breaking.is_empty() {
            return Ok(BumpType::Major);
        }

        // Check for new content
        let has_new_sections = diff.additions.iter()
            .any(|a| a.starts_with("## ") || a.starts_with("### "));
        if has_new_sections {
            return Ok(BumpType::Minor);
        }

        // Default to patch
        Ok(BumpType::Patch)
    }

    /// Detect breaking changes in diff
    fn detect_breaking_changes(&self, diff: &ContentDiff) -> Vec<BreakingChange> {
        let mut breaking = Vec::new();

        // Check for removed CRITICAL RULES
        for removal in &diff.removals {
            if removal.contains("⚠️") || removal.contains("CRITICAL") || removal.contains("NEVER") {
                breaking.push(BreakingChange {
                    description: "Critical rule removed".to_string(),
                    migration_hint: "Verify the rule is no longer applicable".to_string(),
                    affected_sections: vec!["CRITICAL RULES".to_string()],
                });
            }
        }

        // Check for changed command syntax
        for (old, new) in diff.modifications.iter() {
            if old.contains("```bash") && new.contains("```bash") {
                let old_cmd = self.extract_command(old);
                let new_cmd = self.extract_command(new);
                if old_cmd != new_cmd {
                    breaking.push(BreakingChange {
                        description: format!("Command changed from '{}' to '{}'", old_cmd, new_cmd),
                        migration_hint: "Update any scripts using the old command".to_string(),
                        affected_sections: vec![],
                    });
                }
            }
        }

        breaking
    }
}

#[derive(Debug, Clone)]
pub enum BumpType {
    Major,
    Minor,
    Patch,
}
```

### 25.4 Migration Runner

```rust
/// Run migrations to update skill usage
pub struct MigrationRunner {
    version_manager: VersionManager,
}

impl MigrationRunner {
    /// Check if migration is needed
    pub fn check_migration_needed(&self, skill_id: &str, installed_version: &Version) -> Result<Option<MigrationPlan>> {
        let history = self.version_manager.get_history(skill_id)?;

        if installed_version >= &history.current {
            return Ok(None);
        }

        let path = history.migration_path(installed_version, &history.current)
            .ok_or_else(|| anyhow!("No migration path found"))?;

        let total_steps: usize = path.iter().map(|m| m.steps.len()).sum();
        let automatic_steps: usize = path.iter()
            .flat_map(|m| &m.steps)
            .filter(|s| s.automatic)
            .count();

        Ok(Some(MigrationPlan {
            from: installed_version.clone(),
            to: history.current.clone(),
            migrations: path,
            total_steps,
            automatic_steps,
            manual_steps: total_steps - automatic_steps,
        }))
    }

    /// Execute migration
    pub fn run_migration(&self, plan: &MigrationPlan) -> Result<MigrationResult> {
        let mut result = MigrationResult {
            success: true,
            completed_steps: vec![],
            failed_step: None,
            manual_steps_remaining: vec![],
        };

        for migration in &plan.migrations {
            for step in &migration.steps {
                if step.automatic {
                    match self.execute_step(step) {
                        Ok(_) => {
                            result.completed_steps.push(step.description.clone());
                        }
                        Err(e) => {
                            result.success = false;
                            result.failed_step = Some((step.description.clone(), e.to_string()));
                            return Ok(result);
                        }
                    }
                } else {
                    result.manual_steps_remaining.push(ManualStep {
                        description: step.description.clone(),
                        hint: step.hint.clone(),
                    });
                }
            }
        }

        Ok(result)
    }

    fn execute_step(&self, step: &MigrationStep) -> Result<()> {
        match step.action {
            MigrationAction::Update => {
                // Automatic content update
                Ok(())
            }
            MigrationAction::Rename => {
                // Handle rename automatically
                Ok(())
            }
            _ => Err(anyhow!("Step requires manual intervention")),
        }
    }
}

#[derive(Debug)]
pub struct MigrationPlan {
    pub from: Version,
    pub to: Version,
    pub migrations: Vec<Migration>,
    pub total_steps: usize,
    pub automatic_steps: usize,
    pub manual_steps: usize,
}

#[derive(Debug)]
pub struct MigrationResult {
    pub success: bool,
    pub completed_steps: Vec<String>,
    pub failed_step: Option<(String, String)>,
    pub manual_steps_remaining: Vec<ManualStep>,
}

#[derive(Debug)]
pub struct ManualStep {
    pub description: String,
    pub hint: String,
}
```

### 25.5 CLI Commands for Versioning

```bash
# View version history
ms version history rust-patterns
# → rust-patterns version history:
# →   v2.1.0 (current) - 2026-01-13 - Added async error patterns
# →   v2.0.0 - 2026-01-10 - Major refactor, split into focused skills
# →   v1.3.2 - 2026-01-05 - Fixed typos in examples
# →   v1.3.1 - 2026-01-01 - Clarified mutex patterns
# →   ...

# Create new version
ms version bump rust-patterns --minor --message "Added Result chain examples"
# → Created version 2.2.0

# Create major version with breaking changes
ms version bump rust-patterns --major \
    --message "Restructured for Rust 2024 edition" \
    --breaking "Error handling section moved to separate skill"
# → Created version 3.0.0
# → Breaking change recorded: Error handling section moved

# Check if migration needed
ms version check rust-patterns
# → Installed: v2.0.0
# → Available: v2.2.0
# → Migration: 2 automatic steps, 0 manual steps
# → Run 'ms version migrate rust-patterns' to update

# Run migration
ms version migrate rust-patterns
# → Migrating rust-patterns from v2.0.0 to v2.2.0...
# → ✓ Step 1: Update async patterns section
# → ✓ Step 2: Add new examples
# → Migration complete!

# View migration details before running
ms version migrate rust-patterns --dry-run
# → Migration plan:
# →   v2.0.0 → v2.1.0:
# →     [AUTO] Add async error patterns section
# →   v2.1.0 → v2.2.0:
# →     [AUTO] Update Result chain examples

# Compare versions
ms version diff rust-patterns v2.0.0 v2.2.0
# → [Shows unified diff between versions]

# Pin to specific version (don't auto-update)
ms version pin rust-patterns 2.1.0
# → Pinned rust-patterns to v2.1.0

# Unpin
ms version unpin rust-patterns
```

---

## 26. Real-World Pattern Mining: CASS Insights

This section documents actual patterns discovered by mining CASS sessions. These represent the "inner truths" that `ms build` should extract and transform into skills.

### 26.1 Discovered Skill Candidates

#### Pattern 1: UI Polish Checklist (from brenner_bot sessions)

**Source Sessions:** `/home/ubuntu/.claude/projects/-data-projects-brenner-bot/agent-a9a6d6d.jsonl`

**Recurring Categories:**
```
UI Polish Checklist
├── Touch Interactions
│   ├── touch-manipulation (mobile tap response)
│   ├── active:scale-* (press feedback, e.g., active:scale-[0.98])
│   └── min 44px touch targets
├── Focus States
│   ├── focus-visible: (NOT focus:) for keyboard a11y
│   └── focus-visible:ring-2 focus-visible:ring-ring
├── Accessibility
│   ├── aria-label on icon-only buttons
│   ├── aria-pressed for toggle buttons
│   ├── aria-selected for tabs/selections
│   └── aria-hidden="true" on decorative elements
├── Motion Sensitivity
│   ├── useReducedMotion() hook
│   ├── motion-reduce:* Tailwind variants
│   └── prefers-reduced-motion media query
└── Transitions
    ├── transition-colors (NOT transition-all unless needed)
    └── Consistent duration (150ms standard)
```

**Report Format (from sessions):**
```
| File | Line | Issue | Fix |
|------|------|-------|-----|
| path/to/file.tsx | 167 | Missing touch-manipulation | Add `touch-manipulation active:scale-[0.95]` |
```

**Inner Truth → Skill:**
```yaml
name: nextjs-ui-polish
description: Systematic UI polish checklist for Next.js/React apps with Tailwind
tags: [nextjs, react, tailwind, accessibility, mobile]
```

---

#### Pattern 2: Iterative Convergence (from automated_plan_reviser_pro)

**Source Sessions:** `/home/ubuntu/.claude/projects/-data-projects-automated-plan-reviser-pro/`

**The Convergence Pattern:**
> "Specifications improve through multiple iterations like numerical optimization converging to steady state"

**Round Progression Heuristics:**
```rust
pub struct ConvergenceProfile {
    pub round_expectations: Vec<RoundExpectation>,
}

impl Default for ConvergenceProfile {
    fn default() -> Self {
        Self {
            round_expectations: vec![
                RoundExpectation {
                    rounds: 1..=2,
                    label: "Major Fixes",
                    expected_changes: ChangeLevel::Significant,
                    focus_areas: vec![
                        "Security gaps",
                        "Architectural issues",
                        "Critical bugs",
                    ],
                },
                RoundExpectation {
                    rounds: 3..=5,
                    label: "Architecture Refinement",
                    expected_changes: ChangeLevel::Moderate,
                    focus_areas: vec![
                        "Feature completeness",
                        "Interface design",
                        "Edge cases",
                    ],
                },
                RoundExpectation {
                    rounds: 6..=10,
                    label: "Fine-tuning",
                    expected_changes: ChangeLevel::Minor,
                    focus_areas: vec![
                        "Performance",
                        "Abstractions",
                        "Error messages",
                    ],
                },
                RoundExpectation {
                    rounds: 11..=20,
                    label: "Polish",
                    expected_changes: ChangeLevel::Minimal,
                    focus_areas: vec![
                        "Documentation",
                        "Edge cases",
                        "Consistency",
                    ],
                },
            ],
        }
    }
}
```

**Steady-State Detection:**
```rust
pub fn detect_steady_state(
    round_outputs: &[RoundOutput],
    threshold: f32,
) -> SteadyStateResult {
    if round_outputs.len() < 3 {
        return SteadyStateResult::InsufficientData;
    }

    let recent = &round_outputs[round_outputs.len()-3..];
    let deltas: Vec<f32> = recent.windows(2)
        .map(|w| compute_semantic_delta(&w[0], &w[1]))
        .collect();

    let avg_delta = deltas.iter().sum::<f32>() / deltas.len() as f32;

    if avg_delta < threshold {
        SteadyStateResult::Converged {
            final_round: round_outputs.len(),
            avg_delta,
        }
    } else {
        SteadyStateResult::StillConverging {
            current_delta: avg_delta,
            threshold,
            estimated_rounds_remaining: estimate_remaining(avg_delta, threshold),
        }
    }
}
```

---

#### Pattern 3: Brenner Principles Extraction (from brenner_bot)

**Methodology Pattern:**
Sessions reveal extraction of "AppliedPrinciples" from specific instances:

```rust
pub struct AppliedPrinciple {
    pub name: String,           // e.g., "Epistemic Humility"
    pub explanation: String,    // How it was applied
    pub source_line: usize,     // Where detected
    pub confidence: f32,        // 0.0-1.0 detection confidence
}

pub fn extract_principles(
    session_content: &str,
    principle_keywords: &[PrincipleKeyword],
) -> Vec<AppliedPrinciple> {
    let mut found = Vec::new();

    for keyword in principle_keywords {
        for (line_num, line) in session_content.lines().enumerate() {
            if keyword.matches(line) {
                found.push(AppliedPrinciple {
                    name: keyword.principle_name.clone(),
                    explanation: extract_context(session_content, line_num, 3),
                    source_line: line_num,
                    confidence: keyword.compute_confidence(line),
                });
            }
        }
    }

    // Deduplicate and limit
    found.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
    found.truncate(6); // Top 6 for conciseness
    found
}
```

**Inner Truth:** Domain expertise can be encoded as keyword → principle mappings, then extracted from sessions automatically.

---

#### Pattern 4: Accessibility Standards (multi-project)

**Recurring Pattern Across Sessions:**
```typescript
// Pattern: useReducedMotion hook
const prefersReducedMotion = useReducedMotion();

// Pattern: Conditional animation
<motion.div
  animate={{ opacity: 1 }}
  transition={prefersReducedMotion ? { duration: 0 } : { duration: 0.3 }}
>

// Pattern: aria-hidden on decorative
<ArrowIcon aria-hidden="true" />

// Pattern: focus-visible not focus
className="focus-visible:ring-2 focus-visible:ring-ring"
// NOT: className="focus:ring-2 focus:ring-ring"
```

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

```markdown
---
name: ui-polish-nextjs
description: Systematic UI polish for Next.js/React/Tailwind apps. Run as iterative checklist.
version: 1.0.0
tags: [nextjs, react, tailwind, accessibility, mobile, ui]
---

# Next.js UI Polish Skill

Systematic checklist for polishing Next.js/React applications with Tailwind CSS.

## ⚠️ CRITICAL RULES

1. ALWAYS use `focus-visible:` NOT `focus:` for focus rings
2. ALWAYS add `touch-manipulation` to clickable elements
3. NEVER use `transition-all` when specific properties suffice
4. ALWAYS check `prefers-reduced-motion` for animations

## Checklist Categories

### 1. Touch Interactions
- [ ] All buttons have `touch-manipulation`
- [ ] All buttons have `active:scale-[0.98]` or similar
- [ ] Touch targets are minimum 44x44px on mobile

### 2. Focus States
- [ ] Using `focus-visible:ring-2` NOT `focus:ring-2`
- [ ] Focus rings have proper offset
- [ ] All interactive elements have visible focus states

### 3. Accessibility
- [ ] Icon-only buttons have `aria-label`
- [ ] Toggle buttons have `aria-pressed`
- [ ] Decorative elements have `aria-hidden="true"`
- [ ] Form inputs have associated labels

### 4. Motion
- [ ] Animations respect `prefers-reduced-motion`
- [ ] Using `useReducedMotion()` hook where needed
- [ ] Infinite animations can be disabled

### 5. Transitions
- [ ] Using specific properties (`transition-colors`, `transition-transform`)
- [ ] Consistent durations (150ms for micro, 300ms for larger)

## Quick Audit Commands

\`\`\`bash
# Find missing touch-manipulation
rg "onClick|cursor-pointer" --type tsx | rg -v "touch-manipulation"

# Find incorrect focus usage
rg "focus:ring" --type tsx

# Find transition-all usage
rg "transition-all" --type tsx

# Find missing aria-labels on buttons
ast-grep -l tsx -p '<button $$$><Icon /></button>'
\`\`\`

## Example Fixes

**Before:**
\`\`\`tsx
<button onClick={handle} className="cursor-pointer focus:ring-2">
  <Icon />
</button>
\`\`\`

**After:**
\`\`\`tsx
<button
  onClick={handle}
  className="cursor-pointer touch-manipulation active:scale-[0.98] focus-visible:ring-2"
  aria-label="Action description"
>
  <Icon aria-hidden="true" />
</button>
\`\`\`
```

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

```bash
# Category-specific searches
cass search "touch-manipulation focus-visible" --robot --limit 20
cass search "aria-label aria-pressed accessibility" --robot --limit 20
cass search "useReducedMotion prefers-reduced-motion" --robot --limit 20

# Methodology searches
cass search "iterate refine polish rounds" --robot --limit 10
cass search "steady state convergence" --robot --limit 10
cass search "extract principles pattern" --robot --limit 10

# Project-specific deep dives
cass search "UI polish" --robot --limit 20  # Then filter by workspace

# View session context
cass view <session_path> -n <line_number> --json
cass expand <session_path> -n <line_number> -C 5 --json
```

**Query expansion strategy:**
1. Start with exact phrase: `"inner truth"`
2. Expand to component terms: `inner`, `truth`, `abstract`
3. Add synonyms: `general`, `principles`, `universal`
4. Add domain context: `pattern`, `extract`, `lesson`

---

### 26.5 Inner Truth Extraction Algorithm

Based on session analysis, here's the refined extraction algorithm:

```rust
pub struct InnerTruthExtractor {
    /// Terms that indicate generalizable knowledge
    generalization_markers: Vec<&'static str>,
    /// Terms that indicate specific instances
    specificity_markers: Vec<&'static str>,
    /// Minimum occurrences to consider a pattern
    min_pattern_occurrences: usize,
}

impl Default for InnerTruthExtractor {
    fn default() -> Self {
        Self {
            generalization_markers: vec![
                "always", "never", "pattern", "principle",
                "best practice", "checklist", "systematic",
                "every", "all", "any", "standard",
            ],
            specificity_markers: vec![
                "this file", "line 42", "in this case",
                "here specifically", "for this project",
            ],
            min_pattern_occurrences: 3,
        }
    }
}

impl InnerTruthExtractor {
    pub fn extract(&self, sessions: &[Session]) -> Vec<InnerTruth> {
        let mut candidates: HashMap<String, PatternCandidate> = HashMap::new();

        for session in sessions {
            for segment in session.segments() {
                // Score generalization potential
                let gen_score = self.generalization_score(&segment);
                let spec_score = self.specificity_score(&segment);

                // High generalization, low specificity = inner truth candidate
                if gen_score > 0.6 && spec_score < 0.3 {
                    let key = self.normalize_pattern(&segment);
                    candidates.entry(key)
                        .or_insert_with(PatternCandidate::new)
                        .add_occurrence(session.id(), segment.clone());
                }
            }
        }

        // Filter by minimum occurrences
        candidates.into_iter()
            .filter(|(_, c)| c.occurrences >= self.min_pattern_occurrences)
            .map(|(pattern, candidate)| InnerTruth {
                pattern,
                occurrences: candidate.occurrences,
                sessions: candidate.session_ids,
                confidence: candidate.avg_confidence(),
                suggested_skill_content: self.generate_skill_section(&candidate),
            })
            .collect()
    }
}
```

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

```
### SUMMARY OF ISSUES BY CATEGORY:

**Focus Styling Issues (focus: vs focus-visible:):**
- input.tsx (line 44)
- textarea.tsx (line 54)
- nav.tsx (line 417)
- copy-button.tsx (lines 159-181)

**Missing Active Scale Feedback:**
- command-palette.tsx (line 308)
- nav.tsx (lines 187, 343)
- corpus/page.tsx (lines 536, 597, 727)
```

### A.2 Iterative Refinement Session Excerpts

**Session:** automated_plan_reviser_pro exploration
**Key Finding:** Round progression pattern

```
Round 1-2:    Major fixes, security gaps, architectural issues
Round 3-5:    Architecture improvements, feature refinements
Round 6-10:   Fine-tuning abstractions, interfaces, performance
Round 11+:    Polish, documentation, edge cases
```

### A.3 Accessibility Pattern Excerpts

**Multi-project recurring pattern:**
```tsx
// Infinite animations not respecting reduced motion fix:
const prefersReducedMotion = useReducedMotion();
transition={prefersReducedMotion ? { duration: 0 } : springConfig}

// aria-hidden on decorative elements:
<ArrowIcon aria-hidden="true" />

// Proper focus styling:
className="focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-offset-2"
```

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

```
SKILL EXTRACTION LOOP (30-60 min)
─────────────────────────────────
A: SESSION SELECTION → 5-10 candidate sessions
B: COGNITIVE MOVE EXTRACTION → 8-12 moves with evidence
C: THIRD-ALTERNATIVE GUARD → filtered list with confidence
D: SKILL FORMALIZATION → candidate SKILL.md
E: MATERIALIZATION TEST → empirical validation
F: CALIBRATION → documented limitations
```

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

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    BRENNER EXTRACTION WIZARD                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐    ┌─────────┐  │
│  │ SELECT  │───▶│ EXTRACT │───▶│  GUARD  │───▶│FORMALIZE│───▶│  TEST   │  │
│  │Sessions │    │ Moves   │    │ (Filter)│    │ (SKILL) │    │& Refine │  │
│  └─────────┘    └─────────┘    └─────────┘    └─────────┘    └─────────┘  │
│       │              │              │              │              │        │
│       ▼              ▼              ▼              ▼              ▼        │
│   TUI Panel:     TUI Panel:     TUI Panel:     TUI Panel:     TUI Panel:  │
│   Search/pick    See moves,     Confidence     Live preview   Run tests,  │
│   sessions       add evidence   threshold      & edit skill   see results │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### CLI Interface

```bash
# Launch guided wizard
ms mine --guided
ms mine --guided --query "authentication patterns"  # Pre-seed with query

# Resume interrupted wizard session
ms mine --guided --resume abc123
```

#### TUI Screens

**Screen 1: Session Selection**
```
┌─ Session Selection ─────────────────────────────────────────────────────────┐
│                                                                             │
│ Search: [authentication error handling____________]                        │
│                                                                             │
│ ┌─ Results (23 sessions) ─────────────────────────────────────────────────┐│
│ │ [x] 2026-01-10 brenner-bot: OAuth2 token refresh bug fix (★★★★)        ││
│ │ [x] 2026-01-08 brenner-bot: JWT validation edge cases (★★★★)           ││
│ │ [ ] 2026-01-05 xf-project: Rate limit handling (★★★)                    ││
│ │ [x] 2025-12-28 ms-project: Auth middleware refactor (★★★★★)             ││
│ │ [ ] 2025-12-20 cass: Session token rotation (★★★)                       ││
│ └─────────────────────────────────────────────────────────────────────────┘│
│                                                                             │
│ Selected: 3 sessions │ Min recommended: 3-5 │ Quality filter: ★★★+         │
│                                                                             │
│ [Tab: Filter by project] [Enter: Continue] [q: Quit]                       │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Screen 2: Cognitive Move Extraction**
```
┌─ Cognitive Move Extraction ─────────────────────────────────────────────────┐
│                                                                             │
│ Extracted 12 cognitive moves from 3 sessions:                               │
│                                                                             │
│ ┌─ Move: "Always validate token expiry before use" ──────────────────────┐ │
│ │ Tag: [InnerTruth]  Confidence: 0.89  Sources: 3/3 sessions              │ │
│ │                                                                          │ │
│ │ Evidence:                                                                │ │
│ │   • brenner-bot/oauth2: "Check exp claim first, avoid round-trip"       │ │
│ │   • brenner-bot/jwt: "Token validation: expiry > signature > claims"    │ │
│ │   • ms-project/auth: "Pre-flight token check saves network calls"       │ │
│ │                                                                          │ │
│ │ [Include ✓] [Exclude] [Edit] [Add Evidence]                              │ │
│ └──────────────────────────────────────────────────────────────────────────┘│
│                                                                             │
│ Progress: 4/12 moves reviewed │ Included: 3 │ Excluded: 1                   │
│                                                                             │
│ [j/k: Navigate] [Space: Toggle] [e: Edit] [Enter: Next] [S: Skip to Guard] │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Screen 3: Third-Alternative Guard**
```
┌─ Third-Alternative Guard ───────────────────────────────────────────────────┐
│                                                                             │
│ Review low-confidence moves for potential rejection:                        │
│                                                                             │
│ ┌─ ⚠️  "Use refresh tokens for long-lived sessions" (0.62 confidence) ───┐ │
│ │                                                                          │ │
│ │ This move appears in 2/3 sessions but with conflicting approaches:       │ │
│ │   • Session 1: "Always use refresh tokens"                               │ │
│ │   • Session 3: "Refresh tokens add complexity, use short-lived JWTs"     │ │
│ │                                                                          │ │
│ │ Third-Alternative Check:                                                 │ │
│ │   Could BOTH approaches be wrong? Is there a better framing?             │ │
│ │                                                                          │ │
│ │ Decision: [Keep as-is] [Reject] [Reframe] [Mark as context-dependent]    │ │
│ └──────────────────────────────────────────────────────────────────────────┘│
│                                                                             │
│ Confidence threshold: [0.70 ▼] │ Flagged: 3 │ Reviewed: 1/3                 │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Screen 4: Skill Formalization (Live Editor)**
```
┌─ Skill Formalization ───────────────────────────────────────────────────────┐
│                                                                             │
│ ┌─ SKILL.md Preview ──────────────────────────────────────────────────────┐│
│ │ ---                                                                      ││
│ │ name: auth-token-patterns                                                ││
│ │ description: Authentication token handling patterns from production     ││
│ │ version: 1.0.0                                                           ││
│ │ tags: [auth, jwt, oauth, security]                                       ││
│ │ ---                                                                      ││
│ │                                                                          ││
│ │ # Authentication Token Patterns                                          ││
│ │                                                                          ││
│ │ ## ⚠️ CRITICAL RULES                                                    ││
│ │                                                                          ││
│ │ 1. ALWAYS validate token expiry before making authenticated requests    ││
│ │ 2. NEVER store tokens in localStorage (use httpOnly cookies)            ││
│ │ 3. ALWAYS implement token refresh 5 minutes before expiry               ││
│ │                                                                          ││
│ │ ## Token Validation Order                                                ││
│ │ ...                                                                      ││
│ └──────────────────────────────────────────────────────────────────────────┘│
│                                                                             │
│ Quality Score: 0.78 │ Token Count: ~1,240 │ Validation: ✓ Passes            │
│                                                                             │
│ [e: Edit in $EDITOR] [r: Regenerate section] [v: Validate] [Enter: Test]   │
└─────────────────────────────────────────────────────────────────────────────┘
```

**Screen 5: Materialization Test**
```
┌─ Materialization Test ──────────────────────────────────────────────────────┐
│                                                                             │
│ Running validation tests for: auth-token-patterns                           │
│                                                                             │
│ ┌─ Test Results ──────────────────────────────────────────────────────────┐│
│ │ ✓ Skill parses correctly                                                 ││
│ │ ✓ All required sections present                                          ││
│ │ ✓ Tags are valid and indexed                                             ││
│ │ ✓ Search retrieval: ranks #1 for "jwt validation" query                  ││
│ │ ✓ Search retrieval: ranks #2 for "oauth token refresh" query             ││
│ │ ⚠ Search retrieval: ranks #5 for "auth middleware" (expected top 3)      ││
│ │                                                                          ││
│ │ Simulated agent test:                                                    ││
│ │ ✓ Agent correctly applies "validate expiry first" rule                   ││
│ │ ✓ Agent identifies security issue in test scenario                       ││
│ └──────────────────────────────────────────────────────────────────────────┘│
│                                                                             │
│ Overall: 8/9 tests passing │ Quality: GOOD                                  │
│                                                                             │
│ [f: Fix failing test] [s: Save skill] [r: Return to edit] [Enter: Finish]  │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Wizard Output Artifacts

On completion, the wizard produces:

```
auth-token-patterns/
├── SKILL.md                    # The generated skill
├── tests/
│   └── retrieval.yaml          # Auto-generated search tests
├── mining-manifest.json        # Provenance: sessions, moves, decisions
└── calibration.md              # Documented limitations from Guard phase
```

#### Implementation

```rust
/// Guided Brenner extraction wizard state machine
pub struct BrennerWizard {
    state: WizardState,
    sessions: Vec<SelectedSession>,
    moves: Vec<CognitiveMove>,
    skill_draft: Option<SkillDraft>,
    test_results: Option<TestResults>,
}

#[derive(Debug, Clone)]
pub enum WizardState {
    SessionSelection { query: String, results: Vec<SessionResult> },
    MoveExtraction { current: usize, reviewed: HashSet<usize> },
    ThirdAlternativeGuard { flagged: Vec<usize>, current: usize },
    SkillFormalization { draft: SkillDraft, validation: ValidationResult },
    MaterializationTest { results: TestResults },
    Complete { output_dir: PathBuf },
}

impl BrennerWizard {
    /// Run the interactive wizard
    pub async fn run(&mut self, terminal: &mut Terminal) -> Result<WizardOutput> {
        loop {
            match &self.state {
                WizardState::SessionSelection { .. } => {
                    self.render_session_selection(terminal)?;
                    match self.handle_session_selection_input().await? {
                        WizardAction::Next => self.transition_to_extraction().await?,
                        WizardAction::Quit => return Ok(WizardOutput::Cancelled),
                        _ => {}
                    }
                }
                WizardState::MoveExtraction { .. } => {
                    self.render_move_extraction(terminal)?;
                    // ... handle input, transition when all moves reviewed
                }
                WizardState::ThirdAlternativeGuard { .. } => {
                    self.render_guard(terminal)?;
                    // ... handle input, filter low-confidence moves
                }
                WizardState::SkillFormalization { .. } => {
                    self.render_formalization(terminal)?;
                    // ... live preview, edit, validate
                }
                WizardState::MaterializationTest { .. } => {
                    self.render_tests(terminal)?;
                    // ... run tests, show results, allow fixes
                }
                WizardState::Complete { output_dir } => {
                    return Ok(WizardOutput::Success {
                        skill_path: output_dir.join("SKILL.md"),
                        manifest_path: output_dir.join("mining-manifest.json"),
                    });
                }
            }
        }
    }

    /// Allow resuming interrupted wizard session
    pub fn resume(checkpoint: WizardCheckpoint) -> Result<Self> {
        Ok(Self {
            state: checkpoint.state,
            sessions: checkpoint.sessions,
            moves: checkpoint.moves,
            skill_draft: checkpoint.skill_draft,
            test_results: None,
        })
    }
}
```

---

## Section 29: APR Iterative Refinement Patterns

*CASS Mining Deep Dive: automated_plan_reviser_pro methodology (P1 bead: meta_skill-hzg)*

### 29.1 The Numerical Optimizer Analogy

The APR project reveals a powerful insight: **iterative specification refinement follows the same dynamics as numerical optimization**.

> "It very much reminds me of a numerical optimizer gradually converging on a steady state after wild swings in the initial iterations."

**Application to meta_skill:** When building skills through CASS mining, expect early iterations to produce wild swings (major restructures, foundational changes). Later iterations converge on stable formulations. Don't judge early work—judge the convergence trajectory.

### 29.2 The Convergence Pattern

Refinement progresses through predictable phases:

```
┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐
│   Round 1-3     │────▶│   Round 4-7     │────▶│   Round 8-12    │────▶ ...
│   Major fixes   │     │  Architecture   │     │  Refinements    │
│   Security gaps │     │  improvements   │     │  Optimizations  │
└─────────────────┘     └─────────────────┘     └─────────────────┘
        │                       │                       │
        ▼                       ▼                       ▼
   Wild swings            Dampening              Converging
   in design              oscillations           on optimal
```

| Phase | Rounds | Focus |
|-------|--------|-------|
| **Major Fixes** | 1-3 | Security gaps, architectural flaws, fundamental issues |
| **Architecture** | 4-7 | Interface improvements, component boundaries |
| **Refinement** | 8-12 | Edge cases, optimizations, nuanced handling |
| **Polishing** | 13+ | Final abstractions, converging on steady state |

**Key insight:** In early rounds, reviewers focus on "putting out fires." Once major issues are addressed, they can apply "considerable intellectual energies on nuanced particulars."

### 29.3 Convergence Analytics Algorithm

APR implements a quantitative convergence detector using three weighted signals:

```
Convergence Score = (0.35 × output_trend) + (0.35 × change_velocity) + (0.30 × similarity_trend)
```

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

```
┌─────────────────────────────────────────────────────────────────────────┐
│  GROUNDED ABSTRACTION CYCLE                                              │
├─────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  Round 1 ──▶ Round 2 ──▶ Round 3 ──▶ [INCLUDE IMPL] ──▶ Round 4 ──▶ ... │
│    │            │            │              │               │            │
│    └────────────┴────────────┴──────────────┤               │            │
│           Abstract Refinement               │               │            │
│                                             ▼               │            │
│                                   Surface Assumptions       │            │
│                                   Validate Feasibility      │            │
│                                                             │            │
│                          ┌──────────────────────────────────┘            │
│                          │                                               │
│                          ▼                                               │
│                   Feedback Loop: Faulty assumptions                      │
│                   surface earlier when ideas meet code                   │
│                                                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

**Application to meta_skill:** When extracting skills from CASS sessions, periodically test them:
- Can the skill actually be loaded and executed?
- Does the skill produce expected outputs?
- Do agents understand and apply the skill correctly?

### 29.5 Reliability Features for Long Operations

APR implements several reliability patterns for expensive operations:

#### Pre-Flight Validation
Check all preconditions before starting expensive work:
```
Pre-flight checks:
- Required files exist
- Previous dependencies satisfied
- Resources available
- Configuration valid
```

**Application to meta_skill:** Before running expensive CASS operations:
- Verify index is up-to-date
- Check disk space for embeddings
- Validate query parameters
- Confirm output paths writable

#### Auto-Retry with Exponential Backoff
```
Attempt 1 → fail → wait 10s
Attempt 2 → fail → wait 30s  (10s × 3)
Attempt 3 → fail → wait 90s  (30s × 3)
Attempt 4 → success (or final failure)
```

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

```rust
// meta_skill application: --robot flag
pub struct OutputMode {
    human: bool,  // Pretty TUI output
    robot: bool,  // Structured JSON output
}

// Robot mode JSON envelope
{
    "ok": true,
    "code": "ok",
    "data": { ... },
    "hint": "Optional debugging message",
    "meta": { "v": "1.0.0", "ts": "2026-01-13T00:00:00Z" }
}
```

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

```rust
// For small K relative to N, use a min-heap
use std::collections::BinaryHeap;

// For larger K, consider quickselect (O(n) average)
// Rust's select_nth_unstable is quickselect-based
let mut data = vec![...];
data.select_nth_unstable_by(k, |a, b| b.cmp(a));
let top_k = &data[..k];
```

### 30.4 SIMD and Vectorization

#### Memory Layout Considerations

| Layout | Description | SIMD Friendly |
|--------|-------------|---------------|
| **AoS** | Array of Structs: `[{x,y,z}, {x,y,z}]` | ❌ Poor |
| **SoA** | Struct of Arrays: `{xs: [], ys: [], zs: []}` | ✅ Excellent |

```rust
// SoA for SIMD-friendly vector operations
struct VectorIndex {
    xs: Vec<f32>,  // Contiguous x components
    ys: Vec<f32>,  // Contiguous y components
    zs: Vec<f32>,  // Contiguous z components
}
```

#### SIMD Dot Product Pattern

```rust
use wide::f32x8;

pub fn dot_product_simd(a: &[f32], b: &[f32]) -> f32 {
    let chunks = a.len() / 8;
    let mut sum = f32x8::ZERO;
    
    for i in 0..chunks {
        let va = f32x8::from(&a[i * 8..][..8]);
        let vb = f32x8::from(&b[i * 8..][..8]);
        sum += va * vb;
    }
    
    let mut result: f32 = sum.reduce_add();
    
    // Handle remainder
    for i in (chunks * 8)..a.len() {
        result += a[i] * b[i];
    }
    
    result
}
```

#### Quantization (F16 Storage)

```rust
use half::f16;

// Store as F16 for 50% memory reduction
fn quantize_vector(v: &[f32]) -> Vec<f16> {
    v.iter().map(|&x| f16::from_f32(x)).collect()
}

// Dequantize for computation
fn dequantize_vector(v: &[f16]) -> Vec<f32> {
    v.iter().map(|x| x.to_f32()).collect()
}
```

### 30.5 Criterion Benchmark Patterns

#### Basic Benchmark Structure

```rust
use criterion::{black_box, criterion_group, criterion_main, Criterion, BatchSize};

fn bench_operation(c: &mut Criterion) {
    // Setup that runs once
    let data = setup_test_data();
    
    c.bench_function("operation_name", |b| {
        b.iter(|| {
            // black_box prevents compiler from optimizing away the result
            black_box(expensive_operation(black_box(&data)))
        })
    });
}

criterion_group!(benches, bench_operation);
criterion_main!(benches);
```

#### Batched Benchmarks (Setup/Teardown Separation)

```rust
c.bench_function("operation_with_setup", |b| {
    b.iter_batched(
        || {
            // Setup: runs before each batch
            create_fresh_state()
        },
        |state| {
            // Benchmark: only this is measured
            operate_on_state(state)
        },
        BatchSize::SmallInput,  // or LargeInput for expensive setup
    );
});
```

#### Benchmark Groups for Comparison

```rust
fn bench_scaling(c: &mut Criterion) {
    let mut group = c.benchmark_group("search_scaling");
    group.sample_size(20);  // Fewer samples for expensive operations
    
    for size in [100, 500, 1000, 5000] {
        let data = setup_data(size);
        group.bench_with_input(
            format!("size_{}", size),
            &data,
            |b, d| b.iter(|| search(black_box(d)))
        );
    }
    
    group.finish();
}
```

#### Parallel vs Sequential Comparison

```rust
use rayon::prelude::*;

fn bench_parallelization(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_vs_sequential");
    let data = setup_data();
    
    group.bench_function("sequential", |b| {
        b.iter(|| {
            let result: Vec<_> = data.iter()
                .map(|x| process(x))
                .collect();
            black_box(result)
        })
    });
    
    group.bench_function("parallel", |b| {
        b.iter(|| {
            let result: Vec<_> = data.par_iter()
                .map(|x| process(x))
                .collect();
            black_box(result)
        })
    });
    
    group.finish();
}
```

### 30.6 Profiling Build Configuration

#### Cargo Profile for Profiling

```toml
# Cargo.toml - optimized build with debug symbols for profilers
[profile.profiling]
inherits = "release"
debug = true        # Keep debug symbols
strip = false       # Don't strip symbols

# Build with frame pointers for accurate flamegraphs
# RUSTFLAGS="-C force-frame-pointers=yes" cargo build --profile profiling
```

#### Profiling Workflow

```bash
# 1. Build with profiling profile
RUSTFLAGS="-C force-frame-pointers=yes" cargo build --profile profiling

# 2. Run with perf (Linux)
perf record -g ./target/profiling/mybinary --some-args

# 3. Generate flamegraph
perf script | stackcollapse-perf.pl | flamegraph.pl > flame.svg

# 4. Or use cargo-flamegraph
cargo flamegraph --profile profiling -- --some-args
```

### 30.7 I/O and Serialization Optimization

#### Memory-Mapped Files

```rust
use memmap2::Mmap;
use std::fs::File;

fn mmap_read(path: &Path) -> Result<Mmap> {
    let file = File::open(path)?;
    // SAFETY: file is read-only, no concurrent modifications
    unsafe { Mmap::map(&file) }
}

// Benefits:
// - OS handles caching
// - Zero-copy access
// - Lazy loading (only touched pages loaded)
```

#### JSON Parsing Optimization

```rust
// Avoid: parsing entire file upfront
let data: Vec<Record> = serde_json::from_reader(file)?;

// Better: streaming/lazy parsing
use serde_json::Deserializer;
let stream = Deserializer::from_reader(file).into_iter::<Record>();
for record in stream {
    process(record?);
}
```

### 30.8 Cache Design Patterns

#### LRU Cache with TTL

```rust
use lru::LruCache;
use std::time::{Duration, Instant};

struct TtlCache<K, V> {
    cache: LruCache<K, (V, Instant)>,
    ttl: Duration,
}

impl<K: Hash + Eq, V: Clone> TtlCache<K, V> {
    fn get(&mut self, key: &K) -> Option<V> {
        if let Some((value, inserted)) = self.cache.get(key) {
            if inserted.elapsed() < self.ttl {
                return Some(value.clone());
            }
            // Expired - will be evicted on next insert
        }
        None
    }
    
    fn insert(&mut self, key: K, value: V) {
        self.cache.put(key, (value, Instant::now()));
    }
}
```

#### Fast Hash for Cache Keys

```rust
use fxhash::FxHashMap;

// FxHash is faster than std HashMap for small keys
// Good for cache keys, NOT for untrusted input (no DoS protection)
let cache: FxHashMap<String, Vec<u8>> = FxHashMap::default();
```

### 30.9 Parallel Processing Patterns

#### Rayon Work-Stealing

```rust
use rayon::prelude::*;

// Automatic work-stealing parallel iterator
let results: Vec<_> = items
    .par_iter()
    .filter(|x| expensive_filter(x))
    .map(|x| expensive_transform(x))
    .collect();

// Control thread pool size
rayon::ThreadPoolBuilder::new()
    .num_threads(4)
    .build_global()
    .unwrap();
```

#### Chunked Processing

```rust
// Process in chunks to balance parallelism overhead
const CHUNK_SIZE: usize = 1000;

let results: Vec<_> = items
    .par_chunks(CHUNK_SIZE)
    .flat_map(|chunk| {
        chunk.iter()
            .map(|x| process(x))
            .collect::<Vec<_>>()
    })
    .collect();
```

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

```bash
# 1. Run full test suite to establish correctness baseline
cargo test --all-features

# 2. Run representative workload with timing
time cargo run --release -- process large_dataset.json

# 3. Capture golden outputs for regression testing
cargo run --release -- process input.json > golden_output.json
```

**Key Principle**: Never optimize without knowing your starting point.

#### B) Profile Before Proposing

```bash
# CPU profiling with flamegraph
cargo flamegraph --root -- process input.json

# Memory allocation profiling
DHAT=1 cargo run --release -- process input.json

# I/O profiling (Linux)
strace -c -f cargo run --release -- process input.json

# Sampling profiler
cargo build --release
perf record -g ./target/release/meta_skill process input.json
perf report
```

**Anti-pattern**: Optimizing based on intuition rather than profiling data.

#### C) Equivalence Oracle

Define explicit verification criteria before making changes:

```rust
/// Equivalence oracle for optimization validation
struct OptimizationOracle {
    /// Golden outputs that must remain identical
    golden_outputs: HashMap<PathBuf, Vec<u8>>,
    
    /// Invariants that must hold
    invariants: Vec<Box<dyn Fn(&Output) -> bool>>,
    
    /// Acceptable variance (e.g., for floating point)
    tolerance: f64,
}

impl OptimizationOracle {
    fn verify(&self, new_output: &Output) -> Result<(), ValidationError> {
        // Check golden outputs match exactly
        for (path, expected) in &self.golden_outputs {
            let actual = std::fs::read(path)?;
            if actual != *expected {
                return Err(ValidationError::OutputMismatch { path: path.clone() });
            }
        }
        
        // Check all invariants hold
        for (idx, invariant) in self.invariants.iter().enumerate() {
            if !invariant(new_output) {
                return Err(ValidationError::InvariantViolation { index: idx });
            }
        }
        
        Ok(())
    }
}
```

#### D) Isomorphism Proof Per Change

Every optimization diff must include proof that outputs cannot change:

```rust
// OPTIMIZATION: Replace Vec<String> with Vec<Cow<'static, str>>
//
// ISOMORPHISM PROOF:
// - All string values in this collection are either:
//   (a) Static string literals → Cow::Borrowed preserves identity
//   (b) Runtime strings → Cow::Owned preserves value
// - Collection ordering unchanged (same iteration order)
// - Comparison semantics unchanged (Cow<str> derefs to &str)
// - No observable behavior change for any consumer
//
// VERIFIED BY: test_string_collection_equivalence() in tests/optimization.rs
```

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

```
❌ Bad: "Optimize parsing: use SIMD, parallelize, and cache results"
✓ Good: "Use SIMD for float parsing" → "Parallelize file reads" → "Add LRU cache"
```

Benefits:
- Easier to measure individual impact
- Easier to bisect regressions
- Easier to revert if problems arise

#### G) Regression Guardrails

Add benchmark thresholds to CI:

```rust
// benches/regression.rs
use criterion::{criterion_group, criterion_main, Criterion};

fn benchmark_critical_path(c: &mut Criterion) {
    c.bench_function("skill_extraction", |b| {
        b.iter(|| extract_skills(test_data()))
    });
}

criterion_group! {
    name = regression_tests;
    config = Criterion::default()
        .significance_level(0.01)  // 1% significance
        .sample_size(100);
    targets = benchmark_critical_path
}
```

```yaml
# .github/workflows/bench.yml
- name: Check for performance regression
  run: |
    cargo bench -- --save-baseline main
    # Fail if >10% slower than baseline
    cargo bench -- --baseline main --threshold 1.1
```

### 31.2 Memory Optimization Patterns

#### Zero-Copy Pattern

```rust
// BEFORE: Copies data unnecessarily
fn process_data(input: &[u8]) -> Vec<u8> {
    let data = input.to_vec();  // Allocation + copy
    transform(&data)
}

// AFTER: Zero-copy with lifetime management
fn process_data<'a>(input: &'a [u8]) -> Cow<'a, [u8]> {
    if needs_transform(input) {
        Cow::Owned(transform(input))
    } else {
        Cow::Borrowed(input)  // No copy when unchanged
    }
}
```

#### Buffer Reuse Pattern

```rust
/// Reusable buffer pool to avoid repeated allocations
struct BufferPool {
    buffers: Vec<Vec<u8>>,
    buffer_size: usize,
}

impl BufferPool {
    fn acquire(&mut self) -> Vec<u8> {
        self.buffers.pop().unwrap_or_else(|| Vec::with_capacity(self.buffer_size))
    }
    
    fn release(&mut self, mut buffer: Vec<u8>) {
        buffer.clear();  // Reset length but keep capacity
        if self.buffers.len() < 32 {  // Cap pool size
            self.buffers.push(buffer);
        }
    }
}

// Usage pattern
let mut pool = BufferPool::new(4096);
for chunk in input.chunks(4096) {
    let mut buf = pool.acquire();
    buf.extend_from_slice(chunk);
    process(&buf);
    pool.release(buf);
}
```

#### String Interning

```rust
use std::collections::HashSet;
use std::sync::Arc;

/// String interner for deduplicating repeated strings
struct StringInterner {
    strings: HashSet<Arc<str>>,
}

impl StringInterner {
    fn intern(&mut self, s: &str) -> Arc<str> {
        if let Some(existing) = self.strings.get(s) {
            Arc::clone(existing)
        } else {
            let arc: Arc<str> = Arc::from(s);
            self.strings.insert(Arc::clone(&arc));
            arc
        }
    }
}

// Useful for skill names, tag names, etc. that repeat across sessions
```

#### Copy-on-Write (Cow) Pattern

```rust
use std::borrow::Cow;

/// Configuration that's usually static but sometimes modified
struct SkillConfig<'a> {
    name: Cow<'a, str>,
    template: Cow<'a, str>,
    tags: Cow<'a, [String]>,
}

impl<'a> SkillConfig<'a> {
    /// Create from static defaults (no allocation)
    fn default_static() -> Self {
        Self {
            name: Cow::Borrowed("default"),
            template: Cow::Borrowed(include_str!("default.hbs")),
            tags: Cow::Borrowed(&[]),
        }
    }
    
    /// Modify only when needed (allocation on demand)
    fn with_name(mut self, name: String) -> Self {
        self.name = Cow::Owned(name);
        self
    }
}
```

#### Structure of Arrays (SoA) vs Array of Structures (AoS)

```rust
// AoS: Good for single-item access, bad for cache when iterating one field
struct SkillAoS {
    skills: Vec<Skill>,  // Each Skill has name, description, tags, etc.
}

// SoA: Better cache utilization when iterating single fields
struct SkillSoA {
    names: Vec<String>,
    descriptions: Vec<String>,
    tags: Vec<Vec<String>>,
    // All names are contiguous in memory - better for iteration
}

// Hybrid: Common fields together, rare fields separate
struct SkillHybrid {
    // Hot data (frequently accessed together)
    hot: Vec<SkillHot>,
    // Cold data (rarely accessed)
    cold: Vec<SkillCold>,
}

struct SkillHot {
    name: String,
    score: f32,
}

struct SkillCold {
    description: String,
    examples: Vec<String>,
    metadata: HashMap<String, String>,
}
```

### 31.3 Algorithm and Data Structure Optimizations

#### Trie for Prefix Matching

```rust
/// Trie for efficient prefix matching of skill names/commands
struct TrieNode {
    children: HashMap<char, TrieNode>,
    is_end: bool,
    value: Option<usize>,  // Index into skills array
}

impl TrieNode {
    fn insert(&mut self, key: &str, value: usize) {
        let mut node = self;
        for ch in key.chars() {
            node = node.children.entry(ch).or_default();
        }
        node.is_end = true;
        node.value = Some(value);
    }
    
    fn find_prefix_matches(&self, prefix: &str) -> Vec<usize> {
        let mut node = self;
        for ch in prefix.chars() {
            match node.children.get(&ch) {
                Some(child) => node = child,
                None => return vec![],
            }
        }
        self.collect_all_values(node)
    }
}
```

#### Bloom Filter for Membership Testing

```rust
/// Bloom filter for fast "definitely not in set" checks
struct BloomFilter {
    bits: Vec<u64>,
    num_hashes: usize,
}

impl BloomFilter {
    fn insert(&mut self, item: &str) {
        for i in 0..self.num_hashes {
            let hash = self.hash(item, i);
            let idx = hash % (self.bits.len() * 64);
            self.bits[idx / 64] |= 1 << (idx % 64);
        }
    }
    
    fn may_contain(&self, item: &str) -> bool {
        for i in 0..self.num_hashes {
            let hash = self.hash(item, i);
            let idx = hash % (self.bits.len() * 64);
            if self.bits[idx / 64] & (1 << (idx % 64)) == 0 {
                return false;  // Definitely not present
            }
        }
        true  // Possibly present (may be false positive)
    }
}

// Use case: Skip expensive skill matching for sessions that definitely
// don't contain certain patterns
```

#### Interval Tree for Range Queries

```rust
/// Interval tree for efficient range overlap queries
/// Useful for: finding skills applicable to time ranges, line ranges, etc.
struct IntervalTree<T> {
    root: Option<Box<IntervalNode<T>>>,
}

struct IntervalNode<T> {
    interval: (usize, usize),  // (start, end)
    max_end: usize,
    value: T,
    left: Option<Box<IntervalNode<T>>>,
    right: Option<Box<IntervalNode<T>>>,
}

impl<T> IntervalTree<T> {
    fn query_overlapping(&self, start: usize, end: usize) -> Vec<&T> {
        let mut results = vec![];
        self.query_recursive(&self.root, start, end, &mut results);
        results
    }
}
```

#### Segment Tree with Lazy Propagation

```rust
/// Segment tree for range queries with lazy updates
/// Useful for: aggregating scores over ranges, bulk updates
struct SegmentTree {
    tree: Vec<i64>,
    lazy: Vec<i64>,
    n: usize,
}

impl SegmentTree {
    fn range_update(&mut self, l: usize, r: usize, delta: i64) {
        self.update_range(0, 0, self.n - 1, l, r, delta);
    }
    
    fn range_query(&mut self, l: usize, r: usize) -> i64 {
        self.query_range(0, 0, self.n - 1, l, r)
    }
    
    // Lazy propagation defers updates until queries need them
    fn push_down(&mut self, node: usize, start: usize, end: usize) {
        if self.lazy[node] != 0 {
            let mid = (start + end) / 2;
            let left = 2 * node + 1;
            let right = 2 * node + 2;
            
            self.tree[left] += self.lazy[node] * (mid - start + 1) as i64;
            self.tree[right] += self.lazy[node] * (end - mid) as i64;
            self.lazy[left] += self.lazy[node];
            self.lazy[right] += self.lazy[node];
            self.lazy[node] = 0;
        }
    }
}
```

### 31.4 Advanced Algorithmic Techniques

> **Speculative Section**: The techniques below (Convex Hull Trick, Matrix Exponentiation)
> are included for completeness but are unlikely to be needed for typical CLI tool workloads.
> These are competitive programming techniques that apply to specific mathematical structures.
> Profile before implementing - premature optimization with these patterns adds complexity
> with no benefit if the problem structure doesn't match.

#### Convex Hull Trick for DP Optimization

```rust
/// Convex hull trick for optimizing DP recurrences of form:
/// dp[i] = min(dp[j] + b[j] * a[i]) for j < i
/// 
/// Reduces O(n²) to O(n log n) or O(n) if slopes monotonic
struct ConvexHullTrick {
    lines: Vec<(i64, i64)>,  // (slope, intercept)
}

impl ConvexHullTrick {
    fn add_line(&mut self, m: i64, b: i64) {
        // Remove lines that are no longer part of lower envelope
        while self.lines.len() >= 2 {
            let n = self.lines.len();
            let (m1, b1) = self.lines[n - 2];
            let (m2, b2) = self.lines[n - 1];
            // Check if (m2, b2) is useless
            if (b - b2) * (m1 - m2) <= (b2 - b1) * (m2 - m) {
                self.lines.pop();
            } else {
                break;
            }
        }
        self.lines.push((m, b));
    }
    
    fn query(&self, x: i64) -> i64 {
        // Binary search for optimal line
        let idx = self.lines.partition_point(|(m, b)| {
            // Find first line where next line is better
            true  // Simplified - actual implementation more complex
        });
        let (m, b) = self.lines[idx.saturating_sub(1)];
        m * x + b
    }
}
```

#### Matrix Exponentiation for Linear Recurrences

```rust
/// Fast matrix exponentiation for computing linear recurrences
/// Example: Fibonacci F(n) in O(log n) via matrix form
type Matrix = [[i64; 2]; 2];

fn matrix_mult(a: &Matrix, b: &Matrix, modulo: i64) -> Matrix {
    let mut result = [[0; 2]; 2];
    for i in 0..2 {
        for j in 0..2 {
            for k in 0..2 {
                result[i][j] = (result[i][j] + a[i][k] * b[k][j]) % modulo;
            }
        }
    }
    result
}

fn matrix_pow(base: &Matrix, mut exp: u64, modulo: i64) -> Matrix {
    let mut result = [[1, 0], [0, 1]];  // Identity matrix
    let mut base = *base;
    
    while exp > 0 {
        if exp & 1 == 1 {
            result = matrix_mult(&result, &base, modulo);
        }
        base = matrix_mult(&base, &base, modulo);
        exp >>= 1;
    }
    result
}

// Fibonacci: [[1,1],[1,0]]^n gives [F(n+1), F(n)] in first row
fn fibonacci(n: u64) -> i64 {
    let base = [[1, 1], [1, 0]];
    let result = matrix_pow(&base, n, i64::MAX);
    result[0][1]
}
```

#### FFT/NTT for Polynomial Multiplication

```rust
/// Number Theoretic Transform for fast polynomial multiplication
/// Useful for: convolutions, string matching, etc.
const MOD: i64 = 998244353;  // Prime with good NTT properties
const PRIMITIVE_ROOT: i64 = 3;

fn ntt(a: &mut [i64], invert: bool) {
    let n = a.len();
    if n == 1 { return; }
    
    // Bit-reversal permutation
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            a.swap(i, j);
        }
    }
    
    // Cooley-Tukey iterative FFT
    let mut len = 2;
    while len <= n {
        let w = if invert {
            mod_pow(PRIMITIVE_ROOT, (MOD - 1) - (MOD - 1) / len as i64, MOD)
        } else {
            mod_pow(PRIMITIVE_ROOT, (MOD - 1) / len as i64, MOD)
        };
        
        for i in (0..n).step_by(len) {
            let mut wn = 1i64;
            for j in 0..len/2 {
                let u = a[i + j];
                let v = a[i + j + len/2] * wn % MOD;
                a[i + j] = (u + v) % MOD;
                a[i + j + len/2] = (u - v + MOD) % MOD;
                wn = wn * w % MOD;
            }
        }
        len *= 2;
    }
    
    if invert {
        let n_inv = mod_pow(n as i64, MOD - 2, MOD);
        for x in a.iter_mut() {
            *x = *x * n_inv % MOD;
        }
    }
}

fn mod_pow(mut base: i64, mut exp: i64, modulo: i64) -> i64 {
    let mut result = 1;
    while exp > 0 {
        if exp & 1 == 1 {
            result = result * base % modulo;
        }
        base = base * base % modulo;
        exp >>= 1;
    }
    result
}
```

### 31.5 Lazy Evaluation Patterns

#### Lazy Iterator Chains

```rust
// BEFORE: Materializes all intermediate collections
fn process_skills(skills: Vec<Skill>) -> Vec<ProcessedSkill> {
    let filtered: Vec<_> = skills.into_iter()
        .filter(|s| s.is_valid())
        .collect();
    let mapped: Vec<_> = filtered.into_iter()
        .map(|s| s.process())
        .collect();
    mapped
}

// AFTER: Lazy evaluation - no intermediate allocations
fn process_skills(skills: Vec<Skill>) -> Vec<ProcessedSkill> {
    skills.into_iter()
        .filter(|s| s.is_valid())
        .map(|s| s.process())
        .collect()  // Single allocation for final result
}
```

#### Lazy Loading with OnceCell

```rust
use std::cell::OnceCell;

/// Resource loaded on first access
struct LazySkillIndex {
    path: PathBuf,
    index: OnceCell<SkillIndex>,
}

impl LazySkillIndex {
    fn new(path: PathBuf) -> Self {
        Self { path, index: OnceCell::new() }
    }
    
    fn get(&self) -> &SkillIndex {
        self.index.get_or_init(|| {
            // Expensive initialization only happens once, on demand
            SkillIndex::load(&self.path).expect("Failed to load index")
        })
    }
}
```

#### Deferred Computation Pattern

```rust
use std::cell::OnceCell;

/// Computation that's only performed if result is actually used.
///
/// SAFE implementation using OnceCell - no unsafe code needed.
/// Prefer `std::cell::LazyCell` (Rust 1.80+) or `once_cell::sync::Lazy` for
/// thread-safe lazy initialization.
struct Deferred<T, F: FnOnce() -> T> {
    cell: OnceCell<T>,
    init: Option<F>,
}

impl<T, F: FnOnce() -> T> Deferred<T, F> {
    fn new(f: F) -> Self {
        Self {
            cell: OnceCell::new(),
            init: Some(f),
        }
    }

    fn get(&mut self) -> &T {
        self.cell.get_or_init(|| {
            // Take the initializer - this can only happen once
            let f = self.init.take().expect("Deferred already initialized");
            f()
        })
    }

    /// Returns true if the value has been computed
    fn is_computed(&self) -> bool {
        self.cell.get().is_some()
    }
}

// For cases where you need thread-safe lazy evaluation:
use std::sync::LazyLock;

static EXPENSIVE_COMPUTATION: LazyLock<Vec<u8>> = LazyLock::new(|| {
    // This only runs once, when first accessed
    compute_expensive_thing()
});
```

### 31.6 Memoization with Invalidation

#### Time-Based Cache Invalidation

```rust
use std::time::{Duration, Instant};
use std::collections::HashMap;

struct TimedCache<K, V> {
    entries: HashMap<K, (V, Instant)>,
    ttl: Duration,
}

impl<K: Eq + std::hash::Hash, V: Clone> TimedCache<K, V> {
    fn get(&self, key: &K) -> Option<V> {
        self.entries.get(key).and_then(|(value, inserted)| {
            if inserted.elapsed() < self.ttl {
                Some(value.clone())
            } else {
                None  // Expired
            }
        })
    }
    
    fn insert(&mut self, key: K, value: V) {
        self.entries.insert(key, (value, Instant::now()));
    }
    
    fn evict_expired(&mut self) {
        self.entries.retain(|_, (_, inserted)| inserted.elapsed() < self.ttl);
    }
}
```

#### Version-Based Invalidation

```rust
/// Cache that invalidates when source version changes
struct VersionedCache<V> {
    value: Option<V>,
    cached_version: u64,
}

impl<V> VersionedCache<V> {
    fn get<F>(&mut self, current_version: u64, compute: F) -> &V 
    where
        F: FnOnce() -> V,
    {
        if self.value.is_none() || self.cached_version != current_version {
            self.value = Some(compute());
            self.cached_version = current_version;
        }
        self.value.as_ref().unwrap()
    }
    
    fn invalidate(&mut self) {
        self.value = None;
    }
}

// Usage with file modification time
struct FileCache {
    path: PathBuf,
    cache: VersionedCache<ParsedContent>,
}

impl FileCache {
    fn get(&mut self) -> &ParsedContent {
        let mtime = std::fs::metadata(&self.path)
            .and_then(|m| m.modified())
            .map(|t| t.duration_since(std::time::UNIX_EPOCH).unwrap().as_secs())
            .unwrap_or(0);
        
        self.cache.get(mtime, || parse_file(&self.path))
    }
}
```

#### Dependency-Based Invalidation

```rust
/// Cache with explicit dependency tracking
struct DependencyCache<K, V> {
    entries: HashMap<K, CacheEntry<V>>,
    dependencies: HashMap<K, Vec<K>>,  // key -> keys that depend on it
}

struct CacheEntry<V> {
    value: V,
    valid: bool,
}

impl<K: Clone + Eq + std::hash::Hash, V> DependencyCache<K, V> {
    fn invalidate(&mut self, key: &K) {
        if let Some(entry) = self.entries.get_mut(key) {
            entry.valid = false;
        }
        
        // Cascade invalidation to dependents
        if let Some(dependents) = self.dependencies.get(key).cloned() {
            for dependent in dependents {
                self.invalidate(&dependent);
            }
        }
    }
    
    fn set_dependency(&mut self, key: K, depends_on: K) {
        self.dependencies
            .entry(depends_on)
            .or_default()
            .push(key);
    }
}
```

### 31.7 I/O Optimization Patterns

#### Scatter-Gather I/O

```rust
use std::io::{IoSlice, Write};

/// Write multiple buffers in single syscall
fn write_multiple<W: Write>(writer: &mut W, buffers: &[&[u8]]) -> std::io::Result<usize> {
    let slices: Vec<IoSlice> = buffers.iter()
        .map(|b| IoSlice::new(b))
        .collect();
    writer.write_vectored(&slices)
}

// Avoids copying multiple small buffers into one large buffer
// before writing
```

#### Buffered I/O with Controlled Flushing

```rust
use std::io::{BufWriter, Write};

/// Batched writer that flushes at optimal intervals
struct BatchedWriter<W: Write> {
    inner: BufWriter<W>,
    writes_since_flush: usize,
    flush_interval: usize,
}

impl<W: Write> BatchedWriter<W> {
    fn write_item(&mut self, data: &[u8]) -> std::io::Result<()> {
        self.inner.write_all(data)?;
        self.writes_since_flush += 1;
        
        if self.writes_since_flush >= self.flush_interval {
            self.inner.flush()?;
            self.writes_since_flush = 0;
        }
        Ok(())
    }
}
```

#### Async I/O for Concurrent Operations

```rust
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use futures::future::join_all;

/// Read multiple files concurrently
async fn read_all_files(paths: &[PathBuf]) -> Vec<Result<Vec<u8>, std::io::Error>> {
    let futures: Vec<_> = paths.iter()
        .map(|path| async move {
            let mut file = File::open(path).await?;
            let mut contents = Vec::new();
            file.read_to_end(&mut contents).await?;
            Ok(contents)
        })
        .collect();
    
    join_all(futures).await
}
```

### 31.8 Precomputation Patterns

#### Lookup Tables

```rust
/// Precomputed lookup table for fast conversions
struct LookupTable {
    // Precompute all possible byte -> hex conversions
    byte_to_hex: [[u8; 2]; 256],
}

impl LookupTable {
    fn new() -> Self {
        let mut byte_to_hex = [[0u8; 2]; 256];
        for i in 0..256 {
            byte_to_hex[i] = [
                HEX_CHARS[i >> 4],
                HEX_CHARS[i & 0xF],
            ];
        }
        Self { byte_to_hex }
    }
    
    fn to_hex(&self, bytes: &[u8], out: &mut Vec<u8>) {
        out.reserve(bytes.len() * 2);
        for &byte in bytes {
            out.extend_from_slice(&self.byte_to_hex[byte as usize]);
        }
    }
}

const HEX_CHARS: [u8; 16] = *b"0123456789abcdef";
```

#### Compile-Time Computation

```rust
/// Compile-time computed constants
const fn compute_factorial(n: usize) -> usize {
    if n <= 1 { 1 } else { n * compute_factorial(n - 1) }
}

const FACTORIALS: [usize; 13] = [
    compute_factorial(0),  compute_factorial(1),  compute_factorial(2),
    compute_factorial(3),  compute_factorial(4),  compute_factorial(5),
    compute_factorial(6),  compute_factorial(7),  compute_factorial(8),
    compute_factorial(9),  compute_factorial(10), compute_factorial(11),
    compute_factorial(12),
];

// Zero runtime cost - computed at compile time
fn factorial(n: usize) -> Option<usize> {
    FACTORIALS.get(n).copied()
}
```

#### Static Initialization with LazyLock

```rust
use std::sync::LazyLock;
use regex::Regex;

/// Compile regex once, reuse everywhere
static SKILL_PATTERN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(skill|function|pattern|technique)\b.*:\s*(.+)").unwrap()
});

fn extract_skill(text: &str) -> Option<&str> {
    SKILL_PATTERN.captures(text)
        .and_then(|caps| caps.get(2))
        .map(|m| m.as_str())
}
```

### 31.9 N+1 Query Elimination

#### Batch Loading Pattern

```rust
/// Avoid N+1 by loading all related data upfront
struct SkillRepository {
    db: Database,
}

impl SkillRepository {
    // BAD: N+1 queries
    async fn get_skills_with_tags_bad(&self, ids: &[i64]) -> Vec<SkillWithTags> {
        let mut results = vec![];
        for id in ids {
            let skill = self.db.get_skill(*id).await;      // 1 query
            let tags = self.db.get_skill_tags(*id).await;  // N queries
            results.push(SkillWithTags { skill, tags });
        }
        results
    }
    
    // GOOD: 2 queries total
    async fn get_skills_with_tags_good(&self, ids: &[i64]) -> Vec<SkillWithTags> {
        // Batch load all skills
        let skills = self.db.get_skills_batch(ids).await;  // 1 query
        
        // Batch load all tags
        let all_tags = self.db.get_tags_for_skills(ids).await;  // 1 query
        let tags_by_skill: HashMap<i64, Vec<Tag>> = all_tags.into_iter()
            .fold(HashMap::new(), |mut map, tag| {
                map.entry(tag.skill_id).or_default().push(tag);
                map
            });
        
        // Join in memory
        skills.into_iter()
            .map(|skill| SkillWithTags {
                tags: tags_by_skill.get(&skill.id).cloned().unwrap_or_default(),
                skill,
            })
            .collect()
    }
}
```

#### DataLoader Pattern

```rust
use std::collections::HashMap;
use std::sync::Mutex;

/// DataLoader batches and caches requests
struct DataLoader<K, V> {
    load_fn: Box<dyn Fn(&[K]) -> HashMap<K, V> + Send + Sync>,
    cache: Mutex<HashMap<K, V>>,
    pending: Mutex<Vec<K>>,
}

impl<K: Clone + Eq + std::hash::Hash, V: Clone> DataLoader<K, V> {
    fn load(&self, key: K) -> Option<V> {
        // Check cache first
        if let Some(value) = self.cache.lock().unwrap().get(&key) {
            return Some(value.clone());
        }
        
        // Add to pending batch
        self.pending.lock().unwrap().push(key.clone());
        
        // Batch execute on next tick (simplified)
        self.execute_batch();
        
        self.cache.lock().unwrap().get(&key).cloned()
    }
    
    fn execute_batch(&self) {
        let pending: Vec<K> = self.pending.lock().unwrap().drain(..).collect();
        if pending.is_empty() { return; }
        
        let loaded = (self.load_fn)(&pending);
        self.cache.lock().unwrap().extend(loaded);
    }
}
```

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

```
START: Performance problem identified
         │
         ▼
    ┌────────────┐
    │ Profile    │ ◄── "Measure, don't guess"
    │ first      │
    └────┬───────┘
         │
         ▼
    Is it CPU-bound?
    │Yes            │No
    ▼               ▼
┌──────────┐   Is it I/O-bound?
│Algorithm │   │Yes            │No
│& Data    │   ▼               ▼
│Structure │  ┌──────────┐   Is it memory-bound?
└────┬─────┘  │Async/    │   │Yes            │No
     │        │Batch/    │   ▼               ▼
     ▼        │Cache     │  ┌──────────┐   ┌──────────┐
┌──────────┐  └──────────┘  │Memory    │   │Measure   │
│Try:      │                │Layout/   │   │again -   │
│• Better  │                │Zero-copy │   │wrong     │
│  algo    │                │Pooling   │   │diagnosis │
│• SIMD    │                └──────────┘   └──────────┘
│• Paral-  │
│  lelism  │
└──────────┘
```

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

```rust
/// Security audit phases
enum AuditPhase {
    /// Phase 1: Identify attack surface
    AttackSurfaceMapping,
    /// Phase 2: Review each vulnerability category
    CategoryReview,
    /// Phase 3: Validate findings with PoC
    Validation,
    /// Phase 4: Risk assessment and prioritization
    RiskAssessment,
    /// Phase 5: Remediation recommendations
    Remediation,
}

/// Vulnerability severity classification
enum Severity {
    Critical,  // Remote code execution, auth bypass, data breach
    High,      // Privilege escalation, significant data exposure
    Medium,    // Information disclosure, limited impact
    Low,       // Minor issues, defense in depth improvements
}

/// Security finding structure
struct SecurityFinding {
    title: String,
    severity: Severity,
    file_path: String,
    line_number: usize,
    description: String,
    proof_of_concept: Option<String>,
    recommendation: String,
    cwe_id: Option<u32>,  // Common Weakness Enumeration
}
```

#### Attack Surface Mapping Checklist

```markdown
## Attack Surface Categories

### 1. Network Boundaries
- [ ] Public-facing endpoints (HTTP/HTTPS)
- [ ] Internal APIs and services
- [ ] WebSocket connections
- [ ] Database connections
- [ ] External service integrations

### 2. User Input Entry Points
- [ ] Form submissions
- [ ] URL parameters (query strings, path params)
- [ ] HTTP headers (especially X-Forwarded-*, Authorization)
- [ ] File uploads
- [ ] API request bodies (JSON, XML)
- [ ] Command-line arguments
- [ ] Environment variables

### 3. Authentication Boundaries
- [ ] Login flows (OAuth, password, MFA)
- [ ] Session management (cookies, tokens)
- [ ] API key validation
- [ ] Service-to-service auth

### 4. Data Storage
- [ ] Database queries (SQL, NoSQL)
- [ ] File system access
- [ ] Cache storage (Redis, Memcached)
- [ ] Browser storage (localStorage, sessionStorage)

### 5. Process Boundaries
- [ ] Command execution
- [ ] Child process spawning
- [ ] Inter-process communication
```

### 32.2 OWASP-Aligned Vulnerability Categories

#### A01: Broken Access Control

```rust
/// Access control verification pattern
pub fn verify_authorization(
    user: &User,
    resource: &Resource,
    action: Action,
) -> Result<(), AuthError> {
    // 1. Verify user is authenticated
    if !user.is_authenticated() {
        return Err(AuthError::NotAuthenticated);
    }
    
    // 2. Check resource ownership or explicit permission
    let has_access = match resource.access_type {
        AccessType::Owner => resource.owner_id == user.id,
        AccessType::SharedWith => resource.shared_with.contains(&user.id),
        AccessType::Public => true,
        AccessType::RoleBased => user.roles.iter()
            .any(|role| resource.allowed_roles.contains(role)),
    };
    
    if !has_access {
        return Err(AuthError::Forbidden);
    }
    
    // 3. Verify action is permitted
    if !resource.allowed_actions.contains(&action) {
        return Err(AuthError::ActionNotPermitted);
    }
    
    Ok(())
}

// Anti-pattern: Direct object reference without authorization
// BAD: let doc = db.get_document(user_provided_id)?;
// GOOD: let doc = db.get_document_for_user(user_provided_id, current_user.id)?;
```

#### A02: Cryptographic Failures

```rust
use aes_gcm::{Aes256Gcm, KeyInit, Nonce};
use aes_gcm::aead::Aead;
use argon2::{Argon2, PasswordHasher};
use rand::rngs::OsRng;

/// Secure password hashing with Argon2id
pub fn hash_password(password: &str) -> Result<String, CryptoError> {
    let salt = argon2::password_hash::SaltString::generate(&mut OsRng);
    
    // Argon2id with recommended parameters
    let argon2 = Argon2::new(
        argon2::Algorithm::Argon2id,  // Memory-hard, GPU-resistant
        argon2::Version::V0x13,
        argon2::Params::new(
            65536,  // 64 MB memory
            3,      // 3 iterations
            4,      // 4 lanes parallelism
            Some(32), // 32-byte output
        )?,
    );
    
    let hash = argon2.hash_password(password.as_bytes(), &salt)?;
    Ok(hash.to_string())
}

/// Secure encryption with AES-256-GCM
pub fn encrypt_data(
    key: &[u8; 32],
    plaintext: &[u8],
    aad: &[u8],  // Additional authenticated data
) -> Result<Vec<u8>, CryptoError> {
    let cipher = Aes256Gcm::new_from_slice(key)?;
    
    // Generate random nonce (CRITICAL: must be unique per encryption)
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    
    let ciphertext = cipher.encrypt(nonce, aes_gcm::aead::Payload {
        msg: plaintext,
        aad,
    })?;
    
    // Prepend nonce to ciphertext
    let mut result = nonce_bytes.to_vec();
    result.extend(ciphertext);
    Ok(result)
}

// CRITICAL: Nonce derivation anti-pattern
// BAD: XOR-based nonce derivation (can cause collisions)
fn bad_derive_nonce(base: &[u8; 12], counter: u32) -> [u8; 12] {
    let mut nonce = *base;
    let idx_bytes = counter.to_be_bytes();
    for i in 0..4 {
        nonce[8 + i] ^= idx_bytes[i];  // XOR is NOT collision-resistant!
    }
    nonce
}

// GOOD: Counter-based nonce derivation
fn good_derive_nonce(base: &[u8; 12], counter: u32) -> [u8; 12] {
    let mut nonce = *base;
    // Direct assignment of counter in big-endian
    nonce[8..12].copy_from_slice(&counter.to_be_bytes());
    nonce
}
```

#### A03: Injection

```rust
/// SQL injection prevention with parameterized queries
pub fn get_user_by_email(conn: &Connection, email: &str) -> Result<User, DbError> {
    // GOOD: Parameterized query
    conn.query_row(
        "SELECT id, email, name FROM users WHERE email = ?1",
        [email],
        |row| Ok(User {
            id: row.get(0)?,
            email: row.get(1)?,
            name: row.get(2)?,
        }),
    )
}

// BAD: String interpolation (SQL injection vulnerable)
// let query = format!("SELECT * FROM users WHERE email = '{}'", email);

/// Command injection prevention
pub fn safe_execute_command(
    allowed_commands: &[&str],
    command: &str,
    args: &[&str],
) -> Result<Output, SecurityError> {
    // 1. Whitelist validation
    if !allowed_commands.contains(&command) {
        return Err(SecurityError::CommandNotAllowed(command.to_string()));
    }
    
    // 2. Argument validation (no shell metacharacters)
    for arg in args {
        if arg.contains(|c: char| ";&|`$(){}[]<>\\\"'".contains(c)) {
            return Err(SecurityError::InvalidArgument(arg.to_string()));
        }
    }
    
    // 3. Use execve-style execution (no shell)
    Command::new(command)
        .args(args)
        .output()
        .map_err(|e| SecurityError::ExecutionFailed(e))
}

/// Shell argument escaping (when shell is unavoidable)
pub fn escape_shell_arg(arg: &str) -> String {
    // Single-quote escaping: replace ' with '\''
    format!("'{}'", arg.replace('\'', "'\\''"))
}
```

#### A04: Insecure Design

```rust
/// Secure session management
pub struct SecureSession {
    /// Random session ID (cryptographically secure)
    id: [u8; 32],
    /// User identifier
    user_id: Uuid,
    /// Creation timestamp
    created_at: DateTime<Utc>,
    /// Expiration timestamp
    expires_at: DateTime<Utc>,
    /// Last activity timestamp
    last_activity: DateTime<Utc>,
    /// IP address (for anomaly detection)
    ip_address: IpAddr,
    /// User agent fingerprint
    user_agent_hash: [u8; 32],
}

impl SecureSession {
    pub fn new(user_id: Uuid, ip: IpAddr, user_agent: &str) -> Self {
        let mut id = [0u8; 32];
        OsRng.fill_bytes(&mut id);
        
        let now = Utc::now();
        Self {
            id,
            user_id,
            created_at: now,
            expires_at: now + Duration::hours(24),  // Configurable
            last_activity: now,
            ip_address: ip,
            user_agent_hash: sha256(user_agent.as_bytes()),
        }
    }
    
    pub fn validate(&self, ip: IpAddr, user_agent: &str) -> Result<(), SessionError> {
        // Check expiration
        if Utc::now() > self.expires_at {
            return Err(SessionError::Expired);
        }
        
        // Detect session hijacking (IP or user agent change)
        if self.ip_address != ip {
            log::warn!("Session IP mismatch: {} vs {}", self.ip_address, ip);
            // Consider: require re-authentication for sensitive operations
        }
        
        if self.user_agent_hash != sha256(user_agent.as_bytes()) {
            log::warn!("Session user agent mismatch");
        }
        
        Ok(())
    }
}
```

#### A05: Security Misconfiguration

```rust
/// Security configuration validation
pub struct SecurityConfig {
    /// CORS allowed origins
    cors_origins: Vec<String>,
    /// Rate limiting configuration
    rate_limit: RateLimitConfig,
    /// TLS configuration
    tls: TlsConfig,
    /// Secret management
    secrets: SecretsConfig,
}

impl SecurityConfig {
    pub fn validate(&self) -> Result<(), ConfigError> {
        // Validate CORS is not too permissive
        if self.cors_origins.contains(&"*".to_string()) {
            return Err(ConfigError::InsecureCors(
                "Wildcard CORS is dangerous in production".into()
            ));
        }
        
        // Validate rate limiting is enabled
        if self.rate_limit.requests_per_minute == 0 {
            return Err(ConfigError::NoRateLimiting);
        }
        
        // Validate TLS is enforced
        if !self.tls.enforce_https {
            log::warn!("HTTPS enforcement is disabled");
        }
        
        // Validate secrets are not hardcoded
        for (name, value) in &self.secrets.values {
            if value.len() < 32 {
                log::warn!("Secret '{}' may be weak (length < 32)", name);
            }
        }
        
        Ok(())
    }
}

// Environment variable security
pub fn load_secret(name: &str) -> Result<String, ConfigError> {
    std::env::var(name).map_err(|_| {
        ConfigError::MissingSecret(format!(
            "Required secret '{}' not found in environment", name
        ))
    })
}
```

### 32.3 Input Validation Patterns

#### Path Traversal Prevention

```rust
use std::path::{Path, PathBuf, Component};

/// Validate path is within allowed directory
pub fn validate_path(base_dir: &Path, user_path: &str) -> Result<PathBuf, PathError> {
    // 1. Reject absolute paths
    if Path::new(user_path).is_absolute() {
        return Err(PathError::AbsolutePathNotAllowed);
    }

    // 2. Normalize and check for traversal attempts
    // NOTE: canonicalize() fails for non-existent paths. For new file creation,
    // canonicalize the parent directory instead:
    let requested = base_dir.join(user_path);
    let canonical = match requested.canonicalize() {
        Ok(p) => p,
        Err(_) => {
            // Path doesn't exist - canonicalize parent and append filename
            let parent = requested.parent()
                .ok_or(PathError::InvalidPath)?;
            let filename = requested.file_name()
                .ok_or(PathError::InvalidPath)?;
            let canonical_parent = parent.canonicalize()
                .map_err(|_| PathError::InvalidPath)?;
            canonical_parent.join(filename)
        }
    };

    // 3. Verify path is still within base directory
    // NOTE: Use canonical form of base_dir too, to prevent symlink escapes
    let canonical_base = base_dir.canonicalize()
        .map_err(|_| PathError::InvalidPath)?;
    if !canonical.starts_with(&canonical_base) {
        return Err(PathError::TraversalAttempt);
    }
    
    // 4. Additional check: reject suspicious components
    for component in Path::new(user_path).components() {
        match component {
            Component::ParentDir => {
                return Err(PathError::ParentDirNotAllowed);
            }
            Component::Normal(s) => {
                let s = s.to_string_lossy();
                // Reject hidden files and common bypass attempts
                if s.starts_with('.') || s.contains('\0') {
                    return Err(PathError::SuspiciousComponent(s.into()));
                }
            }
            _ => {}
        }
    }
    
    Ok(canonical)
}

/// Sanitize filename for safe storage
pub fn sanitize_filename(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .take(255)  // Max filename length
        .collect()
}
```

#### XSS Prevention

```rust
/// HTML entity escaping for output encoding
pub fn escape_html(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    for c in input.chars() {
        match c {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#x27;"),
            '/' => output.push_str("&#x2F;"),
            _ => output.push(c),
        }
    }
    output
}

/// Content Security Policy header generation
pub fn csp_header() -> String {
    vec![
        "default-src 'self'",
        "script-src 'self' 'strict-dynamic'",
        "style-src 'self' 'unsafe-inline'",  // Consider nonces for inline styles
        "img-src 'self' data: https:",
        "font-src 'self'",
        "connect-src 'self'",
        "frame-ancestors 'none'",
        "base-uri 'self'",
        "form-action 'self'",
    ].join("; ")
}

/// Sanitize user-generated HTML (whitelist approach)
pub fn sanitize_html(input: &str) -> String {
    let allowed_tags = ["p", "br", "b", "i", "u", "a", "ul", "ol", "li"];
    let allowed_attrs = [("a", "href")];
    
    // Use a proper HTML sanitizer library like ammonia in production
    // This is a simplified example
    ammonia::Builder::default()
        .tags(allowed_tags.iter().cloned())
        .tag_attributes(allowed_attrs.iter().cloned().collect())
        .clean(input)
        .to_string()
}
```

### 32.4 Authentication Security Patterns

#### JWT Token Management

```rust
use jsonwebtoken::{encode, decode, Header, Validation, EncodingKey, DecodingKey};

#[derive(Debug, Serialize, Deserialize)]
pub struct AccessTokenClaims {
    /// Subject (user ID)
    pub sub: String,
    /// Email
    pub email: String,
    /// User tier/role
    pub tier: String,
    /// Token type
    pub r#type: String,  // "access"
    /// Issuer
    pub iss: String,
    /// Audience
    pub aud: String,
    /// Issued at
    pub iat: i64,
    /// Expiration
    pub exp: i64,
}

/// Generate access token with secure defaults
pub fn generate_access_token(
    user: &User,
    secret: &[u8],
    issuer: &str,
    audience: &str,
) -> Result<String, TokenError> {
    let now = Utc::now().timestamp();
    let claims = AccessTokenClaims {
        sub: user.id.to_string(),
        email: user.email.clone(),
        tier: user.tier.clone(),
        r#type: "access".to_string(),
        iss: issuer.to_string(),
        aud: audience.to_string(),
        iat: now,
        exp: now + 86400,  // 24 hours
    };
    
    encode(
        &Header::default(),  // HS256
        &claims,
        &EncodingKey::from_secret(secret),
    ).map_err(|e| TokenError::EncodingFailed(e.to_string()))
}

/// Validate access token with strict checks
pub fn validate_access_token(
    token: &str,
    secret: &[u8],
    issuer: &str,
    audience: &str,
) -> Result<AccessTokenClaims, TokenError> {
    // SECURITY: Explicitly pin allowed algorithms to prevent algorithm confusion attacks.
    // Never accept tokens with "none" algorithm or mismatched algorithm types.
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_issuer(&[issuer]);
    validation.set_audience(&[audience]);
    validation.set_required_spec_claims(&["sub", "exp", "iat", "iss", "aud"]);
    
    let token_data = decode::<AccessTokenClaims>(
        token,
        &DecodingKey::from_secret(secret),
        &validation,
    ).map_err(|e| TokenError::ValidationFailed(e.to_string()))?;
    
    // Additional type check
    if token_data.claims.r#type != "access" {
        return Err(TokenError::WrongTokenType);
    }
    
    Ok(token_data.claims)
}

/// Token refresh with rotation
pub struct TokenPair {
    pub access_token: String,
    pub refresh_token: String,
    pub refresh_token_id: Uuid,
}

pub fn refresh_tokens(
    old_refresh_token: &str,
    secret: &[u8],
    db: &Database,
) -> Result<TokenPair, TokenError> {
    // 1. Validate old refresh token
    let claims = validate_refresh_token(old_refresh_token, secret)?;
    
    // 2. Check if refresh token is in database (not revoked)
    let token_hash = sha256(claims.token_id.as_bytes());
    if !db.is_refresh_token_valid(&token_hash)? {
        // Token was revoked - possible token theft, invalidate all user sessions
        db.revoke_all_user_tokens(claims.sub)?;
        return Err(TokenError::TokenRevoked);
    }
    
    // 3. Generate new token pair (rotation)
    let new_refresh_id = Uuid::new_v4();
    let new_pair = generate_token_pair(&claims.sub, secret, new_refresh_id)?;
    
    // 4. Revoke old refresh token, store new one
    db.revoke_refresh_token(&token_hash)?;
    db.store_refresh_token(&sha256(new_refresh_id.as_bytes()), &claims.sub)?;
    
    Ok(new_pair)
}
```

#### OAuth Security

```rust
/// OAuth callback URL validation
pub fn validate_redirect_url(url: &str, allowed_origins: &[&str]) -> Result<Url, OAuthError> {
    let parsed = Url::parse(url)
        .map_err(|_| OAuthError::InvalidRedirectUrl)?;
    
    // 1. Reject non-HTTPS (except localhost for development)
    if parsed.scheme() != "https" && parsed.host_str() != Some("localhost") {
        return Err(OAuthError::InsecureRedirect);
    }
    
    // 2. Check against allowed origins (include port if non-default)
    // SECURITY: Origin must include port to prevent redirects to attacker-controlled ports.
    // e.g., if "https://example.com" is allowed, "https://example.com:8080" must not be.
    let origin = match parsed.port() {
        Some(port) => format!("{}://{}:{}", parsed.scheme(), parsed.host_str().unwrap_or(""), port),
        None => format!("{}://{}", parsed.scheme(), parsed.host_str().unwrap_or("")),
    };
    if !allowed_origins.contains(&origin.as_str()) {
        return Err(OAuthError::UnauthorizedOrigin);
    }
    
    // 3. Reject protocol-relative URLs
    if url.starts_with("//") {
        return Err(OAuthError::ProtocolRelativeUrl);
    }
    
    Ok(parsed)
}

/// PKCE (Proof Key for Code Exchange) implementation
pub struct PkceChallenge {
    pub code_verifier: String,
    pub code_challenge: String,
    pub code_challenge_method: String,
}

impl PkceChallenge {
    pub fn generate() -> Self {
        // Generate 43-128 character code verifier
        let mut verifier_bytes = [0u8; 32];
        OsRng.fill_bytes(&mut verifier_bytes);
        let code_verifier = base64_url_encode(&verifier_bytes);
        
        // SHA256 hash for S256 method
        let challenge_hash = sha256(code_verifier.as_bytes());
        let code_challenge = base64_url_encode(&challenge_hash);
        
        Self {
            code_verifier,
            code_challenge,
            code_challenge_method: "S256".to_string(),
        }
    }
    
    pub fn verify(verifier: &str, challenge: &str) -> bool {
        let computed = base64_url_encode(&sha256(verifier.as_bytes()));
        // Constant-time comparison to prevent timing attacks
        constant_time_compare(computed.as_bytes(), challenge.as_bytes())
    }
}
```

### 32.5 Rate Limiting and DoS Protection

#### IP-Based Rate Limiting

```rust
use std::collections::HashMap;
use std::time::{Duration, Instant};
use parking_lot::Mutex;

pub struct RateLimiter {
    limits: Mutex<HashMap<String, RateLimitEntry>>,
    max_requests: u32,
    window: Duration,
    max_entries: usize,  // Prevent memory exhaustion
}

struct RateLimitEntry {
    count: u32,
    window_start: Instant,
}

impl RateLimiter {
    pub fn check(&self, key: &str) -> Result<(), RateLimitError> {
        let mut limits = self.limits.lock();
        let now = Instant::now();

        // Cleanup expired entries probabilistically (1% chance per request)
        // This bounds memory growth without expensive cleanup on every call.
        // Also force cleanup if we're approaching max_entries.
        let should_cleanup = limits.len() > self.max_entries * 3 / 4
            || rand::random::<f32>() < 0.01;
        if should_cleanup {
            limits.retain(|_, entry| {
                now.duration_since(entry.window_start) < self.window
            });
        }
        
        let entry = limits.entry(key.to_string()).or_insert(RateLimitEntry {
            count: 0,
            window_start: now,
        });
        
        // Reset window if expired
        if now.duration_since(entry.window_start) >= self.window {
            entry.count = 0;
            entry.window_start = now;
        }
        
        entry.count += 1;
        
        if entry.count > self.max_requests {
            Err(RateLimitError::Exceeded {
                retry_after: self.window - now.duration_since(entry.window_start),
            })
        } else {
            Ok(())
        }
    }
}

/// Secure IP extraction (don't trust X-Forwarded-For blindly)
pub fn extract_client_ip(
    request: &Request,
    trusted_proxies: &[IpAddr],
) -> IpAddr {
    // 1. Get connection IP
    let conn_ip = request.connection_info().realip_remote_addr()
        .and_then(|s| s.parse().ok())
        .unwrap_or(IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    
    // 2. Only trust X-Forwarded-For if from trusted proxy
    if trusted_proxies.contains(&conn_ip) {
        if let Some(xff) = request.headers().get("X-Forwarded-For") {
            if let Ok(xff_str) = xff.to_str() {
                // Take last IP before trusted proxy
                let ips: Vec<&str> = xff_str.split(',')
                    .map(|s| s.trim())
                    .collect();
                for ip in ips.iter().rev() {
                    if let Ok(parsed) = ip.parse::<IpAddr>() {
                        if !trusted_proxies.contains(&parsed) {
                            return parsed;
                        }
                    }
                }
            }
        }
    }
    
    conn_ip
}
```

#### ReDoS (Regex Denial of Service) Protection

```rust
use regex::Regex;
use std::time::Duration;

/// Safe regex execution with timeout
pub struct SafeRegex {
    inner: Regex,
    max_input_len: usize,
}

impl SafeRegex {
    pub fn new(pattern: &str, max_input_len: usize) -> Result<Self, RegexError> {
        // Use regex crate which has built-in protections against catastrophic backtracking
        let inner = Regex::new(pattern)?;
        Ok(Self { inner, max_input_len })
    }
    
    pub fn is_match(&self, text: &str) -> bool {
        // Reject oversized inputs
        if text.len() > self.max_input_len {
            return false;
        }
        self.inner.is_match(text)
    }
}

// Anti-pattern: Vulnerable regex patterns
// BAD: (a+)+     - Nested quantifiers
// BAD: (a|a)+    - Overlapping alternatives with quantifiers
// BAD: (.*a.*)+  - Greedy with backtracking
```

### 32.6 Secret Management

#### Environment Variable Security

```rust
use zeroize::Zeroize;

/// Secure secret loading with zeroization
#[derive(Zeroize)]
#[zeroize(drop)]
pub struct Secret(String);

impl Secret {
    pub fn from_env(name: &str) -> Result<Self, SecretError> {
        let value = std::env::var(name)
            .map_err(|_| SecretError::NotFound(name.to_string()))?;
        
        // Remove from environment after loading (defense in depth)
        std::env::remove_var(name);
        
        Ok(Self(value))
    }
    
    pub fn expose(&self) -> &str {
        &self.0
    }
}

/// Secret validation at startup
pub fn validate_required_secrets(names: &[&str]) -> Result<(), ConfigError> {
    let mut missing = Vec::new();
    
    for name in names {
        if std::env::var(name).is_err() {
            missing.push(*name);
        }
    }
    
    if !missing.is_empty() {
        return Err(ConfigError::MissingSecrets(missing));
    }
    
    Ok(())
}
```

#### API Key Best Practices

```rust
// ANTI-PATTERN: Secret in URL (logged by proxies, browsers, servers)
// BAD:
async fn bad_api_call() {
    let url = format!(
        "https://api.example.com/data?api_key={}",
        api_secret  // SECRET IN URL - DANGEROUS!
    );
    reqwest::get(&url).await?;
}

// GOOD: Secret in header
async fn good_api_call(api_secret: &str) -> Result<Response, Error> {
    let client = reqwest::Client::new();
    client.get("https://api.example.com/data")
        .header("Authorization", format!("Bearer {}", api_secret))
        .send()
        .await
}

// GOOD: Use environment-specific secrets
pub struct ApiClient {
    base_url: String,
    secret: Secret,
}

impl ApiClient {
    pub fn from_env() -> Result<Self, ConfigError> {
        Ok(Self {
            base_url: std::env::var("API_BASE_URL")?,
            secret: Secret::from_env("API_SECRET")?,
        })
    }
}
```

### 32.7 Command Execution Security

#### Safe Command Execution Patterns

```rust
use std::process::{Command, Stdio};

/// Whitelist-based command execution
pub struct CommandExecutor {
    allowed_commands: HashSet<String>,
    allowed_cwd: PathBuf,
}

impl CommandExecutor {
    pub fn execute(
        &self,
        command: &str,
        args: &[&str],
        cwd: Option<&Path>,
    ) -> Result<Output, SecurityError> {
        // 1. Validate command is whitelisted
        if !self.allowed_commands.contains(command) {
            return Err(SecurityError::CommandNotWhitelisted(command.to_string()));
        }
        
        // 2. Validate working directory
        let working_dir = cwd.unwrap_or(&self.allowed_cwd);
        if !working_dir.starts_with(&self.allowed_cwd) {
            return Err(SecurityError::InvalidWorkingDirectory);
        }
        
        // 3. Validate arguments (no shell metacharacters)
        for arg in args {
            self.validate_argument(arg)?;
        }
        
        // 4. Execute without shell
        Command::new(command)
            .args(args)
            .current_dir(working_dir)
            .stdin(Stdio::null())  // Prevent stdin attacks
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| SecurityError::ExecutionFailed(e.to_string()))
    }
    
    fn validate_argument(&self, arg: &str) -> Result<(), SecurityError> {
        // Reject shell metacharacters
        let forbidden = ['|', ';', '&', '$', '`', '(', ')', '{', '}', 
                         '[', ']', '<', '>', '\\', '"', '\'', '\n', '\r'];
        
        for c in arg.chars() {
            if forbidden.contains(&c) {
                return Err(SecurityError::ForbiddenCharacter(c));
            }
        }
        
        // Reject null bytes
        if arg.contains('\0') {
            return Err(SecurityError::NullByteInArgument);
        }
        
        Ok(())
    }
}

/// Heredoc analysis for embedded script detection
pub fn analyze_heredoc(content: &str) -> Vec<SecurityWarning> {
    let mut warnings = Vec::new();
    
    // Detect embedded scripts
    let script_patterns = [
        (r"python\s+-c", "Embedded Python code"),
        (r"node\s+-e", "Embedded Node.js code"),
        (r"ruby\s+-e", "Embedded Ruby code"),
        (r"perl\s+-e", "Embedded Perl code"),
        (r"\$\(.*\)", "Command substitution"),
        (r"`.*`", "Backtick command substitution"),
    ];
    
    for (pattern, description) in script_patterns {
        if Regex::new(pattern).unwrap().is_match(content) {
            warnings.push(SecurityWarning {
                severity: Severity::High,
                description: description.to_string(),
            });
        }
    }
    
    warnings
}
```

### 32.8 Security Audit Report Template

```markdown
## Security Audit Report

### Executive Summary
[Brief overview of findings: X critical, Y high, Z medium, W low severity issues]

---

### Critical Vulnerabilities

#### 1. [Title] (CRITICAL)
**File:** `path/to/file.rs` (Line XXX)

**Issue:** [Description of the vulnerability]

**Code:**
```rust
// Vulnerable code snippet
```

**Proof of Concept:**
[Steps to exploit or demonstrate the issue]

**Recommendation:**
```rust
// Fixed code snippet
```

**CWE Reference:** CWE-XXX

---

### High Severity Issues
[Similar format]

---

### Medium Severity Issues
[Similar format]

---

### Low Severity Issues
[Similar format]

---

### Security Strengths
- ✓ [Positive finding 1]
- ✓ [Positive finding 2]

---

### Recommendations Summary

| Priority | Issue | Effort |
|----------|-------|--------|
| P0 | [Critical fix] | Low |
| P1 | [High fix] | Medium |
| P2 | [Medium fix] | Low |

---

### Compliance Notes
- [ ] OWASP Top 10 coverage
- [ ] CWE coverage
- [ ] Industry standard alignment
```

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

```rust
// The error handling spectrum
// 
// Recoverable (Result)                          Unrecoverable (panic)
// ├────────────────────────────────────────────────────────────────┤
// │ File not found    Network timeout    Invalid input │ Invariant │
// │ Permission denied Rate limited       Parse error   │ violated  │
// │ Resource busy     Service unavailable              │ Bug/logic │
// └────────────────────────────────────────────────────────────────┘
```

### 33.2 The thiserror and anyhow Dichotomy

**thiserror** is for library code - create specific, matchable error types:

```rust
use thiserror::Error;

/// Domain-specific error type for skill operations.
/// 
/// Use thiserror when:
/// - Building a library consumed by others
/// - Callers need to match on specific error variants
/// - You want to expose a stable error API
#[derive(Debug, Error)]
pub enum SkillError {
    #[error("skill not found: {name}")]
    NotFound { name: String },
    
    #[error("invalid skill manifest at line {line}: {reason}")]
    InvalidManifest { line: usize, reason: String },
    
    #[error("skill execution failed: {0}")]
    ExecutionFailed(#[source] std::io::Error),
    
    #[error("dependency cycle detected: {path}")]
    CycleDetected { path: String },
    
    #[error("rate limited (retry after {retry_after_secs}s)")]
    RateLimited { retry_after_secs: u64 },
    
    #[error(transparent)]
    Internal(#[from] anyhow::Error),
}

// Enable conversion from io::Error
impl From<std::io::Error> for SkillError {
    fn from(err: std::io::Error) -> Self {
        SkillError::ExecutionFailed(err)
    }
}
```

**anyhow** is for application code - rich context chains without ceremony:

```rust
use anyhow::{Context, Result, bail, ensure};

/// Load and validate a skill file.
/// 
/// Use anyhow when:
/// - Building an application (not a library)
/// - You want rich error context without boilerplate
/// - Errors will be displayed to users, not programmatically matched
pub fn load_skill(path: &Path) -> Result<Skill> {
    // .context() adds human-readable context to errors
    let content = std::fs::read_to_string(path)
        .with_context(|| format!("failed to read skill file at {}", path.display()))?;
    
    // Parse with context
    let manifest: SkillManifest = toml::from_str(&content)
        .with_context(|| format!("invalid TOML in skill manifest: {}", path.display()))?;
    
    // bail! for early returns with error
    if manifest.version != SUPPORTED_VERSION {
        bail!(
            "unsupported skill version {} (expected {})",
            manifest.version,
            SUPPORTED_VERSION
        );
    }
    
    // ensure! for assertions that return errors
    ensure!(
        !manifest.name.is_empty(),
        "skill name cannot be empty in {}",
        path.display()
    );
    
    Ok(Skill::from_manifest(manifest))
}
```

### 33.3 Structured CLI Error Types

For CLI applications, create a structured error type that maps to exit codes:

```rust
/// Structured CLI error with exit code mapping.
/// 
/// Exit codes follow Unix conventions:
/// - 0: Success
/// - 1: General error
/// - 2: Usage error (invalid arguments)
/// - 3: Not found
/// - 4: Permission denied
/// - 5: Conflict
/// - 6: Network/external service error
/// - 7: Resource exhausted
/// - 8: Timeout
/// - 9: Internal error (bug)
#[derive(Debug, Clone)]
pub struct CliError {
    /// Unix exit code (0-9)
    pub code: i32,
    /// Error category for programmatic handling
    pub kind: &'static str,
    /// Human-readable message
    pub message: String,
    /// Actionable suggestion for the user
    pub hint: Option<String>,
    /// Whether the operation can be retried
    pub retryable: bool,
}

impl CliError {
    pub fn usage(message: impl Into<String>, hint: Option<String>) -> Self {
        Self {
            code: 2,
            kind: "usage",
            message: message.into(),
            hint,
            retryable: false,
        }
    }
    
    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            code: 3,
            kind: "not_found",
            message: message.into(),
            hint: None,
            retryable: false,
        }
    }
    
    pub fn network(message: impl Into<String>, retryable: bool) -> Self {
        Self {
            code: 6,
            kind: "network",
            message: message.into(),
            hint: if retryable {
                Some("You may retry this operation".to_string())
            } else {
                None
            },
            retryable,
        }
    }
    
    pub fn timeout(message: impl Into<String>) -> Self {
        Self {
            code: 8,
            kind: "timeout",
            message: message.into(),
            hint: Some("Consider increasing timeout or checking network".to_string()),
            retryable: true,
        }
    }
    
    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            code: 9,
            kind: "internal",
            message: message.into(),
            hint: Some("This is a bug. Please report it.".to_string()),
            retryable: false,
        }
    }
    
    /// Format for machine consumption (--robot mode)
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "status": "error",
            "code": self.code,
            "kind": self.kind,
            "message": self.message,
            "hint": self.hint,
            "retryable": self.retryable
        })
    }
}

pub type CliResult<T = ()> = std::result::Result<T, CliError>;
```

### 33.4 Error Taxonomy Patterns

For protocol or API libraries, define a comprehensive error taxonomy:

```rust
/// FCP-style error taxonomy with numeric codes.
/// 
/// Code ranges:
/// - 1xxx: Protocol errors
/// - 2xxx: Authentication/identity errors
/// - 3xxx: Capability/authorization errors
/// - 4xxx: Zone/topology errors
/// - 5xxx: Lifecycle/health errors
/// - 6xxx: Resource errors
/// - 7xxx: External service errors
/// - 9xxx: Internal errors
#[derive(Debug, Clone)]
pub struct FcpError {
    /// Error code (e.g., "FCP-2001")
    pub code: String,
    /// Human-readable message
    pub message: String,
    /// Whether the operation can be retried
    pub retryable: bool,
    /// Suggested retry delay in milliseconds
    pub retry_after_ms: Option<u64>,
    /// Additional structured context
    pub details: serde_json::Value,
    /// AI-friendly recovery suggestion
    pub ai_recovery_hint: Option<String>,
}

impl FcpError {
    /// Create protocol error (1xxx)
    pub fn protocol(code: u16, message: impl Into<String>) -> Self {
        Self {
            code: format!("FCP-{code}"),
            message: message.into(),
            retryable: false,
            retry_after_ms: None,
            details: serde_json::json!({}),
            ai_recovery_hint: None,
        }
    }
    
    /// Create auth error (2xxx)
    pub fn auth(code: u16, message: impl Into<String>) -> Self {
        Self {
            code: format!("FCP-{code}"),
            message: message.into(),
            retryable: false,
            retry_after_ms: None,
            details: serde_json::json!({}),
            ai_recovery_hint: Some("Check credentials and permissions".to_string()),
        }
    }
    
    /// Create rate limit error (7xxx)
    pub fn rate_limited(retry_after_ms: u64) -> Self {
        Self {
            code: "FCP-7429".to_string(),
            message: "Rate limit exceeded".to_string(),
            retryable: true,
            retry_after_ms: Some(retry_after_ms),
            details: serde_json::json!({
                "limit_type": "requests_per_minute"
            }),
            ai_recovery_hint: Some(format!(
                "Wait {}ms before retrying",
                retry_after_ms
            )),
        }
    }
    
    /// Check if error is in a specific range
    pub fn is_protocol_error(&self) -> bool {
        self.code.starts_with("FCP-1")
    }
    
    pub fn is_auth_error(&self) -> bool {
        self.code.starts_with("FCP-2")
    }
    
    pub fn is_external_error(&self) -> bool {
        self.code.starts_with("FCP-7")
    }
}

/// Common error codes reference
pub mod error_codes {
    // Protocol errors (1xxx)
    pub const INVALID_MESSAGE: u16 = 1001;
    pub const VERSION_MISMATCH: u16 = 1002;
    pub const MALFORMED_REQUEST: u16 = 1003;
    pub const CHECKSUM_MISMATCH: u16 = 1004;
    
    // Auth errors (2xxx)
    pub const TOKEN_EXPIRED: u16 = 2001;
    pub const TOKEN_INVALID: u16 = 2002;
    pub const INSUFFICIENT_SCOPE: u16 = 2003;
    
    // Resource errors (6xxx)
    pub const RESOURCE_NOT_FOUND: u16 = 6001;
    pub const RESOURCE_EXHAUSTED: u16 = 6002;
    pub const RESOURCE_LOCKED: u16 = 6003;
    
    // External errors (7xxx)
    pub const EXTERNAL_UNAVAILABLE: u16 = 7001;
    pub const EXTERNAL_TIMEOUT: u16 = 7002;
    pub const RATE_LIMITED: u16 = 7429;
    
    // Internal errors (9xxx)
    pub const INTERNAL_ERROR: u16 = 9001;
    pub const NOT_IMPLEMENTED: u16 = 9002;
}
```

### 33.5 Error Context Chaining

Build rich error chains that explain the full failure path:

```rust
use anyhow::{Context, Result};

/// Error context should read as a stack trace from specific to general.
/// 
/// Example output:
/// Error: failed to execute skill "git-commit"
/// 
/// Caused by:
///     0: failed to render template "commit_message.md"
///     1: variable 'ticket_id' not found
///     2: missing required context field
pub fn execute_skill(name: &str, context: &SkillContext) -> Result<()> {
    let skill = load_skill(name)
        .with_context(|| format!("failed to load skill '{name}'"))?;
    
    let template = skill.template()
        .with_context(|| format!("failed to get template for skill '{name}'"))?;
    
    let rendered = template.render(context)
        .with_context(|| format!("failed to render template for skill '{name}'"))?;
    
    execute_command(&rendered)
        .with_context(|| format!("failed to execute skill '{name}'"))?;
    
    Ok(())
}

/// For errors crossing module boundaries, wrap with From implementations
impl From<TemplateError> for SkillError {
    fn from(err: TemplateError) -> Self {
        match err {
            TemplateError::VariableNotFound { name } => {
                SkillError::InvalidManifest {
                    line: 0,
                    reason: format!("missing template variable: {name}"),
                }
            }
            TemplateError::SyntaxError { line, message } => {
                SkillError::InvalidManifest { line, reason: message }
            }
            other => SkillError::Internal(other.into()),
        }
    }
}
```

### 33.6 Error Recovery Patterns

Implement retry logic with exponential backoff:

```rust
use std::time::Duration;
use tokio::time::sleep;

/// Retry configuration
#[derive(Debug, Clone)]
pub struct RetryConfig {
    /// Maximum number of retry attempts (not counting initial attempt).
    /// Example: max_retries=3 means up to 4 total attempts (1 initial + 3 retries).
    pub max_retries: u32,
    /// Initial delay before first retry
    pub initial_delay: Duration,
    /// Maximum delay between retries
    pub max_delay: Duration,
    /// Multiplier for exponential backoff
    pub backoff_factor: f64,
    /// Add random jitter to prevent thundering herd
    pub jitter: bool,
}

impl Default for RetryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(30),
            backoff_factor: 2.0,
            jitter: true,
        }
    }
}

/// Retry an operation with exponential backoff
pub async fn with_retry<T, E, F, Fut>(
    config: &RetryConfig,
    mut operation: F,
) -> Result<T, E>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: std::fmt::Debug,
{
    let mut attempt = 0;
    let mut delay = config.initial_delay;
    
    loop {
        match operation().await {
            Ok(result) => return Ok(result),
            Err(err) if attempt >= config.max_retries => {
                tracing::error!(?err, "operation failed after {} attempts", attempt + 1);
                return Err(err);
            }
            Err(err) => {
                attempt += 1;
                tracing::warn!(
                    ?err,
                    attempt,
                    max_retries = config.max_retries,
                    "operation failed, retrying after {:?}",
                    delay
                );
                
                // Add jitter: +/- 25%
                let actual_delay = if config.jitter {
                    let jitter_factor = 0.75 + rand::random::<f64>() * 0.5;
                    delay.mul_f64(jitter_factor)
                } else {
                    delay
                };
                
                sleep(actual_delay).await;
                
                // Exponential backoff with cap
                delay = std::cmp::min(
                    delay.mul_f64(config.backoff_factor),
                    config.max_delay,
                );
            }
        }
    }
}

/// Circuit breaker for preventing cascading failures
#[derive(Debug)]
pub struct CircuitBreaker {
    /// Number of failures before opening
    failure_threshold: u32,
    /// Duration to stay open before half-open
    reset_timeout: Duration,
    /// Current state
    state: std::sync::Mutex<CircuitState>,
}

#[derive(Debug)]
enum CircuitState {
    Closed { failures: u32 },
    Open { opened_at: std::time::Instant },
    /// HalfOpen allows exactly one probe request to test if service recovered.
    /// `probing: true` means a probe is in flight; reject additional requests.
    HalfOpen { probing: bool },
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, reset_timeout: Duration) -> Self {
        Self {
            failure_threshold,
            reset_timeout,
            state: std::sync::Mutex::new(CircuitState::Closed { failures: 0 }),
        }
    }
    
    /// Check if the circuit allows requests
    pub fn allow_request(&self) -> bool {
        let mut state = self.state.lock().unwrap();
        match *state {
            CircuitState::Closed { .. } => true,
            CircuitState::Open { opened_at } => {
                if opened_at.elapsed() >= self.reset_timeout {
                    // Transition to HalfOpen with probe starting
                    *state = CircuitState::HalfOpen { probing: true };
                    true
                } else {
                    false
                }
            }
            CircuitState::HalfOpen { probing } => {
                // Only allow one probe at a time in HalfOpen state
                if probing {
                    false  // Probe already in flight, reject
                } else {
                    *state = CircuitState::HalfOpen { probing: true };
                    true
                }
            }
        }
    }
    
    /// Record a successful operation
    pub fn record_success(&self) {
        let mut state = self.state.lock().unwrap();
        *state = CircuitState::Closed { failures: 0 };
    }
    
    /// Record a failed operation
    pub fn record_failure(&self) {
        let mut state = self.state.lock().unwrap();
        match *state {
            CircuitState::Closed { failures } => {
                let new_failures = failures + 1;
                if new_failures >= self.failure_threshold {
                    *state = CircuitState::Open {
                        opened_at: std::time::Instant::now(),
                    };
                } else {
                    *state = CircuitState::Closed { failures: new_failures };
                }
            }
            CircuitState::HalfOpen { .. } => {
                // Probe failed, go back to Open
                *state = CircuitState::Open {
                    opened_at: std::time::Instant::now(),
                };
            }
            CircuitState::Open { .. } => {}
        }
    }
}
```

### 33.7 Panic vs Result Guidelines

**When to use panic (via `unwrap`, `expect`, `unreachable!`):**

```rust
// ✓ CORRECT: Static data that cannot fail at runtime
static REGEX: LazyLock<Regex> = LazyLock::new(|| {
    // This regex is constant - if it fails, it's a programmer error
    Regex::new(r"^\w+$").expect("static regex must compile")
});

// ✓ CORRECT: Invariant that would indicate a bug
fn pop_from_non_empty_stack(stack: &mut Vec<i32>) -> i32 {
    // Caller guarantees non-empty; if empty, it's a bug
    stack.pop().expect("stack should not be empty")
}

// ✓ CORRECT: Test code
#[test]
fn test_parsing() {
    let result = parse("valid input").unwrap(); // Test should fail if this errors
    assert_eq!(result.value, 42);
}

// ✓ CORRECT: unreachable! for exhaustive matches
match state {
    State::Ready => process(),
    State::Running => wait(),
    State::Complete => return,
    // Enum is non_exhaustive, but we handle all current variants
    _ => unreachable!("unknown state variant"),
}
```

**When to use Result (proper error handling):**

```rust
// ✗ WRONG: User input can fail - don't panic
fn parse_config(input: &str) -> Config {
    toml::from_str(input).unwrap() // DON'T: user input can be invalid
}

// ✓ CORRECT: Return Result for user input
fn parse_config(input: &str) -> Result<Config, ConfigError> {
    toml::from_str(input).map_err(|e| ConfigError::ParseFailed(e.to_string()))
}

// ✗ WRONG: File operations can fail - don't panic
fn read_settings() -> Settings {
    let content = std::fs::read_to_string("settings.toml").unwrap(); // DON'T
    toml::from_str(&content).unwrap() // DON'T
}

// ✓ CORRECT: Return Result for IO operations
fn read_settings() -> Result<Settings> {
    let content = std::fs::read_to_string("settings.toml")
        .context("failed to read settings file")?;
    toml::from_str(&content)
        .context("failed to parse settings file")
}

// ✗ WRONG: Network operations can fail - don't panic
async fn fetch_data(url: &str) -> Data {
    reqwest::get(url).await.unwrap().json().await.unwrap() // DON'T
}

// ✓ CORRECT: Return Result for network operations
async fn fetch_data(url: &str) -> Result<Data> {
    let response = reqwest::get(url).await
        .with_context(|| format!("failed to fetch {url}"))?;
    
    response.json().await
        .with_context(|| format!("failed to parse response from {url}"))
}
```

### 33.8 Error Boundary Patterns

For systems with multiple error domains, create clear boundaries:

```rust
/// Library error type - specific, matchable variants
#[derive(Debug, Error)]
pub enum LibraryError {
    #[error("validation failed: {0}")]
    Validation(String),
    
    #[error("resource not found: {0}")]
    NotFound(String),
    
    #[error("operation timed out after {0:?}")]
    Timeout(Duration),
}

/// Application error type - broader categories
#[derive(Debug)]
pub enum AppError {
    /// User-facing error with helpful message
    User { message: String, hint: Option<String> },
    /// Internal error (log and show generic message)
    Internal { message: String, source: anyhow::Error },
}

/// Convert library errors to application errors at the boundary
impl From<LibraryError> for AppError {
    fn from(err: LibraryError) -> Self {
        match err {
            LibraryError::Validation(msg) => AppError::User {
                message: format!("Invalid input: {msg}"),
                hint: Some("Check your input and try again".to_string()),
            },
            LibraryError::NotFound(resource) => AppError::User {
                message: format!("Not found: {resource}"),
                hint: Some("The requested resource does not exist".to_string()),
            },
            LibraryError::Timeout(duration) => AppError::User {
                message: format!("Operation timed out after {duration:?}"),
                hint: Some("The service may be busy. Try again later.".to_string()),
            },
        }
    }
}

/// Display errors appropriately based on context
impl AppError {
    pub fn display(&self, verbose: bool) -> String {
        match self {
            AppError::User { message, hint } => {
                if let Some(h) = hint {
                    format!("{message}\n\nHint: {h}")
                } else {
                    message.clone()
                }
            }
            AppError::Internal { message, source } => {
                if verbose {
                    format!("Internal error: {message}\n\nCause: {source:?}")
                } else {
                    "An internal error occurred. Run with --verbose for details.".to_string()
                }
            }
        }
    }
}
```

### 33.9 Error Logging Best Practices

```rust
use tracing::{error, warn, info, debug, instrument};

/// Instrument functions that can fail for automatic error logging
#[instrument(skip(content), fields(path = %path.display()))]
pub fn save_file(path: &Path, content: &[u8]) -> Result<()> {
    std::fs::write(path, content)
        .with_context(|| format!("failed to write to {}", path.display()))?;
    
    info!(bytes = content.len(), "file saved successfully");
    Ok(())
}

/// Log errors with appropriate severity
pub fn handle_error(err: &AppError) {
    match err {
        // User errors are warnings - user's fault, not ours
        AppError::User { message, .. } => {
            warn!(error = %message, "user error");
        }
        // Internal errors are errors - something we need to fix
        AppError::Internal { message, source } => {
            error!(
                error = %message,
                cause = ?source,
                "internal error"
            );
        }
    }
}

/// Structured error logging for monitoring
pub fn log_structured_error(err: &CliError) {
    tracing::error!(
        code = err.code,
        kind = err.kind,
        message = %err.message,
        retryable = err.retryable,
        "cli error"
    );
}
```

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

```
| Category           | Approach                      | Rationale                           |
|--------------------|-------------------------------|-------------------------------------|
| External APIs      | Real (or conditional skip)    | Tests reflect actual behavior       |
| Animations         | Mocked (framer-motion)        | Need predictable test execution     |
| Network requests   | Stubbed selectively           | Only for isolated client tests      |
| File system        | Real with temp directories    | Tests actual fs operations          |
| Database/Crypto    | Real implementations          | Test actual algorithms              |
| DOM APIs           | Real via happy-dom            | Tests actual rendering              |
| React hooks        | Real via vitest               | Tests real hook behavior            |
```

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

```typescript
import { describe, it, expect, beforeEach, afterEach } from "bun:test";

// Capture and restore globals
let output: string[] = [];
let errors: string[] = [];
let exitCode: number | undefined;

const originalLog = console.log;
const originalError = console.error;
const originalExit = process.exit;

beforeEach(() => {
  output = [];
  errors = [];
  exitCode = undefined;

  console.log = (...args: unknown[]) => {
    output.push(args.join(" "));
  };
  console.error = (...args: unknown[]) => {
    errors.push(args.join(" "));
  };
  process.exit = ((code?: number) => {
    exitCode = code ?? 0;
    throw new Error("process.exit");
  }) as never;
});

afterEach(() => {
  console.log = originalLog;
  console.error = originalError;
  process.exit = originalExit;
});

describe("commandHandler", () => {
  it("handles valid input with JSON output", async () => {
    try {
      await commandHandler("valid-input", { json: true });
    } catch (e) {
      if ((e as Error).message !== "process.exit") throw e;
    }

    const allOutput = output.join("\n");
    const parsed = JSON.parse(allOutput);
    expect(parsed.success).toBe(true);
  });

  it("exits with error for missing input", async () => {
    try {
      await commandHandler("nonexistent", { json: true });
    } catch (e) {
      if ((e as Error).message !== "process.exit") throw e;
    }

    expect(exitCode).toBe(1);
    expect(output.join("\n")).toContain("not_found");
  });
});
```

#### Go Table-Driven Tests

```go
func TestEvaluateCommand(t *testing.T) {
    tests := []struct {
        name     string
        command  string
        config   *Config
        want     EvaluationResult
    }{
        {
            name:    "allowed_simple_command",
            command: "ls -la",
            config:  DefaultConfig(),
            want:    EvaluationResult{Allowed: true},
        },
        {
            name:    "blocked_destructive_command",
            command: "rm -rf /",
            config:  DefaultConfig(),
            want:    EvaluationResult{Allowed: false, Reason: "destructive"},
        },
        {
            name:    "config_override_allows",
            command: "git push --force",
            config:  ConfigWithOverride("allow", "git push"),
            want:    EvaluationResult{Allowed: true, Via: "override"},
        },
    }

    for _, tt := range tests {
        t.Run(tt.name, func(t *testing.T) {
            got := EvaluateCommand(tt.command, tt.config)
            if got != tt.want {
                t.Errorf("EvaluateCommand(%q) = %v, want %v",
                    tt.command, got, tt.want)
            }
        })
    }
}
```

#### Test File Naming Conventions

| Language | Pattern | Example |
|----------|---------|---------|
| **TypeScript** | `*.test.ts`, `*.test.tsx` | `copy.test.ts`, `Button.test.tsx` |
| **Go** | `*_test.go` | `evaluator_test.go` |
| **Rust** | `mod tests` in same file, or `/tests/*.rs` | `mod tests { ... }` |
| **Bash** | `*.bats` (BATS framework) | `test_utils.bats` |

### 34.3 Test Fixture Patterns

#### Real Filesystem Fixtures

```typescript
import { mkdtempSync, rmSync, existsSync, readFileSync, writeFileSync, mkdirSync } from "fs";
import { join } from "path";
import { tmpdir } from "os";

let testDir: string;
let originalCwd: string;

beforeAll(() => {
  testDir = mkdtempSync(join(tmpdir(), "myapp-test-"));
  originalCwd = process.cwd();
});

afterAll(() => {
  try {
    rmSync(testDir, { recursive: true, force: true });
  } catch (e) {
    console.error("Failed to cleanup test dir:", e);
  }
});

beforeEach(() => {
  process.chdir(testDir);
});

afterEach(() => {
  process.chdir(originalCwd);
});

// Helper to create test fixtures
function writeManifest(dir: string, entries: Array<Record<string, string>>) {
  const manifest = {
    generatedAt: "2026-01-01T00:00:00.000Z",
    version: "1.0.0",
    entries,
  };
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, "manifest.json"), JSON.stringify(manifest));
}
```

#### Go Test Fixtures with t.TempDir()

```go
func TestFileOperations(t *testing.T) {
    // t.TempDir() automatically cleans up after test
    testDir := t.TempDir()

    // Create test fixture
    configPath := filepath.Join(testDir, "config.yaml")
    err := os.WriteFile(configPath, []byte("key: value"), 0644)
    if err != nil {
        t.Fatalf("failed to create fixture: %v", err)
    }

    // Test the operation
    result, err := LoadConfig(configPath)
    if err != nil {
        t.Fatalf("LoadConfig() error = %v", err)
    }

    if result.Key != "value" {
        t.Errorf("LoadConfig().Key = %q, want %q", result.Key, "value")
    }
}
```

#### Environment Variable Isolation

```typescript
// Store original env before tests
let originalEnv: string | undefined;

beforeAll(() => {
  originalEnv = process.env.MY_APP_HOME;
  // Redirect app config to temp directory
  process.env.MY_APP_HOME = testDir;
});

afterAll(() => {
  if (originalEnv === undefined) {
    delete process.env.MY_APP_HOME;
  } else {
    process.env.MY_APP_HOME = originalEnv;
  }
});
```

### 34.4 Property-Based Testing

#### Rust with proptest

**Source**: CASS mining of destructive_command_guard property tests

```rust
#[cfg(test)]
mod proptest_invariants {
    use super::*;
    use proptest::prelude::*;

    /// Generate realistic command strings for testing
    fn command_strategy() -> impl Strategy<Value = String> {
        prop_oneof![
            // Simple commands
            "[a-zA-Z][a-zA-Z0-9_\\-]{0,50}( [a-zA-Z0-9_\\-./]+){0,10}",
            // Commands with pipes
            "[a-z]+ [a-z]+ \\| [a-z]+",
            // Commands with redirects
            "[a-z]+ [a-z]+ > [a-z]+\\.txt",
            // Edge cases
            Just(String::new()),
            Just(" ".to_string()),
            Just("\t\n".to_string()),
        ]
    }

    proptest! {
        /// Normalization should be idempotent: normalize(normalize(x)) == normalize(x)
        #[test]
        fn normalization_is_idempotent(cmd in command_strategy()) {
            let once = normalize_command(&cmd);
            let twice = normalize_command(&once);
            prop_assert_eq!(once, twice, "normalization not idempotent");
        }

        /// Same input should always produce same output
        #[test]
        fn evaluation_is_deterministic(cmd in command_strategy()) {
            let config = Config::default();
            let compiled = config.compile_overrides().unwrap();

            let result1 = evaluate_command(&cmd, &config, &[], &compiled);
            let result2 = evaluate_command(&cmd, &config, &[], &compiled);

            prop_assert_eq!(result1, result2, "evaluation not deterministic");
        }

        /// Function should never panic, regardless of input
        #[test]
        fn evaluation_never_panics(cmd in ".*") {
            let config = Config::default();
            let compiled = config.compile_overrides().unwrap();

            // If this completes without panic, test passes
            let _ = evaluate_command(&cmd, &config, &[], &compiled);
        }

        /// Large inputs should be handled gracefully
        #[test]
        fn handles_large_inputs(size in 1000usize..10000) {
            let cmd = "x".repeat(size);
            let config = Config::default();
            let compiled = config.compile_overrides().unwrap();

            // Should complete in reasonable time, not hang
            let _ = evaluate_command(&cmd, &config, &[], &compiled);
        }
    }
}
```

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

```markdown
## Test Coverage Analysis Report

### Executive Summary
- **Total Packages**: 23
- **Total Test Files**: 55
- **Overall Coverage**: ~70%
- **Test Strategy**: Real implementations > mocks

### Package Coverage

| Package | Files | Test Files | Coverage | Gaps |
|---------|-------|------------|----------|------|
| authfile | 1 | 3 | 100% | None |
| config | 2 | 2 | 100% | None |
| refresh | 5 | 3 | 40% | orchestration untested |
| sync | 9 | 5 | 30% | pool.go, connpool.go |

### Gap Analysis

**Critical Gaps** (test immediately):
- `sync/pool.go` - 20+ untested methods
- `bundle/encrypt.go` - security-critical, untested

**High Priority** (test within sprint):
- `refresh/refresh.go` - core orchestration
- `tui/sync_panel.go` - user-facing UI

**Recommendations**:
1. Continue using real implementations
2. Use temp directories for file operations
3. Add e2e tests for complex workflows
4. Use table-driven tests for validation
```

### 34.6 Snapshot Testing

#### Vitest/Jest Snapshot Pattern

```typescript
import { describe, it, expect } from "vitest";

describe("Renderer", () => {
  it("renders skill card correctly", () => {
    const result = renderSkillCard({
      title: "Test Skill",
      description: "A test description",
      tags: ["rust", "cli"],
    });

    expect(result).toMatchSnapshot();
  });

  it("renders empty state correctly", () => {
    const result = renderSkillCard(null);
    expect(result).toMatchSnapshot();
  });
});
```

#### Managing Snapshot Updates

```bash
# Update snapshots when intentional changes occur
bun run test -- --update-snapshots

# Review changes in version control
git diff __snapshots__/

# Common issues:
# - Obsolete snapshots: tests renamed but snapshots not deleted
# - Different runner formats: bun test vs vitest produce different keys
# - Timestamp drift: ensure deterministic dates in tests
```

### 34.7 E2E Testing Patterns

#### Playwright Configuration

**Source**: CASS mining of brenner_bot E2E test infrastructure

```typescript
// playwright.config.ts
import { defineConfig, devices } from '@playwright/test';

export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: 'html',

  use: {
    baseURL: 'http://localhost:3000',
    trace: 'on-first-retry',
  },

  projects: [
    {
      name: 'chromium',
      use: { ...devices['Desktop Chrome'] },
    },
    {
      name: 'firefox',
      use: { ...devices['Desktop Firefox'] },
    },
    {
      name: 'webkit',
      use: { ...devices['Desktop Safari'] },
    },
    {
      name: 'Mobile Chrome',
      use: { ...devices['Pixel 5'] },
    },
    {
      name: 'Mobile Safari',
      use: { ...devices['iPhone 12'] },
    },
  ],

  webServer: {
    command: 'bun run dev',
    url: 'http://localhost:3000',
    reuseExistingServer: !process.env.CI,
  },
});
```

#### E2E Test Structure

```typescript
import { test, expect } from '@playwright/test';

test.describe('Skill Browser', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/skills');
  });

  test('displays skill list', async ({ page }) => {
    await expect(page.getByRole('heading', { name: 'Skills' })).toBeVisible();
    await expect(page.locator('.skill-card')).toHaveCount(10);
  });

  test('filters skills by category', async ({ page }) => {
    await page.getByRole('button', { name: 'CLI' }).click();

    // Wait for filter to apply
    await expect(page.locator('.skill-card')).toHaveCountLessThan(10);

    // All visible cards should have CLI tag
    const cards = page.locator('.skill-card');
    for (const card of await cards.all()) {
      await expect(card.locator('.tag-cli')).toBeVisible();
    }
  });

  test('opens skill detail on click', async ({ page }) => {
    await page.locator('.skill-card').first().click();
    await expect(page).toHaveURL(/\/skills\/[\w-]+/);
    await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
  });
});
```

### 34.8 BATS Framework for Shell Testing

**Source**: CASS mining of APR BATS test infrastructure

#### Test Helper Structure

```bash
# tests/helpers/test_helper.bash

# Load BATS libraries
load '../libs/bats-support/load'
load '../libs/bats-assert/load'

# Setup isolated test environment
setup_test_environment() {
    export TEST_DIR="$(mktemp -d)"
    export HOME="$TEST_DIR/home"
    export XDG_CONFIG_HOME="$TEST_DIR/.config"
    mkdir -p "$HOME" "$XDG_CONFIG_HOME"

    # Disable interactive features
    export NO_COLOR=1
    export CI=true
    export APR_NO_GUM=1
}

teardown_test_environment() {
    rm -rf "$TEST_DIR"
}

# Helper to capture both streams
capture_streams() {
    local cmd="$1"
    STDOUT_FILE="$(mktemp)"
    STDERR_FILE="$(mktemp)"

    eval "$cmd" > "$STDOUT_FILE" 2> "$STDERR_FILE"
    EXIT_CODE=$?

    CAPTURED_STDOUT="$(cat "$STDOUT_FILE")"
    CAPTURED_STDERR="$(cat "$STDERR_FILE")"

    rm -f "$STDOUT_FILE" "$STDERR_FILE"
}
```

#### Custom Assertions

```bash
# tests/helpers/assertions.bash

# Assert output went to stderr only (for human-readable output)
assert_stderr_only() {
    assert [ -z "$CAPTURED_STDOUT" ]
    assert [ -n "$CAPTURED_STDERR" ]
}

# Assert output went to stdout only (for robot mode)
assert_stdout_only() {
    assert [ -n "$CAPTURED_STDOUT" ]
    assert [ -z "$CAPTURED_STDERR" ]
}

# Assert valid JSON in stdout
assert_valid_json() {
    echo "$CAPTURED_STDOUT" | jq . > /dev/null 2>&1
    assert_success
}

# Assert JSON field value
assert_json_value() {
    local field="$1"
    local expected="$2"
    local actual
    actual=$(echo "$CAPTURED_STDOUT" | jq -r "$field")
    assert_equal "$actual" "$expected"
}

# Assert no ANSI escape codes
assert_no_ansi() {
    refute_output --partial $'\033['
}
```

#### Unit Test Example

```bash
# tests/unit/test_utils.bats

setup() {
    load '../helpers/test_helper'
    load '../helpers/assertions'
    setup_test_environment

    # Source the functions we're testing
    source "$(dirname "$BATS_TEST_DIRNAME")/../lib/utils.sh"
}

teardown() {
    teardown_test_environment
}

@test "version_gt: 1.2.0 > 1.1.0" {
    run version_gt "1.2.0" "1.1.0"
    assert_success
}

@test "version_gt: 1.1.0 > 1.2.0 fails" {
    run version_gt "1.1.0" "1.2.0"
    assert_failure
}

@test "iso_timestamp: returns valid format" {
    run iso_timestamp
    assert_success
    assert_output --regexp '^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$'
}

@test "check_gum: respects APR_NO_GUM" {
    export APR_NO_GUM=1
    run check_gum
    assert_failure
}
```

### 34.9 Real Clipboard Testing

**Source**: CASS mining of jeffreysprompts.com copy command tests

```typescript
describe("Real Clipboard Tests", () => {
  // Detect if clipboard is available
  const isCI = process.env.CI === "true";
  const platform = process.platform;

  function hasClipboardTool(): boolean {
    if (platform === "darwin") {
      return spawnSync("which", ["pbcopy"]).status === 0;
    } else if (platform === "win32") {
      return true; // clip.exe always available
    } else {
      // Linux: check for wl-paste or xclip
      const hasWlPaste = spawnSync("which", ["wl-paste"]).status === 0;
      const hasXclip = spawnSync("which", ["xclip"]).status === 0;
      const hasDisplay = !!(process.env.WAYLAND_DISPLAY || process.env.DISPLAY);
      return (hasWlPaste || hasXclip) && hasDisplay;
    }
  }

  async function readClipboard(): Promise<string | null> {
    return new Promise((resolve) => {
      let cmd: string;
      let args: string[] = [];

      if (platform === "darwin") {
        cmd = "pbpaste";
      } else if (platform === "win32") {
        cmd = "powershell";
        args = ["-command", "Get-Clipboard"];
      } else {
        const hasWlPaste = spawnSync("which", ["wl-paste"]).status === 0;
        if (hasWlPaste) {
          cmd = "wl-paste";
        } else {
          cmd = "xclip";
          args = ["-selection", "clipboard", "-o"];
        }
      }

      const proc = spawn(cmd, args);
      let output = "";

      proc.stdout.on("data", (data) => {
        output += data.toString();
      });

      proc.on("error", () => resolve(null));
      proc.on("close", (code) => {
        resolve(code === 0 ? output : null);
      });
    });
  }

  const shouldSkip = isCI || !hasClipboardTool();
  const itOrSkip = shouldSkip ? it.skip : it;

  if (shouldSkip) {
    it("skipped - no clipboard available", () => {
      console.log("Clipboard tests skipped:", {
        isCI,
        hasClipboardTool: hasClipboardTool(),
        platform,
        DISPLAY: process.env.DISPLAY,
      });
      expect(true).toBe(true);
    });
  }

  itOrSkip("copies content and verifies round-trip", async () => {
    await copyCommand("test-content", { json: true });

    const clipboardContent = await readClipboard();
    expect(clipboardContent).not.toBeNull();
    expect(clipboardContent).toContain("expected content");
  });
});
```

### 34.10 Test Harness Pattern

**Source**: CASS mining of Go testutil.Harness pattern

```go
// internal/testutil/harness.go

// Harness provides structured test execution with logging
type Harness struct {
    t       *testing.T
    tempDir string
    steps   []string
    cleanup []func()
}

func NewHarness(t *testing.T) *Harness {
    t.Helper()
    tempDir := t.TempDir()

    return &Harness{
        t:       t,
        tempDir: tempDir,
        steps:   []string{},
        cleanup: []func(){},
    }
}

// SetStep logs the current test step for debugging failures
func (h *Harness) SetStep(step string) {
    h.t.Helper()
    h.steps = append(h.steps, step)
    h.t.Logf("STEP: %s", step)
}

// CreateFile creates a test file in the temp directory
func (h *Harness) CreateFile(name, content string) string {
    h.t.Helper()
    h.SetStep(fmt.Sprintf("creating file: %s", name))

    path := filepath.Join(h.tempDir, name)
    dir := filepath.Dir(path)

    if err := os.MkdirAll(dir, 0755); err != nil {
        h.t.Fatalf("failed to create directory: %v", err)
    }

    if err := os.WriteFile(path, []byte(content), 0644); err != nil {
        h.t.Fatalf("failed to write file: %v", err)
    }

    return path
}

// OnCleanup registers a cleanup function
func (h *Harness) OnCleanup(fn func()) {
    h.cleanup = append(h.cleanup, fn)
}

// Cleanup runs all registered cleanup functions
func (h *Harness) Cleanup() {
    for i := len(h.cleanup) - 1; i >= 0; i-- {
        h.cleanup[i]()
    }
}

// Usage in tests
func TestComplexWorkflow(t *testing.T) {
    h := testutil.NewHarness(t)
    defer h.Cleanup()

    h.SetStep("setting up config")
    configPath := h.CreateFile("config.yaml", "key: value")

    h.SetStep("loading config")
    cfg, err := LoadConfig(configPath)
    if err != nil {
        t.Fatalf("failed at step %v: %v", h.steps, err)
    }

    h.SetStep("processing")
    result := Process(cfg)

    h.SetStep("verifying output")
    if result.Status != "ok" {
        t.Errorf("unexpected status: %s", result.Status)
    }
}
```

### 34.11 CI Integration Patterns

#### JUnit XML Output for CI

```bash
#!/bin/bash
# tests/ci_runner.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

# Preflight checks
preflight_check() {
    echo "Running preflight checks..."

    # Check BATS is available
    if [[ ! -x "./libs/bats-core/bin/bats" ]]; then
        echo "ERROR: BATS not found. Run 'git submodule update --init'"
        exit 1
    fi

    # Check helper files exist
    for helper in helpers/test_helper.bash helpers/assertions.bash; do
        if [[ ! -f "$helper" ]]; then
            echo "ERROR: Missing $helper"
            exit 1
        fi
    done

    echo "Preflight checks passed"
}

# Run tests with JUnit output
run_tests() {
    local junit_output="test-results.xml"
    local tap_output="test-results.tap"

    echo "Running tests..."
    ./libs/bats-core/bin/bats \
        --formatter junit \
        --output "$junit_output" \
        --tap \
        unit/*.bats integration/*.bats \
        | tee "$tap_output"

    local exit_code=${PIPESTATUS[0]}

    echo ""
    echo "Results saved to: $junit_output, $tap_output"

    return $exit_code
}

preflight_check
run_tests
```

#### GitHub Actions Integration

```yaml
# .github/workflows/test.yml
name: Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
        with:
          submodules: recursive

      - name: Setup Bun
        uses: oven-sh/setup-bun@v1

      - name: Install dependencies
        run: bun install

      - name: Run unit tests
        run: bun run test

      - name: Run E2E tests
        run: bun run test:e2e

      - name: Upload test results
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: test-results
          path: |
            test-results.xml
            playwright-report/
```

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

```yaml
# .github/workflows/ci.yml - Comprehensive CI Pipeline
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]
  workflow_dispatch:  # Manual trigger

jobs:
  # Job 1: Static analysis of shell scripts
  shellcheck:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      # NOTE: Pin third-party actions to a SHA or release tag, not @master.
      # @master is a supply chain risk - the action could be compromised.
      # Use: ludeeus/action-shellcheck@2.0.0 or pin to specific commit SHA
      - uses: ludeeus/action-shellcheck@2.0.0
        with:
          severity: warning
          scandir: '.'
          additional_files: 'my-script install.sh'

  # Job 2: Syntax validation
  syntax:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Check bash syntax
        run: |
          bash -n my-script
          bash -n install.sh
          for script in scripts/*.sh; do
            bash -n "$script"
          done

  # Job 3: Test suite with matrix
  tests:
    needs: [shellcheck, syntax]
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest]
      fail-fast: false
    steps:
      - uses: actions/checkout@v4

      - name: Install bash 5 on macOS
        if: matrix.os == 'macos-latest'
        run: |
          brew install bash
          echo "$(brew --prefix)/bin" >> $GITHUB_PATH

      - name: Show bash version
        run: bash --version

      - name: Run tests (TAP format)
        run: |
          mkdir -p test-results
          ./scripts/run_all_tests.sh --tap 2>&1 | tee test-results/tests.tap
        continue-on-error: true

      - name: Run tests (human format)
        if: failure()
        run: ./scripts/run_all_tests.sh 2>&1 | tee test-results/tests.log

      - name: Upload test artifacts
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: test-results-${{ matrix.os }}
          path: test-results/
          retention-days: 14

  # Job 4: Installation test
  install-test:
    needs: [shellcheck, syntax]
    runs-on: ${{ matrix.os }}
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest]
    steps:
      - uses: actions/checkout@v4

      - name: Install bash 5 on macOS
        if: matrix.os == 'macos-latest'
        run: |
          brew install bash
          echo "$(brew --prefix)/bin" >> $GITHUB_PATH

      - name: Run installer
        run: UNSAFE_MAIN=1 DEST=/tmp/test-install ./install.sh

      - name: Verify installation
        run: |
          test -x /tmp/test-install/my-tool
          /tmp/test-install/my-tool --version
          /tmp/test-install/my-tool --help | head -20

  # Job 5: Version consistency
  version-check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Check VERSION file exists
        run: |
          if [[ ! -f VERSION ]]; then
            echo "::warning::VERSION file not found"
          fi

      - name: Verify version consistency
        run: |
          file_version=$(cat VERSION)
          script_version=$(grep -m1 'VERSION=' my-script | cut -d'"' -f2)

          if [[ "$file_version" != "$script_version" ]]; then
            echo "::error::Version mismatch: VERSION=$file_version, script=$script_version"
            exit 1
          fi

          echo "Version verified: $file_version"
```

### 35.2 Job Dependencies and Ordering

#### Dependency Graph Patterns

```
shellcheck ─┐
            ├─→ tests (matrix: ubuntu, macos)
syntax ─────┤
            └─→ install-test (matrix: ubuntu, macos)

version-check ──→ (independent, runs in parallel)
```

```yaml
# Jobs with dependencies
jobs:
  lint:
    runs-on: ubuntu-latest
    # ... lint steps

  test:
    needs: [lint]  # Waits for lint to complete
    runs-on: ubuntu-latest
    # ... test steps

  build:
    needs: [lint, test]  # Waits for both
    runs-on: ubuntu-latest
    # ... build steps

  deploy:
    needs: [build]
    if: github.ref == 'refs/heads/main'  # Only on main
    runs-on: ubuntu-latest
    # ... deploy steps
```

#### Conditional Execution

```yaml
jobs:
  deploy:
    runs-on: ubuntu-latest
    # Only deploy from main branch
    if: github.ref == 'refs/heads/main'
    steps:
      - name: Deploy to production
        if: success()
        run: ./deploy.sh

      - name: Notify on failure
        if: failure()
        run: ./notify-failure.sh

      - name: Cleanup
        if: always()  # Runs regardless of job status
        run: ./cleanup.sh
```

### 35.3 Release Automation

#### Tag-Triggered Releases

**Source**: CASS mining of repo_updater release workflow

```yaml
# .github/workflows/release.yml
name: Release

on:
  push:
    tags:
      - 'v*'  # Triggers on v1.0.0, v2.1.0-beta, etc.

permissions:
  contents: write  # Allows creating releases

jobs:
  release:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Extract version from tag
        id: version
        run: |
          VERSION=${GITHUB_REF#refs/tags/v}
          echo "version=$VERSION" >> $GITHUB_OUTPUT

      - name: Verify script version matches tag
        run: |
          script_version=$(grep -m1 'VERSION=' my-script | cut -d'"' -f2)
          if [[ "$script_version" != "${{ steps.version.outputs.version }}" ]]; then
            echo "::error::Tag ${{ steps.version.outputs.version }} doesn't match script version $script_version"
            exit 1
          fi
          echo "Version verified: $script_version"

      - name: Verify VERSION file matches tag
        run: |
          if [[ -f VERSION ]]; then
            file_version=$(cat VERSION)
            if [[ "$file_version" != "${{ steps.version.outputs.version }}" ]]; then
              echo "::error::Tag doesn't match VERSION file"
              exit 1
            fi
          fi

      - name: Compute SHA256 checksums
        run: |
          sha256sum my-script install.sh > checksums.txt
          sha256sum my-script | awk '{print $1}' > my-script.sha256
          sha256sum install.sh | awk '{print $1}' > install.sh.sha256
          echo "Generated checksums:"
          cat checksums.txt

      - name: Generate release notes
        run: |
          cat > release_notes.md << 'EOF'
          ## Installation

          ```bash
          curl -fsSL https://example.com/install.sh | bash
          ```

          ## Verification

          ```bash
          echo "$(curl -fsSL https://example.com/my-script.sha256)  my-script" | sha256sum -c -
          ```

          ## Checksums

          See `checksums.txt` for SHA256 hashes.
          EOF

      - name: Create GitHub Release
        uses: softprops/action-gh-release@v1
        with:
          name: my-tool v${{ steps.version.outputs.version }}
          body_path: release_notes.md
          files: |
            my-script
            install.sh
            checksums.txt
            my-script.sha256
            install.sh.sha256
          generate_release_notes: true
          append_body: true
```

### 35.4 Version Management Patterns

#### Dual Version Storage

**Source**: CASS mining of repo_updater version management

```bash
# VERSION file (single source of truth)
1.2.1

# Script variable (for --version flag)
VERSION="1.2.1"

# Version reading with fallback
get_version() {
    local script_dir
    script_dir="$(dirname "${BASH_SOURCE[0]}")"

    if [[ -f "$script_dir/VERSION" ]]; then
        cat "$script_dir/VERSION"
    else
        echo "$VERSION"
    fi
}
```

#### Semantic Version Comparison

```bash
# Compare semantic versions
# Returns 0 if v1 > v2, 1 otherwise
version_gt() {
    local v1="$1" v2="$2"

    # Split into components
    IFS='.' read -ra V1 <<< "$v1"
    IFS='.' read -ra V2 <<< "$v2"

    # Compare each component
    for i in 0 1 2; do
        local n1=${V1[$i]:-0}
        local n2=${V2[$i]:-0}

        if (( n1 > n2 )); then
            return 0
        elif (( n1 < n2 )); then
            return 1
        fi
    done

    return 1  # Equal
}

# Usage in self-update
check_for_update() {
    local current_version="$VERSION"
    local latest_version
    latest_version=$(curl -fsSL "$RELEASE_URL/latest/VERSION" 2>/dev/null)

    if version_gt "$latest_version" "$current_version"; then
        echo "Update available: $current_version → $latest_version"
        return 0
    fi
    return 1
}
```

### 35.5 Matrix Testing Strategies

#### Multi-OS Matrix

```yaml
jobs:
  test:
    strategy:
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
        node-version: [18, 20, 22]
      fail-fast: false  # Continue other matrix jobs if one fails
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
        with:
          node-version: ${{ matrix.node-version }}
      - run: npm test
```

#### Browser Matrix for E2E

**Source**: CASS mining of jeffreysprompts_premium E2E workflow

```yaml
jobs:
  e2e:
    strategy:
      matrix:
        browser: [chromium, webkit, firefox]
        include:
          - browser: chromium
            project: Desktop Chrome
          - browser: webkit
            project: Desktop Safari
          - browser: firefox
            project: Desktop Firefox
      fail-fast: false
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-node@v4
      - run: npm ci
      - run: npx playwright install --with-deps ${{ matrix.browser }}
      - run: npx playwright test --project="${{ matrix.project }}"
      - uses: actions/upload-artifact@v4
        if: failure()
        with:
          name: playwright-report-${{ matrix.browser }}
          path: playwright-report/
```

### 35.6 Container Image Pipelines

#### Multi-Stage Dockerfile with CI

**Source**: CASS mining of flywheel_gateway tenant container pipeline

```dockerfile
# Stage 1: Build application
FROM docker.io/library/golang:1.23-bookworm AS builder
WORKDIR /build
COPY go.mod go.sum ./
RUN go mod download
COPY . .
RUN CGO_ENABLED=0 go build -ldflags="-s -w" -o /app ./cmd/server

# Stage 2: Runtime image
FROM docker.io/library/alpine:3.20
RUN apk add --no-cache ca-certificates tzdata
COPY --from=builder /app /usr/local/bin/app
USER nobody:nobody
ENTRYPOINT ["/usr/local/bin/app"]
```

```yaml
# .github/workflows/build-image.yml
name: Build Container Image

on:
  push:
    tags: ['v*']
  pull_request:
    paths:
      - 'Dockerfile'
      - 'go.mod'
      - 'go.sum'
      - 'cmd/**'
      - 'internal/**'

env:
  REGISTRY: ghcr.io
  IMAGE_NAME: ${{ github.repository }}

jobs:
  build:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      packages: write
    steps:
      - uses: actions/checkout@v4

      - name: Set up QEMU (for multi-arch)
        uses: docker/setup-qemu-action@v3

      - name: Set up Docker Buildx
        uses: docker/setup-buildx-action@v3

      - name: Login to Container Registry
        if: github.event_name != 'pull_request'
        uses: docker/login-action@v3
        with:
          registry: ${{ env.REGISTRY }}
          username: ${{ github.actor }}
          password: ${{ secrets.GITHUB_TOKEN }}

      - name: Extract metadata
        id: meta
        uses: docker/metadata-action@v5
        with:
          images: ${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}
          tags: |
            type=semver,pattern={{version}}
            type=semver,pattern={{major}}.{{minor}}
            type=sha

      - name: Build and push
        uses: docker/build-push-action@v5
        with:
          context: .
          platforms: linux/amd64,linux/arm64
          push: ${{ github.event_name != 'pull_request' }}
          tags: ${{ steps.meta.outputs.tags }}
          labels: ${{ steps.meta.outputs.labels }}
          cache-from: type=gha
          cache-to: type=gha,mode=max

      - name: Run Trivy vulnerability scan
        uses: aquasecurity/trivy-action@master
        with:
          image-ref: ${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}:${{ steps.meta.outputs.version }}
          format: 'sarif'
          output: 'trivy-results.sarif'
          severity: 'CRITICAL,HIGH'

      - name: Upload Trivy scan results
        uses: github/codeql-action/upload-sarif@v2
        with:
          sarif_file: 'trivy-results.sarif'

      - name: Generate SBOM
        uses: anchore/sbom-action@v0
        with:
          image: ${{ env.REGISTRY }}/${{ env.IMAGE_NAME }}:${{ steps.meta.outputs.version }}
          format: spdx-json
          output-file: sbom.spdx.json

      - name: Upload SBOM
        uses: actions/upload-artifact@v4
        with:
          name: sbom
          path: sbom.spdx.json
```

### 35.7 Artifact Management

#### Upload and Download Patterns

```yaml
jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - run: npm run build

      - name: Upload build artifacts
        uses: actions/upload-artifact@v4
        with:
          name: dist
          path: dist/
          retention-days: 7
          if-no-files-found: error

  test:
    needs: build
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Download build artifacts
        uses: actions/download-artifact@v4
        with:
          name: dist
          path: dist/

      - run: npm run test:e2e

  deploy:
    needs: [build, test]
    runs-on: ubuntu-latest
    steps:
      - name: Download artifacts
        uses: actions/download-artifact@v4
        with:
          name: dist

      - name: Deploy to production
        run: ./deploy.sh dist/
```

#### Caching Dependencies

```yaml
jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      # Node.js with cache
      - uses: actions/setup-node@v4
        with:
          node-version: 20
          cache: 'npm'

      # Or Bun with cache
      - uses: oven-sh/setup-bun@v1
        with:
          bun-version: latest

      - name: Cache Bun dependencies
        uses: actions/cache@v4
        with:
          path: ~/.bun/install/cache
          key: ${{ runner.os }}-bun-${{ hashFiles('**/bun.lockb') }}
          restore-keys: |
            ${{ runner.os }}-bun-

      # Rust with cache
      - name: Cache Cargo
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/bin/
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            ~/.cargo/git/db/
            target/
          key: ${{ runner.os }}-cargo-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-
```

### 35.8 Automated Dependency Updates

#### Dependabot Configuration

```yaml
# .github/dependabot.yml
version: 2
updates:
  # npm dependencies
  - package-ecosystem: "npm"
    directory: "/"
    schedule:
      interval: "weekly"
      day: "monday"
    open-pull-requests-limit: 10
    groups:
      typescript:
        patterns:
          - "typescript"
          - "@types/*"
      testing:
        patterns:
          - "vitest"
          - "@vitest/*"
          - "playwright"
    ignore:
      - dependency-name: "*"
        update-types: ["version-update:semver-major"]

  # GitHub Actions
  - package-ecosystem: "github-actions"
    directory: "/"
    schedule:
      interval: "weekly"
    groups:
      actions:
        patterns:
          - "*"

  # Docker
  - package-ecosystem: "docker"
    directory: "/"
    schedule:
      interval: "weekly"

  # Cargo (Rust)
  - package-ecosystem: "cargo"
    directory: "/"
    schedule:
      interval: "weekly"
```

### 35.9 Pre-Commit Hook Integration

#### Installing Pre-Commit Hooks

**Source**: CASS mining of destructive_command_guard hook patterns

```yaml
# .github/workflows/ci.yml
jobs:
  lint:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install pre-commit
        run: pip install pre-commit

      - name: Run pre-commit hooks
        run: pre-commit run --all-files
```

```yaml
# .pre-commit-config.yaml
repos:
  - repo: https://github.com/pre-commit/pre-commit-hooks
    rev: v4.5.0
    hooks:
      - id: trailing-whitespace
      - id: end-of-file-fixer
      - id: check-yaml
      - id: check-added-large-files
      - id: check-merge-conflict

  - repo: https://github.com/shellcheck-py/shellcheck-py
    rev: v0.9.0.6
    hooks:
      - id: shellcheck

  - repo: local
    hooks:
      - id: dcg-check
        name: Destructive Command Guard
        entry: dcg check
        language: system
        types: [shell]
```

### 35.10 Deployment Workflows

#### Vercel Deployment

**Source**: CASS mining of jeffreysprompts_premium deploy workflow

```yaml
# .github/workflows/deploy.yml
name: Deploy

on:
  push:
    tags:
      - 'v*'

jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup Node
        uses: actions/setup-node@v4
        with:
          node-version: 20

      - name: Install dependencies
        run: npm ci

      - name: Run tests
        run: npm test

      - name: Build
        run: npm run build

      - name: Deploy to Vercel
        uses: amondnet/vercel-action@v25
        with:
          vercel-token: ${{ secrets.VERCEL_TOKEN }}
          vercel-org-id: ${{ secrets.VERCEL_ORG_ID }}
          vercel-project-id: ${{ secrets.VERCEL_PROJECT_ID }}
          vercel-args: '--prod'

      - name: Run database migrations
        run: npm run db:migrate
        env:
          DATABASE_URL: ${{ secrets.DATABASE_URL }}

      - name: Smoke test
        run: |
          sleep 30  # Wait for deployment
          curl -f https://my-app.vercel.app/api/health || exit 1
```

### 35.11 Quality Gates

#### Comprehensive Quality Pipeline

```yaml
jobs:
  quality:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Setup
        uses: oven-sh/setup-bun@v1

      - name: Install
        run: bun install

      # Lint check
      - name: Lint
        run: bun run lint

      # Type check
      - name: Type check
        run: bun run typecheck

      # Format check
      - name: Format check
        run: bun run format:check

      # Unit tests
      - name: Unit tests
        run: bun run test

      # Build verification
      - name: Build
        run: bun run build

      # Bundle size check
      - name: Analyze bundle
        run: |
          bun run build
          du -sh dist/
          # Fail if bundle exceeds threshold
          MAX_SIZE=$((1024 * 1024))  # 1MB
          ACTUAL_SIZE=$(du -sb dist/ | cut -f1)
          if [[ $ACTUAL_SIZE -gt $MAX_SIZE ]]; then
            echo "::error::Bundle size $ACTUAL_SIZE exceeds limit $MAX_SIZE"
            exit 1
          fi
```

### 35.12 Self-Update Mechanisms

#### CLI Self-Update Pattern

**Source**: CASS mining of apr self-update implementation

```bash
#!/bin/bash
# Self-update with checksum verification

RELEASE_URL="https://github.com/owner/repo/releases/latest/download"

update_self() {
    local current_version="$VERSION"
    local temp_dir
    temp_dir=$(mktemp -d)

    echo "Checking for updates..."

    # Download new version
    if ! curl -fsSL "$RELEASE_URL/my-tool" -o "$temp_dir/my-tool"; then
        echo "Failed to download update"
        rm -rf "$temp_dir"
        return 1
    fi

    # Download checksum
    if ! curl -fsSL "$RELEASE_URL/my-tool.sha256" -o "$temp_dir/my-tool.sha256"; then
        echo "Failed to download checksum"
        rm -rf "$temp_dir"
        return 1
    fi

    # Verify checksum
    local expected_hash
    expected_hash=$(cat "$temp_dir/my-tool.sha256")
    local actual_hash
    actual_hash=$(sha256sum "$temp_dir/my-tool" | awk '{print $1}')

    if [[ "$expected_hash" != "$actual_hash" ]]; then
        echo "Checksum verification failed!"
        echo "Expected: $expected_hash"
        echo "Got:      $actual_hash"
        rm -rf "$temp_dir"
        return 1
    fi

    # Verify script syntax
    if ! bash -n "$temp_dir/my-tool"; then
        echo "Downloaded script has syntax errors"
        rm -rf "$temp_dir"
        return 1
    fi

    # Install update
    local install_path
    install_path=$(which my-tool)
    chmod +x "$temp_dir/my-tool"

    if ! mv "$temp_dir/my-tool" "$install_path"; then
        echo "Failed to install update (try with sudo)"
        rm -rf "$temp_dir"
        return 1
    fi

    rm -rf "$temp_dir"

    local new_version
    new_version=$("$install_path" --version)
    echo "Updated: $current_version → $new_version"
}
```

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

```
❌ Caching non-deterministic results (random, timestamps)
❌ Caching without invalidation strategy
❌ Unbounded cache growth (no eviction)
❌ Caching across security boundaries without validation
❌ Over-caching (cache overhead > computation cost)
```

### 36.2 Lazy Initialization Patterns

#### Rust: OnceLock for Static Lazy Values

**Source**: CASS mining of xf VectorIndex cache

```rust
use std::sync::OnceLock;

/// Lazy-loaded vector index (only loaded if semantic search used)
static VECTOR_INDEX: OnceLock<VectorIndex> = OnceLock::new();

pub fn get_vector_index() -> &'static VectorIndex {
    VECTOR_INDEX.get_or_init(|| {
        // Expensive initialization only happens once
        VectorIndex::load_from_disk()
            .expect("failed to load vector index")
    })
}

// Alternative: Fallible initialization with once_cell::sync::Lazy
use once_cell::sync::Lazy;

static SIMD_DOT_ENABLED: Lazy<bool> = Lazy::new(|| {
    std::env::var("CASS_SIMD_DOT")
        .map(|v| v != "0")
        .unwrap_or(true)
});
```

**When to use**:
- Configuration that's expensive to compute
- Indices loaded on first access
- Runtime feature flags

#### Go: sync.Once for Thread-Safe Initialization

```go
import "sync"

var (
    globalCache *Cache
    cacheOnce   sync.Once
)

// GetCache returns the singleton cache instance
func GetCache() *Cache {
    cacheOnce.Do(func() {
        globalCache = &Cache{
            entries:   make(map[string]*CacheEntry),
            maxSize:   1000,
            ttl:       5 * time.Minute,
        }
    })
    return globalCache
}

// Alternative: Using sync.OnceValue (Go 1.21+)
var getConfig = sync.OnceValue(func() *Config {
    cfg, err := loadConfig()
    if err != nil {
        panic(fmt.Sprintf("config load failed: %v", err))
    }
    return cfg
})
```

#### TypeScript: Lazy Accessor Pattern

```typescript
class LazyConfig {
    private _parsed: ParsedConfig | null = null;
    private _raw: string;
    
    constructor(rawConfig: string) {
        this._raw = rawConfig;
    }
    
    get parsed(): ParsedConfig {
        if (this._parsed === null) {
            this._parsed = JSON.parse(this._raw);
        }
        return this._parsed;
    }
}

// Class-based lazy with getter
class SkillLoader {
    private _skills: Map<string, Skill> | null = null;
    
    get skills(): Map<string, Skill> {
        return this._skills ??= this.loadAllSkills();
    }
    
    private loadAllSkills(): Map<string, Skill> {
        // Expensive loading logic
    }
}
```

### 36.3 TriageContext Pattern: Unified Lazy Caching

**Source**: CASS mining of beads_viewer TriageContext implementation

This pattern provides a context object that lazily computes and caches multiple related values, avoiding redundant computation in complex workflows.

#### Go Implementation

```go
package analysis

import "sync"

// TriageContext provides unified caching for triage-related computations.
// Values are computed on first access and cached for the lifetime of the context.
type TriageContext struct {
    analyzer           *Analyzer
    
    // Cached values
    actionable         []Issue
    actionableSet      map[string]bool
    actionableComputed bool
    
    blockerDepth       map[string]int
    openBlockers       map[string][]string
    
    unblocksMap        map[string][]string
    unblocksComputed   bool
    
    // Thread safety (nil for single-threaded use)
    mu *sync.Mutex
}

// NewTriageContext creates a single-threaded context
func NewTriageContext(analyzer *Analyzer) *TriageContext {
    return &TriageContext{
        analyzer:     analyzer,
        blockerDepth: make(map[string]int),
        openBlockers: make(map[string][]string),
    }
}

// NewTriageContextThreadSafe creates a thread-safe context
func NewTriageContextThreadSafe(analyzer *Analyzer) *TriageContext {
    ctx := NewTriageContext(analyzer)
    ctx.mu = &sync.Mutex{}
    return ctx
}

func (ctx *TriageContext) lock() {
    if ctx.mu != nil {
        ctx.mu.Lock()
    }
}

func (ctx *TriageContext) unlock() {
    if ctx.mu != nil {
        ctx.mu.Unlock()
    }
}

// ActionableIssues returns cached actionable issues, computing on first call
func (ctx *TriageContext) ActionableIssues() []Issue {
    ctx.lock()
    defer ctx.unlock()
    
    if ctx.actionableComputed {
        return ctx.actionable
    }
    
    // Compute once
    ctx.actionable = ctx.analyzer.GetActionableIssues()
    
    // Build lookup set for O(1) IsActionable checks
    ctx.actionableSet = make(map[string]bool, len(ctx.actionable))
    for _, issue := range ctx.actionable {
        ctx.actionableSet[issue.ID] = true
    }
    
    ctx.actionableComputed = true
    return ctx.actionable
}

// IsActionable is O(1) after first ActionableIssues() call
func (ctx *TriageContext) IsActionable(id string) bool {
    ctx.lock()
    defer ctx.unlock()
    
    if !ctx.actionableComputed {
        // Force computation
        ctx.unlock()
        ctx.ActionableIssues()
        ctx.lock()
    }
    
    return ctx.actionableSet[id]
}

// BlockerDepth computes depth with cycle detection, cached per-issue
func (ctx *TriageContext) BlockerDepth(id string) int {
    ctx.lock()
    defer ctx.unlock()
    
    if depth, ok := ctx.blockerDepth[id]; ok {
        return depth
    }
    
    // Use internal method to avoid nested lock
    depth := ctx.computeBlockerDepthInternal(id, make(map[string]bool))
    ctx.blockerDepth[id] = depth
    return depth
}

func (ctx *TriageContext) computeBlockerDepthInternal(id string, visiting map[string]bool) int {
    if visiting[id] {
        return 0 // Cycle detected
    }
    visiting[id] = true
    defer delete(visiting, id)
    
    blockers := ctx.getOpenBlockersInternal(id)
    if len(blockers) == 0 {
        return 0
    }
    
    maxDepth := 0
    for _, blockerId := range blockers {
        depth := ctx.computeBlockerDepthInternal(blockerId, visiting)
        if depth+1 > maxDepth {
            maxDepth = depth + 1
        }
    }
    return maxDepth
}

// Reset clears all cached values for reuse with new data
func (ctx *TriageContext) Reset() {
    ctx.lock()
    defer ctx.unlock()
    
    ctx.actionable = nil
    ctx.actionableSet = nil
    ctx.actionableComputed = false
    ctx.blockerDepth = make(map[string]int)
    ctx.openBlockers = make(map[string][]string)
    ctx.unblocksMap = nil
    ctx.unblocksComputed = false
}
```

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

```go
package topk

import (
    "container/heap"
    "sort"
)

// Scored pairs an item with its score
type Scored[T any] struct {
    Item  T
    Score float64
}

// Collector collects the top-K highest-scoring items
type Collector[T any] struct {
    k    int
    h    *minHeap[T]
    less func(a, b T) bool // For deterministic ordering of equal scores
}

// minHeap implements heap.Interface for Scored items
type minHeap[T any] struct {
    items []Scored[T]
    less  func(a, b T) bool
}

func (h *minHeap[T]) Len() int           { return len(h.items) }
func (h *minHeap[T]) Less(i, j int) bool { return h.items[i].Score < h.items[j].Score }
func (h *minHeap[T]) Swap(i, j int)      { h.items[i], h.items[j] = h.items[j], h.items[i] }
func (h *minHeap[T]) Push(x any)         { h.items = append(h.items, x.(Scored[T])) }
func (h *minHeap[T]) Pop() any {
    n := len(h.items)
    x := h.items[n-1]
    h.items = h.items[:n-1]
    return x
}

// New creates a Collector for the top k items
func New[T any](k int, less func(a, b T) bool) *Collector[T] {
    if k < 0 {
        k = 0
    }
    h := &minHeap[T]{
        items: make([]Scored[T], 0, k),
        less:  less,
    }
    heap.Init(h)
    return &Collector[T]{k: k, h: h, less: less}
}

// Add considers an item for inclusion in the top-K
// Returns true if the item was added to the collection
func (c *Collector[T]) Add(item T, score float64) bool {
    if c.k <= 0 {
        return false
    }
    
    entry := Scored[T]{Item: item, Score: score}
    
    // If heap not full, always add
    if c.h.Len() < c.k {
        heap.Push(c.h, entry)
        return true
    }
    
    // If score beats minimum, replace
    if score > c.h.items[0].Score {
        heap.Pop(c.h)
        heap.Push(c.h, entry)
        return true
    }
    
    // Tie-breaking for equal scores (deterministic ordering)
    if score == c.h.items[0].Score && c.less != nil {
        if c.less(item, c.h.items[0].Item) {
            heap.Pop(c.h)
            heap.Push(c.h, entry)
            return true
        }
    }
    
    return false
}

// Results returns items sorted by score descending
func (c *Collector[T]) Results() []T {
    results := make([]T, c.h.Len())
    scores := c.ResultsWithScores()
    for i, s := range scores {
        results[i] = s.Item
    }
    return results
}

// ResultsWithScores returns items with their scores, sorted descending
func (c *Collector[T]) ResultsWithScores() []Scored[T] {
    results := make([]Scored[T], c.h.Len())
    copy(results, c.h.items)
    
    sort.Slice(results, func(i, j int) bool {
        if results[i].Score != results[j].Score {
            return results[i].Score > results[j].Score
        }
        // Deterministic tie-breaking
        if c.less != nil {
            return c.less(results[i].Item, results[j].Item)
        }
        return false
    })
    
    return results
}

// Reset allows collector reuse without reallocation
func (c *Collector[T]) Reset() {
    c.h.items = c.h.items[:0]
}
```

#### Rust BinaryHeap Implementation

```rust
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// Top-K collector using min-heap (via Reverse)
pub struct TopKCollector<T> {
    k: usize,
    heap: BinaryHeap<Reverse<ScoredEntry<T>>>,
}

#[derive(Clone, PartialEq)]
pub struct ScoredEntry<T> {
    pub score: f32,
    pub item: T,
}

impl<T: Ord> Eq for ScoredEntry<T> {}

impl<T: Ord> PartialOrd for ScoredEntry<T> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<T: Ord> Ord for ScoredEntry<T> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Primary: score (ascending for min-heap via Reverse)
        // Secondary: item for deterministic tie-breaking
        self.score
            .partial_cmp(&other.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| self.item.cmp(&other.item))
    }
}

impl<T: Ord + Clone> TopKCollector<T> {
    pub fn new(k: usize) -> Self {
        Self {
            k,
            heap: BinaryHeap::with_capacity(k + 1),
        }
    }
    
    pub fn add(&mut self, item: T, score: f32) -> bool {
        if self.k == 0 {
            return false;
        }
        
        let entry = ScoredEntry { score, item };
        
        if self.heap.len() < self.k {
            self.heap.push(Reverse(entry));
            return true;
        }
        
        // Check if beats minimum
        if let Some(Reverse(min)) = self.heap.peek() {
            if score > min.score {
                self.heap.pop();
                self.heap.push(Reverse(entry));
                return true;
            }
        }
        
        false
    }
    
    pub fn results(self) -> Vec<T> {
        let mut results: Vec<_> = self.heap
            .into_iter()
            .map(|Reverse(e)| e)
            .collect();
        
        // Sort descending by score
        results.sort_by(|a, b| {
            b.score.partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.item.cmp(&b.item))
        });
        
        results.into_iter().map(|e| e.item).collect()
    }
}
```

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

```go
package cache

import (
    "container/list"
    "encoding/json"
    "os"
    "path/filepath"
    "sync"
    "time"
)

// DiskCache implements LRU eviction with disk persistence
type DiskCache struct {
    mu         sync.RWMutex
    entries    map[string]*list.Element
    order      *list.List  // LRU order (front = most recent)
    maxEntries int
    ttl        time.Duration
    path       string
}

type cacheEntry struct {
    Key        string    `json:"key"`
    Value      any       `json:"value"`
    Hash       string    `json:"hash"`       // For staleness detection
    ComputedAt time.Time `json:"computed_at"`
    AccessedAt time.Time `json:"accessed_at"`
}

// NewDiskCache creates a persistent LRU cache
func NewDiskCache(path string, maxEntries int, ttl time.Duration) *DiskCache {
    cache := &DiskCache{
        entries:    make(map[string]*list.Element),
        order:      list.New(),
        maxEntries: maxEntries,
        ttl:        ttl,
        path:       path,
    }
    
    // Load existing entries from disk
    cache.loadFromDisk()
    return cache
}

// Get retrieves a cached value, returning nil if expired or missing
func (c *DiskCache) Get(key string) (any, bool) {
    c.mu.Lock()
    defer c.mu.Unlock()
    
    elem, ok := c.entries[key]
    if !ok {
        return nil, false
    }
    
    entry := elem.Value.(*cacheEntry)
    
    // Check TTL
    if time.Since(entry.ComputedAt) > c.ttl {
        c.removeElement(elem)
        return nil, false
    }
    
    // Move to front (most recently used)
    c.order.MoveToFront(elem)
    entry.AccessedAt = time.Now()
    
    return entry.Value, true
}

// Set stores a value, evicting LRU entries if needed
func (c *DiskCache) Set(key string, value any, hash string) {
    c.mu.Lock()
    defer c.mu.Unlock()
    
    now := time.Now()
    
    // Update existing entry
    if elem, ok := c.entries[key]; ok {
        entry := elem.Value.(*cacheEntry)
        entry.Value = value
        entry.Hash = hash
        entry.ComputedAt = now
        entry.AccessedAt = now
        c.order.MoveToFront(elem)
        c.saveToDisk()
        return
    }
    
    // Create new entry
    entry := &cacheEntry{
        Key:        key,
        Value:      value,
        Hash:       hash,
        ComputedAt: now,
        AccessedAt: now,
    }
    elem := c.order.PushFront(entry)
    c.entries[key] = elem
    
    // Evict LRU entries if over capacity
    for c.order.Len() > c.maxEntries {
        oldest := c.order.Back()
        if oldest != nil {
            c.removeElement(oldest)
        }
    }
    
    c.saveToDisk()
}

// IsStale checks if cached value's hash differs from current
func (c *DiskCache) IsStale(key, currentHash string) bool {
    c.mu.RLock()
    defer c.mu.RUnlock()
    
    elem, ok := c.entries[key]
    if !ok {
        return true
    }
    
    entry := elem.Value.(*cacheEntry)
    return entry.Hash != currentHash
}

func (c *DiskCache) removeElement(elem *list.Element) {
    entry := elem.Value.(*cacheEntry)
    delete(c.entries, entry.Key)
    c.order.Remove(elem)
}

func (c *DiskCache) saveToDisk() {
    entries := make([]*cacheEntry, 0, c.order.Len())
    for elem := c.order.Front(); elem != nil; elem = elem.Next() {
        entries = append(entries, elem.Value.(*cacheEntry))
    }
    
    data, err := json.MarshalIndent(entries, "", "  ")
    if err != nil {
        return
    }
    
    // Atomic write: temp file + rename
    tempPath := c.path + ".tmp"
    if err := os.WriteFile(tempPath, data, 0644); err != nil {
        return
    }
    os.Rename(tempPath, c.path)
}

func (c *DiskCache) loadFromDisk() {
    data, err := os.ReadFile(c.path)
    if err != nil {
        return
    }
    
    var entries []*cacheEntry
    if err := json.Unmarshal(data, &entries); err != nil {
        return
    }
    
    now := time.Now()
    for _, entry := range entries {
        // Skip expired entries
        if now.Sub(entry.ComputedAt) > c.ttl {
            continue
        }
        elem := c.order.PushBack(entry)
        c.entries[entry.Key] = elem
    }
}
```

### 36.6 In-Memory Cache with TTL

**Source**: CASS mining of beads_viewer GlobalCache pattern

```go
package cache

import (
    "sync"
    "time"
)

// GlobalCache is a thread-safe in-memory cache with TTL
type GlobalCache struct {
    mu      sync.RWMutex
    entries map[string]*CacheEntry
    ttl     time.Duration
}

type CacheEntry struct {
    Value      any
    Hash       string
    ComputedAt time.Time
}

var (
    defaultCache     *GlobalCache
    defaultCacheOnce sync.Once
)

// Default returns the global cache instance (5-minute TTL)
func Default() *GlobalCache {
    defaultCacheOnce.Do(func() {
        defaultCache = &GlobalCache{
            entries: make(map[string]*CacheEntry),
            ttl:     5 * time.Minute,
        }
    })
    return defaultCache
}

// Get retrieves a cached value
func (c *GlobalCache) Get(key string) (any, bool) {
    c.mu.RLock()
    defer c.mu.RUnlock()
    
    entry, ok := c.entries[key]
    if !ok {
        return nil, false
    }
    
    // Check expiration
    if time.Since(entry.ComputedAt) > c.ttl {
        // Entry expired (will be cleaned on next Set)
        return nil, false
    }
    
    return entry.Value, true
}

// GetOrCompute returns cached value or computes and caches it
func (c *GlobalCache) GetOrCompute(key, hash string, compute func() any) any {
    c.mu.RLock()
    entry, ok := c.entries[key]
    if ok && entry.Hash == hash && time.Since(entry.ComputedAt) < c.ttl {
        c.mu.RUnlock()
        return entry.Value
    }
    c.mu.RUnlock()
    
    // Compute new value
    value := compute()
    
    c.mu.Lock()
    defer c.mu.Unlock()
    
    c.entries[key] = &CacheEntry{
        Value:      value,
        Hash:       hash,
        ComputedAt: time.Now(),
    }
    
    return value
}

// Invalidate removes a specific key
func (c *GlobalCache) Invalidate(key string) {
    c.mu.Lock()
    defer c.mu.Unlock()
    delete(c.entries, key)
}

// Clear removes all entries
func (c *GlobalCache) Clear() {
    c.mu.Lock()
    defer c.mu.Unlock()
    c.entries = make(map[string]*CacheEntry)
}

// Cleanup removes expired entries
func (c *GlobalCache) Cleanup() int {
    c.mu.Lock()
    defer c.mu.Unlock()
    
    now := time.Now()
    removed := 0
    
    for key, entry := range c.entries {
        if now.Sub(entry.ComputedAt) > c.ttl {
            delete(c.entries, key)
            removed++
        }
    }
    
    return removed
}
```

### 36.7 SIMD-Optimized Dot Product

**Source**: CASS mining of xf and cass vector search implementations

```rust
use wide::f32x8;

/// SIMD-optimized dot product using AVX-256 (8 floats per iteration)
#[inline]
pub fn dot_product_simd(a: &[f32], b: &[f32]) -> f32 {
    assert_eq!(a.len(), b.len());
    
    let chunks_a = a.chunks_exact(8);
    let chunks_b = b.chunks_exact(8);
    let remainder_a = chunks_a.remainder();
    let remainder_b = chunks_b.remainder();
    
    // SIMD accumulator
    let mut sum = f32x8::ZERO;
    
    for (ca, cb) in chunks_a.zip(chunks_b) {
        let arr_a: [f32; 8] = ca.try_into().unwrap();
        let arr_b: [f32; 8] = cb.try_into().unwrap();
        
        // 8 FMA operations per iteration
        sum += f32x8::from(arr_a) * f32x8::from(arr_b);
    }
    
    // Horizontal reduction
    let mut scalar_sum: f32 = sum.reduce_add();
    
    // Handle remainder (non-8-multiple dimensions)
    for (a, b) in remainder_a.iter().zip(remainder_b) {
        scalar_sum += a * b;
    }
    
    scalar_sum
}

/// Feature flag for SIMD enable/disable
static SIMD_ENABLED: once_cell::sync::Lazy<bool> = once_cell::sync::Lazy::new(|| {
    std::env::var("CASS_SIMD_DOT")
        .map(|v| v != "0")
        .unwrap_or(true)
});

/// Dispatch to SIMD or scalar based on feature flag
pub fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    if *SIMD_ENABLED {
        dot_product_simd(a, b)
    } else {
        dot_product_scalar(a, b)
    }
}

#[inline]
fn dot_product_scalar(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}
```

### 36.8 Parallel K-NN Search with Thread-Local Heaps

**Source**: CASS mining of cass vector index parallel search

```rust
use rayon::prelude::*;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

const PARALLEL_THRESHOLD: usize = 10_000;
const PARALLEL_CHUNK_SIZE: usize = 1024;

impl VectorIndex {
    pub fn search_top_k(&self, query: &[f32], k: usize) -> Vec<SearchResult> {
        if self.rows.len() >= PARALLEL_THRESHOLD {
            self.search_top_k_parallel(query, k)
        } else {
            self.search_top_k_sequential(query, k)
        }
    }
    
    fn search_top_k_sequential(&self, query: &[f32], k: usize) -> Vec<SearchResult> {
        let mut heap: BinaryHeap<Reverse<ScoredEntry>> = 
            BinaryHeap::with_capacity(k + 1);
        
        for row in &self.rows {
            let score = self.dot_product_at(row.vec_offset, query);
            
            heap.push(Reverse(ScoredEntry { 
                score, 
                doc_id: row.doc_id 
            }));
            
            if heap.len() > k {
                heap.pop();
            }
        }
        
        // Sort results descending
        let mut results: Vec<_> = heap
            .into_iter()
            .map(|Reverse(e)| SearchResult::from(e))
            .collect();
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        results
    }
    
    fn search_top_k_parallel(&self, query: &[f32], k: usize) -> Vec<SearchResult> {
        // Phase 1: Thread-local top-k collection
        let partial_results: Vec<Vec<ScoredEntry>> = self.rows
            .par_chunks(PARALLEL_CHUNK_SIZE)
            .map(|chunk| {
                let mut local_heap: BinaryHeap<Reverse<ScoredEntry>> = 
                    BinaryHeap::with_capacity(k + 1);
                
                for row in chunk {
                    let score = self.dot_product_at(row.vec_offset, query)
                        .unwrap_or(0.0);
                    
                    local_heap.push(Reverse(ScoredEntry { 
                        score, 
                        doc_id: row.doc_id 
                    }));
                    
                    if local_heap.len() > k {
                        local_heap.pop();
                    }
                }
                
                local_heap.into_iter().map(|r| r.0).collect()
            })
            .collect();
        
        // Phase 2: Merge thread-local heaps
        let mut final_heap: BinaryHeap<Reverse<ScoredEntry>> = 
            BinaryHeap::with_capacity(k + 1);
        
        for entries in partial_results {
            for entry in entries {
                final_heap.push(Reverse(entry));
                if final_heap.len() > k {
                    final_heap.pop();
                }
            }
        }
        
        // Sort results descending
        let mut results: Vec<_> = final_heap
            .into_iter()
            .map(|Reverse(e)| SearchResult::from(e))
            .collect();
        results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        results
    }
}
```

### 36.9 Cache-Efficient Data Layout (Struct of Arrays)

**Source**: CASS mining of cass vector index memory layout

```rust
/// Struct of Arrays (SoA) layout for cache efficiency
pub struct VectorIndex {
    // Metadata slab (70 bytes per row)
    rows: Vec<VectorRow>,
    
    // Vector slab (separate, contiguous)
    vectors: VectorStorage,
}

pub struct VectorRow {
    pub message_id: u64,      // 8 bytes
    pub created_at_ms: i64,   // 8 bytes
    pub agent_id: u32,        // 4 bytes
    pub workspace_id: u32,    // 4 bytes
    pub source_id: u32,       // 4 bytes
    pub role: u8,             // 1 byte
    pub chunk_idx: u8,        // 1 byte
    pub vec_offset: u64,      // 8 bytes (offset into vector slab)
    pub content_hash: [u8; 32], // 32 bytes
}

pub enum VectorStorage {
    F32(Vec<f32>),           // Full precision
    F16(Vec<f16>),           // Half precision (memory savings)
    Mmap { offset: u64, len: u64 }, // Memory-mapped file
}

const VECTOR_ALIGN_BYTES: usize = 32; // AVX-256 alignment

/// Ensure vector slab starts at 32-byte boundary
pub fn vector_slab_offset_bytes(header_len: usize, count: u32) -> usize {
    let rows_len = count as usize * std::mem::size_of::<VectorRow>();
    let end = header_len + rows_len;
    align_up(end, VECTOR_ALIGN_BYTES)
}

fn align_up(value: usize, align: usize) -> usize {
    let rem = value % align;
    if rem == 0 { value } else { value + (align - rem) }
}
```

**Benefits of SoA Layout**:
| Aspect | Array of Structs (AoS) | Struct of Arrays (SoA) |
|--------|------------------------|------------------------|
| **Cache utilization** | Poor (loads unused fields) | Excellent (loads only needed data) |
| **SIMD friendliness** | Poor (scattered data) | Excellent (contiguous data) |
| **Memory bandwidth** | Wasteful | Efficient |
| **Prefetching** | Unpredictable | Sequential access patterns |

### 36.10 Hash-Based Content Deduplication

**Source**: CASS mining of xf and cass embedding deduplication

```rust
use ring::digest::{Context, SHA256};

/// Compute content hash for deduplication
pub fn content_hash(text: &str) -> [u8; 32] {
    let mut context = Context::new(&SHA256);
    context.update(text.as_bytes());
    let digest = context.finish();
    
    let mut hash = [0u8; 32];
    hash.copy_from_slice(digest.as_ref());
    hash
}

/// Check if embedding already exists for content
impl EmbeddingCache {
    pub fn get_or_compute<F>(
        &mut self,
        content: &str,
        compute: F,
    ) -> Vec<f32>
    where
        F: FnOnce(&str) -> Vec<f32>,
    {
        let hash = content_hash(content);
        
        // Check cache
        if let Some(embedding) = self.get_by_hash(&hash) {
            return embedding.clone();
        }
        
        // Compute and cache
        let embedding = compute(content);
        self.insert(hash, embedding.clone());
        embedding
    }
}
```

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

```
❌ Random changes hoping something works
❌ Fixing symptoms instead of root cause
❌ Skipping reproduction step
❌ Adding debug code without removing it
❌ Assuming the bug is where you first looked
❌ Ignoring intermittent failures ("it works now")
```

### 37.2 Systematic Code Review for Bug Classes

**Source**: CASS mining of coding_agent_account_manager sync package review

#### Race Condition Hunting

```go
// BEFORE: Race condition on map access
for _, m := range s.state.Pool.Machines {  // No lock!
    // Another goroutine could modify Machines during iteration
}

// AFTER: Proper locking
s.state.Pool.mu.RLock()
machines := make([]*Machine, 0, len(s.state.Pool.Machines))
for _, m := range s.state.Pool.Machines {
    machines = append(machines, m)
}
s.state.Pool.mu.RUnlock()

for _, m := range machines {
    // Safe iteration over snapshot
}
```

**Race Condition Detection Checklist:**
- [ ] Map access from multiple goroutines → needs mutex
- [ ] Pointer/slice assignment without sync → data race
- [ ] Check-then-act without lock → TOCTOU vulnerability
- [ ] Shared mutable state in struct → needs sync primitives

#### Go Race Detector Usage

```bash
# Run tests with race detection
go test -race ./...

# Run specific package with race detection
go test -race -v ./internal/sync/...

# Run benchmarks with race detection (slower but catches races)
go test -race -bench=. ./...
```

**Example race condition fix:**

```go
// Race in OutputCapture - bytes.Buffer isn't thread-safe
type safeBuffer struct {
    mu  sync.Mutex
    buf bytes.Buffer
}

func (s *safeBuffer) Write(p []byte) (int, error) {
    s.mu.Lock()
    defer s.mu.Unlock()
    return s.buf.Write(p)
}

func (s *safeBuffer) String() string {
    s.mu.Lock()
    defer s.mu.Unlock()
    return s.buf.String()
}
```

### 37.3 Error Handling Issue Detection

**Source**: CASS mining of coding_agent_account_manager ssh.go review

#### Error Handling Bug Patterns

| Pattern | Issue | Fix |
|---------|-------|-----|
| **Swallowed error** | `if err != nil { /* ignore */ }` | Log or propagate |
| **Missing defer Close** | Resource opened but not closed on error | Add `defer f.Close()` after open |
| **Half-handled error** | Error checked but not all paths covered | Complete error path coverage |
| **Silent fallback** | Error replaced with default without logging | Log original error before fallback |

```go
// BEFORE: Swallowed error
if err := c.MkdirAll(dir); err != nil {
    // Directory might already exist, continue
}

// AFTER: Proper error handling
if err := c.MkdirAll(dir); err != nil {
    // Check if error is "already exists" (acceptable)
    if !os.IsExist(err) {
        return fmt.Errorf("failed to create directory %s: %w", dir, err)
    }
    // Log for debugging
    log.Debug("directory already exists", "path", dir)
}
```

#### Resource Leak Detection

```go
// BEFORE: Connection leak on error
conn, err := net.Dial("unix", socket)
if err != nil {
    return nil, err
}
client := agent.NewClient(conn)  // If this fails, conn leaks!

// AFTER: Proper cleanup
conn, err := net.Dial("unix", socket)
if err != nil {
    return nil, err
}
defer func() {
    if client == nil {
        conn.Close()  // Only close if we're not returning successfully
    }
}()
client := agent.NewClient(conn)
if client == nil {
    return nil, fmt.Errorf("failed to create agent client")
}
```

### 37.4 Performance Debugging Methodology

**Source**: CASS mining of beads_viewer pkg/ui performance analysis

#### Profiling Hot Paths

**Step 1: Identify the hot path**
```bash
# CPU profiling
go test -cpuprofile=cpu.prof -bench=BenchmarkRender
go tool pprof cpu.prof

# Memory profiling
go test -memprofile=mem.prof -bench=BenchmarkRender
go tool pprof -alloc_space mem.prof

# Trace for latency analysis
go test -trace=trace.out -bench=BenchmarkRender
go tool trace trace.out
```

**Step 2: Measure allocation pressure**

| Allocation Source | Count/Frame | Impact |
|-------------------|-------------|--------|
| `Renderer.NewStyle()` | 16 per item | High - 800 allocs at 50 items |
| `fmt.Sprintf()` | 6 per item | Medium - string allocations |
| `append()` to slice | 8-12 per item | Low with pre-allocation |

**Step 3: Apply targeted fixes**

```go
// BEFORE: Allocation in hot path
func (d *Delegate) Render(i Item) string {
    style := d.renderer.NewStyle().Foreground(color)  // Allocates every call!
    return style.Render(text)
}

// AFTER: Reuse styles
type Delegate struct {
    // Pre-computed styles
    normalStyle  lipgloss.Style
    selectedStyle lipgloss.Style
}

func NewDelegate(r *lipgloss.Renderer) *Delegate {
    return &Delegate{
        normalStyle:   r.NewStyle().Foreground(normalColor),
        selectedStyle: r.NewStyle().Foreground(selectedColor),
    }
}

func (d *Delegate) Render(i Item, selected bool) string {
    if selected {
        return d.selectedStyle.Render(text)  // No allocation!
    }
    return d.normalStyle.Render(text)
}
```

### 37.5 N+1 Query Pattern Detection

**Source**: CASS mining of mcp_agent_mail app.py N+1 analysis

#### Identifying N+1 Patterns

```python
# BEFORE: N+1 query pattern
async def _deliver_message(project, to_names, cc_names, bcc_names):
    # Each _get_agent() executes a separate query!
    to_agents = [await _get_agent(project, name) for name in to_names]    # N queries
    cc_agents = [await _get_agent(project, name) for name in cc_names]    # M queries
    bcc_agents = [await _get_agent(project, name) for name in bcc_names]  # K queries
    # Total: 1 + N + M + K queries instead of 2 queries

# AFTER: Batch query
async def _deliver_message(project, to_names, cc_names, bcc_names):
    all_names = list(set(to_names + cc_names + bcc_names))
    agents_by_name = await _get_agents_batch(project, all_names)  # 1 query!

    to_agents = [agents_by_name[n] for n in to_names]
    cc_agents = [agents_by_name[n] for n in cc_names]
    bcc_agents = [agents_by_name[n] for n in bcc_names]
```

```python
# Batch fetch implementation
async def _get_agents_batch(project: Project, names: list[str]) -> dict[str, Agent]:
    async with get_session() as session:
        stmt = select(Agent).where(
            Agent.project_id == project.id,
            Agent.name.in_(names)
        )
        result = await session.execute(stmt)
        agents = result.scalars().all()
        return {a.name: a for a in agents}
```

#### N+1 Detection Checklist

- [ ] Loop containing database query → batch outside loop
- [ ] Repeated function calls with single ID → batch with list
- [ ] ORM lazy loading in loop → eager load with joins
- [ ] HTTP request per item → batch API call

### 37.6 Test Failure Debugging

**Source**: CASS mining of coding_agent_session_search cli.rs test debugging

#### Analyzing Test Failures

```rust
// Test failure output analysis
// assertion `left == right` failed
//   left: "hi..."
//  right: "hi👋"

// Step 1: Understand what the test expected vs got
// Expected: "hi👋" (4 chars: h, i, 👋 where 👋 is 4 bytes)
// Got: "hi..." (5 chars: h, i, ., ., .)

// Step 2: Analyze the function under test
fn truncate_for_markdown(text: &str, max_bytes: usize) -> String {
    // Truncates at byte boundary, not character boundary
    // "hi👋" is 6 bytes (h=1, i=1, 👋=4)
    // truncate to 5 bytes → "hi" + "..." (can't fit 👋)
}

// Step 3: Determine if test expectation is wrong or code is wrong
// In this case: the function IS correct, test expectation was wrong
// Fix: Update test to expect "hi..."
```

#### Test Debugging Workflow

```bash
# 1. Run single failing test with verbose output
cargo test test_name -- --nocapture

# 2. Add debugging output to test
#[test]
fn my_test() {
    let result = function_under_test(input);
    eprintln!("Input: {:?}", input);
    eprintln!("Result: {:?}", result);
    eprintln!("Result bytes: {:?}", result.as_bytes());
    assert_eq!(result, expected);
}

# 3. Use RUST_BACKTRACE for panic location
RUST_BACKTRACE=1 cargo test test_name

# 4. Run with specific feature flags if needed
cargo test --features "test-utils" test_name
```

### 37.7 Comprehensive Investigation Report Format

**Source**: CASS mining of mcp_agent_mail manifest validation investigation

When debugging complex issues, use a structured report format:

```markdown
## Investigation Report: [Component Name]

### Executive Summary
[1-2 sentence overview of findings]

### CRITICAL ISSUES (Must fix before release)

#### Issue 1: [Descriptive Title]
**File:** `path/to/file.ts:line`
**Issue:** [Clear description of the problem]
**Impact:** [What breaks if not fixed]
**Code:**
```typescript
// Current problematic code
```
**Suggested Fix:**
```typescript
// Corrected code
```

### HIGH-PRIORITY ISSUES (Should fix soon)
[Same format]

### MEDIUM-PRIORITY ISSUES (Fix when convenient)
[Same format]

### LOW-PRIORITY ISSUES (Nice to have)
[Same format]

### Summary Table
| Priority | Issue | Impact | Location |
|----------|-------|--------|----------|
| Critical | ... | ... | file:line |
| High | ... | ... | file:line |
```

### 37.8 Print Debugging Best Practices

**Source**: CASS mining of coding_agent_session_search CLI normalization debugging

#### Strategic Debug Output

```rust
// Instead of random println!, use structured debug output
#[cfg(debug_assertions)]
macro_rules! debug_trace {
    ($($arg:tt)*) => {
        if std::env::var("CASS_DEBUG").is_ok() {
            eprintln!("[DEBUG {}:{}] {}", file!(), line!(), format!($($arg)*));
        }
    };
}

// Usage in code
fn normalize_args(args: &[String]) -> Vec<String> {
    debug_trace!("Input args: {:?}", args);

    let result = args.iter()
        .map(|arg| {
            let normalized = normalize_single_arg(arg);
            debug_trace!("  {} -> {}", arg, normalized);
            normalized
        })
        .collect();

    debug_trace!("Output args: {:?}", result);
    result
}
```

#### Structured Logging for Debugging

```go
// Go: Use structured logging instead of fmt.Println
import "log/slog"

func processItem(item Item) error {
    logger := slog.With(
        "item_id", item.ID,
        "item_type", item.Type,
    )

    logger.Debug("starting processing")

    result, err := doWork(item)
    if err != nil {
        logger.Error("processing failed",
            "error", err,
            "stage", "doWork",
        )
        return err
    }

    logger.Debug("processing complete",
        "result_size", len(result),
        "duration_ms", elapsed.Milliseconds(),
    )
    return nil
}
```

```python
# Python: Use structlog for debugging
import structlog

logger = structlog.get_logger()

async def process_message(msg: Message) -> None:
    log = logger.bind(
        message_id=msg.id,
        sender=msg.sender,
    )

    log.debug("processing_started")

    try:
        result = await deliver(msg)
        log.debug("processing_complete", recipients=len(result.delivered))
    except DeliveryError as e:
        log.error("delivery_failed", error=str(e), stage="deliver")
        raise
```

### 37.9 Concurrency Debugging

**Source**: CASS mining of mcp_agent_mail rate limit debugging

#### Detecting Race Conditions in Async Code

```python
# BEFORE: Race condition in rate limiter
class TokenBucket:
    def __init__(self, rate: float, burst: int):
        self._buckets: dict[str, tuple[float, float]] = {}

    async def acquire(self, key: str) -> bool:
        now = time.monotonic()
        tokens, ts = self._buckets.get(key, (float(self.burst), now))
        # Race: Two coroutines can read same tokens, both succeed
        tokens = min(self.burst, tokens + (now - ts) * self.rate)
        if tokens >= 1:
            self._buckets[key] = (tokens - 1, now)  # Race: lost update
            return True
        return False

# AFTER: Thread-safe with lock
class TokenBucket:
    def __init__(self, rate: float, burst: int):
        self._buckets: dict[str, tuple[float, float]] = {}
        self._lock = asyncio.Lock()

    async def acquire(self, key: str) -> bool:
        async with self._lock:
            now = time.monotonic()
            tokens, ts = self._buckets.get(key, (float(self.burst), now))
            tokens = min(self.burst, tokens + (now - ts) * self.rate)
            if tokens >= 1:
                self._buckets[key] = (tokens - 1, now)
                return True
            return False
```

#### Deadlock Prevention

```go
// Lock ordering to prevent deadlock
// Always acquire locks in consistent order: A before B

// WRONG: Inconsistent lock order
func transfer1(a, b *Account, amount int) {
    a.mu.Lock()
    b.mu.Lock()  // Thread 1 holds A, wants B
    // ...
}

func transfer2(a, b *Account, amount int) {
    b.mu.Lock()
    a.mu.Lock()  // Thread 2 holds B, wants A → DEADLOCK
    // ...
}

// CORRECT: Consistent lock order by ID
func transfer(a, b *Account, amount int) {
    first, second := a, b
    if a.ID > b.ID {
        first, second = b, a
    }
    first.mu.Lock()
    second.mu.Lock()
    defer second.mu.Unlock()
    defer first.mu.Unlock()
    // ...
}
```

### 37.10 Timeout and Context Deadline Debugging

**Source**: CASS mining of coding_agent_account_manager script test handling

```go
// Debugging timeout issues with context
func runWithTimeout(ctx context.Context, cmd *exec.Cmd) error {
    // Create timeout context
    ctx, cancel := context.WithTimeout(ctx, 2*time.Second)
    defer cancel()

    cmd.SysProcAttr = &syscall.SysProcAttr{
        Setpgid: true,  // Create process group for cleanup
    }

    if err := cmd.Start(); err != nil {
        return fmt.Errorf("start failed: %w", err)
    }

    done := make(chan error, 1)
    go func() {
        done <- cmd.Wait()
    }()

    select {
    case err := <-done:
        return err
    case <-ctx.Done():
        // Kill entire process group
        if cmd.Process != nil {
            syscall.Kill(-cmd.Process.Pid, syscall.SIGKILL)
        }
        return fmt.Errorf("command timed out: %w", ctx.Err())
    }
}
```

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

```bash
# Step 1: Run clippy with strict settings
cargo clippy --all-targets -- -D warnings

# Step 2: If many errors, try auto-fix first
cargo clippy --fix --allow-dirty --all-targets

# Step 3: Manually fix remaining issues
# Step 4: Re-run to verify clean state
cargo clippy --all-targets -- -D warnings
```

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

```rust
// BEFORE: Benchmark with type annotation errors (Rust 2024 edition)
c.bench_function("heredoc_scan", |b| {
    b.iter(|| scan_heredoc(&cmd))
});

// AFTER: Explicit type annotations for Criterion closures
c.bench_function("heredoc_scan", |b: &mut criterion::Bencher<'_>| {
    b.iter(|| scan_heredoc(&cmd))
});

// BEFORE: Inefficient string building
result.push_str(&format!("Line: {}\n", line));

// AFTER: Efficient write macro
use std::fmt::Write as _;
writeln!(result, "Line: {}", line).unwrap();

// BEFORE: Verbose pattern matching
if let Some(before) = trimmed.strip_suffix('\\') {
    process(before)
} else {
    process(trimmed)
}

// AFTER: Functional style with map_or
trimmed.strip_suffix('\\').map_or(trimmed, |before| before)
```

### 38.3 Dead Code Removal

#### 38.3.1 Detection Strategies

1. **Compiler warnings**: `#[warn(dead_code)]` in Rust, unused import warnings
2. **IDE analysis**: Gray/faded code indicating unused symbols
3. **Search verification**: `rg "symbol_name"` to confirm no usages
4. **Comment archaeology**: Check if code is referenced only in comments

#### 38.3.2 Safe Removal Process

```bash
# 1. Search for all usages
rg "function_name" --type rust

# 2. Check if only in comments or definitions
rg "function_name" --type rust | grep -v "^.*\.rs\.bak:"

# 3. If truly unused, remove (or add #[allow(dead_code)] with comment)
# 4. Run tests to verify nothing broke
cargo test
```

#### 38.3.3 Example: Orphaned File Detection

From a real session (brenner_bot):

```
**Note:** There's an orphaned `apps/web/src/lib/prompt-builder.ts` file 
(285 lines) that was created but never integrated. Per AGENTS.md Rule 1, 
I cannot delete it without explicit permission. Would you like me to 
delete it?
```

**Key Pattern**: Always flag orphaned files explicitly rather than silently removing them.

### 38.4 Unused Variable Handling

#### 38.4.1 The Underscore Convention

```typescript
// BEFORE: Unused parameter warning
const logger = ({ page }, useFixture, testInfo) => { ... }

// AFTER: Underscore prefix signals intentional non-use
const logger = ({ page: _page }, useFixture, testInfo) => { ... }

// ALTERNATIVE: Destructure to empty object if fixture expects structure
const logger = ({ }, useFixture, testInfo) => { ... }
```

#### 38.4.2 Rust-Specific Patterns

```rust
// Unused but required by trait
fn unused_callback(_ctx: &Context) { }

// Unused in tests but needed for setup
let _guard = setup_test_env();

// Suppress with attribute when intentional
#[allow(unused_variables)]
fn handler(request: Request, _response: Response) { ... }
```

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

```rust
// BEFORE: 200-line run_command function
fn run_command(cmd: &Command) -> Result<Output> {
    // ... 200 lines of logic
}

// AFTER: Decomposed into focused helpers
fn run_command(cmd: &Command) -> Result<Output> {
    let config = load_config()?;
    let validated = validate_command(cmd, &config)?;
    let result = execute_validated(validated)?;
    format_output(result)
}

// Or use allow attribute if decomposition hurts readability
#[allow(clippy::too_many_lines)]
fn run_command(cmd: &Command) -> Result<Output> {
    // Long but cohesive function - splitting would obscure logic
}
```

### 38.6 Code Organization Patterns

#### 38.6.1 Module Structure (Rust Example)

From beads_viewer architecture:

```
pkg/
├── analysis/              # Graph analysis engine (45+ files)
│   ├── graph.go          # Core graph algorithms
│   ├── config.go         # Analysis configuration
│   ├── triage.go         # Unified output format
│   └── [specialized modules]
├── ui/                    # TUI components (60+ files)
│   ├── model.go          # Master state machine
│   └── [view-specific modules]
├── loader/               # Data loading
├── model/                # Data types
└── export/               # Output formats
```

**Key Principles**:
1. **Clear separation of concerns**: loader, analysis, UI, export are independent
2. **Flat-ish structure**: Avoid deep nesting of modules
3. **Test files colocated**: `foo_test.go` next to `foo.go`
4. **Shared types in `model/`**: Prevents circular dependencies

#### 38.6.2 Layered Architecture

```
┌─────────────────────────────────────┐
│           CLI / TUI Layer           │  ← User interaction
├─────────────────────────────────────┤
│         Business Logic Layer        │  ← Core algorithms
├─────────────────────────────────────┤
│          Data Access Layer          │  ← File I/O, DB
├─────────────────────────────────────┤
│           Model / Types             │  ← Shared data structures
└─────────────────────────────────────┘
```

### 38.7 Consistency Improvements

#### 38.7.1 Pattern Normalization

From mcp_agent_mail code review:

```python
# BEFORE: Inconsistent path normalization
spec = _compile_pathspec(existing.path_pattern)  # Raw pattern
return spec.match_file(_normalize(candidate_path))  # Normalized path

# AFTER: Consistent normalization on both sides
spec = _compile_pathspec(_normalize_pathspec_pattern(existing.path_pattern))
return spec.match_file(_normalize(candidate_path))
```

**Impact**: Cache key consistency improved (4 different path formats → 1 cache entry)

#### 38.7.2 Error Handling Consistency

```go
// BEFORE: Mixed error handling styles
if err != nil {
    return err
}
// ... later ...
if err != nil {
    log.Printf("error: %v", err)
    return nil
}

// AFTER: Consistent error propagation
if err != nil {
    return fmt.Errorf("operation failed: %w", err)
}
```

### 38.8 Defensive Refactoring

#### 38.8.1 Redundant But Safe Checks

From code review:

```python
# The `if spec is not None:` check is technically redundant (since we 
# check PathSpec availability in the outer condition), but it's defensive 
# coding that provides robustness if `_compile_pathspec` is ever modified

if PathSpec is not None and GitWildMatchPattern is not None:
    spec = _compile_pathspec(...)
    if spec is not None:  # Defensive - protects against future changes
        return spec.match_file(...)
```

**Principle**: Accept minor redundancy when it protects against future regressions.

#### 38.8.2 Array Mutation Prevention

From brenner_bot bug fix:

```typescript
// BEFORE: Array mutation bug - .sort() mutates in place
const hasCustomizations = value.sort() !== DEFAULT_OPERATORS.sort();

// AFTER: Create copies before sorting
const hasCustomizations = 
    [...value].sort().join(',') !== [...DEFAULT_OPERATORS].sort().join(',');
```

### 38.9 Type System Improvements

#### 38.9.1 Strengthening Types

```rust
// BEFORE: Stringly-typed
fn set_status(status: &str) { ... }

// AFTER: Enum-typed
enum Status { Open, InProgress, Blocked, Closed }
fn set_status(status: Status) { ... }
```

#### 38.9.2 Narrowing Generic Constraints

```rust
// BEFORE: Overly generic
fn process<T>(item: T) { ... }

// AFTER: Constrained to what's actually needed
fn process<T: Display + Clone>(item: T) { ... }
```

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

```typescript
// schemas.ts - Centralized schema registry
import { z } from 'zod';
import { extendZodWithOpenApi } from '@asteasolutions/zod-to-openapi';

extendZodWithOpenApi(z);

// Request Validation Schemas
export const CreateReservationSchema = z.object({
  projectId: z.string().uuid().openapi({ description: 'Project identifier' }),
  paths: z.array(z.string()).min(1).openapi({
    description: 'File paths or glob patterns to reserve'
  }),
  ttlSeconds: z.number().int().min(60).max(86400).default(3600),
  exclusive: z.boolean().default(true),
  reason: z.string().max(500).optional()
}).openapi('CreateReservationRequest');

// Query/Filter Schemas
export const ListReservationsQuerySchema = z.object({
  projectId: z.string().uuid(),
  limit: z.coerce.number().int().min(1).max(100).default(20),
  startingAfter: z.string().optional(),
  endingBefore: z.string().optional(),
  status: z.enum(['active', 'expired', 'released']).optional()
}).openapi('ListReservationsQuery');

// Discriminated Union Schemas (for complex configs)
export const StepConfigSchema = z.discriminatedUnion('type', [
  z.object({ type: z.literal('agent_task'), prompt: z.string() }),
  z.object({ type: z.literal('conditional'), condition: z.string() }),
  z.object({ type: z.literal('parallel'), branches: z.array(z.lazy(() => StepConfigSchema)) }),
  z.object({ type: z.literal('approval'), approvers: z.array(z.string()) }),
  z.object({ type: z.literal('script'), command: z.string() }),
]).openapi('StepConfig');
```

#### 39.2.2 Schema Categories

| Category | Count | Purpose | Example |
|----------|-------|---------|---------|
| **Request Validation** | 46 | Validate POST/PUT bodies | CreateJobSchema, SendMessageSchema |
| **Query/Filter** | 19 | Validate GET query params | ListReposQuerySchema, SearchQuerySchema |
| **Discriminated Union** | 3 | Type-safe polymorphism | StepConfigSchema, BudgetStrategySchema |
| **Enum** | 6 | Constrained string sets | ProviderSchema, ProfileStatusSchema |
| **Configuration** | 8 | Complex nested configs | UpdateConfigSchema, RateCardSchema |

#### 39.2.3 OpenAPI Generation

```typescript
// generate-openapi.ts
import { OpenAPIRegistry, OpenApiGeneratorV31 } from '@asteasolutions/zod-to-openapi';
import * as schemas from './schemas';

const registry = new OpenAPIRegistry();

// Register all schemas
Object.entries(schemas).forEach(([name, schema]) => {
  if (schema instanceof z.ZodType) {
    registry.register(name, schema);
  }
});

// Define routes
registry.registerPath({
  method: 'post',
  path: '/api/v1/reservations',
  description: 'Create a file reservation',
  request: { body: { content: { 'application/json': { schema: CreateReservationSchema } } } },
  responses: {
    201: { description: 'Reservation created', content: { 'application/json': { schema: ReservationResponseSchema } } },
    409: { description: 'Conflict with existing reservation', content: { 'application/json': { schema: ErrorResponseSchema } } }
  }
});

// Generate spec
const generator = new OpenApiGeneratorV31(registry.definitions);
export const openApiDocument = generator.generateDocument({
  openapi: '3.1.0',
  info: { title: 'Flywheel Gateway API', version: '1.0.0' },
  servers: [{ url: '/api/v1' }]
});
```

**Exposed Endpoints:**
- `GET /openapi.json` - Raw OpenAPI 3.1 specification
- `GET /docs` - Swagger UI interactive documentation
- `GET /redoc` - ReDoc documentation

### 39.3 Error Taxonomy

#### 39.3.1 Structured Error Codes

A production error taxonomy with 55+ codes across 7 categories:

```typescript
// error-codes.ts
export type ErrorCategory =
  | 'RESOURCE'    // Entity not found, conflicts, gone
  | 'VALIDATION'  // Input validation failures
  | 'AUTH'        // Authentication/authorization
  | 'QUOTA'       // Rate limits, usage limits
  | 'STATE'       // Invalid state transitions
  | 'DEPENDENCY'  // External service failures
  | 'INTERNAL';   // Server errors

export interface ErrorDefinition {
  code: string;
  httpStatus: number;
  message: string;
  retryable: boolean;
  aiHint?: string;  // Guidance for AI agents
}

export const ERROR_CODES: Record<string, ErrorDefinition> = {
  // RESOURCE errors (404, 409, 410)
  RESOURCE_NOT_FOUND: {
    code: 'RESOURCE_NOT_FOUND',
    httpStatus: 404,
    message: 'The requested resource does not exist',
    retryable: false,
    aiHint: 'Check the ID format and verify the resource was created'
  },
  RESOURCE_CONFLICT: {
    code: 'RESOURCE_CONFLICT',
    httpStatus: 409,
    message: 'Resource already exists or conflicts with current state',
    retryable: false,
    aiHint: 'Use GET to check current state before PUT/POST'
  },
  RESOURCE_GONE: {
    code: 'RESOURCE_GONE',
    httpStatus: 410,
    message: 'Resource has been permanently deleted',
    retryable: false,
    aiHint: 'This resource cannot be recovered; create a new one'
  },

  // VALIDATION errors (400)
  VALIDATION_FAILED: {
    code: 'VALIDATION_FAILED',
    httpStatus: 400,
    message: 'Request validation failed',
    retryable: false,
    aiHint: 'Check the errors array for specific field issues'
  },
  VALIDATION_SCHEMA_MISMATCH: {
    code: 'VALIDATION_SCHEMA_MISMATCH',
    httpStatus: 400,
    message: 'Request body does not match expected schema',
    retryable: false,
    aiHint: 'Verify JSON structure against OpenAPI spec'
  },

  // AUTH errors (401, 403)
  AUTH_TOKEN_EXPIRED: {
    code: 'AUTH_TOKEN_EXPIRED',
    httpStatus: 401,
    message: 'Access token has expired',
    retryable: true,
    aiHint: 'Refresh the token using the refresh_token endpoint'
  },
  AUTH_INSUFFICIENT_SCOPE: {
    code: 'AUTH_INSUFFICIENT_SCOPE',
    httpStatus: 403,
    message: 'Token lacks required permissions',
    retryable: false,
    aiHint: 'Request a new token with the required scope'
  },

  // QUOTA errors (429)
  QUOTA_RATE_LIMITED: {
    code: 'QUOTA_RATE_LIMITED',
    httpStatus: 429,
    message: 'Too many requests',
    retryable: true,
    aiHint: 'Retry after the time indicated in Retry-After header'
  },
  QUOTA_BUDGET_EXCEEDED: {
    code: 'QUOTA_BUDGET_EXCEEDED',
    httpStatus: 429,
    message: 'Cost budget for this period exhausted',
    retryable: false,
    aiHint: 'Wait for budget reset or request budget increase'
  },

  // STATE errors (409, 422)
  STATE_INVALID_TRANSITION: {
    code: 'STATE_INVALID_TRANSITION',
    httpStatus: 409,
    message: 'Cannot transition from current state to requested state',
    retryable: false,
    aiHint: 'Check current state with GET, then apply valid transition'
  },

  // DEPENDENCY errors (502, 503)
  DEPENDENCY_UNAVAILABLE: {
    code: 'DEPENDENCY_UNAVAILABLE',
    httpStatus: 503,
    message: 'Required external service is unavailable',
    retryable: true,
    aiHint: 'Retry with exponential backoff; check status page'
  },

  // INTERNAL errors (500)
  INTERNAL_ERROR: {
    code: 'INTERNAL_ERROR',
    httpStatus: 500,
    message: 'An unexpected error occurred',
    retryable: true,
    aiHint: 'Retry once; if persistent, report with request ID'
  }
};
```

#### 39.3.2 Error Response Format

```typescript
// Standard error response structure
export interface ApiError {
  error: {
    code: string;           // Machine-readable error code
    message: string;        // Human-readable message
    retryable: boolean;     // Can this be retried?
    aiHint?: string;        // Guidance for AI agents
    details?: {             // Validation errors
      field: string;
      issue: string;
      received?: unknown;
    }[];
    requestId: string;      // For support/debugging
  };
}

// Example response
{
  "error": {
    "code": "VALIDATION_FAILED",
    "message": "Request validation failed",
    "retryable": false,
    "aiHint": "Check the errors array for specific field issues",
    "details": [
      { "field": "ttlSeconds", "issue": "must be at least 60", "received": 30 },
      { "field": "paths", "issue": "must contain at least 1 item", "received": [] }
    ],
    "requestId": "req_abc123xyz"
  }
}
```

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

```typescript
// device-codes.ts - Device authorization flow
import { randomBytes } from 'crypto';

interface DeviceCodeRecord {
  deviceCode: string;      // 256-bit random, for polling
  userCode: string;        // 8-char human-readable, for entry
  expiresAt: Date;         // 15-minute window
  interval: number;        // Polling interval (5s)
  scope: string[];
  clientId: string;
  status: 'pending' | 'authorized' | 'denied' | 'expired';
  userId?: string;         // Set when user authorizes
}

// Generate cryptographically secure device code
function generateDeviceCode(): string {
  return randomBytes(32).toString('base64url'); // 256 bits
}

// Generate user-friendly code (excludes confusing chars)
function generateUserCode(): string {
  const alphabet = 'BCDFGHJKLMNPQRSTVWXZ'; // No vowels, no 0/1/I/O
  let code = '';
  const bytes = randomBytes(8);
  for (let i = 0; i < 8; i++) {
    code += alphabet[bytes[i] % alphabet.length];
  }
  return code.slice(0, 4) + '-' + code.slice(4); // XXXX-XXXX format
}

// Endpoints
// POST /oauth/device/code - Start device flow
// POST /oauth/device/token - Poll for token (returns 'authorization_pending')
// GET /device?user_code=XXXX-XXXX - User authorization page
```

#### 39.4.2 Token Management

```typescript
// JWT with refresh token rotation
interface TokenPair {
  accessToken: string;     // Short-lived (15 min)
  refreshToken: string;    // Longer-lived (7 days), single-use
  expiresIn: number;       // Seconds until access token expires
  tokenType: 'Bearer';
}

interface RefreshTokenRecord {
  token: string;           // The refresh token value
  familyId: string;        // Token family for rotation tracking
  userId: string;
  scope: string[];
  expiresAt: Date;
  revokedAt?: Date;        // Set when used or explicitly revoked
}

// Security: Token family rotation
// - Each refresh creates a new token in the same family
// - If an old token is reused (replay attack), revoke entire family
async function refreshTokens(refreshToken: string): Promise<TokenPair> {
  const record = await findRefreshToken(refreshToken);

  if (record.revokedAt) {
    // Replay attack detected! Revoke entire family
    await revokeTokenFamily(record.familyId);
    throw new AuthError('AUTH_TOKEN_REPLAY');
  }

  // Mark current token as used
  await revokeRefreshToken(refreshToken);

  // Issue new token pair in same family
  return issueTokenPair(record.userId, record.scope, record.familyId);
}
```

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

```typescript
// Cursor-based pagination interface
export interface PaginatedRequest {
  limit?: number;          // Page size (default 20, max 100)
  startingAfter?: string;  // Cursor: fetch items after this ID
  endingBefore?: string;   // Cursor: fetch items before this ID
}

export interface PaginatedResponse<T> {
  data: T[];
  hasMore: boolean;        // More items exist
  nextCursor?: string;     // Cursor for next page (if hasMore)
  prevCursor?: string;     // Cursor for previous page
}

// Implementation
async function listReservations(params: ListReservationsParams): Promise<PaginatedResponse<FileReservation>> {
  const limit = Math.min(params.limit ?? 20, 100);

  let query = db.select().from(reservations)
    .where(eq(reservations.projectId, params.projectId))
    .orderBy(desc(reservations.createdAt), desc(reservations.id))
    .limit(limit + 1); // Fetch one extra to detect hasMore

  if (params.startingAfter) {
    const cursor = await findReservation(params.startingAfter);
    query = query.where(
      or(
        lt(reservations.createdAt, cursor.createdAt),
        and(
          eq(reservations.createdAt, cursor.createdAt),
          lt(reservations.id, cursor.id)
        )
      )
    );
  }

  const results = await query;
  const hasMore = results.length > limit;
  const data = hasMore ? results.slice(0, limit) : results;

  return {
    data,
    hasMore,
    nextCursor: hasMore ? data[data.length - 1].id : undefined,
    prevCursor: params.startingAfter
  };
}
```

#### 39.5.2 Cursor Encoding

```typescript
// Opaque cursor encoding (hides implementation details)
function encodeCursor(item: { id: string; createdAt: Date }): string {
  const payload = JSON.stringify({
    i: item.id,
    t: item.createdAt.getTime()
  });
  return Buffer.from(payload).toString('base64url');
}

function decodeCursor(cursor: string): { id: string; timestamp: number } {
  const payload = JSON.parse(Buffer.from(cursor, 'base64url').toString());
  return { id: payload.i, timestamp: payload.t };
}
```

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

```typescript
// idempotency-middleware.ts
interface IdempotencyRecord {
  key: string;             // Client-provided idempotency key
  requestFingerprint: string;  // Hash of request body
  status: 'processing' | 'completed' | 'failed';
  response?: {
    status: number;
    body: unknown;
    headers: Record<string, string>;
  };
  expiresAt: Date;         // TTL for cached response
}

// Middleware implementation
async function idempotencyMiddleware(c: Context, next: Next) {
  const key = c.req.header('Idempotency-Key');
  if (!key) return next(); // No key = normal processing

  const fingerprint = hashRequestBody(await c.req.text());
  const existing = await getIdempotencyRecord(key);

  if (existing) {
    // Check for request mismatch (same key, different request)
    if (existing.requestFingerprint !== fingerprint) {
      return c.json({
        error: {
          code: 'IDEMPOTENCY_KEY_MISMATCH',
          message: 'Idempotency key was used with a different request'
        }
      }, 422);
    }

    // Return cached response if completed
    if (existing.status === 'completed') {
      return new Response(JSON.stringify(existing.response.body), {
        status: existing.response.status,
        headers: existing.response.headers
      });
    }

    // Still processing - client should retry later
    if (existing.status === 'processing') {
      return c.json({
        error: {
          code: 'IDEMPOTENCY_KEY_IN_PROGRESS',
          message: 'Request is still being processed'
        }
      }, 409);
    }
  }

  // Create processing record
  await createIdempotencyRecord({
    key,
    requestFingerprint: fingerprint,
    status: 'processing',
    expiresAt: new Date(Date.now() + 24 * 60 * 60 * 1000) // 24 hours
  });

  // Execute request
  await next();

  // Store response for future retries
  await updateIdempotencyRecord(key, {
    status: c.res.ok ? 'completed' : 'failed',
    response: {
      status: c.res.status,
      body: await c.res.json(),
      headers: Object.fromEntries(c.res.headers)
    }
  });
}
```

#### 39.6.2 Client Usage

```typescript
// Client generating idempotency keys
const idempotencyKey = `${userId}-${operationType}-${Date.now()}-${randomBytes(8).toString('hex')}`;

const response = await fetch('/api/reservations', {
  method: 'POST',
  headers: {
    'Content-Type': 'application/json',
    'Idempotency-Key': idempotencyKey
  },
  body: JSON.stringify(reservationRequest)
});

// Safe to retry on network error with same key
```

### 39.7 Route Organization

#### 39.7.1 Hono-Based Route Structure

```typescript
// routes/reservations.ts
import { Hono } from 'hono';
import { zValidator } from '@hono/zod-validator';
import { CreateReservationSchema, ListReservationsQuerySchema } from '../schemas';

const app = new Hono()
  .post('/',
    zValidator('json', CreateReservationSchema),
    async (c) => {
      const body = c.req.valid('json');
      const reservation = await reservationService.create(body);
      return c.json(reservation, 201);
    }
  )
  .get('/',
    zValidator('query', ListReservationsQuerySchema),
    async (c) => {
      const query = c.req.valid('query');
      const result = await reservationService.list(query);
      return c.json(result);
    }
  )
  .get('/:id', async (c) => {
    const reservation = await reservationService.findById(c.req.param('id'));
    if (!reservation) {
      return c.json({ error: ERROR_CODES.RESOURCE_NOT_FOUND }, 404);
    }
    return c.json(reservation);
  })
  .delete('/:id', async (c) => {
    await reservationService.release(c.req.param('id'));
    return c.body(null, 204);
  });

export default app;
```

#### 39.7.2 Route File Organization

```
apps/gateway/src/
├── routes/
│   ├── accounts.ts        # Account management
│   ├── agents.ts          # Agent lifecycle
│   ├── alerts.ts          # Alert configuration
│   ├── analytics.ts       # Usage analytics
│   ├── audit.ts           # Audit logs
│   ├── beads.ts           # Issue tracking
│   ├── cass.ts            # Session search
│   ├── checkpoints.ts     # State snapshots
│   ├── conflicts.ts       # Conflict detection
│   ├── context.ts         # Context building
│   ├── handoffs.ts        # Agent handoffs
│   ├── history.ts         # Conversation history
│   ├── jobs.ts            # Job orchestration
│   ├── mail.ts            # Agent messaging
│   ├── memory.ts          # Shared memory
│   ├── metrics.ts         # Performance metrics
│   ├── notifications.ts   # User notifications
│   ├── pipelines.ts       # Multi-step workflows
│   ├── reservations.ts    # File locking
│   └── utilities.ts       # Health, version, etc.
├── api/
│   ├── schemas.ts         # Centralized Zod schemas
│   └── generate-openapi.ts # OpenAPI generator
├── middleware/
│   ├── auth.ts            # JWT validation
│   ├── idempotency.ts     # Idempotency handling
│   └── rate-limit.ts      # Rate limiting
└── utils/
    ├── validation.ts      # Zod error transformation
    └── response.ts        # Standard response helpers
```

### 39.8 Request/Response Patterns

#### 39.8.1 Standard Response Helpers

```typescript
// utils/response.ts
export function sendResource<T>(c: Context, data: T, status = 200) {
  return c.json({ data }, status);
}

export function sendCreated<T>(c: Context, data: T, location?: string) {
  if (location) {
    c.header('Location', location);
  }
  return c.json({ data }, 201);
}

export function sendNoContent(c: Context) {
  return c.body(null, 204);
}

export function sendError(c: Context, error: ErrorDefinition, details?: unknown) {
  return c.json({
    error: {
      code: error.code,
      message: error.message,
      retryable: error.retryable,
      aiHint: error.aiHint,
      details,
      requestId: c.get('requestId')
    }
  }, error.httpStatus);
}

export function sendValidationError(c: Context, zodError: ZodError) {
  const details = zodError.errors.map(e => ({
    field: e.path.join('.'),
    issue: e.message,
    received: e.received
  }));
  return sendError(c, ERROR_CODES.VALIDATION_FAILED, details);
}
```

#### 39.8.2 Validation Error Transformation

```typescript
// utils/validation.ts
export function transformZodError(error: ZodError): string {
  return error.errors.map(e => {
    const path = e.path.join('.');
    switch (e.code) {
      case 'invalid_type':
        return `${path}: expected ${e.expected}, received ${e.received}`;
      case 'too_small':
        return `${path}: must be at least ${e.minimum}`;
      case 'too_big':
        return `${path}: must be at most ${e.maximum}`;
      case 'invalid_enum_value':
        return `${path}: must be one of ${e.options.join(', ')}`;
      default:
        return `${path}: ${e.message}`;
    }
  }).join('; ');
}
```

### 39.9 API Versioning Strategies

#### 39.9.1 URL Path Versioning

```typescript
// Preferred for clarity
app.route('/api/v1', v1Routes);
app.route('/api/v2', v2Routes);

// Version in path makes caching easier, clearer in logs
// GET /api/v1/reservations vs GET /api/v2/reservations
```

#### 39.9.2 Header-Based Versioning

```typescript
// For more granular control
app.use(async (c, next) => {
  const version = c.req.header('API-Version') ?? 'v1';
  c.set('apiVersion', version);
  await next();
});

// Handler checks version for behavior differences
app.get('/reservations', async (c) => {
  const version = c.get('apiVersion');
  if (version === 'v2') {
    // New response format
    return c.json({ data: reservations, meta: { total } });
  }
  // Legacy format
  return c.json(reservations);
});
```

#### 39.9.3 Versioning Decision Matrix

| Strategy | Pros | Cons | Use When |
|----------|------|------|----------|
| **URL Path** | Clear, cacheable, visible in logs | URL changes between versions | Breaking changes, public APIs |
| **Header** | URL stable, granular | Hidden, harder to cache | Internal APIs, minor changes |
| **Query Param** | Easy to switch | Pollutes URLs, caching issues | Rarely recommended |

### 39.10 Rate Limiting

#### 39.10.1 Multi-Tier Rate Limiting

```typescript
// rate-limit.ts
interface RateLimitConfig {
  windowMs: number;        // Time window in ms
  max: number;             // Max requests per window
  keyGenerator: (c: Context) => string;
}

const rateLimits: Record<string, RateLimitConfig> = {
  // Global per-IP (burst protection)
  global: {
    windowMs: 1000,        // 1 second
    max: 100,
    keyGenerator: (c) => c.req.header('x-forwarded-for') ?? 'unknown'
  },

  // Per-user sustained rate
  user: {
    windowMs: 60 * 1000,   // 1 minute
    max: 300,
    keyGenerator: (c) => c.get('userId') ?? 'anonymous'
  },

  // Per-endpoint for expensive operations
  expensive: {
    windowMs: 60 * 1000,
    max: 10,
    keyGenerator: (c) => `${c.get('userId')}:${c.req.path}`
  }
};

// Response headers for clients
c.header('X-RateLimit-Limit', limit.toString());
c.header('X-RateLimit-Remaining', remaining.toString());
c.header('X-RateLimit-Reset', resetTime.toString());
c.header('Retry-After', retryAfter.toString());
```

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

```typescript
// BAD: Items shift as data changes
GET /items?offset=100&limit=20

// GOOD: Stable cursor
GET /items?startingAfter=item_abc&limit=20
```

#### 39.13.2 Avoid: Generic Error Responses

```typescript
// BAD: No structure, no guidance
{ "error": "Something went wrong" }

// GOOD: Actionable, machine-readable
{
  "error": {
    "code": "QUOTA_RATE_LIMITED",
    "message": "Too many requests",
    "retryable": true,
    "aiHint": "Retry after the time indicated in Retry-After header"
  }
}
```

#### 39.13.3 Avoid: Inconsistent Naming

```typescript
// BAD: Mixed conventions
GET /getUsers
POST /create-reservation
PUT /update_job

// GOOD: Consistent REST nouns
GET /users
POST /reservations
PUT /jobs/:id
```

#### 39.13.4 Avoid: Overloaded Endpoints

```typescript
// BAD: One endpoint doing many things
POST /api/actions { "action": "create" | "update" | "delete", ... }

// GOOD: RESTful resources
POST /resources      # Create
PUT /resources/:id   # Update
DELETE /resources/:id # Delete
```
