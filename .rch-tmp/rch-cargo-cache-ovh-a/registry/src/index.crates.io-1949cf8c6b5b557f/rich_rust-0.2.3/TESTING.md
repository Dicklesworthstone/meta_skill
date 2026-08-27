# Testing Guidelines for rich_rust

> Living documentation for testing patterns, conventions, and best practices.

---

## Test Naming Conventions

### Unit Tests (in `src/*.rs`)

Unit tests live in a `#[cfg(test)] mod tests` block within each module.

**Pattern**: `test_{function_name}_{scenario}`

```rust
#[test]
fn test_parse_hex_color() { ... }

#[test]
fn test_parse_hex_color_lowercase() { ... }

#[test]
fn test_parse_hex_color_invalid_format() { ... }
```

### Integration Tests (in `tests/*.rs`)

**E2E tests**: `e2e_{feature}_{scenario}`
```rust
fn e2e_table_simple_2x2() { ... }
fn e2e_table_nested_tables() { ... }
```

**Regression tests**: `regression_{category}_{bug_description}`
```rust
fn regression_parsing_empty_markup_tag() { ... }
fn regression_layout_wide_char_overflow() { ... }
```

**Property tests**: `prop_{invariant_being_tested}`
```rust
fn prop_style_combination_is_associative() { ... }
fn prop_color_roundtrip_preserves_values() { ... }
```

**Fuzz tests**: Located in `tests/fuzz_*.rs`
```rust
fn fuzz_style_parser_no_panic() { ... }
fn fuzz_markup_parser_handles_arbitrary_input() { ... }
```

### Naming Standards Enforcement

**Required Prefixes:**

| Test Type | Required Prefix | File Location |
|-----------|-----------------|---------------|
| Unit tests | `test_` | `src/*.rs` (in `mod tests`) |
| E2E tests | `e2e_` | `tests/e2e_*.rs` |
| Property tests | `prop_` | `tests/property_tests.rs` |
| Fuzz tests | `fuzz_` | `tests/fuzz_*.rs` |
| Regression tests | `regression_` | `tests/regression_tests.rs` |
| Conformance tests | `test_` or `conformance_` | `tests/conformance_*.rs` |

**Validation Script:**

Run this to check naming compliance:

```bash
# Check for test functions missing standard prefixes
rg "#\[test\]" -A 1 tests/*.rs | rg "fn [a-z]" | \
  rg -v "fn (test_|e2e_|prop_|fuzz_|regression_|conformance_)" | \
  rg -v "fn (new|contents|write|flush|len|clear)" && \
  echo "Found non-compliant test names!" || \
  echo "All test names comply with standards"
```

**Common Anti-Patterns:**

| Anti-Pattern | Correct Pattern | Example |
|--------------|-----------------|---------|
| `fn check_something()` | `fn test_something()` | `test_color_parsing` |
| `fn verify_output()` | `fn e2e_output_verification()` | `e2e_table_renders_borders` |
| `fn it_should_work()` | `fn test_feature_works()` | `test_style_parse_bold` |
| `fn test1()` | `fn test_feature_scenario()` | `test_color_hex_uppercase` |

**Descriptive Scenarios:**

Good test names describe:
1. **What** is being tested (function/feature)
2. **When** or under what conditions
3. **Expected outcome** (for edge cases)

```rust
// Good examples:
fn test_style_parse_with_invalid_color_returns_none() { ... }
fn e2e_table_with_unicode_content_preserves_alignment() { ... }
fn regression_markup_empty_tag_no_panic() { ... }

// Poor examples:
fn test_style1() { ... }
fn test_it_works() { ... }
fn check_table() { ... }
```

---

## Test File Organization

```
tests/
├── common/             # Shared test utilities
│   └── mod.rs
├── e2e_*.rs            # End-to-end feature tests
├── property_tests.rs   # Property-based tests (proptest)
├── fuzz_*.rs           # Fuzz tests
├── regression_tests.rs # Bug regression tests
├── thread_safety.rs    # Concurrency and thread safety
├── mutex_poison_recovery.rs  # Poison handling tests
├── conformance_*.rs    # Python Rich compatibility
├── golden_test.rs      # Snapshot/golden tests
└── repro_*.rs          # Reproduction tests for specific bugs
```

### Test Categories (in file headers)

Each test file should have a module-level doc comment explaining:
1. What the tests cover
2. How to run them
3. Any special requirements

```rust
//! End-to-end tests for Table rendering.
//!
//! Run with: RUST_LOG=debug cargo test --test e2e_table -- --nocapture
```

---

## Fixture Organization

### Shared Buffer Pattern

For capturing Console output:

```rust
#[derive(Clone)]
struct SharedBuffer(Arc<Mutex<Vec<u8>>>);

impl SharedBuffer {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(Vec::new())))
    }

    fn contents(&self) -> String {
        String::from_utf8_lossy(&self.0.lock().unwrap()).to_string()
    }
}

impl Write for SharedBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.lock().unwrap().flush()
    }
}

fn make_test_console(buffer: SharedBuffer) -> Arc<Console> {
    Console::builder()
        .force_terminal(true)
        .markup(false)
        .file(Box::new(buffer))
        .build()
        .shared()
}
```

### Test Logging Infrastructure

Use the common logging helpers for detailed test output:

```rust
mod common;
use common::{init_test_logging, log_test_context, test_phase};

#[test]
fn my_test() {
    init_test_logging();
    test_phase!("setup");
    // ... test code
    log_test_context!("rendered output", &output);
}
```

---

## When to Use Snapshots

**Use snapshots for**:
- Complex visual output (tables, panels, trees)
- ANSI escape sequence verification
- Conformance with Python Rich

**Don't use snapshots for**:
- Simple assertions (`assert_eq!`)
- Dynamic or time-dependent output
- Tests that verify behavior, not output

### Snapshot Test Pattern

```rust
// In tests/golden_test.rs or tests/demo_showcase_snapshots.rs
#[test]
fn snapshot_table_with_borders() {
    let output = render_table_with_borders();
    insta::assert_snapshot!(output);
}
```

---

## Property Test Patterns

Use `proptest` for invariant verification. Located in `tests/property_tests.rs`.

### Custom Strategies

```rust
fn rgb_triplet() -> impl Strategy<Value = (u8, u8, u8)> {
    (any::<u8>(), any::<u8>(), any::<u8>())
}

fn random_style() -> impl Strategy<Value = Style> {
    // ... complex strategy
}
```

### Property Test Structure

```rust
proptest! {
    #[test]
    fn prop_style_parse_never_panics(s in ".*") {
        // Should never panic, even on invalid input
        let _ = Style::parse(&s);
    }

    #[test]
    fn prop_color_downgrade_preserves_validity(
        r in 0u8..=255u8,
        g in 0u8..=255u8,
        b in 0u8..=255u8
    ) {
        let color = Color::from_rgb(r, g, b);
        let downgraded = color.downgrade(ColorSystem::Standard);
        // Verify downgraded color is valid
        prop_assert!(!downgraded.get_ansi_codes(true).is_empty());
    }
}
```

---

## E2E Test Structure

### Standard E2E Test Pattern

```rust
#[test]
fn e2e_feature_scenario() {
    init_test_logging();
    tracing::info!("Starting E2E test: feature_scenario");

    // 1. Setup
    test_phase!("setup");
    let console = make_test_console(SharedBuffer::new());

    // 2. Action
    test_phase!("action");
    let output = do_something(&console);
    tracing::debug!(output = %output, "Action result");

    // 3. Verify
    test_phase!("verify");
    assert!(output.contains("expected"), "Missing expected content");
    assert!(!output.contains("error"), "Unexpected error");

    tracing::info!("E2E test passed: feature_scenario");
}
```

### Scenario Grouping

Group related tests with section comments:

```rust
// =============================================================================
// Scenario 1: Basic Usage
// =============================================================================

#[test]
fn e2e_table_simple_2x2() { ... }

#[test]
fn e2e_table_empty() { ... }

// =============================================================================
// Scenario 2: Edge Cases
// =============================================================================

#[test]
fn e2e_table_wide_content() { ... }
```

---

## Mock vs Real Guidance

### Prefer Real Components

rich_rust is an output library with minimal external dependencies. **Avoid mocks** in most cases.

**Use real components for**:
- Console (with captured output via SharedBuffer)
- Style parsing
- Rendering pipeline
- Color/ANSI code generation

### When Mocking is Acceptable

**Terminal detection**: Use `force_terminal(true)` to bypass TTY checks.

```rust
let console = Console::builder()
    .force_terminal(true)  // Pretend we have a TTY
    .width(80)             // Fixed width
    .height(24)            // Fixed height
    .color_system(ColorSystem::TrueColor)
    .build();
```

**Time-based tests**: Mock sleep durations for faster tests.

```rust
let options = LiveOptions {
    refresh_per_second: 100.0,  // Fast refresh for testing
    ..Default::default()
};
```

---

## Coverage Expectations

### Target Coverage

| Category | Target | Priority |
|----------|--------|----------|
| Core (Console, Style, Color) | 90%+ | High |
| Renderables | 80%+ | High |
| Sync/Threading | 95%+ | Critical |
| Interactive | 70%+ | Medium |
| Optional features | 75%+ | Medium |

### Measuring Coverage

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Generate coverage report
cargo tarpaulin --out Html --output-dir coverage/

# Coverage for specific files
cargo tarpaulin --files src/console.rs src/style.rs
```

### Coverage Exclusions

Exclude from coverage analysis:
- Generated code
- Debug-only paths (`#[cfg(debug_assertions)]`)
- Platform-specific code not testable in CI

---

## PR Testing Checklist

Before submitting a PR:

### Required

- [ ] `cargo test` passes all tests
- [ ] `cargo clippy --all-targets -- -D warnings` passes
- [ ] `cargo fmt --check` passes
- [ ] No new `unwrap()` in production code (use `lock_recover` for mutexes)

### For Feature Changes

- [ ] Unit tests added for new public APIs
- [ ] E2E test added for user-facing behavior
- [ ] Documentation updated

### For Bug Fixes

- [ ] Regression test added in `tests/regression_tests.rs`
- [ ] Test named with pattern: `regression_{category}_{bug_description}`
- [ ] Comment documenting what bug it prevents

### For Performance Changes

- [ ] Benchmark added or updated
- [ ] No regression in existing benchmarks
- [ ] Property test verifies correctness

---

## Running Tests

### Common Commands

```bash
# Run all tests
cargo test

# Run with output (see println!/tracing)
cargo test -- --nocapture

# Run specific test file
cargo test --test e2e_table

# Run tests matching name pattern
cargo test table

# Run tests with detailed logging
RUST_LOG=debug cargo test --test e2e_table -- --nocapture

# Run in parallel (default) or single-threaded
cargo test -- --test-threads=1
```

### CI Commands

```bash
# Full CI validation
cargo check --all-targets
cargo clippy --all-targets -- -D warnings
cargo test
cargo fmt --check
```

---

## Thread Safety Testing

For concurrent code, use these patterns from `tests/thread_safety.rs`:

```rust
#[test]
fn test_concurrent_access() {
    use std::sync::Arc;
    use std::thread;

    let shared = Arc::new(MyStruct::new());
    let handles: Vec<_> = (0..4)
        .map(|i| {
            let s = Arc::clone(&shared);
            thread::spawn(move || {
                s.do_something(i);
            })
        })
        .collect();

    for h in handles {
        h.join().expect("thread should not panic");
    }
}
```

### Mutex Poison Recovery Testing

See `tests/mutex_poison_recovery.rs` for patterns:

```rust
#[test]
fn test_recovery_after_poison() {
    let mutex = Mutex::new(42);

    // Poison the mutex
    let _ = panic::catch_unwind(AssertUnwindSafe(|| {
        let _guard = mutex.lock().unwrap();
        panic!("intentional");
    }));

    // Verify recovery works
    let guard = lock_recover(&mutex);
    assert_eq!(*guard, 42);
}
```

---

## Additional Resources

- **AGENTS.md**: Tooling and workflow guidelines
- **RICH_SPEC.md**: Python Rich behavior specification
- **FEATURE_PARITY.md**: Feature comparison matrix

---

*Last updated: 2026-01-28 by OrangeFinch (claude-code/opus-4.5)*
