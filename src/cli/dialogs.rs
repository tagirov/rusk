use anyhow::{Context, Result};
use colored::*;
use crossterm::{
    QueueableCommand,
    event::{Event, KeyCode, KeyEvent, KeyModifiers, read},
    style::Print,
    terminal::{disable_raw_mode, enable_raw_mode},
};
use std::io::{self, Write};

use super::HandlerCLI;
use crate::config::theme;

impl HandlerCLI {
    /// Shared "[y/N]: " tail for confirmation prompts: dimmed so the
    /// accent-colored question stays the visual focus.
    pub(crate) fn yn_hint() -> String {
        format!("{} ", "[y/N]:".dimmed())
    }

    pub(crate) fn read_confirmation(prompt: &str) -> Result<bool> {
        let mut stdout = io::stdout();
        enable_raw_mode().context("Failed to enable raw mode")?;

        stdout.queue(Print(prompt))?;
        stdout.flush().context("Failed to flush stdout")?;

        loop {
            if let Event::Key(KeyEvent {
                code, modifiers, ..
            }) = read()?
            {
                match (code, modifiers) {
                    (KeyCode::Char('y') | KeyCode::Char('Y'), _) => {
                        disable_raw_mode().ok();
                        println!("y");
                        return Ok(true);
                    }
                    (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                        disable_raw_mode().ok();
                        println!("\n");
                        return Err(crate::error::AppError::UserAbort.into());
                    }
                    (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                        disable_raw_mode().ok();
                        println!("\n");
                        return Err(crate::error::AppError::UserCancel.into());
                    }
                    _ => {
                        disable_raw_mode().ok();
                        println!();
                        return Ok(false);
                    }
                }
            }
        }
    }

    pub(crate) fn print_delete_confirmation_dialog(task_text: &str, task_id: u8) -> String {
        let max_line_width = Self::get_max_line_width();
        const LEFT_MARGIN: usize = 4;
        const RIGHT_MARGIN: usize = 4;
        const PROMPT_RIGHT_MARGIN: usize = 4;

        let prompt_plain = "[y/N]: ";
        let prompt_with_space = format!(" {}", prompt_plain);
        let prompt_width = prompt_with_space.chars().count();
        let prompt_painted = format!(" {}", Self::yn_hint());

        let available_width_for_text = max_line_width
            .saturating_sub(LEFT_MARGIN)
            .saturating_sub(RIGHT_MARGIN);

        let wrapped_lines = Self::wrap_text_by_words(task_text, available_width_for_text);

        let last_line_width = wrapped_lines.last().map_or(0, |l| l.chars().count());
        let prompt_fits_on_last_line =
            last_line_width + prompt_width <= available_width_for_text;

        println!(
            "{}{}{}",
            theme().accent.paint("Delete task "),
            theme().emphasis.paint(&task_id.to_string()),
            theme().accent.paint(":")
        );

        let left_indent = " ".repeat(LEFT_MARGIN);

        for (idx, line) in wrapped_lines.iter().enumerate() {
            let is_last = idx == wrapped_lines.len() - 1;
            if is_last && prompt_fits_on_last_line {
                // Prompt fits after the final text line: print inline and let
                // the caller read the answer from this position.
                print!("{}{}{}", left_indent, line.bold(), prompt_painted);
                io::stdout().flush().ok();
                return String::new();
            }
            println!("{}{}", left_indent, line.bold());
        }

        let spaces_before_prompt = max_line_width
            .saturating_sub(prompt_width)
            .saturating_sub(PROMPT_RIGHT_MARGIN);
        let indent = " ".repeat(spaces_before_prompt);

        format!("{}{}", indent, prompt_painted)
    }
}
