//! `SkillSpec` format migrations.

use crate::core::SkillSpec;
use crate::error::{MsError, Result};

pub struct SpecMigration {
    pub from: &'static str,
    pub to: &'static str,
    pub apply: fn(SkillSpec) -> Result<SkillSpec>,
}

pub struct MigrationRegistry {
    migrations: Vec<SpecMigration>,
}

impl MigrationRegistry {
    #[must_use] 
    pub const fn with_defaults() -> Self {
        Self {
            migrations: Vec::new(),
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
    use crate::core::{SkillMetadata, SkillSection};

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
}
