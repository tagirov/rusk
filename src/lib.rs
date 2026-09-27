pub mod args;
pub mod atomic;
pub mod backend;
pub mod cli;
pub mod codec;
#[cfg(feature = "completions")]
pub mod completions;
pub mod config;
pub mod error;
mod lock;
pub mod location;
pub mod model;
pub mod output;
pub mod parser;
pub mod printable;
pub mod revision;
pub mod search;
pub mod storage;
pub mod width;
#[cfg(feature = "sync")]
pub mod sync;
#[cfg(any(feature = "backend-http", feature = "backend-ssh"))]
pub mod transport;
#[cfg(feature = "web")]
pub mod web;
pub mod windows_console;

pub use backend::{Backend, Loaded, StaleDatabase};
pub use config::{ColorValue, Config, Theme};
pub use model::{Task, TaskId};

/// True when running under a test harness (cargo test env vars or a test
/// binary name). Debug/test runs must not touch the user's real config file,
/// mirroring the database-path isolation in `TaskManager::resolve_db_path`.
pub(crate) fn is_test_mode() -> bool {
    let env_check = std::env::var("RUST_TEST_THREADS").is_ok()
        || std::env::var("CARGO_TEST").is_ok()
        || std::env::var("__CARGO_TEST_CHANNEL").is_ok();
    let exe_check = std::env::current_exe()
        .ok()
        .and_then(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .map(|s| s.contains("test"))
        })
        .unwrap_or(false);
    env_check || exe_check || cfg!(test)
}
pub use parser::{
    IdListError, is_cli_date_help_value, normalize_date_string, parse_cli_date,
    parse_cli_date_for_edit, parse_cli_date_with_base, parse_edit_args, parse_id_args,
    parse_id_list, split_leading_ids, validate_cli_date_edit_arg,
};
pub use storage::{MarkResult, TaskManager};
