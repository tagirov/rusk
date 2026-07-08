#[cfg(feature = "interactive")]
use crate::parse_cli_date_for_edit;
use crate::config::theme;
use crate::parser::date::is_cli_date_clear_value;
use crate::{Task, TaskManager, validate_cli_date_edit_arg};
use anyhow::Result;
use colored::*;

use super::HandlerCLI;
#[cfg(feature = "interactive")]
use super::editor::EditorExtras;

impl HandlerCLI {
    fn print_added_task(task: &Task) {
        let prefix = if let Some(date) = task.date {
            let colored_date = Self::colored_short_date(date, task.done);
            format!("{} {}: ({})", theme().success.paint("Added task:"), task.id, colored_date)
        } else {
            format!("{} {}:", theme().success.paint("Added task:"), task.id)
        };
        Self::print_task_text_with_wrapping(&prefix, &task.text.bold().to_string());
    }

    /// Returns the draft text for `draft_key` when the user confirms restoring it;
    /// otherwise deletes the draft file and returns `base_prefill` unchanged.
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

    pub fn handle_add_task(
        tm: &mut TaskManager,
        text: Vec<String>,
        date: Option<String>,
    ) -> Result<()> {
        tm.add_task(text, date)?;
        let task = tm.tasks().last().unwrap();
        Self::print_added_task(task);
        Ok(())
    }

    /// Interactive TUI: no inline task text; optional `-d` pre-seeds the first line with that due date.
    #[cfg(feature = "interactive")]
    pub fn handle_add_task_interactive(tm: &mut TaskManager, date: Option<String>) -> Result<()> {
        let draft_dir = TaskManager::get_db_dir();
        let draft_path = Self::draft_path_for(&draft_dir);
        let draft_key = "new-task".to_string();

        let base_prefill = if let Some(ref d) = date {
            let seed = parse_cli_date_for_edit(d, None)?;
            format!("{} ", seed.format("%d-%m-%Y"))
        } else {
            String::new()
        };

        let prompt = format!(
            "{}{}",
            theme().accent.paint("Restore unsaved draft for new task "),
            theme().accent.paint("? [y/N]: ")
        );
        let prefill_owned = Self::prefill_with_draft(base_prefill, &draft_path, &draft_key, &prompt)?;

        let extras = EditorExtras {
            draft_path: Some(draft_path),
            draft_key: Some(draft_key),
            relative_date_base: None,
            ..Default::default()
        };

        let edited = Self::run_multi_line_editor(
            "    ",
            &prefill_owned,
            false,
            None,
            false,
            extras,
        )?;

        if edited.trim().is_empty() {
            return Ok(());
        }

        let (parsed_date, stripped) = Self::extract_leading_date(&edited, None);
        if stripped.trim().is_empty() {
            anyhow::bail!("Task text cannot be empty");
        }

        tm.add_task_with_parsed_date(stripped, parsed_date)?;
        let task = tm.tasks().last().unwrap();
        Self::print_added_task(task);
        Ok(())
    }

    pub fn handle_delete_tasks(tm: &mut TaskManager, ids: Vec<u8>, done: bool) -> Result<()> {
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
        task_id: u8,
        task_date: Option<chrono::NaiveDate>,
        allow_skip: bool,
    ) -> Result<Option<(Option<chrono::NaiveDate>, String)>> {
        let draft_dir = crate::TaskManager::get_db_dir();
        let draft_path = Self::draft_path_for(&draft_dir);
        let draft_key = format!("task-{}", task_id);

        // Prefill embeds the task date as an editable prefix on the first line.
        let base_prefill = if let Some(date) = task_date {
            format!("{} {}", date.format("%d-%m-%Y"), current)
        } else {
            current.to_string()
        };
        let prompt = format!(
            "{} {} {} ",
            theme().accent.paint("Restore unsaved draft for task"),
            theme().emphasis.paint(&task_id.to_string()),
            theme().accent.paint("? [y/N]:")
        );
        let prefill_owned = Self::prefill_with_draft(base_prefill, &draft_path, &draft_key, &prompt)?;

        let extras = EditorExtras {
            draft_path: Some(draft_path),
            draft_key: Some(draft_key),
            relative_date_base: task_date,
            ..Default::default()
        };

        let edited =
            Self::run_multi_line_editor("    ", &prefill_owned, true, None, allow_skip, extras)?;
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
    fn handle_edit_tasks_interactive_internal(tm: &mut TaskManager, ids: Vec<u8>) -> Result<()> {
        let mut any_changed = false;
        let mut edited: Vec<u8> = Vec::new();
        let mut unchanged: Vec<u8> = Vec::new();
        let mut not_found: Vec<u8> = Vec::new();
        let mut edited_info: Vec<(u8, String)> = Vec::new();

        let total_ids = ids.len();
        for (task_idx, id) in ids.iter().enumerate() {
            let is_last = task_idx == total_ids - 1;
            let allow_skip = !is_last;

            if let Some(idx) = tm.find_task_by_id(*id) {
                let current_text = tm.tasks()[idx].text.clone();
                let current_date = tm.tasks()[idx].date;

                match Self::interactive_edit_text(&current_text, *id, current_date, allow_skip) {
                    Ok(Some((new_date, new_text))) => {
                        let text_changed = new_text != current_text;
                        let date_changed = new_date != current_date;
                        if text_changed || date_changed {
                            let task = &mut tm.tasks_mut()[idx];
                            task.text = new_text.clone();
                            task.date = new_date;
                            edited.push(*id);
                            edited_info.push((*id, new_text.clone()));
                            any_changed = true;
                            println!("{} {}", theme().success.paint("Edited task:"), id);
                        } else {
                            unchanged.push(*id);
                            println!("{} {}", theme().notice.paint("Task unchanged:"), id);
                        }
                    }
                    Ok(None) => {
                        unchanged.push(*id);
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
        Ok(())
    }

    #[cfg(feature = "interactive")]
    pub fn handle_edit_tasks_interactive(tm: &mut TaskManager, ids: Vec<u8>) -> Result<()> {
        Self::handle_edit_tasks_interactive_internal(tm, ids)
    }

    #[cfg(feature = "interactive")]
    fn delete_all_done(tm: &mut TaskManager) -> Result<()> {
        let done_count = tm.tasks().iter().filter(|t| t.done).count();
        if done_count == 0 {
            println!("{}", theme().warning.paint("No done tasks to delete."));
            return Ok(());
        }

        let confirmed = Self::read_confirmation(&format!(
            "{}{}{}",
            theme().accent.paint("Delete all done tasks ("),
            theme().emphasis.paint(&done_count.to_string()),
            theme().accent.paint(")? [y/N]: ")
        ))?;

        if confirmed {
            let deleted = tm.delete_all_done()?;
            if deleted > 0 {
                println!(
                    "{}{}{}",
                    theme().accent.paint("Deleted "),
                    theme().emphasis.paint(&deleted.to_string()),
                    theme().accent.paint(" done tasks.")
                );
            }
            Ok(())
        } else {
            println!("Canceled.");
            Ok(())
        }
    }

    #[cfg(not(feature = "interactive"))]
    fn delete_all_done(tm: &mut TaskManager) -> Result<()> {
        let deleted = tm.delete_all_done()?;
        if deleted > 0 {
            println!(
                "{}{}{}",
                theme().accent.paint("Deleted "),
                theme().emphasis.paint(&deleted.to_string()),
                theme().accent.paint(" done tasks.")
            );
        } else {
            println!("{}", theme().warning.paint("No done tasks to delete."));
        }
        Ok(())
    }

    #[cfg(feature = "interactive")]
    fn delete_by_ids(tm: &mut TaskManager, ids: Vec<u8>) -> Result<()> {
        let mut confirmed_ids = Vec::new();
        let mut not_found: Vec<u8> = Vec::new();

        for &id in &ids {
            if let Some(idx) = tm.find_task_by_id(id) {
                let task = &tm.tasks()[idx];
                let prompt = Self::print_delete_confirmation_dialog(&task.text, task.id);
                let confirmed = Self::read_confirmation(&prompt)?;
                if confirmed {
                    confirmed_ids.push(id);
                } else {
                    print!("{} ", theme().notice.paint("Canceled deletion of task"));
                    print!("{}", theme().emphasis.paint(&id.to_string()));
                    println!("{}", theme().notice.paint("."));
                }
            } else {
                not_found.push(id);
            }
        }

        if !confirmed_ids.is_empty() {
            let deleted_count = confirmed_ids.len();
            let _ = tm.delete_tasks(confirmed_ids)?;
            println!(
                "{}{}{}",
                theme().accent.paint("Deleted "),
                theme().emphasis.paint(&deleted_count.to_string()),
                theme().accent.paint(" task(s).")
            );
        }

        Self::print_not_found_ids(&not_found);
        Ok(())
    }

    #[cfg(not(feature = "interactive"))]
    fn delete_by_ids(tm: &mut TaskManager, ids: Vec<u8>) -> Result<()> {
        let mut not_found: Vec<u8> = Vec::new();
        let mut to_delete = Vec::new();

        for &id in &ids {
            if tm.find_task_by_id(id).is_some() {
                to_delete.push(id);
            } else {
                not_found.push(id);
            }
        }

        if !to_delete.is_empty() {
            let deleted_count = to_delete.len();
            let _ = tm.delete_tasks(to_delete)?;
            println!(
                "{}{}{}",
                theme().accent.paint("Deleted "),
                theme().emphasis.paint(&deleted_count.to_string()),
                theme().accent.paint(" task(s).")
            );
        }

        Self::print_not_found_ids(&not_found);
        Ok(())
    }

    pub fn handle_mark_tasks(tm: &mut TaskManager, ids: Vec<u8>, priority: bool) -> Result<()> {
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
                Self::print_task_text_with_wrapping(&prefix, &task.text.bold().to_string());
            }
        }

        Self::print_not_found_ids(&not_found);
        Ok(())
    }

    pub fn handle_edit_tasks(
        tm: &mut TaskManager,
        ids: Vec<u8>,
        text: Option<Vec<String>>,
        date: Option<String>,
    ) -> Result<()> {
        let mut old_dates: Vec<(u8, Option<chrono::NaiveDate>)> = Vec::new();
        for &id in &ids {
            if let Some(idx) = tm.find_task_by_id(id) {
                old_dates.push((id, tm.tasks()[idx].date));
            }
        }

        let is_clearing_date = date.as_deref().is_some_and(is_cli_date_clear_value);
        if let Some(d) = &date
            && !is_cli_date_clear_value(d)
        {
            validate_cli_date_edit_arg(d)?;
        }
        let date_change_requested = date.is_some();

        let (edited, unchanged, not_found) = tm.edit_tasks(ids, text, date)?;

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
                        println!(
                            " {} {} {} {} {}",
                            theme().info.paint("- date:"),
                            "cleared".bold(),
                            "(".normal(),
                            theme().info.paint(&format!("was: {}", old_date_str)),
                            ")".normal()
                        );
                    } else if new_date != old_date {
                        let old_date_str = Self::format_date_for_display(old_date);
                        let new_date_str = Self::format_date_for_display(new_date);
                        if old_date_str == "empty" {
                            println!(
                                " {} {} {} {} {} {}",
                                theme().info.paint("- date:"),
                                new_date_str.bold(),
                                "(".normal(),
                                theme().info.paint("was:"),
                                theme().emphasis.paint(&old_date_str).bold(),
                                ")".normal()
                            );
                        } else {
                            println!(
                                " {} {} {} {} {}",
                                theme().info.paint("- date:"),
                                new_date_str.bold(),
                                "(".normal(),
                                theme().info.paint(&format!("was: {}", old_date_str)),
                                ")".normal()
                            );
                        }
                    } else {
                        let date_str = Self::format_date_for_display(new_date);
                        println!(" {} {}", theme().info.paint("- date:"), date_str.bold());
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
                    println!(" {} {}", theme().info.paint("- date:"), date_str.bold());
                }
            }
        }

        Self::print_not_found_ids(&not_found);
        Ok(())
    }

    pub fn handle_list_tasks(tasks: &[Task], compact: bool) {
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

            if !first_line.is_empty() || !wrapped_lines.is_empty() {
                println!(
                    "  {} {:>2}  {:>9}  {}",
                    status,
                    theme().task_id.paint(&task.id.to_string()).bold(),
                    date_colored,
                    first_line
                );
            }

            if !compact {
                for line in wrapped_lines.iter().skip(1) {
                    println!("{}{}", " ".repeat(prefix_width), line);
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
    fn handle_skip_task_error(e: &anyhow::Error, id: u8) -> bool {
        if e.downcast_ref::<crate::error::AppError>() == Some(&crate::error::AppError::SkipTask) {
            println!("{} {}", theme().warning.paint("Skipped task:"), id);
            true
        } else {
            false
        }
    }
}
