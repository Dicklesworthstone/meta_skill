//! Segment - the atomic rendering unit.
//!
//! A `Segment` is a piece of text with a single style applied. The rendering
//! pipeline produces streams of segments that are then written to the terminal.

use crate::cells::cell_len;
use crate::style::Style;
use smallvec::SmallVec;
use std::borrow::Cow;
use std::fmt;

/// Control codes for terminal manipulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum ControlType {
    Bell = 1,
    CarriageReturn = 2,
    Home = 3,
    Clear = 4,
    ShowCursor = 5,
    HideCursor = 6,
    EnableAltScreen = 7,
    DisableAltScreen = 8,
    CursorUp = 9,
    CursorDown = 10,
    CursorForward = 11,
    CursorBackward = 12,
    CursorMoveToColumn = 13,
    CursorMoveTo = 14,
    EraseInLine = 15,
    SetWindowTitle = 16,
}

/// Remove ASCII control codepoints used by Rich control helpers.
///
/// Python reference: `rich.control.strip_control_codes`.
#[must_use]
pub fn strip_control_codes(text: &str) -> String {
    text.chars()
        .filter(|c| {
            !matches!(
                *c,
                '\x07' // Bell
                | '\x08' // Backspace
                | '\x0b' // Vertical tab
                | '\x0c' // Form feed
                | '\x0d' // Carriage return
            )
        })
        .collect()
}

/// Escape ASCII control codepoints used by Rich control helpers.
///
/// Python reference: `rich.control.escape_control_codes`.
#[must_use]
pub fn escape_control_codes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\x07' => out.push_str("\\a"),
            '\x08' => out.push_str("\\b"),
            '\x0b' => out.push_str("\\v"),
            '\x0c' => out.push_str("\\f"),
            '\x0d' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out
}

/// A control code with optional parameters.
/// Uses `SmallVec` to avoid heap allocation for typical 0-2 parameter cases.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ControlCode {
    pub control_type: ControlType,
    pub params: SmallVec<[i32; 2]>,
}

impl ControlCode {
    /// Create a new control code.
    #[must_use]
    pub fn new(control_type: ControlType) -> Self {
        Self {
            control_type,
            params: SmallVec::new(),
        }
    }

    /// Create a control code with parameters.
    #[must_use]
    pub fn with_params(control_type: ControlType, params: SmallVec<[i32; 2]>) -> Self {
        Self {
            control_type,
            params,
        }
    }

    /// Create a control code with parameters from a Vec (for backward compatibility).
    #[must_use]
    pub fn with_params_vec(control_type: ControlType, params: Vec<i32>) -> Self {
        Self {
            control_type,
            params: SmallVec::from_vec(params),
        }
    }
}

/// The atomic unit of rendering.
///
/// A segment represents a piece of text with a single, consistent style.
/// The rendering pipeline breaks down complex renderables into segments
/// for output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment<'a> {
    /// The text content.
    pub text: Cow<'a, str>,
    /// The style to apply (None = no styling).
    pub style: Option<Style>,
    /// Control codes for terminal manipulation.
    pub control: Option<Vec<ControlCode>>,
}

impl Default for Segment<'_> {
    fn default() -> Self {
        Self::new("", None)
    }
}

impl<'a> Segment<'a> {
    /// Create a new segment with text and optional style.
    #[must_use]
    pub fn new(text: impl Into<Cow<'a, str>>, style: Option<Style>) -> Self {
        Self {
            text: text.into(),
            style,
            control: None,
        }
    }

    /// Create a segment with a style.
    #[must_use]
    pub fn styled(text: impl Into<Cow<'a, str>>, style: Style) -> Self {
        Self::new(text, Some(style))
    }

    /// Create a plain segment with no style.
    #[must_use]
    pub fn plain(text: impl Into<Cow<'a, str>>) -> Self {
        Self::new(text, None)
    }

    /// Create a newline segment.
    #[must_use]
    pub fn line() -> Self {
        Self::new("\n", None)
    }

    /// Create a control segment.
    #[must_use]
    pub fn control(control_codes: Vec<ControlCode>) -> Self {
        Self {
            text: Cow::Borrowed(""),
            style: None,
            control: Some(control_codes),
        }
    }

    /// Convert to an owned segment (static lifetime).
    #[must_use]
    pub fn into_owned(self) -> Segment<'static> {
        Segment {
            text: Cow::Owned(self.text.into_owned()),
            style: self.style,
            control: self.control,
        }
    }

    /// Check if this is a control segment.
    #[must_use]
    pub const fn is_control(&self) -> bool {
        self.control.is_some()
    }

    /// Get the cell width of this segment.
    ///
    /// Control segments have zero width.
    #[must_use]
    pub fn cell_length(&self) -> usize {
        if self.is_control() {
            0
        } else {
            cell_len(&self.text)
        }
    }

    /// Check if this segment is empty (no text or control).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.text.is_empty() && self.control.is_none()
    }

    /// Apply a style to this segment.
    #[must_use]
    pub fn with_style(mut self, style: Style) -> Self {
        self.style = Some(style);
        self
    }

    /// Split this segment at a cell position.
    ///
    /// Returns (left, right) segments.
    #[must_use]
    pub fn split_at_cell(&self, cell_pos: usize) -> (Self, Self) {
        if self.is_control() {
            return (self.clone(), Self::default());
        }

        let mut width = 0;
        let mut byte_pos = 0;

        for (i, c) in self.text.char_indices() {
            let char_width = crate::cells::get_character_cell_size(c);
            if width + char_width > cell_pos {
                break;
            }
            width += char_width;
            byte_pos = i + c.len_utf8();
        }

        // Optimized split using Cow
        let (left, right) = match &self.text {
            Cow::Borrowed(s) => {
                let (l, r) = s.split_at(byte_pos);
                (Cow::Borrowed(l), Cow::Borrowed(r))
            }
            Cow::Owned(s) => {
                let (l, r) = s.split_at(byte_pos);
                (Cow::Owned(l.to_string()), Cow::Owned(r.to_string()))
            }
        };

        (
            Self::new(left, self.style.clone()),
            Self::new(right, self.style.clone()),
        )
    }
}

impl<'a> From<&'a str> for Segment<'a> {
    fn from(value: &'a str) -> Self {
        Self::plain(value)
    }
}

impl From<String> for Segment<'_> {
    fn from(value: String) -> Self {
        Self::plain(value)
    }
}

impl fmt::Display for Segment<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.text)
    }
}

// ============================================================================
// Segment Operations
// ============================================================================

/// Apply styles to an iterator of segments.
pub fn apply_style<'a, I>(
    segments: I,
    style: Option<&'a Style>,
    post_style: Option<&'a Style>,
) -> impl Iterator<Item = Segment<'a>> + 'a
where
    I: Iterator<Item = Segment<'a>> + 'a,
{
    segments.map(move |mut seg| {
        if seg.is_control() {
            return seg;
        }

        if let Some(pre) = style {
            seg.style = Some(match seg.style {
                Some(s) => pre.combine(&s),
                None => pre.clone(),
            });
        }

        if let Some(post) = post_style {
            seg.style = Some(match seg.style {
                Some(s) => s.combine(post),
                None => post.clone(),
            });
        }

        seg
    })
}

/// Split segments into lines at newline characters.
/// Uses direct iterator over `split()` to avoid intermediate Vec allocation.
pub fn split_lines<'a>(segments: impl Iterator<Item = Segment<'a>>) -> Vec<Vec<Segment<'a>>> {
    let mut lines: Vec<Vec<Segment<'a>>> = vec![Vec::new()];

    for segment in segments {
        if segment.is_control() {
            lines.last_mut().expect("at least one line").push(segment);
            continue;
        }

        match segment.text {
            Cow::Borrowed(s) => {
                let mut first = true;
                for part in s.split('\n') {
                    if !first {
                        lines.push(Vec::new());
                    }
                    first = false;
                    if !part.is_empty() {
                        lines
                            .last_mut()
                            .expect("at least one line")
                            .push(Segment::new(part, segment.style.clone()));
                    }
                }
            }
            Cow::Owned(ref s) => {
                let mut first = true;
                for part in s.split('\n') {
                    if !first {
                        lines.push(Vec::new());
                    }
                    first = false;
                    if !part.is_empty() {
                        lines
                            .last_mut()
                            .expect("at least one line")
                            .push(Segment::new(part.to_string(), segment.style.clone()));
                    }
                }
            }
        }
    }

    lines
}

/// Adjust line length by padding or truncating.
#[must_use]
pub fn adjust_line_length(
    mut line: Vec<Segment<'_>>,
    length: usize,
    style: Option<Style>,
    pad: bool,
) -> Vec<Segment<'_>> {
    let current_length: usize = line.iter().map(Segment::cell_length).sum();

    if current_length < length && pad {
        // Pad with spaces
        let padding = length - current_length;
        line.push(Segment::new(" ".repeat(padding), style));
    } else if current_length > length {
        // Truncate
        line = truncate_line(line, length);
    }

    line
}

/// Truncate a line to a maximum cell width.
fn truncate_line(segments: Vec<Segment<'_>>, max_width: usize) -> Vec<Segment<'_>> {
    let mut result = Vec::new();
    let mut remaining = max_width;

    for segment in segments {
        if segment.is_control() {
            result.push(segment);
            continue;
        }

        if remaining == 0 {
            continue;
        }

        let seg_width = segment.cell_length();
        if seg_width <= remaining {
            result.push(segment);
            remaining -= seg_width;
        } else if remaining > 0 {
            let (left, _) = segment.split_at_cell(remaining);
            result.push(left);
            remaining = 0;
        }
    }

    result
}

/// Simplify segments by merging adjacent segments with identical styles.
#[must_use]
pub fn simplify<'a>(segments: impl Iterator<Item = Segment<'a>>) -> Vec<Segment<'a>> {
    let mut result: Vec<Segment<'a>> = Vec::new();

    for segment in segments {
        if segment.is_control() || segment.text.is_empty() {
            if segment.is_control() {
                result.push(segment);
            }
            continue;
        }

        if let Some(last) = result.last_mut()
            && !last.is_control()
            && last.style == segment.style
        {
            // We need to merge text. If last is borrowed and segment is borrowed,
            // and they are adjacent, we could technically merge? No, they are str.
            // Converting to Owned is the only way to append generally.

            let mut last_owned = last.text.clone().into_owned();
            last_owned.push_str(&segment.text);
            last.text = Cow::Owned(last_owned);
            continue;
        }

        result.push(segment);
    }

    result
}

/// Divide segments at specified cell positions.
#[must_use]
pub fn divide<'a>(segments: Vec<Segment<'a>>, cuts: &[usize]) -> Vec<Vec<Segment<'a>>> {
    if cuts.is_empty() {
        return vec![segments];
    }

    let mut result: Vec<Vec<Segment<'a>>> = vec![Vec::new(); cuts.len() + 1];
    let mut current_pos = 0;
    let mut cut_idx = 0;

    for segment in segments {
        if segment.is_control() {
            result[cut_idx].push(segment);
            continue;
        }

        let seg_width = segment.cell_length();
        let seg_end = current_pos + seg_width;

        // Find which divisions this segment spans
        while cut_idx < cuts.len() && cuts[cut_idx] <= current_pos {
            cut_idx += 1;
        }

        if cut_idx >= cuts.len() || seg_end <= cuts[cut_idx] {
            // Segment fits entirely in current division
            let target_idx = cut_idx.min(result.len() - 1);
            result[target_idx].push(segment);
        } else {
            // Segment spans multiple divisions - need to split
            let mut remaining = segment;
            let mut pos = current_pos;

            while cut_idx < cuts.len() && pos + remaining.cell_length() > cuts[cut_idx] {
                let split_at = cuts[cut_idx] - pos;
                let (left, right) = remaining.split_at_cell(split_at);

                if !left.text.is_empty() {
                    result[cut_idx].push(left);
                }

                pos = cuts[cut_idx];
                cut_idx += 1;
                remaining = right;
            }

            if !remaining.text.is_empty() {
                let target_idx = cut_idx.min(result.len() - 1);
                result[target_idx].push(remaining);
            }
        }

        current_pos = seg_end;
    }

    result
}

/// Align lines to the top of a given height.
#[must_use]
#[expect(
    clippy::needless_pass_by_value,
    reason = "style ownership allows caller to avoid clone"
)]
pub fn align_top(
    lines: Vec<Vec<Segment<'_>>>,
    width: usize,
    height: usize,
    style: Style,
) -> Vec<Vec<Segment<'_>>> {
    let mut result = lines;

    // Pad existing lines to width
    for line in &mut result {
        let line_width: usize = line.iter().map(Segment::cell_length).sum();
        if line_width < width {
            line.push(Segment::new(
                " ".repeat(width - line_width),
                Some(style.clone()),
            ));
        }
    }

    // Add blank lines at bottom
    while result.len() < height {
        result.push(vec![Segment::new(" ".repeat(width), Some(style.clone()))]);
    }

    result
}

/// Align lines to the bottom of a given height.
#[must_use]
#[expect(
    clippy::needless_pass_by_value,
    reason = "style ownership allows caller to avoid clone"
)]
pub fn align_bottom(
    lines: Vec<Vec<Segment<'_>>>,
    width: usize,
    height: usize,
    style: Style,
) -> Vec<Vec<Segment<'_>>> {
    let mut result = Vec::new();
    let blank_line = vec![Segment::new(" ".repeat(width), Some(style.clone()))];

    // Add blank lines at top
    let padding = height.saturating_sub(lines.len());
    for _ in 0..padding {
        result.push(blank_line.clone());
    }

    // Add content lines
    for mut line in lines {
        let line_width: usize = line.iter().map(Segment::cell_length).sum();
        if line_width < width {
            line.push(Segment::new(
                " ".repeat(width - line_width),
                Some(style.clone()),
            ));
        }
        result.push(line);
    }

    result
}

/// Align lines to the middle of a given height.
#[must_use]
pub fn align_middle(
    lines: Vec<Vec<Segment<'_>>>,
    width: usize,
    height: usize,
    style: Style,
) -> Vec<Vec<Segment<'_>>> {
    let content_height = lines.len();
    if content_height >= height {
        return align_top(lines, width, height, style);
    }

    let mut result = Vec::new();
    let blank_line = vec![Segment::new(" ".repeat(width), Some(style.clone()))];

    let total_padding = height - content_height;
    let top_padding = total_padding / 2;
    let bottom_padding = total_padding - top_padding;

    // Top padding
    for _ in 0..top_padding {
        result.push(blank_line.clone());
    }

    // Content
    for mut line in lines {
        let line_width: usize = line.iter().map(Segment::cell_length).sum();
        if line_width < width {
            line.push(Segment::new(
                " ".repeat(width - line_width),
                Some(style.clone()),
            ));
        }
        result.push(line);
    }

    // Bottom padding
    for _ in 0..bottom_padding {
        result.push(blank_line.clone());
    }

    result
}

/// Get the total cell length of a line of segments.
#[must_use]
pub fn line_length(line: &[Segment]) -> usize {
    line.iter().map(Segment::cell_length).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;
    use crate::style::Attributes;

    #[test]
    fn test_segment_new() {
        let seg = Segment::new("hello", None);
        assert_eq!(seg.text, "hello");
        assert!(seg.style.is_none());
    }

    #[test]
    fn test_segment_styled() {
        let style = Style::new().bold();
        let seg = Segment::styled("hello", style.clone());
        assert_eq!(seg.style, Some(style));
    }

    #[test]
    fn test_segment_line() {
        let seg = Segment::line();
        assert_eq!(seg.text, "\n");
    }

    #[test]
    fn test_segment_cell_length() {
        let seg = Segment::new("hello", None);
        assert_eq!(seg.cell_length(), 5);
    }

    #[test]
    fn test_segment_control_zero_length() {
        let seg = Segment::control(vec![ControlCode::new(ControlType::Bell)]);
        assert_eq!(seg.cell_length(), 0);
        assert!(seg.is_control());
    }

    #[test]
    fn test_strip_control_codes_removes_expected_codepoints() {
        let input = "a\x07b\x08c\x0bd\x0ce\rf";
        assert_eq!(strip_control_codes(input), "abcdef");
    }

    #[test]
    fn test_escape_control_codes_replaces_expected_codepoints() {
        let input = "a\x07b\x08c\x0bd\x0ce\rf";
        assert_eq!(escape_control_codes(input), "a\\ab\\bc\\vd\\fe\\rf");
    }

    #[test]
    fn test_segment_split_at_cell() {
        let seg = Segment::new("hello world", None);
        let (left, right) = seg.split_at_cell(5);
        assert_eq!(left.text, "hello");
        assert_eq!(right.text, " world");
    }

    #[test]
    fn test_split_lines() {
        let segments = vec![
            Segment::new("line1\nline2", None),
            Segment::new("\nline3", None),
        ];
        let lines = split_lines(segments.into_iter());
        assert_eq!(lines.len(), 3);
    }

    #[test]
    fn test_simplify() {
        let style = Style::new().bold();
        let segments = vec![
            Segment::styled("hello", style.clone()),
            Segment::styled(" ", style.clone()),
            Segment::styled("world", style.clone()),
        ];
        let simplified = simplify(segments.into_iter());
        assert_eq!(simplified.len(), 1);
        assert_eq!(simplified[0].text, "hello world");
    }

    #[test]
    fn test_adjust_line_length_pad() {
        let line = vec![Segment::new("hi", None)];
        let adjusted = adjust_line_length(line, 5, None, true);
        assert_eq!(line_length(&adjusted), 5);
    }

    #[test]
    fn test_adjust_line_length_truncate() {
        let line = vec![Segment::new("hello world", None)];
        let adjusted = adjust_line_length(line, 5, None, false);
        assert_eq!(line_length(&adjusted), 5);
    }

    #[test]
    fn test_divide() {
        let segments = vec![Segment::new("hello world", None)];
        let divided = divide(segments, &[5]);
        assert_eq!(divided.len(), 2);
        assert_eq!(divided[0][0].text, "hello");
        assert_eq!(divided[1][0].text, " world");
    }

    #[test]
    fn test_align_top() {
        let lines = vec![vec![Segment::new("hi", None)]];
        let aligned = align_top(lines, 5, 3, Style::null());
        assert_eq!(aligned.len(), 3);
    }

    #[test]
    fn test_align_bottom() {
        let lines = vec![vec![Segment::new("hi", None)]];
        let aligned = align_bottom(lines, 5, 3, Style::null());
        assert_eq!(aligned.len(), 3);
        // Content should be at bottom
        assert!(aligned[2][0].text.starts_with("hi"));
    }

    #[test]
    fn test_align_middle() {
        let lines = vec![vec![Segment::new("hi", None)]];
        let aligned = align_middle(lines, 5, 3, Style::null());
        assert_eq!(aligned.len(), 3);
        // Content should be in middle
        assert!(aligned[1][0].text.starts_with("hi"));
    }

    // ============================================================================
    // SPEC VALIDATION TESTS - RICH_SPEC.md Section 3 (Segment)
    // ============================================================================

    // 3.1 ControlType Enum - All 16 control types with correct values
    #[test]
    fn test_spec_control_type_values() {
        assert_eq!(ControlType::Bell as u8, 1);
        assert_eq!(ControlType::CarriageReturn as u8, 2);
        assert_eq!(ControlType::Home as u8, 3);
        assert_eq!(ControlType::Clear as u8, 4);
        assert_eq!(ControlType::ShowCursor as u8, 5);
        assert_eq!(ControlType::HideCursor as u8, 6);
        assert_eq!(ControlType::EnableAltScreen as u8, 7);
        assert_eq!(ControlType::DisableAltScreen as u8, 8);
        assert_eq!(ControlType::CursorUp as u8, 9);
        assert_eq!(ControlType::CursorDown as u8, 10);
        assert_eq!(ControlType::CursorForward as u8, 11);
        assert_eq!(ControlType::CursorBackward as u8, 12);
        assert_eq!(ControlType::CursorMoveToColumn as u8, 13);
        assert_eq!(ControlType::CursorMoveTo as u8, 14);
        assert_eq!(ControlType::EraseInLine as u8, 15);
        assert_eq!(ControlType::SetWindowTitle as u8, 16);
    }

    // 3.2 Segment Structure - Fields and methods
    #[test]
    fn test_spec_segment_structure() {
        // Test structure: text, style, control
        let seg = Segment::new("test", None);
        assert_eq!(seg.text, "test");
        assert!(seg.style.is_none());
        assert!(seg.control.is_none());

        // With style
        let style = Style::new().bold();
        let seg = Segment::styled("text", style.clone());
        assert_eq!(seg.style, Some(style));

        // Control segment
        let seg = Segment::control(vec![ControlCode::new(ControlType::Bell)]);
        assert!(seg.control.is_some());
    }

    // 3.2 Segment - cell_length() returns 0 for control, else cell_len(text)
    #[test]
    fn test_spec_segment_cell_length() {
        // Regular segment uses cell_len
        let seg = Segment::new("hello", None);
        assert_eq!(seg.cell_length(), 5);

        // CJK characters count as 2 cells
        let seg = Segment::new("日本", None);
        assert_eq!(seg.cell_length(), 4); // 2 chars * 2 cells

        // Control segment always 0
        let seg = Segment::control(vec![ControlCode::new(ControlType::Bell)]);
        assert_eq!(seg.cell_length(), 0);

        // Control segment with params still 0
        let seg = Segment::control(vec![ControlCode::with_params_vec(
            ControlType::CursorMoveTo,
            vec![1, 2],
        )]);
        assert_eq!(seg.cell_length(), 0);
    }

    // 3.2 Segment - is_control() returns control.is_some()
    #[test]
    fn test_spec_segment_is_control() {
        let seg = Segment::new("text", None);
        assert!(!seg.is_control());

        let seg = Segment::control(vec![ControlCode::new(ControlType::Bell)]);
        assert!(seg.is_control());
    }

    // 3.3 line() - Creates newline segment
    #[test]
    fn test_spec_line_creation() {
        let seg = Segment::line();
        assert_eq!(seg.text, "\n");
        assert!(seg.style.is_none());
        assert!(seg.control.is_none());
    }

    // 3.3 apply_style - Applies style + segment.style or segment.style + post_style
    #[test]
    fn test_spec_apply_style() {
        let red = Style::new().color(Color::parse("red").unwrap());
        let bold = Style::new().bold();

        // Pre-style: style + segment.style
        let segments = vec![Segment::styled("hello", bold.clone())];
        let result: Vec<_> = apply_style(segments.into_iter(), Some(&red), None).collect();
        let combined_style = result[0].style.as_ref().unwrap();
        // The segment should have both red (from pre) and bold (from segment)
        assert!(combined_style.attributes.contains(Attributes::BOLD));

        // Post-style: segment.style + post_style
        let segments = vec![Segment::styled("hello", red.clone())];
        let result: Vec<_> = apply_style(segments.into_iter(), None, Some(&bold)).collect();
        let combined_style = result[0].style.as_ref().unwrap();
        assert!(combined_style.attributes.contains(Attributes::BOLD));

        // Control segments are not modified
        let segments = vec![Segment::control(vec![ControlCode::new(ControlType::Bell)])];
        let result: Vec<_> = apply_style(segments.into_iter(), Some(&red), None).collect();
        assert!(result[0].is_control());
        assert!(result[0].style.is_none());
    }

    // 3.3 split_lines - Splits at newline characters
    #[test]
    fn test_spec_split_lines() {
        // Single newline
        let segments = vec![Segment::new("a\nb", None)];
        let lines = split_lines(segments.into_iter());
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0][0].text, "a");
        assert_eq!(lines[1][0].text, "b");

        // Multiple newlines across segments
        let segments = vec![
            Segment::new("line1\n", None),
            Segment::new("line2\nline3", None),
        ];
        let lines = split_lines(segments.into_iter());
        assert_eq!(lines.len(), 3);

        // Trailing newline creates empty line
        let segments = vec![Segment::new("text\n", None)];
        let lines = split_lines(segments.into_iter());
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[1].len(), 0);

        // Control segments stay on current line
        let segments = vec![
            Segment::new("text", None),
            Segment::control(vec![ControlCode::new(ControlType::Bell)]),
        ];
        let lines = split_lines(segments.into_iter());
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].len(), 2);
    }

    // 3.3 adjust_line_length - Pads or truncates
    #[test]
    fn test_spec_adjust_line_length() {
        // Shorter than length with pad=true: appends padding
        let line = vec![Segment::new("hi", None)];
        let result = adjust_line_length(line, 5, None, true);
        assert_eq!(line_length(&result), 5);

        // Shorter than length with pad=false: no change
        let line = vec![Segment::new("hi", None)];
        let result = adjust_line_length(line, 5, None, false);
        assert_eq!(line_length(&result), 2);

        // Longer than length: truncates (may split segments)
        let line = vec![Segment::new("hello world", None)];
        let result = adjust_line_length(line, 5, None, false);
        assert_eq!(line_length(&result), 5);

        // Control segments never truncated
        let line = vec![
            Segment::control(vec![ControlCode::new(ControlType::Bell)]),
            Segment::new("text", None),
        ];
        let result = adjust_line_length(line, 2, None, false);
        assert!(result[0].is_control()); // Control preserved

        // Control segments after truncation are preserved
        let line = vec![
            Segment::new("text", None),
            Segment::control(vec![ControlCode::new(ControlType::Bell)]),
        ];
        let result = adjust_line_length(line, 2, None, false);
        assert!(result.last().is_some_and(Segment::is_control));
    }

    // 3.3 simplify - Merges contiguous segments with identical styles
    #[test]
    fn test_spec_simplify() {
        let style = Style::new().bold();

        // Merge same styles
        let segments = vec![
            Segment::styled("a", style.clone()),
            Segment::styled("b", style.clone()),
            Segment::styled("c", style.clone()),
        ];
        let result = simplify(segments.into_iter());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "abc");

        // Different styles not merged
        let red = Style::new().color(Color::parse("red").unwrap());
        let segments = vec![
            Segment::styled("a", style.clone()),
            Segment::styled("b", red.clone()),
        ];
        let result = simplify(segments.into_iter());
        assert_eq!(result.len(), 2);

        // Control segments preserved (not merged)
        let segments = vec![
            Segment::styled("a", style.clone()),
            Segment::control(vec![ControlCode::new(ControlType::Bell)]),
            Segment::styled("b", style.clone()),
        ];
        let result = simplify(segments.into_iter());
        assert_eq!(result.len(), 3);
        assert!(result[1].is_control());

        // Empty text segments dropped
        let segments = vec![
            Segment::styled("a", style.clone()),
            Segment::styled("", style.clone()),
            Segment::styled("b", style.clone()),
        ];
        let result = simplify(segments.into_iter());
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].text, "ab");
    }

    // 3.3 divide - Divides segments at specified cell positions
    #[test]
    fn test_spec_divide() {
        // Empty cuts returns single group
        let segments = vec![Segment::new("hello", None)];
        let result = divide(segments, &[]);
        assert_eq!(result.len(), 1);

        // Single cut
        let segments = vec![Segment::new("hello world", None)];
        let result = divide(segments, &[5]);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0][0].text, "hello");
        assert_eq!(result[1][0].text, " world");

        // Multiple cuts
        let segments = vec![Segment::new("abcdefghij", None)];
        let result = divide(segments, &[3, 6]);
        assert_eq!(result.len(), 3);
        assert_eq!(result[0][0].text, "abc");
        assert_eq!(result[1][0].text, "def");
        assert_eq!(result[2][0].text, "ghij");

        // Cut on CJK boundary
        let segments = vec![Segment::new("日本語", None)];
        let result = divide(segments, &[2]); // After first CJK char
        assert_eq!(result.len(), 2);
        assert_eq!(result[0][0].text, "日");
        assert_eq!(result[1][0].text, "本語");

        // Control segments placed in current division
        let segments = vec![
            Segment::control(vec![ControlCode::new(ControlType::Bell)]),
            Segment::new("abc", None),
        ];
        let result = divide(segments, &[2]);
        assert!(result[0][0].is_control());
    }

    // 3.3 Alignment methods
    #[test]
    fn test_spec_align_top() {
        let lines = vec![vec![Segment::new("a", None)]];
        let result = align_top(lines, 3, 3, Style::null());

        // 3 lines total
        assert_eq!(result.len(), 3);
        // Content at top (index 0)
        assert!(result[0][0].text.starts_with('a'));
        // Each line padded to width 3
        for line in &result {
            assert_eq!(line_length(line), 3);
        }
    }

    #[test]
    fn test_spec_align_bottom() {
        let lines = vec![vec![Segment::new("a", None)]];
        let result = align_bottom(lines, 3, 3, Style::null());

        assert_eq!(result.len(), 3);
        // Content at bottom (index 2)
        assert!(result[2][0].text.starts_with('a'));
        // Blank lines at top
        assert!(!result[0][0].text.contains('a'));
        assert!(!result[1][0].text.contains('a'));
    }

    #[test]
    fn test_spec_align_middle() {
        let lines = vec![vec![Segment::new("a", None)]];
        let result = align_middle(lines, 3, 5, Style::null());

        assert_eq!(result.len(), 5);
        // Content in middle (index 2 for 5 lines with 1 content line)
        // top_padding = (5-1)/2 = 2, so content at index 2
        assert!(result[2][0].text.starts_with('a'));
    }

    // Additional: ControlCode with parameters
    #[test]
    fn test_spec_control_code_params() {
        // No params
        let code = ControlCode::new(ControlType::Bell);
        assert!(code.params.is_empty());

        // With params (e.g., CURSOR_MOVE_TO uses x, y)
        let code = ControlCode::with_params_vec(ControlType::CursorMoveTo, vec![10, 20]);
        assert_eq!(code.params.as_slice(), &[10, 20]);
    }

    // Additional: split_at_cell for CJK
    #[test]
    fn test_spec_split_at_cell_cjk() {
        let seg = Segment::new("日本語", None);

        // Split at cell 2 (after first char)
        let (left, right) = seg.split_at_cell(2);
        assert_eq!(left.text, "日");
        assert_eq!(right.text, "本語");

        // Split at cell 3 (middle of second char) - stops before
        let (left, right) = seg.split_at_cell(3);
        assert_eq!(left.text, "日");
        assert_eq!(right.text, "本語");

        // Split at cell 4 (after second char)
        let (left, right) = seg.split_at_cell(4);
        assert_eq!(left.text, "日本");
        assert_eq!(right.text, "語");

        // Control segment split returns clone
        let seg = Segment::control(vec![ControlCode::new(ControlType::Bell)]);
        let (left, right) = seg.split_at_cell(1);
        assert!(left.is_control());
        assert!(!right.is_control()); // Right is default (empty)
    }
}
