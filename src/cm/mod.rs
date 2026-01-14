//! CM (cass-memory) integration.
//!
//! Wraps the `cm` CLI for retrieving playbook context and rules.

pub mod client;

pub use client::{CmClient, CmContext};
