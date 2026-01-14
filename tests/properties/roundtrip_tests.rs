use proptest::prelude::*;

use ms::config::Config;
use ms::core::skill::{BlockType, SkillBlock, SkillMetadata, SkillSection, SkillSpec};

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

fn arb_skill_spec() -> impl Strategy<Value = SkillSpec> {
    let block = (
        "[a-z][a-z0-9_\-]{2,16}",
        arb_block_type(),
        ".{0,120}",
    )
        .prop_map(|(id, block_type, content)| SkillBlock {
            id,
            block_type,
            content,
        });

    let section = (
        "[a-z][a-z0-9_\-]{2,16}",
        ".{1,32}",
        prop::collection::vec(block, 0..4),
    )
        .prop_map(|(id, title, blocks)| SkillSection { id, title, blocks });

    (
        "[a-z][a-z0-9_]{2,16}",
        ".{1,32}",
        "[0-9]+\\.[0-9]+\\.[0-9]+",
        ".{0,80}",
        prop::collection::vec("[a-z]{2,12}", 0..4),
        prop::collection::vec(section, 0..3),
    )
        .prop_map(|(id, name, version, description, tags, sections)| SkillSpec {
            metadata: SkillMetadata {
                id,
                name,
                version,
                description,
                tags,
                ..Default::default()
            },
            sections,
        })
}

fn arb_config() -> impl Strategy<Value = Config> {
    (
        prop_oneof![
            Just("minimal".to_string()),
            Just("moderate".to_string()),
            Just("standard".to_string()),
        ],
        100u32..2000u32,
        any::<bool>(),
        0u64..10_000u64,
        prop_oneof![Just("hash".to_string()), Just("none".to_string())],
        16u32..512u32,
        0.0f32..1.0f32,
        0.0f32..1.0f32,
    )
        .prop_map(
            |(
                default_level,
                token_budget,
                auto_suggest,
                cooldown_seconds,
                backend,
                dims,
                bm25_weight,
                semantic_weight,
            )| {
                let mut config = Config::default();
                config.disclosure.default_level = default_level;
                config.disclosure.token_budget = token_budget;
                config.disclosure.auto_suggest = auto_suggest;
                config.disclosure.cooldown_seconds = cooldown_seconds;
                config.search.embedding_backend = backend;
                config.search.embedding_dims = dims;
                config.search.bm25_weight = bm25_weight;
                config.search.semantic_weight = semantic_weight;
                config
            },
        )
}

proptest! {
    #[test]
    fn test_skill_spec_roundtrip(spec in arb_skill_spec()) {
        let serialized = serde_json::to_string(&spec).unwrap();
        let deserialized: SkillSpec = serde_json::from_str(&serialized).unwrap();
        let left: serde_json::Value = serde_json::from_str(&serialized).unwrap();
        let right = serde_json::to_value(&deserialized).unwrap();
        prop_assert_eq!(left, right);
    }

    #[test]
    fn test_config_roundtrip(config in arb_config()) {
        let toml_str = toml::to_string(&config).unwrap();
        let parsed: Config = toml::from_str(&toml_str).unwrap();
        let left: toml::Value = toml::from_str(&toml_str).unwrap();
        let right: toml::Value = toml::from_str(&toml::to_string(&parsed).unwrap()).unwrap();
        prop_assert_eq!(left, right);
    }
}
