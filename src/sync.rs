//! `rusk sync`: whole-database synchronization with a remote.
//!
//! Transports (chosen by the shape of `sync_remote`) are the remote storage
//! backends themselves: `user@host:/path/tasks.json` uses the ssh backend
//! (system `ssh`; pull is a remote `cat`, push is an atomic temp+`mv`
//! replace), `http(s)://host[:port]` uses the http backend (the API of a
//! running `rusk serve` via the system `curl`, optional Bearer token from
//! `sync_token`). The value is read exactly like `rusk_db` (see
//! [`crate::location`]), except that it cannot be a local path.
//!
//! Conflict safety without merge machinery: a state file next to the
//! database stores the hash of the last synced content. Comparing
//! local/remote/base classifies the situation as in-sync, fast-forward
//! (one side changed), or diverged (both changed — push/pull need --force).
//! Hashes are taken over the canonical serialization (parsed tasks
//! re-encoded as compact JSON), so the pretty-printed local file and the
//! compact HTTP body compare equal, as do CSV-backed databases.
//!
//! Both sides are read first and written later. A side that another writer
//! changes in between is not overwritten: the backends refuse to replace
//! what they did not read (`StaleDatabase`), and the sync is simply run
//! again.

use crate::backend::{Backend, Loaded, http::HttpBackend, ssh::SshBackend};
use crate::config::theme;
use crate::location::Location;
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
    /// A sync remote is a location (see [`crate::location`]) that is not
    /// on this machine.
    /// The token is asked for only when the remote is a server.
    fn parse(s: &str, token: impl FnOnce() -> Result<Option<String>>) -> Result<Self> {
        let location = Location::parse(s).with_context(|| {
            format!("invalid sync remote '{}'", crate::printable::escape(s))
        })?;
        match location {
            Location::Http(url) => Ok(Remote::Http(HttpBackend::new(&url, token()?))),
            Location::Ssh(ssh) => Ok(Remote::Ssh(SshBackend::new(&ssh)?)),
            Location::Local(_) => bail!(
                "invalid sync remote '{}': expected `user@host:/path/tasks.json` (ssh) \
                 or `https://host` (rusk serve API)",
                crate::printable::escape(s)
            ),
        }
    }

    fn describe(&self) -> String {
        match self {
            Remote::Ssh(b) => b.describe(),
            Remote::Http(b) => b.describe(),
        }
    }

    fn fetch(&self) -> Result<Loaded> {
        match self {
            Remote::Ssh(b) => b.read(),
            // A server that answers holds a database; one that does not is
            // an error, never an absence.
            Remote::Http(b) => Ok(Loaded {
                tasks: b.load()?,
                missing: false,
            }),
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

/// What one side holds, as far as this matters for the guard below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    /// There is no database at that location at all — it was never
    /// created, or the file is gone. (A database that exists but cannot be
    /// read never gets here: the backends make that an error.)
    Missing,
    /// A database holding no tasks.
    Empty,
    Tasks(usize),
}

impl Side {
    fn of(db: &Loaded) -> Self {
        match (db.missing, db.tasks.len()) {
            (true, _) => Side::Missing,
            (false, 0) => Side::Empty,
            (false, count) => Side::Tasks(count),
        }
    }

    fn has_tasks(self) -> bool {
        matches!(self, Side::Tasks(_))
    }

    /// How the side reads in the guard's message, after `what` names it.
    fn describe(self, what: &str) -> String {
        match self {
            Side::Missing => format!("{what} does not exist"),
            Side::Empty => format!("{what} has no tasks"),
            Side::Tasks(count) => format!("{what} has {count} task(s)"),
        }
    }
}

/// A fast-forward that would replace a side holding tasks with zero tasks.
///
/// Both reasons for the zero need an explicit `--force`: a database file
/// that is not there is no instruction to delete anything, and "the user
/// emptied it" is a change too big to propagate on a guess.
#[derive(Debug, PartialEq, Eq)]
enum Wipe {
    /// Pushing an empty local database over a remote holding tasks.
    Remote,
    /// Pulling an empty remote over a local database holding tasks.
    Local,
}

fn wipe_guard(status: &SyncStatus, local: Side, remote: Side) -> Option<Wipe> {
    match status {
        SyncStatus::LocalAhead if !local.has_tasks() && remote.has_tasks() => Some(Wipe::Remote),
        SyncStatus::RemoteAhead if !remote.has_tasks() && local.has_tasks() => Some(Wipe::Local),
        _ => None,
    }
}

/// Stable across formats and transports, see [`crate::revision`].
fn canonical_hash(tasks: &[Task]) -> Result<String> {
    crate::revision::list_revision(tasks)
}

fn state_path(db_path: &Path) -> PathBuf {
    crate::backend::aux_path(db_path, "sync")
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
        let _ = crate::atomic::replace_aux_file(
            state_path,
            json.as_bytes(),
            crate::atomic::Mode::Keep,
        );
    }
}

pub fn run(direction: Direction) -> Result<()> {
    let config = crate::config::config();
    let remote_str = crate::config::env_or_config("RUSK_SYNC_REMOTE", &config.sync_remote)?
        .context(
            "no sync remote configured: set `sync_remote` in the config file \
             or the RUSK_SYNC_REMOTE environment variable",
        )?;
    let remote = Remote::parse(&remote_str, || {
        crate::config::env_or_config("RUSK_SYNC_TOKEN", &config.sync_token)
    })?;

    let local = Backend::resolve()?;
    let Some(db_path) = local.local_path().map(Path::to_path_buf) else {
        bail!(
            "`rusk sync` needs a local database, but the database itself is remote ({}); \
             remote databases have nothing to sync",
            local.describe()
        );
    };
    let local_db = local.read()?;
    let remote_db = remote.fetch()?;
    let (local_side, remote_side) = (Side::of(&local_db), Side::of(&remote_db));
    let (local_tasks, remote_tasks) = (local_db.tasks, remote_db.tasks);

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

    let forced = matches!(
        direction,
        Direction::Push { force: true } | Direction::Pull { force: true }
    );
    if !forced {
        match wipe_guard(&status, local_side, remote_side) {
            Some(Wipe::Remote) => bail!(
                "{}, while {}; refusing to empty the remote. Use `rusk sync pull --force` to \
                 restore the local database from the remote, or `rusk sync push --force` to \
                 really empty the remote",
                local_side.describe(&format!("the local database at {}", db_path.display())),
                remote_side.describe(&remote.describe())
            ),
            Some(Wipe::Local) => bail!(
                "{}, while {}; refusing to empty the local database. Use `rusk sync push --force` \
                 to restore the remote from the local database, or `rusk sync pull --force` to \
                 really empty the local database",
                remote_side.describe(&remote.describe()),
                local_side.describe(&format!("the local database at {}", db_path.display()))
            ),
            None => {}
        }
    }

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
            Remote::parse("user@vps:/srv/tasks/tasks.json", || Ok(None)).unwrap(),
            Remote::Ssh(_)
        ));
        // Bare host (no user@) is accepted for sync remotes.
        assert!(matches!(
            Remote::parse("vps:/srv/tasks/tasks.json", || Ok(None)).unwrap(),
            Remote::Ssh(_)
        ));
        let http = Remote::parse("https://tasks.example.com/", || Ok(None)).unwrap();
        assert!(matches!(http, Remote::Http(_)));
        assert_eq!(http.describe(), "https://tasks.example.com");
        assert!(Remote::parse("just-a-host", || Ok(None)).is_err());
        assert!(Remote::parse(":/path", || Ok(None)).is_err());
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
    fn wipe_guard_blocks_only_emptying_fast_forwards() {
        use Side::{Empty, Missing, Tasks};
        use SyncStatus::*;
        // A local database with nothing in it about to be pushed over a
        // remote that holds tasks — emptied or never created, both are
        // refused.
        assert_eq!(wipe_guard(&LocalAhead, Empty, Tasks(2)), Some(Wipe::Remote));
        assert_eq!(wipe_guard(&LocalAhead, Missing, Tasks(2)), Some(Wipe::Remote));
        // The same the other way round.
        assert_eq!(wipe_guard(&RemoteAhead, Tasks(2), Empty), Some(Wipe::Local));
        assert_eq!(wipe_guard(&RemoteAhead, Tasks(2), Missing), Some(Wipe::Local));
        // Seeding a side that holds nothing is always fine.
        assert_eq!(wipe_guard(&LocalAhead, Tasks(1), Missing), None);
        assert_eq!(wipe_guard(&RemoteAhead, Missing, Tasks(1)), None);
        // Ordinary fast-forwards and the other states are untouched.
        assert_eq!(wipe_guard(&LocalAhead, Tasks(1), Tasks(2)), None);
        assert_eq!(wipe_guard(&RemoteAhead, Tasks(1), Tasks(2)), None);
        assert_eq!(wipe_guard(&InSync, Empty, Empty), None);
        assert_eq!(wipe_guard(&Diverged, Missing, Tasks(1)), None);
    }

    #[test]
    fn a_side_says_which_kind_of_nothing_it_holds() {
        let side = |missing, count: usize| {
            Side::of(&Loaded {
                tasks: (0..count)
                    .map(|i| Task {
                        id: i as u32 + 1,
                        text: "t".into(),
                        date: None,
                        done: false,
                        priority: false,
                        after: Vec::new(),
                    })
                    .collect(),
                missing,
            })
        };
        assert_eq!(side(true, 0), Side::Missing);
        assert_eq!(side(false, 0), Side::Empty);
        assert_eq!(side(false, 2), Side::Tasks(2));
        assert_eq!(side(true, 0).describe("the local database"), "the local database does not exist");
        assert_eq!(side(false, 0).describe("the remote"), "the remote has no tasks");
        assert_eq!(side(false, 2).describe("the remote"), "the remote has 2 task(s)");
    }

    #[test]
    fn canonical_hash_is_format_independent() {
        let tasks = vec![Task {
            id: 1,
            text: "hello".into(),
            date: chrono::NaiveDate::from_ymd_opt(2026, 7, 8),
            done: false,
            priority: true, after: Vec::new(),
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
