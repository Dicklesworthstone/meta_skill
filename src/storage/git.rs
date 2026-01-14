//! Git archive layer for skill versioning

use std::path::Path;

use git2::Repository;

use crate::error::Result;

/// Git archive for skill versioning and audit trail
pub struct GitArchive {
    repo: Repository,
}

impl GitArchive {
    /// Open or initialize git archive at the given path
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        
        // Ensure directory exists
        std::fs::create_dir_all(path)?;
        
        let repo = match Repository::open(path) {
            Ok(repo) => repo,
            Err(_) => Repository::init(path)?,
        };
        
        Ok(Self { repo })
    }
    
    /// Get a reference to the repository
    pub fn repo(&self) -> &Repository {
        &self.repo
    }
}
