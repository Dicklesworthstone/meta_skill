//! Markdown rendering for the terminal.
//!
//! This module provides markdown rendering using pulldown-cmark for parsing
//! and converting to styled terminal output. It supports the full `CommonMark`
//! specification plus GitHub Flavored Markdown extensions.
//!
//! # Feature Flag
//!
//! This module requires the `markdown` feature to be enabled:
//!
//! ```toml
//! [dependencies]
//! rich_rust = { version = "0.1", features = ["markdown"] }
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
//! Enabling this feature adds the [`pulldown-cmark`](https://docs.rs/pulldown-cmark) crate
//! as a dependency for Markdown parsing.
//!
//! # Basic Usage
//!
//! ```rust,ignore
//! use rich_rust::renderables::markdown::Markdown;
//!
//! let md = Markdown::new("# Hello\n\nThis is **bold** and *italic*.");
//! let segments = md.render(80);
//! ```
//!
//! # Supported Markdown Features
//!
//! - **Headings**: H1-H6 with distinct styles
//! - **Emphasis**: *italic*, **bold**, ~~strikethrough~~
//! - **Code**: `inline code` and fenced code blocks
//! - **Lists**: Ordered (1. 2. 3.) and unordered (- * +)
//! - **Task lists**: GitHub-style `- [ ]` and `- [x]` with checkbox rendering
//! - **Links**: `[text](url)` with optional URL display
//! - **Blockquotes**: `> quoted text`
//! - **Tables**: GitHub Flavored Markdown tables with alignment
//! - **Horizontal rules**: `---` or `***`
//!
//! # Customizing Styles
//!
//! All element styles can be customized via builder methods:
//!
//! ```rust,ignore
//! use rich_rust::renderables::markdown::Markdown;
//! use rich_rust::style::Style;
//!
//! let md = Markdown::new("# Custom Styled Heading")
//!     .h1_style(Style::new().bold().color_str("bright_magenta").unwrap())
//!     .h2_style(Style::new().bold().color_str("magenta").unwrap())
//!     .emphasis_style(Style::new().italic().color_str("yellow").unwrap())
//!     .strong_style(Style::new().bold().color_str("red").unwrap())
//!     .code_style(Style::new().bgcolor_str("bright_black").unwrap())
//!     .link_style(Style::new().underline().color_str("blue").unwrap())
//!     .quote_style(Style::new().italic().color_str("bright_black").unwrap());
//!
//! let segments = md.render(80);
//! ```
//!
//! # List Customization
//!
//! ```rust,ignore
//! use rich_rust::renderables::markdown::Markdown;
//!
//! let md = Markdown::new("- Item 1\n- Item 2")
//!     .bullet_char('→')  // Custom bullet character
//!     .list_indent(4);   // 4-space indent for nested lists
//! ```
//!
//! # Link Display
//!
//! ```rust,ignore
//! use rich_rust::renderables::markdown::Markdown;
//!
//! // Python Rich behavior:
//! // - `hyperlinks=true` (default): render the link text only, with an OSC8 hyperlink.
//! // - `hyperlinks=false`: render `text (url)` with a styled URL suffix (no OSC8).
//! let md = Markdown::new("[Click here](https://example.com)")
//!     .hyperlinks(true);
//! ```
//!
//! # Known Limitations
//!
//! - **Images**: Rendered as an emoji + alt text. With `hyperlinks=true`, the alt text is an OSC8 hyperlink.
//! - **HTML**: Inline HTML is ignored
//! - **Footnotes**: Supported by the parser; rendering is minimal and may differ from Python Rich
//! - **Task lists**: GitHub-style task lists (`- [ ]` / `- [x]`) render as checkboxes
//! - **Fenced code blocks**: Language hints are parsed, but `rich_rust` renders fenced blocks as styled text
//!   unless the `syntax` feature is enabled and a language is present (then it renders via `Syntax`).
//! - **Hyperlinks**: Use `.hyperlinks(false)` to disable OSC8 and show a URL suffix (Python Rich-compatible).

use std::fmt::Write;

use crate::cells;
use crate::segment::Segment;
use crate::style::Style;

use pulldown_cmark::{Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

#[cfg(feature = "syntax")]
use crate::renderables::Syntax;

/// A markdown document that can be rendered to the terminal.
#[derive(Debug, Clone)]
pub struct Markdown {
    /// The markdown source text.
    source: String,
    /// Style for H1 headings.
    h1_style: Style,
    /// Style for H2 headings.
    h2_style: Style,
    /// Style for H3 headings.
    h3_style: Style,
    /// Style for H4-H6 headings.
    h4_style: Style,
    /// Style for emphasis (italic).
    emphasis_style: Style,
    /// Style for strong emphasis (bold).
    strong_style: Style,
    /// Style for strikethrough text.
    strikethrough_style: Style,
    /// Style for inline code.
    code_style: Style,
    /// Style for code blocks.
    code_block_style: Style,
    /// Style for links.
    link_style: Style,
    /// Style for link text when OSC8 hyperlinks are disabled (`hyperlinks=false`).
    link_text_style: Style,
    /// Style for blockquotes.
    quote_style: Style,
    /// Style for table headers.
    table_header_style: Style,
    /// Style for table borders.
    table_border_style: Style,
    /// Character for bullet points.
    bullet_char: char,
    /// Indent for nested lists.
    list_indent: usize,
    /// Whether to emit OSC8 hyperlinks for links and images.
    hyperlinks: bool,
}

impl Default for Markdown {
    fn default() -> Self {
        Self {
            source: String::new(),
            h1_style: Style::new()
                .bold()
                .underline()
                .color_str("bright_cyan")
                .unwrap_or_default(),
            h2_style: Style::new().bold().color_str("cyan").unwrap_or_default(),
            h3_style: Style::new().bold().color_str("blue").unwrap_or_default(),
            h4_style: Style::new()
                .bold()
                .color_str("bright_blue")
                .unwrap_or_default(),
            emphasis_style: Style::new().italic(),
            strong_style: Style::new().bold(),
            strikethrough_style: Style::new().strike(),
            code_style: Style::new()
                .color_str("bright_magenta")
                .unwrap_or_default()
                .bgcolor_str("bright_black")
                .unwrap_or_default(),
            code_block_style: Style::new()
                .color_str("white")
                .unwrap_or_default()
                .bgcolor_str("bright_black")
                .unwrap_or_default(),
            link_style: Style::new()
                .color_str("blue")
                .unwrap_or_default()
                .underline(),
            link_text_style: Style::new().color_str("bright_blue").unwrap_or_default(),
            quote_style: Style::new()
                .italic()
                .color_str("bright_black")
                .unwrap_or_default(),
            table_header_style: Style::new()
                .bold()
                .color_str("bright_white")
                .unwrap_or_default(),
            table_border_style: Style::new().color_str("bright_black").unwrap_or_default(),
            bullet_char: '•',
            list_indent: 2,
            hyperlinks: true,
        }
    }
}

impl Markdown {
    /// Create a new Markdown document.
    #[must_use]
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            ..Default::default()
        }
    }

    /// Set the style for H1 headings.
    #[must_use]
    pub fn h1_style(mut self, style: Style) -> Self {
        self.h1_style = style;
        self
    }

    /// Set the style for H2 headings.
    #[must_use]
    pub fn h2_style(mut self, style: Style) -> Self {
        self.h2_style = style;
        self
    }

    /// Set the style for H3 headings.
    #[must_use]
    pub fn h3_style(mut self, style: Style) -> Self {
        self.h3_style = style;
        self
    }

    /// Set the style for H4-H6 headings.
    #[must_use]
    pub fn h4_style(mut self, style: Style) -> Self {
        self.h4_style = style;
        self
    }

    /// Set the style for emphasis (italic).
    #[must_use]
    pub fn emphasis_style(mut self, style: Style) -> Self {
        self.emphasis_style = style;
        self
    }

    /// Set the style for strong emphasis (bold).
    #[must_use]
    pub fn strong_style(mut self, style: Style) -> Self {
        self.strong_style = style;
        self
    }

    /// Set the style for inline code.
    #[must_use]
    pub fn code_style(mut self, style: Style) -> Self {
        self.code_style = style;
        self
    }

    /// Set the style for code blocks.
    #[must_use]
    pub fn code_block_style(mut self, style: Style) -> Self {
        self.code_block_style = style;
        self
    }

    /// Set the style for links.
    #[must_use]
    pub fn link_style(mut self, style: Style) -> Self {
        self.link_style = style;
        self
    }

    /// Set the style for blockquotes.
    #[must_use]
    pub fn quote_style(mut self, style: Style) -> Self {
        self.quote_style = style;
        self
    }

    /// Set the style for table headers.
    #[must_use]
    pub fn table_header_style(mut self, style: Style) -> Self {
        self.table_header_style = style;
        self
    }

    /// Set the style for table borders.
    #[must_use]
    pub fn table_border_style(mut self, style: Style) -> Self {
        self.table_border_style = style;
        self
    }

    /// Set the bullet character for unordered lists.
    #[must_use]
    pub fn bullet_char(mut self, c: char) -> Self {
        self.bullet_char = c;
        self
    }

    /// Set the indent for nested lists.
    #[must_use]
    pub fn list_indent(mut self, indent: usize) -> Self {
        self.list_indent = indent;
        self
    }

    /// Enable or disable OSC8 hyperlinks.
    ///
    /// Matches Python Rich's `Markdown(..., hyperlinks=...)` behavior:
    /// - `true` (default): emit OSC8 hyperlinks and do not append ` (url)` for links.
    /// - `false`: do not emit OSC8; render `text (url)` for links.
    #[must_use]
    pub fn hyperlinks(mut self, enabled: bool) -> Self {
        self.hyperlinks = enabled;
        self
    }

    /// Render the markdown to segments.
    #[must_use]
    #[allow(clippy::too_many_lines)]
    pub fn render(&self, max_width: usize) -> Vec<Segment<'_>> {
        let mut segments = Vec::new();
        let mut style_stack: Vec<Style> = Vec::new();
        let mut list_stack: Vec<(bool, usize)> = Vec::new(); // (is_ordered, item_number)
        let mut list_item_prefix_len: Vec<usize> = Vec::new();
        let mut list_item_first_paragraph: Vec<bool> = Vec::new();
        let mut list_item_prefix_pending = false;
        let mut in_code_block = false;
        let mut code_block_text = String::new();
        let mut code_block_language = String::new();
        let mut code_block_use_syntax = false;
        let mut code_block_style_pushed = false;
        let mut in_blockquote = false;
        let mut blockquote_prefix_pending = false;
        let mut blockquote_first_paragraph = false;
        let mut current_link_url = String::new();
        let mut image_style_pushed = false;

        // Table state
        let mut in_table = false;
        let mut table_alignments: Vec<Alignment> = Vec::new();
        let mut table_rows: Vec<Vec<String>> = Vec::new();
        let mut current_row: Vec<String> = Vec::new();
        let mut current_cell_content = String::new();
        let mut in_table_head = false;
        let mut header_row = None;

        let options = Options::ENABLE_STRIKETHROUGH
            | Options::ENABLE_TABLES
            | Options::ENABLE_FOOTNOTES
            | Options::ENABLE_TASKLISTS;

        let parser = Parser::new_ext(&self.source, options);

        let combined_style = |stack: &[Style]| -> Option<Style> {
            if stack.is_empty() {
                return None;
            }
            let mut combined = Style::new();
            for style in stack {
                combined = combined.combine(style);
            }
            Some(combined)
        };

        let parse_fence_language = |info: &str| -> String {
            info.split_whitespace()
                .next()
                .unwrap_or("")
                .trim()
                .to_string()
        };

        // Helper macros to inline prefix logic (avoids borrow issues with closures)
        macro_rules! ensure_blockquote_prefix {
            ($segs:expr) => {
                if in_blockquote && blockquote_prefix_pending {
                    $segs.push(Segment::new("│ ", Some(self.quote_style.clone())));
                    blockquote_prefix_pending = false;
                }
            };
        }

        macro_rules! ensure_list_prefix {
            ($segs:expr) => {
                if list_item_prefix_pending {
                    if let Some(prefix_len) = list_item_prefix_len.last() {
                        if *prefix_len > 0 {
                            $segs.push(Segment::new(" ".repeat(*prefix_len), None));
                        }
                    }
                    list_item_prefix_pending = false;
                }
            };
        }

        for event in parser {
            match event {
                Event::Start(tag) => {
                    match tag {
                        Tag::Heading { level, .. } => {
                            // Add newline before heading if not at start
                            if !segments.is_empty() {
                                segments.push(Segment::new("\n\n", None));
                            }
                            let style = match level {
                                HeadingLevel::H1 => self.h1_style.clone(),
                                HeadingLevel::H2 => self.h2_style.clone(),
                                HeadingLevel::H3 => self.h3_style.clone(),
                                _ => self.h4_style.clone(),
                            };
                            style_stack.push(style);
                        }
                        Tag::Paragraph => {
                            if in_blockquote {
                                if !blockquote_first_paragraph {
                                    segments.push(Segment::new("\n", None));
                                }
                                blockquote_prefix_pending = true;
                                blockquote_first_paragraph = false;
                                if let Some(first) = list_item_first_paragraph.last_mut() {
                                    if !*first {
                                        list_item_prefix_pending = true;
                                    }
                                    *first = false;
                                }
                            } else if !segments.is_empty() && !in_table {
                                if let Some(first) = list_item_first_paragraph.last_mut() {
                                    if !*first {
                                        segments.push(Segment::new("\n", None));
                                        list_item_prefix_pending = true;
                                    }
                                    *first = false;
                                } else {
                                    segments.push(Segment::new("\n\n", None));
                                }
                            }
                        }
                        Tag::Emphasis => {
                            style_stack.push(self.emphasis_style.clone());
                        }
                        Tag::Strong => {
                            style_stack.push(self.strong_style.clone());
                        }
                        Tag::Strikethrough => {
                            style_stack.push(self.strikethrough_style.clone());
                        }
                        Tag::CodeBlock(kind) => {
                            in_code_block = true;
                            code_block_text.clear();
                            code_block_language.clear();
                            code_block_style_pushed = false;

                            if !segments.is_empty() {
                                segments.push(Segment::new("\n", None));
                            }

                            // If this is a fenced code block with a language info string, and the
                            // `syntax` feature is enabled, render through `Syntax` for parity with
                            // Python Rich Markdown.
                            if let CodeBlockKind::Fenced(info) = kind {
                                code_block_language = parse_fence_language(info.as_ref());
                            }
                            code_block_use_syntax =
                                cfg!(feature = "syntax") && !code_block_language.is_empty();

                            if !code_block_use_syntax {
                                style_stack.push(self.code_block_style.clone());
                                code_block_style_pushed = true;
                            }
                        }
                        Tag::Link { dest_url, .. } => {
                            current_link_url = dest_url.to_string();
                            if self.hyperlinks {
                                style_stack
                                    .push(self.link_style.clone().link(current_link_url.clone()));
                            } else {
                                style_stack.push(self.link_text_style.clone());
                            }
                        }
                        Tag::Image { dest_url, .. } => {
                            // Python Rich renders images as an emoji + alt text, optionally linked.
                            ensure_blockquote_prefix!(segments);
                            ensure_list_prefix!(segments);
                            segments.push(Segment::new("🌆 ", None));
                            image_style_pushed = false;
                            if self.hyperlinks {
                                style_stack.push(Style::new().link(dest_url.to_string()));
                                image_style_pushed = true;
                            }
                        }
                        Tag::BlockQuote(_) => {
                            in_blockquote = true;
                            blockquote_first_paragraph = true;
                            blockquote_prefix_pending = true;
                            if !segments.is_empty() {
                                segments.push(Segment::new("\n", None));
                            }
                            style_stack.push(self.quote_style.clone());
                        }
                        Tag::List(start_num) => {
                            if !segments.is_empty() {
                                segments.push(Segment::new("\n", None));
                            }
                            let is_ordered = start_num.is_some();
                            #[allow(clippy::cast_possible_truncation)]
                            let start = start_num.unwrap_or(1) as usize;
                            list_stack.push((is_ordered, start));
                        }
                        Tag::Item => {
                            ensure_blockquote_prefix!(segments);
                            // Add indent based on list nesting
                            let indent_len = list_stack.len() * self.list_indent;
                            let indent = " ".repeat(indent_len);
                            segments.push(Segment::new(indent, None));

                            if let Some((is_ordered, num)) = list_stack.last_mut() {
                                if *is_ordered {
                                    let marker = format!("{num}. ");
                                    let marker_len = cells::cell_len(&marker);
                                    segments.push(Segment::new(marker, None));
                                    list_item_prefix_len.push(indent_len + marker_len);
                                    *num += 1;
                                } else {
                                    let marker = format!("{} ", self.bullet_char);
                                    let marker_len = cells::cell_len(&marker);
                                    segments.push(Segment::new(marker, None));
                                    list_item_prefix_len.push(indent_len + marker_len);
                                }
                            }
                            list_item_first_paragraph.push(true);
                        }
                        Tag::Table(alignments) => {
                            in_table = true;
                            table_alignments.clone_from(&alignments);
                            table_rows.clear();
                            header_row = None;
                            if !segments.is_empty() {
                                segments.push(Segment::new("\n", None));
                            }
                        }
                        Tag::TableHead => {
                            in_table_head = true;
                            current_row.clear();
                        }
                        Tag::TableRow => {
                            current_row.clear();
                        }
                        Tag::TableCell => {
                            current_cell_content.clear();
                        }
                        _ => {}
                    }
                }
                Event::End(tag_end) => {
                    match tag_end {
                        TagEnd::Heading(_) => {
                            style_stack.pop();
                        }
                        TagEnd::Paragraph => {}
                        TagEnd::Emphasis | TagEnd::Strong | TagEnd::Strikethrough => {
                            style_stack.pop();
                        }
                        TagEnd::CodeBlock => {
                            in_code_block = false;

                            if code_block_use_syntax {
                                // Render syntax-highlighted code block (plain layout parity; ANSI differs).
                                #[cfg(feature = "syntax")]
                                {
                                    let lang = if code_block_language.is_empty() {
                                        String::from("text")
                                    } else {
                                        std::mem::take(&mut code_block_language)
                                    };

                                    // Python Rich Markdown always enables `Syntax(word_wrap=True, padding=1)`.
                                    // Model `word_wrap=True` by setting `word_wrap(Some(max_width))` so Syntax
                                    // uses the available console width (it will clamp internally as needed).
                                    let syntax = Syntax::new(code_block_text.clone(), lang)
                                        .word_wrap(Some(max_width))
                                        .padding(1, 1);

                                    // If the lexer name is unknown, fall back to plain text but keep the
                                    // same layout/padding.
                                    let mut syntax_segments: Vec<Segment<'static>> =
                                        if let Ok(segs) = syntax.render(Some(max_width)) {
                                            segs.into_iter().map(Segment::into_owned).collect()
                                        } else {
                                            let fallback =
                                                Syntax::new(code_block_text.clone(), "text")
                                                    .word_wrap(Some(max_width))
                                                    .padding(1, 1);
                                            fallback
                                                .render(Some(max_width))
                                                .unwrap_or_default()
                                                .into_iter()
                                                .map(Segment::into_owned)
                                                .collect()
                                        };

                                    // Ensure blockquote/list indentation prefixes are applied per line,
                                    // using the same mechanism as the non-syntax code block path.
                                    let needs_quote_prefix = in_blockquote;
                                    let needs_list_prefix = !list_item_prefix_len.is_empty();
                                    let mut at_line_start = true;
                                    for seg in syntax_segments.drain(..) {
                                        if at_line_start {
                                            blockquote_prefix_pending = needs_quote_prefix;
                                            list_item_prefix_pending = needs_list_prefix;
                                            ensure_blockquote_prefix!(segments);
                                            ensure_list_prefix!(segments);
                                            at_line_start = false;
                                        }
                                        let is_newline = seg.text == "\n";
                                        segments.push(seg);
                                        if is_newline {
                                            at_line_start = true;
                                        }
                                    }

                                    // If we're still inside a blockquote/list, the next block should
                                    // start with the appropriate prefix.
                                    if in_blockquote {
                                        blockquote_prefix_pending = true;
                                    }
                                    if !list_item_prefix_len.is_empty() {
                                        list_item_prefix_pending = true;
                                    }
                                }
                                #[cfg(not(feature = "syntax"))]
                                {
                                    // Unreachable due to `code_block_use_syntax` gating above.
                                }
                            } else {
                                let current_style = combined_style(&style_stack);
                                for line in code_block_text.lines() {
                                    ensure_blockquote_prefix!(segments);
                                    ensure_list_prefix!(segments);
                                    segments.push(Segment::new(
                                        format!("  {line}"),
                                        current_style.clone(),
                                    ));
                                    segments.push(Segment::new("\n", None));
                                    if in_blockquote {
                                        blockquote_prefix_pending = true;
                                    }
                                    if !list_item_prefix_len.is_empty() {
                                        list_item_prefix_pending = true;
                                    }
                                }
                                if code_block_style_pushed {
                                    style_stack.pop();
                                }
                                code_block_language.clear();
                            }
                        }
                        TagEnd::Link => {
                            style_stack.pop();
                            if !self.hyperlinks && !current_link_url.is_empty() && !in_table {
                                segments.push(Segment::new(" (", None));
                                segments.push(Segment::new(
                                    current_link_url.clone(),
                                    Some(self.link_style.clone()),
                                ));
                                segments.push(Segment::new(")", None));
                            }
                            current_link_url.clear();
                        }
                        TagEnd::Image => {
                            if image_style_pushed {
                                style_stack.pop();
                            }
                            image_style_pushed = false;
                        }
                        TagEnd::BlockQuote(_) => {
                            in_blockquote = false;
                            blockquote_prefix_pending = false;
                            blockquote_first_paragraph = false;
                            style_stack.pop();
                        }
                        TagEnd::List(_) => {
                            list_stack.pop();
                        }
                        TagEnd::Item => {
                            segments.push(Segment::new("\n", None));
                            list_item_prefix_len.pop();
                            list_item_first_paragraph.pop();
                            list_item_prefix_pending = false;
                            if in_blockquote {
                                blockquote_prefix_pending = true;
                            }
                        }
                        TagEnd::Table => {
                            // Render the collected table
                            self.render_table(
                                &mut segments,
                                header_row.as_ref(),
                                &table_rows,
                                &table_alignments,
                            );
                            in_table = false;
                            table_rows.clear();
                            header_row = None;
                        }
                        TagEnd::TableHead => {
                            in_table_head = false;
                            header_row = Some(std::mem::take(&mut current_row));
                        }
                        TagEnd::TableRow => {
                            if !in_table_head {
                                table_rows.push(std::mem::take(&mut current_row));
                            }
                        }
                        TagEnd::TableCell => {
                            current_row.push(std::mem::take(&mut current_cell_content));
                        }
                        _ => {}
                    }
                }
                Event::Text(text) => {
                    if in_table {
                        current_cell_content.push_str(&text.replace('\n', " "));
                    } else {
                        let current_style = combined_style(&style_stack);
                        if in_code_block {
                            // Preserve code block formatting (defer emission until TagEnd::CodeBlock so
                            // we can render fenced blocks via Syntax when appropriate).
                            code_block_text.push_str(&text);
                        } else {
                            ensure_blockquote_prefix!(segments);
                            ensure_list_prefix!(segments);
                            segments.push(Segment::new(text.to_string(), current_style));
                        }
                    }
                }
                Event::Code(code) => {
                    if in_table {
                        let _ = write!(current_cell_content, "`{}`", code.replace('\n', " "));
                    } else {
                        ensure_blockquote_prefix!(segments);
                        ensure_list_prefix!(segments);
                        segments.push(Segment::new(
                            format!(" {code} "),
                            Some(self.code_style.clone()),
                        ));
                    }
                }
                Event::SoftBreak => {
                    if in_table {
                        current_cell_content.push(' ');
                    } else {
                        segments.push(Segment::new(" ", None));
                    }
                }
                Event::HardBreak => {
                    if in_table {
                        current_cell_content.push(' ');
                    } else {
                        segments.push(Segment::new("\n", None));
                        if in_blockquote {
                            blockquote_prefix_pending = true;
                        }
                        if !list_item_prefix_len.is_empty() {
                            list_item_prefix_pending = true;
                        }
                    }
                }
                Event::Rule => {
                    let rule_width = if max_width > 0 { max_width } else { 40 };
                    let rule_width = rule_width.max(1);
                    segments.push(Segment::new("\n", None));
                    segments.push(Segment::new(
                        "─".repeat(rule_width),
                        Some(Style::new().color_str("bright_black").unwrap_or_default()),
                    ));
                    segments.push(Segment::new("\n", None));
                }
                Event::TaskListMarker(checked) => {
                    // Render checkbox for task list items
                    // This event comes right after Start(Tag::Item), so the bullet is already rendered
                    let checkbox = if checked { "☑ " } else { "☐ " };
                    let style = if checked {
                        Style::new().color_str("green").unwrap_or_default()
                    } else {
                        Style::new().color_str("bright_black").unwrap_or_default()
                    };
                    segments.push(Segment::new(checkbox.to_string(), Some(style)));
                }
                _ => {}
            }
        }

        if max_width > 0 {
            pad_segments_to_width(segments, max_width)
        } else {
            segments
        }
    }

    /// Render a table to segments.
    fn render_table(
        &self,
        segments: &mut Vec<Segment>,
        header: Option<&Vec<String>>,
        rows: &[Vec<String>],
        alignments: &[Alignment],
    ) {
        // Calculate column widths
        let num_cols = header.map_or_else(|| rows.first().map_or(0, Vec::len), Vec::len);

        if num_cols == 0 {
            return;
        }

        let mut col_widths = vec![0usize; num_cols];

        // Measure header
        if let Some(hdr) = header {
            for (i, cell) in hdr.iter().enumerate() {
                if i < col_widths.len() {
                    col_widths[i] = col_widths[i].max(cells::cell_len(cell));
                }
            }
        }

        // Measure rows
        for row in rows {
            for (i, cell) in row.iter().enumerate() {
                if i < col_widths.len() {
                    col_widths[i] = col_widths[i].max(cells::cell_len(cell));
                }
            }
        }

        // Ensure minimum width
        for w in &mut col_widths {
            *w = (*w).max(3);
        }

        let border_style = Some(self.table_border_style.clone());

        // Helper to render a horizontal border
        let render_border =
            |segs: &mut Vec<Segment>, left: &str, mid: &str, right: &str, style: Option<Style>| {
                segs.push(Segment::new(left.to_string(), style.clone()));
                for (i, &width) in col_widths.iter().enumerate() {
                    segs.push(Segment::new("─".repeat(width + 2), style.clone()));
                    if i < col_widths.len() - 1 {
                        segs.push(Segment::new(mid.to_string(), style.clone()));
                    }
                }
                segs.push(Segment::new(right.to_string(), style));
                segs.push(Segment::new("\n", None));
            };

        // Helper to render a row
        let render_row =
            |segs: &mut Vec<Segment>, cells: &[String], style: Option<Style>, is_header: bool| {
                segs.push(Segment::new("│", border_style.clone()));
                for (i, width) in col_widths.iter().enumerate() {
                    let content = cells.get(i).map_or("", String::as_str);
                    let alignment = alignments.get(i).copied().unwrap_or(Alignment::None);
                    let padded = Self::pad_cell(content, *width, alignment);
                    segs.push(Segment::new(" ", None));
                    if is_header {
                        segs.push(Segment::new(padded, Some(self.table_header_style.clone())));
                    } else {
                        segs.push(Segment::new(padded, style.clone()));
                    }
                    segs.push(Segment::new(" ", None));
                    segs.push(Segment::new("│", border_style.clone()));
                }
                segs.push(Segment::new("\n", None));
            };

        // Top border
        render_border(segments, "┌", "┬", "┐", border_style.clone());

        // Header row
        if let Some(hdr) = header {
            render_row(segments, hdr, None, true);
            // Header separator
            render_border(segments, "├", "┼", "┤", border_style.clone());
        }

        // Data rows
        for row in rows {
            render_row(segments, row, None, false);
        }

        // Bottom border
        render_border(segments, "└", "┴", "┘", border_style);
    }

    /// Pad a cell's content according to alignment.
    fn pad_cell(content: &str, width: usize, alignment: Alignment) -> String {
        let content_len = cells::cell_len(content);
        if content_len >= width {
            return content.to_string();
        }

        let padding = width - content_len;
        match alignment {
            Alignment::Left | Alignment::None => {
                format!("{content}{}", " ".repeat(padding))
            }
            Alignment::Right => {
                format!("{}{content}", " ".repeat(padding))
            }
            Alignment::Center => {
                let left_pad = padding / 2;
                let right_pad = padding - left_pad;
                format!("{}{content}{}", " ".repeat(left_pad), " ".repeat(right_pad))
            }
        }
    }

    /// Get the source markdown text.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }
}

fn pad_segments_to_width(segments: Vec<Segment<'_>>, width: usize) -> Vec<Segment<'_>> {
    let mut padded = Vec::new();
    let mut line_width = 0usize;

    for segment in segments {
        if segment.is_control() {
            padded.push(segment);
            continue;
        }

        let style = segment.style.clone();
        let text = segment.text;
        let text_ref = text.as_ref();
        let mut start = 0usize;

        for (idx, ch) in text_ref.char_indices() {
            if ch == '\n' {
                let part = &text_ref[start..idx];
                if !part.is_empty() {
                    padded.push(Segment::new(part.to_string(), style.clone()));
                    line_width += cells::cell_len(part);
                }
                if line_width < width {
                    padded.push(Segment::new(" ".repeat(width - line_width), None));
                }
                padded.push(Segment::line());
                line_width = 0;
                start = idx + 1;
            }
        }

        let tail = &text_ref[start..];
        if !tail.is_empty() {
            padded.push(Segment::new(tail.to_string(), style));
            line_width += cells::cell_len(tail);
        }
    }

    if line_width > 0 {
        if line_width < width {
            padded.push(Segment::new(" ".repeat(width - line_width), None));
        }
        padded.push(Segment::line());
    }

    padded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Attributes;

    #[test]
    fn test_markdown_new() {
        let md = Markdown::new("# Hello");
        assert_eq!(md.source(), "# Hello");
    }

    #[test]
    fn test_markdown_builder() {
        let md = Markdown::new("test")
            .bullet_char('*')
            .list_indent(4)
            .hyperlinks(false);
        assert_eq!(md.bullet_char, '*');
        assert_eq!(md.list_indent, 4);
        assert!(!md.hyperlinks);
    }

    #[test]
    fn test_render_heading() {
        let md = Markdown::new("# Title");
        let segments = md.render(80);
        assert!(!segments.is_empty());
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("Title"));
    }

    #[test]
    fn test_render_multiple_headings() {
        let md = Markdown::new("# H1\n## H2\n### H3");
        let segments = md.render(80);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("H1"));
        assert!(text.contains("H2"));
        assert!(text.contains("H3"));
    }

    #[test]
    fn test_render_emphasis() {
        let md = Markdown::new("This is *italic* and **bold**.");
        let segments = md.render(80);
        assert!(!segments.is_empty());
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("italic"));
        assert!(text.contains("bold"));
    }

    #[test]
    fn test_render_nested_emphasis_combines_styles() {
        let md = Markdown::new("**bold *italic***");
        let segments = md.render(80);
        let italic_segment = segments
            .iter()
            .find(|seg| seg.text.contains("italic"))
            .expect("missing italic segment");
        let style = italic_segment
            .style
            .as_ref()
            .expect("missing style for italic segment");
        assert!(style.attributes.contains(Attributes::BOLD));
        assert!(style.attributes.contains(Attributes::ITALIC));
    }

    #[test]
    fn test_render_code() {
        let md = Markdown::new("Use `inline code` here.");
        let segments = md.render(80);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("inline code"));
    }

    #[test]
    fn test_render_code_block() {
        let md = Markdown::new("```rust\nfn main() {}\n```");
        let segments = md.render(80);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("fn main"));
    }

    #[test]
    fn test_render_unordered_list() {
        let md = Markdown::new("- Item 1\n- Item 2\n- Item 3");
        let segments = md.render(80);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("Item 1"));
        assert!(text.contains("Item 2"));
        assert!(text.contains("•")); // Default bullet
    }

    #[test]
    fn test_render_ordered_list() {
        let md = Markdown::new("1. First\n2. Second\n3. Third");
        let segments = md.render(80);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("First"));
        assert!(text.contains("1."));
        assert!(text.contains("2."));
    }

    #[test]
    fn test_render_list_item_multiple_paragraphs_indent() {
        let md = Markdown::new("- First\n\n  Second");
        let segments = md.render(80);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        let lines: Vec<&str> = text.lines().filter(|line| !line.is_empty()).collect();

        assert!(lines.len() >= 2, "expected list item to render two lines");
        assert!(lines[0].contains("First"));
        assert!(lines[1].contains("Second"));
        assert!(
            !lines[1].contains('•'),
            "continuation line should not repeat bullet"
        );
        let leading_spaces = lines[1].chars().take_while(|c| *c == ' ').count();
        assert!(leading_spaces >= 2, "continuation line should be indented");
    }

    #[test]
    fn test_render_list_item_continuation_respects_marker_width() {
        let bullet = '🦀';
        let indent = 2;
        let md = Markdown::new("- First\n\n  Second")
            .bullet_char(bullet)
            .list_indent(indent);
        let segments = md.render(80);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        let lines: Vec<&str> = text.lines().filter(|line| !line.is_empty()).collect();

        assert!(lines.len() >= 2, "expected list item to render two lines");
        let marker = format!("{bullet} ");
        let expected = indent + cells::cell_len(&marker);
        let leading_spaces = lines[1].chars().take_while(|c| *c == ' ').count();
        assert_eq!(
            leading_spaces, expected,
            "continuation line should align to marker width"
        );
    }

    #[test]
    fn test_render_link() {
        let md = Markdown::new("[Click here](https://example.com)");
        let segments = md.render(80);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("Click here"));
        assert!(!text.contains("example.com"));
    }

    #[test]
    fn test_render_link_hyperlinks_disabled_shows_url_suffix() {
        let md = Markdown::new("[Click here](https://example.com)").hyperlinks(false);
        let segments = md.render(80);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("Click here"));
        assert!(text.contains("example.com"));
        assert!(text.contains(" (https://example.com)"));
    }

    #[test]
    fn test_render_blockquote() {
        let md = Markdown::new("> This is a quote");
        let segments = md.render(80);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("This is a quote"));
        assert!(text.contains("│")); // Quote prefix
    }

    #[test]
    fn test_render_blockquote_multiple_paragraphs_prefix() {
        let md = Markdown::new("> First\n>\n> Second");
        let segments = md.render(80);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        let lines: Vec<&str> = text.lines().filter(|line| !line.is_empty()).collect();

        assert!(lines.len() >= 2, "expected multiple blockquote lines");
        assert!(lines[0].starts_with("│ "));
        assert!(lines[1].starts_with("│ "));
        assert!(lines[0].contains("First"));
        assert!(lines[1].contains("Second"));
    }

    #[test]
    fn test_render_horizontal_rule() {
        let md = Markdown::new("Above\n\n---\n\nBelow");
        let segments = md.render(80);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("Above"));
        assert!(text.contains("Below"));
        assert!(text.contains("─")); // Rule character
    }

    #[test]
    fn test_render_strikethrough() {
        let md = Markdown::new("This is ~~deleted~~ text.");
        let segments = md.render(80);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("deleted"));
    }

    #[test]
    fn test_custom_bullet() {
        let md = Markdown::new("- Item").bullet_char('→');
        let segments = md.render(80);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("→"));
    }

    #[test]
    fn test_render_table() {
        let md = Markdown::new("| Name | Age |\n|------|-----|\n| Alice | 30 |\n| Bob | 25 |");
        let segments = md.render(80);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("Name"));
        assert!(text.contains("Age"));
        assert!(text.contains("Alice"));
        assert!(text.contains("Bob"));
        assert!(text.contains("30"));
        assert!(text.contains("25"));
        // Check for table border characters
        assert!(text.contains("┌")); // Top left corner
        assert!(text.contains("│")); // Vertical border
        assert!(text.contains("─")); // Horizontal border
    }

    #[test]
    fn test_render_table_unicode_width_alignment() {
        let md = Markdown::new("| A | B |\n| --- | --- |\n| 日本 | x |");
        let segments = md.render(80);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        let lines: Vec<&str> = text.lines().filter(|line| !line.is_empty()).collect();

        assert!(lines.len() >= 3, "expected table output lines");
        let expected_width = cells::cell_len(lines[0]);
        for line in lines {
            assert_eq!(
                cells::cell_len(line),
                expected_width,
                "table lines should have consistent cell width"
            );
        }
    }

    #[test]
    fn test_render_nested_list() {
        let md = Markdown::new("- Item 1\n  - Nested 1\n  - Nested 2\n- Item 2");
        let segments = md.render(80);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("Item 1"));
        assert!(text.contains("Nested 1"));
        assert!(text.contains("Nested 2"));
        assert!(text.contains("Item 2"));
    }

    #[test]
    fn test_render_task_list() {
        let md = Markdown::new("- [ ] Unchecked\n- [x] Checked\n- [ ] Another");
        let segments = md.render(80);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        // Check for task text
        assert!(text.contains("Unchecked"));
        assert!(text.contains("Checked"));
        assert!(text.contains("Another"));
        // Check for checkbox symbols
        assert!(text.contains("☐"), "unchecked box should appear");
        assert!(text.contains("☑"), "checked box should appear");
    }

    #[test]
    fn test_render_task_list_checkbox_styles() {
        let md = Markdown::new("- [x] Done task");
        let segments = md.render(80);
        // Find the checked checkbox segment
        let checkbox_segment = segments
            .iter()
            .find(|seg| seg.text.contains('☑'))
            .expect("missing checkbox segment");
        let style = checkbox_segment
            .style
            .as_ref()
            .expect("checkbox should have a style");
        // Checked boxes should be green
        assert!(style.color.is_some(), "checkbox should have a color");
    }

    #[test]
    fn test_render_task_list_mixed_with_regular() {
        let md = Markdown::new("- Regular item\n- [ ] Task item\n- Another regular");
        let segments = md.render(80);
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("Regular item"));
        assert!(text.contains("Task item"));
        assert!(text.contains("☐"), "task item should have checkbox");
    }
}
