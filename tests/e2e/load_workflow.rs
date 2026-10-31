//! E2E Scenario: Load Workflow Integration Tests
//!
//! Comprehensive tests for the `ms load` command covering:
//! - Loading a skill by ID after indexing
//! - Loading with different disclosure levels
//! - Loading a non-existent skill
//! - Loading with --full and --complete flags
//! - Loading with token budget (--pack)
//! - Robot/JSON output format verification
//! - Which programs a skill's tool checks can run

use super::fixture::E2EFixture;
use ms::error::Result;

/// A skill can name a tool in a block condition, in `requires` or in `context.tools`, and
/// skills reach a machine through imports, bundles and remotes. These run the real binary
/// against an executable that records every run, so "nothing ran" is observed, not assumed.
#[cfg(unix)]
mod tool_checks {
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    use super::E2EFixture;
    use crate::fixture::CommandOutput;
    use ms::error::Result;
    use serde_json::Value;

    const ALWAYS: &str = "Text every reader gets.";
    const GATED: &str = "Text behind the tool check.";

    fn skill_with_condition(condition: &str) -> String {
        format!(
            "---\nname: Tool Check\ndescription: A block that depends on a tool\n\
             tags: [tools]\n---\n\n# Tool Check\n\n## Usage\n\n{ALWAYS}\n\n\
             ::: if {condition} :::\n{GATED}\n::: endif :::\n"
        )
    }

    fn skill_with_frontmatter(frontmatter: &str) -> String {
        format!(
            "---\nname: Tool Check\ndescription: A skill that names a tool\n\
             tags: [tools]\n{frontmatter}\n---\n\n# Tool Check\n\n## Usage\n\n{ALWAYS}\n"
        )
    }

    struct Bench {
        fixture: E2EFixture,
        marker: PathBuf,
        tools: PathBuf,
    }

    impl Bench {
        fn new(scenario: &str, skill: &str) -> Result<Self> {
            let mut fixture = E2EFixture::new(scenario);
            let output = fixture.init();
            fixture.assert_success(&output, "init");
            fixture.create_skill("tool-check", skill)?;
            let output = fixture.run_ms(&["--robot", "index"]);
            fixture.assert_success(&output, "index");

            let tools = fixture.root.join("tools-on-path");
            fs::create_dir_all(&tools)?;
            let marker = fixture.root.join("tool-ran");
            Ok(Self {
                fixture,
                marker,
                tools,
            })
        }

        /// An executable that reports version 2.5.0 and records that it ran.
        fn plant(&self, path: &Path) -> Result<()> {
            let script = format!(
                "#!/bin/sh\necho ran >> '{}'\necho 'planted 2.5.0'\n",
                self.marker.display()
            );
            write_executable(path, &script)
        }

        /// A stand-in for dcg that answers every `explain` with `decision`.
        fn gate(&self, decision: &str) -> Result<String> {
            let path = self.fixture.root.join("stand-in-dcg");
            let script = format!(
                "#!/bin/sh\nif [ \"$1\" = --version ]; then echo 'dcg 0.0.0'; exit 0; fi\n\
                 echo '{{\"decision\":\"{decision}\"}}'\n"
            );
            write_executable(&path, &script)?;
            Ok(path.to_str().unwrap().to_string())
        }

        fn no_gate(&self) -> String {
            let path = self.fixture.root.join("no-such-dcg");
            path.to_str().unwrap().to_string()
        }

        fn tools_on_path(&self) -> String {
            self.tools.to_str().unwrap().to_string()
        }

        fn ms(&mut self, args: &[&str], gate: &str, path: &str) -> CommandOutput {
            self.fixture
                .run_ms_with_env(args, &[("MS_SAFETY_DCG_BIN", gate), ("PATH", path)])
        }

        fn load(&mut self, gate: &str, path: &str) -> String {
            let output = self.ms(&["--robot", "load", "tool-check", "--full"], gate, path);
            self.fixture.assert_success(&output, "load");
            output.json()["data"]["content"]
                .as_str()
                .expect("content")
                .to_string()
        }

        fn requirements(&mut self, gate: &str, path: &str) -> Value {
            let output = self.ms(&["--robot", "requirements", "tool-check"], gate, path);
            self.fixture.assert_success(&output, "requirements");
            output.json()
        }

        fn tool_ran(&self) -> bool {
            self.marker.exists()
        }
    }

    fn write_executable(path: &Path, script: &str) -> Result<()> {
        fs::write(path, script)?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
        Ok(())
    }

    fn check<'a>(report: &'a Value, name: &str) -> &'a Value {
        report["checks"]
            .as_array()
            .expect("checks")
            .iter()
            .find(|check| check["name"] == name)
            .unwrap_or_else(|| panic!("no check named {name}: {report}"))
    }

    fn assert_block_hidden(bench: &Bench, content: &str) {
        assert!(!bench.tool_ran(), "the planted tool must not run");
        assert!(content.contains(ALWAYS), "the rest stays: {content}");
        assert!(!content.contains(GATED), "the block stays out: {content}");
    }

    #[test]
    fn test_tool_condition_with_a_path_runs_nothing() -> Result<()> {
        let skill = skill_with_condition("tool:./planted >= 1.0");
        let mut bench = Bench::new("tool_condition_path", &skill)?;
        bench.plant(&bench.fixture.root.join("planted"))?;
        let (gate, path) = (bench.gate("allow")?, bench.tools_on_path());

        let content = bench.load(&gate, &path);
        assert_block_hidden(&bench, &content);
        Ok(())
    }

    #[test]
    fn test_tool_condition_with_a_leading_dash_runs_nothing() -> Result<()> {
        let skill = skill_with_condition("tool:-planted >= 1.0");
        let mut bench = Bench::new("tool_condition_dash", &skill)?;
        bench.plant(&bench.tools.join("-planted"))?;
        let (gate, path) = (bench.gate("allow")?, bench.tools_on_path());

        let content = bench.load(&gate, &path);
        assert_block_hidden(&bench, &content);
        Ok(())
    }

    #[test]
    fn test_tool_condition_ignores_a_dot_path_entry() -> Result<()> {
        let skill = skill_with_condition("tool:planted >= 1.0");
        let mut bench = Bench::new("tool_condition_dot_entry", &skill)?;
        bench.plant(&bench.fixture.root.join("planted"))?;
        let gate = bench.gate("allow")?;

        let content = bench.load(&gate, ".");
        assert_block_hidden(&bench, &content);
        Ok(())
    }

    #[test]
    fn test_tool_condition_ignores_an_empty_path_entry() -> Result<()> {
        let skill = skill_with_condition("tool:planted >= 1.0");
        let mut bench = Bench::new("tool_condition_empty_entry", &skill)?;
        bench.plant(&bench.fixture.root.join("planted"))?;
        let gate = bench.gate("allow")?;
        let path = format!(":{}", bench.tools_on_path());

        let content = bench.load(&gate, &path);
        assert_block_hidden(&bench, &content);
        Ok(())
    }

    #[test]
    fn test_tool_condition_ignores_a_relative_path_entry() -> Result<()> {
        let skill = skill_with_condition("tool:planted >= 1.0");
        let mut bench = Bench::new("tool_condition_relative_entry", &skill)?;
        bench.plant(&bench.tools.join("planted"))?;
        let gate = bench.gate("allow")?;

        let content = bench.load(&gate, "tools-on-path");
        assert_block_hidden(&bench, &content);
        Ok(())
    }

    #[test]
    fn test_tool_condition_runs_a_tool_on_path_through_the_gate() -> Result<()> {
        let skill = skill_with_condition("tool:planted >= 1.0");
        let mut bench = Bench::new("tool_condition_allowed", &skill)?;
        bench.plant(&bench.tools.join("planted"))?;
        let (gate, path) = (bench.gate("allow")?, bench.tools_on_path());

        let content = bench.load(&gate, &path);
        assert!(bench.tool_ran(), "the gate allowed the version check");
        assert!(content.contains(GATED), "the check passed: {content}");
        Ok(())
    }

    #[test]
    fn test_tool_condition_compares_the_version_it_reads() -> Result<()> {
        let skill = skill_with_condition("tool:planted >= 9.0");
        let mut bench = Bench::new("tool_condition_low_version", &skill)?;
        bench.plant(&bench.tools.join("planted"))?;
        let (gate, path) = (bench.gate("allow")?, bench.tools_on_path());

        let content = bench.load(&gate, &path);
        assert!(bench.tool_ran(), "the gate allowed the version check");
        assert!(!content.contains(GATED), "2.5.0 is below 9.0: {content}");
        Ok(())
    }

    #[test]
    fn test_tool_condition_does_not_run_when_the_gate_refuses() -> Result<()> {
        let skill = skill_with_condition("tool:planted >= 1.0");
        let mut bench = Bench::new("tool_condition_refused", &skill)?;
        bench.plant(&bench.tools.join("planted"))?;
        let (gate, path) = (bench.gate("deny")?, bench.tools_on_path());

        let content = bench.load(&gate, &path);
        assert_block_hidden(&bench, &content);
        Ok(())
    }

    #[test]
    fn test_tool_condition_does_not_run_with_no_gate() -> Result<()> {
        let skill = skill_with_condition("tool:planted >= 1.0");
        let mut bench = Bench::new("tool_condition_no_gate", &skill)?;
        bench.plant(&bench.tools.join("planted"))?;
        let (gate, path) = (bench.no_gate(), bench.tools_on_path());

        let content = bench.load(&gate, &path);
        assert_block_hidden(&bench, &content);
        Ok(())
    }

    #[test]
    fn test_tool_presence_condition_needs_no_gate_because_nothing_runs() -> Result<()> {
        let skill = skill_with_condition("tool:planted");
        let mut bench = Bench::new("tool_condition_presence", &skill)?;
        bench.plant(&bench.tools.join("planted"))?;
        let (gate, path) = (bench.no_gate(), bench.tools_on_path());

        let content = bench.load(&gate, &path);
        assert!(!bench.tool_ran(), "a presence check runs nothing");
        assert!(content.contains(GATED), "the tool is there: {content}");
        Ok(())
    }

    #[test]
    fn test_bare_requirement_with_a_path_runs_nothing() -> Result<()> {
        let skill = skill_with_frontmatter("requires: [\"./planted\"]");
        let mut bench = Bench::new("tool_requires_bare_path", &skill)?;
        bench.plant(&bench.fixture.root.join("planted"))?;
        let (gate, path) = (bench.gate("allow")?, bench.tools_on_path());

        let report = bench.requirements(&gate, &path);
        assert!(!bench.tool_ran(), "the planted tool must not run");
        assert_eq!(check(&report, "./planted")["satisfied"], false);
        Ok(())
    }

    #[test]
    fn test_bin_requirement_with_a_path_runs_nothing() -> Result<()> {
        let skill = skill_with_frontmatter("requires: [\"bin:./planted\"]");
        let mut bench = Bench::new("tool_requires_bin_path", &skill)?;
        bench.plant(&bench.fixture.root.join("planted"))?;
        let (gate, path) = (bench.gate("allow")?, bench.tools_on_path());

        let report = bench.requirements(&gate, &path);
        assert!(!bench.tool_ran(), "the planted tool must not run");
        let check = check(&report, "tool:./planted");
        assert_eq!(check["satisfied"], false);
        assert!(
            check["message"]
                .as_str()
                .expect("message")
                .contains("not a plain tool name"),
            "the check must say why: {check}"
        );
        Ok(())
    }

    #[test]
    fn test_context_tool_with_a_path_runs_nothing() -> Result<()> {
        let skill = skill_with_frontmatter("context:\n  tools:\n    - ./planted");
        let mut bench = Bench::new("tool_context_path", &skill)?;
        bench.plant(&bench.fixture.root.join("planted"))?;
        let (gate, path) = (bench.gate("allow")?, bench.tools_on_path());

        let report = bench.requirements(&gate, &path);
        assert!(!bench.tool_ran(), "the planted tool must not run");
        assert_eq!(check(&report, "context_tool:./planted")["satisfied"], false);
        Ok(())
    }

    #[test]
    fn test_bin_requirement_reads_the_version_through_the_gate() -> Result<()> {
        let skill = skill_with_frontmatter("requires: [\"bin:planted\"]");
        let mut bench = Bench::new("tool_requires_allowed", &skill)?;
        bench.plant(&bench.tools.join("planted"))?;
        let (gate, path) = (bench.gate("allow")?, bench.tools_on_path());

        let report = bench.requirements(&gate, &path);
        assert!(bench.tool_ran(), "the gate allowed the version check");
        let check = check(&report, "tool:planted");
        assert_eq!(check["satisfied"], true);
        assert!(
            check["message"]
                .as_str()
                .expect("message")
                .contains("planted 2.5.0"),
            "the check reports the version: {check}"
        );
        Ok(())
    }

    #[test]
    fn test_bin_requirement_is_met_without_running_when_the_gate_refuses() -> Result<()> {
        let skill = skill_with_frontmatter("requires: [\"bin:planted\"]");
        let mut bench = Bench::new("tool_requires_refused", &skill)?;
        bench.plant(&bench.tools.join("planted"))?;
        let (gate, path) = (bench.gate("deny")?, bench.tools_on_path());

        let report = bench.requirements(&gate, &path);
        assert!(!bench.tool_ran(), "the gate refused the version check");
        assert_eq!(check(&report, "tool:planted")["satisfied"], true);
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Test skill definitions
// ---------------------------------------------------------------------------

const SKILL_RUST_ERRORS: &str = r#"---
name: Rust Error Handling
description: Best practices for error handling in Rust
tags: [rust, errors, advanced]
---

# Rust Error Handling

Use `Result<T, E>` and propagate errors with `?`.

## Guidelines

- Use thiserror for library errors
- Use anyhow for application errors
- Never use `.unwrap()` in library code

## Examples

```rust
fn read_file(path: &str) -> Result<String> {
    let content = std::fs::read_to_string(path)?;
    Ok(content)
}
```

## Pitfalls

- Avoid swallowing errors silently
- Do not overuse `Box<dyn Error>`
"#;

const SKILL_GO_CONCURRENCY: &str = r#"---
name: Go Concurrency
description: Concurrency patterns in Go
tags: [go, concurrency, intermediate]
provides: [go-concurrency]
---

# Go Concurrency

Use goroutines and channels for concurrent operations.

## Guidelines

- Prefer channels over shared memory
- Always handle done/cancel signals
- Use sync.WaitGroup for fan-out patterns

## Examples

```go
func worker(ch <-chan int, done chan<- bool) {
    for v := range ch {
        fmt.Println(v)
    }
    done <- true
}
```
"#;

const SKILL_WITH_DEPS: &str = r#"---
name: Go Testing
description: Testing patterns in Go
tags: [go, testing]
requires: [go-concurrency]
---

# Go Testing

Test concurrent Go code properly.

## Guidelines

- Use t.Parallel() for independent tests
- Use race detector with -race flag
"#;

// ---------------------------------------------------------------------------
// Setup helpers
// ---------------------------------------------------------------------------

/// Create a fixture with skills indexed and ready to load.
fn setup_load_fixture(scenario: &str) -> Result<E2EFixture> {
    let mut fixture = E2EFixture::new(scenario);

    fixture.log_step("Initialize ms");
    let output = fixture.init();
    fixture.assert_success(&output, "init");

    fixture.log_step("Create skills");
    fixture.create_skill("rust-error-handling", SKILL_RUST_ERRORS)?;
    fixture.create_skill("go-concurrency", SKILL_GO_CONCURRENCY)?;
    fixture.create_skill("go-testing", SKILL_WITH_DEPS)?;

    fixture.log_step("Index skills");
    let output = fixture.run_ms(&["--robot", "index"]);
    fixture.assert_success(&output, "index");

    fixture.checkpoint("load:indexed");

    Ok(fixture)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

/// Test loading a skill by its ID in JSON (robot) mode.
#[test]
fn test_load_skill_by_id() -> Result<()> {
    let mut fixture = setup_load_fixture("load_skill_by_id")?;

    fixture.log_step("Load skill by ID");
    let output = fixture.run_ms(&["--robot", "load", "rust-error-handling"]);
    fixture.assert_success(&output, "load rust-error-handling");

    // Verify JSON structure
    let json = output.json();
    assert_eq!(json["status"].as_str(), Some("ok"), "Status should be ok");

    let data = &json["data"];
    assert_eq!(
        data["skill_id"].as_str(),
        Some("rust-error-handling"),
        "Skill ID should match"
    );
    assert_eq!(
        data["name"].as_str(),
        Some("Rust Error Handling"),
        "Skill name should match"
    );

    // Content should be present
    assert!(
        data["content"].as_str().is_some(),
        "Content should be present in load output"
    );

    // Token count should be positive
    let token_count = data["token_count"].as_u64().unwrap_or(0);
    assert!(token_count > 0, "Token count should be positive");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "load",
        &format!("Loaded skill with {} tokens", token_count),
        Some(serde_json::json!({
            "skill_id": "rust-error-handling",
            "token_count": token_count
        })),
    );

    fixture.checkpoint("load:after_load");
    fixture.generate_report();
    Ok(())
}

/// Test loading a skill that does not exist.
#[test]
fn test_load_nonexistent_skill() -> Result<()> {
    let mut fixture = setup_load_fixture("load_nonexistent_skill")?;

    fixture.log_step("Attempt to load non-existent skill");
    let output = fixture.run_ms(&["--robot", "load", "this-skill-does-not-exist"]);

    // The command should fail
    assert!(!output.success, "Loading a non-existent skill should fail");

    // stderr or stdout should mention not found
    let combined = format!("{}{}", output.stdout, output.stderr);
    let mentions_not_found = combined.to_lowercase().contains("not found")
        || combined.to_lowercase().contains("error")
        || combined.to_lowercase().contains("no skill");
    assert!(
        mentions_not_found,
        "Output should indicate skill was not found. Got: {}",
        combined
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "load",
        "Non-existent skill correctly rejected",
        None,
    );

    fixture.generate_report();
    Ok(())
}

/// Test loading with the --full flag for full disclosure.
#[test]
fn test_load_full_disclosure() -> Result<()> {
    let mut fixture = setup_load_fixture("load_full_disclosure")?;

    fixture.log_step("Load skill with --full flag");
    let output = fixture.run_ms(&["--robot", "load", "rust-error-handling", "--full"]);
    fixture.assert_success(&output, "load --full");

    let json = output.json();
    let data = &json["data"];

    // Disclosure level should be "full"
    let level = data["disclosure_level"].as_str().unwrap_or("");
    assert_eq!(level, "full", "Disclosure level should be 'full'");

    // Full disclosure should include more content
    let content = data["content"].as_str().unwrap_or("");
    assert!(
        !content.is_empty(),
        "Full disclosure should produce content"
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "load",
        "Full disclosure load verified",
        Some(serde_json::json!({
            "disclosure_level": level,
            "content_length": content.len()
        })),
    );

    fixture.generate_report();
    Ok(())
}

/// Test loading with the --complete flag for complete disclosure (scripts + references).
#[test]
fn test_load_complete_disclosure() -> Result<()> {
    let mut fixture = setup_load_fixture("load_complete_disclosure")?;

    fixture.log_step("Load skill with --complete flag");
    let output = fixture.run_ms(&["--robot", "load", "rust-error-handling", "--complete"]);
    fixture.assert_success(&output, "load --complete");

    let json = output.json();
    let data = &json["data"];

    // Disclosure level should be "complete"
    let level = data["disclosure_level"].as_str().unwrap_or("");
    assert_eq!(level, "complete", "Disclosure level should be 'complete'");

    // Scripts and references arrays should be present (even if empty)
    assert!(
        data.get("scripts").is_some(),
        "Complete disclosure should include scripts field"
    );
    assert!(
        data.get("references").is_some(),
        "Complete disclosure should include references field"
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "load",
        "Complete disclosure load verified",
        Some(serde_json::json!({
            "disclosure_level": level
        })),
    );

    fixture.generate_report();
    Ok(())
}

/// Test loading with a token budget via --pack.
#[test]
fn test_load_with_pack_budget() -> Result<()> {
    let mut fixture = setup_load_fixture("load_with_pack_budget")?;

    fixture.log_step("Load skill with --pack budget");
    let output = fixture.run_ms(&["--robot", "load", "rust-error-handling", "--pack", "500"]);
    fixture.assert_success(&output, "load --pack 500");

    let json = output.json();
    let data = &json["data"];

    // Pack info should be present
    let pack = &data["pack"];
    assert!(
        !pack.is_null(),
        "Pack info should be present when using --pack"
    );

    // Token count should be within or near the budget
    let token_count = data["token_count"].as_u64().unwrap_or(0);
    assert!(
        token_count > 0,
        "Token count should be positive with pack budget"
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "load",
        &format!("Pack budget load: {} tokens (budget 500)", token_count),
        Some(serde_json::json!({
            "budget": 500,
            "actual_tokens": token_count
        })),
    );

    fixture.generate_report();
    Ok(())
}

/// Test loading a skill with dependencies.
#[test]
fn test_load_with_dependencies() -> Result<()> {
    let mut fixture = setup_load_fixture("load_with_dependencies")?;

    fixture.log_step("Load skill that has dependencies");
    let output = fixture.run_ms(&["--robot", "load", "go-testing"]);
    fixture.assert_success(&output, "load go-testing (has deps)");

    let json = output.json();
    let data = &json["data"];

    // Verify the skill was loaded
    assert_eq!(
        data["skill_id"].as_str(),
        Some("go-testing"),
        "Skill ID should be go-testing"
    );

    // Dependencies loaded should be present (may or may not have resolved deps)
    let deps = data["dependencies_loaded"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "load",
        &format!("Loaded skill with {} dependencies", deps),
        Some(serde_json::json!({
            "skill_id": "go-testing",
            "dependencies_loaded": deps
        })),
    );

    fixture.generate_report();
    Ok(())
}

/// Test loading with dependencies turned off.
#[test]
fn test_load_deps_off() -> Result<()> {
    let mut fixture = setup_load_fixture("load_deps_off")?;

    fixture.log_step("Load skill with --deps off");
    let output = fixture.run_ms(&["--robot", "load", "go-testing", "--deps", "off"]);
    fixture.assert_success(&output, "load --deps off");

    let json = output.json();
    let data = &json["data"];

    // With deps off, dependencies_loaded should be empty
    let deps = data["dependencies_loaded"]
        .as_array()
        .map(|a| a.len())
        .unwrap_or(0);
    assert_eq!(deps, 0, "Dependencies should not be loaded with --deps off");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "load",
        "Deps off mode verified",
        Some(serde_json::json!({ "dependencies_loaded": 0 })),
    );

    fixture.generate_report();
    Ok(())
}

/// Test that loading a skill records usage (verify through JSON output structure).
#[test]
fn test_load_json_output_structure() -> Result<()> {
    let mut fixture = setup_load_fixture("load_json_structure")?;

    fixture.log_step("Load skill and verify JSON output structure");
    let output = fixture.run_ms(&["--robot", "load", "rust-error-handling"]);
    fixture.assert_success(&output, "load for JSON verification");

    let json = output.json();

    // Verify top-level fields
    assert!(json.get("status").is_some(), "Should have 'status' field");
    assert!(
        json.get("timestamp").is_some(),
        "Should have 'timestamp' field"
    );
    assert!(json.get("version").is_some(), "Should have 'version' field");
    assert!(json.get("data").is_some(), "Should have 'data' field");
    assert!(
        json.get("warnings").is_some(),
        "Should have 'warnings' field"
    );

    // Verify data fields
    let data = &json["data"];
    assert!(data.get("skill_id").is_some(), "data should have skill_id");
    assert!(data.get("name").is_some(), "data should have name");
    assert!(
        data.get("disclosure_level").is_some(),
        "data should have disclosure_level"
    );
    assert!(
        data.get("token_count").is_some(),
        "data should have token_count"
    );
    assert!(data.get("content").is_some(), "data should have content");
    assert!(
        data.get("frontmatter").is_some(),
        "data should have frontmatter"
    );
    assert!(
        data.get("dependencies_loaded").is_some(),
        "data should have dependencies_loaded"
    );
    assert!(
        data.get("inheritance_chain").is_some(),
        "data should have inheritance_chain"
    );

    // Verify frontmatter fields
    let fm = &data["frontmatter"];
    assert!(fm.get("id").is_some(), "frontmatter should have id");
    assert!(fm.get("name").is_some(), "frontmatter should have name");
    assert!(
        fm.get("description").is_some() || fm.get("version").is_some(),
        "frontmatter should have metadata fields"
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "load",
        "JSON output structure verified",
        Some(serde_json::json!({
            "top_level_fields": ["status", "timestamp", "version", "data", "warnings"],
            "data_fields": ["skill_id", "name", "disclosure_level", "token_count", "content"]
        })),
    );

    fixture.generate_report();
    Ok(())
}

/// Test loading with an explicit disclosure level.
#[test]
fn test_load_explicit_level() -> Result<()> {
    let mut fixture = setup_load_fixture("load_explicit_level")?;

    fixture.log_step("Load skill with --level overview");
    let output = fixture.run_ms(&[
        "--robot",
        "load",
        "rust-error-handling",
        "--level",
        "overview",
    ]);
    fixture.assert_success(&output, "load --level overview");

    let json = output.json();
    let data = &json["data"];
    let level = data["disclosure_level"].as_str().unwrap_or("");

    // The level should be "overview"
    assert_eq!(level, "overview", "Disclosure level should be 'overview'");

    // Load again with a higher level and compare token counts
    fixture.log_step("Load skill with --level full");
    let output_full =
        fixture.run_ms(&["--robot", "load", "rust-error-handling", "--level", "full"]);
    fixture.assert_success(&output_full, "load --level full");

    let json_full = output_full.json();
    let tokens_overview = data["token_count"].as_u64().unwrap_or(0);
    let tokens_full = json_full["data"]["token_count"].as_u64().unwrap_or(0);

    // Full disclosure should generally have more tokens than overview
    // (but allow equal in edge cases for very small skills)
    assert!(
        tokens_full >= tokens_overview,
        "Full disclosure ({}) should have >= tokens than overview ({})",
        tokens_full,
        tokens_overview
    );

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "load",
        "Explicit disclosure levels verified",
        Some(serde_json::json!({
            "overview_tokens": tokens_overview,
            "full_tokens": tokens_full
        })),
    );

    fixture.generate_report();
    Ok(())
}

/// Test that loading without --robot produces human-readable output.
#[test]
fn test_load_human_output() -> Result<()> {
    let mut fixture = setup_load_fixture("load_human_output")?;

    fixture.log_step("Load skill without --robot (human output)");
    let output = fixture.run_ms(&["load", "rust-error-handling"]);
    fixture.assert_success(&output, "load human output");

    // Human output should contain the skill name
    fixture.assert_output_contains(&output, "Rust Error Handling");

    // Human output should not be valid JSON (it is prose/formatted)
    let is_json = serde_json::from_str::<serde_json::Value>(&output.stdout).is_ok();
    assert!(!is_json, "Human output should not be raw JSON");

    fixture.emit_event(
        super::fixture::LogLevel::Info,
        "load",
        "Human output format verified",
        None,
    );

    fixture.generate_report();
    Ok(())
}
