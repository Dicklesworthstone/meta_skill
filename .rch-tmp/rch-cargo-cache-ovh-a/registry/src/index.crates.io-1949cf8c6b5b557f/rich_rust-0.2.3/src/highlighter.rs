//! Highlighters (Python Rich `rich.highlighter` parity).
//!
//! In Python Rich, a "highlighter" post-processes a `Text` instance and adds style spans
//! based on regex matches. This is used by `Console` when `highlight=True`.

use std::sync::Arc;

use crate::console::Console;
use crate::style::Style;
use crate::text::Text;

/// A highlighter modifies a [`Text`] in-place by adding style spans.
///
/// This mirrors Python Rich's `Highlighter.highlight(text)` contract.
pub trait Highlighter: Send + Sync {
    /// Apply highlighting to `text`.
    fn highlight(&self, console: &Console, text: &mut Text);
}

/// A no-op highlighter.
#[derive(Debug, Default, Clone, Copy)]
pub struct NullHighlighter;

impl Highlighter for NullHighlighter {
    fn highlight(&self, _console: &Console, _text: &mut Text) {}
}

/// An error compiling or executing a highlighter regex.
#[derive(Debug, Clone)]
pub struct HighlighterRegexError {
    pattern: String,
    message: String,
}

impl std::fmt::Display for HighlighterRegexError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "highlighter regex error for {:?}: {}",
            self.pattern, self.message
        )
    }
}

impl std::error::Error for HighlighterRegexError {}

/// Regex-based highlighter, compatible with Python Rich `RegexHighlighter`.
///
/// Each regex may contain multiple *named* capture groups. For every group match,
/// a style named `{base_style}{group_name}` is applied to that range.
///
/// Example:
/// - `base_style = "repr."`
/// - capture group name `number` -> style name `repr.number`
#[derive(Clone)]
pub struct RegexHighlighter {
    base_style: String,
    highlights: Vec<Arc<fancy_regex::Regex>>,
}

impl std::fmt::Debug for RegexHighlighter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RegexHighlighter")
            .field("base_style", &self.base_style)
            .field("highlights_len", &self.highlights.len())
            .finish()
    }
}

impl RegexHighlighter {
    /// Create a new `RegexHighlighter`.
    pub fn new(
        base_style: impl Into<String>,
        highlights: &[&str],
    ) -> Result<Self, HighlighterRegexError> {
        let mut compiled: Vec<Arc<fancy_regex::Regex>> = Vec::with_capacity(highlights.len());
        for pattern in highlights {
            let re = fancy_regex::Regex::new(pattern).map_err(|e| HighlighterRegexError {
                pattern: (*pattern).to_string(),
                message: e.to_string(),
            })?;
            compiled.push(Arc::new(re));
        }
        Ok(Self {
            base_style: base_style.into(),
            highlights: compiled,
        })
    }

    /// Add a highlight regex.
    pub fn push(&mut self, pattern: &str) -> Result<(), HighlighterRegexError> {
        let re = fancy_regex::Regex::new(pattern).map_err(|e| HighlighterRegexError {
            pattern: pattern.to_string(),
            message: e.to_string(),
        })?;
        self.highlights.push(Arc::new(re));
        Ok(())
    }

    fn apply_regex(&self, console: &Console, text: &mut Text, re: &fancy_regex::Regex) {
        // Collect all style operations first, then apply them.
        // This avoids holding an immutable borrow on `text` while mutating it.
        let ops = {
            let plain = text.plain();
            if plain.is_empty() {
                return;
            }

            // Map byte indices -> char indices once (O(n)), then convert matches with O(log n).
            let char_starts: Vec<usize> = plain.char_indices().map(|(i, _)| i).collect();
            let total_chars = char_starts.len();
            let total_bytes = plain.len();

            let mut ops: Vec<(usize, usize, Style)> = Vec::new();
            let iter = re.captures_iter(plain);
            for next in iter {
                let Ok(caps) = next else {
                    break; // runtime regex error; don't take down rendering
                };

                // Skip 0 = whole match; we only care about named groups.
                for (group_index, group_name) in re.capture_names().enumerate() {
                    let Some(name) = group_name else {
                        continue;
                    };
                    if group_index == 0 {
                        continue;
                    }
                    let Some(m) = caps.get(group_index) else {
                        continue;
                    };

                    let byte_start = m.start();
                    let byte_end = m.end();
                    if byte_start >= byte_end || byte_start > total_bytes || byte_end > total_bytes
                    {
                        continue;
                    }

                    let char_start = char_starts.binary_search(&byte_start).unwrap_or_else(|x| x);
                    let char_end = if byte_end == total_bytes {
                        total_chars
                    } else {
                        char_starts.binary_search(&byte_end).unwrap_or_else(|x| x)
                    };

                    if char_start >= char_end {
                        continue;
                    }

                    let style_name = format!("{}{}", self.base_style, name);
                    let style = console.get_style(&style_name);
                    if style == Style::default() {
                        continue;
                    }
                    ops.push((char_start, char_end, style));
                }
            }
            ops
        }; // immutable borrow on `text` released here

        for (start, end, style) in ops {
            text.stylize(start, end, style);
        }
    }
}

impl Highlighter for RegexHighlighter {
    fn highlight(&self, console: &Console, text: &mut Text) {
        for re in &self.highlights {
            self.apply_regex(console, text, re);
        }
    }
}

// Patterns from Python Rich `rich.highlighter.ReprHighlighter` (as shipped with Rich 13.x).
// We intentionally compile them with fancy-regex to preserve look-around semantics.
const REPR_HIGHLIGHTS: &[&str] = &[
    r"(?P<tag_start><)(?P<tag_name>[-\w.:|]*)(?P<tag_contents>[\w\W]*)(?P<tag_end>>)",
    r#"(?P<attrib_name>[\w_]{1,50})=(?P<attrib_value>"?[\w_]+"?)?"#,
    r"(?P<brace>[\\[\\]{}()])",
    r#"(?P<ipv4>[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3}\.[0-9]{1,3})|(?P<ipv6>([A-Fa-f0-9]{1,4}::?){1,7}[A-Fa-f0-9]{1,4})|(?P<eui64>(?:[0-9A-Fa-f]{1,2}-){7}[0-9A-Fa-f]{1,2}|(?:[0-9A-Fa-f]{1,2}:){7}[0-9A-Fa-f]{1,2}|(?:[0-9A-Fa-f]{4}\.){3}[0-9A-Fa-f]{4})|(?P<eui48>(?:[0-9A-Fa-f]{1,2}-){5}[0-9A-Fa-f]{1,2}|(?:[0-9A-Fa-f]{1,2}:){5}[0-9A-Fa-f]{1,2}|(?:[0-9A-Fa-f]{4}\.){2}[0-9A-Fa-f]{4})|(?P<uuid>[a-fA-F0-9]{8}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{4}-[a-fA-F0-9]{12})|(?P<call>[\w.]*?)\(|\b(?P<bool_true>True)\b|\b(?P<bool_false>False)\b|\b(?P<none>None)\b|(?P<ellipsis>\.\.\.)|(?P<number_complex>(?<!\w)(?:\-?[0-9]+\.?[0-9]*(?:e[-+]?\d+?)?)(?:[-+](?:[0-9]+\.?[0-9]*(?:e[-+]?\d+)?))?j)|(?P<number>(?<!\w)\-?[0-9]+\.?[0-9]*(e[-+]?\d+?)?\b|0x[0-9a-fA-F]*)|(?P<path>\B(/[-\w._+]+)*\/)(?P<filename>[-\w._+]*)?|(?<![\\w])(?P<str>b?'''.*?(?<!\\)'''|b?'.*?(?<!\\)'|b?""".*?(?<!\\)"""|b?".*?(?<!\\)")|(?P<url>(file|https|http|ws|wss)://[-0-9a-zA-Z$_+!`(),.?/;:&=%#~@]*)"#,
];

/// Default "repr" highlighter, mirroring Python Rich `ReprHighlighter`.
#[derive(Debug, Clone)]
pub struct ReprHighlighter {
    inner: RegexHighlighter,
}

impl Default for ReprHighlighter {
    fn default() -> Self {
        let inner =
            RegexHighlighter::new("repr.", REPR_HIGHLIGHTS).unwrap_or_else(|_| RegexHighlighter {
                base_style: "repr.".to_string(),
                highlights: Vec::new(),
            });
        Self { inner }
    }
}

impl Highlighter for ReprHighlighter {
    fn highlight(&self, console: &Console, text: &mut Text) {
        self.inner.highlight(console, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_repr_highlighter_patterns_compile() {
        for pattern in REPR_HIGHLIGHTS {
            fancy_regex::Regex::new(pattern).unwrap_or_else(|err| {
                std::panic::panic_any(format!(
                    "ReprHighlighter pattern failed to compile: {pattern:?}: {err}"
                ))
            });
        }
    }

    #[test]
    fn test_repr_highlighter_applies_named_styles() {
        let console = Console::new();
        let mut text = Text::new("True False None 123 0xFF 'hi' (x) ...");
        ReprHighlighter::default().highlight(&console, &mut text);

        let repr_true = console.get_style("repr.bool_true");
        let repr_false = console.get_style("repr.bool_false");
        let repr_none = console.get_style("repr.none");
        let repr_number = console.get_style("repr.number");
        let repr_str = console.get_style("repr.str");
        let repr_brace = console.get_style("repr.brace");
        let repr_ellipsis = console.get_style("repr.ellipsis");

        let styles: Vec<Style> = text.spans().iter().map(|s| s.style.clone()).collect();
        assert!(styles.contains(&repr_true));
        assert!(styles.contains(&repr_false));
        assert!(styles.contains(&repr_none));
        assert!(styles.contains(&repr_number));
        assert!(styles.contains(&repr_str));
        assert!(styles.contains(&repr_brace));
        assert!(styles.contains(&repr_ellipsis));
    }

    #[test]
    fn test_null_highlighter_noop() {
        let console = Console::new();
        let mut text = Text::new("123");
        NullHighlighter.highlight(&console, &mut text);
        assert_eq!(text.spans().len(), 0);
    }
}
