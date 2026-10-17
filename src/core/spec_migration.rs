//! `SkillSpec` format migrations.

use crate::core::SkillSpec;
use crate::error::{MsError, Result};

pub struct SpecMigration {
    pub from: &'static str,
    pub to: &'static str,
    pub apply: fn(SkillSpec) -> Result<SkillSpec>,
}

pub struct MigrationRegistry {
    migrations: &'static [SpecMigration],
}

const DEFAULT_MIGRATIONS: &[SpecMigration] = &[
    SpecMigration {
        from: "0.1",
        to: "0.9",
        apply: migrate_0_1_to_0_9,
    },
    SpecMigration {
        from: "0.1.0",
        to: "0.9",
        apply: migrate_0_1_to_0_9,
    },
    SpecMigration {
        from: "0.2",
        to: "0.9",
        apply: migrate_0_1_to_0_9,
    },
    SpecMigration {
        from: "0.9",
        to: "1.0",
        apply: migrate_0_9_to_1_0,
    },
    SpecMigration {
        from: "1",
        to: "1.0",
        apply: migrate_0_9_to_1_0,
    },
    SpecMigration {
        from: "v1",
        to: "1.0",
        apply: migrate_0_9_to_1_0,
    },
    SpecMigration {
        from: "v1.0",
        to: "1.0",
        apply: migrate_0_9_to_1_0,
    },
];

fn slugify_identifier(input: &str) -> String {
    let mut slug = String::with_capacity(input.len());
    let mut last_dash = false;
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash && !slug.is_empty() {
            slug.push('-');
            last_dash = true;
        }
    }
    slug.trim_matches('-').to_string()
}

#[allow(clippy::unnecessary_wraps)]
fn migrate_0_1_to_0_9(mut spec: SkillSpec) -> Result<SkillSpec> {
    if spec.metadata.version.trim().is_empty() {
        spec.metadata.version = "0.1.0".to_string();
    }
    for (sec_idx, section) in spec.sections.iter_mut().enumerate() {
        if section.id.trim().is_empty() {
            let slug = slugify_identifier(&section.title);
            section.id = if slug.is_empty() {
                format!("section-{}", sec_idx + 1)
            } else {
                slug
            };
        }
        for (blk_idx, block) in section.blocks.iter_mut().enumerate() {
            if block.id.trim().is_empty() {
                block.id = format!("{}-block-{}", section.id, blk_idx + 1);
            }
        }
    }
    Ok(spec)
}

#[allow(clippy::unnecessary_wraps)]
fn migrate_0_9_to_1_0(mut spec: SkillSpec) -> Result<SkillSpec> {
    if spec.metadata.id.trim().is_empty() && !spec.metadata.name.trim().is_empty() {
        spec.metadata.id = slugify_identifier(&spec.metadata.name);
    }
    let mut seen_tags = std::collections::HashSet::new();
    spec.metadata.tags = spec
        .metadata
        .tags
        .into_iter()
        .map(|t| t.trim().to_ascii_lowercase())
        .filter(|t| !t.is_empty() && seen_tags.insert(t.clone()))
        .collect();
    Ok(spec)
}

impl MigrationRegistry {
    #[must_use]
    pub const fn with_defaults() -> Self {
        Self {
            migrations: DEFAULT_MIGRATIONS,
        }
    }

    #[must_use]
    pub fn find(&self, from: &str) -> Option<&SpecMigration> {
        self.migrations.iter().find(|m| m.from == from)
    }
}

pub fn migrate_spec(mut spec: SkillSpec) -> Result<(SkillSpec, bool)> {
    let target = SkillSpec::FORMAT_VERSION;
    let mut changed = false;

    if spec.format_version.trim().is_empty() {
        spec.format_version = target.to_string();
        return Ok((spec, true));
    }

    if spec.format_version == target {
        return Ok((spec, false));
    }

    let registry = MigrationRegistry::with_defaults();

    while spec.format_version != target {
        let current = spec.format_version.clone();
        let migration = registry.find(&current).ok_or_else(|| {
            MsError::NotFound(format!("no migration path from {current} to {target}"))
        })?;

        let mut updated = (migration.apply)(spec)?;
        updated.format_version = migration.to.to_string();
        spec = updated;
        changed = true;

        if spec.format_version == current {
            return Err(MsError::ValidationFailed(format!(
                "migration {current} -> {target} did not advance format version"
            )));
        }
    }

    Ok((spec, changed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{BlockType, SkillBlock, SkillMetadata, SkillSection};

    fn base_spec() -> SkillSpec {
        SkillSpec {
            format_version: SkillSpec::FORMAT_VERSION.to_string(),
            metadata: SkillMetadata {
                id: "test".to_string(),
                name: "Test".to_string(),
                version: "0.1.0".to_string(),
                description: String::new(),
                ..Default::default()
            },
            sections: vec![SkillSection {
                id: "intro".to_string(),
                title: "Intro".to_string(),
                blocks: vec![],
            }],
            ..Default::default()
        }
    }

    #[test]
    fn migrate_current_version_noop() {
        let spec = base_spec();
        let (migrated, changed) = migrate_spec(spec.clone()).unwrap();
        assert!(!changed);
        assert_eq!(migrated.format_version, spec.format_version);
    }

    #[test]
    fn migrate_empty_version_sets_default() {
        let mut spec = base_spec();
        spec.format_version = String::new();
        let (migrated, changed) = migrate_spec(spec).unwrap();
        assert!(changed);
        assert_eq!(migrated.format_version, SkillSpec::FORMAT_VERSION);
    }

    #[test]
    fn migrate_from_0_1_to_1_0_chains_steps() {
        let mut spec = base_spec();
        spec.format_version = "0.1".to_string();
        spec.metadata.id = String::new();
        spec.metadata.name = "My Legacy Skill".to_string();
        spec.metadata.version = String::new();
        spec.metadata.tags = vec!["Rust".to_string(), " rust ".to_string(), "CLI".to_string()];
        spec.sections[0].id = String::new();
        spec.sections[0].title = "Getting Started".to_string();
        spec.sections[0].blocks.push(SkillBlock {
            id: String::new(),
            block_type: BlockType::Text,
            content: "Hello".to_string(),
        });

        let (migrated, changed) = migrate_spec(spec).unwrap();
        assert!(changed);
        assert_eq!(migrated.format_version, SkillSpec::FORMAT_VERSION);
        assert_eq!(migrated.metadata.id, "my-legacy-skill");
        assert_eq!(migrated.metadata.version, "0.1.0");
        assert_eq!(
            migrated.metadata.tags,
            vec!["rust".to_string(), "cli".to_string()]
        );
        assert_eq!(migrated.sections[0].id, "getting-started");
        assert_eq!(migrated.sections[0].blocks[0].id, "getting-started-block-1");
    }

    #[test]
    fn migrate_unknown_version_returns_error() {
        let mut spec = base_spec();
        spec.format_version = "99.9".to_string();
        assert!(migrate_spec(spec).is_err());
    }
}
