//! Dependency Graph Resolution
//!
//! Skills declare dependencies (`requires`), capabilities (`provides`), and environment
//! requirements in metadata. ms builds a dependency graph to resolve load order,
//! detect cycles, and auto-load prerequisites.

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::error::{MsError, Result};

/// Mode for loading dependencies
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DependencyLoadMode {
    /// No dependency loading
    Off,
    /// Dependencies at overview, root at requested level (default)
    #[default]
    Auto,
    /// Dependencies at full disclosure
    Full,
    /// Dependencies at overview/minimal
    Overview,
}

/// Disclosure level for a skill
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum DisclosureLevel {
    Minimal,
    Overview,
    #[default]
    Standard,
    Full,
    Complete,
}


/// A node in the dependency graph
#[derive(Debug, Clone)]
pub struct DependencyNode {
    /// Skill ID
    pub skill_id: String,
    /// Capabilities this skill provides
    pub provides: Vec<String>,
    /// Capabilities this skill requires
    pub requires: Vec<String>,
}

/// An edge in the dependency graph (from -> to means "from depends on to")
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DependencyEdge {
    /// The skill that has the dependency
    pub from: String,
    /// The skill that is depended upon
    pub to: String,
    /// The capability that created this edge
    pub capability: String,
}

/// The dependency graph for skills
#[derive(Debug, Clone, Default)]
pub struct DependencyGraph {
    /// Nodes in the graph (`skill_id` -> node)
    nodes: HashMap<String, DependencyNode>,
    /// Edges in the graph
    edges: Vec<DependencyEdge>,
    /// Index: capability -> skills that provide it
    capability_providers: HashMap<String, Vec<String>>,
}

impl DependencyGraph {
    /// Create a new empty dependency graph
    #[must_use] 
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a skill node to the graph
    pub fn add_skill(&mut self, skill_id: String, requires: Vec<String>, provides: Vec<String>) {
        // Update capability providers index
        for cap in &provides {
            self.capability_providers
                .entry(cap.clone())
                .or_default()
                .push(skill_id.clone());
        }

        self.nodes.insert(
            skill_id.clone(),
            DependencyNode {
                skill_id,
                provides,
                requires,
            },
        );
    }

    /// Build edges based on requires/provides relationships
    pub fn build_edges(&mut self) {
        self.edges.clear();

        // Sort nodes for deterministic edge generation
        let mut sorted_nodes: Vec<_> = self.nodes.values().collect();
        sorted_nodes.sort_by(|a, b| a.skill_id.cmp(&b.skill_id));

        for node in sorted_nodes {
            let mut requires = node.requires.clone();
            requires.sort(); // Deterministic capability processing

            for required_cap in &requires {
                if let Some(providers) = self.capability_providers.get(required_cap) {
                    let mut sorted_providers = providers.clone();
                    sorted_providers.sort(); // Deterministic provider order

                    for provider in sorted_providers {
                        if provider != node.skill_id {
                            self.edges.push(DependencyEdge {
                                from: node.skill_id.clone(),
                                to: provider,
                                capability: required_cap.clone(),
                            });
                        }
                    }
                }
            }
        }
    }

    /// Get all nodes
    pub fn nodes(&self) -> impl Iterator<Item = &DependencyNode> {
        self.nodes.values()
    }

    /// Get a specific node
    #[must_use] 
    pub fn get_node(&self, skill_id: &str) -> Option<&DependencyNode> {
        self.nodes.get(skill_id)
    }

    /// Get all edges
    #[must_use] 
    pub fn edges(&self) -> &[DependencyEdge] {
        &self.edges
    }

    /// Get direct dependencies of a skill
    #[must_use] 
    pub fn direct_dependencies(&self, skill_id: &str) -> Vec<&str> {
        self.edges
            .iter()
            .filter(|e| e.from == skill_id)
            .map(|e| e.to.as_str())
            .collect()
    }

    /// Get skills that directly depend on this skill
    #[must_use] 
    pub fn direct_dependents(&self, skill_id: &str) -> Vec<&str> {
        self.edges
            .iter()
            .filter(|e| e.to == skill_id)
            .map(|e| e.from.as_str())
            .collect()
    }

    /// Find providers for a capability
    #[must_use] 
    pub fn find_providers(&self, capability: &str) -> Option<&Vec<String>> {
        self.capability_providers.get(capability)
    }
}

/// A single skill load plan entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillLoadPlan {
    /// Skill ID to load
    pub skill_id: String,
    /// Disclosure level to use
    pub disclosure: DisclosureLevel,
    /// Reason for loading (e.g., "root", "dependency of X")
    pub reason: String,
}

/// Result of resolving dependencies
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResolvedDependencyPlan {
    /// Skills in topological order (dependencies first)
    pub ordered: Vec<SkillLoadPlan>,
    /// Missing capabilities that could not be resolved
    pub missing: Vec<MissingCapability>,
    /// Detected cycles (each cycle is a vec of skill IDs)
    pub cycles: Vec<Vec<String>>,
}

/// A missing capability that couldn't be satisfied
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MissingCapability {
    /// The skill that requires this capability
    pub required_by: String,
    /// The capability that's missing
    pub capability: String,
}

impl ResolvedDependencyPlan {
    /// Check if resolution was successful (no missing deps, no cycles)
    #[must_use] 
    pub fn is_ok(&self) -> bool {
        self.missing.is_empty() && self.cycles.is_empty()
    }

    /// Check if there are any issues
    #[must_use] 
    pub fn has_issues(&self) -> bool {
        !self.missing.is_empty() || !self.cycles.is_empty()
    }
}

/// Resolver for dependency graphs
pub struct DependencyResolver<'a> {
    graph: &'a DependencyGraph,
    max_depth: usize,
}

impl<'a> DependencyResolver<'a> {
    /// Create a new resolver
    #[must_use] 
    pub const fn new(graph: &'a DependencyGraph) -> Self {
        Self {
            graph,
            max_depth: 100, // Prevent infinite recursion
        }
    }

    /// Set maximum depth for dependency traversal
    #[must_use] 
    pub const fn with_max_depth(mut self, depth: usize) -> Self {
        self.max_depth = depth;
        self
    }

    /// Resolve dependencies for a root skill
    pub fn resolve(
        &self,
        root_skill_id: &str,
        root_disclosure: DisclosureLevel,
        mode: DependencyLoadMode,
    ) -> Result<ResolvedDependencyPlan> {
        if mode == DependencyLoadMode::Off {
            return Ok(ResolvedDependencyPlan {
                ordered: vec![SkillLoadPlan {
                    skill_id: root_skill_id.to_string(),
                    disclosure: root_disclosure,
                    reason: "root".to_string(),
                }],
                missing: vec![],
                cycles: vec![],
            });
        }

        // Step 1: Expand dependency closure (BFS)
        let (closure, missing) = self.expand_closure(root_skill_id)?;

        // Step 2: Detect cycles using Tarjan's algorithm
        let cycles = self.detect_cycles(&closure);

        // Step 3: Topological sort (if no cycles)
        let topo_order = if cycles.is_empty() {
            self.topological_sort(&closure, root_skill_id)?
        } else {
            // If there are cycles, we can't do a clean topo sort
            // Return partial ordering with cycle members at the end
            // Sort for determinism in error scenarios
            let mut sorted: Vec<String> = closure.into_iter().collect();
            sorted.sort();
            sorted
        };

        // Step 4: Assign disclosure levels
        let ordered =
            self.assign_disclosure_levels(&topo_order, root_skill_id, root_disclosure, mode)?;

        Ok(ResolvedDependencyPlan {
            ordered,
            missing,
            cycles,
        })
    }

    /// Expand dependency closure using BFS
    fn expand_closure(&self, root: &str) -> Result<(HashSet<String>, Vec<MissingCapability>)> {
        let mut closure = HashSet::new();
        let mut missing = Vec::new();
        let mut queue = VecDeque::new();
        let mut depth = 0;

        queue.push_back(root.to_string());
        closure.insert(root.to_string());

        while !queue.is_empty() && depth < self.max_depth {
            let level_size = queue.len();

            // Collect level nodes to process them deterministically if needed, 
            // but BFS queue order is already determined by insertion order.
            // We just need to ensure insertion order is deterministic.

            for _ in 0..level_size {
                if let Some(skill_id) = queue.pop_front() {
                    if let Some(node) = self.graph.get_node(&skill_id) {
                        // Sort requirements for deterministic processing
                        let mut requires = node.requires.clone();
                        requires.sort();

                        for required_cap in &requires {
                            if let Some(providers) = self.graph.find_providers(required_cap) {
                                // Sort providers for deterministic queue insertion
                                let mut sorted_providers = providers.clone();
                                sorted_providers.sort();

                                for provider in sorted_providers {
                                    if !closure.contains(&provider) {
                                        closure.insert(provider.clone());
                                        queue.push_back(provider);
                                    }
                                }
                            } else {
                                missing.push(MissingCapability {
                                    required_by: skill_id.clone(),
                                    capability: required_cap.clone(),
                                });
                            }
                        }
                    }
                }
            }
            depth += 1;
        }

        Ok((closure, missing))
    }

    /// Detect cycles using DFS with back-edge detection
    fn detect_cycles(&self, closure: &HashSet<String>) -> Vec<Vec<String>> {
        let mut cycles = Vec::new();
        let mut visited = HashSet::new();
        let mut rec_stack = HashSet::new();
        let mut path = Vec::new();

        for skill_id in closure {
            if !visited.contains(skill_id) {
                self.dfs_detect_cycles(
                    skill_id,
                    closure,
                    &mut visited,
                    &mut rec_stack,
                    &mut path,
                    &mut cycles,
                );
            }
        }

        cycles
    }

    fn dfs_detect_cycles(
        &self,
        node: &str,
        closure: &HashSet<String>,
        visited: &mut HashSet<String>,
        rec_stack: &mut HashSet<String>,
        path: &mut Vec<String>,
        cycles: &mut Vec<Vec<String>>,
    ) {
        visited.insert(node.to_string());
        rec_stack.insert(node.to_string());
        path.push(node.to_string());

        for dep in self.graph.direct_dependencies(node) {
            if !closure.contains(dep) {
                continue;
            }

            if !visited.contains(dep) {
                self.dfs_detect_cycles(dep, closure, visited, rec_stack, path, cycles);
            } else if rec_stack.contains(dep) {
                // Found a cycle - extract it from the path
                if let Some(cycle_start) = path.iter().position(|x| x == dep) {
                    let cycle: Vec<String> = path[cycle_start..].to_vec();
                    cycles.push(cycle);
                }
            }
        }

        path.pop();
        rec_stack.remove(node);
    }

    /// Topological sort using Kahn's algorithm
    fn topological_sort(&self, closure: &HashSet<String>, _root: &str) -> Result<Vec<String>> {
        let mut in_degree: HashMap<&str, usize> = HashMap::new();
        let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();

        // Initialize
        for skill_id in closure {
            in_degree.insert(skill_id.as_str(), 0);
            adj.insert(skill_id.as_str(), vec![]);
        }

        // Build adjacency list and in-degrees
        for skill_id in closure {
            for dep in self.graph.direct_dependencies(skill_id) {
                if closure.contains(dep) {
                    // Use if-let to safely handle missing keys (defensive against graph bugs)
                    if let Some(adj_list) = adj.get_mut(skill_id.as_str()) {
                        adj_list.push(dep);
                    }
                    if let Some(degree) = in_degree.get_mut(dep) {
                        *degree += 1;
                    }
                }
            }
        }

        // Sort adjacency lists for determinism
        for list in adj.values_mut() {
            list.sort_unstable();
        }

        // Kahn's algorithm
        // Collect 0-degree nodes and sort them
        let mut zero_degree: Vec<&str> = in_degree
            .iter()
            .filter(|&(_, &degree)| degree == 0)
            .map(|(skill, _)| *skill)
            .collect();
        zero_degree.sort_unstable();

        let mut queue: VecDeque<&str> = VecDeque::from(zero_degree);

        let mut result = Vec::new();
        while let Some(skill) = queue.pop_front() {
            result.push(skill.to_string());

            if let Some(neighbors) = adj.get(skill) {
                for &neighbor in neighbors {
                    if let Some(degree) = in_degree.get_mut(neighbor) {
                        *degree -= 1;
                        if *degree == 0 {
                            queue.push_back(neighbor);
                        }
                    }
                }
            }
        }

        // Reverse so dependencies come first
        result.reverse();

        Ok(result)
    }

    /// Assign disclosure levels based on mode
    fn assign_disclosure_levels(
        &self,
        topo_order: &[String],
        root_skill_id: &str,
        root_disclosure: DisclosureLevel,
        mode: DependencyLoadMode,
    ) -> Result<Vec<SkillLoadPlan>> {
        let dep_disclosure = match mode {
            DependencyLoadMode::Off => {
                return Err(MsError::ValidationFailed(
                    "dependency resolution requested with load mode off".to_string(),
                ));
            }
            DependencyLoadMode::Auto => DisclosureLevel::Overview,
            DependencyLoadMode::Full => DisclosureLevel::Full,
            DependencyLoadMode::Overview => DisclosureLevel::Overview,
        };

        Ok(topo_order
            .iter()
            .map(|skill_id| {
                let (disclosure, reason) = if skill_id == root_skill_id {
                    (root_disclosure, "root".to_string())
                } else {
                    (dep_disclosure, format!("dependency of {root_skill_id}"))
                };

                SkillLoadPlan {
                    skill_id: skill_id.clone(),
                    disclosure,
                    reason,
                }
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_graph() {
        let graph = DependencyGraph::new();
        assert_eq!(graph.nodes().count(), 0);
        assert!(graph.edges().is_empty());
    }

    #[test]
    fn test_add_skill() {
        let mut graph = DependencyGraph::new();
        graph.add_skill(
            "skill-a".to_string(),
            vec!["cap-x".to_string()],
            vec!["cap-a".to_string()],
        );

        assert_eq!(graph.nodes().count(), 1);
        assert!(graph.get_node("skill-a").is_some());
    }

    #[test]
    fn test_build_edges() {
        let mut graph = DependencyGraph::new();
        graph.add_skill(
            "skill-a".to_string(),
            vec!["cap-b".to_string()], // A requires cap-b
            vec!["cap-a".to_string()], // A provides cap-a
        );
        graph.add_skill(
            "skill-b".to_string(),
            vec![],                    // B requires nothing
            vec!["cap-b".to_string()], // B provides cap-b
        );

        graph.build_edges();

        assert_eq!(graph.edges().len(), 1);
        assert_eq!(graph.edges()[0].from, "skill-a");
        assert_eq!(graph.edges()[0].to, "skill-b");
    }

    #[test]
    fn test_direct_dependencies() {
        let mut graph = DependencyGraph::new();
        graph.add_skill("a".to_string(), vec!["cap-b".to_string()], vec![]);
        graph.add_skill("b".to_string(), vec![], vec!["cap-b".to_string()]);
        graph.build_edges();

        let deps = graph.direct_dependencies("a");
        assert_eq!(deps, vec!["b"]);
    }

    #[test]
    fn test_resolve_no_deps() {
        let graph = DependencyGraph::new();
        let resolver = DependencyResolver::new(&graph);

        // Note: This will resolve even without the skill in the graph
        // since we're just testing the resolution logic
        let plan = resolver
            .resolve("root", DisclosureLevel::Standard, DependencyLoadMode::Off)
            .unwrap();

        assert_eq!(plan.ordered.len(), 1);
        assert_eq!(plan.ordered[0].skill_id, "root");
        assert!(plan.is_ok());
    }

    #[test]
    fn test_resolve_with_deps() {
        let mut graph = DependencyGraph::new();
        graph.add_skill("root".to_string(), vec!["cap-a".to_string()], vec![]);
        graph.add_skill("dep-a".to_string(), vec![], vec!["cap-a".to_string()]);
        graph.build_edges();

        let resolver = DependencyResolver::new(&graph);
        let plan = resolver
            .resolve("root", DisclosureLevel::Full, DependencyLoadMode::Auto)
            .unwrap();

        assert_eq!(plan.ordered.len(), 2);
        // Dependencies should come first
        assert_eq!(plan.ordered[0].skill_id, "dep-a");
        assert_eq!(plan.ordered[0].disclosure, DisclosureLevel::Overview);
        assert_eq!(plan.ordered[1].skill_id, "root");
        assert_eq!(plan.ordered[1].disclosure, DisclosureLevel::Full);
        assert!(plan.is_ok());
    }

    #[test]
    fn test_detect_cycle() {
        let mut graph = DependencyGraph::new();
        graph.add_skill(
            "a".to_string(),
            vec!["cap-b".to_string()],
            vec!["cap-a".to_string()],
        );
        graph.add_skill(
            "b".to_string(),
            vec!["cap-a".to_string()],
            vec!["cap-b".to_string()],
        );
        graph.build_edges();

        let resolver = DependencyResolver::new(&graph);
        let plan = resolver
            .resolve("a", DisclosureLevel::Standard, DependencyLoadMode::Auto)
            .unwrap();

        assert!(!plan.cycles.is_empty());
        assert!(plan.has_issues());
    }

    #[test]
    fn test_missing_capability() {
        let mut graph = DependencyGraph::new();
        graph.add_skill("a".to_string(), vec!["missing-cap".to_string()], vec![]);
        graph.build_edges();

        let resolver = DependencyResolver::new(&graph);
        let plan = resolver
            .resolve("a", DisclosureLevel::Standard, DependencyLoadMode::Auto)
            .unwrap();

        assert_eq!(plan.missing.len(), 1);
        assert_eq!(plan.missing[0].capability, "missing-cap");
        assert!(plan.has_issues());
    }

    #[test]
    fn test_transitive_deps() {
        let mut graph = DependencyGraph::new();
        graph.add_skill("a".to_string(), vec!["cap-b".to_string()], vec![]);
        graph.add_skill(
            "b".to_string(),
            vec!["cap-c".to_string()],
            vec!["cap-b".to_string()],
        );
        graph.add_skill("c".to_string(), vec![], vec!["cap-c".to_string()]);
        graph.build_edges();

        let resolver = DependencyResolver::new(&graph);
        let plan = resolver
            .resolve("a", DisclosureLevel::Full, DependencyLoadMode::Auto)
            .unwrap();

        assert_eq!(plan.ordered.len(), 3);
        // Order should be: c, b, a (dependencies first)
        assert_eq!(plan.ordered[0].skill_id, "c");
        assert_eq!(plan.ordered[1].skill_id, "b");
        assert_eq!(plan.ordered[2].skill_id, "a");
        assert!(plan.is_ok());
    }

    #[test]
    fn test_disclosure_modes() {
        let mut graph = DependencyGraph::new();
        graph.add_skill("root".to_string(), vec!["cap-a".to_string()], vec![]);
        graph.add_skill("dep".to_string(), vec![], vec!["cap-a".to_string()]);
        graph.build_edges();

        let resolver = DependencyResolver::new(&graph);

        // Test Full mode
        let plan = resolver
            .resolve("root", DisclosureLevel::Complete, DependencyLoadMode::Full)
            .unwrap();
        assert_eq!(plan.ordered[0].disclosure, DisclosureLevel::Full);
        assert_eq!(plan.ordered[1].disclosure, DisclosureLevel::Complete);

        // Test Overview mode
        let plan = resolver
            .resolve(
                "root",
                DisclosureLevel::Complete,
                DependencyLoadMode::Overview,
            )
            .unwrap();
        assert_eq!(plan.ordered[0].disclosure, DisclosureLevel::Overview);
    }

    #[test]
    fn test_edges_determinism() {
        let mut graph = DependencyGraph::new();
        // Add skills in arbitrary order (HashMap will randomize iteration anyway)
        graph.add_skill("a".to_string(), vec!["cap-b".to_string(), "cap-c".to_string()], vec![]);
        graph.add_skill("b".to_string(), vec![], vec!["cap-b".to_string()]);
        graph.add_skill("c".to_string(), vec![], vec!["cap-c".to_string()]);
        
        graph.build_edges();
        
        // Check that edges are sorted or at least consistent if we run this multiple times
        // Ideally edges should be sorted by (from, to, capability)
        // For now, we just assert that we can get a stable output if we implement sorting
    }

    #[test]
    fn test_topo_sort_determinism() {
        let mut graph = DependencyGraph::new();
        // Diamond dependency: A -> B, A -> C, B -> D, C -> D
        // B provides cap-b (needed by A)
        // C provides cap-c (needed by A)
        // D provides cap-d (needed by B and C)
        graph.add_skill("a".to_string(), vec!["cap-b".to_string(), "cap-c".to_string()], vec![]);
        graph.add_skill("b".to_string(), vec!["cap-d".to_string()], vec!["cap-b".to_string()]);
        graph.add_skill("c".to_string(), vec!["cap-d".to_string()], vec!["cap-c".to_string()]);
        graph.add_skill("d".to_string(), vec![], vec!["cap-d".to_string()]);
        graph.build_edges();

        let resolver = DependencyResolver::new(&graph);
        
        let plan1 = resolver.resolve("a", DisclosureLevel::Standard, DependencyLoadMode::Auto).unwrap();
        
        // Re-create to test stability across runs (though hashmap iteration inside same process usually stable for small size)
        // Real determinism issue arises from hashmap iteration order varying by random seed between runs
        // But we can check if the output satisfies one of the valid topological sorts and stays consistent if we enforce it.
        // D must be first. A must be last. B and C can be in any order relative to each other, but should be deterministic.
        
        let order: Vec<_> = plan1.ordered.iter().map(|p| p.skill_id.as_str()).collect();
        assert_eq!(order.first().unwrap(), &"d");
        assert_eq!(order.last().unwrap(), &"a");
        
        // We enforce alphabetical for ties: B comes before C
        // So expected: D, B, C, A
        // If sorting isn't implemented, this might be D, C, B, A randomly.
    }
}
