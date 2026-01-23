//! E2E test suite entry point.

mod bundle_workflow;
mod cass_workflow;
#[path = "../common/mod.rs"]
mod common;
mod fixture;
mod fresh_install;
mod layer_conflict;
mod rich_output_workflow;
mod safety_workflow;
mod search_workflow;
mod security_workflow;
mod skill_creation;
mod skill_discovery;
mod sync_workflow;
