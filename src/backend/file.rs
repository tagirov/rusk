//! The default backend: a local file, whole database per save, written
//! atomically (temp sibling + rename, with copy and direct-write fallbacks
//! for filesystems where rename fails). The file extension picks the format
//! (see [`crate::codec`]). With the `backend-git` feature and
//! `git_backend = true` every save is also committed to a git repository in
//! the database directory.

use super::{aux_path, warn_yellow};
use crate::codec::DbFormat;
use crate::model::Task;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct FileBackend {
    path: PathBuf,
    format: DbFormat,
    #[cfg(feature = "backend-git")]
    git: bool,
}

impl FileBackend {
    pub fn new(path: PathBuf) -> Result<Self> {
        let format = DbFormat::from_path(&path)?;
        Ok(Self {
            path,
            format,
            #[cfg(feature = "backend-git")]
            git: crate::config::config().git_backend,
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<Vec<Task>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let data = fs::read_to_string(&self.path).context("Failed to read the database file")?;

        self.format.decode(&data).map_err(|e| {
            // JSON gets the detailed corruption report with line context;
            // it is the default format and the one hand-edited most.
            if let Some(json_err) = e.downcast_ref::<serde_json::Error>() {
                let context_line = json_error_line_context(&data, json_err)
                    .map(|c| format!(" Context: {c}"))
                    .unwrap_or_default();
                anyhow::anyhow!(
                    "Failed to parse the database file at '{}'. The file appears to be corrupted.\n\
                    JSON parsing error: {}{}\n\
                    \n\
                    To fix this issue, you can:\n\
                    1. Delete the corrupted file: rm '{}'\n\
                    2. Or restore from backup if you have one\n\
                    3. The application will create a new empty database on next run",
                    self.path.display(),
                    json_err,
                    context_line,
                    self.path.display()
                )
            } else {
                anyhow::anyhow!(
                    "Failed to parse the {} database file at '{}': {e}",
                    self.format.name(),
                    self.path.display()
                )
            }
        })
    }

    pub fn save(&self, tasks: &[Task]) -> Result<()> {
        let ensure_dir = || {
            if let Some(parent) = self.path.parent() {
                fs::create_dir_all(parent)
            } else {
                Ok(())
            }
        };

        ensure_dir().context("Failed to create directory for the database file")?;

        let data = self.format.encode(tasks)?;
        let temp_path = aux_path(&self.path, "tmp");

        fs::write(&temp_path, &data).context("Failed to write temporary database file")?;

        match fs::rename(&temp_path, &self.path) {
            Ok(_) => {}
            Err(e) => {
                ensure_dir().ok();

                match fs::copy(&temp_path, &self.path) {
                    Ok(_) => {
                        let _ = fs::remove_file(&temp_path);
                        if !crate::is_test_mode() {
                            warn_yellow(&format!(
                                "Warning: Atomic rename failed ({e}), used copy+remove instead"
                            ));
                        }
                    }
                    Err(copy_err) => {
                        ensure_dir().ok();
                        let _ = fs::remove_file(&temp_path);
                        fs::write(&self.path, data).context("Failed to write database file")?;
                        if !crate::is_test_mode() {
                            warn_yellow(&format!(
                                "Warning: Atomic write failed ({e}), copy fallback also failed ({copy_err}), used direct write instead"
                            ));
                        }
                    }
                }
            }
        }

        #[cfg(feature = "backend-git")]
        if self.git {
            super::git::commit_db(&self.path, tasks.len());
        }

        Ok(())
    }
}

/// Byte offset (0-based) of the first character of each 1-based line in `s`.
fn line_starts(s: &str) -> Vec<usize> {
    let mut v = vec![0];
    for (i, c) in s.char_indices() {
        if c == '\n' {
            v.push(i + 1);
        }
    }
    v
}

fn line_byte_end(data: &str, starts: &[usize], one_based_line: usize) -> Option<usize> {
    if one_based_line < 1 || one_based_line > starts.len() {
        return None;
    }
    let s = if one_based_line < starts.len() {
        starts[one_based_line]
    } else {
        data.len()
    };
    Some(s)
}

fn error_byte_in_file(data: &str, line: usize, column: usize) -> Option<usize> {
    if line < 1 {
        return None;
    }
    let starts = line_starts(data);
    if line > starts.len() {
        return None;
    }
    let line0 = line - 1;
    let line_start = starts[line0];
    let after_line = if line0 + 1 < starts.len() {
        starts[line0 + 1]
    } else {
        data.len()
    };
    let line_len = after_line - line_start;
    let col0 = column.saturating_sub(1);
    if col0 > line_len {
        return None;
    }
    Some(line_start + col0)
}

fn json_error_line_context(data: &str, e: &serde_json::Error) -> Option<String> {
    let n = e.line();
    if n == 0 {
        return None;
    }
    let lines: Vec<_> = data.lines().collect();
    let i = n - 1;
    if i >= lines.len() {
        return None;
    }
    let starts = line_starts(data);
    let first = i.saturating_sub(1) + 1;
    let last = (i + 2).min(lines.len());
    let range_str = if first <= last {
        match (
            starts.get(first - 1).copied(),
            line_byte_end(data, &starts, last),
        ) {
            (Some(a), Some(b)) if a <= b => format!("context file bytes {a}..{b}"),
            _ => String::new(),
        }
    } else {
        String::new()
    };

    let err_str = error_byte_in_file(data, n, e.column())
        .map(|b| format!("(error at byte {b})"))
        .unwrap_or_default();

    let ctx = (i.saturating_sub(1)..(i + 2).min(lines.len()))
        .map(|j| format!("{}: {}", j + 1, lines[j].trim_end()))
        .collect::<Vec<_>>()
        .join(" | ");

    let parts = [err_str.as_str(), range_str.as_str()]
        .into_iter()
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if parts.is_empty() {
        Some(ctx)
    } else {
        Some(format!("{parts}: {ctx}"))
    }
}
