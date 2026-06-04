use chrono::NaiveDate;
use rusk::Task;
use std::env;
use std::path::{Path, PathBuf};

fn rusk_bin_name() -> String {
    format!("rusk{}", env::consts::EXE_SUFFIX)
}

fn push_unique(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.iter().any(|existing| existing == &path) {
        paths.push(path);
    }
}

fn push_profile_paths(paths: &mut Vec<PathBuf>, target_dir: &Path, profile: &str) {
    push_unique(
        paths,
        target_dir.join(profile).join(rusk_bin_name()),
    );
    if let Ok(triple) = env::var("HOST") {
        push_unique(
            paths,
            target_dir.join(triple).join(profile).join(rusk_bin_name()),
        );
    }
}

fn find_target_dir(path: &Path) -> Option<PathBuf> {
    let mut current = path.parent()?;
    loop {
        if current.file_name()?.to_str()? == "target" {
            return Some(current.to_path_buf());
        }
        current = current.parent()?;
    }
}

/// Candidate paths to the rusk binary, ordered from most to least reliable.
fn rusk_bin_candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let bin_name = rusk_bin_name();

    if let Some(path) = env::var("CARGO_BIN_EXE_rusk")
        .ok()
        .filter(|value| !value.is_empty())
    {
        push_unique(&mut paths, PathBuf::from(path));
    }

    // Ripgrep-style lookup: integration tests live in target/{profile}/deps/.
    if let Ok(current_exe) = env::current_exe() {
        if let Some(parent) = current_exe.parent() {
            push_unique(&mut paths, parent.join(format!("../{bin_name}")));
        }
        if let Some(target_dir) = find_target_dir(&current_exe) {
            for profile in ["debug", "release"] {
                push_profile_paths(&mut paths, &target_dir, profile);
            }
        }
    }

    push_unique(
        &mut paths,
        PathBuf::from(env!("CARGO_BIN_EXE_rusk")),
    );

    if let Ok(manifest_dir) = env::var("CARGO_MANIFEST_DIR") {
        let target_dir = PathBuf::from(manifest_dir).join("target");
        for profile in ["debug", "release"] {
            push_profile_paths(&mut paths, &target_dir, profile);
        }
    }

    if let Ok(cwd) = env::current_dir() {
        let target_dir = cwd.join("target");
        for profile in ["debug", "release"] {
            push_profile_paths(&mut paths, &target_dir, profile);
        }
    }

    paths
}

/// Returns path to rusk binary for CLI integration tests.
#[allow(dead_code)]
pub fn rusk_bin_path() -> PathBuf {
    rusk_bin_candidates()
        .into_iter()
        .find(|path| path.exists())
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_BIN_EXE_rusk")))
}

/// Ensures rusk binary exists.
#[allow(dead_code)]
pub fn require_rusk_bin() -> anyhow::Result<PathBuf> {
    if let Some(path) = rusk_bin_candidates().into_iter().find(|candidate| candidate.exists()) {
        return Ok(path);
    }

    let expected = env!("CARGO_BIN_EXE_rusk");
    anyhow::bail!(
        "rusk binary not found (checked {} locations, expected {}). Run `cargo build` before integration tests.",
        rusk_bin_candidates().len(),
        expected
    )
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
