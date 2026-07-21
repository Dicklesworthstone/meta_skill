mod agent_compat_tests;
mod agent_detection_tests;
mod auto_load_tests;
mod backup_tests;
mod beads_real_tests;
mod bundle_fixture_tests;
mod cli_tests;
mod composition_tests;
mod context_detection_tests;
mod db_state_tests;
mod error_handling_tests;
mod fixture;
mod fixture_tests;
mod migration_tests;
mod more_cli_tests;
mod output_format_tests;
mod security_tests;
mod skill_md_tests;
// The ubs_staged tests drive a POSIX shell stub, so the whole module is
// unix-only; gating the `mod` keeps its imports from tripping
// `-D unused-imports` on Windows.
#[cfg(unix)]
mod ubs_staged_tests;
mod workflow_tests;
