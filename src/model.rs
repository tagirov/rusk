use chrono::NaiveDate;
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::HashSet;

/// Task identifier. u32 keeps ids compact in every storage format and
/// converts losslessly to SQLite's i64 column type.
pub type TaskId = u32;

/// A task. In JSON (and NDJSON) only `text` is required: any other field that
/// is left out or `null` has its empty value, and a task without an id gets
/// one when the list is read (see [`normalize`]).
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Task {
    /// 0 only in a list just read from storage: "no id yet".
    #[serde(default, deserialize_with = "null_as_default")]
    pub id: TaskId,
    pub text: String,
    pub date: Option<NaiveDate>,
    #[serde(default, deserialize_with = "null_as_default")]
    pub done: bool,
    #[serde(default, deserialize_with = "null_as_default")]
    pub priority: bool,
    /// Ids of tasks this one depends on (`--after`): it should be done no
    /// earlier than them. Advisory — an ordering hint for agents/tooling;
    /// completion is never blocked. Skipped in JSON when empty so existing
    /// databases stay byte-identical until the feature is used.
    #[serde(
        default,
        deserialize_with = "null_as_default",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub after: Vec<TaskId>,
}

/// `null` reads as the field's empty value, the same as leaving it out.
fn null_as_default<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: Default + Deserialize<'de>,
{
    Ok(Option::<T>::deserialize(deserializer)?.unwrap_or_default())
}

/// The text a task that was stored without one is given, so that it shows
/// in the list and `rusk edit` / `rusk del` can reach it. It is saved like
/// any other text.
pub const NO_TEXT: &str = "(no text)";

/// Makes a task list read from storage into one rusk can hold, the same way
/// for every format and backend:
///
/// - every task has a text. A record with neither text (blank, or
///   whitespace only) nor id is no task — an empty `- [ ]`, a todo.txt line
///   of metadata only — and is left out. One that has an id was stored as a
///   task: it keeps its id, date, flags and dependencies, and gets
///   [`NO_TEXT`] for a text;
/// - every task has an id of its own: the first task with an id keeps it; a
///   later task with the same id, and one with none (0), gets the lowest
///   free id, in list order;
/// - a task depends only on other tasks that exist, each listed once.
///
/// Dependencies are settled against the ids the list itself gives, before
/// any id is handed out: a dependency on a task that is not there must not
/// come to mean the task that happens to get that id.
///
/// The same list always comes out the same, and a list that follows the
/// rules comes out unchanged. What had to change is returned.
pub fn normalize(tasks: &mut Vec<Task>) -> anyhow::Result<Repairs> {
    let mut repairs = Repairs::default();

    tasks.retain(|task| {
        let is_task = task.id != 0 || !task.text.trim().is_empty();
        if !is_task {
            repairs.skipped += 1;
        }
        is_task
    });
    let written: Vec<TaskId> = tasks.iter().map(|task| task.id).collect();
    let mut untitled: Vec<usize> = Vec::new();
    for (index, task) in tasks.iter_mut().enumerate() {
        if task.text.trim().is_empty() {
            task.text = NO_TEXT.to_string();
            untitled.push(index);
        }
    }

    let mut owned: HashSet<TaskId> = HashSet::new();
    let needs_id: Vec<bool> = tasks
        .iter()
        .map(|task| task.id == 0 || !owned.insert(task.id))
        .collect();

    let mut dropped: Vec<(usize, TaskId, DependencyProblem)> = Vec::new();
    for (index, task) in tasks.iter_mut().enumerate() {
        // The id the task was written with: for a task that shares it, a
        // dependency on it is one on itself as far as its writer could tell.
        let own = task.id;
        let mut listed: HashSet<TaskId> = HashSet::new();
        task.after.retain(|&on| {
            let problem = if !owned.contains(&on) {
                Some(DependencyProblem::Missing)
            } else if on == own {
                Some(DependencyProblem::Itself)
            } else if !listed.insert(on) {
                Some(DependencyProblem::Repeated)
            } else {
                None
            };
            if let Some(problem) = problem {
                dropped.push((index, on, problem));
            }
            problem.is_none()
        });
    }

    let mut next: TaskId = 1;
    for (task, needs_id) in tasks.iter_mut().zip(needs_id) {
        if !needs_id {
            continue;
        }
        while owned.contains(&next) {
            next = next
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("Maximum number of tasks reached"))?;
        }
        if task.id == 0 {
            repairs.numbered.push(next);
        } else {
            repairs.renumbered.push(Renumbered {
                was: task.id,
                now: next,
                text: task.text.clone(),
            });
        }
        task.id = next;
        owned.insert(next);
    }

    repairs.untitled = untitled
        .into_iter()
        .map(|index| Untitled {
            was: written[index],
            now: tasks[index].id,
        })
        .collect();
    repairs.dropped_dependencies = dropped
        .into_iter()
        .map(|(index, on, problem)| DroppedDependency {
            task: written[index],
            now: tasks[index].id,
            on,
            problem,
        })
        .collect();
    Ok(repairs)
}

/// What [`normalize`] changed in a list; empty when it followed the rules
/// already.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Repairs {
    /// Records with neither text nor id: no tasks, left out.
    pub skipped: usize,
    /// Tasks stored without text, now [`NO_TEXT`].
    pub untitled: Vec<Untitled>,
    /// Tasks that had the id of an earlier task.
    pub renumbered: Vec<Renumbered>,
    /// Ids given to tasks that had none — for the text formats the usual
    /// way to add a task by hand.
    pub numbered: Vec<TaskId>,
    /// Dependencies left out.
    pub dropped_dependencies: Vec<DroppedDependency>,
}

/// A task stored without text: the id it was written with and its id now
/// (they differ only when it shared its id with an earlier task).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Untitled {
    pub was: TaskId,
    pub now: TaskId,
}

/// A task that had the id of an earlier task and got a new one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Renumbered {
    pub was: TaskId,
    pub now: TaskId,
    pub text: String,
}

/// A dependency on `on` left out of a task's `after` list. `task` is the id
/// the task was written with (0 for none), `now` the id it has now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DroppedDependency {
    pub task: TaskId,
    pub now: TaskId,
    pub on: TaskId,
    pub problem: DependencyProblem,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DependencyProblem {
    /// No task has that id.
    Missing,
    /// The task itself.
    Itself,
    /// Listed before in the same list.
    Repeated,
}

/// How many entries of a list a message names before "and N more".
const NAMED: usize = 5;

impl Repairs {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// What to tell the user who reads the list at `location`: whatever
    /// changes what the stored list says about a task. The rest — ids for
    /// tasks added by hand, a dependency of a task on itself or one listed
    /// twice — is routine upkeep. Until the next save the file still says
    /// what it said, and every sentence here is true of it.
    pub fn warnings(&self, location: &str) -> Vec<String> {
        let mut warnings = Vec::new();
        if !self.renumbered.is_empty() {
            let moves = self.renumbered.iter().map(|r| {
                format!("\"{}\" (id {}) is task {} now", snippet(&r.text), r.was, r.now)
            });
            warnings.push(format!(
                "Warning: tasks in '{location}' share ids, so these got new ones: {}; the file \
                 keeps the old ids until the next save",
                enumerate(moves, ", ")
            ));
        }
        match self.untitled.as_slice() {
            [] => {}
            [one] => warnings.push(format!(
                "Warning: task {} in '{location}' has no text; rusk shows it as \"{NO_TEXT}\" — \
                 give it one with `rusk edit {} <text>`, or delete it",
                one.now, one.now
            )),
            many => warnings.push(format!(
                "Warning: tasks {} in '{location}' have no text; rusk shows them as \"{NO_TEXT}\" \
                 — give them one with `rusk edit <id> <text>`, or delete them",
                enumerate(many.iter().map(|u| u.now.to_string()), ", ")
            )),
        }
        match self.skipped {
            0 => {}
            1 => warnings.push(format!(
                "Warning: '{location}' has an item with neither text nor id; it is no task, and \
                 the next save leaves it out"
            )),
            n => warnings.push(format!(
                "Warning: '{location}' has {n} items with neither text nor id; they are no tasks, \
                 and the next save leaves them out"
            )),
        }
        let missing: Vec<&DroppedDependency> = self
            .dropped_dependencies
            .iter()
            .filter(|d| d.problem == DependencyProblem::Missing)
            .collect();
        // "Saved with": an id handed out on reading (an item added by hand
        // without one) does not count, though `rusk list` shows it.
        match missing.as_slice() {
            [] => {}
            [one] => warnings.push(format!(
                "Warning: in '{location}' task {} depends on id {}, which no task there was \
                 saved with; the dependency is dropped (the file keeps it until the next save)",
                one.now, one.on
            )),
            many => warnings.push(format!(
                "Warning: in '{location}' some tasks depend on ids no task there was saved with \
                 ({}); these dependencies are dropped (the file keeps them until the next save)",
                enumerate(many.iter().map(|d| format!("task {} on {}", d.now, d.on)), ", ")
            )),
        }
        warnings
    }

    /// Everything that is not as it should be, for a client that sent the
    /// list and has to fix it. Ids are the ones the client sent.
    pub fn problems(&self) -> String {
        let mut problems: Vec<String> = Vec::new();
        match self.skipped {
            0 => {}
            1 => problems.push("a task has neither text nor id".to_string()),
            n => problems.push(format!("{n} tasks have neither text nor id")),
        }
        for untitled in &self.untitled {
            problems.push(format!("task {} has no text", untitled.was));
        }
        for renumbered in &self.renumbered {
            problems.push(format!(
                "task id {} is used more than once (ids must be unique)",
                renumbered.was
            ));
        }
        match self.numbered.len() {
            0 => {}
            1 => problems.push("a task is without an id (ids start at 1)".to_string()),
            n => problems.push(format!("{n} tasks are without an id (ids start at 1)")),
        }
        for dropped in &self.dropped_dependencies {
            let task = match dropped.task {
                0 => "a task without an id".to_string(),
                id => format!("task {id}"),
            };
            let on = dropped.on;
            problems.push(match dropped.problem {
                DependencyProblem::Missing => {
                    format!("{task} depends on task {on}, which does not exist")
                }
                DependencyProblem::Itself => format!("{task} depends on itself"),
                DependencyProblem::Repeated => format!("{task} lists task {on} more than once"),
            });
        }
        // The same fault can be found more than once (an id used three
        // times, a task listed as its own dependency twice).
        let mut seen: HashSet<String> = HashSet::new();
        problems.retain(|problem| seen.insert(problem.clone()));
        enumerate(problems.into_iter(), "; ")
    }
}

/// The items joined by `separator`; past [`NAMED`] of them the rest is
/// counted: "a, b, c, d, e, and 3 more".
fn enumerate(items: impl Iterator<Item = String>, separator: &str) -> String {
    let items: Vec<String> = items.collect();
    let named = items.len().min(NAMED);
    let mut text = items[..named].join(separator);
    if items.len() > named {
        text.push_str(&format!("{separator}and {} more", items.len() - named));
    }
    text
}

/// The first line of a task text, short enough for a message and escaped
/// for the terminal it is printed on.
fn snippet(text: &str) -> String {
    const MAX_CHARS: usize = 40;
    let text = crate::printable::escape(text);
    let line = text.trim().lines().next().unwrap_or_default();
    if line.chars().count() > MAX_CHARS {
        let cut: String = line.chars().take(MAX_CHARS - 1).collect();
        format!("{}…", cut.trim_end())
    } else {
        line.to_string()
    }
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
            priority: false,
            after: Vec::new(),
        }
    }

    fn with_after(mut task: Task, after: &[TaskId]) -> Task {
        task.after = after.to_vec();
        task
    }

    fn ids(tasks: &[Task]) -> Vec<TaskId> {
        tasks.iter().map(|t| t.id).collect()
    }

    #[test]
    fn a_list_that_follows_the_rules_comes_out_unchanged() {
        let list = vec![
            task(2, "b"),
            with_after(task(1, "a"), &[2]),
            with_after(task(7, "c"), &[1, 2]),
            task(3, NO_TEXT),
        ];
        let mut tasks = list.clone();
        assert!(normalize(&mut tasks).unwrap().is_empty());
        assert_eq!(tasks, list);
    }

    #[test]
    fn missing_and_shared_ids_get_the_lowest_free_ones() {
        let mut tasks = vec![task(2, "a"), task(0, "b"), task(2, "c"), task(1, "d")];
        let repairs = normalize(&mut tasks).unwrap();
        // Every id the list gives (2 and 1) is kept before any is handed
        // out, so the missing id and the second 2 get 3 and 4.
        assert_eq!(ids(&tasks), [2, 3, 4, 1]);
        assert_eq!(repairs.numbered, [3]);
        assert_eq!(
            repairs.renumbered,
            [Renumbered { was: 2, now: 4, text: "c".into() }]
        );

        // Again: nothing left to repair, the same ids.
        let once = tasks.clone();
        assert!(normalize(&mut tasks).unwrap().is_empty());
        assert_eq!(tasks, once);
    }

    /// Without text and without id a record is markup left behind; with an
    /// id it is a task that was stored without text, and everything else
    /// it has is kept.
    #[test]
    fn records_without_text() {
        let mut stored = with_after(task(4, " \n\t"), &[1]);
        stored.done = true;
        stored.date = NaiveDate::from_ymd_opt(2026, 12, 24);
        let mut tasks = vec![task(0, ""), task(1, "a"), stored.clone(), task(0, "  ")];
        let repairs = normalize(&mut tasks).unwrap();
        assert_eq!(repairs.skipped, 2);
        assert_eq!(repairs.untitled, [Untitled { was: 4, now: 4 }]);
        assert_eq!(tasks, [task(1, "a"), Task { text: NO_TEXT.into(), ..stored }]);
    }

    #[test]
    fn dependencies_only_on_other_tasks_that_exist_each_once() {
        let mut tasks = vec![
            with_after(task(1, "a"), &[1, 2, 2, 9, 0, 3]),
            task(2, "b"),
            with_after(task(3, "c"), &[1]),
        ];
        let repairs = normalize(&mut tasks).unwrap();
        assert_eq!(tasks[0].after, [2, 3]);
        assert_eq!(tasks[2].after, [1]);
        let problems: Vec<(TaskId, DependencyProblem)> = repairs
            .dropped_dependencies
            .iter()
            .map(|d| (d.on, d.problem))
            .collect();
        use DependencyProblem::*;
        assert_eq!(problems, [(1, Itself), (2, Repeated), (9, Missing), (0, Missing)]);
    }

    /// A dependency that leads nowhere is dropped before ids are handed
    /// out: the task that gets the free id must not inherit it.
    #[test]
    fn a_new_id_never_picks_up_a_dangling_dependency() {
        let mut tasks = vec![with_after(task(2, "blocked"), &[1]), task(0, "typed by hand")];
        normalize(&mut tasks).unwrap();
        assert_eq!(ids(&tasks), [2, 1]);
        assert!(tasks[0].after.is_empty());

        // The same for the second of two tasks with one id: "b" was written
        // as 1, so its dependency on 1 was one on itself.
        let mut tasks = vec![task(1, "a"), with_after(task(1, "b"), &[1]), with_after(task(3, "c"), &[1])];
        normalize(&mut tasks).unwrap();
        assert_eq!(ids(&tasks), [1, 2, 3]);
        assert!(tasks[1].after.is_empty());
        assert_eq!(tasks[2].after, [1], "a dependency on a shared id means the first task");
    }

    #[test]
    fn only_what_changes_a_task_is_worth_a_warning() {
        let mut tasks = vec![
            task(0, "typed by hand"),
            with_after(task(2, "b"), &[2]),
            with_after(task(3, "c"), &[2, 2]),
        ];
        let repairs = normalize(&mut tasks).unwrap();
        assert!(!repairs.is_empty());
        assert!(repairs.warnings("x.md").is_empty(), "{:?}", repairs.warnings("x.md"));

        let mut tasks = vec![
            task(1, "alpha"),
            task(1, "a very long text that goes on and on and on, past the cut\nsecond line"),
            task(5, ""),
            task(0, " "),
            with_after(task(6, "f"), &[9]),
        ];
        let warnings = normalize(&mut tasks).unwrap().warnings("/db/tasks.csv");
        assert_eq!(warnings.len(), 4, "{warnings:?}");
        assert!(
            warnings[0].contains("'/db/tasks.csv'")
                && warnings[0].contains("\"a very long text that goes on and on an…\" (id 1) is task 2 now"),
            "{}",
            warnings[0]
        );
        assert!(warnings[1].contains("task 5 in '/db/tasks.csv' has no text"), "{}", warnings[1]);
        assert!(warnings[1].contains("`rusk edit 5 <text>`"), "{}", warnings[1]);
        assert!(warnings[2].contains("an item with neither text nor id"), "{}", warnings[2]);
        assert!(
            warnings[3].contains("task 6 depends on id 9, which no task there was saved with"),
            "{}",
            warnings[3]
        );
    }

    #[test]
    fn long_lists_are_cut_short_in_messages() {
        let mut tasks: Vec<Task> = (0..8).map(|_| task(1, "same")).collect();
        let repairs = normalize(&mut tasks).unwrap();
        let warning = &repairs.warnings("t.json")[0];
        assert!(warning.contains("is task 6 now, and 2 more;"), "{warning}");
        assert_eq!(
            repairs.problems(),
            "task id 1 is used more than once (ids must be unique)",
            "the same fault is named once"
        );
    }

    #[test]
    fn problems_name_every_rule_that_is_broken() {
        let mut tasks = vec![
            task(1, "a"),
            with_after(task(1, "b"), &[4]),
            task(0, "c"),
            task(3, ""),
            task(0, ""),
        ];
        let problems = normalize(&mut tasks).unwrap().problems();
        assert_eq!(
            problems,
            "a task has neither text nor id; task 3 has no text; task id 1 is used more than once \
             (ids must be unique); a task is without an id (ids start at 1); task 1 depends on \
             task 4, which does not exist"
        );
    }

    #[test]
    fn json_needs_only_the_text() {
        let tasks: Vec<Task> = serde_json::from_str(
            r#"[{"text":"a"},{"id":null,"text":"b","date":null,"done":null,"priority":null,"after":null}]"#,
        )
        .unwrap();
        assert_eq!(tasks, [task(0, "a"), task(0, "b")]);
        assert!(serde_json::from_str::<Vec<Task>>(r#"[{"id":1}]"#).is_err());
        assert!(serde_json::from_str::<Vec<Task>>(r#"[{"id":1,"text":null}]"#).is_err());
        // Wrong types stay errors: absent is not the same as malformed.
        assert!(serde_json::from_str::<Vec<Task>>(r#"[{"text":"a","done":"yes"}]"#).is_err());
    }
}
