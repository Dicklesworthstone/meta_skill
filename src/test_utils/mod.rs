//! Shared test utilities for ms.

pub mod fixtures;
pub mod logging;

#[cfg(test)]
pub mod arbitrary;

/// Table-driven test case structure.
#[derive(Debug, Clone)]
pub struct TestCase<I, E> {
    pub name: &'static str,
    pub input: I,
    pub expected: E,
    pub should_panic: bool,
}

/// Run table-driven tests with detailed logging.
pub fn run_table_tests<I, E, F>(cases: Vec<TestCase<I, E>>, test_fn: F)
where
    I: std::fmt::Debug + Clone + std::panic::RefUnwindSafe,
    E: std::fmt::Debug + PartialEq,
    F: Fn(I) -> E + std::panic::UnwindSafe + std::panic::RefUnwindSafe,
{
    for case in cases {
        let start = std::time::Instant::now();
        println!("[TEST] Running: {}", case.name);
        println!("[TEST] Input: {:?}", case.input);

        let result = std::panic::catch_unwind(|| test_fn(case.input.clone()));
        let elapsed = start.elapsed();

        if case.should_panic {
            assert!(result.is_err(), "Test '{}' expected panic", case.name);
            println!("[TEST] Expected panic occurred");
            println!("[TEST] PASSED: {} ({:?})\n", case.name, elapsed);
            continue;
        }

        let actual = result.unwrap_or_else(|_| {
            panic!("Test '{}' panicked unexpectedly", case.name);
        });

        println!("[TEST] Expected: {:?}", case.expected);
        println!("[TEST] Actual: {:?}", actual);
        println!("[TEST] Timing: {:?}", elapsed);

        assert_eq!(actual, case.expected, "Test '{}' failed", case.name);
        println!("[TEST] PASSED: {} ({:?})\n", case.name, elapsed);
    }
}
