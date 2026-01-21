//! Content parser for splitting and classifying text blocks.
//!
//! The parser splits unstructured text into logical blocks and
//! applies classifiers to determine each block's type.

use super::classifiers::{
    BlockClassifier, ChecklistClassifier, ContextClassifier, ExampleClassifier, MetadataClassifier,
    PitfallClassifier, RuleClassifier,
};
use super::types::{ClassificationResult, ContentBlock, ContentBlockType, SourceSpan};

// =============================================================================
// CONTENT PARSER
// =============================================================================

/// Parser for analyzing and classifying unstructured text content.
///
/// The parser performs two main operations:
/// 1. Splits text into logical blocks (paragraphs, code blocks, lists)
/// 2. Classifies each block using a set of heuristic classifiers
///
/// # Example
///
/// ```ignore
/// let parser = ContentParser::new();
/// let blocks = parser.parse(content);
///
/// for block in blocks {
///     println!("{}: {}", block.block_type, block.preview(50));
/// }
/// ```
pub struct ContentParser {
    classifiers: Vec<Box<dyn BlockClassifier>>,
}

impl Default for ContentParser {
    fn default() -> Self {
        Self::new()
    }
}

impl ContentParser {
    /// Create a new parser with default classifiers.
    #[must_use]
    pub fn new() -> Self {
        Self {
            classifiers: vec![
                Box::new(MetadataClassifier),
                Box::new(ExampleClassifier),
                Box::new(ChecklistClassifier),
                Box::new(PitfallClassifier),
                Box::new(RuleClassifier),
                Box::new(ContextClassifier),
            ],
        }
    }

    /// Create a parser with custom classifiers.
    #[must_use]
    pub fn with_classifiers(classifiers: Vec<Box<dyn BlockClassifier>>) -> Self {
        Self { classifiers }
    }

    /// Parse content into classified blocks.
    ///
    /// This is the main entry point for parsing. It:
    /// 1. Splits the content into raw blocks
    /// 2. Classifies each block
    /// 3. Returns a vector of classified `ContentBlock`s
    #[must_use]
    pub fn parse(&self, content: &str) -> Vec<ContentBlock> {
        let raw_blocks = self.split_into_blocks(content);

        raw_blocks
            .into_iter()
            .map(|(text, span)| {
                let (block_type, confidence, signals) = self.classify_block(&text);
                ContentBlock::new(block_type, text, confidence, span, signals)
            })
            .collect()
    }

    /// Split content into logical blocks.
    ///
    /// Splitting rules:
    /// - Code fences (```) are preserved as single units
    /// - Double newlines separate paragraphs
    /// - Markdown headers start new blocks
    /// - Horizontal rules (---) separate blocks
    /// - List item groups are kept together
    fn split_into_blocks(&self, content: &str) -> Vec<(String, SourceSpan)> {
        let mut blocks = Vec::new();
        let mut current_block = String::new();
        let mut block_start = 0;
        let mut block_start_line = 1;
        let mut current_line = 1;
        let mut in_code_fence = false;
        let mut current_pos = 0;

        for line in content.lines() {
            let line_start = current_pos;
            let line_len = line.len();

            // Check for code fence toggle
            if line.trim().starts_with("```") {
                if in_code_fence {
                    // End of code fence - add line and finish block
                    current_block.push_str(line);
                    current_block.push('\n');
                    in_code_fence = false;

                    // Commit this block
                    let text = current_block.trim().to_string();
                    if !text.is_empty() {
                        blocks.push((
                            text,
                            SourceSpan::new(
                                block_start,
                                current_pos + line_len,
                                block_start_line,
                                current_line,
                            ),
                        ));
                    }
                    current_block = String::new();
                    block_start = current_pos + line_len + 1;
                    block_start_line = current_line + 1;
                } else {
                    // Start of code fence - commit previous block first
                    let text = current_block.trim().to_string();
                    if !text.is_empty() {
                        blocks.push((
                            text,
                            SourceSpan::new(
                                block_start,
                                line_start,
                                block_start_line,
                                current_line - 1,
                            ),
                        ));
                    }
                    current_block = String::new();
                    current_block.push_str(line);
                    current_block.push('\n');
                    block_start = line_start;
                    block_start_line = current_line;
                    in_code_fence = true;
                }
            } else if in_code_fence {
                // Inside code fence - just accumulate
                current_block.push_str(line);
                current_block.push('\n');
            } else if line.is_empty() {
                // Empty line - potential block boundary
                let text = current_block.trim().to_string();
                if !text.is_empty() {
                    blocks.push((
                        text,
                        SourceSpan::new(
                            block_start,
                            line_start,
                            block_start_line,
                            current_line - 1,
                        ),
                    ));
                    current_block = String::new();
                    block_start = current_pos + 1;
                    block_start_line = current_line + 1;
                }
            } else if line.starts_with('#') && !current_block.is_empty() {
                // Markdown header - start new block
                let text = current_block.trim().to_string();
                if !text.is_empty() {
                    blocks.push((
                        text,
                        SourceSpan::new(
                            block_start,
                            line_start,
                            block_start_line,
                            current_line - 1,
                        ),
                    ));
                }
                current_block = line.to_string();
                current_block.push('\n');
                block_start = line_start;
                block_start_line = current_line;
            } else if line.trim() == "---" && current_block.trim().is_empty() {
                // YAML frontmatter start - begin accumulating
                current_block.push_str(line);
                current_block.push('\n');
                block_start = line_start;
                block_start_line = current_line;
            } else if line.trim() == "---" && current_block.trim().starts_with("---") {
                // YAML frontmatter end
                current_block.push_str(line);
                current_block.push('\n');
                let text = current_block.trim().to_string();
                if !text.is_empty() {
                    blocks.push((
                        text,
                        SourceSpan::new(
                            block_start,
                            current_pos + line_len,
                            block_start_line,
                            current_line,
                        ),
                    ));
                }
                current_block = String::new();
                block_start = current_pos + line_len + 1;
                block_start_line = current_line + 1;
            } else if line.trim() == "---" {
                // Horizontal rule - separate blocks
                let text = current_block.trim().to_string();
                if !text.is_empty() {
                    blocks.push((
                        text,
                        SourceSpan::new(
                            block_start,
                            line_start,
                            block_start_line,
                            current_line - 1,
                        ),
                    ));
                }
                current_block = String::new();
                block_start = current_pos + line_len + 1;
                block_start_line = current_line + 1;
            } else {
                // Regular line - accumulate
                current_block.push_str(line);
                current_block.push('\n');
            }

            current_pos += line_len + 1; // +1 for newline
            current_line += 1;
        }

        // Don't forget the last block
        let text = current_block.trim().to_string();
        if !text.is_empty() {
            blocks.push((
                text,
                SourceSpan::new(block_start, current_pos, block_start_line, current_line - 1),
            ));
        }

        blocks
    }

    /// Classify a single block of text.
    ///
    /// Runs all classifiers and returns the highest-confidence result.
    fn classify_block(
        &self,
        block: &str,
    ) -> (
        ContentBlockType,
        f32,
        Vec<super::types::ClassificationSignal>,
    ) {
        let mut best_result: Option<ClassificationResult> = None;

        for classifier in &self.classifiers {
            if let Some(result) = classifier.classify(block) {
                match &best_result {
                    None => best_result = Some(result),
                    Some(current) if result.confidence > current.confidence => {
                        best_result = Some(result);
                    }
                    _ => {}
                }
            }
        }

        best_result
            .map(|r| (r.block_type, r.confidence, r.signals))
            .unwrap_or((ContentBlockType::Unknown, 0.0, vec![]))
    }
}

// =============================================================================
// PARSING STATISTICS
// =============================================================================

/// Statistics about parsed content for debugging and analysis.
#[derive(Debug, Default)]
pub struct ParseStats {
    /// Total number of blocks parsed
    pub total_blocks: usize,
    /// Blocks by type
    pub blocks_by_type: std::collections::HashMap<ContentBlockType, usize>,
    /// Average confidence
    pub avg_confidence: f32,
    /// High confidence blocks (>= 0.7)
    pub high_confidence_count: usize,
    /// Unknown blocks
    pub unknown_count: usize,
}

impl ParseStats {
    /// Compute statistics from a list of parsed blocks.
    #[must_use]
    pub fn from_blocks(blocks: &[ContentBlock]) -> Self {
        let mut stats = Self {
            total_blocks: blocks.len(),
            ..Default::default()
        };

        let mut total_confidence = 0.0;

        for block in blocks {
            *stats.blocks_by_type.entry(block.block_type).or_insert(0) += 1;
            total_confidence += block.confidence;

            if block.is_high_confidence() {
                stats.high_confidence_count += 1;
            }
            if block.block_type == ContentBlockType::Unknown {
                stats.unknown_count += 1;
            }
        }

        if !blocks.is_empty() {
            stats.avg_confidence = total_confidence / blocks.len() as f32;
        }

        stats
    }
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parser_basic() {
        let parser = ContentParser::new();
        let content =
            "# Title\n\nSome context here.\n\n- Always handle errors\n- Never ignore exceptions";
        let blocks = parser.parse(content);

        assert!(!blocks.is_empty());
    }

    #[test]
    fn test_parser_code_fence() {
        let parser = ContentParser::new();
        let content = r#"Some text before.

```rust
fn main() {
    println!("Hello");
}
```

Some text after."#;

        let blocks = parser.parse(content);

        // Should have 3 blocks: text, code, text
        assert_eq!(blocks.len(), 3);

        // The code block should be classified as Example
        let code_block = &blocks[1];
        assert!(code_block.content.contains("fn main()"));
        assert_eq!(code_block.block_type, ContentBlockType::Example);
    }

    #[test]
    fn test_parser_yaml_frontmatter() {
        let parser = ContentParser::new();
        let content = r#"---
id: test-skill
version: 1.0
---

# Test Skill

Some content here."#;

        let blocks = parser.parse(content);

        // Should have at least the YAML block
        assert!(!blocks.is_empty());

        // First block should be metadata
        let first = &blocks[0];
        assert!(first.content.contains("id: test-skill"));
    }

    #[test]
    fn test_parser_mixed_content() {
        let parser = ContentParser::new();
        let content = r#"# Error Handling Guide

This guide covers error handling best practices.

## Rules

- Always handle errors explicitly
- Never ignore exceptions
- Use typed errors when possible

## Example

```python
try:
    do_something()
except ValueError as e:
    log.error(f"Invalid value: {e}")
```

## Warning

⚠️ Don't use bare except clauses - they catch everything including KeyboardInterrupt."#;

        let blocks = parser.parse(content);

        // Should have multiple blocks of different types
        let stats = ParseStats::from_blocks(&blocks);
        assert!(stats.total_blocks >= 4);
    }

    #[test]
    fn test_parser_checklist() {
        let parser = ContentParser::new();
        let content = r#"## Before Deploying

- [ ] Run all tests
- [x] Update version number
- [ ] Review security checklist
- [ ] Notify team"#;

        let blocks = parser.parse(content);

        // Should find a checklist block
        let checklist = blocks
            .iter()
            .find(|b| b.block_type == ContentBlockType::Checklist);
        assert!(checklist.is_some());
    }

    #[test]
    fn test_parse_stats() {
        let parser = ContentParser::new();
        let content = "# Title\n\nContext text.\n\n- Always do this\n- Never do that";
        let blocks = parser.parse(content);
        let stats = ParseStats::from_blocks(&blocks);

        assert_eq!(stats.total_blocks, blocks.len());
        assert!(stats.avg_confidence >= 0.0);
    }

    #[test]
    fn test_empty_content() {
        let parser = ContentParser::new();
        let blocks = parser.parse("");
        assert!(blocks.is_empty());

        let blocks = parser.parse("   \n\n   ");
        assert!(blocks.is_empty());
    }

    #[test]
    fn test_source_spans() {
        let parser = ContentParser::new();
        let content = "First block.\n\nSecond block.";
        let blocks = parser.parse(content);

        assert_eq!(blocks.len(), 2);

        // First block span
        assert_eq!(blocks[0].span.start_line, 1);
        assert!(!blocks[0].span.is_empty());

        // Second block span
        assert!(blocks[1].span.start_line >= 2);
    }

    #[test]
    fn test_classifier_priority() {
        let parser = ContentParser::new();

        // Content that could match multiple classifiers
        // Code fence should win over rules
        let content = "```\nalways handle errors\n```";
        let blocks = parser.parse(content);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].block_type, ContentBlockType::Example);
    }
}
