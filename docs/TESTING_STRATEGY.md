# Testing Strategy for meta_skill (ms)

## Purpose

Define a **no-mocks, real-world** testing philosophy for ms that exercises real
code paths (SQLite, filesystem, Git archives) with deterministic fixtures and
observability-first logs. This is the cross-cutting policy that guides all test
beads and CI enforcement.

## Core Principles

1. **No mocks for core logic**
   - Use real parsers, real SQLite files, and real Git repositories.
   - Stubs are only acceptable at system boundaries (e.g., network calls), and
     must be documented with a rationale.
2. **Determinism first**
   - Tests must be repeatable across machines and time.
   - Use fixed seeds for randomized tests, controlled clocks, and isolated temp
     directories.
3. **Observability**
   - Every test emits structured logs with inputs, outputs, duration, and error
     context.
4. **Coverage-first**
   - Core modules must meet coverage targets before feature expansion.

## Test Types (and Ownership Beads)

- **Unit tests** (`meta_skill-7t2`)
  - Table-driven tests for pure logic.
  - Property-based tests for invariants and idempotence.
- **Integration tests** (`meta_skill-9pr`)
  - Temp directories + on-disk SQLite + Git archive fixtures.
  - Validate the dual persistence behavior and migration safety.
- **E2E CLI tests** (`meta_skill-2kd`)
  - Full CLI flows: `init → index → search → load → build`.
  - Validate exit codes, JSON (robot) output, and human formatting.
- **Snapshot tests** (`meta_skill-wnk`)
  - Stable output validation for JSON and human-readable output.
  - Snapshots must be deterministic (no timestamps unless normalized).
- **Benchmarks** (`meta_skill-ftb`)
  - Search, pack, and suggest performance.
  - Use fixed datasets and document baseline numbers.
- **Skill tests** (`meta_skill-x7k`)
  - Validate skill compilation, packing, and coverage constraints.

## Required Logging Standard

All tests must emit:

- Test name + timestamp
- Inputs + environment (paths, flags, seeds)
- Expected vs actual output
- Duration per test
- Failure context (stack trace, stderr capture)

Prefer structured logs (JSON) where possible.

## Determinism Guardrails

- Fixed RNG seeds for property-based tests.
- Controlled clocks or time-freezing helpers for time-sensitive logic.
- Temp directories per test case; no shared mutable state.
- Avoid network I/O in tests unless explicitly required and documented.

## CI Integration Requirements

CI must run and **gate** on:

- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo test` (or `cargo nextest` if adopted)
- **UBS** scan on changed files (machine-readable output)
- `cargo audit` for supply-chain security
- Coverage report published as build artifact (target ≥ 80% for core modules)

Additionally:

- JUnit/TAP output for CI dashboards
- Failing tests block release

## Acceptance Criteria (for this strategy bead)

- Testing philosophy is documented and referenced by child test beads.
- All required test suites are defined with clear ownership.
- CI gates defined with UBS and audit requirements.
- Determinism and logging standards are explicit.

## Notes

This document is the policy layer. Implementation details are handled in the
child beads and the CI/CD bead.
