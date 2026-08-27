//! JSON - Pretty-printed JSON with syntax highlighting.
//!
//! This module provides a JSON renderable for rendering JSON data
//! with syntax highlighting and configurable formatting. It uses semantic
//! coloring to distinguish keys, strings, numbers, booleans, and null values.
//!
//! # Feature Flag
//!
//! This module requires the `json` feature to be enabled:
//!
//! ```toml
//! [dependencies]
//! rich_rust = { version = "0.1", features = ["json"] }
//! ```
//!
//! Or enable all optional features with:
//!
//! ```toml
//! rich_rust = { version = "0.1", features = ["full"] }
//! ```
//!
//! # Dependencies
//!
//! Enabling this feature adds the [`serde_json`](https://docs.rs/serde_json) crate
//! as a dependency for JSON parsing and value representation.
//!
//! # Basic Usage
//!
//! ```rust,ignore
//! use rich_rust::renderables::json::Json;
//!
//! // From a JSON string
//! let data = r#"{"name": "Alice", "age": 30, "active": true}"#;
//! let json = Json::from_str(data).unwrap();
//! let segments = json.render();
//!
//! // From a serde_json::Value
//! use serde_json::json;
//! let json = Json::new(json!({"key": "value"}));
//! ```
//!
//! # Indentation Options
//!
//! ```rust,ignore
//! use rich_rust::renderables::json::Json;
//!
//! let json = Json::from_str(r#"{"a": [1, 2, 3]}"#).unwrap()
//!     .indent(4);  // Use 4-space indentation (default is 2)
//! ```
//!
//! # Sorting Keys
//!
//! ```rust,ignore
//! use rich_rust::renderables::json::Json;
//!
//! // Keys will appear in alphabetical order
//! let json = Json::from_str(r#"{"z": 1, "a": 2, "m": 3}"#).unwrap()
//!     .sort_keys(true);
//! ```
//!
//! # Custom Themes
//!
//! The default theme uses semantic colors:
//! - **Keys**: Blue, bold
//! - **Strings**: Green
//! - **Numbers**: Cyan
//! - **Booleans**: Yellow
//! - **Null**: Magenta, italic
//! - **Brackets/braces**: White
//! - **Punctuation**: White
//!
//! You can customize the theme:
//!
//! ```rust,ignore
//! use rich_rust::renderables::json::{Json, JsonTheme};
//! use rich_rust::style::Style;
//!
//! let theme = JsonTheme {
//!     key: Style::new().bold().color_str("red").unwrap(),
//!     string: Style::new().color_str("blue").unwrap(),
//!     number: Style::new().color_str("green").unwrap(),
//!     boolean: Style::new().color_str("yellow").unwrap(),
//!     null: Style::new().color_str("white").unwrap(),
//!     bracket: Style::new().color_str("cyan").unwrap(),
//!     punctuation: Style::new().color_str("magenta").unwrap(),
//! };
//!
//! let json = Json::from_str(r#"{"key": "value"}"#).unwrap()
//!     .theme(theme);
//! ```
//!
//! # Disabling Highlighting
//!
//! ```rust,ignore
//! use rich_rust::renderables::json::Json;
//!
//! // Render without colors (plain text)
//! let json = Json::from_str(r#"{"key": "value"}"#).unwrap()
//!     .highlight(false);
//! ```
//!
//! # Plain Text Output
//!
//! ```rust,ignore
//! use rich_rust::renderables::json::Json;
//!
//! let json = Json::from_str(r#"{"key": "value"}"#).unwrap();
//! let plain = json.to_plain_string();  // Get formatted JSON without ANSI codes
//! ```
//!
//! # Known Limitations
//!
//! - **Large JSON**: Very large JSON documents may be slow to render due to
//!   per-token segment creation
//! - **Streaming**: Does not support streaming JSON parsing; the entire document
//!   must fit in memory
//! - **Trailing commas**: Standard JSON only; no trailing comma support
//! - **Python Rich JSON option parity**: `rich_rust` matches Python Rich JSON formatting
//!   for the supported option set (`indent: None|int|str`, `sort_keys`, `ensure_ascii`,
//!   `highlight`). Python-only options such as `check_circular`, `allow_nan`, and `default`
//!   exist in Python's `json.dumps` API but don't map cleanly to Rust's `serde_json`
//!   value model.

use std::fmt::Write as _;

use serde::Serialize;
use serde_json::Value;

use crate::segment::Segment;
use crate::style::Style;

/// Default theme colors for JSON syntax highlighting.
#[derive(Debug, Clone)]
pub struct JsonTheme {
    /// Style for object/array keys.
    pub key: Style,
    /// Style for string values.
    pub string: Style,
    /// Style for number values.
    pub number: Style,
    /// Style for boolean `true`.
    pub bool_true: Style,
    /// Style for boolean `false`.
    pub bool_false: Style,
    /// Style for null values.
    pub null: Style,
    /// Style for brackets and braces.
    pub bracket: Style,
    /// Style for colons and commas.
    pub punctuation: Style,
}

impl Default for JsonTheme {
    fn default() -> Self {
        // These defaults are intended to match Python Rich's theme defaults:
        // - json.key: bold blue
        // - json.str: green
        // - json.number: bold cyan
        // - json.bool_true: bright_green italic
        // - json.bool_false: bright_red italic
        // - json.null: magenta italic
        // - json.brace: bold
        Self {
            key: Style::new().color_str("blue").unwrap_or_default().bold(),
            string: Style::new().color_str("green").unwrap_or_default(),
            number: Style::new().color_str("cyan").unwrap_or_default().bold(),
            null: Style::new()
                .color_str("magenta")
                .unwrap_or_default()
                .italic(),
            bool_true: Style::new()
                .color_str("bright_green")
                .unwrap_or_default()
                .italic(),
            bool_false: Style::new()
                .color_str("bright_red")
                .unwrap_or_default()
                .italic(),
            bracket: Style::new().bold(),
            punctuation: Style::new(),
        }
    }
}

/// JSON indentation configuration (Python Rich compatible).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JsonIndent {
    /// Compact mode (`indent=None` in Python): no newlines; still uses `": "` and `", "` separators.
    None,
    /// Indent using N spaces (`indent=int` in Python).
    Spaces(usize),
    /// Indent using the given string (`indent=str` in Python).
    ///
    /// Note: Tabs in the indent string are expanded to spaces using the console tab size
    /// when rendering, matching Rich's default `Text.expand_tabs` behavior.
    String(String),
}

impl Default for JsonIndent {
    fn default() -> Self {
        Self::Spaces(2)
    }
}

/// Formatting options for rendering JSON.
///
/// This is intentionally aligned with Python Rich's `rich.json.JSON` constructor options
/// where they map cleanly to Rust.
#[derive(Debug, Clone)]
pub struct JsonOptions {
    pub indent: JsonIndent,
    pub highlight: bool,
    pub sort_keys: bool,
    pub ensure_ascii: bool,
}

impl Default for JsonOptions {
    fn default() -> Self {
        Self {
            indent: JsonIndent::default(),
            highlight: true,
            sort_keys: false,
            ensure_ascii: false,
        }
    }
}

/// A renderable for JSON data with syntax highlighting.
#[derive(Debug, Clone)]
pub struct Json {
    /// The JSON value to render.
    value: Value,
    /// Indentation configuration (pretty vs compact).
    indent: JsonIndent,
    /// Whether to sort object keys alphabetically.
    sort_keys: bool,
    /// Whether to escape non-ASCII characters.
    ensure_ascii: bool,
    /// Whether to apply syntax highlighting.
    highlight: bool,
    /// Theme for syntax highlighting.
    theme: JsonTheme,
}

impl Json {
    /// Create a new Json renderable from a `serde_json::Value`.
    #[must_use]
    pub fn new(value: Value) -> Self {
        Self {
            value,
            indent: JsonIndent::default(),
            sort_keys: false,
            ensure_ascii: false,
            highlight: true,
            theme: JsonTheme::default(),
        }
    }

    /// Create a new Json renderable from a `serde_json::Value` with explicit options.
    #[must_use]
    pub fn with_options(value: Value, options: JsonOptions) -> Self {
        Self {
            value,
            indent: options.indent,
            sort_keys: options.sort_keys,
            ensure_ascii: options.ensure_ascii,
            highlight: options.highlight,
            theme: JsonTheme::default(),
        }
    }

    /// Create a Json renderable from a JSON string.
    ///
    /// # Errors
    ///
    /// Returns an error if the string is not valid JSON.
    #[expect(
        clippy::should_implement_trait,
        reason = "returns Result with custom error, not FromStr pattern"
    )]
    pub fn from_str(s: &str) -> Result<Self, JsonError> {
        let value: Value = serde_json::from_str(s).map_err(JsonError::Parse)?;
        Ok(Self::new(value))
    }

    /// Create a Json renderable from a JSON string with explicit options.
    ///
    /// # Errors
    ///
    /// Returns an error if the string is not valid JSON.
    pub fn from_str_with_options(s: &str, options: JsonOptions) -> Result<Self, JsonError> {
        let value: Value = serde_json::from_str(s).map_err(JsonError::Parse)?;
        Ok(Self::with_options(value, options))
    }

    /// Encode JSON from serializable Rust data (Python Rich `JSON.from_data` equivalent).
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails (e.g. non-finite floats).
    pub fn from_data<T: Serialize>(data: &T) -> Result<Self, JsonError> {
        let value = serde_json::to_value(data).map_err(JsonError::Serialize)?;
        Ok(Self::new(value))
    }

    /// Set the number of spaces for indentation (pretty mode).
    #[must_use]
    pub fn indent(mut self, spaces: usize) -> Self {
        self.indent = JsonIndent::Spaces(spaces);
        self
    }

    /// Set a custom indentation string (pretty mode).
    #[must_use]
    pub fn indent_str(mut self, unit: impl Into<String>) -> Self {
        self.indent = JsonIndent::String(unit.into());
        self
    }

    /// Render JSON in compact mode (Python `indent=None`).
    #[must_use]
    pub fn compact(mut self) -> Self {
        self.indent = JsonIndent::None;
        self
    }

    /// Set whether to sort object keys alphabetically.
    #[must_use]
    pub fn sort_keys(mut self, sort: bool) -> Self {
        self.sort_keys = sort;
        self
    }

    /// Set whether to escape non-ASCII characters.
    #[must_use]
    pub fn ensure_ascii(mut self, ensure_ascii: bool) -> Self {
        self.ensure_ascii = ensure_ascii;
        self
    }

    /// Set whether to apply syntax highlighting.
    #[must_use]
    pub fn highlight(mut self, highlight: bool) -> Self {
        self.highlight = highlight;
        self
    }

    /// Set a custom theme for syntax highlighting.
    #[must_use]
    pub fn theme(mut self, theme: JsonTheme) -> Self {
        self.theme = theme;
        self
    }

    /// Get a style, or no style if highlighting is disabled.
    fn style(&self, style: &Style) -> Option<Style> {
        if self.highlight {
            Some(style.clone())
        } else {
            None
        }
    }

    fn is_compact(&self) -> bool {
        matches!(self.indent, JsonIndent::None)
    }

    fn indent_prefix(&self, depth: usize, tab_size: usize) -> String {
        match &self.indent {
            JsonIndent::None => String::new(),
            JsonIndent::Spaces(n) => " ".repeat(n.saturating_mul(depth)),
            JsonIndent::String(unit) => expand_tabs_at_col0(&unit.repeat(depth), tab_size),
        }
    }

    /// Render a JSON value at the given depth.
    fn render_value(&self, value: &Value, depth: usize, tab_size: usize) -> Vec<Segment<'_>> {
        match value {
            Value::Null => vec![Segment::new("null", self.style(&self.theme.null))],
            Value::Bool(b) => {
                let text = if *b { "true" } else { "false" };
                let style = if *b {
                    self.style(&self.theme.bool_true)
                } else {
                    self.style(&self.theme.bool_false)
                };
                vec![Segment::new(text, style)]
            }
            Value::Number(n) => {
                vec![Segment::new(n.to_string(), self.style(&self.theme.number))]
            }
            Value::String(s) => {
                // Escape and quote the string
                let escaped = escape_json_string(s, self.ensure_ascii);
                vec![Segment::new(
                    format!("\"{escaped}\""),
                    self.style(&self.theme.string),
                )]
            }
            Value::Array(arr) => self.render_array(arr, depth, tab_size),
            Value::Object(obj) => self.render_object(obj, depth, tab_size),
        }
    }

    /// Render an array.
    fn render_array(&self, arr: &[Value], depth: usize, tab_size: usize) -> Vec<Segment<'_>> {
        const MAX_DEPTH: usize = 20;
        if depth > MAX_DEPTH {
            return vec![Segment::new("[...]", self.style(&self.theme.bracket))];
        }

        if arr.is_empty() {
            return vec![Segment::new("[]", self.style(&self.theme.bracket))];
        }

        let mut segments = Vec::new();

        // Opening bracket
        segments.push(Segment::new("[", self.style(&self.theme.bracket)));
        if self.is_compact() {
            for (i, item) in arr.iter().enumerate() {
                segments.extend(self.render_value(item, depth + 1, tab_size));
                if i < arr.len() - 1 {
                    segments.push(Segment::new(", ", self.style(&self.theme.punctuation)));
                }
            }
            segments.push(Segment::new("]", self.style(&self.theme.bracket)));
        } else {
            let indent_str = self.indent_prefix(depth + 1, tab_size);
            let close_indent = self.indent_prefix(depth, tab_size);

            segments.push(Segment::new("\n", None));
            for (i, item) in arr.iter().enumerate() {
                segments.push(Segment::new(indent_str.clone(), None));
                segments.extend(self.render_value(item, depth + 1, tab_size));
                if i < arr.len() - 1 {
                    segments.push(Segment::new(",", self.style(&self.theme.punctuation)));
                }
                segments.push(Segment::new("\n", None));
            }
            segments.push(Segment::new(close_indent, None));
            segments.push(Segment::new("]", self.style(&self.theme.bracket)));
        }

        segments
    }

    /// Render an object.
    fn render_object(
        &self,
        obj: &serde_json::Map<String, Value>,
        depth: usize,
        tab_size: usize,
    ) -> Vec<Segment<'_>> {
        const MAX_DEPTH: usize = 20;
        if depth > MAX_DEPTH {
            return vec![Segment::new("{...}", self.style(&self.theme.bracket))];
        }

        if obj.is_empty() {
            return vec![Segment::new("{}", self.style(&self.theme.bracket))];
        }

        let mut segments = Vec::new();

        // Get keys, optionally sorted
        let keys: Vec<&String> = if self.sort_keys {
            let mut k: Vec<_> = obj.keys().collect();
            k.sort();
            k
        } else {
            obj.keys().collect()
        };

        // Opening brace
        segments.push(Segment::new("{", self.style(&self.theme.bracket)));
        if self.is_compact() {
            for (i, key) in keys.iter().enumerate() {
                let value = &obj[*key];
                let escaped_key = escape_json_string(key, self.ensure_ascii);
                segments.push(Segment::new(
                    format!("\"{escaped_key}\""),
                    self.style(&self.theme.key),
                ));
                segments.push(Segment::new(": ", self.style(&self.theme.punctuation)));
                segments.extend(self.render_value(value, depth + 1, tab_size));
                if i < keys.len() - 1 {
                    segments.push(Segment::new(", ", self.style(&self.theme.punctuation)));
                }
            }
            segments.push(Segment::new("}", self.style(&self.theme.bracket)));
        } else {
            let indent_str = self.indent_prefix(depth + 1, tab_size);
            let close_indent = self.indent_prefix(depth, tab_size);

            segments.push(Segment::new("\n", None));
            for (i, key) in keys.iter().enumerate() {
                let value = &obj[*key];
                segments.push(Segment::new(indent_str.clone(), None));

                let escaped_key = escape_json_string(key, self.ensure_ascii);
                segments.push(Segment::new(
                    format!("\"{escaped_key}\""),
                    self.style(&self.theme.key),
                ));
                segments.push(Segment::new(": ", self.style(&self.theme.punctuation)));
                segments.extend(self.render_value(value, depth + 1, tab_size));
                if i < keys.len() - 1 {
                    segments.push(Segment::new(",", self.style(&self.theme.punctuation)));
                }
                segments.push(Segment::new("\n", None));
            }
            segments.push(Segment::new(close_indent, None));
            segments.push(Segment::new("}", self.style(&self.theme.bracket)));
        }

        segments
    }

    /// Render the JSON to segments, using the given tab size for indentation expansion.
    #[must_use]
    pub fn render_with_tab_size(&self, tab_size: usize) -> Vec<Segment<'_>> {
        self.render_value(&self.value, 0, tab_size)
    }

    /// Render the JSON to segments using the default tab size (8).
    #[must_use]
    pub fn render(&self) -> Vec<Segment<'_>> {
        self.render_with_tab_size(8)
    }

    /// Render to a plain string without ANSI codes.
    #[must_use]
    pub fn to_plain_string(&self) -> String {
        self.render().iter().map(|s| s.text.as_ref()).collect()
    }
}

/// Escape special characters in a JSON string.
fn escape_json_string(s: &str, ensure_ascii: bool) -> String {
    let mut result = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\u{0008}' => result.push_str("\\b"),
            '\u{000c}' => result.push_str("\\f"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if ensure_ascii && !c.is_ascii() => {
                let code = c as u32;
                if code <= 0xFFFF {
                    let _ = write!(result, "\\u{code:04x}");
                } else {
                    // Encode as UTF-16 surrogate pair.
                    let n = code - 0x1_0000;
                    let high_bits = u16::try_from((n >> 10) & 0x03FF).unwrap_or_default();
                    let low_bits = u16::try_from(n & 0x03FF).unwrap_or_default();
                    let high = 0xD800u16 | high_bits;
                    let low = 0xDC00u16 | low_bits;
                    let _ = write!(result, "\\u{high:04x}\\u{low:04x}");
                }
            }
            c if c.is_control() => {
                let _ = write!(result, "\\u{:04x}", c as u32);
            }
            c => result.push(c),
        }
    }
    result
}

fn expand_tabs_at_col0(s: &str, tab_size: usize) -> String {
    if tab_size == 0 || !s.contains('\t') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        if ch == '\t' {
            out.push_str(&" ".repeat(tab_size));
        } else {
            out.push(ch);
        }
    }
    out
}

/// Error type for JSON parsing.
#[derive(Debug)]
pub enum JsonError {
    /// JSON parsing error.
    Parse(serde_json::Error),
    /// JSON serialization error.
    Serialize(serde_json::Error),
}

impl std::fmt::Display for JsonError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(e) => write!(f, "JSON parse error: {e}"),
            Self::Serialize(e) => write!(f, "JSON serialize error: {e}"),
        }
    }
}

impl std::error::Error for JsonError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(e) => Some(e),
            Self::Serialize(e) => Some(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_json_null() {
        let json = Json::new(Value::Null);
        let segments = json.render();
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert_eq!(text, "null");
    }

    #[test]
    fn test_json_bool_true() {
        let json = Json::new(Value::Bool(true));
        let segments = json.render();
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert_eq!(text, "true");
    }

    #[test]
    fn test_json_bool_false() {
        let json = Json::new(Value::Bool(false));
        let segments = json.render();
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert_eq!(text, "false");
    }

    #[test]
    fn test_json_number_int() {
        let json = Json::new(serde_json::json!(42));
        let segments = json.render();
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert_eq!(text, "42");
    }

    #[test]
    fn test_json_number_float() {
        let json = Json::new(serde_json::json!(1.23));
        let segments = json.render();
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert_eq!(text, "1.23");
    }

    #[test]
    fn test_json_string() {
        let json = Json::new(serde_json::json!("hello"));
        let segments = json.render();
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert_eq!(text, "\"hello\"");
    }

    #[test]
    fn test_json_string_escaped() {
        let json = Json::new(serde_json::json!("line1\nline2"));
        let segments = json.render();
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert_eq!(text, "\"line1\\nline2\"");
    }

    #[test]
    fn test_json_string_ensure_ascii_surrogate_pair() {
        let json = Json::new(serde_json::json!("😀")).ensure_ascii(true);
        let text = json.to_plain_string();
        assert_eq!(text, "\"\\ud83d\\ude00\"");
    }

    #[test]
    fn test_json_empty_array() {
        let json = Json::new(serde_json::json!([]));
        let segments = json.render();
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert_eq!(text, "[]");
    }

    #[test]
    fn test_json_simple_array() {
        let json = Json::new(serde_json::json!([1, 2, 3])).indent(2);
        let text = json.to_plain_string();
        assert!(text.contains("[\n"));
        assert!(text.contains("  1"));
        assert!(text.contains("  2"));
        assert!(text.contains("  3"));
        assert!(text.contains(']'));
    }

    #[test]
    fn test_json_empty_object() {
        let json = Json::new(serde_json::json!({}));
        let segments = json.render();
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert_eq!(text, "{}");
    }

    #[test]
    fn test_json_simple_object() {
        let json = Json::new(serde_json::json!({"name": "Alice"})).indent(2);
        let text = json.to_plain_string();
        assert!(text.contains("{\n"));
        assert!(text.contains("\"name\""));
        assert!(text.contains(": \"Alice\""));
        assert!(text.contains('}'));
    }

    #[test]
    fn test_json_compact_object_has_spaces() {
        let json = Json::new(serde_json::json!({"age": 30, "name": "Alice"})).compact();
        let text = json.to_plain_string();
        assert!(text.starts_with('{'));
        assert!(text.ends_with('}'));
        assert!(text.contains("\"age\": 30"));
        assert!(text.contains(", \"name\""));
        assert!(text.contains(": "));
        assert!(text.contains(", "));
        assert!(!text.contains('\n'));
    }

    #[test]
    fn test_json_nested_object() {
        let json = Json::new(serde_json::json!({
            "person": {
                "name": "Alice",
                "age": 30
            }
        }))
        .indent(2);
        let text = json.to_plain_string();
        assert!(text.contains("\"person\""));
        assert!(text.contains("\"name\""));
        assert!(text.contains("\"Alice\""));
        assert!(text.contains("\"age\""));
        assert!(text.contains("30"));
    }

    #[test]
    fn test_json_from_str() {
        let json = Json::from_str(r#"{"key": "value"}"#).unwrap();
        let text = json.to_plain_string();
        assert!(text.contains("\"key\""));
        assert!(text.contains("\"value\""));
    }

    #[test]
    fn test_json_from_str_invalid() {
        let result = Json::from_str("not valid json");
        assert!(result.is_err());
    }

    #[test]
    fn test_json_from_data_round_trip() {
        #[derive(Serialize)]
        struct X {
            a: i32,
        }
        let json = Json::from_data(&X { a: 1 }).unwrap();
        assert!(json.to_plain_string().contains("\"a\""));
    }

    #[test]
    fn test_json_sort_keys() {
        let json = Json::new(serde_json::json!({"z": 1, "a": 2, "m": 3})).sort_keys(true);
        let text = json.to_plain_string();

        // Find positions of keys
        let pos_a = text.find("\"a\"").unwrap();
        let pos_m = text.find("\"m\"").unwrap();
        let pos_z = text.find("\"z\"").unwrap();

        // Keys should appear in sorted order
        assert!(pos_a < pos_m);
        assert!(pos_m < pos_z);
    }

    #[test]
    fn test_json_no_highlight() {
        let json = Json::new(serde_json::json!("test")).highlight(false);
        let segments = json.render();
        // Without highlighting, styles should be None
        assert!(segments.iter().all(|s| s.style.is_none()));
    }

    #[test]
    fn test_json_with_highlight() {
        let json = Json::new(serde_json::json!("test")).highlight(true);
        let segments = json.render();
        // With highlighting, string should have a style
        assert!(segments.iter().any(|s| s.style.is_some()));
    }

    #[test]
    fn test_json_custom_indent() {
        let json = Json::new(serde_json::json!([1])).indent(4);
        let text = json.to_plain_string();
        // Should have 4-space indentation
        assert!(text.contains("    1"));
    }

    #[test]
    fn test_json_mixed_array() {
        let json = Json::new(serde_json::json!([1, "two", true, null]));
        let text = json.to_plain_string();
        assert!(text.contains('1'));
        assert!(text.contains("\"two\""));
        assert!(text.contains("true"));
        assert!(text.contains("null"));
    }

    #[test]
    fn test_json_complex() {
        let json = Json::new(serde_json::json!({
            "users": [
                {"name": "Alice", "active": true},
                {"name": "Bob", "active": false}
            ],
            "count": 2,
            "meta": null
        }))
        .sort_keys(true);

        let text = json.to_plain_string();
        assert!(text.contains("\"users\""));
        assert!(text.contains("\"count\""));
        assert!(text.contains("\"meta\""));
        assert!(text.contains("\"Alice\""));
        assert!(text.contains("\"Bob\""));
        assert!(text.contains("true"));
        assert!(text.contains("false"));
        assert!(text.contains("null"));
        assert!(text.contains('2'));
    }

    #[test]
    fn test_escape_json_string() {
        assert_eq!(escape_json_string("hello", false), "hello");
        assert_eq!(escape_json_string("say \"hi\"", false), "say \\\"hi\\\"");
        assert_eq!(escape_json_string("a\\b", false), "a\\\\b");
        assert_eq!(escape_json_string("line1\nline2", false), "line1\\nline2");
        assert_eq!(escape_json_string("tab\there", false), "tab\\there");
        assert_eq!(escape_json_string("\u{0008}", false), "\\b");
        assert_eq!(escape_json_string("\u{000c}", false), "\\f");
    }

    #[test]
    fn test_json_custom_theme() {
        let theme = JsonTheme {
            key: Style::new().color_str("red").unwrap_or_default(),
            string: Style::new().color_str("blue").unwrap_or_default(),
            number: Style::new().color_str("green").unwrap_or_default(),
            bool_true: Style::new().color_str("yellow").unwrap_or_default(),
            bool_false: Style::new().color_str("yellow").unwrap_or_default(),
            null: Style::new().color_str("white").unwrap_or_default(),
            bracket: Style::new().color_str("cyan").unwrap_or_default(),
            punctuation: Style::new().color_str("magenta").unwrap_or_default(),
        };

        let json = Json::new(serde_json::json!({"key": "value"})).theme(theme);
        let segments = json.render();

        // Should have styled segments
        assert!(segments.iter().any(|s| s.style.is_some()));
    }
}
