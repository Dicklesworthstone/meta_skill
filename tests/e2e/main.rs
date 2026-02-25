//! E2E test suite entry point.

mod bundle_workflow;
mod cass_workflow;
#[path = "../common/mod.rs"]
mod common;
mod dedup_workflow;
mod doctor_workflow;
mod experiment_workflow;
mod fixture;
mod fresh_install;
mod graph_workflow;
mod import_workflow;
mod index_workflow;
mod layer_conflict;
mod list_workflow;
mod load_workflow;
mod mcp_workflow;
mod prune_workflow;
mod rich_output_workflow;
mod safety_workflow;
mod search_workflow;
mod security_workflow;
mod show_workflow;
mod skill_creation;
mod skill_discovery;
mod suggestions_workflow;
mod sync_workflow;
mod template_workflow;
