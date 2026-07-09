//! `rusk sync`: whole-database synchronization with a remote.
//!
//! Transports (chosen by the shape of `sync_remote`) are the remote storage
//! backends themselves: `user@host:/path/tasks.json` uses the ssh backend
//! (system `ssh`; pull is a remote `cat`, push is an atomic temp+`mv`
//! replace), `http(s)://host[:port]` uses the http backend (the API of a
//! running `rusk serve` via the system `curl`, optional Bearer token from
//! `sync_token`). Unlike `rusk_db`, the ssh form here also accepts a bare
//! `host:/path` without `user@`.
//!
//! Conflict safety without merge machinery: a state file next to the
//! database stores the hash of the last synced content. Comparing
//! local/remote/base classifies the situation as in-sync, fast-forward
//! (one side changed), or diverged (both changed — push/pull need --force).
//! Hashes are taken over the canonical serialization (parsed tasks
//! re-encoded as compact JSON), so the pretty-printed local file and the
//! compact HTTP body compare equal, as do CSV-backed databases.

use crate::backend::{Backend, http::HttpBackend, ssh::SshBackend};
use crate::config::theme;
use crate::Task;
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy)]
pub enum Direction {
    Auto,
    Push { force: bool },
    Pull { force: bool },
}

enum Remote {
    Ssh(SshBackend),
    Http(HttpBackend),
}

impl Remote {
    fn parse(s: &str, token: Option<String>) -> Result<Self> {
        if s.starts_with("http://") || s.starts_with("https://") {
            return Ok(Remote::Http(HttpBackend::new(s, token)));
        }
        if let Some((target, path)) = s.split_once(':')
            && !target.is_empty()
            && !path.is_empty()
        {
            return Ok(Remote::Ssh(SshBackend::new(target, path)?));
        }
        bail!(
            "invalid sync remote '{s}': expected `user@host:/path/tasks.json` (ssh) \
             or `https://host` (rusk serve API)"
        );
    }

    fn describe(&self) -> String {
        match self {
            Remote::Ssh(b) => b.describe(),
            Remote::Http(b) => b.describe(),
        }
    }

    fn fetch(&self) -> Result<Vec<Task>> {
        match self {
            Remote::Ssh(b) => b.load(),
            Remote::Http(b) => b.load(),
        }
    }

    fn push(&self, tasks: &[Task]) -> Result<()> {
        match self {
            Remote::Ssh(b) => b.save(tasks),
            Remote::Http(b) => b.save(tasks),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum SyncStatus {
    InSync,
    LocalAhead,
    RemoteAhead,
    Diverged,
}

/// Classifies against the last synced hash. `base` is `None` on first sync.
fn decide(base: Option<&str>, local: &str, remote: &str, remote_empty: bool) -> SyncStatus {
    if local == remote {
        return SyncStatus::InSync;
    }
    match base {
        Some(b) if b == remote => SyncStatus::LocalAhead,
        Some(b) if b == local => SyncStatus::RemoteAhead,
        Some(_) => SyncStatus::Diverged,
        // No sync history: an empty remote is safe to seed; anything else
        // is unknown territory and needs an explicit direction.
        None if remote_empty => SyncStatus::LocalAhead,
        None => SyncStatus::Diverged,
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Stable across formats: tasks re-encoded as compact JSON, then FNV-1a 64.
fn canonical_hash(tasks: &[Task]) -> Result<String> {
    let json = serde_json::to_string(tasks).context("Failed to serialize tasks")?;
    Ok(format!("{:016x}", fnv1a(json.as_bytes())))
}

fn state_path(db_path: &Path) -> PathBuf {
    let ext = db_path.extension().and_then(|e| e.to_str()).unwrap_or("json");
    db_path.with_extension(format!("{ext}.sync"))
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SyncState {
    remote: String,
    hash: String,
}

/// Last-synced hash, only when it was recorded for the same remote.
fn read_base(state_path: &Path, remote: &str) -> Option<String> {
    let data = std::fs::read_to_string(state_path).ok()?;
    let state: SyncState = serde_json::from_str(&data).ok()?;
    (state.remote == remote).then_some(state.hash)
}

fn write_base(state_path: &Path, remote: &str, hash: &str) {
    let state = SyncState {
        remote: remote.to_string(),
        hash: hash.to_string(),
    };
    if let Ok(json) = serde_json::to_string(&state) {
        let _ = std::fs::write(state_path, json);
    }
}

pub fn run(direction: Direction) -> Result<()> {
    let config = crate::config::config();
    let remote_str = crate::config::env_or_config("RUSK_SYNC_REMOTE", &config.sync_remote)
        .context(
            "no sync remote configured: set `sync_remote` in the config file \
             or the RUSK_SYNC_REMOTE environment variable",
        )?;
    let token = crate::config::env_or_config("RUSK_SYNC_TOKEN", &config.sync_token);
    let remote = Remote::parse(&remote_str, token)?;

    let local = Backend::resolve()?;
    let Some(db_path) = local.local_path().map(Path::to_path_buf) else {
        bail!(
            "`rusk sync` needs a local database, but the database itself is remote ({}); \
             remote databases have nothing to sync",
            local.describe()
        );
    };
    let local_tasks = local.load()?;
    let remote_tasks = remote.fetch()?;

    let local_hash = canonical_hash(&local_tasks)?;
    let remote_hash = canonical_hash(&remote_tasks)?;
    let state_file = state_path(&db_path);
    let base = read_base(&state_file, &remote_str);

    let status = decide(
        base.as_deref(),
        &local_hash,
        &remote_hash,
        remote_tasks.is_empty(),
    );

    let push = |tasks: &[Task]| -> Result<()> {
        remote.push(tasks)?;
        write_base(&state_file, &remote_str, &canonical_hash(tasks)?);
        println!(
            "{} {} task(s) to {}",
            theme().success.paint("Pushed"),
            tasks.len(),
            remote.describe()
        );
        Ok(())
    };
    let pull = |tasks: Vec<Task>| -> Result<()> {
        let count = tasks.len();
        local.save(&tasks)?;
        write_base(&state_file, &remote_str, &canonical_hash(&tasks)?);
        println!(
            "{} {} task(s) from {}",
            theme().success.paint("Pulled"),
            count,
            remote.describe()
        );
        Ok(())
    };
    let already_in_sync = || {
        println!(
            "{} with {}",
            theme().success.paint("Already in sync"),
            remote.describe()
        );
    };

    match (direction, status) {
        (_, SyncStatus::InSync) => {
            // Record the base so a later divergence is detected even if the
            // first contact happened with identical content.
            write_base(&state_file, &remote_str, &local_hash);
            already_in_sync();
            Ok(())
        }
        (Direction::Auto, SyncStatus::LocalAhead) => push(&local_tasks),
        (Direction::Auto, SyncStatus::RemoteAhead) => pull(remote_tasks),
        (Direction::Auto, SyncStatus::Diverged) => bail!(
            "local and remote have both changed since the last sync; \
             resolve with `rusk sync push --force` (keep local) \
             or `rusk sync pull --force` (keep remote)"
        ),
        (Direction::Push { force: true }, _) => push(&local_tasks),
        (Direction::Push { force: false }, SyncStatus::LocalAhead) => push(&local_tasks),
        (Direction::Push { force: false }, _) => bail!(
            "the remote has changes not present locally; \
             use `rusk sync pull` first or `rusk sync push --force` to overwrite them"
        ),
        (Direction::Pull { force: true }, _) => pull(remote_tasks),
        (Direction::Pull { force: false }, SyncStatus::RemoteAhead) => pull(remote_tasks),
        (Direction::Pull { force: false }, _) => bail!(
            "the local database has changes not present on the remote; \
             use `rusk sync push` first or `rusk sync pull --force` to discard them"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_parsing() {
        assert!(matches!(
            Remote::parse("user@vps:/srv/tasks/tasks.json", None).unwrap(),
            Remote::Ssh(_)
        ));
        // Bare host (no user@) is accepted for sync remotes.
        assert!(matches!(
            Remote::parse("vps:/srv/tasks/tasks.json", None).unwrap(),
            Remote::Ssh(_)
        ));
        let http = Remote::parse("https://tasks.example.com/", None).unwrap();
        assert!(matches!(http, Remote::Http(_)));
        assert_eq!(http.describe(), "https://tasks.example.com");
        assert!(Remote::parse("just-a-host", None).is_err());
        assert!(Remote::parse(":/path", None).is_err());
    }

    #[test]
    fn decide_matrix() {
        // Identical content is always in sync.
        assert_eq!(decide(None, "a", "a", false), SyncStatus::InSync);
        assert_eq!(decide(Some("x"), "a", "a", false), SyncStatus::InSync);
        // Fast-forwards.
        assert_eq!(decide(Some("r"), "l", "r", false), SyncStatus::LocalAhead);
        assert_eq!(decide(Some("l"), "l", "r", false), SyncStatus::RemoteAhead);
        // Both sides moved.
        assert_eq!(decide(Some("b"), "l", "r", false), SyncStatus::Diverged);
        // First sync: empty remote is seedable, non-empty needs a decision.
        assert_eq!(decide(None, "l", "r", true), SyncStatus::LocalAhead);
        assert_eq!(decide(None, "l", "r", false), SyncStatus::Diverged);
    }

    #[test]
    fn canonical_hash_is_format_independent() {
        let tasks = vec![Task {
            id: 1,
            text: "hello".into(),
            date: chrono::NaiveDate::from_ymd_opt(2026, 7, 8),
            done: false,
            priority: true,
        }];
        // Pretty JSON on disk and compact JSON over HTTP hash identically
        // because hashing re-encodes parsed tasks canonically.
        let pretty: Vec<Task> =
            serde_json::from_str(&serde_json::to_string_pretty(&tasks).unwrap()).unwrap();
        assert_eq!(
            canonical_hash(&tasks).unwrap(),
            canonical_hash(&pretty).unwrap()
        );
        assert_ne!(
            canonical_hash(&tasks).unwrap(),
            canonical_hash(&[]).unwrap()
        );
    }

    #[test]
    fn state_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("tasks.json");
        let sp = state_path(&db);
        assert!(sp.to_string_lossy().ends_with("tasks.json.sync"));
        assert_eq!(read_base(&sp, "r"), None);
        write_base(&sp, "user@h:/p", "abc");
        assert_eq!(read_base(&sp, "user@h:/p"), Some("abc".to_string()));
        // A different remote invalidates the recorded base.
        assert_eq!(read_base(&sp, "user@other:/p"), None);
    }
}
