# Safety Invariant Layer - DCG Integration Update

## Section Reference
Section 5.18 - Safety Invariant Layer

## Overview

**CRITICAL**: This feature integrates DCG (Destructive Command Guard) from `/data/projects/destructive_command_guard` as the primary safety enforcement layer. DCG is a battle-tested, high-performance hook system specifically designed to block destructive commands before they execute.

Rather than building custom command classification from scratch, ms leverages DCG's modular pack system, SIMD-accelerated filtering, and fail-open design.

## Why DCG (not custom implementation)

| Aspect | Custom Implementation | DCG |
|--------|----------------------|-----|
| **Maturity** | New, untested | Battle-tested, production-ready |
| **Performance** | Unknown | Sub-millisecond (SIMD-accelerated) |
| **Coverage** | Limited | 49+ security packs (git, db, k8s, cloud, etc.) |
| **AST scanning** | None | Heredoc/inline script analysis |
| **Context detection** | Basic | Smart (won't block data patterns) |
| **Error handling** | Build from scratch | Fail-open design |
| **Maintenance** | Must track new threats | Community-maintained packs |

## DCG Integration Architecture

```rust
/// DCG-based command safety layer
struct DcgSafetyLayer {
    /// Path to dcg binary
    dcg_path: PathBuf,
    /// Enabled packs (from dcg config)
    enabled_packs: Vec<String>,
    /// Custom allowlist for ms operations
    ms_allowlist: Vec<AllowlistEntry>,
    /// Audit log
    audit_log: AuditLog,
}

/// Integration with skill execution
impl DcgSafetyLayer {
    /// Check command before allowing skill to suggest it
    fn check_command(&self, cmd: &str) -> SafetyResult {
        // Call dcg check --json <cmd>
        // Parse result
        // Log to audit
    }

    /// Explain why command is blocked (wraps dcg explain)
    fn explain(&self, cmd: &str) -> Explanation {
        // Call dcg explain <cmd>
    }

    /// List all enabled packs
    fn packs(&self) -> Vec<PackInfo> {
        // Call dcg packs --verbose
    }
}

enum SafetyResult {
    /// Command is safe to suggest
    Safe,
    /// Command blocked by DCG
    Blocked {
        reason: String,
        pack: String,
        tip: Option<String>,
    },
    /// DCG not available, fail-open
    DcgUnavailable,
}
```

## DCG Packs Relevant to Skill Mining

Core packs (always enabled):
- `core.filesystem` - Protects against dangerous file deletions
- `core.git` - Protects against destructive git commands

Database packs (enable when mining DB-related skills):
- `database.postgresql`, `database.mysql`, `database.mongodb`
- `database.redis`, `database.sqlite`

Container packs (enable when mining container skills):
- `containers.docker`, `containers.compose`, `containers.podman`

Kubernetes packs:
- `kubernetes.kubectl`, `kubernetes.helm`, `kubernetes.kustomize`

Cloud packs:
- `cloud.aws`, `cloud.azure`, `cloud.gcp`

Infrastructure packs:
- `infrastructure.ansible`, `infrastructure.terraform`

## CLI Commands (leveraging DCG)

```bash
# Check command safety (wraps dcg)
ms safety check "git status"
ms safety check --json "npm install"

# Explain why blocked (wraps dcg explain)
ms safety explain "git push --force"

# List available packs (wraps dcg packs)
ms safety packs
ms safety packs --verbose

# Enable/disable packs for skill context
ms safety enable database.postgresql
ms safety disable cloud.aws

# Audit log
ms safety audit
ms safety audit --since 7d

# Verify DCG installation
ms safety status
```

## Tasks

1. [ ] Detect DCG installation and version
2. [ ] Implement DcgSafetyLayer wrapper
3. [ ] Map DCG packs to skill contexts (db skills → db packs)
4. [ ] Implement MandatorySafetySlice for packer integration
5. [ ] Build CLI commands that wrap DCG
6. [ ] Add audit logging for all safety checks
7. [ ] Integrate with skill extraction pipeline
8. [ ] Integrate with skill suggestion system
9. [ ] Document DCG installation requirements

## Testing Requirements

- DCG integration tests (check, explain, packs commands)
- Mandatory slice preservation in packer
- Fail-open behavior when DCG unavailable
- Audit log completeness
- CLI command tests

## Acceptance Criteria

- DCG detected and integrated
- All skill commands checked before suggestion
- Mandatory safety slices preserved in packing
- Audit log captures all checks
- CLI commands functional and well-documented
- Fail-open when DCG not installed (with warning)

## References

- DCG repository: /data/projects/destructive_command_guard
- DCG README: /data/projects/destructive_command_guard/README.md
- DCG SKILL.md: /data/projects/destructive_command_guard/SKILL.md
- Plan Section 5.18

Labels: [phase-4 safety invariants destructive dcg]
