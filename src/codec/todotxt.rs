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
//! an `id:N` token pins the rusk id (assigned on load when missing).
//! Projects/contexts/other `key:value` tokens are kept as part of the task
//! text. Completion/creation dates at the head of a line are accepted but
//! not preserved. Newlines inside task text are stored as a literal `\n`
//! (with `\\` escaping a backslash) — nonstandard, but round-trips.

use super::assign_missing_ids;
use crate::model::Task;
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
        out.push_str(&escape(&task.text));
        if let Some(date) = task.date {
            out.push_str(&format!(" due:{date}"));
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

pub fn decode(data: &str) -> Result<Vec<Task>> {
    let mut tasks: Vec<Task> = Vec::new();

    for raw in data.trim_start_matches('\u{feff}').lines() {
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
        let mut words: Vec<&str> = Vec::new();
        for token in rest.split(' ') {
            if let Some(d) = token.strip_prefix("due:")
                && let Ok(d) = d.parse::<NaiveDate>()
                && date.is_none()
            {
                date = Some(d);
            } else if let Some(n) = token.strip_prefix("id:")
                && let Ok(n) = n.parse::<u8>()
                && n != 0
                && id.is_none()
            {
                id = Some(n);
            } else {
                words.push(token);
            }
        }

        tasks.push(Task {
            id: id.unwrap_or(0),
            text: unescape(&words.join(" ")),
            date,
            done,
            priority,
        });
    }

    assign_missing_ids(&mut tasks)?;
    Ok(tasks)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: u8, text: &str) -> Task {
        Task {
            id,
            text: text.to_string(),
            date: None,
            done: false,
            priority: false,
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
        assert_eq!(tasks[0].id, 1);
        assert!(tasks[1].priority);
        assert_eq!(tasks[1].text, "call mom");
        assert_eq!(tasks[1].id, 2);
    }

    #[test]
    fn invalid_due_and_id_stay_in_text() {
        let tasks = decode("ship due:tomorrow id:999\n").unwrap();
        assert_eq!(tasks[0].text, "ship due:tomorrow id:999");
        assert_eq!(tasks[0].date, None);
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
}
