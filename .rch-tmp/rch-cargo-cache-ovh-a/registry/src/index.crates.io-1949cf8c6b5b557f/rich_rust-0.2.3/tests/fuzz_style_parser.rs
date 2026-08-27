//! Fuzz-style property tests for Style parser and ANSI generation.
//!
//! Validates that parsers never panic on arbitrary input, that ANSI
//! output is well-formed, and that color downgrade invariants hold.
//!
//! Run with: cargo test --test fuzz_style_parser

use proptest::prelude::*;

use rich_rust::color::{Color, ColorSystem, ColorTriplet};
use rich_rust::style::Style;

// ============================================================================
// Custom strategies
// ============================================================================

/// Generate arbitrary ASCII strings that might appear in style definitions.
fn style_like_input() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9 #(),;:_/\\-]{0,80}"
}

/// Generate strings that look like colors (hex, rgb, named, or garbage).
fn color_like_input() -> impl Strategy<Value = String> {
    prop_oneof![
        // Valid hex formats
        "#[0-9a-fA-F]{6}",
        "#[0-9a-fA-F]{3}",
        // Valid rgb format
        "rgb\\([0-9]{1,3},[0-9]{1,3},[0-9]{1,3}\\)",
        // Valid color() format
        "color\\([0-9]{1,3}\\)",
        // Named colors and garbage
        "[a-z_]{1,20}",
        // Totally arbitrary
        "[a-zA-Z0-9 #(),;:\\-]{0,30}",
    ]
}

/// Generate valid attribute names (including invalid ones).
fn attribute_name() -> impl Strategy<Value = String> {
    prop_oneof![
        // Valid attributes
        Just("bold".to_string()),
        Just("italic".to_string()),
        Just("underline".to_string()),
        Just("dim".to_string()),
        Just("blink".to_string()),
        Just("reverse".to_string()),
        Just("conceal".to_string()),
        Just("strike".to_string()),
        Just("overline".to_string()),
        // Short aliases
        Just("b".to_string()),
        Just("i".to_string()),
        Just("u".to_string()),
        Just("d".to_string()),
        Just("s".to_string()),
        Just("r".to_string()),
        // Invalid attributes
        Just("boldd".to_string()),
        Just("foobar".to_string()),
        Just("".to_string()),
    ]
}

// ============================================================================
// 1. Color parser: never panics on arbitrary input
// ============================================================================

#[test]
fn fuzz_color_parse_empty() {
    // Empty string should return Err, not panic
    let result = Color::parse("");
    assert!(result.is_err(), "empty string should fail to parse");
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    #[test]
    fn fuzz_color_parse_no_panic(input in color_like_input()) {
        // Should never panic, only return Ok or Err
        let _ = Color::parse(&input);
    }

    #[test]
    fn fuzz_color_parse_arbitrary_string(input in "\\PC{0,50}") {
        let _ = Color::parse(&input);
    }
}

// ============================================================================
// 2. RGB value bounds
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn fuzz_rgb_bounds_valid(r in 0u8..=255u8, g in 0u8..=255u8, b in 0u8..=255u8) {
        let input = format!("rgb({r},{g},{b})");
        let result = Color::parse(&input);
        prop_assert!(result.is_ok(), "valid RGB should parse: {input}");

        let color = result.unwrap();
        let triplet = color.get_truecolor();
        prop_assert_eq!(triplet.red, r);
        prop_assert_eq!(triplet.green, g);
        prop_assert_eq!(triplet.blue, b);
    }

    #[test]
    fn fuzz_rgb_out_of_range(val in 256u32..1000u32) {
        let input = format!("rgb({val},0,0)");
        let result = Color::parse(&input);
        prop_assert!(result.is_err(), "out-of-range RGB should fail: {input}");
    }

    #[test]
    fn fuzz_color_number_boundary(n in 0u16..300u16) {
        let input = format!("color({n})");
        let result = Color::parse(&input);
        if n <= 255 {
            prop_assert!(result.is_ok(), "color(0-255) should parse: {input}");
        } else {
            prop_assert!(result.is_err(), "color(>255) should fail: {input}");
        }
    }

    #[test]
    fn fuzz_color_standard_vs_eightbit_boundary(n in 0u8..=255u8) {
        let color = Color::from_ansi(n);
        if n < 16 {
            prop_assert!(
                matches!(color.color_type, rich_rust::color::ColorType::Standard),
                "0-15 should be Standard, got {:?} for {n}", color.color_type
            );
        } else {
            prop_assert!(
                matches!(color.color_type, rich_rust::color::ColorType::EightBit),
                "16-255 should be EightBit, got {:?} for {n}", color.color_type
            );
        }
    }
}

// ============================================================================
// 3. Hex parsing edge cases
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn fuzz_hex_valid_6digit(r in 0u8..=255u8, g in 0u8..=255u8, b in 0u8..=255u8) {
        let hex = format!("#{r:02x}{g:02x}{b:02x}");
        let result = Color::parse(&hex);
        prop_assert!(result.is_ok(), "valid 6-digit hex should parse: {hex}");
    }

    #[test]
    fn fuzz_hex_case_insensitive(r in 0u8..=255u8, g in 0u8..=255u8, b in 0u8..=255u8) {
        let lower = format!("#{r:02x}{g:02x}{b:02x}");
        let upper = format!("#{r:02X}{g:02X}{b:02X}");
        let lower_color = Color::parse(&lower).unwrap();
        let upper_color = Color::parse(&upper).unwrap();
        prop_assert_eq!(
            lower_color.get_truecolor().red,
            upper_color.get_truecolor().red,
            "case should not matter for hex"
        );
    }

    #[test]
    fn fuzz_hex_invalid_chars(s in "#[g-zG-Z]{6}") {
        let result = Color::parse(&s);
        prop_assert!(result.is_err(), "non-hex chars should fail: {s}");
    }

    #[test]
    fn fuzz_hex_wrong_length(len in 1usize..20) {
        if len == 3 || len == 6 {
            return Ok(());
        }
        let hex = format!("#{}", "f".repeat(len));
        let result = Color::parse(&hex);
        prop_assert!(result.is_err(), "hex with {len} digits should fail: {hex}");
    }
}

// ============================================================================
// 4. Style parser: never panics on arbitrary input
// ============================================================================

#[test]
fn fuzz_style_parse_empty() {
    let result = Style::parse("");
    assert!(result.is_ok(), "empty string should produce null style");
    assert!(
        result.unwrap().is_null(),
        "empty string should be null style"
    );
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    #[test]
    fn fuzz_style_parse_no_panic(input in style_like_input()) {
        let _ = Style::parse(&input);
    }

    #[test]
    fn fuzz_style_parse_arbitrary(input in "\\PC{0,50}") {
        let _ = Style::parse(&input);
    }

    #[test]
    fn fuzz_style_parse_attribute_combinations(
        attrs in prop::collection::vec(attribute_name(), 0..6)
    ) {
        let input = attrs.join(" ");
        let _ = Style::parse(&input);
    }

    #[test]
    fn fuzz_style_parse_with_colors(
        attr in attribute_name(),
        color in color_like_input(),
    ) {
        let input = format!("{attr} {color}");
        let _ = Style::parse(&input);

        let with_bg = format!("{attr} {color} on {color}");
        let _ = Style::parse(&with_bg);
    }

    #[test]
    fn fuzz_style_parse_with_link(url in "[a-z]{0,30}") {
        let input = format!("bold link {url}");
        let _ = Style::parse(&input);
    }

    #[test]
    fn fuzz_style_parse_not_prefix(attr in attribute_name()) {
        let input = format!("not {attr}");
        let _ = Style::parse(&input);
    }

    #[test]
    fn fuzz_style_parse_on_prefix(color in color_like_input()) {
        let input = format!("on {color}");
        let _ = Style::parse(&input);
    }
}

// ============================================================================
// 5. ANSI escape validity
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn fuzz_ansi_codes_format(r in 0u8..=255u8, g in 0u8..=255u8, b in 0u8..=255u8) {
        let style = Style::new().color(Color::from_rgb(r, g, b));

        for system in [ColorSystem::TrueColor, ColorSystem::EightBit, ColorSystem::Standard] {
            let codes = style.make_ansi_codes(system);
            if !codes.is_empty() {
                // All parts should be numeric
                for part in codes.split(';') {
                    prop_assert!(
                        part.parse::<u32>().is_ok(),
                        "ANSI code part should be numeric, got '{part}' from system {system:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn fuzz_ansi_render_balanced(r in 0u8..=255u8, g in 0u8..=255u8, b in 0u8..=255u8) {
        let style = Style::new()
            .bold()
            .color(Color::from_rgb(r, g, b));

        for system in [ColorSystem::TrueColor, ColorSystem::EightBit, ColorSystem::Standard] {
            let (prefix, suffix) = style.render_ansi(system).as_ref().clone();

            // If prefix is non-empty, suffix should contain reset
            if !prefix.is_empty() {
                prop_assert!(
                    suffix.contains("\x1b[0m") || suffix.contains("\x1b["),
                    "non-empty prefix should have reset suffix for {system:?}"
                );
            }

            // Both should be valid UTF-8 (they're Strings, but verify no panics)
            let _ = prefix.len();
            let _ = suffix.len();
        }
    }

    #[test]
    fn fuzz_ansi_render_text_contains_original(
        text in "[a-zA-Z0-9]{1,20}",
        r in 0u8..=255u8,
        g in 0u8..=255u8,
        b in 0u8..=255u8,
    ) {
        let style = Style::new().color(Color::from_rgb(r, g, b));
        let rendered = style.render(&text, ColorSystem::TrueColor);
        prop_assert!(
            rendered.contains(&text),
            "rendered text should contain original '{text}', got: {rendered}"
        );
    }
}

// ============================================================================
// 6. Theme resolution
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn fuzz_theme_lookup_no_panic(name in "[a-z._]{0,30}") {
        use rich_rust::theme::Theme;
        let theme = Theme::default();
        // Theme lookup should never panic
        let _ = theme.get(&name);
    }

    #[test]
    fn fuzz_theme_from_styles_no_panic(
        key in "[a-z.]{1,20}",
        value in style_like_input(),
    ) {
        use rich_rust::theme::Theme;
        let definitions = vec![(key, value)];
        let _ = Theme::from_style_definitions(definitions.into_iter(), false);
    }
}

// ============================================================================
// 7. Hyperlink URLs
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn fuzz_hyperlink_render_no_panic(url in "[a-zA-Z0-9:/._\\-?&=#]{0,50}") {
        let style = Style::new().link(&url);
        let (prefix, suffix) = style.render_ansi(ColorSystem::TrueColor).as_ref().clone();

        // Should produce OSC 8 sequences when URL is non-empty
        if !url.is_empty() {
            prop_assert!(
                prefix.contains("\x1b]8;"),
                "link style should produce OSC 8 opener for url '{url}'"
            );
            prop_assert!(
                suffix.contains("\x1b]8;;"),
                "link style should produce OSC 8 closer for url '{url}'"
            );
        }
    }

    #[test]
    fn fuzz_hyperlink_with_id_no_panic(
        url in "[a-zA-Z0-9:/._]{1,20}",
        id in "[a-zA-Z0-9]{1,10}",
    ) {
        let style = Style::new().link_with_id(&url, &id);
        let (prefix, _suffix) = style.render_ansi(ColorSystem::TrueColor).as_ref().clone();

        // Should include the ID in the params
        prop_assert!(
            prefix.contains(&format!("id={id}")),
            "link with id should include id={id} in OSC 8"
        );
    }

    #[test]
    fn fuzz_hyperlink_combined_with_style(url in "[a-z]{1,10}") {
        let style = Style::new().bold().italic().link(&url);
        let (prefix, suffix) = style.render_ansi(ColorSystem::TrueColor).as_ref().clone();

        // Should have both SGR and OSC 8
        prop_assert!(prefix.contains("\x1b["), "should have SGR codes");
        prop_assert!(prefix.contains("\x1b]8;"), "should have OSC 8 opener");
        prop_assert!(suffix.contains("\x1b[0m"), "should have SGR reset");
        prop_assert!(suffix.contains("\x1b]8;;"), "should have OSC 8 closer");
    }
}

// ============================================================================
// 8. Extreme attribute counts
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn fuzz_all_attributes_at_once(_n in 0..1i32) {
        let style = Style::new()
            .bold()
            .italic()
            .underline()
            .dim()
            .blink()
            .reverse()
            .conceal()
            .strike()
            .overline();

        // Should render without panicking for all color systems
        for system in [ColorSystem::TrueColor, ColorSystem::EightBit, ColorSystem::Standard] {
            let codes = style.make_ansi_codes(system);
            prop_assert!(!codes.is_empty(), "all attributes should produce codes");

            let (prefix, suffix) = style.render_ansi(system).as_ref().clone();
            prop_assert!(!prefix.is_empty(), "all attributes should produce prefix");
            prop_assert!(!suffix.is_empty(), "all attributes should produce suffix");
        }
    }

    #[test]
    fn fuzz_all_attributes_with_colors(_n in 0..1i32) {
        let style = Style::new()
            .bold()
            .italic()
            .underline()
            .dim()
            .strike()
            .overline()
            .color(Color::from_rgb(255, 0, 0))
            .bgcolor(Color::from_rgb(0, 0, 255))
            .link("https://example.com");

        for system in [ColorSystem::TrueColor, ColorSystem::EightBit, ColorSystem::Standard] {
            let rendered = style.render("test", system);
            prop_assert!(rendered.contains("test"), "rendered should contain text");
            prop_assert!(rendered.contains("\x1b["), "should have ANSI codes");
        }
    }

    #[test]
    fn fuzz_repeated_parse_roundtrip(input in "bold|italic|underline|dim|strike|overline") {
        // Parsing a valid attribute string should succeed
        let style = Style::parse(&input);
        prop_assert!(style.is_ok(), "valid attribute '{input}' should parse");

        // The resulting style should be non-null
        let style = style.unwrap();
        prop_assert!(!style.is_null(), "'{input}' should produce non-null style");
    }
}

// ============================================================================
// 9. Color downgrade chain invariants
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    #[test]
    fn fuzz_downgrade_truecolor_to_eightbit(r in 0u8..=255u8, g in 0u8..=255u8, b in 0u8..=255u8) {
        let tc = Color::from_rgb(r, g, b);
        let eb = tc.downgrade(ColorSystem::EightBit);

        // Downgraded color should be Standard or EightBit
        prop_assert!(
            matches!(eb.color_type, rich_rust::color::ColorType::Standard | rich_rust::color::ColorType::EightBit),
            "downgrade to EightBit should be Standard or EightBit"
        );

        // Further downgrade to Standard should be Standard
        let std = eb.downgrade(ColorSystem::Standard);
        prop_assert!(
            matches!(std.color_type, rich_rust::color::ColorType::Standard),
            "double downgrade to Standard should be Standard"
        );
    }

    #[test]
    fn fuzz_downgrade_idempotent(r in 0u8..=255u8, g in 0u8..=255u8, b in 0u8..=255u8) {
        let color = Color::from_rgb(r, g, b);

        // TrueColor → TrueColor should be identity
        let same = color.downgrade(ColorSystem::TrueColor);
        prop_assert_eq!(
            same.get_truecolor().red,
            color.get_truecolor().red,
            "TrueColor downgrade to TrueColor should be identity"
        );

        // Standard → Standard should be idempotent
        let std1 = color.downgrade(ColorSystem::Standard);
        let std2 = std1.downgrade(ColorSystem::Standard);
        prop_assert_eq!(
            std1.color_type, std2.color_type,
            "Standard downgrade should be idempotent"
        );
    }

    #[test]
    fn fuzz_named_colors_parse_correctly(idx in 0usize..10) {
        let names = [
            "red", "green", "blue", "yellow", "magenta",
            "cyan", "white", "black", "bright_red", "bright_blue",
        ];
        let name = names[idx];
        let result = Color::parse(name);
        prop_assert!(result.is_ok(), "named color '{name}' should parse");
    }
}

// ============================================================================
// 10. ColorTriplet operations
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn fuzz_colortriplet_hex_roundtrip(r in 0u8..=255u8, g in 0u8..=255u8, b in 0u8..=255u8) {
        let triplet = ColorTriplet::new(r, g, b);
        let hex = triplet.hex();

        // Parse back
        let color = Color::parse(&hex).expect("hex from ColorTriplet should parse");
        let roundtrip = color.get_truecolor();
        prop_assert_eq!(roundtrip.red, r, "red should roundtrip");
        prop_assert_eq!(roundtrip.green, g, "green should roundtrip");
        prop_assert_eq!(roundtrip.blue, b, "blue should roundtrip");
    }

    #[test]
    fn fuzz_colortriplet_normalized_range(r in 0u8..=255u8, g in 0u8..=255u8, b in 0u8..=255u8) {
        let triplet = ColorTriplet::new(r, g, b);
        let (nr, ng, nb) = triplet.normalized();

        prop_assert!((0.0..=1.0).contains(&nr), "normalized red out of range: {nr}");
        prop_assert!((0.0..=1.0).contains(&ng), "normalized green out of range: {ng}");
        prop_assert!((0.0..=1.0).contains(&nb), "normalized blue out of range: {nb}");
    }
}
