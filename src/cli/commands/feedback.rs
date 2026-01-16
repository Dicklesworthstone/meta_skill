//! ms feedback - Record and inspect skill feedback.

use clap::{Args, Subcommand};

use crate::app::AppContext;
use crate::cli::output::{HumanLayout, emit_json};
use crate::error::{MsError, Result};

#[derive(Args, Debug)]
pub struct FeedbackArgs {
    #[command(subcommand)]
    pub command: FeedbackCommand,
}

#[derive(Subcommand, Debug)]
pub enum FeedbackCommand {
    /// Add feedback for a skill
    Add(FeedbackAddArgs),
    /// List feedback records
    List(FeedbackListArgs),
}

#[derive(Args, Debug)]
pub struct FeedbackAddArgs {
    /// Skill ID or name
    pub skill: String,

    /// Mark as positive feedback
    #[arg(long)]
    pub positive: bool,

    /// Mark as negative feedback
    #[arg(long)]
    pub negative: bool,

    /// Numeric rating (1-5)
    #[arg(long)]
    pub rating: Option<i64>,

    /// Optional comment
    #[arg(long)]
    pub comment: Option<String>,
}

#[derive(Args, Debug)]
pub struct FeedbackListArgs {
    /// Filter by skill ID or name
    #[arg(long)]
    pub skill: Option<String>,

    /// Limit results
    #[arg(long, default_value = "20")]
    pub limit: usize,

    /// Offset results
    #[arg(long, default_value = "0")]
    pub offset: usize,
}

pub fn run(ctx: &AppContext, args: &FeedbackArgs) -> Result<()> {
    match &args.command {
        FeedbackCommand::Add(add) => run_add(ctx, add),
        FeedbackCommand::List(list) => run_list(ctx, list),
    }
}

fn run_add(ctx: &AppContext, args: &FeedbackAddArgs) -> Result<()> {
    let skill_id = resolve_skill_id(ctx, &args.skill)?;
    let feedback_type = select_feedback_type(args)?;

    if let Some(rating) = args.rating {
        if !(1..=5).contains(&rating) {
            return Err(MsError::ValidationFailed(
                "rating must be between 1 and 5".to_string(),
            ));
        }
    }

    let record = ctx.db.record_skill_feedback(
        &skill_id,
        &feedback_type,
        args.rating,
        args.comment.as_deref(),
    )?;

    if ctx.robot_mode {
        let payload = serde_json::json!({
            "status": "ok",
            "record": record,
        });
        return emit_json(&payload);
    }

    let mut layout = HumanLayout::new();
    layout
        .title("Feedback Recorded")
        .kv("Skill", &record.skill_id)
        .kv("Type", &record.feedback_type)
        .kv(
            "Rating",
            &record
                .rating.map_or_else(|| "-".to_string(), |r| r.to_string()),
        )
        .kv(
            "Comment",
            &record.comment.clone().unwrap_or_else(|| "-".to_string()),
        );
    crate::cli::output::emit_human(layout);
    Ok(())
}

fn run_list(ctx: &AppContext, args: &FeedbackListArgs) -> Result<()> {
    let skill_id = match &args.skill {
        Some(skill) => Some(resolve_skill_id(ctx, skill)?),
        None => None,
    };

    let records = ctx
        .db
        .list_skill_feedback(skill_id.as_deref(), args.limit, args.offset)?;

    if ctx.robot_mode {
        let payload = serde_json::json!({
            "status": "ok",
            "records": records,
        });
        return emit_json(&payload);
    }

    if records.is_empty() {
        println!("No feedback records.");
        return Ok(());
    }

    let mut layout = HumanLayout::new();
    layout.title("Feedback Records");
    for record in records {
        let label = format!("{} ({})", record.skill_id, record.feedback_type);
        let rating = record
            .rating.map_or_else(|| "-".to_string(), |r| r.to_string());
        let comment = record.comment.unwrap_or_else(|| "-".to_string());
        layout.kv(&label, &format!("rating {rating} · {comment}"));
    }
    crate::cli::output::emit_human(layout);
    Ok(())
}

fn select_feedback_type(args: &FeedbackAddArgs) -> Result<String> {
    if args.positive && args.negative {
        return Err(MsError::ValidationFailed(
            "cannot set both --positive and --negative".to_string(),
        ));
    }
    if args.positive {
        return Ok("positive".to_string());
    }
    if args.negative {
        return Ok("negative".to_string());
    }
    if args.rating.is_some() {
        return Ok("rating".to_string());
    }
    Err(MsError::ValidationFailed(
        "provide --positive, --negative, or --rating".to_string(),
    ))
}

fn resolve_skill_id(ctx: &AppContext, input: &str) -> Result<String> {
    if let Some(skill) = ctx.db.get_skill(input)? {
        return Ok(skill.id);
    }
    if let Ok(Some(alias)) = ctx.db.resolve_alias(input) {
        if let Some(skill) = ctx.db.get_skill(&alias.canonical_id)? {
            return Ok(skill.id);
        }
    }
    Err(MsError::SkillNotFound(format!("skill not found: {input}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::{Parser, Subcommand};

    #[derive(Parser)]
    struct TestCli {
        #[command(subcommand)]
        cmd: TestCommand,
    }

    #[derive(Subcommand)]
    enum TestCommand {
        Feedback(FeedbackArgs),
    }

    #[test]
    fn parse_feedback_add_flags() {
        let parsed = TestCli::parse_from([
            "test",
            "feedback",
            "add",
            "skill-1",
            "--positive",
            "--rating",
            "5",
            "--comment",
            "nice",
        ]);
        let TestCommand::Feedback(args) = parsed.cmd;
        match args.command {
            FeedbackCommand::Add(add) => {
                assert_eq!(add.skill, "skill-1");
                assert!(add.positive);
                assert!(!add.negative);
                assert_eq!(add.rating, Some(5));
                assert_eq!(add.comment.as_deref(), Some("nice"));
            }
            _ => panic!("expected add"),
        }
    }

    #[test]
    fn parse_feedback_list_defaults() {
        let parsed = TestCli::parse_from(["test", "feedback", "list"]);
        let TestCommand::Feedback(args) = parsed.cmd;
        match args.command {
            FeedbackCommand::List(list) => {
                assert_eq!(list.limit, 20);
                assert_eq!(list.offset, 0);
                assert!(list.skill.is_none());
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn select_feedback_type_validation() {
        let base = FeedbackAddArgs {
            skill: "skill".to_string(),
            positive: false,
            negative: false,
            rating: None,
            comment: None,
        };
        assert!(select_feedback_type(&base).is_err());

        let positive = FeedbackAddArgs {
            skill: "skill".to_string(),
            positive: true,
            negative: false,
            rating: None,
            comment: None,
        };
        assert_eq!(select_feedback_type(&positive).unwrap(), "positive");

        let negative = FeedbackAddArgs {
            skill: "skill".to_string(),
            positive: false,
            negative: true,
            rating: None,
            comment: None,
        };
        assert_eq!(select_feedback_type(&negative).unwrap(), "negative");

        let rating = FeedbackAddArgs {
            skill: "skill".to_string(),
            positive: false,
            negative: false,
            rating: Some(3),
            comment: None,
        };
        assert_eq!(select_feedback_type(&rating).unwrap(), "rating");

        let both = FeedbackAddArgs {
            skill: "skill".to_string(),
            positive: true,
            negative: true,
            rating: None,
            comment: None,
        };
        assert!(select_feedback_type(&both).is_err());
    }
}
