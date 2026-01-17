//! `MetaSkill` manager - resolves slices, evaluates conditions, and packs content.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use crate::app::AppContext;
use crate::core::skill::{SkillSlice, SkillSpec};
use crate::core::slicing::SkillSlicer;
use crate::core::spec_lens::parse_markdown;
use crate::error::{MsError, Result};
use crate::storage::sqlite::SkillRecord;

use super::types::{MetaDisclosureLevel, MetaSkill, MetaSkillSliceRef, SliceCondition};

/// Overhead for each slice (newlines, titles, separators).
const TOKEN_OVERHEAD_PER_SLICE: usize = 4;

/// Result of loading a meta-skill.
#[derive(Debug)]
pub struct MetaSkillLoadResult {
    /// The meta-skill that was loaded.
    pub meta_skill_id: String,
    /// All resolved slices with their content.
    pub slices: Vec<ResolvedSlice>,
    /// Total tokens used.
    pub tokens_used: usize,
    /// Slices that were skipped (conditions not met or budget exceeded).
    pub skipped: Vec<SkippedSlice>,
    /// Packed content ready for use.
    pub packed_content: String,
}

/// A resolved slice from a meta-skill.
#[derive(Debug, Clone)]
pub struct ResolvedSlice {
    pub skill_id: String,
    pub slice_id: String,
    pub content: String,
    pub token_estimate: usize,
    pub priority: u8,
    pub required: bool,
}

/// A slice that was skipped during loading.
#[derive(Debug)]
pub struct SkippedSlice {
    pub skill_id: String,
    pub slice_id: Option<String>,
    pub reason: SkipReason,
}

/// Reason a slice was skipped.
#[derive(Debug)]
pub enum SkipReason {
    ConditionNotMet(String),
    BudgetExceeded,
    SkillNotFound,
    SliceNotFound,
    ResolutionError(String),
}

/// Context for condition evaluation.
pub struct ConditionContext<'a> {
    pub working_dir: &'a Path,
    pub tech_stacks: &'a [String],
    pub loaded_slices: &'a HashSet<(String, String)>,
}

impl ConditionContext<'_> {
    #[must_use] 
    pub fn evaluate(&self, condition: &SliceCondition) -> bool {
        match condition {
            SliceCondition::TechStack { value } => {
                self.tech_stacks.iter().any(|s| s.eq_ignore_ascii_case(value))
            }
            SliceCondition::FileExists { value } => {
                if is_safe_relative_path(value) {
                    self.working_dir.join(value).exists()
                } else {
                    false
                }
            }
            SliceCondition::EnvVar { value } => std::env::var(value).is_ok(),
            SliceCondition::DependsOn { skill_id, slice_id } => {
                self.loaded_slices.contains(&(skill_id.clone(), slice_id.clone()))
            }
        }
    }

    #[must_use] 
    pub fn evaluate_all(&self, conditions: &[SliceCondition]) -> bool {
        conditions.iter().all(|c| self.evaluate(c))
    }
}

fn is_safe_relative_path(path_str: &str) -> bool {
    let path = Path::new(path_str);
    if path.is_absolute() {
        return false;
    }
    for component in path.components() {
        match component {
            std::path::Component::ParentDir | std::path::Component::RootDir | std::path::Component::Prefix(_) => {
                return false;
            }
            _ => {}
        }
    }
    true
}

/// Manager for loading and resolving meta-skills.
pub struct MetaSkillManager<'a> {
    ctx: &'a AppContext,
}

impl<'a> MetaSkillManager<'a> {
    #[must_use] 
    pub const fn new(ctx: &'a AppContext) -> Self {
        Self { ctx }
    }

    /// Load a meta-skill and resolve all slices.
    pub fn load(
        &self,
        meta_skill: &MetaSkill,
        token_budget: usize,
        condition_ctx: &ConditionContext,
    ) -> Result<MetaSkillLoadResult> {
        let mut resolved_slices = Vec::new();
        let mut skipped = Vec::new();
        let mut loaded_slice_keys: HashSet<(String, String)> = HashSet::new();
        let mut parsed_skills_cache: HashMap<String, SkillSpec> = HashMap::new();

        // First pass: resolve all slice references
        for slice_ref in &meta_skill.slices {
            let resolution = self.resolve_slice_ref(
                slice_ref,
                condition_ctx,
                &loaded_slice_keys,
                &mut parsed_skills_cache,
            );

            match resolution {
                SliceResolution::Resolved(slices) => {
                    for slice in slices {
                        loaded_slice_keys.insert((slice.skill_id.clone(), slice.slice_id.clone()));
                        resolved_slices.push(slice);
                    }
                }
                SliceResolution::Skipped(skip) => {
                    skipped.push(skip);
                }
            }
        }

        // Sort by priority (higher first) then by required status
        resolved_slices.sort_by(|a, b| {
            b.required.cmp(&a.required)
                .then_with(|| b.priority.cmp(&a.priority))
        });

        // Check that required slices fit in budget
        let required_tokens: usize = resolved_slices
            .iter()
            .filter(|s| s.required)
            .map(|s| s.token_estimate)
            .sum();

        if required_tokens > token_budget {
            return Err(MsError::ValidationFailed(format!(
                "required slices need {required_tokens} tokens but budget is {token_budget}"
            )));
        }

        // Pack slices within budget
        let (packed_slices, tokens_used) = self.pack_within_budget(
            resolved_slices,
            token_budget,
            &mut skipped,
        );

        // Generate packed content
        let packed_content = self.render_packed_content(&packed_slices, meta_skill);

        Ok(MetaSkillLoadResult {
            meta_skill_id: meta_skill.id.clone(),
            slices: packed_slices,
            tokens_used,
            skipped,
            packed_content,
        })
    }

    fn resolve_slice_ref(
        &self,
        slice_ref: &MetaSkillSliceRef,
        condition_ctx: &ConditionContext,
        loaded_slices: &HashSet<(String, String)>,
        parsed_skills_cache: &mut HashMap<String, SkillSpec>,
    ) -> SliceResolution {
        // Check conditions first
        if !slice_ref.conditions.is_empty() {
            // Create updated context with currently loaded slices
            let updated_ctx = ConditionContext {
                working_dir: condition_ctx.working_dir,
                tech_stacks: condition_ctx.tech_stacks,
                loaded_slices,
            };

            if !updated_ctx.evaluate_all(&slice_ref.conditions) {
                return SliceResolution::Skipped(SkippedSlice {
                    skill_id: slice_ref.skill_id.clone(),
                    slice_id: None,
                    reason: SkipReason::ConditionNotMet("conditions not satisfied".to_string()),
                });
            }
        }

        // Ensure skill is in cache
        if !parsed_skills_cache.contains_key(&slice_ref.skill_id) {
            // Look up the skill
            let skill_record = match self.lookup_skill(&slice_ref.skill_id) {
                Some(record) => record,
                None => {
                    return SliceResolution::Skipped(SkippedSlice {
                        skill_id: slice_ref.skill_id.clone(),
                        slice_id: None,
                        reason: SkipReason::SkillNotFound,
                    });
                }
            };

            // Parse and slice the skill
            let spec = match parse_markdown(&skill_record.body) {
                Ok(s) => s,
                Err(e) => {
                    return SliceResolution::Skipped(SkippedSlice {
                        skill_id: slice_ref.skill_id.clone(),
                        slice_id: None,
                        reason: SkipReason::ResolutionError(e.to_string()),
                    });
                }
            };
            parsed_skills_cache.insert(slice_ref.skill_id.clone(), spec);
        }

        // Get spec from cache
        let spec = &parsed_skills_cache[&slice_ref.skill_id];

        let slice_index = SkillSlicer::slice(spec);
        let all_slices = slice_index.slices;

        // Filter to requested slices or use level-based selection
        let selected_slices = if slice_ref.slice_ids.is_empty() {
            // Use level-based selection
            self.select_by_level(&all_slices, slice_ref.level)
        } else {
            // Use explicit slice IDs
            all_slices
                .into_iter()
                .filter(|s| slice_ref.slice_ids.contains(&s.id))
                .collect()
        };

        if selected_slices.is_empty() && !slice_ref.slice_ids.is_empty() {
            return SliceResolution::Skipped(SkippedSlice {
                skill_id: slice_ref.skill_id.clone(),
                slice_id: slice_ref.slice_ids.first().cloned(),
                reason: SkipReason::SliceNotFound,
            });
        }

        let resolved: Vec<ResolvedSlice> = selected_slices
            .into_iter()
            .map(|s| ResolvedSlice {
                skill_id: slice_ref.skill_id.clone(),
                slice_id: s.id.clone(),
                content: s.content.clone(),
                token_estimate: s.token_estimate,
                priority: slice_ref.priority,
                required: slice_ref.required,
            })
            .collect();

        SliceResolution::Resolved(resolved)
    }

    fn select_by_level(&self, slices: &[SkillSlice], level: Option<MetaDisclosureLevel>) -> Vec<SkillSlice> {
        use crate::core::skill::SliceType;

        let level = level.unwrap_or(MetaDisclosureLevel::Core);

        slices
            .iter()
            .filter(|s| {
                match level {
                    MetaDisclosureLevel::Core => {
                        matches!(s.slice_type, SliceType::Overview | SliceType::Policy)
                    }
                    MetaDisclosureLevel::Extended => {
                        matches!(s.slice_type, SliceType::Overview | SliceType::Policy | SliceType::Example | SliceType::Command)
                    }
                    MetaDisclosureLevel::Deep => {
                        // Include all slice types
                        true
                    }
                }
            })
            .cloned()
            .collect()
    }

    fn lookup_skill(&self, skill_id: &str) -> Option<SkillRecord> {
        // Only use exact match for meta-skill resolution to ensure deterministic
        // and safe dependency loading. Fuzzy search (FTS) is appropriate for
        // user queries but not for spec resolution.
        self.ctx.db.get_skill(skill_id).ok().flatten()
    }

    fn pack_within_budget(
        &self,
        slices: Vec<ResolvedSlice>,
        budget: usize,
        skipped: &mut Vec<SkippedSlice>,
    ) -> (Vec<ResolvedSlice>, usize) {
        let mut packed = Vec::new();
        let mut used = 0usize;

        for slice in slices {
            let slice_cost = slice.token_estimate + TOKEN_OVERHEAD_PER_SLICE;
            
            if slice.required {
                // Required slices always included (we already verified they fit total budget, 
                // though individual packing might slightly exceed if we didn't account for overhead before)
                used += slice_cost;
                packed.push(slice);
            } else if used + slice_cost <= budget {
                used += slice_cost;
                packed.push(slice);
            } else {
                skipped.push(SkippedSlice {
                    skill_id: slice.skill_id,
                    slice_id: Some(slice.slice_id),
                    reason: SkipReason::BudgetExceeded,
                });
            }
        }

        (packed, used)
    }

    fn render_packed_content(&self, slices: &[ResolvedSlice], meta_skill: &MetaSkill) -> String {
        let mut output = String::new();

        // Header
        output.push_str(&format!("# Meta-Skill: {}\n\n", meta_skill.name));
        output.push_str(&format!("{}\n\n", meta_skill.description));
        output.push_str("---\n\n");

        // Group slices by skill for better readability
        let mut by_skill: std::collections::HashMap<&str, Vec<&ResolvedSlice>> = std::collections::HashMap::new();
        for slice in slices {
            by_skill.entry(&slice.skill_id).or_default().push(slice);
        }

        // Sort skills by ID for deterministic output
        let mut skill_ids: Vec<&str> = by_skill.keys().copied().collect();
        skill_ids.sort_unstable();

        for skill_id in skill_ids {
            let skill_slices = &by_skill[skill_id];
            output.push_str(&format!("## From: {skill_id}\n\n"));
            for slice in skill_slices {
                output.push_str(&slice.content);
                output.push_str("\n\n");
            }
        }

        output
    }
}

enum SliceResolution {
    Resolved(Vec<ResolvedSlice>),
    Skipped(SkippedSlice),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn condition_evaluates_tech_stack() {
        let ctx = ConditionContext {
            working_dir: Path::new("/tmp"),
            tech_stacks: &["rust".to_string(), "typescript".to_string()],
            loaded_slices: &HashSet::new(),
        };

        assert!(ctx.evaluate(&SliceCondition::TechStack {
            value: "rust".to_string()
        }));
        assert!(ctx.evaluate(&SliceCondition::TechStack {
            value: "RUST".to_string()
        }));
        assert!(!ctx.evaluate(&SliceCondition::TechStack {
            value: "python".to_string()
        }));
    }

    #[test]
    fn condition_evaluates_env_var() {
        // Test that existing env vars are detected correctly.
        // We check a variable that is typically set in all environments.
        let ctx = ConditionContext {
            working_dir: Path::new("/tmp"),
            tech_stacks: &[],
            loaded_slices: &HashSet::new(),
        };

        // PATH is almost always set
        assert!(ctx.evaluate(&SliceCondition::EnvVar {
            value: "PATH".to_string()
        }));
        // This variable should not exist
        assert!(!ctx.evaluate(&SliceCondition::EnvVar {
            value: "MS_TEST_NONEXISTENT_VAR_12345".to_string()
        }));
    }

    #[test]
    fn condition_evaluates_depends_on() {
        let mut loaded = HashSet::new();
        loaded.insert(("skill-a".to_string(), "slice-1".to_string()));

        let ctx = ConditionContext {
            working_dir: Path::new("/tmp"),
            tech_stacks: &[],
            loaded_slices: &loaded,
        };

        assert!(ctx.evaluate(&SliceCondition::DependsOn {
            skill_id: "skill-a".to_string(),
            slice_id: "slice-1".to_string(),
        }));
        assert!(!ctx.evaluate(&SliceCondition::DependsOn {
            skill_id: "skill-b".to_string(),
            slice_id: "slice-2".to_string(),
        }));
    }

    #[test]
    fn condition_file_exists_blocks_traversal() {
        let ctx = ConditionContext {
            working_dir: Path::new("/tmp/project"),
            tech_stacks: &[],
            loaded_slices: &HashSet::new(),
        };

        // Should return false (safe default) for unsafe paths, even if they exist
        assert!(!ctx.evaluate(&SliceCondition::FileExists {
            value: "/etc/passwd".to_string()
        }));
        assert!(!ctx.evaluate(&SliceCondition::FileExists {
            value: "../outside".to_string()
        }));

        // Safe relative path
        // Note: we can't easily test true positive without fs, but false positive on unsafe is key
    }

    // =========================================
    // is_safe_relative_path Tests
    // =========================================

    #[test]
    fn safe_relative_path_simple() {
        assert!(is_safe_relative_path("file.txt"));
        assert!(is_safe_relative_path("src/main.rs"));
        assert!(is_safe_relative_path("a/b/c/d.txt"));
    }

    #[test]
    fn safe_relative_path_blocks_absolute() {
        assert!(!is_safe_relative_path("/etc/passwd"));
        assert!(!is_safe_relative_path("/home/user/file.txt"));
    }

    #[test]
    fn safe_relative_path_blocks_parent_traversal() {
        assert!(!is_safe_relative_path("../file.txt"));
        assert!(!is_safe_relative_path("a/../../../etc/passwd"));
        assert!(!is_safe_relative_path(".."));
    }

    #[test]
    fn safe_relative_path_allows_current_dir() {
        assert!(is_safe_relative_path("./file.txt"));
        assert!(is_safe_relative_path("./src/main.rs"));
    }

    // =========================================
    // ConditionContext evaluate_all Tests
    // =========================================

    #[test]
    fn evaluate_all_empty_conditions() {
        let ctx = ConditionContext {
            working_dir: Path::new("/tmp"),
            tech_stacks: &[],
            loaded_slices: &HashSet::new(),
        };

        // Empty conditions should return true (all satisfied)
        assert!(ctx.evaluate_all(&[]));
    }

    #[test]
    fn evaluate_all_single_condition_true() {
        let ctx = ConditionContext {
            working_dir: Path::new("/tmp"),
            tech_stacks: &["rust".to_string()],
            loaded_slices: &HashSet::new(),
        };

        let conditions = vec![SliceCondition::TechStack { value: "rust".to_string() }];
        assert!(ctx.evaluate_all(&conditions));
    }

    #[test]
    fn evaluate_all_single_condition_false() {
        let ctx = ConditionContext {
            working_dir: Path::new("/tmp"),
            tech_stacks: &[],
            loaded_slices: &HashSet::new(),
        };

        let conditions = vec![SliceCondition::TechStack { value: "rust".to_string() }];
        assert!(!ctx.evaluate_all(&conditions));
    }

    #[test]
    fn evaluate_all_multiple_conditions_all_true() {
        let mut loaded = HashSet::new();
        loaded.insert(("skill-a".to_string(), "slice-1".to_string()));

        let ctx = ConditionContext {
            working_dir: Path::new("/tmp"),
            tech_stacks: &["rust".to_string()],
            loaded_slices: &loaded,
        };

        let conditions = vec![
            SliceCondition::TechStack { value: "rust".to_string() },
            SliceCondition::DependsOn { skill_id: "skill-a".to_string(), slice_id: "slice-1".to_string() },
        ];
        assert!(ctx.evaluate_all(&conditions));
    }

    #[test]
    fn evaluate_all_multiple_conditions_one_false() {
        let ctx = ConditionContext {
            working_dir: Path::new("/tmp"),
            tech_stacks: &["rust".to_string()],
            loaded_slices: &HashSet::new(),
        };

        let conditions = vec![
            SliceCondition::TechStack { value: "rust".to_string() },  // true
            SliceCondition::TechStack { value: "python".to_string() }, // false
        ];
        assert!(!ctx.evaluate_all(&conditions));
    }

    // =========================================
    // SkipReason Tests
    // =========================================

    #[test]
    fn skip_reason_variants() {
        let _ = SkipReason::ConditionNotMet("test".to_string());
        let _ = SkipReason::BudgetExceeded;
        let _ = SkipReason::SkillNotFound;
        let _ = SkipReason::SliceNotFound;
        let _ = SkipReason::ResolutionError("error".to_string());
    }

    #[test]
    fn skip_reason_debug() {
        let reason = SkipReason::BudgetExceeded;
        let debug = format!("{:?}", reason);
        assert!(debug.contains("BudgetExceeded"));
    }

    // =========================================
    // SkippedSlice Tests
    // =========================================

    #[test]
    fn skipped_slice_debug() {
        let skipped = SkippedSlice {
            skill_id: "skill-a".to_string(),
            slice_id: Some("slice-1".to_string()),
            reason: SkipReason::BudgetExceeded,
        };
        let debug = format!("{:?}", skipped);
        assert!(debug.contains("SkippedSlice"));
        assert!(debug.contains("skill-a"));
    }

    // =========================================
    // ResolvedSlice Tests
    // =========================================

    #[test]
    fn resolved_slice_clone() {
        let slice = ResolvedSlice {
            skill_id: "skill-a".to_string(),
            slice_id: "slice-1".to_string(),
            content: "content".to_string(),
            token_estimate: 100,
            priority: 5,
            required: true,
        };
        let cloned = slice.clone();
        assert_eq!(cloned.skill_id, "skill-a");
        assert_eq!(cloned.token_estimate, 100);
    }

    #[test]
    fn resolved_slice_debug() {
        let slice = ResolvedSlice {
            skill_id: "skill-a".to_string(),
            slice_id: "slice-1".to_string(),
            content: "content".to_string(),
            token_estimate: 100,
            priority: 5,
            required: true,
        };
        let debug = format!("{:?}", slice);
        assert!(debug.contains("ResolvedSlice"));
        assert!(debug.contains("skill-a"));
    }

    // =========================================
    // MetaSkillLoadResult Tests
    // =========================================

    #[test]
    fn load_result_debug() {
        let result = MetaSkillLoadResult {
            meta_skill_id: "meta-1".to_string(),
            slices: vec![],
            tokens_used: 500,
            skipped: vec![],
            packed_content: "content".to_string(),
        };
        let debug = format!("{:?}", result);
        assert!(debug.contains("MetaSkillLoadResult"));
        assert!(debug.contains("meta-1"));
    }
}
