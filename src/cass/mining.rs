//! Pattern mining from CASS sessions
//!
//! Extracts reusable patterns from coding session transcripts.
//! Patterns are the intermediate representation between raw sessions
//! and synthesized skills.

use std::io::Write;

use serde::{Deserialize, Serialize};
use tempfile::Builder;
use tracing::warn;

use crate::error::Result;
use crate::quality::ubs::UbsClient;
use crate::security::{contains_injection_patterns, contains_sensitive_data, SafetyGate};

use super::client::Session;

// =============================================================================
// Pattern Types
// =============================================================================

/// Types of patterns that can be extracted from sessions
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PatternType {
    /// Command sequence pattern (shell commands, CLI invocations)
    CommandPattern {
        commands: Vec<String>,
        frequency: usize,
        contexts: Vec<String>,
    },

    /// Code pattern (snippets, idioms, templates)
    CodePattern {
        language: String,
        code: String,
        purpose: String,
        frequency: usize,
    },

    /// Workflow pattern (multi-step procedures)
    WorkflowPattern {
        steps: Vec<WorkflowStep>,
        triggers: Vec<String>,
        outcomes: Vec<String>,
    },

    /// Decision pattern (conditional logic, branching)
    DecisionPattern {
        condition: String,
        branches: Vec<DecisionBranch>,
        default_action: Option<String>,
    },

    /// Error handling pattern (recovery, diagnostics)
    ErrorPattern {
        error_type: String,
        symptoms: Vec<String>,
        resolution_steps: Vec<String>,
        prevention: Option<String>,
    },

    /// Refactoring pattern (code transformations)
    RefactorPattern {
        before_pattern: String,
        after_pattern: String,
        rationale: String,
        safety_checks: Vec<String>,
    },

    /// Configuration pattern (settings, environment)
    ConfigPattern {
        config_type: String,
        settings: Vec<ConfigSetting>,
        context: String,
    },

    /// Tool usage pattern (specific tool invocations)
    ToolPattern {
        tool_name: String,
        common_args: Vec<String>,
        use_cases: Vec<String>,
    },
}

/// A step in a workflow pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStep {
    pub order: usize,
    pub action: String,
    pub description: String,
    pub optional: bool,
    pub conditions: Vec<String>,
}

/// A branch in a decision pattern
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionBranch {
    pub condition: String,
    pub action: String,
    pub rationale: Option<String>,
}

/// A configuration setting
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigSetting {
    pub key: String,
    pub value: String,
    pub description: Option<String>,
}

// =============================================================================
// Extracted Patterns
// =============================================================================

/// A pattern extracted from one or more sessions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtractedPattern {
    /// Unique identifier for this pattern
    pub id: String,

    /// The type and data of the pattern
    pub pattern_type: PatternType,

    /// Evidence from sessions supporting this pattern
    pub evidence: Vec<EvidenceRef>,

    /// Confidence score (0.0 to 1.0)
    pub confidence: f32,

    /// Number of times this pattern was observed
    pub frequency: usize,

    /// Tags for categorization
    pub tags: Vec<String>,

    /// Human-readable description
    pub description: Option<String>,

    /// Taint label from ACIP analysis (None = safe, Some = requires review)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taint_label: Option<TaintLabel>,
}

/// Taint label indicating content safety status from ACIP analysis
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaintLabel {
    /// Content from untrusted source, may contain sensitive data
    Sensitive,
    /// Content was redacted
    Redacted,
    /// Content requires manual review before use
    RequiresReview,
}

/// Reference to evidence in a session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceRef {
    /// Session ID where pattern was found
    pub session_id: String,

    /// Message indices where pattern appears
    pub message_indices: Vec<usize>,

    /// Relevance score for this evidence
    pub relevance: f32,

    /// Snippet of the evidence (truncated)
    pub snippet: Option<String>,
}

// =============================================================================
// Pattern IR (Intermediate Representation)
// =============================================================================

/// Typed intermediate representation for patterns
///
/// This provides a normalized form for pattern analysis and transformation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "ir_type", rename_all = "snake_case")]
pub enum PatternIR {
    /// Raw text content
    Text { content: String, role: TextRole },

    /// Structured command sequence
    CommandSeq {
        commands: Vec<CommandIR>,
        working_dir: Option<String>,
    },

    /// Code block with metadata
    Code {
        language: String,
        content: String,
        file_path: Option<String>,
        line_range: Option<(usize, usize)>,
    },

    /// Tool invocation record
    ToolUse {
        tool_name: String,
        arguments: serde_json::Value,
        result_summary: Option<String>,
    },

    /// Conditional structure
    Conditional {
        condition: Box<PatternIR>,
        then_branch: Box<PatternIR>,
        else_branch: Option<Box<PatternIR>>,
    },

    /// Sequence of IR nodes
    Sequence { items: Vec<PatternIR> },

    /// Reference to another pattern
    PatternRef { pattern_id: String },
}

/// Role of text in a pattern
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TextRole {
    Instruction,
    Explanation,
    Warning,
    Note,
    Example,
}

/// IR representation of a command
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandIR {
    pub executable: String,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub description: Option<String>,
}

// =============================================================================
// Session Segmentation
// =============================================================================

/// Phase of a coding session
///
/// Sessions typically progress through these phases:
/// 1. Reconnaissance - understanding the problem, reading code
/// 2. Change - making modifications to solve the problem
/// 3. Validation - testing, verifying the changes work
/// 4. WrapUp - committing, cleanup, final summaries
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionPhase {
    /// Initial exploration: reading files, searching, understanding
    Reconnaissance,
    /// Active changes: editing, writing, running commands
    Change,
    /// Verification: running tests, checking builds, reviewing
    Validation,
    /// Final steps: commits, cleanup, summaries
    WrapUp,
}

/// A segment of messages belonging to a single phase
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSegment {
    /// The phase this segment belongs to
    pub phase: SessionPhase,
    /// Starting message index (inclusive)
    pub start_idx: usize,
    /// Ending message index (exclusive)
    pub end_idx: usize,
    /// Confidence that this segmentation is correct (0.0 to 1.0)
    pub confidence: f32,
}

/// A session divided into phase segments
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentedSession {
    /// The original session ID
    pub session_id: String,
    /// Ordered list of segments
    pub segments: Vec<SessionSegment>,
    /// Total message count
    pub total_messages: usize,
}

impl SegmentedSession {
    /// Get all segments of a particular phase
    pub fn segments_for_phase(&self, phase: SessionPhase) -> Vec<&SessionSegment> {
        self.segments.iter().filter(|s| s.phase == phase).collect()
    }

    /// Get the dominant phase (most messages)
    pub fn dominant_phase(&self) -> Option<SessionPhase> {
        let mut counts = std::collections::HashMap::new();
        for seg in &self.segments {
            let len = seg.end_idx - seg.start_idx;
            *counts.entry(seg.phase).or_insert(0usize) += len;
        }
        counts.into_iter().max_by_key(|(_, c)| *c).map(|(p, _)| p)
    }
}

/// Segment a session into phases based on tool usage and message patterns
pub fn segment_session(session: &Session) -> SegmentedSession {
    let mut segments = Vec::new();
    let mut current_phase = SessionPhase::Reconnaissance;
    let mut phase_start = 0;

    for (idx, msg) in session.messages.iter().enumerate() {
        let detected_phase = classify_message_phase(msg);

        // Phase transition detected
        if detected_phase != current_phase && idx > phase_start {
            // Only record segment if it has messages
            segments.push(SessionSegment {
                phase: current_phase,
                start_idx: phase_start,
                end_idx: idx,
                confidence: compute_segment_confidence(
                    &session.messages[phase_start..idx],
                    current_phase,
                ),
            });
            current_phase = detected_phase;
            phase_start = idx;
        }
    }

    // Record final segment
    if phase_start < session.messages.len() {
        segments.push(SessionSegment {
            phase: current_phase,
            start_idx: phase_start,
            end_idx: session.messages.len(),
            confidence: compute_segment_confidence(&session.messages[phase_start..], current_phase),
        });
    }

    // Merge adjacent segments of same phase
    segments = merge_adjacent_segments(segments);

    SegmentedSession {
        session_id: session.id.clone(),
        segments,
        total_messages: session.messages.len(),
    }
}

/// Classify a message into a session phase based on its content and tool usage
fn classify_message_phase(msg: &super::client::SessionMessage) -> SessionPhase {
    // Check tool calls to determine phase
    for tool in &msg.tool_calls {
        match tool.name.as_str() {
            // Reconnaissance tools
            "Read" | "read" | "Glob" | "glob" | "Grep" | "grep" | "ListDirectory" => {
                return SessionPhase::Reconnaissance;
            }
            // Change tools
            "Edit" | "edit" | "Write" | "write" | "NotebookEdit" => {
                return SessionPhase::Change;
            }
            // Bash commands need deeper analysis
            "Bash" | "bash" => {
                if let Some(cmd) = tool.arguments.get("command").and_then(|v| v.as_str()) {
                    return classify_bash_command(cmd);
                }
            }
            _ => {}
        }
    }

    // Check content for phase indicators
    let content_lower = msg.content.to_lowercase();

    // WrapUp indicators
    if content_lower.contains("commit")
        || content_lower.contains("done")
        || content_lower.contains("complete")
        || content_lower.contains("finished")
        || content_lower.contains("summary")
    {
        return SessionPhase::WrapUp;
    }

    // Validation indicators
    if content_lower.contains("test")
        || content_lower.contains("verify")
        || content_lower.contains("check")
        || content_lower.contains("works")
    {
        return SessionPhase::Validation;
    }

    // Default to reconnaissance for user messages, change for assistant
    if msg.role == "user" {
        SessionPhase::Reconnaissance
    } else {
        SessionPhase::Change
    }
}

/// Classify a bash command into a session phase
fn classify_bash_command(cmd: &str) -> SessionPhase {
    let cmd_lower = cmd.to_lowercase();

    // WrapUp commands
    if cmd_lower.starts_with("git commit")
        || cmd_lower.starts_with("git push")
        || cmd_lower.contains("git tag")
    {
        return SessionPhase::WrapUp;
    }

    // Validation commands
    if cmd_lower.contains("test")
        || cmd_lower.contains("cargo check")
        || cmd_lower.contains("cargo build")
        || cmd_lower.contains("npm run")
        || cmd_lower.contains("pytest")
        || cmd_lower.contains("go test")
        || cmd_lower.starts_with("git status")
        || cmd_lower.starts_with("git diff")
    {
        return SessionPhase::Validation;
    }

    // Reconnaissance commands
    if cmd_lower.starts_with("ls")
        || cmd_lower.starts_with("cat")
        || cmd_lower.starts_with("head")
        || cmd_lower.starts_with("tail")
        || cmd_lower.starts_with("find")
        || cmd_lower.starts_with("grep")
        || cmd_lower.starts_with("rg")
        || cmd_lower.starts_with("git log")
        || cmd_lower.starts_with("git show")
    {
        return SessionPhase::Reconnaissance;
    }

    // Default to Change for other bash commands
    SessionPhase::Change
}

/// Compute confidence score for a segment
fn compute_segment_confidence(
    messages: &[super::client::SessionMessage],
    expected_phase: SessionPhase,
) -> f32 {
    if messages.is_empty() {
        return 0.0;
    }

    let mut matching = 0;
    for msg in messages {
        if classify_message_phase(msg) == expected_phase {
            matching += 1;
        }
    }

    matching as f32 / messages.len() as f32
}

/// Merge adjacent segments that have the same phase
fn merge_adjacent_segments(segments: Vec<SessionSegment>) -> Vec<SessionSegment> {
    if segments.is_empty() {
        return segments;
    }

    let mut merged = Vec::new();
    let mut current = segments[0].clone();

    for seg in segments.into_iter().skip(1) {
        if seg.phase == current.phase && seg.start_idx == current.end_idx {
            // Merge: extend current segment
            current.end_idx = seg.end_idx;
            // Recompute confidence as weighted average
            let current_len = current.end_idx - current.start_idx;
            let seg_len = seg.end_idx - seg.start_idx;
            current.confidence = (current.confidence * (current_len - seg_len) as f32
                + seg.confidence * seg_len as f32)
                / current_len as f32;
        } else {
            merged.push(current);
            current = seg;
        }
    }
    merged.push(current);

    merged
}

// =============================================================================
// Legacy Pattern Type (for backward compatibility)
// =============================================================================

/// Simple pattern struct for basic extraction
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Pattern {
    pub id: String,
    pub pattern_type: SimplePatternType,
    pub content: String,
    pub confidence: f32,
}

/// Simple pattern type enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SimplePatternType {
    /// Command recipe (e.g., "cargo build --release")
    CommandRecipe,
    /// Debugging decision tree
    DiagnosticTree,
    /// Invariant to maintain
    Invariant,
    /// Pitfall to avoid
    Pitfall,
    /// Prompt macro
    PromptMacro,
    /// Refactoring playbook
    RefactorPlaybook,
    /// Checklist item
    Checklist,
}

// =============================================================================
// Mining Functions
// =============================================================================

/// Extract patterns from a session transcript
pub fn extract_patterns(_session_path: &str) -> Result<Vec<Pattern>> {
    // TODO: Implement pattern extraction
    Ok(vec![])
}

/// Extract patterns from a parsed session
pub fn extract_from_session(session: &Session) -> Result<Vec<ExtractedPattern>> {
    // ACIP pre-scan: identify messages with injection or sensitive content
    let tainted_indices = scan_for_tainted_messages(session);

    let mut patterns = Vec::new();

    // Segment the session into phases
    let segmented = segment_session(session);

    // Extract command patterns from tool calls
    let command_pattern = extract_command_patterns(session);
    if let Some(p) = command_pattern {
        patterns.push(p);
    }

    // Extract code patterns from messages
    let code_patterns = extract_code_patterns(session);
    patterns.extend(code_patterns);

    // Extract workflow patterns from session phases
    let workflow_pattern = extract_workflow_pattern(session, &segmented);
    if let Some(p) = workflow_pattern {
        patterns.push(p);
    }

    // Extract error handling patterns
    let error_patterns = extract_error_patterns(session);
    patterns.extend(error_patterns);

    // Apply ACIP taint labels based on evidence from tainted messages
    let patterns = apply_taint_labels(patterns, &tainted_indices);

    // Normalize and deduplicate patterns
    let patterns = normalize_patterns(patterns);
    let patterns = deduplicate_patterns(patterns);

    Ok(patterns)
}

/// Message taint status from ACIP analysis
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MessageTaint {
    /// Contains prompt injection patterns - exclude from extraction
    Injection,
    /// Contains sensitive data patterns - flag for review
    Sensitive,
}

/// Check if content contains prompt injection patterns.
fn contains_injection_patterns(content: &str) -> bool {
    let lower = content.to_lowercase();
    let markers = [
        "ignore previous instructions",
        "ignore all previous",
        "disregard prior",
        "new instructions:",
        "system prompt:",
        "you are now",
        "forget everything",
        "override:",
        "admin mode",
        "sudo mode",
        "jailbreak",
        "developer mode",
        "dan mode",
        "</system>",
        "<|im_start|>",
        "<|endoftext|>",
    ];
    markers.iter().any(|m| lower.contains(m))
}

/// Check if content contains sensitive data patterns.
fn contains_sensitive_data(content: &str) -> bool {
    if content.contains("sk-")
        || content.contains("api_key")
        || content.contains("apikey")
        || content.contains("secret_key")
        || content.contains("access_token")
        || content.contains("bearer ")
        || content.contains("AKIA")
        || content.contains("aws_secret")
    {
        return true;
    }
    if content.contains("-----BEGIN") && content.contains("PRIVATE KEY") {
        return true;
    }
    let lower = content.to_lowercase();
    lower.contains("password=") || lower.contains("passwd=") || lower.contains("pwd=")
}

/// Scan session messages for injection and sensitive content patterns.
fn scan_for_tainted_messages(session: &Session) -> std::collections::HashMap<usize, MessageTaint> {
    let mut tainted = std::collections::HashMap::new();

    for msg in &session.messages {
        // Check message content
        if contains_injection_patterns(&msg.content) {
            tainted.insert(msg.index, MessageTaint::Injection);
            continue;
        }
        if contains_sensitive_data(&msg.content) {
            tainted.insert(msg.index, MessageTaint::Sensitive);
            continue;
        }

        // Check tool results (untrusted external data)
        for result in &msg.tool_results {
            if contains_injection_patterns(&result.content) {
                tainted.insert(msg.index, MessageTaint::Injection);
                break;
            }
            if contains_sensitive_data(&result.content) {
                tainted.entry(msg.index).or_insert(MessageTaint::Sensitive);
            }
        }
    }

    tainted
}

/// Apply taint labels to patterns based on their evidence sources.
fn apply_taint_labels(
    patterns: Vec<ExtractedPattern>,
    tainted: &std::collections::HashMap<usize, MessageTaint>,
) -> Vec<ExtractedPattern> {
    if tainted.is_empty() {
        return patterns;
    }

    patterns
        .into_iter()
        .filter_map(|mut pattern| {
            let mut has_injection = false;
            let mut has_sensitive = false;

            for evidence in &pattern.evidence {
                for &idx in &evidence.message_indices {
                    match tainted.get(&idx) {
                        Some(MessageTaint::Injection) => has_injection = true,
                        Some(MessageTaint::Sensitive) => has_sensitive = true,
                        None => {}
                    }
                }
            }

            // Exclude patterns with injection-tainted evidence entirely
            if has_injection {
                warn!(
                    pattern_id = %pattern.id,
                    "Excluding pattern due to injection-tainted evidence"
                );
                return None;
            }

            // Mark patterns with sensitive-tainted evidence
            if has_sensitive && pattern.taint_label.is_none() {
                pattern.taint_label = Some(TaintLabel::RequiresReview);
            }

            Some(pattern)
        })
        .collect()
}

/// Extract command patterns from session tool calls
fn extract_command_patterns(session: &Session) -> Option<ExtractedPattern> {
    let mut commands = Vec::new();
    let mut evidence = Vec::new();

    for msg in &session.messages {
        for tool_call in &msg.tool_calls {
            let tool_name = tool_call.name.to_lowercase();
            let is_command_tool = matches!(
                tool_name.as_str(),
                "bash" | "shell" | "command" | "terminal" | "exec"
            );
            if !is_command_tool {
                continue;
            }

            let cmd = tool_call
                .arguments
                .get("command")
                .and_then(|v| v.as_str())
                .or_else(|| tool_call.arguments.get("cmd").and_then(|v| v.as_str()));

            if let Some(cmd) = cmd {
                commands.push(cmd.to_string());
                evidence.push(EvidenceRef {
                    session_id: session.id.clone(),
                    message_indices: vec![msg.index],
                    relevance: 0.8,
                    snippet: Some(truncate(cmd, 100)),
                });
            }
        }
    }

    if commands.is_empty() {
        return None;
    }

    let frequency = evidence.len();
    Some(ExtractedPattern {
        id: format!("cmd_{}", &session.id[..8.min(session.id.len())]),
        pattern_type: PatternType::CommandPattern {
            commands,
            frequency,
            contexts: vec![session.metadata.project.clone().unwrap_or_default()],
        },
        evidence,
        confidence: 0.6,
        frequency,
        tags: vec!["auto-extracted".to_string(), "commands".to_string()],
        description: Some("Command sequence extracted from session".to_string()),
        taint_label: None,
    })
}

/// Extract code patterns from session messages
fn extract_code_patterns(session: &Session) -> Vec<ExtractedPattern> {
    let mut patterns = Vec::new();
    let ubs_client = match SafetyGate::from_env() {
        Ok(gate) => UbsClient::new(None).with_safety(gate),
        Err(_) => UbsClient::new(None),
    };

    for msg in &session.messages {
        if msg.role == "assistant" {
            // Look for code blocks in content
            let code_blocks = extract_code_blocks(&msg.content);
            for (lang, code) in code_blocks {
                if code.len() > 50 {
                    if !code_passes_ubs(&ubs_client, &lang, &code) {
                        continue;
                    }
                    // Only significant code blocks
                    patterns.push(ExtractedPattern {
                        id: format!(
                            "code_{}_{}_{}",
                            &session.id[..8.min(session.id.len())],
                            msg.index,
                            patterns.len()
                        ),
                        pattern_type: PatternType::CodePattern {
                            language: lang.clone(),
                            code: code.clone(),
                            purpose: "Extracted code block".to_string(),
                            frequency: 1,
                        },
                        evidence: vec![EvidenceRef {
                            session_id: session.id.clone(),
                            message_indices: vec![msg.index],
                            relevance: 0.7,
                            snippet: Some(truncate(&code, 100)),
                        }],
                        confidence: 0.5,
                        frequency: 1,
                        tags: vec!["auto-extracted".to_string(), lang],
                        description: None,
                        taint_label: None,
                    });
                }
            }
        }
    }

    patterns
}

/// Extract workflow pattern from segmented session
fn extract_workflow_pattern(
    session: &Session,
    segmented: &SegmentedSession,
) -> Option<ExtractedPattern> {
    // Need at least 2 distinct phases to form a workflow
    let unique_phases: std::collections::HashSet<_> =
        segmented.segments.iter().map(|s| s.phase).collect();
    if unique_phases.len() < 2 {
        return None;
    }

    let mut steps = Vec::new();
    let mut triggers = Vec::new();
    let mut outcomes = Vec::new();
    let mut evidence = Vec::new();

    for (order, segment) in segmented.segments.iter().enumerate() {
        // Collect representative actions from each phase
        let phase_actions = collect_phase_actions(session, segment);
        if phase_actions.is_empty() {
            continue;
        }

        let step_description = summarize_phase_actions(&phase_actions, segment.phase);
        steps.push(WorkflowStep {
            order: order + 1,
            action: format!("{:?}", segment.phase),
            description: step_description,
            optional: segment.confidence < 0.5,
            conditions: vec![],
        });

        // Track evidence
        evidence.push(EvidenceRef {
            session_id: session.id.clone(),
            message_indices: (segment.start_idx..segment.end_idx).collect(),
            relevance: segment.confidence,
            snippet: Some(truncate(&phase_actions.join("; "), 100)),
        });
    }

    if steps.len() < 2 {
        return None;
    }

    // Extract triggers from first phase (usually user request)
    if let Some(first_msg) = session.messages.first() {
        if first_msg.role == "user" && !first_msg.content.is_empty() {
            triggers.push(truncate(&first_msg.content, 200));
        }
    }

    // Extract outcomes from last phase (usually completion message)
    if let Some(last_segment) = segmented.segments.last() {
        if last_segment.phase == SessionPhase::WrapUp {
            outcomes.push("Task completed successfully".to_string());
        }
    }

    // Compute overall confidence based on segment confidences
    let avg_confidence = segmented
        .segments
        .iter()
        .map(|s| s.confidence)
        .sum::<f32>()
        / segmented.segments.len() as f32;

    Some(ExtractedPattern {
        id: format!("workflow_{}", &session.id[..8.min(session.id.len())]),
        pattern_type: PatternType::WorkflowPattern {
            steps,
            triggers,
            outcomes,
        },
        evidence,
        confidence: avg_confidence * 0.8, // Discount for auto-extraction
        frequency: 1,
        tags: vec!["auto-extracted".to_string(), "workflow".to_string()],
        description: Some("Workflow pattern extracted from session phases".to_string()),
        taint_label: None,
    })
}

/// Collect representative actions from a session segment
fn collect_phase_actions(session: &Session, segment: &SessionSegment) -> Vec<String> {
    let mut actions = Vec::new();

    for idx in segment.start_idx..segment.end_idx {
        if let Some(msg) = session.messages.get(idx) {
            for tool in &msg.tool_calls {
                match tool.name.as_str() {
                    "Bash" | "bash" => {
                        if let Some(cmd) = tool.arguments.get("command").and_then(|v| v.as_str()) {
                            actions.push(format!("Run: {}", truncate(cmd, 50)));
                        }
                    }
                    "Edit" | "edit" => {
                        if let Some(path) =
                            tool.arguments.get("file_path").and_then(|v| v.as_str())
                        {
                            actions.push(format!("Edit: {}", path_basename(path)));
                        }
                    }
                    "Read" | "read" => {
                        if let Some(path) =
                            tool.arguments.get("file_path").and_then(|v| v.as_str())
                        {
                            actions.push(format!("Read: {}", path_basename(path)));
                        }
                    }
                    "Write" | "write" => {
                        if let Some(path) =
                            tool.arguments.get("file_path").and_then(|v| v.as_str())
                        {
                            actions.push(format!("Write: {}", path_basename(path)));
                        }
                    }
                    "Glob" | "glob" => {
                        if let Some(pattern) =
                            tool.arguments.get("pattern").and_then(|v| v.as_str())
                        {
                            actions.push(format!("Search: {}", pattern));
                        }
                    }
                    "Grep" | "grep" => {
                        if let Some(pattern) =
                            tool.arguments.get("pattern").and_then(|v| v.as_str())
                        {
                            actions.push(format!("Grep: {}", truncate(pattern, 30)));
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    actions
}

/// Summarize phase actions into a description
fn summarize_phase_actions(actions: &[String], phase: SessionPhase) -> String {
    if actions.is_empty() {
        return format!("{:?} phase", phase);
    }

    let prefix = match phase {
        SessionPhase::Reconnaissance => "Explored",
        SessionPhase::Change => "Modified",
        SessionPhase::Validation => "Verified",
        SessionPhase::WrapUp => "Finalized",
    };

    if actions.len() == 1 {
        format!("{}: {}", prefix, actions[0])
    } else {
        format!("{} {} items: {}", prefix, actions.len(), actions[0])
    }
}

/// Extract basename from a path
fn path_basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// Extract error handling patterns from session
fn extract_error_patterns(session: &Session) -> Vec<ExtractedPattern> {
    let mut patterns = Vec::new();
    let mut current_error: Option<ErrorContext> = None;

    for (idx, msg) in session.messages.iter().enumerate() {
        // Look for error indicators in tool results
        for result in &msg.tool_results {
            let result_lower = result.content.to_lowercase();
            if result_lower.contains("error")
                || result_lower.contains("failed")
                || result_lower.contains("panic")
                || result_lower.contains("exception")
            {
                // Found an error - start tracking
                current_error = Some(ErrorContext {
                    error_idx: idx,
                    error_text: truncate(&result.content, 200),
                    symptoms: extract_error_symptoms(&result.content),
                    resolution_steps: Vec::new(),
                });
            }
        }

        // If we're tracking an error, look for resolution
        if let Some(ref mut err_ctx) = current_error {
            // Check if this message contains resolution steps
            for tool in &msg.tool_calls {
                match tool.name.as_str() {
                    "Edit" | "edit" | "Write" | "write" => {
                        if let Some(path) =
                            tool.arguments.get("file_path").and_then(|v| v.as_str())
                        {
                            err_ctx
                                .resolution_steps
                                .push(format!("Fix in {}", path_basename(path)));
                        }
                    }
                    "Bash" | "bash" => {
                        if let Some(cmd) = tool.arguments.get("command").and_then(|v| v.as_str()) {
                            if !classify_bash_command(cmd).eq(&SessionPhase::Reconnaissance) {
                                err_ctx
                                    .resolution_steps
                                    .push(format!("Run: {}", truncate(cmd, 50)));
                            }
                        }
                    }
                    _ => {}
                }
            }

            // Check for success indicators (error resolved)
            let content_lower = msg.content.to_lowercase();
            if content_lower.contains("fixed")
                || content_lower.contains("resolved")
                || content_lower.contains("works")
                || content_lower.contains("passing")
            {
                // Error was resolved - emit pattern
                if !err_ctx.resolution_steps.is_empty() {
                    let error_type = classify_error_type(&err_ctx.error_text);
                    patterns.push(ExtractedPattern {
                        id: format!(
                            "error_{}_{}_{}",
                            &session.id[..8.min(session.id.len())],
                            err_ctx.error_idx,
                            patterns.len()
                        ),
                        pattern_type: PatternType::ErrorPattern {
                            error_type,
                            symptoms: err_ctx.symptoms.clone(),
                            resolution_steps: err_ctx.resolution_steps.clone(),
                            prevention: None,
                        },
                        evidence: vec![EvidenceRef {
                            session_id: session.id.clone(),
                            message_indices: (err_ctx.error_idx..=idx).collect(),
                            relevance: 0.7,
                            snippet: Some(truncate(&err_ctx.error_text, 100)),
                        }],
                        confidence: compute_error_pattern_confidence(err_ctx),
                        frequency: 1,
                        tags: vec!["auto-extracted".to_string(), "error-handling".to_string()],
                        description: Some("Error handling pattern from session".to_string()),
                        taint_label: None,
                    });
                }
                current_error = None;
            }
        }
    }

    patterns
}

/// Context for tracking an error through resolution
struct ErrorContext {
    error_idx: usize,
    error_text: String,
    symptoms: Vec<String>,
    resolution_steps: Vec<String>,
}

/// Extract symptoms from an error message
fn extract_error_symptoms(error_text: &str) -> Vec<String> {
    let mut symptoms = Vec::new();

    // Look for common error patterns
    if error_text.contains("not found") || error_text.contains("No such file") {
        symptoms.push("Missing file or module".to_string());
    }
    if error_text.contains("undefined") || error_text.contains("undeclared") {
        symptoms.push("Undefined identifier".to_string());
    }
    if error_text.contains("type") && (error_text.contains("mismatch") || error_text.contains("expected")) {
        symptoms.push("Type mismatch".to_string());
    }
    if error_text.contains("borrow") || error_text.contains("lifetime") {
        symptoms.push("Borrow/lifetime issue".to_string());
    }
    if error_text.contains("syntax") || error_text.contains("parse") {
        symptoms.push("Syntax error".to_string());
    }
    if error_text.contains("permission") || error_text.contains("denied") {
        symptoms.push("Permission denied".to_string());
    }
    if error_text.contains("timeout") || error_text.contains("timed out") {
        symptoms.push("Operation timed out".to_string());
    }

    // If no specific symptoms detected, add generic one
    if symptoms.is_empty() {
        symptoms.push("Build/runtime error".to_string());
    }

    symptoms
}

/// Classify error into a type
fn classify_error_type(error_text: &str) -> String {
    let text_lower = error_text.to_lowercase();

    if text_lower.contains("compile") || text_lower.contains("build") {
        "compilation".to_string()
    } else if text_lower.contains("test") {
        "test_failure".to_string()
    } else if text_lower.contains("import") || text_lower.contains("module") {
        "module_resolution".to_string()
    } else if text_lower.contains("type") {
        "type_error".to_string()
    } else if text_lower.contains("permission") || text_lower.contains("access") {
        "permission".to_string()
    } else if text_lower.contains("network") || text_lower.contains("connection") {
        "network".to_string()
    } else {
        "runtime".to_string()
    }
}

/// Compute confidence for an error pattern
fn compute_error_pattern_confidence(ctx: &ErrorContext) -> f32 {
    let mut confidence: f32 = 0.4; // Base confidence

    // More resolution steps = higher confidence
    if ctx.resolution_steps.len() >= 2 {
        confidence += 0.2;
    }
    if ctx.resolution_steps.len() >= 4 {
        confidence += 0.1;
    }

    // More symptoms identified = higher confidence
    if ctx.symptoms.len() >= 2 {
        confidence += 0.1;
    }

    // Cap at 0.85 for auto-extracted patterns
    confidence.min(0.85_f32)
}

/// Normalize patterns for consistency
fn normalize_patterns(patterns: Vec<ExtractedPattern>) -> Vec<ExtractedPattern> {
    patterns
        .into_iter()
        .map(|mut p| {
            // Normalize tags to lowercase
            p.tags = p.tags.iter().map(|t| t.to_lowercase()).collect();

            // Ensure confidence is in valid range
            p.confidence = p.confidence.clamp(0.0, 1.0);

            // Ensure description is present
            if p.description.is_none() {
                p.description = Some(generate_pattern_description(&p.pattern_type));
            }

            p
        })
        .collect()
}

/// Generate a description for a pattern type
fn generate_pattern_description(pattern_type: &PatternType) -> String {
    match pattern_type {
        PatternType::CommandPattern { commands, .. } => {
            format!("Command sequence with {} commands", commands.len())
        }
        PatternType::CodePattern { language, .. } => {
            format!("Code pattern in {}", language)
        }
        PatternType::WorkflowPattern { steps, .. } => {
            format!("Workflow with {} steps", steps.len())
        }
        PatternType::ErrorPattern { error_type, .. } => {
            format!("Error handling for {}", error_type)
        }
        PatternType::DecisionPattern { .. } => "Decision tree pattern".to_string(),
        PatternType::RefactorPattern { .. } => "Refactoring pattern".to_string(),
        PatternType::ConfigPattern { config_type, .. } => {
            format!("Configuration for {}", config_type)
        }
        PatternType::ToolPattern { tool_name, .. } => {
            format!("Tool usage pattern for {}", tool_name)
        }
    }
}

/// Deduplicate patterns based on similarity
fn deduplicate_patterns(patterns: Vec<ExtractedPattern>) -> Vec<ExtractedPattern> {
    if patterns.len() <= 1 {
        return patterns;
    }

    let mut unique: Vec<ExtractedPattern> = Vec::new();

    for pattern in patterns {
        // Check if a similar pattern already exists
        let is_duplicate = unique.iter().any(|existing| {
            patterns_are_similar(existing, &pattern)
        });

        if !is_duplicate {
            unique.push(pattern);
        } else {
            // Merge with existing similar pattern - increase frequency
            if let Some(existing) = unique.iter_mut().find(|e| patterns_are_similar(e, &pattern)) {
                existing.frequency += pattern.frequency;
                existing.evidence.extend(pattern.evidence);
                // Boost confidence when pattern is seen multiple times
                existing.confidence = (existing.confidence + 0.1).min(0.95);
            }
        }
    }

    unique
}

/// Check if two patterns are similar enough to be deduplicated
fn patterns_are_similar(a: &ExtractedPattern, b: &ExtractedPattern) -> bool {
    // Must be same pattern type category
    match (&a.pattern_type, &b.pattern_type) {
        (
            PatternType::CommandPattern { commands: ca, .. },
            PatternType::CommandPattern { commands: cb, .. },
        ) => {
            // Similar if >70% command overlap
            let overlap = ca.iter().filter(|c| cb.contains(c)).count();
            let total = ca.len().max(cb.len());
            total > 0 && overlap as f32 / total as f32 > 0.7
        }
        (
            PatternType::CodePattern {
                language: la,
                code: ca,
                ..
            },
            PatternType::CodePattern {
                language: lb,
                code: cb,
                ..
            },
        ) => {
            // Similar if same language and code starts similarly
            la == lb && ca.len() > 20 && cb.len() > 20 && ca[..20.min(ca.len())] == cb[..20.min(cb.len())]
        }
        (
            PatternType::ErrorPattern {
                error_type: ta,
                symptoms: sa,
                ..
            },
            PatternType::ErrorPattern {
                error_type: tb,
                symptoms: sb,
                ..
            },
        ) => {
            // Similar if same error type and overlapping symptoms
            ta == tb && sa.iter().any(|s| sb.contains(s))
        }
        (
            PatternType::WorkflowPattern { steps: sa, .. },
            PatternType::WorkflowPattern { steps: sb, .. },
        ) => {
            // Similar if same number of steps with matching phases
            sa.len() == sb.len()
                && sa
                    .iter()
                    .zip(sb.iter())
                    .all(|(a, b)| a.action == b.action)
        }
        _ => false,
    }
}

fn code_passes_ubs(client: &UbsClient, language: &str, code: &str) -> bool {
    let ext = extension_for_language(language);
    if ext == "txt" {
        return true;
    }

    let suffix = format!(".{ext}");
    let mut temp: tempfile::NamedTempFile =
        match Builder::new().prefix("ms-ubs-").suffix(&suffix).tempfile() {
            Ok(file) => file,
            Err(err) => {
                warn!("ubs temp file error: {err}");
                return true;
            }
        };

    if let Err(err) = temp.write_all(code.as_bytes()) {
        warn!("ubs temp write error: {err}");
        return true;
    }
    if let Err(err) = temp.flush() {
        warn!("ubs temp flush error: {err}");
        return true;
    }

    let path = temp.path().to_path_buf();
    match client.check_files(&[path]) {
        Ok(result) => result.is_clean(),
        Err(err) => {
            warn!("ubs check failed: {err}");
            true
        }
    }
}

fn extension_for_language(language: &str) -> &'static str {
    match language.trim().to_lowercase().as_str() {
        "rust" | "rs" => "rs",
        "go" => "go",
        "python" | "py" => "py",
        "javascript" | "js" => "js",
        "typescript" | "ts" => "ts",
        "bash" | "sh" | "shell" => "sh",
        "json" => "json",
        "toml" => "toml",
        "yaml" | "yml" => "yaml",
        _ => "txt",
    }
}

/// Extract code blocks from markdown content
fn extract_code_blocks(content: &str) -> Vec<(String, String)> {
    let mut blocks = Vec::new();
    let mut in_block = false;
    let mut current_lang = String::new();
    let mut current_code = String::new();

    for line in content.lines() {
        if line.starts_with("```") {
            if in_block {
                // End of block
                blocks.push((current_lang.clone(), current_code.trim().to_string()));
                current_lang.clear();
                current_code.clear();
                in_block = false;
            } else {
                // Start of block
                current_lang = line.trim_start_matches('`').trim().to_string();
                in_block = true;
            }
        } else if in_block {
            current_code.push_str(line);
            current_code.push('\n');
        }
    }

    blocks
}

/// Convert extracted pattern to IR
pub fn pattern_to_ir(pattern: &ExtractedPattern) -> PatternIR {
    match &pattern.pattern_type {
        PatternType::CommandPattern { commands, .. } => PatternIR::CommandSeq {
            commands: commands
                .iter()
                .map(|cmd| {
                    let parts: Vec<&str> = cmd.split_whitespace().collect();
                    CommandIR {
                        executable: parts.first().unwrap_or(&"").to_string(),
                        args: parts.iter().skip(1).map(|s| s.to_string()).collect(),
                        env: vec![],
                        description: None,
                    }
                })
                .collect(),
            working_dir: None,
        },

        PatternType::CodePattern { language, code, .. } => PatternIR::Code {
            language: language.clone(),
            content: code.clone(),
            file_path: None,
            line_range: None,
        },

        PatternType::WorkflowPattern { steps, .. } => PatternIR::Sequence {
            items: steps
                .iter()
                .map(|step| PatternIR::Text {
                    content: format!("{}. {}", step.order, step.action),
                    role: TextRole::Instruction,
                })
                .collect(),
        },

        _ => PatternIR::Text {
            content: pattern.description.clone().unwrap_or_default(),
            role: TextRole::Explanation,
        },
    }
}

/// Truncate string to max length
fn truncate(s: &str, max_len: usize) -> String {
    let mut out = String::new();
    for (idx, ch) in s.chars().enumerate() {
        if idx >= max_len {
            break;
        }
        out.push(ch);
    }
    if s.chars().count() > max_len {
        if max_len >= 3 {
            let trimmed = out
                .chars()
                .take(max_len.saturating_sub(3))
                .collect::<String>();
            format!("{trimmed}...")
        } else {
            "...".to_string()
        }
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_code_blocks() {
        let content = r#"Here is some code:

```rust
fn main() {
    println!("Hello");
}
```

And more text.
"#;
        let blocks = extract_code_blocks(content);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].0, "rust");
        assert!(blocks[0].1.contains("fn main()"));
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("hello", 10), "hello");
        assert_eq!(truncate("hello world", 8), "hello...");
    }

    #[test]
    fn test_pattern_serialization() {
        let pattern = ExtractedPattern {
            id: "test-1".to_string(),
            pattern_type: PatternType::CommandPattern {
                commands: vec!["cargo build".to_string()],
                frequency: 1,
                contexts: vec!["rust".to_string()],
            },
            evidence: vec![],
            confidence: 0.8,
            frequency: 1,
            tags: vec!["test".to_string()],
            description: None,
            taint_label: None,
        };

        let json = serde_json::to_string(&pattern).unwrap();
        assert!(json.contains("command_pattern"));
        assert!(json.contains("cargo build"));
    }

    #[test]
    fn test_pattern_ir_serialization() {
        let ir = PatternIR::Code {
            language: "rust".to_string(),
            content: "fn foo() {}".to_string(),
            file_path: None,
            line_range: None,
        };

        let json = serde_json::to_string(&ir).unwrap();
        assert!(json.contains("code"));
        assert!(json.contains("rust"));
    }

    #[test]
    fn test_classify_bash_command_validation() {
        assert_eq!(
            classify_bash_command("cargo test"),
            SessionPhase::Validation
        );
        assert_eq!(
            classify_bash_command("npm run test"),
            SessionPhase::Validation
        );
        assert_eq!(classify_bash_command("pytest"), SessionPhase::Validation);
        assert_eq!(
            classify_bash_command("cargo check"),
            SessionPhase::Validation
        );
        assert_eq!(
            classify_bash_command("git status"),
            SessionPhase::Validation
        );
    }

    #[test]
    fn test_classify_bash_command_wrapup() {
        assert_eq!(
            classify_bash_command("git commit -m 'fix'"),
            SessionPhase::WrapUp
        );
        assert_eq!(
            classify_bash_command("git push origin main"),
            SessionPhase::WrapUp
        );
    }

    #[test]
    fn test_classify_bash_command_recon() {
        assert_eq!(
            classify_bash_command("ls -la"),
            SessionPhase::Reconnaissance
        );
        assert_eq!(
            classify_bash_command("cat file.txt"),
            SessionPhase::Reconnaissance
        );
        assert_eq!(
            classify_bash_command("git log --oneline"),
            SessionPhase::Reconnaissance
        );
        assert_eq!(
            classify_bash_command("rg pattern"),
            SessionPhase::Reconnaissance
        );
    }

    #[test]
    fn test_classify_bash_command_change() {
        assert_eq!(classify_bash_command("mkdir new_dir"), SessionPhase::Change);
        assert_eq!(classify_bash_command("rm old_file"), SessionPhase::Change);
        assert_eq!(
            classify_bash_command("cargo build --release"),
            SessionPhase::Validation
        ); // build is validation
    }

    #[test]
    fn test_session_phase_serialization() {
        let phase = SessionPhase::Reconnaissance;
        let json = serde_json::to_string(&phase).unwrap();
        assert_eq!(json, "\"reconnaissance\"");

        let phase = SessionPhase::WrapUp;
        let json = serde_json::to_string(&phase).unwrap();
        assert_eq!(json, "\"wrap_up\"");
    }

    #[test]
    fn test_merge_adjacent_segments() {
        let segments = vec![
            SessionSegment {
                phase: SessionPhase::Reconnaissance,
                start_idx: 0,
                end_idx: 2,
                confidence: 0.8,
            },
            SessionSegment {
                phase: SessionPhase::Reconnaissance,
                start_idx: 2,
                end_idx: 4,
                confidence: 0.9,
            },
            SessionSegment {
                phase: SessionPhase::Change,
                start_idx: 4,
                end_idx: 6,
                confidence: 0.7,
            },
        ];

        let merged = merge_adjacent_segments(segments);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].phase, SessionPhase::Reconnaissance);
        assert_eq!(merged[0].start_idx, 0);
        assert_eq!(merged[0].end_idx, 4);
        assert_eq!(merged[1].phase, SessionPhase::Change);
    }

    #[test]
    fn test_segmented_session_dominant_phase() {
        let session = SegmentedSession {
            session_id: "test".to_string(),
            segments: vec![
                SessionSegment {
                    phase: SessionPhase::Reconnaissance,
                    start_idx: 0,
                    end_idx: 2,
                    confidence: 0.8,
                },
                SessionSegment {
                    phase: SessionPhase::Change,
                    start_idx: 2,
                    end_idx: 10,
                    confidence: 0.9,
                },
            ],
            total_messages: 10,
        };

        assert_eq!(session.dominant_phase(), Some(SessionPhase::Change));
    }

    #[test]
    fn test_extract_error_symptoms() {
        let symptoms = extract_error_symptoms("error: file not found");
        assert!(symptoms.contains(&"Missing file or module".to_string()));

        let symptoms = extract_error_symptoms("type mismatch: expected u32");
        assert!(symptoms.contains(&"Type mismatch".to_string()));

        let symptoms = extract_error_symptoms("syntax error on line 5");
        assert!(symptoms.contains(&"Syntax error".to_string()));

        let symptoms = extract_error_symptoms("cannot borrow as mutable");
        assert!(symptoms.contains(&"Borrow/lifetime issue".to_string()));

        // Default case
        let symptoms = extract_error_symptoms("unknown problem");
        assert!(symptoms.contains(&"Build/runtime error".to_string()));
    }

    #[test]
    fn test_classify_error_type() {
        assert_eq!(classify_error_type("compile error"), "compilation");
        assert_eq!(classify_error_type("build failed"), "compilation");
        assert_eq!(classify_error_type("test failure"), "test_failure");
        assert_eq!(classify_error_type("module not found"), "module_resolution");
        assert_eq!(classify_error_type("type error"), "type_error");
        assert_eq!(classify_error_type("permission denied"), "permission");
        assert_eq!(classify_error_type("network error"), "network");
        assert_eq!(classify_error_type("something else"), "runtime");
    }

    #[test]
    fn test_compute_error_pattern_confidence() {
        // Minimal context
        let ctx = ErrorContext {
            error_idx: 0,
            error_text: "error".to_string(),
            symptoms: vec!["s1".to_string()],
            resolution_steps: vec!["r1".to_string()],
        };
        assert!((compute_error_pattern_confidence(&ctx) - 0.4).abs() < 0.01);

        // More resolution steps
        let ctx = ErrorContext {
            error_idx: 0,
            error_text: "error".to_string(),
            symptoms: vec!["s1".to_string()],
            resolution_steps: vec!["r1".to_string(), "r2".to_string()],
        };
        assert!((compute_error_pattern_confidence(&ctx) - 0.6).abs() < 0.01);

        // Many resolution steps and symptoms
        let ctx = ErrorContext {
            error_idx: 0,
            error_text: "error".to_string(),
            symptoms: vec!["s1".to_string(), "s2".to_string()],
            resolution_steps: vec![
                "r1".to_string(),
                "r2".to_string(),
                "r3".to_string(),
                "r4".to_string(),
            ],
        };
        assert!((compute_error_pattern_confidence(&ctx) - 0.8).abs() < 0.01);
    }

    #[test]
    fn test_normalize_patterns() {
        let patterns = vec![ExtractedPattern {
            id: "test".to_string(),
            pattern_type: PatternType::CommandPattern {
                commands: vec!["cmd".to_string()],
                frequency: 1,
                contexts: vec![],
            },
            evidence: vec![],
            confidence: 1.5, // Invalid - should be clamped
            frequency: 1,
            tags: vec!["TEST".to_string(), "Mixed".to_string()],
            description: None, // Should be auto-generated
            taint_label: None,
        }];

        let normalized = normalize_patterns(patterns);
        assert_eq!(normalized.len(), 1);
        assert_eq!(normalized[0].confidence, 1.0); // Clamped
        assert!(normalized[0].tags.iter().all(|t| t == &t.to_lowercase())); // Lowercase
        assert!(normalized[0].description.is_some()); // Generated
    }

    #[test]
    fn test_path_basename() {
        assert_eq!(path_basename("/foo/bar/baz.rs"), "baz.rs");
        assert_eq!(path_basename("simple.txt"), "simple.txt");
        assert_eq!(path_basename("/"), "");
        assert_eq!(path_basename("a/b/c"), "c");
    }

    #[test]
    fn test_patterns_are_similar_commands() {
        let p1 = ExtractedPattern {
            id: "1".to_string(),
            pattern_type: PatternType::CommandPattern {
                commands: vec!["cargo build".to_string(), "cargo test".to_string()],
                frequency: 1,
                contexts: vec![],
            },
            evidence: vec![],
            confidence: 0.8,
            frequency: 1,
            tags: vec![],
            description: None,
        };

        let p2 = ExtractedPattern {
            id: "2".to_string(),
            pattern_type: PatternType::CommandPattern {
                commands: vec!["cargo build".to_string(), "cargo test".to_string()],
                frequency: 1,
                contexts: vec![],
            },
            evidence: vec![],
            confidence: 0.8,
            frequency: 1,
            tags: vec![],
            description: None,
        };

        assert!(patterns_are_similar(&p1, &p2));

        // Different commands - not similar
        let p3 = ExtractedPattern {
            id: "3".to_string(),
            pattern_type: PatternType::CommandPattern {
                commands: vec!["npm install".to_string()],
                frequency: 1,
                contexts: vec![],
            },
            evidence: vec![],
            confidence: 0.8,
            frequency: 1,
            tags: vec![],
            description: None,
        };

        assert!(!patterns_are_similar(&p1, &p3));
    }

    #[test]
    fn test_patterns_are_similar_errors() {
        let p1 = ExtractedPattern {
            id: "1".to_string(),
            pattern_type: PatternType::ErrorPattern {
                error_type: "compilation".to_string(),
                symptoms: vec!["Type mismatch".to_string()],
                resolution_steps: vec!["Fix type".to_string()],
                prevention: None,
            },
            evidence: vec![],
            confidence: 0.8,
            frequency: 1,
            tags: vec![],
            description: None,
        };

        let p2 = ExtractedPattern {
            id: "2".to_string(),
            pattern_type: PatternType::ErrorPattern {
                error_type: "compilation".to_string(),
                symptoms: vec!["Type mismatch".to_string(), "Other".to_string()],
                resolution_steps: vec!["Other fix".to_string()],
                prevention: None,
            },
            evidence: vec![],
            confidence: 0.8,
            frequency: 1,
            tags: vec![],
            description: None,
        };

        assert!(patterns_are_similar(&p1, &p2));

        // Different error type - not similar
        let p3 = ExtractedPattern {
            id: "3".to_string(),
            pattern_type: PatternType::ErrorPattern {
                error_type: "runtime".to_string(),
                symptoms: vec!["Type mismatch".to_string()],
                resolution_steps: vec!["Fix".to_string()],
                prevention: None,
            },
            evidence: vec![],
            confidence: 0.8,
            frequency: 1,
            tags: vec![],
            description: None,
        };

        assert!(!patterns_are_similar(&p1, &p3));
    }

    #[test]
    fn test_deduplicate_patterns() {
        let patterns = vec![
            ExtractedPattern {
                id: "1".to_string(),
                pattern_type: PatternType::CommandPattern {
                    commands: vec!["cargo build".to_string()],
                    frequency: 1,
                    contexts: vec![],
                },
                evidence: vec![],
                confidence: 0.6,
                frequency: 1,
                tags: vec![],
                description: None,
            },
            ExtractedPattern {
                id: "2".to_string(),
                pattern_type: PatternType::CommandPattern {
                    commands: vec!["cargo build".to_string()],
                    frequency: 1,
                    contexts: vec![],
                },
                evidence: vec![],
                confidence: 0.6,
                frequency: 1,
                tags: vec![],
                description: None,
            },
        ];

        let deduped = deduplicate_patterns(patterns);
        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].frequency, 2); // Merged
        assert!(deduped[0].confidence > 0.6); // Boosted
    }

    #[test]
    fn test_generate_pattern_description() {
        assert!(generate_pattern_description(&PatternType::CommandPattern {
            commands: vec!["a".to_string(), "b".to_string()],
            frequency: 2,
            contexts: vec![],
        })
        .contains("2 commands"));

        assert!(generate_pattern_description(&PatternType::CodePattern {
            language: "rust".to_string(),
            code: "".to_string(),
            purpose: "".to_string(),
            frequency: 1,
        })
        .contains("rust"));

        assert!(generate_pattern_description(&PatternType::ErrorPattern {
            error_type: "compilation".to_string(),
            symptoms: vec![],
            resolution_steps: vec![],
            prevention: None,
        })
        .contains("compilation"));
    }

    #[test]
    fn test_acip_injection_patterns_excluded() {
        // Pattern with evidence from injection-tainted message should be excluded
        let mut tainted = std::collections::HashMap::new();
        tainted.insert(0, MessageTaint::Injection);

        let patterns = vec![ExtractedPattern {
            id: "test-injection".to_string(),
            pattern_type: PatternType::CommandPattern {
                commands: vec!["ignore previous instructions".to_string()],
                frequency: 1,
                contexts: vec![],
            },
            evidence: vec![EvidenceRef {
                session_id: "s1".to_string(),
                message_indices: vec![0], // Points to injection-tainted message
                relevance: 0.8,
                snippet: None,
            }],
            confidence: 0.8,
            frequency: 1,
            tags: vec![],
            description: None,
            taint_label: None,
        }];

        let result = apply_taint_labels(patterns, &tainted);
        assert!(result.is_empty(), "Pattern with injection evidence should be excluded");
    }

    #[test]
    fn test_acip_sensitive_patterns_labeled() {
        // Pattern with evidence from sensitive-tainted message should get RequiresReview label
        let mut tainted = std::collections::HashMap::new();
        tainted.insert(1, MessageTaint::Sensitive);

        let patterns = vec![ExtractedPattern {
            id: "test-sensitive".to_string(),
            pattern_type: PatternType::CommandPattern {
                commands: vec!["export API_KEY=xxx".to_string()],
                frequency: 1,
                contexts: vec![],
            },
            evidence: vec![EvidenceRef {
                session_id: "s1".to_string(),
                message_indices: vec![1], // Points to sensitive-tainted message
                relevance: 0.8,
                snippet: None,
            }],
            confidence: 0.8,
            frequency: 1,
            tags: vec![],
            description: None,
            taint_label: None,
        }];

        let result = apply_taint_labels(patterns, &tainted);
        assert_eq!(result.len(), 1, "Pattern with sensitive evidence should be kept");
        assert_eq!(
            result[0].taint_label,
            Some(TaintLabel::RequiresReview),
            "Sensitive pattern should have RequiresReview label"
        );
    }

    #[test]
    fn test_acip_clean_patterns_unchanged() {
        // Pattern without tainted evidence should remain unchanged
        let tainted = std::collections::HashMap::new(); // No tainted messages

        let patterns = vec![ExtractedPattern {
            id: "test-clean".to_string(),
            pattern_type: PatternType::CommandPattern {
                commands: vec!["cargo build".to_string()],
                frequency: 1,
                contexts: vec![],
            },
            evidence: vec![EvidenceRef {
                session_id: "s1".to_string(),
                message_indices: vec![0],
                relevance: 0.8,
                snippet: None,
            }],
            confidence: 0.8,
            frequency: 1,
            tags: vec![],
            description: None,
            taint_label: None,
        }];

        let result = apply_taint_labels(patterns, &tainted);
        assert_eq!(result.len(), 1, "Clean pattern should be kept");
        assert_eq!(result[0].taint_label, None, "Clean pattern should have no taint label");
    }
}
