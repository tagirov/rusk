//! Cheap identities of database content. A backend remembers what it
//! loaded and compares that with what is there right before it writes, so a
//! save based on a stale read is noticed instead of silently dropping what
//! another process wrote in between (see `Backend::save`). The identity of
//! a task list doubles as the `ETag` of the web API and as the sync hash.

use crate::model::Task;
use anyhow::{Context, Result};

/// FNV-1a 64. Not cryptographic, and it does not have to be: whoever can
/// write the database needs no collision to change it.
fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Identity of raw file content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Fingerprint {
    len: u64,
    hash: u64,
}

impl Fingerprint {
    pub(crate) fn of(bytes: &[u8]) -> Self {
        Self {
            len: bytes.len() as u64,
            hash: fnv1a(bytes),
        }
    }
}

/// What a backend found at the database location when it last read or
/// wrote it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Seen {
    /// Nothing was read through this backend yet: the caller owns the task
    /// list and replaces the database unconditionally (tests, tools that
    /// write a fresh list).
    Unchecked,
    /// There was no database file.
    Missing,
    Content(Fingerprint),
}

/// Identity of a task list, stable across formats and transports: the tasks
/// re-encoded as compact JSON, then FNV-1a 64. The pretty-printed local
/// file, the compact HTTP body and a CSV database holding the same tasks
/// all share one revision.
pub fn list_revision(tasks: &[Task]) -> Result<String> {
    let json = serde_json::to_string(tasks).context("Failed to serialize tasks")?;
    Ok(format!("{:016x}", fnv1a(json.as_bytes())))
}

/// Identity of a piece of text. The editor pins a draft to the task text
/// it was typed against with this, so a draft cannot be offered back for a
/// task that has changed under it.
pub fn text_revision(text: &str) -> String {
    format!("{:016x}", fnv1a(text.as_bytes()))
}

/// Identity of one task, the same way: its compact JSON, then FNV-1a 64.
/// The web page computes it from the task objects it was served (see
/// `taskTag` in `web/template.html`) to say which state of a task a change
/// was meant for; the two must stay in step.
pub fn task_revision(task: &Task) -> Result<String> {
    let json = serde_json::to_string(task).context("Failed to serialize the task")?;
    Ok(format!("{:016x}", fnv1a(json.as_bytes())))
}

/// The revision an HTTP entity tag names: `W/"abc"` and `"abc"` are both
/// `abc` (a proxy that compresses the response weakens the tag).
pub fn clean_etag(value: &str) -> String {
    let value = value.trim();
    let value = value.strip_prefix("W/").unwrap_or(value);
    value.trim_matches('"').to_string()
}

/// Evaluates an `If-Match` header against the revision the server holds.
pub fn if_match_allows(header: &str, current: &str) -> bool {
    header.split(',').any(|tag| {
        let tag = tag.trim();
        tag == "*" || clean_etag(tag) == current
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn if_match_header_forms() {
        assert!(if_match_allows("\"abc\"", "abc"));
        assert!(if_match_allows("W/\"abc\"", "abc"));
        assert!(if_match_allows("\"old\", \"abc\"", "abc"));
        assert!(if_match_allows("*", "abc"));
        assert!(!if_match_allows("\"old\"", "abc"));
        assert!(!if_match_allows("", "abc"));
    }

    /// Pinned: the web page hashes `JSON.stringify(task)` and must arrive at
    /// the same tag (checked against a live server in `tests/web_tests.rs`).
    #[test]
    fn task_revision_is_the_hash_of_the_compact_json() {
        let task = Task {
            id: 3,
            text: "quote \" backslash \\ tab \t newline \n emoji \u{1f600} <b>".into(),
            date: chrono::NaiveDate::from_ymd_opt(2026, 7, 8),
            done: false,
            priority: true,
            after: vec![1, 2],
        };
        let json = serde_json::to_string(&task).unwrap();
        assert_eq!(
            json,
            r#"{"id":3,"text":"quote \" backslash \\ tab \t newline \n emoji 😀 <b>","date":"2026-07-08","done":false,"priority":true,"after":[1,2]}"#
        );
        assert_eq!(task_revision(&task).unwrap(), format!("{:016x}", fnv1a(json.as_bytes())));
        let mut other = task.clone();
        other.done = true;
        assert_ne!(task_revision(&task).unwrap(), task_revision(&other).unwrap());
    }

    #[test]
    fn fingerprints_tell_content_apart() {
        assert_eq!(Fingerprint::of(b"[]"), Fingerprint::of(b"[]"));
        assert_ne!(Fingerprint::of(b"[]"), Fingerprint::of(b"[ ]"));
        assert_ne!(Fingerprint::of(b""), Fingerprint::of(b"\0"));
    }

    #[test]
    fn list_revision_is_format_independent() {
        let tasks = vec![Task {
            id: 1,
            text: "hello".into(),
            date: chrono::NaiveDate::from_ymd_opt(2026, 7, 8),
            done: false,
            priority: true,
            after: Vec::new(),
        }];
        // Pretty JSON on disk and compact JSON over HTTP share a revision
        // because it is taken over the re-encoded tasks.
        let pretty: Vec<Task> =
            serde_json::from_str(&serde_json::to_string_pretty(&tasks).unwrap()).unwrap();
        assert_eq!(list_revision(&tasks).unwrap(), list_revision(&pretty).unwrap());
        assert_ne!(list_revision(&tasks).unwrap(), list_revision(&[]).unwrap());
        // The sync state files written by earlier versions keep matching.
        assert_eq!(list_revision(&[]).unwrap(), "09612b07b5ecb5a5");
    }
}
