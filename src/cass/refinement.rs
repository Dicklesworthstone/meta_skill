//! Iterative skill refinement
//!
//! Improves skills through feedback and iteration.

use crate::Result;

use super::synthesis::SkillDraft;

/// Refine a skill draft based on feedback.
///
/// Parses the feedback string for refinement commands:
/// - `+tag:X` - Add tag X
/// - `-tag:X` - Remove tag X
/// - `name:X` - Set the skill name to X
/// - `description:X` - Set the skill description to X
///
/// Any feedback lines that don't match a command are appended
/// to the content as a "Refinement Notes" section.
pub fn refine_skill(draft: &mut SkillDraft, feedback: &str) -> Result<()> {
    let mut notes: Vec<String> = Vec::new();

    for line in feedback.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some(tag) = trimmed.strip_prefix("+tag:") {
            // Add tag
            let tag = tag.trim().to_string();
            if !tag.is_empty() && !draft.tags.contains(&tag) {
                draft.tags.push(tag);
            }
        } else if let Some(tag) = trimmed.strip_prefix("-tag:") {
            // Remove tag
            let tag = tag.trim();
            draft.tags.retain(|t| t != tag);
        } else if let Some(name) = trimmed.strip_prefix("name:") {
            // Set name
            let name = name.trim();
            if !name.is_empty() {
                draft.name = name.to_string();
            }
        } else if let Some(desc) = trimmed.strip_prefix("description:") {
            // Set description
            let desc = desc.trim();
            if !desc.is_empty() {
                draft.description = desc.to_string();
            }
        } else {
            // Collect as note
            notes.push(trimmed.to_string());
        }
    }

    // Append notes to content if any
    if !notes.is_empty() {
        if !draft.content.is_empty() && !draft.content.ends_with('\n') {
            draft.content.push('\n');
        }
        draft.content.push_str("\n## Refinement Notes\n\n");
        for note in notes {
            draft.content.push_str("- ");
            draft.content.push_str(&note);
            draft.content.push('\n');
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_draft() -> SkillDraft {
        SkillDraft {
            name: "Test Skill".to_string(),
            description: "A test skill".to_string(),
            content: "## Content\n\nSome content.".to_string(),
            tags: vec!["rust".to_string(), "testing".to_string()],
        }
    }

    #[test]
    fn test_add_tag() {
        let mut draft = sample_draft();
        refine_skill(&mut draft, "+tag:new-tag").unwrap();
        assert!(draft.tags.contains(&"new-tag".to_string()));
    }

    #[test]
    fn test_add_duplicate_tag() {
        let mut draft = sample_draft();
        let orig_len = draft.tags.len();
        refine_skill(&mut draft, "+tag:rust").unwrap();
        assert_eq!(draft.tags.len(), orig_len, "Should not add duplicate");
    }

    #[test]
    fn test_remove_tag() {
        let mut draft = sample_draft();
        assert!(draft.tags.contains(&"testing".to_string()));
        refine_skill(&mut draft, "-tag:testing").unwrap();
        assert!(!draft.tags.contains(&"testing".to_string()));
    }

    #[test]
    fn test_rename() {
        let mut draft = sample_draft();
        refine_skill(&mut draft, "name:New Name").unwrap();
        assert_eq!(draft.name, "New Name");
    }

    #[test]
    fn test_update_description() {
        let mut draft = sample_draft();
        refine_skill(&mut draft, "description:Updated description").unwrap();
        assert_eq!(draft.description, "Updated description");
    }

    #[test]
    fn test_notes_appended() {
        let mut draft = sample_draft();
        refine_skill(&mut draft, "This is a general note\nAnother note").unwrap();
        assert!(draft.content.contains("## Refinement Notes"));
        assert!(draft.content.contains("- This is a general note"));
        assert!(draft.content.contains("- Another note"));
    }

    #[test]
    fn test_mixed_feedback() {
        let mut draft = sample_draft();
        let feedback = r#"
+tag:advanced
-tag:testing
name:Improved Skill
This is additional context
More context here
"#;
        refine_skill(&mut draft, feedback).unwrap();

        assert!(draft.tags.contains(&"advanced".to_string()));
        assert!(!draft.tags.contains(&"testing".to_string()));
        assert_eq!(draft.name, "Improved Skill");
        assert!(draft.content.contains("- This is additional context"));
        assert!(draft.content.contains("- More context here"));
    }

    #[test]
    fn test_empty_feedback() {
        let mut draft = sample_draft();
        let original_content = draft.content.clone();
        refine_skill(&mut draft, "").unwrap();
        assert_eq!(draft.content, original_content, "Empty feedback should not modify");
    }
}
