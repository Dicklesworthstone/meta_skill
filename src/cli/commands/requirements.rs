//! ms requirements - Check environment requirements
//!
//! Checks for the presence of external tools and dependencies
//! that ms interacts with (git, cass, bd, etc.).

use std::path::PathBuf;
use std::process::Command;

use clap::Args;
use colored::Colorize;
use which::which;

use crate::app::AppContext;
use crate::cli::output::OutputFormat;
use crate::error::Result;

#[derive(Args, Debug)]
pub struct RequirementsArgs {
    /// Skill to check requirements for (optional, checks specific skill deps)
    pub skill: Option<String>,

    /// Output format: text, json
    #[arg(long, default_value = "text")]
    pub format: String,

    /// Check all indexed skills
    #[arg(long)]
    pub all: bool,
}

#[derive(Debug, serde::Serialize)]
struct RequirementCheck {
    name: String,
    bin: String,
    path: Option<PathBuf>,
    version: Option<String>,
    required: bool,
    present: bool,
    error: Option<String>,
}

impl RequirementCheck {
    fn new(name: &str, bin: &str, required: bool) -> Self {
        Self {
            name: name.to_string(),
            bin: bin.to_string(),
            path: None,
            version: None,
            required,
            present: false,
            error: None,
        }
    }

    fn check(&mut self) {
        match which(&self.bin) {
            Ok(path) => {
                self.path = Some(path);
                self.present = true;

                // Try to get version
                if let Ok(output) = Command::new(&self.bin).arg("--version").output() {
                    if output.status.success() {
                        let v = String::from_utf8_lossy(&output.stdout).trim().to_string();
                        // Clean up version string (e.g. "git version 2.34.1" -> "2.34.1")
                        let clean_v = if v.to_lowercase().starts_with(&self.bin)
                            || v.to_lowercase().starts_with("git version")
                        {
                            v.split_whitespace().last().unwrap_or(&v).to_string()
                        } else {
                            v
                        };
                        self.version = Some(clean_v);
                    }
                }
            }
            Err(e) => {
                self.present = false;
                self.error = Some(e.to_string());
            }
        }
    }
}

pub fn run(ctx: &AppContext, args: &RequirementsArgs) -> Result<()> {
    if let Some(_skill) = &args.skill {
        // TODO: Implement skill-specific requirements check (read from SKILL.md)
        if ctx.output_format == OutputFormat::Human {
            println!("Skill-specific requirements not yet implemented.");
        }
        return Ok(());
    }

    let mut checks = vec![
        RequirementCheck::new("Git Version Control", "git", true),
        RequirementCheck::new("CASS (Context Aware Semantic Search)", "cass", false),
        RequirementCheck::new("Beads Issue Tracker", "bd", false),
        RequirementCheck::new("Ripgrep", "rg", false),
        RequirementCheck::new("Tar Archiver", "tar", true),
    ];

    for check in &mut checks {
        check.check();
    }

    if ctx.output_format != OutputFormat::Human || args.format == "json" {
        println!("{}", serde_json::to_string_pretty(&checks)?);
        return Ok(());
    }

    println!("{}", "System Requirements Check".bold());
    println!("{}", "─".repeat(60));
    println!();

    let mut all_passed = true;

    for check in &checks {
        let status = if check.present {
            "✓".green()
        } else if check.required {
            all_passed = false;
            "✗".red()
        } else {
            "-".yellow()
        };

        let path_str = check
            .path
            .as_ref().map_or_else(|| "Not found".dimmed().to_string(), |p| p.to_string_lossy().to_string());

        let version_str = check
            .version
            .as_ref()
            .map(|v| format!("(v{v})"))
            .unwrap_or_default();

        println!("{} {} {}", status, check.name.bold(), version_str);
        println!("    Bin:  {}", check.bin.cyan());
        println!("    Path: {path_str}");
        if !check.present && !check.required {
            println!("    Note: Optional dependency");
        }
        println!();
    }

    if all_passed {
        println!("\n{} All required dependencies found.", "Success:".green());
    } else {
        println!("\n{} Missing required dependencies.", "Error:".red());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Parser, Subcommand};

    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: TestCommand,
    }

    #[derive(Subcommand)]
    enum TestCommand {
        Requirements(RequirementsArgs),
    }

    #[test]
    fn parse_requirements_defaults() {
        let parsed = TestCli::parse_from(["test", "requirements"]);
        let TestCommand::Requirements(args) = parsed.cmd;
        assert!(args.skill.is_none());
        assert_eq!(args.format, "text");
        assert!(!args.all);
    }
}
