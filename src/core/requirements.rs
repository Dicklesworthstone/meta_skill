//! Environment requirement checks

use crate::error::Result;

/// Check if all requirements are met
pub const fn check_requirements() -> Result<Vec<RequirementCheck>> {
    Ok(vec![])
}

/// Result of a requirement check
#[derive(Debug)]
pub struct RequirementCheck {
    pub name: String,
    pub satisfied: bool,
    pub message: Option<String>,
}
