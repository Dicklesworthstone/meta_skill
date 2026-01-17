//! ms preferences - Manage skill favorites and hidden skills.

use clap::{Args, Subcommand};

use crate::app::AppContext;
use crate::cli::output::OutputFormat;
use crate::cli::output::{HumanLayout, emit_json};
use crate::error::{MsError, Result};

#[derive(Args, Debug)]
pub struct PreferencesArgs {
    #[command(subcommand)]
    pub command: PreferencesCommand,
}

#[derive(Subcommand, Debug)]
pub enum PreferencesCommand {
    /// Mark a skill as a favorite
    Favorite(FavoriteArgs),
    /// Hide a skill from suggestions
    Hide(HideArgs),
    /// Unhide a previously hidden skill
    Unhide(UnhideArgs),
    /// List all favorite skills
    Favorites(ListFavoritesArgs),
    /// List all hidden skills
    Hidden(ListHiddenArgs),
}

#[derive(Args, Debug)]
pub struct FavoriteArgs {
    /// Skill ID or name to favorite
    pub skill: String,

    /// Remove the favorite (unfavorite)
    #[arg(long)]
    pub remove: bool,
}

#[derive(Args, Debug)]
pub struct HideArgs {
    /// Skill ID or name to hide
    pub skill: String,
}

#[derive(Args, Debug)]
pub struct UnhideArgs {
    /// Skill ID or name to unhide
    pub skill: String,
}

#[derive(Args, Debug)]
pub struct ListFavoritesArgs {
    /// Limit results
    #[arg(long, default_value = "50")]
    pub limit: usize,

    /// Offset results
    #[arg(long, default_value = "0")]
    pub offset: usize,
}

#[derive(Args, Debug)]
pub struct ListHiddenArgs {
    /// Limit results
    #[arg(long, default_value = "50")]
    pub limit: usize,

    /// Offset results
    #[arg(long, default_value = "0")]
    pub offset: usize,
}

pub fn run(ctx: &AppContext, args: &PreferencesArgs) -> Result<()> {
    match &args.command {
        PreferencesCommand::Favorite(a) => run_favorite(ctx, a),
        PreferencesCommand::Hide(a) => run_hide(ctx, a),
        PreferencesCommand::Unhide(a) => run_unhide(ctx, a),
        PreferencesCommand::Favorites(a) => run_list_favorites(ctx, a),
        PreferencesCommand::Hidden(a) => run_list_hidden(ctx, a),
    }
}

fn run_favorite(ctx: &AppContext, args: &FavoriteArgs) -> Result<()> {
    let skill_id = resolve_skill_id(ctx, &args.skill)?;

    if args.remove {
        let removed = ctx.db.remove_user_preference(&skill_id, "favorite")?;
        if ctx.output_format != OutputFormat::Human {
            return emit_json(&serde_json::json!({
                "status": "ok",
                "action": "unfavorite",
                "skill_id": skill_id,
                "removed": removed,
            }));
        }
        if removed {
            println!("Removed {} from favorites.", skill_id);
        } else {
            println!("{} was not in favorites.", skill_id);
        }
    } else {
        let record = ctx.db.set_user_preference(&skill_id, "favorite")?;
        if ctx.output_format != OutputFormat::Human {
            return emit_json(&serde_json::json!({
                "status": "ok",
                "action": "favorite",
                "record": record,
            }));
        }
        let mut layout = HumanLayout::new();
        layout
            .title("Skill Favorited")
            .kv("Skill", &record.skill_id)
            .kv("Added", &record.created_at);
        crate::cli::output::emit_human(layout);
    }
    Ok(())
}

fn run_hide(ctx: &AppContext, args: &HideArgs) -> Result<()> {
    let skill_id = resolve_skill_id(ctx, &args.skill)?;

    let record = ctx.db.set_user_preference(&skill_id, "hidden")?;

    if ctx.output_format != OutputFormat::Human {
        return emit_json(&serde_json::json!({
            "status": "ok",
            "action": "hide",
            "record": record,
        }));
    }

    let mut layout = HumanLayout::new();
    layout
        .title("Skill Hidden")
        .kv("Skill", &record.skill_id)
        .kv("Hidden", &record.created_at);
    crate::cli::output::emit_human(layout);
    Ok(())
}

fn run_unhide(ctx: &AppContext, args: &UnhideArgs) -> Result<()> {
    let skill_id = resolve_skill_id(ctx, &args.skill)?;

    let removed = ctx.db.remove_user_preference(&skill_id, "hidden")?;

    if ctx.output_format != OutputFormat::Human {
        return emit_json(&serde_json::json!({
            "status": "ok",
            "action": "unhide",
            "skill_id": skill_id,
            "removed": removed,
        }));
    }

    if removed {
        println!("Unhidden skill: {}", skill_id);
    } else {
        println!("{} was not hidden.", skill_id);
    }
    Ok(())
}

fn run_list_favorites(ctx: &AppContext, args: &ListFavoritesArgs) -> Result<()> {
    let records = ctx.db.list_user_preferences("favorite", args.limit, args.offset)?;

    if ctx.output_format != OutputFormat::Human {
        return emit_json(&serde_json::json!({
            "status": "ok",
            "favorites": records,
        }));
    }

    if records.is_empty() {
        println!("No favorite skills.");
        return Ok(());
    }

    let mut layout = HumanLayout::new();
    layout.title("Favorite Skills");
    for record in records {
        layout.kv(&record.skill_id, &record.created_at);
    }
    crate::cli::output::emit_human(layout);
    Ok(())
}

fn run_list_hidden(ctx: &AppContext, args: &ListHiddenArgs) -> Result<()> {
    let records = ctx.db.list_user_preferences("hidden", args.limit, args.offset)?;

    if ctx.output_format != OutputFormat::Human {
        return emit_json(&serde_json::json!({
            "status": "ok",
            "hidden": records,
        }));
    }

    if records.is_empty() {
        println!("No hidden skills.");
        return Ok(());
    }

    let mut layout = HumanLayout::new();
    layout.title("Hidden Skills");
    for record in records {
        layout.kv(&record.skill_id, &record.created_at);
    }
    crate::cli::output::emit_human(layout);
    Ok(())
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
        Preferences(PreferencesArgs),
    }

    #[test]
    fn parse_favorite() {
        let parsed = TestCli::parse_from(["test", "preferences", "favorite", "my-skill"]);
        let TestCommand::Preferences(args) = parsed.cmd;
        match args.command {
            PreferencesCommand::Favorite(f) => {
                assert_eq!(f.skill, "my-skill");
                assert!(!f.remove);
            }
            _ => panic!("expected favorite"),
        }
    }

    #[test]
    fn parse_favorite_remove() {
        let parsed = TestCli::parse_from([
            "test",
            "preferences",
            "favorite",
            "my-skill",
            "--remove",
        ]);
        let TestCommand::Preferences(args) = parsed.cmd;
        match args.command {
            PreferencesCommand::Favorite(f) => {
                assert_eq!(f.skill, "my-skill");
                assert!(f.remove);
            }
            _ => panic!("expected favorite"),
        }
    }

    #[test]
    fn parse_hide() {
        let parsed = TestCli::parse_from(["test", "preferences", "hide", "annoying-skill"]);
        let TestCommand::Preferences(args) = parsed.cmd;
        match args.command {
            PreferencesCommand::Hide(h) => {
                assert_eq!(h.skill, "annoying-skill");
            }
            _ => panic!("expected hide"),
        }
    }

    #[test]
    fn parse_unhide() {
        let parsed = TestCli::parse_from(["test", "preferences", "unhide", "some-skill"]);
        let TestCommand::Preferences(args) = parsed.cmd;
        match args.command {
            PreferencesCommand::Unhide(u) => {
                assert_eq!(u.skill, "some-skill");
            }
            _ => panic!("expected unhide"),
        }
    }

    #[test]
    fn parse_favorites_list_defaults() {
        let parsed = TestCli::parse_from(["test", "preferences", "favorites"]);
        let TestCommand::Preferences(args) = parsed.cmd;
        match args.command {
            PreferencesCommand::Favorites(l) => {
                assert_eq!(l.limit, 50);
                assert_eq!(l.offset, 0);
            }
            _ => panic!("expected favorites"),
        }
    }

    #[test]
    fn parse_hidden_list_with_args() {
        let parsed = TestCli::parse_from([
            "test",
            "preferences",
            "hidden",
            "--limit",
            "10",
            "--offset",
            "5",
        ]);
        let TestCommand::Preferences(args) = parsed.cmd;
        match args.command {
            PreferencesCommand::Hidden(l) => {
                assert_eq!(l.limit, 10);
                assert_eq!(l.offset, 5);
            }
            _ => panic!("expected hidden"),
        }
    }
}
