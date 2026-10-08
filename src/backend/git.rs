//! Optional git layer over the file backend (`git_backend = true`, feature
//! `backend-git`): after every successful save the database file is staged
//! and committed in a repository living in the database directory, giving
//! full history and `git revert`-style undo beyond the single `.backup`
//! copy. Uses the system `git` binary (2.9 or newer).
//!
//! An enclosing repository is used as it is. Only a directory that git
//! itself says is in no repository — and that has no repository above it
//! that git would find without its `GIT_CEILING_DIRECTORIES` and filesystem
//! boundaries — is initialized, with a `.gitignore` for the auxiliary files;
//! never the home directory, the filesystem root, or a directory other users
//! can write to (the temp directory). The auxiliary files of the database
//! are kept out of `git status` in any repository through its
//! `info/exclude`. A symlinked database is committed where the file is.
//!
//! git runs with the configuration of the repository, as `git commit` there
//! would — which is why rusk does not run it in a repository other users
//! can change: its config could run anything (a filter, `gpg.program`, a
//! hook of its own), SECURITY.md M1. Nor does any hook run: neither the
//! repository's hook scripts nor the hooks its config defines, whose
//! failure would lose the history of a save. The identity is git's own;
//! `rusk <rusk@localhost>` stands in only where git has none.
//!
//! Failures here never fail the save that already happened — they are
//! reported as warnings.

use super::file::Change;
use super::warn_yellow;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Auxiliary siblings that must not clutter the history.
const GITIGNORE: &str = "*.tmp\n*.backup\n*.before_restore*\n*.sync\n*.lock\n*.draft\n*.draft.corrupt\n";

/// The variables through which the environment points git at a repository
/// of its choosing (`git rev-parse --local-env-vars`): the repository meant
/// is the one of the database directory, whatever rusk was started from.
const REPOSITORY_ENV: &[&str] = &[
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CONFIG",
    "GIT_CONFIG_PARAMETERS",
    "GIT_CONFIG_COUNT",
    "GIT_OBJECT_DIRECTORY",
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_IMPLICIT_WORK_TREE",
    "GIT_GRAFT_FILE",
    "GIT_INDEX_FILE",
    "GIT_NO_REPLACE_OBJECTS",
    "GIT_REPLACE_REF_BASE",
    "GIT_PREFIX",
    "GIT_SHALLOW_FILE",
    "GIT_COMMON_DIR",
];

/// Where hooks are looked for: somewhere no hook can be. On Windows that is
/// the rusk executable, a file (`NUL` would be a path relative to the
/// repository there).
fn no_hooks() -> OsString {
    if cfg!(windows) {
        std::env::current_exe().map_or_else(|_| OsString::from("NUL"), PathBuf::into_os_string)
    } else {
        OsString::from("/dev/null")
    }
}

/// git in one repository directory, with the hooks it would run switched
/// off: the hook scripts (`core.hooksPath`), fsmonitor, and — once they are
/// known — the hooks its config defines.
struct Git<'a> {
    dir: &'a Path,
    settings: Vec<String>,
}

impl<'a> Git<'a> {
    fn new(dir: &'a Path) -> Self {
        Self {
            dir,
            settings: Vec::new(),
        }
    }

    /// Adds `-c key=value` to every call from now on.
    fn set(&mut self, key: &str, value: &str) {
        self.settings.push("-c".to_string());
        self.settings.push(format!("{key}={value}"));
    }

    fn run(&self, args: &[&str]) -> Result<Output, String> {
        let mut hooks_path = OsString::from("core.hooksPath=");
        hooks_path.push(no_hooks());
        let mut cmd = Command::new("git");
        cmd.arg("-C")
            .arg(self.dir)
            .arg("-c")
            .arg(hooks_path)
            .args(["-c", "core.fsmonitor=false", "--literal-pathspecs"])
            .args(&self.settings)
            .args(args)
            // git's own words, to tell its answers apart.
            .env("LC_ALL", "C");
        for var in REPOSITORY_ENV {
            cmd.env_remove(var);
        }
        // Unit tests run git without the developer's own configuration and
        // identity.
        #[cfg(test)]
        {
            cmd.env("GIT_CONFIG_GLOBAL", if cfg!(windows) { "NUL" } else { "/dev/null" })
                .env("GIT_CONFIG_NOSYSTEM", "1");
            for var in ["EMAIL", "GIT_AUTHOR_NAME", "GIT_AUTHOR_EMAIL", "GIT_COMMITTER_NAME", "GIT_COMMITTER_EMAIL"] {
                cmd.env_remove(var);
            }
        }
        cmd.output().map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "`git` not found in PATH (required for git_backend = true)".to_string()
            } else {
                format!("failed to run git: {e}")
            }
        })
    }

    /// [`run`](Self::run), with a non-zero exit an error that names the
    /// subcommand and carries git's stderr.
    fn ok(&self, args: &[&str]) -> Result<Output, String> {
        let output = self.run(args)?;
        if output.status.success() {
            Ok(output)
        } else {
            Err(format!("git {} failed: {}", args.first().unwrap_or(&""), stderr_of(&output)))
        }
    }
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).trim().to_string()
}

/// A path git printed, as it is: not every path is UTF-8.
fn path_from(bytes: &[u8]) -> PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        PathBuf::from(OsString::from_vec(bytes.to_vec()))
    }
    #[cfg(not(unix))]
    {
        PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
    }
}

/// The repository around a directory, as git sees it.
#[derive(Debug)]
struct Repository {
    git_dir: PathBuf,
    common_dir: PathBuf,
    in_work_tree: bool,
    exclude: PathBuf,
    /// The directory relative to the top of the work tree, `sub/` or empty.
    prefix: String,
}

/// What git says about the repository around the directory of `git`: one
/// call. `None` is no repository at all — only where git says exactly that
/// and no repository git would find is above: any other failure, a
/// repository of another owner above all, is no licence to start a
/// repository inside the user's own (REVIEW №177).
fn repository(git: &Git) -> Result<Option<Repository>, String> {
    let output = git.run(&[
        "rev-parse",
        "--absolute-git-dir",
        "--git-common-dir",
        "--is-inside-work-tree",
        "--git-path",
        "info/exclude",
        "--show-prefix",
    ])?;
    if output.status.success() {
        let lines: Vec<&[u8]> = output.stdout.split(|&b| b == b'\n').collect();
        let [git_dir, common_dir, in_work_tree, exclude, prefix, ..] = lines[..] else {
            return Err(format!("git rev-parse said too little: {}", String::from_utf8_lossy(&output.stdout)));
        };
        let absolute = |bytes: &[u8]| {
            let path = path_from(bytes);
            if path.is_absolute() { path } else { git.dir.join(path) }
        };
        return Ok(Some(Repository {
            git_dir: path_from(git_dir),
            common_dir: absolute(common_dir),
            in_work_tree: in_work_tree == b"true",
            exclude: absolute(exclude),
            prefix: String::from_utf8_lossy(prefix).into_owned(),
        }));
    }
    let stderr = stderr_of(&output);
    if stderr.contains("dubious ownership") {
        // Not git's advice to add a `safe.directory`: that is what would let
        // the repository's config run things as the user.
        return Err(format!(
            "the repository around '{}' belongs to another user, and git does not trust it; \
             rusk does not run git in it — keep the database in a directory of your own",
            git.dir.display()
        ));
    }
    if stderr.contains("not a git repository") {
        let ceilings: Vec<PathBuf> = std::env::var_os("GIT_CEILING_DIRECTORIES")
            .map(|value| std::env::split_paths(&value).filter(|p| p.is_absolute()).collect())
            .unwrap_or_default();
        let across_filesystems = std::env::var("GIT_DISCOVERY_ACROSS_FILESYSTEM")
            .is_ok_and(|value| matches!(value.to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on"));
        return match git_dir_above(git.dir, &ceilings, across_filesystems) {
            None => Ok(None),
            Some(found) => Err(format!(
                "git does not use the repository at '{}' ({}), and rusk does not start another \
                 inside it",
                found.display(),
                stderr.lines().next().unwrap_or_default()
            )),
        };
    }
    Err(format!("git cannot use the repository around '{}': {stderr}", git.dir.display()))
}

/// The `.git` git would find for `dir` if it could use it: in `dir` or
/// above, but not in or above a `ceiling` (`GIT_CEILING_DIRECTORIES`) and
/// not beyond a filesystem boundary unless `across_filesystems`; and a
/// `.git` that is no repository (an empty directory) is none. A `.git`
/// that git found and refused is one (review of R22: an unused `.git`
/// above blocked every commit for good).
fn git_dir_above(dir: &Path, ceilings: &[PathBuf], across_filesystems: bool) -> Option<PathBuf> {
    let looks_like_one = |git: &Path| git.join("HEAD").is_file() || std::fs::read(git).is_ok_and(|b| b.starts_with(b"gitdir:"));
    #[cfg(unix)]
    let device = |path: &Path| {
        use std::os::unix::fs::MetadataExt;
        std::fs::metadata(path).map(|m| m.dev()).ok()
    };
    #[cfg(unix)]
    let start = device(dir);
    let is_ceiling = |path: &Path| {
        ceilings.iter().any(|ceiling| {
            ceiling == path || ceiling.canonicalize().map(plain).is_ok_and(|c| c == path)
        })
    };
    for current in dir.ancestors() {
        let candidate = current.join(".git");
        if looks_like_one(&candidate) {
            return Some(candidate);
        }
        let parent = current.parent()?;
        if is_ceiling(parent) {
            return None;
        }
        #[cfg(unix)]
        if !across_filesystems && device(parent) != start {
            return None;
        }
        #[cfg(not(unix))]
        let _ = across_filesystems;
    }
    None
}

/// Why other users could change `path` — and with it, when it is part of a
/// repository, what git runs: `None` when only this user can. A directory
/// or file of another user, one anybody may write to, or one writable by a
/// group that is not the user's own.
#[cfg(unix)]
fn writable_by_others(path: &Path) -> Option<String> {
    use std::os::unix::fs::MetadataExt;
    let meta = std::fs::metadata(path).ok()?;
    // SAFETY: both only read a property of this process and cannot fail.
    let (uid, gid) = unsafe { (libc::geteuid(), libc::getegid()) };
    if meta.uid() != uid {
        Some(format!("'{}' belongs to another user", path.display()))
    } else if meta.mode() & 0o002 != 0 {
        Some(format!("anybody may write to '{}'", path.display()))
    } else if meta.mode() & 0o020 != 0 && meta.gid() != gid {
        Some(format!("a group of other users may write to '{}'", path.display()))
    } else {
        None
    }
}

#[cfg(not(unix))]
fn writable_by_others(_: &Path) -> Option<String> {
    None
}

/// Why other users could change what git reads from `repo` when rusk runs
/// it there: its config, which can run commands (a filter, `gpg.program`,
/// hooks of its own), and `info/` (attributes and excludes). SECURITY.md
/// M1: a repository shared with others is theirs to program.
fn repository_writable_by_others(repo: &Repository) -> Option<String> {
    [
        repo.git_dir.clone(),
        repo.common_dir.clone(),
        repo.common_dir.join("config"),
        repo.git_dir.join("config.worktree"),
        repo.common_dir.join("info"),
    ]
    .iter()
    .find_map(|path| writable_by_others(path))
}

/// Directories rusk does not turn into a repository of its own: one there
/// would take in everything under it (REVIEW №98), or would be a repository
/// other users can program (the temp directory, a shared one).
fn unfit_for_a_repository(dir: &Path) -> Option<String> {
    // `dir` is canonical and plain (no `\\?\` on Windows): the other side
    // has to be the same to compare equal.
    let same = |other: Option<PathBuf>| {
        other.and_then(|p| p.canonicalize().ok()).map(plain).as_deref() == Some(dir)
    };
    if dir.parent().is_none() {
        Some("the root of the filesystem".to_string())
    } else if same(std::env::home_dir()) {
        Some("your home directory".to_string())
    } else if same(Some(std::env::temp_dir())) {
        Some("the temp directory".to_string())
    } else {
        writable_by_others(dir).map(|_| "a directory other users can write to".to_string())
    }
}

/// `text` as a gitignore pattern that matches it literally.
fn literal(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (i, c) in text.chars().enumerate() {
        if matches!(c, '*' | '?' | '[' | '\\') || (i == 0 && matches!(c, '#' | '!')) {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// Keeps the auxiliary files of the database `name` out of `git status`, in
/// any repository: its `info/exclude` names them, anchored to the database
/// directory, so nothing of the user's own elsewhere in the repository is
/// hidden (REVIEW №100). Nothing is committed for it. The file is read as
/// bytes and only added to; a name with a line break in it is left out, as
/// no pattern can hold one.
fn exclude_aux_files(repo: &Repository, name: &str) -> Result<(), String> {
    if name.contains(['\n', '\r']) || repo.prefix.contains(['\n', '\r']) {
        return Err(format!(
            "its auxiliary files may show in `git status`: a pattern of {} cannot hold the \
             line break in the path of the database",
            repo.exclude.display()
        ));
    }
    let at = format!("/{}", literal(&repo.prefix));
    let db = literal(name);
    let patterns = [
        format!("{at}{db}.backup"),
        format!("{at}{db}.before_restore*"),
        format!("{at}{db}.sync"),
        format!("{at}{db}.lock"),
        format!("{at}{db}.*.tmp"),
        format!("{at}editor-*.draft"),
        format!("{at}editor-*.draft.corrupt"),
    ];
    let current = match std::fs::read(&repo.exclude) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(format!("failed to read '{}': {e}", repo.exclude.display())),
    };
    let missing: Vec<&String> = patterns
        .iter()
        .filter(|p| !current.split(|&b| b == b'\n').any(|line| line == p.as_bytes()))
        .collect();
    if missing.is_empty() {
        return Ok(());
    }
    let mut added = String::new();
    if !current.is_empty() && !current.ends_with(b"\n") {
        added.push('\n');
    }
    added.push_str(&format!(
        "# rusk: auxiliary files of {}\n",
        crate::printable::escape(&format!("{}{name}", repo.prefix))
    ));
    for pattern in missing {
        added.push_str(pattern);
        added.push('\n');
    }
    if let Some(parent) = repo.exclude.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("failed to create '{}': {e}", parent.display()))?;
    }
    use std::io::Write;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&repo.exclude)
        .and_then(|mut file| file.write_all(added.as_bytes()))
        .map_err(|e| format!("failed to update '{}': {e}", repo.exclude.display()))
}

/// Switches off the hooks the repository's config defines (`hook.<name>.
/// command`, git 2.46): `core.hooksPath` does not reach them. A hook whose
/// name `-c` cannot carry is reason not to run git at all.
fn quiet_config_hooks(git: &mut Git) -> Result<(), String> {
    let output = git.run(&["config", "--name-only", "--get-regexp", r"^hook\."])?;
    let listed = String::from_utf8_lossy(&output.stdout).into_owned();
    let mut names: Vec<&str> = listed
        .lines()
        .filter_map(|key| key.strip_prefix("hook."))
        .filter_map(|rest| rest.strip_suffix(".command").or_else(|| rest.strip_suffix(".event")))
        .collect();
    names.sort_unstable();
    names.dedup();
    for name in names {
        if name.contains(['=', '\n']) {
            return Err(format!(
                "the repository's config defines a hook, '{}', that rusk cannot switch off",
                crate::printable::escape(name)
            ));
        }
        git.set(&format!("hook.{name}.enabled"), "false");
    }
    Ok(())
}

/// What of an identity git could not commit without, from its words: the
/// settings that stand in for it. git makes an identity up from the user
/// and host names where none is configured, and takes it where the host
/// has a domain; where it has none, or `user.useConfigOnly` forbids making
/// one up, it says so — in one of these words for the name, one of those
/// for the email.
fn missing_identity(stderr: &str) -> Vec<(&'static str, &'static str)> {
    let mut missing = Vec::new();
    if stderr.contains("empty ident name")
        || stderr.contains("no name was given")
        || stderr.contains("unable to auto-detect name")
    {
        missing.push(("user.name", "rusk"));
    }
    if stderr.contains("unable to auto-detect email address") || stderr.contains("no email was given") {
        missing.push(("user.email", "rusk@localhost"));
    }
    missing
}

/// The directory of the database: a bare file name is in the current one
/// (REVIEW №96: it had "no parent directory" and was never committed).
fn database_dir(db_path: &Path) -> &Path {
    match db_path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    }
}

/// `path` without the `\\?\` in front that `canonicalize` gives on Windows,
/// where it can be done without changing what the path names.
fn plain(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    if let Some(text) = path.to_str()
        && let Some(rest) = text.strip_prefix(r"\\?\")
        && !rest.starts_with("UNC\\")
    {
        return PathBuf::from(rest);
    }
    path
}

fn try_commit(db_path: &Path, task_count: usize, change: Change) -> Result<(), String> {
    // The file itself: a symlink would be committed as a link, with no
    // history of the tasks at all (review of R22).
    let db_path = std::fs::canonicalize(db_path).map(plain).unwrap_or_else(|_| db_path.to_path_buf());
    let dir = database_dir(&db_path);
    let dir = dir
        .canonicalize()
        .map(plain)
        .or_else(|_| std::path::absolute(dir))
        .map_err(|e| format!("cannot find the database directory '{}': {e}", dir.display()))?;
    let Some(name) = db_path.file_name().and_then(|n| n.to_str()) else {
        return Err("database path has no file name".to_string());
    };
    let mut git = Git::new(&dir);

    let mut pathspec: Vec<&str> = vec![name];
    let repo = match repository(&git)? {
        Some(repo) => repo,
        None => {
            if let Some(what) = unfit_for_a_repository(&dir) {
                return Err(format!(
                    "the database is directly in {what} ('{}'), which rusk does not make a git \
                     repository. Keep the database in a directory of its own (e.g. \
                     ~/.local/share/rusk/tasks.json), or create a repository there yourself",
                    dir.display()
                ));
            }
            git.ok(&["init", "-q"])?;
            warn_yellow(&format!(
                "Note: git_backend: created a git repository in '{}' for the history of {name}",
                dir.display()
            ));
            // Never through a link somebody put there (review of R22).
            let created = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(dir.join(".gitignore"));
            match created {
                Ok(mut file) => {
                    use std::io::Write;
                    file.write_all(GITIGNORE.as_bytes())
                        .map_err(|e| format!("failed to write .gitignore: {e}"))?;
                    git.ok(&["add", "--", ".gitignore"])?;
                    pathspec.push(".gitignore");
                }
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(format!("failed to write .gitignore: {e}")),
            }
            repository(&git)?.ok_or("git init made no repository")?
        }
    };
    if !repo.in_work_tree {
        return Err(format!(
            "the database is inside the git directory '{}', not in a work tree; nothing is \
             committed",
            repo.git_dir.display()
        ));
    }
    if let Some(why) = repository_writable_by_others(&repo) {
        return Err(format!(
            "rusk does not run git in the repository at '{}': other users could program it \
             ({why}). Keep the database in a repository only you can change",
            repo.git_dir.display()
        ));
    }
    quiet_config_hooks(&mut git)?;
    // Keeping them out of `git status` is a convenience: no reason to keep
    // the save out of the history (review of R22).
    if let Err(e) = exclude_aux_files(&repo, name) {
        warn_yellow(&format!("Warning: git_backend: {e}"));
    }

    let added = git.run(&["add", "--", name])?;
    if !added.status.success() {
        let stderr = stderr_of(&added);
        if stderr.contains("ignored by one of your .gitignore files") {
            return Err(format!(
                "the repository at '{}' ignores {name}, so nothing is committed; take it out of \
                 the ignore files there, or keep the database in a directory of its own",
                repo.git_dir.display()
            ));
        }
        return Err(format!("git add failed: {stderr}"));
    }

    // Everything below stays pathspec-limited so that unrelated changes the
    // user has staged in an enclosing repository are never swept along.
    let mut diff_args = vec!["diff", "--cached", "--quiet", "--"];
    diff_args.extend(&pathspec);
    if git.run(&diff_args)?.status.success() {
        return Ok(()); // content unchanged: skip the empty commit quietly
    }

    let message = match change {
        Change::Update => format!("rusk: update {name} ({task_count} tasks)"),
        Change::Restore => format!("rusk: restore {name} from backup ({task_count} tasks)"),
    };
    let mut commit_args = vec!["commit", "-q", "--no-verify", "-m", &message, "--"];
    commit_args.extend(&pathspec);
    // The identity git has — configured, from `EMAIL`, `author.*`, the
    // environment — is the one used (REVIEW №21); only the part git has not
    // got does `rusk <rusk@localhost>` stand in for.
    let mut filled: Vec<&str> = Vec::new();
    loop {
        let committed = git.run(&commit_args)?;
        if committed.status.success() {
            return Ok(());
        }
        let stderr = stderr_of(&committed);
        let missing: Vec<_> = missing_identity(&stderr)
            .into_iter()
            .filter(|(key, _)| !filled.contains(key))
            .collect();
        if missing.is_empty() {
            return Err(format!("git commit failed: {stderr}"));
        }
        for (key, value) in missing {
            git.set(key, value);
            filled.push(key);
        }
    }
}

/// Commits the saved database file; failures become warnings, never errors.
pub(super) fn commit_db(db_path: &Path, task_count: usize, change: Change) {
    if let Err(e) = try_commit(db_path, task_count, change) {
        warn_yellow(&format!("Warning: git_backend: {e}"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// git as the user would run it in `dir` — without the developer's own
    /// configuration, as rusk's calls in unit tests.
    fn git_in(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("LC_ALL", "C")
            .env("GIT_CONFIG_GLOBAL", if cfg!(windows) { "NUL" } else { "/dev/null" })
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn git_available() -> bool {
        Command::new("git").arg("--version").output().is_ok()
    }

    /// A repository of Alice's in a fresh directory.
    fn alices_repository() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        git_in(dir.path(), &["init", "-q"]);
        git_in(dir.path(), &["config", "user.name", "Alice"]);
        git_in(dir.path(), &["config", "user.email", "alice@example.com"]);
        dir
    }

    #[test]
    fn saves_are_committed_and_history_grows() {
        if !git_available() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("tasks.json");

        std::fs::write(&db, "[]").unwrap();
        try_commit(&db, 0, Change::Update).unwrap();
        assert!(dir.path().join(".git").exists());
        assert!(dir.path().join(".gitignore").exists());

        // Unchanged content: no new commit, no error.
        try_commit(&db, 0, Change::Update).unwrap();

        std::fs::write(&db, "[{}]").unwrap();
        try_commit(&db, 1, Change::Update).unwrap();

        // A restore is a commit of its own, named as such.
        std::fs::write(&db, "[]").unwrap();
        try_commit(&db, 0, Change::Restore).unwrap();

        let commits = git_in(dir.path(), &["log", "--oneline"]);
        assert_eq!(commits.lines().count(), 3, "{commits}");
        assert!(commits.contains("rusk: update tasks.json (1 tasks)"));
        assert!(commits.contains("rusk: restore tasks.json from backup (0 tasks)"));
        // Whose the commits are depends on the host: git makes an identity
        // up from the user and host names and takes it where the host has
        // a domain (a CI runner); see the next test for rusk's stand-in.
    }

    /// The identity git has is used (REVIEW №21); what it has not got,
    /// `rusk <rusk@localhost>` stands in for. `user.useConfigOnly` makes
    /// that the same on every host: without it, a host with a domain in
    /// its name gives git an identity to make up, a laptop does not.
    #[test]
    fn rusk_stands_in_for_the_identity_git_has_not_got() {
        if !git_available() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        git_in(dir.path(), &["init", "-q"]);
        git_in(dir.path(), &["config", "user.useConfigOnly", "true"]);
        let db = dir.path().join("tasks.json");
        std::fs::write(&db, "[]").unwrap();
        try_commit(&db, 0, Change::Update).unwrap();
        assert_eq!(
            git_in(dir.path(), &["log", "-1", "--format=%an <%ae>"]).trim(),
            "rusk <rusk@localhost>"
        );

        // A name git has got is kept; only the email is rusk's.
        let dir = tempfile::tempdir().unwrap();
        git_in(dir.path(), &["init", "-q"]);
        git_in(dir.path(), &["config", "user.useConfigOnly", "true"]);
        git_in(dir.path(), &["config", "user.name", "Alice"]);
        let db = dir.path().join("tasks.json");
        std::fs::write(&db, "[]").unwrap();
        try_commit(&db, 0, Change::Update).unwrap();
        assert_eq!(
            git_in(dir.path(), &["log", "-1", "--format=%an <%ae>"]).trim(),
            "Alice <rusk@localhost>"
        );
    }

    /// git's words for each part of an identity it has not got, with and
    /// without `user.useConfigOnly`.
    #[test]
    fn missing_identity_reads_every_wording_of_git() {
        assert_eq!(
            missing_identity("fatal: empty ident name (for <a@b>) not allowed"),
            vec![("user.name", "rusk")]
        );
        assert_eq!(
            missing_identity("fatal: no name was given and auto-detection is disabled"),
            vec![("user.name", "rusk")]
        );
        assert_eq!(
            missing_identity("fatal: unable to auto-detect name (got '')"),
            vec![("user.name", "rusk")]
        );
        assert_eq!(
            missing_identity("fatal: unable to auto-detect email address (got 'alex@host.(none)')"),
            vec![("user.email", "rusk@localhost")]
        );
        assert_eq!(
            missing_identity("fatal: no email was given and auto-detection is disabled"),
            vec![("user.email", "rusk@localhost")]
        );
        assert!(missing_identity("fatal: not a git repository").is_empty());
    }

    #[test]
    fn a_bare_file_name_is_in_the_current_directory() {
        assert_eq!(database_dir(Path::new("tasks.json")), Path::new("."));
        assert_eq!(database_dir(Path::new("./tasks.json")), Path::new("."));
        assert_eq!(database_dir(Path::new("/srv/t/tasks.json")), Path::new("/srv/t"));
    }

    #[test]
    fn a_name_is_a_literal_pattern() {
        assert_eq!(literal("tasks.json"), "tasks.json");
        assert_eq!(literal("my*tasks[1].json"), "my\\*tasks\\[1].json");
        assert_eq!(literal("#notes.json"), "\\#notes.json");
        assert_eq!(literal("a!b"), "a!b");
    }

    /// SECURITY.md M1: a hook of the repository ran on every save — a hook
    /// script, and (review of R22) a hook its config defines.
    #[test]
    #[cfg(unix)]
    fn no_hook_of_the_repository_runs() {
        use std::os::unix::fs::PermissionsExt;
        if !git_available() {
            return;
        }
        let dir = alices_repository();
        let ran = dir.path().join("HOOK_RAN");
        for hook in ["pre-commit", "commit-msg", "post-commit"] {
            let path = dir.path().join(".git").join("hooks").join(hook);
            std::fs::write(&path, format!("#!/bin/sh\ntouch '{}'\n", ran.display())).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let from_config = dir.path().join("CONFIG_HOOK_RAN");
        for event in ["pre-commit", "post-commit", "post-index-change", "reference-transaction"] {
            git_in(dir.path(), &["config", &format!("hook.h-{event}.event"), event]);
            git_in(
                dir.path(),
                &["config", &format!("hook.h-{event}.command"), &format!("touch '{}'", from_config.display())],
            );
        }
        let db = dir.path().join("tasks.json");
        std::fs::write(&db, "[]").unwrap();
        try_commit(&db, 0, Change::Update).unwrap();
        assert!(!ran.exists(), "a hook script ran");
        assert!(!from_config.exists(), "a hook of the config ran");
        assert!(git_in(dir.path(), &["log", "--oneline"]).contains("rusk: update"));
    }

    /// Review of R22: a repository other users can change is theirs to
    /// program — a filter of its config ran on every save.
    #[test]
    #[cfg(unix)]
    fn a_repository_others_can_change_is_left_alone() {
        use std::os::unix::fs::PermissionsExt;
        if !git_available() {
            return;
        }
        let dir = alices_repository();
        let ran = dir.path().join("FILTER_RAN");
        std::fs::write(dir.path().join(".git").join("info").join("attributes"), "* filter=evil\n").unwrap();
        git_in(dir.path(), &["config", "filter.evil.clean", &format!("sh -c 'touch {}; cat'", ran.display())]);
        let config = dir.path().join(".git").join("config");
        std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o666)).unwrap();
        let db = dir.path().join("tasks.json");
        std::fs::write(&db, "[]").unwrap();
        let err = try_commit(&db, 0, Change::Update).unwrap_err();
        assert!(err.contains("other users could program it") && err.contains("anybody may write"), "{err}");
        assert!(!ran.exists(), "the filter ran");

        // And no repository is started in a directory anybody may write to.
        let shared = tempfile::tempdir().unwrap();
        std::fs::set_permissions(shared.path(), std::fs::Permissions::from_mode(0o1777)).unwrap();
        let db = shared.path().join("tasks.json");
        std::fs::write(&db, "[]").unwrap();
        let err = try_commit(&db, 0, Change::Update).unwrap_err();
        assert!(err.contains("other users can write to"), "{err}");
        assert!(!shared.path().join(".git").exists());
    }

    /// Review of R22: a `.git` that git does not use — above a ceiling,
    /// or empty — blocked every commit for good.
    #[test]
    fn only_a_repository_git_would_find_is_one_above() {
        let root = tempfile::tempdir().unwrap();
        let root_path = root.path().canonicalize().unwrap();
        let inner = root_path.join("a").join("b");
        std::fs::create_dir_all(&inner).unwrap();
        assert_eq!(git_dir_above(&inner, &[], false), None);
        // An empty `.git` is none.
        std::fs::create_dir(root_path.join(".git")).unwrap();
        assert_eq!(git_dir_above(&inner, &[], false), None);
        std::fs::write(root_path.join(".git").join("HEAD"), "ref: refs/heads/main\n").unwrap();
        assert_eq!(git_dir_above(&inner, &[], false), Some(root_path.join(".git")));
        // Behind a ceiling it is out of sight.
        assert_eq!(git_dir_above(&inner, std::slice::from_ref(&root_path), false), None);
        assert_eq!(git_dir_above(&inner, &[root_path.join("a")], false), None);
        // A gitfile is one.
        std::fs::write(inner.join(".git"), "gitdir: /elsewhere\n").unwrap();
        assert_eq!(git_dir_above(&inner, &[], false), Some(inner.join(".git")));
    }

    /// REVIEW №100, review of R22: the patterns go in once, anchored; the
    /// file is read as bytes and kept; a line break in the path is refused.
    #[test]
    fn the_aux_files_are_excluded_once_and_the_rest_is_kept() {
        let dir = tempfile::tempdir().unwrap();
        let exclude = dir.path().join("exclude");
        std::fs::write(&exclude, b"# caf\xe9 notes\n*.log").unwrap();
        let repo = Repository {
            git_dir: dir.path().to_path_buf(),
            common_dir: dir.path().to_path_buf(),
            in_work_tree: true,
            exclude: exclude.clone(),
            prefix: "sub/".to_string(),
        };
        exclude_aux_files(&repo, "tasks.json").unwrap();
        exclude_aux_files(&repo, "tasks.json").unwrap();
        let written = std::fs::read(&exclude).unwrap();
        assert!(written.starts_with(b"# caf\xe9 notes\n*.log\n# rusk: auxiliary files of sub/tasks.json\n"));
        let text = String::from_utf8_lossy(&written);
        assert_eq!(text.matches("/sub/tasks.json.backup").count(), 1, "{text}");
        let err = exclude_aux_files(&repo, "x\n*").unwrap_err();
        assert!(err.contains("line break"), "{err}");
        assert!(!String::from_utf8_lossy(&std::fs::read(&exclude).unwrap()).lines().any(|l| l == "*"));
    }

    /// Review of R22: `.gitignore` was written through a link somebody put
    /// in its place.
    #[test]
    #[cfg(unix)]
    fn a_gitignore_in_place_is_not_written_through() {
        if !git_available() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("victim");
        std::os::unix::fs::symlink(&target, dir.path().join(".gitignore")).unwrap();
        let db = dir.path().join("tasks.json");
        std::fs::write(&db, "[]").unwrap();
        try_commit(&db, 0, Change::Update).unwrap();
        assert!(!target.exists(), "the link was written through");
        let files = git_in(dir.path(), &["ls-files"]);
        assert_eq!(files.trim(), "tasks.json");
    }

    /// Review of R22: a symlinked database was committed as the link.
    #[test]
    #[cfg(unix)]
    fn a_symlinked_database_is_committed_where_it_is() {
        if !git_available() {
            return;
        }
        let real = alices_repository();
        let elsewhere = tempfile::tempdir().unwrap();
        let file = real.path().join("tasks.json");
        std::fs::write(&file, "[]").unwrap();
        let link = elsewhere.path().join("tasks.json");
        std::os::unix::fs::symlink(&file, &link).unwrap();
        try_commit(&link, 0, Change::Update).unwrap();
        assert_eq!(git_in(real.path(), &["ls-files", "-s"]).split_whitespace().next(), Some("100644"));
        assert!(!elsewhere.path().join(".git").exists());
    }

    /// Review of R22: a glob in the name of the database took in files
    /// next to it. Brackets, since Windows allows no `*` in a file name:
    /// `tasks[1].json` as a pattern is `tasks1.json`.
    #[test]
    fn the_name_is_no_pattern() {
        if !git_available() {
            return;
        }
        let dir = alices_repository();
        std::fs::write(dir.path().join("tasks1.json"), "secret").unwrap();
        let db = dir.path().join("tasks[1].json");
        std::fs::write(&db, "[]").unwrap();
        try_commit(&db, 0, Change::Update).unwrap();
        assert_eq!(git_in(dir.path(), &["ls-files"]).trim(), "tasks[1].json");
    }

    /// Review of R22 (C7): a database inside the git directory is no file
    /// of a work tree.
    #[test]
    fn a_database_inside_the_git_directory_is_not_committed() {
        if !git_available() {
            return;
        }
        let dir = alices_repository();
        let db = dir.path().join(".git").join("tasks.json");
        std::fs::write(&db, "[]").unwrap();
        let err = try_commit(&db, 0, Change::Update).unwrap_err();
        assert!(err.contains("inside the git directory"), "{err}");
    }
}
