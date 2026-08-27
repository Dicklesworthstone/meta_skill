//! End-to-end tests for Hyperlink (OSC 8) support.
//!
//! Verifies that hyperlinks are correctly stored, parsed, and rendered
//! according to the OSC 8 specification.

mod common;

use common::init_test_logging;
use rich_rust::color::ColorSystem;
use rich_rust::prelude::*;

// =============================================================================
// OSC 8 Format Reference
// =============================================================================
// ESC ] 8 ; params ; URI ST ... text ... ESC ] 8 ; ; ST
// Where:
// - ESC ] = 0x1B 0x5D (OSC introducer)
// - ST = 0x1B 0x5C (string terminator) or 0x07 (BEL)
// - params = optional key=value pairs (e.g., id=unique-id)
// - URI = the link URL

// =============================================================================
// Style::link() Tests
// =============================================================================

/// Test: Style::link() stores URL correctly
#[test]
fn test_style_link_stores_url() {
    init_test_logging();

    let style = Style::new().link("https://example.com");
    assert_eq!(style.link, Some("https://example.com".to_string()));
}

/// Test: Style::link() with complex URL
#[test]
fn test_style_link_complex_url() {
    init_test_logging();

    let url = "https://example.com/path?query=value&other=123#anchor";
    let style = Style::new().link(url);
    assert_eq!(style.link, Some(url.to_string()));
}

/// Test: Style::link() can be combined with other attributes
#[test]
fn test_style_link_with_attributes() {
    init_test_logging();

    let style = Style::new()
        .bold()
        .italic()
        .color(Color::parse("red").unwrap())
        .link("https://example.com");

    assert!(style.attributes.contains(Attributes::BOLD));
    assert!(style.attributes.contains(Attributes::ITALIC));
    assert!(style.color.is_some());
    assert_eq!(style.link, Some("https://example.com".to_string()));
}

// =============================================================================
// Style Parsing Tests
// =============================================================================

/// Test: Style::parse() handles "link URL" syntax
#[test]
fn test_style_parse_link_space_syntax() {
    init_test_logging();

    let style = Style::parse("link https://example.com").unwrap();
    assert_eq!(style.link, Some("https://example.com".to_string()));
}

/// Test: Style::parse() link with other attributes
#[test]
fn test_style_parse_link_with_attributes() {
    init_test_logging();

    let style = Style::parse("bold red link https://example.com").unwrap();
    assert!(style.attributes.contains(Attributes::BOLD));
    assert!(style.color.is_some());
    assert_eq!(style.link, Some("https://example.com".to_string()));
}

/// Test: Style::parse() link only (no other attributes)
#[test]
fn test_style_parse_link_only() {
    init_test_logging();

    let style = Style::parse("link https://rust-lang.org").unwrap();
    assert!(style.attributes.is_empty());
    assert!(style.color.is_none());
    assert_eq!(style.link, Some("https://rust-lang.org".to_string()));
}

// =============================================================================
// ANSI Rendering Tests
// =============================================================================

/// Test: Style with link renders OSC 8 open and close sequences
#[test]
fn test_style_link_renders_osc8() {
    init_test_logging();

    let style = Style::new().link("https://example.com");
    let rendered = style.render("click me", ColorSystem::TrueColor);

    // Check for OSC 8 open sequence
    assert!(
        rendered.contains("\x1b]8;;https://example.com\x1b\\"),
        "Should contain OSC 8 open sequence, got: {:?}",
        rendered
    );

    // Check for OSC 8 close sequence
    assert!(
        rendered.contains("\x1b]8;;\x1b\\"),
        "Should contain OSC 8 close sequence, got: {:?}",
        rendered
    );

    // Check text is present
    assert!(rendered.contains("click me"), "Should contain the text");
}

/// Test: Style with link and attributes renders both
#[test]
fn test_style_link_with_bold_renders_both() {
    init_test_logging();

    let style = Style::new().bold().link("https://example.com");
    let rendered = style.render("bold link", ColorSystem::TrueColor);

    // Check for OSC 8 sequences
    assert!(rendered.contains("\x1b]8;;https://example.com\x1b\\"));
    assert!(rendered.contains("\x1b]8;;\x1b\\"));

    // Check for bold (SGR 1)
    assert!(rendered.contains("\x1b[1m"), "Should contain bold SGR code");
}

/// Test: render_ansi returns correct prefix/suffix for links
#[test]
fn test_style_render_ansi_link_prefix_suffix() {
    init_test_logging();

    let style = Style::new().bold().link("https://example.com");
    let ansi = style.render_ansi(ColorSystem::TrueColor);
    let (prefix, suffix) = &*ansi;

    assert!(prefix.contains("\x1b]8;;https://example.com"));
    assert!(suffix.contains("\x1b]8;;\x1b\\"));
}

// =============================================================================
// Markup Rendering Tests
// =============================================================================

/// Test: Markup [link=URL]text[/link] renders correctly
#[test]
fn test_markup_link_tag() {
    init_test_logging();

    let text = rich_rust::markup::render("[link=https://example.com]click here[/link]").unwrap();
    assert_eq!(text.plain(), "click here");

    // Verify the span has a link
    let spans = text.spans();
    assert!(!spans.is_empty(), "Should have at least one span");
    assert!(
        spans.iter().any(|s| s.style.link.is_some()),
        "At least one span should have a link"
    );
}

/// Test: Markup link with nested styles
#[test]
fn test_markup_link_with_nested_styles() {
    init_test_logging();

    let text = rich_rust::markup::render("[bold][link=https://example.com]bold link[/link][/bold]")
        .unwrap();
    assert_eq!(text.plain(), "bold link");
}

/// Test: Markup link with special characters in URL
#[test]
fn test_markup_link_special_chars() {
    init_test_logging();

    let text =
        rich_rust::markup::render("[link=https://example.com/path?a=1&b=2]url[/link]").unwrap();
    assert_eq!(text.plain(), "url");

    let spans = text.spans();
    assert!(!spans.is_empty());
    let link_span = spans.iter().find(|s| s.style.link.is_some()).unwrap();
    assert_eq!(
        link_span.style.link.as_deref(),
        Some("https://example.com/path?a=1&b=2")
    );
}

/// Test: Markup link inside styled text
#[test]
fn test_markup_styled_link() {
    init_test_logging();

    let text = rich_rust::markup::render(
        "[red]before [link=https://example.com]linked[/link] after[/red]",
    )
    .unwrap();
    assert_eq!(text.plain(), "before linked after");
}

// =============================================================================
// Style Combination Tests
// =============================================================================

/// Test: Style combination preserves links
#[test]
fn test_style_combine_preserves_link() {
    init_test_logging();

    let style1 = Style::new().bold();
    let style2 = Style::new().link("https://example.com");

    let combined = style1.combine(&style2);

    assert!(combined.attributes.contains(Attributes::BOLD));
    assert_eq!(combined.link, Some("https://example.com".to_string()));
}

/// Test: Later link overrides earlier link
#[test]
fn test_style_combine_link_override() {
    init_test_logging();

    let style1 = Style::new().link("https://first.com");
    let style2 = Style::new().link("https://second.com");

    let combined = style1.combine(&style2);

    assert_eq!(combined.link, Some("https://second.com".to_string()));
}

/// Test: Null style doesn't override link
#[test]
fn test_style_combine_null_preserves_link() {
    init_test_logging();

    let style = Style::new().link("https://example.com");
    let null = Style::null();

    let combined = style.combine(&null);

    assert_eq!(combined.link, Some("https://example.com".to_string()));
}

// =============================================================================
// Display Tests
// =============================================================================

/// Test: Style with link displays correctly
#[test]
fn test_style_display_with_link() {
    init_test_logging();

    let style = Style::new().bold().link("https://example.com");
    let display = style.to_string();

    assert!(display.contains("bold"), "Display should contain 'bold'");
    assert!(display.contains("link"), "Display should contain 'link'");
    assert!(
        display.contains("https://example.com"),
        "Display should contain URL"
    );
}

// =============================================================================
// Edge Cases
// =============================================================================

/// Test: Empty URL is stored
#[test]
fn test_style_link_empty_url() {
    init_test_logging();

    let style = Style::new().link("");
    assert_eq!(style.link, Some("".to_string()));
}

/// Test: URL with unicode characters
#[test]
fn test_style_link_unicode_url() {
    init_test_logging();

    let url = "https://example.com/日本語";
    let style = Style::new().link(url);
    assert_eq!(style.link, Some(url.to_string()));
}

/// Test: Very long URL
#[test]
fn test_style_link_long_url() {
    init_test_logging();

    let long_path = "a".repeat(1000);
    let url = format!("https://example.com/{}", long_path);
    let style = Style::new().link(&url);
    assert_eq!(style.link, Some(url));
}

/// Test: Link with no scheme (relative URL)
#[test]
fn test_style_link_relative_url() {
    init_test_logging();

    let style = Style::new().link("/path/to/resource");
    assert_eq!(style.link, Some("/path/to/resource".to_string()));
}

/// Test: File URL scheme
#[test]
fn test_style_link_file_url() {
    init_test_logging();

    let style = Style::new().link("file:///home/user/doc.txt");
    assert_eq!(style.link, Some("file:///home/user/doc.txt".to_string()));
}

/// Test: Mailto URL scheme
#[test]
fn test_style_link_mailto_url() {
    init_test_logging();

    let style = Style::new().link("mailto:user@example.com");
    assert_eq!(style.link, Some("mailto:user@example.com".to_string()));
}
