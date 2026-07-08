//! `rusk sync`: whole-database synchronization with a remote.
//!
//! Transports (chosen by the shape of `sync_remote`):
//! - `user@host:/path/tasks.json` — the system `ssh` binary; pull is a remote
//!   `cat`, push streams the file to a temp path and `mv`s it into place
//!   (atomic replace). The user's keys, agent and ~/.ssh/config just work.
//! - `http(s)://host[:port]` — the API of a running `rusk serve`, via the
//!   system `curl` (GET /api/tasks, PUT /api/tasks) with an optional Bearer
//!   token (`sync_token`).
//!
//! Conflict safety without merge machinery: a state file next to the
//! database stores the hash of the last synced content. Comparing
//! local/remote/base classifies the situation as in-sync, fast-forward
//! (one side changed), or diverged (both changed — push/pull need --force).
//! Hashes are taken over the canonical serialization (parsed tasks
//! re-encoded as compact JSON), so the pretty-printed local file and the
//! compact HTTP body compare equal, as do CSV-backed databases.

use crate::config::theme;
use crate::{Task, TaskManager};
use anyhow::{Context, Result, bail};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Debug, Clone, Copy)]
pub enum Direction {
    Auto,
    Push { force: bool },
    Pull { force: bool },
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Remote {
    Ssh { target: String, path: String },
    Http { base: String },
}

impl Remote {
    fn parse(s: &str) -> Result<Self> {
        if s.starts_with("http://") || s.starts_with("https://") {
            return Ok(Remote::Http {
                base: s.trim_end_matches('/').to_string(),
            });
        }
        if let Some((target, path)) = s.split_once(':')
            && !target.is_empty()
            && !path.is_empty()
        {
            return Ok(Remote::Ssh {
                target: target.to_string(),
                path: path.to_string(),
            });
        }
        bail!(
            "invalid sync remote '{s}': expected `user@host:/path/tasks.json` (ssh) \
             or `https://host` (rusk serve API)"
        );
    }
}

#[derive(Debug, PartialEq, Eq)]
enum SyncStatus {
    InSync,
    LocalAhead,
    RemoteAhead,
    Diverged,
}

/// Classifies against the last synced hash. `base` is `None` on first sync.
fn decide(base: Option<&str>, local: &str, remote: &str, remote_empty: bool) -> SyncStatus {
    if local == remote {
        return SyncStatus::InSync;
    }
    match base {
        Some(b) if b == remote => SyncStatus::LocalAhead,
        Some(b) if b == local => SyncStatus::RemoteAhead,
        Some(_) => SyncStatus::Diverged,
        // No sync history: an empty remote is safe to seed; anything else
        // is unknown territory and needs an explicit direction.
        None if remote_empty => SyncStatus::LocalAhead,
        None => SyncStatus::Diverged,
    }
}

fn fnv1a(bytes: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

/// Stable across formats: tasks re-encoded as compact JSON, then FNV-1a 64.
fn canonical_hash(tasks: &[Task]) -> Result<String> {
    let json = serde_json::to_string(tasks).context("Failed to serialize tasks")?;
    Ok(format!("{:016x}", fnv1a(json.as_bytes())))
}

fn state_path(db_path: &Path) -> PathBuf {
    let ext = db_path.extension().and_then(|e| e.to_str()).unwrap_or("json");
    db_path.with_extension(format!("{ext}.sync"))
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SyncState {
    remote: String,
    hash: String,
}

/// Last-synced hash, only when it was recorded for the same remote.
fn read_base(state_path: &Path, remote: &str) -> Option<String> {
    let data = std::fs::read_to_string(state_path).ok()?;
    let state: SyncState = serde_json::from_str(&data).ok()?;
    (state.remote == remote).then_some(state.hash)
}

fn write_base(state_path: &Path, remote: &str, hash: &str) {
    let state = SyncState {
        remote: remote.to_string(),
        hash: hash.to_string(),
    };
    if let Ok(json) = serde_json::to_string(&state) {
        let _ = std::fs::write(state_path, json);
    }
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

fn run_tool(mut cmd: Command, stdin_data: Option<&[u8]>, tool: &str) -> Result<Vec<u8>> {
    cmd.stdin(if stdin_data.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            anyhow::anyhow!("`{tool}` not found in PATH (required for this sync remote)")
        } else {
            anyhow::anyhow!("failed to run {tool}: {e}")
        }
    })?;
    if let Some(data) = stdin_data {
        child
            .stdin
            .take()
            .expect("stdin piped")
            .write_all(data)
            .with_context(|| format!("failed to stream data to {tool}"))?;
    }
    let output = child
        .wait_with_output()
        .with_context(|| format!("failed to wait for {tool}"))?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("{tool} failed ({}): {}", output.status, stderr.trim());
    }
    Ok(output.stdout)
}

impl Remote {
    fn describe(&self) -> String {
        match self {
            Remote::Ssh { target, path } => format!("{target}:{path}"),
            Remote::Http { base } => base.clone(),
        }
    }

    fn fetch(&self, token: Option<&str>) -> Result<Vec<Task>> {
        match self {
            Remote::Ssh { target, path } => {
                let quoted = shell_quote(path);
                let mut cmd = Command::new("ssh");
                cmd.arg(target)
                    .arg(format!("test -f {quoted} && cat {quoted} || true"));
                let raw = run_tool(cmd, None, "ssh")?;
                let text = String::from_utf8_lossy(&raw);
                if text.trim().is_empty() {
                    return Ok(Vec::new());
                }
                match crate::codec::DbFormat::from_path(Path::new(path)) {
                    crate::codec::DbFormat::Csv => crate::codec::from_csv(&text)
                        .with_context(|| format!("remote file {path} is not valid CSV")),
                    crate::codec::DbFormat::Json => serde_json::from_str(&text)
                        .with_context(|| format!("remote file {path} is not a valid task list")),
                }
            }
            Remote::Http { base } => {
                let mut cmd = Command::new("curl");
                cmd.args(["-fsS", "--max-time", "30"]);
                if let Some(token) = token {
                    cmd.args(["-H", &format!("Authorization: Bearer {token}")]);
                }
                cmd.arg(format!("{base}/api/tasks"));
                let raw = run_tool(cmd, None, "curl")?;
                serde_json::from_slice(&raw).context("remote API returned an invalid task list")
            }
        }
    }

    fn push(&self, tasks: &[Task], token: Option<&str>) -> Result<()> {
        match self {
            Remote::Ssh { target, path } => {
                let data = match crate::codec::DbFormat::from_path(Path::new(path)) {
                    crate::codec::DbFormat::Csv => crate::codec::to_csv(tasks),
                    crate::codec::DbFormat::Json => serde_json::to_string_pretty(tasks)
                        .context("Failed to serialize tasks")?,
                };
                let quoted = shell_quote(path);
                let quoted_tmp = shell_quote(&format!("{path}.tmp"));
                let mkdir = match path.rsplit_once('/') {
                    Some((dir, _)) if !dir.is_empty() => {
                        format!("mkdir -p {} && ", shell_quote(dir))
                    }
                    _ => String::new(),
                };
                let mut cmd = Command::new("ssh");
                cmd.arg(target).arg(format!(
                    "{mkdir}cat > {quoted_tmp} && mv {quoted_tmp} {quoted}"
                ));
                run_tool(cmd, Some(data.as_bytes()), "ssh")?;
                Ok(())
            }
            Remote::Http { base } => {
                let json = serde_json::to_string(tasks).context("Failed to serialize tasks")?;
                let mut cmd = Command::new("curl");
                cmd.args([
                    "-fsS",
                    "--max-time",
                    "30",
                    "-X",
                    "PUT",
                    "-H",
                    "Content-Type: application/json",
                    "--data-binary",
                    "@-",
                ]);
                if let Some(token) = token {
                    cmd.args(["-H", &format!("Authorization: Bearer {token}")]);
                }
                cmd.arg(format!("{base}/api/tasks"));
                run_tool(cmd, Some(json.as_bytes()), "curl")?;
                Ok(())
            }
        }
    }
}

fn env_or_config(env: &str, config_value: &Option<String>) -> Option<String> {
    std::env::var(env)
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| config_value.clone())
}

pub fn run(direction: Direction) -> Result<()> {
    let config = crate::config::config();
    let remote_str = env_or_config("RUSK_SYNC_REMOTE", &config.sync_remote).context(
        "no sync remote configured: set `sync_remote` in the config file \
         or the RUSK_SYNC_REMOTE environment variable",
    )?;
    let token = env_or_config("RUSK_SYNC_TOKEN", &config.sync_token);
    let remote = Remote::parse(&remote_str)?;

    let db_path = TaskManager::resolve_db_path();
    let local_tasks = TaskManager::load_tasks_from_path(&db_path)?;
    let remote_tasks = remote.fetch(token.as_deref())?;

    let local_hash = canonical_hash(&local_tasks)?;
    let remote_hash = canonical_hash(&remote_tasks)?;
    let state_file = state_path(&db_path);
    let base = read_base(&state_file, &remote_str);

    let status = decide(
        base.as_deref(),
        &local_hash,
        &remote_hash,
        remote_tasks.is_empty(),
    );

    let push = |tasks: &[Task]| -> Result<()> {
        remote.push(tasks, token.as_deref())?;
        write_base(&state_file, &remote_str, &canonical_hash(tasks)?);
        println!(
            "{} {} task(s) to {}",
            theme().success.paint("Pushed"),
            tasks.len(),
            remote.describe()
        );
        Ok(())
    };
    let pull = |tasks: Vec<Task>| -> Result<()> {
        let count = tasks.len();
        let tm = TaskManager {
            tasks,
            db_path: db_path.clone(),
        };
        tm.save()?;
        write_base(&state_file, &remote_str, &canonical_hash(&tm.tasks)?);
        println!(
            "{} {} task(s) from {}",
            theme().success.paint("Pulled"),
            count,
            remote.describe()
        );
        Ok(())
    };
    let already_in_sync = || {
        println!(
            "{} with {}",
            theme().success.paint("Already in sync"),
            remote.describe()
        );
    };

    match (direction, status) {
        (_, SyncStatus::InSync) => {
            // Record the base so a later divergence is detected even if the
            // first contact happened with identical content.
            write_base(&state_file, &remote_str, &local_hash);
            already_in_sync();
            Ok(())
        }
        (Direction::Auto, SyncStatus::LocalAhead) => push(&local_tasks),
        (Direction::Auto, SyncStatus::RemoteAhead) => pull(remote_tasks),
        (Direction::Auto, SyncStatus::Diverged) => bail!(
            "local and remote have both changed since the last sync; \
             resolve with `rusk sync push --force` (keep local) \
             or `rusk sync pull --force` (keep remote)"
        ),
        (Direction::Push { force: true }, _) => push(&local_tasks),
        (Direction::Push { force: false }, SyncStatus::LocalAhead) => push(&local_tasks),
        (Direction::Push { force: false }, _) => bail!(
            "the remote has changes not present locally; \
             use `rusk sync pull` first or `rusk sync push --force` to overwrite them"
        ),
        (Direction::Pull { force: true }, _) => pull(remote_tasks),
        (Direction::Pull { force: false }, SyncStatus::RemoteAhead) => pull(remote_tasks),
        (Direction::Pull { force: false }, _) => bail!(
            "the local database has changes not present on the remote; \
             use `rusk sync push` first or `rusk sync pull --force` to discard them"
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remote_parsing() {
        assert_eq!(
            Remote::parse("user@vps:/srv/tasks/tasks.json").unwrap(),
            Remote::Ssh {
                target: "user@vps".into(),
                path: "/srv/tasks/tasks.json".into()
            }
        );
        assert_eq!(
            Remote::parse("https://tasks.example.com/").unwrap(),
            Remote::Http {
                base: "https://tasks.example.com".into()
            }
        );
        assert!(Remote::parse("just-a-host").is_err());
        assert!(Remote::parse(":/path").is_err());
    }

    #[test]
    fn decide_matrix() {
        // Identical content is always in sync.
        assert_eq!(decide(None, "a", "a", false), SyncStatus::InSync);
        assert_eq!(decide(Some("x"), "a", "a", false), SyncStatus::InSync);
        // Fast-forwards.
        assert_eq!(decide(Some("r"), "l", "r", false), SyncStatus::LocalAhead);
        assert_eq!(decide(Some("l"), "l", "r", false), SyncStatus::RemoteAhead);
        // Both sides moved.
        assert_eq!(decide(Some("b"), "l", "r", false), SyncStatus::Diverged);
        // First sync: empty remote is seedable, non-empty needs a decision.
        assert_eq!(decide(None, "l", "r", true), SyncStatus::LocalAhead);
        assert_eq!(decide(None, "l", "r", false), SyncStatus::Diverged);
    }

    #[test]
    fn canonical_hash_is_format_independent() {
        let tasks = vec![Task {
            id: 1,
            text: "hello".into(),
            date: chrono::NaiveDate::from_ymd_opt(2026, 7, 8),
            done: false,
            priority: true,
        }];
        // Pretty JSON on disk and compact JSON over HTTP hash identically
        // because hashing re-encodes parsed tasks canonically.
        let pretty: Vec<Task> =
            serde_json::from_str(&serde_json::to_string_pretty(&tasks).unwrap()).unwrap();
        assert_eq!(
            canonical_hash(&tasks).unwrap(),
            canonical_hash(&pretty).unwrap()
        );
        assert_ne!(
            canonical_hash(&tasks).unwrap(),
            canonical_hash(&[]).unwrap()
        );
    }

    #[test]
    fn state_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("tasks.json");
        let sp = state_path(&db);
        assert!(sp.to_string_lossy().ends_with("tasks.json.sync"));
        assert_eq!(read_base(&sp, "r"), None);
        write_base(&sp, "user@h:/p", "abc");
        assert_eq!(read_base(&sp, "user@h:/p"), Some("abc".to_string()));
        // A different remote invalidates the recorded base.
        assert_eq!(read_base(&sp, "user@other:/p"), None);
    }

    #[test]
    fn shell_quoting() {
        assert_eq!(shell_quote("/plain/path"), "'/plain/path'");
        assert_eq!(shell_quote("with'quote"), r"'with'\''quote'");
    }
}
