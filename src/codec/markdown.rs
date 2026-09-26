//! Markdown task-list database format (`tasks.md`), feature `fmt-markdown`.
//!
//! One task per GitHub-style list item; continuation lines of a multi-line
//! task are indented by two spaces:
//!
//! ```text
//! - [ ] ! Fix the roof @2026-07-15 <!-- id:3 -->
//!   the ladder is in the garage
//! - [x] plain done task <!-- id:1 -->
//! ```
//!
//! `[x]` marks done, a leading `!` token marks priority, a trailing
//! `@YYYY-MM-DD` token is the due date, and the trailing HTML comment pins
//! the task id and optional dependencies (`<!-- id:3 after:1,2 -->`,
//! invisible in rendered Markdown). Hand-added items may omit
//! the id comment: the lowest free id is assigned on load. The file renders
//! as a normal task list on GitHub/Obsidian and can be edited by hand.
//!
//! Caveats: rusk owns the file — lines that are not list items or their
//! continuations (headers, prose) are ignored on load and dropped on the
//! next save, and so is an item with neither text nor id comment (an empty
//! `- [ ]`, see [`crate::model::normalize`]). Task text that itself starts
//! with a `!` token, ends with an `@YYYY-MM-DD` token after other words, or
//! contains a line that looks like a `- [ ]` item is indistinguishable from
//! the markup and will be reinterpreted; use JSON if that matters.

use crate::model::{Task, TaskId};
use anyhow::Result;
use chrono::NaiveDate;

pub fn encode(tasks: &[Task]) -> String {
    let mut out = String::new();
    for task in tasks {
        let mut lines = task.text.split('\n');
        let first = lines.next().unwrap_or("");
        out.push_str(if task.done { "- [x] " } else { "- [ ] " });
        if task.priority {
            out.push_str("! ");
        }
        out.push_str(first);
        if let Some(date) = task.date {
            out.push_str(&format!(" @{date}"));
        }
        if task.after.is_empty() {
            out.push_str(&format!(" <!-- id:{} -->\n", task.id));
        } else {
            let after = task
                .after
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(",");
            out.push_str(&format!(" <!-- id:{} after:{} -->\n", task.id, after));
        }
        for line in lines {
            out.push_str("  ");
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/// The content after a `- [ ] ` / `- [x] ` bullet, with the done flag.
fn item_start(line: &str) -> Option<(bool, &str)> {
    let rest = line
        .strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .or_else(|| line.strip_prefix("+ "))?;
    let done = match rest.get(..3) {
        Some("[ ]") => false,
        Some("[x]") | Some("[X]") => true,
        _ => return None,
    };
    let content = &rest[3..];
    Some((done, content.strip_prefix(' ').unwrap_or(content)))
}

/// Strips the trailing `<!-- id:N -->` / `<!-- id:N after:A,B -->` metadata
/// comment; foreign comments stay in the text.
fn take_meta(content: &str) -> (Option<TaskId>, Vec<TaskId>, &str) {
    let trimmed = content.trim_end();
    if let Some(before_close) = trimmed.strip_suffix("-->")
        && let Some((text, inner)) = before_close.rsplit_once("<!--")
        && let Some((id, after)) = parse_meta_comment(inner)
    {
        return (Some(id), after, text.trim_end());
    }
    (None, Vec::new(), trimmed)
}

/// `id:N [after:A,B]` inside the metadata comment. Anything else (a foreign
/// comment, an invalid id) rejects the whole comment so it stays in the text.
fn parse_meta_comment(inner: &str) -> Option<(TaskId, Vec<TaskId>)> {
    let mut id = None;
    let mut after = Vec::new();
    for token in inner.split_whitespace() {
        if let Some(n) = token.strip_prefix("id:") {
            match n.parse::<TaskId>() {
                Ok(n) if n != 0 && id.is_none() => id = Some(n),
                _ => return None,
            }
        } else if let Some(list) = token.strip_prefix("after:") {
            for part in list.split(',') {
                match part.trim().parse::<TaskId>() {
                    Ok(n) if n != 0 => after.push(n),
                    _ => return None,
                }
            }
        } else {
            return None;
        }
    }
    id.map(|id| (id, after))
}

/// Strips a trailing `@YYYY-MM-DD` token — unless nothing but the priority
/// mark would be left: then the token is the text (as rusk writes a task
/// whose whole text is `@2026-01-01`).
fn take_date(content: &str) -> (Option<NaiveDate>, &str) {
    if let Some((text, last)) = content.rsplit_once(' ')
        && !matches!(text.trim(), "" | "!")
        && let Some(date) = last.strip_prefix('@')
        && let Ok(date) = date.parse::<NaiveDate>()
    {
        return (Some(date), text.trim_end());
    }
    (None, content)
}

pub fn decode(data: &str) -> Result<Vec<Task>> {
    let lines: Vec<&str> = data.lines().collect();
    let mut tasks: Vec<Task> = Vec::new();
    let mut in_task = false;

    for (i, raw) in lines.iter().enumerate() {
        let line = raw.trim_end_matches('\r');
        if let Some((done, content)) = item_start(line.trim_start()) {
            let (id, after, content) = take_meta(content);
            let (date, content) = take_date(content);
            let (priority, text) = match content.strip_prefix("! ") {
                Some(rest) => (true, rest),
                None => (false, content),
            };
            tasks.push(Task {
                id: id.unwrap_or(0),
                text: text.to_string(),
                date,
                done,
                priority,
                after,
            });
            in_task = true;
        } else if in_task && line.starts_with("  ") {
            tasks.last_mut().expect("in_task implies a task").text +=
                &format!("\n{}", &line[2..]);
        } else if in_task && line.trim().is_empty() {
            // A blank line continues the task only when an indented
            // continuation follows; otherwise it just separates items.
            let continues = lines[i + 1..]
                .iter()
                .map(|l| l.trim_end_matches('\r'))
                .find(|l| !l.trim().is_empty())
                .is_some_and(|l| l.starts_with("  ") && item_start(l.trim_start()).is_none());
            if continues {
                tasks.last_mut().expect("in_task implies a task").text += "\n";
            } else {
                in_task = false;
            }
        } else {
            // Headers, prose, anything else: not part of a task.
            in_task = false;
        }
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
        let mut t1 = task(1, "multi\nline task\n\nwith a blank line");
        t1.date = NaiveDate::from_ymd_opt(2026, 7, 15);
        t1.priority = true;
        let mut t2 = task(3, "plain done");
        t2.done = true;
        let tasks = vec![t1, t2];

        let md = encode(&tasks);
        assert_eq!(decode(&md).unwrap(), tasks);
    }

    #[test]
    fn encoded_shape_is_a_github_task_list() {
        let mut t = task(2, "fix roof");
        t.date = NaiveDate::from_ymd_opt(2026, 7, 15);
        t.priority = true;
        assert_eq!(encode(&[t]), "- [ ] ! fix roof @2026-07-15 <!-- id:2 -->\n");
    }

    #[test]
    fn hand_written_items_get_free_ids() {
        let md = "# My tasks\n\n- [ ] no id here\n- [x] also none\nsome prose\n- [ ] third <!-- id:1 -->\n";
        let mut tasks = decode(md).unwrap();
        assert_eq!(tasks.len(), 3);
        // The decoder says "no id"; the load hands out the free ones.
        assert_eq!((tasks[0].id, tasks[1].id, tasks[2].id), (0, 0, 1));
        crate::model::normalize(&mut tasks).unwrap();
        assert_eq!(tasks[0].id, 2);
        assert!(tasks[1].done);
        assert_eq!(tasks[1].id, 3);
        assert_eq!(tasks[2].id, 1);
    }

    #[test]
    fn alternative_bullets_and_bom() {
        let md = "\u{feff}* [ ] star bullet\n+ [X] plus bullet\n";
        // The BOM is stripped by `codec::content`, before any decoder.
        let tasks = crate::codec::DbFormat::Markdown.decode(md).unwrap();
        assert_eq!(tasks[0].text, "star bullet");
        assert!(tasks[1].done);
    }

    #[test]
    fn foreign_comment_stays_in_text() {
        let tasks = decode("- [ ] keep <!-- note --> this <!-- id:5 -->\n").unwrap();
        assert_eq!(tasks[0].text, "keep <!-- note --> this");
        assert_eq!(tasks[0].id, 5);
    }

    #[test]
    fn after_ids_roundtrip_in_the_meta_comment() {
        let mut t = task(3, "blocked");
        t.after = vec![1, 2];
        let md = encode(&[t.clone()]);
        assert_eq!(md, "- [ ] blocked <!-- id:3 after:1,2 -->\n");
        assert_eq!(decode(&md).unwrap(), vec![t]);

        // A malformed after list rejects the whole comment (kept as text).
        let tasks = decode("- [ ] x <!-- id:3 after:oops -->\n").unwrap();
        assert_eq!(tasks[0].text, "x <!-- id:3 after:oops -->");
    }

    #[test]
    fn prose_between_items_does_not_leak_into_tasks() {
        let md = "- [ ] a <!-- id:1 -->\n\n## header\n  indented prose after header\n- [ ] b <!-- id:2 -->\n";
        let tasks = decode(md).unwrap();
        assert_eq!(tasks[0].text, "a");
        assert_eq!(tasks[1].text, "b");
    }

    #[test]
    fn empty_input() {
        assert_eq!(decode("").unwrap(), Vec::<Task>::new());
    }

    /// A text that is nothing but a date token comes back as that text,
    /// not as a date on a task without text.
    #[test]
    fn a_text_of_a_date_token_round_trips() {
        for text in ["@2026-01-01", " @2026-01-01"] {
            for (priority, date) in [(false, None), (true, None), (false, NaiveDate::from_ymd_opt(2026, 5, 5)), (true, NaiveDate::from_ymd_opt(2026, 5, 5))] {
                let mut t = task(1, text);
                t.priority = priority;
                t.date = date;
                let md = encode(std::slice::from_ref(&t));
                assert_eq!(decode(&md).unwrap(), vec![t], "{md:?}");
            }
        }
    }
}
