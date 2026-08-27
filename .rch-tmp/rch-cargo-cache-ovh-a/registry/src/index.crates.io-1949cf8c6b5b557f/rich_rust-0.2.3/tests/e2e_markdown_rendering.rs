//! End-to-end tests for Markdown rendering.
//!
//! Tests the `Markdown` renderable with Console integration: headers, lists,
//! code blocks, links, tables, blockquotes, inline formatting, task lists,
//! and complex documents.

#![cfg(feature = "markdown")]

use std::io::Write;
use std::sync::{Arc, Mutex};

use rich_rust::color::ColorSystem;
use rich_rust::prelude::*;
use rich_rust::renderables::Markdown;
use rich_rust::sync::lock_recover;

// ============================================================================
// Helpers
// ============================================================================

struct BufferWriter(Arc<Mutex<Vec<u8>>>);

impl Write for BufferWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.lock().unwrap().flush()
    }
}

fn render_md_plain(source: &str) -> String {
    let md = Markdown::new(source);
    let segments = md.render(80);
    segments.iter().map(|s| s.text.as_ref()).collect()
}

fn render_md_via_console(source: &str) -> String {
    let md = Markdown::new(source);
    let buf = Arc::new(Mutex::new(Vec::new()));
    let writer = BufferWriter(Arc::clone(&buf));
    let console = Console::builder()
        .color_system(ColorSystem::TrueColor)
        .force_terminal(true)
        .width(80)
        .file(Box::new(writer))
        .build();
    console.print_renderable(&md);
    let guard = lock_recover(&buf);
    String::from_utf8_lossy(&guard).into_owned()
}

fn render_md_no_color(source: &str) -> String {
    let md = Markdown::new(source);
    let buf = Arc::new(Mutex::new(Vec::new()));
    let writer = BufferWriter(Arc::clone(&buf));
    let console = Console::builder()
        .force_terminal(false)
        .width(80)
        .file(Box::new(writer))
        .build();
    console.print_renderable(&md);
    let guard = lock_recover(&buf);
    String::from_utf8_lossy(&guard).into_owned()
}

// ============================================================================
// 1. Headers
// ============================================================================

#[test]
fn h1_renders_text() {
    let plain = render_md_plain("# Hello World");
    assert!(plain.contains("Hello World"), "H1 text missing: {plain}");
}

#[test]
fn h2_renders_text() {
    let plain = render_md_plain("## Section Two");
    assert!(plain.contains("Section Two"));
}

#[test]
fn h3_renders_text() {
    let plain = render_md_plain("### Subsection");
    assert!(plain.contains("Subsection"));
}

#[test]
fn h4_through_h6_render() {
    let plain = render_md_plain("#### H4\n\n##### H5\n\n###### H6");
    assert!(plain.contains("H4"));
    assert!(plain.contains("H5"));
    assert!(plain.contains("H6"));
}

#[test]
fn multiple_heading_levels() {
    let source = "# Title\n\n## Chapter 1\n\n### Section 1.1\n\n## Chapter 2";
    let plain = render_md_plain(source);
    assert!(plain.contains("Title"));
    assert!(plain.contains("Chapter 1"));
    assert!(plain.contains("Section 1.1"));
    assert!(plain.contains("Chapter 2"));
}

#[test]
fn headings_have_ansi_styling() {
    let output = render_md_via_console("# Styled Heading");
    assert!(
        output.contains("\x1b["),
        "heading should have ANSI codes: {output}"
    );
    assert!(output.contains("Styled Heading"));
}

// ============================================================================
// 2. Lists
// ============================================================================

#[test]
fn unordered_list_renders_bullets() {
    let source = "- Item 1\n- Item 2\n- Item 3";
    let plain = render_md_plain(source);
    assert!(plain.contains("Item 1"));
    assert!(plain.contains("Item 2"));
    assert!(plain.contains("Item 3"));
    // Default bullet char is '•'
    assert!(plain.contains('•'), "should use bullet char: {plain}");
}

#[test]
fn ordered_list_renders_numbers() {
    let source = "1. First\n2. Second\n3. Third";
    let plain = render_md_plain(source);
    assert!(plain.contains("First"));
    assert!(plain.contains("Second"));
    assert!(plain.contains("Third"));
    // Should contain number markers
    assert!(
        plain.contains("1.") || plain.contains("1"),
        "should have numbering"
    );
}

#[test]
fn nested_list() {
    let source = "- Outer 1\n  - Inner A\n  - Inner B\n- Outer 2";
    let plain = render_md_plain(source);
    assert!(plain.contains("Outer 1"));
    assert!(plain.contains("Inner A"));
    assert!(plain.contains("Inner B"));
    assert!(plain.contains("Outer 2"));
}

#[test]
fn custom_bullet_char() {
    let md = Markdown::new("- Item 1\n- Item 2").bullet_char('→');
    let segments = md.render(80);
    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(text.contains('→'), "should use custom bullet: {text}");
}

#[test]
fn custom_list_indent() {
    let md = Markdown::new("- Item\n  - Nested").list_indent(4);
    let segments = md.render(80);
    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(text.contains("Item"));
    assert!(text.contains("Nested"));
}

#[test]
fn task_list_renders_checkboxes() {
    let source = "- [ ] Todo\n- [x] Done";
    let plain = render_md_plain(source);
    assert!(plain.contains("Todo"));
    assert!(plain.contains("Done"));
    // Should contain checkbox characters
    assert!(
        plain.contains('☐') || plain.contains("[ ]"),
        "unchecked task should have checkbox: {plain}"
    );
    assert!(
        plain.contains('☑') || plain.contains("[x]"),
        "checked task should have checkbox: {plain}"
    );
}

// ============================================================================
// 3. Code blocks
// ============================================================================

#[test]
fn inline_code_renders() {
    let plain = render_md_plain("Use `cargo test` to run tests.");
    assert!(
        plain.contains("cargo test"),
        "inline code content missing: {plain}"
    );
}

#[test]
fn fenced_code_block_renders() {
    let source = "```rust\nfn main() {\n    println!(\"hello\");\n}\n```";
    let plain = render_md_plain(source);
    assert!(
        plain.contains("fn main()"),
        "code block content missing: {plain}"
    );
    assert!(plain.contains("println!"));
}

#[test]
fn code_block_without_language() {
    let source = "```\nplain code\n```";
    let plain = render_md_plain(source);
    assert!(plain.contains("plain code"));
}

#[test]
fn code_block_preserves_whitespace() {
    let source = "```\n  indented\n    more indented\n```";
    let plain = render_md_plain(source);
    assert!(plain.contains("indented"));
    assert!(plain.contains("more indented"));
}

#[test]
fn inline_code_has_styling() {
    let output = render_md_via_console("Use `code` here.");
    assert!(output.contains("\x1b["), "inline code should be styled");
    assert!(output.contains("code"));
}

// ============================================================================
// 4. Links
// ============================================================================

#[test]
fn link_renders_text() {
    let plain = render_md_plain("[Click here](https://example.com)");
    assert!(plain.contains("Click here"), "link text missing: {plain}");
}

#[test]
fn link_hides_url_by_default() {
    let plain = render_md_plain("[Docs](https://docs.rs)");
    assert!(
        !plain.contains("docs.rs"),
        "URL should be hidden by default (hyperlinks=true): {plain}"
    );
}

#[test]
fn link_shows_url_when_hyperlinks_disabled() {
    let md = Markdown::new("[Hidden](https://secret.com)").hyperlinks(false);
    let segments = md.render(80);
    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(text.contains("Hidden"));
    assert!(text.contains("secret.com"), "URL should be shown: {text}");
    assert!(text.contains(" (https://secret.com)"));
}

#[test]
fn link_with_special_chars_in_url() {
    let plain = render_md_plain("[Search](https://example.com/search?q=hello&page=1)");
    assert!(plain.contains("Search"));
}

// ============================================================================
// 5. Tables
// ============================================================================

#[test]
fn simple_table_renders() {
    let source = "| Name  | Age |\n|-------|-----|\n| Alice | 30  |\n| Bob   | 25  |";
    let plain = render_md_plain(source);
    assert!(plain.contains("Name"), "header missing: {plain}");
    assert!(plain.contains("Age"));
    assert!(plain.contains("Alice"));
    assert!(plain.contains("30"));
    assert!(plain.contains("Bob"));
    assert!(plain.contains("25"));
}

#[test]
fn table_has_borders() {
    let source = "| A | B |\n|---|---|\n| 1 | 2 |";
    let plain = render_md_plain(source);
    // Should contain box drawing characters
    let has_border = plain.chars().any(|c| {
        matches!(
            c,
            '┌' | '┐' | '└' | '┘' | '─' | '│' | '┼' | '┬' | '┴' | '├' | '┤'
        )
    });
    assert!(has_border, "table should have border chars: {plain}");
}

#[test]
fn table_with_alignment() {
    let source = "| Left | Center | Right |\n|:-----|:------:|------:|\n| a    | b      | c     |";
    let plain = render_md_plain(source);
    assert!(plain.contains("Left"));
    assert!(plain.contains("Center"));
    assert!(plain.contains("Right"));
}

#[test]
fn table_with_unicode_content() {
    let source = "| 名前 | 年齢 |\n|------|------|\n| 太郎 | 30   |";
    let plain = render_md_plain(source);
    assert!(plain.contains("名前"));
    assert!(plain.contains("太郎"));
}

#[test]
fn table_has_ansi_styling() {
    let source = "| H1 | H2 |\n|----|----|\n| a  | b  |";
    let output = render_md_via_console(source);
    assert!(output.contains("\x1b["), "table should have ANSI styling");
}

// ============================================================================
// 6. Blockquotes
// ============================================================================

#[test]
fn blockquote_renders_text() {
    let plain = render_md_plain("> This is a quote");
    assert!(plain.contains("This is a quote"));
}

#[test]
fn blockquote_has_prefix() {
    let plain = render_md_plain("> Quoted text");
    // Should have a quote prefix character (│ or >)
    assert!(
        plain.contains('│') || plain.contains('>'),
        "blockquote should have prefix: {plain}"
    );
}

#[test]
fn multi_paragraph_blockquote() {
    let source = "> First paragraph\n>\n> Second paragraph";
    let plain = render_md_plain(source);
    assert!(plain.contains("First paragraph"));
    assert!(plain.contains("Second paragraph"));
}

#[test]
fn blockquote_has_styling() {
    let output = render_md_via_console("> Styled quote");
    assert!(output.contains("\x1b["), "blockquote should be styled");
    assert!(output.contains("Styled quote"));
}

// ============================================================================
// 7. Inline formatting
// ============================================================================

#[test]
fn bold_text_renders() {
    let plain = render_md_plain("This is **bold** text.");
    assert!(plain.contains("bold"), "bold text missing: {plain}");
}

#[test]
fn italic_text_renders() {
    let plain = render_md_plain("This is *italic* text.");
    assert!(plain.contains("italic"), "italic text missing: {plain}");
}

#[test]
fn strikethrough_renders() {
    let plain = render_md_plain("This is ~~deleted~~ text.");
    assert!(
        plain.contains("deleted"),
        "strikethrough text missing: {plain}"
    );
}

#[test]
fn nested_formatting() {
    let plain = render_md_plain("***bold and italic***");
    assert!(plain.contains("bold and italic"));
}

#[test]
fn mixed_inline_formatting() {
    let plain = render_md_plain("**bold** and *italic* and `code` together");
    assert!(plain.contains("bold"));
    assert!(plain.contains("italic"));
    assert!(plain.contains("code"));
    assert!(plain.contains("together"));
}

#[test]
fn bold_has_ansi_styling() {
    let output = render_md_via_console("**styled bold**");
    assert!(output.contains("\x1b["), "bold should have ANSI codes");
    assert!(output.contains("styled bold"));
}

// ============================================================================
// 8. Complex documents
// ============================================================================

#[test]
fn readme_style_document() {
    let source = "\
# My Project

A description of the project.

## Features

- **Fast**: Blazing fast performance
- **Safe**: Memory safe by default
- *Extensible*: Plugin system

## Usage

```rust
fn main() {
    println!(\"hello\");
}
```

## Configuration

| Option | Default | Description |
|--------|---------|-------------|
| debug  | false   | Enable debug |
| port   | 8080    | Server port  |

> Note: Configuration is optional.

For more info, visit [the docs](https://docs.rs).
";
    let plain = render_md_plain(source);
    assert!(plain.contains("My Project"));
    assert!(plain.contains("Fast"));
    assert!(plain.contains("Safe"));
    assert!(plain.contains("Extensible"));
    assert!(plain.contains("fn main()"));
    assert!(plain.contains("debug"));
    assert!(plain.contains("8080"));
    assert!(plain.contains("Configuration is optional"));
    assert!(plain.contains("the docs"));
}

#[test]
fn complex_document_via_console() {
    let source = "# Title\n\n**Bold** and *italic*.\n\n- List item\n\n```\ncode\n```";
    let output = render_md_via_console(source);
    assert!(output.contains("Title"));
    assert!(output.contains("Bold"));
    assert!(output.contains("italic"));
    assert!(output.contains("code"));
    assert!(output.contains("\x1b["), "should have styling");
}

#[test]
fn complex_document_no_color() {
    let source = "# Title\n\n**Bold** text.\n\n1. First\n2. Second";
    let output = render_md_no_color(source);
    assert!(
        !output.contains("\x1b["),
        "no-color console should strip ANSI: {output}"
    );
    assert!(output.contains("Title"));
    assert!(output.contains("Bold"));
    assert!(output.contains("First"));
    assert!(output.contains("Second"));
}

// ============================================================================
// 9. Horizontal rules
// ============================================================================

#[test]
fn horizontal_rule_renders() {
    let plain = render_md_plain("---");
    // Should contain horizontal line character
    assert!(
        plain.contains('─') || plain.contains('-'),
        "horizontal rule should render: {plain}"
    );
}

#[test]
fn horizontal_rule_between_sections() {
    let source = "Above\n\n---\n\nBelow";
    let plain = render_md_plain(source);
    assert!(plain.contains("Above"));
    assert!(plain.contains("Below"));
}

// ============================================================================
// 10. Style customization
// ============================================================================

#[test]
fn custom_h1_style() {
    let md = Markdown::new("# Custom").h1_style(Style::parse("#ff0000 bold").unwrap());
    let segments = md.render(80);
    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(text.contains("Custom"));
    // Verify segments have styling
    let has_styled = segments.iter().any(|s| s.style.is_some());
    assert!(has_styled, "custom h1 should have styled segments");
}

#[test]
fn custom_emphasis_style() {
    let md = Markdown::new("*emphasis*").emphasis_style(Style::parse("bold green").unwrap());
    let segments = md.render(80);
    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(text.contains("emphasis"));
}

#[test]
fn custom_code_style() {
    let md = Markdown::new("`code`").code_style(Style::parse("yellow on black").unwrap());
    let segments = md.render(80);
    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(text.contains("code"));
}

#[test]
fn custom_quote_style() {
    let md = Markdown::new("> Quote").quote_style(Style::parse("italic cyan").unwrap());
    let segments = md.render(80);
    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(text.contains("Quote"));
}

// ============================================================================
// 11. Edge cases
// ============================================================================

#[test]
fn empty_markdown_renders() {
    let plain = render_md_plain("");
    // Empty markdown should not panic
    let _ = plain;
}

#[test]
fn whitespace_only_markdown() {
    let plain = render_md_plain("   \n\n   ");
    let _ = plain;
}

#[test]
fn very_long_paragraph() {
    let text = "word ".repeat(1000);
    let plain = render_md_plain(&text);
    assert!(plain.contains("word"));
}

#[test]
fn many_headings() {
    let source: String = (1..=100).map(|i| format!("## Heading {i}\n\n")).collect();
    let plain = render_md_plain(&source);
    assert!(plain.contains("Heading 1"));
    assert!(plain.contains("Heading 100"));
}

#[test]
fn unicode_in_all_elements() {
    let source = "\
# 日本語タイトル

**太字** と *斜体* のテスト。

- リスト項目1
- リスト項目2

> 引用テキスト

`コード`

| ヘッダー | 値 |
|---------|---|
| キー    | 値 |
";
    let plain = render_md_plain(source);
    assert!(plain.contains("日本語タイトル"));
    assert!(plain.contains("太字"));
    assert!(plain.contains("斜体"));
    assert!(plain.contains("リスト項目1"));
    assert!(plain.contains("引用テキスト"));
    assert!(plain.contains("コード"));
    assert!(plain.contains("ヘッダー"));
}

#[test]
fn paragraph_with_line_breaks() {
    let source = "First line\nSecond line\n\nNew paragraph";
    let plain = render_md_plain(source);
    assert!(plain.contains("First line"));
    assert!(plain.contains("New paragraph"));
}

#[test]
fn narrow_width_rendering() {
    let md = Markdown::new("# Title\n\nA paragraph with some text.");
    let segments = md.render(20);
    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(text.contains("Title"));
    assert!(text.contains("paragraph"));
}

#[test]
fn wide_width_rendering() {
    let md = Markdown::new("# Title\n\nContent here.");
    let segments = md.render(200);
    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(text.contains("Title"));
    assert!(text.contains("Content"));
}
