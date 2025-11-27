use crate::{Task, TaskManager, normalize_date_string};
use anyhow::{Context, Result};
use colored::*;
use crossterm::{
    QueueableCommand,
    cursor::MoveTo,
    event::{Event, KeyCode, KeyEvent, KeyModifiers, read},
    style::Print,
    terminal::{disable_raw_mode, enable_raw_mode, size},
};
use std::io::{self, Write};

/// Interactive command handlers for CLI operations
pub struct HandlerCLI;

impl HandlerCLI {
    /// Get the maximum line width for text wrapping
    /// Returns the minimum of 80 and terminal width, or 80 if terminal width cannot be determined
    fn get_max_line_width() -> usize {
        const DEFAULT_WIDTH: usize = 80;
        match size() {
            Ok((width, _)) => {
                let terminal_width = width as usize;
                if terminal_width < DEFAULT_WIDTH {
                    terminal_width
                } else {
                    DEFAULT_WIDTH
                }
            }
            Err(_) => DEFAULT_WIDTH,
        }
    }

    /// Read confirmation (y/n) with immediate response on key press (no Enter required)
    /// Returns true only for 'y' or 'Y', false for any other key (including Enter, Escape, n, N, etc.)
    fn read_confirmation(prompt: &str) -> Result<bool> {
        let mut stdout = io::stdout();
        enable_raw_mode().context("Failed to enable raw mode")?;

        // Print prompt
        stdout.queue(Print(prompt))?;
        stdout.flush().context("Failed to flush stdout")?;

        loop {
            match read()? {
                Event::Key(KeyEvent { code, modifiers, .. }) => {
                    match (code, modifiers) {
                        (KeyCode::Char('y') | KeyCode::Char('Y'), _) => {
                            disable_raw_mode().ok();
                            println!("y");
                            return Ok(true);
                        }
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
                        _ => {
                            // Any other key (n, N, Enter, Escape, etc.) means "no" (cancel)
                            disable_raw_mode().ok();
                            println!();
                            return Ok(false);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Print message for unchanged task with optional edited info
    fn print_unchanged_task_message(current_text: &str, edited_info: &[(u8, String)]) {
        let prefix = if !edited_info.is_empty() {
            let edited_texts: Vec<String> = edited_info
                .iter()
                .map(|(eid, text)| format!("{eid}: {text}"))
                .collect();
            format!(
                "{} {} {}",
                "Task already has this content:".magenta(),
                "(edited:".cyan(),
                format!("{})", edited_texts.join(", ")).bold()
            )
        } else {
            format!("{}", "Task already has this content:".magenta())
        };
        Self::print_task_text_with_wrapping(&prefix, &current_text.bold().to_string());
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
        let prefix = if let Some(date) = task.date {
            let today = chrono::Local::now().date_naive();
            let date_str = date.format("%d-%m-%Y").to_string();
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
        let prefix = format!(
            "{} {} {}",
            "Current text[".cyan(),
            task_id.to_string().bright_cyan().bold(),
            "]:".cyan()
        );
        Self::print_task_text_with_wrapping(&prefix, &current.bold().to_string());
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
                            let prefix = format!("{} {}: ", "Edited task:".green(), id);
                            Self::print_task_text_with_wrapping(&prefix, &task.text.bold().to_string());
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
                                        let prefix = format!("{} {}: ", "Edited task:".green(), id);
                                        Self::print_task_text_with_wrapping(&prefix, &task.text.bold().to_string());
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

    /// Delete specific tasks by IDs with confirmation
    fn delete_by_ids(tm: &mut TaskManager, ids: Vec<u8>) -> Result<()> {
        let mut confirmed_ids = Vec::new();
        let mut not_found: Vec<u8> = Vec::new();

        // Get user confirmation for each task
        for &id in &ids {
            if let Some(idx) = tm.find_task_by_id(id) {
                let task = &tm.tasks()[idx];
                // Print delete confirmation dialog and get prompt
                let prompt = Self::print_delete_confirmation_dialog(&task.text, task.id);
                // Show confirmation prompt (empty if already printed)
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
                let prefix = format!("{} {}: ", format!("Marked task as {status}:").green(), id);
                Self::print_task_text_with_wrapping(&prefix, &task.text.bold().to_string());
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
                let prefix = format!("{} {}: ", "Edited task:".green(), id);
                Self::print_task_text_with_wrapping(&prefix, &task.text.bold().to_string());

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
                let prefix = format!("{} ", "Task already has this content:".magenta());
                Self::print_task_text_with_wrapping(&prefix, &task.text.bold().to_string());

                // Print date information if date was provided
                if parsed_new_date.is_some() {
                    let date_str = Self::format_date_for_display(current_date);
                    println!(" {} {}", "- date:".cyan(), date_str.bold());
                }
            }
        }

        Self::print_not_found_ids(&not_found);
        Ok(())
    }

    /// Print delete confirmation dialog with proper formatting
    /// task_text: the task text to print (will be bold)
    /// task_id: the task ID
    /// Returns the formatted confirmation prompt string for read_confirmation
    /// If prompt fits on the same line, it's already printed and empty string is returned
    /// Text has 4 spaces left and right margin
    fn print_delete_confirmation_dialog(task_text: &str, task_id: u8) -> String {
        let max_line_width = Self::get_max_line_width();
        const LEFT_MARGIN: usize = 4;
        const RIGHT_MARGIN: usize = 4;
        const PROMPT_RIGHT_MARGIN: usize = 4;
        
        let prompt_plain = "[y/N]: ";
        let prompt_with_space = format!(" {}", prompt_plain);
        let prompt_width = prompt_with_space.chars().count();
        
        // Calculate available width for task text (with left and right margins)
        let available_width_for_text = max_line_width
            .saturating_sub(LEFT_MARGIN)
            .saturating_sub(RIGHT_MARGIN);
        
        // Wrap task text
        let wrapped_lines = Self::wrap_text_by_words(task_text, available_width_for_text);
        
        // Check if prompt fits on last line
        let empty_string = String::new();
        let last_line = wrapped_lines.last().unwrap_or(&empty_string);
        let last_line_width = last_line.chars().count();
        let space_needed_for_prompt = prompt_width; // space + prompt
        
        let prompt_fits_on_last_line = last_line_width + space_needed_for_prompt <= available_width_for_text;
        
        // Print "Delete [ID x]:" on first line
        println!(
            "{}{}{}{}",
            "Delete ".truecolor(255, 165, 0),
            "[ID ".truecolor(255, 165, 0),
            format!("{}", task_id).white(),
            "]:".truecolor(255, 165, 0)
        );
        
        // Print text lines starting from second line with left margin
        let left_indent = " ".repeat(LEFT_MARGIN);
        
        // Print first line
        if let Some(first_line) = wrapped_lines.first() {
            let is_single_line = wrapped_lines.len() == 1;
            
            if is_single_line && prompt_fits_on_last_line {
                // Single line with prompt - everything fits on one line
                print!(
                    "{}{}{}",
                    left_indent,
                    first_line.bold(),
                    prompt_with_space.truecolor(255, 165, 0)
                );
                io::stdout().flush().ok();
                // Return empty string - prompt already printed, read_confirmation will handle input
                return String::new();
            } else {
                // First line without prompt
                print!("{}{}", left_indent, first_line.bold());
                println!();
            }
        }
        
        // Print continuation lines
        for (idx, line) in wrapped_lines.iter().enumerate().skip(1) {
            let is_last = idx == wrapped_lines.len() - 1;
            
            if is_last {
                // Last line
                if prompt_fits_on_last_line {
                    // Prompt fits on last line
                    print!(
                        "{}{}{}",
                        left_indent,
                        line.bold(),
                        prompt_with_space.truecolor(255, 165, 0)
                    );
                    io::stdout().flush().ok();
                    // Return empty string - prompt already printed
                    return String::new();
                } else {
                    // Prompt doesn't fit - just print line
                    print!("{}{}", left_indent, line.bold());
                    println!();
                }
            } else {
                // Not last line
                print!("{}{}", left_indent, line.bold());
                println!();
            }
        }
        
        // If we get here, prompt needs to be on a new line with right alignment
        // Calculate spaces before prompt: max_line_width - prompt_width - 4 (right margin)
        let spaces_before_prompt = max_line_width
            .saturating_sub(prompt_width)
            .saturating_sub(PROMPT_RIGHT_MARGIN);
        let indent = " ".repeat(spaces_before_prompt);
        
        // Build the prompt string with formatting
        let prompt = format!("{}{}", indent, prompt_with_space.truecolor(255, 165, 0));
        // Return prompt for read_confirmation to print
        prompt
    }

    /// Print task text with wrapping to fit within terminal width (max 80 characters)
    /// prefix: the prefix string (e.g., "Marked task as done: 5: ")
    /// text: the task text to print (may contain ANSI codes)
    /// Header is printed on first line, text starts on second line
    /// Text has 4 spaces left and right margin
    fn print_task_text_with_wrapping(prefix: &str, text: &str) {
        let max_line_width = Self::get_max_line_width();
        const LEFT_MARGIN: usize = 4;
        const RIGHT_MARGIN: usize = 4;
        
        // Strip ANSI codes from text for width calculation
        let text_plain = Self::strip_ansi_codes(text);
        
        // Extract ANSI codes from the beginning of the original text (for formatting)
        let (ansi_prefix, ansi_suffix) = Self::extract_ansi_codes(text);
        
        // Calculate available width for text (with left and right margins)
        let available_width = max_line_width.saturating_sub(LEFT_MARGIN).saturating_sub(RIGHT_MARGIN);
        
        // Wrap plain text (without ANSI codes) to get correct line breaks
        let wrapped_lines_plain = Self::wrap_text_by_words(&text_plain, available_width);
        
        // Print header on first line
        println!("{}", prefix);
        
        // Print all text lines starting from second line with left margin
        let left_indent = " ".repeat(LEFT_MARGIN);
        for line in wrapped_lines_plain.iter() {
            println!("{}{}{}{}", left_indent, ansi_prefix, line, ansi_suffix);
        }
    }
    
    /// Extract ANSI codes from the beginning and end of a string
    /// Returns (prefix_codes, suffix_codes) where prefix_codes are codes at the start
    /// and suffix_codes are reset codes at the end
    fn extract_ansi_codes(s: &str) -> (String, String) {
        let mut prefix = String::new();
        let mut suffix = String::new();
        let mut chars = s.chars().peekable();
        let mut in_ansi = false;
        let mut ansi_seq = String::new();
        
        // Extract prefix ANSI codes
        while let Some(&ch) = chars.peek() {
            if ch == '\x1b' {
                in_ansi = true;
                ansi_seq.push(ch);
                chars.next();
                while let Some(&next) = chars.peek() {
                    ansi_seq.push(next);
                    chars.next();
                    if next == 'm' {
                        prefix.push_str(&ansi_seq);
                        ansi_seq.clear();
                        in_ansi = false;
                        break;
                    }
                }
            } else if in_ansi {
                break;
            } else {
                // Found first non-ANSI character, stop
                break;
            }
        }
        
        // Check for reset code at the end (we'll add it if prefix had formatting)
        if !prefix.is_empty() {
            suffix.push_str("\x1b[0m");
        }
        
        (prefix, suffix)
    }
    
    /// Strip ANSI color codes from a string to get plain text length
    fn strip_ansi_codes(s: &str) -> String {
        // Simple ANSI code stripper - removes escape sequences
        let mut result = String::new();
        let mut chars = s.chars().peekable();
        
        while let Some(ch) = chars.next() {
            if ch == '\x1b' {
                // Skip escape sequence
                while let Some(&next) = chars.peek() {
                    if next == 'm' {
                        chars.next();
                        break;
                    }
                    chars.next();
                }
            } else {
                result.push(ch);
            }
        }
        
        result
    }

    /// Wrap text by words to fit within a given width
    fn wrap_text_by_words(text: &str, width: usize) -> Vec<String> {
        if text.is_empty() {
            return vec![String::new()];
        }

        let mut lines = Vec::new();
        let mut current_line = String::new();

        for word in text.split_whitespace() {
            let word_len = word.chars().count();
            
            if current_line.is_empty() {
                // First word on line
                if word_len <= width {
                    current_line.push_str(word);
                } else {
                    // Word is longer than width, split it character by character
                    let mut chars: Vec<char> = word.chars().collect();
                    while !chars.is_empty() {
                        let chunk: Vec<char> = chars.drain(..width.min(chars.len())).collect();
                        lines.push(chunk.iter().collect());
                    }
                }
            } else {
                // Check if adding this word would exceed width
                let space_needed = 1 + word_len; // space + word
                if current_line.chars().count() + space_needed <= width {
                    current_line.push(' ');
                    current_line.push_str(word);
                } else {
                    // Current line is full, start a new one
                    lines.push(current_line);
                    current_line = String::new();
                    if word_len <= width {
                        current_line.push_str(word);
                    } else {
                        // Word is longer than width, split it character by character
                        let mut chars: Vec<char> = word.chars().collect();
                        while !chars.is_empty() {
                            let chunk: Vec<char> = chars.drain(..width.min(chars.len())).collect();
                            lines.push(chunk.iter().collect());
                        }
                    }
                }
            }
        }

        if !current_line.is_empty() {
            lines.push(current_line);
        }

        if lines.is_empty() {
            vec![String::new()]
        } else {
            lines
        }
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

        // Maximum line width is terminal width (max 80 characters)
        let max_line_width = Self::get_max_line_width();
        
        // Calculate prefix width: "  " (2) + status (1) + " " (1) + id (3) + " " (1) + date (10) + " " (1) = 19
        // The prefix is: "  " + status + " " + id + " " + date + " " = 19 characters
        let prefix_width = 19; // Width of prefix (status + id + date + spaces)
        // Subtract right margin of 4 spaces
        let available_width = max_line_width.saturating_sub(prefix_width).saturating_sub(4);

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

            // Wrap task text by words
            let wrapped_lines = Self::wrap_text_by_words(&task.text, available_width);

            // Print first line with status, id, and date
            if let Some(first_line) = wrapped_lines.first() {
                println!(
                    "  {} {:>3} {:^10} {}",
                    status,
                    task.id.to_string().bold(),
                    date_colored,
                    first_line
                );
            }

            // Print continuation lines with proper indentation
            for line in wrapped_lines.iter().skip(1) {
                // Indent continuation lines to align with task text start
                println!(
                    "  {} {:>3} {:^10} {}",
                    " ", // Empty status space
                    " ", // Empty id space
                    " ", // Empty date space
                    line
                );
            }
        }

        println!("\n");
    }

    /// Handle restoring database from backup
    pub fn handle_restore(tm: &mut TaskManager) -> Result<()> {
        tm.restore_from_backup()
    }
}
