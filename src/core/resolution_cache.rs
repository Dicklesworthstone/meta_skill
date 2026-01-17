//! Resolution caching for resolved skills
//!
//! Provides a two-level cache (in-memory LRU + SQLite backing) for resolved skills
//! to avoid repeated resolution of inheritance and composition chains.

use std::collections::{HashMap, HashSet, VecDeque};

use parking_lot::RwLock;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::core::resolution::{ResolutionWarning, ResolvedSkillSpec};
use crate::core::skill::SkillSpec;
use crate::error::{MsError, Result};

/// Maximum entries in the in-memory LRU cache
const DEFAULT_CACHE_CAPACITY: usize = 256;

/// Cached resolved skill entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedResolvedSkill {
    /// The resolved skill spec
    pub spec: SkillSpec,
    /// Chain of skill IDs from root to this skill
    pub inheritance_chain: Vec<String>,
    /// Skill IDs that were included into this skill
    pub included_from: Vec<String>,
    /// Warnings encountered during resolution
    pub warnings: Vec<ResolutionWarning>,
    /// Content hash of the cached entry
    pub cache_key_hash: String,
    /// Hashes of all dependency skills at cache time
    pub dependency_hashes: HashMap<String, String>,
}

impl CachedResolvedSkill {
    /// Convert to a ResolvedSkillSpec
    pub fn to_resolved_spec(&self) -> ResolvedSkillSpec {
        ResolvedSkillSpec {
            spec: self.spec.clone(),
            inheritance_chain: self.inheritance_chain.clone(),
            included_from: self.included_from.clone(),
            warnings: self.warnings.clone(),
        }
    }
}

/// Key for cache entries
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey {
    /// The skill ID
    pub skill_id: String,
    /// Hash of all dependencies' content hashes
    pub dependency_hash: String,
}

impl CacheKey {
    /// Create a new cache key from skill ID and dependency content hashes
    pub fn new(skill_id: &str, dependency_hashes: &HashMap<String, String>) -> Self {
        // Sort dependency hashes for deterministic ordering
        let mut sorted_deps: Vec<_> = dependency_hashes.iter().collect();
        sorted_deps.sort_by_key(|(k, _)| *k);

        let mut hasher = Sha256::new();
        hasher.update(skill_id.as_bytes());
        for (dep_id, dep_hash) in sorted_deps {
            hasher.update(dep_id.as_bytes());
            hasher.update(dep_hash.as_bytes());
        }

        Self {
            skill_id: skill_id.to_string(),
            dependency_hash: hex::encode(hasher.finalize()),
        }
    }

    /// Get the combined hash as a string
    pub fn hash(&self) -> String {
        format!("{}:{}", self.skill_id, self.dependency_hash)
    }
}

/// In-memory LRU cache for resolved skills
struct MemoryCache {
    /// Map from skill_id to cached entry
    entries: HashMap<String, CachedResolvedSkill>,
    /// Access order for LRU eviction (front = oldest)
    order: VecDeque<String>,
    /// Maximum capacity
    capacity: usize,
}

impl MemoryCache {
    fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::with_capacity(capacity),
            order: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    fn get(&mut self, skill_id: &str) -> Option<&CachedResolvedSkill> {
        if self.entries.contains_key(skill_id) {
            // Move to back of order (most recently used)
            self.order.retain(|id| id != skill_id);
            self.order.push_back(skill_id.to_string());
            self.entries.get(skill_id)
        } else {
            None
        }
    }

    fn insert(&mut self, skill_id: String, entry: CachedResolvedSkill) {
        // Remove existing entry if present
        if self.entries.contains_key(&skill_id) {
            self.order.retain(|id| id != &skill_id);
        }

        // Evict oldest entries if at capacity
        while self.entries.len() >= self.capacity {
            if let Some(oldest) = self.order.pop_front() {
                self.entries.remove(&oldest);
            } else {
                break;
            }
        }

        self.entries.insert(skill_id.clone(), entry);
        self.order.push_back(skill_id);
    }

    fn invalidate(&mut self, skill_id: &str) {
        self.entries.remove(skill_id);
        self.order.retain(|id| id != skill_id);
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }
}

/// Dependency graph for cache invalidation
#[derive(Debug, Clone, Default)]
pub struct DependencyGraph {
    /// Maps skill_id -> set of skills that depend on it
    dependents: HashMap<String, HashSet<String>>,
    /// Maps skill_id -> set of skills it depends on
    dependencies: HashMap<String, HashSet<String>>,
}

impl DependencyGraph {
    /// Create a new empty dependency graph
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a dependency relationship
    pub fn add_dependency(&mut self, skill_id: &str, depends_on: &str, _dep_type: &str) {
        self.dependents
            .entry(depends_on.to_string())
            .or_default()
            .insert(skill_id.to_string());
        self.dependencies
            .entry(skill_id.to_string())
            .or_default()
            .insert(depends_on.to_string());
    }

    /// Remove all dependencies for a skill
    pub fn remove_skill(&mut self, skill_id: &str) {
        // Remove from dependents of other skills
        if let Some(deps) = self.dependencies.remove(skill_id) {
            for dep in deps {
                if let Some(dependents) = self.dependents.get_mut(&dep) {
                    dependents.remove(skill_id);
                }
            }
        }

        // Remove skills that depend on this skill
        self.dependents.remove(skill_id);
    }

    /// Get all skills that would need invalidation if the given skill changes
    pub fn get_transitive_dependents(&self, skill_id: &str) -> HashSet<String> {
        let mut result = HashSet::new();
        let mut to_visit = vec![skill_id.to_string()];

        while let Some(current) = to_visit.pop() {
            if let Some(dependents) = self.dependents.get(&current) {
                for dependent in dependents {
                    if result.insert(dependent.clone()) {
                        to_visit.push(dependent.clone());
                    }
                }
            }
        }

        result
    }

    /// Load from SQLite
    pub fn load_from_db(conn: &Connection) -> Result<Self> {
        let mut graph = Self::new();

        let mut stmt = conn.prepare(
            "SELECT skill_id, depends_on, dependency_type FROM skill_dependency_graph",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;

        for row in rows {
            let (skill_id, depends_on, dep_type) = row?;
            graph.add_dependency(&skill_id, &depends_on, &dep_type);
        }

        Ok(graph)
    }

    /// Save to SQLite (replaces all data)
    pub fn save_to_db(&self, conn: &Connection) -> Result<()> {
        // Clear existing data
        conn.execute("DELETE FROM skill_dependency_graph", [])?;

        let mut stmt = conn.prepare(
            "INSERT INTO skill_dependency_graph (skill_id, depends_on, dependency_type) VALUES (?, ?, ?)",
        )?;

        for (skill_id, deps) in &self.dependencies {
            for dep in deps {
                // Determine type based on whether it's extends or includes
                // For simplicity, we'll store "dependency" - the exact type can be
                // derived from the skill spec if needed
                stmt.execute(params![skill_id, dep, "dependency"])?;
            }
        }

        Ok(())
    }

    /// Add dependency to SQLite
    pub fn add_dependency_to_db(
        conn: &Connection,
        skill_id: &str,
        depends_on: &str,
        dep_type: &str,
    ) -> Result<()> {
        conn.execute(
            "INSERT OR REPLACE INTO skill_dependency_graph (skill_id, depends_on, dependency_type) VALUES (?, ?, ?)",
            params![skill_id, depends_on, dep_type],
        )?;
        Ok(())
    }

    /// Remove skill dependencies from SQLite
    pub fn remove_skill_from_db(conn: &Connection, skill_id: &str) -> Result<()> {
        conn.execute(
            "DELETE FROM skill_dependency_graph WHERE skill_id = ?",
            params![skill_id],
        )?;
        Ok(())
    }
}

/// Resolution cache with in-memory LRU and SQLite backing
pub struct ResolutionCache {
    /// In-memory LRU cache
    memory: RwLock<MemoryCache>,
    /// Dependency graph for invalidation
    dependency_graph: RwLock<DependencyGraph>,
}

impl std::fmt::Debug for ResolutionCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolutionCache")
            .field("capacity", &DEFAULT_CACHE_CAPACITY)
            .finish_non_exhaustive()
    }
}

impl Default for ResolutionCache {
    fn default() -> Self {
        Self::new()
    }
}

impl ResolutionCache {
    /// Create a new resolution cache
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CACHE_CAPACITY)
    }

    /// Create a new resolution cache with the given capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            memory: RwLock::new(MemoryCache::new(capacity)),
            dependency_graph: RwLock::new(DependencyGraph::new()),
        }
    }

    /// Load cache state from SQLite
    pub fn load_from_db(&self, conn: &Connection) -> Result<()> {
        // Load dependency graph
        let graph = DependencyGraph::load_from_db(conn)?;
        *self.dependency_graph.write() = graph;

        // Load cached entries into memory
        let mut stmt = conn.prepare(
            "SELECT skill_id, resolved_json, cache_key_hash, inheritance_chain, \
             included_from, dependency_hashes, warnings_json FROM resolved_skill_cache",
        )?;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
                row.get::<_, String>(6)?,
            ))
        })?;

        let mut memory = self.memory.write();
        for row in rows {
            let (skill_id, resolved_json, cache_key_hash, chain_json, included_json, dep_hashes_json, warnings_json) = row?;

            let spec: SkillSpec = serde_json::from_str(&resolved_json)
                .map_err(|e| MsError::Serialization(format!("failed to parse cached spec: {e}")))?;
            let inheritance_chain: Vec<String> = serde_json::from_str(&chain_json)
                .map_err(|e| MsError::Serialization(format!("failed to parse inheritance chain: {e}")))?;
            let included_from: Vec<String> = serde_json::from_str(&included_json)
                .map_err(|e| MsError::Serialization(format!("failed to parse included_from: {e}")))?;
            let dependency_hashes: HashMap<String, String> = serde_json::from_str(&dep_hashes_json)
                .map_err(|e| MsError::Serialization(format!("failed to parse dependency hashes: {e}")))?;
            let warnings: Vec<ResolutionWarning> = serde_json::from_str(&warnings_json)
                .map_err(|e| MsError::Serialization(format!("failed to parse warnings: {e}")))?;

            let entry = CachedResolvedSkill {
                spec,
                inheritance_chain,
                included_from,
                warnings,
                cache_key_hash,
                dependency_hashes,
            };

            memory.insert(skill_id, entry);
        }

        Ok(())
    }

    /// Get a cached resolved skill if valid
    pub fn get(
        &self,
        skill_id: &str,
        current_dependency_hashes: &HashMap<String, String>,
    ) -> Option<CachedResolvedSkill> {
        let mut memory = self.memory.write();

        if let Some(entry) = memory.get(skill_id) {
            // Validate that dependency hashes match
            if entry.dependency_hashes == *current_dependency_hashes {
                return Some(entry.clone());
            }
        }

        None
    }

    /// Get from SQLite if not in memory
    pub fn get_from_db(
        &self,
        conn: &Connection,
        skill_id: &str,
        current_dependency_hashes: &HashMap<String, String>,
    ) -> Result<Option<CachedResolvedSkill>> {
        // Check memory first
        if let Some(entry) = self.get(skill_id, current_dependency_hashes) {
            return Ok(Some(entry));
        }

        // Check SQLite
        let mut stmt = conn.prepare(
            "SELECT resolved_json, cache_key_hash, inheritance_chain, \
             included_from, dependency_hashes, warnings_json FROM resolved_skill_cache WHERE skill_id = ?",
        )?;

        let result = stmt.query_row([skill_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
                row.get::<_, String>(5)?,
            ))
        });

        match result {
            Ok((resolved_json, cache_key_hash, chain_json, included_json, dep_hashes_json, warnings_json)) => {
                let spec: SkillSpec = serde_json::from_str(&resolved_json)
                    .map_err(|e| MsError::Serialization(format!("failed to parse cached spec: {e}")))?;
                let inheritance_chain: Vec<String> = serde_json::from_str(&chain_json)
                    .map_err(|e| MsError::Serialization(format!("failed to parse inheritance chain: {e}")))?;
                let included_from: Vec<String> = serde_json::from_str(&included_json)
                    .map_err(|e| MsError::Serialization(format!("failed to parse included_from: {e}")))?;
                let dependency_hashes: HashMap<String, String> = serde_json::from_str(&dep_hashes_json)
                    .map_err(|e| MsError::Serialization(format!("failed to parse dependency hashes: {e}")))?;
                let warnings: Vec<ResolutionWarning> = serde_json::from_str(&warnings_json)
                    .map_err(|e| MsError::Serialization(format!("failed to parse warnings: {e}")))?;

                // Validate dependency hashes
                if dependency_hashes != *current_dependency_hashes {
                    return Ok(None);
                }

                let entry = CachedResolvedSkill {
                    spec,
                    inheritance_chain,
                    included_from,
                    warnings,
                    cache_key_hash,
                    dependency_hashes,
                };

                // Update memory cache
                self.memory.write().insert(skill_id.to_string(), entry.clone());

                Ok(Some(entry))
            }
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Cache a resolved skill
    pub fn put(&self, skill_id: &str, resolved: &ResolvedSkillSpec, dependency_hashes: HashMap<String, String>) {
        let cache_key = CacheKey::new(skill_id, &dependency_hashes);

        let entry = CachedResolvedSkill {
            spec: resolved.spec.clone(),
            inheritance_chain: resolved.inheritance_chain.clone(),
            included_from: resolved.included_from.clone(),
            warnings: resolved.warnings.clone(),
            cache_key_hash: cache_key.hash(),
            dependency_hashes,
        };

        self.memory.write().insert(skill_id.to_string(), entry);
    }

    /// Cache a resolved skill and persist to SQLite
    pub fn put_to_db(
        &self,
        conn: &Connection,
        skill_id: &str,
        resolved: &ResolvedSkillSpec,
        dependency_hashes: HashMap<String, String>,
    ) -> Result<()> {
        let cache_key = CacheKey::new(skill_id, &dependency_hashes);

        let entry = CachedResolvedSkill {
            spec: resolved.spec.clone(),
            inheritance_chain: resolved.inheritance_chain.clone(),
            included_from: resolved.included_from.clone(),
            warnings: resolved.warnings.clone(),
            cache_key_hash: cache_key.hash(),
            dependency_hashes: dependency_hashes.clone(),
        };

        // Update memory
        self.memory.write().insert(skill_id.to_string(), entry.clone());

        // Update SQLite
        let resolved_json = serde_json::to_string(&resolved.spec)?;
        let chain_json = serde_json::to_string(&resolved.inheritance_chain)?;
        let included_json = serde_json::to_string(&resolved.included_from)?;
        let dep_hashes_json = serde_json::to_string(&dependency_hashes)?;
        let warnings_json = serde_json::to_string(&resolved.warnings)?;

        conn.execute(
            "INSERT OR REPLACE INTO resolved_skill_cache \
             (skill_id, resolved_json, cache_key_hash, inheritance_chain, included_from, dependency_hashes, warnings_json) \
             VALUES (?, ?, ?, ?, ?, ?, ?)",
            params![
                skill_id,
                resolved_json,
                entry.cache_key_hash,
                chain_json,
                included_json,
                dep_hashes_json,
                warnings_json,
            ],
        )?;

        // Update dependency graph
        {
            let mut graph = self.dependency_graph.write();

            // Remove old dependencies
            graph.remove_skill(skill_id);

            // Add new dependencies
            for parent_id in &resolved.inheritance_chain {
                if parent_id != skill_id {
                    graph.add_dependency(skill_id, parent_id, "extends");
                    DependencyGraph::add_dependency_to_db(conn, skill_id, parent_id, "extends")?;
                }
            }

            for included_id in &resolved.included_from {
                graph.add_dependency(skill_id, included_id, "includes");
                DependencyGraph::add_dependency_to_db(conn, skill_id, included_id, "includes")?;
            }
        }

        Ok(())
    }

    /// Invalidate a skill and all its dependents
    pub fn invalidate(&self, skill_id: &str) -> HashSet<String> {
        let dependents = self.dependency_graph.read().get_transitive_dependents(skill_id);

        let mut memory = self.memory.write();
        memory.invalidate(skill_id);

        for dependent in &dependents {
            memory.invalidate(dependent);
        }

        dependents
    }

    /// Invalidate a skill and all its dependents, also removing from SQLite
    pub fn invalidate_from_db(&self, conn: &Connection, skill_id: &str) -> Result<HashSet<String>> {
        let dependents = self.invalidate(skill_id);

        // Remove from SQLite
        conn.execute(
            "DELETE FROM resolved_skill_cache WHERE skill_id = ?",
            params![skill_id],
        )?;

        for dependent in &dependents {
            conn.execute(
                "DELETE FROM resolved_skill_cache WHERE skill_id = ?",
                params![dependent],
            )?;
        }

        // Update dependency graph in DB
        DependencyGraph::remove_skill_from_db(conn, skill_id)?;

        Ok(dependents)
    }

    /// Get a resolved skill from cache, or resolve and cache it if not present.
    ///
    /// This is the main entry point for cached resolution. It:
    /// 1. Checks memory cache first
    /// 2. If not found, checks SQLite cache
    /// 3. If still not found or stale, resolves using the repository and caches
    ///
    /// # Arguments
    /// * `conn` - SQLite connection for persistence
    /// * `skill_id` - ID of the skill to resolve
    /// * `raw_spec` - The raw skill spec (used if resolution is needed)
    /// * `repository` - Repository to fetch parent/included skills from
    /// * `compute_hash` - Function to compute content hash for a skill ID
    pub fn get_or_resolve<R, F>(
        &self,
        conn: &rusqlite::Connection,
        skill_id: &str,
        raw_spec: &crate::core::skill::SkillSpec,
        repository: &R,
        compute_hash: F,
    ) -> Result<ResolvedSkillSpec>
    where
        R: crate::core::resolution::SkillRepository + ?Sized,
        F: Fn(&str) -> Option<String>,
    {
        // Build dependency hashes for cache validation
        let mut dependency_hashes = HashMap::new();

        // Add self
        if let Some(hash) = compute_hash(skill_id) {
            dependency_hashes.insert(skill_id.to_string(), hash);
        }

        // Add parent if extends
        if let Some(parent_id) = &raw_spec.extends {
            if let Some(hash) = compute_hash(parent_id) {
                dependency_hashes.insert(parent_id.clone(), hash);
            }
        }

        // Add includes
        for include in &raw_spec.includes {
            if let Some(hash) = compute_hash(&include.skill) {
                dependency_hashes.insert(include.skill.clone(), hash);
            }
        }

        // Try to get from cache
        if let Some(cached) = self.get_from_db(conn, skill_id, &dependency_hashes)? {
            return Ok(cached.to_resolved_spec());
        }

        // Not in cache or stale - resolve
        let resolved = crate::core::resolution::resolve_full(raw_spec, repository)?;

        // Cache the result
        self.put_to_db(conn, skill_id, &resolved, dependency_hashes)?;

        Ok(resolved)
    }

    /// Clear all cache entries
    pub fn clear(&self) {
        self.memory.write().clear();
    }

    /// Clear all cache entries including SQLite
    pub fn clear_db(&self, conn: &Connection) -> Result<()> {
        self.clear();
        conn.execute("DELETE FROM resolved_skill_cache", [])?;
        conn.execute("DELETE FROM skill_dependency_graph", [])?;
        Ok(())
    }

    /// Get cache statistics
    pub fn stats(&self) -> CacheStats {
        let memory = self.memory.read();
        let graph = self.dependency_graph.read();

        CacheStats {
            memory_entries: memory.entries.len(),
            memory_capacity: memory.capacity,
            dependency_edges: graph.dependencies.values().map(|s| s.len()).sum(),
        }
    }
}

/// Cache statistics
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Number of entries in memory cache
    pub memory_entries: usize,
    /// Memory cache capacity
    pub memory_capacity: usize,
    /// Number of dependency edges in the graph
    pub dependency_edges: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::skill::SkillSpec;

    fn make_resolved(id: &str, chain: Vec<&str>, included: Vec<&str>) -> ResolvedSkillSpec {
        ResolvedSkillSpec {
            spec: SkillSpec::new(id, id),
            inheritance_chain: chain.into_iter().map(|s| s.to_string()).collect(),
            included_from: included.into_iter().map(|s| s.to_string()).collect(),
            warnings: vec![],
        }
    }

    fn make_dep_hashes(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    // =========================================================================
    // CacheKey tests
    // =========================================================================

    #[test]
    fn cache_key_deterministic() {
        let hashes1 = make_dep_hashes(&[("a", "hash_a"), ("b", "hash_b")]);
        let hashes2 = make_dep_hashes(&[("b", "hash_b"), ("a", "hash_a")]); // Different order

        let key1 = CacheKey::new("skill", &hashes1);
        let key2 = CacheKey::new("skill", &hashes2);

        assert_eq!(key1.dependency_hash, key2.dependency_hash);
    }

    #[test]
    fn cache_key_different_deps_different_hash() {
        let hashes1 = make_dep_hashes(&[("a", "hash_a")]);
        let hashes2 = make_dep_hashes(&[("a", "hash_b")]);

        let key1 = CacheKey::new("skill", &hashes1);
        let key2 = CacheKey::new("skill", &hashes2);

        assert_ne!(key1.dependency_hash, key2.dependency_hash);
    }

    // =========================================================================
    // MemoryCache tests
    // =========================================================================

    #[test]
    fn memory_cache_basic_operations() {
        let mut cache = MemoryCache::new(10);

        let resolved = make_resolved("skill1", vec!["skill1"], vec![]);
        let entry = CachedResolvedSkill {
            spec: resolved.spec.clone(),
            inheritance_chain: resolved.inheritance_chain.clone(),
            included_from: resolved.included_from.clone(),
            warnings: vec![],
            cache_key_hash: "hash1".to_string(),
            dependency_hashes: HashMap::new(),
        };

        cache.insert("skill1".to_string(), entry);

        assert!(cache.get("skill1").is_some());
        assert!(cache.get("nonexistent").is_none());
    }

    #[test]
    fn memory_cache_lru_eviction() {
        let mut cache = MemoryCache::new(2);

        for i in 1..=3 {
            let id = format!("skill{}", i);
            let resolved = make_resolved(&id, vec![&id], vec![]);
            let entry = CachedResolvedSkill {
                spec: resolved.spec.clone(),
                inheritance_chain: resolved.inheritance_chain.clone(),
                included_from: resolved.included_from.clone(),
                warnings: vec![],
                cache_key_hash: format!("hash{}", i),
                dependency_hashes: HashMap::new(),
            };
            cache.insert(id, entry);
        }

        // skill1 should have been evicted (oldest)
        assert!(cache.get("skill1").is_none());
        assert!(cache.get("skill2").is_some());
        assert!(cache.get("skill3").is_some());
    }

    #[test]
    fn memory_cache_lru_access_updates_order() {
        let mut cache = MemoryCache::new(2);

        // Insert skill1 and skill2
        for i in 1..=2 {
            let id = format!("skill{}", i);
            let resolved = make_resolved(&id, vec![&id], vec![]);
            let entry = CachedResolvedSkill {
                spec: resolved.spec.clone(),
                inheritance_chain: resolved.inheritance_chain.clone(),
                included_from: resolved.included_from.clone(),
                warnings: vec![],
                cache_key_hash: format!("hash{}", i),
                dependency_hashes: HashMap::new(),
            };
            cache.insert(id, entry);
        }

        // Access skill1 to make it recently used
        cache.get("skill1");

        // Insert skill3 - should evict skill2 (now oldest)
        let resolved = make_resolved("skill3", vec!["skill3"], vec![]);
        let entry = CachedResolvedSkill {
            spec: resolved.spec.clone(),
            inheritance_chain: resolved.inheritance_chain.clone(),
            included_from: resolved.included_from.clone(),
            warnings: vec![],
            cache_key_hash: "hash3".to_string(),
            dependency_hashes: HashMap::new(),
        };
        cache.insert("skill3".to_string(), entry);

        assert!(cache.get("skill1").is_some()); // Still present
        assert!(cache.get("skill2").is_none()); // Evicted
        assert!(cache.get("skill3").is_some()); // Present
    }

    #[test]
    fn memory_cache_invalidate() {
        let mut cache = MemoryCache::new(10);

        let resolved = make_resolved("skill1", vec!["skill1"], vec![]);
        let entry = CachedResolvedSkill {
            spec: resolved.spec.clone(),
            inheritance_chain: resolved.inheritance_chain.clone(),
            included_from: resolved.included_from.clone(),
            warnings: vec![],
            cache_key_hash: "hash1".to_string(),
            dependency_hashes: HashMap::new(),
        };
        cache.insert("skill1".to_string(), entry);

        cache.invalidate("skill1");
        assert!(cache.get("skill1").is_none());
    }

    // =========================================================================
    // DependencyGraph tests
    // =========================================================================

    #[test]
    fn dependency_graph_basic() {
        let mut graph = DependencyGraph::new();

        graph.add_dependency("child", "parent", "extends");
        graph.add_dependency("grandchild", "child", "extends");

        let dependents = graph.get_transitive_dependents("parent");
        assert!(dependents.contains("child"));
        assert!(dependents.contains("grandchild"));
        assert!(!dependents.contains("parent"));
    }

    #[test]
    fn dependency_graph_includes() {
        let mut graph = DependencyGraph::new();

        graph.add_dependency("main", "errors", "includes");
        graph.add_dependency("main", "logging", "includes");

        let dependents = graph.get_transitive_dependents("errors");
        assert!(dependents.contains("main"));

        let dependents = graph.get_transitive_dependents("logging");
        assert!(dependents.contains("main"));
    }

    #[test]
    fn dependency_graph_remove_skill() {
        let mut graph = DependencyGraph::new();

        graph.add_dependency("child", "parent", "extends");
        graph.remove_skill("child");

        let dependents = graph.get_transitive_dependents("parent");
        assert!(!dependents.contains("child"));
    }

    // =========================================================================
    // ResolutionCache tests
    // =========================================================================

    #[test]
    fn resolution_cache_basic() {
        let cache = ResolutionCache::new();

        let resolved = make_resolved("skill1", vec!["skill1"], vec![]);
        let dep_hashes = make_dep_hashes(&[("skill1", "hash_skill1")]);

        cache.put("skill1", &resolved, dep_hashes.clone());

        let entry = cache.get("skill1", &dep_hashes);
        assert!(entry.is_some());

        let entry = entry.unwrap();
        assert_eq!(entry.spec.metadata.id, "skill1");
    }

    #[test]
    fn resolution_cache_invalid_dep_hash() {
        let cache = ResolutionCache::new();

        let resolved = make_resolved("skill1", vec!["skill1"], vec![]);
        let dep_hashes = make_dep_hashes(&[("skill1", "hash_skill1")]);

        cache.put("skill1", &resolved, dep_hashes);

        // Different hash should miss
        let different_hashes = make_dep_hashes(&[("skill1", "different_hash")]);
        let entry = cache.get("skill1", &different_hashes);
        assert!(entry.is_none());
    }

    #[test]
    fn resolution_cache_invalidate_dependents() {
        let cache = ResolutionCache::new();

        // Set up dependency: child extends parent
        {
            let mut graph = cache.dependency_graph.write();
            graph.add_dependency("child", "parent", "extends");
            graph.add_dependency("grandchild", "child", "extends");
        }

        // Cache all three
        let dep_hashes = HashMap::new();
        cache.put("parent", &make_resolved("parent", vec!["parent"], vec![]), dep_hashes.clone());
        cache.put("child", &make_resolved("child", vec!["parent", "child"], vec![]), dep_hashes.clone());
        cache.put("grandchild", &make_resolved("grandchild", vec!["parent", "child", "grandchild"], vec![]), dep_hashes.clone());

        // Invalidate parent - should also invalidate child and grandchild
        let invalidated = cache.invalidate("parent");

        assert!(invalidated.contains("child"));
        assert!(invalidated.contains("grandchild"));

        // All should be missing from cache
        assert!(cache.get("parent", &dep_hashes).is_none());
        assert!(cache.get("child", &dep_hashes).is_none());
        assert!(cache.get("grandchild", &dep_hashes).is_none());
    }

    #[test]
    fn resolution_cache_stats() {
        let cache = ResolutionCache::new();

        let stats = cache.stats();
        assert_eq!(stats.memory_entries, 0);

        let dep_hashes = HashMap::new();
        cache.put("skill1", &make_resolved("skill1", vec!["skill1"], vec![]), dep_hashes.clone());
        cache.put("skill2", &make_resolved("skill2", vec!["skill2"], vec![]), dep_hashes);

        let stats = cache.stats();
        assert_eq!(stats.memory_entries, 2);
    }

    #[test]
    fn cached_resolved_skill_to_resolved_spec() {
        let entry = CachedResolvedSkill {
            spec: SkillSpec::new("skill1", "Skill 1"),
            inheritance_chain: vec!["parent".to_string(), "skill1".to_string()],
            included_from: vec!["helper".to_string()],
            warnings: vec![],
            cache_key_hash: "hash".to_string(),
            dependency_hashes: HashMap::new(),
        };

        let resolved = entry.to_resolved_spec();
        assert_eq!(resolved.spec.metadata.id, "skill1");
        assert_eq!(resolved.inheritance_chain, vec!["parent", "skill1"]);
        assert_eq!(resolved.included_from, vec!["helper"]);
        assert!(resolved.warnings.is_empty());
    }
}
