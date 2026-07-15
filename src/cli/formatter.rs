use colored::*;

use super::HandlerCLI;
use crate::config::theme;

impl HandlerCLI {
    #[doc(hidden)]
    pub fn get_max_line_width() -> usize {
        #[cfg(feature = "interactive")]
        let raw = crossterm::terminal::size().map_or(0, |(width, _)| width);
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

    /// Short list-style date: `D-mon-yy` (e.g. `7-jul-26`). Overdue dates on
    /// not-done tasks use `date_overdue`, today's use `date_today`, the rest
    /// (and all done tasks) use `date_upcoming`. Defaults reproduce the old
    /// red/cyan behavior (`date_today` == `date_upcoming`).
    pub(crate) fn colored_short_date(date: chrono::NaiveDate, done: bool) -> ColoredString {
        use chrono::Datelike;
        let date_str = format!(
            "{}-{}-{}",
            date.day(),
            date.format("%b").to_string().to_lowercase(),
            date.format("%y")
        );
        let today = chrono::Local::now().date_naive();
        let color = if done {
            theme().date_upcoming
        } else if date < today {
            theme().date_overdue
        } else if date == today {
            theme().date_today
        } else {
            theme().date_upcoming
        };
        color.paint(&date_str)
    }

    pub(crate) fn print_task_text_with_wrapping(prefix: &str, text: &str) {
        Self::print_task_text_with_wrapping_suffixed(prefix, text, None);
    }

    /// Like `print_task_text_with_wrapping`, but appends `suffix` verbatim
    /// after the last wrapped line. The suffix keeps its own ANSI styling
    /// (wrapping strips inner codes from `text`, so it can't live there).
    pub(crate) fn print_task_text_with_wrapping_suffixed(prefix: &str, text: &str, suffix: Option<&str>) {
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
        let last = wrapped_lines_plain.len().saturating_sub(1);
        for (i, line) in wrapped_lines_plain.iter().enumerate() {
            let tail = if i == last { suffix.unwrap_or("") } else { "" };
            println!("{}{}{}{}{}", left_indent, ansi_prefix, line, ansi_suffix, tail);
        }
    }

    #[doc(hidden)]
    pub fn extract_ansi_codes(s: &str) -> (String, String) {
        let mut prefix = String::new();
        let mut chars = s.chars().peekable();

        // Collect leading complete ANSI sequences (ESC ... 'm'); an unterminated
        // sequence is dropped, matching the previous behavior.
        while chars.peek() == Some(&'\x1b') {
            let mut ansi_seq = String::new();
            for ch in chars.by_ref() {
                ansi_seq.push(ch);
                if ch == 'm' {
                    prefix.push_str(&ansi_seq);
                    break;
                }
            }
            if !ansi_seq.ends_with('m') {
                break;
            }
        }

        let suffix = if prefix.is_empty() {
            String::new()
        } else {
            "\x1b[0m".to_string()
        };

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

        // Split an over-long word into width-sized chunks. Take at least one
        // char per chunk so a zero width cannot loop forever.
        let push_word_chunks = |lines: &mut Vec<String>, word: &str| {
            let mut chars: Vec<char> = word.chars().collect();
            while !chars.is_empty() {
                let take = width.min(chars.len()).max(1);
                lines.push(chars.drain(..take).collect());
            }
        };

        for word in text.split_whitespace() {
            let word_len = word.chars().count();

            if current_line.is_empty() {
                if word_len <= width {
                    current_line.push_str(word);
                } else {
                    push_word_chunks(&mut lines, word);
                }
            } else if current_line.chars().count() + 1 + word_len <= width {
                current_line.push(' ');
                current_line.push_str(word);
            } else {
                lines.push(std::mem::take(&mut current_line));
                if word_len <= width {
                    current_line.push_str(word);
                } else {
                    push_word_chunks(&mut lines, word);
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

    /// Finds the next case-insensitive occurrence of `needle` (pre-lowercased
    /// chars) in `haystack` starting at byte offset `from` (must be a char
    /// boundary); returns the byte range of the match in `haystack`.
    #[doc(hidden)]
    pub fn find_ci(haystack: &str, needle: &[char], from: usize) -> Option<(usize, usize)> {
        if needle.is_empty() {
            return None;
        }
        'starts: for (offset, _) in haystack[from..].char_indices() {
            let start = from + offset;
            let mut matched = 0;
            let mut end = start;
            for c in haystack[start..].chars() {
                // A single char may lowercase to several (e.g. İ); a match may
                // not end in the middle of such an expansion.
                for lc in c.to_lowercase() {
                    if matched >= needle.len() || lc != needle[matched] {
                        continue 'starts;
                    }
                    matched += 1;
                }
                end += c.len_utf8();
                if matched == needle.len() {
                    return Some((start, end));
                }
            }
        }
        None
    }

    /// Wraps every case-insensitive occurrence of `needle` in the theme's
    /// `search_match` color (bold), leaving the rest of the line untouched.
    #[doc(hidden)]
    pub fn highlight_matches(line: &str, needle: &[char]) -> String {
        let mut out = String::new();
        let mut pos = 0;
        while let Some((start, end)) = Self::find_ci(line, needle, pos) {
            out.push_str(&line[pos..start]);
            out.push_str(
                &theme()
                    .search_match
                    .paint(&line[start..end])
                    .bold()
                    .to_string(),
            );
            pos = end;
        }
        out.push_str(&line[pos..]);
        out
    }

    pub(crate) fn maybe_highlight<'a>(
        line: &'a str,
        needle: Option<&[char]>,
    ) -> std::borrow::Cow<'a, str> {
        match needle {
            Some(n) => std::borrow::Cow::Owned(Self::highlight_matches(line, n)),
            None => std::borrow::Cow::Borrowed(line),
        }
    }

    /// Colors the leading keyword of a task with the `keyword` theme color.
    /// Only the first whitespace-delimited word is considered, and only an
    /// exact match against the config `keywords` list (default
    /// `TEMP INFO FIXME WIP`) counts — "TEMPO" stays untouched.
    #[doc(hidden)]
    pub fn highlight_keywords(line: &str) -> std::borrow::Cow<'_, str> {
        let end = line
            .find(char::is_whitespace)
            .unwrap_or(line.len());
        let token = &line[..end];
        let keywords = &crate::config::config().keywords;
        if token.is_empty() || !keywords.iter().any(|keyword| keyword == token) {
            return std::borrow::Cow::Borrowed(line);
        }
        std::borrow::Cow::Owned(format!(
            "{}{}",
            theme().keyword.paint(token),
            &line[end..]
        ))
    }

    pub(crate) fn print_not_found_ids(not_found: &[crate::model::TaskId]) {
        if !not_found.is_empty() {
            let list = not_found
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(" ");
            println!("{} {}", theme().warning.paint("Tasks not found IDs:"), list);
        }
    }
}
