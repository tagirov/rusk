//! Crash-safe file replacement: the one write path for every file rusk
//! keeps on disk — the database, its `.backup` and `.before_restore`
//! copies, `rusk gen -o`, the sync state and the editor draft.
//!
//! The new content goes to a temp file next to the destination, created
//! with `create_new` (O_EXCL) under a unique name. It gets the permissions
//! of the file it replaces, is fsynced and renamed over the destination;
//! the directory is fsynced last. So:
//!
//! - a reader, a crash or a full disk leaves the old content or the new
//!   one, never a truncated mix;
//! - concurrent writers never share (or delete) each other's temp file;
//! - a symlink planted under a predictable name is never written through:
//!   O_EXCL refuses it, and the final rename replaces a link instead of
//!   following it;
//! - a private (0600) file stays private.
//!
//! A process killed between the create and the rename leaves a
//! `<name>.<16 hex digits>.tmp` file behind. It is never read; the next
//! successful write of the same file removes such leftovers once they are
//! an hour old.

use std::ffi::{OsStr, OsString};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// Where the permissions of a file rusk names itself come from. Unix only;
/// other platforms keep their default.
#[derive(Debug, Clone, Copy)]
pub enum Mode {
    /// The mode of the file being replaced; a file that did not exist gets
    /// the process default (0666 minus umask).
    Keep,
    /// Owner-only (0600): editor drafts.
    Private,
}

/// How the destination was replaced.
#[derive(Debug)]
pub enum Replaced {
    /// Temp file + rename: crash-safe.
    Atomically,
    /// The destination cannot be replaced by a rename (the payload says
    /// why) — a bind-mounted file, a filesystem without rename-over — so it
    /// was rewritten in place: complete, but not crash-safe.
    InPlace(io::Error),
    /// The destination is not a regular file (`/dev/stdout`, a FIFO, a
    /// process substitution): the data was written straight into it.
    /// Renaming a temp file over such a path would replace the device.
    Streamed,
}

/// Replaces a file the user keeps their data in: the database.
///
/// A symlink of the user's own is followed, so the link survives and the
/// file it points to is replaced; a dangling link, or one that belongs to
/// another user, is an error. The mode of the replaced file is kept. Where
/// a rename cannot replace the destination (a bind-mounted file) it is
/// rewritten in place, see [`Replaced::InPlace`]; a failure that a rewrite
/// would hit as well (disk full, no temp file possible) is returned with
/// the old content untouched.
pub fn replace_user_file(path: &Path, data: &[u8]) -> io::Result<Replaced> {
    replace(path, Perms::Keep, Trust::UserData, &REAL, &mut |out| {
        io::Write::write_all(out, data)
    })
}

/// Replaces a file rusk generates at a path the user chose: `rusk gen -o`.
///
/// Like [`replace_user_file`], but the content can be generated again, so
/// getting it there matters more than how: a file in a directory that does
/// not allow a temp file next to it (a page pre-created for the user in a
/// root-owned web root) is rewritten in place.
pub fn replace_output_file(path: &Path, data: &[u8]) -> io::Result<Replaced> {
    replace(path, Perms::Keep, Trust::UserOutput, &REAL, &mut |out| {
        io::Write::write_all(out, data)
    })
}

/// Replaces a file rusk names itself next to the database: `.sync`, the
/// editor draft.
///
/// Those names are predictable, so nothing already sitting there is
/// trusted: a symlink is replaced, never followed, and there is no in-place
/// fallback that could write through one.
pub fn replace_aux_file(path: &Path, data: &[u8], mode: Mode) -> io::Result<()> {
    let perms = match mode {
        Mode::Keep => Perms::Keep,
        Mode::Private => Perms::Private,
    };
    replace(path, perms, Trust::OwnName, &REAL, &mut |out| {
        io::Write::write_all(out, data)
    })
    .map(drop)
}

/// [`replace_aux_file`] with the content and the mode of `from`: a copy of
/// the database (`.backup`) is never more readable than the database.
pub fn copy_to_aux_file(from: &Path, to: &Path) -> io::Result<()> {
    let mut source = File::open(from)?;
    // One open serves both the mode and the content: the path may be
    // swapped between two lookups.
    let template = source.metadata()?;
    replace(to, Perms::Template(template), Trust::OwnName, &REAL, &mut |out| {
        io::copy(&mut source, out).map(drop)
    })
    .map(drop)
}

/// [`copy_to_aux_file`] for a copy made from the content rather than the
/// bytes of `like` (the image SQLite makes of a database): `data` with the
/// mode of `like`. The mode is looked up without opening `like` — closing a
/// descriptor of a SQLite file would drop the locks SQLite holds on it.
pub fn replace_aux_file_like(to: &Path, data: &[u8], like: &Path) -> io::Result<()> {
    let template = fs::metadata(like)?;
    replace(to, Perms::Template(template), Trust::OwnName, &REAL, &mut |out| {
        io::Write::write_all(out, data)
    })
    .map(drop)
}

/// Copies `from` to `to` only when nothing exists there yet: `create_new`
/// fails with `AlreadyExists` on a file and on a planted symlink alike, so
/// an earlier copy is never overwritten. The mode follows `from`; the copy
/// is fsynced before this returns, and removed again if it fails half-way.
pub fn copy_to_new_file(from: &Path, to: &Path) -> io::Result<()> {
    let mut source = File::open(from)?;
    let template = source.metadata()?;
    write_new_file(to, &template, &mut |out| io::copy(&mut source, out).map(drop))
}

/// [`copy_to_new_file`] for a copy made from the content of `like` (see
/// [`replace_aux_file_like`]).
pub fn write_new_file_like(to: &Path, data: &[u8], like: &Path) -> io::Result<()> {
    let template = fs::metadata(like)?;
    write_new_file(to, &template, &mut |out| io::Write::write_all(out, data))
}

fn write_new_file(to: &Path, template: &fs::Metadata, fill: Fill<'_>) -> io::Result<()> {
    let mut out = create_exclusive(to, PRIVATE)?;
    let written = fill(&mut out).and_then(|_| {
        apply_template(&out, template, parent_dir(to));
        out.sync_all()
    });
    drop(out);
    match written {
        Ok(()) => {
            sync_dir(parent_dir(to));
            Ok(())
        }
        Err(e) => {
            let _ = fs::remove_file(to);
            Err(e)
        }
    }
}

/// Whether the destination name can be trusted to be what the user meant,
/// and how much a degraded write is worth.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Trust {
    /// The user named this path and keeps data there: follow their
    /// symlink, tolerate a destination that cannot be renamed over, but
    /// never start a write that cannot be completed safely.
    UserData,
    /// The user named this path for generated output: as above, and a
    /// plain rewrite is fine where no temp file can be created.
    UserOutput,
    /// rusk derived this name: whatever sits there may have been planted.
    OwnName,
}

impl Trust {
    fn user_named(self) -> bool {
        self != Trust::OwnName
    }
}

/// Where the mode of the new file comes from (see [`Mode`]).
enum Perms {
    Keep,
    Private,
    /// An already fetched `fstat` of the file being copied.
    Template(fs::Metadata),
}

const PRIVATE: u32 = 0o600;
const UMASK_DEFAULT: u32 = 0o666;
const TEMP_NAME_ATTEMPTS: usize = 32;
/// No write takes anywhere near this long: a temp file this old belongs to
/// a process that died.
const STALE_TEMP_AGE: Duration = Duration::from_secs(60 * 60);

/// The steps tests need to control: the rename (to exercise the in-place
/// fallback) and the temp name (to plant something exactly there).
struct Hooks {
    rename: fn(&Path, &Path) -> io::Result<()>,
    unique: fn() -> u64,
}

const REAL: Hooks = Hooks {
    rename: rename_over,
    unique,
};

type Fill<'a> = &'a mut dyn FnMut(&mut File) -> io::Result<()>;

fn rename_over(from: &Path, to: &Path) -> io::Result<()> {
    // Windows refuses to replace a file that another process holds open
    // without delete sharing (an antivirus scan, an indexer, a reader).
    // That is momentary, so retry briefly before giving up on the rename.
    let attempts = if cfg!(windows) { 10 } else { 1 };
    let mut result = fs::rename(from, to);
    for _ in 1..attempts {
        match &result {
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => {
                std::thread::sleep(Duration::from_millis(20));
                result = fs::rename(from, to);
            }
            _ => break,
        }
    }
    result
}

fn replace(
    path: &Path,
    perms: Perms,
    trust: Trust,
    hooks: &Hooks,
    fill: Fill<'_>,
) -> io::Result<Replaced> {
    if trust.user_named() && is_special_file(path) {
        let mut out = OpenOptions::new().write(true).open(path)?;
        fill(&mut out)?;
        return Ok(Replaced::Streamed);
    }
    let (dest, followed_link) = if trust.user_named() {
        resolve_symlink(path)?
    } else {
        (path.to_path_buf(), None)
    };
    let name = dest.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("'{}' has no file name", dest.display()),
        )
    })?;
    let dir = parent_dir(&dest);

    let replaced = replaced_file_metadata(&dest, trust);
    // Never readable more widely than the final mode, not even while the
    // content is being written: only a brand-new `Keep` file is created
    // with the umask default it will end up with anyway.
    let (template, create_mode) = match perms {
        Perms::Keep if replaced.is_none() => (None, UMASK_DEFAULT),
        Perms::Keep => (replaced.clone(), PRIVATE),
        Perms::Template(template) => (Some(template), PRIVATE),
        Perms::Private => (None, PRIVATE),
    };

    // `file` is declared last, so it is closed before `temp` removes it.
    let (mut temp, mut file) = match create_temp(dir, name, create_mode, hooks.unique) {
        Ok(created) => created,
        Err(e)
            if trust == Trust::UserOutput
                && e.kind() == io::ErrorKind::PermissionDenied
                && replaced.is_some() =>
        {
            return rewrite_without_temp(&dest, e, fill);
        }
        Err(e) => {
            return Err(io::Error::new(
                e.kind(),
                format!("cannot create a temporary file in '{}' ({e})", dir.display()),
            ));
        }
    };
    if let Some(link) = &followed_link {
        refuse_foreign_symlink(link, &file)?;
    }
    fill(&mut file)?;
    if let Some(template) = &template {
        apply_template(&file, template, dir);
    }
    file.sync_all()?;
    drop(file);

    match (hooks.rename)(temp.path(), &dest) {
        Ok(()) => {
            temp.disarm();
            sync_dir(dir);
            remove_stale_temps(dir, name, &dest);
            Ok(Replaced::Atomically)
        }
        Err(rename_error) if trust.user_named() && cannot_be_renamed_over(&rename_error) => {
            rewrite_in_place(temp, &dest, rename_error)
        }
        Err(rename_error) => Err(rename_error),
    }
}

/// The rename failures that say "this destination cannot be replaced by a
/// rename" (a mount point, a filesystem that refuses to replace), as
/// opposed to "this write cannot succeed" (disk full, quota, I/O error):
/// for the latter an in-place rewrite would only truncate the old content
/// and then fail as well.
fn cannot_be_renamed_over(error: &io::Error) -> bool {
    use io::ErrorKind::*;
    matches!(
        error.kind(),
        ResourceBusy | CrossesDevices | PermissionDenied | Unsupported | AlreadyExists
    )
}

/// The fallback for destinations that cannot be renamed over. The complete
/// new content already sits in the temp file, so a failure half-way leaves
/// that file behind as the only full copy instead of deleting it.
fn rewrite_in_place(mut temp: TempFile, dest: &Path, rename_error: io::Error) -> io::Result<Replaced> {
    let opened = File::open(temp.path()).and_then(|source| {
        let out = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(dest)?;
        Ok((source, out))
    });
    let (mut source, mut out) = match opened {
        Ok(files) => files,
        // Nothing was touched yet: report the rename, drop the temp file.
        Err(open_error) => {
            return Err(io::Error::new(
                rename_error.kind(),
                format!("{rename_error}; rewriting in place failed too: {open_error}"),
            ));
        }
    };
    match io::copy(&mut source, &mut out).and_then(|_| out.sync_all()) {
        Ok(()) => Ok(Replaced::InPlace(rename_error)),
        Err(copy_error) => {
            let kept = temp.keep();
            Err(io::Error::new(
                copy_error.kind(),
                format!(
                    "{rename_error}; rewriting in place failed half-way ({copy_error}) — \
                     the complete new content was kept in '{}'",
                    kept.display()
                ),
            ))
        }
    }
}

/// Generated output in a directory that allows no temp file next to it:
/// the existing file is rewritten directly, as a plain `fs::write` would.
fn rewrite_without_temp(dest: &Path, reason: io::Error, fill: Fill<'_>) -> io::Result<Replaced> {
    let mut out = OpenOptions::new().write(true).truncate(true).open(dest)?;
    fill(&mut out)?;
    out.sync_all()?;
    Ok(Replaced::InPlace(reason))
}

/// Something that exists but is neither a regular file nor a directory: a
/// character device, a FIFO, a socket. Checked through symlinks
/// (`/dev/stdout` is one).
fn is_special_file(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|meta| !meta.is_file() && !meta.is_dir())
}

/// A symlinked destination is replaced where it really lives, so the link
/// stays a link (dotfiles, synced folders). Only an actual symlink is
/// resolved; every other path is used exactly as given. Returns the link's
/// own metadata along with the target when one was followed.
fn resolve_symlink(path: &Path) -> io::Result<(PathBuf, Option<fs::Metadata>)> {
    match fs::symlink_metadata(path) {
        // Callers name the path in their own message.
        Ok(link) if link.file_type().is_symlink() => match fs::canonicalize(path) {
            Ok(target) => Ok((target, Some(link))),
            Err(e) => Err(io::Error::new(
                e.kind(),
                format!("it is a symbolic link whose target cannot be resolved ({e})"),
            )),
        },
        _ => Ok((path.to_path_buf(), None)),
    }
}

/// A link is followed only when it is the user's own (or root's): in a
/// directory other people can write to, anyone may have put a link to
/// `~/.ssh/authorized_keys` where the database used to be. The owner of
/// the temp file just created tells who "the user" is.
#[cfg(unix)]
fn refuse_foreign_symlink(link: &fs::Metadata, created_by_us: &File) -> io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    if may_follow(link.uid(), created_by_us.metadata()?.uid()) {
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::PermissionDenied,
        "it is a symbolic link that belongs to another user; refusing to write through it",
    ))
}

#[cfg(not(unix))]
fn refuse_foreign_symlink(_link: &fs::Metadata, _created_by_us: &File) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn may_follow(link_owner: u32, user: u32) -> bool {
    link_owner == user || link_owner == 0
}

/// Metadata of the regular file about to be replaced, if there is one. For
/// a name rusk derived itself the entry is inspected without following it:
/// a planted symlink must not lend its target's mode.
fn replaced_file_metadata(dest: &Path, trust: Trust) -> Option<fs::Metadata> {
    let meta = if trust.user_named() {
        fs::metadata(dest)
    } else {
        fs::symlink_metadata(dest)
    };
    meta.ok().filter(|m| m.file_type().is_file())
}

fn parent_dir(path: &Path) -> &Path {
    match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    }
}

fn create_exclusive(path: &Path, mode: u32) -> io::Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(mode);
    }
    #[cfg(not(unix))]
    let _ = mode;
    options.open(path)
}

fn create_temp(
    dir: &Path,
    name: &OsStr,
    mode: u32,
    unique: fn() -> u64,
) -> io::Result<(TempFile, File)> {
    for _ in 0..TEMP_NAME_ATTEMPTS {
        let mut temp_name = OsString::from(name);
        temp_name.push(format!(".{:016x}.tmp", unique()));
        let path = dir.join(temp_name);
        match create_exclusive(&path, mode) {
            Ok(file) => return Ok((TempFile { path: Some(path) }, file)),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "every candidate name is taken",
    ))
}

/// `<name>.<16 lowercase hex digits>.tmp`: exactly what `create_temp` makes
/// for `name`, and nothing a user would name a file of their own.
fn is_temp_name_of(candidate: &OsStr, name: &OsStr) -> bool {
    let (Some(candidate), Some(name)) = (candidate.to_str(), name.to_str()) else {
        return false;
    };
    candidate
        .strip_prefix(name)
        .and_then(|rest| rest.strip_prefix('.'))
        .and_then(|rest| rest.strip_suffix(".tmp"))
        .is_some_and(|hex| {
            hex.len() == 16 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
        })
}

/// Unique temp names are never reused, so what a killed process left
/// behind would stay forever. Best effort, and only files old enough that
/// no running write can still own them. Ages are measured against the
/// file just written, not the local clock: on a network filesystem the two
/// may disagree by more than the threshold.
fn remove_stale_temps(dir: &Path, name: &OsStr, just_written: &Path) {
    let now = fs::metadata(just_written)
        .and_then(|meta| meta.modified())
        .unwrap_or_else(|_| SystemTime::now());
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        if !is_temp_name_of(&entry.file_name(), name) {
            continue;
        }
        // DirEntry::metadata does not follow symlinks.
        let stale = entry
            .metadata()
            .and_then(|meta| meta.modified())
            .ok()
            .and_then(|modified| now.duration_since(modified).ok())
            .is_some_and(|age| age > STALE_TEMP_AGE);
        if stale {
            let _ = fs::remove_file(entry.path());
        }
    }
}

/// Unpredictable enough that another local user cannot pre-create every
/// candidate name; uniqueness itself is guaranteed by `create_new`.
pub(crate) fn unique() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQUENCE: AtomicU64 = AtomicU64::new(0);

    // RandomState is seeded from the OS once per process.
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u32(std::process::id());
    hasher.write_u64(SEQUENCE.fetch_add(1, Ordering::Relaxed));
    if let Ok(now) = SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
        hasher.write_u128(now.as_nanos());
    }
    hasher.finish()
}

/// Removes the temp file unless it was renamed away or deliberately kept.
struct TempFile {
    path: Option<PathBuf>,
}

impl TempFile {
    fn path(&self) -> &Path {
        self.path.as_deref().expect("the temp path is set until consumed")
    }

    /// The rename moved the file: there is nothing left to remove.
    fn disarm(&mut self) {
        self.path = None;
    }

    /// Leaves the content on disk and returns where. It moves to an
    /// `.unsaved` name: under its temp name the next successful write would
    /// sweep it away as a leftover.
    fn keep(&mut self) -> PathBuf {
        let temp = self.path.take().expect("the temp path is set until consumed");
        let unsaved = temp.with_extension("unsaved");
        match fs::rename(&temp, &unsaved) {
            Ok(()) => unsaved,
            Err(_) => temp,
        }
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            let _ = fs::remove_file(path);
        }
    }
}

/// Gives `file` the mode of `template`. Best effort: the file was created
/// owner-only, so a filesystem that cannot chmod (FAT, some network
/// mounts) leaves it too strict, never too open.
///
/// Owner and group follow the template only when the directory belongs to
/// the same user as the template — root saving a user's database in that
/// user's own directory must not take the file away from them. Anywhere
/// else the owner of whatever sat at the path proves nothing, and the new
/// file stays with the user who wrote it.
#[cfg(unix)]
fn apply_template(file: &File, template: &fs::Metadata, dir: &Path) {
    use std::os::unix::fs::{MetadataExt, PermissionsExt, fchown};

    let their_own_directory = fs::metadata(dir).is_ok_and(|dir| dir.uid() == template.uid());
    if their_own_directory && let Ok(own) = file.metadata() {
        // Owner before mode (chown may clear mode bits). Only root can give
        // a file away, and a group needs membership: failing is normal.
        if (own.uid() != template.uid() || own.gid() != template.gid())
            && fchown(file, Some(template.uid()), Some(template.gid())).is_err()
            && own.gid() != template.gid()
        {
            let _ = fchown(file, None, Some(template.gid()));
        }
    }
    // Permission bits only: a data file never inherits setuid/setgid/sticky.
    let _ = file.set_permissions(fs::Permissions::from_mode(template.mode() & 0o777));
}

#[cfg(not(unix))]
fn apply_template(_file: &File, _template: &fs::Metadata, _dir: &Path) {}

/// Makes the rename itself durable. Best effort: some filesystems cannot
/// fsync a directory, and the file content is already safe either way.
fn sync_dir(dir: &Path) {
    #[cfg(unix)]
    if let Ok(handle) = File::open(dir) {
        let _ = handle.sync_all();
    }
    #[cfg(not(unix))]
    let _ = dir;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// `replace` with a byte payload.
    fn replace_with(
        path: &Path,
        perms: Perms,
        trust: Trust,
        hooks: &Hooks,
        data: &[u8],
    ) -> io::Result<Replaced> {
        replace(path, perms, trust, hooks, &mut |out| {
            io::Write::write_all(out, data)
        })
    }

    #[test]
    fn creates_and_replaces_without_leaving_temp_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.json");

        assert!(matches!(
            replace_user_file(&path, b"one").unwrap(),
            Replaced::Atomically
        ));
        assert_eq!(fs::read(&path).unwrap(), b"one");

        replace_user_file(&path, b"two").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"two");
        assert_eq!(names_in(dir.path()), ["tasks.json"]);
    }

    #[test]
    fn a_bare_relative_file_name_has_a_usable_parent() {
        assert_eq!(parent_dir(Path::new("tasks.json")), Path::new("."));
        assert_eq!(parent_dir(Path::new("db/tasks.json")), Path::new("db"));
    }

    #[test]
    fn a_failed_write_keeps_the_old_content_and_removes_the_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.json");
        fs::write(&path, "old").unwrap();

        let result = replace(&path, Perms::Keep, Trust::UserData, &REAL, &mut |out| {
            io::Write::write_all(out, b"half of the new cont")?;
            Err(io::Error::other("disk full"))
        });

        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "old");
        assert_eq!(names_in(dir.path()), ["tasks.json"]);
    }

    /// The replaced file is a new inode: whoever still holds the old one
    /// (a hard link here, an open reader in real life) keeps the old,
    /// complete content.
    #[test]
    #[cfg(unix)]
    fn the_old_file_is_never_rewritten_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.json");
        let witness = dir.path().join("witness");
        fs::write(&path, "old").unwrap();
        fs::hard_link(&path, &witness).unwrap();

        replace_user_file(&path, b"new").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(fs::read_to_string(&witness).unwrap(), "old");
    }

    #[cfg(unix)]
    fn mode_of(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[cfg(unix)]
    fn chmod(path: &Path, mode: u32) {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
    }

    /// None of the modes is the 0600 the temp file is created with, so
    /// each of them has to be applied to come out right.
    #[test]
    #[cfg(unix)]
    fn keep_preserves_the_mode_of_the_replaced_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.json");
        for mode in [0o640, 0o664, 0o444] {
            let _ = fs::remove_file(&path);
            fs::write(&path, "old").unwrap();
            chmod(&path, mode);
            replace_user_file(&path, b"new").unwrap();
            assert_eq!(mode_of(&path), mode, "mode {mode:o} was not kept");
            assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        }
    }

    #[test]
    #[cfg(unix)]
    fn private_files_and_copies_as_private_as_their_source() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("tasks.json");
        fs::write(&db, "db").unwrap();
        chmod(&db, 0o640);

        let draft = dir.path().join("editor.draft");
        fs::write(&draft, "older draft").unwrap();
        chmod(&draft, 0o644);
        replace_aux_file(&draft, b"draft", Mode::Private).unwrap();
        assert_eq!(mode_of(&draft), 0o600);

        // A stale read-only copy does not stop the next one (rename does
        // not need write access to the file it replaces).
        let backup = dir.path().join("tasks.json.backup");
        fs::write(&backup, "stale").unwrap();
        chmod(&backup, 0o444);
        copy_to_aux_file(&db, &backup).unwrap();
        assert_eq!(fs::read_to_string(&backup).unwrap(), "db");
        assert_eq!(mode_of(&backup), 0o640);

        // A copy made from the content (a SQLite image) takes the mode too.
        chmod(&backup, 0o644);
        replace_aux_file_like(&backup, b"image", &db).unwrap();
        assert_eq!(fs::read_to_string(&backup).unwrap(), "image");
        assert_eq!(mode_of(&backup), 0o640);
        let kept = dir.path().join("tasks.json.before_restore");
        write_new_file_like(&kept, b"image", &db).unwrap();
        assert_eq!(mode_of(&kept), 0o640);
        let err = write_new_file_like(&kept, b"later", &db).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(&kept).unwrap(), "image");
    }

    #[test]
    #[cfg(unix)]
    fn a_user_symlink_is_followed_and_survives() {
        let dir = tempfile::tempdir().unwrap();
        let real_dir = dir.path().join("real");
        fs::create_dir(&real_dir).unwrap();
        let real = real_dir.join("tasks.json");
        let link = dir.path().join("tasks.json");
        fs::write(&real, "old").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();

        replace_user_file(&link, b"new").unwrap();

        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(fs::read_to_string(&real).unwrap(), "new");
        assert_eq!(names_in(&real_dir), ["tasks.json"]);
    }

    /// Whose link it is cannot be staged without a second account; the
    /// rule itself can be pinned.
    #[test]
    #[cfg(unix)]
    fn only_the_users_own_links_and_roots_are_followed() {
        assert!(may_follow(1000, 1000));
        assert!(may_follow(0, 1000));
        assert!(!may_follow(1001, 1000));
        // Not even root writes through a link somebody else planted.
        assert!(!may_follow(1000, 0));
    }

    #[test]
    #[cfg(unix)]
    fn a_dangling_user_symlink_is_an_error_and_stays_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let link = dir.path().join("tasks.json");
        std::os::unix::fs::symlink(dir.path().join("unmounted/tasks.json"), &link).unwrap();

        let err = replace_user_file(&link, b"new").unwrap_err();

        assert!(err.to_string().contains("symbolic link"), "{err}");
        assert!(fs::symlink_metadata(&link).unwrap().file_type().is_symlink());
        assert_eq!(names_in(dir.path()), ["tasks.json"]);
    }

    #[test]
    #[cfg(unix)]
    fn a_planted_symlink_at_an_own_name_is_replaced_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let victim = dir.path().join("victim.txt");
        let backup = dir.path().join("tasks.json.backup");
        fs::write(&victim, "do not touch").unwrap();
        chmod(&victim, 0o604);
        std::os::unix::fs::symlink(&victim, &backup).unwrap();

        replace_aux_file(&backup, b"backup", Mode::Keep).unwrap();

        assert_eq!(fs::read_to_string(&victim).unwrap(), "do not touch");
        assert!(fs::symlink_metadata(&backup).unwrap().file_type().is_file());
        assert_eq!(fs::read_to_string(&backup).unwrap(), "backup");
        // The victim's mode was not borrowed through the link either.
        assert_ne!(mode_of(&backup), 0o604);
    }

    fn planted_then_free() -> u64 {
        use std::sync::atomic::{AtomicU64, Ordering};
        static CALLS: AtomicU64 = AtomicU64::new(0);
        if CALLS.fetch_add(1, Ordering::Relaxed) == 0 {
            0xaaaa
        } else {
            0xbbbb
        }
    }

    /// The temp file is opened with O_EXCL: something already sitting at
    /// the chosen name — a symlink to a victim here — is never opened, let
    /// alone truncated; another name is picked.
    #[test]
    #[cfg(unix)]
    fn a_symlink_planted_at_the_temp_name_itself_is_not_written_through() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.json");
        let victim = dir.path().join("victim.txt");
        let planted = dir.path().join("tasks.json.000000000000aaaa.tmp");
        fs::write(&victim, "do not touch").unwrap();
        std::os::unix::fs::symlink(&victim, &planted).unwrap();
        let hooks = Hooks {
            rename: rename_over,
            unique: planted_then_free,
        };

        replace_with(&path, Perms::Keep, Trust::UserData, &hooks, b"new").unwrap();

        assert_eq!(fs::read_to_string(&victim).unwrap(), "do not touch");
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert!(fs::symlink_metadata(&planted).unwrap().file_type().is_symlink());
    }

    #[test]
    fn copy_to_new_file_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("tasks.json");
        let to = dir.path().join("tasks.json.before_restore");
        fs::write(&from, "current").unwrap();

        copy_to_new_file(&from, &to).unwrap();
        assert_eq!(fs::read_to_string(&to).unwrap(), "current");

        fs::write(&from, "later").unwrap();
        let err = copy_to_new_file(&from, &to).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(&to).unwrap(), "current");
    }

    #[test]
    #[cfg(unix)]
    fn copy_to_new_file_refuses_a_planted_symlink_and_copies_the_mode() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("tasks.json");
        let victim = dir.path().join("victim.txt");
        let planted = dir.path().join("tasks.json.before_restore");
        fs::write(&from, "current").unwrap();
        chmod(&from, 0o640);
        fs::write(&victim, "do not touch").unwrap();
        std::os::unix::fs::symlink(&victim, &planted).unwrap();

        let err = copy_to_new_file(&from, &planted).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read_to_string(&victim).unwrap(), "do not touch");

        let free = dir.path().join("tasks.json.before_restore.1");
        copy_to_new_file(&from, &free).unwrap();
        assert_eq!(mode_of(&free), 0o640);
    }

    #[test]
    fn temp_names_are_recognized_strictly() {
        let name = OsStr::new("tasks.json");
        let is_temp = |candidate: &str| is_temp_name_of(OsStr::new(candidate), name);
        assert!(is_temp("tasks.json.0123456789abcdef.tmp"));
        assert!(!is_temp("tasks.json.tmp"));
        assert!(!is_temp("tasks.json.0123456789ABCDEF.tmp"));
        assert!(!is_temp("tasks.json.0123456789abcde.tmp"));
        assert!(!is_temp("tasks.json.0123456789abcdef.unsaved"));
        assert!(!is_temp("tasks.json.backup.0123456789abcdef.tmp"));
        assert!(!is_temp("other.json.0123456789abcdef.tmp"));
    }

    #[test]
    fn leftovers_of_a_killed_writer_are_removed_once_stale() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.json");
        let stale = dir.path().join("tasks.json.00000000000000aa.tmp");
        let fresh = dir.path().join("tasks.json.00000000000000bb.tmp");
        let foreign = dir.path().join("notes.json.00000000000000cc.tmp");
        for leftover in [&stale, &fresh, &foreign] {
            fs::write(leftover, "half a database").unwrap();
        }
        let long_ago = SystemTime::now() - 2 * STALE_TEMP_AGE;
        for old in [&stale, &foreign] {
            File::options()
                .write(true)
                .open(old)
                .unwrap()
                .set_modified(long_ago)
                .unwrap();
        }

        replace_user_file(&path, b"new").unwrap();

        assert_eq!(
            names_in(dir.path()),
            [
                "notes.json.00000000000000cc.tmp",
                "tasks.json",
                "tasks.json.00000000000000bb.tmp"
            ]
        );
    }

    /// The only complete copy of a save that failed half-way must outlive
    /// the sweep of stale temp files.
    #[test]
    fn a_kept_copy_does_not_look_like_a_leftover() {
        let dir = tempfile::tempdir().unwrap();
        let (mut temp, file) =
            create_temp(dir.path(), OsStr::new("tasks.json"), PRIVATE, unique).unwrap();
        drop(file);

        let kept = temp.keep();
        drop(temp);

        assert!(kept.exists());
        let kept_name = kept.file_name().unwrap();
        assert!(kept_name.to_string_lossy().ends_with(".unsaved"), "{kept_name:?}");
        assert!(!is_temp_name_of(kept_name, OsStr::new("tasks.json")));
    }

    /// As root the temp file could be created in `/dev`, and the rename
    /// would turn `/dev/null` into a regular file.
    #[test]
    #[cfg(unix)]
    fn a_device_is_written_into_not_replaced() {
        use std::os::unix::fs::FileTypeExt;
        let device = Path::new("/dev/null");

        let outcome = replace_user_file(device, b"discarded").unwrap();

        assert!(matches!(outcome, Replaced::Streamed));
        assert!(fs::metadata(device).unwrap().file_type().is_char_device());
    }

    /// What renaming over a bind-mounted file answers (EBUSY).
    fn refused_rename(_: &Path, _: &Path) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::ResourceBusy, "rename refused"))
    }

    fn disk_full_rename(_: &Path, _: &Path) -> io::Result<()> {
        Err(io::Error::new(io::ErrorKind::StorageFull, "no space left"))
    }

    const REFUSED: Hooks = Hooks {
        rename: refused_rename,
        unique,
    };

    /// Truncating the old content to then fail the same way would turn a
    /// failed save into a destroyed database.
    #[test]
    fn a_rename_that_failed_for_lack_of_space_never_falls_back_to_truncating() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.json");
        fs::write(&path, "old").unwrap();
        let hooks = Hooks {
            rename: disk_full_rename,
            unique,
        };

        let result = replace_with(&path, Perms::Keep, Trust::UserData, &hooks, b"new");

        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::StorageFull);
        assert_eq!(fs::read_to_string(&path).unwrap(), "old");
        assert_eq!(names_in(dir.path()), ["tasks.json"]);
    }

    #[test]
    fn a_user_file_that_cannot_be_renamed_over_is_rewritten_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.json");
        fs::write(&path, "old and longer than the new content").unwrap();

        let outcome = replace_with(&path, Perms::Keep, Trust::UserData, &REFUSED, b"new").unwrap();

        assert!(matches!(outcome, Replaced::InPlace(_)));
        assert_eq!(fs::read_to_string(&path).unwrap(), "new");
        assert_eq!(names_in(dir.path()), ["tasks.json"]);
    }

    #[test]
    fn an_own_name_is_never_rewritten_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.json.backup");
        fs::write(&path, "old").unwrap();

        let result = replace_with(&path, Perms::Keep, Trust::OwnName, &REFUSED, b"new");

        assert!(result.is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "old");
        assert_eq!(names_in(dir.path()), ["tasks.json.backup"]);
    }

    /// `rusk gen -o` into a page pre-created for the user inside a
    /// directory they cannot write to: no temp file is possible, and the
    /// page is regenerated anyway, so it is rewritten. The database in the
    /// same spot is not: that write is refused before anything is touched.
    #[test]
    #[cfg(unix)]
    fn only_generated_output_is_rewritten_where_no_temp_file_is_possible() {
        let dir = tempfile::tempdir().unwrap();
        let locked = dir.path().join("www");
        fs::create_dir(&locked).unwrap();
        let page = locked.join("index.html");
        let db = locked.join("tasks.json");
        fs::write(&page, "previous page, longer than the next").unwrap();
        fs::write(&db, "old").unwrap();
        chmod(&locked, 0o555);
        if File::create(locked.join("probe")).is_ok() {
            chmod(&locked, 0o755);
            eprintln!("skipping: running as a user that ignores directory modes (root?)");
            return;
        }

        let page_outcome = replace_output_file(&page, b"next page");
        let db_outcome = replace_user_file(&db, b"new");
        chmod(&locked, 0o755);

        assert!(matches!(page_outcome.unwrap(), Replaced::InPlace(_)));
        assert_eq!(fs::read_to_string(&page).unwrap(), "next page");
        let err = db_outcome.unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
        assert!(err.to_string().contains("temporary file"), "{err}");
        assert_eq!(fs::read_to_string(&db).unwrap(), "old");
    }

    /// Writers racing on one destination: every write succeeds, the file is
    /// always one complete payload, no temp file is shared or left behind.
    #[test]
    fn concurrent_writers_do_not_interfere() {
        const WRITERS: usize = 8;
        const ROUNDS: usize = 25;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.json");
        let payload = |writer: usize| format!("{writer}").repeat(4096);

        std::thread::scope(|scope| {
            for writer in 0..WRITERS {
                let path = &path;
                scope.spawn(move || {
                    for _ in 0..ROUNDS {
                        replace_user_file(path, payload(writer).as_bytes()).unwrap();
                    }
                });
            }
            let path = &path;
            scope.spawn(move || {
                for _ in 0..ROUNDS * 4 {
                    if let Ok(seen) = fs::read_to_string(path) {
                        assert!(
                            (0..WRITERS).any(|writer| seen == payload(writer)),
                            "a reader saw a partial file ({} bytes)",
                            seen.len()
                        );
                    }
                }
            });
        });

        let last = fs::read_to_string(&path).unwrap();
        assert!((0..WRITERS).any(|writer| last == payload(writer)));
        assert_eq!(names_in(dir.path()), ["tasks.json"]);
    }
}
