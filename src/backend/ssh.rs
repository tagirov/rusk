//! SSH backend (`rusk_db = user@host:/path/tasks.json`), feature
//! `backend-ssh`: the database is a file on a remote machine, read and
//! written over the system `ssh` (keys, agent and `~/.ssh/config` apply).
//! The remote extension picks the format exactly like a local path, so a
//! remote `tasks.md` stays a readable Markdown task list. Writes stream to
//! a temp sibling and `mv` into place (atomic replace).
//!
//! There is no lock and no compare-and-swap on the remote side, so the
//! protection against overwriting somebody else's change is a second look:
//! when the content was read a while ago (an editor or a prompt sat in
//! between), it is read again right before the write and compared. For a
//! command that loads and saves back to back a second look would cost a
//! round trip without narrowing the window, so there is none.

use super::{ChangeFn, Loaded, StaleDatabase, Updated};
use crate::codec::DbFormat;
use crate::location::SshLocation;
use crate::model::Task;
use crate::revision::{Fingerprint, Seen};
use crate::transport;
use anyhow::{Context, Result, bail};
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long what was read is taken to still be there.
const TRUSTED_FOR: Duration = Duration::from_secs(2);

#[derive(Debug)]
pub struct SshBackend {
    /// `[user@]host` as written, for messages.
    host: String,
    /// What `ssh` connects to.
    destination: String,
    path: String,
    format: DbFormat,
    /// What the last load, or our own last write, left on the remote, and
    /// when. `None` until then: the caller replaces unconditionally.
    seen: Mutex<Option<(Seen, Instant)>>,
}

/// One look at the remote file.
struct Fetched {
    tasks: Vec<Task>,
    /// The content it was read from — or that there was no file.
    seen: Seen,
}

impl SshBackend {
    pub fn new(location: &SshLocation) -> Result<Self> {
        // A remote path cannot be asked whether it is a directory without a
        // round trip, but the shapes that say so by themselves are honoured
        // exactly as they are locally: `user@host:/srv/rusk/` is that
        // directory's `tasks.json`, not a file with an empty name, and
        // `user@host:` is the one in the remote home directory.
        let path = &location.path;
        let path = if path.is_empty() || path.ends_with('/') {
            format!("{path}tasks.json")
        } else {
            path.clone()
        };
        if super::is_sqlite_path(Path::new(&path)) {
            bail!(
                "'{}:{path}' is a SQLite database; SQLite works on a local file \
                 only — over ssh use a file format (tasks.json, tasks.md, …)",
                location.host
            );
        }
        let format = DbFormat::from_path(Path::new(&path))?;
        Ok(Self {
            host: location.host.clone(),
            destination: location.destination.clone(),
            path,
            format,
            seen: Mutex::new(None),
        })
    }

    /// `host:path` as it reads back: a path from the remote home that
    /// starts with a `~` of its own keeps the `~/` in front, or it would
    /// read as another user's home (review of R21).
    pub fn describe(&self) -> String {
        if self.path.starts_with('~') {
            format!("{}:~/{}", self.host, self.path)
        } else {
            format!("{}:{}", self.host, self.path)
        }
    }

    pub fn format(&self) -> DbFormat {
        self.format
    }

    /// The path on the other side, without the `user@host:` part.
    pub(super) fn remote_path(&self) -> &str {
        &self.path
    }

    fn set_seen(&self, seen: Seen) {
        *self.seen.lock().unwrap_or_else(|e| e.into_inner()) = Some((seen, Instant::now()));
    }

    /// Reads the remote file. A file that is not there is no database yet
    /// and reads as no tasks; one that cannot be read is an error, so that
    /// the next save does not replace what is still on the remote (see the
    /// transport docs).
    fn fetch(&self) -> Result<Fetched> {
        let raw = transport::ssh_read_file(&self.destination, &self.path)
            .with_context(|| format!("failed to load tasks from {}", self.describe()))?;
        let Some(raw) = raw else {
            return Ok(Fetched {
                tasks: Vec::new(),
                seen: Seen::Missing,
            });
        };
        let seen = Seen::Content(Fingerprint::of(&raw));
        // Not lossy: a byte that is no UTF-8 would come back as U+FFFD and
        // the next save would write that over the remote file. Unreadable
        // content is unreadable, here as locally.
        let text = String::from_utf8(raw).map_err(|e| {
            anyhow::anyhow!("not valid UTF-8: {}", e.utf8_error()).context(format!(
                "failed to load tasks from {}",
                self.describe()
            ))
        })?;
        let records = self.format.decode(&text).with_context(|| {
            format!(
                "remote file {} is not a valid {} task list",
                self.path,
                self.format.name()
            )
        })?;
        // The same rule as for a local file: content that is no task list
        // at all must not pass as an empty database (see `FileBackend`).
        if records.is_empty() && !self.format.is_empty_database(&text) {
            super::warn_once(&format!(
                "Warning: no tasks were recognized in {}, and it is not an empty {} \
                 database either; the next save will overwrite its content",
                self.describe(),
                self.format.name()
            ));
        }
        Ok(Fetched {
            tasks: super::normalized(records, &self.describe())?,
            seen,
        })
    }

    pub fn load(&self) -> Result<Vec<Task>> {
        Ok(self.read()?.tasks)
    }

    pub fn read(&self) -> Result<Loaded> {
        let fetched = self.fetch()?;
        self.set_seen(fetched.seen);
        Ok(Loaded {
            tasks: fetched.tasks,
            missing: fetched.seen == Seen::Missing,
        })
    }

    /// The remote content when it is no longer what this backend saw;
    /// `None` when it is, or when it was seen too recently to look again.
    fn changed_meanwhile(&self) -> Result<Option<Fetched>> {
        let seen = *self.seen.lock().unwrap_or_else(|e| e.into_inner());
        let Some((seen, at)) = seen else {
            return Ok(None);
        };
        if at.elapsed() < TRUSTED_FOR {
            return Ok(None);
        }
        let now = self.fetch()?;
        Ok((now.seen != seen).then_some(now))
    }

    fn write(&self, tasks: &[Task]) -> Result<()> {
        let data = self.format.encode(tasks)?;
        let backup = crate::config::config().backup;
        transport::ssh_write_file(&self.destination, &self.path, data.as_bytes(), backup)
            .with_context(|| format!("failed to save tasks to {}", self.describe()))?;
        self.set_seen(Seen::Content(Fingerprint::of(data.as_bytes())));
        Ok(())
    }

    /// Replaces the remote file, unless a second look (see the module
    /// docs) shows that it has changed since this backend read it: that is
    /// a [`StaleDatabase`] error.
    pub fn save(&self, tasks: &[Task]) -> Result<()> {
        if self.changed_meanwhile()?.is_some() {
            return Err(StaleDatabase::at(self.describe()));
        }
        self.write(tasks)
    }

    /// Applies `change` to `snapshot` — or to the remote content, when a
    /// second look shows it has changed since — and writes the result. A
    /// snapshot with edits of its own (`snapshot_is_clean` is false) cannot
    /// be swapped for the fresh content, so that is a stale database.
    pub(super) fn update<T>(
        &self,
        snapshot: &[Task],
        snapshot_is_clean: bool,
        change: ChangeFn<'_, T>,
    ) -> Result<Updated<T>> {
        let reread = match self.changed_meanwhile()? {
            Some(_) if !snapshot_is_clean => return Err(StaleDatabase::at(self.describe())),
            reread => reread,
        };
        let base = reread.as_ref().map_or(snapshot, |fetched| &fetched.tasks[..]);
        let (tasks, value, changed) = super::apply(base, change)?;
        let stored = changed || reread.is_some() || snapshot_is_clean;
        if changed {
            self.write(&tasks)?;
        } else if let Some(fetched) = reread {
            self.set_seen(fetched.seen);
        }
        Ok(Updated {
            tasks,
            value,
            stored,
        })
    }
}
