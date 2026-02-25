//! Test runner for skill tests.
//!
//! Executes test definitions and collects results.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::definition::{Requirement, SkipCondition, TestDefinition};
use super::steps::StepExecutor;
use crate::app::AppContext;
use crate::error::{MsError, Result};

/// Options for controlling test execution.
#[derive(Debug, Clone, Default)]
pub struct TestOptions {
    /// Show verbose output during test execution.
    pub verbose: bool,

    /// Stop on first failure.
    pub fail_fast: bool,

    /// Run only this specific test by name.
    pub test_name: Option<String>,

    /// Only run tests with these tags.
    pub include_tags: Vec<String>,

    /// Skip tests with these tags.
    pub exclude_tags: Vec<String>,

    /// Override default test timeout.
    pub timeout_override: Option<Duration>,
}

/// Status of a test execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestStatus {
    /// Test passed successfully.
    Passed,
    /// Test failed.
    Failed,
    /// Test was skipped.
    Skipped,
    /// Test timed out.
    Timeout,
}

/// Result of running a single test.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    /// Test name.
    pub name: String,

    /// Test status.
    pub status: TestStatus,

    /// Duration in milliseconds.
    pub duration_ms: u64,

    /// Failure messages (if any).
    #[serde(default)]
    pub failures: Vec<String>,
}

/// Report for all tests run against a skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillTestReport {
    /// Skill ID.
    pub skill_id: String,

    /// Number of tests executed.
    pub tests_run: usize,

    /// Number of tests passed.
    pub passed: usize,

    /// Number of tests failed.
    pub failed: usize,

    /// Number of tests skipped.
    pub skipped: usize,

    /// Total duration in milliseconds.
    pub duration_ms: u64,

    /// Individual test results.
    pub results: Vec<TestResult>,
}

impl SkillTestReport {
    /// Returns true if all tests passed.
    #[must_use]
    pub const fn success(&self) -> bool {
        self.failed == 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_serialization() {
        let json = serde_json::to_string(&TestStatus::Passed).unwrap();
        assert_eq!(json, "\"passed\"");
        let json = serde_json::to_string(&TestStatus::Failed).unwrap();
        assert_eq!(json, "\"failed\"");
        let json = serde_json::to_string(&TestStatus::Skipped).unwrap();
        assert_eq!(json, "\"skipped\"");
        let json = serde_json::to_string(&TestStatus::Timeout).unwrap();
        assert_eq!(json, "\"timeout\"");
    }

    #[test]
    fn test_status_deserialization() {
        let s: TestStatus = serde_json::from_str("\"passed\"").unwrap();
        assert_eq!(s, TestStatus::Passed);
        let s: TestStatus = serde_json::from_str("\"failed\"").unwrap();
        assert_eq!(s, TestStatus::Failed);
    }

    #[test]
    fn test_report_success_all_passed() {
        let report = SkillTestReport {
            skill_id: "test-skill".to_string(),
            tests_run: 3,
            passed: 3,
            failed: 0,
            skipped: 0,
            duration_ms: 100,
            results: vec![],
        };
        assert!(report.success());
    }

    #[test]
    fn test_report_success_with_failures() {
        let report = SkillTestReport {
            skill_id: "test-skill".to_string(),
            tests_run: 3,
            passed: 2,
            failed: 1,
            skipped: 0,
            duration_ms: 200,
            results: vec![],
        };
        assert!(!report.success());
    }

    #[test]
    fn test_report_success_with_skipped_only() {
        let report = SkillTestReport {
            skill_id: "test-skill".to_string(),
            tests_run: 2,
            passed: 0,
            failed: 0,
            skipped: 2,
            duration_ms: 0,
            results: vec![],
        };
        assert!(report.success());
    }

    #[test]
    fn test_result_serialization_roundtrip() {
        let result = TestResult {
            name: "my test".to_string(),
            status: TestStatus::Failed,
            duration_ms: 42,
            failures: vec!["exit_code mismatch".to_string()],
        };
        let json = serde_json::to_string(&result).unwrap();
        let parsed: TestResult = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "my test");
        assert_eq!(parsed.status, TestStatus::Failed);
        assert_eq!(parsed.duration_ms, 42);
        assert_eq!(parsed.failures.len(), 1);
    }

    #[test]
    fn test_options_default() {
        let opts = TestOptions::default();
        assert!(!opts.verbose);
        assert!(!opts.fail_fast);
        assert!(opts.test_name.is_none());
        assert!(opts.include_tags.is_empty());
        assert!(opts.exclude_tags.is_empty());
        assert!(opts.timeout_override.is_none());
    }

    #[test]
    fn test_report_serialization() {
        let report = SkillTestReport {
            skill_id: "my-skill".to_string(),
            tests_run: 1,
            passed: 1,
            failed: 0,
            skipped: 0,
            duration_ms: 50,
            results: vec![TestResult {
                name: "basic".to_string(),
                status: TestStatus::Passed,
                duration_ms: 50,
                failures: vec![],
            }],
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"skill_id\":\"my-skill\""));
        assert!(json.contains("\"passed\":1"));
    }
}

/// Runner for skill tests.
pub struct SkillTestRunner<'a> {
    ctx: &'a AppContext,
    options: TestOptions,
}

impl<'a> SkillTestRunner<'a> {
    /// Create a new test runner.
    #[must_use]
    pub const fn new(ctx: &'a AppContext, options: TestOptions) -> Self {
        Self { ctx, options }
    }

    /// Run tests for all skills.
    pub fn run_all(&self) -> Result<Vec<SkillTestReport>> {
        let skills = self.discover_skills_with_tests()?;
        let mut reports = Vec::new();

        for skill_id in skills {
            let report = self.run_for_skill(&skill_id)?;
            let failed = report.failed > 0;
            reports.push(report);

            if self.options.fail_fast && failed {
                break;
            }
        }

        Ok(reports)
    }

    /// Run tests for a specific skill.
    pub fn run_for_skill(&self, skill_id: &str) -> Result<SkillTestReport> {
        let start = Instant::now();
        let tests = self.discover_tests(skill_id)?;

        let mut results = Vec::new();
        let mut passed = 0;
        let mut failed = 0;
        let mut skipped = 0;

        for test in tests {
            // Filter by test name if specified
            if let Some(ref name) = self.options.test_name {
                if test.name != *name {
                    continue;
                }
            }

            // Filter by tags
            if !self.should_run_test(&test) {
                skipped += 1;
                results.push(TestResult {
                    name: test.name.clone(),
                    status: TestStatus::Skipped,
                    duration_ms: 0,
                    failures: vec!["Skipped by tag filter".to_string()],
                });
                continue;
            }

            // Check skip conditions
            if self.should_skip(&test) {
                skipped += 1;
                results.push(TestResult {
                    name: test.name.clone(),
                    status: TestStatus::Skipped,
                    duration_ms: 0,
                    failures: vec!["Skipped by skip_if condition".to_string()],
                });
                continue;
            }

            // Check requirements
            if let Some(missing) = self.check_requirements(&test) {
                skipped += 1;
                results.push(TestResult {
                    name: test.name.clone(),
                    status: TestStatus::Skipped,
                    duration_ms: 0,
                    failures: vec![format!("Missing requirement: {missing}")],
                });
                continue;
            }

            // Run the test
            let result = self.run_test(&test)?;

            match result.status {
                TestStatus::Passed => passed += 1,
                TestStatus::Failed | TestStatus::Timeout => failed += 1,
                TestStatus::Skipped => skipped += 1,
            }

            let test_failed =
                result.status == TestStatus::Failed || result.status == TestStatus::Timeout;
            results.push(result);

            if self.options.fail_fast && test_failed {
                break;
            }
        }

        let duration = start.elapsed();

        Ok(SkillTestReport {
            skill_id: skill_id.to_string(),
            tests_run: results.len(),
            passed,
            failed,
            skipped,
            duration_ms: duration.as_millis() as u64,
            results,
        })
    }

    /// Discover all skills that have tests.
    fn discover_skills_with_tests(&self) -> Result<Vec<String>> {
        let mut skills = Vec::new();

        // Look in the skills archive for directories with tests/
        let archive_root = self.ctx.git.root();
        let skills_dir = archive_root.join("skills");

        if skills_dir.exists() {
            self.scan_for_tests(&skills_dir, &mut skills)?;
        }

        Ok(skills)
    }

    fn scan_for_tests(&self, dir: &PathBuf, skills: &mut Vec<String>) -> Result<()> {
        if !dir.is_dir() {
            return Ok(());
        }

        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.is_dir() {
                // Check if this directory has a tests/ subdirectory
                let tests_dir = path.join("tests");
                if tests_dir.exists() && tests_dir.is_dir() {
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        skills.push(name.to_string());
                    }
                }
                // Recurse into nested directories (skip tests/ itself)
                if path.file_name().and_then(|n| n.to_str()) != Some("tests") {
                    self.scan_for_tests(&path, skills)?;
                }
            }
        }

        Ok(())
    }

    /// Discover tests for a skill.
    fn discover_tests(&self, skill_id: &str) -> Result<Vec<TestDefinition>> {
        let skill_path = self
            .ctx
            .git
            .skill_path(skill_id)
            .ok_or_else(|| MsError::SkillNotFound(format!("Skill not found: {skill_id}")))?;

        let tests_dir = skill_path.join("tests");
        if !tests_dir.exists() {
            return Ok(Vec::new());
        }

        let mut tests = Vec::new();

        for entry in std::fs::read_dir(&tests_dir)? {
            let entry = entry?;
            let path = entry.path();

            if path
                .extension()
                .is_some_and(|ext| ext == "yaml" || ext == "yml")
            {
                let content = std::fs::read_to_string(&path).map_err(|e| {
                    MsError::Config(format!("Failed to read {}: {e}", path.display()))
                })?;

                let test: TestDefinition = serde_yaml::from_str(&content).map_err(|e| {
                    MsError::ValidationFailed(format!(
                        "Failed to parse test {}: {e}",
                        path.display()
                    ))
                })?;

                tests.push(test);
            }
        }

        Ok(tests)
    }

    /// Check if a test should be run based on tag filters.
    fn should_run_test(&self, test: &TestDefinition) -> bool {
        // If include tags specified, test must have at least one
        if !self.options.include_tags.is_empty() {
            let has_include = test
                .tags
                .iter()
                .any(|t| self.options.include_tags.contains(t));
            if !has_include {
                return false;
            }
        }

        // If exclude tags specified, test must not have any
        if !self.options.exclude_tags.is_empty() {
            let has_exclude = test
                .tags
                .iter()
                .any(|t| self.options.exclude_tags.contains(t));
            if has_exclude {
                return false;
            }
        }

        true
    }

    /// Check skip conditions.
    fn should_skip(&self, test: &TestDefinition) -> bool {
        let conditions = match &test.skip_if {
            Some(c) => c,
            None => return false,
        };

        for condition in conditions {
            match condition {
                SkipCondition::Platform(platform) => {
                    let current = std::env::consts::OS;
                    if platform == current {
                        return true;
                    }
                }
                SkipCondition::CommandMissing(cmd) => {
                    if which::which(cmd).is_err() {
                        return true;
                    }
                }
                SkipCondition::FileMissing(path) => {
                    if !std::path::Path::new(path).exists() {
                        return true;
                    }
                }
                SkipCondition::EnvMissing(var) => {
                    if std::env::var(var).is_err() {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Check requirements and return the first missing one.
    fn check_requirements(&self, test: &TestDefinition) -> Option<String> {
        let requirements = match &test.requires {
            Some(r) => r,
            None => return None,
        };

        for req in requirements {
            match req {
                Requirement::Command(cmd) => {
                    if which::which(cmd).is_err() {
                        return Some(format!("command '{cmd}'"));
                    }
                }
                Requirement::Env(var) => {
                    if std::env::var(var).is_err() {
                        return Some(format!("environment variable '{var}'"));
                    }
                }
                Requirement::File(path) => {
                    if !std::path::Path::new(path).exists() {
                        return Some(format!("file '{path}'"));
                    }
                }
                Requirement::Platform(platform) => {
                    let current = std::env::consts::OS;
                    if current != platform {
                        return Some(format!("platform '{platform}' (current: {current})"));
                    }
                }
            }
        }

        None
    }

    /// Run a single test.
    fn run_test(&self, test: &TestDefinition) -> Result<TestResult> {
        let start = Instant::now();
        let timeout = self
            .options
            .timeout_override
            .or(test.timeout)
            .unwrap_or(Duration::from_secs(60));

        if self.options.verbose {
            eprintln!("[TEST] Running: {}", test.name);
        }

        let mut executor = StepExecutor::new(self.ctx, self.options.verbose);
        let mut failures = Vec::new();

        // Run setup steps
        if let Some(ref setup) = test.setup {
            for step in setup {
                if let Err(e) = executor.execute(step) {
                    failures.push(format!("Setup failed: {e}"));
                }
            }
        }

        // Run test steps (if setup succeeded)
        if failures.is_empty() {
            for step in &test.steps {
                // Check timeout
                if start.elapsed() > timeout {
                    failures.push("Test timed out".to_string());
                    break;
                }

                if let Err(e) = executor.execute(step) {
                    failures.push(e.to_string());
                    if self.options.fail_fast {
                        break;
                    }
                }
            }
        }

        // Always run cleanup
        if let Some(ref cleanup) = test.cleanup {
            for step in cleanup {
                if let Err(e) = executor.execute(step) {
                    if self.options.verbose {
                        eprintln!("[CLEANUP] Warning: {e}");
                    }
                }
            }
        }

        let duration = start.elapsed();
        let timed_out = duration > timeout && failures.iter().any(|f| f.contains("timed out"));

        let status = if timed_out {
            TestStatus::Timeout
        } else if failures.is_empty() {
            TestStatus::Passed
        } else {
            TestStatus::Failed
        };

        Ok(TestResult {
            name: test.name.clone(),
            status,
            duration_ms: duration.as_millis() as u64,
            failures,
        })
    }
}
