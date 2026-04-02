#[cfg(feature = "interactive")]
use crate::error::AppError;
#[cfg(feature = "interactive")]
use anyhow::{Context, Result};
#[cfg(feature = "interactive")]
use colored::*;
#[cfg(feature = "interactive")]
use crossterm::{
    QueueableCommand,
    cursor::MoveTo,
    event::{Event, KeyCode, KeyEvent, KeyModifiers, read},
    style::Print,
    terminal::{
        Clear, ClearType, DisableLineWrap, EnableLineWrap, disable_raw_mode, enable_raw_mode, size,
    },
};
#[cfg(feature = "interactive")]
use std::io::{self, Write};

use super::HandlerCLI;

impl HandlerCLI {
    #[cfg(feature = "interactive")]
    pub(crate) fn interactive_line_editor(
        prompt: &str,
        prefill: &str,
        cursor_at_start: bool,
        validate: Option<fn(&str) -> bool>,
        use_ghost_prefill: bool,
        allow_skip: bool,
    ) -> Result<String> {
        let mut stdout = io::stdout();
        enable_raw_mode().context("Failed to enable raw mode")?;
        stdout.queue(DisableLineWrap)?;
        stdout.flush().ok();
        let (term_cols_u16, term_rows_u16) = size().unwrap_or((0, 0));
        let _term_cols = term_cols_u16 as usize;
        let editor_row_raw = crossterm::cursor::position().unwrap_or((0, 0)).1;
        let editor_row = if term_rows_u16 > 0 && editor_row_raw + 1 >= term_rows_u16 {
            editor_row_raw.saturating_sub(1)
        } else {
            editor_row_raw
        };

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
        stdout.queue(MoveTo(0, editor_row))?;
        stdout.queue(Clear(ClearType::CurrentLine))?;
        stdout.queue(Print(prompt))?;
        let current_cols = size().unwrap_or((term_cols_u16, term_rows_u16)).0 as usize;
        let visible_width = current_cols
            .saturating_sub(prompt.len().saturating_add(1))
            .max(1);
        let (visible_buffer, visible_cursor_char, _) =
            Self::single_line_view(&buffer, cursor_index, visible_width);
        let mut ghost_suffix =
            Self::calculate_ghost_suffix(ghost_active, cursor_index, &normalized_prefill);
        if let Some(gs) = ghost_suffix {
            if visible_buffer.chars().count() + gs.chars().count() > visible_width {
                ghost_suffix = None;
            }
        }
        Self::render_buffer(&mut stdout, &visible_buffer, validate.as_ref(), ghost_suffix)?;
        Self::move_cursor_to(&mut stdout, prompt, visible_cursor_char, editor_row)?;
        stdout.flush().ok();

        loop {
            #[allow(clippy::single_match)]
            match read()? {
                Event::Key(KeyEvent {
                    code, modifiers, ..
                }) => {
                    match (code, modifiers) {
                        (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                            stdout.queue(EnableLineWrap).ok();
                            stdout.flush().ok();
                            disable_raw_mode().ok();
                            println!("\n");
                            std::process::exit(130);
                        }
                        (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                            stdout.queue(EnableLineWrap).ok();
                            stdout.flush().ok();
                            disable_raw_mode().ok();
                            println!("\n");
                            std::process::exit(0);
                        }
                        (KeyCode::Esc, _) => {
                            stdout.queue(EnableLineWrap).ok();
                            stdout.flush().ok();
                            disable_raw_mode().ok();
                            if allow_skip {
                                println!("\n{}", "Skipping task.".yellow());
                                return Err(AppError::SkipTask.into());
                            } else {
                                println!("\n{}", "Nothing changed.".yellow());
                                std::process::exit(0);
                            }
                        }
                        (KeyCode::Enter, _) => {
                            if let Some(v) = validate {
                                if !buffer.trim().is_empty() && !v(buffer.as_str()) {
                                    print!("\x07");
                                    stdout.flush().ok();
                                    continue;
                                }
                            }
                            stdout.queue(EnableLineWrap).ok();
                            stdout.flush().ok();
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
                                // keep ghost suggestion
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
                    stdout.queue(MoveTo(0, editor_row))?;
                    stdout.queue(Clear(ClearType::CurrentLine))?;
                    stdout.queue(Print(prompt))?;
                    let current_cols = size().unwrap_or((term_cols_u16, term_rows_u16)).0 as usize;
                    let visible_width = current_cols
                        .saturating_sub(prompt.len().saturating_add(1))
                        .max(1);
                    let (visible_buffer, visible_cursor_char, _) =
                        Self::single_line_view(&buffer, cursor_index, visible_width);
                    let mut ghost_suffix =
                        Self::calculate_ghost_suffix(ghost_active, cursor_index, &normalized_prefill);
                    if let Some(gs) = ghost_suffix {
                        if visible_buffer.chars().count() + gs.chars().count() > visible_width {
                            ghost_suffix = None;
                        }
                    }
                    Self::render_buffer(&mut stdout, &visible_buffer, validate.as_ref(), ghost_suffix)?;
                    Self::move_cursor_to(&mut stdout, prompt, visible_cursor_char, editor_row)?;
                    stdout.flush().ok();
                }
                Event::Paste(pasted) => {
                    if ghost_active {
                        buffer.clear();
                        ghost_active = false;
                        cursor_index = 0;
                    }
                    buffer.insert_str(cursor_index, &pasted);
                    cursor_index = (cursor_index + pasted.len()).min(buffer.len());
                }
                _other => {}
            }
        }
    }

    #[cfg(feature = "interactive")]
    fn move_cursor_to(
        stdout: &mut io::Stdout,
        prompt: &str,
        visible_cursor_char: usize,
        editor_row: u16,
    ) -> Result<()> {
        let requested_x = prompt.len() + visible_cursor_char;
        let mut x = requested_x as u16;
        if let Ok((cols, _)) = size() {
            if cols > 0 && x >= cols {
                x = cols.saturating_sub(1);
            }
        }
        stdout.queue(MoveTo(x, editor_row))?;
        Ok(())
    }

    #[cfg(feature = "interactive")]
    fn single_line_view(
        buffer: &str,
        cursor_index: usize,
        visible_width: usize,
    ) -> (String, usize, usize) {
        let total_chars = buffer.chars().count();
        let cursor_char = Self::byte_idx_to_char_count(buffer, cursor_index).min(total_chars);
        if total_chars <= visible_width {
            return (buffer.to_string(), cursor_char, 0);
        }
        let start_char = cursor_char.saturating_sub(visible_width.saturating_sub(1));
        let end_char = (start_char + visible_width).min(total_chars);
        let visible: String = buffer
            .chars()
            .skip(start_char)
            .take(end_char.saturating_sub(start_char))
            .collect();
        let visible_cursor = cursor_char.saturating_sub(start_char);
        (visible, visible_cursor, start_char)
    }

    #[cfg(feature = "interactive")]
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

    #[doc(hidden)]
    pub fn prev_char_boundary(s: &str, byte_idx: usize) -> usize {
        if byte_idx == 0 {
            return 0;
        }
        let mut idx = byte_idx;
        while idx > 0 && !s.is_char_boundary(idx) {
            idx -= 1;
        }
        if idx > 0 {
            if let Some((prev_idx, _)) = s.char_indices().take_while(|(i, _)| *i < idx).last() {
                prev_idx
            } else {
                0
            }
        } else {
            0
        }
    }

    #[doc(hidden)]
    pub fn next_char_boundary(s: &str, byte_idx: usize) -> usize {
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

    #[doc(hidden)]
    pub fn calculate_ghost_suffix(
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

    #[doc(hidden)]
    pub fn byte_idx_to_char_count(s: &str, byte_idx: usize) -> usize {
        s.char_indices().take_while(|(i, _)| *i < byte_idx).count()
    }

    #[doc(hidden)]
    pub fn is_word_char(c: char) -> bool {
        c.is_alphanumeric() || c == '_' || c == '-'
    }

    #[doc(hidden)]
    pub fn jump_prev_word(buffer: &str, cursor: usize) -> usize {
        if cursor == 0 {
            return 0;
        }
        let mut pos = cursor;
        if !buffer.is_char_boundary(pos) {
            pos = Self::prev_char_boundary(buffer, pos);
        }
        let chars: Vec<(usize, char)> = buffer
            .char_indices()
            .take_while(|(idx, _)| *idx < pos)
            .collect();
        if chars.is_empty() {
            return 0;
        }
        let mut i = chars.len() - 1;
        while i > 0 && Self::is_word_char(chars[i].1) {
            i -= 1;
        }
        while i > 0 && !Self::is_word_char(chars[i].1) {
            i -= 1;
        }
        if i < chars.len() && Self::is_word_char(chars[i].1) {
            while i > 0 && Self::is_word_char(chars[i - 1].1) {
                i -= 1;
            }
            chars[i].0
        } else {
            if i + 1 < chars.len() {
                chars[i + 1].0
            } else {
                chars[i].0
            }
        }
    }

    #[doc(hidden)]
    pub fn jump_next_word(buffer: &str, cursor: usize) -> usize {
        let len = buffer.len();
        if cursor >= len {
            return len;
        }
        let mut pos = cursor;
        if !buffer.is_char_boundary(pos) {
            pos = Self::next_char_boundary(buffer, pos);
        }
        let chars: Vec<(usize, char)> = buffer
            .char_indices()
            .skip_while(|(idx, _)| *idx < pos)
            .collect();
        if chars.is_empty() {
            return len;
        }
        let mut i = 0;
        while i < chars.len() && Self::is_word_char(chars[i].1) {
            i += 1;
        }
        while i < chars.len() && !Self::is_word_char(chars[i].1) {
            i += 1;
        }
        if i < chars.len() { chars[i].0 } else { len }
    }
}
