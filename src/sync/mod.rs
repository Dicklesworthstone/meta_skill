//! Multi-machine synchronization support.

pub mod config;
pub mod engine;
pub mod machine;
pub mod state;

pub use config::{
    ConflictStrategy, RemoteConfig, RemoteType, SyncConfig, SyncDirection, SyncSettings,
};
pub use engine::{SyncEngine, SyncOptions, SyncReport};
pub use machine::{MachineIdentity, MachineMetadata};
pub use state::{SkillSyncState, SkillSyncStatus, SyncState};
