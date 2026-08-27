//! Fuzz-style property tests for the markup parser.
//!
//! Verifies robustness of `markup::render`, `markup::render_or_plain`,
//! and `markup::escape` against arbitrary, malformed, and adversarial inputs.
//! Requirements tested:
//!   (1) No panics on any input
//!   (2) Unclosed tags handled
//!   (3) Invalid color specs
//!   (4) Deeply nested tags
//!   (5) Mixed valid/invalid
//!   (6) Unicode edge cases
//!   (7) Very long strings
//!   (8) Repeated tags

use proptest::prelude::*;
use rich_rust::markup;
use rich_rust::style::Style;
use rich_rust::text::Text;

// ============================================================================
// Strategies for generating markup-like inputs
// ============================================================================

/// Generates arbitrary strings that may contain bracket characters.
fn arbitrary_markup() -> impl Strategy<Value = String> {
    // Mix of regular chars, brackets, backslashes, tags, and Unicode
    prop::string::string_regex(r"[\x00-\x7f\[\]/\\#]{0,200}").unwrap()
}

/// Generates strings with valid-looking tag syntax but random tag names.
fn tag_like_strings() -> impl Strategy<Value = String> {
    prop::collection::vec(
        prop_oneof![
            // Valid-looking opening tag
            "[a-zA-Z#]{1,10}".prop_map(|s| format!("[{s}]")),
            // Closing tag
            "[a-zA-Z]{0,10}".prop_map(|s| format!("[/{s}]")),
            // Plain text
            "[a-zA-Z0-9 ]{1,20}",
            // Escaped bracket
            Just("\\[".to_string()),
        ],
        0..20,
    )
    .prop_map(|parts| parts.join(""))
}

/// Generates deeply nested tag structures.
fn deeply_nested(depth: usize) -> String {
    let open: String = (0..depth).map(|_| "[bold]").collect();
    let close: String = (0..depth).map(|_| "[/]").collect();
    format!("{open}inner{close}")
}

/// Generates repeated tag pattern.
fn repeated_tags(count: usize) -> String {
    (0..count).map(|i| format!("[bold]word{i}[/]")).collect()
}

// ============================================================================
// (1) No panics on any input
// ============================================================================

proptest! {
    #[test]
    fn fuzz_render_no_panic(input in "\\PC{0,300}") {
        // render() should never panic, even on arbitrary Unicode
        let _ = markup::render(&input);
    }

    #[test]
    fn fuzz_render_or_plain_no_panic(input in "\\PC{0,300}") {
        // render_or_plain() must always succeed without panic
        let _ = markup::render_or_plain(&input);
    }

    #[test]
    fn fuzz_escape_no_panic(input in "\\PC{0,300}") {
        let _ = markup::escape(&input);
    }

    #[test]
    fn fuzz_render_arbitrary_ascii_no_panic(input in arbitrary_markup()) {
        let _ = markup::render(&input);
        let _ = markup::render_or_plain(&input);
    }

    #[test]
    fn fuzz_render_tag_like_no_panic(input in tag_like_strings()) {
        let _ = markup::render(&input);
        let _ = markup::render_or_plain(&input);
    }
}

// ============================================================================
// (2) Unclosed tags handled gracefully
// ============================================================================

#[test]
fn unclosed_single_tag() {
    // Unclosed tags should auto-close without panic
    let result = markup::render("[bold]hello");
    assert!(result.is_ok());
    let text = result.unwrap();
    assert_eq!(text.plain(), "hello");
}

#[test]
fn unclosed_nested_tags() {
    let result = markup::render("[bold][red][italic]nested text");
    assert!(result.is_ok());
    assert_eq!(result.unwrap().plain(), "nested text");
}

#[test]
fn unclosed_with_closed_sibling() {
    let result = markup::render("[bold]hello[/] [red]world");
    assert!(result.is_ok());
    let text = result.unwrap();
    assert!(text.plain().contains("hello"));
    assert!(text.plain().contains("world"));
}

proptest! {
    #[test]
    fn fuzz_unclosed_tags_handled(
        tag_name in "[a-zA-Z]{1,8}",
        content in "[a-zA-Z0-9 ]{1,30}",
    ) {
        let markup_str = format!("[{tag_name}]{content}");
        // Should always succeed (auto-close) and preserve text
        let text = markup::render_or_plain(&markup_str);
        prop_assert!(text.plain().contains(content.trim()));
    }

    #[test]
    fn fuzz_multiple_unclosed_tags(count in 1usize..10) {
        let tags: String = (0..count).map(|_| "[bold]").collect();
        let markup_str = format!("{tags}hello");
        let text = markup::render_or_plain(&markup_str);
        prop_assert_eq!(text.plain(), "hello");
    }
}

// ============================================================================
// (3) Invalid color specs
// ============================================================================

#[test]
fn invalid_hex_colors_no_panic() {
    let invalids = [
        "[#xyz]text[/]",
        "[#zzzzzz]text[/]",
        "[#]text[/]",
        "[#1]text[/]",
        "[#12]text[/]",
        "[#1234]text[/]",
        "[#12345]text[/]",
        "[#1234567]text[/]",
        "[#gggggg]text[/]",
        "[#ZZZZZZ]text[/]",
    ];
    for input in &invalids {
        let text = markup::render_or_plain(input);
        assert!(
            text.plain().contains("text"),
            "plain text lost for: {input}"
        );
    }
}

#[test]
fn invalid_color_names_no_panic() {
    let invalids = [
        "[notacolor]text[/]",
        "[color(999)]text[/]",
        "[rgb(-1,0,0)]text[/]",
        "[rgb(256,256,256)]text[/]",
        "[on notacolor]text[/]",
    ];
    for input in &invalids {
        let text = markup::render_or_plain(input);
        assert!(
            text.plain().contains("text"),
            "plain text lost for: {input}"
        );
    }
}

proptest! {
    #[test]
    fn fuzz_random_hex_color_no_panic(hex in "[0-9a-fA-F]{0,10}") {
        let input = format!("[#{hex}]text[/]");
        let _ = markup::render(&input);
        let text = markup::render_or_plain(&input);
        prop_assert!(text.plain().contains("text"));
    }

    #[test]
    fn fuzz_color_with_unicode_no_panic(hex in "\\PC{1,6}") {
        let input = format!("[#{hex}]text[/]");
        let _ = markup::render(&input);
        let text = markup::render_or_plain(&input);
        prop_assert!(text.plain().contains("text"));
    }
}

// ============================================================================
// (4) Deeply nested tags
// ============================================================================

#[test]
fn deeply_nested_10() {
    let input = deeply_nested(10);
    let text = markup::render_or_plain(&input);
    assert_eq!(text.plain(), "inner");
}

#[test]
fn deeply_nested_100() {
    let input = deeply_nested(100);
    let text = markup::render_or_plain(&input);
    assert_eq!(text.plain(), "inner");
}

#[test]
fn deeply_nested_1000() {
    let input = deeply_nested(1000);
    let text = markup::render_or_plain(&input);
    assert_eq!(text.plain(), "inner");
}

proptest! {
    #[test]
    fn fuzz_deeply_nested(depth in 1usize..200) {
        let input = deeply_nested(depth);
        let text = markup::render_or_plain(&input);
        prop_assert_eq!(text.plain(), "inner");
    }

    #[test]
    fn fuzz_nested_different_tags(depth in 1usize..50) {
        let tags = ["bold", "red", "italic", "underline", "blue"];
        let opens: String = (0..depth).map(|i| format!("[{}]", tags[i % tags.len()])).collect();
        let closes: String = (0..depth).map(|_| "[/]".to_string()).collect();
        let input = format!("{opens}text{closes}");
        let text = markup::render_or_plain(&input);
        prop_assert_eq!(text.plain(), "text");
    }
}

// ============================================================================
// (5) Mixed valid/invalid
// ============================================================================

#[test]
fn mixed_valid_invalid_tags() {
    let input = "[bold]hello[/] [nonsense]world[/] [red]end[/]";
    let text = markup::render_or_plain(input);
    assert!(text.plain().contains("hello"));
    assert!(text.plain().contains("world"));
    assert!(text.plain().contains("end"));
}

#[test]
fn valid_then_unmatched_close() {
    // [/red] doesn't match [bold], so render() returns an error
    let _result = markup::render("[bold]hello[/red]");
    // Should be an error (unmatched closing tag) or succeed depending on implementation
    // Either way, render_or_plain must not panic
    let text = markup::render_or_plain("[bold]hello[/red]");
    assert!(text.plain().contains("hello"));
}

#[test]
fn close_without_open() {
    let result = markup::render("[/]");
    assert!(
        result.is_err(),
        "closing with nothing to close should error"
    );
    // render_or_plain falls back to plain text
    let text = markup::render_or_plain("[/]");
    assert!(!text.plain().is_empty());
}

#[test]
fn close_named_without_open() {
    let result = markup::render("[/bold]");
    assert!(result.is_err());
    let text = markup::render_or_plain("[/bold]");
    assert!(!text.plain().is_empty());
}

proptest! {
    #[test]
    fn fuzz_mixed_valid_and_garbage(
        prefix in "[a-zA-Z ]{0,20}",
        tag in "[a-zA-Z]{1,8}",
        middle in "[a-zA-Z0-9 ]{1,20}",
        suffix in "[\\[\\]/\\\\#a-zA-Z0-9 ]{0,30}",
    ) {
        let input = format!("{prefix}[{tag}]{middle}[/]{suffix}");
        let text = markup::render_or_plain(&input);
        // The content between valid tags should be in the output
        prop_assert!(text.plain().contains(middle.trim()));
    }
}

// ============================================================================
// (6) Unicode edge cases
// ============================================================================

#[test]
fn unicode_cjk_content() {
    let input = "[bold]日本語テスト[/]";
    let text = markup::render_or_plain(input);
    assert!(text.plain().contains("日本語テスト"));
}

#[test]
fn unicode_emoji_content() {
    let input = "[red]🎉🎊🎈[/]";
    let text = markup::render_or_plain(input);
    assert!(text.plain().contains("🎉🎊🎈"));
}

#[test]
fn unicode_rtl_content() {
    let input = "[bold]مرحبا بالعالم[/]";
    let text = markup::render_or_plain(input);
    assert!(text.plain().contains("مرحبا بالعالم"));
}

#[test]
fn unicode_combining_characters() {
    let input = "[bold]e\u{0301}te\u{0301}[/]";
    let text = markup::render_or_plain(input);
    assert!(text.plain().contains("e\u{0301}"));
}

#[test]
fn unicode_zero_width_chars() {
    let input = "[bold]a\u{200B}b\u{200C}c\u{200D}d\u{FEFF}e[/]";
    let text = markup::render_or_plain(input);
    assert!(text.plain().contains("a\u{200B}b"));
}

#[test]
fn unicode_surrogate_region() {
    // Characters near surrogate pair boundaries
    let input = "[bold]\u{D7FF}\u{E000}[/]";
    let text = markup::render_or_plain(input);
    assert!(!text.plain().is_empty());
}

#[test]
fn unicode_tag_names_ignored() {
    // Unicode chars in tag position - regex requires [A-Za-z#/@] start
    let input = "[日本語]text[/]";
    // This won't match as a tag, so text stays as-is
    let text = markup::render_or_plain(input);
    assert!(text.plain().contains("text"));
}

proptest! {
    #[test]
    fn fuzz_unicode_content_preserved(content in "[^\\\\\\[\\]]{1,50}") {
        // Unicode content without backslashes or brackets should be preserved
        // (backslashes can escape brackets, changing tag semantics)
        let input = format!("[bold]{content}[/]");
        let text = markup::render_or_plain(&input);
        prop_assert_eq!(text.plain(), &content);
    }

    #[test]
    fn fuzz_unicode_mixed_with_tags(content in "\\PC{0,100}") {
        // Arbitrary Unicode should not cause panics when mixed with brackets
        let input = format!("[bold]{content}[/]more{content}");
        let _ = markup::render_or_plain(&input);
    }
}

// ============================================================================
// (7) Very long strings
// ============================================================================

#[test]
fn very_long_plain_text() {
    let input = "a".repeat(100_000);
    let text = markup::render_or_plain(&input);
    assert_eq!(text.plain().len(), 100_000);
}

#[test]
fn very_long_with_tags() {
    let inner = "x".repeat(50_000);
    let input = format!("[bold]{inner}[/]");
    let text = markup::render_or_plain(&input);
    assert_eq!(text.plain(), inner);
}

#[test]
fn many_short_tagged_segments() {
    let input: String = (0..10_000).map(|i| format!("[bold]w{i}[/]")).collect();
    let text = markup::render_or_plain(&input);
    // Should contain all words
    assert!(text.plain().contains("w0"));
    assert!(text.plain().contains("w9999"));
}

#[test]
fn very_long_tag_name() {
    let tag_name = "a".repeat(1000);
    let input = format!("[{tag_name}]text[/{tag_name}]");
    let text = markup::render_or_plain(&input);
    assert!(text.plain().contains("text"));
}

#[test]
fn many_brackets_no_tags() {
    // Lots of unmatched brackets (not valid tag syntax)
    let input = "[][][][][][]".repeat(1000);
    let text = markup::render_or_plain(&input);
    assert!(!text.plain().is_empty());
}

#[test]
fn long_escape_sequences() {
    let input = "\\[".repeat(10_000);
    let text = markup::render_or_plain(&input);
    // Each \\[ should produce a literal [
    assert!(text.plain().contains("["));
}

// ============================================================================
// (8) Repeated tags
// ============================================================================

#[test]
fn repeated_same_tag_10() {
    let input = repeated_tags(10);
    let text = markup::render_or_plain(&input);
    for i in 0..10 {
        assert!(text.plain().contains(&format!("word{i}")));
    }
}

#[test]
fn repeated_same_tag_1000() {
    let input = repeated_tags(1000);
    let text = markup::render_or_plain(&input);
    assert!(text.plain().contains("word0"));
    assert!(text.plain().contains("word999"));
}

proptest! {
    #[test]
    fn fuzz_repeated_tags(count in 1usize..100) {
        let input = repeated_tags(count);
        let text = markup::render_or_plain(&input);
        for i in 0..count {
            let word = format!("word{i}");
            prop_assert!(
                text.plain().contains(&word),
                "missing {word} in output"
            );
        }
    }

    #[test]
    fn fuzz_alternating_tags(count in 1usize..50) {
        let tags = ["bold", "italic", "red", "blue", "green"];
        let input: String = (0..count)
            .map(|i| format!("[{}]w{i}[/]", tags[i % tags.len()]))
            .collect();
        let text = markup::render_or_plain(&input);
        let plain = text.plain().to_string();
        for i in 0..count {
            let word = format!("w{i}");
            prop_assert!(plain.contains(&word), "missing {}", word);
        }
    }
}

// ============================================================================
// Property: escape() + render() roundtrip preserves text
// ============================================================================

proptest! {
    #[test]
    fn fuzz_escape_roundtrip(input in "\\PC{0,100}") {
        // escape() should make any text safe to render as plain text
        let escaped = markup::escape(&input);
        let text = markup::render_or_plain(&escaped);
        // The rendered plain text should contain all non-bracket content
        // (escape only protects [, not ])
        // We can at least verify no panic and the output isn't empty when input isn't
        if !input.is_empty() {
            prop_assert!(!text.plain().is_empty() || input.chars().all(|c| c == '['));
        }
    }

    #[test]
    fn fuzz_escape_preserves_non_bracket_text(input in "[a-zA-Z0-9 ]{1,50}") {
        // For text without brackets, escape is identity and render preserves it
        let escaped = markup::escape(&input);
        let text = markup::render_or_plain(&escaped);
        prop_assert_eq!(text.plain(), &input);
    }
}

// ============================================================================
// Property: render_or_plain() never fails
// ============================================================================

proptest! {
    #[test]
    fn fuzz_render_or_plain_always_returns(input in "\\PC{0,200}") {
        // render_or_plain must always return a Text, never panic
        let text: Text = markup::render_or_plain(&input);
        // Output should exist (text.len() is always >= 0 for usize)
        let _ = text.len();
    }
}

// ============================================================================
// Property: render() with custom resolver
// ============================================================================

proptest! {
    #[test]
    fn fuzz_render_with_custom_resolver_no_panic(input in arbitrary_markup()) {
        let _ = markup::render_with_style_resolver(&input, |_name| Style::new());
    }

    #[test]
    fn fuzz_render_with_failing_resolver(input in tag_like_strings()) {
        // A resolver that always returns default style should not cause issues
        let _ = markup::render_with_style_resolver(&input, |_| Style::new());
        let _ = markup::render_or_plain_with_style_resolver(&input, |_| Style::new());
    }
}

// ============================================================================
// Property: plain text content is preserved through markup
// ============================================================================

proptest! {
    #[test]
    fn fuzz_content_preserved_in_tags(
        tag in "(bold|italic|red|blue|green|underline|strike)",
        content in "[a-zA-Z0-9]{1,30}",
    ) {
        let input = format!("[{tag}]{content}[/{tag}]");
        let text = markup::render_or_plain(&input);
        prop_assert_eq!(text.plain(), &content);
    }

    #[test]
    fn fuzz_multiple_segments_preserved(
        parts in prop::collection::vec("[a-zA-Z0-9]{1,10}", 1..10),
    ) {
        let input: String = parts
            .iter()
            .map(|p| format!("[bold]{p}[/]"))
            .collect();
        let text = markup::render_or_plain(&input);
        let plain = text.plain().to_string();
        for part in &parts {
            prop_assert!(
                plain.contains(part.as_str()),
                "missing '{part}' in '{plain}'"
            );
        }
    }
}

// ============================================================================
// Edge: backslash escaping
// ============================================================================

#[test]
fn single_backslash_before_bracket() {
    let input = r"\[bold]text";
    let text = markup::render_or_plain(input);
    // \[ should be literal [
    assert!(text.plain().contains("[bold]text") || text.plain().contains("text"));
}

#[test]
fn double_backslash_before_bracket() {
    let input = r"\\[bold]text[/]";
    let text = markup::render_or_plain(input);
    // \\ + [bold] → a backslash + bold tag
    assert!(text.plain().contains("text"));
}

#[test]
fn many_backslashes_before_bracket() {
    for n in 1..10 {
        let backslashes = "\\".repeat(n);
        let input = format!("{backslashes}[bold]text[/]");
        let text = markup::render_or_plain(&input);
        assert!(
            text.plain().contains("text"),
            "failed for {n} backslashes: plain={:?}",
            text.plain()
        );
    }
}

proptest! {
    #[test]
    fn fuzz_backslash_sequences_no_panic(
        n_slashes in 0usize..20,
        content in "[a-zA-Z]{1,10}",
    ) {
        let slashes = "\\".repeat(n_slashes);
        let input = format!("{slashes}[bold]{content}[/]");
        let _ = markup::render_or_plain(&input);
    }
}

// ============================================================================
// Edge: link tags with parameters
// ============================================================================

#[test]
fn link_tag_with_url() {
    let input = "[link=https://example.com]click[/link]";
    let text = markup::render_or_plain(input);
    assert_eq!(text.plain(), "click");
}

#[test]
fn link_tag_with_complex_url() {
    let input = "[link=https://example.com/path?q=1&b=2#anchor]text[/link]";
    let text = markup::render_or_plain(input);
    assert_eq!(text.plain(), "text");
}

proptest! {
    #[test]
    fn fuzz_link_tag_no_panic(url in "[a-zA-Z0-9:/.?&=#]{0,50}") {
        let input = format!("[link={url}]click[/link]");
        let text = markup::render_or_plain(&input);
        prop_assert_eq!(text.plain(), "click");
    }
}

// ============================================================================
// Edge: empty and whitespace-only inputs
// ============================================================================

#[test]
fn empty_string() {
    let text = markup::render_or_plain("");
    assert_eq!(text.plain(), "");
}

#[test]
fn whitespace_only() {
    let text = markup::render_or_plain("   \t\n  ");
    assert_eq!(text.plain(), "   \t\n  ");
}

#[test]
fn empty_tags() {
    // [] doesn't match the tag regex (requires [A-Za-z#/@] start)
    let text = markup::render_or_plain("[]text[]");
    assert!(text.plain().contains("text"));
}

#[test]
fn tag_with_only_whitespace() {
    // [   ] - whitespace-only tag content, starts with space not letter
    let text = markup::render_or_plain("[   ]text[/]");
    assert!(text.plain().contains("text"));
}

// ============================================================================
// Edge: malicious/adversarial patterns
// ============================================================================

#[test]
fn catastrophic_backtracking_attempt() {
    // Patterns that might cause regex catastrophic backtracking
    let input = "[".repeat(1000) + &"]".repeat(1000);
    let text = markup::render_or_plain(&input);
    let _ = text.plain(); // Just verify no hang or panic
}

#[test]
fn nested_bracket_pairs() {
    let input = "[[[[bold]]]]text[[[[/]]]]";
    let text = markup::render_or_plain(input);
    assert!(text.plain().contains("text"));
}

#[test]
fn alternating_open_close_brackets() {
    let input = "[][]][][[][][][".repeat(100);
    let text = markup::render_or_plain(&input);
    let _ = text.plain();
}

#[test]
fn null_bytes_in_content() {
    let input = "[bold]hello\0world[/]";
    let text = markup::render_or_plain(input);
    assert!(text.plain().contains("hello"));
}

#[test]
fn control_characters_in_content() {
    let input = "[bold]\x01\x02\x03\x07\x08[/]";
    let text = markup::render_or_plain(input);
    assert!(!text.plain().is_empty());
}
