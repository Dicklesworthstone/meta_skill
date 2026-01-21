//! SKILL.md auto-generation module.
//!
//! Generates a SKILL.md file that documents ms capabilities for AI coding agents.
//! The generated file follows a standard format that agents can parse to discover
//! available tools and capabilities.

use std::fmt::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

mod templates;

/// Information about an MCP tool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolInfo {
    /// Tool name
    pub name: String,
    /// Tool description
    pub description: String,
}

/// Information about a CLI command.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandInfo {
    /// Command name
    pub name: String,
    /// Command description
    pub description: String,
    /// Whether it supports robot mode (-O json)
    pub robot_mode: bool,
}

/// Generator for SKILL.md content.
pub struct SkillMdGenerator {
    version: String,
    mcp_tools: Vec<McpToolInfo>,
    commands: Vec<CommandInfo>,
}

impl Default for SkillMdGenerator {
    fn default() -> Self {
        Self::new()
    }
}

impl SkillMdGenerator {
    /// Create a new generator with default settings.
    #[must_use]
    pub fn new() -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION").to_string(),
            mcp_tools: collect_mcp_tools(),
            commands: collect_command_info(),
        }
    }

    /// Generate the complete SKILL.md content.
    #[must_use]
    pub fn generate(&self) -> String {
        let mut out = String::with_capacity(8192);

        // Header
        self.write_header(&mut out);

        // Capabilities section
        self.write_capabilities(&mut out);

        // Robot mode section
        self.write_robot_mode(&mut out);

        // MCP section
        self.write_mcp_section(&mut out);

        // Context integration
        self.write_context_section(&mut out);

        // Examples
        self.write_examples(&mut out);

        out
    }

    /// Write the header section.
    fn write_header(&self, out: &mut String) {
        writeln!(out, "# ms — Meta Skill CLI").unwrap();
        writeln!(out).unwrap();
        writeln!(
            out,
            "> Local-first skill management platform for AI coding agents"
        )
        .unwrap();
        writeln!(out).unwrap();
        writeln!(out, "> Version: {}", self.version).unwrap();
        writeln!(out).unwrap();
    }

    /// Write the capabilities section.
    fn write_capabilities(&self, out: &mut String) {
        writeln!(out, "## Capabilities").unwrap();
        writeln!(out).unwrap();

        // Core commands
        writeln!(out, "### Core Commands").unwrap();
        for cmd in &self.commands {
            writeln!(out, "- **{}**: {}", cmd.name, cmd.description).unwrap();
        }
        writeln!(out).unwrap();
    }

    /// Write the robot mode section.
    fn write_robot_mode(&self, out: &mut String) {
        writeln!(out, "### Robot Mode").unwrap();
        writeln!(out, "All commands support `-O json` for JSON output:").unwrap();
        writeln!(out, "```bash").unwrap();

        // Generate examples for robot-mode enabled commands
        let examples = [
            ("search", "search \"query\" -O json"),
            ("load", "load skill-name -O json --level overview"),
            ("suggest", "suggest -O json"),
            ("list", "list -O json"),
        ];

        for (_, example) in examples {
            writeln!(out, "ms {example}").unwrap();
        }

        writeln!(out, "```").unwrap();
        writeln!(out).unwrap();
    }

    /// Write the MCP section.
    fn write_mcp_section(&self, out: &mut String) {
        writeln!(out, "## MCP Server").unwrap();
        writeln!(out, "Start MCP server for native tool integration:").unwrap();
        writeln!(out, "```bash").unwrap();
        writeln!(
            out,
            "ms mcp serve           # stdio transport (Claude Code)"
        )
        .unwrap();
        writeln!(out, "ms mcp serve --tcp-port 8080  # HTTP transport").unwrap();
        writeln!(out, "```").unwrap();
        writeln!(out).unwrap();

        writeln!(out, "### Available MCP Tools").unwrap();
        for tool in &self.mcp_tools {
            writeln!(out, "- `{}` - {}", tool.name, tool.description).unwrap();
        }
        writeln!(out).unwrap();
    }

    /// Write the context integration section.
    fn write_context_section(&self, out: &mut String) {
        writeln!(out, "## Context Integration").unwrap();
        writeln!(
            out,
            "- Reads `.ms/config.toml` for project-specific settings"
        )
        .unwrap();
        writeln!(
            out,
            "- Respects `NO_COLOR` and `FORCE_COLOR` environment variables"
        )
        .unwrap();
        writeln!(out, "- Auto-detects project type from marker files").unwrap();
        writeln!(out).unwrap();
    }

    /// Write the examples section.
    fn write_examples(&self, out: &mut String) {
        writeln!(out, "## Examples").unwrap();
        writeln!(out, "```bash").unwrap();
        writeln!(out, "# Find skills for error handling").unwrap();
        writeln!(out, "ms search \"rust error handling\"").unwrap();
        writeln!(out).unwrap();
        writeln!(out, "# Load with full content").unwrap();
        writeln!(out, "ms load rust-error-patterns --level full").unwrap();
        writeln!(out).unwrap();
        writeln!(out, "# Get suggestions for current project").unwrap();
        writeln!(out, "ms suggest --explain").unwrap();
        writeln!(out).unwrap();
        writeln!(out, "# Validate a skill file").unwrap();
        writeln!(out, "ms lint SKILL.md").unwrap();
        writeln!(out).unwrap();
        writeln!(out, "# Run health checks").unwrap();
        writeln!(out, "ms doctor").unwrap();
        writeln!(out, "```").unwrap();
    }

    /// Write the SKILL.md content to a file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn write_to_file(&self, path: &Path) -> std::io::Result<()> {
        let content = self.generate();
        std::fs::write(path, content)
    }

    /// Get the version string.
    #[must_use]
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Get the MCP tools.
    #[must_use]
    pub fn mcp_tools(&self) -> &[McpToolInfo] {
        &self.mcp_tools
    }

    /// Get the CLI commands.
    #[must_use]
    pub fn commands(&self) -> &[CommandInfo] {
        &self.commands
    }
}

/// Collect MCP tool information from the MCP module.
fn collect_mcp_tools() -> Vec<McpToolInfo> {
    // These correspond to the tools defined in src/cli/commands/mcp.rs define_tools()
    vec![
        McpToolInfo {
            name: "search".to_string(),
            description: "Search for skills using BM25 full-text search".to_string(),
        },
        McpToolInfo {
            name: "load".to_string(),
            description: "Load a skill by ID".to_string(),
        },
        McpToolInfo {
            name: "suggest".to_string(),
            description: "Get context-aware skill suggestions".to_string(),
        },
        McpToolInfo {
            name: "evidence".to_string(),
            description: "View provenance evidence for skill rules".to_string(),
        },
        McpToolInfo {
            name: "list".to_string(),
            description: "List all indexed skills".to_string(),
        },
        McpToolInfo {
            name: "show".to_string(),
            description: "Show detailed information about a specific skill".to_string(),
        },
        McpToolInfo {
            name: "doctor".to_string(),
            description: "Run health checks on the ms installation".to_string(),
        },
        McpToolInfo {
            name: "lint".to_string(),
            description: "Lint a skill file for validation issues".to_string(),
        },
        McpToolInfo {
            name: "feedback".to_string(),
            description: "Record skill feedback".to_string(),
        },
        McpToolInfo {
            name: "index".to_string(),
            description: "Re-index skills".to_string(),
        },
    ]
}

/// Collect CLI command information from clap metadata.
fn collect_command_info() -> Vec<CommandInfo> {
    // Core commands that agents typically use
    vec![
        CommandInfo {
            name: "search".to_string(),
            description: "Hybrid BM25 + semantic skill search".to_string(),
            robot_mode: true,
        },
        CommandInfo {
            name: "load".to_string(),
            description: "Progressive disclosure skill loading with token packing".to_string(),
            robot_mode: true,
        },
        CommandInfo {
            name: "suggest".to_string(),
            description: "Context-aware recommendations with Thompson sampling".to_string(),
            robot_mode: true,
        },
        CommandInfo {
            name: "build".to_string(),
            description: "Extract skills from CASS sessions".to_string(),
            robot_mode: true,
        },
        CommandInfo {
            name: "list".to_string(),
            description: "List all indexed skills".to_string(),
            robot_mode: true,
        },
        CommandInfo {
            name: "show".to_string(),
            description: "Show skill details".to_string(),
            robot_mode: true,
        },
        CommandInfo {
            name: "lint".to_string(),
            description: "Validate skill files for issues".to_string(),
            robot_mode: true,
        },
        CommandInfo {
            name: "doctor".to_string(),
            description: "Health checks and diagnostics".to_string(),
            robot_mode: true,
        },
        CommandInfo {
            name: "index".to_string(),
            description: "Index skills from configured paths".to_string(),
            robot_mode: true,
        },
        CommandInfo {
            name: "setup".to_string(),
            description: "Auto-configure ms for AI coding agents".to_string(),
            robot_mode: true,
        },
    ]
}

/// Generate SKILL.md for a project directory.
///
/// This is a convenience function that creates a generator and writes to the
/// standard location.
///
/// # Errors
///
/// Returns an error if the file cannot be written.
pub fn generate_skill_md_for_project(project_root: &Path) -> std::io::Result<std::path::PathBuf> {
    let generator = SkillMdGenerator::new();
    let skill_md_path = project_root.join("SKILL.md");
    generator.write_to_file(&skill_md_path)?;
    Ok(skill_md_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_skill_md_contains_version() {
        let generator = SkillMdGenerator::new();
        let content = generator.generate();
        assert!(content.contains(env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn test_skill_md_header_title() {
        let generator = SkillMdGenerator::new();
        let content = generator.generate();
        assert!(content.starts_with("# ms — Meta Skill CLI"));
    }

    #[test]
    fn test_skill_md_header_description() {
        let generator = SkillMdGenerator::new();
        let content = generator.generate();
        assert!(content.contains("Local-first skill management platform for AI coding agents"));
    }

    #[test]
    fn test_skill_md_contains_mcp_tools() {
        let generator = SkillMdGenerator::new();
        let content = generator.generate();
        assert!(content.contains("search"));
        assert!(content.contains("load"));
        assert!(content.contains("suggest"));
        assert!(content.contains("lint"));
    }

    #[test]
    fn test_skill_md_contains_capabilities_section() {
        let generator = SkillMdGenerator::new();
        let content = generator.generate();
        assert!(content.contains("## Capabilities"));
        assert!(content.contains("### Core Commands"));
    }

    #[test]
    fn test_skill_md_contains_robot_mode_section() {
        let generator = SkillMdGenerator::new();
        let content = generator.generate();
        assert!(content.contains("### Robot Mode"));
        assert!(content.contains("```bash"));
    }

    #[test]
    fn test_skill_md_valid_markdown() {
        let generator = SkillMdGenerator::new();
        let content = generator.generate();

        // Check structure
        assert!(content.starts_with("# ms"));
        assert!(content.contains("## Capabilities"));
        assert!(content.contains("## MCP Server"));
        assert!(content.contains("## Examples"));
    }

    #[test]
    fn test_skill_md_robot_mode_examples() {
        let generator = SkillMdGenerator::new();
        let content = generator.generate();
        assert!(content.contains("-O json"));
    }

    #[test]
    fn test_skill_md_robot_mode_examples_include_all() {
        let generator = SkillMdGenerator::new();
        let content = generator.generate();
        for (_, example) in [
            ("search", "ms search \"query\" -O json"),
            ("load", "ms load skill-name -O json --level overview"),
            ("suggest", "ms suggest -O json"),
            ("list", "ms list -O json"),
        ] {
            assert!(content.contains(example));
        }
    }

    #[test]
    fn test_skill_md_mcp_section() {
        let generator = SkillMdGenerator::new();
        let content = generator.generate();
        assert!(content.contains("### Available MCP Tools"));
        assert!(content.contains("ms mcp serve"));
    }

    #[test]
    fn test_skill_md_mcp_section_examples_present() {
        let generator = SkillMdGenerator::new();
        let content = generator.generate();
        assert!(content.contains("ms mcp serve --tcp-port 8080"));
    }

    #[test]
    fn test_skill_md_context_section_present() {
        let generator = SkillMdGenerator::new();
        let content = generator.generate();
        assert!(content.contains("## Context Integration"));
        assert!(content.contains("Reads `.ms/config.toml`"));
        assert!(content.contains("Respects `NO_COLOR`"));
        assert!(content.contains("Auto-detects project type"));
    }

    #[test]
    fn test_skill_md_examples_section_present() {
        let generator = SkillMdGenerator::new();
        let content = generator.generate();
        assert!(content.contains("## Examples"));
    }

    #[test]
    fn test_skill_md_examples_include_search() {
        let generator = SkillMdGenerator::new();
        let content = generator.generate();
        assert!(content.contains("ms search \"rust error handling\""));
    }

    #[test]
    fn test_skill_md_examples_include_load() {
        let generator = SkillMdGenerator::new();
        let content = generator.generate();
        assert!(content.contains("ms load rust-error-patterns --level full"));
    }

    #[test]
    fn test_skill_md_examples_include_suggest() {
        let generator = SkillMdGenerator::new();
        let content = generator.generate();
        assert!(content.contains("ms suggest --explain"));
    }

    #[test]
    fn test_skill_md_examples_include_lint() {
        let generator = SkillMdGenerator::new();
        let content = generator.generate();
        assert!(content.contains("ms lint SKILL.md"));
    }

    #[test]
    fn test_skill_md_examples_include_doctor() {
        let generator = SkillMdGenerator::new();
        let content = generator.generate();
        assert!(content.contains("ms doctor"));
    }

    #[test]
    fn test_mcp_tools_collection() {
        let tools = collect_mcp_tools();
        assert!(!tools.is_empty());
        assert!(tools.iter().any(|t| t.name == "search"));
        assert!(tools.iter().any(|t| t.name == "lint"));
    }

    #[test]
    fn test_mcp_tools_collection_unique_names() {
        let tools = collect_mcp_tools();
        let mut names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), tools.len());
    }

    #[test]
    fn test_mcp_tools_have_descriptions() {
        let tools = collect_mcp_tools();
        assert!(tools.iter().all(|t| !t.description.trim().is_empty()));
    }

    #[test]
    fn test_skill_md_includes_all_mcp_tool_lines() {
        let generator = SkillMdGenerator::new();
        let content = generator.generate();
        for tool in collect_mcp_tools() {
            let line = format!("- `{}` - {}", tool.name, tool.description);
            assert!(content.contains(&line));
        }
    }

    #[test]
    fn test_commands_collection() {
        let commands = collect_command_info();
        assert!(!commands.is_empty());
        assert!(commands.iter().any(|c| c.name == "search"));
        assert!(commands.iter().any(|c| c.name == "setup"));
    }

    #[test]
    fn test_commands_collection_unique_names() {
        let commands = collect_command_info();
        let mut names: Vec<&str> = commands.iter().map(|c| c.name.as_str()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), commands.len());
    }

    #[test]
    fn test_commands_have_descriptions() {
        let commands = collect_command_info();
        assert!(commands.iter().all(|c| !c.description.trim().is_empty()));
    }

    #[test]
    fn test_commands_all_robot_mode_true() {
        let commands = collect_command_info();
        assert!(commands.iter().all(|c| c.robot_mode));
    }

    #[test]
    fn test_skill_md_includes_all_command_lines() {
        let generator = SkillMdGenerator::new();
        let content = generator.generate();
        for cmd in collect_command_info() {
            let line = format!("- **{}**: {}", cmd.name, cmd.description);
            assert!(content.contains(&line));
        }
    }

    #[test]
    fn test_generator_accessors() {
        let generator = SkillMdGenerator::new();
        assert!(!generator.version().is_empty());
        assert!(!generator.mcp_tools().is_empty());
        assert!(!generator.commands().is_empty());
    }

    #[test]
    fn test_generator_default() {
        let generator = SkillMdGenerator::default();
        assert!(!generator.version().is_empty());
    }

    #[test]
    fn test_write_to_file_writes_content() {
        let generator = SkillMdGenerator::new();
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("SKILL.md");
        generator.write_to_file(&path).expect("write");
        let contents = std::fs::read_to_string(&path).expect("read");
        assert_eq!(contents, generator.generate());
    }

    #[test]
    fn test_generate_skill_md_for_project_creates_file() {
        let dir = tempdir().expect("tempdir");
        let path = generate_skill_md_for_project(dir.path()).expect("generate");
        assert_eq!(path, dir.path().join("SKILL.md"));
        assert!(path.exists());
    }

    #[test]
    fn test_generate_skill_md_for_project_content_matches_generator() {
        let dir = tempdir().expect("tempdir");
        let path = generate_skill_md_for_project(dir.path()).expect("generate");
        let contents = std::fs::read_to_string(&path).expect("read");
        let generator = SkillMdGenerator::new();
        assert_eq!(contents, generator.generate());
    }
}
