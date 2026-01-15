//! Anti-pattern mining and extraction
//!
//! This module extracts anti-patterns from CASS sessions by detecting:
//! - Rollback operations (git reset, git revert, file restore)
//! - Explicit corrections ("No, do X instead")
//! - Failure signals (test failures, build errors, exceptions)
//! - Explicit markers (sessions marked as anti-pattern examples)
//!
//! Anti-patterns are negative patterns that represent what NOT to do.
//! Each anti-pattern should link to the positive pattern it constrains.
//!
//! # Architecture
//!
//! ```text
//! Session → Detection → Signals → Context Extraction → Clustering → Synthesis → AntiPattern
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! use ms::antipatterns::{DefaultDetector, AntiPatternDetector, mine_anti_patterns};
//! use ms::cass::client::Session;
//!
//! let session: Session = // ... load session
//! let detector = DefaultDetector::default();
//!
//! // Detect signals
//! let signals = detector.detect_signals(&session);
//! let rollbacks = detector.detect_rollbacks(&session);
//! let corrections = detector.find_corrections(&session);
//!
//! // Mine anti-patterns
//! let anti_patterns = mine_anti_patterns(&[session], &detector)?;
//! ```

pub mod detection;
pub mod types;

pub use detection::{
    AntiPatternDetector, AntiPatternSignal, Correction, DefaultDetector, MarkedAntiPattern,
    RollbackSequence,
};
pub use types::{
    AntiPattern, AntiPatternCluster, AntiPatternContext, AntiPatternEvidence, AntiPatternId,
    AntiPatternSection, AntiPatternSeverity, AntiPatternSource, Condition, FailureIncident,
    FailureMode, FailureSignalType, FormattedAntiPattern, NegativeRule, PatternId, RollbackType,
    SessionId,
};

use crate::cass::client::Session;
use crate::error::Result;

// =============================================================================
// High-Level Mining API
// =============================================================================

/// Mine anti-patterns from a collection of sessions
pub fn mine_anti_patterns(
    sessions: &[Session],
    detector: &dyn AntiPatternDetector,
) -> Result<Vec<AntiPattern>> {
    let mut all_evidence: Vec<AntiPatternEvidence> = Vec::new();

    for session in sessions {
        // Detect all signal types
        let signals = detector.detect_signals(session);
        let rollbacks = detector.detect_rollbacks(session);
        let corrections = detector.find_corrections(session);

        // Convert to evidence
        all_evidence.extend(detection::signals_to_evidence(signals, session));
        all_evidence.extend(detection::rollbacks_to_evidence(rollbacks, session));
        all_evidence.extend(detection::corrections_to_evidence(corrections, session));

        // Check for explicit markers
        if let Some(marker) = detector.check_markers(session) {
            let incident = FailureIncident {
                description: format!("Marked as anti-pattern: {}", marker.marker),
                failed_action: "session marked as anti-pattern example".to_string(),
                message_indices: vec![marker.message_idx],
                context: types::IncidentContext {
                    before: vec![],
                    after: vec![],
                    location: None,
                    tool: None,
                },
                correction: marker.annotation.clone(),
            };

            all_evidence.push(AntiPatternEvidence::new(
                AntiPatternSource::MarkedAntiPattern {
                    marker: marker.marker,
                },
                SessionId::new(&session.id),
                incident,
            ));
        }
    }

    // Cluster and synthesize anti-patterns
    let clusters = cluster_evidence(&all_evidence);
    let anti_patterns = synthesize_from_clusters(clusters);

    Ok(anti_patterns)
}

/// Cluster similar evidence into groups
fn cluster_evidence(evidence: &[AntiPatternEvidence]) -> Vec<EvidenceCluster> {
    if evidence.is_empty() {
        return Vec::new();
    }

    // Simple clustering by source type and failed action similarity
    let mut clusters: Vec<EvidenceCluster> = Vec::new();

    for ev in evidence {
        // Try to find a matching cluster
        let mut found = false;
        for cluster in &mut clusters {
            if cluster.matches(ev) {
                cluster.add(ev.clone());
                found = true;
                break;
            }
        }

        if !found {
            let mut new_cluster = EvidenceCluster::new();
            new_cluster.add(ev.clone());
            clusters.push(new_cluster);
        }
    }

    clusters
}

/// A cluster of similar evidence
struct EvidenceCluster {
    evidence: Vec<AntiPatternEvidence>,
    primary_source_type: Option<std::mem::Discriminant<AntiPatternSource>>,
}

impl EvidenceCluster {
    fn new() -> Self {
        Self {
            evidence: Vec::new(),
            primary_source_type: None,
        }
    }

    fn add(&mut self, ev: AntiPatternEvidence) {
        if self.primary_source_type.is_none() {
            self.primary_source_type = Some(std::mem::discriminant(&ev.source));
        }
        self.evidence.push(ev);
    }

    fn matches(&self, ev: &AntiPatternEvidence) -> bool {
        // Same source type
        if let Some(primary) = &self.primary_source_type {
            if std::mem::discriminant(&ev.source) != *primary {
                return false;
            }
        }

        // Check for similar failed actions
        if let Some(first) = self.evidence.first() {
            let similarity = compute_action_similarity(
                &first.incident.failed_action,
                &ev.incident.failed_action,
            );
            return similarity > 0.5;
        }

        true
    }
}

/// Compute similarity between two action descriptions
fn compute_action_similarity(a: &str, b: &str) -> f32 {
    let a_lower = a.to_lowercase();
    let b_lower = b.to_lowercase();

    // Normalize CLI flags: strip leading dashes so "--hard" becomes "hard"
    // This ensures "git reset hard" and "git reset --hard" are considered similar
    let a_words: std::collections::HashSet<_> = a_lower
        .split_whitespace()
        .map(|w| w.trim_start_matches('-'))
        .collect();
    let b_words: std::collections::HashSet<_> = b_lower
        .split_whitespace()
        .map(|w| w.trim_start_matches('-'))
        .collect();

    if a_words.is_empty() || b_words.is_empty() {
        return 0.0;
    }

    let intersection = a_words.intersection(&b_words).count();
    let union = a_words.union(&b_words).count();

    intersection as f32 / union as f32
}

/// Synthesize anti-patterns from clusters
fn synthesize_from_clusters(clusters: Vec<EvidenceCluster>) -> Vec<AntiPattern> {
    clusters
        .into_iter()
        .filter(|c| !c.evidence.is_empty())
        .map(|cluster| {
            // Generate rule from cluster
            let rule = synthesize_rule(&cluster);

            // Create anti-pattern
            let mut ap = AntiPattern::new(rule);

            // Add all evidence
            for ev in cluster.evidence {
                ap.add_evidence(ev);
            }

            // Extract failure modes
            ap.failure_modes = extract_failure_modes(&ap.evidence);

            ap
        })
        .collect()
}

/// Synthesize a negative rule from a cluster
fn synthesize_rule(cluster: &EvidenceCluster) -> NegativeRule {
    let first = cluster
        .evidence
        .first()
        .expect("cluster should not be empty");

    // Generate statement based on source type
    let statement = match &first.source {
        AntiPatternSource::RollbackDetected { rollback_type } => {
            format!(
                "AVOID actions that require {} to fix",
                rollback_type.description()
            )
        }
        AntiPatternSource::FailureSignal { signal_type } => {
            format!("AVOID actions that cause {}", signal_type.description())
        }
        AntiPatternSource::WrongFix {
            original,
            correction,
        } => {
            format!(
                "NEVER {} — instead {}",
                truncate(original, 50),
                truncate(correction, 50)
            )
        }
        AntiPatternSource::MarkedAntiPattern { marker } => {
            format!("AVOID: {}", marker)
        }
        AntiPatternSource::CounterExample { .. } => {
            "AVOID this counter-example pattern".to_string()
        }
    };

    // Determine severity based on evidence count
    let severity = if cluster.evidence.len() >= 3 {
        AntiPatternSeverity::Warning
    } else {
        AntiPatternSeverity::Advisory
    };

    let mut rule = NegativeRule::new(statement, severity);

    // Add rationale from first evidence
    rule.rationale = Some(first.incident.description.clone());

    // Add "instead" from correction if available
    if let Some(correction) = &first.incident.correction {
        rule.instead = Some(correction.clone());
    }

    rule
}

/// Extract failure modes from evidence
fn extract_failure_modes(evidence: &[AntiPatternEvidence]) -> Vec<FailureMode> {
    let mut modes: std::collections::HashMap<String, FailureMode> =
        std::collections::HashMap::new();

    for ev in evidence {
        let desc = &ev.incident.description;
        modes
            .entry(desc.clone())
            .and_modify(|m| m.increment())
            .or_insert_with(|| {
                let mut fm = FailureMode::new(desc);
                fm.example_session = Some(ev.session_id.clone());
                fm
            });
    }

    modes.into_values().collect()
}

/// Truncate string with ellipsis (character-aware for UTF-8 safety)
fn truncate(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars).collect();
        format!("{}...", truncated)
    }
}

// =============================================================================
// Formatting and Output
// =============================================================================

/// Format anti-patterns for inclusion in skill output
pub fn format_anti_patterns(anti_patterns: &[AntiPattern]) -> AntiPatternSection {
    let mut section = AntiPatternSection::default();

    for ap in anti_patterns {
        // Skip low-confidence anti-patterns
        if ap.confidence < 0.3 {
            continue;
        }

        let formatted = FormattedAntiPattern {
            rule: ap.rule.statement.clone(),
            severity: ap.rule.severity,
            conditions: ap
                .trigger_conditions
                .iter()
                .map(|c| c.description.clone())
                .collect(),
            rationale: ap
                .rule
                .rationale
                .clone()
                .unwrap_or_else(|| "Observed failure pattern".to_string()),
            instead: ap
                .rule
                .instead
                .clone()
                .unwrap_or_else(|| "Use the recommended approach".to_string()),
            evidence_summary: format!(
                "{} session(s), {} evidence source(s)",
                count_unique_sessions(&ap.evidence),
                ap.evidence.len()
            ),
            example: ap
                .evidence
                .first()
                .map(|e| e.incident.failed_action.clone()),
        };

        section.patterns.push(formatted);
    }

    section
}

/// Count unique sessions in evidence
fn count_unique_sessions(evidence: &[AntiPatternEvidence]) -> usize {
    let sessions: std::collections::HashSet<_> = evidence.iter().map(|e| &e.session_id.0).collect();
    sessions.len()
}

/// Find orphaned anti-patterns (no positive counterpart)
pub fn find_orphaned(anti_patterns: &[AntiPattern]) -> Vec<&AntiPattern> {
    anti_patterns.iter().filter(|ap| ap.is_orphaned()).collect()
}

/// Link an anti-pattern to a positive pattern
pub fn link_to_pattern(anti_pattern: &mut AntiPattern, pattern_id: PatternId) {
    anti_pattern.constrains = Some(pattern_id);
    anti_pattern.updated_at = chrono::Utc::now();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_action_similarity() {
        // CLI flags should be normalized (--hard == hard)
        assert!(compute_action_similarity("git reset hard", "git reset --hard") > 0.5);
        // Completely different actions should have low similarity
        assert!(compute_action_similarity("build project", "run tests") < 0.5);
        // Identical strings should have similarity 1.0
        assert!((compute_action_similarity("git commit", "git commit") - 1.0).abs() < 0.001);
        // Single-dash flags should also be normalized (-f == f)
        assert!(compute_action_similarity("rm -f file", "rm f file") > 0.5);
        // Double-dash long flags
        assert!(compute_action_similarity("npm install --save-dev", "npm install save-dev") > 0.5);
    }

    #[test]
    fn test_format_anti_patterns_filters_low_confidence() {
        let mut ap = AntiPattern::new(NegativeRule::new("Test", AntiPatternSeverity::Advisory));
        ap.confidence = 0.1; // Below threshold

        let section = format_anti_patterns(&[ap]);
        assert!(section.is_empty());
    }

    #[test]
    fn test_truncate() {
        assert_eq!(truncate("short", 10), "short");
        assert_eq!(truncate("this is a long string", 10), "this is a ...");
    }

    #[test]
    fn test_truncate_utf8_safety() {
        // Multi-byte UTF-8 characters should not panic
        let emoji_str = "Hello 🦀🦀🦀 world!";
        let result = truncate(emoji_str, 8);
        // Should truncate to 8 characters: "Hello 🦀🦀"
        assert_eq!(result, "Hello 🦀🦀...");

        // Japanese characters (3 bytes each)
        let japanese = "日本語テスト";
        let result = truncate(japanese, 3);
        assert_eq!(result, "日本語...");
    }
}
