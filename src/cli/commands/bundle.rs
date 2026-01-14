//! ms bundle - Manage skill bundles

use std::collections::HashSet;
use std::path::{PathBuf};

use clap::{Args, Subcommand};

use crate::app::AppContext;
use crate::bundler::{Bundle, BundleInfo, BundleManifest, BundledSkill};
use crate::bundler::github::{download_bundle, download_url, publish_bundle, GitHubConfig};
use crate::bundler::install::{install as install_bundle, InstallReport};
use crate::error::{MsError, Result};
use crate::cli::output::emit_json;

#[derive(Args, Debug)]
pub struct BundleArgs {
    #[command(subcommand)]
    pub command: BundleCommand,
}

#[derive(Subcommand, Debug)]
pub enum BundleCommand {
    /// Create a new bundle
    Create(BundleCreateArgs),
    /// Publish a bundle to GitHub
    Publish(BundlePublishArgs),
    /// Install a bundle
    Install(BundleInstallArgs),
    /// List installed bundles
    List,
}

#[derive(Args, Debug)]
pub struct BundleCreateArgs {
    /// Bundle name
    pub name: String,

    /// Skills to include
    #[arg(long)]
    pub skills: Vec<String>,

    /// Bundle id (defaults to slug of name)
    #[arg(long)]
    pub id: Option<String>,

    /// Bundle version
    #[arg(long, default_value = "0.1.0")]
    pub version: String,

    /// Output path for bundle file
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Write manifest TOML alongside bundle
    #[arg(long)]
    pub write_manifest: bool,
}

#[derive(Args, Debug)]
pub struct BundlePublishArgs {
    /// Bundle path
    pub path: String,

    /// GitHub repository
    #[arg(long)]
    pub repo: Option<String>,

    /// GitHub token (overrides env)
    #[arg(long)]
    pub token: Option<String>,

    /// Release tag (defaults to v<bundle version>)
    #[arg(long)]
    pub tag: Option<String>,

    /// Asset name (defaults to bundle filename)
    #[arg(long)]
    pub asset_name: Option<String>,

    /// Create release as draft
    #[arg(long)]
    pub draft: bool,

    /// Create release as prerelease
    #[arg(long)]
    pub prerelease: bool,
}

#[derive(Args, Debug)]
pub struct BundleInstallArgs {
    /// Bundle source (path or URL)
    pub source: String,

    /// Skills to install (defaults to all)
    #[arg(long)]
    pub skills: Vec<String>,

    /// GitHub token (overrides env)
    #[arg(long)]
    pub token: Option<String>,

    /// Release tag (defaults to latest)
    #[arg(long)]
    pub tag: Option<String>,

    /// Asset name to download
    #[arg(long)]
    pub asset_name: Option<String>,
}

pub fn run(_ctx: &AppContext, _args: &BundleArgs) -> Result<()> {
    let ctx = _ctx;
    let args = _args;

    match &args.command {
        BundleCommand::Create(create) => run_create(ctx, create),
        BundleCommand::Install(install) => run_install(ctx, install),
        BundleCommand::Publish(publish) => run_publish(ctx, publish),
        _ => Ok(()),
    }
}

fn run_create(ctx: &AppContext, args: &BundleCreateArgs) -> Result<()> {
    let skills = normalize_skill_list(&args.skills);
    if skills.is_empty() {
        return Err(MsError::ValidationFailed(
            "bundle create requires --skills".to_string(),
        ));
    }

    let bundle_id = args.id.clone().unwrap_or_else(|| slugify(&args.name));
    let root = ctx.git.root().to_path_buf();

    let mut entries = Vec::new();
    for skill_id in skills {
        let skill_dir = ctx.git.skill_path(&skill_id).ok_or_else(|| {
            MsError::SkillNotFound(format!("invalid skill id: {}", skill_id))
        })?;
        if !skill_dir.exists() {
            return Err(MsError::SkillNotFound(format!(
                "skill not found in archive: {}",
                skill_id
            )));
        }

        let rel = skill_dir
            .strip_prefix(&root)
            .unwrap_or(&skill_dir)
            .to_path_buf();
        let metadata = ctx.git.read_metadata(&skill_id)?;
        let version = if metadata.version.trim().is_empty() {
            None
        } else {
            Some(metadata.version)
        };
        entries.push(BundledSkill {
            name: skill_id,
            path: rel,
            version,
            hash: None,
            optional: false,
        });
    }

    let manifest = BundleManifest {
        bundle: BundleInfo {
            id: bundle_id.clone(),
            name: args.name.clone(),
            version: args.version.clone(),
            description: None,
            authors: Vec::new(),
            license: None,
            repository: None,
            keywords: Vec::new(),
            ms_version: Some(env!("CARGO_PKG_VERSION").to_string()),
        },
        skills: entries,
        dependencies: Vec::new(),
        checksum: None,
        signatures: Vec::new(),
    };
    manifest.validate()?;

    let bundle = Bundle::new(manifest, &root);
    let package = bundle.package()?;
    package.verify()?;

    let output = args
        .output
        .clone()
        .unwrap_or_else(|| PathBuf::from(format!("{bundle_id}.msb")));
    if output.exists() {
        return Err(MsError::ValidationFailed(format!(
            "bundle output already exists: {}",
            output.display()
        )));
    }
    let bytes = package.to_bytes()?;
    std::fs::write(&output, bytes).map_err(|err| {
        MsError::Config(format!("write {}: {err}", output.display()))
    })?;

    let mut manifest_path = None;
    if args.write_manifest {
        let path = output
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join(format!("{bundle_id}.bundle.toml"));
        if path.exists() {
            return Err(MsError::ValidationFailed(format!(
                "manifest output already exists: {}",
                path.display()
            )));
        }
        let toml = package.manifest.to_toml_string()?;
        std::fs::write(&path, toml).map_err(|err| {
            MsError::Config(format!("write {}: {err}", path.display()))
        })?;
        manifest_path = Some(path);
    }

    if ctx.robot_mode {
        let report = BundleCreateReport {
            id: bundle_id,
            output: output.display().to_string(),
            manifest_path: manifest_path.map(|p| p.display().to_string()),
            checksum: package.manifest.checksum.clone(),
        };
        return emit_json(&report);
    }

    println!("Bundle created: {}", output.display());
    if let Some(path) = manifest_path {
        println!("Manifest written: {}", path.display());
    }

    Ok(())
}

fn run_install(ctx: &AppContext, args: &BundleInstallArgs) -> Result<()> {
    let local_path = expand_local_path(&args.source);
    let bytes = if local_path.exists() {
        std::fs::read(&local_path).map_err(|err| {
            MsError::Config(format!("read {}: {err}", local_path.display()))
        })?
    } else if looks_like_path(&args.source) {
        return Err(MsError::ValidationFailed(format!(
            "bundle source not found: {}",
            local_path.display()
        )));
    } else if args.source.starts_with("http://") || args.source.starts_with("https://") {
        download_url(&args.source, args.token.clone())?
    } else if let Some((repo, tag)) = split_repo_tag(&args.source) {
        let tag = args.tag.as_deref().or(tag);
        let download = download_bundle(repo, tag, args.asset_name.as_deref(), args.token.clone())?;
        download.bytes
    } else {
        return Err(MsError::ValidationFailed(format!(
            "bundle source not found: {}",
            args.source
        )));
    };
    let package = crate::bundler::package::BundlePackage::from_bytes(&bytes)?;

    let only = normalize_skill_list(&args.skills);
    let report = install_bundle(&package, ctx.git.root(), &only)?;

    if ctx.robot_mode {
        return emit_json(&report);
    }

    print_install_report(&report);
    Ok(())
}

fn run_publish(ctx: &AppContext, args: &BundlePublishArgs) -> Result<()> {
    let repo = args.repo.clone().ok_or_else(|| {
        MsError::ValidationFailed("bundle publish requires --repo".to_string())
    })?;
    let config = GitHubConfig {
        repo,
        token: args.token.clone(),
        tag: args.tag.clone(),
        asset_name: args.asset_name.clone(),
        draft: args.draft,
        prerelease: args.prerelease,
    };
    let result = publish_bundle(std::path::Path::new(&args.path), &config)?;

    if ctx.robot_mode {
        return emit_json(&result);
    }

    println!("Published bundle to {}", result.repo);
    println!("Release: {}", result.release_url);
    println!("Asset: {} (tag {})", result.asset_name, result.tag);
    Ok(())
}

fn normalize_skill_list(values: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for value in values {
        for part in value.split(',') {
            let trimmed = part.trim();
            if trimmed.is_empty() {
                continue;
            }
            if seen.insert(trimmed.to_string()) {
                out.push(trimmed.to_string());
            }
        }
    }
    out
}

fn slugify(input: &str) -> String {
    let mut out = String::new();
    let mut prev_dash = false;
    for ch in input.chars() {
        let lower = ch.to_ascii_lowercase();
        if lower.is_ascii_alphanumeric() {
            out.push(lower);
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    let trimmed = out.trim_matches('-');
    if trimmed.is_empty() {
        "bundle".to_string()
    } else {
        trimmed.to_string()
    }
}

fn print_install_report(report: &InstallReport) {
    println!("Bundle installed: {}", report.bundle_id);
    if !report.installed.is_empty() {
        println!("Installed:");
        for skill in &report.installed {
            println!("  - {}", skill);
        }
    }
    if !report.skipped.is_empty() {
        println!("Skipped:");
        for skill in &report.skipped {
            println!("  - {}", skill);
        }
    }
    println!("Blobs written: {}", report.blobs_written);
}

fn split_repo_tag(input: &str) -> Option<(&str, Option<&str>)> {
    if let Some((repo, tag)) = input.split_once('@') {
        return Some((repo, Some(tag)));
    }
    if input.contains('/') {
        return Some((input, None));
    }
    None
}

fn looks_like_path(input: &str) -> bool {
    input == "~"
        || input.starts_with("~/")
        || input.starts_with("./")
        || input.starts_with("../")
        || input.starts_with('/')
}

fn expand_local_path(input: &str) -> PathBuf {
    if input == "~" {
        if let Some(home) = dirs::home_dir() {
            return home;
        }
    }
    if let Some(stripped) = input.strip_prefix("~/") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    PathBuf::from(input)
}

#[derive(serde::Serialize)]
struct BundleCreateReport {
    id: String,
    output: String,
    manifest_path: Option<String>,
    checksum: Option<String>,
}
