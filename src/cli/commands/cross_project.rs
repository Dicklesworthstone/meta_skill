//! ms cross-project - Cross-project learning and coverage analysis
//!
//! Provides lightweight summaries across projects using CASS session metadata.

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use clap::{Args, Subcommand};
use colored::Colorize;
use serde::Serialize;

use crate::app::AppContext;
use crate::cass::CassClient;
use crate::cass::mining::{ExtractedPattern, PatternType, extract_from_session};
use crate::cli::output::{HumanLayout, emit_json};
use crate::error::{MsError, Result};
use crate::utils::format::truncate_string;

#[derive(Args, Debug)]
pub struct CrossProjectArgs {
    #[command(subcommand)]
    pub command: CrossProjectCommand,
}

#[derive(Subcommand, Debug)]
pub enum CrossProjectCommand {
    /// Summarize sessions by project
    Summary(CrossProjectSummaryArgs),
    /// Extract patterns across projects
    Patterns(CrossProjectPatternsArgs),
    /// Show cross-project coverage gaps (patterns with weak/no skill match)
    Gaps(CrossProjectGapsArgs),
}

#[derive(Args, Debug)]
pub struct CrossProjectSummaryArgs {
    /// CASS query (default: *)
    #[arg(long, default_value = "*")]
    pub query: String,

    /// Maximum number of sessions to scan
    #[arg(long, default_value = "1000")]
    pub limit: usize,

    /// Minimum sessions per project to include
    #[arg(long, default_value = "1")]
    pub min_sessions: usize,

    /// Maximum projects to display (0 = all)
    #[arg(long, default_value = "20")]
    pub top: usize,

    /// Include sessions without a project label
    #[arg(long)]
    pub include_unknown: bool,

    /// Override path to cass binary
    #[arg(long)]
    pub cass_path: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct CrossProjectPatternsArgs {
    /// CASS query (default: *)
    #[arg(long, default_value = "*")]
    pub query: String,

    /// Maximum number of sessions to scan
    #[arg(long, default_value = "200")]
    pub limit: usize,

    /// Minimum pattern occurrences to include
    #[arg(long, default_value = "2")]
    pub min_occurrences: usize,

    /// Minimum distinct projects per pattern
    #[arg(long, default_value = "2")]
    pub min_projects: usize,

    /// Maximum patterns to display (0 = all)
    #[arg(long, default_value = "20")]
    pub top: usize,

    /// Include sessions without a project label
    #[arg(long)]
    pub include_unknown: bool,

    /// Override path to cass binary
    #[arg(long)]
    pub cass_path: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct CrossProjectGapsArgs {
    /// CASS query (default: *)
    #[arg(long, default_value = "*")]
    pub query: String,

    /// Maximum number of sessions to scan
    #[arg(long, default_value = "200")]
    pub limit: usize,

    /// Minimum pattern occurrences to include
    #[arg(long, default_value = "2")]
    pub min_occurrences: usize,

    /// Minimum distinct projects per pattern
    #[arg(long, default_value = "2")]
    pub min_projects: usize,

    /// Maximum gaps to display (0 = all)
    #[arg(long, default_value = "20")]
    pub top: usize,

    /// Include sessions without a project label
    #[arg(long)]
    pub include_unknown: bool,

    /// Maximum search results per pattern
    #[arg(long, default_value = "3")]
    pub search_limit: usize,

    /// Minimum BM25 score to consider covered
    #[arg(long, default_value = "0.0")]
    pub min_score: f32,

    /// Override path to cass binary
    #[arg(long)]
    pub cass_path: Option<PathBuf>,
}

pub fn run(ctx: &AppContext, args: &CrossProjectArgs) -> Result<()> {
    match &args.command {
        CrossProjectCommand::Summary(summary) => run_summary(ctx, summary),
        CrossProjectCommand::Patterns(patterns) => run_patterns(ctx, patterns),
        CrossProjectCommand::Gaps(gaps) => run_gaps(ctx, gaps),
    }
}

fn run_summary(ctx: &AppContext, args: &CrossProjectSummaryArgs) -> Result<()> {
    if args.limit == 0 {
        return Err(MsError::ValidationFailed(
            "limit must be greater than 0".to_string(),
        ));
    }

    let cass = cass_client(ctx, &args.cass_path);

    if !cass.is_available() {
        return Err(MsError::CassUnavailable(
            "cass binary not found or not executable".to_string(),
        ));
    }

    let matches = cass.search(&args.query, args.limit)?;
    if matches.is_empty() {
        if ctx.robot_mode {
            let report = CrossProjectReport {
                query: args.query.clone(),
                total_sessions: 0,
                total_projects: 0,
                projects: Vec::new(),
            };
            return emit_json(&report);
        }

        println!("{}", "No sessions matched the query.".dimmed());
        return Ok(());
    }

    let mut aggregates: HashMap<String, ProjectAggregate> = HashMap::new();
    for m in matches {
        let project = m
            .project
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        if project == "unknown" && !args.include_unknown {
            continue;
        }

        let entry = aggregates.entry(project).or_default();
        entry.session_count += 1;
        if let Some(ts) = m.timestamp.as_deref() {
            update_timestamp(entry, ts);
        }
    }

    let mut projects: Vec<ProjectSummary> = aggregates
        .into_iter()
        .filter(|(_, agg)| agg.session_count >= args.min_sessions)
        .map(|(project, agg)| ProjectSummary {
            project,
            sessions: agg.session_count,
            share: 0.0,
            first_seen: agg.first_seen.map(|dt| dt.to_rfc3339()),
            last_seen: agg.last_seen.map(|dt| dt.to_rfc3339()),
        })
        .collect();

    projects.sort_by(|a, b| b.sessions.cmp(&a.sessions));

    let total_sessions: usize = projects.iter().map(|p| p.sessions).sum();
    for project in &mut projects {
        project.share = if total_sessions > 0 {
            project.sessions as f64 / total_sessions as f64
        } else {
            0.0
        };
    }

    if args.top > 0 && projects.len() > args.top {
        projects.truncate(args.top);
    }

    let report = CrossProjectReport {
        query: args.query.clone(),
        total_sessions,
        total_projects: projects.len(),
        projects,
    };

    if ctx.robot_mode {
        return emit_json(&report);
    }

    print_summary(&report);
    Ok(())
}

fn run_patterns(ctx: &AppContext, args: &CrossProjectPatternsArgs) -> Result<()> {
    if args.limit == 0 {
        return Err(MsError::ValidationFailed(
            "limit must be greater than 0".to_string(),
        ));
    }
    if args.min_projects == 0 {
        return Err(MsError::ValidationFailed(
            "min-projects must be greater than 0".to_string(),
        ));
    }

    let cass = cass_client(ctx, &args.cass_path);

    if !cass.is_available() {
        return Err(MsError::CassUnavailable(
            "cass binary not found or not executable".to_string(),
        ));
    }

    let (aggregates, scanned) = collect_pattern_aggregates(
        ctx,
        &cass,
        &args.query,
        args.limit,
        args.include_unknown,
    )?;

    let mut patterns: Vec<PatternSummary> = aggregates
        .into_iter()
        .filter_map(|(_, agg)| agg.into_summary(args.min_occurrences, args.min_projects))
        .collect();

    patterns.sort_by(|a, b| b.occurrences.cmp(&a.occurrences));

    if args.top > 0 && patterns.len() > args.top {
        patterns.truncate(args.top);
    }

    let report = CrossProjectPatternsReport {
        query: args.query.clone(),
        scanned_sessions: scanned,
        patterns,
    };

    if ctx.robot_mode {
        return emit_json(&report);
    }

    print_patterns(&report);
    Ok(())
}

fn run_gaps(ctx: &AppContext, args: &CrossProjectGapsArgs) -> Result<()> {
    if args.limit == 0 {
        return Err(MsError::ValidationFailed(
            "limit must be greater than 0".to_string(),
        ));
    }
    if args.min_projects == 0 {
        return Err(MsError::ValidationFailed(
            "min-projects must be greater than 0".to_string(),
        ));
    }
    if args.search_limit == 0 {
        return Err(MsError::ValidationFailed(
            "search-limit must be greater than 0".to_string(),
        ));
    }

    let cass = cass_client(ctx, &args.cass_path);

    if !cass.is_available() {
        return Err(MsError::CassUnavailable(
            "cass binary not found or not executable".to_string(),
        ));
    }

    let (aggregates, scanned) = collect_pattern_aggregates(
        ctx,
        &cass,
        &args.query,
        args.limit,
        args.include_unknown,
    )?;

    let mut summaries: Vec<PatternSummary> = aggregates
        .into_iter()
        .filter_map(|(_, agg)| agg.into_summary(args.min_occurrences, args.min_projects))
        .collect();

    summaries.sort_by(|a, b| b.occurrences.cmp(&a.occurrences));

    let mut gaps = Vec::new();
    for summary in summaries {
        let best = if let Some(query) = build_search_query(&summary) {
            let results = ctx.search.search(&query, args.search_limit)?;
            results.first().map(|r| SkillMatch {
                skill_id: r.skill_id.clone(),
                name: r.name.clone(),
                score: r.score,
                layer: r.layer.clone(),
            })
        } else {
            None
        };

        let covered = best
            .as_ref()
            .is_some_and(|match_| match_.score >= args.min_score);

        if !covered {
            gaps.push(GapSummary {
                label: summary.label,
                occurrences: summary.occurrences,
                projects: summary.projects,
                avg_confidence: summary.avg_confidence,
                example: summary.example,
                best_match: best,
                tags: summary.tags,
            });
        }
    }

    gaps.sort_by(|a, b| b.occurrences.cmp(&a.occurrences));

    if args.top > 0 && gaps.len() > args.top {
        gaps.truncate(args.top);
    }

    let report = CrossProjectGapsReport {
        query: args.query.clone(),
        scanned_sessions: scanned,
        gaps,
    };

    if ctx.robot_mode {
        return emit_json(&report);
    }

    print_gaps(&report);
    Ok(())
}

fn print_summary(report: &CrossProjectReport) {
    let mut layout = HumanLayout::new();
    layout
        .title("Cross-Project Summary")
        .kv("Query", &report.query)
        .kv("Sessions", &report.total_sessions.to_string())
        .kv("Projects", &report.total_projects.to_string())
        .blank()
        .section("Projects")
        .push_line(format!(
            "{:32} {:>8} {:>7} {:>12} {:>12}",
            "Project".bold(),
            "Sessions".bold(),
            "Share".bold(),
            "First".bold(),
            "Last".bold()
        ))
        .push_line("-".repeat(80));

    for project in &report.projects {
        let share = format!("{:.0}%", project.share * 100.0);
        let first_seen = format_date(project.first_seen.as_deref());
        let last_seen = format_date(project.last_seen.as_deref());
        layout.push_line(format!(
            "{:32} {:>8} {:>7} {:>12} {:>12}",
            project.project,
            project.sessions,
            share,
            first_seen,
            last_seen
        ));
    }

    println!("{}", layout.build());
}

fn update_timestamp(agg: &mut ProjectAggregate, raw: &str) {
    let parsed = parse_timestamp(raw);
    let Some(ts) = parsed else {
        return;
    };

    match agg.first_seen {
        Some(current) if ts < current => agg.first_seen = Some(ts),
        None => agg.first_seen = Some(ts),
        _ => {}
    }

    match agg.last_seen {
        Some(current) if ts > current => agg.last_seen = Some(ts),
        None => agg.last_seen = Some(ts),
        _ => {}
    }
}

fn parse_timestamp(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn format_date(raw: Option<&str>) -> String {
    let Some(raw) = raw else {
        return "-".to_string();
    };
    let parsed = parse_timestamp(raw);
    parsed.map_or_else(|| raw.to_string(), |dt| dt.format("%Y-%m-%d").to_string())
}

fn cass_client(ctx: &AppContext, override_path: &Option<PathBuf>) -> CassClient {
    if let Some(cass_path) = override_path {
        CassClient::with_binary(cass_path)
    } else if let Some(ref cass_path) = ctx.config.cass.cass_path {
        CassClient::with_binary(cass_path)
    } else {
        CassClient::new()
    }
}

fn collect_pattern_aggregates(
    ctx: &AppContext,
    cass: &CassClient,
    query: &str,
    limit: usize,
    include_unknown: bool,
) -> Result<(HashMap<String, PatternAggregate>, usize)> {
    let matches = cass.search(query, limit)?;
    if matches.is_empty() {
        return Ok((HashMap::new(), 0));
    }

    let mut aggregates: HashMap<String, PatternAggregate> = HashMap::new();
    let mut scanned = 0usize;

    for m in matches {
        let project = m
            .project
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        if project == "unknown" && !include_unknown {
            continue;
        }

        let session = match cass.get_session(&m.session_id) {
            Ok(session) => session,
            Err(err) => {
                if !ctx.robot_mode {
                    eprintln!(
                        "{}: failed to load {}: {}",
                        "warning".yellow(),
                        m.session_id,
                        err
                    );
                }
                continue;
            }
        };

        scanned += 1;
        let patterns = extract_from_session(&session)
            .map_err(|err| MsError::MiningFailed(format!("extract patterns: {err}")))?;

        for pattern in patterns {
            let label = pattern_label(&pattern);
            let key = label.to_lowercase();
            let entry = aggregates
                .entry(key)
                .or_insert_with(|| PatternAggregate::new(label.clone(), pattern.tags.clone()));

            entry.total += 1;
            entry.confidence_sum += f64::from(pattern.confidence);
            *entry.projects.entry(project.clone()).or_insert(0) += 1;

            if entry.example.is_none() {
                entry.example = pattern_example(&pattern);
            }
        }
    }

    Ok((aggregates, scanned))
}

fn pattern_label(pattern: &ExtractedPattern) -> String {
    match &pattern.pattern_type {
        PatternType::CommandPattern { commands, .. } => {
            let names = command_names(commands, 3);
            if names.is_empty() {
                "Command pattern".to_string()
            } else {
                format!("Commands: {}", names.join(", "))
            }
        }
        PatternType::CodePattern { language, purpose, .. } => {
            let purpose = truncate_string(&one_line(purpose), 40);
            format!("Code ({language}): {purpose}")
        }
        PatternType::WorkflowPattern { steps, .. } => {
            let first = steps
                .first()
                .map(|s| truncate_string(&one_line(&s.action), 40));
            match first {
                Some(step) => format!("Workflow: {step}"),
                None => "Workflow pattern".to_string(),
            }
        }
        PatternType::DecisionPattern { condition, .. } => {
            format!("Decision: {}", truncate_string(&one_line(condition), 50))
        }
        PatternType::ErrorPattern { error_type, .. } => {
            format!("Error: {}", truncate_string(&one_line(error_type), 50))
        }
        PatternType::RefactorPattern {
            before_pattern,
            after_pattern,
            ..
        } => format!(
            "Refactor: {} -> {}",
            truncate_string(&one_line(before_pattern), 30),
            truncate_string(&one_line(after_pattern), 30)
        ),
        PatternType::ConfigPattern { config_type, .. } => {
            format!("Config: {}", truncate_string(&one_line(config_type), 50))
        }
        PatternType::ToolPattern { tool_name, .. } => {
            format!("Tool: {}", truncate_string(&one_line(tool_name), 50))
        }
    }
}

fn pattern_example(pattern: &ExtractedPattern) -> Option<String> {
    if let Some(desc) = &pattern.description {
        let desc = truncate_string(&one_line(desc), 80);
        if !desc.is_empty() {
            return Some(desc);
        }
    }

    match &pattern.pattern_type {
        PatternType::CommandPattern { commands, .. } => commands
            .first()
            .map(|cmd| truncate_string(&one_line(cmd), 80)),
        PatternType::CodePattern { code, .. } => Some(truncate_string(&one_line(code), 80)),
        PatternType::WorkflowPattern { steps, .. } => steps
            .first()
            .map(|step| truncate_string(&one_line(&step.description), 80)),
        PatternType::DecisionPattern { condition, .. } => {
            Some(truncate_string(&one_line(condition), 80))
        }
        PatternType::ErrorPattern { error_type, .. } => {
            Some(truncate_string(&one_line(error_type), 80))
        }
        PatternType::RefactorPattern { before_pattern, .. } => {
            Some(truncate_string(&one_line(before_pattern), 80))
        }
        PatternType::ConfigPattern { context, .. } => {
            Some(truncate_string(&one_line(context), 80))
        }
        PatternType::ToolPattern { use_cases, .. } => use_cases
            .first()
            .map(|case| truncate_string(&one_line(case), 80)),
    }
}

fn command_names(commands: &[String], limit: usize) -> Vec<String> {
    let mut names = Vec::new();
    for cmd in commands {
        if let Some(name) = cmd.split_whitespace().next() {
            let label = truncate_string(name, 24);
            if !names.contains(&label) {
                names.push(label);
            }
        }
        if names.len() >= limit {
            break;
        }
    }
    names
}

fn one_line(input: &str) -> String {
    input.replace(['\n', '\r'], " ")
}

fn sanitize_query(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    for ch in input.chars() {
        if ch.is_alphanumeric() {
            out.push(ch);
        } else if ch.is_whitespace() {
            out.push(' ');
        } else {
            out.push(' ');
        }
    }
    out
}

fn build_search_query(summary: &PatternSummary) -> Option<String> {
    let mut combined = summary.label.clone();
    if !summary.tags.is_empty() {
        combined.push(' ');
        combined.push_str(&summary.tags.join(" "));
    }
    let sanitized = sanitize_query(&combined);
    if sanitized.trim().is_empty() {
        None
    } else {
        Some(sanitized)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cass::mining::{EvidenceRef, ExtractedPattern, PatternType};

    fn make_pattern(pattern_type: PatternType) -> ExtractedPattern {
        ExtractedPattern {
            id: "test-pattern".to_string(),
            pattern_type,
            evidence: vec![EvidenceRef {
                session_id: "sess".to_string(),
                message_indices: vec![1],
                relevance: 0.5,
                snippet: None,
            }],
            confidence: 0.6,
            frequency: 1,
            tags: vec!["test".to_string()],
            description: Some("example description".to_string()),
            taint_label: None,
        }
    }

    #[test]
    fn one_line_strips_newlines() {
        let input = "hello\nworld\r\nok";
        assert_eq!(one_line(input), "hello world  ok");
    }

    #[test]
    fn command_names_dedup_and_limit() {
        let commands = vec![
            "rg foo".to_string(),
            "rg bar".to_string(),
            "cargo check".to_string(),
            "rg baz".to_string(),
        ];
        let names = command_names(&commands, 2);
        assert_eq!(names, vec!["rg".to_string(), "cargo".to_string()]);
    }

    #[test]
    fn pattern_label_command_includes_command_names() {
        let pattern = make_pattern(PatternType::CommandPattern {
            commands: vec!["rg foo".to_string(), "cargo test".to_string()],
            frequency: 2,
            contexts: vec![],
        });
        let label = pattern_label(&pattern);
        assert!(label.contains("rg"));
        assert!(label.contains("cargo"));
    }

    #[test]
    fn sanitize_query_strips_colons_and_quotes() {
        let query = "Error: \"panic\"";
        let sanitized = sanitize_query(query);
        assert!(!sanitized.contains(':'));
        assert!(!sanitized.contains('"'));
    }
}

#[derive(Debug, Default)]
struct ProjectAggregate {
    session_count: usize,
    first_seen: Option<DateTime<Utc>>,
    last_seen: Option<DateTime<Utc>>,
}

#[derive(Debug)]
struct PatternAggregate {
    label: String,
    total: usize,
    confidence_sum: f64,
    projects: HashMap<String, usize>,
    example: Option<String>,
    tags: Vec<String>,
}

impl PatternAggregate {
    fn new(label: String, tags: Vec<String>) -> Self {
        Self {
            label,
            total: 0,
            confidence_sum: 0.0,
            projects: HashMap::new(),
            example: None,
            tags,
        }
    }

    fn into_summary(self, min_occurrences: usize, min_projects: usize) -> Option<PatternSummary> {
        if self.total < min_occurrences || self.projects.len() < min_projects {
            return None;
        }

        let mut projects: Vec<ProjectCount> = self
            .projects
            .into_iter()
            .map(|(project, count)| ProjectCount { project, count })
            .collect();
        projects.sort_by(|a, b| b.count.cmp(&a.count));

        let avg_confidence = if self.total > 0 {
            self.confidence_sum / self.total as f64
        } else {
            0.0
        };

        Some(PatternSummary {
            label: self.label,
            occurrences: self.total,
            projects,
            avg_confidence,
            example: self.example,
            tags: self.tags,
        })
    }
}

#[derive(Debug, Serialize)]
struct CrossProjectReport {
    query: String,
    total_sessions: usize,
    total_projects: usize,
    projects: Vec<ProjectSummary>,
}

#[derive(Debug, Serialize)]
struct ProjectSummary {
    project: String,
    sessions: usize,
    share: f64,
    first_seen: Option<String>,
    last_seen: Option<String>,
}

#[derive(Debug, Serialize)]
struct CrossProjectPatternsReport {
    query: String,
    scanned_sessions: usize,
    patterns: Vec<PatternSummary>,
}

#[derive(Debug, Serialize)]
struct PatternSummary {
    label: String,
    occurrences: usize,
    projects: Vec<ProjectCount>,
    avg_confidence: f64,
    example: Option<String>,
    tags: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ProjectCount {
    project: String,
    count: usize,
}

#[derive(Debug, Serialize)]
struct CrossProjectGapsReport {
    query: String,
    scanned_sessions: usize,
    gaps: Vec<GapSummary>,
}

#[derive(Debug, Serialize)]
struct GapSummary {
    label: String,
    occurrences: usize,
    projects: Vec<ProjectCount>,
    avg_confidence: f64,
    example: Option<String>,
    best_match: Option<SkillMatch>,
    tags: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SkillMatch {
    skill_id: String,
    name: String,
    score: f32,
    layer: String,
}

fn print_patterns(report: &CrossProjectPatternsReport) {
    let mut layout = HumanLayout::new();
    layout
        .title("Cross-Project Patterns")
        .kv("Query", &report.query)
        .kv("Scanned sessions", &report.scanned_sessions.to_string())
        .kv("Patterns", &report.patterns.len().to_string())
        .blank()
        .section("Top Patterns")
        .push_line(format!(
            "{:42} {:>6} {:>6} {:>8} {:>18}",
            "Pattern".bold(),
            "Count".bold(),
            "Proj".bold(),
            "Conf".bold(),
            "Top projects".bold()
        ))
        .push_line("-".repeat(90));

    for pattern in &report.patterns {
        let top_projects = pattern
            .projects
            .iter()
            .take(3)
            .map(|p| format!("{}({})", p.project, p.count))
            .collect::<Vec<_>>()
            .join(", ");

        let label = truncate_string(&pattern.label, 40);
        layout.push_line(format!(
            "{:42} {:>6} {:>6} {:>8.2} {:>18}",
            label,
            pattern.occurrences,
            pattern.projects.len(),
            pattern.avg_confidence,
            truncate_string(&top_projects, 18)
        ));

        if let Some(example) = &pattern.example {
            layout.push_line(format!("  {}", truncate_string(example, 80).dimmed()));
        }
    }

    println!("{}", layout.build());
}

fn print_gaps(report: &CrossProjectGapsReport) {
    let mut layout = HumanLayout::new();
    layout
        .title("Cross-Project Gaps")
        .kv("Query", &report.query)
        .kv("Scanned sessions", &report.scanned_sessions.to_string())
        .kv("Gaps", &report.gaps.len().to_string())
        .blank()
        .section("Top Gaps")
        .push_line(format!(
            "{:38} {:>6} {:>6} {:>8} {:>18}",
            "Pattern".bold(),
            "Count".bold(),
            "Proj".bold(),
            "Conf".bold(),
            "Best match".bold()
        ))
        .push_line("-".repeat(90));

    for gap in &report.gaps {
        let best = gap
            .best_match
            .as_ref().map_or_else(|| "-".to_string(), |m| format!("{} ({:.2})", m.skill_id, m.score));
        let label = truncate_string(&gap.label, 36);
        layout.push_line(format!(
            "{:38} {:>6} {:>6} {:>8.2} {:>18}",
            label,
            gap.occurrences,
            gap.projects.len(),
            gap.avg_confidence,
            truncate_string(&best, 18)
        ));
        if let Some(example) = &gap.example {
            layout.push_line(format!("  {}", truncate_string(example, 80).dimmed()));
        }
    }

    println!("{}", layout.build());
}
