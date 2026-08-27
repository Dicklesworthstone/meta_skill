use crate::console::{Console, ConsoleOptions};
use crate::emoji::{EmojiVariant, get as get_emoji};
use crate::renderables::Renderable;
use crate::segment::Segment;
use crate::style::Style;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NoEmoji {
    name: String,
}

impl std::fmt::Display for NoEmoji {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "no emoji called {:?}", self.name)
    }
}

impl std::error::Error for NoEmoji {}

/// A single emoji character (Rich-style), optionally with a presentation variant.
#[derive(Debug, Clone)]
pub struct Emoji {
    name: String,
    style: Style,
    variant: Option<EmojiVariant>,
}

impl Emoji {
    /// Construct an Emoji by name (e.g. `"smile"`).
    ///
    /// Returns an error if the emoji name is not known.
    pub fn new(name: impl Into<String>) -> Result<Self, NoEmoji> {
        let name = name.into();
        if get_emoji(&name).is_none() {
            return Err(NoEmoji { name });
        }
        Ok(Self {
            name,
            style: Style::null(),
            variant: None,
        })
    }

    /// Set the style for this emoji.
    #[must_use]
    pub fn style(mut self, style: Style) -> Self {
        self.style = style;
        self
    }

    /// Set the emoji presentation variant.
    #[must_use]
    pub fn variant(mut self, variant: Option<EmojiVariant>) -> Self {
        self.variant = variant;
        self
    }

    /// Get the emoji name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl Renderable for Emoji {
    fn render<'a>(&'a self, console: &Console, _options: &ConsoleOptions) -> Vec<Segment<'a>> {
        // Match Python Rich's Console(emoji=False) expectation: don't emit unicode emoji glyphs.
        if !console.emoji() {
            return vec![Segment::plain(format!(":{name}:", name = self.name))];
        }

        let Some(emoji) = get_emoji(&self.name) else {
            return vec![Segment::plain(format!(":{name}:", name = self.name))];
        };

        let selector = self.variant.map_or("", EmojiVariant::selector);
        let glyph = if selector.is_empty() {
            emoji.to_string()
        } else {
            let mut s = String::with_capacity(emoji.len() + selector.len());
            s.push_str(emoji);
            s.push_str(selector);
            s
        };

        let segment = if self.style.is_null() {
            Segment::plain(glyph)
        } else {
            Segment::styled(glyph, self.style.clone())
        };

        vec![segment]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::style::Attributes;

    // ==========================================================================
    // NoEmoji Error Tests
    // ==========================================================================

    #[test]
    fn test_no_emoji_error_display() {
        let err = NoEmoji {
            name: "unknown_emoji".to_string(),
        };
        assert_eq!(err.to_string(), r#"no emoji called "unknown_emoji""#);
    }

    #[test]
    fn test_no_emoji_error_debug() {
        let err = NoEmoji {
            name: "test".to_string(),
        };
        let debug_str = format!("{err:?}");
        assert!(debug_str.contains("NoEmoji"));
        assert!(debug_str.contains("test"));
    }

    #[test]
    fn test_no_emoji_error_clone() {
        let err1 = NoEmoji {
            name: "foo".to_string(),
        };
        let err2 = err1.clone();
        assert_eq!(err1, err2);
    }

    #[test]
    fn test_no_emoji_is_error() {
        let err: Box<dyn std::error::Error> = Box::new(NoEmoji {
            name: "test".to_string(),
        });
        assert!(err.to_string().contains("no emoji"));
    }

    // ==========================================================================
    // Emoji::new() Tests
    // ==========================================================================

    #[test]
    fn test_emoji_new_valid_name() {
        // "+1" is a valid emoji (thumbs up)
        let emoji = Emoji::new("+1").expect("should create emoji");
        assert_eq!(emoji.name(), "+1");
    }

    #[test]
    fn test_emoji_new_valid_name_smile() {
        let emoji = Emoji::new("smile").expect("should create emoji");
        assert_eq!(emoji.name(), "smile");
    }

    #[test]
    fn test_emoji_new_invalid_name() {
        let result = Emoji::new("this_does_not_exist_12345");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err.name, "this_does_not_exist_12345");
    }

    #[test]
    fn test_emoji_new_empty_string() {
        let result = Emoji::new("");
        assert!(result.is_err());
    }

    #[test]
    fn test_emoji_new_accepts_string() {
        let name = String::from("+1");
        let emoji = Emoji::new(name).expect("should accept String");
        assert_eq!(emoji.name(), "+1");
    }

    // ==========================================================================
    // Emoji Builder Methods Tests
    // ==========================================================================

    #[test]
    fn test_emoji_style_method() {
        let emoji = Emoji::new("+1")
            .expect("should create emoji")
            .style(Style::new().bold());
        // The style should be set (we can verify via render)
        let console = Console::builder().force_terminal(false).build();
        let options = console.options();
        let segments = emoji.render(&console, &options);
        assert_eq!(segments.len(), 1);
        // Styled segment should have a style
        assert!(segments[0].style.is_some());
    }

    #[test]
    fn test_emoji_variant_method() {
        let emoji = Emoji::new("+1")
            .expect("should create emoji")
            .variant(Some(EmojiVariant::Emoji));
        let console = Console::builder().force_terminal(false).build();
        let options = console.options();
        let segments = emoji.render(&console, &options);
        assert_eq!(segments.len(), 1);
        // With variant, the output should include the variant selector
        let text = &segments[0].text;
        assert!(
            text.contains('\u{FE0F}'),
            "should contain emoji variant selector"
        );
    }

    #[test]
    fn test_emoji_variant_text() {
        let emoji = Emoji::new("+1")
            .expect("should create emoji")
            .variant(Some(EmojiVariant::Text));
        let console = Console::builder().force_terminal(false).build();
        let options = console.options();
        let segments = emoji.render(&console, &options);
        let text = &segments[0].text;
        assert!(
            text.contains('\u{FE0E}'),
            "should contain text variant selector"
        );
    }

    #[test]
    fn test_emoji_variant_none() {
        let emoji = Emoji::new("+1").expect("should create emoji").variant(None);
        let console = Console::builder().force_terminal(false).build();
        let options = console.options();
        let segments = emoji.render(&console, &options);
        let text = &segments[0].text;
        assert!(!text.contains('\u{FE0F}'));
        assert!(!text.contains('\u{FE0E}'));
    }

    #[test]
    fn test_emoji_name_getter() {
        let emoji = Emoji::new("100").expect("should create emoji");
        assert_eq!(emoji.name(), "100");
    }

    // ==========================================================================
    // Emoji::render() Tests
    // ==========================================================================

    #[test]
    fn test_emoji_render_basic() {
        let emoji = Emoji::new("+1").expect("should create emoji");
        let console = Console::builder().force_terminal(false).build();
        let options = console.options();
        let segments = emoji.render(&console, &options);
        assert_eq!(segments.len(), 1);
        assert_eq!(&segments[0].text, "👍");
    }

    #[test]
    fn test_emoji_render_100() {
        let emoji = Emoji::new("100").expect("should create emoji");
        let console = Console::builder().force_terminal(false).build();
        let options = console.options();
        let segments = emoji.render(&console, &options);
        assert_eq!(segments.len(), 1);
        assert_eq!(&segments[0].text, "💯");
    }

    #[test]
    fn test_emoji_render_without_style() {
        let emoji = Emoji::new("+1").expect("should create emoji");
        let console = Console::builder().force_terminal(false).build();
        let options = console.options();
        let segments = emoji.render(&console, &options);
        // Without explicit style, segment should have no style
        assert!(
            segments[0].style.is_none() || segments[0].style.as_ref().is_some_and(Style::is_null)
        );
    }

    #[test]
    fn test_emoji_render_with_style() {
        let emoji = Emoji::new("+1")
            .expect("should create emoji")
            .style(Style::new().bold().italic());
        let console = Console::builder().force_terminal(false).build();
        let options = console.options();
        let segments = emoji.render(&console, &options);
        let style = segments[0].style.as_ref().expect("should have style");
        assert!(style.attributes.contains(Attributes::BOLD));
        assert!(style.attributes.contains(Attributes::ITALIC));
    }

    #[test]
    fn test_emoji_render_with_emoji_variant() {
        let emoji = Emoji::new("+1")
            .expect("should create emoji")
            .variant(Some(EmojiVariant::Emoji));
        let console = Console::builder().force_terminal(false).build();
        let options = console.options();
        let segments = emoji.render(&console, &options);
        // Should be emoji + variant selector
        assert_eq!(&segments[0].text, "👍\u{FE0F}");
    }

    #[test]
    fn test_emoji_render_with_text_variant() {
        let emoji = Emoji::new("+1")
            .expect("should create emoji")
            .variant(Some(EmojiVariant::Text));
        let console = Console::builder().force_terminal(false).build();
        let options = console.options();
        let segments = emoji.render(&console, &options);
        // Should be emoji + text variant selector
        assert_eq!(&segments[0].text, "👍\u{FE0E}");
    }

    // ==========================================================================
    // Emoji Clone/Debug Tests
    // ==========================================================================

    #[test]
    fn test_emoji_clone() {
        let emoji1 = Emoji::new("+1")
            .expect("should create emoji")
            .style(Style::new().bold());
        let emoji2 = emoji1.clone();
        assert_eq!(emoji1.name(), emoji2.name());
    }

    #[test]
    fn test_emoji_debug() {
        let emoji = Emoji::new("+1").expect("should create emoji");
        let debug_str = format!("{emoji:?}");
        assert!(debug_str.contains("Emoji"));
        assert!(debug_str.contains("+1"));
    }
}
