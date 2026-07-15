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

impl HandlerCLI {
    /// `(19,22)` suffix for a task with dependencies; appended after the
    /// task text everywhere the task is shown.
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
        Some(format!("({ids})"))
    }

    fn print_added_task(task: &Task) {
        let prefix = if let Some(date) = task.date {
            let colored_date = Self::colored_short_date(date, task.done);
            format!("{} {}: ({})", theme().success.paint("Added task:"), task.id, colored_date)
        } else {
            format!("{} {}:", theme().success.paint("Added task:"), task.id)
        };
        let text = match Self::after_suffix(task) {
            Some(suffix) => format!("{} {}", task.text, suffix),
            None => task.text.clone(),
        };
        Self::print_task_text_with_wrapping(&prefix, &text.bold().to_string());
    }

    /// If a draft stored under `draft_key` exists and differs from `base_prefill`,
    /// asks for confirmation: returns the draft text on "y", otherwise deletes the
    /// draft file. In all other cases returns `base_prefill` unchanged.
    #[cfg(feature = "interactive")]
    fn prefill_with_draft(
        base_prefill: String,
        draft_path: &std::path::Path,
        draft_key: &str,
        confirm_prompt: &str,
    ) -> Result<String> {
        if draft_path.exists()
            && let Some(text) = Self::read_draft_for(draft_path, draft_key)
            && text != base_prefill
        {
            if Self::read_confirmation(confirm_prompt)? {
                return Ok(text);
            }
            let _ = std::fs::remove_file(draft_path);
        }
        Ok(base_prefill)
    }

    /// Runs the multi-line editor with draft persistence wired up: resolves the
    /// draft path, offers to restore an existing draft, and passes the draft
    /// settings through `EditorExtras`.
    #[cfg(feature = "interactive")]
    fn run_editor_with_draft(
        draft_key: String,
        base_prefill: String,
        confirm_prompt: &str,
        relative_date_base: Option<chrono::NaiveDate>,
        cursor_at_start: bool,
        allow_skip: bool,
    ) -> Result<String> {
        let draft_path = Self::draft_path_for(&TaskManager::get_db_dir());
        let prefill = Self::prefill_with_draft(base_prefill, &draft_path, &draft_key, confirm_prompt)?;

        let extras = EditorExtras {
            draft_path: Some(draft_path),
            draft_key: Some(draft_key),
            relative_date_base,
            ..Default::default()
        };

        Self::run_multi_line_editor("    ", &prefill, cursor_at_start, None, allow_skip, extras)
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
        let base_prefill = if let Some(ref d) = date {
            let seed = parse_cli_date_for_edit(d, None)?;
            format!("{} ", seed.format("%d-%m-%Y"))
        } else {
            String::new()
        };

        let prompt = format!(
            "{} {}",
            theme().accent.paint("Restore unsaved draft for new task?"),
            Self::yn_hint()
        );
        let edited =
            Self::run_editor_with_draft("new-task".to_string(), base_prefill, &prompt, None, false, false)?;

        if edited.trim().is_empty() {
            return Ok(());
        }

        let (parsed_date, stripped) = Self::extract_leading_date(&edited, None);
        if stripped.trim().is_empty() {
            anyhow::bail!("Task text cannot be empty");
        }

        tm.add_task_full(stripped, parsed_date, after)?;
        let task = tm.tasks().last().unwrap();
        Self::print_added_task(task);
        Ok(())
    }

    pub fn handle_delete_tasks(tm: &mut TaskManager, ids: Vec<TaskId>, done: bool) -> Result<()> {
        if done && ids.is_empty() {
            Self::delete_all_done(tm)
        } else if !ids.is_empty() {
            Self::delete_by_ids(tm, ids)
        } else {
            println!("{}", theme().warning.paint("Please specify id(s) or --done."));
            Ok(())
        }
    }

    #[cfg(feature = "interactive")]
    fn interactive_edit_text(
        current: &str,
        task_id: TaskId,
        task_date: Option<chrono::NaiveDate>,
        allow_skip: bool,
    ) -> Result<Option<(Option<chrono::NaiveDate>, String)>> {
        // Prefill embeds the task date as an editable prefix on the first line.
        let base_prefill = if let Some(date) = task_date {
            format!("{} {}", date.format("%d-%m-%Y"), current)
        } else {
            current.to_string()
        };
        let prompt = format!(
            "{} {}{} {}",
            theme().accent.paint("Restore unsaved draft for task"),
            theme().emphasis.paint(&task_id.to_string()),
            theme().accent.paint("?"),
            Self::yn_hint()
        );
        let edited = Self::run_editor_with_draft(
            format!("task-{}", task_id),
            base_prefill,
            &prompt,
            task_date,
            true,
            allow_skip,
        )?;
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

            if let Some(idx) = tm.find_task_by_id(*id) {
                let current_text = tm.tasks()[idx].text.clone();
                let current_date = tm.tasks()[idx].date;

                match Self::interactive_edit_text(&current_text, *id, current_date, allow_skip) {
                    Ok(Some((new_date, new_text)))
                        if new_text != current_text || new_date != current_date =>
                    {
                        let task = &mut tm.tasks_mut()[idx];
                        task.text = new_text;
                        task.date = new_date;
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
            } else {
                not_found.push(*id);
            }
        }

        if any_changed {
            tm.save()?;
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
            let done_count = tm.tasks().iter().filter(|t| t.done).count();
            if done_count == 0 {
                println!("{}", theme().warning.paint("No done tasks to delete."));
                return Ok(());
            }

            let confirmed = Self::read_confirmation(&format!(
                "{}{}{} {}",
                theme().accent.paint("Delete all done tasks ("),
                theme().emphasis.paint(&done_count.to_string()),
                theme().accent.paint(")?"),
                Self::yn_hint()
            ))?;
            if !confirmed {
                println!("{}", theme().notice.paint("Canceled."));
                return Ok(());
            }
        }

        let deleted = tm.delete_all_done()?;
        if deleted > 0 {
            Self::print_deleted(deleted, " done tasks.");
        }
        #[cfg(not(feature = "interactive"))]
        if deleted == 0 {
            println!("{}", theme().warning.paint("No done tasks to delete."));
        }
        Ok(())
    }

    fn delete_by_ids(tm: &mut TaskManager, ids: Vec<TaskId>) -> Result<()> {
        let mut to_delete = Vec::new();
        let mut not_found: Vec<TaskId> = Vec::new();

        for &id in &ids {
            match tm.find_task_by_id(id) {
                #[cfg(feature = "interactive")]
                Some(idx) => {
                    let task = &tm.tasks()[idx];
                    let prompt = Self::print_delete_confirmation_dialog(&task.text, task.id);
                    let confirmed = Self::read_confirmation(&prompt)?;
                    if confirmed {
                        to_delete.push(id);
                    } else {
                        print!("{} ", theme().notice.paint("Canceled deletion of task"));
                        print!("{}", theme().emphasis.paint(&id.to_string()));
                        println!("{}", theme().notice.paint("."));
                    }
                }
                #[cfg(not(feature = "interactive"))]
                Some(_) => to_delete.push(id),
                None => not_found.push(id),
            }
        }

        if !to_delete.is_empty() {
            let deleted_count = to_delete.len();
            let _ = tm.delete_tasks(to_delete)?;
            Self::print_deleted(deleted_count, " task(s).");
        }

        Self::print_not_found_ids(&not_found);
        Ok(())
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
                let text = match Self::after_suffix(task) {
                    Some(suffix) => format!("{} {}", task.text, suffix),
                    None => task.text.clone(),
                };
                Self::print_task_text_with_wrapping(&prefix, &text.bold().to_string());
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
                Self::print_task_text_with_wrapping(&prefix, &task.text.bold().to_string());

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
                Self::print_task_text_with_wrapping(&prefix, &task.text.bold().to_string());

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
            .filter(|t| Self::find_ci(&t.text, &needle, 0).is_some())
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

        println!(
            "\n  #  {}    {}       {}",
            theme().list_header.paint("id"),
            theme().list_header.paint("date"),
            theme().list_header.paint("task")
        );
        println!("  ──────────────────────────────────────────────");

        let max_line_width = Self::get_max_line_width();

        let prefix_width = 19;
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

            let text_for_list = if compact {
                Self::trim_first_line_for_compact_list(task.text.lines().next().unwrap_or(""))
            } else {
                task.text.as_str()
            };
            let wrapped_lines = Self::wrap_text_by_words(text_for_list, available_width);

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

            // Dependencies land in bold right after the text: on the single
            // shown line in compact mode, otherwise after the last line.
            let after_note = Self::after_suffix(task).map(|s| format!(" {}", s.bold()));
            let last_line_idx = if compact { 0 } else { wrapped_lines.len().saturating_sub(1) };

            if !first_line.is_empty() || !wrapped_lines.is_empty() {
                println!(
                    "  {} {:>2}  {:>9}  {}{}",
                    status,
                    id_theme.paint(&task.id.to_string()).bold(),
                    date_colored,
                    Self::highlight_keywords(&Self::maybe_highlight(first_line, highlight)),
                    if last_line_idx == 0 {
                        after_note.as_deref().unwrap_or("")
                    } else {
                        ""
                    }
                );
            }

            if !compact {
                for (i, line) in wrapped_lines.iter().enumerate().skip(1) {
                    println!(
                        "{}{}{}",
                        " ".repeat(prefix_width),
                        Self::maybe_highlight(line, highlight),
                        if i == last_line_idx {
                            after_note.as_deref().unwrap_or("")
                        } else {
                            ""
                        }
                    );
                }
            }
        }

        println!("\n");
    }

    pub fn handle_list_tasks_for_completion(tasks: &[Task]) {
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
