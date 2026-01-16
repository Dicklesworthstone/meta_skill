# Skill Composition

This document describes how to use the `extends` and `includes` fields to build hierarchical and composable skill systems.

## Overview

`ms` supports two composition mechanisms:

| Mechanism | Purpose | Relationship |
|-----------|---------|--------------|
| `extends` | Single inheritance from a parent skill | One parent per skill |
| `includes` | Compose content from multiple skills | Many skills merged into sections |

**Resolution order:**
1. Resolve inheritance chain (`extends`)
2. Apply includes from other skills
3. Cache the resolved spec

## Inheritance with `extends`

Use `extends` to create specialized versions of base skills.

### Basic Example

```yaml
# skills/by-id/error-handling-base/SKILL.md
---
id: error-handling-base
name: Error Handling Base
description: Foundation for error handling patterns
tags: [error-handling, foundation]
---

# Error Handling Base

## Rules

- Always handle errors explicitly
- Use meaningful error messages
- Include context in error messages
```

```yaml
# skills/by-id/rust-error-handling/SKILL.md
---
id: rust-error-handling
name: Rust Error Handling
description: Rust-specific error handling patterns
tags: [error-handling, rust]
extends: error-handling-base
---

# Rust Error Handling

## Rules

- Use thiserror for library errors
- Use anyhow for application errors
- Use ? operator for error propagation
```

When `rust-error-handling` is resolved, it inherits all rules from `error-handling-base` and adds the Rust-specific rules.

### Inheritance Behavior

| Field | Behavior |
|-------|----------|
| `id`, `name`, `description` | Replaced by child |
| `tags` | Replaced if child has tags |
| Sections | Merged (child sections added to parent) |
| Blocks (rules, examples, etc.) | Appended to parent blocks by default |
| `extends` | Cleared after resolution |

### Replace Flags

By default, child blocks are appended to parent blocks. Use replace flags to override parent content:

```yaml
---
id: strict-error-handling
extends: error-handling-base
replace_rules: true    # Replace ALL parent rules
replace_examples: true # Replace ALL parent examples
replace_pitfalls: true # Replace ALL parent pitfalls
replace_checklist: true # Replace ALL parent checklist items
---

# Strict Error Handling

## Rules

- These rules REPLACE the parent rules entirely
```

### Deep Inheritance

Skills can inherit through multiple levels:

```
error-handling-base
    └── rust-error-handling
            └── advanced-rust-error-handling
```

**Warning:** Inheritance chains deeper than 5 levels generate a warning. Deep chains make skills harder to understand and maintain.

### Cycle Detection

Cycles in inheritance are detected and rejected:

```yaml
# This creates a cycle and will fail
---
id: skill-a
extends: skill-b
---

---
id: skill-b
extends: skill-a
---
```

Error: `CyclicInheritance { skill_id: "skill-a", cycle: ["skill-a", "skill-b", "skill-a"] }`

## Composition with `includes`

Use `includes` to compose content from multiple skills without inheritance relationships.

### Basic Example

```yaml
---
id: rust-complete
name: Complete Rust Development
description: Comprehensive Rust skill combining multiple patterns
tags: [rust, complete]
includes:
  - skill: rust-error-handling
    into: rules
  - skill: testing-patterns
    into: checklist
  - skill: logging-patterns
    into: rules
    prefix: "[Logging] "
---

# Complete Rust Development

## Rules

- Follow Rust idioms and conventions
- Use clippy for linting
```

### Include Fields

| Field | Required | Description |
|-------|----------|-------------|
| `skill` | Yes | ID of the skill to include content from |
| `into` | Yes | Target section to merge content into |
| `prefix` | No | Text prefix added to each included item |
| `sections` | No | Filter to specific sections from source |
| `position` | No | `prepend` or `append` (default: `append`) |

### Include Targets

| Target | Merges | Into Section |
|--------|--------|--------------|
| `rules` | Rule blocks | `rules` section |
| `examples` | Code blocks | `examples` section |
| `pitfalls` | Pitfall blocks | `pitfalls` section |
| `checklist` | Checklist blocks | `checklist` section |
| `context` | Text blocks | `context` section |

### Position Control

```yaml
includes:
  - skill: safety-rules
    into: rules
    position: prepend  # Safety rules come first
  - skill: extra-rules
    into: rules
    position: append   # Extra rules come after existing
```

### Filtering by Section

Include only specific sections from a skill:

```yaml
includes:
  - skill: big-skill
    into: rules
    sections: ["critical-rules", "must-haves"]
```

### Prefix for Clarity

Add prefixes to distinguish included content:

```yaml
includes:
  - skill: security-patterns
    into: rules
    prefix: "[Security] "
```

Results in rules like:
- `[Security] Validate all user input`
- `[Security] Use parameterized queries`

## Combining extends and includes

A skill can use both inheritance and composition:

```yaml
---
id: enterprise-rust
extends: rust-error-handling       # Inherit error patterns
includes:
  - skill: security-patterns       # Add security rules
    into: rules
  - skill: testing-patterns        # Add testing checklist
    into: checklist
---
```

**Resolution order:**
1. Resolve `extends` chain first
2. Apply `includes` to the inheritance-resolved spec
3. Clear both fields from the final resolved spec

## Indexing Composed Skills

When running `ms index`, composed skills are resolved and cached:

```bash
# Index all skills, resolving composition
ms index

# Force re-resolution of all skills
ms index --force
```

The index stores:
- The resolved skill spec
- Inheritance chain for cache invalidation
- List of included skills

### Cache Invalidation

When a parent or included skill changes:
1. Its direct resolved cache is invalidated
2. All skills that depend on it are also invalidated
3. Re-indexing resolves them fresh

## Best Practices

### When to Use `extends`

- Creating language/framework-specific variants of base skills
- Specializing a general pattern for a specific use case
- Building skill "families" with shared foundations

### When to Use `includes`

- Combining orthogonal concerns (error handling + logging + testing)
- Creating "meta-skills" that bundle related capabilities
- Reusing specific sections without full inheritance

### Design Tips

1. **Keep inheritance shallow** - Aim for 2-3 levels maximum
2. **Use includes for composition** - When you need pieces from multiple skills
3. **Prefer includes over deep inheritance** - More flexible, easier to understand
4. **Use prefixes** - Help users understand where rules came from
5. **Document composition** - Note in the skill description what's inherited/included

## Common Patterns

### Base + Language Variants

```
error-handling-base
├── rust-error-handling
├── python-error-handling
└── typescript-error-handling
```

### Meta-Skill Bundle

```yaml
includes:
  - skill: error-handling
    into: rules
  - skill: logging
    into: rules
  - skill: testing
    into: checklist
  - skill: security
    into: pitfalls
```

### Progressive Enhancement

```
minimal-api
└── standard-api (adds logging)
    └── production-api (adds security, monitoring)
```

## Troubleshooting

### "Parent skill not found"

```
ParentSkillNotFound { parent_id: "base-skill", child_id: "child-skill" }
```

**Fix:** Ensure the parent skill is indexed before the child:
```bash
# Index base skills first
ms index skills/by-id/base-skill

# Then index dependent skills
ms index
```

### "Cyclic inheritance detected"

```
CyclicInheritance { skill_id: "skill-a", cycle: ["skill-a", "skill-b", "skill-a"] }
```

**Fix:** Review your `extends` and `includes` references. Break the cycle by:
- Removing one direction of the dependency
- Extracting common content into a shared base skill

### "Deep inheritance warning"

```
DeepInheritance { depth: 6, chain: [...] }
```

**Fix:** Consider flattening your hierarchy or using `includes` instead of deep inheritance.

### "Many includes warning"

```
ManyIncludes { target: Rules, count: 5, sources: [...] }
```

**Fix:** This is informational. If intentional, it's fine. Otherwise, consider consolidating some included skills.

### Included Content Not Appearing

1. Check the source skill has the expected block types
2. Verify `into` matches the block type (`rules` for Rule blocks, etc.)
3. Check `sections` filter isn't excluding the content
4. Run `ms show <skill-id> --resolved` to see the final resolved spec

### Changes Not Reflected After Edit

```bash
# Force re-index to clear cache
ms index --force
```

## Reference

### Inheritance Depth Limit

Maximum recommended depth: 5 levels

### Cycle Detection

- Detects cycles in `extends` chain
- Detects cycles involving both `extends` and `includes`
- Cycles are errors, not warnings

### Resolution Cache

Resolved specs are cached:
- In-memory LRU cache for fast repeated access
- SQLite backing for persistence across sessions
- Automatic invalidation when dependencies change
