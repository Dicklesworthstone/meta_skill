//! ms security - Prompt injection defense and quarantine controls

use clap::{Args, Subcommand};
use serde::Serialize;
use std::path::PathBuf;

use crate::app::AppContext;
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
    match &args.command {
        SecurityCommand::Status => status(ctx),
        SecurityCommand::Config => config(ctx),
        SecurityCommand::Version => version(ctx),
        SecurityCommand::Test { input, source } => test(ctx, input, source),
        SecurityCommand::Scan(args) => scan(ctx, args),
        SecurityCommand::Quarantine(cmd) => quarantine(ctx, cmd),
    }
}

fn status(ctx: &AppContext) -> Result<()> {
    let cfg = &ctx.config.security.acip;
    let detected = prompt_version(&cfg.prompt_path).ok().flatten();
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
            if ctx.robot_mode {
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
    if ctx.robot_mode {
        emit_json(payload)
    } else {
        let pretty = serde_json::to_string_pretty(payload)
            .map_err(|err| MsError::Config(format!("serialize output: {err}")))?;
        println!("{pretty}");
        Ok(())
    }
}
