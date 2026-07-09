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
//! for the local (file and SQLite) backends.

pub mod file;
#[cfg(feature = "backend-git")]
pub mod git;
#[cfg(feature = "backend-http")]
pub mod http;
#[cfg(feature = "backend-sqlite")]
pub mod sqlite;
#[cfg(feature = "backend-ssh")]
pub mod ssh;

use crate::model::Task;
use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

/// Non-fatal warning on stderr (backup / atomic-write / git fallbacks), in
/// the theme warning color (yellow by default).
pub(crate) fn warn_yellow(msg: &str) {
    eprintln!("{}", crate::config::theme().warning.paint(msg));
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

/// `user@host:/path` (an `@` before the `:` keeps Windows drive paths like
/// `C:\tasks.json` out of the ssh branch; `rusk sync` stays more permissive).
fn looks_like_ssh(s: &str) -> bool {
    match (s.find('@'), s.find(':')) {
        (Some(at), Some(colon)) => at < colon && colon + 1 < s.len(),
        _ => false,
    }
}

/// Directory values (existing dir or trailing `/`) get `tasks.json` appended.
fn db_path_from_value(path: PathBuf) -> PathBuf {
    if path.is_dir() || path.to_string_lossy().ends_with('/') {
        path.join("tasks.json")
    } else {
        path
    }
}

fn is_sqlite_path(path: &Path) -> bool {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    ["db", "sqlite", "sqlite3"]
        .iter()
        .any(|ext| name.ends_with(&format!(".{ext}")) || name.contains(&format!(".{ext}.")))
}

impl Backend {
    /// Resolves the configured database location. Test and debug runs are
    /// pinned to a temp file, mirroring the config isolation in
    /// `config::load`.
    pub fn resolve() -> Result<Self> {
        if crate::is_test_mode() || cfg!(debug_assertions) {
            let path = std::env::temp_dir().join("rusk_debug").join("tasks.json");
            Self::from_local_path(path)
        } else if let Ok(location) = std::env::var("RUSK_DB") {
            Self::parse(&location)
        } else if let Some(location) = &crate::config::config().rusk_db {
            // `rusk_db` from the config file; the RUSK_DB env var wins above.
            Self::parse(location)
        } else {
            Self::from_local_path(PathBuf::from(".rusk").join("tasks.json"))
        }
    }

    /// Picks the backend from the shape of a `rusk_db` value.
    pub fn parse(location: &str) -> Result<Self> {
        if location.starts_with("http://") || location.starts_with("https://") {
            #[cfg(feature = "backend-http")]
            {
                let token = crate::config::env_or_config(
                    "RUSK_DB_TOKEN",
                    &crate::config::config().db_token,
                );
                return Ok(Backend::Http(http::HttpBackend::new(location, token)));
            }
            #[cfg(not(feature = "backend-http"))]
            bail!(
                "'{location}' is an http(s) database location; this rusk build \
                 does not include it — rebuild with `--features backend-http`"
            );
        }
        if looks_like_ssh(location) {
            #[cfg(feature = "backend-ssh")]
            {
                let (target, path) = location.split_once(':').expect("checked by looks_like_ssh");
                return Ok(Backend::Ssh(ssh::SshBackend::new(target, path)?));
            }
            #[cfg(not(feature = "backend-ssh"))]
            bail!(
                "'{location}' is an ssh database location; this rusk build \
                 does not include it — rebuild with `--features backend-ssh`"
            );
        }
        Self::from_local_path(db_path_from_value(PathBuf::from(location)))
    }

    /// A local database: SQLite when the extension says so, a plain file
    /// otherwise.
    pub fn from_local_path(path: PathBuf) -> Result<Self> {
        if is_sqlite_path(&path) {
            #[cfg(feature = "backend-sqlite")]
            return Ok(Backend::Sqlite(sqlite::SqliteBackend::new(path)));
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

    pub fn save(&self, tasks: &[Task]) -> Result<()> {
        // One `.backup` sibling per save for the local backends, kept here
        // so every local representation gets it uniformly.
        if crate::config::config().backup
            && let Some(path) = self.local_path()
            && path.exists()
        {
            let backup_path = aux_path(path, "backup");
            if let Err(e) = std::fs::copy(path, &backup_path) {
                warn_yellow(&format!("Warning: Failed to create backup: {e}"));
            }
        }
        match self {
            Backend::File(b) => b.save(tasks),
            #[cfg(feature = "backend-sqlite")]
            Backend::Sqlite(b) => b.save(tasks),
            #[cfg(feature = "backend-http")]
            Backend::Http(b) => b.save(tasks),
            #[cfg(feature = "backend-ssh")]
            Backend::Ssh(b) => b.save(tasks),
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

    /// The same backend kind reading another file (backup copies).
    fn sibling(&self, path: PathBuf) -> Result<Self> {
        match self {
            Backend::File(_) => Ok(Backend::File(file::FileBackend::new(path)?)),
            #[cfg(feature = "backend-sqlite")]
            Backend::Sqlite(_) => Ok(Backend::Sqlite(sqlite::SqliteBackend::new(path))),
            #[cfg(any(feature = "backend-http", feature = "backend-ssh"))]
            _ => bail!("remote databases have no local siblings"),
        }
    }

    /// Replaces the database with its `.backup` sibling and returns the
    /// restored tasks. Local backends only.
    pub fn restore_from_backup(&self) -> Result<Vec<Task>> {
        let Some(db_path) = self.local_path() else {
            bail!(
                "`rusk restore` needs a local database; the database is at {} — \
                 restore from a backup on the remote side instead",
                self.describe()
            );
        };
        let backup_path = aux_path(db_path, "backup");
        if !backup_path.exists() {
            bail!("No backup file found at '{}'", backup_path.display());
        }

        let backup_tasks = self.sibling(backup_path.clone())?.load()?;

        if db_path.exists() {
            let current_backup_path = aux_path(db_path, "before_restore");
            match self.load() {
                Ok(_) => {
                    if let Err(e) = std::fs::copy(db_path, &current_backup_path) {
                        warn_yellow(&format!("Warning: Failed to backup current database: {e}"));
                    } else {
                        println!(
                            "Current database backed up to: {}",
                            current_backup_path.display()
                        );
                    }
                }
                Err(_) => {
                    println!("Current database is corrupted, skipping backup");
                }
            }
        }

        std::fs::copy(&backup_path, db_path).context("Failed to restore from backup")?;

        println!(
            "Successfully restored {} tasks from backup",
            backup_tasks.len()
        );
        println!("Backup file: {}", backup_path.display());

        Ok(backup_tasks)
    }
}

/// Auxiliary sibling of the database file: `tasks.json` + `backup` →
/// `tasks.json.backup` (and `tasks.csv` → `tasks.csv.backup`), so the
/// base format stays recognizable in the name.
pub(crate) fn aux_path(path: &Path, suffix: &str) -> PathBuf {
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("json");
    path.with_extension(format!("{ext}.{suffix}"))
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn windows_drive_paths_are_not_ssh() {
        assert!(!looks_like_ssh(r"C:\tasks\tasks.json"));
        assert!(!looks_like_ssh("plain-host:/path"));
        assert!(looks_like_ssh("user@host:/path/tasks.json"));
        assert!(!looks_like_ssh("user@host:"));
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
            aux_path(Path::new("/a/tasks.json"), "tmp"),
            PathBuf::from("/a/tasks.json.tmp")
        );
    }
}
