use insta::{assert_json_snapshot, assert_snapshot};
use regex::Regex;
use serde_json::Value;

use super::fixture::TestFixture;

#[test]
fn test_list_output_human() {
    let fixture = TestFixture::with_sample_skills("snapshot_list_human");
    let output = fixture.run_ms(&["list"]);
    assert!(output.success, "list failed: {}", output.stderr);

    let sanitized = sanitize_human(&output.stdout);
    assert_snapshot!("list_human", sanitized);
}

#[test]
fn test_list_output_robot_json() {
    let fixture = TestFixture::with_sample_skills("snapshot_list_robot");
    let output = fixture.run_ms(&["--robot", "list"]);
    assert!(output.success, "list --robot failed: {}", output.stderr);

    let mut json = output.json();
    sanitize_json(&mut json);
    assert_json_snapshot!("list_robot_json", json);
}

#[test]
fn test_search_output_human() {
    let fixture = TestFixture::with_sample_skills("snapshot_search_human");
    let output = fixture.run_ms(&["search", "rust"]);
    assert!(output.success, "search failed: {}", output.stderr);

    let sanitized = sanitize_human(&output.stdout);
    assert_snapshot!("search_human", sanitized);
}

#[test]
fn test_search_output_robot_json() {
    let fixture = TestFixture::with_sample_skills("snapshot_search_robot");
    let output = fixture.run_ms(&["--robot", "search", "rust"]);
    assert!(output.success, "search --robot failed: {}", output.stderr);

    let mut json = output.json();
    sanitize_json(&mut json);
    assert_json_snapshot!("search_robot_json", json);
}

#[test]
fn test_show_output_human() {
    let fixture = TestFixture::with_sample_skills("snapshot_show_human");
    let output = fixture.run_ms(&["show", "rust-error-handling"]);
    assert!(output.success, "show failed: {}", output.stderr);

    let sanitized = sanitize_human(&output.stdout);
    assert_snapshot!("show_human", sanitized);
}

#[test]
fn test_show_output_robot_json() {
    let fixture = TestFixture::with_sample_skills("snapshot_show_robot");
    let output = fixture.run_ms(&["--robot", "show", "rust-error-handling"]);
    assert!(output.success, "show --robot failed: {}", output.stderr);

    let mut json = output.json();
    sanitize_json(&mut json);
    assert_json_snapshot!("show_robot_json", json);
}

fn sanitize_human(input: &str) -> String {
    let re_iso = Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}Z?").unwrap();
    let re_space = Regex::new(r"\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}").unwrap();
    let re_date = Regex::new(r"\d{4}-\d{2}-\d{2}").unwrap();
    let re_tmp = Regex::new(r"/tmp/\.tmp[a-zA-Z0-9]+").unwrap();

    let mut out = re_iso.replace_all(input, "[TIMESTAMP]").to_string();
    out = re_space.replace_all(&out, "[TIMESTAMP]").to_string();
    out = re_date.replace_all(&out, "[DATE]").to_string();
    out = re_tmp.replace_all(&out, "/tmp/[TEMP_DIR]").to_string();
    out
}

fn sanitize_json(value: &mut Value) {
    match value {
        Value::Object(map) => {
            let keys_to_remove = [
                "timestamp",
                "modified_at",
                "indexed_at",
                "created_at",
                "updated_at",
                "started_at",
                "source_path",
            ];
            for key in keys_to_remove {
                map.remove(key);
            }
            for val in map.values_mut() {
                sanitize_json(val);
            }
        }
        Value::Array(values) => {
            for val in values {
                sanitize_json(val);
            }
        }
        _ => {}
    }
}
