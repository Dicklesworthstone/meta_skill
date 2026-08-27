//! Markup parsing for Rich-style text.
//!
//! This module provides functionality to parse markup strings like
//! `[bold red]Hello[/]` into styled `Text` objects.

use regex::Regex;
use std::fmt;
use std::sync::LazyLock;

use crate::style::Style;
use crate::text::Text;

/// Error type for markup parsing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkupError {
    /// Closing tag with nothing to close.
    UnmatchedClosingTag(Option<String>),
    /// Invalid tag syntax.
    InvalidTag(String),
}

impl fmt::Display for MarkupError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnmatchedClosingTag(None) => {
                write!(f, "closing tag '[/]' has nothing to close")
            }
            Self::UnmatchedClosingTag(Some(tag)) => {
                write!(f, "closing tag '[/{tag}]' doesn't match any open tag")
            }
            Self::InvalidTag(msg) => write!(f, "invalid tag: {msg}"),
        }
    }
}

impl std::error::Error for MarkupError {}

/// A parsed tag from markup.
#[derive(Debug, Clone)]
pub struct Tag {
    /// The tag name (e.g., "bold", "red", "/", "/bold").
    pub name: String,
    /// Optional parameter (e.g., "link" tag might have a URL).
    pub parameters: Option<String>,
}

impl Tag {
    /// Create a new tag.
    pub fn new(name: impl Into<String>, parameters: Option<String>) -> Self {
        Self {
            name: name.into(),
            parameters,
        }
    }

    /// Check if this is a closing tag.
    #[must_use]
    pub fn is_closing(&self) -> bool {
        self.name.starts_with('/')
    }

    /// Get the tag name without the leading slash for closing tags.
    #[must_use]
    pub fn base_name(&self) -> &str {
        if self.is_closing() {
            &self.name[1..]
        } else {
            &self.name
        }
    }
}

/// Result of parsing a single element from markup.
#[derive(Debug, Clone)]
pub enum ParseElement {
    /// Plain text.
    Text(String),
    /// A tag (opening or closing).
    Tag(Tag),
}

// Regex for matching tags: ((\\*)\[([A-Za-z#/@][^[]*?)])
// Matches: optional backslashes, then [tag_content]
static TAG_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\\*)\[([A-Za-z#/@][^\[\]]*?)\]").expect("invalid regex"));

/// Parse markup string into elements.
///
/// Yields (position, optional plain text, optional tag) tuples.
fn parse_elements(markup: &str) -> Vec<(usize, Option<String>, Option<Tag>)> {
    let mut elements = Vec::new();
    let mut last_end = 0;

    for cap in TAG_PATTERN.captures_iter(markup) {
        let full_match = cap.get(0).unwrap();
        let backslashes = cap.get(1).map_or("", |m| m.as_str());
        let tag_content = cap.get(2).map_or("", |m| m.as_str());

        let match_start = full_match.start();

        // Text before this match
        if match_start > last_end {
            let text = &markup[last_end..match_start];
            elements.push((last_end, Some(text.to_string()), None));
        }

        // Count backslashes
        let num_backslashes = backslashes.len();
        let escaped = num_backslashes % 2 == 1;

        // Handle backslashes (each pair becomes one literal backslash)
        if num_backslashes > 0 {
            let literal_backslashes = num_backslashes / 2;
            if literal_backslashes > 0 {
                elements.push((match_start, Some("\\".repeat(literal_backslashes)), None));
            }
        }

        if escaped {
            // Escaped bracket - treat as literal text
            elements.push((match_start, Some(format!("[{tag_content}]")), None));
        } else {
            // Parse the tag
            let tag = parse_tag(tag_content);
            elements.push((match_start, None, Some(tag)));
        }

        last_end = full_match.end();
    }

    // Remaining text
    if last_end < markup.len() {
        elements.push((last_end, Some(markup[last_end..].to_string()), None));
    }

    elements
}

/// Parse tag content into a Tag struct.
fn parse_tag(content: &str) -> Tag {
    let trimmed = content.trim();

    // Check for parameter (e.g., "link=https://...")
    if let Some(eq_pos) = trimmed.find('=') {
        let name = trimmed[..eq_pos].trim().to_string();
        let param = trimmed[eq_pos + 1..].trim().to_string();
        return Tag::new(name, Some(param));
    }

    // Check for handler syntax @handler(args)
    // Guard: paren_start < paren_end prevents panic on malformed input like "@)("
    if (trimmed.starts_with('@') || trimmed.starts_with("/@"))
        && let Some(paren_start) = trimmed.find('(')
        && let Some(paren_end) = trimmed.rfind(')')
        && paren_start < paren_end
    {
        let name = trimmed[..paren_start].to_string();
        let param = trimmed[paren_start + 1..paren_end].to_string();
        return Tag::new(name, Some(param));
    }

    Tag::new(trimmed, None)
}

/// Render markup string to a Text object.
///
/// # Examples
///
/// ```ignore
/// use rich_rust::markup::render;
///
/// let text = render("[bold]Hello[/] [red]World[/]").unwrap();
/// ```
pub fn render(markup: &str) -> Result<Text, MarkupError> {
    render_with_style_resolver(markup, |definition| {
        Style::parse(definition).unwrap_or_else(|_| Style::new())
    })
}

/// Render markup string to a Text object using a custom style resolver.
///
/// The resolver is given the normalized tag name (see [`Style::normalize`]) and
/// must return the style to apply to that tag.
pub fn render_with_style_resolver<F>(markup: &str, resolve_style: F) -> Result<Text, MarkupError>
where
    F: Fn(&str) -> Style,
{
    // Optimization: if no '[', return plain text
    if !markup.contains('[') {
        return Ok(Text::new(markup));
    }

    let mut text = Text::new("");
    let mut style_stack: Vec<(usize, Tag)> = Vec::new();

    for (_position, plain_text, tag) in parse_elements(markup) {
        // Add any plain text
        if let Some(plain) = plain_text {
            // Replace escaped brackets (double backslash-bracket becomes backslash-bracket)
            let unescaped = plain.replace("\\[", "[");
            text.append(&unescaped);
        }

        // Process tag
        if let Some(tag) = tag {
            if tag.is_closing() {
                // Closing tag
                let style_name = tag.base_name().trim();

                let (start, open_tag) = if style_name.is_empty() {
                    // Implicit close [/]
                    style_stack
                        .pop()
                        .ok_or(MarkupError::UnmatchedClosingTag(None))?
                } else {
                    // Explicit close [/name] - search stack
                    pop_matching(&mut style_stack, style_name).ok_or_else(|| {
                        MarkupError::UnmatchedClosingTag(Some(style_name.to_string()))
                    })?
                };

                // Apply style from the opening tag
                let style = tag_to_style_with_resolver(&open_tag, &resolve_style);
                let end = text.len();
                if start < end {
                    text.stylize(start, end, style);
                }
            } else {
                // Opening tag - push to stack
                let normalized = Tag::new(Style::normalize(&tag.name), tag.parameters.clone());
                style_stack.push((text.len(), normalized));
            }
        }
    }

    // Auto-close any unclosed tags
    while let Some((start, tag)) = style_stack.pop() {
        let style = tag_to_style_with_resolver(&tag, &resolve_style);
        let end = text.len();
        if start < end {
            text.stylize(start, end, style);
        }
    }

    Ok(text)
}

/// Pop a matching tag from the stack by name.
fn pop_matching(stack: &mut Vec<(usize, Tag)>, name: &str) -> Option<(usize, Tag)> {
    let search_name = Style::normalize(name);

    // Search from top of stack
    for i in (0..stack.len()).rev() {
        // Stack entries are normalized on push.
        if stack[i].1.name == search_name {
            return Some(stack.remove(i));
        }
    }
    None
}

/// Convert a tag to a Style using a custom resolver.
fn tag_to_style_with_resolver<F>(tag: &Tag, resolve_style: &F) -> Style
where
    F: Fn(&str) -> Style,
{
    // Handle link tag specially
    if tag.name.eq_ignore_ascii_case("link")
        && let Some(ref url) = tag.parameters
    {
        return Style::new().link(url);
    }

    resolve_style(&tag.name)
}

/// Escape text for use in markup.
///
/// This escapes any `[` characters so they are treated as literal text.
#[must_use]
pub fn escape(text: &str) -> String {
    text.replace('[', "\\[")
}

/// Render markup to Text, returning plain text on error.
///
/// This is a convenience function that never fails - on parse error,
/// it returns the original markup as plain text.
#[must_use]
pub fn render_or_plain(markup: &str) -> Text {
    render(markup).unwrap_or_else(|_| Text::new(markup))
}

/// Render markup to Text using a custom style resolver, returning plain text on error.
#[must_use]
pub fn render_or_plain_with_style_resolver<F>(markup: &str, resolve_style: F) -> Text
where
    F: Fn(&str) -> Style,
{
    render_with_style_resolver(markup, resolve_style).unwrap_or_else(|_| Text::new(markup))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_plain() {
        let text = render("hello world").unwrap();
        assert_eq!(text.plain(), "hello world");
        assert_eq!(text.spans().len(), 0);
    }

    #[test]
    fn test_render_bold() {
        let text = render("[bold]hello[/bold]").unwrap();
        assert_eq!(text.plain(), "hello");
        assert_eq!(text.spans().len(), 1);
    }

    #[test]
    fn test_render_implicit_close() {
        let text = render("[bold]hello[/]").unwrap();
        assert_eq!(text.plain(), "hello");
        assert_eq!(text.spans().len(), 1);
    }

    #[test]
    fn test_render_nested() {
        let text = render("[bold][red]hello[/red][/bold]").unwrap();
        assert_eq!(text.plain(), "hello");
        assert_eq!(text.spans().len(), 2);
    }

    #[test]
    fn test_render_multiple_styles() {
        let text = render("[bold red]hello[/]").unwrap();
        assert_eq!(text.plain(), "hello");
        assert_eq!(text.spans().len(), 1);
    }

    #[test]
    fn test_render_normalizes_style_token_order() {
        let text = render("[red bold]hello[/bold red]").unwrap();
        assert_eq!(text.plain(), "hello");
        assert_eq!(text.spans().len(), 1);
    }

    #[test]
    fn test_render_explicit_close_with_different_token_order() {
        let text = render("[red bold]hello[/red bold]").unwrap();
        assert_eq!(text.plain(), "hello");
        assert_eq!(text.spans().len(), 1);
    }

    #[test]
    fn test_render_explicit_close_matches_full_style() {
        let text = render("[bold red]hello[/bold red]").unwrap();
        assert_eq!(text.plain(), "hello");
        assert_eq!(text.spans().len(), 1);
    }

    #[test]
    fn test_render_escaped_bracket() {
        let text = render("\\[not a tag]").unwrap();
        assert_eq!(text.plain(), "[not a tag]");
    }

    #[test]
    fn test_render_unclosed_tag() {
        let text = render("[bold]hello").unwrap();
        assert_eq!(text.plain(), "hello");
        assert_eq!(text.spans().len(), 1); // Auto-closed
    }

    #[test]
    fn test_render_mixed() {
        let text = render("hello [bold]world[/]!").unwrap();
        assert_eq!(text.plain(), "hello world!");
        assert_eq!(text.spans().len(), 1);
    }

    #[test]
    fn test_escape() {
        assert_eq!(escape("hello [world]"), "hello \\[world]");
    }

    #[test]
    fn test_unmatched_closing_tag() {
        let result = render("[/bold]");
        let err = result.expect_err("expected error for unmatched closing tag");
        assert_eq!(
            err.to_string(),
            "closing tag '[/bold]' doesn't match any open tag"
        );
    }

    #[test]
    fn test_explicit_close_requires_full_tag_match() {
        let result = render("[bold red]text[/bold]");
        let err = result.expect_err("expected error for mismatched closing tag");
        assert_eq!(
            err.to_string(),
            "closing tag '[/bold]' doesn't match any open tag"
        );
    }

    #[test]
    fn test_empty_close_nothing_to_close() {
        let result = render("hello[/]");
        let err = result.expect_err("expected error for empty close");
        assert_eq!(err.to_string(), "closing tag '[/]' has nothing to close");
    }

    #[test]
    fn test_no_brackets_optimization() {
        let text = render("plain text without any brackets").unwrap();
        assert_eq!(text.plain(), "plain text without any brackets");
    }

    #[test]
    fn test_link_tag() {
        let text = render("[link=https://example.com]click here[/link]").unwrap();
        assert_eq!(text.plain(), "click here");
        assert_eq!(text.spans().len(), 1);
    }

    #[test]
    fn test_tag_parsing() {
        let tag = parse_tag("bold red");
        assert_eq!(tag.name, "bold red");
        assert!(tag.parameters.is_none());

        let tag = parse_tag("link=https://example.com");
        assert_eq!(tag.name, "link");
        assert_eq!(tag.parameters, Some("https://example.com".to_string()));
    }

    #[test]
    fn test_render_color() {
        let text = render("[red]error[/] [green]success[/]").unwrap();
        assert_eq!(text.plain(), "error success");
        assert_eq!(text.spans().len(), 2);
    }

    #[test]
    fn test_render_uppercase_tag() {
        let text = render("[BOLD]hello[/BOLD]").unwrap();
        assert_eq!(text.plain(), "hello");
        assert_eq!(text.spans().len(), 1);
    }

    // ============================================================
    // Additional tests for comprehensive coverage (rich_rust-xz9)
    // ============================================================

    // --- Deep Nesting Tests ---

    #[test]
    fn test_deep_nesting_3_levels() {
        let text = render("[bold][italic][underline]deep[/][/][/]").unwrap();
        assert_eq!(text.plain(), "deep");
        assert_eq!(text.spans().len(), 3);
    }

    #[test]
    fn test_deep_nesting_4_levels() {
        let text = render("[bold][red][italic][underline]very deep[/][/][/][/]").unwrap();
        assert_eq!(text.plain(), "very deep");
        assert_eq!(text.spans().len(), 4);
    }

    #[test]
    fn test_nested_with_explicit_close() {
        let text = render("[bold][italic]text[/italic][/bold]").unwrap();
        assert_eq!(text.plain(), "text");
        assert_eq!(text.spans().len(), 2);
    }

    // --- Sibling Tags Tests ---

    #[test]
    fn test_sibling_tags() {
        let text = render("[bold]one[/][italic]two[/][underline]three[/]").unwrap();
        assert_eq!(text.plain(), "onetwothree");
        assert_eq!(text.spans().len(), 3);
    }

    #[test]
    fn test_sibling_tags_with_text_between() {
        let text = render("[bold]one[/] and [italic]two[/] and [red]three[/]").unwrap();
        assert_eq!(text.plain(), "one and two and three");
        assert_eq!(text.spans().len(), 3);
    }

    // --- Style Inheritance Tests ---

    #[test]
    fn test_style_combination_bold_red() {
        let text = render("[bold red]styled[/]").unwrap();
        assert_eq!(text.plain(), "styled");
        assert_eq!(text.spans().len(), 1);
        // Style should contain both bold and red
        let style = &text.spans()[0].style;
        assert!(style.attributes.contains(crate::style::Attributes::BOLD));
        assert!(style.color.is_some());
    }

    #[test]
    fn test_style_on_background() {
        let text = render("[red on blue]text[/]").unwrap();
        assert_eq!(text.plain(), "text");
        assert_eq!(text.spans().len(), 1);
        let style = &text.spans()[0].style;
        assert!(style.color.is_some());
        assert!(style.bgcolor.is_some());
    }

    // --- Escaping Tests ---

    #[test]
    fn test_double_backslash() {
        let text = render("\\\\[bold]text[/]").unwrap();
        // Double backslash becomes single backslash
        assert!(text.plain().contains('\\'));
        assert!(text.plain().contains("text"));
    }

    #[test]
    fn test_mixed_escaped_and_tags() {
        let text = render("\\[not tag] [bold]real tag[/]").unwrap();
        assert_eq!(text.plain(), "[not tag] real tag");
        assert_eq!(text.spans().len(), 1);
    }

    #[test]
    fn test_escaped_bracket_in_middle() {
        let text = render("[bold]hello\\[world[/]").unwrap();
        assert!(text.plain().contains("hello"));
    }

    // --- Edge Cases ---

    #[test]
    fn test_empty_string() {
        let text = render("").unwrap();
        assert_eq!(text.plain(), "");
        assert_eq!(text.spans().len(), 0);
    }

    #[test]
    fn test_only_whitespace() {
        let text = render("   ").unwrap();
        assert_eq!(text.plain(), "   ");
    }

    #[test]
    fn test_empty_tag_content() {
        // [] is not matched by the regex (requires [a-z#/@...])
        let text = render("[]").unwrap();
        assert_eq!(text.plain(), "[]");
    }

    #[test]
    fn test_unclosed_bracket() {
        // [bold without closing ] - not matched by regex
        let text = render("[bold without closing").unwrap();
        assert_eq!(text.plain(), "[bold without closing");
    }

    #[test]
    fn test_unopened_bracket() {
        // Just ] is plain text
        let text = render("text] more").unwrap();
        assert_eq!(text.plain(), "text] more");
    }

    #[test]
    fn test_invalid_style_graceful() {
        // Invalid style should not panic - tag_to_style returns default
        let text = render("[invalidstyle12345]text[/]").unwrap();
        assert_eq!(text.plain(), "text");
    }

    #[test]
    fn test_nested_brackets_in_content() {
        // Brackets inside styled text
        let text = render("[bold]hello \\[world\\][/]").unwrap();
        assert!(text.plain().contains("hello"));
    }

    // --- No Panic Tests ---

    #[test]
    fn test_no_panic_on_random_brackets() {
        // Should not panic on any input
        let inputs = [
            "[[[]]]",
            "[[[",
            "]]]",
            "[/][/][/]",
            "\\\\\\\\",
            "[bold[italic]text[/]",
            "[=value]text[/]",
            "[@handler]text[/]",
        ];

        for input in inputs {
            // Should not panic - may return Ok or Err
            let _ = render(input);
        }
    }

    #[test]
    fn test_no_panic_unicode() {
        let text = render("[bold]日本語テキスト[/]").unwrap();
        assert_eq!(text.plain(), "日本語テキスト");
    }

    #[test]
    fn test_no_panic_emoji() {
        let text = render("[red]🎉 celebration 🎉[/]").unwrap();
        assert_eq!(text.plain(), "🎉 celebration 🎉");
    }

    // --- Link and Parameter Tests ---

    #[test]
    fn test_link_with_special_chars() {
        let text = render("[link=https://example.com/path?a=1&b=2]url[/link]").unwrap();
        assert_eq!(text.plain(), "url");
    }

    #[test]
    fn test_handler_syntax() {
        // @handler(args) syntax
        let tag = parse_tag("@click(button1)");
        assert_eq!(tag.name, "@click");
        assert_eq!(tag.parameters, Some("button1".to_string()));
    }

    #[test]
    fn test_handler_syntax_malformed_parens() {
        // Malformed handler with reversed parens should not panic (bd-panic-fix)
        // Previously this caused a slice panic: paren_start + 1 > paren_end
        let tag = parse_tag("@)(");
        // Falls through to plain tag parsing since parens are reversed
        assert_eq!(tag.name, "@)(");
        assert!(tag.parameters.is_none());

        // Also test with content
        let tag2 = parse_tag("@handler)(args");
        assert_eq!(tag2.name, "@handler)(args");
        assert!(tag2.parameters.is_none());
    }

    // --- Multiple Tags Same Line ---

    #[test]
    fn test_adjacent_tags_no_space() {
        let text = render("[bold]A[/][italic]B[/][underline]C[/]").unwrap();
        assert_eq!(text.plain(), "ABC");
        assert_eq!(text.spans().len(), 3);
    }

    #[test]
    fn test_many_tags_single_line() {
        let markup = "[red]R[/][green]G[/][blue]B[/][yellow]Y[/][magenta]M[/][cyan]C[/]";
        let text = render(markup).unwrap();
        assert_eq!(text.plain(), "RGBYMC");
        assert_eq!(text.spans().len(), 6);
    }

    // --- Tag with Whitespace ---

    #[test]
    fn test_tag_with_internal_whitespace() {
        let text = render("[bold  red]styled[/]").unwrap();
        assert_eq!(text.plain(), "styled");
    }

    #[test]
    fn test_tag_trimming() {
        let tag = parse_tag("  bold  ");
        assert_eq!(tag.name, "bold");
    }

    // --- Complex Cases ---

    #[test]
    fn test_interleaved_text_and_tags() {
        let text = render("start [bold]middle[/] end").unwrap();
        assert_eq!(text.plain(), "start middle end");
    }

    #[test]
    fn test_color_hex() {
        let text = render("[#ff0000]red hex[/]").unwrap();
        assert_eq!(text.plain(), "red hex");
        // Should have a span with the color
        assert_eq!(text.spans().len(), 1);
    }

    #[test]
    fn test_render_or_plain_fallback() {
        // render_or_plain should not fail
        let text = render_or_plain("[/]");
        assert_eq!(text.plain(), "[/]"); // Falls back to plain text
    }

    #[test]
    fn test_tag_is_closing() {
        let open = Tag::new("bold", None);
        let close = Tag::new("/bold", None);
        assert!(!open.is_closing());
        assert!(close.is_closing());
    }

    #[test]
    fn test_tag_base_name() {
        let tag = Tag::new("/bold", None);
        assert_eq!(tag.base_name(), "bold");

        let tag = Tag::new("bold", None);
        assert_eq!(tag.base_name(), "bold");
    }
}
