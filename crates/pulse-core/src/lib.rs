//! Pulse core: domain models, SQLite store, config, paths, IPC, and task state machine.

pub mod checkin;
pub mod config;
pub mod db;
pub mod dedup;
pub mod error;
pub mod export;
pub mod ipc;
pub mod models;
pub mod paths;
pub mod state;
pub mod store;

pub use checkin::{apply_checkin_answer, parse_answer_input};
pub use config::{
    load as load_config, parse_str as parse_config_str, write_atomic as write_config, Config,
};
pub use db::{open as open_db, open_in_memory, LATEST_SCHEMA_VERSION};
pub use dedup::{compute_dedup_key, normalize_title};
pub use error::{PulseError, Result};
pub use export::{export_history, ExportFormat};
pub use ipc::{
    live_service_pid, try_connect, write_pid_file, IpcClient, RpcHandler, ServicePidFile,
};
pub use models::*;
pub use paths::{default_data_dir, PulsePaths};
pub use state::{can_transition, validate_transition};
pub use store::Store;
