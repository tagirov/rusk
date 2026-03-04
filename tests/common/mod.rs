use chrono::NaiveDate;
use rusk::Task;
use std::path::PathBuf;

/// Returns path to rusk binary for CLI integration tests.
/// Uses CARGO_BIN_EXE_rusk when set (by cargo test), otherwise builds path from CARGO_MANIFEST_DIR.
#[allow(dead_code)]
pub fn rusk_bin_path() -> PathBuf {
    // First: CARGO_BIN_EXE_rusk is set by cargo test when running tests
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_rusk") {
        return PathBuf::from(path);
    }

    // Second: try to find from current exe path (test binary location)
    // Test binary is in target/debug/deps/ or target/debug/integration_tests/
    // rusk binary is in target/debug/ or target/release/
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(target_dir) = find_target_dir(&current_exe) {
            let profile = std::env::var("CARGO_PROFILE").unwrap_or_else(|_| "debug".to_string());
            let rusk_path = target_dir.join(&profile).join("rusk");
            if rusk_path.exists() {
                return rusk_path;
            }
        }
    }

    // Third: fallback to CARGO_MANIFEST_DIR
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .unwrap_or_else(|_| ".".to_string());
    let profile = std::env::var("CARGO_PROFILE").unwrap_or_else(|_| "debug".to_string());
    PathBuf::from(manifest_dir).join("target").join(profile).join("rusk")
}

/// Find the target directory from a given path by looking for "target" in ancestors.
fn find_target_dir(path: &std::path::Path) -> Option<PathBuf> {
    let mut current = path.parent()?;
    loop {
        if current.file_name()?.to_str()? == "target" {
            return Some(current.to_path_buf());
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => break,
        }
    }
    None
}

/// Ensures rusk binary exists. Returns error with hint if not (e.g. run `cargo build` first).
#[allow(dead_code)]
pub fn require_rusk_bin() -> anyhow::Result<PathBuf> {
    let path = rusk_bin_path();
    if path.exists() {
        return Ok(path);
    }

    // Try alternative locations if the primary path doesn't work
    let alternatives = alternative_rusk_paths();
    for alt_path in alternatives {
        if alt_path.exists() {
            return Ok(alt_path);
        }
    }

    anyhow::bail!(
        "rusk binary not found at {}. Run `cargo build` before integration tests.",
        path.display()
    )
}

/// Returns list of alternative paths to check for the rusk binary.
fn alternative_rusk_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // Try from current working directory
    if let Ok(cwd) = std::env::current_dir() {
        let profile = std::env::var("CARGO_PROFILE").unwrap_or_else(|_| "debug".to_string());
        paths.push(cwd.join("target").join(&profile).join("rusk"));
    }

    // Try from exe path's target directory
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(target_dir) = find_target_dir(&current_exe) {
            let profile = std::env::var("CARGO_PROFILE").unwrap_or_else(|_| "debug".to_string());
            paths.push(target_dir.join(&profile).join("rusk"));
        }
    }

    paths
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
