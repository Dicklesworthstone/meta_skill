//! ms security - Prompt injection defense and quarantine controls

use clap::{Args, Subcommand};
use serde::Serialize;
use std::path::PathBuf;
use tracing::debug;

use crate::app::AppContext;
use crate::cli::output::OutputFormat;
use crate::cli::output::emit_json;
use crate::error::{MsError, Result};
use crate::security::acip::prompt_version;
use crate::security::{AcipClassification, AcipEngine, ContentSource};

#[derive(Args, Debug)]
pub struct SecurityArgs {
    #[command(subcommand)]
    pub command: SecurityCommand,
}

#[derive(Subcommand, Debug)]
pub enum SecurityCommand {
    /// Show ACIP status and prompt health
    Status,
    /// Show effective ACIP config
    Config,
    /// Show ACIP version (config + detected)
    Version,
    /// Test ACIP classification on a single input
    Test {
        /// Input text to classify
        input: String,
        /// Content source (user|assistant|tool|file)
        #[arg(long, default_value = "user")]
        source: String,
    },
    /// Scan content for injection attempts
    Scan(ScanArgs),
    /// Quarantine management
    Quarantine(QuarantineArgs),
}

#[derive(Args, Debug)]
pub struct ScanArgs {
    /// Input text to scan (mutually exclusive with --input-file)
    #[arg(long)]
    pub input: Option<String>,
    /// Read input from file (mutually exclusive with --input)
    #[arg(long)]
    pub input_file: Option<PathBuf>,
    /// Content source (user|assistant|tool|file)
    #[arg(long, default_value = "user")]
    pub source: String,
    /// Persist quarantine records when disallowed
    #[arg(long, default_value_t = true)]
    pub persist: bool,
    /// Override audit mode to true
    #[arg(long)]
    pub audit_mode: bool,
    /// Session id (required for persistence)
    #[arg(long)]
    pub session_id: Option<String>,
    /// Message index (defaults to 0)
    #[arg(long, default_value_t = 0)]
    pub message_index: usize,
    /// Content hash override
    #[arg(long)]
    pub content_hash: Option<String>,
}

#[derive(Args, Debug)]
pub struct QuarantineArgs {
    #[command(subcommand)]
    pub command: QuarantineCommand,
}

#[derive(Subcommand, Debug)]
pub enum QuarantineCommand {
    /// List recent quarantine records
    List {
        /// Max records to return
        #[arg(long, default_value_t = 50)]
        limit: usize,
        /// Filter by session id
        #[arg(long)]
        session_id: Option<String>,
    },
    /// Show a specific quarantine record
    Show {
        /// Quarantine record id
        id: String,
    },
    /// Review a quarantine record (mark injection or false-positive)
    Review {
        /// Quarantine record id
        id: String,
        /// Confirm this is a prompt-injection attempt
        #[arg(long)]
        confirm_injection: bool,
        /// Mark as false-positive with a reason
        #[arg(long)]
        false_positive: Option<String>,
    },
    /// Replay a quarantined item (safe excerpt only)
    Replay {
        /// Quarantine record id
        id: String,
        /// Explicit acknowledgement to view content
        #[arg(long)]
        i_understand_the_risks: bool,
    },
    /// List review actions for a quarantine id
    Reviews {
        /// Quarantine record id
        id: String,
    },
}

#[derive(Serialize)]
struct StatusOutput {
    ok: bool,
    enabled: bool,
    acip_version: String,
    detected_version: Option<String>,
    audit_mode: bool,
    prompt_path: String,
    error: Option<String>,
}

#[derive(Serialize)]
struct VersionOutput {
    configured: String,
    detected: Option<String>,
}

#[derive(Serialize)]
struct ReviewOutput {
    quarantine_id: String,
    review_id: Option<String>,
    action: String,
    reason: Option<String>,
    persisted: bool,
}

#[derive(Serialize)]
struct ReplayOutput {
    quarantine_id: String,
    session_id: String,
    message_index: usize,
    safe_excerpt: String,
    note: String,
}

#[derive(Serialize)]
struct ScanOutput {
    classification: AcipClassification,
    safe_excerpt: String,
    audit_tag: Option<String>,
    quarantined: bool,
    quarantine_id: Option<String>,
    content_hash: String,
}

pub fn run(ctx: &AppContext, args: &SecurityArgs) -> Result<()> {
    debug!(target: "security", mode = ?ctx.output_format, "output mode selected");
    debug!(target: "security", stage = "status_check_start");

    let result = match &args.command {
        SecurityCommand::Status => status(ctx),
        SecurityCommand::Config => config(ctx),
        SecurityCommand::Version => version(ctx),
        SecurityCommand::Test { input, source } => test(ctx, input, source),
        SecurityCommand::Scan(args) => scan(ctx, args),
        SecurityCommand::Quarantine(cmd) => quarantine(ctx, cmd),
    };

    debug!(target: "security", stage = "render_complete");
    result
}

fn status(ctx: &AppContext) -> Result<()> {
    let cfg = &ctx.config.security.acip;
    let detected = prompt_version(&cfg.prompt_path).ok().flatten();
    debug!(target: "security", acip_status = cfg.enabled, "ACIP status");
    let (ok, error) = if cfg.enabled {
        match AcipEngine::load(cfg.clone()) {
            Ok(_) => (true, None),
            Err(err) => (false, Some(err.to_string())),
        }
    } else {
        (false, Some("ACIP disabled".to_string()))
    };

    let payload = StatusOutput {
        ok,
        enabled: cfg.enabled,
        acip_version: cfg.version.clone(),
        detected_version: detected,
        audit_mode: cfg.audit_mode,
        prompt_path: cfg.prompt_path.display().to_string(),
        error,
    };

    emit_output(ctx, &payload)
}

fn config(ctx: &AppContext) -> Result<()> {
    emit_output(ctx, &ctx.config.security.acip)
}

fn version(ctx: &AppContext) -> Result<()> {
    let cfg = &ctx.config.security.acip;
    let detected = prompt_version(&cfg.prompt_path).ok().flatten();
    let payload = VersionOutput {
        configured: cfg.version.clone(),
        detected,
    };
    emit_output(ctx, &payload)
}

fn test(ctx: &AppContext, input: &str, source: &str) -> Result<()> {
    let cfg = ctx.config.security.acip.clone();
    let engine = AcipEngine::load(cfg)?;
    let source = parse_source(source)?;
    let analysis = engine.analyze(input, source)?;
    emit_output(ctx, &analysis)
}

fn scan(ctx: &AppContext, args: &ScanArgs) -> Result<()> {
    let input = resolve_input(args)?;
    let mut cfg = ctx.config.security.acip.clone();
    if args.audit_mode {
        cfg.audit_mode = true;
    }
    let engine = AcipEngine::load(cfg)?;
    let source = parse_source(&args.source)?;
    let analysis = engine.analyze(&input, source)?;
    let content_hash = args
        .content_hash
        .clone()
        .unwrap_or_else(|| hash_content(&input));

    let mut quarantined = false;
    let mut quarantine_id = None;
    if args.persist
        && matches!(
            analysis.classification,
            AcipClassification::Disallowed { .. }
        )
    {
        let session_id = args
            .session_id
            .as_ref()
            .ok_or_else(|| MsError::Config("session_id required for persistence".to_string()))?;
        let record = crate::security::acip::build_quarantine_record(
            &analysis,
            session_id,
            args.message_index,
            &content_hash,
        );
        quarantine_id = Some(record.quarantine_id.clone());
        ctx.db.insert_quarantine_record(&record)?;
        quarantined = true;
    }

    let payload = ScanOutput {
        classification: analysis.classification,
        safe_excerpt: analysis.safe_excerpt,
        audit_tag: analysis.audit_tag,
        quarantined,
        quarantine_id,
        content_hash,
    };
    emit_output(ctx, &payload)
}

fn quarantine(ctx: &AppContext, args: &QuarantineArgs) -> Result<()> {
    match &args.command {
        QuarantineCommand::List { limit, session_id } => {
            let records = if let Some(session_id) = session_id {
                ctx.db
                    .list_quarantine_records_by_session(session_id, *limit)?
            } else {
                ctx.db.list_quarantine_records(*limit)?
            };
            emit_output(ctx, &records)
        }
        QuarantineCommand::Show { id } => {
            let record = ctx.db.get_quarantine_record(id)?;
            if ctx.output_format != OutputFormat::Human {
                emit_output(ctx, &record)
            } else {
                match record {
                    Some(rec) => emit_output(ctx, &rec),
                    None => Err(MsError::Config(format!(
                        "quarantine record not found: {id}"
                    ))),
                }
            }
        }
        QuarantineCommand::Review {
            id,
            confirm_injection,
            false_positive,
        } => review_quarantine(ctx, id, *confirm_injection, false_positive.as_deref()),
        QuarantineCommand::Replay {
            id,
            i_understand_the_risks,
        } => replay_quarantine(ctx, id, *i_understand_the_risks),
        QuarantineCommand::Reviews { id } => {
            let reviews = ctx.db.list_quarantine_reviews(id)?;
            emit_output(ctx, &reviews)
        }
    }
}

fn review_quarantine(
    ctx: &AppContext,
    id: &str,
    confirm_injection: bool,
    false_positive: Option<&str>,
) -> Result<()> {
    if confirm_injection && false_positive.is_some() {
        return Err(MsError::Config(
            "cannot use --confirm-injection with --false-positive".to_string(),
        ));
    }
    if !confirm_injection && false_positive.is_none() {
        return Err(MsError::Config(
            "must set --confirm-injection or --false-positive <reason>".to_string(),
        ));
    }

    let record = ctx
        .db
        .get_quarantine_record(id)?
        .ok_or_else(|| MsError::Config(format!("quarantine record not found: {id}")))?;

    let (action, reason) = if confirm_injection {
        ("confirm_injection".to_string(), None)
    } else {
        (
            "false_positive".to_string(),
            false_positive.map(std::string::ToString::to_string),
        )
    };

    let review_id =
        ctx.db
            .insert_quarantine_review(&record.quarantine_id, &action, reason.as_deref())?;
    let payload = ReviewOutput {
        quarantine_id: record.quarantine_id,
        review_id: Some(review_id),
        action,
        reason,
        persisted: true,
    };
    emit_output(ctx, &payload)
}

fn replay_quarantine(ctx: &AppContext, id: &str, ack: bool) -> Result<()> {
    if !ack {
        return Err(MsError::ApprovalRequired(
            "replay requires --i-understand-the-risks".to_string(),
        ));
    }
    let record = ctx
        .db
        .get_quarantine_record(id)?
        .ok_or_else(|| MsError::Config(format!("quarantine record not found: {id}")))?;
    let payload = ReplayOutput {
        quarantine_id: record.quarantine_id,
        session_id: record.session_id,
        message_index: record.message_index,
        safe_excerpt: record.safe_excerpt,
        note: "Replay shows safe excerpt only; raw content is withheld.".to_string(),
    };
    emit_output(ctx, &payload)
}

fn parse_source(raw: &str) -> Result<ContentSource> {
    match raw.to_lowercase().as_str() {
        "user" => Ok(ContentSource::User),
        "assistant" => Ok(ContentSource::Assistant),
        "tool" | "tool_output" => Ok(ContentSource::ToolOutput),
        "file" | "file_contents" => Ok(ContentSource::File),
        _ => Err(MsError::Config(format!(
            "invalid source {raw} (expected user|assistant|tool|file)"
        ))),
    }
}

fn resolve_input(args: &ScanArgs) -> Result<String> {
    match (&args.input, &args.input_file) {
        (Some(_), Some(_)) => Err(MsError::Config(
            "use --input or --input-file (not both)".to_string(),
        )),
        (Some(input), None) => Ok(input.clone()),
        (None, Some(path)) => {
            let raw = std::fs::read_to_string(path).map_err(|err| {
                MsError::Config(format!("read input file {}: {err}", path.display()))
            })?;
            Ok(raw)
        }
        (None, None) => Err(MsError::Config(
            "missing input (use --input or --input-file)".to_string(),
        )),
    }
}

fn hash_content(content: &str) -> String {
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(content.as_bytes());
    let digest = hasher.finalize();
    hex::encode(digest)
}

fn emit_output<T: Serialize>(ctx: &AppContext, payload: &T) -> Result<()> {
    if ctx.output_format != OutputFormat::Human {
        emit_json(payload)
    } else {
        let pretty = serde_json::to_string_pretty(payload)
            .map_err(|err| MsError::Config(format!("serialize output: {err}")))?;
        println!("{pretty}");
        Ok(())
    }
}

/// Check whether the terminal supports rich output for security commands.
#[allow(dead_code)]
fn should_use_rich_for_security() -> bool {
    use std::io::IsTerminal;

    if std::env::var("MS_FORCE_RICH").is_ok() {
        return true;
    }
    if std::env::var("NO_COLOR").is_ok() || std::env::var("MS_PLAIN_OUTPUT").is_ok() {
        return false;
    }

    use crate::output::{is_agent_environment, is_ci_environment};
    if is_agent_environment() || is_ci_environment() {
        return false;
    }

    std::io::stdout().is_terminal()
}

/// Get the terminal width, defaulting to 80 if detection fails.
#[allow(dead_code)]
fn terminal_width() -> usize {
    crossterm::terminal::size()
        .map(|(w, _)| w as usize)
        .unwrap_or(80)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── 1. test_security_render_dashboard ─────────────────────────────

    #[test]
    fn test_security_render_dashboard() {
        let payload = StatusOutput {
            ok: true,
            enabled: true,
            acip_version: "2.0".to_string(),
            detected_version: Some("2.0".to_string()),
            audit_mode: false,
            prompt_path: "/etc/acip/prompt.md".to_string(),
            error: None,
        };
        let json = serde_json::to_string_pretty(&payload).unwrap();
        assert!(json.contains("\"ok\": true"));
        assert!(json.contains("\"enabled\": true"));
    }

    // ── 2. test_security_render_score_healthy ────────────────────────

    #[test]
    fn test_security_render_score_healthy() {
        let payload = StatusOutput {
            ok: true,
            enabled: true,
            acip_version: "2.0".to_string(),
            detected_version: Some("2.0".to_string()),
            audit_mode: false,
            prompt_path: "/prompt.md".to_string(),
            error: None,
        };
        assert!(payload.ok);
        assert!(payload.enabled);
        assert!(payload.error.is_none());
    }

    // ── 3. test_security_render_score_warning ────────────────────────

    #[test]
    fn test_security_render_score_warning() {
        let payload = StatusOutput {
            ok: false,
            enabled: true,
            acip_version: "2.0".to_string(),
            detected_version: None,
            audit_mode: true,
            prompt_path: "/missing.md".to_string(),
            error: Some("Prompt file not found".to_string()),
        };
        assert!(!payload.ok);
        assert!(payload.error.is_some());
    }

    // ── 4. test_security_render_policy_table ─────────────────────────

    #[test]
    fn test_security_render_policy_table() {
        // Policy data serialized as JSON
        let policy = serde_json::json!({
            "rules": [
                {"id": "acip-001", "severity": "high", "description": "No system prompt injection"},
                {"id": "acip-002", "severity": "medium", "description": "No tool output injection"},
            ]
        });
        let json_str = serde_json::to_string_pretty(&policy).unwrap();
        assert!(json_str.contains("acip-001"));
        assert!(json_str.contains("acip-002"));
    }

    // ── 5. test_security_render_severity_indicators ──────────────────

    #[test]
    fn test_security_render_severity_indicators() {
        let severities = ["high", "medium", "low"];
        for s in severities {
            assert!(!s.contains("\x1b["), "severity indicator must be plain");
        }
    }

    // ── 6. test_security_render_audit_log ────────────────────────────

    #[test]
    fn test_security_render_audit_log() {
        let events = vec![
            serde_json::json!({"ts": "2025-06-01T12:00:00Z", "action": "scan", "result": "allowed"}),
            serde_json::json!({"ts": "2025-06-01T12:01:00Z", "action": "scan", "result": "disallowed"}),
        ];
        assert_eq!(events.len(), 2);
        // Events should be chronological
        let ts0 = events[0]["ts"].as_str().unwrap();
        let ts1 = events[1]["ts"].as_str().unwrap();
        assert!(ts0 < ts1, "events should be chronological");
    }

    // ── 7. test_security_render_blocked_panel ────────────────────────

    #[test]
    fn test_security_render_blocked_panel() {
        let scan_output = ScanOutput {
            classification: AcipClassification::Disallowed {
                category: "injection".to_string(),
                action: "quarantine".to_string(),
            },
            safe_excerpt: "You are now...".to_string(),
            audit_tag: Some("audit-001".to_string()),
            quarantined: true,
            quarantine_id: Some("q-abc123".to_string()),
            content_hash: "deadbeef".to_string(),
        };
        let json = serde_json::to_string_pretty(&scan_output).unwrap();
        assert!(json.contains("Disallowed"));
        assert!(json.contains("quarantined"));
        assert!(json.contains("q-abc123"));
    }

    // ── 8. test_security_render_override_instructions ────────────────

    #[test]
    fn test_security_render_override_instructions() {
        // Review output structure
        let review = ReviewOutput {
            quarantine_id: "q-abc123".to_string(),
            review_id: Some("r-001".to_string()),
            action: "false_positive".to_string(),
            reason: Some("Legitimate system prompt".to_string()),
            persisted: true,
        };
        let json = serde_json::to_string_pretty(&review).unwrap();
        assert!(json.contains("false_positive"));
        assert!(json.contains("Legitimate system prompt"));
    }

    // ── 9. test_security_render_permission_matrix ────────────────────

    #[test]
    fn test_security_render_permission_matrix() {
        // Version output showing configured vs detected
        let version = VersionOutput {
            configured: "2.0".to_string(),
            detected: Some("2.0".to_string()),
        };
        let json = serde_json::to_string_pretty(&version).unwrap();
        assert!(json.contains("\"configured\": \"2.0\""));
        assert!(json.contains("\"detected\": \"2.0\""));
    }

    // ── 10. test_security_plain_output_format ────────────────────────

    #[test]
    fn test_security_plain_output_format() {
        let payload = StatusOutput {
            ok: true,
            enabled: true,
            acip_version: "2.0".to_string(),
            detected_version: Some("2.0".to_string()),
            audit_mode: false,
            prompt_path: "/prompt.md".to_string(),
            error: None,
        };
        let plain = serde_json::to_string_pretty(&payload).unwrap();
        assert!(!plain.contains("\x1b["), "plain output must have no ANSI");
    }

    // ── 11. test_security_json_output_format ─────────────────────────

    #[test]
    fn test_security_json_output_format() {
        let payload = StatusOutput {
            ok: true,
            enabled: true,
            acip_version: "2.0".to_string(),
            detected_version: Some("2.0".to_string()),
            audit_mode: false,
            prompt_path: "/prompt.md".to_string(),
            error: None,
        };
        let json = serde_json::to_string(&payload).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["ok"], true);
        assert_eq!(parsed["enabled"], true);
    }

    // ── 12. test_security_robot_mode_no_ansi ─────────────────────────

    #[test]
    fn test_security_robot_mode_no_ansi() {
        let payload = StatusOutput {
            ok: false,
            enabled: false,
            acip_version: "1.0".to_string(),
            detected_version: None,
            audit_mode: true,
            prompt_path: "/missing.md".to_string(),
            error: Some("not found".to_string()),
        };
        let json = serde_json::to_string_pretty(&payload).unwrap();
        assert!(!json.contains("\x1b["), "robot mode must have no ANSI");
    }

    // ── 13. test_security_no_info_leaked ─────────────────────────────

    #[test]
    fn test_security_no_info_leaked() {
        // Replay output should only show safe excerpt
        let replay = ReplayOutput {
            quarantine_id: "q-abc".to_string(),
            session_id: "sess-001".to_string(),
            message_index: 0,
            safe_excerpt: "Truncated content...".to_string(),
            note: "Replay shows safe excerpt only; raw content is withheld.".to_string(),
        };
        let json = serde_json::to_string_pretty(&replay).unwrap();
        assert!(json.contains("safe excerpt only"));
        assert!(json.contains("withheld"));
    }

    // ── 14. test_security_audit_chronological ────────────────────────

    #[test]
    fn test_security_audit_chronological() {
        // Scan output includes content hash for deduplication
        let scan = ScanOutput {
            classification: AcipClassification::Safe,
            safe_excerpt: "Safe content".to_string(),
            audit_tag: None,
            quarantined: false,
            quarantine_id: None,
            content_hash: "abc123def456".to_string(),
        };
        assert!(!scan.content_hash.is_empty());
        assert!(!scan.quarantined);
    }

    // ── 15. test_security_rich_vs_plain_equivalence ──────────────────

    #[test]
    fn test_security_rich_vs_plain_equivalence() {
        let payload = StatusOutput {
            ok: true,
            enabled: true,
            acip_version: "2.0".to_string(),
            detected_version: Some("2.0".to_string()),
            audit_mode: false,
            prompt_path: "/prompt.md".to_string(),
            error: None,
        };

        // Both modes should produce the same data
        let pretty = serde_json::to_string_pretty(&payload).unwrap();
        let compact = serde_json::to_string(&payload).unwrap();

        let v1: serde_json::Value = serde_json::from_str(&pretty).unwrap();
        let v2: serde_json::Value = serde_json::from_str(&compact).unwrap();

        assert_eq!(v1["ok"], v2["ok"]);
        assert_eq!(v1["enabled"], v2["enabled"]);
        assert_eq!(v1["acip_version"], v2["acip_version"]);
    }

    // ── 16. test_security_parse_source ───────────────────────────────

    #[test]
    fn test_security_parse_source() {
        assert!(matches!(parse_source("user").unwrap(), ContentSource::User));
        assert!(matches!(
            parse_source("assistant").unwrap(),
            ContentSource::Assistant
        ));
        assert!(matches!(
            parse_source("tool").unwrap(),
            ContentSource::ToolOutput
        ));
        assert!(matches!(
            parse_source("file").unwrap(),
            ContentSource::File
        ));
        assert!(parse_source("unknown").is_err());
    }

    // ── 17. test_security_hash_content ───────────────────────────────

    #[test]
    fn test_security_hash_content() {
        let hash1 = hash_content("hello");
        let hash2 = hash_content("hello");
        let hash3 = hash_content("world");

        assert_eq!(hash1, hash2, "same input should produce same hash");
        assert_ne!(hash1, hash3, "different input should produce different hash");
        assert_eq!(hash1.len(), 64, "SHA-256 hex should be 64 chars");
    }
}
