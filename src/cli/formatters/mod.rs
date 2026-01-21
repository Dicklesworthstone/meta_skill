//! Output formatters for CLI commands
//!
//! Provides structured formatters for common output types that can render
//! to multiple formats (Human, JSON, JSONL, Plain, TSV).

mod search_results;
mod skill_card;
mod suggestion;

pub use search_results::SearchResults;
pub use skill_card::SkillCard;
pub use suggestion::{
    ScorePercentageBreakdown, SuggestionContext, SuggestionItem, SuggestionOutput,
};
