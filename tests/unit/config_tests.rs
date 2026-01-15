use std::fs;
use std::path::PathBuf;

use ms::config::Config;
use ms::test_utils::{TestCase, run_table_tests};

fn fixture_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

#[test]
fn config_disclosure_search_from_fixture() -> Result<(), String> {
    let cases = vec![TestCase {
        name: "default",
        input: "tests/fixtures/configs/default.toml",
        expected: (
            "minimal".to_string(),
            500u32,
            false,
            120u64,
            "hash".to_string(),
            0.4f32,
            0.6f32,
        ),
        should_panic: false,
    }];

    run_table_tests(cases, |relative_path| {
        let path = fixture_path(relative_path);
        let content = fs::read_to_string(&path).expect("read fixture");
        let config: Config = toml::from_str(&content).expect("parse config");
        (
            config.disclosure.default_level,
            config.disclosure.token_budget,
            config.disclosure.auto_suggest,
            config.disclosure.cooldown_seconds,
            config.search.embedding_backend,
            config.search.bm25_weight,
            config.search.semantic_weight,
        )
    })?;
    Ok(())
}

#[test]
fn config_paths_and_layers_from_fixture() -> Result<(), String> {
    let cases = vec![TestCase {
        name: "custom",
        input: "tests/fixtures/configs/custom.toml",
        expected: (
            vec!["/tmp/skills".to_string()],
            vec![".ms/skills".to_string()],
            vec!["/tmp/community".to_string()],
            vec!["/tmp/local".to_string()],
            vec!["project".to_string(), "global".to_string()],
            false,
            true,
            Some("/tmp/cass".to_string()),
            "*.ndjson".to_string(),
            "text".to_string(),
            false,
        ),
        should_panic: false,
    }];

    run_table_tests(cases, |relative_path| {
        let path = fixture_path(relative_path);
        let content = fs::read_to_string(&path).expect("read fixture");
        let config: Config = toml::from_str(&content).expect("parse config");
        (
            config.skill_paths.global,
            config.skill_paths.project,
            config.skill_paths.community,
            config.skill_paths.local,
            config.layers.priority,
            config.layers.auto_detect,
            config.layers.project_overrides,
            config.cass.cass_path,
            config.cass.session_pattern,
            config.robot.format,
            config.robot.include_metadata,
        )
    })?;
    Ok(())
}
