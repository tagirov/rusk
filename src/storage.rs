use anyhow::Result;
use chrono::NaiveDate;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::backend::Backend;
use crate::model::{Task, TaskId};
use crate::parse_cli_date_for_edit;
use crate::parser::date::is_cli_date_clear_value;

pub type MarkResult = (Vec<(TaskId, bool)>, Vec<TaskId>);

/// Manages task operations and persistence.
///
/// Every operation that changes tasks goes through [`update`]: the change
/// is applied to the current state of the database and stored as one step,
/// so commands running at the same time (another terminal, `rusk serve`, a
/// cron job) all take effect instead of the last one overwriting the rest.
///
/// [`update`]: Self::update
pub struct TaskManager {
    pub tasks: Vec<Task>,
    backend: Backend,
    /// The list as the database holds it as far as this manager knows:
    /// what was loaded, or written last. `tasks` differing from it means
    /// edits that are in memory only.
    synced: Mutex<Vec<Task>>,
}

/// An interactive edit was refused: the task is no longer what the editor
/// was opened with. Nothing was saved.
#[derive(Debug, PartialEq, Eq)]
pub struct TaskChanged {
    pub id: TaskId,
    /// The task is gone altogether.
    pub deleted: bool,
}

impl std::fmt::Display for TaskChanged {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let what = if self.deleted { "deleted" } else { "changed" };
        write!(
            f,
            "task {} was {what} by another process while it was being edited; nothing was saved",
            self.id
        )
    }
}

impl std::error::Error for TaskChanged {}

/// What became of a deletion the user confirmed task by task.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct ConfirmedDeletion {
    pub deleted: Vec<TaskId>,
    /// Still there, but no longer the task that was confirmed (another
    /// process changed it, or reused the id): left alone.
    pub changed: Vec<TaskId>,
    /// Already deleted by another process.
    pub gone: Vec<TaskId>,
    /// The tasks that stay and depended on a deleted one, with the ids
    /// they no longer depend on.
    pub unlinked: Vec<(TaskId, Vec<TaskId>)>,
}

/// What [`TaskManager::add_copies`] made of the tasks it was given.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Copied {
    /// The new id of each task, in the order they were given.
    pub ids: Vec<TaskId>,
    /// The copies that lost dependencies, by new id, with the ids (as the
    /// input gave them) of the tasks that did not come along.
    pub dropped: Vec<(TaskId, Vec<TaskId>)>,
}

/// Position of the task with this id.
pub fn position_of(tasks: &[Task], id: TaskId) -> Option<usize> {
    tasks.iter().position(|t| t.id == id)
}

/// The lowest id no task uses. A list that was read has unique nonzero ids
/// (see [`crate::model::normalize`]); one put together in memory might not,
/// and a repeated id or a 0 must not stop the search early.
pub fn next_free_id(tasks: &[Task]) -> Result<TaskId> {
    let mut used: Vec<TaskId> = tasks.iter().map(|t| t.id).collect();
    used.sort_unstable();

    let mut id: TaskId = 1;
    for &used_id in &used {
        if used_id > id {
            break;
        }
        if used_id == id {
            id = id
                .checked_add(1)
                .ok_or_else(|| anyhow::anyhow!("Maximum number of tasks reached"))?;
        }
    }

    Ok(id)
}

/// Adds `task` to the list. A list in id order — as it is while rusk alone
/// adds to it — stays in id order, so a new task that takes the id a deleted
/// one left free is shown in that one's place, not at the end (REVIEW №133).
/// A list someone has put in an order of their own (a hand-edited file)
/// keeps it, and the new task goes at the end.
pub fn insert_by_id(tasks: &mut Vec<Task>, task: Task) {
    let in_id_order = tasks.windows(2).all(|pair| pair[0].id < pair[1].id);
    let at = match in_id_order {
        true => tasks.iter().position(|t| t.id > task.id).unwrap_or(tasks.len()),
        false => tasks.len(),
    };
    tasks.insert(at, task);
}

/// A task text as it is stored: without whitespace at its edges, which the
/// list does not show and which would otherwise tell apart texts that look
/// the same (REVIEW №115). Whitespace inside is the user's.
pub fn clean_text(text: &str) -> &str {
    text.trim()
}

/// Whether storing `new` (cleaned) changes the text `stored`. A difference
/// in whitespace at the edges alone is none: a text stored before texts
/// were cleaned keeps its spaces until its words change.
pub fn text_changes(stored: &str, new: &str) -> bool {
    clean_text(stored) != clean_text(new)
}

/// Validates a `--after` dependency list against `tasks`: every id must
/// exist, self-references and cycles are rejected. Returns the list
/// deduplicated in input order. `own_id` is the task being edited (`None`
/// when adding: a new task cannot be in a cycle).
pub fn validate_after(
    tasks: &[Task],
    own_id: Option<TaskId>,
    after: &[TaskId],
) -> Result<Vec<TaskId>> {
    let mut clean: Vec<TaskId> = Vec::new();
    let mut missing: Vec<TaskId> = Vec::new();
    for &dep in after {
        if own_id == Some(dep) {
            anyhow::bail!("Task {dep} cannot depend on itself");
        }
        if clean.contains(&dep) || missing.contains(&dep) {
            continue;
        }
        if position_of(tasks, dep).is_some() {
            clean.push(dep);
        } else {
            missing.push(dep);
        }
    }
    if !missing.is_empty() {
        let list = missing
            .iter()
            .map(|id| id.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        anyhow::bail!("Cannot depend on missing task(s): {list}");
    }
    if let Some(own) = own_id {
        // Walk the dependency graph from the new deps; reaching the task
        // itself would make the ordering unsatisfiable.
        let mut stack: Vec<TaskId> = clean.clone();
        let mut seen: std::collections::HashSet<TaskId> = std::collections::HashSet::new();
        while let Some(cur) = stack.pop() {
            if cur == own {
                anyhow::bail!(
                    "Dependency cycle: task {own} would (transitively) depend on itself"
                );
            }
            if !seen.insert(cur) {
                continue;
            }
            if let Some(idx) = position_of(tasks, cur) {
                stack.extend(tasks[idx].after.iter().copied());
            }
        }
    }
    Ok(clean)
}

/// Takes the `removed` ids off every dependency list (deleted tasks must
/// not linger as dependencies: their ids get reused). Returns the tasks
/// that lost some, with the ids each one lost.
fn strip_deps(tasks: &mut [Task], removed: &[TaskId]) -> Vec<(TaskId, Vec<TaskId>)> {
    let mut unlinked = Vec::new();
    for task in tasks {
        let lost: Vec<TaskId> =
            task.after.iter().copied().filter(|dep| removed.contains(dep)).collect();
        if !lost.is_empty() {
            task.after.retain(|dep| !removed.contains(dep));
            unlinked.push((task.id, lost));
        }
    }
    unlinked
}

/// The tasks whose dependency list names `id`.
pub fn dependents_of(tasks: &[Task], id: TaskId) -> Vec<TaskId> {
    tasks.iter().filter(|t| t.after.contains(&id)).map(|t| t.id).collect()
}

/// Whether storing the dependency list `new` changes `stored`. The list is
/// a set: the same ids in another order are no change, and the stored
/// order stays.
pub fn deps_change(stored: &[TaskId], new: &[TaskId]) -> bool {
    let set = |ids: &[TaskId]| ids.iter().copied().collect::<std::collections::BTreeSet<_>>();
    set(stored) != set(new)
}

/// Deletes the tasks with these ids, along with every dependency on them.
/// Returns the ids no task had.
pub fn remove_tasks(tasks: &mut Vec<Task>, ids: &[TaskId]) -> Vec<TaskId> {
    let mut deleted = Vec::new();
    let mut not_found = Vec::new();
    for &id in ids {
        if let Some(idx) = position_of(tasks, id) {
            tasks.remove(idx);
            deleted.push(id);
        } else {
            not_found.push(id);
        }
    }
    strip_deps(tasks, &deleted);
    not_found
}

/// Which tasks `rusk rm` goes through when given a word instead of ids,
/// and the day of a task its bounds go by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// `rm done`: the done tasks, by the day they were marked done.
    Done,
    /// `rm all`: every task, open or done, by its due date — the one day an
    /// open task has.
    All,
}

impl Scope {
    /// The word on the command line.
    pub fn word(self) -> &'static str {
        match self {
            Scope::Done => "done",
            Scope::All => "all",
        }
    }

    pub fn covers(self, task: &Task) -> bool {
        match self {
            Scope::Done => task.done,
            Scope::All => true,
        }
    }

    /// The day the bounds compare: `done_on` for the done tasks, the due
    /// date for all; `None` when the task has none.
    pub fn day_of(self, task: &Task) -> Option<NaiveDate> {
        match self {
            Scope::Done => task.done_on,
            Scope::All => task.date,
        }
    }
}

/// The tasks `rusk rm done` / `rusk rm all` select: those of the scope
/// whose day lies from `newer` to `older`, both inclusive, either bound
/// optional (`--newer`, `--older`). With a bound set, a task of the scope
/// without that day — a done task without the day on record (done before
/// rusk recorded it, or ticked off in a file by hand), a task without a
/// due date — is of unknown age and not selected: the caller says how many
/// it kept.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Selection {
    pub scope: Scope,
    /// With a day on this one or before.
    pub older: Option<NaiveDate>,
    /// With a day on this one or after.
    pub newer: Option<NaiveDate>,
}

impl Selection {
    /// Every task of the scope, dated or not.
    pub fn every(scope: Scope) -> Self {
        Self { scope, older: None, newer: None }
    }

    pub fn is_bounded(&self) -> bool {
        self.older.is_some() || self.newer.is_some()
    }

    pub fn selects(&self, task: &Task) -> bool {
        if !self.scope.covers(task) {
            return false;
        }
        if !self.is_bounded() {
            return true;
        }
        self.scope.day_of(task).is_some_and(|on| {
            self.older.is_none_or(|day| on <= day) && self.newer.is_none_or(|day| on >= day)
        })
    }

    /// A task of the scope the bounds do not reach: no day on record.
    pub fn leaves_undated(&self, task: &Task) -> bool {
        self.is_bounded() && self.scope.covers(task) && self.scope.day_of(task).is_none()
    }
}

/// Deletes every task the selection selects, along with every dependency
/// on them. Returns how many there were.
pub fn remove_selected(tasks: &mut Vec<Task>, selection: Selection) -> usize {
    let ids: Vec<TaskId> = tasks.iter().filter(|t| selection.selects(t)).map(|t| t.id).collect();
    tasks.retain(|t| !ids.contains(&t.id));
    strip_deps(tasks, &ids);
    ids.len()
}

fn eprint_db_location(location: &str) {
    crate::errln!("{}", crate::config::theme().info.paint(&format!("Database path: {location}")));
}

impl TaskManager {
    fn is_test_mode() -> bool {
        crate::is_test_mode()
    }

    /// A debug run says which database it works on; under a test harness
    /// it keeps quiet.
    fn maybe_log_db_location(location: &str) {
        if cfg!(debug_assertions) && !Self::is_test_mode() {
            eprint_db_location(location);
        }
    }

    fn create_sample_tasks() -> Vec<Task> {
        let today = chrono::Local::now().date_naive();
        let yesterday = today - chrono::Duration::days(1);
        let tomorrow = today + chrono::Duration::days(1);
        let next_week = today + chrono::Duration::days(7);
        let last_week = today - chrono::Duration::days(7);

        vec![
            Task { id: 1, text: "Simple task without date".to_string(), date: None, done: false, done_on: None, priority: false, after: Vec::new() },
            Task { id: 2, text: "Completed task without date".to_string(), date: None, done: true, done_on: Some(last_week), priority: false, after: Vec::new() },
            Task { id: 3, text: "Overdue task from last week".to_string(), date: Some(last_week), done: false, done_on: None, priority: true, after: Vec::new() },
            Task { id: 4, text: "Completed overdue task".to_string(), date: Some(yesterday), done: true, done_on: Some(yesterday), priority: false, after: Vec::new() },
            Task { id: 5, text: "Task due today".to_string(), date: Some(today), done: false, done_on: None, priority: true, after: Vec::new() },
            Task { id: 6, text: "Completed task due today".to_string(), date: Some(today), done: true, done_on: Some(today), priority: false, after: Vec::new() },
            Task { id: 7, text: "Task due tomorrow".to_string(), date: Some(tomorrow), done: false, done_on: None, priority: false, after: Vec::new() },
            Task { id: 8, text: "Completed future task".to_string(), date: Some(next_week), done: true, done_on: Some(today), priority: false, after: Vec::new() },
            Task { id: 9, text: "Short".to_string(), date: None, done: false, done_on: None, priority: false, after: Vec::new() },
            Task { id: 10, text: "This is a very long task description that contains multiple words and demonstrates how the system handles longer text content".to_string(), date: Some(tomorrow), done: false, done_on: None, priority: false, after: Vec::new() },
            Task { id: 11, text: "Task with special chars: @#$%^&*()".to_string(), date: None, done: false, done_on: None, priority: false, after: Vec::new() },
            Task { id: 12, text: "Complete task 42 and review items 1-10".to_string(), date: Some(next_week), done: false, done_on: None, priority: false, after: Vec::new() },
            Task { id: 13, text: "Buy groceries: milk, bread, eggs, and cheese".to_string(), date: Some(tomorrow), done: false, done_on: None, priority: false, after: Vec::new() },
            Task { id: 14, text: "Long-term project milestone".to_string(), date: Some(today + chrono::Duration::days(30)), done: false, done_on: None, priority: false, after: Vec::new() },
        ]
    }

    /// `tasks` as just loaded from (or about to be the content of) `backend`.
    fn with(tasks: Vec<Task>, backend: Backend) -> Self {
        Self {
            synced: Mutex::new(tasks.clone()),
            tasks,
            backend,
        }
    }

    pub fn new() -> Result<Self> {
        let backend = Backend::resolve()?;
        let tasks = backend.load()?;
        Self::maybe_log_db_location(&backend.describe());
        let mut tm = Self::with(tasks, backend);

        if cfg!(debug_assertions) && !Self::is_test_mode() && tm.tasks.is_empty() {
            // Through `update`: debug runs starting at the same time all
            // find the database empty, and only the first one seeds it.
            tm.update(|tasks| {
                if tasks.is_empty() {
                    *tasks = Self::create_sample_tasks();
                }
                Ok(())
            })?;
        }

        Ok(tm)
    }

    /// Fresh manager without the debug sample-task seeding: used where the
    /// database is reloaded per operation (web server requests).
    pub fn open() -> Result<Self> {
        let backend = Backend::resolve()?;
        let tasks = backend.load()?;
        Ok(Self::with(tasks, backend))
    }

    pub fn new_for_restore() -> Result<Self> {
        let backend = Backend::resolve()?;
        Self::maybe_log_db_location(&backend.describe());
        Ok(Self::with(Vec::new(), backend))
    }

    pub fn new_empty() -> Result<Self> {
        // One path per call: tests run in parallel threads of one process
        // and must not share a database file.
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let db_path = crate::scratch::private_dir()?
            .join("test")
            .join(format!("{}-{seq}", std::process::id()))
            .join("tasks.json");
        Self::maybe_log_db_location(&db_path.display().to_string());
        Ok(Self::new_empty_with_path(db_path))
    }

    /// Manager over the local database at `path`, loaded (tests and tools).
    pub fn open_at(path: PathBuf) -> Result<Self> {
        let backend = Backend::from_local_path(path)?;
        let tasks = backend.load()?;
        Ok(Self::with(tasks, backend))
    }

    /// Empty manager over a local database at `path` (tests and tools).
    /// Panics on a database location this build cannot handle — the callers
    /// pass known-good extensions.
    pub fn new_empty_with_path(path: PathBuf) -> Self {
        let backend =
            Backend::from_local_path(path).expect("unsupported database path in this build");
        Self::with(Vec::new(), backend)
    }

    /// What the database holds once `tasks` are saved to it (see
    /// [`Backend::stored_form`]).
    pub fn stored_form(&self, tasks: &[Task]) -> Vec<Task> {
        self.backend.stored_form(tasks)
    }

    pub fn tasks(&self) -> &[Task] {
        &self.tasks
    }

    pub fn tasks_mut(&mut self) -> &mut Vec<Task> {
        &mut self.tasks
    }

    pub fn backend(&self) -> &Backend {
        &self.backend
    }

    /// The database file path for local backends; `None` for remote ones.
    pub fn local_path(&self) -> Option<&Path> {
        self.backend.local_path()
    }

    /// Where the database lives, for messages and logs.
    pub fn location(&self) -> String {
        self.backend.describe()
    }

    /// Adds a task and returns its id.
    pub fn add_task(&mut self, text: Vec<String>, date: Option<String>) -> Result<TaskId> {
        self.add_task_with_after(text, date, Vec::new())
    }

    /// Like [`add_task`](Self::add_task) with a `--after` dependency list
    /// (validated against the current database). A date of `_` is no date,
    /// as everywhere else a date is given.
    pub fn add_task_with_after(
        &mut self,
        text: Vec<String>,
        date: Option<String>,
        after: Vec<TaskId>,
    ) -> Result<TaskId> {
        let text = text.join(" ");
        let date = match date {
            Some(d) if !is_cli_date_clear_value(&d) => Some(parse_cli_date_for_edit(&d, None)?),
            _ => None,
        };
        self.add_task_full(text, date, after)
    }

    /// Like [`add_task`](Self::add_task) but with an already-parsed due date (avoids re-parsing after the editor).
    pub fn add_task_with_parsed_date(
        &mut self,
        text: String,
        date: Option<chrono::NaiveDate>,
    ) -> Result<TaskId> {
        self.add_task_full(text, date, Vec::new())
    }

    /// The full add: already-parsed date plus a dependency list. The id is
    /// picked, and the dependencies are validated, against the database as
    /// it is at the moment of the save. Returns the id.
    pub fn add_task_full(
        &mut self,
        text: String,
        date: Option<chrono::NaiveDate>,
        after: Vec<TaskId>,
    ) -> Result<TaskId> {
        let text = clean_text(&text);
        if text.is_empty() {
            anyhow::bail!("Task text cannot be empty");
        }
        self.update(|tasks| {
            let after = validate_after(tasks, None, &after)?;
            let id = next_free_id(tasks)?;
            insert_by_id(
                tasks,
                Task {
                    id,
                    text: text.to_string(),
                    date,
                    done: false,
                    done_on: None,
                    priority: false,
                    after,
                },
            );
            Ok(id)
        })
    }

    /// Adds `copies` — tasks of another database, as `rusk <IDS> --json`
    /// printed them — as new tasks, all in one step: each gets an id of
    /// this database's own and keeps its text, due date, priority and done
    /// flag (a day it was done on only with the flag). A dependency on a task
    /// among the copies points at that task's new id; one on any other task
    /// means nothing here and is dropped, and returned. A dependency on the
    /// task itself, and dependencies that make a cycle (a hand-written
    /// input), are refused, as `edit -a` refuses them.
    pub fn add_copies(&mut self, copies: &[Task]) -> Result<Copied> {
        for (n, copy) in copies.iter().enumerate() {
            if clean_text(&copy.text).is_empty() {
                anyhow::bail!("Task text cannot be empty (task {} of the input)", n + 1);
            }
        }
        self.update(|tasks| {
            let mut ids = Vec::with_capacity(copies.len());
            for copy in copies {
                let id = next_free_id(tasks)?;
                insert_by_id(
                    tasks,
                    Task {
                        id,
                        text: clean_text(&copy.text).to_string(),
                        date: copy.date,
                        done: copy.done,
                        done_on: copy.done_on.filter(|_| copy.done),
                        priority: copy.priority,
                        after: Vec::new(),
                    },
                );
                ids.push(id);
            }
            // The id a copy had where it came from, to its new one: an id
            // given twice is the first task's, and 0 is no id at all.
            let mut new_ids: std::collections::HashMap<TaskId, TaskId> =
                std::collections::HashMap::new();
            for (copy, &id) in copies.iter().zip(&ids) {
                if copy.id != 0 {
                    new_ids.entry(copy.id).or_insert(id);
                }
            }
            let mut dropped = Vec::new();
            for (copy, &id) in copies.iter().zip(&ids) {
                let mut after: Vec<TaskId> = Vec::new();
                let mut lost: Vec<TaskId> = Vec::new();
                for on in &copy.after {
                    match new_ids.get(on) {
                        Some(&dep) if dep == id => {
                            anyhow::bail!("Task {on} of the input cannot depend on itself")
                        }
                        Some(&dep) if !after.contains(&dep) => after.push(dep),
                        None if !lost.contains(on) => lost.push(*on),
                        // Named once already.
                        _ => {}
                    }
                }
                if let Some(idx) = position_of(tasks, id) {
                    tasks[idx].after = after;
                }
                if !lost.is_empty() {
                    dropped.push((id, lost));
                }
            }
            // Named by the ids the input gave: the new ones are never saved.
            // A copy without one is no dependency of anything, so never in
            // a cycle.
            for (copy, &id) in copies.iter().zip(&ids) {
                if let Some(idx) = position_of(tasks, id) {
                    let after = tasks[idx].after.clone();
                    if validate_after(tasks, Some(id), &after).is_err() {
                        anyhow::bail!(
                            "Dependency cycle in the input: task {} would (transitively) depend on itself",
                            copy.id
                        );
                    }
                }
            }
            Ok(Copied { ids, dropped })
        })
    }

    /// Validates a `--after` dependency list against the tasks in memory,
    /// see [`validate_after`].
    pub fn validate_after(&self, own_id: Option<TaskId>, after: &[TaskId]) -> Result<Vec<TaskId>> {
        validate_after(&self.tasks, own_id, after)
    }

    pub fn delete_tasks(&mut self, ids: Vec<TaskId>) -> Result<Vec<TaskId>> {
        let mut sorted_ids = ids;
        sorted_ids.sort_unstable_by(|a, b| b.cmp(a));

        self.update(|tasks| Ok(remove_tasks(tasks, &sorted_ids)))
    }

    /// Deletes the tasks the user confirmed one by one — `confirmed` holds
    /// them as they were shown — as far as they still are those tasks:
    /// `same_task(shown, now)` decides. While a prompt waits, another
    /// process may change a task or delete it and hand its id to a new
    /// one; such a task is reported instead of deleted.
    pub fn delete_confirmed(
        &mut self,
        confirmed: &[Task],
        same_task: impl Fn(&Task, &Task) -> bool,
    ) -> Result<ConfirmedDeletion> {
        self.update(|tasks| {
            let mut outcome = ConfirmedDeletion::default();
            for shown in confirmed {
                match position_of(tasks, shown.id) {
                    Some(idx) if same_task(shown, &tasks[idx]) => {
                        tasks.remove(idx);
                        outcome.deleted.push(shown.id);
                    }
                    Some(_) => outcome.changed.push(shown.id),
                    None => outcome.gone.push(shown.id),
                }
            }
            outcome.unlinked = strip_deps(tasks, &outcome.deleted);
            Ok(outcome)
        })
    }

    pub fn delete_all_done(&mut self) -> Result<usize> {
        self.update(|tasks| Ok(remove_selected(tasks, Selection::every(Scope::Done))))
    }

    /// Toggles `done` for the given task ids. A task marked done gets
    /// today as the day it was done on (what `rusk rm done --older` goes by);
    /// one marked undone loses it.
    pub fn mark_tasks(&mut self, ids: Vec<TaskId>) -> Result<MarkResult> {
        let today = chrono::Local::now().date_naive();
        self.toggle_tasks(ids, |task| {
            task.done = !task.done;
            task.done_on = task.done.then_some(today);
            task.done
        })
    }

    /// Toggles the `priority` flag for the given task ids. Returns `(Vec<(id, new_priority)>, not_found)`.
    /// Does not touch `done`: the priority is preserved across later done toggles.
    pub fn mark_priority_tasks(&mut self, ids: Vec<TaskId>) -> Result<MarkResult> {
        self.toggle_tasks(ids, |task| {
            task.priority = !task.priority;
            task.priority
        })
    }

    /// Shared toggle loop: `toggle` flips one flag on the task and returns its new state.
    fn toggle_tasks(
        &mut self,
        ids: Vec<TaskId>,
        toggle: impl Fn(&mut Task) -> bool,
    ) -> Result<MarkResult> {
        self.update(|tasks| {
            let mut not_found = Vec::new();
            let mut marked = Vec::new();
            for &id in &ids {
                if let Some(idx) = position_of(tasks, id) {
                    marked.push((id, toggle(&mut tasks[idx])));
                } else {
                    not_found.push(id);
                }
            }
            Ok((marked, not_found))
        })
    }

    pub fn edit_tasks(
        &mut self,
        ids: Vec<TaskId>,
        text: Option<Vec<String>>,
        date: Option<String>,
    ) -> Result<(Vec<TaskId>, Vec<TaskId>, Vec<TaskId>)> {
        self.edit_tasks_with_after(ids, text, date, None)
    }

    /// Like [`edit_tasks`](Self::edit_tasks) plus an optional new dependency
    /// list: `Some(vec![])` clears it, `None` leaves it untouched.
    pub fn edit_tasks_with_after(
        &mut self,
        ids: Vec<TaskId>,
        text: Option<Vec<String>>,
        date: Option<String>,
        after: Option<Vec<TaskId>>,
    ) -> Result<(Vec<TaskId>, Vec<TaskId>, Vec<TaskId>)> {
        let text = text.map(|words| clean_text(&words.join(" ")).to_string());
        // The same rule as for a new task (`rusk edit 1 "$EMPTY"`).
        if text.as_deref().is_some_and(str::is_empty) {
            anyhow::bail!("Task text cannot be empty");
        }

        self.update(|tasks| {
            let mut not_found = Vec::new();
            let mut edited = Vec::new();
            let mut unchanged = Vec::new();

            for &id in &ids {
                let Some(idx) = position_of(tasks, id) else {
                    not_found.push(id);
                    continue;
                };
                // Cycle detection depends on the task being edited, so the
                // list is validated per id before the task is borrowed.
                let new_after = match &after {
                    Some(list) => Some(validate_after(tasks, Some(id), list)?),
                    None => None,
                };

                let task = &mut tasks[idx];
                let mut was_changed = false;

                if let Some(new_text) = &text
                    && text_changes(&task.text, new_text)
                {
                    task.text = new_text.clone();
                    was_changed = true;
                }

                if let Some(new_date) = &date {
                    if is_cli_date_clear_value(new_date) {
                        if task.date.is_some() {
                            task.date = None;
                            was_changed = true;
                        }
                    } else {
                        // `+1d` counts from each task's own date, so one
                        // of several can be the one it fails for.
                        let parsed_date = parse_cli_date_for_edit(new_date, task.date)
                            .map_err(|e| anyhow::anyhow!("task {id}: {e:#}"))?;
                        if task.date != Some(parsed_date) {
                            task.date = Some(parsed_date);
                            was_changed = true;
                        }
                    }
                }

                if let Some(new_after) = new_after
                    && deps_change(&task.after, &new_after)
                {
                    task.after = new_after;
                    was_changed = true;
                }

                if was_changed {
                    edited.push(id);
                } else {
                    unchanged.push(id);
                }
            }

            Ok((edited, unchanged, not_found))
        })
    }

    /// Stores what an editor session made of a task, provided the task
    /// still has the text and date the editor was opened with
    /// ([`TaskChanged`] otherwise). Everything else about the task, and
    /// every other task, is taken as the database has it now.
    pub fn edit_task_as_seen(
        &mut self,
        id: TaskId,
        seen: (&str, Option<chrono::NaiveDate>),
        new: (&str, Option<chrono::NaiveDate>),
    ) -> Result<()> {
        self.update(|tasks| {
            let Some(idx) = position_of(tasks, id) else {
                return Err(TaskChanged { id, deleted: true }.into());
            };
            let task = &mut tasks[idx];
            if (task.text.as_str(), task.date) != seen {
                return Err(TaskChanged { id, deleted: false }.into());
            }
            task.text = clean_text(new.0).to_string();
            task.date = new.1;
            Ok(())
        })
    }

    pub fn find_task_by_id(&self, id: TaskId) -> Option<usize> {
        position_of(&self.tasks, id)
    }

    pub fn generate_next_id(&self) -> Result<TaskId> {
        next_free_id(&self.tasks)
    }

    /// Applies `change` to the task list and saves the result as one step.
    ///
    /// The change runs on the tasks in memory while the database still is
    /// what they were loaded from. If another process has written since,
    /// the database is read again and the change runs on that instead —
    /// so look tasks up by id inside `change`, never by an index taken
    /// before. It may run more than once and must have no effects of its
    /// own. On success `tasks` is the list that was saved.
    ///
    /// Nothing is written when `change` fails or changes nothing. Tasks
    /// edited in memory without a [`save`] cannot be carried over to a
    /// fresh read: if the database changed meanwhile, that is a
    /// [`StaleDatabase`](crate::StaleDatabase) error.
    ///
    /// [`save`]: Self::save
    pub fn update<T>(
        &mut self,
        mut change: impl FnMut(&mut Vec<Task>) -> Result<T>,
    ) -> Result<T> {
        let clean = self.tasks == *self.synced();
        let updated = self.backend.update(&self.tasks, clean, &mut change)?;
        // Edits that were in memory only, and still are because the change
        // gave no reason to write, must not pass for saved ones.
        if updated.stored {
            *self.synced() = updated.tasks.clone();
        }
        self.tasks = updated.tasks;
        Ok(updated.value)
    }

    fn synced(&self) -> std::sync::MutexGuard<'_, Vec<Task>> {
        self.synced.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Replaces the whole database with the tasks in memory. Refused with
    /// a [`StaleDatabase`](crate::StaleDatabase) error when another process
    /// changed the database after this manager loaded it; prefer
    /// [`update`](Self::update), which applies the change to the current
    /// state instead.
    pub fn save(&self) -> Result<()> {
        self.backend.save(&self.tasks)?;
        *self.synced() = self.tasks.clone();
        Ok(())
    }

    /// The directory of a local database: where what rusk keeps beside it
    /// (the editor's drafts) goes. `None` for a remote database, which has
    /// no directory on this machine (see `cli::editor::draft::dir_for`).
    pub fn local_db_dir() -> Option<PathBuf> {
        let backend = Backend::resolve().ok()?;
        let path = backend.local_path()?;
        Some(path.parent().unwrap_or(path).to_path_buf())
    }

    pub fn load_tasks_from_path(path: &Path) -> Result<Vec<Task>> {
        Backend::from_local_path(path.to_path_buf())?.load()
    }

    pub fn restore_from_backup(&mut self) -> Result<()> {
        self.tasks = self.backend.restore_from_backup()?;
        *self.synced() = self.tasks.clone();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{TaskManager, dependents_of, deps_change, strip_deps};
    use crate::model::{Task, TaskId};
    use chrono::NaiveDate;

    /// `rusk mark` records the day a task is done and forgets it when the
    /// task is marked undone; `rusk rm done --older` goes by it.
    #[test]
    fn marking_done_records_the_day_and_rm_reads_it() {
        use super::{Scope, Selection, remove_selected};
        let mut tm = TaskManager::new_empty().unwrap();
        for text in ["a", "b", "c"] {
            tm.add_task(vec![text.to_string()], None).unwrap();
        }
        let today = chrono::Local::now().date_naive();
        tm.mark_tasks(vec![1, 2]).unwrap();
        assert_eq!(tm.tasks()[0].done_on, Some(today));
        tm.mark_tasks(vec![2]).unwrap();
        assert!(!tm.tasks()[1].done && tm.tasks()[1].done_on.is_none());
        // Done ten days ago; done, day unknown; open.
        tm.update(|tasks| {
            tasks[0].done_on = Some(today - chrono::Duration::days(10));
            tasks[1].done = true;
            Ok(())
        })
        .unwrap();
        let week_ago = today - chrono::Duration::days(7);
        let older = Selection { scope: Scope::Done, older: Some(week_ago), newer: None };
        let all = Selection::every(Scope::Done);
        assert!(older.selects(&tm.tasks()[0]));
        assert!(!older.selects(&tm.tasks()[1]) && all.selects(&tm.tasks()[1]));
        assert!(older.leaves_undated(&tm.tasks()[1]) && !older.leaves_undated(&tm.tasks()[0]));
        assert!(!all.leaves_undated(&tm.tasks()[1]), "without bounds nothing is left for want of a day");
        assert!(!older.leaves_undated(&tm.tasks()[2]), "an open task is outside `done`");
        assert!(!all.selects(&tm.tasks()[2]));
        // Both bounds, inclusive: a range of one day holds the day itself.
        let ten_days_ago = today - chrono::Duration::days(10);
        let that_day = Selection { scope: Scope::Done, older: Some(ten_days_ago), newer: Some(ten_days_ago) };
        assert!(that_day.selects(&tm.tasks()[0]));
        let after = Selection { scope: Scope::Done, older: None, newer: Some(week_ago) };
        assert!(!after.selects(&tm.tasks()[0]) && !after.selects(&tm.tasks()[1]));
        let mut tasks = tm.tasks().to_vec();
        assert_eq!(remove_selected(&mut tasks, older), 1);
        assert_eq!(tasks.iter().map(|t| t.id).collect::<Vec<_>>(), [2, 3]);
        assert_eq!(remove_selected(&mut tasks, all), 1);
        assert_eq!(tasks.iter().map(|t| t.id).collect::<Vec<_>>(), [3]);
    }

    /// `rusk rm all` goes through every task, open or done, and its bounds
    /// by the due date: a task without one is kept and counted; the day a
    /// done task was marked done plays no part.
    #[test]
    fn rm_all_selects_every_task_and_bounds_by_the_due_date() {
        use super::{Scope, Selection, remove_selected};
        let today = chrono::Local::now().date_naive();
        let days_ago = |n: i64| today - chrono::Duration::days(n);
        let task = |id: TaskId, date: Option<NaiveDate>, done: bool, done_on: Option<NaiveDate>| Task {
            id,
            text: format!("t{id}"),
            date,
            done,
            done_on,
            priority: false,
            after: Vec::new(),
        };
        // Open, due ten days ago; open, undated; done long ago, due three
        // days ago; done long ago, undated; open, due tomorrow, after 1.
        let tasks = vec![
            task(1, Some(days_ago(10)), false, None),
            task(2, None, false, None),
            task(3, Some(days_ago(3)), true, Some(days_ago(30))),
            task(4, None, true, Some(days_ago(30))),
            Task { after: vec![1], ..task(5, Some(days_ago(-1)), false, None) },
        ];
        let every = Selection::every(Scope::All);
        assert!(tasks.iter().all(|t| every.selects(t)));
        assert!(tasks.iter().all(|t| !every.leaves_undated(t)));
        let older = Selection { scope: Scope::All, older: Some(days_ago(7)), newer: None };
        assert!(older.selects(&tasks[0]) && !older.selects(&tasks[2]) && !older.selects(&tasks[4]));
        assert!(!older.selects(&tasks[1]) && !older.selects(&tasks[3]), "no due date, no age");
        assert!(older.leaves_undated(&tasks[1]) && older.leaves_undated(&tasks[3]));
        assert!(!older.leaves_undated(&tasks[0]));
        let newer = Selection { scope: Scope::All, older: None, newer: Some(days_ago(7)) };
        assert!(!newer.selects(&tasks[0]) && newer.selects(&tasks[2]) && newer.selects(&tasks[4]));
        let that_day = Selection { scope: Scope::All, older: Some(days_ago(3)), newer: Some(days_ago(3)) };
        assert!(that_day.selects(&tasks[2]) && !that_day.selects(&tasks[0]));

        let mut left = tasks.clone();
        assert_eq!(remove_selected(&mut left, older), 1);
        assert_eq!(left.iter().map(|t| t.id).collect::<Vec<_>>(), [2, 3, 4, 5]);
        assert!(left[3].after.is_empty(), "the dependency on the deleted task goes with it");
        assert_eq!(remove_selected(&mut left, every), 4);
        assert!(left.is_empty());
    }

    #[test]
    fn add_task_with_parsed_date_roundtrip() {
        let mut tm = TaskManager::new_empty().unwrap();
        let d = NaiveDate::from_ymd_opt(2025, 6, 15).unwrap();
        tm.add_task_with_parsed_date("Hello".to_string(), Some(d))
            .unwrap();
        assert_eq!(tm.tasks[0].text, "Hello");
        assert_eq!(tm.tasks[0].date, Some(d));
    }

    fn tm_with_tasks(n: u32) -> TaskManager {
        // A path of its own: tests run in parallel, and a manager applies
        // its changes to whatever another writer left in a shared file.
        let mut tm = TaskManager::new_empty().unwrap();
        for i in 1..=n {
            tm.add_task(vec![format!("task {i}")], None).unwrap();
        }
        tm
    }

    #[test]
    fn the_next_free_id_is_free_even_in_a_list_with_repeats() {
        let with_ids = |ids: &[TaskId]| -> Vec<crate::model::Task> {
            ids.iter()
                .map(|&id| crate::model::Task {
                    id,
                    text: "t".into(),
                    date: None,
                    done: false,
                    done_on: None,
                    priority: false,
                    after: Vec::new(),
                })
                .collect()
        };
        assert_eq!(super::next_free_id(&with_ids(&[])).unwrap(), 1);
        assert_eq!(super::next_free_id(&with_ids(&[2, 1, 4])).unwrap(), 3);
        // REVIEW №14: a repeated id or a 0 ended the search on a taken id.
        assert_eq!(super::next_free_id(&with_ids(&[1, 1, 2])).unwrap(), 3);
        assert_eq!(super::next_free_id(&with_ids(&[0, 1])).unwrap(), 2);
    }

    #[test]
    fn a_new_task_goes_where_its_id_puts_it_in_a_list_in_id_order() {
        let ids = |tasks: &[crate::model::Task]| tasks.iter().map(|t| t.id).collect::<Vec<_>>();
        let new = |id: TaskId| crate::model::Task {
            id,
            text: format!("task {id}"),
            date: None,
            done: false,
            done_on: None,
            priority: false,
            after: Vec::new(),
        };
        let with = |ids: &[TaskId]| -> Vec<crate::model::Task> { ids.iter().map(|&id| new(id)).collect() };

        let mut tasks = with(&[1, 3, 4]);
        super::insert_by_id(&mut tasks, new(2));
        assert_eq!(ids(&tasks), [1, 2, 3, 4]);
        super::insert_by_id(&mut tasks, new(9));
        assert_eq!(ids(&tasks), [1, 2, 3, 4, 9]);
        let mut tasks = with(&[]);
        super::insert_by_id(&mut tasks, new(1));
        assert_eq!(ids(&tasks), [1]);

        // An order of the user's own is kept.
        let mut tasks = with(&[5, 1, 3]);
        super::insert_by_id(&mut tasks, new(2));
        assert_eq!(ids(&tasks), [5, 1, 3, 2]);
    }

    #[test]
    fn after_validation_rejects_missing_self_and_cycles() {
        let mut tm = tm_with_tasks(3);

        // Missing dependency ids are reported.
        let err = tm
            .add_task_with_after(vec!["x".into()], None, vec![9])
            .unwrap_err()
            .to_string();
        assert!(err.contains("missing task(s): 9"), "{err}");

        // Valid deps are stored deduplicated in input order.
        tm.add_task_with_after(vec!["y".into()], None, vec![2, 1, 2])
            .unwrap();
        assert_eq!(tm.tasks.last().unwrap().after, vec![2, 1]);

        // Self-reference and cycles are rejected on edit.
        let err = tm
            .edit_tasks_with_after(vec![1], None, None, Some(vec![1]))
            .unwrap_err()
            .to_string();
        assert!(err.contains("depend on itself"), "{err}");

        tm.edit_tasks_with_after(vec![1], None, None, Some(vec![2]))
            .unwrap();
        let err = tm
            .edit_tasks_with_after(vec![2], None, None, Some(vec![1]))
            .unwrap_err()
            .to_string();
        assert!(err.contains("cycle"), "{err}");

        // Clearing always works.
        tm.edit_tasks_with_after(vec![1], None, None, Some(vec![]))
            .unwrap();
        assert!(tm.tasks[0].after.is_empty());
    }

    #[test]
    fn a_dependency_list_is_a_set() {
        assert!(!deps_change(&[1, 2], &[2, 1]));
        assert!(!deps_change(&[], &[]));
        assert!(deps_change(&[1, 2], &[1]));
        assert!(deps_change(&[1], &[1, 2]));
        assert!(deps_change(&[1, 2], &[1, 3]));
    }

    #[test]
    fn a_deletion_says_which_lists_it_changed() {
        let mut tasks: Vec<Task> = (1..=4)
            .map(|id| Task {
                id,
                text: format!("task {id}"),
                date: None,
                done: false,
                done_on: None,
                priority: false,
                after: Vec::new(),
            })
            .collect();
        tasks[2].after = vec![1, 2];
        tasks[3].after = vec![2];
        assert_eq!(dependents_of(&tasks, 2), [3, 4]);
        assert_eq!(strip_deps(&mut tasks, &[1, 2]), [(3, vec![1, 2]), (4, vec![2])]);
        assert!(strip_deps(&mut tasks, &[1, 2]).is_empty());
    }

    #[test]
    fn deleting_tasks_strips_them_from_after_lists() {
        let mut tm = tm_with_tasks(3);
        tm.edit_tasks_with_after(vec![3], None, None, Some(vec![1, 2]))
            .unwrap();

        tm.delete_tasks(vec![1]).unwrap();
        assert_eq!(tm.tasks.iter().find(|t| t.id == 3).unwrap().after, vec![2]);

        tm.mark_tasks(vec![2]).unwrap();
        tm.delete_all_done().unwrap();
        assert!(tm.tasks.iter().find(|t| t.id == 3).unwrap().after.is_empty());
    }

    #[test]
    fn csv_db_roundtrip_via_task_manager() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.csv");
        let mut tm = TaskManager::new_empty_with_path(path.clone());
        let d = NaiveDate::from_ymd_opt(2026, 7, 8).unwrap();
        tm.add_task_with_parsed_date("CSV task, with \"quotes\"\nsecond line".to_string(), Some(d))
            .unwrap();
        tm.add_task_with_parsed_date("plain".to_string(), None)
            .unwrap();

        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.starts_with("id,text,date,done,priority"));

        let loaded = TaskManager::load_tasks_from_path(&path).unwrap();
        assert_eq!(loaded, tm.tasks);
    }

    #[cfg(feature = "fmt-markdown")]
    #[test]
    fn markdown_db_roundtrip_via_task_manager() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.md");
        let mut tm = TaskManager::new_empty_with_path(path.clone());
        tm.add_task_with_parsed_date(
            "markdown task\nwith continuation".to_string(),
            NaiveDate::from_ymd_opt(2026, 7, 15),
        )
        .unwrap();

        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(raw.starts_with("- [ ] markdown task @2026-07-15"), "{raw}");

        let loaded = TaskManager::load_tasks_from_path(&path).unwrap();
        assert_eq!(loaded, tm.tasks);
    }

    #[cfg(feature = "backend-sqlite")]
    #[test]
    fn sqlite_db_roundtrip_via_task_manager() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.db");
        let mut tm = TaskManager::new_empty_with_path(path.clone());
        tm.add_task_with_parsed_date("sqlite task".to_string(), None)
            .unwrap();

        let loaded = TaskManager::load_tasks_from_path(&path).unwrap();
        assert_eq!(loaded, tm.tasks);
    }
}
