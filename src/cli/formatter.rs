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
        if w == 0 {
            DEFAULT_WIDTH
        } else {
            w.min(DEFAULT_WIDTH)
        }
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
        let available_width = max_line_width
            .saturating_sub(LEFT_MARGIN)
            .saturating_sub(RIGHT_MARGIN);
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

    fn is_trailing_sentence_punct(c: char) -> bool {
        matches!(
            c,
            '.' | ',' | ';' | ':' | '!' | '?' | '…'
                | '"' | '\''
                | '«' | '»' | '„' | '“' | '”' | '‚' | '‘' | '’'
                | '‹' | '›'
                | '‐' | '‑' | '‒' | '–' | '—' | '―'
                | '‼' | '⁇' | '⁈' | '⁉'
                | '·'
                | '¿' | '¡'
                // Typographic / locale comma variants (not U+002C)
                | '\u{060c}' // Arabic comma
                | '\u{ff0c}' // Fullwidth comma
                | '\u{fe50}' // Small comma
                | '\u{fe10}' // Presentation form for vertical comma
                | '\u{2e41}' // Reversed comma
        )
    }

    /// Trims whitespace and invisible trailing marks so punctuation before them is reachable.
    fn trim_end_display_noise(s: &str) -> &str {
        let mut end = s.len();
        while end > 0 {
            let slice = &s[..end];
            let Some(ch) = slice.chars().next_back() else {
                break;
            };
            if ch.is_whitespace()
                || matches!(
                    ch,
                    '\u{200b}' // ZWSP
                        | '\u{200c}' // ZWNJ
                        | '\u{200d}' // ZWJ
                        | '\u{2060}' // Word joiner
                        | '\u{feff}' // BOM / ZWNBSP
                )
            {
                end -= ch.len_utf8();
            } else {
                break;
            }
        }
        &s[..end]
    }

    /// Strips trailing sentence punctuation (and matching quotes/dashes). Used for `list --compact`.
    #[doc(hidden)]
    pub fn trim_trailing_punctuation(s: &str) -> &str {
        let mut end = s.len();
        while end > 0 {
            let slice = &s[..end];
            let Some(ch) = slice.chars().next_back() else {
                break;
            };
            if Self::is_trailing_sentence_punct(ch) {
                end -= ch.len_utf8();
            } else {
                break;
            }
        }
        &s[..end]
    }

    /// Trims end whitespace, then repeatedly removes trailing punctuation and whitespace (e.g. `"a. "` → `"a"`).
    #[doc(hidden)]
    pub fn trim_first_line_for_compact_list(s: &str) -> &str {
        let mut s = Self::trim_end_display_noise(s);
        loop {
            let next = Self::trim_trailing_punctuation(s);
            let next = Self::trim_end_display_noise(next);
            if next.len() == s.len() {
                return s;
            }
            s = next;
        }
    }

    #[doc(hidden)]
    pub fn wrap_text_by_words(text: &str, width: usize) -> Vec<String> {
        if text.is_empty() {
            return vec![String::new()];
        }

        // Preserve hard line breaks from the input, then word-wrap each source line.
        let has_hard_breaks = text.contains('\n');
        if has_hard_breaks {
            let mut out: Vec<String> = Vec::new();
            for src_line in text.split('\n') {
                let wrapped = Self::wrap_single_line_by_words(src_line, width);
                out.extend(wrapped);
            }
            if out.is_empty() {
                vec![String::new()]
            } else {
                out
            }
        } else {
            Self::wrap_single_line_by_words(text, width)
        }
    }

    fn wrap_single_line_by_words(text: &str, width: usize) -> Vec<String> {
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
}
