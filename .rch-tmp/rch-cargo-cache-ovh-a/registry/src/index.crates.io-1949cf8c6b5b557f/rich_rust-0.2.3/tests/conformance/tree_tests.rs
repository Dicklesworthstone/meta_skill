//! Tree markup parsing conformance tests.
//!
//! These tests verify Tree behavior with markup in labels.
//!
//! **Important Note on Tree Markup Behavior:**
//!
//! Tree does NOT automatically parse Rich markup syntax in labels:
//! - `TreeNode::new("...")` - Does NOT parse markup, treats as plain text
//! - `Tree::with_label("...")` - Does NOT parse markup (uses TreeNode::new internally)
//!
//! To use styled content, you must:
//! 1. Create a `Text` object with explicit spans/styles
//! 2. Use `TreeNode::new(styled_text)` with pre-styled Text
//!
//! This is consistent with the Cell and Panel API behavior.

use super::TestCase;
use rich_rust::prelude::*;
use rich_rust::renderables::tree::{Tree, TreeGuides, TreeNode};
use rich_rust::segment::Segment;
use rich_rust::text::Text;

/// Test case for Tree rendering.
#[derive(Debug)]
pub struct TreeTest {
    pub name: &'static str,
    pub root_label: &'static str,
    pub children: Vec<&'static str>,
    pub guides: TreeGuides,
}

impl TestCase for TreeTest {
    fn name(&self) -> &str {
        self.name
    }

    fn render(&self) -> Vec<Segment<'static>> {
        let mut tree = Tree::with_label(self.root_label).guides(self.guides);

        for child in &self.children {
            tree = tree.child(TreeNode::new(*child));
        }

        tree.render().into_iter().map(Segment::into_owned).collect()
    }

    fn python_rich_code(&self) -> Option<String> {
        let children_code = self
            .children
            .iter()
            .map(|c| format!("tree.add(\"{}\")", c))
            .collect::<Vec<_>>()
            .join("\n");

        Some(format!(
            r#"from rich.console import Console
from rich.tree import Tree

console = Console(force_terminal=True, width=80)
tree = Tree("{}")
{}
console.print(tree, end="")"#,
            self.root_label, children_code
        ))
    }
}

/// Standard tree test cases for conformance testing.
pub fn standard_tree_tests() -> Vec<Box<dyn TestCase>> {
    vec![
        Box::new(TreeTest {
            name: "tree_simple",
            root_label: "root",
            children: vec!["child1", "child2"],
            guides: TreeGuides::Unicode,
        }),
        Box::new(TreeTest {
            name: "tree_ascii",
            root_label: "root",
            children: vec!["child1", "child2"],
            guides: TreeGuides::Ascii,
        }),
        Box::new(TreeTest {
            name: "tree_rounded",
            root_label: "root",
            children: vec!["child"],
            guides: TreeGuides::Rounded,
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conformance::run_test;

    // =========================================================================
    // Basic Tree Tests
    // =========================================================================

    #[test]
    fn test_tree_simple() {
        let test = TreeTest {
            name: "tree_simple",
            root_label: "root",
            children: vec!["child1", "child2"],
            guides: TreeGuides::Unicode,
        };
        let output = run_test(&test);
        assert!(output.contains("root"), "Root should be present");
        assert!(output.contains("child1"), "Child1 should be present");
        assert!(output.contains("child2"), "Child2 should be present");
    }

    #[test]
    fn test_tree_with_nested_children() {
        let tree =
            Tree::with_label("root").child(TreeNode::new("parent").child(TreeNode::new("child")));

        let output: String = tree
            .render()
            .into_iter()
            .map(|s| s.text.into_owned())
            .collect();

        assert!(output.contains("root"));
        assert!(output.contains("parent"));
        assert!(output.contains("child"));
    }

    // =========================================================================
    // Markup Parsing Tests - TreeNode Labels
    // =========================================================================

    #[test]
    fn test_tree_node_does_not_parse_markup() {
        // TreeNode::new() takes impl Into<Text>, which converts strings via Text::new().
        // Text::new() does NOT parse markup - this is by design (consistent with Cell/Panel).
        // Therefore, markup tags appear literally in the output.
        let tree = Tree::new(TreeNode::new("[bold]Styled Root[/]"))
            .child(TreeNode::new("[italic]Styled Child[/]"));

        let output: String = tree
            .render()
            .into_iter()
            .map(|s| s.text.into_owned())
            .collect();

        // The raw markup tags should be present (NOT parsed)
        assert!(
            output.contains("[bold]"),
            "Raw [bold] tag should appear literally (not parsed)"
        );
        assert!(
            output.contains("[/]"),
            "Raw [/] tag should appear literally (not parsed)"
        );
        assert!(
            output.contains("[italic]"),
            "Raw [italic] tag should appear literally"
        );
        assert!(
            output.contains("Styled Root"),
            "Root label text should be present"
        );
        assert!(
            output.contains("Styled Child"),
            "Child label text should be present"
        );
    }

    #[test]
    fn test_tree_with_label_does_not_parse_markup() {
        // Tree::with_label() internally uses TreeNode::new(), so same behavior
        let tree = Tree::with_label("[red]Root[/]").child(TreeNode::new("child"));

        let output: String = tree
            .render()
            .into_iter()
            .map(|s| s.text.into_owned())
            .collect();

        assert!(
            output.contains("[red]"),
            "Raw [red] tag should appear literally"
        );
        assert!(
            output.contains("[/]"),
            "Raw [/] tag should appear literally"
        );
    }

    #[test]
    fn test_tree_node_with_prestyled_text_preserves_styles() {
        // To get styled labels, create a Text object with explicit styles.
        // This is the intended pattern for styled Tree labels.
        let mut styled_label = Text::new("Styled Root");
        styled_label.stylize(0, 6, Style::new().bold()); // "Styled" in bold

        let tree = Tree::new(TreeNode::new(styled_label));
        let segments: Vec<Segment<'_>> = tree.render();

        // Find the label segment
        let label_segment = segments
            .iter()
            .find(|seg| seg.text.contains("Styled"))
            .expect("Label segment should exist");

        // The style should be preserved
        let style = label_segment.style.as_ref().expect("Should have style");
        assert!(
            style.attributes.contains(Attributes::BOLD),
            "Label should have bold attribute from explicit styling"
        );

        // Should NOT contain raw markup tags
        let output: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(
            !output.contains("[bold]"),
            "Should not contain raw [bold] tag"
        );
    }

    // =========================================================================
    // Guide Style Tests
    // =========================================================================

    #[test]
    fn test_tree_guide_style_applied() {
        let tree = Tree::with_label("root")
            .guide_style(Style::new().color(Color::parse("green").unwrap()))
            .child(TreeNode::new("child"));

        let segments = tree.render();

        // Check that guide segments have green color style
        let has_colored_guide = segments.iter().any(|seg| {
            // Guide characters are ├── └── │ etc.
            let is_guide = seg.text.contains('─')
                || seg.text.contains('│')
                || seg.text.contains('├')
                || seg.text.contains('└');
            if is_guide {
                if let Some(ref style) = seg.style {
                    style.color.is_some()
                } else {
                    false
                }
            } else {
                false
            }
        });

        assert!(
            has_colored_guide,
            "Guide segments should have color style applied"
        );
    }

    #[test]
    fn test_tree_ascii_guides() {
        let tree = Tree::with_label("root")
            .guides(TreeGuides::Ascii)
            .child(TreeNode::new("child1"))
            .child(TreeNode::new("child2"));

        let output: String = tree
            .render()
            .into_iter()
            .map(|s| s.text.into_owned())
            .collect();

        // ASCII guides use +-- and `--
        assert!(
            output.contains("+--") || output.contains("`--"),
            "Should have ASCII guide characters"
        );
    }

    #[test]
    fn test_tree_unicode_guides() {
        let tree = Tree::with_label("root")
            .guides(TreeGuides::Unicode)
            .child(TreeNode::new("child1"))
            .child(TreeNode::new("child2"));

        let output: String = tree
            .render()
            .into_iter()
            .map(|s| s.text.into_owned())
            .collect();

        // Unicode guides use ├── and └──
        assert!(
            output.contains("├──") || output.contains("└──"),
            "Should have Unicode guide characters"
        );
    }

    // =========================================================================
    // Highlight Style Tests
    // =========================================================================

    #[test]
    fn test_tree_highlight_style_applied() {
        let tree = Tree::with_label("root")
            .highlight_style(Style::new().bold())
            .child(TreeNode::new("child"));

        let segments = tree.render();

        // All non-control segments should have bold style from highlight
        let has_bold = segments.iter().any(|seg| {
            !seg.is_control()
                && seg
                    .style
                    .as_ref()
                    .is_some_and(|s| s.attributes.contains(Attributes::BOLD))
        });

        assert!(has_bold, "Should have bold style from highlight_style");
    }

    #[test]
    fn test_tree_highlight_combines_with_label_style() {
        // Create a label with italic style
        let mut styled_label = Text::new("Root");
        styled_label.stylize(0, 4, Style::new().italic());

        // Add bold highlight style
        let tree = Tree::new(TreeNode::new(styled_label)).highlight_style(Style::new().bold());

        let segments = tree.render();

        // Find the root label segment
        let root_segment = segments
            .iter()
            .find(|seg| seg.text.contains("Root"))
            .expect("Root segment should exist");

        let style = root_segment.style.as_ref().expect("Should have style");

        // Should have both italic (from label) and bold (from highlight)
        assert!(
            style.attributes.contains(Attributes::ITALIC),
            "Should preserve italic from label"
        );
        assert!(
            style.attributes.contains(Attributes::BOLD),
            "Should have bold from highlight"
        );
    }

    // =========================================================================
    // Icon Tests
    // =========================================================================

    #[test]
    fn test_tree_node_with_icon() {
        let tree =
            Tree::new(TreeNode::with_icon("📁", "folder")).child(TreeNode::with_icon("📄", "file"));

        let output: String = tree
            .render()
            .into_iter()
            .map(|s| s.text.into_owned())
            .collect();

        assert!(output.contains("📁"), "Should have folder icon");
        assert!(output.contains("📄"), "Should have file icon");
        assert!(output.contains("folder"), "Should have folder label");
        assert!(output.contains("file"), "Should have file label");
    }

    #[test]
    fn test_tree_icon_style_applied() {
        let tree = Tree::new(
            TreeNode::with_icon("*", "root")
                .icon_style(Style::new().color(Color::parse("yellow").unwrap())),
        );

        let segments = tree.render();

        // Find the icon segment
        let icon_segment = segments
            .iter()
            .find(|seg| seg.text.contains('*'))
            .expect("Icon segment should exist");

        let style = icon_segment.style.as_ref().expect("Icon should have style");
        assert!(style.color.is_some(), "Icon should have color style");
    }

    // =========================================================================
    // ANSI Code Verification Tests
    // =========================================================================

    #[test]
    fn test_tree_with_styled_label_has_ansi_codes() {
        let mut styled_label = Text::new("Label");
        styled_label.stylize(0, 5, Style::new().bold());

        let tree = Tree::new(TreeNode::new(styled_label));
        let segments = tree.render();

        // Check that at least one segment has bold attribute
        let has_bold = segments.iter().any(|seg| {
            seg.style
                .as_ref()
                .is_some_and(|s| s.attributes.contains(Attributes::BOLD))
        });

        assert!(has_bold, "Should have bold style in segments");
    }

    #[test]
    fn test_tree_with_colored_guides_has_color_in_segments() {
        let tree = Tree::with_label("root")
            .guide_style(Style::new().color(Color::parse("blue").unwrap()))
            .child(TreeNode::new("child"));

        let segments = tree.render();

        let has_color = segments
            .iter()
            .any(|seg| seg.style.as_ref().is_some_and(|s| s.color.is_some()));

        assert!(has_color, "Should have color in guide segments");
    }

    // =========================================================================
    // Raw Markup Absence Tests
    // =========================================================================

    #[test]
    fn test_tree_parsed_labels_have_no_raw_markup() {
        // When using pre-styled Text, no raw markup should appear
        let mut text = Text::new("Styled Label");
        text.stylize(0, 6, Style::new().bold());

        let tree = Tree::new(TreeNode::new(text));
        let output: String = tree
            .render()
            .into_iter()
            .map(|s| s.text.into_owned())
            .collect();

        // Verify common markup patterns are absent
        let markup_patterns = ["[bold]", "[/bold]", "[italic]", "[/italic]", "[red]", "[/]"];

        for pattern in markup_patterns {
            assert!(
                !output.contains(pattern),
                "Pre-styled content should not contain raw markup: {}",
                pattern
            );
        }
    }

    // =========================================================================
    // Nested/Complex Style Tests
    // =========================================================================

    #[test]
    fn test_tree_nested_styles() {
        // Create text with overlapping styles
        let mut root_text = Text::new("Bold Root");
        root_text.stylize(0, 4, Style::new().bold()); // "Bold" in bold

        let mut child_text = Text::new("Italic Child");
        child_text.stylize(0, 6, Style::new().italic()); // "Italic" in italic

        let tree = Tree::new(TreeNode::new(root_text)).child(TreeNode::new(child_text));

        let segments = tree.render();

        // Check for bold style on root
        let has_bold = segments.iter().any(|seg| {
            seg.text.contains("Bold")
                && seg
                    .style
                    .as_ref()
                    .is_some_and(|s| s.attributes.contains(Attributes::BOLD))
        });

        // Check for italic style on child
        let has_italic = segments.iter().any(|seg| {
            seg.text.contains("Italic")
                && seg
                    .style
                    .as_ref()
                    .is_some_and(|s| s.attributes.contains(Attributes::ITALIC))
        });

        assert!(has_bold, "Should have bold style on 'Bold'");
        assert!(has_italic, "Should have italic style on 'Italic'");
    }

    // =========================================================================
    // Edge Cases
    // =========================================================================

    #[test]
    fn test_tree_collapsed_node_indicator() {
        let tree = Tree::with_label("root").child(
            TreeNode::new("collapsed")
                .collapsed()
                .child(TreeNode::new("hidden")),
        );

        let output: String = tree
            .render()
            .into_iter()
            .map(|s| s.text.into_owned())
            .collect();

        assert!(
            output.contains("[...]"),
            "Collapsed indicator should appear"
        );
        assert!(!output.contains("hidden"), "Hidden child should not appear");
    }

    #[test]
    fn test_tree_max_depth() {
        let tree = Tree::with_label("root")
            .max_depth(1)
            .child(TreeNode::new("visible").child(TreeNode::new("hidden")));

        let output: String = tree
            .render()
            .into_iter()
            .map(|s| s.text.into_owned())
            .collect();

        assert!(output.contains("root"), "Root should be visible");
        assert!(output.contains("visible"), "Level 1 should be visible");
        assert!(
            !output.contains("hidden"),
            "Level 2 should be hidden by max_depth"
        );
    }

    #[test]
    fn test_tree_hide_root() {
        let tree = Tree::with_label("hidden_root")
            .hide_root()
            .child(TreeNode::new("visible_child"));

        let output: String = tree
            .render()
            .into_iter()
            .map(|s| s.text.into_owned())
            .collect();

        assert!(!output.contains("hidden_root"), "Root should be hidden");
        assert!(output.contains("visible_child"), "Child should be visible");
    }

    // =========================================================================
    // Conformance Test Runner
    // =========================================================================

    #[test]
    fn test_all_standard_tree_tests() {
        for test in standard_tree_tests() {
            let output = run_test(test.as_ref());
            assert!(
                !output.is_empty(),
                "Test '{}' produced empty output",
                test.name()
            );
        }
    }
}
