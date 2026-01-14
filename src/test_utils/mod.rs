//! Shared test utilities for ms.

pub mod fixtures;
pub mod logging;

#[cfg(test)]
pub mod arbitrary;

#[cfg(test)]
pub mod mock_server;

/// Table-driven test case structure.
#[derive(Debug, Clone)]
pub struct TestCase<I, E> {
    pub name: &'static str,
    pub input: I,
    pub expected: E,
    pub should_panic: bool,
}

/// Run table-driven tests with detailed logging.
pub fn run_table_tests<I, E, F>(cases: Vec<TestCase<I, E>>, test_fn: F) -> Result<(), String>
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
            if !result.is_err() {
                return Err(format!("Test '{}' expected panic", case.name));
            }
            println!("[TEST] Expected panic occurred");
            println!("[TEST] PASSED: {} ({:?})\n", case.name, elapsed);
            continue;
        }

        let actual = match result {
            Ok(value) => value,
            Err(_) => {
                return Err(format!("Test '{}' panicked unexpectedly", case.name));
            }
        };

        println!("[TEST] Expected: {:?}", case.expected);
        println!("[TEST] Actual: {:?}", actual);
        println!("[TEST] Timing: {:?}", elapsed);

        if actual != case.expected {
            return Err(format!(
                "Test '{}' failed: expected {:?}, got {:?}",
                case.name, case.expected, actual
            ));
        }
        println!("[TEST] PASSED: {} ({:?})\n", case.name, elapsed);
    }
    Ok(())
}
