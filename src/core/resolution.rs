//! Skill inheritance resolution
//!
//! Implements single-inheritance resolution for skills using the `extends` field.
//! Handles cycle detection, section merging, and inheritance chain tracking.

use std::collections::{HashMap, HashSet};

use crate::core::skill::{BlockType, SkillBlock, SkillSection, SkillSpec};
use crate::error::{MsError, Result};

/// Maximum inheritance depth before warning
pub const MAX_INHERITANCE_DEPTH: usize = 5;

/// A trait for resolving skill references during inheritance resolution
pub trait SkillRepository: Send + Sync {
    /// Get a skill spec by ID
    fn get(&self, skill_id: &str) -> Result<Option<SkillSpec>>;
}

/// A resolved skill with inheritance applied
#[derive(Debug, Clone)]
pub struct ResolvedSkillSpec {
    /// The final resolved spec
    pub spec: SkillSpec,
    /// Chain of skill IDs from root to this skill (oldest first)
    pub inheritance_chain: Vec<String>,
    /// Warnings encountered during resolution
    pub warnings: Vec<ResolutionWarning>,
}

/// Warnings that can occur during resolution
#[derive(Debug, Clone)]
pub enum ResolutionWarning {
    /// Inheritance depth exceeds recommended maximum
    DeepInheritance {
        depth: usize,
        chain: Vec<String>,
    },
    /// Section in child shadows parent section completely
    SectionShadowed {
        section_id: String,
        parent_id: String,
    },
}

/// Result of cycle detection
#[derive(Debug, Clone)]
pub enum CycleDetectionResult {
    /// No cycle found
    NoCycle,
    /// Cycle detected, contains the cycle path
    CycleFound(Vec<String>),
}

/// Detect if there's a cycle in the inheritance chain starting from a skill
pub fn detect_inheritance_cycle<R: SkillRepository + ?Sized>(
    skill_id: &str,
    repository: &R,
) -> Result<CycleDetectionResult> {
    let mut visited = HashSet::new();
    let mut chain = Vec::new();

    let mut current_id = skill_id.to_string();

    loop {
        // Check if we've seen this skill before
        if visited.contains(&current_id) {
            // Find where the cycle starts in our chain
            let cycle_start = chain.iter().position(|id| id == &current_id).unwrap();
            let mut cycle_path = chain[cycle_start..].to_vec();
            cycle_path.push(current_id);
            return Ok(CycleDetectionResult::CycleFound(cycle_path));
        }

        visited.insert(current_id.clone());
        chain.push(current_id.clone());

        // Get the skill and check for parent
        let skill = repository.get(&current_id)?;
        match skill.and_then(|s| s.extends) {
            Some(parent_id) => current_id = parent_id,
            None => return Ok(CycleDetectionResult::NoCycle),
        }
    }
}

/// Resolve a skill's inheritance, applying parent sections
pub fn resolve_extends<R: SkillRepository + ?Sized>(
    skill: &SkillSpec,
    repository: &R,
) -> Result<ResolvedSkillSpec> {
    let mut warnings = Vec::new();

    // Base case: no extends
    let Some(parent_id) = &skill.extends else {
        return Ok(ResolvedSkillSpec {
            spec: skill.clone(),
            inheritance_chain: vec![skill.metadata.id.clone()],
            warnings,
        });
    };

    // Check for cycles first
    match detect_inheritance_cycle(&skill.metadata.id, repository)? {
        CycleDetectionResult::NoCycle => {}
        CycleDetectionResult::CycleFound(cycle) => {
            return Err(MsError::CyclicInheritance {
                skill_id: skill.metadata.id.clone(),
                cycle,
            });
        }
    }

    // Get parent skill
    let parent = repository.get(parent_id)?.ok_or_else(|| MsError::ParentSkillNotFound {
        parent_id: parent_id.clone(),
        child_id: skill.metadata.id.clone(),
    })?;

    // Recursively resolve parent
    let resolved_parent = resolve_extends(&parent, repository)?;

    // Check inheritance depth
    let depth = resolved_parent.inheritance_chain.len() + 1;
    if depth > MAX_INHERITANCE_DEPTH {
        let mut chain = resolved_parent.inheritance_chain.clone();
        chain.push(skill.metadata.id.clone());
        warnings.push(ResolutionWarning::DeepInheritance { depth, chain });
    }

    // Merge child onto parent
    let merged_spec = merge_skills(&resolved_parent.spec, skill, &mut warnings);

    // Build inheritance chain
    let mut inheritance_chain = resolved_parent.inheritance_chain;
    inheritance_chain.push(skill.metadata.id.clone());

    // Collect all warnings
    warnings.extend(resolved_parent.warnings);

    Ok(ResolvedSkillSpec {
        spec: merged_spec,
        inheritance_chain,
        warnings,
    })
}

/// Merge a child skill onto a parent, applying inheritance rules
fn merge_skills(parent: &SkillSpec, child: &SkillSpec, warnings: &mut Vec<ResolutionWarning>) -> SkillSpec {
    let mut result = parent.clone();

    // Always replace these from child
    result.metadata.id = child.metadata.id.clone();
    result.format_version = child.format_version.clone();

    // Replace metadata if child provides it
    if !child.metadata.name.is_empty() {
        result.metadata.name = child.metadata.name.clone();
    }
    if !child.metadata.description.is_empty() {
        result.metadata.description = child.metadata.description.clone();
    }
    if !child.metadata.version.is_empty() {
        result.metadata.version = child.metadata.version.clone();
    }
    if !child.metadata.tags.is_empty() {
        result.metadata.tags = child.metadata.tags.clone();
    }
    if child.metadata.author.is_some() {
        result.metadata.author = child.metadata.author.clone();
    }
    if child.metadata.license.is_some() {
        result.metadata.license = child.metadata.license.clone();
    }
    if !child.metadata.requires.is_empty() {
        result.metadata.requires = child.metadata.requires.clone();
    }
    if !child.metadata.provides.is_empty() {
        result.metadata.provides = child.metadata.provides.clone();
    }
    if !child.metadata.platforms.is_empty() {
        result.metadata.platforms = child.metadata.platforms.clone();
    }
    if !child.metadata.context.is_empty() {
        result.metadata.context = child.metadata.context.clone();
    }

    // Clear extends from result (it's now resolved)
    result.extends = None;
    result.replace_rules = false;
    result.replace_examples = false;
    result.replace_pitfalls = false;
    result.replace_checklist = false;

    // Merge sections based on replace_* flags
    merge_sections(&mut result.sections, &child.sections, child, warnings);

    result
}

/// Merge child sections into parent sections
fn merge_sections(
    parent_sections: &mut Vec<SkillSection>,
    child_sections: &[SkillSection],
    child_spec: &SkillSpec,
    warnings: &mut Vec<ResolutionWarning>,
) {
    // Build a map of parent sections by ID for efficient lookup
    let mut parent_map: HashMap<String, usize> = parent_sections
        .iter()
        .enumerate()
        .map(|(i, s)| (s.id.clone(), i))
        .collect();

    for child_section in child_sections {
        if let Some(&parent_idx) = parent_map.get(&child_section.id) {
            // Section exists in parent - merge blocks
            let parent_section = &mut parent_sections[parent_idx];
            merge_blocks(
                &mut parent_section.blocks,
                &child_section.blocks,
                child_spec,
                warnings,
                &parent_section.id,
            );
            // Update title if child provides one
            if !child_section.title.is_empty() {
                parent_section.title = child_section.title.clone();
            }
        } else {
            // New section from child - add it
            parent_sections.push(child_section.clone());
            parent_map.insert(child_section.id.clone(), parent_sections.len() - 1);
        }
    }
}

/// Merge child blocks into parent blocks based on block types and replace flags
fn merge_blocks(
    parent_blocks: &mut Vec<SkillBlock>,
    child_blocks: &[SkillBlock],
    child_spec: &SkillSpec,
    _warnings: &mut Vec<ResolutionWarning>,
    _section_id: &str,
) {
    // Group parent blocks by type for replacement logic
    let mut blocks_by_type: HashMap<BlockType, Vec<SkillBlock>> = HashMap::new();
    for block in parent_blocks.drain(..) {
        blocks_by_type.entry(block.block_type.clone()).or_default().push(block);
    }

    // Process child blocks
    for child_block in child_blocks {
        let block_type = &child_block.block_type;
        let should_replace = match block_type {
            BlockType::Rule => child_spec.replace_rules,
            BlockType::Code => child_spec.replace_examples,
            BlockType::Pitfall => child_spec.replace_pitfalls,
            BlockType::Checklist => child_spec.replace_checklist,
            // Text, Command blocks: always append
            _ => false,
        };

        if should_replace {
            // Clear parent blocks of this type and add child's
            blocks_by_type.insert(block_type.clone(), vec![child_block.clone()]);
        } else {
            // Append to existing
            blocks_by_type
                .entry(block_type.clone())
                .or_default()
                .push(child_block.clone());
        }
    }

    // Rebuild parent_blocks maintaining a reasonable order
    let type_order = [
        BlockType::Text,
        BlockType::Rule,
        BlockType::Code,
        BlockType::Command,
        BlockType::Pitfall,
        BlockType::Checklist,
    ];

    for block_type in &type_order {
        if let Some(blocks) = blocks_by_type.remove(block_type) {
            parent_blocks.extend(blocks);
        }
    }

    // Add any remaining block types not in our order list
    for (_, blocks) in blocks_by_type {
        parent_blocks.extend(blocks);
    }
}

/// Get the full inheritance chain for a skill (root to leaf)
pub fn get_inheritance_chain<R: SkillRepository + ?Sized>(
    skill_id: &str,
    repository: &R,
) -> Result<Vec<String>> {
    let mut chain = Vec::new();
    let mut visited = HashSet::new();
    let mut current_id = skill_id.to_string();

    // First, collect the chain going up to root
    loop {
        if visited.contains(&current_id) {
            return Err(MsError::CyclicInheritance {
                skill_id: skill_id.to_string(),
                cycle: chain,
            });
        }
        visited.insert(current_id.clone());
        chain.push(current_id.clone());

        let skill = repository.get(&current_id)?;
        match skill.and_then(|s| s.extends) {
            Some(parent_id) => current_id = parent_id,
            None => break,
        }
    }

    // Reverse to get root-to-leaf order
    chain.reverse();
    Ok(chain)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A simple in-memory skill repository for testing
    struct TestRepository {
        skills: HashMap<String, SkillSpec>,
    }

    impl TestRepository {
        fn new() -> Self {
            Self {
                skills: HashMap::new(),
            }
        }

        fn add(&mut self, spec: SkillSpec) {
            self.skills.insert(spec.metadata.id.clone(), spec);
        }
    }

    impl SkillRepository for TestRepository {
        fn get(&self, skill_id: &str) -> Result<Option<SkillSpec>> {
            Ok(self.skills.get(skill_id).cloned())
        }
    }

    fn make_skill(id: &str, name: &str) -> SkillSpec {
        SkillSpec::new(id, name)
    }

    fn make_skill_with_parent(id: &str, name: &str, parent: &str) -> SkillSpec {
        let mut spec = SkillSpec::new(id, name);
        spec.extends = Some(parent.to_string());
        spec
    }

    #[test]
    fn test_no_inheritance() {
        let repo = TestRepository::new();
        let skill = make_skill("standalone", "Standalone Skill");

        let resolved = resolve_extends(&skill, &repo).unwrap();

        assert_eq!(resolved.spec.metadata.id, "standalone");
        assert_eq!(resolved.inheritance_chain, vec!["standalone"]);
        assert!(resolved.warnings.is_empty());
    }

    #[test]
    fn test_simple_inheritance() {
        let mut repo = TestRepository::new();

        let mut parent = make_skill("parent", "Parent Skill");
        parent.sections.push(SkillSection {
            id: "intro".to_string(),
            title: "Introduction".to_string(),
            blocks: vec![SkillBlock {
                id: "intro-1".to_string(),
                block_type: BlockType::Text,
                content: "Parent intro".to_string(),
            }],
        });
        repo.add(parent);

        let child = make_skill_with_parent("child", "Child Skill", "parent");

        let resolved = resolve_extends(&child, &repo).unwrap();

        assert_eq!(resolved.spec.metadata.id, "child");
        assert_eq!(resolved.spec.metadata.name, "Child Skill");
        assert_eq!(resolved.inheritance_chain, vec!["parent", "child"]);
        assert_eq!(resolved.spec.sections.len(), 1);
        assert_eq!(resolved.spec.sections[0].id, "intro");
    }

    #[test]
    fn test_cycle_detection() {
        let mut repo = TestRepository::new();

        // Create a cycle: A -> B -> C -> A
        let a = make_skill_with_parent("a", "A", "b");
        let b = make_skill_with_parent("b", "B", "c");
        let c = make_skill_with_parent("c", "C", "a");

        repo.add(a.clone());
        repo.add(b);
        repo.add(c);

        let result = detect_inheritance_cycle("a", &repo).unwrap();
        match result {
            CycleDetectionResult::CycleFound(cycle) => {
                assert!(cycle.contains(&"a".to_string()));
                assert!(cycle.contains(&"b".to_string()));
                assert!(cycle.contains(&"c".to_string()));
            }
            CycleDetectionResult::NoCycle => panic!("Expected cycle to be detected"),
        }

        // resolve_extends should fail with a cycle error
        let err = resolve_extends(&a, &repo).unwrap_err();
        match err {
            MsError::CyclicInheritance { skill_id, cycle } => {
                assert_eq!(skill_id, "a");
                assert!(!cycle.is_empty());
            }
            _ => panic!("Expected CyclicInheritance error"),
        }
    }

    #[test]
    fn test_missing_parent() {
        let repo = TestRepository::new();
        let child = make_skill_with_parent("child", "Child", "nonexistent");

        let err = resolve_extends(&child, &repo).unwrap_err();
        match err {
            MsError::ParentSkillNotFound { parent_id, child_id } => {
                assert_eq!(parent_id, "nonexistent");
                assert_eq!(child_id, "child");
            }
            _ => panic!("Expected ParentSkillNotFound error"),
        }
    }

    #[test]
    fn test_deep_inheritance_warning() {
        let mut repo = TestRepository::new();

        // Create a chain deeper than MAX_INHERITANCE_DEPTH
        let mut prev_id = None;
        for i in 0..=MAX_INHERITANCE_DEPTH + 1 {
            let id = format!("skill-{}", i);
            let mut skill = make_skill(&id, &format!("Skill {}", i));
            if let Some(parent) = prev_id {
                skill.extends = Some(parent);
            }
            repo.add(skill);
            prev_id = Some(id);
        }

        // Get the deepest skill
        let deepest_id = format!("skill-{}", MAX_INHERITANCE_DEPTH + 1);
        let deepest = repo.get(&deepest_id).unwrap().unwrap();

        let resolved = resolve_extends(&deepest, &repo).unwrap();

        // Should have a deep inheritance warning
        let has_warning = resolved.warnings.iter().any(|w| {
            matches!(w, ResolutionWarning::DeepInheritance { .. })
        });
        assert!(has_warning, "Expected DeepInheritance warning");
    }

    #[test]
    fn test_section_merging() {
        let mut repo = TestRepository::new();

        let mut parent = make_skill("parent", "Parent");
        parent.sections.push(SkillSection {
            id: "rules".to_string(),
            title: "Rules".to_string(),
            blocks: vec![SkillBlock {
                id: "rule-1".to_string(),
                block_type: BlockType::Rule,
                content: "Parent rule".to_string(),
            }],
        });
        repo.add(parent);

        let mut child = make_skill_with_parent("child", "Child", "parent");
        child.sections.push(SkillSection {
            id: "rules".to_string(),
            title: "Child Rules".to_string(),
            blocks: vec![SkillBlock {
                id: "rule-2".to_string(),
                block_type: BlockType::Rule,
                content: "Child rule".to_string(),
            }],
        });

        let resolved = resolve_extends(&child, &repo).unwrap();

        // Should have merged rules section with both blocks (append mode)
        assert_eq!(resolved.spec.sections.len(), 1);
        let rules_section = &resolved.spec.sections[0];
        assert_eq!(rules_section.title, "Child Rules"); // Child title takes precedence
        assert_eq!(rules_section.blocks.len(), 2); // Both rules
    }

    #[test]
    fn test_replace_rules_flag() {
        let mut repo = TestRepository::new();

        let mut parent = make_skill("parent", "Parent");
        parent.sections.push(SkillSection {
            id: "rules".to_string(),
            title: "Rules".to_string(),
            blocks: vec![SkillBlock {
                id: "rule-1".to_string(),
                block_type: BlockType::Rule,
                content: "Parent rule".to_string(),
            }],
        });
        repo.add(parent);

        let mut child = make_skill_with_parent("child", "Child", "parent");
        child.replace_rules = true; // Replace instead of append
        child.sections.push(SkillSection {
            id: "rules".to_string(),
            title: "".to_string(),
            blocks: vec![SkillBlock {
                id: "rule-2".to_string(),
                block_type: BlockType::Rule,
                content: "Child rule only".to_string(),
            }],
        });

        let resolved = resolve_extends(&child, &repo).unwrap();

        // Should only have child's rule (replaced parent's)
        let rules_section = &resolved.spec.sections[0];
        assert_eq!(rules_section.blocks.len(), 1);
        assert_eq!(rules_section.blocks[0].content, "Child rule only");
    }

    #[test]
    fn test_new_section_from_child() {
        let mut repo = TestRepository::new();

        let parent = make_skill("parent", "Parent");
        repo.add(parent);

        let mut child = make_skill_with_parent("child", "Child", "parent");
        child.sections.push(SkillSection {
            id: "new-section".to_string(),
            title: "New Section".to_string(),
            blocks: vec![],
        });

        let resolved = resolve_extends(&child, &repo).unwrap();

        assert_eq!(resolved.spec.sections.len(), 1);
        assert_eq!(resolved.spec.sections[0].id, "new-section");
    }

    #[test]
    fn test_inheritance_chain() {
        let mut repo = TestRepository::new();

        let root = make_skill("root", "Root");
        let middle = make_skill_with_parent("middle", "Middle", "root");
        let leaf = make_skill_with_parent("leaf", "Leaf", "middle");

        repo.add(root);
        repo.add(middle);
        repo.add(leaf.clone());

        let chain = get_inheritance_chain("leaf", &repo).unwrap();
        assert_eq!(chain, vec!["root", "middle", "leaf"]);
    }

    #[test]
    fn test_extends_field_cleared_after_resolution() {
        let mut repo = TestRepository::new();

        let parent = make_skill("parent", "Parent");
        repo.add(parent);

        let child = make_skill_with_parent("child", "Child", "parent");
        let resolved = resolve_extends(&child, &repo).unwrap();

        // extends should be cleared in the resolved spec
        assert!(resolved.spec.extends.is_none());
    }

    #[test]
    fn test_has_parent_and_parent_id() {
        let standalone = make_skill("standalone", "Standalone");
        assert!(!standalone.has_parent());
        assert!(standalone.parent_id().is_none());

        let child = make_skill_with_parent("child", "Child", "parent");
        assert!(child.has_parent());
        assert_eq!(child.parent_id(), Some("parent"));
    }
}
