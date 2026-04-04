# Resource Tree Usage Guide

## Purpose

This document explains the current shipped behavior of the resource-tree update in `meta_skill`.

It is intentionally focused on:

- what the system does now
- how to use it from the CLI
- how agents should use it through MCP / `meta_skill_*` tools
- what output to expect

This is not a proposal document. It describes the current verified behavior only.

## Summary

`meta_skill` no longer treats a skill as only a single `SKILL.md` file during indexing.

The current implementation now supports a **skill package** model:

- `SKILL.md` remains the canonical skill entrypoint
- companion files in the same skill directory are indexed as package resources
- nested package files under the same skill root are persisted as resources
- package resource metadata is stored in SQLite
- package resources are preserved in the git archive
- exact package search can return the matching skill together with its package resources
- full load/show paths can return embedded resource content, not only `SKILL.md`

## What Counts as a Package

A skill package is a directory that contains `SKILL.md`.

Examples:

- `skills/bom-execute/`
- `skills/visual-explainer/`

Resources under that package root are scanned and persisted.

Examples for `bom-execute`:

- `SKILL.md`
- `implementer-prompt.md`
- `spec-reviewer-prompt.md`
- `code-quality-reviewer-prompt.md`

Examples for `visual-explainer`:

- `SKILL.md`
- `commands/...`
- `references/...`
- `scripts/...`
- `templates/...`

## What the Indexer Stores

For each indexed skill package, the current system stores:

1. The canonical skill body from `SKILL.md`
2. A package manifest summary
3. A `bundle_hash` for the package
4. A `skill_resources` row for each discovered resource
5. Archived package resources under:

```text
.ms/archive/skills/by-id/<skill-id>/resources/
```

## Verified CLI Model

The CLI currently has three different roles:

1. `search`
2. `load`
3. `show`

They are separate commands.

### Important

`search` does **not** mean "search and expand in one command".

The correct workflow is:

1. search if needed
2. load or show for full content

## Recommended CLI Commands

### If you already know the skill id

Use this command:

```bash
ms load bom-execute --complete --output-format json
```

This is the recommended command for agents and automation when the skill id is already known.

Examples:

```bash
ms load visual-explainer --complete --output-format json
ms load bom-execute --complete --output-format json
```

### If you do not know the exact skill id yet

Step 1:

```bash
ms search "bom-execute" --output-format json
```

Step 2:

```bash
ms load bom-execute --complete --output-format json
```

### If you want a detailed record view

Use:

```bash
ms show bom-execute --full --output-format json
```

## Exact Search Behavior

The current search path has special exact-package behavior for exact skill/package identifiers.

Verified examples:

```bash
ms search "bom-execute"
ms search "bom:execute"
ms search "visual-explainer"
```

For exact package-style queries, the current system returns:

- one exact result
- `search_type: "exact"` in JSON mode
- `package_resources` metadata in the search result

### Example

```bash
ms --output-format json search "bom-execute" --limit 5
```

Expected behavior:

- result count is `1`
- result id is `bom-execute`
- result name is `bom:execute`
- `package_resources` contains the four package files

## Full Load Behavior

`load --complete` is the main command for full machine-readable skill retrieval.

### Command

```bash
ms --output-format json load bom-execute --complete
```

### Current output includes

- `skill_id`
- `name`
- `content` (main disclosed body)
- `frontmatter`
- `package_manifest`
- `package_resources`

Each `package_resources` entry now includes:

- `relative_path`
- `resource_type`
- `size_bytes`
- `content_hash`
- `content`

The `content` field contains:

- `uri`
- `mimeType`
- `text` for UTF-8 / text resources
- or `blob` for binary resources
- or `error` if the archived resource cannot be read

### Example: read one companion file

```bash
ms --output-format json load bom-execute --complete | jq -r '.data.package_resources[] | select(.relative_path=="spec-reviewer-prompt.md") | .content.text'
```

## Full Show Behavior

`show --full` also exposes package resource content in JSON mode.

### Command

```bash
ms --output-format json show bom-execute --full
```

### Current output includes

- main skill record fields
- `body`
- `package_manifest`
- `package_resources`

Like `load --complete`, each resource entry includes embedded `content`.

### Example: read one companion file

```bash
ms --output-format json show bom-execute --full | jq -r '.skill.package_resources[] | select(.relative_path=="spec-reviewer-prompt.md") | .content.text'
```

## MCP and Agent Usage

For agents using MCP / `meta_skill_*` tools, the recommended full-content call is:

```text
meta_skill_load(skill="bom-execute", full=true)
```

This is the intended agent-facing equivalent of:

```bash
ms load bom-execute --complete --output-format json
```

### Agent recommendation

If the agent already knows the exact skill id, it should skip search and directly load full content.

Use:

```text
meta_skill_load(skill="bom-execute", full=true)
```

or:

```text
meta_skill_show(skill="bom-execute", full=true)
```

### Search for agents

If the skill id is not known yet, use search first:

```text
meta_skill_search(query="bom-execute")
```

Then load the exact skill:

```text
meta_skill_load(skill="bom-execute", full=true)
```

## MCP Resource Behavior

The current MCP implementation supports:

- `resources/list`
- `resources/read`

It also embeds package resources directly into `load(full=true)` and `show(full=true)` responses.

This means the agent has two valid ways to access package files:

1. load/show full response with embedded resource content
2. MCP resource read by URI

## Verified Real-World Example: `bom-execute`

Verified package resources:

- `SKILL.md`
- `code-quality-reviewer-prompt.md`
- `implementer-prompt.md`
- `spec-reviewer-prompt.md`

Verified commands:

```bash
ms --output-format json search "bom-execute" --limit 5
ms --output-format json search "bom:execute" --limit 5
ms --output-format json load bom-execute --complete
ms --output-format json show bom-execute --full
```

Verified result:

- exact hit is returned for the package
- package resources are listed
- companion file content is embedded in full load/show output

## Verified Real-World Example: `visual-explainer`

Verified package resources include:

- `SKILL.md`
- `commands/*`
- `references/*`
- `scripts/*`
- `templates/*`

Verified commands:

```bash
ms --output-format json search "visual-explainer" --limit 5
ms --output-format json load visual-explainer --complete
ms --output-format json show visual-explainer --full
```

Verified result:

- exact hit is returned
- package resources are included in search metadata
- full load/show include embedded content for text resources

## The One Command To Remember

If you want one command for full agent-readable skill content, use:

```bash
ms load <skill-id> --complete --output-format json
```

Example:

```bash
ms load bom-execute --complete --output-format json
```

## Common Mistake

This is wrong:

```bash
ms search "bom-execute" load --complete
```

Why it is wrong:

- `search` is one command
- `load` is another command
- they are not chained that way

Use this instead:

```bash
ms search "bom-execute"
ms load bom-execute --complete --output-format json
```

Or, if you already know the skill id:

```bash
ms load bom-execute --complete --output-format json
```

## Output Shape Notes

### Search JSON

Current exact result shape includes:

- `search_type`
- `count`
- `results[*].id`
- `results[*].name`
- `results[*].package_resources`

### Load JSON

Current full load JSON includes:

- `data.content`
- `data.package_manifest`
- `data.package_resources[*].content`

### Show JSON

Current full show JSON includes:

- `skill.body`
- `skill.package_manifest`
- `skill.package_resources[*].content`

## Operational Note

If CLI behavior and agent tool behavior appear inconsistent, verify that the running MCP server has been restarted against the current installed build.

The CLI and the MCP tool layer must both be using the updated runtime for `full=true` package resource content to appear consistently.

## Final Recommendation

For non-interactive automation and coding agents, use this as the default full-content command:

```bash
ms load <skill-id> --complete --output-format json
```

For example:

```bash
ms load bom-execute --complete --output-format json
```

This is the simplest supported way to retrieve:

- the main skill body
- package manifest information
- companion resource metadata
- embedded companion resource content
