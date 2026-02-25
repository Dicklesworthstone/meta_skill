//! Test file specifications and parsing

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::error::{MsError, Result};

/// A complete test definition (YAML spec)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestDefinition {
    /// Test name (required)
    pub name: String,

    /// What this test validates
    #[serde(default)]
    pub description: Option<String>,

    /// Skill ID to test (required)
    #[serde(default)]
    pub skill: Option<String>,

    /// Setup steps (run before test)
    #[serde(default)]
    pub setup: Option<Vec<TestStep>>,

    /// Main test steps (required)
    #[serde(default)]
    pub steps: Vec<TestStep>,

    /// Cleanup steps (run after test, even on failure)
    #[serde(default)]
    pub cleanup: Option<Vec<TestStep>>,

    /// Test timeout
    #[serde(default)]
    #[serde(with = "humantime_serde")]
    pub timeout: Option<Duration>,

    /// Tags for filtering
    #[serde(default)]
    pub tags: Vec<String>,

    /// Conditions to skip the test
    #[serde(default)]
    pub skip_if: Option<Vec<SkipCondition>>,

    /// System requirements
    #[serde(default)]
    pub requires: Option<Vec<Requirement>>,
}

/// Alias for backward compatibility
pub type TestSpec = TestDefinition;

/// A single test step
///
/// Uses untagged serde representation to support YAML format like:
/// ```yaml
/// - load_skill:
///     level: standard
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum TestStep {
    /// Load a skill
    LoadSkill { load_skill: LoadSkillStep },

    /// Run a shell command
    Run { run: RunStep },

    /// Assert conditions
    Assert { assert: AssertStep },

    /// Write a file
    WriteFile { write_file: WriteFileStep },

    /// Create a directory
    Mkdir { mkdir: MkdirStep },

    /// Remove a file or directory
    Remove { remove: RemoveStep },

    /// Copy a file
    Copy { copy: CopyStep },

    /// Sleep for a duration
    Sleep { sleep: SleepStep },

    /// Set a variable
    Set { set: SetStep },

    /// Conditional execution
    If {
        #[serde(rename = "if")]
        if_step: IfStep,
    },
}

/// Load a skill step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadSkillStep {
    /// Disclosure level
    #[serde(default = "default_level")]
    pub level: String,

    /// Token budget
    pub budget: Option<usize>,

    /// Suggestion context
    #[serde(default)]
    pub context: HashMap<String, serde_json::Value>,
}

fn default_level() -> String {
    "standard".to_string()
}

/// Run a command step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunStep {
    /// Command to run
    pub cmd: String,

    /// Working directory
    pub cwd: Option<String>,

    /// Environment variables
    #[serde(default)]
    pub env: HashMap<String, String>,

    /// Stdin input
    pub stdin: Option<String>,

    /// Command timeout
    #[serde(default)]
    #[serde(with = "humantime_serde")]
    pub timeout: Option<Duration>,
}

/// Assert conditions step
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssertStep {
    /// Expected exit code (from previous run)
    pub exit_code: Option<i32>,

    /// stdout should contain this text
    pub stdout_contains: Option<String>,

    /// stdout should not contain this text
    pub stdout_not_contains: Option<String>,

    /// stderr should be empty
    pub stderr_empty: Option<bool>,

    /// File should exist
    pub file_exists: Option<String>,

    /// File should contain text
    pub file_contains: Option<FileContains>,

    /// Skill should be loaded
    pub skill_loaded: Option<bool>,

    /// Sections that should be present
    pub sections_present: Option<Vec<String>>,

    /// Tokens used should be less than
    pub tokens_used_lt: Option<usize>,

    /// Retrieval rank should be at most
    pub retrieval_rank_le: Option<usize>,
}

/// File contains assertion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContains {
    pub path: String,
    pub text: String,
}

/// Type alias for backward compatibility with steps.rs
pub type Assertions = AssertStep;

/// Condition for if-step evaluation (struct-based for combining multiple checks)
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Condition {
    /// Platform check (e.g., "linux", "macos", "windows")
    pub platform: Option<String>,
    /// Environment variable must exist
    pub env_exists: Option<String>,
    /// Environment variables must equal specific values
    #[serde(default)]
    pub env_equals: Option<std::collections::HashMap<String, String>>,
}

/// Write a file step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WriteFileStep {
    pub path: String,
    pub content: String,
}

/// Create directory step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MkdirStep {
    pub path: String,
    #[serde(default)]
    pub parents: bool,
}

/// Remove file/directory step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemoveStep {
    pub path: String,
    #[serde(default)]
    pub recursive: bool,
}

/// Copy file step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CopyStep {
    pub from: String,
    pub to: String,
}

/// Sleep step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SleepStep {
    #[serde(with = "humantime_serde")]
    pub duration: Duration,
}

/// Set variable step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SetStep {
    pub name: String,
    pub value: String,
}

/// Conditional execution step
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IfStep {
    pub condition: Condition,
    #[serde(rename = "then")]
    pub then_steps: Vec<TestStep>,
    #[serde(rename = "else", default)]
    pub else_steps: Option<Vec<TestStep>>,
}

/// Conditions for skipping tests
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkipCondition {
    /// Skip on specific platform
    Platform(String),
    /// Skip if command not found
    CommandMissing(String),
    /// Skip if file doesn't exist
    FileMissing(String),
    /// Skip if environment variable not set
    EnvMissing(String),
}

/// System requirements
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Requirement {
    /// Requires a command to be available
    Command(String),
    /// Requires a file to exist
    File(String),
    /// Requires an environment variable
    Env(String),
    /// Requires a specific platform
    Platform(String),
}

impl TestSpec {
    /// Parse a test spec from YAML
    pub fn from_yaml(content: &str) -> Result<Self> {
        serde_yaml::from_str(content)
            .map_err(|err| MsError::ValidationFailed(format!("invalid test YAML: {err}")))
    }

    /// Load a test spec from a file
    pub fn from_file(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path).map_err(|err| {
            MsError::Io(std::io::Error::new(
                err.kind(),
                format!("read test file {}: {err}", path.display()),
            ))
        })?;
        Self::from_yaml(&content)
    }

    /// Check if this test should be skipped based on conditions
    #[must_use]
    pub fn should_skip(&self) -> Option<String> {
        let conditions = self.skip_if.as_ref()?;
        for condition in conditions {
            match condition {
                SkipCondition::Platform(p) => {
                    let current = std::env::consts::OS;
                    if current == p {
                        return Some(format!("skip on platform: {p}"));
                    }
                }
                SkipCondition::CommandMissing(cmd) => {
                    if which::which(cmd).is_err() {
                        return Some(format!("command not found: {cmd}"));
                    }
                }
                SkipCondition::FileMissing(f) => {
                    if !std::path::Path::new(f).exists() {
                        return Some(format!("file missing: {f}"));
                    }
                }
                SkipCondition::EnvMissing(var) => {
                    if std::env::var(var).is_err() {
                        return Some(format!("env var missing: {var}"));
                    }
                }
            }
        }
        None
    }

    /// Check if requirements are met
    pub fn check_requirements(&self) -> Result<()> {
        let requirements = match &self.requires {
            Some(r) => r,
            None => return Ok(()),
        };
        for req in requirements {
            match req {
                Requirement::Command(cmd) => {
                    if which::which(cmd).is_err() {
                        return Err(MsError::ValidationFailed(format!(
                            "required command not found: {cmd}"
                        )));
                    }
                }
                Requirement::File(path) => {
                    if !std::path::Path::new(path).exists() {
                        return Err(MsError::ValidationFailed(format!(
                            "required file not found: {path}"
                        )));
                    }
                }
                Requirement::Env(var) => {
                    if std::env::var(var).is_err() {
                        return Err(MsError::ValidationFailed(format!(
                            "required env var not set: {var}"
                        )));
                    }
                }
                Requirement::Platform(platform) => {
                    let current = std::env::consts::OS;
                    if current != platform {
                        return Err(MsError::ValidationFailed(format!(
                            "requires platform {platform}, got {current}"
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    /// Check if test has a specific tag
    #[must_use]
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|t| t.eq_ignore_ascii_case(tag))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_TEST: &str = r#"
name: "Basic load test"
description: "Test that skill loads correctly"
skill: rust-error-handling
timeout: 30s
tags: [smoke, load]

setup:
  - mkdir:
      path: "/tmp/test-workspace"
      parents: true

steps:
  - load_skill:
      level: standard
  - run:
      cmd: "echo hello"
  - assert:
      exit_code: 0
      stdout_contains: "hello"

cleanup:
  - remove:
      path: "/tmp/test-workspace"
      recursive: true
"#;

    #[test]
    fn parse_test_spec() {
        let spec = TestSpec::from_yaml(SAMPLE_TEST).unwrap();
        assert_eq!(spec.name, "Basic load test");
        assert_eq!(spec.skill, Some("rust-error-handling".to_string()));
        assert_eq!(spec.timeout, Some(Duration::from_secs(30)));
        assert!(spec.has_tag("smoke"));
        assert!(spec.has_tag("load"));
        assert!(!spec.has_tag("integration"));
        assert_eq!(spec.setup.as_ref().map(|s| s.len()), Some(1));
        assert_eq!(spec.steps.len(), 3);
        assert_eq!(spec.cleanup.as_ref().map(|c| c.len()), Some(1));
    }

    #[test]
    fn parse_load_skill_step() {
        let yaml = r#"
name: test
skill: test-skill
steps:
  - load_skill:
      level: comprehensive
      budget: 2000
"#;
        let spec = TestSpec::from_yaml(yaml).unwrap();
        match &spec.steps[0] {
            TestStep::LoadSkill { load_skill } => {
                assert_eq!(load_skill.level, "comprehensive");
                assert_eq!(load_skill.budget, Some(2000));
            }
            _ => panic!("expected LoadSkill step"),
        }
    }

    #[test]
    fn parse_run_step() {
        let yaml = r#"
name: test
skill: test-skill
steps:
  - run:
      cmd: "cargo build"
      cwd: "/tmp"
      env:
        RUST_BACKTRACE: "1"
      timeout: 10s
"#;
        let spec = TestSpec::from_yaml(yaml).unwrap();
        match &spec.steps[0] {
            TestStep::Run { run } => {
                assert_eq!(run.cmd, "cargo build");
                assert_eq!(run.cwd, Some("/tmp".to_string()));
                assert_eq!(run.env.get("RUST_BACKTRACE"), Some(&"1".to_string()));
                assert_eq!(run.timeout, Some(Duration::from_secs(10)));
            }
            _ => panic!("expected Run step"),
        }
    }

    #[test]
    fn parse_assert_step() {
        let yaml = r#"
name: test
skill: test-skill
steps:
  - assert:
      exit_code: 0
      stdout_contains: "success"
      file_exists: "/tmp/output.txt"
"#;
        let spec = TestSpec::from_yaml(yaml).unwrap();
        match &spec.steps[0] {
            TestStep::Assert { assert } => {
                assert_eq!(assert.exit_code, Some(0));
                assert_eq!(assert.stdout_contains, Some("success".to_string()));
                assert_eq!(assert.file_exists, Some("/tmp/output.txt".to_string()));
            }
            _ => panic!("expected Assert step"),
        }
    }

    #[test]
    fn parse_minimal_spec() {
        let yaml = "name: minimal\nsteps: []\n";
        let spec = TestSpec::from_yaml(yaml).unwrap();
        assert_eq!(spec.name, "minimal");
        assert!(spec.description.is_none());
        assert!(spec.skill.is_none());
        assert!(spec.setup.is_none());
        assert!(spec.steps.is_empty());
        assert!(spec.cleanup.is_none());
        assert!(spec.timeout.is_none());
        assert!(spec.tags.is_empty());
        assert!(spec.skip_if.is_none());
        assert!(spec.requires.is_none());
    }

    #[test]
    fn parse_write_file_step() {
        let yaml = r#"
name: test
steps:
  - write_file:
      path: "/tmp/out.txt"
      content: "hello world"
"#;
        let spec = TestSpec::from_yaml(yaml).unwrap();
        match &spec.steps[0] {
            TestStep::WriteFile { write_file } => {
                assert_eq!(write_file.path, "/tmp/out.txt");
                assert_eq!(write_file.content, "hello world");
            }
            _ => panic!("expected WriteFile step"),
        }
    }

    #[test]
    fn parse_set_step() {
        let yaml = r#"
name: test
steps:
  - set:
      name: MY_VAR
      value: some_value
"#;
        let spec = TestSpec::from_yaml(yaml).unwrap();
        match &spec.steps[0] {
            TestStep::Set { set } => {
                assert_eq!(set.name, "MY_VAR");
                assert_eq!(set.value, "some_value");
            }
            _ => panic!("expected Set step"),
        }
    }

    #[test]
    fn parse_copy_step() {
        let yaml = r#"
name: test
steps:
  - copy:
      from: "/tmp/a.txt"
      to: "/tmp/b.txt"
"#;
        let spec = TestSpec::from_yaml(yaml).unwrap();
        match &spec.steps[0] {
            TestStep::Copy { copy } => {
                assert_eq!(copy.from, "/tmp/a.txt");
                assert_eq!(copy.to, "/tmp/b.txt");
            }
            _ => panic!("expected Copy step"),
        }
    }

    #[test]
    fn parse_sleep_step() {
        let yaml = r#"
name: test
steps:
  - sleep:
      duration: 500ms
"#;
        let spec = TestSpec::from_yaml(yaml).unwrap();
        match &spec.steps[0] {
            TestStep::Sleep { sleep } => {
                assert_eq!(sleep.duration, Duration::from_millis(500));
            }
            _ => panic!("expected Sleep step"),
        }
    }

    #[test]
    fn parse_mkdir_step() {
        let yaml = r#"
name: test
steps:
  - mkdir:
      path: "/tmp/testdir"
      parents: true
"#;
        let spec = TestSpec::from_yaml(yaml).unwrap();
        match &spec.steps[0] {
            TestStep::Mkdir { mkdir } => {
                assert_eq!(mkdir.path, "/tmp/testdir");
                assert!(mkdir.parents);
            }
            _ => panic!("expected Mkdir step"),
        }
    }

    #[test]
    fn parse_remove_step() {
        let yaml = r#"
name: test
steps:
  - remove:
      path: "/tmp/testdir"
      recursive: true
"#;
        let spec = TestSpec::from_yaml(yaml).unwrap();
        match &spec.steps[0] {
            TestStep::Remove { remove } => {
                assert_eq!(remove.path, "/tmp/testdir");
                assert!(remove.recursive);
            }
            _ => panic!("expected Remove step"),
        }
    }

    #[test]
    fn parse_if_step() {
        let yaml = r#"
name: test
steps:
  - if:
      condition:
        platform: linux
      then:
        - set:
            name: OS
            value: linux
      else:
        - set:
            name: OS
            value: other
"#;
        let spec = TestSpec::from_yaml(yaml).unwrap();
        match &spec.steps[0] {
            TestStep::If { if_step } => {
                assert_eq!(if_step.condition.platform, Some("linux".to_string()));
                assert_eq!(if_step.then_steps.len(), 1);
                assert!(if_step.else_steps.is_some());
                assert_eq!(if_step.else_steps.as_ref().unwrap().len(), 1);
            }
            _ => panic!("expected If step"),
        }
    }

    #[test]
    fn parse_skip_conditions() {
        let yaml = r#"
name: test
steps: []
skip_if:
  - platform: windows
  - command_missing: nonexistent-cmd
  - env_missing: NONEXISTENT_VAR
"#;
        let spec = TestSpec::from_yaml(yaml).unwrap();
        let skip_if = spec.skip_if.as_ref().unwrap();
        assert_eq!(skip_if.len(), 3);
        matches!(&skip_if[0], SkipCondition::Platform(p) if p == "windows");
        matches!(&skip_if[1], SkipCondition::CommandMissing(c) if c == "nonexistent-cmd");
        matches!(&skip_if[2], SkipCondition::EnvMissing(v) if v == "NONEXISTENT_VAR");
    }

    #[test]
    fn parse_requirements() {
        let yaml = r#"
name: test
steps: []
requires:
  - command: git
  - env: HOME
  - platform: linux
"#;
        let spec = TestSpec::from_yaml(yaml).unwrap();
        let reqs = spec.requires.as_ref().unwrap();
        assert_eq!(reqs.len(), 3);
        matches!(&reqs[0], Requirement::Command(c) if c == "git");
        matches!(&reqs[1], Requirement::Env(e) if e == "HOME");
        matches!(&reqs[2], Requirement::Platform(p) if p == "linux");
    }

    #[test]
    fn invalid_yaml_errors() {
        let result = TestSpec::from_yaml("not: [valid: yaml: spec");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("invalid test YAML"));
    }

    #[test]
    fn has_tag_case_insensitive() {
        let yaml = "name: test\nsteps: []\ntags: [Smoke, Integration]\n";
        let spec = TestSpec::from_yaml(yaml).unwrap();
        assert!(spec.has_tag("smoke"));
        assert!(spec.has_tag("SMOKE"));
        assert!(spec.has_tag("Smoke"));
        assert!(spec.has_tag("integration"));
        assert!(!spec.has_tag("unit"));
    }

    #[test]
    fn should_skip_returns_none_without_conditions() {
        let yaml = "name: test\nsteps: []\n";
        let spec = TestSpec::from_yaml(yaml).unwrap();
        assert!(spec.should_skip().is_none());
    }

    #[test]
    fn should_skip_env_missing() {
        let yaml = r#"
name: test
steps: []
skip_if:
  - env_missing: MS_TEST_NONEXISTENT_VAR_12345
"#;
        let spec = TestSpec::from_yaml(yaml).unwrap();
        let reason = spec.should_skip();
        assert!(reason.is_some());
        assert!(reason.unwrap().contains("env var missing"));
    }

    #[test]
    fn check_requirements_passes_when_none() {
        let yaml = "name: test\nsteps: []\n";
        let spec = TestSpec::from_yaml(yaml).unwrap();
        assert!(spec.check_requirements().is_ok());
    }

    #[test]
    fn check_requirements_fails_missing_command() {
        let yaml = r#"
name: test
steps: []
requires:
  - command: nonexistent_command_xyz_99
"#;
        let spec = TestSpec::from_yaml(yaml).unwrap();
        let result = spec.check_requirements();
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("command not found"));
    }

    #[test]
    fn load_skill_default_level() {
        let yaml = r#"
name: test
steps:
  - load_skill:
      budget: 500
"#;
        let spec = TestSpec::from_yaml(yaml).unwrap();
        match &spec.steps[0] {
            TestStep::LoadSkill { load_skill } => {
                assert_eq!(load_skill.level, "standard");
                assert_eq!(load_skill.budget, Some(500));
            }
            _ => panic!("expected LoadSkill step"),
        }
    }

    #[test]
    fn parse_multiple_step_types() {
        let yaml = r#"
name: multi-step
steps:
  - set:
      name: dir
      value: /tmp/test
  - mkdir:
      path: /tmp/test
  - run:
      cmd: echo done
  - assert:
      exit_code: 0
"#;
        let spec = TestSpec::from_yaml(yaml).unwrap();
        assert_eq!(spec.steps.len(), 4);
        assert!(matches!(&spec.steps[0], TestStep::Set { .. }));
        assert!(matches!(&spec.steps[1], TestStep::Mkdir { .. }));
        assert!(matches!(&spec.steps[2], TestStep::Run { .. }));
        assert!(matches!(&spec.steps[3], TestStep::Assert { .. }));
    }

    #[test]
    fn parse_condition_env_equals() {
        let yaml = r#"
name: test
steps:
  - if:
      condition:
        env_equals:
          CI: "true"
      then:
        - set:
            name: mode
            value: ci
"#;
        let spec = TestSpec::from_yaml(yaml).unwrap();
        match &spec.steps[0] {
            TestStep::If { if_step } => {
                let env_eq = if_step.condition.env_equals.as_ref().unwrap();
                assert_eq!(env_eq.get("CI"), Some(&"true".to_string()));
            }
            _ => panic!("expected If step"),
        }
    }
}
