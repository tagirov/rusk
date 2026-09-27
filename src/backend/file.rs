//! The default backend: a local file, whole database per save, replaced
//! atomically (see [`crate::atomic`]: unique temp sibling, fsync, rename;
//! mode, owner and a symlinked database path survive the save). The file
//! extension picks the format (see [`crate::codec`]). With the
//! `backend-git` feature and `git_backend = true` every save is also
//! committed to a git repository in the database directory.
//!
//! Writers never overwrite each other's work. Every write happens under the
//! writer lock ([`crate::lock`]) and starts by comparing the file with what
//! this backend last read or wrote ([`Seen`]): a plain [`save`] of a list
//! based on older content is refused, an [`update`] reads the file again
//! and applies its change to that.
//!
//! [`save`]: FileBackend::save
//! [`update`]: FileBackend::update

use super::{ChangeFn, StaleDatabase, Updated, warn_yellow};
use crate::atomic::{self, Replaced};
use crate::codec::DbFormat;
use crate::lock::{self, WriterLock};
use crate::model::Task;
use crate::revision::{Fingerprint, Seen};
use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// What is being read: decides how a file that does not parse is reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Reading {
    Database,
    /// The `.backup` copy, for `rusk restore`.
    Backup,
}

/// Why the database file is being written.
#[derive(Debug, Clone, Copy)]
pub(super) enum Change {
    /// A regular save.
    Update,
    /// `rusk restore` putting the `.backup` state back.
    Restore,
}

#[derive(Debug)]
pub struct FileBackend {
    path: PathBuf,
    format: DbFormat,
    #[cfg(feature = "backend-git")]
    git: bool,
    /// What the last load, or our own last write, left at `path`.
    seen: Mutex<Seen>,
}

impl FileBackend {
    pub fn new(path: PathBuf) -> Result<Self> {
        let format = DbFormat::from_path(&path)?;
        Ok(Self {
            path,
            format,
            #[cfg(feature = "backend-git")]
            git: crate::config::config().git_backend,
            seen: Mutex::new(Seen::Unchecked),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn format(&self) -> DbFormat {
        self.format
    }

    fn seen(&self) -> Seen {
        *self.seen.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn set_seen(&self, seen: Seen) {
        *self.seen.lock().unwrap_or_else(|e| e.into_inner()) = seen;
    }

    pub fn load(&self) -> Result<Vec<Task>> {
        Ok(self.read()?.tasks)
    }

    pub(super) fn read(&self) -> Result<super::Loaded> {
        let (tasks, seen) = self.read_current()?;
        self.set_seen(seen);
        Ok(super::Loaded {
            tasks,
            missing: seen == Seen::Missing,
        })
    }

    /// The tasks in the file right now, with the identity of the content
    /// they were decoded from.
    fn read_current(&self) -> Result<(Vec<Task>, Seen)> {
        let Some(data) = self.read_text()? else {
            return Ok((Vec::new(), Seen::Missing));
        };
        let tasks = self.records(&data, Reading::Database)?;
        Ok((tasks, Seen::Content(Fingerprint::of(data.as_bytes()))))
    }

    /// The bytes of the database file, or `None` when there is no database
    /// file at all.
    ///
    /// Only "nothing exists at that path" is `None`. A file that exists but
    /// cannot be read — no permission on it or on its directory, a
    /// directory in its place, an I/O error — is an error, and so is a
    /// symbolic link pointing nowhere: read as an empty database, any of
    /// them would have the next save replace data that is still there.
    fn read_file(&self) -> Result<Option<Vec<u8>>> {
        match fs::read(&self.path) {
            Ok(data) => Ok(Some(data)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                match fs::symlink_metadata(&self.path) {
                    // Nothing there at all: the database is yet to be made.
                    Err(_) => Ok(None),
                    // The link is there, its target is not (an unmounted
                    // disk, a moved file). Saving through it would create
                    // the target — somewhere the user no longer means.
                    Ok(_) => Err(super::dangling_link(&self.path)),
                }
            }
            Err(e) => Err(anyhow::Error::new(e).context(self.failed_to("read"))),
        }
    }

    /// The file as text, or `None` when there is no file. Every format is
    /// text; bytes that are no UTF-8 are as unreadable as a damaged file.
    fn read_text(&self) -> Result<Option<String>> {
        self.read_file()?
            .map(|bytes| {
                String::from_utf8(bytes).map_err(|e| {
                    anyhow::anyhow!("not valid UTF-8: {}", e.utf8_error())
                        .context(self.failed_to("read"))
                })
            })
            .transpose()
    }

    /// True when the file still holds what this backend last read or wrote.
    /// A backend that never read it has nothing to compare with, and what
    /// is not a regular file has no content to compare: a device or a FIFO
    /// is written into as it is (a directory in that place fails the read
    /// before anything gets here).
    fn is_current(&self) -> Result<bool> {
        let seen = self.seen();
        if seen == Seen::Unchecked {
            return Ok(true);
        }
        if fs::metadata(&self.path).is_ok_and(|meta| !meta.is_file() && !meta.is_dir()) {
            return Ok(true);
        }
        // Exactly what `load` sees, errors included: a database that cannot
        // be read must not be replaced on the assumption that it is gone.
        let now = match self.read_file()? {
            None => Seen::Missing,
            Some(data) => Seen::Content(Fingerprint::of(&data)),
        };
        Ok(now == seen)
    }

    /// Creates the database directory and takes the writer lock. Looking at
    /// the current content, deciding and replacing it all have to happen
    /// while the returned guard lives.
    pub(super) fn lock(&self) -> Result<WriterLock> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create the directory '{}' for the database file",
                    parent.display()
                )
            })?;
        }
        Ok(lock::acquire(&self.path))
    }

    /// Reads a `.backup` copy for `rusk restore`: the tasks plus the raw
    /// content they were decoded from. Stricter than [`load`]: a file that
    /// is not a task list at all decodes to zero tasks in the lenient
    /// formats, and must not replace a database.
    ///
    /// [`load`]: Self::load
    pub(super) fn load_backup(&self) -> Result<(Vec<Task>, String)> {
        let Some(data) = self.read_text()? else {
            bail!("No backup file found at '{}'", self.path.display());
        };
        let tasks = self.records(&data, Reading::Backup)?;
        Ok((tasks, data))
    }

    /// The tasks in `data`, made to follow the rules (see
    /// [`super::normalized`]). Content that decodes to no record at all
    /// without being an empty database of its format is something else than
    /// a task list — an empty file where the format writes `[]`, a page of
    /// prose, half a write that never finished: a warning for the database,
    /// a refusal for a backup. (Records that are no tasks — an item without
    /// text — still show the file is a task list.)
    fn records(&self, data: &str, reading: Reading) -> Result<Vec<Task>> {
        let records = self.decode(data, reading)?;
        if records.is_empty() && !self.format.is_empty_database(data) {
            match reading {
                Reading::Database => super::warn_once(&format!(
                    "Warning: no tasks were recognized in '{}', and it is not an empty {} \
                     database either; the next save will overwrite its content",
                    self.path.display(),
                    self.format.name()
                )),
                Reading::Backup => bail!(
                    "no tasks were recognized in the backup '{}', and it is not an empty {} \
                     database either; refusing to replace the database with it",
                    self.path.display(),
                    self.format.name()
                ),
            }
        }
        super::normalized(records, &self.path.display().to_string())
    }

    /// Context for an I/O failure; the cause follows it in the chain.
    fn failed_to(&self, action: &str) -> String {
        format!("Failed to {action} the database file '{}'", self.path.display())
    }

    fn decode(&self, data: &str, reading: Reading) -> Result<Vec<Task>> {
        self.format.decode(data).map_err(|e| {
            // JSON gets the detailed corruption report with line context;
            // it is the default format and the one hand-edited most. (An
            // NDJSON error is a serde_json error too, but its position is
            // counted within one line, not within the file. And a backup
            // that fails during `rusk restore` must not be answered with
            // "restore from backup".)
            if self.format == DbFormat::Json
                && reading == Reading::Database
                && let Some(json_err) = e.downcast_ref::<serde_json::Error>()
            {
                let context_line = json_error_line_context(data, json_err)
                    .map(|c| format!(" Context: {c}"))
                    .unwrap_or_default();
                // Well-formed JSON with a value rusk does not take (a date
                // that is no day, text where a number goes): every task is
                // still there, and deleting the file is the last thing to
                // suggest (review of R25).
                if json_err.classify() == serde_json::error::Category::Data {
                    return anyhow::anyhow!(
                        "Failed to parse the database file at '{}'. The file is well-formed \
                        JSON, but a value in it is not one rusk can read.\n\
                        JSON parsing error: {}{}\n\
                        \n\
                        To fix this issue, correct that value in the file (a date is \
                        written YYYY-MM-DD), or restore from backup if you have one.",
                        self.path.display(),
                        json_err,
                        context_line
                    );
                }
                anyhow::anyhow!(
                    "Failed to parse the database file at '{}'. The file appears to be corrupted.\n\
                    JSON parsing error: {}{}\n\
                    \n\
                    To fix this issue, you can:\n\
                    1. Delete the corrupted file: rm '{}'\n\
                    2. Or restore from backup if you have one\n\
                    3. The application will create a new empty database on next run",
                    self.path.display(),
                    json_err,
                    context_line,
                    self.path.display()
                )
            } else {
                // `{:#}`: a decoder names the place ("NDJSON line 2"), the
                // cause under it says what is wrong there.
                anyhow::anyhow!(
                    "Failed to parse the {} {} at '{}': {e:#}",
                    self.format.name(),
                    match reading {
                        Reading::Database => "database file",
                        Reading::Backup => "backup",
                    },
                    self.path.display()
                )
            }
        })
    }

    /// Replaces the database with `tasks`, unless it has changed since this
    /// backend read it: that is a [`StaleDatabase`] error and nothing is
    /// written.
    pub fn save(&self, tasks: &[Task]) -> Result<()> {
        self.write(tasks, Change::Update)
    }

    pub(super) fn write(&self, tasks: &[Task], change: Change) -> Result<()> {
        let data = self.format.encode(tasks)?;
        let _lock = self.lock()?;
        self.write_raw(data.as_bytes(), tasks.len(), change)
    }

    /// Applies `change` to the task list and stores the result as one step
    /// under the writer lock. `snapshot` is reused while the file still
    /// holds what it was loaded from; otherwise the file is read again and
    /// the change applies to that — unless the snapshot carries edits of
    /// its own (`snapshot_is_clean` is false), which a fresh read would
    /// drop. Nothing is written when `change` fails or changes nothing.
    pub(super) fn update<T>(
        &self,
        snapshot: &[Task],
        snapshot_is_clean: bool,
        change: ChangeFn<'_, T>,
    ) -> Result<Updated<T>> {
        let _lock = self.lock()?;
        let reread = if self.is_current()? {
            None
        } else if snapshot_is_clean {
            Some(self.read_current()?)
        } else {
            return Err(StaleDatabase::at(self.path.display()));
        };
        let base = reread.as_ref().map_or(snapshot, |(tasks, _)| tasks);
        let (tasks, value, changed) = super::apply(base, change)?;
        // What this backend has seen moves only together with the list the
        // caller gets back: after a failure both stay as they were.
        let stored = changed || reread.is_some() || snapshot_is_clean;
        if changed {
            let data = self.format.encode(&tasks)?;
            self.replace(data.as_bytes(), tasks.len(), Change::Update)?;
        } else if let Some((_, seen)) = reread {
            self.set_seen(seen);
        }
        Ok(Updated {
            tasks,
            value,
            stored,
        })
    }

    /// Replaces the database file with `data` as it is, unless the file has
    /// changed since this backend read it. `rusk restore` puts the backup
    /// back byte for byte this way: whatever the format does not model
    /// (prose around a Markdown list, foreign iCalendar components) comes
    /// back too. The caller holds the writer lock (see [`lock`]).
    ///
    /// [`lock`]: Self::lock
    pub(super) fn write_raw(&self, data: &[u8], task_count: usize, change: Change) -> Result<()> {
        if !self.is_current()? {
            return Err(StaleDatabase::at(self.path.display()));
        }
        self.replace(data, task_count, change)
    }

    /// The write itself, under the writer lock and after the staleness
    /// check. A regular save keeps the content it replaces as `.backup`
    /// first. `change` also names the git commit: a restore is committed
    /// under its own message, so it can be told apart — and reverted — in
    /// the history.
    fn replace(&self, data: &[u8], task_count: usize, change: Change) -> Result<()> {
        if matches!(change, Change::Update) {
            super::refresh_backup(&self.path);
        }

        match atomic::replace_user_file(&self.path, data) {
            Ok(Replaced::Atomically | Replaced::Streamed) => {}
            Ok(Replaced::InPlace(reason)) => warn_yellow(&format!(
                "Warning: '{}' could not be replaced atomically ({reason}); \
                 it was rewritten in place instead",
                self.path.display()
            )),
            Err(e) => bail!(
                "Failed to write the database file '{}': {e}",
                self.path.display()
            ),
        }
        self.set_seen(Seen::Content(Fingerprint::of(data)));

        #[cfg(feature = "backend-git")]
        if self.git {
            super::git::commit_db(&self.path, task_count, change);
        }
        #[cfg(not(feature = "backend-git"))]
        let _ = (task_count, change);

        Ok(())
    }
}

/// Byte offset (0-based) of the first character of each 1-based line in `s`.
fn line_starts(s: &str) -> Vec<usize> {
    let mut v = vec![0];
    for (i, c) in s.char_indices() {
        if c == '\n' {
            v.push(i + 1);
        }
    }
    v
}

fn line_byte_end(data: &str, starts: &[usize], one_based_line: usize) -> Option<usize> {
    if one_based_line < 1 || one_based_line > starts.len() {
        return None;
    }
    let s = if one_based_line < starts.len() {
        starts[one_based_line]
    } else {
        data.len()
    };
    Some(s)
}

fn error_byte_in_file(data: &str, line: usize, column: usize) -> Option<usize> {
    if line < 1 {
        return None;
    }
    let starts = line_starts(data);
    if line > starts.len() {
        return None;
    }
    let line0 = line - 1;
    let line_start = starts[line0];
    let after_line = if line0 + 1 < starts.len() {
        starts[line0 + 1]
    } else {
        data.len()
    };
    let line_len = after_line - line_start;
    let col0 = column.saturating_sub(1);
    if col0 > line_len {
        return None;
    }
    Some(line_start + col0)
}

fn json_error_line_context(data: &str, e: &serde_json::Error) -> Option<String> {
    let n = e.line();
    if n == 0 {
        return None;
    }
    let lines: Vec<_> = data.lines().collect();
    let i = n - 1;
    if i >= lines.len() {
        return None;
    }
    let starts = line_starts(data);
    let first = i.saturating_sub(1) + 1;
    let last = (i + 2).min(lines.len());
    let range_str = if first <= last {
        match (
            starts.get(first - 1).copied(),
            line_byte_end(data, &starts, last),
        ) {
            (Some(a), Some(b)) if a <= b => format!("context file bytes {a}..{b}"),
            _ => String::new(),
        }
    } else {
        String::new()
    };

    let err_str = error_byte_in_file(data, n, e.column())
        .map(|b| format!("(error at byte {b})"))
        .unwrap_or_default();

    let ctx = (i.saturating_sub(1)..(i + 2).min(lines.len()))
        .map(|j| format!("{}: {}", j + 1, lines[j].trim_end()))
        .collect::<Vec<_>>()
        .join(" | ");

    let parts = [err_str.as_str(), range_str.as_str()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if parts.is_empty() {
        Some(ctx)
    } else {
        Some(format!("{parts}: {ctx}"))
    }
}
