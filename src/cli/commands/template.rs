//! ms template - curated skill templates.

use std::sync::Arc;

use clap::{Args, Subcommand, ValueEnum};

use crate::app::AppContext;
use crate::cli::output::OutputFormat;
use crate::core::{SkillLayer, spec_lens::parse_markdown};
use crate::error::{MsError, Result};
use crate::storage::TxManager;
use crate::templates::{TemplateContext, find_template, list_templates, render_template};

#[derive(Args, Debug)]
pub struct TemplateArgs {
    #[command(subcommand)]
    pub command: TemplateCommand,
}

#[derive(Subcommand, Debug)]
pub enum TemplateCommand {
    /// List available templates
    List(TemplateListArgs),
    /// Show a template (markdown or JSON)
    Show(TemplateShowArgs),
    /// Apply a template to create a new skill
    Apply(TemplateApplyArgs),
}

#[derive(Args, Debug)]
pub struct TemplateListArgs {}

#[derive(ValueEnum, Copy, Clone, Debug)]
pub enum TemplateFormat {
    Markdown,
    Json,
}

#[derive(Args, Debug)]
pub struct TemplateShowArgs {
    /// Template id or name
    pub template: String,

    /// Output format (markdown/json)
    #[arg(long, value_enum, default_value = "markdown")]
    pub format: TemplateFormat,
}

#[derive(Args, Debug)]
pub struct TemplateApplyArgs {
    /// Template id or name
    pub template: String,

    /// New skill id
    #[arg(long)]
    pub id: String,

    /// New skill name
    #[arg(long)]
    pub name: String,

    /// Description (what + when to use)
    #[arg(long)]
    pub description: String,

    /// Tags for the new skill (repeat or comma-separated)
    #[arg(long = "tag", alias = "tags", value_delimiter = ',')]
    pub tags: Vec<String>,

    /// Skill layer (base/org/project/user)
    #[arg(long, default_value = "project")]
    pub layer: String,
}

pub fn run(ctx: &AppContext, args: &TemplateArgs) -> Result<()> {
    match &args.command {
        TemplateCommand::List(list) => run_list(ctx, list),
        TemplateCommand::Show(show) => run_show(ctx, show),
        TemplateCommand::Apply(apply) => run_apply(ctx, apply),
    }
}

fn run_list(ctx: &AppContext, _args: &TemplateListArgs) -> Result<()> {
    let templates = list_templates();

    if ctx.output_format != OutputFormat::Human {
        let payload = templates
            .iter()
            .map(|template| {
                serde_json::json!({
                    "id": template.id,
                    "name": template.name,
                    "summary": template.summary,
                    "tags": template.default_tags,
                })
            })
            .collect::<Vec<_>>();
        return crate::cli::output::emit_json(&serde_json::json!({
            "status": "ok",
            "count": templates.len(),
            "templates": payload,
        }));
    }

    println!("Templates:");
    for template in templates {
        println!(
            "  {:<14} {} — {}",
            template.id, template.name, template.summary
        );
    }
    Ok(())
}

fn run_show(ctx: &AppContext, args: &TemplateShowArgs) -> Result<()> {
    let template = find_template(&args.template)
        .ok_or_else(|| MsError::NotFound(format!("template not found: {}", args.template)))?;

    let payload = serde_json::json!({
        "id": template.id,
        "name": template.name,
        "summary": template.summary,
        "tags": template.default_tags,
        "body": template.body,
    });

    if ctx.output_format != OutputFormat::Human || matches!(args.format, TemplateFormat::Json) {
        return crate::cli::output::emit_json(&payload);
    }

    println!("{}", template.body);
    Ok(())
}

fn run_apply(ctx: &AppContext, args: &TemplateApplyArgs) -> Result<()> {
    let template = find_template(&args.template)
        .ok_or_else(|| MsError::NotFound(format!("template not found: {}", args.template)))?;

    let layer = resolve_layer(&args.layer);
    let context = TemplateContext {
        id: args.id.clone(),
        name: args.name.clone(),
        description: args.description.clone(),
        tags: args.tags.clone(),
    };

    let rendered = render_template(template, &context)?;
    let spec = parse_markdown(&rendered)?;

    if spec.metadata.id.trim().is_empty() {
        return Err(MsError::ValidationFailed(
            "template produced empty skill id".to_string(),
        ));
    }
    if ctx.git.skill_exists(&spec.metadata.id) {
        return Err(MsError::ValidationFailed(format!(
            "skill already exists: {}",
            spec.metadata.id
        )));
    }

    let tx_mgr = TxManager::new(
        Arc::clone(&ctx.db),
        Arc::clone(&ctx.git),
        ctx.ms_root.clone(),
    )?;
    tx_mgr.write_skill_with_layer(&spec, layer)?;

    if let Some(record) = ctx.db.get_skill(&spec.metadata.id)? {
        ctx.search.index_skill(&record)?;
        ctx.search.commit()?;
    }

    let skill_path = ctx
        .git
        .skill_path(&spec.metadata.id)
        .map(|path| path.display().to_string())
        .unwrap_or_default();

    if ctx.output_format != OutputFormat::Human {
        return crate::cli::output::emit_json(&serde_json::json!({
            "status": "ok",
            "template": template.id,
            "skill_id": spec.metadata.id,
            "path": skill_path,
        }));
    }

    println!("Created skill: {}", spec.metadata.id);
    if !skill_path.is_empty() {
        println!("Path: {skill_path}");
    }
    Ok(())
}

fn resolve_layer(input: &str) -> SkillLayer {
    match input.to_lowercase().as_str() {
        "base" | "system" => SkillLayer::Base,
        "org" | "global" => SkillLayer::Org,
        "user" | "local" => SkillLayer::User,
        _ => SkillLayer::Project,
    }
}
