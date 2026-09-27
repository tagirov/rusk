//! todo.txt database format (`tasks.txt`), feature `fmt-todotxt`.
//! Spec: <https://github.com/todotxt/todo.txt>.
//!
//! One task per line, readable by the todo.txt ecosystem
//! (todo.sh, Simpletask, …):
//!
//! ```text
//! x write the report due:2026-07-15 id:3
//! (A) call mom +family id:1
//! ```
//!
//! Mapping: `x ` prefix = done; any `(A)`–`(Z)` priority = the priority
//! flag (written back as `(A)`); a `due:YYYY-MM-DD` token = the due date;
//! an `id:N` token pins the rusk id (assigned on load when missing);
//! an `after:A,B` token lists task ids this task depends on.
//! Projects/contexts/other `key:value` tokens are kept as part of the task
//! text. Completion/creation dates at the head of a line are accepted but
//! not preserved. Newlines inside task text are stored as a literal `\n`
//! (with `\\` escaping a backslash) — nonstandard, but round-trips.
//!
//! A word of the task text that would be read as metadata — `x`, a
//! priority or a date as the first word, a valid `due:`, `id:` or `after:`
//! tag anywhere — is written with a backslash in front (`\x marks the
//! spot`, `ticket \id:42 needs review`), which decoding takes off again: a
//! text comes back as it was, whatever it looks like. A line of metadata
//! only (`x (A) id:7`) has no text; what becomes of it is up to
//! [`crate::model::normalize`].

use crate::model::{Task, TaskId};
use anyhow::Result;
use chrono::NaiveDate;

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\").replace('\n', "\\n")
}

fn unescape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('\\') => out.push('\\'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

pub fn encode(tasks: &[Task]) -> String {
    let mut out = String::new();
    for task in tasks {
        if task.done {
            out.push_str("x ");
        }
        if task.priority {
            out.push_str("(A) ");
        }
        out.push_str(&protect(&escape(&task.text)));
        if let Some(date) = task.date {
            out.push_str(&format!(" due:{date}"));
        }
        if !task.after.is_empty() {
            let after = task
                .after
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(",");
            out.push_str(&format!(" after:{after}"));
        }
        out.push_str(&format!(" id:{}\n", task.id));
    }
    out
}

fn is_priority_token(token: &str) -> bool {
    let bytes = token.as_bytes();
    bytes.len() == 3
        && bytes[0] == b'('
        && bytes[1].is_ascii_uppercase()
        && bytes[2] == b')'
}

fn is_date_token(token: &str) -> bool {
    token.parse::<NaiveDate>().is_ok()
}

/// Comma-separated ids after `after:`; any invalid part rejects the whole
/// token so it stays in the text (like an invalid `due:`).
fn parse_after_list(list: &str) -> Option<Vec<TaskId>> {
    list.split(',')
        .map(|part| match part.trim().parse::<TaskId>() {
            Ok(n) if n != 0 => Some(n),
            _ => None,
        })
        .collect()
}

fn due_tag(token: &str) -> Option<NaiveDate> {
    token.strip_prefix("due:")?.parse().ok()
}

fn id_tag(token: &str) -> Option<TaskId> {
    token.strip_prefix("id:")?.parse().ok().filter(|&n| n != 0)
}

fn after_tag(token: &str) -> Option<Vec<TaskId>> {
    parse_after_list(token.strip_prefix("after:")?)
}

/// A token that is metadata wherever it stands.
fn is_tag(token: &str) -> bool {
    due_tag(token).is_some() || id_tag(token).is_some() || after_tag(token).is_some()
}

/// A token that is metadata at the head of a line: the done mark, a
/// priority, a completion or creation date.
fn is_head_token(token: &str) -> bool {
    token == "x" || is_priority_token(token) || is_date_token(token)
}

/// Marks the words of an escaped text that `decode` would take for
/// metadata with a backslash in front. Only the first word can be at the
/// head of the line: a marked one ends the head there. `escape` doubles
/// every backslash of the text, so a single one before a word is this
/// mark and nothing else.
fn protect(escaped: &str) -> String {
    let mut first = true;
    escaped
        .split(' ')
        .map(|word| {
            let at_head = first && !word.is_empty();
            if !word.is_empty() {
                first = false;
            }
            if (at_head && is_head_token(word)) || is_tag(word) {
                format!("\\{word}")
            } else {
                word.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Takes off the mark [`protect`] put on a word.
fn unprotect(token: &str) -> &str {
    match token.strip_prefix('\\') {
        Some(word) if is_head_token(word) || is_tag(word) => word,
        _ => token,
    }
}

pub fn decode(data: &str) -> Result<Vec<Task>> {
    let mut tasks: Vec<Task> = Vec::new();

    for raw in data.lines() {
        let line = raw.trim_end_matches('\r').trim_end();
        if line.trim().is_empty() {
            continue;
        }

        let mut done = false;
        let mut priority = false;
        let mut rest = line;
        if let Some(r) = rest.strip_prefix("x ") {
            done = true;
            rest = r.trim_start();
        }
        // Head metadata in any order todo.txt tools produce: a priority and
        // up to two dates (completion, creation) — the dates are dropped.
        let mut skipped_dates = 0;
        loop {
            let (token, tail) = rest.split_once(' ').unwrap_or((rest, ""));
            if is_priority_token(token) && !priority {
                priority = true;
            } else if is_date_token(token) && skipped_dates < 2 {
                skipped_dates += 1;
            } else {
                break;
            }
            rest = tail.trim_start();
        }

        // Split on single spaces so that runs of spaces inside the text
        // survive the rebuild (empty tokens are kept verbatim).
        let mut date = None;
        let mut id = None;
        let mut after: Vec<TaskId> = Vec::new();
        let mut words: Vec<&str> = Vec::new();
        for token in rest.split(' ') {
            if let Some(d) = due_tag(token)
                && date.is_none()
            {
                date = Some(d);
            } else if let Some(n) = id_tag(token)
                && id.is_none()
            {
                id = Some(n);
            } else if let Some(ids) = after_tag(token)
                && after.is_empty()
            {
                after = ids;
            } else {
                words.push(unprotect(token));
            }
        }

        tasks.push(Task {
            id: id.unwrap_or(0),
            text: unescape(&words.join(" ")),
            date,
            done,
            priority,
            after,
        });
    }

    Ok(tasks)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: TaskId, text: &str) -> Task {
        Task {
            id,
            text: text.to_string(),
            date: None,
            done: false,
            priority: false, after: Vec::new(),
        }
    }

    #[test]
    fn roundtrip() {
        let mut t1 = task(1, "multi\nline with \\backslash");
        t1.date = NaiveDate::from_ymd_opt(2026, 7, 15);
        t1.priority = true;
        let mut t2 = task(3, "plain +project @context note:keep");
        t2.done = true;
        let tasks = vec![t1, t2];

        let txt = encode(&tasks);
        assert_eq!(decode(&txt).unwrap(), tasks);
    }

    #[test]
    fn encoded_shape_follows_the_spec() {
        let mut t = task(2, "write report");
        t.done = true;
        t.priority = true;
        t.date = NaiveDate::from_ymd_opt(2026, 7, 15);
        assert_eq!(encode(&[t]), "x (A) write report due:2026-07-15 id:2\n");
    }

    #[test]
    fn foreign_lines_with_dates_and_priorities() {
        // Typical todo.sh output: completion + creation dates, letter priority.
        let txt = "x 2026-07-09 2026-07-01 pay the bill due:2026-07-20\n(B) 2026-07-02 call mom\n";
        let tasks = decode(txt).unwrap();
        assert!(tasks[0].done);
        assert_eq!(tasks[0].text, "pay the bill");
        assert_eq!(tasks[0].date, NaiveDate::from_ymd_opt(2026, 7, 20));
        assert_eq!(tasks[0].id, 0, "no id: the load hands one out");
        assert!(tasks[1].priority);
        assert_eq!(tasks[1].text, "call mom");
        assert_eq!(tasks[1].id, 0);
    }

    /// Review of R25: a `due:` word any date chrono reads is protected by
    /// the encoder, so the decoder has to take the `\` off it again — held
    /// to four-digit years, `pay \due:2026-7-1 bill` kept its backslash.
    #[test]
    fn a_protected_lenient_due_word_comes_back_bare() {
        let t = task(1, "pay due:2026-7-1 bill");
        let txt = encode(std::slice::from_ref(&t));
        assert!(txt.contains("\\due:2026-7-1"), "{txt}");
        assert_eq!(decode(&txt).unwrap(), vec![t]);
    }

    #[test]
    fn invalid_due_and_id_stay_in_text() {
        let tasks = decode("ship due:tomorrow id:zero\n").unwrap();
        assert_eq!(tasks[0].text, "ship due:tomorrow id:zero");
        assert_eq!(tasks[0].date, None);
    }

    #[test]
    fn after_token_roundtrips_and_invalid_stays_in_text() {
        let mut t = task(3, "blocked");
        t.after = vec![1, 2];
        let txt = encode(&[t.clone()]);
        assert_eq!(txt, "blocked after:1,2 id:3\n");
        assert_eq!(decode(&txt).unwrap(), vec![t]);

        let tasks = decode("ship after:1,x id:3\n").unwrap();
        assert_eq!(tasks[0].text, "ship after:1,x");
        assert_eq!(tasks[0].after, Vec::<TaskId>::new());
    }

    #[test]
    fn double_spaces_survive() {
        let tasks = decode(&encode(&[task(1, "two  spaces")])).unwrap();
        assert_eq!(tasks[0].text, "two  spaces");
    }

    #[test]
    fn empty_and_blank_input() {
        assert_eq!(decode("").unwrap(), Vec::<Task>::new());
        assert_eq!(decode("\n  \n").unwrap(), Vec::<Task>::new());
    }

    /// REVIEW №30: a text that looks like metadata comes back as it was,
    /// whatever the task's flags, date and dependencies.
    #[test]
    fn texts_that_look_like_metadata_round_trip() {
        let texts = [
            "x",
            "x marks the spot",
            "(B) plan the release",
            "(A)",
            "2026-09-20 dentist appointment",
            "2026-10-01",
            "due:2026-10-01",
            "ticket id:42 needs review",
            "do after:2 is done",
            "x id:1",
            "moved from due:2027-01-01 to next month",
            "x (A) 2026-01-01 id:9",
            "\\x is a backslash and an x",
            "\\id:5 too",
            "id:0 is no id, due:soon no date",
            "two  spaces",
        ];
        for text in texts {
            for (done, priority) in [(false, false), (true, false), (false, true), (true, true)] {
                let mut t = task(7, text);
                t.done = done;
                t.priority = priority;
                t.date = NaiveDate::from_ymd_opt(2027, 2, 2);
                t.after = vec![1];
                let txt = encode(std::slice::from_ref(&t));
                assert_eq!(decode(&txt).unwrap(), vec![t], "{txt:?}");
            }
        }
        // Only what would be misread is marked.
        assert_eq!(
            encode(&[task(3, "x (B) at 2026-01-01, see id:4")]),
            "\\x (B) at 2026-01-01, see \\id:4 id:3\n"
        );
    }

    /// A backslash that is no mark (a foreign file) stays as it is.
    #[test]
    fn a_backslash_before_other_words_is_kept() {
        let tasks = decode("path \\share\\x id:1\n").unwrap();
        assert_eq!(tasks[0].text, "path \\share\\x");
    }
}
