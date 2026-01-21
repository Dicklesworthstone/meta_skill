//! Convenience builders for common rich output patterns.
//!
//! This module provides pre-built renderables for common ms output patterns,
//! handling both rich and plain output modes automatically.
//!
//! # Example
//!
//! ```rust,ignore
//! use ms::output::{RichOutput, builders};
//!
//! let output = RichOutput::new(&config, &format, robot_mode);
//!
//! // Build a search results table
//! let table = builders::search_results_table(&results, output.width());
//! output.print_table(&table);
//!
//! // Build a quality bar
//! let bar = builders::quality_bar(0.85, 20);
//! output.println(&bar);
//! ```

use rich_rust::prelude::*;
use rich_rust::renderables::{Panel, Table, Tree};

// ============================================================================
// Search Results
// ============================================================================

/// Build a table for displaying search results.
///
/// Returns a rich_rust Table configured for search result display.
/// The table includes columns for rank, name, score, and layer.
///
/// # Arguments
/// * `results` - Tuples of (name, score, layer, description)
/// * `width` - Terminal width for layout
#[must_use]
pub fn search_results_table(results: &[(&str, f32, &str, &str)], width: usize) -> Table {
    let mut table = Table::new()
        .with_column(Column::new("#").justify(JustifyMethod::Right))
        .with_column(Column::new("Skill").style(Style::new().bold()))
        .with_column(Column::new("Score").justify(JustifyMethod::Right))
        .with_column(Column::new("Layer"))
        .with_column(Column::new("Description"));

    // Calculate description column width (remaining space)
    let fixed_width = 5 + 25 + 8 + 12 + 10; // approx column widths + padding
    let desc_width = width.saturating_sub(fixed_width).max(20);

    for (i, (name, score, layer, description)) in results.iter().enumerate() {
        // Truncate description if needed
        let desc = if description.len() > desc_width {
            format!("{}...", &description[..desc_width.saturating_sub(3)])
        } else {
            (*description).to_string()
        };

        table = table.with_row_cells([
            (i + 1).to_string(),
            (*name).to_string(),
            format!("{score:.2}"),
            (*layer).to_string(),
            desc,
        ]);
    }

    table
}

/// Build a table for displaying search results with IDs.
///
/// Similar to `search_results_table` but includes the skill ID.
#[must_use]
pub fn search_results_table_with_id(results: &[(&str, &str, f32, &str)], width: usize) -> Table {
    let mut table = Table::new()
        .with_column(Column::new("#").justify(JustifyMethod::Right))
        .with_column(Column::new("ID"))
        .with_column(Column::new("Name").style(Style::new().bold()))
        .with_column(Column::new("Score").justify(JustifyMethod::Right))
        .with_column(Column::new("Layer"));

    let _ = width; // Reserved for future layout calculations

    for (i, (id, name, score, layer)) in results.iter().enumerate() {
        table = table.with_row_cells([
            (i + 1).to_string(),
            (*id).to_string(),
            (*name).to_string(),
            format!("{score:.2}"),
            (*layer).to_string(),
        ]);
    }

    table
}

// ============================================================================
// Skill Display
// ============================================================================

/// Default panel width for rendering.
const DEFAULT_PANEL_WIDTH: usize = 80;

/// Build a panel for displaying skill information.
///
/// Creates a bordered panel with the skill name as title and
/// description as content. Returns the rendered string.
#[must_use]
pub fn skill_panel(name: &str, description: &str, layer: &str) -> String {
    skill_panel_with_width(name, description, layer, DEFAULT_PANEL_WIDTH)
}

/// Build a panel for displaying skill information with custom width.
#[must_use]
pub fn skill_panel_with_width(name: &str, description: &str, layer: &str, width: usize) -> String {
    let content = format!("{description}\n\nLayer: {layer}");
    let panel = Panel::from_text(&content)
        .title(name.to_string())
        .border_style(Style::new().color(Color::parse("cyan").unwrap_or(Color::default())));
    panel.render_plain(width)
}

/// Build a panel for displaying skill with full content.
#[must_use]
pub fn skill_detail_panel(
    name: &str,
    description: &str,
    layer: &str,
    quality: f64,
    content: &str,
) -> String {
    skill_detail_panel_with_width(
        name,
        description,
        layer,
        quality,
        content,
        DEFAULT_PANEL_WIDTH,
    )
}

/// Build a panel for displaying skill with full content and custom width.
#[must_use]
pub fn skill_detail_panel_with_width(
    name: &str,
    description: &str,
    layer: &str,
    quality: f64,
    content: &str,
    width: usize,
) -> String {
    let quality_bar_str = quality_bar_plain(quality, 10);
    let header = format!("{description}\n\nLayer: {layer}  Quality: {quality_bar_str}");
    let full_content = format!("{header}\n\n{content}");
    let panel = Panel::from_text(&full_content)
        .title(name.to_string())
        .border_style(Style::new().color(Color::parse("cyan").unwrap_or(Color::default())));
    panel.render_plain(width)
}

// ============================================================================
// Status Display
// ============================================================================

/// Status of a check or operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    /// Check passed successfully
    Ok,
    /// Check completed with warnings
    Warning,
    /// Check failed
    Error,
    /// Check was skipped
    Skipped,
    /// Check is in progress
    InProgress,
}

impl CheckStatus {
    /// Get the icon for this status.
    #[must_use]
    pub const fn icon(&self) -> &'static str {
        match self {
            Self::Ok => "✓",
            Self::Warning => "⚠",
            Self::Error => "✗",
            Self::Skipped => "○",
            Self::InProgress => "◐",
        }
    }

    /// Get the plain text icon for this status.
    #[must_use]
    pub const fn plain_icon(&self) -> &'static str {
        match self {
            Self::Ok => "[OK]",
            Self::Warning => "[WARN]",
            Self::Error => "[ERR]",
            Self::Skipped => "[SKIP]",
            Self::InProgress => "[...]",
        }
    }
}

/// A check result for display in a status tree.
pub struct CheckResult {
    /// Name of the check
    pub name: String,
    /// Status of the check
    pub status: CheckStatus,
    /// Optional message
    pub message: Option<String>,
    /// Child checks (for nested results)
    pub children: Vec<CheckResult>,
}

impl CheckResult {
    /// Create a new check result.
    #[must_use]
    pub fn new(name: impl Into<String>, status: CheckStatus) -> Self {
        Self {
            name: name.into(),
            status,
            message: None,
            children: Vec::new(),
        }
    }

    /// Add a message to this check result.
    #[must_use]
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }

    /// Add a child check result.
    #[must_use]
    pub fn with_child(mut self, child: CheckResult) -> Self {
        self.children.push(child);
        self
    }
}

/// Build a tree for displaying status check results.
///
/// Creates a hierarchical tree showing check status with icons.
#[must_use]
pub fn status_tree(checks: &[CheckResult]) -> Tree {
    fn build_node(check: &CheckResult) -> TreeNode {
        let label = match &check.message {
            Some(msg) => format!("{} {} - {}", check.status.icon(), check.name, msg),
            None => format!("{} {}", check.status.icon(), check.name),
        };

        let mut node = TreeNode::new(label);
        for child in &check.children {
            node = node.child(build_node(child));
        }
        node
    }

    // Create root node
    let root = TreeNode::new("Status").children(checks.iter().map(build_node).collect::<Vec<_>>());

    Tree::new(root)
}

/// Build a status tree with a custom root label.
#[must_use]
pub fn status_tree_with_title(title: &str, checks: &[CheckResult]) -> Tree {
    fn build_node(check: &CheckResult) -> TreeNode {
        let label = match &check.message {
            Some(msg) => format!("{} {} - {}", check.status.icon(), check.name, msg),
            None => format!("{} {}", check.status.icon(), check.name),
        };

        let mut node = TreeNode::new(label);
        for child in &check.children {
            node = node.child(build_node(child));
        }
        node
    }

    let root = TreeNode::new(title).children(checks.iter().map(build_node).collect::<Vec<_>>());

    Tree::new(root)
}

// ============================================================================
// Error Display
// ============================================================================

/// Build a panel for displaying an error.
///
/// Creates a red-bordered panel with the error title and message.
/// Returns the rendered string.
#[must_use]
pub fn error_panel(title: &str, message: &str) -> String {
    error_panel_with_width(title, message, DEFAULT_PANEL_WIDTH)
}

/// Build a panel for displaying an error with custom width.
#[must_use]
pub fn error_panel_with_width(title: &str, message: &str, width: usize) -> String {
    let panel = Panel::from_text(message)
        .title(format!("✗ {title}"))
        .border_style(Style::new().color(Color::parse("red").unwrap_or(Color::default())));
    panel.render_plain(width)
}

/// Build a panel for displaying an error with suggestion.
/// Returns the rendered string.
#[must_use]
pub fn error_panel_with_hint(title: &str, message: &str, hint: &str) -> String {
    error_panel_with_hint_and_width(title, message, hint, DEFAULT_PANEL_WIDTH)
}

/// Build a panel for displaying an error with suggestion and custom width.
#[must_use]
pub fn error_panel_with_hint_and_width(
    title: &str,
    message: &str,
    hint: &str,
    width: usize,
) -> String {
    let content = format!("{message}\n\nHint: {hint}");
    let panel = Panel::from_text(&content)
        .title(format!("✗ {title}"))
        .border_style(Style::new().color(Color::parse("red").unwrap_or(Color::default())));
    panel.render_plain(width)
}

/// Build a panel for displaying a warning.
/// Returns the rendered string.
#[must_use]
pub fn warning_panel(title: &str, message: &str) -> String {
    warning_panel_with_width(title, message, DEFAULT_PANEL_WIDTH)
}

/// Build a panel for displaying a warning with custom width.
#[must_use]
pub fn warning_panel_with_width(title: &str, message: &str, width: usize) -> String {
    let panel = Panel::from_text(message)
        .title(format!("⚠ {title}"))
        .border_style(Style::new().color(Color::parse("yellow").unwrap_or(Color::default())));
    panel.render_plain(width)
}

/// Build a panel for displaying success.
/// Returns the rendered string.
#[must_use]
pub fn success_panel(title: &str, message: &str) -> String {
    success_panel_with_width(title, message, DEFAULT_PANEL_WIDTH)
}

/// Build a panel for displaying success with custom width.
#[must_use]
pub fn success_panel_with_width(title: &str, message: &str, width: usize) -> String {
    let panel = Panel::from_text(message)
        .title(format!("✓ {title}"))
        .border_style(Style::new().color(Color::parse("green").unwrap_or(Color::default())));
    panel.render_plain(width)
}

// ============================================================================
// Progress Display
// ============================================================================

/// Build a progress line string.
///
/// Returns a string like: "Processing: [████████░░] 80% (8/10)"
///
/// # Arguments
/// * `current` - Current progress value
/// * `total` - Total value
/// * `message` - Message to display before the progress bar
/// * `width` - Width of the progress bar (default: 20)
#[must_use]
pub fn progress_line(current: u64, total: u64, message: &str, width: usize) -> String {
    let percentage = if total > 0 {
        (current as f64 / total as f64 * 100.0) as u64
    } else {
        0
    };

    let filled = if total > 0 {
        (current as f64 / total as f64 * width as f64) as usize
    } else {
        0
    };
    let empty = width.saturating_sub(filled);

    let bar = format!("[{}{}]", "█".repeat(filled), "░".repeat(empty));

    format!("{message}: {bar} {percentage}% ({current}/{total})")
}

/// Build a plain text progress line (no Unicode).
#[must_use]
pub fn progress_line_plain(current: u64, total: u64, message: &str, width: usize) -> String {
    let percentage = if total > 0 {
        (current as f64 / total as f64 * 100.0) as u64
    } else {
        0
    };

    let filled = if total > 0 {
        (current as f64 / total as f64 * width as f64) as usize
    } else {
        0
    };
    let empty = width.saturating_sub(filled);

    let bar = format!("[{}{}]", "#".repeat(filled), "-".repeat(empty));

    format!("{message}: {bar} {percentage}% ({current}/{total})")
}

// ============================================================================
// Quality Display
// ============================================================================

/// Build a quality bar string.
///
/// Returns a colored bar representing quality score.
/// Colors: green (>0.7), yellow (>0.4), red (<=0.4)
///
/// # Arguments
/// * `score` - Quality score (0.0 - 1.0)
/// * `width` - Width of the bar
#[must_use]
pub fn quality_bar(score: f64, width: usize) -> String {
    let filled = (score * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);

    let bar_char = "█";
    let empty_char = "░";

    // Determine color based on score
    let color = if score > 0.7 {
        "green"
    } else if score > 0.4 {
        "yellow"
    } else {
        "red"
    };

    format!(
        "[{color}]{}[/]{} {:.0}%",
        bar_char.repeat(filled),
        empty_char.repeat(empty),
        score * 100.0
    )
}

/// Build a plain text quality bar (no ANSI codes).
#[must_use]
pub fn quality_bar_plain(score: f64, width: usize) -> String {
    let filled = (score * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);

    format!(
        "[{}{}] {:.0}%",
        "#".repeat(filled),
        "-".repeat(empty),
        score * 100.0
    )
}

/// Build a quality indicator with label.
#[must_use]
pub fn quality_indicator(score: f64) -> String {
    let label = if score >= 0.9 {
        "Excellent"
    } else if score >= 0.7 {
        "Good"
    } else if score >= 0.5 {
        "Fair"
    } else if score >= 0.3 {
        "Poor"
    } else {
        "Very Poor"
    };

    format!("{} ({:.0}%)", label, score * 100.0)
}

// ============================================================================
// Key-Value Display
// ============================================================================

/// Build a key-value table.
///
/// Creates a two-column table for displaying key-value pairs.
#[must_use]
pub fn key_value_table(pairs: &[(&str, &str)]) -> Table {
    let mut table = Table::new()
        .with_column(Column::new("Key").style(Style::new().bold()))
        .with_column(Column::new("Value"));

    for (key, value) in pairs {
        table = table.with_row_cells([(*key).to_string(), (*value).to_string()]);
    }

    table
}

/// Format key-value pairs as plain text.
#[must_use]
pub fn key_value_plain(pairs: &[(&str, &str)], separator: &str) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{k}{separator}{v}"))
        .collect::<Vec<_>>()
        .join("\n")
}

// ============================================================================
// List Display
// ============================================================================

/// Build a bulleted list as a string.
#[must_use]
pub fn bulleted_list(items: &[&str]) -> String {
    items
        .iter()
        .map(|item| format!("• {item}"))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Build a numbered list as a string.
#[must_use]
pub fn numbered_list(items: &[&str]) -> String {
    items
        .iter()
        .enumerate()
        .map(|(i, item)| format!("{}. {item}", i + 1))
        .collect::<Vec<_>>()
        .join("\n")
}

/// Build a plain text bulleted list (ASCII only).
#[must_use]
pub fn bulleted_list_plain(items: &[&str]) -> String {
    items
        .iter()
        .map(|item| format!("- {item}"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_progress_line() {
        let line = progress_line(5, 10, "Processing", 10);
        assert!(line.contains("Processing"));
        assert!(line.contains("50%"));
        assert!(line.contains("5/10"));
    }

    #[test]
    fn test_progress_line_zero_total() {
        let line = progress_line(0, 0, "Test", 10);
        assert!(line.contains("0%"));
        assert!(line.contains("0/0"));
    }

    #[test]
    fn test_quality_bar() {
        let bar = quality_bar(0.85, 10);
        assert!(bar.contains("85%"));
        assert!(bar.contains("green"));
    }

    #[test]
    fn test_quality_bar_yellow() {
        let bar = quality_bar(0.5, 10);
        assert!(bar.contains("50%"));
        assert!(bar.contains("yellow"));
    }

    #[test]
    fn test_quality_bar_red() {
        let bar = quality_bar(0.2, 10);
        assert!(bar.contains("20%"));
        assert!(bar.contains("red"));
    }

    #[test]
    fn test_quality_bar_plain() {
        let bar = quality_bar_plain(0.5, 10);
        assert!(bar.contains("50%"));
        assert!(bar.contains("#"));
        assert!(bar.contains("-"));
        assert!(!bar.contains("[green]"));
    }

    #[test]
    fn test_quality_indicator() {
        assert!(quality_indicator(0.95).contains("Excellent"));
        assert!(quality_indicator(0.75).contains("Good"));
        assert!(quality_indicator(0.55).contains("Fair"));
        assert!(quality_indicator(0.35).contains("Poor"));
        assert!(quality_indicator(0.15).contains("Very Poor"));
    }

    #[test]
    fn test_bulleted_list() {
        let list = bulleted_list(&["Item 1", "Item 2"]);
        assert!(list.contains("• Item 1"));
        assert!(list.contains("• Item 2"));
    }

    #[test]
    fn test_numbered_list() {
        let list = numbered_list(&["First", "Second"]);
        assert!(list.contains("1. First"));
        assert!(list.contains("2. Second"));
    }

    #[test]
    fn test_key_value_plain() {
        let kv = key_value_plain(&[("Key1", "Val1"), ("Key2", "Val2")], ": ");
        assert!(kv.contains("Key1: Val1"));
        assert!(kv.contains("Key2: Val2"));
    }

    #[test]
    fn test_check_status_icons() {
        assert_eq!(CheckStatus::Ok.icon(), "✓");
        assert_eq!(CheckStatus::Error.icon(), "✗");
        assert_eq!(CheckStatus::Ok.plain_icon(), "[OK]");
        assert_eq!(CheckStatus::Error.plain_icon(), "[ERR]");
    }

    #[test]
    fn test_check_result_builder() {
        let result = CheckResult::new("Database", CheckStatus::Ok)
            .with_message("Connected")
            .with_child(CheckResult::new("Tables", CheckStatus::Ok));

        assert_eq!(result.name, "Database");
        assert_eq!(result.status, CheckStatus::Ok);
        assert!(result.message.is_some());
        assert_eq!(result.children.len(), 1);
    }

    #[test]
    fn test_search_results_table() {
        let results = vec![
            ("skill-1", 0.95_f32, "user", "Test skill"),
            ("skill-2", 0.85_f32, "global", "Another skill"),
        ];
        let table = search_results_table(&results, 80);
        // Table should be created without panicking
        let _ = table;
    }
}
