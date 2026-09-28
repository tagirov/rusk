use anyhow::{Context, Result};
use colored::*;
use crossterm::{
    event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, read},
    terminal::{disable_raw_mode, enable_raw_mode},
};

use super::HandlerCLI;
use crate::config::theme;
use crate::model::TaskId;
use crate::{out, outln};

impl HandlerCLI {
    /// Shared "[y/N]: " tail for confirmation prompts: dimmed so the
    /// accent-colored question stays the visual focus.
    pub(crate) fn yn_hint() -> String {
        format!("{} ", "[y/N]:".dimmed())
    }

    /// Whether there is a terminal to talk to: the question on standard
    /// output, the keys from standard input. Every interactive part of
    /// rusk — the editor, the delete prompt — needs both: with either
    /// redirected there is nobody to answer, or the answer comes from
    /// somewhere else than the keys typed (REVIEW №69).
    pub fn on_a_terminal() -> bool {
        use std::io::IsTerminal;
        std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
    }

    /// Puts `prompt` on the screen and reads one key: `y` is yes, Ctrl+C
    /// aborts, Ctrl+D cancels, anything else is no. Raw mode is on only
    /// while the key is read, and off again whatever the read gives.
    pub(crate) fn read_confirmation(prompt: &str) -> Result<bool> {
        out!("{prompt}")?;
        enable_raw_mode().context("Failed to enable raw mode")?;
        let key = Self::read_key();
        disable_raw_mode().ok();
        match key? {
            (KeyCode::Char('y') | KeyCode::Char('Y'), _) => {
                outln!("y")?;
                Ok(true)
            }
            (KeyCode::Char('c'), KeyModifiers::CONTROL) => {
                outln!("\n")?;
                Err(crate::error::AppError::UserAbort.into())
            }
            (KeyCode::Char('d'), KeyModifiers::CONTROL) => {
                outln!("\n")?;
                Err(crate::error::AppError::UserCancel.into())
            }
            _ => {
                outln!()?;
                Ok(false)
            }
        }
    }

    /// The next key pressed. Releases (which Windows reports too, the Enter
    /// that started rusk among them) are no answer.
    fn read_key() -> Result<(KeyCode, KeyModifiers)> {
        loop {
            if let Event::Key(KeyEvent {
                code,
                modifiers,
                kind: KeyEventKind::Press,
                ..
            }) = read()?
            {
                return Ok((code, modifiers));
            }
        }
    }

    /// `dependents` are the tasks that depend on this one: the question
    /// names them, since the deletion takes it off their lists.
    pub(crate) fn print_delete_confirmation_dialog(
        task_text: &str,
        task_id: TaskId,
        dependents: &[TaskId],
    ) -> Result<String> {
        let max_line_width = Self::get_max_line_width();
        const LEFT_MARGIN: usize = 4;
        const RIGHT_MARGIN: usize = 4;
        const PROMPT_RIGHT_MARGIN: usize = 4;

        let prompt_plain = "[y/N]: ";
        let prompt_with_space = format!(" {}", prompt_plain);
        let prompt_width = crate::width::width(&prompt_with_space);
        let prompt_painted = format!(" {}", Self::yn_hint());

        let available_width_for_text = max_line_width
            .saturating_sub(LEFT_MARGIN)
            .saturating_sub(RIGHT_MARGIN);

        let wrapped_lines =
            Self::wrap_text_by_words(&crate::printable::escape(task_text), available_width_for_text);

        let last_line_width = wrapped_lines.last().map_or(0, |l| crate::width::width(l));
        let prompt_fits_on_last_line =
            last_line_width + prompt_width <= available_width_for_text;

        let depended_on = depended_on(dependents)
            .map(|note| theme().notice.paint(&format!(" ({note})")).to_string())
            .unwrap_or_default();
        outln!(
            "{}{}{}{}",
            theme().accent.paint("Delete task "),
            theme().emphasis.paint(&task_id.to_string()),
            depended_on,
            theme().accent.paint(":")
        )?;

        let left_indent = " ".repeat(LEFT_MARGIN);

        for (idx, line) in wrapped_lines.iter().enumerate() {
            let is_last = idx == wrapped_lines.len() - 1;
            if is_last && prompt_fits_on_last_line {
                // Prompt fits after the final text line: print inline and let
                // the caller read the answer from this position.
                out!("{}{}{}", left_indent, line.bold(), prompt_painted)?;
                return Ok(String::new());
            }
            outln!("{}{}", left_indent, line.bold())?;
        }

        let spaces_before_prompt = max_line_width
            .saturating_sub(prompt_width)
            .saturating_sub(PROMPT_RIGHT_MARGIN);
        let indent = " ".repeat(spaces_before_prompt);

        Ok(format!("{}{}", indent, prompt_painted))
    }
}

/// What the delete question says of the tasks that depend on the one it
/// asks about: all of them up to a handful, then how many more (review of
/// R27: a task that 38 depend on made one line of 38 ids).
fn depended_on(dependents: &[TaskId]) -> Option<String> {
    const NAMED: usize = 5;
    let list = |ids: &[TaskId]| ids.iter().map(|id| id.to_string()).collect::<Vec<_>>().join(", ");
    match dependents {
        [] => None,
        [one] => Some(format!("task {one} depends on it")),
        few if few.len() <= NAMED => Some(format!("tasks {} depend on it", list(few))),
        many => Some(format!(
            "tasks {} and {} more depend on it",
            list(&many[..NAMED - 1]),
            many.len() - (NAMED - 1)
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::depended_on;

    #[test]
    fn the_question_names_a_handful_of_dependents() {
        assert_eq!(depended_on(&[]), None);
        assert_eq!(depended_on(&[3]).as_deref(), Some("task 3 depends on it"));
        assert_eq!(depended_on(&[3, 5, 8, 9, 10]).as_deref(), Some("tasks 3, 5, 8, 9, 10 depend on it"));
        let many: Vec<u32> = (2..40).collect();
        assert_eq!(depended_on(&many).as_deref(), Some("tasks 2, 3, 4, 5 and 34 more depend on it"));
    }
}
