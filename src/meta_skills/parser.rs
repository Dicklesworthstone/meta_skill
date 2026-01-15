use std::path::Path;

use crate::error::{MsError, Result};

use super::types::{MetaSkill, MetaSkillDoc};

pub struct MetaSkillParser;

impl MetaSkillParser {
    pub fn parse_str(content: &str, source: &Path) -> Result<MetaSkill> {
        let doc: MetaSkillDoc = toml::from_str(content).map_err(|err| {
            MsError::InvalidSkill(format!(
                "meta-skill parse error ({}): {err}",
                source.display()
            ))
        })?;
        doc.into_meta_skill()
    }

    pub fn parse_path(path: &Path) -> Result<MetaSkill> {
        let content = std::fs::read_to_string(path).map_err(|err| {
            MsError::InvalidSkill(format!("read meta-skill {}: {err}", path.display()))
        })?;
        Self::parse_str(&content, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_meta_skill_minimal() {
        let toml = r#"
            [meta_skill]
            id = "test-meta"
            name = "Test Meta"
            description = "A test meta-skill"

            [[slices]]
            skill_id = "skill-1"
            slice_ids = ["slice-a", "slice-b"]
            priority = 10
            required = true
        "#;

        let parsed = MetaSkillParser::parse_str(toml, Path::new("test.toml")).unwrap();
        assert_eq!(parsed.id, "test-meta");
        assert_eq!(parsed.slices.len(), 1);
        assert!(parsed.slices[0].required);
    }
}
