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

    /// Prints a task's `text` in bold under `prefix`, wrapped to the
    /// terminal, and appends `suffix` verbatim after the last line. `text` is
    /// the task's own, unstyled: its control characters are escaped here, so
    /// a task cannot drive the terminal, and each wrapped line is styled on
    /// its own. The suffix keeps its ANSI styling; one that would push the
    /// last line past the terminal width gets a line of its own instead.
    pub(crate) fn print_task_text_with_wrapping_suffixed(prefix: &str, text: &str, suffix: Option<&str>) {
        let max_line_width = Self::get_max_line_width();
        const LEFT_MARGIN: usize = 4;
        const RIGHT_MARGIN: usize = 4;

        let available_width = max_line_width
            .saturating_sub(LEFT_MARGIN)
            .saturating_sub(RIGHT_MARGIN);
        let wrapped_lines = Self::wrap_text_by_words(&crate::printable::escape(text), available_width);

        println!("{}", prefix);

        let left_indent = " ".repeat(LEFT_MARGIN);
        let last = wrapped_lines.len().saturating_sub(1);
        let last_line_width = wrapped_lines.last().map_or(0, |l| crate::width::width(l));
        let suffix_fits =
            last_line_width + suffix.map_or(0, Self::display_width) <= available_width;
        for (i, line) in wrapped_lines.iter().enumerate() {
            let tail = match suffix {
                Some(s) if i == last && suffix_fits => s,
                _ => "",
            };
            println!("{}{}{}", left_indent, line.bold(), tail);
        }
        if let Some(s) = suffix
            && !suffix_fits
        {
            for line in Self::wrap_suffix_alone(s, available_width) {
                println!("{}{}", left_indent, line);
            }
        }
    }

    /// Width of styled text in terminal cells: ANSI codes take no cells.
    #[doc(hidden)]
    pub fn display_width(s: &str) -> usize {
        crate::width::width(&Self::strip_ansi_codes(s))
    }

    /// The dependency note `(1,2,…)` laid out on lines of its own, for when it
    /// does not fit after the task text. Its own line is no free pass past the
    /// width: a long list of ids is wrapped like any other text, at the commas
    /// so the ids stay readable. The styling of the ids is dropped — wrapping
    /// cannot carry ANSI codes through.
    #[doc(hidden)]
    pub fn wrap_suffix_alone(suffix: &str, width: usize) -> Vec<String> {
        let plain = Self::strip_ansi_codes(suffix);
        let plain = plain.trim_start();
        if crate::width::width(plain) <= width {
            return vec![plain.to_string()];
        }
        // Too long even on a line of its own: break it at the commas, where
        // an id stays readable, rather than through the digits.
        Self::wrap_text_by_words(&plain.replace(',', ", "), width)
    }

    /// `s` without the ANSI sequences rusk styles its own output with, so
    /// it can be measured. A sequence ends where the terminal ends it: a CSI
    /// (`ESC [`) at its final byte (`@`..`~`), an OSC (`ESC ]`) at BEL or
    /// ST, any other escape after one character. Task text is escaped before
    /// it is styled (`printable::escape`) and holds no ESC by then.
    #[doc(hidden)]
    pub fn strip_ansi_codes(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        let mut chars = s.chars().peekable();

        while let Some(ch) = chars.next() {
            if ch != '\x1b' {
                result.push(ch);
                continue;
            }
            match chars.next() {
                Some('[') => {
                    for c in chars.by_ref() {
                        if ('@'..='~').contains(&c) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    while let Some(c) = chars.next() {
                        if c == '\x07' {
                            break;
                        }
                        if c == '\x1b' && chars.next_if_eq(&'\\').is_some() {
                            break;
                        }
                    }
                }
                _ => {}
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
        let mut current_width = 0usize;

        // Split an over-long word into chunks that fit the width, cutting
        // only between grapheme clusters so a flag, a ZWJ sequence or a
        // letter with its accent stays whole. A cluster wider than the whole
        // width still gets a chunk of its own, so a zero (or narrow) width
        // cannot loop forever.
        let push_word_chunks = |lines: &mut Vec<String>, word: &str| {
            let mut rest = word;
            while !rest.is_empty() {
                let take = crate::width::prefix_within(rest, width);
                lines.push(rest[..take].to_string());
                rest = &rest[take..];
            }
        };

        for word in text.split_whitespace() {
            let word_width = crate::width::width(word);

            if current_line.is_empty() {
                if word_width <= width {
                    current_line.push_str(word);
                    current_width = word_width;
                } else {
                    push_word_chunks(&mut lines, word);
                }
            } else if current_width + 1 + word_width <= width {
                current_line.push(' ');
                current_line.push_str(word);
                current_width += 1 + word_width;
            } else {
                lines.push(std::mem::take(&mut current_line));
                current_width = 0;
                if word_width <= width {
                    current_line.push_str(word);
                    current_width = word_width;
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
