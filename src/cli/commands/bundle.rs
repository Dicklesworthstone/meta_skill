//! ms bundle - Manage skill bundles

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use clap::{Args, Subcommand};
use semver::Version;
use serde::Serialize;

use crate::app::AppContext;
use crate::bundler::github::{GitHubConfig, download_bundle, download_url, publish_bundle};
use crate::bundler::install::InstallReport;
use crate::bundler::local_safety::{
    ModificationStatus, SkillModificationReport, backup_file, detect_modifications, hash_bytes,
};
use crate::bundler::registry::{BundleRegistry, InstallSource, InstalledBundle, ParsedSource};
use crate::bundler::{Bundle, BundleInfo, BundleManifest, BundlePackage, BundledSkill};
use crate::cli::output::OutputFormat;
use crate::cli::output::emit_json;
use crate::error::{MsError, Result};
use crate::storage::GlobalLock;

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
    /// Remove an installed bundle
    Remove(BundleRemoveArgs),
    /// Update installed bundles
    Update(BundleUpdateArgs),
    /// List installed bundles
    List,
    /// Show details of a bundle
    Show(BundleShowArgs),
    /// Verify bundle integrity, checksums, and signatures
    Verify(BundleVerifyArgs),
    /// Search installed and local bundles
    Search(BundleSearchArgs),
    /// Check for local modifications and conflicts
    Conflicts(BundleConflictsArgs),
}

#[derive(Args, Debug)]
pub struct BundleCreateArgs {
    /// Bundle name
    pub name: String,

    /// Skills to include (by ID)
    #[arg(long)]
    pub skills: Vec<String>,

    /// Directory containing skills to discover and include
    #[arg(long, visible_alias = "from", conflicts_with = "skills")]
    pub from_dir: Option<PathBuf>,

    /// Bundle id (defaults to slug of name)
    #[arg(long)]
    pub id: Option<String>,

    /// Bundle version
    #[arg(long = "bundle-version", default_value = "0.1.0")]
    pub bundle_version: String,

    /// Output path for bundle file (.msb or .tar.gz)
    #[arg(long)]
    pub output: Option<PathBuf>,

    /// Write manifest TOML alongside bundle
    #[arg(long)]
    pub write_manifest: bool,

    /// Sign the bundle with SSH key
    #[arg(long)]
    pub sign: bool,

    /// Path to SSH private key for signing
    #[arg(long, requires = "sign")]
    pub sign_key: Option<PathBuf>,
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

    /// Release tag (defaults to `v<bundle version>`)
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

    /// Target skill layer (system, global, project, local)
    #[arg(long, default_value = "project")]
    pub layer: String,

    /// GitHub token (overrides env)
    #[arg(long)]
    pub token: Option<String>,

    /// Release tag (defaults to latest)
    #[arg(long)]
    pub tag: Option<String>,

    /// Asset name to download
    #[arg(long)]
    pub asset_name: Option<String>,

    /// Skip signature and checksum verification (not recommended)
    #[arg(long)]
    pub no_verify: bool,

    /// Force reinstallation if bundle is already installed
    #[arg(long, short = 'f')]
    pub force: bool,
}

#[derive(Args, Debug)]
pub struct BundleShowArgs {
    /// Bundle source (path, URL, or repo)
    pub source: String,

    /// GitHub token (overrides env)
    #[arg(long)]
    pub token: Option<String>,

    /// Release tag (for repo sources)
    #[arg(long)]
    pub tag: Option<String>,
}

#[derive(Args, Debug)]
pub struct BundleRemoveArgs {
    /// Bundle ID to remove
    pub bundle_id: String,

    /// Remove installed skills as well
    #[arg(long)]
    pub remove_skills: bool,

    /// Force removal without confirmation
    #[arg(long, short = 'f')]
    pub force: bool,
}

#[derive(Args, Debug)]
pub struct BundleUpdateArgs {
    /// Bundle ID to update (default: all installed bundles)
    pub bundle_id: Option<String>,

    /// Check for updates without applying
    #[arg(long)]
    pub check: bool,

    /// Update all bundles (ignores `bundle_id`)
    #[arg(long)]
    pub all: bool,

    /// Show what would change without applying
    #[arg(long)]
    pub dry_run: bool,

    /// Force update (overwrite local changes)
    #[arg(long, short = 'f')]
    pub force: bool,

    /// GitHub token (overrides env)
    #[arg(long)]
    pub token: Option<String>,

    /// Skip signature verification (checksum still enforced)
    #[arg(long)]
    pub no_verify: bool,
}

#[derive(Args, Debug)]
pub struct BundleVerifyArgs {
    /// Bundle path (.msb)
    pub path: PathBuf,

    /// Optional public or private SSH/hex key file to trust for signature verification
    #[arg(long)]
    pub key: Option<PathBuf>,
}

#[derive(Args, Debug)]
pub struct BundleSearchArgs {
    /// Search query (matches bundle id, name, description, or skill names)
    pub query: String,

    /// Maximum number of results
    #[arg(long, short, default_value = "20")]
    pub limit: usize,
}

#[derive(Args, Debug)]
pub struct BundleConflictsArgs {
    /// Skill to check (default: all installed skills)
    #[arg(long)]
    pub skill: Option<String>,

    /// Bundle to check against (default: detect from installed bundles)
    #[arg(long)]
    pub bundle: Option<String>,

    /// Show only modified files
    #[arg(long)]
    pub modified_only: bool,

    /// Show detailed diff information
    #[arg(long)]
    pub diff: bool,
}

pub fn run(_ctx: &AppContext, _args: &BundleArgs) -> Result<()> {
    let ctx = _ctx;
    let args = _args;

    match &args.command {
        BundleCommand::Create(create) => run_create(ctx, create),
        BundleCommand::Install(install) => run_install(ctx, install),
        BundleCommand::Remove(remove) => run_remove(ctx, remove),
        BundleCommand::Publish(publish) => run_publish(ctx, publish),
        BundleCommand::Update(update) => run_update(ctx, update),
        BundleCommand::List => run_list(ctx),
        BundleCommand::Show(show) => run_show(ctx, show),
        BundleCommand::Verify(verify) => run_verify(ctx, verify),
        BundleCommand::Search(search) => run_search(ctx, search),
        BundleCommand::Conflicts(conflicts) => run_conflicts(ctx, conflicts),
    }
}

fn run_create(ctx: &AppContext, args: &BundleCreateArgs) -> Result<()> {
    // Discover skills from --skills list or --from-dir directory
    let skills = if let Some(ref from_dir) = args.from_dir {
        discover_skills_in_dir(from_dir)?
    } else {
        normalize_skill_list(&args.skills)
    };

    if skills.is_empty() {
        return Err(MsError::ValidationFailed(
            "bundle create requires --skills or --from-dir".to_string(),
        ));
    }

    let bundle_id = args.id.clone().unwrap_or_else(|| slugify(&args.name));
    let root = if let Some(ref dir) = args.from_dir {
        dir.canonicalize().unwrap_or(dir.clone())
    } else {
        ctx.git
            .root()
            .canonicalize()
            .unwrap_or(ctx.git.root().to_path_buf())
    };

    let mut entries = Vec::new();
    for skill_id in skills {
        // Resolve skill directory: check root first (handle --from-dir), then archive
        let skill_dir = root.join(&skill_id);
        let skill_dir = if skill_dir.exists() {
            skill_dir
        } else if let Some(path) = ctx.git.skill_path(&skill_id) {
            if path.exists() {
                path
            } else {
                return Err(MsError::SkillNotFound(format!(
                    "skill not found in archive: {}",
                    skill_id
                )));
            }
        } else {
            return Err(MsError::SkillNotFound(format!(
                "invalid skill id: {}",
                skill_id
            )));
        };

        if !skill_dir.exists() {
            return Err(MsError::SkillNotFound(format!(
                "skill directory not found: {}",
                skill_dir.display()
            )));
        }

        let skill_canon = skill_dir.canonicalize().unwrap_or(skill_dir.clone());
        let rel = skill_canon
            .strip_prefix(&root)
            .unwrap_or(&skill_canon)
            .to_path_buf();

        // Read metadata if available, use defaults otherwise
        let version = ctx.git.read_metadata(&skill_id).ok().and_then(|m| {
            if m.version.trim().is_empty() {
                None
            } else {
                Some(m.version)
            }
        });

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
            version: args.bundle_version.clone(),
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
    let mut package = bundle.package()?;
    package.verify()?;

    // Sign the bundle if requested
    if args.sign {
        let key_path = if let Some(path) = args.sign_key.clone() {
            path
        } else {
            let home = dirs::home_dir().ok_or_else(|| {
                MsError::Config(
                    "cannot find home directory for default SSH key; use --sign-key to specify"
                        .to_string(),
                )
            })?;
            home.join(".ssh").join("id_ed25519")
        };

        let signer = crate::bundler::Ed25519Signer::from_openssh_file(&key_path)?;

        // Sign the checksum (the payload that represents bundle integrity)
        let checksum = package.manifest.checksum.as_ref().ok_or_else(|| {
            MsError::ValidationFailed("bundle has no checksum to sign".to_string())
        })?;

        // Get signer identity from git config or username
        let signer_name = std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .unwrap_or_else(|_| "unknown".to_string());

        let signature = signer.sign(checksum.as_bytes(), &signer_name);
        package.manifest.signatures.push(signature);

        // Persist signer public key in <ms_root>/trusted_keys so locally signed
        // bundles can be verified and installed without --no-verify.
        let trusted_dir = ctx.ms_root.join("trusted_keys");
        if std::fs::create_dir_all(&trusted_dir).is_ok() {
            let safe_id = signer.key_id().replace(':', "_");
            let pub_path = trusted_dir.join(format!("{safe_id}.pub"));
            let _ = std::fs::write(&pub_path, hex::encode(signer.public_key()));
        }

        if ctx.output_format == OutputFormat::Human {
            eprintln!("Signed bundle with key: {}", key_path.display());
        }
    }

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
    std::fs::write(&output, bytes)
        .map_err(|err| MsError::Config(format!("write {}: {err}", output.display())))?;

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
        std::fs::write(&path, toml)
            .map_err(|err| MsError::Config(format!("write {}: {err}", path.display())))?;
        manifest_path = Some(path);
    }

    if ctx.output_format != OutputFormat::Human {
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
    // Acquire lock to prevent concurrent modifications
    let _lock = GlobalLock::acquire(&ctx.ms_root)?;

    // Parse the source using the new ParsedSource
    let parsed = ParsedSource::parse(&args.source)?;

    // Override tag and asset from args if provided
    let source = match parsed.source {
        InstallSource::GitHub { repo, tag, asset } => InstallSource::GitHub {
            repo,
            tag: args.tag.clone().or(tag),
            asset: args.asset_name.clone().or(asset),
        },
        other => other,
    };

    // Download or read the bundle bytes
    let bytes = match &source {
        InstallSource::File { path } => {
            let local_path = PathBuf::from(path);
            if !local_path.exists() {
                return Err(MsError::ValidationFailed(format!(
                    "bundle source not found: {}",
                    local_path.display()
                )));
            }
            std::fs::read(&local_path)
                .map_err(|err| MsError::Config(format!("read {}: {err}", local_path.display())))?
        }
        InstallSource::Url { url } => download_url(url, args.token.clone())?,
        InstallSource::GitHub { repo, tag, asset } => {
            let download =
                download_bundle(repo, tag.as_deref(), asset.as_deref(), args.token.clone())?;
            download.bytes
        }
    };

    let package = crate::bundler::package::BundlePackage::from_bytes(&bytes)?;
    let bundle_id = package.manifest.bundle.id.clone();
    let bundle_version = package.manifest.bundle.version.clone();
    let checksum = package.manifest.checksum.clone();

    // Check if already installed
    let mut registry = BundleRegistry::open(ctx.git.root())?;
    if registry.is_installed(&bundle_id) {
        if !args.force {
            return Err(MsError::ValidationFailed(format!(
                "bundle {bundle_id} is already installed; use --force to reinstall or ms bundle remove first"
            )));
        }
        // --force: remove existing skill directories before reinstalling
        if let Some(existing) = registry.get(&bundle_id) {
            for skill_id in &existing.skills {
                if let Some(skill_path) = ctx.git.skill_path(skill_id) {
                    if skill_path.exists() {
                        std::fs::remove_dir_all(&skill_path).map_err(|err| {
                            MsError::Config(format!(
                                "failed to remove existing skill {skill_id}: {err}"
                            ))
                        })?;
                    }
                }
            }
        }
        // Unregister old entry before re-registering
        registry.unregister(&bundle_id)?;
    }

    let only = normalize_skill_list(&args.skills);

    // Install with verification
    let report = if args.no_verify {
        let options = crate::bundler::InstallOptions::<
            crate::bundler::manifest::NoopSignatureVerifier,
        >::allow_unsigned();
        crate::bundler::install_with_options(&package, ctx.git.root(), &only, &options)?
    } else if package.manifest.signatures.is_empty() {
        // Unsigned bundle: allow but warn (development/testing scenario)
        if ctx.output_format == OutputFormat::Human {
            eprintln!(
                "Warning: Installing unsigned bundle '{}'. \
                 Use signed bundles for production deployments.",
                package.manifest.bundle.id
            );
        }
        let options = crate::bundler::InstallOptions::<
            crate::bundler::manifest::NoopSignatureVerifier,
        >::allow_unsigned();
        crate::bundler::install_with_options(&package, ctx.git.root(), &only, &options)?
    } else {
        // Signed bundle: verify against trusted Ed25519 keys in <ms_root>/trusted_keys
        use crate::bundler::manifest::SignatureVerifier;
        let verifier = load_trusted_verifier(&ctx.ms_root, None)?;
        let has_trusted = package
            .manifest
            .signatures
            .iter()
            .any(|sig| verifier.is_trusted(&sig.key_id));
        if !has_trusted {
            return Err(MsError::ValidationFailed(format!(
                "Bundle '{}' is signed, but no matching trusted key was found in {}. \
                 Use --no-verify to install unsigned/untrusted bundles, or add the signer's public key to trusted_keys.",
                package.manifest.bundle.id,
                ctx.ms_root.join("trusted_keys").display()
            )));
        }
        let options = crate::bundler::InstallOptions::with_verifier(&verifier);
        crate::bundler::install_with_options(&package, ctx.git.root(), &only, &options)?
    };

    // Persist installed skills into SQLite, GitArchive, and Tantivy search index
    let target_layer = match args.layer.to_ascii_lowercase().as_str() {
        "system" | "base" => crate::core::SkillLayer::Base,
        "global" | "org" => crate::core::SkillLayer::Org,
        "local" | "user" | "session" => crate::core::SkillLayer::User,
        _ => crate::core::SkillLayer::Project,
    };
    if !report.installed.is_empty() {
        let tx_mgr =
            crate::storage::TxManager::new(ctx.db.clone(), ctx.git.clone(), ctx.ms_root.clone())?;
        let _ = tx_mgr.recover();
        for skill_id in &report.installed {
            if let Ok(skill_spec) = ctx.git.read_skill(skill_id) {
                tx_mgr.write_skill_with_layer(&skill_spec, target_layer)?;
                if let Ok(Some(record)) = ctx.db.get_skill(skill_id) {
                    let _ = ctx.search.index_skill(&record);
                }
            }
        }
        let _ = ctx.search.commit();
    }

    // Register the installation
    let installed = InstalledBundle {
        id: bundle_id,
        version: bundle_version,
        source,
        installed_at: chrono::Utc::now(),
        skills: report.installed.clone(),
        checksum,
    };
    registry.register(installed)?;

    if ctx.output_format != OutputFormat::Human {
        return emit_json(&report);
    }

    print_install_report(&report);
    Ok(())
}

fn run_remove(ctx: &AppContext, args: &BundleRemoveArgs) -> Result<()> {
    use std::io::Write;

    // Acquire lock to prevent concurrent modifications
    let _lock = GlobalLock::acquire(&ctx.ms_root)?;

    let mut registry = BundleRegistry::open(ctx.git.root())?;

    // Check if bundle is in registry
    let installed = registry.get(&args.bundle_id).cloned();

    // Also check for legacy .msb file
    let bundles_dir = ctx.git.root().join("bundles");
    let bundle_path = bundles_dir.join(format!("{}.msb", args.bundle_id));
    let has_bundle_file = bundle_path.exists();

    if installed.is_none() && !has_bundle_file {
        return Err(MsError::NotFound(format!(
            "bundle '{}' is not installed",
            args.bundle_id
        )));
    }

    if !args.force && ctx.output_format == OutputFormat::Human {
        eprintln!("About to remove bundle: {}", args.bundle_id);
        if let Some(ref inst) = installed {
            eprintln!("Version: {}", inst.version);
            eprintln!("Installed skills: {}", inst.skills.join(", "));
        }
        if args.remove_skills {
            eprintln!("Warning: --remove-skills will also delete installed skill files");
        }
        eprint!("Continue? [y/N] ");
        std::io::stderr().flush()?;
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if !input.trim().eq_ignore_ascii_case("y") {
            return Err(MsError::Config("Removal cancelled".into()));
        }
    }

    // Remove skills if requested via 2PC TxManager + Tantivy index
    let mut removed_skills = Vec::new();
    if args.remove_skills {
        if let Some(ref inst) = installed {
            let tx_mgr = crate::storage::TxManager::new(
                ctx.db.clone(),
                ctx.git.clone(),
                ctx.ms_root.clone(),
            )?;
            let _ = tx_mgr.recover();
            for skill_id in &inst.skills {
                let exists_on_disk = ctx.git.skill_exists(skill_id);
                let exists_in_db = ctx.db.get_skill(skill_id).ok().flatten().is_some();
                if exists_on_disk || exists_in_db {
                    if exists_on_disk {
                        if let Err(err) = tx_mgr.delete_skill_unlocked(skill_id) {
                            // Fallback if skill was not tracked in Git index yet
                            tracing::debug!(skill_id = %skill_id, error = %err, "fallback skill removal");
                            if let Some(skill_path) = ctx.git.skill_path(skill_id) {
                                if skill_path.exists() {
                                    std::fs::remove_dir_all(&skill_path)?;
                                }
                            }
                            let _ = ctx.db.delete_skill(skill_id);
                        }
                    } else {
                        let _ = ctx.db.delete_skill(skill_id);
                    }
                    let _ = ctx.search.delete_skill(skill_id);
                    removed_skills.push(skill_id.clone());
                }
            }
            let _ = ctx.search.commit();
        }
    }

    // Remove from registry
    registry.unregister(&args.bundle_id)?;

    // Remove legacy bundle file if exists
    if has_bundle_file {
        std::fs::remove_file(&bundle_path)?;
    }

    if ctx.output_format != OutputFormat::Human {
        return emit_json(&serde_json::json!({
            "removed": args.bundle_id,
            "skills_removed": removed_skills,
        }));
    }

    println!("Removed bundle: {}", args.bundle_id);
    if !removed_skills.is_empty() {
        println!("Removed skills:");
        for skill in &removed_skills {
            println!("  - {skill}");
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct BundleUpdateItem {
    bundle_id: String,
    current_version: String,
    available_version: Option<String>,
    update_available: bool,
    applied: bool,
    source: String,
    conflicts: Vec<BundleConflictSummary>,
    skipped_reason: Option<String>,
    error: Option<String>,
}

#[derive(Serialize)]
struct BundleUpdateSummary {
    status: String,
    updates: Vec<BundleUpdateItem>,
}

#[derive(Serialize)]
struct BundleConflictSummary {
    skill_id: String,
    modified: usize,
    deleted: usize,
    conflict: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    modified_files: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    conflicting_files: Vec<String>,
}

fn run_update(ctx: &AppContext, args: &BundleUpdateArgs) -> Result<()> {
    // Acquire lock to prevent concurrent modifications
    let _lock = GlobalLock::acquire(&ctx.ms_root)?;

    if args.all && args.bundle_id.is_some() {
        return Err(MsError::ValidationFailed(
            "--all cannot be used with bundle_id".to_string(),
        ));
    }

    let registry = BundleRegistry::open(ctx.git.root())?;
    let mut targets: Vec<InstalledBundle> = if let Some(ref id) = args.bundle_id {
        vec![
            registry
                .get(id)
                .cloned()
                .ok_or_else(|| MsError::NotFound(format!("bundle '{id}' is not installed")))?,
        ]
    } else {
        registry.list().cloned().collect()
    };

    if targets.is_empty() {
        if ctx.output_format != OutputFormat::Human {
            return emit_json(&BundleUpdateSummary {
                status: "ok".to_string(),
                updates: Vec::new(),
            });
        }
        println!("No bundles installed.");
        return Ok(());
    }

    targets.sort_by(|a, b| a.id.cmp(&b.id));

    let mut updates = Vec::new();
    let default_check = !args.check && !args.dry_run && !args.all && args.bundle_id.is_none();

    for installed in targets {
        let item = match fetch_update_candidate(ctx, args, &installed) {
            Ok(candidate) => build_update_item(ctx, args, &installed, candidate, default_check)?,
            Err(err) => BundleUpdateItem {
                bundle_id: installed.id.clone(),
                current_version: installed.version.clone(),
                available_version: None,
                update_available: false,
                applied: false,
                source: installed.source.to_string(),
                conflicts: Vec::new(),
                skipped_reason: None,
                error: Some(err.to_string()),
            },
        };

        updates.push(item);
    }

    if ctx.output_format != OutputFormat::Human {
        return emit_json(&BundleUpdateSummary {
            status: "ok".to_string(),
            updates,
        });
    }

    print_update_summary(&updates);
    Ok(())
}

struct UpdateCandidate {
    package: BundlePackage,
    source: InstallSource,
}

fn fetch_update_candidate(
    ctx: &AppContext,
    args: &BundleUpdateArgs,
    installed: &InstalledBundle,
) -> Result<UpdateCandidate> {
    let (bytes, source_override) = match &installed.source {
        InstallSource::GitHub { repo, tag, asset } => {
            let result =
                download_bundle(repo, tag.as_deref(), asset.as_deref(), args.token.clone())?;
            (
                result.bytes,
                Some(InstallSource::GitHub {
                    repo: repo.clone(),
                    tag: Some(result.tag),
                    asset: Some(result.asset_name),
                }),
            )
        }
        InstallSource::Url { url } => (download_url(url, args.token.clone())?, None),
        InstallSource::File { path } => (
            std::fs::read(path)
                .map_err(|err| MsError::Config(format!("read bundle {path}: {err}")))?,
            None,
        ),
    };

    let package = BundlePackage::from_bytes(&bytes)?;
    let source = source_override.unwrap_or_else(|| installed.source.clone());
    if package.manifest.bundle.id != installed.id {
        return Err(MsError::ValidationFailed(format!(
            "bundle id mismatch: expected {}, got {}",
            installed.id, package.manifest.bundle.id
        )));
    }
    verify_bundle(ctx, args, &package)?;
    Ok(UpdateCandidate { package, source })
}

fn verify_bundle(ctx: &AppContext, args: &BundleUpdateArgs, package: &BundlePackage) -> Result<()> {
    package.verify()?;

    if args.no_verify {
        return Ok(());
    }

    if package.manifest.signatures.is_empty() {
        if ctx.output_format == OutputFormat::Human {
            eprintln!(
                "Warning: Updating with unsigned bundle '{}'. Use signed bundles for production.",
                package.manifest.bundle.id
            );
        }
        return Ok(());
    }

    let verifier = load_trusted_verifier(&ctx.ms_root, None)?;
    package.verify_signatures(&verifier)
}

fn build_update_item(
    ctx: &AppContext,
    args: &BundleUpdateArgs,
    installed: &InstalledBundle,
    candidate: UpdateCandidate,
    default_check: bool,
) -> Result<BundleUpdateItem> {
    let new_version = candidate.package.manifest.bundle.version.clone();
    let update_available = is_newer_version(&installed.version, &new_version)?;

    let mut item = BundleUpdateItem {
        bundle_id: installed.id.clone(),
        current_version: installed.version.clone(),
        available_version: Some(new_version),
        update_available,
        applied: false,
        source: installed.source.to_string(),
        conflicts: Vec::new(),
        skipped_reason: None,
        error: None,
    };

    let check_only = args.check || args.dry_run || default_check;
    if !update_available {
        item.available_version = None;
        return Ok(item);
    }

    if check_only {
        item.skipped_reason = Some("check_only".to_string());
        return Ok(item);
    }

    let apply_result = apply_bundle_update(ctx, args, installed, &candidate)?;
    item.applied = apply_result.applied;
    item.conflicts = apply_result.conflicts;
    item.skipped_reason = apply_result.skipped_reason;
    Ok(item)
}

struct ApplyResult {
    applied: bool,
    conflicts: Vec<BundleConflictSummary>,
    skipped_reason: Option<String>,
}

fn apply_bundle_update(
    ctx: &AppContext,
    args: &BundleUpdateArgs,
    installed: &InstalledBundle,
    candidate: &UpdateCandidate,
) -> Result<ApplyResult> {
    let mut conflicts = Vec::new();
    let backup_root = backup_root(ctx, &installed.id);
    let mut pending = Vec::new();

    for skill in &candidate.package.manifest.skills {
        let (entries, new_hashes) = bundle_skill_entries(&candidate.package, skill)?;
        let target = resolve_bundle_target(ctx.git.root(), &skill.path, &skill.name)?;

        let report = if target.exists() {
            if let Some(existing_hashes) = load_bundle_meta(&target)? {
                let report = detect_modifications(&target, &skill.name, &existing_hashes)?;
                if report.needs_attention() {
                    let modified_files = report
                        .modified_files()
                        .into_iter()
                        .map(|f| f.path.display().to_string())
                        .collect();
                    let conflicting_files = report
                        .conflicting_files()
                        .into_iter()
                        .map(|f| f.path.display().to_string())
                        .collect();
                    conflicts.push(BundleConflictSummary {
                        skill_id: report.skill_id.clone(),
                        modified: report.summary.modified,
                        deleted: report.summary.deleted,
                        conflict: report.summary.conflict,
                        modified_files,
                        conflicting_files,
                    });
                }
                Some(report)
            } else {
                if ctx.output_format == OutputFormat::Human {
                    eprintln!(
                        "Warning: bundle metadata missing for '{}'; update will overwrite without modification detection.",
                        skill.name
                    );
                }
                None
            }
        } else {
            None
        };

        pending.push((skill.name.clone(), target, entries, new_hashes, report));
    }

    if !conflicts.is_empty() && !args.force {
        return Ok(ApplyResult {
            applied: false,
            conflicts,
            skipped_reason: Some("conflicts_detected".to_string()),
        });
    }

    for (skill_name, target, entries, expected_hashes, report) in pending {
        let skill_backup_root = backup_root.join(&skill_name);
        if let Some(report) = report {
            if report.needs_attention() && args.force {
                for file in report
                    .modified_files()
                    .into_iter()
                    .chain(report.conflicting_files())
                {
                    let path = target.join(&file.path);
                    if path.exists() {
                        let _ = backup_file(&path, &skill_backup_root);
                    }
                }
            }
        }

        write_bundle_files(&target, &entries)?;
        write_bundle_meta(&target, &expected_hashes)?;
    }

    let installed = InstalledBundle {
        id: candidate.package.manifest.bundle.id.clone(),
        version: candidate.package.manifest.bundle.version.clone(),
        source: candidate.source.clone(),
        installed_at: chrono::Utc::now(),
        skills: candidate
            .package
            .manifest
            .skills
            .iter()
            .map(|s| s.name.clone())
            .collect(),
        checksum: candidate.package.manifest.checksum.clone(),
    };
    BundleRegistry::open(ctx.git.root())?.register(installed)?;

    Ok(ApplyResult {
        applied: true,
        conflicts,
        skipped_reason: None,
    })
}

fn is_newer_version(current: &str, candidate: &str) -> Result<bool> {
    let current_trim = current.trim_start_matches('v');
    let candidate_trim = candidate.trim_start_matches('v');
    match (Version::parse(current_trim), Version::parse(candidate_trim)) {
        (Ok(c), Ok(n)) => Ok(n > c),
        _ => Ok(candidate_trim != current_trim),
    }
}

fn bundle_skill_entries(
    package: &BundlePackage,
    skill: &BundledSkill,
) -> Result<(Vec<(PathBuf, Vec<u8>)>, HashMap<PathBuf, String>)> {
    let hash = skill.hash.as_ref().ok_or_else(|| {
        MsError::ValidationFailed(format!("missing blob hash for {}", skill.name))
    })?;
    let blob = package
        .blobs
        .iter()
        .find(|b| &b.hash == hash)
        .ok_or_else(|| {
            MsError::ValidationFailed(format!("bundle missing blob {} for {}", hash, skill.name))
        })?;

    let entries = decode_blob_entries(&blob.bytes)?;
    let mut expected_hashes = HashMap::new();
    for (path, bytes) in &entries {
        expected_hashes.insert(path.clone(), hash_bytes(bytes));
    }
    Ok((entries, expected_hashes))
}

fn decode_blob_entries(input: &[u8]) -> Result<Vec<(PathBuf, Vec<u8>)>> {
    let mut cursor = 0usize;
    let mut entries = Vec::new();
    while cursor < input.len() {
        let name_len = read_u64(input, &mut cursor)? as usize;
        if name_len == 0 {
            return Err(MsError::ValidationFailed(
                "bundle entry has empty path".to_string(),
            ));
        }
        let name_bytes = read_slice(input, &mut cursor, name_len)?;
        let name = std::str::from_utf8(name_bytes).map_err(|_| {
            MsError::ValidationFailed("bundle entry path is invalid UTF-8".to_string())
        })?;
        let file_len = read_u64(input, &mut cursor)? as usize;
        let file_bytes = read_slice(input, &mut cursor, file_len)?.to_vec();
        entries.push((PathBuf::from(name), file_bytes));
    }
    Ok(entries)
}

fn write_bundle_files(target: &Path, entries: &[(PathBuf, Vec<u8>)]) -> Result<()> {
    for (rel, bytes) in entries {
        ensure_relative(rel)?;
        let path = target.join(rel);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|err| MsError::Config(format!("create {}: {err}", parent.display())))?;
        }
        std::fs::write(&path, bytes)
            .map_err(|err| MsError::Config(format!("write {}: {err}", path.display())))?;
    }
    Ok(())
}

fn load_bundle_meta(target: &Path) -> Result<Option<HashMap<PathBuf, String>>> {
    let meta_path = target.join(".bundle_meta.json");
    if !meta_path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&meta_path)
        .map_err(|err| MsError::Config(format!("read {}: {err}", meta_path.display())))?;
    let parsed = serde_json::from_str(&content)
        .map_err(|err| MsError::Config(format!("parse {}: {err}", meta_path.display())))?;
    Ok(Some(parsed))
}

fn write_bundle_meta(target: &Path, expected_hashes: &HashMap<PathBuf, String>) -> Result<()> {
    let meta_path = target.join(".bundle_meta.json");
    let content = serde_json::to_string_pretty(expected_hashes)
        .map_err(|err| MsError::Config(format!("serialize bundle meta: {err}")))?;
    std::fs::write(&meta_path, content)
        .map_err(|err| MsError::Config(format!("write {}: {err}", meta_path.display())))
}

fn resolve_bundle_target(root: &Path, path: &Path, fallback_id: &str) -> Result<PathBuf> {
    if !path.as_os_str().is_empty() {
        ensure_relative(path)?;
        return Ok(root.join(path));
    }
    ensure_safe_id(fallback_id)?;
    Ok(root.join("skills").join("by-id").join(fallback_id))
}

fn ensure_safe_id(id: &str) -> Result<()> {
    if id.is_empty() {
        return Err(MsError::ValidationFailed(
            "skill id must not be empty".to_string(),
        ));
    }
    if id.contains("..") || id.contains('/') || id.contains('\\') {
        return Err(MsError::ValidationFailed(format!(
            "skill id contains invalid characters: {id}"
        )));
    }
    Ok(())
}

fn ensure_relative(path: &Path) -> Result<()> {
    if path.is_absolute() {
        return Err(MsError::ValidationFailed(format!(
            "bundle path must be relative: {}",
            path.display()
        )));
    }
    for comp in path.components() {
        match comp {
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => {
                return Err(MsError::ValidationFailed(format!(
                    "bundle path contains invalid component: {}",
                    path.display()
                )));
            }
            _ => {}
        }
    }
    Ok(())
}

fn read_u64(input: &[u8], cursor: &mut usize) -> Result<u64> {
    if *cursor + 8 > input.len() {
        return Err(MsError::ValidationFailed(
            "bundle data truncated".to_string(),
        ));
    }
    let mut buf = [0u8; 8];
    buf.copy_from_slice(&input[*cursor..*cursor + 8]);
    *cursor += 8;
    Ok(u64::from_be_bytes(buf))
}

fn read_slice<'a>(input: &'a [u8], cursor: &mut usize, len: usize) -> Result<&'a [u8]> {
    if *cursor + len > input.len() {
        return Err(MsError::ValidationFailed(
            "bundle data truncated".to_string(),
        ));
    }
    let slice = &input[*cursor..*cursor + len];
    *cursor += len;
    Ok(slice)
}

fn backup_root(ctx: &AppContext, bundle_id: &str) -> PathBuf {
    let timestamp = chrono::Utc::now().format("%Y%m%d%H%M%S").to_string();
    ctx.git
        .root()
        .join("bundles")
        .join("backups")
        .join(bundle_id)
        .join(timestamp)
}

fn print_update_summary(updates: &[BundleUpdateItem]) {
    if updates.is_empty() {
        println!("No bundles to update.");
        return;
    }

    for update in updates {
        println!(
            "{} {} (current: {})",
            if update.applied { "✓" } else { "-" },
            update.bundle_id,
            update.current_version
        );
        if let Some(ref available) = update.available_version {
            println!("  available: {available}");
        }
        if !update.conflicts.is_empty() {
            println!("  conflicts:");
            for conflict in &update.conflicts {
                println!(
                    "    - {} (modified: {}, deleted: {}, conflict: {})",
                    conflict.skill_id, conflict.modified, conflict.deleted, conflict.conflict
                );
            }
        }
        if let Some(ref reason) = update.skipped_reason {
            println!("  skipped: {reason}");
        }
        if let Some(ref error) = update.error {
            println!("  error: {error}");
        }
    }
}

fn run_publish(ctx: &AppContext, args: &BundlePublishArgs) -> Result<()> {
    let repo = args
        .repo
        .clone()
        .ok_or_else(|| MsError::ValidationFailed("bundle publish requires --repo".to_string()))?;
    let config = GitHubConfig {
        repo,
        token: args.token.clone(),
        tag: args.tag.clone(),
        asset_name: args.asset_name.clone(),
        draft: args.draft,
        prerelease: args.prerelease,
    };
    let result = publish_bundle(std::path::Path::new(&args.path), &config)?;

    if ctx.output_format != OutputFormat::Human {
        return emit_json(&result);
    }

    println!("Published bundle to {}", result.repo);
    println!("Release: {}", result.release_url);
    println!("Asset: {} (tag {})", result.asset_name, result.tag);
    Ok(())
}

fn run_verify(ctx: &AppContext, args: &BundleVerifyArgs) -> Result<()> {
    let path = expand_local_path(&args.path.to_string_lossy());
    if !path.exists() {
        return Err(MsError::ValidationFailed(format!(
            "bundle file not found: {}",
            path.display()
        )));
    }
    let bytes = std::fs::read(&path)
        .map_err(|err| MsError::Config(format!("read {}: {err}", path.display())))?;
    let package = BundlePackage::from_bytes(&bytes)?;
    package.verify()?;

    let signed = !package.manifest.signatures.is_empty();
    let mut signatures_verified = false;
    if signed {
        let verifier = load_trusted_verifier(&ctx.ms_root, args.key.as_deref())?;
        package.verify_signatures(&verifier)?;
        signatures_verified = true;
    }

    let mut cached_blobs_verified = 0usize;
    if let Ok(store) = crate::bundler::blob::BlobStore::open(ctx.git.root().join("bundles")) {
        for blob in &package.blobs {
            if store.has_blob(&blob.hash) && store.verify_blob(&blob.hash).unwrap_or(false) {
                cached_blobs_verified += 1;
            }
        }
    }

    if ctx.output_format != OutputFormat::Human {
        return emit_json(&serde_json::json!({
            "status": "ok",
            "valid": true,
            "bundle_id": package.manifest.bundle.id,
            "version": package.manifest.bundle.version,
            "checksum": package.manifest.checksum,
            "signed": signed,
            "signatures_verified": signatures_verified,
            "signature_count": package.manifest.signatures.len(),
            "skills": package.manifest.skills.len(),
            "blobs": package.blobs.len(),
            "cached_blobs_verified": cached_blobs_verified,
        }));
    }

    println!(
        "✓ Bundle '{}' v{} is valid (checksum: {}, blobs: {}, cached_verified: {})",
        package.manifest.bundle.id,
        package.manifest.bundle.version,
        package.manifest.checksum.as_deref().unwrap_or("none"),
        package.blobs.len(),
        cached_blobs_verified
    );
    if signed {
        println!(
            "  ✓ {} Ed25519 signature(s) verified",
            package.manifest.signatures.len()
        );
    } else {
        println!("  ! Bundle is unsigned");
    }
    Ok(())
}

fn run_search(ctx: &AppContext, args: &BundleSearchArgs) -> Result<()> {
    let q = args.query.to_lowercase();
    let registry = BundleRegistry::open(ctx.git.root())?;

    let mut matches: Vec<serde_json::Value> = Vec::new();
    let mut seen_ids = HashSet::new();

    for inst in registry.list() {
        let id_match = inst.id.to_lowercase().contains(&q);
        let skill_match = inst.skills.iter().any(|s| s.to_lowercase().contains(&q));
        let source_match = inst.source.to_string().to_lowercase().contains(&q);
        if q.is_empty() || id_match || skill_match || source_match {
            seen_ids.insert(inst.id.clone());
            matches.push(serde_json::json!({
                "id": inst.id,
                "version": inst.version,
                "source": inst.source.to_string(),
                "skills": inst.skills,
                "installed": true,
            }));
        }
    }

    // Also scan <git_root>/bundles/*.msb if present
    let bundles_dir = ctx.git.root().join("bundles");
    if bundles_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&bundles_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("msb") {
                    if let Ok(bytes) = std::fs::read(&path) {
                        if let Ok(pkg) = BundlePackage::from_bytes(&bytes) {
                            let b = &pkg.manifest.bundle;
                            if seen_ids.contains(&b.id) {
                                continue;
                            }
                            let skills: Vec<String> =
                                pkg.manifest.skills.iter().map(|s| s.name.clone()).collect();
                            let matched = q.is_empty()
                                || b.id.to_lowercase().contains(&q)
                                || b.name.to_lowercase().contains(&q)
                                || b.description
                                    .as_deref()
                                    .unwrap_or("")
                                    .to_lowercase()
                                    .contains(&q)
                                || skills.iter().any(|s| s.to_lowercase().contains(&q));
                            if matched {
                                seen_ids.insert(b.id.clone());
                                matches.push(serde_json::json!({
                                    "id": b.id,
                                    "version": b.version,
                                    "source": path.display().to_string(),
                                    "skills": skills,
                                    "installed": false,
                                }));
                            }
                        }
                    }
                }
            }
        }
    }

    matches.truncate(args.limit);

    if ctx.output_format != OutputFormat::Human {
        return emit_json(&serde_json::json!({
            "status": "ok",
            "query": args.query,
            "count": matches.len(),
            "bundles": matches,
        }));
    }

    if matches.is_empty() {
        println!("No bundles matching '{}' found.", args.query);
    } else {
        println!("Bundles matching '{}' ({}):", args.query, matches.len());
        for m in &matches {
            let id = m["id"].as_str().unwrap_or("");
            let ver = m["version"].as_str().unwrap_or("");
            let inst = if m["installed"].as_bool().unwrap_or(false) {
                "installed"
            } else {
                "local"
            };
            println!("  - {id} v{ver} [{inst}]");
        }
    }
    Ok(())
}

fn load_trusted_verifier(
    ms_root: &Path,
    extra_key: Option<&Path>,
) -> Result<crate::bundler::Ed25519Verifier> {
    let mut verifier = crate::bundler::Ed25519Verifier::new();
    let trusted_dir = ms_root.join("trusted_keys");
    if trusted_dir.is_dir() {
        if let Ok(entries) = std::fs::read_dir(&trusted_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let _ = add_key_file_to_verifier(&mut verifier, &path);
                }
            }
        }
    }
    if let Some(key_path) = extra_key {
        add_key_file_to_verifier(&mut verifier, key_path)?;
    }
    Ok(verifier)
}

fn add_key_file_to_verifier(
    verifier: &mut crate::bundler::Ed25519Verifier,
    path: &Path,
) -> Result<()> {
    let raw = std::fs::read_to_string(path)
        .map_err(|err| MsError::Config(format!("read key {}: {err}", path.display())))?;
    let trimmed = raw.trim();

    // Case 1: OpenSSH private key file
    if trimmed.contains("BEGIN OPENSSH PRIVATE KEY") {
        let signer = crate::bundler::Ed25519Signer::from_openssh_str(trimmed)?;
        verifier.add_key(signer.key_id().to_string(), signer.public_key().to_vec());
        return Ok(());
    }

    // Case 2: OpenSSH public key line: "ssh-ed25519 <base64> [comment]"
    if trimmed.starts_with("ssh-ed25519 ") {
        use base64::Engine;
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        if parts.len() >= 2 {
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(parts[1])
                .map_err(|e| {
                    MsError::ValidationFailed(format!("invalid ssh-ed25519 base64: {e}"))
                })?;
            if decoded.len() >= 32 {
                let pub_bytes = decoded[decoded.len() - 32..].to_vec();
                let key_id = format!("ed25519:{}", hex::encode(&pub_bytes[..8]));
                verifier.add_key(key_id, pub_bytes);
                return Ok(());
            }
        }
    }

    // Case 3: 64-character hex-encoded 32-byte Ed25519 public key (optionally prefixed with "ed25519:")
    let hex_str = trimmed.strip_prefix("ed25519:").unwrap_or(trimmed);
    let pub_bytes = hex::decode(hex_str).map_err(|e| {
        MsError::ValidationFailed(format!(
            "unrecognized Ed25519 key format in {}: {e}",
            path.display()
        ))
    })?;
    if pub_bytes.len() != 32 {
        return Err(MsError::ValidationFailed(format!(
            "expected 32-byte Ed25519 public key in {}, got {} bytes",
            path.display(),
            pub_bytes.len()
        )));
    }
    let key_id = format!("ed25519:{}", hex::encode(&pub_bytes[..8]));
    verifier.add_key(key_id, pub_bytes);
    Ok(())
}

fn run_conflicts(ctx: &AppContext, args: &BundleConflictsArgs) -> Result<()> {
    let by_id_dir = ctx.git.root().join("skills/by-id");
    let legacy_dir = ctx.git.root().join("skills");
    let skills_dir = if by_id_dir.exists() {
        by_id_dir
    } else {
        legacy_dir
    };
    if !skills_dir.exists() {
        if ctx.output_format != OutputFormat::Human {
            return emit_json(&ConflictsReport {
                skills: vec![],
                total_modified: 0,
                total_conflicts: 0,
            });
        }
        println!("No skills installed.");
        return Ok(());
    }

    let mut reports = Vec::new();

    // Filter by --skill or --bundle if requested
    let skill_ids: Vec<String> = if let Some(ref skill) = args.skill {
        vec![skill.clone()]
    } else if let Some(ref bundle_id) = args.bundle {
        let registry = BundleRegistry::open(ctx.git.root())?;
        let installed = registry
            .get(bundle_id)
            .ok_or_else(|| MsError::NotFound(format!("bundle '{bundle_id}' is not installed")))?;
        installed.skills.clone()
    } else {
        // List all installed skills
        let mut ids = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&skills_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                        if name != "by-id" {
                            ids.push(name.to_string());
                        }
                    }
                }
            }
        }
        ids.sort();
        ids
    };

    for skill_id in skill_ids {
        let skill_path = skills_dir.join(&skill_id);
        if !skill_path.exists() {
            if args.skill.is_some() {
                return Err(MsError::SkillNotFound(format!(
                    "skill not found: {skill_id}"
                )));
            }
            continue;
        }

        // Try to load expected hashes from bundle metadata
        let meta_path = skill_path.join(".bundle_meta.json");
        let expected_hashes: HashMap<PathBuf, String> = if meta_path.exists() {
            match std::fs::read_to_string(&meta_path) {
                Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
                Err(_) => HashMap::new(),
            }
        } else {
            HashMap::new()
        };

        let report = detect_modifications(&skill_path, &skill_id, &expected_hashes)?;

        // Filter to modified only if requested
        if args.modified_only && report.status == ModificationStatus::Clean {
            continue;
        }

        reports.push(report);
    }

    let total_modified = reports
        .iter()
        .filter(|r| r.status != ModificationStatus::Clean)
        .count();
    let total_conflicts = reports.iter().map(|r| r.summary.conflict).sum();

    if ctx.output_format != OutputFormat::Human {
        return emit_json(&ConflictsReport {
            skills: reports,
            total_modified,
            total_conflicts,
        });
    }

    if reports.is_empty() {
        println!("No modifications detected.");
        return Ok(());
    }

    for report in &reports {
        println!("Skill: {}", report.skill_id);
        println!("  Status: {:?}", report.status);
        println!(
            "  Files: {} total, {} modified, {} new, {} deleted",
            report.summary.total(),
            report.summary.modified,
            report.summary.new,
            report.summary.deleted
        );

        if args.diff && !report.files.is_empty() {
            println!("  Changes:");
            for file in &report.files {
                let status_str = match file.status {
                    ModificationStatus::Clean => "clean",
                    ModificationStatus::Modified => "modified",
                    ModificationStatus::New => "new",
                    ModificationStatus::Deleted => "deleted",
                    ModificationStatus::Conflict => "conflict",
                };
                println!("    {} [{}]", file.path.display(), status_str);
            }
        }
        println!();
    }

    println!(
        "Summary: {total_modified} skill(s) with modifications, {total_conflicts} conflict(s)"
    );

    Ok(())
}

fn run_list(ctx: &AppContext) -> Result<()> {
    let registry = BundleRegistry::open(ctx.git.root())?;
    let installed: Vec<_> = registry.list().collect();

    if ctx.output_format != OutputFormat::Human {
        let bundles: Vec<_> = installed
            .iter()
            .map(|b| BundleListEntry {
                id: b.id.clone(),
                version: b.version.clone(),
                source: b.source.to_string(),
                skills: b.skills.clone(),
                installed_at: b.installed_at.to_rfc3339(),
            })
            .collect();
        return emit_json(&BundleListReportDetailed {
            count: bundles.len(),
            bundles,
        });
    }

    if installed.is_empty() {
        println!("No bundles installed.");
    } else {
        println!("Installed bundles:");
        for bundle in installed {
            println!("  {} v{}", bundle.id, bundle.version);
            println!("    Source: {}", bundle.source);
            println!("    Skills: {}", bundle.skills.join(", "));
            println!(
                "    Installed: {}",
                bundle.installed_at.format("%Y-%m-%d %H:%M")
            );
            println!();
        }
    }
    Ok(())
}

fn run_show(ctx: &AppContext, args: &BundleShowArgs) -> Result<()> {
    let local_path = expand_local_path(&args.source);
    let bytes = if local_path.exists() {
        std::fs::read(&local_path)
            .map_err(|err| MsError::Config(format!("read {}: {err}", local_path.display())))?
    } else if looks_like_path(&args.source) {
        return Err(MsError::ValidationFailed(format!(
            "bundle source not found: {}",
            local_path.display()
        )));
    } else if args.source.starts_with("http://") || args.source.starts_with("https://") {
        download_url(&args.source, args.token.clone())?
    } else if let Some((repo, tag)) = split_repo_tag(&args.source) {
        let tag = args.tag.as_deref().or(tag);
        let download = download_bundle(repo, tag, None, args.token.clone())?;
        download.bytes
    } else {
        return Err(MsError::ValidationFailed(format!(
            "bundle source not found: {}",
            args.source
        )));
    };
    let package = crate::bundler::package::BundlePackage::from_bytes(&bytes)?;
    let manifest = &package.manifest;

    if ctx.output_format != OutputFormat::Human {
        return emit_json(&BundleShowReport {
            id: manifest.bundle.id.clone(),
            name: manifest.bundle.name.clone(),
            version: manifest.bundle.version.clone(),
            description: manifest.bundle.description.clone(),
            authors: manifest.bundle.authors.clone(),
            license: manifest.bundle.license.clone(),
            repository: manifest.bundle.repository.clone(),
            keywords: manifest.bundle.keywords.clone(),
            ms_version: manifest.bundle.ms_version.clone(),
            skills: manifest.skills.iter().map(|s| s.name.clone()).collect(),
            skill_count: manifest.skills.len(),
            checksum: manifest.checksum.clone(),
            signed: !manifest.signatures.is_empty(),
        });
    }

    println!("Bundle: {} ({})", manifest.bundle.name, manifest.bundle.id);
    println!("Version: {}", manifest.bundle.version);
    if let Some(desc) = &manifest.bundle.description {
        println!("Description: {desc}");
    }
    if !manifest.bundle.authors.is_empty() {
        println!("Authors: {}", manifest.bundle.authors.join(", "));
    }
    if let Some(license) = &manifest.bundle.license {
        println!("License: {license}");
    }
    if let Some(repo) = &manifest.bundle.repository {
        println!("Repository: {repo}");
    }
    if !manifest.bundle.keywords.is_empty() {
        println!("Keywords: {}", manifest.bundle.keywords.join(", "));
    }
    if let Some(ms_ver) = &manifest.bundle.ms_version {
        println!("MS Version: {ms_ver}");
    }

    println!("\nSkills ({}):", manifest.skills.len());
    for skill in &manifest.skills {
        let version_str = skill.version.as_deref().unwrap_or("-");
        let optional_str = if skill.optional { " (optional)" } else { "" };
        println!("  - {} v{}{}", skill.name, version_str, optional_str);
    }

    if let Some(checksum) = &manifest.checksum {
        println!("\nChecksum: {checksum}");
    }
    if manifest.signatures.is_empty() {
        println!("Signed: no");
    } else {
        println!("Signed: yes ({} signature(s))", manifest.signatures.len());
    }

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

/// Discover skills in a directory by looking for subdirectories containing SKILL.md
fn discover_skills_in_dir(dir: &std::path::Path) -> Result<Vec<String>> {
    if !dir.exists() {
        return Err(MsError::ValidationFailed(format!(
            "directory not found: {}",
            dir.display()
        )));
    }

    if !dir.is_dir() {
        return Err(MsError::ValidationFailed(format!(
            "not a directory: {}",
            dir.display()
        )));
    }

    let mut skills = Vec::new();
    let entries = std::fs::read_dir(dir)
        .map_err(|err| MsError::Config(format!("read {}: {err}", dir.display())))?;

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        // Check if this directory contains SKILL.md (indicating it's a skill)
        let skill_md = path.join("SKILL.md");
        if skill_md.exists() {
            if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                skills.push(name.to_string());
            }
        }
    }

    skills.sort();
    Ok(skills)
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
            println!("  - {skill}");
        }
    }
    if !report.skipped.is_empty() {
        println!("Skipped:");
        for skill in &report.skipped {
            println!("  - {skill}");
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

#[derive(serde::Serialize)]
struct BundleListReportDetailed {
    bundles: Vec<BundleListEntry>,
    count: usize,
}

#[derive(serde::Serialize)]
struct BundleListEntry {
    id: String,
    version: String,
    source: String,
    skills: Vec<String>,
    installed_at: String,
}

#[derive(serde::Serialize)]
struct BundleShowReport {
    id: String,
    name: String,
    version: String,
    description: Option<String>,
    authors: Vec<String>,
    license: Option<String>,
    repository: Option<String>,
    keywords: Vec<String>,
    ms_version: Option<String>,
    skills: Vec<String>,
    skill_count: usize,
    checksum: Option<String>,
    signed: bool,
}

#[derive(serde::Serialize)]
struct ConflictsReport {
    skills: Vec<SkillModificationReport>,
    total_modified: usize,
    total_conflicts: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ==================== Helper Function Tests ====================

    #[test]
    fn test_normalize_skill_list_simple() {
        let input = vec!["skill1".to_string(), "skill2".to_string()];
        let result = normalize_skill_list(&input);
        assert_eq!(result, vec!["skill1", "skill2"]);
    }

    #[test]
    fn test_normalize_skill_list_comma_separated() {
        let input = vec!["skill1,skill2,skill3".to_string()];
        let result = normalize_skill_list(&input);
        assert_eq!(result, vec!["skill1", "skill2", "skill3"]);
    }

    #[test]
    fn test_normalize_skill_list_mixed() {
        let input = vec!["skill1,skill2".to_string(), "skill3".to_string()];
        let result = normalize_skill_list(&input);
        assert_eq!(result, vec!["skill1", "skill2", "skill3"]);
    }

    #[test]
    fn test_normalize_skill_list_trims_whitespace() {
        let input = vec![" skill1 , skill2 ".to_string()];
        let result = normalize_skill_list(&input);
        assert_eq!(result, vec!["skill1", "skill2"]);
    }

    #[test]
    fn test_normalize_skill_list_deduplicates() {
        let input = vec!["skill1,skill2,skill1".to_string()];
        let result = normalize_skill_list(&input);
        assert_eq!(result, vec!["skill1", "skill2"]);
    }

    #[test]
    fn test_normalize_skill_list_empty() {
        let input: Vec<String> = vec![];
        let result = normalize_skill_list(&input);
        assert!(result.is_empty());
    }

    #[test]
    fn test_normalize_skill_list_empty_strings() {
        let input = vec![",,".to_string(), "".to_string()];
        let result = normalize_skill_list(&input);
        assert!(result.is_empty());
    }

    // ==================== Slugify Tests ====================

    #[test]
    fn test_slugify_simple() {
        assert_eq!(slugify("My Bundle"), "my-bundle");
    }

    #[test]
    fn test_slugify_already_slug() {
        assert_eq!(slugify("my-bundle"), "my-bundle");
    }

    #[test]
    fn test_slugify_uppercase() {
        assert_eq!(slugify("MY_BUNDLE"), "my-bundle");
    }

    #[test]
    fn test_slugify_special_chars() {
        assert_eq!(slugify("My!@#Bundle$%^Name"), "my-bundle-name");
    }

    #[test]
    fn test_slugify_consecutive_special() {
        assert_eq!(slugify("my!!!bundle"), "my-bundle");
    }

    #[test]
    fn test_slugify_leading_trailing_special() {
        assert_eq!(slugify("!!!my-bundle!!!"), "my-bundle");
    }

    #[test]
    fn test_slugify_numbers() {
        assert_eq!(slugify("Bundle v2.0"), "bundle-v2-0");
    }

    #[test]
    fn test_slugify_empty() {
        assert_eq!(slugify(""), "bundle");
    }

    #[test]
    fn test_slugify_only_special_chars() {
        assert_eq!(slugify("!@#$%"), "bundle");
    }

    // ==================== Split Repo Tag Tests ====================

    #[test]
    fn test_split_repo_tag_with_tag() {
        let result = split_repo_tag("owner/repo@v1.0.0");
        assert_eq!(result, Some(("owner/repo", Some("v1.0.0"))));
    }

    #[test]
    fn test_split_repo_tag_without_tag() {
        let result = split_repo_tag("owner/repo");
        assert_eq!(result, Some(("owner/repo", None)));
    }

    #[test]
    fn test_split_repo_tag_no_slash() {
        let result = split_repo_tag("just-a-name");
        assert_eq!(result, None);
    }

    #[test]
    fn test_split_repo_tag_nested_path() {
        let result = split_repo_tag("owner/repo/subpath@v2.0");
        assert_eq!(result, Some(("owner/repo/subpath", Some("v2.0"))));
    }

    // ==================== Looks Like Path Tests ====================

    #[test]
    fn test_looks_like_path_absolute() {
        assert!(looks_like_path("/absolute/path"));
        assert!(looks_like_path("/"));
    }

    #[test]
    fn test_looks_like_path_relative() {
        assert!(looks_like_path("./relative/path"));
        assert!(looks_like_path("../parent/path"));
    }

    #[test]
    fn test_looks_like_path_home() {
        assert!(looks_like_path("~"));
        assert!(looks_like_path("~/path/to/file"));
    }

    #[test]
    fn test_looks_like_path_not_a_path() {
        assert!(!looks_like_path("owner/repo"));
        assert!(!looks_like_path("just-a-name"));
        assert!(!looks_like_path("https://example.com"));
    }

    // ==================== Expand Local Path Tests ====================

    #[test]
    fn test_expand_local_path_relative() {
        let result = expand_local_path("./relative");
        assert_eq!(result, PathBuf::from("./relative"));
    }

    #[test]
    fn test_expand_local_path_absolute() {
        let result = expand_local_path("/absolute/path");
        assert_eq!(result, PathBuf::from("/absolute/path"));
    }

    #[test]
    fn test_expand_local_path_home_tilde() {
        let result = expand_local_path("~");
        // Should either be home dir or "~" if no home dir
        if let Some(home) = dirs::home_dir() {
            assert_eq!(result, home);
        } else {
            assert_eq!(result, PathBuf::from("~"));
        }
    }

    #[test]
    fn test_expand_local_path_home_subpath() {
        let result = expand_local_path("~/subpath");
        if let Some(home) = dirs::home_dir() {
            assert_eq!(result, home.join("subpath"));
        } else {
            assert_eq!(result, PathBuf::from("~/subpath"));
        }
    }

    // ==================== Discover Skills In Dir Tests ====================

    #[test]
    fn test_discover_skills_in_dir_finds_skills() {
        let temp = TempDir::new().unwrap();

        // Create skill directories with SKILL.md
        let skill1_dir = temp.path().join("skill-one");
        fs::create_dir(&skill1_dir).unwrap();
        fs::write(skill1_dir.join("SKILL.md"), "# Skill One").unwrap();

        let skill2_dir = temp.path().join("skill-two");
        fs::create_dir(&skill2_dir).unwrap();
        fs::write(skill2_dir.join("SKILL.md"), "# Skill Two").unwrap();

        let result = discover_skills_in_dir(temp.path()).unwrap();
        assert_eq!(result.len(), 2);
        assert!(result.contains(&"skill-one".to_string()));
        assert!(result.contains(&"skill-two".to_string()));
    }

    #[test]
    fn test_discover_skills_in_dir_ignores_non_skills() {
        let temp = TempDir::new().unwrap();

        // Create skill directory with SKILL.md
        let skill_dir = temp.path().join("real-skill");
        fs::create_dir(&skill_dir).unwrap();
        fs::write(skill_dir.join("SKILL.md"), "# Real Skill").unwrap();

        // Create directory without SKILL.md
        let non_skill_dir = temp.path().join("not-a-skill");
        fs::create_dir(&non_skill_dir).unwrap();
        fs::write(non_skill_dir.join("README.md"), "# Not a skill").unwrap();

        // Create a regular file (not a directory)
        fs::write(temp.path().join("file.txt"), "just a file").unwrap();

        let result = discover_skills_in_dir(temp.path()).unwrap();
        assert_eq!(result, vec!["real-skill"]);
    }

    #[test]
    fn test_discover_skills_in_dir_empty() {
        let temp = TempDir::new().unwrap();
        let result = discover_skills_in_dir(temp.path()).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_discover_skills_in_dir_not_found() {
        let result = discover_skills_in_dir(std::path::Path::new("/nonexistent/path"));
        assert!(result.is_err());
    }

    #[test]
    fn test_discover_skills_in_dir_not_a_directory() {
        let temp = TempDir::new().unwrap();
        let file_path = temp.path().join("file.txt");
        fs::write(&file_path, "content").unwrap();

        let result = discover_skills_in_dir(&file_path);
        assert!(result.is_err());
    }

    #[test]
    fn test_discover_skills_in_dir_sorted() {
        let temp = TempDir::new().unwrap();

        // Create skills in non-alphabetical order
        for name in ["zebra-skill", "alpha-skill", "middle-skill"] {
            let dir = temp.path().join(name);
            fs::create_dir(&dir).unwrap();
            fs::write(dir.join("SKILL.md"), format!("# {}", name)).unwrap();
        }

        let result = discover_skills_in_dir(temp.path()).unwrap();
        assert_eq!(result, vec!["alpha-skill", "middle-skill", "zebra-skill"]);
    }

    // ==================== Argument Parsing Tests ====================

    #[test]
    fn test_bundle_create_args_default_version() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: BundleCommand,
        }

        let args = TestCli::parse_from(["test", "create", "my-bundle", "--skills", "skill1"]);
        if let BundleCommand::Create(create) = args.cmd {
            assert_eq!(create.name, "my-bundle");
            assert_eq!(create.bundle_version, "0.1.0"); // default
            assert_eq!(create.skills, vec!["skill1"]);
            assert!(!create.sign);
            assert!(!create.write_manifest);
        } else {
            panic!("Expected Create command");
        }
    }

    #[test]
    fn test_bundle_create_args_with_options() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: BundleCommand,
        }

        let args = TestCli::parse_from([
            "test",
            "create",
            "my-bundle",
            "--skills",
            "skill1,skill2",
            "--bundle-version",
            "1.0.0",
            "--id",
            "custom-id",
            "--write-manifest",
        ]);

        if let BundleCommand::Create(create) = args.cmd {
            assert_eq!(create.name, "my-bundle");
            assert_eq!(create.bundle_version, "1.0.0");
            assert_eq!(create.id, Some("custom-id".to_string()));
            assert!(create.write_manifest);
        } else {
            panic!("Expected Create command");
        }
    }

    #[test]
    fn test_bundle_install_args_defaults() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: BundleCommand,
        }

        let args = TestCli::parse_from(["test", "install", "./bundle.msb"]);
        if let BundleCommand::Install(install) = args.cmd {
            assert_eq!(install.source, "./bundle.msb");
            assert!(!install.no_verify);
            assert!(!install.force);
            assert!(install.skills.is_empty());
        } else {
            panic!("Expected Install command");
        }
    }

    #[test]
    fn test_bundle_install_args_with_options() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: BundleCommand,
        }

        let args = TestCli::parse_from([
            "test",
            "install",
            "owner/repo@v1.0.0",
            "--skills",
            "skill1",
            "--force",
            "--no-verify",
        ]);

        if let BundleCommand::Install(install) = args.cmd {
            assert_eq!(install.source, "owner/repo@v1.0.0");
            assert!(install.force);
            assert!(install.no_verify);
            assert_eq!(install.skills, vec!["skill1"]);
        } else {
            panic!("Expected Install command");
        }
    }

    #[test]
    fn test_bundle_remove_args() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: BundleCommand,
        }

        let args =
            TestCli::parse_from(["test", "remove", "my-bundle", "--remove-skills", "--force"]);

        if let BundleCommand::Remove(remove) = args.cmd {
            assert_eq!(remove.bundle_id, "my-bundle");
            assert!(remove.remove_skills);
            assert!(remove.force);
        } else {
            panic!("Expected Remove command");
        }
    }

    #[test]
    fn test_bundle_publish_args() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: BundleCommand,
        }

        let args = TestCli::parse_from([
            "test",
            "publish",
            "./bundle.msb",
            "--repo",
            "owner/repo",
            "--tag",
            "v1.0.0",
            "--draft",
        ]);

        if let BundleCommand::Publish(publish) = args.cmd {
            assert_eq!(publish.path, "./bundle.msb");
            assert_eq!(publish.repo, Some("owner/repo".to_string()));
            assert_eq!(publish.tag, Some("v1.0.0".to_string()));
            assert!(publish.draft);
            assert!(!publish.prerelease);
        } else {
            panic!("Expected Publish command");
        }
    }

    #[test]
    fn test_bundle_show_args() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: BundleCommand,
        }

        let args = TestCli::parse_from(["test", "show", "owner/repo@v1.0.0", "--tag", "latest"]);

        if let BundleCommand::Show(show) = args.cmd {
            assert_eq!(show.source, "owner/repo@v1.0.0");
            assert_eq!(show.tag, Some("latest".to_string()));
        } else {
            panic!("Expected Show command");
        }
    }

    #[test]
    fn test_bundle_conflicts_args() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: BundleCommand,
        }

        let args = TestCli::parse_from([
            "test",
            "conflicts",
            "--skill",
            "my-skill",
            "--modified-only",
            "--diff",
        ]);

        if let BundleCommand::Conflicts(conflicts) = args.cmd {
            assert_eq!(conflicts.skill, Some("my-skill".to_string()));
            assert!(conflicts.modified_only);
            assert!(conflicts.diff);
        } else {
            panic!("Expected Conflicts command");
        }
    }

    #[test]
    fn test_bundle_update_args_defaults() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: BundleCommand,
        }

        let args = TestCli::parse_from(["test", "update"]);
        if let BundleCommand::Update(update) = args.cmd {
            assert!(update.bundle_id.is_none());
            assert!(!update.check);
            assert!(!update.all);
            assert!(!update.dry_run);
            assert!(!update.force);
            assert!(!update.no_verify);
        } else {
            panic!("Expected Update command");
        }
    }

    #[test]
    fn test_bundle_update_args_with_options() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: BundleCommand,
        }

        let args = TestCli::parse_from([
            "test",
            "update",
            "bundle-1",
            "--check",
            "--all",
            "--dry-run",
            "--force",
            "--token",
            "ghp_123",
            "--no-verify",
        ]);
        if let BundleCommand::Update(update) = args.cmd {
            assert_eq!(update.bundle_id.as_deref(), Some("bundle-1"));
            assert!(update.check);
            assert!(update.all);
            assert!(update.dry_run);
            assert!(update.force);
            assert_eq!(update.token.as_deref(), Some("ghp_123"));
            assert!(update.no_verify);
        } else {
            panic!("Expected Update command");
        }
    }

    #[test]
    fn test_bundle_list_command() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: BundleCommand,
        }

        let args = TestCli::parse_from(["test", "list"]);
        assert!(matches!(args.cmd, BundleCommand::List));
    }

    // ==================== Argument Conflict Tests ====================

    #[test]
    fn test_bundle_create_skills_and_from_dir_conflict() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: BundleCommand,
        }

        // --skills and --from-dir should conflict
        let result = TestCli::try_parse_from([
            "test",
            "create",
            "my-bundle",
            "--skills",
            "skill1",
            "--from-dir",
            "/some/path",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn test_bundle_create_sign_key_requires_sign() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: BundleCommand,
        }

        // --sign-key requires --sign
        let result = TestCli::try_parse_from([
            "test",
            "create",
            "my-bundle",
            "--skills",
            "skill1",
            "--sign-key",
            "/path/to/key",
        ]);
        assert!(result.is_err());
    }

    #[test]
    fn test_bundle_create_sign_with_key() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: BundleCommand,
        }

        // --sign with --sign-key should work
        let args = TestCli::parse_from([
            "test",
            "create",
            "my-bundle",
            "--skills",
            "skill1",
            "--sign",
            "--sign-key",
            "/path/to/key",
        ]);
        if let BundleCommand::Create(create) = args.cmd {
            assert!(create.sign);
            assert_eq!(create.sign_key, Some(PathBuf::from("/path/to/key")));
        } else {
            panic!("Expected Create command");
        }
    }

    #[test]
    fn test_bundle_install_force_short_flag() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: BundleCommand,
        }

        // Test -f short flag for --force
        let args = TestCli::parse_from(["test", "install", "bundle.msb", "-f"]);
        if let BundleCommand::Install(install) = args.cmd {
            assert!(install.force);
        } else {
            panic!("Expected Install command");
        }
    }

    #[test]
    fn test_bundle_remove_force_short_flag() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: BundleCommand,
        }

        // Test -f short flag for --force
        let args = TestCli::parse_from(["test", "remove", "my-bundle", "-f"]);
        if let BundleCommand::Remove(remove) = args.cmd {
            assert!(remove.force);
        } else {
            panic!("Expected Remove command");
        }
    }

    #[test]
    fn test_bundle_publish_prerelease_and_draft() {
        use clap::Parser;

        #[derive(Parser)]
        struct TestCli {
            #[command(subcommand)]
            cmd: BundleCommand,
        }

        // Both --draft and --prerelease should work together
        let args = TestCli::parse_from([
            "test",
            "publish",
            "bundle.msb",
            "--repo",
            "owner/repo",
            "--draft",
            "--prerelease",
        ]);
        if let BundleCommand::Publish(publish) = args.cmd {
            assert!(publish.draft);
            assert!(publish.prerelease);
        } else {
            panic!("Expected Publish command");
        }
    }

    // ==================== Helper Edge Cases ====================

    #[test]
    fn test_normalize_skill_list_with_extra_commas() {
        let input = vec!["skill1,,skill2,,,skill3".to_string()];
        let result = normalize_skill_list(&input);
        assert_eq!(result, vec!["skill1", "skill2", "skill3"]);
    }

    #[test]
    fn test_normalize_skill_list_whitespace_only() {
        let input = vec!["   ,  ,   ".to_string()];
        let result = normalize_skill_list(&input);
        assert!(result.is_empty());
    }

    #[test]
    fn test_slugify_unicode() {
        // Unicode chars should be stripped (not alphanumeric ASCII)
        assert_eq!(slugify("café bundle"), "caf-bundle");
    }

    #[test]
    fn test_slugify_mixed_separators() {
        assert_eq!(slugify("my_bundle-name.v1"), "my-bundle-name-v1");
    }

    #[test]
    fn test_split_repo_tag_at_in_tag() {
        // Handle @ in tag name (edge case)
        let result = split_repo_tag("owner/repo@v1.0.0@special");
        // First @ splits, rest is tag
        assert_eq!(result, Some(("owner/repo", Some("v1.0.0@special"))));
    }

    #[test]
    fn test_split_repo_tag_org_repo() {
        // GitHub org/repo format
        let result = split_repo_tag("anthropic/claude-code@latest");
        assert_eq!(result, Some(("anthropic/claude-code", Some("latest"))));
    }

    #[test]
    fn test_looks_like_path_windows_style() {
        // Windows-style paths shouldn't be recognized (we're on Unix)
        assert!(!looks_like_path("C:\\Users\\name"));
    }

    #[test]
    fn test_expand_local_path_empty() {
        let result = expand_local_path("");
        assert_eq!(result, PathBuf::from(""));
    }

    // ==================== Serialization Tests ====================

    #[test]
    fn test_bundle_create_report_serialization() {
        let report = BundleCreateReport {
            id: "test-bundle".to_string(),
            output: "/path/to/output.msb".to_string(),
            manifest_path: Some("/path/to/manifest.toml".to_string()),
            checksum: Some("sha256:abc123".to_string()),
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"id\":\"test-bundle\""));
        assert!(json.contains("\"checksum\":\"sha256:abc123\""));
    }

    #[test]
    fn test_bundle_show_report_serialization() {
        let report = BundleShowReport {
            id: "test".to_string(),
            name: "Test Bundle".to_string(),
            version: "1.0.0".to_string(),
            description: Some("A test bundle".to_string()),
            authors: vec!["author1".to_string()],
            license: Some("MIT".to_string()),
            repository: None,
            keywords: vec!["test".to_string()],
            ms_version: Some("0.1.0".to_string()),
            skills: vec!["skill1".to_string(), "skill2".to_string()],
            skill_count: 2,
            checksum: Some("sha256:xyz".to_string()),
            signed: false,
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"signed\":false"));
        assert!(json.contains("\"skill_count\":2"));
    }

    #[test]
    fn test_bundle_list_entry_serialization() {
        let entry = BundleListEntry {
            id: "bundle-1".to_string(),
            version: "1.0.0".to_string(),
            source: "owner/repo".to_string(),
            skills: vec!["skill1".to_string()],
            installed_at: "2026-01-14T12:00:00Z".to_string(),
        };
        let json = serde_json::to_string(&entry).unwrap();
        assert!(json.contains("\"id\":\"bundle-1\""));
        assert!(json.contains("\"installed_at\":\"2026-01-14T12:00:00Z\""));
    }

    // ==================== Conflicts Report Tests ====================

    #[test]
    fn test_conflicts_report_serialization() {
        let report = ConflictsReport {
            skills: vec![],
            total_modified: 0,
            total_conflicts: 0,
        };
        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"total_modified\":0"));
        assert!(json.contains("\"total_conflicts\":0"));
    }
}
