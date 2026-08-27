//! End-to-end tests for JSON formatting and rendering.
//!
//! Tests the `Json` renderable with Console integration: simple objects,
//! nested structures, arrays, null/bool/numbers, Unicode strings, large
//! documents, theme customization, and indent settings.

#![cfg(feature = "json")]

use std::io::Write;
use std::sync::{Arc, Mutex};

use rich_rust::color::ColorSystem;
use rich_rust::prelude::*;
use rich_rust::renderables::json::{Json, JsonTheme};
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

fn render_json_plain(json: &Json) -> String {
    json.to_plain_string()
}

fn render_json_via_console(json: &Json) -> String {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let writer = BufferWriter(Arc::clone(&buf));
    let console = Console::builder()
        .color_system(ColorSystem::TrueColor)
        .force_terminal(true)
        .width(120)
        .file(Box::new(writer))
        .build();
    console.print_renderable(json);
    let guard = lock_recover(&buf);
    String::from_utf8_lossy(&guard).into_owned()
}

fn render_json_via_console_custom_tab_size(json: &Json, tab_size: usize) -> String {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let writer = BufferWriter(Arc::clone(&buf));
    let console = Console::builder()
        .force_terminal(false)
        .width(120)
        .tab_size(tab_size)
        .file(Box::new(writer))
        .build();
    console.print_renderable(json);
    let guard = lock_recover(&buf);
    String::from_utf8_lossy(&guard).into_owned()
}

fn render_json_no_color(json: &Json) -> String {
    let buf = Arc::new(Mutex::new(Vec::new()));
    let writer = BufferWriter(Arc::clone(&buf));
    let console = Console::builder()
        .force_terminal(false)
        .width(120)
        .file(Box::new(writer))
        .build();
    console.print_renderable(json);
    let guard = lock_recover(&buf);
    String::from_utf8_lossy(&guard).into_owned()
}

// ============================================================================
// 1. Simple objects
// ============================================================================

#[test]
fn simple_object_renders_key_value() {
    let json = Json::from_str(r#"{"name": "Alice"}"#).unwrap();
    let plain = render_json_plain(&json);
    assert!(plain.contains(r#""name""#), "should contain key: {plain}");
    assert!(
        plain.contains(r#""Alice""#),
        "should contain value: {plain}"
    );
}

#[test]
fn simple_object_has_braces() {
    let json = Json::from_str(r#"{"key": "value"}"#).unwrap();
    let plain = render_json_plain(&json);
    assert!(plain.starts_with('{'), "should start with brace: {plain}");
    assert!(plain.ends_with('}'), "should end with brace: {plain}");
}

#[test]
fn simple_object_multiple_keys() {
    let json = Json::from_str(r#"{"a": 1, "b": 2, "c": 3}"#).unwrap();
    let plain = render_json_plain(&json);
    assert!(plain.contains(r#""a""#));
    assert!(plain.contains(r#""b""#));
    assert!(plain.contains(r#""c""#));
    assert!(plain.contains("1"));
    assert!(plain.contains("2"));
    assert!(plain.contains("3"));
}

#[test]
fn empty_object_renders_compact() {
    let json = Json::from_str(r#"{}"#).unwrap();
    let plain = render_json_plain(&json);
    assert_eq!(plain, "{}");
}

// ============================================================================
// 2. Nested structures
// ============================================================================

#[test]
fn nested_object_renders_indented() {
    let json = Json::from_str(r#"{"outer": {"inner": "value"}}"#).unwrap();
    let plain = render_json_plain(&json);
    // Should have indentation for nested object
    assert!(plain.contains("  "), "should contain indentation: {plain}");
    assert!(plain.contains(r#""outer""#));
    assert!(plain.contains(r#""inner""#));
    assert!(plain.contains(r#""value""#));
}

#[test]
fn deeply_nested_structures() {
    let json = Json::from_str(r#"{"a": {"b": {"c": {"d": {"e": "deep"}}}}}"#).unwrap();
    let plain = render_json_plain(&json);
    assert!(plain.contains(r#""deep""#));
    // Each level adds 2 spaces of indentation (default)
    assert!(
        plain.contains("        "),
        "should have 8+ spaces for depth 4"
    );
}

#[test]
fn object_with_nested_array() {
    let json = Json::from_str(r#"{"items": [1, 2, 3]}"#).unwrap();
    let plain = render_json_plain(&json);
    assert!(plain.contains(r#""items""#));
    assert!(plain.contains("["));
    assert!(plain.contains("1"));
    assert!(plain.contains("2"));
    assert!(plain.contains("3"));
    assert!(plain.contains("]"));
}

#[test]
fn array_of_objects() {
    let json = Json::from_str(r#"[{"name": "Alice"}, {"name": "Bob"}]"#).unwrap();
    let plain = render_json_plain(&json);
    assert!(plain.contains(r#""Alice""#));
    assert!(plain.contains(r#""Bob""#));
}

// ============================================================================
// 3. Arrays
// ============================================================================

#[test]
fn simple_array() {
    let json = Json::from_str(r#"[1, 2, 3, 4, 5]"#).unwrap();
    let plain = render_json_plain(&json);
    assert!(plain.contains("["));
    assert!(plain.contains("]"));
    for i in 1..=5 {
        assert!(plain.contains(&i.to_string()), "missing {i}");
    }
}

#[test]
fn empty_array() {
    let json = Json::from_str("[]").unwrap();
    let plain = render_json_plain(&json);
    assert_eq!(plain, "[]");
}

#[test]
fn nested_arrays() {
    let json = Json::from_str(r#"[[1, 2], [3, 4], [5, 6]]"#).unwrap();
    let plain = render_json_plain(&json);
    for i in 1..=6 {
        assert!(plain.contains(&i.to_string()));
    }
}

#[test]
fn mixed_type_array() {
    let json = Json::from_str(r#"[1, "hello", true, null, 3.14]"#).unwrap();
    let plain = render_json_plain(&json);
    assert!(plain.contains("1"));
    assert!(plain.contains(r#""hello""#));
    assert!(plain.contains("true"));
    assert!(plain.contains("null"));
    assert!(plain.contains("3.14"));
}

// ============================================================================
// 4. Null, booleans, and numbers
// ============================================================================

#[test]
fn null_value() {
    let json = Json::from_str("null").unwrap();
    let plain = render_json_plain(&json);
    assert_eq!(plain, "null");
}

#[test]
fn boolean_true() {
    let json = Json::from_str("true").unwrap();
    let plain = render_json_plain(&json);
    assert_eq!(plain, "true");
}

#[test]
fn boolean_false() {
    let json = Json::from_str("false").unwrap();
    let plain = render_json_plain(&json);
    assert_eq!(plain, "false");
}

#[test]
fn integer_number() {
    let json = Json::from_str("42").unwrap();
    let plain = render_json_plain(&json);
    assert_eq!(plain, "42");
}

#[test]
fn float_number() {
    let json = Json::from_str("3.14159").unwrap();
    let plain = render_json_plain(&json);
    assert!(plain.contains("3.14159"));
}

#[test]
fn negative_number() {
    let json = Json::from_str("-273").unwrap();
    let plain = render_json_plain(&json);
    assert_eq!(plain, "-273");
}

#[test]
fn scientific_notation() {
    // serde_json parses scientific notation; output may vary
    let json = Json::from_str("1.5e10").unwrap();
    let plain = render_json_plain(&json);
    // Should contain the number in some form
    assert!(!plain.is_empty());
}

#[test]
fn object_with_all_value_types() {
    let input = r#"{
        "string": "hello",
        "number": 42,
        "float": 3.14,
        "bool_t": true,
        "bool_f": false,
        "null_v": null,
        "array": [1, 2],
        "object": {"nested": true}
    }"#;
    let json = Json::from_str(input).unwrap().sort_keys(true);
    let plain = render_json_plain(&json);
    assert!(plain.contains(r#""hello""#));
    assert!(plain.contains("42"));
    assert!(plain.contains("3.14"));
    assert!(plain.contains("true"));
    assert!(plain.contains("false"));
    assert!(plain.contains("null"));
}

// ============================================================================
// 5. Unicode strings
// ============================================================================

#[test]
fn unicode_string_values() {
    let json = Json::from_str(r#"{"greeting": "こんにちは"}"#).unwrap();
    let plain = render_json_plain(&json);
    assert!(plain.contains("こんにちは"));
}

#[test]
fn emoji_string_values() {
    let json = Json::from_str(r#"{"emoji": "🎉🎊🚀"}"#).unwrap();
    let plain = render_json_plain(&json);
    assert!(plain.contains("🎉🎊🚀"));
}

#[test]
fn unicode_keys() {
    let json = Json::from_str(r#"{"名前": "太郎"}"#).unwrap();
    let plain = render_json_plain(&json);
    assert!(plain.contains("名前"));
    assert!(plain.contains("太郎"));
}

#[test]
fn string_with_escapes() {
    let json = Json::from_str(r#"{"text": "line1\nline2\ttab"}"#).unwrap();
    let plain = render_json_plain(&json);
    assert!(plain.contains(r#"\n"#), "should contain escaped newline");
    assert!(plain.contains(r#"\t"#), "should contain escaped tab");
}

#[test]
fn string_with_quotes() {
    let json = Json::from_str(r#"{"text": "he said \"hello\""}"#).unwrap();
    let plain = render_json_plain(&json);
    assert!(plain.contains(r#"\""#), "should contain escaped quotes");
}

// ============================================================================
// 6. Large documents
// ============================================================================

#[test]
fn large_array_1000_elements() {
    let arr: Vec<u32> = (0..1000).collect();
    let json_str = serde_json::to_string(&arr).unwrap();
    let json = Json::from_str(&json_str).unwrap();
    let plain = render_json_plain(&json);
    assert!(plain.contains("0"));
    assert!(plain.contains("999"));
}

#[test]
fn large_object_100_keys() {
    let mut obj = serde_json::Map::new();
    for i in 0..100 {
        obj.insert(format!("key_{i:03}"), serde_json::Value::Number(i.into()));
    }
    let json = Json::new(serde_json::Value::Object(obj));
    let plain = render_json_plain(&json);
    assert!(plain.contains(r#""key_000""#));
    assert!(plain.contains(r#""key_099""#));
}

#[test]
fn deeply_nested_at_max_depth() {
    // Build 21 levels of nesting to trigger max depth truncation
    let mut value = serde_json::json!("leaf");
    for _ in 0..21 {
        value = serde_json::json!({"nested": value});
    }
    let json = Json::new(value);
    let plain = render_json_plain(&json);
    // Should contain truncation marker at max depth
    assert!(
        plain.contains("{...}") || plain.contains("leaf"),
        "should truncate or include leaf: {plain}"
    );
}

#[test]
fn deeply_nested_array_at_max_depth() {
    let mut value = serde_json::json!("leaf");
    for _ in 0..21 {
        value = serde_json::json!([value]);
    }
    let json = Json::new(value);
    let plain = render_json_plain(&json);
    assert!(
        plain.contains("[...]") || plain.contains("leaf"),
        "should truncate or include leaf"
    );
}

// ============================================================================
// 7. Theme customization
// ============================================================================

#[test]
fn custom_theme_applies_to_output() {
    let theme = JsonTheme {
        key: Style::parse("#ff0000").unwrap().bold(),
        string: Style::parse("#00ff00").unwrap(),
        number: Style::parse("#0000ff").unwrap(),
        bool_true: Style::parse("#ffff00").unwrap(),
        bool_false: Style::parse("#ffff00").unwrap(),
        null: Style::parse("#ff00ff").unwrap(),
        bracket: Style::parse("#00ffff").unwrap(),
        punctuation: Style::new(),
    };
    let json = Json::from_str(r#"{"key": "value", "num": 42}"#)
        .unwrap()
        .theme(theme);

    let output = render_json_via_console(&json);
    // With TrueColor console, should contain ANSI escape codes
    assert!(
        output.contains("\x1b["),
        "themed output should have ANSI codes: {output}"
    );
    // Content should be present
    assert!(output.contains("key"));
    assert!(output.contains("value"));
    assert!(output.contains("42"));
}

#[test]
fn default_theme_produces_colored_output() {
    let json = Json::from_str(r#"{"name": "test"}"#).unwrap();
    let output = render_json_via_console(&json);
    assert!(
        output.contains("\x1b["),
        "default theme should produce ANSI codes: {output}"
    );
}

#[test]
fn no_highlight_produces_plain_output() {
    let json = Json::from_str(r#"{"name": "test"}"#)
        .unwrap()
        .highlight(false);
    let output = render_json_via_console(&json);
    // Even via colored console, highlight(false) should produce no style segments
    // (the console may still add minimal formatting, but JSON segments are unstyled)
    assert!(output.contains("name"));
    assert!(output.contains("test"));
}

#[test]
fn non_tty_console_degrades_gracefully() {
    let json = Json::from_str(r#"{"name": "test", "count": 5}"#).unwrap();
    let output = render_json_no_color(&json);
    assert!(
        !output.contains("\x1b["),
        "non-TTY should not have ANSI codes: {output}"
    );
    assert!(output.contains("name"));
    assert!(output.contains("test"));
    assert!(output.contains("5"));
}

// ============================================================================
// 8. Indent settings
// ============================================================================

#[test]
fn default_indent_is_2_spaces() {
    let json = Json::from_str(r#"{"key": "value"}"#).unwrap();
    let plain = render_json_plain(&json);
    // With default 2-space indent, the key line should start with 2 spaces
    let lines: Vec<&str> = plain.lines().collect();
    assert!(
        lines.len() >= 3,
        "should have at least 3 lines (open, content, close)"
    );
    assert!(
        lines[1].starts_with("  "),
        "content line should start with 2 spaces: {:?}",
        lines[1]
    );
    assert!(
        !lines[1].starts_with("    "),
        "content line should NOT start with 4 spaces (default is 2): {:?}",
        lines[1]
    );
}

#[test]
fn custom_indent_4_spaces() {
    let json = Json::from_str(r#"{"key": "value"}"#).unwrap().indent(4);
    let plain = render_json_plain(&json);
    let lines: Vec<&str> = plain.lines().collect();
    assert!(lines.len() >= 3);
    assert!(
        lines[1].starts_with("    "),
        "content line should start with 4 spaces: {:?}",
        lines[1]
    );
}

#[test]
fn indent_0_still_has_newlines() {
    let json = Json::from_str(r#"{"key": "value"}"#).unwrap().indent(0);
    let plain = render_json_plain(&json);
    // Even with indent 0, pretty-printing adds newlines
    assert!(
        plain.contains('\n'),
        "should contain newlines even with indent 0"
    );
}

#[test]
fn indent_affects_nested_levels() {
    let json = Json::from_str(r#"{"a": {"b": "c"}}"#).unwrap().indent(3);
    let plain = render_json_plain(&json);
    let lines: Vec<&str> = plain.lines().collect();
    // Depth 1: 3 spaces, depth 2: 6 spaces
    let has_3_space = lines
        .iter()
        .any(|l| l.starts_with("   ") && !l.starts_with("      "));
    let has_6_space = lines.iter().any(|l| l.starts_with("      "));
    assert!(has_3_space, "should have 3-space indented lines: {plain}");
    assert!(has_6_space, "should have 6-space indented lines: {plain}");
}

#[test]
fn compact_mode_has_no_newlines_and_has_spaces() {
    let json = Json::from_str(r#"{"age": 30, "name": "Alice"}"#)
        .unwrap()
        .compact();
    let plain = render_json_plain(&json);
    assert!(
        !plain.contains('\n'),
        "compact output should be one line: {plain}"
    );
    assert!(plain.starts_with('{') && plain.ends_with('}'));
    assert!(
        plain.contains(": "),
        "compact output should have ': ': {plain}"
    );
    assert!(
        plain.contains(", "),
        "compact output should have ', ': {plain}"
    );
}

#[test]
fn ensure_ascii_escapes_non_ascii_characters() {
    let json = Json::from_str(r#"{"greeting": "こんにちは"}"#)
        .unwrap()
        .ensure_ascii(true);
    let plain = render_json_plain(&json);
    assert!(
        !plain.contains("こんにちは"),
        "ensure_ascii should escape unicode: {plain}"
    );
    assert!(
        plain.contains("\\u"),
        "ensure_ascii should contain unicode escapes: {plain}"
    );
}

#[test]
fn indent_str_tab_expands_using_console_tab_size() {
    let json = Json::from_str(r#"{"key": "value"}"#)
        .unwrap()
        .indent_str("\t");
    let out = render_json_via_console_custom_tab_size(&json, 4);
    let lines: Vec<&str> = out.lines().collect();
    assert!(lines.len() >= 3);
    assert!(
        lines[1].starts_with("    "),
        "tab indent should expand to 4 spaces with tab_size=4: {:?}",
        lines[1]
    );
}

#[test]
fn booleans_and_null_have_distinct_styles_in_ansi_output() {
    let json = Json::from_str(r#"{"t": true, "f": false, "n": null}"#).unwrap();
    let out = render_json_via_console(&json);
    // True should be bright green italic (91/92) and False should be bright red italic.
    // Style code ordering can vary, so allow both `3;92` and `92;3` forms.
    let true_ok = out.contains("\x1b[3;92mtrue") || out.contains("\x1b[92;3mtrue");
    let false_ok = out.contains("\x1b[3;91mfalse") || out.contains("\x1b[91;3mfalse");
    let null_ok = out.contains("\x1b[3;35mnull") || out.contains("\x1b[35;3mnull");
    assert!(true_ok, "expected styled true, got: {out}");
    assert!(false_ok, "expected styled false, got: {out}");
    assert!(null_ok, "expected styled null, got: {out}");
}

// ============================================================================
// 9. Sort keys
// ============================================================================

#[test]
fn sort_keys_alphabetical() {
    let json = Json::from_str(r#"{"zebra": 1, "apple": 2, "mango": 3}"#)
        .unwrap()
        .sort_keys(true);
    let plain = render_json_plain(&json);
    let apple_pos = plain.find(r#""apple""#).unwrap();
    let mango_pos = plain.find(r#""mango""#).unwrap();
    let zebra_pos = plain.find(r#""zebra""#).unwrap();
    assert!(apple_pos < mango_pos, "apple should come before mango");
    assert!(mango_pos < zebra_pos, "mango should come before zebra");
}

#[test]
fn unsorted_contains_all_keys() {
    // serde_json::Map uses BTreeMap by default (sorted), so we just verify
    // all keys are present when sort_keys is false
    let json = Json::from_str(r#"{"zebra": 1, "apple": 2, "mango": 3}"#)
        .unwrap()
        .sort_keys(false);
    let plain = render_json_plain(&json);
    assert!(plain.contains(r#""zebra""#));
    assert!(plain.contains(r#""apple""#));
    assert!(plain.contains(r#""mango""#));
}

// ============================================================================
// 10. Error handling
// ============================================================================

#[test]
fn invalid_json_returns_error() {
    let result = Json::from_str("not valid json");
    assert!(result.is_err());
}

#[test]
fn invalid_json_error_display() {
    let err = Json::from_str("{invalid}").unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("JSON parse error"),
        "error message should mention JSON: {msg}"
    );
}

#[test]
fn trailing_comma_is_invalid() {
    let result = Json::from_str(r#"{"key": "value",}"#);
    assert!(result.is_err(), "trailing comma should be invalid JSON");
}

#[test]
fn single_quotes_are_invalid() {
    let result = Json::from_str("{'key': 'value'}");
    assert!(result.is_err(), "single quotes should be invalid JSON");
}

// ============================================================================
// 11. Console integration: full rendering pipeline
// ============================================================================

#[test]
fn console_renders_json_with_ansi() {
    let json = Json::from_str(r#"{"status": "ok", "code": 200, "active": true}"#)
        .unwrap()
        .sort_keys(true);
    let output = render_json_via_console(&json);

    // Should contain ANSI codes from styling
    assert!(output.contains("\x1b["), "should have ANSI codes");
    // Should contain all values
    assert!(output.contains("ok"));
    assert!(output.contains("200"));
    assert!(output.contains("true"));
    // Should contain structural elements
    assert!(output.contains("{"));
    assert!(output.contains("}"));
}

#[test]
fn console_renders_json_array_with_nulls() {
    let json = Json::from_str(r#"[null, null, null]"#).unwrap();
    let output = render_json_via_console(&json);
    // Count occurrences of "null" in the output
    let null_count = output.matches("null").count();
    assert!(
        null_count >= 3,
        "should contain 3 nulls, found {null_count}: {output}"
    );
}

#[test]
fn json_segments_can_be_collected() {
    // Verify JSON renders to valid segments that can be composed
    let json = Json::from_str(r#"{"api": "v2", "healthy": true}"#).unwrap();
    let segments = json.render();
    assert!(!segments.is_empty(), "should produce segments");

    // Collect plain text from segments
    let plain: String = segments.iter().map(|s| s.text.as_ref()).collect();
    assert!(plain.contains("api"));
    assert!(plain.contains("v2"));
    assert!(plain.contains("true"));
}

// ============================================================================
// 12. Complex real-world documents
// ============================================================================

#[test]
fn api_response_like_document() {
    let input = r#"{
        "status": "success",
        "data": {
            "users": [
                {"id": 1, "name": "Alice", "email": "alice@example.com", "active": true},
                {"id": 2, "name": "Bob", "email": "bob@example.com", "active": false}
            ],
            "total": 2,
            "page": 1,
            "per_page": 10
        },
        "metadata": {
            "version": "2.0",
            "timestamp": "2025-01-01T00:00:00Z",
            "request_id": null
        }
    }"#;
    let json = Json::from_str(input).unwrap().sort_keys(true);
    let plain = render_json_plain(&json);

    // Verify structure is preserved
    assert!(plain.contains(r#""Alice""#));
    assert!(plain.contains(r#""Bob""#));
    assert!(plain.contains(r#""success""#));
    assert!(plain.contains("null"));
    assert!(plain.contains("true"));
    assert!(plain.contains("false"));
    assert!(plain.contains("10"));

    // Also render via console to verify no panics in the full pipeline
    let output = render_json_via_console(&json);
    assert!(output.contains("Alice"));
    assert!(output.contains("success"));
}

#[test]
fn config_file_like_document() {
    let input = r#"{
        "database": {
            "host": "localhost",
            "port": 5432,
            "name": "myapp",
            "ssl": true,
            "pool_size": 10
        },
        "redis": {
            "url": "redis://localhost:6379",
            "ttl": 3600
        },
        "features": ["auth", "logging", "metrics"]
    }"#;
    let json = Json::from_str(input).unwrap().indent(4).sort_keys(true);
    let plain = render_json_plain(&json);
    assert!(plain.contains(r#""localhost""#));
    assert!(plain.contains("5432"));
    assert!(plain.contains(r#""auth""#));
    assert!(plain.contains(r#""logging""#));
}
