//! HTTP backend (`rusk_db = https://host`), feature `backend-http`: the
//! database lives behind the API of a running `rusk serve` and there is no
//! local copy at all — every command loads via `GET /api/tasks` and saves
//! via `PUT /api/tasks`. The trade-off is that a network and the server
//! must be reachable. Auth via `db_token` / `RUSK_DB_TOKEN` (Bearer), or
//! the `user:password@` of the URL (basic authentication, for a reverse
//! proxy): curl gets them in a file, never on its command line, and
//! messages and the sync state name the URL with the user alone.
//!
//! The list comes with an `ETag` (its revision) and goes back with
//! `If-Match`, so the server refuses (412) to replace a list that another
//! writer — another machine, the web UI, a cron script — changed in the
//! meantime. A plain `save` reports that as a stale database; `update`
//! fetches the list again and applies its change to that. A server from
//! before the `ETag` sends none, and saves to it stay unconditional.

use super::{ChangeFn, StaleDatabase, Updated};
use crate::model::Task;
use crate::transport::{self, Auth, HttpResponse};
use anyhow::{Context, Result, bail};
use std::sync::Mutex;

/// How often `update` starts over when other writers keep winning. Each
/// round has a winner, so this many writers at once all get through.
const UPDATE_ATTEMPTS: u32 = 12;

/// A pause before starting over, so that writers that collided do not
/// collide again: random, and longer with every attempt (up to ~1 s).
fn back_off(attempt: u32) {
    use std::hash::{BuildHasher, Hasher};
    // RandomState is seeded from the OS once per process.
    let mut hasher = std::collections::hash_map::RandomState::new().build_hasher();
    hasher.write_u32(attempt);
    let ceiling_ms = (20u64 << attempt.min(6)).min(1_000);
    std::thread::sleep(std::time::Duration::from_millis(hasher.finish() % ceiling_ms));
}

#[derive(Debug)]
pub struct HttpBackend {
    /// The server's base URL without the user and password: what curl is
    /// given.
    base: String,
    /// The base URL as messages name it: with the user, never the
    /// password.
    shown: String,
    auth: Auth,
    /// Revision of the list the last load, or our own last save, left on
    /// the server.
    etag: Mutex<Option<String>>,
}

impl HttpBackend {
    /// `token` for the server, and the URL's credentials for a proxy in
    /// front of it (see `transport::Auth`).
    pub fn new(base: &str, token: Option<String>) -> Self {
        let url = split_userinfo(base.trim_end_matches('/'));
        Self {
            base: url.base,
            shown: url.shown,
            auth: Auth {
                token,
                basic: url.credentials,
            },
            etag: Mutex::new(None),
        }
    }

    pub fn describe(&self) -> String {
        self.shown.clone()
    }

    fn etag(&self) -> Option<String> {
        self.etag.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    fn set_etag(&self, etag: Option<String>) {
        *self.etag.lock().unwrap_or_else(|e| e.into_inner()) = etag;
    }

    fn request(
        &self,
        method: Option<&str>,
        body: Option<&[u8]>,
        if_match: Option<&str>,
    ) -> Result<HttpResponse> {
        let url = format!("{}/api/tasks", self.base);
        transport::http_request(&url, method, &self.auth, body, if_match)
    }

    /// The server answered, but not with a success (that includes a
    /// redirect: curl does not follow it, so nothing was loaded or saved).
    fn refused(&self, what: &str, res: &HttpResponse) -> anyhow::Error {
        anyhow::anyhow!(
            "failed to {what} {}: HTTP {} ({})",
            self.shown,
            res.status,
            res.error_text()
        )
    }

    /// The list on the server with its revision.
    fn get(&self) -> Result<(Vec<Task>, Option<String>)> {
        let res = self
            .request(None, None, None)
            .with_context(|| format!("failed to load tasks from {}", self.shown))?;
        if !res.is_success() {
            return Err(self.refused("load tasks from", &res));
        }
        let tasks =
            serde_json::from_slice(&res.body).context("remote API returned an invalid task list")?;
        // A rusk server sends a list that follows the rules already.
        Ok((super::normalized(tasks, &self.shown)?, res.etag))
    }

    /// Replaces the list on the server if it still has the revision
    /// `if_match`; returns the revision of the new list and the list as the
    /// server holds it now, when it says (a server before R21 does not).
    fn put(&self, tasks: &[Task], if_match: Option<&str>) -> Result<(Option<String>, Option<Vec<Task>>)> {
        let json = serde_json::to_string(tasks).context("Failed to serialize tasks")?;
        let res = self
            .request(Some("PUT"), Some(json.as_bytes()), if_match)
            .with_context(|| format!("failed to save tasks to {}", self.shown))?;
        match res.status {
            _ if res.is_success() => {
                let held = serde_json::from_slice::<serde_json::Value>(&res.body)
                    .ok()
                    .and_then(|reply| reply.get("tasks").cloned())
                    .and_then(|tasks| serde_json::from_value::<Vec<Task>>(tasks).ok());
                Ok((res.etag, held))
            }
            412 => Err(StaleDatabase::at(&self.shown)),
            _ => Err(self.refused("save tasks to", &res)),
        }
    }

    pub fn load(&self) -> Result<Vec<Task>> {
        let (tasks, etag) = self.get()?;
        self.set_etag(etag);
        Ok(tasks)
    }

    /// Replaces the list on the server, unless it has changed since this
    /// backend loaded it: that is a [`StaleDatabase`] error.
    pub fn save(&self, tasks: &[Task]) -> Result<()> {
        self.save_held(tasks).map(|_| ())
    }

    /// [`save`](Self::save), and what the server holds after it: the list
    /// as its database stored it (its format may hold less), worked out by
    /// the server as it stored it; `None` from a server that does not say.
    pub fn save_held(&self, tasks: &[Task]) -> Result<Option<Vec<Task>>> {
        let (etag, held) = self.put(tasks, self.etag().as_deref())?;
        self.set_etag(etag);
        Ok(held)
    }

    /// Applies `change` to `snapshot` and saves the result; when the server
    /// says the list has changed since, fetches it again and starts over on
    /// that — unless the snapshot carries edits of its own
    /// (`snapshot_is_clean` is false), which a fresh list would drop.
    pub(super) fn update<T>(
        &self,
        snapshot: &[Task],
        snapshot_is_clean: bool,
        change: ChangeFn<'_, T>,
    ) -> Result<Updated<T>> {
        let mut base = snapshot.to_vec();
        let mut reread = false;
        // The remembered revision moves only together with the list the
        // caller gets back: after a failure both stay as they were.
        let mut etag = self.etag();
        for attempt in 1.. {
            let (tasks, value, changed) = super::apply(&base, change)?;
            let stored = changed || reread || snapshot_is_clean;
            if !changed {
                self.set_etag(etag);
                return Ok(Updated {
                    tasks,
                    value,
                    stored,
                });
            }
            match self.put(&tasks, etag.as_deref()) {
                Ok((new_etag, _)) => {
                    self.set_etag(new_etag);
                    return Ok(Updated {
                        tasks,
                        value,
                        stored,
                    });
                }
                Err(e) if e.is::<StaleDatabase>() && snapshot_is_clean => {
                    if attempt == UPDATE_ATTEMPTS {
                        bail!(
                            "{} keeps changing: gave up after {UPDATE_ATTEMPTS} attempts; \
                             nothing was saved — run the command again",
                            self.shown
                        );
                    }
                    back_off(attempt);
                    (base, etag) = self.get()?;
                    reread = true;
                }
                Err(e) => return Err(e),
            }
        }
        unreachable!("the loop returns or bails")
    }
}

/// A base URL taken apart: `https://alex:pw@host/x` is curl's
/// `https://host/x`, the messages' `https://alex@host/x`, and the
/// credentials `alex:pw`.
struct UserInfo {
    base: String,
    shown: String,
    credentials: Option<Vec<u8>>,
}

fn split_userinfo(url: &str) -> UserInfo {
    let plain = |url: &str| UserInfo {
        base: url.to_string(),
        shown: url.to_string(),
        credentials: None,
    };
    let Some((scheme, rest)) = url.split_once("://") else {
        return plain(url);
    };
    let authority_end = rest.find('/').unwrap_or(rest.len());
    let (authority, path) = rest.split_at(authority_end);
    // The last `@`, as the location check reads the host.
    let Some((userinfo, host)) = authority.rsplit_once('@') else {
        return plain(url);
    };
    let (user, password) = userinfo.split_once(':').unwrap_or((userinfo, ""));
    let mut credentials = percent_decode(user);
    credentials.push(b':');
    credentials.extend(percent_decode(password));
    // The user as typed, up to a colon written `%3A` (what follows it is
    // a password as well); no user, no `@`.
    let lower = user.to_ascii_lowercase();
    let user = &user[..lower.find("%3a").unwrap_or(user.len())];
    let shown = match user {
        "" => format!("{scheme}://{host}{path}"),
        user => format!("{scheme}://{user}@{host}{path}"),
    };
    UserInfo {
        base: format!("{scheme}://{host}{path}"),
        shown,
        credentials: Some(credentials),
    }
}

/// `%XX` as the byte it stands for, as curl reads a URL's user and
/// password; anything else as it is.
fn percent_decode(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        // Two hex digits: `u8::from_str_radix` would take `+f` as well.
        let hex = bytes
            .get(i + 1..i + 3)
            .filter(|h| h.iter().all(u8::is_ascii_hexdigit))
            .and_then(|h| std::str::from_utf8(h).ok());
        match (bytes[i], hex.and_then(|h| u8::from_str_radix(h, 16).ok())) {
            (b'%', Some(byte)) => {
                out.push(byte);
                i += 3;
            }
            (byte, _) => {
                out.push(byte);
                i += 1;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_credentials_of_a_url_leave_it() {
        let url = split_userinfo("https://alex:p%40ss:w@host:8443/tasks");
        assert_eq!(url.base, "https://host:8443/tasks");
        assert_eq!(url.shown, "https://alex@host:8443/tasks");
        assert_eq!(url.credentials.as_deref(), Some(&b"alex:p@ss:w"[..]));
        let url = split_userinfo("http://alex@host");
        assert_eq!((url.base.as_str(), url.shown.as_str()), ("http://host", "http://alex@host"));
        assert_eq!(url.credentials.as_deref(), Some(&b"alex:"[..]));
        let url = split_userinfo("http://host:7272/x@y");
        assert_eq!(url.base, "http://host:7272/x@y");
        assert!(url.credentials.is_none());
        assert_eq!(percent_decode("a%2Fb%zz%4"), b"a/b%zz%4");
        // Review of R28: `%+f` is no escape.
        assert_eq!(percent_decode("%+f%-1%41"), b"%+f%-1A");
        // No user: no `@` in what is shown; a `%3A` in the user is where
        // the password starts.
        assert_eq!(split_userinfo("http://:pw@host").shown, "http://host");
        let url = split_userinfo("http://alex%3Apw@host");
        assert_eq!(url.shown, "http://alex@host");
        assert_eq!(url.credentials.as_deref(), Some(&b"alex:pw:"[..]));
    }

    #[test]
    fn the_token_and_the_credentials_both_go() {
        let backend = HttpBackend::new("http://u:pw@h", Some("t".into()));
        assert_eq!(backend.auth.token.as_deref(), Some("t"));
        assert_eq!(backend.auth.basic.as_deref(), Some(&b"u:pw"[..]));
        assert_eq!(backend.describe(), "http://u@h");
        let backend = HttpBackend::new("http://h", None);
        assert_eq!(backend.auth, Auth::default());
        assert!(!format!("{:?}", HttpBackend::new("http://u:pw@h", Some("t0k".into()))).contains("pw"));
    }
}
