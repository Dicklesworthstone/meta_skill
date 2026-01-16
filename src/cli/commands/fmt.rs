//! ms fmt - Format skill files

use clap::Args;
use itertools::Itertools;

use crate::app::AppContext;
use crate::cli::commands::{discover_skill_markdowns, resolve_skill_markdown};
use crate::core::spec_lens::{compile_markdown, parse_markdown};
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

    for path in targets {
        let raw = std::fs::read_to_string(&path).map_err(|err| {
            crate::error::MsError::Config(format!("read {}: {err}", path.display()))
        })?;
        let spec = parse_markdown(&raw)?;
        let formatted = compile_markdown(&spec);

        if raw != formatted {
            dirty.push(path.clone());
        }

        if args.diff {
            let diff = simple_diff(&raw, &formatted);
            println!("--- {}", path.display());
            println!("+++ formatted");
            print!("{diff}");
            continue;
        }

        if args.check {
            continue;
        }

        if raw != formatted {
            std::fs::write(&path, formatted).map_err(|err| {
                crate::error::MsError::Config(format!("write {}: {err}", path.display()))
            })?;
        }
    }

    if args.check && !dirty.is_empty() {
        return Err(crate::error::MsError::ValidationFailed(format!(
            "{} files need formatting",
            dirty.len()
        )));
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
