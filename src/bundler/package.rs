//! Bundle packaging

use serde::{Deserialize, Serialize};
use crate::error::Result;

/// A skill bundle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bundle {
    pub name: String,
    pub version: String,
    pub skills: Vec<String>,
}

impl Bundle {
    /// Create a new bundle
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: "0.1.0".to_string(),
            skills: vec![],
        }
    }
    
    /// Package the bundle for distribution
    pub fn package(&self) -> Result<Vec<u8>> {
        todo!("package not implemented")
    }
}
