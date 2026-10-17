//! ms import - Import skills from unstructured text
//!
//! Provides a comprehensive import wizard for converting system prompts,
//! documentation, and other unstructured text into well-formed SkillSpec files.

use std::collections::HashMap;
use std::path::PathBuf;

use clap::{Args, ValueEnum};
use console::style;
use glob::glob;
use serde::Serialize;

use crate::app::AppContext;
use crate::cli::output::OutputFormat;
use crate::cli::output::{HumanLayout, emit_human, emit_json};
use crate::core::spec_lens::compile_markdown;
use crate::error::{MsError, Result};
use crate::import::{
    ContentBlock, ContentBlockType, ContentParser, GeneratedSkill, GeneratorConfig,
    ImportFormatHint, ImportHints, ImportStats, ParseStats, SkillGenerator, Suggestion,
    UnknownHandling, Warning,
};
use crate::lint::ValidationEngine;
use crate::lint::rules::all_rules;

// =============================================================================
// ARGUMENT TYPES
// =============================================================================

/// Input format for import
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum InputFormat {
    /// Auto-detect format from content and extension
    #[default]
    Auto,
    /// Markdown document
    Markdown,
    /// Plain text
    Plaintext,
    /// AGENTS.md style document
    AgentsMd,
    /// LLM system prompt
    SystemPrompt,
}

impl From<InputFormat> for ImportFormatHint {
    fn from(fmt: InputFormat) -> Self {
        match fmt {
            InputFormat::Auto => Self::Auto,
            InputFormat::Markdown => Self::Markdown,
            InputFormat::Plaintext => Self::Plaintext,
            InputFormat::AgentsMd => Self::AgentsMd,
            InputFormat::SystemPrompt => Self::SystemPrompt,
        }
    }
}

/// Output skill format
#[derive(Debug, Clone, Copy, Default, ValueEnum)]
pub enum SkillFormat {
    /// Markdown with YAML frontmatter (default)
    #[default]
    Markdown,
    /// Pure YAML
    Yaml,
    /// TOML
    Toml,
}

#[derive(Args, Debug)]
pub struct ImportArgs {
    /// Path to file or directory to import
    #[arg(value_name = "PATH")]
    pub path: PathBuf,

    /// Import all files in directory
    #[arg(long)]
    pub batch: bool,

    /// Output path (file or directory)
    #[arg(long, short)]
    pub output: Option<PathBuf>,

    /// Input format
    #[arg(long, value_enum, default_value_t = InputFormat::Auto)]
    pub format: InputFormat,

    /// Output skill format
    #[arg(long, value_enum, default_value_t = SkillFormat::Markdown)]
    pub skill_format: SkillFormat,

    /// Skip interactive mode
    #[arg(long)]
    pub non_interactive: bool,

    /// Run linting after import
    #[arg(long)]
    pub lint: bool,

    /// Auto-fix lint issues
    #[arg(long, requires = "lint")]
    pub fix: bool,

    /// Persist and index the imported skill into the local .ms store
    #[arg(long)]
    pub index: bool,

    /// Minimum confidence for classification (0.0-1.0)
    #[arg(long, default_value = "0.3")]
    pub min_confidence: f32,

    /// Skill ID hint
    #[arg(long)]
    pub id: Option<String>,

    /// Skill name hint
    #[arg(long)]
    pub name: Option<String>,

    /// Domain hint (e.g., programming, devops, security)
    #[arg(long)]
    pub domain: Option<String>,

    /// Tags to add (comma-separated)
    #[arg(long, value_delimiter = ',')]
    pub tags: Option<Vec<String>>,

    /// File pattern for batch mode (e.g., "*.md")
    #[arg(long, default_value = "*")]
    pub pattern: String,

    /// Show detailed classification signals
    #[arg(long)]
    pub verbose_signals: bool,

    /// Dry run - parse and classify without writing
    #[arg(long)]
    pub dry_run: bool,
}

// =============================================================================
// MAIN ENTRY POINT
// =============================================================================

pub fn run(ctx: &AppContext, args: &ImportArgs) -> Result<()> {
    if args.batch {
        run_batch(ctx, args)
    } else {
        run_single(ctx, args)
    }
}

// =============================================================================
// SINGLE FILE IMPORT
// =============================================================================

fn run_single(ctx: &AppContext, args: &ImportArgs) -> Result<()> {
    // Read input file
    let content = std::fs::read_to_string(&args.path)
        .map_err(|e| MsError::Config(format!("Failed to read {}: {e}", args.path.display())))?;

    // Parse content into blocks using format hint
    let parser = ContentParser::new();
    let blocks = parser.parse_with_format(&content, args.format.into());

    // Build hints from args
    let hints = build_hints(args);

    // Run classification preview (always shown in human mode)
    if ctx.output_format == OutputFormat::Human {
        show_classification_preview(&blocks, args.verbose_signals);
    }

    // Collect classification signals for robot report if requested
    let signal_summaries = if args.verbose_signals {
        Some(
            blocks
                .iter()
                .map(|b| BlockSignalSummary {
                    block_type: b.block_type.as_str().to_string(),
                    confidence: b.confidence,
                    preview: b.preview(60),
                    signals: b.signals.iter().map(|s| s.name.clone()).collect(),
                })
                .collect(),
        )
    } else {
        None
    };

    // In interactive mode, prompt for review
    if !args.non_interactive && ctx.output_format == OutputFormat::Human {
        eprintln!(
            "{} Interactive mode not available in this environment. Using non-interactive.",
            style("Note:").yellow()
        );
    }

    // Configure generator
    let gen_config = GeneratorConfig {
        min_confidence: args.min_confidence,
        unknown_handling: UnknownHandling::AddToContext,
        infer_metadata: true,
        deduplicate: true,
    };

    // Generate skill
    let generator = SkillGenerator::with_config(gen_config);
    let mut generated = generator.generate(blocks, &hints);

    // Determine output path
    let output_path = args.output.clone().unwrap_or_else(|| {
        let stem = args
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("imported");
        PathBuf::from(format!("{}.skill.md", stem))
    });

    // Show warnings and suggestions
    if ctx.output_format == OutputFormat::Human {
        show_generation_warnings(&generated.warnings);
        show_suggestions(&generated.suggestions);
    }

    // Run linting if requested
    let lint_result = if args.lint {
        let mut engine = ValidationEngine::with_defaults();
        for rule in all_rules() {
            engine.register(rule);
        }
        let mut result = engine.validate(&generated.skill);

        if args.fix && !result.passed {
            let _fix_result = engine.auto_fix(&mut generated.skill)?;
            result = engine.validate(&generated.skill);
        }

        Some(result)
    } else {
        None
    };

    // Write output and optionally index unless dry run
    let mut indexed = false;
    if !args.dry_run {
        let output_content = format_skill(&generated, args.skill_format);
        std::fs::write(&output_path, &output_content).map_err(|e| {
            MsError::Config(format!("Failed to write {}: {e}", output_path.display()))
        })?;

        if args.index {
            let tx_mgr = crate::storage::TxManager::new(
                ctx.db.clone(),
                ctx.git.clone(),
                ctx.ms_root.clone(),
            )?;
            tx_mgr.write_skill_with_layer(&generated.skill, crate::core::SkillLayer::Project)?;
            if let Ok(Some(record)) = ctx.db.get_skill(&generated.skill.metadata.id) {
                let _ = ctx.search.index_skill(&record);
                let _ = ctx.search.commit();
            }
            indexed = true;
        }
    }

    // Output based on mode
    if ctx.output_format != OutputFormat::Human {
        let report = ImportReport {
            source: args.path.display().to_string(),
            output: output_path.display().to_string(),
            stats: generated.stats.clone(),
            metadata: ImportedMetadata {
                id: generated.skill.metadata.id.clone(),
                name: generated.skill.metadata.name.clone(),
                description: generated.skill.metadata.description.clone(),
                domain: args.domain.clone(),
                tags: generated.skill.metadata.tags.clone(),
            },
            warnings: generated
                .warnings
                .iter()
                .map(|w| format_warning(w))
                .collect(),
            suggestions: generated
                .suggestions
                .iter()
                .map(|s| format_suggestion(s))
                .collect(),
            lint_passed: lint_result.as_ref().map(|r| r.passed),
            lint_errors: lint_result.as_ref().map(|r| r.error_count()),
            lint_warnings: lint_result.as_ref().map(|r| r.warning_count()),
            indexed: if args.index { Some(indexed) } else { None },
            classification_signals: signal_summaries,
        };
        emit_json(&report)?;
    } else {
        show_generation_stats(&generated.stats);
        if !args.dry_run {
            println!(
                "\n{} Created {}{}",
                style("✓").green(),
                style(output_path.display()).bold(),
                if indexed { " (indexed into store)" } else { "" }
            );
        } else {
            println!("\n{} Dry run - no file written", style("ℹ").blue());
        }
    }

    Ok(())
}

// =============================================================================
// BATCH IMPORT
// =============================================================================

fn run_batch(ctx: &AppContext, args: &ImportArgs) -> Result<()> {
    // Ensure path is a directory
    if !args.path.is_dir() {
        return Err(MsError::Config(format!(
            "Batch mode requires a directory, got: {}",
            args.path.display()
        )));
    }

    // Determine output directory
    let output_dir = args.output.clone().unwrap_or_else(|| args.path.clone());
    if !output_dir.exists() {
        std::fs::create_dir_all(&output_dir).map_err(|e| {
            MsError::Config(format!(
                "Failed to create output directory {}: {e}",
                output_dir.display()
            ))
        })?;
    }

    // Find files matching pattern
    let pattern = args.path.join(&args.pattern);
    let pattern_str = pattern.to_string_lossy();
    let files: Vec<PathBuf> = glob(&pattern_str)
        .map_err(|e| MsError::Config(format!("Invalid glob pattern: {e}")))?
        .filter_map(|r| r.ok())
        .filter(|p| p.is_file())
        .collect();

    if files.is_empty() {
        return Err(MsError::Config(format!(
            "No files matching '{}' in {}",
            args.pattern,
            args.path.display()
        )));
    }

    let mut results = Vec::new();
    let total = files.len();

    for (i, file) in files.iter().enumerate() {
        if ctx.output_format == OutputFormat::Human {
            println!("\n[{}/{}] {}", i + 1, total, style(file.display()).bold());
        }

        let result = import_single_for_batch(ctx, args, file, &output_dir);
        results.push(BatchFileResult {
            source: file.clone(),
            result,
        });
    }

    // Output summary
    if ctx.output_format != OutputFormat::Human {
        let report = BatchReport {
            total: results.len(),
            imported: results.iter().filter(|r| r.result.is_ok()).count(),
            skipped: results.iter().filter(|r| r.result.is_err()).count(),
            files: results
                .iter()
                .map(|r| BatchFileReport {
                    source: r.source.display().to_string(),
                    success: r.result.is_ok(),
                    output: r
                        .result
                        .as_ref()
                        .ok()
                        .and_then(|s| s.output_path.clone())
                        .map(|p| p.display().to_string()),
                    stats: r.result.as_ref().ok().map(|s| s.stats.clone()),
                    error: r.result.as_ref().err().map(|e| e.to_string()),
                })
                .collect(),
        };
        emit_json(&report)?;
    } else {
        let mut layout = HumanLayout::new();
        layout.section("Batch Import Summary");
        layout.kv("Total files", &results.len().to_string());
        layout.kv(
            "Imported",
            &results
                .iter()
                .filter(|r| r.result.is_ok())
                .count()
                .to_string(),
        );
        layout.kv(
            "Skipped",
            &results
                .iter()
                .filter(|r| r.result.is_err())
                .count()
                .to_string(),
        );
        emit_human(layout);
    }

    Ok(())
}

struct BatchFileResult {
    source: PathBuf,
    result: Result<BatchSuccess>,
}

struct BatchSuccess {
    output_path: Option<PathBuf>,
    stats: ImportStats,
}

fn import_single_for_batch(
    ctx: &AppContext,
    args: &ImportArgs,
    file: &PathBuf,
    output_dir: &PathBuf,
) -> Result<BatchSuccess> {
    // Read input
    let content = std::fs::read_to_string(file)
        .map_err(|e| MsError::Config(format!("Failed to read {}: {e}", file.display())))?;

    // Parse
    let parser = ContentParser::new();
    let blocks = parser.parse_with_format(&content, args.format.into());

    // Check if worth importing using ParseStats
    let parse_stats = ParseStats::from_blocks(&blocks);
    let avg_confidence = parse_stats.avg_confidence;

    if avg_confidence < args.min_confidence {
        return Err(MsError::Config(format!(
            "Low average confidence ({:.2}), skipped",
            avg_confidence
        )));
    }

    if blocks.is_empty() {
        return Err(MsError::Config("No content blocks found".to_string()));
    }

    // Build hints
    let mut hints = build_hints(args);
    hints.source_filename = file.file_stem().and_then(|s| s.to_str()).map(String::from);

    // Generate
    let gen_config = GeneratorConfig {
        min_confidence: args.min_confidence,
        unknown_handling: UnknownHandling::AddToContext,
        infer_metadata: true,
        deduplicate: true,
    };

    let generator = SkillGenerator::with_config(gen_config);
    let generated = generator.generate(blocks, &hints);

    // Determine output path
    let stem = file
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("imported");
    let output_path = output_dir.join(format!("{}.skill.md", stem));

    // Write unless dry run
    if !args.dry_run {
        let output_content = format_skill(&generated, args.skill_format);
        std::fs::write(&output_path, &output_content).map_err(|e| {
            MsError::Config(format!("Failed to write {}: {e}", output_path.display()))
        })?;

        if args.index {
            let tx_mgr = crate::storage::TxManager::new(
                ctx.db.clone(),
                ctx.git.clone(),
                ctx.ms_root.clone(),
            )?;
            tx_mgr.write_skill_with_layer(&generated.skill, crate::core::SkillLayer::Project)?;
            if let Ok(Some(record)) = ctx.db.get_skill(&generated.skill.metadata.id) {
                let _ = ctx.search.index_skill(&record);
                let _ = ctx.search.commit();
            }
        }

        if ctx.output_format == OutputFormat::Human {
            println!(
                "  {} → {} ({} rules, {} examples)",
                style("✓").green(),
                style(output_path.display()).dim(),
                generated.stats.rules_count,
                generated.stats.examples_count,
            );
        }
    }

    Ok(BatchSuccess {
        output_path: if args.dry_run {
            None
        } else {
            Some(output_path)
        },
        stats: generated.stats,
    })
}

// =============================================================================
// HELPERS
// =============================================================================

fn build_hints(args: &ImportArgs) -> ImportHints {
    ImportHints {
        suggested_id: args.id.clone(),
        suggested_name: args.name.clone(),
        source_filename: args
            .path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(String::from),
        domain: args.domain.clone(),
        tags: args.tags.clone().unwrap_or_default(),
    }
}

fn show_classification_preview(blocks: &[ContentBlock], verbose_signals: bool) {
    let mut layout = HumanLayout::new();
    layout.title("Classification Preview");

    let parse_stats = ParseStats::from_blocks(blocks);
    layout.kv(
        "High-Confidence Blocks",
        &format!(
            "{}/{} (avg {:.2})",
            parse_stats.high_confidence_count, parse_stats.total_blocks, parse_stats.avg_confidence
        ),
    );

    // Group by type
    let mut by_type: HashMap<ContentBlockType, Vec<&ContentBlock>> = HashMap::new();
    for block in blocks {
        by_type.entry(block.block_type).or_default().push(block);
    }

    // Display counts
    let type_order = [
        (ContentBlockType::Rule, "📋 Rules"),
        (ContentBlockType::Example, "💡 Examples"),
        (ContentBlockType::Pitfall, "⚠️  Pitfalls"),
        (ContentBlockType::Checklist, "✓  Checklist"),
        (ContentBlockType::Context, "📝 Context"),
        (ContentBlockType::Metadata, "🏷️  Metadata"),
        (ContentBlockType::Unknown, "❓ Unknown"),
    ];

    for (block_type, label) in type_order {
        if let Some(type_blocks) = by_type.get(&block_type) {
            let avg_conf: f32 =
                type_blocks.iter().map(|b| b.confidence).sum::<f32>() / type_blocks.len() as f32;
            layout.kv(
                label,
                &format!(
                    "{} blocks (avg confidence: {:.2})",
                    type_blocks.len(),
                    avg_conf
                ),
            );
            if verbose_signals {
                for b in type_blocks {
                    let sigs: Vec<_> = b
                        .signals
                        .iter()
                        .map(|s| format!("{} ({:.2})", s.name, s.weight))
                        .collect();
                    layout.bullet(&format!(
                        "[conf {:.2}] \"{}\" -> signals: [{}]",
                        b.confidence,
                        b.preview(45),
                        sigs.join(", ")
                    ));
                }
            }
        }
    }

    emit_human(layout);
}

fn show_generation_warnings(warnings: &[Warning]) {
    if warnings.is_empty() {
        return;
    }

    let mut layout = HumanLayout::new();
    layout.section("Warnings");

    for warning in warnings {
        layout.bullet(&format_warning(warning));
    }

    emit_human(layout);
}

fn show_suggestions(suggestions: &[Suggestion]) {
    if suggestions.is_empty() {
        return;
    }

    let mut layout = HumanLayout::new();
    layout.section("Suggestions");

    for suggestion in suggestions {
        layout.bullet(&format_suggestion(suggestion));
    }

    emit_human(layout);
}

fn show_generation_stats(stats: &ImportStats) {
    let mut layout = HumanLayout::new();
    layout.section("Generation Statistics");
    layout.kv("Total blocks", &stats.total_blocks.to_string());
    layout.kv("Rules", &stats.rules_count.to_string());
    layout.kv("Examples", &stats.examples_count.to_string());
    layout.kv("Pitfalls", &stats.pitfalls_count.to_string());
    layout.kv("Checklist", &stats.checklist_count.to_string());
    layout.kv("Context", &stats.context_count.to_string());
    layout.kv("Unknown", &stats.unknown_count.to_string());
    layout.kv("Avg confidence", &format!("{:.2}", stats.avg_confidence));
    if stats.low_confidence_skipped > 0 {
        layout.kv(
            "Skipped (low conf)",
            &stats.low_confidence_skipped.to_string(),
        );
    }
    if stats.duplicates_skipped > 0 {
        layout.kv("Skipped (dupes)", &stats.duplicates_skipped.to_string());
    }
    emit_human(layout);
}

fn format_warning(warning: &Warning) -> String {
    match warning {
        Warning::LowConfidence {
            content_preview,
            confidence,
        } => {
            format!(
                "Low confidence ({:.2}): \"{}\"",
                confidence, content_preview
            )
        }
        Warning::Discarded {
            content_preview,
            reason,
        } => {
            format!("Discarded: \"{}\" - {}", content_preview, reason)
        }
        Warning::Duplicate { content_preview } => {
            format!("Duplicate: \"{}\"", content_preview)
        }
        Warning::MissingContent { expected } => {
            format!("Missing: {}", expected)
        }
    }
}

fn format_suggestion(suggestion: &Suggestion) -> String {
    match suggestion {
        Suggestion::ClassifyBlock {
            content_preview,
            likely_types,
        } => {
            let types: Vec<_> = likely_types.iter().map(|t| t.as_str()).collect();
            format!(
                "Review classification for \"{}\": likely {}",
                content_preview,
                types.join(" or ")
            )
        }
        Suggestion::ReviewMetadata {
            field,
            inferred_value,
        } => {
            format!("Review inferred {}: \"{}\"", field, inferred_value)
        }
    }
}

fn format_skill(generated: &GeneratedSkill, format: SkillFormat) -> String {
    match format {
        SkillFormat::Markdown => compile_markdown(&generated.skill),
        SkillFormat::Yaml => {
            // For YAML output, serialize the whole spec
            serde_yaml::to_string(&generated.skill)
                .unwrap_or_else(|_| compile_markdown(&generated.skill))
        }
        SkillFormat::Toml => {
            // For TOML output
            toml::to_string_pretty(&generated.skill)
                .unwrap_or_else(|_| compile_markdown(&generated.skill))
        }
    }
}

// =============================================================================
// JSON OUTPUT TYPES
// =============================================================================

#[derive(Serialize)]
struct BlockSignalSummary {
    block_type: String,
    confidence: f32,
    preview: String,
    signals: Vec<String>,
}

#[derive(Serialize)]
struct ImportReport {
    source: String,
    output: String,
    stats: ImportStats,
    metadata: ImportedMetadata,
    warnings: Vec<String>,
    suggestions: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lint_passed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lint_errors: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lint_warnings: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    indexed: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    classification_signals: Option<Vec<BlockSignalSummary>>,
}

#[derive(Serialize)]
struct ImportedMetadata {
    id: String,
    name: String,
    description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    domain: Option<String>,
    tags: Vec<String>,
}

#[derive(Serialize)]
struct BatchReport {
    total: usize,
    imported: usize,
    skipped: usize,
    files: Vec<BatchFileReport>,
}

#[derive(Serialize)]
struct BatchFileReport {
    source: String,
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stats: Option<ImportStats>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

// =============================================================================
// TESTS
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_warning_low_confidence() {
        let warning = Warning::LowConfidence {
            content_preview: "test content".to_string(),
            confidence: 0.25,
        };
        let formatted = format_warning(&warning);
        assert!(formatted.contains("0.25"));
        assert!(formatted.contains("test content"));
    }

    #[test]
    fn test_format_warning_duplicate() {
        let warning = Warning::Duplicate {
            content_preview: "duplicate text".to_string(),
        };
        let formatted = format_warning(&warning);
        assert!(formatted.contains("Duplicate"));
        assert!(formatted.contains("duplicate text"));
    }

    #[test]
    fn test_format_suggestion_classify() {
        let suggestion = Suggestion::ClassifyBlock {
            content_preview: "ambiguous content".to_string(),
            likely_types: vec![ContentBlockType::Rule, ContentBlockType::Context],
        };
        let formatted = format_suggestion(&suggestion);
        assert!(formatted.contains("ambiguous content"));
        assert!(formatted.contains("rule"));
        assert!(formatted.contains("context"));
    }

    #[test]
    fn test_build_hints() {
        let args = ImportArgs {
            path: PathBuf::from("test.md"),
            batch: false,
            output: None,
            format: InputFormat::Auto,
            skill_format: SkillFormat::Markdown,
            non_interactive: true,
            lint: false,
            fix: false,
            index: false,
            min_confidence: 0.3,
            id: Some("my-skill".to_string()),
            name: Some("My Skill".to_string()),
            domain: Some("programming".to_string()),
            tags: Some(vec!["rust".to_string(), "testing".to_string()]),
            pattern: "*".to_string(),
            verbose_signals: false,
            dry_run: false,
        };

        let hints = build_hints(&args);
        assert_eq!(hints.suggested_id, Some("my-skill".to_string()));
        assert_eq!(hints.suggested_name, Some("My Skill".to_string()));
        assert_eq!(hints.domain, Some("programming".to_string()));
        assert!(hints.tags.contains(&"rust".to_string()));
    }
}
