//! Pipeline simulation logic for demo_showcase.
//!
//! This module provides the logic to simulate a realistic deployment pipeline
//! with stages, progress updates, and log entries. It's designed to work with
//! the `DemoState` model and respect `--quick`/`--speed` timing settings.

// Some helper functions prepared for future scene implementations
#![allow(dead_code)]

use std::time::Duration;

use crate::keys;
use crate::state::{LogLevel, PipelineStage, SharedDemoState, StageStatus};
use crate::timing::Timing;

/// Standard deployment pipeline stages.
pub const PIPELINE_STAGES: &[&str] = &[
    "lint",
    "build",
    "unit_tests",
    "package",
    "deploy",
    "smoke_tests",
];

/// Initialize the pipeline with standard stages.
pub fn init_pipeline(state: &SharedDemoState) {
    state.update(|demo| {
        demo.pipeline = PIPELINE_STAGES
            .iter()
            .map(|&name| PipelineStage {
                name: name.to_string(),
                status: StageStatus::Pending,
                progress: 0.0,
                eta: None,
            })
            .collect();
        demo.push_log(LogLevel::Info, "Pipeline initialized");
    });
}

/// Configuration for a simulated stage.
#[derive(Debug, Clone)]
pub struct StageConfig {
    /// Base duration for this stage (before speed scaling).
    pub duration: Duration,
    /// Whether this stage can fail.
    pub can_fail: bool,
    /// Probability of failure (0.0-1.0) if can_fail is true.
    pub failure_prob: f64,
}

impl Default for StageConfig {
    fn default() -> Self {
        Self {
            duration: Duration::from_secs(2),
            can_fail: false,
            failure_prob: 0.0,
        }
    }
}

/// Get the default configuration for a stage by name.
#[must_use]
pub fn stage_config(name: &str) -> StageConfig {
    match name {
        "lint" => StageConfig {
            duration: Duration::from_millis(1500),
            can_fail: true,
            failure_prob: 0.05,
        },
        "build" => StageConfig {
            duration: Duration::from_secs(3),
            can_fail: true,
            failure_prob: 0.1,
        },
        "unit_tests" => StageConfig {
            duration: Duration::from_secs(4),
            can_fail: true,
            failure_prob: 0.15,
        },
        "package" => StageConfig {
            duration: Duration::from_millis(1200),
            can_fail: false,
            failure_prob: 0.0,
        },
        "deploy" => StageConfig {
            duration: Duration::from_secs(5),
            can_fail: true,
            failure_prob: 0.1,
        },
        "smoke_tests" => StageConfig {
            duration: Duration::from_secs(2),
            can_fail: true,
            failure_prob: 0.08,
        },
        _ => StageConfig::default(),
    }
}

/// Result of simulating a single stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageResult {
    Success,
    Failed,
    /// User requested to quit early via keyboard input.
    Cancelled,
}

/// Simulate a single pipeline stage with progress updates.
///
/// This function:
/// 1. Sets the stage to Running
/// 2. Updates progress over the configured duration
/// 3. Sets the stage to Done or Failed based on outcome
/// 4. Logs the transition
///
/// Returns the final result of the stage.
pub fn simulate_stage(
    state: &SharedDemoState,
    stage_idx: usize,
    timing: &Timing,
    rng: &mut crate::timing::DemoRng,
    force_success: bool,
) -> StageResult {
    let stage_name = {
        let snapshot = state.snapshot();
        if stage_idx >= snapshot.pipeline.len() {
            return StageResult::Failed;
        }
        snapshot.pipeline[stage_idx].name.clone()
    };

    let config = stage_config(&stage_name);

    // Determine outcome upfront
    let will_fail = if force_success {
        false
    } else if config.can_fail {
        let roll = (rng.next_u64() % 1000) as f64 / 1000.0;
        roll < config.failure_prob
    } else {
        false
    };

    // Start the stage
    state.update(|demo| {
        if stage_idx < demo.pipeline.len() {
            demo.pipeline[stage_idx].status = StageStatus::Running;
            demo.pipeline[stage_idx].progress = 0.0;
            demo.pipeline[stage_idx].eta = Some(timing.scale(config.duration));
        }
        demo.push_log(
            LogLevel::Info,
            format!("[{}] Starting", stage_name.to_uppercase()),
        );
    });

    // Simulate progress
    let steps = 20;
    let step_duration = config.duration / steps;

    for step in 1..=steps {
        // Check for user quit request (non-blocking)
        if keys::should_quit() {
            state.update(|demo| {
                if stage_idx < demo.pipeline.len() {
                    demo.pipeline[stage_idx].status = StageStatus::Pending;
                    demo.pipeline[stage_idx].eta = None;
                }
                demo.push_log(LogLevel::Info, "Pipeline cancelled by user");
            });
            return StageResult::Cancelled;
        }

        timing.sleep(step_duration);

        let progress = step as f64 / steps as f64;

        // If it's going to fail, fail partway through
        if will_fail && progress > 0.6 {
            state.update(|demo| {
                if stage_idx < demo.pipeline.len() {
                    demo.pipeline[stage_idx].status = StageStatus::Failed;
                    demo.pipeline[stage_idx].progress = progress;
                    demo.pipeline[stage_idx].eta = None;
                }
                demo.push_log(
                    LogLevel::Error,
                    format!(
                        "[{}] FAILED at {:.0}%",
                        stage_name.to_uppercase(),
                        progress * 100.0
                    ),
                );
            });
            return StageResult::Failed;
        }

        state.update(|demo| {
            if stage_idx < demo.pipeline.len() {
                demo.pipeline[stage_idx].progress = progress;
                let remaining = config.duration.saturating_sub(step_duration * step);
                demo.pipeline[stage_idx].eta = Some(timing.scale(remaining));
            }
        });
    }

    // Stage completed successfully
    state.update(|demo| {
        if stage_idx < demo.pipeline.len() {
            demo.pipeline[stage_idx].status = StageStatus::Done;
            demo.pipeline[stage_idx].progress = 1.0;
            demo.pipeline[stage_idx].eta = None;
        }
        demo.push_log(
            LogLevel::Info,
            format!("[{}] Completed", stage_name.to_uppercase()),
        );
    });

    StageResult::Success
}

/// Run the full pipeline simulation.
///
/// Runs each stage in sequence. If a stage fails, subsequent stages remain
/// in Pending status and the function returns early.
///
/// # Arguments
/// * `state` - The shared demo state to update
/// * `timing` - Timing configuration for sleeps
/// * `rng` - Random number generator for failure simulation
/// * `force_success` - If true, all stages will succeed (useful for demos)
///
/// # Returns
/// `true` if the entire pipeline succeeded, `false` if any stage failed.
pub fn run_pipeline(
    state: &SharedDemoState,
    timing: &Timing,
    rng: &mut crate::timing::DemoRng,
    force_success: bool,
) -> bool {
    init_pipeline(state);

    state.update(|demo| {
        demo.headline = "Pipeline running...".to_string();
    });

    let stage_count = PIPELINE_STAGES.len();

    for idx in 0..stage_count {
        let result = simulate_stage(state, idx, timing, rng, force_success);

        match result {
            StageResult::Success => continue,
            StageResult::Failed => {
                state.update(|demo| {
                    demo.headline = format!("Pipeline failed at stage {}/{}", idx + 1, stage_count);
                });
                return false;
            }
            StageResult::Cancelled => {
                state.update(|demo| {
                    demo.headline = "Pipeline cancelled".to_string();
                });
                return false;
            }
        }
    }

    state.update(|demo| {
        demo.headline = "Pipeline completed successfully!".to_string();
        demo.push_log(LogLevel::Info, "All stages completed");
    });

    true
}

/// Render a progress bar for a pipeline stage.
///
/// Returns a configured `ProgressBar` renderable for the given stage.
#[must_use]
pub fn stage_progress_bar(
    stage: &PipelineStage,
    width: usize,
) -> rich_rust::renderables::ProgressBar {
    use rich_rust::renderables::ProgressBar;

    let completed = (stage.progress * 100.0).round() as u64;
    let mut bar = ProgressBar::with_total(100).width(width);
    bar.update(completed);

    if let Some(eta) = stage.eta {
        bar = bar.description(format!("{} (eta: {}s)", stage.name, eta.as_secs()));
    } else {
        bar = bar.description(stage.name.as_str());
    }

    if stage.status == StageStatus::Done {
        bar = bar.finished_message("Done");
    }

    bar
}

/// Get an appropriate spinner for a running stage.
#[must_use]
pub fn stage_spinner() -> rich_rust::renderables::Spinner {
    rich_rust::renderables::Spinner::dots()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::timing::DemoRng;

    #[test]
    fn test_init_pipeline_creates_stages() {
        let state = SharedDemoState::new(1, 0);
        init_pipeline(&state);

        let snapshot = state.snapshot();
        assert_eq!(snapshot.pipeline.len(), PIPELINE_STAGES.len());
        for stage in &snapshot.pipeline {
            assert_eq!(stage.status, StageStatus::Pending);
            assert_eq!(stage.progress, 0.0);
        }
    }

    #[test]
    fn test_stage_config_varies_by_name() {
        let lint = stage_config("lint");
        let build = stage_config("build");

        assert!(lint.duration < build.duration);
        assert!(lint.can_fail);
        assert!(build.can_fail);
    }

    #[test]
    fn test_simulate_stage_success() {
        let state = SharedDemoState::new(1, 0);
        init_pipeline(&state);

        let timing = Timing::new(1.0, true); // Quick mode for fast tests
        let mut rng = DemoRng::new(0);

        let result = simulate_stage(&state, 0, &timing, &mut rng, true);
        assert_eq!(result, StageResult::Success);

        let snapshot = state.snapshot();
        assert_eq!(snapshot.pipeline[0].status, StageStatus::Done);
        assert_eq!(snapshot.pipeline[0].progress, 1.0);
    }

    #[test]
    fn test_run_pipeline_force_success() {
        let state = SharedDemoState::new(1, 0);
        let timing = Timing::new(1.0, true); // Quick mode
        let mut rng = DemoRng::new(0);

        let success = run_pipeline(&state, &timing, &mut rng, true);
        assert!(success);

        let snapshot = state.snapshot();
        for stage in &snapshot.pipeline {
            assert_eq!(stage.status, StageStatus::Done);
        }
    }

    #[test]
    fn test_stage_progress_bar_configuration() {
        let stage = PipelineStage {
            name: "build".to_string(),
            status: StageStatus::Running,
            progress: 0.5,
            eta: Some(Duration::from_secs(3)),
        };

        let bar = stage_progress_bar(&stage, 40);
        // Just verify it creates without panicking
        let _ = bar;
    }
}
