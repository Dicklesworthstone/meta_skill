use proptest::prelude::*;

use ms::config::Config;
use ms::core::spec_lens::parse_markdown;
use ms::core::validation::validate;
use ms::core::SkillSpec;
use ms::search::SearchFilters;
use ms::test_utils::arbitrary::{arb_config, arb_skill_spec};

proptest! {
    // =========================================================================
    // Parser Safety Tests
    // =========================================================================

    #[test]
    fn test_parse_markdown_never_panics(input in ".*") {
        let _ = parse_markdown(&input);
    }

    #[test]
    fn test_parse_markdown_arbitrary_bytes(bytes in prop::collection::vec(any::<u8>(), 0..1000)) {
        let input = String::from_utf8_lossy(&bytes);
        let _ = parse_markdown(&input);
    }

    // =========================================================================
    // Search Safety Tests
    // =========================================================================

    #[test]
    fn test_search_tags_never_panics(input in ".*") {
        let _ = SearchFilters::parse_tags(&input);
    }

    #[test]
    fn test_search_tags_arbitrary_bytes(bytes in prop::collection::vec(any::<u8>(), 0..200)) {
        let input = String::from_utf8_lossy(&bytes);
        let _ = SearchFilters::parse_tags(&input);
    }

    // =========================================================================
    // Validation Safety Tests
    // =========================================================================

    #[test]
    fn test_validate_spec_never_panics(spec in arb_skill_spec()) {
        let _ = validate(&spec);
    }

    #[test]
    fn test_validate_empty_spec_never_panics() {
        let spec = SkillSpec::new("", "");
        let _ = validate(&spec);
    }

    // =========================================================================
    // Serialization Safety Tests
    // =========================================================================

    #[test]
    fn test_skill_spec_json_serialize_never_panics(spec in arb_skill_spec()) {
        let _ = serde_json::to_string(&spec);
    }

    #[test]
    fn test_skill_spec_json_deserialize_never_panics(input in ".*") {
        let _: Result<SkillSpec, _> = serde_json::from_str(&input);
    }

    #[test]
    fn test_config_toml_serialize_never_panics(config in arb_config()) {
        let _ = toml::to_string(&config);
    }

    #[test]
    fn test_config_toml_deserialize_never_panics(input in ".*") {
        let _: Result<Config, _> = toml::from_str(&input);
    }

    // =========================================================================
    // Config Default Safety Tests
    // =========================================================================

    #[test]
    fn test_config_default_never_panics(_seed in any::<u64>()) {
        let _ = Config::default();
    }

    // =========================================================================
    // SkillSpec Construction Safety Tests
    // =========================================================================

    #[test]
    fn test_skill_spec_new_never_panics(
        id in ".*",
        name in ".*"
    ) {
        let _ = SkillSpec::new(id, name);
    }
}
