//! Context-aware search ranking

/// Context for personalized search ranking
pub struct SearchContext {
    /// Current working directory
    pub cwd: Option<String>,
    /// Recent skills accessed
    pub recent_skills: Vec<String>,
    /// Project tech stack
    pub tech_stack: Vec<String>,
}

impl Default for SearchContext {
    fn default() -> Self {
        Self {
            cwd: None,
            recent_skills: vec![],
            tech_stack: vec![],
        }
    }
}
