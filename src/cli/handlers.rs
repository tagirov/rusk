#[cfg(feature = "interactive")]
use crate::parse_cli_date_for_edit;
use crate::config::theme;
use crate::parser::date::is_cli_date_clear_value;
use crate::{Task, TaskId, TaskManager, validate_cli_date_edit_arg};
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
    seed_date: Option<&'a str>,
    relative_date_base: Option<chrono::NaiveDate>,
    cursor_at_start: bool,
    allow_skip: bool,
}

/// Narrowest id column in `rusk list`, matching the "id" header.
const ID_COLUMN_MIN_WIDTH: usize = 2;
/// Everything in a list line except the id: `"  "`, the status marker, `" "`,
/// `"  "`, the 9-column date and `"  "`. Also the indent of wrapped lines.
const LIST_PREFIX_FIXED_WIDTH: usize = 17;
/// Rule under the list header for a two-digit id column.
const LIST_RULE_WIDTH: usize = 46;
/// Columns held by the short date (`14-sep-26`) in a list line.
const DATE_COLUMN_WIDTH: usize = 9;

impl HandlerCLI {
    /// `(19,22)` suffix for a task with dependencies; appended after the
    /// task text everywhere the task is shown. Ids are bold, parens are not.
    fn after_suffix(task: &Task) -> Option<String> {
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

    fn print_added_task(task: &Task) {
        let prefix = if let Some(date) = task.date {
            let colored_date = Self::colored_short_date(date, task.done);
            format!("{} {}: ({})", theme().success.paint("Added task:"), task.id, colored_date)
        } else {
            format!("{} {}:", theme().success.paint("Added task:"), task.id)
        };
        let suffix = Self::after_suffix(task).map(|s| format!(" {s}"));
        Self::print_task_text_with_wrapping_suffixed(
            &prefix,
            &task.text,
            suffix.as_deref(),
        );
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
        let restored = Self::offer_draft(&slot, what)?;
        // A draft holds text, not a due date, so a `-d` on the command line
        // applies to the restored text too — unless the draft already
        // begins with a date of its own, which the user typed and which
        // wins. Saying so beats letting the argument disappear.
        let restored = restored.map(|text| {
            match (seed_date, Self::extract_leading_date(&text, None).0) {
                (Some(seed), None) => format!("{seed} {text}"),
                (Some(_), Some(own)) => {
                    crate::backend::warn_once(&format!(
                        "Warning: the restored draft already starts with a date \
                         ({}), so it is used instead of the one on the command line",
                        own.format("%d-%m-%Y")
                    ));
                    text
                }
                _ => text,
            }
        });

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
    /// fails leaves it exactly where the next edit will look for it. All
    /// that is left to do here is say so.
    #[cfg(feature = "interactive")]
    fn draft_survives_failed_save(error: anyhow::Error, retry_command: &str) -> anyhow::Error {
        anyhow::anyhow!(
            "{error:#}. Your text was kept as a draft: run `{retry_command}` to restore it"
        )
    }

    pub fn handle_add_task(
        tm: &mut TaskManager,
        text: Vec<String>,
        date: Option<String>,
        after: Vec<crate::TaskId>,
    ) -> Result<()> {
        tm.add_task_with_after(text, date, after)?;
        let task = tm.tasks().last().unwrap();
        Self::print_added_task(task);
        Ok(())
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
        let seed = match date {
            Some(ref d) => Some(parse_cli_date_for_edit(d, None)?.format("%d-%m-%Y").to_string()),
            None => None,
        };
        // There is no task yet, so every new-task draft is pinned to the
        // same empty base: a `-d` seed must not make the draft typed
        // without one unofferable.
        let edited = Self::run_editor_with_draft(EditorSession {
            draft_key: "new-task",
            what: "new task",
            base: "",
            prefill: seed.as_ref().map_or(String::new(), |s| format!("{s} ")),
            seed_date: seed.as_deref(),
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

        tm.add_task_full(stripped, parsed_date, after)
            .map_err(|e| Self::draft_survives_failed_save(e, "rusk add"))?;
        // Stored: the draft has done its job.
        super::editor::draft::remove(&Self::draft_slot("new-task", ""));
        let task = tm.tasks().last().unwrap();
        Self::print_added_task(task);
        Ok(())
    }

    /// `--done` deletes every completed task; otherwise `ids` says which
    /// ones. It is one or the other.
    pub fn handle_delete_tasks(tm: &mut TaskManager, ids: Vec<TaskId>, done: bool) -> Result<()> {
        match (done, ids.is_empty()) {
            (true, true) => Self::delete_all_done(tm),
            (false, false) => Self::delete_by_ids(tm, ids),
            (true, false) => anyhow::bail!("`--done` deletes all completed tasks and takes no task ids"),
            (false, true) => anyhow::bail!("no task ids given; e.g. `rusk del 1,2,3`, or `rusk del --done`"),
        }
    }

    #[cfg(feature = "interactive")]
    /// What the editor opens on for a task: its date as an editable prefix
    /// on the first line, then its text. Also what its draft is pinned to.
    #[cfg(feature = "interactive")]
    fn edit_prefill(current: &str, task_date: Option<chrono::NaiveDate>) -> String {
        match task_date {
            Some(date) => format!("{} {}", date.format("%d-%m-%Y"), current),
            None => current.to_string(),
        }
    }

    #[cfg(feature = "interactive")]
    fn interactive_edit_text(
        current: &str,
        task_id: TaskId,
        task_date: Option<chrono::NaiveDate>,
        allow_skip: bool,
    ) -> Result<Option<(Option<chrono::NaiveDate>, String)>> {
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
        let new_text = if stripped.trim().is_empty() {
            current.to_string()
        } else {
            stripped
        };
        // The buffer cannot hold a tab or a CR (both are normalized on the
        // way in), so a text that differs from the stored one by nothing but
        // that normalization is not an edit: the editor showed it as unchanged
        // and reported no changes to discard.
        let normalized_current =
            super::editor::text_ops::split_multi_line_prefill(current).join("\n");
        let new_text = if new_text != current && new_text == normalized_current {
            current.to_string()
        } else {
            new_text
        };
        Ok(Some((parsed_date, new_text)))
    }

    #[cfg(feature = "interactive")]
    fn extract_leading_date(
        edited: &str,
        task_date: Option<chrono::NaiveDate>,
    ) -> (Option<chrono::NaiveDate>, String) {
        let mut parts = edited.splitn(2, '\n');
        let first = parts.next().unwrap_or("");
        let rest = parts.next();
        let token: String = first.chars().take_while(|c| !c.is_whitespace()).collect();
        if token.is_empty() {
            return (None, edited.to_string());
        }

        // Text with the leading token removed, dropping exactly one separating
        // whitespace after it if present.
        let text_without_token = || {
            let mut tail = first.chars().skip(token.chars().count());
            let peek = tail.clone().next();
            if matches!(peek, Some(c) if c.is_whitespace()) {
                tail.next();
            }
            let first_rest: String = tail.collect();
            match rest {
                Some(r) => format!("{}\n{}", first_rest, r),
                None => first_rest,
            }
        };

        if is_cli_date_clear_value(&token) {
            return (None, text_without_token());
        }
        match crate::parse_cli_date_for_edit(&token, task_date) {
            Ok(date) => (Some(date), text_without_token()),
            Err(_) => (None, edited.to_string()),
        }
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
                Ok(Some((new_date, new_text)))
                    if new_text != current_text || new_date != current_date =>
                {
                    // Saved right away, against the database as it is now:
                    // the editor may have been open for minutes, and a
                    // later Esc or Ctrl+C in this batch must not take a
                    // confirmed edit with it. Success is reported only
                    // once the edit is stored.
                    tm.edit_task_as_seen(
                        *id,
                        (&current_text, current_date),
                        (&new_text, new_date),
                    )
                    .map_err(|e| {
                        Self::draft_survives_failed_save(e, &format!("rusk edit {id}"))
                    })?;
                    // Stored: the draft has done its job.
                    super::editor::draft::remove(&Self::draft_slot(
                        &format!("task-{id}"),
                        &Self::edit_prefill(&current_text, current_date),
                    ));
                    any_changed = true;
                    println!("{} {}", theme().success.paint("Edited task:"), id);
                }
                Ok(_) => {
                    println!("{} {}", theme().notice.paint("Task unchanged:"), id);
                }
                Err(e) => {
                    if Self::handle_skip_task_error(&e, *id) {
                        continue;
                    }
                    return Err(e);
                }
            }
        }

        Self::print_not_found_ids(&not_found);
        if any_changed {
            // Blank separator comes from the list header's leading newline.
            Self::handle_list_tasks(tm.tasks(), crate::config::config().compact);
        }
        Ok(())
    }

    fn print_deleted(count: usize, suffix: &str) {
        println!(
            "{}{}{}",
            theme().accent.paint("Deleted "),
            theme().emphasis.paint(&count.to_string()),
            theme().accent.paint(suffix)
        );
    }

    fn delete_all_done(tm: &mut TaskManager) -> Result<()> {
        #[cfg(feature = "interactive")]
        {
            let done: Vec<Task> = tm.tasks().iter().filter(|t| t.done).cloned().collect();
            if done.is_empty() {
                println!("{}", theme().warning.paint("No done tasks to delete."));
                return Ok(());
            }

            let confirmed = Self::read_confirmation(&format!(
                "{}{}{} {}",
                theme().accent.paint("Delete all done tasks ("),
                theme().emphasis.paint(&done.len().to_string()),
                theme().accent.paint(")?"),
                Self::yn_hint()
            ))?;
            if !confirmed {
                println!("{}", theme().notice.paint("Canceled."));
                return Ok(());
            }

            // Exactly the tasks that were counted in the prompt: one that
            // another process has reopened, rewritten or marked done while
            // the prompt was waiting is not what the user agreed to delete.
            let outcome =
                tm.delete_confirmed(&done, |shown, now| now.done && now.text == shown.text)?;
            if !outcome.deleted.is_empty() {
                Self::print_deleted(outcome.deleted.len(), " done tasks.");
            }
            Self::report_unconfirmed(&outcome)
        }

        #[cfg(not(feature = "interactive"))]
        {
            let deleted = tm.delete_all_done()?;
            if deleted > 0 {
                Self::print_deleted(deleted, " done tasks.");
            } else {
                println!("{}", theme().warning.paint("No done tasks to delete."));
            }
            Ok(())
        }
    }

    /// Tasks that were confirmed for deletion but are no longer the tasks
    /// the prompt showed. Already deleted ones are fine; changed ones were
    /// kept, and that is an error: the command did not do what was asked.
    #[cfg(feature = "interactive")]
    fn report_unconfirmed(outcome: &crate::storage::ConfirmedDeletion) -> Result<()> {
        let list = |ids: &[TaskId]| {
            ids.iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(" ")
        };
        if !outcome.gone.is_empty() {
            println!(
                "{} {}",
                theme()
                    .warning
                    .paint("Already deleted by another process, IDs:"),
                list(&outcome.gone)
            );
        }
        match outcome.changed.as_slice() {
            [] => Ok(()),
            [id] => anyhow::bail!(
                "task {id} was changed by another process after you confirmed; \
                 it was not deleted — check `rusk list` and run the command again"
            ),
            ids => anyhow::bail!(
                "tasks {} were changed by another process after you confirmed; \
                 they were not deleted — check `rusk list` and run the command again",
                list(ids)
            ),
        }
    }

    fn delete_by_ids(tm: &mut TaskManager, ids: Vec<TaskId>) -> Result<()> {
        let mut not_found: Vec<TaskId> = Vec::new();

        #[cfg(feature = "interactive")]
        {
            let mut to_delete: Vec<Task> = Vec::new();
            for &id in &ids {
                let Some(idx) = tm.find_task_by_id(id) else {
                    not_found.push(id);
                    continue;
                };
                let task = &tm.tasks()[idx];
                let prompt = Self::print_delete_confirmation_dialog(&task.text, task.id);
                if Self::read_confirmation(&prompt)? {
                    to_delete.push(task.clone());
                } else {
                    print!("{} ", theme().notice.paint("Canceled deletion of task"));
                    print!("{}", theme().emphasis.paint(&id.to_string()));
                    println!("{}", theme().notice.paint("."));
                }
            }

            let mut outcome = crate::storage::ConfirmedDeletion::default();
            if !to_delete.is_empty() {
                // The prompts showed texts: only a task that still has the
                // text that was confirmed is deleted.
                outcome = tm.delete_confirmed(&to_delete, |shown, now| now.text == shown.text)?;
                if !outcome.deleted.is_empty() {
                    Self::print_deleted(outcome.deleted.len(), " task(s).");
                }
                // A task that is gone has nothing left to restore into,
                // and its id will be handed to a new one.
                for task in to_delete.iter().filter(|t| outcome.deleted.contains(&t.id)) {
                    super::editor::draft::remove(&Self::draft_slot(
                        &format!("task-{}", task.id),
                        &Self::edit_prefill(&task.text, task.date),
                    ));
                }
            }
            Self::print_not_found_ids(&not_found);
            Self::report_unconfirmed(&outcome)
        }

        #[cfg(not(feature = "interactive"))]
        {
            let to_delete: Vec<TaskId> = ids
                .iter()
                .copied()
                .filter(|&id| tm.find_task_by_id(id).is_some())
                .collect();
            not_found.extend(ids.iter().filter(|id| !to_delete.contains(id)));
            if !to_delete.is_empty() {
                let gone = tm.delete_tasks(to_delete.clone())?;
                let deleted_count = to_delete.len() - gone.len();
                if deleted_count > 0 {
                    Self::print_deleted(deleted_count, " task(s).");
                }
                not_found.extend(gone);
            }
            Self::print_not_found_ids(&not_found);
            Ok(())
        }
    }

    pub fn handle_mark_tasks(tm: &mut TaskManager, ids: Vec<TaskId>, priority: bool) -> Result<()> {
        let (marked, not_found) = if priority {
            tm.mark_priority_tasks(ids)?
        } else {
            tm.mark_tasks(ids)?
        };

        for (id, _) in marked {
            if let Some(idx) = tm.find_task_by_id(id) {
                let task = &tm.tasks()[idx];
                let status = if task.done {
                    "done"
                } else if task.priority {
                    "priority"
                } else {
                    "undone"
                };
                let prefix = format!(
                    "{} {}: ",
                    theme().success.paint(&format!("Marked task as {status}:")),
                    id
                );
                let suffix = Self::after_suffix(task).map(|s| format!(" {s}"));
                Self::print_task_text_with_wrapping_suffixed(
                    &prefix,
                    &task.text,
                    suffix.as_deref(),
                );
            }
        }

        Self::print_not_found_ids(&not_found);
        Ok(())
    }

    /// Prints a " - {label}: {new}" report line, appending "(was: ...)" when
    /// `old` is given. The old value "empty" gets emphasis styling only when
    /// `emphasize_empty` is set (the changed-from-empty branch).
    fn print_field_line(label: &str, new_display: ColoredString, old: Option<&str>, emphasize_empty: bool) {
        let label = format!("- {label}:");
        match old {
            None => println!(" {} {}", theme().info.paint(&label), new_display),
            Some(old) if emphasize_empty && old == "empty" => println!(
                " {} {} {} {} {} {}",
                theme().info.paint(&label),
                new_display,
                "(".normal(),
                theme().info.paint("was:"),
                theme().emphasis.paint(old).bold(),
                ")".normal()
            ),
            Some(old) => println!(
                " {} {} {} {} {}",
                theme().info.paint(&label),
                new_display,
                "(".normal(),
                theme().info.paint(&format!("was: {}", old)),
                ")".normal()
            ),
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
        let mut old_dates: Vec<(TaskId, Option<chrono::NaiveDate>)> = Vec::new();
        let mut old_afters: Vec<(TaskId, Vec<TaskId>)> = Vec::new();
        for &id in &ids {
            if let Some(idx) = tm.find_task_by_id(id) {
                old_dates.push((id, tm.tasks()[idx].date));
                old_afters.push((id, tm.tasks()[idx].after.clone()));
            }
        }

        let is_clearing_date = date.as_deref().is_some_and(is_cli_date_clear_value);
        if let Some(d) = &date
            && !is_cli_date_clear_value(d)
        {
            validate_cli_date_edit_arg(d)?;
        }
        let date_change_requested = date.is_some();
        let after_change_requested = after.is_some();
        let is_clearing_after = after.as_deref().is_some_and(<[TaskId]>::is_empty);

        let (edited, unchanged, not_found) = tm.edit_tasks_with_after(ids, text, date, after)?;
        let any_edited = !edited.is_empty();

        for id in edited {
            if let Some(idx) = tm.find_task_by_id(id) {
                let task = &tm.tasks()[idx];
                let old_date = old_dates
                    .iter()
                    .find(|(i, _)| *i == id)
                    .and_then(|(_, d)| *d);
                let new_date = task.date;

                let prefix = format!("{} {}: ", theme().success.paint("Edited task:"), id);
                Self::print_task_text_with_wrapping(&prefix, &task.text);

                if date_change_requested {
                    if is_clearing_date {
                        let old_date_str = Self::format_date_for_display(old_date);
                        Self::print_field_line("date", "cleared".bold(), Some(&old_date_str), false);
                    } else if new_date != old_date {
                        let old_date_str = Self::format_date_for_display(old_date);
                        let new_date_str = Self::format_date_for_display(new_date);
                        Self::print_field_line("date", new_date_str.bold(), Some(&old_date_str), true);
                    } else {
                        let date_str = Self::format_date_for_display(new_date);
                        Self::print_field_line("date", date_str.bold(), None, false);
                    }
                }

                if after_change_requested {
                    let old_after = old_afters
                        .iter()
                        .find(|(i, _)| *i == id)
                        .map(|(_, a)| a.as_slice())
                        .unwrap_or(&[]);
                    if is_clearing_after {
                        let old_str = Self::format_after_for_display(old_after);
                        Self::print_field_line("after", "cleared".bold(), Some(&old_str), false);
                    } else if task.after != old_after {
                        let old_str = Self::format_after_for_display(old_after);
                        let new_str = Self::format_after_for_display(&task.after);
                        Self::print_field_line("after", new_str.bold(), Some(&old_str), true);
                    } else {
                        let after_str = Self::format_after_for_display(&task.after);
                        Self::print_field_line("after", after_str.bold(), None, false);
                    }
                }
            }
        }

        for id in unchanged {
            if let Some(idx) = tm.find_task_by_id(id) {
                let task = &tm.tasks()[idx];
                let current_date = task.date;

                let prefix = format!("{} ", theme().notice.paint("Task already has this content:"));
                Self::print_task_text_with_wrapping(&prefix, &task.text);

                if date_change_requested {
                    let date_str = Self::format_date_for_display(current_date);
                    Self::print_field_line("date", date_str.bold(), None, false);
                }
                if after_change_requested {
                    let after_str = Self::format_after_for_display(&task.after);
                    Self::print_field_line("after", after_str.bold(), None, false);
                }
            }
        }

        Self::print_not_found_ids(&not_found);
        if any_edited {
            // Blank separator comes from the list header's leading newline.
            Self::handle_list_tasks(tm.tasks(), crate::config::config().compact);
        }
        Ok(())
    }

    pub fn handle_list_tasks(tasks: &[Task], compact: bool) {
        let all: Vec<&Task> = tasks.iter().collect();
        Self::render_task_list(&all, compact, None);
    }

    /// Case-insensitive phrase search: prints matching tasks in the usual list
    /// format (always full text, never compact) with matches highlighted.
    /// With `only_ids`, prints bare task IDs one per line (script-friendly).
    pub fn handle_search_tasks(tasks: &[Task], query: &str, only_ids: bool) {
        let needle: Vec<char> = query.to_lowercase().chars().collect();
        let matched: Vec<&Task> = tasks
            .iter()
            // Matched against the text as it is shown (escaped), so what
            // search finds is what it can highlight.
            .filter(|t| Self::find_ci(&crate::printable::escape(&t.text), &needle, 0).is_some())
            .collect();

        if only_ids {
            for task in &matched {
                println!("{}", task.id);
            }
            return;
        }

        if matched.is_empty() {
            println!("{}", theme().warning.paint("No matching tasks"));
            return;
        }

        Self::render_task_list(&matched, false, Some(&needle));
    }

    fn render_task_list(tasks: &[&Task], compact: bool, highlight: Option<&[char]>) {
        if tasks.is_empty() {
            println!("{}", theme().warning.paint("No tasks"));
            return;
        }

        // The id column is as wide as the widest id on screen (never narrower
        // than the two digits the header needs), so three- and four-digit ids
        // keep the date and the text where the header promises them and the
        // continuation indent below matches the first line.
        let id_width = tasks
            .iter()
            .map(|t| t.id.to_string().len())
            .max()
            .unwrap_or(ID_COLUMN_MIN_WIDTH)
            .max(ID_COLUMN_MIN_WIDTH);
        let id_pad = " ".repeat(id_width - ID_COLUMN_MIN_WIDTH);
        let max_line_width = Self::get_max_line_width();

        println!(
            "\n  #  {}{}    {}       {}",
            id_pad,
            theme().list_header.paint("id"),
            theme().list_header.paint("date"),
            theme().list_header.paint("task")
        );
        let rule = (LIST_RULE_WIDTH + id_width - ID_COLUMN_MIN_WIDTH)
            .min(max_line_width.saturating_sub(2));
        println!("  {}", "─".repeat(rule));

        // "  " + status + " " + id + "  " + date(9) + "  "
        let prefix_width = LIST_PREFIX_FIXED_WIDTH + id_width;
        let available_width = max_line_width
            .saturating_sub(prefix_width)
            .saturating_sub(4);

        for task in tasks {
            let status = if task.done {
                theme().done_marker.paint("✔")
            } else if task.priority {
                theme().priority_marker.paint("p").bold()
            } else {
                "•".normal()
            };

            let date_colored = task
                .date
                .map(|d| Self::colored_short_date(d, task.done))
                .unwrap_or_else(|| "".normal());

            // Escaped before anything measures or highlights it: the text is
            // data, and a control character in it must not reach the terminal.
            let shown = crate::printable::escape(&task.text);
            let text_for_list = if compact {
                Self::trim_first_line_for_compact_list(shown.lines().next().unwrap_or(""))
            } else {
                &shown
            };

            // Dependencies land right after the text (ids bold, parens not):
            // on the single shown line in compact mode, otherwise after the
            // last line. Compact mode shows one line only, so the note takes
            // its room out of the wrap budget instead of running past the
            // terminal width.
            let after_note = Self::after_suffix(task).map(|s| format!(" {s}"));
            let after_width = after_note.as_deref().map_or(0, Self::display_width);
            // A note worth more than half the line would leave a stub of a
            // task instead of a compact line: then it goes below, like in the
            // full view, rather than eating the text.
            let wrap_width = if compact && after_width * 2 <= available_width {
                available_width - after_width
            } else {
                available_width
            };
            let wrapped_lines = Self::wrap_text_by_words(text_for_list, wrap_width);

            let first_line: &str = if compact {
                wrapped_lines
                    .first()
                    .map(|s| Self::trim_first_line_for_compact_list(s))
                    .unwrap_or("")
            } else {
                wrapped_lines.first().map(|s| s.as_str()).unwrap_or("")
            };

            // Done tasks get the id in the marker color too, matching the ✔.
            let id_theme = if task.done {
                theme().done_marker
            } else {
                theme().task_id
            };

            let last_line_idx = if compact { 0 } else { wrapped_lines.len().saturating_sub(1) };
            let last_shown: &str = if compact {
                first_line
            } else {
                wrapped_lines.last().map(|s| s.as_str()).unwrap_or("")
            };
            // A note that does not fit after the text gets a line of its own.
            let note_fits = crate::width::width(last_shown) + after_width <= available_width;
            let note_on = |i: usize| -> &str {
                match &after_note {
                    Some(s) if note_fits && i == last_line_idx => s.as_str(),
                    _ => "",
                }
            };

            // Columns are padded by hand: a styled cell carries invisible
            // escape bytes, which `{:>width$}` would count as characters.
            let id_txt = task.id.to_string();
            let id_lead = " ".repeat(id_width - id_txt.len());
            let date_txt = date_colored.to_string();
            let date_lead = " ".repeat(
                DATE_COLUMN_WIDTH.saturating_sub(Self::display_width(&date_txt)),
            );

            if !first_line.is_empty() || !wrapped_lines.is_empty() {
                println!(
                    "  {} {}{}  {}{}  {}{}",
                    status,
                    id_lead,
                    id_theme.paint(&id_txt).bold(),
                    date_lead,
                    date_txt,
                    Self::highlight_keywords(&Self::maybe_highlight(first_line, highlight)),
                    note_on(0)
                );
            }

            if !compact {
                for (i, line) in wrapped_lines.iter().enumerate().skip(1) {
                    println!(
                        "{}{}{}",
                        " ".repeat(prefix_width),
                        Self::maybe_highlight(line, highlight),
                        note_on(i)
                    );
                }
            }

            if let Some(note) = &after_note
                && !note_fits
            {
                for line in Self::wrap_suffix_alone(note, available_width) {
                    println!("{}{}", " ".repeat(prefix_width), line);
                }
            }
        }

        println!("\n");
    }

    /// `rusk list --for-completion-lines`, what the shell completion scripts
    /// read: one line per task, see [`completion_line`].
    pub fn handle_list_tasks_for_completion(tasks: &[Task]) {
        let listing: String = tasks.iter().map(completion_line).collect();
        print!("{listing}");
    }

    /// `rusk list --for-completion`: the listing as rusk 0.7.3 printed it, for
    /// the completion scripts installed by then — a copy that an upgrade of
    /// the binary does not replace. Their parsers fit this form (a line that
    /// does not start with `<digits>\t` continues the text); fed the escaped
    /// one, they would store `\\` and `\n` in place of a backslash and a
    /// line break.
    pub fn handle_list_tasks_for_old_completion(tasks: &[Task]) {
        for task in tasks {
            let lines: Vec<&str> = task.text.lines().collect();
            if let Some(first) = lines.first() {
                println!("{}\t{}", task.id, first);
                for line in lines.iter().skip(1) {
                    println!("{}", line);
                }
            } else {
                println!("{}\t", task.id);
            }
        }
    }

    pub fn handle_restore(tm: &mut TaskManager) -> Result<()> {
        tm.restore_from_backup()
    }

    #[cfg(feature = "interactive")]
    fn handle_skip_task_error(e: &anyhow::Error, id: TaskId) -> bool {
        if e.downcast_ref::<crate::error::AppError>() == Some(&crate::error::AppError::SkipTask) {
            println!("{} {}", theme().warning.paint("Skipped task:"), id);
            true
        } else {
            false
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
