use clap::Parser;

use ms::cli::commands;
use ms::cli::{Cli, Commands};

fn parse(args: &[&str]) -> Commands {
    let mut argv = vec!["ms"];
    argv.extend_from_slice(args);
    Cli::parse_from(argv).command
}

#[test]
fn parse_config_list() {
    match parse(&["config", "--list"]) {
        Commands::Config(args) => {
            assert!(args.list);
            assert!(args.key.is_none());
            assert!(args.value.is_none());
            assert!(!args.unset);
        }
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn parse_config_set_value() {
    match parse(&["config", "search.use_embeddings", "false"]) {
        Commands::Config(args) => {
            assert_eq!(args.key.as_deref(), Some("search.use_embeddings"));
            assert_eq!(args.value.as_deref(), Some("false"));
            assert!(!args.list);
            assert!(!args.unset);
        }
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn parse_diff_flags() {
    match parse(&[
        "diff",
        "skill-a",
        "skill-b",
        "--structure-only",
        "--format",
        "json",
    ]) {
        Commands::Diff(args) => {
            assert_eq!(args.skill_a, "skill-a");
            assert_eq!(args.skill_b, "skill-b");
            assert!(args.structure_only);
            assert_eq!(args.format, "json");
        }
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn parse_fmt_flags() {
    match parse(&["fmt", "skill-a", "skill-b", "--check", "--diff"]) {
        Commands::Fmt(args) => {
            assert_eq!(
                args.skills,
                vec!["skill-a".to_string(), "skill-b".to_string()]
            );
            assert!(args.check);
            assert!(args.diff);
        }
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn parse_edit_flags() {
    match parse(&["edit", "skill-a", "--editor", "vim", "--meta"]) {
        Commands::Edit(args) => {
            assert_eq!(args.skill, "skill-a");
            assert_eq!(args.editor.as_deref(), Some("vim"));
            assert!(args.meta);
        }
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn parse_alias_list_shortcut() {
    match parse(&["alias", "--list"]) {
        Commands::Alias(args) => {
            assert!(args.list);
            assert!(args.command.is_none());
        }
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn parse_alias_add() {
    match parse(&["alias", "add", "old", "--target", "new", "--kind", "legacy"]) {
        Commands::Alias(args) => match args.command {
            Some(commands::alias::AliasCommand::Add {
                alias,
                target,
                kind,
            }) => {
                assert_eq!(alias, "old");
                assert_eq!(target, "new");
                assert_eq!(kind, "legacy");
            }
            other => panic!("unexpected alias command: {:?}", other),
        },
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn parse_bandit_stats_path() {
    match parse(&["bandit", "stats", "--path", "/tmp/bandit.json"]) {
        Commands::Bandit(args) => match args.command {
            commands::bandit::BanditCommand::Stats(stats) => {
                assert_eq!(
                    stats.path.as_deref(),
                    Some(std::path::Path::new("/tmp/bandit.json"))
                );
            }
            other => panic!("unexpected bandit command: {:?}", other),
        },
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn parse_cm_rules_filters() {
    match parse(&["cm", "rules", "--category", "debug", "--limit", "5"]) {
        Commands::Cm(args) => match args.command {
            commands::cm::CmCommand::Rules { category, limit } => {
                assert_eq!(category.as_deref(), Some("debug"));
                assert_eq!(limit, 5);
            }
            other => panic!("unexpected cm command: {:?}", other),
        },
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn parse_migrate_with_check() {
    match parse(&["migrate", "skill-a", "skill-b", "--check"]) {
        Commands::Migrate(args) => {
            assert_eq!(
                args.skills,
                vec!["skill-a".to_string(), "skill-b".to_string()]
            );
            assert!(args.check);
        }
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn parse_pre_commit_flags() {
    match parse(&["pre-commit", "--repo", "/tmp/repo", "--only", "rust"]) {
        Commands::PreCommit(args) => {
            assert_eq!(
                args.repo.as_deref(),
                Some(std::path::Path::new("/tmp/repo"))
            );
            assert_eq!(args.only.as_deref(), Some("rust"));
        }
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn parse_quality_args() {
    match parse(&["quality", "skill-a", "--update"]) {
        Commands::Quality(args) => {
            assert_eq!(args.skill.as_deref(), Some("skill-a"));
            assert!(!args.all);
            assert!(args.update);
        }
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn parse_security_scan_args() {
    match parse(&[
        "security",
        "scan",
        "--input",
        "hello",
        "--source",
        "tool",
        "--audit-mode",
        "--session-id",
        "sess-1",
        "--message-index",
        "2",
    ]) {
        Commands::Security(args) => match args.command {
            commands::security::SecurityCommand::Scan(scan) => {
                assert_eq!(scan.input.as_deref(), Some("hello"));
                assert!(scan.input_file.is_none());
                assert_eq!(scan.source, "tool");
                assert!(scan.persist);
                assert!(scan.audit_mode);
                assert_eq!(scan.session_id.as_deref(), Some("sess-1"));
                assert_eq!(scan.message_index, 2);
            }
            other => panic!("unexpected security command: {:?}", other),
        },
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn parse_template_apply_args() {
    match parse(&[
        "template",
        "apply",
        "debugging",
        "--id",
        "debug-skill",
        "--name",
        "Debug Skill",
        "--description",
        "Diagnose failures",
        "--tag",
        "rust,build",
        "--layer",
        "project",
    ]) {
        Commands::Template(args) => match args.command {
            commands::template::TemplateCommand::Apply(apply) => {
                assert_eq!(apply.template, "debugging");
                assert_eq!(apply.id, "debug-skill");
                assert_eq!(apply.name, "Debug Skill");
                assert_eq!(apply.description, "Diagnose failures");
                assert_eq!(apply.tags, vec!["rust".to_string(), "build".to_string()]);
                assert_eq!(apply.layer, "project");
            }
            other => panic!("unexpected template command: {:?}", other),
        },
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn parse_test_args() {
    match parse(&[
        "test",
        "--all",
        "--tags",
        "smoke,fast",
        "--exclude-tags",
        "slow",
        "--timeout",
        "2m",
        "--fail-fast",
    ]) {
        Commands::Test(args) => {
            assert!(args.all);
            assert_eq!(args.tags.as_deref(), Some("smoke,fast"));
            assert_eq!(args.exclude_tags.as_deref(), Some("slow"));
            assert_eq!(args.timeout.as_deref(), Some("2m"));
            assert!(args.fail_fast);
        }
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn parse_update_args() {
    match parse(&[
        "update",
        "--check",
        "--channel",
        "beta",
        "--target-version",
        "0.2.0",
        "--force",
    ]) {
        Commands::Update(args) => {
            assert!(args.check);
            assert!(args.force);
            assert_eq!(args.channel.as_deref(), Some("beta"));
            assert_eq!(args.target_version.as_deref(), Some("0.2.0"));
        }
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn parse_validate_args() {
    match parse(&["validate", "skill-a", "--ubs"]) {
        Commands::Validate(args) => {
            assert_eq!(args.skill, "skill-a");
            assert!(args.ubs);
        }
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn parse_remote_add_git_flags() {
    match parse(&[
        "remote",
        "add",
        "origin",
        "https://example.com/skills.git",
        "--remote-type",
        "git",
        "--branch",
        "main",
        "--auth",
        "token",
        "--token-env",
        "GIT_TOKEN",
        "--direction",
        "push-only",
    ]) {
        Commands::Remote(args) => match args.command {
            commands::remote::RemoteCommand::Add(add) => {
                assert_eq!(add.name, "origin");
                assert_eq!(add.url, "https://example.com/skills.git");
                assert_eq!(add.remote_type, "git");
                assert_eq!(add.branch.as_deref(), Some("main"));
                assert_eq!(add.auth.as_deref(), Some("token"));
                assert_eq!(add.token_env.as_deref(), Some("GIT_TOKEN"));
                assert_eq!(add.direction.as_deref(), Some("push-only"));
            }
            other => panic!("unexpected remote command: {:?}", other),
        },
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn parse_backup_create_args() {
    match parse(&["backup", "create", "--id", "snap-1"]) {
        Commands::Backup(args) => match args.command {
            commands::backup::BackupCommand::Create(create) => {
                assert_eq!(create.id.as_deref(), Some("snap-1"));
            }
            other => panic!("unexpected backup command: {:?}", other),
        },
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn parse_backup_list_args() {
    match parse(&["backup", "list", "--limit", "5"]) {
        Commands::Backup(args) => match args.command {
            commands::backup::BackupCommand::List(list) => {
                assert_eq!(list.limit, 5);
            }
            other => panic!("unexpected backup command: {:?}", other),
        },
        other => panic!("unexpected command: {:?}", other),
    }
}

#[test]
fn parse_backup_restore_args() {
    match parse(&["backup", "restore", "snap-1", "--approve"]) {
        Commands::Backup(args) => match args.command {
            commands::backup::BackupCommand::Restore(restore) => {
                assert_eq!(restore.id.as_deref(), Some("snap-1"));
                assert!(restore.approve);
                assert!(!restore.latest);
            }
            other => panic!("unexpected backup command: {:?}", other),
        },
        other => panic!("unexpected command: {:?}", other),
    }
}
