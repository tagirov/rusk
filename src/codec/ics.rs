//! iCalendar VTODO database format (`tasks.ics`), feature `fmt-ics`.
//! RFC 5545; the interop champion: the file opens in Thunderbird, Nextcloud
//! Tasks, Apple Reminders and anything else that speaks iCalendar.
//!
//! Mapping per VTODO: SUMMARY = task text (`\n`-escaped for multi-line),
//! DUE (date value) = due date, STATUS:COMPLETED = done, PRIORITY 1–4 =
//! the priority flag (written as 1), UID `rusk-<id>@rusk` pins the rusk id
//! (foreign UIDs get the lowest free id on load), RELATED-TO with a rusk
//! UID = a `--after` dependency (foreign relations are ignored). Other components
//! (VEVENT, VALARM) and properties are ignored and not preserved — rusk
//! owns the file.

use super::assign_missing_ids;
use crate::model::{Task, TaskId};
use anyhow::{Context, Result};
use chrono::NaiveDate;

/// TEXT value escaping per RFC 5545 §3.3.11.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            ';' => out.push_str("\\;"),
            ',' => out.push_str("\\,"),
            '\n' => out.push_str("\\n"),
            '\r' => {}
            c => out.push(c),
        }
    }
    out
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
            Some('n') | Some('N') => out.push('\n'),
            Some(other) => out.push(other),
            None => out.push('\\'),
        }
    }
    out
}

/// Content-line folding per RFC 5545 §3.1: lines longer than 75 octets are
/// split with CRLF + one space, on UTF-8 boundaries.
fn fold_into(out: &mut String, line: &str) {
    const LIMIT: usize = 75;
    let mut budget = LIMIT;
    for c in line.chars() {
        let len = c.len_utf8();
        if len > budget {
            out.push_str("\r\n ");
            budget = LIMIT - 1;
        }
        out.push(c);
        budget -= len;
    }
    out.push_str("\r\n");
}

pub fn encode(tasks: &[Task]) -> String {
    let mut out = String::new();
    fold_into(&mut out, "BEGIN:VCALENDAR");
    fold_into(&mut out, "VERSION:2.0");
    fold_into(&mut out, "PRODID:-//rusk//tasks//EN");
    let stamp = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    for task in tasks {
        fold_into(&mut out, "BEGIN:VTODO");
        fold_into(&mut out, &format!("UID:rusk-{}@rusk", task.id));
        fold_into(&mut out, &format!("DTSTAMP:{stamp}"));
        fold_into(&mut out, &format!("SUMMARY:{}", escape(&task.text)));
        if let Some(date) = task.date {
            fold_into(
                &mut out,
                &format!("DUE;VALUE=DATE:{}", date.format("%Y%m%d")),
            );
        }
        if task.priority {
            fold_into(&mut out, "PRIORITY:1");
        }
        for dep in &task.after {
            fold_into(&mut out, &format!("RELATED-TO:rusk-{dep}@rusk"));
        }
        fold_into(
            &mut out,
            if task.done {
                "STATUS:COMPLETED"
            } else {
                "STATUS:NEEDS-ACTION"
            },
        );
        fold_into(&mut out, "END:VTODO");
    }
    fold_into(&mut out, "END:VCALENDAR");
    out
}

/// Reverses folding: a line starting with a space or tab continues the
/// previous one.
fn unfold(data: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in data.trim_start_matches('\u{feff}').lines() {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if let Some(rest) = line.strip_prefix(' ').or_else(|| line.strip_prefix('\t'))
            && let Some(last) = lines.last_mut()
        {
            last.push_str(rest);
        } else {
            lines.push(line.to_string());
        }
    }
    lines
}

/// Splits a content line into (NAME, VALUE), dropping parameters. NAME is
/// uppercased for comparison; iCalendar names are case-insensitive.
fn property(line: &str) -> Option<(String, &str)> {
    let (head, value) = line.split_once(':')?;
    let name = head.split(';').next().unwrap_or(head);
    Some((name.trim().to_ascii_uppercase(), value))
}

/// `19970714` or `19970714T170000Z` → date; DUE may be either form.
fn parse_ics_date(value: &str) -> Option<NaiveDate> {
    let digits = value.split('T').next()?.trim();
    NaiveDate::parse_from_str(digits, "%Y%m%d").ok()
}

pub fn decode(data: &str) -> Result<Vec<Task>> {
    let mut tasks: Vec<Task> = Vec::new();
    // Depth of nested non-VTODO components (VALARM inside VTODO, VEVENT...):
    // properties are only read at VTODO level itself.
    let mut in_vtodo = false;
    let mut nested = 0usize;

    for line in unfold(data) {
        let Some((name, value)) = property(&line) else {
            continue;
        };
        match (name.as_str(), in_vtodo) {
            ("BEGIN", false) if value.trim().eq_ignore_ascii_case("VTODO") => {
                in_vtodo = true;
                tasks.push(Task {
                    id: 0,
                    text: String::new(),
                    date: None,
                    done: false,
                    priority: false, after: Vec::new(),
                });
            }
            ("BEGIN", true) => nested += 1,
            ("END", true) => {
                if nested > 0 {
                    nested -= 1;
                } else if value.trim().eq_ignore_ascii_case("VTODO") {
                    in_vtodo = false;
                } else {
                    anyhow::bail!("iCalendar: END:{} while inside a VTODO", value.trim());
                }
            }
            (_, true) if nested == 0 => {
                let task = tasks.last_mut().context("VTODO property without VTODO")?;
                match name.as_str() {
                    "UID" => {
                        let id = value
                            .trim()
                            .strip_prefix("rusk-")
                            .map(|r| r.split('@').next().unwrap_or(r))
                            .and_then(|n| n.parse::<TaskId>().ok());
                        task.id = id.unwrap_or(0);
                    }
                    "SUMMARY" => task.text = unescape(value),
                    "DUE" => task.date = parse_ics_date(value),
                    // Dependencies (`--after`) travel as RELATED-TO pointing
                    // at rusk UIDs; foreign relations are ignored.
                    "RELATED-TO" => {
                        if let Some(dep) = value
                            .trim()
                            .strip_prefix("rusk-")
                            .map(|r| r.split('@').next().unwrap_or(r))
                            .and_then(|n| n.parse::<TaskId>().ok())
                            && dep != 0
                        {
                            task.after.push(dep);
                        }
                    }
                    "STATUS" => task.done = value.trim().eq_ignore_ascii_case("COMPLETED"),
                    "PRIORITY" => {
                        task.priority = matches!(value.trim().parse::<u8>(), Ok(1..=4));
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    if in_vtodo {
        anyhow::bail!("iCalendar: unterminated VTODO component");
    }
    assign_missing_ids(&mut tasks)?;
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
        let mut t1 = task(1, "multi\nline; with, punctuation\\slash");
        t1.date = NaiveDate::from_ymd_opt(2026, 7, 15);
        t1.priority = true;
        let mut t2 = task(3, "plain done");
        t2.done = true;
        let tasks = vec![t1, t2];

        let ics = encode(&tasks);
        assert_eq!(decode(&ics).unwrap(), tasks);
    }

    #[test]
    fn long_lines_are_folded_and_unfold_back() {
        let long = "long task ".repeat(30);
        let t = task(1, long.trim_end());
        let ics = encode(std::slice::from_ref(&t));
        assert!(ics.lines().all(|l| l.len() <= 75), "unfolded line left");
        assert_eq!(decode(&ics).unwrap()[0].text, t.text);
    }

    #[test]
    fn foreign_calendar_input() {
        // Thunderbird-style: CRLF, params on DUE, foreign UID, VALARM inside.
        let ics = "BEGIN:VCALENDAR\r\nBEGIN:VTODO\r\nUID:abc-123@thunderbird\r\n\
                   SUMMARY:Water the plants\r\nDUE;TZID=Europe/Berlin:20260801T090000\r\n\
                   BEGIN:VALARM\r\nSUMMARY:not the task summary\r\nEND:VALARM\r\n\
                   STATUS:COMPLETED\r\nPRIORITY:9\r\nEND:VTODO\r\nEND:VCALENDAR\r\n";
        let tasks = decode(ics).unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].text, "Water the plants");
        assert_eq!(tasks[0].date, NaiveDate::from_ymd_opt(2026, 8, 1));
        assert!(tasks[0].done);
        assert!(!tasks[0].priority, "PRIORITY:9 is low, not the flag");
        assert_eq!(tasks[0].id, 1, "foreign UID gets a free id");
    }

    #[test]
    fn after_ids_roundtrip_as_related_to() {
        let mut t = task(3, "blocked");
        t.after = vec![1, 2];
        let ics = encode(std::slice::from_ref(&t));
        assert!(ics.contains("RELATED-TO:rusk-1@rusk"), "{ics}");
        assert_eq!(decode(&ics).unwrap(), vec![t]);

        // Foreign RELATED-TO values are not dependencies.
        let foreign = "BEGIN:VCALENDAR\nBEGIN:VTODO\nSUMMARY:x\nRELATED-TO:abc@else\nEND:VTODO\nEND:VCALENDAR\n";
        assert_eq!(decode(foreign).unwrap()[0].after, Vec::<TaskId>::new());
    }

    #[test]
    fn vevents_are_ignored() {
        let ics = "BEGIN:VCALENDAR\nBEGIN:VEVENT\nSUMMARY:a meeting\nEND:VEVENT\nEND:VCALENDAR\n";
        assert_eq!(decode(ics).unwrap(), Vec::<Task>::new());
    }

    #[test]
    fn empty_input() {
        assert_eq!(decode("").unwrap(), Vec::<Task>::new());
    }
}
