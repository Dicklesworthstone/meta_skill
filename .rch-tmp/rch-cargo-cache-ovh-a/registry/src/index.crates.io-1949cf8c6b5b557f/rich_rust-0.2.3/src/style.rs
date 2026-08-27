//! Style system for terminal text attributes.
//!
//! This module provides the [`Style`] struct for representing visual attributes
//! including colors, text decorations (bold, italic, etc.), and hyperlinks.
//!
//! # Examples
//!
//! ```
//! use rich_rust::style::{Style, Attributes};
//! use rich_rust::color::{Color, ColorSystem};
//!
//! // Create a style using the builder pattern
//! let style = Style::new()
//!     .bold()
//!     .italic()
//!     .color(Color::from_ansi(1));  // Red
//!
//! // Render text with the style
//! let output = style.render("Hello!", ColorSystem::TrueColor);
//! assert!(output.contains("\x1b["));  // Contains ANSI codes
//!
//! // Parse a style from a string
//! let style = Style::parse("bold red on white").unwrap();
//! assert!(style.attributes.contains(Attributes::BOLD));
//! ```
//!
//! # Style Combination
//!
//! Styles can be combined using the `+` operator or [`Style::combine`], where the
//! right-hand style takes precedence for conflicting properties:
//!
//! ```
//! use rich_rust::style::Style;
//! use rich_rust::color::Color;
//!
//! let base = Style::new().bold().color(Color::from_ansi(1));
//! let highlight = Style::new().italic().color(Color::from_ansi(2));
//!
//! // highlight's color (green) overrides base's color (red)
//! let combined = base + highlight;
//! ```
//!
//! # Hyperlinks
//!
//! Styles support terminal hyperlinks via OSC 8 escape sequences:
//!
//! ```
//! use rich_rust::style::Style;
//! use rich_rust::color::ColorSystem;
//!
//! let style = Style::new()
//!     .bold()
//!     .link("https://example.com");
//!
//! let output = style.render("Click me", ColorSystem::TrueColor);
//! assert!(output.contains("\x1b]8;"));  // Contains OSC 8 hyperlink
//! ```

use bitflags::bitflags;
use lru::LruCache;
use smallvec::SmallVec;
use std::fmt::{self, Write as _};
use std::num::NonZeroUsize;
use std::str::FromStr;
use std::sync::{Arc, LazyLock, Mutex};

use crate::color::{Color, ColorParseError, ColorSystem, ColorTriplet, TerminalTheme, blend_rgb};
use crate::sync::lock_recover;

bitflags! {
    /// Text attribute flags.
    ///
    /// Each flag corresponds to an ANSI SGR (Select Graphic Rendition) code.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
    pub struct Attributes: u16 {
        /// Bold/bright text (SGR 1).
        const BOLD      = 1 << 0;
        /// Dim/faint text (SGR 2).
        const DIM       = 1 << 1;
        /// Italic text (SGR 3).
        const ITALIC    = 1 << 2;
        /// Single underline (SGR 4).
        const UNDERLINE = 1 << 3;
        /// Slow blinking text (SGR 5).
        const BLINK     = 1 << 4;
        /// Fast blinking text (SGR 6).
        const BLINK2    = 1 << 5;
        /// Reverse video (SGR 7).
        const REVERSE   = 1 << 6;
        /// Concealed/hidden text (SGR 8).
        const CONCEAL   = 1 << 7;
        /// Strikethrough text (SGR 9).
        const STRIKE    = 1 << 8;
        /// Double underline (SGR 21).
        const UNDERLINE2 = 1 << 9;
        /// Framed text (SGR 51).
        const FRAME     = 1 << 10;
        /// Encircled text (SGR 52).
        const ENCIRCLE  = 1 << 11;
        /// Overlined text (SGR 53).
        const OVERLINE  = 1 << 12;
    }
}

impl Attributes {
    /// Map of attribute flags to their ANSI SGR codes.
    const SGR_CODES: [(Self, u8); 13] = [
        (Self::BOLD, 1),
        (Self::DIM, 2),
        (Self::ITALIC, 3),
        (Self::UNDERLINE, 4),
        (Self::BLINK, 5),
        (Self::BLINK2, 6),
        (Self::REVERSE, 7),
        (Self::CONCEAL, 8),
        (Self::STRIKE, 9),
        (Self::UNDERLINE2, 21),
        (Self::FRAME, 51),
        (Self::ENCIRCLE, 52),
        (Self::OVERLINE, 53),
    ];

    /// Get the ANSI SGR codes for enabled attributes.
    /// Uses `SmallVec` to avoid heap allocation for typical 1-4 attribute cases.
    #[must_use]
    pub fn to_sgr_codes(&self) -> SmallVec<[u8; 4]> {
        Self::SGR_CODES
            .iter()
            .filter_map(|(attr, code)| {
                if self.contains(*attr) {
                    Some(*code)
                } else {
                    None
                }
            })
            .collect()
    }
}

/// Visual style for terminal text.
///
/// A `Style` represents the complete visual appearance of text including:
/// - Foreground and background colors
/// - Text attributes (bold, italic, etc.)
/// - Hyperlinks
///
/// Styles can be combined using the `+` operator, where the right-hand style
/// takes precedence for conflicting properties.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Style {
    /// Foreground color.
    pub color: Option<Color>,
    /// Background color.
    pub bgcolor: Option<Color>,
    /// Enabled attributes.
    pub attributes: Attributes,
    /// Which attributes are explicitly set (vs inherited).
    pub set_attributes: Attributes,
    /// URL for hyperlinks.
    pub link: Option<String>,
    /// Hyperlink ID for OSC 8 tracking/deduplication.
    /// If set, the OSC 8 sequence will include `id={link_id}`.
    pub link_id: Option<String>,
    /// Arbitrary metadata attached to this style.
    /// Used for storing custom data that doesn't affect rendering.
    pub meta: Option<Vec<u8>>,
    /// Whether this is a null/empty style.
    null: bool,
}

impl Style {
    /// Create an empty (null) style.
    #[must_use]
    pub fn null() -> Self {
        Self {
            null: true,
            ..Default::default()
        }
    }

    /// Create a new style builder.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if this is a null/empty style.
    #[must_use]
    pub const fn is_null(&self) -> bool {
        self.null
    }

    /// Convert this style to a CSS rule string for HTML export.
    ///
    /// Mirrors Python Rich's `Style.get_html_style(theme)`.
    #[must_use]
    pub fn get_html_style(&self, theme: TerminalTheme) -> String {
        let mut css: Vec<String> = Vec::new();

        let mut color = self.color.clone();
        let mut bgcolor = self.bgcolor.clone();

        if self.attributes.contains(Attributes::REVERSE) {
            std::mem::swap(&mut color, &mut bgcolor);
        }

        if self.attributes.contains(Attributes::DIM) {
            let foreground_color = match &color {
                None => theme.foreground_color,
                Some(c) => c.get_truecolor_with_theme(theme, true),
            };
            color = Some(Color::from_triplet(blend_rgb(
                foreground_color,
                theme.background_color,
                0.5,
            )));
        }

        if let Some(c) = &color {
            let theme_color = c.get_truecolor_with_theme(theme, true).hex();
            css.push(format!("color: {theme_color}"));
            css.push(format!("text-decoration-color: {theme_color}"));
        }
        if let Some(c) = &bgcolor {
            let theme_color = c.get_truecolor_with_theme(theme, false).hex();
            css.push(format!("background-color: {theme_color}"));
        }
        if self.attributes.contains(Attributes::BOLD) {
            css.push("font-weight: bold".to_string());
        }
        if self.attributes.contains(Attributes::ITALIC) {
            css.push("font-style: italic".to_string());
        }
        if self.attributes.contains(Attributes::UNDERLINE)
            || self.attributes.contains(Attributes::UNDERLINE2)
        {
            css.push("text-decoration: underline".to_string());
        }
        if self.attributes.contains(Attributes::STRIKE) {
            css.push("text-decoration: line-through".to_string());
        }
        if self.attributes.contains(Attributes::OVERLINE) {
            css.push("text-decoration: overline".to_string());
        }

        css.join("; ")
    }

    /// Convert this style to a CSS rule string for SVG export.
    ///
    /// Mirrors the `get_svg_style(style)` helper inside Python Rich's `Console.export_svg`.
    #[must_use]
    pub fn get_svg_style(&self, theme: TerminalTheme) -> String {
        let mut css_rules: Vec<String> = Vec::new();

        let mut color = match &self.color {
            None => theme.foreground_color,
            Some(c) if c.is_default() => theme.foreground_color,
            Some(c) => c.get_truecolor_with_theme(theme, true),
        };

        let mut bgcolor = match &self.bgcolor {
            None => theme.background_color,
            Some(c) if c.is_default() => theme.background_color,
            Some(c) => c.get_truecolor_with_theme(theme, false),
        };

        if self.attributes.contains(Attributes::REVERSE) {
            std::mem::swap(&mut color, &mut bgcolor);
        }

        if self.attributes.contains(Attributes::DIM) {
            color = blend_rgb(color, bgcolor, 0.4);
        }

        css_rules.push(format!("fill: {}", color.hex()));
        if self.attributes.contains(Attributes::BOLD) {
            css_rules.push("font-weight: bold".to_string());
        }
        if self.attributes.contains(Attributes::ITALIC) {
            css_rules.push("font-style: italic;".to_string());
        }
        if self.attributes.contains(Attributes::UNDERLINE)
            || self.attributes.contains(Attributes::UNDERLINE2)
        {
            css_rules.push("text-decoration: underline;".to_string());
        }
        if self.attributes.contains(Attributes::STRIKE) {
            css_rules.push("text-decoration: line-through;".to_string());
        }

        css_rules.join(";")
    }

    /// Set the foreground color.
    #[must_use]
    pub fn color(mut self, color: Color) -> Self {
        self.color = Some(color);
        self.null = false;
        self
    }

    /// Set the foreground color from a string.
    pub fn color_str(self, color: &str) -> Result<Self, StyleParseError> {
        let c = Color::parse(color)?;
        Ok(self.color(c))
    }

    /// Set the background color.
    #[must_use]
    pub fn bgcolor(mut self, color: Color) -> Self {
        self.bgcolor = Some(color);
        self.null = false;
        self
    }

    /// Set the background color from a string.
    pub fn bgcolor_str(self, color: &str) -> Result<Self, StyleParseError> {
        let c = Color::parse(color)?;
        Ok(self.bgcolor(c))
    }

    /// Enable bold text.
    #[must_use]
    pub fn bold(mut self) -> Self {
        self.attributes.insert(Attributes::BOLD);
        self.set_attributes.insert(Attributes::BOLD);
        self.null = false;
        self
    }

    /// Enable dim/faint text.
    #[must_use]
    pub fn dim(mut self) -> Self {
        self.attributes.insert(Attributes::DIM);
        self.set_attributes.insert(Attributes::DIM);
        self.null = false;
        self
    }

    /// Enable italic text.
    #[must_use]
    pub fn italic(mut self) -> Self {
        self.attributes.insert(Attributes::ITALIC);
        self.set_attributes.insert(Attributes::ITALIC);
        self.null = false;
        self
    }

    /// Enable underlined text.
    #[must_use]
    pub fn underline(mut self) -> Self {
        self.attributes.insert(Attributes::UNDERLINE);
        self.set_attributes.insert(Attributes::UNDERLINE);
        self.null = false;
        self
    }

    /// Enable blinking text.
    #[must_use]
    pub fn blink(mut self) -> Self {
        self.attributes.insert(Attributes::BLINK);
        self.set_attributes.insert(Attributes::BLINK);
        self.null = false;
        self
    }

    /// Enable reverse video.
    #[must_use]
    pub fn reverse(mut self) -> Self {
        self.attributes.insert(Attributes::REVERSE);
        self.set_attributes.insert(Attributes::REVERSE);
        self.null = false;
        self
    }

    /// Enable concealed/hidden text.
    #[must_use]
    pub fn conceal(mut self) -> Self {
        self.attributes.insert(Attributes::CONCEAL);
        self.set_attributes.insert(Attributes::CONCEAL);
        self.null = false;
        self
    }

    /// Enable strikethrough text.
    #[must_use]
    pub fn strike(mut self) -> Self {
        self.attributes.insert(Attributes::STRIKE);
        self.set_attributes.insert(Attributes::STRIKE);
        self.null = false;
        self
    }

    /// Enable overlined text.
    #[must_use]
    pub fn overline(mut self) -> Self {
        self.attributes.insert(Attributes::OVERLINE);
        self.set_attributes.insert(Attributes::OVERLINE);
        self.null = false;
        self
    }

    /// Set a hyperlink URL.
    #[must_use]
    pub fn link(mut self, url: impl Into<String>) -> Self {
        self.link = Some(url.into());
        self.null = false;
        self
    }

    /// Set a hyperlink URL with an explicit ID for tracking/deduplication.
    ///
    /// The ID is included in the OSC 8 sequence: `\x1b]8;id={link_id};{url}\x1b\\`
    #[must_use]
    pub fn link_with_id(mut self, url: impl Into<String>, id: impl Into<String>) -> Self {
        self.link = Some(url.into());
        self.link_id = Some(id.into());
        self.null = false;
        self
    }

    /// Set the hyperlink ID for OSC 8 tracking.
    ///
    /// This is useful when you want to set the ID separately from the URL.
    #[must_use]
    pub fn link_id(mut self, id: impl Into<String>) -> Self {
        self.link_id = Some(id.into());
        self.null = false;
        self
    }

    /// Set arbitrary metadata attached to this style.
    ///
    /// Metadata does not affect rendering and is used for storing custom data.
    #[must_use]
    pub fn meta(mut self, data: impl Into<Vec<u8>>) -> Self {
        self.meta = Some(data.into());
        self.null = false;
        self
    }

    /// Disable a specific attribute.
    #[must_use]
    pub fn not(mut self, attr: Attributes) -> Self {
        self.attributes.remove(attr);
        self.set_attributes.insert(attr);
        self.null = false;
        self
    }

    /// Combine this style with another, with the other style taking precedence.
    #[must_use]
    pub fn combine(&self, other: &Style) -> Style {
        if other.is_null() {
            return self.clone();
        }
        if self.is_null() {
            return other.clone();
        }

        Style {
            color: other.color.clone().or_else(|| self.color.clone()),
            bgcolor: other.bgcolor.clone().or_else(|| self.bgcolor.clone()),
            attributes: (self.attributes & !other.set_attributes)
                | (other.attributes & other.set_attributes),
            set_attributes: self.set_attributes | other.set_attributes,
            link: other.link.clone().or_else(|| self.link.clone()),
            link_id: other.link_id.clone().or_else(|| self.link_id.clone()),
            meta: other.meta.clone().or_else(|| self.meta.clone()),
            null: false,
        }
    }

    /// Generate ANSI escape codes for this style.
    #[must_use]
    pub fn make_ansi_codes(&self, color_system: ColorSystem) -> String {
        let mut result = String::new();
        self.make_ansi_codes_into(color_system, &mut result);
        result
    }

    /// Generate ANSI escape codes for this style, appending to an existing buffer.
    ///
    /// This is more efficient than [`Self::make_ansi_codes`] when you need to reuse buffers
    /// or avoid allocations in hot paths.
    pub fn make_ansi_codes_into(&self, color_system: ColorSystem, buf: &mut String) {
        use std::fmt::Write;

        let mut first = true;

        // Add attribute codes
        for code in self.attributes.to_sgr_codes() {
            if !first {
                buf.push(';');
            }
            // write! to String is infallible
            let _ = write!(buf, "{code}");
            first = false;
        }

        // Add foreground color codes
        if let Some(color) = &self.color {
            let downgraded = color.downgrade(color_system);
            for code in downgraded.get_ansi_codes(true) {
                if !first {
                    buf.push(';');
                }
                buf.push_str(&code);
                first = false;
            }
        }

        // Add background color codes
        if let Some(bgcolor) = &self.bgcolor {
            let downgraded = bgcolor.downgrade(color_system);
            for code in downgraded.get_ansi_codes(false) {
                if !first {
                    buf.push(';');
                }
                buf.push_str(&code);
                first = false;
            }
        }
    }

    /// Render text with this style applied.
    #[must_use]
    pub fn render(&self, text: &str, color_system: ColorSystem) -> String {
        use std::fmt::Write;

        if self.is_null() {
            return text.to_string();
        }

        let codes = self.make_ansi_codes(color_system);
        let has_link = self.link.is_some();

        // Early return only if no codes AND no link
        if codes.is_empty() && !has_link {
            return text.to_string();
        }

        // Estimate capacity: text + codes + ANSI overhead + link overhead
        let link_overhead = self.link.as_ref().map_or(0, |l| l.len() + 30);
        let mut result = String::with_capacity(text.len() + codes.len() + 20 + link_overhead);

        // Handle hyperlinks (OSC 8)
        // Format: \x1b]8;{params};{url}\x1b\\ where params can include id={link_id}
        if let Some(link) = &self.link {
            result.push_str("\x1b]8;");
            if let Some(id) = &self.link_id {
                let _ = write!(result, "id={id}");
            }
            result.push(';');
            result.push_str(link);
            result.push_str("\x1b\\");
        }

        // Apply style (only if there are codes)
        if !codes.is_empty() {
            result.push_str("\x1b[");
            result.push_str(&codes);
            result.push('m');
        }
        result.push_str(text);
        if !codes.is_empty() {
            result.push_str("\x1b[0m");
        }

        // Close hyperlink
        if has_link {
            result.push_str("\x1b]8;;\x1b\\");
        }

        result
    }

    /// Render ANSI escape codes for this style (cached).
    ///
    /// Returns an `Arc` containing (prefix, suffix) where:
    /// - prefix: ANSI codes to apply the style
    /// - suffix: ANSI codes to reset the style
    ///
    /// Results are cached for performance when the same style is rendered repeatedly.
    #[must_use]
    #[expect(
        clippy::items_after_statements,
        reason = "static cache placed close to usage for clarity"
    )]
    #[expect(
        clippy::type_complexity,
        reason = "LRU cache type is inherently complex"
    )]
    pub fn render_ansi(&self, color_system: ColorSystem) -> Arc<(String, String)> {
        // Fast path: null style returns empty strings without cache lookup
        if self.is_null() {
            static EMPTY: LazyLock<Arc<(String, String)>> =
                LazyLock::new(|| Arc::new((String::new(), String::new())));
            return EMPTY.clone();
        }

        // Cache key is (Style, ColorSystem) since ANSI output varies by color system
        static ANSI_CACHE: LazyLock<Mutex<LruCache<(Style, ColorSystem), Arc<(String, String)>>>> =
            LazyLock::new(|| Mutex::new(LruCache::new(NonZeroUsize::new(256).expect("non-zero"))));

        // Try to get cached result
        {
            let mut cache = lock_recover(&ANSI_CACHE);
            if let Some(cached) = cache.get(&(self.clone(), color_system)) {
                return cached.clone();
            }
        }

        // Compute result
        let result = Arc::new(self.render_ansi_uncached(color_system));

        // Cache the result
        lock_recover(&ANSI_CACHE).put((self.clone(), color_system), result.clone());

        result
    }

    /// Render ANSI escape codes without caching (internal implementation).
    fn render_ansi_uncached(&self, color_system: ColorSystem) -> (String, String) {
        let codes = self.make_ansi_codes(color_system);
        if codes.is_empty() && self.link.is_none() {
            return (String::new(), String::new());
        }

        let mut prefix = String::new();
        let suffix;

        // Handle hyperlinks (OSC 8)
        // Format: \x1b]8;{params};{url}\x1b\\ where params can include id={link_id}
        if let Some(link) = &self.link {
            let params = self
                .link_id
                .as_ref()
                .map_or(String::new(), |id| format!("id={id}"));
            let _ = write!(prefix, "\x1b]8;{params};{link}\x1b\\");
        }

        // Apply style (only if there are codes)
        if !codes.is_empty() {
            let _ = write!(prefix, "\x1b[{codes}m");
        }

        // Build suffix
        if self.link.is_some() {
            if codes.is_empty() {
                suffix = String::from("\x1b]8;;\x1b\\");
            } else {
                suffix = String::from("\x1b[0m\x1b]8;;\x1b\\");
            }
        } else {
            suffix = String::from("\x1b[0m");
        }

        (prefix, suffix)
    }

    /// Parse a style from a string (cached).
    ///
    /// Supported formats:
    /// - Empty/none: `""`, `"none"` -> null style
    /// - Attribute: `"bold"`, `"italic"`, `"underline"`
    /// - Negative: `"not bold"`
    /// - Color: `"red"`, `"#ff0000"`
    /// - Background: `"on red"`, `"on #ff0000"`
    /// - Link: `"link https://..."`
    /// - Combined: `"bold red on white"`
    pub fn parse(style: &str) -> Result<Self, StyleParseError> {
        static CACHE: LazyLock<Mutex<LruCache<String, Style>>> =
            LazyLock::new(|| Mutex::new(LruCache::new(NonZeroUsize::new(512).expect("non-zero"))));

        let normalized = style.trim().to_lowercase();

        {
            let mut cache = lock_recover(&CACHE);
            if let Some(cached) = cache.get(&normalized) {
                return Ok(cached.clone());
            }
        }

        let result = Self::parse_uncached(&normalized)?;

        lock_recover(&CACHE).put(normalized, result.clone());

        Ok(result)
    }

    /// Normalize a style definition to a canonical string form.
    ///
    /// Mirrors Python Rich's `Style.normalize()` behavior:
    /// - If the definition parses, return `Style::parse(def).to_string()`.
    /// - If it doesn't parse, return the lowercased, trimmed input.
    #[must_use]
    pub fn normalize(style: &str) -> String {
        match Self::parse(style) {
            Ok(parsed) => parsed.to_string(),
            Err(_) => style.trim().to_lowercase(),
        }
    }

    fn parse_uncached(style: &str) -> Result<Self, StyleParseError> {
        if style.is_empty() || style == "none" {
            return Ok(Self::null());
        }

        let mut result = Style::new();
        let words: Vec<&str> = style.split_whitespace().collect();
        let mut i = 0;

        while i < words.len() {
            let word = words[i];

            // Handle "not <attribute>"
            if word == "not" {
                if i + 1 >= words.len() {
                    return Err(StyleParseError::InvalidFormat(
                        "'not' requires an attribute".to_string(),
                    ));
                }
                i += 1;
                let attr_name = words[i];
                if let Some(attr) = parse_attribute(attr_name) {
                    result = result.not(attr);
                } else {
                    return Err(StyleParseError::UnknownAttribute(attr_name.to_string()));
                }
                i += 1;
                continue;
            }

            // Handle "on <color>" for background
            if word == "on" {
                if i + 1 >= words.len() {
                    return Err(StyleParseError::InvalidFormat(
                        "'on' requires a color".to_string(),
                    ));
                }
                i += 1;
                let color_name = words[i];
                result = result.bgcolor_str(color_name)?;
                i += 1;
                continue;
            }

            // Handle "link <url>"
            if word == "link" {
                if i + 1 >= words.len() {
                    return Err(StyleParseError::InvalidFormat(
                        "'link' requires a URL".to_string(),
                    ));
                }
                i += 1;
                result = result.link(words[i]);
                i += 1;
                continue;
            }

            // Try as attribute
            if let Some(attr) = parse_attribute(word) {
                match attr {
                    Attributes::BOLD => result = result.bold(),
                    Attributes::DIM => result = result.dim(),
                    Attributes::ITALIC => result = result.italic(),
                    Attributes::UNDERLINE => result = result.underline(),
                    Attributes::BLINK => result = result.blink(),
                    Attributes::REVERSE => result = result.reverse(),
                    Attributes::CONCEAL => result = result.conceal(),
                    Attributes::STRIKE => result = result.strike(),
                    Attributes::OVERLINE => result = result.overline(),
                    // Attributes without dedicated builder methods
                    Attributes::BLINK2
                    | Attributes::UNDERLINE2
                    | Attributes::FRAME
                    | Attributes::ENCIRCLE => {
                        result.attributes.insert(attr);
                        result.set_attributes.insert(attr);
                        result.null = false;
                    }
                    _ => {}
                }
                i += 1;
                continue;
            }

            // Try as foreground color
            if Color::parse(word).is_ok() {
                result = result.color_str(word)?;
                i += 1;
                continue;
            }

            return Err(StyleParseError::UnknownToken(word.to_string()));
        }

        Ok(result)
    }
}

impl std::ops::Add for Style {
    type Output = Style;

    fn add(self, rhs: Self) -> Self::Output {
        self.combine(&rhs)
    }
}

impl std::ops::Add<&Style> for Style {
    type Output = Style;

    fn add(self, rhs: &Self) -> Self::Output {
        self.combine(rhs)
    }
}

impl std::ops::Add<Style> for &Style {
    type Output = Style;

    fn add(self, rhs: Style) -> Self::Output {
        self.combine(&rhs)
    }
}

impl std::ops::Add<&Style> for &Style {
    type Output = Style;

    fn add(self, rhs: &Style) -> Self::Output {
        self.combine(rhs)
    }
}

impl fmt::Display for Style {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_null() {
            return write!(f, "none");
        }

        let mut parts = Vec::new();

        // Add attributes
        for (attr, name) in [
            (Attributes::BOLD, "bold"),
            (Attributes::DIM, "dim"),
            (Attributes::ITALIC, "italic"),
            (Attributes::UNDERLINE, "underline"),
            (Attributes::BLINK, "blink"),
            (Attributes::REVERSE, "reverse"),
            (Attributes::CONCEAL, "conceal"),
            (Attributes::STRIKE, "strike"),
            (Attributes::OVERLINE, "overline"),
        ] {
            if self.attributes.contains(attr) {
                parts.push(name.to_string());
            }
        }

        // Add foreground color
        if let Some(color) = &self.color {
            parts.push(color.to_string());
        }

        // Add background color
        if let Some(bgcolor) = &self.bgcolor {
            parts.push(format!("on {bgcolor}"));
        }

        // Add link with optional id
        if let Some(link) = &self.link {
            if let Some(id) = &self.link_id {
                parts.push(format!("link[{id}] {link}"));
            } else {
                parts.push(format!("link {link}"));
            }
        }

        write!(f, "{}", parts.join(" "))
    }
}

impl FromStr for Style {
    type Err = StyleParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<&str> for Style {
    type Error = StyleParseError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl TryFrom<String> for Style {
    type Error = StyleParseError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(value.as_str())
    }
}

impl From<Color> for Style {
    fn from(color: Color) -> Self {
        Self::new().color(color)
    }
}

impl From<ColorTriplet> for Style {
    fn from(triplet: ColorTriplet) -> Self {
        Self::new().color(Color::from(triplet))
    }
}

impl From<(u8, u8, u8)> for Style {
    fn from((red, green, blue): (u8, u8, u8)) -> Self {
        Self::new().color(Color::from((red, green, blue)))
    }
}

impl From<[u8; 3]> for Style {
    fn from([red, green, blue]: [u8; 3]) -> Self {
        Self::new().color(Color::from((red, green, blue)))
    }
}

/// Parse an attribute name to its flag.
fn parse_attribute(name: &str) -> Option<Attributes> {
    match name {
        "bold" | "b" => Some(Attributes::BOLD),
        "dim" | "d" => Some(Attributes::DIM),
        "italic" | "i" => Some(Attributes::ITALIC),
        "underline" | "u" => Some(Attributes::UNDERLINE),
        "blink" => Some(Attributes::BLINK),
        "blink2" => Some(Attributes::BLINK2),
        "reverse" | "r" => Some(Attributes::REVERSE),
        "conceal" | "c" => Some(Attributes::CONCEAL),
        "strike" | "s" => Some(Attributes::STRIKE),
        "underline2" | "uu" => Some(Attributes::UNDERLINE2),
        "frame" => Some(Attributes::FRAME),
        "encircle" => Some(Attributes::ENCIRCLE),
        "overline" | "o" => Some(Attributes::OVERLINE),
        _ => None,
    }
}

/// Style stack for nested style application.
#[derive(Debug, Clone)]
pub struct StyleStack {
    stack: Vec<Style>,
}

impl StyleStack {
    /// Create a new style stack with a default base style.
    #[must_use]
    pub fn new(default: Style) -> Self {
        Self {
            stack: vec![default],
        }
    }

    /// Get the current combined style.
    #[must_use]
    pub fn current(&self) -> &Style {
        self.stack.last().expect("stack should never be empty")
    }

    /// Push a new style onto the stack, combining with current.
    #[expect(
        clippy::needless_pass_by_value,
        reason = "style ownership simplifies API"
    )]
    pub fn push(&mut self, style: Style) {
        let combined = self.current().combine(&style);
        self.stack.push(combined);
    }

    /// Pop the most recent style from the stack.
    pub fn pop(&mut self) -> &Style {
        if self.stack.len() > 1 {
            self.stack.pop();
        }
        self.current()
    }

    /// Get the depth of the stack.
    #[must_use]
    pub fn len(&self) -> usize {
        self.stack.len()
    }

    /// Check if the stack is empty (only base style).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.stack.len() <= 1
    }
}

impl Default for StyleStack {
    fn default() -> Self {
        Self::new(Style::null())
    }
}

/// Error type for style parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StyleParseError {
    InvalidFormat(String),
    UnknownAttribute(String),
    UnknownToken(String),
    ColorError(ColorParseError),
}

impl fmt::Display for StyleParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFormat(s) => write!(f, "Invalid style format: {s}"),
            Self::UnknownAttribute(s) => write!(f, "Unknown attribute: {s}"),
            Self::UnknownToken(s) => write!(f, "Unknown token: {s}"),
            Self::ColorError(e) => write!(f, "Color error: {e}"),
        }
    }
}

impl std::error::Error for StyleParseError {}

impl From<ColorParseError> for StyleParseError {
    fn from(err: ColorParseError) -> Self {
        Self::ColorError(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_attributes_sgr_codes() {
        let attrs = Attributes::BOLD | Attributes::ITALIC;
        let codes = attrs.to_sgr_codes();
        assert!(codes.contains(&1));
        assert!(codes.contains(&3));
    }

    #[test]
    fn test_style_null() {
        let style = Style::null();
        assert!(style.is_null());
    }

    #[test]
    fn test_style_builder() {
        let style = Style::new().bold().italic().color(Color::from_ansi(1));

        assert!(style.attributes.contains(Attributes::BOLD));
        assert!(style.attributes.contains(Attributes::ITALIC));
        assert!(style.color.is_some());
    }

    #[test]
    fn test_style_combine() {
        let style1 = Style::new().bold().color(Color::from_ansi(1));
        let style2 = Style::new().italic().color(Color::from_ansi(2));

        let combined = style1.combine(&style2);

        assert!(combined.attributes.contains(Attributes::BOLD));
        assert!(combined.attributes.contains(Attributes::ITALIC));
        // style2's color should override
        assert_eq!(combined.color.unwrap().number, Some(2));
    }

    #[test]
    fn test_style_combine_null() {
        let style = Style::new().bold();
        let null = Style::null();

        assert_eq!(style.combine(&null), style);
        assert_eq!(null.combine(&style), style);
    }

    #[test]
    fn test_style_parse_simple() {
        let style = Style::parse("bold").unwrap();
        assert!(style.attributes.contains(Attributes::BOLD));
    }

    #[test]
    fn test_style_parse_color() {
        let style = Style::parse("red").unwrap();
        assert!(style.color.is_some());
    }

    #[test]
    fn test_style_parse_background() {
        let style = Style::parse("on blue").unwrap();
        assert!(style.bgcolor.is_some());
    }

    #[test]
    fn test_style_parse_combined() {
        let style = Style::parse("bold red on white").unwrap();
        assert!(style.attributes.contains(Attributes::BOLD));
        assert!(style.color.is_some());
        assert!(style.bgcolor.is_some());
    }

    #[test]
    fn test_style_parse_not() {
        let style = Style::parse("not bold").unwrap();
        assert!(style.set_attributes.contains(Attributes::BOLD));
        assert!(!style.attributes.contains(Attributes::BOLD));
    }

    #[test]
    fn test_style_parse_link() {
        let style = Style::parse("link https://example.com").unwrap();
        assert_eq!(style.link, Some("https://example.com".to_string()));
    }

    #[test]
    fn test_style_render() {
        let style = Style::new().bold();
        let rendered = style.render("test", ColorSystem::TrueColor);
        assert!(rendered.contains("\x1b[1m"));
        assert!(rendered.contains("\x1b[0m"));
    }

    #[test]
    fn test_style_stack() {
        let mut stack = StyleStack::new(Style::null());

        stack.push(Style::new().bold());
        assert!(stack.current().attributes.contains(Attributes::BOLD));

        stack.push(Style::new().italic());
        assert!(stack.current().attributes.contains(Attributes::BOLD));
        assert!(stack.current().attributes.contains(Attributes::ITALIC));

        stack.pop();
        assert!(stack.current().attributes.contains(Attributes::BOLD));
        assert!(!stack.current().attributes.contains(Attributes::ITALIC));
    }

    #[test]
    fn test_style_add_operator() {
        let s1 = Style::new().bold();
        let s2 = Style::new().italic();
        let combined = s1 + s2;

        assert!(combined.attributes.contains(Attributes::BOLD));
        assert!(combined.attributes.contains(Attributes::ITALIC));
    }

    // --- Additional Comprehensive Tests ---

    #[test]
    fn test_style_combine_associativity() {
        // (a + b) + c should equal a + (b + c)
        let a = Style::new().bold();
        let b = Style::new().italic().color(Color::from_ansi(1));
        let c = Style::new().underline().bgcolor(Color::from_ansi(4));

        let left = (a.clone() + b.clone()) + c.clone();
        let right = a + (b + c);

        assert_eq!(left.attributes, right.attributes);
        assert_eq!(left.color, right.color);
        assert_eq!(left.bgcolor, right.bgcolor);
    }

    #[test]
    fn test_style_parse_invalid_unknown_token() {
        let result = Style::parse("invalid_style_word");
        assert!(matches!(
            result,
            Err(StyleParseError::UnknownToken(ref unknown))
                if unknown == "invalid_style_word"
        ));
    }

    #[test]
    fn test_style_parse_invalid_not_without_attribute() {
        let result = Style::parse("not");
        assert!(matches!(
            result,
            Err(StyleParseError::InvalidFormat(ref msg)) if msg.contains("requires an attribute")
        ));
    }

    #[test]
    fn test_style_parse_invalid_on_without_color() {
        let result = Style::parse("on");
        assert!(matches!(
            result,
            Err(StyleParseError::InvalidFormat(ref msg)) if msg.contains("requires a color")
        ));
    }

    #[test]
    fn test_style_parse_empty_is_null() {
        let style = Style::parse("").unwrap();
        assert!(style.is_null());
    }

    #[test]
    fn test_style_parse_none_is_null() {
        let style = Style::parse("none").unwrap();
        assert!(style.is_null());
    }

    #[test]
    fn test_style_render_null_returns_text_unchanged() {
        let style = Style::null();
        let rendered = style.render("hello", ColorSystem::TrueColor);
        assert_eq!(rendered, "hello");
    }

    #[test]
    fn test_style_render_foreground_color_truecolor() {
        let style = Style::new().color(Color::from_rgb(255, 0, 0));
        let rendered = style.render("text", ColorSystem::TrueColor);
        assert!(rendered.contains("\x1b["));
        assert!(rendered.contains("38;2;255;0;0"));
    }

    #[test]
    fn test_style_render_background_color_truecolor() {
        let style = Style::new().bgcolor(Color::from_rgb(0, 255, 0));
        let rendered = style.render("text", ColorSystem::TrueColor);
        assert!(rendered.contains("\x1b["));
        assert!(rendered.contains("48;2;0;255;0"));
    }

    #[test]
    fn test_style_render_foreground_color_256() {
        let style = Style::new().color(Color::from_ansi(196));
        let rendered = style.render("text", ColorSystem::EightBit);
        assert!(rendered.contains("\x1b["));
        assert!(rendered.contains("38;5;196"));
    }

    #[test]
    fn test_style_render_combined_attributes_and_colors() {
        let style = Style::new()
            .bold()
            .italic()
            .color(Color::from_ansi(1))
            .bgcolor(Color::from_ansi(4));
        let rendered = style.render("text", ColorSystem::TrueColor);

        // Should contain SGR codes for bold (1), italic (3)
        assert!(rendered.contains('1'));
        assert!(rendered.contains('3'));
        // Should contain reset at end
        assert!(rendered.contains("\x1b[0m"));
    }

    #[test]
    fn test_style_render_with_hyperlink() {
        let style = Style::new().link("https://example.com");
        let rendered = style.render("click", ColorSystem::TrueColor);

        // Should contain OSC 8 opening and closing sequences
        assert!(rendered.contains("\x1b]8;;https://example.com"));
        assert!(rendered.contains("\x1b]8;;\x1b\\"));
    }

    #[test]
    fn test_style_all_attributes() {
        // Test each attribute individually
        let bold = Style::new().bold();
        let dim = Style::new().dim();
        let italic = Style::new().italic();
        let underline = Style::new().underline();
        let blink = Style::new().blink();
        let reverse = Style::new().reverse();
        let conceal = Style::new().conceal();
        let strike = Style::new().strike();
        let overline = Style::new().overline();

        assert!(bold.attributes.contains(Attributes::BOLD));
        assert!(dim.attributes.contains(Attributes::DIM));
        assert!(italic.attributes.contains(Attributes::ITALIC));
        assert!(underline.attributes.contains(Attributes::UNDERLINE));
        assert!(blink.attributes.contains(Attributes::BLINK));
        assert!(reverse.attributes.contains(Attributes::REVERSE));
        assert!(conceal.attributes.contains(Attributes::CONCEAL));
        assert!(strike.attributes.contains(Attributes::STRIKE));
        assert!(overline.attributes.contains(Attributes::OVERLINE));
    }

    #[test]
    fn test_style_not_removes_attribute() {
        let style = Style::new().bold().not(Attributes::BOLD);
        assert!(!style.attributes.contains(Attributes::BOLD));
        assert!(style.set_attributes.contains(Attributes::BOLD));
    }

    #[test]
    fn test_style_display() {
        let style = Style::new().bold().italic();
        let display = format!("{style}");
        assert!(display.contains("bold"));
        assert!(display.contains("italic"));
    }

    #[test]
    fn test_style_display_null() {
        let style = Style::null();
        assert_eq!(format!("{style}"), "none");
    }

    #[test]
    fn test_style_from_color() {
        let style: Style = Color::from_ansi(1).into();
        assert!(style.color.is_some());
        assert_eq!(style.color.unwrap().number, Some(1));
    }

    #[test]
    fn test_style_from_tuple() {
        let style: Style = (255u8, 128u8, 0u8).into();
        assert!(style.color.is_some());
    }

    #[test]
    fn test_style_parse_hex_color() {
        let style = Style::parse("#ff0000").unwrap();
        assert!(style.color.is_some());
    }

    #[test]
    fn test_style_parse_attribute_aliases() {
        // Test short aliases
        let bold = Style::parse("b").unwrap();
        let dim = Style::parse("d").unwrap();
        let italic = Style::parse("i").unwrap();
        let underline = Style::parse("u").unwrap();

        assert!(bold.attributes.contains(Attributes::BOLD));
        assert!(dim.attributes.contains(Attributes::DIM));
        assert!(italic.attributes.contains(Attributes::ITALIC));
        assert!(underline.attributes.contains(Attributes::UNDERLINE));
    }

    #[test]
    fn test_attributes_empty() {
        let attrs = Attributes::empty();
        assert!(attrs.to_sgr_codes().is_empty());
    }

    #[test]
    fn test_style_render_ansi_tuple() {
        let style = Style::new().bold();
        let ansi = style.render_ansi(ColorSystem::TrueColor);
        let (prefix, suffix) = &*ansi;

        assert!(prefix.contains("\x1b[1m"));
        assert!(suffix.contains("\x1b[0m"));
    }

    #[test]
    fn test_style_render_ansi_with_link() {
        let style = Style::new().bold().link("https://test.com");
        let ansi = style.render_ansi(ColorSystem::TrueColor);
        let (prefix, suffix) = &*ansi;

        assert!(prefix.contains("\x1b]8;;https://test.com"));
        assert!(suffix.contains("\x1b]8;;\x1b\\"));
    }

    #[test]
    fn test_style_render_ansi_with_link_id() {
        let style = Style::new()
            .bold()
            .link_with_id("https://example.com", "test-id");
        let rendered = style.render("click here", ColorSystem::TrueColor);

        // Should contain OSC 8 sequences
        assert!(rendered.contains("\x1b]8;id=test-id;https://example.com\x1b\\"));
        assert!(rendered.contains("click here"));
        assert!(rendered.contains("\x1b]8;;\x1b\\"));
        // Should contain SGR reset since it is bold
        assert!(rendered.contains("\x1b[0m"));
    }

    #[test]
    fn test_style_render_ansi_link_only_with_id() {
        // Link only (no other attributes) with id
        let style = Style::new().link_with_id("https://test.com", "solo-id");
        let ansi = style.render_ansi(ColorSystem::TrueColor);
        let (prefix, suffix) = &*ansi;

        assert!(prefix.contains("\x1b]8;id=solo-id;https://test.com\x1b\\"));
        // No SGR codes, so prefix shouldn't have \x1b[...m
        assert!(!prefix.contains("\x1b["));
        // Suffix should just close hyperlink, no reset needed
        assert_eq!(suffix, "\x1b]8;;\x1b\\");
    }

    #[test]
    fn test_style_render_ansi_null() {
        let style = Style::null();
        let ansi = style.render_ansi(ColorSystem::TrueColor);
        let (prefix, suffix) = &*ansi;
        assert_eq!(prefix, "");
        assert_eq!(suffix, "");
    }

    #[test]
    fn test_style_render_ansi_empty_codes() {
        // Style with no attributes and no colors
        let style = Style::new();
        let ansi = style.render_ansi(ColorSystem::TrueColor);
        let (prefix, suffix) = &*ansi;
        assert_eq!(prefix, "");
        assert_eq!(suffix, "");
    }

    #[test]
    fn test_style_stack_empty() {
        let stack = StyleStack::default();
        assert!(stack.is_empty());
        assert_eq!(stack.len(), 1); // Base style always exists
    }

    #[test]
    fn test_style_parse_caching() {
        // Parse same style twice - should return same result (cached)
        let style1 = Style::parse("bold red").unwrap();
        let style2 = Style::parse("bold red").unwrap();
        assert_eq!(style1, style2);
    }

    #[test]
    fn test_style_render_ansi_caching() {
        // Test that render_ansi caching works correctly
        let style = Style::new().bold().color(Color::from_ansi(1));

        // First call populates cache
        let ansi1 = style.render_ansi(ColorSystem::TrueColor);
        let (prefix1, suffix1) = &*ansi1;

        // Second call should return cached result
        let ansi2 = style.render_ansi(ColorSystem::TrueColor);
        let (prefix2, suffix2) = &*ansi2;

        assert_eq!(prefix1, prefix2);
        assert_eq!(suffix1, suffix2);

        // Different color system should produce different result
        let ansi_8bit = style.render_ansi(ColorSystem::EightBit);
        let (prefix_8bit, _suffix_8bit) = &*ansi_8bit;
        // The prefix should still contain the style codes
        assert!(prefix_8bit.contains("\x1b[1m") || prefix_8bit.contains("1;"));

        // Verify result is correct
        assert!(prefix1.contains("\x1b["));
        assert!(suffix1.contains("\x1b[0m"));
    }

    #[test]
    fn test_style_render_ansi_caching_different_styles() {
        // Verify that different styles produce different cached results
        let bold = Style::new().bold();
        let italic = Style::new().italic();

        let bold_ansi = bold.render_ansi(ColorSystem::TrueColor);
        let (bold_prefix, _) = &*bold_ansi;
        let italic_ansi = italic.render_ansi(ColorSystem::TrueColor);
        let (italic_prefix, _) = &*italic_ansi;

        assert_ne!(bold_prefix, italic_prefix);
        assert!(bold_prefix.contains("1m")); // SGR 1 for bold
        assert!(italic_prefix.contains("3m")); // SGR 3 for italic
    }

    // --- Additional Tests for 100% Coverage ---

    #[test]
    fn test_all_attributes_sgr_codes() {
        // Test each attribute produces correct SGR code
        assert_eq!(Attributes::BOLD.to_sgr_codes().as_slice(), &[1]);
        assert_eq!(Attributes::DIM.to_sgr_codes().as_slice(), &[2]);
        assert_eq!(Attributes::ITALIC.to_sgr_codes().as_slice(), &[3]);
        assert_eq!(Attributes::UNDERLINE.to_sgr_codes().as_slice(), &[4]);
        assert_eq!(Attributes::BLINK.to_sgr_codes().as_slice(), &[5]);
        assert_eq!(Attributes::BLINK2.to_sgr_codes().as_slice(), &[6]);
        assert_eq!(Attributes::REVERSE.to_sgr_codes().as_slice(), &[7]);
        assert_eq!(Attributes::CONCEAL.to_sgr_codes().as_slice(), &[8]);
        assert_eq!(Attributes::STRIKE.to_sgr_codes().as_slice(), &[9]);
        assert_eq!(Attributes::UNDERLINE2.to_sgr_codes().as_slice(), &[21]);
        assert_eq!(Attributes::FRAME.to_sgr_codes().as_slice(), &[51]);
        assert_eq!(Attributes::ENCIRCLE.to_sgr_codes().as_slice(), &[52]);
        assert_eq!(Attributes::OVERLINE.to_sgr_codes().as_slice(), &[53]);
    }

    #[test]
    fn test_style_parse_blink2_frame_encircle() {
        // Test attributes without dedicated builder methods
        let blink2 = Style::parse("blink2").unwrap();
        assert!(blink2.attributes.contains(Attributes::BLINK2));

        let underline2 = Style::parse("underline2").unwrap();
        assert!(underline2.attributes.contains(Attributes::UNDERLINE2));

        let frame = Style::parse("frame").unwrap();
        assert!(frame.attributes.contains(Attributes::FRAME));

        let encircle = Style::parse("encircle").unwrap();
        assert!(encircle.attributes.contains(Attributes::ENCIRCLE));
    }

    #[test]
    fn test_style_parse_short_aliases() {
        // Test all short aliases
        assert!(
            Style::parse("r")
                .unwrap()
                .attributes
                .contains(Attributes::REVERSE)
        );
        assert!(
            Style::parse("c")
                .unwrap()
                .attributes
                .contains(Attributes::CONCEAL)
        );
        assert!(
            Style::parse("s")
                .unwrap()
                .attributes
                .contains(Attributes::STRIKE)
        );
        assert!(
            Style::parse("o")
                .unwrap()
                .attributes
                .contains(Attributes::OVERLINE)
        );
        assert!(
            Style::parse("uu")
                .unwrap()
                .attributes
                .contains(Attributes::UNDERLINE2)
        );
    }

    #[test]
    fn test_style_fromstr_trait() {
        use std::str::FromStr;

        let style: Style = "bold red".parse().unwrap();
        assert!(style.attributes.contains(Attributes::BOLD));
        assert!(style.color.is_some());

        let style2 = Style::from_str("italic blue").unwrap();
        assert!(style2.attributes.contains(Attributes::ITALIC));
    }

    #[test]
    fn test_style_tryfrom_str() {
        let style: Style = Style::try_from("bold").unwrap();
        assert!(style.attributes.contains(Attributes::BOLD));
    }

    #[test]
    fn test_style_tryfrom_string() {
        let style: Style = Style::try_from(String::from("italic")).unwrap();
        assert!(style.attributes.contains(Attributes::ITALIC));
    }

    #[test]
    fn test_style_from_color_triplet() {
        let triplet = ColorTriplet::new(100, 150, 200);
        let style: Style = triplet.into();
        assert!(style.color.is_some());
        let color = style.color.unwrap();
        assert_eq!(color.triplet, Some(ColorTriplet::new(100, 150, 200)));
    }

    #[test]
    fn test_style_from_array() {
        let style: Style = [255u8, 128u8, 64u8].into();
        assert!(style.color.is_some());
    }

    #[test]
    fn test_style_make_ansi_codes() {
        let style = Style::new().bold().italic().color(Color::from_ansi(1));

        let codes = style.make_ansi_codes(ColorSystem::TrueColor);
        assert!(codes.contains('1')); // Bold
        assert!(codes.contains('3')); // Italic
    }

    #[test]
    fn test_style_make_ansi_codes_empty() {
        let style = Style::new();
        let codes = style.make_ansi_codes(ColorSystem::TrueColor);
        assert_eq!(codes, "");
    }

    #[test]
    fn test_style_stack_multiple_operations() {
        let mut stack = StyleStack::new(Style::null());
        assert!(stack.is_empty());

        stack.push(Style::new().bold());
        stack.push(Style::new().italic());
        stack.push(Style::new().underline());
        assert_eq!(stack.len(), 4); // Base + 3

        stack.pop();
        assert!(stack.current().attributes.contains(Attributes::BOLD));
        assert!(stack.current().attributes.contains(Attributes::ITALIC));

        stack.pop();
        stack.pop();
        // Should stop at base
        stack.pop();
        stack.pop();
        assert!(stack.current().is_null());
    }

    #[test]
    fn test_style_combine_attribute_inheritance() {
        // Test that set_attributes properly tracks what's explicitly set
        let bold = Style::new().bold();
        let not_bold = Style::new().not(Attributes::BOLD);

        // Combining bold + not_bold should result in not bold
        // because not_bold explicitly sets BOLD to off
        let combined = bold.combine(&not_bold);
        assert!(!combined.attributes.contains(Attributes::BOLD));
        assert!(combined.set_attributes.contains(Attributes::BOLD));
    }

    #[test]
    fn test_style_add_with_refs() {
        let s1 = Style::new().bold();
        let s2 = Style::new().italic();

        // Test &Style + &Style
        let c1 = &s1 + &s2;
        assert!(c1.attributes.contains(Attributes::BOLD));
        assert!(c1.attributes.contains(Attributes::ITALIC));

        // Test Style + &Style
        let c2 = s1.clone() + &s2;
        assert!(c2.attributes.contains(Attributes::BOLD));

        // Test &Style + Style
        let c3 = &s1 + s2.clone();
        assert!(c3.attributes.contains(Attributes::ITALIC));
    }

    #[test]
    fn test_style_display_with_colors_and_link() {
        let style = Style::new()
            .bold()
            .color(Color::from_ansi(1))
            .bgcolor(Color::from_ansi(4))
            .link("https://example.com");

        let display = format!("{style}");
        assert!(display.contains("bold"));
        assert!(display.contains("on"));
        assert!(display.contains("link"));
        assert!(display.contains("https://example.com"));
    }

    #[test]
    fn test_style_parse_error_display() {
        let err1 = StyleParseError::InvalidFormat("test".to_string());
        assert!(err1.to_string().contains("Invalid style format"));

        let err2 = StyleParseError::UnknownAttribute("xyz".to_string());
        assert!(err2.to_string().contains("Unknown attribute"));

        let err3 = StyleParseError::UnknownToken("abc".to_string());
        assert!(err3.to_string().contains("Unknown token"));
    }

    #[test]
    fn test_style_parse_not_with_unknown_attribute() {
        let result = Style::parse("not unknown_attr");
        assert!(matches!(
            result,
            Err(StyleParseError::UnknownAttribute(ref attr)) if attr == "unknown_attr"
        ));
    }

    #[test]
    fn test_style_parse_link_without_url() {
        let result = Style::parse("link");
        assert!(matches!(
            result,
            Err(StyleParseError::InvalidFormat(ref msg)) if msg.contains("requires a URL")
        ));
    }

    #[test]
    fn test_style_parse_whitespace_handling() {
        // Test that extra whitespace is handled
        let style = Style::parse("  bold   red   on   blue  ").unwrap();
        assert!(style.attributes.contains(Attributes::BOLD));
        assert!(style.color.is_some());
        assert!(style.bgcolor.is_some());
    }

    #[test]
    fn test_style_parse_case_insensitive() {
        let style1 = Style::parse("BOLD RED").unwrap();
        let style2 = Style::parse("bold red").unwrap();
        assert_eq!(style1.attributes, style2.attributes);
    }

    #[test]
    fn test_style_color_str_error() {
        let result = Style::new().color_str("not_a_color");
        assert!(result.is_err());
    }

    #[test]
    fn test_style_bgcolor_str_error() {
        let result = Style::new().bgcolor_str("not_a_color");
        assert!(result.is_err());
    }

    #[test]
    fn test_style_is_null_vs_new() {
        let null = Style::null();
        let new = Style::new();

        assert!(null.is_null());
        assert!(!new.is_null()); // new() is not null, it's default
    }

    #[test]
    fn test_style_render_link_only() {
        // Test rendering a style with only a link (no other attributes or colors)
        let style = Style::new().link("https://test.com");
        let rendered = style.render("text", ColorSystem::TrueColor);

        // Should contain OSC 8 sequences but no SGR codes
        assert!(rendered.contains("\x1b]8;;https://test.com\x1b\\"));
        assert!(rendered.contains("text"));
        assert!(rendered.contains("\x1b]8;;\x1b\\"));
        // Should NOT contain SGR reset since no colors/attributes
        assert!(!rendered.contains("\x1b[0m"));
    }

    #[test]
    fn test_attributes_combine_multiple() {
        let attrs = Attributes::BOLD | Attributes::DIM | Attributes::ITALIC | Attributes::STRIKE;
        let codes = attrs.to_sgr_codes();
        assert_eq!(codes.len(), 4);
        assert!(codes.contains(&1)); // BOLD
        assert!(codes.contains(&2)); // DIM
        assert!(codes.contains(&3)); // ITALIC
        assert!(codes.contains(&9)); // STRIKE
    }

    // --- Tests for link_id (OSC 8 hyperlink tracking) ---

    #[test]
    fn test_style_link_with_id() {
        let style = Style::new().link_with_id("https://example.com", "link-123");
        assert_eq!(style.link, Some("https://example.com".to_string()));
        assert_eq!(style.link_id, Some("link-123".to_string()));
        assert!(!style.is_null());
    }

    #[test]
    fn test_style_link_id_method() {
        let style = Style::new().link("https://example.com").link_id("my-id");
        assert_eq!(style.link, Some("https://example.com".to_string()));
        assert_eq!(style.link_id, Some("my-id".to_string()));
    }

    #[test]
    fn test_style_render_link_with_id() {
        let style = Style::new().link_with_id("https://example.com", "test-id");
        let rendered = style.render("click here", ColorSystem::TrueColor);

        // Should contain OSC 8 with id parameter: \x1b]8;id={id};{url}\x1b\\
        assert!(rendered.contains("\x1b]8;id=test-id;https://example.com\x1b\\"));
        assert!(rendered.contains("click here"));
        assert!(rendered.contains("\x1b]8;;\x1b\\")); // Close sequence
    }

    #[test]
    fn test_style_render_link_without_id() {
        let style = Style::new().link("https://example.com");
        let rendered = style.render("click", ColorSystem::TrueColor);

        // Should contain OSC 8 without id: \x1b]8;;{url}\x1b\\
        assert!(rendered.contains("\x1b]8;;https://example.com\x1b\\"));
        assert!(rendered.contains("click"));
        assert!(!rendered.contains("id="));
    }

    #[test]
    fn test_style_combine_link_id() {
        let style1 = Style::new().link("https://a.com").link_id("id-a");
        let style2 = Style::new().link("https://b.com").link_id("id-b");

        // style2 should take precedence
        let combined = style1.combine(&style2);
        assert_eq!(combined.link, Some("https://b.com".to_string()));
        assert_eq!(combined.link_id, Some("id-b".to_string()));
    }

    #[test]
    fn test_style_combine_link_id_partial() {
        // First style has link_id, second doesn't have link_id but has link
        let style1 = Style::new().link("https://a.com").link_id("id-a");
        let style2 = Style::new().link("https://b.com"); // No link_id

        let combined = style1.combine(&style2);
        // Link from style2 takes precedence
        assert_eq!(combined.link, Some("https://b.com".to_string()));
        // link_id should fall back to style1's id
        assert_eq!(combined.link_id, Some("id-a".to_string()));
    }

    #[test]
    fn test_style_combine_preserves_link_id() {
        // Second style has no link at all
        let style1 = Style::new().link("https://a.com").link_id("id-a");
        let style2 = Style::new().bold();

        let combined = style1.combine(&style2);
        assert_eq!(combined.link, Some("https://a.com".to_string()));
        assert_eq!(combined.link_id, Some("id-a".to_string()));
    }

    #[test]
    fn test_style_display_with_link_id() {
        let style = Style::new()
            .bold()
            .link_with_id("https://example.com", "disp-id");
        let display = format!("{style}");

        assert!(display.contains("bold"));
        assert!(display.contains("link[disp-id] https://example.com"));
    }

    #[test]
    fn test_style_display_link_without_id() {
        let style = Style::new().link("https://example.com");
        let display = format!("{style}");

        assert!(display.contains("link https://example.com"));
        assert!(!display.contains("link[")); // No id bracket
    }

    // --- Tests for meta field (serialized metadata) ---

    #[test]
    fn test_style_meta_set() {
        let style = Style::new().meta(vec![1, 2, 3, 4]);
        assert_eq!(style.meta, Some(vec![1, 2, 3, 4]));
        assert!(!style.is_null());
    }

    #[test]
    fn test_style_meta_from_slice() {
        let data: &[u8] = &[10, 20, 30];
        let style = Style::new().meta(data.to_vec());
        assert_eq!(style.meta, Some(vec![10, 20, 30]));
    }

    #[test]
    fn test_style_meta_empty() {
        let style = Style::new().meta(Vec::new());
        assert_eq!(style.meta, Some(Vec::new()));
    }

    #[test]
    fn test_style_combine_meta() {
        let style1 = Style::new().bold().meta(vec![1, 2, 3]);
        let style2 = Style::new().italic().meta(vec![4, 5, 6]);

        let combined = style1.combine(&style2);
        // style2's meta should take precedence
        assert_eq!(combined.meta, Some(vec![4, 5, 6]));
    }

    #[test]
    fn test_style_combine_meta_fallback() {
        let style1 = Style::new().meta(vec![1, 2, 3]);
        let style2 = Style::new().italic(); // No meta

        let combined = style1.combine(&style2);
        // Should fall back to style1's meta
        assert_eq!(combined.meta, Some(vec![1, 2, 3]));
    }

    #[test]
    fn test_style_combine_preserves_meta() {
        let style1 = Style::new().meta(vec![7, 8, 9]);
        let style2 = Style::new().bold();

        let combined = style1.combine(&style2);
        assert_eq!(combined.meta, Some(vec![7, 8, 9]));
        assert!(combined.attributes.contains(Attributes::BOLD));
    }

    #[test]
    fn test_style_meta_does_not_affect_rendering() {
        let style1 = Style::new().bold().meta(vec![1, 2, 3]);
        let style2 = Style::new().bold(); // Same attributes, no meta

        // Rendering should be identical - meta doesn't affect output
        let rendered1 = style1.render("test", ColorSystem::TrueColor);
        let rendered2 = style2.render("test", ColorSystem::TrueColor);
        assert_eq!(rendered1, rendered2);
    }
}
