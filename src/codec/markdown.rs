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
//! Numbered items (`1. [ ]`) are tasks too, and so are the other checkbox
//! marks of one character of Obsidian and its themes: `[-]` (cancelled) is
//! done, `[/]`, `[>]`, `[?]`, `[b]` and the rest are not; rusk writes every
//! task back as `- [ ]` or `- [x]` (REVIEW №89). That holds for items at
//! the start of a line: an indented line is a continuation of the task
//! above unless it is a bulleted `[ ]`/`[x]` item, as before. A
//! continuation line may be indented with a tab as well (REVIEW №90).
//!
//! Caveats: rusk owns the file — lines that are not list items or their
//! continuations (headers, prose) are ignored on load and dropped on the
//! next save, and so is an item with neither text nor id comment (an empty
//! `- [ ]`, see [`crate::model::normalize`]). Task text that itself starts
//! with a `!` token, whose first line ends with an `@YYYY-MM-DD` token
//! after a space, or that contains a line that looks like a `- [ ]` item is
//! indistinguishable from the markup and will be reinterpreted; use JSON if
//! that matters. A CR right before a line break of a text (a CRLF) is not
//! kept: Windows editors end every line of the file that way. Nor are the
//! spaces in front of a first line that is nothing but a date token (see
//! [`encode`]). The rest of a text is: rusk takes off exactly what it
//! writes around it (REVIEW №31, №88), so an empty first line, spaces at
//! the start or the end of a line and the `! ` in front of an empty first
//! line come back as they were.

use crate::model::{Task, TaskId};
use anyhow::Result;
use chrono::NaiveDate;

/// A text is written as it is, with one exception: a first line that is
/// nothing but spaces and a date token, of a task without a due date, is
/// written without the spaces — with them it is what an empty first line
/// and a due date look like (REVIEW №31), without them it reads as the
/// text it is.
pub fn encode(tasks: &[Task]) -> String {
    let mut out = String::new();
    for task in tasks {
        let mut lines = task.text.split('\n');
        let mut first = lines.next().unwrap_or("");
        let lone_date = |line: &str| {
            line.trim().strip_prefix('@').is_some_and(|d| d.parse::<NaiveDate>().is_ok())
        };
        if task.date.is_none() && first.starts_with(char::is_whitespace) && lone_date(first) {
            first = first.trim_start();
        }
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

/// A bullet (`-`, `*`, `+`) and the space after it; the rest of the line.
fn after_bullet(line: &str) -> Option<&str> {
    line.strip_prefix("- ")
        .or_else(|| line.strip_prefix("* "))
        .or_else(|| line.strip_prefix("+ "))
}

/// A list item's marker: a bullet or a number (`1.`, `1)`), and the space
/// after it; the rest of the line.
fn after_marker(line: &str) -> Option<&str> {
    if let Some(rest) = after_bullet(line) {
        return Some(rest);
    }
    let digits = line.find(|c: char| !c.is_ascii_digit()).unwrap_or(line.len());
    if digits == 0 || digits > 9 {
        return None;
    }
    let rest = &line[digits..];
    rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") "))
}

/// The content after a list item's checkbox, with the done flag (see the
/// module docs for the marks). An indented line starts an item only as a
/// bulleted `[ ]` or `[x]`, the one form read before numbered items and
/// other marks were (review of R23): it is how rusk writes the continuation
/// lines of a text, and a text that has such a line keeps it.
fn item_start(line: &str) -> Option<(bool, &str)> {
    let trimmed = line.trim_start();
    let indented = trimmed.len() != line.len();
    let rest = if indented { after_bullet(trimmed)? } else { after_marker(trimmed)? };
    let mut chars = rest.chars();
    let (Some('['), Some(mark), Some(']')) = (chars.next(), chars.next(), chars.next()) else {
        return None;
    };
    let content = chars.as_str();
    let done = match mark {
        ' ' => false,
        'x' | 'X' => true,
        _ if indented => return None,
        // Any other mark of one character, as Obsidian and its themes have
        // them (`[/]`, `[>]`, `[?]`, `[b]`, …), with a space or nothing
        // after it: `[-]` (cancelled) is done, the others are not. A digit
        // (`[1]`, a reference) or a link (`[a](…)`) is no checkbox.
        mark if mark.is_whitespace() || mark.is_ascii_digit() || matches!(mark, '[' | ']') => {
            return None;
        }
        _ if !(content.is_empty() || content.starts_with(' ')) => return None,
        mark => mark == '-',
    };
    Some((done, content.strip_prefix(' ').unwrap_or(content)))
}

/// A continuation line of a multi-line task without its indent: two
/// spaces, as rusk writes it, or a tab.
fn continuation(line: &str) -> Option<&str> {
    line.strip_prefix("  ").or_else(|| line.strip_prefix('\t'))
}

/// Strips the trailing `<!-- id:N -->` / `<!-- id:N after:A,B -->` metadata
/// comment and the one space rusk writes before it — no more: the spaces
/// in front of that are the text's own (REVIEW №88), and so is a `! ` in
/// front of an empty first line (REVIEW №31). Foreign comments stay in the
/// text.
fn take_meta(content: &str) -> (Option<TaskId>, Vec<TaskId>, &str) {
    let trimmed = content.trim_end();
    if let Some(before_close) = trimmed.strip_suffix("-->")
        && let Some((text, inner)) = before_close.rsplit_once("<!--")
        && let Some((id, after)) = parse_meta_comment(inner)
    {
        return (Some(id), after, text.strip_suffix(' ').unwrap_or(text));
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
/// whose whole text is `@2026-01-01`). The one space rusk writes before the
/// token goes with it; what is in front of that is the text's: nothing, for
/// a task without text, and `! ` for one with priority.
fn take_date(content: &str) -> (Option<NaiveDate>, &str) {
    if let Some((text, last)) = content.trim_end().rsplit_once(' ')
        && text != "!"
        && let Some(date) = last.strip_prefix('@')
        && let Ok(date) = date.parse::<NaiveDate>()
    {
        return (Some(date), text);
    }
    (None, content)
}

pub fn decode(data: &str) -> Result<Vec<Task>> {
    let lines: Vec<&str> = data.lines().collect();
    let mut tasks: Vec<Task> = Vec::new();
    let mut in_task = false;

    for (i, raw) in lines.iter().enumerate() {
        let line = raw.trim_end_matches('\r');
        if let Some((done, content)) = item_start(line) {
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
        } else if in_task && let Some(rest) = continuation(line) {
            tasks.last_mut().expect("in_task implies a task").text += &format!("\n{rest}");
        } else if in_task && line.trim().is_empty() {
            // A blank line continues the task only when an indented
            // continuation follows; otherwise it just separates items.
            let continues = lines[i + 1..]
                .iter()
                .map(|l| l.trim_end_matches('\r'))
                .find(|l| !l.trim().is_empty())
                .is_some_and(|l| continuation(l).is_some() && item_start(l).is_none());
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

    /// REVIEW №31: a priority task whose first line is empty lost the flag,
    /// and its `!` went into the text. Such a file (rusk wrote them) reads
    /// right; rusk writes the text from its first line that is not empty.
    #[test]
    fn a_priority_task_with_an_empty_first_line_keeps_it() {
        let tasks = decode("- [ ] !  <!-- id:1 -->\n  Actual text\n").unwrap();
        assert!(tasks[0].priority);
        assert_eq!(tasks[0].text, "\nActual text");
        // Review of R23: what the encoder writes of a text is the text, so
        // nothing of a later line moves up to where it reads as markup.
        let texts = ["\n  Actual text", "\n! call mom", " ! x", "\nx @2026-01-01", " !", "  indented", "\n\n"];
        for text in texts {
            for date in [None, NaiveDate::from_ymd_opt(2026, 5, 5)] {
                for priority in [false, true] {
                    let mut t = task(1, text);
                    t.priority = priority;
                    t.date = date;
                    let md = encode(std::slice::from_ref(&t));
                    assert_eq!(decode(&md).unwrap(), vec![t], "{md:?}");
                }
            }
        }
        // What 0.7.3 wrote for an empty first line reads the same.
        let tasks = decode("- [ ]  <!-- id:1 -->\n  ! call mom\n").unwrap();
        assert!(!tasks[0].priority);
        assert_eq!(tasks[0].text, "\n! call mom");
        // A text that is nothing but `!` stays that text; no text is none.
        for (text, date) in [("!", None), ("", NaiveDate::from_ymd_opt(2026, 5, 5))] {
            let mut t = task(1, text);
            t.date = date;
            assert_eq!(decode(&encode(std::slice::from_ref(&t))).unwrap(), vec![t]);
        }
    }

    /// REVIEW №88: spaces at the end of the first line, a line of spaces
    /// and a CR inside a line went.
    #[test]
    fn a_text_keeps_its_spaces() {
        for text in ["first   \nsecond", "last  ", "cr\rinside", "tail cr\r\nnext", "two\n\n  indented"] {
            for date in [None, NaiveDate::from_ymd_opt(2026, 5, 5)] {
                let mut t = task(1, text);
                t.date = date;
                let md = encode(std::slice::from_ref(&t));
                assert_eq!(decode(&md).unwrap(), vec![t], "{md:?}");
            }
        }
        // By hand, spaces before the comment or the date still read.
        let tasks = decode("- [ ] task @2026-01-01   <!-- id:3 -->\n- [ ] plain   \n").unwrap();
        assert_eq!(tasks[0].date, NaiveDate::from_ymd_opt(2026, 1, 1));
        assert_eq!(tasks[0].text, "task");
        assert_eq!(tasks[1].text, "plain");
    }

    /// REVIEW №89: numbered items and marks other than `[ ]`/`[x]` were no
    /// tasks, and the next save dropped them.
    #[test]
    fn numbered_items_and_other_marks_are_tasks() {
        let md = "# Todo\n\n1. [ ] ordered item\n2) [x] ordered done\n- [-] cancelled item\n\
                  - [/] in progress\n* [>] deferred\n- [ ] normal item\n- [1] a reference\n\
                  - [link](http://example.com)\n10. no checkbox\n- [a](http://example.com)\n\
                  * [b] bookmark\n- [\"]\n";
        let tasks = decode(md).unwrap();
        let shown: Vec<(&str, bool)> = tasks.iter().map(|t| (t.text.as_str(), t.done)).collect();
        assert_eq!(
            shown,
            [
                ("ordered item", false),
                ("ordered done", true),
                ("cancelled item", true),
                ("in progress", false),
                ("deferred", false),
                ("normal item", false),
                ("bookmark", false),
                ("", false),
            ]
        );
        // Review of R23: an indented line is a continuation, as it was before
        // these were read, unless it is a bulleted `[ ]`/`[x]` item: a text
        // rusk wrote with such a line keeps it.
        let t = task(1, "Plan\n- [-] dropped idea\n1. [ ] step\n  * [b] note");
        assert_eq!(decode(&encode(std::slice::from_ref(&t))).unwrap(), vec![t]);
        let tasks = decode("- [ ] a <!-- id:1 -->\n  - [ ] nested\n").unwrap();
        assert_eq!(tasks.len(), 2);
    }

    /// REVIEW №90: a continuation line indented with a tab was dropped.
    #[test]
    fn a_tab_indents_a_continuation_too() {
        let tasks = decode("- [ ] first <!-- id:1 -->\n\tsecond line\n\n\tafter a blank\n- [ ] other <!-- id:2 -->\n").unwrap();
        assert_eq!(tasks[0].text, "first\nsecond line\n\nafter a blank");
        assert_eq!(tasks[1].text, "other");
        // Written back with two spaces.
        assert!(encode(&tasks).contains("\n  second line\n"));
    }

    /// A text that is nothing but a date token comes back as that text,
    /// not as a date on a task without text (the spaces in front of it are
    /// not written: with them, it is how an empty first line and a due date
    /// look).
    #[test]
    fn a_text_of_a_date_token_round_trips() {
        for text in ["@2026-01-01", " @2026-01-01", "\t @2026-01-01 "] {
            for (priority, date) in [(false, None), (true, None), (false, NaiveDate::from_ymd_opt(2026, 5, 5)), (true, NaiveDate::from_ymd_opt(2026, 5, 5))] {
                let mut t = task(1, text);
                t.priority = priority;
                t.date = date;
                let md = encode(std::slice::from_ref(&t));
                if date.is_none() {
                    t.text = text.trim_start().into();
                }
                assert_eq!(decode(&md).unwrap(), vec![t], "{md:?}");
            }
        }
        let tasks = decode("- [ ]  @2026-01-01 <!-- id:1 -->\n  buy milk\n").unwrap();
        assert_eq!((tasks[0].date, tasks[0].text.as_str()), (NaiveDate::from_ymd_opt(2026, 1, 1), "\nbuy milk"));
    }
}
