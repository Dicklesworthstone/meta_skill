//! Unit tests for the quality module.
//!
//! Tests cover:
//! - Quality scoring functions
//! - Weight calculations
//! - Issue detection
//! - UBS output parsing

use chrono::{Duration, Utc};

use ms::core::{BlockType, SkillBlock, SkillMetadata, SkillSection, SkillSpec};
use ms::quality::ubs::{UbsClient, UbsFinding, UbsResult, UbsSeverity};
use ms::quality::{QualityBreakdown, QualityContext, QualityIssue, QualityScorer, QualityWeights};

// ============================================================================
// Test Fixtures
// ============================================================================

fn empty_spec() -> SkillSpec {
    SkillSpec {
        format_version: SkillSpec::FORMAT_VERSION.to_string(),
        metadata: SkillMetadata {
            id: "empty".to_string(),
            name: "Empty".to_string(),
            ..Default::default()
        },
        sections: vec![],
        ..Default::default()
    }
}

fn minimal_spec() -> SkillSpec {
    SkillSpec {
        format_version: SkillSpec::FORMAT_VERSION.to_string(),
        metadata: SkillMetadata {
            id: "minimal".to_string(),
            name: "Minimal".to_string(),
            ..Default::default()
        },
        sections: vec![SkillSection {
            id: "overview".to_string(),
            title: "Overview".to_string(),
            blocks: vec![SkillBlock {
                id: "b1".to_string(),
                block_type: BlockType::Text,
                content: "Short content".to_string(),
            }],
        }],
        ..Default::default()
    }
}

fn spec_with_sections(count: usize) -> SkillSpec {
    let sections = (0..count)
        .map(|i| SkillSection {
            id: format!("section-{i}"),
            title: format!("Section {i}"),
            blocks: vec![SkillBlock {
                id: format!("b{i}"),
                block_type: BlockType::Text,
                content: "a".repeat(200),
            }],
        })
        .collect();
    SkillSpec {
        format_version: SkillSpec::FORMAT_VERSION.to_string(),
        metadata: SkillMetadata {
            id: "multi".to_string(),
            name: "Multi Section".to_string(),
            ..Default::default()
        },
        sections,
        ..Default::default()
    }
}

fn spec_with_code_example() -> SkillSpec {
    SkillSpec {
        format_version: SkillSpec::FORMAT_VERSION.to_string(),
        metadata: SkillMetadata {
            id: "with-code".to_string(),
            name: "With Code".to_string(),
            tags: vec!["rust".to_string()],
            ..Default::default()
        },
        sections: vec![
            SkillSection {
                id: "overview".to_string(),
                title: "Overview".to_string(),
                blocks: vec![SkillBlock {
                    id: "b1".to_string(),
                    block_type: BlockType::Text,
                    content: "a".repeat(500),
                }],
            },
            SkillSection {
                id: "examples".to_string(),
                title: "Examples".to_string(),
                blocks: vec![SkillBlock {
                    id: "b2".to_string(),
                    block_type: BlockType::Code,
                    content: r#"fn main() { println!("Hello"); }"#.to_string(),
                }],
            },
        ],
        ..Default::default()
    }
}

fn rich_spec() -> SkillSpec {
    SkillSpec {
        format_version: SkillSpec::FORMAT_VERSION.to_string(),
        metadata: SkillMetadata {
            id: "rich".to_string(),
            name: "Rich Skill".to_string(),
            description: "A well-documented skill".to_string(),
            tags: vec!["rust".to_string(), "testing".to_string()],
            ..Default::default()
        },
        sections: vec![
            SkillSection {
                id: "overview".to_string(),
                title: "Overview".to_string(),
                blocks: vec![SkillBlock {
                    id: "b1".to_string(),
                    block_type: BlockType::Text,
                    content: "a".repeat(800),
                }],
            },
            SkillSection {
                id: "guidelines".to_string(),
                title: "Guidelines".to_string(),
                blocks: vec![SkillBlock {
                    id: "b2".to_string(),
                    block_type: BlockType::Text,
                    content: "a".repeat(600),
                }],
            },
            SkillSection {
                id: "examples".to_string(),
                title: "Examples".to_string(),
                blocks: vec![
                    SkillBlock {
                        id: "b3".to_string(),
                        block_type: BlockType::Code,
                        content: "a".repeat(400),
                    },
                    SkillBlock {
                        id: "b4".to_string(),
                        block_type: BlockType::Code,
                        content: "b".repeat(300),
                    },
                ],
            },
        ],
        ..Default::default()
    }
}

// ============================================================================
// QualityScorer Tests
// ============================================================================

#[test]
fn quality_scorer_with_defaults() {
    let scorer = QualityScorer::with_defaults();
    let weights = &scorer.weights;

    // Default weights should sum to approximately 1.0
    let sum = weights.structure_weight
        + weights.content_weight
        + weights.evidence_weight
        + weights.usage_weight
        + weights.toolchain_weight
        + weights.freshness_weight;
    assert!(
        (sum - 1.0).abs() < 0.01,
        "Weights should sum to ~1.0, got {sum}"
    );
}

#[test]
fn quality_scorer_new_with_custom_weights() {
    let weights = QualityWeights {
        structure_weight: 0.5,
        content_weight: 0.5,
        evidence_weight: 0.0,
        usage_weight: 0.0,
        toolchain_weight: 0.0,
        freshness_weight: 0.0,
    };
    let scorer = QualityScorer::new(weights.clone());
    assert_eq!(scorer.weights.structure_weight, 0.5);
    assert_eq!(scorer.weights.content_weight, 0.5);
}

#[test]
fn quality_scorer_default_is_with_defaults() {
    let default = QualityScorer::default();
    let with_defaults = QualityScorer::with_defaults();
    assert_eq!(
        default.weights.structure_weight,
        with_defaults.weights.structure_weight
    );
}

#[test]
fn score_spec_empty_produces_low_score() {
    let scorer = QualityScorer::with_defaults();
    let score = scorer.score_spec(&empty_spec(), &QualityContext::default());

    assert!(score.overall >= 0.0 && score.overall <= 1.0);
    assert!(score.overall < 0.5, "Empty spec should have low score");
    assert!(
        score.breakdown.structure < 0.5,
        "Empty spec structure should be low"
    );
}

#[test]
fn score_spec_minimal_produces_moderate_score() {
    let scorer = QualityScorer::with_defaults();
    let score = scorer.score_spec(&minimal_spec(), &QualityContext::default());

    assert!(score.overall >= 0.0 && score.overall <= 1.0);
    // Minimal has 1 section so structure should be moderate
    assert!(score.breakdown.structure >= 0.3);
}

#[test]
fn score_spec_rich_produces_high_score() {
    let scorer = QualityScorer::with_defaults();
    let context = QualityContext {
        usage_count: Some(20),
        evidence_count: Some(5),
        modified_at: Some(Utc::now()),
        toolchain_match: true,
    };
    let score = scorer.score_spec(&rich_spec(), &context);

    assert!(score.overall >= 0.0 && score.overall <= 1.0);
    assert!(
        score.overall > 0.6,
        "Rich spec with good context should score high: {}",
        score.overall
    );
}

#[test]
fn score_spec_always_in_range() {
    let scorer = QualityScorer::with_defaults();

    for spec in [
        empty_spec(),
        minimal_spec(),
        spec_with_code_example(),
        rich_spec(),
    ] {
        for context in [
            QualityContext::default(),
            QualityContext {
                usage_count: Some(0),
                evidence_count: Some(0),
                modified_at: Some(Utc::now() - Duration::days(365)),
                toolchain_match: false,
            },
            QualityContext {
                usage_count: Some(100),
                evidence_count: Some(50),
                modified_at: Some(Utc::now()),
                toolchain_match: true,
            },
        ] {
            let score = scorer.score_spec(&spec, &context);
            assert!(
                score.overall >= 0.0 && score.overall <= 1.0,
                "Score out of range: {}",
                score.overall
            );
            assert!(score.breakdown.structure >= 0.0 && score.breakdown.structure <= 1.0);
            assert!(score.breakdown.content >= 0.0 && score.breakdown.content <= 1.0);
            assert!(score.breakdown.evidence >= 0.0 && score.breakdown.evidence <= 1.0);
            assert!(score.breakdown.usage >= 0.0 && score.breakdown.usage <= 1.0);
            assert!(score.breakdown.toolchain >= 0.0 && score.breakdown.toolchain <= 1.0);
            assert!(score.breakdown.freshness >= 0.0 && score.breakdown.freshness <= 1.0);
        }
    }
}

// ============================================================================
// Structure Score Tests
// ============================================================================

#[test]
fn structure_score_zero_sections() {
    let scorer = QualityScorer::with_defaults();
    let score = scorer.score_spec(&empty_spec(), &QualityContext::default());
    assert!(
        score.breakdown.structure < 0.2,
        "0 sections should score very low"
    );
}

#[test]
fn structure_score_one_section() {
    let scorer = QualityScorer::with_defaults();
    let score = scorer.score_spec(&spec_with_sections(1), &QualityContext::default());
    assert!(score.breakdown.structure >= 0.3 && score.breakdown.structure <= 0.5);
}

#[test]
fn structure_score_two_sections() {
    let scorer = QualityScorer::with_defaults();
    let score = scorer.score_spec(&spec_with_sections(2), &QualityContext::default());
    assert!(score.breakdown.structure >= 0.6 && score.breakdown.structure <= 0.8);
}

#[test]
fn structure_score_three_plus_sections() {
    let scorer = QualityScorer::with_defaults();
    let score = scorer.score_spec(&spec_with_sections(3), &QualityContext::default());
    assert_eq!(score.breakdown.structure, 1.0);

    let score5 = scorer.score_spec(&spec_with_sections(5), &QualityContext::default());
    assert_eq!(
        score5.breakdown.structure, 1.0,
        "3+ sections should all score 1.0"
    );
}

// ============================================================================
// Content Score Tests
// ============================================================================

#[test]
fn content_score_very_short() {
    let scorer = QualityScorer::with_defaults();
    let mut spec = minimal_spec();
    spec.sections[0].blocks[0].content = "x".repeat(50); // Very short
    let score = scorer.score_spec(&spec, &QualityContext::default());
    assert!(score.breakdown.content < 0.3);
}

#[test]
fn content_score_moderate() {
    let scorer = QualityScorer::with_defaults();
    let mut spec = minimal_spec();
    spec.sections[0].blocks[0].content = "x".repeat(500);
    let score = scorer.score_spec(&spec, &QualityContext::default());
    assert!(score.breakdown.content >= 0.5 && score.breakdown.content <= 0.7);
}

#[test]
fn content_score_long() {
    let scorer = QualityScorer::with_defaults();
    let mut spec = minimal_spec();
    spec.sections[0].blocks[0].content = "x".repeat(2500);
    let score = scorer.score_spec(&spec, &QualityContext::default());
    assert!(score.breakdown.content >= 0.9);
}

#[test]
fn content_score_code_bonus() {
    let scorer = QualityScorer::with_defaults();

    // Without code
    let mut spec_no_code = minimal_spec();
    spec_no_code.sections[0].blocks[0].content = "x".repeat(500);

    // With code block
    let spec_code = spec_with_code_example();

    let score_no_code = scorer.score_spec(&spec_no_code, &QualityContext::default());
    let score_code = scorer.score_spec(&spec_code, &QualityContext::default());

    // Code should give a bonus
    assert!(score_code.breakdown.content >= score_no_code.breakdown.content);
}

// ============================================================================
// Evidence Score Tests
// ============================================================================

#[test]
fn evidence_score_none() {
    let scorer = QualityScorer::with_defaults();
    let context = QualityContext {
        evidence_count: None,
        ..Default::default()
    };
    let score = scorer.score_spec(&minimal_spec(), &context);
    assert!(score.breakdown.evidence < 0.3);
}

#[test]
fn evidence_score_zero() {
    let scorer = QualityScorer::with_defaults();
    let context = QualityContext {
        evidence_count: Some(0),
        ..Default::default()
    };
    let score = scorer.score_spec(&minimal_spec(), &context);
    assert!(score.breakdown.evidence < 0.3);
}

#[test]
fn evidence_score_low() {
    let scorer = QualityScorer::with_defaults();
    let context = QualityContext {
        evidence_count: Some(2),
        ..Default::default()
    };
    let score = scorer.score_spec(&minimal_spec(), &context);
    assert!(score.breakdown.evidence >= 0.4 && score.breakdown.evidence <= 0.6);
}

#[test]
fn evidence_score_medium() {
    let scorer = QualityScorer::with_defaults();
    let context = QualityContext {
        evidence_count: Some(4),
        ..Default::default()
    };
    let score = scorer.score_spec(&minimal_spec(), &context);
    assert!(score.breakdown.evidence >= 0.6 && score.breakdown.evidence <= 0.8);
}

#[test]
fn evidence_score_high() {
    let scorer = QualityScorer::with_defaults();
    let context = QualityContext {
        evidence_count: Some(10),
        ..Default::default()
    };
    let score = scorer.score_spec(&minimal_spec(), &context);
    assert_eq!(score.breakdown.evidence, 1.0);
}

// ============================================================================
// Usage Score Tests
// ============================================================================

#[test]
fn usage_score_none() {
    let scorer = QualityScorer::with_defaults();
    let context = QualityContext {
        usage_count: None,
        ..Default::default()
    };
    let score = scorer.score_spec(&minimal_spec(), &context);
    assert!(score.breakdown.usage < 0.2);
}

#[test]
fn usage_score_zero() {
    let scorer = QualityScorer::with_defaults();
    let context = QualityContext {
        usage_count: Some(0),
        ..Default::default()
    };
    let score = scorer.score_spec(&minimal_spec(), &context);
    assert!(score.breakdown.usage < 0.2);
}

#[test]
fn usage_score_low() {
    let scorer = QualityScorer::with_defaults();
    let context = QualityContext {
        usage_count: Some(2),
        ..Default::default()
    };
    let score = scorer.score_spec(&minimal_spec(), &context);
    assert!(score.breakdown.usage >= 0.2 && score.breakdown.usage <= 0.4);
}

#[test]
fn usage_score_medium() {
    let scorer = QualityScorer::with_defaults();
    let context = QualityContext {
        usage_count: Some(5),
        ..Default::default()
    };
    let score = scorer.score_spec(&minimal_spec(), &context);
    assert!(score.breakdown.usage >= 0.4 && score.breakdown.usage <= 0.6);
}

#[test]
fn usage_score_high() {
    let scorer = QualityScorer::with_defaults();
    let context = QualityContext {
        usage_count: Some(8),
        ..Default::default()
    };
    let score = scorer.score_spec(&minimal_spec(), &context);
    assert!(score.breakdown.usage >= 0.7 && score.breakdown.usage <= 0.9);
}

#[test]
fn usage_score_very_high() {
    let scorer = QualityScorer::with_defaults();
    let context = QualityContext {
        usage_count: Some(20),
        ..Default::default()
    };
    let score = scorer.score_spec(&minimal_spec(), &context);
    assert_eq!(score.breakdown.usage, 1.0);
}

// ============================================================================
// Toolchain Score Tests
// ============================================================================

#[test]
fn toolchain_score_match() {
    let scorer = QualityScorer::with_defaults();
    let context = QualityContext {
        toolchain_match: true,
        ..Default::default()
    };
    let score = scorer.score_spec(&minimal_spec(), &context);
    assert_eq!(score.breakdown.toolchain, 1.0);
}

#[test]
fn toolchain_score_no_match() {
    let scorer = QualityScorer::with_defaults();
    let context = QualityContext {
        toolchain_match: false,
        ..Default::default()
    };
    let score = scorer.score_spec(&minimal_spec(), &context);
    assert!(score.breakdown.toolchain < 0.5);
}

// ============================================================================
// Freshness Score Tests
// ============================================================================

#[test]
fn freshness_score_none() {
    let scorer = QualityScorer::with_defaults();
    let context = QualityContext {
        modified_at: None,
        ..Default::default()
    };
    let score = scorer.score_spec(&minimal_spec(), &context);
    assert_eq!(score.breakdown.freshness, 0.5);
}

#[test]
fn freshness_score_very_fresh() {
    let scorer = QualityScorer::with_defaults();
    let context = QualityContext {
        modified_at: Some(Utc::now()),
        ..Default::default()
    };
    let score = scorer.score_spec(&minimal_spec(), &context);
    assert_eq!(score.breakdown.freshness, 1.0);
}

#[test]
fn freshness_score_recent() {
    let scorer = QualityScorer::with_defaults();
    let context = QualityContext {
        modified_at: Some(Utc::now() - Duration::days(15)),
        ..Default::default()
    };
    let score = scorer.score_spec(&minimal_spec(), &context);
    assert_eq!(score.breakdown.freshness, 1.0);
}

#[test]
fn freshness_score_moderate() {
    let scorer = QualityScorer::with_defaults();
    let context = QualityContext {
        modified_at: Some(Utc::now() - Duration::days(60)),
        ..Default::default()
    };
    let score = scorer.score_spec(&minimal_spec(), &context);
    assert!(score.breakdown.freshness >= 0.6 && score.breakdown.freshness <= 0.8);
}

#[test]
fn freshness_score_stale() {
    let scorer = QualityScorer::with_defaults();
    let context = QualityContext {
        modified_at: Some(Utc::now() - Duration::days(120)),
        ..Default::default()
    };
    let score = scorer.score_spec(&minimal_spec(), &context);
    assert!(score.breakdown.freshness >= 0.4 && score.breakdown.freshness <= 0.6);
}

#[test]
fn freshness_score_very_old() {
    let scorer = QualityScorer::with_defaults();
    let context = QualityContext {
        modified_at: Some(Utc::now() - Duration::days(365)),
        ..Default::default()
    };
    let score = scorer.score_spec(&minimal_spec(), &context);
    assert!(score.breakdown.freshness < 0.4);
}

// ============================================================================
// Issue Detection Tests
// ============================================================================

#[test]
fn issues_detected_for_empty_spec() {
    let scorer = QualityScorer::with_defaults();
    let score = scorer.score_spec(&empty_spec(), &QualityContext::default());

    assert!(!score.issues.is_empty(), "Empty spec should have issues");

    // Should have missing section issue
    let has_missing_section = score
        .issues
        .iter()
        .any(|i| matches!(i, QualityIssue::MissingSection(_)));
    assert!(has_missing_section, "Should detect missing section");
}

#[test]
fn issues_no_examples() {
    let scorer = QualityScorer::with_defaults();
    let score = scorer.score_spec(&minimal_spec(), &QualityContext::default());

    let has_no_examples = score
        .issues
        .iter()
        .any(|i| matches!(i, QualityIssue::NoExamples));
    assert!(has_no_examples, "Should detect no examples");
}

#[test]
fn issues_no_tags() {
    let scorer = QualityScorer::with_defaults();
    let score = scorer.score_spec(&minimal_spec(), &QualityContext::default());

    let has_no_tags = score
        .issues
        .iter()
        .any(|i| matches!(i, QualityIssue::NoTags));
    assert!(has_no_tags, "Should detect no tags");
}

#[test]
fn issues_low_evidence() {
    let scorer = QualityScorer::with_defaults();
    // evidence=0 gives score 0.2, which is < 0.5 threshold
    let context = QualityContext {
        evidence_count: Some(0),
        ..Default::default()
    };
    let score = scorer.score_spec(&minimal_spec(), &context);

    let has_low_evidence = score
        .issues
        .iter()
        .any(|i| matches!(i, QualityIssue::LowEvidence(_)));
    assert!(has_low_evidence, "Should detect low evidence");
}

#[test]
fn issues_low_usage() {
    let scorer = QualityScorer::with_defaults();
    // usage=0 gives score 0.1, which is < 0.3 threshold
    let context = QualityContext {
        usage_count: Some(0),
        ..Default::default()
    };
    let score = scorer.score_spec(&minimal_spec(), &context);

    let has_low_usage = score
        .issues
        .iter()
        .any(|i| matches!(i, QualityIssue::LowUsage(_)));
    assert!(has_low_usage, "Should detect low usage");
}

#[test]
fn no_issues_for_rich_spec_with_good_context() {
    let scorer = QualityScorer::with_defaults();
    let context = QualityContext {
        usage_count: Some(20),
        evidence_count: Some(10),
        modified_at: Some(Utc::now()),
        toolchain_match: true,
    };
    let score = scorer.score_spec(&rich_spec(), &context);

    // Rich spec with tags and examples should have few issues
    let has_no_examples = score
        .issues
        .iter()
        .any(|i| matches!(i, QualityIssue::NoExamples));
    let has_no_tags = score
        .issues
        .iter()
        .any(|i| matches!(i, QualityIssue::NoTags));
    assert!(!has_no_examples, "Rich spec has examples");
    assert!(!has_no_tags, "Rich spec has tags");
}

#[test]
fn suggestions_accompany_issues() {
    let scorer = QualityScorer::with_defaults();
    let score = scorer.score_spec(&empty_spec(), &QualityContext::default());

    // When there are issues, there should be suggestions
    if !score.issues.is_empty() {
        assert!(
            !score.suggestions.is_empty(),
            "Issues should come with suggestions"
        );
    }
}

// ============================================================================
// QualityContext Tests
// ============================================================================

#[test]
fn quality_context_default() {
    let ctx = QualityContext::default();
    assert!(ctx.usage_count.is_none());
    assert!(ctx.evidence_count.is_none());
    assert!(ctx.modified_at.is_none());
    assert!(ctx.toolchain_match);
}

// ============================================================================
// QualityWeights Tests
// ============================================================================

#[test]
fn quality_weights_default() {
    let weights = QualityWeights::default();
    assert!(weights.structure_weight > 0.0);
    assert!(weights.content_weight > 0.0);
    assert!(weights.evidence_weight > 0.0);
    assert!(weights.usage_weight > 0.0);
    assert!(weights.toolchain_weight > 0.0);
    assert!(weights.freshness_weight > 0.0);
}

#[test]
fn quality_weights_sum_to_one() {
    let weights = QualityWeights::default();
    let sum = weights.structure_weight
        + weights.content_weight
        + weights.evidence_weight
        + weights.usage_weight
        + weights.toolchain_weight
        + weights.freshness_weight;
    assert!((sum - 1.0).abs() < 0.001, "Weights should sum to 1.0");
}

// ============================================================================
// UbsClient Tests
// ============================================================================

#[test]
fn ubs_client_new_default_path() {
    let client = UbsClient::new(None);
    // Should not panic
    let _ = format!("{client:?}");
}

#[test]
fn ubs_client_new_custom_path() {
    let client = UbsClient::new(Some("/custom/path/ubs".into()));
    let debug = format!("{client:?}");
    assert!(debug.contains("/custom/path/ubs"));
}

#[test]
fn ubs_client_with_safety() {
    // Note: SafetyGate requires AppContext or env, so we just test that
    // with_safety chain method exists and returns the right type
    let client = UbsClient::new(None);
    // Just verify the type is correct - we can't easily create SafetyGate without context
    let debug = format!("{client:?}");
    assert!(debug.contains("UbsClient"));
}

// ============================================================================
// UbsResult Tests
// ============================================================================

#[test]
fn ubs_result_is_clean_zero_exit_no_findings() {
    let result = UbsResult {
        exit_code: 0,
        stdout: String::new(),
        stderr: String::new(),
        findings: vec![],
    };
    assert!(result.is_clean());
}

#[test]
fn ubs_result_not_clean_nonzero_exit() {
    let result = UbsResult {
        exit_code: 1,
        stdout: String::new(),
        stderr: String::new(),
        findings: vec![],
    };
    assert!(!result.is_clean());
}

#[test]
fn ubs_result_not_clean_with_findings() {
    let result = UbsResult {
        exit_code: 0,
        stdout: String::new(),
        stderr: String::new(),
        findings: vec![UbsFinding {
            category: "test".to_string(),
            severity: UbsSeverity::Contextual,
            file: "test.rs".into(),
            line: 1,
            column: 1,
            message: "test finding".to_string(),
            suggested_fix: None,
        }],
    };
    assert!(!result.is_clean());
}

// ============================================================================
// UbsSeverity Tests
// ============================================================================

#[test]
fn ubs_severity_debug() {
    assert!(format!("{:?}", UbsSeverity::Critical).contains("Critical"));
    assert!(format!("{:?}", UbsSeverity::Important).contains("Important"));
    assert!(format!("{:?}", UbsSeverity::Contextual).contains("Contextual"));
}

#[test]
fn ubs_severity_clone() {
    let s1 = UbsSeverity::Critical;
    let s2 = s1;
    assert!(matches!(s2, UbsSeverity::Critical));
}

// ============================================================================
// QualityBreakdown Tests
// ============================================================================

#[test]
fn quality_breakdown_debug() {
    let breakdown = QualityBreakdown {
        structure: 0.5,
        content: 0.6,
        evidence: 0.7,
        usage: 0.8,
        toolchain: 0.9,
        freshness: 1.0,
    };
    let debug = format!("{breakdown:?}");
    assert!(debug.contains("structure"));
    assert!(debug.contains("0.5"));
}

#[test]
fn quality_breakdown_clone() {
    let breakdown = QualityBreakdown {
        structure: 0.5,
        content: 0.6,
        evidence: 0.7,
        usage: 0.8,
        toolchain: 0.9,
        freshness: 1.0,
    };
    let cloned = breakdown.clone();
    assert_eq!(cloned.structure, 0.5);
    assert_eq!(cloned.freshness, 1.0);
}

// ============================================================================
// QualityIssue Tests
// ============================================================================

#[test]
fn quality_issue_variants() {
    let issues = vec![
        QualityIssue::MissingSection("overview".to_string()),
        QualityIssue::ShortContent("section".to_string(), 100),
        QualityIssue::NoExamples,
        QualityIssue::LowEvidence(2),
        QualityIssue::LowUsage(1),
        QualityIssue::NoTags,
    ];

    for issue in issues {
        let debug = format!("{issue:?}");
        assert!(!debug.is_empty());
    }
}

#[test]
fn quality_issue_clone() {
    let issue = QualityIssue::MissingSection("test".to_string());
    let cloned = issue.clone();
    if let QualityIssue::MissingSection(name) = cloned {
        assert_eq!(name, "test");
    } else {
        panic!("Clone should preserve variant");
    }
}
