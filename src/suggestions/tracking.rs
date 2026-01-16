//! Session and suggestion tracking for implicit feedback collection.
//!
//! This module provides trackers that collect implicit signals about skill
//! usefulness by monitoring skill loading/unloading patterns and suggestion
//! selection behavior.

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::suggestions::bandit::features::ContextFeatures;
use crate::suggestions::bandit::rewards::SkillFeedback;
use crate::suggestions::bandit::ContextualBandit;

/// Tracks skill loading sessions for implicit feedback.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SessionTracker {
    /// Unique session identifier.
    pub session_id: String,

    /// When the session started.
    pub started_at: Option<DateTime<Utc>>,

    /// Currently loaded skills with their session data.
    loaded_skills: HashMap<String, SkillSession>,

    /// Interaction events during this session.
    interactions: Vec<SessionInteraction>,
}

/// Session data for a single loaded skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillSession {
    /// Skill ID.
    pub skill_id: String,

    /// When the skill was loaded.
    pub loaded_at: DateTime<Utc>,

    /// When the skill was unloaded (if applicable).
    pub unloaded_at: Option<DateTime<Utc>>,

    /// Interactions with this skill during the session.
    pub interactions: Vec<SkillInteraction>,
}

/// An interaction with a skill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillInteraction {
    /// Type of interaction.
    pub interaction_type: InteractionType,

    /// When the interaction occurred.
    pub timestamp: DateTime<Utc>,
}

/// Types of interactions with a skill.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum InteractionType {
    /// Skill content was viewed/displayed.
    ContentViewed,
    /// A rule from the skill was followed.
    RuleFollowed,
    /// An example from the skill was used.
    ExampleUsed,
    /// Checklist item was completed.
    ChecklistProgressed,
    /// Skill was searched for.
    Searched,
    /// Skill was loaded at a specific disclosure level.
    LoadedAtLevel { level: String },
}

/// High-level interaction during a session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInteraction {
    /// Skill ID (if skill-specific).
    pub skill_id: Option<String>,

    /// What happened.
    pub event: SessionEvent,

    /// When it happened.
    pub timestamp: DateTime<Utc>,
}

/// Session-level events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SessionEvent {
    /// Session started.
    Started,
    /// Skill was loaded.
    SkillLoaded { skill_id: String },
    /// Skill was unloaded.
    SkillUnloaded { skill_id: String },
    /// Suggestion was shown.
    SuggestionShown { skill_ids: Vec<String> },
    /// Suggestion was selected.
    SuggestionSelected { skill_id: String },
    /// Session ended.
    Ended,
}

impl SessionTracker {
    /// Create a new session tracker.
    #[must_use]
    pub fn new() -> Self {
        Self {
            session_id: uuid::Uuid::new_v4().to_string(),
            started_at: Some(Utc::now()),
            loaded_skills: HashMap::new(),
            interactions: vec![SessionInteraction {
                skill_id: None,
                event: SessionEvent::Started,
                timestamp: Utc::now(),
            }],
        }
    }

    /// Create a tracker with a specific session ID.
    #[must_use]
    pub fn with_id(session_id: String) -> Self {
        Self {
            session_id,
            started_at: Some(Utc::now()),
            loaded_skills: HashMap::new(),
            interactions: vec![SessionInteraction {
                skill_id: None,
                event: SessionEvent::Started,
                timestamp: Utc::now(),
            }],
        }
    }

    /// Record that a skill was loaded.
    pub fn on_skill_load(&mut self, skill_id: &str) {
        let now = Utc::now();
        self.loaded_skills.insert(
            skill_id.to_string(),
            SkillSession {
                skill_id: skill_id.to_string(),
                loaded_at: now,
                unloaded_at: None,
                interactions: vec![],
            },
        );
        self.interactions.push(SessionInteraction {
            skill_id: Some(skill_id.to_string()),
            event: SessionEvent::SkillLoaded {
                skill_id: skill_id.to_string(),
            },
            timestamp: now,
        });
    }

    /// Record that a skill was unloaded.
    pub fn on_skill_unload(&mut self, skill_id: &str) -> Option<SkillFeedback> {
        let now = Utc::now();
        self.interactions.push(SessionInteraction {
            skill_id: Some(skill_id.to_string()),
            event: SessionEvent::SkillUnloaded {
                skill_id: skill_id.to_string(),
            },
            timestamp: now,
        });

        if let Some(session) = self.loaded_skills.get_mut(skill_id) {
            session.unloaded_at = Some(now);
            Some(Self::compute_implicit_feedback_static(session))
        } else {
            None
        }
    }

    /// Record an interaction with a skill.
    pub fn record_interaction(&mut self, skill_id: &str, interaction_type: InteractionType) {
        if let Some(session) = self.loaded_skills.get_mut(skill_id) {
            session.interactions.push(SkillInteraction {
                interaction_type,
                timestamp: Utc::now(),
            });
        }
    }

    /// Check if a skill is currently loaded.
    #[must_use]
    pub fn is_skill_loaded(&self, skill_id: &str) -> bool {
        self.loaded_skills
            .get(skill_id)
            .map_or(false, |s| s.unloaded_at.is_none())
    }

    /// Get the duration a skill has been loaded (in minutes).
    #[must_use]
    pub fn skill_load_duration_minutes(&self, skill_id: &str) -> Option<u32> {
        self.loaded_skills.get(skill_id).map(|session| {
            let end = session.unloaded_at.unwrap_or_else(Utc::now);
            let duration = end.signed_duration_since(session.loaded_at);
            duration.num_minutes().max(0) as u32
        })
    }

    /// End the session and return feedback for all loaded skills.
    pub fn end_session(&mut self) -> Vec<(String, SkillFeedback)> {
        let now = Utc::now();
        self.interactions.push(SessionInteraction {
            skill_id: None,
            event: SessionEvent::Ended,
            timestamp: now,
        });

        let mut feedbacks = Vec::new();
        for (skill_id, session) in &mut self.loaded_skills {
            if session.unloaded_at.is_none() {
                session.unloaded_at = Some(now);
            }
            let feedback = Self::compute_implicit_feedback_static(session);
            feedbacks.push((skill_id.clone(), feedback));
        }
        feedbacks
    }

    /// Compute implicit feedback based on a skill session.
    fn compute_implicit_feedback_static(session: &SkillSession) -> SkillFeedback {
        let end = session.unloaded_at.unwrap_or_else(Utc::now);
        let duration = end.signed_duration_since(session.loaded_at);
        let minutes = duration.num_minutes().max(0) as u32;

        if minutes < 1 {
            SkillFeedback::UnloadedQuickly
        } else if session.interactions.is_empty() {
            SkillFeedback::LoadedOnly
        } else {
            SkillFeedback::UsedDuration { minutes }
        }
    }

    /// Get session statistics.
    #[must_use]
    pub fn stats(&self) -> SessionStats {
        let total_loaded = self.loaded_skills.len();
        let currently_loaded = self
            .loaded_skills
            .values()
            .filter(|s| s.unloaded_at.is_none())
            .count();
        let total_interactions: usize = self
            .loaded_skills
            .values()
            .map(|s| s.interactions.len())
            .sum();

        SessionStats {
            session_id: self.session_id.clone(),
            started_at: self.started_at,
            total_skills_loaded: total_loaded,
            currently_loaded,
            total_interactions,
        }
    }
}

/// Session statistics summary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionStats {
    /// Session ID.
    pub session_id: String,
    /// When the session started.
    pub started_at: Option<DateTime<Utc>>,
    /// Total skills loaded during session.
    pub total_skills_loaded: usize,
    /// Skills still loaded.
    pub currently_loaded: usize,
    /// Total interactions with skills.
    pub total_interactions: usize,
}

/// Tracks suggestions shown to users for feedback collection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SuggestionTracker {
    /// Suggestions that have been shown.
    shown_suggestions: HashMap<String, SuggestionRecord>,

    /// Timestamp of the last suggestion display.
    last_suggestion_time: Option<DateTime<Utc>>,
}

/// Record of a suggestion that was shown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuggestionRecord {
    /// Skill ID that was suggested.
    pub skill_id: String,

    /// When the suggestion was shown.
    pub shown_at: DateTime<Utc>,

    /// Context fingerprint hash when shown (u64 hash of the full ContextFingerprint).
    pub context_fingerprint_hash: Option<u64>,

    /// Position in the suggestion list (0 = top).
    pub position: usize,

    /// What happened with this suggestion.
    pub outcome: SuggestionOutcome,
}

/// Outcome of a suggestion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum SuggestionOutcome {
    /// Suggestion is pending (not yet acted upon).
    Pending,
    /// User selected/loaded this skill.
    Selected,
    /// Suggestion was ignored (session ended without selection).
    Ignored,
    /// User explicitly hid this skill.
    Hidden,
}

impl SuggestionTracker {
    /// Create a new suggestion tracker.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Record that suggestions were shown to the user.
    pub fn record_suggestions(
        &mut self,
        skill_ids: &[String],
        context_fingerprint_hash: Option<u64>,
    ) {
        let now = Utc::now();
        self.last_suggestion_time = Some(now);

        for (position, skill_id) in skill_ids.iter().enumerate() {
            // Only record if not already pending
            if !self
                .shown_suggestions
                .get(skill_id)
                .map_or(false, |r| r.outcome == SuggestionOutcome::Pending)
            {
                self.shown_suggestions.insert(
                    skill_id.clone(),
                    SuggestionRecord {
                        skill_id: skill_id.clone(),
                        shown_at: now,
                        context_fingerprint_hash,
                        position,
                        outcome: SuggestionOutcome::Pending,
                    },
                );
            }
        }
    }

    /// Record that a user selected a suggestion.
    pub fn on_suggestion_selected(&mut self, skill_id: &str) {
        if let Some(record) = self.shown_suggestions.get_mut(skill_id) {
            record.outcome = SuggestionOutcome::Selected;
        }
    }

    /// Record that a user hid a suggestion.
    pub fn on_suggestion_hidden(&mut self, skill_id: &str) {
        if let Some(record) = self.shown_suggestions.get_mut(skill_id) {
            record.outcome = SuggestionOutcome::Hidden;
        }
    }

    /// End tracking and return feedback for ignored suggestions.
    pub fn end_tracking(&mut self) -> Vec<(String, SkillFeedback)> {
        let mut feedbacks = Vec::new();

        for (skill_id, record) in &mut self.shown_suggestions {
            if record.outcome == SuggestionOutcome::Pending {
                record.outcome = SuggestionOutcome::Ignored;
                feedbacks.push((skill_id.clone(), SkillFeedback::Ignored));
            }
        }

        feedbacks
    }

    /// Get pending suggestions that haven't been acted upon.
    #[must_use]
    pub fn pending_suggestions(&self) -> Vec<&SuggestionRecord> {
        self.shown_suggestions
            .values()
            .filter(|r| r.outcome == SuggestionOutcome::Pending)
            .collect()
    }

    /// Get all suggestion records.
    #[must_use]
    pub fn all_suggestions(&self) -> Vec<&SuggestionRecord> {
        self.shown_suggestions.values().collect()
    }

    /// Clear all tracked suggestions.
    pub fn clear(&mut self) {
        self.shown_suggestions.clear();
        self.last_suggestion_time = None;
    }
}

/// Combines session and suggestion tracking.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FeedbackCollector {
    /// Session tracker for skill load/unload signals.
    pub session: SessionTracker,

    /// Suggestion tracker for suggestion selection signals.
    pub suggestions: SuggestionTracker,
}

impl FeedbackCollector {
    /// Create a new feedback collector.
    #[must_use]
    pub fn new() -> Self {
        Self {
            session: SessionTracker::new(),
            suggestions: SuggestionTracker::new(),
        }
    }

    /// Create with a specific session ID.
    #[must_use]
    pub fn with_session_id(session_id: String) -> Self {
        Self {
            session: SessionTracker::with_id(session_id),
            suggestions: SuggestionTracker::new(),
        }
    }

    /// Record skill load event.
    pub fn on_skill_load(&mut self, skill_id: &str) {
        self.session.on_skill_load(skill_id);
        // If this skill was suggested, mark it as selected
        self.suggestions.on_suggestion_selected(skill_id);
    }

    /// Record skill unload event.
    pub fn on_skill_unload(&mut self, skill_id: &str) -> Option<SkillFeedback> {
        self.session.on_skill_unload(skill_id)
    }

    /// Record suggestions shown.
    pub fn on_suggestions_shown(
        &mut self,
        skill_ids: &[String],
        context_fingerprint_hash: Option<u64>,
    ) {
        self.suggestions
            .record_suggestions(skill_ids, context_fingerprint_hash);
    }

    /// End the session and collect all feedback.
    pub fn end_session(&mut self) -> Vec<(String, SkillFeedback)> {
        let mut all_feedback = Vec::new();

        // Get session feedback for loaded skills
        all_feedback.extend(self.session.end_session());

        // Get feedback for ignored suggestions
        all_feedback.extend(self.suggestions.end_tracking());

        all_feedback
    }

    /// Get session ID.
    #[must_use]
    pub fn session_id(&self) -> &str {
        &self.session.session_id
    }

    /// Update a contextual bandit with feedback from a single skill interaction.
    ///
    /// Call this after skill unload events to provide incremental feedback.
    pub fn update_bandit_for_skill(
        &self,
        bandit: &mut ContextualBandit,
        skill_id: &str,
        feedback: &SkillFeedback,
        features: &ContextFeatures,
    ) {
        bandit.update(skill_id, features, feedback);
    }

    /// End session and flush all collected feedback to the bandit.
    ///
    /// This collects all implicit feedback (from loaded skills and ignored suggestions)
    /// and updates the bandit for each skill.
    pub fn end_session_and_update_bandit(
        &mut self,
        bandit: &mut ContextualBandit,
        features: &ContextFeatures,
    ) -> Vec<(String, SkillFeedback)> {
        let all_feedback = self.end_session();

        for (skill_id, feedback) in &all_feedback {
            bandit.update(skill_id, features, feedback);
        }

        all_feedback
    }

    /// Record skill unload and immediately update the bandit.
    ///
    /// Convenience method that combines unload tracking with bandit update.
    pub fn on_skill_unload_and_update_bandit(
        &mut self,
        skill_id: &str,
        bandit: &mut ContextualBandit,
        features: &ContextFeatures,
    ) -> Option<SkillFeedback> {
        if let Some(feedback) = self.on_skill_unload(skill_id) {
            bandit.update(skill_id, features, &feedback);
            Some(feedback)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_session_tracker_new() {
        let tracker = SessionTracker::new();
        assert!(!tracker.session_id.is_empty());
        assert!(tracker.started_at.is_some());
    }

    #[test]
    fn test_skill_load_unload() {
        let mut tracker = SessionTracker::new();

        tracker.on_skill_load("rust-errors");
        assert!(tracker.is_skill_loaded("rust-errors"));

        let feedback = tracker.on_skill_unload("rust-errors");
        assert!(feedback.is_some());
        // Quick unload since no time elapsed
        assert!(matches!(feedback.unwrap(), SkillFeedback::UnloadedQuickly));
    }

    #[test]
    fn test_skill_with_interactions() {
        let mut tracker = SessionTracker::new();

        tracker.on_skill_load("rust-errors");
        tracker.record_interaction("rust-errors", InteractionType::ContentViewed);
        tracker.record_interaction("rust-errors", InteractionType::RuleFollowed);

        // End session should produce feedback
        let feedbacks = tracker.end_session();
        assert_eq!(feedbacks.len(), 1);

        // Should be LoadedOnly or UsedDuration depending on timing
        let (skill_id, _feedback) = &feedbacks[0];
        assert_eq!(skill_id, "rust-errors");
    }

    #[test]
    fn test_session_stats() {
        let mut tracker = SessionTracker::new();

        tracker.on_skill_load("skill-a");
        tracker.on_skill_load("skill-b");
        tracker.record_interaction("skill-a", InteractionType::ExampleUsed);
        let _ = tracker.on_skill_unload("skill-b");

        let stats = tracker.stats();
        assert_eq!(stats.total_skills_loaded, 2);
        assert_eq!(stats.currently_loaded, 1);
        assert_eq!(stats.total_interactions, 1);
    }

    #[test]
    fn test_suggestion_tracker() {
        let mut tracker = SuggestionTracker::new();

        tracker.record_suggestions(&["skill-a".to_string(), "skill-b".to_string()], None);

        assert_eq!(tracker.pending_suggestions().len(), 2);

        tracker.on_suggestion_selected("skill-a");

        let pending = tracker.pending_suggestions();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].skill_id, "skill-b");
    }

    #[test]
    fn test_suggestion_end_tracking() {
        let mut tracker = SuggestionTracker::new();

        tracker.record_suggestions(
            &[
                "skill-a".to_string(),
                "skill-b".to_string(),
                "skill-c".to_string(),
            ],
            None,
        );

        tracker.on_suggestion_selected("skill-a");
        tracker.on_suggestion_hidden("skill-b");

        let feedbacks = tracker.end_tracking();

        // Only skill-c was ignored
        assert_eq!(feedbacks.len(), 1);
        assert_eq!(feedbacks[0].0, "skill-c");
        assert!(matches!(feedbacks[0].1, SkillFeedback::Ignored));
    }

    #[test]
    fn test_feedback_collector_integration() {
        let mut collector = FeedbackCollector::new();

        // Show suggestions
        collector.on_suggestions_shown(
            &["skill-a".to_string(), "skill-b".to_string()],
            None,
        );

        // Load skill-a (marks as selected)
        collector.on_skill_load("skill-a");

        // Unload skill-a quickly
        let feedback = collector.on_skill_unload("skill-a");
        assert!(feedback.is_some());

        // End session
        let all_feedback = collector.end_session();

        // Should have feedback for skill-a (loaded) and skill-b (ignored)
        assert!(all_feedback.len() >= 1);
    }

    #[test]
    fn test_feedback_collector_session_id() {
        let collector = FeedbackCollector::with_session_id("test-session-123".to_string());
        assert_eq!(collector.session_id(), "test-session-123");
    }

    #[test]
    fn test_loaded_duration_calculation() {
        let mut tracker = SessionTracker::new();
        tracker.on_skill_load("skill-a");

        // Duration should be 0 or close to 0 immediately
        let duration = tracker.skill_load_duration_minutes("skill-a");
        assert!(duration.is_some());
        assert!(duration.unwrap() < 1);
    }
}
