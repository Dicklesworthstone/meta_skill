//! Multi-machine synchronization support.

pub mod config;
pub mod engine;
pub mod machine;
pub mod ru;
pub mod state;

pub use config::{
    ConflictStrategy, RemoteAuth, RemoteConfig, RemoteType, SyncConfig, SyncDirection,
    SyncSettings, validate_remote_name,
};
pub use engine::{SyncEngine, SyncOptions, SyncReport};
pub use machine::{MachineIdentity, MachineMetadata};
pub use ru::{
    RuClient, RuConflict, RuError, RuExitCode, RuRepoStatus, RuSyncOptions, RuSyncResult,
};
pub use state::{SkillSyncState, SkillSyncStatus, SyncState};
