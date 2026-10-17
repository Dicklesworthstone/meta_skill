//! ms fmt - Format skill files

use clap::Args;
use itertools::Itertools;

use crate::app::AppContext;
use crate::cli::commands::{discover_skill_markdowns, resolve_skill_markdown};
use crate::cli::output::{OutputFormat, emit_json};
use crate::core::spec_lens::{SpecLens, compile_markdown, parse_markdown};
use crate::error::Result;

#[derive(Args, Debug)]
pub struct FmtArgs {
    /// Skills to format (default: all)
    pub skills: Vec<String>,

    /// Check formatting without modifying
    #[arg(long)]
    pub check: bool,

    /// Show diff instead of modifying
    #[arg(long)]
    pub diff: bool,
}

pub fn run(_ctx: &AppContext, _args: &FmtArgs) -> Result<()> {
    let ctx = _ctx;
    let args = _args;

    let targets = if args.skills.is_empty() {
        discover_skill_markdowns(ctx)?
    } else {
        args.skills
            .iter()
            .map(|skill| resolve_skill_markdown(ctx, skill))
            .collect::<Result<Vec<_>>>()?
    };

    let mut dirty = Vec::new();
    let total_files = targets.len();
    let lens = SpecLens;

    for path in targets {
        let raw = std::fs::read_to_string(&path).map_err(|err| {
            crate::error::MsError::Config(format!("read {}: {err}", path.display()))
        })?;
        let spec = parse_markdown(&raw)?;
        lens.verify_roundtrip(&spec)?;
        let formatted = compile_markdown(&spec);

        if raw != formatted {
            dirty.push(path.clone());
        }

        if args.diff {
            let diff = simple_diff(&raw, &formatted);
            if ctx.output_format == OutputFormat::Human {
                println!("--- {}", path.display());
                println!("+++ formatted");
                print!("{diff}");
            }
            continue;
        }

        if args.check {
            continue;
        }

        if raw != formatted {
            std::fs::write(&path, formatted).map_err(|err| {
                crate::error::MsError::Config(format!("write {}: {err}", path.display()))
            })?;
            if let Ok(Some(existing)) = ctx.db.get_skill(&spec.metadata.id) {
                if let Ok(tx_mgr) = crate::storage::TxManager::new(
                    ctx.db.clone(),
                    ctx.git.clone(),
                    ctx.ms_root.clone(),
                ) {
                    let layer = match existing.source_layer.to_ascii_lowercase().as_str() {
                        "base" | "system" => crate::core::SkillLayer::Base,
                        "org" | "global" => crate::core::SkillLayer::Org,
                        "user" | "local" | "session" => crate::core::SkillLayer::User,
                        _ => crate::core::SkillLayer::Project,
                    };
                    let _ = tx_mgr.write_skill_with_layer(&spec, layer);
                }
            }
        }
    }

    if args.check && !dirty.is_empty() {
        return Err(crate::error::MsError::ValidationFailed(format!(
            "{} files need formatting",
            dirty.len()
        )));
    }

    if ctx.output_format != OutputFormat::Human {
        return emit_json(&serde_json::json!({
            "status": "ok",
            "total_files": total_files,
            "modified_or_dirty": dirty.len(),
            "check": args.check,
            "diff": args.diff,
            "files": dirty.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
        }));
    }

    Ok(())
}

fn simple_diff(old: &str, new: &str) -> String {
    let mut out = String::new();
    for pair in old.lines().zip_longest(new.lines()) {
        match pair {
            itertools::EitherOrBoth::Both(left, right) if left == right => {}
            itertools::EitherOrBoth::Both(left, right) => {
                out.push('-');
                out.push_str(left);
                out.push('\n');
                out.push('+');
                out.push_str(right);
                out.push('\n');
            }
            itertools::EitherOrBoth::Left(left) => {
                out.push('-');
                out.push_str(left);
                out.push('\n');
            }
            itertools::EitherOrBoth::Right(right) => {
                out.push('+');
                out.push_str(right);
                out.push('\n');
            }
        }
    }
    out
}
