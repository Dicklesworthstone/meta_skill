use insta::assert_yaml_snapshot;

use crate::snapshots::fixture::sample_skills;
use ms::core::disclosure::{DisclosureLevel, DisclosurePlan, disclose};
use ms::core::skill::SkillAssets;
use ms::core::spec_lens::parse_markdown;

fn build_spec() -> ms::core::SkillSpec {
    let skill = sample_skills::rust_error_handling();
    parse_markdown(&skill.content).expect("parse test skill")
}

#[test]
fn test_disclosure_minimal() {
    let spec = build_spec();
    let disclosed = disclose(
        &spec,
        &SkillAssets::default(),
        &DisclosurePlan::Level(DisclosureLevel::Minimal),
    );
    assert_yaml_snapshot!("disclosure_minimal", disclosed);
}

#[test]
fn test_disclosure_overview() {
    let spec = build_spec();
    let disclosed = disclose(
        &spec,
        &SkillAssets::default(),
        &DisclosurePlan::Level(DisclosureLevel::Overview),
    );
    assert_yaml_snapshot!("disclosure_overview", disclosed);
}

#[test]
fn test_disclosure_standard() {
    let spec = build_spec();
    let disclosed = disclose(
        &spec,
        &SkillAssets::default(),
        &DisclosurePlan::Level(DisclosureLevel::Standard),
    );
    assert_yaml_snapshot!("disclosure_standard", disclosed);
}

#[test]
fn test_disclosure_full() {
    let spec = build_spec();
    let disclosed = disclose(
        &spec,
        &SkillAssets::default(),
        &DisclosurePlan::Level(DisclosureLevel::Full),
    );
    assert_yaml_snapshot!("disclosure_full", disclosed);
}

#[test]
fn test_disclosure_complete() {
    let spec = build_spec();
    let disclosed = disclose(
        &spec,
        &SkillAssets::default(),
        &DisclosurePlan::Level(DisclosureLevel::Complete),
    );
    assert_yaml_snapshot!("disclosure_complete", disclosed);
}
