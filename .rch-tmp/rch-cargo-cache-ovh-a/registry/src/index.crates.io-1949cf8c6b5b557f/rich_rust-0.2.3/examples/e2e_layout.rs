use std::time::Instant;

use num_rational::Ratio;
use rich_rust::console::Console;
use rich_rust::prelude::*;
use rich_rust::renderables::{Layout, LayoutSplitter, Region};
use tracing::{debug, info, warn};
use tracing_subscriber::EnvFilter;

#[derive(Clone)]
struct SpecLayout {
    name: Option<&'static str>,
    size: Option<usize>,
    min_size: usize,
    ratio: usize,
    visible: bool,
    splitter: LayoutSplitter,
    children: Vec<SpecLayout>,
}

impl SpecLayout {
    fn new(name: Option<&'static str>) -> Self {
        Self {
            name,
            size: None,
            min_size: 1,
            ratio: 1,
            visible: true,
            splitter: LayoutSplitter::Column,
            children: Vec::new(),
        }
    }

    fn leaf(name: &'static str) -> Self {
        Self::new(Some(name))
    }

    fn split(name: &'static str, splitter: LayoutSplitter, children: Vec<SpecLayout>) -> Self {
        Self {
            name: Some(name),
            splitter,
            children,
            ..Self::new(Some(name))
        }
    }

    fn size(mut self, size: usize) -> Self {
        self.size = Some(size);
        self
    }

    fn min_size(mut self, min_size: usize) -> Self {
        self.min_size = min_size.max(1);
        self
    }

    fn ratio(mut self, ratio: usize) -> Self {
        self.ratio = ratio.max(1);
        self
    }

    fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }

    fn label(&self) -> &'static str {
        self.name.unwrap_or("<unnamed>")
    }

    fn visible_children(&self) -> Vec<&SpecLayout> {
        self.children.iter().filter(|c| c.visible).collect()
    }

    fn log_tree(&self, indent: usize) {
        debug!(
            indent = indent,
            name = self.label(),
            size = ?self.size,
            min_size = self.min_size,
            ratio = self.ratio,
            visible = self.visible,
            splitter = ?self.splitter,
            children = self.children.len(),
            "layout node"
        );
        for child in &self.children {
            child.log_tree(indent + 2);
        }
    }

    fn compute_regions(&self, region: Region, out: &mut Vec<(String, Region)>) {
        out.push((self.label().to_string(), region));

        let children = self.visible_children();
        if children.is_empty() {
            if !self.children.is_empty() {
                warn!(node = self.label(), "all children are hidden");
            }
            return;
        }

        match self.splitter {
            LayoutSplitter::Row => {
                let widths = ratio_resolve(region.width, &children, "width", self.label());
                let mut offset = region.x;
                for (child, width) in children.into_iter().zip(widths) {
                    let child_region = Region::new(offset, region.y, width, region.height);
                    debug!(
                        node = self.label(),
                        child = child.label(),
                        x = child_region.x,
                        y = child_region.y,
                        width = child_region.width,
                        height = child_region.height,
                        "child region"
                    );
                    child.compute_regions(child_region, out);
                    offset = offset.saturating_add(width);
                }
            }
            LayoutSplitter::Column => {
                let heights = ratio_resolve(region.height, &children, "height", self.label());
                let mut offset = region.y;
                for (child, height) in children.into_iter().zip(heights) {
                    let child_region = Region::new(region.x, offset, region.width, height);
                    debug!(
                        node = self.label(),
                        child = child.label(),
                        x = child_region.x,
                        y = child_region.y,
                        width = child_region.width,
                        height = child_region.height,
                        "child region"
                    );
                    child.compute_regions(child_region, out);
                    offset = offset.saturating_add(height);
                }
            }
        }
    }
}

fn ratio_resolve(total: usize, children: &[&SpecLayout], axis: &str, node: &str) -> Vec<usize> {
    if children.is_empty() {
        return Vec::new();
    }

    let mut sizes = Vec::with_capacity(children.len());
    let mut mins = Vec::with_capacity(children.len());
    let mut ratios = Vec::with_capacity(children.len());

    let mut fixed_total = 0usize;
    let mut flex_min_total = 0usize;

    for child in children {
        let min_size = child.min_size.max(1);
        mins.push(min_size);
        if let Some(size) = child.size {
            let size = size.max(min_size);
            sizes.push(size);
            ratios.push(0);
            fixed_total = fixed_total.saturating_add(size);
        } else {
            sizes.push(min_size);
            ratios.push(child.ratio.max(1));
            flex_min_total = flex_min_total.saturating_add(min_size);
        }
    }

    let min_sum = fixed_total.saturating_add(flex_min_total);
    if min_sum > total {
        warn!(
            node,
            axis, total, min_sum, "total is smaller than sum of minimum sizes"
        );
    }

    let mut remaining = total.saturating_sub(min_sum);
    let total_ratio: usize = ratios.iter().sum();

    debug!(
        node,
        axis,
        total,
        fixed_total,
        flex_min_total,
        remaining,
        ratios = ?ratios,
        mins = ?mins,
        "ratio resolve start"
    );

    if total_ratio > 0 && remaining > 0 {
        let mut distributed = 0usize;
        let mut flex_idx = 0usize;
        let flex_count = ratios.iter().filter(|&&r| r > 0).count();
        for (i, &ratio) in ratios.iter().enumerate() {
            if ratio > 0 {
                flex_idx += 1;
                let share = Ratio::new(ratio, total_ratio);
                let extra = if flex_idx == flex_count {
                    remaining.saturating_sub(distributed)
                } else {
                    (share * remaining).round().to_integer()
                };
                sizes[i] = sizes[i].saturating_add(extra);
                distributed = distributed.saturating_add(extra);
            }
        }
        remaining = remaining.saturating_sub(distributed);
    }

    if remaining > 0 {
        let mut idx = 0usize;
        while remaining > 0 && !sizes.is_empty() {
            sizes[idx] = sizes[idx].saturating_add(1);
            remaining = remaining.saturating_sub(1);
            idx = (idx + 1) % sizes.len();
        }
    }

    sizes = clamp_sizes(sizes, &mins, total, axis, node);

    debug!(node, axis, sizes = ?sizes, "ratio resolve result");
    sizes
}

fn clamp_sizes(
    mut sizes: Vec<usize>,
    mins: &[usize],
    total: usize,
    axis: &str,
    node: &str,
) -> Vec<usize> {
    let mut sum: usize = sizes.iter().sum();
    if sum <= total {
        return sizes;
    }

    while sum > total {
        let mut reduced = false;
        for idx in (0..sizes.len()).rev() {
            if sizes[idx] > mins[idx] {
                sizes[idx] = sizes[idx].saturating_sub(1);
                sum = sum.saturating_sub(1);
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
        warn!(
            node,
            axis, total, sum, "sizes exceed total after minimum clamping"
        );
        let mut idx = 0usize;
        while sum > total && !sizes.is_empty() {
            if sizes[idx] > 0 {
                sizes[idx] = sizes[idx].saturating_sub(1);
                sum = sum.saturating_sub(1);
            }
            idx = (idx + 1) % sizes.len();
        }
    }

    sizes
}

fn init_logging() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("debug"));
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}

fn run_case(name: &str, layout: &Layout, spec: &SpecLayout, widths: &[usize], height: usize) {
    info!(case = name, height, "case start");
    spec.log_tree(0);

    for &width in widths {
        if width == 0 || height == 0 {
            warn!(case = name, width, height, "rendering with zero dimension");
        }
        let region = Region::new(0, 0, width, height);
        let mut regions = Vec::new();
        spec.compute_regions(region, &mut regions);
        for (label, region) in &regions {
            debug!(
                case = name,
                label,
                x = region.x,
                y = region.y,
                width = region.width,
                height = region.height,
                "region allocation"
            );
        }

        let console = Console::builder()
            .width(width)
            .height(height)
            .force_terminal(true)
            .build();
        let start = Instant::now();
        let output = console.export_renderable_text(layout);
        let elapsed = start.elapsed();
        debug!(
            case = name,
            width,
            height,
            elapsed_ms = elapsed.as_millis(),
            output_len = output.len(),
            output_lines = output.lines().count(),
            "render pass complete"
        );
    }

    info!(case = name, "case complete");
}

fn scenario_single_row() -> (Layout, SpecLayout) {
    let mut layout = Layout::new().name("root");
    let left = Layout::from_renderable(Text::new("Left"))
        .name("left")
        .ratio(1);
    let right = Layout::from_renderable(Text::new("Right"))
        .name("right")
        .ratio(1);
    layout.split_row(vec![left, right]);

    let spec = SpecLayout::split(
        "root",
        LayoutSplitter::Row,
        vec![SpecLayout::leaf("left"), SpecLayout::leaf("right")],
    );
    (layout, spec)
}

fn scenario_single_column_with_update() -> (Layout, SpecLayout) {
    let mut layout = Layout::new().name("root");
    let header = Layout::from_renderable(Text::new("Header"))
        .name("header")
        .size(2);
    let body = Layout::from_renderable(Text::new("Body"))
        .name("body")
        .ratio(2);
    layout.split_column(vec![header, body]);

    let spec = SpecLayout::split(
        "root",
        LayoutSplitter::Column,
        vec![
            SpecLayout::leaf("header").size(2),
            SpecLayout::leaf("body").ratio(2),
        ],
    );
    (layout, spec)
}

fn scenario_nested_layout() -> (Layout, SpecLayout) {
    let mut root = Layout::new().name("root");
    let header = Layout::from_renderable(Text::new("Header"))
        .name("header")
        .size(3);
    let mut body = Layout::new().name("body");
    let left = Layout::from_renderable(Text::new("Left"))
        .name("left")
        .ratio(1)
        .minimum_size(8);

    let mut right = Layout::new().name("right");
    let top = Layout::from_renderable(Text::new("Top"))
        .name("top")
        .ratio(1);
    let mid = Layout::from_renderable(Text::new("Middle"))
        .name("middle")
        .ratio(2);
    let bottom = Layout::from_renderable(Text::new("Bottom"))
        .name("bottom")
        .ratio(3);
    right.split_column(vec![top, mid, bottom]);

    body.split_row(vec![left, right]);
    root.split_column(vec![header, body]);

    let spec = SpecLayout::split(
        "root",
        LayoutSplitter::Column,
        vec![
            SpecLayout::leaf("header").size(3),
            SpecLayout::split(
                "body",
                LayoutSplitter::Row,
                vec![
                    SpecLayout::leaf("left").ratio(1).min_size(8),
                    SpecLayout::split(
                        "right",
                        LayoutSplitter::Column,
                        vec![
                            SpecLayout::leaf("top").ratio(1),
                            SpecLayout::leaf("middle").ratio(2),
                            SpecLayout::leaf("bottom").ratio(3),
                        ],
                    ),
                ],
            )
            .ratio(3),
        ],
    );

    (root, spec)
}

fn scenario_minimum_sizes() -> (Layout, SpecLayout) {
    let mut layout = Layout::new().name("root");
    let left = Layout::from_renderable(Text::new("A"))
        .name("a")
        .minimum_size(8);
    let middle = Layout::from_renderable(Text::new("B"))
        .name("b")
        .minimum_size(8);
    let right = Layout::from_renderable(Text::new("C"))
        .name("c")
        .minimum_size(8);
    layout.split_row(vec![left, middle, right]);

    let spec = SpecLayout::split(
        "root",
        LayoutSplitter::Row,
        vec![
            SpecLayout::leaf("a").min_size(8),
            SpecLayout::leaf("b").min_size(8),
            SpecLayout::leaf("c").min_size(8),
        ],
    );
    (layout, spec)
}

fn scenario_visibility(hidden: bool) -> (Layout, SpecLayout) {
    let mut layout = Layout::new().name("root");
    let left = Layout::from_renderable(Text::new("Visible")).name("left");
    let right = Layout::from_renderable(Text::new("Hidden"))
        .name("right")
        .visible(!hidden);
    layout.split_row(vec![left, right]);

    let spec = SpecLayout::split(
        "root",
        LayoutSplitter::Row,
        vec![
            SpecLayout::leaf("left"),
            SpecLayout::leaf("right").visible(!hidden),
        ],
    );
    (layout, spec)
}

fn main() {
    init_logging();
    info!("layout e2e script start");

    let (layout, spec) = scenario_single_row();
    run_case("single_row_1_1", &layout, &spec, &[0, 5, 20, 80], 6);

    let (mut layout, spec) = scenario_single_column_with_update();
    info!("named lookup: body = {:?}", layout.get("body").is_some());
    if let Some(body) = layout.get_mut("body") {
        body.update(Panel::from_text("Updated Body").title("Body"));
    }
    run_case("single_column_update", &layout, &spec, &[10, 40, 120], 6);

    let (layout, spec) = scenario_nested_layout();
    run_case("nested_3_depth", &layout, &spec, &[20, 40, 80], 10);

    let (layout, spec) = scenario_minimum_sizes();
    run_case("minimum_size_enforcement", &layout, &spec, &[10, 16, 30], 4);

    let (layout_hidden, spec_hidden) = scenario_visibility(true);
    run_case(
        "visibility_hidden",
        &layout_hidden,
        &spec_hidden,
        &[20, 40],
        4,
    );

    let (layout_visible, spec_visible) = scenario_visibility(false);
    run_case(
        "visibility_visible",
        &layout_visible,
        &spec_visible,
        &[20, 40],
        4,
    );

    info!("layout e2e script complete");
}
