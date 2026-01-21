//! ms setup - Zero-config agent integration
//!
//! Automatically detects installed AI coding agents and configures
//! ms integration for each of them.

use std::fs;
use std::path::{Path, PathBuf};

use clap::{Args, CommandFactory};
use clap_complete::{Shell, generate};
use colored::Colorize;
use serde::Serialize;
use tracing::{debug, info};

use crate::agent_detection::{
    AgentDetectionService, AgentType, DetectedAgent, DetectionSummary, IntegrationStatus,
};
use crate::app::AppContext;
use crate::cli::output::{OutputFormat, emit_json, robot_ok};
use crate::error::{MsError, Result};
use crate::skill_md::SkillMdGenerator;

/// Setup command arguments.
#[derive(Args, Debug)]
pub struct SetupArgs {
    /// Only detect agents, do not configure
    #[arg(long)]
    pub detect_only: bool,

    /// Show current integration status
    #[arg(long)]
    pub status: bool,

    /// Remove ms integrations
    #[arg(long)]
    pub uninstall: bool,

    /// Overwrite existing configurations
    #[arg(long)]
    pub force: bool,

    /// Show what would be done without doing it
    #[arg(long)]
    pub dry_run: bool,

    /// Generate shell completions only
    #[arg(long)]
    pub completions: bool,

    /// Shell to generate completions for (bash, zsh, fish)
    #[arg(long, value_enum)]
    pub shell: Option<Shell>,

    // Agent-specific flags
    /// Configure Claude Code only
    #[arg(long)]
    pub claude_code: bool,

    /// Configure Codex only
    #[arg(long)]
    pub codex: bool,

    /// Configure Gemini CLI only
    #[arg(long)]
    pub gemini_cli: bool,

    /// Configure Cursor only
    #[arg(long)]
    pub cursor: bool,

    /// Configure Aider only
    #[arg(long)]
    pub aider: bool,
}

impl SetupArgs {
    /// Get the specific agent type if one was requested.
    fn specific_agent(&self) -> Option<AgentType> {
        if self.claude_code {
            Some(AgentType::ClaudeCode)
        } else if self.codex {
            Some(AgentType::Codex)
        } else if self.gemini_cli {
            Some(AgentType::GeminiCli)
        } else if self.cursor {
            Some(AgentType::Cursor)
        } else if self.aider {
            Some(AgentType::Aider)
        } else {
            None
        }
    }
}

/// Type of setup action.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SetupActionType {
    /// Create SKILL.md in project
    CreateSkillMd,
    /// Configure shell completion hook
    ConfigureShellHook,
    /// Configure MCP server for agent
    ConfigureMcpServer,
    /// Create agent-specific config
    CreateAgentConfig,
    /// Update .ms/ directory
    UpdateProjectMarker,
    /// Generate shell tab completions
    GenerateCompletions,
    /// Install pre-commit hook
    InstallPreCommitHook,
}

/// Status of a setup action.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionStatus {
    /// Action is pending
    Pending,
    /// Would create (dry-run)
    WouldCreate,
    /// Would update (dry-run with existing)
    WouldUpdate,
    /// Successfully created
    Created,
    /// Successfully updated
    Updated,
    /// Skipped (already configured)
    Skipped,
    /// Failed with error
    Failed(String),
}

/// A single setup action.
#[derive(Debug, Clone, Serialize)]
pub struct SetupAction {
    /// Agent this action is for (if applicable)
    pub agent: Option<AgentType>,
    /// Type of action
    pub action_type: SetupActionType,
    /// Target path for the action
    pub target_path: PathBuf,
    /// Status of the action
    pub status: ActionStatus,
    /// Human-readable description
    pub description: String,
}

impl SetupAction {
    fn new(
        action_type: SetupActionType,
        target_path: PathBuf,
        description: impl Into<String>,
    ) -> Self {
        Self {
            agent: None,
            action_type,
            target_path,
            status: ActionStatus::Pending,
            description: description.into(),
        }
    }

    fn with_agent(mut self, agent: AgentType) -> Self {
        self.agent = Some(agent);
        self
    }

    fn with_status(mut self, status: ActionStatus) -> Self {
        self.status = status;
        self
    }
}

/// Complete setup report.
#[derive(Debug, Clone, Serialize)]
pub struct SetupReport {
    /// Agents that were detected
    pub detected_agents: Vec<DetectedAgent>,
    /// Actions that were taken/planned
    pub actions_taken: Vec<SetupAction>,
    /// Summary statistics
    pub summary: SetupSummary,
    /// Suggested next steps
    pub next_steps: Vec<String>,
}

/// Summary of setup actions.
#[derive(Debug, Clone, Serialize)]
pub struct SetupSummary {
    /// Number of agents configured
    pub agents_configured: usize,
    /// Number of files created
    pub files_created: usize,
    /// Number of files updated
    pub files_updated: usize,
    /// Whether completions were installed
    pub completions_installed: bool,
    /// Whether pre-commit hook was installed
    pub hooks_installed: bool,
    /// Number of errors
    pub errors: usize,
}

impl SetupSummary {
    fn from_actions(actions: &[SetupAction]) -> Self {
        let files_created = actions
            .iter()
            .filter(|a| matches!(a.status, ActionStatus::Created))
            .count();
        let files_updated = actions
            .iter()
            .filter(|a| matches!(a.status, ActionStatus::Updated))
            .count();
        let completions_installed = actions.iter().any(|a| {
            matches!(a.action_type, SetupActionType::GenerateCompletions)
                && matches!(a.status, ActionStatus::Created)
        });
        let hooks_installed = actions.iter().any(|a| {
            matches!(a.action_type, SetupActionType::InstallPreCommitHook)
                && matches!(a.status, ActionStatus::Created)
        });
        let errors = actions
            .iter()
            .filter(|a| matches!(a.status, ActionStatus::Failed(_)))
            .count();
        let agents_configured = actions
            .iter()
            .filter(|a| {
                a.agent.is_some()
                    && matches!(a.status, ActionStatus::Created | ActionStatus::Updated)
            })
            .map(|a| a.agent)
            .collect::<std::collections::HashSet<_>>()
            .len();

        Self {
            agents_configured,
            files_created,
            files_updated,
            completions_installed,
            hooks_installed,
            errors,
        }
    }
}

/// Run the setup command.
pub fn run(ctx: &AppContext, args: &SetupArgs) -> Result<()> {
    let format = ctx.output_format;

    // Handle status check
    if args.status {
        return run_status(format);
    }

    // Handle completions-only mode
    if args.completions {
        return run_completions_only(format, args);
    }

    // Handle uninstall
    if args.uninstall {
        return run_uninstall(format, args);
    }

    // Main setup flow
    run_setup(ctx, format, args)
}

/// Show current integration status.
fn run_status(format: OutputFormat) -> Result<()> {
    info!("Checking agent integration status");

    let service = AgentDetectionService::new();
    let summary = service.summary();
    let agents = service.detect_all();

    if format == OutputFormat::Human {
        println!("{}", "Agent Integration Status".bold());
        println!();

        if agents.is_empty() {
            println!("No AI coding agents detected.");
            println!();
            println!("Supported agents:");
            for agent in AgentType::all() {
                println!("  - {}", agent.display_name());
            }
        } else {
            println!(
                "Found {} agent{}:",
                summary.total_detected,
                if summary.total_detected == 1 { "" } else { "s" }
            );
            println!();

            for agent in &agents {
                let status_icon = match agent.integration_status {
                    IntegrationStatus::FullyConfigured => "✓".green(),
                    IntegrationStatus::PartiallyConfigured => "◐".yellow(),
                    IntegrationStatus::Outdated => "!".yellow(),
                    IntegrationStatus::NotConfigured => "○".dimmed(),
                };

                let version_str = agent
                    .version
                    .as_ref()
                    .map(|v| format!(" v{}", v))
                    .unwrap_or_default();

                println!(
                    "  {} {}{} - {:?}",
                    status_icon,
                    agent.agent_type.display_name(),
                    version_str,
                    agent.integration_status
                );
            }

            println!();
            if summary.needs_configuration > 0 {
                println!(
                    "{} Run {} to configure all agents",
                    "→".cyan(),
                    "ms setup".bold()
                );
            } else {
                println!("{} All agents fully configured", "✓".green());
            }
        }
    } else {
        emit_json(&robot_ok(serde_json::json!({
            "agents": agents,
            "summary": summary,
        })))?;
    }

    Ok(())
}

/// Generate shell completions only.
fn run_completions_only(format: OutputFormat, args: &SetupArgs) -> Result<()> {
    info!("Generating shell completions");

    let shell = args.shell.or_else(detect_user_shell);

    let Some(shell) = shell else {
        return Err(MsError::Config(
            "Could not detect shell. Use --shell to specify.".into(),
        ));
    };

    let actions = vec![setup_shell_completions(shell, args.dry_run)?];
    let summary = SetupSummary::from_actions(&actions);

    if format == OutputFormat::Human {
        for action in &actions {
            print_action(action);
        }
        println!();
        if summary.completions_installed {
            println!(
                "{} Shell completions installed for {:?}",
                "✓".green(),
                shell
            );
        }
    } else {
        emit_json(&robot_ok(serde_json::json!({
            "actions": actions,
            "summary": summary,
        })))?;
    }

    Ok(())
}

/// Remove ms integrations.
fn run_uninstall(format: OutputFormat, args: &SetupArgs) -> Result<()> {
    info!("Removing ms integrations");

    let mut actions = Vec::new();

    // Remove SKILL.md if present
    let project_root = std::env::current_dir()?;
    let skill_md = project_root.join("SKILL.md");

    if skill_md.exists() {
        if args.dry_run {
            actions.push(
                SetupAction::new(SetupActionType::CreateSkillMd, skill_md, "Remove SKILL.md")
                    .with_status(ActionStatus::WouldUpdate),
            );
        } else {
            fs::remove_file(&skill_md)?;
            actions.push(
                SetupAction::new(SetupActionType::CreateSkillMd, skill_md, "Removed SKILL.md")
                    .with_status(ActionStatus::Updated),
            );
        }
    }

    // Remove pre-commit hook if present
    let git_hook = project_root.join(".git/hooks/pre-commit");
    if git_hook.exists() {
        let content = fs::read_to_string(&git_hook).unwrap_or_default();
        if content.contains("ms pre-commit") {
            if args.dry_run {
                actions.push(
                    SetupAction::new(
                        SetupActionType::InstallPreCommitHook,
                        git_hook,
                        "Remove ms pre-commit hook",
                    )
                    .with_status(ActionStatus::WouldUpdate),
                );
            } else {
                fs::remove_file(&git_hook)?;
                actions.push(
                    SetupAction::new(
                        SetupActionType::InstallPreCommitHook,
                        git_hook,
                        "Removed ms pre-commit hook",
                    )
                    .with_status(ActionStatus::Updated),
                );
            }
        }
    }

    if format == OutputFormat::Human {
        if actions.is_empty() {
            println!("No ms integrations found to remove.");
        } else {
            for action in &actions {
                print_action(action);
            }
            println!();
            println!("{} ms integrations removed", "✓".green());
        }
    } else {
        emit_json(&robot_ok(serde_json::json!({
            "actions": actions,
            "uninstalled": !actions.is_empty(),
        })))?;
    }

    Ok(())
}

/// Main setup flow.
fn run_setup(_ctx: &AppContext, format: OutputFormat, args: &SetupArgs) -> Result<()> {
    info!("Running ms setup");

    // Detect agents
    let service = AgentDetectionService::new();
    let detected_agents = if let Some(specific) = args.specific_agent() {
        service
            .detect_by_type(specific)
            .into_iter()
            .collect::<Vec<_>>()
    } else {
        service.detect_all()
    };

    let detection_summary = DetectionSummary::from_agents(&detected_agents);

    // Early return for detect-only mode
    if args.detect_only {
        if format == OutputFormat::Human {
            println!("{}", "Detected Agents".bold());
            println!();
            if detected_agents.is_empty() {
                println!("No AI coding agents detected.");
            } else {
                for agent in &detected_agents {
                    let version = agent.version.as_deref().unwrap_or("unknown");
                    println!(
                        "  {} {} (v{}) via {}",
                        "•".cyan(),
                        agent.agent_type.display_name(),
                        version,
                        agent.detected_via
                    );
                }
            }
        } else {
            emit_json(&robot_ok(serde_json::json!({
                "detected_agents": detected_agents,
                "summary": detection_summary,
            })))?;
        }
        return Ok(());
    }

    // Collect setup actions
    let mut actions = Vec::new();
    let project_root = std::env::current_dir()?;

    // 1. Create SKILL.md
    let skill_md_action = setup_skill_md(&project_root, args)?;
    actions.push(skill_md_action);

    // 2. Install shell completions
    if let Some(shell) = detect_user_shell() {
        let completions_action = setup_shell_completions(shell, args.dry_run)?;
        actions.push(completions_action);
    }

    // 3. Install pre-commit hook if in git repo
    if project_root.join(".git").exists() {
        let hook_action = setup_pre_commit_hook(&project_root, args.dry_run)?;
        actions.push(hook_action);
    }

    // 4. Agent-specific setup
    for agent in &detected_agents {
        if agent.integration_status.needs_configuration() || args.force {
            let agent_actions = setup_agent(agent, &project_root, args)?;
            actions.extend(agent_actions);
        }
    }

    // Build summary
    let summary = SetupSummary::from_actions(&actions);

    // Generate next steps
    let mut next_steps = Vec::new();
    if summary.errors > 0 {
        next_steps.push("Review errors above and fix any issues".to_string());
    }
    if !detected_agents.is_empty() && summary.agents_configured == 0 {
        next_steps.push("Run 'ms setup --force' to reconfigure agents".to_string());
    }
    if summary.completions_installed {
        next_steps.push("Restart your shell to enable completions".to_string());
    }
    if actions
        .iter()
        .any(|a| matches!(a.action_type, SetupActionType::CreateSkillMd))
    {
        next_steps.push("Commit SKILL.md to your repository".to_string());
    }

    // Create report
    let report = SetupReport {
        detected_agents,
        actions_taken: actions,
        summary,
        next_steps,
    };

    // Output
    if format == OutputFormat::Human {
        print_setup_report(&report, args.dry_run);
    } else {
        emit_json(&robot_ok(report))?;
    }

    Ok(())
}

/// Setup SKILL.md file.
fn setup_skill_md(project_root: &Path, args: &SetupArgs) -> Result<SetupAction> {
    let skill_md_path = project_root.join("SKILL.md");

    // Check if exists and not forcing
    if skill_md_path.exists() && !args.force {
        return Ok(SetupAction::new(
            SetupActionType::CreateSkillMd,
            skill_md_path,
            "SKILL.md already exists",
        )
        .with_status(ActionStatus::Skipped));
    }

    let status = if args.dry_run {
        if skill_md_path.exists() {
            ActionStatus::WouldUpdate
        } else {
            ActionStatus::WouldCreate
        }
    } else {
        let generator = SkillMdGenerator::new();
        let content = generator.generate();
        fs::write(&skill_md_path, content)?;
        ActionStatus::Created
    };

    Ok(SetupAction::new(
        SetupActionType::CreateSkillMd,
        skill_md_path,
        "Generate SKILL.md for AI agent discovery",
    )
    .with_status(status))
}

/// Setup shell completions.
fn setup_shell_completions(shell: Shell, dry_run: bool) -> Result<SetupAction> {
    let completions_dir = match shell {
        Shell::Bash => dirs::home_dir().map(|h| h.join(".local/share/bash-completion/completions")),
        Shell::Zsh => dirs::home_dir().map(|h| h.join(".zfunc")),
        Shell::Fish => dirs::config_dir().map(|c| c.join("fish/completions")),
        _ => None,
    };

    let Some(dir) = completions_dir else {
        return Ok(SetupAction::new(
            SetupActionType::GenerateCompletions,
            PathBuf::new(),
            format!("No standard completions directory for {:?}", shell),
        )
        .with_status(ActionStatus::Skipped));
    };

    let filename = match shell {
        Shell::Bash => "ms.bash",
        Shell::Zsh => "_ms",
        Shell::Fish => "ms.fish",
        _ => "ms",
    };

    let target_path = dir.join(filename);

    if dry_run {
        return Ok(SetupAction::new(
            SetupActionType::GenerateCompletions,
            target_path,
            format!("Generate {:?} completions", shell),
        )
        .with_status(ActionStatus::WouldCreate));
    }

    // Generate completions
    fs::create_dir_all(&dir)?;
    let mut file = fs::File::create(&target_path)?;
    let mut cmd = crate::cli::Cli::command();
    generate(shell, &mut cmd, "ms", &mut file);

    Ok(SetupAction::new(
        SetupActionType::GenerateCompletions,
        target_path,
        format!("Generated {:?} completions", shell),
    )
    .with_status(ActionStatus::Created))
}

/// Setup pre-commit hook.
fn setup_pre_commit_hook(project_root: &Path, dry_run: bool) -> Result<SetupAction> {
    let git_dir = project_root.join(".git");
    if !git_dir.exists() {
        return Ok(SetupAction::new(
            SetupActionType::InstallPreCommitHook,
            PathBuf::new(),
            "Not a git repository",
        )
        .with_status(ActionStatus::Skipped));
    }

    let hooks_dir = git_dir.join("hooks");
    let pre_commit = hooks_dir.join("pre-commit");

    // Check if hook already exists with our content
    if pre_commit.exists() {
        let content = fs::read_to_string(&pre_commit).unwrap_or_default();
        if content.contains("ms pre-commit") {
            return Ok(SetupAction::new(
                SetupActionType::InstallPreCommitHook,
                pre_commit,
                "ms pre-commit hook already installed",
            )
            .with_status(ActionStatus::Skipped));
        }
    }

    if dry_run {
        return Ok(SetupAction::new(
            SetupActionType::InstallPreCommitHook,
            pre_commit,
            "Install ms pre-commit hook",
        )
        .with_status(ActionStatus::WouldCreate));
    }

    fs::create_dir_all(&hooks_dir)?;

    let hook_content = r#"#!/bin/bash
# ms pre-commit hook - validates skills and runs UBS
exec ms pre-commit "$@"
"#;

    fs::write(&pre_commit, hook_content)?;

    // Make executable on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&pre_commit, fs::Permissions::from_mode(0o755))?;
    }

    Ok(SetupAction::new(
        SetupActionType::InstallPreCommitHook,
        pre_commit,
        "Installed ms pre-commit hook",
    )
    .with_status(ActionStatus::Created))
}

/// Setup agent-specific configuration.
fn setup_agent(
    agent: &DetectedAgent,
    _project_root: &Path,
    args: &SetupArgs,
) -> Result<Vec<SetupAction>> {
    let mut actions = Vec::new();

    debug!(agent = ?agent.agent_type, "Setting up agent");

    // Get agent-specific config path
    let service = AgentDetectionService::new();
    let detector = service.get_detector(agent.agent_type);

    if let Some(detector) = detector {
        if let Some(config_path) = detector.get_config_path() {
            let action = setup_agent_config(agent.agent_type, &config_path, args)?;
            actions.push(action);
        }
    }

    Ok(actions)
}

/// Setup agent configuration file.
fn setup_agent_config(
    agent_type: AgentType,
    config_path: &Path,
    args: &SetupArgs,
) -> Result<SetupAction> {
    // For now, we just note that we would configure this agent
    // Full MCP server integration would require modifying agent configs

    let status = if args.dry_run {
        ActionStatus::WouldUpdate
    } else {
        // In a full implementation, we would modify the config here
        // For now, we mark it as skipped since we don't want to
        // modify third-party config files without explicit consent
        ActionStatus::Skipped
    };

    Ok(SetupAction::new(
        SetupActionType::ConfigureMcpServer,
        config_path.to_path_buf(),
        format!("Configure MCP server for {}", agent_type.display_name()),
    )
    .with_agent(agent_type)
    .with_status(status))
}

/// Detect the user's shell.
fn detect_user_shell() -> Option<Shell> {
    std::env::var("SHELL").ok().and_then(|s| {
        if s.contains("bash") {
            Some(Shell::Bash)
        } else if s.contains("zsh") {
            Some(Shell::Zsh)
        } else if s.contains("fish") {
            Some(Shell::Fish)
        } else {
            None
        }
    })
}

/// Print a setup action in human format.
fn print_action(action: &SetupAction) {
    let icon = match &action.status {
        ActionStatus::Created => "✓".green(),
        ActionStatus::Updated => "↺".blue(),
        ActionStatus::WouldCreate => "○".yellow(),
        ActionStatus::WouldUpdate => "○".yellow(),
        ActionStatus::Skipped => "-".dimmed(),
        ActionStatus::Failed(_) => "✗".red(),
        ActionStatus::Pending => "?".dimmed(),
    };

    let agent_str = action
        .agent
        .map(|a| format!("[{}] ", a.display_name()))
        .unwrap_or_default();

    println!("  {} {}{}", icon, agent_str, action.description);

    if let ActionStatus::Failed(err) = &action.status {
        println!("    {}", err.red());
    }

    if !action.target_path.as_os_str().is_empty() {
        println!(
            "    → {}",
            action.target_path.display().to_string().dimmed()
        );
    }
}

/// Print the complete setup report.
fn print_setup_report(report: &SetupReport, dry_run: bool) {
    if dry_run {
        println!("{}", "Setup (dry run)".bold());
    } else {
        println!("{}", "Setup".bold());
    }
    println!();

    // Detected agents
    println!("{}:", "Detected Agents".bold());
    if report.detected_agents.is_empty() {
        println!("  No AI coding agents detected");
    } else {
        for agent in &report.detected_agents {
            let version = agent.version.as_deref().unwrap_or("unknown");
            let status_icon = match agent.integration_status {
                IntegrationStatus::FullyConfigured => "✓".green(),
                IntegrationStatus::PartiallyConfigured => "◐".yellow(),
                IntegrationStatus::Outdated => "!".yellow(),
                IntegrationStatus::NotConfigured => "○".dimmed(),
            };
            println!(
                "  {} {} v{} ({:?})",
                status_icon,
                agent.agent_type.display_name(),
                version,
                agent.integration_status
            );
        }
    }
    println!();

    // Actions
    println!("{}:", "Actions".bold());
    if report.actions_taken.is_empty() {
        println!("  No actions to perform");
    } else {
        for action in &report.actions_taken {
            print_action(action);
        }
    }
    println!();

    // Summary
    let s = &report.summary;
    println!(
        "{}: {} created, {} updated, {} skipped, {} errors",
        "Summary".bold(),
        s.files_created,
        s.files_updated,
        report
            .actions_taken
            .iter()
            .filter(|a| matches!(a.status, ActionStatus::Skipped))
            .count(),
        s.errors
    );

    // Next steps
    if !report.next_steps.is_empty() {
        println!();
        println!("{}:", "Next Steps".bold());
        for step in &report.next_steps {
            println!("  → {}", step);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_skill_md_generator_contains_version() {
        let generator = SkillMdGenerator::new();
        let content = generator.generate();
        assert!(content.contains(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn test_skill_md_generator_structure() {
        let generator = SkillMdGenerator::new();
        let content = generator.generate();
        assert!(content.starts_with("# ms"));
        assert!(content.contains("## Capabilities"));
        assert!(content.contains("## MCP Server"));
    }

    #[test]
    fn test_detect_user_shell() {
        // This test depends on environment
        let _ = detect_user_shell();
    }

    #[test]
    fn test_setup_action_builder() {
        let action = SetupAction::new(
            SetupActionType::CreateSkillMd,
            PathBuf::from("/test/SKILL.md"),
            "Test action",
        )
        .with_agent(AgentType::ClaudeCode)
        .with_status(ActionStatus::Created);

        assert_eq!(action.agent, Some(AgentType::ClaudeCode));
        assert!(matches!(action.status, ActionStatus::Created));
    }

    #[test]
    fn test_setup_summary_from_actions() {
        let actions = vec![
            SetupAction::new(SetupActionType::CreateSkillMd, PathBuf::new(), "test")
                .with_status(ActionStatus::Created),
            SetupAction::new(SetupActionType::GenerateCompletions, PathBuf::new(), "test")
                .with_status(ActionStatus::Created),
            SetupAction::new(
                SetupActionType::InstallPreCommitHook,
                PathBuf::new(),
                "test",
            )
            .with_status(ActionStatus::Skipped),
        ];

        let summary = SetupSummary::from_actions(&actions);
        assert_eq!(summary.files_created, 2);
        assert!(summary.completions_installed);
        assert!(!summary.hooks_installed);
        assert_eq!(summary.errors, 0);
    }

    #[test]
    fn test_setup_pre_commit_hook_dry_run() {
        let temp = TempDir::new().unwrap();
        let git_dir = temp.path().join(".git");
        fs::create_dir_all(&git_dir).unwrap();

        let action = setup_pre_commit_hook(temp.path(), true).unwrap();
        assert!(matches!(action.status, ActionStatus::WouldCreate));
    }

    #[test]
    fn test_setup_pre_commit_hook_no_git() {
        let temp = TempDir::new().unwrap();
        // No .git directory

        let action = setup_pre_commit_hook(temp.path(), false).unwrap();
        assert!(matches!(action.status, ActionStatus::Skipped));
    }

    #[test]
    fn test_setup_skill_md_already_exists() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("SKILL.md"), "existing content").unwrap();

        let args = SetupArgs {
            detect_only: false,
            status: false,
            uninstall: false,
            force: false,
            dry_run: false,
            completions: false,
            shell: None,
            claude_code: false,
            codex: false,
            gemini_cli: false,
            cursor: false,
            aider: false,
        };

        let action = setup_skill_md(temp.path(), &args).unwrap();
        assert!(matches!(action.status, ActionStatus::Skipped));
    }

    #[test]
    fn test_setup_skill_md_force() {
        let temp = TempDir::new().unwrap();
        fs::write(temp.path().join("SKILL.md"), "existing content").unwrap();

        let args = SetupArgs {
            detect_only: false,
            status: false,
            uninstall: false,
            force: true,
            dry_run: false,
            completions: false,
            shell: None,
            claude_code: false,
            codex: false,
            gemini_cli: false,
            cursor: false,
            aider: false,
        };

        let action = setup_skill_md(temp.path(), &args).unwrap();
        assert!(matches!(action.status, ActionStatus::Created));
    }

    #[test]
    fn test_specific_agent_selection() {
        let args = SetupArgs {
            detect_only: false,
            status: false,
            uninstall: false,
            force: false,
            dry_run: false,
            completions: false,
            shell: None,
            claude_code: true,
            codex: false,
            gemini_cli: false,
            cursor: false,
            aider: false,
        };

        assert_eq!(args.specific_agent(), Some(AgentType::ClaudeCode));
    }
}
