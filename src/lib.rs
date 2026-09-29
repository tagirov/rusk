pub mod args;
pub mod atomic;
pub mod backend;
#[cfg(any(feature = "interactive", feature = "backend-http", feature = "backend-ssh"))]
mod base64;
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
mod scratch;
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

/// Set by `rusk serve` to a number of its own. Each request it makes to
/// reach its database names it (see `transport::http_request`), so a server
/// that is its own database — `rusk_db` names the address it serves on —
/// is told by the first request instead of waiting on itself.
#[cfg(any(feature = "web", feature = "backend-http", feature = "backend-ssh"))]
pub(crate) static SERVE_ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();

/// The header that carries [`SERVE_ID`], and [`SERVE_VIA`] after it.
#[cfg(any(feature = "web", feature = "backend-http", feature = "backend-ssh"))]
pub(crate) const SERVE_ID_HEADER: &str = "X-Rusk-Serve";

#[cfg(any(feature = "web", feature = "backend-http", feature = "backend-ssh"))]
thread_local! {
    /// The servers the request this thread answers came through: the ids
    /// of its [`SERVE_ID_HEADER`]. A request this thread makes passes them
    /// on after its own, so that two servers that are each other's
    /// database are told too (review of R28).
    pub(crate) static SERVE_VIA: std::cell::RefCell<Vec<String>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// Set by the `rusk` command when it starts (see [`is_test_mode`]).
static COMMAND: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Tells the library it runs as the `rusk` command, not in a test harness.
#[doc(hidden)]
pub fn running_as_the_command() {
    COMMAND.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// True when running under a test harness (cargo test env vars or a test
/// binary name). Debug/test runs must not touch the user's real config file,
/// mirroring the database-path isolation in `Backend::resolve`.
///
/// The `rusk` command itself built for release never is: one that finds
/// `RUST_TEST_THREADS` in its environment, or is installed under a name
/// holding "test" (`rusk-latest`), is still the user's rusk with the user's
/// database and config (REVIEW №12). The library in a test binary is, in
/// any profile: `cargo test --release` must not reach the user's database
/// either (review of R25).
pub(crate) fn is_test_mode() -> bool {
    if cfg!(test) {
        return true;
    }
    if !cfg!(debug_assertions) && COMMAND.load(std::sync::atomic::Ordering::Relaxed) {
        return false;
    }
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
    env_check || exe_check
}
pub use parser::{
    IdListError, is_cli_date_help_value, normalize_date_string, parse_cli_date,
    parse_cli_date_for_edit, parse_cli_date_with_base, parse_edit_args, parse_id_args,
    parse_id_list, split_leading_ids, validate_cli_date_edit_arg,
};
pub use storage::{MarkResult, TaskManager};
