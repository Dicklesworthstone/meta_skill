//! Emoji support (Rich-style `:name:` codes).
//!
//! Python Rich supports `:emoji_name:` inline codes with optional variants:
//! `:smile-text:` and `:smile-emoji:`. When enabled on the Console, these
//! codes are replaced with the corresponding unicode emoji + (optional)
//! variant selector.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::LazyLock;

/// Emoji presentation variant selector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EmojiVariant {
    /// Prefer emoji presentation (U+FE0F).
    Emoji,
    /// Prefer text presentation (U+FE0E).
    Text,
}

impl EmojiVariant {
    #[must_use]
    pub const fn selector(self) -> &'static str {
        match self {
            Self::Emoji => "\u{FE0F}",
            Self::Text => "\u{FE0E}",
        }
    }
}

static EMOJI_MAP: LazyLock<HashMap<&'static str, &'static str>> = LazyLock::new(|| {
    let mut map = HashMap::new();
    for line in include_str!("emoji_codes.tsv").lines() {
        let line = line.trim_end();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((name, emoji)) = line.split_once('\t') else {
            continue;
        };
        if name.is_empty() || emoji.is_empty() {
            continue;
        }
        map.insert(name, emoji);
    }
    map
});

/// Look up an emoji by its Rich shortcode name (case-sensitive).
#[must_use]
pub fn get(name: &str) -> Option<&'static str> {
    EMOJI_MAP.get(name).copied()
}

/// Replace Rich-style emoji codes in `text`.
///
/// Matches Python Rich's behavior:
/// - `:smile:` is replaced if the emoji exists (lookup uses lowercased name).
/// - Optional variants: `:smile-emoji:` / `:smile-text:` append U+FE0F / U+FE0E.
/// - Unknown emoji codes are left unchanged.
/// - If `default_variant` is provided, it applies when no explicit variant is present.
#[must_use]
pub fn replace(text: &str, default_variant: Option<EmojiVariant>) -> Cow<'_, str> {
    // Fast path: no colon means no emoji code.
    if !text.as_bytes().contains(&b':') {
        return Cow::Borrowed(text);
    }

    let default_selector = default_variant.map_or("", EmojiVariant::selector);

    let mut cursor = 0;
    let mut search = 0;
    let mut out: Option<String> = None;

    while let Some(rel_start) = text[search..].find(':') {
        let start = search + rel_start;
        if let Some((end, replacement)) = try_replace_at(text, start, default_selector) {
            if let Some(buf) = out.as_mut() {
                buf.push_str(&text[cursor..start]);
                buf.push_str(&replacement);
            } else {
                let mut buf = String::with_capacity(text.len());
                buf.push_str(&text[..start]);
                buf.push_str(&replacement);
                out = Some(buf);
            }
            cursor = end + 1;
            search = cursor;
        } else {
            // Not a valid / known emoji code: keep scanning after the ':'.
            search = start + 1;
        }
    }

    match out {
        None => Cow::Borrowed(text),
        Some(mut buf) => {
            buf.push_str(&text[cursor..]);
            Cow::Owned(buf)
        }
    }
}

fn try_replace_at(
    text: &str,
    start: usize,
    default_selector: &'static str,
) -> Option<(usize, String)> {
    debug_assert_eq!(text.as_bytes()[start], b':');

    let bytes = text.as_bytes();
    let mut i = start + 1;
    let mut end = None;

    while i < bytes.len() {
        let b = bytes[i];
        if b == b':' {
            end = Some(i);
            break;
        }
        if b.is_ascii_whitespace() {
            break;
        }
        i += 1;
    }

    let end = end?;
    let inner = &text[start + 1..end];

    // Variant suffixes are case-sensitive in Python Rich's regex: only "-emoji" / "-text".
    let (name, selector) = if let Some(name) = inner.strip_suffix("-emoji") {
        (name, EmojiVariant::Emoji.selector())
    } else if let Some(name) = inner.strip_suffix("-text") {
        (name, EmojiVariant::Text.selector())
    } else {
        (inner, default_selector)
    };

    // Match Python Rich: lookup lowercased name in the emoji dictionary.
    let emoji_name = name.to_lowercase();
    let emoji = EMOJI_MAP.get(emoji_name.as_str()).copied()?;

    let mut replacement = String::with_capacity(emoji.len() + selector.len());
    replacement.push_str(emoji);
    replacement.push_str(selector);
    Some((end, replacement))
}

#[cfg(test)]
mod tests {
    use super::{EmojiVariant, replace};

    #[test]
    fn test_replace_basic() {
        assert_eq!(replace("hi :smile:", None), "hi 😄");
    }

    #[test]
    fn test_replace_lowercases_name() {
        assert_eq!(replace("hi :SMILE:", None), "hi 😄");
    }

    #[test]
    fn test_replace_variant_text() {
        assert_eq!(
            replace(":smile-text:", None),
            format!("😄{}", EmojiVariant::Text.selector())
        );
    }

    #[test]
    fn test_replace_variant_emoji() {
        assert_eq!(
            replace(":smile-emoji:", None),
            format!("😄{}", EmojiVariant::Emoji.selector())
        );
    }

    #[test]
    fn test_replace_default_variant() {
        assert_eq!(
            replace(":smile:", Some(EmojiVariant::Emoji)),
            format!("😄{}", EmojiVariant::Emoji.selector())
        );
    }

    #[test]
    fn test_unknown_passthrough() {
        assert_eq!(
            replace("hi :definitely_not_real:", None),
            "hi :definitely_not_real:"
        );
    }

    #[test]
    fn test_whitespace_breaks_match() {
        assert_eq!(replace("hi :smile :", None), "hi :smile :");
    }

    #[test]
    fn test_variant_must_be_lowercase() {
        // Matches Python Rich: "-EMOJI" is not recognized as a variant suffix.
        assert_eq!(replace(":smile-EMOJI:", None), ":smile-EMOJI:");
    }
}
