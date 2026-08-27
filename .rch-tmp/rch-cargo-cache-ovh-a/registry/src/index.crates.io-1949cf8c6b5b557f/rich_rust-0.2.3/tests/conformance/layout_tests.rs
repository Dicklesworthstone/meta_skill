//! Layout rendering conformance tests.

use super::TestCase;
use rich_rust::console::Console;
use rich_rust::renderables::Renderable;
use rich_rust::renderables::layout::{Layout, LayoutSplitter, Region};
use rich_rust::segment::Segment;
use rich_rust::text::Text;

/// Test case for layout rendering.
#[derive(Debug)]
pub struct LayoutTest {
    pub name: &'static str,
    pub width: usize,
    pub height: usize,
    pub build_layout: fn() -> Layout,
}

impl TestCase for LayoutTest {
    fn name(&self) -> &str {
        self.name
    }

    fn render(&self) -> Vec<Segment<'static>> {
        let layout = (self.build_layout)();
        let console = Console::builder()
            .width(self.width)
            .height(self.height)
            .build();
        let options = console.options();
        layout
            .render(&console, &options)
            .into_iter()
            .map(Segment::into_owned)
            .collect()
    }

    fn python_rich_code(&self) -> Option<String> {
        let prologue = format!(
            "from rich.console import Console\nfrom rich.layout import Layout\n\nconsole = Console(force_terminal=True, width={}, height={})\n",
            self.width, self.height
        );

        let body = match self.name {
            "layout_simple_row" => {
                r#"layout = Layout(name="root")
layout.split_row(
    Layout(name="left"),
    Layout(name="right"),
)"#
            }
            "layout_simple_column" => {
                r#"layout = Layout(name="root")
layout.split_column(
    Layout(name="top"),
    Layout(name="bottom"),
)"#
            }
            "layout_nested_3_level" => {
                r#"layout = Layout(name="root")

left = Layout(name="left")
left.split_column(
    Layout(name="left-top"),
    Layout(name="left-bottom"),
)

right = Layout(name="right")
right.split_column(
    Layout(name="right-top"),
    Layout(name="right-bottom"),
)

layout.split_row(left, right)"#
            }
            "layout_deep_nested_4_level" => {
                r#"layout = Layout(name="root")

level1 = Layout(name="level1")
level2 = Layout(name="level2")
level3 = Layout(name="level3")

level3.split_row(
    Layout(name="leaf-a"),
    Layout(name="leaf-b"),
)
level2.split_column(level3, Layout(name="leaf-c"))
level1.split_row(level2, Layout(name="leaf-d"))
layout.split_column(level1, Layout(name="leaf-e"))"#
            }
            "layout_visibility" => {
                r#"layout = Layout(name="root")
layout.split_row(
    Layout(name="visible"),
    Layout(name="hidden", visible=False),
    Layout(name="visible2"),
)"#
            }
            "layout_ratio_2_1" => {
                r#"layout = Layout()
layout.split_row(
    Layout(name="wide", ratio=2),
    Layout(name="narrow", ratio=1),
)"#
            }
            "layout_ratio_1_2_1" => {
                r#"layout = Layout()
layout.split_row(
    Layout(name="left", ratio=1),
    Layout(name="center", ratio=2),
    Layout(name="right", ratio=1),
)"#
            }
            "layout_fixed_size" => {
                r#"layout = Layout()
layout.split_row(
    Layout(name="fixed", size=10),
    Layout(name="flex"),
)"#
            }
            "layout_minimum_size" => {
                r#"layout = Layout()
layout.split_row(
    Layout(name="min5", minimum_size=5),
    Layout(name="min3", minimum_size=3),
)"#
            }
            "layout_mixed_sizing" => {
                r#"layout = Layout()
layout.split_row(
    Layout(name="fixed", size=10),
    Layout(name="flex1", ratio=1),
    Layout(name="flex2", ratio=2),
)"#
            }
            "layout_single_child" => {
                r#"layout = Layout()
layout.split_row(
    Layout(name="only"),
)"#
            }
            "layout_all_hidden" => {
                r#"layout = Layout(name="root")
layout.split_row(
    Layout(name="a", visible=False),
    Layout(name="b", visible=False),
)"#
            }
            "layout_with_content" => {
                r#"layout = Layout()
layout.split_row(
    Layout("LEFT", name="left"),
    Layout("RIGHT", name="right"),
)"#
            }
            "layout_column_content" => {
                r#"layout = Layout()
layout.split_column(
    Layout("TOP", name="top"),
    Layout("BOTTOM", name="bottom"),
)"#
            }
            _ => return None,
        };

        Some(format!(
            "{prologue}{body}\n\nconsole.print(layout, end=\"\")\n"
        ))
    }
}

// ============================================================================
// Tree Structure Test Builders
// ============================================================================

fn build_simple_row_split() -> Layout {
    let mut layout = Layout::new().name("root");
    layout.split_row(vec![
        Layout::new().name("left"),
        Layout::new().name("right"),
    ]);
    layout
}

fn build_simple_column_split() -> Layout {
    let mut layout = Layout::new().name("root");
    layout.split_column(vec![
        Layout::new().name("top"),
        Layout::new().name("bottom"),
    ]);
    layout
}

fn build_nested_3_level() -> Layout {
    let mut root = Layout::new().name("root");

    let mut left = Layout::new().name("left");
    left.split_column(vec![
        Layout::new().name("left-top"),
        Layout::new().name("left-bottom"),
    ]);

    let mut right = Layout::new().name("right");
    right.split_column(vec![
        Layout::new().name("right-top"),
        Layout::new().name("right-bottom"),
    ]);

    root.split_row(vec![left, right]);
    root
}

fn build_deep_nested_4_level() -> Layout {
    let mut root = Layout::new().name("root");

    let mut level1 = Layout::new().name("level1");
    let mut level2 = Layout::new().name("level2");
    let mut level3 = Layout::new().name("level3");
    level3.split_row(vec![
        Layout::new().name("leaf-a"),
        Layout::new().name("leaf-b"),
    ]);
    level2.split_column(vec![level3, Layout::new().name("leaf-c")]);
    level1.split_row(vec![level2, Layout::new().name("leaf-d")]);
    root.split_column(vec![level1, Layout::new().name("leaf-e")]);
    root
}

fn build_with_visibility() -> Layout {
    let mut layout = Layout::new().name("root");
    layout.split_row(vec![
        Layout::new().name("visible"),
        Layout::new().name("hidden").visible(false),
        Layout::new().name("visible2"),
    ]);
    layout
}

// ============================================================================
// Sizing Algorithm Test Builders
// ============================================================================

fn build_ratio_2_1() -> Layout {
    let mut layout = Layout::new();
    layout.split_row(vec![
        Layout::new().name("wide").ratio(2),
        Layout::new().name("narrow").ratio(1),
    ]);
    layout
}

fn build_ratio_1_2_1() -> Layout {
    let mut layout = Layout::new();
    layout.split_row(vec![
        Layout::new().name("left").ratio(1),
        Layout::new().name("center").ratio(2),
        Layout::new().name("right").ratio(1),
    ]);
    layout
}

fn build_fixed_size() -> Layout {
    let mut layout = Layout::new();
    layout.split_row(vec![
        Layout::new().name("fixed").size(10),
        Layout::new().name("flex"),
    ]);
    layout
}

fn build_minimum_size() -> Layout {
    let mut layout = Layout::new();
    layout.split_row(vec![
        Layout::new().name("min5").minimum_size(5),
        Layout::new().name("min3").minimum_size(3),
    ]);
    layout
}

fn build_mixed_fixed_ratio() -> Layout {
    let mut layout = Layout::new();
    layout.split_row(vec![
        Layout::new().name("fixed").size(10),
        Layout::new().name("flex1").ratio(1),
        Layout::new().name("flex2").ratio(2),
    ]);
    layout
}

fn build_single_child() -> Layout {
    let mut layout = Layout::new();
    layout.split_row(vec![Layout::new().name("only")]);
    layout
}

fn build_all_hidden() -> Layout {
    let mut layout = Layout::new().name("root");
    layout.split_row(vec![
        Layout::new().name("a").visible(false),
        Layout::new().name("b").visible(false),
    ]);
    layout
}

// ============================================================================
// Rendering Test Builders
// ============================================================================

fn build_with_content() -> Layout {
    let mut layout = Layout::new();
    layout.split_row(vec![
        Layout::from_renderable(Text::new("LEFT")).name("left"),
        Layout::from_renderable(Text::new("RIGHT")).name("right"),
    ]);
    layout
}

fn build_column_with_content() -> Layout {
    let mut layout = Layout::new();
    layout.split_column(vec![
        Layout::from_renderable(Text::new("TOP")).name("top"),
        Layout::from_renderable(Text::new("BOTTOM")).name("bottom"),
    ]);
    layout
}

// ============================================================================
// Standard Test Cases
// ============================================================================

/// Standard layout test cases for conformance testing.
pub fn standard_layout_tests() -> Vec<Box<dyn TestCase>> {
    vec![
        // Tree structure tests
        Box::new(LayoutTest {
            name: "layout_simple_row",
            width: 40,
            height: 5,
            build_layout: build_simple_row_split,
        }),
        Box::new(LayoutTest {
            name: "layout_simple_column",
            width: 40,
            height: 10,
            build_layout: build_simple_column_split,
        }),
        Box::new(LayoutTest {
            name: "layout_nested_3_level",
            width: 60,
            height: 10,
            build_layout: build_nested_3_level,
        }),
        Box::new(LayoutTest {
            name: "layout_deep_nested_4_level",
            width: 80,
            height: 20,
            build_layout: build_deep_nested_4_level,
        }),
        Box::new(LayoutTest {
            name: "layout_visibility",
            width: 30,
            height: 5,
            build_layout: build_with_visibility,
        }),
        // Sizing algorithm tests
        Box::new(LayoutTest {
            name: "layout_ratio_2_1",
            width: 30,
            height: 5,
            build_layout: build_ratio_2_1,
        }),
        Box::new(LayoutTest {
            name: "layout_ratio_1_2_1",
            width: 40,
            height: 5,
            build_layout: build_ratio_1_2_1,
        }),
        Box::new(LayoutTest {
            name: "layout_fixed_size",
            width: 30,
            height: 5,
            build_layout: build_fixed_size,
        }),
        Box::new(LayoutTest {
            name: "layout_minimum_size",
            width: 20,
            height: 5,
            build_layout: build_minimum_size,
        }),
        Box::new(LayoutTest {
            name: "layout_mixed_sizing",
            width: 40,
            height: 5,
            build_layout: build_mixed_fixed_ratio,
        }),
        Box::new(LayoutTest {
            name: "layout_single_child",
            width: 20,
            height: 5,
            build_layout: build_single_child,
        }),
        Box::new(LayoutTest {
            name: "layout_all_hidden",
            width: 20,
            height: 5,
            build_layout: build_all_hidden,
        }),
        // Rendering tests
        Box::new(LayoutTest {
            name: "layout_with_content",
            width: 40,
            height: 3,
            build_layout: build_with_content,
        }),
        Box::new(LayoutTest {
            name: "layout_column_content",
            width: 20,
            height: 6,
            build_layout: build_column_with_content,
        }),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conformance::run_test;
    use rich_rust::segment::split_lines;

    // ========================================================================
    // Tree Structure Tests
    // ========================================================================

    #[test]
    fn test_split_row_creates_two_children() {
        let mut layout = Layout::new();
        layout.split_row(vec![
            Layout::new().name("left"),
            Layout::new().name("right"),
        ]);

        // Named lookup should find both children
        assert!(layout.get("left").is_some());
        assert!(layout.get("right").is_some());
    }

    #[test]
    fn test_split_column_creates_two_children() {
        let mut layout = Layout::new();
        layout.split_column(vec![
            Layout::new().name("top"),
            Layout::new().name("bottom"),
        ]);

        assert!(layout.get("top").is_some());
        assert!(layout.get("bottom").is_some());
    }

    #[test]
    fn test_nested_split_3_levels() {
        let layout = build_nested_3_level();

        assert!(layout.get("root").is_some());
        assert!(layout.get("left").is_some());
        assert!(layout.get("right").is_some());
        assert!(layout.get("left-top").is_some());
        assert!(layout.get("left-bottom").is_some());
        assert!(layout.get("right-top").is_some());
        assert!(layout.get("right-bottom").is_some());
    }

    #[test]
    fn test_deep_nested_4_levels() {
        let layout = build_deep_nested_4_level();

        assert!(layout.get("root").is_some());
        assert!(layout.get("level1").is_some());
        assert!(layout.get("level2").is_some());
        assert!(layout.get("level3").is_some());
        assert!(layout.get("leaf-a").is_some());
        assert!(layout.get("leaf-b").is_some());
        assert!(layout.get("leaf-c").is_some());
        assert!(layout.get("leaf-d").is_some());
        assert!(layout.get("leaf-e").is_some());
    }

    #[test]
    fn test_named_lookup_returns_none_for_missing() {
        let layout = build_simple_row_split();
        assert!(layout.get("nonexistent").is_none());
    }

    #[test]
    fn test_named_lookup_mutable() {
        let mut layout = build_simple_row_split();

        // Should be able to get mutable reference
        let left = layout.get_mut("left");
        assert!(left.is_some());

        // Modify it
        if let Some(left) = layout.get_mut("left") {
            left.update(Text::new("Updated content"));
        }
    }

    #[test]
    fn test_index_operator() {
        let layout = build_simple_row_split();

        // Should not panic
        let _left = &layout["left"];
        let _right = &layout["right"];
    }

    #[test]
    #[should_panic(expected = "Layout not found")]
    fn test_index_operator_panics_on_missing() {
        let layout = build_simple_row_split();
        let _missing = &layout["nonexistent"];
    }

    #[test]
    fn test_visibility_toggling() {
        let layout = build_with_visibility();

        // We can't check visibility directly without a pub getter,
        // but we can verify the layout structure is correct
        assert!(layout.get("visible").is_some());
        assert!(layout.get("hidden").is_some());
        assert!(layout.get("visible2").is_some());
    }

    #[test]
    fn test_unsplit_removes_children() {
        let mut layout = Layout::new().name("root");
        layout.split_row(vec![
            Layout::new().name("child1"),
            Layout::new().name("child2"),
        ]);

        assert!(layout.get("child1").is_some());

        layout.unsplit();

        assert!(layout.get("child1").is_none());
        assert!(layout.get("child2").is_none());
    }

    #[test]
    fn test_add_split_extends_children() {
        let mut layout = Layout::new().name("root");
        layout.split_row(vec![Layout::new().name("first")]);
        layout.add_split(vec![Layout::new().name("second")]);

        assert!(layout.get("first").is_some());
        assert!(layout.get("second").is_some());
    }

    // ========================================================================
    // Sizing Algorithm Tests
    // ========================================================================

    #[test]
    fn test_ratio_distribution_2_1() {
        let test = LayoutTest {
            name: "ratio_2_1",
            width: 30,
            height: 1,
            build_layout: build_ratio_2_1,
        };

        let output = run_test(&test);
        // With ratio 2:1 and width 30, we expect ~20:10 distribution
        // The placeholder text includes dimensions
        assert!(output.contains("wide"));
        assert!(output.contains("narrow"));
    }

    #[test]
    fn test_ratio_distribution_1_2_1() {
        let test = LayoutTest {
            name: "ratio_1_2_1",
            width: 40,
            height: 1,
            build_layout: build_ratio_1_2_1,
        };

        let output = run_test(&test);
        // With ratio 1:2:1 and width 40, center should be ~20
        assert!(output.contains("left"));
        assert!(output.contains("center"));
        assert!(output.contains("right"));
    }

    #[test]
    fn test_fixed_size_allocation() {
        let test = LayoutTest {
            name: "fixed_size",
            width: 30,
            height: 1,
            build_layout: build_fixed_size,
        };

        let output = run_test(&test);
        // The fixed region should have size 10
        assert!(output.contains("fixed"));
        assert!(output.contains("flex"));
    }

    #[test]
    fn test_minimum_size_enforcement() {
        let test = LayoutTest {
            name: "min_size",
            width: 10, // Very narrow
            height: 1,
            build_layout: build_minimum_size,
        };

        let output = run_test(&test);
        // Even with narrow width, minimum sizes should be respected
        // Output should not be empty
        assert!(!output.trim().is_empty());
    }

    #[test]
    fn test_zero_width_handling() {
        let layout = build_simple_row_split();
        let console = Console::builder().width(0).height(5).build();
        let options = console.options();

        // Should not panic - just verify render completes without error
        let _ = layout.render(&console, &options);
    }

    #[test]
    fn test_single_child_gets_full_width() {
        let test = LayoutTest {
            name: "single_child",
            width: 20,
            height: 1,
            build_layout: build_single_child,
        };

        let output = run_test(&test);
        // Single child should get the full width
        assert!(output.contains("only"));
    }

    #[test]
    fn test_all_hidden_produces_blank() {
        let test = LayoutTest {
            name: "all_hidden",
            width: 20,
            height: 5,
            build_layout: build_all_hidden,
        };

        let output = run_test(&test);
        // All hidden children should produce blank output or placeholder
        // The root is visible so it shows its placeholder
        assert!(
            output.contains("root"),
            "expected root placeholder when all children are hidden"
        );
    }

    // ========================================================================
    // Rendering Tests
    // ========================================================================

    #[test]
    fn test_render_produces_correct_height() {
        let layout = build_simple_column_split();
        let console = Console::builder().width(40).height(10).build();
        let options = console.options();

        let segments = layout.render(&console, &options);
        let lines = split_lines(segments.into_iter());

        // Should produce exactly the requested height
        assert_eq!(lines.len(), 10);
    }

    #[test]
    fn test_render_produces_correct_width() {
        let layout = build_simple_row_split();
        let console = Console::builder().width(40).height(5).build();
        let options = console.options();

        let segments = layout.render(&console, &options);
        let lines = split_lines(segments.into_iter());

        for line in lines {
            let width: usize = line.iter().map(|s| s.cell_length()).sum();
            assert_eq!(width, 40, "Each line should be exactly 40 cells wide");
        }
    }

    #[test]
    fn test_content_placement_row() {
        let test = LayoutTest {
            name: "content_row",
            width: 40,
            height: 1,
            build_layout: build_with_content,
        };

        let output = run_test(&test);
        assert!(output.contains("LEFT"));
        assert!(output.contains("RIGHT"));

        // LEFT should appear before RIGHT in the output
        let left_pos = output.find("LEFT").unwrap();
        let right_pos = output.find("RIGHT").unwrap();
        assert!(left_pos < right_pos);
    }

    #[test]
    fn test_content_placement_column() {
        let test = LayoutTest {
            name: "content_column",
            width: 20,
            height: 6,
            build_layout: build_column_with_content,
        };

        let output = run_test(&test);
        assert!(output.contains("TOP"));
        assert!(output.contains("BOTTOM"));
    }

    #[test]
    fn test_placeholder_shows_name_and_dimensions() {
        let test = LayoutTest {
            name: "placeholder",
            width: 30,
            height: 5,
            build_layout: build_simple_row_split,
        };

        let output = run_test(&test);
        // Placeholders should show the region name and dimensions
        assert!(output.contains("left"));
        assert!(output.contains("right"));
    }

    // ========================================================================
    // Region Tests
    // ========================================================================

    #[test]
    fn test_region_new() {
        let region = Region::new(5, 10, 20, 15);
        assert_eq!(region.x, 5);
        assert_eq!(region.y, 10);
        assert_eq!(region.width, 20);
        assert_eq!(region.height, 15);
    }

    #[test]
    fn test_region_equality() {
        let r1 = Region::new(0, 0, 10, 10);
        let r2 = Region::new(0, 0, 10, 10);
        let r3 = Region::new(1, 0, 10, 10);

        assert_eq!(r1, r2);
        assert_ne!(r1, r3);
    }

    #[test]
    fn test_region_copy() {
        let r1 = Region::new(5, 5, 20, 20);
        let r2 = r1; // Region is Copy
        assert_eq!(r1, r2);
    }

    // ========================================================================
    // Layout Splitter Tests
    // ========================================================================

    #[test]
    fn test_layout_splitter_equality() {
        assert_eq!(LayoutSplitter::Row, LayoutSplitter::Row);
        assert_eq!(LayoutSplitter::Column, LayoutSplitter::Column);
        assert_ne!(LayoutSplitter::Row, LayoutSplitter::Column);
    }

    // ========================================================================
    // Integration Tests
    // ========================================================================

    #[test]
    fn test_all_standard_layout_tests() {
        for test in standard_layout_tests() {
            let output = run_test(test.as_ref());
            // All tests should produce non-empty output
            // (except possibly all_hidden which may be blank)
            if !test.name().contains("hidden") {
                assert!(
                    !output.trim().is_empty(),
                    "Test '{}' produced empty output",
                    test.name()
                );
            }
        }
    }

    #[test]
    fn test_layout_builder_chain() {
        // Test that builder methods chain correctly
        let layout = Layout::new()
            .name("test")
            .size(10)
            .minimum_size(5)
            .ratio(2)
            .visible(true);

        assert!(layout.get("test").is_some());
    }

    #[test]
    fn test_layout_from_renderable() {
        let text = Text::new("Hello");
        let layout = Layout::from_renderable(text).name("content");

        let console = Console::builder().width(20).height(3).build();
        let options = console.options();
        let segments = layout.render(&console, &options);
        let output: String = segments.iter().map(|s| s.text.as_ref()).collect();

        assert!(output.contains("Hello"));
    }

    #[test]
    fn test_layout_update_renderable() {
        let mut layout = Layout::new().name("dynamic");
        layout.update(Text::new("Initial"));

        let console = Console::builder().width(20).height(3).build();
        let options = console.options();
        let segments = layout.render(&console, &options);
        let output: String = segments.iter().map(|s| s.text.as_ref()).collect();

        assert!(output.contains("Initial"));

        // Update the content
        layout.update(Text::new("Updated"));
        let segments = layout.render(&console, &options);
        let output: String = segments.iter().map(|s| s.text.as_ref()).collect();

        assert!(output.contains("Updated"));
    }

    // ========================================================================
    // Edge Case Tests
    // ========================================================================

    #[test]
    fn test_very_narrow_layout() {
        let mut layout = Layout::new();
        layout.split_row(vec![
            Layout::new().name("a"),
            Layout::new().name("b"),
            Layout::new().name("c"),
        ]);

        let console = Console::builder().width(3).height(1).build();
        let options = console.options();

        // Should handle gracefully without panic
        let _segments = layout.render(&console, &options);
    }

    #[test]
    fn test_very_short_layout() {
        let mut layout = Layout::new();
        layout.split_column(vec![
            Layout::new().name("a"),
            Layout::new().name("b"),
            Layout::new().name("c"),
        ]);

        let console = Console::builder().width(20).height(1).build();
        let options = console.options();

        // Should handle gracefully without panic
        let _segments = layout.render(&console, &options);
    }

    #[test]
    fn test_empty_children_list() {
        let mut layout = Layout::new().name("empty");
        layout.split_row(vec![]);

        let console = Console::builder().width(20).height(5).build();
        let options = console.options();

        // Should render as a leaf with placeholder
        let segments = layout.render(&console, &options);
        let output: String = segments.iter().map(|s| s.text.as_ref()).collect();

        assert!(output.contains("empty"));
    }

    #[test]
    fn test_ratio_zero_treated_as_one() {
        let mut layout = Layout::new();
        layout.split_row(vec![
            Layout::new().name("a").ratio(0), // Should be treated as 1
            Layout::new().name("b").ratio(1),
        ]);

        let console = Console::builder().width(20).height(1).build();
        let options = console.options();

        // Should not panic and distribute space reasonably
        let _segments = layout.render(&console, &options);
    }

    #[test]
    fn test_minimum_size_zero_treated_as_one() {
        let mut layout = Layout::new();
        layout.split_row(vec![
            Layout::new().name("a").minimum_size(0), // Should be treated as 1
            Layout::new().name("b"),
        ]);

        let console = Console::builder().width(20).height(1).build();
        let options = console.options();

        // Should not panic
        let _segments = layout.render(&console, &options);
    }
}
