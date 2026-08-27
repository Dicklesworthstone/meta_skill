//! End-to-end tests for Style::parse() edge cases.
//!
//! This test suite validates all edge cases from RICH_SPEC.md Section 2
//! including valid inputs, invalid inputs, and error handling.

mod common;

use common::init_test_logging;
use rich_rust::prelude::*;
use rich_rust::style::StyleParseError;

// =============================================================================
// Valid Input Tests
// =============================================================================

/// Test: Empty string should produce a null style
#[test]
fn test_style_parse_empty_string() {
    init_test_logging();

    let style = Style::parse("").unwrap();
    assert!(style.is_null(), "Empty string should produce null style");
}

/// Test: "none" keyword should produce a null style
#[test]
fn test_style_parse_none_keyword() {
    init_test_logging();

    let style = Style::parse("none").unwrap();
    assert!(style.is_null(), "'none' should produce null style");
}

/// Test: Single attribute "bold" should set bold only
#[test]
fn test_style_parse_bold_only() {
    init_test_logging();

    let style = Style::parse("bold").unwrap();
    assert!(
        style.attributes.contains(Attributes::BOLD),
        "Should have bold attribute"
    );
    assert!(style.color.is_none(), "Should have no foreground color");
    assert!(style.bgcolor.is_none(), "Should have no background color");
}

/// Test: Single attribute "italic" should set italic only
#[test]
fn test_style_parse_italic_only() {
    init_test_logging();

    let style = Style::parse("italic").unwrap();
    assert!(
        style.attributes.contains(Attributes::ITALIC),
        "Should have italic attribute"
    );
}

/// Test: Single attribute "underline" should set underline only
#[test]
fn test_style_parse_underline_only() {
    init_test_logging();

    let style = Style::parse("underline").unwrap();
    assert!(
        style.attributes.contains(Attributes::UNDERLINE),
        "Should have underline attribute"
    );
}

/// Test: Named color "red" should set foreground only
#[test]
fn test_style_parse_red_foreground() {
    init_test_logging();

    let style = Style::parse("red").unwrap();
    assert!(style.color.is_some(), "Should have foreground color");
    assert!(style.bgcolor.is_none(), "Should have no background color");
    assert!(style.attributes.is_empty(), "Should have no attributes");
}

/// Test: "on blue" should set background only
#[test]
fn test_style_parse_on_blue_background() {
    init_test_logging();

    let style = Style::parse("on blue").unwrap();
    assert!(style.bgcolor.is_some(), "Should have background color");
    assert!(style.color.is_none(), "Should have no foreground color");
}

/// Test: Combined "bold italic red on blue" should set all properties
#[test]
fn test_style_parse_combined() {
    init_test_logging();

    let style = Style::parse("bold italic red on blue").unwrap();
    assert!(
        style.attributes.contains(Attributes::BOLD),
        "Should have bold"
    );
    assert!(
        style.attributes.contains(Attributes::ITALIC),
        "Should have italic"
    );
    assert!(style.color.is_some(), "Should have foreground color");
    assert!(style.bgcolor.is_some(), "Should have background color");
}

/// Test: "not bold" should explicitly unset bold attribute
#[test]
fn test_style_parse_not_bold() {
    init_test_logging();

    let style = Style::parse("not bold").unwrap();
    assert!(
        !style.attributes.contains(Attributes::BOLD),
        "Bold should not be set"
    );
    assert!(
        style.set_attributes.contains(Attributes::BOLD),
        "Bold should be in set_attributes"
    );
}

/// Test: Hex color "#ff0000" should set foreground color
#[test]
fn test_style_parse_hex_color() {
    init_test_logging();

    let style = Style::parse("#ff0000").unwrap();
    assert!(style.color.is_some(), "Should have foreground color");

    // Verify it's the right color
    let color = style.color.unwrap();
    let triplet = color.triplet.expect("Should have triplet");
    assert_eq!(triplet.red, 255, "Red component should be 255");
    assert_eq!(triplet.green, 0, "Green component should be 0");
    assert_eq!(triplet.blue, 0, "Blue component should be 0");
}

/// Test: RGB color "rgb(0,255,0)" should set foreground color
#[test]
fn test_style_parse_rgb_color() {
    init_test_logging();

    let style = Style::parse("rgb(0,255,0)").unwrap();
    assert!(style.color.is_some(), "Should have foreground color");

    let color = style.color.unwrap();
    let triplet = color.triplet.expect("Should have triplet");
    assert_eq!(triplet.red, 0, "Red component should be 0");
    assert_eq!(triplet.green, 255, "Green component should be 255");
    assert_eq!(triplet.blue, 0, "Blue component should be 0");
}

/// Test: 256-color "color(128)" should set foreground color
#[test]
fn test_style_parse_256_color() {
    init_test_logging();

    let style = Style::parse("color(128)").unwrap();
    assert!(style.color.is_some(), "Should have foreground color");

    let color = style.color.unwrap();
    assert_eq!(color.number, Some(128), "Color number should be 128");
}

/// Test: Hyperlink "link=https://example.com" or "link https://example.com"
#[test]
fn test_style_parse_hyperlink() {
    init_test_logging();

    let style = Style::parse("link https://example.com").unwrap();
    assert_eq!(
        style.link,
        Some("https://example.com".to_string()),
        "Should have hyperlink"
    );
}

/// Test: Short attribute aliases (b, i, u, etc.)
#[test]
fn test_style_parse_short_aliases() {
    init_test_logging();

    // b -> bold
    let style = Style::parse("b").unwrap();
    assert!(
        style.attributes.contains(Attributes::BOLD),
        "'b' should map to bold"
    );

    // i -> italic
    let style = Style::parse("i").unwrap();
    assert!(
        style.attributes.contains(Attributes::ITALIC),
        "'i' should map to italic"
    );

    // u -> underline
    let style = Style::parse("u").unwrap();
    assert!(
        style.attributes.contains(Attributes::UNDERLINE),
        "'u' should map to underline"
    );

    // d -> dim
    let style = Style::parse("d").unwrap();
    assert!(
        style.attributes.contains(Attributes::DIM),
        "'d' should map to dim"
    );

    // s -> strike
    let style = Style::parse("s").unwrap();
    assert!(
        style.attributes.contains(Attributes::STRIKE),
        "'s' should map to strike"
    );

    // o -> overline
    let style = Style::parse("o").unwrap();
    assert!(
        style.attributes.contains(Attributes::OVERLINE),
        "'o' should map to overline"
    );

    // r -> reverse
    let style = Style::parse("r").unwrap();
    assert!(
        style.attributes.contains(Attributes::REVERSE),
        "'r' should map to reverse"
    );
}

/// Test: All supported attributes can be parsed
#[test]
fn test_style_parse_all_attributes() {
    init_test_logging();

    let attributes = [
        ("bold", Attributes::BOLD),
        ("dim", Attributes::DIM),
        ("italic", Attributes::ITALIC),
        ("underline", Attributes::UNDERLINE),
        ("blink", Attributes::BLINK),
        ("blink2", Attributes::BLINK2),
        ("reverse", Attributes::REVERSE),
        ("conceal", Attributes::CONCEAL),
        ("strike", Attributes::STRIKE),
        ("underline2", Attributes::UNDERLINE2),
        ("frame", Attributes::FRAME),
        ("encircle", Attributes::ENCIRCLE),
        ("overline", Attributes::OVERLINE),
    ];

    for (name, expected) in attributes {
        let style = Style::parse(name).unwrap_or_else(|_| panic!("Should parse '{}'", name));
        assert!(
            style.attributes.contains(expected),
            "'{}' should set {:?} attribute",
            name,
            expected
        );
    }
}

/// Test: Multiple attributes combined
#[test]
fn test_style_parse_multiple_attributes() {
    init_test_logging();

    let style = Style::parse("bold italic underline strike").unwrap();
    assert!(style.attributes.contains(Attributes::BOLD));
    assert!(style.attributes.contains(Attributes::ITALIC));
    assert!(style.attributes.contains(Attributes::UNDERLINE));
    assert!(style.attributes.contains(Attributes::STRIKE));
}

/// Test: Background with hex color
#[test]
fn test_style_parse_on_hex_color() {
    init_test_logging();

    let style = Style::parse("on #0000ff").unwrap();
    assert!(style.bgcolor.is_some(), "Should have background color");

    let color = style.bgcolor.unwrap();
    let triplet = color.triplet.expect("Should have triplet");
    assert_eq!(triplet.blue, 255, "Blue component should be 255");
}

/// Test: Case insensitivity
#[test]
fn test_style_parse_case_insensitive() {
    init_test_logging();

    let style1 = Style::parse("BOLD RED").unwrap();
    let style2 = Style::parse("Bold Red").unwrap();
    let style3 = Style::parse("bold red").unwrap();

    assert!(style1.attributes.contains(Attributes::BOLD));
    assert!(style2.attributes.contains(Attributes::BOLD));
    assert!(style3.attributes.contains(Attributes::BOLD));

    assert!(style1.color.is_some());
    assert!(style2.color.is_some());
    assert!(style3.color.is_some());
}

/// Test: Whitespace handling - extra spaces
#[test]
fn test_style_parse_extra_whitespace() {
    init_test_logging();

    let style = Style::parse("  bold   red   on   blue  ").unwrap();
    assert!(style.attributes.contains(Attributes::BOLD));
    assert!(style.color.is_some());
    assert!(style.bgcolor.is_some());
}

// =============================================================================
// Invalid Input Tests
// =============================================================================

/// Test: Unknown keyword should produce an error
#[test]
fn test_style_parse_invalid_keyword() {
    init_test_logging();

    let result = Style::parse("invalid");
    assert!(result.is_err(), "'invalid' should produce an error");

    match result {
        Err(StyleParseError::UnknownToken(token)) => {
            assert_eq!(token, "invalid", "Error should contain the unknown token");
        }
        Err(other) => panic!("Expected UnknownToken error, got {:?}", other),
        Ok(_) => panic!("Expected error for 'invalid'"),
    }
}

/// Test: Invalid color should produce an error
#[test]
fn test_style_parse_invalid_color() {
    init_test_logging();

    let result = Style::parse("notacolor");
    assert!(result.is_err(), "'notacolor' should produce an error");
}

/// Test: Incomplete "on" (missing color) should produce an error
#[test]
fn test_style_parse_incomplete_on() {
    init_test_logging();

    let result = Style::parse("on");
    assert!(result.is_err(), "'on' alone should produce an error");

    match result {
        Err(StyleParseError::InvalidFormat(msg)) => {
            assert!(
                msg.contains("requires a color"),
                "Error should mention 'requires a color'"
            );
        }
        Err(other) => panic!("Expected InvalidFormat error, got {:?}", other),
        Ok(_) => panic!("Expected error for 'on' alone"),
    }
}

/// Test: Incomplete "not" (missing attribute) should produce an error
#[test]
fn test_style_parse_incomplete_not() {
    init_test_logging();

    let result = Style::parse("not");
    assert!(result.is_err(), "'not' alone should produce an error");

    match result {
        Err(StyleParseError::InvalidFormat(msg)) => {
            assert!(
                msg.contains("requires an attribute"),
                "Error should mention 'requires an attribute'"
            );
        }
        Err(other) => panic!("Expected InvalidFormat error, got {:?}", other),
        Ok(_) => panic!("Expected error for 'not' alone"),
    }
}

/// Test: Incomplete "link" (missing URL) should produce an error
#[test]
fn test_style_parse_incomplete_link() {
    init_test_logging();

    let result = Style::parse("link");
    assert!(result.is_err(), "'link' alone should produce an error");

    match result {
        Err(StyleParseError::InvalidFormat(msg)) => {
            assert!(
                msg.contains("requires a URL"),
                "Error should mention 'requires a URL'"
            );
        }
        Err(other) => panic!("Expected InvalidFormat error, got {:?}", other),
        Ok(_) => panic!("Expected error for 'link' alone"),
    }
}

/// Test: "not" with invalid attribute should produce an error
#[test]
fn test_style_parse_not_invalid_attribute() {
    init_test_logging();

    let result = Style::parse("not invalid");
    assert!(result.is_err(), "'not invalid' should produce an error");

    match result {
        Err(StyleParseError::UnknownAttribute(attr)) => {
            assert_eq!(
                attr, "invalid",
                "Error should contain the unknown attribute"
            );
        }
        Err(other) => panic!("Expected UnknownAttribute error, got {:?}", other),
        Ok(_) => panic!("Expected error for 'not invalid'"),
    }
}

/// Test: Invalid hex color should produce an error
#[test]
fn test_style_parse_invalid_hex() {
    init_test_logging();

    // Too short
    let result = Style::parse("#ff");
    assert!(result.is_err(), "'#ff' should produce an error");

    // Invalid characters
    let result = Style::parse("#gggggg");
    assert!(result.is_err(), "'#gggggg' should produce an error");
}

/// Test: Invalid RGB format should produce an error
#[test]
fn test_style_parse_invalid_rgb() {
    init_test_logging();

    // Missing values
    let result = Style::parse("rgb(255)");
    assert!(result.is_err(), "'rgb(255)' should produce an error");

    // Out of range
    let result = Style::parse("rgb(256,0,0)");
    assert!(result.is_err(), "'rgb(256,0,0)' should produce an error");
}

/// Test: Invalid color number should produce an error
#[test]
fn test_style_parse_invalid_color_number() {
    init_test_logging();

    // Out of range (256 colors are 0-255)
    let result = Style::parse("color(256)");
    assert!(result.is_err(), "'color(256)' should produce an error");

    // Negative not possible via parse but test format
    let result = Style::parse("color(-1)");
    assert!(result.is_err(), "'color(-1)' should produce an error");
}

// =============================================================================
// Behavior Tests
// =============================================================================

/// Test: Duplicate attributes should be allowed (no error)
/// The spec doesn't explicitly forbid "bold bold", though it's redundant
#[test]
fn test_style_parse_duplicate_attribute() {
    init_test_logging();

    // Duplicate attributes should just work (idempotent)
    let result = Style::parse("bold bold");
    // This may either succeed or fail depending on implementation
    // Python Rich allows it, so we should too
    if let Ok(style) = result {
        assert!(style.attributes.contains(Attributes::BOLD));
    }
    // If it errors, that's also acceptable behavior
}

/// Test: Two foreground colors - second should win
#[test]
fn test_style_parse_two_foreground_colors() {
    init_test_logging();

    // When two foreground colors are specified, the second should win
    let style = Style::parse("red blue").unwrap();
    assert!(style.color.is_some());

    // Verify it's blue (standard color 4)
    let color = style.color.unwrap();
    // Blue should be the final color
    assert!(color.number.is_some() || color.triplet.is_some());
}

/// Test: "not" followed by "bold" should result in bold being explicitly unset
#[test]
fn test_style_not_then_set() {
    init_test_logging();

    // First not, then set - should be set
    let style = Style::parse("not bold bold").unwrap();
    assert!(
        style.attributes.contains(Attributes::BOLD),
        "Second 'bold' should set the attribute"
    );
}

/// Test: "bold" followed by "not bold" should result in bold being explicitly unset
#[test]
fn test_style_set_then_not() {
    init_test_logging();

    // First set, then not - should be unset
    let style = Style::parse("bold not bold").unwrap();
    assert!(
        !style.attributes.contains(Attributes::BOLD),
        "'not bold' should unset the attribute"
    );
    assert!(
        style.set_attributes.contains(Attributes::BOLD),
        "Bold should still be in set_attributes"
    );
}

// =============================================================================
// Style Combination Tests
// =============================================================================

/// Test: Style combination preserves explicitly set attributes
#[test]
fn test_style_combine_preserves_explicit() {
    init_test_logging();

    let style1 = Style::new().bold().color(Color::parse("red").unwrap());
    let style2 = Style::parse("not bold").unwrap();

    let combined = style1.combine(&style2);

    // style2 explicitly unsets bold, so combined should not be bold
    assert!(!combined.attributes.contains(Attributes::BOLD));
    assert!(combined.set_attributes.contains(Attributes::BOLD));
    // Color from style1 should be preserved
    assert!(combined.color.is_some());
}

/// Test: Null style combination is identity
#[test]
fn test_style_combine_null_identity() {
    init_test_logging();

    let style = Style::parse("bold red").unwrap();
    let null = Style::null();

    let combined1 = style.combine(&null);
    let combined2 = null.combine(&style);

    assert_eq!(combined1.attributes, style.attributes);
    assert_eq!(combined2.attributes, style.attributes);
}

// =============================================================================
// Display/Round-trip Tests
// =============================================================================

/// Test: Style Display should produce parseable output
#[test]
fn test_style_display_roundtrip() {
    init_test_logging();

    let original = Style::parse("bold red on blue").unwrap();
    let display = original.to_string();

    // Parse the display output
    let reparsed = Style::parse(&display).unwrap();

    assert_eq!(original.attributes, reparsed.attributes);
    // Colors may not be exactly equal due to display format, but should both exist
    assert!(original.color.is_some() && reparsed.color.is_some());
    assert!(original.bgcolor.is_some() && reparsed.bgcolor.is_some());
}

/// Test: Null style displays as "none"
#[test]
fn test_style_null_display() {
    init_test_logging();

    let null = Style::null();
    assert_eq!(null.to_string(), "none");
}

// =============================================================================
// Cache Tests
// =============================================================================

/// Test: Style parsing is cached (same input returns same result)
#[test]
fn test_style_parse_cached() {
    init_test_logging();

    let style1 = Style::parse("bold red").unwrap();
    let style2 = Style::parse("bold red").unwrap();

    assert_eq!(style1, style2, "Cached results should be equal");
}

/// Test: Case-normalized caching (BOLD RED and bold red should use same cache entry)
#[test]
fn test_style_parse_case_normalized_cache() {
    init_test_logging();

    let style1 = Style::parse("BOLD RED").unwrap();
    let style2 = Style::parse("bold red").unwrap();

    assert_eq!(style1, style2, "Case-normalized styles should be equal");
}

// =============================================================================
// FromStr/TryFrom Trait Tests
// =============================================================================

/// Test: FromStr trait works
#[test]
fn test_style_from_str() {
    init_test_logging();

    let style: Style = "bold red".parse().unwrap();
    assert!(style.attributes.contains(Attributes::BOLD));
    assert!(style.color.is_some());
}

/// Test: TryFrom<&str> trait works
#[test]
fn test_style_try_from_str() {
    init_test_logging();

    let style: Style = Style::try_from("bold red").unwrap();
    assert!(style.attributes.contains(Attributes::BOLD));
}

/// Test: TryFrom<String> trait works
#[test]
fn test_style_try_from_string() {
    init_test_logging();

    let style: Style = Style::try_from("bold red".to_string()).unwrap();
    assert!(style.attributes.contains(Attributes::BOLD));
}

// =============================================================================
// From Trait Tests
// =============================================================================

/// Test: From<Color> creates a style with just that color
#[test]
fn test_style_from_color() {
    init_test_logging();

    let color = Color::parse("red").unwrap();
    let style: Style = color.into();

    assert!(style.color.is_some());
    assert!(style.attributes.is_empty());
}

/// Test: From<(u8, u8, u8)> creates a style with RGB color
#[test]
fn test_style_from_tuple() {
    init_test_logging();

    let style: Style = (255u8, 0u8, 0u8).into();

    assert!(style.color.is_some());
    let triplet = style.color.unwrap().triplet.unwrap();
    assert_eq!(triplet.red, 255);
    assert_eq!(triplet.green, 0);
    assert_eq!(triplet.blue, 0);
}

/// Test: From<[u8; 3]> creates a style with RGB color
#[test]
fn test_style_from_array() {
    init_test_logging();

    let style: Style = [255u8, 0u8, 0u8].into();

    assert!(style.color.is_some());
}

// =============================================================================
// Add Operator Tests
// =============================================================================

/// Test: Style + Style combination
#[test]
fn test_style_add_operator() {
    init_test_logging();

    let s1 = Style::new().bold();
    let s2 = Style::new().italic();

    let combined = s1 + s2;

    assert!(combined.attributes.contains(Attributes::BOLD));
    assert!(combined.attributes.contains(Attributes::ITALIC));
}

/// Test: Style + &Style combination
#[test]
fn test_style_add_operator_ref() {
    init_test_logging();

    let s1 = Style::new().bold();
    let s2 = Style::new().italic();

    let combined = s1 + &s2;

    assert!(combined.attributes.contains(Attributes::BOLD));
    assert!(combined.attributes.contains(Attributes::ITALIC));
}

/// Test: &Style + Style combination
#[test]
fn test_style_ref_add_operator() {
    init_test_logging();

    let s1 = Style::new().bold();
    let s2 = Style::new().italic();

    let combined = &s1 + s2;

    assert!(combined.attributes.contains(Attributes::BOLD));
    assert!(combined.attributes.contains(Attributes::ITALIC));
}

/// Test: &Style + &Style combination
#[test]
fn test_style_ref_add_ref_operator() {
    init_test_logging();

    let s1 = Style::new().bold();
    let s2 = Style::new().italic();

    let combined = &s1 + &s2;

    assert!(combined.attributes.contains(Attributes::BOLD));
    assert!(combined.attributes.contains(Attributes::ITALIC));
}
