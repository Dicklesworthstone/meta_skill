//! Built-in pack contract presets.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use super::skill::PackContract;
use crate::error::{MsError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackContractPreset {
    Complete,
    Debug,
    Refactor,
    Learn,
    QuickRef,
    CodeGen,
}

impl PackContractPreset {
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Debug => "debug",
            Self::Refactor => "refactor",
            Self::Learn => "learn",
            Self::QuickRef => "quickref",
            Self::CodeGen => "codegen",
        }
    }

    #[must_use]
    pub fn contract(&self) -> PackContract {
        match self {
            Self::Complete => PackContract {
                id: "complete".to_string(),
                description: "Full coverage with default constraints.".to_string(),
                required_groups: Vec::new(),
                mandatory_slices: Vec::new(),
                max_per_group: None,
                group_weights: None,
                tag_weights: None,
            },
            Self::Debug => PackContract {
                id: "debug".to_string(),
                description: "Debug-first pack (pitfalls, rules, commands).".to_string(),
                required_groups: vec![
                    "pitfalls".to_string(),
                    "rules".to_string(),
                    "commands".to_string(),
                ],
                mandatory_slices: Vec::new(),
                max_per_group: Some(3),
                group_weights: Some(weight_map(&[
                    ("pitfalls", 2.0),
                    ("rules", 1.3),
                    ("checklists", 1.2),
                    ("commands", 1.1),
                    ("examples", 0.8),
                    ("overview", 0.6),
                    ("reference", 0.6),
                ])),
                tag_weights: Some(weight_map(&[("debug", 1.4), ("debugging", 1.4)])),
            },
            Self::Refactor => PackContract {
                id: "refactor".to_string(),
                description: "Refactor-focused pack (rules, examples, pitfalls).".to_string(),
                required_groups: vec![
                    "rules".to_string(),
                    "examples".to_string(),
                    "pitfalls".to_string(),
                ],
                mandatory_slices: Vec::new(),
                max_per_group: Some(3),
                group_weights: Some(weight_map(&[
                    ("examples", 1.6),
                    ("rules", 1.2),
                    ("commands", 1.1),
                    ("checklists", 1.0),
                    ("pitfalls", 0.9),
                    ("overview", 0.7),
                    ("reference", 0.6),
                ])),
                tag_weights: Some(weight_map(&[("refactor", 1.3)])),
            },
            Self::Learn => PackContract {
                id: "learn".to_string(),
                description: "Learning-focused pack (overview, examples).".to_string(),
                required_groups: vec!["overview".to_string(), "examples".to_string()],
                mandatory_slices: Vec::new(),
                max_per_group: Some(3),
                group_weights: Some(weight_map(&[
                    ("overview", 1.7),
                    ("examples", 1.5),
                    ("rules", 1.1),
                    ("checklists", 0.9),
                    ("pitfalls", 0.9),
                    ("reference", 0.8),
                ])),
                tag_weights: Some(weight_map(&[("learn", 1.2), ("learning", 1.2)])),
            },
            Self::QuickRef => PackContract {
                id: "quickref".to_string(),
                description: "Quick reference pack (overview + rules, tight budget).".to_string(),
                required_groups: vec!["overview".to_string(), "rules".to_string()],
                mandatory_slices: Vec::new(),
                max_per_group: Some(1),
                group_weights: Some(weight_map(&[
                    ("rules", 1.5),
                    ("commands", 1.4),
                    ("checklists", 1.2),
                    ("overview", 0.4),
                    ("examples", 0.5),
                    ("pitfalls", 0.8),
                    ("reference", 0.7),
                ])),
                tag_weights: None,
            },
            Self::CodeGen => PackContract {
                id: "codegen".to_string(),
                description: "Code generation pack (examples, commands, rules).".to_string(),
                required_groups: vec![
                    "examples".to_string(),
                    "commands".to_string(),
                    "rules".to_string(),
                ],
                mandatory_slices: Vec::new(),
                max_per_group: Some(2),
                group_weights: Some(weight_map(&[
                    ("examples", 1.7),
                    ("commands", 1.4),
                    ("rules", 1.2),
                    ("checklists", 1.0),
                    ("overview", 0.6),
                    ("pitfalls", 0.8),
                    ("reference", 0.6),
                ])),
                tag_weights: Some(weight_map(&[("codegen", 1.2), ("template", 1.2)])),
            },
        }
    }
}

#[must_use]
pub fn contract_from_name(name: &str) -> Option<PackContract> {
    let normalized = name.trim().to_lowercase();
    let preset = match normalized.as_str() {
        "complete" => PackContractPreset::Complete,
        "debug" => PackContractPreset::Debug,
        "refactor" => PackContractPreset::Refactor,
        "learn" => PackContractPreset::Learn,
        "quickref" | "quick-ref" | "quick_ref" => PackContractPreset::QuickRef,
        "codegen" | "code-gen" | "code_gen" => PackContractPreset::CodeGen,
        _ => return None,
    };
    Some(preset.contract())
}

fn weight_map(entries: &[(&str, f32)]) -> HashMap<String, f32> {
    entries
        .iter()
        .map(|(key, value)| (key.to_lowercase(), *value))
        .collect()
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct ContractStore {
    version: String,
    contracts: Vec<PackContract>,
}

impl Default for ContractStore {
    fn default() -> Self {
        Self {
            version: "1".to_string(),
            contracts: Vec::new(),
        }
    }
}

#[must_use]
pub fn custom_contracts_path(ms_root: &Path) -> PathBuf {
    ms_root.join("contracts.json")
}

#[must_use]
pub fn builtin_contracts() -> Vec<PackContract> {
    [
        PackContractPreset::Complete,
        PackContractPreset::Debug,
        PackContractPreset::Refactor,
        PackContractPreset::Learn,
        PackContractPreset::QuickRef,
        PackContractPreset::CodeGen,
    ]
    .iter()
    .map(PackContractPreset::contract)
    .collect()
}

pub fn load_custom_contracts(path: &Path) -> Result<Vec<PackContract>> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    let raw = std::fs::read_to_string(path)
        .map_err(|err| MsError::Config(format!("read contracts: {err}")))?;
    let value: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|err| MsError::Config(format!("parse contracts: {err}")))?;
    if value.is_array() {
        let contracts: Vec<PackContract> = serde_json::from_value(value)
            .map_err(|err| MsError::Config(format!("parse contracts: {err}")))?;
        return Ok(normalize_contracts(contracts));
    }
    let store: ContractStore = serde_json::from_value(value)
        .map_err(|err| MsError::Config(format!("parse contracts: {err}")))?;
    Ok(normalize_contracts(store.contracts))
}

pub fn save_custom_contracts(path: &Path, contracts: &[PackContract]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|err| MsError::Config(format!("create contracts dir: {err}")))?;
    }
    let store = ContractStore {
        version: "1".to_string(),
        contracts: normalize_contracts(contracts.to_vec()),
    };
    let payload = serde_json::to_string_pretty(&store)
        .map_err(|err| MsError::Config(format!("serialize contracts: {err}")))?;
    let temp_path = path.with_extension("tmp");
    std::fs::write(&temp_path, payload)
        .map_err(|err| MsError::Config(format!("write contracts: {err}")))?;
    match std::fs::rename(&temp_path, path) {
        Ok(()) => {}
        Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
            std::fs::remove_file(path)
                .map_err(|err| MsError::Config(format!("remove contracts: {err}")))?;
            if let Err(err) = std::fs::rename(&temp_path, path) {
                let _ = std::fs::remove_file(&temp_path);
                return Err(MsError::Config(format!("move contracts: {err}")));
            }
        }
        Err(err) => {
            let _ = std::fs::remove_file(&temp_path);
            return Err(MsError::Config(format!("move contracts: {err}")));
        }
    }
    Ok(())
}

pub fn add_custom_contract(path: &Path, mut contract: PackContract) -> Result<()> {
    contract = normalize_contract(contract)?;
    let mut contracts = load_custom_contracts(path)?;
    let existing: HashSet<String> = builtin_contracts()
        .into_iter()
        .map(|c| c.id.to_lowercase())
        .chain(contracts.iter().map(|c| c.id.to_lowercase()))
        .collect();
    if existing.contains(&contract.id.to_lowercase()) {
        return Err(MsError::ValidationFailed(format!(
            "contract id already exists: {}",
            contract.id
        )));
    }
    contracts.push(contract);
    save_custom_contracts(path, &contracts)
}

pub fn find_custom_contract(path: &Path, id: &str) -> Result<Option<PackContract>> {
    let id_norm = id.trim().to_lowercase();
    let contracts = load_custom_contracts(path)?;
    Ok(contracts
        .into_iter()
        .find(|contract| contract.id.to_lowercase() == id_norm))
}

fn normalize_contracts(contracts: Vec<PackContract>) -> Vec<PackContract> {
    contracts
        .into_iter()
        .filter_map(|c| normalize_contract(c).ok())
        .collect()
}

fn normalize_contract(mut contract: PackContract) -> Result<PackContract> {
    let id = contract.id.trim();
    if id.is_empty() {
        return Err(MsError::ValidationFailed(
            "contract id must be non-empty".to_string(),
        ));
    }
    contract.id = id.to_string();
    if !contract.required_groups.is_empty() {
        contract.required_groups = contract
            .required_groups
            .into_iter()
            .map(|group| group.trim().to_lowercase())
            .filter(|group| !group.is_empty())
            .collect();
    }
    if let Some(weights) = &mut contract.group_weights {
        *weights = normalize_weights(weights)?;
    }
    if let Some(weights) = &mut contract.tag_weights {
        *weights = normalize_weights(weights)?;
    }
    Ok(contract)
}

fn normalize_weights(weights: &HashMap<String, f32>) -> Result<HashMap<String, f32>> {
    let mut out = HashMap::new();
    for (key, value) in weights {
        if *value < 0.0 {
            return Err(MsError::ValidationFailed(format!(
                "weight must be >= 0 for {key}"
            )));
        }
        out.insert(key.to_lowercase(), *value);
    }
    Ok(out)
}
