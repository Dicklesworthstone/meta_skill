//! Syntax highlighting renderable.
//!
//! This module provides syntax highlighting for code using the syntect library.
//! It supports numerous programming languages and themes out of the box.
//!
//! # Feature Flag
//!
//! This module requires the `syntax` feature to be enabled:
//!
//! ```toml
//! [dependencies]
//! rich_rust = { version = "0.1", features = ["syntax"] }
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
//! Enabling this feature adds the [`syntect`](https://docs.rs/syntect) crate as a dependency,
//! which provides the underlying syntax definitions and theme support.
//!
//! # Basic Usage
//!
//! ```rust,ignore
//! use rich_rust::renderables::syntax::Syntax;
//!
//! // Create a syntax-highlighted code block
//! let code = r#"fn main() { println!("Hello"); }"#;
//! let syntax = Syntax::new(code, "rust");
//! let segments = syntax.render(None)?;
//! ```
//!
//! # Line Numbers and Themes
//!
//! ```rust,ignore
//! use rich_rust::renderables::syntax::Syntax;
//!
//! let code = "def hello():\n    print('world')";
//! let syntax = Syntax::new(code, "python")
//!     .line_numbers(true)
//!     .start_line(10)  // Start numbering from line 10
//!     .theme("python-rich-default");
//!
//! let segments = syntax.render(None)?;
//! ```
//!
//! # Loading from Files
//!
//! ```rust,ignore
//! use rich_rust::renderables::syntax::Syntax;
//!
//! // Auto-detect language from file extension
//! let syntax = Syntax::from_path("src/main.rs")?
//!     .line_numbers(true)
//!     .theme("InspiredGitHub");
//! ```
//!
//! # Available Themes
//!
//! Call [`Syntax::available_themes()`] to list all built-in themes. Common themes include:
//! - `python-rich-default` (default compatibility mode)
//! - `base16-ocean.light`
//! - `InspiredGitHub`
//! - `Solarized (dark)`
//! - `Solarized (light)`
//!
//! # Supported Languages
//!
//! Call [`Syntax::available_languages()`] to list all supported languages. Syntect includes
//! syntax definitions for 100+ languages including Rust, Python, JavaScript, TypeScript,
//! Go, Java, C/C++, Ruby, and many more.
//!
//! # Known Limitations
//!
//! - **Theme loading**: Custom `.tmTheme` loading is opt-in and requires reading and parsing theme
//!   files from disk. Prefer reusing loaded theme sets to avoid repeated parsing.
//! - **Syntax definitions**: Custom `.sublime-syntax` loading is opt-in and requires reading and
//!   parsing syntax definitions from disk. Prefer reusing loaded syntax sets to avoid repeated parsing.
//! - **Large files**: Rendering very large files may be slow due to per-line highlighting.
//! - **Word wrap**: Wrap is supported (use `word_wrap(Some(width))`), and is whitespace-preserving
//!   (tuned for code rather than prose reflow).

use crate::cells;
use crate::color::Color;
use crate::segment::Segment;
use crate::style::Style;
use crate::text::Text;

use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::sync::LazyLock;

use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

static SYNTAX_SET: LazyLock<SyntaxSet> = LazyLock::new(SyntaxSet::load_defaults_newlines);
static THEME_SET: LazyLock<ThemeSet> = LazyLock::new(ThemeSet::load_defaults);

const PYTHON_RICH_THEME: &str = "python-rich-default";
const PYTHON_RICH_FALLBACK_THEME: &str = "base16-ocean.dark";

/// Error type for syntax highlighting operations.
#[derive(Debug, Clone)]
pub enum SyntaxError {
    /// The specified language is not supported.
    UnknownLanguage(String),
    /// The specified theme is not found.
    UnknownTheme(String),
    /// Failed to read the file.
    IoError(String),
    /// Failed to load syntect assets from disk.
    LoadError(String),
}

impl std::fmt::Display for SyntaxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownLanguage(lang) => write!(f, "Unknown language: {lang}"),
            Self::UnknownTheme(theme) => write!(f, "Unknown theme: {theme}"),
            Self::IoError(msg) => write!(f, "IO error: {msg}"),
            Self::LoadError(msg) => write!(f, "Load error: {msg}"),
        }
    }
}

impl std::error::Error for SyntaxError {}

/// A syntax-highlighted code block renderable.
///
/// Uses syntect for syntax highlighting with support for themes,
/// line numbers, and background colors.
#[derive(Debug, Clone)]
pub struct Syntax {
    /// The source code to highlight.
    code: String,
    /// The language for syntax highlighting.
    language: String,
    /// Whether to show line numbers.
    line_numbers: bool,
    /// The starting line number (for excerpts).
    start_line: usize,
    /// The theme name to use.
    theme_name: String,
    /// Optional background color override.
    background_color: Option<Color>,
    /// Whether to show indentation guides.
    indent_guides: bool,
    /// Tab size for rendering.
    tab_size: usize,
    /// Optional word wrap width.
    word_wrap: Option<usize>,
    /// Style for the line number column.
    line_number_style: Style,
    /// Padding around the code block.
    padding: (usize, usize),
    /// Optional custom syntax set (loaded from user paths).
    custom_syntax_set: Option<Arc<SyntaxSet>>,
    /// Optional custom theme set (loaded from user paths).
    custom_theme_set: Option<Arc<ThemeSet>>,
}

impl Default for Syntax {
    fn default() -> Self {
        Self {
            code: String::new(),
            language: String::from("text"),
            line_numbers: false,
            start_line: 1,
            theme_name: String::from(PYTHON_RICH_THEME),
            background_color: None,
            indent_guides: false,
            tab_size: 4,
            word_wrap: None,
            line_number_style: Style::new().color_str("bright_black").unwrap_or_default(),
            padding: (0, 0),
            custom_syntax_set: None,
            custom_theme_set: None,
        }
    }
}

impl Syntax {
    /// Create a new syntax highlighted code block.
    ///
    /// # Arguments
    ///
    /// * `code` - The source code to highlight
    /// * `language` - The programming language (e.g., "rust", "python", "javascript")
    #[must_use]
    pub fn new(code: impl Into<String>, language: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            language: language.into(),
            ..Default::default()
        }
    }

    /// Load syntax from a file path, auto-detecting the language.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read.
    pub fn from_path(path: impl AsRef<Path>) -> Result<Self, SyntaxError> {
        let path = path.as_ref();
        let code = fs::read_to_string(path).map_err(|e| SyntaxError::IoError(e.to_string()))?;

        // Auto-detect language from extension
        let language = path
            .extension()
            .and_then(|ext| ext.to_str())
            .map_or_else(|| String::from("text"), Self::extension_to_language);

        Ok(Self::new(code, language))
    }

    /// Load a syntect syntax set from a folder of `.sublime-syntax` definitions.
    ///
    /// Use this to opt-in to custom / user-provided syntax definitions.
    ///
    /// # Errors
    ///
    /// Returns an error if the folder can't be read or the definitions can't be parsed.
    pub fn load_syntaxes_from_folder(
        folder: impl AsRef<Path>,
    ) -> Result<Arc<SyntaxSet>, SyntaxError> {
        SyntaxSet::load_from_folder(folder)
            .map(Arc::new)
            .map_err(|e| SyntaxError::LoadError(e.to_string()))
    }

    /// Load a syntect theme set from a folder of `.tmTheme` files.
    ///
    /// Use this to opt-in to custom / user-provided themes.
    ///
    /// # Errors
    ///
    /// Returns an error if the folder can't be read or the themes can't be parsed.
    pub fn load_themes_from_folder(folder: impl AsRef<Path>) -> Result<Arc<ThemeSet>, SyntaxError> {
        ThemeSet::load_from_folder(folder)
            .map(Arc::new)
            .map_err(|e| SyntaxError::LoadError(e.to_string()))
    }

    /// Provide a custom syntect syntax set (e.g. loaded via [`Self::load_syntaxes_from_folder`]).
    #[must_use]
    pub fn syntax_set(mut self, syntax_set: Arc<SyntaxSet>) -> Self {
        self.custom_syntax_set = Some(syntax_set);
        self
    }

    /// Provide a custom syntect theme set (e.g. loaded via [`Self::load_themes_from_folder`]).
    #[must_use]
    pub fn theme_set(mut self, theme_set: Arc<ThemeSet>) -> Self {
        self.custom_theme_set = Some(theme_set);
        self
    }

    /// Map file extension to language name.
    fn extension_to_language(ext: &str) -> String {
        match ext.to_lowercase().as_str() {
            "rs" => "rust",
            "py" => "python",
            "js" => "javascript",
            "ts" => "typescript",
            "jsx" => "javascript",
            "tsx" => "typescript",
            "rb" => "ruby",
            "go" => "go",
            "java" => "java",
            "c" => "c",
            "cpp" | "cxx" | "cc" => "c++",
            "h" | "hpp" => "c++",
            "cs" => "c#",
            "php" => "php",
            "swift" => "swift",
            "kt" | "kts" => "kotlin",
            "scala" => "scala",
            "sh" | "bash" => "bash",
            "zsh" => "zsh",
            "fish" => "fish",
            "ps1" => "powershell",
            "sql" => "sql",
            "html" | "htm" => "html",
            "css" => "css",
            "scss" => "scss",
            "less" => "less",
            "json" => "json",
            "yaml" | "yml" => "yaml",
            "toml" => "toml",
            "xml" => "xml",
            "md" | "markdown" => "markdown",
            "r" => "r",
            "lua" => "lua",
            "perl" | "pl" => "perl",
            "vim" => "vim",
            "dockerfile" => "dockerfile",
            "makefile" => "makefile",
            _ => ext,
        }
        .to_string()
    }

    /// Enable or disable line numbers.
    #[must_use]
    pub fn line_numbers(mut self, enabled: bool) -> Self {
        self.line_numbers = enabled;
        self
    }

    /// Set the starting line number (useful for code excerpts).
    #[must_use]
    pub fn start_line(mut self, line: usize) -> Self {
        self.start_line = line.max(1);
        self
    }

    /// Set the theme for syntax highlighting.
    ///
    /// Common themes: "python-rich-default", "base16-ocean.dark", "base16-ocean.light",
    /// `InspiredGitHub`, `Solarized (dark)`, `Solarized (light)`
    #[must_use]
    pub fn theme(mut self, theme_name: impl Into<String>) -> Self {
        self.theme_name = theme_name.into();
        self
    }

    /// Override the background color.
    #[must_use]
    pub fn background_color(mut self, color: Color) -> Self {
        self.background_color = Some(color);
        self
    }

    /// Enable or disable indentation guides.
    #[must_use]
    pub fn indent_guides(mut self, enabled: bool) -> Self {
        self.indent_guides = enabled;
        self
    }

    /// Set the tab size.
    #[must_use]
    pub fn tab_size(mut self, size: usize) -> Self {
        self.tab_size = size.max(1);
        self
    }

    /// Set word wrap width.
    #[must_use]
    pub fn word_wrap(mut self, width: Option<usize>) -> Self {
        self.word_wrap = width;
        self
    }

    /// Set the style for line numbers.
    #[must_use]
    pub fn line_number_style(mut self, style: Style) -> Self {
        self.line_number_style = style;
        self
    }

    /// Set padding around the code block (top/bottom, left/right).
    #[must_use]
    pub fn padding(mut self, vertical: usize, horizontal: usize) -> Self {
        self.padding = (vertical, horizontal);
        self
    }

    /// Get the list of available themes.
    #[must_use]
    pub fn available_themes() -> Vec<String> {
        let mut themes: Vec<String> = THEME_SET.themes.keys().cloned().collect();
        themes.push(PYTHON_RICH_THEME.to_string());
        themes.sort();
        themes.dedup();
        themes
    }

    /// Get the list of available languages.
    #[must_use]
    pub fn available_languages() -> Vec<String> {
        SYNTAX_SET
            .syntaxes()
            .iter()
            .map(|s| s.name.clone())
            .collect()
    }

    /// Render the syntax-highlighted code to segments.
    ///
    /// # Errors
    ///
    /// Returns an error if the theme or language is not found.
    pub fn render(&self, max_width: Option<usize>) -> Result<Vec<Segment<'_>>, SyntaxError> {
        let ps: &SyntaxSet = self.custom_syntax_set.as_deref().unwrap_or(&*SYNTAX_SET);
        let ts: &ThemeSet = self.custom_theme_set.as_deref().unwrap_or(&*THEME_SET);
        let use_python_rich_theme = self.uses_python_rich_theme();
        let use_python_rich_rust = use_python_rich_theme && Self::is_rust_language(&self.language);

        // Find the syntax definition
        let syntax = ps
            .find_syntax_by_token(&self.language)
            .or_else(|| ps.find_syntax_by_extension(&self.language))
            .ok_or_else(|| SyntaxError::UnknownLanguage(self.language.clone()))?;

        // Get the theme. The Python-Rich compatibility theme name is an alias; use a built-in
        // syntect theme as fallback for non-Rust languages and any behavior that still depends on
        // syntect internals.
        let resolved_theme_name = if use_python_rich_theme {
            PYTHON_RICH_FALLBACK_THEME
        } else {
            &self.theme_name
        };
        let theme = ts
            .themes
            .get(resolved_theme_name)
            .ok_or_else(|| SyntaxError::UnknownTheme(self.theme_name.clone()))?;

        let mut highlighter = HighlightLines::new(syntax, theme);
        let mut segments: Vec<Segment<'static>> = Vec::new();

        // Background used for padding/fill and for styling indent guides.
        let bg = if let Some(ref override_bg) = self.background_color {
            override_bg.clone()
        } else if use_python_rich_theme {
            Color::from_rgb(39, 40, 34)
        } else {
            let bg_color = theme
                .settings
                .background
                .unwrap_or(syntect::highlighting::Color::BLACK);
            Color::from_rgb(bg_color.r, bg_color.g, bg_color.b)
        };
        let base_bg_style = Style::new().bgcolor(bg.clone());
        let guide_style = base_bg_style.combine(&Style::new().dim());

        // Calculate line number width (digits only).
        let line_count = self.code.lines().count();
        let last_line = self.start_line.saturating_add(line_count.saturating_sub(1));
        let line_num_width = last_line.to_string().len();
        let line_number_padding = 2usize; // Rich-style line number gutter
        let line_prefix_width = if self.line_numbers {
            line_number_padding + line_num_width + 1 // +1 for trailing space after number
        } else {
            0
        };
        let line_number_style = base_bg_style.combine(&self.line_number_style);

        // If enabled, wrap the *code content* to this cell width (excluding gutter).
        //
        // Python Rich's `Syntax` wraps to the full available console width (minus any line number
        // gutter) and then applies padding externally, which can crop a character that no longer
        // fits once padding is added. To match that behavior, we *do not* subtract horizontal
        // padding here; instead we crop/pad the final segment stream to `max_width` below.
        //
        // Wrapping is whitespace-preserving (tuned for code rather than prose reflow).
        let wrap_width = self.word_wrap.and_then(|w| {
            if w == 0 {
                return None;
            }
            let cap = max_width.unwrap_or(usize::MAX);
            let available = cap.saturating_sub(line_prefix_width);
            if available == 0 {
                None
            } else {
                Some(w.min(available))
            }
        });

        // Add top padding
        for _ in 0..self.padding.0 {
            segments.push(Segment::line());
        }

        // Process each physical line (including an optional trailing newline per line).
        for (idx, line) in LinesWithEndings::from(&self.code).enumerate() {
            let line_num = self.start_line + idx;

            let normalized = line.replace("\r\n", "\n");
            let had_newline = normalized.ends_with('\n');
            let mut line_no_nl = normalized.as_str();
            if had_newline {
                line_no_nl = &line_no_nl[..line_no_nl.len().saturating_sub(1)];
            }

            // Expand tabs for stable display + wrapping.
            let tab_expanded = line_no_nl.replace('\t', &" ".repeat(self.tab_size));

            // Indentation guides: inject guide characters into leading whitespace, then style them
            // as dim while preserving the background.
            let leading_spaces = tab_expanded.chars().take_while(|c| *c == ' ').count();
            let line_for_highlight = if self.indent_guides && leading_spaces > 0 {
                apply_indent_guides(&tab_expanded, self.tab_size)
            } else {
                tab_expanded
            };

            let mut line_text = Text::new("");
            if use_python_rich_rust {
                for (text, style) in self.python_rich_rust_highlight(&line_for_highlight, &bg) {
                    line_text.append_styled(&text, style);
                }
            } else {
                // Highlight the line (no trailing newline).
                let ranges = highlighter
                    .highlight_line(&line_for_highlight, ps)
                    .unwrap_or_else(|_| {
                        vec![(
                            syntect::highlighting::Style::default(),
                            line_for_highlight.as_str(),
                        )]
                    });

                let mut col = 0usize;
                for (style, text) in ranges {
                    if text.is_empty() {
                        continue;
                    }
                    let rich_style = if use_python_rich_theme {
                        self.syntect_style_to_python_rich_compat(style, text, &bg, theme)
                    } else {
                        self.syntect_style_to_rich(style, theme)
                    };
                    append_syntax_text(
                        &mut line_text,
                        text,
                        &rich_style,
                        leading_spaces,
                        &mut col,
                        &guide_style,
                    );
                }
            }

            let visual_lines: Vec<Text> = if let Some(wrap_width) = wrap_width {
                wrap_text_preserving_whitespace(&line_text, wrap_width)
            } else {
                vec![line_text]
            };

            for (visual_idx, visual_line) in visual_lines.iter().cloned().enumerate() {
                // Left padding (styled with background so the block background is continuous).
                if self.padding.1 > 0 {
                    segments.push(Segment::new(
                        " ".repeat(self.padding.1),
                        Some(base_bg_style.clone()),
                    ));
                }

                // In Python Rich, horizontal padding is applied by an outer `Padding` renderable
                // that crops/pads the *inner* content to `width - left - right`, and then appends
                // the left/right pad segments. To match that behavior, we:
                // 1) render the line content (gutter + code),
                // 2) crop/pad it to the inner width (when a max width is known),
                // 3) append right padding afterwards.
                let mut content_line: Vec<Segment<'static>> = Vec::new();

                // Line number gutter (Rich-style: two-space gutter, number, trailing space).
                if self.line_numbers {
                    let gutter = if visual_idx == 0 {
                        format!(
                            "{}{:>width$} ",
                            " ".repeat(line_number_padding),
                            line_num,
                            width = line_num_width
                        )
                    } else {
                        " ".repeat(line_number_padding + line_num_width + 1)
                    };
                    content_line.push(Segment::new(gutter, Some(line_number_style.clone())));
                }

                // Highlighted code for this visual line.
                content_line.extend(visual_line.render("").into_iter().map(Segment::into_owned));

                if let Some(cap) = max_width.filter(|value| *value > 0)
                    && self.padding.1 > 0
                {
                    let inner_width = cap.saturating_sub(self.padding.1.saturating_mul(2));
                    content_line = crate::segment::adjust_line_length(
                        content_line,
                        inner_width,
                        Some(base_bg_style.clone()),
                        true,
                    );
                }

                segments.extend(content_line);

                // Right padding
                if self.padding.1 > 0 {
                    segments.push(Segment::new(
                        " ".repeat(self.padding.1),
                        Some(base_bg_style.clone()),
                    ));
                }

                // Newline between wrapped visual lines; preserve the original newline if present.
                let is_last_visual = visual_idx + 1 == visual_lines.len();
                if !is_last_visual || had_newline {
                    segments.push(Segment::line());
                }
            }
        }

        // Add bottom padding
        for _ in 0..self.padding.0 {
            segments.push(Segment::line());
        }

        if let Some(width) = max_width.filter(|value| *value > 0) {
            Ok(pad_segments_to_width(segments, width, Some(&base_bg_style)))
        } else {
            Ok(segments)
        }
    }

    /// Convert syntect style to rich Style.
    fn syntect_style_to_rich(&self, style: syntect::highlighting::Style, theme: &Theme) -> Style {
        let fg = Color::from_rgb(style.foreground.r, style.foreground.g, style.foreground.b);

        let bg = if let Some(ref override_bg) = self.background_color {
            override_bg.clone()
        } else {
            let bg_color = theme
                .settings
                .background
                .unwrap_or(syntect::highlighting::Color::BLACK);
            Color::from_rgb(bg_color.r, bg_color.g, bg_color.b)
        };

        let mut rich_style = Style::new().color(fg).bgcolor(bg);

        // Apply font style modifiers
        if style
            .font_style
            .contains(syntect::highlighting::FontStyle::BOLD)
        {
            rich_style = rich_style.bold();
        }
        if style
            .font_style
            .contains(syntect::highlighting::FontStyle::ITALIC)
        {
            rich_style = rich_style.italic();
        }
        if style
            .font_style
            .contains(syntect::highlighting::FontStyle::UNDERLINE)
        {
            rich_style = rich_style.underline();
        }

        rich_style
    }

    fn syntect_style_to_python_rich_compat(
        &self,
        style: syntect::highlighting::Style,
        text: &str,
        background: &Color,
        theme: &Theme,
    ) -> Style {
        let mut rich_style = self.syntect_style_to_rich(style, theme);

        if self.background_color.is_none() {
            rich_style = rich_style.bgcolor(background.clone());
        }

        let mapped_color = python_rich_mapped_foreground(style.foreground, text);
        rich_style.color(mapped_color)
    }

    fn uses_python_rich_theme(&self) -> bool {
        self.theme_name == PYTHON_RICH_THEME
    }

    fn is_rust_language(language: &str) -> bool {
        matches!(language.to_ascii_lowercase().as_str(), "rust" | "rs")
    }

    fn python_rich_style(token_kind: RustTokenKind, background: &Color) -> Style {
        let fg = match token_kind {
            RustTokenKind::Keyword => Color::from_rgb(102, 217, 239),
            RustTokenKind::Function => Color::from_rgb(166, 226, 46),
            RustTokenKind::Operator => Color::from_rgb(255, 70, 137),
            RustTokenKind::Number => Color::from_rgb(174, 129, 255),
            RustTokenKind::String => Color::from_rgb(230, 219, 116),
            RustTokenKind::Comment => Color::from_rgb(117, 113, 94),
            RustTokenKind::Plain => Color::from_rgb(248, 248, 242),
        };

        let mut style = Style::new().color(fg).bgcolor(background.clone());
        if matches!(token_kind, RustTokenKind::Comment) {
            style = style.italic();
        }
        style
    }

    fn python_rich_rust_highlight(&self, line: &str, background: &Color) -> Vec<(String, Style)> {
        let mut out: Vec<(String, Style)> = Vec::new();
        let bytes = line.as_bytes();
        let mut i = 0usize;

        while i < bytes.len() {
            let b = bytes[i];

            if b.is_ascii_whitespace() {
                let start = i;
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_whitespace() {
                    i += 1;
                }
                out.push((
                    line[start..i].to_string(),
                    Self::python_rich_style(RustTokenKind::Plain, background),
                ));
                continue;
            }

            if b == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                out.push((
                    line[i..].to_string(),
                    Self::python_rich_style(RustTokenKind::Comment, background),
                ));
                break;
            }

            if b == b'"' {
                out.push((
                    "\"".to_string(),
                    Self::python_rich_style(RustTokenKind::String, background),
                ));
                i += 1;

                let string_start = i;
                let mut escaped = false;
                while i < bytes.len() {
                    let current = bytes[i];
                    if escaped {
                        escaped = false;
                        i += 1;
                        continue;
                    }
                    if current == b'\\' {
                        escaped = true;
                        i += 1;
                        continue;
                    }
                    if current == b'"' {
                        break;
                    }
                    i += 1;
                }

                if string_start < i {
                    out.push((
                        line[string_start..i].to_string(),
                        Self::python_rich_style(RustTokenKind::String, background),
                    ));
                }

                if i < bytes.len() && bytes[i] == b'"' {
                    out.push((
                        "\"".to_string(),
                        Self::python_rich_style(RustTokenKind::String, background),
                    ));
                    i += 1;
                }
                continue;
            }

            if b.is_ascii_digit() {
                let start = i;
                i += 1;
                while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == b'_') {
                    i += 1;
                }
                out.push((
                    line[start..i].to_string(),
                    Self::python_rich_style(RustTokenKind::Number, background),
                ));
                continue;
            }

            if is_identifier_start(b) {
                let start = i;
                i += 1;
                while i < bytes.len() && is_identifier_continue(bytes[i]) {
                    i += 1;
                }

                let mut end = i;
                if end < bytes.len() && bytes[end] == b'!' {
                    end += 1;
                    i = end;
                }

                let lexeme = &line[start..end];
                let keyword_lexeme = lexeme.strip_suffix('!').unwrap_or(lexeme);
                let next_non_whitespace = next_non_whitespace_byte(bytes, i);

                let kind = if is_rust_keyword(keyword_lexeme) {
                    RustTokenKind::Keyword
                } else if lexeme.ends_with('!') || next_non_whitespace == Some(b'(') {
                    RustTokenKind::Function
                } else {
                    RustTokenKind::Plain
                };

                out.push((
                    lexeme.to_string(),
                    Self::python_rich_style(kind, background),
                ));
                continue;
            }

            if is_operator_byte(b) {
                let start = i;
                i += 1;
                while i < bytes.len() && is_operator_byte(bytes[i]) {
                    i += 1;
                }
                out.push((
                    line[start..i].to_string(),
                    Self::python_rich_style(RustTokenKind::Operator, background),
                ));
                continue;
            }

            let ch_len = line[i..].chars().next().map_or(1, char::len_utf8);
            out.push((
                line[i..i + ch_len].to_string(),
                Self::python_rich_style(RustTokenKind::Plain, background),
            ));
            i += ch_len;
        }

        out
    }

    /// Get the highlighted code as a concatenated string (for testing/preview).
    #[must_use]
    pub fn plain_text(&self) -> String {
        self.code.clone()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RustTokenKind {
    Keyword,
    Function,
    Operator,
    Number,
    String,
    Comment,
    Plain,
}

fn is_identifier_start(byte: u8) -> bool {
    byte == b'_' || byte.is_ascii_alphabetic()
}

fn is_identifier_continue(byte: u8) -> bool {
    is_identifier_start(byte) || byte.is_ascii_digit()
}

fn is_operator_byte(byte: u8) -> bool {
    matches!(
        byte,
        b'=' | b'+' | b'-' | b'*' | b'/' | b'%' | b'!' | b'<' | b'>' | b'&' | b'|' | b'^'
    )
}

fn next_non_whitespace_byte(bytes: &[u8], mut index: usize) -> Option<u8> {
    while index < bytes.len() {
        if !bytes[index].is_ascii_whitespace() {
            return Some(bytes[index]);
        }
        index += 1;
    }
    None
}

fn is_rust_keyword(token: &str) -> bool {
    matches!(
        token,
        "as" | "async"
            | "await"
            | "break"
            | "const"
            | "continue"
            | "crate"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "self"
            | "Self"
            | "static"
            | "struct"
            | "super"
            | "trait"
            | "true"
            | "type"
            | "unsafe"
            | "use"
            | "where"
            | "while"
    )
}

fn python_rich_mapped_foreground(fg: syntect::highlighting::Color, text: &str) -> Color {
    if token_is_operator(text) {
        return Color::from_rgb(255, 70, 137);
    }
    if token_is_quote_literal(text) {
        return Color::from_rgb(230, 219, 116);
    }

    match (fg.r, fg.g, fg.b) {
        // base16-ocean.dark -> Python Rich default syntax palette remap
        (180, 142, 173) => Color::from_rgb(102, 217, 239), // keyword
        (143, 161, 179) => Color::from_rgb(166, 226, 46),  // function/name
        (192, 197, 206) => Color::from_rgb(248, 248, 242), // plain/punctuation
        (163, 190, 140) => Color::from_rgb(230, 219, 116), // string
        (208, 135, 112) => Color::from_rgb(174, 129, 255), // number
        (191, 97, 106) => Color::from_rgb(255, 70, 137),   // operator-ish
        (96, 129, 139) => Color::from_rgb(117, 113, 94),   // comment-ish
        _ => Color::from_rgb(fg.r, fg.g, fg.b),
    }
}

fn token_is_operator(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| "=+-*/%<>!&|^~".contains(c) || c.is_whitespace())
        && text.chars().any(|c| "=+-*/%<>!&|^~".contains(c))
}

fn token_is_quote_literal(text: &str) -> bool {
    !text.is_empty() && text.chars().all(|c| c == '"' || c == '\'')
}

fn apply_indent_guides(line: &str, tab_size: usize) -> String {
    if tab_size == 0 {
        return line.to_string();
    }

    let leading_spaces = line.chars().take_while(|c| *c == ' ').count();
    if leading_spaces < tab_size {
        return line.to_string();
    }

    let mut out = String::with_capacity(line.len());
    for (col, ch) in line.chars().enumerate() {
        if col < leading_spaces && ch == ' ' {
            // Rich-style indent guides: show a guide at the start of each indent level:
            // 4 spaces -> "│   ", 8 spaces -> "│   │   ", etc.
            if col.is_multiple_of(tab_size) {
                out.push('│');
            } else {
                out.push(' ');
            }
        } else {
            out.push(ch);
        }
    }
    out
}

fn append_syntax_text(
    out: &mut Text,
    text: &str,
    token_style: &Style,
    leading_spaces: usize,
    col: &mut usize,
    guide_style: &Style,
) {
    let mut buf = String::new();
    let mut buf_is_guide = false;
    let mut started = false;

    for ch in text.chars() {
        let is_guide = *col < leading_spaces && ch == '│';

        if started && is_guide != buf_is_guide {
            let seg_style = if buf_is_guide {
                guide_style
            } else {
                token_style
            };
            out.append_styled(&std::mem::take(&mut buf), seg_style.clone());
        }

        if !started {
            started = true;
        }
        buf_is_guide = is_guide;

        buf.push(ch);
        *col = (*col).saturating_add(1);
    }

    if !buf.is_empty() {
        let seg_style = if buf_is_guide {
            guide_style
        } else {
            token_style
        };
        out.append_styled(&buf, seg_style.clone());
    }
}

fn wrap_text_preserving_whitespace(line: &Text, width: usize) -> Vec<Text> {
    if width == 0 {
        return vec![Text::new("")];
    }

    if line.cell_len() <= width {
        return vec![line.clone()];
    }

    let chars: Vec<char> = line.plain().chars().collect();
    let mut out = Vec::new();
    let mut start = 0usize;

    while start < chars.len() {
        let mut cell_width = 0usize;
        let mut i = start;
        let mut last_whitespace: Option<usize> = None;

        while i < chars.len() {
            let w = cells::get_character_cell_size(chars[i]);
            if cell_width + w > width {
                break;
            }
            cell_width += w;
            if chars[i].is_whitespace() {
                last_whitespace = Some(i);
            }
            i += 1;
        }

        if i == start {
            // Can't fit even a single character in the width (e.g. width=1, wide char),
            // so we force progress by taking 1 char.
            out.push(line.slice(start, (start + 1).min(chars.len())));
            start = (start + 1).min(chars.len());
            continue;
        }

        if i >= chars.len() {
            out.push(line.slice(start, chars.len()));
            break;
        }

        if let Some(ws) = last_whitespace.filter(|ws| *ws >= start) {
            // Wrap after whitespace, keeping the whitespace at the end of the previous line.
            let end = (ws + 1).min(chars.len());
            out.push(line.slice(start, end));
            start = end;
        } else {
            out.push(line.slice(start, i));
            start = i;
        }
    }

    if out.is_empty() {
        out.push(Text::new(""));
    }

    out
}

fn pad_segments_to_width(
    segments: Vec<Segment<'static>>,
    width: usize,
    fill_style: Option<&Style>,
) -> Vec<Segment<'static>> {
    let fill_style = fill_style.cloned();
    let mut out: Vec<Segment<'static>> = Vec::new();
    let mut line: Vec<Segment<'static>> = Vec::new();

    for segment in segments {
        if segment.is_control() {
            line.push(segment);
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
                    line.push(Segment::new(part.to_string(), style.clone()));
                }

                // Python Rich uses `Segment.split_and_crop_lines(...)` downstream of padding; that
                // has the effect of cropping any characters that no longer fit once padding is
                // applied. We replicate that here by truncating/padding each final line to width.
                let adjusted = crate::segment::adjust_line_length(
                    std::mem::take(&mut line),
                    width,
                    fill_style.clone(),
                    true,
                );
                out.extend(adjusted);
                out.push(Segment::line());
                start = idx + 1;
            }
        }

        let tail = &text_ref[start..];
        if !tail.is_empty() {
            line.push(Segment::new(tail.to_string(), style));
        }
    }

    // If there's a trailing partial line (no newline), match prior behavior by padding/truncating
    // it to width and emitting a final newline.
    if !line.is_empty() {
        let adjusted = crate::segment::adjust_line_length(line, width, fill_style, true);
        out.extend(adjusted);
        out.push(Segment::line());
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn test_syntax_new() {
        let syntax = Syntax::new("let x = 1;", "rust");
        assert_eq!(syntax.code, "let x = 1;");
        assert_eq!(syntax.language, "rust");
        assert!(!syntax.line_numbers);
    }

    #[test]
    fn test_syntax_builder() {
        let syntax = Syntax::new("print('hello')", "python")
            .line_numbers(true)
            .start_line(10)
            .theme("base16-ocean.dark")
            .indent_guides(true)
            .tab_size(2)
            .padding(1, 2);

        assert!(syntax.line_numbers);
        assert_eq!(syntax.start_line, 10);
        assert_eq!(syntax.theme_name, "base16-ocean.dark");
        assert!(syntax.indent_guides);
        assert_eq!(syntax.tab_size, 2);
        assert_eq!(syntax.padding, (1, 2));
    }

    #[test]
    fn test_extension_to_language() {
        assert_eq!(Syntax::extension_to_language("rs"), "rust");
        assert_eq!(Syntax::extension_to_language("py"), "python");
        assert_eq!(Syntax::extension_to_language("js"), "javascript");
        assert_eq!(Syntax::extension_to_language("ts"), "typescript");
        assert_eq!(Syntax::extension_to_language("go"), "go");
        assert_eq!(Syntax::extension_to_language("unknown"), "unknown");
    }

    #[test]
    fn test_available_themes() {
        let themes = Syntax::available_themes();
        assert!(!themes.is_empty());
        assert!(themes.iter().any(|t| t.contains("base16")));
    }

    #[test]
    fn test_available_languages() {
        let langs = Syntax::available_languages();
        assert!(!langs.is_empty());
    }

    #[test]
    fn test_render_simple() {
        let code = r#"fn main() {
    println!("Hello, world!");
}"#;
        let syntax = Syntax::new(code, "rust");
        let result = syntax.render(None);
        assert!(result.is_ok());
        let segments = result.unwrap();
        assert!(!segments.is_empty());
    }

    #[test]
    fn test_render_with_line_numbers() {
        let code = "x = 1\ny = 2";
        let syntax = Syntax::new(code, "python").line_numbers(true);
        let result = syntax.render(None);
        assert!(result.is_ok());
        let segments = result.unwrap();
        // Should contain line number segments
        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains('1'));
        assert!(text.contains('2'));
    }

    #[test]
    fn test_render_unknown_language() {
        let syntax = Syntax::new("code", "nonexistent_lang_xyz");
        let result = syntax.render(None);
        assert!(
            matches!(result, Err(SyntaxError::UnknownLanguage(ref lang)) if lang == "nonexistent_lang_xyz")
        );
    }

    #[test]
    fn test_render_unknown_theme() {
        let syntax = Syntax::new("let x = 1", "rust").theme("nonexistent_theme_xyz");
        let result = syntax.render(None);
        assert!(
            matches!(result, Err(SyntaxError::UnknownTheme(ref theme)) if theme == "nonexistent_theme_xyz")
        );
    }

    #[test]
    fn test_plain_text() {
        let code = "fn main() {}";
        let syntax = Syntax::new(code, "rust");
        assert_eq!(syntax.plain_text(), code);
    }

    #[test]
    fn test_background_color_override() {
        let syntax = Syntax::new("code", "text").background_color(Color::parse("red").unwrap());
        assert!(syntax.background_color.is_some());
    }

    #[test]
    fn test_start_line_minimum() {
        let syntax = Syntax::new("code", "text").start_line(0);
        assert_eq!(syntax.start_line, 1); // Should be at minimum 1
    }

    #[test]
    fn test_tab_size_minimum() {
        let syntax = Syntax::new("code", "text").tab_size(0);
        assert_eq!(syntax.tab_size, 1); // Should be at minimum 1
    }

    #[test]
    fn test_padding_does_not_shift_lines() {
        // Use "rust" as a valid language (syntect doesn't have a "text" syntax)
        let syntax = Syntax::new("a\nb", "rust").padding(0, 2);
        let text = syntax
            .render(None)
            .expect("render should succeed")
            .iter()
            .map(|s| s.text.as_ref())
            .collect::<String>();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines, vec!["  a  ", "  b  "]);
    }

    #[test]
    fn test_render_crlf_strips_carriage_returns() {
        let code = "let x = 1;\r\nlet y = 2;\r\n";
        let syntax = Syntax::new(code, "rust");
        let text = syntax
            .render(None)
            .expect("render should succeed")
            .iter()
            .map(|s| s.text.as_ref())
            .collect::<String>();
        assert!(!text.contains('\r'));
        assert!(text.contains("let x = 1;"));
        assert!(text.contains("let y = 2;"));
    }

    // =========================================================================
    // Runtime Theme Switching Tests (bd-hf2k)
    // =========================================================================

    #[test]
    fn test_theme_switching_via_builder() {
        // Test that calling theme() multiple times correctly updates the theme
        let syntax = Syntax::new("fn main() {}", "rust")
            .theme("base16-ocean.dark")
            .theme("InspiredGitHub"); // Switch to a different theme

        assert_eq!(syntax.theme_name, "InspiredGitHub");
    }

    #[test]
    fn test_different_themes_produce_different_styles() {
        let code = "fn main() { println!(\"hello\"); }";

        // Render with two different themes
        let syntax_dark = Syntax::new(code, "rust").theme("base16-ocean.dark");
        let syntax_light = Syntax::new(code, "rust").theme("InspiredGitHub");

        let segments_dark = syntax_dark.render(None).expect("dark theme render");
        let segments_light = syntax_light.render(None).expect("light theme render");

        // Collect styles from each render
        let styles_dark: Vec<_> = segments_dark
            .iter()
            .filter_map(|s| s.style.as_ref())
            .collect();
        let styles_light: Vec<_> = segments_light
            .iter()
            .filter_map(|s| s.style.as_ref())
            .collect();

        // Both should have styles (non-empty)
        assert!(
            !styles_dark.is_empty(),
            "dark theme should produce styled segments"
        );
        assert!(
            !styles_light.is_empty(),
            "light theme should produce styled segments"
        );

        // The themes should produce different background colors
        // (dark themes have dark backgrounds, light themes have light backgrounds)
        // This is a soft check - we just verify they're not identical
        let dark_first = styles_dark.first().unwrap();
        let light_first = styles_light.first().unwrap();
        assert_ne!(
            dark_first.to_string(),
            light_first.to_string(),
            "different themes should produce different styles"
        );
    }

    #[test]
    fn test_render_with_all_available_themes() {
        let code = "let x = 42;";
        let themes = Syntax::available_themes();

        // Ensure we have multiple themes to test
        assert!(
            themes.len() >= 2,
            "expected multiple themes, got {}",
            themes.len()
        );

        // Render with each available theme - all should succeed
        for theme_name in &themes {
            let syntax = Syntax::new(code, "rust").theme(theme_name);
            let result = syntax.render(None);
            assert!(
                result.is_ok(),
                "rendering with theme '{theme_name}' should succeed",
            );
        }
    }

    #[test]
    fn test_clone_and_change_theme() {
        let original = Syntax::new("x = 1", "python").theme("base16-ocean.dark");

        // Clone and change theme
        let modified = original.clone().theme("InspiredGitHub");

        // Original should be unchanged
        assert_eq!(original.theme_name, "base16-ocean.dark");
        assert_eq!(modified.theme_name, "InspiredGitHub");

        // Both should render successfully
        assert!(original.render(None).is_ok());
        assert!(modified.render(None).is_ok());
    }

    #[test]
    fn test_background_color_override_takes_precedence_over_theme() {
        let code = "fn main() {}";
        let custom_bg = Color::parse("#ff0000").expect("parse red");

        let syntax = Syntax::new(code, "rust")
            .theme("base16-ocean.dark")
            .background_color(custom_bg.clone());

        let segments = syntax.render(None).expect("render");

        // Find a segment with a style and check its background
        let styled_segment = segments.iter().find(|s| s.style.is_some());
        assert!(styled_segment.is_some(), "should have styled segments");

        // The background should be our custom color, not the theme's background
        if let Some(seg) = styled_segment {
            let style = seg.style.as_ref().unwrap();
            // The style should contain our custom background
            // Note: exact comparison depends on internal representation
            let style_str = style.to_string();
            assert!(
                style_str.contains("on #ff0000")
                    || style_str.contains("on rgb(255,0,0)")
                    || style_str.contains("on color("),
                "expected custom background in style, got: {style_str}",
            );
        }
    }

    #[test]
    fn test_default_theme_is_python_rich_default() {
        let syntax = Syntax::new("code", "rust");
        assert_eq!(syntax.theme_name, "python-rich-default");
    }

    #[test]
    fn test_python_rich_default_theme_is_available() {
        let themes = Syntax::available_themes();
        assert!(themes.iter().any(|name| name == "python-rich-default"));
    }

    #[test]
    fn test_python_rich_rust_token_styles() {
        let syntax = Syntax::new("", "rust");
        let background = Color::from_rgb(39, 40, 34);
        let highlighted = syntax.python_rich_rust_highlight("let x = 1234;", &background);

        let eq_style = highlighted
            .iter()
            .find(|(text, _)| text == "=")
            .map(|(_, style)| style.clone())
            .expect("operator token should exist");
        assert_eq!(eq_style.color, Some(Color::from_rgb(255, 70, 137)));
        assert_eq!(eq_style.bgcolor, Some(background.clone()));

        let number_style = highlighted
            .iter()
            .find(|(text, _)| text == "1234")
            .map(|(_, style)| style.clone())
            .expect("number token should exist");
        assert_eq!(number_style.color, Some(Color::from_rgb(174, 129, 255)));
        assert_eq!(number_style.bgcolor, Some(background));
    }

    #[test]
    fn test_python_rich_non_rust_operator_and_quote_styles() {
        let syntax = Syntax::new("x = \"hi\"", "python");
        let segments = syntax.render(Some(40)).expect("render should succeed");

        let eq_style = segments
            .iter()
            .find(|segment| segment.text == "=")
            .and_then(|segment| segment.style.clone())
            .expect("operator segment style should exist");
        assert_eq!(eq_style.color, Some(Color::from_rgb(255, 70, 137)));
        assert_eq!(eq_style.bgcolor, Some(Color::from_rgb(39, 40, 34)));

        let quote_style = segments
            .iter()
            .find(|segment| segment.text == "\"")
            .and_then(|segment| segment.style.clone())
            .expect("quote segment style should exist");
        assert_eq!(quote_style.color, Some(Color::from_rgb(230, 219, 116)));
        assert_eq!(quote_style.bgcolor, Some(Color::from_rgb(39, 40, 34)));
    }

    #[test]
    fn test_word_wrap_builder() {
        let syntax = Syntax::new("code", "rust").word_wrap(Some(80));
        assert_eq!(syntax.word_wrap, Some(80));

        let syntax2 = Syntax::new("code", "rust").word_wrap(None);
        assert_eq!(syntax2.word_wrap, None);
    }

    #[test]
    fn test_indent_guides_place_guide_at_indent_start() {
        let syntax = Syntax::new("    x\n", "python")
            .line_numbers(true)
            .indent_guides(true)
            .tab_size(4);

        let text: String = syntax
            .render(None)
            .expect("render should succeed")
            .iter()
            .map(|s| s.text.as_ref())
            .collect();

        // Rich-style: 4 spaces of indent become "│   ".
        assert!(
            text.contains("│   x"),
            "expected indent guide to render as '│   x', got: {text:?}"
        );
    }

    #[test]
    fn test_word_wrap_preserves_whitespace_and_continuation_gutter() {
        let code = "def long():\n    x = 'this is a very long string that should wrap'\n";
        let syntax = Syntax::new(code, "python")
            .line_numbers(true)
            .indent_guides(true)
            .tab_size(4)
            // Wrap to a narrow width so the string literal is forced to wrap.
            .word_wrap(Some(36))
            .padding(0, 0);

        let text: String = syntax
            .render(None)
            .expect("render should succeed")
            .iter()
            .map(|s| s.text.as_ref())
            .collect();

        // Ensure we wrapped across a whitespace boundary without deleting the whitespace:
        // we should see a space immediately before a newline.
        assert!(
            text.contains("string \n"),
            "expected trailing whitespace to be preserved before wrap, got: {text:?}"
        );

        // Wrapped continuation lines should be aligned under the code column (spaces where the
        // line number gutter was).
        assert!(
            text.contains("\n    that should"),
            "expected wrapped continuation line to start with the gutter width, got: {text:?}"
        );
    }

    #[test]
    fn test_line_number_style_builder() {
        use crate::style::Attributes;

        let custom_style = Style::new()
            .bold()
            .color_str("cyan")
            .expect("cyan should be a valid color");
        let syntax = Syntax::new("code", "rust").line_number_style(custom_style);

        // The line number style should be set with bold attribute
        assert!(
            syntax
                .line_number_style
                .attributes
                .contains(Attributes::BOLD)
        );
    }

    #[test]
    fn test_custom_theme_and_syntax_loading_from_folder() {
        // Create a unique temp folder (we don't delete it; this avoids any ambiguity
        // about destructive filesystem operations in agent environments).
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("rich_rust_syntect_custom_{nonce}"));
        std::fs::create_dir_all(&root).expect("create temp dir");

        // Minimal tmTheme (plist) that syntect can parse.
        let theme_path = root.join("TestTheme.tmTheme");
        let theme = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>name</key>
  <string>TestTheme</string>
  <key>settings</key>
  <array>
    <dict>
      <key>settings</key>
      <dict>
        <key>background</key><string>#ffffff</string>
        <key>foreground</key><string>#000000</string>
      </dict>
    </dict>
  </array>
</dict>
</plist>
"#;
        std::fs::write(&theme_path, theme).expect("write theme");

        // Minimal sublime-syntax definition.
        let syntax_path = root.join("TestLang.sublime-syntax");
        let syntax_def = r"%YAML 1.2
---
name: TestLang
file_extensions:
  - testlang
scope: source.testlang
contexts:
  main:
    - match: '.+'
      scope: text.plain
...
";
        std::fs::write(&syntax_path, syntax_def).expect("write syntax");

        let theme_set = Syntax::load_themes_from_folder(&root).expect("load themes");
        assert!(
            theme_set.themes.contains_key("TestTheme"),
            "expected TestTheme in loaded theme set"
        );
        let syntax_set = Syntax::load_syntaxes_from_folder(&root).expect("load syntaxes");

        let code = "hello\n";
        let syntax = Syntax::new(code, "testlang")
            .syntax_set(syntax_set)
            .theme_set(theme_set)
            .theme("TestTheme");

        let rendered = syntax.render(Some(40)).expect("render with custom assets");
        let plain: String = rendered.iter().map(|s| s.text.as_ref()).collect();
        assert!(plain.contains("hello"));
    }
}
