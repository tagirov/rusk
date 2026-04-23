use crate::parser::date::is_cli_date_clear_value;
use crate::{Task, TaskManager, validate_cli_date_edit_arg};
use anyhow::Result;
use chrono::Datelike;
use colored::*;

use super::HandlerCLI;
#[cfg(feature = "interactive")]
use super::editor::EditorExtras;

impl HandlerCLI {
    pub fn handle_add_task(
        tm: &mut TaskManager,
        text: Vec<String>,
        date: Option<String>,
    ) -> Result<()> {
        tm.add_task(text, date)?;
        let task = tm.tasks().last().unwrap();
        let prefix = if let Some(date) = task.date {
            let today = chrono::Local::now().date_naive();
            let day = date.day();
            let month = date.format("%b").to_string().to_lowercase();
            let year = date.format("%y").to_string();
            let date_str = format!("{}-{}-{}", day, month, year);
            let colored_date = if date < today {
                date_str.red()
            } else {
                date_str.cyan()
            };
            format!("{} {}: ({})", "Added task:".green(), task.id, colored_date)
        } else {
            format!("{} {}:", "Added task:".green(), task.id)
        };
        Self::print_task_text_with_wrapping(&prefix, &task.text.bold().to_string());
        Ok(())
    }

    pub fn handle_delete_tasks(tm: &mut TaskManager, ids: Vec<u8>, done: bool) -> Result<()> {
        if done && ids.is_empty() {
            Self::delete_all_done(tm)
        } else if !ids.is_empty() {
            Self::delete_by_ids(tm, ids)
        } else {
            println!("{}", "Please specify id(s) or --done.".yellow());
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
        let mut prefill_owned = base_prefill.clone();
        if draft_path.exists() {
            if let Some(text) = Self::read_draft_for(&draft_path, &draft_key) {
                if text != base_prefill {
                    let prompt = format!(
                        "{} {} {} ",
                        "Restore unsaved draft for task".truecolor(255, 165, 0),
                        task_id.to_string().white(),
                        "? [y/N]:".truecolor(255, 165, 0)
                    );
                    if Self::read_confirmation(&prompt)? {
                        prefill_owned = text;
                    } else {
                        let _ = std::fs::remove_file(&draft_path);
                    }
                }
            }
        }

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
        if is_cli_date_clear_value(&token) {
            let token_chars = token.chars().count();
            let mut tail = first.chars().skip(token_chars);
            let peek = tail.clone().next();
            if matches!(peek, Some(c) if c.is_whitespace()) {
                tail.next();
            }
            let first_rest: String = tail.collect();
            let new_text = match rest {
                Some(r) => format!("{}\n{}", first_rest, r),
                None => first_rest,
            };
            return (None, new_text);
        }
        match crate::parse_cli_date_for_edit(&token, task_date) {
            Ok(date) => {
                let token_chars = token.chars().count();
                let mut tail = first.chars().skip(token_chars);
                // Drop exactly one separating whitespace if present.
                let peek = tail.clone().next();
                if matches!(peek, Some(c) if c.is_whitespace()) {
                    tail.next();
                }
                let first_rest: String = tail.collect();
                let new_text = match rest {
                    Some(r) => format!("{}\n{}", first_rest, r),
                    None => first_rest,
                };
                (Some(date), new_text)
            }
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
                            println!("{} {}", "Edited task:".green(), id);
                        } else {
                            unchanged.push(*id);
                            println!("{} {}", "Task unchanged:".magenta(), id);
                        }
                    }
                    Ok(None) => {
                        unchanged.push(*id);
                        println!("{} {}", "Task unchanged:".magenta(), id);
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
            println!("{}", "No done tasks to delete.".yellow());
            return Ok(());
        }

        let confirmed = Self::read_confirmation(&format!(
            "{}{}{}",
            "Delete all done tasks (".truecolor(255, 165, 0),
            done_count.to_string().white(),
            ")? [y/N]: ".truecolor(255, 165, 0)
        ))?;

        if confirmed {
            let deleted = tm.delete_all_done()?;
            if deleted > 0 {
                println!(
                    "{}{}{}",
                    "Deleted ".truecolor(255, 165, 0),
                    deleted.to_string().white(),
                    " done tasks.".truecolor(255, 165, 0)
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
                "Deleted ".truecolor(255, 165, 0),
                deleted.to_string().white(),
                " done tasks.".truecolor(255, 165, 0)
            );
        } else {
            println!("{}", "No done tasks to delete.".yellow());
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
                    print!("{} ", "Canceled deletion of task".magenta());
                    print!("{}", id.to_string().white());
                    println!("{}", ".".magenta());
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
                "Deleted ".truecolor(255, 165, 0),
                deleted_count.to_string().white(),
                " task(s).".truecolor(255, 165, 0)
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
                "Deleted ".truecolor(255, 165, 0),
                deleted_count.to_string().white(),
                " task(s).".truecolor(255, 165, 0)
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
                let prefix = format!("{} {}: ", format!("Marked task as {status}:").green(), id);
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
        let ids_copy = ids.clone();
        let mut old_dates: Vec<(u8, Option<chrono::NaiveDate>)> = Vec::new();
        for &id in &ids_copy {
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

                let prefix = format!("{} {}: ", "Edited task:".green(), id);
                Self::print_task_text_with_wrapping(&prefix, &task.text.bold().to_string());

                if date_change_requested {
                    if is_clearing_date {
                        let old_date_str = Self::format_date_for_display(old_date);
                        println!(
                            " {} {} {} {} {}",
                            "- date:".cyan(),
                            "cleared".bold(),
                            "(".normal(),
                            format!("was: {}", old_date_str).cyan(),
                            ")".normal()
                        );
                    } else {
                        if new_date != old_date {
                            let old_date_str = Self::format_date_for_display(old_date);
                            let new_date_str = Self::format_date_for_display(new_date);
                            if old_date_str == "empty" {
                                println!(
                                    " {} {} {} {} {} {}",
                                    "- date:".cyan(),
                                    new_date_str.bold(),
                                    "(".normal(),
                                    "was:".cyan(),
                                    old_date_str.white().bold(),
                                    ")".normal()
                                );
                            } else {
                                println!(
                                    " {} {} {} {} {}",
                                    "- date:".cyan(),
                                    new_date_str.bold(),
                                    "(".normal(),
                                    format!("was: {}", old_date_str).cyan(),
                                    ")".normal()
                                );
                            }
                        } else {
                            let date_str = Self::format_date_for_display(new_date);
                            println!(" {} {}", "- date:".cyan(), date_str.bold());
                        }
                    }
                }
            }
        }

        for id in unchanged {
            if let Some(idx) = tm.find_task_by_id(id) {
                let task = &tm.tasks()[idx];
                let _old_date = old_dates
                    .iter()
                    .find(|(i, _)| *i == id)
                    .and_then(|(_, d)| *d);
                let current_date = task.date;

                let prefix = format!("{} ", "Task already has this content:".magenta());
                Self::print_task_text_with_wrapping(&prefix, &task.text.bold().to_string());

                if date_change_requested {
                    let date_str = Self::format_date_for_display(current_date);
                    println!(" {} {}", "- date:".cyan(), date_str.bold());
                }
            }
        }

        Self::print_not_found_ids(&not_found);
        Ok(())
    }

    pub fn handle_list_tasks(tasks: &[Task], compact: bool) {
        if tasks.is_empty() {
            println!("{}", "No tasks".yellow());
            return;
        }

        println!(
            "\n  #  {}    {}       {}",
            "id".blue(),
            "date".blue(),
            "task".blue()
        );
        println!("  ──────────────────────────────────────────────");

        let max_line_width = Self::get_max_line_width();

        let prefix_width = 19;
        let available_width = max_line_width
            .saturating_sub(prefix_width)
            .saturating_sub(4);

        for task in tasks {
            let status = if task.done {
                "✔".green()
            } else if task.priority {
                "p".truecolor(255, 165, 0).bold()
            } else {
                "•".normal()
            };

            let date_str = task
                .date
                .map(|d| {
                    let day = d.day();
                    let month = d.format("%b").to_string().to_lowercase();
                    let year = d.format("%y").to_string();
                    format!("{}-{}-{}", day, month, year)
                })
                .unwrap_or_default();

            let date_colored = if let Some(d) = task.date {
                if d < chrono::Local::now().date_naive() && !task.done {
                    date_str.red()
                } else {
                    date_str.cyan()
                }
            } else {
                "".normal()
            };

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
                    task.id.to_string().bold(),
                    date_colored,
                    first_line
                );
            }

            if !compact {
                for line in wrapped_lines.iter().skip(1) {
                    println!("  {} {:>3} {:>10} {}", " ", " ", " ", line);
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
            println!("{} {}", "Skipped task:".yellow(), id);
            true
        } else {
            false
        }
    }
}
