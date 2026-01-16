//! Skill import module for parsing unstructured prompts and documents.
//!
//! This module provides tools to analyze unstructured text (system prompts,
//! documentation, READMEs) and classify content blocks into appropriate
//! SkillSpec sections (rules, examples, pitfalls, checklists, etc.).
//!
//! # Architecture
//!
//! The import pipeline consists of:
//! 1. **Content Parser** - Splits text into logical blocks
//! 2. **Block Classifiers** - Classify each block by type
//! 3. **Skill Generator** - Transform classified blocks into SkillSpec
//!
//! # Example
//!
//! ```ignore
//! use ms::import::{ContentParser, ContentBlockType};
//!
//! let parser = ContentParser::new();
//! let blocks = parser.parse(prompt_text);
//!
//! for block in blocks {
//!     println!("{:?}: {} (confidence: {:.2})",
//!         block.block_type, block.content.chars().take(50).collect::<String>(), block.confidence);
//! }
//! ```

mod classifiers;
mod parser;
mod types;

pub use classifiers::*;
pub use parser::*;
pub use types::*;
