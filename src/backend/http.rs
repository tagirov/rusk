//! HTTP backend (`rusk_db = https://host`), feature `backend-http`: the
//! database lives behind the API of a running `rusk serve` and there is no
//! local copy at all — every command loads via `GET /api/tasks` and saves
//! via `PUT /api/tasks`. Because a single serve instance serializes all
//! writes, concurrent writers (another machine, the web UI, cron scripts)
//! never diverge — the trade-off is that a network and the server must be
//! reachable. Auth via `db_token` / `RUSK_DB_TOKEN` (Bearer).

use crate::model::Task;
use crate::transport;
use anyhow::{Context, Result};

#[derive(Debug)]
pub struct HttpBackend {
    base: String,
    token: Option<String>,
}

impl HttpBackend {
    pub fn new(base: &str, token: Option<String>) -> Self {
        Self {
            base: base.trim_end_matches('/').to_string(),
            token,
        }
    }

    pub fn describe(&self) -> String {
        self.base.clone()
    }

    pub fn load(&self) -> Result<Vec<Task>> {
        let url = format!("{}/api/tasks", self.base);
        let raw = transport::http_request(&url, None, self.token.as_deref(), None)
            .with_context(|| format!("failed to load tasks from {}", self.base))?;
        serde_json::from_slice(&raw).context("remote API returned an invalid task list")
    }

    pub fn save(&self, tasks: &[Task]) -> Result<()> {
        let json = serde_json::to_string(tasks).context("Failed to serialize tasks")?;
        let url = format!("{}/api/tasks", self.base);
        transport::http_request(&url, Some("PUT"), self.token.as_deref(), Some(json.as_bytes()))
            .with_context(|| format!("failed to save tasks to {}", self.base))?;
        Ok(())
    }
}
