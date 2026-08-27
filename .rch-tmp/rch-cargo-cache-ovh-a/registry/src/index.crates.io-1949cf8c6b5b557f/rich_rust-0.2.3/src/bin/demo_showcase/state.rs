//! Demo state and simulation data for demo_showcase.

// State types prepared for dashboard/live scene implementations
#![allow(dead_code)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceHealth {
    Ok,
    Warn,
    Err,
}

impl ServiceHealth {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ok => "OK",
            Self::Warn => "WARN",
            Self::Err => "ERR",
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServiceInfo {
    pub name: String,
    pub health: ServiceHealth,
    pub latency: Duration,
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageStatus {
    Pending,
    Running,
    Done,
    Failed,
}

impl StageStatus {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Done => "done",
            Self::Failed => "failed",
        }
    }
}

#[derive(Debug, Clone)]
pub struct PipelineStage {
    pub name: String,
    pub status: StageStatus,
    pub progress: f64,
    pub eta: Option<Duration>,
}

impl PipelineStage {
    pub fn set_progress(&mut self, progress: f64) {
        self.progress = progress.clamp(0.0, 1.0);
    }
}

// ============================================================================
// Failure Narrative Types (bd-2wxz)
// ============================================================================
//
// The "failure moment" makes the demo feel real by showing how rich_rust helps
// when things go wrong. The failure must:
// 1. Be believable and related to earlier output (services/pipeline)
// 2. Justify showing: logs, inspect/pretty, and traceback
// 3. Allow recovery to show a final summary (unless --fail-fast)

/// The type of failure scenario to simulate.
///
/// Each scenario produces a distinct failure narrative with appropriate
/// log messages, error details, and state changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FailureScenario {
    /// Database connection timeout during deployment.
    /// - Trigger: db service goes ERR during deploy stage
    /// - Effect: cleanup stage fails, unable to mark deployment complete
    /// - Debug value: Shows connection pool state, retry attempts
    #[default]
    DatabaseTimeout,

    /// Configuration validation failure.
    /// - Trigger: Invalid config detected during verify stage
    /// - Effect: verify stage fails, deployment rolled back
    /// - Debug value: Shows config diff, validation errors
    ConfigValidation,

    /// Health check failure after deployment.
    /// - Trigger: API service fails health check after deploy completes
    /// - Effect: verify stage fails, triggers rollback
    /// - Debug value: Shows health check responses, expected vs actual
    HealthCheckFailure,

    /// Resource exhaustion during deployment.
    /// - Trigger: Worker service hits memory limit
    /// - Effect: deploy stage fails mid-progress
    /// - Debug value: Shows resource metrics, allocation history
    ResourceExhaustion,
}

impl FailureScenario {
    /// Human-readable name for this failure scenario.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::DatabaseTimeout => "Database Connection Timeout",
            Self::ConfigValidation => "Configuration Validation Error",
            Self::HealthCheckFailure => "Health Check Failure",
            Self::ResourceExhaustion => "Resource Exhaustion",
        }
    }

    /// Short code for this failure scenario (useful for logs).
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::DatabaseTimeout => "DB_TIMEOUT",
            Self::ConfigValidation => "CONFIG_INVALID",
            Self::HealthCheckFailure => "HEALTH_FAIL",
            Self::ResourceExhaustion => "OOM",
        }
    }

    /// The error message shown when this failure occurs.
    #[must_use]
    pub const fn error_message(self) -> &'static str {
        match self {
            Self::DatabaseTimeout => {
                "Connection to database timed out after 30s (max_retries=3 exhausted)"
            }
            Self::ConfigValidation => {
                "Configuration validation failed: missing required field 'api_key' in production profile"
            }
            Self::HealthCheckFailure => {
                "Health check failed: endpoint /health returned 503 Service Unavailable"
            }
            Self::ResourceExhaustion => {
                "Worker terminated: memory limit exceeded (used: 512MB, limit: 256MB)"
            }
        }
    }

    /// The stage that fails for this scenario.
    #[must_use]
    pub const fn failing_stage(self) -> &'static str {
        match self {
            Self::DatabaseTimeout => "cleanup",
            Self::ConfigValidation => "verify",
            Self::HealthCheckFailure => "verify",
            Self::ResourceExhaustion => "deploy",
        }
    }

    /// The service primarily affected by this failure.
    #[must_use]
    pub const fn affected_service(self) -> &'static str {
        match self {
            Self::DatabaseTimeout => "db",
            Self::ConfigValidation => "api",
            Self::HealthCheckFailure => "api",
            Self::ResourceExhaustion => "worker",
        }
    }
}

/// Captured details of a failure event.
///
/// This struct contains all information needed to display the failure
/// using rich_rust's debugging tools (traceback, inspect, pretty).
#[derive(Debug, Clone)]
pub struct FailureEvent {
    /// The type of failure that occurred.
    pub scenario: FailureScenario,

    /// When the failure occurred (relative to demo start).
    pub timestamp: Duration,

    /// The stage that was active when the failure occurred.
    pub stage: String,

    /// The error message.
    pub message: String,

    /// Additional context for debugging (key-value pairs).
    pub context: Vec<(String, String)>,

    /// Simulated stack trace frames.
    pub stack_frames: Vec<StackFrame>,
}

/// A single frame in a simulated stack trace.
#[derive(Debug, Clone)]
pub struct StackFrame {
    /// Function name.
    pub function: String,
    /// File path (relative).
    pub file: String,
    /// Line number.
    pub line: u32,
    /// Local variable context (for inspect).
    pub locals: Vec<(String, String)>,
}

impl FailureEvent {
    /// Create a new failure event for the given scenario.
    #[must_use]
    pub fn new(scenario: FailureScenario, timestamp: Duration) -> Self {
        let stage = scenario.failing_stage().to_string();
        let message = scenario.error_message().to_string();

        // Build context based on scenario
        let context = Self::build_context(scenario);
        let stack_frames = Self::build_stack_trace(scenario);

        Self {
            scenario,
            timestamp,
            stage,
            message,
            context,
            stack_frames,
        }
    }

    /// Build debugging context for a scenario.
    fn build_context(scenario: FailureScenario) -> Vec<(String, String)> {
        match scenario {
            FailureScenario::DatabaseTimeout => vec![
                ("host".to_string(), "db.nebula.internal:5432".to_string()),
                ("pool_size".to_string(), "10".to_string()),
                ("active_connections".to_string(), "0".to_string()),
                ("timeout_ms".to_string(), "30000".to_string()),
                ("retry_count".to_string(), "3".to_string()),
                ("last_error".to_string(), "ETIMEDOUT".to_string()),
            ],
            FailureScenario::ConfigValidation => vec![
                ("profile".to_string(), "production".to_string()),
                ("config_file".to_string(), "deploy.toml".to_string()),
                (
                    "missing_fields".to_string(),
                    "[api_key, secret]".to_string(),
                ),
                ("validation_mode".to_string(), "strict".to_string()),
            ],
            FailureScenario::HealthCheckFailure => vec![
                ("endpoint".to_string(), "/health".to_string()),
                ("expected_status".to_string(), "200".to_string()),
                ("actual_status".to_string(), "503".to_string()),
                ("response_time_ms".to_string(), "2847".to_string()),
                ("check_interval".to_string(), "5s".to_string()),
            ],
            FailureScenario::ResourceExhaustion => vec![
                ("process".to_string(), "worker-3".to_string()),
                ("memory_used".to_string(), "524288000".to_string()),
                ("memory_limit".to_string(), "268435456".to_string()),
                ("oom_score".to_string(), "999".to_string()),
                ("killed_at".to_string(), "2026-01-28T02:30:45Z".to_string()),
            ],
        }
    }

    /// Build a simulated stack trace for a scenario.
    fn build_stack_trace(scenario: FailureScenario) -> Vec<StackFrame> {
        match scenario {
            FailureScenario::DatabaseTimeout => vec![
                StackFrame {
                    function: "nebula::db::pool::acquire".to_string(),
                    file: "src/db/pool.rs".to_string(),
                    line: 142,
                    locals: vec![
                        ("timeout".to_string(), "Duration(30s)".to_string()),
                        ("attempts".to_string(), "3".to_string()),
                    ],
                },
                StackFrame {
                    function: "nebula::db::Connection::connect".to_string(),
                    file: "src/db/connection.rs".to_string(),
                    line: 87,
                    locals: vec![
                        ("host".to_string(), "\"db.nebula.internal\"".to_string()),
                        ("port".to_string(), "5432".to_string()),
                    ],
                },
                StackFrame {
                    function: "nebula::deploy::cleanup::mark_complete".to_string(),
                    file: "src/deploy/cleanup.rs".to_string(),
                    line: 203,
                    locals: vec![("deployment_id".to_string(), "\"deploy-7f3a2b\"".to_string())],
                },
                StackFrame {
                    function: "nebula::pipeline::Stage::run".to_string(),
                    file: "src/pipeline/stage.rs".to_string(),
                    line: 56,
                    locals: vec![("stage_name".to_string(), "\"cleanup\"".to_string())],
                },
            ],
            FailureScenario::ConfigValidation => vec![
                StackFrame {
                    function: "nebula::config::Validator::validate".to_string(),
                    file: "src/config/validator.rs".to_string(),
                    line: 78,
                    locals: vec![
                        ("mode".to_string(), "Strict".to_string()),
                        (
                            "errors".to_string(),
                            "vec![\"missing: api_key\"]".to_string(),
                        ),
                    ],
                },
                StackFrame {
                    function: "nebula::config::Config::load".to_string(),
                    file: "src/config/mod.rs".to_string(),
                    line: 134,
                    locals: vec![("path".to_string(), "\"deploy.toml\"".to_string())],
                },
                StackFrame {
                    function: "nebula::deploy::verify::check_config".to_string(),
                    file: "src/deploy/verify.rs".to_string(),
                    line: 45,
                    locals: vec![],
                },
            ],
            FailureScenario::HealthCheckFailure => vec![
                StackFrame {
                    function: "nebula::health::check_endpoint".to_string(),
                    file: "src/health/checker.rs".to_string(),
                    line: 92,
                    locals: vec![
                        ("url".to_string(), "\"http://api:8080/health\"".to_string()),
                        ("status".to_string(), "503".to_string()),
                    ],
                },
                StackFrame {
                    function: "nebula::deploy::verify::health_check".to_string(),
                    file: "src/deploy/verify.rs".to_string(),
                    line: 112,
                    locals: vec![("retries".to_string(), "0".to_string())],
                },
                StackFrame {
                    function: "nebula::pipeline::Stage::run".to_string(),
                    file: "src/pipeline/stage.rs".to_string(),
                    line: 56,
                    locals: vec![("stage_name".to_string(), "\"verify\"".to_string())],
                },
            ],
            FailureScenario::ResourceExhaustion => vec![
                StackFrame {
                    function: "nebula::worker::allocate_buffer".to_string(),
                    file: "src/worker/memory.rs".to_string(),
                    line: 234,
                    locals: vec![
                        ("requested".to_string(), "268435456".to_string()),
                        ("available".to_string(), "0".to_string()),
                    ],
                },
                StackFrame {
                    function: "nebula::worker::process_batch".to_string(),
                    file: "src/worker/processor.rs".to_string(),
                    line: 156,
                    locals: vec![("batch_size".to_string(), "10000".to_string())],
                },
                StackFrame {
                    function: "nebula::deploy::execute::run_workers".to_string(),
                    file: "src/deploy/execute.rs".to_string(),
                    line: 89,
                    locals: vec![("worker_count".to_string(), "4".to_string())],
                },
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "TRACE",
            Self::Debug => "DEBUG",
            Self::Info => "INFO",
            Self::Warn => "WARN",
            Self::Error => "ERROR",
        }
    }
}

#[derive(Debug, Clone)]
pub struct LogLine {
    pub t: Duration,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct DemoState {
    pub run_id: u64,
    pub seed: u64,
    started_at: Instant,
    pub headline: String,
    pub services: Vec<ServiceInfo>,
    pub pipeline: Vec<PipelineStage>,
    logs: VecDeque<LogLine>,
    log_capacity: usize,
    /// The failure event if one has occurred (for debugging demos).
    pub failure: Option<FailureEvent>,
    /// The configured failure scenario (None = no failure).
    failure_scenario: Option<FailureScenario>,
}

impl DemoState {
    #[must_use]
    pub fn new(run_id: u64, seed: u64) -> Self {
        Self {
            run_id,
            seed,
            started_at: Instant::now(),
            headline: String::new(),
            services: Vec::new(),
            pipeline: Vec::new(),
            logs: VecDeque::new(),
            log_capacity: 200,
            failure: None,
            failure_scenario: None,
        }
    }

    #[must_use]
    pub fn with_log_capacity(run_id: u64, seed: u64, log_capacity: usize) -> Self {
        Self {
            log_capacity: log_capacity.max(1),
            ..Self::new(run_id, seed)
        }
    }

    /// Create a demo state with a configured failure scenario.
    ///
    /// The failure will be triggered later via `trigger_failure()`.
    #[must_use]
    pub fn with_failure_scenario(run_id: u64, seed: u64, scenario: FailureScenario) -> Self {
        let mut state = Self::with_log_capacity(run_id, seed, 200);
        state.failure_scenario = Some(scenario);
        state
    }

    /// Check if a failure scenario is configured.
    #[must_use]
    pub fn has_failure_scenario(&self) -> bool {
        self.failure_scenario.is_some()
    }

    /// Get the configured failure scenario, if any.
    #[must_use]
    pub fn failure_scenario(&self) -> Option<FailureScenario> {
        self.failure_scenario
    }

    /// Trigger the configured failure, updating state and logging the event.
    ///
    /// Returns `true` if a failure was triggered, `false` if no scenario was configured
    /// or a failure has already occurred.
    pub fn trigger_failure(&mut self) -> bool {
        let Some(scenario) = self.failure_scenario else {
            return false;
        };

        if self.failure.is_some() {
            return false; // Already failed
        }

        let timestamp = self.elapsed();

        // Update the affected service health
        let affected_service = scenario.affected_service();
        for svc in &mut self.services {
            if svc.name == affected_service {
                svc.health = ServiceHealth::Err;
                svc.latency = Duration::from_millis(0); // Connection lost
            }
        }

        // Update the failing stage
        let failing_stage = scenario.failing_stage();
        for stage in &mut self.pipeline {
            if stage.name == failing_stage {
                stage.status = StageStatus::Failed;
                // Progress stays at current value (frozen)
                stage.eta = None;
            }
        }

        // Generate failure log sequence
        self.generate_failure_logs(scenario);

        // Create and store the failure event
        self.failure = Some(FailureEvent::new(scenario, timestamp));

        // Update headline
        self.headline = format!("❌ {}", scenario.name());

        true
    }

    /// Generate the log sequence leading up to and including the failure.
    fn generate_failure_logs(&mut self, scenario: FailureScenario) {
        match scenario {
            FailureScenario::DatabaseTimeout => {
                self.push_log(LogLevel::Debug, "Connecting to db.nebula.internal:5432...");
                self.push_log(LogLevel::Warn, "Connection attempt 1/3 timed out");
                self.push_log(LogLevel::Debug, "Retrying connection (backoff: 1s)...");
                self.push_log(LogLevel::Warn, "Connection attempt 2/3 timed out");
                self.push_log(LogLevel::Debug, "Retrying connection (backoff: 2s)...");
                self.push_log(LogLevel::Warn, "Connection attempt 3/3 timed out");
                self.push_log(
                    LogLevel::Error,
                    "DB_TIMEOUT: Connection to database timed out after 30s",
                );
                self.push_log(
                    LogLevel::Error,
                    "cleanup stage failed: cannot mark deployment complete",
                );
            }
            FailureScenario::ConfigValidation => {
                self.push_log(LogLevel::Debug, "Loading configuration from deploy.toml");
                self.push_log(LogLevel::Debug, "Profile: production (strict validation)");
                self.push_log(
                    LogLevel::Warn,
                    "Config warning: deprecated field 'legacy_mode'",
                );
                self.push_log(
                    LogLevel::Error,
                    "CONFIG_INVALID: Missing required field 'api_key'",
                );
                self.push_log(
                    LogLevel::Error,
                    "verify stage failed: configuration validation error",
                );
            }
            FailureScenario::HealthCheckFailure => {
                self.push_log(
                    LogLevel::Info,
                    "Deployment complete, starting health checks",
                );
                self.push_log(LogLevel::Debug, "GET /health -> connecting to api:8080");
                self.push_log(
                    LogLevel::Warn,
                    "Health check returned 503 Service Unavailable",
                );
                self.push_log(
                    LogLevel::Debug,
                    "Response body: {\"status\":\"degraded\",\"db\":\"disconnected\"}",
                );
                self.push_log(
                    LogLevel::Error,
                    "HEALTH_FAIL: endpoint /health returned 503",
                );
                self.push_log(
                    LogLevel::Error,
                    "verify stage failed: health check did not pass",
                );
            }
            FailureScenario::ResourceExhaustion => {
                self.push_log(LogLevel::Info, "Starting worker batch processing");
                self.push_log(LogLevel::Debug, "worker-3: processing batch of 10000 items");
                self.push_log(
                    LogLevel::Warn,
                    "worker-3: memory usage at 90% (230MB/256MB)",
                );
                self.push_log(
                    LogLevel::Warn,
                    "worker-3: memory usage at 100% (256MB/256MB)",
                );
                self.push_log(
                    LogLevel::Error,
                    "OOM: worker-3 killed by OOM killer (512MB requested, 256MB limit)",
                );
                self.push_log(
                    LogLevel::Error,
                    "deploy stage failed: worker process terminated",
                );
            }
        }
    }

    /// Generate pre-failure logs that build up tension before the failure.
    ///
    /// Call this during the simulation to add realistic log messages before
    /// `trigger_failure()` is called.
    pub fn generate_prefailure_logs(&mut self, scenario: FailureScenario) {
        match scenario {
            FailureScenario::DatabaseTimeout => {
                self.push_log(LogLevel::Info, "Starting cleanup stage");
                self.push_log(LogLevel::Debug, "Checking connection pool status");
                self.push_log(LogLevel::Warn, "Connection pool: 0 active, 0 idle");
            }
            FailureScenario::ConfigValidation => {
                self.push_log(LogLevel::Info, "Starting verify stage");
                self.push_log(LogLevel::Debug, "Validating deployment configuration");
            }
            FailureScenario::HealthCheckFailure => {
                self.push_log(LogLevel::Info, "Deploy stage complete");
                self.push_log(LogLevel::Debug, "Containers started successfully");
                self.push_log(LogLevel::Info, "Starting verify stage");
            }
            FailureScenario::ResourceExhaustion => {
                self.push_log(LogLevel::Info, "Deploy stage in progress");
                self.push_log(LogLevel::Debug, "Scaling workers to 4 replicas");
                self.push_log(LogLevel::Info, "Workers starting batch processing");
            }
        }
    }

    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.started_at.elapsed()
    }

    pub fn push_log(&mut self, level: LogLevel, message: impl Into<String>) {
        let line = LogLine {
            t: self.elapsed(),
            level,
            message: message.into(),
        };

        self.logs.push_back(line);
        while self.logs.len() > self.log_capacity {
            self.logs.pop_front();
        }
    }

    #[must_use]
    pub fn logs_snapshot(&self) -> Vec<LogLine> {
        self.logs.iter().cloned().collect()
    }

    #[must_use]
    pub fn demo_seeded(run_id: u64, seed: u64) -> Self {
        let mut state = Self::with_log_capacity(run_id, seed, 200);

        state.headline = "Booting Nebula Deploy…".to_string();

        state.services = vec![
            ServiceInfo {
                name: "api".to_string(),
                health: ServiceHealth::Ok,
                latency: Duration::from_millis(12),
                version: "1.2.3".to_string(),
            },
            ServiceInfo {
                name: "worker".to_string(),
                health: ServiceHealth::Warn,
                latency: Duration::from_millis(48),
                version: "1.2.3".to_string(),
            },
            ServiceInfo {
                name: "db".to_string(),
                health: ServiceHealth::Err,
                latency: Duration::from_millis(0),
                version: "13.4".to_string(),
            },
        ];

        let service_logs: Vec<String> = state
            .services
            .iter()
            .map(|service| {
                format!(
                    "svc {}: {} ({}ms) v{}",
                    service.name,
                    service.health.as_str(),
                    service.latency.as_millis(),
                    service.version
                )
            })
            .collect();

        for line in service_logs {
            state.push_log(LogLevel::Info, line);
        }

        let mut stage_plan = PipelineStage {
            name: "plan".to_string(),
            status: StageStatus::Done,
            progress: 1.0,
            eta: None,
        };
        stage_plan.set_progress(1.0);

        let mut stage_deploy = PipelineStage {
            name: "deploy".to_string(),
            status: StageStatus::Running,
            progress: 0.0,
            eta: Some(Duration::from_secs(12)),
        };
        stage_deploy.set_progress(0.42);

        let stage_verify = PipelineStage {
            name: "verify".to_string(),
            status: StageStatus::Pending,
            progress: 0.0,
            eta: None,
        };

        let stage_cleanup = PipelineStage {
            name: "cleanup".to_string(),
            status: StageStatus::Failed,
            progress: 0.0,
            eta: None,
        };

        state.pipeline = vec![stage_plan, stage_deploy, stage_verify, stage_cleanup];

        let stage_logs: Vec<String> = state
            .pipeline
            .iter()
            .map(|stage| {
                let eta = stage
                    .eta
                    .map(|d| format!(" eta={}s", d.as_secs()))
                    .unwrap_or_default();
                format!("stage {} -> {}{}", stage.name, stage.status.as_str(), eta)
            })
            .collect();

        for line in stage_logs {
            state.push_log(LogLevel::Debug, line);
        }

        for level in [
            LogLevel::Trace,
            LogLevel::Debug,
            LogLevel::Info,
            LogLevel::Warn,
            LogLevel::Error,
        ] {
            state.push_log(level, format!("{}: demo log line", level.as_str()));
        }

        state
    }

    /// Create a demo state that has experienced a failure.
    ///
    /// This is useful for scenes that want to show the aftermath of a failure
    /// and demonstrate rich_rust's debugging capabilities (traceback, inspect, etc.).
    ///
    /// The state includes:
    /// - A populated failure event with stack trace
    /// - Updated service health reflecting the failure
    /// - Updated pipeline stages with the failed stage
    /// - Log messages showing the failure sequence
    #[must_use]
    pub fn demo_with_failure(run_id: u64, seed: u64, scenario: FailureScenario) -> Self {
        let mut state = Self::with_log_capacity(run_id, seed, 200);

        // Start with a "before failure" headline
        state.headline = "Deploying Nebula v1.2.3…".to_string();
        state.failure_scenario = Some(scenario);

        // Set up services (initially healthy, except for hints of trouble)
        state.services = vec![
            ServiceInfo {
                name: "api".to_string(),
                health: ServiceHealth::Ok,
                latency: Duration::from_millis(12),
                version: "1.2.3".to_string(),
            },
            ServiceInfo {
                name: "worker".to_string(),
                health: ServiceHealth::Ok,
                latency: Duration::from_millis(25),
                version: "1.2.3".to_string(),
            },
            ServiceInfo {
                name: "db".to_string(),
                health: ServiceHealth::Ok,
                latency: Duration::from_millis(8),
                version: "13.4".to_string(),
            },
        ];

        // Set up pipeline stages (deployment in progress)
        state.pipeline = vec![
            PipelineStage {
                name: "plan".to_string(),
                status: StageStatus::Done,
                progress: 1.0,
                eta: None,
            },
            PipelineStage {
                name: "deploy".to_string(),
                status: StageStatus::Done,
                progress: 1.0,
                eta: None,
            },
            PipelineStage {
                name: "verify".to_string(),
                status: StageStatus::Running,
                progress: 0.6,
                eta: Some(Duration::from_secs(5)),
            },
            PipelineStage {
                name: "cleanup".to_string(),
                status: StageStatus::Pending,
                progress: 0.0,
                eta: None,
            },
        ];

        // Generate startup logs
        state.push_log(LogLevel::Info, "Nebula Deploy v1.2.3 starting");
        state.push_log(LogLevel::Info, "Environment: production");
        state.push_log(LogLevel::Debug, "Loading deployment manifest...");
        state.push_log(LogLevel::Info, "plan stage complete");
        state.push_log(LogLevel::Info, "deploy stage complete");
        state.push_log(LogLevel::Info, "Starting verify stage");

        // Generate pre-failure tension
        state.generate_prefailure_logs(scenario);

        // Now trigger the failure
        state.trigger_failure();

        state
    }
}

#[derive(Debug, Clone)]
pub struct DemoStateSnapshot {
    pub run_id: u64,
    pub seed: u64,
    pub elapsed: Duration,
    pub headline: String,
    pub services: Vec<ServiceInfo>,
    pub pipeline: Vec<PipelineStage>,
    pub logs: Vec<LogLine>,
    /// The failure event if one has occurred.
    pub failure: Option<FailureEvent>,
}

impl DemoStateSnapshot {
    /// Check if this snapshot represents a failed state.
    #[must_use]
    pub fn has_failure(&self) -> bool {
        self.failure.is_some()
    }
}

impl From<&DemoState> for DemoStateSnapshot {
    fn from(value: &DemoState) -> Self {
        Self {
            run_id: value.run_id,
            seed: value.seed,
            elapsed: value.elapsed(),
            headline: value.headline.clone(),
            services: value.services.clone(),
            pipeline: value.pipeline.clone(),
            logs: value.logs_snapshot(),
            failure: value.failure.clone(),
        }
    }
}

#[derive(Clone)]
pub struct SharedDemoState {
    inner: Arc<Mutex<DemoState>>,
}

impl SharedDemoState {
    #[must_use]
    pub fn new(run_id: u64, seed: u64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(DemoState::new(run_id, seed))),
        }
    }

    #[must_use]
    pub fn demo_seeded(run_id: u64, seed: u64) -> Self {
        Self {
            inner: Arc::new(Mutex::new(DemoState::demo_seeded(run_id, seed))),
        }
    }

    /// Create a shared demo state that has experienced a failure.
    ///
    /// See [`DemoState::demo_with_failure`] for details.
    #[must_use]
    pub fn demo_with_failure(run_id: u64, seed: u64, scenario: FailureScenario) -> Self {
        Self {
            inner: Arc::new(Mutex::new(DemoState::demo_with_failure(
                run_id, seed, scenario,
            ))),
        }
    }

    /// Update the demo state atomically.
    ///
    /// # Poison Recovery
    /// If a previous holder panicked while holding the lock, we recover by
    /// extracting the inner value. This is acceptable for demo_showcase because:
    /// - The state is ephemeral (demo session only)
    /// - A corrupted state just means visual glitches, not data loss
    /// - We prefer graceful degradation over cascading panics
    pub fn update<F>(&self, f: F)
    where
        F: FnOnce(&mut DemoState),
    {
        let mut guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut guard);
    }

    /// Take a snapshot of the current demo state.
    ///
    /// # Poison Recovery
    /// Same rationale as `update` - we recover from poison rather than panic.
    #[must_use]
    pub fn snapshot(&self) -> DemoStateSnapshot {
        let guard = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        DemoStateSnapshot::from(&*guard)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_ring_buffer_caps() {
        let mut state = DemoState::with_log_capacity(1, 2, 2);
        state.push_log(LogLevel::Info, "one");
        state.push_log(LogLevel::Info, "two");
        state.push_log(LogLevel::Info, "three");

        let logs = state.logs_snapshot();
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0].message, "two");
        assert_eq!(logs[1].message, "three");
    }

    #[test]
    fn shared_snapshot_is_clone_safe() {
        let shared = SharedDemoState::new(123, 456);
        shared.update(|state| {
            state.headline = "Starting".to_string();
            state.services.push(ServiceInfo {
                name: "api".to_string(),
                health: ServiceHealth::Ok,
                latency: Duration::from_millis(12),
                version: "1.2.3".to_string(),
            });
            state.push_log(LogLevel::Info, "hello");
        });

        let snap = shared.snapshot();
        assert_eq!(snap.run_id, 123);
        assert_eq!(snap.seed, 456);
        assert_eq!(snap.headline, "Starting");
        assert_eq!(snap.services.len(), 1);
        assert_eq!(snap.logs.len(), 1);
        assert_eq!(snap.logs[0].level, LogLevel::Info);
    }

    // ========== Failure Narrative Tests (bd-2wxz) ==========

    #[test]
    fn failure_scenario_has_correct_metadata() {
        assert_eq!(
            FailureScenario::DatabaseTimeout.name(),
            "Database Connection Timeout"
        );
        assert_eq!(FailureScenario::DatabaseTimeout.code(), "DB_TIMEOUT");
        assert_eq!(FailureScenario::DatabaseTimeout.failing_stage(), "cleanup");
        assert_eq!(FailureScenario::DatabaseTimeout.affected_service(), "db");

        assert_eq!(
            FailureScenario::ConfigValidation.name(),
            "Configuration Validation Error"
        );
        assert_eq!(FailureScenario::ConfigValidation.code(), "CONFIG_INVALID");
        assert_eq!(FailureScenario::ConfigValidation.failing_stage(), "verify");

        assert_eq!(
            FailureScenario::HealthCheckFailure.name(),
            "Health Check Failure"
        );
        assert_eq!(
            FailureScenario::HealthCheckFailure.failing_stage(),
            "verify"
        );

        assert_eq!(
            FailureScenario::ResourceExhaustion.name(),
            "Resource Exhaustion"
        );
        assert_eq!(FailureScenario::ResourceExhaustion.code(), "OOM");
        assert_eq!(
            FailureScenario::ResourceExhaustion.failing_stage(),
            "deploy"
        );
    }

    #[test]
    fn failure_event_has_context_and_stack_trace() {
        let event = FailureEvent::new(FailureScenario::DatabaseTimeout, Duration::from_secs(5));

        assert_eq!(event.scenario, FailureScenario::DatabaseTimeout);
        assert_eq!(event.stage, "cleanup");
        assert!(!event.message.is_empty());
        assert!(!event.context.is_empty(), "should have context entries");
        assert!(!event.stack_frames.is_empty(), "should have stack frames");

        // Verify stack trace has expected structure
        let frame = &event.stack_frames[0];
        assert!(!frame.function.is_empty());
        assert!(!frame.file.is_empty());
        assert!(frame.line > 0);
    }

    #[test]
    fn trigger_failure_updates_state() {
        let mut state = DemoState::with_failure_scenario(1, 42, FailureScenario::DatabaseTimeout);

        // Initially no failure
        assert!(state.failure.is_none());
        assert!(state.has_failure_scenario());

        // Set up services and pipeline so trigger_failure can update them
        state.services = vec![ServiceInfo {
            name: "db".to_string(),
            health: ServiceHealth::Ok,
            latency: Duration::from_millis(10),
            version: "1.0".to_string(),
        }];
        state.pipeline = vec![PipelineStage {
            name: "cleanup".to_string(),
            status: StageStatus::Running,
            progress: 0.5,
            eta: Some(Duration::from_secs(10)),
        }];

        // Trigger failure
        let triggered = state.trigger_failure();
        assert!(triggered, "should trigger failure");

        // Verify state was updated
        assert!(state.failure.is_some());
        assert_eq!(state.services[0].health, ServiceHealth::Err);
        assert_eq!(state.pipeline[0].status, StageStatus::Failed);
        assert!(state.headline.contains("Database Connection Timeout"));

        // Second trigger should return false
        let triggered_again = state.trigger_failure();
        assert!(!triggered_again, "should not trigger twice");
    }

    #[test]
    fn demo_with_failure_creates_complete_failure_state() {
        let state = DemoState::demo_with_failure(1, 42, FailureScenario::DatabaseTimeout);

        // Should have failure event
        assert!(state.failure.is_some());
        let failure = state.failure.as_ref().unwrap();
        assert_eq!(failure.scenario, FailureScenario::DatabaseTimeout);

        // Should have updated headline
        assert!(state.headline.contains("Database Connection Timeout"));

        // Should have logs (pre-failure + failure logs)
        let logs = state.logs_snapshot();
        assert!(!logs.is_empty());

        // Should have at least one error log
        let has_error = logs.iter().any(|l| l.level == LogLevel::Error);
        assert!(has_error, "should have error logs");
    }

    #[test]
    fn all_failure_scenarios_create_valid_events() {
        let scenarios = [
            FailureScenario::DatabaseTimeout,
            FailureScenario::ConfigValidation,
            FailureScenario::HealthCheckFailure,
            FailureScenario::ResourceExhaustion,
        ];

        for scenario in scenarios {
            let state = DemoState::demo_with_failure(1, 42, scenario);
            let failure = state.failure.as_ref().unwrap();

            // Each scenario should have valid metadata
            assert!(
                !failure.message.is_empty(),
                "{:?} should have message",
                scenario
            );
            assert!(
                !failure.context.is_empty(),
                "{:?} should have context",
                scenario
            );
            assert!(
                !failure.stack_frames.is_empty(),
                "{:?} should have stack frames",
                scenario
            );

            // Stack frames should have realistic structure
            for frame in &failure.stack_frames {
                assert!(
                    frame.function.contains("::"),
                    "function should be namespaced"
                );
                assert!(frame.file.ends_with(".rs"), "file should be Rust source");
                assert!(frame.line > 0, "line should be positive");
            }
        }
    }

    #[test]
    fn snapshot_includes_failure() {
        let state = DemoState::demo_with_failure(1, 42, FailureScenario::HealthCheckFailure);
        let snapshot = DemoStateSnapshot::from(&state);

        assert!(snapshot.has_failure());
        assert!(snapshot.failure.is_some());
        let failure = snapshot.failure.unwrap();
        assert_eq!(failure.scenario, FailureScenario::HealthCheckFailure);
    }

    #[test]
    fn shared_demo_with_failure_works() {
        let shared = SharedDemoState::demo_with_failure(1, 42, FailureScenario::ConfigValidation);
        let snapshot = shared.snapshot();

        assert!(snapshot.has_failure());
        assert_eq!(
            snapshot.failure.unwrap().scenario,
            FailureScenario::ConfigValidation
        );
    }
}
