//! Core types for content classification and parsing.

use serde::{Deserialize, Serialize};

/// Type of content block identified during parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContentBlockType {
    /// Imperative statements, guidelines, policies.
    /// Examples: "Always handle errors", "Never use eval()"
    Rule,

    /// Code examples with optional context.
    /// Identified by code fences, indentation, or example markers.
    Example,

    /// Warnings, anti-patterns, things to avoid.
    /// Examples: "Warning: Don't...", "Common mistake: ..."
    Pitfall,

    /// Numbered steps, checkbox items, procedural lists.
    /// Examples: "1. First, ...", "- [ ] Check that..."
    Checklist,

    /// Background information, prerequisites, context.
    /// Descriptive text that sets up other content.
    Context,

    /// Title, description candidates, metadata-like content.
    /// Headers, skill name candidates, version info.
    Metadata,

    /// Unclassified content that doesn't fit other categories.
    Unknown,
}

impl ContentBlockType {
    /// Returns all block types in priority order for classification.
    #[must_use]
    pub fn all() -> &'static [Self] {
        &[
            Self::Metadata,
            Self::Rule,
            Self::Example,
            Self::Pitfall,
            Self::Checklist,
            Self::Context,
            Self::Unknown,
        ]
    }

    /// Returns a human-readable name for this block type.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Rule => "rule",
            Self::Example => "example",
            Self::Pitfall => "pitfall",
            Self::Checklist => "checklist",
            Self::Context => "context",
            Self::Metadata => "metadata",
            Self::Unknown => "unknown",
        }
    }
}

impl std::fmt::Display for ContentBlockType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// A signal that contributed to a classification decision.
///
/// Signals provide transparency into why a block was classified
/// a certain way, enabling debugging and refinement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationSignal {
    /// Name of the signal (e.g., "imperative_start", "code_fence")
    pub name: String,

    /// The text that matched this signal
    pub matched: String,

    /// Weight contribution of this signal (typically 0.0 - 1.0)
    pub weight: f32,
}

impl ClassificationSignal {
    /// Create a new classification signal.
    #[must_use]
    pub fn new(name: impl Into<String>, matched: impl Into<String>, weight: f32) -> Self {
        Self {
            name: name.into(),
            matched: matched.into(),
            weight,
        }
    }
}

/// Source location span within the original text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceSpan {
    /// Byte offset of the start
    pub start: usize,

    /// Byte offset of the end (exclusive)
    pub end: usize,

    /// Line number of the start (1-indexed)
    pub start_line: usize,

    /// Line number of the end (1-indexed)
    pub end_line: usize,
}

impl SourceSpan {
    /// Create a new source span.
    #[must_use]
    pub const fn new(start: usize, end: usize, start_line: usize, end_line: usize) -> Self {
        Self {
            start,
            end,
            start_line,
            end_line,
        }
    }

    /// Returns the byte length of this span.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.end.saturating_sub(self.start)
    }

    /// Returns true if the span is empty.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.start >= self.end
    }
}

/// A classified content block from parsed text.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBlock {
    /// The classified type of this block.
    pub block_type: ContentBlockType,

    /// The raw content of this block.
    pub content: String,

    /// Classification confidence (0.0 - 1.0).
    /// Higher values indicate stronger signal matches.
    pub confidence: f32,

    /// Source location in the original text.
    pub span: SourceSpan,

    /// Signals that contributed to the classification.
    pub signals: Vec<ClassificationSignal>,
}

impl ContentBlock {
    /// Create a new content block.
    #[must_use]
    pub fn new(
        block_type: ContentBlockType,
        content: String,
        confidence: f32,
        span: SourceSpan,
        signals: Vec<ClassificationSignal>,
    ) -> Self {
        Self {
            block_type,
            content,
            confidence: confidence.clamp(0.0, 1.0),
            span,
            signals,
        }
    }

    /// Create an unknown block with no confidence.
    #[must_use]
    pub fn unknown(content: String, span: SourceSpan) -> Self {
        Self {
            block_type: ContentBlockType::Unknown,
            content,
            confidence: 0.0,
            span,
            signals: Vec::new(),
        }
    }

    /// Returns true if this block has high confidence (>= 0.7).
    #[must_use]
    pub fn is_high_confidence(&self) -> bool {
        self.confidence >= 0.7
    }

    /// Returns true if this block has medium confidence (>= 0.4).
    #[must_use]
    pub fn is_medium_confidence(&self) -> bool {
        self.confidence >= 0.4
    }

    /// Get the first N characters of content as a preview.
    #[must_use]
    pub fn preview(&self, max_chars: usize) -> String {
        let preview: String = self.content.chars().take(max_chars).collect();
        if self.content.len() > max_chars {
            format!("{}...", preview)
        } else {
            preview
        }
    }
}

/// Result of a classification attempt by a single classifier.
#[derive(Debug, Clone)]
pub struct ClassificationResult {
    /// The determined block type.
    pub block_type: ContentBlockType,

    /// Classification confidence.
    pub confidence: f32,

    /// Signals that contributed to this classification.
    pub signals: Vec<ClassificationSignal>,
}

impl ClassificationResult {
    /// Create a new classification result.
    #[must_use]
    pub fn new(
        block_type: ContentBlockType,
        confidence: f32,
        signals: Vec<ClassificationSignal>,
    ) -> Self {
        Self {
            block_type,
            confidence: confidence.clamp(0.0, 1.0),
            signals,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_block_type_display() {
        assert_eq!(ContentBlockType::Rule.to_string(), "rule");
        assert_eq!(ContentBlockType::Example.to_string(), "example");
        assert_eq!(ContentBlockType::Pitfall.to_string(), "pitfall");
        assert_eq!(ContentBlockType::Checklist.to_string(), "checklist");
        assert_eq!(ContentBlockType::Context.to_string(), "context");
        assert_eq!(ContentBlockType::Metadata.to_string(), "metadata");
        assert_eq!(ContentBlockType::Unknown.to_string(), "unknown");
    }

    #[test]
    fn test_source_span() {
        let span = SourceSpan::new(10, 50, 2, 5);
        assert_eq!(span.len(), 40);
        assert!(!span.is_empty());

        let empty_span = SourceSpan::new(10, 10, 2, 2);
        assert!(empty_span.is_empty());
    }

    #[test]
    fn test_content_block_confidence() {
        let high = ContentBlock::new(
            ContentBlockType::Rule,
            "test".to_string(),
            0.8,
            SourceSpan::new(0, 4, 1, 1),
            vec![],
        );
        assert!(high.is_high_confidence());
        assert!(high.is_medium_confidence());

        let medium = ContentBlock::new(
            ContentBlockType::Rule,
            "test".to_string(),
            0.5,
            SourceSpan::new(0, 4, 1, 1),
            vec![],
        );
        assert!(!medium.is_high_confidence());
        assert!(medium.is_medium_confidence());

        let low = ContentBlock::new(
            ContentBlockType::Rule,
            "test".to_string(),
            0.2,
            SourceSpan::new(0, 4, 1, 1),
            vec![],
        );
        assert!(!low.is_high_confidence());
        assert!(!low.is_medium_confidence());
    }

    #[test]
    fn test_content_block_preview() {
        let block = ContentBlock::new(
            ContentBlockType::Context,
            "This is a long piece of text that should be truncated".to_string(),
            0.5,
            SourceSpan::new(0, 53, 1, 1),
            vec![],
        );
        assert_eq!(block.preview(10), "This is a ...");
        assert_eq!(block.preview(100), "This is a long piece of text that should be truncated");
    }

    #[test]
    fn test_classification_signal() {
        let signal = ClassificationSignal::new("test_signal", "matched text", 0.5);
        assert_eq!(signal.name, "test_signal");
        assert_eq!(signal.matched, "matched text");
        assert!((signal.weight - 0.5).abs() < 0.001);
    }

    #[test]
    fn test_confidence_clamping() {
        let result = ClassificationResult::new(ContentBlockType::Rule, 1.5, vec![]);
        assert!((result.confidence - 1.0).abs() < 0.001);

        let result = ClassificationResult::new(ContentBlockType::Rule, -0.5, vec![]);
        assert!(result.confidence.abs() < 0.001);
    }
}
