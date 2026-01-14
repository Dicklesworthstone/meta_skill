//! Skill validation

use crate::error::{MsError, Result};
use super::skill::SkillSpec;

/// Validate a skill specification
pub fn validate(spec: &SkillSpec) -> Result<Vec<ValidationWarning>> {
    let mut warnings = vec![];

    if spec.metadata.id.is_empty() {
        return Err(MsError::ValidationFailed("skill ID is required".into()));
    }

    if spec.metadata.name.is_empty() {
        return Err(MsError::ValidationFailed("skill name is required".into()));
    }

    if spec.metadata.description.is_empty() {
        warnings.push(ValidationWarning {
            field: "description".to_string(),
            message: "skill should have a description".to_string(),
        });
    }

    if spec.metadata.tags.is_empty() {
        warnings.push(ValidationWarning {
            field: "tags".to_string(),
            message: "skill should have at least one tag".to_string(),
        });
    }

    Ok(warnings)
}

/// A validation warning (not an error)
#[derive(Debug)]
pub struct ValidationWarning {
    pub field: String,
    pub message: String,
}
