//! Optional git layer over the file backend (`git_backend = true`, feature
//! `backend-git`): after every successful save the database file is staged
//! and committed in a repository living in the database directory, giving
//! full history and `git revert`-style undo beyond the single `.backup`
//! copy. Uses the system `git` binary; a missing repository is initialized
//! on first save with a `.gitignore` for the auxiliary files.
//!
//! Failures here never fail the save that already happened — they are
//! reported as warnings.

use super::warn_yellow;
use std::path::Path;
use std::process::Command;

/// Auxiliary siblings that must not clutter the history.
const GITIGNORE: &str = "*.tmp\n*.backup\n*.before_restore\n*.sync\n";

fn git(dir: &Path, args: &[&str]) -> Result<std::process::Output, String> {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "`git` not found in PATH (required for git_backend = true)".to_string()
            } else {
                format!("failed to run git: {e}")
            }
        })
}

/// Runs git and treats a non-zero exit as an error with git's stderr.
fn git_ok(dir: &Path, args: &[&str]) -> Result<(), String> {
    let output = git(dir, args)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git {} failed: {}",
            args.first().unwrap_or(&""),
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}

fn try_commit(db_path: &Path, task_count: usize) -> Result<(), String> {
    let dir = db_path.parent().filter(|p| !p.as_os_str().is_empty());
    let Some(dir) = dir else {
        return Err("database path has no parent directory".to_string());
    };
    let Some(name) = db_path.file_name().and_then(|n| n.to_str()) else {
        return Err("database path has no file name".to_string());
    };

    // An enclosing repository (the database directory may live inside one)
    // is used as-is; only a directory with no repository at all is
    // initialized, so no nested repo ever shadows the user's own.
    let mut pathspec: Vec<&str> = vec![name];
    let inside_repo = git(dir, &["rev-parse", "--git-dir"])?.status.success();
    if !inside_repo {
        git_ok(dir, &["init", "-q"])?;
        let gitignore = dir.join(".gitignore");
        if !gitignore.exists() {
            std::fs::write(&gitignore, GITIGNORE)
                .map_err(|e| format!("failed to write .gitignore: {e}"))?;
        }
        git_ok(dir, &["add", "--", ".gitignore"])?;
        pathspec.push(".gitignore");
    }

    git_ok(dir, &["add", "--", name])?;

    // Everything below stays pathspec-limited so that unrelated changes the
    // user has staged in an enclosing repository are never swept along.
    let mut diff_args = vec!["diff", "--cached", "--quiet", "--"];
    diff_args.extend(&pathspec);
    if git(dir, &diff_args)?.status.success() {
        return Ok(()); // content unchanged: skip the empty commit quietly
    }

    let message = format!("rusk: update {name} ({task_count} tasks)");
    // A local fallback identity keeps commits working where user.name/email
    // are not configured; a configured identity wins as usual.
    let mut commit_args = vec![
        "-c",
        "user.name=rusk",
        "-c",
        "user.email=rusk@localhost",
        "commit",
        "-q",
        "-m",
        &message,
        "--",
    ];
    commit_args.extend(&pathspec);
    git_ok(dir, &commit_args)
}

/// Commits the saved database file; failures become warnings, never errors.
pub fn commit_db(db_path: &Path, task_count: usize) {
    if let Err(e) = try_commit(db_path, task_count) {
        warn_yellow(&format!("Warning: git_backend: {e}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_are_committed_and_history_grows() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("tasks.json");

        std::fs::write(&db, "[]").unwrap();
        try_commit(&db, 0).unwrap();
        assert!(dir.path().join(".git").exists());
        assert!(dir.path().join(".gitignore").exists());

        // Unchanged content: no new commit, no error.
        try_commit(&db, 0).unwrap();

        std::fs::write(&db, "[{}]").unwrap();
        try_commit(&db, 1).unwrap();

        let log = git(dir.path(), &["log", "--oneline"]).unwrap();
        let commits = String::from_utf8_lossy(&log.stdout);
        assert_eq!(commits.lines().count(), 2, "{commits}");
        assert!(commits.contains("rusk: update tasks.json (1 tasks)"));
    }
}
