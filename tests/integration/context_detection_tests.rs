//! Comprehensive tests for context detection and auto-loading.
//!
//! Tests cover:
//! - Project detector edge cases (symlinks, permissions, nested projects)
//! - Context collector caching behavior
//! - Context fingerprinting stability
//! - Relevance scorer edge cases

use std::collections::HashSet;
use std::fs::{self, File, Permissions};
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;

use ms::context::{
    CollectedContext, ContextCollector, ContextCollectorConfig, DefaultDetector, DetectedProject,
    ProjectDetector, ProjectMarker, ProjectType, RelevanceScorer, ScoringContext, ScoringWeights,
};
use ms::core::skill::{ContextSignal, ContextTags, SkillMetadata};

// =============================================================================
// ProjectDetector Tests - Edge Cases
// =============================================================================

mod project_detector_tests {
    use super::*;

    fn setup_project(files: &[&str]) -> TempDir {
        let dir = TempDir::new().unwrap();
        for file in files {
            let path = dir.path().join(file);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            File::create(&path).unwrap();
        }
        dir
    }

    #[test]
    fn detect_nested_rust_project() {
        // Rust project inside a monorepo
        let dir = TempDir::new().unwrap();
        let nested = dir.path().join("packages/my-crate");
        fs::create_dir_all(&nested).unwrap();
        File::create(nested.join("Cargo.toml")).unwrap();

        let detector = DefaultDetector::new();
        let results = detector.detect(&nested);

        assert!(!results.is_empty());
        assert!(results.iter().any(|r| r.project_type == ProjectType::Rust));
    }

    #[test]
    fn detect_with_many_markers() {
        // Monorepo with multiple project types
        let dir = setup_project(&[
            "Cargo.toml",
            "package.json",
            "pyproject.toml",
            "go.mod",
            "pom.xml",
        ]);

        let detector = DefaultDetector::new();
        let results = detector.detect_with_confidence(dir.path());

        // Should detect all 5 project types
        assert!(results.len() >= 5);

        let types: HashSet<_> = results.iter().map(|(t, _)| *t).collect();
        assert!(types.contains(&ProjectType::Rust));
        assert!(types.contains(&ProjectType::Node));
        assert!(types.contains(&ProjectType::Python));
        assert!(types.contains(&ProjectType::Go));
        assert!(types.contains(&ProjectType::Java));
    }

    #[test]
    fn detect_multiple_lockfiles() {
        // Node project with multiple lock files (pnpm, yarn, npm)
        let dir = setup_project(&[
            "package.json",
            "yarn.lock",
            "package-lock.json",
            "pnpm-lock.yaml",
        ]);

        let detector = DefaultDetector::new();
        let results = detector.detect(dir.path());

        // All should be detected as Node
        let node_results: Vec<_> = results
            .iter()
            .filter(|r| r.project_type == ProjectType::Node)
            .collect();

        // Should have 4 detections (package.json + 3 lock files)
        assert!(node_results.len() >= 4);
    }

    #[test]
    fn detect_with_glob_patterns() {
        // C# project with .csproj file
        let dir = setup_project(&["MyProject.csproj", "Program.cs"]);

        let detector = DefaultDetector::new();
        let results = detector.detect(dir.path());

        let csharp = results
            .iter()
            .find(|r| r.project_type == ProjectType::CSharp);
        assert!(csharp.is_some());
        assert!((csharp.unwrap().confidence - 1.0).abs() < 0.001);
    }

    #[test]
    fn detect_haskell_with_cabal() {
        // Haskell project with .cabal file
        let dir = setup_project(&["my-project.cabal", "src/Main.hs"]);

        let detector = DefaultDetector::new();
        let results = detector.detect(dir.path());

        let haskell = results
            .iter()
            .find(|r| r.project_type == ProjectType::Haskell);
        assert!(haskell.is_some());
    }

    #[test]
    fn detect_nim_with_nimble() {
        let dir = setup_project(&["myapp.nimble", "src/main.nim"]);

        let detector = DefaultDetector::new();
        let results = detector.detect(dir.path());

        let nim = results.iter().find(|r| r.project_type == ProjectType::Nim);
        assert!(nim.is_some());
    }

    #[test]
    fn detect_zig_project() {
        let dir = setup_project(&["build.zig", "build.zig.zon", "src/main.zig"]);

        let detector = DefaultDetector::new();
        let results = detector.detect(dir.path());

        // Should detect Zig from both build.zig and build.zig.zon
        let zig_results: Vec<_> = results
            .iter()
            .filter(|r| r.project_type == ProjectType::Zig)
            .collect();
        assert!(zig_results.len() >= 2);
    }

    #[test]
    fn detect_nonexistent_directory() {
        let detector = DefaultDetector::new();
        let results = detector.detect(PathBuf::from("/nonexistent/path/12345").as_path());

        assert!(results.is_empty());
    }

    #[test]
    #[cfg(unix)]
    fn detect_with_unreadable_directory() {
        let dir = TempDir::new().unwrap();
        let subdir = dir.path().join("unreadable");
        fs::create_dir(&subdir).unwrap();
        File::create(subdir.join("Cargo.toml")).unwrap();

        // Make directory unreadable (Unix-specific)
        fs::set_permissions(&subdir, Permissions::from_mode(0o000)).unwrap();

        let detector = DefaultDetector::new();
        let results = detector.detect(&subdir);

        // Should return empty (gracefully handle permission error)
        assert!(results.is_empty());

        // Restore permissions for cleanup
        fs::set_permissions(&subdir, Permissions::from_mode(0o755)).unwrap();
    }

    #[test]
    #[cfg(unix)]
    fn detect_with_symlink_to_marker() {
        let dir = TempDir::new().unwrap();
        let actual_cargo = dir.path().join("actual_Cargo.toml");
        let symlink_cargo = dir.path().join("Cargo.toml");

        // Create actual file
        File::create(&actual_cargo).unwrap();

        // Create symlink
        std::os::unix::fs::symlink(&actual_cargo, &symlink_cargo).unwrap();

        let detector = DefaultDetector::new();
        let results = detector.detect(dir.path());

        // Should detect Rust via symlink
        assert!(results.iter().any(|r| r.project_type == ProjectType::Rust));
    }

    #[test]
    fn confidence_ordering() {
        // Verify confidence ordering is correct
        let dir = setup_project(&["package.json", "requirements.txt"]);

        let detector = DefaultDetector::new();
        let results = detector.detect_with_confidence(dir.path());

        // Should be sorted by confidence descending
        for i in 1..results.len() {
            assert!(
                results[i - 1].1 >= results[i].1,
                "Results should be sorted by confidence descending"
            );
        }
    }

    #[test]
    fn custom_marker_priority() {
        let dir = setup_project(&["custom.marker", "Cargo.toml"]);

        let mut detector = DefaultDetector::new();
        // Add custom marker with higher confidence
        detector.add_marker(ProjectMarker::new(
            "custom.marker",
            ProjectType::Unknown,
            1.5, // Higher than any default
        ));

        let results = detector.detect_with_confidence(dir.path());

        // Custom marker should appear in results
        assert!(results.iter().any(|(t, _)| *t == ProjectType::Unknown));
    }

    #[test]
    fn project_type_id_lowercase() {
        // Verify all project type IDs are lowercase
        let types = [
            ProjectType::Rust,
            ProjectType::Node,
            ProjectType::Python,
            ProjectType::Go,
            ProjectType::Java,
            ProjectType::CSharp,
            ProjectType::Ruby,
            ProjectType::Elixir,
            ProjectType::Php,
            ProjectType::Swift,
            ProjectType::Kotlin,
            ProjectType::Scala,
            ProjectType::Haskell,
            ProjectType::Clojure,
            ProjectType::Cpp,
            ProjectType::C,
            ProjectType::Zig,
            ProjectType::Nim,
            ProjectType::Unknown,
        ];

        for pt in &types {
            let id = pt.id();
            assert_eq!(id, id.to_lowercase(), "ID should be lowercase: {id}");
        }
    }
}

// =============================================================================
// ContextCollector Tests - Caching and Fingerprinting
// =============================================================================

mod context_collector_tests {
    use super::*;

    #[test]
    fn collector_caches_results() {
        let dir = TempDir::new().unwrap();
        File::create(dir.path().join("Cargo.toml")).unwrap();

        let config = ContextCollectorConfig::default();
        let collector = ContextCollector::new(config);

        // First collection
        let ctx1 = collector.collect(dir.path()).unwrap();
        let fp1 = ctx1.fingerprint;

        // Second collection should be cached (within TTL)
        let ctx2 = collector.collect(dir.path()).unwrap();

        // Fingerprints should match
        assert_eq!(fp1.0, ctx2.fingerprint.0);
    }

    #[test]
    fn fingerprint_changes_with_projects() {
        let fp1 = create_context_fingerprint(&[], &[], &[]);
        let fp2 = create_context_fingerprint(&[ProjectType::Rust], &[], &[]);

        assert_ne!(fp1.0, fp2.0, "Adding project should change fingerprint");
    }

    #[test]
    fn fingerprint_changes_with_files() {
        let fp1 = create_context_fingerprint(&[], &[], &[]);
        let fp2 = create_context_fingerprint(&[], &["src/main.rs"], &[]);

        assert_ne!(fp1.0, fp2.0, "Adding file should change fingerprint");
    }

    #[test]
    fn fingerprint_changes_with_tools() {
        let fp1 = create_context_fingerprint(&[], &[], &[]);
        let fp2 = create_context_fingerprint(&[], &[], &["cargo"]);

        assert_ne!(fp1.0, fp2.0, "Adding tool should change fingerprint");
    }

    #[test]
    fn fingerprint_ignores_timestamp() {
        // Create two contexts with different timestamps
        let ctx1 = CollectedContext {
            cwd: PathBuf::from("/test"),
            detected_projects: vec![],
            recent_files: vec![],
            detected_tools: HashSet::new(),
            git_context: None,
            env_signals: std::collections::HashMap::new(),
            collected_at: chrono::Utc::now(),
            fingerprint: ms::context::CollectorFingerprint(0),
        };

        let mut ctx2 = ctx1.clone();
        ctx2.collected_at = ctx1.collected_at + chrono::Duration::hours(1);

        // Fingerprints should match (timestamp ignored)
        assert_eq!(ctx1.fingerprint().0, ctx2.fingerprint().0);
    }

    #[test]
    fn collector_respects_max_recent_files() {
        let dir = TempDir::new().unwrap();

        // Create many files
        for i in 0..50 {
            File::create(dir.path().join(format!("file{i}.txt"))).unwrap();
        }

        let config = ContextCollectorConfig {
            max_recent_files: 10,
            recent_file_max_age: Duration::from_secs(3600),
            scan_depth: 1,
            ignore_patterns: vec![],
        };
        let collector = ContextCollector::new(config);

        let ctx = collector.collect(dir.path()).unwrap();

        assert!(
            ctx.recent_files.len() <= 10,
            "Should respect max_recent_files"
        );
    }

    #[test]
    fn collector_respects_ignore_patterns() {
        let dir = TempDir::new().unwrap();

        // Create files in ignored directories
        fs::create_dir_all(dir.path().join("node_modules")).unwrap();
        File::create(dir.path().join("node_modules/package.json")).unwrap();

        fs::create_dir_all(dir.path().join(".git")).unwrap();
        File::create(dir.path().join(".git/config")).unwrap();

        // Create non-ignored file
        File::create(dir.path().join("main.rs")).unwrap();

        let config = ContextCollectorConfig {
            max_recent_files: 100,
            recent_file_max_age: Duration::from_secs(3600),
            scan_depth: 3,
            ignore_patterns: vec!["node_modules".to_string(), ".git".to_string()],
        };
        let collector = ContextCollector::new(config);

        let ctx = collector.collect(dir.path()).unwrap();

        // Should only find main.rs, not files in ignored directories
        assert_eq!(ctx.recent_files.len(), 1);
        assert!(ctx.recent_files[0].path.ends_with("main.rs"));
    }

    #[test]
    fn collector_respects_scan_depth() {
        let dir = TempDir::new().unwrap();

        // Create nested structure
        fs::create_dir_all(dir.path().join("a/b/c/d/e")).unwrap();
        File::create(dir.path().join("a/b/c/d/e/deep.txt")).unwrap();
        File::create(dir.path().join("a/shallow.txt")).unwrap();

        let config = ContextCollectorConfig {
            max_recent_files: 100,
            recent_file_max_age: Duration::from_secs(3600),
            scan_depth: 2, // Only go 2 levels deep
            ignore_patterns: vec![],
        };
        let collector = ContextCollector::new(config);

        let ctx = collector.collect(dir.path()).unwrap();

        // Should find shallow.txt but not deep.txt
        let paths: Vec<_> = ctx
            .recent_files
            .iter()
            .map(|f| f.path.file_name().unwrap().to_str().unwrap())
            .collect();

        assert!(paths.contains(&"shallow.txt"));
        assert!(
            !paths.contains(&"deep.txt"),
            "Should not find files beyond scan_depth"
        );
    }

    // Helper to create fingerprints for testing
    fn create_context_fingerprint(
        project_types: &[ProjectType],
        files: &[&str],
        tools: &[&str],
    ) -> ms::context::CollectorFingerprint {
        let ctx = CollectedContext {
            cwd: PathBuf::from("/test"),
            detected_projects: project_types
                .iter()
                .map(|pt| DetectedProject {
                    project_type: *pt,
                    confidence: 1.0,
                    marker_path: PathBuf::from("marker"),
                    marker_pattern: "marker".to_string(),
                })
                .collect(),
            recent_files: files
                .iter()
                .map(|f| ms::context::RecentFile {
                    path: PathBuf::from(f),
                    extension: PathBuf::from(f)
                        .extension()
                        .map(|e| e.to_string_lossy().to_string()),
                    modified_at: chrono::Utc::now(),
                    size: 0,
                })
                .collect(),
            detected_tools: tools.iter().map(|s| s.to_string()).collect(),
            git_context: None,
            env_signals: std::collections::HashMap::new(),
            collected_at: chrono::Utc::now(),
            fingerprint: ms::context::CollectorFingerprint(0),
        };
        ctx.fingerprint()
    }
}

// =============================================================================
// RelevanceScorer Tests - Edge Cases
// =============================================================================

mod relevance_scorer_tests {
    use super::*;

    fn sample_skill(
        id: &str,
        project_types: Vec<&str>,
        file_patterns: Vec<&str>,
        tools: Vec<&str>,
    ) -> SkillMetadata {
        SkillMetadata {
            id: id.to_string(),
            name: id.to_string(),
            context: ContextTags {
                project_types: project_types.iter().map(|s| s.to_string()).collect(),
                file_patterns: file_patterns.iter().map(|s| s.to_string()).collect(),
                tools: tools.iter().map(|s| s.to_string()).collect(),
                signals: vec![],
            },
            ..Default::default()
        }
    }

    fn context_for_project(project_type: ProjectType) -> ScoringContext {
        ScoringContext::new().with_projects(vec![DetectedProject {
            project_type,
            confidence: 1.0,
            marker_path: PathBuf::from("marker"),
            marker_pattern: "marker".to_string(),
        }])
    }

    #[test]
    fn score_empty_skill_context() {
        let scorer = RelevanceScorer::default();
        let skill = SkillMetadata {
            id: "generic".to_string(),
            name: "Generic Skill".to_string(),
            context: ContextTags::default(),
            ..Default::default()
        };
        let context = context_for_project(ProjectType::Rust);

        let score = scorer.score(&skill, &context);
        assert!(score < 0.001, "Empty context skill should have near-zero score");
    }

    #[test]
    fn score_empty_working_context() {
        let scorer = RelevanceScorer::default();
        let skill = sample_skill("rust-skill", vec!["rust"], vec!["*.rs"], vec!["cargo"]);
        let context = ScoringContext::new(); // Empty context

        let score = scorer.score(&skill, &context);
        assert!(score < 0.001, "Empty working context should yield near-zero score");
    }

    #[test]
    fn score_partial_tool_match() {
        let scorer = RelevanceScorer::default();
        let skill = sample_skill(
            "rust-skill",
            vec![],
            vec![],
            vec!["cargo", "rustc", "rust-analyzer"],
        );

        // Context with only cargo installed
        let context = ScoringContext::new().with_tools(["cargo"].map(String::from));

        let breakdown = scorer.breakdown(&skill, &context);

        // Should have 1/3 tool match
        assert!((breakdown.tools - 0.333).abs() < 0.01);
    }

    #[test]
    fn score_case_insensitive_project_type() {
        let scorer = RelevanceScorer::default();
        // Skill uses uppercase
        let skill = sample_skill("rust-skill", vec!["RUST"], vec![], vec![]);

        // Context uses lowercase
        let context = context_for_project(ProjectType::Rust);

        let breakdown = scorer.breakdown(&skill, &context);
        assert!((breakdown.project_type - 1.0).abs() < 0.001);
    }

    #[test]
    fn score_case_insensitive_tools() {
        let scorer = RelevanceScorer::default();
        let skill = sample_skill("node-skill", vec![], vec![], vec!["NPM", "Node"]);

        let context = ScoringContext::new().with_tools(["npm", "node"].map(String::from));

        let breakdown = scorer.breakdown(&skill, &context);
        assert!((breakdown.tools - 1.0).abs() < 0.001);
    }

    #[test]
    fn score_signal_pattern_matching() {
        let scorer = RelevanceScorer::default();
        let mut skill = SkillMetadata {
            id: "async-skill".to_string(),
            name: "Async Skill".to_string(),
            context: ContextTags {
                project_types: vec![],
                file_patterns: vec![],
                tools: vec![],
                signals: vec![
                    ContextSignal::new("tokio", "tokio::.*", 1.0),
                    ContextSignal::new("async-std", "async_std::.*", 0.5),
                ],
            },
            ..Default::default()
        };

        // Context with tokio usage
        let context =
            ScoringContext::new().with_content(vec!["use tokio::runtime::Runtime;".to_string()]);

        let breakdown = scorer.breakdown(&skill, &context);

        // Should match tokio signal (weight 1.0 out of total 1.5)
        assert!(breakdown.signals > 0.6);
    }

    #[test]
    fn score_multiple_project_types() {
        let scorer = RelevanceScorer::default();
        let skill = sample_skill("multi-skill", vec!["rust", "python"], vec![], vec![]);

        // Context with both Rust and Python detected
        let context = ScoringContext::new().with_projects(vec![
            DetectedProject {
                project_type: ProjectType::Rust,
                confidence: 1.0,
                marker_path: PathBuf::from("Cargo.toml"),
                marker_pattern: "Cargo.toml".to_string(),
            },
            DetectedProject {
                project_type: ProjectType::Python,
                confidence: 0.8,
                marker_path: PathBuf::from("pyproject.toml"),
                marker_pattern: "pyproject.toml".to_string(),
            },
        ]);

        let breakdown = scorer.breakdown(&skill, &context);

        // Should use highest confidence match
        assert!((breakdown.project_type - 1.0).abs() < 0.001);
    }

    #[test]
    fn score_file_pattern_normalization() {
        let scorer = RelevanceScorer::default();
        let skill = sample_skill("rust-skill", vec![], vec!["*.rs", "Cargo.toml"], vec![]);

        let context = ScoringContext::new().with_recent_files(vec![
            "src/main.rs".to_string(),
            "src/lib.rs".to_string(),
            "Cargo.toml".to_string(),
        ]);

        let breakdown = scorer.breakdown(&skill, &context);

        // All 3 files match patterns
        assert!((breakdown.file_patterns - 1.0).abs() < 0.001);
    }

    #[test]
    fn custom_weights_normalize() {
        // Custom weights that don't sum to 1.0
        let weights = ScoringWeights::new(10.0, 5.0, 3.0, 1.0, 1.0);
        let normalized = weights.normalized();

        let sum = normalized.project_type
            + normalized.file_patterns
            + normalized.tools
            + normalized.signals
            + normalized.historical;

        assert!((sum - 1.0).abs() < 0.001, "Weights should normalize to 1.0");
    }

    #[test]
    fn rank_preserves_order_for_same_score() {
        let scorer = RelevanceScorer::default();

        // Create skills with same context (should have same score)
        let skills = vec![
            sample_skill("skill-a", vec!["rust"], vec![], vec![]),
            sample_skill("skill-b", vec!["rust"], vec![], vec![]),
            sample_skill("skill-c", vec!["rust"], vec![], vec![]),
        ];

        let context = context_for_project(ProjectType::Rust);
        let ranked = scorer.rank(&skills, &context);

        assert_eq!(ranked.len(), 3);
        // All should have same score
        let first_score = ranked[0].score;
        for r in &ranked {
            assert!((r.score - first_score).abs() < 0.001);
        }
    }

    #[test]
    fn above_threshold_filters_correctly() {
        let scorer = RelevanceScorer::default();

        let skills = vec![
            sample_skill("high-match", vec!["rust"], vec!["*.rs"], vec!["cargo"]),
            sample_skill("low-match", vec!["python"], vec![], vec![]),
        ];

        let context = context_for_project(ProjectType::Rust)
            .with_recent_files(vec!["main.rs".to_string()])
            .with_tools(["cargo"].map(String::from));

        let relevant = scorer.above_threshold(&skills, &context, 0.5);

        // Only high-match should pass threshold
        assert_eq!(relevant.len(), 1);
        assert_eq!(relevant[0].skill_id, "high-match");
    }

    #[test]
    fn top_n_respects_limit() {
        let scorer = RelevanceScorer::default();

        let skills: Vec<_> = (0..10)
            .map(|i| sample_skill(&format!("skill-{i}"), vec!["rust"], vec![], vec![]))
            .collect();

        let context = context_for_project(ProjectType::Rust);

        let top = scorer.top_n(&skills, &context, 3);
        assert_eq!(top.len(), 3);
    }

    #[test]
    fn primary_project_type_selects_highest_confidence() {
        let context = ScoringContext::new().with_projects(vec![
            DetectedProject {
                project_type: ProjectType::Python,
                confidence: 0.5,
                marker_path: PathBuf::from("req.txt"),
                marker_pattern: "requirements.txt".to_string(),
            },
            DetectedProject {
                project_type: ProjectType::Rust,
                confidence: 1.0,
                marker_path: PathBuf::from("Cargo.toml"),
                marker_pattern: "Cargo.toml".to_string(),
            },
        ]);

        assert_eq!(context.primary_project_type(), Some(ProjectType::Rust));
    }
}
