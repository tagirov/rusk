use chrono::NaiveDate;
use rusk::Task;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

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
pub fn create_test_task(id: u32, text: &str, done: bool) -> Task {
    Task {
        id,
        text: text.to_string(),
        date: None,
        done,
        priority: false, after: Vec::new(),
    }
}

#[allow(dead_code)]
pub fn create_test_task_with_date(id: u32, text: &str, done: bool, date: &str) -> Task {
    Task {
        id,
        text: text.to_string(),
        date: NaiveDate::parse_from_str(date, "%d-%m-%Y").ok(),
        done,
        priority: false, after: Vec::new(),
    }
}

#[allow(dead_code)]
pub fn create_test_task_with_priority(id: u32, text: &str, done: bool, priority: bool) -> Task {
    Task {
        id,
        text: text.to_string(),
        date: None,
        done,
        priority,
        after: Vec::new(),
    }
}

/// Private world for one test that spawns the `rusk` binary: its own temp
/// dir, database, HOME and config, so tests never share state with each
/// other, with a developer's debug runs or with a parallel `cargo test`.
///
/// Debug and test-mode binaries pin the database to
/// `$TMPDIR/rusk_debug/tasks.json` and ignore `RUSK_DB`; release binaries
/// honor `RUSK_DB`. The sandbox points both at the same private file. The
/// child is forced into test mode (`RUST_TEST_THREADS`), so a debug binary
/// neither seeds its demo tasks into an empty database nor prints the
/// database location.
#[allow(dead_code)]
pub struct Sandbox {
    root: tempfile::TempDir,
}

#[allow(dead_code)]
impl Sandbox {
    pub fn new() -> Self {
        let root = tempfile::tempdir().expect("failed to create a sandbox dir");
        std::fs::create_dir_all(root.path().join("rusk_debug")).unwrap();
        std::fs::create_dir_all(root.path().join("home")).unwrap();
        Self { root }
    }

    /// Sandbox with the given JSON as the database content.
    pub fn with_db(tasks_json: &str) -> Self {
        let sb = Self::new();
        sb.write_db(tasks_json);
        sb
    }

    pub fn path(&self) -> &Path {
        self.root.path()
    }

    pub fn db_path(&self) -> PathBuf {
        self.root.path().join("rusk_debug").join("tasks.json")
    }

    pub fn write_db(&self, tasks_json: &str) {
        std::fs::write(self.db_path(), tasks_json).unwrap();
    }

    /// Database content, empty string when the file does not exist.
    pub fn read_db(&self) -> String {
        std::fs::read_to_string(self.db_path()).unwrap_or_default()
    }

    /// `rusk` bound to this sandbox. The config system is disabled
    /// (`RUSK_CONFIG=""`) and colors are off; override per test as needed.
    pub fn cmd(&self) -> Command {
        let bin = require_rusk_bin().expect("rusk binary not found, run cargo build");
        let mut cmd = Command::new(bin);
        let home = self.root.path().join("home");
        cmd.env("TMPDIR", self.root.path())
            .env("TMP", self.root.path())
            .env("TEMP", self.root.path())
            .env("RUSK_DB", self.db_path())
            .env("RUSK_CONFIG", "")
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("RUST_TEST_THREADS", "1")
            .env("RUSK_NO_COLOR", "1")
            .env_remove("RUSK_SYNC_REMOTE")
            .env_remove("RUSK_SYNC_TOKEN")
            .env_remove("RUSK_DB_TOKEN")
            .current_dir(self.root.path());
        cmd
    }

    /// `cmd()` with a fake `ssh` first in PATH. The shim runs the remote
    /// command locally (`ssh <target> <command>` -> `sh -c <command>`), so
    /// an ssh location `user@host:/abs/path` addresses `/abs/path` on this
    /// machine. No sshd, no network.
    #[cfg(unix)]
    pub fn cmd_with_fake_ssh(&self) -> Command {
        self.cmd_with_ssh_shim("#!/bin/sh\nshift\nexec sh -c \"$1\"\n")
    }

    /// `cmd()` with an `ssh` of the caller's own first in PATH: a login
    /// shell that prints a banner, an ssh that cannot connect, anything a
    /// test needs the other side to be. The shim is written once per
    /// sandbox, so all calls of one test share it.
    #[cfg(unix)]
    pub fn cmd_with_ssh_shim(&self, script: &str) -> Command {
        use std::os::unix::fs::PermissionsExt;
        let bin_dir = self.root.path().join("fake-bin");
        let ssh = bin_dir.join("ssh");
        if !ssh.exists() {
            std::fs::create_dir_all(&bin_dir).unwrap();
            std::fs::write(&ssh, script).unwrap();
            std::fs::set_permissions(&ssh, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let path = match env::var_os("PATH") {
            Some(old) => {
                let mut dirs = vec![bin_dir];
                dirs.extend(env::split_paths(&old));
                env::join_paths(dirs).unwrap()
            }
            None => bin_dir.into_os_string(),
        };
        let mut cmd = self.cmd();
        cmd.env("PATH", path);
        cmd
    }
}
