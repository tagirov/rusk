//! iCalendar VTODO database format (`tasks.ics`), feature `fmt-ics`.
//! RFC 5545; the interop champion: the file opens in Thunderbird, Nextcloud
//! Tasks, Apple Reminders and anything else that speaks iCalendar.
//!
//! Mapping per VTODO: SUMMARY = task text (`\n`-escaped for multi-line),
//! DUE (date value) = due date, STATUS:COMPLETED or CANCELLED = done (with
//! no STATUS, COMPLETED or PERCENT-COMPLETE:100 say it), PRIORITY 1–4 = the
//! priority flag (written as 1), a rusk UID (`rusk-<id>@<hash>.rusk`, or
//! `rusk-<id>@rusk` as rusk wrote it up to 0.7.3) pins the rusk id (a
//! foreign UID gets the lowest free id on load, and a rusk UID on the next
//! save, which pins that id), `RELATED-TO;RELTYPE=DEPENDS-ON` (or
//! FINISHTOSTART) with a rusk UID = a `--after` dependency, and so is a
//! RELATED-TO without RELTYPE to a UID of the old form, the way rusk wrote
//! one up to 0.7.3; to any other UID a RELATED-TO without RELTYPE is the
//! parent of a subtask (RFC 5545). Other relations, components (VEVENT,
//! VALARM) and properties are ignored and not preserved — rusk owns the
//! file. A VTODO without SUMMARY has no text: no task, unless its UID is a
//! rusk one (see [`crate::model::normalize`]).
//!
//! A save keeps what the file it replaces says of each task: its UID and,
//! while the task is unchanged, its DTSTAMP; a changed task gets the time of
//! the save, so a calendar client sees the tasks that changed as changed
//! (REVIEW №179). A task new to the file gets a UID made of its id and its
//! text: a new task that got the id of a deleted one has a UID of its own
//! (REVIEW №93), and a task written to two files (a local file and its sync
//! remote) has the same UID in both. The file knows its tasks by id only:
//! when a single save both removes a task and gives its id to a new one (a
//! `rusk sync` that brings both changes at once, a `PUT`), the new task
//! keeps the UID, as an edit of the old one; and a task renumbered on load
//! (two VTODOs with one id) gets a new UID, while a dependency on it goes to
//! the first of the two, as everywhere (see [`crate::model::normalize`]).
//! rusk up to 0.7.3 reads the id of a UID as the digits in front of its `@`,
//! so it keeps the ids and dependencies of a file this one wrote (and writes
//! its own UIDs back).

use crate::model::{Task, TaskId};
use anyhow::{Context, Result};
use chrono::NaiveDate;
use std::collections::HashMap;

/// TEXT value escaping per RFC 5545 §3.3.11. A TEXT value has no escape
/// for a carriage return, and a bare one is not allowed in a content line:
/// it is dropped, and a CRLF line break in a text is stored as `\n`.
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

/// The rusk id a UID pins, and whether the UID is of the form rusk wrote
/// up to 0.7.3: `rusk-<id>@<hex>.rusk`, or `rusk-<id>@rusk` (the old
/// form). Any other UID is foreign, one that starts with `rusk-<digits>`
/// too (`rusk-2024-standup@company.example`).
fn rusk_uid(uid: &str) -> Option<(TaskId, bool)> {
    let (id, host) = uid.trim().strip_prefix("rusk-")?.split_once('@')?;
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let old = host == "rusk";
    let unique = host
        .strip_suffix(".rusk")
        .is_some_and(|hex| !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit()));
    if !old && !unique {
        return None;
    }
    id.parse::<TaskId>().ok().filter(|&id| id != 0).map(|id| (id, old))
}

fn rusk_id(uid: &str) -> Option<TaskId> {
    rusk_uid(uid).map(|(id, _)| id)
}

/// The UID of a task new to the file: its id, as a rusk that reads only
/// the digits in front of the `@` (0.7.3) finds it, and a hash of its id
/// and text. A new task that got the id of a deleted one has a text of its
/// own, and so a UID of its own: a client that knew the deleted task does
/// not take the new one for it (REVIEW №93). And a task written to two
/// files (a local .ics and its sync remote) gets the same UID in both.
fn new_uid(task: &Task) -> String {
    let unique = crate::revision::text_revision(&format!("{}\n{}", task.id, task.text));
    format!("rusk-{}@{unique}.rusk", task.id)
}

/// A DTSTAMP as RFC 5545 has it: a UTC date-time. Another one (a date, an
/// empty value) is not kept: the save stamps the task anew.
fn is_stamp(value: &str) -> bool {
    let b = value.as_bytes();
    b.len() == 16
        && b[8] == b'T'
        && b[15] == b'Z'
        && b.iter().enumerate().all(|(i, c)| matches!(i, 8 | 15) || c.is_ascii_digit())
}

/// Encodes `tasks` as the file that replaces `previous` (the file as it is
/// now, if there is one): a task keeps the UID it has there and, if it is
/// unchanged, its DTSTAMP (see the module docs).
pub fn encode(tasks: &[Task], previous: Option<&str>) -> String {
    let known = previous.map(known_tasks).unwrap_or_default();
    let now = chrono::Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
    // Every UID first: a dependency names the UID of the task it points at.
    let uids: HashMap<TaskId, String> = tasks
        .iter()
        .map(|task| {
            let uid = known.get(&task.id).map_or_else(|| new_uid(task), |todo| todo.uid.clone());
            (task.id, uid)
        })
        .collect();

    let mut out = String::new();
    fold_into(&mut out, "BEGIN:VCALENDAR");
    fold_into(&mut out, "VERSION:2.0");
    fold_into(&mut out, "PRODID:-//rusk//tasks//EN");
    for task in tasks {
        let stamp = match known.get(&task.id) {
            Some(todo) if todo.task == *task => {
                todo.stamp.as_deref().filter(|stamp| is_stamp(stamp)).unwrap_or(&now)
            }
            _ => &now,
        };
        fold_into(&mut out, "BEGIN:VTODO");
        fold_into(&mut out, &format!("UID:{}", uids[&task.id]));
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
            let uid = uids.get(dep).cloned().unwrap_or_else(|| format!("rusk-{dep}@rusk"));
            fold_into(&mut out, &format!("RELATED-TO;RELTYPE=DEPENDS-ON:{uid}"));
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

/// The rusk tasks of an earlier version of the file, by id. A file that
/// does not read as a calendar knows nothing; of two VTODOs with one id the
/// first is the one the load kept.
fn known_tasks(previous: &str) -> HashMap<TaskId, Todo> {
    let mut known = HashMap::new();
    for todo in read_todos(previous).unwrap_or_default() {
        if todo.task.id != 0 {
            known.entry(todo.task.id).or_insert(todo);
        }
    }
    known
}

/// Reverses folding: a line starting with a space or tab continues the
/// previous one.
fn unfold(data: &str) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    for raw in data.lines() {
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

/// One content line: NAME (uppercased; iCalendar names are
/// case-insensitive), its parameters (names uppercased, values without
/// their quotes) and VALUE.
struct Property<'a> {
    name: String,
    params: Vec<(String, String)>,
    value: &'a str,
}

impl Property<'_> {
    fn param(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

/// Splits `text` at every `sep` that is not inside a quoted parameter
/// value.
fn split_unquoted(text: &str, sep: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut quoted = false;
    let mut start = 0;
    for (i, c) in text.char_indices() {
        if c == '"' {
            quoted = !quoted;
        } else if c == sep && !quoted {
            parts.push(&text[start..i]);
            start = i + c.len_utf8();
        }
    }
    parts.push(&text[start..]);
    parts
}

/// Splits a content line into a [`Property`]. The value starts at the
/// first `:` outside a quoted parameter value: RFC 5545 §3.1 allows one in
/// there (`SUMMARY;X-NOTE="see: here":the text`, REVIEW №94).
fn property(line: &str) -> Option<Property<'_>> {
    let mut quoted = false;
    let unquoted = line.char_indices().find(|&(_, c)| {
        if c == '"' {
            quoted = !quoted;
        }
        c == ':' && !quoted
    });
    // A quote that is never closed quotes nothing: the value starts at the
    // first `:`, as it did before quotes were read (review of R23).
    let colon = match unquoted {
        Some((colon, _)) => colon,
        None => line.find(':')?,
    };
    let mut head = split_unquoted(&line[..colon], ';').into_iter();
    let name = head.next().unwrap_or_default().trim().to_ascii_uppercase();
    let params = head
        .filter_map(|param| param.split_once('='))
        .map(|(key, value)| {
            (key.trim().to_ascii_uppercase(), value.trim().trim_matches('"').to_string())
        })
        .collect();
    Some(Property {
        name,
        params,
        value: &line[colon + 1..],
    })
}

/// `19970714` or `19970714T170000Z` → date; DUE may be either form.
fn parse_ics_date(value: &str) -> Option<NaiveDate> {
    let digits = value.split('T').next()?.trim();
    NaiveDate::parse_from_str(digits, "%Y%m%d").ok()
}

/// One VTODO as read.
struct Todo {
    /// The task, id 0 for a foreign UID.
    task: Task,
    uid: String,
    stamp: Option<String>,
}

/// Whether a VTODO is done: STATUS says it when there is one — COMPLETED,
/// or CANCELLED, which is not to do either — and without one COMPLETED (a
/// completion time) or PERCENT-COMPLETE:100 do (REVIEW №92).
#[derive(Default)]
struct Done {
    status: Option<bool>,
    completed: bool,
}

impl Done {
    fn done(&self) -> bool {
        self.status.unwrap_or(self.completed)
    }
}

fn read_todos(data: &str) -> Result<Vec<Todo>> {
    let mut todos: Vec<Todo> = Vec::new();
    let mut done = Done::default();
    // Depth of nested non-VTODO components (VALARM inside VTODO, VEVENT...):
    // properties are only read at VTODO level itself.
    let mut in_vtodo = false;
    let mut nested = 0usize;

    for line in unfold(data) {
        let Some(prop) = property(&line) else {
            continue;
        };
        let value = prop.value;
        match (prop.name.as_str(), in_vtodo) {
            ("BEGIN", false) if value.trim().eq_ignore_ascii_case("VTODO") => {
                in_vtodo = true;
                done = Done::default();
                todos.push(Todo {
                    task: Task {
                        id: 0,
                        text: String::new(),
                        date: None,
                        done: false,
                        priority: false,
                        after: Vec::new(),
                    },
                    uid: String::new(),
                    stamp: None,
                });
            }
            ("BEGIN", true) => nested += 1,
            ("END", true) => {
                if nested > 0 {
                    nested -= 1;
                } else if value.trim().eq_ignore_ascii_case("VTODO") {
                    in_vtodo = false;
                    if let Some(todo) = todos.last_mut() {
                        todo.task.done = done.done();
                    }
                } else {
                    anyhow::bail!("iCalendar: END:{} while inside a VTODO", value.trim());
                }
            }
            (_, true) if nested == 0 => {
                let todo = todos.last_mut().context("VTODO property without VTODO")?;
                let task = &mut todo.task;
                match prop.name.as_str() {
                    "UID" => {
                        todo.uid = value.trim().to_string();
                        task.id = rusk_id(value).unwrap_or(0);
                    }
                    "DTSTAMP" => todo.stamp = Some(value.trim().to_string()),
                    "SUMMARY" => task.text = unescape(value),
                    "DUE" => task.date = parse_ics_date(value),
                    // Dependencies (`--after`) travel as RELATED-TO with
                    // RELTYPE=DEPENDS-ON pointing at rusk UIDs (RFC 9253;
                    // FINISHTOSTART says the same). Without RELTYPE a
                    // relation is a parent (RFC 5545). Other relations, and
                    // relations to foreign UIDs, are ignored.
                    "RELATED-TO" => {
                        if let Some((dep, old)) = rusk_uid(value) {
                            let depends = match prop.param("RELTYPE") {
                                Some(kind) => ["DEPENDS-ON", "FINISHTOSTART"]
                                    .iter()
                                    .any(|k| kind.eq_ignore_ascii_case(k)),
                                // The parent of a subtask a client made —
                                // unless the UID is of the old form, which
                                // is how rusk wrote a dependency up to 0.7.3.
                                None => old,
                            };
                            if depends {
                                task.after.push(dep);
                            }
                        }
                    }
                    "STATUS" => {
                        let status = value.trim();
                        done.status = Some(
                            status.eq_ignore_ascii_case("COMPLETED")
                                || status.eq_ignore_ascii_case("CANCELLED"),
                        );
                    }
                    "COMPLETED" => done.completed = true,
                    "PERCENT-COMPLETE" if value.trim().parse::<u8>() == Ok(100) => {
                        done.completed = true;
                    }
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
    Ok(todos)
}

pub fn decode(data: &str) -> Result<Vec<Task>> {
    Ok(read_todos(data)?.into_iter().map(|todo| todo.task).collect())
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

        let ics = encode(&tasks, None);
        assert_eq!(decode(&ics).unwrap(), tasks);
    }

    #[test]
    fn long_lines_are_folded_and_unfold_back() {
        let long = "long task ".repeat(30);
        let t = task(1, long.trim_end());
        let ics = encode(std::slice::from_ref(&t), None);
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
        assert_eq!(tasks[0].id, 0, "a foreign UID is no id: the load hands one out");
    }

    #[test]
    fn after_ids_roundtrip_as_related_to() {
        let mut t = task(3, "blocked");
        t.after = vec![1, 2];
        let ics = encode(std::slice::from_ref(&t), None);
        assert!(ics.contains("RELATED-TO;RELTYPE=DEPENDS-ON:rusk-1@rusk"), "{ics}");
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

    fn uid_of(ics: &str, id: TaskId) -> String {
        let todos = read_todos(ics).unwrap();
        todos.into_iter().find(|todo| todo.task.id == id).unwrap().uid
    }

    fn stamp_of(ics: &str, id: TaskId) -> String {
        let todos = read_todos(ics).unwrap();
        todos.into_iter().find(|todo| todo.task.id == id).unwrap().stamp.unwrap()
    }

    /// REVIEW №93: `rusk-<id>@rusk` went to the next task that got the id
    /// of a deleted one; a client took it for the deleted task, changed.
    #[test]
    fn a_task_keeps_its_uid_and_a_new_one_gets_a_uid_of_its_own() {
        let first = encode(&[task(1, "first"), task(2, "second")], None);
        let (one, two) = (uid_of(&first, 1), uid_of(&first, 2));
        assert!(one.starts_with("rusk-1@") && one.ends_with(".rusk"), "{one}");
        assert_ne!(one, two);
        // Review of R23: rusk up to 0.7.3 reads the id as the digits in
        // front of the `@`, so it keeps ids and dependencies of this file.
        let as_0_7_3 = |uid: &str| uid.strip_prefix("rusk-")?.split('@').next()?.parse::<TaskId>().ok();
        assert_eq!(as_0_7_3(&one), Some(1));
        // The same task written to another file (a sync remote) gets the
        // same UID there.
        assert_eq!(uid_of(&encode(&[task(1, "first")], None), 1), one);

        // Saved again: the same UIDs.
        let again = encode(&[task(1, "first, edited"), task(2, "second")], Some(&first));
        assert_eq!((uid_of(&again, 1), uid_of(&again, 2)), (one.clone(), two.clone()));

        // Task 1 deleted (by a client), then a new task gets id 1.
        let without_one = encode(&[task(2, "second")], Some(&again));
        let reused = encode(&[task(1, "brand new"), task(2, "second")], Some(&without_one));
        assert_ne!(uid_of(&reused, 1), one);
        assert_eq!(uid_of(&reused, 2), two);
        // One save that does both (a sync that brings them at once) is an
        // edit of task 1 to the file: it keeps the UID (see the module docs).
        let at_once = encode(&[task(1, "brand new"), task(2, "second")], Some(&again));
        assert_eq!(uid_of(&at_once, 1), one);

        // What rusk wrote up to 0.7.3 keeps its UIDs.
        let old = "BEGIN:VCALENDAR\r\nBEGIN:VTODO\r\nUID:rusk-7@rusk\r\nSUMMARY:old\r\nEND:VTODO\r\nEND:VCALENDAR\r\n";
        let tasks = decode(old).unwrap();
        assert_eq!(tasks[0].id, 7);
        assert_eq!(uid_of(&encode(&tasks, Some(old)), 7), "rusk-7@rusk");
    }

    /// REVIEW №179: every save stamped every VTODO with the time of the
    /// save.
    #[test]
    fn an_unchanged_task_keeps_its_stamp() {
        let old = "BEGIN:VCALENDAR\r\nBEGIN:VTODO\r\nUID:rusk-1@rusk\r\nDTSTAMP:20200101T000000Z\r\n\
                   SUMMARY:same\r\nSTATUS:NEEDS-ACTION\r\nEND:VTODO\r\nBEGIN:VTODO\r\nUID:rusk-2@rusk\r\n\
                   DTSTAMP:20200101T000000Z\r\nSUMMARY:to change\r\nEND:VTODO\r\nEND:VCALENDAR\r\n";
        let mut tasks = decode(old).unwrap();
        tasks[1].done = true;
        let new = encode(&tasks, Some(old));
        assert_eq!(stamp_of(&new, 1), "20200101T000000Z");
        assert_ne!(stamp_of(&new, 2), "20200101T000000Z");
        // Nothing changed: the same file, byte for byte.
        assert_eq!(encode(&decode(&new).unwrap(), Some(&new)), new);
    }

    /// REVIEW №91: RELATED-TO without RELTYPE is a parent (RFC 5545), not a
    /// dependency.
    #[test]
    fn a_dependency_says_it_is_one() {
        let mut blocked = task(2, "blocked");
        blocked.after = vec![1];
        let ics = encode(&[task(1, "first"), blocked.clone()], None);
        let line = ics.lines().find(|l| l.starts_with("RELATED-TO")).unwrap();
        assert_eq!(line, format!("RELATED-TO;RELTYPE=DEPENDS-ON:{}", uid_of(&ics, 1)));
        assert_eq!(decode(&ics).unwrap()[1].after, vec![1]);

        let related = |param: &str| {
            let ics = format!(
                "BEGIN:VCALENDAR\nBEGIN:VTODO\nUID:rusk-2@rusk\nSUMMARY:x\nRELATED-TO{param}:rusk-1@rusk\nEND:VTODO\nEND:VCALENDAR\n"
            );
            decode(&ics).unwrap()[0].after.clone()
        };
        assert_eq!(related(""), vec![1], "as rusk wrote it up to 0.7.3");
        assert_eq!(related(";RELTYPE=depends-on"), vec![1]);
        assert_eq!(related(";RELTYPE=FINISHTOSTART"), vec![1]);
        // Review of R23: without RELTYPE to a UID rusk writes now, it is the
        // parent of a subtask a client made.
        let parent = "BEGIN:VCALENDAR\nBEGIN:VTODO\nUID:rusk-1@aaaa.rusk\nSUMMARY:Plan trip\nEND:VTODO\n\
                      BEGIN:VTODO\nUID:c0ffee@nextcloud\nSUMMARY:Book hotel\nRELATED-TO:rusk-1@aaaa.rusk\n\
                      END:VTODO\nEND:VCALENDAR\n";
        assert!(decode(parent).unwrap()[1].after.is_empty());
        assert_eq!(related(";RELTYPE=PARENT"), Vec::<TaskId>::new());
        assert_eq!(related(";RELTYPE=CHILD"), Vec::<TaskId>::new());
    }

    /// REVIEW №92: a VTODO completed elsewhere without a STATUS was open.
    #[test]
    fn completed_without_a_status_is_done() {
        let todo = |props: &str| {
            let ics = format!("BEGIN:VCALENDAR\nBEGIN:VTODO\nSUMMARY:x\n{props}END:VTODO\nEND:VCALENDAR\n");
            decode(&ics).unwrap()[0].done
        };
        assert!(todo("COMPLETED:20260102T100000Z\n"));
        assert!(todo("PERCENT-COMPLETE:100\n"));
        assert!(!todo("PERCENT-COMPLETE:50\n"));
        assert!(todo("STATUS:CANCELLED\n"));
        // STATUS says it when there is one.
        assert!(!todo("STATUS:NEEDS-ACTION\nPERCENT-COMPLETE:100\n"));
        assert!(!todo("COMPLETED:20260102T100000Z\nSTATUS:IN-PROCESS\n"));
        assert!(!todo(""));
    }

    /// REVIEW №94: the value was cut at the first `:`, one inside a quoted
    /// parameter too.
    #[test]
    fn a_colon_in_a_quoted_parameter_is_not_the_value() {
        let ics = "BEGIN:VCALENDAR\nBEGIN:VTODO\nUID:q1\nSUMMARY;X-NOTE=\"see: here\":real summary\n\
                   DUE;TZID=\"Etc/GMT+1\":20270301T090000\nEND:VTODO\nEND:VCALENDAR\n";
        let tasks = decode(ics).unwrap();
        assert_eq!(tasks[0].text, "real summary");
        assert_eq!(tasks[0].date, NaiveDate::from_ymd_opt(2027, 3, 1));
        let prop = property(r#"RELATED-TO;X-A="a;b:c";RELTYPE=DEPENDS-ON:rusk-1@rusk"#).unwrap();
        assert_eq!((prop.name.as_str(), prop.value), ("RELATED-TO", "rusk-1@rusk"));
        assert_eq!(prop.param("X-A"), Some("a;b:c"));
        assert_eq!(prop.param("RELTYPE"), Some("DEPENDS-ON"));
    }

    #[test]
    fn a_rusk_uid_names_its_id() {
        let cases = [
            ("rusk-7@rusk", Some(7)),
            ("rusk-7@00ff.rusk", Some(7)),
            (" rusk-12@d42edf9ace21fcc8.rusk ", Some(12)),
            ("rusk-7", None),
            ("rusk-0@rusk", None),
            ("rusk-7x@rusk", None),
            ("rusk-@rusk", None),
            ("abc@rusk", None),
            ("rusk-7@.rusk", None),
            ("rusk-7@xyz.rusk", None),
            ("rusk-7@company.example", None),
            // Review of R23: a foreign UID that starts like a rusk one.
            ("rusk-2024-standup@company.example", None),
        ];
        for (uid, id) in cases {
            assert_eq!(rusk_id(uid), id, "{uid}");
        }
    }

    /// Review of R23: a quote left open quotes nothing; a DTSTAMP that is
    /// no UTC date-time is not kept.
    #[test]
    fn broken_values_are_read_as_before_and_stamps_made_anew() {
        let ics = "BEGIN:VCALENDAR\nBEGIN:VTODO\nUID:v1@elsewhere\nSUMMARY;X-NOTE=\"unclosed:made elsewhere\n\
                   END:VTODO\nEND:VCALENDAR\n";
        assert_eq!(decode(ics).unwrap()[0].text, "made elsewhere");
        for stamp in ["", "20200101", "2020-01-01T00:00:00Z"] {
            let old = format!(
                "BEGIN:VCALENDAR\r\nBEGIN:VTODO\r\nUID:rusk-1@rusk\r\nDTSTAMP:{stamp}\r\nSUMMARY:same\r\n\
                 STATUS:NEEDS-ACTION\r\nEND:VTODO\r\nEND:VCALENDAR\r\n"
            );
            let new = encode(&decode(&old).unwrap(), Some(&old));
            assert!(is_stamp(&stamp_of(&new, 1)), "{stamp:?}: {new}");
        }
        assert!(is_stamp("20200101T000000Z"));
    }
}
