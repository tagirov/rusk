//! Where a task database lives, read from a `rusk_db` / `RUSK_DB` /
//! `sync_remote` value. There is one reading of such a value, and every
//! place that takes one goes through it, so a value means the same thing
//! wherever it is written:
//!
//! - `http://…` / `https://…` (scheme in any case) — the API of a running
//!   `rusk serve`. The URL is the server's base: a trailing `/` or
//!   `/api/tasks` is dropped, a query, a fragment or a space is refused
//!   (a token belongs in `db_token` / `sync_token`, not in the URL);
//! - any other `scheme://…` — refused: rusk does not guess what an
//!   `ssh://`, `sftp://` or `file://` value was meant to be;
//! - `[user@]host:path`, the form `scp` reads — a file over ssh. The part
//!   before the first `:` must look like a host: no `/`, not a single
//!   letter on Windows (a drive), not a scheme that lost its `//`
//!   (`https:/host`), not starting with `-`, brackets or not (it would
//!   reach ssh as an option); an IPv6 host goes in brackets
//!   (`user@[::1]:/srv/tasks.json`).
//!   `~` and `~/` at the start of the remote path are the remote home
//!   directory — the one a relative remote path starts from anyway — and an
//!   empty path is that directory too;
//! - anything else — a local path. `~` and `~/…` are the home directory,
//!   a relative path is taken from the current directory, and a directory
//!   (one that exists, or a value ending in `/`) holds `tasks.json`. A local
//!   file whose name has a `:` in it is written with a `./` or `/` in front,
//!   which makes it a path in every case (`./notes@2026:tasks.json`).
//!
//! What the location holds — JSON, CSV, SQLite… — is not decided here:
//! that is the file name's last extension, read by the backends.

use anyhow::{Context, Result, bail};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    /// The database file on this machine: `~` expanded, `tasks.json`
    /// appended to a directory.
    Local(PathBuf),
    /// Base URL of a `rusk serve` API: lower-case scheme, no trailing `/`,
    /// no `/api/tasks`.
    Http(String),
    Ssh(SshLocation),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SshLocation {
    /// `[user@]host` as written, for messages (IPv6 brackets kept, so that
    /// `host:path` stays readable).
    pub host: String,
    /// What `ssh` is given as its destination: `[user@]host` without the
    /// IPv6 brackets.
    pub destination: String,
    /// The path on the remote. Relative paths start from the remote home
    /// directory; a directory (a path ending in `/`, or empty) stands for
    /// its `tasks.json`.
    pub path: String,
}

impl Location {
    /// Reads a location value. It is not empty: an empty `RUSK_DB` or
    /// `rusk_db` means "not set" and never gets here.
    pub fn parse(value: &str) -> Result<Self> {
        match classify(value)? {
            Shape::Http(url) => Ok(Location::Http(url)),
            Shape::Ssh(ssh) => Ok(Location::Ssh(ssh)),
            Shape::Local => local(Path::new(value), ends_with_separator(value.as_bytes())),
        }
    }

    /// [`parse`](Self::parse) for a value from the environment, which does
    /// not have to be UTF-8. A local path is fine either way — it is the
    /// file system's business — but a URL or an ssh target that is not
    /// valid text cannot be what the user meant, and saying so beats
    /// treating the variable as unset and writing somewhere else.
    pub fn parse_os(value: &OsStr) -> Result<Self> {
        if let Some(text) = value.to_str() {
            return Self::parse(text);
        }
        match classify(&value.to_string_lossy())? {
            Shape::Local => local(
                Path::new(value),
                ends_with_separator(value.as_encoded_bytes()),
            ),
            Shape::Http(_) | Shape::Ssh(_) => {
                bail!("a remote location has to be valid UTF-8 text")
            }
        }
    }
}

enum Shape {
    Local,
    Http(String),
    Ssh(SshLocation),
}

fn classify(value: &str) -> Result<Shape> {
    if value.trim().is_empty() {
        bail!("the location is empty");
    }
    if let Some((scheme, rest)) = split_scheme(value) {
        return match scheme.to_ascii_lowercase().as_str() {
            scheme @ ("http" | "https") => http(scheme, rest).map(Shape::Http),
            "ssh" | "sftp" | "scp" => bail!(
                "{scheme}:// URLs are not read — a file over ssh is written \
                 `user@host:/path/tasks.json`"
            ),
            "file" => bail!("file:// URLs are not read — write the path itself"),
            _ => bail!(
                "unknown scheme {scheme}:// — a database location is a path, \
                 `https://host` (rusk serve API) or `user@host:/path` (ssh)"
            ),
        };
    }
    if is_explicit_path(value) {
        return Ok(Shape::Local);
    }
    let Some(colon) = host_colon(value) else {
        return Ok(Shape::Local);
    };
    let (head, path) = (&value[..colon], &value[colon + 1..]);
    // A drive letter only where there are drives; elsewhere `h:/srv` is
    // the host `h`, as scp reads it.
    let is_drive = cfg!(windows) && head.len() == 1 && head.as_bytes()[0].is_ascii_alphabetic();
    if head.is_empty() || is_drive || head.contains(['/', '\\']) {
        return Ok(Shape::Local);
    }
    // `https:/host`, `HTTP:host`: a scheme that lost a slash, not a host
    // called `https`.
    if let lower @ ("http" | "https" | "ssh" | "sftp" | "scp" | "file") =
        head.to_ascii_lowercase().as_str()
    {
        bail!("`{head}:` is not a host — a URL is written `{lower}://…`");
    }
    ssh(head, path).map(Shape::Ssh)
}

/// `scheme://rest` with an RFC 3986 scheme of two letters or more (one
/// letter is a Windows drive: `C://tasks.json`).
fn split_scheme(value: &str) -> Option<(&str, &str)> {
    let (scheme, rest) = value.split_once("://")?;
    let mut chars = scheme.chars();
    let valid = scheme.len() >= 2
        && chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    valid.then_some((scheme, rest))
}

/// A value that is a path whatever follows: absolute, `./`, `../`, `~`.
fn is_explicit_path(value: &str) -> bool {
    let first = value.split(['/', '\\']).next().unwrap_or("");
    value.starts_with(['/', '\\']) || matches!(first, "." | ".." | "~")
}

/// The `:` that ends the host part: the first one outside `[…]` (or just
/// the first one, when a `[` is never closed — the host check says what is
/// wrong with it).
fn host_colon(value: &str) -> Option<usize> {
    let mut in_brackets = false;
    for (i, c) in value.char_indices() {
        match c {
            '[' => in_brackets = true,
            ']' => in_brackets = false,
            ':' if !in_brackets => return Some(i),
            _ => {}
        }
    }
    if in_brackets { value.find(':') } else { None }
}

/// `value` as an error message may quote it: control characters escaped,
/// and the password of anything that looks like a URL — valid or not, any
/// scheme, one slash or two, spaces in front — is `***` (the user stays:
/// it tells which account is meant). A value is quoted because it is
/// wrong, so the password is taken to run to the last `@`: a `/`, `?` or
/// `#` in it is no end (review of R28). Masking a little too much is fine.
pub fn shown(value: &str) -> String {
    let escaped = crate::printable::escape(value).into_owned();
    let trimmed = escaped.trim_start();
    let lead = &escaped[..escaped.len() - trimmed.len()];
    let Some((scheme, rest)) = trimmed.split_once(':') else {
        return escaped;
    };
    let is_scheme = scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'));
    let after = rest.trim_start_matches('/');
    let slashes = &rest[..rest.len() - after.len()];
    // No slash: `user@host:path`, the form ssh reads, which has no password.
    if !is_scheme || slashes.is_empty() {
        return escaped;
    }
    let Some((userinfo, host)) = after.rsplit_once('@') else {
        return escaped;
    };
    match userinfo.split_once(':') {
        Some((user, _)) if !user.contains('/') => format!("{lead}{scheme}:{slashes}{user}:***@{host}"),
        _ => escaped,
    }
}

fn http(scheme: &str, rest: &str) -> Result<String> {
    if rest.chars().any(|c| c.is_whitespace() || c.is_control()) {
        bail!("a URL cannot contain spaces");
    }
    if rest.contains(['?', '#']) {
        bail!(
            "the URL of a rusk server is its base address, without a query or a \
             fragment (a token goes in db_token / RUSK_DB_TOKEN or sync_token / \
             RUSK_SYNC_TOKEN)"
        );
    }
    if let Some(c) = rest
        .chars()
        .find(|c| matches!(c, '{' | '}' | '"' | '<' | '>' | '\\' | '`' | '|' | '^'))
    {
        bail!("'{c}' does not belong in a URL");
    }
    let rest = rest.trim_end_matches('/');
    let rest = rest
        .strip_suffix("/api/tasks")
        .unwrap_or(rest)
        .trim_end_matches('/');
    let authority = rest.split('/').next().unwrap_or("");
    let host = authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host);
    if host.is_empty() || host.starts_with(':') {
        bail!("the URL names no host");
    }
    // A port is a number: `http://alex:pw/x@host` — a `/` in a password —
    // reads as the host `alex` on the port `pw` (review of R28).
    let port = match host.strip_prefix('[') {
        Some(inner) => inner.split_once(']').and_then(|(_, rest)| rest.strip_prefix(':')),
        None => host.split_once(':').map(|(_, port)| port),
    };
    if let Some(port) = port
        && !port.is_empty()
        && !(port.bytes().all(|b| b.is_ascii_digit()) && port.parse::<u16>().is_ok())
    {
        bail!(
            "the port of a URL is a number from 0 to 65535 (a `/` or `@` in a password is \
             written %2F or %40)"
        );
    }
    Ok(format!("{scheme}://{rest}"))
}

fn ssh(head: &str, path: &str) -> Result<SshLocation> {
    if head.chars().any(|c| c.is_whitespace() || c.is_control()) {
        bail!("an ssh host cannot contain spaces");
    }
    let (user, host) = match head.rsplit_once('@') {
        Some((user, host)) => (Some(user), host),
        None => (None, head),
    };
    if user.is_some_and(str::is_empty) || host.is_empty() {
        bail!("`{head}` is not an ssh host — expected `[user@]host:path`");
    }
    let not_a_host = || {
        anyhow::anyhow!(
            "`{host}` is not a host — an IPv6 address goes in brackets \
             (`[::1]`); a local file with `[` in its name is written `./{head}:…`"
        )
    };
    let bare_host = match host.strip_prefix('[') {
        Some(inner) => match inner.strip_suffix(']') {
            Some(v6) if !v6.is_empty() && !v6.contains(['[', ']']) => v6,
            _ => return Err(not_a_host()),
        },
        None if host.contains(['[', ']']) => return Err(not_a_host()),
        None => host,
    };
    let destination = match user {
        Some(user) => format!("{user}@{bare_host}"),
        None => bare_host.to_string(),
    };
    // ssh reads a destination that starts with `-` as one of its options —
    // checked on what ssh is given, brackets taken off.
    if head.starts_with('-') || bare_host.starts_with('-') || destination.starts_with('-') {
        bail!("an ssh host cannot start with '-'");
    }
    Ok(SshLocation {
        host: head.to_string(),
        destination,
        path: remote_path(path)?,
    })
}

/// The remote shell never sees a `~`: rusk quotes every path it sends, and
/// a quoted `~` is a directory called `~`. A path relative to the remote
/// home directory is simply a relative path there.
fn remote_path(path: &str) -> Result<String> {
    if path == "~" || path == "~/" {
        return Ok(String::new());
    }
    if let Some(rest) = path.strip_prefix("~/") {
        return Ok(rest.trim_start_matches('/').to_string());
    }
    if path.starts_with('~') {
        bail!(
            "`~user` is not expanded on the remote — write the path from the \
             remote home directory, or the absolute path"
        );
    }
    Ok(path.to_string())
}

fn ends_with_separator(bytes: &[u8]) -> bool {
    bytes.ends_with(b"/") || (cfg!(windows) && bytes.ends_with(b"\\"))
}

/// A local database file: `~` expanded, `tasks.json` in a directory.
fn local(path: &Path, names_a_directory: bool) -> Result<Location> {
    let path = expand_home(path)?;
    let path = if names_a_directory || path.is_dir() {
        path.join("tasks.json")
    } else {
        path
    };
    Ok(Location::Local(path))
}

fn expand_home(path: &Path) -> Result<PathBuf> {
    let Ok(rest) = path.strip_prefix("~") else {
        return Ok(path.to_path_buf());
    };
    let home = dirs::home_dir().context("`~` stands for the home directory, which is unknown")?;
    Ok(if rest.as_os_str().is_empty() {
        home
    } else {
        home.join(rest)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(value: &str) -> Location {
        Location::parse(value).unwrap_or_else(|e| panic!("{value}: {e:#}"))
    }

    fn refused(value: &str) -> String {
        match Location::parse(value) {
            Ok(location) => panic!("{value} was read as {location:?}"),
            Err(e) => format!("{e:#}"),
        }
    }

    fn ssh(value: &str) -> SshLocation {
        match parse(value) {
            Location::Ssh(ssh) => ssh,
            other => panic!("{value} was read as {other:?}"),
        }
    }

    fn local_path(value: &str) -> PathBuf {
        match parse(value) {
            Location::Local(path) => path,
            other => panic!("{value} was read as {other:?}"),
        }
    }

    #[test]
    fn a_quoted_url_hides_its_password() {
        assert_eq!(shown("https://alex:s3cret@host/x y"), "https://alex:***@host/x y");
        assert_eq!(shown("HTTP://alex:s3cret@host"), "HTTP://alex:***@host");
        assert_eq!(shown("http://alex@host"), "http://alex@host");
        assert_eq!(shown("http://host/a:b@c"), "http://host/a:b@c");
        assert_eq!(shown("user:pw@host:/srv/tasks.json"), "user:pw@host:/srv/tasks.json");
        // Review of R28: whatever looks like a URL, valid or not.
        assert_eq!(shown("sftp://alex:s3cret@host/tasks.json"), "sftp://alex:***@host/tasks.json");
        assert_eq!(shown("https:/alex:s3cret@host"), "https:/alex:***@host");
        assert_eq!(shown(" http://alex:s3cret@host"), " http://alex:***@host");
        assert_eq!(shown("http://alex:s3#cr?et@host"), "http://alex:***@host");
        assert_eq!(shown("http://alex:s3/cret@127.0.0.1:9"), "http://alex:***@127.0.0.1:9");
        assert_eq!(shown("C:/Users/x@y"), "C:/Users/x@y");
    }

    /// Review of R28: `http://alex:pw/x@host` was taken, as the host
    /// `alex` on the port `pw/x@host`'s `pw`.
    #[test]
    fn a_port_is_a_number() {
        for bad in ["http://alex:se/cret@127.0.0.1:9", "http://host:http", "http://host:70000", "http://[::1]:x"] {
            let err = format!("{:#}", Location::parse(bad).unwrap_err());
            assert!(err.contains("the port of a URL is a number"), "{bad}: {err}");
        }
        for good in ["http://host:7272", "http://[::1]:7272", "http://u:p@host:1", "http://host:"] {
            assert!(Location::parse(good).is_ok(), "{good}");
        }
    }

    #[test]
    fn http_urls_are_normalized_to_the_server_base() {
        for (value, base) in [
            ("https://tasks.example.com/", "https://tasks.example.com"),
            ("HTTP://Tasks.example.com", "http://Tasks.example.com"),
            ("Https://h:8443/rusk//", "https://h:8443/rusk"),
            ("http://127.0.0.1:7272/api/tasks", "http://127.0.0.1:7272"),
            ("http://127.0.0.1:7272/api/tasks/", "http://127.0.0.1:7272"),
            ("http://[::1]:7272", "http://[::1]:7272"),
            ("https://user:pw@h", "https://user:pw@h"),
        ] {
            assert_eq!(parse(value), Location::Http(base.to_string()), "{value}");
        }
    }

    #[test]
    fn http_urls_that_cannot_be_a_base_are_refused() {
        assert!(refused("http://h/?token=abc").contains("token goes in"));
        assert!(refused("http://h/#frag").contains("without a query"));
        assert!(refused("http://h ").contains("spaces"));
        assert!(refused("http://h/{a,b}").contains("'{'"));
        assert!(refused("https://").contains("no host"));
        assert!(refused("https:///path").contains("no host"));
    }

    #[test]
    fn other_schemes_are_refused_not_turned_into_paths() {
        assert!(refused("ssh://alex@vps/srv/tasks.json").contains("user@host:/path"));
        assert!(refused("SFTP://alex@vps/srv/tasks.json").contains("user@host:/path"));
        assert!(refused("file:///tmp/tasks.json").contains("path itself"));
        assert!(refused("ftp://h/tasks.json").contains("unknown scheme ftp://"));
    }

    #[test]
    fn scp_form_is_ssh_with_or_without_user() {
        let s = ssh("alex@vps:/srv/tasks.json");
        assert_eq!(
            (s.host.as_str(), s.destination.as_str(), s.path.as_str()),
            ("alex@vps", "alex@vps", "/srv/tasks.json")
        );
        let s = ssh("vps:/srv/tasks.json");
        assert_eq!(
            (s.host.as_str(), s.path.as_str()),
            ("vps", "/srv/tasks.json")
        );
        let s = ssh("vps:tasks.json");
        assert_eq!(s.path, "tasks.json");
        // Nothing after the colon: the remote home directory.
        assert_eq!(ssh("alex@vps:").path, "");
    }

    #[test]
    fn ipv6_hosts_go_in_brackets() {
        let s = ssh("u@[::1]:/srv/tasks.json");
        assert_eq!(s.host, "u@[::1]");
        assert_eq!(s.destination, "u@::1");
        assert_eq!(s.path, "/srv/tasks.json");
        assert_eq!(ssh("[fe80::1%eth0]:t.json").destination, "fe80::1%eth0");
        assert!(refused("u@[::1:/srv").contains("brackets"));
        assert!(refused("u@[]:/srv").contains("brackets"));
        assert!(refused("a[1]:x.json").contains("./a[1]:"));
    }

    #[test]
    fn a_host_is_never_an_ssh_option() {
        assert!(refused("-oProxyCommand=x@h:/srv/tasks.json").contains("'-'"));
        // With a `/` before the colon it is a (strange) local path, and
        // nothing is handed to ssh.
        assert!(matches!(
            parse("-oProxyCommand=touch /tmp/x@h:/srv/tasks.json"),
            Location::Local(_)
        ));
        assert!(refused("u@-oProxyCommand=x:/srv").contains("'-'"));
        assert!(refused("[-oProxyCommand=touch${IFS}pwned]:t.json").contains("'-'"));
        assert!(refused("[-v]:t.json").contains("'-'"));
        // No host starts with `-`, so there is no need to tell `u@-v` apart.
        assert!(refused("u@[-v]:t.json").contains("'-'"));
        assert!(refused("@h:/srv").contains("not an ssh host"));
        assert!(refused("u@:/srv").contains("not an ssh host"));
        assert!(refused("my host:/srv").contains("spaces"));
    }

    #[test]
    fn remote_tilde_is_the_remote_home() {
        assert_eq!(ssh("u@h:~/tasks/tasks.json").path, "tasks/tasks.json");
        assert_eq!(ssh("u@h:~").path, "");
        assert_eq!(ssh("u@h:~/").path, "");
        assert!(refused("u@h:~bob/tasks.json").contains("~user"));
    }

    #[test]
    fn paths_stay_local() {
        for value in [
            "/tmp/some/tasks.json",
            "/srv/l13/dir@x/a:b.json",
            "./notes@2026:tasks.json",
            "../x@y:z.json",
            "plain.json",
            ":odd.json",
            "rel/dir@host:x.json",
        ] {
            assert!(matches!(parse(value), Location::Local(_)), "{value}");
        }
        assert_eq!(
            local_path("/tmp/some/tasks.json"),
            PathBuf::from("/tmp/some/tasks.json")
        );
    }

    #[test]
    fn a_one_letter_head_is_a_drive_only_on_windows() {
        for value in [r"C:\tasks\tasks.json", "C:tasks.json", "h:/srv/tasks.json"] {
            let local = matches!(parse(value), Location::Local(_));
            assert_eq!(local, cfg!(windows), "{value}");
        }
    }

    #[test]
    fn a_scheme_without_its_slashes_is_not_a_host() {
        for value in [
            "https:/tasks.example.com",
            "HTTP:tasks.example.com",
            "ssh:/srv/t.json",
            "file:/tmp/t.json",
        ] {
            assert!(refused(value).contains("is not a host"), "{value}");
        }
    }

    #[test]
    fn a_directory_holds_tasks_json() {
        assert_eq!(
            local_path("/tmp/some/dir/"),
            PathBuf::from("/tmp/some/dir/tasks.json")
        );
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            local_path(dir.path().to_str().unwrap()),
            dir.path().join("tasks.json")
        );
    }

    #[test]
    fn local_tilde_is_the_home_directory() {
        let Some(home) = dirs::home_dir() else { return };
        assert_eq!(
            local_path("~/tilde/tasks.json"),
            home.join("tilde/tasks.json")
        );
        assert_eq!(
            local_path("~/new-dir/"),
            home.join("new-dir").join("tasks.json")
        );
        // Bare `~` is the home directory itself, which holds tasks.json.
        if home.is_dir() {
            assert_eq!(local_path("~"), home.join("tasks.json"));
        }
        // `~name` is a file name like any other.
        assert_eq!(local_path("~name.json"), PathBuf::from("~name.json"));
    }

    #[test]
    fn empty_values_are_refused() {
        assert!(refused("").contains("empty"));
        assert!(refused("  ").contains("empty"));
    }

    #[cfg(unix)]
    #[test]
    fn non_utf8_env_values_are_local_paths_or_errors() {
        use std::os::unix::ffi::OsStrExt;
        let raw = OsStr::from_bytes(b"/tmp/caf\xe9/tasks.json");
        assert_eq!(
            Location::parse_os(raw).unwrap(),
            Location::Local(PathBuf::from(raw))
        );
        let remote = OsStr::from_bytes(b"u@h:/srv/caf\xe9.json");
        assert!(format!("{:#}", Location::parse_os(remote).unwrap_err()).contains("UTF-8"));
        let url = OsStr::from_bytes(b"https://caf\xe9");
        assert!(Location::parse_os(url).is_err());
    }
}
