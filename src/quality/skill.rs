//! Skill quality scoring.

use chrono::{DateTime, Utc};

use crate::core::{BlockType, SkillSpec};

#[derive(Debug, Clone)]
pub struct QualityScorer {
    pub weights: QualityWeights,
}

impl QualityScorer {
    pub fn new(weights: QualityWeights) -> Self {
        Self { weights }
    }

    pub fn with_defaults() -> Self {
        Self::new(QualityWeights::default())
    }

    pub fn score_spec(
        &self,
        spec: &SkillSpec,
        context: &QualityContext,
    ) -> QualityScore {
        let structure = score_structure(spec);
        let content = score_content(spec);
        let evidence = score_evidence(context.evidence_count);
        let usage = score_usage(context.usage_count);
        let toolchain = if context.toolchain_match { 1.0 } else { 0.4 };
        let freshness = score_freshness(context.modified_at);

        let overall = weighted_average(
            &[
                (structure, self.weights.structure_weight),
                (content, self.weights.content_weight),
                (evidence, self.weights.evidence_weight),
                (usage, self.weights.usage_weight),
                (toolchain, self.weights.toolchain_weight),
                (freshness, self.weights.freshness_weight),
            ],
        );

        let (issues, suggestions) = collect_issues(spec, context, structure, content, evidence, usage);

        QualityScore {
            overall,
            breakdown: QualityBreakdown {
                structure,
                content,
                evidence,
                usage,
                toolchain,
                freshness,
            },
            issues,
            suggestions,
        }
    }
}

impl Default for QualityScorer {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[derive(Debug, Clone)]
pub struct QualityContext {
    pub usage_count: Option<u64>,
    pub evidence_count: Option<u64>,
    pub modified_at: Option<DateTime<Utc>>,
    pub toolchain_match: bool,
}

impl Default for QualityContext {
    fn default() -> Self {
        Self {
            usage_count: None,
            evidence_count: None,
            modified_at: None,
            toolchain_match: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct QualityWeights {
    pub structure_weight: f32,
    pub content_weight: f32,
    pub evidence_weight: f32,
    pub usage_weight: f32,
    pub toolchain_weight: f32,
    pub freshness_weight: f32,
}

impl Default for QualityWeights {
    fn default() -> Self {
        Self {
            structure_weight: 0.15,
            content_weight: 0.25,
            evidence_weight: 0.20,
            usage_weight: 0.20,
            toolchain_weight: 0.10,
            freshness_weight: 0.10,
        }
    }
}

#[derive(Debug, Clone)]
pub struct QualityScore {
    pub overall: f32,
    pub breakdown: QualityBreakdown,
    pub issues: Vec<QualityIssue>,
    pub suggestions: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct QualityBreakdown {
    pub structure: f32,
    pub content: f32,
    pub evidence: f32,
    pub usage: f32,
    pub toolchain: f32,
    pub freshness: f32,
}

#[derive(Debug, Clone)]
pub enum QualityIssue {
    MissingSection(String),
    ShortContent(String, usize),
    NoExamples,
    LowEvidence(u64),
    LowUsage(u64),
    NoTags,
}

fn score_structure(spec: &SkillSpec) -> f32 {
    let sections = spec.sections.len();
    match sections {
        0 => 0.1,
        1 => 0.4,
        2 => 0.7,
        _ => 1.0,
    }
}

fn score_content(spec: &SkillSpec) -> f32 {
    let mut chars = 0usize;
    let mut code_blocks = 0;
    for section in &spec.sections {
        for block in &section.blocks {
            chars += block.content.len();
            if block.block_type == BlockType::Code {
                code_blocks += 1;
            }
        }
    }

    let base: f32 = if chars > 2000 {
        1.0
    } else if chars > 1000 {
        0.8
    } else if chars > 400 {
        0.6
    } else if chars > 200 {
        0.4
    } else {
        0.2
    };

    let bonus: f32 = if code_blocks > 0 { 0.1 } else { 0.0 };
    (base + bonus).min(1.0)
}

fn score_evidence(count: Option<u64>) -> f32 {
    match count.unwrap_or(0) {
        0 => 0.2,
        1 | 2 => 0.5,
        3 | 4 => 0.7,
        _ => 1.0,
    }
}

fn score_usage(count: Option<u64>) -> f32 {
    match count.unwrap_or(0) {
        0 => 0.1,
        1 | 2 => 0.3,
        3..=5 => 0.5,
        6..=10 => 0.8,
        _ => 1.0,
    }
}

fn score_freshness(modified_at: Option<DateTime<Utc>>) -> f32 {
    let Some(modified_at) = modified_at else {
        return 0.5;
    };
    let age = Utc::now().signed_duration_since(modified_at).num_days().max(0) as u64;
    match age {
        0..=30 => 1.0,
        31..=90 => 0.7,
        91..=180 => 0.5,
        _ => 0.3,
    }
}

fn weighted_average(values: &[(f32, f32)]) -> f32 {
    let mut total = 0.0;
    let mut weight_sum = 0.0;
    for (value, weight) in values {
        total += value * weight;
        weight_sum += weight;
    }
    if weight_sum == 0.0 {
        0.0
    } else {
        total / weight_sum
    }
}

fn collect_issues(
    spec: &SkillSpec,
    context: &QualityContext,
    structure: f32,
    content: f32,
    evidence: f32,
    usage: f32,
) -> (Vec<QualityIssue>, Vec<String>) {
    let mut issues = Vec::new();
    let mut suggestions = Vec::new();

    if spec.sections.is_empty() || structure < 0.5 {
        issues.push(QualityIssue::MissingSection("overview".to_string()));
        suggestions.push("Add a brief overview section".to_string());
    }

    if content < 0.4 {
        issues.push(QualityIssue::ShortContent(
            "overall".to_string(),
            spec.sections.len(),
        ));
        suggestions.push("Expand core sections with more detail".to_string());
    }

    let has_examples = spec.sections.iter().any(|section| {
        section.title.to_lowercase().contains("example")
            || section.blocks.iter().any(|b| b.block_type == BlockType::Code)
    });
    if !has_examples {
        issues.push(QualityIssue::NoExamples);
        suggestions.push("Add at least one code example".to_string());
    }

    if evidence < 0.5 {
        let count = context.evidence_count.unwrap_or(0);
        issues.push(QualityIssue::LowEvidence(count));
        suggestions.push("Add provenance/evidence links".to_string());
    }

    if usage < 0.3 {
        let count = context.usage_count.unwrap_or(0);
        issues.push(QualityIssue::LowUsage(count));
    }

    if spec.metadata.tags.is_empty() {
        issues.push(QualityIssue::NoTags);
        suggestions.push("Add tags for discoverability".to_string());
    }

    (issues, suggestions)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{SkillBlock, SkillMetadata, SkillSection};

    fn minimal_spec() -> SkillSpec {
        SkillSpec {
            metadata: SkillMetadata {
                id: "test".to_string(),
                name: "Test".to_string(),
                ..Default::default()
            },
            sections: vec![SkillSection {
                id: "overview".to_string(),
                title: "Overview".to_string(),
                blocks: vec![SkillBlock {
                    id: "b1".to_string(),
                    block_type: BlockType::Text,
                    content: "Short".to_string(),
                }],
            }],
        }
    }

    #[test]
    fn scores_in_range() {
        let scorer = QualityScorer::with_defaults();
        let score = scorer.score_spec(&minimal_spec(), &QualityContext::default());
        assert!(score.overall >= 0.0 && score.overall <= 1.0);
    }
}
