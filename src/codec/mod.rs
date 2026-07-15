//! On-disk database formats. JSON stays the default; the database file
//! extension (`RUSK_DB` / `rusk_db`) picks the format:
//!
//! - `.json` (or anything unrecognized) — pretty-printed JSON, the default;
//! - `.csv` — RFC 4180 with a fixed `id,text,date,done,priority,after` schema
//!   for spreadsheet interop (LibreOffice/Excel/Google Sheets);
//! - `.md` / `.markdown` — GitHub-style task list (feature `fmt-markdown`);
//! - `.txt` — todo.txt (feature `fmt-todotxt`);
//! - `.ndjson` / `.jsonl` — one compact JSON task per line (feature `fmt-ndjson`);
//! - `.ics` — iCalendar VTODO (feature `fmt-ics`).
//!
//! Every format encodes the whole database and decodes it back; formats
//! whose feature is disabled are still recognized by extension so the error
//! names the missing feature instead of mis-parsing the file as JSON.

pub mod csv;
#[cfg(feature = "fmt-ics")]
pub mod ics;
#[cfg(feature = "fmt-markdown")]
pub mod markdown;
#[cfg(feature = "fmt-ndjson")]
pub mod ndjson;
#[cfg(feature = "fmt-todotxt")]
pub mod todotxt;

use crate::model::{Task, TaskId};
use anyhow::{Context, Result};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DbFormat {
    Json,
    Csv,
    #[cfg(feature = "fmt-markdown")]
    Markdown,
    #[cfg(feature = "fmt-todotxt")]
    TodoTxt,
    #[cfg(feature = "fmt-ndjson")]
    Ndjson,
    #[cfg(feature = "fmt-ics")]
    Ics,
}

/// Errors uniformly when `path` selects a format whose feature is compiled
/// out. The `$feature` literal is repeated because `cfg` cannot expand from
/// a variable.
macro_rules! gated {
    ($path:expr, $feature:literal, $variant:ident) => {{
        #[cfg(feature = $feature)]
        {
            return Ok(DbFormat::$variant);
        }
        #[cfg(not(feature = $feature))]
        {
            anyhow::bail!(
                "'{}' selects a database format that this rusk build does not include; \
                 rebuild with `--features {}`",
                $path.display(),
                $feature
            );
        }
    }};
}

impl DbFormat {
    /// Detects the format from the database file name. Auxiliary files keep
    /// the base extension in the name (`tasks.csv.backup`, `tasks.csv.tmp`),
    /// so a known extension segment anywhere selects the format.
    pub fn from_path(path: &Path) -> Result<Self> {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        let has = |ext: &str| {
            name.ends_with(&format!(".{ext}")) || name.contains(&format!(".{ext}."))
        };

        if has("csv") {
            return Ok(DbFormat::Csv);
        }
        if has("md") || has("markdown") {
            gated!(path, "fmt-markdown", Markdown);
        }
        if has("txt") {
            gated!(path, "fmt-todotxt", TodoTxt);
        }
        if has("ndjson") || has("jsonl") {
            gated!(path, "fmt-ndjson", Ndjson);
        }
        if has("ics") {
            gated!(path, "fmt-ics", Ics);
        }
        Ok(DbFormat::Json)
    }

    /// Human name for error messages ("Failed to parse the … database file").
    pub fn name(self) -> &'static str {
        match self {
            DbFormat::Json => "JSON",
            DbFormat::Csv => "CSV",
            #[cfg(feature = "fmt-markdown")]
            DbFormat::Markdown => "Markdown",
            #[cfg(feature = "fmt-todotxt")]
            DbFormat::TodoTxt => "todo.txt",
            #[cfg(feature = "fmt-ndjson")]
            DbFormat::Ndjson => "NDJSON",
            #[cfg(feature = "fmt-ics")]
            DbFormat::Ics => "iCalendar",
        }
    }

    pub fn encode(self, tasks: &[Task]) -> Result<String> {
        match self {
            DbFormat::Json => {
                serde_json::to_string_pretty(tasks).context("Failed to serialize tasks")
            }
            DbFormat::Csv => Ok(csv::to_csv(tasks)),
            #[cfg(feature = "fmt-markdown")]
            DbFormat::Markdown => Ok(markdown::encode(tasks)),
            #[cfg(feature = "fmt-todotxt")]
            DbFormat::TodoTxt => Ok(todotxt::encode(tasks)),
            #[cfg(feature = "fmt-ndjson")]
            DbFormat::Ndjson => ndjson::encode(tasks),
            #[cfg(feature = "fmt-ics")]
            DbFormat::Ics => Ok(ics::encode(tasks)),
        }
    }

    /// Decodes a whole database. JSON errors are returned raw (the file
    /// backend downcasts them for line-context reporting).
    pub fn decode(self, data: &str) -> Result<Vec<Task>> {
        match self {
            DbFormat::Json => serde_json::from_str(data).map_err(Into::into),
            DbFormat::Csv => csv::from_csv(data),
            #[cfg(feature = "fmt-markdown")]
            DbFormat::Markdown => markdown::decode(data),
            #[cfg(feature = "fmt-todotxt")]
            DbFormat::TodoTxt => todotxt::decode(data),
            #[cfg(feature = "fmt-ndjson")]
            DbFormat::Ndjson => ndjson::decode(data),
            #[cfg(feature = "fmt-ics")]
            DbFormat::Ics => ics::decode(data),
        }
    }
}

/// Post-parse id fixup for interop formats where the id is optional metadata
/// (Markdown, todo.txt, iCalendar): tasks parsed with id 0 (missing) or with
/// an id already taken earlier in the file get the lowest free id, mirroring
/// `TaskManager::generate_next_id`.
#[cfg(any(feature = "fmt-markdown", feature = "fmt-todotxt", feature = "fmt-ics"))]
pub(crate) fn assign_missing_ids(tasks: &mut [Task]) -> Result<()> {
    let mut used: std::collections::HashSet<TaskId> = std::collections::HashSet::new();
    for task in tasks.iter_mut() {
        if task.id != 0 && !used.insert(task.id) {
            task.id = 0; // duplicate: first occurrence wins, this one is reassigned
        }
    }
    // Ids only ever get claimed, so scanning upward from the previous
    // assignment still yields the lowest free id for every task.
    let mut next: TaskId = 1;
    for task in tasks.iter_mut().filter(|t| t.id == 0) {
        while used.contains(&next) {
            next = next
                .checked_add(1)
                .context("Maximum number of tasks reached")?;
        }
        task.id = next;
        used.insert(next);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fmt(name: &str) -> DbFormat {
        DbFormat::from_path(&PathBuf::from(name)).unwrap()
    }

    #[test]
    fn format_detection() {
        assert_eq!(fmt("/a/tasks.json"), DbFormat::Json);
        assert_eq!(fmt("/a/tasks"), DbFormat::Json);
        assert_eq!(fmt("/a/tasks.csv"), DbFormat::Csv);
        assert_eq!(fmt("/a/Tasks.CSV"), DbFormat::Csv);
        // Auxiliary files inherit the base format.
        assert_eq!(fmt("/a/tasks.csv.backup"), DbFormat::Csv);
        assert_eq!(fmt("/a/tasks.json.backup"), DbFormat::Json);
    }

    #[cfg(feature = "fmt-markdown")]
    #[test]
    fn markdown_detection() {
        assert_eq!(fmt("/a/tasks.md"), DbFormat::Markdown);
        assert_eq!(fmt("/a/tasks.markdown"), DbFormat::Markdown);
        assert_eq!(fmt("/a/tasks.md.backup"), DbFormat::Markdown);
    }

    #[cfg(feature = "fmt-todotxt")]
    #[test]
    fn todotxt_detection() {
        assert_eq!(fmt("/a/todo.txt"), DbFormat::TodoTxt);
        assert_eq!(fmt("/a/tasks.txt.tmp"), DbFormat::TodoTxt);
    }

    #[cfg(feature = "fmt-ndjson")]
    #[test]
    fn ndjson_detection() {
        assert_eq!(fmt("/a/tasks.ndjson"), DbFormat::Ndjson);
        assert_eq!(fmt("/a/tasks.jsonl"), DbFormat::Ndjson);
    }

    #[cfg(feature = "fmt-ics")]
    #[test]
    fn ics_detection() {
        assert_eq!(fmt("/a/tasks.ics"), DbFormat::Ics);
    }

    #[cfg(any(feature = "fmt-markdown", feature = "fmt-todotxt", feature = "fmt-ics"))]
    #[test]
    fn missing_and_duplicate_ids_get_lowest_free() {
        let task = |id: TaskId| Task {
            id,
            text: String::new(),
            date: None,
            done: false,
            priority: false, after: Vec::new(),
        };
        let mut tasks = vec![task(2), task(0), task(2), task(1)];
        assign_missing_ids(&mut tasks).unwrap();
        let ids: Vec<TaskId> = tasks.iter().map(|t| t.id).collect();
        // All explicit ids (2 and 1) are reserved before any assignment, so
        // the missing id and the duplicate 2 get the lowest free ones: 3, 4.
        assert_eq!(ids, vec![2, 3, 4, 1]);
    }
}
