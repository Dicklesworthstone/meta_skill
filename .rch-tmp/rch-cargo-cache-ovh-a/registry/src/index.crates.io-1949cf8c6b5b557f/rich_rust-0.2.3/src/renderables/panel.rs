//! Panel - bordered box containing content.
//!
//! A Panel renders content inside a decorative border with optional
//! title and subtitle.

use crate::r#box::{ASCII, BoxChars, ROUNDED, SQUARE};
use crate::cells;
use crate::console::{Console, ConsoleOptions};
use crate::markup;
use crate::renderables::Renderable;
use crate::segment::{Segment, adjust_line_length};
use crate::style::Style;
use crate::text::{JustifyMethod, OverflowMethod, Text};

use super::padding::PaddingDimensions;

/// A bordered panel containing content.
#[derive(Debug, Clone)]
pub struct Panel<'a> {
    /// Content lines to render inside the panel.
    content_lines: Vec<Vec<Segment<'a>>>,
    /// Box drawing style.
    box_style: &'static BoxChars,
    /// Use ASCII-safe characters.
    safe_box: Option<bool>,
    /// Expand to fill available width.
    expand: bool,
    /// Style for the panel background.
    style: Style,
    /// Style for the border.
    border_style: Style,
    /// Fixed width (None = auto).
    width: Option<usize>,
    /// Fixed height (None = auto).
    height: Option<usize>,
    /// Padding inside the border.
    padding: PaddingDimensions,
    /// Optional title.
    title: Option<Text>,
    /// Title alignment.
    title_align: JustifyMethod,
    /// Optional subtitle (bottom).
    subtitle: Option<Text>,
    /// Subtitle alignment.
    subtitle_align: JustifyMethod,
}

impl Default for Panel<'_> {
    fn default() -> Self {
        Self {
            content_lines: Vec::new(),
            box_style: &ROUNDED,
            safe_box: None,
            expand: true,
            style: Style::new(),
            border_style: Style::new(),
            width: None,
            height: None,
            padding: PaddingDimensions::symmetric(0, 1),
            title: None,
            title_align: JustifyMethod::Center,
            subtitle: None,
            subtitle_align: JustifyMethod::Center,
        }
    }
}

impl<'a> Panel<'a> {
    /// Create a new panel with content lines.
    #[must_use]
    pub fn new(content_lines: Vec<Vec<Segment<'a>>>) -> Self {
        Self {
            content_lines,
            ..Self::default()
        }
    }

    /// Create a panel from plain text content.
    ///
    /// This does **NOT** parse Rich markup. If you want markup styling,
    /// parse into a [`Text`] first with [`crate::markup::render_or_plain`]
    /// and use [`Panel::from_rich_text`].
    #[must_use]
    pub fn from_text(text: &'a str) -> Self {
        let lines: Vec<Vec<Segment<'a>>> = text
            .lines()
            .map(|line| vec![Segment::new(line, None)])
            .collect();
        Self::new(lines)
    }

    /// Create a panel from a pre-styled Text object.
    ///
    /// Use this when you already have a `Text` with spans (for example from
    /// [`crate::markup::render_or_plain`]).
    #[must_use]
    pub fn from_rich_text(text: &'a Text, width: usize) -> Self {
        // Split into logical lines first, then render each line to segments.
        let lines = text
            .split_lines()
            .into_iter()
            .map(|line| {
                line.render("")
                    .into_iter()
                    .map(super::super::segment::Segment::into_owned)
                    .collect()
            })
            .collect();

        Self {
            content_lines: lines,
            width: Some(width),
            ..Self::default()
        }
    }

    /// Set the box style.
    #[must_use]
    pub fn box_style(mut self, style: &'static BoxChars) -> Self {
        self.box_style = style;
        self
    }

    /// Use rounded box style.
    #[must_use]
    pub fn rounded(mut self) -> Self {
        self.box_style = &ROUNDED;
        self
    }

    /// Use square box style.
    #[must_use]
    pub fn square(mut self) -> Self {
        self.box_style = &SQUARE;
        self
    }

    /// Use ASCII-safe box style.
    #[must_use]
    pub fn ascii(mut self) -> Self {
        self.box_style = &ASCII;
        self.safe_box = Some(true);
        self
    }

    /// Force ASCII-safe rendering.
    #[must_use]
    pub fn safe_box(mut self, safe: bool) -> Self {
        self.safe_box = Some(safe);
        self
    }

    /// Set whether to expand to fill width.
    #[must_use]
    pub fn expand(mut self, expand: bool) -> Self {
        self.expand = expand;
        self
    }

    /// Set the background style.
    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Set the border style.
    #[must_use]
    pub fn border_style(mut self, style: Style) -> Self {
        self.border_style = style;
        self
    }

    /// Set fixed width.
    #[must_use]
    pub fn width(mut self, width: usize) -> Self {
        self.width = Some(width);
        self
    }

    /// Set fixed height.
    #[must_use]
    pub fn height(mut self, height: usize) -> Self {
        self.height = Some(height);
        self
    }

    /// Set padding.
    #[must_use]
    pub fn padding(mut self, padding: impl Into<PaddingDimensions>) -> Self {
        self.padding = padding.into();
        self
    }

    /// Set the title.
    ///
    /// Passing a `&str` uses `Text::new()` and does **NOT** parse markup.
    /// For styled titles, pass a pre-styled `Text` (e.g. from
    /// [`crate::markup::render_or_plain`]).
    #[must_use]
    pub fn title(mut self, title: impl Into<Text>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set title alignment.
    #[must_use]
    pub fn title_align(mut self, align: JustifyMethod) -> Self {
        self.title_align = align;
        self
    }

    /// Set the title from a markup string.
    ///
    /// This parses Rich markup tags like `[bold cyan]Title[/]` into styled text.
    ///
    /// # Example
    ///
    /// ```rust
    /// use rich_rust::renderables::Panel;
    ///
    /// let panel = Panel::from_text("Content")
    ///     .title_from_markup("[bold cyan]Styled Title[/]");
    /// ```
    #[must_use]
    pub fn title_from_markup(mut self, title: &str) -> Self {
        self.title = Some(markup::render_or_plain(title));
        self
    }

    /// Set the subtitle.
    ///
    /// Passing a `&str` uses `Text::new()` and does **NOT** parse markup.
    /// For styled subtitles, pass a pre-styled `Text` (e.g. from
    /// [`crate::markup::render_or_plain`]).
    #[must_use]
    pub fn subtitle(mut self, subtitle: impl Into<Text>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    /// Set subtitle alignment.
    #[must_use]
    pub fn subtitle_align(mut self, align: JustifyMethod) -> Self {
        self.subtitle_align = align;
        self
    }

    /// Set the subtitle from a markup string.
    ///
    /// This parses Rich markup tags like `[dim]Subtitle[/]` into styled text.
    ///
    /// # Example
    ///
    /// ```rust
    /// use rich_rust::renderables::Panel;
    ///
    /// let panel = Panel::from_text("Content")
    ///     .subtitle_from_markup("[dim italic]Footer text[/]");
    /// ```
    #[must_use]
    pub fn subtitle_from_markup(mut self, subtitle: &str) -> Self {
        self.subtitle = Some(markup::render_or_plain(subtitle));
        self
    }

    /// Get the effective box characters.
    fn effective_box(&self) -> &'static BoxChars {
        let safe = self.safe_box.unwrap_or(false);
        if safe && !self.box_style.ascii {
            &ASCII
        } else {
            self.box_style
        }
    }

    /// Calculate content width from content lines.
    fn content_width(&self) -> usize {
        self.content_lines
            .iter()
            .map(|line: &Vec<Segment<'a>>| line.iter().map(|seg| cells::cell_len(&seg.text)).sum())
            .max()
            .unwrap_or(0)
    }

    /// Render the panel to segments.
    #[must_use]
    pub fn render(&self, max_width: usize) -> Vec<Segment<'a>> {
        let box_chars = self.effective_box();

        // Calculate panel width
        let panel_width = if self.expand {
            self.width.unwrap_or(max_width).min(max_width)
        } else {
            let content_w = self.content_width();
            let min_width = content_w + 2 + self.padding.horizontal();
            self.width.unwrap_or(min_width).min(max_width)
        };

        // Inner width (inside borders)
        let inner_width = panel_width.saturating_sub(2);

        let mut pad_left = self.padding.left;
        let mut pad_right = self.padding.right;
        let max_content_width = self.content_width();
        if max_content_width <= inner_width {
            let available_for_padding = inner_width.saturating_sub(max_content_width);
            if pad_left + pad_right > available_for_padding {
                let mut remaining = available_for_padding;
                pad_left = pad_left.min(remaining);
                remaining = remaining.saturating_sub(pad_left);
                pad_right = pad_right.min(remaining);
            }
        }

        // Content width (inside borders and padding)
        let content_width = inner_width.saturating_sub(pad_left + pad_right);

        let mut pad_top = self.padding.top;
        let mut pad_bottom = self.padding.bottom;
        let mut content_lines = self.content_lines.clone();

        if let Some(height) = self.height {
            let max_inner_lines = height.saturating_sub(2);
            if content_lines.len() > max_inner_lines {
                content_lines.truncate(max_inner_lines);
                pad_top = 0;
                pad_bottom = 0;
            } else {
                let remaining_after_content = max_inner_lines - content_lines.len();
                if pad_top + pad_bottom > remaining_after_content {
                    let mut remaining = remaining_after_content;
                    pad_top = pad_top.min(remaining);
                    remaining = remaining.saturating_sub(pad_top);
                    pad_bottom = pad_bottom.min(remaining);
                }

                let max_content_lines = max_inner_lines.saturating_sub(pad_top + pad_bottom);
                if content_lines.len() < max_content_lines {
                    content_lines.extend(
                        std::iter::repeat_with(Vec::new)
                            .take(max_content_lines - content_lines.len()),
                    );
                }
            }
        }

        let mut segments = Vec::new();

        // Top border with optional title
        segments.extend(self.render_top_border(box_chars, inner_width));
        segments.push(Segment::line());

        // Top padding
        for _ in 0..pad_top {
            segments.push(Segment::new(
                box_chars.head[0].to_string(),
                Some(self.border_style.clone()),
            ));
            segments.push(Segment::new(
                " ".repeat(inner_width),
                Some(self.style.clone()),
            ));
            segments.push(Segment::new(
                box_chars.head[3].to_string(),
                Some(self.border_style.clone()),
            ));
            segments.push(Segment::line());
        }

        // Content lines
        let left_pad = " ".repeat(pad_left);
        let right_pad = " ".repeat(pad_right);

        for line in &content_lines {
            // Left border
            segments.push(Segment::new(
                box_chars.head[0].to_string(),
                Some(self.border_style.clone()),
            ));

            // Left padding
            if pad_left > 0 {
                segments.push(Segment::new(left_pad.clone(), Some(self.style.clone())));
            }

            // Content (truncate/pad to content width)
            let mut content_segments: Vec<Segment<'a>> = line
                .iter()
                .cloned()
                .map(|mut seg: Segment<'a>| {
                    if !seg.is_control() {
                        seg.style = Some(match seg.style.take() {
                            Some(existing) => self.style.combine(&existing),
                            None => self.style.clone(),
                        });
                    }
                    seg
                })
                .collect();

            content_segments = adjust_line_length(
                content_segments,
                content_width,
                Some(self.style.clone()),
                true,
            );

            segments.extend(content_segments);

            // Right padding
            if pad_right > 0 {
                segments.push(Segment::new(right_pad.clone(), Some(self.style.clone())));
            }

            // Right border
            segments.push(Segment::new(
                box_chars.head[3].to_string(),
                Some(self.border_style.clone()),
            ));
            segments.push(Segment::line());
        }

        // Bottom padding
        for _ in 0..pad_bottom {
            segments.push(Segment::new(
                box_chars.head[0].to_string(),
                Some(self.border_style.clone()),
            ));
            segments.push(Segment::new(
                " ".repeat(inner_width),
                Some(self.style.clone()),
            ));
            segments.push(Segment::new(
                box_chars.head[3].to_string(),
                Some(self.border_style.clone()),
            ));
            segments.push(Segment::line());
        }

        // Bottom border with optional subtitle
        segments.extend(self.render_bottom_border(box_chars, inner_width));
        segments.push(Segment::line());

        segments
    }

    /// Render the top border with optional title.
    fn render_top_border(&self, box_chars: &BoxChars, inner_width: usize) -> Vec<Segment<'a>> {
        let border_style = Some(self.border_style.clone());
        if let Some(title) = &self.title {
            let mut segments = Vec::new();
            let max_text_width = if inner_width >= 4 {
                inner_width.saturating_sub(4)
            } else {
                inner_width.saturating_sub(2)
            };
            let title_text = if inner_width >= 2 {
                if title.cell_len() > max_text_width {
                    truncate_text_to_width(title, max_text_width)
                } else {
                    title.clone()
                }
            } else {
                truncate_text_to_width(title, inner_width)
            };

            let title_width = title_text.cell_len();
            if inner_width < 2 {
                segments.push(Segment::new(
                    box_chars.top[0].to_string(),
                    border_style.clone(),
                ));
                segments.extend(
                    title_text
                        .render("")
                        .into_iter()
                        .map(super::super::segment::Segment::into_owned),
                );
                let remaining = inner_width.saturating_sub(title_width);
                if remaining > 0 {
                    segments.push(Segment::new(
                        box_chars.top[1].to_string().repeat(remaining),
                        border_style.clone(),
                    ));
                }
                segments.push(Segment::new(
                    box_chars.top[3].to_string(),
                    border_style.clone(),
                ));
                return segments;
            }

            let title_total_width = title_width.saturating_add(2);
            let available = inner_width.saturating_sub(title_total_width);
            let (left_rule, right_rule) = if available == 0 {
                (0, 0)
            } else {
                match self.title_align {
                    JustifyMethod::Left | JustifyMethod::Default => {
                        (1, available.saturating_sub(1))
                    }
                    JustifyMethod::Right => (available.saturating_sub(1), 1),
                    JustifyMethod::Center | JustifyMethod::Full => {
                        let left = available / 2;
                        (left, available - left)
                    }
                }
            };

            let left_border = if left_rule > 0 {
                format!("{}{}", box_chars.top[0], box_chars.top[1])
            } else {
                box_chars.top[0].to_string()
            };
            segments.push(Segment::new(left_border, border_style.clone()));
            if left_rule > 1 {
                segments.push(Segment::new(
                    box_chars.top[1].to_string().repeat(left_rule - 1),
                    border_style.clone(),
                ));
            }

            // Keep title padding colored like the border to match Rich's ANSI output.
            segments.push(Segment::new(" ", border_style.clone()));
            segments.extend(
                title_text
                    .render("")
                    .into_iter()
                    .map(super::super::segment::Segment::into_owned),
            );
            segments.push(Segment::new(" ", border_style.clone()));

            if right_rule > 1 {
                segments.push(Segment::new(
                    box_chars.top[1].to_string().repeat(right_rule - 1),
                    border_style.clone(),
                ));
            }
            let right_border = if right_rule > 0 {
                format!("{}{}", box_chars.top[1], box_chars.top[3])
            } else {
                box_chars.top[3].to_string()
            };
            segments.push(Segment::new(right_border, border_style));
            return segments;
        }

        vec![Segment::new(
            format!(
                "{}{}{}",
                box_chars.top[0],
                box_chars.top[1].to_string().repeat(inner_width),
                box_chars.top[3]
            ),
            border_style,
        )]
    }

    /// Render the bottom border with optional subtitle.
    fn render_bottom_border(&self, box_chars: &BoxChars, inner_width: usize) -> Vec<Segment<'a>> {
        let mut segments = Vec::new();

        if let Some(subtitle) = &self.subtitle {
            // Left corner
            segments.push(Segment::new(
                box_chars.bottom[0].to_string(),
                Some(self.border_style.clone()),
            ));
            let max_text_width = if inner_width >= 4 {
                inner_width.saturating_sub(4)
            } else {
                inner_width.saturating_sub(2)
            };
            let subtitle_text = if inner_width >= 2 {
                if subtitle.cell_len() > max_text_width {
                    truncate_text_to_width(subtitle, max_text_width)
                } else {
                    subtitle.clone()
                }
            } else {
                truncate_text_to_width(subtitle, inner_width)
            };

            let subtitle_width = subtitle_text.cell_len();
            if inner_width < 2 {
                segments.extend(
                    subtitle_text
                        .render("")
                        .into_iter()
                        .map(super::super::segment::Segment::into_owned),
                );
                let remaining = inner_width.saturating_sub(subtitle_width);
                if remaining > 0 {
                    segments.push(Segment::new(
                        box_chars.bottom[1].to_string().repeat(remaining),
                        Some(self.border_style.clone()),
                    ));
                }
            } else {
                let subtitle_total_width = subtitle_width.saturating_add(2);
                let available = inner_width.saturating_sub(subtitle_total_width);
                let (left_rule, right_rule) = if available == 0 {
                    (0, 0)
                } else {
                    match self.subtitle_align {
                        JustifyMethod::Left | JustifyMethod::Default => {
                            (1, available.saturating_sub(1))
                        }
                        JustifyMethod::Right => (available.saturating_sub(1), 1),
                        JustifyMethod::Center | JustifyMethod::Full => {
                            let left = available / 2;
                            (left, available - left)
                        }
                    }
                };

                if left_rule > 0 {
                    segments.push(Segment::new(
                        box_chars.bottom[1].to_string().repeat(left_rule),
                        Some(self.border_style.clone()),
                    ));
                }

                segments.push(Segment::new(" ", Some(subtitle_text.style().clone())));
                segments.extend(
                    subtitle_text
                        .render("")
                        .into_iter()
                        .map(super::super::segment::Segment::into_owned),
                );
                segments.push(Segment::new(" ", Some(subtitle_text.style().clone())));

                if right_rule > 0 {
                    segments.push(Segment::new(
                        box_chars.bottom[1].to_string().repeat(right_rule),
                        Some(self.border_style.clone()),
                    ));
                }
            }
        } else {
            // No subtitle: render as one border segment to preserve ANSI continuity.
            return vec![Segment::new(
                format!(
                    "{}{}{}",
                    box_chars.bottom[0],
                    box_chars.bottom[1].to_string().repeat(inner_width),
                    box_chars.bottom[3]
                ),
                Some(self.border_style.clone()),
            )];
        }

        // Right corner (subtitle case)
        segments.push(Segment::new(
            box_chars.bottom[3].to_string(),
            Some(self.border_style.clone()),
        ));

        segments
    }

    /// Render to plain text.
    #[must_use]
    pub fn render_plain(&self, max_width: usize) -> String {
        self.render(max_width)
            .into_iter()
            .map(|seg| seg.text.into_owned())
            .collect()
    }
}

impl Renderable for Panel<'_> {
    fn render<'b>(&'b self, console: &Console, options: &ConsoleOptions) -> Vec<Segment<'b>> {
        if self.safe_box.is_some() {
            return self.render(options.max_width).into_iter().collect();
        }

        // Inherit the Console's safe_box setting unless explicitly overridden.
        let effective = self.clone().safe_box(console.safe_box());
        effective.render(options.max_width).into_iter().collect()
    }
}

/// Truncate a Text object to a maximum cell width with ellipsis.
fn truncate_text_to_width(text: &Text, max_width: usize) -> Text {
    let mut truncated = text.clone();
    truncated.truncate(max_width, OverflowMethod::Ellipsis, false);
    truncated
}

/// Create a panel with content that fits (doesn't expand).
#[must_use]
pub fn fit_panel(text: &str) -> Panel<'_> {
    Panel::from_text(text).expand(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::segment::split_lines;
    use crate::style::Attributes;

    #[test]
    fn test_panel_from_text() {
        let panel = Panel::from_text("Hello\nWorld");
        assert_eq!(panel.content_lines.len(), 2);
    }

    #[test]
    fn test_panel_render() {
        let panel = Panel::from_text("Hello").width(20);
        let segments = panel.render(80);
        assert_ne!(segments.len(), 0);

        let text: String = segments.iter().map(|s| s.text.as_ref()).collect();
        assert!(text.contains("Hello"));
        // Should have rounded corners by default
        assert!(text.contains('\u{256D}')); // ╭
    }

    #[test]
    fn test_panel_with_title() {
        let panel = Panel::from_text("Content").title("Title").width(30);
        let text = panel.render_plain(80);
        assert!(text.contains("Title"));
        assert!(text.contains("Content"));
    }

    #[test]
    fn test_panel_ascii() {
        let panel = Panel::from_text("Hello").ascii().width(20);
        let text = panel.render_plain(80);
        assert!(text.contains('+')); // ASCII corners
        assert!(text.contains('-')); // ASCII horizontal
    }

    #[test]
    fn test_panel_square() {
        let panel = Panel::from_text("Hello").square().width(20);
        let text = panel.render_plain(80);
        assert!(text.contains('\u{250C}')); // ┌
    }

    #[test]
    fn test_panel_padding() {
        let panel = Panel::from_text("Hi").padding((1, 2)).width(20);
        let segments = panel.render(80);
        // Count newlines to verify padding
        let newlines = segments.iter().filter(|s| s.text == "\n").count();
        // Should have: top border, 1 top pad, content, 1 bottom pad, bottom border
        assert!(newlines >= 5);
    }

    #[test]
    fn test_panel_subtitle() {
        let panel = Panel::from_text("Content").subtitle("Footer").width(30);
        let text = panel.render_plain(80);
        assert!(text.contains("Footer"));
    }

    #[test]
    fn test_panel_truncates_to_width() {
        let panel = Panel::from_text("This is a very long line")
            .width(10)
            .padding(0);

        let segments = panel.render(10);
        let lines = split_lines(segments.into_iter());

        for line in lines {
            let width: usize = line.iter().map(Segment::cell_length).sum();
            if width > 0 {
                assert_eq!(width, 10);
            }
        }
    }

    #[test]
    fn test_panel_height_limits_content_lines() {
        let panel = Panel::from_text("A\nB\nC").height(4).padding(0).width(10);

        let segments = panel.render(10);
        let lines = split_lines(segments.into_iter());
        let non_empty_lines = lines
            .iter()
            .filter(|line| line.iter().map(Segment::cell_length).sum::<usize>() > 0)
            .count();

        assert_eq!(non_empty_lines, 4);
        let text: String = lines
            .iter()
            .map(|line| line.iter().map(|seg| seg.text.as_ref()).collect::<String>())
            .collect();
        assert!(!text.contains('C'));
    }

    #[test]
    fn test_panel_height_pads_content_lines() {
        let panel = Panel::from_text("A").height(5).padding(0).width(10);

        let segments = panel.render(10);
        let lines = split_lines(segments.into_iter());
        let non_empty_lines = lines
            .iter()
            .filter(|line| line.iter().map(Segment::cell_length).sum::<usize>() > 0)
            .count();

        assert_eq!(non_empty_lines, 5);
    }

    #[test]
    fn test_panel_height_prefers_content_over_padding() {
        let panel = Panel::from_text("A").height(4).padding((2, 0)).width(10);

        let segments = panel.render(10);
        let lines = split_lines(segments.into_iter());
        let non_empty_lines = lines
            .iter()
            .filter(|line| line.iter().map(Segment::cell_length).sum::<usize>() > 0)
            .count();

        assert_eq!(non_empty_lines, 4);
        let text: String = lines
            .iter()
            .map(|line| line.iter().map(|seg| seg.text.as_ref()).collect::<String>())
            .collect();
        assert!(text.contains('A'));
    }

    #[test]
    fn test_fit_panel() {
        let panel = fit_panel("Short");
        assert!(!panel.expand);
    }

    #[test]
    fn test_truncate_text_to_width() {
        let text = Text::new("Hello World");
        let truncated = truncate_text_to_width(&text, 5);
        assert_eq!(truncated.plain(), "He...");
    }

    #[test]
    fn test_panel_title_preserves_spans() {
        let mut title = Text::new("AB");
        title.stylize(0, 1, Style::new().italic());

        let panel = Panel::from_text("Content").title(title).width(20);
        let segments = panel.render(20);
        let title_segment = segments
            .iter()
            .find(|seg| seg.text.contains('A'))
            .expect("expected title segment");
        let style = title_segment
            .style
            .as_ref()
            .expect("expected styled segment");
        assert!(style.attributes.contains(Attributes::ITALIC));
    }

    #[test]
    fn test_panel_title_from_markup() {
        let panel = Panel::from_text("Content")
            .title_from_markup("[bold]Styled Title[/]")
            .width(30);
        let segments = panel.render(30);

        // Find the title segment and verify it has bold style
        let title_segment = segments
            .iter()
            .find(|seg| seg.text.contains("Styled Title"))
            .expect("expected title segment with styled text");
        let style = title_segment
            .style
            .as_ref()
            .expect("expected styled segment");
        assert!(
            style.attributes.contains(Attributes::BOLD),
            "title should be bold"
        );
    }

    #[test]
    fn test_panel_subtitle_from_markup() {
        let panel = Panel::from_text("Content")
            .subtitle_from_markup("[italic]Footer[/]")
            .width(30);
        let segments = panel.render(30);

        // Find the subtitle segment and verify it has italic style
        let footer_segment = segments
            .iter()
            .find(|seg| seg.text.contains("Footer"))
            .expect("expected subtitle segment with styled text");
        let style = footer_segment
            .style
            .as_ref()
            .expect("expected styled segment");
        assert!(
            style.attributes.contains(Attributes::ITALIC),
            "subtitle should be italic"
        );
    }

    #[test]
    fn test_panel_title_from_markup_no_markup() {
        // When there's no markup, text should still render correctly
        let panel = Panel::from_text("Content")
            .title_from_markup("Plain Title")
            .width(30);
        let text = panel.render_plain(30);
        assert!(text.contains("Plain Title"));
    }
}
