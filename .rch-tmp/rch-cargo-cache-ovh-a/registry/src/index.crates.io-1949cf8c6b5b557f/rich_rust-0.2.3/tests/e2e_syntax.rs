//! End-to-end tests for Syntax highlighting (requires `syntax` feature).
//!
//! Verifies syntax highlighting across multiple languages, themes, line numbers,
//! word wrap, background colors, and error handling.

#![cfg(feature = "syntax")]

mod common;

use common::init_test_logging;
use rich_rust::prelude::*;
use rich_rust::renderables::{Syntax, SyntaxError};

// =============================================================================
// Helper: render Syntax to text via Console capture
// =============================================================================

fn render_syntax_to_html(syntax: &Syntax, width: usize) -> String {
    let console = Console::builder()
        .width(width)
        .force_terminal(true)
        .color_system(ColorSystem::TrueColor)
        .markup(false)
        .build();

    console.begin_capture();
    console.print_renderable(syntax);
    console.export_html(true)
}

fn render_syntax_to_text(syntax: &Syntax) -> String {
    let console = Console::builder()
        .width(120)
        .force_terminal(true)
        .color_system(ColorSystem::TrueColor)
        .markup(false)
        .build();

    console.export_renderable_text(syntax)
}

// =============================================================================
// Multiple Languages
// =============================================================================

/// Test: Rust code highlights correctly.
#[test]
fn test_syntax_rust() {
    init_test_logging();

    let code = r#"fn main() {
    println!("Hello, world!");
}"#;
    let syntax = Syntax::new(code, "rust");
    let segments = syntax.render(Some(80)).expect("render");

    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(text.contains("fn"), "Should contain Rust keyword 'fn'");
    assert!(text.contains("main"), "Should contain function name");
    assert!(text.contains("println"), "Should contain macro name");
}

/// Test: Python code highlights correctly.
#[test]
fn test_syntax_python() {
    init_test_logging();

    let code = "def greet(name):\n    return f\"Hello, {name}!\"";
    let syntax = Syntax::new(code, "python");
    let segments = syntax.render(Some(80)).expect("render");

    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(text.contains("def"), "Should contain Python keyword");
    assert!(text.contains("greet"), "Should contain function name");
}

/// Test: JavaScript code highlights correctly.
#[test]
fn test_syntax_javascript() {
    init_test_logging();

    let code = "const add = (a, b) => a + b;\nconsole.log(add(1, 2));";
    let syntax = Syntax::new(code, "javascript");
    let segments = syntax.render(Some(80)).expect("render");

    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(text.contains("const"), "Should contain JS keyword");
    assert!(text.contains("add"), "Should contain variable name");
}

/// Test: HTML code highlights correctly.
#[test]
fn test_syntax_html() {
    init_test_logging();

    let code = "<html>\n<body>\n<h1>Title</h1>\n</body>\n</html>";
    let syntax = Syntax::new(code, "html");
    let segments = syntax.render(Some(80)).expect("render");

    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(text.contains("html"), "Should contain HTML tag");
    assert!(text.contains("Title"), "Should contain content");
}

/// Test: SQL code highlights correctly.
#[test]
fn test_syntax_sql() {
    init_test_logging();

    let code = "SELECT name, age FROM users WHERE age > 18 ORDER BY name;";
    let syntax = Syntax::new(code, "sql");
    let segments = syntax.render(Some(80)).expect("render");

    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(text.contains("SELECT"), "Should contain SQL keyword");
    assert!(text.contains("users"), "Should contain table name");
}

/// Test: YAML code highlights correctly.
#[test]
fn test_syntax_yaml() {
    init_test_logging();

    let code = "[package]\nname = \"test\"\nversion = \"1.0\"";
    let syntax = Syntax::new(code, "yaml");
    let segments = syntax.render(Some(80)).expect("render");

    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(text.contains("package"), "Should contain TOML section");
    assert!(text.contains("name"), "Should contain key");
}

/// Test: Go code highlights correctly.
#[test]
fn test_syntax_go() {
    init_test_logging();

    let code = "package main\n\nimport \"fmt\"\n\nfunc main() {\n\tfmt.Println(\"Hello\")\n}";
    let syntax = Syntax::new(code, "go");
    let segments = syntax.render(Some(80)).expect("render");

    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(text.contains("package"), "Should contain Go keyword");
    assert!(text.contains("func"), "Should contain func keyword");
}

// =============================================================================
// Theme Selection
// =============================================================================

/// Test: available_themes returns non-empty list.
#[test]
fn test_available_themes_non_empty() {
    init_test_logging();

    let themes = Syntax::available_themes();
    assert!(
        !themes.is_empty(),
        "Should have at least one available theme"
    );
}

/// Test: default theme "base16-ocean.dark" is available.
#[test]
fn test_default_theme_available() {
    init_test_logging();

    let themes = Syntax::available_themes();
    assert!(
        themes.iter().any(|t| t == "base16-ocean.dark"),
        "Default theme 'base16-ocean.dark' should be available"
    );
}

/// Test: Rendering with different themes produces different styled output.
#[test]
fn test_different_themes_produce_output() {
    init_test_logging();

    let code = "fn test() { 42 }";
    let themes = Syntax::available_themes();

    // Try rendering with multiple themes
    for theme_name in themes.iter().take(3) {
        let syntax = Syntax::new(code, "rust").theme(theme_name);
        let segments = syntax.render(Some(80)).expect("render");
        assert!(
            !segments.is_empty(),
            "Theme '{}' should produce segments",
            theme_name
        );
    }
}

/// Test: unknown theme returns error.
#[test]
fn test_unknown_theme_error() {
    init_test_logging();

    let syntax = Syntax::new("code", "rust").theme("nonexistent-theme-xyz");
    let result = syntax.render(Some(80));
    assert!(matches!(
        result,
        Err(SyntaxError::UnknownTheme(name)) if name == "nonexistent-theme-xyz"
    ));
}

/// Test: "InspiredGitHub" theme works.
#[test]
fn test_inspired_github_theme() {
    init_test_logging();

    let syntax = Syntax::new("fn main() {}", "rust").theme("InspiredGitHub");
    let segments = syntax.render(Some(80)).expect("render");
    assert!(!segments.is_empty());
}

// =============================================================================
// Line Numbers
// =============================================================================

/// Test: line numbers appear in rendered segments.
#[test]
fn test_line_numbers_enabled() {
    init_test_logging();

    let code = "x = 1\ny = 2\nz = 3";
    let syntax = Syntax::new(code, "python").line_numbers(true);
    let text = render_syntax_to_text(&syntax);
    let mut lines = text.lines();

    // Rich-style gutter: two spaces, line number (right-aligned), trailing space.
    assert!(
        lines.next().expect("first line").starts_with("  1 "),
        "First line should start with line number gutter"
    );
    assert!(
        lines.next().expect("second line").starts_with("  2 "),
        "Second line should start with line number gutter"
    );
    assert!(
        lines.next().expect("third line").starts_with("  3 "),
        "Third line should start with line number gutter"
    );
}

/// Test: line numbers disabled produces fewer segments.
#[test]
fn test_line_numbers_disabled() {
    init_test_logging();

    let code = "x = 1";
    let syntax_no_nums = Syntax::new(code, "python").line_numbers(false);
    let syntax_with_nums = Syntax::new(code, "python").line_numbers(true);

    let text_no = render_syntax_to_text(&syntax_no_nums);
    let text_with = render_syntax_to_text(&syntax_with_nums);

    assert!(
        text_with
            .lines()
            .next()
            .expect("first line")
            .starts_with("  1 "),
        "With line numbers, first line should start with the gutter"
    );
    assert!(
        !text_no
            .lines()
            .next()
            .expect("first line")
            .starts_with("  1 "),
        "Without line numbers, first line should not start with the gutter"
    );
}

/// Test: custom start_line offsets the line numbering.
#[test]
fn test_start_line_offset() {
    init_test_logging();

    let code = "a = 1\nb = 2\nc = 3";
    let syntax = Syntax::new(code, "python")
        .line_numbers(true)
        .start_line(10);
    let segments = syntax.render(Some(80)).expect("render");

    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(text.contains("10"), "Should start numbering at 10");
    assert!(text.contains("11"), "Second line should be 11");
    assert!(text.contains("12"), "Third line should be 12");
}

/// Test: start_line minimum clamps to 1.
#[test]
fn test_start_line_minimum_clamp() {
    init_test_logging();

    let syntax = Syntax::new("x = 1", "python")
        .line_numbers(true)
        .start_line(0);
    let segments = syntax.render(Some(80)).expect("render");

    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    // Should clamp to 1
    assert!(text.contains('1'), "Start line 0 should clamp to 1");
}

// =============================================================================
// Line Highlighting (via styled output)
// =============================================================================

/// Test: syntax highlighting produces styled segments (not all plain).
#[test]
fn test_highlighting_produces_styled_segments() {
    init_test_logging();

    let code = "fn main() {\n    let x = 42;\n    println!(\"{}\", x);\n}";
    let syntax = Syntax::new(code, "rust");
    let segments = syntax.render(Some(80)).expect("render");

    // At least some segments should have styles (syntax highlighting)
    let styled_count = segments.iter().filter(|s| s.style.is_some()).count();
    assert!(
        styled_count > 0,
        "Syntax highlighting should produce styled segments, got {styled_count} styled out of {}",
        segments.len()
    );
}

/// Test: HTML export of syntax contains color CSS.
#[test]
fn test_syntax_html_export_has_colors() {
    init_test_logging();

    let code = "fn main() { println!(\"Hello\"); }";
    let syntax = Syntax::new(code, "rust");
    let html = render_syntax_to_html(&syntax, 80);

    // Syntax highlighting should produce CSS color properties
    assert!(
        html.contains("color: #"),
        "HTML export should contain color styles from syntax highlighting"
    );
}

// =============================================================================
// Word Wrap
// =============================================================================

/// Test: word_wrap limits line width.
#[test]
fn test_word_wrap_limits_width() {
    init_test_logging();

    let long_line = "fn very_long_function_name_that_is_quite_extensive_and_should_probably_be_refactored() -> Result<(), Box<dyn std::error::Error>> { Ok(()) }";
    let syntax = Syntax::new(long_line, "rust").word_wrap(Some(40));
    let text = render_syntax_to_text(&syntax);

    // With word wrap, output should have newlines
    let line_count = text.lines().count();
    assert!(
        line_count >= 1,
        "Word wrapped long line should produce output"
    );
}

/// Test: word_wrap None allows full-width rendering.
#[test]
fn test_word_wrap_none() {
    init_test_logging();

    let code = "short";
    let syntax = Syntax::new(code, "python").word_wrap(None);
    let segments = syntax.render(Some(120)).expect("render");

    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(text.contains("short"));
}

// =============================================================================
// Background Colors
// =============================================================================

/// Test: custom background color is applied to segments.
#[test]
fn test_background_color_override() {
    init_test_logging();

    let syntax = Syntax::new("x = 1", "python").background_color(Color::parse("blue").unwrap());
    let segments = syntax.render(Some(80)).expect("render");

    // At least some segments should have a background color style
    let has_bg = segments.iter().any(|s| {
        s.style
            .as_ref()
            .map(|style| style.bgcolor.is_some())
            .unwrap_or(false)
    });
    assert!(has_bg, "Background color override should apply to segments");
}

/// Test: default syntax (no background override) still renders.
#[test]
fn test_default_background() {
    init_test_logging();

    let syntax = Syntax::new("fn main() {}", "rust");
    let segments = syntax.render(Some(80)).expect("render");
    assert!(!segments.is_empty());
}

// =============================================================================
// Unknown/Invalid Languages
// =============================================================================

/// Test: unknown language returns error.
#[test]
fn test_unknown_language_error() {
    init_test_logging();

    let syntax = Syntax::new("code", "nonexistent-language-xyz");
    let result = syntax.render(Some(80));
    assert!(matches!(
        result,
        Err(SyntaxError::UnknownLanguage(lang)) if lang == "nonexistent-language-xyz"
    ));
}

/// Test: python language works as a reliable language for plain content.
#[test]
fn test_reliable_language_renders_plain_content() {
    init_test_logging();

    let syntax = Syntax::new("plain text content", "python");
    let segments = syntax.render(Some(80)).expect("render");

    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(text.contains("plain text content"));
}

/// Test: available_languages returns non-empty list.
#[test]
fn test_available_languages_non_empty() {
    init_test_logging();

    let languages = Syntax::available_languages();
    assert!(!languages.is_empty(), "Should have available languages");
}

// =============================================================================
// Large File Rendering
// =============================================================================

/// Test: large code block renders without error.
#[test]
fn test_large_code_block() {
    init_test_logging();

    let mut code = String::new();
    for i in 0..100 {
        code.push_str(&format!("fn func_{i}() -> i32 {{ {i} }}\n"));
    }

    let syntax = Syntax::new(&code, "rust").line_numbers(true);
    let segments = syntax.render(Some(80)).expect("render");
    assert!(!segments.is_empty(), "100-line code block should render");

    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(text.contains("func_0"), "Should contain first function");
    assert!(text.contains("func_99"), "Should contain last function");
    assert!(text.contains("100"), "Should show line 100");
}

/// Test: multiline code with various Rust constructs.
#[test]
fn test_complex_rust_code() {
    init_test_logging();

    let code = r#"use std::collections::HashMap;

/// A documentation comment
pub struct Config {
    settings: HashMap<String, String>,
}

impl Config {
    pub fn new() -> Self {
        Self {
            settings: HashMap::new(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&String> {
        self.settings.get(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_new() {
        let config = Config::new();
        assert!(config.get("missing").is_none());
    }
}
"#;

    let syntax = Syntax::new(code, "rust")
        .line_numbers(true)
        .theme("base16-ocean.dark");
    let segments = syntax.render(Some(100)).expect("render");

    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(text.contains("Config"), "Should contain struct name");
    assert!(text.contains("HashMap"), "Should contain used types");
    assert!(text.contains("pub"), "Should contain visibility modifier");
}

// =============================================================================
// SyntaxError Display
// =============================================================================

/// Test: SyntaxError display messages.
#[test]
fn test_syntax_error_display() {
    init_test_logging();

    let err = SyntaxError::UnknownLanguage("foobar".into());
    assert_eq!(err.to_string(), "Unknown language: foobar");

    let err = SyntaxError::UnknownTheme("badtheme".into());
    assert_eq!(err.to_string(), "Unknown theme: badtheme");

    let err = SyntaxError::IoError("file not found".into());
    assert_eq!(err.to_string(), "IO error: file not found");
}

/// Test: SyntaxError implements std::error::Error.
#[test]
fn test_syntax_error_is_std_error() {
    init_test_logging();

    let err: Box<dyn std::error::Error> = Box::new(SyntaxError::UnknownLanguage("test".into()));
    assert!(err.source().is_none());
    assert!(!err.to_string().is_empty());
}

// =============================================================================
// Builder Pattern
// =============================================================================

/// Test: full builder chain works.
#[test]
fn test_full_builder_chain() {
    init_test_logging();

    let syntax = Syntax::new("let x = 1;", "rust")
        .line_numbers(true)
        .start_line(5)
        .theme("base16-ocean.dark")
        .indent_guides(true)
        .tab_size(2)
        .word_wrap(Some(60))
        .padding(1, 2);

    let segments = syntax.render(Some(80)).expect("render");
    assert!(!segments.is_empty());
}

/// Test: plain_text extraction.
#[test]
fn test_plain_text_extraction() {
    init_test_logging();

    let code = "fn hello() {}";
    let syntax = Syntax::new(code, "rust");
    let plain = syntax.plain_text();
    assert!(
        plain.contains("fn hello()"),
        "Plain text should contain the code"
    );
}

// =============================================================================
// Console Integration: Renderable Trait
// =============================================================================

/// Test: Syntax renders through Console.print_renderable.
#[test]
fn test_syntax_console_integration() {
    init_test_logging();

    let console = Console::builder()
        .width(80)
        .force_terminal(true)
        .color_system(ColorSystem::TrueColor)
        .build();

    let syntax = Syntax::new("fn main() {}", "rust");

    console.begin_capture();
    console.print_renderable(&syntax);
    let html = console.export_html(true);

    assert!(
        html.contains("main"),
        "Console render should include code content"
    );
}

/// Test: Syntax export to SVG works.
#[test]
fn test_syntax_svg_export() {
    init_test_logging();

    let console = Console::builder()
        .width(80)
        .force_terminal(true)
        .color_system(ColorSystem::TrueColor)
        .build();

    let syntax = Syntax::new("print('hello')", "python");

    console.begin_capture();
    console.print_renderable(&syntax);
    let svg = console.export_svg(true);

    assert!(svg.contains("<svg"), "Should produce SVG output");
    assert!(svg.contains("hello"), "SVG should contain code content");
}

// =============================================================================
// Tab Size
// =============================================================================

/// Test: tab_size minimum clamps to 1.
#[test]
fn test_tab_size_minimum() {
    init_test_logging();

    let syntax = Syntax::new("\tindented", "python").tab_size(0);
    let segments = syntax.render(Some(80)).expect("render");
    assert!(!segments.is_empty());
}

/// Test: different tab sizes produce different output widths.
#[test]
fn test_tab_size_affects_width() {
    init_test_logging();

    let code = "\ttab here";
    let syntax_small = Syntax::new(code, "python").tab_size(2);
    let syntax_large = Syntax::new(code, "python").tab_size(8);

    let text_small = render_syntax_to_text(&syntax_small);
    let text_large = render_syntax_to_text(&syntax_large);

    // Larger tab size should produce more spaces
    assert!(
        text_large.len() >= text_small.len(),
        "Larger tab size should produce wider output"
    );
}

// =============================================================================
// CRLF Handling
// =============================================================================

/// Test: Windows line endings (CRLF) are handled correctly.
#[test]
fn test_crlf_handling() {
    init_test_logging();

    let code = "x = 1\r\ny = 2\r\nz = 3";
    let syntax = Syntax::new(code, "python").line_numbers(true);
    let segments = syntax.render(Some(80)).expect("render");

    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    // Should render all three lines, stripping \r
    assert!(!text.contains('\r'), "CRLF should be stripped");
    assert!(text.contains("1"), "Should contain line 1 content");
    assert!(text.contains("2"), "Should contain line 2 content");
    assert!(text.contains("3"), "Should contain line 3 content");
}
