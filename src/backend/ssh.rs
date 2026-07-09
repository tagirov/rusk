//! SSH backend (`rusk_db = user@host:/path/tasks.json`), feature
//! `backend-ssh`: the database is a file on a remote machine, read and
//! written over the system `ssh` (keys, agent and `~/.ssh/config` apply).
//! The remote extension picks the format exactly like a local path, so a
//! remote `tasks.md` stays a readable Markdown task list. Writes stream to
//! a temp sibling and `mv` into place (atomic replace).

use crate::codec::DbFormat;
use crate::model::Task;
use crate::transport;
use anyhow::{Context, Result};
use std::path::Path;

#[derive(Debug)]
pub struct SshBackend {
    target: String,
    path: String,
    format: DbFormat,
}

impl SshBackend {
    pub fn new(target: &str, path: &str) -> Result<Self> {
        let format = DbFormat::from_path(Path::new(path))?;
        Ok(Self {
            target: target.to_string(),
            path: path.to_string(),
            format,
        })
    }

    pub fn describe(&self) -> String {
        format!("{}:{}", self.target, self.path)
    }

    pub fn load(&self) -> Result<Vec<Task>> {
        let raw = transport::ssh_read_file(&self.target, &self.path)?;
        let text = String::from_utf8_lossy(&raw);
        if text.trim().is_empty() {
            return Ok(Vec::new());
        }
        self.format.decode(&text).with_context(|| {
            format!(
                "remote file {} is not a valid {} task list",
                self.path,
                self.format.name()
            )
        })
    }

    pub fn save(&self, tasks: &[Task]) -> Result<()> {
        let data = self.format.encode(tasks)?;
        transport::ssh_write_file(&self.target, &self.path, data.as_bytes())
    }
}
