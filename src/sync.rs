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
//! database records, for the remote it was synced with, the hash of what
//! each side held right after that sync — each side as it reads back, since
//! a format that cannot hold everything (todo.txt, Markdown) stores less
//! than it was sent (REVIEW №24). A side whose hash moved since has changed:
//! one side changed is a fast-forward, both is a divergence that needs
//! `--force`. Without a sync on record, a side holding no tasks is seeded
//! from the other; two sides holding different tasks need `--force`.
//! Hashes are taken over the canonical serialization (parsed tasks
//! re-encoded as compact JSON), so the pretty-printed local file and the
//! compact HTTP body compare equal, as do CSV-backed databases.
//!
//! Both sides are read first and written later. A side that another writer
//! changes in between is not overwritten: the backends refuse to replace
//! what they did not read (`StaleDatabase`), and the sync is simply run
//! again. `--force` reads only the side it copies from, so it can replace a
//! side that no longer reads at all (REVIEW №70).

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
            format!("invalid sync remote '{}'", crate::location::shown(s))
        })?;
        match location {
            Location::Http(url) => Ok(Remote::Http(HttpBackend::new(&url, token()?))),
            Location::Ssh(ssh) => Ok(Remote::Ssh(SshBackend::new(&ssh)?)),
            Location::Local(_) => bail!(
                "invalid sync remote '{}': expected `user@host:/path/tasks.json` (ssh) \
                 or `https://host` (rusk serve API)",
                crate::location::shown(s)
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

    /// Replaces the remote with `tasks`; what it holds then (see
    /// [`stored_form`](Self::stored_form)).
    fn push(&self, tasks: &[Task]) -> Result<Vec<Task>> {
        match self {
            Remote::Ssh(b) => {
                b.save(tasks)?;
                Ok(b.format().stored_form(tasks))
            }
            // The server says what its database made of the list; one that
            // does not (before R21) is taken to hold it as it is.
            Remote::Http(b) => Ok(b.save_held(tasks)?.unwrap_or_else(|| tasks.to_vec())),
        }
    }

    /// What the remote holds once `tasks` are pushed to it, worked out
    /// without pushing: a file over ssh in its format; a server, as far as
    /// can be known here, as they are.
    fn stored_form(&self, tasks: &[Task]) -> Vec<Task> {
        match self {
            Remote::Ssh(b) => b.format().stored_form(tasks),
            Remote::Http(_) => tasks.to_vec(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum SyncStatus {
    InSync,
    LocalAhead,
    RemoteAhead,
    /// Both sides changed since the last sync.
    Diverged,
    /// No sync on record, and both sides hold tasks, different ones.
    Unrelated,
}

/// What each side held right after the last sync with this remote.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Base {
    local: String,
    remote: String,
}

/// Classifies the two sides against the last sync (`base`, `None` when
/// there is none on record).
fn decide(base: Option<&Base>, local: &str, remote: &str, local_side: Side, remote_side: Side) -> SyncStatus {
    if local == remote {
        return SyncStatus::InSync;
    }
    match base {
        Some(base) => match (local != base.local, remote != base.remote) {
            // Each side as the last sync left it, only reading differently.
            (false, false) => SyncStatus::InSync,
            (true, false) => SyncStatus::LocalAhead,
            (false, true) => SyncStatus::RemoteAhead,
            (true, true) => SyncStatus::Diverged,
        },
        // No sync on record: a side holding no tasks is seeded from the
        // other, either way round (REVIEW №23); two sides holding
        // different tasks are for the user to choose between.
        None if !remote_side.has_tasks() => SyncStatus::LocalAhead,
        None if !local_side.has_tasks() => SyncStatus::RemoteAhead,
        None => SyncStatus::Unrelated,
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

/// The state file: the remote it is about, and what each side held after
/// the last sync with it. A file of an older rusk has one `hash` for both.
#[derive(serde::Serialize, serde::Deserialize)]
struct SyncState {
    remote: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    local_hash: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    remote_hash: Option<String>,
}

/// The remote a state file names, spelled the way `remote` describes
/// itself, so that `http://h:p/` and `http://h:p` are one remote (REVIEW
/// №58).
fn remote_key(value: &str) -> Option<String> {
    Remote::parse(value, || Ok(None)).ok().map(|remote| remote.describe())
}

/// What the state file says about the sync with `remote` (its key).
#[derive(Debug, PartialEq, Eq)]
enum History {
    Synced(Base),
    /// No state file: never synced from here.
    Never,
    /// A state file that cannot be used; why, for the messages.
    Unusable(String),
}

fn read_history(state_path: &Path, remote: &str) -> History {
    let data = match std::fs::read_to_string(state_path) {
        Ok(data) => data,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return History::Never,
        Err(e) => {
            return History::Unusable(format!(
                "the sync state '{}' cannot be read: {e}",
                state_path.display()
            ));
        }
    };
    let unreadable = || {
        History::Unusable(format!(
            "the sync state '{}' is not one rusk wrote",
            state_path.display()
        ))
    };
    let Ok(state) = serde_json::from_str::<SyncState>(&data) else {
        return unreadable();
    };
    // The key itself, or a spelling of the same remote (an older rusk wrote
    // it as typed). The key first: not every key reads back as a remote
    // (`user@host:~odd.json`, review of R21).
    if state.remote != remote && remote_key(&state.remote).as_deref() != Some(remote) {
        return History::Unusable(format!(
            "the sync state '{}' is for another remote, {}",
            state_path.display(),
            crate::location::shown(&state.remote)
        ));
    }
    match (state.local_hash, state.remote_hash, state.hash) {
        (Some(local), Some(remote), _) => History::Synced(Base { local, remote }),
        (_, _, Some(hash)) => History::Synced(Base { local: hash.clone(), remote: hash }),
        _ => unreadable(),
    }
}

/// What `rusk restore` has to add when the database it restored, now
/// holding `restored`, is synced with the remote the next `rusk sync` goes
/// to: the restored tasks are not what the last sync left here, so that
/// sync takes them for a change made here — and sends them on, replacing
/// what the remote holds. Nothing to add for another remote, or none, or
/// when the restored tasks are what the last sync left (review of R28).
pub fn note_after_restore(db_path: &Path, restored: &[Task]) -> Option<String> {
    let data = std::fs::read_to_string(state_path(db_path)).ok()?;
    let state = serde_json::from_str::<SyncState>(&data).ok()?;
    let config = crate::config::config();
    let configured = crate::config::env_or_config("RUSK_SYNC_REMOTE", &config.sync_remote).ok()??;
    let remote = remote_key(&configured)?;
    if state.remote != remote && remote_key(&state.remote).as_deref() != Some(remote.as_str()) {
        return None;
    }
    let left_here = state.local_hash.as_ref().or(state.hash.as_ref())?;
    if canonical_hash(restored).ok()? == *left_here {
        return None;
    }
    Some(format!(
        "Note: this database is synced with {remote}. The next `rusk sync` takes the restored \
         tasks for a change made here and sends them there, unless the remote has changed too; \
         `rusk sync pull --force` takes the remote's tasks back instead."
    ))
}

/// Records `base` as the state after a sync with `remote`. A failure is
/// said, not swallowed (REVIEW №107): the sync itself went through, but the
/// next one will not know about it.
fn write_history(state_path: &Path, remote: &str, base: &Base) {
    let state = SyncState {
        remote: remote.to_string(),
        hash: None,
        local_hash: Some(base.local.clone()),
        remote_hash: Some(base.remote.clone()),
    };
    let written = serde_json::to_string(&state)
        .map_err(std::io::Error::other)
        .and_then(|json| {
            crate::atomic::replace_aux_file(state_path, json.as_bytes(), crate::atomic::Mode::Keep)
        });
    if let Err(e) = written {
        crate::backend::warn_yellow(&format!(
            "Warning: this sync could not be recorded in '{}' ({e}); the next `rusk sync` \
             will not know about it and may ask you to choose a side with --force",
            state_path.display()
        ));
    }
}

/// Says so when a side does not read back what it was sent: its format
/// cannot hold everything (a todo.txt text that starts with `x `, a
/// Markdown text that looks like a list item).
fn warn_if_lossy(side: &str, sent: &[Task], held: &[Task]) {
    let held_by_id: std::collections::HashMap<crate::TaskId, &Task> =
        held.iter().map(|task| (task.id, task)).collect();
    let sent_ids: std::collections::HashSet<crate::TaskId> = sent.iter().map(|task| task.id).collect();
    let changed = sent
        .iter()
        .filter(|task| held_by_id.get(&task.id) != Some(task))
        .map(|task| task.id);
    let added = held.iter().filter(|task| !sent_ids.contains(&task.id)).map(|task| task.id);
    let differ: Vec<crate::TaskId> = changed.chain(added).collect();
    if let Some(first) = differ.first() {
        crate::backend::warn_yellow(&format!(
            "Warning: {side} does not hold the tasks exactly as they were sent, as its format \
             cannot hold everything: {} task(s) differ there (task {first} among them); the next \
             sync goes by what it holds",
            differ.len()
        ));
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
    let key = remote.describe();
    let state_file = state_path(&db_path);
    let sync = Sync {
        local: &local,
        remote: &remote,
        state_file: &state_file,
        key: &key,
    };

    // `--force` needs only the side it copies from: the side it replaces
    // may not read at all, which can be the very reason to replace it
    // (REVIEW №70). When it does read, it is read: holding the same tasks,
    // it is not written again (its `.backup` stays what it was), and its
    // save checks that nobody changed it meanwhile (review of R21).
    match direction {
        Direction::Push { force: true } => {
            let tasks = local.read()?.tasks;
            if let Ok(held) = remote.fetch()
                && canonical_hash(&held.tasks)? == canonical_hash(&tasks)?
            {
                return sync.already_in_sync(&tasks, &held.tasks);
            }
            return sync.push(&tasks);
        }
        Direction::Pull { force: true } => {
            let tasks = remote.fetch()?.tasks;
            if let Ok(held) = local.read()
                && canonical_hash(&held.tasks)? == canonical_hash(&tasks)?
            {
                return sync.already_in_sync(&held.tasks, &tasks);
            }
            return sync.pull(tasks);
        }
        _ => {}
    }

    let local_db = local.read()?;
    let remote_db = remote.fetch()?;
    let (local_side, remote_side) = (Side::of(&local_db), Side::of(&remote_db));
    let (local_tasks, remote_tasks) = (local_db.tasks, remote_db.tasks);

    let local_hash = canonical_hash(&local_tasks)?;
    let remote_hash = canonical_hash(&remote_tasks)?;
    let history = read_history(&state_file, &key);
    let base = match &history {
        History::Synced(base) => Some(base),
        History::Never | History::Unusable(_) => None,
    };
    let mut status = decide(base, &local_hash, &remote_hash, local_side, remote_side);
    // The remote holds exactly what the local tasks become in its format:
    // nothing to push, and nothing to pull that the local side lacks. A
    // lossy remote synced by an older rusk, whose state has one hash for
    // both sides, is in this state (review of R21).
    if status != SyncStatus::InSync
        && canonical_hash(&remote.stored_form(&local_tasks))? == remote_hash
    {
        status = SyncStatus::InSync;
    }

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

    // Both ways out of a conflict, and nothing that sends the user round in
    // a circle (REVIEW №23).
    let choose = "keep one side: `rusk sync push --force` keeps the local tasks, \
                  `rusk sync pull --force` the remote ones";
    // Why the state file says nothing, when it is there but of no use.
    let why = match &history {
        History::Unusable(why) => format!(" ({why})"),
        _ => String::new(),
    };
    match (direction, status) {
        (_, SyncStatus::InSync) => sync.already_in_sync(&local_tasks, &remote_tasks),
        (Direction::Auto | Direction::Push { .. }, SyncStatus::LocalAhead) => sync.push(&local_tasks),
        (Direction::Auto | Direction::Pull { .. }, SyncStatus::RemoteAhead) => sync.pull(remote_tasks),
        (_, SyncStatus::Diverged) => bail!(
            "the local database and {} have both changed since the last sync; {choose}",
            remote.describe()
        ),
        (_, SyncStatus::Unrelated) => bail!(
            "the local database and {} hold different tasks and have not been synced \
             before{why}; {choose}",
            remote.describe()
        ),
        // No sync on record: a direction was named, the other side is the
        // one without tasks (review of R21: "changed since the last sync"
        // spoke of a sync there never was).
        (Direction::Push { .. }, SyncStatus::RemoteAhead) if base.is_none() => bail!(
            "{}, while {} holds tasks; they have not been synced before{why}: \
             `rusk sync pull` (or `rusk sync`) copies those tasks here, \
             `rusk sync push --force` empties {}",
            local_side.describe(&format!("the local database at {}", db_path.display())),
            remote.describe(),
            remote.describe()
        ),
        (Direction::Pull { .. }, SyncStatus::LocalAhead) if base.is_none() => bail!(
            "{}, while the local database holds tasks; they have not been synced before{why}: \
             `rusk sync push` (or `rusk sync`) copies the local tasks there, \
             `rusk sync pull --force` empties the local database",
            remote_side.describe(&remote.describe())
        ),
        (Direction::Push { .. }, SyncStatus::RemoteAhead) => bail!(
            "{} has changed since the last sync and the local database has not: \
             `rusk sync pull` (or `rusk sync`) brings the change here, \
             `rusk sync push --force` overwrites it",
            remote.describe()
        ),
        (Direction::Pull { .. }, SyncStatus::LocalAhead) => bail!(
            "the local database has changed since the last sync and {} has not: \
             `rusk sync push` (or `rusk sync`) sends the change, \
             `rusk sync pull --force` discards it",
            remote.describe()
        ),
    }
}

/// The two sides of one `rusk sync` and where it keeps its state.
struct Sync<'a> {
    local: &'a Backend,
    remote: &'a Remote,
    state_file: &'a Path,
    key: &'a str,
}

impl Sync<'_> {
    /// Records that the two sides hold `local` and `remote`, the same tasks
    /// — even when nothing moved, so that a later change is told from a
    /// first contact.
    fn already_in_sync(&self, local: &[Task], remote: &[Task]) -> Result<()> {
        let base = Base {
            local: canonical_hash(local)?,
            remote: canonical_hash(remote)?,
        };
        write_history(self.state_file, self.key, &base);
        crate::outln!(
            "{} with {}",
            theme().success.paint("Already in sync"),
            self.remote.describe()
        )
    }

    /// Replaces the remote with `tasks`, the local list, and records what
    /// each side holds now: the remote as its format stores the list,
    /// worked out rather than read back — another writer may have changed
    /// it since, and that change must not pass for this sync's (review of
    /// R21).
    fn push(&self, tasks: &[Task]) -> Result<()> {
        let held = self.remote.push(tasks)?;
        warn_if_lossy(&self.remote.describe(), tasks, &held);
        let base = Base {
            local: canonical_hash(tasks)?,
            remote: canonical_hash(&held)?,
        };
        write_history(self.state_file, self.key, &base);
        crate::outln!(
            "{} {} task(s) to {}",
            theme().success.paint("Pushed"),
            tasks.len(),
            self.remote.describe()
        )
    }

    /// Replaces the local database with `tasks`, the remote list, and
    /// records what each side holds now: the local one as its format stores
    /// the list (see [`push`](Self::push)).
    fn pull(&self, tasks: Vec<Task>) -> Result<()> {
        self.local.save(&tasks)?;
        let held = self.local.stored_form(&tasks);
        warn_if_lossy("the local database", &tasks, &held);
        let base = Base {
            local: canonical_hash(&held)?,
            remote: canonical_hash(&tasks)?,
        };
        write_history(self.state_file, self.key, &base);
        crate::outln!(
            "{} {} task(s) from {}",
            theme().success.paint("Pulled"),
            tasks.len(),
            self.remote.describe()
        )
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

    fn base(local: &str, remote: &str) -> Base {
        Base {
            local: local.into(),
            remote: remote.into(),
        }
    }

    #[test]
    fn decide_matrix() {
        use Side::{Empty, Missing, Tasks};
        let two = Tasks(2);
        // Identical content is always in sync.
        assert_eq!(decide(None, "a", "a", two, two), SyncStatus::InSync);
        assert_eq!(decide(Some(&base("x", "y")), "a", "a", two, two), SyncStatus::InSync);
        // Fast-forwards.
        assert_eq!(decide(Some(&base("b", "r")), "l", "r", two, two), SyncStatus::LocalAhead);
        assert_eq!(decide(Some(&base("l", "b")), "l", "r", two, two), SyncStatus::RemoteAhead);
        // Both sides moved.
        assert_eq!(decide(Some(&base("b", "b")), "l", "r", two, two), SyncStatus::Diverged);
        // REVIEW №24: each side as the last sync left it, though they read
        // differently (a remote format that cannot hold everything).
        assert_eq!(decide(Some(&base("l", "r")), "l", "r", two, two), SyncStatus::InSync);
        // First sync: a side with no tasks is seeded, either way round
        // (REVIEW №23); two lists of tasks need a decision.
        assert_eq!(decide(None, "l", "r", two, Empty), SyncStatus::LocalAhead);
        assert_eq!(decide(None, "l", "r", two, Missing), SyncStatus::LocalAhead);
        assert_eq!(decide(None, "l", "r", Empty, two), SyncStatus::RemoteAhead);
        assert_eq!(decide(None, "l", "r", Missing, two), SyncStatus::RemoteAhead);
        assert_eq!(decide(None, "l", "r", two, two), SyncStatus::Unrelated);
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
        assert_eq!(read_history(&sp, "user@h:/p"), History::Never);
        write_history(&sp, "user@h:/p", &base("l", "r"));
        assert_eq!(read_history(&sp, "user@h:/p"), History::Synced(base("l", "r")));
        // A different remote invalidates the recorded base, and says so.
        let History::Unusable(why) = read_history(&sp, "user@other:/p") else {
            panic!("another remote's state was used");
        };
        assert!(why.contains("another remote, user@h:/p"), "{why}");
    }

    /// Review of R21: a key that does not read back as a remote (a path
    /// that starts with `~` of its own) made the state of the very same
    /// remote "another remote's".
    #[test]
    fn the_key_itself_is_the_same_remote() {
        let dir = tempfile::tempdir().unwrap();
        let sp = dir.path().join("tasks.json.sync");
        let key = remote_key("user@vps:~/~odd.json").unwrap();
        assert_eq!(key, "user@vps:~/~odd.json");
        assert_eq!(remote_key(&key).as_deref(), Some(key.as_str()));
        write_history(&sp, &key, &base("l", "r"));
        assert_eq!(read_history(&sp, &key), History::Synced(base("l", "r")));
    }

    /// REVIEW №58: the state was keyed by the remote as typed, so
    /// `http://h:p/` and `http://h:p` were two remotes.
    #[test]
    fn a_remote_is_one_however_it_is_spelled() {
        let dir = tempfile::tempdir().unwrap();
        let sp = dir.path().join("tasks.json.sync");
        std::fs::write(&sp, r#"{"remote":"http://127.0.0.1:18700/","hash":"h"}"#).unwrap();
        let key = remote_key("http://127.0.0.1:18700").unwrap();
        // An older rusk's state: one hash for both sides.
        assert_eq!(read_history(&sp, &key), History::Synced(base("h", "h")));
    }

    /// REVIEW №23: a state file that is not one rusk wrote was ignored in
    /// silence; the conflict it leads to says why there is no base.
    #[test]
    fn an_unusable_state_says_why() {
        let dir = tempfile::tempdir().unwrap();
        let sp = dir.path().join("tasks.json.sync");
        std::fs::write(&sp, "garbage").unwrap();
        let History::Unusable(why) = read_history(&sp, "user@h:/p") else {
            panic!("garbage was taken for a state");
        };
        assert!(why.contains("is not one rusk wrote") && why.contains("tasks.json.sync"), "{why}");
    }

    /// REVIEW №107: a state that could not be written was not said.
    #[test]
    #[cfg(unix)]
    fn a_state_that_cannot_be_written_is_warned_about() {
        let dir = tempfile::tempdir().unwrap();
        // A directory in the way fails the write for every user.
        let sp = dir.path().join("tasks.json.sync");
        std::fs::create_dir(&sp).unwrap();
        write_history(&sp, "user@h:/p", &base("l", "r"));
        assert!(sp.is_dir(), "the write went through a directory");
    }
}
