//! Skill registry management

use super::skill::Skill;
use crate::error::Result;

/// Skill registry for querying and managing skills
pub struct Registry {
    // TODO: Add database reference
}

impl Registry {
    /// Get skill by ID
    pub fn get(&self, _id: &str) -> Result<Option<Skill>> {
        Ok(None)
    }

    /// List all skills
    pub fn list(&self) -> Result<Vec<Skill>> {
        Ok(vec![])
    }
}
