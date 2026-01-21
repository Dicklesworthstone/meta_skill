//! Test step execution
//!
//! Handles execution of individual test steps defined in test YAML files.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::app::AppContext;
use crate::error::{MsError, Result};
use crate::security::SafetyGate;

use super::definition::{
    Assertions, Condition, CopyStep, IfStep, LoadSkillStep, MkdirStep, RemoveStep, RunStep,
    SetStep, SleepStep, TestStep, WriteFileStep,
};

/// Step executor that manages test context and executes steps
pub struct StepExecutor<'a> {
    #[allow(dead_code)]
    ctx: &'a AppContext,
    test_ctx: TestContext,
    verbose: bool,
    safety: Option<SafetyGate>,
}

impl<'a> StepExecutor<'a> {
    /// Create a new step executor
    #[must_use]
    pub fn new(ctx: &'a AppContext, verbose: bool) -> Self {
        Self {
            ctx,
            test_ctx: TestContext::default(),
            verbose,
            safety: None,
        }
    }

    /// Enable safety gate for command execution.
    ///
    /// When enabled, all `run` steps will be validated through DCG
    /// before execution.
    #[must_use]
    pub fn with_safety(mut self, gate: SafetyGate) -> Self {
        self.safety = Some(gate);
        self
    }

    /// Execute a single test step
    pub fn execute(&mut self, step: &TestStep) -> Result<()> {
        execute_step(step, &mut self.test_ctx, self.verbose, self.safety.as_ref())
    }

    /// Get a reference to the test context
    #[must_use]
    pub const fn test_context(&self) -> &TestContext {
        &self.test_ctx
    }
}

/// Context for test execution
#[derive(Debug, Default)]
pub struct TestContext {
    /// Variables set during test
    pub variables: HashMap<String, String>,
    /// Last command stdout
    pub last_stdout: String,
    /// Last command stderr
    pub last_stderr: String,
    /// Last command exit code
    pub last_exit_code: Option<i32>,
    /// Loaded skill info
    pub loaded_skill: Option<LoadedSkillInfo>,
    /// Tokens used in last load
    pub tokens_used: usize,
    /// Retrieval rank (if applicable)
    pub retrieval_rank: Option<usize>,
}

/// Info about a loaded skill
#[derive(Debug, Clone)]
pub struct LoadedSkillInfo {
    pub skill_id: String,
    pub sections: Vec<String>,
    pub content_length: usize,
}

impl TestContext {
    /// Expand variables in a string (${var} syntax)
    #[must_use]
    pub fn expand(&self, input: &str) -> String {
        let mut result = input.to_string();
        for (name, value) in &self.variables {
            let pattern = format!("${{{name}}}");
            result = result.replace(&pattern, value);
        }
        result
    }
}

/// Execute a single test step
pub fn execute_step(
    step: &TestStep,
    ctx: &mut TestContext,
    verbose: bool,
    safety: Option<&SafetyGate>,
) -> Result<()> {
    match step {
        TestStep::LoadSkill { load_skill } => execute_load_skill(load_skill, ctx, verbose),
        TestStep::Run { run } => execute_run(run, ctx, verbose, safety),
        TestStep::Assert { assert } => execute_assert(assert, ctx, verbose),
        TestStep::WriteFile { write_file } => execute_write_file(write_file, ctx, verbose),
        TestStep::Mkdir { mkdir } => execute_mkdir(mkdir, ctx, verbose),
        TestStep::Remove { remove } => execute_remove(remove, ctx, verbose),
        TestStep::Copy { copy } => execute_copy(copy, ctx, verbose),
        TestStep::Sleep { sleep } => execute_sleep(sleep, ctx, verbose),
        TestStep::Set { set } => execute_set(set, ctx, verbose),
        TestStep::If { if_step } => execute_if(if_step, ctx, verbose, safety),
    }
}

fn execute_load_skill(step: &LoadSkillStep, ctx: &mut TestContext, verbose: bool) -> Result<()> {
    if verbose {
        println!("[STEP] load_skill level={}", step.level);
    }

    // For now, simulate a successful load
    // In a real implementation, this would call the actual skill loader
    ctx.loaded_skill = Some(LoadedSkillInfo {
        skill_id: "test-skill".to_string(),
        sections: vec![
            "overview".to_string(),
            "examples".to_string(),
            "best-practices".to_string(),
        ],
        content_length: 1000,
    });
    ctx.tokens_used = step.budget.unwrap_or(1000);

    Ok(())
}

fn execute_run(
    step: &RunStep,
    ctx: &mut TestContext,
    verbose: bool,
    safety: Option<&SafetyGate>,
) -> Result<()> {
    let cmd = ctx.expand(&step.cmd);
    let cwd = step.cwd.as_ref().map(|c| ctx.expand(c));
    let stdin = step.stdin.as_ref().map(|s| ctx.expand(s));

    if verbose {
        println!("[STEP] run: {cmd}");
        if let Some(ref dir) = cwd {
            println!("[STEP]   cwd: {dir}");
        }
    }

    // Enforce safety gate before execution
    if let Some(gate) = safety {
        gate.enforce(&cmd, None)?;
    }

    let shell = if cfg!(windows) { "cmd" } else { "sh" };
    let shell_arg = if cfg!(windows) { "/C" } else { "-c" };

    let mut command = Command::new(shell);
    command.arg(shell_arg).arg(&cmd);

    if let Some(ref dir) = cwd {
        command.current_dir(dir);
    }

    for (key, value) in &step.env {
        command.env(key, ctx.expand(value));
    }

    let timeout = step.timeout.unwrap_or(Duration::from_secs(30));

    if stdin.is_some() {
        command.stdin(Stdio::piped());
    }
    command.stdout(Stdio::piped()).stderr(Stdio::piped());

    let mut child = command
        .spawn()
        .map_err(|err| MsError::Config(format!("failed to execute command '{cmd}': {err}")))?;

    if let Some(input) = stdin {
        if let Some(mut child_stdin) = child.stdin.take() {
            child_stdin.write_all(input.as_bytes()).map_err(|err| {
                MsError::Config(format!("failed to write stdin for '{cmd}': {err}"))
            })?;
        }
    }

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| MsError::Config(format!("failed to capture stdout for '{cmd}'")))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| MsError::Config(format!("failed to capture stderr for '{cmd}'")))?;

    let stdout_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let mut reader = stdout;
        reader.read_to_end(&mut buf).map(|_| buf)
    });
    let stderr_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let mut reader = stderr;
        reader.read_to_end(&mut buf).map(|_| buf)
    });

    let start = Instant::now();
    let mut timed_out = false;
    let exit_status;

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                exit_status = Some(status);
                break;
            }
            Ok(None) => {
                if start.elapsed() > timeout {
                    timed_out = true;
                    let _ = child.kill();
                    exit_status = Some(child.wait().map_err(|err| {
                        MsError::Config(format!("failed to wait for '{cmd}': {err}"))
                    })?);
                    break;
                }
                std::thread::sleep(Duration::from_millis(25));
            }
            Err(err) => {
                return Err(MsError::Config(format!(
                    "failed to wait for command '{cmd}': {err}"
                )));
            }
        }
    }

    let stdout_bytes = stdout_handle
        .join()
        .map_err(|_| MsError::Config(format!("stdout capture panicked for '{cmd}'")))?
        .map_err(|err| MsError::Config(format!("read stdout for '{cmd}': {err}")))?;
    let stderr_bytes = stderr_handle
        .join()
        .map_err(|_| MsError::Config(format!("stderr capture panicked for '{cmd}'")))?
        .map_err(|err| MsError::Config(format!("read stderr for '{cmd}': {err}")))?;

    ctx.last_stdout = String::from_utf8_lossy(&stdout_bytes).to_string();
    ctx.last_stderr = String::from_utf8_lossy(&stderr_bytes).to_string();
    ctx.last_exit_code = exit_status.and_then(|status| status.code());

    if verbose {
        if !ctx.last_stdout.is_empty() {
            println!("[STEP]   stdout: {}", ctx.last_stdout.trim());
        }
        if !ctx.last_stderr.is_empty() {
            println!("[STEP]   stderr: {}", ctx.last_stderr.trim());
        }
        println!("[STEP]   exit: {:?}", ctx.last_exit_code);
    }

    if timed_out {
        return Err(MsError::ValidationFailed(format!(
            "command timed out after {timeout:?}: {cmd}"
        )));
    }

    Ok(())
}

fn execute_assert(step: &Assertions, ctx: &mut TestContext, verbose: bool) -> Result<()> {
    if verbose {
        println!("[STEP] assert");
    }

    let mut failures = Vec::new();

    // Check exit code
    if let Some(expected) = step.exit_code {
        if ctx.last_exit_code != Some(expected) {
            failures.push(format!(
                "exit_code: expected {}, got {:?}",
                expected, ctx.last_exit_code
            ));
        }
    }

    // Check stdout contains
    if let Some(ref text) = step.stdout_contains {
        if !ctx.last_stdout.contains(text) {
            failures.push(format!("stdout_contains: '{text}' not found in stdout"));
        }
    }

    // Check stdout not contains
    if let Some(ref text) = step.stdout_not_contains {
        if ctx.last_stdout.contains(text) {
            failures.push(format!("stdout_not_contains: '{text}' found in stdout"));
        }
    }

    // Check stderr empty
    if step.stderr_empty == Some(true) && !ctx.last_stderr.trim().is_empty() {
        failures.push(format!(
            "stderr_empty: stderr is not empty: {}",
            ctx.last_stderr.trim()
        ));
    }

    // Check file exists
    if let Some(ref path) = step.file_exists {
        let expanded = ctx.expand(path);
        if !Path::new(&expanded).exists() {
            failures.push(format!("file_exists: {expanded} does not exist"));
        }
    }

    // Check file contains
    if let Some(ref fc) = step.file_contains {
        let path = ctx.expand(&fc.path);
        match std::fs::read_to_string(&path) {
            Ok(content) => {
                if !content.contains(&fc.text) {
                    failures.push(format!(
                        "file_contains: '{}' not found in {}",
                        fc.text, path
                    ));
                }
            }
            Err(err) => {
                failures.push(format!("file_contains: cannot read {path}: {err}"));
            }
        }
    }

    // Check skill loaded
    if step.skill_loaded == Some(true) && ctx.loaded_skill.is_none() {
        failures.push("skill_loaded: no skill was loaded".to_string());
    }

    // Check sections present
    if let Some(ref expected_sections) = step.sections_present {
        if let Some(ref skill) = ctx.loaded_skill {
            for section in expected_sections {
                if !skill
                    .sections
                    .iter()
                    .any(|s| s.eq_ignore_ascii_case(section))
                {
                    failures.push(format!("sections_present: section '{section}' not found"));
                }
            }
        } else {
            failures.push("sections_present: no skill loaded".to_string());
        }
    }

    // Check tokens used
    if let Some(max) = step.tokens_used_lt {
        if ctx.tokens_used >= max {
            failures.push(format!("tokens_used_lt: {} >= {}", ctx.tokens_used, max));
        }
    }

    // Check retrieval rank
    if let Some(max_rank) = step.retrieval_rank_le {
        if let Some(rank) = ctx.retrieval_rank {
            if rank > max_rank {
                failures.push(format!("retrieval_rank_le: {rank} > {max_rank}"));
            }
        }
    }

    if failures.is_empty() {
        if verbose {
            println!("[STEP]   all assertions passed");
        }
        Ok(())
    } else {
        if verbose {
            for f in &failures {
                println!("[STEP]   FAIL: {f}");
            }
        }
        Err(MsError::ValidationFailed(failures.join("; ")))
    }
}

fn execute_write_file(step: &WriteFileStep, ctx: &mut TestContext, verbose: bool) -> Result<()> {
    let path = ctx.expand(&step.path);
    let content = ctx.expand(&step.content);

    if verbose {
        println!("[STEP] write_file: {} ({} bytes)", path, content.len());
    }

    // Create parent directories if needed
    if let Some(parent) = Path::new(&path).parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            MsError::Io(std::io::Error::new(
                err.kind(),
                format!("create parent dirs for {path}: {err}"),
            ))
        })?;
    }

    std::fs::write(&path, &content).map_err(|err| {
        MsError::Io(std::io::Error::new(
            err.kind(),
            format!("write {path}: {err}"),
        ))
    })?;

    Ok(())
}

fn execute_mkdir(step: &MkdirStep, ctx: &mut TestContext, verbose: bool) -> Result<()> {
    let path = ctx.expand(&step.path);

    if verbose {
        println!("[STEP] mkdir: {} (parents={})", path, step.parents);
    }

    if step.parents {
        std::fs::create_dir_all(&path)
    } else {
        std::fs::create_dir(&path)
    }
    .map_err(|err| {
        MsError::Io(std::io::Error::new(
            err.kind(),
            format!("mkdir {path}: {err}"),
        ))
    })?;

    Ok(())
}

fn execute_remove(step: &RemoveStep, ctx: &mut TestContext, verbose: bool) -> Result<()> {
    let path = ctx.expand(&step.path);

    if verbose {
        println!("[STEP] remove: {} (recursive={})", path, step.recursive);
    }

    let p = Path::new(&path);
    if !p.exists() {
        return Ok(()); // Nothing to remove
    }

    if p.is_dir() {
        if step.recursive {
            std::fs::remove_dir_all(&path)
        } else {
            std::fs::remove_dir(&path)
        }
    } else {
        std::fs::remove_file(&path)
    }
    .map_err(|err| {
        MsError::Io(std::io::Error::new(
            err.kind(),
            format!("remove {path}: {err}"),
        ))
    })?;

    Ok(())
}

fn execute_copy(step: &CopyStep, ctx: &mut TestContext, verbose: bool) -> Result<()> {
    let from = ctx.expand(&step.from);
    let to = ctx.expand(&step.to);

    if verbose {
        println!("[STEP] copy: {from} -> {to}");
    }

    // Create parent directories if needed
    if let Some(parent) = Path::new(&to).parent() {
        std::fs::create_dir_all(parent).map_err(|err| {
            MsError::Io(std::io::Error::new(
                err.kind(),
                format!("create parent dirs for {to}: {err}"),
            ))
        })?;
    }

    std::fs::copy(&from, &to).map_err(|err| {
        MsError::Io(std::io::Error::new(
            err.kind(),
            format!("copy {from} -> {to}: {err}"),
        ))
    })?;

    Ok(())
}

fn execute_sleep(step: &SleepStep, _ctx: &mut TestContext, verbose: bool) -> Result<()> {
    if verbose {
        println!("[STEP] sleep: {:?}", step.duration);
    }

    std::thread::sleep(step.duration);
    Ok(())
}

fn execute_set(step: &SetStep, ctx: &mut TestContext, verbose: bool) -> Result<()> {
    let value = ctx.expand(&step.value);

    if verbose {
        println!("[STEP] set: {}={}", step.name, value);
    }

    ctx.variables.insert(step.name.clone(), value);
    Ok(())
}

fn execute_if(
    step: &IfStep,
    ctx: &mut TestContext,
    verbose: bool,
    safety: Option<&SafetyGate>,
) -> Result<()> {
    if verbose {
        println!("[STEP] if condition");
    }

    let condition_met = evaluate_condition(&step.condition);

    let steps_to_run = if condition_met {
        &step.then_steps
    } else {
        match &step.else_steps {
            Some(steps) => steps,
            None => return Ok(()),
        }
    };

    for s in steps_to_run {
        execute_step(s, ctx, verbose, safety)?;
    }

    Ok(())
}

fn evaluate_condition(condition: &Condition) -> bool {
    // Platform check
    if let Some(ref platform) = condition.platform {
        if platform != std::env::consts::OS {
            return false;
        }
    }

    // Env exists check
    if let Some(ref var) = condition.env_exists {
        if std::env::var(var).is_err() {
            return false;
        }
    }

    // Env equals check
    if let Some(ref vars) = condition.env_equals {
        for (key, expected) in vars {
            match std::env::var(key) {
                Ok(actual) if actual == *expected => {}
                _ => return false,
            }
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_expand() {
        let mut ctx = TestContext::default();
        ctx.variables
            .insert("name".to_string(), "world".to_string());
        ctx.variables.insert("path".to_string(), "/tmp".to_string());

        assert_eq!(ctx.expand("hello ${name}"), "hello world");
        assert_eq!(ctx.expand("${path}/file.txt"), "/tmp/file.txt");
        assert_eq!(ctx.expand("no vars"), "no vars");
    }

    #[test]
    fn test_execute_set() {
        let mut ctx = TestContext::default();
        let step = SetStep {
            name: "foo".to_string(),
            value: "bar".to_string(),
        };
        execute_set(&step, &mut ctx, false).unwrap();
        assert_eq!(ctx.variables.get("foo"), Some(&"bar".to_string()));
    }

    #[test]
    fn test_execute_run_echo() {
        let mut ctx = TestContext::default();
        let step = RunStep {
            cmd: "echo hello".to_string(),
            cwd: None,
            env: HashMap::new(),
            stdin: None,
            timeout: None,
        };
        execute_run(&step, &mut ctx, false, None).unwrap();
        assert!(ctx.last_stdout.contains("hello"));
        assert_eq!(ctx.last_exit_code, Some(0));
    }

    #[test]
    fn test_execute_assert_exit_code() {
        let mut ctx = TestContext::default();
        ctx.last_exit_code = Some(0);

        let step = Assertions {
            exit_code: Some(0),
            ..Default::default()
        };
        assert!(execute_assert(&step, &mut ctx, false).is_ok());

        let step = Assertions {
            exit_code: Some(1),
            ..Default::default()
        };
        assert!(execute_assert(&step, &mut ctx, false).is_err());
    }

    #[test]
    fn test_execute_assert_stdout() {
        let mut ctx = TestContext::default();
        ctx.last_stdout = "hello world".to_string();

        let step = Assertions {
            stdout_contains: Some("world".to_string()),
            ..Default::default()
        };
        assert!(execute_assert(&step, &mut ctx, false).is_ok());

        let step = Assertions {
            stdout_not_contains: Some("error".to_string()),
            ..Default::default()
        };
        assert!(execute_assert(&step, &mut ctx, false).is_ok());

        let step = Assertions {
            stdout_contains: Some("missing".to_string()),
            ..Default::default()
        };
        assert!(execute_assert(&step, &mut ctx, false).is_err());
    }
}
