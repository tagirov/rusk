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

impl HandlerCLI {
    pub(crate) fn read_confirmation(prompt: &str) -> Result<bool> {
        let mut stdout = io::stdout();
        enable_raw_mode().context("Failed to enable raw mode")?;

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
                            disable_raw_mode().ok();
                            println!("\n");
                            std::process::exit(130);
                        }
                        (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                            disable_raw_mode().ok();
                            println!("\n");
                            std::process::exit(0);
                        }
                        _ => {
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

    pub(crate) fn print_delete_confirmation_dialog(task_text: &str, task_id: u8) -> String {
        let max_line_width = Self::get_max_line_width();
        const LEFT_MARGIN: usize = 4;
        const RIGHT_MARGIN: usize = 4;
        const PROMPT_RIGHT_MARGIN: usize = 4;

        let prompt_plain = "[y/N]: ";
        let prompt_with_space = format!(" {}", prompt_plain);
        let prompt_width = prompt_with_space.chars().count();

        let available_width_for_text = max_line_width
            .saturating_sub(LEFT_MARGIN)
            .saturating_sub(RIGHT_MARGIN);

        let wrapped_lines = Self::wrap_text_by_words(task_text, available_width_for_text);

        let empty_string = String::new();
        let last_line = wrapped_lines.last().unwrap_or(&empty_string);
        let last_line_width = last_line.chars().count();
        let space_needed_for_prompt = prompt_width;

        let prompt_fits_on_last_line = last_line_width + space_needed_for_prompt <= available_width_for_text;

        println!(
            "{}{}{}{}",
            "Delete ".truecolor(255, 165, 0),
            "[ID ".truecolor(255, 165, 0),
            format!("{}", task_id).white(),
            "]:".truecolor(255, 165, 0)
        );

        let left_indent = " ".repeat(LEFT_MARGIN);

        if let Some(first_line) = wrapped_lines.first() {
            let is_single_line = wrapped_lines.len() == 1;

            if is_single_line && prompt_fits_on_last_line {
                print!(
                    "{}{}{}",
                    left_indent,
                    first_line.bold(),
                    prompt_with_space.truecolor(255, 165, 0)
                );
                io::stdout().flush().ok();
                return String::new();
            } else {
                print!("{}{}", left_indent, first_line.bold());
                println!();
            }
        }

        for (idx, line) in wrapped_lines.iter().enumerate().skip(1) {
            let is_last = idx == wrapped_lines.len() - 1;

            if is_last {
                if prompt_fits_on_last_line {
                    print!(
                        "{}{}{}",
                        left_indent,
                        line.bold(),
                        prompt_with_space.truecolor(255, 165, 0)
                    );
                    io::stdout().flush().ok();
                    return String::new();
                } else {
                    print!("{}{}", left_indent, line.bold());
                    println!();
                }
            } else {
                print!("{}{}", left_indent, line.bold());
                println!();
            }
        }

        let spaces_before_prompt = max_line_width
            .saturating_sub(prompt_width)
            .saturating_sub(PROMPT_RIGHT_MARGIN);
        let indent = " ".repeat(spaces_before_prompt);

        format!("{}{}", indent, prompt_with_space.truecolor(255, 165, 0))
    }
}
