//! Git archive layer for skill versioning

use std::fs;
use std::path::{Path, PathBuf};

use git2::{Commit, ErrorCode, Oid, Repository, Signature};
use serde::{Deserialize, Serialize};

use crate::core::{SkillMetadata, SkillSpec};
use crate::error::{MsError, Result};

/// Git archive for skill versioning and audit trail
pub struct GitArchive {
    repo: Repository,
    root: PathBuf,
    signature: Signature<'static>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillCommit {
    pub oid: String,
    pub message: String,
}

impl GitArchive {
    /// Open existing archive or initialize new one
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let root = path.as_ref().to_path_buf();
        fs::create_dir_all(&root)?;

        let repo = match Repository::open(&root) {
            Ok(repo) => repo,
            Err(_) => Repository::init(&root)?,
        };

        Self::ensure_structure(&root)?;

        let signature = repo
            .signature()
            .or_else(|_| Signature::now("ms", "ms@localhost"))
            .map_err(|err| MsError::Git(err))?;

        Ok(Self {
            repo,
            root,
            signature,
        })
    }

    /// Get a reference to the repository
    pub fn repo(&self) -> &Repository {
        &self.repo
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Get the path to a skill directory in the archive
    ///
    /// Returns None if skill_id contains path traversal sequences.
    pub fn skill_path(&self, skill_id: &str) -> Option<PathBuf> {
        // Prevent path traversal attacks
        if skill_id.contains("..") || skill_id.contains('/') || skill_id.contains('\\') {
            return None;
        }
        Some(self.root.join("skills/by-id").join(skill_id))
    }

    /// Check if a skill exists in the archive (has spec file)
    pub fn skill_exists(&self, skill_id: &str) -> bool {
        self.skill_path(skill_id)
            .map(|p| p.join("skill.spec.json").exists())
            .unwrap_or(false)
    }

    pub fn list_skill_ids(&self) -> Result<Vec<String>> {
        let base = self.root.join("skills/by-id");
        if !base.exists() {
            return Ok(Vec::new());
        }
        let mut ids = Vec::new();
        for entry in fs::read_dir(base)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    ids.push(name.to_string());
                }
            }
        }
        ids.sort();
        Ok(ids)
    }

    pub fn recent_commits(&self, limit: usize) -> Result<Vec<SkillCommit>> {
        if limit == 0 {
            return Ok(Vec::new());
        }

        let mut revwalk = self.repo.revwalk()?;
        match self.repo.head() {
            Ok(head) => {
                if let Some(oid) = head.target() {
                    revwalk.push(oid)?;
                } else {
                    return Ok(Vec::new());
                }
            }
            Err(err) if err.code() == ErrorCode::UnbornBranch => return Ok(Vec::new()),
            Err(err) if err.code() == ErrorCode::NotFound => return Ok(Vec::new()),
            Err(err) => return Err(MsError::Git(err)),
        }

        let mut commits = Vec::new();
        for oid in revwalk.take(limit) {
            let oid = oid.map_err(MsError::Git)?;
            let commit = self.repo.find_commit(oid)?;
            let message = commit.summary().unwrap_or_default().to_string();
            commits.push(SkillCommit {
                oid: oid.to_string(),
                message,
            });
        }

        Ok(commits)
    }

    /// Write a skill spec + compiled markdown into the archive and commit.
    pub fn write_skill(&self, spec: &SkillSpec) -> Result<SkillCommit> {
        let skill_id = spec.metadata.id.trim();
        if skill_id.is_empty() {
            return Err(MsError::ValidationFailed(
                "skill id must be non-empty".to_string(),
            ));
        }
        // Prevent path traversal attacks
        if skill_id.contains("..") || skill_id.contains('/') || skill_id.contains('\\') {
            return Err(MsError::ValidationFailed(
                "skill id must not contain path traversal sequences".to_string(),
            ));
        }

        let skill_dir = self.root.join("skills/by-id").join(skill_id);
        fs::create_dir_all(&skill_dir)?;
        fs::create_dir_all(skill_dir.join("evidence"))?;
        fs::create_dir_all(skill_dir.join("tests"))?;

        let metadata_path = skill_dir.join("metadata.yaml");
        let spec_path = skill_dir.join("skill.spec.json");
        let lens_path = skill_dir.join("spec.lens.json");
        let markdown_path = skill_dir.join("SKILL.md");
        let evidence_path = skill_dir.join("evidence.json");
        let slices_path = skill_dir.join("slices.json");
        let usage_log_path = skill_dir.join("usage-log.jsonl");

        write_string(&metadata_path, &serde_yaml::to_string(&spec.metadata)?)?;
        write_string(&spec_path, &serde_json::to_string_pretty(spec)?)?;
        write_string(&markdown_path, &render_skill_markdown(spec))?;
        write_string(&lens_path, "{}")?;
        write_string(&evidence_path, "{}")?;
        write_string(&slices_path, "[]")?;
        ensure_file(&usage_log_path)?;

        let mut index = self.repo.index()?;
        add_path(&mut index, &self.root, &metadata_path)?;
        add_path(&mut index, &self.root, &spec_path)?;
        add_path(&mut index, &self.root, &lens_path)?;
        add_path(&mut index, &self.root, &markdown_path)?;
        add_path(&mut index, &self.root, &evidence_path)?;
        add_path(&mut index, &self.root, &slices_path)?;
        add_path(&mut index, &self.root, &usage_log_path)?;
        index.write()?;

        let tree_id = index.write_tree()?;
        let tree = self.repo.find_tree(tree_id)?;
        let message = format!("Update skill {}", skill_id);
        let oid = commit_with_parents(&self.repo, &self.signature, &tree, &message)?;

        Ok(SkillCommit {
            oid: oid.to_string(),
            message,
        })
    }

    /// Read a skill spec from the archive.
    pub fn read_skill(&self, skill_id: &str) -> Result<SkillSpec> {
        let skill_path = self.skill_path(skill_id).ok_or_else(|| {
            MsError::ValidationFailed("skill id contains path traversal sequences".to_string())
        })?;
        let spec_path = skill_path.join("skill.spec.json");
        let contents = fs::read_to_string(spec_path)?;
        let spec = serde_json::from_str(&contents)?;
        Ok(spec)
    }

    /// Read skill metadata from the archive.
    pub fn read_metadata(&self, skill_id: &str) -> Result<SkillMetadata> {
        let skill_path = self.skill_path(skill_id).ok_or_else(|| {
            MsError::ValidationFailed("skill id contains path traversal sequences".to_string())
        })?;
        let metadata_path = skill_path.join("metadata.yaml");
        let contents = fs::read_to_string(metadata_path)?;
        let metadata = serde_yaml::from_str(&contents)?;
        Ok(metadata)
    }

    /// Delete a skill directory and commit the removal.
    pub fn delete_skill(&self, skill_id: &str) -> Result<SkillCommit> {
        let skill_dir = self.skill_path(skill_id).ok_or_else(|| {
            MsError::ValidationFailed("skill id contains path traversal sequences".to_string())
        })?;
        if !skill_dir.exists() {
            return Err(MsError::SkillNotFound(skill_id.to_string()));
        }
        let tombstone_dir = tombstone_skill_dir(&self.root, &skill_dir)?;

        let mut index = self.repo.index()?;
        let rel = skill_dir.strip_prefix(&self.root).map_err(|_| {
            MsError::ValidationFailed("skill path not under archive root".to_string())
        })?;
        index.remove_dir(rel, 0)?;
        add_dir_recursive(&mut index, &self.root, &tombstone_dir)?;
        index.write()?;

        let tree_id = index.write_tree()?;
        let tree = self.repo.find_tree(tree_id)?;
        let message = format!("Tombstone skill {}", skill_id);
        let oid = commit_with_parents(&self.repo, &self.signature, &tree, &message)?;

        Ok(SkillCommit {
            oid: oid.to_string(),
            message,
        })
    }

    fn ensure_structure(root: &Path) -> Result<()> {
        fs::create_dir_all(root.join("skills/by-id"))?;
        fs::create_dir_all(root.join("skills/by-source"))?;
        fs::create_dir_all(root.join("builds"))?;
        fs::create_dir_all(root.join("bundles/published"))?;
        let readme = root.join("README.md");
        if !readme.exists() {
            write_string(
                &readme,
                "# ms archive\n\nThis directory contains the ms skill archive.\n",
            )?;
        }
        Ok(())
    }
}

fn tombstone_skill_dir(root: &Path, skill_dir: &Path) -> Result<PathBuf> {
    let tombstones = root.join("tombstones");
    fs::create_dir_all(&tombstones)?;
    let name = skill_dir
        .file_name()
        .ok_or_else(|| MsError::ValidationFailed("invalid skill directory".to_string()))?;
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%S");
    let tombstone = tombstones.join(format!("{}_{}", name.to_string_lossy(), stamp));
    fs::rename(skill_dir, &tombstone)?;
    Ok(tombstone)
}

fn render_skill_markdown(spec: &SkillSpec) -> String {
    let mut out = String::new();
    out.push_str("# ");
    out.push_str(&spec.metadata.name);
    out.push_str("\n\n");
    if !spec.metadata.description.is_empty() {
        out.push_str(&spec.metadata.description);
        out.push_str("\n\n");
    }

    for section in &spec.sections {
        out.push_str("## ");
        out.push_str(&section.title);
        out.push_str("\n\n");
        for block in &section.blocks {
            match block.block_type {
                crate::core::BlockType::Code => {
                    let content = block.content.trim_end();
                    if content.trim_start().starts_with("```") {
                        out.push_str(content);
                        out.push_str("\n\n");
                    } else {
                        out.push_str("```\n");
                        out.push_str(content);
                        out.push_str("\n```\n\n");
                    }
                }
                _ => {
                    out.push_str(&block.content);
                    out.push_str("\n\n");
                }
            }
        }
    }
    out
}

fn write_string(path: &Path, contents: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    Ok(())
}

fn ensure_file(path: &Path) -> Result<()> {
    if !path.exists() {
        write_string(path, "")?;
    }
    Ok(())
}

fn add_path(index: &mut git2::Index, root: &Path, path: &Path) -> Result<()> {
    let rel = path
        .strip_prefix(root)
        .map_err(|_| MsError::ValidationFailed("path not under archive root".to_string()))?;
    index.add_path(rel)?;
    Ok(())
}

fn add_dir_recursive(index: &mut git2::Index, root: &Path, dir: &Path) -> Result<()> {
    for entry in walkdir::WalkDir::new(dir).into_iter() {
        let entry = entry.map_err(|err| MsError::Config(format!("walk tombstone: {err}")))?;
        if entry.file_type().is_file() {
            add_path(index, root, entry.path())?;
        }
    }
    Ok(())
}

fn commit_with_parents(
    repo: &Repository,
    signature: &Signature,
    tree: &git2::Tree<'_>,
    message: &str,
) -> Result<Oid> {
    let parents = match repo.head() {
        Ok(head) => {
            if let Some(oid) = head.target() {
                vec![repo.find_commit(oid)?]
            } else {
                Vec::new()
            }
        }
        Err(err) if err.code() == ErrorCode::UnbornBranch => Vec::new(),
        Err(err) if err.code() == ErrorCode::NotFound => Vec::new(),
        Err(err) => return Err(MsError::Git(err)),
    };

    let parent_refs: Vec<&Commit<'_>> = parents.iter().collect();
    let oid = repo.commit(
        Some("HEAD"),
        signature,
        signature,
        message,
        tree,
        &parent_refs,
    )?;
    Ok(oid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn sample_spec(id: &str) -> SkillSpec {
        SkillSpec {
            format_version: SkillSpec::FORMAT_VERSION.to_string(),
            metadata: crate::core::SkillMetadata {
                id: id.to_string(),
                name: "Sample Skill".to_string(),
                version: "1.0.0".to_string(),
                description: "Sample description".to_string(),
                ..Default::default()
            },
            sections: vec![crate::core::SkillSection {
                id: "intro".to_string(),
                title: "Introduction".to_string(),
                blocks: vec![crate::core::SkillBlock {
                    id: "block-1".to_string(),
                    block_type: crate::core::BlockType::Text,
                    content: "Hello".to_string(),
                }],
            }],
        }
    }

    #[test]
    fn test_archive_init() {
        let dir = tempdir().unwrap();
        let archive = GitArchive::open(dir.path()).unwrap();

        assert!(dir.path().join(".git").exists());
        assert!(archive.root().join("skills/by-id").exists());
        assert!(archive.root().join("builds").exists());
        assert!(archive.root().join("README.md").exists());
    }

    #[test]
    fn test_skill_write_read() {
        let dir = tempdir().unwrap();
        let archive = GitArchive::open(dir.path()).unwrap();

        let spec = sample_spec("test-skill");
        archive.write_skill(&spec).unwrap();

        let skill_dir = dir.path().join("skills/by-id/test-skill");
        assert!(skill_dir.join("skill.spec.json").exists());
        assert!(skill_dir.join("SKILL.md").exists());
        assert!(skill_dir.join("metadata.yaml").exists());

        let read_spec = archive.read_skill("test-skill").unwrap();
        assert_eq!(read_spec.metadata.id, "test-skill");

        let metadata = archive.read_metadata("test-skill").unwrap();
        assert_eq!(metadata.id, "test-skill");
        assert!(skill_dir.join("evidence").exists());
        assert!(skill_dir.join("tests").exists());
        assert!(skill_dir.join("usage-log.jsonl").exists());
    }

    #[test]
    fn test_git_history() {
        let dir = tempdir().unwrap();
        let archive = GitArchive::open(dir.path()).unwrap();

        let spec = sample_spec("hist-skill");
        let commit = archive.write_skill(&spec).unwrap();

        assert!(!commit.oid.is_empty());
        assert!(commit.message.contains("hist-skill"));
    }

    #[test]
    fn test_skill_delete() {
        let dir = tempdir().unwrap();
        let archive = GitArchive::open(dir.path()).unwrap();

        let spec = sample_spec("delete-skill");
        archive.write_skill(&spec).unwrap();
        let commit = archive.delete_skill("delete-skill").unwrap();
        assert!(commit.message.contains("delete-skill"));
        assert!(!dir.path().join("skills/by-id/delete-skill").exists());
    }

    #[test]
    fn test_list_skill_ids() {
        let dir = tempdir().unwrap();
        let archive = GitArchive::open(dir.path()).unwrap();
        let spec_a = sample_spec("alpha");
        let spec_b = sample_spec("beta");
        archive.write_skill(&spec_b).unwrap();
        archive.write_skill(&spec_a).unwrap();

        let ids = archive.list_skill_ids().unwrap();
        assert_eq!(ids, vec!["alpha".to_string(), "beta".to_string()]);
    }

    #[test]
    fn test_recent_commits() {
        let dir = tempdir().unwrap();
        let archive = GitArchive::open(dir.path()).unwrap();
        let spec = sample_spec("recent-skill");
        archive.write_skill(&spec).unwrap();

        let commits = archive.recent_commits(5).unwrap();
        assert!(!commits.is_empty());
        assert!(commits[0].message.contains("recent-skill"));
    }

    #[test]
    fn test_path_traversal_blocked() {
        let dir = tempdir().unwrap();
        let archive = GitArchive::open(dir.path()).unwrap();

        // Test that path traversal is blocked in skill_path
        assert!(archive.skill_path("../etc/passwd").is_none());
        assert!(archive.skill_path("foo/../bar").is_none());
        assert!(archive.skill_path("foo/bar").is_none());
        assert!(archive.skill_path("foo\\bar").is_none());

        // Test valid skill_id passes
        assert!(archive.skill_path("valid-skill").is_some());
        assert!(archive.skill_path("skill_123").is_some());

        // Test that write_skill rejects path traversal
        let mut spec = sample_spec("../malicious");
        let err = archive.write_skill(&spec).unwrap_err();
        assert!(err.to_string().contains("path traversal"));

        // Test that read_skill rejects path traversal
        spec.metadata.id = "valid-skill".to_string();
        archive.write_skill(&spec).unwrap();
        let err = archive.read_skill("../malicious").unwrap_err();
        assert!(err.to_string().contains("path traversal"));

        // Test that skill_exists returns false for path traversal
        assert!(!archive.skill_exists("../etc/passwd"));
    }
}
