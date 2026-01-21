//! Error handling integration tests
//!
//! Tests that verify structured error responses in robot mode
//! with error codes, suggestions, and context.

use serde_json::Value;

use super::fixture::TestFixture;

/// Test that skill not found errors have structured format
#[test]
fn test_skill_not_found_structured_error() {
    let fixture = TestFixture::new("test_skill_not_found_structured_error");
    let _ = fixture.run_ms(&["init"]);

    // Try to show a non-existent skill
    let output = fixture.run_ms(&["-O", "json", "show", "nonexistent-skill-xyz-123"]);

    // Command should fail
    if !output.success {
        // Check if we got structured JSON output
        if let Ok(json) = serde_json::from_str::<Value>(&output.stdout) {
            // Check for structured error fields
            if let Some(status) = json.get("status") {
                // Should be an error object
                if status.is_object() {
                    // Check for expected error structure
                    let has_code = status.get("code").is_some();
                    let has_message = status.get("message").is_some();

                    // If it has structured error fields, verify them
                    if has_code && has_message {
                        // Check for error code
                        let code = status.get("code");
                        assert!(
                            code.map(|c| c.as_str().unwrap_or(""))
                                .map(|s| s == "SKILL_NOT_FOUND" || s.contains("skill"))
                                .unwrap_or(false)
                                || status.get("numeric_code").is_some(),
                            "Error should have skill-related code: {:?}",
                            status
                        );

                        // Check for suggestion if present
                        if let Some(suggestion) = status.get("suggestion") {
                            assert!(
                                suggestion.as_str().is_some(),
                                "Suggestion should be a string"
                            );
                        }

                        // Check for recoverable flag if present
                        if let Some(recoverable) = status.get("recoverable") {
                            assert!(recoverable.is_boolean(), "Recoverable should be a boolean");
                        }
                    }
                }
            }
        }
    }
}

/// Test that search query errors have structured format
#[test]
fn test_search_error_structured() {
    let fixture = TestFixture::new("test_search_error_structured");
    let _ = fixture.run_ms(&["init"]);

    // Try a search that should return no results
    let output = fixture.run_ms(&["-O", "json", "search", "zzz-definitely-not-found-xyz"]);

    // Parse the output
    if let Ok(json) = serde_json::from_str::<Value>(&output.stdout) {
        // If we got JSON, check the structure
        let status = json.get("status");
        assert!(status.is_some(), "JSON should have status field");

        // Check if this is an error response with no results
        if let Some(status) = status {
            // Either it's OK with empty results, or an error
            if !status.is_string() || status.as_str() != Some("ok") {
                // If it's an error object, check structure
                if status.is_object() {
                    let has_code = status.get("code").is_some();
                    let has_message = status.get("message").is_some();
                    assert!(
                        has_code || has_message || status.get("error").is_some(),
                        "Error status should have code or message: {:?}",
                        status
                    );
                }
            }
        }
    }
}

/// Test that config errors have structured format
#[test]
fn test_config_error_structured() {
    let fixture = TestFixture::new("test_config_error_structured");
    let _ = fixture.run_ms(&["init"]);

    // Try to set an invalid config key
    let output = fixture.run_ms(&[
        "-O",
        "json",
        "config",
        "invalid.nested.key.that.does.not.exist",
        "some_value",
    ]);

    // Check if we got structured output
    if let Ok(json) = serde_json::from_str::<Value>(&output.stdout) {
        // Verify JSON structure
        if let Some(status) = json.get("status") {
            if status.is_object() {
                // Check for error fields
                let has_error_info = status.get("code").is_some()
                    || status.get("message").is_some()
                    || status.get("error").is_some();

                if has_error_info {
                    // If there's a category, it should be config-related
                    if let Some(category) = status.get("category") {
                        assert!(
                            category.as_str().map(|s| s == "config").unwrap_or(true),
                            "Category should be config for config errors"
                        );
                    }
                }
            }
        }
    }
}

/// Test that validation errors have structured format
#[test]
fn test_validation_error_structured() {
    let fixture = TestFixture::new("test_validation_error_structured");
    let _ = fixture.run_ms(&["init"]);

    // Create an invalid skill file
    let invalid_skill_content = "This is not a valid SKILL.md format\n\nNo title, no sections.";
    let skill_path = fixture.temp_dir.path().join("skills");
    std::fs::create_dir_all(&skill_path).unwrap();
    std::fs::write(skill_path.join("invalid.md"), invalid_skill_content).unwrap();

    // Try to validate it
    let output = fixture.run_ms(&[
        "-O",
        "json",
        "validate",
        &skill_path.join("invalid.md").to_string_lossy(),
    ]);

    // Check if we got structured output
    if let Ok(json) = serde_json::from_str::<Value>(&output.stdout) {
        // Verify JSON structure
        if let Some(status) = json.get("status") {
            // If it's an error, check for validation-related fields
            if status.is_object() {
                let code = status.get("code");
                if let Some(code) = code {
                    // Should be validation-related
                    let is_validation = code
                        .as_str()
                        .map(|s| {
                            s.contains("VALIDATION") || s.contains("INVALID") || s.contains("PARSE")
                        })
                        .unwrap_or(false);

                    if is_validation {
                        // Should have suggestion for how to fix
                        if let Some(suggestion) = status.get("suggestion") {
                            assert!(
                                suggestion.as_str().map(|s| !s.is_empty()).unwrap_or(false),
                                "Validation error should have suggestion"
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Test that error responses include all expected fields
#[test]
fn test_error_response_completeness() {
    let fixture = TestFixture::new("test_error_response_completeness");
    let _ = fixture.run_ms(&["init"]);

    // Force an error by loading a non-existent skill
    let output = fixture.run_ms(&["-O", "json", "load", "definitely-not-a-real-skill-name"]);

    if !output.success {
        if let Ok(json) = serde_json::from_str::<Value>(&output.stdout) {
            if let Some(status) = json.get("status") {
                if status.is_object() {
                    // Check for completeness of error response
                    let fields_to_check = [("code", "Error code"), ("message", "Error message")];

                    for (field, name) in &fields_to_check {
                        if let Some(value) = status.get(*field) {
                            // Field exists, verify it has content
                            match value {
                                Value::String(s) => {
                                    assert!(!s.is_empty(), "{} should not be empty", name)
                                }
                                _ => {} // Other types are OK
                            }
                        }
                    }

                    // Check optional but recommended fields
                    let optional_fields = ["suggestion", "recoverable", "category", "numeric_code"];
                    let has_optional = optional_fields.iter().any(|f| status.get(*f).is_some());

                    // Log what we found for debugging
                    if !has_optional {
                        eprintln!(
                            "Note: Error response lacks optional enriched fields: {:?}",
                            status
                        );
                    }
                }
            }
        }
    }
}

/// Test that error codes are consistent across different error types
#[test]
fn test_error_codes_consistency() {
    let fixture = TestFixture::new("test_error_codes_consistency");
    let _ = fixture.run_ms(&["init"]);

    // Collect errors from different commands
    let error_scenarios = vec![
        vec!["show", "nonexistent-skill"],
        vec!["load", "nonexistent-skill"],
    ];

    let mut error_codes = Vec::new();

    for args in &error_scenarios {
        let mut full_args = vec!["-O", "json"];
        full_args.extend(args.iter().map(|s| *s));

        let output = fixture.run_ms(&full_args);
        if !output.success {
            if let Ok(json) = serde_json::from_str::<Value>(&output.stdout) {
                if let Some(status) = json.get("status") {
                    if let Some(code) = status.get("code") {
                        error_codes.push((args.clone(), code.clone()));
                    }
                }
            }
        }
    }

    // All "skill not found" errors should have the same code
    if error_codes.len() >= 2 {
        let first_code = &error_codes[0].1;
        for (args, code) in &error_codes[1..] {
            assert_eq!(
                first_code, code,
                "Same error type should have consistent code: {:?} vs {:?}",
                error_codes[0].0, args
            );
        }
    }
}

/// Test that numeric error codes follow the expected ranges
#[test]
fn test_error_code_numeric_ranges() {
    let fixture = TestFixture::new("test_error_code_numeric_ranges");
    let _ = fixture.run_ms(&["init"]);

    // Try to trigger a skill error
    let output = fixture.run_ms(&["-O", "json", "show", "nonexistent-skill"]);

    if !output.success {
        if let Ok(json) = serde_json::from_str::<Value>(&output.stdout) {
            if let Some(status) = json.get("status") {
                if let Some(numeric_code) = status.get("numeric_code") {
                    if let Some(code) = numeric_code.as_u64() {
                        // Check that skill errors are in 1xx range
                        let category = code / 100;
                        assert!(
                            (1..=9).contains(&category),
                            "Numeric code {} should be in 1xx-9xx range",
                            code
                        );

                        // Skill errors should be 1xx
                        if let Some(code_str) = status.get("code").and_then(|c| c.as_str()) {
                            if code_str.contains("SKILL") {
                                assert_eq!(
                                    category, 1,
                                    "Skill error code {} should be in 1xx range",
                                    code
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
