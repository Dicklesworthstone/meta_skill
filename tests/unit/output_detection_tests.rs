//! Unit tests for output detection module.
//!
//! These tests verify the output detection logic for determining
//! rich vs plain output mode based on format, environment, and terminal state.
//!
//! # Test Strategy
//!
//! Since the project forbids unsafe code and environment variable manipulation
//! requires unsafe in Rust 2024 edition, these tests use the module's built-in
//! test support: `OutputEnvironment::new()` and `OutputDetector::with_env()`
//! to create controlled test scenarios without modifying actual environment.

use ms::cli::output::OutputFormat;
use ms::output::detection::{
    AGENT_ENV_VARS, CI_ENV_VARS, IDE_ENV_VARS, OutputDecisionReason,
    OutputDetector, OutputEnvironment, OutputModeReport, is_agent_environment, is_ci_environment,
    is_ide_environment, maybe_print_debug_output, should_use_rich_output,
    should_use_rich_with_flags,
};

// =============================================================================
// OutputEnvironment Tests
// =============================================================================

mod output_environment {
    use super::*;

    #[test]
    fn new_creates_custom_environment() {
        let env = OutputEnvironment::new(true, false, false, true);
        assert!(env.no_color);
        assert!(!env.plain_output);
        assert!(!env.force_rich);
        assert!(env.stdout_is_terminal);
    }

    #[test]
    fn new_with_all_false() {
        let env = OutputEnvironment::new(false, false, false, false);
        assert!(!env.no_color);
        assert!(!env.plain_output);
        assert!(!env.force_rich);
        assert!(!env.stdout_is_terminal);
    }

    #[test]
    fn new_with_all_true() {
        let env = OutputEnvironment::new(true, true, true, true);
        assert!(env.no_color);
        assert!(env.plain_output);
        assert!(env.force_rich);
        assert!(env.stdout_is_terminal);
    }

    #[test]
    fn from_env_returns_valid_struct() {
        // Just verify it doesn't panic and returns valid data
        let env = OutputEnvironment::from_env();
        // We can't assert specific values since they depend on the test environment
        // but we can verify the struct is properly constructed
        let _ = env.no_color;
        let _ = env.plain_output;
        let _ = env.force_rich;
        let _ = env.stdout_is_terminal;
    }

    #[test]
    fn equality_comparison() {
        let env1 = OutputEnvironment::new(true, false, true, false);
        let env2 = OutputEnvironment::new(true, false, true, false);
        let env3 = OutputEnvironment::new(false, false, true, false);

        assert_eq!(env1, env2);
        assert_ne!(env1, env3);
    }

    #[test]
    fn copy_semantics() {
        let env1 = OutputEnvironment::new(true, false, true, false);
        let env2 = env1; // Copy
        assert_eq!(env1, env2);
    }

    #[test]
    fn clone_semantics() {
        let env1 = OutputEnvironment::new(true, false, true, false);
        let env2 = env1.clone();
        assert_eq!(env1, env2);
    }

    #[test]
    fn debug_formatting() {
        let env = OutputEnvironment::new(true, false, true, false);
        let debug_str = format!("{:?}", env);
        assert!(debug_str.contains("OutputEnvironment"));
        assert!(debug_str.contains("no_color"));
        assert!(debug_str.contains("plain_output"));
    }
}

// =============================================================================
// OutputDecision Tests
// =============================================================================

mod output_decision {
    use super::*;

    #[test]
    fn rich_decision_has_use_rich_true() {
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(false, false, false, true),
        );
        let decision = detector.decide();
        assert!(decision.use_rich);
    }

    #[test]
    fn plain_decision_has_use_rich_false() {
        let detector = OutputDetector::with_env(
            OutputFormat::Json,
            false,
            OutputEnvironment::new(false, false, false, true),
        );
        let decision = detector.decide();
        assert!(!decision.use_rich);
    }

    #[test]
    fn decision_equality() {
        let detector1 = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(false, false, false, true),
        );
        let detector2 = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(false, false, false, true),
        );
        assert_eq!(detector1.decide(), detector2.decide());
    }

    #[test]
    fn decision_copy_semantics() {
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(false, false, false, true),
        );
        let decision1 = detector.decide();
        let decision2 = decision1; // Copy
        assert_eq!(decision1, decision2);
    }

    #[test]
    fn decision_debug_formatting() {
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(false, false, false, true),
        );
        let decision = detector.decide();
        let debug_str = format!("{:?}", decision);
        assert!(debug_str.contains("OutputDecision"));
        assert!(debug_str.contains("use_rich"));
        assert!(debug_str.contains("reason"));
    }
}

// =============================================================================
// OutputDecisionReason Tests
// =============================================================================

mod output_decision_reason {
    use super::*;

    #[test]
    fn machine_readable_format_reason() {
        let detector = OutputDetector::with_env(
            OutputFormat::Json,
            false,
            OutputEnvironment::new(false, false, false, true),
        );
        let decision = detector.decide();
        assert_eq!(decision.reason, OutputDecisionReason::MachineReadableFormat);
    }

    #[test]
    fn plain_format_reason() {
        let detector = OutputDetector::with_env(
            OutputFormat::Plain,
            false,
            OutputEnvironment::new(false, false, false, true),
        );
        let decision = detector.decide();
        assert_eq!(decision.reason, OutputDecisionReason::PlainFormat);
    }

    #[test]
    fn robot_mode_reason() {
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            true, // robot_mode = true
            OutputEnvironment::new(false, false, false, true),
        );
        let decision = detector.decide();
        assert_eq!(decision.reason, OutputDecisionReason::RobotMode);
    }

    #[test]
    fn env_no_color_reason() {
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(true, false, false, true), // no_color = true
        );
        let decision = detector.decide();
        assert_eq!(decision.reason, OutputDecisionReason::EnvNoColor);
    }

    #[test]
    fn env_plain_output_reason() {
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(false, true, false, true), // plain_output = true
        );
        let decision = detector.decide();
        assert_eq!(decision.reason, OutputDecisionReason::EnvPlainOutput);
    }

    #[test]
    fn not_terminal_reason() {
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(false, false, false, false), // stdout_is_terminal = false
        );
        let decision = detector.decide();
        assert_eq!(decision.reason, OutputDecisionReason::NotTerminal);
    }

    #[test]
    fn forced_rich_reason() {
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(false, false, true, true), // force_rich = true
        );
        let decision = detector.decide();
        assert_eq!(decision.reason, OutputDecisionReason::ForcedRich);
    }

    #[test]
    fn human_default_reason() {
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(false, false, false, true), // all defaults, terminal = true
        );
        let decision = detector.decide();
        assert_eq!(decision.reason, OutputDecisionReason::HumanDefault);
    }

    #[test]
    fn reason_equality() {
        assert_eq!(
            OutputDecisionReason::MachineReadableFormat,
            OutputDecisionReason::MachineReadableFormat
        );
        assert_ne!(
            OutputDecisionReason::MachineReadableFormat,
            OutputDecisionReason::PlainFormat
        );
    }

    #[test]
    fn reason_debug_formatting() {
        let reason = OutputDecisionReason::HumanDefault;
        let debug_str = format!("{:?}", reason);
        assert!(debug_str.contains("HumanDefault"));
    }
}

// =============================================================================
// OutputDetector Tests
// =============================================================================

mod output_detector {
    use super::*;

    #[test]
    fn new_creates_detector() {
        let detector = OutputDetector::new(OutputFormat::Human, false);
        // Just verify it doesn't panic and returns valid decisions
        let _ = detector.decide();
    }

    #[test]
    fn with_env_creates_detector() {
        let env = OutputEnvironment::new(false, false, false, true);
        let detector = OutputDetector::with_env(OutputFormat::Human, false, env);
        let decision = detector.decide();
        assert!(decision.use_rich);
    }

    #[test]
    fn should_use_rich_returns_decision_use_rich() {
        let env = OutputEnvironment::new(false, false, false, true);
        let detector = OutputDetector::with_env(OutputFormat::Human, false, env);
        assert!(detector.should_use_rich());

        let env2 = OutputEnvironment::new(true, false, false, true);
        let detector2 = OutputDetector::with_env(OutputFormat::Human, false, env2);
        assert!(!detector2.should_use_rich());
    }
}

// =============================================================================
// Format-based Detection Tests
// =============================================================================

mod format_detection {
    use super::*;

    #[test]
    fn json_format_forces_plain() {
        let detector = OutputDetector::with_env(
            OutputFormat::Json,
            false,
            OutputEnvironment::new(false, false, false, true),
        );
        assert!(!detector.should_use_rich());
        assert_eq!(
            detector.decide().reason,
            OutputDecisionReason::MachineReadableFormat
        );
    }

    #[test]
    fn jsonl_format_forces_plain() {
        let detector = OutputDetector::with_env(
            OutputFormat::Jsonl,
            false,
            OutputEnvironment::new(false, false, false, true),
        );
        assert!(!detector.should_use_rich());
        assert_eq!(
            detector.decide().reason,
            OutputDecisionReason::MachineReadableFormat
        );
    }

    #[test]
    fn tsv_format_forces_plain() {
        let detector = OutputDetector::with_env(
            OutputFormat::Tsv,
            false,
            OutputEnvironment::new(false, false, false, true),
        );
        assert!(!detector.should_use_rich());
        assert_eq!(
            detector.decide().reason,
            OutputDecisionReason::MachineReadableFormat
        );
    }

    #[test]
    fn plain_format_forces_plain() {
        let detector = OutputDetector::with_env(
            OutputFormat::Plain,
            false,
            OutputEnvironment::new(false, false, false, true),
        );
        assert!(!detector.should_use_rich());
        assert_eq!(detector.decide().reason, OutputDecisionReason::PlainFormat);
    }

    #[test]
    fn human_format_allows_rich_on_terminal() {
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(false, false, false, true),
        );
        assert!(detector.should_use_rich());
        assert_eq!(detector.decide().reason, OutputDecisionReason::HumanDefault);
    }

    #[test]
    fn human_format_plain_on_non_terminal() {
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(false, false, false, false),
        );
        assert!(!detector.should_use_rich());
        assert_eq!(detector.decide().reason, OutputDecisionReason::NotTerminal);
    }
}

// =============================================================================
// Robot Mode Tests
// =============================================================================

mod robot_mode {
    use super::*;

    #[test]
    fn robot_mode_forces_plain() {
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            true, // robot mode
            OutputEnvironment::new(false, false, false, true),
        );
        assert!(!detector.should_use_rich());
        assert_eq!(detector.decide().reason, OutputDecisionReason::RobotMode);
    }

    #[test]
    fn robot_mode_takes_precedence_over_terminal() {
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            true,                                              // robot mode
            OutputEnvironment::new(false, false, false, true), // terminal = true
        );
        assert!(!detector.should_use_rich());
    }

    #[test]
    fn robot_mode_takes_precedence_over_force_rich() {
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            true,                                             // robot mode
            OutputEnvironment::new(false, false, true, true), // force_rich = true
        );
        assert!(!detector.should_use_rich());
        assert_eq!(detector.decide().reason, OutputDecisionReason::RobotMode);
    }
}

// =============================================================================
// Environment Variable Detection Tests
// =============================================================================

mod env_detection {
    use super::*;

    #[test]
    fn no_color_forces_plain() {
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(true, false, false, true), // no_color = true
        );
        assert!(!detector.should_use_rich());
        assert_eq!(detector.decide().reason, OutputDecisionReason::EnvNoColor);
    }

    #[test]
    fn ms_plain_output_forces_plain() {
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(false, true, false, true), // plain_output = true
        );
        assert!(!detector.should_use_rich());
        assert_eq!(
            detector.decide().reason,
            OutputDecisionReason::EnvPlainOutput
        );
    }

    #[test]
    fn ms_force_rich_forces_rich() {
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(false, false, true, true), // force_rich = true
        );
        assert!(detector.should_use_rich());
        assert_eq!(detector.decide().reason, OutputDecisionReason::ForcedRich);
    }

    #[test]
    fn force_rich_works_without_terminal() {
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(false, false, true, false), // force_rich but no terminal
        );
        // NotTerminal takes precedence over ForcedRich in the current logic
        assert!(!detector.should_use_rich());
        assert_eq!(detector.decide().reason, OutputDecisionReason::NotTerminal);
    }
}

// =============================================================================
// Precedence Tests
// =============================================================================

mod precedence {
    use super::*;

    #[test]
    fn machine_readable_beats_force_rich() {
        let detector = OutputDetector::with_env(
            OutputFormat::Json,
            false,
            OutputEnvironment::new(false, false, true, true), // force_rich = true
        );
        assert!(!detector.should_use_rich());
        assert_eq!(
            detector.decide().reason,
            OutputDecisionReason::MachineReadableFormat
        );
    }

    #[test]
    fn plain_format_beats_force_rich() {
        let detector = OutputDetector::with_env(
            OutputFormat::Plain,
            false,
            OutputEnvironment::new(false, false, true, true), // force_rich = true
        );
        assert!(!detector.should_use_rich());
        assert_eq!(detector.decide().reason, OutputDecisionReason::PlainFormat);
    }

    #[test]
    fn robot_mode_beats_force_rich() {
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            true, // robot mode
            OutputEnvironment::new(false, false, true, true),
        );
        assert!(!detector.should_use_rich());
        assert_eq!(detector.decide().reason, OutputDecisionReason::RobotMode);
    }

    #[test]
    fn no_color_beats_force_rich() {
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(true, false, true, true), // no_color + force_rich
        );
        assert!(!detector.should_use_rich());
        assert_eq!(detector.decide().reason, OutputDecisionReason::EnvNoColor);
    }

    #[test]
    fn plain_output_beats_force_rich() {
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(false, true, true, true), // plain_output + force_rich
        );
        assert!(!detector.should_use_rich());
        assert_eq!(
            detector.decide().reason,
            OutputDecisionReason::EnvPlainOutput
        );
    }

    #[test]
    fn no_color_beats_plain_output() {
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(true, true, false, true), // both no_color and plain_output
        );
        assert!(!detector.should_use_rich());
        // NO_COLOR is checked first, so it wins
        assert_eq!(detector.decide().reason, OutputDecisionReason::EnvNoColor);
    }

    #[test]
    fn not_terminal_beats_force_rich_in_current_logic() {
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(false, false, true, false), // force_rich + not terminal
        );
        // NotTerminal is checked before ForcedRich
        assert!(!detector.should_use_rich());
        assert_eq!(detector.decide().reason, OutputDecisionReason::NotTerminal);
    }

    #[test]
    fn detection_order_complete() {
        // Test the full precedence chain:
        // 1. Machine-readable format
        // 2. Plain format
        // 3. Robot mode
        // 4. NO_COLOR
        // 5. MS_PLAIN_OUTPUT
        // 6. Not terminal
        // 7. MS_FORCE_RICH
        // 8. Human default

        // Machine-readable wins over everything
        let detector = OutputDetector::with_env(
            OutputFormat::Json,
            true,
            OutputEnvironment::new(true, true, true, true),
        );
        assert_eq!(
            detector.decide().reason,
            OutputDecisionReason::MachineReadableFormat
        );

        // Without machine format, plain format wins
        let detector = OutputDetector::with_env(
            OutputFormat::Plain,
            true,
            OutputEnvironment::new(true, true, true, true),
        );
        assert_eq!(detector.decide().reason, OutputDecisionReason::PlainFormat);

        // Without plain format, robot mode wins
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            true,
            OutputEnvironment::new(true, true, true, true),
        );
        assert_eq!(detector.decide().reason, OutputDecisionReason::RobotMode);

        // Without robot mode, NO_COLOR wins
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(true, true, true, true),
        );
        assert_eq!(detector.decide().reason, OutputDecisionReason::EnvNoColor);

        // Without NO_COLOR, MS_PLAIN_OUTPUT wins
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(false, true, true, true),
        );
        assert_eq!(
            detector.decide().reason,
            OutputDecisionReason::EnvPlainOutput
        );

        // Without MS_PLAIN_OUTPUT, not terminal wins over force_rich
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(false, false, true, false),
        );
        assert_eq!(detector.decide().reason, OutputDecisionReason::NotTerminal);

        // With terminal, force_rich wins
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(false, false, true, true),
        );
        assert_eq!(detector.decide().reason, OutputDecisionReason::ForcedRich);

        // Without force_rich, human default
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(false, false, false, true),
        );
        assert_eq!(detector.decide().reason, OutputDecisionReason::HumanDefault);
    }
}

// =============================================================================
// Convenience Function Tests
// =============================================================================

mod convenience_functions {
    use super::*;

    #[test]
    fn should_use_rich_output_works() {
        // This function reads from actual environment, so just verify it runs
        let _ = should_use_rich_output(OutputFormat::Human, false);
        let _ = should_use_rich_output(OutputFormat::Json, false);
        let _ = should_use_rich_output(OutputFormat::Human, true);
    }

    #[test]
    fn should_use_rich_with_flags_force_plain() {
        // force_plain takes precedence
        let result = should_use_rich_with_flags(OutputFormat::Human, false, true, false);
        assert!(!result);
    }

    #[test]
    fn should_use_rich_with_flags_force_rich() {
        // force_rich takes precedence when force_plain is false
        let result = should_use_rich_with_flags(OutputFormat::Human, false, false, true);
        assert!(result);
    }

    #[test]
    fn should_use_rich_with_flags_force_plain_beats_force_rich() {
        // force_plain beats force_rich
        let result = should_use_rich_with_flags(OutputFormat::Human, false, true, true);
        assert!(!result);
    }

    #[test]
    fn should_use_rich_with_flags_falls_back_to_detection() {
        // Neither flag set, falls back to detection
        let _ = should_use_rich_with_flags(OutputFormat::Human, false, false, false);
        let _ = should_use_rich_with_flags(OutputFormat::Json, false, false, false);
    }
}

// =============================================================================
// Environment Variable Constants Tests
// =============================================================================

mod env_var_constants {
    use super::*;

    #[test]
    fn agent_env_vars_contains_expected_vars() {
        assert!(AGENT_ENV_VARS.contains(&"CLAUDE_CODE"));
        assert!(AGENT_ENV_VARS.contains(&"CURSOR_AI"));
        assert!(AGENT_ENV_VARS.contains(&"OPENAI_CODEX"));
        assert!(AGENT_ENV_VARS.contains(&"AIDER_MODE"));
        assert!(AGENT_ENV_VARS.contains(&"CODEIUM_ENABLED"));
        assert!(AGENT_ENV_VARS.contains(&"WINDSURF_AGENT"));
        assert!(AGENT_ENV_VARS.contains(&"COPILOT_AGENT"));
        assert!(AGENT_ENV_VARS.contains(&"COPILOT_WORKSPACE"));
        assert!(AGENT_ENV_VARS.contains(&"AGENT_MODE"));
        assert!(AGENT_ENV_VARS.contains(&"IDE_AGENT"));
        assert!(AGENT_ENV_VARS.contains(&"CONTINUE_DEV"));
        assert!(AGENT_ENV_VARS.contains(&"SOURCEGRAPH_CODY"));
        assert!(AGENT_ENV_VARS.contains(&"TABNINE_AGENT"));
        assert!(AGENT_ENV_VARS.contains(&"AMAZON_Q"));
        assert!(AGENT_ENV_VARS.contains(&"GEMINI_CODE"));
    }

    #[test]
    fn agent_env_vars_count() {
        assert_eq!(AGENT_ENV_VARS.len(), 15);
    }

    #[test]
    fn ci_env_vars_contains_expected_vars() {
        assert!(CI_ENV_VARS.contains(&"CI"));
        assert!(CI_ENV_VARS.contains(&"GITHUB_ACTIONS"));
        assert!(CI_ENV_VARS.contains(&"GITLAB_CI"));
        assert!(CI_ENV_VARS.contains(&"JENKINS_URL"));
        assert!(CI_ENV_VARS.contains(&"TRAVIS"));
        assert!(CI_ENV_VARS.contains(&"CIRCLECI"));
        assert!(CI_ENV_VARS.contains(&"BUILDKITE"));
        assert!(CI_ENV_VARS.contains(&"BITBUCKET_PIPELINES"));
        assert!(CI_ENV_VARS.contains(&"TF_BUILD"));
        assert!(CI_ENV_VARS.contains(&"TEAMCITY_VERSION"));
        assert!(CI_ENV_VARS.contains(&"DRONE"));
        assert!(CI_ENV_VARS.contains(&"WOODPECKER"));
    }

    #[test]
    fn ci_env_vars_count() {
        assert_eq!(CI_ENV_VARS.len(), 12);
    }

    #[test]
    fn ide_env_vars_contains_expected_vars() {
        assert!(IDE_ENV_VARS.contains(&"VSCODE_GIT_ASKPASS_NODE"));
        assert!(IDE_ENV_VARS.contains(&"VSCODE_INJECTION"));
        assert!(IDE_ENV_VARS.contains(&"CODESPACES"));
        assert!(IDE_ENV_VARS.contains(&"GITPOD_WORKSPACE_ID"));
        assert!(IDE_ENV_VARS.contains(&"REPLIT_DB_URL"));
        assert!(IDE_ENV_VARS.contains(&"CLOUD_SHELL"));
    }

    #[test]
    fn ide_env_vars_count() {
        assert_eq!(IDE_ENV_VARS.len(), 6);
    }

    #[test]
    fn env_var_lists_have_no_duplicates() {
        use std::collections::HashSet;

        let agent_set: HashSet<_> = AGENT_ENV_VARS.iter().collect();
        assert_eq!(
            agent_set.len(),
            AGENT_ENV_VARS.len(),
            "AGENT_ENV_VARS has duplicates"
        );

        let ci_set: HashSet<_> = CI_ENV_VARS.iter().collect();
        assert_eq!(
            ci_set.len(),
            CI_ENV_VARS.len(),
            "CI_ENV_VARS has duplicates"
        );

        let ide_set: HashSet<_> = IDE_ENV_VARS.iter().collect();
        assert_eq!(
            ide_set.len(),
            IDE_ENV_VARS.len(),
            "IDE_ENV_VARS has duplicates"
        );
    }

    #[test]
    fn env_var_lists_are_non_empty() {
        assert!(!AGENT_ENV_VARS.is_empty());
        assert!(!CI_ENV_VARS.is_empty());
        assert!(!IDE_ENV_VARS.is_empty());
    }
}

// =============================================================================
// Environment Detection Functions Tests
// =============================================================================

mod env_detection_functions {
    use super::*;

    #[test]
    fn is_agent_environment_runs_without_panic() {
        // Can't control the environment, just verify it runs
        let _ = is_agent_environment();
    }

    #[test]
    fn is_ci_environment_runs_without_panic() {
        let _ = is_ci_environment();
    }

    #[test]
    fn is_ide_environment_runs_without_panic() {
        let _ = is_ide_environment();
    }

    #[test]
    fn detected_agent_vars_returns_valid_list() {
        let vars = ms::output::detection::detected_agent_vars();
        // All returned vars should be in AGENT_ENV_VARS
        for var in &vars {
            assert!(
                AGENT_ENV_VARS.contains(&var.as_str()),
                "{} not in AGENT_ENV_VARS",
                var
            );
        }
    }

    #[test]
    fn detected_ci_vars_returns_valid_list() {
        let vars = ms::output::detection::detected_ci_vars();
        // All returned vars should be in CI_ENV_VARS
        for var in &vars {
            assert!(
                CI_ENV_VARS.contains(&var.as_str()),
                "{} not in CI_ENV_VARS",
                var
            );
        }
    }

    #[test]
    fn detected_ide_vars_returns_valid_list() {
        let vars = ms::output::detection::detected_ide_vars();
        // All returned vars should be in IDE_ENV_VARS
        for var in &vars {
            assert!(
                IDE_ENV_VARS.contains(&var.as_str()),
                "{} not in IDE_ENV_VARS",
                var
            );
        }
    }
}

// =============================================================================
// OutputModeReport Tests
// =============================================================================

mod output_mode_report {
    use super::*;

    #[test]
    fn generate_creates_report() {
        let report = OutputModeReport::generate(OutputFormat::Human, false);
        assert_eq!(report.format, "Human");
        assert!(!report.robot_mode);
    }

    #[test]
    fn generate_with_robot_mode() {
        let report = OutputModeReport::generate(OutputFormat::Human, true);
        assert!(report.robot_mode);
    }

    #[test]
    fn generate_with_json_format() {
        let report = OutputModeReport::generate(OutputFormat::Json, false);
        assert_eq!(report.format, "Json");
        assert!(!report.decision.use_rich);
    }

    #[test]
    fn report_contains_environment() {
        let report = OutputModeReport::generate(OutputFormat::Human, false);
        // Environment fields should be populated
        let _ = report.env.no_color;
        let _ = report.env.plain_output;
        let _ = report.env.force_rich;
        let _ = report.env.stdout_is_terminal;
    }

    #[test]
    fn report_contains_detected_vars() {
        let report = OutputModeReport::generate(OutputFormat::Human, false);
        // These are vectors, may be empty but should exist
        let _ = report.agent_vars.len();
        let _ = report.ci_vars.len();
        let _ = report.ide_vars.len();
    }

    #[test]
    fn report_contains_terminal_info() {
        let report = OutputModeReport::generate(OutputFormat::Human, false);
        // Optional fields
        let _ = report.term;
        let _ = report.colorterm;
        let _ = report.columns;
    }

    #[test]
    fn report_contains_decision() {
        let report = OutputModeReport::generate(OutputFormat::Human, false);
        let _ = report.decision.use_rich;
        let _ = report.decision.reason;
    }

    #[test]
    fn format_text_produces_output() {
        let report = OutputModeReport::generate(OutputFormat::Human, false);
        let text = report.format_text();
        assert!(!text.is_empty());
        assert!(text.contains("Output Mode Detection Report"));
    }

    #[test]
    fn format_text_includes_format() {
        let report = OutputModeReport::generate(OutputFormat::Json, false);
        let text = report.format_text();
        assert!(text.contains("Format: Json"));
    }

    #[test]
    fn format_text_includes_robot_mode() {
        let report = OutputModeReport::generate(OutputFormat::Human, true);
        let text = report.format_text();
        assert!(text.contains("Robot Mode: true"));
    }

    #[test]
    fn format_text_includes_environment_section() {
        let report = OutputModeReport::generate(OutputFormat::Human, false);
        let text = report.format_text();
        assert!(text.contains("Environment Variables:"));
        assert!(text.contains("NO_COLOR:"));
        assert!(text.contains("MS_PLAIN_OUTPUT:"));
        assert!(text.contains("MS_FORCE_RICH:"));
    }

    #[test]
    fn format_text_includes_terminal_section() {
        let report = OutputModeReport::generate(OutputFormat::Human, false);
        let text = report.format_text();
        assert!(text.contains("Terminal:"));
        assert!(text.contains("is_terminal():"));
        assert!(text.contains("TERM:"));
        assert!(text.contains("COLORTERM:"));
        assert!(text.contains("COLUMNS:"));
    }

    #[test]
    fn format_text_includes_decision_section() {
        let report = OutputModeReport::generate(OutputFormat::Human, false);
        let text = report.format_text();
        assert!(text.contains("Decision:"));
        assert!(text.contains("Mode:"));
        assert!(text.contains("Reason:"));
    }

    #[test]
    fn report_clone() {
        let report1 = OutputModeReport::generate(OutputFormat::Human, false);
        let report2 = report1.clone();
        assert_eq!(report1.format, report2.format);
        assert_eq!(report1.robot_mode, report2.robot_mode);
    }

    #[test]
    fn report_debug() {
        let report = OutputModeReport::generate(OutputFormat::Human, false);
        let debug_str = format!("{:?}", report);
        assert!(debug_str.contains("OutputModeReport"));
    }
}

// =============================================================================
// Debug Output Function Tests
// =============================================================================

mod debug_output {
    use super::*;

    #[test]
    fn maybe_print_debug_output_runs_without_panic() {
        // This function checks MS_DEBUG_OUTPUT env var
        // We can't control it, but we can verify it doesn't panic
        maybe_print_debug_output(OutputFormat::Human, false);
        maybe_print_debug_output(OutputFormat::Json, true);
    }
}

// =============================================================================
// Edge Cases Tests
// =============================================================================

mod edge_cases {
    use super::*;

    #[test]
    fn all_env_flags_set_machine_format_wins() {
        let detector = OutputDetector::with_env(
            OutputFormat::Json,
            true,
            OutputEnvironment::new(true, true, true, true),
        );
        assert!(!detector.should_use_rich());
        assert_eq!(
            detector.decide().reason,
            OutputDecisionReason::MachineReadableFormat
        );
    }

    #[test]
    fn all_env_flags_set_plain_format_wins() {
        let detector = OutputDetector::with_env(
            OutputFormat::Plain,
            true,
            OutputEnvironment::new(true, true, true, true),
        );
        assert!(!detector.should_use_rich());
        assert_eq!(detector.decide().reason, OutputDecisionReason::PlainFormat);
    }

    #[test]
    fn conflicting_no_color_and_force_rich() {
        // no_color should win because it's checked first
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(true, false, true, true),
        );
        assert!(!detector.should_use_rich());
        assert_eq!(detector.decide().reason, OutputDecisionReason::EnvNoColor);
    }

    #[test]
    fn conflicting_plain_output_and_force_rich() {
        // plain_output should win because it's checked before force_rich
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(false, true, true, true),
        );
        assert!(!detector.should_use_rich());
        assert_eq!(
            detector.decide().reason,
            OutputDecisionReason::EnvPlainOutput
        );
    }

    #[test]
    fn non_terminal_with_all_rich_indicators() {
        // Being non-terminal should still force plain
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(false, false, true, false), // force_rich but no terminal
        );
        assert!(!detector.should_use_rich());
        assert_eq!(detector.decide().reason, OutputDecisionReason::NotTerminal);
    }

    #[test]
    fn human_on_terminal_is_rich_by_default() {
        // The happy path: Human format on a terminal with no env overrides
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(false, false, false, true),
        );
        assert!(detector.should_use_rich());
        assert_eq!(detector.decide().reason, OutputDecisionReason::HumanDefault);
    }
}

// =============================================================================
// All OutputFormat Variants Tests
// =============================================================================

mod all_formats {
    use super::*;

    #[test]
    fn human_format_on_terminal() {
        let detector = OutputDetector::with_env(
            OutputFormat::Human,
            false,
            OutputEnvironment::new(false, false, false, true),
        );
        assert!(detector.should_use_rich());
    }

    #[test]
    fn plain_format_on_terminal() {
        let detector = OutputDetector::with_env(
            OutputFormat::Plain,
            false,
            OutputEnvironment::new(false, false, false, true),
        );
        assert!(!detector.should_use_rich());
    }

    #[test]
    fn json_format_on_terminal() {
        let detector = OutputDetector::with_env(
            OutputFormat::Json,
            false,
            OutputEnvironment::new(false, false, false, true),
        );
        assert!(!detector.should_use_rich());
    }

    #[test]
    fn jsonl_format_on_terminal() {
        let detector = OutputDetector::with_env(
            OutputFormat::Jsonl,
            false,
            OutputEnvironment::new(false, false, false, true),
        );
        assert!(!detector.should_use_rich());
    }

    #[test]
    fn tsv_format_on_terminal() {
        let detector = OutputDetector::with_env(
            OutputFormat::Tsv,
            false,
            OutputEnvironment::new(false, false, false, true),
        );
        assert!(!detector.should_use_rich());
    }
}
