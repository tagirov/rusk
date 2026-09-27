#[cfg(feature = "interactive")]
use crate::parse_cli_date_for_edit;
use crate::config::theme;
use crate::parser::date::is_cli_date_clear_value;
use crate::search::Query;
use crate::{Task, TaskId, TaskManager, validate_cli_date_edit_arg};
use crate::{out, outln};
use anyhow::Result;
use colored::*;

use super::HandlerCLI;
#[cfg(feature = "interactive")]
use super::editor::EditorExtras;

/// One interactive editor session, as the handlers ask for it.
#[cfg(feature = "interactive")]
struct EditorSession<'a> {
    /// `new-task`, or `task-<id>`: which draft file this session owns.
    draft_key: &'a str,
    /// How the restore prompt names it ("new task", "task 3").
    what: &'a str,
    /// What the task holds now — the buffer's baseline, and what the draft
    /// is pinned to.
    base: &'a str,
    /// What the buffer starts from when there is no draft to restore. For
    /// `rusk add -d` this differs from `base`: the seeded due date is not
    /// part of the task's identity.
    prefill: String,
    /// The due date `-d` put in front, if there was one.
    seed_date: Option<chrono::NaiveDate>,
    relative_date_base: Option<chrono::NaiveDate>,
    cursor_at_start: bool,
    allow_skip: bool,
}

impl HandlerCLI {
    /// `(19,22)` suffix for a task with dependencies; appended after the
    /// task text everywhere the task is shown. Ids are bold, parens are not.
    pub(crate) fn after_suffix(task: &Task) -> Option<String> {
        if task.after.is_empty() {
            return None;
        }
        let ids = task
            .after
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(",");
        Some(format!("({})", ids.bold()))
    }

    /// `<what>: <id>:` — the line every report on a task starts with, the
    /// task's text following on the lines below.
    fn task_heading(what: ColoredString, id: TaskId) -> String {
        format!("{what} {id}:")
    }

    fn print_added_task(tm: &TaskManager, id: TaskId) -> Result<()> {
        let Some(task) = tm.find_task_by_id(id).map(|idx| &tm.tasks()[idx]) else {
            return Ok(());
        };
        let mut heading = Self::task_heading(theme().success.paint("Added task:"), task.id);
        if let Some(date) = task.date {
            heading = format!("{heading} ({})", Self::colored_short_date(date, task.done));
        }
        let suffix = Self::after_suffix(task).map(|s| format!(" {s}"));
        Self::print_task_text_with_wrapping_suffixed(&heading, &task.text, suffix.as_deref())
    }

    /// The draft slot of one task, in the directory the drafts of this
    /// database belong in.
    #[cfg(feature = "interactive")]
    fn draft_slot(key: &str, base: &str) -> super::editor::draft::Slot {
        use super::editor::draft;
        let dir = draft::dir_for(&TaskManager::get_db_dir(), TaskManager::db_is_local());
        draft::Slot {
            path: draft::path_for(&dir, key),
            key: key.to_string(),
            base: draft::base_of(base),
        }
    }

    /// Offers back the draft left by an editor session that ended without
    /// storing anything. It says how old it is and what is in it, so that
    /// answering is not a guess.
    #[cfg(feature = "interactive")]
    fn offer_draft(slot: &super::editor::draft::Slot, what: &str) -> Result<Option<String>> {
        let Some(saved) = super::editor::draft::read(slot) else {
            return Ok(None);
        };
        let age = saved.age().map_or(String::new(), |age| format!("{age}, "));
        let prompt = format!(
            "{} {} {}{} {}",
            theme().accent.paint("Restore unsaved draft for"),
            theme().emphasis.paint(what),
            theme().notice.paint(&format!("({age}")),
            theme().notice.paint(&format!("“{}”)?", saved.preview())),
            Self::yn_hint()
        );
        if Self::read_confirmation(&prompt)? {
            return Ok(Some(saved.text));
        }
        super::editor::draft::remove(slot);
        Ok(None)
    }

    /// A restored draft with the due date of `rusk add -d` put in front. A
    /// draft holds text, not a due date, so the date on the command line
    /// applies to the restored text too — unless the draft already begins
    /// with a date of its own, which the user typed and which wins. Saying
    /// so beats letting the argument disappear. A draft that begins with
    /// `_`, the empty date, gets the date in its place, laid out as the
    /// editor would open it: the words after it stay text.
    #[cfg(feature = "interactive")]
    fn with_seed_date(text: String, seed: Option<chrono::NaiveDate>) -> String {
        let Some(seed) = seed else {
            return text;
        };
        match Self::extract_leading_date(&text, None) {
            (Some(own), _) => {
                crate::backend::warn_once(&format!(
                    "Warning: the restored draft already starts with a date \
                     ({}), so it is used instead of the one on the command line",
                    own.format("%d-%m-%Y")
                ));
                text
            }
            (None, body) => Self::edit_prefill(&body, Some(seed)),
        }
    }

    /// Runs the multi-line editor with draft persistence wired up.
    #[cfg(feature = "interactive")]
    fn run_editor_with_draft(session: EditorSession<'_>) -> Result<String> {
        let EditorSession {
            draft_key,
            what,
            base,
            prefill,
            seed_date,
            relative_date_base,
            cursor_at_start,
            allow_skip,
        } = session;
        let slot = Self::draft_slot(draft_key, base);
        let restored = Self::offer_draft(&slot, what)?
            .map(|text| Self::with_seed_date(text, seed_date));

        let extras = EditorExtras {
            draft: Some(slot),
            relative_date_base,
            ..Default::default()
        };

        Self::run_multi_line_editor(
            "    ",
            &prefill,
            restored.as_deref(),
            cursor_at_start,
            None,
            allow_skip,
            extras,
        )
    }

    /// The editor keeps its draft until the text is stored, so a save that
    /// fails leaves it exactly where the next edit will look for it — unless
    /// the draft could not be written either (a read-only directory refuses
    /// both; the editor has warned about it), in which case what is there is
    /// an older draft or none. Only a draft that holds `buffer`, the text the
    /// editor saved, is promised; otherwise the report carries the text
    /// itself, the one copy left.
    #[cfg(feature = "interactive")]
    fn draft_survives_failed_save(
        error: anyhow::Error,
        slot: &super::editor::draft::Slot,
        buffer: &str,
        retry_command: &str,
    ) -> anyhow::Error {
        let kept = super::editor::draft::read(slot).is_some_and(|saved| saved.text == buffer);
        if kept {
            anyhow::anyhow!(
                "{error:#}. Your text was kept as a draft: run `{retry_command}` to restore it"
            )
        } else {
            anyhow::anyhow!(
                "{error:#}. Your text could not be kept as a draft either; here it is:\n{buffer}"
            )
        }
    }

    pub fn handle_add_task(
        tm: &mut TaskManager,
        text: Vec<String>,
        date: Option<String>,
        after: Vec<crate::TaskId>,
    ) -> Result<()> {
        let id = tm.add_task_with_after(text, date, after)?;
        Self::print_added_task(tm, id)
    }

    /// Interactive TUI: no inline task text; optional `-d` pre-seeds the first line with that due date.
    #[cfg(feature = "interactive")]
    pub fn handle_add_task_interactive(
        tm: &mut TaskManager,
        date: Option<String>,
        after: Vec<crate::TaskId>,
    ) -> Result<()> {
        // Reject a bad dependency list before the editor opens, not after
        // the text has been typed.
        let after = tm.validate_after(None, &after)?;
        let seed = match date.as_deref() {
            // `_` is "no date", which is what a new task has anyway.
            Some(d) if !is_cli_date_clear_value(d) => Some(parse_cli_date_for_edit(d, None)?),
            _ => None,
        };
        // There is no task yet, so every new-task draft is pinned to the
        // same empty base: a `-d` seed must not make the draft typed
        // without one unofferable.
        let edited = Self::run_editor_with_draft(EditorSession {
            draft_key: "new-task",
            what: "new task",
            base: "",
            prefill: seed.map_or(String::new(), |s| format!("{} ", s.format("%d-%m-%Y"))),
            seed_date: seed,
            relative_date_base: None,
            cursor_at_start: false,
            allow_skip: false,
        })?;

        if edited.trim().is_empty() {
            return Ok(());
        }

        let (parsed_date, stripped) = Self::extract_leading_date(&edited, None);
        if stripped.trim().is_empty() {
            anyhow::bail!("Task text cannot be empty");
        }

        let slot = Self::draft_slot("new-task", "");
        let id = tm
            .add_task_full(stripped, parsed_date, after)
            .map_err(|e| Self::draft_survives_failed_save(e, &slot, &edited, "rusk add"))?;
        // Stored: the draft has done its job.
        super::editor::draft::remove(&slot);
        Self::print_added_task(tm, id)
    }

    /// `--done` deletes every completed task; otherwise `ids` says which
    /// ones. It is one or the other. Each deletion is confirmed on the
    /// terminal unless `yes` says it already is.
    pub fn handle_delete_tasks(
        tm: &mut TaskManager,
        ids: Vec<TaskId>,
        done: bool,
        yes: bool,
    ) -> Result<()> {
        match (done, ids.is_empty()) {
            (true, true) => Self::delete_all_done(tm, yes),
            (false, false) => Self::delete_by_ids(tm, ids, yes),
            (true, false) => anyhow::bail!("`--done` deletes all completed tasks and takes no task ids"),
            (false, true) => anyhow::bail!("no task ids given; e.g. `rusk del 1,2,3`, or `rusk del --done`"),
        }
    }

    /// What the editor opens on for a task: its date as an editable prefix
    /// on the first line, then its text. Also what its draft is pinned to.
    ///
    /// The first word of the buffer is where the editor reads a date, and a
    /// `_` word right after that date (or after `_`, the empty date) marks
    /// the word after it as text (see [`extract_leading_date`]). So a text
    /// that starts with a word that reads as a date ("tomorrow call mom",
    /// "2d fix") opens as `_ tomorrow call mom` without a date and as
    /// `01-01-2027 _ tomorrow call mom` with one, and a text that starts
    /// with `_` gets the mark too. Saved as it is — or with the date typed
    /// in front, deleted or replaced by `_` — the text keeps that word
    /// instead of losing it to a due date (REVIEW №33).
    ///
    /// [`extract_leading_date`]: Self::extract_leading_date
    #[cfg(feature = "interactive")]
    fn edit_prefill(current: &str, task_date: Option<chrono::NaiveDate>) -> String {
        // Judged on the text as the editor will hold it (control characters
        // dropped): that is what it reads the date from.
        let shown = super::editor::text_ops::split_multi_line_prefill(current);
        let first = shown.first().map_or("", String::as_str);
        match (task_date, Self::leading_date_token(first, task_date)) {
            (Some(date), Some(_)) => format!("{} _ {current}", date.format("%d-%m-%Y")),
            (Some(date), None) => format!("{} {current}", date.format("%d-%m-%Y")),
            // The empty date, then the mark: a `_` right after `_` is one.
            (None, Some(None)) => format!("_ _ {current}"),
            (None, Some(Some(_))) => format!("_ {current}"),
            (None, None) => current.to_string(),
        }
    }

    /// What the editor made of a task: the date and text to store, and the
    /// buffer they were read from (what its draft holds).
    #[cfg(feature = "interactive")]
    fn interactive_edit_text(
        current: &str,
        task_id: TaskId,
        task_date: Option<chrono::NaiveDate>,
        allow_skip: bool,
    ) -> Result<Option<(Option<chrono::NaiveDate>, String, String)>> {
        let base_prefill = Self::edit_prefill(current, task_date);
        let edited = Self::run_editor_with_draft(EditorSession {
            draft_key: &format!("task-{task_id}"),
            what: &format!("task {task_id}"),
            base: &base_prefill,
            prefill: base_prefill.clone(),
            seed_date: None,
            relative_date_base: task_date,
            cursor_at_start: true,
            allow_skip,
        })?;
        if edited.trim().is_empty() {
            return Ok(None);
        }
        let (parsed_date, stripped) = Self::extract_leading_date(&edited, task_date);
        // If user removed all body text but kept the date, fall back to the
        // original text so the task never becomes empty on a date-only edit.
        // A text is stored without whitespace at its edges (see
        // `TaskManager::add_task_full`).
        let new_text = match stripped.trim() {
            "" => current.to_string(),
            text => text.to_string(),
        };
        // The buffer cannot hold a tab or a CR (both are normalized on the
        // way in), so a text that differs from the stored one by nothing but
        // that normalization — or by whitespace at its edges, which a text
        // stored before it was trimmed may have — is not an edit: the editor
        // showed it as unchanged and reported no changes to discard.
        let normalized_current =
            super::editor::text_ops::split_multi_line_prefill(current).join("\n");
        let new_text = if new_text != current && new_text == normalized_current.trim() {
            current.to_string()
        } else {
            new_text
        };
        Ok(Some((parsed_date, new_text, edited)))
    }

    /// The first word of the first line of `text` when the editor reads it
    /// as a date: `Some(Some(date))`, or `Some(None)` for `_`, the empty date.
    #[cfg(feature = "interactive")]
    fn leading_date_token(
        text: &str,
        task_date: Option<chrono::NaiveDate>,
    ) -> Option<Option<chrono::NaiveDate>> {
        let first = text.split('\n').next().unwrap_or("");
        let token = first.split(char::is_whitespace).next().unwrap_or("");
        if token.is_empty() {
            None
        } else if is_cli_date_clear_value(token) {
            Some(None)
        } else {
            crate::parse_cli_date_for_edit(token, task_date).ok().map(Some)
        }
    }

    /// The date at the head of an editor buffer and the text after it. The
    /// first word is the date when it reads as one (`_` is the empty date).
    /// A `_` word right after it marks the word after that as text and goes
    /// too (see [`edit_prefill`](Self::edit_prefill)).
    #[cfg(feature = "interactive")]
    fn extract_leading_date(
        edited: &str,
        task_date: Option<chrono::NaiveDate>,
    ) -> (Option<chrono::NaiveDate>, String) {
        let Some(date) = Self::leading_date_token(edited, task_date) else {
            return (None, edited.to_string());
        };
        let (first, rest) = match edited.split_once('\n') {
            Some((first, rest)) => (first, Some(rest)),
            None => (edited, None),
        };
        // `line` without its first word and one whitespace after it.
        let skip_word = |line: &str| -> (usize, usize) {
            let word = line.split(char::is_whitespace).next().unwrap_or("").len();
            let space = line[word..].chars().next().filter(|c| c.is_whitespace()).map_or(0, char::len_utf8);
            (word, word + space)
        };
        let mut first_rest = &first[skip_word(first).1..];
        let (word, next) = skip_word(first_rest);
        if &first_rest[..word] == "_" {
            first_rest = &first_rest[next..];
        }
        let text = match rest {
            // The date had the first line to itself: the text starts on
            // the next one, not with an empty line (REVIEW №32).
            Some(rest) if first_rest.is_empty() => rest.to_string(),
            Some(rest) => format!("{first_rest}\n{rest}"),
            None => first_rest.to_string(),
        };
        (date, text)
    }

    #[cfg(feature = "interactive")]
    pub fn handle_edit_tasks_interactive(tm: &mut TaskManager, ids: Vec<TaskId>) -> Result<()> {
        let mut any_changed = false;
        let mut not_found: Vec<TaskId> = Vec::new();

        let total_ids = ids.len();
        for (task_idx, id) in ids.iter().enumerate() {
            let is_last = task_idx == total_ids - 1;
            let allow_skip = !is_last;

            let Some(idx) = tm.find_task_by_id(*id) else {
                not_found.push(*id);
                continue;
            };
            let current_text = tm.tasks()[idx].text.clone();
            let current_date = tm.tasks()[idx].date;

            match Self::interactive_edit_text(&current_text, *id, current_date, allow_skip) {
                Ok(Some((new_date, new_text, buffer)))
                    if new_text != current_text || new_date != current_date =>
                {
                    // Saved right away, against the database as it is now:
                    // the editor may have been open for minutes, and a
                    // later Esc or Ctrl+C in this batch must not take a
                    // confirmed edit with it. Success is reported only
                    // once the edit is stored.
                    let slot = Self::draft_slot(
                        &format!("task-{id}"),
                        &Self::edit_prefill(&current_text, current_date),
                    );
                    tm.edit_task_as_seen(
                        *id,
                        (&current_text, current_date),
                        (&new_text, new_date),
                    )
                    .map_err(|e| {
                        Self::draft_survives_failed_save(e, &slot, &buffer, &format!("rusk edit {id}"))
                    })?;
                    // Stored: the draft has done its job.
                    super::editor::draft::remove(&slot);
                    any_changed = true;
                    outln!("{} {}", theme().success.paint("Edited task:"), id)?;
                }
                Ok(_) => {
                    outln!("{} {}", theme().notice.paint("Task unchanged:"), id)?;
                }
                Err(e) => {
                    if Self::handle_skip_task_error(&e, *id)? {
                        continue;
                    }
                    return Err(e);
                }
            }
        }

        Self::print_not_found_ids(&not_found)?;
        if any_changed {
            // Blank separator comes from the list header's leading newline.
            Self::handle_list_tasks(tm.tasks(), crate::config::config().compact)?;
        }
        Ok(())
    }

    fn print_deleted(count: usize, suffix: &str) -> Result<()> {
        outln!(
            "{}{}{}",
            theme().accent.paint("Deleted "),
            theme().emphasis.paint(&count.to_string()),
            theme().accent.paint(suffix)
        )
    }

    /// `rusk del` asks before it deletes, on the terminal; without one there
    /// is nobody to answer (REVIEW №69). That is an error before anything
    /// is asked, and it names the way to delete without asking.
    #[cfg(feature = "interactive")]
    fn check_can_ask() -> Result<()> {
        if Self::on_a_terminal() {
            return Ok(());
        }
        anyhow::bail!(
            "`rusk del` asks before it deletes, and there is no terminal to ask on; \
             pass --yes to delete without asking"
        )
    }

    /// The tasks of `tasks` the user agrees to delete, asked one by one.
    #[cfg(feature = "interactive")]
    fn confirm_each(tasks: Vec<Task>) -> Result<Vec<Task>> {
        Self::check_can_ask()?;
        let mut confirmed = Vec::new();
        for task in tasks {
            let prompt = Self::print_delete_confirmation_dialog(&task.text, task.id)?;
            if Self::read_confirmation(&prompt)? {
                confirmed.push(task);
            } else {
                outln!(
                    "{} {}{}",
                    theme().notice.paint("Canceled deletion of task"),
                    theme().emphasis.paint(&task.id.to_string()),
                    theme().notice.paint(".")
                )?;
            }
        }
        Ok(confirmed)
    }

    /// A deleted task has nothing left to restore a draft into, and its id
    /// will be handed to a new one.
    fn forget_drafts(tasks: &[Task], outcome: &crate::storage::ConfirmedDeletion) {
        #[cfg(feature = "interactive")]
        for task in tasks.iter().filter(|t| outcome.deleted.contains(&t.id)) {
            super::editor::draft::remove(&Self::draft_slot(
                &format!("task-{}", task.id),
                &Self::edit_prefill(&task.text, task.date),
            ));
        }
        #[cfg(not(feature = "interactive"))]
        let _ = (tasks, outcome);
    }

    fn delete_all_done(tm: &mut TaskManager, yes: bool) -> Result<()> {
        let done: Vec<Task> = tm.tasks().iter().filter(|t| t.done).cloned().collect();
        if done.is_empty() {
            return outln!("{}", theme().warning.paint("No done tasks to delete."));
        }

        #[cfg(feature = "interactive")]
        if !yes {
            Self::check_can_ask()?;
            let confirmed = Self::read_confirmation(&format!(
                "{}{}{} {}",
                theme().accent.paint("Delete all done tasks ("),
                theme().emphasis.paint(&done.len().to_string()),
                theme().accent.paint(")?"),
                Self::yn_hint()
            ))?;
            if !confirmed {
                return outln!("{}", theme().notice.paint("Canceled."));
            }
        }
        // A build without a terminal UI has nothing to ask with: it deletes
        // as told.
        #[cfg(not(feature = "interactive"))]
        let _ = yes;

        // Exactly the tasks that were counted: one that another process has
        // reopened, rewritten or marked done meanwhile (while the prompt was
        // waiting) is not what the user agreed to delete.
        let outcome =
            tm.delete_confirmed(&done, |shown, now| now.done && now.text == shown.text)?;
        Self::forget_drafts(&done, &outcome);
        if !outcome.deleted.is_empty() {
            Self::print_deleted(outcome.deleted.len(), " done tasks.")?;
        }
        Self::report_unconfirmed(&outcome)
    }

    /// Tasks that were to be deleted but are no longer the tasks read when
    /// the command started (and shown by the prompt). Already deleted ones
    /// are fine; changed ones were kept, and that is an error: the command
    /// did not do what was asked.
    fn report_unconfirmed(outcome: &crate::storage::ConfirmedDeletion) -> Result<()> {
        let list = |ids: &[TaskId]| {
            ids.iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        };
        if !outcome.gone.is_empty() {
            outln!(
                "{} {}",
                theme()
                    .warning
                    .paint("Already deleted by another process, IDs:"),
                list(&outcome.gone)
            )?;
        }
        match outcome.changed.as_slice() {
            [] => Ok(()),
            [id] => anyhow::bail!(
                "task {id} was changed by another process meanwhile; \
                 it was not deleted — check `rusk list` and run the command again"
            ),
            ids => anyhow::bail!(
                "tasks {} were changed by another process meanwhile; \
                 they were not deleted — check `rusk list` and run the command again",
                list(ids)
            ),
        }
    }

    fn delete_by_ids(tm: &mut TaskManager, ids: Vec<TaskId>, yes: bool) -> Result<()> {
        let mut not_found: Vec<TaskId> = Vec::new();
        let mut found: Vec<Task> = Vec::new();
        for id in ids {
            match tm.find_task_by_id(id) {
                Some(idx) => found.push(tm.tasks()[idx].clone()),
                None => not_found.push(id),
            }
        }

        #[cfg(feature = "interactive")]
        let found = if yes || found.is_empty() {
            found
        } else {
            Self::confirm_each(found)?
        };
        #[cfg(not(feature = "interactive"))]
        let _ = yes;

        let mut outcome = crate::storage::ConfirmedDeletion::default();
        if !found.is_empty() {
            // Only a task that still has the text that was shown (and
            // confirmed) is deleted.
            outcome = tm.delete_confirmed(&found, |shown, now| now.text == shown.text)?;
            Self::forget_drafts(&found, &outcome);
            if !outcome.deleted.is_empty() {
                Self::print_deleted(outcome.deleted.len(), " task(s).")?;
            }
        }
        Self::print_not_found_ids(&not_found)?;
        Self::report_unconfirmed(&outcome)
    }

    pub fn handle_mark_tasks(tm: &mut TaskManager, ids: Vec<TaskId>, priority: bool) -> Result<()> {
        let (marked, not_found) = if priority {
            tm.mark_priority_tasks(ids)?
        } else {
            tm.mark_tasks(ids)?
        };

        // What the toggle did to its flag, not where the task ended up: the
        // list shows done before priority, and "undone" for a priority taken
        // off told the wrong story (REVIEW №74).
        for (id, now_set) in marked {
            let Some(idx) = tm.find_task_by_id(id) else {
                continue;
            };
            let task = &tm.tasks()[idx];
            let what = match (priority, now_set) {
                (false, true) => "Marked task as done:",
                (false, false) => "Marked task as undone:",
                (true, true) => "Marked task as priority:",
                (true, false) => "Removed priority from task:",
            };
            let heading = Self::task_heading(theme().success.paint(what), id);
            let suffix = Self::after_suffix(task).map(|s| format!(" {s}"));
            Self::print_task_text_with_wrapping_suffixed(&heading, &task.text, suffix.as_deref())?;
        }

        Self::print_not_found_ids(&not_found)
    }

    /// A ` - <label>: <value>` line of the `edit` report, ending in
    /// `(was: <old>)` when the value changed.
    fn field_line(label: &str, value: &str, old: Option<&str>) -> String {
        let line = format!(" {} {}", theme().info.paint(&format!("- {label}:")), value.bold());
        match old {
            None => line,
            Some(old) => {
                let old = if old == "empty" {
                    theme().emphasis.paint(old).bold()
                } else {
                    theme().info.paint(old)
                };
                format!("{line} {}{}{}", theme().info.paint("(was: "), old, theme().info.paint(")"))
            }
        }
    }

    /// The report line for a field that `edit` was asked to set: its value,
    /// "cleared" when a value went away, and what it was when it changed.
    /// Clearing a value that was not there changes nothing (REVIEW №134).
    fn changed_field_line(label: &str, old: &str, new: &str) -> String {
        match (old, new) {
            _ if old == new => Self::field_line(label, new, None),
            (_, "empty") => Self::field_line(label, "cleared", Some(old)),
            _ => Self::field_line(label, new, Some(old)),
        }
    }

    fn format_after_for_display(after: &[TaskId]) -> String {
        if after.is_empty() {
            "empty".to_string()
        } else {
            after
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(",")
        }
    }

    pub fn handle_edit_tasks(
        tm: &mut TaskManager,
        ids: Vec<TaskId>,
        text: Option<Vec<String>>,
        date: Option<String>,
        after: Option<Vec<TaskId>>,
    ) -> Result<()> {
        // Each task as it was, for the report of what changed.
        let before: Vec<Task> = ids
            .iter()
            .filter_map(|&id| tm.find_task_by_id(id).map(|idx| tm.tasks()[idx].clone()))
            .collect();

        if let Some(d) = &date
            && !is_cli_date_clear_value(d)
        {
            validate_cli_date_edit_arg(d)?;
        }
        let date_change_requested = date.is_some();
        let after_change_requested = after.is_some();

        let (edited, unchanged, not_found) = tm.edit_tasks_with_after(ids, text, date, after)?;
        let any_edited = !edited.is_empty();

        let reports = edited
            .iter()
            .map(|&id| (id, theme().success.paint("Edited task:")))
            .chain(unchanged.iter().map(|&id| (id, theme().notice.paint("Task unchanged:"))));
        for (id, what) in reports {
            let Some(task) = tm.find_task_by_id(id).map(|idx| &tm.tasks()[idx]) else {
                continue;
            };
            let old = before.iter().find(|t| t.id == id).unwrap_or(task);
            let mut lines = vec![];
            if date_change_requested {
                lines.push(Self::changed_field_line(
                    "date",
                    &Self::format_date_for_display(old.date),
                    &Self::format_date_for_display(task.date),
                ));
            }
            if after_change_requested {
                lines.push(Self::changed_field_line(
                    "after",
                    &Self::format_after_for_display(&old.after),
                    &Self::format_after_for_display(&task.after),
                ));
            }
            Self::print_task_text_with_wrapping(&Self::task_heading(what, id), &task.text)?;
            for line in lines {
                outln!("{line}")?;
            }
        }

        Self::print_not_found_ids(&not_found)?;
        if any_edited {
            // Blank separator comes from the list header's leading newline.
            Self::handle_list_tasks(tm.tasks(), crate::config::config().compact)?;
        }
        Ok(())
    }

    pub fn handle_list_tasks(tasks: &[Task], compact: bool) -> Result<()> {
        let all: Vec<&Task> = tasks.iter().collect();
        Self::render_task_list(&all, compact, None)
    }

    /// Case-insensitive phrase search (see [`crate::search`]): prints
    /// matching tasks in the usual list format (always full text, never
    /// compact) with matches highlighted. With `only_ids`, prints bare task
    /// IDs one per line (script-friendly).
    pub fn handle_search_tasks(tasks: &[Task], query: &str, only_ids: bool) -> Result<()> {
        let query = Query::new(query);
        let matched: Vec<&Task> = tasks
            .iter()
            // Matched against the text as it is shown (escaped), so what
            // search finds is what it can highlight.
            .filter(|t| query.matches(&crate::printable::escape(&t.text)))
            .collect();

        if only_ids {
            let ids: String = matched.iter().map(|task| format!("{}\n", task.id)).collect();
            return out!("{ids}");
        }

        if matched.is_empty() {
            return outln!("{}", theme().warning.paint("No matching tasks"));
        }

        Self::render_task_list(&matched, false, Some(&query))
    }

    fn render_task_list(tasks: &[&Task], compact: bool, query: Option<&Query>) -> Result<()> {
        let listing = Self::format_task_list(tasks, compact, query, Self::get_max_line_width());
        out!("{listing}")
    }

    /// `rusk list --for-completion-lines`, what the shell completion scripts
    /// read: one line per task, see [`completion_line`].
    pub fn handle_list_tasks_for_completion(tasks: &[Task]) -> Result<()> {
        let listing: String = tasks.iter().map(completion_line).collect();
        out!("{listing}")
    }

    /// `rusk list --for-completion`: the listing as rusk 0.7.3 printed it, for
    /// the completion scripts installed by then — a copy that an upgrade of
    /// the binary does not replace. Their parsers fit this form (a line that
    /// does not start with `<digits>\t` continues the text); fed the escaped
    /// one, they would store `\\` and `\n` in place of a backslash and a
    /// line break.
    pub fn handle_list_tasks_for_old_completion(tasks: &[Task]) -> Result<()> {
        let mut listing = String::new();
        for task in tasks {
            let lines: Vec<&str> = task.text.lines().collect();
            if let Some(first) = lines.first() {
                listing.push_str(&format!("{}\t{}\n", task.id, first));
                for line in lines.iter().skip(1) {
                    listing.push_str(&format!("{line}\n"));
                }
            } else {
                listing.push_str(&format!("{}\t\n", task.id));
            }
        }
        out!("{listing}")
    }

    pub fn handle_restore(tm: &mut TaskManager) -> Result<()> {
        tm.restore_from_backup()
    }

    #[cfg(feature = "interactive")]
    fn handle_skip_task_error(e: &anyhow::Error, id: TaskId) -> Result<bool> {
        if e.downcast_ref::<crate::error::AppError>() == Some(&crate::error::AppError::SkipTask) {
            outln!("{} {}", theme().warning.paint("Skipped task:"), id)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

/// A task as the completion scripts read it: `<id>\t<text>\n`, and the
/// line holds nothing else. In the text a backslash is `\\`, a line feed
/// `\n`, a carriage return `\r` and a tab `\t` — what `printf %b` and its
/// like in every shell decode — so no part of a text can be read as a line
/// of its own, and the first tab of a line is the one after the id. A text
/// with a NUL cannot be passed as an argument at all: its field is left
/// empty, and the scripts offer nothing for it.
pub(crate) fn completion_line(task: &Task) -> String {
    let mut line = format!("{}\t", task.id);
    if !task.text.contains('\0') {
        for c in task.text.chars() {
            match c {
                '\\' => line.push_str("\\\\"),
                '\n' => line.push_str("\\n"),
                '\r' => line.push_str("\\r"),
                '\t' => line.push_str("\\t"),
                c => line.push(c),
            }
        }
    }
    line.push('\n');
    line
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: TaskId, text: &str) -> Task {
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
    fn a_completion_line_holds_the_whole_text_on_one_line() {
        assert_eq!(completion_line(&task(3, "plain text")), "3\tplain text\n");
        assert_eq!(
            completion_line(&task(2, "two first\n1\thijack of one")),
            "2\ttwo first\\n1\\thijack of one\n"
        );
        assert_eq!(
            completion_line(&task(7, "a\\nb \\\\ c\r\nd\n")),
            "7\ta\\\\nb \\\\\\\\ c\\r\\nd\\n\n"
        );
        assert_eq!(completion_line(&task(9, "nul\0byte")), "9\t\n");
    }

    /// A failed save says the text was kept only if the draft is there; if
    /// it could not be written either, the report carries the text.
    #[cfg(feature = "interactive")]
    #[test]
    fn a_failed_save_tells_where_the_text_is() {
        let dir = tempfile::tempdir().unwrap();
        let slot = crate::cli::editor::draft::Slot {
            path: dir.path().join("editor-task-1.draft"),
            key: "task-1".into(),
            base: String::new(),
        };
        let failed = || anyhow::anyhow!("Failed to write the database file");

        let report = |buffer: &str| {
            format!(
                "{:#}",
                HandlerCLI::draft_survives_failed_save(failed(), &slot, buffer, "rusk edit 1")
            )
        };
        let told = report("typed text");
        assert!(told.contains("could not be kept as a draft") && told.ends_with("\ntyped text"), "{told}");

        // A draft of an earlier session is no copy of this text.
        crate::cli::editor::draft::write(&slot, "OLD DRAFT").unwrap();
        let told = report("typed text");
        assert!(told.contains("could not be kept as a draft") && told.ends_with("\ntyped text"), "{told}");

        crate::cli::editor::draft::write(&slot, "typed text").unwrap();
        let told = report("typed text");
        assert!(told.contains("kept as a draft: run `rusk edit 1`"), "{told}");
        assert!(!told.contains("typed text"), "{told}");
    }

    /// REVIEW №33: a task without a date whose text starts with a word that
    /// reads as a date lost the word to the date on a save, even untouched.
    #[cfg(feature = "interactive")]
    #[test]
    fn a_date_like_first_word_stays_text_through_the_editor() {
        let date = chrono::NaiveDate::from_ymd_opt(2027, 1, 1);
        let round_trip = |text: &str, task_date| {
            let prefill = HandlerCLI::edit_prefill(text, task_date);
            assert_eq!(
                HandlerCLI::extract_leading_date(&prefill, task_date),
                (task_date, text.to_string()),
                "{prefill:?}"
            );
            prefill
        };
        for text in ["tomorrow call mom", "Tomorrow call mom", "2d fix", "11-jan-25 x", "+1w later"] {
            assert_eq!(round_trip(text, None), format!("_ {text}"));
            assert_eq!(round_trip(text, date), format!("01-01-2027 _ {text}"));
        }
        // A text that starts with `_` gets the mark after the empty date.
        assert_eq!(round_trip("_ note", None), "_ _ _ note");
        assert_eq!(round_trip("_ note", date), "01-01-2027 _ _ note");
        assert_eq!(round_trip("_", None), "_ _ _");
        assert_eq!(round_trip("call mom tomorrow", None), "call mom tomorrow");
        assert_eq!(round_trip("call mom", date), "01-01-2027 call mom");
        assert_eq!(round_trip("_note", None), "_note");
    }

    /// Review of R24: the date typed in front of the `_`, the date deleted
    /// or replaced by `_` — the word after the mark stays text.
    #[cfg(feature = "interactive")]
    #[test]
    fn a_date_edited_around_the_mark_leaves_the_text_alone() {
        let date = chrono::NaiveDate::from_ymd_opt(2027, 1, 1);
        for (buffer, expected) in [
            ("01-01-2027 _ tomorrow call mom", (date, "tomorrow call mom")),
            ("_ tomorrow call mom", (None, "tomorrow call mom")),
            ("_ _ tomorrow call mom", (None, "tomorrow call mom")),
            ("01-01-2027 _\ntomorrow", (date, "tomorrow")),
            // Only one mark goes, and only a whole `_` word.
            ("01-01-2027 _ _ x", (date, "_ x")),
            ("01-01-2027 _x", (date, "_x")),
            ("01-01-2027 x _ y", (date, "x _ y")),
        ] {
            let (date, text) = expected;
            assert_eq!(HandlerCLI::extract_leading_date(buffer, None), (date, text.to_string()), "{buffer:?}");
        }
    }

    /// Review of R24: the check ran on the stored text, while the editor
    /// reads the date from the text it shows, control characters dropped.
    #[cfg(feature = "interactive")]
    #[test]
    fn a_date_hidden_behind_a_control_character_is_marked_too() {
        for (text, shown) in [("2\u{1b}d fix", "2d fix"), ("tomorrow\u{0} call", "tomorrow call")] {
            let prefill = HandlerCLI::edit_prefill(text, None);
            assert_eq!(prefill, format!("_ {text}"));
            let buffer = crate::cli::editor::text_ops::split_multi_line_prefill(&prefill).join("\n");
            assert_eq!(HandlerCLI::extract_leading_date(&buffer, None), (None, shown.to_string()));
        }
    }

    /// `rusk add -d` on a restored draft: the date goes in front, a date
    /// of the draft's own wins, and `_` gives its place to the date.
    #[cfg(feature = "interactive")]
    #[test]
    fn a_restored_draft_gets_the_date_of_the_command_line() {
        let seed = chrono::NaiveDate::from_ymd_opt(2027, 1, 1);
        for (draft, buffer) in [
            ("buy milk", "01-01-2027 buy milk"),
            ("_ tomorrow call", "01-01-2027 _ tomorrow call"),
            ("_ buy milk", "01-01-2027 buy milk"),
            ("_ _ _ note", "01-01-2027 _ _ note"),
            ("05-05-2027 buy milk", "05-05-2027 buy milk"),
        ] {
            let restored = HandlerCLI::with_seed_date(draft.to_string(), seed);
            assert_eq!(restored, buffer, "{draft:?}");
        }
        assert_eq!(HandlerCLI::with_seed_date("_ x".to_string(), None), "_ x");
        // What is stored is the date and the text after the mark.
        let restored = HandlerCLI::with_seed_date("_ tomorrow call".to_string(), seed);
        assert_eq!(HandlerCLI::extract_leading_date(&restored, None), (seed, "tomorrow call".to_string()));
    }

    /// REVIEW №32: a date alone on the first line left the text starting
    /// with an empty line.
    #[cfg(feature = "interactive")]
    #[test]
    fn a_date_alone_on_the_first_line_leaves_no_empty_line() {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 9, 19);
        for (buffer, text) in [
            ("19-09-2026\nbuy milk", "buy milk"),
            ("19-09-2026 \nbuy milk", "buy milk"),
            ("19-09-2026 first\nsecond", "first\nsecond"),
            ("19-09-2026 only", "only"),
        ] {
            assert_eq!(HandlerCLI::extract_leading_date(buffer, None), (date, text.to_string()));
        }
        assert_eq!(
            HandlerCLI::extract_leading_date("no date here", None),
            (None, "no date here".to_string())
        );
        assert_eq!(HandlerCLI::extract_leading_date("_\nplain", None), (None, "plain".to_string()));
    }

    /// What `printf %b` makes of the field is the text again.
    #[test]
    fn a_completion_line_decodes_to_the_text() {
        fn decode(field: &str) -> String {
            let mut out = String::new();
            let mut chars = field.chars();
            while let Some(c) = chars.next() {
                if c != '\\' {
                    out.push(c);
                    continue;
                }
                match chars.next() {
                    Some('n') => out.push('\n'),
                    Some('r') => out.push('\r'),
                    Some('t') => out.push('\t'),
                    Some('\\') => out.push('\\'),
                    other => panic!("unexpected escape {other:?} in {field:?}"),
                }
            }
            out
        }
        for text in ["x", "\\", "\\n", "a\\\nb", "tab\there", "\r\n\t\\", "it's \"q\" $HOME"] {
            let line = completion_line(&task(1, text));
            let field = line.strip_prefix("1\t").unwrap().strip_suffix('\n').unwrap();
            assert!(!field.contains(['\n', '\r', '\t']), "{field:?}");
            assert_eq!(decode(field), text);
        }
    }
}
