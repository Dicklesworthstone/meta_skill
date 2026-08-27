//! # `rich_rust`
//!
//! A Rust port of Python's Rich library for beautiful terminal output.
//!
//! This library provides an abstraction over ANSI escape codes to render styled text,
//! tables, panels, progress bars, trees, and more in the terminal.
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use rich_rust::prelude::*;
//!
//! let console = Console::new();
//! console.print("[bold red]Hello[/] [green]World[/]!");
//! ```
//!
//! ## Core Concepts
//!
//! - **[`Console`]**: The central entry point for printing styled output. Handles color
//!   detection, terminal dimensions, and ANSI code generation.
//! - **[`Style`]**: Visual attributes including foreground/background colors, text
//!   decorations (bold, italic, underline, etc.), and hyperlinks.
//! - **[`Text`]**: Rich text with overlapping style spans that can be justified, wrapped,
//!   and rendered to the terminal.
//! - **[`Segment`]**: The atomic rendering unit combining text content with a style.
//! - **[`Color`]**: Terminal colors supporting 4-bit ANSI, 8-bit (256), and 24-bit
//!   true color with automatic downgrading.
//!
//! ## Renderables
//!
//! The library provides several high-level renderables for structured output:
//!
//! - **[`renderables::Table`]**: Display data in rows and columns with borders
//! - **[`renderables::Panel`]**: Frame content with a title and border
//! - **[`renderables::Tree`]**: Hierarchical data with guide lines
//! - **[`renderables::ProgressBar`]**: Visual progress indicators
//! - **[`renderables::Rule`]**: Horizontal divider lines
//! - **[`renderables::Columns`]**: Multi-column text layout
//!
//! ## Markup Syntax
//!
//! The Console supports a simple markup syntax for inline styling:
//!
//! ```rust,ignore
//! // Basic styling
//! console.print("[bold]Bold text[/bold]");
//! console.print("[italic red]Red italic[/]");  // [/] closes any open tag
//!
//! // Colors
//! console.print("[green]Green[/] [#ff8800]Orange[/] [rgb(100,150,200)]Blue[/]");
//!
//! // Combinations
//! console.print("[bold underline magenta on white]Styled text[/]");
//!
//! // Hyperlinks
//! console.print("[link=https://example.com]Click here[/link]");
//! ```
//!
//! ## Features
//!
//! Optional features can be enabled in `Cargo.toml`:
//!
//! - **`syntax`**: Syntax highlighting for source code via syntect
//! - **`markdown`**: Markdown rendering via pulldown-cmark
//! - **`json`**: JSON formatting with syntax highlighting
//! - **`tracing`**: Tracing integration via `RichTracingLayer`
//!
//! ```toml
//! [dependencies]
//! rich_rust = { version = "0.1", features = ["syntax", "markdown", "json"] }
//! ```
//!
//! ## Thread Safety
//!
//! All public types in `rich_rust` are `Send + Sync`. You can safely:
//!
//! - Share a [`Console`] between threads via `Arc<Console>`
//! - Call [`Console::print`], [`Console::log`], etc. from multiple threads
//! - Use [`live::Live`] displays with concurrent updates
//! - Use [`interactive::Status`] spinners from any thread
//!
//! ### Mutex Poison Recovery
//!
//! `rich_rust` uses **poison recovery** for all internal mutexes. If a thread panics
//! while holding a lock, subsequent operations will continue rather than propagate
//! the panic. This is appropriate because:
//!
//! 1. **Caches**: Style/cell/color caches can safely use potentially stale data
//! 2. **Output buffers**: A corrupted buffer produces garbled output, not crashes
//! 3. **Configuration**: Theme/options are read-mostly and self-healing
//!
//! See the [`sync`] module for details on the poison recovery strategy.
//!
//! ### Caveats
//!
//! - Output may interleave when printing from multiple threads simultaneously
//! - Style/color parsing caches are thread-safe but not deterministic under contention
//! - For deterministic output ordering, synchronize at the application level
//!
//! ## Examples
//!
//! ### Styled Text
//!
//! ```rust,ignore
//! use rich_rust::prelude::*;
//!
//! // Build text programmatically
//! let mut text = Text::new("Hello, ");
//! text.append("World", Style::new().bold().color(Color::parse("red").unwrap()));
//! text.append("!", Style::new().italic());
//!
//! let console = Console::new();
//! console.print_text(&text);
//! ```
//!
//! ### Tables
//!
//! ```rust,ignore
//! use rich_rust::prelude::*;
//!
//! let table = Table::new()
//!     .add_column(Column::new("Name").style(Style::new().bold()))
//!     .add_column(Column::new("Age").justify(JustifyMethod::Right))
//!     .add_row(Row::new().cell("Alice").cell("30"))
//!     .add_row(Row::new().cell("Bob").cell("25"));
//!
//! console.print_renderable(&table);
//! ```
//!
//! ### Panels
//!
//! ```rust,ignore
//! use rich_rust::prelude::*;
//!
//! let panel = Panel::new("Content inside a box")
//!     .title("My Panel")
//!     .border_style(Style::new().color(Color::parse("blue").unwrap()));
//!
//! console.print_renderable(&panel);
//! ```
//!
//! ### Trees
//!
//! ```rust,ignore
//! use rich_rust::prelude::*;
//!
//! let tree = Tree::new(
//!     TreeNode::new("Project")
//!         .child(TreeNode::new("src")
//!             .child(TreeNode::new("main.rs"))
//!             .child(TreeNode::new("lib.rs")))
//!         .child(TreeNode::new("Cargo.toml")),
//! );
//!
//! console.print_renderable(&tree);
//! ```

#![forbid(unsafe_code)]
#![warn(clippy::pedantic)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::struct_excessive_bools)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::unused_self)]
#![allow(clippy::too_many_lines)]

pub mod ansi;
pub mod r#box;
pub mod cells;
pub mod color;
pub mod console;
pub mod emoji;
pub mod filesize;
pub mod highlighter;
pub mod interactive;
pub mod live;
pub mod logging;
pub mod markup;
pub mod measure;
pub mod protocol;
pub mod renderables;
pub mod segment;
pub mod style;
pub mod sync;
pub mod terminal;
pub mod text;
pub mod theme;

/// Re-exports for convenient usage
pub mod prelude {
    pub use crate::ansi::AnsiDecoder;
    pub use crate::r#box::BoxChars;
    pub use crate::color::{
        Color, ColorSystem, ColorTriplet, ColorType, DEFAULT_TERMINAL_THEME, DIMMED_MONOKAI,
        MONOKAI, NIGHT_OWLISH, SVG_EXPORT_THEME, TerminalTheme,
    };
    pub use crate::console::{
        CONSOLE_HTML_FORMAT, CONSOLE_SVG_FORMAT, Console, ConsoleOptions, ExportHtmlOptions,
        ExportSvgOptions, LogLevel, LogOptions,
    };
    pub use crate::emoji::EmojiVariant;
    pub use crate::filesize::{
        SizeUnit, binary, binary_speed, binary_with_precision, decimal, decimal_speed,
        decimal_with_precision, format_size, format_speed,
    };
    pub use crate::highlighter::{Highlighter, NullHighlighter, RegexHighlighter, ReprHighlighter};
    pub use crate::interactive::{Pager, Prompt, PromptError, Status};
    pub use crate::live::{Live, LiveOptions, VerticalOverflowMethod};
    pub use crate::logging::RichLogger;
    #[cfg(feature = "tracing")]
    pub use crate::logging::RichTracingLayer;
    pub use crate::measure::Measurement;
    pub use crate::protocol::{RichCast, RichCastOutput, rich_cast};
    pub use crate::renderables::{
        Align, AlignLines, AlignMethod, BarStyle, Cell, Column, Columns, Constrain, Control,
        DownloadColumn, Emoji, FileSizeColumn, Inspect, InspectOptions, Layout, LayoutSplitter,
        PaddingDimensions, Panel, Pretty, PrettyOptions, ProgressBar, Region, Row, Rule, Spinner,
        Table, TotalFileSizeColumn, Traceback, TracebackFrame, TransferSpeedColumn, Tree,
        TreeGuides, TreeNode, VerticalAlign, VerticalAlignMethod, align_text, inspect,
        print_exception,
    };
    pub use crate::segment::{
        ControlCode, ControlType, Segment, escape_control_codes, strip_control_codes,
    };
    pub use crate::style::{Attributes, Style};
    pub use crate::text::{JustifyMethod, OverflowMethod, Span, Text};
    pub use crate::theme::{Theme, ThemeError, ThemeStackError};

    #[cfg(feature = "syntax")]
    pub use crate::renderables::{Syntax, SyntaxError};

    #[cfg(feature = "markdown")]
    pub use crate::renderables::Markdown;

    #[cfg(feature = "json")]
    pub use crate::renderables::{Json, JsonError, JsonTheme};
}

// Re-export key types at crate root
pub use color::{
    Color, ColorSystem, ColorTriplet, ColorType, DEFAULT_TERMINAL_THEME, DIMMED_MONOKAI, MONOKAI,
    NIGHT_OWLISH, SVG_EXPORT_THEME, TerminalTheme,
};
pub use console::Console;
pub use console::{CONSOLE_HTML_FORMAT, CONSOLE_SVG_FORMAT, ExportHtmlOptions, ExportSvgOptions};
pub use live::{Live, LiveOptions, VerticalOverflowMethod};
pub use logging::RichLogger;
#[cfg(feature = "tracing")]
pub use logging::RichTracingLayer;
pub use renderables::{Layout, LayoutSplitter, Region};
pub use segment::Segment;
pub use style::{Attributes, Style};
pub use text::{Span, Text};
pub use theme::{Theme, ThemeError, ThemeStackError};
