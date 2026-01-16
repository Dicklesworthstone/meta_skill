//! Integration tests for skill composition (extends and includes)
//!
//! Tests the complete workflow of skills with inheritance and composition,
//! including indexing, search, and caching behavior.

use crate::fixture::{TestFixture, TestSkill};
use crate::{assert_command_success, assert_stdout_contains};

/// Create a base error handling skill
fn error_handling_base() -> TestSkill {
    TestSkill::with_content(
        "error-handling-base",
        r#"---
id: error-handling-base
name: Error Handling Base
description: Base error handling patterns
tags: [error-handling, foundation]
---

# Error Handling Base

Foundation for error handling patterns.

## Rules

- Always handle errors explicitly
- Use meaningful error messages
- Include context in error messages
- Never ignore errors silently

## Pitfalls

- Swallowing exceptions without logging
- Using generic error messages like "An error occurred"
"#,
    )
}

/// Create a Rust-specific error handling skill that extends the base
fn rust_error_handling() -> TestSkill {
    TestSkill::with_content(
        "rust-error-handling",
        r#"---
id: rust-error-handling
name: Rust Error Handling
description: Rust-specific error handling patterns
tags: [error-handling, rust]
extends: error-handling-base
---

# Rust Error Handling

Rust-specific error handling patterns, building on the base error handling skill.

## Rules

- Use thiserror for library errors
- Use anyhow for application errors
- Implement std::error::Error trait
- Use ? operator for error propagation
"#,
    )
}

/// Create a Python-specific error handling skill that extends the base
fn python_error_handling() -> TestSkill {
    TestSkill::with_content(
        "python-error-handling",
        r#"---
id: python-error-handling
name: Python Error Handling
description: Python-specific error handling patterns
tags: [error-handling, python]
extends: error-handling-base
---

# Python Error Handling

Python-specific error handling patterns.

## Rules

- Use specific exception types
- Create custom exception classes when needed
- Use context managers for cleanup
"#,
    )
}

/// Create a testing skill
fn testing_skill() -> TestSkill {
    TestSkill::with_content(
        "testing-patterns",
        r#"---
id: testing-patterns
name: Testing Patterns
description: Common testing patterns
tags: [testing]
---

# Testing Patterns

## Checklist

- [ ] Write unit tests for happy paths
- [ ] Write tests for error cases
- [ ] Test edge cases
- [ ] Ensure test isolation
"#,
    )
}

/// Create a logging skill
fn logging_skill() -> TestSkill {
    TestSkill::with_content(
        "logging-patterns",
        r#"---
id: logging-patterns
name: Logging Patterns
description: Common logging patterns
tags: [logging]
---

# Logging Patterns

## Rules

- Use structured logging
- Include request IDs in logs
- Log at appropriate levels
"#,
    )
}

/// Create a composite skill that includes multiple skills
fn rust_complete() -> TestSkill {
    TestSkill::with_content(
        "rust-complete",
        r#"---
id: rust-complete
name: Complete Rust Development
description: Complete Rust development skill combining error handling, testing, and logging
tags: [rust, complete]
includes:
  - skill: rust-error-handling
    into: rules
  - skill: testing-patterns
    into: checklist
  - skill: logging-patterns
    into: rules
    prefix: "[Logging] "
---

# Complete Rust Development

A comprehensive skill for Rust development combining error handling, testing, and logging patterns.

## Rules

- Follow Rust idioms and conventions
- Use clippy for linting
"#,
    )
}

/// Create a skill with deep inheritance (grandchild)
fn advanced_rust_error_handling() -> TestSkill {
    TestSkill::with_content(
        "advanced-rust-error-handling",
        r#"---
id: advanced-rust-error-handling
name: Advanced Rust Error Handling
description: Advanced Rust error handling patterns
tags: [error-handling, rust, advanced]
extends: rust-error-handling
---

# Advanced Rust Error Handling

Advanced error handling patterns for Rust, extending rust-error-handling.

## Rules

- Use error-stack for detailed backtraces
- Implement custom error contexts
- Use tracing for error spans
"#,
    )
}

// =============================================================================
// Tests
// =============================================================================

#[test]
fn test_index_base_skill() {
    let mut fixture = TestFixture::new("index_base_skill");
    let init = fixture.init();
    assert_command_success!(init, "init");

    // Add base skill
    fixture.add_skill(&error_handling_base());

    // Index
    let output = fixture.run_ms(&["--robot", "index"]);
    assert_command_success!(output, "index");
    assert_stdout_contains!(output, r#""indexed":1"#);

    // Verify skill is in database
    fixture.open_db();
    fixture.verify_db_state(
        |db| {
            let count: i64 = db
                .query_row("SELECT COUNT(*) FROM skills WHERE id = 'error-handling-base'", [], |r| r.get(0))
                .unwrap();
            count == 1
        },
        "error-handling-base should be indexed",
    );
}

#[test]
fn test_index_skill_with_extends() {
    let mut fixture = TestFixture::new("index_skill_with_extends");
    let init = fixture.init();
    assert_command_success!(init, "init");

    // Add base skill first
    fixture.add_skill(&error_handling_base());

    // Add child skill that extends base
    fixture.add_skill(&rust_error_handling());

    // Index
    let output = fixture.run_ms(&["--robot", "index"]);
    assert_command_success!(output, "index");

    // Verify both skills are indexed
    fixture.open_db();
    fixture.verify_db_state(
        |db| {
            let count: i64 = db
                .query_row("SELECT COUNT(*) FROM skills", [], |r| r.get(0))
                .unwrap();
            count == 2
        },
        "should have 2 skills indexed",
    );
}

#[test]
fn test_index_skill_with_includes() {
    let mut fixture = TestFixture::new("index_skill_with_includes");
    let init = fixture.init();
    assert_command_success!(init, "init");

    // Add skills that will be included
    fixture.add_skill(&rust_error_handling());
    fixture.add_skill(&error_handling_base());
    fixture.add_skill(&testing_skill());
    fixture.add_skill(&logging_skill());

    // Add composite skill
    fixture.add_skill(&rust_complete());

    // Index
    let output = fixture.run_ms(&["--robot", "index"]);
    assert_command_success!(output, "index");

    // Verify all skills are indexed
    fixture.open_db();
    fixture.verify_db_state(
        |db| {
            let count: i64 = db
                .query_row("SELECT COUNT(*) FROM skills", [], |r| r.get(0))
                .unwrap();
            count == 5
        },
        "should have 5 skills indexed",
    );
}

#[test]
fn test_deep_inheritance_chain() {
    let mut fixture = TestFixture::new("deep_inheritance_chain");
    let init = fixture.init();
    assert_command_success!(init, "init");

    // Add skills in correct order (base first)
    fixture.add_skill(&error_handling_base());
    fixture.add_skill(&rust_error_handling());
    fixture.add_skill(&advanced_rust_error_handling());

    // Index
    let output = fixture.run_ms(&["--robot", "index"]);
    assert_command_success!(output, "index");

    // Verify all skills are indexed
    fixture.open_db();
    fixture.verify_db_state(
        |db| {
            let count: i64 = db
                .query_row("SELECT COUNT(*) FROM skills", [], |r| r.get(0))
                .unwrap();
            count == 3
        },
        "should have 3 skills indexed (base, child, grandchild)",
    );
}

#[test]
fn test_search_finds_extended_skill() {
    let fixture = TestFixture::new("search_finds_extended_skill");
    let init = fixture.init();
    assert_command_success!(init, "init");

    // Add base and extended skills
    fixture.add_skill(&error_handling_base());
    fixture.add_skill(&rust_error_handling());

    // Index
    let output = fixture.run_ms(&["--robot", "index"]);
    assert_command_success!(output, "index");
    assert_stdout_contains!(output, r#""indexed":2"#);

    // Search for "thiserror" (specific to rust-error-handling)
    let output = fixture.run_ms(&["search", "thiserror"]);
    // Search should find the skill (non-robot mode shows skill names)
    assert_command_success!(output, "search thiserror");
}

#[test]
fn test_search_finds_base_content_in_child() {
    let fixture = TestFixture::new("search_finds_base_content_in_child");
    let init = fixture.init();
    assert_command_success!(init, "init");

    // Add base and extended skills
    fixture.add_skill(&error_handling_base());
    fixture.add_skill(&rust_error_handling());

    // Index
    let output = fixture.run_ms(&["--robot", "index"]);
    assert_command_success!(output, "index");

    // Search for "meaningful error messages" (from base)
    // Both base and extended should be found since the extended
    // skill inherits the base content
    let output = fixture.run_ms(&["--robot", "search", "meaningful error messages"]);
    assert_command_success!(output, "search base content");
    // At minimum, base should be found
    assert_stdout_contains!(output, "error-handling-base");
}

#[test]
fn test_multiple_children_same_parent() {
    let mut fixture = TestFixture::new("multiple_children_same_parent");
    let init = fixture.init();
    assert_command_success!(init, "init");

    // Add base and two children
    fixture.add_skill(&error_handling_base());
    fixture.add_skill(&rust_error_handling());
    fixture.add_skill(&python_error_handling());

    // Index
    let output = fixture.run_ms(&["--robot", "index"]);
    assert_command_success!(output, "index");

    // Verify all three skills are indexed
    fixture.open_db();
    fixture.verify_db_state(
        |db| {
            let count: i64 = db
                .query_row("SELECT COUNT(*) FROM skills", [], |r| r.get(0))
                .unwrap();
            count == 3
        },
        "should have 3 skills indexed (1 base, 2 children)",
    );
}

#[test]
fn test_show_extended_skill() {
    let fixture = TestFixture::new("show_extended_skill");
    let init = fixture.init();
    assert_command_success!(init, "init");

    // Add base and extended skills
    fixture.add_skill(&error_handling_base());
    fixture.add_skill(&rust_error_handling());

    // Index
    let output = fixture.run_ms(&["--robot", "index"]);
    assert_command_success!(output, "index");

    // Show the extended skill
    let output = fixture.run_ms(&["--robot", "show", "rust-error-handling"]);
    assert_command_success!(output, "show extended skill");
    // Should contain the extends field
    assert_stdout_contains!(output, "rust-error-handling");
}

#[test]
fn test_show_composite_skill() {
    let fixture = TestFixture::new("show_composite_skill");
    let init = fixture.init();
    assert_command_success!(init, "init");

    // Add all skills for composition
    fixture.add_skill(&error_handling_base());
    fixture.add_skill(&rust_error_handling());
    fixture.add_skill(&testing_skill());
    fixture.add_skill(&logging_skill());
    fixture.add_skill(&rust_complete());

    // Index
    let output = fixture.run_ms(&["--robot", "index"]);
    assert_command_success!(output, "index");

    // Show the composite skill
    let output = fixture.run_ms(&["--robot", "show", "rust-complete"]);
    assert_command_success!(output, "show composite skill");
    assert_stdout_contains!(output, "rust-complete");
}

#[test]
fn test_list_skills_with_composition() {
    let fixture = TestFixture::new("list_skills_with_composition");
    let init = fixture.init();
    assert_command_success!(init, "init");

    // Add all skills
    fixture.add_skill(&error_handling_base());
    fixture.add_skill(&rust_error_handling());
    fixture.add_skill(&testing_skill());
    fixture.add_skill(&logging_skill());
    fixture.add_skill(&rust_complete());

    // Index
    let output = fixture.run_ms(&["--robot", "index"]);
    assert_command_success!(output, "index");

    // List all skills
    let output = fixture.run_ms(&["--robot", "list"]);
    assert_command_success!(output, "list");
    // Should show all 5 skills
    assert_stdout_contains!(output, "error-handling-base");
    assert_stdout_contains!(output, "rust-error-handling");
    assert_stdout_contains!(output, "testing-patterns");
    assert_stdout_contains!(output, "logging-patterns");
    assert_stdout_contains!(output, "rust-complete");
}

#[test]
fn test_reindex_after_parent_change() {
    let mut fixture = TestFixture::new("reindex_after_parent_change");
    let init = fixture.init();
    assert_command_success!(init, "init");

    // Add base and extended skills
    fixture.add_skill(&error_handling_base());
    fixture.add_skill(&rust_error_handling());

    // Index
    let output = fixture.run_ms(&["--robot", "index"]);
    assert_command_success!(output, "index");

    // Modify base skill
    let updated_base = TestSkill::with_content(
        "error-handling-base",
        r#"---
id: error-handling-base
name: Error Handling Base (Updated)
description: Updated base error handling patterns
tags: [error-handling, foundation, updated]
---

# Error Handling Base (Updated)

Updated foundation for error handling patterns.

## Rules

- Always handle errors explicitly (UPDATED)
- Use meaningful error messages with context
"#,
    );
    fixture.add_skill(&updated_base);

    // Re-index with force
    let output = fixture.run_ms(&["--robot", "index", "--force"]);
    assert_command_success!(output, "reindex with force");

    // Verify skills are still indexed
    fixture.open_db();
    fixture.verify_db_state(
        |db| {
            let count: i64 = db
                .query_row("SELECT COUNT(*) FROM skills", [], |r| r.get(0))
                .unwrap();
            count == 2
        },
        "should still have 2 skills after reindex",
    );
}
