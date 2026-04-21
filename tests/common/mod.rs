use chrono::NaiveDate;
use rusk::Task;
use std::path::PathBuf;

/// Returns path to rusk binary for CLI integration tests.
/// Uses compile-time CARGO_BIN_EXE_rusk set by cargo — works in Nix and any build env.
#[allow(dead_code)]
pub fn rusk_bin_path() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_rusk"))
}

/// Ensures rusk binary exists.
#[allow(dead_code)]
pub fn require_rusk_bin() -> anyhow::Result<PathBuf> {
    let path = rusk_bin_path();
    if path.exists() {
        Ok(path)
    } else {
        anyhow::bail!(
            "rusk binary not found at {}. Run `cargo build` before integration tests.",
            path.display()
        )
    }
}

#[allow(dead_code)]
pub fn create_test_task(id: u8, text: &str, done: bool) -> Task {
    Task {
        id,
        text: text.to_string(),
        date: None,
        done,
        priority: false,
    }
}

#[allow(dead_code)]
pub fn create_test_task_with_date(id: u8, text: &str, done: bool, date: &str) -> Task {
    Task {
        id,
        text: text.to_string(),
        date: NaiveDate::parse_from_str(date, "%d-%m-%Y").ok(),
        done,
        priority: false,
    }
}

#[allow(dead_code)]
pub fn create_test_task_with_priority(id: u8, text: &str, done: bool, priority: bool) -> Task {
    Task {
        id,
        text: text.to_string(),
        date: None,
        done,
        priority,
    }
}
