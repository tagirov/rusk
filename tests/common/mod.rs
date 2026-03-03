use chrono::NaiveDate;
use rusk::Task;
use std::path::PathBuf;

/// Returns path to rusk binary for CLI integration tests.
/// Uses CARGO_BIN_EXE_rusk when set (by cargo test), otherwise builds path from CARGO_MANIFEST_DIR.
#[allow(dead_code)]
pub fn rusk_bin_path() -> PathBuf {
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_rusk") {
        return PathBuf::from(path);
    }
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .unwrap_or_else(|_| ".".to_string());
    let profile = std::env::var("CARGO_PROFILE").unwrap_or_else(|_| "debug".to_string());
    PathBuf::from(manifest_dir).join("target").join(profile).join("rusk")
}

/// Ensures rusk binary exists. Returns error with hint if not (e.g. run `cargo build` first).
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

// Helper function to create test tasks
#[allow(dead_code)]
pub fn create_test_task(id: u8, text: &str, done: bool) -> Task {
    Task {
        id,
        text: text.to_string(),
        date: None,
        done,
    }
}

// Helper function to create test tasks with date
#[allow(dead_code)]
pub fn create_test_task_with_date(id: u8, text: &str, done: bool, date: &str) -> Task {
    Task {
        id,
        text: text.to_string(),
        date: NaiveDate::parse_from_str(date, "%d-%m-%Y").ok(),
        done,
    }
}
