//! Progressive disclosure levels

/// Disclosure level for skill loading
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisclosureLevel {
    /// ~100 tokens - name + 1-line description
    Minimal,
    /// ~500 tokens - + section headings, key points
    Overview,
    /// ~1500 tokens - + main content, truncated examples
    Standard,
    /// Variable - complete SKILL.md
    Full,
    /// Variable - + scripts/ + references/ + assets/
    Complete,
    /// Auto-select based on context
    Auto,
}

impl Default for DisclosureLevel {
    fn default() -> Self {
        Self::Auto
    }
}
