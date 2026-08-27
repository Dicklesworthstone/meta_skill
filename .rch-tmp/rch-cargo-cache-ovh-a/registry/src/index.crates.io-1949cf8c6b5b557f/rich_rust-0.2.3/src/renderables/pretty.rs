//! Pretty printing / inspection helpers.
//!
//! This is a Rust-idiomatic implementation inspired by Python Rich's `rich.pretty`
//! and `rich.inspect` modules.
//!
//! ## Differences vs Python Rich
//!
//! Rust doesn't support general-purpose runtime reflection of struct fields and
//! attributes (like Python). As a result:
//! - `Pretty` renders values via their `Debug` representation.
//! - `Inspect` can show the Rust type name and a pretty representation; it may
//!   also extract *simple* top-level `Debug` struct fields when available, but
//!   this depends on the `Debug` implementation.

use std::any;
use std::fmt::Debug;

use crate::cells::cell_len;
use crate::console::{Console, ConsoleOptions};
use crate::renderables::Renderable;
use crate::segment::Segment;
use crate::style::Style;
use crate::text::Text;

use super::table::{Column, Table};

/// Configuration for [`Pretty`].
#[derive(Debug, Clone)]
pub struct PrettyOptions {
    /// Override the width used for wrapping (defaults to `ConsoleOptions.max_width`).
    pub max_width: Option<usize>,
    /// If true, use compact `Debug` (`{:?}`) instead of pretty `Debug` (`{:#?}`).
    pub compact: bool,
    /// If true, wrap long lines to `max_width`.
    pub wrap: bool,
}

impl Default for PrettyOptions {
    fn default() -> Self {
        Self {
            max_width: None,
            compact: false,
            wrap: true,
        }
    }
}

/// Render a Rust value using a stable, width-aware `Debug` representation.
///
/// This "pretty printer" is intended for terminal UIs and is based on `Debug`
/// output (no general-purpose runtime reflection).
#[derive(Debug)]
pub struct Pretty<'a, T: Debug + ?Sized> {
    value: &'a T,
    options: PrettyOptions,
    style: Option<Style>,
}

impl<'a, T: Debug + ?Sized> Pretty<'a, T> {
    /// Create a new [`Pretty`] wrapper.
    #[must_use]
    pub fn new(value: &'a T) -> Self {
        Self {
            value,
            options: PrettyOptions::default(),
            style: None,
        }
    }

    /// Override the wrapping width.
    #[must_use]
    pub fn max_width(mut self, width: usize) -> Self {
        self.options.max_width = Some(width);
        self
    }

    /// Render using compact `Debug` output (`{:?}`).
    #[must_use]
    pub fn compact(mut self, compact: bool) -> Self {
        self.options.compact = compact;
        self
    }

    /// Enable/disable wrapping.
    #[must_use]
    pub fn wrap(mut self, wrap: bool) -> Self {
        self.options.wrap = wrap;
        self
    }

    /// Apply a style to the entire pretty output.
    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }
}

impl<T: Debug + ?Sized> Renderable for Pretty<'_, T> {
    fn render<'a>(&'a self, _console: &Console, options: &ConsoleOptions) -> Vec<Segment<'a>> {
        let width = self.options.max_width.unwrap_or(options.max_width).max(1);

        let repr = if self.options.compact {
            format!("{:?}", self.value)
        } else {
            format!("{:#?}", self.value)
        };

        let lines: Vec<String> = if self.options.wrap {
            wrap_debug_preserving_indent(&repr, width)
        } else {
            repr.lines().map(str::to_string).collect()
        };

        let mut segments: Vec<Segment<'static>> = Vec::new();
        let line_count = lines.len();
        for (idx, line) in lines.into_iter().enumerate() {
            segments.push(Segment::new(line, self.style.clone()));
            if idx + 1 < line_count {
                segments.push(Segment::line());
            }
        }

        segments.into_iter().collect()
    }
}

/// Configuration for [`Inspect`].
#[derive(Debug, Clone)]
pub struct InspectOptions {
    /// Override the width used for rendering (defaults to `ConsoleOptions.max_width`).
    pub max_width: Option<usize>,
    /// Show the Rust type name.
    pub show_type: bool,
    /// Attempt to extract simple top-level fields from `Debug` output.
    pub show_fields: bool,
}

impl Default for InspectOptions {
    fn default() -> Self {
        Self {
            max_width: None,
            show_type: true,
            show_fields: true,
        }
    }
}

/// Inspect a Rust value: show its type and a readable representation.
///
/// This is inspired by Python Rich's `inspect`, but is limited by Rust's lack
/// of runtime reflection. Field extraction follows a documented heuristic over
/// the value's `Debug` output; types with custom `Debug` impls may not expose
/// field structure.
#[derive(Debug)]
pub struct Inspect<'a, T: Debug + ?Sized> {
    value: &'a T,
    options: InspectOptions,
}

impl<'a, T: Debug + ?Sized> Inspect<'a, T> {
    /// Create a new inspector.
    #[must_use]
    pub fn new(value: &'a T) -> Self {
        Self {
            value,
            options: InspectOptions::default(),
        }
    }

    /// Override the rendering width.
    #[must_use]
    pub fn max_width(mut self, width: usize) -> Self {
        self.options.max_width = Some(width);
        self
    }

    /// Show/hide the type line.
    #[must_use]
    pub fn show_type(mut self, show: bool) -> Self {
        self.options.show_type = show;
        self
    }

    /// Enable/disable field extraction.
    #[must_use]
    pub fn show_fields(mut self, show: bool) -> Self {
        self.options.show_fields = show;
        self
    }
}

impl<T: Debug + ?Sized> Renderable for Inspect<'_, T> {
    fn render<'a>(&'a self, console: &Console, options: &ConsoleOptions) -> Vec<Segment<'a>> {
        let width = self.options.max_width.unwrap_or(options.max_width).max(1);

        let mut output: Vec<Segment<'static>> = Vec::new();

        if self.options.show_type {
            let type_name = any::type_name_of_val(self.value);
            let header =
                Text::assemble(&[("Type: ", Some(Style::new().bold())), (type_name, None)]);
            output.extend(header.render("").into_iter().map(Segment::into_owned));
            output.push(Segment::line());
        }

        if self.options.show_fields {
            let repr = format!("{:#?}", self.value);
            if let Some(fields) = extract_simple_struct_fields(&repr) {
                let mut table = Table::new()
                    .with_column(Column::new("Field").style(Style::new().bold()))
                    .with_column(Column::new("Value"));
                for (name, value) in fields {
                    table.add_row_cells([name, value]);
                }
                let mut rendered: Vec<Segment<'static>> = table.render(width);
                output.append(&mut rendered);
                return output.into_iter().collect();
            }
        }

        let pretty = Pretty::new(self.value).max_width(width);
        output.extend(
            pretty
                .render(console, options)
                .into_iter()
                .map(Segment::into_owned),
        );
        output.into_iter().collect()
    }
}

/// Convenience helper to print an [`Inspect`] view to a [`Console`].
pub fn inspect<T: Debug + ?Sized>(console: &Console, value: &T) {
    let renderable = Inspect::new(value);
    console.print_renderable(&renderable);
}

fn wrap_debug_preserving_indent(text: &str, width: usize) -> Vec<String> {
    text.lines()
        .flat_map(|line| wrap_line_preserving_indent(line, width))
        .collect()
}

fn wrap_line_preserving_indent(line: &str, width: usize) -> Vec<String> {
    let indent_len = line.chars().take_while(|c| c.is_whitespace()).count();
    let indent: String = line.chars().take(indent_len).collect();
    let rest: String = line.chars().skip(indent_len).collect();

    let indent_width = cell_len(&indent);
    if rest.is_empty() || width <= indent_width + 1 {
        return vec![line.to_string()];
    }

    let available = width.saturating_sub(indent_width).max(1);
    let wrapped = Text::new(rest).wrap(available);
    wrapped
        .into_iter()
        .map(|t| format!("{indent}{}", t.plain()))
        .collect()
}

fn extract_simple_struct_fields(repr: &str) -> Option<Vec<(String, String)>> {
    let mut lines = repr.lines().peekable();
    let first = lines.next()?.trim_end();
    if !first.ends_with('{') {
        return None;
    }

    let mut fields = Vec::new();
    let mut current_field: Option<(String, String)> = None;
    let mut nesting_depth = 0;

    for line in lines {
        let trimmed = line.trim_end();
        if trimmed == "}" && nesting_depth == 0 {
            // End of top-level struct, save any pending field
            if let Some((name, value)) = current_field.take() {
                fields.push((name, value));
            }
            break;
        }

        // Only consider lines indented exactly 4 spaces (top-level fields)
        let Some(stripped) = trimmed.strip_prefix("    ") else {
            // Handle continuation of multi-line values
            if nesting_depth > 0 || current_field.is_some() {
                // Track nesting for multi-line values
                nesting_depth += trimmed.chars().filter(|&c| c == '[' || c == '{').count();
                nesting_depth = nesting_depth
                    .saturating_sub(trimmed.chars().filter(|&c| c == ']' || c == '}').count());
            }
            continue;
        };

        // Check if this line is further indented (part of a nested structure)
        if stripped.starts_with(' ') || stripped.starts_with('\t') {
            // This is a nested field, not a top-level one - track nesting
            nesting_depth += stripped.chars().filter(|&c| c == '[' || c == '{').count();
            nesting_depth = nesting_depth
                .saturating_sub(stripped.chars().filter(|&c| c == ']' || c == '}').count());
            continue;
        }

        // Save any pending field before starting a new one
        if let Some((name, value)) = current_field.take() {
            fields.push((name, value));
        }

        // Parse field name and value
        let Some((name, value)) = stripped.split_once(':') else {
            continue;
        };
        let name = name.to_string();
        if name.is_empty() {
            continue;
        }
        let mut value = value.trim().to_string();
        if value.ends_with(',') {
            value.pop();
            value = value.trim_end().to_string();
        }
        if value.is_empty() {
            continue;
        }

        // Track nesting for multi-line values
        nesting_depth = value.chars().filter(|&c| c == '[' || c == '{').count();
        nesting_depth =
            nesting_depth.saturating_sub(value.chars().filter(|&c| c == ']' || c == '}').count());

        // Simplify nested structures for display
        if (value.starts_with('[') && !value.ends_with(']'))
            || (value.starts_with('{') && !value.ends_with('}'))
        {
            // Multi-line array or struct - show as collapsed
            let opener = value.chars().next().unwrap();
            let closer = if opener == '[' { ']' } else { '}' };
            value = format!("{opener}...{closer}");
            nesting_depth = 0; // We're collapsing, so reset depth
        } else if value.ends_with('{') && nesting_depth > 0 {
            // Named struct like "Inner {" -> "Inner {...}"
            value = format!("{value}...}}");
            nesting_depth = 0;
        }

        current_field = Some((name, value));
    }

    // Handle any remaining field
    if let Some((name, value)) = current_field {
        fields.push((name, value));
    }

    if fields.is_empty() {
        None
    } else {
        Some(fields)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::Console;
    use std::collections::HashMap;

    #[derive(Debug)]
    #[allow(dead_code)]
    struct Inner {
        name: String,
        values: Vec<i32>,
    }

    #[derive(Debug)]
    #[allow(dead_code)]
    struct Outer {
        id: u32,
        inner: Inner,
    }

    #[derive(Debug)]
    #[allow(dead_code)]
    struct Simple {
        field1: String,
        field2: i32,
    }

    fn test_console(width: usize) -> Console {
        Console::builder()
            .no_color()
            .force_terminal(false)
            .emoji(false)
            .markup(false)
            .highlight(false)
            .width(width)
            .build()
    }

    // =========================================================================
    // PrettyOptions Tests
    // =========================================================================

    #[test]
    fn test_pretty_options_default() {
        let options = PrettyOptions::default();
        assert!(options.max_width.is_none());
        assert!(!options.compact);
        assert!(options.wrap);
    }

    #[test]
    fn test_pretty_options_custom() {
        let options = PrettyOptions {
            max_width: Some(50),
            compact: true,
            wrap: false,
        };
        assert_eq!(options.max_width, Some(50));
        assert!(options.compact);
        assert!(!options.wrap);
    }

    // =========================================================================
    // InspectOptions Tests
    // =========================================================================

    #[test]
    fn test_inspect_options_default() {
        let options = InspectOptions::default();
        assert!(options.max_width.is_none());
        assert!(options.show_type);
        assert!(options.show_fields);
    }

    #[test]
    fn test_inspect_options_custom() {
        let options = InspectOptions {
            max_width: Some(100),
            show_type: false,
            show_fields: false,
        };
        assert_eq!(options.max_width, Some(100));
        assert!(!options.show_type);
        assert!(!options.show_fields);
    }

    // =========================================================================
    // Pretty Creation Tests
    // =========================================================================

    #[test]
    fn test_pretty_new() {
        let value = 42i32;
        let pretty = Pretty::new(&value);
        assert!(!pretty.options.compact);
        assert!(pretty.options.wrap);
        assert!(pretty.style.is_none());
    }

    #[test]
    fn test_pretty_builder_chain() {
        let value = "test";
        let pretty = Pretty::new(&value)
            .max_width(40)
            .compact(true)
            .wrap(false)
            .style(Style::new().bold());

        assert_eq!(pretty.options.max_width, Some(40));
        assert!(pretty.options.compact);
        assert!(!pretty.options.wrap);
        assert!(pretty.style.is_some());
    }

    // =========================================================================
    // Inspect Creation Tests
    // =========================================================================

    #[test]
    fn test_inspect_new() {
        let value = 42i32;
        let inspect = Inspect::new(&value);
        assert!(inspect.options.show_type);
        assert!(inspect.options.show_fields);
    }

    #[test]
    fn test_inspect_builder_chain() {
        let value = "test";
        let inspect = Inspect::new(&value)
            .max_width(80)
            .show_type(false)
            .show_fields(false);

        assert_eq!(inspect.options.max_width, Some(80));
        assert!(!inspect.options.show_type);
        assert!(!inspect.options.show_fields);
    }

    // =========================================================================
    // Pretty Rendering Tests
    // =========================================================================

    #[test]
    fn test_pretty_render_primitive() {
        let value = 42i32;
        let console = test_console(80);
        let pretty = Pretty::new(&value);
        let options = console.options();
        let segments = pretty.render(&console, &options);

        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("42"));
    }

    #[test]
    fn test_pretty_render_string() {
        let value = "Hello, World!";
        let console = test_console(80);
        let pretty = Pretty::new(&value);
        let options = console.options();
        let segments = pretty.render(&console, &options);

        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("Hello"));
    }

    #[test]
    fn test_pretty_render_struct() {
        let value = Simple {
            field1: "test".to_string(),
            field2: 123,
        };
        let console = test_console(80);
        let pretty = Pretty::new(&value);
        let options = console.options();
        let segments = pretty.render(&console, &options);

        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("Simple"));
        assert!(text.contains("field1"));
        assert!(text.contains("field2"));
        assert!(text.contains("test"));
        assert!(text.contains("123"));
    }

    #[test]
    fn test_pretty_render_vec() {
        let value = vec![1, 2, 3, 4, 5];
        let console = test_console(80);
        let pretty = Pretty::new(&value);
        let options = console.options();
        let segments = pretty.render(&console, &options);

        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains('1'));
        assert!(text.contains('5'));
    }

    #[test]
    fn test_pretty_render_compact() {
        let value = Simple {
            field1: "test".to_string(),
            field2: 123,
        };
        let console = test_console(80);
        let pretty = Pretty::new(&value).compact(true);
        let options = console.options();
        let segments = pretty.render(&console, &options);

        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        // Compact format should be single line
        assert!(!text.contains('\n') || text.lines().count() == 1);
    }

    #[test]
    fn test_pretty_render_no_wrap() {
        let value = "a".repeat(100);
        let console = test_console(20);
        let pretty = Pretty::new(&value).wrap(false);
        let options = console.options();
        let segments = pretty.render(&console, &options);

        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        // Without wrapping, line should exceed 20 chars
        let longest_line = text.lines().map(str::len).max().unwrap_or(0);
        assert!(longest_line > 20);
    }

    #[test]
    fn test_pretty_render_with_style() {
        let value = 42i32;
        let console = test_console(80);
        let style = Style::new().bold();
        let pretty = Pretty::new(&value).style(style.clone());
        let options = console.options();
        let segments = pretty.render(&console, &options);

        // At least one segment should have the style
        assert!(segments.iter().any(|s| s.style.as_ref() == Some(&style)));
    }

    // =========================================================================
    // Inspect Rendering Tests
    // =========================================================================

    #[test]
    fn test_inspect_render_with_type() {
        let value = 42i32;
        let console = test_console(80);
        let inspect = Inspect::new(&value).show_type(true).show_fields(false);
        let options = console.options();
        let segments = inspect.render(&console, &options);

        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("Type:"));
        assert!(text.contains("i32"));
    }

    #[test]
    fn test_inspect_render_without_type() {
        let value = 42i32;
        let console = test_console(80);
        let inspect = Inspect::new(&value).show_type(false);
        let options = console.options();
        let segments = inspect.render(&console, &options);

        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(!text.contains("Type:"));
    }

    #[test]
    fn test_inspect_render_struct_fields() {
        let value = Simple {
            field1: "hello".to_string(),
            field2: 42,
        };
        let console = test_console(80);
        let inspect = Inspect::new(&value).show_fields(true);
        let options = console.options();
        let segments = inspect.render(&console, &options);

        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        // Should contain field names
        assert!(text.contains("field1") || text.contains("Simple"));
    }

    #[test]
    fn test_inspect_render_without_fields() {
        let value = Simple {
            field1: "hello".to_string(),
            field2: 42,
        };
        let console = test_console(80);
        let inspect = Inspect::new(&value).show_type(false).show_fields(false);
        let options = console.options();
        let segments = inspect.render(&console, &options);

        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        // Should fall back to Pretty output
        assert!(text.contains("Simple"));
    }

    // =========================================================================
    // wrap_debug_preserving_indent Tests
    // =========================================================================

    #[test]
    fn test_wrap_debug_short_lines() {
        let text = "Short line\nAnother short";
        let wrapped = wrap_debug_preserving_indent(text, 80);
        assert_eq!(wrapped.len(), 2);
        assert_eq!(wrapped[0], "Short line");
        assert_eq!(wrapped[1], "Another short");
    }

    #[test]
    fn test_wrap_debug_with_indent() {
        let text = "    indented line";
        let wrapped = wrap_debug_preserving_indent(text, 80);
        assert_eq!(wrapped.len(), 1);
        assert!(wrapped[0].starts_with("    "));
    }

    #[test]
    fn test_wrap_line_preserving_indent_empty() {
        let wrapped = wrap_line_preserving_indent("", 80);
        assert_eq!(wrapped.len(), 1);
        assert_eq!(wrapped[0], "");
    }

    #[test]
    fn test_wrap_line_preserving_indent_only_whitespace() {
        let wrapped = wrap_line_preserving_indent("    ", 80);
        assert_eq!(wrapped.len(), 1);
    }

    #[test]
    fn test_wrap_line_width_too_small() {
        let wrapped = wrap_line_preserving_indent("    some text", 2);
        // When width is too small, should return original
        assert_ne!(wrapped.len(), 0);
    }

    // =========================================================================
    // extract_simple_struct_fields Tests
    // =========================================================================

    #[test]
    fn test_extract_simple_struct_fields_valid() {
        let repr = "MyStruct {\n    field1: \"value\",\n    field2: 42,\n}";
        let fields = extract_simple_struct_fields(repr);
        assert!(fields.is_some());
        let fields = fields.unwrap();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].0, "field1");
        assert_eq!(fields[0].1, "\"value\"");
        assert_eq!(fields[1].0, "field2");
        assert_eq!(fields[1].1, "42");
    }

    #[test]
    fn test_extract_simple_struct_fields_no_brace() {
        let repr = "NotAStruct";
        let fields = extract_simple_struct_fields(repr);
        assert!(fields.is_none());
    }

    #[test]
    fn test_extract_simple_struct_fields_empty() {
        let repr = "EmptyStruct {\n}";
        let fields = extract_simple_struct_fields(repr);
        assert!(fields.is_none()); // No fields extracted
    }

    #[test]
    fn test_extract_simple_struct_fields_nested() {
        // Nested structs have deeper indentation
        let repr = "Outer {\n    inner: Inner {\n        field: 1,\n    },\n}";
        let fields = extract_simple_struct_fields(repr);
        // Should only extract top-level fields
        assert!(fields.is_some());
        let fields = fields.unwrap();
        // "inner" should be extracted with the collapsed value
        let inner_field = fields.iter().find(|(name, _)| name == "inner");
        assert!(inner_field.is_some(), "should have 'inner' field");
        let (_, value) = inner_field.unwrap();
        assert_eq!(value, "Inner {...}", "nested struct should be collapsed");
    }

    #[test]
    fn test_extract_simple_struct_fields_array_of_structs() {
        // Bug bd-1bt2: nested struct fields were being extracted as top-level
        let repr = r#"DemoState {
    name: "test",
    services: [
        Service {
            name: "api",
            health: Ok,
            latency: 12,
        },
        Service {
            name: "worker",
            health: Warn,
            latency: 45,
        },
    ],
    count: 2,
}"#;
        let fields = extract_simple_struct_fields(repr);
        assert!(fields.is_some());
        let fields = fields.unwrap();

        // Should extract only top-level fields
        let names: Vec<&str> = fields.iter().map(|(n, _)| n.as_str()).collect();
        assert!(names.contains(&"name"), "Should have 'name' field");
        assert!(names.contains(&"services"), "Should have 'services' field");
        assert!(names.contains(&"count"), "Should have 'count' field");

        // Should NOT have nested field names at top level
        assert!(
            !names.contains(&"health"),
            "Should NOT have nested 'health' at top level"
        );
        assert!(
            !names.contains(&"latency"),
            "Should NOT have nested 'latency' at top level"
        );

        // services value should be collapsed
        let services = fields.iter().find(|(n, _)| n == "services").unwrap();
        assert_eq!(
            services.1, "[...]",
            "Nested array should be collapsed to [...]"
        );
    }

    #[test]
    fn test_extract_simple_struct_fields_no_colon() {
        let repr = "TupleStruct {\n    element1\n}";
        let fields = extract_simple_struct_fields(repr);
        assert!(fields.is_none()); // No colon means no key-value
    }

    // =========================================================================
    // inspect helper function Tests
    // =========================================================================

    #[test]
    fn test_inspect_helper_function() {
        let console = test_console(80);
        let value = 42i32;
        // Should not panic
        inspect(&console, &value);
    }

    // =========================================================================
    // Edge Cases
    // =========================================================================

    #[test]
    fn test_pretty_render_nested_struct() {
        let value = Outer {
            id: 1,
            inner: Inner {
                name: "nested".to_string(),
                values: vec![1, 2, 3],
            },
        };
        let console = test_console(80);
        let pretty = Pretty::new(&value);
        let options = console.options();
        let segments = pretty.render(&console, &options);

        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("Outer"));
        assert!(text.contains("Inner"));
        assert!(text.contains("nested"));
    }

    #[test]
    fn test_pretty_render_hashmap() {
        let mut map = HashMap::new();
        map.insert("key1", 1);
        map.insert("key2", 2);

        let console = test_console(80);
        let pretty = Pretty::new(&map);
        let options = console.options();
        let segments = pretty.render(&console, &options);

        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("key1") || text.contains("key2"));
    }

    #[test]
    fn test_pretty_render_option_some() {
        let value: Option<i32> = Some(42);
        let console = test_console(80);
        let pretty = Pretty::new(&value);
        let options = console.options();
        let segments = pretty.render(&console, &options);

        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("Some"));
        assert!(text.contains("42"));
    }

    #[test]
    fn test_pretty_render_option_none() {
        let value: Option<i32> = None;
        let console = test_console(80);
        let pretty = Pretty::new(&value);
        let options = console.options();
        let segments = pretty.render(&console, &options);

        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("None"));
    }

    #[test]
    fn test_pretty_render_result_ok() {
        let value: Result<i32, &str> = Ok(42);
        let console = test_console(80);
        let pretty = Pretty::new(&value);
        let options = console.options();
        let segments = pretty.render(&console, &options);

        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("Ok"));
    }

    #[test]
    fn test_pretty_render_result_err() {
        let value: Result<i32, &str> = Err("error");
        let console = test_console(80);
        let pretty = Pretty::new(&value);
        let options = console.options();
        let segments = pretty.render(&console, &options);

        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("Err"));
        assert!(text.contains("error"));
    }

    #[test]
    fn test_pretty_narrow_width() {
        let value = Simple {
            field1: "a-very-long-string-value".to_string(),
            field2: 12345,
        };
        let console = test_console(15);
        let pretty = Pretty::new(&value).wrap(true);
        let options = console.options();
        let segments = pretty.render(&console, &options);

        // Should produce multiple lines due to wrapping
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.lines().count() > 1);
    }

    // =========================================================================
    // Snapshot Tests (kept from original)
    // =========================================================================

    #[test]
    fn pretty_wraps_to_width_and_is_stable() {
        let value = Outer {
            id: 42,
            inner: Inner {
                name: "a-very-long-name-to-wrap".to_string(),
                values: vec![1, 2, 3, 4, 5],
            },
        };
        let console = test_console(22);
        let pretty = Pretty::new(&value);
        let plain = console.export_renderable_text(&pretty);
        insta::assert_snapshot!(plain);
    }

    #[test]
    fn inspect_shows_type_and_fields_when_available() {
        let value = Inner {
            name: "Zed".to_string(),
            values: vec![1, 2, 3],
        };
        let console = test_console(60);
        let inspect = Inspect::new(&value);
        let plain = console.export_renderable_text(&inspect);
        insta::assert_snapshot!(plain);
    }
}
