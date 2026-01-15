//! Session quality scoring for CASS mining
//!
//! Scores sessions for signal quality before pattern extraction.
//! Low-quality sessions (abandoned, no resolution, excessive backtracking)
//! are filtered out to prevent polluting the skill corpus with noise.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{Session, SessionMessage};

/// Quality score and contributing signals for a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionQuality {
    /// Normalized score from 0.0 to 1.0
    pub score: f32,
    /// Signals that contributed positively or negatively
    pub signals: Vec<String>,
    /// Missing signals that could have improved the score
    pub missing: Vec<MissingSignal>,
    /// When this score was computed
    pub computed_at: DateTime<Utc>,
}

impl SessionQuality {
    /// Check if the session passes the quality threshold
    pub fn passes_threshold(&self, config: &QualityConfig) -> bool {
        self.score >= config.min_score
    }

    /// Get a human-readable summary of the quality assessment
    pub fn summary(&self) -> String {
        let grade = if self.score >= 0.8 {
            "excellent"
        } else if self.score >= 0.6 {
            "good"
        } else if self.score >= 0.4 {
            "fair"
        } else if self.score >= 0.2 {
            "poor"
        } else {
            "very poor"
        };

        format!(
            "Quality: {} ({:.0}%) - {} positive signals, {} missing",
            grade,
            self.score * 100.0,
            self.signals.len(),
            self.missing.len()
        )
    }
}

/// Signals that could improve session quality if present
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingSignal {
    /// No test execution or test results found
    NoTestsPassed,
    /// No explicit user confirmation of success
    NoUserConfirmation,
    /// No clear resolution marker at end
    NoClearResolution,
    /// No code changes were made
    NoCodeChanges,
    /// Session is too short to be meaningful
    TooShort,
    /// Session is excessively long (may indicate thrashing)
    TooLong,
}

impl MissingSignal {
    /// Get a human-readable description
    pub fn description(&self) -> &'static str {
        match self {
            Self::NoTestsPassed => "No tests were run or passed",
            Self::NoUserConfirmation => "User did not confirm success",
            Self::NoClearResolution => "No clear completion marker found",
            Self::NoCodeChanges => "No code changes were made",
            Self::TooShort => "Session is too short to contain meaningful patterns",
            Self::TooLong => "Session is excessively long, may indicate thrashing",
        }
    }
}

/// Configuration for quality scoring thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityConfig {
    /// Minimum score to pass quality gate (default: 0.3)
    pub min_score: f32,
    /// Minimum number of turns required (default: 3)
    pub min_turns: usize,
    /// Maximum turns before penalizing (default: 500)
    pub max_turns: usize,
    /// Whether to require code changes (default: false)
    pub require_code_changes: bool,

    // Signal weights (positive)
    /// Weight for tests passing signal
    pub weight_tests_passed: f32,
    /// Weight for clear resolution signal
    pub weight_clear_resolution: f32,
    /// Weight for code changes signal
    pub weight_code_changes: f32,
    /// Weight for user confirmation signal
    pub weight_user_confirmed: f32,

    // Signal weights (negative)
    /// Penalty for backtracking
    pub penalty_backtracking: f32,
    /// Penalty for abandoned session
    pub penalty_abandoned: f32,
}

impl Default for QualityConfig {
    fn default() -> Self {
        Self {
            min_score: 0.3,
            min_turns: 3,
            max_turns: 500,
            require_code_changes: false,

            weight_tests_passed: 0.25,
            weight_clear_resolution: 0.25,
            weight_code_changes: 0.15,
            weight_user_confirmed: 0.15,

            penalty_backtracking: 0.10,
            penalty_abandoned: 0.20,
        }
    }
}

/// Computes quality scores for sessions
pub struct QualityScorer {
    config: QualityConfig,
}

impl QualityScorer {
    /// Create a new quality scorer with the given config
    pub fn new(config: QualityConfig) -> Self {
        Self { config }
    }

    /// Create a quality scorer with default config
    pub fn with_defaults() -> Self {
        Self::new(QualityConfig::default())
    }

    /// Score a session for quality
    pub fn score(&self, session: &Session) -> SessionQuality {
        let mut score: f32 = 0.0;
        let mut signals = Vec::new();
        let mut missing = Vec::new();

        let message_count = session.messages.len();

        // Check minimum/maximum turn bounds
        if message_count < self.config.min_turns {
            missing.push(MissingSignal::TooShort);
        }
        if message_count > self.config.max_turns {
            missing.push(MissingSignal::TooLong);
            // Small penalty for very long sessions
            score -= 0.05;
        }

        // Positive signals
        if has_tests_passed(&session.messages) {
            score += self.config.weight_tests_passed;
            signals.push("tests_passed".to_string());
        } else {
            missing.push(MissingSignal::NoTestsPassed);
        }

        if has_clear_resolution(&session.messages) {
            score += self.config.weight_clear_resolution;
            signals.push("clear_resolution".to_string());
        } else {
            missing.push(MissingSignal::NoClearResolution);
        }

        if has_code_changes(&session.messages) {
            score += self.config.weight_code_changes;
            signals.push("code_changes".to_string());
        } else {
            missing.push(MissingSignal::NoCodeChanges);
            if self.config.require_code_changes {
                // Additional penalty if code changes required
                score -= 0.10;
            }
        }

        if has_user_confirmation(&session.messages) {
            score += self.config.weight_user_confirmed;
            signals.push("user_confirmed".to_string());
        } else {
            missing.push(MissingSignal::NoUserConfirmation);
        }

        // Negative signals
        if has_backtracking(&session.messages) {
            score -= self.config.penalty_backtracking;
            signals.push("backtracking".to_string());
        }

        if is_abandoned(&session.messages) {
            score -= self.config.penalty_abandoned;
            signals.push("abandoned".to_string());
        }

        // Normalize score to 0.0-1.0 range
        let score = score.clamp(0.0, 1.0);

        SessionQuality {
            score,
            signals,
            missing,
            computed_at: Utc::now(),
        }
    }

    /// Get the configuration
    pub fn config(&self) -> &QualityConfig {
        &self.config
    }
}

impl Default for QualityScorer {
    fn default() -> Self {
        Self::with_defaults()
    }
}

// =============================================================================
// Signal Detection Functions
// =============================================================================

/// Check if tests were run and passed
fn has_tests_passed(messages: &[SessionMessage]) -> bool {
    for msg in messages {
        // Check tool results for test execution
        for result in &msg.tool_results {
            let content_lower = result.content.to_lowercase();

            // Positive test indicators
            if content_lower.contains("tests passed")
                || content_lower.contains("all tests passing")
                || content_lower.contains("test passed")
                || (content_lower.contains("ok") && content_lower.contains("test"))
            {
                return true;
            }

            // pytest success patterns
            if content_lower.contains("pytest") && !content_lower.contains("failed") {
                if content_lower.contains("passed") {
                    return true;
                }
            }

            // cargo test success - require success indicator, not just "cargo test"
            if content_lower.contains("cargo test")
                && !content_lower.contains("failed")
                && (content_lower.contains("ok") || content_lower.contains("passed"))
            {
                return true;
            }
            // cargo test output pattern (e.g., "running 5 tests" ... "test foo ... ok")
            if content_lower.contains("running")
                && content_lower.contains("test")
                && content_lower.contains("ok")
                && !content_lower.contains("failed")
            {
                return true;
            }

            // npm/jest test success
            if (content_lower.contains("jest") || content_lower.contains("npm test"))
                && content_lower.contains("passed")
            {
                return true;
            }

            // go test success
            if content_lower.contains("go test") && content_lower.contains("pass") {
                return true;
            }
        }

        // Also check assistant content for test result mentions
        if msg.role == "assistant" {
            let content_lower = msg.content.to_lowercase();
            if content_lower.contains("all tests pass")
                || content_lower.contains("tests are passing")
                || content_lower.contains("tests pass")
            {
                return true;
            }
        }
    }
    false
}

/// Check if the session has a clear resolution marker
fn has_clear_resolution(messages: &[SessionMessage]) -> bool {
    // Check the last few assistant messages for resolution markers
    let assistant_messages: Vec<_> = messages.iter().filter(|m| m.role == "assistant").collect();

    // Check last 3 assistant messages
    for msg in assistant_messages.iter().rev().take(3) {
        let content_lower = msg.content.to_lowercase();

        // Common resolution markers
        if content_lower.contains("completed")
            || content_lower.contains("done")
            || content_lower.contains("finished")
            || content_lower.contains("implemented")
            || content_lower.contains("fixed the")
            || content_lower.contains("resolved")
            || content_lower.contains("successfully")
            || content_lower.contains("task is complete")
            || content_lower.contains("changes have been made")
            || content_lower.contains("ready for review")
            || content_lower.contains("pushed to")
            || content_lower.contains("committed")
        {
            return true;
        }
    }
    false
}

/// Check if code changes were made in the session
fn has_code_changes(messages: &[SessionMessage]) -> bool {
    for msg in messages {
        // Check tool calls for write/edit operations
        for tool_call in &msg.tool_calls {
            let name_lower = tool_call.name.to_lowercase();
            if name_lower.contains("write")
                || name_lower.contains("edit")
                || name_lower.contains("create")
                || name_lower.contains("modify")
                || name_lower == "bash"
            {
                // For bash, check if it's a git commit or write operation
                if name_lower == "bash" {
                    if let Some(cmd) = tool_call.arguments.get("command").and_then(|v| v.as_str()) {
                        let cmd_lower = cmd.to_lowercase();
                        if cmd_lower.contains("git commit")
                            || cmd_lower.contains("git add")
                            || cmd_lower.contains("echo")
                            || cmd_lower.contains(">")
                            || cmd_lower.contains("sed")
                            || cmd_lower.contains("patch")
                        {
                            return true;
                        }
                    }
                } else {
                    return true;
                }
            }
        }
    }
    false
}

/// Check if user confirmed success
fn has_user_confirmation(messages: &[SessionMessage]) -> bool {
    // Check user messages in the last portion of the conversation
    let user_messages: Vec<_> = messages.iter().filter(|m| m.role == "user").collect();

    // Check last 5 user messages
    for msg in user_messages.iter().rev().take(5) {
        let content_lower = msg.content.to_lowercase();

        // Positive confirmation patterns
        if content_lower.contains("thanks")
            || content_lower.contains("thank you")
            || content_lower.contains("perfect")
            || content_lower.contains("great")
            || content_lower.contains("looks good")
            || content_lower.contains("lgtm")
            || content_lower.contains("works")
            || content_lower.contains("that's right")
            || content_lower.contains("correct")
            || content_lower.contains("exactly")
            || content_lower.contains("nice")
            || content_lower.contains("awesome")
        {
            return true;
        }
    }
    false
}

/// Check if the session has backtracking (undos, reverts)
fn has_backtracking(messages: &[SessionMessage]) -> bool {
    let mut revert_count = 0;

    for msg in messages {
        // Check assistant content for backtracking indicators
        if msg.role == "assistant" {
            let content_lower = msg.content.to_lowercase();
            if content_lower.contains("let me undo")
                || content_lower.contains("reverting")
                || content_lower.contains("let me fix that")
                || content_lower.contains("that was wrong")
                || content_lower.contains("my mistake")
                || content_lower.contains("sorry, i")
                || content_lower.contains("let me try again")
                || content_lower.contains("actually, let me")
            {
                revert_count += 1;
            }
        }

        // Check tool calls for revert patterns
        for tool_call in &msg.tool_calls {
            if tool_call.name.to_lowercase() == "bash" {
                if let Some(cmd) = tool_call.arguments.get("command").and_then(|v| v.as_str()) {
                    let cmd_lower = cmd.to_lowercase();
                    if cmd_lower.contains("git checkout")
                        || cmd_lower.contains("git reset")
                        || cmd_lower.contains("git revert")
                        || cmd_lower.contains("git restore")
                    {
                        revert_count += 1;
                    }
                }
            }
        }
    }

    // Consider backtracking significant if it happens multiple times
    revert_count >= 2
}

/// Check if the session was abandoned
fn is_abandoned(messages: &[SessionMessage]) -> bool {
    if messages.is_empty() {
        return true;
    }

    // Check if the last message is a user abort or shows abandonment
    let last_msg = messages.last().unwrap();

    // If last message is from user and is short/empty, might be abandoned
    if last_msg.role == "user" {
        let content_lower = last_msg.content.to_lowercase();

        // Explicit abandon patterns
        if content_lower.contains("nevermind")
            || content_lower.contains("forget it")
            || content_lower.contains("stop")
            || content_lower.contains("cancel")
            || content_lower.contains("abort")
            || content_lower.is_empty()
        {
            return true;
        }
    }

    // Check for error-heavy sessions ending without resolution
    let last_few: Vec<_> = messages.iter().rev().take(5).collect();
    let error_count = last_few
        .iter()
        .filter(|m| {
            m.tool_results.iter().any(|r| r.is_error) || m.content.to_lowercase().contains("error")
        })
        .count();

    // If last 5 messages are error-heavy and no resolution, consider abandoned
    if error_count >= 3 && !has_clear_resolution(messages) {
        return true;
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cass::{SessionMetadata, ToolResult};

    fn make_message(role: &str, content: &str) -> SessionMessage {
        SessionMessage {
            index: 0,
            role: role.to_string(),
            content: content.to_string(),
            tool_calls: vec![],
            tool_results: vec![],
        }
    }

    fn make_message_with_tool_result(
        role: &str,
        content: &str,
        result_content: &str,
    ) -> SessionMessage {
        SessionMessage {
            index: 0,
            role: role.to_string(),
            content: content.to_string(),
            tool_calls: vec![],
            tool_results: vec![ToolResult {
                tool_call_id: "test".to_string(),
                content: result_content.to_string(),
                is_error: false,
            }],
        }
    }

    fn make_session(messages: Vec<SessionMessage>) -> Session {
        Session {
            id: "test-session".to_string(),
            path: "/test/path".to_string(),
            messages,
            metadata: SessionMetadata {
                project: None,
                agent: None,
                model: None,
                started_at: None,
                ended_at: None,
                message_count: 0,
                token_count: None,
                tags: vec![],
            },
            content_hash: "abc123".to_string(),
        }
    }

    #[test]
    fn test_has_tests_passed_positive() {
        let messages = vec![make_message_with_tool_result(
            "assistant",
            "Running tests",
            "All tests passed!",
        )];
        assert!(has_tests_passed(&messages));
    }

    #[test]
    fn test_has_tests_passed_negative() {
        let messages = vec![make_message("assistant", "Let me check the code")];
        assert!(!has_tests_passed(&messages));
    }

    #[test]
    fn test_has_clear_resolution() {
        let messages = vec![
            make_message("user", "Fix the bug"),
            make_message(
                "assistant",
                "I've fixed the bug and the changes are complete.",
            ),
        ];
        assert!(has_clear_resolution(&messages));
    }

    #[test]
    fn test_has_clear_resolution_negative() {
        let messages = vec![
            make_message("user", "Fix the bug"),
            make_message("assistant", "Looking at the code..."),
        ];
        assert!(!has_clear_resolution(&messages));
    }

    #[test]
    fn test_has_user_confirmation() {
        let messages = vec![
            make_message("assistant", "Done!"),
            make_message("user", "Thanks, looks good!"),
        ];
        assert!(has_user_confirmation(&messages));
    }

    #[test]
    fn test_is_abandoned_explicit() {
        let messages = vec![
            make_message("assistant", "Let me start..."),
            make_message("user", "nevermind"),
        ];
        assert!(is_abandoned(&messages));
    }

    #[test]
    fn test_is_abandoned_empty() {
        let messages: Vec<SessionMessage> = vec![];
        assert!(is_abandoned(&messages));
    }

    #[test]
    fn test_quality_scorer_high_quality() {
        let messages = vec![
            make_message("user", "Fix the test"),
            make_message_with_tool_result("assistant", "Running tests", "All tests passed!"),
            make_message(
                "assistant",
                "I've completed the fix and all tests are passing.",
            ),
            make_message("user", "Thanks, perfect!"),
        ];
        let session = make_session(messages);

        let scorer = QualityScorer::with_defaults();
        let quality = scorer.score(&session);

        assert!(quality.score >= 0.5);
        assert!(quality.signals.contains(&"tests_passed".to_string()));
        assert!(quality.signals.contains(&"clear_resolution".to_string()));
        assert!(quality.signals.contains(&"user_confirmed".to_string()));
    }

    #[test]
    fn test_quality_scorer_low_quality() {
        let messages = vec![
            make_message("user", "Do something"),
            make_message("assistant", "Looking..."),
            make_message("user", "nevermind"),
        ];
        let session = make_session(messages);

        let scorer = QualityScorer::with_defaults();
        let quality = scorer.score(&session);

        assert!(quality.score < 0.3);
        assert!(quality.signals.contains(&"abandoned".to_string()));
    }

    #[test]
    fn test_quality_config_default() {
        let config = QualityConfig::default();
        assert_eq!(config.min_score, 0.3);
        assert_eq!(config.weight_tests_passed, 0.25);
    }

    #[test]
    fn test_session_quality_passes_threshold() {
        let quality = SessionQuality {
            score: 0.5,
            signals: vec!["tests_passed".to_string()],
            missing: vec![],
            computed_at: Utc::now(),
        };
        let config = QualityConfig::default();
        assert!(quality.passes_threshold(&config));

        let low_quality = SessionQuality {
            score: 0.1,
            signals: vec![],
            missing: vec![MissingSignal::NoTestsPassed],
            computed_at: Utc::now(),
        };
        assert!(!low_quality.passes_threshold(&config));
    }

    #[test]
    fn test_session_quality_summary() {
        let quality = SessionQuality {
            score: 0.65,
            signals: vec!["tests_passed".to_string(), "code_changes".to_string()],
            missing: vec![MissingSignal::NoUserConfirmation],
            computed_at: Utc::now(),
        };
        let summary = quality.summary();
        assert!(summary.contains("good"));
        assert!(summary.contains("65%"));
    }

    #[test]
    fn test_cargo_test_requires_success_indicator() {
        // "cargo test" alone should NOT indicate tests passed
        let messages = vec![make_message_with_tool_result(
            "assistant",
            "Running cargo test",
            "cargo test",
        )];
        assert!(
            !has_tests_passed(&messages),
            "cargo test alone should not indicate success"
        );

        // "cargo test" with "failed" should NOT indicate tests passed
        let messages = vec![make_message_with_tool_result(
            "assistant",
            "Running tests",
            "cargo test\ntest foo ... FAILED",
        )];
        assert!(
            !has_tests_passed(&messages),
            "cargo test with failure should not indicate success"
        );
    }

    #[test]
    fn test_cargo_test_with_success_indicator() {
        // "cargo test" with "ok" should indicate tests passed
        let messages = vec![make_message_with_tool_result(
            "assistant",
            "Running tests",
            "cargo test\ntest result: ok. 5 passed",
        )];
        assert!(
            has_tests_passed(&messages),
            "cargo test with ok should indicate success"
        );

        // Standard cargo test output pattern should work
        let messages = vec![make_message_with_tool_result(
            "assistant",
            "Running tests",
            "running 3 tests\ntest foo ... ok\ntest bar ... ok",
        )];
        assert!(
            has_tests_passed(&messages),
            "running tests with ok should indicate success"
        );
    }
}
