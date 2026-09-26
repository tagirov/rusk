// Regression tests for the urgent fixes from REVIEW.md (the number in each
// test's doc comment is the report item). All of them must stay green.

use std::fs;

mod common;
use common::Sandbox;

const ONE_TASK_DB: &str =
    r#"[{"id":1,"text":"first task","date":null,"done":false,"priority":false}]"#;

fn stderr_of(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

fn stdout_of(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// REVIEW №1: `rusk restore` must work exactly when the database is
/// corrupted — it used to load the database before dispatching and died
/// with the corruption report it was supposed to fix.
#[test]
fn restore_works_on_a_corrupted_database() {
    let sb = Sandbox::with_db(ONE_TASK_DB);
    // A second save leaves the first state in `.backup`.
    let out = sb.cmd().args(["add", "second task"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    let backup = sb.db_path().with_extension("json.backup");
    assert!(backup.exists(), "a save must leave a .backup sibling");

    let garbage = "{ this is not json: \"unsaved but precious\"";
    sb.write_db(garbage);

    // Every ordinary command refuses the corrupted database...
    let out = sb.cmd().arg("list").output().unwrap();
    assert!(!out.status.success());

    // ...and restore repairs it.
    let out = sb.cmd().arg("restore").output().unwrap();
    assert!(
        out.status.success(),
        "restore must not require a loadable database: {}",
        stderr_of(&out)
    );
    assert!(sb.read_db().contains("first task"), "db: {}", sb.read_db());

    let out = sb.cmd().arg("list").output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    assert!(stdout_of(&out).contains("first task"));
}

/// REVIEW №1 (addendum): the corrupted database is overwritten by restore,
/// so its raw bytes must be kept — they may hold tasks the backup lacks.
#[test]
fn restore_keeps_a_raw_copy_of_the_corrupted_database() {
    let sb = Sandbox::with_db(ONE_TASK_DB);
    let out = sb.cmd().args(["add", "second task"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));

    let garbage = "{ this is not json: \"unsaved but precious\"";
    sb.write_db(garbage);
    let out = sb.cmd().arg("restore").output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));

    let kept = sb.db_path().with_extension("json.before_restore");
    assert_eq!(
        fs::read_to_string(&kept).unwrap_or_default(),
        garbage,
        "the corrupted database must survive byte for byte in .before_restore"
    );

    // Why the database could not be read, in one line: the full report ends
    // in advice to restore from a backup, which is what is going on.
    let stdout = stdout_of(&out);
    assert!(stdout.contains("cannot be read (Failed to parse the database file"), "{stdout}");
    assert!(!stdout.contains("To fix this issue"), "{stdout}");
}

/// REVIEW №161: `hidden` on #banner / #daterow was overridden by their
/// `display: flex` id rules, so the red error banner was always visible.
#[test]
#[cfg(feature = "web")]
fn web_template_keeps_the_hidden_attribute_authoritative() {
    let sb = Sandbox::with_db(ONE_TASK_DB);
    let out = sb.cmd().args(["gen", "-o", "-"]).output().unwrap();
    assert!(out.status.success(), "{}", stderr_of(&out));
    let html = stdout_of(&out);

    let compact: String = html.chars().filter(|c| !c.is_whitespace()).collect();
    assert!(
        compact.contains("[hidden]{display:none!important;}"),
        "the stylesheet must force display:none for [hidden] elements"
    );
    // The elements the rule protects are still declared hidden by default
    // (whatever other attributes their tags carry).
    for id in ["banner", "daterow", "edit-error"] {
        let at = html
            .find(&format!(r#" id="{id}""#))
            .unwrap_or_else(|| panic!("#{id} is missing from the page"));
        let tag_end = at + html[at..].find('>').unwrap();
        assert!(
            html[at..tag_end].split_whitespace().any(|attr| attr == "hidden"),
            "#{id} must be hidden by default: {}",
            &html[at..tag_end]
        );
    }
}

#[cfg(all(unix, feature = "sync"))]
mod sync_over_fake_ssh {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    struct Pair {
        sb: Sandbox,
        remote_file: PathBuf,
        remote: String,
    }

    impl Pair {
        /// Local database with two tasks, already pushed to the remote.
        fn synced() -> Self {
            let sb = Sandbox::new();
            let remote_file = sb.path().join("remote").join("tasks.json");
            let remote = format!("user@vps:{}", remote_file.display());
            let pair = Self {
                sb,
                remote_file,
                remote,
            };
            for text in ["keep me", "and me"] {
                let out = pair.rusk(&["add", text]);
                assert!(out.status.success(), "{}", stderr_of(&out));
            }
            let out = pair.rusk(&["sync"]);
            assert!(out.status.success(), "{}", stderr_of(&out));
            assert!(stdout_of(&out).contains("Pushed 2"), "{}", stdout_of(&out));
            pair
        }

        fn rusk(&self, args: &[&str]) -> std::process::Output {
            self.sb
                .cmd_with_fake_ssh()
                .env("RUSK_SYNC_REMOTE", &self.remote)
                .args(args)
                .output()
                .unwrap()
        }

        fn remote_content(&self) -> String {
            fs::read_to_string(&self.remote_file).unwrap_or_default()
        }
    }

    /// REVIEW №6: a missing local database file loaded as "0 tasks" and was
    /// pushed automatically, emptying the remote.
    #[test]
    fn missing_local_database_is_not_pushed_as_delete_everything() {
        let pair = Pair::synced();
        fs::remove_file(pair.sb.db_path()).unwrap();

        let out = pair.rusk(&["sync"]);
        assert!(!out.status.success(), "sync must refuse: {}", stdout_of(&out));
        assert!(stderr_of(&out).contains("--force"), "{}", stderr_of(&out));
        assert!(pair.remote_content().contains("keep me"));

        // Plain push is a fast-forward too and must refuse as well.
        let out = pair.rusk(&["sync", "push"]);
        assert!(!out.status.success());
        assert!(pair.remote_content().contains("keep me"));

        // The suggested way out restores the local side.
        let out = pair.rusk(&["sync", "pull", "--force"]);
        assert!(out.status.success(), "{}", stderr_of(&out));
        assert!(pair.sb.read_db().contains("keep me"));
    }

    /// REVIEW №6, the other direction: a missing remote file was pulled as
    /// "0 tasks" and emptied the local database.
    #[test]
    fn missing_remote_file_is_not_pulled_as_delete_everything() {
        let pair = Pair::synced();
        fs::remove_file(&pair.remote_file).unwrap();

        let out = pair.rusk(&["sync"]);
        assert!(!out.status.success(), "sync must refuse: {}", stdout_of(&out));
        assert!(stderr_of(&out).contains("--force"), "{}", stderr_of(&out));
        assert!(pair.sb.read_db().contains("keep me"));

        let out = pair.rusk(&["sync", "pull"]);
        assert!(!out.status.success());
        assert!(pair.sb.read_db().contains("keep me"));

        // The suggested way out re-creates the remote.
        let out = pair.rusk(&["sync", "push", "--force"]);
        assert!(out.status.success(), "{}", stderr_of(&out));
        assert!(pair.remote_content().contains("keep me"));
    }

    /// An explicit `--force` still allows really emptying the other side.
    #[test]
    fn force_can_still_empty_the_other_side() {
        let pair = Pair::synced();
        pair.sb.write_db("[]");
        let out = pair.rusk(&["sync", "push", "--force"]);
        assert!(out.status.success(), "{}", stderr_of(&out));
        assert!(!pair.remote_content().contains("keep me"));
    }

    /// Seeding an empty remote on first contact needs no `--force`.
    #[test]
    fn first_sync_to_an_empty_remote_still_pushes() {
        let pair = Pair::synced();
        assert!(pair.remote_content().contains("keep me"));
    }

    /// REVIEW №7: `cat` failures were masked by `|| true`, so an unreadable
    /// remote file read as an empty database and was pulled over local data.
    #[test]
    fn unreadable_remote_file_is_an_error_not_an_empty_database() {
        let pair = Pair::synced();
        fs::set_permissions(&pair.remote_file, fs::Permissions::from_mode(0o000)).unwrap();
        if fs::read(&pair.remote_file).is_ok() {
            eprintln!("skipping: running as a user that ignores file modes (root?)");
            return;
        }

        let out = pair.rusk(&["sync"]);
        fs::set_permissions(&pair.remote_file, fs::Permissions::from_mode(0o644)).unwrap();

        assert!(!out.status.success(), "sync must fail: {}", stdout_of(&out));
        let stderr = stderr_of(&out);
        assert!(stderr.contains("ssh failed"), "{stderr}");
        assert!(!stdout_of(&out).contains("Pulled"));
        assert!(pair.sb.read_db().contains("keep me"));
        assert!(pair.remote_content().contains("keep me"));
    }

    /// REVIEW №7: a directory in place of the remote database read as an
    /// empty database.
    #[test]
    fn directory_in_place_of_the_remote_file_is_an_error() {
        let pair = Pair::synced();
        fs::remove_file(&pair.remote_file).unwrap();
        fs::create_dir(&pair.remote_file).unwrap();

        let out = pair.rusk(&["sync"]);
        assert!(!out.status.success(), "sync must fail: {}", stdout_of(&out));
        assert!(stderr_of(&out).contains("ssh failed"), "{}", stderr_of(&out));
        assert!(pair.sb.read_db().contains("keep me"));
    }
}

/// REVIEW №154: SQLite `load()` ran outside the `save()` transaction, so two
/// writers that both loaded before either saved silently lost one update.
#[cfg(feature = "backend-sqlite")]
mod sqlite_concurrent_writers {
    use rusk::{Backend, Task};

    fn task(id: u32, text: &str) -> Task {
        Task {
            id,
            text: text.to_string(),
            date: None,
            done: false,
            priority: false,
            after: Vec::new(),
        }
    }

    #[test]
    fn stale_save_never_silently_drops_another_writers_task() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.db");
        Backend::from_local_path(path.clone())
            .unwrap()
            .save(&[task(1, "seed")])
            .unwrap();

        let a = Backend::from_local_path(path.clone()).unwrap();
        let b = Backend::from_local_path(path.clone()).unwrap();
        let mut tasks_a = a.load().unwrap();
        let mut tasks_b = b.load().unwrap();
        tasks_a.push(task(2, "written by a"));
        tasks_b.push(task(2, "written by b"));

        a.save(&tasks_a).unwrap();
        let second = b.save(&tasks_b);

        let on_disk = Backend::from_local_path(path).unwrap().load().unwrap();
        let has = |text: &str| on_disk.iter().any(|t| t.text == text);
        assert!(has("written by a"), "a's committed task was lost: {on_disk:?}");
        assert!(
            second.is_err() || has("written by b"),
            "b reported success but its task is not in the database"
        );
    }
}
