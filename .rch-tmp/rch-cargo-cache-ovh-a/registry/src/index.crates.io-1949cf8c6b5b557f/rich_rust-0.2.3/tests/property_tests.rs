//! Property-based tests for rich_rust.
//!
//! Uses proptest to verify invariants with 1000+ generated test cases.
//! These tests verify fundamental properties that should always hold.

use proptest::prelude::*;

use rich_rust::color::{Color, ColorSystem, ColorTriplet, ColorType};
use rich_rust::measure::Measurement;
use rich_rust::segment::Segment;
use rich_rust::style::{Attributes, Style};
use rich_rust::text::Text;

// ============================================================================
// Custom Strategies
// ============================================================================

/// Generate a valid RGB color triplet.
fn rgb_triplet() -> impl Strategy<Value = (u8, u8, u8)> {
    (any::<u8>(), any::<u8>(), any::<u8>())
}

/// Generate a valid ANSI color number (0-255).
fn ansi_color_number() -> impl Strategy<Value = u8> {
    0u8..=255u8
}

/// Generate random Attributes bitflags.
fn random_attributes() -> impl Strategy<Value = Attributes> {
    (0u16..8192u16).prop_map(Attributes::from_bits_truncate)
}

/// Generate a random Style.
fn random_style() -> impl Strategy<Value = Style> {
    (
        prop::option::of(rgb_triplet()),
        prop::option::of(rgb_triplet()),
        random_attributes(),
        prop::option::of("[a-z]{0,20}"),
    )
        .prop_map(|(fg, bg, attrs, link)| {
            let mut style = Style::new();
            if let Some((r, g, b)) = fg {
                style = style.color(Color::from_rgb(r, g, b));
            }
            if let Some((r, g, b)) = bg {
                style = style.bgcolor(Color::from_rgb(r, g, b));
            }
            // Apply attributes through the style methods
            if attrs.contains(Attributes::BOLD) {
                style = style.bold();
            }
            if attrs.contains(Attributes::ITALIC) {
                style = style.italic();
            }
            if attrs.contains(Attributes::UNDERLINE) {
                style = style.underline();
            }
            if attrs.contains(Attributes::STRIKE) {
                style = style.strike();
            }
            if let Some(url) = link
                && !url.is_empty()
            {
                style = style.link(url);
            }
            style
        })
}

/// Generate a random Measurement with valid bounds.
fn random_measurement() -> impl Strategy<Value = Measurement> {
    (0usize..1000, 0usize..1000).prop_map(|(a, b)| Measurement::new(a, b))
}

/// Generate ASCII text (simpler than full Unicode for basic tests).
fn ascii_text() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9 ]{0,100}"
}

// ============================================================================
// Color Property Tests
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    /// RGB roundtrip: from_rgb().get_truecolor() preserves values.
    #[test]
    fn prop_color_rgb_roundtrip(r in any::<u8>(), g in any::<u8>(), b in any::<u8>()) {
        let color = Color::from_rgb(r, g, b);
        let triplet = color.get_truecolor();
        prop_assert_eq!(triplet.red, r);
        prop_assert_eq!(triplet.green, g);
        prop_assert_eq!(triplet.blue, b);
    }

    /// Hex parsing roundtrip: parse(hex).triplet matches original values.
    #[test]
    fn prop_color_hex_roundtrip(r in any::<u8>(), g in any::<u8>(), b in any::<u8>()) {
        let hex = format!("#{r:02x}{g:02x}{b:02x}");
        let color = Color::parse(&hex).expect("valid hex should parse");
        let triplet = color.get_truecolor();
        prop_assert_eq!(triplet.red, r);
        prop_assert_eq!(triplet.green, g);
        prop_assert_eq!(triplet.blue, b);
    }

    /// Downgrade never increases color resolution.
    /// TrueColor > EightBit > Standard
    #[test]
    fn prop_color_downgrade_monotonic(r in any::<u8>(), g in any::<u8>(), b in any::<u8>()) {
        let truecolor = Color::from_rgb(r, g, b);
        prop_assert_eq!(truecolor.color_type, ColorType::TrueColor);

        // Downgrade to EightBit
        let eightbit = truecolor.downgrade(ColorSystem::EightBit);
        prop_assert!(
            matches!(eightbit.color_type, ColorType::Standard | ColorType::EightBit),
            "downgrade to 8-bit should be Standard or EightBit"
        );

        // Downgrade to Standard
        let standard = truecolor.downgrade(ColorSystem::Standard);
        prop_assert!(
            matches!(standard.color_type, ColorType::Standard),
            "downgrade to standard should be Standard"
        );

        // Downgrade is idempotent for Standard
        let standard_again = standard.downgrade(ColorSystem::Standard);
        prop_assert_eq!(standard.color_type, standard_again.color_type);
    }

    /// Standard colors (0-15) remain standard after downgrade.
    #[test]
    fn prop_color_standard_stable(n in 0u8..16u8) {
        let color = Color::from_ansi(n);
        prop_assert!(matches!(color.color_type, ColorType::Standard));

        let downgraded = color.downgrade(ColorSystem::Standard);
        prop_assert!(matches!(downgraded.color_type, ColorType::Standard));
    }

    /// All standard colors produce valid ANSI codes.
    #[test]
    fn prop_color_standard_valid_codes(n in 0u8..16u8) {
        let color = Color::from_ansi(n);
        let fg_codes = color.get_ansi_codes(true);
        let bg_codes = color.get_ansi_codes(false);

        prop_assert!(!fg_codes.is_empty(), "foreground codes should not be empty");
        prop_assert!(!bg_codes.is_empty(), "background codes should not be empty");

        // Parse codes as numbers to verify they're valid
        for code in &fg_codes {
            let _: u32 = code.parse().expect("code should be numeric");
        }
        for code in &bg_codes {
            let _: u32 = code.parse().expect("code should be numeric");
        }
    }

    /// 8-bit colors produce valid ANSI codes.
    #[test]
    fn prop_color_eightbit_valid_codes(n in ansi_color_number()) {
        let color = Color::from_ansi(n);
        let fg_codes = color.get_ansi_codes(true);
        let bg_codes = color.get_ansi_codes(false);

        prop_assert!(!fg_codes.is_empty());
        prop_assert!(!bg_codes.is_empty());
    }

    /// TrueColor produces valid ANSI codes.
    #[test]
    fn prop_color_truecolor_valid_codes((r, g, b) in rgb_triplet()) {
        let color = Color::from_rgb(r, g, b);
        let fg_codes = color.get_ansi_codes(true);
        let bg_codes = color.get_ansi_codes(false);

        // TrueColor should produce: ["38", "2", "r", "g", "b"] for foreground
        prop_assert_eq!(fg_codes.len(), 5);
        prop_assert_eq!(bg_codes.len(), 5);
        prop_assert_eq!(&fg_codes[0], "38");
        prop_assert_eq!(&bg_codes[0], "48");
        prop_assert_eq!(&fg_codes[1], "2");
        prop_assert_eq!(&bg_codes[1], "2");
    }

    /// ColorTriplet hex() produces valid 7-character hex string.
    #[test]
    fn prop_colortriplet_hex_format((r, g, b) in rgb_triplet()) {
        let triplet = ColorTriplet::new(r, g, b);
        let hex = triplet.hex();

        prop_assert_eq!(hex.len(), 7, "hex should be 7 chars");
        prop_assert!(hex.starts_with('#'), "hex should start with #");
        prop_assert!(hex[1..].chars().all(|c| c.is_ascii_hexdigit()));
    }

    /// ColorTriplet normalized values are in [0.0, 1.0].
    #[test]
    fn prop_colortriplet_normalized_range((r, g, b) in rgb_triplet()) {
        let triplet = ColorTriplet::new(r, g, b);
        let (nr, ng, nb) = triplet.normalized();

        prop_assert!((0.0..=1.0).contains(&nr));
        prop_assert!((0.0..=1.0).contains(&ng));
        prop_assert!((0.0..=1.0).contains(&nb));
    }
}

// ============================================================================
// Style Property Tests
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    /// Null style is left identity: null.combine(s) = s (for non-null s).
    #[test]
    fn prop_style_null_left_identity(style in random_style()) {
        let null = Style::null();
        let combined = null.combine(&style);

        // Properties should match the non-null style
        prop_assert_eq!(combined.color, style.color);
        prop_assert_eq!(combined.bgcolor, style.bgcolor);
        prop_assert_eq!(combined.link, style.link);
    }

    /// Null style is right identity: s.combine(null) = s.
    #[test]
    fn prop_style_null_right_identity(style in random_style()) {
        let null = Style::null();
        let combined = style.combine(&null);

        // Properties should match the original style
        prop_assert_eq!(combined.color, style.color);
        prop_assert_eq!(combined.bgcolor, style.bgcolor);
        prop_assert_eq!(combined.link, style.link);
    }

    /// Null combined with null is null.
    #[test]
    fn prop_style_null_combined_null(_n in 0..1i32) {
        let null1 = Style::null();
        let null2 = Style::null();
        let combined = null1.combine(&null2);

        // Combining two nulls should give original null
        prop_assert!(combined.is_null() || (combined.color.is_none() && combined.bgcolor.is_none()));
    }

    /// Style render produces balanced ANSI sequences.
    #[test]
    fn prop_style_render_balanced(style in random_style(), text in ascii_text()) {
        let rendered = style.render(&text, ColorSystem::TrueColor);

        // Count escape sequence starts and resets
        let sgr_starts = rendered.matches("\x1b[").count();
        let sgr_resets = rendered.matches("\x1b[0m").count();

        // For non-empty styles, should have balanced open/close
        // (Note: this is a simplified check - actual ANSI can be complex)
        if !style.is_null() && !rendered.is_empty() && style.color.is_some() || style.bgcolor.is_some() {
            prop_assert!(sgr_resets > 0 || sgr_starts == 0,
                "non-null style with colors should reset or have no codes");
        }
    }

    /// Make_ansi_codes produces valid semicolon-separated codes.
    #[test]
    fn prop_style_ansi_codes_format(style in random_style()) {
        let codes = style.make_ansi_codes(ColorSystem::TrueColor);

        // Codes should be empty or valid semicolon-separated numbers
        if !codes.is_empty() {
            for part in codes.split(';') {
                let _: u32 = part.parse().expect("code part should be numeric");
            }
        }
    }

    /// Combine is associative: (a.combine(b)).combine(c) == a.combine(b.combine(c)).
    #[test]
    fn prop_style_combine_associative(
        a in random_style(),
        b in random_style(),
        c in random_style(),
    ) {
        let left = a.combine(&b).combine(&c);
        let right = a.combine(&b.combine(&c));

        // Properties should match between associative orderings
        prop_assert_eq!(left.color, right.color);
        prop_assert_eq!(left.bgcolor, right.bgcolor);
        prop_assert_eq!(left.link, right.link);
    }

    /// Attribute toggles are idempotent: bold().bold() == bold().
    #[test]
    fn prop_style_attribute_idempotent(_n in 0..1i32) {
        // Bold
        let bold_once = Style::new().bold();
        let bold_twice = Style::new().bold().bold();
        prop_assert_eq!(bold_once.attributes, bold_twice.attributes);

        // Italic
        let italic_once = Style::new().italic();
        let italic_twice = Style::new().italic().italic();
        prop_assert_eq!(italic_once.attributes, italic_twice.attributes);

        // Underline
        let underline_once = Style::new().underline();
        let underline_twice = Style::new().underline().underline();
        prop_assert_eq!(underline_once.attributes, underline_twice.attributes);

        // Strike
        let strike_once = Style::new().strike();
        let strike_twice = Style::new().strike().strike();
        prop_assert_eq!(strike_once.attributes, strike_twice.attributes);
    }

    /// Link is preserved through combine when set.
    #[test]
    fn prop_style_link_preservation(url in "[a-z]{5,15}") {
        let linked = Style::new().link(&url);
        let null = Style::null();

        // Link should survive combine with null
        let combined = linked.combine(&null);
        prop_assert_eq!(combined.link, Some(url.clone()));

        // When both have links, later overrides
        let other_url = format!("{url}_other");
        let other_linked = Style::new().link(&other_url);
        let combined2 = linked.combine(&other_linked);
        prop_assert_eq!(combined2.link, Some(other_url));
    }
}

// ============================================================================
// Measurement Property Tests
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    /// Measurement invariant: minimum <= maximum after construction.
    #[test]
    fn prop_measurement_min_le_max(a in 0usize..10000, b in 0usize..10000) {
        let m = Measurement::new(a, b);
        prop_assert!(m.minimum <= m.maximum,
            "minimum {} should be <= maximum {}", m.minimum, m.maximum);
    }

    /// Measurement::exact has min == max.
    #[test]
    fn prop_measurement_exact(size in 0usize..10000) {
        let m = Measurement::exact(size);
        prop_assert_eq!(m.minimum, size);
        prop_assert_eq!(m.maximum, size);
        prop_assert_eq!(m.span(), 0);
    }

    /// Normalize preserves the invariant.
    #[test]
    fn prop_measurement_normalize(a in 0usize..10000, b in 0usize..10000) {
        let m = Measurement { minimum: a, maximum: b }; // Directly construct possibly invalid
        let n = m.normalize();
        prop_assert!(n.minimum <= n.maximum);
    }

    /// with_maximum clamps both values.
    #[test]
    fn prop_measurement_with_maximum(m in random_measurement(), cap in 0usize..2000) {
        let capped = m.with_maximum(cap);
        prop_assert!(capped.maximum <= cap,
            "maximum {} should be <= cap {}", capped.maximum, cap);
        prop_assert!(capped.minimum <= cap,
            "minimum {} should be <= cap {}", capped.minimum, cap);
    }

    /// with_minimum raises both values if needed.
    #[test]
    fn prop_measurement_with_minimum(m in random_measurement(), floor in 0usize..2000) {
        let floored = m.with_minimum(floor);
        prop_assert!(floored.minimum >= floor,
            "minimum {} should be >= floor {}", floored.minimum, floor);
        prop_assert!(floored.maximum >= floor,
            "maximum {} should be >= floor {}", floored.maximum, floor);
    }

    /// Clamp respects both bounds.
    #[test]
    fn prop_measurement_clamp(
        m in random_measurement(),
        min_bound in prop::option::of(0usize..500),
        max_bound in prop::option::of(500usize..2000),
    ) {
        let clamped = m.clamp(min_bound, max_bound);

        if let Some(min_b) = min_bound {
            prop_assert!(clamped.minimum >= min_b.min(max_bound.unwrap_or(usize::MAX)),
                "clamped minimum should respect lower bound");
        }
        if let Some(max_b) = max_bound {
            prop_assert!(clamped.maximum <= max_b,
                "clamped maximum should respect upper bound");
        }
    }

    /// Union is commutative.
    #[test]
    fn prop_measurement_union_commutative(a in random_measurement(), b in random_measurement()) {
        let ab = a.union(&b);
        let ba = b.union(&a);
        prop_assert_eq!(ab.minimum, ba.minimum);
        prop_assert_eq!(ab.maximum, ba.maximum);
    }

    /// Union is associative.
    #[test]
    fn prop_measurement_union_associative(
        a in random_measurement(),
        b in random_measurement(),
        c in random_measurement(),
    ) {
        let ab_c = a.union(&b).union(&c);
        let a_bc = a.union(&b.union(&c));
        prop_assert_eq!(ab_c.minimum, a_bc.minimum);
        prop_assert_eq!(ab_c.maximum, a_bc.maximum);
    }

    /// Add operator is commutative.
    #[test]
    fn prop_measurement_add_commutative(a in random_measurement(), b in random_measurement()) {
        let ab = a + b;
        let ba = b + a;
        prop_assert_eq!(ab.minimum, ba.minimum);
        prop_assert_eq!(ab.maximum, ba.maximum);
    }

    /// Span is non-negative.
    #[test]
    fn prop_measurement_span_nonnegative(m in random_measurement()) {
        let span = m.span();
        prop_assert!(span <= m.maximum, "span should be <= maximum");
    }

    /// Fits is correct for boundary values.
    #[test]
    fn prop_measurement_fits(m in random_measurement()) {
        // minimum should fit
        prop_assert!(m.fits(m.minimum), "minimum should fit");
        // maximum should fit
        prop_assert!(m.fits(m.maximum), "maximum should fit");
        // values outside should not fit (when there's a gap)
        if m.minimum > 0 {
            prop_assert!(!m.fits(m.minimum - 1), "below minimum should not fit");
        }
        if m.maximum < usize::MAX {
            prop_assert!(!m.fits(m.maximum + 1), "above maximum should not fit");
        }
    }

    /// Intersect returns None for non-overlapping ranges.
    #[test]
    fn prop_measurement_intersect_disjoint(
        a_min in 0usize..100,
        a_span in 0usize..50,
        gap in 1usize..100,
        b_span in 0usize..50,
    ) {
        let a = Measurement::new(a_min, a_min + a_span);
        let b_min = a_min + a_span + gap;
        let b = Measurement::new(b_min, b_min + b_span);

        prop_assert!(a.intersect(&b).is_none(), "disjoint ranges should not intersect");
    }
}

// ============================================================================
// Segment Property Tests
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    /// Split at cell preserves total content.
    #[test]
    fn prop_segment_split_preserves_content(text in ascii_text(), pos in 0usize..200) {
        let segment = Segment::plain(&text);
        let (left, right) = segment.split_at_cell(pos);

        let combined = format!("{}{}", left.text, right.text);
        prop_assert_eq!(combined, text, "split should preserve content");
    }

    /// Split at 0 gives empty left.
    #[test]
    fn prop_segment_split_at_zero(text in ascii_text()) {
        let segment = Segment::plain(text.clone());
        let (left, right) = segment.split_at_cell(0);

        prop_assert!(left.text.is_empty(), "split at 0 should give empty left");
        prop_assert_eq!(right.text, text, "split at 0 should give full right");
    }

    /// Split beyond length gives full left.
    #[test]
    fn prop_segment_split_beyond_length(text in ascii_text()) {
        let segment = Segment::plain(text.clone());
        let (left, right) = segment.split_at_cell(1000);

        prop_assert_eq!(left.text, text, "split beyond length should give full left");
        prop_assert!(right.text.is_empty(), "split beyond length should give empty right");
    }

    /// Segment cell_length is consistent.
    #[test]
    fn prop_segment_cell_length_consistent(text in ascii_text()) {
        let segment = Segment::plain(&text);
        let len1 = segment.cell_length();
        let len2 = segment.cell_length();
        prop_assert_eq!(len1, len2, "cell_length should be consistent");
    }

    /// Control segments have zero width.
    #[test]
    fn prop_segment_control_zero_width(_n in 0..10i32) {
        use rich_rust::segment::{ControlCode, ControlType};
        let segment = Segment::control(vec![ControlCode::new(ControlType::Bell)]);
        prop_assert_eq!(segment.cell_length(), 0, "control segments should have zero width");
        prop_assert!(segment.is_control(), "should be marked as control");
    }

    /// Styled segment preserves style through split.
    #[test]
    fn prop_segment_split_preserves_style(text in ascii_text(), pos in 0usize..100) {
        let style = Style::new().bold().italic();
        let segment = Segment::styled(&text, style.clone());
        let (left, right) = segment.split_at_cell(pos);

        if !left.text.is_empty() {
            prop_assert_eq!(left.style, Some(style.clone()), "left should preserve style");
        }
        if !right.text.is_empty() {
            prop_assert_eq!(right.style, Some(style), "right should preserve style");
        }
    }

    /// Empty segment is empty.
    #[test]
    fn prop_segment_empty(_n in 0..1i32) {
        let segment = Segment::plain("");
        prop_assert!(segment.is_empty(), "empty segment should be empty");
        prop_assert_eq!(segment.cell_length(), 0, "empty segment should have zero width");
    }
}

// ============================================================================
// Text Property Tests
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(1000))]

    /// Divide at no offsets returns original.
    #[test]
    fn prop_text_divide_empty_offsets(text in ascii_text()) {
        let t = Text::new(&text);
        let parts = t.divide(&[]);

        prop_assert_eq!(parts.len(), 1, "divide with no offsets should return 1 part");
        prop_assert_eq!(parts[0].plain(), text, "divide with no offsets should preserve content");
    }

    /// Divide then concatenate preserves content (with sorted, unique offsets).
    #[test]
    fn prop_text_divide_concat(text in ascii_text(), offsets in prop::collection::vec(0usize..200, 0..5)) {
        let t = Text::new(&text);

        // Sort and deduplicate offsets for correct divide behavior
        let mut sorted_offsets: Vec<usize> = offsets.into_iter().collect();
        sorted_offsets.sort();
        sorted_offsets.dedup();

        let parts = t.divide(&sorted_offsets);

        let concatenated: String = parts.iter().map(|p| p.plain()).collect();
        prop_assert_eq!(concatenated, text, "divide then concat should preserve content");
    }

    /// Slice within bounds produces valid result.
    #[test]
    fn prop_text_slice_bounds(text in ascii_text(), start in 0usize..150, len in 0usize..50) {
        let t = Text::new(&text);
        let text_len = t.len();
        let end = (start + len).min(text_len);
        let actual_start = start.min(text_len);

        let sliced = t.slice(actual_start, end);
        prop_assert!(sliced.len() <= end.saturating_sub(actual_start) + 1,
            "slice length should be bounded");
    }

    /// Slice of entire text equals original.
    #[test]
    fn prop_text_slice_full(text in ascii_text()) {
        let t = Text::new(&text);
        let sliced = t.slice(0, t.len());
        prop_assert_eq!(sliced.plain(), text, "full slice should equal original");
    }

    /// Empty slice is empty.
    #[test]
    fn prop_text_slice_empty(text in ascii_text()) {
        let t = Text::new(&text);
        let sliced = t.slice(0, 0);
        prop_assert!(sliced.is_empty(), "zero-length slice should be empty");
    }

    /// Append preserves both parts.
    #[test]
    fn prop_text_append(text1 in ascii_text(), text2 in ascii_text()) {
        let mut t = Text::new(&text1);
        t.append(&text2);

        let expected = format!("{text1}{text2}");
        prop_assert_eq!(t.plain(), expected, "append should concatenate");
    }

    /// Split lines preserves content (modulo newlines).
    #[test]
    fn prop_text_split_lines_content(lines in prop::collection::vec(ascii_text(), 1..5)) {
        let text = lines.join("\n");
        let t = Text::new(&text);
        let split = t.split_lines();

        // Content should match (excluding newlines)
        let rejoined: String = split.iter()
            .map(|l| l.plain())
            .collect::<Vec<_>>()
            .join("\n");
        prop_assert_eq!(rejoined, text, "split_lines then join should preserve content");
    }

    /// Text length equals character count.
    #[test]
    fn prop_text_len_char_count(text in ascii_text()) {
        let t = Text::new(&text);
        prop_assert_eq!(t.len(), text.chars().count(), "len should equal char count");
    }

    /// Stylize doesn't change plain text.
    #[test]
    fn prop_text_stylize_preserves_plain(text in ascii_text(), start in 0usize..50, len in 1usize..20) {
        let mut t = Text::new(&text);
        let end = start + len;
        t.stylize(start, end, Style::new().bold());

        prop_assert_eq!(t.plain(), text, "stylize should not change plain text");
    }
}

// ============================================================================
// Integration Property Tests
// ============================================================================

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Style rendering with various color systems.
    #[test]
    fn prop_integration_style_color_systems((r, g, b) in rgb_triplet(), text in ascii_text()) {
        let style = Style::new().color(Color::from_rgb(r, g, b));

        // All color systems should produce valid output
        let _truecolor = style.render(&text, ColorSystem::TrueColor);
        let _eightbit = style.render(&text, ColorSystem::EightBit);
        let _standard = style.render(&text, ColorSystem::Standard);

        // Output should contain the original text
        prop_assert!(_truecolor.contains(&text) || text.is_empty());
        prop_assert!(_eightbit.contains(&text) || text.is_empty());
        prop_assert!(_standard.contains(&text) || text.is_empty());
    }

    /// Segment from Text conversion.
    #[test]
    fn prop_integration_text_to_segment(text in ascii_text()) {
        let t = Text::new(&text);
        let plain = t.plain();
        let segment = Segment::plain(plain);

        prop_assert_eq!(segment.text, text, "text to segment should preserve content");
    }
}

// ============================================================================
// Table Property Tests
// ============================================================================

use rich_rust::prelude::{Column, Table};
use rich_rust::segment::split_lines;

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Table renders with correct structure for N columns × M rows.
    #[test]
    fn prop_table_structure(num_cols in 1usize..6, num_rows in 1usize..5) {
        let mut table = Table::new();

        for i in 0..num_cols {
            table.add_column(Column::new(format!("Col{i}")));
        }

        for _row_idx in 0..num_rows {
            let cells: Vec<&str> = (0..num_cols).map(|_| "x").collect();
            table.add_row_cells(cells);
        }

        // Render should not panic and should produce non-empty output
        let output = table.render_plain(80);
        prop_assert!(!output.is_empty(), "table should produce output");

        // Headers should be present
        for i in 0..num_cols {
            prop_assert!(output.contains(&format!("Col{i}")),
                "missing header Col{i}");
        }
    }

    /// Empty table should render without panicking.
    #[test]
    fn prop_table_empty_handling(_n in 0..1i32) {
        let table = Table::new();

        // Should not panic
        let segments = table.render(80);
        // Empty table may produce empty output or just box borders
        let _ = segments;
    }

    /// Single column table should render correctly.
    #[test]
    fn prop_table_single_column(num_rows in 1usize..5, cell_text in "[a-zA-Z0-9]{1,20}") {
        let mut table = Table::new()
            .with_column(Column::new("Header"));

        for _ in 0..num_rows {
            table.add_row_cells([cell_text.as_str()]);
        }

        let output = table.render_plain(80);

        // Should contain header
        prop_assert!(output.contains("Header"), "missing Header");
        // Cell text (no leading/trailing spaces in strategy) should appear
        prop_assert!(output.contains(&cell_text) || output.contains("…"),
            "should contain cell text '{}' or ellipsis if truncated", cell_text);
    }

    /// Single row table renders without panicking.
    #[test]
    fn prop_table_single_row(num_cols in 1usize..5) {
        let mut table = Table::new();

        for i in 0..num_cols {
            table.add_column(Column::new(format!("H{i}")));
        }

        let cells: Vec<String> = (0..num_cols).map(|i| format!("C{i}")).collect();
        table.add_row_cells(cells.iter().map(|s| s.as_str()));

        let output = table.render_plain(80);
        prop_assert!(!output.is_empty(), "single row table should produce output");
    }

    /// Table width constraint should be respected.
    #[test]
    fn prop_table_width_constraint(width in 20usize..120) {
        let mut table = Table::new()
            .with_column(Column::new("A"))
            .with_column(Column::new("B"));
        table.add_row_cells(["Cell A", "Cell B"]);

        let segments = table.render(width);
        let lines = split_lines(segments.into_iter().map(|s| s.into_owned()));

        // Each line should not exceed the width
        for line in lines {
            let line_width: usize = line.iter().map(|s| s.cell_length()).sum();
            prop_assert!(line_width <= width,
                "line width {} should not exceed constraint {}", line_width, width);
        }
    }

    /// Cell content should be preserved in output (possibly truncated).
    #[test]
    fn prop_table_cell_content_preserved(cell_text in "[a-z]{1,10}") {
        let mut table = Table::new()
            .with_column(Column::new("Header"));
        table.add_row_cells([cell_text.as_str()]);

        let output = table.render_plain(80);

        // Cell content should appear or be ellipsized
        prop_assert!(output.contains(&cell_text) || output.contains("…"),
            "cell text '{}' should appear or be truncated with ellipsis", cell_text);
    }

    /// Row heights should be consistent within a row (no panic on long text).
    #[test]
    fn prop_table_row_height_consistent(cols in 2usize..5, long_text in "[a-z ]{20,50}") {
        let mut table = Table::new();

        for i in 0..cols {
            table.add_column(Column::new(format!("Col{i}")));
        }

        // Build one row: first cell is long, rest are short
        let mut cells: Vec<String> = vec![long_text.clone()];
        for _ in 1..cols {
            cells.push("X".to_string());
        }
        table.add_row_cells(cells.iter().map(|s| s.as_str()));

        // Should render without panicking
        let segments = table.render(60);
        prop_assert!(!segments.is_empty(), "table with content should produce output");
    }

    /// Border characters should be valid Unicode box drawing or printable.
    #[test]
    fn prop_table_border_chars_valid(_n in 0..1i32) {
        let mut table = Table::new()
            .with_column(Column::new("A"))
            .with_column(Column::new("B"));
        table.add_row_cells(["1", "2"]);

        let output = table.render_plain(40);

        // All characters should be printable, whitespace, or box-drawing
        // (Unicode block 2500-257F covers all box drawing characters)
        for ch in output.chars() {
            let is_box_drawing = ('\u{2500}'..='\u{257F}').contains(&ch);
            prop_assert!(
                ch.is_alphanumeric() || ch.is_whitespace() || ch == '…' ||
                is_box_drawing || ch == '\n',
                "unexpected character: {:?} (U+{:04X})", ch, ch as u32
            );
        }
    }
}
