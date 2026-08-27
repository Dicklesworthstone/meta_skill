//! Layout - split the terminal into rows/columns with nested regions.

use std::ops::{Index, IndexMut};

use num_rational::Ratio;

use crate::console::{Console, ConsoleOptions};
use crate::measure::{Measurement, RichMeasure};
use crate::renderables::Renderable;
use crate::segment::{Segment, adjust_line_length, split_lines};
use crate::text::{JustifyMethod, OverflowMethod, Text};

/// Rectangular region of the screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl Region {
    #[must_use]
    pub const fn new(x: usize, y: usize, width: usize, height: usize) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// Layout splitter direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayoutSplitter {
    Row,
    Column,
}

impl LayoutSplitter {
    fn divide<'a>(self, children: &'a [&Layout], region: Region) -> Vec<(Region, &'a Layout)> {
        match self {
            Self::Row => divide_row(children, region),
            Self::Column => divide_column(children, region),
        }
    }
}

/// Layout node.
pub struct Layout {
    renderable: Option<Box<dyn Renderable + Send + Sync>>,
    name: Option<String>,
    size: Option<usize>,
    minimum_size: usize,
    ratio: usize,
    visible: bool,
    splitter: LayoutSplitter,
    children: Vec<Layout>,
}

impl Default for Layout {
    fn default() -> Self {
        Self {
            renderable: None,
            name: None,
            size: None,
            minimum_size: 1,
            ratio: 1,
            visible: true,
            splitter: LayoutSplitter::Column,
            children: Vec::new(),
        }
    }
}

impl Layout {
    /// Create a new empty Layout.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a Layout from a renderable.
    #[must_use]
    pub fn from_renderable<R>(renderable: R) -> Self
    where
        R: Renderable + Send + Sync + 'static,
    {
        Self {
            renderable: Some(Box::new(renderable)),
            ..Self::default()
        }
    }

    /// Set the layout name.
    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set fixed size (width or height, depending on splitter).
    #[must_use]
    pub fn size(mut self, size: usize) -> Self {
        self.size = Some(size);
        self
    }

    /// Set minimum size.
    #[must_use]
    pub fn minimum_size(mut self, minimum_size: usize) -> Self {
        self.minimum_size = minimum_size.max(1);
        self
    }

    /// Set flex ratio.
    #[must_use]
    pub fn ratio(mut self, ratio: usize) -> Self {
        self.ratio = ratio.max(1);
        self
    }

    /// Set visibility.
    #[must_use]
    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    /// Set the layout renderable.
    #[must_use]
    pub fn renderable<R>(mut self, renderable: R) -> Self
    where
        R: Renderable + Send + Sync + 'static,
    {
        self.renderable = Some(Box::new(renderable));
        self
    }

    /// Update the layout content.
    pub fn update<R>(&mut self, renderable: R)
    where
        R: Renderable + Send + Sync + 'static,
    {
        self.renderable = Some(Box::new(renderable));
    }

    /// Split into child layouts with explicit splitter.
    pub fn split(&mut self, layouts: Vec<Layout>, splitter: LayoutSplitter) {
        self.splitter = splitter;
        self.children = layouts;
    }

    /// Split horizontally (row).
    pub fn split_row(&mut self, layouts: Vec<Layout>) {
        self.split(layouts, LayoutSplitter::Row);
    }

    /// Split vertically (column).
    pub fn split_column(&mut self, layouts: Vec<Layout>) {
        self.split(layouts, LayoutSplitter::Column);
    }

    /// Add children to an existing split.
    pub fn add_split(&mut self, layouts: Vec<Layout>) {
        self.children.extend(layouts);
    }

    /// Remove all children.
    pub fn unsplit(&mut self) {
        self.children.clear();
    }

    /// Get a child layout by name.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&Layout> {
        if self.name.as_deref() == Some(name) {
            return Some(self);
        }
        for child in &self.children {
            if let Some(found) = child.get(name) {
                return Some(found);
            }
        }
        None
    }

    /// Get a mutable child layout by name.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut Layout> {
        if self.name.as_deref() == Some(name) {
            return Some(self);
        }
        for child in &mut self.children {
            if let Some(found) = child.get_mut(name) {
                return Some(found);
            }
        }
        None
    }

    fn visible_children(&self) -> Vec<&Layout> {
        self.children.iter().filter(|c| c.visible).collect()
    }

    fn render_region(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        region: Region,
    ) -> Vec<Vec<Segment<'static>>> {
        if !self.visible {
            return blank_lines(region.width, region.height);
        }

        let child_layouts = self.visible_children();
        if child_layouts.is_empty() {
            return self.render_leaf(console, options, region);
        }

        let mut rendered_children = Vec::new();
        for (child_region, child) in self.splitter.divide(&child_layouts, region) {
            let child_options = options.update_dimensions(child_region.width, child_region.height);
            let child_lines = child.render_region(console, &child_options, child_region);
            rendered_children.push((child_region, child_lines));
        }

        merge_children(region, &rendered_children)
    }

    fn render_leaf(
        &self,
        console: &Console,
        options: &ConsoleOptions,
        region: Region,
    ) -> Vec<Vec<Segment<'static>>> {
        let content_segments = if let Some(renderable) = self.renderable.as_ref() {
            renderable
                .render(
                    console,
                    &options.update_dimensions(region.width, region.height),
                )
                .into_iter()
                .map(Segment::into_owned)
                .collect()
        } else {
            placeholder_segments(self.name.as_deref(), region.width, region.height)
        };

        let mut lines = split_lines(content_segments.into_iter());
        let mut padded = Vec::new();

        for mut line in lines.drain(..) {
            line = adjust_line_length(line, region.width, None, true);
            padded.push(line);
        }

        if padded.len() > region.height {
            padded.truncate(region.height);
        } else if padded.len() < region.height {
            let filler = vec![Segment::new(" ".repeat(region.width), None)];
            for _ in padded.len()..region.height {
                padded.push(filler.clone());
            }
        }

        padded
    }
}

impl Renderable for Layout {
    fn render<'a>(&'a self, console: &Console, options: &ConsoleOptions) -> Vec<Segment<'a>> {
        let width = options.max_width;
        let height = options.height.unwrap_or(options.size.height);
        let region = Region::new(0, 0, width, height);
        let lines = self.render_region(console, options, region);

        let mut segments: Vec<Segment<'static>> = Vec::new();
        for (idx, mut line) in lines.into_iter().enumerate() {
            segments.append(&mut line);
            if idx + 1 < height {
                segments.push(Segment::line());
            }
        }

        segments.into_iter().collect()
    }
}

impl RichMeasure for Layout {
    fn rich_measure(&self, _console: &Console, options: &ConsoleOptions) -> Measurement {
        Measurement::exact(options.max_width)
    }
}

impl Index<&str> for Layout {
    type Output = Layout;

    fn index(&self, name: &str) -> &Self::Output {
        self.get(name).unwrap_or_else(|| {
            panic!("Layout not found: {name}");
        })
    }
}

impl IndexMut<&str> for Layout {
    fn index_mut(&mut self, name: &str) -> &mut Self::Output {
        self.get_mut(name).unwrap_or_else(|| {
            panic!("Layout not found: {name}");
        })
    }
}

fn blank_lines(width: usize, height: usize) -> Vec<Vec<Segment<'static>>> {
    let filler = vec![Segment::new(" ".repeat(width), None)];
    (0..height).map(|_| filler.clone()).collect()
}

fn placeholder_segments(name: Option<&str>, width: usize, height: usize) -> Vec<Segment<'static>> {
    let label = if let Some(name) = name {
        format!("{name} ({width} x {height})")
    } else {
        format!("({width} x {height})")
    };
    let mut text = Text::new(label);
    text.overflow = OverflowMethod::Ellipsis;
    text.justify = JustifyMethod::Center;
    text.pad(width, JustifyMethod::Center);
    text.render("")
        .into_iter()
        .map(Segment::into_owned)
        .collect()
}

fn merge_children(
    region: Region,
    children: &[(Region, Vec<Vec<Segment<'static>>>)],
) -> Vec<Vec<Segment<'static>>> {
    let mut lines: Vec<Vec<Segment<'static>>> = Vec::with_capacity(region.height);
    for row in 0..region.height {
        let mut line = Vec::new();
        let mut cursor = 0usize;
        let abs_row = region.y + row;

        let mut children_for_row: Vec<(usize, &Vec<Segment<'static>>)> = Vec::new();
        for (child_region, child_lines) in children {
            if abs_row >= child_region.y && abs_row < child_region.y + child_region.height {
                let child_line = &child_lines[abs_row - child_region.y];
                // Store relative x offset
                children_for_row.push((child_region.x - region.x, child_line));
            }
        }

        children_for_row.sort_by_key(|(x, _)| *x);
        for (x, child_line) in children_for_row {
            if x > cursor {
                line.push(Segment::new(" ".repeat(x - cursor), None));
                cursor = x;
            }
            line.extend(child_line.clone());
            cursor =
                cursor.saturating_add(child_line.iter().map(Segment::cell_length).sum::<usize>());
        }

        if cursor < region.width {
            line.push(Segment::new(" ".repeat(region.width - cursor), None));
        }

        lines.push(adjust_line_length(line, region.width, None, true));
    }
    lines
}

fn divide_row<'a>(children: &'a [&Layout], region: Region) -> Vec<(Region, &'a Layout)> {
    let widths = ratio_resolve(region.width, children);
    let mut result = Vec::new();
    let mut offset = 0;
    for (child, width) in children.iter().zip(widths) {
        let child_region = Region::new(region.x + offset, region.y, width, region.height);
        result.push((child_region, *child));
        offset += width;
    }
    result
}

fn divide_column<'a>(children: &'a [&Layout], region: Region) -> Vec<(Region, &'a Layout)> {
    let heights = ratio_resolve(region.height, children);
    let mut result = Vec::new();
    let mut offset = 0;
    for (child, height) in children.iter().zip(heights) {
        let child_region = Region::new(region.x, region.y + offset, region.width, height);
        result.push((child_region, *child));
        offset += height;
    }
    result
}

fn ratio_resolve(total: usize, children: &[&Layout]) -> Vec<usize> {
    if children.is_empty() {
        return Vec::new();
    }

    let mut sizes = Vec::with_capacity(children.len());
    let mut mins = Vec::with_capacity(children.len());
    let mut ratios = Vec::with_capacity(children.len());

    let mut fixed_total = 0usize;
    let mut flex_min_total = 0usize;

    for child in children {
        let min_size = child.minimum_size.max(1);
        mins.push(min_size);
        if let Some(size) = child.size {
            let size = size.max(min_size);
            sizes.push(size);
            ratios.push(0);
            fixed_total += size;
        } else {
            sizes.push(min_size);
            ratios.push(child.ratio.max(1));
            flex_min_total += min_size;
        }
    }

    let mut remaining = total.saturating_sub(fixed_total + flex_min_total);
    let total_ratio: usize = ratios.iter().sum();
    if total_ratio > 0 && remaining > 0 {
        let mut distributed = 0usize;
        let mut flex_idx = 0usize;
        let flex_count = ratios.iter().filter(|&&r| r > 0).count();
        for (i, &ratio) in ratios.iter().enumerate() {
            if ratio > 0 {
                flex_idx += 1;
                let share = Ratio::new(ratio, total_ratio);
                let extra = if flex_idx == flex_count {
                    remaining - distributed
                } else {
                    (share * remaining).round().to_integer()
                };
                sizes[i] = sizes[i].saturating_add(extra);
                distributed += extra;
            }
        }
        remaining = remaining.saturating_sub(distributed);
    }

    if remaining > 0 {
        let mut i = 0usize;
        while remaining > 0 && !sizes.is_empty() {
            sizes[i] = sizes[i].saturating_add(1);
            remaining = remaining.saturating_sub(1);
            i = (i + 1) % sizes.len();
        }
    }

    clamp_sizes(sizes, &mins, total)
}

fn clamp_sizes(mut sizes: Vec<usize>, mins: &[usize], total: usize) -> Vec<usize> {
    let mut sum: usize = sizes.iter().sum();
    if sum <= total {
        return sizes;
    }

    while sum > total {
        let mut reduced = false;
        for idx in (0..sizes.len()).rev() {
            if sizes[idx] > mins[idx] {
                sizes[idx] -= 1;
                sum -= 1;
                reduced = true;
                if sum == total {
                    break;
                }
            }
        }
        if !reduced {
            break;
        }
    }

    if sum > total {
        let mut idx = 0usize;
        while sum > total {
            if sizes[idx] > 0 {
                sizes[idx] -= 1;
                sum -= 1;
            }
            idx = (idx + 1) % sizes.len();
        }
    }

    sizes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::renderables::Panel;

    // =========================================================================
    // Region Tests
    // =========================================================================

    #[test]
    fn test_region_new() {
        let region = Region::new(5, 10, 80, 24);
        assert_eq!(region.x, 5);
        assert_eq!(region.y, 10);
        assert_eq!(region.width, 80);
        assert_eq!(region.height, 24);
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
    fn test_region_clone() {
        let r1 = Region::new(1, 2, 3, 4);
        let r2 = r1; // Region is Copy
        assert_eq!(r1, r2);
    }

    // =========================================================================
    // LayoutSplitter Tests
    // =========================================================================

    #[test]
    fn test_layout_splitter_variants() {
        let row = LayoutSplitter::Row;
        let col = LayoutSplitter::Column;

        assert_ne!(row, col);
        assert_eq!(row, LayoutSplitter::Row);
        assert_eq!(col, LayoutSplitter::Column);
    }

    // =========================================================================
    // Layout Creation Tests
    // =========================================================================

    #[test]
    fn test_layout_new() {
        let layout = Layout::new();
        assert!(layout.renderable.is_none());
        assert!(layout.name.is_none());
        assert!(layout.size.is_none());
        assert_eq!(layout.minimum_size, 1);
        assert_eq!(layout.ratio, 1);
        assert!(layout.visible);
        assert!(layout.children.is_empty());
    }

    #[test]
    fn test_layout_default() {
        let layout = Layout::default();
        assert!(layout.renderable.is_none());
        assert_eq!(layout.splitter, LayoutSplitter::Column);
    }

    #[test]
    fn test_layout_from_renderable() {
        let text = Text::new("Hello");
        let layout = Layout::from_renderable(text);
        assert!(layout.renderable.is_some());
    }

    #[test]
    fn test_layout_builder_chain() {
        let layout = Layout::new()
            .name("main")
            .size(50)
            .minimum_size(10)
            .ratio(2)
            .visible(true);

        assert_eq!(layout.name, Some("main".to_string()));
        assert_eq!(layout.size, Some(50));
        assert_eq!(layout.minimum_size, 10);
        assert_eq!(layout.ratio, 2);
        assert!(layout.visible);
    }

    #[test]
    fn test_layout_minimum_size_clamped() {
        // minimum_size should be at least 1
        let layout = Layout::new().minimum_size(0);
        assert_eq!(layout.minimum_size, 1);
    }

    #[test]
    fn test_layout_ratio_clamped() {
        // ratio should be at least 1
        let layout = Layout::new().ratio(0);
        assert_eq!(layout.ratio, 1);
    }

    // =========================================================================
    // Layout Splitting Tests
    // =========================================================================

    #[test]
    fn test_layout_split_row_dimensions() {
        let mut layout = Layout::new();
        layout.split_row(vec![
            Layout::new().name("left"),
            Layout::new().name("right"),
        ]);

        let console = Console::builder().width(10).height(2).build();
        let options = console.options();
        let segments = layout.render(&console, &options);
        let lines = split_lines(segments.into_iter().map(Segment::into_owned));

        assert_eq!(lines.len(), 2);
        for line in &lines {
            let width: usize = line.iter().map(Segment::cell_length).sum();
            assert_eq!(width, 10);
        }
    }

    #[test]
    fn test_layout_split_column() {
        let mut layout = Layout::new();
        layout.split_column(vec![
            Layout::new().name("top"),
            Layout::new().name("bottom"),
        ]);

        assert_eq!(layout.splitter, LayoutSplitter::Column);
        assert_eq!(layout.children.len(), 2);
    }

    #[test]
    fn test_layout_split_row() {
        let mut layout = Layout::new();
        layout.split_row(vec![
            Layout::new().name("left"),
            Layout::new().name("right"),
        ]);

        assert_eq!(layout.splitter, LayoutSplitter::Row);
        assert_eq!(layout.children.len(), 2);
    }

    #[test]
    fn test_layout_add_split() {
        let mut layout = Layout::new();
        layout.split_column(vec![Layout::new().name("first")]);
        layout.add_split(vec![Layout::new().name("second")]);

        assert_eq!(layout.children.len(), 2);
    }

    #[test]
    fn test_layout_unsplit() {
        let mut layout = Layout::new();
        layout.split_column(vec![Layout::new().name("a"), Layout::new().name("b")]);
        layout.unsplit();

        assert!(layout.children.is_empty());
    }

    // =========================================================================
    // Layout Lookup Tests
    // =========================================================================

    #[test]
    fn test_layout_named_lookup() {
        let mut layout = Layout::new().name("root");
        layout.split_column(vec![
            Layout::new().name("header").size(1),
            Layout::new().name("body").ratio(2),
        ]);

        assert!(layout.get("root").is_some());
        assert!(layout.get("header").is_some());
        assert!(layout.get("body").is_some());
        assert!(layout.get("missing").is_none());
    }

    #[test]
    fn test_layout_get_mut() {
        let mut layout = Layout::new().name("root");
        layout.split_column(vec![Layout::new().name("child")]);

        let child = layout.get_mut("child");
        assert!(child.is_some());

        // Modify through mut reference
        if let Some(child) = layout.get_mut("child") {
            child.update(Text::new("Updated"));
        }
    }

    #[test]
    fn test_layout_nested_lookup() {
        let mut layout = Layout::new().name("root");
        let mut middle = Layout::new().name("middle");
        middle.split_column(vec![Layout::new().name("deep")]);
        layout.split_column(vec![middle]);

        assert!(layout.get("root").is_some());
        assert!(layout.get("middle").is_some());
        assert!(layout.get("deep").is_some());
    }

    #[test]
    fn test_layout_index_trait() {
        let mut layout = Layout::new().name("root");
        layout.split_column(vec![Layout::new().name("child")]);

        // Test Index trait
        let child = &layout["child"];
        assert_eq!(child.name, Some("child".to_string()));
    }

    #[test]
    fn test_layout_index_mut_trait() {
        let mut layout = Layout::new().name("root");
        layout.split_column(vec![Layout::new().name("child")]);

        // Test IndexMut trait
        layout["child"].update(Text::new("New content"));
    }

    #[test]
    #[should_panic(expected = "Layout not found")]
    fn test_layout_index_missing_panics() {
        let layout = Layout::new().name("root");
        let _ = &layout["nonexistent"];
    }

    // =========================================================================
    // Layout Visibility Tests
    // =========================================================================

    #[test]
    fn test_layout_invisible() {
        let mut layout = Layout::new();
        layout.split_row(vec![
            Layout::new().name("visible"),
            Layout::new().name("hidden").visible(false),
        ]);

        // invisible layout should not appear in visible_children
        let visible = layout.visible_children();
        assert_eq!(visible.len(), 1);
    }

    #[test]
    fn test_layout_all_invisible() {
        let mut layout = Layout::new();
        layout.split_row(vec![
            Layout::new().name("a").visible(false),
            Layout::new().name("b").visible(false),
        ]);

        let visible = layout.visible_children();
        assert!(visible.is_empty());
    }

    // =========================================================================
    // Layout Update Tests
    // =========================================================================

    #[test]
    fn test_layout_update() {
        let mut layout = Layout::new();
        assert!(layout.renderable.is_none());

        layout.update(Text::new("Content"));
        assert!(layout.renderable.is_some());
    }

    #[test]
    fn test_layout_renderable_builder() {
        let layout = Layout::new().renderable(Text::new("Builder content"));
        assert!(layout.renderable.is_some());
    }

    // =========================================================================
    // Region Sizing Tests
    // =========================================================================

    #[test]
    fn test_ratio_resolve_equal() {
        let children: Vec<Layout> = vec![Layout::new().ratio(1), Layout::new().ratio(1)];
        let refs: Vec<&Layout> = children.iter().collect();
        let sizes = ratio_resolve(10, &refs);

        assert_eq!(sizes.len(), 2);
        assert_eq!(sizes[0], 5);
        assert_eq!(sizes[1], 5);
    }

    #[test]
    fn test_ratio_resolve_unequal() {
        let children: Vec<Layout> = vec![Layout::new().ratio(1), Layout::new().ratio(2)];
        let refs: Vec<&Layout> = children.iter().collect();
        let sizes = ratio_resolve(9, &refs);

        assert_eq!(sizes.len(), 2);
        // 1:2 ratio with total 9 should give 3:6
        assert_eq!(sizes[0], 3);
        assert_eq!(sizes[1], 6);
    }

    #[test]
    fn test_ratio_resolve_with_fixed_size() {
        let children: Vec<Layout> = vec![
            Layout::new().size(5),  // Fixed 5
            Layout::new().ratio(1), // Flexible
        ];
        let refs: Vec<&Layout> = children.iter().collect();
        let sizes = ratio_resolve(10, &refs);

        assert_eq!(sizes.len(), 2);
        assert_eq!(sizes[0], 5); // Fixed
        assert_eq!(sizes[1], 5); // Remaining
    }

    #[test]
    fn test_ratio_resolve_empty() {
        let children: Vec<Layout> = vec![];
        let refs: Vec<&Layout> = children.iter().collect();
        let sizes = ratio_resolve(10, &refs);

        assert_eq!(sizes.len(), 0);
    }

    #[test]
    fn test_ratio_resolve_minimum_size() {
        let children: Vec<Layout> = vec![
            Layout::new().minimum_size(8), // Min 8
            Layout::new().minimum_size(8), // Min 8
        ];
        let refs: Vec<&Layout> = children.iter().collect();
        // Total 10, but minimums require 16 - should clamp
        let sizes = ratio_resolve(10, &refs);

        assert_eq!(sizes.len(), 2);
        // Sizes should be clamped to total of 10
        assert_eq!(sizes.iter().sum::<usize>(), 10);
    }

    // =========================================================================
    // Rendering Tests
    // =========================================================================

    #[test]
    fn test_layout_render_empty() {
        let layout = Layout::new().name("empty");
        let console = Console::builder()
            .width(20)
            .height(5)
            .force_terminal(true)
            .build();
        let options = console.options();

        let segments = layout.render(&console, &options);
        // Should render placeholder
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("empty") || text.contains("20") || text.contains('5'));
    }

    #[test]
    fn test_layout_render_with_content() {
        let layout = Layout::new().renderable(Text::new("Hello"));
        let console = Console::builder()
            .width(20)
            .height(3)
            .force_terminal(true)
            .build();
        let options = console.options();

        let segments = layout.render(&console, &options);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("Hello"));
    }

    #[test]
    fn test_layout_render_nested() {
        let mut layout = Layout::new();
        layout.split_column(vec![
            Layout::new().name("top").renderable(Text::new("TOP")),
            Layout::new().name("bottom").renderable(Text::new("BOTTOM")),
        ]);

        let console = Console::builder()
            .width(20)
            .height(4)
            .force_terminal(true)
            .build();
        let options = console.options();

        let segments = layout.render(&console, &options);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("TOP") || text.contains("BOTTOM"));
    }

    #[test]
    fn test_layout_render_row_split() {
        let mut layout = Layout::new();
        layout.split_row(vec![
            Layout::new().name("left").renderable(Text::new("L")),
            Layout::new().name("right").renderable(Text::new("R")),
        ]);

        let console = Console::builder()
            .width(20)
            .height(2)
            .force_terminal(true)
            .build();
        let options = console.options();

        let segments = layout.render(&console, &options);
        let lines = split_lines(segments.into_iter().map(Segment::into_owned));

        // Each line should have total width 20
        for line in lines {
            let width: usize = line.iter().map(Segment::cell_length).sum();
            assert_eq!(width, 20);
        }
    }

    // =========================================================================
    // Edge Cases
    // =========================================================================

    #[test]
    fn test_layout_single_child() {
        let mut layout = Layout::new();
        layout.split_column(vec![Layout::new().name("only")]);

        let console = Console::builder()
            .width(10)
            .height(5)
            .force_terminal(true)
            .build();
        let options = console.options();

        let segments = layout.render(&console, &options);
        let lines = split_lines(segments.into_iter().map(Segment::into_owned));

        // Single child should take full height
        assert_eq!(lines.len(), 5);
    }

    #[test]
    fn test_layout_measure() {
        let layout = Layout::new();
        let console = Console::builder().width(80).height(24).build();
        let options = console.options();

        let measurement = layout.rich_measure(&console, &options);
        assert_eq!(measurement.minimum, 80);
        assert_eq!(measurement.maximum, 80);
    }

    #[test]
    fn test_blank_lines() {
        let lines = blank_lines(5, 3);
        assert_eq!(lines.len(), 3);
        for line in lines {
            let width: usize = line.iter().map(Segment::cell_length).sum();
            assert_eq!(width, 5);
        }
    }

    #[test]
    fn test_placeholder_segments_with_name() {
        let segments = placeholder_segments(Some("myname"), 20, 5);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("myname"));
        assert!(text.contains("20"));
        assert!(text.contains('5'));
    }

    #[test]
    fn test_placeholder_segments_without_name() {
        let segments = placeholder_segments(None, 20, 5);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("20"));
        assert!(text.contains('5'));
    }

    // =========================================================================
    // Integration Tests with Panel
    // =========================================================================

    #[test]
    fn test_layout_with_panel() {
        let layout = Layout::new().renderable(Panel::from_text("Panel content").title("Test"));

        let console = Console::builder()
            .width(30)
            .height(5)
            .force_terminal(true)
            .build();
        let options = console.options();

        let segments = layout.render(&console, &options);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        // Should contain panel content
        assert!(text.contains("Panel content") || text.contains("Test"));
    }

    #[test]
    fn test_divide_row() {
        let children: Vec<Layout> = vec![Layout::new().ratio(1), Layout::new().ratio(1)];
        let refs: Vec<&Layout> = children.iter().collect();
        let region = Region::new(0, 0, 10, 5);

        let result = divide_row(&refs, region);
        assert_eq!(result.len(), 2);

        // First region should start at x=0, second at x=5
        assert_eq!(result[0].0.x, 0);
        assert_eq!(result[0].0.width, 5);
        assert_eq!(result[1].0.x, 5);
        assert_eq!(result[1].0.width, 5);

        // Heights should be the same
        assert_eq!(result[0].0.height, 5);
        assert_eq!(result[1].0.height, 5);
    }

    #[test]
    fn test_divide_column() {
        let children: Vec<Layout> = vec![Layout::new().ratio(1), Layout::new().ratio(1)];
        let refs: Vec<&Layout> = children.iter().collect();
        let region = Region::new(0, 0, 10, 6);

        let result = divide_column(&refs, region);
        assert_eq!(result.len(), 2);

        // First region should start at y=0, second at y=3
        assert_eq!(result[0].0.y, 0);
        assert_eq!(result[0].0.height, 3);
        assert_eq!(result[1].0.y, 3);
        assert_eq!(result[1].0.height, 3);

        // Widths should be the same
        assert_eq!(result[0].0.width, 10);
        assert_eq!(result[1].0.width, 10);
    }
}
