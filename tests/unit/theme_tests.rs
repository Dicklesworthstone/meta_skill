//! Unit tests for the theme system module.
//!
//! These tests verify the theme presets, colors, icons, box/tree/progress
//! styles, terminal capability adaptation, and theme validation.
//!
//! # Test Categories
//!
//! - Theme Presets: All preset creation and configuration
//! - Theme Colors: Semantic colors, parsing, and manipulation
//! - Theme Icons: Unicode/ASCII icons and spinner frames
//! - Box/Tree/Progress Styles: Character sets and FromStr parsing
//! - Terminal Adaptation: Color downgrade and ASCII fallback
//! - Validation: Theme completeness checks

use std::str::FromStr;

use ms::output::{
    BoxStyle, ProgressStyle, TerminalBackground, TerminalCapabilities, Theme, ThemeColors,
    ThemeError, ThemeIcons, ThemePreset, TreeGuides,
};
use rich_rust::color::ColorSystem;
use rich_rust::style::Style;

// ============================================================================
// Theme Preset Tests
// ============================================================================

#[test]
fn default_theme_has_correct_name() {
    let theme = Theme::default();
    assert_eq!(theme.name, "default");
}

#[test]
fn default_theme_uses_unicode_icons() {
    let theme = Theme::default();
    // Default theme should have unicode icons with non-empty unicode values
    assert!(!theme.icons.success.unicode.is_empty());
    assert!(!theme.icons.error.unicode.is_empty());
}

#[test]
fn default_theme_uses_rounded_box_style() {
    let theme = Theme::default();
    assert_eq!(theme.box_style, BoxStyle::Rounded);
}

#[test]
fn default_theme_uses_unicode_tree_guides() {
    let theme = Theme::default();
    assert_eq!(theme.tree_guides, TreeGuides::Unicode);
}

#[test]
fn default_theme_uses_block_progress() {
    let theme = Theme::default();
    assert_eq!(theme.progress_style, ProgressStyle::Block);
}

#[test]
fn default_theme_is_dark_mode() {
    let theme = Theme::default();
    assert!(!theme.is_light_mode);
}

#[test]
fn minimal_theme_has_correct_name() {
    let theme = ThemePreset::Minimal.to_theme();
    assert_eq!(theme.name, "minimal");
}

#[test]
fn minimal_theme_uses_ascii_box_style() {
    let theme = ThemePreset::Minimal.to_theme();
    assert_eq!(theme.box_style, BoxStyle::Ascii);
}

#[test]
fn minimal_theme_uses_ascii_tree_guides() {
    let theme = ThemePreset::Minimal.to_theme();
    assert_eq!(theme.tree_guides, TreeGuides::Ascii);
}

#[test]
fn minimal_theme_uses_ascii_progress() {
    let theme = ThemePreset::Minimal.to_theme();
    assert_eq!(theme.progress_style, ProgressStyle::Ascii);
}

#[test]
fn minimal_theme_uses_ascii_icons() {
    let theme = ThemePreset::Minimal.to_theme();
    // ASCII icons have empty unicode fields
    assert!(theme.icons.success.unicode.is_empty());
    assert!(theme.icons.error.unicode.is_empty());
}

#[test]
fn vibrant_theme_has_correct_name() {
    let theme = ThemePreset::Vibrant.to_theme();
    assert_eq!(theme.name, "vibrant");
}

#[test]
fn vibrant_theme_uses_unicode_icons() {
    let theme = ThemePreset::Vibrant.to_theme();
    assert!(!theme.icons.success.unicode.is_empty());
}

#[test]
fn vibrant_theme_uses_rounded_box() {
    let theme = ThemePreset::Vibrant.to_theme();
    assert_eq!(theme.box_style, BoxStyle::Rounded);
}

#[test]
fn monochrome_theme_has_correct_name() {
    let theme = ThemePreset::Monochrome.to_theme();
    assert_eq!(theme.name, "monochrome");
}

#[test]
fn monochrome_theme_uses_ascii_styles() {
    let theme = ThemePreset::Monochrome.to_theme();
    assert_eq!(theme.box_style, BoxStyle::Ascii);
    assert_eq!(theme.tree_guides, TreeGuides::Ascii);
    assert_eq!(theme.progress_style, ProgressStyle::Ascii);
}

#[test]
fn monochrome_theme_uses_ascii_icons() {
    let theme = ThemePreset::Monochrome.to_theme();
    assert!(theme.icons.success.unicode.is_empty());
}

#[test]
fn light_theme_has_correct_name() {
    let theme = ThemePreset::Light.to_theme();
    assert_eq!(theme.name, "light");
}

#[test]
fn light_theme_is_light_mode() {
    let theme = ThemePreset::Light.to_theme();
    assert!(theme.is_light_mode);
}

#[test]
fn light_theme_uses_unicode_icons() {
    let theme = ThemePreset::Light.to_theme();
    assert!(!theme.icons.success.unicode.is_empty());
}

#[test]
fn auto_preset_returns_valid_theme() {
    // Auto should either return default (dark) or light based on environment
    let theme = ThemePreset::Auto.to_theme();
    // Should be a valid theme regardless of detection
    assert!(!theme.name.is_empty());
}

#[test]
fn from_preset_produces_same_as_to_theme() {
    let preset = ThemePreset::Vibrant;
    let t1 = preset.to_theme();
    let t2 = Theme::from_preset(preset);
    assert_eq!(t1.name, t2.name);
    assert_eq!(t1.box_style, t2.box_style);
    assert_eq!(t1.tree_guides, t2.tree_guides);
    assert_eq!(t1.progress_style, t2.progress_style);
}

#[test]
fn all_presets_produce_valid_themes() {
    let presets = [
        ThemePreset::Default,
        ThemePreset::Minimal,
        ThemePreset::Vibrant,
        ThemePreset::Monochrome,
        ThemePreset::Light,
        ThemePreset::Auto,
    ];

    for preset in presets {
        let theme = preset.to_theme();
        assert!(
            theme.validate().is_ok(),
            "Preset {:?} produced invalid theme",
            preset
        );
    }
}

// ============================================================================
// ThemePreset FromStr Tests
// ============================================================================

#[test]
fn preset_from_str_default() {
    assert_eq!(
        ThemePreset::from_str("default").unwrap(),
        ThemePreset::Default
    );
}

#[test]
fn preset_from_str_minimal() {
    assert_eq!(
        ThemePreset::from_str("minimal").unwrap(),
        ThemePreset::Minimal
    );
}

#[test]
fn preset_from_str_vibrant() {
    assert_eq!(
        ThemePreset::from_str("vibrant").unwrap(),
        ThemePreset::Vibrant
    );
}

#[test]
fn preset_from_str_monochrome() {
    assert_eq!(
        ThemePreset::from_str("monochrome").unwrap(),
        ThemePreset::Monochrome
    );
}

#[test]
fn preset_from_str_light() {
    assert_eq!(ThemePreset::from_str("light").unwrap(), ThemePreset::Light);
}

#[test]
fn preset_from_str_auto() {
    assert_eq!(ThemePreset::from_str("auto").unwrap(), ThemePreset::Auto);
}

#[test]
fn preset_from_str_case_insensitive() {
    assert_eq!(
        ThemePreset::from_str("DEFAULT").unwrap(),
        ThemePreset::Default
    );
    assert_eq!(
        ThemePreset::from_str("MINIMAL").unwrap(),
        ThemePreset::Minimal
    );
    assert_eq!(
        ThemePreset::from_str("Vibrant").unwrap(),
        ThemePreset::Vibrant
    );
}

#[test]
fn preset_from_str_with_hyphens() {
    // normalize_key converts hyphens to underscores, but these don't have underscores
    // Still should work for simple names
    assert_eq!(
        ThemePreset::from_str(" default ").unwrap(),
        ThemePreset::Default
    );
}

#[test]
fn preset_from_str_invalid_returns_error() {
    let result = ThemePreset::from_str("nonexistent");
    assert!(result.is_err());
    match result {
        Err(ThemeError::InvalidPreset(s)) => assert_eq!(s, "nonexistent"),
        _ => panic!("Expected InvalidPreset error"),
    }
}

// ============================================================================
// Theme Colors Tests
// ============================================================================

#[test]
fn default_dark_colors_has_all_semantic_colors() {
    let colors = ThemeColors::default_dark();

    // All semantic colors should be defined (non-null styles)
    // We test by getting each one and ensuring no panic
    assert!(colors.get("success").is_some());
    assert!(colors.get("error").is_some());
    assert!(colors.get("warning").is_some());
    assert!(colors.get("info").is_some());
    assert!(colors.get("hint").is_some());
    assert!(colors.get("debug").is_some());
}

#[test]
fn default_dark_colors_has_content_colors() {
    let colors = ThemeColors::default_dark();
    assert!(colors.get("skill_name").is_some());
    assert!(colors.get("tag").is_some());
    assert!(colors.get("path").is_some());
    assert!(colors.get("url").is_some());
    assert!(colors.get("code").is_some());
    assert!(colors.get("command").is_some());
    assert!(colors.get("version").is_some());
}

#[test]
fn default_dark_colors_has_data_colors() {
    let colors = ThemeColors::default_dark();
    assert!(colors.get("key").is_some());
    assert!(colors.get("value").is_some());
    assert!(colors.get("number").is_some());
    assert!(colors.get("string").is_some());
    assert!(colors.get("boolean").is_some());
    assert!(colors.get("null").is_some());
}

#[test]
fn default_dark_colors_has_layout_colors() {
    let colors = ThemeColors::default_dark();
    assert!(colors.get("header").is_some());
    assert!(colors.get("subheader").is_some());
    assert!(colors.get("border").is_some());
    assert!(colors.get("separator").is_some());
    assert!(colors.get("emphasis").is_some());
    assert!(colors.get("muted").is_some());
    assert!(colors.get("highlight").is_some());
}

#[test]
fn default_dark_colors_has_progress_colors() {
    let colors = ThemeColors::default_dark();
    assert!(colors.get("progress_done").is_some());
    assert!(colors.get("progress_remaining").is_some());
    assert!(colors.get("progress_text").is_some());
    assert!(colors.get("spinner").is_some());
}

#[test]
fn light_colors_has_all_semantic_colors() {
    let colors = ThemeColors::light();
    assert!(colors.get("success").is_some());
    assert!(colors.get("error").is_some());
    assert!(colors.get("warning").is_some());
    assert!(colors.get("info").is_some());
}

#[test]
fn minimal_colors_has_all_semantic_colors() {
    let colors = ThemeColors::minimal();
    assert!(colors.get("success").is_some());
    assert!(colors.get("error").is_some());
    assert!(colors.get("warning").is_some());
}

#[test]
fn vibrant_colors_has_all_semantic_colors() {
    let colors = ThemeColors::vibrant();
    assert!(colors.get("success").is_some());
    assert!(colors.get("error").is_some());
    assert!(colors.get("warning").is_some());
}

#[test]
fn monochrome_colors_has_all_semantic_colors() {
    let colors = ThemeColors::monochrome();
    assert!(colors.get("success").is_some());
    assert!(colors.get("error").is_some());
}

#[test]
fn colors_get_unknown_key_returns_none() {
    let colors = ThemeColors::default_dark();
    assert!(colors.get("nonexistent").is_none());
}

#[test]
fn colors_get_normalizes_key() {
    let colors = ThemeColors::default_dark();
    // Should work with different casing and hyphens
    assert!(colors.get("SUCCESS").is_some());
    assert!(colors.get("skill-name").is_some());
    assert!(colors.get("PROGRESS_DONE").is_some());
}

#[test]
fn colors_set_valid_key() {
    let mut colors = ThemeColors::default_dark();
    let new_style = Style::parse("bold red").unwrap();
    let result = colors.set("success", new_style.clone());
    assert!(result.is_ok());
    // Verify it was set
    assert_eq!(colors.success, new_style);
}

#[test]
fn colors_set_unknown_key_returns_error() {
    let mut colors = ThemeColors::default_dark();
    let style = Style::parse("bold").unwrap();
    let result = colors.set("nonexistent", style);
    assert!(result.is_err());
    match result {
        Err(ThemeError::UnknownColorKey(k)) => assert_eq!(k, "nonexistent"),
        _ => panic!("Expected UnknownColorKey error"),
    }
}

#[test]
fn colors_set_str_valid_style() {
    let mut colors = ThemeColors::default_dark();
    let result = colors.set_str("error", "bold magenta");
    assert!(result.is_ok());
}

#[test]
fn colors_set_str_invalid_style() {
    let mut colors = ThemeColors::default_dark();
    let result = colors.set_str("error", "not a valid style @#$%");
    assert!(result.is_err());
}

#[test]
fn colors_strip_colors_removes_fg_bg() {
    let colors = ThemeColors::default_dark();
    let stripped = colors.strip_colors();

    // Stripping removes foreground and background colors
    // The style should still exist but with no color
    assert!(stripped.success.color.is_none());
    assert!(stripped.error.color.is_none());
}

#[test]
fn colors_downgrade_to_256_preserves_styles() {
    let colors = ThemeColors::default_dark();
    let downgraded = colors.downgrade_to_256();

    // Should still have styles defined
    assert!(downgraded.get("success").is_some());
    assert!(downgraded.get("error").is_some());
}

#[test]
fn colors_downgrade_to_16_preserves_styles() {
    let colors = ThemeColors::default_dark();
    let downgraded = colors.downgrade_to_16();

    // Should still have styles defined
    assert!(downgraded.get("success").is_some());
    assert!(downgraded.get("error").is_some());
}

#[test]
fn colors_for_light_background_adjusts_colors() {
    let colors = ThemeColors::default_dark();
    let light = colors.for_light_background();

    // Light background colors should exist
    assert!(light.get("success").is_some());
    assert!(light.get("error").is_some());
}

#[test]
fn colors_for_dark_background_adjusts_colors() {
    let colors = ThemeColors::light();
    let dark = colors.for_dark_background();

    // Dark background colors should exist
    assert!(dark.get("success").is_some());
    assert!(dark.get("error").is_some());
}

#[test]
fn theme_colors_default_is_dark() {
    let default: ThemeColors = Default::default();
    let dark = ThemeColors::default_dark();
    // Default should be the same as default_dark
    assert_eq!(default.success, dark.success);
}

// ============================================================================
// Theme Icons Tests
// ============================================================================

#[test]
fn unicode_icons_has_all_icons() {
    let icons = ThemeIcons::unicode();
    assert!(!icons.success.unicode.is_empty());
    assert!(!icons.error.unicode.is_empty());
    assert!(!icons.warning.unicode.is_empty());
    assert!(!icons.info.unicode.is_empty());
    assert!(!icons.hint.unicode.is_empty());
    assert!(!icons.skill.unicode.is_empty());
    assert!(!icons.tag.unicode.is_empty());
    assert!(!icons.folder.unicode.is_empty());
    assert!(!icons.file.unicode.is_empty());
    assert!(!icons.search.unicode.is_empty());
    assert!(!icons.loading.unicode.is_empty());
    assert!(!icons.done.unicode.is_empty());
    assert!(!icons.arrow.unicode.is_empty());
    assert!(!icons.bullet.unicode.is_empty());
}

#[test]
fn unicode_icons_has_ascii_fallback() {
    let icons = ThemeIcons::unicode();
    assert!(!icons.success.ascii.is_empty());
    assert!(!icons.error.ascii.is_empty());
    assert!(!icons.warning.ascii.is_empty());
    assert!(!icons.info.ascii.is_empty());
}

#[test]
fn unicode_icons_spinner_frames_non_empty() {
    let icons = ThemeIcons::unicode();
    assert!(
        !icons.spinner_frames.is_empty(),
        "Spinner frames should not be empty"
    );
    // Verify each frame is non-empty
    for (i, frame) in icons.spinner_frames.iter().enumerate() {
        assert!(!frame.is_empty(), "Spinner frame {} should not be empty", i);
    }
}

#[test]
fn ascii_icons_has_empty_unicode() {
    let icons = ThemeIcons::ascii();
    // ASCII icons have empty unicode fields, so select() falls back to ascii
    assert!(icons.success.unicode.is_empty());
    assert!(icons.error.unicode.is_empty());
}

#[test]
fn ascii_icons_has_all_ascii() {
    let icons = ThemeIcons::ascii();
    assert!(!icons.success.ascii.is_empty());
    assert!(!icons.error.ascii.is_empty());
    assert!(!icons.warning.ascii.is_empty());
    assert!(!icons.info.ascii.is_empty());
    assert!(!icons.hint.ascii.is_empty());
}

#[test]
fn ascii_icons_spinner_frames_non_empty() {
    let icons = ThemeIcons::ascii();
    assert!(!icons.spinner_frames.is_empty());
    // ASCII spinner frames: -, \, |, /
    assert_eq!(icons.spinner_frames.len(), 4);
}

#[test]
fn none_icons_has_empty_everything() {
    let icons = ThemeIcons::none();
    assert!(icons.success.unicode.is_empty());
    assert!(icons.success.ascii.is_empty());
    assert!(icons.error.unicode.is_empty());
    assert!(icons.error.ascii.is_empty());
}

#[test]
fn none_icons_spinner_has_one_empty_frame() {
    let icons = ThemeIcons::none();
    assert_eq!(icons.spinner_frames.len(), 1);
    assert!(icons.spinner_frames[0].is_empty());
}

#[test]
fn icon_get_returns_unicode_when_enabled() {
    let icons = ThemeIcons::unicode();
    let icon = icons.get("success", true);
    assert_eq!(icon, icons.success.unicode);
}

#[test]
fn icon_get_returns_ascii_when_disabled() {
    let icons = ThemeIcons::unicode();
    let icon = icons.get("success", false);
    assert_eq!(icon, icons.success.ascii);
}

#[test]
fn icon_get_normalizes_key() {
    let icons = ThemeIcons::unicode();
    // Should work with different casing
    let icon1 = icons.get("SUCCESS", true);
    let icon2 = icons.get("success", true);
    assert_eq!(icon1, icon2);
}

#[test]
fn icon_get_unknown_returns_empty() {
    let icons = ThemeIcons::unicode();
    let icon = icons.get("nonexistent", true);
    assert!(icon.is_empty());
}

#[test]
fn iconset_select_prefers_unicode() {
    let icons = ThemeIcons::unicode();
    // When use_unicode is true and unicode is non-empty, return unicode
    let selected = icons.success.select(true);
    assert_eq!(selected, &icons.success.unicode);
}

#[test]
fn iconset_select_falls_back_to_ascii() {
    let icons = ThemeIcons::unicode();
    // When use_unicode is false, return ascii
    let selected = icons.success.select(false);
    assert_eq!(selected, &icons.success.ascii);
}

#[test]
fn iconset_select_falls_back_when_unicode_empty() {
    let icons = ThemeIcons::ascii();
    // ASCII icons have empty unicode, so select(true) should still return ascii
    let selected = icons.success.select(true);
    assert_eq!(selected, &icons.success.ascii);
}

#[test]
fn theme_icons_default_is_unicode() {
    let default: ThemeIcons = Default::default();
    let unicode = ThemeIcons::unicode();
    assert_eq!(default.success.unicode, unicode.success.unicode);
}

// ============================================================================
// BoxStyle Tests
// ============================================================================

#[test]
fn box_style_rounded_chars() {
    let chars = BoxStyle::Rounded.chars();
    assert_eq!(chars.top_left, "\u{256d}");
    assert_eq!(chars.top_right, "\u{256e}");
    assert_eq!(chars.bottom_left, "\u{2570}");
    assert_eq!(chars.bottom_right, "\u{256f}");
    assert_eq!(chars.horizontal, "\u{2500}");
    assert_eq!(chars.vertical, "\u{2502}");
}

#[test]
fn box_style_square_chars() {
    let chars = BoxStyle::Square.chars();
    assert_eq!(chars.top_left, "\u{250c}");
    assert_eq!(chars.top_right, "\u{2510}");
    assert_eq!(chars.bottom_left, "\u{2514}");
    assert_eq!(chars.bottom_right, "\u{2518}");
}

#[test]
fn box_style_heavy_chars() {
    let chars = BoxStyle::Heavy.chars();
    assert_eq!(chars.top_left, "\u{250f}");
    assert_eq!(chars.horizontal, "\u{2501}");
    assert_eq!(chars.vertical, "\u{2503}");
}

#[test]
fn box_style_double_chars() {
    let chars = BoxStyle::Double.chars();
    assert_eq!(chars.top_left, "\u{2554}");
    assert_eq!(chars.horizontal, "\u{2550}");
    assert_eq!(chars.vertical, "\u{2551}");
}

#[test]
fn box_style_ascii_chars() {
    let chars = BoxStyle::Ascii.chars();
    assert_eq!(chars.top_left, "+");
    assert_eq!(chars.top_right, "+");
    assert_eq!(chars.bottom_left, "+");
    assert_eq!(chars.bottom_right, "+");
    assert_eq!(chars.horizontal, "-");
    assert_eq!(chars.vertical, "|");
}

#[test]
fn box_style_none_chars() {
    let chars = BoxStyle::None.chars();
    assert!(chars.top_left.is_empty());
    assert!(chars.horizontal.is_empty());
    assert!(chars.vertical.is_empty());
}

#[test]
fn box_style_from_str_all_variants() {
    assert_eq!(BoxStyle::from_str("rounded").unwrap(), BoxStyle::Rounded);
    assert_eq!(BoxStyle::from_str("square").unwrap(), BoxStyle::Square);
    assert_eq!(BoxStyle::from_str("heavy").unwrap(), BoxStyle::Heavy);
    assert_eq!(BoxStyle::from_str("double").unwrap(), BoxStyle::Double);
    assert_eq!(BoxStyle::from_str("ascii").unwrap(), BoxStyle::Ascii);
    assert_eq!(BoxStyle::from_str("none").unwrap(), BoxStyle::None);
}

#[test]
fn box_style_from_str_case_insensitive() {
    assert_eq!(BoxStyle::from_str("ROUNDED").unwrap(), BoxStyle::Rounded);
    assert_eq!(BoxStyle::from_str("Square").unwrap(), BoxStyle::Square);
}

#[test]
fn box_style_from_str_invalid() {
    let result = BoxStyle::from_str("invalid");
    assert!(result.is_err());
    match result {
        Err(ThemeError::InvalidBoxStyle(s)) => assert_eq!(s, "invalid"),
        _ => panic!("Expected InvalidBoxStyle error"),
    }
}

#[test]
fn box_style_default_is_rounded() {
    let default: BoxStyle = Default::default();
    assert_eq!(default, BoxStyle::Rounded);
}

// ============================================================================
// TreeGuides Tests
// ============================================================================

#[test]
fn tree_guides_unicode_chars() {
    let chars = TreeGuides::Unicode.chars();
    assert_eq!(chars.vertical, "\u{2502}");
    assert_eq!(chars.branch, "\u{251c}");
    assert_eq!(chars.last, "\u{2514}");
    assert_eq!(chars.horizontal, "\u{2500}");
}

#[test]
fn tree_guides_rounded_chars() {
    let chars = TreeGuides::Rounded.chars();
    assert_eq!(chars.vertical, "\u{2502}");
    assert_eq!(chars.branch, "\u{251c}");
    assert_eq!(chars.last, "\u{2570}"); // Rounded corner
    assert_eq!(chars.horizontal, "\u{2500}");
}

#[test]
fn tree_guides_ascii_chars() {
    let chars = TreeGuides::Ascii.chars();
    assert_eq!(chars.vertical, "|");
    assert_eq!(chars.branch, "+");
    assert_eq!(chars.last, "`");
    assert_eq!(chars.horizontal, "-");
}

#[test]
fn tree_guides_bold_chars() {
    let chars = TreeGuides::Bold.chars();
    assert_eq!(chars.vertical, "\u{2503}");
    assert_eq!(chars.branch, "\u{2523}");
    assert_eq!(chars.last, "\u{2517}");
    assert_eq!(chars.horizontal, "\u{2501}");
}

#[test]
fn tree_guides_from_str_all_variants() {
    assert_eq!(
        TreeGuides::from_str("unicode").unwrap(),
        TreeGuides::Unicode
    );
    assert_eq!(
        TreeGuides::from_str("rounded").unwrap(),
        TreeGuides::Rounded
    );
    assert_eq!(TreeGuides::from_str("ascii").unwrap(), TreeGuides::Ascii);
    assert_eq!(TreeGuides::from_str("bold").unwrap(), TreeGuides::Bold);
}

#[test]
fn tree_guides_from_str_case_insensitive() {
    assert_eq!(
        TreeGuides::from_str("UNICODE").unwrap(),
        TreeGuides::Unicode
    );
    assert_eq!(TreeGuides::from_str("Ascii").unwrap(), TreeGuides::Ascii);
}

#[test]
fn tree_guides_from_str_invalid() {
    let result = TreeGuides::from_str("invalid");
    assert!(result.is_err());
    match result {
        Err(ThemeError::InvalidTreeGuides(s)) => assert_eq!(s, "invalid"),
        _ => panic!("Expected InvalidTreeGuides error"),
    }
}

#[test]
fn tree_guides_default_is_unicode() {
    let default: TreeGuides = Default::default();
    assert_eq!(default, TreeGuides::Unicode);
}

// ============================================================================
// ProgressStyle Tests
// ============================================================================

#[test]
fn progress_style_block_chars() {
    let chars = ProgressStyle::Block.chars();
    assert_eq!(chars.filled, "\u{2588}"); // Full block
    assert_eq!(chars.empty, "\u{2591}"); // Light shade
}

#[test]
fn progress_style_ascii_chars() {
    let chars = ProgressStyle::Ascii.chars();
    assert_eq!(chars.filled, "#");
    assert_eq!(chars.empty, "-");
}

#[test]
fn progress_style_line_chars() {
    let chars = ProgressStyle::Line.chars();
    assert_eq!(chars.filled, "\u{2501}"); // Heavy horizontal
    assert_eq!(chars.empty, "\u{2500}"); // Light horizontal
}

#[test]
fn progress_style_dots_chars() {
    let chars = ProgressStyle::Dots.chars();
    assert_eq!(chars.filled, "\u{25cf}"); // Black circle
    assert_eq!(chars.empty, "\u{25cb}"); // White circle
}

#[test]
fn progress_style_from_str_all_variants() {
    assert_eq!(
        ProgressStyle::from_str("block").unwrap(),
        ProgressStyle::Block
    );
    assert_eq!(
        ProgressStyle::from_str("ascii").unwrap(),
        ProgressStyle::Ascii
    );
    assert_eq!(
        ProgressStyle::from_str("line").unwrap(),
        ProgressStyle::Line
    );
    assert_eq!(
        ProgressStyle::from_str("dots").unwrap(),
        ProgressStyle::Dots
    );
}

#[test]
fn progress_style_from_str_case_insensitive() {
    assert_eq!(
        ProgressStyle::from_str("BLOCK").unwrap(),
        ProgressStyle::Block
    );
    assert_eq!(
        ProgressStyle::from_str("Dots").unwrap(),
        ProgressStyle::Dots
    );
}

#[test]
fn progress_style_from_str_invalid() {
    let result = ProgressStyle::from_str("invalid");
    assert!(result.is_err());
    match result {
        Err(ThemeError::InvalidProgressStyle(s)) => assert_eq!(s, "invalid"),
        _ => panic!("Expected InvalidProgressStyle error"),
    }
}

#[test]
fn progress_style_default_is_block() {
    let default: ProgressStyle = Default::default();
    assert_eq!(default, ProgressStyle::Block);
}

// ============================================================================
// Theme Validation Tests
// ============================================================================

#[test]
fn valid_theme_passes_validation() {
    let theme = Theme::default();
    assert!(theme.validate().is_ok());
}

#[test]
fn all_preset_themes_pass_validation() {
    for preset in [
        ThemePreset::Default,
        ThemePreset::Minimal,
        ThemePreset::Vibrant,
        ThemePreset::Monochrome,
        ThemePreset::Light,
    ] {
        let theme = preset.to_theme();
        assert!(
            theme.validate().is_ok(),
            "Preset {:?} failed validation",
            preset
        );
    }
}

#[test]
fn theme_with_empty_spinner_frames_fails_validation() {
    let mut theme = Theme::default();
    theme.icons.spinner_frames = Vec::new();
    let result = theme.validate();
    assert!(result.is_err());
    let errors = result.unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| matches!(e, ThemeError::EmptySpinnerFrames))
    );
}

// ============================================================================
// Theme Terminal Adaptation Tests
// ============================================================================

#[test]
fn theme_with_ascii_fallback_uses_ascii_styles() {
    let theme = Theme::default().with_ascii_fallback();
    assert_eq!(theme.box_style, BoxStyle::Ascii);
    assert_eq!(theme.tree_guides, TreeGuides::Ascii);
    assert_eq!(theme.progress_style, ProgressStyle::Ascii);
}

#[test]
fn theme_adapted_for_no_color_strips_colors() {
    let caps = TerminalCapabilities {
        color_system: None,
        supports_unicode: true,
        supports_hyperlinks: false,
    };
    let theme = Theme::default().adapted_for_terminal(&caps);

    // Colors should be stripped
    assert!(theme.colors.success.color.is_none());
}

#[test]
fn theme_adapted_for_truecolor_preserves_colors() {
    let caps = TerminalCapabilities {
        color_system: Some(ColorSystem::TrueColor),
        supports_unicode: true,
        supports_hyperlinks: false,
    };
    let theme = Theme::default().adapted_for_terminal(&caps);

    // Should keep full color support (theme retains original colors)
    // At minimum the theme should be valid
    assert!(theme.validate().is_ok());
}

#[test]
fn theme_adapted_for_256_colors_downgrades() {
    let caps = TerminalCapabilities {
        color_system: Some(ColorSystem::EightBit),
        supports_unicode: true,
        supports_hyperlinks: false,
    };
    let theme = Theme::default().adapted_for_terminal(&caps);

    // Should still be valid after downgrade
    assert!(theme.validate().is_ok());
}

#[test]
fn theme_adapted_for_16_colors_downgrades() {
    let caps = TerminalCapabilities {
        color_system: Some(ColorSystem::Standard),
        supports_unicode: true,
        supports_hyperlinks: false,
    };
    let theme = Theme::default().adapted_for_terminal(&caps);

    // Should still be valid after downgrade
    assert!(theme.validate().is_ok());
}

#[test]
fn theme_adapted_for_no_unicode_uses_ascii() {
    let caps = TerminalCapabilities {
        color_system: Some(ColorSystem::TrueColor),
        supports_unicode: false,
        supports_hyperlinks: false,
    };
    let theme = Theme::default().adapted_for_terminal(&caps);

    // Should use ASCII fallbacks
    assert_eq!(theme.box_style, BoxStyle::Ascii);
    assert_eq!(theme.tree_guides, TreeGuides::Ascii);
    assert_eq!(theme.progress_style, ProgressStyle::Ascii);
}

#[test]
fn theme_adapted_for_dumb_terminal() {
    let caps = TerminalCapabilities {
        color_system: None,
        supports_unicode: false,
        supports_hyperlinks: false,
    };
    let theme = Theme::default().adapted_for_terminal(&caps);

    // Should strip colors and use ASCII
    assert!(theme.colors.success.color.is_none());
    assert_eq!(theme.box_style, BoxStyle::Ascii);
}

// ============================================================================
// Theme Color Override Tests
// ============================================================================

#[test]
fn theme_with_color_override_sets_color() {
    let new_style = Style::parse("bold red").unwrap();
    let theme = Theme::default().with_color_override("success", new_style.clone());
    assert_eq!(theme.colors.success, new_style);
}

#[test]
fn theme_with_color_override_invalid_key_ignored() {
    let new_style = Style::parse("bold").unwrap();
    // Invalid key should be ignored (no panic)
    let theme = Theme::default().with_color_override("nonexistent", new_style);
    // Theme should still be valid
    assert!(theme.validate().is_ok());
}

// ============================================================================
// Terminal Background Tests
// ============================================================================

#[test]
fn terminal_background_variants_exist() {
    // Just verify the enum variants compile
    let _light = TerminalBackground::Light;
    let _dark = TerminalBackground::Dark;
    let _unknown = TerminalBackground::Unknown;
}

#[test]
fn terminal_capabilities_struct_works() {
    let caps = TerminalCapabilities {
        color_system: Some(ColorSystem::TrueColor),
        supports_unicode: true,
        supports_hyperlinks: false,
    };
    assert_eq!(caps.color_system, Some(ColorSystem::TrueColor));
    assert!(caps.supports_unicode);
    assert!(!caps.supports_hyperlinks);
}

#[test]
fn terminal_capabilities_none_color() {
    let caps = TerminalCapabilities {
        color_system: None,
        supports_unicode: true,
        supports_hyperlinks: false,
    };
    assert!(caps.color_system.is_none());
}

// ============================================================================
// Theme Serialization Tests
// ============================================================================

#[test]
fn theme_serializes_to_json() {
    let theme = Theme::default();
    let json = serde_json::to_string(&theme);
    assert!(json.is_ok());
    let json = json.unwrap();
    assert!(json.contains("\"name\":\"default\""));
}

#[test]
fn theme_deserializes_from_json() {
    let theme = Theme::default();
    let json = serde_json::to_string(&theme).unwrap();
    let deserialized: Theme = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.name, theme.name);
    assert_eq!(deserialized.box_style, theme.box_style);
    assert_eq!(deserialized.tree_guides, theme.tree_guides);
    assert_eq!(deserialized.progress_style, theme.progress_style);
}

#[test]
fn theme_preset_serializes_correctly() {
    let preset = ThemePreset::Vibrant;
    let json = serde_json::to_string(&preset).unwrap();
    assert_eq!(json, "\"vibrant\"");
}

#[test]
fn theme_preset_deserializes_correctly() {
    let preset: ThemePreset = serde_json::from_str("\"minimal\"").unwrap();
    assert_eq!(preset, ThemePreset::Minimal);
}

#[test]
fn box_style_serializes_correctly() {
    let style = BoxStyle::Double;
    let json = serde_json::to_string(&style).unwrap();
    assert_eq!(json, "\"double\"");
}

#[test]
fn box_style_deserializes_correctly() {
    let style: BoxStyle = serde_json::from_str("\"heavy\"").unwrap();
    assert_eq!(style, BoxStyle::Heavy);
}

#[test]
fn tree_guides_serializes_correctly() {
    let guides = TreeGuides::Rounded;
    let json = serde_json::to_string(&guides).unwrap();
    assert_eq!(json, "\"rounded\"");
}

#[test]
fn tree_guides_deserializes_correctly() {
    let guides: TreeGuides = serde_json::from_str("\"bold\"").unwrap();
    assert_eq!(guides, TreeGuides::Bold);
}

#[test]
fn progress_style_serializes_correctly() {
    let style = ProgressStyle::Line;
    let json = serde_json::to_string(&style).unwrap();
    assert_eq!(json, "\"line\"");
}

#[test]
fn progress_style_deserializes_correctly() {
    let style: ProgressStyle = serde_json::from_str("\"dots\"").unwrap();
    assert_eq!(style, ProgressStyle::Dots);
}

// ============================================================================
// BoxChars, TreeChars, ProgressChars Equality Tests
// ============================================================================

#[test]
fn box_chars_equality() {
    let chars1 = BoxStyle::Rounded.chars();
    let chars2 = BoxStyle::Rounded.chars();
    assert_eq!(chars1, chars2);

    let chars3 = BoxStyle::Square.chars();
    assert_ne!(chars1, chars3);
}

#[test]
fn tree_chars_equality() {
    let chars1 = TreeGuides::Unicode.chars();
    let chars2 = TreeGuides::Unicode.chars();
    assert_eq!(chars1, chars2);

    let chars3 = TreeGuides::Ascii.chars();
    assert_ne!(chars1, chars3);
}

#[test]
fn progress_chars_equality() {
    let chars1 = ProgressStyle::Block.chars();
    let chars2 = ProgressStyle::Block.chars();
    assert_eq!(chars1, chars2);

    let chars3 = ProgressStyle::Ascii.chars();
    assert_ne!(chars1, chars3);
}

// ============================================================================
// All Semantic Colors Defined (Comprehensive Check)
// ============================================================================

const ALL_SEMANTIC_COLOR_KEYS: &[&str] = &[
    "success",
    "error",
    "warning",
    "info",
    "hint",
    "debug",
    "skill_name",
    "tag",
    "path",
    "url",
    "code",
    "command",
    "version",
    "key",
    "value",
    "number",
    "string",
    "boolean",
    "null",
    "header",
    "subheader",
    "border",
    "separator",
    "emphasis",
    "muted",
    "highlight",
    "progress_done",
    "progress_remaining",
    "progress_text",
    "spinner",
];

#[test]
fn default_dark_has_all_30_semantic_colors() {
    let colors = ThemeColors::default_dark();
    for key in ALL_SEMANTIC_COLOR_KEYS {
        assert!(
            colors.get(key).is_some(),
            "default_dark missing color: {}",
            key
        );
    }
}

#[test]
fn light_has_all_30_semantic_colors() {
    let colors = ThemeColors::light();
    for key in ALL_SEMANTIC_COLOR_KEYS {
        assert!(colors.get(key).is_some(), "light missing color: {}", key);
    }
}

#[test]
fn minimal_has_all_30_semantic_colors() {
    let colors = ThemeColors::minimal();
    for key in ALL_SEMANTIC_COLOR_KEYS {
        assert!(colors.get(key).is_some(), "minimal missing color: {}", key);
    }
}

#[test]
fn vibrant_has_all_30_semantic_colors() {
    let colors = ThemeColors::vibrant();
    for key in ALL_SEMANTIC_COLOR_KEYS {
        assert!(colors.get(key).is_some(), "vibrant missing color: {}", key);
    }
}

#[test]
fn monochrome_has_all_30_semantic_colors() {
    let colors = ThemeColors::monochrome();
    for key in ALL_SEMANTIC_COLOR_KEYS {
        assert!(
            colors.get(key).is_some(),
            "monochrome missing color: {}",
            key
        );
    }
}

// ============================================================================
// All Icons Defined (Comprehensive Check)
// ============================================================================

const ALL_ICON_KEYS: &[&str] = &[
    "success", "error", "warning", "info", "hint", "skill", "tag", "folder", "file", "search",
    "loading", "done", "arrow", "bullet",
];

#[test]
fn unicode_icons_has_all_14_icons() {
    let icons = ThemeIcons::unicode();
    for key in ALL_ICON_KEYS {
        let icon = icons.get(key, true);
        assert!(!icon.is_empty(), "unicode icons missing: {}", key);
    }
}

#[test]
fn ascii_icons_has_all_14_icons() {
    let icons = ThemeIcons::ascii();
    for key in ALL_ICON_KEYS {
        let icon = icons.get(key, false);
        assert!(!icon.is_empty(), "ascii icons missing: {}", key);
    }
}
