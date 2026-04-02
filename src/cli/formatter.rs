use colored::*;

use super::HandlerCLI;

impl HandlerCLI {
    #[doc(hidden)]
    pub fn get_max_line_width() -> usize {
        #[cfg(feature = "interactive")]
        let raw = match crossterm::terminal::size() {
            Ok((width, _)) => width,
            Err(_) => 0,
        };
        #[cfg(not(feature = "interactive"))]
        let raw = 0u16;
        Self::normalize_terminal_width(raw)
    }

    #[doc(hidden)]
    pub fn normalize_terminal_width(raw: u16) -> usize {
        const DEFAULT_WIDTH: usize = 80;
        let w = raw as usize;
        if w == 0 { DEFAULT_WIDTH } else { w.min(DEFAULT_WIDTH) }
    }

    #[doc(hidden)]
    pub fn format_date_for_display(date: Option<chrono::NaiveDate>) -> String {
        date.map(|d| d.format("%d-%m-%Y").to_string())
            .unwrap_or_else(|| "empty".to_string())
    }

    pub(crate) fn print_task_text_with_wrapping(prefix: &str, text: &str) {
        let max_line_width = Self::get_max_line_width();
        const LEFT_MARGIN: usize = 4;
        const RIGHT_MARGIN: usize = 4;

        let text_plain = Self::strip_ansi_codes(text);
        let (ansi_prefix, ansi_suffix) = Self::extract_ansi_codes(text);
        let available_width = max_line_width.saturating_sub(LEFT_MARGIN).saturating_sub(RIGHT_MARGIN);
        let wrapped_lines_plain = Self::wrap_text_by_words(&text_plain, available_width);

        println!("{}", prefix);

        let left_indent = " ".repeat(LEFT_MARGIN);
        for line in wrapped_lines_plain.iter() {
            println!("{}{}{}{}", left_indent, ansi_prefix, line, ansi_suffix);
        }
    }

    #[doc(hidden)]
    pub fn extract_ansi_codes(s: &str) -> (String, String) {
        let mut prefix = String::new();
        let mut suffix = String::new();
        let mut chars = s.chars().peekable();
        let mut in_ansi = false;
        let mut ansi_seq = String::new();

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
                break;
            }
        }

        if !prefix.is_empty() {
            suffix.push_str("\x1b[0m");
        }

        (prefix, suffix)
    }

    #[doc(hidden)]
    pub fn strip_ansi_codes(s: &str) -> String {
        let mut result = String::new();
        let mut chars = s.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch == '\x1b' {
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

    #[doc(hidden)]
    pub fn wrap_text_by_words(text: &str, width: usize) -> Vec<String> {
        if text.is_empty() {
            return vec![String::new()];
        }

        let mut lines = Vec::new();
        let mut current_line = String::new();

        for word in text.split_whitespace() {
            let word_len = word.chars().count();

            if current_line.is_empty() {
                if word_len <= width {
                    current_line.push_str(word);
                } else {
                    let mut chars: Vec<char> = word.chars().collect();
                    while !chars.is_empty() {
                        let chunk: Vec<char> = chars.drain(..width.min(chars.len())).collect();
                        lines.push(chunk.iter().collect());
                    }
                }
            } else {
                let space_needed = 1 + word_len;
                if current_line.chars().count() + space_needed <= width {
                    current_line.push(' ');
                    current_line.push_str(word);
                } else {
                    lines.push(current_line);
                    current_line = String::new();
                    if word_len <= width {
                        current_line.push_str(word);
                    } else {
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

    pub(crate) fn print_not_found_ids(not_found: &[u8]) {
        if !not_found.is_empty() {
            let list = not_found
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(" ");
            println!("{} {}", "Tasks not found IDs:".yellow(), list);
        }
    }

    pub(crate) fn print_unchanged_task_message(current_text: &str, edited_info: &[(u8, String)]) {
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
}
