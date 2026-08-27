# Proposed Architecture for `rich_rust`

> **Author:** Gemini
> **Date:** 2026-01-16
> **Reference:** `EXISTING_RICH_STRUCTURE_AND_ARCHITECTURE.md`

## Executive Summary

This document defines the Rust architecture for `rich_rust`. It translates the dynamic, protocol-based architecture of Python's Rich into a static, trait-based, and zero-cost architecture in Rust.

## 1. Core Traits (The Protocols)

In Python, Rich relies on `__rich_console__` and `__rich__` dunder methods. In Rust, we will define traits.

### 1.1 `ConsoleRender` (The Primary Trait)

This is the equivalent of `__rich_console__`. It produces an iterator of Segments.

```rust
pub trait ConsoleRender {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> RenderResult;
}

// RenderResult is likely an Iterator or a custom struct that implements Iterator
pub type RenderResult = Box<dyn Iterator<Item = Segment> + Send>; 
// OR: simplified to return a Vec<Segment> for Phase 1 simplicity
```

### 1.2 `RichDisplay` (The Conversion Trait)

Equivalent to `__rich__`. It converts a high-level object into something that implements `ConsoleRender` (usually `Text`).

```rust
pub trait RichDisplay {
    fn to_rich(&self) -> impl ConsoleRender;
}
```

### 1.3 `Measure` (The Layout Trait)

Equivalent to `__rich_measure__`.

```rust
pub trait Measure {
    fn measure(&self, console: &Console, options: &ConsoleOptions) -> Measurement;
}

pub struct Measurement {
    pub min: usize,
    pub max: usize,
}
```

## 2. Core Data Structures

### 2.1 `Console`

The coordinator.

```rust
pub struct Console {
    pub options: ConsoleOptions,
    writer: Box<dyn Write + Send + Sync>,
    // thread-local buffer?
}

impl Console {
    pub fn print(&self, renderable: &impl ConsoleRender) {
        // 1. Get iterator from renderable
        // 2. Iterate segments
        // 3. Diff styles
        // 4. Write ANSI codes + Text to stream
    }
}
```

### 2.2 `Style`

Optimized for size and copying.

```rust
use bitflags::bitflags;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub struct Style {
    pub foreground: Option<Color>,
    pub background: Option<Color>,
    pub attributes: Attributes,
}

bitflags! {
    #[derive(Default)]
    pub struct Attributes: u16 {
        const BOLD      = 1 << 0;
        const DIM       = 1 << 1;
        const ITALIC    = 1 << 2;
        const UNDERLINE = 1 << 3;
        const BLINK     = 1 << 4;
        const REVERSE   = 1 << 5;
        const HIDDEN    = 1 << 6;
        const STRIKE    = 1 << 7;
    }
}
```

### 2.3 `Text` and `Segment`

```rust
pub struct Segment {
    pub text: String, // Or Cow<'a, str> for optimization
    pub style: Style,
}

pub struct Text {
    pub spans: Vec<Span>,
    pub plain: String,
}

pub struct Span {
    pub start: usize,
    pub end: usize,
    pub style: Style,
}
```

## 3. Rendering Pipeline Strategy

### 3.1 Immediate Mode vs Buffering

Rich (Python) is largely immediate mode but buffers lines for layout (tables). `rich_rust` will strictly follow the **Iterator** pattern. Renderables will return Iterators that yield Segments lazily where possible.

### 3.2 ANSI Generation

We will use a dedicated module `ansi.rs` to handle the diffing of styles.

```rust
// Logic:
// current_style = Style::default();
// for segment in segments {
//     let diff_codes = current_style.diff(segment.style);
//     writer.write(diff_codes);
//     writer.write(segment.text);
//     current_style = segment.style;
// }
// writer.write(RESET);
```

## 4. Layout Engine

The `Table` implementation is the hardest part.

1.  **Measure Pass:** Call `measure()` on all cells to determine min/max widths.
2.  **Calculate Column Widths:** Use the same ratio/distribute algorithm as Python (ported to Rust).
3.  **Render Pass:** Call `render()` with the calculated column widths injected into `ConsoleOptions`.

## 5. Ecosystem Dependencies

| Component | Recommended Crate |
|-----------|-------------------|
| CLI Args | `clap` |
| Regex | `regex` (for markup parsing) |
| Colors | `palette` or custom struct |
| Terminal | `crossterm` (for detection/size) |
| Syntax | `syntect` |
| Markdown | `pulldown-cmark` |

## 6. Live Display System

> Reference: `RICH_SPEC.md` Section 16

The `Live` system enables dynamic, auto-refreshing displays for progress bars, spinners, and dashboards.

### 6.1 Core Types

```rust
/// Vertical overflow handling strategy
#[derive(Clone, Copy, Debug, Default)]
pub enum VerticalOverflow {
    Crop,       // Truncate excess lines
    #[default]
    Ellipsis,   // Show "..." for overflow
    Visible,    // Allow overflow (final render only)
}

/// Configuration for Live display
#[derive(Clone)]
pub struct LiveOptions {
    pub screen: bool,                      // Use alternate screen buffer
    pub auto_refresh: bool,                // Enable refresh thread (default: true)
    pub refresh_per_second: f64,           // Refresh rate (default: 4.0)
    pub transient: bool,                   // Clear display on exit
    pub redirect_stdout: bool,             // Capture stdout (default: true)
    pub redirect_stderr: bool,             // Capture stderr (default: true)
    pub vertical_overflow: VerticalOverflow,
}

impl Default for LiveOptions {
    fn default() -> Self {
        Self {
            screen: false,
            auto_refresh: true,
            refresh_per_second: 4.0,
            transient: false,
            redirect_stdout: true,
            redirect_stderr: true,
            vertical_overflow: VerticalOverflow::Ellipsis,
        }
    }
}
```

### 6.2 Live Struct

```rust
use std::sync::{Arc, RwLock, atomic::{AtomicBool, Ordering}};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub struct Live<'a> {
    // Content
    renderable: Arc<RwLock<Option<Box<dyn ConsoleRender + Send + Sync>>>>,
    get_renderable: Option<Box<dyn Fn() -> Box<dyn ConsoleRender + Send + Sync> + Send + Sync>>,

    // Console integration
    console: &'a Console,
    options: LiveOptions,

    // State
    started: AtomicBool,
    nested: AtomicBool,
    alt_screen_active: AtomicBool,

    // Refresh thread
    refresh_thread: Option<JoinHandle<()>>,
    refresh_stop: Arc<AtomicBool>,

    // Rendering state
    live_render: Arc<RwLock<LiveRender>>,
}

/// Tracks cursor position and rendered lines for refresh
pub struct LiveRender {
    shape: Option<(usize, usize)>,  // (width, height) of last render
    last_lines: usize,              // Lines rendered on last refresh
}
```

### 6.3 Builder Pattern

```rust
impl<'a> Live<'a> {
    pub fn new(console: &'a Console) -> Self {
        Self::with_options(console, LiveOptions::default())
    }

    pub fn with_options(console: &'a Console, options: LiveOptions) -> Self {
        assert!(options.refresh_per_second > 0.0, "refresh_per_second must be > 0");
        let mut opts = options;
        if opts.screen {
            opts.transient = true;  // Screen mode implies transient
        }
        Self {
            console,
            options: opts,
            renderable: Arc::new(RwLock::new(None)),
            get_renderable: None,
            started: AtomicBool::new(false),
            nested: AtomicBool::new(false),
            alt_screen_active: AtomicBool::new(false),
            refresh_thread: None,
            refresh_stop: Arc::new(AtomicBool::new(false)),
            live_render: Arc::new(RwLock::new(LiveRender::default())),
        }
    }

    pub fn renderable<R: ConsoleRender + Send + Sync + 'static>(mut self, r: R) -> Self {
        *self.renderable.write().unwrap() = Some(Box::new(r));
        self
    }

    pub fn get_renderable<F>(mut self, f: F) -> Self
    where
        F: Fn() -> Box<dyn ConsoleRender + Send + Sync> + Send + Sync + 'static,
    {
        self.get_renderable = Some(Box::new(f));
        self
    }
}
```

### 6.4 Lifecycle Management

```rust
impl<'a> Live<'a> {
    pub fn start(&mut self) -> Result<(), LiveError> {
        if self.started.swap(true, Ordering::SeqCst) {
            return Ok(());  // Already started
        }

        // Register with console (detect nesting)
        if !self.console.set_live(self) {
            self.nested.store(true, Ordering::SeqCst);
            return Ok(());  // Nested, delegate to parent
        }

        // Enable alternate screen
        if self.options.screen {
            self.console.set_alt_screen(true)?;
            self.alt_screen_active.store(true, Ordering::SeqCst);
        }

        // Hide cursor
        self.console.show_cursor(false)?;

        // Push render hook for output interception
        self.console.push_render_hook(self);

        // Initial refresh
        self.refresh()?;

        // Start refresh thread
        if self.options.auto_refresh {
            self.start_refresh_thread();
        }

        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), LiveError> {
        if !self.started.swap(false, Ordering::SeqCst) {
            return Ok(());  // Already stopped
        }

        // Stop refresh thread
        if let Some(handle) = self.refresh_thread.take() {
            self.refresh_stop.store(true, Ordering::SeqCst);
            let _ = handle.join();
        }

        if self.nested.load(Ordering::SeqCst) {
            return Ok(());  // Nested, parent handles cleanup
        }

        // Pop render hook
        self.console.pop_render_hook();

        // Final refresh with transient handling
        if !self.options.transient {
            // Render one last time without overflow cropping
            self.refresh_with_overflow(VerticalOverflow::Visible)?;
            self.console.line()?;  // Newline after final content
        }

        // Restore cursor
        self.console.show_cursor(true)?;

        // Disable alternate screen
        if self.alt_screen_active.load(Ordering::SeqCst) {
            self.console.set_alt_screen(false)?;
        }

        // Unregister from console
        self.console.clear_live();

        Ok(())
    }
}

// Drop implementation ensures cleanup
impl<'a> Drop for Live<'a> {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}
```

### 6.5 Refresh Thread

```rust
impl<'a> Live<'a> {
    fn start_refresh_thread(&mut self) {
        let renderable = Arc::clone(&self.renderable);
        let live_render = Arc::clone(&self.live_render);
        let stop = Arc::clone(&self.refresh_stop);
        let interval = Duration::from_secs_f64(1.0 / self.options.refresh_per_second);
        let console = self.console.clone();  // Console is Clone/Arc-based

        self.refresh_thread = Some(thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                thread::sleep(interval);
                if !stop.load(Ordering::Relaxed) {
                    let guard = renderable.read().unwrap();
                    if let Some(ref r) = *guard {
                        let mut lr = live_render.write().unwrap();
                        let _ = lr.refresh(&console, r.as_ref());
                    }
                }
            }
        }));
    }
}
```

### 6.6 Refresh Logic

```rust
impl LiveRender {
    /// Refresh the display with current content
    pub fn refresh(
        &mut self,
        console: &Console,
        renderable: &dyn ConsoleRender,
    ) -> Result<(), LiveError> {
        self.refresh_inner(console, renderable, VerticalOverflow::Ellipsis)
    }

    fn refresh_inner(
        &mut self,
        console: &Console,
        renderable: &dyn ConsoleRender,
        overflow: VerticalOverflow,
    ) -> Result<(), LiveError> {
        let (width, height) = console.size();
        let shape = (width, height);

        // Check if terminal resized
        let shape_changed = self.shape != Some(shape);
        self.shape = Some(shape);

        // Calculate available height for content
        let available_height = height.saturating_sub(1);  // Leave room for cursor

        // Render content to segments
        let options = ConsoleOptions {
            max_width: Some(width),
            ..console.options()
        };
        let segments: Vec<Segment> = renderable.render(console, &options).collect();

        // Split into lines and handle overflow
        let lines = Segment::split_lines(&segments);
        let (display_lines, needs_ellipsis) = match overflow {
            VerticalOverflow::Crop => {
                let cropped: Vec<_> = lines.take(available_height).collect();
                (cropped, false)
            }
            VerticalOverflow::Ellipsis => {
                let all_lines: Vec<_> = lines.collect();
                if all_lines.len() > available_height {
                    let mut display = all_lines.into_iter().take(available_height - 1).collect::<Vec<_>>();
                    (display, true)
                } else {
                    (all_lines, false)
                }
            }
            VerticalOverflow::Visible => {
                (lines.collect(), false)
            }
        };

        // Move cursor up to overwrite previous render
        if self.last_lines > 0 && !shape_changed {
            console.control(Control::move_up(self.last_lines))?;
        }

        // Clear and write new content
        for line in &display_lines {
            console.print_segments(line)?;
            console.control(Control::erase_line(EraseMode::ToEnd))?;
            console.line()?;
        }

        // Ellipsis indicator
        if needs_ellipsis {
            console.print_styled("...", Style::dim())?;
            console.control(Control::erase_line(EraseMode::ToEnd))?;
        }

        // Clear any remaining lines from previous render
        let current_lines = display_lines.len() + if needs_ellipsis { 1 } else { 0 };
        for _ in current_lines..self.last_lines {
            console.control(Control::erase_line(EraseMode::All))?;
            console.line()?;
        }

        self.last_lines = current_lines;
        console.flush()?;

        Ok(())
    }
}
```

### 6.7 Console Integration Points

The `Console` struct needs these additions for Live support:

```rust
impl Console {
    // Live registration (returns false if already has active Live)
    pub fn set_live(&self, live: &Live) -> bool;
    pub fn clear_live(&self);

    // Render hooks for output interception
    pub fn push_render_hook(&self, hook: &dyn RenderHook);
    pub fn pop_render_hook(&self);

    // Terminal control
    pub fn set_alt_screen(&self, enable: bool) -> Result<(), ConsoleError>;
    pub fn show_cursor(&self, show: bool) -> Result<(), ConsoleError>;
    pub fn control(&self, ctrl: Control) -> Result<(), ConsoleError>;
}

pub trait RenderHook {
    fn process(&self, segments: &[Segment]) -> RenderHookResult;
}

pub enum RenderHookResult {
    Passthrough,           // Continue normal output
    Intercept(Vec<Segment>), // Modified output
    Suppress,              // Suppress output entirely
}
```

### 6.8 Usage Examples

```rust
// Basic usage with context manager pattern
fn main() -> Result<(), Box<dyn Error>> {
    let console = Console::new();

    let mut live = Live::new(&console)
        .renderable(Panel::new("Loading..."));

    live.start()?;

    for i in 0..100 {
        live.update(Panel::new(format!("Progress: {}%", i)));
        thread::sleep(Duration::from_millis(50));
    }

    live.stop()?;
    Ok(())
}

// With dynamic get_renderable callback
let mut live = Live::new(&console)
    .get_renderable(|| Box::new(get_current_status()));

// Alternate screen mode for full-screen apps
let mut live = Live::with_options(&console, LiveOptions {
    screen: true,
    refresh_per_second: 30.0,
    ..Default::default()
});
```

### 6.9 Threading Considerations

- **RwLock for renderable:** Allows reads from refresh thread while main thread updates
- **AtomicBool for state:** Lock-free status checks
- **Arc sharing:** Console and LiveRender shared between main and refresh threads
- **Graceful shutdown:** `refresh_stop` flag checked before each refresh

## 7. Layout System

> Reference: `RICH_SPEC.md` Section 17

The `Layout` system creates dashboard-style interfaces by dividing screen space into rows and columns using ratio-based distribution.

### 7.1 Core Types

```rust
/// Rectangular region on screen
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Region {
    pub x: usize,      // Horizontal position (0 = left)
    pub y: usize,      // Vertical position (0 = top)
    pub width: usize,
    pub height: usize,
}

impl Region {
    pub fn new(x: usize, y: usize, width: usize, height: usize) -> Self {
        Self { x, y, width, height }
    }

    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

    /// Slice region horizontally at offset, returning (left, right)
    pub fn split_horizontal(self, offset: usize) -> (Self, Self) {
        let left = Self { width: offset, ..self };
        let right = Self { x: self.x + offset, width: self.width.saturating_sub(offset), ..self };
        (left, right)
    }

    /// Slice region vertically at offset, returning (top, bottom)
    pub fn split_vertical(self, offset: usize) -> (Self, Self) {
        let top = Self { height: offset, ..self };
        let bottom = Self { y: self.y + offset, height: self.height.saturating_sub(offset), ..self };
        (top, bottom)
    }
}
```

### 7.2 Splitter Trait

```rust
/// Strategy for dividing a region among children
pub trait Splitter: Send + Sync {
    fn name(&self) -> &str;
    fn tree_icon(&self) -> &str;
    fn divide(&self, children: &[&Layout], region: Region) -> Vec<Region>;
}

/// Split horizontally (children side-by-side)
pub struct RowSplitter;

impl Splitter for RowSplitter {
    fn name(&self) -> &str { "row" }
    fn tree_icon(&self) -> &str { "⬌" }

    fn divide(&self, children: &[&Layout], region: Region) -> Vec<Region> {
        let widths = ratio_resolve(region.width, children);
        let mut regions = Vec::with_capacity(children.len());
        let mut x = region.x;

        for width in widths {
            regions.push(Region { x, width, ..region });
            x += width;
        }
        regions
    }
}

/// Split vertically (children stacked)
pub struct ColumnSplitter;

impl Splitter for ColumnSplitter {
    fn name(&self) -> &str { "column" }
    fn tree_icon(&self) -> &str { "⬍" }

    fn divide(&self, children: &[&Layout], region: Region) -> Vec<Region> {
        let heights = ratio_resolve(region.height, children);
        let mut regions = Vec::with_capacity(children.len());
        let mut y = region.y;

        for height in heights {
            regions.push(Region { y, height, ..region });
            y += height;
        }
        regions
    }
}
```

### 7.3 Edge Trait (for Ratio Resolution)

```rust
/// Element with size constraints for ratio distribution
pub trait Edge {
    fn size(&self) -> Option<usize>;      // Fixed size, or None for flexible
    fn ratio(&self) -> usize;             // Flex ratio (default: 1)
    fn minimum_size(&self) -> usize;      // Minimum allowed size (default: 1)
}
```

### 7.4 Layout Struct

```rust
use std::sync::{Arc, RwLock};
use std::collections::HashMap;

/// Result of rendering a layout region
pub struct LayoutRender {
    pub region: Region,
    pub lines: Vec<Vec<Segment>>,  // Rendered content as lines
}

pub type RenderMap = HashMap<String, LayoutRender>;

pub struct Layout {
    // Identity & content
    name: Option<String>,
    renderable: Box<dyn ConsoleRender + Send + Sync>,

    // Size constraints (implements Edge)
    size: Option<usize>,       // Fixed size (cells for row, lines for column)
    minimum_size: usize,       // Minimum size (default: 1)
    ratio: usize,              // Flex ratio (default: 1)

    // State
    visible: bool,             // Include in layout (default: true)
    splitter: Box<dyn Splitter>,
    children: Vec<Layout>,

    // Render cache (for nested access)
    render_map: Arc<RwLock<RenderMap>>,
}

impl Edge for Layout {
    fn size(&self) -> Option<usize> { self.size }
    fn ratio(&self) -> usize { self.ratio }
    fn minimum_size(&self) -> usize { self.minimum_size }
}
```

### 7.5 Builder Pattern

```rust
impl Layout {
    pub fn new() -> Self {
        Self {
            name: None,
            renderable: Box::new(Placeholder),
            size: None,
            minimum_size: 1,
            ratio: 1,
            visible: true,
            splitter: Box::new(ColumnSplitter),
            children: Vec::new(),
            render_map: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn renderable<R: ConsoleRender + Send + Sync + 'static>(mut self, r: R) -> Self {
        self.renderable = Box::new(r);
        self
    }

    pub fn size(mut self, size: usize) -> Self {
        self.size = Some(size);
        self
    }

    pub fn minimum_size(mut self, min: usize) -> Self {
        self.minimum_size = min;
        self
    }

    pub fn ratio(mut self, ratio: usize) -> Self {
        self.ratio = ratio;
        self
    }

    pub fn visible(mut self, visible: bool) -> Self {
        self.visible = visible;
        self
    }
}
```

### 7.6 Split Operations

```rust
impl Layout {
    /// Split into sub-layouts with given splitter
    pub fn split<S: Splitter + 'static>(&mut self, splitter: S, children: Vec<Layout>) {
        self.splitter = Box::new(splitter);
        self.children = children;
    }

    /// Split horizontally (children side-by-side)
    pub fn split_row(&mut self, children: Vec<Layout>) {
        self.split(RowSplitter, children);
    }

    /// Split vertically (children stacked)
    pub fn split_column(&mut self, children: Vec<Layout>) {
        self.split(ColumnSplitter, children);
    }

    /// Add children to existing split
    pub fn add_split(&mut self, children: Vec<Layout>) {
        self.children.extend(children);
    }

    /// Remove all children
    pub fn unsplit(&mut self) {
        self.children.clear();
    }

    /// Check if this layout has children
    pub fn is_split(&self) -> bool {
        !self.children.is_empty()
    }
}
```

### 7.7 Named Lookup

```rust
impl Layout {
    /// Get layout by name (recursive search)
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

    /// Get mutable layout by name
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

    /// Update content of named layout
    pub fn update<R: ConsoleRender + Send + Sync + 'static>(&mut self, name: &str, renderable: R) {
        if let Some(layout) = self.get_mut(name) {
            layout.renderable = Box::new(renderable);
        }
    }
}

// Index syntax support
impl std::ops::Index<&str> for Layout {
    type Output = Layout;
    fn index(&self, name: &str) -> &Self::Output {
        self.get(name).expect("Layout not found")
    }
}

impl std::ops::IndexMut<&str> for Layout {
    fn index_mut(&mut self, name: &str) -> &mut Self::Output {
        self.get_mut(name).expect("Layout not found")
    }
}
```

### 7.8 Rendering Algorithm

```rust
impl Layout {
    /// Render into a region, returning region map
    pub fn render_to_region(
        &self,
        console: &Console,
        region: Region,
    ) -> RenderMap {
        let mut render_map = HashMap::new();
        self.render_recursive(console, region, &mut render_map);
        render_map
    }

    fn render_recursive(
        &self,
        console: &Console,
        region: Region,
        render_map: &mut RenderMap,
    ) {
        if !self.visible || region.is_empty() {
            return;
        }

        if self.is_split() {
            // Get visible children
            let visible: Vec<&Layout> = self.children.iter()
                .filter(|c| c.visible)
                .collect();

            // Divide region among children
            let child_regions = self.splitter.divide(&visible, region);

            // Recurse into each child
            for (child, child_region) in visible.into_iter().zip(child_regions) {
                child.render_recursive(console, child_region, render_map);
            }
        } else {
            // Leaf node: render content
            let options = ConsoleOptions {
                max_width: Some(region.width),
                height: Some(region.height),
                ..console.options()
            };

            let segments: Vec<Segment> = self.renderable
                .render(console, &options)
                .collect();

            // Split into lines and crop to region height
            let mut lines: Vec<Vec<Segment>> = Segment::split_lines(&segments)
                .take(region.height)
                .collect();

            // Pad to full height if needed
            while lines.len() < region.height {
                lines.push(vec![Segment::new(" ".repeat(region.width))]);
            }

            // Pad/crop each line to region width
            for line in &mut lines {
                let line_width: usize = line.iter().map(|s| s.cell_len()).sum();
                if line_width < region.width {
                    line.push(Segment::new(" ".repeat(region.width - line_width)));
                }
                // Cropping handled by ConsoleOptions::max_width
            }

            let layout_render = LayoutRender { region, lines };

            if let Some(name) = &self.name {
                render_map.insert(name.clone(), layout_render);
            }
        }
    }
}
```

### 7.9 ConsoleRender Implementation

```rust
impl ConsoleRender for Layout {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> RenderResult {
        let width = options.max_width.unwrap_or_else(|| console.width());
        let height = options.height.unwrap_or_else(|| console.height());

        let region = Region::new(0, 0, width, height);
        let render_map = self.render_to_region(console, region);

        // Composite all rendered regions into output lines
        let mut canvas: Vec<Vec<Segment>> = vec![vec![]; height];

        for (_, layout_render) in &render_map {
            let LayoutRender { region, lines } = layout_render;
            for (i, line) in lines.iter().enumerate() {
                let y = region.y + i;
                if y < canvas.len() {
                    // Insert segments at correct x position
                    // (In practice, use a more sophisticated compositor)
                    canvas[y].extend(line.iter().cloned());
                }
            }
        }

        // Flatten to segments
        let segments: Vec<Segment> = canvas.into_iter()
            .flat_map(|line| {
                let mut row = line;
                row.push(Segment::newline());
                row
            })
            .collect();

        Box::new(segments.into_iter())
    }
}
```

### 7.10 Placeholder Renderable

```rust
/// Default placeholder shown when no content is set
pub struct Placeholder;

impl ConsoleRender for Placeholder {
    fn render(&self, console: &Console, options: &ConsoleOptions) -> RenderResult {
        let width = options.max_width.unwrap_or(80);
        let height = options.height.unwrap_or(1);

        let text = format!("({} x {})", width, height);
        let styled = Text::styled(&text, Style::dim());

        Box::new(styled.render(console, options))
    }
}
```

### 7.11 Usage Examples

```rust
// Basic 2-column layout
let mut layout = Layout::new();
layout.split_row(vec![
    Layout::new().name("left").renderable(Panel::new("Left pane")),
    Layout::new().name("right").renderable(Panel::new("Right pane")),
]);

// Dashboard with header, body (2 columns), footer
let mut layout = Layout::new();
layout.split_column(vec![
    Layout::new().name("header").size(3).renderable(Panel::new("Header")),
    Layout::new().name("body"),  // Will be split further
    Layout::new().name("footer").size(3).renderable(Panel::new("Footer")),
]);

layout["body"].split_row(vec![
    Layout::new().name("sidebar").size(20).renderable(list),
    Layout::new().name("main").ratio(3).renderable(content),
]);

// Update content dynamically
layout.update("main", new_content);

// Render to console
console.print(&layout)?;
```

### 7.12 Integration with Live

Layout is often used with Live for real-time dashboards:

```rust
let mut layout = build_dashboard_layout();
let mut live = Live::new(&console)
    .renderable(layout.clone());

live.start()?;

loop {
    // Update layout contents
    layout.update("status", get_current_status());
    layout.update("logs", get_recent_logs());

    live.update(layout.clone());
    thread::sleep(Duration::from_millis(100));
}
```

### 7.13 Thread Safety

- **RenderMap with Arc<RwLock>:** Allows safe access from multiple threads
- **Clone for updates:** Create new Layout trees rather than mutating shared state
- **Lock scope:** Keep locks brief during render; release before I/O

## 8. Directory Structure

```
src/
├── main.rs (CLI entry point for testing)
├── lib.rs
├── console.rs
├── style.rs
├── text.rs
├── segment.rs
├── measure.rs
├── terminal.rs
├── live/
│   ├── mod.rs
│   ├── live.rs
│   └── live_render.rs
├── layout/
│   ├── mod.rs
│   ├── layout.rs
│   ├── region.rs
│   └── splitter.rs
├── renderables/
│   ├── mod.rs
│   ├── table.rs
│   ├── panel.rs
│   └── ...
├── markup/
│   ├── mod.rs
│   └── parser.rs
├── logging/
│   ├── mod.rs
│   └── handler.rs
└── macros.rs (e.g., console_print!)
```

## 9. Logging Integration

> Reference: `RICH_SPEC.md` Section 18

Rust has two major logging ecosystems: `log` (simple facade) and `tracing` (async-aware, structured). We'll support both.

### 9.1 Architecture Overview

```
┌─────────────────┐     ┌─────────────────┐
│  log::Log       │     │ tracing::Layer  │
│  (log crate)    │     │ (tracing-sub.)  │
└────────┬────────┘     └────────┬────────┘
         │                       │
         └───────────┬───────────┘
                     ▼
              ┌──────────────┐
              │  RichHandler │
              │  (common)    │
              └──────┬───────┘
                     ▼
              ┌──────────────┐
              │   Console    │
              │   output     │
              └──────────────┘
```

### 9.2 Configuration

```rust
/// Configuration for Rich logging
#[derive(Clone)]
pub struct RichLogConfig {
    // Display columns
    pub show_time: bool,           // Show timestamp column (default: true)
    pub show_level: bool,          // Show log level column (default: true)
    pub show_target: bool,         // Show target/module (default: true)
    pub show_file: bool,           // Show file:line (default: true)
    pub enable_link_path: bool,    // Terminal hyperlinks (default: true)

    // Formatting
    pub time_format: TimeFormat,   // strftime or callback (default: "[%H:%M:%S]")
    pub omit_repeated_times: bool, // Skip duplicate times (default: true)
    pub level_width: usize,        // Level column width (default: 5)

    // Message styling
    pub markup: bool,              // Parse Rich markup (default: false)
    pub highlighter: Option<Box<dyn Highlighter>>,  // Message highlighter
    pub keywords: Vec<String>,     // Keywords to highlight (default: HTTP methods)

    // Tracebacks (tracing-error integration)
    pub rich_tracebacks: bool,     // Enable SpanTrace rendering (default: false)
    pub tracebacks_show_locals: bool,  // Show local variables (default: false)
}

impl Default for RichLogConfig {
    fn default() -> Self {
        Self {
            show_time: true,
            show_level: true,
            show_target: true,
            show_file: true,
            enable_link_path: true,
            time_format: TimeFormat::Strftime("[%H:%M:%S]".into()),
            omit_repeated_times: true,
            level_width: 5,
            markup: false,
            highlighter: None,
            keywords: vec![
                "GET", "POST", "PUT", "DELETE", "PATCH", "HEAD", "OPTIONS"
            ].into_iter().map(String::from).collect(),
            rich_tracebacks: false,
            tracebacks_show_locals: false,
        }
    }
}

pub enum TimeFormat {
    Strftime(String),
    Callback(Box<dyn Fn(&DateTime<Local>) -> Text + Send + Sync>),
}
```

### 9.3 Level Styling

```rust
/// Style names for log levels (theme-defined)
fn level_style(level: Level) -> &'static str {
    match level {
        Level::Error => "logging.level.error",     // Red, bold
        Level::Warn  => "logging.level.warning",   // Yellow
        Level::Info  => "logging.level.info",      // Green
        Level::Debug => "logging.level.debug",     // Blue, dim
        Level::Trace => "logging.level.trace",     // Dim
    }
}

fn format_level(level: Level, width: usize) -> Text {
    let name = match level {
        Level::Error => "ERROR",
        Level::Warn  => "WARN",
        Level::Info  => "INFO",
        Level::Debug => "DEBUG",
        Level::Trace => "TRACE",
    };
    Text::styled(format!("{:width$}", name), level_style(level))
}
```

### 9.4 Log Record Rendering

```rust
/// Renders a log record as a grid table row
pub struct LogRender {
    config: RichLogConfig,
    last_time: Option<Text>,
}

impl LogRender {
    pub fn render(
        &mut self,
        console: &Console,
        timestamp: DateTime<Local>,
        level: Level,
        target: &str,
        file: Option<&str>,
        line: Option<u32>,
        message: Text,
    ) -> Table {
        let mut grid = Table::grid().padding((0, 1));
        grid.expand = true;

        // Add columns based on config
        if self.config.show_time {
            grid.add_column(Column::new().style("log.time"));
        }
        if self.config.show_level {
            grid.add_column(Column::new()
                .style("log.level")
                .width(self.config.level_width));
        }
        grid.add_column(Column::new()
            .ratio(1)
            .style("log.message")
            .overflow(Overflow::Fold));
        if self.config.show_file && file.is_some() {
            grid.add_column(Column::new().style("log.path"));
        }

        // Build row
        let mut row = Vec::new();

        if self.config.show_time {
            let time_text = self.format_time(&timestamp);
            if self.config.omit_repeated_times && Some(&time_text) == self.last_time.as_ref() {
                row.push(Text::new(" ".repeat(time_text.len())));
            } else {
                self.last_time = Some(time_text.clone());
                row.push(time_text);
            }
        }

        if self.config.show_level {
            row.push(format_level(level, self.config.level_width));
        }

        row.push(message);

        if self.config.show_file {
            if let Some(f) = file {
                row.push(self.format_path(f, line));
            }
        }

        grid.add_row(row);
        grid
    }

    fn format_time(&self, dt: &DateTime<Local>) -> Text {
        match &self.config.time_format {
            TimeFormat::Strftime(fmt) => Text::new(dt.format(fmt).to_string()),
            TimeFormat::Callback(f) => f(dt),
        }
    }

    fn format_path(&self, file: &str, line: Option<u32>) -> Text {
        let mut text = Text::new();
        let filename = Path::new(file).file_name()
            .map(|s| s.to_string_lossy())
            .unwrap_or_default();

        if self.config.enable_link_path {
            text.append(&filename, Style::link(format!("file://{}", file)));
        } else {
            text.append(&filename, Style::default());
        }

        if let Some(n) = line {
            text.append(":", Style::default());
            if self.config.enable_link_path {
                text.append(&n.to_string(), Style::link(format!("file://{}#{}", file, n)));
            } else {
                text.append(&n.to_string(), Style::default());
            }
        }
        text
    }
}
```

### 9.5 Message Processing

```rust
impl LogRender {
    fn process_message(&self, msg: &str) -> Text {
        // 1. Parse markup if enabled
        let mut text = if self.config.markup {
            Text::from_markup(msg)
        } else {
            Text::new(msg)
        };

        // 2. Apply highlighter
        if let Some(h) = &self.config.highlighter {
            text = h.highlight(text);
        }

        // 3. Highlight keywords
        if !self.config.keywords.is_empty() {
            text.highlight_words(&self.config.keywords, "logging.keyword");
        }

        text
    }
}
```

### 9.6 Integration with `log` Crate

```rust
use log::{Log, Record, Level, Metadata, SetLoggerError};
use std::sync::Mutex;

pub struct RichLogger {
    console: Console,
    config: RichLogConfig,
    render: Mutex<LogRender>,
    level: Level,
}

impl RichLogger {
    pub fn new(console: Console, config: RichLogConfig) -> Self {
        Self {
            console,
            render: Mutex::new(LogRender::new(config.clone())),
            config,
            level: Level::Info,
        }
    }

    pub fn with_level(mut self, level: Level) -> Self {
        self.level = level;
        self
    }

    /// Install as the global logger
    pub fn install(self) -> Result<(), SetLoggerError> {
        let level = self.level;
        log::set_boxed_logger(Box::new(self))?;
        log::set_max_level(level.to_level_filter());
        Ok(())
    }
}

impl Log for RichLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= self.level
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let message = self.render.lock().unwrap()
            .process_message(&record.args().to_string());

        let table = self.render.lock().unwrap().render(
            &self.console,
            Local::now(),
            record.level(),
            record.target(),
            record.file(),
            record.line(),
            message,
        );

        // Thread-safe print
        let _ = self.console.print(&table);
    }

    fn flush(&self) {
        let _ = self.console.flush();
    }
}
```

### 9.7 Integration with `tracing` Crate

```rust
use tracing::{Event, Subscriber, span};
use tracing_subscriber::{Layer, layer::Context, registry::LookupSpan};

pub struct RichLayer {
    console: Console,
    config: RichLogConfig,
    render: Mutex<LogRender>,
}

impl<S> Layer<S> for RichLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        // Extract fields from event
        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);

        let level = match *event.metadata().level() {
            tracing::Level::ERROR => Level::Error,
            tracing::Level::WARN => Level::Warn,
            tracing::Level::INFO => Level::Info,
            tracing::Level::DEBUG => Level::Debug,
            tracing::Level::TRACE => Level::Trace,
        };

        let message = self.render.lock().unwrap()
            .process_message(&visitor.message);

        let table = self.render.lock().unwrap().render(
            &self.console,
            Local::now(),
            level,
            event.metadata().target(),
            event.metadata().file(),
            event.metadata().line(),
            message,
        );

        let _ = self.console.print(&table);
    }
}

#[derive(Default)]
struct FieldVisitor {
    message: String,
    fields: Vec<(String, String)>,
}

impl tracing::field::Visit for FieldVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{:?}", value);
        } else {
            self.fields.push((field.name().to_string(), format!("{:?}", value)));
        }
    }
}
```

### 9.8 Macro Helpers

```rust
/// Initialize Rich logging with defaults
#[macro_export]
macro_rules! init_rich_logging {
    () => {
        RichLogger::new(Console::stdout(), RichLogConfig::default())
            .install()
            .expect("Failed to install logger")
    };
    ($level:expr) => {
        RichLogger::new(Console::stdout(), RichLogConfig::default())
            .with_level($level)
            .install()
            .expect("Failed to install logger")
    };
}

/// Configure Rich logging for tracing
pub fn init_rich_tracing() {
    use tracing_subscriber::prelude::*;

    tracing_subscriber::registry()
        .with(RichLayer::new(Console::stdout(), RichLogConfig::default()))
        .init();
}
```

### 9.9 Usage Examples

```rust
// Basic log crate usage
use log::{info, warn, error};
use rich_rust::logging::{RichLogger, RichLogConfig};

fn main() {
    RichLogger::new(Console::stdout(), RichLogConfig::default())
        .with_level(log::Level::Debug)
        .install()
        .unwrap();

    info!("Server starting...");
    info!("GET /api/users 200 OK");
    warn!("High memory usage detected");
    error!("Connection refused");
}

// Output:
// [12:34:56] INFO  Server starting...                    main.rs:8
//           INFO  GET /api/users 200 OK                  main.rs:9
// [12:34:57] WARN  High memory usage detected            main.rs:10
//           ERROR Connection refused                     main.rs:11

// With tracing
use tracing::{info, instrument};

#[instrument]
fn process_request(id: u32) {
    info!(request_id = id, "Processing request");
}

fn main() {
    init_rich_tracing();
    process_request(42);
}
```

### 9.10 Theme Integration

Default styles in theme:

```rust
impl Theme {
    fn logging_styles() -> HashMap<&'static str, Style> {
        hashmap! {
            "log.time" => Style::dim(),
            "log.level" => Style::default(),
            "log.message" => Style::default(),
            "log.path" => Style::dim(),
            "logging.level.error" => Style::new().red().bold(),
            "logging.level.warning" => Style::new().yellow(),
            "logging.level.info" => Style::new().green(),
            "logging.level.debug" => Style::new().blue().dim(),
            "logging.level.trace" => Style::dim(),
            "logging.keyword" => Style::new().yellow().bold(),
        }
    }
}
```

### 9.11 Error Handling with tracing-error

```rust
use tracing_error::{SpanTrace, ErrorLayer};

/// Rich-formatted error with span trace
pub struct RichError {
    source: Box<dyn std::error::Error + Send + Sync>,
    span_trace: SpanTrace,
}

impl RichError {
    pub fn render(&self, console: &Console) -> Text {
        let mut text = Text::new();

        // Error message
        text.append(&self.source.to_string(), Style::new().red().bold());
        text.append("\n\n", Style::default());

        // Span trace (similar to Python traceback)
        text.append("Trace:\n", Style::dim());
        for span in self.span_trace.iter() {
            text.append(&format!("  {} at {}:{}\n",
                span.name(),
                span.file().unwrap_or("?"),
                span.line().unwrap_or(0)
            ), Style::dim());
        }

        text
    }
}
```

### 9.12 Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Dual support | Both log and tracing | Cover all use cases |
| Thread safety | Mutex around LogRender | Simple, correct; contention unlikely |
| Keyword highlighting | Opt-in | Avoid false positives in structured logs |
| Time omission | Match Python behavior | Cleaner output for rapid logs |
| Hyperlinks | Default on | Modern terminals support it |
| Tracebacks | Via tracing-error | Native Rust span traces |

## 10. HTML/SVG Export

> Reference: `RICH_SPEC.md` Section 19

Export console output to static HTML and SVG for documentation, sharing, and embedding.

### 10.1 Architecture Overview

```
┌────────────────┐
│ Console        │ record=true
│ (record_buffer)│
└───────┬────────┘
        │ Vec<Segment>
        ▼
┌────────────────┐
│ Segment        │
│ Processing     │ filter_control, simplify
└───────┬────────┘
        │
   ┌────┴─────┐
   ▼          ▼
┌──────┐  ┌──────┐
│ HTML │  │ SVG  │
│ Exp. │  │ Exp. │
└──────┘  └──────┘
```

### 10.2 Terminal Theme

```rust
/// Color palette for export rendering
#[derive(Clone, Debug)]
pub struct TerminalTheme {
    pub background: ColorTriplet,
    pub foreground: ColorTriplet,
    pub ansi_colors: [ColorTriplet; 16],  // Standard + bright ANSI
}

impl TerminalTheme {
    pub fn new(
        background: (u8, u8, u8),
        foreground: (u8, u8, u8),
        normal: [(u8, u8, u8); 8],
        bright: Option<[(u8, u8, u8); 8]>,
    ) -> Self {
        let bright = bright.unwrap_or(normal);
        let mut ansi_colors = [ColorTriplet::default(); 16];
        for (i, c) in normal.iter().enumerate() {
            ansi_colors[i] = ColorTriplet::from(*c);
        }
        for (i, c) in bright.iter().enumerate() {
            ansi_colors[i + 8] = ColorTriplet::from(*c);
        }
        Self {
            background: ColorTriplet::from(background),
            foreground: ColorTriplet::from(foreground),
            ansi_colors,
        }
    }

    /// Resolve a Rich Color to RGB using this theme
    pub fn resolve_color(&self, color: &Color, foreground: bool) -> ColorTriplet {
        match color.color_type {
            ColorType::Default => {
                if foreground { self.foreground } else { self.background }
            }
            ColorType::Standard(n) | ColorType::Windows(n) => {
                self.ansi_colors[n as usize % 16]
            }
            ColorType::EightBit(n) => {
                EIGHT_BIT_PALETTE[n as usize]  // Standard 256-color lookup
            }
            ColorType::TrueColor(r, g, b) => {
                ColorTriplet::new(r, g, b)
            }
        }
    }
}

// Built-in themes
pub static DEFAULT_TERMINAL_THEME: Lazy<TerminalTheme> = Lazy::new(|| {
    TerminalTheme::new(
        (255, 255, 255), (0, 0, 0),
        LIGHT_NORMAL, Some(LIGHT_BRIGHT)
    )
});

pub static SVG_EXPORT_THEME: Lazy<TerminalTheme> = Lazy::new(|| {
    TerminalTheme::new(
        (41, 41, 41), (197, 200, 198),
        DARK_NORMAL, Some(DARK_BRIGHT)
    )
});

pub static MONOKAI: Lazy<TerminalTheme> = Lazy::new(|| { /* ... */ });
pub static DIMMED_MONOKAI: Lazy<TerminalTheme> = Lazy::new(|| { /* ... */ });
pub static NIGHT_OWLISH: Lazy<TerminalTheme> = Lazy::new(|| { /* ... */ });
```

### 10.3 Style to CSS Conversion

```rust
impl Style {
    /// Convert to CSS rules for HTML export
    pub fn to_css(&self, theme: &TerminalTheme) -> String {
        let mut rules = Vec::new();

        let (fg, bg) = if self.reverse {
            (self.bgcolor.as_ref(), self.color.as_ref())
        } else {
            (self.color.as_ref(), self.bgcolor.as_ref())
        };

        // Foreground color (with dim handling)
        if let Some(color) = fg {
            let mut rgb = theme.resolve_color(color, true);
            if self.dim {
                rgb = blend_rgb(&rgb, &theme.background, 0.5);
            }
            rules.push(format!("color: {}", rgb.hex()));
            rules.push(format!("text-decoration-color: {}", rgb.hex()));
        }

        // Background color
        if let Some(color) = bg {
            let rgb = theme.resolve_color(color, false);
            rules.push(format!("background-color: {}", rgb.hex()));
        }

        // Text attributes
        if self.bold { rules.push("font-weight: bold".into()); }
        if self.italic { rules.push("font-style: italic".into()); }
        if self.underline { rules.push("text-decoration: underline".into()); }
        if self.strike { rules.push("text-decoration: line-through".into()); }
        if self.overline { rules.push("text-decoration: overline".into()); }

        rules.join("; ")
    }

    /// Convert to CSS rules for SVG export (uses fill instead of color)
    pub fn to_svg_css(&self, theme: &TerminalTheme) -> String {
        let mut rules = Vec::new();

        let (fg, bg) = if self.reverse {
            (self.bgcolor.as_ref(), self.color.as_ref())
        } else {
            (self.color.as_ref(), self.bgcolor.as_ref())
        };

        // Fill color (SVG equivalent of color)
        let mut rgb = fg.map(|c| theme.resolve_color(c, true))
            .unwrap_or(theme.foreground);
        if self.dim {
            let bg_rgb = bg.map(|c| theme.resolve_color(c, false))
                .unwrap_or(theme.background);
            rgb = blend_rgb(&rgb, &bg_rgb, 0.4);
        }
        rules.push(format!("fill: {}", rgb.hex()));

        // Text attributes
        if self.bold { rules.push("font-weight: bold".into()); }
        if self.italic { rules.push("font-style: italic".into()); }
        if self.underline { rules.push("text-decoration: underline".into()); }
        if self.strike { rules.push("text-decoration: line-through".into()); }

        rules.join(";")
    }
}

fn blend_rgb(fg: &ColorTriplet, bg: &ColorTriplet, factor: f64) -> ColorTriplet {
    let blend = |a: u8, b: u8| -> u8 {
        ((a as f64) * (1.0 - factor) + (b as f64) * factor) as u8
    };
    ColorTriplet::new(
        blend(fg.red, bg.red),
        blend(fg.green, bg.green),
        blend(fg.blue, bg.blue),
    )
}
```

### 10.4 HTML Export

```rust
/// Configuration for HTML export
#[derive(Clone)]
pub struct HtmlExportConfig {
    pub theme: TerminalTheme,
    pub inline_styles: bool,    // Inline vs stylesheet (default: false)
    pub template: Option<String>, // Custom HTML template
}

impl Default for HtmlExportConfig {
    fn default() -> Self {
        Self {
            theme: DEFAULT_TERMINAL_THEME.clone(),
            inline_styles: false,
            template: None,
        }
    }
}

pub struct HtmlExporter<'a> {
    config: HtmlExportConfig,
    segments: &'a [Segment],
}

impl<'a> HtmlExporter<'a> {
    pub fn new(segments: &'a [Segment], config: HtmlExportConfig) -> Self {
        Self { config, segments }
    }

    pub fn export(&self) -> String {
        let processed = Segment::simplify(
            Segment::filter_control(self.segments.iter().cloned())
        ).collect::<Vec<_>>();

        if self.config.inline_styles {
            self.export_inline(&processed)
        } else {
            self.export_stylesheet(&processed)
        }
    }

    fn export_inline(&self, segments: &[Segment]) -> String {
        let mut html = String::new();
        for seg in segments {
            let escaped = html_escape(&seg.text);
            if let Some(style) = &seg.style {
                let css = style.to_css(&self.config.theme);
                if let Some(link) = &style.link {
                    html.push_str(&format!(
                        r#"<a href="{}" style="{}">{}</a>"#,
                        html_escape(link), css, escaped
                    ));
                } else if !css.is_empty() {
                    html.push_str(&format!(
                        r#"<span style="{}">{}</span>"#, css, escaped
                    ));
                } else {
                    html.push_str(&escaped);
                }
            } else {
                html.push_str(&escaped);
            }
        }
        self.wrap_html(html, "")
    }

    fn export_stylesheet(&self, segments: &[Segment]) -> String {
        let mut html = String::new();
        let mut styles: HashMap<String, usize> = HashMap::new();

        for seg in segments {
            let escaped = html_escape(&seg.text);
            if let Some(style) = &seg.style {
                let css = style.to_css(&self.config.theme);
                let class_num = *styles.entry(css.clone())
                    .or_insert_with(|| styles.len() + 1);

                if let Some(link) = &style.link {
                    html.push_str(&format!(
                        r#"<a class="r{}" href="{}">{}</a>"#,
                        class_num, html_escape(link), escaped
                    ));
                } else {
                    html.push_str(&format!(
                        r#"<span class="r{}">{}</span>"#,
                        class_num, escaped
                    ));
                }
            } else {
                html.push_str(&escaped);
            }
        }

        let stylesheet = styles.iter()
            .map(|(css, num)| format!(".r{} {{ {} }}", num, css))
            .collect::<Vec<_>>()
            .join("\n");

        self.wrap_html(html, &stylesheet)
    }

    fn wrap_html(&self, code: String, stylesheet: &str) -> String {
        let template = self.config.template.as_deref()
            .unwrap_or(DEFAULT_HTML_TEMPLATE);

        template
            .replace("{code}", &code)
            .replace("{stylesheet}", stylesheet)
            .replace("{foreground}", &self.config.theme.foreground.hex())
            .replace("{background}", &self.config.theme.background.hex())
    }
}

const DEFAULT_HTML_TEMPLATE: &str = r#"<!DOCTYPE html>
<html>
<head>
<meta charset="UTF-8">
<style>
{stylesheet}
body {
    color: {foreground};
    background-color: {background};
}
</style>
</head>
<body>
    <pre style="font-family:Menlo,'DejaVu Sans Mono',consolas,'Courier New',monospace">
        <code style="font-family:inherit">{code}</code>
    </pre>
</body>
</html>"#;

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
```

### 10.5 SVG Export

```rust
/// Configuration for SVG export
#[derive(Clone)]
pub struct SvgExportConfig {
    pub theme: TerminalTheme,
    pub title: String,
    pub font_aspect_ratio: f64,   // Width/height ratio (default: 0.61 for Fira Code)
    pub char_height: f64,         // Character height in pixels (default: 20)
    pub template: Option<String>, // Custom SVG template
}

impl Default for SvgExportConfig {
    fn default() -> Self {
        Self {
            theme: SVG_EXPORT_THEME.clone(),
            title: "Rich".into(),
            font_aspect_ratio: 0.61,
            char_height: 20.0,
            template: None,
        }
    }
}

pub struct SvgExporter<'a> {
    config: SvgExportConfig,
    segments: &'a [Segment],
    width: usize,  // Console width
}

impl<'a> SvgExporter<'a> {
    pub fn new(segments: &'a [Segment], width: usize, config: SvgExportConfig) -> Self {
        Self { config, segments, width }
    }

    pub fn export(&self) -> String {
        let processed = Segment::filter_control(self.segments.iter().cloned())
            .collect::<Vec<_>>();

        let char_height = self.config.char_height;
        let char_width = char_height * self.config.font_aspect_ratio;
        let line_height = char_height * 1.22;

        // Layout constants
        const MARGIN: f64 = 1.0;
        const PADDING_TOP: f64 = 40.0;
        const PADDING_SIDE: f64 = 8.0;

        // Generate unique ID from content hash
        let unique_id = self.compute_unique_id();

        // Process segments into positioned text elements
        let mut text_elements = Vec::new();
        let mut background_rects = Vec::new();
        let mut styles: HashMap<String, usize> = HashMap::new();
        let mut y = 0usize;

        for line in Segment::split_and_crop_lines(&processed, self.width) {
            let mut x = 0usize;
            for seg in line {
                let style = seg.style.as_ref().cloned().unwrap_or_default();
                let css = style.to_svg_css(&self.config.theme);
                let class_num = *styles.entry(css.clone())
                    .or_insert_with(|| styles.len() + 1);

                // Background rectangle
                if style.bgcolor.is_some() || style.reverse {
                    let bg_color = if style.reverse {
                        style.color.as_ref()
                            .map(|c| self.config.theme.resolve_color(c, true))
                            .unwrap_or(self.config.theme.foreground)
                    } else {
                        style.bgcolor.as_ref()
                            .map(|c| self.config.theme.resolve_color(c, false))
                            .unwrap_or(self.config.theme.background)
                    };

                    background_rects.push(format!(
                        r#"<rect fill="{}" x="{}" y="{}" width="{}" height="{}" shape-rendering="crispEdges"/>"#,
                        bg_color.hex(),
                        x as f64 * char_width,
                        y as f64 * line_height + 1.5,
                        char_width * seg.text.len() as f64,
                        line_height + 0.25
                    ));
                }

                // Text element (skip whitespace-only)
                if seg.text.trim().len() > 0 {
                    text_elements.push(format!(
                        r#"<text class="{}-r{}" x="{}" y="{}" textLength="{}">{}</text>"#,
                        unique_id,
                        class_num,
                        x as f64 * char_width,
                        y as f64 * line_height + char_height,
                        char_width * seg.text.len() as f64,
                        svg_escape(&seg.text)
                    ));
                }

                x += cell_len(&seg.text);
            }
            y += 1;
        }

        // Build stylesheet
        let styles_css = styles.iter()
            .map(|(css, num)| format!(".{}-r{} {{ {} }}", unique_id, num, css))
            .collect::<Vec<_>>()
            .join("\n");

        // Calculate dimensions
        let terminal_width = char_width * self.width as f64 + PADDING_SIDE * 2.0;
        let terminal_height = line_height * y as f64 + PADDING_TOP + PADDING_SIDE;
        let total_width = terminal_width + MARGIN * 2.0;
        let total_height = terminal_height + MARGIN * 2.0;

        // Build chrome (window decoration)
        let chrome = self.build_chrome(terminal_width, terminal_height, &unique_id);

        // Assemble SVG
        let template = self.config.template.as_deref()
            .unwrap_or(DEFAULT_SVG_TEMPLATE);

        template
            .replace("{unique_id}", &unique_id)
            .replace("{width}", &total_width.to_string())
            .replace("{height}", &total_height.to_string())
            .replace("{char_height}", &char_height.to_string())
            .replace("{line_height}", &line_height.to_string())
            .replace("{terminal_width}", &(char_width * self.width as f64).to_string())
            .replace("{terminal_height}", &(line_height * y as f64).to_string())
            .replace("{terminal_x}", &(MARGIN + PADDING_SIDE).to_string())
            .replace("{terminal_y}", &(MARGIN + PADDING_TOP).to_string())
            .replace("{styles}", &styles_css)
            .replace("{chrome}", &chrome)
            .replace("{backgrounds}", &background_rects.join("\n"))
            .replace("{matrix}", &text_elements.join("\n"))
    }

    fn compute_unique_id(&self) -> String {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for seg in self.segments {
            seg.text.hash(&mut hasher);
        }
        self.config.title.hash(&mut hasher);
        format!("terminal-{}", hasher.finish())
    }

    fn build_chrome(&self, width: f64, height: f64, unique_id: &str) -> String {
        let bg = self.config.theme.background.hex();
        let fg = self.config.theme.foreground.hex();

        format!(
            r#"<rect fill="{bg}" stroke="rgba(255,255,255,0.35)" stroke-width="1" x="1" y="1" width="{width}" height="{height}" rx="8"/>
<text class="{unique_id}-title" fill="{fg}" text-anchor="middle" x="{title_x}" y="26">{title}</text>
<g transform="translate(26,22)">
<circle cx="0" cy="0" r="7" fill="#ff5f57"/>
<circle cx="22" cy="0" r="7" fill="#febc2e"/>
<circle cx="44" cy="0" r="7" fill="#28c840"/>
</g>"#,
            bg = bg,
            width = width,
            height = height,
            unique_id = unique_id,
            fg = fg,
            title_x = width / 2.0,
            title = svg_escape(&self.config.title)
        )
    }
}

fn svg_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace(' ', "&#160;")  // Non-breaking space
}

const DEFAULT_SVG_TEMPLATE: &str = r#"<svg class="rich-terminal" viewBox="0 0 {width} {height}" xmlns="http://www.w3.org/2000/svg">
<style>
@font-face {
    font-family: "Fira Code";
    src: local("FiraCode-Regular"),
         url("https://cdnjs.cloudflare.com/ajax/libs/firacode/6.2.0/woff2/FiraCode-Regular.woff2") format("woff2");
    font-weight: 400;
}
.{unique_id}-matrix {
    font-family: Fira Code, monospace;
    font-size: {char_height}px;
    line-height: {line_height}px;
}
.{unique_id}-title {
    font-size: 18px;
    font-weight: bold;
    font-family: arial;
}
{styles}
</style>
<defs>
<clipPath id="{unique_id}-clip">
  <rect x="0" y="0" width="{terminal_width}" height="{terminal_height}" />
</clipPath>
</defs>
{chrome}
<g transform="translate({terminal_x}, {terminal_y})" clip-path="url(#{unique_id}-clip)">
{backgrounds}
<g class="{unique_id}-matrix">
{matrix}
</g>
</g>
</svg>"#;
```

### 10.6 Console Integration

```rust
impl Console {
    /// Export recorded output as HTML
    pub fn export_html(&self, config: HtmlExportConfig) -> Result<String, ExportError> {
        let buffer = self.record_buffer.read()
            .map_err(|_| ExportError::LockFailed)?;

        if buffer.is_empty() {
            return Err(ExportError::NoRecordedContent);
        }

        let exporter = HtmlExporter::new(&buffer, config);
        Ok(exporter.export())
    }

    /// Save recorded output as HTML file
    pub fn save_html(&self, path: impl AsRef<Path>, config: HtmlExportConfig) -> Result<(), ExportError> {
        let html = self.export_html(config)?;
        std::fs::write(path, html)?;
        Ok(())
    }

    /// Export recorded output as SVG
    pub fn export_svg(&self, config: SvgExportConfig) -> Result<String, ExportError> {
        let buffer = self.record_buffer.read()
            .map_err(|_| ExportError::LockFailed)?;

        if buffer.is_empty() {
            return Err(ExportError::NoRecordedContent);
        }

        let exporter = SvgExporter::new(&buffer, self.width(), config);
        Ok(exporter.export())
    }

    /// Save recorded output as SVG file
    pub fn save_svg(&self, path: impl AsRef<Path>, config: SvgExportConfig) -> Result<(), ExportError> {
        let svg = self.export_svg(config)?;
        std::fs::write(path, svg)?;
        Ok(())
    }

    /// Clear the record buffer
    pub fn clear_record_buffer(&self) {
        if let Ok(mut buffer) = self.record_buffer.write() {
            buffer.clear();
        }
    }
}

#[derive(Debug)]
pub enum ExportError {
    NoRecordedContent,
    LockFailed,
    IoError(std::io::Error),
}

impl From<std::io::Error> for ExportError {
    fn from(e: std::io::Error) -> Self {
        ExportError::IoError(e)
    }
}
```

### 10.7 Module Layout

```
src/export/
├── mod.rs           // Re-exports
├── theme.rs         // TerminalTheme + built-in themes
├── html.rs          // HtmlExporter, HtmlExportConfig
├── svg.rs           // SvgExporter, SvgExportConfig
└── templates/
    ├── html.html    // DEFAULT_HTML_TEMPLATE
    └── svg.svg      // DEFAULT_SVG_TEMPLATE
```

### 10.8 Usage Examples

```rust
use rich_rust::Console;
use rich_rust::export::{HtmlExportConfig, SvgExportConfig, MONOKAI};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create console with recording enabled
    let console = Console::builder()
        .record(true)
        .build();

    // Print styled content
    console.print(Panel::new("Hello, World!"))?;
    console.print(Table::from_data(&data))?;

    // Export as HTML with inline styles
    let html = console.export_html(HtmlExportConfig {
        inline_styles: true,
        ..Default::default()
    })?;

    // Export as SVG with custom theme
    let svg = console.export_svg(SvgExportConfig {
        theme: MONOKAI.clone(),
        title: "My Terminal Output".into(),
        ..Default::default()
    })?;

    // Save to files
    console.save_html("output.html", HtmlExportConfig::default())?;
    console.save_svg("output.svg", SvgExportConfig::default())?;

    Ok(())
}
```

### 10.9 Design Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Separate configs | HtmlExportConfig / SvgExportConfig | Different use cases and options |
| Template strings | Include templates as const strs | Compile-time embedding, no runtime loading |
| Style deduplication | HashMap-based class generation | Smaller output, efficient |
| SVG unique IDs | Content hash | Allows multiple SVGs on one page |
| Font loading | CDN fallback for Fira Code | Works without local font install |
| Escape functions | Minimal, focused | Security without over-escaping |
