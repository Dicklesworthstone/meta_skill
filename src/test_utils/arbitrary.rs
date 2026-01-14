use std::path::PathBuf;

use proptest::prelude::*;

use crate::config::{
    CacheConfig, CassConfig, Config, DisclosureConfig, LayersConfig, RobotConfig, SafetyConfig,
    SearchConfig, SecurityConfig, SkillPathsConfig, UpdateConfig,
};
use crate::core::skill::{BlockType, SkillBlock, SkillMetadata, SkillSection, SkillSpec};
use crate::security::{AcipConfig, TrustBoundaryConfig, TrustLevel};

fn arb_block_type() -> impl Strategy<Value = BlockType> {
    prop_oneof![
        Just(BlockType::Text),
        Just(BlockType::Code),
        Just(BlockType::Rule),
        Just(BlockType::Pitfall),
        Just(BlockType::Command),
        Just(BlockType::Checklist),
    ]
}

fn arb_skill_block() -> impl Strategy<Value = SkillBlock> {
    (
        "[a-z][a-z0-9_-]{2,24}",
        arb_block_type(),
        ".{0,200}",
    )
        .prop_map(|(id, block_type, content)| SkillBlock {
            id,
            block_type,
            content,
        })
}

fn arb_skill_section() -> impl Strategy<Value = SkillSection> {
    (
        "[a-z][a-z0-9_-]{2,24}",
        ".{1,40}",
        prop::collection::vec(arb_skill_block(), 0..5),
    )
        .prop_map(|(id, title, blocks)| SkillSection { id, title, blocks })
}

fn arb_skill_metadata() -> impl Strategy<Value = SkillMetadata> {
    (
        "[a-z][a-z0-9_]{2,24}",
        ".{1,40}",
        "[0-9]+\\.[0-9]+\\.[0-9]+",
        ".{0,120}",
        prop::collection::vec("[a-z]{2,12}", 0..5),
        prop::collection::vec("[a-z]{2,12}", 0..3),
        prop::collection::vec("[a-z]{2,12}", 0..3),
        prop::collection::vec("[a-z]{2,12}", 0..3),
        prop::option::of(".{0,40}"),
        prop::option::of(".{0,20}"),
    )
        .prop_map(
            |(
                id,
                name,
                version,
                description,
                tags,
                requires,
                provides,
                platforms,
                author,
                license,
            )| SkillMetadata {
                id,
                name,
                version,
                description,
                tags,
                requires,
                provides,
                platforms,
                author,
                license,
            },
        )
}

/// Generate arbitrary SkillSpec.
pub fn arb_skill_spec() -> impl Strategy<Value = SkillSpec> {
    (
        arb_skill_metadata(),
        prop::collection::vec(arb_skill_section(), 0..4),
    )
        .prop_map(|(metadata, sections)| SkillSpec { metadata, sections })
}

fn arb_skill_paths() -> impl Strategy<Value = SkillPathsConfig> {
    let list = prop::collection::vec("[a-zA-Z0-9_./-]{1,20}", 0..4);
    (list.clone(), list.clone(), list.clone(), list).prop_map(
        |(global, project, community, local)| SkillPathsConfig {
            global,
            project,
            community,
            local,
        },
    )
}

fn arb_layers() -> impl Strategy<Value = LayersConfig> {
    let layer = prop_oneof![
        Just("project".to_string()),
        Just("global".to_string()),
        Just("community".to_string()),
        Just("local".to_string()),
    ];
    (
        prop::collection::vec(layer, 0..4),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(|(priority, auto_detect, project_overrides)| LayersConfig {
            priority,
            auto_detect,
            project_overrides,
        })
}

fn arb_disclosure() -> impl Strategy<Value = DisclosureConfig> {
    let level = prop_oneof![
        Just("minimal".to_string()),
        Just("moderate".to_string()),
        Just("standard".to_string()),
        Just("full".to_string()),
    ];
    (
        level,
        100u32..2000u32,
        any::<bool>(),
        0u64..10_000u64,
    )
        .prop_map(|(default_level, token_budget, auto_suggest, cooldown_seconds)| {
            DisclosureConfig {
                default_level,
                token_budget,
                auto_suggest,
                cooldown_seconds,
            }
        })
}

fn arb_search() -> impl Strategy<Value = SearchConfig> {
    let backend = prop_oneof![Just("hash".to_string()), Just("none".to_string())];
    (
        any::<bool>(),
        backend,
        16u32..1024u32,
        0.0f32..1.0f32,
        0.0f32..1.0f32,
    )
        .prop_map(
            |(use_embeddings, embedding_backend, embedding_dims, bm25_weight, semantic_weight)| {
                SearchConfig {
                    use_embeddings,
                    embedding_backend,
                    embedding_dims,
                    bm25_weight,
                    semantic_weight,
                }
            },
        )
}

fn arb_cass() -> impl Strategy<Value = CassConfig> {
    (
        any::<bool>(),
        prop::option::of("[a-zA-Z0-9_./-]{1,24}"),
        prop_oneof![Just("*.jsonl".to_string()), Just("*.ndjson".to_string())],
    )
        .prop_map(|(auto_detect, cass_path, session_pattern)| CassConfig {
            auto_detect,
            cass_path,
            session_pattern,
        })
}

fn arb_cache() -> impl Strategy<Value = CacheConfig> {
    (
        any::<bool>(),
        1u32..500u32,
        0u64..86_400u64,
    )
        .prop_map(|(enabled, max_size_mb, ttl_seconds)| CacheConfig {
            enabled,
            max_size_mb,
            ttl_seconds,
        })
}

fn arb_update() -> impl Strategy<Value = UpdateConfig> {
    let channel = prop_oneof![Just("stable".to_string()), Just("beta".to_string())];
    (
        any::<bool>(),
        1u32..168u32,
        channel,
    )
        .prop_map(|(auto_check, check_interval_hours, channel)| UpdateConfig {
            auto_check,
            check_interval_hours,
            channel,
        })
}

fn arb_robot() -> impl Strategy<Value = RobotConfig> {
    let format = prop_oneof![Just("json".to_string()), Just("text".to_string())];
    (format, any::<bool>()).prop_map(|(format, include_metadata)| RobotConfig {
        format,
        include_metadata,
    })
}

fn arb_trust_boundary() -> impl Strategy<Value = TrustBoundaryConfig> {
    let level = prop_oneof![
        Just(TrustLevel::Trusted),
        Just(TrustLevel::VerifyRequired),
        Just(TrustLevel::Untrusted),
    ];
    (
        level.clone(),
        level.clone(),
        level.clone(),
        level,
    )
        .prop_map(
            |(user_messages, assistant_messages, tool_outputs, file_contents)| {
                TrustBoundaryConfig {
                    user_messages,
                    assistant_messages,
                    tool_outputs,
                    file_contents,
                }
            },
        )
}

fn arb_acip() -> impl Strategy<Value = AcipConfig> {
    (
        any::<bool>(),
        Just("1.3".to_string()),
        "[a-zA-Z0-9_./-]{1,24}",
        any::<bool>(),
        arb_trust_boundary(),
    )
        .prop_map(|(enabled, version, prompt_path, audit_mode, trust)| AcipConfig {
            enabled,
            version,
            prompt_path: PathBuf::from(prompt_path),
            audit_mode,
            trust,
        })
}

fn arb_security() -> impl Strategy<Value = SecurityConfig> {
    arb_acip().prop_map(|acip| SecurityConfig { acip })
}

fn arb_safety() -> impl Strategy<Value = SafetyConfig> {
    (any::<bool>(),).prop_map(|(require_verbatim_approval,)| SafetyConfig {
        dcg_bin: PathBuf::new(),
        dcg_packs: vec![],
        dcg_explain_format: String::new(),
        require_verbatim_approval,
    })
}

/// Generate arbitrary Config.
pub fn arb_config() -> impl Strategy<Value = Config> {
    (
        arb_skill_paths(),
        arb_layers(),
        arb_disclosure(),
        arb_search(),
        arb_cass(),
        arb_cache(),
        arb_update(),
        arb_robot(),
        arb_security(),
        arb_safety(),
    )
        .prop_map(
            |(skill_paths, layers, disclosure, search, cass, cache, update, robot, security, safety)| {
                Config {
                    skill_paths,
                    layers,
                    disclosure,
                    search,
                    cass,
                    cm: crate::config::CmConfig::default(),
                    cache,
                    update,
                    robot,
                    security,
                    safety,
                }
            },
        )
}

/// Generate arbitrary search query.
pub fn arb_search_query() -> impl Strategy<Value = String> {
    prop::string::string_regex("[a-zA-Z0-9 ]{1,100}").unwrap()
}
