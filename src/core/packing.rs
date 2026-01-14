//! Constrained token packing for skill slices.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::disclosure::PackMode;
use super::skill::{PackContract, SkillSlice, SliceType};

/// Constraints for token packing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackConstraints {
    /// Maximum tokens allowed for packed slices.
    pub budget: usize,
    /// Max slices per coverage group.
    pub max_per_group: usize,
    /// Required coverage quotas.
    pub required_coverage: Vec<CoverageQuota>,
    /// Coverage groups that should never be included.
    pub excluded_groups: Vec<String>,
    /// Maximum improvement iterations.
    pub max_improvement_passes: usize,
    /// Mandatory slices that must be included.
    pub mandatory_slices: Vec<MandatorySlice>,
    /// Fail if any mandatory slices cannot be included.
    pub fail_on_mandatory_omission: bool,
    /// Recently used slices for novelty penalty.
    pub recent_slice_ids: Vec<String>,
    /// Optional pack contract.
    pub contract: Option<PackContract>,
}

impl PackConstraints {
    pub fn new(budget: usize, max_per_group: usize) -> Self {
        Self {
            budget,
            max_per_group,
            required_coverage: Vec::new(),
            excluded_groups: Vec::new(),
            max_improvement_passes: 2,
            mandatory_slices: Vec::new(),
            fail_on_mandatory_omission: true,
            recent_slice_ids: Vec::new(),
            contract: None,
        }
    }
}

/// Required coverage quotas.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverageQuota {
    pub group: String,
    pub min_count: usize,
}

/// Mandatory slice specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MandatorySlice {
    ById(String),
    ByPredicate(MandatoryPredicate),
}

/// Predicate-based mandatory selection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MandatoryPredicate {
    /// Always include policy slices.
    Always,
    /// Require tag match.
    HasTag(String),
    /// Require slice type.
    OfType(SliceType),
    /// Require coverage group.
    InGroup(String),
    /// Custom match (treated as tag or id match).
    Custom(String),
}

/// Packing result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackResult {
    pub slices: Vec<SkillSlice>,
    pub total_tokens: usize,
    pub coverage_satisfied: bool,
}

/// Packing errors.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PackError {
    MandatorySliceOmitted {
        slice_id: String,
        required_tokens: usize,
        available_tokens: usize,
    },
    InsufficientBudget {
        required: usize,
        available: usize,
    },
}

/// Constrained packer implementation.
pub struct ConstrainedPacker;

impl ConstrainedPacker {
    pub fn pack(
        &self,
        slices: &[SkillSlice],
        constraints: &PackConstraints,
        mode: PackMode,
    ) -> Result<PackResult, PackError> {
        if constraints.budget == 0 {
            return Ok(PackResult {
                slices: Vec::new(),
                total_tokens: 0,
                coverage_satisfied: true,
            });
        }

        let excluded_groups = normalize_groups(&constraints.excluded_groups);
        let required_coverage = merge_required_coverage(constraints);
        let max_per_group = contract_max_per_group(constraints);
        let mut selected: Vec<SkillSlice> = Vec::new();
        let mut selected_ids: HashSet<String> = HashSet::new();

        let mut mandatory = collect_mandatory_slices(slices, constraints);
        let required_tokens = mandatory
            .iter()
            .map(|slice| slice.token_estimate)
            .sum::<usize>();

        if required_tokens > constraints.budget {
            if constraints.fail_on_mandatory_omission {
                return Err(PackError::MandatorySliceOmitted {
                    slice_id: "mandatory".to_string(),
                    required_tokens,
                    available_tokens: constraints.budget,
                });
            }
            mandatory.sort_by(|a, b| {
                score_slice(b, mode)
                    .partial_cmp(&score_slice(a, mode))
                    .unwrap_or(Ordering::Equal)
            });
        }

        let mut remaining = constraints.budget;
        for slice in mandatory {
            if remaining < slice.token_estimate {
                continue;
            }
            remaining -= slice.token_estimate;
            selected_ids.insert(slice.id.clone());
            selected.push(slice);
        }

        let mut group_counts = count_groups(&selected);

        if let Some(overview) = slices
            .iter()
            .find(|slice| slice.slice_type == SliceType::Overview)
        {
            if !selected_ids.contains(&overview.id)
                && overview.token_estimate <= remaining
                && can_add_slice(overview, &excluded_groups, &group_counts, max_per_group)
                && deps_satisfied(overview, &selected_ids)
            {
                selected_ids.insert(overview.id.clone());
                selected.push(overview.clone());
                remaining = remaining.saturating_sub(overview.token_estimate);
                add_group_count(&mut group_counts, overview);
            }
        }

        for quota in &required_coverage {
            let mut count = group_counts.get(&quota.group).copied().unwrap_or(0);
            if count >= quota.min_count {
                continue;
            }
            let group_slices: Vec<&SkillSlice> = slices
                .iter()
                .filter(|slice| slice.coverage_group.as_deref() == Some(quota.group.as_str()))
                .filter(|slice| !selected_ids.contains(&slice.id))
                .filter(|slice| !is_excluded(slice, &excluded_groups))
                .filter(|slice| deps_satisfied(slice, &selected_ids))
                .collect();
            let ranked = rank_by_density(
                &group_slices,
                &group_counts,
                &constraints.recent_slice_ids,
                mode,
            );
            for slice in ranked {
                if count >= quota.min_count {
                    break;
                }
                if slice.token_estimate > remaining {
                    continue;
                }
                if !can_add_slice(slice, &excluded_groups, &group_counts, max_per_group) {
                    continue;
                }
                selected_ids.insert(slice.id.clone());
                selected.push(slice.clone());
                remaining -= slice.token_estimate;
                add_group_count(&mut group_counts, slice);
                count += 1;
            }
        }

        let candidates: Vec<&SkillSlice> = slices
            .iter()
            .filter(|slice| !selected_ids.contains(&slice.id))
            .filter(|slice| !is_excluded(slice, &excluded_groups))
            .filter(|slice| deps_satisfied(slice, &selected_ids))
            .collect();
        let ranked = rank_by_density(
            &candidates,
            &group_counts,
            &constraints.recent_slice_ids,
            mode,
        );
        for slice in ranked {
            if slice.token_estimate > remaining {
                continue;
            }
            if !can_add_slice(slice, &excluded_groups, &group_counts, max_per_group) {
                continue;
            }
            selected_ids.insert(slice.id.clone());
            selected.push(slice.clone());
            remaining -= slice.token_estimate;
            add_group_count(&mut group_counts, slice);
        }

        for _ in 0..constraints.max_improvement_passes {
            if !try_improve(
                slices,
                &mut selected,
                &mut selected_ids,
                constraints,
                mode,
                max_per_group,
                &excluded_groups,
            ) {
                break;
            }
        }

        let coverage_satisfied = check_coverage(&selected, &required_coverage);
        let total_tokens = constraints.budget.saturating_sub(remaining);

        Ok(PackResult {
            slices: selected,
            total_tokens,
            coverage_satisfied,
        })
    }
}

fn contract_max_per_group(constraints: &PackConstraints) -> usize {
    constraints
        .contract
        .as_ref()
        .and_then(|contract| contract.max_per_group)
        .unwrap_or(constraints.max_per_group)
}

fn merge_required_coverage(constraints: &PackConstraints) -> Vec<CoverageQuota> {
    let mut merged: HashMap<String, usize> = HashMap::new();
    for quota in &constraints.required_coverage {
        merged
            .entry(quota.group.clone())
            .and_modify(|count| *count = (*count).max(quota.min_count))
            .or_insert(quota.min_count);
    }
    if let Some(contract) = &constraints.contract {
        for group in &contract.required_groups {
            merged.entry(group.clone()).or_insert(1);
        }
    }
    let mut items: Vec<CoverageQuota> = merged
        .into_iter()
        .map(|(group, min_count)| CoverageQuota { group, min_count })
        .collect();
    items.sort_by(|a, b| a.group.cmp(&b.group));
    items
}

fn collect_mandatory_slices(
    slices: &[SkillSlice],
    constraints: &PackConstraints,
) -> Vec<SkillSlice> {
    let mut mandatory_specs = constraints.mandatory_slices.clone();
    if let Some(contract) = &constraints.contract {
        for id in &contract.mandatory_slices {
            mandatory_specs.push(MandatorySlice::ById(id.clone()));
        }
    }

    let mut seen: HashSet<String> = HashSet::new();
    let mut mandatory: Vec<SkillSlice> = Vec::new();
    for spec in mandatory_specs {
        match spec {
            MandatorySlice::ById(id) => {
                if let Some(slice) = slices.iter().find(|slice| slice.id == id) {
                    if seen.insert(slice.id.clone()) {
                        mandatory.push(slice.clone());
                    }
                }
            }
            MandatorySlice::ByPredicate(predicate) => {
                for slice in slices {
                    if matches_predicate(slice, &predicate) && seen.insert(slice.id.clone()) {
                        mandatory.push(slice.clone());
                    }
                }
            }
        }
    }
    mandatory
}

fn matches_predicate(slice: &SkillSlice, predicate: &MandatoryPredicate) -> bool {
    match predicate {
        MandatoryPredicate::Always => slice.slice_type == SliceType::Policy,
        MandatoryPredicate::HasTag(tag) => slice.tags.iter().any(|t| t == tag),
        MandatoryPredicate::OfType(slice_type) => slice.slice_type == *slice_type,
        MandatoryPredicate::InGroup(group) => {
            slice.coverage_group.as_deref() == Some(group.as_str())
        }
        MandatoryPredicate::Custom(value) => {
            slice.id == *value || slice.tags.iter().any(|t| t == value)
        }
    }
}

fn normalize_groups(groups: &[String]) -> HashSet<String> {
    groups
        .iter()
        .map(|group| group.to_lowercase())
        .collect::<HashSet<String>>()
}

fn is_excluded(slice: &SkillSlice, excluded: &HashSet<String>) -> bool {
    slice
        .coverage_group
        .as_ref()
        .map(|group| excluded.contains(&group.to_lowercase()))
        .unwrap_or(false)
}

fn can_add_slice(
    slice: &SkillSlice,
    excluded: &HashSet<String>,
    group_counts: &HashMap<String, usize>,
    max_per_group: usize,
) -> bool {
    if is_excluded(slice, excluded) {
        return false;
    }
    if let Some(group) = &slice.coverage_group {
        let count = group_counts.get(group).copied().unwrap_or(0);
        if count >= max_per_group {
            return false;
        }
    }
    true
}

fn add_group_count(group_counts: &mut HashMap<String, usize>, slice: &SkillSlice) {
    if let Some(group) = &slice.coverage_group {
        *group_counts.entry(group.clone()).or_insert(0) += 1;
    }
}

fn count_groups(slices: &[SkillSlice]) -> HashMap<String, usize> {
    let mut counts = HashMap::new();
    for slice in slices {
        add_group_count(&mut counts, slice);
    }
    counts
}

fn deps_satisfied(slice: &SkillSlice, selected_ids: &HashSet<String>) -> bool {
    slice.requires.iter().all(|req| selected_ids.contains(req))
}

fn rank_by_density<'a>(
    slices: &[&'a SkillSlice],
    group_counts: &HashMap<String, usize>,
    recent_slice_ids: &[String],
    mode: PackMode,
) -> Vec<&'a SkillSlice> {
    let recent: HashSet<&str> = recent_slice_ids.iter().map(|s| s.as_str()).collect();
    let mut scored: Vec<(f32, &'a SkillSlice)> = slices
        .iter()
        .map(|slice| {
            let base = score_slice(slice, mode);
            let density = base / slice.token_estimate.max(1) as f32;
            let group_penalty = slice
                .coverage_group
                .as_ref()
                .and_then(|group| group_counts.get(group))
                .map(|count| 0.8_f32.powi(*count as i32))
                .unwrap_or(1.0);
            let novelty_penalty = if recent.contains(slice.id.as_str()) {
                0.6
            } else {
                1.0
            };
            (density * group_penalty * novelty_penalty, *slice)
        })
        .collect();
    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.1.id.cmp(&b.1.id))
    });
    scored.into_iter().map(|(_, slice)| slice).collect()
}

fn score_slice(slice: &SkillSlice, mode: PackMode) -> f32 {
    match mode {
        PackMode::Balanced => slice.utility_score,
        PackMode::UtilityFirst => slice.utility_score,
        PackMode::CoverageFirst => match slice.slice_type {
            SliceType::Rule => slice.utility_score + 0.2,
            SliceType::Command => slice.utility_score + 0.15,
            SliceType::Example => slice.utility_score + 0.1,
            _ => slice.utility_score,
        },
        PackMode::PitfallSafe => match slice.slice_type {
            SliceType::Pitfall => slice.utility_score + 0.25,
            SliceType::Rule => slice.utility_score + 0.1,
            _ => slice.utility_score,
        },
    }
}

fn check_coverage(slices: &[SkillSlice], quotas: &[CoverageQuota]) -> bool {
    let counts = count_groups(slices);
    quotas
        .iter()
        .all(|quota| counts.get(&quota.group).copied().unwrap_or(0) >= quota.min_count)
}

fn try_improve(
    slices: &[SkillSlice],
    selected: &mut Vec<SkillSlice>,
    selected_ids: &mut HashSet<String>,
    constraints: &PackConstraints,
    mode: PackMode,
    max_per_group: usize,
    excluded: &HashSet<String>,
) -> bool {
    if selected.is_empty() {
        return false;
    }

    let mandatory_ids = collect_mandatory_ids(slices, constraints);
    let mut group_counts = count_groups(selected);
    let remaining_budget = constraints
        .budget
        .saturating_sub(selected.iter().map(|s| s.token_estimate).sum::<usize>());

    let candidates: Vec<&SkillSlice> = slices
        .iter()
        .filter(|slice| !selected_ids.contains(&slice.id))
        .filter(|slice| !is_excluded(slice, excluded))
        .filter(|slice| deps_satisfied(slice, selected_ids))
        .collect();
    if candidates.is_empty() {
        return false;
    }

    let mut ranked = rank_by_density(
        &candidates,
        &group_counts,
        &constraints.recent_slice_ids,
        mode,
    );
    let candidate = ranked.remove(0);

    let mut removable: Vec<(f32, usize)> = selected
        .iter()
        .enumerate()
        .filter(|(_, slice)| !mandatory_ids.contains(&slice.id))
        .map(|(idx, slice)| (score_slice(slice, mode), idx))
        .collect();
    if removable.is_empty() {
        return false;
    }
    removable.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));
    let (_score, remove_idx) = removable[0];

    // Extract fields before removal to satisfy borrow checker
    let removed_id = selected[remove_idx].id.clone();
    let removed_tokens = selected[remove_idx].token_estimate;
    let removed_group = selected[remove_idx].coverage_group.clone();

    if selected
        .iter()
        .any(|slice| slice.requires.contains(&removed_id))
    {
        return false;
    }
    if candidate.requires.iter().any(|req| *req == removed_id) {
        return false;
    }

    let mut tokens_after = remaining_budget + removed_tokens;
    if candidate.token_estimate > tokens_after {
        return false;
    }
    if !can_add_slice(candidate, excluded, &group_counts, max_per_group) {
        return false;
    }

    tokens_after -= candidate.token_estimate;
    if tokens_after > constraints.budget {
        return false;
    }

    selected_ids.remove(&removed_id);
    selected.remove(remove_idx);
    if let Some(group) = &removed_group {
        if let Some(count) = group_counts.get_mut(group) {
            *count = count.saturating_sub(1);
        }
    }

    selected.push(candidate.clone());
    selected_ids.insert(candidate.id.clone());
    add_group_count(&mut group_counts, candidate);

    true
}

fn collect_mandatory_ids(slices: &[SkillSlice], constraints: &PackConstraints) -> HashSet<String> {
    collect_mandatory_slices(slices, constraints)
        .into_iter()
        .map(|slice| slice.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_slice(
        id: &str,
        slice_type: SliceType,
        tokens: usize,
        utility: f32,
        group: &str,
    ) -> SkillSlice {
        SkillSlice {
            id: id.to_string(),
            slice_type,
            token_estimate: tokens,
            utility_score: utility,
            coverage_group: Some(group.to_string()),
            tags: vec![],
            requires: vec![],
            condition: None,
            content: format!("slice {id}"),
        }
    }

    #[test]
    fn test_mandatory_policy_included() {
        let slices = vec![
            make_slice("policy-1", SliceType::Policy, 20, 0.2, "policy"),
            make_slice("rule-1", SliceType::Rule, 20, 0.9, "rules"),
        ];
        let mut constraints = PackConstraints::new(50, 2);
        constraints
            .mandatory_slices
            .push(MandatorySlice::ByPredicate(MandatoryPredicate::Always));
        let packer = ConstrainedPacker;
        let result = packer
            .pack(&slices, &constraints, PackMode::Balanced)
            .unwrap();
        let ids: HashSet<String> = result.slices.into_iter().map(|s| s.id).collect();
        assert!(ids.contains("policy-1"));
    }

    #[test]
    fn test_required_coverage_respected() {
        let slices = vec![
            make_slice("rule-1", SliceType::Rule, 20, 0.8, "rules"),
            make_slice("command-1", SliceType::Command, 20, 0.6, "commands"),
        ];
        let mut constraints = PackConstraints::new(40, 2);
        constraints.required_coverage.push(CoverageQuota {
            group: "commands".to_string(),
            min_count: 1,
        });
        let packer = ConstrainedPacker;
        let result = packer
            .pack(&slices, &constraints, PackMode::Balanced)
            .unwrap();
        assert!(result.slices.iter().any(|s| s.id == "command-1"));
    }

    #[test]
    fn test_max_per_group_enforced() {
        let slices = vec![
            make_slice("rule-1", SliceType::Rule, 10, 0.9, "rules"),
            make_slice("rule-2", SliceType::Rule, 10, 0.8, "rules"),
            make_slice("rule-3", SliceType::Rule, 10, 0.7, "rules"),
        ];
        let constraints = PackConstraints::new(50, 1);
        let packer = ConstrainedPacker;
        let result = packer
            .pack(&slices, &constraints, PackMode::UtilityFirst)
            .unwrap();
        let count = result
            .slices
            .iter()
            .filter(|slice| slice.coverage_group.as_deref() == Some("rules"))
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_mandatory_budget_failure() {
        let slices = vec![
            make_slice("policy-1", SliceType::Policy, 40, 0.2, "policy"),
            make_slice("policy-2", SliceType::Policy, 40, 0.2, "policy"),
        ];
        let mut constraints = PackConstraints::new(50, 2);
        constraints
            .mandatory_slices
            .push(MandatorySlice::ByPredicate(MandatoryPredicate::Always));
        let packer = ConstrainedPacker;
        let err = packer
            .pack(&slices, &constraints, PackMode::Balanced)
            .unwrap_err();
        match err {
            PackError::MandatorySliceOmitted { .. } => {}
            _ => panic!("Expected mandatory omission error"),
        }
    }

    #[test]
    fn test_dependencies_respected() {
        let mut dependent = make_slice("rule-2", SliceType::Rule, 10, 0.9, "rules");
        dependent.requires.push("rule-1".to_string());
        let slices = vec![
            make_slice("rule-1", SliceType::Rule, 100, 0.1, "rules"),
            dependent,
        ];
        let constraints = PackConstraints::new(30, 2);
        let packer = ConstrainedPacker;
        let result = packer
            .pack(&slices, &constraints, PackMode::UtilityFirst)
            .unwrap();
        assert!(!result.slices.iter().any(|s| s.id == "rule-2"));
    }
}
