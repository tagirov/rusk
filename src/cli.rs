use crate::{Task, TaskManager, normalize_date_string};
use anyhow::{Context, Result};
use colored::*;
use crossterm::{
    QueueableCommand,
    cursor::MoveTo,
    event::{Event, KeyCode, KeyEvent, KeyModifiers, read},
    style::Print,
    terminal::{disable_raw_mode, enable_raw_mode},
};
use std::io::{self, Write};

/// Interactive command handlers for CLI operations
pub struct HandlerCLI;

impl HandlerCLI {
    /// Read user input from stdin with a prompt
    fn read_user_input(prompt: &str) -> Result<String> {
        print!("{prompt}");
        io::stdout().flush().context("Failed to flush stdout")?;
        let mut input = String::new();
        io::stdin().read_line(&mut input).context("Input error")?;
        Ok(input.trim().to_string())
    }

    /// Print message for unchanged task with optional edited info
    fn print_unchanged_task_message(current_text: &str, edited_info: &[(u8, String)]) {
        if !edited_info.is_empty() {
            let edited_texts: Vec<String> = edited_info
                .iter()
                .map(|(eid, text)| format!("{eid}: {text}"))
                .collect();
            println!(
                "{} {} {} {}",
                "Task already has this content:".magenta(),
                current_text.bold(),
                "(edited:".cyan(),
                format!("{})", edited_texts.join(", ")).bold()
            );
        } else {
            println!(
                "{} {}",
                "Task already has this content:".magenta(),
                current_text.bold()
            );
        }
    }

    /// Handle SkipTask error - return true if skipped, false otherwise
    fn handle_skip_task_error(e: &anyhow::Error, id: u8) -> bool {
        if e.to_string() == "SkipTask" {
            println!("{} {}", "Skipped task:".yellow(), id);
            true
        } else {
            false
        }
    }

    /// Print list of not found task IDs
    fn print_not_found_ids(not_found: &[u8]) {
        if !not_found.is_empty() {
            let list = not_found
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(" ");
            println!("{} {}", "Tasks not found IDs:".yellow(), list);
        }
    }

    /// Format date for display (returns "empty" if None)
    fn format_date_for_display(date: Option<chrono::NaiveDate>) -> String {
        date.map(|d| d.format("%d-%m-%Y").to_string())
            .unwrap_or_else(|| "empty".to_string())
    }

    /// Handle adding a new task with user interaction
    pub fn handle_add_task(
        tm: &mut TaskManager,
        text: Vec<String>,
        date: Option<String>,
    ) -> Result<()> {
        tm.add_task(text, date)?;
        let task = tm.tasks().last().unwrap();
        if let Some(date) = task.date {
            let today = chrono::Local::now().date_naive();
            let date_str = date.format("%d-%m-%Y").to_string();
            let colored_date = if date < today {
                date_str.red()
            } else {
                date_str.cyan()
            };
            println!(
                "{} {}: {} ({})",
                "Added task:".green(),
                task.id,
                task.text.bold(),
                colored_date
            );
        } else {
            println!(
                "{} {}: {}",
                "Added task:".green(),
                task.id,
                task.text.bold()
            );
        }
        Ok(())
    }

    /// Handle deleting tasks with user interaction
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

    /// Interactive single-line editor for task text (no external editor)
    /// If the user submits an empty line, the text is considered unchanged
    /// If allow_skip is true, Escape will return an error instead of exiting (for multi-task editing)
    fn interactive_edit_text(current: &str, task_id: u8, allow_skip: bool) -> Result<Option<String>> {
        println!(
            "{} {} {} {}",
            "Current text[".cyan(),
            task_id.to_string().bright_cyan().bold(),
            "]:".cyan(),
            current.bold()
        );
        println!(
            "{}",
            "Enter new text and press Enter (leave empty to keep, Tab to autocomplete from prefill):".cyan()
        );
        let edited = Self::interactive_line_editor("> ", current, true, None, true, allow_skip)?;
        if edited.trim().is_empty() {
            Ok(None)
        } else {
            Ok(Some(edited))
        }
    }

    /// Low-level single-line editor with raw-mode, prefill, cursor-at-start, Ctrl+Arrows word jumps,
    /// Escape to cancel (exits the program), and optional live validation (with color feedback)
    /// If allow_skip is true, Escape will return an error instead of exiting (for multi-task editing)
    fn interactive_line_editor(
        prompt: &str,
        prefill: &str,
        cursor_at_start: bool,
        validate: Option<fn(&str) -> bool>,
        use_ghost_prefill: bool,
        allow_skip: bool,
    ) -> Result<String> {
        let mut stdout = io::stdout();
        enable_raw_mode().context("Failed to enable raw mode")?;

        // buffer and cursor
        // For single-line editor, normalize prefill to first line only (remove newlines)
        let normalized_prefill: String = prefill.lines().next().unwrap_or("").to_string();
        let mut buffer: String = if use_ghost_prefill {
            String::new()
        } else {
            normalized_prefill.clone()
        };
        let mut cursor_index: usize = if use_ghost_prefill || cursor_at_start {
            0
        } else {
            buffer.len()
        };
        let mut ghost_active: bool = use_ghost_prefill && !normalized_prefill.is_empty();

        // initial render
        stdout.queue(Print(prompt))?;
        let ghost_suffix = Self::calculate_ghost_suffix(ghost_active, cursor_index, &normalized_prefill);
        Self::render_buffer(&mut stdout, &buffer, validate.as_ref(), ghost_suffix)?;
        Self::move_cursor_to(&mut stdout, prompt, &buffer, cursor_index)?;
        stdout.flush().ok();

        loop {
            #[allow(clippy::single_match)]
            match read()? {
                Event::Key(KeyEvent {
                    code, modifiers, ..
                }) => {
                    match (code, modifiers) {
                        (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                            // Ctrl+C: interrupt and exit
                            disable_raw_mode().ok();
                            println!("\n");
                            std::process::exit(130);
                        }
                        (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                            // Ctrl+D: EOF, exit
                            disable_raw_mode().ok();
                            println!("\n");
                            std::process::exit(0);
                        }
                        (KeyCode::Esc, _) => {
                            disable_raw_mode().ok();
                            if allow_skip {
                                println!("\n{}", "Skipping task.".yellow());
                                return Err(anyhow::anyhow!("SkipTask"));
                            } else {
                                println!("\n{}", "Nothing changed.".yellow());
                                std::process::exit(0);
                            }
                        }
                        (KeyCode::Enter, _) => {
                            if let Some(v) = validate {
                                if !buffer.trim().is_empty() && !v(buffer.as_str()) {
                                    // invalid, beep and continue
                                    print!("\x07");
                                    stdout.flush().ok();
                                    continue;
                                }
                            }
                            disable_raw_mode().ok();
                            println!();
                            return Ok(buffer);
                        }
                        (KeyCode::Left, KeyModifiers::CONTROL) => {
                            cursor_index = Self::jump_prev_word(&buffer, cursor_index);
                        }
                        (KeyCode::Right, KeyModifiers::CONTROL) => {
                            cursor_index = Self::jump_next_word(&buffer, cursor_index);
                        }
                        (KeyCode::Tab, _) | (KeyCode::Up, KeyModifiers::CONTROL) => {
                            if !normalized_prefill.is_empty() {
                                // Accept normalized prefill (first line only)
                                buffer = normalized_prefill.clone();
                                cursor_index = buffer.len();
                                ghost_active = false;
                            } else {
                                buffer.clear();
                                cursor_index = 0;
                                ghost_active = false;
                            }
                        }
                        (KeyCode::Char('w'), KeyModifiers::CONTROL) => {
                            if cursor_index > 0 {
                                let new_index = Self::jump_prev_word(&buffer, cursor_index);
                                buffer.drain(new_index..cursor_index);
                                cursor_index = new_index;
                            }
                        }
                        (KeyCode::Backspace, KeyModifiers::CONTROL) => {
                            if cursor_index > 0 {
                                let new_index = Self::jump_prev_word(&buffer, cursor_index);
                                buffer.drain(new_index..cursor_index);
                                cursor_index = new_index;
                            }
                        }
                        (KeyCode::Left, _) => {
                            cursor_index = Self::prev_char_boundary(&buffer, cursor_index);
                        }
                        (KeyCode::Right, _) => {
                            cursor_index = Self::next_char_boundary(&buffer, cursor_index);
                        }
                        (KeyCode::Home, _) => {
                            cursor_index = 0;
                        }
                        (KeyCode::End, _) => {
                            cursor_index = buffer.len();
                        }
                        (KeyCode::Backspace, _) => {
                            if cursor_index > 0 {
                                let prev = Self::prev_char_boundary(&buffer, cursor_index);
                                buffer.drain(prev..cursor_index);
                                cursor_index = prev;
                            } else if ghost_active {
                                // keep ghost suggestion, do nothing
                            }
                        }
                        (KeyCode::Delete, _) => {
                            if cursor_index < buffer.len() {
                                let next = Self::next_char_boundary(&buffer, cursor_index);
                                buffer.drain(cursor_index..next);
                            }
                        }
                        (KeyCode::Char(c), _) => {
                            if ghost_active {
                                buffer.clear();
                                ghost_active = false;
                                cursor_index = 0;
                            }
                            buffer.insert(cursor_index, c);
                            cursor_index = Self::next_char_boundary(&buffer, cursor_index);
                        }
                        _ => {}
                    }

                    // redraw line
                    let (_cx, cy) = crossterm::cursor::position().unwrap_or((0, 0));
                    stdout.queue(MoveTo(0, cy))?;
                    // clear line manually by printing carriage return + spaces + return
                    // Account for ghost text length when calculating total length to clear
                    // Use maximum of current buffer length and normalized prefill length to ensure complete clearing
                    let max_len = std::cmp::max(buffer.len(), normalized_prefill.len());
                    let total_len = prompt.len() + max_len + 16; // extra to wipe colors and ghost text
                    stdout.queue(Print("\r"))?;
                    stdout.queue(Print(" ".repeat(total_len)))?;
                    stdout.queue(Print("\r"))?;
                    stdout.queue(Print(prompt))?;
                    let ghost_suffix = Self::calculate_ghost_suffix(ghost_active, cursor_index, &normalized_prefill);
                    Self::render_buffer(&mut stdout, &buffer, validate.as_ref(), ghost_suffix)?;
                    Self::move_cursor_to(&mut stdout, prompt, &buffer, cursor_index)?;
                    stdout.flush().ok();
                }
                _ => {}
            }
        }
    }

    fn move_cursor_to(
        stdout: &mut io::Stdout,
        prompt: &str,
        buffer: &str,
        cursor_index: usize,
    ) -> Result<()> {
        // We assume single-line input; compute the x position as prompt width + character count up to cursor
        // Use current row
        let (_x, y) = crossterm::cursor::position().unwrap_or((0, 0));
        // Count characters (not bytes) up to cursor_index
        let char_count = Self::byte_idx_to_char_count(buffer, cursor_index);
        let x = (prompt.len() + char_count) as u16;
        stdout.queue(MoveTo(x, y))?;
        Ok(())
    }

    fn render_buffer(
        stdout: &mut io::Stdout,
        buffer: &str,
        validate: Option<&fn(&str) -> bool>,
        ghost_suffix: Option<&str>,
    ) -> Result<()> {
        if let Some(v) = validate {
            let trimmed = buffer.trim();
            if trimmed.is_empty() {
                stdout.queue(Print(buffer))?;
            } else if v(trimmed) {
                stdout.queue(Print(buffer.green()))?;
            } else {
                stdout.queue(Print(buffer.red()))?;
            }
        } else {
            stdout.queue(Print(buffer))?;
        }
        if let Some(ghost) = ghost_suffix {
            if !ghost.is_empty() {
                stdout.queue(Print(ghost.truecolor(128, 128, 128)))?;
            }
        }
        Ok(())
    }

    /// Get the byte index of the previous character boundary, or 0 if at start
    fn prev_char_boundary(s: &str, byte_idx: usize) -> usize {
        if byte_idx == 0 {
            return 0;
        }

        // Find the previous valid char boundary
        // First, ensure we're on a char boundary
        let mut idx = byte_idx;
        while idx > 0 && !s.is_char_boundary(idx) {
            idx -= 1;
        }

        // Now move to the previous char boundary
        if idx > 0 {
            // Find the byte index of the previous character
            // We can use char_indices to find it efficiently
            if let Some((prev_idx, _)) = s.char_indices().take_while(|(i, _)| *i < idx).last() {
                prev_idx
            } else {
                0
            }
        } else {
            0
        }
    }

    /// Get the byte index of the next character boundary, or s.len() if at end
    fn next_char_boundary(s: &str, byte_idx: usize) -> usize {
        let len = s.len();
        if byte_idx >= len {
            return len;
        }
        let mut idx = byte_idx + 1;
        while idx < len && !s.is_char_boundary(idx) {
            idx += 1;
        }
        idx.min(len)
    }

    /// Calculate ghost suffix for ghost prefill display
    fn calculate_ghost_suffix(
        ghost_active: bool,
        cursor_index: usize,
        normalized_prefill: &str,
    ) -> Option<&str> {
        if !ghost_active {
            return None;
        }

        if cursor_index == 0 {
            Some(normalized_prefill)
        } else {
            let safe_idx = if cursor_index < normalized_prefill.len()
                && normalized_prefill.is_char_boundary(cursor_index)
            {
                cursor_index
            } else {
                Self::next_char_boundary(
                    normalized_prefill,
                    cursor_index.min(normalized_prefill.len()),
                )
            };
            if safe_idx < normalized_prefill.len() {
                Some(&normalized_prefill[safe_idx..])
            } else {
                None
            }
        }
    }

    /// Count characters up to byte index
    fn byte_idx_to_char_count(s: &str, byte_idx: usize) -> usize {
        s.char_indices().take_while(|(i, _)| *i < byte_idx).count()
    }

    fn is_word_char(c: char) -> bool {
        c.is_alphanumeric() || c == '_' || c == '-'
    }

    fn jump_prev_word(buffer: &str, cursor: usize) -> usize {
        if cursor == 0 {
            return 0;
        }

        // First, ensure we're at a char boundary
        let mut pos = cursor;
        if !buffer.is_char_boundary(pos) {
            pos = Self::prev_char_boundary(buffer, pos);
        }

        // Get all char indices before cursor
        let chars: Vec<(usize, char)> = buffer
            .char_indices()
            .take_while(|(idx, _)| *idx < pos)
            .collect();

        if chars.is_empty() {
            return 0;
        }

        // Find word boundary going backwards
        let mut i = chars.len() - 1;
        // Skip word chars
        while i > 0 && Self::is_word_char(chars[i].1) {
            i -= 1;
        }
        // Skip non-word chars
        while i > 0 && !Self::is_word_char(chars[i].1) {
            i -= 1;
        }
        // If we stopped on a word char, move to its start
        if i < chars.len() && Self::is_word_char(chars[i].1) {
            while i > 0 && Self::is_word_char(chars[i - 1].1) {
                i -= 1;
            }
            chars[i].0
        } else {
            // We're at a non-word char, move past it
            if i + 1 < chars.len() {
                chars[i + 1].0
            } else {
                chars[i].0
            }
        }
    }

    fn jump_next_word(buffer: &str, cursor: usize) -> usize {
        let len = buffer.len();
        if cursor >= len {
            return len;
        }

        // First, ensure we're at a char boundary
        let mut pos = cursor;
        if !buffer.is_char_boundary(pos) {
            pos = Self::next_char_boundary(buffer, pos);
        }

        // Get all char indices from cursor
        let chars: Vec<(usize, char)> = buffer
            .char_indices()
            .skip_while(|(idx, _)| *idx < pos)
            .collect();

        if chars.is_empty() {
            return len;
        }

        // Find word boundary going forwards
        let mut i = 0;
        // Skip word chars
        while i < chars.len() && Self::is_word_char(chars[i].1) {
            i += 1;
        }
        // Skip non-word chars
        while i < chars.len() && !Self::is_word_char(chars[i].1) {
            i += 1;
        }

        if i < chars.len() { chars[i].0 } else { len }
    }

    /// Internal function to handle interactive editing with optional date editing
    fn handle_edit_tasks_interactive_internal(
        tm: &mut TaskManager,
        ids: Vec<u8>,
        edit_date: bool,
    ) -> Result<()> {
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

                match Self::interactive_edit_text(&current_text, *id, allow_skip) {
                    Ok(Some(new_text)) => {
                        if new_text != current_text {
                            let task = &mut tm.tasks_mut()[idx];
                            task.text = new_text.clone();
                            edited.push(*id);
                            edited_info.push((*id, new_text.clone()));
                            any_changed = true;
                            println!("{} {}: {}", "Edited task:".green(), id, task.text.bold());
                        } else {
                            unchanged.push(*id);
                            Self::print_unchanged_task_message(&current_text, &edited_info);
                        }
                    }
                    Ok(None) => {
                        unchanged.push(*id);
                        Self::print_unchanged_task_message(&current_text, &edited_info);
                    }
                    Err(e) => {
                        if Self::handle_skip_task_error(&e, *id) {
                            continue;
                        }
                        return Err(e);
                    }
                }

                // Edit date if requested
                if edit_date {
                    let current_date = tm.tasks()[idx]
                        .date
                        .map(|d| d.format("%d-%m-%Y").to_string())
                        .unwrap_or_default();
                    println!(
                        "{} {}",
                        "Current date:".cyan(),
                        if current_date.is_empty() {
                            "empty".bold()
                        } else {
                            current_date.bold()
                        }
                    );
                    println!(
                        "{}",
                        "Enter new date DD-MM-YYYY or DD/MM/YYYY (short year like 25 is OK, leave empty to keep, Tab to autocomplete from ghost prefill):".cyan()
                    );
                    let date_editor = |s: &str| {
                        let normalized = normalize_date_string(s);
                        chrono::NaiveDate::parse_from_str(&normalized, "%d-%m-%Y").is_ok()
                    };
                    match Self::interactive_line_editor(
                        "> ",
                        &current_date,
                        true,
                        Some(date_editor),
                        true,
                        allow_skip,
                    ) {
                        Ok(date_input) => {
                            if !date_input.trim().is_empty() {
                                let normalized = normalize_date_string(date_input.trim());
                                if let Ok(parsed) =
                                    chrono::NaiveDate::parse_from_str(&normalized, "%d-%m-%Y")
                                {
                                    let task = &mut tm.tasks_mut()[idx];
                                    if task.date != Some(parsed) {
                                        task.date = Some(parsed);
                                        if !edited.contains(id) {
                                            edited.push(*id);
                                        }
                                        any_changed = true;
                                        println!(
                                            "{} {}: {}",
                                            "Edited task:".green(),
                                            id,
                                            task.text.bold()
                                        );
                                    }
                                }
                            }
                            let final_date_display = if date_input.trim().is_empty() {
                                if current_date.is_empty() {
                                    "empty".to_string()
                                } else {
                                    current_date.clone()
                                }
                            } else {
                                date_input.trim().to_string()
                            };
                            println!(
                                "{} {}",
                                "Date:".cyan(),
                                if final_date_display == "empty" {
                                    "empty".bold()
                                } else {
                                    final_date_display.bold()
                                }
                            );
                        }
                        Err(e) => {
                            if Self::handle_skip_task_error(&e, *id) {
                                continue;
                            }
                            return Err(e);
                        }
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

    /// Handle interactive editing when -d provided without value: per-task edit (text then date)
    pub fn handle_edit_tasks_interactive(tm: &mut TaskManager, ids: Vec<u8>) -> Result<()> {
        Self::handle_edit_tasks_interactive_internal(tm, ids, true)
    }

    /// Handle interactive editing text-only when called without any date flag
    pub fn handle_edit_tasks_interactive_text_only(
        tm: &mut TaskManager,
        ids: Vec<u8>,
    ) -> Result<()> {
        Self::handle_edit_tasks_interactive_internal(tm, ids, false)
    }
    /// Delete all completed tasks with confirmation
    fn delete_all_done(tm: &mut TaskManager) -> Result<()> {
        let done_count = tm.tasks().iter().filter(|t| t.done).count();
        if done_count == 0 {
            println!("{}", "No done tasks to delete.".yellow());
            return Ok(());
        }

        let input = Self::read_user_input(&format!(
            "{}{}{}",
            "Delete all done tasks (".truecolor(255, 165, 0),
            done_count.to_string().white(),
            ")? [y/N]: ".truecolor(255, 165, 0)
        ))?;

        if input.eq_ignore_ascii_case("y") {
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

    /// Delete specific tasks by IDs with confirmation
    fn delete_by_ids(tm: &mut TaskManager, ids: Vec<u8>) -> Result<()> {
        let mut confirmed_ids = Vec::new();
        let mut not_found: Vec<u8> = Vec::new();

        // Get user confirmation for each task
        for &id in &ids {
            if let Some(idx) = tm.find_task_by_id(id) {
                let task = &tm.tasks()[idx];
                let input = Self::read_user_input(&format!(
                    "{}{}{}{}{}{}",
                    "Delete '".truecolor(255, 165, 0),
                    task.text.white(),
                    "' ".truecolor(255, 165, 0),
                    "[ID ".truecolor(255, 165, 0),
                    format!("{}", task.id).white(),
                    "]? [y/N]: ".truecolor(255, 165, 0)
                ))?;
                if input.eq_ignore_ascii_case("y") {
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

        // Delete confirmed tasks using TaskManager
        if !confirmed_ids.is_empty() {
            let deleted_count = confirmed_ids.len();
            let _ = tm.delete_tasks(confirmed_ids)?; // TaskManager handles saving
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

    /// Handle marking tasks as done/undone with user interaction
    pub fn handle_mark_tasks(tm: &mut TaskManager, ids: Vec<u8>) -> Result<()> {
        let (marked, not_found) = tm.mark_tasks(ids)?;

        // Show success messages for marked tasks
        for (id, done) in marked {
            if let Some(idx) = tm.find_task_by_id(id) {
                let task = &tm.tasks()[idx];
                let status = if done { "done" } else { "undone" };
                println!(
                    "{} {}: {}",
                    format!("Marked task as {status}:").green(),
                    id,
                    task.text.bold()
                );
            }
        }

        Self::print_not_found_ids(&not_found);
        Ok(())
    }

    /// Handle editing tasks with user interaction
    pub fn handle_edit_tasks(
        tm: &mut TaskManager,
        ids: Vec<u8>,
        text: Option<Vec<String>>,
        date: Option<String>,
    ) -> Result<()> {
        // Save old dates and IDs before editing
        let ids_copy = ids.clone();
        let mut old_dates: Vec<(u8, Option<chrono::NaiveDate>)> = Vec::new();
        for &id in &ids_copy {
            if let Some(idx) = tm.find_task_by_id(id) {
                old_dates.push((id, tm.tasks()[idx].date));
            }
        }

        // Parse new date before it's moved
        let parsed_new_date = date
            .as_ref()
            .and_then(|d| {
                let normalized = normalize_date_string(d);
                chrono::NaiveDate::parse_from_str(&normalized, "%d-%m-%Y").ok()
            });

        let (edited, unchanged, not_found) = tm.edit_tasks(ids, text, date)?;

        // Show success messages for edited tasks
        for id in edited {
            if let Some(idx) = tm.find_task_by_id(id) {
                let task = &tm.tasks()[idx];
                let old_date = old_dates
                    .iter()
                    .find(|(i, _)| *i == id)
                    .and_then(|(_, d)| *d);
                let new_date = task.date;

                // Print task text
                print!("{} {}: {}", "Edited task:".green(), id, task.text.bold());

                // Print date information
                if parsed_new_date.is_some() {
                    // Date was provided in command
                    if new_date != old_date {
                        // Date changed
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
                        // Date didn't change
                        let date_str = Self::format_date_for_display(new_date);
                        println!(" {} {}", "- date:".cyan(), date_str.bold());
                    }
                } else {
                    println!();
                }
            }
        }

        // Show messages for unchanged tasks (magenta color)
        for id in unchanged {
            if let Some(idx) = tm.find_task_by_id(id) {
                let task = &tm.tasks()[idx];
                let _old_date = old_dates
                    .iter()
                    .find(|(i, _)| *i == id)
                    .and_then(|(_, d)| *d);
                let current_date = task.date;

                // Print task text
                print!(
                    "{} {}",
                    "Task already has this content:".magenta(),
                    task.text.bold()
                );

                // Print date information if date was provided
                if parsed_new_date.is_some() {
                    let date_str = Self::format_date_for_display(current_date);
                    println!(" {} {}", "- date:".cyan(), date_str.bold());
                } else {
                    println!();
                }
            }
        }

        Self::print_not_found_ids(&not_found);
        Ok(())
    }

    /// List all tasks with their status, id, date, and text
    pub fn handle_list_tasks(tasks: &[Task]) {
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

        for task in tasks {
            let status = if task.done {
                "✔".green()
            } else {
                "•".normal()
            };

            let date_str = task
                .date
                .map(|d| d.format("%d-%m-%Y").to_string())
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

            println!(
                "  {} {:>3} {:^10} {}",
                status,
                task.id.to_string().bold(),
                date_colored,
                task.text
            );
        }

        println!("\n");
    }

    /// Handle restoring database from backup
    pub fn handle_restore(tm: &mut TaskManager) -> Result<()> {
        tm.restore_from_backup()
    }
}
