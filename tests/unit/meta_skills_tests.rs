//! Unit tests for the meta_skills module.
//!
//! Tests cover:
//! - MetaSkill validation
//! - MetaSkillSliceRef validation
//! - MetaSkillMetadata defaults
//! - PinStrategy serialization
//! - SliceCondition serialization
//! - MetaSkillDoc conversion
//! - MetaSkillParser edge cases
//! - MetaSkillRegistry CRUD and search
//! - ConditionContext evaluation

use std::collections::HashSet;
use std::path::Path;

use ms::meta_skills::{
    ConditionContext, MetaDisclosureLevel, MetaSkill, MetaSkillDoc, MetaSkillHeader,
    MetaSkillMetadata, MetaSkillParser, MetaSkillQuery, MetaSkillRegistry, MetaSkillSliceRef,
    PinStrategy, SliceCondition,
};

// ============================================================================
// Test Fixtures
// ============================================================================

fn valid_slice_ref() -> MetaSkillSliceRef {
    MetaSkillSliceRef {
        skill_id: "test-skill".to_string(),
        slice_ids: vec!["slice-1".to_string()],
        level: None,
        priority: 0,
        required: false,
        conditions: vec![],
    }
}

fn valid_meta_skill() -> MetaSkill {
    MetaSkill {
        id: "test-meta".to_string(),
        name: "Test Meta-Skill".to_string(),
        description: "A test meta-skill".to_string(),
        slices: vec![valid_slice_ref()],
        pin_strategy: PinStrategy::LatestCompatible,
        metadata: MetaSkillMetadata::default(),
        min_context_tokens: 0,
        recommended_context_tokens: 0,
    }
}

fn meta_skill_with_tags(tags: Vec<&str>, tech_stacks: Vec<&str>) -> MetaSkill {
    let mut meta = valid_meta_skill();
    meta.metadata.tags = tags.into_iter().map(String::from).collect();
    meta.metadata.tech_stacks = tech_stacks.into_iter().map(String::from).collect();
    meta
}

// ============================================================================
// MetaSkill Validation Tests
// ============================================================================

#[test]
fn meta_skill_validate_valid() {
    let meta = valid_meta_skill();
    assert!(meta.validate().is_ok());
}

#[test]
fn meta_skill_validate_empty_id() {
    let mut meta = valid_meta_skill();
    meta.id = "".to_string();
    let err = meta.validate().unwrap_err();
    assert!(err.to_string().contains("id must be non-empty"));
}

#[test]
fn meta_skill_validate_whitespace_id() {
    let mut meta = valid_meta_skill();
    meta.id = "   ".to_string();
    let err = meta.validate().unwrap_err();
    assert!(err.to_string().contains("id must be non-empty"));
}

#[test]
fn meta_skill_validate_empty_name() {
    let mut meta = valid_meta_skill();
    meta.name = "".to_string();
    let err = meta.validate().unwrap_err();
    assert!(err.to_string().contains("name must be non-empty"));
}

#[test]
fn meta_skill_validate_whitespace_name() {
    let mut meta = valid_meta_skill();
    meta.name = "   ".to_string();
    let err = meta.validate().unwrap_err();
    assert!(err.to_string().contains("name must be non-empty"));
}

#[test]
fn meta_skill_validate_empty_description() {
    let mut meta = valid_meta_skill();
    meta.description = "".to_string();
    let err = meta.validate().unwrap_err();
    assert!(err.to_string().contains("description must be non-empty"));
}

#[test]
fn meta_skill_validate_no_slices() {
    let mut meta = valid_meta_skill();
    meta.slices = vec![];
    let err = meta.validate().unwrap_err();
    assert!(err.to_string().contains("at least one slice"));
}

#[test]
fn meta_skill_validate_invalid_context_tokens() {
    let mut meta = valid_meta_skill();
    meta.min_context_tokens = 100;
    meta.recommended_context_tokens = 50;
    let err = meta.validate().unwrap_err();
    assert!(err
        .to_string()
        .contains("recommended_context_tokens must be >= min_context_tokens"));
}

#[test]
fn meta_skill_validate_context_tokens_zero_ok() {
    let mut meta = valid_meta_skill();
    meta.min_context_tokens = 0;
    meta.recommended_context_tokens = 0;
    assert!(meta.validate().is_ok());
}

#[test]
fn meta_skill_validate_context_tokens_equal_ok() {
    let mut meta = valid_meta_skill();
    meta.min_context_tokens = 100;
    meta.recommended_context_tokens = 100;
    assert!(meta.validate().is_ok());
}

#[test]
fn meta_skill_validate_context_tokens_recommended_greater_ok() {
    let mut meta = valid_meta_skill();
    meta.min_context_tokens = 100;
    meta.recommended_context_tokens = 200;
    assert!(meta.validate().is_ok());
}

#[test]
fn meta_skill_validate_invalid_slice_ref() {
    let mut meta = valid_meta_skill();
    meta.slices = vec![MetaSkillSliceRef {
        skill_id: "".to_string(),
        slice_ids: vec![],
        level: None,
        priority: 0,
        required: false,
        conditions: vec![],
    }];
    let err = meta.validate().unwrap_err();
    assert!(err.to_string().contains("skill_id must be non-empty"));
}

// ============================================================================
// MetaSkillSliceRef Validation Tests
// ============================================================================

#[test]
fn slice_ref_validate_valid() {
    let slice = valid_slice_ref();
    assert!(slice.validate().is_ok());
}

#[test]
fn slice_ref_validate_empty_skill_id() {
    let slice = MetaSkillSliceRef {
        skill_id: "".to_string(),
        slice_ids: vec![],
        level: None,
        priority: 0,
        required: false,
        conditions: vec![],
    };
    let err = slice.validate().unwrap_err();
    assert!(err.to_string().contains("skill_id must be non-empty"));
}

#[test]
fn slice_ref_validate_whitespace_skill_id() {
    let slice = MetaSkillSliceRef {
        skill_id: "  ".to_string(),
        slice_ids: vec![],
        level: None,
        priority: 0,
        required: false,
        conditions: vec![],
    };
    let err = slice.validate().unwrap_err();
    assert!(err.to_string().contains("skill_id must be non-empty"));
}

#[test]
fn slice_ref_validate_empty_slice_ids_ok() {
    let slice = MetaSkillSliceRef {
        skill_id: "skill".to_string(),
        slice_ids: vec![],
        level: None,
        priority: 0,
        required: false,
        conditions: vec![],
    };
    assert!(slice.validate().is_ok());
}

// ============================================================================
// MetaSkillMetadata Tests
// ============================================================================

#[test]
fn metadata_default_version() {
    let meta = MetaSkillMetadata::default();
    assert_eq!(meta.version, "0.1.0");
}

#[test]
fn metadata_default_empty_tags() {
    let meta = MetaSkillMetadata::default();
    assert!(meta.tags.is_empty());
}

#[test]
fn metadata_default_empty_tech_stacks() {
    let meta = MetaSkillMetadata::default();
    assert!(meta.tech_stacks.is_empty());
}

#[test]
fn metadata_default_no_author() {
    let meta = MetaSkillMetadata::default();
    assert!(meta.author.is_none());
}

#[test]
fn metadata_default_no_updated_at() {
    let meta = MetaSkillMetadata::default();
    assert!(meta.updated_at.is_none());
}

// ============================================================================
// MetaDisclosureLevel Tests
// ============================================================================

#[test]
fn disclosure_level_core_serde() {
    let level = MetaDisclosureLevel::Core;
    let json = serde_json::to_string(&level).unwrap();
    assert_eq!(json, "\"core\"");
    let parsed: MetaDisclosureLevel = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, MetaDisclosureLevel::Core);
}

#[test]
fn disclosure_level_extended_serde() {
    let level = MetaDisclosureLevel::Extended;
    let json = serde_json::to_string(&level).unwrap();
    assert_eq!(json, "\"extended\"");
    let parsed: MetaDisclosureLevel = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, MetaDisclosureLevel::Extended);
}

#[test]
fn disclosure_level_deep_serde() {
    let level = MetaDisclosureLevel::Deep;
    let json = serde_json::to_string(&level).unwrap();
    assert_eq!(json, "\"deep\"");
    let parsed: MetaDisclosureLevel = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, MetaDisclosureLevel::Deep);
}

// ============================================================================
// PinStrategy Tests
// ============================================================================

#[test]
fn pin_strategy_default_is_latest_compatible() {
    let strategy = PinStrategy::default();
    assert_eq!(strategy, PinStrategy::LatestCompatible);
}

#[test]
fn pin_strategy_latest_compatible_serde() {
    let strategy = PinStrategy::LatestCompatible;
    let json = serde_json::to_string(&strategy).unwrap();
    let parsed: PinStrategy = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, PinStrategy::LatestCompatible);
}

#[test]
fn pin_strategy_floating_major_serde() {
    let strategy = PinStrategy::FloatingMajor;
    let json = serde_json::to_string(&strategy).unwrap();
    let parsed: PinStrategy = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, PinStrategy::FloatingMajor);
}

#[test]
fn pin_strategy_local_installed_serde() {
    let strategy = PinStrategy::LocalInstalled;
    let json = serde_json::to_string(&strategy).unwrap();
    let parsed: PinStrategy = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, PinStrategy::LocalInstalled);
}

#[test]
fn pin_strategy_exact_version_serde() {
    let strategy = PinStrategy::ExactVersion("1.2.3".to_string());
    let json = serde_json::to_string(&strategy).unwrap();
    let parsed: PinStrategy = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, PinStrategy::ExactVersion("1.2.3".to_string()));
}

#[test]
fn pin_strategy_per_skill_serde() {
    let mut map = std::collections::HashMap::new();
    map.insert("skill-a".to_string(), "1.0.0".to_string());
    map.insert("skill-b".to_string(), "2.0.0".to_string());

    let strategy = PinStrategy::PerSkill(map);
    let json = serde_json::to_string(&strategy).unwrap();
    let parsed: PinStrategy = serde_json::from_str(&json).unwrap();

    if let PinStrategy::PerSkill(parsed_map) = parsed {
        assert_eq!(parsed_map.get("skill-a"), Some(&"1.0.0".to_string()));
        assert_eq!(parsed_map.get("skill-b"), Some(&"2.0.0".to_string()));
    } else {
        panic!("Expected PerSkill variant");
    }
}

// ============================================================================
// SliceCondition Tests
// ============================================================================

#[test]
fn slice_condition_tech_stack_serde() {
    let condition = SliceCondition::TechStack {
        value: "rust".to_string(),
    };
    let json = serde_json::to_string(&condition).unwrap();
    assert!(json.contains("\"type\":\"tech_stack\""));
    assert!(json.contains("\"value\":\"rust\""));

    let parsed: SliceCondition = serde_json::from_str(&json).unwrap();
    if let SliceCondition::TechStack { value } = parsed {
        assert_eq!(value, "rust");
    } else {
        panic!("Expected TechStack variant");
    }
}

#[test]
fn slice_condition_file_exists_serde() {
    let condition = SliceCondition::FileExists {
        value: "Cargo.toml".to_string(),
    };
    let json = serde_json::to_string(&condition).unwrap();
    assert!(json.contains("\"type\":\"file_exists\""));

    let parsed: SliceCondition = serde_json::from_str(&json).unwrap();
    if let SliceCondition::FileExists { value } = parsed {
        assert_eq!(value, "Cargo.toml");
    } else {
        panic!("Expected FileExists variant");
    }
}

#[test]
fn slice_condition_env_var_serde() {
    let condition = SliceCondition::EnvVar {
        value: "HOME".to_string(),
    };
    let json = serde_json::to_string(&condition).unwrap();
    assert!(json.contains("\"type\":\"env_var\""));

    let parsed: SliceCondition = serde_json::from_str(&json).unwrap();
    if let SliceCondition::EnvVar { value } = parsed {
        assert_eq!(value, "HOME");
    } else {
        panic!("Expected EnvVar variant");
    }
}

#[test]
fn slice_condition_depends_on_serde() {
    let condition = SliceCondition::DependsOn {
        skill_id: "skill-a".to_string(),
        slice_id: "slice-1".to_string(),
    };
    let json = serde_json::to_string(&condition).unwrap();
    assert!(json.contains("\"type\":\"depends_on\""));

    let parsed: SliceCondition = serde_json::from_str(&json).unwrap();
    if let SliceCondition::DependsOn { skill_id, slice_id } = parsed {
        assert_eq!(skill_id, "skill-a");
        assert_eq!(slice_id, "slice-1");
    } else {
        panic!("Expected DependsOn variant");
    }
}

// ============================================================================
// MetaSkillParser Tests
// ============================================================================

#[test]
fn parser_parse_minimal_toml() {
    let toml = r#"
        [meta_skill]
        id = "test"
        name = "Test"
        description = "A test"

        [[slices]]
        skill_id = "skill-1"
    "#;

    let result = MetaSkillParser::parse_str(toml, Path::new("test.toml"));
    assert!(result.is_ok());
    let meta = result.unwrap();
    assert_eq!(meta.id, "test");
    assert_eq!(meta.name, "Test");
    assert_eq!(meta.slices.len(), 1);
}

#[test]
fn parser_parse_full_toml() {
    let toml = r#"
        [meta_skill]
        id = "full-meta"
        name = "Full Meta"
        description = "A fully specified meta-skill"
        min_context_tokens = 100
        recommended_context_tokens = 500

        [meta_skill.pin_strategy]
        exact_version = "1.2.3"

        [meta_skill.metadata]
        author = "test@example.com"
        version = "2.0.0"
        tags = ["tag1", "tag2"]
        tech_stacks = ["rust", "python"]

        [[slices]]
        skill_id = "skill-a"
        slice_ids = ["slice-1", "slice-2"]
        level = "extended"
        priority = 10
        required = true

        [[slices.conditions]]
        type = "tech_stack"
        value = "rust"

        [[slices]]
        skill_id = "skill-b"
        priority = 5
    "#;

    let result = MetaSkillParser::parse_str(toml, Path::new("full.toml"));
    assert!(result.is_ok());
    let meta = result.unwrap();

    assert_eq!(meta.id, "full-meta");
    assert_eq!(meta.min_context_tokens, 100);
    assert_eq!(meta.recommended_context_tokens, 500);
    assert_eq!(meta.metadata.author, Some("test@example.com".to_string()));
    assert_eq!(meta.metadata.version, "2.0.0");
    assert_eq!(meta.metadata.tags, vec!["tag1", "tag2"]);
    assert_eq!(meta.slices.len(), 2);
    assert!(meta.slices[0].required);
    assert_eq!(meta.slices[0].level, Some(MetaDisclosureLevel::Extended));
}

#[test]
fn parser_parse_invalid_toml() {
    let toml = "not valid toml at all {{{";
    let result = MetaSkillParser::parse_str(toml, Path::new("bad.toml"));
    assert!(result.is_err());
}

#[test]
fn parser_parse_missing_required_fields() {
    let toml = r#"
        [meta_skill]
        id = "test"
    "#;
    let result = MetaSkillParser::parse_str(toml, Path::new("missing.toml"));
    assert!(result.is_err());
}

#[test]
fn parser_parse_invalid_slice_fails_validation() {
    let toml = r#"
        [meta_skill]
        id = "test"
        name = "Test"
        description = "Test"

        [[slices]]
        skill_id = ""
    "#;
    let result = MetaSkillParser::parse_str(toml, Path::new("invalid-slice.toml"));
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("skill_id"));
}

#[test]
fn parser_parse_no_slices_fails_validation() {
    let toml = r#"
        [meta_skill]
        id = "test"
        name = "Test"
        description = "Test"
    "#;
    let result = MetaSkillParser::parse_str(toml, Path::new("no-slices.toml"));
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("at least one slice"));
}

// ============================================================================
// MetaSkillRegistry Tests
// ============================================================================

#[test]
fn registry_new_empty() {
    let registry = MetaSkillRegistry::new();
    let stats = registry.stats();
    assert_eq!(stats.total, 0);
}

#[test]
fn registry_insert_and_get() {
    let mut registry = MetaSkillRegistry::new();
    let meta = valid_meta_skill();
    registry.insert(meta.clone()).unwrap();

    let retrieved = registry.get("test-meta");
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().name, "Test Meta-Skill");
}

#[test]
fn registry_insert_invalid_fails() {
    let mut registry = MetaSkillRegistry::new();
    let mut meta = valid_meta_skill();
    meta.id = "".to_string();

    let result = registry.insert(meta);
    assert!(result.is_err());
}

#[test]
fn registry_insert_updates_existing() {
    let mut registry = MetaSkillRegistry::new();

    let mut meta1 = valid_meta_skill();
    meta1.name = "Original Name".to_string();
    registry.insert(meta1).unwrap();

    let mut meta2 = valid_meta_skill();
    meta2.name = "Updated Name".to_string();
    registry.insert(meta2).unwrap();

    let retrieved = registry.get("test-meta").unwrap();
    assert_eq!(retrieved.name, "Updated Name");

    let stats = registry.stats();
    assert_eq!(stats.total, 1);
}

#[test]
fn registry_get_nonexistent() {
    let registry = MetaSkillRegistry::new();
    assert!(registry.get("nonexistent").is_none());
}

#[test]
fn registry_all_returns_all() {
    let mut registry = MetaSkillRegistry::new();

    let mut meta1 = valid_meta_skill();
    meta1.id = "meta-1".to_string();
    registry.insert(meta1).unwrap();

    let mut meta2 = valid_meta_skill();
    meta2.id = "meta-2".to_string();
    registry.insert(meta2).unwrap();

    let all = registry.all();
    assert_eq!(all.len(), 2);
}

#[test]
fn registry_indexes_tags() {
    let mut registry = MetaSkillRegistry::new();

    let meta = meta_skill_with_tags(vec!["rust", "cli"], vec![]);
    registry.insert(meta).unwrap();

    let stats = registry.stats();
    assert_eq!(stats.tags_indexed, 2);
}

#[test]
fn registry_indexes_tech_stacks() {
    let mut registry = MetaSkillRegistry::new();

    let meta = meta_skill_with_tags(vec![], vec!["rust", "python"]);
    registry.insert(meta).unwrap();

    let stats = registry.stats();
    assert_eq!(stats.tech_stacks_indexed, 2);
}

#[test]
fn registry_update_removes_old_index_entries() {
    let mut registry = MetaSkillRegistry::new();

    let meta1 = meta_skill_with_tags(vec!["old-tag"], vec!["old-stack"]);
    registry.insert(meta1).unwrap();

    let mut meta2 = valid_meta_skill();
    meta2.metadata.tags = vec!["new-tag".to_string()];
    meta2.metadata.tech_stacks = vec!["new-stack".to_string()];
    registry.insert(meta2).unwrap();

    let stats = registry.stats();
    // Note: Registry keeps empty index keys - the IDs are removed from lists
    // but the keys remain in the hashmaps. So we have 2 keys (old + new).
    // The important thing is that search won't find the old tags.
    assert_eq!(stats.tags_indexed, 2);
    assert_eq!(stats.tech_stacks_indexed, 2);
    assert_eq!(stats.total, 1); // Only one meta-skill

    // Verify old tags don't match anymore
    let query = MetaSkillQuery {
        tags: vec!["old-tag".to_string()],
        ..Default::default()
    };
    let results = registry.search(&query);
    assert!(results.is_empty(), "Old tag should not match after update");
}

// ============================================================================
// MetaSkillQuery and Search Tests
// ============================================================================

#[test]
fn registry_search_empty_query_returns_all() {
    let mut registry = MetaSkillRegistry::new();
    registry.insert(valid_meta_skill()).unwrap();

    let query = MetaSkillQuery::default();
    let results = registry.search(&query);
    assert_eq!(results.len(), 1);
}

#[test]
fn registry_search_by_text_in_id() {
    let mut registry = MetaSkillRegistry::new();

    let mut meta1 = valid_meta_skill();
    meta1.id = "rust-guide".to_string();
    registry.insert(meta1).unwrap();

    let mut meta2 = valid_meta_skill();
    meta2.id = "python-guide".to_string();
    registry.insert(meta2).unwrap();

    let query = MetaSkillQuery {
        text: Some("rust".to_string()),
        ..Default::default()
    };
    let results = registry.search(&query);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "rust-guide");
}

#[test]
fn registry_search_by_text_in_name() {
    let mut registry = MetaSkillRegistry::new();

    let mut meta1 = valid_meta_skill();
    meta1.id = "meta-1".to_string();
    meta1.name = "Rust Guide".to_string();
    registry.insert(meta1).unwrap();

    let mut meta2 = valid_meta_skill();
    meta2.id = "meta-2".to_string();
    meta2.name = "Python Guide".to_string();
    registry.insert(meta2).unwrap();

    let query = MetaSkillQuery {
        text: Some("Python".to_string()),
        ..Default::default()
    };
    let results = registry.search(&query);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "Python Guide");
}

#[test]
fn registry_search_by_text_in_description() {
    let mut registry = MetaSkillRegistry::new();

    let mut meta1 = valid_meta_skill();
    meta1.id = "meta-1".to_string();
    meta1.description = "Guide for backend development".to_string();
    registry.insert(meta1).unwrap();

    let mut meta2 = valid_meta_skill();
    meta2.id = "meta-2".to_string();
    meta2.description = "Guide for frontend development".to_string();
    registry.insert(meta2).unwrap();

    let query = MetaSkillQuery {
        text: Some("backend".to_string()),
        ..Default::default()
    };
    let results = registry.search(&query);
    assert_eq!(results.len(), 1);
}

#[test]
fn registry_search_text_case_insensitive() {
    let mut registry = MetaSkillRegistry::new();

    let mut meta = valid_meta_skill();
    meta.name = "RUST GUIDE".to_string();
    registry.insert(meta).unwrap();

    let query = MetaSkillQuery {
        text: Some("rust".to_string()),
        ..Default::default()
    };
    let results = registry.search(&query);
    assert_eq!(results.len(), 1);
}

#[test]
fn registry_search_by_tag() {
    let mut registry = MetaSkillRegistry::new();

    let mut meta1 = meta_skill_with_tags(vec!["rust"], vec![]);
    meta1.id = "meta-1".to_string();
    registry.insert(meta1).unwrap();

    let mut meta2 = meta_skill_with_tags(vec!["python"], vec![]);
    meta2.id = "meta-2".to_string();
    registry.insert(meta2).unwrap();

    let query = MetaSkillQuery {
        tags: vec!["rust".to_string()],
        ..Default::default()
    };
    let results = registry.search(&query);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "meta-1");
}

#[test]
fn registry_search_by_multiple_tags() {
    let mut registry = MetaSkillRegistry::new();

    let mut meta1 = meta_skill_with_tags(vec!["rust", "cli"], vec![]);
    meta1.id = "meta-1".to_string();
    registry.insert(meta1).unwrap();

    let mut meta2 = meta_skill_with_tags(vec!["rust", "web"], vec![]);
    meta2.id = "meta-2".to_string();
    registry.insert(meta2).unwrap();

    let mut meta3 = meta_skill_with_tags(vec!["python"], vec![]);
    meta3.id = "meta-3".to_string();
    registry.insert(meta3).unwrap();

    // Search for any of these tags (OR)
    let query = MetaSkillQuery {
        tags: vec!["cli".to_string(), "web".to_string()],
        ..Default::default()
    };
    let results = registry.search(&query);
    assert_eq!(results.len(), 2);
}

#[test]
fn registry_search_by_tech_stack() {
    let mut registry = MetaSkillRegistry::new();

    let mut meta1 = meta_skill_with_tags(vec![], vec!["rust"]);
    meta1.id = "meta-1".to_string();
    registry.insert(meta1).unwrap();

    let mut meta2 = meta_skill_with_tags(vec![], vec!["python"]);
    meta2.id = "meta-2".to_string();
    registry.insert(meta2).unwrap();

    let query = MetaSkillQuery {
        tech_stack: Some("rust".to_string()),
        ..Default::default()
    };
    let results = registry.search(&query);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "meta-1");
}

#[test]
fn registry_search_combined_filters() {
    let mut registry = MetaSkillRegistry::new();

    let mut meta1 = meta_skill_with_tags(vec!["guide"], vec!["rust"]);
    meta1.id = "rust-guide".to_string();
    meta1.name = "Rust Guide".to_string();
    registry.insert(meta1).unwrap();

    let mut meta2 = meta_skill_with_tags(vec!["guide"], vec!["python"]);
    meta2.id = "python-guide".to_string();
    meta2.name = "Python Guide".to_string();
    registry.insert(meta2).unwrap();

    // Text + tech_stack
    let query = MetaSkillQuery {
        text: Some("guide".to_string()),
        tech_stack: Some("rust".to_string()),
        ..Default::default()
    };
    let results = registry.search(&query);
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].id, "rust-guide");
}

#[test]
fn registry_search_no_matches() {
    let mut registry = MetaSkillRegistry::new();
    registry.insert(valid_meta_skill()).unwrap();

    let query = MetaSkillQuery {
        text: Some("nonexistent".to_string()),
        ..Default::default()
    };
    let results = registry.search(&query);
    assert!(results.is_empty());
}

// ============================================================================
// ConditionContext Tests
// ============================================================================

#[test]
fn condition_context_tech_stack_match() {
    let ctx = ConditionContext {
        working_dir: Path::new("/tmp"),
        tech_stacks: &["rust".to_string(), "typescript".to_string()],
        loaded_slices: &HashSet::new(),
    };

    assert!(ctx.evaluate(&SliceCondition::TechStack {
        value: "rust".to_string()
    }));
}

#[test]
fn condition_context_tech_stack_no_match() {
    let ctx = ConditionContext {
        working_dir: Path::new("/tmp"),
        tech_stacks: &["rust".to_string()],
        loaded_slices: &HashSet::new(),
    };

    assert!(!ctx.evaluate(&SliceCondition::TechStack {
        value: "python".to_string()
    }));
}

#[test]
fn condition_context_tech_stack_case_insensitive() {
    let ctx = ConditionContext {
        working_dir: Path::new("/tmp"),
        tech_stacks: &["Rust".to_string()],
        loaded_slices: &HashSet::new(),
    };

    assert!(ctx.evaluate(&SliceCondition::TechStack {
        value: "RUST".to_string()
    }));
    assert!(ctx.evaluate(&SliceCondition::TechStack {
        value: "rust".to_string()
    }));
}

#[test]
fn condition_context_env_var_exists() {
    let ctx = ConditionContext {
        working_dir: Path::new("/tmp"),
        tech_stacks: &[],
        loaded_slices: &HashSet::new(),
    };

    // PATH is almost always set
    assert!(ctx.evaluate(&SliceCondition::EnvVar {
        value: "PATH".to_string()
    }));
}

#[test]
fn condition_context_env_var_not_exists() {
    let ctx = ConditionContext {
        working_dir: Path::new("/tmp"),
        tech_stacks: &[],
        loaded_slices: &HashSet::new(),
    };

    assert!(!ctx.evaluate(&SliceCondition::EnvVar {
        value: "MS_NONEXISTENT_VAR_XYZ_12345".to_string()
    }));
}

#[test]
fn condition_context_depends_on_loaded() {
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
}

#[test]
fn condition_context_depends_on_not_loaded() {
    let ctx = ConditionContext {
        working_dir: Path::new("/tmp"),
        tech_stacks: &[],
        loaded_slices: &HashSet::new(),
    };

    assert!(!ctx.evaluate(&SliceCondition::DependsOn {
        skill_id: "skill-a".to_string(),
        slice_id: "slice-1".to_string(),
    }));
}

#[test]
fn condition_context_file_exists_blocks_absolute_path() {
    let ctx = ConditionContext {
        working_dir: Path::new("/tmp"),
        tech_stacks: &[],
        loaded_slices: &HashSet::new(),
    };

    // Should return false for absolute paths (security)
    assert!(!ctx.evaluate(&SliceCondition::FileExists {
        value: "/etc/passwd".to_string()
    }));
}

#[test]
fn condition_context_file_exists_blocks_parent_traversal() {
    let ctx = ConditionContext {
        working_dir: Path::new("/tmp/project"),
        tech_stacks: &[],
        loaded_slices: &HashSet::new(),
    };

    // Should return false for path traversal attempts
    assert!(!ctx.evaluate(&SliceCondition::FileExists {
        value: "../outside".to_string()
    }));
    assert!(!ctx.evaluate(&SliceCondition::FileExists {
        value: "subdir/../../outside".to_string()
    }));
}

#[test]
fn condition_context_evaluate_all_empty() {
    let ctx = ConditionContext {
        working_dir: Path::new("/tmp"),
        tech_stacks: &[],
        loaded_slices: &HashSet::new(),
    };

    // Empty conditions should return true
    assert!(ctx.evaluate_all(&[]));
}

#[test]
fn condition_context_evaluate_all_single_true() {
    let ctx = ConditionContext {
        working_dir: Path::new("/tmp"),
        tech_stacks: &["rust".to_string()],
        loaded_slices: &HashSet::new(),
    };

    assert!(ctx.evaluate_all(&[SliceCondition::TechStack {
        value: "rust".to_string()
    }]));
}

#[test]
fn condition_context_evaluate_all_single_false() {
    let ctx = ConditionContext {
        working_dir: Path::new("/tmp"),
        tech_stacks: &["rust".to_string()],
        loaded_slices: &HashSet::new(),
    };

    assert!(!ctx.evaluate_all(&[SliceCondition::TechStack {
        value: "python".to_string()
    }]));
}

#[test]
fn condition_context_evaluate_all_multiple_all_true() {
    let mut loaded = HashSet::new();
    loaded.insert(("skill-a".to_string(), "slice-1".to_string()));

    let ctx = ConditionContext {
        working_dir: Path::new("/tmp"),
        tech_stacks: &["rust".to_string()],
        loaded_slices: &loaded,
    };

    assert!(ctx.evaluate_all(&[
        SliceCondition::TechStack {
            value: "rust".to_string()
        },
        SliceCondition::DependsOn {
            skill_id: "skill-a".to_string(),
            slice_id: "slice-1".to_string(),
        }
    ]));
}

#[test]
fn condition_context_evaluate_all_one_false() {
    let ctx = ConditionContext {
        working_dir: Path::new("/tmp"),
        tech_stacks: &["rust".to_string()],
        loaded_slices: &HashSet::new(),
    };

    // One true, one false -> should return false (AND logic)
    assert!(!ctx.evaluate_all(&[
        SliceCondition::TechStack {
            value: "rust".to_string()
        },
        SliceCondition::TechStack {
            value: "python".to_string()
        }
    ]));
}

// ============================================================================
// MetaSkillDoc Tests
// ============================================================================

#[test]
fn meta_skill_doc_into_meta_skill() {
    let doc = MetaSkillDoc {
        meta_skill: MetaSkillHeader {
            id: "doc-test".to_string(),
            name: "Doc Test".to_string(),
            description: "Testing doc conversion".to_string(),
            pin_strategy: PinStrategy::LatestCompatible,
            metadata: MetaSkillMetadata::default(),
            min_context_tokens: 0,
            recommended_context_tokens: 0,
        },
        slices: vec![valid_slice_ref()],
    };

    let meta = doc.into_meta_skill().unwrap();
    assert_eq!(meta.id, "doc-test");
    assert_eq!(meta.name, "Doc Test");
}

#[test]
fn meta_skill_doc_validates_on_convert() {
    let doc = MetaSkillDoc {
        meta_skill: MetaSkillHeader {
            id: "".to_string(), // Invalid
            name: "Test".to_string(),
            description: "Test".to_string(),
            pin_strategy: PinStrategy::LatestCompatible,
            metadata: MetaSkillMetadata::default(),
            min_context_tokens: 0,
            recommended_context_tokens: 0,
        },
        slices: vec![valid_slice_ref()],
    };

    let result = doc.into_meta_skill();
    assert!(result.is_err());
}

// ============================================================================
// Serialization Round-Trip Tests
// ============================================================================

#[test]
fn meta_skill_json_roundtrip() {
    let meta = valid_meta_skill();
    let json = serde_json::to_string(&meta).unwrap();
    let parsed: MetaSkill = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.id, meta.id);
    assert_eq!(parsed.name, meta.name);
}

#[test]
fn meta_skill_with_all_fields_json_roundtrip() {
    let mut meta = valid_meta_skill();
    meta.metadata = MetaSkillMetadata {
        author: Some("author@example.com".to_string()),
        version: "1.0.0".to_string(),
        tags: vec!["tag1".to_string(), "tag2".to_string()],
        tech_stacks: vec!["rust".to_string()],
        updated_at: Some(chrono::Utc::now()),
    };
    meta.min_context_tokens = 100;
    meta.recommended_context_tokens = 500;
    meta.pin_strategy = PinStrategy::ExactVersion("1.2.3".to_string());

    let json = serde_json::to_string(&meta).unwrap();
    let parsed: MetaSkill = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.metadata.author, meta.metadata.author);
    assert_eq!(parsed.metadata.tags, meta.metadata.tags);
    assert_eq!(parsed.min_context_tokens, 100);
}

#[test]
fn slice_ref_with_conditions_json_roundtrip() {
    let slice = MetaSkillSliceRef {
        skill_id: "test".to_string(),
        slice_ids: vec!["a".to_string(), "b".to_string()],
        level: Some(MetaDisclosureLevel::Extended),
        priority: 10,
        required: true,
        conditions: vec![
            SliceCondition::TechStack {
                value: "rust".to_string(),
            },
            SliceCondition::DependsOn {
                skill_id: "dep".to_string(),
                slice_id: "slice".to_string(),
            },
        ],
    };

    let json = serde_json::to_string(&slice).unwrap();
    let parsed: MetaSkillSliceRef = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.conditions.len(), 2);
    assert!(parsed.required);
    assert_eq!(parsed.level, Some(MetaDisclosureLevel::Extended));
}
