use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{MsError, Result};
use crate::security::{AcipConfig, TrustLevel};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub skill_paths: SkillPathsConfig,
    #[serde(default)]
    pub layers: LayersConfig,
    #[serde(default)]
    pub disclosure: DisclosureConfig,
    #[serde(default)]
    pub search: SearchConfig,
    #[serde(default)]
    pub cass: CassConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub update: UpdateConfig,
    #[serde(default)]
    pub robot: RobotConfig,
    #[serde(default)]
    pub security: SecurityConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            skill_paths: SkillPathsConfig::default(),
            layers: LayersConfig::default(),
            disclosure: DisclosureConfig::default(),
            search: SearchConfig::default(),
            cass: CassConfig::default(),
            cache: CacheConfig::default(),
            update: UpdateConfig::default(),
            robot: RobotConfig::default(),
            security: SecurityConfig::default(),
        }
    }
}

impl Config {
    pub fn load(explicit_path: Option<&Path>, ms_root: &Path) -> Result<Self> {
        let mut config = Self::default();

        let explicit = explicit_path
            .map(PathBuf::from)
            .or_else(|| std::env::var("MS_CONFIG").ok().map(PathBuf::from));

        if let Some(path) = explicit {
            if let Some(patch) = Self::load_patch(&path)? {
                config.merge_patch(patch);
            }
        } else {
            if let Some(global) = Self::load_global()? {
                config.merge_patch(global);
            }
            if let Some(project) = Self::load_project(ms_root)? {
                config.merge_patch(project);
            }
        }

        config.apply_env_overrides()?;

        Ok(config)
    }

    fn load_global() -> Result<Option<ConfigPatch>> {
        let path = dirs::config_dir()
            .ok_or_else(|| MsError::MissingConfig("config directory not found".to_string()))?
            .join("ms/config.toml");
        Self::load_patch(&path)
    }

    fn load_project(ms_root: &Path) -> Result<Option<ConfigPatch>> {
        let path = ms_root.join("config.toml");
        Self::load_patch(&path)
    }

    fn load_patch(path: &Path) -> Result<Option<ConfigPatch>> {
        if !path.exists() {
            return Ok(None);
        }

        let raw = std::fs::read_to_string(path)
            .map_err(|err| MsError::Config(format!("read config {}: {err}", path.display())))?;
        let patch = toml::from_str(&raw)
            .map_err(|err| MsError::Config(format!("parse config {}: {err}", path.display())))?;
        Ok(Some(patch))
    }

    fn merge_patch(&mut self, patch: ConfigPatch) {
        if let Some(patch) = patch.skill_paths {
            self.skill_paths.merge(patch);
        }
        if let Some(patch) = patch.layers {
            self.layers.merge(patch);
        }
        if let Some(patch) = patch.disclosure {
            self.disclosure.merge(patch);
        }
        if let Some(patch) = patch.search {
            self.search.merge(patch);
        }
        if let Some(patch) = patch.cass {
            self.cass.merge(patch);
        }
        if let Some(patch) = patch.cache {
            self.cache.merge(patch);
        }
        if let Some(patch) = patch.update {
            self.update.merge(patch);
        }
        if let Some(patch) = patch.robot {
            self.robot.merge(patch);
        }
        if let Some(patch) = patch.security {
            self.security.merge(patch);
        }
    }

    fn apply_env_overrides(&mut self) -> Result<()> {
        if env_bool("MS_ROBOT").unwrap_or(false) {
            self.robot.format = "json".to_string();
            self.robot.include_metadata = true;
        }
        if env_bool("MS_CACHE_DISABLED").unwrap_or(false) {
            self.cache.enabled = false;
        }

        if let Some(values) = env_list("MS_SKILL_PATHS_GLOBAL")? {
            self.skill_paths.global = merge_unique(values, &self.skill_paths.global);
        }
        if let Some(values) = env_list("MS_SKILL_PATHS_PROJECT")? {
            self.skill_paths.project = merge_unique(values, &self.skill_paths.project);
        }
        if let Some(values) = env_list("MS_SKILL_PATHS_COMMUNITY")? {
            self.skill_paths.community = merge_unique(values, &self.skill_paths.community);
        }
        if let Some(values) = env_list("MS_SKILL_PATHS_LOCAL")? {
            self.skill_paths.local = merge_unique(values, &self.skill_paths.local);
        }

        if let Some(values) = env_list("MS_LAYERS_PRIORITY")? {
            self.layers.priority = values;
        }
        if let Some(value) = env_bool("MS_LAYERS_AUTO_DETECT") {
            self.layers.auto_detect = value;
        }
        if let Some(value) = env_bool("MS_LAYERS_PROJECT_OVERRIDES") {
            self.layers.project_overrides = value;
        }

        if let Some(value) = env_string("MS_DISCLOSURE_DEFAULT_LEVEL") {
            self.disclosure.default_level = value;
        }
        if let Some(value) = env_u32("MS_DISCLOSURE_TOKEN_BUDGET")? {
            self.disclosure.token_budget = value;
        }
        if let Some(value) = env_bool("MS_DISCLOSURE_AUTO_SUGGEST") {
            self.disclosure.auto_suggest = value;
        }
        if let Some(value) = env_u64("MS_DISCLOSURE_COOLDOWN_SECONDS")? {
            self.disclosure.cooldown_seconds = value;
        }

        if let Some(value) = env_bool("MS_SEARCH_USE_EMBEDDINGS") {
            self.search.use_embeddings = value;
        }
        if let Some(value) = env_string("MS_SEARCH_EMBEDDING_BACKEND") {
            self.search.embedding_backend = value;
        }
        if let Some(value) = env_u32("MS_SEARCH_EMBEDDING_DIMS")? {
            self.search.embedding_dims = value;
        }
        if let Some(value) = env_f32("MS_SEARCH_BM25_WEIGHT")? {
            self.search.bm25_weight = value;
        }
        if let Some(value) = env_f32("MS_SEARCH_SEMANTIC_WEIGHT")? {
            self.search.semantic_weight = value;
        }

        if let Some(value) = env_bool("MS_CASS_AUTO_DETECT") {
            self.cass.auto_detect = value;
        }
        if let Some(value) = env_string("MS_CASS_PATH") {
            self.cass.cass_path = Some(value);
        }
        if let Some(value) = env_string("MS_CASS_SESSION_PATTERN") {
            self.cass.session_pattern = value;
        }

        if let Some(value) = env_bool("MS_CACHE_ENABLED") {
            self.cache.enabled = value;
        }
        if let Some(value) = env_u32("MS_CACHE_MAX_SIZE_MB")? {
            self.cache.max_size_mb = value;
        }
        if let Some(value) = env_u64("MS_CACHE_TTL_SECONDS")? {
            self.cache.ttl_seconds = value;
        }

        if let Some(value) = env_bool("MS_UPDATE_AUTO_CHECK") {
            self.update.auto_check = value;
        }
        if let Some(value) = env_u32("MS_UPDATE_CHECK_INTERVAL_HOURS")? {
            self.update.check_interval_hours = value;
        }
        if let Some(value) = env_string("MS_UPDATE_CHANNEL") {
            self.update.channel = value;
        }

        if let Some(value) = env_string("MS_ROBOT_FORMAT") {
            self.robot.format = value;
        }
        if let Some(value) = env_bool("MS_ROBOT_INCLUDE_METADATA") {
            self.robot.include_metadata = value;
        }

        if let Some(value) = env_bool("MS_SECURITY_ACIP_ENABLED") {
            self.security.acip.enabled = value;
        }
        if let Some(value) = env_string("MS_SECURITY_ACIP_VERSION") {
            self.security.acip.version = value;
        }
        if let Some(value) = env_string("MS_SECURITY_ACIP_PROMPT_PATH") {
            self.security.acip.prompt_path = PathBuf::from(value);
        }
        if let Some(value) = env_bool("MS_SECURITY_ACIP_AUDIT_MODE") {
            self.security.acip.audit_mode = value;
        }
        if let Some(value) = env_string("MS_SECURITY_ACIP_TRUST_USER_MESSAGES") {
            self.security.acip.trust.user_messages = parse_trust_level(&value)?;
        }
        if let Some(value) = env_string("MS_SECURITY_ACIP_TRUST_ASSISTANT_MESSAGES") {
            self.security.acip.trust.assistant_messages = parse_trust_level(&value)?;
        }
        if let Some(value) = env_string("MS_SECURITY_ACIP_TRUST_TOOL_OUTPUTS") {
            self.security.acip.trust.tool_outputs = parse_trust_level(&value)?;
        }
        if let Some(value) = env_string("MS_SECURITY_ACIP_TRUST_FILE_CONTENTS") {
            self.security.acip.trust.file_contents = parse_trust_level(&value)?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillPathsConfig {
    #[serde(default)]
    pub global: Vec<String>,
    #[serde(default)]
    pub project: Vec<String>,
    #[serde(default)]
    pub community: Vec<String>,
    #[serde(default)]
    pub local: Vec<String>,
}

impl Default for SkillPathsConfig {
    fn default() -> Self {
        Self {
            global: vec!["~/.local/share/ms/skills".to_string()],
            project: vec![".ms/skills".to_string()],
            community: vec!["~/.local/share/ms/community".to_string()],
            local: Vec::new(),
        }
    }
}

impl SkillPathsConfig {
    fn merge(&mut self, patch: SkillPathsPatch) {
        if let Some(values) = patch.global {
            self.global = merge_unique(values, &self.global);
        }
        if let Some(values) = patch.project {
            self.project = merge_unique(values, &self.project);
        }
        if let Some(values) = patch.community {
            self.community = merge_unique(values, &self.community);
        }
        if let Some(values) = patch.local {
            self.local = merge_unique(values, &self.local);
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayersConfig {
    #[serde(default)]
    pub priority: Vec<String>,
    #[serde(default)]
    pub auto_detect: bool,
    #[serde(default)]
    pub project_overrides: bool,
}

impl Default for LayersConfig {
    fn default() -> Self {
        Self {
            priority: vec![
                "project".to_string(),
                "global".to_string(),
                "community".to_string(),
            ],
            auto_detect: true,
            project_overrides: true,
        }
    }
}

impl LayersConfig {
    fn merge(&mut self, patch: LayersPatch) {
        if let Some(values) = patch.priority {
            self.priority = values;
        }
        if let Some(value) = patch.auto_detect {
            self.auto_detect = value;
        }
        if let Some(value) = patch.project_overrides {
            self.project_overrides = value;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisclosureConfig {
    #[serde(default)]
    pub default_level: String,
    #[serde(default)]
    pub token_budget: u32,
    #[serde(default)]
    pub auto_suggest: bool,
    #[serde(default)]
    pub cooldown_seconds: u64,
}

impl Default for DisclosureConfig {
    fn default() -> Self {
        Self {
            default_level: "moderate".to_string(),
            token_budget: 800,
            auto_suggest: true,
            cooldown_seconds: 300,
        }
    }
}

impl DisclosureConfig {
    fn merge(&mut self, patch: DisclosurePatch) {
        if let Some(value) = patch.default_level {
            self.default_level = value;
        }
        if let Some(value) = patch.token_budget {
            self.token_budget = value;
        }
        if let Some(value) = patch.auto_suggest {
            self.auto_suggest = value;
        }
        if let Some(value) = patch.cooldown_seconds {
            self.cooldown_seconds = value;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchConfig {
    #[serde(default)]
    pub use_embeddings: bool,
    #[serde(default)]
    pub embedding_backend: String,
    #[serde(default)]
    pub embedding_dims: u32,
    #[serde(default)]
    pub bm25_weight: f32,
    #[serde(default)]
    pub semantic_weight: f32,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            use_embeddings: true,
            embedding_backend: "hash".to_string(),
            embedding_dims: 384,
            bm25_weight: 0.5,
            semantic_weight: 0.5,
        }
    }
}

impl SearchConfig {
    fn merge(&mut self, patch: SearchPatch) {
        if let Some(value) = patch.use_embeddings {
            self.use_embeddings = value;
        }
        if let Some(value) = patch.embedding_backend {
            self.embedding_backend = value;
        }
        if let Some(value) = patch.embedding_dims {
            self.embedding_dims = value;
        }
        if let Some(value) = patch.bm25_weight {
            self.bm25_weight = value;
        }
        if let Some(value) = patch.semantic_weight {
            self.semantic_weight = value;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CassConfig {
    #[serde(default)]
    pub auto_detect: bool,
    #[serde(default)]
    pub cass_path: Option<String>,
    #[serde(default)]
    pub session_pattern: String,
}

impl Default for CassConfig {
    fn default() -> Self {
        Self {
            auto_detect: true,
            cass_path: None,
            session_pattern: "*.jsonl".to_string(),
        }
    }
}

impl CassConfig {
    fn merge(&mut self, patch: CassPatch) {
        if let Some(value) = patch.auto_detect {
            self.auto_detect = value;
        }
        if let Some(value) = patch.cass_path {
            self.cass_path = Some(value);
        }
        if let Some(value) = patch.session_pattern {
            self.session_pattern = value;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub max_size_mb: u32,
    #[serde(default)]
    pub ttl_seconds: u64,
}

impl Default for CacheConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_size_mb: 100,
            ttl_seconds: 3600,
        }
    }
}

impl CacheConfig {
    fn merge(&mut self, patch: CachePatch) {
        if let Some(value) = patch.enabled {
            self.enabled = value;
        }
        if let Some(value) = patch.max_size_mb {
            self.max_size_mb = value;
        }
        if let Some(value) = patch.ttl_seconds {
            self.ttl_seconds = value;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateConfig {
    #[serde(default)]
    pub auto_check: bool,
    #[serde(default)]
    pub check_interval_hours: u32,
    #[serde(default)]
    pub channel: String,
}

impl Default for UpdateConfig {
    fn default() -> Self {
        Self {
            auto_check: true,
            check_interval_hours: 24,
            channel: "stable".to_string(),
        }
    }
}

impl UpdateConfig {
    fn merge(&mut self, patch: UpdatePatch) {
        if let Some(value) = patch.auto_check {
            self.auto_check = value;
        }
        if let Some(value) = patch.check_interval_hours {
            self.check_interval_hours = value;
        }
        if let Some(value) = patch.channel {
            self.channel = value;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RobotConfig {
    #[serde(default)]
    pub format: String,
    #[serde(default)]
    pub include_metadata: bool,
}

impl Default for RobotConfig {
    fn default() -> Self {
        Self {
            format: "json".to_string(),
            include_metadata: true,
        }
    }
}

impl RobotConfig {
    fn merge(&mut self, patch: RobotPatch) {
        if let Some(value) = patch.format {
            self.format = value;
        }
        if let Some(value) = patch.include_metadata {
            self.include_metadata = value;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(default)]
    pub acip: AcipConfig,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            acip: AcipConfig::default(),
        }
    }
}

impl SecurityConfig {
    fn merge(&mut self, patch: SecurityPatch) {
        if let Some(patch) = patch.acip {
            self.acip.merge(patch);
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ConfigPatch {
    pub skill_paths: Option<SkillPathsPatch>,
    pub layers: Option<LayersPatch>,
    pub disclosure: Option<DisclosurePatch>,
    pub search: Option<SearchPatch>,
    pub cass: Option<CassPatch>,
    pub cache: Option<CachePatch>,
    pub update: Option<UpdatePatch>,
    pub robot: Option<RobotPatch>,
    pub security: Option<SecurityPatch>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct SkillPathsPatch {
    pub global: Option<Vec<String>>,
    pub project: Option<Vec<String>>,
    pub community: Option<Vec<String>>,
    pub local: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct LayersPatch {
    pub priority: Option<Vec<String>>,
    pub auto_detect: Option<bool>,
    pub project_overrides: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct DisclosurePatch {
    pub default_level: Option<String>,
    pub token_budget: Option<u32>,
    pub auto_suggest: Option<bool>,
    pub cooldown_seconds: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct SearchPatch {
    pub use_embeddings: Option<bool>,
    pub embedding_backend: Option<String>,
    pub embedding_dims: Option<u32>,
    pub bm25_weight: Option<f32>,
    pub semantic_weight: Option<f32>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct CassPatch {
    pub auto_detect: Option<bool>,
    pub cass_path: Option<String>,
    pub session_pattern: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct CachePatch {
    pub enabled: Option<bool>,
    pub max_size_mb: Option<u32>,
    pub ttl_seconds: Option<u64>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct UpdatePatch {
    pub auto_check: Option<bool>,
    pub check_interval_hours: Option<u32>,
    pub channel: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct RobotPatch {
    pub format: Option<String>,
    pub include_metadata: Option<bool>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct SecurityPatch {
    pub acip: Option<AcipPatch>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct AcipPatch {
    pub enabled: Option<bool>,
    pub version: Option<String>,
    pub prompt_path: Option<PathBuf>,
    pub audit_mode: Option<bool>,
    pub trust: Option<TrustBoundaryPatch>,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct TrustBoundaryPatch {
    pub user_messages: Option<TrustLevel>,
    pub assistant_messages: Option<TrustLevel>,
    pub tool_outputs: Option<TrustLevel>,
    pub file_contents: Option<TrustLevel>,
}

impl AcipConfig {
    fn merge(&mut self, patch: AcipPatch) {
        if let Some(value) = patch.enabled {
            self.enabled = value;
        }
        if let Some(value) = patch.version {
            self.version = value;
        }
        if let Some(value) = patch.prompt_path {
            self.prompt_path = value;
        }
        if let Some(value) = patch.audit_mode {
            self.audit_mode = value;
        }
        if let Some(patch) = patch.trust {
            if let Some(value) = patch.user_messages {
                self.trust.user_messages = value;
            }
            if let Some(value) = patch.assistant_messages {
                self.trust.assistant_messages = value;
            }
            if let Some(value) = patch.tool_outputs {
                self.trust.tool_outputs = value;
            }
            if let Some(value) = patch.file_contents {
                self.trust.file_contents = value;
            }
        }
    }
}

fn merge_unique(values: Vec<String>, existing: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for value in values.into_iter().chain(existing.iter().cloned()) {
        if seen.insert(value.clone()) {
            out.push(value);
        }
    }
    out
}

fn parse_trust_level(value: &str) -> Result<TrustLevel> {
    match value.to_lowercase().as_str() {
        "trusted" => Ok(TrustLevel::Trusted),
        "verify_required" | "verifyrequired" | "verify-required" => Ok(TrustLevel::VerifyRequired),
        "untrusted" => Ok(TrustLevel::Untrusted),
        _ => Err(MsError::Config(format!(
            "invalid trust level {value} (expected trusted|verify_required|untrusted)"
        ))),
    }
}

fn env_string(key: &str) -> Option<String> {
    std::env::var(key).ok()
}

fn env_bool(key: &str) -> Option<bool> {
    std::env::var(key).ok().map(|value| {
        matches!(
            value.to_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

fn env_u32(key: &str) -> Result<Option<u32>> {
    match std::env::var(key) {
        Ok(value) => value.parse::<u32>().map(Some).map_err(|err| {
            MsError::Config(format!("invalid {key} value {value}: {err}"))
        }),
        Err(_) => Ok(None),
    }
}

fn env_u64(key: &str) -> Result<Option<u64>> {
    match std::env::var(key) {
        Ok(value) => value.parse::<u64>().map(Some).map_err(|err| {
            MsError::Config(format!("invalid {key} value {value}: {err}"))
        }),
        Err(_) => Ok(None),
    }
}

fn env_f32(key: &str) -> Result<Option<f32>> {
    match std::env::var(key) {
        Ok(value) => value.parse::<f32>().map(Some).map_err(|err| {
            MsError::Config(format!("invalid {key} value {value}: {err}"))
        }),
        Err(_) => Ok(None),
    }
}

fn env_list(key: &str) -> Result<Option<Vec<String>>> {
    match std::env::var(key) {
        Ok(value) => {
            let list = value
                .split(',')
                .map(|entry| entry.trim())
                .filter(|entry| !entry.is_empty())
                .map(|entry| entry.to_string())
                .collect::<Vec<_>>();
            Ok(Some(list))
        }
        Err(_) => Ok(None),
    }
}
