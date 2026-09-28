use std::ops::Range;

use anyhow::Result;
use colored::*;

use super::HandlerCLI;
use crate::config::theme;
use crate::model::Task;
use crate::search::Query;

/// Narrowest id column in `rusk list`, matching the "id" header.
const ID_COLUMN_MIN_WIDTH: usize = 2;
/// Everything in a list line except the id: `"  "`, the status marker, `" "`,
/// `"  "`, the 9-column date and `"  "`. Also the indent of wrapped lines.
const LIST_PREFIX_FIXED_WIDTH: usize = 17;
/// Rule under the list header for a two-digit id column.
const LIST_RULE_WIDTH: usize = 46;
/// Columns held by the short date (`14-sep-26`) in a list line; a date of
/// another century (`14-sep-1975`) widens the column.
const DATE_COLUMN_WIDTH: usize = 9;
/// Marks a task the compact view shows only the beginning of.
const SHORTENED: &str = "…";

/// A row of wrapped text: the byte ranges of the words (or of the pieces of
/// a word too long for a row) it shows, printed with one space between them.
type Row = Vec<Range<usize>>;

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

    /// A date as every message prints it, the list's `D-mon-yy`
    /// (`7-jul-26`); "empty" for none.
    #[doc(hidden)]
    pub fn format_date_for_display(date: Option<chrono::NaiveDate>) -> String {
        date.map_or_else(|| "empty".to_string(), Self::short_date)
    }

    /// `D-mon-yy`; a year outside 2000-2099 keeps all four digits, as two
    /// of them would name another century (REVIEW №18).
    fn short_date(date: chrono::NaiveDate) -> String {
        use chrono::Datelike;
        let year = if (2000..=2099).contains(&date.year()) {
            date.format("%y").to_string()
        } else {
            // Four digits, a year below 1000 read from a file too.
            format!("{:04}", date.year())
        };
        format!(
            "{}-{}-{year}",
            date.day(),
            date.format("%b").to_string().to_lowercase()
        )
    }

    /// Short list-style date: `D-mon-yy` (e.g. `7-jul-26`). Overdue dates on
    /// not-done tasks use `date_overdue`, today's use `date_today`, the rest
    /// (and all done tasks) use `date_upcoming`. Defaults reproduce the old
    /// red/cyan behavior (`date_today` == `date_upcoming`).
    pub(crate) fn colored_short_date(date: chrono::NaiveDate, done: bool) -> ColoredString {
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
        color.paint(&Self::short_date(date))
    }

    pub(crate) fn print_task_text_with_wrapping(prefix: &str, text: &str) -> Result<()> {
        Self::print_task_text_with_wrapping_suffixed(prefix, text, None)
    }

    /// Prints a task's `text` in bold under `prefix`, wrapped to the
    /// terminal, and appends `suffix` verbatim after the last line. `text` is
    /// the task's own, unstyled: its control characters are escaped here, so
    /// a task cannot drive the terminal, and each wrapped line is styled on
    /// its own. The suffix keeps its ANSI styling; one that would push the
    /// last line past the terminal width gets a line of its own instead.
    pub(crate) fn print_task_text_with_wrapping_suffixed(
        prefix: &str,
        text: &str,
        suffix: Option<&str>,
    ) -> Result<()> {
        let max_line_width = Self::get_max_line_width();
        const LEFT_MARGIN: usize = 4;
        const RIGHT_MARGIN: usize = 4;

        let available_width = max_line_width
            .saturating_sub(LEFT_MARGIN)
            .saturating_sub(RIGHT_MARGIN);
        let wrapped_lines = Self::wrap_text_by_words(&crate::printable::escape(text), available_width);

        let mut out = format!("{prefix}\n");
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
            out.push_str(&format!("{}{}{}\n", left_indent, line.bold(), tail));
        }
        if let Some(s) = suffix
            && !suffix_fits
        {
            for line in Self::wrap_suffix_alone(s, available_width) {
                out.push_str(&format!("{left_indent}{line}\n"));
            }
        }
        crate::out!("{out}")
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

    /// Punctuation that ends a sentence or a clause, which the compact view
    /// cuts off the end of a line. Quotes and brackets are not in it: a
    /// closing one belongs to an opening one earlier in the line (REVIEW
    /// №122).
    fn is_trailing_sentence_punct(c: char) -> bool {
        matches!(
            c,
            '.' | ',' | ';' | ':' | '!' | '?' | '…'
                | '‐' | '‑' | '‒' | '–' | '—' | '―'
                | '‼' | '⁇' | '⁈' | '⁉'
                | '·'
                | '¿' | '¡'
                // CJK and fullwidth forms
                | '。' | '、' | '｡' | '､'
                | '\u{ff0c}' // Fullwidth comma
                | '\u{ff0e}' // Fullwidth full stop
                | '\u{ff1a}' // Fullwidth colon
                | '\u{ff1b}' // Fullwidth semicolon
                | '\u{ff01}' // Fullwidth exclamation mark
                | '\u{ff1f}' // Fullwidth question mark
                // Arabic
                | '\u{060c}' // Arabic comma
                | '\u{061b}' // Arabic semicolon
                | '\u{061f}' // Arabic question mark
                | '\u{06d4}' // Arabic full stop
                // Small and vertical forms
                | '\u{fe50}'..='\u{fe57}'
                | '\u{fe10}'..='\u{fe16}'
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

    /// Strips trailing sentence punctuation. Used for `list --compact`.
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

    /// Trims end whitespace, then repeatedly removes trailing punctuation and
    /// whitespace (e.g. `"a. "` → `"a"`). A line of nothing but punctuation
    /// (`...`) is left as it is rather than shown as nothing.
    #[doc(hidden)]
    pub fn trim_first_line_for_compact_list(s: &str) -> &str {
        let s = Self::trim_end_display_noise(s);
        let mut trimmed = s;
        loop {
            let next = Self::trim_trailing_punctuation(trimmed);
            let next = Self::trim_end_display_noise(next);
            if next.len() == trimmed.len() {
                break;
            }
            trimmed = next;
        }
        if trimmed.is_empty() { s } else { trimmed }
    }

    /// Rows of `text` laid out in `width` cells: each line of the text is
    /// broken between words, a word wider than a row is cut into pieces
    /// between grapheme clusters, and every line takes at least one row.
    /// Rows hold byte ranges of `text`, so what a search matched in `text`
    /// can be found again in them (see [`Self::paint_rows`]).
    fn wrap_rows(text: &str, width: usize) -> Vec<Row> {
        let mut rows = Vec::new();
        let mut line_start = 0;
        for line in text.split('\n') {
            Self::wrap_line(line, line_start, width, &mut rows);
            line_start += line.len() + 1;
        }
        rows
    }

    fn wrap_line(line: &str, base: usize, width: usize, rows: &mut Vec<Row>) {
        let first_row = rows.len();
        let mut current: Row = Vec::new();
        let mut current_width = 0usize;

        // Split an over-long word into chunks that fit the width, cutting
        // only between grapheme clusters so a flag, a ZWJ sequence or a
        // letter with its accent stays whole. A cluster wider than the whole
        // width still gets a chunk of its own, so a zero (or narrow) width
        // cannot loop forever.
        let push_word_chunks = |rows: &mut Vec<Row>, start: usize, word: &str| {
            let mut offset = 0;
            while offset < word.len() {
                let take = crate::width::prefix_within(&word[offset..], width);
                let piece = start + offset..start + offset + take;
                rows.push(vec![piece]);
                offset += take;
            }
        };

        for (offset, word) in words(line) {
            let start = base + offset;
            let word_width = crate::width::width(word);

            if !current.is_empty() && current_width + 1 + word_width <= width {
                current.push(start..start + word.len());
                current_width += 1 + word_width;
                continue;
            }
            if !current.is_empty() {
                rows.push(std::mem::take(&mut current));
            }
            if word_width <= width {
                current.push(start..start + word.len());
                current_width = word_width;
            } else {
                push_word_chunks(rows, start, word);
                current_width = 0;
            }
        }

        if !current.is_empty() {
            rows.push(current);
        }
        if rows.len() == first_row {
            rows.push(Vec::new());
        }
    }

    /// `row` of `text` as printed: its pieces with one space between them.
    fn row_text(text: &str, row: &[Range<usize>]) -> String {
        let pieces: Vec<&str> = row.iter().map(|range| &text[range.clone()]).collect();
        pieces.join(" ")
    }

    #[doc(hidden)]
    pub fn wrap_text_by_words(text: &str, width: usize) -> Vec<String> {
        Self::wrap_rows(text, width)
            .iter()
            .map(|row| Self::row_text(text, row))
            .collect()
    }

    /// Rows of `text` wrapped to `width` as [`wrap_text_by_words`] lays them
    /// out, with whatever `query` finds in `text` painted in the theme's
    /// `search_match` color (bold). A match is found in the text as it is,
    /// so it is painted wherever its words land: across a row break, in both
    /// pieces of a word cut in two, and over the one space printed for a
    /// run of whitespace (REVIEW №36).
    ///
    /// [`wrap_text_by_words`]: Self::wrap_text_by_words
    #[doc(hidden)]
    pub fn paint_rows(text: &str, width: usize, query: Option<&Query>) -> Vec<String> {
        let rows = Self::wrap_rows(text, width);
        let matches = query.map_or_else(Vec::new, |query| query.find_all(text));
        rows.iter()
            .map(|row| {
                if matches.is_empty() {
                    Self::row_text(text, row)
                } else {
                    Self::paint_row(text, row, &matches)
                }
            })
            .collect()
    }

    fn paint_row(text: &str, row: &[Range<usize>], matches: &[Range<usize>]) -> String {
        // Runs of the row, each wholly inside a match or wholly outside:
        // neighbors in the same state are merged, so a phrase is painted as
        // one span.
        let mut runs: Vec<(String, bool)> = Vec::new();
        let mut push = |piece: &str, lit: bool| match runs.last_mut() {
            Some((run, run_lit)) if *run_lit == lit => run.push_str(piece),
            _ => runs.push((piece.to_string(), lit)),
        };
        // The matches are in order and do not overlap: the first one that
        // ends after `at` is the one `at` is in, or else the next to come.
        // Looked up by bisection, so painting takes time in proportion to
        // the text, however many matches it holds.
        let from = |at: usize| matches.partition_point(|m| m.end <= at);
        for (i, range) in row.iter().enumerate() {
            if i > 0 {
                // The space stands for the whitespace between two words: lit
                // when a match runs on across it.
                let gap = row[i - 1].end..range.start;
                let lit = matches
                    .get(from(gap.start))
                    .is_some_and(|m| m.start < gap.start && gap.end < m.end);
                push(" ", lit);
            }
            let mut at = range.start;
            while at < range.end {
                // Up to the next boundary of a match, or the end of the piece.
                let (lit, next) = match matches.get(from(at)) {
                    Some(m) if m.start <= at => (true, m.end),
                    Some(m) => (false, m.start),
                    None => (false, range.end),
                };
                let next = next.min(range.end);
                push(&text[at..next], lit);
                at = next;
            }
        }
        runs.into_iter()
            .map(|(run, lit)| {
                if lit {
                    theme().search_match.paint(&run).bold().to_string()
                } else {
                    run
                }
            })
            .collect()
    }

    /// The one row the compact view shows of `text` (escaped) in `width`
    /// cells: the first row of its first line, trailing sentence punctuation
    /// cut. When that is not the whole task — the line goes on in further
    /// rows, or more lines follow — it ends in `…`, and the row is made one
    /// cell narrower to keep room for it (REVIEW №169).
    #[doc(hidden)]
    pub fn compact_row(text: &str, width: usize) -> String {
        let (row, shortened) = Self::compact_row_parts(text, width);
        if shortened { format!("{row}{SHORTENED}") } else { row }
    }

    /// [`compact_row`](Self::compact_row) without its `…`, and whether it
    /// takes one: the list colors a leading keyword of the row first.
    fn compact_row_parts(text: &str, width: usize) -> (String, bool) {
        let (first, rest) = text.split_once('\n').unwrap_or((text, ""));
        let first = Self::trim_first_line_for_compact_list(first);
        let rows = Self::wrap_text_by_words(first, width);
        let shortened = rows.len() > 1 || !rest.trim().is_empty();
        if !shortened {
            return (rows.into_iter().next().unwrap_or_default(), false);
        }
        let rows = Self::wrap_text_by_words(first, width.saturating_sub(1));
        let row = rows.first().map_or("", |row| Self::trim_first_line_for_compact_list(row));
        (row.to_string(), true)
    }

    /// `tasks` as `rusk list` prints them to a terminal `max_line_width`
    /// cells wide: a header, a row per task line, and the text wrapped under
    /// its column. `query` paints what it finds (search); `compact` shows one
    /// row per task.
    pub(crate) fn format_task_list(
        tasks: &[&Task],
        compact: bool,
        query: Option<&Query>,
        max_line_width: usize,
    ) -> String {
        if tasks.is_empty() {
            return format!("{}\n", theme().warning.paint("No tasks"));
        }

        // The id column is as wide as the widest id on screen (never narrower
        // than the two digits the header needs), so three- and four-digit ids
        // keep the date and the text where the header promises them and the
        // continuation indent below matches the first line.
        let id_width = tasks
            .iter()
            .map(|t| t.id.to_string().len())
            .max()
            .unwrap_or(ID_COLUMN_MIN_WIDTH)
            .max(ID_COLUMN_MIN_WIDTH);
        let id_pad = " ".repeat(id_width - ID_COLUMN_MIN_WIDTH);
        // So is the date column as wide as the widest date: one of another
        // century shows all four digits of its year (REVIEW №18), and the
        // text after it must not move.
        let date_width = tasks
            .iter()
            .filter_map(|t| t.date)
            .map(|d| Self::short_date(d).chars().count())
            .max()
            .unwrap_or(DATE_COLUMN_WIDTH)
            .max(DATE_COLUMN_WIDTH);
        let date_pad = " ".repeat(date_width - DATE_COLUMN_WIDTH);

        let mut out = format!(
            "\n  #  {}{}    {}{}       {}\n",
            id_pad,
            theme().list_header.paint("id"),
            date_pad,
            theme().list_header.paint("date"),
            theme().list_header.paint("task")
        );
        let rule = (LIST_RULE_WIDTH + id_width - ID_COLUMN_MIN_WIDTH + date_width - DATE_COLUMN_WIDTH)
            .min(max_line_width.saturating_sub(2));
        out.push_str(&format!("  {}\n", "─".repeat(rule)));

        // "  " + status + " " + id + "  " + date + "  "
        let prefix_width = LIST_PREFIX_FIXED_WIDTH + id_width + date_width - DATE_COLUMN_WIDTH;
        let indent = " ".repeat(prefix_width);
        let available_width = max_line_width
            .saturating_sub(prefix_width)
            .saturating_sub(4);

        for task in tasks {
            let status = if task.done {
                theme().done_marker.paint("✔")
            } else if task.priority {
                theme().priority_marker.paint("p").bold()
            } else {
                "•".normal()
            };

            let date_colored = task
                .date
                .map(|d| Self::colored_short_date(d, task.done))
                .unwrap_or_else(|| "".normal());

            // Escaped before anything measures or highlights it: the text is
            // data, and a control character in it must not reach the terminal.
            let shown = crate::printable::escape(&task.text);

            // Dependencies land right after the text (ids bold, parens not):
            // on the single shown line in compact mode, otherwise after the
            // last line. Compact mode shows one line only, so the note takes
            // its room out of the wrap budget instead of running past the
            // terminal width.
            let after_note = Self::after_suffix(task).map(|s| format!(" {s}"));
            let after_width = after_note.as_deref().map_or(0, Self::display_width);
            // A note worth more than half the line would leave a stub of a
            // task instead of a compact line: then it goes below, like in the
            // full view, rather than eating the text.
            let wrap_width = if compact && after_width * 2 <= available_width {
                available_width - after_width
            } else {
                available_width
            };
            // A keyword at the head of the text is colored on the first row,
            // before the compact row gets its `…`.
            let rows = if compact {
                let (row, shortened) = Self::compact_row_parts(&shown, wrap_width);
                let mark = if shortened { SHORTENED } else { "" };
                vec![format!("{}{mark}", Self::highlight_keywords(&row))]
            } else {
                let mut rows = Self::paint_rows(&shown, wrap_width, query);
                rows[0] = Self::highlight_keywords(&rows[0]).into_owned();
                rows
            };

            // Done tasks get the id in the marker color too, matching the ✔.
            let id_theme = if task.done {
                theme().done_marker
            } else {
                theme().task_id
            };

            // A note that does not fit after the text gets a line of its own.
            let last_row = rows.len() - 1;
            let note_fits = Self::display_width(&rows[last_row]) + after_width <= available_width;
            let note_on = |i: usize| -> &str {
                match &after_note {
                    Some(s) if note_fits && i == last_row => s.as_str(),
                    _ => "",
                }
            };

            // Columns are padded by hand: a styled cell carries invisible
            // escape bytes, which `{:>width$}` would count as characters.
            let id_txt = task.id.to_string();
            let id_lead = " ".repeat(id_width - id_txt.len());
            let date_txt = date_colored.to_string();
            let date_lead = " ".repeat(date_width.saturating_sub(Self::display_width(&date_txt)));

            out.push_str(&format!(
                "  {} {}{}  {}{}  {}{}\n",
                status,
                id_lead,
                id_theme.paint(&id_txt).bold(),
                date_lead,
                date_txt,
                rows[0],
                note_on(0)
            ));
            for (i, row) in rows.iter().enumerate().skip(1) {
                out.push_str(&format!("{indent}{row}{}\n", note_on(i)));
            }

            if let Some(note) = &after_note
                && !note_fits
            {
                for line in Self::wrap_suffix_alone(note, available_width) {
                    out.push_str(&format!("{indent}{line}\n"));
                }
            }
        }

        out.push_str("\n\n");
        out
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

    pub(crate) fn print_not_found_ids(not_found: &[crate::model::TaskId]) -> Result<()> {
        if not_found.is_empty() {
            return Ok(());
        }
        let list = not_found
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        crate::outln!("{} {}", theme().warning.paint("Tasks not found IDs:"), list)
    }
}

/// The words of `line` (runs of non-whitespace) with their byte offsets.
fn words(line: &str) -> impl Iterator<Item = (usize, &str)> {
    line.split(char::is_whitespace)
        .scan(0usize, |offset, word| {
            let start = *offset;
            *offset += word.len();
            // The separator after it: one whitespace char (its length).
            if let Some(sep) = line[*offset..].chars().next() {
                *offset += sep.len_utf8();
            }
            Some((start, word))
        })
        .filter(|(_, word)| !word.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(text: &str, width: usize) -> Vec<String> {
        HandlerCLI::wrap_text_by_words(text, width)
    }

    #[test]
    fn words_come_with_their_offsets() {
        let line = " a  bb\tc ";
        let found: Vec<(usize, &str)> = words(line).collect();
        assert_eq!(found, [(1, "a"), (4, "bb"), (7, "c")]);
        for (offset, word) in found {
            assert_eq!(&line[offset..offset + word.len()], word);
        }
        assert_eq!(words("é ü").collect::<Vec<_>>(), [(0, "é"), (3, "ü")]);
        assert_eq!(words("").count(), 0);
    }

    #[test]
    fn rows_are_ranges_of_the_text() {
        let text = "one  two\nthree four five\n\nsix";
        let wrapped = HandlerCLI::wrap_rows(text, 10);
        let shown: Vec<Vec<&str>> = wrapped
            .iter()
            .map(|row| row.iter().map(|r| &text[r.clone()]).collect())
            .collect();
        assert_eq!(
            shown,
            [vec!["one", "two"], vec!["three", "four"], vec!["five"], vec![], vec!["six"]]
        );
        assert_eq!(rows(text, 10), ["one two", "three four", "five", "", "six"]);
    }

    #[test]
    fn a_long_word_is_cut_between_clusters_into_rows_of_its_own() {
        assert_eq!(rows("ab abcdefgh cd", 4), ["ab", "abcd", "efgh", "cd"]);
        let text = format!("x {}", "日本".repeat(3));
        // Two cells a character: three fit in a row of 7.
        assert_eq!(rows(&text, 7), ["x", "日本日", "本日本"]);
        // A zero width cannot loop forever.
        assert_eq!(rows("abc", 0), ["a", "b", "c"]);
    }

    /// REVIEW №124: widths in cells, not in chars.
    #[test]
    fn rows_are_measured_in_cells() {
        for text in ["日本語のテキスト ".repeat(9), "🍎🍎🍎 ".repeat(20), "e\u{301}".repeat(40)] {
            for row in rows(&text, 20) {
                assert!(crate::width::width(&row) <= 20, "{row:?}");
            }
        }
    }

    #[test]
    fn a_match_is_painted_wherever_its_words_land() {
        let _colors = crate::cli::tests::force_colors();
        let text = format!("{} alpha beta gamma", "x".repeat(10));
        let query = Query::new("ALPHA  beta");
        let painted = HandlerCLI::paint_rows(&text, 16, Some(&query));
        let lit = |row: &str| -> String {
            let styled = theme().search_match.paint("\u{1}").bold().to_string();
            let (open, close) = styled.split_once('\u{1}').unwrap();
            row.split(open)
                .skip(1)
                .map(|part| part.split(close).next().unwrap().to_string())
                .collect::<Vec<_>>()
                .join("|")
        };
        // "xxxxxxxxxx alpha" fills the first row; "beta gamma" the second.
        assert_eq!(painted.iter().map(|r| lit(r)).collect::<Vec<_>>(), ["alpha", "beta"]);

        // One space printed for a run: lit as part of the phrase.
        let text = "double  space alpha  beta here";
        let painted = HandlerCLI::paint_rows(text, 80, Some(&Query::new("alpha beta")));
        assert_eq!(lit(&painted[0]), "alpha beta");
        assert_eq!(HandlerCLI::strip_ansi_codes(&painted[0]), "double space alpha beta here");

        // A word cut in two: both pieces carry their part of the match.
        let text = format!("{}needle{}", "a".repeat(8), "b".repeat(4));
        let painted = HandlerCLI::paint_rows(&text, 10, Some(&Query::new("needle")));
        assert_eq!(painted.iter().map(|r| lit(r)).collect::<Vec<_>>(), ["ne", "edle"]);

        // Every match of a row, and nothing painted without one.
        let painted = HandlerCLI::paint_rows("foo bar Foo", 80, Some(&Query::new("foo")));
        assert_eq!(lit(&painted[0]), "foo|Foo");
        assert_eq!(HandlerCLI::paint_rows("foo bar", 80, Some(&Query::new("baz"))), ["foo bar"]);
    }

    /// REVIEW section 4: every piece of a row looked through all the
    /// matches, so a text with many took time quadratic in their number (a
    /// word of 2 000 000 `h`s searched for `hh`: over a minute).
    #[test]
    fn many_matches_are_painted_in_time() {
        let _colors = crate::cli::tests::force_colors();
        let text = "h".repeat(200_000);
        let started = std::time::Instant::now();
        let painted = HandlerCLI::paint_rows(&text, 80, Some(&Query::new("hh")));
        assert!(started.elapsed() < std::time::Duration::from_secs(10), "{:?}", started.elapsed());
        assert_eq!(painted.len(), 2_500);
        // The matches tile the word: each row is one painted run.
        let styled = theme().search_match.paint(&"h".repeat(80)).bold().to_string();
        assert!(painted.iter().all(|row| *row == styled));
    }

    /// REVIEW №122: quotes are not sentence punctuation; CJK ends are.
    #[test]
    fn the_compact_line_loses_sentence_ends_only() {
        for (text, shown) in [
            ("Read the book \"Dune\"", "Read the book \"Dune\""),
            ("Read «Le Petit Prince».", "Read «Le Petit Prince»"),
            ("He said 'go'.", "He said 'go'"),
            ("...", "..."),
            ("!? ", "!?"),
            ("Buy milk.", "Buy milk"),
            ("牛乳を買う。", "牛乳を買う"),
            ("本当に？", "本当に"),
            ("注意：", "注意"),
            ("سلام،", "سلام"),
            ("wait — ", "wait"),
        ] {
            assert_eq!(HandlerCLI::trim_first_line_for_compact_list(text), shown, "{text:?}");
        }
    }

    /// REVIEW №169: a compact line that is not the whole task says so.
    #[test]
    fn a_shortened_compact_line_ends_in_an_ellipsis() {
        assert_eq!(HandlerCLI::compact_row("whole.", 20), "whole");
        assert_eq!(HandlerCLI::compact_row("short first\nsecond", 20), "short first…");
        assert_eq!(HandlerCLI::compact_row("Step one:\nstep two", 20), "Step one…");
        // Blank lines after the text are no hidden text.
        assert_eq!(HandlerCLI::compact_row("done\n  \n", 20), "done");
        let long = "one two three four five six";
        let row = HandlerCLI::compact_row(long, 12);
        assert_eq!(row, "one two…");
        assert!(crate::width::width(&row) <= 12);
        // The mark takes a cell of its own even when the row is full.
        assert_eq!(HandlerCLI::compact_row("abcdefgh ij", 8), "abcdefg…");
        assert_eq!(HandlerCLI::compact_row("one, two", 5), "one…");
    }

    /// Review of R19: `FIXME…` is no keyword, so the `…` of a shortened
    /// compact row took the keyword's color away.
    #[test]
    fn a_shortened_compact_row_keeps_its_keyword_color() {
        let _colors = crate::cli::tests::force_colors();
        let tasks = [task(1, "FIXME\ncheck the logs")];
        let refs: Vec<&Task> = tasks.iter().collect();
        let listing = HandlerCLI::format_task_list(&refs, true, None, 80);
        let keyword = theme().keyword.paint("FIXME").to_string();
        assert!(listing.contains(&format!("{keyword}…")), "{listing:?}");
    }

    #[test]
    fn dates_are_printed_the_way_the_list_prints_them() {
        let date = chrono::NaiveDate::from_ymd_opt(2027, 1, 1);
        assert_eq!(HandlerCLI::format_date_for_display(date), "1-jan-27");
        assert_eq!(HandlerCLI::format_date_for_display(None), "empty");
        // Another century keeps its four digits.
        let date = chrono::NaiveDate::from_ymd_opt(1975, 5, 4);
        assert_eq!(HandlerCLI::format_date_for_display(date), "4-may-1975");
        let date = chrono::NaiveDate::from_ymd_opt(100, 1, 1);
        assert_eq!(HandlerCLI::format_date_for_display(date), "1-jan-0100");
    }

    /// Review of R25: a date of another century is wider than the date
    /// column, and pushed its text out of line with the others.
    #[test]
    fn a_wide_date_widens_its_column_not_the_row() {
        let mut old = task(1, "old one");
        old.date = chrono::NaiveDate::from_ymd_opt(1975, 12, 24);
        let mut new = task(2, "new one");
        new.date = chrono::NaiveDate::from_ymd_opt(2027, 1, 1);
        let plain = task(3, "no date\nsecond line");
        let refs = [&old, &new, &plain];
        let listing = HandlerCLI::strip_ansi_codes(&HandlerCLI::format_task_list(&refs, false, None, 80));
        let column = |needle: &str| {
            let line = listing.lines().find(|l| l.contains(needle)).unwrap();
            line[..line.find(needle).unwrap()].chars().count()
        };
        let at = column("old one");
        assert_eq!(column("new one"), at, "{listing}");
        assert_eq!(column("no date"), at, "{listing}");
        assert_eq!(column("second line"), at, "{listing}");
        // The header's `task` moves with the text.
        assert_eq!(column("task") - at, 3, "{listing}");
        let usual = [&new, &plain];
        let listing = HandlerCLI::strip_ansi_codes(&HandlerCLI::format_task_list(&usual, false, None, 80));
        let at = |needle: &str| {
            let line = listing.lines().find(|l| l.contains(needle)).unwrap();
            line[..line.find(needle).unwrap()].chars().count()
        };
        assert_eq!(at("task") - at("new one"), 3, "{listing}");
    }

    fn task(id: crate::model::TaskId, text: &str) -> Task {
        Task {
            id,
            text: text.to_string(),
            date: None,
            done: false,
            priority: false,
            after: Vec::new(),
        }
    }

    /// REVIEW №124: ids past two digits keep the columns lined up, and the
    /// continuation rows start under the text.
    #[test]
    fn the_list_lines_up_under_its_header() {
        let tasks = [task(7, "short"), task(1234, &"word ".repeat(30))];
        let refs: Vec<&Task> = tasks.iter().collect();
        let listing = HandlerCLI::strip_ansi_codes(&HandlerCLI::format_task_list(&refs, false, None, 60));
        // Header, rule, "short", the first and a further row of the long one.
        let lines: Vec<&str> = listing.lines().filter(|l| !l.trim().is_empty()).collect();
        let text_column = |line: &str| line.chars().position(char::is_alphabetic).unwrap();
        assert_eq!(text_column(lines[2]), text_column(lines[3]), "{listing}");
        assert_eq!(text_column(lines[3]), text_column(lines[4]), "{listing}");
        assert!(lines[2].contains("   7  "), "the id is right-aligned: {listing}");
        for line in &lines {
            assert!(crate::width::width(line) <= 60, "{line:?}");
        }
    }
}
