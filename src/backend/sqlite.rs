//! SQLite backend (`tasks.db` / `.sqlite` / `.sqlite3`), feature
//! `backend-sqlite`. One `tasks` table, whole database per save inside a
//! transaction — same load/save contract as the file formats. Concurrent
//! writers (CLI commands, `rusk serve`) serialize on the SQLite write lock:
//! a plain `save` based on a stale read is refused instead of silently
//! overwriting what another process wrote in between (see `Expected`), and
//! `update` reads the table again inside its transaction and applies the
//! change to that, so every writer's change lands.
//!
//! `pos` keeps the display order (insertion order, independent of the
//! reusable ids). Foreign writers may INSERT without `pos` — rowid
//! semantics append them at the end. `"after"` holds the comma-separated
//! ids of the tasks a task depends on (NULL when none).
//!
//! Reading changes nothing in the file: it is opened without being
//! created, no table is created and no column added — a write-protected
//! database reads fine. A file without any table is an empty database; one
//! whose tables are all another program's is refused, never furnished with
//! a `tasks` table. The write that needs it brings an older table up to
//! date (see [`Layout`]) and marks the header as rusk's, with the schema
//! it holds (`application_id`, `user_version`): a file a newer rusk wrote,
//! or another program marked as its own, is neither read nor written.
//! Deleted rows are overwritten (`secure_delete`), a save that leaves much
//! of the file free gives the space back, and `.backup` is the image SQLite
//! makes of the committed database.

use super::file::Change;
use super::{ChangeFn, StaleDatabase, Updated};
use crate::model::{Task, TaskId};
use anyhow::{Context, Result, bail};
use rusqlite::{Connection, OpenFlags, TransactionBehavior};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// How long a command waits for another process that holds the database.
const BUSY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

const SCHEMA: &str = "CREATE TABLE tasks (
    pos      INTEGER PRIMARY KEY,
    id       INTEGER NOT NULL UNIQUE CHECK (id > 0),
    text     TEXT NOT NULL,
    date     TEXT,
    done     INTEGER NOT NULL DEFAULT 0,
    priority INTEGER NOT NULL DEFAULT 0,
    \"after\" TEXT
)";

/// The header of a database rusk makes says whose file it is and what it
/// holds: `application_id` is "rusk" in ASCII, `user_version` the schema.
const APPLICATION_ID: i64 = 0x7275_736B;
/// The schema of [`SCHEMA`]. A file of a later one was written by a newer
/// rusk: this one neither reads it as its own nor writes it.
const SCHEMA_VERSION: i64 = 1;

/// `application_id` and `user_version` of the file.
fn header(conn: &Connection, at: At<'_>) -> Result<(i64, i64)> {
    let get = |pragma: &str| conn.query_row(pragma, [], |row| row.get::<_, i64>(0));
    Ok((
        get("PRAGMA application_id").map_err(at.failed("Failed to read the header"))?,
        get("PRAGMA user_version").map_err(at.failed("Failed to read the header"))?,
    ))
}

/// Inside the write transaction, once the file holds [`SCHEMA`]: the
/// header says so. A file whose `user_version` somebody set without an
/// application id keeps its header as it is — the number is theirs — and
/// so does one that holds tables of another program beside `tasks`
/// (review of R27): the file is not rusk's alone to mark.
fn stamp_header(tx: &Connection, at: At<'_>) -> Result<()> {
    let (application_id, version) = header(tx, at)?;
    let others: i64 = tx
        .query_row(
            "SELECT count(*) FROM sqlite_master
             WHERE type = 'table' AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\'
               AND lower(name) <> 'tasks'",
            [],
            |row| row.get(0),
        )
        .map_err(at.failed("Failed to look at the tables"))?;
    let unclaimed = application_id == 0 && version == 0 && others == 0;
    if unclaimed || (application_id == APPLICATION_ID && version < SCHEMA_VERSION) {
        tx.execute_batch(&format!(
            "PRAGMA application_id = {APPLICATION_ID}; PRAGMA user_version = {SCHEMA_VERSION};"
        ))
        .map_err(at.failed("Failed to write the header"))?;
    }
    Ok(())
}

/// What the file holds as far as rusk is concerned. Looked at inside the
/// transaction that reads or writes: a read takes the file as it is, a
/// write first makes it hold the current [`SCHEMA`].
#[derive(Debug, Clone, PartialEq, Eq)]
enum Layout {
    /// No table at all: a database to be — a zero-length file, or one
    /// another tool only created.
    Empty,
    /// Tables, none of them `tasks` (their names): another program's
    /// database, which rusk neither reads nor writes.
    Foreign(Vec<String>),
    /// The `tasks` table. `has_after`: tables made before dependencies
    /// existed lack that column. `narrow_ids`: the table still has the
    /// `CHECK (id BETWEEN 1 AND 255)` of the time ids were one byte, so it
    /// would refuse task 256.
    Tasks { has_after: bool, narrow_ids: bool },
}

fn layout(conn: &Connection, at: At<'_>) -> Result<Layout> {
    let tables = conn
        .prepare(
            "SELECT name, sql FROM sqlite_master
             WHERE type = 'table' AND name NOT LIKE 'sqlite\\_%' ESCAPE '\\'
             ORDER BY name",
        )
        .and_then(|mut stmt| {
            stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()
        })
        .map_err(at.failed("Failed to look at the tables"))?;
    // Table names are case-insensitive in SQL: `Tasks` is the table.
    let Some((_, sql)) = tables.iter().find(|(name, _)| name.eq_ignore_ascii_case("tasks")) else {
        return Ok(if tables.is_empty() {
            Layout::Empty
        } else {
            Layout::Foreign(tables.into_iter().map(|(name, _)| name).collect())
        });
    };
    let definition: String = sql
        .as_deref()
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_ascii_lowercase();
    Ok(Layout::Tasks {
        has_after: has_after_column(conn).map_err(at.failed("Failed to inspect the tasks table"))?,
        narrow_ids: definition.contains("check(idbetween1and255)"),
    })
}

/// Whether SQLite itself refused the file as a database (not one at all, or
/// a malformed image): as `explain` reports it, or SQLite's own answer under
/// another context. Such a file can be neither read nor copied by SQLite.
pub(super) fn rejected(error: &anyhow::Error) -> bool {
    use rusqlite::ErrorCode::{DatabaseCorrupt, NotADatabase};
    error.chain().any(|cause| {
        cause.is::<Rejected>()
            || cause
                .downcast_ref::<SqliteCause>()
                .is_some_and(|e| matches!(e.code(), Some(NotADatabase | DatabaseCorrupt)))
    })
}

/// The refusal for [`Layout::Foreign`].
fn foreign(path: &Path, tables: &[String]) -> anyhow::Error {
    const NAMED: usize = 3;
    let mut names = tables
        .iter()
        .take(NAMED)
        .map(|name| crate::printable::escape(name).into_owned())
        .collect::<Vec<_>>()
        .join(", ");
    if tables.len() > NAMED {
        names.push_str(&format!(" and {} more", tables.len() - NAMED));
    }
    anyhow::anyhow!(
        "the SQLite database '{}' has no tasks table: it belongs to another program (its tables: \
         {names}), and rusk leaves it alone — point rusk_db / RUSK_DB at a file of its own",
        path.display()
    )
}

/// What `save` must find in the database for the write to be safe: the
/// state `load` saw. The load/save contract replaces the whole table, so
/// without this check two processes that both loaded before either saved
/// would silently drop each other's changes.
#[derive(Debug, Clone, Copy)]
enum Expected {
    /// No `load` on this backend yet: the caller replaces the database
    /// unconditionally (tests, tools that write a fresh task list).
    Unchecked,
    /// `load` found no database file: `save` may only fill an empty table.
    Missing,
    /// `PRAGMA data_version` of the kept connection right after `load` (or
    /// after our own last `save`). SQLite changes it exactly when another
    /// connection commits.
    Version(i64),
}

/// The connection is kept from `load` to `save`: `data_version` is only
/// comparable within one connection. It never leaves the session, so no
/// early exit can separate it from the `expected` that belongs to it.
#[derive(Debug)]
struct Session {
    conn: Option<Connection>,
    expected: Expected,
    /// The file `conn` has open. `data_version` reports commits to that
    /// file; it says nothing when the path is made to name another file (a
    /// sync tool replacing the database by rename) — the connection would
    /// go on writing to the old, unlinked one.
    file: Option<FileId>,
}

/// Device and inode; nothing on platforms without them (there an open
/// database cannot be renamed over anyway).
type FileId = (u64, u64);

fn file_id(path: &Path) -> Option<FileId> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(path).ok().map(|meta| (meta.dev(), meta.ino()))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        None
    }
}

/// SQLite does not accept the file as a database: it is not one at all, or
/// its image is damaged. Says so in the user's terms and names the way out;
/// `rusk restore` looks for this error to know that the file cannot be
/// replaced through a connection.
#[derive(Debug)]
struct Rejected {
    path: PathBuf,
    reason: String,
}

impl std::fmt::Display for Rejected {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "'{}' is not a valid SQLite database ({}): the file appears to be corrupted, or it \
             is not a SQLite database at all.\n\
             To fix this, restore the last backup with `rusk restore` (the current file is kept \
             as a `.before_restore` copy), or move the file away to start with an empty database",
            self.path.display(),
            self.reason
        )
    }
}

impl std::error::Error for Rejected {}

/// SQLite gave up waiting for another process that holds the database.
/// Nothing was read or written.
#[derive(Debug)]
struct Locked {
    message: String,
}

impl std::fmt::Display for Locked {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for Locked {}

/// A rusqlite error as the cause in an error chain. rusqlite hands out
/// SQLite's message with the bare result code as its `source`, and a printed
/// chain would say everything twice ("file is not a database: Error code
/// 26: file is not a database"); this stops at the message.
#[derive(Debug)]
struct SqliteCause(rusqlite::Error);

impl SqliteCause {
    fn code(&self) -> Option<rusqlite::ErrorCode> {
        match &self.0 {
            rusqlite::Error::SqliteFailure(failure, _) => Some(failure.code),
            _ => None,
        }
    }
}

impl std::fmt::Display for SqliteCause {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::error::Error for SqliteCause {}

/// What a failed call was part of; decides what is worth saying about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Doing {
    Reading,
    /// Reading a `.backup` copy for `rusk restore`.
    ReadingBackup,
    /// Only here is "nothing was saved" worth saying.
    Saving,
}

/// Which database SQLite is being called for, and what for: all
/// [`explain`] needs besides the error and the step.
#[derive(Debug, Clone, Copy)]
struct At<'a> {
    path: &'a Path,
    doing: Doing,
}

impl At<'_> {
    /// For `map_err` on a rusqlite result.
    fn failed(self, step: &str) -> impl FnOnce(rusqlite::Error) -> anyhow::Error {
        move |error| explain(error, step, self)
    }
}

/// The error the user gets for a failed SQLite call. What the user can act
/// on is named as what it is, whichever step happened to hit it (every one
/// of them can be the first to find the file locked or damaged); anything
/// else names the step and the database, with SQLite's message as the cause.
fn explain(error: rusqlite::Error, step: &str, at: At<'_>) -> anyhow::Error {
    use rusqlite::ErrorCode::{
        CannotOpen, DatabaseBusy, DatabaseCorrupt, DatabaseLocked, NotADatabase, ReadOnly,
    };
    let error = SqliteCause(error);
    let path = at.path.display();
    let (what, nothing_saved) = match at.doing {
        Doing::Reading => ("database", ""),
        Doing::ReadingBackup => ("backup", ""),
        Doing::Saving => ("database", "; nothing was saved"),
    };
    match error.code() {
        // "try again", not "run the command again": `rusk serve` hands the
        // message to a web page.
        Some(DatabaseBusy | DatabaseLocked) => anyhow::Error::new(Locked {
            message: format!(
                "the {what} '{path}' is locked by another process{nothing_saved} — try again"
            ),
        }),
        Some(NotADatabase | DatabaseCorrupt) => match at.doing {
            // "Restore the last backup" would point at the very file that
            // failed.
            Doing::ReadingBackup => anyhow::anyhow!(
                "the backup '{path}' is not a valid SQLite database ({error}); \
                 refusing to replace the database with it"
            ),
            Doing::Reading | Doing::Saving => anyhow::Error::new(Rejected {
                path: at.path.to_path_buf(),
                reason: error.to_string(),
            }),
        },
        Some(ReadOnly) => anyhow::anyhow!(
            "the {what} '{path}' is read-only{nothing_saved} — check the permissions of the \
             file and of its directory"
        ),
        // SQLite's message for this one is the path once more.
        Some(CannotOpen) => anyhow::anyhow!(
            "cannot open the {what} '{path}'{nothing_saved} — check that the file and its \
             directory exist and are accessible"
        ),
        _ => anyhow::Error::new(error).context(format!("{step} (SQLite {what} '{path}')")),
    }
}

#[derive(Debug)]
pub struct SqliteBackend {
    path: PathBuf,
    session: Mutex<Session>,
}

fn has_after_column(conn: &Connection) -> rusqlite::Result<bool> {
    conn.prepare("SELECT 1 FROM pragma_table_info('tasks') WHERE name = 'after' COLLATE NOCASE")
        .and_then(|mut stmt| stmt.exists([]))
}

fn data_version(conn: &Connection, at: At<'_>) -> Result<i64> {
    conn.query_row("PRAGMA data_version", [], |row| row.get(0))
        .map_err(at.failed("Failed to read the database version"))
}

/// Inside the write transaction: does the table still hold what `load` saw?
fn is_current(tx: &Connection, expected: Expected, layout: &Layout, at: At<'_>) -> Result<bool> {
    Ok(match expected {
        Expected::Unchecked => true,
        Expected::Missing => match layout {
            Layout::Empty => true,
            // Refused as what it is by the write.
            Layout::Foreign(_) => true,
            Layout::Tasks { .. } => !tx
                .query_row("SELECT EXISTS (SELECT 1 FROM tasks)", [], |row| {
                    row.get::<_, bool>(0)
                })
                .map_err(at.failed("Failed to inspect the tasks table"))?,
        },
        Expected::Version(seen) => data_version(tx, at)? == seen,
    })
}

/// Inside the write transaction, before the rows are replaced: makes the
/// file hold the current [`SCHEMA`]. A table with the one-byte id CHECK is
/// made anew — its rows are replaced right after anyway, and SQLite cannot
/// drop a constraint in place; one that only lacks `after` gets the column.
/// Then the header says what the file now is (see [`stamp_header`]).
fn upgrade_schema(tx: &Connection, layout: &Layout, at: At<'_>) -> Result<()> {
    let sql = match layout {
        Layout::Empty => SCHEMA.to_string(),
        Layout::Tasks {
            narrow_ids: true, ..
        } => format!("DROP TABLE tasks; {SCHEMA};"),
        Layout::Tasks {
            has_after: false, ..
        } => "ALTER TABLE tasks ADD COLUMN \"after\" TEXT".to_string(),
        Layout::Tasks { .. } => String::new(),
        Layout::Foreign(_) => return Ok(()),
    };
    tx.execute_batch(&sql)
        .map_err(at.failed("Failed to bring the tasks table up to date"))?;
    stamp_header(tx, at)
}

/// The committed database as one self-contained file, made by SQLite from
/// inside a transaction of `conn`: what is committed only in a WAL is in
/// it too. The header is switched to the rollback journal (bytes 18 and 19,
/// the file format versions: 2 is WAL), so the copy is read without a WAL
/// of its own.
fn image(conn: &Connection, at: At<'_>) -> Result<Vec<u8>> {
    let mut image = conn
        .serialize(rusqlite::MAIN_DB)
        .map_err(at.failed("Failed to copy the database"))?
        .to_vec();
    if image.get(18..20) == Some(&[2, 2]) {
        image[18..20].copy_from_slice(&[1, 1]);
    }
    Ok(image)
}

/// After a commit that left a quarter of the file or more free (tasks
/// deleted): VACUUM gives the space back. Best effort — it needs the
/// database to itself, and a reader of another process makes it fail at
/// once instead of waiting; the next save tries again.
fn compact(conn: &Connection) {
    let pages = |pragma: &str| conn.query_row(pragma, [], |row| row.get::<_, i64>(0));
    let (Ok(free), Ok(total)) = (pages("PRAGMA freelist_count"), pages("PRAGMA page_count")) else {
        return;
    };
    if free == 0 || free * 4 < total {
        return;
    }
    if conn.busy_timeout(std::time::Duration::ZERO).is_ok() {
        let _ = conn.execute_batch("VACUUM");
    }
    let _ = conn.busy_timeout(BUSY_TIMEOUT);
}

fn replace_rows(tx: &Connection, tasks: &[Task], at: At<'_>) -> Result<()> {
    tx.execute("DELETE FROM tasks", [])
        .map_err(at.failed("Failed to clear the tasks table"))?;
    let mut stmt = tx
        .prepare(
            "INSERT INTO tasks (pos, id, text, date, done, priority, \"after\")
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        )
        .map_err(at.failed("Failed to prepare the insert"))?;
    for (pos, task) in tasks.iter().enumerate() {
        let after = if task.after.is_empty() {
            None
        } else {
            Some(
                task.after
                    .iter()
                    .map(|id| id.to_string())
                    .collect::<Vec<_>>()
                    .join(","),
            )
        };
        stmt.execute(rusqlite::params![
            pos as i64 + 1,
            task.id,
            task.text,
            task.date.map(|d| d.to_string()),
            task.done,
            task.priority,
            after,
        ])
        .map_err(at.failed(&format!("Failed to insert task {}", task.id)))?;
    }
    Ok(())
}

impl SqliteBackend {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            session: Mutex::new(Session {
                conn: None,
                expected: Expected::Unchecked,
                file: None,
            }),
        }
    }

    fn session(&self) -> std::sync::MutexGuard<'_, Session> {
        self.session.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn at(&self, doing: Doing) -> At<'_> {
        At {
            path: &self.path,
            doing,
        }
    }

    /// A connection that reads or writes. Only a save creates the file; a
    /// write-protected one SQLite opens read-only, so it reads and a save
    /// says what is wrong. Nothing reads the file yet: a file that is not a
    /// database, or a locked one, is found out by the first statement.
    fn open(&self, doing: Doing) -> Result<Connection> {
        let at = self.at(doing);
        let mut flags = OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        if doing == Doing::Saving {
            flags |= OpenFlags::SQLITE_OPEN_CREATE;
        }
        let conn = Connection::open_with_flags(&self.path, flags)
            .map_err(at.failed("Failed to open the file"))?;
        conn.busy_timeout(BUSY_TIMEOUT)
            .map_err(at.failed("Failed to set up the connection"))?;
        // Deleted rows are overwritten with zeros: the text of a deleted
        // task stays neither in the file nor in the copies made of it.
        conn.pragma_update(None, "secure_delete", true)
            .map_err(at.failed("Failed to set up the connection"))?;
        Ok(conn)
    }

    /// What the header says about the file: one of a newer rusk, or of
    /// another program that marked it as its own, is refused — read and
    /// written alike. A file without an application id is taken on its
    /// tables (an older rusk wrote none).
    fn check_header(&self, conn: &Connection, at: At<'_>) -> Result<()> {
        let (application_id, version) = header(conn, at)?;
        // The header holds 32 bits; SQLite hands them over signed.
        let id = application_id as u32;
        let backup = at.doing == Doing::ReadingBackup;
        if application_id == APPLICATION_ID && version > SCHEMA_VERSION {
            if backup {
                bail!(
                    "the backup '{}' was written by a newer rusk (schema {version}; this rusk \
                     knows schema {SCHEMA_VERSION}); refusing to replace the database with it",
                    self.path.display()
                );
            }
            bail!(
                "the SQLite database '{}' was written by a newer rusk (schema {version}; this \
                 rusk knows schema {SCHEMA_VERSION}): upgrade rusk to use it",
                self.path.display()
            );
        }
        if application_id != 0 && application_id != APPLICATION_ID {
            if backup {
                bail!(
                    "the backup '{}' belongs to another program (its application id is \
                     {id:#010x}): it is not a rusk database; refusing to replace the database \
                     with it",
                    self.path.display()
                );
            }
            bail!(
                "the SQLite database '{}' belongs to another program (its application id is \
                 {id:#010x}), and rusk leaves it alone — point rusk_db / RUSK_DB at a file of \
                 its own",
                self.path.display()
            );
        }
        Ok(())
    }

    /// The rows of the table `layout` describes; another program's database
    /// is refused.
    fn read_layout(&self, conn: &Connection, layout: &Layout, at: At<'_>) -> Result<Vec<Task>> {
        match layout {
            Layout::Empty => Ok(Vec::new()),
            Layout::Foreign(tables) => Err(foreign(&self.path, tables)),
            Layout::Tasks { has_after, .. } => self.read_tasks(conn, *has_after, at),
        }
    }

    /// For `rusk restore`: the image SQLite makes of the database as it is
    /// now (see [`image`]), to keep as `.before_restore` — also when `load`
    /// failed on a row rusk cannot read, as long as SQLite can read the
    /// file; `None` for a file of no pages, whose bytes are the copy. The
    /// save that follows is checked against exactly this state: if another
    /// process commits in between, the restore is refused rather than
    /// replacing what the copy does not hold. A locked database is an error.
    pub(super) fn image(&self) -> Result<Option<Vec<u8>>> {
        let at = self.at(Doing::Reading);
        let mut session = self.session();
        if session.conn.is_none() {
            session.conn = Some(self.open(Doing::Reading)?);
            session.file = file_id(&self.path);
        }
        let conn = session.conn.as_ref().expect("opened above");
        let tx = conn
            .unchecked_transaction()
            .map_err(at.failed("Failed to start a read transaction"))?;
        let pages: i64 = tx
            .query_row("PRAGMA page_count", [], |row| row.get(0))
            .map_err(at.failed("Failed to read the size of the database"))?;
        let copy = if pages > 0 { Some(image(&tx, at)?) } else { None };
        let version = data_version(&tx, at)?;
        tx.commit()
            .map_err(at.failed("Failed to finish the read transaction"))?;
        session.expected = Expected::Version(version);
        Ok(copy)
    }

    /// Reads a `.backup` copy for `rusk restore` without changing a byte of
    /// it: opened read-only, never created or migrated, and accepted only
    /// when it really holds a `tasks` table. (A zero-length file is a valid
    /// empty SQLite database; opened the usual way it would get the schema
    /// written into it and then "restore" zero tasks over a healthy
    /// database.)
    pub fn load_backup(&self) -> Result<Vec<Task>> {
        let at = self.at(Doing::ReadingBackup);
        let conn = Connection::open_with_flags(
            &self.path,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(at.failed("Failed to open the file"))?;
        // What the header says first: a backup of a later schema may have
        // no `tasks` table at all.
        self.check_header(&conn, at)?;
        let has_tasks_table = conn
            .prepare(
                "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'tasks' COLLATE NOCASE",
            )
            .and_then(|mut stmt| stmt.exists([]))
            .map_err(at.failed("Failed to look for the tasks table"))?;
        if !has_tasks_table {
            bail!(
                "the backup '{}' has no tasks table: it is not a rusk database; \
                 refusing to replace the database with it",
                self.path.display()
            );
        }
        // The copy was taken from a live file: make sure it is not a torn
        // mix of pages before trusting what it decodes to.
        let integrity: String = conn
            .query_row("PRAGMA quick_check", [], |row| row.get(0))
            .map_err(at.failed("Failed to check the integrity"))?;
        if integrity != "ok" {
            bail!(
                "the backup '{}' fails the SQLite integrity check ({integrity}); \
                 refusing to replace the database with it",
                self.path.display()
            );
        }
        let has_after = has_after_column(&conn)
            .map_err(at.failed("Failed to inspect the tasks table"))?;
        self.read_tasks(&conn, has_after, at)
    }

    /// For `rusk restore` over a database that failed to load: when SQLite
    /// itself rejects the file (not a database, malformed image) it cannot
    /// be replaced through a connection, so it is emptied — a zero-length
    /// file is a valid empty SQLite database — and the save that follows
    /// fills it. Emptying instead of deleting keeps the inode: the mode, the
    /// owner and a symlinked database path survive. Any other failure (a
    /// busy or unreadable file, a bad row) leaves the file alone.
    ///
    /// The caller has already kept a byte copy at `kept`; the journal/WAL
    /// of the rejected file would be replayed into the new one, so they are
    /// moved next to that copy, where SQLite expects them for it.
    ///
    /// Returns whether the file was emptied.
    pub(super) fn reset_if_rejected_by_sqlite(
        &self,
        load_error: &anyhow::Error,
        kept: &Path,
    ) -> Result<bool> {
        if !rejected(load_error) {
            return Ok(false);
        }
        *self.session() = Session {
            conn: None,
            expected: Expected::Unchecked,
            file: None,
        };
        // SQLite names the sidecars after the real file, not after a link.
        let real = std::fs::canonicalize(&self.path).unwrap_or_else(|_| self.path.clone());
        let with_suffix = |path: &Path, suffix: &str| {
            let mut name = path.as_os_str().to_os_string();
            name.push(suffix);
            PathBuf::from(name)
        };
        const SIDECARS: [&str; 3] = ["-journal", "-wal", "-shm"];
        for suffix in SIDECARS {
            let sidecar = with_suffix(&real, suffix);
            if sidecar.exists() {
                crate::atomic::copy_to_new_file(&sidecar, &with_suffix(kept, suffix))
                    .with_context(|| format!("Failed to keep a copy of '{}'", sidecar.display()))?;
            }
        }
        std::fs::File::options()
            .write(true)
            .truncate(true)
            .open(&real)
            .and_then(|file| file.sync_all())
            .with_context(|| {
                format!("Failed to reset the unreadable database '{}'", self.path.display())
            })?;
        for suffix in SIDECARS {
            let _ = std::fs::remove_file(with_suffix(&real, suffix));
        }
        Ok(true)
    }

    pub fn load(&self) -> Result<Vec<Task>> {
        Ok(self.read()?.tasks)
    }

    pub(super) fn read(&self) -> Result<super::Loaded> {
        let mut session = self.session();
        // Not there at all is an empty database to be; a file that cannot
        // even be looked at (no permission on its directory) is an error.
        if !super::exists(&self.path)? {
            *session = Session {
                conn: None,
                expected: Expected::Missing,
                file: None,
            };
            return Ok(super::Loaded {
                tasks: Vec::new(),
                missing: true,
            });
        }
        let at = self.at(Doing::Reading);
        let conn = self.open(Doing::Reading)?;
        // One read transaction: the version belongs to exactly the snapshot
        // the tasks are read from.
        let (tasks, version) = {
            let tx = conn
                .unchecked_transaction()
                .map_err(at.failed("Failed to start a read transaction"))?;
            self.check_header(&tx, at)?;
            let tasks = self.read_layout(&tx, &layout(&tx, at)?, at)?;
            let version = data_version(&tx, at)?;
            tx.commit()
                .map_err(at.failed("Failed to finish the read transaction"))?;
            (tasks, version)
        };
        *session = Session {
            conn: Some(conn),
            expected: Expected::Version(version),
            file: file_id(&self.path),
        };
        Ok(super::Loaded {
            tasks,
            missing: false,
        })
    }

    /// The rows as tasks that follow the rules ([`super::normalized`]): a
    /// table another tool wrote need not have rusk's constraints. A NULL
    /// reads like a JSON `null` — no id, a flag that is not set — except in
    /// `text`, which a task must have. `has_after` is false for a table made
    /// before dependencies existed: reading never adds the column.
    fn read_tasks(&self, conn: &Connection, has_after: bool, at: At<'_>) -> Result<Vec<Task>> {
        let after = if has_after { "\"after\"" } else { "NULL" };
        let mut stmt = conn
            .prepare(&format!(
                "SELECT pos, id, text, date, done, priority, {after} FROM tasks ORDER BY pos"
            ))
            .map_err(at.failed("Failed to query the tasks table"))?;
        // A row that SQLite hands over but that is not a task (a NULL text,
        // a date that is not one): the cause names the row and the column.
        let not_a_task = || {
            format!(
                "Failed to parse the SQLite database at '{}'",
                self.path.display()
            )
        };
        let rows = stmt
            .query_map([], |row| {
                // Each column on its own, so that one that is not what a
                // task has is told together with the row it is in.
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<TaskId>>(1),
                    row.get::<_, String>(2),
                    row.get::<_, Option<String>>(3),
                    row.get::<_, Option<bool>>(4),
                    row.get::<_, Option<bool>>(5),
                    row.get::<_, Option<String>>(6),
                ))
            })
            .map_err(at.failed("Failed to read the tasks table"))?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|error| match error {
                // The file itself failed half-way through the rows.
                rusqlite::Error::SqliteFailure(..) => {
                    explain(error, "Failed to read the tasks table", at)
                }
                _ => anyhow::Error::new(SqliteCause(error)).context(not_a_task()),
            })?;
        let tasks = rows
            .into_iter()
            .map(|(pos, id, text, date, done, priority, after)| {
                let row = match &id {
                    Ok(Some(id)) => format!("row {pos} (id {id})"),
                    _ => format!("row {pos}"),
                };
                let task = || -> Result<Task> {
                    let date = date
                        .map_err(SqliteCause)?
                        .map(|d| {
                            d.parse::<chrono::NaiveDate>()
                                .with_context(|| format!("invalid date '{d}'"))
                        })
                        .transpose()?;
                    let after = after
                        .map_err(SqliteCause)?
                        .unwrap_or_default()
                        .split(',')
                        .filter(|s| !s.trim().is_empty())
                        .map(|s| {
                            s.trim()
                                .parse::<TaskId>()
                                .with_context(|| format!("invalid after id '{s}'"))
                        })
                        .collect::<Result<Vec<_>>>()?;
                    Ok(Task {
                        id: id.map_err(SqliteCause)?.unwrap_or(0),
                        text: text.map_err(SqliteCause)?,
                        date,
                        done: done.map_err(SqliteCause)?.unwrap_or(false),
                        priority: priority.map_err(SqliteCause)?.unwrap_or(false),
                        after,
                    })
                };
                task().context(row)
            })
            .collect::<Result<Vec<_>>>()
            .with_context(not_a_task)?;
        super::normalized(tasks, &self.path.display().to_string())
    }

    /// Replaces the table with `tasks`, unless another connection committed
    /// since this backend read it: that is a [`StaleDatabase`] error and
    /// nothing is written.
    pub fn save(&self, tasks: &[Task]) -> Result<()> {
        self.write(tasks, Change::Update)
    }

    fn create_parent_dir(&self) -> Result<()> {
        if let Some(parent) = self.path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create the directory '{}' for the database file",
                    parent.display()
                )
            })?;
        }
        Ok(())
    }

    /// The kept connection, or a new one when there is none — or when the
    /// path no longer names the file the kept one has open. In that case
    /// the returned flag is set: whatever this backend loaded came from
    /// another file, so it is not current, whatever `data_version` says.
    fn connect(&self, session: &mut Session) -> Result<bool> {
        let replaced = session.conn.is_some() && file_id(&self.path) != session.file;
        if replaced {
            session.conn = None;
        }
        if session.conn.is_none() {
            session.conn = Some(self.open(Doing::Saving)?);
            session.file = file_id(&self.path);
        }
        Ok(replaced)
    }

    pub(super) fn write(&self, tasks: &[Task], change: Change) -> Result<()> {
        self.create_parent_dir()?;
        let at = self.at(Doing::Saving);
        let mut session = self.session();
        let replaced = self.connect(&mut session)?;
        let Session { conn, expected, .. } = &mut *session;
        let conn = conn.as_mut().expect("connected above");
        // IMMEDIATE takes the write lock up front: concurrent savers queue
        // here (busy timeout) and the staleness check below runs with no
        // other writer able to slip in before the commit.
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(at.failed("Failed to start a transaction"))?;
        self.check_header(&tx, at)?;
        let layout = layout(&tx, at)?;
        if replaced || !is_current(&tx, *expected, &layout, at)? {
            return Err(StaleDatabase::at(self.path.display()));
        }
        self.replace(&tx, tasks, change, &layout)?;
        tx.commit().map_err(at.failed("Failed to commit the save"))?;
        compact(conn);
        // Our own commit does not move our data_version; re-read it anyway
        // so the next save on this backend checks against this state.
        *expected = Expected::Version(data_version(conn, at)?);
        Ok(())
    }

    /// Applies `change` to the task list and stores the result inside one
    /// IMMEDIATE transaction. `snapshot` is the base while the table still
    /// holds what it was loaded from; otherwise the table is read again,
    /// inside the same transaction, and the change applies to that — unless
    /// the snapshot carries edits of its own (`snapshot_is_clean` is
    /// false), which a fresh read would drop.
    pub(super) fn update<T>(
        &self,
        snapshot: &[Task],
        snapshot_is_clean: bool,
        change: ChangeFn<'_, T>,
    ) -> Result<Updated<T>> {
        self.create_parent_dir()?;
        let at = self.at(Doing::Saving);
        let mut session = self.session();
        let replaced = self.connect(&mut session)?;
        let Session { conn, expected, .. } = &mut *session;
        let conn = conn.as_mut().expect("connected above");
        let tx = conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(at.failed("Failed to start a transaction"))?;
        self.check_header(&tx, at)?;
        let layout = layout(&tx, at)?;
        let reread = if !replaced && is_current(&tx, *expected, &layout, at)? {
            None
        } else if snapshot_is_clean {
            Some(self.read_layout(&tx, &layout, at)?)
        } else {
            return Err(StaleDatabase::at(self.path.display()));
        };
        let (tasks, value, changed) = super::apply(reread.as_deref().unwrap_or(snapshot), change)?;
        let stored = changed || reread.is_some() || snapshot_is_clean;
        if changed {
            self.replace(&tx, &tasks, Change::Update, &layout)?;
        }
        tx.commit().map_err(at.failed("Failed to commit the save"))?;
        if changed {
            compact(conn);
        }
        *expected = Expected::Version(data_version(conn, at)?);
        Ok(Updated {
            tasks,
            value,
            stored,
        })
    }

    /// The write itself, inside the IMMEDIATE transaction and after the
    /// staleness check; `layout` is what the file holds. Another program's
    /// database is refused. A regular save of a database that has tasks
    /// refreshes `.backup` first: with the write lock held and nothing of
    /// this transaction written yet, SQLite's image of it is the last
    /// committed state, and no writer can be half-way through changing it.
    /// Then the schema is brought up to date and the rows are replaced.
    fn replace(&self, tx: &Connection, tasks: &[Task], change: Change, layout: &Layout) -> Result<()> {
        let at = self.at(Doing::Saving);
        match layout {
            Layout::Foreign(tables) => return Err(foreign(&self.path, tables)),
            Layout::Tasks { .. } if matches!(change, Change::Update) => {
                super::refresh_backup_from_image(&self.path, || image(tx, at));
            }
            Layout::Tasks { .. } | Layout::Empty => {}
        }
        upgrade_schema(tx, layout, at)?;
        replace_rows(tx, tasks, at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn roundtrip_preserves_order_and_fields() {
        let dir = tempfile::tempdir().unwrap();
        let backend = SqliteBackend::new(dir.path().join("tasks.db"));

        assert_eq!(backend.load().unwrap(), Vec::<Task>::new());

        // Insertion order differs from id order: id 1 was reused after 2.
        let tasks = vec![
            Task {
                id: 2,
                text: "first\nmulti-line".into(),
                date: NaiveDate::from_ymd_opt(2026, 7, 15),
                done: false,
                priority: true, after: Vec::new(),
            },
            Task {
                id: 1,
                text: "second".into(),
                date: None,
                done: true,
                priority: false, after: Vec::new(),
            },
        ];
        backend.save(&tasks).unwrap();
        assert_eq!(backend.load().unwrap(), tasks);

        // Saving again replaces, not appends.
        backend.save(&tasks[..1]).unwrap();
        assert_eq!(backend.load().unwrap().len(), 1);
    }

    fn task(id: TaskId, text: &str) -> Task {
        Task {
            id,
            text: text.into(),
            date: None,
            done: false,
            priority: false,
            after: Vec::new(),
        }
    }

    /// Two writers that both loaded before either saved: the second save
    /// must be refused, not silently drop the first writer's task.
    #[test]
    fn stale_save_is_refused_instead_of_losing_updates() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.db");
        SqliteBackend::new(path.clone())
            .save(&[task(1, "seed")])
            .unwrap();

        let a = SqliteBackend::new(path.clone());
        let b = SqliteBackend::new(path.clone());
        let mut tasks_a = a.load().unwrap();
        let mut tasks_b = b.load().unwrap();

        tasks_a.push(task(2, "from a"));
        a.save(&tasks_a).unwrap();

        tasks_b.push(task(2, "from b"));
        let err = b.save(&tasks_b).unwrap_err().to_string();
        assert!(err.contains("changed by another process"), "{err}");

        let on_disk = SqliteBackend::new(path).load().unwrap();
        assert_eq!(on_disk, tasks_a, "the refused save must not write anything");

        // After re-reading, the same writer succeeds.
        let mut fresh = b.load().unwrap();
        fresh.push(task(3, "from b"));
        b.save(&fresh).unwrap();
        assert_eq!(b.load().unwrap().len(), 3);
    }

    /// Both writers saw "no database yet": only the first may create it.
    #[test]
    fn stale_save_is_refused_when_the_database_appeared_meanwhile() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.db");
        let a = SqliteBackend::new(path.clone());
        let b = SqliteBackend::new(path.clone());
        assert!(a.load().unwrap().is_empty());
        assert!(b.load().unwrap().is_empty());

        a.save(&[task(1, "from a")]).unwrap();
        assert!(b.save(&[task(1, "from b")]).is_err());
        assert_eq!(SqliteBackend::new(path).load().unwrap()[0].text, "from a");
    }

    /// Consecutive saves of one writer are never stale against themselves.
    #[test]
    fn repeated_saves_of_one_writer_succeed() {
        let dir = tempfile::tempdir().unwrap();
        let backend = SqliteBackend::new(dir.path().join("tasks.db"));
        let mut tasks = backend.load().unwrap();
        for id in 1..=3 {
            tasks.push(task(id, "t"));
            backend.save(&tasks).unwrap();
        }
        assert_eq!(backend.load().unwrap().len(), 3);
    }

    /// Contention is normal now; the error has to say that it was one.
    #[test]
    fn a_busy_database_is_reported_as_locked() {
        let busy = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_BUSY),
            None,
        );
        let saving = At {
            path: Path::new("/a/tasks.db"),
            doing: Doing::Saving,
        };
        let message = explain(busy, "Failed to start a transaction", saving);
        assert!(message.to_string().contains("locked by another process"), "{message}");
        assert!(message.is::<Locked>());

        let other = rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_FULL),
            None,
        );
        let message = explain(other, "Failed to commit the save", saving);
        assert_eq!(
            message.to_string(),
            "Failed to commit the save (SQLite database '/a/tasks.db')"
        );
        // The cause is kept for whoever prints the chain.
        assert!(format!("{message:#}").contains("full"), "{message:#}");
    }

    /// Whichever step hits them, the conditions a user can act on read the
    /// same; "nothing was saved" is said only of a save.
    #[test]
    fn explain_names_what_the_user_can_act_on() {
        let failure = |code| {
            rusqlite::Error::SqliteFailure(rusqlite::ffi::Error::new(code), None)
        };
        let at = |doing| At {
            path: Path::new("/a/tasks.db"),
            doing,
        };

        let reading = explain(failure(rusqlite::ffi::SQLITE_BUSY), "step", at(Doing::Reading));
        assert!(reading.to_string().contains("locked by another process"), "{reading}");
        assert!(!reading.to_string().contains("nothing was saved"), "{reading}");
        let saving = explain(failure(rusqlite::ffi::SQLITE_BUSY), "step", at(Doing::Saving));
        assert!(saving.to_string().contains("nothing was saved"), "{saving}");

        let read_only = explain(failure(rusqlite::ffi::SQLITE_READONLY), "step", at(Doing::Saving));
        assert!(read_only.to_string().contains("'/a/tasks.db' is read-only"), "{read_only}");

        for code in [rusqlite::ffi::SQLITE_NOTADB, rusqlite::ffi::SQLITE_CORRUPT] {
            let rejected = explain(failure(code), "step", at(Doing::Reading));
            assert!(rejected.is::<Rejected>(), "{rejected}");
            assert!(rejected.to_string().contains("rusk restore"), "{rejected}");
            // Not for the backup itself: restoring is what just failed.
            let backup = explain(failure(code), "step", at(Doing::ReadingBackup));
            assert!(!backup.is::<Rejected>(), "{backup}");
            assert!(!format!("{backup:#}").contains("rusk restore"), "{backup:#}");
            assert!(format!("{backup:#}").contains("the backup '/a/tasks.db'"), "{backup:#}");
        }

        // A locked backup is locked, not "not a database".
        let backup = explain(failure(rusqlite::ffi::SQLITE_BUSY), "step", at(Doing::ReadingBackup));
        assert!(backup.to_string().contains("the backup '/a/tasks.db' is locked"), "{backup}");

        // SQLite's own message for a file it cannot open is the path again.
        let cannot_open = explain(failure(rusqlite::ffi::SQLITE_CANTOPEN), "step", at(Doing::Reading));
        assert_eq!(cannot_open.to_string().matches("/a/tasks.db").count(), 1, "{cannot_open}");
    }

    #[test]
    fn after_ids_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let backend = SqliteBackend::new(dir.path().join("tasks.db"));
        let tasks = vec![
            task(1, "first"),
            task(2, "second"),
            Task {
                after: vec![2, 1],
                ..task(3, "blocked")
            },
        ];
        backend.save(&tasks).unwrap();
        assert_eq!(backend.load().unwrap(), tasks);
    }

    /// The rows another tool wrote are held to the rules like any other
    /// list: here a NULL flag, a shared id and a dependency on nothing.
    #[test]
    fn rows_without_the_constraints_load_as_a_valid_list() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.db");
        Connection::open(&path)
            .unwrap()
            .execute_batch(
                "CREATE TABLE tasks (pos INTEGER PRIMARY KEY, id INTEGER, text TEXT,
                                     date TEXT, done INTEGER, priority INTEGER, \"after\" TEXT);
                 INSERT INTO tasks (pos, id, text, done, priority, \"after\") VALUES
                     (1, 1, 'a', NULL, 0, NULL), (2, 1, 'b', 1, NULL, '8');",
            )
            .unwrap();
        let loaded = SqliteBackend::new(path).load().unwrap();
        assert_eq!(
            loaded,
            [
                task(1, "a"),
                Task {
                    done: true,
                    ..task(2, "b")
                }
            ]
        );
    }

    /// The tables of the time before dependencies and wide ids: reading
    /// leaves them as they are, the first save makes them current.
    #[test]
    fn older_tables_are_brought_up_to_date_by_the_first_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.db");
        let old = "CREATE TABLE tasks (
                    pos      INTEGER PRIMARY KEY,
                    id       INTEGER NOT NULL UNIQUE CHECK (id BETWEEN 1 AND 255),
                    text     TEXT NOT NULL,
                    date     TEXT,
                    done     INTEGER NOT NULL DEFAULT 0,
                    priority INTEGER NOT NULL DEFAULT 0
                )";
        let definition = || -> String {
            Connection::open(&path)
                .unwrap()
                .query_row("SELECT sql FROM sqlite_master WHERE name = 'tasks'", [], |row| {
                    row.get(0)
                })
                .unwrap()
        };
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute(old, []).unwrap();
            conn.execute(
                "INSERT INTO tasks (pos, id, text) VALUES (1, 1, 'legacy'), (2, 7, 'other')",
                [],
            )
            .unwrap();
        }
        let backend = SqliteBackend::new(path.clone());
        let loaded = backend.load().unwrap();
        assert_eq!(loaded[0].text, "legacy");
        assert_eq!(loaded[0].after, Vec::<TaskId>::new());
        assert_eq!(definition(), old, "reading changed the schema");

        let mut tasks = loaded;
        tasks[0].after = vec![7];
        tasks.push(task(300, "wide"));
        backend.save(&tasks).unwrap();
        assert_eq!(backend.load().unwrap(), tasks);
        assert_eq!(definition(), SCHEMA);
    }

    /// A table that only lacks `after` (another tool made it) keeps what
    /// else it has: the column is added, the table is not made anew.
    #[test]
    fn a_table_without_after_gets_the_column() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.db");
        Connection::open(&path)
            .unwrap()
            .execute_batch(
                "CREATE TABLE tasks (pos INTEGER PRIMARY KEY, id INTEGER, text TEXT,
                                     date TEXT, done INTEGER, priority INTEGER, note TEXT);
                 INSERT INTO tasks (pos, id, text, note) VALUES (1, 1, 'a', 'kept');",
            )
            .unwrap();
        let backend = SqliteBackend::new(path.clone());
        let mut tasks = backend.load().unwrap();
        tasks[0].after = Vec::new();
        tasks.push(Task { after: vec![1], ..task(2, "b") });
        backend.save(&tasks).unwrap();
        assert_eq!(backend.load().unwrap(), tasks);
        let columns: Vec<String> = Connection::open(&path)
            .unwrap()
            .prepare("SELECT name FROM pragma_table_info('tasks')")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        assert!(columns.contains(&"note".to_string()), "{columns:?}");
        assert!(columns.contains(&"after".to_string()), "{columns:?}");
    }

    #[test]
    fn layout_tells_the_kinds_of_file_apart() {
        let at = At {
            path: Path::new("/a/tasks.db"),
            doing: Doing::Reading,
        };
        let of = |sql: &str| {
            let conn = Connection::open_in_memory().unwrap();
            conn.execute_batch(sql).unwrap();
            layout(&conn, at).unwrap()
        };
        assert_eq!(of(""), Layout::Empty);
        // SQLite's own tables are no sign of another program.
        assert_eq!(of("CREATE TABLE t (id INTEGER PRIMARY KEY AUTOINCREMENT); DROP TABLE t;"), Layout::Empty);
        assert_eq!(
            of("CREATE TABLE b (x); CREATE TABLE a (y);"),
            Layout::Foreign(vec!["a".into(), "b".into()])
        );
        assert_eq!(
            of(SCHEMA),
            Layout::Tasks {
                has_after: true,
                narrow_ids: false
            }
        );
        assert!(matches!(of("CREATE TABLE TASKS (id, text)"), Layout::Tasks { .. }));
        // The CHECK as the old SCHEMA wrote it, and as a hand would.
        for check in ["CHECK (id BETWEEN 1 AND 255)", "check(ID between 1 and\n 255)"] {
            assert_eq!(
                of(&format!("CREATE TABLE tasks (pos INTEGER PRIMARY KEY, id INTEGER {check}, text TEXT)")),
                Layout::Tasks {
                    has_after: false,
                    narrow_ids: true
                },
                "{check}"
            );
        }
    }

    #[test]
    fn another_programs_tables_are_named_escaped_and_counted() {
        let tables: Vec<String> = ["a", "b\x1b[31m", "c", "d", "e"].map(Into::into).into();
        let message = foreign(Path::new("/a/x.db"), &tables).to_string();
        assert!(message.contains("'/a/x.db' has no tasks table"), "{message}");
        assert!(message.contains("a, b\\x1b[31m, c and 2 more"), "{message}");
        assert!(!message.contains('\x1b'), "{message}");
    }

    /// The image is the committed database, WAL included, and is read on
    /// its own.
    #[test]
    fn the_image_of_a_wal_database_needs_no_wal() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.db");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch(&format!(
            "PRAGMA journal_mode = WAL; PRAGMA wal_autocheckpoint = 0; {SCHEMA};
             INSERT INTO tasks (pos, id, text) VALUES (1, 1, 'in the wal');"
        ))
        .unwrap();
        let at = At {
            path: &path,
            doing: Doing::Saving,
        };
        let copy = image(&conn, at).unwrap();
        assert_eq!(&copy[18..20], &[1, 1]);
        let copy_path = dir.path().join("copy.db");
        std::fs::write(&copy_path, &copy).unwrap();
        let loaded = SqliteBackend::new(copy_path.clone()).load_backup().unwrap();
        assert_eq!(loaded, [task(1, "in the wal")]);
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .filter(|n| n.starts_with("copy.db"))
            .collect();
        assert_eq!(names, ["copy.db"], "reading the copy needed files of its own");
    }

    /// The copy `rusk restore` keeps is the state the restore may replace:
    /// a commit of another process after it makes the restore stale.
    #[test]
    fn a_restore_is_checked_against_the_state_it_copied() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.db");
        SqliteBackend::new(path.clone()).save(&[task(1, "one")]).unwrap();
        // No load before (it may have failed): the image opens its own.
        let backend = SqliteBackend::new(path.clone());
        assert!(backend.image().unwrap().is_some());
        Connection::open(&path)
            .unwrap()
            .execute("INSERT INTO tasks (pos, id, text) VALUES (2, 2, 'late')", [])
            .unwrap();
        let err = backend.write(&[task(1, "restored")], Change::Restore).unwrap_err();
        assert!(err.is::<StaleDatabase>(), "{err:#}");

        let backend = SqliteBackend::new(path.clone());
        backend.image().unwrap();
        backend.write(&[task(1, "restored")], Change::Restore).unwrap();
        assert_eq!(SqliteBackend::new(path).load().unwrap(), [task(1, "restored")]);

        // A file of no pages has only its bytes to keep.
        let empty = dir.path().join("empty.db");
        std::fs::write(&empty, "").unwrap();
        assert_eq!(SqliteBackend::new(empty).image().unwrap(), None);
    }

    fn file_holds(path: &Path, needle: &str) -> bool {
        std::fs::read(path)
            .unwrap()
            .windows(needle.len())
            .any(|w| w == needle.as_bytes())
    }

    /// A deleted row is overwritten even when too little is freed for a
    /// VACUUM: that is `secure_delete`, not the compaction.
    #[test]
    fn a_deleted_text_is_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.db");
        let backend = SqliteBackend::new(path.clone());
        let mut tasks: Vec<Task> = (1..=40).map(|id| task(id, &"x".repeat(900))).collect();
        tasks[20].text = "SECRET-deleted-text".into();
        backend.save(&tasks).unwrap();
        let pages = || std::fs::metadata(&path).unwrap().len() / 4096;
        let before = pages();
        tasks.remove(20);
        backend.save(&tasks).unwrap();
        assert_eq!(pages(), before, "the file was compacted; this is to test the overwrite");
        assert!(!file_holds(&path, "SECRET-deleted-text"));
    }

    /// Deleting most of the tasks gives the space back.
    #[test]
    fn a_save_that_frees_much_of_the_file_compacts_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.db");
        let backend = SqliteBackend::new(path.clone());
        let tasks: Vec<Task> = (1..=40).map(|id| task(id, &"x".repeat(900))).collect();
        backend.save(&tasks).unwrap();
        let full = std::fs::metadata(&path).unwrap().len();
        backend.save(&tasks[..2]).unwrap();
        let small = std::fs::metadata(&path).unwrap().len();
        assert!(small * 4 < full, "{small} of {full} bytes");
        assert_eq!(backend.load().unwrap(), tasks[..2]);
        // The connection goes on working after the VACUUM.
        backend.save(&tasks[..3]).unwrap();
        assert_eq!(SqliteBackend::new(path).load().unwrap(), tasks[..3]);
    }

    /// A reader elsewhere keeps the VACUUM from running, and the save does
    /// not wait for it: the next one gives the space back.
    #[test]
    fn a_reader_elsewhere_does_not_hold_up_the_compaction() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.db");
        let backend = SqliteBackend::new(path.clone());
        let tasks: Vec<Task> = (1..=40).map(|id| task(id, &"x".repeat(900))).collect();
        backend.save(&tasks).unwrap();
        let conn = Connection::open(&path).unwrap();
        conn.execute("DELETE FROM tasks WHERE id > 2", []).unwrap();
        let full = std::fs::metadata(&path).unwrap().len();

        let reader = Connection::open(&path).unwrap();
        reader.execute_batch("BEGIN; SELECT count(*) FROM tasks;").unwrap();
        let started = std::time::Instant::now();
        compact(&conn);
        assert!(started.elapsed() < std::time::Duration::from_secs(1), "{:?}", started.elapsed());
        assert_eq!(std::fs::metadata(&path).unwrap().len(), full);

        reader.execute_batch("COMMIT").unwrap();
        compact(&conn);
        assert!(std::fs::metadata(&path).unwrap().len() * 4 < full);
    }
}
