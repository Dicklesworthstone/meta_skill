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
    #[must_use] 
    pub const fn new(budget: usize, max_per_group: usize) -> Self {
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
        let contract = constraints.contract.as_ref();
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
                score_slice_with_contract(b, mode, contract)
                    .partial_cmp(&score_slice_with_contract(a, mode, contract))
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
                .collect();
            let ranked = rank_by_density(
                &group_slices,
                &group_counts,
                &constraints.recent_slice_ids,
                mode,
                contract,
            );
            for slice in ranked {
                if count >= quota.min_count {
                    break;
                }
                if slice.token_estimate > remaining {
                    continue;
                }
                if !deps_satisfied(slice, &selected_ids) {
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
            .collect();
        let ranked = rank_by_density(
            &candidates,
            &group_counts,
            &constraints.recent_slice_ids,
            mode,
            contract,
        );
        for slice in ranked {
            if slice.token_estimate > remaining {
                continue;
            }
            if !deps_satisfied(slice, &selected_ids) {
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
        .is_some_and(|group| excluded.contains(&group.to_lowercase()))
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
    contract: Option<&PackContract>,
) -> Vec<&'a SkillSlice> {
    let recent: HashSet<&str> = recent_slice_ids.iter().map(std::string::String::as_str).collect();
    let mut scored: Vec<(f32, &'a SkillSlice)> = slices
        .iter()
        .map(|slice| {
            let base = score_slice_with_contract(slice, mode, contract);
            let density = base / slice.token_estimate.max(1) as f32;
            let group_penalty = slice
                .coverage_group
                .as_ref()
                .and_then(|group| group_counts.get(group))
                .map_or(1.0, |count| 0.8_f32.powi(*count as i32));
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
        // Balanced: uniform small boosts to encourage even type distribution
        PackMode::Balanced => match slice.slice_type {
            SliceType::Rule => slice.utility_score + 0.08,
            SliceType::Command => slice.utility_score + 0.08,
            SliceType::Example => slice.utility_score + 0.08,
            SliceType::Pitfall => slice.utility_score + 0.08,
            SliceType::Overview => slice.utility_score + 0.05,
            _ => slice.utility_score,
        },
        // UtilityFirst: pure utility score, no type bonuses
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

fn score_slice_with_contract(
    slice: &SkillSlice,
    mode: PackMode,
    contract: Option<&PackContract>,
) -> f32 {
    let base = score_slice(slice, mode);
    base * contract_weight(slice, contract)
}

fn contract_weight(slice: &SkillSlice, contract: Option<&PackContract>) -> f32 {
    let Some(contract) = contract else {
        return 1.0;
    };
    let mut weight = 1.0;

    if let Some(group_weights) = &contract.group_weights {
        if let Some(group) = &slice.coverage_group {
            let key = group.to_lowercase();
            if let Some(value) = group_weights.get(&key) {
                weight *= *value;
            }
        }
    }

    if let Some(tag_weights) = &contract.tag_weights {
        let mut best = 1.0;
        for tag in &slice.tags {
            if let Some(value) = tag_weights.get(&tag.to_lowercase()) {
                if *value > best {
                    best = *value;
                }
            }
        }
        weight *= best;
    }

    weight.max(0.0)
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

    let ranked = rank_by_density(
        &candidates,
        &group_counts,
        &constraints.recent_slice_ids,
        mode,
        constraints.contract.as_ref(),
    );

    // Optimization: Compute removable scores once since they don't change relative to candidates.
    // We only swap once and return, so selected doesn't change during iteration.
    let mut removable: Vec<(f32, usize)> = selected
        .iter()
        .enumerate()
        .filter(|(_, slice)| !mandatory_ids.contains(&slice.id))
        .map(|(idx, slice)| {
            (
                score_slice_with_contract(slice, mode, constraints.contract.as_ref()),
                idx,
            )
        })
        .collect();

    if removable.is_empty() {
        return false;
    }
    removable.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(Ordering::Equal));

    for candidate in ranked {
        let candidate_score =
            score_slice_with_contract(candidate, mode, constraints.contract.as_ref());

        for (score, remove_idx) in &removable {
            // Optimization: removable is sorted by score (ascending).
            // If candidate_score <= *score, we don't gain utility.
            // And for all subsequent items, score will be even higher, so we can stop checking.
            if candidate_score <= *score {
                break;
            }

            let remove_idx = *remove_idx;
            // Extract fields before removal to satisfy borrow checker
            let removed_id = selected[remove_idx].id.clone();
            let removed_tokens = selected[remove_idx].token_estimate;
            let removed_group = selected[remove_idx].coverage_group.clone();

            if selected
                .iter()
                .any(|slice| slice.requires.contains(&removed_id))
            {
                continue;
            }
            if candidate.requires.contains(&removed_id) {
                continue;
            }

            let tokens_after = remaining_budget + removed_tokens;
            if candidate.token_estimate > tokens_after {
                continue;
            }

            // Update group counts BEFORE can_add_slice check to account for the removal.
            // This allows same-group swaps where the candidate replaces the removed slice.
            let mut temp_group_counts = group_counts.clone();
            if let Some(group) = &removed_group {
                if let Some(count) = temp_group_counts.get_mut(group) {
                    *count = count.saturating_sub(1);
                }
            }

            if !can_add_slice(candidate, excluded, &temp_group_counts, max_per_group) {
                continue;
            }

            selected_ids.remove(&removed_id);
            selected.remove(remove_idx);

            group_counts = temp_group_counts;
            selected.push(candidate.clone());
            selected_ids.insert(candidate.id.clone());
            add_group_count(&mut group_counts, candidate);

            return true;
        }
    }

    false
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
            section_title: None,
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
    fn test_contract_required_groups_enforced() {
        let slices = vec![
            make_slice("rule-1", SliceType::Rule, 20, 0.8, "rules"),
            make_slice("command-1", SliceType::Command, 20, 0.6, "commands"),
        ];
        let mut constraints = PackConstraints::new(40, 2);
        constraints.contract = Some(PackContract {
            id: "debug".to_string(),
            description: "debug".to_string(),
            required_groups: vec!["commands".to_string()],
            mandatory_slices: Vec::new(),
            max_per_group: None,
            group_weights: None,
            tag_weights: None,
        });
        let packer = ConstrainedPacker;
        let result = packer
            .pack(&slices, &constraints, PackMode::Balanced)
            .unwrap();
        assert!(result.slices.iter().any(|s| s.id == "command-1"));
    }

    #[test]
    fn test_contract_weights_influence_selection() {
        let slices = vec![
            make_slice("rule-1", SliceType::Rule, 10, 0.5, "rules"),
            make_slice("pitfall-1", SliceType::Pitfall, 10, 0.5, "pitfalls"),
        ];
        let mut weights = std::collections::HashMap::new();
        weights.insert("pitfalls".to_string(), 2.0);
        let mut constraints = PackConstraints::new(10, 2);
        constraints.contract = Some(PackContract {
            id: "debug".to_string(),
            description: "debug".to_string(),
            required_groups: Vec::new(),
            mandatory_slices: Vec::new(),
            max_per_group: None,
            group_weights: Some(weights),
            tag_weights: None,
        });
        let packer = ConstrainedPacker;
        let result = packer
            .pack(&slices, &constraints, PackMode::Balanced)
            .unwrap();
        assert!(result.slices.iter().any(|s| s.id == "pitfall-1"));
        assert!(!result.slices.iter().any(|s| s.id == "rule-1"));
    }

    #[test]
    fn test_contract_tag_weights_influence_selection() {
        let mut favored = make_slice("rule-1", SliceType::Rule, 10, 0.5, "rules");
        favored.tags.push("preferred".to_string());
        let other = make_slice("rule-2", SliceType::Rule, 10, 0.5, "rules");
        let slices = vec![favored, other];

        let mut tag_weights = std::collections::HashMap::new();
        tag_weights.insert("preferred".to_string(), 2.0);
        let mut constraints = PackConstraints::new(10, 2);
        constraints.contract = Some(PackContract {
            id: "tag".to_string(),
            description: "tag".to_string(),
            required_groups: Vec::new(),
            mandatory_slices: Vec::new(),
            max_per_group: None,
            group_weights: None,
            tag_weights: Some(tag_weights),
        });
        let packer = ConstrainedPacker;
        let result = packer
            .pack(&slices, &constraints, PackMode::Balanced)
            .unwrap();
        assert_eq!(result.slices.len(), 1);
        assert_eq!(result.slices[0].id, "rule-1");
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
    fn test_mandatory_budget_failure() -> Result<(), PackError> {
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
        if matches!(err, PackError::MandatorySliceOmitted { .. }) {
            Ok(())
        } else {
            Err(err)
        }
    }

    #[test]
    fn test_optimization_local_minimum_trap() {
        // Budget 100
        // Slices:
        // A (Worst): Score 0.1, Tokens 1 -> Fits
        // B (Victim): Score 20.0, Tokens 10 -> Fits
        // C (Candidate): Score 105.0, Tokens 99 -> Does not fit initially

        // Greedy steps:
        // 1. Pick B (Highest density 2.0). Rem 90.
        // 2. C needs 99. Skip.
        // 3. Pick A (Density 0.1). Rem 89.
        // Selected: [B, A].
        
        // Improve:
        // Candidate C (105u).
        // Removable: A (0.1), B (20.0).
        // Try remove A (1t). Freed 1 + Slack 89 = 90. C needs 99. Fail.
        // Try remove B (10t). Freed 10 + Slack 89 = 99. C needs 99. Success!
        // Swap B for C.
        // Result: A, C.

        let slices = vec![
            make_slice("A", SliceType::Example, 1, 0.1, "g1"),
            make_slice("B", SliceType::Rule, 10, 20.0, "g2"),
            make_slice("C", SliceType::Rule, 99, 105.0, "g3"),
        ];

        let constraints = PackConstraints::new(100, 1);
        let packer = ConstrainedPacker;
        
        let result = packer.pack(&slices, &constraints, PackMode::Balanced).unwrap();
        
        assert!(result.slices.iter().any(|s| s.id == "C"), "Optimization failed to swap B for C");
        assert!(!result.slices.iter().any(|s| s.id == "B"), "B should have been removed");
        assert!(result.slices.iter().any(|s| s.id == "A"), "A should remain");
    }

    #[test]
    fn test_improvement_iterates_candidates() {
        // Budget 12.
        // A (2t, 6s). D=3.
        // B (13t, 100s). D=7.6.
        // C (12t, 30s). D=2.5.
        
        // Greedy:
        // Ranked: B, A, C.
        // 1. B (13t). > 12. Skip.
        // 2. A (2t). <= 12. Sel [A]. Rem 10.
        // 3. C (12t). > 10. Skip.
        // Result [A]. Score 6.

        // Improvement:
        // Candidates: [B, C].
        // Top: B.
        // Removable: A (2t).
        // Avail: 10 + 2 = 12.
        // B needs 13. Fail.
        // BUG: Stops here.
        
        // Expected: Check C.
        // Swap A (2t) for C (12t)?
        // Avail: 12.
        // C needs 12. Fits.
        // C score 30 > A score 6. Swap!

        let slices = vec![
            make_slice("A", SliceType::Rule, 2, 6.0, "g1"),
            make_slice("B", SliceType::Rule, 13, 100.0, "g2"),
            make_slice("C", SliceType::Rule, 12, 30.0, "g3"),
        ];

        let constraints = PackConstraints::new(12, 1);
        let packer = ConstrainedPacker;

        let result = packer.pack(&slices, &constraints, PackMode::Balanced).unwrap();

        assert!(result.slices.iter().any(|s| s.id == "C"), "Should have swapped A for C");
        assert!(!result.slices.iter().any(|s| s.id == "A"), "A should be removed");
    }
}
