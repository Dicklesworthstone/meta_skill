use std::fs;
use std::path::PathBuf;

use ms::core::spec_lens::parse_markdown;
use ms::test_utils::{TestCase, run_table_tests};

fn fixture_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

#[test]
fn parse_markdown_table() -> Result<(), String> {
    let cases = vec![
        TestCase {
            name: "valid_minimal",
            input: "tests/fixtures/skills/valid_minimal.md",
            expected: ("Minimal Skill".to_string(), 1usize),
            should_panic: false,
        },
        TestCase {
            name: "valid_full",
            input: "tests/fixtures/skills/valid_full.md",
            expected: ("Full Skill".to_string(), 2usize),
            should_panic: false,
        },
        TestCase {
            name: "no_header",
            input: "tests/fixtures/skills/no_header.md",
            expected: ("".to_string(), 1usize),
            should_panic: false,
        },
    ];

    run_table_tests(cases, |relative_path| {
        let path = fixture_path(relative_path);
        let content = fs::read_to_string(&path).expect("read fixture");
        let spec = parse_markdown(&content).expect("parse markdown");
        (spec.metadata.name, spec.sections.len())
    })?;
    Ok(())
}
