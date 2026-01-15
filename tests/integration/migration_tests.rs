use std::fs;

use ms::core::SkillSpec;

use crate::fixture::TestFixture;

#[test]
fn migrate_sets_missing_format_version() {
    let fixture = TestFixture::with_sample_skills("migrate_sets_missing_format_version");
    let skill_id = "rust-error-handling";
    let spec_path = fixture
        .ms_root
        .join("archive/skills/by-id")
        .join(skill_id)
        .join("skill.spec.json");

    let mut json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&spec_path).unwrap()).unwrap();
    if let Some(obj) = json.as_object_mut() {
        obj.insert(
            "format_version".to_string(),
            serde_json::Value::String("".to_string()),
        );
    }
    fs::write(&spec_path, serde_json::to_string_pretty(&json).unwrap()).unwrap();

    let output = fixture.run_ms(&["migrate", skill_id]);
    assert!(output.success, "migrate failed: {}", output.stderr);

    let updated: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&spec_path).unwrap()).unwrap();
    assert_eq!(
        updated["format_version"],
        serde_json::Value::String(SkillSpec::FORMAT_VERSION.to_string())
    );
}
