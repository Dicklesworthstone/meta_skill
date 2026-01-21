//! ms diff - Semantic diff between skills

use clap::Args;

use crate::app::AppContext;
use crate::cli::commands::resolve_skill_markdown;
use crate::cli::output;
use crate::cli::output::OutputFormat;
use crate::core::SkillSpec;
use crate::core::spec_lens::parse_markdown;
use crate::error::Result;

#[derive(Args, Debug)]
pub struct DiffArgs {
    /// First skill
    pub skill_a: String,

    /// Second skill
    pub skill_b: String,

    /// Show only structural differences
    #[arg(long)]
    pub structure_only: bool,

    /// Output format: text, json
    #[arg(long, default_value = "text")]
    pub format: String,
}

pub fn run(_ctx: &AppContext, _args: &DiffArgs) -> Result<()> {
    let ctx = _ctx;
    let args = _args;

    let path_a = resolve_skill_markdown(ctx, &args.skill_a)?;
    let path_b = resolve_skill_markdown(ctx, &args.skill_b)?;

    let spec_a = parse_markdown(&std::fs::read_to_string(&path_a).map_err(|err| {
        crate::error::MsError::Config(format!("read {}: {err}", path_a.display()))
    })?)?;
    let spec_b = parse_markdown(&std::fs::read_to_string(&path_b).map_err(|err| {
        crate::error::MsError::Config(format!("read {}: {err}", path_b.display()))
    })?)?;

    let diffs = diff_specs(&spec_a, &spec_b, args.structure_only);
    let same = diffs.is_empty();

    if ctx.output_format != OutputFormat::Human || args.format == "json" {
        let payload = DiffReport {
            skill_a: path_a.display().to_string(),
            skill_b: path_b.display().to_string(),
            same,
            differences: diffs,
        };
        return output::emit_json(&payload);
    }

    if same {
        println!("No differences.");
    } else {
        for diff in diffs {
            println!("- {diff}");
        }
    }
    Ok(())
}

#[derive(serde::Serialize)]
struct DiffReport {
    skill_a: String,
    skill_b: String,
    same: bool,
    differences: Vec<String>,
}

fn diff_specs(a: &SkillSpec, b: &SkillSpec, structure_only: bool) -> Vec<String> {
    let mut diffs = Vec::new();

    if a.metadata.name != b.metadata.name {
        diffs.push(format!(
            "metadata.name: '{}' != '{}'",
            a.metadata.name, b.metadata.name
        ));
    }
    if a.metadata.description != b.metadata.description {
        diffs.push("metadata.description differs".to_string());
    }
    if a.metadata.version != b.metadata.version {
        diffs.push(format!(
            "metadata.version: '{}' != '{}'",
            a.metadata.version, b.metadata.version
        ));
    }
    if a.metadata.tags != b.metadata.tags {
        diffs.push("metadata.tags differ".to_string());
    }

    if a.sections.len() != b.sections.len() {
        diffs.push(format!(
            "sections.count: {} != {}",
            a.sections.len(),
            b.sections.len()
        ));
    }

    for (idx, (section_a, section_b)) in a.sections.iter().zip(b.sections.iter()).enumerate() {
        if section_a.title != section_b.title {
            diffs.push(format!(
                "section[{idx}].title: '{}' != '{}'",
                section_a.title, section_b.title
            ));
        }
        if section_a.blocks.len() != section_b.blocks.len() {
            diffs.push(format!(
                "section[{idx}].blocks.count: {} != {}",
                section_a.blocks.len(),
                section_b.blocks.len()
            ));
        }

        for (bidx, (block_a, block_b)) in section_a
            .blocks
            .iter()
            .zip(section_b.blocks.iter())
            .enumerate()
        {
            if block_a.block_type != block_b.block_type {
                diffs.push(format!("section[{idx}].blocks[{bidx}].type differs"));
            }
            if !structure_only && block_a.content != block_b.content {
                diffs.push(format!("section[{idx}].blocks[{bidx}].content differs"));
            }
        }
    }

    diffs
}
