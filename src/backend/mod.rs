//! Storage backends: where the task database lives. The shape of `rusk_db`
//! (or the `RUSK_DB` environment variable) picks the backend:
//!
//! - `https://host[:port]` — the API of a running `rusk serve`
//!   (feature `backend-http`, token via `db_token` / `RUSK_DB_TOKEN`);
//! - `user@host:/path/tasks.json` — a file over ssh; the remote extension
//!   picks the format as usual (feature `backend-ssh`);
//! - a path ending in `.db` / `.sqlite` / `.sqlite3` — SQLite
//!   (feature `backend-sqlite`);
//! - any other path — a local file; the extension picks the format
//!   (see [`crate::codec`]). With `git_backend = true` every save is also
//!   committed to a git repository in the database directory
//!   (feature `backend-git`).
//!
//! Every backend implements the same whole-database `load`/`save` contract
//! the rest of rusk is built on; `.backup` copies and `rusk restore` work
//! for the local (file and SQLite) backends. Every local file — database,
//! `.backup`, `.before_restore` — is written through [`crate::atomic`].
//! Whatever a backend reads passes [`normalized`] before anything uses it:
//! every task has a text, an id of its own and dependencies only on other
//! tasks that exist, however the stored list was edited.
//!
//! Writers do not overwrite each other. A backend remembers what it loaded,
//! and [`Backend::save`] refuses to replace a database that has changed
//! since ([`StaleDatabase`]). [`Backend::update`] is the read-modify-write
//! built on that: the change is applied to the current state of the
//! database — under a writer lock locally, guarded by `If-Match` over http
//! — so commands running at the same time all take effect.

pub mod file;
#[cfg(feature = "backend-git")]
pub mod git;
#[cfg(feature = "backend-http")]
pub mod http;
#[cfg(feature = "backend-sqlite")]
pub mod sqlite;
#[cfg(feature = "backend-ssh")]
pub mod ssh;

use crate::atomic;
use crate::location::Location;
use crate::model::Task;
use file::Change;
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Non-fatal warning on stderr (backup / atomic-write / git fallbacks), in
/// the theme warning color (yellow by default).
pub(crate) fn warn_yellow(msg: &str) {
    crate::errln!("{}", crate::config::theme().warning.paint(msg));
}

/// Makes the records a backend read from `location` into a task list rusk
/// can hold ([`crate::model::normalize`]); what changes a task is reported
/// on stderr. Every read goes through here — loads, the re-reads of
/// `update`, `.backup` copies — so no code ever sees a list that breaks
/// the rules.
///
/// A warning is printed once per process: the same file read again (the
/// re-read of an `update`, `rusk serve` reading it for every request)
/// has nothing new to say until its content does.
pub(crate) fn normalized(mut tasks: Vec<Task>, location: &str) -> Result<Vec<Task>> {
    let repairs = crate::model::normalize(&mut tasks)?;
    for warning in repairs.warnings(location) {
        warn_once(&warning);
    }
    Ok(tasks)
}

/// `git_backend = true` commits a file database on this machine; `what`
/// is not one, and is not committed — said rather than done in silence
/// (review of R22).
#[cfg_attr(
    not(any(feature = "backend-sqlite", feature = "backend-http", feature = "backend-ssh")),
    allow(dead_code)
)]
fn git_backend_does_not_apply(what: &str) {
    if crate::config::config().git_backend {
        warn_once(&format!(
            "Warning: git_backend = true commits a database file on this machine; {what} is \
             not committed"
        ));
    }
}

/// A warning about what was read, said once per process: the same file read
/// again (the re-read of an `update`, the second look before an ssh save,
/// `rusk serve` reading it for every request) has nothing new to say until
/// its content does.
pub(crate) fn warn_once(msg: &str) {
    static PRINTED: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());
    let mut printed = PRINTED.lock().unwrap_or_else(|e| e.into_inner());
    if !printed.iter().any(|seen| seen == msg) {
        warn_yellow(msg);
        printed.push(msg.to_string());
    }
}

/// What a load found at a database location.
#[derive(Debug)]
pub struct Loaded {
    pub tasks: Vec<Task>,
    /// True when there is no database at the location at all: no local
    /// file, no remote file. A database that exists but cannot be read is
    /// an error and never gets here, so this really does mean "nothing was
    /// ever stored here". `rusk sync` needs the difference: a database
    /// nobody created is not one the user emptied, and must not empty the
    /// other side.
    pub missing: bool,
}

/// A write was refused because the database no longer holds what this
/// backend read: another process changed it in between, and replacing it
/// now would silently drop that change. Nothing was written.
#[derive(Debug)]
pub struct StaleDatabase {
    location: String,
}

impl StaleDatabase {
    pub(crate) fn at(location: impl std::fmt::Display) -> anyhow::Error {
        anyhow::Error::new(Self {
            location: location.to_string(),
        })
    }
}

impl std::fmt::Display for StaleDatabase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "the database '{}' was changed by another process after it was read; \
             nothing was saved — run the command again",
            self.location
        )
    }
}

impl std::error::Error for StaleDatabase {}

/// A change to the task list for [`Backend::update`]. It may run more than
/// once (on a fresh list each time) when the first attempt loses against
/// another writer, so it must not have effects of its own.
pub type ChangeFn<'a, T> = &'a mut dyn FnMut(&mut Vec<Task>) -> Result<T>;

/// Runs `change` on a copy of `base`: the new list, what the change
/// returned, and whether there is anything to write.
fn apply<T>(base: &[Task], change: ChangeFn<'_, T>) -> Result<(Vec<Task>, T, bool)> {
    let mut tasks = base.to_vec();
    let value = change(&mut tasks)?;
    let changed = tasks != base;
    Ok((tasks, value, changed))
}

/// What [`Backend::update`] made of a change.
#[derive(Debug)]
pub struct Updated<T> {
    /// The task list with the change applied.
    pub tasks: Vec<Task>,
    /// What the change returned.
    pub value: T,
    /// Whether the database holds `tasks` now — as reading it gives them:
    /// a file that needed repairs when it was read (see [`normalized`])
    /// keeps its own form until something is written. False in one case
    /// only: the caller's list carried edits of its own, the change added
    /// nothing to them, so nothing was written and they are still in
    /// memory only.
    pub stored: bool,
}

/// One `.backup` sibling per save for the local backends: the content about
/// to be replaced. Called by the backend inside its write step — after the
/// staleness check, so a refused save leaves the backup alone, and with
/// writers locked out, so the copy is never taken mid-write. The copy
/// replaces the previous one atomically and is as private as the database;
/// a failure is a warning, never a reason to lose the save itself.
///
/// This is the byte copy of a file database; SQLite makes its own copy,
/// see [`refresh_backup_from_image`].
fn refresh_backup(path: &Path) {
    if !crate::config::config().backup {
        return;
    }
    // A zero-length database holds nothing to keep (what a crashed
    // in-place write or a full disk leaves behind looks like this),
    // while the backup already there may be the last good copy. There is
    // nothing to copy either when the file is not there yet — the first
    // save of a new database. Anything else, a file that cannot even be
    // looked at included, goes on to the copy, which says what went wrong.
    match std::fs::metadata(path) {
        Ok(meta) if meta.len() == 0 => return,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return,
        _ => {}
    }
    if let Err(e) = atomic::copy_to_aux_file(path, &aux_path(path, "backup")) {
        warn_yellow(&format!("Warning: Failed to create backup: {e}"));
    }
}

/// [`refresh_backup`] for SQLite: the bytes of the main file are not the
/// database (what is committed in a WAL is missing from them), so the copy
/// is the image SQLite makes of it — `image` is called only when backups
/// are on.
#[cfg(feature = "backend-sqlite")]
fn refresh_backup_from_image(path: &Path, image: impl FnOnce() -> Result<Vec<u8>>) {
    if !crate::config::config().backup {
        return;
    }
    let written = image().and_then(|image| {
        atomic::replace_aux_file_like(&aux_path(path, "backup"), &image, path).map_err(Into::into)
    });
    if let Err(e) = written {
        warn_yellow(&format!("Warning: Failed to create backup: {e:#}"));
    }
}


#[derive(Debug)]
pub enum Backend {
    File(file::FileBackend),
    #[cfg(feature = "backend-sqlite")]
    Sqlite(sqlite::SqliteBackend),
    #[cfg(feature = "backend-http")]
    Http(http::HttpBackend),
    #[cfg(feature = "backend-ssh")]
    Ssh(ssh::SshBackend),
}

/// Whether the file name selects SQLite, read like every other format (see
/// [`crate::codec::format_extension`]).
pub(crate) fn is_sqlite_path(path: &Path) -> bool {
    ["db", "sqlite", "sqlite3"].contains(&crate::codec::format_extension(path).as_str())
}

impl Backend {
    /// Resolves the configured database location. Test and debug runs are
    /// pinned to a temp file, mirroring the config isolation in
    /// `config::load`. An empty `RUSK_DB` is not set, as an empty
    /// `rusk_db` is.
    pub fn resolve() -> Result<Self> {
        if crate::is_test_mode() || cfg!(debug_assertions) {
            let path = std::env::temp_dir().join("rusk_debug").join("tasks.json");
            Self::from_local_path(path)
        } else if let Some(value) = std::env::var_os("RUSK_DB").filter(|v| !v.is_empty()) {
            let location = Location::parse_os(&value).with_context(|| {
                format!("invalid RUSK_DB '{}'", crate::location::shown(&value.to_string_lossy()))
            })?;
            Self::at(location)
        } else if let Some(value) = &crate::config::config().rusk_db {
            // `rusk_db` from the config file; the RUSK_DB env var wins above.
            let location = Location::parse(value).with_context(|| {
                format!("invalid rusk_db '{}' in the config file", crate::location::shown(value))
            })?;
            Self::at(location)
        } else {
            Self::from_local_path(PathBuf::from(".rusk").join("tasks.json"))
        }
    }

    /// The backend for a `rusk_db` value (see [`crate::location`]).
    pub fn parse(value: &str) -> Result<Self> {
        Self::at(Location::parse(value)?)
    }

    /// The backend that serves a location.
    pub fn at(location: Location) -> Result<Self> {
        match location {
            Location::Local(path) => Self::from_local_path(path),
            #[cfg(feature = "backend-http")]
            Location::Http(url) => {
                git_backend_does_not_apply("a database behind `rusk serve`");
                let token = crate::config::env_or_config(
                    "RUSK_DB_TOKEN",
                    &crate::config::config().db_token,
                )?;
                Ok(Backend::Http(http::HttpBackend::new(&url, token)))
            }
            #[cfg(not(feature = "backend-http"))]
            Location::Http(url) => bail!(
                "'{}' is an http(s) database location; this rusk build \
                 does not include it — rebuild with `--features backend-http`",
                crate::location::shown(&url)
            ),
            #[cfg(feature = "backend-ssh")]
            Location::Ssh(ssh) => {
                git_backend_does_not_apply("a database over ssh");
                Ok(Backend::Ssh(ssh::SshBackend::new(&ssh)?))
            }
            #[cfg(not(feature = "backend-ssh"))]
            Location::Ssh(ssh) => bail!(
                "'{}:{}' is an ssh database location; this rusk build \
                 does not include it — rebuild with `--features backend-ssh`",
                ssh.host,
                ssh.path
            ),
        }
    }

    /// A local database: SQLite when the extension says so, a plain file
    /// otherwise.
    pub fn from_local_path(path: PathBuf) -> Result<Self> {
        if is_sqlite_path(&path) {
            #[cfg(feature = "backend-sqlite")]
            {
                git_backend_does_not_apply("a SQLite database");
                return Ok(Backend::Sqlite(sqlite::SqliteBackend::new(path)));
            }
            #[cfg(not(feature = "backend-sqlite"))]
            bail!(
                "'{}' is a SQLite database; this rusk build does not include \
                 it — rebuild with `--features backend-sqlite`",
                path.display()
            );
        }
        Ok(Backend::File(file::FileBackend::new(path)?))
    }

    pub fn load(&self) -> Result<Vec<Task>> {
        match self {
            Backend::File(b) => b.load(),
            #[cfg(feature = "backend-sqlite")]
            Backend::Sqlite(b) => b.load(),
            #[cfg(feature = "backend-http")]
            Backend::Http(b) => b.load(),
            #[cfg(feature = "backend-ssh")]
            Backend::Ssh(b) => b.load(),
        }
    }

    /// [`load`], plus whether there was a database to load at all. Only
    /// `rusk sync` has to know: for every other command a database that is
    /// not there yet simply holds no tasks.
    ///
    /// [`load`]: Self::load
    pub fn read(&self) -> Result<Loaded> {
        match self {
            Backend::File(b) => b.read(),
            #[cfg(feature = "backend-sqlite")]
            Backend::Sqlite(b) => b.read(),
            // A server that answers is a database; one that does not is an
            // error, never an absence.
            #[cfg(feature = "backend-http")]
            Backend::Http(b) => Ok(Loaded {
                tasks: b.load()?,
                missing: false,
            }),
            #[cfg(feature = "backend-ssh")]
            Backend::Ssh(b) => b.read(),
        }
    }

    /// Replaces the whole database with `tasks` — unless it has changed
    /// since this backend loaded it: then the list is based on content that
    /// is gone, nothing is written and the error is a [`StaleDatabase`].
    /// A backend that never loaded anything replaces unconditionally. The
    /// local backends keep the replaced content as `.backup`.
    pub fn save(&self, tasks: &[Task]) -> Result<()> {
        self.write(tasks, Change::Update)
    }

    fn write(&self, tasks: &[Task], change: Change) -> Result<()> {
        match self {
            Backend::File(b) => b.write(tasks, change),
            #[cfg(feature = "backend-sqlite")]
            Backend::Sqlite(b) => b.write(tasks, change),
            #[cfg(feature = "backend-http")]
            Backend::Http(b) => b.save(tasks),
            #[cfg(feature = "backend-ssh")]
            Backend::Ssh(b) => b.save(tasks),
        }
    }

    /// Read-modify-write as one step: `change` is applied to the current
    /// task list and the result is stored, with no other writer able to
    /// slip in between.
    ///
    /// `snapshot` is the list the caller holds from [`load`]; it is the
    /// base of the change while the database still is what it was loaded
    /// from. Otherwise the database is read again and `change` runs on
    /// that. A snapshot that carries edits of its own (`snapshot_is_clean`
    /// is false) cannot be swapped for a fresh read without losing them,
    /// so that case is a [`StaleDatabase`] error.
    ///
    /// Nothing is written when `change` fails or leaves the list as it was.
    ///
    /// [`load`]: Self::load
    pub fn update<T>(
        &self,
        snapshot: &[Task],
        snapshot_is_clean: bool,
        change: ChangeFn<'_, T>,
    ) -> Result<Updated<T>> {
        // A command that changes nothing must not leave a directory, a lock
        // file or an empty SQLite database where there was no database.
        // Only a location known to hold nothing counts; one that cannot
        // even be looked at goes to the backend, which reports why.
        if let Some(path) = self.local_path()
            && matches!(path.try_exists(), Ok(false))
        {
            let (tasks, value, changed) = apply(snapshot, change)?;
            if !changed {
                return Ok(Updated {
                    tasks,
                    value,
                    stored: snapshot_is_clean,
                });
            }
        }
        match self {
            Backend::File(b) => b.update(snapshot, snapshot_is_clean, change),
            #[cfg(feature = "backend-sqlite")]
            Backend::Sqlite(b) => b.update(snapshot, snapshot_is_clean, change),
            #[cfg(feature = "backend-http")]
            Backend::Http(b) => b.update(snapshot, snapshot_is_clean, change),
            #[cfg(feature = "backend-ssh")]
            Backend::Ssh(b) => b.update(snapshot, snapshot_is_clean, change),
        }
    }

    /// What this database holds once `tasks` are saved to it (see
    /// [`DbFormat::stored_form`](crate::codec::DbFormat::stored_form)).
    /// SQLite holds a list as it is; what a server holds, it says itself
    /// (see `HttpBackend::save_held`).
    pub fn stored_form(&self, tasks: &[Task]) -> Vec<Task> {
        match self {
            Backend::File(b) => b.format().stored_form(tasks),
            #[cfg(feature = "backend-sqlite")]
            Backend::Sqlite(_) => tasks.to_vec(),
            #[cfg(feature = "backend-http")]
            Backend::Http(_) => tasks.to_vec(),
            #[cfg(feature = "backend-ssh")]
            Backend::Ssh(b) => b.format().stored_form(tasks),
        }
    }

    /// Where the database lives, for messages and logs.
    pub fn describe(&self) -> String {
        match self {
            Backend::File(b) => b.path().display().to_string(),
            #[cfg(feature = "backend-sqlite")]
            Backend::Sqlite(b) => b.path().display().to_string(),
            #[cfg(feature = "backend-http")]
            Backend::Http(b) => b.describe(),
            #[cfg(feature = "backend-ssh")]
            Backend::Ssh(b) => b.describe(),
        }
    }

    /// The database file path for local backends; `None` for remote ones.
    pub fn local_path(&self) -> Option<&Path> {
        match self {
            Backend::File(b) => Some(b.path()),
            #[cfg(feature = "backend-sqlite")]
            Backend::Sqlite(b) => Some(b.path()),
            #[cfg(any(feature = "backend-http", feature = "backend-ssh"))]
            _ => None,
        }
    }

    /// Reads the `.backup` sibling the strict way (see the backends'
    /// `load_backup`): the file is not modified, and one that is not a task
    /// database is an error rather than "zero tasks".
    fn load_backup(&self, path: PathBuf) -> Result<BackupContent> {
        match self {
            Backend::File(_) => {
                let (tasks, raw) = file::FileBackend::new(path)?.load_backup()?;
                Ok(BackupContent {
                    tasks,
                    raw: Some(raw.into_bytes()),
                })
            }
            #[cfg(feature = "backend-sqlite")]
            Backend::Sqlite(_) => Ok(BackupContent {
                tasks: sqlite::SqliteBackend::new(path).load_backup()?,
                raw: None,
            }),
            #[cfg(any(feature = "backend-http", feature = "backend-ssh"))]
            _ => bail!("remote databases have no local backup"),
        }
    }

    /// Replaces the database with its `.backup` sibling and returns the
    /// restored tasks. Local backends only.
    ///
    /// Nothing is lost at any step, whatever fails or crashes in between:
    /// the backup is validated without being modified; the current database
    /// file is first copied byte for byte to a `.before_restore` name that
    /// never overwrites an earlier copy; only then is the database replaced
    /// — a file atomically by the very bytes of the backup (and, with
    /// `git_backend`, as a commit of its own), SQLite inside a transaction.
    /// The `.backup` itself is left as it is.
    pub fn restore_from_backup(&self) -> Result<Vec<Task>> {
        let Some(db_path) = self.local_path() else {
            // Both remote backends do keep a `.backup`, but neither can be
            // restored from here — say where it is instead of sending the
            // user to look for a copy that may not exist.
            #[cfg(feature = "backend-ssh")]
            if let Backend::Ssh(b) = self {
                bail!(
                    "`rusk restore` needs a local database; the database is at {} — \
                     the previous content is kept on the remote as '{}.backup', \
                     copy it over the database there",
                    self.describe(),
                    b.remote_path()
                );
            }
            bail!(
                "`rusk restore` needs a local database; the database is at {} — \
                 `rusk serve` keeps the backup next to the database it serves, \
                 restore it on that machine",
                self.describe()
            );
        };
        let backup_path = aux_path(db_path, "backup");
        // "Not there" and "cannot be looked at" are different answers: an
        // unreadable directory must not be reported as a missing backup.
        if !exists(&backup_path)? {
            bail!("No backup file found at '{}'", backup_path.display());
        }
        // Writers stay out from the look at the current database to its
        // replacement: what gets replaced is exactly what was kept as
        // `.before_restore`. (SQLite guards its own write, see `sqlite`.)
        let _lock = match self {
            Backend::File(b) => Some(b.lock()?),
            #[cfg(any(
                feature = "backend-sqlite",
                feature = "backend-http",
                feature = "backend-ssh"
            ))]
            _ => None,
        };

        // rusk never writes a zero-length backup (see `refresh_backup`), and
        // for the formats whose empty database is an empty file it could
        // not be told from the leftover of an interrupted copy.
        if std::fs::metadata(&backup_path).is_ok_and(|meta| meta.len() == 0) {
            bail!(
                "The backup file '{}' is empty (zero bytes); there is nothing to restore from",
                backup_path.display()
            );
        }
        // Nothing has been touched yet, and the report says so (REVIEW
        // №101): the cause first, like the other refusals of a restore.
        let backup = self.load_backup(backup_path.clone()).map_err(|e| {
            anyhow::anyhow!("{e:#}; nothing was restored, the database is unchanged")
        })?;
        let saved = modified(&backup_path);
        let backup_label = match saved {
            Some(time) => format!("{} (saved {})", backup_path.display(), local_time(time)),
            None => backup_path.display().to_string(),
        };

        let mut kept_now: Option<PathBuf> = None;
        // Set when a file SQLite rejects was emptied to make room: from
        // then on the kept copy is the only one.
        #[cfg_attr(not(feature = "backend-sqlite"), allow(unused_mut))]
        let mut emptied = false;
        if exists(db_path)? {
            let current = self.load();
            if self.holds_the_backup_already(db_path, &backup_path, &backup, &current) {
                crate::outln!(
                    "The database already matches the backup ({} tasks); nothing to restore",
                    backup.tasks.len()
                )?;
                crate::outln!("Backup file: {backup_label}")?;
                return Ok(backup.tasks);
            }
            if let Some(warning) = stale_backup_warning(saved, modified(db_path)) {
                warn_yellow(&warning);
            }

            // Keep a copy of whatever is about to be replaced. A database
            // that no longer parses matters most: its raw bytes may still
            // hold tasks the backup does not have. SQLite copies a file it
            // can read itself, like its `.backup`.
            let kept = self.engine_copy(&current).and_then(|image| {
                let kept = keep_before_restore(db_path, image.as_deref())?;
                Ok((kept, image.is_none()))
            });
            let (kept, raw) = kept.map_err(|e| {
                anyhow::anyhow!(
                    "Failed to keep a copy of the current database ({e:#}); nothing was restored"
                )
            })?;
            kept_now = Some(kept.clone());
            match &current {
                // Said on the way, and the way goes on whether or not
                // anybody reads it: an output that fails must not leave a
                // restore half done.
                Ok(_) => {
                    crate::outln!("Current database backed up to: {}", kept.display()).ok();
                }
                // Why, in one line: a damaged file is not the only reason,
                // and the full report ends in advice to do what is being
                // done.
                Err(error) => {
                    let report = format!("{error:#}");
                    crate::outln!(
                        "Current database cannot be read ({}); {} saved to: {}",
                        report.lines().next().unwrap_or_default().trim_end_matches('.'),
                        if raw { "raw copy" } else { "a copy" },
                        kept.display()
                    )
                    .ok();
                }
            }
            #[cfg(feature = "backend-sqlite")]
            if let (Backend::Sqlite(b), Err(error)) = (self, &current) {
                emptied = b.reset_if_rejected_by_sqlite(error, &kept)?;
            }
        }

        let replaced = match (self, &backup.raw) {
            (Backend::File(b), Some(raw)) => b.write_raw(raw, backup.tasks.len(), Change::Restore),
            _ => self.write(&backup.tasks, Change::Restore),
        };
        if let Err(e) = replaced {
            // Refused before anything was written (another process got in
            // after the copy was taken, or holds the database locked): the
            // database still holds all the copy does, and keeping it would
            // stack up one more with every retry.
            if !emptied
                && self.untouched_by(&e)
                && let Some(kept) = &kept_now
                && std::fs::remove_file(kept).is_ok()
            {
                // Its path has just been printed.
                crate::outln!("The database was not touched; the copy was removed again").ok();
            }
            return Err(e.context("Failed to restore from backup"));
        }

        crate::outln!(
            "Successfully restored {} tasks from backup",
            backup.tasks.len()
        )?;
        crate::outln!("Backup file: {backup_label}")?;

        Ok(backup.tasks)
    }

    /// The copy of the database its engine makes, where it makes one: SQLite
    /// for a file it can read (see [`refresh_backup_from_image`] and
    /// `SqliteBackend::image`), whether or not rusk could make tasks of its
    /// rows. `None` where the bytes of the file are the copy to keep: file
    /// databases, and a file SQLite rejects.
    fn engine_copy(&self, current: &Result<Vec<Task>>) -> Result<Option<Vec<u8>>> {
        match (self, current) {
            #[cfg(feature = "backend-sqlite")]
            (Backend::Sqlite(_), Err(error)) if sqlite::rejected(error) => Ok(None),
            #[cfg(feature = "backend-sqlite")]
            (Backend::Sqlite(b), _) => b.image(),
            _ => Ok(None),
        }
    }

    /// Whether a failed write is known to have left the database as it was:
    /// refused as stale before anything was written — or, for SQLite, any
    /// failure at all, its write being one transaction.
    fn untouched_by(&self, error: &anyhow::Error) -> bool {
        match self {
            #[cfg(feature = "backend-sqlite")]
            Backend::Sqlite(_) => true,
            _ => error.is::<StaleDatabase>(),
        }
    }

    /// A file database is restored byte for byte, so it matches when the
    /// bytes do. The bytes of a SQLite file say little (they differ after
    /// every save, and with a WAL they are not even the whole database):
    /// there the tasks are compared.
    fn holds_the_backup_already(
        &self,
        db_path: &Path,
        backup_path: &Path,
        backup: &BackupContent,
        current: &Result<Vec<Task>>,
    ) -> bool {
        if backup.raw.is_some() {
            same_content(db_path, backup_path)
        } else {
            current.as_ref().is_ok_and(|tasks| *tasks == backup.tasks)
        }
    }
}

/// A validated `.backup`: its tasks and, for file databases, the exact
/// bytes they were decoded from.
struct BackupContent {
    tasks: Vec<Task>,
    raw: Option<Vec<u8>>,
}

/// Copies the current database — its bytes, or `image` when there is one —
/// to `<db>.before_restore`, or to the first free `<db>.before_restore.N`:
/// a second `rusk restore` must not destroy the state the first one saved.
fn keep_before_restore(db_path: &Path, image: Option<&[u8]>) -> std::io::Result<PathBuf> {
    const MAX_COPIES: u32 = 10_000;
    let base = aux_path(db_path, "before_restore");
    for n in 0..MAX_COPIES {
        let candidate = if n == 0 {
            base.clone()
        } else {
            aux_path(&base, &n.to_string())
        };
        let written = match image {
            Some(image) => atomic::write_new_file_like(&candidate, image, db_path),
            None => atomic::copy_to_new_file(db_path, &candidate),
        };
        match written {
            Ok(()) => return Ok(candidate),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        format!(
            "{MAX_COPIES} copies named '{}' already exist; remove the old ones",
            base.display()
        ),
    ))
}

/// Whether there is a file at `path` — asked so that "no" really means
/// "nothing is there". `Path::exists` answers no to a directory nobody may
/// look into and to an I/O error alike, and a database that cannot be
/// reached is not a database that is gone. Nor is a symbolic link whose
/// target is gone: `try_exists` follows the link and reports the target,
/// while the link itself is there and a save would replace it.
fn exists(path: &Path) -> Result<bool> {
    let there = path
        .try_exists()
        .with_context(|| format!("Failed to look at '{}'", path.display()))?;
    if !there && std::fs::symlink_metadata(path).is_ok() {
        return Err(dangling_link(path));
    }
    Ok(there)
}

/// One wording for the link that points nowhere, wherever it turns up.
pub(crate) fn dangling_link(path: &Path) -> anyhow::Error {
    anyhow::anyhow!(
        "The database file '{}' is a symbolic link that points to nothing \
         (is the target's filesystem mounted?)",
        path.display()
    )
}

/// True when both files are readable and byte-identical.
fn same_content(a: &Path, b: &Path) -> bool {
    let same_size = match (std::fs::metadata(a), std::fs::metadata(b)) {
        (Ok(a), Ok(b)) => a.len() == b.len(),
        _ => false,
    };
    same_size
        && match (std::fs::read(a), std::fs::read(b)) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        }
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

fn local_time(time: SystemTime) -> String {
    chrono::DateTime::<chrono::Local>::from(time)
        .format("%d-%m-%Y %H:%M")
        .to_string()
}

/// A save refreshes `.backup` moments before it replaces the database, so
/// the two are normally the same age. A backup that is clearly older means
/// the database was changed without it being refreshed (`backup = false`,
/// a hand edit): restoring goes back further than "one save".
fn stale_backup_warning(backup: Option<SystemTime>, database: Option<SystemTime>) -> Option<String> {
    const SAME_SAVE: Duration = Duration::from_secs(60);
    let backups_disabled = !crate::config::config().backup;
    let saved = backup.map(local_time);
    let lag = match (backup, database) {
        (Some(backup), Some(database)) => database.duration_since(backup).ok(),
        _ => None,
    }
    .filter(|lag| *lag > SAME_SAVE);

    match (backups_disabled, lag) {
        (false, None) => None,
        (true, _) => Some(format!(
            "Warning: backups are disabled (`backup = false`), so this one is not refreshed \
             on save{}; changes made since then are not in it",
            saved.map(|s| format!(": it was saved {s}")).unwrap_or_default()
        )),
        (false, Some(lag)) => Some(format!(
            "Warning: the backup is {} older than the database{}; \
             changes made since then are not in it",
            describe_duration(lag),
            saved.map(|s| format!(" (saved {s})")).unwrap_or_default()
        )),
    }
}

fn describe_duration(duration: Duration) -> String {
    let secs = duration.as_secs();
    let (count, unit) = match secs {
        0..120 => (secs, "second"),
        120..7_200 => (secs / 60, "minute"),
        7_200..172_800 => (secs / 3_600, "hour"),
        _ => (secs / 86_400, "day"),
    };
    format!("{count} {unit}{}", if count == 1 { "" } else { "s" })
}

/// Auxiliary sibling of the database file: the suffix is appended to the
/// whole name — `tasks.json` + `backup` → `tasks.json.backup`, `tasks.csv`
/// → `tasks.csv.backup` — so the base format stays recognizable, and an
/// extensionless `tasks` gets `tasks.backup` instead of sharing the
/// siblings of a `tasks.json` next to it.
pub(crate) fn aux_path(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(".");
    name.push(suffix);
    PathBuf::from(name)
}

/// What `candidate` is to the database file `db`, when it is the database
/// itself or one of the files rusk and SQLite keep beside it (`.backup`,
/// `.sync`, …) — under any name for it: a relative path, a detour through
/// `..`, a symbolic or hard link. `rusk gen -o` writes a page, and over one
/// of those it would replace it without a backup.
pub fn database_file_role(db: &Path, candidate: &Path) -> Option<&'static str> {
    let target = real_path(candidate)?;
    // The database under a name of its own.
    if let (Ok(a), Ok(b)) = (std::fs::metadata(&target), std::fs::metadata(db))
        && same_file(&a, &b)
    {
        return Some("the database");
    }
    // Its siblings go beside the path it is named by, and beside the file
    // a link names.
    let bases = [named_path(db), real_path(db)];
    bases.into_iter().flatten().find_map(|base| {
        if target.parent() != base.parent() {
            return None;
        }
        let rest = target.file_name()?.to_str()?.strip_prefix(base.file_name()?.to_str()?)?;
        match rest {
            "" => Some("the database"),
            ".backup" => Some("the database's backup"),
            ".sync" => Some("the database's sync state"),
            ".lock" => Some("the database's lock file"),
            "-journal" | "-wal" | "-shm" => Some("a file SQLite keeps beside the database"),
            ".before_restore" => Some("a copy `rusk restore` kept"),
            _ => match rest.strip_prefix(".before_restore.") {
                Some(n) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => {
                    Some("a copy `rusk restore` kept")
                }
                _ => None,
            },
        }
    })
}

/// The file `path` names wherever links and `..` lead, whether or not it
/// exists yet.
fn real_path(path: &Path) -> Option<PathBuf> {
    std::fs::canonicalize(path).ok().or_else(|| named_path(path))
}

/// `path` with its directory resolved and its own name as it is: a link
/// stays the link.
fn named_path(path: &Path) -> Option<PathBuf> {
    let dir = match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    };
    Some(std::fs::canonicalize(dir).ok()?.join(path.file_name()?))
}

#[cfg(unix)]
fn same_file(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    a.dev() == b.dev() && a.ino() == b.ino()
}

/// Without device and inode numbers only the names tell (see
/// [`database_file_role`]).
#[cfg(not(unix))]
fn same_file(_: &std::fs::Metadata, _: &std::fs::Metadata) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_files_of_a_database_are_known_by_any_name() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("tasks.json");
        std::fs::write(&db, "[]").unwrap();
        let role = |name: &str| database_file_role(&db, &dir.path().join(name));
        assert_eq!(role("tasks.json"), Some("the database"));
        assert_eq!(role("tasks.json.backup"), Some("the database's backup"));
        assert_eq!(role("tasks.json.sync"), Some("the database's sync state"));
        assert_eq!(role("tasks.json.before_restore.2"), Some("a copy `rusk restore` kept"));
        assert_eq!(role("tasks.json.before_restore_notes.html"), None);
        assert_eq!(role("tasks.json.before_restore."), None);
        // Windows resolves `..` by the name alone, a missing directory too.
        #[cfg(unix)]
        assert_eq!(role("sub/../tasks.json"), None, "no such directory");
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        assert_eq!(role("sub/../tasks.json"), Some("the database"));
        assert_eq!(role("tasks.json.html"), None);
        assert_eq!(role("index.html"), None);
        assert_eq!(role("sub/tasks.json"), None);
        #[cfg(unix)]
        {
            std::fs::hard_link(&db, dir.path().join("hard.html")).unwrap();
            assert_eq!(role("hard.html"), Some("the database"));
            // A database named through a link keeps its siblings beside
            // the link and beside the file.
            let linked = dir.path().join("sub").join("link.json");
            std::os::unix::fs::symlink(&db, &linked).unwrap();
            let role = |path: &Path| database_file_role(&linked, path);
            assert_eq!(role(&dir.path().join("sub/link.json.backup")), Some("the database's backup"));
            assert_eq!(role(&dir.path().join("tasks.json.backup")), Some("the database's backup"));
        }
    }

    #[test]
    fn local_paths_stay_local() {
        let b = Backend::parse("/tmp/some/tasks.json").unwrap();
        assert!(matches!(b, Backend::File(_)));
        assert_eq!(b.local_path(), Some(Path::new("/tmp/some/tasks.json")));

        // A trailing slash means a directory: tasks.json is appended.
        let b = Backend::parse("/tmp/some/dir/").unwrap();
        assert_eq!(b.describe(), "/tmp/some/dir/tasks.json");
    }

    #[test]
    fn the_last_extension_selects_sqlite() {
        for name in ["/a/tasks.db", "/a/t.SQLite", "/a/t.sqlite3", "/a/tasks.db.before_restore.2"] {
            assert!(is_sqlite_path(Path::new(name)), "{name}");
        }
        for name in ["/a/tasks.db.json", "/a/tasks.dbx", "/a/db", "/a/tasks.sqlite.md"] {
            assert!(!is_sqlite_path(Path::new(name)), "{name}");
        }
    }

    #[cfg(feature = "backend-ssh")]
    #[test]
    fn ssh_location() {
        let b = Backend::parse("alex@vps:/srv/tasks/tasks.md");
        #[cfg(feature = "fmt-markdown")]
        {
            let b = b.unwrap();
            assert!(matches!(b, Backend::Ssh(_)));
            assert_eq!(b.describe(), "alex@vps:/srv/tasks/tasks.md");
            assert_eq!(b.local_path(), None);
        }
        #[cfg(not(feature = "fmt-markdown"))]
        assert!(b.is_err());
    }

    #[cfg(feature = "backend-http")]
    #[test]
    fn http_location() {
        let b = Backend::parse("https://tasks.example.com/").unwrap();
        assert!(matches!(b, Backend::Http(_)));
        assert_eq!(b.describe(), "https://tasks.example.com");
        assert_eq!(b.local_path(), None);
    }

    #[cfg(feature = "backend-sqlite")]
    #[test]
    fn sqlite_location() {
        for name in ["/a/tasks.db", "/a/tasks.sqlite", "/a/Tasks.SQLITE3"] {
            let b = Backend::parse(name).unwrap();
            assert!(matches!(b, Backend::Sqlite(_)), "{name}");
        }
    }

    #[cfg(not(feature = "backend-sqlite"))]
    #[test]
    fn sqlite_location_errors_without_the_feature() {
        let err = Backend::parse("/a/tasks.db").unwrap_err().to_string();
        assert!(err.contains("backend-sqlite"), "{err}");
    }

    #[test]
    fn aux_paths_keep_the_base_extension() {
        assert_eq!(
            aux_path(Path::new("/a/tasks.csv"), "backup"),
            PathBuf::from("/a/tasks.csv.backup")
        );
        assert_eq!(
            aux_path(Path::new("/a/tasks.json"), "sync"),
            PathBuf::from("/a/tasks.json.sync")
        );
        // No extension: no borrowed `json`, so `tasks` and `tasks.json` in
        // one directory never share a sibling.
        assert_eq!(
            aux_path(Path::new("/a/tasks"), "backup"),
            PathBuf::from("/a/tasks.backup")
        );
        assert_eq!(
            aux_path(Path::new("/a/tasks.json.before_restore"), "2"),
            PathBuf::from("/a/tasks.json.before_restore.2")
        );
    }

    #[test]
    fn durations_read_naturally() {
        assert_eq!(describe_duration(Duration::from_secs(1)), "1 second");
        assert_eq!(describe_duration(Duration::from_secs(90)), "90 seconds");
        assert_eq!(describe_duration(Duration::from_secs(3_600)), "60 minutes");
        assert_eq!(describe_duration(Duration::from_secs(7_200)), "2 hours");
        assert_eq!(describe_duration(Duration::from_secs(86_400 * 3)), "3 days");
    }

    #[test]
    fn a_backup_as_old_as_the_database_is_not_stale() {
        let now = SystemTime::now();
        let earlier = |secs| now - Duration::from_secs(secs);
        assert_eq!(stale_backup_warning(Some(earlier(5)), Some(now)), None);
        assert_eq!(stale_backup_warning(None, Some(now)), None);
        // A backup newer than the database (hand-copied) is not stale either.
        assert_eq!(stale_backup_warning(Some(now), Some(earlier(600))), None);

        let warning = stale_backup_warning(Some(earlier(3 * 86_400)), Some(now)).unwrap();
        assert!(warning.contains("3 days older"), "{warning}");
    }
}
