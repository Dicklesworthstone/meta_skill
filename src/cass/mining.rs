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
use crate::security::{SafetyGate, contains_injection_patterns, contains_sensitive_data};

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
        condition: Box<Self>,
        then_branch: Box<Self>,
        else_branch: Option<Box<Self>>,
    },

    /// Sequence of IR nodes
    Sequence { items: Vec<Self> },

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
/// 3. Validation - running tests, verifying the changes work
/// 4. `WrapUp` - committing, cleanup, final summaries
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
    #[must_use]
    pub fn segments_for_phase(&self, phase: SessionPhase) -> Vec<&SessionSegment> {
        self.segments.iter().filter(|s| s.phase == phase).collect()
    }

    /// Get the dominant phase (most messages)
    #[must_use]
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

    // Initialize phase from first message instead of defaulting to Reconnaissance
    let mut current_phase = session
        .messages
        .first()
        .map_or(SessionPhase::Reconnaissance, classify_message_phase);
    let mut phase_start = 0;

    for (idx, msg) in session.messages.iter().enumerate() {
        let detected_phase = classify_message_phase(msg);

        // Phase transition detected (skip first message since we used it to initialize)
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

    // Merge adjacent segments that have the same phase
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
        match tool.name.to_lowercase().as_str() {
            // Reconnaissance tools
            "read" | "glob" | "grep" | "listdirectory" => {
                return SessionPhase::Reconnaissance;
            }
            // Change tools
            "edit" | "write" | "notebookedit" => {
                return SessionPhase::Change;
            }
            // Bash commands need deeper analysis
            "bash" | "shell" | "command" | "terminal" | "exec" => {
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
    // Use word boundaries or start/end anchors to avoid false positives like "latest" matching "test"
    let is_validation = cmd_lower.contains("cargo check")
        || cmd_lower.contains("cargo build")
        || cmd_lower.contains("npm run")
        || cmd_lower.contains("pytest")
        || cmd_lower.contains("go test")
        || cmd_lower.starts_with("git status")
        || cmd_lower.starts_with("git diff")
        // Check for "test" as a standalone word or command
        || is_command_match(&cmd_lower, "test");

    if is_validation {
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

/// Check if a command string matches a target command name (exact or word boundary)
fn is_command_match(cmd: &str, target: &str) -> bool {
    if cmd == target {
        return true;
    }
    if cmd.starts_with(&format!("{target} ")) {
        return true;
    }
    // Check if it appears as a distinct word using tokenization
    tokenize_command(cmd).iter().any(|part| part == target)
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
        // Check if segments are adjacent: same phase and new segment starts where current ends
        #[allow(clippy::suspicious_operation_groupings)]
        if seg.phase == current.phase && seg.start_idx == current.end_idx {
            // Merge: extend current segment
            current.end_idx = seg.end_idx;
            // Recompute confidence as weighted average
            let current_len = current.end_idx - current.start_idx;
            let seg_len = seg.end_idx - seg.start_idx;
            // Use saturating_sub to prevent underflow if segments have unexpected values
            let old_len = current_len.saturating_sub(seg_len);
            current.confidence = if current_len > 0 {
                current
                    .confidence
                    .mul_add(old_len as f32, seg.confidence * seg_len as f32)
                    / current_len as f32
            } else {
                0.0
            };
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

/// Extract patterns from a session transcript file
///
/// Parses the session file (JSON or JSONL format) and extracts patterns.
pub fn extract_patterns(session_path: &str) -> Result<Vec<Pattern>> {
    use crate::error::MsError;
    use std::path::Path;

    let path = Path::new(session_path);
    if !path.exists() {
        return Err(MsError::SkillNotFound(format!(
            "Session file not found: {session_path}"
        )));
    }

    let content = std::fs::read_to_string(path)
        .map_err(|e| MsError::MiningFailed(format!("Failed to read session: {e}")))?;

    // Try to parse as a full Session object first
    let session: super::client::Session = serde_json::from_str(&content).map_err(|e| {
        MsError::MiningFailed(format!(
            "Failed to parse session file as JSON: {e}. File: {session_path}"
        ))
    })?;

    // Extract patterns using the full extraction pipeline
    let extracted = extract_from_session(&session)?;

    // Convert ExtractedPattern to simple Pattern format
    let patterns = extracted
        .into_iter()
        .map(|ep| Pattern {
            id: ep.id,
            pattern_type: pattern_type_to_simple(&ep.pattern_type),
            content: pattern_content(&ep.pattern_type),
            confidence: ep.confidence,
        })
        .collect();

    Ok(patterns)
}

/// Convert a full `PatternType` to the simple `SimplePatternType`
const fn pattern_type_to_simple(pt: &PatternType) -> SimplePatternType {
    match pt {
        PatternType::CommandPattern { .. } => SimplePatternType::CommandRecipe,
        PatternType::CodePattern { .. } => SimplePatternType::PromptMacro, // Code is like a prompt/template
        PatternType::WorkflowPattern { .. } => SimplePatternType::Checklist,
        PatternType::DecisionPattern { .. } => SimplePatternType::DiagnosticTree,
        PatternType::ErrorPattern { .. } => SimplePatternType::Pitfall, // Errors are pitfalls to avoid
        PatternType::RefactorPattern { .. } => SimplePatternType::RefactorPlaybook,
        PatternType::ConfigPattern { .. } => SimplePatternType::Invariant, // Config as invariant to maintain
        PatternType::ToolPattern { .. } => SimplePatternType::CommandRecipe, // Tool patterns are commands
    }
}

/// Extract the main content string from a `PatternType`
fn pattern_content(pt: &PatternType) -> String {
    match pt {
        PatternType::CommandPattern { commands, .. } => commands.join(" && "),
        PatternType::CodePattern { code, .. } => code.clone(),
        PatternType::WorkflowPattern { steps, .. } => steps
            .iter()
            .map(|s| s.action.clone())
            .collect::<Vec<_>>()
            .join(" -> "),
        PatternType::DecisionPattern {
            condition,
            branches,
            ..
        } => {
            let branch_strs: Vec<_> = branches.iter().map(|b| b.action.clone()).collect();
            format!("{}: {}", condition, branch_strs.join(" | "))
        }
        PatternType::ErrorPattern {
            error_type,
            resolution_steps,
            ..
        } => {
            format!("{}: {}", error_type, resolution_steps.join(", "))
        }
        PatternType::RefactorPattern {
            before_pattern,
            after_pattern,
            ..
        } => {
            format!("{before_pattern} -> {after_pattern}")
        }
        PatternType::ConfigPattern {
            config_type,
            settings,
            ..
        } => {
            let keys: Vec<_> = settings.iter().map(|s| s.key.clone()).collect();
            format!("{}: {}", config_type, keys.join(", "))
        }
        PatternType::ToolPattern {
            tool_name,
            common_args,
            ..
        } => {
            format!("{} {}", tool_name, common_args.join(" "))
        }
    }
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
        id: format!("cmd_{}", safe_prefix(&session.id, 8)),
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
                            safe_prefix(&session.id, 8),
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
    let avg_confidence = segmented.segments.iter().map(|s| s.confidence).sum::<f32>()
        / segmented.segments.len() as f32;

    Some(ExtractedPattern {
        id: format!("workflow_{}", safe_prefix(&session.id, 8)),
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
                match tool.name.to_lowercase().as_str() {
                    "bash" | "shell" | "command" | "terminal" | "exec" => {
                        if let Some(cmd) = tool.arguments.get("command").and_then(|v| v.as_str()) {
                            actions.push(format!("Run: {}", truncate(cmd, 50)));
                        }
                    }
                    "edit" | "write" | "notebookedit" => {
                        if let Some(path) = tool.arguments.get("file_path").and_then(|v| v.as_str())
                        {
                            actions.push(format!("Edit: {}", path_basename(path)));
                        }
                    }
                    "read" => {
                        if let Some(path) = tool.arguments.get("file_path").and_then(|v| v.as_str())
                        {
                            actions.push(format!("Read: {}", path_basename(path)));
                        }
                    }
                    "glob" | "list_directory" => {
                        if let Some(pattern) =
                            tool.arguments.get("pattern").and_then(|v| v.as_str())
                        {
                            actions.push(format!("Search: {pattern}"));
                        }
                    }
                    "grep" | "search_file_content" => {
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
        return format!("{phase:?} phase");
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
    // Split by both forward and backslash to handle cross-platform paths
    path.rsplit(['/', '\\']).next().unwrap_or(path)
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
                match tool.name.to_lowercase().as_str() {
                    "edit" | "write" | "notebookedit" => {
                        if let Some(path) = tool.arguments.get("file_path").and_then(|v| v.as_str())
                        {
                            err_ctx
                                .resolution_steps
                                .push(format!("Fix in {}", path_basename(path)));
                        }
                    }
                    "bash" | "shell" | "command" | "terminal" | "exec" => {
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
                            safe_prefix(&session.id, 8),
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
    if error_text.contains("type")
        && (error_text.contains("mismatch") || error_text.contains("expected"))
    {
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
            format!("Code pattern in {language}")
        }
        PatternType::WorkflowPattern { steps, .. } => {
            format!("Workflow with {} steps", steps.len())
        }
        PatternType::ErrorPattern { error_type, .. } => {
            format!("Error handling for {error_type}")
        }
        PatternType::DecisionPattern { .. } => "Decision tree pattern".to_string(),
        PatternType::RefactorPattern { .. } => "Refactoring pattern".to_string(),
        PatternType::ConfigPattern { config_type, .. } => {
            format!("Configuration for {config_type}")
        }
        PatternType::ToolPattern { tool_name, .. } => {
            format!("Tool usage pattern for {tool_name}")
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
        let is_duplicate = unique
            .iter()
            .any(|existing| patterns_are_similar(existing, &pattern));

        if is_duplicate {
            // Merge with existing similar pattern - increase frequency
            if let Some(existing) = unique
                .iter_mut()
                .find(|e| patterns_are_similar(e, &pattern))
            {
                existing.frequency += pattern.frequency;
                existing.evidence.extend(pattern.evidence);
                // Boost confidence when pattern is seen multiple times
                existing.confidence = (existing.confidence + 0.1).min(0.95);
            }
        } else {
            unique.push(pattern);
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
            // Must be same language
            if la != lb {
                return false;
            }

            if la == "python" || la == "py" || la == "yaml" || la == "yml" {
                // Indentation-sensitive comparison
                let norm_a = normalize_indentation_sensitive(ca);
                let norm_b = normalize_indentation_sensitive(cb);
                norm_a == norm_b
            } else {
                // Whitespace-insensitive comparison (normalize to single space)
                let norm_a: String = ca.split_whitespace().collect::<Vec<_>>().join(" ");
                let norm_b: String = cb.split_whitespace().collect::<Vec<_>>().join(" ");
                norm_a == norm_b
            }
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
            sa.len() == sb.len() && sa.iter().zip(sb.iter()).all(|(a, b)| a.action == b.action)
        }
        _ => false,
    }
}

fn normalize_indentation_sensitive(code: &str) -> String {
    code.lines()
        .map(str::trim_end) // Keep leading whitespace, trim trailing
        .filter(|line| !line.is_empty()) // Remove empty lines
        .collect::<Vec<_>>()
        .join("\n")
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
    let mut current_fence = String::new();

    for line in content.lines() {
        let trimmed_start = line.trim_start();

        if !in_block {
            if let Some((fence, lang)) = parse_opening_fence(trimmed_start) {
                current_fence = fence;
                current_lang = lang;
                current_code.clear();
                in_block = true;
            }
        } else {
            // Check for closing fence
            if trimmed_start.starts_with(&current_fence) {
                let fence_char = current_fence.chars().next().unwrap_or('`');
                let closing_len = trimmed_start
                    .chars()
                    .take_while(|&c| c == fence_char)
                    .count();

                // Closing fence must be at least as long as opening fence
                if closing_len >= current_fence.len() {
                    // Rest of the line must be empty (or whitespace)
                    let rest = &trimmed_start[closing_len..];
                    if rest.trim().is_empty() {
                        blocks.push((current_lang.clone(), current_code.trim().to_string()));
                        in_block = false;
                        continue;
                    }
                }
            }
            current_code.push_str(line);
            current_code.push('\n');
        }
    }

    blocks
}

fn parse_opening_fence(s: &str) -> Option<(String, String)> {
    if s.starts_with("```") {
        let len = s.chars().take_while(|&c| c == '`').count();
        let fence = s[..len].to_string();
        let lang = s[len..].trim().to_string();
        Some((fence, lang))
    } else if s.starts_with("~~~") {
        let len = s.chars().take_while(|&c| c == '~').count();
        let fence = s[..len].to_string();
        let lang = s[len..].trim().to_string();
        Some((fence, lang))
    } else {
        None
    }
}

/// Tokenize a shell command string, respecting quotes and escapes.
fn tokenize_command(cmd: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_quote = None; // None, Some('\''), Some('"')
    let mut escape = false;

    for c in cmd.chars() {
        if escape {
            current.push(c);
            escape = false;
        } else if c == '\\' {
            escape = true;
        } else if let Some(q) = in_quote {
            if c == q && !escape {
                in_quote = None;
            } else {
                current.push(c);
            }
        } else if c == '"' || c == '\'' {
            in_quote = Some(c);
        } else if c.is_whitespace() {
            if !current.is_empty() {
                args.push(current.clone());
                current.clear();
            }
        } else {
            current.push(c);
        }
    }
    if !current.is_empty() {
        args.push(current);
    }
    args
}

/// Convert extracted pattern to IR
#[must_use]
pub fn pattern_to_ir(pattern: &ExtractedPattern) -> PatternIR {
    match &pattern.pattern_type {
        PatternType::CommandPattern { commands, .. } => PatternIR::CommandSeq {
            commands: commands
                .iter()
                .map(|cmd| {
                    let parts = tokenize_command(cmd);
                    // Handle env vars (VAR=val) at the start
                    let mut start_idx = 0;
                    let mut env_vars = Vec::new();

                    while start_idx < parts.len() {
                        let part = &parts[start_idx];
                        if part.contains('=') && !part.starts_with('-') {
                            let mut split = part.splitn(2, '=');
                            if let (Some(key), Some(val)) = (split.next(), split.next()) {
                                env_vars.push((key.to_string(), val.to_string()));
                                start_idx += 1;
                                continue;
                            }
                        }
                        break;
                    }

                    let executable = if start_idx < parts.len() {
                        parts[start_idx].clone()
                    } else {
                        "".to_string()
                    };

                    let args = if start_idx + 1 < parts.len() {
                        parts[start_idx + 1..].to_vec()
                    } else {
                        vec![]
                    };

                    CommandIR {
                        executable,
                        args,
                        env: env_vars,
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

/// Get a safe prefix of a string by character count (UTF-8 safe)
fn safe_prefix(s: &str, max_chars: usize) -> String {
    s.chars().take(max_chars).collect()
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
    fn test_safe_prefix() {
        // ASCII
        assert_eq!(safe_prefix("hello", 3), "hel");
        assert_eq!(safe_prefix("hello", 10), "hello");

        // UTF-8 multi-byte characters (e.g., Japanese)
        // "こんにちは" = 5 characters, but 15 bytes
        let japanese = "こんにちは";
        assert_eq!(japanese.len(), 15); // Verify it's multi-byte
        assert_eq!(safe_prefix(japanese, 3), "こんに");
        assert_eq!(safe_prefix(japanese, 5), "こんにちは");
        assert_eq!(safe_prefix(japanese, 10), "こんにちは"); // Doesn't panic on oversized

        // Mix of ASCII and multi-byte
        let mixed = "abc日本語def";
        assert_eq!(safe_prefix(mixed, 5), "abc日本");
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
            taint_label: None,
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
            taint_label: None,
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
            taint_label: None,
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
            taint_label: None,
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
            taint_label: None,
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
            taint_label: None,
        };

        assert!(!patterns_are_similar(&p1, &p3));
    }

    #[test]
    fn test_patterns_are_similar_false_positive_fixed() {
        // Two very different Python scripts with same imports
        let code_a = "import os\nimport sys\n\nprint('Hello World')";
        let code_b = "import os\nimport sys\n\nos.remove('/')"; // Dangerous different code

        let p1 = ExtractedPattern {
            id: "1".to_string(),
            pattern_type: PatternType::CodePattern {
                language: "python".to_string(),
                code: code_a.to_string(),
                purpose: "a".to_string(),
                frequency: 1,
            },
            evidence: vec![],
            confidence: 0.8,
            frequency: 1,
            tags: vec![],
            description: None,
            taint_label: None,
        };

        let p2 = ExtractedPattern {
            id: "2".to_string(),
            pattern_type: PatternType::CodePattern {
                language: "python".to_string(),
                code: code_b.to_string(),
                purpose: "b".to_string(),
                frequency: 1,
            },
            evidence: vec![],
            confidence: 0.8,
            frequency: 1,
            tags: vec![],
            description: None,
            taint_label: None,
        };

        // Fixed behavior: returns false because content (normalized) is different
        assert!(!patterns_are_similar(&p1, &p2));
    }

    #[test]
    fn test_tokenize_command() {
        let cmd = r#"git commit -m "fix bug" --author='Jane Doe'"#;
        let tokens = tokenize_command(cmd);
        assert_eq!(tokens.len(), 5);
        assert_eq!(tokens[0], "git");
        assert_eq!(tokens[1], "commit");
        assert_eq!(tokens[2], "-m");
        assert_eq!(tokens[3], "fix bug");
        assert_eq!(tokens[4], "--author=Jane Doe");

        let escaped = r#"echo "escaped \" quote""#;
        let tokens = tokenize_command(escaped);
        assert_eq!(tokens.len(), 2);
        assert_eq!(tokens[1], "escaped \" quote");
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
                taint_label: None,
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
                taint_label: None,
            },
        ];

        let deduped = deduplicate_patterns(patterns);
        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].frequency, 2); // Merged
        assert!(deduped[0].confidence > 0.6); // Boosted
    }

    #[test]
    fn test_classify_bash_command_false_positives() {
        // "latest" contains "test"
        assert_ne!(
            classify_bash_command("echo 'latest results'"),
            SessionPhase::Validation,
            "echo 'latest results' should not be Validation"
        );

        // "checking" contains "check"
        assert_ne!(
            classify_bash_command("echo 'checking connection'"),
            SessionPhase::Validation,
            "echo 'checking connection' should not be Validation"
        );
    }

    #[test]
    fn test_extract_command_patterns_env_vars() {
        use super::super::client::{Session, SessionMessage, ToolCall};
        let session = Session {
            id: "test-session".to_string(),
            path: "/path/to/session.json".to_string(),
            content_hash: "hash".to_string(),
            messages: vec![SessionMessage {
                index: 0,
                role: "assistant".to_string(),
                content: "running command".to_string(),
                tool_calls: vec![ToolCall {
                    id: "call-1".to_string(),
                    name: "bash".to_string(),
                    arguments: serde_json::json!({
                        "command": "RUST_LOG=debug ./my-script.sh"
                    }),
                }],
                tool_results: vec![],
            }],
            metadata: Default::default(),
        };

        let pattern = extract_command_patterns(&session).expect("Should extract pattern");

        if let PatternType::CommandPattern { commands: ref _outer_commands, .. } = pattern.pattern_type {
            let ir = pattern_to_ir(&pattern);
            if let PatternIR::CommandSeq { commands, .. } = ir {
                let cmd = &commands[0];
                assert_ne!(
                    cmd.executable, "RUST_LOG=debug",
                    "Environment variable captured as executable"
                );
                assert_eq!(
                    cmd.executable, "./my-script.sh",
                    "Executable should be the command itself"
                );
            } else {
                assert!(false, "Expected CommandSeq IR");
            }
        }
    }
}
