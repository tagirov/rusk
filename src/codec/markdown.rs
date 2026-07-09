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
//! the task id (invisible in rendered Markdown). Hand-added items may omit
//! the id comment: the lowest free id is assigned on load. The file renders
//! as a normal task list on GitHub/Obsidian and can be edited by hand.
//!
//! Caveats: rusk owns the file — lines that are not list items or their
//! continuations (headers, prose) are ignored on load and dropped on the
//! next save. Task text that itself starts with a `!` token, ends with an
//! `@YYYY-MM-DD` token, or contains a line that looks like a `- [ ]` item is
//! indistinguishable from the markup and will be reinterpreted; use JSON if
//! that matters.

use super::assign_missing_ids;
use crate::model::Task;
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
        out.push_str(&format!(" <!-- id:{} -->\n", task.id));
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

/// Strips the trailing `<!-- id:N -->` comment; foreign comments stay in the
/// text.
fn take_id(content: &str) -> (Option<u8>, &str) {
    let trimmed = content.trim_end();
    if let Some(before_close) = trimmed.strip_suffix("-->")
        && let Some((text, inner)) = before_close.rsplit_once("<!--")
        && let Some(id) = inner.trim().strip_prefix("id:")
        && let Ok(id) = id.trim().parse::<u8>()
        && id != 0
    {
        return (Some(id), text.trim_end());
    }
    (None, trimmed)
}

/// Strips a trailing `@YYYY-MM-DD` token.
fn take_date(content: &str) -> (Option<NaiveDate>, &str) {
    if let Some((text, last)) = content.rsplit_once(' ')
        && let Some(date) = last.strip_prefix('@')
        && let Ok(date) = date.parse::<NaiveDate>()
    {
        return (Some(date), text.trim_end());
    }
    (None, content)
}

pub fn decode(data: &str) -> Result<Vec<Task>> {
    let lines: Vec<&str> = data.trim_start_matches('\u{feff}').lines().collect();
    let mut tasks: Vec<Task> = Vec::new();
    let mut in_task = false;

    for (i, raw) in lines.iter().enumerate() {
        let line = raw.trim_end_matches('\r');
        if let Some((done, content)) = item_start(line.trim_start()) {
            let (id, content) = take_id(content);
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
        let tasks = decode(md).unwrap();
        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0].id, 2);
        assert!(tasks[1].done);
        assert_eq!(tasks[1].id, 3);
        assert_eq!(tasks[2].id, 1);
    }

    #[test]
    fn alternative_bullets_and_bom() {
        let md = "\u{feff}* [ ] star bullet\n+ [X] plus bullet\n";
        let tasks = decode(md).unwrap();
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
}
