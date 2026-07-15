//! SQLite backend (`tasks.db` / `.sqlite` / `.sqlite3`), feature
//! `backend-sqlite`. One `tasks` table, whole database per save inside a
//! transaction — same load/save contract as the file formats, but with
//! real concurrent-writer safety: simultaneous CLI and `rusk serve` writes
//! serialize on the SQLite lock instead of racing on a file rename.
//!
//! `pos` keeps the display order (insertion order, independent of the
//! reusable ids). Foreign writers may INSERT without `pos` — rowid
//! semantics append them at the end. `"after"` holds the comma-separated
//! ids of the tasks a task depends on (NULL when none); databases created
//! before that column exist are migrated in place on open.

use crate::model::{Task, TaskId};
use anyhow::{Context, Result};
use rusqlite::Connection;
use std::path::{Path, PathBuf};

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS tasks (
    pos      INTEGER PRIMARY KEY,
    id       INTEGER NOT NULL UNIQUE CHECK (id > 0),
    text     TEXT NOT NULL,
    date     TEXT,
    done     INTEGER NOT NULL DEFAULT 0,
    priority INTEGER NOT NULL DEFAULT 0,
    \"after\" TEXT
)";

#[derive(Debug)]
pub struct SqliteBackend {
    path: PathBuf,
}

impl SqliteBackend {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    fn open(&self) -> Result<Connection> {
        let conn = Connection::open(&self.path)
            .with_context(|| format!("Failed to open SQLite database '{}'", self.path.display()))?;
        conn.execute(SCHEMA, [])
            .context("Failed to create the tasks table")?;
        // Databases created before the dependency feature lack the "after"
        // column; add it in place (ALTER is cheap and preserves the data).
        let has_after = conn
            .prepare("SELECT 1 FROM pragma_table_info('tasks') WHERE name = 'after'")
            .and_then(|mut stmt| stmt.exists([]))
            .context("Failed to inspect the tasks table")?;
        if !has_after {
            conn.execute("ALTER TABLE tasks ADD COLUMN \"after\" TEXT", [])
                .context("Failed to add the after column")?;
        }
        Ok(conn)
    }

    pub fn load(&self) -> Result<Vec<Task>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let conn = self.open()?;
        let mut stmt = conn
            .prepare("SELECT id, text, date, done, priority, \"after\" FROM tasks ORDER BY pos")
            .context("Failed to query the tasks table")?;
        let tasks = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, TaskId>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, bool>(3)?,
                    row.get::<_, bool>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            })
            .context("Failed to read the tasks table")?
            .collect::<rusqlite::Result<Vec<_>>>()
            .with_context(|| {
                format!(
                    "Failed to parse the SQLite database at '{}'",
                    self.path.display()
                )
            })?
            .into_iter()
            .map(|(id, text, date, done, priority, after)| {
                let date = date
                    .map(|d| {
                        d.parse::<chrono::NaiveDate>()
                            .with_context(|| format!("task {id}: invalid date '{d}'"))
                    })
                    .transpose()?;
                let after = after
                    .as_deref()
                    .unwrap_or("")
                    .split(',')
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| {
                        s.trim()
                            .parse::<TaskId>()
                            .with_context(|| format!("task {id}: invalid after id '{s}'"))
                    })
                    .collect::<Result<Vec<_>>>()?;
                Ok(Task {
                    id,
                    text,
                    date,
                    done,
                    priority,
                    after,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(tasks)
    }

    pub fn save(&self, tasks: &[Task]) -> Result<()> {
        if let Some(parent) = self.path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .context("Failed to create directory for the database file")?;
        }
        let mut conn = self.open()?;
        let tx = conn.transaction().context("Failed to start a transaction")?;
        tx.execute("DELETE FROM tasks", [])
            .context("Failed to clear the tasks table")?;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO tasks (pos, id, text, date, done, priority, \"after\")
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                )
                .context("Failed to prepare the insert")?;
            for (pos, task) in tasks.iter().enumerate() {
                let after = if task.after.is_empty() {
                    None
                } else {
                    Some(
                        task.after
                            .iter()
                            .map(|id| id.to_string())
                            .collect::<Vec<_>>()
                            .join(","),
                    )
                };
                stmt.execute(rusqlite::params![
                    pos as i64 + 1,
                    task.id,
                    task.text,
                    task.date.map(|d| d.to_string()),
                    task.done,
                    task.priority,
                    after,
                ])
                .with_context(|| format!("Failed to insert task {}", task.id))?;
            }
        }
        tx.commit().context("Failed to commit the save")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    #[test]
    fn roundtrip_preserves_order_and_fields() {
        let dir = tempfile::tempdir().unwrap();
        let backend = SqliteBackend::new(dir.path().join("tasks.db"));

        assert_eq!(backend.load().unwrap(), Vec::<Task>::new());

        // Insertion order differs from id order: id 1 was reused after 2.
        let tasks = vec![
            Task {
                id: 2,
                text: "first\nmulti-line".into(),
                date: NaiveDate::from_ymd_opt(2026, 7, 15),
                done: false,
                priority: true, after: Vec::new(),
            },
            Task {
                id: 1,
                text: "second".into(),
                date: None,
                done: true,
                priority: false, after: Vec::new(),
            },
        ];
        backend.save(&tasks).unwrap();
        assert_eq!(backend.load().unwrap(), tasks);

        // Saving again replaces, not appends.
        backend.save(&tasks[..1]).unwrap();
        assert_eq!(backend.load().unwrap().len(), 1);
    }

    #[test]
    fn after_ids_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let backend = SqliteBackend::new(dir.path().join("tasks.db"));
        let tasks = vec![Task {
            id: 3,
            text: "blocked".into(),
            date: None,
            done: false,
            priority: false,
            after: vec![1, 2],
        }];
        backend.save(&tasks).unwrap();
        assert_eq!(backend.load().unwrap(), tasks);
    }

    #[test]
    fn pre_after_databases_are_migrated_on_open() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute(
                "CREATE TABLE tasks (
                    pos      INTEGER PRIMARY KEY,
                    id       INTEGER NOT NULL UNIQUE CHECK (id BETWEEN 1 AND 255),
                    text     TEXT NOT NULL,
                    date     TEXT,
                    done     INTEGER NOT NULL DEFAULT 0,
                    priority INTEGER NOT NULL DEFAULT 0
                )",
                [],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO tasks (pos, id, text) VALUES (1, 1, 'legacy')",
                [],
            )
            .unwrap();
        }
        let backend = SqliteBackend::new(path);
        let loaded = backend.load().unwrap();
        assert_eq!(loaded[0].text, "legacy");
        assert_eq!(loaded[0].after, Vec::<TaskId>::new());
        // The migrated column persists through a save.
        let mut tasks = loaded;
        tasks[0].after = vec![7];
        backend.save(&tasks).unwrap();
        assert_eq!(backend.load().unwrap(), tasks);
    }
}
