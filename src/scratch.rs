//! A directory of this user's own below the temp directory.
//!
//! The temp directory (`/tmp`) is shared with every other user of the
//! machine. What rusk keeps there — the database of debug and test builds
//! (see `Backend::resolve`), those of the unit tests, the editor's drafts
//! for a remote database — goes into `<temp>/rusk-<uid>` instead, a
//! directory only its owner may look inside (REVIEW П3, №48).

use anyhow::{Context, Result, bail};
use std::path::{Path, PathBuf};

/// `<temp>/rusk-<uid>`, created if need be, and known to be this user's
/// own: a directory, not a link; theirs, not another user's — anybody may
/// make a `rusk-1000` in `/tmp` before this user does, and would then hold
/// the database of their debug builds; and closed to others, whoever left
/// it open.
pub(crate) fn private_dir() -> Result<PathBuf> {
    private_dir_in(&std::env::temp_dir())
}

fn private_dir_in(temp: &Path) -> Result<PathBuf> {
    let dir = temp.join(format!("rusk-{}", user_id()));
    let created = create_private_dir(&dir);
    // Whatever is there is looked at first: it says more than "File
    // exists" from the creation.
    if let Ok(meta) = std::fs::symlink_metadata(&dir) {
        check_own(&dir, &meta)?;
    }
    created.with_context(|| format!("cannot create {}", dir.display()))?;
    Ok(dir)
}

/// The user this process runs as, for a name of their own below a shared
/// directory.
#[cfg(unix)]
pub(crate) fn user_id() -> String {
    // SAFETY: `geteuid` reads one process property and cannot fail.
    unsafe { libc::geteuid() }.to_string()
}

#[cfg(not(unix))]
pub(crate) fn user_id() -> String {
    std::env::var("USERNAME").unwrap_or_else(|_| "user".to_string())
}

/// Creates `dir`, and what leads to it, so that only its owner may look
/// inside; one that exists is left as it is.
#[cfg(unix)]
pub(crate) fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)
}

#[cfg(not(unix))]
pub(crate) fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)
}

#[cfg(unix)]
fn check_own(dir: &Path, meta: &std::fs::Metadata) -> Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let shared = "the temp directory is shared with every user of this machine";
    // SAFETY: `geteuid` reads one process property and cannot fail.
    let uid = unsafe { libc::geteuid() };
    if !meta.is_dir() {
        let what = if meta.file_type().is_symlink() { "a symbolic link" } else { "a file" };
        // On a sticky temp directory only its owner can remove it.
        let remove = if meta.uid() == uid {
            "remove it".to_string()
        } else {
            format!("it belongs to uid {}, ask them to remove it", meta.uid())
        };
        bail!(
            "{} is {what}, not a directory of your own: {remove}, or point TMPDIR at a \
             directory of yours ({shared})",
            dir.display()
        );
    }
    if meta.uid() != uid {
        bail!(
            "{} belongs to another user (uid {}), not to you: ask them to remove it, or point \
             TMPDIR at a directory of your own ({shared})",
            dir.display(),
            meta.uid()
        );
    }
    if meta.mode() & 0o077 != 0 {
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))
            .with_context(|| format!("cannot close {} to other users", dir.display()))?;
    }
    Ok(())
}

/// Elsewhere the temp directory is the user's own already.
#[cfg(not(unix))]
fn check_own(_: &Path, _: &std::fs::Metadata) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// REVIEW П3: the directory names the user and admits nobody else.
    #[test]
    fn the_directory_is_the_users_own() {
        let temp = tempfile::tempdir().unwrap();
        let dir = private_dir_in(temp.path()).unwrap();
        assert_eq!(dir, temp.path().join(format!("rusk-{}", user_id())));
        assert!(dir.is_dir());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = || std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(), 0o700, "{:o}", mode());
            // One left open (an older rusk, a chmod) is closed again.
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o755)).unwrap();
            assert_eq!(private_dir_in(temp.path()).unwrap(), dir);
            assert_eq!(mode(), 0o700, "{:o}", mode());
        }
    }

    /// REVIEW П3: a `rusk-<uid>` somebody else put there first is not
    /// taken for the user's own — a link least of all.
    #[test]
    #[cfg(unix)]
    fn a_link_in_its_place_is_refused() {
        let temp = tempfile::tempdir().unwrap();
        let elsewhere = temp.path().join("elsewhere");
        std::fs::create_dir(&elsewhere).unwrap();
        let name = format!("rusk-{}", user_id());
        std::os::unix::fs::symlink(&elsewhere, temp.path().join(&name)).unwrap();
        let err = private_dir_in(temp.path()).unwrap_err().to_string();
        assert!(err.contains("is a symbolic link, not a directory of your own"), "{err}");
        assert!(err.contains(&name), "{err}");

        std::fs::remove_file(temp.path().join(&name)).unwrap();
        std::fs::write(temp.path().join(&name), "").unwrap();
        let err = private_dir_in(temp.path()).unwrap_err().to_string();
        assert!(err.contains("is a file, not a directory of your own"), "{err}");
    }
}
