//! Renderable components for rich terminal output.
//!
//! This module provides high-level components for structured terminal output:
//!
//! - [`Table`]: Display data in rows and columns with borders
//! - [`Panel`]: Frame content with a title and border
//! - [`Tree`]: Hierarchical data with guide lines
//! - [`ProgressBar`] / [`Spinner`]: Visual progress indicators
//! - [`Rule`]: Horizontal divider lines
//! - [`Columns`]: Multi-column text layout
//! - [`Align`]: Text alignment utilities
//! - [`Emoji`]: Single emoji renderable (Rich-style)
//! - [`Group`]: Combine multiple renderables into one
//!
//! # Examples
//!
//! ## Tables
//!
//! ```rust,ignore
//! use rich_rust::prelude::*;
//!
//! let table = Table::new()
//!     .title("Users")
//!     .add_column(Column::new("Name").style(Style::new().bold()))
//!     .add_column(Column::new("Email"))
//!     .add_row(Row::new().cell("Alice").cell("alice@example.com"))
//!     .add_row(Row::new().cell("Bob").cell("bob@example.com"));
//!
//! // Render to segments
//! for segment in table.render(80) {
//!     print!("{}", segment.text);
//! }
//! ```
//!
//! ## Panels
//!
//! ```rust,ignore
//! use rich_rust::prelude::*;
//!
//! let panel = Panel::new("Important message!")
//!     .title("Notice")
//!     .border_style(Style::new().color(Color::parse("yellow").unwrap()));
//!
//! for segment in panel.render(60) {
//!     print!("{}", segment.text);
//! }
//! ```
//!
//! ## Trees
//!
//! ```rust,ignore
//! use rich_rust::prelude::*;
//!
//! let tree = Tree::new(
//!     TreeNode::new("Root")
//!         .child(TreeNode::new("Branch A")
//!             .child(TreeNode::new("Leaf 1")))
//!         .child(TreeNode::new("Branch B")),
//! )
//! .guides(TreeGuides::Unicode);
//!
//! for segment in tree.render() {
//!     print!("{}", segment.text);
//! }
//! ```
//!
//! ## Rules (Dividers)
//!
//! ```rust,ignore
//! use rich_rust::prelude::*;
//!
//! let rule = Rule::new()
//!     .style(Style::new().color(Color::parse("blue").unwrap()));
//!
//! let titled_rule = Rule::with_title("Section Title");
//!
//! for segment in rule.render(80) {
//!     print!("{}", segment.text);
//! }
//! ```
//!
//! # Optional Features
//!
//! Additional renderables are available with feature flags:
//!
//! - **`syntax`**: [`Syntax`] - Syntax-highlighted source code
//! - **`markdown`**: [`Markdown`] - Markdown document rendering
//! - **`json`**: [`Json`] - JSON formatting with syntax highlighting

use crate::console::{Console, ConsoleOptions};
use crate::markup;
use crate::segment::Segment;
use crate::text::Text;

/// Trait for objects that can be rendered to the console.
pub trait Renderable {
    /// Render the object to a list of segments.
    fn render<'a>(&'a self, console: &Console, options: &ConsoleOptions) -> Vec<Segment<'a>>;
}

pub mod align;
pub mod columns;
pub mod constrain;
pub mod control;
pub mod emoji;
pub mod group;
pub mod layout;
pub mod padding;
pub mod panel;
pub mod pretty;
pub mod progress;
pub mod rule;
pub mod table;
pub mod traceback;
pub mod tree;

// Re-export commonly used types
pub use align::{Align, AlignLines, AlignMethod, VerticalAlignMethod, align_text};
pub use columns::Columns;
pub use constrain::Constrain;
pub use control::Control;
pub use emoji::{Emoji, NoEmoji};
pub use group::{Group, group};
pub use layout::{Layout, LayoutSplitter, Region};
pub use padding::{Padding, PaddingDimensions};
pub use panel::Panel;
pub use pretty::{Inspect, InspectOptions, Pretty, PrettyOptions, inspect};
pub use progress::{
    BarStyle, DownloadColumn, FileSizeColumn, ProgressBar, Spinner, TotalFileSizeColumn,
    TransferSpeedColumn,
};
pub use rule::Rule;
pub use table::{Cell, Column, Row, Table, VerticalAlign};
pub use traceback::{Traceback, TracebackFrame, print_exception};
pub use tree::{Tree, TreeGuides, TreeNode};

impl Renderable for str {
    fn render<'a>(&'a self, console: &Console, options: &ConsoleOptions) -> Vec<Segment<'a>> {
        let content = if console.emoji() {
            crate::emoji::replace(self, None)
        } else {
            std::borrow::Cow::Borrowed(self)
        };

        // Honor the markup setting from ConsoleOptions
        let mut text = if options.markup.unwrap_or(true) {
            markup::render_or_plain_with_style_resolver(content.as_ref(), |definition| {
                console.get_style(definition)
            })
        } else {
            Text::new(content.as_ref())
        };

        // Apply Console highlighter when enabled (parity with Python Rich's default string pipeline).
        console.apply_highlighter_to_text(options, &mut text);

        text.render("")
            .into_iter()
            .map(Segment::into_owned)
            .collect()
    }
}

impl Renderable for String {
    fn render<'a>(&'a self, console: &Console, options: &ConsoleOptions) -> Vec<Segment<'a>> {
        self.as_str().render(console, options)
    }
}

impl<T: Renderable + ?Sized> Renderable for &T {
    fn render<'a>(&'a self, console: &Console, options: &ConsoleOptions) -> Vec<Segment<'a>> {
        (*self).render(console, options)
    }
}

// Phase 3+: Syntax highlighting (requires "syntax" feature)
#[cfg(feature = "syntax")]
pub mod syntax;

#[cfg(feature = "syntax")]
pub use syntax::{Syntax, SyntaxError};

#[cfg(feature = "syntax")]
impl Renderable for Syntax {
    fn render<'a>(&'a self, _console: &Console, options: &ConsoleOptions) -> Vec<Segment<'a>> {
        self.render(Some(options.max_width))
            .unwrap_or_default()
            .into_iter()
            .map(Segment::into_owned) // Ensure static/owned segments
            .collect()
    }
}

// Phase 3+: Markdown rendering (requires "markdown" feature)
#[cfg(feature = "markdown")]
pub mod markdown;

#[cfg(feature = "markdown")]
pub use markdown::Markdown;

#[cfg(feature = "markdown")]
impl Renderable for Markdown {
    fn render<'a>(&'a self, _console: &Console, options: &ConsoleOptions) -> Vec<Segment<'a>> {
        self.render(options.max_width).into_iter().collect()
    }
}

// Phase 4: JSON rendering (requires "json" feature)
#[cfg(feature = "json")]
pub mod json;

#[cfg(feature = "json")]
pub use json::{Json, JsonError, JsonTheme};

#[cfg(feature = "json")]
impl Renderable for Json {
    fn render<'a>(&'a self, console: &Console, options: &ConsoleOptions) -> Vec<Segment<'a>> {
        // Python Rich's JSON output wraps at console width, including cases where `": "` becomes
        // a line break. We render JSON to styled segments, then run it through `Text::wrap` so the
        // wrapping behavior stays consistent with the rest of the library.
        let width = options.max_width;
        let segments = self.render_with_tab_size(console.tab_size());

        let mut text = Text::new("");
        text.tab_size = console.tab_size();
        for segment in &segments {
            if segment.is_control() {
                continue;
            }
            if let Some(style) = segment.style.as_ref() {
                text.append_styled(segment.text.as_ref(), style.clone());
            } else {
                text.append(segment.text.as_ref());
            }
        }

        let lines = text.wrap(width);
        let mut wrapped: Vec<Segment<'static>> = Vec::new();
        for (idx, line) in lines.iter().enumerate() {
            wrapped.extend(line.render("").into_iter().map(Segment::into_owned));
            if idx + 1 < lines.len() {
                wrapped.push(Segment::new("\n", None));
            }
        }

        wrapped.into_iter().map(Segment::into_owned).collect()
    }
}

#[cfg(test)]
mod tests {
    use crate::color::ColorSystem;
    use crate::console::Console;
    use crate::renderables::Renderable;

    #[test]
    fn str_renderable_applies_console_highlighter_when_enabled() {
        let console = Console::builder()
            .force_terminal(true)
            .color_system(ColorSystem::TrueColor)
            .highlight(true)
            .build();
        let options = console.options();

        let segments = "True".render(&console, &options);
        assert!(segments.iter().any(|segment| segment.style.is_some()));

        let mut buf = Vec::new();
        console
            .print_segments_to(&mut buf, &segments)
            .expect("print_segments_to failed");
        let ansi = String::from_utf8(buf).expect("utf8");
        assert!(ansi.contains("\x1b["));
    }
}
