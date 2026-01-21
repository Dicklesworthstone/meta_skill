//! Skill simulation sandbox.
//!
//! Executes runnable elements from skills in an isolated temp workspace and
//! emits a structured report.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tempfile::TempDir;
use walkdir::WalkDir;

use crate::app::AppContext;
use crate::core::skill::{BlockType, SkillSpec};
use crate::core::spec_lens::parse_markdown;
use crate::error::{MsError, Result};
use crate::security::SafetyGate;
use crate::storage::sqlite::SkillRecord;

#[derive(Debug, Clone)]
pub struct SimulationConfig {
    pub allow_network: bool,
    pub allow_external_fs: bool,
    pub command_timeout: Duration,
    pub total_timeout: Duration,
    pub max_output_bytes: usize,
    pub blocked_commands: Vec<String>,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            allow_network: false,
            allow_external_fs: false,
            command_timeout: Duration::from_secs(30),
            total_timeout: Duration::from_secs(300),
            max_output_bytes: 16 * 1024,
            blocked_commands: vec![
                "rm -rf".to_string(),
                "mkfs".to_string(),
                "dd if=".to_string(),
                "sudo".to_string(),
            ],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimulationResult {
    Success,
    PartialSuccess { passed: usize, failed: usize },
    Failure { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ElementStatus {
    Passed,
    Failed,
    Skipped,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ElementResult {
    pub element: String,
    pub status: ElementStatus,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stdout: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stderr: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationIssue {
    pub severity: IssueSeverity,
    pub element: String,
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IssueSeverity {
    Error,
    Warning,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileSystemChanges {
    pub created: Vec<String>,
    pub modified: Vec<String>,
    pub deleted: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationReport {
    pub skill_id: String,
    pub skill_name: String,
    pub started_at: String,
    pub duration_ms: u64,
    pub result: SimulationResult,
    pub element_results: Vec<ElementResult>,
    pub fs_changes: FileSystemChanges,
    pub issues: Vec<SimulationIssue>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
enum SimElement {
    Command {
        command: String,
        source: String,
    },
    CodeSnippet {
        language: String,
        code: String,
        source: String,
    },
    Unsupported {
        language: Option<String>,
        source: String,
    },
}

pub struct SimulationEngine<'a> {
    ctx: &'a AppContext,
    safety: SafetyGate,
}

impl<'a> SimulationEngine<'a> {
    #[must_use]
    pub fn new(ctx: &'a AppContext) -> Self {
        Self {
            ctx,
            safety: SafetyGate::from_context(ctx),
        }
    }

    pub fn simulate(
        &self,
        skill_ref: &str,
        fixtures: Option<&Path>,
        config: SimulationConfig,
    ) -> Result<SimulationReport> {
        let skill = resolve_skill(self.ctx, skill_ref)?;
        let spec = parse_markdown(&skill.body).map_err(|err| {
            MsError::ValidationFailed(format!("failed to parse skill body: {err}"))
        })?;

        let mut sandbox = SimulationSandbox::new(config.clone())?;
        if let Some(fixtures_path) = fixtures {
            sandbox.setup_fixtures(fixtures_path)?;
        }

        let started = chrono::Utc::now().to_rfc3339();
        let start_clock = Instant::now();
        let mut warnings = Vec::new();
        let mut issues = Vec::new();
        let elements = extract_elements(&spec);
        if elements.is_empty() {
            warnings.push("no simulatable elements found".to_string());
        }

        let mut results = Vec::new();
        let mut passed = 0usize;
        let mut failed = 0usize;

        for element in elements {
            if start_clock.elapsed() > config.total_timeout {
                issues.push(SimulationIssue {
                    severity: IssueSeverity::Error,
                    element: "overall".to_string(),
                    description: "simulation timeout exceeded".to_string(),
                    suggestion: Some("reduce number of elements or increase timeout".to_string()),
                });
                break;
            }

            let result = match element {
                SimElement::Command { command, source } => {
                    self.run_command(&mut sandbox, &command, &source)?
                }
                SimElement::CodeSnippet {
                    language,
                    code,
                    source,
                } => self.run_code(&mut sandbox, &language, &code, &source)?,
                SimElement::Unsupported { language, source } => ElementResult {
                    element: format!(
                        "Code ({}) from {}",
                        language.unwrap_or("unknown".to_string()),
                        source
                    ),
                    status: ElementStatus::Skipped,
                    duration_ms: 0,
                    stdout: None,
                    stderr: None,
                    exit_code: None,
                    note: Some("unsupported language".to_string()),
                },
            };

            match result.status {
                ElementStatus::Passed => passed += 1,
                ElementStatus::Failed => {
                    failed += 1;
                    if let Some(desc) = result.stderr.clone() {
                        issues.push(SimulationIssue {
                            severity: IssueSeverity::Error,
                            element: result.element.clone(),
                            description: desc,
                            suggestion: None,
                        });
                    }
                }
                ElementStatus::Skipped => {
                    warnings.push(format!("skipped {}", result.element));
                }
            }
            results.push(result);
        }

        let fs_changes = sandbox.fs_changes()?;
        let duration_ms = start_clock.elapsed().as_millis() as u64;
        let result = if failed == 0 && !results.is_empty() {
            SimulationResult::Success
        } else if failed == 0 && results.is_empty() {
            SimulationResult::Failure {
                reason: "no simulatable elements found".to_string(),
            }
        } else if passed > 0 {
            SimulationResult::PartialSuccess { passed, failed }
        } else {
            SimulationResult::Failure {
                reason: "all elements failed".to_string(),
            }
        };

        Ok(SimulationReport {
            skill_id: skill.id.clone(),
            skill_name: skill.name,
            started_at: started,
            duration_ms,
            result,
            element_results: results,
            fs_changes,
            issues,
            warnings,
        })
    }

    fn run_command(
        &self,
        sandbox: &mut SimulationSandbox,
        command: &str,
        source: &str,
    ) -> Result<ElementResult> {
        sandbox.guard_command(command)?;
        self.safety.enforce(command, None)?;

        let start = Instant::now();
        let result = sandbox.execute_command(command, None)?;
        let duration_ms = start.elapsed().as_millis() as u64;
        let status = if result.exit_code == 0 {
            ElementStatus::Passed
        } else {
            ElementStatus::Failed
        };

        Ok(ElementResult {
            element: format!("Command: {command} ({source})"),
            status,
            duration_ms,
            stdout: Some(result.stdout),
            stderr: Some(result.stderr),
            exit_code: Some(result.exit_code),
            note: None,
        })
    }

    fn run_code(
        &self,
        sandbox: &mut SimulationSandbox,
        language: &str,
        code: &str,
        source: &str,
    ) -> Result<ElementResult> {
        let (cmd, path, note) = sandbox.prepare_code(language, code)?;
        sandbox.guard_command(&cmd)?;
        self.safety.enforce(&cmd, None)?;

        let start = Instant::now();
        let result =
            sandbox.execute_command(&cmd, Some(path.parent().unwrap_or(Path::new("."))))?;
        let duration_ms = start.elapsed().as_millis() as u64;
        let status = if result.exit_code == 0 {
            ElementStatus::Passed
        } else {
            ElementStatus::Failed
        };

        Ok(ElementResult {
            element: format!("Code ({language}) from {source}"),
            status,
            duration_ms,
            stdout: Some(result.stdout),
            stderr: Some(result.stderr),
            exit_code: Some(result.exit_code),
            note,
        })
    }
}

fn resolve_skill(ctx: &AppContext, skill_ref: &str) -> Result<SkillRecord> {
    if let Some(skill) = ctx.db.get_skill(skill_ref)? {
        return Ok(skill);
    }
    if let Some(alias) = ctx.db.resolve_alias(skill_ref)? {
        if let Some(skill) = ctx.db.get_skill(&alias.canonical_id)? {
            return Ok(skill);
        }
    }
    Err(MsError::SkillNotFound(format!(
        "skill not found: {skill_ref}"
    )))
}

fn extract_elements(spec: &SkillSpec) -> Vec<SimElement> {
    let mut elements = Vec::new();
    for section in &spec.sections {
        for block in &section.blocks {
            if block.block_type != BlockType::Code {
                continue;
            }
            let (lang, content) = parse_code_block(&block.content);
            if content.trim().is_empty() {
                continue;
            }
            let source = section.title.clone();
            match lang.as_deref().map(str::to_lowercase) {
                Some(lang) if is_shell_lang(&lang) => {
                    for cmd in extract_shell_commands(&content) {
                        elements.push(SimElement::Command {
                            command: cmd,
                            source: source.clone(),
                        });
                    }
                }
                Some(lang) if is_supported_code_lang(&lang) => {
                    elements.push(SimElement::CodeSnippet {
                        language: lang,
                        code: content.clone(),
                        source,
                    });
                }
                None => elements.push(SimElement::Unsupported {
                    language: None,
                    source,
                }),
                Some(lang) => elements.push(SimElement::Unsupported {
                    language: Some(lang),
                    source,
                }),
            }
        }
    }
    elements
}

fn is_shell_lang(lang: &str) -> bool {
    matches!(lang, "bash" | "sh" | "shell")
}

fn is_supported_code_lang(lang: &str) -> bool {
    matches!(lang, "python" | "py" | "javascript" | "js" | "rust" | "rs")
}

fn extract_shell_commands(content: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('#') || trimmed.starts_with('$') {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
}

fn parse_code_block(content: &str) -> (Option<String>, String) {
    let mut lines = content.lines();
    let first = lines.next().unwrap_or("");
    if first.trim_start().starts_with("```") {
        let lang = first.trim_start().trim_start_matches("```").trim();
        let mut body: Vec<&str> = lines.collect();
        if let Some(last) = body.last() {
            if last.trim() == "```" {
                body.pop();
            }
        }
        let text = body.join("\n");
        let language = if lang.is_empty() {
            None
        } else {
            Some(lang.to_string())
        };
        return (language, text);
    }

    (None, content.to_string())
}

struct SimulationSandbox {
    workspace: TempDir,
    env: HashMap<String, String>,
    initial_state: FileSystemState,
    config: SimulationConfig,
}

impl SimulationSandbox {
    fn new(config: SimulationConfig) -> Result<Self> {
        let workspace = TempDir::new()
            .map_err(|err| MsError::Config(format!("create temp workspace: {err}")))?;
        let mut sandbox = Self {
            workspace,
            env: HashMap::new(),
            initial_state: FileSystemState::empty(),
            config,
        };
        sandbox.initial_state = sandbox.capture_fs_state()?;
        Ok(sandbox)
    }

    fn setup_fixtures(&mut self, fixtures: &Path) -> Result<()> {
        if !fixtures.exists() {
            return Err(MsError::NotFound(format!(
                "fixtures not found: {}",
                fixtures.display()
            )));
        }
        copy_dir_recursive(fixtures, self.workspace.path())?;
        self.initial_state = self.capture_fs_state()?;
        Ok(())
    }

    fn guard_command(&self, command: &str) -> Result<()> {
        for blocked in &self.config.blocked_commands {
            if command.contains(blocked) {
                return Err(MsError::DestructiveBlocked(format!(
                    "blocked command in simulation: {blocked}"
                )));
            }
        }
        if !self.config.allow_network {
            let network_commands = ["curl", "wget", "ssh", "scp"];
            if network_commands.iter().any(|cmd| command.contains(cmd)) {
                return Err(MsError::DestructiveBlocked(
                    "network commands blocked (use --allow-network to enable)".to_string(),
                ));
            }
        }
        Ok(())
    }

    fn prepare_code(
        &self,
        language: &str,
        code: &str,
    ) -> Result<(String, PathBuf, Option<String>)> {
        let ext = extension_for_language(language);
        let filename = format!("snippet_{}.{}", uuid::Uuid::new_v4(), ext);
        let path = self.workspace.path().join(filename);
        std::fs::write(&path, code).map_err(|err| {
            MsError::Config(format!("write code snippet {}: {err}", path.display()))
        })?;

        let (cmd, note) = match language {
            "python" | "py" => (format!("python3 {}", path.display()), None),
            "javascript" | "js" => (format!("node {}", path.display()), None),
            "bash" | "sh" | "shell" => (format!("bash {}", path.display()), None),
            "rust" | "rs" => {
                let bin = path.with_extension("bin");
                (
                    format!("rustc {} -o {}", path.display(), bin.display()),
                    Some("compiled only (binary not executed)".to_string()),
                )
            }
            _ => {
                return Err(MsError::NotImplemented(format!(
                    "language not supported: {language}"
                )));
            }
        };

        Ok((cmd, path, note))
    }

    fn execute_command(&mut self, cmd: &str, cwd: Option<&Path>) -> Result<CommandResult> {
        let shell = if cfg!(windows) { "cmd" } else { "sh" };
        let shell_arg = if cfg!(windows) { "/C" } else { "-c" };
        let working_dir = cwd.map_or_else(
            || self.workspace.path().to_path_buf(),
            std::path::Path::to_path_buf,
        );

        let mut command = Command::new(shell);
        command.arg(shell_arg).arg(cmd);
        command.current_dir(&working_dir);
        command.stdin(Stdio::null());
        command.stdout(Stdio::piped()).stderr(Stdio::piped());

        for (key, value) in &self.env {
            command.env(key, value);
        }

        let output = execute_with_timeout(&mut command, self.config.command_timeout)?;
        let stdout = truncate_output(&output.stdout, self.config.max_output_bytes);
        let stderr = truncate_output(&output.stderr, self.config.max_output_bytes);

        Ok(CommandResult {
            exit_code: output.status.code().unwrap_or(-1),
            stdout,
            stderr,
        })
    }

    fn fs_changes(&self) -> Result<FileSystemChanges> {
        let current = self.capture_fs_state()?;
        Ok(self.initial_state.diff(&current))
    }

    fn capture_fs_state(&self) -> Result<FileSystemState> {
        let mut state = FileSystemState::empty();
        for entry in WalkDir::new(self.workspace.path())
            .follow_links(false)
            .into_iter()
            .filter_map(std::result::Result::ok)
        {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let rel = path
                .strip_prefix(self.workspace.path())
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();
            let content = std::fs::read(path).unwrap_or_default();
            let hash = hash_content(&content);
            state.files.insert(rel, FileInfo { hash });
        }
        Ok(state)
    }
}

struct CommandResult {
    exit_code: i32,
    stdout: String,
    stderr: String,
}

#[derive(Debug, Clone)]
struct FileSystemState {
    files: HashMap<String, FileInfo>,
}

impl FileSystemState {
    fn empty() -> Self {
        Self {
            files: HashMap::new(),
        }
    }

    fn diff(&self, other: &Self) -> FileSystemChanges {
        let mut created = Vec::new();
        let mut modified = Vec::new();
        let mut deleted = Vec::new();

        for (path, info) in &other.files {
            match self.files.get(path) {
                None => created.push(path.clone()),
                Some(old) if old.hash != info.hash => modified.push(path.clone()),
                _ => {}
            }
        }

        for path in self.files.keys() {
            if !other.files.contains_key(path) {
                deleted.push(path.clone());
            }
        }

        FileSystemChanges {
            created,
            modified,
            deleted,
        }
    }
}

#[derive(Debug, Clone)]
struct FileInfo {
    hash: String,
}

fn extension_for_language(lang: &str) -> &'static str {
    match lang {
        "rust" | "rs" => "rs",
        "python" | "py" => "py",
        "javascript" | "js" => "js",
        "bash" | "sh" | "shell" => "sh",
        _ => "txt",
    }
}

fn execute_with_timeout(command: &mut Command, timeout: Duration) -> Result<std::process::Output> {
    let mut child = command.spawn().map_err(|err| {
        MsError::Config(format!(
            "failed to execute command '{}': {err}",
            command_str(command)
        ))
    })?;

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| MsError::Config("failed to capture stdout for command".to_string()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| MsError::Config("failed to capture stderr for command".to_string()))?;

    let stdout_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let mut reader = stdout;
        let _ = reader.read_to_end(&mut buf);
        buf
    });
    let stderr_handle = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let mut reader = stderr;
        let _ = reader.read_to_end(&mut buf);
        buf
    });

    let start = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|err| MsError::Config(format!("failed to poll command: {err}")))?
        {
            let stdout = stdout_handle.join().unwrap_or_else(|_| Vec::new());
            let stderr = stderr_handle.join().unwrap_or_else(|_| Vec::new());
            return Ok(std::process::Output {
                status,
                stdout,
                stderr,
            });
        }

        if start.elapsed() > timeout {
            let _ = child.kill();
            return Err(MsError::Timeout(format!(
                "command timed out after {}s",
                timeout.as_secs()
            )));
        }

        std::thread::sleep(Duration::from_millis(50));
    }
}

fn truncate_output(bytes: &[u8], max: usize) -> String {
    let text = String::from_utf8_lossy(bytes).to_string();
    if text.len() > max {
        let mut end = max;
        while end > 0 && !text.is_char_boundary(end) {
            end -= 1;
        }
        format!("{}...[truncated]", &text[..end])
    } else {
        text
    }
}

fn command_str(command: &Command) -> String {
    let mut out = String::new();
    out.push_str(command.get_program().to_string_lossy().as_ref());
    for arg in command.get_args() {
        out.push(' ');
        out.push_str(arg.to_string_lossy().as_ref());
    }
    out
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)
        .map_err(|err| MsError::Config(format!("create fixtures dir {}: {err}", dst.display())))?;
    for entry in std::fs::read_dir(src)
        .map_err(|err| MsError::Config(format!("read fixtures dir: {err}")))?
    {
        let entry = entry.map_err(|err| MsError::Config(format!("read fixtures entry: {err}")))?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path).map_err(|err| {
                MsError::Config(format!("copy fixture {}: {err}", src_path.display()))
            })?;
        }
    }
    Ok(())
}

fn hash_content(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    let digest = hasher.finalize();
    format!("{digest:x}")[..16].to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::skill::{SkillBlock, SkillMetadata, SkillSection, SkillSpec};

    #[test]
    fn parse_code_block_keeps_language_and_body() {
        let input = "```bash\nls -la\n```";
        let (lang, body) = parse_code_block(input);
        assert_eq!(lang.as_deref(), Some("bash"));
        assert_eq!(body.trim(), "ls -la");
    }

    #[test]
    fn extract_elements_from_shell_block() {
        let spec = SkillSpec {
            format_version: SkillSpec::FORMAT_VERSION.to_string(),
            metadata: SkillMetadata {
                id: "test".to_string(),
                name: "Test".to_string(),
                ..Default::default()
            },
            sections: vec![SkillSection {
                id: "setup".to_string(),
                title: "Setup".to_string(),
                blocks: vec![SkillBlock {
                    id: "setup-block-1".to_string(),
                    block_type: BlockType::Code,
                    content: "```bash\n# comment\n$ echo skip\nls\n```".to_string(),
                }],
            }],
            ..Default::default()
        };

        let elements = extract_elements(&spec);
        assert_eq!(elements.len(), 1);
        match &elements[0] {
            SimElement::Command { command, .. } => assert_eq!(command, "ls"),
            _ => panic!("expected command element"),
        }
    }
}
