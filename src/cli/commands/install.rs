//! ms install - Install a bundle from a path, URL, or repo shorthand.

use clap::Args;

use crate::app::AppContext;
use crate::cli::commands::bundle::{BundleArgs, BundleCommand, BundleInstallArgs};
use crate::error::Result;

#[derive(Args, Debug)]
pub struct InstallArgs {
    /// Bundle source (path, URL, or repo shorthand)
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

    /// Skip signature and checksum verification (not recommended)
    #[arg(long)]
    pub no_verify: bool,

    /// Force reinstallation if bundle is already installed
    #[arg(long, short = 'f')]
    pub force: bool,
}

pub fn run(ctx: &AppContext, args: &InstallArgs) -> Result<()> {
    let install = BundleInstallArgs {
        source: args.source.clone(),
        skills: args.skills.clone(),
        token: args.token.clone(),
        tag: args.tag.clone(),
        asset_name: args.asset_name.clone(),
        no_verify: args.no_verify,
        force: args.force,
    };

    let bundle_args = BundleArgs {
        command: BundleCommand::Install(install),
    };

    crate::cli::commands::bundle::run(ctx, &bundle_args)
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
        Install(InstallArgs),
    }

    #[test]
    fn parse_install_args() {
        let parsed = TestCli::parse_from([
            "test",
            "install",
            "owner/repo@v1.0.0",
            "--skills",
            "skill-a",
            "--skills",
            "skill-b",
            "--token",
            "tok",
            "--tag",
            "v1.0.0",
            "--asset-name",
            "bundle.msb",
            "--no-verify",
            "-f",
        ]);
        let TestCommand::Install(args) = parsed.cmd;
        assert_eq!(args.source, "owner/repo@v1.0.0");
        assert_eq!(
            args.skills,
            vec!["skill-a".to_string(), "skill-b".to_string()]
        );
        assert_eq!(args.token.as_deref(), Some("tok"));
        assert_eq!(args.tag.as_deref(), Some("v1.0.0"));
        assert_eq!(args.asset_name.as_deref(), Some("bundle.msb"));
        assert!(args.no_verify);
        assert!(args.force);
    }
}
