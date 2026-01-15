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
    pub cm: CmConfig,
    #[serde(default)]
    pub cache: CacheConfig,
    #[serde(default)]
    pub update: UpdateConfig,
    #[serde(default)]
    pub robot: RobotConfig,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub safety: SafetyConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            skill_paths: SkillPathsConfig::default(),
            layers: LayersConfig::default(),
            disclosure: DisclosureConfig::default(),
            search: SearchConfig::default(),
            cass: CassConfig::default(),
            cm: CmConfig::default(),
            cache: CacheConfig::default(),
            update: UpdateConfig::default(),
            robot: RobotConfig::default(),
            security: SecurityConfig::default(),
            safety: SafetyConfig::default(),
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
        if let Some(patch) = patch.cm {
            self.cm.merge(patch);
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
        if let Some(patch) = patch.safety {
            self.safety.merge(patch);
        }
    }

    fn apply_env_overrides(&mut self) -> Result<()> {
        if env_bool("MS_ROBOT")?.unwrap_or(false) {
            self.robot.format = "json".to_string();
            self.robot.include_metadata = true;
        }
        if env_bool("MS_CACHE_DISABLED")?.unwrap_or(false) {
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
        if let Some(value) = env_bool("MS_LAYERS_AUTO_DETECT")? {
            self.layers.auto_detect = value;
        }
        if let Some(value) = env_bool("MS_LAYERS_PROJECT_OVERRIDES")? {
            self.layers.project_overrides = value;
        }

        if let Some(value) = env_string("MS_DISCLOSURE_DEFAULT_LEVEL") {
            self.disclosure.default_level = value;
        }
        if let Some(value) = env_u32("MS_DISCLOSURE_TOKEN_BUDGET")? {
            self.disclosure.token_budget = value;
        }
        if let Some(value) = env_bool("MS_DISCLOSURE_AUTO_SUGGEST")? {
            self.disclosure.auto_suggest = value;
        }
        if let Some(value) = env_u64("MS_DISCLOSURE_COOLDOWN_SECONDS")? {
            self.disclosure.cooldown_seconds = value;
        }

        if let Some(value) = env_bool("MS_SEARCH_USE_EMBEDDINGS")? {
            self.search.use_embeddings = value;
        }
        if let Some(value) = env_string("MS_SEARCH_EMBEDDING_BACKEND") {
            self.search.embedding_backend = value;
        }
        if let Some(value) = env_u32("MS_SEARCH_EMBEDDING_DIMS")? {
            self.search.embedding_dims = value;
        }
        if let Some(value) = env_f32("MS_SEARCH_BM25_WEIGHT")? {
            validate_weight("MS_SEARCH_BM25_WEIGHT", value)?;
            self.search.bm25_weight = value;
        }
        if let Some(value) = env_f32("MS_SEARCH_SEMANTIC_WEIGHT")? {
            validate_weight("MS_SEARCH_SEMANTIC_WEIGHT", value)?;
            self.search.semantic_weight = value;
        }

        if let Some(value) = env_bool("MS_CASS_AUTO_DETECT")? {
            self.cass.auto_detect = value;
        }
        if let Some(value) = env_string("MS_CASS_PATH") {
            self.cass.cass_path = Some(value);
        }
        if let Some(value) = env_string("MS_CASS_SESSION_PATTERN") {
            self.cass.session_pattern = value;
        }
        if let Some(value) = env_bool("MS_CM_ENABLED")? {
            self.cm.enabled = value;
        }
        if let Some(value) = env_string("MS_CM_PATH") {
            self.cm.cm_path = Some(value);
        }
        if let Some(values) = env_list("MS_CM_DEFAULT_FLAGS")? {
            self.cm.default_flags = values;
        }

        if let Some(value) = env_bool("MS_CACHE_ENABLED")? {
            self.cache.enabled = value;
        }
        if let Some(value) = env_u32("MS_CACHE_MAX_SIZE_MB")? {
            self.cache.max_size_mb = value;
        }
        if let Some(value) = env_u64("MS_CACHE_TTL_SECONDS")? {
            self.cache.ttl_seconds = value;
        }

        if let Some(value) = env_bool("MS_UPDATE_AUTO_CHECK")? {
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
        if let Some(value) = env_bool("MS_ROBOT_INCLUDE_METADATA")? {
            self.robot.include_metadata = value;
        }

        if let Some(value) = env_bool("MS_SECURITY_ACIP_ENABLED")? {
            self.security.acip.enabled = value;
        }
        if let Some(value) = env_string("MS_SECURITY_ACIP_VERSION") {
            self.security.acip.version = value;
        }
        if let Some(value) = env_string("MS_SECURITY_ACIP_PROMPT_PATH") {
            self.security.acip.prompt_path = PathBuf::from(value);
        }
        if let Some(value) = env_bool("MS_SECURITY_ACIP_AUDIT_MODE")? {
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
        if let Some(value) = env_string("MS_SAFETY_DCG_BIN") {
            self.safety.dcg_bin = PathBuf::from(value);
        }
        if let Some(values) = env_list("MS_SAFETY_DCG_PACKS")? {
            self.safety.dcg_packs = values;
        }
        if let Some(value) = env_string("MS_SAFETY_DCG_EXPLAIN_FORMAT") {
            self.safety.dcg_explain_format = value;
        }
        if let Some(value) = env_bool("MS_SAFETY_REQUIRE_VERBATIM_APPROVAL")? {
            self.safety.require_verbatim_approval = value;
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
pub struct CmConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub cm_path: Option<String>,
    #[serde(default)]
    pub default_flags: Vec<String>,
}

impl Default for CmConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            cm_path: None,
            default_flags: Vec::new(),
        }
    }
}

impl CmConfig {
    fn merge(&mut self, patch: CmPatch) {
        if let Some(value) = patch.enabled {
            self.enabled = value;
        }
        if let Some(value) = patch.cm_path {
            self.cm_path = Some(value);
        }
        if let Some(values) = patch.default_flags {
            self.default_flags = values;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafetyConfig {
    #[serde(default)]
    pub dcg_bin: PathBuf,
    #[serde(default)]
    pub dcg_packs: Vec<String>,
    #[serde(default)]
    pub dcg_explain_format: String,
    #[serde(default)]
    pub require_verbatim_approval: bool,
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            dcg_bin: PathBuf::from("dcg"),
            dcg_packs: Vec::new(),
            dcg_explain_format: "json".to_string(),
            require_verbatim_approval: true,
        }
    }
}

impl SafetyConfig {
    fn merge(&mut self, patch: SafetyPatch) {
        if let Some(value) = patch.dcg_bin {
            self.dcg_bin = value;
        }
        if let Some(values) = patch.dcg_packs {
            self.dcg_packs = values;
        }
        if let Some(value) = patch.dcg_explain_format {
            self.dcg_explain_format = value;
        }
        if let Some(value) = patch.require_verbatim_approval {
            self.require_verbatim_approval = value;
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
    pub cm: Option<CmPatch>,
    pub cache: Option<CachePatch>,
    pub update: Option<UpdatePatch>,
    pub robot: Option<RobotPatch>,
    pub security: Option<SecurityPatch>,
    pub safety: Option<SafetyPatch>,
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
struct CmPatch {
    pub enabled: Option<bool>,
    pub cm_path: Option<String>,
    pub default_flags: Option<Vec<String>>,
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
struct SafetyPatch {
    pub dcg_bin: Option<PathBuf>,
    pub dcg_packs: Option<Vec<String>>,
    pub dcg_explain_format: Option<String>,
    pub require_verbatim_approval: Option<bool>,
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

fn env_bool(key: &str) -> Result<Option<bool>> {
    match std::env::var(key) {
        Ok(value) => match value.to_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => Ok(Some(true)),
            "0" | "false" | "no" | "off" | "" => Ok(Some(false)),
            _ => Err(MsError::Config(format!(
                "invalid boolean value for {key}: {value} (expected true|false|1|0|yes|no|on|off)"
            ))),
        },
        Err(_) => Ok(None),
    }
}

fn env_u32(key: &str) -> Result<Option<u32>> {
    match std::env::var(key) {
        Ok(value) => value
            .parse::<u32>()
            .map(Some)
            .map_err(|err| MsError::Config(format!("invalid {key} value {value}: {err}"))),
        Err(_) => Ok(None),
    }
}

fn env_u64(key: &str) -> Result<Option<u64>> {
    match std::env::var(key) {
        Ok(value) => value
            .parse::<u64>()
            .map(Some)
            .map_err(|err| MsError::Config(format!("invalid {key} value {value}: {err}"))),
        Err(_) => Ok(None),
    }
}

fn env_f32(key: &str) -> Result<Option<f32>> {
    match std::env::var(key) {
        Ok(value) => value
            .parse::<f32>()
            .map(Some)
            .map_err(|err| MsError::Config(format!("invalid {key} value {value}: {err}"))),
        Err(_) => Ok(None),
    }
}

fn validate_weight(name: &str, value: f32) -> Result<()> {
    if value.is_nan() {
        return Err(MsError::Config(format!("{name} cannot be NaN")));
    }
    if !(0.0..=1.0).contains(&value) {
        return Err(MsError::Config(format!(
            "{name} must be between 0.0 and 1.0, got {value}"
        )));
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    // =========================================================================
    // Config default tests
    // =========================================================================

    #[test]
    fn config_default_has_all_fields() {
        let config = Config::default();
        // Verify all sections have sensible defaults
        assert!(!config.skill_paths.global.is_empty());
        assert!(!config.layers.priority.is_empty());
        assert!(!config.disclosure.default_level.is_empty());
        assert!(config.search.embedding_dims > 0);
        assert!(config.cache.max_size_mb > 0);
        assert!(config.update.check_interval_hours > 0);
    }

    #[test]
    fn config_serialization_roundtrip() {
        let config = Config::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(
            config.disclosure.default_level,
            deserialized.disclosure.default_level
        );
        assert_eq!(config.cache.enabled, deserialized.cache.enabled);
    }

    // =========================================================================
    // SkillPathsConfig tests
    // =========================================================================

    #[test]
    fn skill_paths_config_defaults() {
        let config = SkillPathsConfig::default();
        assert_eq!(config.global.len(), 1);
        assert!(config.global[0].contains("ms/skills"));
        assert_eq!(config.project.len(), 1);
        assert!(config.project[0].contains(".ms/skills"));
        assert_eq!(config.community.len(), 1);
        assert!(config.local.is_empty());
    }

    #[test]
    fn skill_paths_config_serialization() {
        let config = SkillPathsConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("global"));
        assert!(json.contains("project"));
    }

    // =========================================================================
    // LayersConfig tests
    // =========================================================================

    #[test]
    fn layers_config_defaults() {
        let config = LayersConfig::default();
        assert_eq!(config.priority.len(), 3);
        assert_eq!(config.priority[0], "project");
        assert_eq!(config.priority[1], "global");
        assert_eq!(config.priority[2], "community");
        assert!(config.auto_detect);
        assert!(config.project_overrides);
    }

    #[test]
    fn layers_config_serialization() {
        let config = LayersConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"auto_detect\":true"));
    }

    // =========================================================================
    // DisclosureConfig tests
    // =========================================================================

    #[test]
    fn disclosure_config_defaults() {
        let config = DisclosureConfig::default();
        assert_eq!(config.default_level, "moderate");
        assert_eq!(config.token_budget, 800);
        assert!(config.auto_suggest);
        assert_eq!(config.cooldown_seconds, 300);
    }

    #[test]
    fn disclosure_config_serialization() {
        let config = DisclosureConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"token_budget\":800"));
    }

    // =========================================================================
    // SearchConfig tests
    // =========================================================================

    #[test]
    fn search_config_defaults() {
        let config = SearchConfig::default();
        assert!(config.use_embeddings);
        assert_eq!(config.embedding_backend, "hash");
        assert_eq!(config.embedding_dims, 384);
        assert!((config.bm25_weight - 0.5).abs() < f32::EPSILON);
        assert!((config.semantic_weight - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn search_config_serialization() {
        let config = SearchConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"embedding_dims\":384"));
    }

    // =========================================================================
    // CassConfig tests
    // =========================================================================

    #[test]
    fn cass_config_defaults() {
        let config = CassConfig::default();
        assert!(config.auto_detect);
        assert!(config.cass_path.is_none());
        assert_eq!(config.session_pattern, "*.jsonl");
    }

    #[test]
    fn cass_config_serialization() {
        let config = CassConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"session_pattern\":\"*.jsonl\""));
    }

    // =========================================================================
    // CmConfig tests
    // =========================================================================

    #[test]
    fn cm_config_defaults() {
        let config = CmConfig::default();
        assert!(config.enabled);
        assert!(config.cm_path.is_none());
        assert!(config.default_flags.is_empty());
    }

    #[test]
    fn cm_config_serialization() {
        let config = CmConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"enabled\":true"));
    }

    // =========================================================================
    // CacheConfig tests
    // =========================================================================

    #[test]
    fn cache_config_defaults() {
        let config = CacheConfig::default();
        assert!(config.enabled);
        assert_eq!(config.max_size_mb, 100);
        assert_eq!(config.ttl_seconds, 3600);
    }

    #[test]
    fn cache_config_serialization() {
        let config = CacheConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"max_size_mb\":100"));
    }

    // =========================================================================
    // UpdateConfig tests
    // =========================================================================

    #[test]
    fn update_config_defaults() {
        let config = UpdateConfig::default();
        assert!(config.auto_check);
        assert_eq!(config.check_interval_hours, 24);
        assert_eq!(config.channel, "stable");
    }

    #[test]
    fn update_config_serialization() {
        let config = UpdateConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"channel\":\"stable\""));
    }

    // =========================================================================
    // RobotConfig tests
    // =========================================================================

    #[test]
    fn robot_config_defaults() {
        let config = RobotConfig::default();
        assert_eq!(config.format, "json");
        assert!(config.include_metadata);
    }

    #[test]
    fn robot_config_serialization() {
        let config = RobotConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"format\":\"json\""));
    }

    // =========================================================================
    // SafetyConfig tests
    // =========================================================================

    #[test]
    fn safety_config_defaults() {
        let config = SafetyConfig::default();
        assert_eq!(config.dcg_bin, PathBuf::from("dcg"));
        assert!(config.dcg_packs.is_empty());
        assert_eq!(config.dcg_explain_format, "json");
        assert!(config.require_verbatim_approval);
    }

    #[test]
    fn safety_config_serialization() {
        let config = SafetyConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"dcg_explain_format\":\"json\""));
    }

    // =========================================================================
    // merge_unique tests
    // =========================================================================

    #[test]
    fn merge_unique_combines_lists() {
        let new = vec!["a".to_string(), "b".to_string()];
        let existing = vec!["c".to_string(), "d".to_string()];
        let result = merge_unique(new, &existing);
        assert_eq!(result.len(), 4);
        assert_eq!(result, vec!["a", "b", "c", "d"]);
    }

    #[test]
    fn merge_unique_removes_duplicates() {
        let new = vec!["a".to_string(), "b".to_string()];
        let existing = vec!["b".to_string(), "c".to_string()];
        let result = merge_unique(new, &existing);
        assert_eq!(result.len(), 3);
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    #[test]
    fn merge_unique_prefers_new_order() {
        let new = vec!["x".to_string(), "y".to_string()];
        let existing = vec!["y".to_string(), "z".to_string()];
        let result = merge_unique(new, &existing);
        // New values come first
        assert_eq!(result[0], "x");
        assert_eq!(result[1], "y");
        assert_eq!(result[2], "z");
    }

    #[test]
    fn merge_unique_empty_new() {
        let new = vec![];
        let existing = vec!["a".to_string(), "b".to_string()];
        let result = merge_unique(new, &existing);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn merge_unique_empty_existing() {
        let new = vec!["a".to_string(), "b".to_string()];
        let existing = vec![];
        let result = merge_unique(new, &existing);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn merge_unique_both_empty() {
        let new: Vec<String> = vec![];
        let existing: Vec<String> = vec![];
        let result = merge_unique(new, &existing);
        assert!(result.is_empty());
    }

    // =========================================================================
    // parse_trust_level tests
    // =========================================================================

    #[test]
    fn parse_trust_level_trusted() {
        assert_eq!(parse_trust_level("trusted").unwrap(), TrustLevel::Trusted);
        assert_eq!(parse_trust_level("TRUSTED").unwrap(), TrustLevel::Trusted);
        assert_eq!(parse_trust_level("Trusted").unwrap(), TrustLevel::Trusted);
    }

    #[test]
    fn parse_trust_level_untrusted() {
        assert_eq!(
            parse_trust_level("untrusted").unwrap(),
            TrustLevel::Untrusted
        );
        assert_eq!(
            parse_trust_level("UNTRUSTED").unwrap(),
            TrustLevel::Untrusted
        );
    }

    #[test]
    fn parse_trust_level_verify_required_variants() {
        assert_eq!(
            parse_trust_level("verify_required").unwrap(),
            TrustLevel::VerifyRequired
        );
        assert_eq!(
            parse_trust_level("verifyrequired").unwrap(),
            TrustLevel::VerifyRequired
        );
        assert_eq!(
            parse_trust_level("verify-required").unwrap(),
            TrustLevel::VerifyRequired
        );
        assert_eq!(
            parse_trust_level("VERIFY_REQUIRED").unwrap(),
            TrustLevel::VerifyRequired
        );
    }

    #[test]
    fn parse_trust_level_invalid() {
        let result = parse_trust_level("invalid");
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("invalid trust level"));
    }

    // =========================================================================
    // Config::load_patch tests (file-based)
    // =========================================================================

    #[test]
    fn load_patch_nonexistent_file() {
        let result = Config::load_patch(Path::new("/nonexistent/path/config.toml")).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn load_patch_valid_toml() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[cache]
enabled = false
max_size_mb = 200
"#,
        )
        .unwrap();

        let patch = Config::load_patch(&path).unwrap().unwrap();
        assert!(patch.cache.is_some());
        let cache_patch = patch.cache.unwrap();
        assert_eq!(cache_patch.enabled, Some(false));
        assert_eq!(cache_patch.max_size_mb, Some(200));
    }

    #[test]
    fn load_patch_partial_config() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("config.toml");
        std::fs::write(
            &path,
            r#"
[disclosure]
token_budget = 1000
"#,
        )
        .unwrap();

        let patch = Config::load_patch(&path).unwrap().unwrap();
        assert!(patch.disclosure.is_some());
        assert!(patch.cache.is_none());
        assert!(patch.search.is_none());
    }

    #[test]
    fn load_patch_invalid_toml() {
        let temp = TempDir::new().unwrap();
        let path = temp.path().join("config.toml");
        std::fs::write(&path, "this is not valid toml [[[").unwrap();

        let result = Config::load_patch(&path);
        assert!(result.is_err());
    }

    // =========================================================================
    // Config merge tests
    // =========================================================================

    #[test]
    fn config_merge_patch_updates_values() {
        let mut config = Config::default();
        assert!(config.cache.enabled);

        let patch = ConfigPatch {
            cache: Some(CachePatch {
                enabled: Some(false),
                max_size_mb: None,
                ttl_seconds: None,
            }),
            ..Default::default()
        };

        config.merge_patch(patch);
        assert!(!config.cache.enabled);
        // Other values unchanged
        assert_eq!(config.cache.max_size_mb, 100);
    }

    #[test]
    fn config_merge_patch_empty_noop() {
        let config_before = Config::default();
        let mut config = Config::default();

        let patch = ConfigPatch::default();
        config.merge_patch(patch);

        // Values unchanged
        assert_eq!(config.cache.enabled, config_before.cache.enabled);
        assert_eq!(
            config.disclosure.token_budget,
            config_before.disclosure.token_budget
        );
    }

    #[test]
    fn config_merge_multiple_sections() {
        let mut config = Config::default();

        let patch = ConfigPatch {
            cache: Some(CachePatch {
                enabled: Some(false),
                max_size_mb: Some(50),
                ttl_seconds: Some(7200),
            }),
            update: Some(UpdatePatch {
                auto_check: Some(false),
                check_interval_hours: Some(48),
                channel: Some("beta".to_string()),
            }),
            ..Default::default()
        };

        config.merge_patch(patch);

        assert!(!config.cache.enabled);
        assert_eq!(config.cache.max_size_mb, 50);
        assert_eq!(config.cache.ttl_seconds, 7200);
        assert!(!config.update.auto_check);
        assert_eq!(config.update.check_interval_hours, 48);
        assert_eq!(config.update.channel, "beta");
    }

    // =========================================================================
    // Config::load tests (integration)
    // =========================================================================

    #[test]
    fn config_load_from_explicit_path() {
        let temp = TempDir::new().unwrap();
        let config_path = temp.path().join("custom_config.toml");
        let ms_root = temp.path().join(".ms");
        std::fs::create_dir_all(&ms_root).unwrap();

        std::fs::write(
            &config_path,
            r#"
[cache]
enabled = false
"#,
        )
        .unwrap();

        let config = Config::load(Some(&config_path), &ms_root).unwrap();
        assert!(!config.cache.enabled);
    }

    #[test]
    fn config_load_project_config() {
        let temp = TempDir::new().unwrap();
        let ms_root = temp.path().join(".ms");
        std::fs::create_dir_all(&ms_root).unwrap();

        let project_config = ms_root.join("config.toml");
        std::fs::write(
            &project_config,
            r#"
[disclosure]
token_budget = 1500
"#,
        )
        .unwrap();

        let config = Config::load(None, &ms_root).unwrap();
        assert_eq!(config.disclosure.token_budget, 1500);
    }

    #[test]
    fn config_load_with_no_config_files() {
        let temp = TempDir::new().unwrap();
        let ms_root = temp.path().join(".ms");
        std::fs::create_dir_all(&ms_root).unwrap();

        let config = Config::load(None, &ms_root).unwrap();
        // Should get defaults
        assert_eq!(config.disclosure.token_budget, 800);
        assert!(config.cache.enabled);
    }
}
