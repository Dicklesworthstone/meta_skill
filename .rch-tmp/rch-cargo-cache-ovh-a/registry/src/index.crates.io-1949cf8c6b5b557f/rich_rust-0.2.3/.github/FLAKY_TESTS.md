# Flaky Tests Tracking

> This document tracks tests with non-deterministic behavior and the strategies used to handle them.

## Current Quarantine List

| Test | Status | Reason | Mitigation |
|------|--------|--------|------------|
| *None currently* | - | - | - |

## Categories of Flakiness

### 1. Timing-Sensitive Tests

Tests that depend on wall-clock time or sleep durations.

**Symptoms:**
- Fails on slow CI runners
- Passes locally but fails in CI
- Intermittent `elapsed > expected` failures

**Mitigations:**
- Use `FlakyConfig` with retries
- Increase timeout margins
- Use relative timing checks

### 2. Thread-Sensitive Tests

Tests involving concurrent operations.

**Symptoms:**
- Race condition failures
- Deadlock timeouts
- Inconsistent ordering assertions

**Mitigations:**
- Use `retry_test()` for known races
- Add synchronization barriers
- Use deterministic thread counts

### 3. Terminal/TTY Tests

Tests that depend on terminal state.

**Symptoms:**
- Fails when no TTY available
- Different behavior in CI vs local
- Platform-specific failures

**Mitigations:**
- Use `fake_terminal` harness
- Skip or adjust for CI environment
- Use `force_terminal(true)` in tests

### 4. Property Tests

Randomized tests that occasionally find edge cases.

**Symptoms:**
- Rare failures with specific seeds
- "Flaky" but actually finding real bugs
- Different results on each run

**Mitigations:**
- Log `PROPTEST_SEED` on failure
- Increase test case count locally
- Add regression tests for found bugs

## Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| `FLAKY_TEST_RETRIES` | 2 | Number of retry attempts for flaky tests |
| `FLAKY_TEST_DELAY_MS` | 100 | Delay between retries in milliseconds |
| `FLAKY_TEST_LOG` | unset | If set, log retry attempts |
| `PROPTEST_SEED` | random | Seed for property tests (for reproducibility) |

## Adding Tests to Quarantine

1. Create an issue describing the flakiness
2. Add the test to the table above with:
   - Test name (fully qualified)
   - Status: `investigating`, `mitigated`, `resolved`
   - Root cause if known
   - Current mitigation strategy
3. Update the test to use `known_flaky!` macro
4. Consider using `retry_test()` for temporary mitigation

## Removing Tests from Quarantine

1. Identify and fix the root cause
2. Remove the `known_flaky!` macro
3. Run the test 100+ times locally: `cargo test test_name -- --test-threads=1`
4. Monitor CI for 1 week
5. Update this document

## CI Configuration

The CI workflow uses these strategies for flaky tests:

1. **Retry on failure**: Tests use the `retry_test()` wrapper
2. **Artifact preservation**: Test logs are uploaded on failure
3. **Separate job for flaky detection**: Runs quarantined tests with extra retries

## Reporting New Flakiness

If you encounter a flaky test:

1. Note the exact test name and error message
2. Check if it's already in the quarantine list
3. If not, run the test 10+ times to confirm flakiness:
   ```bash
   for i in {1..10}; do cargo test --test test_file test_name -- --test-threads=1 || echo "FAILED on run $i"; done
   ```
4. Create an issue with:
   - Test name
   - Error message
   - Number of failures out of runs
   - Platform and environment details
5. Add to quarantine list while investigating

---

*Last updated: 2026-01-28*
