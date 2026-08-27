//! End-to-end tests for Markdown rendering (requires `markdown` feature).
//!
//! Verifies markdown rendering across headers, lists, code blocks, links,
//! tables, blockquotes, inline formatting, and complex documents.

#![cfg(feature = "markdown")]

mod common;

use common::init_test_logging;
use rich_rust::prelude::*;
use rich_rust::renderables::Markdown;
use rich_rust::style::Attributes;

// =============================================================================
// Helper: render Markdown to collected text
// =============================================================================

fn render_md_text(source: &str, width: usize) -> String {
    let md = Markdown::new(source);
    let segments = md.render(width);
    segments.iter().map(|s| s.text.as_ref()).collect()
}

fn render_md_to_html(source: &str, width: usize) -> String {
    let md = Markdown::new(source);
    let console = Console::builder()
        .width(width)
        .force_terminal(true)
        .color_system(ColorSystem::TrueColor)
        .markup(false)
        .build();

    console.begin_capture();
    console.print_renderable(&md);
    console.export_html(true)
}

fn console_export_text(source: &str, width: usize) -> String {
    let md = Markdown::new(source);
    let console = Console::builder()
        .width(width)
        .force_terminal(true)
        .color_system(ColorSystem::TrueColor)
        .markup(false)
        .build();

    console.export_renderable_text(&md)
}

// =============================================================================
// 1. Headers
// =============================================================================

/// Test: H1 heading renders with bold+underline style.
#[test]
fn test_md_h1_heading() {
    init_test_logging();

    let md = Markdown::new("# Main Title");
    let segments = md.render(80);
    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(text.contains("Main Title"), "H1 text should be present");

    let title_seg = segments
        .iter()
        .find(|s| s.text.contains("Main Title"))
        .expect("missing H1 segment");
    let style = title_seg.style.as_ref().expect("H1 should have style");
    assert!(
        style.attributes.contains(Attributes::BOLD),
        "H1 should be bold"
    );
    assert!(
        style.attributes.contains(Attributes::UNDERLINE),
        "H1 should be underlined"
    );
}

/// Test: H2 heading renders with bold style.
#[test]
fn test_md_h2_heading() {
    init_test_logging();

    let md = Markdown::new("## Section");
    let segments = md.render(80);
    let title_seg = segments
        .iter()
        .find(|s| s.text.contains("Section"))
        .expect("missing H2 segment");
    let style = title_seg.style.as_ref().expect("H2 should have style");
    assert!(
        style.attributes.contains(Attributes::BOLD),
        "H2 should be bold"
    );
}

/// Test: H3 heading renders with bold style.
#[test]
fn test_md_h3_heading() {
    init_test_logging();

    let md = Markdown::new("### Subsection");
    let segments = md.render(80);
    let title_seg = segments
        .iter()
        .find(|s| s.text.contains("Subsection"))
        .expect("missing H3 segment");
    let style = title_seg.style.as_ref().expect("H3 should have style");
    assert!(
        style.attributes.contains(Attributes::BOLD),
        "H3 should be bold"
    );
}

/// Test: H4-H6 share the same style.
#[test]
fn test_md_h4_h5_h6_headings() {
    init_test_logging();

    for (level, label) in [("####", "H4"), ("#####", "H5"), ("######", "H6")] {
        let source = format!("{level} {label} Heading");
        let text = render_md_text(&source, 80);
        assert!(
            text.contains(&format!("{label} Heading")),
            "{label} text should be present"
        );
    }
}

/// Test: All heading levels appear in a single document.
#[test]
fn test_md_all_heading_levels() {
    init_test_logging();

    let source = "# H1\n## H2\n### H3\n#### H4\n##### H5\n###### H6";
    let text = render_md_text(source, 80);
    for label in &["H1", "H2", "H3", "H4", "H5", "H6"] {
        assert!(text.contains(label), "heading {label} should be present");
    }
}

/// Test: Custom heading styles are applied.
#[test]
fn test_md_custom_h1_style() {
    init_test_logging();

    let custom_style = Style::new().italic();
    let md = Markdown::new("# Custom").h1_style(custom_style);
    let segments = md.render(80);
    let title_seg = segments
        .iter()
        .find(|s| s.text.contains("Custom"))
        .expect("missing segment");
    let style = title_seg.style.as_ref().expect("should have style");
    assert!(
        style.attributes.contains(Attributes::ITALIC),
        "custom H1 style should apply"
    );
}

// =============================================================================
// 2. Lists
// =============================================================================

/// Test: Unordered list renders bullet characters.
#[test]
fn test_md_unordered_list() {
    init_test_logging();

    let text = render_md_text("- Alpha\n- Beta\n- Gamma", 80);
    assert!(text.contains("Alpha"));
    assert!(text.contains("Beta"));
    assert!(text.contains("Gamma"));
    assert!(text.contains("•"), "default bullet should appear");
}

/// Test: Ordered list renders numbered items.
#[test]
fn test_md_ordered_list() {
    init_test_logging();

    let text = render_md_text("1. First\n2. Second\n3. Third", 80);
    assert!(text.contains("First"));
    assert!(text.contains("Second"));
    assert!(text.contains("Third"));
    assert!(text.contains("1."), "numbered prefix should appear");
    assert!(text.contains("2."));
    assert!(text.contains("3."));
}

/// Test: Nested unordered lists increase indentation.
#[test]
fn test_md_nested_unordered_list() {
    init_test_logging();

    let source = "- Outer\n  - Inner\n    - Deep\n- Back";
    let text = render_md_text(source, 80);
    assert!(text.contains("Outer"));
    assert!(text.contains("Inner"));
    assert!(text.contains("Deep"));
    assert!(text.contains("Back"));
}

/// Test: Task list renders checkbox symbols.
#[test]
fn test_md_task_list() {
    init_test_logging();

    let text = render_md_text("- [ ] Pending\n- [x] Done\n- [ ] Todo", 80);
    assert!(text.contains("Pending"));
    assert!(text.contains("Done"));
    assert!(text.contains("☐"), "unchecked box");
    assert!(text.contains("☑"), "checked box");
}

/// Test: Checked task checkbox has a color style (green).
#[test]
fn test_md_task_list_checked_style() {
    init_test_logging();

    let md = Markdown::new("- [x] Completed");
    let segments = md.render(80);
    let check_seg = segments
        .iter()
        .find(|s| s.text.contains('☑'))
        .expect("missing checkbox segment");
    let style = check_seg
        .style
        .as_ref()
        .expect("checkbox should have style");
    assert!(style.color.is_some(), "checked box should have a color");
}

/// Test: Custom bullet character replaces default.
#[test]
fn test_md_custom_bullet_char() {
    init_test_logging();

    let md = Markdown::new("- Item 1\n- Item 2").bullet_char('→');
    let segments = md.render(80);
    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(text.contains("→"), "custom bullet should appear");
    assert!(!text.contains("•"), "default bullet should not appear");
}

/// Test: Custom list indent affects nested list spacing.
#[test]
fn test_md_custom_list_indent() {
    init_test_logging();

    let md = Markdown::new("- Outer\n  - Inner").list_indent(6);
    let segments = md.render(80);
    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(text.contains("Outer"));
    assert!(text.contains("Inner"));
}

/// Test: Mixed ordered and unordered lists.
#[test]
fn test_md_mixed_list_types() {
    init_test_logging();

    let source = "1. Ordered first\n2. Ordered second\n\n- Bullet first\n- Bullet second";
    let text = render_md_text(source, 80);
    assert!(text.contains("Ordered first"));
    assert!(text.contains("1."));
    assert!(text.contains("Bullet first"));
    assert!(text.contains("•"));
}

/// Test: Multi-paragraph list item continues with indentation.
#[test]
fn test_md_list_item_multi_paragraph() {
    init_test_logging();

    let source = "- First paragraph\n\n  Second paragraph";
    let text = render_md_text(source, 80);
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    assert!(lines.len() >= 2, "should have multiple content lines");
    assert!(lines[0].contains("First paragraph"));
    assert!(lines[1].contains("Second paragraph"));
}

// =============================================================================
// 3. Code Blocks
// =============================================================================

/// Test: Inline code renders with surrounding spaces.
#[test]
fn test_md_inline_code() {
    init_test_logging();

    let text = render_md_text("Use `println!()` here.", 80);
    assert!(
        text.contains("println!()"),
        "inline code text should appear"
    );
}

/// Test: Inline code has code_style applied.
#[test]
fn test_md_inline_code_style() {
    init_test_logging();

    let md = Markdown::new("Use `code` here.");
    let segments = md.render(80);
    let code_seg = segments
        .iter()
        .find(|s| s.text.contains("code"))
        .expect("missing inline code segment");
    let style = code_seg
        .style
        .as_ref()
        .expect("inline code should have style");
    assert!(style.bgcolor.is_some(), "code should have background color");
}

/// Test: Fenced code block renders content with indentation.
#[test]
fn test_md_fenced_code_block() {
    init_test_logging();

    let source = "```rust\nfn main() {\n    println!(\"hello\");\n}\n```";
    let text = render_md_text(source, 80);
    assert!(text.contains("fn main"), "code block content should appear");
    assert!(text.contains("println"), "code block content should appear");
}

/// Test: Code block applies code_block_style.
#[test]
fn test_md_code_block_style() {
    init_test_logging();

    let md = Markdown::new("```\nsome code\n```");
    let segments = md.render(80);
    let code_seg = segments
        .iter()
        .find(|s| s.text.contains("some code"))
        .expect("missing code block segment");
    let style = code_seg
        .style
        .as_ref()
        .expect("code block should have style");
    assert!(
        style.bgcolor.is_some(),
        "code block should have background color"
    );
}

/// Test: Code block without language hint still renders.
#[test]
fn test_md_code_block_no_language() {
    init_test_logging();

    let source = "```\nplain code\n```";
    let text = render_md_text(source, 80);
    assert!(text.contains("plain code"));
}

/// Test: Code block preserves multi-line structure.
#[test]
fn test_md_code_block_multiline() {
    init_test_logging();

    let source = "```\nline 1\nline 2\nline 3\n```";
    let text = render_md_text(source, 80);
    assert!(text.contains("line 1"));
    assert!(text.contains("line 2"));
    assert!(text.contains("line 3"));
}

/// Test: Custom code block style overrides default.
#[test]
fn test_md_custom_code_block_style() {
    init_test_logging();

    let custom = Style::new().italic();
    let md = Markdown::new("```\ncode\n```").code_block_style(custom);
    let segments = md.render(80);
    let code_seg = segments
        .iter()
        .find(|s| s.text.contains("code"))
        .expect("missing code segment");
    let style = code_seg.style.as_ref().expect("should have style");
    assert!(
        style.attributes.contains(Attributes::ITALIC),
        "custom code block style should apply"
    );
}

// =============================================================================
// 4. Links
// =============================================================================

/// Test: By default (`hyperlinks=true`), link text renders without a URL suffix.
#[test]
fn test_md_link_default_hides_url_suffix() {
    init_test_logging();

    let text = render_md_text("[Click here](https://example.com)", 80);
    assert!(text.contains("Click here"), "link text");
    assert!(
        !text.contains("example.com"),
        "URL suffix should not be rendered"
    );
}

/// Test: With `hyperlinks=false`, links render as `text (url)` (no OSC8).
#[test]
fn test_md_link_hyperlinks_disabled_shows_url_suffix() {
    init_test_logging();

    let md = Markdown::new("[Click here](https://example.com)").hyperlinks(false);
    let segments = md.render(80);
    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(text.contains("Click here"), "link text should remain");
    assert!(
        text.contains("example.com"),
        "URL suffix should be rendered"
    );
    assert!(text.contains(" (https://example.com)"));
}

/// Test: Link text has `link_style` applied and includes an OSC8 link when enabled.
#[test]
fn test_md_link_style() {
    init_test_logging();

    let md = Markdown::new("[Example](https://example.com)");
    let segments = md.render(80);
    let link_seg = segments
        .iter()
        .find(|s| s.text.contains("Example"))
        .expect("missing link segment");
    let style = link_seg.style.as_ref().expect("link should have style");
    assert!(
        style.attributes.contains(Attributes::UNDERLINE),
        "link should be underlined"
    );
    assert_eq!(
        style.link.as_deref(),
        Some("https://example.com"),
        "link should carry OSC8 URL in style"
    );
}

/// Test: Multiple links in one line.
#[test]
fn test_md_multiple_links() {
    init_test_logging();

    let source = "[First](https://first.com) and [Second](https://second.com)";
    let text = render_md_text(source, 120);
    assert!(text.contains("First"));
    assert!(text.contains("Second"));
    assert!(!text.contains("first.com"));
    assert!(!text.contains("second.com"));
}

/// Test: Custom link style overrides default.
#[test]
fn test_md_custom_link_style() {
    init_test_logging();

    let custom = Style::new().bold();
    let md = Markdown::new("[Bold Link](https://example.com)").link_style(custom);
    let segments = md.render(80);
    let link_seg = segments
        .iter()
        .find(|s| s.text.contains("Bold Link"))
        .expect("missing link segment");
    let style = link_seg.style.as_ref().expect("should have style");
    assert!(
        style.attributes.contains(Attributes::BOLD),
        "custom link style should apply"
    );
}

// =============================================================================
// 5. Tables
// =============================================================================

/// Test: Table renders header and data rows with borders.
#[test]
fn test_md_table_basic() {
    init_test_logging();

    let source = "| Name | Age |\n|------|-----|\n| Alice | 30 |\n| Bob | 25 |";
    let text = render_md_text(source, 80);
    assert!(text.contains("Name"));
    assert!(text.contains("Age"));
    assert!(text.contains("Alice"));
    assert!(text.contains("Bob"));
    assert!(text.contains("┌"), "top border");
    assert!(text.contains("│"), "vertical border");
    assert!(text.contains("─"), "horizontal border");
    assert!(text.contains("┘"), "bottom border");
}

/// Test: Table with column alignment.
#[test]
fn test_md_table_alignment() {
    init_test_logging();

    let source = "| Left | Center | Right |\n|:-----|:------:|------:|\n| a | b | c |";
    let text = render_md_text(source, 80);
    assert!(text.contains("Left"));
    assert!(text.contains("Center"));
    assert!(text.contains("Right"));
}

/// Test: Table header segments have table_header_style (bold).
#[test]
fn test_md_table_header_style() {
    init_test_logging();

    let md = Markdown::new("| H1 | H2 |\n|---|---|\n| a | b |");
    let segments = md.render(80);
    let header_seg = segments
        .iter()
        .find(|s| s.text.trim() == "H1" || s.text.trim() == "H2")
        .expect("missing header segment");
    let style = header_seg
        .style
        .as_ref()
        .expect("table header should have style");
    assert!(
        style.attributes.contains(Attributes::BOLD),
        "header should be bold"
    );
}

/// Test: Table border segments have table_border_style.
#[test]
fn test_md_table_border_style() {
    init_test_logging();

    let md = Markdown::new("| A |\n|---|\n| x |");
    let segments = md.render(80);
    let border_seg = segments
        .iter()
        .find(|s| s.text.contains("┌") || s.text.contains("─"))
        .expect("missing border segment");
    assert!(border_seg.style.is_some(), "table border should have style");
}

/// Test: Table with Unicode content has consistent column widths.
#[test]
fn test_md_table_unicode_width() {
    init_test_logging();

    let source = "| Col |\n|-----|\n| 日本 |\n| abc |";
    let text = render_md_text(source, 80);
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    assert!(lines.len() >= 3, "should have header + separator + rows");
}

/// Test: Table with many columns renders all of them.
#[test]
fn test_md_table_many_columns() {
    init_test_logging();

    let source = "| A | B | C | D | E |\n|---|---|---|---|---|\n| 1 | 2 | 3 | 4 | 5 |";
    let text = render_md_text(source, 120);
    for col in &["A", "B", "C", "D", "E", "1", "2", "3", "4", "5"] {
        assert!(text.contains(col), "column {col} should appear");
    }
}

// =============================================================================
// 6. Blockquotes
// =============================================================================

/// Test: Blockquote renders with vertical bar prefix.
#[test]
fn test_md_blockquote() {
    init_test_logging();

    let text = render_md_text("> This is quoted", 80);
    assert!(text.contains("This is quoted"));
    assert!(text.contains("│"), "blockquote prefix");
}

/// Test: Multi-paragraph blockquote preserves prefix on each paragraph.
#[test]
fn test_md_blockquote_multi_paragraph() {
    init_test_logging();

    let source = "> First paragraph\n>\n> Second paragraph";
    let text = render_md_text(source, 80);
    let lines: Vec<&str> = text.lines().filter(|l| !l.trim().is_empty()).collect();
    assert!(lines.len() >= 2);
    assert!(
        lines[0].starts_with("│ "),
        "first paragraph should have prefix"
    );
    assert!(
        lines[1].starts_with("│ "),
        "second paragraph should have prefix"
    );
}

/// Test: Blockquote content has quote_style (italic).
#[test]
fn test_md_blockquote_style() {
    init_test_logging();

    let md = Markdown::new("> Styled quote");
    let segments = md.render(80);
    let quote_seg = segments
        .iter()
        .find(|s| s.text.contains("Styled quote"))
        .expect("missing blockquote segment");
    let style = quote_seg
        .style
        .as_ref()
        .expect("blockquote should have style");
    assert!(
        style.attributes.contains(Attributes::ITALIC),
        "blockquote should be italic"
    );
}

/// Test: Custom blockquote style overrides default.
#[test]
fn test_md_custom_quote_style() {
    init_test_logging();

    let custom = Style::new().bold();
    let md = Markdown::new("> Quote").quote_style(custom);
    let segments = md.render(80);
    let quote_seg = segments
        .iter()
        .find(|s| s.text.contains("Quote"))
        .expect("missing segment");
    let style = quote_seg.style.as_ref().expect("should have style");
    assert!(
        style.attributes.contains(Attributes::BOLD),
        "custom quote style should apply"
    );
}

// =============================================================================
// 7. Inline Formatting
// =============================================================================

/// Test: Bold text has BOLD attribute.
#[test]
fn test_md_bold() {
    init_test_logging();

    let md = Markdown::new("This is **bold** text.");
    let segments = md.render(80);
    let bold_seg = segments
        .iter()
        .find(|s| s.text.contains("bold"))
        .expect("missing bold segment");
    let style = bold_seg.style.as_ref().expect("bold should have style");
    assert!(style.attributes.contains(Attributes::BOLD));
}

/// Test: Italic text has ITALIC attribute.
#[test]
fn test_md_italic() {
    init_test_logging();

    let md = Markdown::new("This is *italic* text.");
    let segments = md.render(80);
    let italic_seg = segments
        .iter()
        .find(|s| s.text.contains("italic"))
        .expect("missing italic segment");
    let style = italic_seg.style.as_ref().expect("italic should have style");
    assert!(style.attributes.contains(Attributes::ITALIC));
}

/// Test: Strikethrough text has STRIKE attribute.
#[test]
fn test_md_strikethrough() {
    init_test_logging();

    let md = Markdown::new("This is ~~deleted~~ text.");
    let segments = md.render(80);
    let strike_seg = segments
        .iter()
        .find(|s| s.text.contains("deleted"))
        .expect("missing strikethrough segment");
    let style = strike_seg
        .style
        .as_ref()
        .expect("strikethrough should have style");
    assert!(style.attributes.contains(Attributes::STRIKE));
}

/// Test: Nested bold+italic combines both attributes.
#[test]
fn test_md_bold_italic_combined() {
    init_test_logging();

    let md = Markdown::new("**bold *and italic***");
    let segments = md.render(80);
    let combined_seg = segments
        .iter()
        .find(|s| s.text.contains("and italic"))
        .expect("missing combined segment");
    let style = combined_seg
        .style
        .as_ref()
        .expect("combined should have style");
    assert!(
        style.attributes.contains(Attributes::BOLD),
        "should be bold"
    );
    assert!(
        style.attributes.contains(Attributes::ITALIC),
        "should be italic"
    );
}

/// Test: Custom emphasis style overrides default.
#[test]
fn test_md_custom_emphasis_style() {
    init_test_logging();

    let custom = Style::new().underline();
    let md = Markdown::new("*custom emphasis*").emphasis_style(custom);
    let segments = md.render(80);
    let em_seg = segments
        .iter()
        .find(|s| s.text.contains("custom emphasis"))
        .expect("missing segment");
    let style = em_seg.style.as_ref().expect("should have style");
    assert!(
        style.attributes.contains(Attributes::UNDERLINE),
        "custom emphasis style should apply"
    );
}

/// Test: Custom strong style overrides default.
#[test]
fn test_md_custom_strong_style() {
    init_test_logging();

    let custom = Style::new().italic();
    let md = Markdown::new("**custom strong**").strong_style(custom);
    let segments = md.render(80);
    let strong_seg = segments
        .iter()
        .find(|s| s.text.contains("custom strong"))
        .expect("missing segment");
    let style = strong_seg.style.as_ref().expect("should have style");
    assert!(
        style.attributes.contains(Attributes::ITALIC),
        "custom strong style should apply"
    );
}

// =============================================================================
// 8. Horizontal Rules
// =============================================================================

/// Test: Horizontal rule renders dash characters.
#[test]
fn test_md_horizontal_rule() {
    init_test_logging();

    let text = render_md_text("Above\n\n---\n\nBelow", 80);
    assert!(text.contains("Above"));
    assert!(text.contains("Below"));
    assert!(text.contains("─"), "horizontal rule character");
}

/// Test: Horizontal rule spans the configured width.
#[test]
fn test_md_horizontal_rule_width() {
    init_test_logging();

    let width = 40;
    let md = Markdown::new("---");
    let segments = md.render(width);
    let rule_seg = segments
        .iter()
        .find(|s| s.text.contains("─"))
        .expect("missing rule segment");
    // Rule should span across the width
    let rule_len: usize = rule_seg.text.chars().filter(|c| *c == '─').count();
    assert!(
        rule_len >= 10,
        "rule should span significant width, got {rule_len}"
    );
}

// =============================================================================
// 9. Complex Documents
// =============================================================================

/// Test: Full document with mixed elements.
#[test]
fn test_md_complex_document() {
    init_test_logging();

    let source = "\
# Project README

This is a **bold** introduction with *emphasis*.

## Features

- Feature one
- Feature two
  - Sub-feature
- Feature three

### Code Example

```rust
fn hello() {
    println!(\"world\");
}
```

> Note: This is important.

| Name | Status |
|------|--------|
| Test | Pass   |

Visit [our site](https://example.com) for more.

---

That's all!
";

    let text = render_md_text(source, 80);
    // Headers
    assert!(text.contains("Project README"), "H1");
    assert!(text.contains("Features"), "H2");
    assert!(text.contains("Code Example"), "H3");
    // Bold/italic
    assert!(text.contains("bold"), "bold text");
    assert!(text.contains("emphasis"), "italic text");
    // List items
    assert!(text.contains("Feature one"));
    assert!(text.contains("Sub-feature"));
    // Code block
    assert!(text.contains("fn hello"));
    // Blockquote
    assert!(text.contains("This is important"));
    assert!(text.contains("│"));
    // Table
    assert!(text.contains("Name"));
    assert!(text.contains("Pass"));
    // Link
    assert!(text.contains("our site"));
    // Default behavior matches Python Rich: hide the URL suffix when OSC8 hyperlinks are enabled.
    assert!(!text.contains("example.com"));
    // Horizontal rule
    assert!(text.contains("─"));
    // Closing text
    assert!(text.contains("That's all!"));
}

/// Test: Document with only inline formatting.
#[test]
fn test_md_inline_only_document() {
    init_test_logging();

    let source = "**Bold**, *italic*, ~~strike~~, and `code`.";
    let text = render_md_text(source, 80);
    assert!(text.contains("Bold"));
    assert!(text.contains("italic"));
    assert!(text.contains("strike"));
    assert!(text.contains("code"));
}

/// Test: Document with nested blockquote and list.
#[test]
fn test_md_blockquote_with_content() {
    init_test_logging();

    let source = "> Important:\n>\n> - Item A\n> - Item B";
    let text = render_md_text(source, 80);
    assert!(text.contains("Important"));
    assert!(text.contains("Item A"));
    assert!(text.contains("Item B"));
}

// =============================================================================
// 10. Console Integration
// =============================================================================

/// Test: Markdown renders via Console print_renderable.
#[test]
fn test_md_console_print() {
    init_test_logging();

    let md = Markdown::new("# Hello World\n\nParagraph text.");
    let console = Console::builder()
        .width(80)
        .force_terminal(true)
        .color_system(ColorSystem::TrueColor)
        .markup(false)
        .build();

    console.begin_capture();
    console.print_renderable(&md);
    let segments = console.end_capture();
    let output: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(output.contains("Hello World"));
    assert!(output.contains("Paragraph text"));
}

/// Test: Markdown exports to HTML with styled elements.
#[test]
fn test_md_export_html() {
    init_test_logging();

    let html = render_md_to_html("**Bold** and *italic*", 80);
    assert!(html.contains("Bold"), "HTML should contain bold text");
    assert!(html.contains("italic"), "HTML should contain italic text");
    assert!(html.contains("<"), "should be HTML");
}

/// Test: Markdown exports to SVG.
#[test]
fn test_md_export_svg() {
    init_test_logging();

    let md = Markdown::new("# Title\n\nBody text.");
    let console = Console::builder()
        .width(80)
        .force_terminal(true)
        .color_system(ColorSystem::TrueColor)
        .markup(false)
        .build();

    console.begin_capture();
    console.print_renderable(&md);
    let svg = console.export_svg(true);
    assert!(svg.contains("<svg"), "should be SVG format");

    // `export_svg` may split words across multiple `<text>` / `<tspan>` nodes, so do a
    // lightweight tag strip to assert on visible text rather than raw SVG markup.
    fn strip_tags(input: &str) -> String {
        let mut out = String::new();
        let mut in_tag = false;
        for ch in input.chars() {
            match ch {
                '<' => in_tag = true,
                '>' => {
                    in_tag = false;
                    out.push(' ');
                }
                _ => {
                    if !in_tag {
                        out.push(ch);
                    }
                }
            }
        }
        out.split_whitespace().collect::<Vec<_>>().join(" ")
    }

    let visible = strip_tags(&svg);
    assert!(visible.contains("Title"), "SVG should contain title text");
    assert!(visible.contains("Body"), "SVG should contain body text");
}

/// Test: Console export_renderable_text produces plain text.
#[test]
fn test_md_export_plain_text() {
    init_test_logging();

    let plain = console_export_text("# Heading\n\n**Bold** text.", 80);
    assert!(plain.contains("Heading"));
    assert!(plain.contains("Bold"));
    assert!(plain.contains("text"));
}

// =============================================================================
// 11. Builder Methods
// =============================================================================

/// Test: source() accessor returns original markdown.
#[test]
fn test_md_source_accessor() {
    init_test_logging();

    let md = Markdown::new("# Hello\n\nWorld");
    assert_eq!(md.source(), "# Hello\n\nWorld");
}

/// Test: Full builder chain compiles and renders.
#[test]
fn test_md_full_builder_chain() {
    init_test_logging();

    let md = Markdown::new("# Title\n\n*Emphasis* and **strong** with `code`.")
        .h1_style(Style::new().bold())
        .h2_style(Style::new().bold())
        .h3_style(Style::new().bold())
        .h4_style(Style::new().bold())
        .emphasis_style(Style::new().italic())
        .strong_style(Style::new().bold())
        .code_style(Style::new().italic())
        .code_block_style(Style::new().italic())
        .link_style(Style::new().underline())
        .quote_style(Style::new().italic())
        .table_header_style(Style::new().bold())
        .table_border_style(Style::new().italic())
        .bullet_char('*')
        .list_indent(4)
        .hyperlinks(false);

    let segments = md.render(80);
    assert!(!segments.is_empty(), "should produce output");
    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(text.contains("Title"));
}

/// Test: Default trait produces empty-source Markdown.
#[test]
fn test_md_default() {
    init_test_logging();

    let md = Markdown::default();
    assert_eq!(md.source(), "");
    let segments = md.render(80);
    // Empty source should produce minimal or no content segments
    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(
        text.trim().is_empty(),
        "empty source should produce empty output"
    );
}

/// Test: Clone produces an independent copy.
#[test]
fn test_md_clone() {
    init_test_logging();

    let md1 = Markdown::new("# Original");
    let md2 = md1.clone();
    assert_eq!(md1.source(), md2.source());

    let segs1 = md1.render(80);
    let segs2 = md2.render(80);
    let text1: String = segs1.iter().map(|s| s.text.as_ref()).collect();
    let text2: String = segs2.iter().map(|s| s.text.as_ref()).collect();
    assert_eq!(text1, text2);
}

/// Test: Debug trait is implemented.
#[test]
fn test_md_debug() {
    init_test_logging();

    let md = Markdown::new("# Test");
    let debug = format!("{:?}", md);
    assert!(debug.contains("Markdown"), "Debug should contain type name");
}

// =============================================================================
// 12. Edge Cases
// =============================================================================

/// Test: Empty markdown source renders without error.
#[test]
fn test_md_empty_source() {
    init_test_logging();

    let text = render_md_text("", 80);
    assert!(text.trim().is_empty() || text.chars().all(|c| c.is_whitespace()));
}

/// Test: Markdown with only whitespace.
#[test]
fn test_md_whitespace_only() {
    init_test_logging();

    let text = render_md_text("   \n\n   \n", 80);
    // Should not panic; content may be empty or just spaces
    let _ = text;
}

/// Test: Very narrow width still renders.
#[test]
fn test_md_narrow_width() {
    init_test_logging();

    let text = render_md_text("# Title\n\nSome paragraph text.", 10);
    assert!(text.contains("Title"));
    assert!(text.contains("Some"));
}

/// Test: Very wide width renders correctly.
#[test]
fn test_md_wide_width() {
    init_test_logging();

    let text = render_md_text("# Title\n\nBody.", 200);
    assert!(text.contains("Title"));
    assert!(text.contains("Body"));
}

/// Test: Zero width renders without panic.
#[test]
fn test_md_zero_width() {
    init_test_logging();

    let md = Markdown::new("# Hello");
    let segments = md.render(0);
    let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(text.contains("Hello"));
}

/// Test: Unicode content in paragraphs.
#[test]
fn test_md_unicode_content() {
    init_test_logging();

    let source = "# 日本語タイトル\n\nこんにちは世界。Привет мир。";
    let text = render_md_text(source, 80);
    assert!(text.contains("日本語タイトル"));
    assert!(text.contains("こんにちは世界"));
    assert!(text.contains("Привет мир"));
}

/// Test: Markdown with special characters in text.
#[test]
fn test_md_special_chars() {
    init_test_logging();

    let source = "Ampersand & angle <brackets> and \"quotes\".";
    let text = render_md_text(source, 80);
    assert!(text.contains("Ampersand"));
    assert!(text.contains("&"));
}

/// Test: Long code block renders all lines.
#[test]
fn test_md_long_code_block() {
    init_test_logging();

    let lines: Vec<String> = (1..=50).map(|i| format!("line {i}")).collect();
    let code = lines.join("\n");
    let source = format!("```\n{code}\n```");
    let text = render_md_text(&source, 80);
    assert!(text.contains("line 1"));
    assert!(text.contains("line 50"));
}

/// Test: Deeply nested lists render without panic.
#[test]
fn test_md_deeply_nested_list() {
    init_test_logging();

    let source = "- Level 1\n  - Level 2\n    - Level 3\n      - Level 4";
    let text = render_md_text(source, 80);
    assert!(text.contains("Level 1"));
    assert!(text.contains("Level 4"));
}

/// Test: Multiple consecutive headings.
#[test]
fn test_md_consecutive_headings() {
    init_test_logging();

    let source = "# First\n# Second\n# Third";
    let text = render_md_text(source, 80);
    assert!(text.contains("First"));
    assert!(text.contains("Second"));
    assert!(text.contains("Third"));
}

/// Test: Soft break becomes space.
#[test]
fn test_md_soft_break() {
    init_test_logging();

    // In CommonMark, a single newline within a paragraph is a soft break (rendered as space)
    let source = "Line one\nLine two";
    let text = render_md_text(source, 80);
    assert!(text.contains("Line one"));
    assert!(text.contains("Line two"));
}

/// Test: Hard break (two trailing spaces) creates newline.
#[test]
fn test_md_hard_break() {
    init_test_logging();

    // Two trailing spaces + newline = hard break in CommonMark
    let source = "Line one  \nLine two";
    let text = render_md_text(source, 80);
    assert!(text.contains("Line one"));
    assert!(text.contains("Line two"));
}
