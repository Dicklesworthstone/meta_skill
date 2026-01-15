//! Skill Layering & Conflict Resolution
//!
//! Implements layered skill resolution where the same skill ID can exist at
//! multiple scopes (base/org/project/user). Higher layers override lower layers
//! by default, with explicit conflict reporting and optional merge policies.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::error::Result;

use super::overlay::{OverlayApplicationResult, OverlayContext, SkillOverlay};
use super::skill::{SkillLayer, SkillSection, SkillSpec};

// =============================================================================
// CONFLICT STRATEGIES
// =============================================================================

/// Strategy for resolving conflicts between layers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictStrategy {
    /// Prefer higher-priority layer (user > project > org > base)
    #[default]
    PreferHigher,
    /// Prefer lower-priority layer (base > org > project > user)
    PreferLower,
    /// Require interactive resolution
    Interactive,
}

/// Strategy for merging sections from different layers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeStrategy {
    /// Auto-merge when diffs are non-overlapping
    #[default]
    Auto,
    /// Prefer higher-layer rules/pitfalls but keep lower-layer examples/references
    PreferSections,
    /// No merging - take winner entirely
    Replace,
}

// =============================================================================
// CONFLICT DETAILS
// =============================================================================

/// Details about a conflict between layers for a specific section
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictDetail {
    /// Section ID where the conflict occurred
    pub section_id: String,
    /// Section name/title
    pub section_name: String,
    /// Higher priority layer that won
    pub higher_layer: SkillLayer,
    /// Lower priority layer that was overridden
    pub lower_layer: SkillLayer,
    /// How the conflict was resolved
    pub resolution: ConflictResolution,
    /// Optional diff showing what changed
    pub diff: Option<SectionDiff>,
}

/// How a conflict was resolved
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictResolution {
    /// Higher layer won completely
    HigherWins,
    /// Lower layer won completely
    LowerWins,
    /// Sections were merged
    Merged,
    /// Conflict requires interactive resolution
    Unresolved,
}

/// Diff between two sections
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionDiff {
    /// Blocks that only exist in higher layer
    pub higher_only: Vec<String>,
    /// Blocks that only exist in lower layer
    pub lower_only: Vec<String>,
    /// Blocks that exist in both but differ
    pub modified: Vec<BlockDiff>,
}

/// Diff between two blocks
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockDiff {
    /// Block ID
    pub block_id: String,
    /// Content in higher layer
    pub higher_content: String,
    /// Content in lower layer
    pub lower_content: String,
}

// =============================================================================
// RESOLVED SKILL
// =============================================================================

/// A skill resolved through the layering system
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedSkill {
    /// The effective skill spec after resolution
    pub spec: SkillSpec,
    /// Which layer provided this skill
    pub source_layer: SkillLayer,
    /// Layers that had candidates for this skill
    pub candidate_layers: Vec<SkillLayer>,
    /// Conflicts detected during resolution
    pub conflicts: Vec<ConflictDetail>,
    /// Whether interactive resolution is required
    pub needs_resolution: bool,
    /// Overlay application results
    pub overlay_results: Vec<OverlayApplicationResult>,
}

impl ResolvedSkill {
    /// Create a resolved skill from a single candidate (no conflicts)
    pub fn from_single(spec: SkillSpec, layer: SkillLayer) -> Self {
        Self {
            spec,
            source_layer: layer,
            candidate_layers: vec![layer],
            conflicts: vec![],
            needs_resolution: false,
            overlay_results: vec![],
        }
    }

    /// Check if there were any conflicts
    pub fn has_conflicts(&self) -> bool {
        !self.conflicts.is_empty()
    }
}

// =============================================================================
// SKILL CANDIDATE
// =============================================================================

/// A skill candidate from a specific layer
#[derive(Debug, Clone)]
pub struct SkillCandidate {
    /// The skill spec
    pub spec: SkillSpec,
    /// Which layer this came from
    pub layer: SkillLayer,
    /// Path to the skill source
    pub source_path: String,
}

// =============================================================================
// LAYERED REGISTRY
// =============================================================================

/// Registry that manages skills across multiple layers
#[derive(Debug, Default)]
pub struct LayeredRegistry {
    /// Skills indexed by ID, then by layer
    skills: HashMap<String, HashMap<SkillLayer, SkillCandidate>>,
    /// Overlays indexed by skill ID
    overlays: HashMap<String, Vec<SkillOverlay>>,
    /// Default conflict strategy
    conflict_strategy: ConflictStrategy,
    /// Default merge strategy
    merge_strategy: MergeStrategy,
}

impl LayeredRegistry {
    /// Create a new layered registry
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with specific strategies
    pub fn with_strategies(conflict: ConflictStrategy, merge: MergeStrategy) -> Self {
        Self {
            skills: HashMap::new(),
            overlays: HashMap::new(),
            conflict_strategy: conflict,
            merge_strategy: merge,
        }
    }

    /// Register a skill candidate at a specific layer
    pub fn register(&mut self, candidate: SkillCandidate) {
        let id = candidate.spec.metadata.id.clone();
        self.skills
            .entry(id)
            .or_default()
            .insert(candidate.layer, candidate);
    }

    /// Register an overlay for a skill.
    pub fn register_overlay(&mut self, overlay: SkillOverlay) {
        self.overlays
            .entry(overlay.skill_id.clone())
            .or_default()
            .push(overlay);
    }

    /// Remove a skill candidate from a specific layer
    pub fn unregister(&mut self, id: &str, layer: SkillLayer) -> Option<SkillCandidate> {
        self.skills
            .get_mut(id)
            .and_then(|by_layer| by_layer.remove(&layer))
    }

    /// Get all candidate layers for a skill ID
    pub fn candidate_layers(&self, id: &str) -> Vec<SkillLayer> {
        self.skills
            .get(id)
            .map(|by_layer| {
                let mut layers: Vec<_> = by_layer.keys().copied().collect();
                layers.sort();
                layers
            })
            .unwrap_or_default()
    }

    /// Get effective (resolved) skill by ID
    pub fn effective(&self, id: &str) -> Result<Option<ResolvedSkill>> {
        self.effective_with_strategies(id, self.conflict_strategy, self.merge_strategy)
    }

    /// Get effective skill with custom strategies
    pub fn effective_with_strategies(
        &self,
        id: &str,
        conflict_strategy: ConflictStrategy,
        merge_strategy: MergeStrategy,
    ) -> Result<Option<ResolvedSkill>> {
        let candidates = match self.skills.get(id) {
            Some(by_layer) if !by_layer.is_empty() => by_layer,
            _ => return Ok(None),
        };

        // Collect candidates sorted by layer (highest first for PreferHigher)
        let mut sorted: Vec<_> = candidates.iter().collect();
        sorted.sort_by(|a, b| b.0.cmp(a.0)); // Descending order (User > Project > Org > Base)

        let candidate_layers: Vec<SkillLayer> = sorted.iter().map(|(layer, _)| **layer).collect();

        // Single candidate - no conflicts possible
        if sorted.len() == 1 {
            let (layer, candidate) = sorted[0];
            return Ok(Some(ResolvedSkill::from_single(
                candidate.spec.clone(),
                *layer,
            )));
        }

        // Multiple candidates - resolve conflicts
        let mut resolved = match conflict_strategy {
            ConflictStrategy::PreferHigher => {
                self.resolve_prefer_higher(&sorted, merge_strategy, candidate_layers)
            }
            ConflictStrategy::PreferLower => {
                self.resolve_prefer_lower(&sorted, merge_strategy, candidate_layers)
            }
            ConflictStrategy::Interactive => self.resolve_interactive(&sorted, candidate_layers),
        }?;

        if let Some(ref mut resolved_skill) = resolved {
            let overlay_results =
                self.apply_overlays(id, &mut resolved_skill.spec, &OverlayContext::from_env());
            resolved_skill.overlay_results = overlay_results;
        }

        Ok(resolved)
    }

    /// Resolve by preferring higher layer
    fn resolve_prefer_higher(
        &self,
        sorted: &[(&SkillLayer, &SkillCandidate)],
        merge_strategy: MergeStrategy,
        candidate_layers: Vec<SkillLayer>,
    ) -> Result<Option<ResolvedSkill>> {
        let (winner_layer, winner) = sorted[0];
        let mut conflicts = Vec::new();

        // Compare with lower layers to detect conflicts
        for (lower_layer, lower_candidate) in sorted.iter().skip(1) {
            let section_conflicts = detect_section_conflicts(
                &winner.spec,
                *winner_layer,
                &lower_candidate.spec,
                **lower_layer,
                merge_strategy,
            );
            conflicts.extend(section_conflicts);
        }

        let spec = match merge_strategy {
            MergeStrategy::Replace => winner.spec.clone(),
            MergeStrategy::Auto | MergeStrategy::PreferSections => {
                // Merge sections from lower layers if non-overlapping
                merge_specs(sorted, merge_strategy)
            }
        };

        Ok(Some(ResolvedSkill {
            spec,
            source_layer: *winner_layer,
            candidate_layers,
            conflicts,
            needs_resolution: false,
            overlay_results: vec![],
        }))
    }

    /// Resolve by preferring lower layer
    fn resolve_prefer_lower(
        &self,
        sorted: &[(&SkillLayer, &SkillCandidate)],
        merge_strategy: MergeStrategy,
        candidate_layers: Vec<SkillLayer>,
    ) -> Result<Option<ResolvedSkill>> {
        // Reverse the sort to prefer lower layers
        let mut reversed = sorted.to_vec();
        reversed.reverse();

        let (winner_layer, winner) = reversed[0];
        let mut conflicts = Vec::new();

        for (higher_layer, higher_candidate) in reversed.iter().skip(1) {
            let section_conflicts = detect_section_conflicts(
                &higher_candidate.spec,
                **higher_layer,
                &winner.spec,
                *winner_layer,
                merge_strategy,
            );
            for mut conflict in section_conflicts {
                conflict.resolution = ConflictResolution::LowerWins;
                conflicts.push(conflict);
            }
        }

        // Apply merge strategy - merge non-overlapping sections from higher layers
        let spec = match merge_strategy {
            MergeStrategy::Replace => winner.spec.clone(),
            MergeStrategy::Auto | MergeStrategy::PreferSections => {
                // Pass reversed slice (ascending order) - merge_specs will start with
                // winner (lowest layer) and merge in sections from higher layers
                merge_specs(&reversed, merge_strategy)
            }
        };

        Ok(Some(ResolvedSkill {
            spec,
            source_layer: *winner_layer,
            candidate_layers,
            conflicts,
            needs_resolution: false,
            overlay_results: vec![],
        }))
    }

    /// Resolve interactively (marks as needing resolution)
    fn resolve_interactive(
        &self,
        sorted: &[(&SkillLayer, &SkillCandidate)],
        candidate_layers: Vec<SkillLayer>,
    ) -> Result<Option<ResolvedSkill>> {
        let (winner_layer, winner) = sorted[0];
        let mut conflicts = Vec::new();

        for (lower_layer, lower_candidate) in sorted.iter().skip(1) {
            let mut section_conflicts = detect_section_conflicts(
                &winner.spec,
                *winner_layer,
                &lower_candidate.spec,
                **lower_layer,
                MergeStrategy::Replace, // Don't auto-merge in interactive mode
            );
            for conflict in &mut section_conflicts {
                conflict.resolution = ConflictResolution::Unresolved;
            }
            conflicts.extend(section_conflicts);
        }

        let needs_resolution = !conflicts.is_empty();
        Ok(Some(ResolvedSkill {
            spec: winner.spec.clone(),
            source_layer: *winner_layer,
            candidate_layers,
            conflicts,
            needs_resolution,
            overlay_results: vec![],
        }))
    }

    /// List all skill IDs in the registry
    pub fn list_ids(&self) -> Vec<String> {
        self.skills.keys().cloned().collect()
    }

    /// Get candidate at a specific layer
    pub fn get_at_layer(&self, id: &str, layer: SkillLayer) -> Option<&SkillCandidate> {
        self.skills
            .get(id)
            .and_then(|by_layer| by_layer.get(&layer))
    }
}

impl LayeredRegistry {
    fn apply_overlays(
        &self,
        id: &str,
        spec: &mut SkillSpec,
        context: &OverlayContext,
    ) -> Vec<OverlayApplicationResult> {
        let overlays: &Vec<SkillOverlay> = match self.overlays.get(id) {
            Some(overlays) => overlays,
            None => return Vec::new(),
        };

        let mut sorted = overlays.clone();
        sorted.sort_by(|a, b| a.priority.cmp(&b.priority));

        let mut results = Vec::new();
        for overlay in &sorted {
            results.push(overlay.apply_to(spec, context));
        }
        results
    }
}

// =============================================================================
// CONFLICT DETECTION
// =============================================================================

/// Detect conflicts between two skill specs at the section level
fn detect_section_conflicts(
    higher: &SkillSpec,
    higher_layer: SkillLayer,
    lower: &SkillSpec,
    lower_layer: SkillLayer,
    _merge_strategy: MergeStrategy,
) -> Vec<ConflictDetail> {
    let mut conflicts = Vec::new();

    // Build section map for lower spec
    let lower_sections: HashMap<&str, &SkillSection> =
        lower.sections.iter().map(|s| (s.id.as_str(), s)).collect();

    // Check for sections that exist in both, iterating in order
    for higher_section in &higher.sections {
        if let Some(lower_section) = lower_sections.get(higher_section.id.as_str()) {
            let diff = compute_section_diff(higher_section, lower_section);
            if !diff.higher_only.is_empty()
                || !diff.lower_only.is_empty()
                || !diff.modified.is_empty()
            {
                conflicts.push(ConflictDetail {
                    section_id: higher_section.id.clone(),
                    section_name: higher_section.title.clone(),
                    higher_layer,
                    lower_layer,
                    resolution: ConflictResolution::HigherWins,
                    diff: Some(diff),
                });
            }
        }
    }

    conflicts
}

/// Compute diff between two sections
fn compute_section_diff(higher: &SkillSection, lower: &SkillSection) -> SectionDiff {
    let higher_blocks_map: HashMap<&str, &str> = higher
        .blocks
        .iter()
        .map(|b| (b.id.as_str(), b.content.as_str()))
        .collect();

    let lower_blocks_map: HashMap<&str, &str> = lower
        .blocks
        .iter()
        .map(|b| (b.id.as_str(), b.content.as_str()))
        .collect();

    let mut higher_only = Vec::new();
    let mut lower_only = Vec::new();
    let mut modified = Vec::new();

    // Find blocks only in higher or modified (iterating in order)
    for block in &higher.blocks {
        if !lower_blocks_map.contains_key(block.id.as_str()) {
            higher_only.push(block.id.clone());
        } else if lower_blocks_map.get(block.id.as_str()) != Some(&block.content.as_str()) {
            modified.push(BlockDiff {
                block_id: block.id.clone(),
                higher_content: block.content.clone(),
                lower_content: lower_blocks_map.get(block.id.as_str()).unwrap().to_string(),
            });
        }
    }

    // Find blocks only in lower (iterating in order)
    for block in &lower.blocks {
        if !higher_blocks_map.contains_key(block.id.as_str()) {
            lower_only.push(block.id.clone());
        }
    }

    SectionDiff {
        higher_only,
        lower_only,
        modified,
    }
}

// =============================================================================
// MERGING
// =============================================================================

/// Merge specs from multiple layers
fn merge_specs(
    sorted: &[(&SkillLayer, &SkillCandidate)],
    merge_strategy: MergeStrategy,
) -> SkillSpec {
    if sorted.is_empty() {
        return SkillSpec::new("", "");
    }

    // Start with the highest priority spec
    let mut result = sorted[0].1.spec.clone();

    match merge_strategy {
        MergeStrategy::Replace => return result,
        MergeStrategy::Auto => {
            // Add sections from lower layers that don't exist in higher
            for (_, candidate) in sorted.iter().skip(1) {
                merge_non_overlapping_sections(&mut result, &candidate.spec);
            }
        }
        MergeStrategy::PreferSections => {
            // Keep higher-layer rules/pitfalls, add lower-layer examples/references
            for (_, candidate) in sorted.iter().skip(1) {
                merge_by_section_preference(&mut result, &candidate.spec);
            }
        }
    }

    result
}

/// Merge non-overlapping sections from source into target
fn merge_non_overlapping_sections(target: &mut SkillSpec, source: &SkillSpec) {
    let existing_ids: std::collections::HashSet<String> =
        target.sections.iter().map(|s| s.id.clone()).collect();

    let to_add: Vec<_> = source
        .sections
        .iter()
        .filter(|section| !existing_ids.contains(&section.id))
        .cloned()
        .collect();

    target.sections.extend(to_add);
}

/// Merge by section preference: keep higher-layer rules/pitfalls, add lower-layer examples
fn merge_by_section_preference(target: &mut SkillSpec, source: &SkillSpec) {
    let existing_ids: std::collections::HashSet<String> =
        target.sections.iter().map(|s| s.id.clone()).collect();

    let to_add: Vec<_> = source
        .sections
        .iter()
        .filter(|section| {
            if existing_ids.contains(&section.id) {
                return false;
            }
            // Only add example/reference type sections from lower layers
            let section_lower = section.title.to_lowercase();
            section_lower.contains("example")
                || section_lower.contains("reference")
                || section_lower.contains("template")
        })
        .cloned()
        .collect();

    target.sections.extend(to_add);
}

// =============================================================================
// RESOLUTION HELPERS
// =============================================================================

/// Resolution options for interactive conflict resolution
#[derive(Debug, Clone)]
pub struct ResolutionOptions {
    /// Skill ID being resolved
    pub skill_id: String,
    /// Conflicts requiring resolution
    pub conflicts: Vec<ConflictDetail>,
    /// Available layers to choose from
    pub available_layers: Vec<SkillLayer>,
}

impl LayeredRegistry {
    /// Get resolution options for a skill with conflicts
    pub fn get_resolution_options(&self, id: &str) -> Option<ResolutionOptions> {
        let candidates = self.skills.get(id)?;
        if candidates.len() <= 1 {
            return None;
        }

        let mut sorted: Vec<_> = candidates.iter().collect();
        sorted.sort_by(|a, b| b.0.cmp(a.0));

        let mut conflicts = Vec::new();
        let (winner_layer, winner) = sorted[0];

        for (lower_layer, lower_candidate) in sorted.iter().skip(1) {
            let section_conflicts = detect_section_conflicts(
                &winner.spec,
                *winner_layer,
                &lower_candidate.spec,
                **lower_layer,
                MergeStrategy::Replace,
            );
            for mut conflict in section_conflicts {
                conflict.resolution = ConflictResolution::Unresolved;
                conflicts.push(conflict);
            }
        }

        if conflicts.is_empty() {
            return None;
        }

        Some(ResolutionOptions {
            skill_id: id.to_string(),
            conflicts,
            available_layers: sorted.iter().map(|(layer, _)| **layer).collect(),
        })
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::skill::{BlockType, SkillBlock, SkillMetadata};

    fn make_skill_spec(id: &str, name: &str, sections: Vec<SkillSection>) -> SkillSpec {
        SkillSpec {
            format_version: SkillSpec::FORMAT_VERSION.to_string(),
            metadata: SkillMetadata {
                id: id.to_string(),
                name: name.to_string(),
                ..Default::default()
            },
            sections,
        }
    }

    fn make_section(id: &str, title: &str, blocks: Vec<(&str, &str)>) -> SkillSection {
        SkillSection {
            id: id.to_string(),
            title: title.to_string(),
            blocks: blocks
                .into_iter()
                .map(|(bid, content)| SkillBlock {
                    id: bid.to_string(),
                    block_type: BlockType::Text,
                    content: content.to_string(),
                })
                .collect(),
        }
    }

    #[test]
    fn test_single_candidate_no_conflicts() {
        let mut registry = LayeredRegistry::new();

        let spec = make_skill_spec("test-skill", "Test Skill", vec![]);
        registry.register(SkillCandidate {
            spec: spec.clone(),
            layer: SkillLayer::Project,
            source_path: "project/.ms/skills/test".to_string(),
        });

        let resolved = registry.effective("test-skill").unwrap().unwrap();
        assert_eq!(resolved.source_layer, SkillLayer::Project);
        assert!(!resolved.has_conflicts());
        assert!(!resolved.needs_resolution);
    }

    #[test]
    fn test_layer_precedence_higher_wins() {
        let mut registry = LayeredRegistry::new();

        let base_spec = make_skill_spec(
            "test-skill",
            "Base Skill",
            vec![make_section(
                "s1",
                "Section One",
                vec![("b1", "base content")],
            )],
        );
        let user_spec = make_skill_spec(
            "test-skill",
            "User Skill",
            vec![make_section(
                "s1",
                "Section One",
                vec![("b1", "user content")],
            )],
        );

        registry.register(SkillCandidate {
            spec: base_spec,
            layer: SkillLayer::Base,
            source_path: "base".to_string(),
        });
        registry.register(SkillCandidate {
            spec: user_spec,
            layer: SkillLayer::User,
            source_path: "user".to_string(),
        });

        let resolved = registry.effective("test-skill").unwrap().unwrap();
        assert_eq!(resolved.source_layer, SkillLayer::User);
        assert_eq!(resolved.spec.metadata.name, "User Skill");
        assert!(resolved.has_conflicts()); // Same section exists in both
    }

    #[test]
    fn test_candidate_layers_ordering() {
        let mut registry = LayeredRegistry::new();

        registry.register(SkillCandidate {
            spec: make_skill_spec("test", "Test", vec![]),
            layer: SkillLayer::User,
            source_path: "user".to_string(),
        });
        registry.register(SkillCandidate {
            spec: make_skill_spec("test", "Test", vec![]),
            layer: SkillLayer::Base,
            source_path: "base".to_string(),
        });
        registry.register(SkillCandidate {
            spec: make_skill_spec("test", "Test", vec![]),
            layer: SkillLayer::Project,
            source_path: "project".to_string(),
        });

        let layers = registry.candidate_layers("test");
        assert_eq!(
            layers,
            vec![SkillLayer::Base, SkillLayer::Project, SkillLayer::User]
        );
    }

    #[test]
    fn test_interactive_strategy_marks_unresolved() {
        let mut registry =
            LayeredRegistry::with_strategies(ConflictStrategy::Interactive, MergeStrategy::Replace);

        let base_spec = make_skill_spec(
            "test",
            "Base",
            vec![make_section("s1", "Section", vec![("b1", "base")])],
        );
        let user_spec = make_skill_spec(
            "test",
            "User",
            vec![make_section("s1", "Section", vec![("b1", "user")])],
        );

        registry.register(SkillCandidate {
            spec: base_spec,
            layer: SkillLayer::Base,
            source_path: "base".to_string(),
        });
        registry.register(SkillCandidate {
            spec: user_spec,
            layer: SkillLayer::User,
            source_path: "user".to_string(),
        });

        let resolved = registry.effective("test").unwrap().unwrap();
        assert!(resolved.needs_resolution);
        assert!(
            resolved
                .conflicts
                .iter()
                .all(|c| matches!(c.resolution, ConflictResolution::Unresolved))
        );
    }

    #[test]
    fn test_merge_non_overlapping_sections() {
        let mut registry =
            LayeredRegistry::with_strategies(ConflictStrategy::PreferHigher, MergeStrategy::Auto);

        let base_spec = make_skill_spec(
            "test",
            "Test",
            vec![make_section(
                "examples",
                "Examples",
                vec![("e1", "example")],
            )],
        );
        let user_spec = make_skill_spec(
            "test",
            "Test",
            vec![make_section("rules", "Rules", vec![("r1", "rule")])],
        );

        registry.register(SkillCandidate {
            spec: base_spec,
            layer: SkillLayer::Base,
            source_path: "base".to_string(),
        });
        registry.register(SkillCandidate {
            spec: user_spec,
            layer: SkillLayer::User,
            source_path: "user".to_string(),
        });

        let resolved = registry.effective("test").unwrap().unwrap();
        // Should have both sections merged
        assert_eq!(resolved.spec.sections.len(), 2);
    }

    #[test]
    fn test_unregister() {
        let mut registry = LayeredRegistry::new();

        registry.register(SkillCandidate {
            spec: make_skill_spec("test", "Test", vec![]),
            layer: SkillLayer::User,
            source_path: "user".to_string(),
        });

        assert!(registry.effective("test").unwrap().is_some());

        let removed = registry.unregister("test", SkillLayer::User);
        assert!(removed.is_some());

        assert!(registry.effective("test").unwrap().is_none());
    }

    #[test]
    fn test_prefer_lower_with_auto_merge() {
        // PreferLower + Auto should take base layer but merge in non-overlapping sections from user
        let mut registry =
            LayeredRegistry::with_strategies(ConflictStrategy::PreferLower, MergeStrategy::Auto);

        let base_spec = make_skill_spec(
            "test",
            "Base Skill",
            vec![make_section("rules", "Rules", vec![("r1", "base rule")])],
        );
        let user_spec = make_skill_spec(
            "test",
            "User Skill",
            vec![make_section(
                "examples",
                "Examples",
                vec![("e1", "user example")],
            )],
        );

        registry.register(SkillCandidate {
            spec: base_spec,
            layer: SkillLayer::Base,
            source_path: "base".to_string(),
        });
        registry.register(SkillCandidate {
            spec: user_spec,
            layer: SkillLayer::User,
            source_path: "user".to_string(),
        });

        let resolved = registry.effective("test").unwrap().unwrap();
        // Source layer should be Base (lower wins)
        assert_eq!(resolved.source_layer, SkillLayer::Base);
        // But should have both sections merged (non-overlapping)
        assert_eq!(resolved.spec.sections.len(), 2);
        // Name should be from base layer
        assert_eq!(resolved.spec.metadata.name, "Base Skill");
    }

    #[test]
    fn test_conflict_determinism() {
        // Create specs with multiple sections in a specific order
        let sections = vec![
            make_section("s1", "First", vec![("b1", "A")]),
            make_section("s2", "Second", vec![("b2", "B")]),
            make_section("s3", "Third", vec![("b3", "C")]),
        ];
        let higher = make_skill_spec("test", "Test", sections);

        // Create lower spec with conflicting content in all sections
        let lower_sections = vec![
            make_section("s1", "First", vec![("b1", "X")]),
            make_section("s2", "Second", vec![("b2", "Y")]),
            make_section("s3", "Third", vec![("b3", "Z")]),
        ];
        let lower = make_skill_spec("test", "Test", lower_sections);

        // Detect conflicts
        let conflicts = detect_section_conflicts(
            &higher,
            SkillLayer::User,
            &lower,
            SkillLayer::Base,
            MergeStrategy::Auto,
        );

        // Verify order matches 'higher' spec order (s1, s2, s3)
        assert_eq!(conflicts.len(), 3);
        assert_eq!(conflicts[0].section_id, "s1");
        assert_eq!(conflicts[1].section_id, "s2");
        assert_eq!(conflicts[2].section_id, "s3");
    }
}
