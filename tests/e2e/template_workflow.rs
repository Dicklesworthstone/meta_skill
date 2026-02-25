//! E2E Scenario: Template Workflow Integration Tests
//!
//! Covers template listing, preview, skill generation from templates,
//! variable substitution, validation, and duplicate handling.

use super::fixture::E2EFixture;
use ms::error::Result;

// =============================================================================
// HELPER: Setup fixture
// =============================================================================

fn setup_template_fixture(scenario: &str) -> Result<E2EFixture> {
    let mut fixture = E2EFixture::new(scenario);

    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    Ok(fixture)
}

// =============================================================================
// TEST: List built-in templates
// =============================================================================

#[test]
fn test_template_list() -> Result<()> {
    let mut fixture = setup_template_fixture("template_list")?;

    fixture.log_step("checkpoint:template:setup");
    fixture.checkpoint("template_setup");

    fixture.log_step("List available templates");
    let output = fixture.run_ms(&["--robot", "template", "list"]);
    fixture.assert_success(&output, "template list");

    let json = output.json();
    assert_eq!(
        json["status"].as_str().unwrap_or(""),
        "ok",
        "List should succeed"
    );

    let count = json["count"].as_u64().unwrap_or(0);
    assert!(count >= 3, "Should have at least 3 built-in templates, got {}", count);

    let templates = json["templates"].as_array().expect("templates array");
    println!("[VERIFY] Found {} templates", templates.len());

    for template in templates {
        let id = template["id"].as_str().unwrap_or("?");
        let name = template["name"].as_str().unwrap_or("?");
        let summary = template["summary"].as_str().unwrap_or("?");
        println!(
            "[VERIFY] Template: id={}, name={}, summary={}",
            id, name, summary
        );

        // Each template should have required fields
        assert!(!id.is_empty(), "Template should have an id");
        assert!(!name.is_empty(), "Template should have a name");
        assert!(!summary.is_empty(), "Template should have a summary");
    }

    // Verify known built-in templates exist
    let template_ids: Vec<&str> = templates
        .iter()
        .filter_map(|t| t["id"].as_str())
        .collect();
    assert!(
        template_ids.contains(&"debugging"),
        "Should contain 'debugging' template"
    );

    fixture.log_step("checkpoint:template:teardown");
    fixture.checkpoint("template_list_done");

    Ok(())
}

// =============================================================================
// TEST: Show template details
// =============================================================================

#[test]
fn test_template_show() -> Result<()> {
    let mut fixture = setup_template_fixture("template_show")?;

    fixture.log_step("checkpoint:template:setup");

    fixture.log_step("checkpoint:template:select");
    fixture.log_step("Show debugging template in JSON format");
    let output = fixture.run_ms(&["--robot", "template", "show", "debugging", "--format", "json"]);
    fixture.assert_success(&output, "template show json");

    let json = output.json();
    assert_eq!(
        json["id"].as_str().unwrap_or(""),
        "debugging",
        "Template id should be debugging"
    );
    assert!(
        json["name"].as_str().is_some(),
        "Template should have a name"
    );
    assert!(
        json["body"].as_str().is_some(),
        "Template should have a body"
    );

    let body = json["body"].as_str().unwrap_or("");
    assert!(!body.is_empty(), "Template body should not be empty");
    println!(
        "[VERIFY] Template body length: {} chars",
        body.len()
    );

    // Body should contain template placeholders
    assert!(
        body.contains("{{id}}") || body.contains("{{name}}") || body.contains("{{description}}"),
        "Template body should contain variable placeholders"
    );

    fixture.log_step("checkpoint:template:teardown");
    fixture.checkpoint("template_show_done");

    Ok(())
}

// =============================================================================
// TEST: Generate skill from template
// =============================================================================

#[test]
fn test_template_generate() -> Result<()> {
    let mut fixture = setup_template_fixture("template_generate")?;

    fixture.log_step("checkpoint:template:setup");

    fixture.log_step("checkpoint:template:select");
    fixture.log_step("Apply debugging template to create a skill");
    let output = fixture.run_ms(&[
        "--robot",
        "template",
        "apply",
        "debugging",
        "--id",
        "debug-rust-builds",
        "--name",
        "Debug Rust Builds",
        "--description",
        "Diagnose Rust build failures and compiler errors.",
        "--tag",
        "rust,build,debugging",
    ]);
    fixture.assert_success(&output, "template apply");

    fixture.log_step("checkpoint:template:generate");

    let json = output.json();
    assert_eq!(
        json["status"].as_str().unwrap_or(""),
        "ok",
        "Apply should succeed"
    );
    assert_eq!(
        json["skill_id"].as_str().unwrap_or(""),
        "debug-rust-builds",
        "Skill id should match"
    );
    assert_eq!(
        json["template"].as_str().unwrap_or(""),
        "debugging",
        "Template id should be debugging"
    );

    println!(
        "[VERIFY] Created skill: {}",
        json["skill_id"].as_str().unwrap_or("?")
    );

    // Verify the skill was actually created
    fixture.log_step("checkpoint:template:verify");
    let output = fixture.run_ms(&["--robot", "show", "debug-rust-builds"]);
    fixture.assert_success(&output, "show generated skill");

    let json = output.json();
    let skill = &json["skill"];
    assert_eq!(
        skill["name"].as_str().unwrap_or(""),
        "Debug Rust Builds",
        "Skill name should match"
    );

    fixture.log_step("checkpoint:template:teardown");
    fixture.checkpoint("template_generate_done");

    Ok(())
}

// =============================================================================
// TEST: Variable substitution
// =============================================================================

#[test]
fn test_template_variables() -> Result<()> {
    let mut fixture = setup_template_fixture("template_variables")?;

    fixture.log_step("checkpoint:template:setup");

    // Apply template with specific variables
    fixture.log_step("checkpoint:template:substitute");
    let output = fixture.run_ms(&[
        "--robot",
        "template",
        "apply",
        "refactor",
        "--id",
        "refactor-auth-module",
        "--name",
        "Refactor Auth Module",
        "--description",
        "Step-by-step guide for refactoring authentication modules safely.",
        "--tag",
        "refactoring,auth,security",
    ]);
    fixture.assert_success(&output, "template apply with variables");

    fixture.log_step("checkpoint:template:generate");

    let json = output.json();
    assert_eq!(json["status"].as_str().unwrap_or(""), "ok");
    assert_eq!(
        json["skill_id"].as_str().unwrap_or(""),
        "refactor-auth-module"
    );

    // Verify the generated skill has the substituted values
    fixture.log_step("Verify variable substitution in generated skill");
    let output = fixture.run_ms(&["--robot", "show", "refactor-auth-module"]);
    fixture.assert_success(&output, "show skill with substituted vars");

    let json = output.json();
    let skill = &json["skill"];
    assert_eq!(
        skill["name"].as_str().unwrap_or(""),
        "Refactor Auth Module",
        "Name should be substituted"
    );

    // Check tags were applied
    if let Some(tags) = skill.get("tags").and_then(|t| t.as_array()) {
        let tag_strs: Vec<&str> = tags.iter().filter_map(|t| t.as_str()).collect();
        println!("[VERIFY] Tags: {:?}", tag_strs);
    }

    fixture.log_step("checkpoint:template:teardown");
    fixture.checkpoint("template_variables_done");

    Ok(())
}

// =============================================================================
// TEST: Validate generated skill
// =============================================================================

#[test]
fn test_template_validate() -> Result<()> {
    let mut fixture = setup_template_fixture("template_validate")?;

    fixture.log_step("checkpoint:template:setup");

    // Generate a skill from template
    fixture.log_step("Generate skill from template");
    let output = fixture.run_ms(&[
        "--robot",
        "template",
        "apply",
        "debugging",
        "--id",
        "validate-test-skill",
        "--name",
        "Validate Test Skill",
        "--description",
        "Skill for testing validation pipeline.",
        "--tag",
        "test,validation",
    ]);
    fixture.assert_success(&output, "template apply");

    fixture.log_step("checkpoint:template:generate");

    let json = output.json();
    let skill_path = json["path"].as_str().unwrap_or("").to_string();
    assert!(
        !skill_path.is_empty(),
        "Template apply should return a skill path"
    );

    // Validate using the path returned by template apply
    fixture.log_step("checkpoint:template:validate");
    let output = fixture.run_ms(&["--robot", "validate", &skill_path]);
    fixture.assert_success(&output, "validate generated skill");

    println!(
        "[VERIFY] Validation output: {}",
        if output.stdout.len() > 200 {
            format!("{}...", &output.stdout[..200])
        } else {
            output.stdout.clone()
        }
    );

    fixture.log_step("checkpoint:template:teardown");
    fixture.checkpoint("template_validate_done");

    Ok(())
}

// =============================================================================
// TEST: Handle existing skill (overwrite protection)
// =============================================================================

#[test]
fn test_template_overwrite() -> Result<()> {
    let mut fixture = setup_template_fixture("template_overwrite")?;

    fixture.log_step("checkpoint:template:setup");

    // Create a skill first
    fixture.log_step("Create initial skill from template");
    let output = fixture.run_ms(&[
        "--robot",
        "template",
        "apply",
        "debugging",
        "--id",
        "overwrite-test",
        "--name",
        "Overwrite Test",
        "--description",
        "Initial skill for overwrite testing.",
        "--tag",
        "test",
    ]);
    fixture.assert_success(&output, "initial create");

    // Try to create again with same id - should fail
    fixture.log_step("Attempt to create duplicate skill");
    let output = fixture.run_ms(&[
        "--robot",
        "template",
        "apply",
        "debugging",
        "--id",
        "overwrite-test",
        "--name",
        "Overwrite Test v2",
        "--description",
        "Duplicate skill attempt.",
        "--tag",
        "test",
    ]);

    // Should fail because skill already exists
    assert!(
        !output.success,
        "Creating duplicate skill should fail"
    );
    println!(
        "[VERIFY] Duplicate creation correctly rejected: exit_code={}",
        output.exit_code
    );

    // Verify original skill is unchanged
    fixture.log_step("Verify original skill unchanged");
    let output = fixture.run_ms(&["--robot", "show", "overwrite-test"]);
    fixture.assert_success(&output, "show original skill");

    let json = output.json();
    assert_eq!(
        json["skill"]["name"].as_str().unwrap_or(""),
        "Overwrite Test",
        "Original skill name should be preserved"
    );

    fixture.log_step("checkpoint:template:teardown");
    fixture.checkpoint("template_overwrite_done");

    Ok(())
}

// =============================================================================
// TEST: Show template in markdown format
// =============================================================================

#[test]
fn test_template_show_markdown() -> Result<()> {
    let mut fixture = setup_template_fixture("template_show_markdown")?;

    fixture.log_step("checkpoint:template:setup");

    // Show template in markdown format (default)
    fixture.log_step("Show template in markdown format");
    let output = fixture.run_ms(&["template", "show", "debugging"]);
    fixture.assert_success(&output, "template show markdown");

    // Markdown output should contain template syntax
    assert!(
        !output.stdout.is_empty(),
        "Markdown output should not be empty"
    );
    println!(
        "[VERIFY] Markdown output length: {} chars",
        output.stdout.len()
    );

    // Should contain typical skill markdown elements
    assert!(
        output.stdout.contains('#') || output.stdout.contains("---"),
        "Markdown should contain headings or frontmatter"
    );

    fixture.log_step("checkpoint:template:teardown");
    fixture.checkpoint("template_show_markdown_done");

    Ok(())
}

// =============================================================================
// TEST: Apply template with different layers
// =============================================================================

#[test]
fn test_template_layers() -> Result<()> {
    let mut fixture = setup_template_fixture("template_layers")?;

    fixture.log_step("checkpoint:template:setup");

    // Apply template with org layer
    fixture.log_step("Apply template to org layer");
    let output = fixture.run_ms(&[
        "--robot",
        "template",
        "apply",
        "debugging",
        "--id",
        "org-debug-skill",
        "--name",
        "Org Debug Skill",
        "--description",
        "Organization-wide debugging patterns.",
        "--tag",
        "debugging",
        "--layer",
        "org",
    ]);
    fixture.assert_success(&output, "template apply to org layer");

    let json = output.json();
    assert_eq!(json["status"].as_str().unwrap_or(""), "ok");
    assert_eq!(
        json["skill_id"].as_str().unwrap_or(""),
        "org-debug-skill"
    );

    // Verify skill was created
    fixture.log_step("checkpoint:template:verify");
    let output = fixture.run_ms(&["--robot", "show", "org-debug-skill"]);
    fixture.assert_success(&output, "show org layer skill");

    println!(
        "[VERIFY] Created skill in org layer: {}",
        json["skill_id"].as_str().unwrap_or("?")
    );

    fixture.log_step("checkpoint:template:teardown");
    fixture.checkpoint("template_layers_done");

    Ok(())
}
