//! Group renderable for combining multiple renderables.
//!
//! A `Group` combines multiple renderables into a single unit that can be
//! passed to containers like Panel or Layout. This is useful when you want
//! to pass multiple renderables as panel content.
//!
//! # Examples
//!
//! ```rust,ignore
//! use rich_rust::renderables::{Group, Panel, Rule};
//! use rich_rust::text::Text;
//!
//! // Combine multiple renderables into a group
//! let group = Group::new()
//!     .push("First paragraph of text")
//!     .push(Rule::new())
//!     .push("Second paragraph");
//!
//! // Use the group as panel content
//! let panel = Panel::from_renderable(&group, 80)
//!     .title("Grouped Content");
//!
//! // Or render directly
//! console.print_renderable(&group);
//! ```
//!
//! # Fit Option
//!
//! By default, each renderable is rendered on its own lines. Use `fit(true)`
//! to attempt to render items inline when they fit.

use crate::console::{Console, ConsoleOptions};
use crate::segment::Segment;

use super::Renderable;

/// A container that groups multiple renderables together.
///
/// Group implements the Renderable trait, allowing you to combine
/// multiple renderables into a single unit. Each child is rendered
/// in sequence with optional separators.
#[derive(Default)]
pub struct Group<'a> {
    /// The renderables in this group.
    children: Vec<Box<dyn Renderable + 'a>>,
    /// Whether to fit items inline when possible.
    fit: bool,
}

impl<'a> Group<'a> {
    /// Create a new empty group.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a renderable to the group.
    ///
    /// The renderable is boxed and stored. You can add any type that
    /// implements the Renderable trait.
    #[must_use]
    pub fn push<R: Renderable + 'a>(mut self, renderable: R) -> Self {
        self.children.push(Box::new(renderable));
        self
    }

    /// Add a boxed renderable to the group.
    #[must_use]
    pub fn push_boxed(mut self, renderable: Box<dyn Renderable + 'a>) -> Self {
        self.children.push(renderable);
        self
    }

    /// Set whether to fit items inline when possible.
    ///
    /// When `fit` is true, items that fit on the same line are
    /// rendered together. When false (default), each item gets its own line.
    #[must_use]
    pub fn fit(mut self, fit: bool) -> Self {
        self.fit = fit;
        self
    }

    /// Check if the group is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.children.is_empty()
    }

    /// Return the number of items in the group.
    #[must_use]
    pub fn len(&self) -> usize {
        self.children.len()
    }
}

impl Renderable for Group<'_> {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> Vec<Segment<'_>> {
        let mut segments = Vec::new();

        for (i, child) in self.children.iter().enumerate() {
            // Add newline between items unless fit mode is on
            if i > 0 && !self.fit {
                segments.push(Segment::new("\n".to_string(), None));
            }

            // Render the child
            let child_segments = child.render(console, options);
            segments.extend(child_segments.into_iter().map(Segment::into_owned));
        }

        segments
    }
}

/// Create a group from an iterator of renderables.
///
/// This is a convenience function for creating groups from iterators.
///
/// # Examples
///
/// ```rust,ignore
/// use rich_rust::renderables::group::group;
///
/// let items = vec!["Item 1", "Item 2", "Item 3"];
/// let g = group(items.into_iter());
/// ```
pub fn group<'a, I, R>(iter: I) -> Group<'a>
where
    I: IntoIterator<Item = R>,
    R: Renderable + 'a,
{
    let mut g = Group::new();
    for item in iter {
        g = g.push(item);
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::console::Console;

    #[test]
    fn test_group_new() {
        let g: Group = Group::new();
        assert!(g.is_empty());
        assert_eq!(g.len(), 0);
    }

    #[test]
    fn test_group_add_strings() {
        let g = Group::new().push("First").push("Second").push("Third");
        assert_eq!(g.len(), 3);
    }

    #[test]
    fn test_group_render() {
        let g = Group::new().push("Line 1").push("Line 2");

        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .build();
        let options = console.options();

        let segments = g.render(&console, &options);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();

        assert!(text.contains("Line 1"));
        assert!(text.contains("Line 2"));
        assert!(text.contains('\n'), "should have newline between items");
    }

    #[test]
    fn test_group_render_fit_mode() {
        let g = Group::new().push("Part1").push("Part2").fit(true);

        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .build();
        let options = console.options();

        let segments = g.render(&console, &options);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();

        assert!(text.contains("Part1"));
        assert!(text.contains("Part2"));
        // In fit mode, no newlines between items
        assert!(!text.contains('\n'));
    }

    #[test]
    fn test_group_function() {
        let items = vec!["A", "B", "C"];
        let g = group(items);
        assert_eq!(g.len(), 3);
    }

    #[test]
    fn test_group_single_item() {
        let g = Group::new().push("Solo");
        assert_eq!(g.len(), 1);
        assert!(!g.is_empty());

        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .build();
        let options = console.options();

        let segments = g.render(&console, &options);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("Solo"));
        // Single item should have no newlines
        assert!(!text.contains('\n'));
    }

    #[test]
    fn test_group_empty_render() {
        let g: Group = Group::new();

        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .build();
        let options = console.options();

        let segments = g.render(&console, &options);
        assert_eq!(segments.len(), 0);
    }

    #[test]
    fn test_group_nested() {
        let inner = Group::new().push("Inner 1").push("Inner 2");
        let outer = Group::new().push("Outer").push(inner);

        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .build();
        let options = console.options();

        let segments = outer.render(&console, &options);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();

        assert!(text.contains("Outer"));
        assert!(text.contains("Inner 1"));
        assert!(text.contains("Inner 2"));
    }

    #[test]
    fn test_group_push_boxed() {
        let boxed: Box<dyn Renderable> = Box::new("Boxed content");
        let g = Group::new().push_boxed(boxed);
        assert_eq!(g.len(), 1);

        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .build();
        let options = console.options();

        let segments = g.render(&console, &options);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("Boxed content"));
    }

    #[test]
    fn test_group_is_empty_after_push() {
        let g = Group::new();
        assert!(g.is_empty());

        let g2 = g.push("Item");
        assert!(!g2.is_empty());
    }

    #[test]
    fn test_group_default() {
        let g: Group = Group::default();
        assert!(g.is_empty());
        assert_eq!(g.len(), 0);
        assert!(!g.fit);
    }

    #[test]
    fn test_group_fit_toggle() {
        let g = Group::new().fit(false);
        assert!(!g.fit);

        let g2 = g.fit(true);
        assert!(g2.fit);
    }

    #[test]
    fn test_group_many_items() {
        let mut g = Group::new();
        for i in 0..10 {
            g = g.push(format!("Item {i}"));
        }
        assert_eq!(g.len(), 10);

        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .build();
        let options = console.options();

        let segments = g.render(&console, &options);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();

        for i in 0..10 {
            assert!(text.contains(&format!("Item {i}")));
        }
    }

    #[test]
    fn test_group_fit_with_multiple() {
        // Test that fit mode works with many items
        let g = Group::new()
            .push("A")
            .push("B")
            .push("C")
            .push("D")
            .fit(true);

        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .build();
        let options = console.options();

        let segments = g.render(&console, &options);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();

        // In fit mode, all items should be on same conceptual line
        assert!(!text.contains('\n'));
        assert!(text.contains('A'));
        assert!(text.contains('D'));
    }

    #[test]
    fn test_group_empty_items() {
        // Group with empty strings should still work
        let g = Group::new().push("").push("Middle").push("");

        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .build();
        let options = console.options();

        let segments = g.render(&console, &options);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("Middle"));
        assert_eq!(g.len(), 3);
    }

    // =========================================================================
    // Mixed Renderable Types
    // =========================================================================

    #[test]
    fn test_group_mixed_text_and_str() {
        use crate::text::Text;

        let text_obj = Text::new("Styled text");
        let g = Group::new()
            .push("Plain string")
            .push(text_obj)
            .push("Another string");

        assert_eq!(g.len(), 3);

        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .build();
        let options = console.options();

        let segments = g.render(&console, &options);
        let output: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(output.contains("Plain string"));
        assert!(output.contains("Styled text"));
        assert!(output.contains("Another string"));
    }

    #[test]
    fn test_group_mixed_str_and_rule() {
        use crate::renderables::Rule;

        let g = Group::new()
            .push("Header text")
            .push(Rule::new())
            .push("Footer text");

        assert_eq!(g.len(), 3);

        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .build();
        let options = console.options();

        let segments = g.render(&console, &options);
        let output: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(output.contains("Header text"));
        assert!(output.contains("Footer text"));
        // Rule renders some characters (typically ─)
        assert!(segments.len() >= 3);
    }

    #[test]
    fn test_group_mixed_text_rule_string() {
        use crate::renderables::Rule;
        use crate::text::Text;

        let g = Group::new()
            .push(Text::new("Rich text"))
            .push(Rule::new())
            .push("Plain str".to_string());

        assert_eq!(g.len(), 3);

        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .build();
        let options = console.options();

        let segments = g.render(&console, &options);
        let output: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(output.contains("Rich text"));
        assert!(output.contains("Plain str"));
    }

    #[test]
    fn test_group_boxed_mixed_types() {
        use crate::text::Text;

        let boxed_str: Box<dyn Renderable> = Box::new("boxed str");
        let boxed_text: Box<dyn Renderable> = Box::new(Text::new("boxed text"));

        let g = Group::new().push_boxed(boxed_str).push_boxed(boxed_text);
        assert_eq!(g.len(), 2);

        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .build();
        let options = console.options();

        let segments = g.render(&console, &options);
        let output: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(output.contains("boxed str"));
        assert!(output.contains("boxed text"));
    }

    // =========================================================================
    // Width Propagation
    // =========================================================================

    #[test]
    fn test_group_width_propagation_narrow() {
        use crate::renderables::Rule;

        let g = Group::new().push(Rule::new());

        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .width(20)
            .build();
        let options = console.options();

        let segments = g.render(&console, &options);
        // Rule should render to fit within the narrow width
        let total_width: usize = segments.iter().map(Segment::cell_length).sum();
        assert!(
            total_width <= 20,
            "Rule should fit in 20 columns, got {total_width}"
        );
    }

    #[test]
    fn test_group_width_propagation_wide() {
        use crate::renderables::Rule;

        let g = Group::new().push(Rule::new());

        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .width(120)
            .build();
        let options = console.options();

        let segments = g.render(&console, &options);
        let total_width: usize = segments.iter().map(Segment::cell_length).sum();
        // Rule should expand to fill the wider width
        assert!(
            total_width > 20,
            "Rule at width 120 should be wider than 20 chars, got {total_width}"
        );
    }

    #[test]
    fn test_group_width_update() {
        let g = Group::new().push("Short").push("Also short");

        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .width(40)
            .build();
        let options = console.options();
        let narrowed = options.update_width(10);

        // Both option sets should produce valid renders
        let segs_normal = g.render(&console, &options);
        let segs_narrow = g.render(&console, &narrowed);

        assert_ne!(segs_normal.len(), 0);
        assert_ne!(segs_narrow.len(), 0);
    }

    // =========================================================================
    // Segment Joining Behavior
    // =========================================================================

    #[test]
    fn test_group_newline_segments_between_items() {
        let g = Group::new().push("A").push("B").push("C");

        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .build();
        let options = console.options();

        let segments = g.render(&console, &options);

        // Count newline segments - should have 2 (between A-B and B-C)
        let newline_count = segments.iter().filter(|s| s.text.as_ref() == "\n").count();
        assert_eq!(
            newline_count, 2,
            "Should have 2 newlines between 3 items, got {newline_count}"
        );
    }

    #[test]
    fn test_group_no_newline_in_fit_mode() {
        let g = Group::new().push("A").push("B").push("C").fit(true);

        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .build();
        let options = console.options();

        let segments = g.render(&console, &options);

        let newline_count = segments.iter().filter(|s| s.text.as_ref() == "\n").count();
        assert_eq!(
            newline_count, 0,
            "Fit mode should have no newlines, got {newline_count}"
        );
    }

    #[test]
    fn test_group_segments_are_owned() {
        // Group's render uses into_owned() on child segments,
        // so the returned segments should have 'static-compatible lifetimes
        let g = Group::new().push("test content");

        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .build();
        let options = console.options();

        let segments = g.render(&console, &options);
        // Verify segments contain expected content
        let any_match = segments
            .iter()
            .any(|s| s.text.as_ref().contains("test content"));
        assert!(any_match);
    }

    #[test]
    fn test_group_segment_count_no_fit() {
        // Each push adds child segments + newline separator (except before first)
        let g = Group::new().push("X").push("Y");

        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .build();
        let options = console.options();

        let segments = g.render(&console, &options);
        // "X" renders to >=1 segment, then newline, then "Y" >=1 segment
        assert!(
            segments.len() >= 3,
            "Should have at least 3 segments (X + newline + Y), got {}",
            segments.len()
        );
    }

    // =========================================================================
    // Nested Groups (Extended)
    // =========================================================================

    #[test]
    fn test_group_deeply_nested() {
        let level3 = Group::new().push("L3");
        let level2 = Group::new().push("L2").push(level3);
        let level1 = Group::new().push("L1").push(level2);

        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .build();
        let options = console.options();

        let segments = level1.render(&console, &options);
        let output: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(output.contains("L1"));
        assert!(output.contains("L2"));
        assert!(output.contains("L3"));
    }

    #[test]
    fn test_group_nested_fit_modes() {
        // Outer group: no fit (newlines). Inner group: fit (no newlines).
        let inner = Group::new().push("A").push("B").fit(true);
        let outer = Group::new().push("Before").push(inner).push("After");

        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .build();
        let options = console.options();

        let segments = outer.render(&console, &options);
        let output: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(output.contains("Before"));
        assert!(output.contains('A'));
        assert!(output.contains('B'));
        assert!(output.contains("After"));

        // Inner group rendered in fit mode, so A and B should be adjacent (no newline between them)
        // But outer group adds newlines between its children
        let newlines_in_outer = segments.iter().filter(|s| s.text.as_ref() == "\n").count();
        // Outer has 3 children → 2 newlines from outer
        assert_eq!(newlines_in_outer, 2);
    }

    #[test]
    fn test_group_nested_empty_inner() {
        let inner: Group = Group::new(); // empty
        let outer = Group::new().push("Before").push(inner).push("After");

        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .build();
        let options = console.options();

        let segments = outer.render(&console, &options);
        let output: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(output.contains("Before"));
        assert!(output.contains("After"));
    }

    // =========================================================================
    // group() Helper Function (Extended)
    // =========================================================================

    #[test]
    fn test_group_function_empty_iter() {
        let g = group(std::iter::empty::<&str>());
        assert!(g.is_empty());
        assert_eq!(g.len(), 0);
    }

    #[test]
    fn test_group_function_single_item() {
        let g = group(std::iter::once("only"));
        assert_eq!(g.len(), 1);

        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .build();
        let options = console.options();

        let segments = g.render(&console, &options);
        let output: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(output.contains("only"));
    }

    #[test]
    fn test_group_function_from_vec() {
        let items = vec!["one", "two", "three"];
        let g = group(items);
        assert_eq!(g.len(), 3);
    }

    #[test]
    fn test_group_function_from_owned_strings() {
        let items: Vec<String> = vec!["owned1".into(), "owned2".into()];
        let g = group(items);
        assert_eq!(g.len(), 2);

        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .build();
        let options = console.options();

        let segments = g.render(&console, &options);
        let output: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(output.contains("owned1"));
        assert!(output.contains("owned2"));
    }

    // =========================================================================
    // Builder Chaining
    // =========================================================================

    #[test]
    fn test_group_builder_chain_returns_self() {
        // Verify that push and fit return Self for chaining
        let g = Group::new()
            .fit(false)
            .push("A")
            .push("B")
            .fit(true)
            .push("C");

        assert_eq!(g.len(), 3);
        assert!(g.fit);
    }

    #[test]
    fn test_group_push_after_fit_toggle() {
        let g = Group::new().push("Before fit").fit(true).push("After fit");

        let console = Console::builder()
            .force_terminal(false)
            .markup(false)
            .build();
        let options = console.options();

        let segments = g.render(&console, &options);
        let output: String = segments.iter().map(|s| s.text.as_ref()).collect();
        // fit was set to true, so no newlines
        assert!(!output.contains('\n'));
        assert!(output.contains("Before fit"));
        assert!(output.contains("After fit"));
    }
}
