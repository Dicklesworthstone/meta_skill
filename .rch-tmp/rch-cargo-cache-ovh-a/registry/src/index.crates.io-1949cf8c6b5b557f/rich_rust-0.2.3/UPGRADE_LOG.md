# Dependency Upgrade Log

**Date:** 2026-01-18  |  **Project:** rich_rust  |  **Language:** Rust

## Summary
- **Updated:** 17  |  **Skipped:** 0  |  **Failed:** 0  |  **Needs attention:** 0

### Needs Attention
- (None)

## Dependencies to Update

| Dependency | Current | Latest | Type |
|------------|---------|--------|------|
| bitflags | 2 | 2.10.0 | minor |
| regex | 1 | 1.12.2 | minor |
| crossterm | 0.28 | 0.29.0 | minor |
| unicode-width | 0.2 | 0.2.2 | patch |
| lru | 0.12 | 0.16.3 | minor |
| num-rational | 0.4 | 0.4.2 | patch |
| once_cell | 1.19 | 1.21.3 | minor |
| syntect | 5 | 5.3.0 | minor |
| pulldown-cmark | 0.12 | 0.13.0 | minor |
| serde_json | 1.0 | 1.0.149 | patch |
| criterion | 0.5 | 0.8.1 | minor |
| insta | 1.40 | 1.46.1 | minor |
| tracing | 0.1 | 0.1.44 | patch |
| tracing-subscriber | 0.3 | 0.3.22 | patch |
| tracing-test | 0.2 | 0.2.5 | patch |
| test-log | 0.2 | 0.2.19 | patch |

## Updates

### crossterm: 0.28 → 0.29
- **Breaking changes researched:** Rustix now default (was libc), FileDesc lifetime, KeyEventState serialization
- **Impact on codebase:** None - only basic terminal ops used
- **Tests:** ✓ All passed

### lru: 0.12 → 0.16
- **Breaking changes researched:** `promote`/`demote` return bool (v0.15), MSRV raised to 1.70 (v0.14)
- **Impact on codebase:** None - no promote/demote usage, Rust version compatible
- **Tests:** ✓ All passed

### pulldown-cmark: 0.12 → 0.13
- **Breaking changes researched:** New Tag variants (Superscript, Subscript, DefinitionList*), WikiLinks feature
- **Impact on codebase:** None - wildcard matches handle new variants, specific Options used
- **Tests:** ✓ All passed

### criterion: 0.5 → 0.8
- **Breaking changes researched:** MSRV 1.80, `real_blackbox` removed, `criterion::black_box` deprecated
- **Impact on codebase:** Deprecation warnings - `black_box` should migrate to `std::hint::black_box()`
- **Tests:** ✓ All passed, benchmarks compile with warnings

### insta: 1.40 → 1.46
- **Breaking changes researched:** Minor updates, new assertion macros
- **Impact on codebase:** None
- **Tests:** ✓ All passed

### Batch Update (minor/patch)
- **bitflags:** 2 → 2.10
- **regex:** 1 → 1.12
- **unicode-width:** 0.2 → 0.2.2
- **num-rational:** 0.4 → 0.4.2
- **once_cell:** 1.19 → 1.21
- **syntect:** 5 → 5.3
- **tracing:** 0.1 → 0.1.44
- **tracing-subscriber:** 0.3 → 0.3.22
- **tracing-test:** 0.2 → 0.2.5
- **test-log:** 0.2 → 0.2.19
- **Tests:** ✓ All 424 lib tests passed

