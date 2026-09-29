//! Crash-safe draft autosave for the interactive editor.
//!
//! A draft is what the editor had in its buffer when it stopped without
//! storing anything: Ctrl+C, a SIGTERM, a save the database refused, a
//! crash. It exists so that the next `rusk edit 1` can offer the text back,
//! and everything here follows from that one job:
//!
//! * one file per task, named after it, so editing task 2 cannot overwrite
//!   the draft of task 1;
//! * the file lives where only its owner can read it — next to the
//!   database, or, for a remote database, in a private directory of this
//!   user's, never in a `/tmp/rusk` everyone shares;
//! * it is pinned to the text it was started from, so a draft left over
//!   from a task that has since been deleted is not offered for the *new*
//!   task that reused its id;
//! * it outlives the editor and is removed by whoever stored the text —
//!   the draft is the copy that survives a save that fails;
//! * a draft that cannot be read is kept as `.corrupt`, not deleted: the
//!   text inside it may still be readable by a human.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const AUTOSAVE_INTERVAL_MS: u64 = 3000;

/// Optional inputs the caller can plumb into the editor:
/// the colored first-line prompt, the autosave slot and the date base.
#[derive(Clone, Default)]
pub struct EditorExtras {
    /// Colored rendering of the first-line prompt (plain-width must match `prompt`).
    pub first_line_colored: Option<String>,
    /// Where autosave drafts go; `None` disables autosave.
    pub draft: Option<Slot>,
    /// Task due date before edit: first-line tokens starting with `+` resolve relative to this.
    pub relative_date_base: Option<chrono::NaiveDate>,
}

/// The one draft file an editor session owns.
#[derive(Clone)]
pub struct Slot {
    pub path: PathBuf,
    /// `new-task`, or `task-<id>`.
    pub key: String,
    /// Identity of the text the draft was started from (see [`base_of`]).
    pub base: String,
}

/// What a draft is pinned to: the stored task as the editor lays it out.
/// For `rusk add` there is no task yet, so every new-task draft shares the
/// empty base — a `-d` seed is not part of the identity, or the draft typed
/// before it could never be offered again.
pub fn base_of(text: &str) -> String {
    crate::revision::text_revision(text)
}

/// Periodic autosave tick. Call once per event-loop iteration; the function
/// rate-limits itself internally so it's safe to call on every tick.
pub(super) fn tick(extras: &EditorExtras, lines: &[String], prefill: &str, state: &mut Autosave) {
    if state.last_attempt.elapsed() < Duration::from_millis(AUTOSAVE_INTERVAL_MS) {
        return;
    }
    state.last_attempt = Instant::now();
    save(extras, lines, prefill, state);
}

/// Writes the buffer out now, whatever the interval says: the editor is
/// about to stop and this is the last chance to keep what is in it.
pub(super) fn flush(extras: &EditorExtras, lines: &[String], prefill: &str, state: &mut Autosave) {
    save(extras, lines, prefill, state);
}

/// Autosave bookkeeping for one editor session.
pub(super) struct Autosave {
    last_attempt: Instant,
    /// What the draft file holds, as far as this session knows. `None`
    /// until it has written or removed anything — an empty string is a
    /// buffer the user cleared, which is an edit like any other and must
    /// not be mistaken for "nothing has happened yet".
    last_written: Option<String>,
    /// What went wrong the first time a draft could not be written. Kept
    /// rather than printed: the terminal belongs to the editor, so the
    /// warning waits until it has been given back.
    pub(super) error: Option<String>,
}

impl Default for Autosave {
    fn default() -> Self {
        Self {
            last_attempt: Instant::now(),
            last_written: None,
            error: None,
        }
    }
}

fn save(extras: &EditorExtras, lines: &[String], prefill: &str, state: &mut Autosave) {
    let Some(slot) = &extras.draft else {
        return;
    };
    let joined = lines.join("\n");
    if state.last_written.as_deref() == Some(joined.as_str()) {
        return;
    }
    // Back to where it started: there is nothing unsaved to keep, and a
    // draft still lying around would be offered back as if there were.
    if joined == prefill {
        let _ = std::fs::remove_file(&slot.path);
        state.last_written = Some(joined);
        return;
    }
    match write(slot, &joined) {
        Ok(()) => state.last_written = Some(joined),
        Err(e) => {
            state.error.get_or_insert_with(|| format!("{e:#}"));
        }
    }
}

/// Remove the draft. Called once the text is somewhere safer — stored in
/// the database, or deliberately discarded.
pub fn remove(slot: &Slot) {
    let _ = std::fs::remove_file(&slot.path);
}

/// Where the drafts of a database live.
///
/// A local database keeps them beside itself (`local` is its directory).
/// A remote one has no directory on this machine, and the temp directory
/// is shared with every other user of it, so those go to a private
/// directory of this user's instead: `$XDG_RUNTIME_DIR/rusk` where there
/// is one (as `transport` takes it: an absolute path to a directory that
/// exists, and `rusk` can be made in it — review of R31), otherwise
/// `<temp>/rusk-<uid>` (see `crate::scratch`) — never a `/tmp/rusk`
/// everyone shares.
pub fn dir_for(local: Option<&Path>) -> Result<PathBuf> {
    if let Some(dir) = local {
        return Ok(dir.to_path_buf());
    }
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute() && dir.is_dir());
    if let Some(runtime) = runtime {
        let dir = runtime.join("rusk");
        if crate::scratch::create_private_dir(&dir).is_ok() {
            return Ok(dir);
        }
    }
    crate::scratch::private_dir().context("no place for the drafts")
}

/// One file per task: `editor-task-3.draft`. The key is rusk's own
/// (`new-task`, `task-<id>`), but it is spelled out defensively anyway — a
/// draft file name is never allowed to reach outside its directory.
pub fn path_for(dir: &Path, key: &str) -> PathBuf {
    let safe: String = key
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    dir.join(format!("editor-{safe}.draft"))
}

pub fn write(slot: &Slot, text: &str) -> Result<()> {
    if let Some(parent) = slot.path.parent() {
        // What fails here fails the write below, which says so.
        let _ = crate::scratch::create_private_dir(parent);
    }
    let payload = serde_json::json!({
        "key": slot.key,
        "base": slot.base,
        "text": text,
        "timestamp": chrono::Local::now().to_rfc3339(),
    });
    // Atomic and owner-only: an autosave interrupted half-way must not
    // destroy the previous draft, and the text is as private as it gets.
    crate::atomic::replace_aux_file(
        &slot.path,
        &serde_json::to_vec_pretty(&payload)?,
        crate::atomic::Mode::Private,
    )
    .with_context(|| format!("Failed to write the draft at {}", slot.path.display()))?;
    Ok(())
}

/// A draft worth offering back.
pub struct Saved {
    pub text: String,
    pub saved_at: Option<chrono::DateTime<chrono::Local>>,
}

impl Saved {
    /// "3 minutes ago", for the restore prompt. `None` when the draft
    /// carries no readable timestamp.
    pub fn age(&self) -> Option<String> {
        let saved_at = self.saved_at?;
        let seconds = chrono::Local::now()
            .signed_duration_since(saved_at)
            .num_seconds()
            .max(0);
        Some(match seconds {
            0..=90 => "just now".to_string(),
            s if s < 90 * 60 => format!("{} minutes ago", s / 60),
            s if s < 36 * 3600 => format!("{} hours ago", s / 3600),
            s => format!("{} days ago", s / 86400),
        })
    }

    /// The first line, shortened, for the restore prompt.
    pub fn preview(&self) -> String {
        let text = crate::printable::escape(&self.text);
        let line = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
        let line = line.trim();
        let short: String = line.chars().take(40).collect();
        if short.chars().count() < line.chars().count() {
            format!("{short}…")
        } else {
            short
        }
    }
}

/// Reads the draft in `slot`, if there is one that belongs to it.
///
/// A draft that cannot be parsed is not thrown away: the text inside may
/// still be readable, so it is kept under a `.corrupt` name and said so.
pub fn read(slot: &Slot) -> Option<Saved> {
    let raw = std::fs::read_to_string(&slot.path).ok()?;
    let Some(value) = serde_json::from_str::<serde_json::Value>(&raw).ok() else {
        keep_corrupt(&slot.path);
        return None;
    };
    let field = |name: &str| value.get(name).and_then(|v| v.as_str());
    let (Some(key), Some(text)) = (field("key"), field("text")) else {
        keep_corrupt(&slot.path);
        return None;
    };
    // Not this task's, or this task's text is no longer what the draft was
    // typed against — the id was reused, or the task was edited elsewhere.
    if key != slot.key || field("base").unwrap_or_default() != slot.base {
        return None;
    }
    Some(Saved {
        text: text.to_string(),
        saved_at: field("timestamp")
            .and_then(|t| chrono::DateTime::parse_from_rfc3339(t).ok())
            .map(|t| t.with_timezone(&chrono::Local)),
    })
}

fn keep_corrupt(path: &Path) {
    let kept = path.with_extension("draft.corrupt");
    let message = match std::fs::rename(path, &kept) {
        Ok(()) => format!(
            "Warning: the draft at {} could not be read; it was kept as {}",
            path.display(),
            kept.display()
        ),
        Err(e) => format!(
            "Warning: the draft at {} could not be read, nor moved aside: {e}",
            path.display()
        ),
    };
    crate::backend::warn_once(&message);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slot(dir: &Path, key: &str, base: &str) -> Slot {
        Slot {
            path: path_for(dir, key),
            key: key.to_string(),
            base: base_of(base),
        }
    }

    #[test]
    fn roundtrip_by_key() {
        let dir = tempfile::tempdir().unwrap();
        let three = slot(dir.path(), "task-3", "hello");
        write(&three, "first line\nsecond line").unwrap();

        assert_eq!(
            read(&three).map(|d| d.text).as_deref(),
            Some("first line\nsecond line")
        );
        // REVIEW №48: another task is another file, so neither can take
        // the other's place.
        let four = slot(dir.path(), "task-4", "hello");
        assert!(read(&four).is_none());
        write(&four, "task four").unwrap();
        assert_eq!(read(&three).map(|d| d.text).as_deref(), Some("first line\nsecond line"));
        assert_ne!(three.path, four.path);
    }

    /// REVIEW №186: a draft belongs to the text it was typed against. The
    /// id alone is not enough — it can be freed and handed to a new task.
    #[test]
    fn a_draft_is_not_offered_for_a_task_that_replaced_the_old_one() {
        let dir = tempfile::tempdir().unwrap();
        let old = slot(dir.path(), "task-1", "old task about the dentist");
        write(&old, "DRAFT old task about the dentist").unwrap();

        let reused = slot(dir.path(), "task-1", "brand new unrelated task");
        assert!(read(&reused).is_none());
        // ...and the original edit still gets it back.
        assert!(read(&old).is_some());
    }

    /// REVIEW №188: a draft that does not parse keeps its text under a
    /// `.corrupt` name instead of being deleted.
    #[test]
    fn an_unreadable_draft_is_kept_aside() {
        let dir = tempfile::tempdir().unwrap();
        let slot = slot(dir.path(), "new-task", "");
        std::fs::write(&slot.path, "{\n  \"key\": \"new-task\",\n  \"text\": \"important").unwrap();

        assert!(read(&slot).is_none());
        assert!(!slot.path.exists());
        let kept = std::fs::read_to_string(dir.path().join("editor-new-task.draft.corrupt")).unwrap();
        assert!(kept.contains("important"), "{kept}");
    }

    /// REVIEW №160: a buffer the user cleared is an edit, not "nothing
    /// written yet" — `Ctrl+C` before the first autosave must still keep it.
    #[test]
    fn an_emptied_buffer_is_kept_like_any_other_edit() {
        let dir = tempfile::tempdir().unwrap();
        let extras = EditorExtras {
            draft: Some(slot(dir.path(), "task-1", "alpha beta")),
            ..Default::default()
        };
        let mut state = Autosave::default();

        flush(&extras, &[String::new()], "alpha beta", &mut state);

        let saved = read(extras.draft.as_ref().unwrap()).expect("no draft was written");
        assert_eq!(saved.text, "");
    }

    /// REVIEW №119: a buffer brought back to the task's own text has
    /// nothing unsaved in it, so the draft goes.
    #[test]
    fn a_buffer_back_at_its_prefill_removes_the_draft() {
        let dir = tempfile::tempdir().unwrap();
        let extras = EditorExtras {
            draft: Some(slot(dir.path(), "task-1", "hello world")),
            ..Default::default()
        };
        let mut state = Autosave::default();

        flush(&extras, &["abchello world".to_string()], "hello world", &mut state);
        assert!(extras.draft.as_ref().unwrap().path.exists());

        flush(&extras, &["hello world".to_string()], "hello world", &mut state);
        assert!(!extras.draft.as_ref().unwrap().path.exists());
        assert!(state.error.is_none());
    }

    /// REVIEW №120: a draft that cannot be written is remembered, so the
    /// editor can say so once it has the terminal back.
    #[test]
    #[cfg(unix)]
    fn a_draft_that_cannot_be_written_is_remembered() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let locked = dir.path().join("locked");
        std::fs::create_dir(&locked).unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o500)).unwrap();
        let extras = EditorExtras {
            draft: Some(slot(&locked, "task-1", "hello")),
            ..Default::default()
        };
        let mut state = Autosave::default();

        flush(&extras, &["typed".to_string()], "hello", &mut state);
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();

        if std::fs::File::create(locked.join("probe")).is_ok() {
            eprintln!("skipping: running as a user that ignores file modes (root?)");
            return;
        }
        let error = state.error.expect("the failed write was not remembered");
        assert!(error.contains("draft"), "{error}");
    }

    /// REVIEW №188: an autosave replaces the previous draft atomically (a
    /// write that dies half-way cannot destroy it), leaves no temp file
    /// behind, and the draft is readable by its owner only.
    #[test]
    #[cfg(unix)]
    fn autosave_replaces_the_previous_draft_atomically_and_privately() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let slot = slot(dir.path(), "new-task", "");
        write(&slot, "previous draft").unwrap();
        let witness = dir.path().join("witness");
        std::fs::hard_link(&slot.path, &witness).unwrap();

        write(&slot, "current draft").unwrap();

        assert_eq!(read(&slot).map(|d| d.text).as_deref(), Some("current draft"));
        assert!(
            std::fs::read_to_string(&witness).unwrap().contains("previous draft"),
            "the previous draft was truncated and rewritten in place"
        );
        let mode = std::fs::metadata(&slot.path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "draft mode is {mode:o}");

        let mut names: Vec<_> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        assert_eq!(names, ["editor-new-task.draft", "witness"]);
    }

    /// REVIEW №48: the drafts of a remote database go somewhere only this
    /// user can look into, never a `/tmp/rusk` shared with everyone.
    #[test]
    #[cfg(unix)]
    fn remote_drafts_live_in_a_private_directory() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempfile::tempdir().unwrap();
        let runtime = temp.path().join("run");
        std::fs::create_dir(&runtime).unwrap();
        // SAFETY: single-threaded test, and the variable is read back at once.
        unsafe { std::env::set_var("XDG_RUNTIME_DIR", &runtime) };
        let dir = dir_for(None).unwrap();
        // A runtime directory that is not there (a stale variable) is none
        // (review of R31): the fallback below serves instead.
        unsafe { std::env::set_var("XDG_RUNTIME_DIR", temp.path().join("gone")) };
        let without = dir_for(None).unwrap();
        unsafe { std::env::remove_var("XDG_RUNTIME_DIR") };

        assert_eq!(dir, runtime.join("rusk"));
        let mode = std::fs::metadata(&dir).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "draft directory mode is {mode:o}");
        assert!(!without.starts_with(temp.path()), "{}", without.display());

        // Without a runtime directory the fallback is this user's own.
        let fallback = dir_for(None).unwrap();
        assert_eq!(fallback, without);
        assert!(
            fallback.file_name().unwrap().to_string_lossy().starts_with("rusk-"),
            "{}",
            fallback.display()
        );
        // A local database keeps its drafts beside itself.
        assert_eq!(dir_for(Some(Path::new("/db"))).unwrap(), Path::new("/db"));
    }

    #[test]
    fn a_key_never_names_a_file_outside_its_directory() {
        let path = path_for(Path::new("/drafts"), "../../etc/passwd");
        assert_eq!(path.parent().unwrap(), Path::new("/drafts"));
    }

    #[test]
    fn the_prompt_shows_how_old_the_draft_is_and_what_is_in_it() {
        let fresh = Saved {
            text: "  a fairly long first line that will not fit into the prompt\nmore".to_string(),
            saved_at: Some(chrono::Local::now() - chrono::Duration::minutes(5)),
        };
        assert_eq!(fresh.age().as_deref(), Some("5 minutes ago"));
        assert_eq!(fresh.preview(), "a fairly long first line that will not f…");

        let undated = Saved { text: "short".to_string(), saved_at: None };
        assert_eq!(undated.age(), None);
        assert_eq!(undated.preview(), "short");
    }
}
