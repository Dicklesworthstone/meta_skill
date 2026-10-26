//! Environment and skill requirement checks

use std::collections::HashSet;
use std::process::Command;

use serde::{Deserialize, Serialize};
use which::which;

use crate::core::skill::SkillSpec;
use crate::core::slicing::extract_block_directives;
use crate::error::Result;

/// Result of a single requirement check
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequirementCheck {
    pub name: String,
    pub category: String,
    pub required: bool,
    pub satisfied: bool,
    pub message: Option<String>,
}

/// Report of all requirement checks for a specific skill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRequirementReport {
    pub skill_id: String,
    pub skill_name: String,
    pub satisfied: bool,
    pub checks: Vec<RequirementCheck>,
}

/// Check if core system tool requirements are met
pub fn check_requirements() -> Result<Vec<RequirementCheck>> {
    let tools = [
        ("Git Version Control", "git", true),
        ("CASS (Context Aware Semantic Search)", "cass", false),
        ("Beads Issue Tracker", "bd", false),
        ("Ripgrep", "rg", false),
        ("Tar Archiver", "tar", true),
    ];

    let mut results = Vec::with_capacity(tools.len());
    for (name, bin, required) in tools {
        let (satisfied, message) = check_binary(bin);
        results.push(RequirementCheck {
            name: name.to_string(),
            category: "system_tool".to_string(),
            required,
            satisfied,
            message: Some(message),
        });
    }
    Ok(results)
}

/// Check requirements declared by a specific `SkillSpec` against available skills/capabilities
/// and the current runtime environment.
#[must_use]
pub fn check_skill_requirements(
    spec: &SkillSpec,
    available_skills_and_capabilities: &HashSet<String>,
) -> SkillRequirementReport {
    let mut checks = Vec::new();
    let working_dir = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));

    // 1. Check metadata.requires (can be skill IDs, capability names, bin:<tool>, or env:<VAR>)
    for req in &spec.metadata.requires {
        let req_trimmed = req.trim();
        if req_trimmed.is_empty() {
            continue;
        }
        if let Some(bin) = req_trimmed
            .strip_prefix("bin:")
            .or_else(|| req_trimmed.strip_prefix("tool:"))
        {
            let bin = bin.trim();
            let (satisfied, msg) = check_binary(bin);
            checks.push(RequirementCheck {
                name: format!("tool:{bin}"),
                category: "tool".to_string(),
                required: true,
                satisfied,
                message: Some(msg),
            });
        } else if let Some(var) = req_trimmed.strip_prefix("env:") {
            let var = var.trim();
            let satisfied = std::env::var(var).is_ok_and(|v| !v.is_empty());
            checks.push(RequirementCheck {
                name: format!("env:{var}"),
                category: "env".to_string(),
                required: true,
                satisfied,
                message: Some(if satisfied {
                    format!("Environment variable {var} is set")
                } else {
                    format!("Environment variable {var} is not set")
                }),
            });
        } else if available_skills_and_capabilities.contains(req_trimmed) {
            checks.push(RequirementCheck {
                name: req_trimmed.to_string(),
                category: "skill_or_capability".to_string(),
                required: true,
                satisfied: true,
                message: Some(format!("Skill or capability '{req_trimmed}' is indexed")),
            });
        } else if which(req_trimmed).is_ok() {
            let (_, msg) = check_binary(req_trimmed);
            checks.push(RequirementCheck {
                name: req_trimmed.to_string(),
                category: "tool".to_string(),
                required: true,
                satisfied: true,
                message: Some(msg),
            });
        } else {
            checks.push(RequirementCheck {
                name: req_trimmed.to_string(),
                category: "skill_or_capability".to_string(),
                required: true,
                satisfied: false,
                message: Some(format!(
                    "Required skill, capability, or binary '{req_trimmed}' not found"
                )),
            });
        }
    }

    // 2. Check parent skill if `extends` is set
    if let Some(parent) = &spec.extends {
        let satisfied = available_skills_and_capabilities.contains(parent);
        checks.push(RequirementCheck {
            name: format!("extends:{parent}"),
            category: "inheritance".to_string(),
            required: true,
            satisfied,
            message: Some(if satisfied {
                format!("Parent skill '{parent}' is indexed")
            } else {
                format!("Parent skill '{parent}' is missing from index")
            }),
        });
    }

    // 3. Check included skills if `includes` is non-empty
    for include in &spec.includes {
        let satisfied = available_skills_and_capabilities.contains(&include.skill);
        checks.push(RequirementCheck {
            name: format!("includes:{}", include.skill),
            category: "composition".to_string(),
            required: true,
            satisfied,
            message: Some(if satisfied {
                format!("Included skill '{}' is indexed", include.skill)
            } else {
                format!("Included skill '{}' is missing from index", include.skill)
            }),
        });
    }

    // 4. Check context tools and build typed SkillRequirements / SkillTrigger descriptors
    let network_req = if spec
        .metadata
        .requires
        .iter()
        .any(|r| r == "network:required")
    {
        crate::core::skill::NetworkRequirement::Required
    } else if spec
        .metadata
        .requires
        .iter()
        .any(|r| r == "network:prefer_offline")
    {
        crate::core::skill::NetworkRequirement::PreferOffline
    } else {
        crate::core::skill::NetworkRequirement::OfflineOk
    };
    let mut typed_reqs = crate::core::skill::SkillRequirements {
        platforms: Vec::new(),
        tools: Vec::new(),
        env: Vec::new(),
        network: network_req,
    };
    for tool in &spec.metadata.context.tools {
        typed_reqs.tools.push(crate::core::skill::ToolRequirement {
            name: tool.clone(),
            min_version: None,
            required: false,
        });
    }
    for req_tool in &typed_reqs.tools {
        let (satisfied, msg) = check_binary(&req_tool.name);
        checks.push(RequirementCheck {
            name: format!("context_tool:{}", req_tool.name),
            category: "context_tool".to_string(),
            required: req_tool.required,
            satisfied,
            message: Some(msg),
        });
    }

    let _triggers: Vec<crate::core::skill::SkillTrigger> = spec
        .metadata
        .context
        .file_patterns
        .iter()
        .map(|pat| crate::core::skill::SkillTrigger {
            trigger_type: "file_pattern".to_string(),
            pattern: pat.clone(),
            boost: 0.25,
        })
        .collect();

    // 5. Check platforms if specified
    if !spec.metadata.platforms.is_empty() {
        for p in &spec.metadata.platforms {
            let platform = match p.to_lowercase().as_str() {
                "linux" => crate::core::skill::Platform::Linux,
                "macos" | "darwin" => crate::core::skill::Platform::Macos,
                "windows" | "win32" => crate::core::skill::Platform::Windows,
                _ => crate::core::skill::Platform::Any,
            };
            typed_reqs.platforms.push(platform);
        }
        let current_os = std::env::consts::OS.to_lowercase();
        let satisfied = spec.metadata.platforms.iter().any(|p| {
            let p_lower = p.to_lowercase();
            p_lower == "any" || p_lower == "*" || p_lower == current_os
        });
        checks.push(RequirementCheck {
            name: format!("platform:{}", spec.metadata.platforms.join(",")),
            category: "platform".to_string(),
            required: true,
            satisfied,
            message: Some(format!("Current OS is '{current_os}'")),
        });
    }

    // 6. Check conditional block predicates (optional context conditions)
    for section in &spec.sections {
        for block in &section.blocks {
            let (_, condition, _) = extract_block_directives(&block.content);
            if let Some(pred) = condition {
                let satisfied = pred.evaluate(&working_dir);
                checks.push(RequirementCheck {
                    name: format!("condition:{}", pred.expr),
                    category: "block_condition".to_string(),
                    required: false,
                    satisfied,
                    message: Some(format!(
                        "Block '{}' condition '{}' evaluated to {satisfied}",
                        block.id, pred.expr
                    )),
                });
            }
        }
    }

    let all_required_satisfied = checks.iter().filter(|c| c.required).all(|c| c.satisfied);

    SkillRequirementReport {
        skill_id: spec.metadata.id.clone(),
        skill_name: spec.metadata.name.clone(),
        satisfied: all_required_satisfied,
        checks,
    }
}

fn check_binary(bin: &str) -> (bool, String) {
    match which(bin) {
        Ok(path) => {
            let version = Command::new(bin)
                .arg("--version")
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| {
                    String::from_utf8_lossy(&o.stdout)
                        .lines()
                        .next()
                        .unwrap_or("")
                        .trim()
                        .to_string()
                })
                .filter(|v| !v.is_empty());
            let msg = match version {
                Some(v) => format!("Found at {} ({v})", path.display()),
                None => format!("Found at {}", path.display()),
            };
            (true, msg)
        }
        Err(err) => (false, err.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::skill::SkillMetadata;

    #[test]
    fn test_check_skill_requirements_evaluates_deps_and_tools() {
        let mut spec = SkillSpec::new("my-skill", "My Skill");
        spec.metadata = SkillMetadata {
            id: "my-skill".to_string(),
            name: "My Skill".to_string(),
            requires: vec!["base-skill".to_string(), "missing-skill".to_string()],
            ..Default::default()
        };
        let mut available = HashSet::new();
        available.insert("base-skill".to_string());

        let report = check_skill_requirements(&spec, &available);
        assert!(!report.satisfied);
        assert_eq!(report.checks.len(), 2);
        assert!(report.checks[0].satisfied);
        assert!(!report.checks[1].satisfied);
    }
}
