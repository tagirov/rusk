//! Advisory writer lock of a local file database.
//!
//! A save is "look at what is there, decide, replace the file". Two rusk
//! processes doing that at the same moment would both find the old content
//! in place, and the second rename would drop the first writer's change
//! without a trace. So the whole step runs under an exclusive lock on a
//! `<database>.lock` sibling.
//!
//! The lock file is empty and stays where it is: removing it would race
//! with the processes waiting on it. The lock itself belongs to the open
//! file and is released by the OS when the holder exits, however it exits,
//! so there is never a stale lock to clean up.
//!
//! Best effort by design. Where the lock file cannot be created or the
//! filesystem cannot lock, the save goes ahead without it: the content
//! check in the backend still catches every conflict except two writers
//! inside the same few milliseconds. Readers never take the lock — the
//! atomic replace already shows them the old content or the new one.

use crate::backend::warn_yellow;
use std::fs::{self, File, OpenOptions, TryLockError};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Far longer than a save takes. A holder that needs more is stuck (a git
/// hook waiting for a passphrase), and blocking every other rusk command
/// behind it forever would be worse than saving next to it.
const PATIENCE: Duration = Duration::from_secs(10);
const LONGEST_NAP: Duration = Duration::from_millis(20);

/// Keeps the lock until dropped. It may hold nothing, see the module docs.
#[derive(Debug)]
pub(crate) struct WriterLock {
    _file: Option<File>,
}

impl WriterLock {
    #[cfg(test)]
    fn is_held(&self) -> bool {
        self._file.is_some()
    }
}

/// Waits for the writer lock of the database at `db_path`. The directory
/// of the database must exist.
pub(crate) fn acquire(db_path: &Path) -> WriterLock {
    acquire_within(db_path, PATIENCE)
}

fn acquire_within(db_path: &Path, patience: Duration) -> WriterLock {
    let path = lock_path(db_path);
    let file = match open(&path, db_path) {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::InvalidInput => {
            warn_yellow(&format!(
                "Warning: '{}' is a symbolic link and will not be used as a lock; \
                 saving without the writer lock",
                path.display()
            ));
            return WriterLock { _file: None };
        }
        // A directory that takes no new file: the write itself reports it.
        Err(_) => return WriterLock { _file: None },
    };

    let started = Instant::now();
    let mut nap = Duration::from_millis(1);
    loop {
        match file.try_lock() {
            Ok(()) => return WriterLock { _file: Some(file) },
            Err(TryLockError::WouldBlock) if started.elapsed() < patience => {
                std::thread::sleep(nap);
                nap = (nap * 2).min(LONGEST_NAP);
            }
            Err(TryLockError::WouldBlock) => {
                warn_yellow(&format!(
                    "Warning: another process has been holding '{}' for more than {} s; \
                     saving without waiting any longer",
                    path.display(),
                    patience.as_secs()
                ));
                return WriterLock { _file: None };
            }
            // No lock support here (some network and FUSE filesystems).
            Err(TryLockError::Error(_)) => return WriterLock { _file: None },
        }
    }
}

/// The database is replaced where it really lives (a symlinked path is
/// followed, see `atomic::replace_user_file`), so that is where the writers
/// have to meet.
fn lock_path(db_path: &Path) -> PathBuf {
    let real = fs::canonicalize(db_path).unwrap_or_else(|_| db_path.to_path_buf());
    crate::backend::aux_path(&real, "lock")
}

fn open(path: &Path, db_path: &Path) -> io::Result<File> {
    // A name rusk derives itself: whatever sits there may have been
    // planted, and a link must not get some other file locked.
    if fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "the lock path is a symbolic link",
        ));
    }
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    // A new lock file is as private as the database: whoever can open it
    // can hold the lock and make every save wait out its patience.
    #[cfg(unix)]
    if let Ok(db) = fs::metadata(db_path) {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        options.mode(db.permissions().mode() & 0o666);
    }
    #[cfg(not(unix))]
    let _ = db_path;
    let created = options.open(path);
    match created {
        // Another user's lock file next to a shared database: being able
        // to read it is enough to lock it.
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => File::open(path),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_writer_waits_until_the_first_is_done() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("tasks.json");

        let first = acquire(&db);
        assert!(first.is_held());
        assert!(dir.path().join("tasks.json.lock").exists());

        // Held elsewhere: the wait runs out and the caller goes on unlocked.
        let impatient = acquire_within(&db, Duration::from_millis(30));
        assert!(!impatient.is_held());

        // Released on drop: the next writer gets it. Not necessarily at
        // once: a process another test thread is starting holds a copy of
        // every descriptor of this one from its fork to its exec, the lock
        // with it (seen once under a full `cargo test`).
        drop(first);
        assert!(acquire_within(&db, Duration::from_secs(5)).is_held());
    }

    #[test]
    fn a_waiting_writer_gets_the_lock_as_soon_as_it_is_free() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("tasks.json");
        let first = acquire(&db);

        let waiter = {
            let db = db.clone();
            std::thread::spawn(move || acquire(&db).is_held())
        };
        std::thread::sleep(Duration::from_millis(50));
        drop(first);
        assert!(waiter.join().unwrap());
    }

    #[test]
    fn a_missing_directory_means_no_lock_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("no-such-dir").join("tasks.json");
        assert!(!acquire(&db).is_held());
    }

    #[cfg(unix)]
    #[test]
    fn writers_of_a_symlinked_database_meet_at_the_real_file() {
        let dir = tempfile::tempdir().unwrap();
        let real_dir = dir.path().join("dotfiles");
        fs::create_dir(&real_dir).unwrap();
        fs::write(real_dir.join("tasks.json"), "[]").unwrap();
        let link = dir.path().join("tasks.json");
        std::os::unix::fs::symlink(real_dir.join("tasks.json"), &link).unwrap();

        let through_link = acquire(&link);
        assert!(through_link.is_held());
        assert!(real_dir.join("tasks.json.lock").exists());
        assert!(!dir.path().join("tasks.json.lock").exists());
        let direct = acquire_within(&real_dir.join("tasks.json"), Duration::ZERO);
        assert!(!direct.is_held());
    }

    #[cfg(unix)]
    #[test]
    fn the_lock_file_of_a_private_database_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("tasks.json");
        fs::write(&db, "[]").unwrap();
        fs::set_permissions(&db, fs::Permissions::from_mode(0o600)).unwrap();

        assert!(acquire(&db).is_held());
        let lock = fs::metadata(dir.path().join("tasks.json.lock")).unwrap();
        assert_eq!(lock.permissions().mode() & 0o777, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn a_planted_symlink_is_not_locked_through() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("tasks.json");
        let victim = dir.path().join("victim");
        fs::write(&victim, "keep").unwrap();
        std::os::unix::fs::symlink(&victim, dir.path().join("tasks.json.lock")).unwrap();

        assert!(!acquire(&db).is_held());
        assert_eq!(fs::read_to_string(&victim).unwrap(), "keep");
    }
}
