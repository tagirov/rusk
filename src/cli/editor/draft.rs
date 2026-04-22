//! Crash-safe draft autosave for the interactive editor.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const AUTOSAVE_INTERVAL_MS: u64 = 3000;

/// Optional inputs the caller can plumb into the editor:
/// the colored first-line prompt, the autosave destination and its task key.
#[derive(Clone, Default)]
pub struct EditorExtras {
    /// Colored rendering of the first-line prompt (plain-width must match `prompt`).
    pub first_line_colored: Option<String>,
    /// Where to write autosave drafts; `None` disables autosave.
    pub draft_path: Option<PathBuf>,
    /// Identifier written alongside the draft content.
    pub draft_key: Option<String>,
    /// Task due date before edit: first-line tokens starting with `+` resolve relative to this.
    pub relative_date_base: Option<chrono::NaiveDate>,
}

/// Periodic autosave tick. Call once per event-loop iteration; the function
/// rate-limits itself internally so it's safe to call on every tick.
pub(super) fn tick(
    extras: &EditorExtras,
    lines: &[String],
    prefill: &str,
    last_saved: &mut String,
    last_autosave: &mut Instant,
) {
    let Some(draft_path) = &extras.draft_path else {
        return;
    };
    if last_autosave.elapsed() < Duration::from_millis(AUTOSAVE_INTERVAL_MS) {
        return;
    }
    let joined = lines.join("\n");
    if joined != prefill && &joined != last_saved {
        write(
            draft_path,
            extras.draft_key.as_deref().unwrap_or(""),
            &joined,
        )
        .ok();
        *last_saved = joined;
    }
    *last_autosave = Instant::now();
}

/// Remove the draft file if configured. Silently ignores errors.
pub(super) fn cleanup(extras: &EditorExtras) {
    if let Some(p) = &extras.draft_path {
        let _ = std::fs::remove_file(p);
    }
}

pub fn path_for(dir: &Path) -> PathBuf {
    dir.join("editor.draft")
}

pub fn write(path: &Path, key: &str, text: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    let payload = serde_json::json!({
        "key": key,
        "text": text,
        "timestamp": chrono::Local::now().to_rfc3339(),
    });
    std::fs::write(path, serde_json::to_vec_pretty(&payload)?)
        .context("Failed to write editor draft")?;
    Ok(())
}

pub fn read_for(path: &Path, key: &str) -> Option<String> {
    let raw = std::fs::read_to_string(path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&raw).ok()?;
    let saved_key = value.get("key")?.as_str()?.to_string();
    if saved_key != key {
        return None;
    }
    Some(value.get("text")?.as_str()?.to_string())
}
