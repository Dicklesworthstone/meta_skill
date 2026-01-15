//! Anti-pattern signal detection
//!
//! Scans sessions for signals that indicate anti-patterns:
//! - Rollback operations (git reset, git revert, file restore)
//! - Explicit corrections ("No, do X instead")
//! - Failure signals (test failures, build errors, exceptions)
//! - User markers (explicit anti-pattern annotations)

use crate::cass::client::Session;
use crate::error::Result;

use super::types::{
    AntiPatternContext, AntiPatternEvidence, AntiPatternSource, Condition, FailureIncident,
    FailureSignalType, IncidentContext, RollbackType, SessionId,
};

// =============================================================================
// Detection Trait
// =============================================================================

/// Trait for detecting anti-pattern signals in sessions
pub trait AntiPatternDetector {
    /// Scan session for anti-pattern signals
    fn detect_signals(&self, session: &Session) -> Vec<AntiPatternSignal>;

    /// Check for explicit anti-pattern markers
    fn check_markers(&self, session: &Session) -> Option<MarkedAntiPattern>;

    /// Detect rollback/undo sequences
    fn detect_rollbacks(&self, session: &Session) -> Vec<RollbackSequence>;

    /// Find explicit corrections ("No, do X instead")
    fn find_corrections(&self, session: &Session) -> Vec<Correction>;
}

/// A detected anti-pattern signal
#[derive(Debug, Clone)]
pub struct AntiPatternSignal {
    /// Type of signal
    pub signal_type: FailureSignalType,
    /// Message index where signal was detected
    pub message_idx: usize,
    /// Context around the signal
    pub context: ContextWindow,
    /// Action that preceded the failure (if identifiable)
    pub preceding_action: Option<ActionSummary>,
}

/// Context window around a detected signal
#[derive(Debug, Clone)]
pub struct ContextWindow {
    /// Messages before the signal
    pub before: Vec<MessageSummary>,
    /// Messages after the signal
    pub after: Vec<MessageSummary>,
}

/// Summary of a message for context
#[derive(Debug, Clone)]
pub struct MessageSummary {
    pub idx: usize,
    pub role: String,
    pub content_preview: String,
    pub has_tool_calls: bool,
}

/// Summary of an action
#[derive(Debug, Clone)]
pub struct ActionSummary {
    pub description: String,
    pub tool_name: Option<String>,
    pub message_idx: usize,
}

/// An explicitly marked anti-pattern session
#[derive(Debug, Clone)]
pub struct MarkedAntiPattern {
    pub marker: String,
    pub annotation: Option<String>,
    pub message_idx: usize,
}

/// A detected rollback sequence
#[derive(Debug, Clone)]
pub struct RollbackSequence {
    pub rollback_type: RollbackType,
    pub command: String,
    pub message_idx: usize,
    /// What was undone
    pub undone_action: Option<String>,
}

/// A detected correction
#[derive(Debug, Clone)]
pub struct Correction {
    /// The original (wrong) action
    pub original: String,
    /// The correction
    pub correction: String,
    /// Message index where correction was made
    pub message_idx: usize,
    /// Confidence in this being a correction (0.0 to 1.0)
    pub confidence: f32,
}

// =============================================================================
// Default Detector Implementation
// =============================================================================

/// Default implementation of anti-pattern detection
pub struct DefaultDetector {
    /// Minimum confidence threshold for signals
    pub min_confidence: f32,
    /// Context window size (messages before/after)
    pub context_window: usize,
}

impl Default for DefaultDetector {
    fn default() -> Self {
        Self {
            min_confidence: 0.5,
            context_window: 3,
        }
    }
}

impl DefaultDetector {
    /// Create a new detector with custom settings
    pub fn new(min_confidence: f32, context_window: usize) -> Self {
        Self {
            min_confidence,
            context_window,
        }
    }

    /// Get context window around a message index
    fn get_context_window(&self, session: &Session, idx: usize) -> ContextWindow {
        let mut before = Vec::new();
        let mut after = Vec::new();

        // Get messages before
        let start = idx.saturating_sub(self.context_window);
        for i in start..idx {
            if let Some(msg) = session.messages.get(i) {
                before.push(MessageSummary {
                    idx: i,
                    role: msg.role.clone(),
                    content_preview: truncate_content(&msg.content, 100),
                    has_tool_calls: !msg.tool_calls.is_empty(),
                });
            }
        }

        // Get messages after
        let end = (idx + 1 + self.context_window).min(session.messages.len());
        for i in (idx + 1)..end {
            if let Some(msg) = session.messages.get(i) {
                after.push(MessageSummary {
                    idx: i,
                    role: msg.role.clone(),
                    content_preview: truncate_content(&msg.content, 100),
                    has_tool_calls: !msg.tool_calls.is_empty(),
                });
            }
        }

        ContextWindow { before, after }
    }

    /// Extract preceding action from context
    fn find_preceding_action(&self, session: &Session, signal_idx: usize) -> Option<ActionSummary> {
        // Look backwards for the most recent action
        for i in (0..signal_idx).rev() {
            if let Some(msg) = session.messages.get(i) {
                // Check for tool calls
                for tool in &msg.tool_calls {
                    // Skip read-only tools
                    if is_read_only_tool(&tool.name) {
                        continue;
                    }
                    return Some(ActionSummary {
                        description: format!("{} call", tool.name),
                        tool_name: Some(tool.name.clone()),
                        message_idx: i,
                    });
                }
            }
        }
        None
    }
}

impl AntiPatternDetector for DefaultDetector {
    fn detect_signals(&self, session: &Session) -> Vec<AntiPatternSignal> {
        let mut signals = Vec::new();

        for (idx, msg) in session.messages.iter().enumerate() {
            // Check message content for failure signals
            if let Some(signal_type) = detect_failure_in_content(&msg.content) {
                signals.push(AntiPatternSignal {
                    signal_type,
                    message_idx: idx,
                    context: self.get_context_window(session, idx),
                    preceding_action: self.find_preceding_action(session, idx),
                });
            }

            // Check tool results for failures
            for tool_result in &msg.tool_results {
                if let Some(signal_type) =
                    detect_failure_in_tool_result(&tool_result.content, tool_result.is_error)
                {
                    signals.push(AntiPatternSignal {
                        signal_type,
                        message_idx: idx,
                        context: self.get_context_window(session, idx),
                        preceding_action: self.find_preceding_action(session, idx),
                    });
                }
            }
        }

        signals
    }

    fn check_markers(&self, session: &Session) -> Option<MarkedAntiPattern> {
        // Patterns that indicate explicit anti-pattern marking
        let markers = [
            "anti-pattern",
            "antipattern",
            "wrong approach",
            "don't do this",
            "bad example",
            "what not to do",
        ];

        for (idx, msg) in session.messages.iter().enumerate() {
            let content_lower = msg.content.to_lowercase();
            for marker in &markers {
                if content_lower.contains(marker) {
                    return Some(MarkedAntiPattern {
                        marker: (*marker).to_string(),
                        annotation: extract_annotation(&msg.content, marker),
                        message_idx: idx,
                    });
                }
            }
        }

        None
    }

    fn detect_rollbacks(&self, session: &Session) -> Vec<RollbackSequence> {
        let mut rollbacks = Vec::new();

        for (idx, msg) in session.messages.iter().enumerate() {
            // Check Bash tool calls for rollback commands
            for tool in &msg.tool_calls {
                if tool.name.to_lowercase() == "bash" {
                    if let Some(cmd) = tool.arguments.get("command").and_then(|v| v.as_str()) {
                        if let Some((rollback_type, undone)) = detect_rollback_command(cmd) {
                            rollbacks.push(RollbackSequence {
                                rollback_type,
                                command: cmd.to_string(),
                                message_idx: idx,
                                undone_action: undone,
                            });
                        }
                    }
                }
            }
        }

        rollbacks
    }

    fn find_corrections(&self, session: &Session) -> Vec<Correction> {
        let mut corrections = Vec::new();

        // Patterns that indicate corrections
        let correction_patterns = [
            ("no,", "no, "),
            ("actually,", "actually, "),
            ("instead,", "instead, "),
            ("correction:", "correction:"),
            ("wait,", "wait, "),
            ("sorry,", "sorry, "),
            ("that's wrong", "that's wrong"),
            ("that was wrong", "that was wrong"),
            ("my mistake", "my mistake"),
            ("let me fix", "let me fix"),
        ];

        for (idx, msg) in session.messages.iter().enumerate() {
            // Only check user messages for corrections (they correct the assistant)
            if msg.role != "user" {
                continue;
            }

            let content_lower = msg.content.to_lowercase();
            for (pattern, _display) in &correction_patterns {
                if content_lower.contains(pattern) {
                    // Try to extract what's being corrected and the correction
                    if let Some((original, correction)) =
                        extract_correction(&msg.content, pattern, session, idx)
                    {
                        corrections.push(Correction {
                            original,
                            correction,
                            message_idx: idx,
                            confidence: compute_correction_confidence(&content_lower),
                        });
                    }
                }
            }
        }

        corrections
    }
}

// =============================================================================
// Helper Functions
// =============================================================================

/// Truncate content to a maximum length (character-aware for UTF-8 safety)
fn truncate_content(content: &str, max_chars: usize) -> String {
    let char_count = content.chars().count();
    if char_count <= max_chars {
        content.to_string()
    } else {
        let truncated: String = content.chars().take(max_chars).collect();
        format!("{}...", truncated)
    }
}

/// Check if a tool is read-only
fn is_read_only_tool(tool_name: &str) -> bool {
    matches!(
        tool_name.to_lowercase().as_str(),
        "read" | "glob" | "grep" | "listdirectory" | "search" | "webfetch"
    )
}

/// Detect failure signals in message content
fn detect_failure_in_content(content: &str) -> Option<FailureSignalType> {
    let content_lower = content.to_lowercase();

    // Test failures
    if content_lower.contains("test failed")
        || content_lower.contains("tests failed")
        || content_lower.contains("assertion failed")
        || content_lower.contains("failed test")
    {
        return Some(FailureSignalType::TestFailure);
    }

    // Build errors
    if content_lower.contains("build failed")
        || content_lower.contains("compilation error")
        || content_lower.contains("compile error")
        || content_lower.contains("error[e")
        // Rust error format
        || content_lower.contains("cannot find")
    {
        return Some(FailureSignalType::BuildError);
    }

    // Runtime exceptions
    if content_lower.contains("exception")
        || content_lower.contains("panic")
        || content_lower.contains("stack trace")
        || content_lower.contains("traceback")
        || content_lower.contains("error:")
            && (content_lower.contains("runtime") || content_lower.contains("at line"))
    {
        return Some(FailureSignalType::RuntimeException);
    }

    // User rejection
    if content_lower.contains("that's not what i")
        || content_lower.contains("that's wrong")
        || content_lower.contains("don't do that")
        || content_lower.contains("stop")
            && (content_lower.contains("wrong") || content_lower.contains("not right"))
    {
        return Some(FailureSignalType::UserRejection);
    }

    // Explicit no
    if (content_lower.starts_with("no,") || content_lower.starts_with("no "))
        && content_lower.len() < 200
    {
        return Some(FailureSignalType::ExplicitNo);
    }

    // Frustration
    if content_lower.contains("frustrated")
        || content_lower.contains("annoying")
        || content_lower.contains("why isn't this working")
        || content_lower.contains("still broken")
    {
        return Some(FailureSignalType::Frustration);
    }

    // Timeout
    if content_lower.contains("timed out")
        || content_lower.contains("timeout")
        || content_lower.contains("hung")
        || content_lower.contains("not responding")
    {
        return Some(FailureSignalType::Timeout);
    }

    // Permission errors
    if content_lower.contains("permission denied")
        || content_lower.contains("access denied")
        || content_lower.contains("unauthorized")
        || content_lower.contains("forbidden")
    {
        return Some(FailureSignalType::PermissionError);
    }

    None
}

/// Detect failure in tool result
fn detect_failure_in_tool_result(result: &str, is_error: bool) -> Option<FailureSignalType> {
    let result_lower = result.to_lowercase();

    // If marked as error, always treat as failure
    if is_error {
        if result_lower.contains("test") {
            return Some(FailureSignalType::TestFailure);
        } else if result_lower.contains("permission") || result_lower.contains("denied") {
            return Some(FailureSignalType::PermissionError);
        } else if result_lower.contains("timeout") || result_lower.contains("timed out") {
            return Some(FailureSignalType::Timeout);
        } else if result_lower.contains("build") || result_lower.contains("compil") {
            return Some(FailureSignalType::BuildError);
        }
        return Some(FailureSignalType::RuntimeException);
    }

    // Check for implicit failure indicators even if not marked as error
    if result_lower.contains("error") || result_lower.contains("failed") {
        if result_lower.contains("test") {
            return Some(FailureSignalType::TestFailure);
        } else if result_lower.contains("build") || result_lower.contains("compil") {
            return Some(FailureSignalType::BuildError);
        }
        return Some(FailureSignalType::RuntimeException);
    }

    None
}

/// Detect rollback commands
fn detect_rollback_command(cmd: &str) -> Option<(RollbackType, Option<String>)> {
    let cmd_lower = cmd.to_lowercase();

    // Git reset
    if cmd_lower.contains("git reset") {
        let undone = if cmd_lower.contains("--hard") {
            Some("discarded all local changes".to_string())
        } else if cmd_lower.contains("head~") || cmd_lower.contains("head^") {
            Some("undid recent commit(s)".to_string())
        } else {
            None
        };
        return Some((RollbackType::GitReset, undone));
    }

    // Git revert
    if cmd_lower.contains("git revert") {
        return Some((RollbackType::GitRevert, Some("reverted commit".to_string())));
    }

    // Git checkout (file restore)
    if cmd_lower.contains("git checkout") && cmd_lower.contains("--") {
        return Some((
            RollbackType::FileRestore,
            Some("restored file from previous version".to_string()),
        ));
    }

    // Git restore
    if cmd_lower.contains("git restore") {
        return Some((
            RollbackType::FileRestore,
            Some("restored file(s)".to_string()),
        ));
    }

    // Git stash pop after stash (could indicate rollback of changes)
    if cmd_lower.contains("git stash") && !cmd_lower.contains("pop") {
        return Some((
            RollbackType::ManualUndo,
            Some("stashed changes".to_string()),
        ));
    }

    None
}

/// Extract annotation from marked anti-pattern
fn extract_annotation(content: &str, marker: &str) -> Option<String> {
    // Try to find text after the marker
    if let Some(pos) = content.to_lowercase().find(marker) {
        let after = &content[pos + marker.len()..];
        let annotation = after.trim().lines().next()?.trim().to_string();
        if !annotation.is_empty() && annotation.len() < 200 {
            return Some(annotation);
        }
    }
    None
}

/// Extract correction details
fn extract_correction(
    content: &str,
    _pattern: &str,
    session: &Session,
    correction_idx: usize,
) -> Option<(String, String)> {
    // Get the previous assistant message as the "original"
    let original = if correction_idx > 0 {
        session
            .messages
            .get(correction_idx - 1)
            .filter(|m| m.role == "assistant")
            .map(|m| truncate_content(&m.content, 200))
    } else {
        None
    };

    // The correction is the current message content
    let correction = truncate_content(content, 200);

    original.map(|orig| (orig, correction))
}

/// Compute confidence for a correction
fn compute_correction_confidence(content_lower: &str) -> f32 {
    let mut confidence: f32 = 0.5;

    // Strong correction indicators
    if content_lower.contains("that's wrong") || content_lower.contains("that was wrong") {
        confidence += 0.3;
    }

    // Medium indicators
    if content_lower.starts_with("no,") || content_lower.starts_with("no ") {
        confidence += 0.2;
    }

    // Weak indicators
    if content_lower.contains("actually") || content_lower.contains("instead") {
        confidence += 0.1;
    }

    confidence.min(1.0)
}

// =============================================================================
// Context Extraction
// =============================================================================

/// Extract anti-pattern context from a signal
pub fn extract_context(
    signal: &AntiPatternSignal,
    session: &Session,
) -> Result<AntiPatternContext> {
    let failed_action = signal
        .preceding_action
        .as_ref()
        .map(|a| a.description.clone())
        .unwrap_or_else(|| "unknown action".to_string());

    let failure_reason = session
        .messages
        .get(signal.message_idx)
        .map(|m| truncate_content(&m.content, 150));

    let conditions = extract_conditions_from_context(&signal.context);

    let correction = find_correction_in_context(&signal.context, session, signal.message_idx);

    Ok(AntiPatternContext {
        failed_action,
        failure_reason,
        conditions,
        correction,
        session_id: SessionId::new(&session.id),
        message_idx: signal.message_idx,
    })
}

/// Extract conditions from context window
fn extract_conditions_from_context(context: &ContextWindow) -> Vec<Condition> {
    let mut conditions = Vec::new();

    // Look for conditional language in context
    for msg in &context.before {
        if msg.content_preview.to_lowercase().contains("if ")
            || msg.content_preview.to_lowercase().contains("when ")
        {
            conditions.push(Condition::suggested(truncate_content(
                &msg.content_preview,
                100,
            )));
        }
    }

    conditions
}

/// Find correction in context after the failure
fn find_correction_in_context(
    context: &ContextWindow,
    session: &Session,
    _signal_idx: usize,
) -> Option<String> {
    // Look for corrections in messages after the failure
    for msg_summary in &context.after {
        if let Some(msg) = session.messages.get(msg_summary.idx) {
            let content_lower = msg.content.to_lowercase();
            if content_lower.contains("instead")
                || content_lower.contains("fix")
                || content_lower.contains("correct")
            {
                return Some(truncate_content(&msg.content, 150));
            }
        }
    }
    None
}

/// Convert detection results into evidence
pub fn signals_to_evidence(
    signals: Vec<AntiPatternSignal>,
    session: &Session,
) -> Vec<AntiPatternEvidence> {
    signals
        .into_iter()
        .filter_map(|signal| {
            let context = extract_context(&signal, session).ok()?;

            let incident = FailureIncident {
                description: signal.signal_type.description().to_string(),
                failed_action: context.failed_action.clone(),
                message_indices: vec![signal.message_idx],
                context: IncidentContext {
                    before: signal
                        .context
                        .before
                        .iter()
                        .map(|m| m.content_preview.clone())
                        .collect(),
                    after: signal
                        .context
                        .after
                        .iter()
                        .map(|m| m.content_preview.clone())
                        .collect(),
                    location: None,
                    tool: signal
                        .preceding_action
                        .as_ref()
                        .and_then(|a| a.tool_name.clone()),
                },
                correction: context.correction,
            };

            Some(AntiPatternEvidence::new(
                AntiPatternSource::FailureSignal {
                    signal_type: signal.signal_type,
                },
                SessionId::new(&session.id),
                incident,
            ))
        })
        .collect()
}

/// Convert rollbacks into evidence
pub fn rollbacks_to_evidence(
    rollbacks: Vec<RollbackSequence>,
    session: &Session,
) -> Vec<AntiPatternEvidence> {
    rollbacks
        .into_iter()
        .map(|rollback| {
            let incident = FailureIncident {
                description: rollback.rollback_type.description().to_string(),
                failed_action: rollback.command.clone(),
                message_indices: vec![rollback.message_idx],
                context: IncidentContext {
                    before: vec![],
                    after: vec![],
                    location: None,
                    tool: Some("Bash".to_string()),
                },
                correction: rollback.undone_action.clone(),
            };

            AntiPatternEvidence::new(
                AntiPatternSource::RollbackDetected {
                    rollback_type: rollback.rollback_type,
                },
                SessionId::new(&session.id),
                incident,
            )
        })
        .collect()
}

/// Convert corrections into evidence
pub fn corrections_to_evidence(
    corrections: Vec<Correction>,
    session: &Session,
) -> Vec<AntiPatternEvidence> {
    corrections
        .into_iter()
        .filter(|c| c.confidence >= 0.5)
        .map(|correction| {
            let incident = FailureIncident {
                description: "explicit correction".to_string(),
                failed_action: correction.original.clone(),
                message_indices: vec![correction.message_idx],
                context: IncidentContext {
                    before: vec![],
                    after: vec![correction.correction.clone()],
                    location: None,
                    tool: None,
                },
                correction: Some(correction.correction.clone()),
            };

            AntiPatternEvidence::new(
                AntiPatternSource::WrongFix {
                    original: correction.original,
                    correction: correction.correction,
                },
                SessionId::new(&session.id),
                incident,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_test_failure() {
        let result = detect_failure_in_content("The test failed with assertion error");
        assert_eq!(result, Some(FailureSignalType::TestFailure));
    }

    #[test]
    fn test_detect_build_error() {
        let result = detect_failure_in_content("Build failed: compilation error in main.rs");
        assert_eq!(result, Some(FailureSignalType::BuildError));
    }

    #[test]
    fn test_detect_user_rejection() {
        let result = detect_failure_in_content("That's wrong, I asked for something else");
        assert_eq!(result, Some(FailureSignalType::UserRejection));
    }

    #[test]
    fn test_detect_rollback_git_reset() {
        let result = detect_rollback_command("git reset --hard HEAD~1");
        assert!(result.is_some());
        let (rollback_type, _) = result.unwrap();
        assert_eq!(rollback_type, RollbackType::GitReset);
    }

    #[test]
    fn test_detect_rollback_git_revert() {
        let result = detect_rollback_command("git revert abc123");
        assert!(result.is_some());
        let (rollback_type, _) = result.unwrap();
        assert_eq!(rollback_type, RollbackType::GitRevert);
    }

    #[test]
    fn test_correction_confidence() {
        assert!(compute_correction_confidence("that's wrong") > 0.7);
        assert!(compute_correction_confidence("no, do this instead") > 0.6);
        assert!(compute_correction_confidence("maybe try something else") < 0.6);
    }
}
