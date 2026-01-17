//! Skill inheritance and composition resolution
//!
//! Implements single-inheritance resolution for skills using the `extends` field,
//! and composition via the `includes` field. Handles cycle detection, section
//! merging, and inheritance/composition chain tracking.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::core::skill::{BlockType, IncludePosition, IncludeTarget, SkillBlock, SkillInclude, SkillSection, SkillSpec};
use crate::error::{MsError, Result};

/// Maximum inheritance depth before warning
pub const MAX_INHERITANCE_DEPTH: usize = 5;

/// A trait for resolving skill references during inheritance resolution
pub trait SkillRepository {
    /// Get a skill spec by ID
    fn get(&self, skill_id: &str) -> Result<Option<SkillSpec>>;
}

/// A `SkillRepository` backed by a Git archive.
///
/// Loads skills from the Git archive's `skill.spec.json` files.
pub struct GitSkillRepository<'a> {
    git: &'a crate::storage::GitArchive,
}

impl<'a> GitSkillRepository<'a> {
    /// Create a new repository backed by a Git archive.
    pub const fn new(git: &'a crate::storage::GitArchive) -> Self {
        Self { git }
    }
}

impl<'a> SkillRepository for GitSkillRepository<'a> {
    fn get(&self, skill_id: &str) -> Result<Option<SkillSpec>> {
        if !self.git.skill_exists(skill_id) {
            return Ok(None);
        }
        match self.git.read_skill(skill_id) {
            Ok(spec) => Ok(Some(spec)),
            Err(e) => Err(e),
        }
    }
}

/// A `SkillRepository` backed by the SQLite database.
///
/// Loads skills from the database and parses their markdown bodies.
pub struct DbSkillRepository<'a> {
    db: &'a crate::storage::Database,
}

impl<'a> DbSkillRepository<'a> {
    /// Create a new repository backed by the database.
    pub const fn new(db: &'a crate::storage::Database) -> Self {
        Self { db }
    }
}

impl<'a> SkillRepository for DbSkillRepository<'a> {
    fn get(&self, skill_id: &str) -> Result<Option<SkillSpec>> {
        let Some(record) = self.db.get_skill(skill_id)? else {
            return Ok(None);
        };
        // Parse the body to get the spec
        let spec = crate::core::spec_lens::parse_markdown(&record.body)?;
        Ok(Some(spec))
    }
}

/// A resolved skill with inheritance and composition applied
#[derive(Debug, Clone)]
pub struct ResolvedSkillSpec {
    /// The final resolved spec
    pub spec: SkillSpec,
    /// Chain of skill IDs from root to this skill (oldest first)
    pub inheritance_chain: Vec<String>,
    /// Skill IDs that were included (composed) into this skill
    pub included_from: Vec<String>,
    /// Warnings encountered during resolution
    pub warnings: Vec<ResolutionWarning>,
}

/// Warnings that can occur during resolution
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Many skills are being included into the same target section
    ManyIncludes {
        target: IncludeTarget,
        count: usize,
        sources: Vec<String>,
    },
    /// An included skill was not found
    IncludedSkillNotFound {
        skill_id: String,
        included_by: String,
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

/// Detect if there's a cycle in the inheritance chain starting from a skill.
/// Only checks the `extends` chain (single inheritance).
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

/// Detect if there's a cycle involving both extends and includes.
/// This performs a full DFS to detect cycles in the dependency graph.
pub fn detect_full_cycle<R: SkillRepository + ?Sized>(
    skill_id: &str,
    repository: &R,
) -> Result<CycleDetectionResult> {
    fn visit<R: SkillRepository + ?Sized>(
        id: &str,
        repo: &R,
        visited: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> Result<Option<Vec<String>>> {
        // Check for cycle
        if visited.contains(id) {
            if let Some(start) = path.iter().position(|p| p == id) {
                let mut cycle = path[start..].to_vec();
                cycle.push(id.to_string());
                return Ok(Some(cycle));
            }
            // Already visited but not in current path - no cycle through this node
            return Ok(None);
        }

        visited.insert(id.to_string());
        path.push(id.to_string());

        // Get the skill
        if let Some(skill) = repo.get(id)? {
            // Check extends
            if let Some(parent) = &skill.extends {
                if let Some(cycle) = visit(parent, repo, visited, path)? {
                    return Ok(Some(cycle));
                }
            }

            // Check includes
            for include in &skill.includes {
                if let Some(cycle) = visit(&include.skill, repo, visited, path)? {
                    return Ok(Some(cycle));
                }
            }
        }

        path.pop();
        Ok(None)
    }

    match visit(skill_id, repository, &mut HashSet::new(), &mut Vec::new())? {
        Some(cycle) => Ok(CycleDetectionResult::CycleFound(cycle)),
        None => Ok(CycleDetectionResult::NoCycle),
    }
}

/// Resolve a skill's inheritance, applying parent sections.
/// Does NOT resolve includes - use `resolve_full` for complete resolution.
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
            included_from: Vec::new(),
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
        included_from: resolved_parent.included_from,
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

    // Preserve includes from child (these will be resolved later)
    result.includes = child.includes.clone();

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

// =============================================================================
// INCLUDES RESOLUTION
// =============================================================================

/// Resolve a skill fully, applying both inheritance and includes.
///
/// Resolution order:
/// 1. Resolve inheritance chain (extends)
/// 2. Apply includes from other skills
/// 3. Clear includes from the resolved spec
pub fn resolve_full<R: SkillRepository + ?Sized>(
    skill: &SkillSpec,
    repository: &R,
) -> Result<ResolvedSkillSpec> {
    // First resolve inheritance
    let mut resolved = resolve_extends(skill, repository)?;

    // Check for cycles involving includes
    match detect_full_cycle(&skill.metadata.id, repository)? {
        CycleDetectionResult::NoCycle => {}
        CycleDetectionResult::CycleFound(cycle) => {
            return Err(MsError::CyclicInheritance {
                skill_id: skill.metadata.id.clone(),
                cycle,
            });
        }
    }

    // Apply includes
    resolve_includes(&mut resolved, repository)?;

    // Check for conflicts
    check_include_conflicts(skill, &mut resolved.warnings);

    // Clear includes from resolved spec (they're now applied)
    resolved.spec.includes = Vec::new();

    Ok(resolved)
}

/// Apply includes to a resolved skill spec.
fn resolve_includes<R: SkillRepository + ?Sized>(
    resolved: &mut ResolvedSkillSpec,
    repository: &R,
) -> Result<()> {
    // Process includes in order
    for include in &resolved.spec.includes.clone() {
        // Get the included skill
        let included_skill = match repository.get(&include.skill)? {
            Some(skill) => skill,
            None => {
                resolved.warnings.push(ResolutionWarning::IncludedSkillNotFound {
                    skill_id: include.skill.clone(),
                    included_by: resolved.spec.metadata.id.clone(),
                });
                continue;
            }
        };

        // Resolve the included skill first (recursively)
        let resolved_included = resolve_full(&included_skill, repository)?;

        // Apply the include
        apply_include(&mut resolved.spec, &resolved_included.spec, include);

        // Track included skill
        resolved.included_from.push(include.skill.clone());

        // Collect warnings from included skill resolution
        resolved.warnings.extend(resolved_included.warnings);
    }

    Ok(())
}

/// Apply a single include to the target spec.
fn apply_include(target: &mut SkillSpec, source: &SkillSpec, include: &SkillInclude) {
    let target_block_type = include.into.to_block_type();

    // Extract blocks from source that match the target type
    let mut source_blocks: Vec<SkillBlock> = Vec::new();
    for section in &source.sections {
        // Filter by sections if specified
        if let Some(section_filter) = &include.sections {
            if !section_filter.iter().any(|s| s == &section.id || s == &section.title) {
                continue;
            }
        }

        for block in &section.blocks {
            // For Context target, include Text blocks
            // For other targets, match the specific block type
            let matches = match include.into {
                IncludeTarget::Context => block.block_type == BlockType::Text,
                _ => block.block_type == target_block_type,
            };

            if matches {
                let mut block = block.clone();
                // Apply prefix if specified
                if let Some(prefix) = &include.prefix {
                    block.content = format!("{}{}", prefix, block.content);
                }
                source_blocks.push(block);
            }
        }
    }

    if source_blocks.is_empty() {
        return;
    }

    // Find or create the target section
    let section_id = match include.into {
        IncludeTarget::Rules => "rules",
        IncludeTarget::Examples => "examples",
        IncludeTarget::Pitfalls => "pitfalls",
        IncludeTarget::Checklist => "checklist",
        IncludeTarget::Context => "context",
    };

    let target_section = target
        .sections
        .iter_mut()
        .find(|s| s.id == section_id);

    if let Some(section) = target_section {
        // Apply blocks based on position
        match include.position {
            IncludePosition::Prepend => {
                let mut new_blocks = source_blocks;
                new_blocks.extend(section.blocks.drain(..));
                section.blocks = new_blocks;
            }
            IncludePosition::Append => {
                section.blocks.extend(source_blocks);
            }
        }
    } else {
        // Create new section
        let title = match include.into {
            IncludeTarget::Rules => "Rules",
            IncludeTarget::Examples => "Examples",
            IncludeTarget::Pitfalls => "Pitfalls",
            IncludeTarget::Checklist => "Checklist",
            IncludeTarget::Context => "Context",
        };
        target.sections.push(SkillSection {
            id: section_id.to_string(),
            title: title.to_string(),
            blocks: source_blocks,
        });
    }
}

/// Check for potential conflicts with includes and add warnings.
fn check_include_conflicts(skill: &SkillSpec, warnings: &mut Vec<ResolutionWarning>) {
    // Group includes by target
    let mut by_target: HashMap<IncludeTarget, Vec<&str>> = HashMap::new();

    for include in &skill.includes {
        by_target
            .entry(include.into.clone())
            .or_default()
            .push(&include.skill);
    }

    // Warn if many skills include into the same target
    const MANY_INCLUDES_THRESHOLD: usize = 3;
    for (target, sources) in by_target {
        if sources.len() > MANY_INCLUDES_THRESHOLD {
            warnings.push(ResolutionWarning::ManyIncludes {
                target,
                count: sources.len(),
                sources: sources.iter().map(|s| s.to_string()).collect(),
            });
        }
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
            CycleDetectionResult::NoCycle => assert!(false, "Expected cycle to be detected"),
        }

        // resolve_extends should fail with a cycle error
        let err = resolve_extends(&a, &repo).unwrap_err();
        match err {
            MsError::CyclicInheritance { skill_id, cycle } => {
                assert_eq!(skill_id, "a");
                assert!(!cycle.is_empty());
            }
            _ => assert!(false, "Expected CyclicInheritance error"),
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
            _ => assert!(false, "Expected ParentSkillNotFound error"),
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

    // =========================================================================
    // INCLUDES TESTS
    // =========================================================================

    fn make_skill_with_rules(id: &str, name: &str, rules: Vec<&str>) -> SkillSpec {
        let mut spec = SkillSpec::new(id, name);
        spec.sections.push(SkillSection {
            id: "rules".to_string(),
            title: "Rules".to_string(),
            blocks: rules
                .into_iter()
                .enumerate()
                .map(|(i, content)| SkillBlock {
                    id: format!("rule-{}", i + 1),
                    block_type: BlockType::Rule,
                    content: content.to_string(),
                })
                .collect(),
        });
        spec
    }

    #[test]
    fn test_simple_include() {
        let mut repo = TestRepository::new();

        // Create a skill to include
        let error_skill = make_skill_with_rules(
            "error-handling",
            "Error Handling",
            vec!["Always handle errors", "Log errors with context"],
        );
        repo.add(error_skill);

        // Create a skill that includes it
        let mut main_skill = SkillSpec::new("main", "Main Skill");
        main_skill.includes.push(SkillInclude {
            skill: "error-handling".to_string(),
            into: IncludeTarget::Rules,
            prefix: None,
            sections: None,
            position: IncludePosition::Append,
        });
        main_skill.sections.push(SkillSection {
            id: "rules".to_string(),
            title: "Rules".to_string(),
            blocks: vec![SkillBlock {
                id: "main-rule-1".to_string(),
                block_type: BlockType::Rule,
                content: "Main rule".to_string(),
            }],
        });

        let resolved = resolve_full(&main_skill, &repo).unwrap();

        // Should have main rule + 2 included rules
        let rules_section = resolved.spec.sections.iter().find(|s| s.id == "rules").unwrap();
        assert_eq!(rules_section.blocks.len(), 3);
        assert_eq!(rules_section.blocks[0].content, "Main rule");
        assert_eq!(rules_section.blocks[1].content, "Always handle errors");
        assert_eq!(rules_section.blocks[2].content, "Log errors with context");

        // Should track included skills
        assert_eq!(resolved.included_from, vec!["error-handling"]);
    }

    #[test]
    fn test_include_with_prepend() {
        let mut repo = TestRepository::new();

        let error_skill = make_skill_with_rules("error-handling", "Error Handling", vec!["Error rule"]);
        repo.add(error_skill);

        let mut main_skill = make_skill_with_rules("main", "Main", vec!["Main rule"]);
        main_skill.includes.push(SkillInclude {
            skill: "error-handling".to_string(),
            into: IncludeTarget::Rules,
            prefix: None,
            sections: None,
            position: IncludePosition::Prepend,
        });

        let resolved = resolve_full(&main_skill, &repo).unwrap();

        let rules_section = resolved.spec.sections.iter().find(|s| s.id == "rules").unwrap();
        // Error rule should come first (prepended)
        assert_eq!(rules_section.blocks[0].content, "Error rule");
        assert_eq!(rules_section.blocks[1].content, "Main rule");
    }

    #[test]
    fn test_include_with_prefix() {
        let mut repo = TestRepository::new();

        let error_skill = make_skill_with_rules("error-handling", "Error Handling", vec!["Handle errors"]);
        repo.add(error_skill);

        let mut main_skill = SkillSpec::new("main", "Main");
        main_skill.includes.push(SkillInclude {
            skill: "error-handling".to_string(),
            into: IncludeTarget::Rules,
            prefix: Some("[Error] ".to_string()),
            sections: None,
            position: IncludePosition::Append,
        });

        let resolved = resolve_full(&main_skill, &repo).unwrap();

        let rules_section = resolved.spec.sections.iter().find(|s| s.id == "rules").unwrap();
        assert_eq!(rules_section.blocks[0].content, "[Error] Handle errors");
    }

    #[test]
    fn test_multiple_includes() {
        let mut repo = TestRepository::new();

        let error_skill = make_skill_with_rules("error-handling", "Errors", vec!["Error rule"]);
        let testing_skill = make_skill_with_rules("testing", "Testing", vec!["Test rule"]);
        repo.add(error_skill);
        repo.add(testing_skill);

        let mut main_skill = SkillSpec::new("main", "Main");
        main_skill.includes.push(SkillInclude {
            skill: "error-handling".to_string(),
            into: IncludeTarget::Rules,
            prefix: None,
            sections: None,
            position: IncludePosition::Append,
        });
        main_skill.includes.push(SkillInclude {
            skill: "testing".to_string(),
            into: IncludeTarget::Rules,
            prefix: None,
            sections: None,
            position: IncludePosition::Append,
        });

        let resolved = resolve_full(&main_skill, &repo).unwrap();

        let rules_section = resolved.spec.sections.iter().find(|s| s.id == "rules").unwrap();
        assert_eq!(rules_section.blocks.len(), 2);
        assert_eq!(resolved.included_from, vec!["error-handling", "testing"]);
    }

    #[test]
    fn test_include_creates_section_if_missing() {
        let mut repo = TestRepository::new();

        let error_skill = make_skill_with_rules("error-handling", "Errors", vec!["Error rule"]);
        repo.add(error_skill);

        // Main skill has no rules section
        let mut main_skill = SkillSpec::new("main", "Main");
        main_skill.includes.push(SkillInclude {
            skill: "error-handling".to_string(),
            into: IncludeTarget::Rules,
            prefix: None,
            sections: None,
            position: IncludePosition::Append,
        });

        let resolved = resolve_full(&main_skill, &repo).unwrap();

        // Should have created a rules section
        let rules_section = resolved.spec.sections.iter().find(|s| s.id == "rules");
        assert!(rules_section.is_some());
        assert_eq!(rules_section.unwrap().blocks.len(), 1);
    }

    #[test]
    fn test_include_missing_skill_warning() {
        let repo = TestRepository::new();

        let mut main_skill = SkillSpec::new("main", "Main");
        main_skill.includes.push(SkillInclude {
            skill: "nonexistent".to_string(),
            into: IncludeTarget::Rules,
            prefix: None,
            sections: None,
            position: IncludePosition::Append,
        });

        let resolved = resolve_full(&main_skill, &repo).unwrap();

        // Should have a warning about missing skill
        let has_warning = resolved.warnings.iter().any(|w| {
            matches!(w, ResolutionWarning::IncludedSkillNotFound { skill_id, .. } if skill_id == "nonexistent")
        });
        assert!(has_warning);
    }

    #[test]
    fn test_include_with_inheritance() {
        let mut repo = TestRepository::new();

        // Parent skill
        let parent = make_skill_with_rules("parent", "Parent", vec!["Parent rule"]);
        repo.add(parent);

        // Skill to include
        let error_skill = make_skill_with_rules("errors", "Errors", vec!["Error rule"]);
        repo.add(error_skill);

        // Child extends parent and includes errors
        let mut child = make_skill_with_parent("child", "Child", "parent");
        child.includes.push(SkillInclude {
            skill: "errors".to_string(),
            into: IncludeTarget::Rules,
            prefix: None,
            sections: None,
            position: IncludePosition::Append,
        });

        let resolved = resolve_full(&child, &repo).unwrap();

        // Should have parent rule + error rule
        let rules_section = resolved.spec.sections.iter().find(|s| s.id == "rules").unwrap();
        assert_eq!(rules_section.blocks.len(), 2);
        assert_eq!(resolved.inheritance_chain, vec!["parent", "child"]);
        assert_eq!(resolved.included_from, vec!["errors"]);
    }

    #[test]
    fn test_include_cycle_detection() {
        let mut repo = TestRepository::new();

        // A includes B, B includes A
        let mut skill_a = SkillSpec::new("a", "A");
        skill_a.includes.push(SkillInclude {
            skill: "b".to_string(),
            into: IncludeTarget::Rules,
            prefix: None,
            sections: None,
            position: IncludePosition::Append,
        });

        let mut skill_b = SkillSpec::new("b", "B");
        skill_b.includes.push(SkillInclude {
            skill: "a".to_string(),
            into: IncludeTarget::Rules,
            prefix: None,
            sections: None,
            position: IncludePosition::Append,
        });

        repo.add(skill_a.clone());
        repo.add(skill_b);

        let result = detect_full_cycle("a", &repo).unwrap();
        assert!(matches!(result, CycleDetectionResult::CycleFound(_)));

        let err = resolve_full(&skill_a, &repo).unwrap_err();
        assert!(matches!(err, MsError::CyclicInheritance { .. }));
    }

    #[test]
    fn test_many_includes_warning() {
        let mut repo = TestRepository::new();

        // Create 5 skills to include
        for i in 1..=5 {
            let skill = make_skill_with_rules(&format!("skill-{}", i), &format!("Skill {}", i), vec!["Rule"]);
            repo.add(skill);
        }

        let mut main_skill = SkillSpec::new("main", "Main");
        for i in 1..=5 {
            main_skill.includes.push(SkillInclude {
                skill: format!("skill-{}", i),
                into: IncludeTarget::Rules,
                prefix: None,
                sections: None,
                position: IncludePosition::Append,
            });
        }

        let resolved = resolve_full(&main_skill, &repo).unwrap();

        // Should have warning about many includes to same target
        let has_warning = resolved.warnings.iter().any(|w| {
            matches!(w, ResolutionWarning::ManyIncludes { count, .. } if *count == 5)
        });
        assert!(has_warning);
    }

    #[test]
    fn test_includes_cleared_after_resolution() {
        let mut repo = TestRepository::new();

        let error_skill = make_skill_with_rules("errors", "Errors", vec!["Error rule"]);
        repo.add(error_skill);

        let mut main_skill = SkillSpec::new("main", "Main");
        main_skill.includes.push(SkillInclude {
            skill: "errors".to_string(),
            into: IncludeTarget::Rules,
            prefix: None,
            sections: None,
            position: IncludePosition::Append,
        });

        let resolved = resolve_full(&main_skill, &repo).unwrap();

        // Includes should be cleared from resolved spec
        assert!(resolved.spec.includes.is_empty());
    }

    #[test]
    fn test_include_into_different_targets() {
        let mut repo = TestRepository::new();

        // Create a skill with both rules and pitfalls
        let mut source_skill = SkillSpec::new("source", "Source");
        source_skill.sections.push(SkillSection {
            id: "rules".to_string(),
            title: "Rules".to_string(),
            blocks: vec![SkillBlock {
                id: "rule-1".to_string(),
                block_type: BlockType::Rule,
                content: "Source rule".to_string(),
            }],
        });
        source_skill.sections.push(SkillSection {
            id: "pitfalls".to_string(),
            title: "Pitfalls".to_string(),
            blocks: vec![SkillBlock {
                id: "pitfall-1".to_string(),
                block_type: BlockType::Pitfall,
                content: "Source pitfall".to_string(),
            }],
        });
        repo.add(source_skill);

        // Include rules and pitfalls separately
        let mut main_skill = SkillSpec::new("main", "Main");
        main_skill.includes.push(SkillInclude {
            skill: "source".to_string(),
            into: IncludeTarget::Rules,
            prefix: None,
            sections: None,
            position: IncludePosition::Append,
        });
        main_skill.includes.push(SkillInclude {
            skill: "source".to_string(),
            into: IncludeTarget::Pitfalls,
            prefix: None,
            sections: None,
            position: IncludePosition::Append,
        });

        let resolved = resolve_full(&main_skill, &repo).unwrap();

        // Should have both sections
        let rules = resolved.spec.sections.iter().find(|s| s.id == "rules").unwrap();
        let pitfalls = resolved.spec.sections.iter().find(|s| s.id == "pitfalls").unwrap();

        assert_eq!(rules.blocks.len(), 1);
        assert_eq!(pitfalls.blocks.len(), 1);
    }

    #[test]
    fn test_has_includes() {
        let standalone = make_skill("standalone", "Standalone");
        assert!(!standalone.has_includes());

        let mut with_includes = SkillSpec::new("with-includes", "With Includes");
        with_includes.includes.push(SkillInclude {
            skill: "other".to_string(),
            into: IncludeTarget::Rules,
            prefix: None,
            sections: None,
            position: IncludePosition::Append,
        });
        assert!(with_includes.has_includes());
    }
}
