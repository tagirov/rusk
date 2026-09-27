//! Process-based network transports shared by the remote database backends
//! and `rusk sync`: the system `ssh` and `curl` binaries do the wire work,
//! so the user's keys, agent, `~/.ssh/config` and proxy setup just work and
//! rusk needs no TLS or ssh dependencies.

use anyhow::{Context, Result, bail};
use std::io::Write;
use std::process::{Command, Stdio};

pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

fn run_tool(cmd: Command, stdin_data: Option<&[u8]>, tool: &str) -> Result<Vec<u8>> {
    let output = spawn_tool(cmd, stdin_data, tool)?;
    if !output.status.success() {
        bail!("{tool} failed ({}): {}", output.status, stderr_of(&output));
    }
    Ok(output.stdout)
}

fn stderr_of(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).trim().to_string()
}

/// Runs the tool to completion and hands back everything it said.
///
/// The data goes into its stdin from a thread of its own while this one
/// drains stdout and stderr. Writing from here instead would deadlock
/// against a tool that fills the pipe back before it has read its input
/// (`ssh -v` on a database of a few megabytes), and it would also hide the
/// ordinary failure: a tool that dies early — ssh that cannot connect, a
/// remote shell that cannot create the file — leaves the write with a
/// broken pipe, and reporting *that* buries the tool's own words about why
/// it died. So the write error is only worth raising when the tool itself
/// was happy, or when it is something other than the pipe closing.
fn spawn_tool(
    mut cmd: Command,
    stdin_data: Option<&[u8]>,
    tool: &str,
) -> Result<std::process::Output> {
    cmd.stdin(if stdin_data.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            anyhow::anyhow!("`{tool}` not found in PATH (required for this database location)")
        } else {
            anyhow::anyhow!("failed to run {tool}: {e}")
        }
    })?;
    let mut pipe = child.stdin.take();

    std::thread::scope(|scope| {
        let writer = stdin_data.map(|data| {
            let mut pipe = pipe.take().expect("stdin piped");
            // Dropping the pipe at the end of the closure is the EOF the
            // remote `cat` waits for.
            scope.spawn(move || pipe.write_all(data).and_then(|()| pipe.flush()))
        });
        let output = child
            .wait_with_output()
            .with_context(|| format!("failed to wait for {tool}"))?;
        let wrote = writer.map(|handle| handle.join());
        match wrote {
            // Only when the tool itself was happy: one that died has
            // already said why on its stderr, and the write that then hit
            // a closed pipe would bury it.
            Some(Ok(Err(e))) if output.status.success() => Err(anyhow::Error::new(e)
                .context(format!("failed to stream data to {tool}"))),
            Some(Err(_)) if output.status.success() => {
                bail!("failed to stream data to {tool}")
            }
            _ => Ok(output),
        }
    })
}

/// The exit status the remote shell is told to use for "there is no such
/// path". A status, not a marker in the output: whatever the remote login
/// shell may print of its own (a banner, an `echo` in an rc file) cannot
/// turn a missing file into an empty one or the other way round.
const NO_SUCH_PATH: i32 = 9;

/// A fence printed right before and right after the file content, so that
/// whatever the remote login shell says of its own — a banner, an `echo` in
/// an rc file, a "Last login" line — stays out of the database. It is
/// unpredictable per call, and the content is taken between the *first* and
/// the *last* fence, so a file that happens to contain one is still read
/// whole.
fn fence() -> String {
    format!("--rusk-{:016x}--", crate::atomic::unique())
}

/// The bytes between the first and the last `fence`, or what to say when
/// the fence is not there at all.
fn between_fences(raw: &[u8], fence: &str) -> Result<Vec<u8>> {
    let mark = fence.as_bytes();
    let (Some(start), Some(end)) = (find(raw, mark), rfind(raw, mark)) else {
        bail!(
            "the remote answered with something that is not the file: {}",
            preview(raw)
        );
    };
    let (from, to) = (start + mark.len(), end);
    if to < from {
        bail!("the remote answered with a truncated file: {}", preview(raw));
    }
    Ok(raw[from..to].to_vec())
}

/// The head of what came back, for an error message.
fn preview(raw: &[u8]) -> String {
    let text: String = String::from_utf8_lossy(raw)
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .take(120)
        .collect();
    let text = text.trim().to_string();
    if text.is_empty() {
        "nothing at all".to_string()
    } else {
        format!("'{text}'")
    }
}

/// Reads a remote file over ssh. `None` means there is nothing at that
/// path — the database is yet to be created.
///
/// Only that is `None`. Anything that exists but cannot be read
/// (permissions, a directory, I/O errors) makes `cat` fail and surfaces as
/// an error: reading it as an empty database would let the next save
/// overwrite the real data.
pub fn ssh_read_file(target: &str, path: &str) -> Result<Option<Vec<u8>>> {
    let fence = fence();
    let mut cmd = Command::new("ssh");
    cmd.arg(target).arg(ssh_read_command(path, &fence));
    let output = spawn_tool(cmd, None, "ssh")?;
    if output.status.code() == Some(NO_SUCH_PATH) {
        return Ok(None);
    }
    if !output.status.success() {
        bail!("ssh failed ({}): {}", output.status, stderr_of(&output));
    }
    Ok(Some(between_fences(&output.stdout, &fence)?))
}

/// The remote line, run through `sh -c` so that the login shell on the
/// other side — which may be fish, csh or anything else — only has to pass
/// one string on, and the script itself is plain POSIX sh.
///
/// `cat` answers whenever there is something to read or something to
/// complain about; only three cases end as "nothing is there":
///
/// - the path holds nothing and its directory can be searched, so the
///   answer "not there" is one the remote could actually give;
/// - the directory does not exist either (a save creates it);
/// - never a symbolic link whose target is gone: `-e` follows the link and
///   says no while the link is right there, and a save would replace it.
fn ssh_read_command(path: &str, fence: &str) -> String {
    // The path goes as an argument of the script, not inside it: that
    // leaves one level of quoting for the login shell to undo, and one
    // level is what every shell agrees about (fish and csh read a
    // backslash inside single quotes differently than sh does, which a
    // script quoted into a script would run into).
    format!(
        "sh -c 'if test -e \"$1\" || test -L \"$1\" || \
         {{ test -d \"$2\" && test ! -x \"$2\"; }}; then \
         printf %s \"$3\"; cat -- \"$1\" && printf %s \"$3\"; \
         else exit {NO_SUCH_PATH}; fi' rusk {} {} {}",
        shell_quote(path),
        shell_quote(parent_dir(path)),
        shell_quote(fence)
    )
}

/// The directory part of a remote path, as the remote shell sees it.
fn parent_dir(path: &str) -> &str {
    match path.rsplit_once('/') {
        Some(("", _)) => "/",
        Some((dir, _)) => dir,
        None => ".",
    }
}

/// Prefix of the remote script's own complaints, so that what rusk asked it
/// to say is told apart from whatever else the remote shell prints.
const REMOTE_SAYS: &str = "rusk:";

/// Writes a remote file over ssh atomically: stream to a temp sibling of
/// its own, check that all of it arrived, then `mv` into place. Parent
/// directories are created as needed; with `backup`, the content about to
/// be replaced is kept as a `.backup` sibling first, as it is locally.
///
/// Nothing is replaced unless the remote file has exactly the byte count
/// that was sent: a transfer that ends early — rusk killed mid-stream, the
/// remote disk full — leaves the previous database where it was, and the
/// temp file is removed. (Corruption that keeps the length is ssh's own
/// business: every byte it carries is under a MAC.)
pub fn ssh_write_file(target: &str, path: &str, data: &[u8], backup: bool) -> Result<()> {
    let mut cmd = Command::new("ssh");
    cmd.arg(target)
        .arg(ssh_write_command(path, data.len(), backup));
    let output = spawn_tool(cmd, Some(data), "ssh")?;
    if !output.status.success() {
        bail!("ssh failed ({}): {}", output.status, stderr_of(&output));
    }
    // The write went through, but something rusk asked the remote to
    // attempt on the side did not (a `.backup` it could not copy).
    for line in String::from_utf8_lossy(&output.stderr).lines() {
        if let Some(message) = line.trim().strip_prefix(REMOTE_SAYS) {
            crate::backend::warn_once(&format!("Warning:{message}"));
        }
    }
    Ok(())
}

/// The remote line for a write. POSIX sh, path as an argument (see
/// [`ssh_read_command`]), and every step that can go wrong says so on
/// stderr instead of leaving a status to guess from:
///
/// - a symbolic link is followed one level, so a remote `tasks.json` that
///   points somewhere stays a link, as it does locally;
/// - a directory in place of the database is named, not silently filled
///   with a `tasks.json/tasks.json.tmp`;
/// - the temp file carries a per-call suffix, so two writers never share
///   one, and it is removed whenever it is not renamed away;
/// - the `.backup` copy is best effort: it is not worth losing a save.
fn ssh_write_command(path: &str, len: usize, backup: bool) -> String {
    // One line on purpose: csh and tcsh do not allow a literal newline
    // inside quotes, and the login shell on the other side is whatever the
    // remote account was given. `--` before every path, so a database
    // called `-odd.json` is a name and not an option.
    const SCRIPT: &str = concat!(
        "mkdir -p -- \"$2\" || exit 1; ",
        "dst=$1; ",
        "if test -L \"$dst\"; then ",
        "link=$(readlink -- \"$dst\" 2>/dev/null); ",
        "if test -n \"$link\"; then ",
        "case $link in /*) dst=$link ;; ",
        "*) case $dst in */*) dst=${dst%/*}/$link ;; *) dst=$link ;; esac ;; esac; ",
        "fi; fi; ",
        "if test -d \"$dst\"; then ",
        "echo \"rusk: $dst is a directory on the remote, not a task file\" >&2; exit 20; fi; ",
        "tmp=$dst.$4.tmp; ",
        "cat > \"$tmp\" || { ",
        "echo \"rusk: cannot write $tmp on the remote\" >&2; rm -f -- \"$tmp\"; exit 21; }; ",
        "got=$(wc -c < \"$tmp\" | tr -dc 0-9); ",
        "if test -z \"$got\" || test \"$got\" != \"$3\"; then ",
        "rm -f -- \"$tmp\"; ",
        "echo \"rusk: only $got of $3 bytes reached $dst; it was left as it was\" >&2; ",
        "exit 22; fi; ",
        "if test -n \"$5\" && test -f \"$1\"; then ",
        "cp -p -- \"$1\" \"$1.backup\" 2>/dev/null || ",
        "echo \"rusk: could not keep $1.backup on the remote\" >&2; fi; ",
        "mv -- \"$tmp\" \"$dst\" || { ",
        "echo \"rusk: cannot put $tmp in place of $dst\" >&2; rm -f -- \"$tmp\"; exit 23; }",
    );

    format!(
        "sh -c {} rusk {} {} {len} {:016x} {}",
        shell_quote(SCRIPT),
        shell_quote(path),
        shell_quote(parent_dir(path)),
        crate::atomic::unique(),
        shell_quote(if backup { "1" } else { "" }),
    )
}

/// What one curl call brought back. Any HTTP status is a response; only a
/// failed transfer (DNS, connect, TLS, timeout) is an error.
#[derive(Debug)]
pub struct HttpResponse {
    pub status: u16,
    /// The `ETag` header, without quotes and weakness prefix.
    pub etag: Option<String>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// For error messages: the `error` field of a rusk API reply, or the
    /// start of whatever else the server said.
    pub fn error_text(&self) -> String {
        if let Ok(value) = serde_json::from_slice::<serde_json::Value>(&self.body)
            && let Some(message) = value.get("error").and_then(|e| e.as_str())
        {
            return message.to_string();
        }
        let text = String::from_utf8_lossy(&self.body);
        text.trim().chars().take(200).collect()
    }
}

/// A header that curl reads from a file (`-H @file`) instead of from its
/// command line, which every local user can read in `ps` or
/// `/proc/<pid>/cmdline` (REVIEW №103). The file is created new and
/// readable by its owner only, and removed again when this is dropped. (Not
/// curl's config on its standard input: that holds the request body, and a
/// config line of 10 MiB is curl's limit.)
struct HeaderFile(std::path::PathBuf);

/// A header file older than this was left by a process that ended while
/// curl ran (Ctrl+C), and goes with the next request.
const STALE_HEADER_FILE: std::time::Duration = std::time::Duration::from_secs(600);

impl HeaderFile {
    fn new(header: &str) -> Result<Self> {
        if header.contains(['\r', '\n']) {
            bail!("a token for the http location must not contain a line break");
        }
        let dir = Self::dir();
        Self::sweep(&dir);
        let path = dir.join(format!("rusk-header-{:016x}", crate::atomic::unique()));
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&path)
            .with_context(|| format!("failed to create '{}' for curl", path.display()))?;
        let created = Self(path);
        file.write_all(header.as_bytes())
            .and_then(|()| file.write_all(b"\n"))
            .with_context(|| format!("failed to write '{}' for curl", created.0.display()))?;
        Ok(created)
    }

    /// Where it goes: the user's runtime directory, private to the user
    /// and in memory on most systems, when there is one; else the temp
    /// directory (review of R21: a temp directory that cannot be written
    /// failed every request with a token).
    fn dir() -> std::path::PathBuf {
        std::env::var_os("XDG_RUNTIME_DIR")
            .map(std::path::PathBuf::from)
            .filter(|dir| dir.is_absolute() && dir.is_dir())
            .unwrap_or_else(std::env::temp_dir)
    }

    /// Removes the header files a process that ended while curl ran left in
    /// `dir` (review of R21); another user's, in a shared temp directory,
    /// are not ours to remove and stay.
    fn sweep(dir: &std::path::Path) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            if !entry.file_name().to_string_lossy().starts_with("rusk-header-") {
                continue;
            }
            let stale = entry
                .metadata()
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|modified| modified.elapsed().ok())
                .is_some_and(|age| age > STALE_HEADER_FILE);
            if stale {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }

    /// The `-H` argument that names it.
    fn arg(&self) -> std::ffi::OsString {
        let mut arg = std::ffi::OsString::from("@");
        arg.push(&self.0);
        arg
    }
}

impl Drop for HeaderFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// One curl call. `method` of `None` is a plain GET; a JSON `body` is sent
/// with the right Content-Type; `token` becomes a Bearer header, handed to
/// curl in a file; `if_match` makes the request conditional on the server
/// still holding that `ETag`.
pub fn http_request(
    url: &str,
    method: Option<&str>,
    token: Option<&str>,
    json_body: Option<&[u8]>,
    if_match: Option<&str>,
) -> Result<HttpResponse> {
    let mut cmd = Command::new("curl");
    // `-i` puts the response headers in front of the body: the status and
    // the ETag are needed, and `-f` would hide both behind an exit code.
    // `-g`: a URL is an address, never a curl glob pattern (`[…]`, `{…}`).
    cmd.args(["-sS", "-i", "-g", "--max-time", "30"]);
    if let Some(method) = method {
        cmd.args(["-X", method]);
    }
    if json_body.is_some() {
        cmd.args(["-H", "Content-Type: application/json", "--data-binary", "@-"]);
    }
    // Kept until curl is done with it.
    let token_file = token
        .map(|token| HeaderFile::new(&format!("Authorization: Bearer {token}")))
        .transpose()?;
    if let Some(file) = &token_file {
        cmd.arg("-H").arg(file.arg());
    }
    if let Some(etag) = if_match {
        cmd.args(["-H", &format!("If-Match: \"{etag}\"")]);
    }
    cmd.arg(url);
    parse_http_output(&run_tool(cmd, json_body, "curl")?)
}

/// Splits `curl -i` output. Interim responses come first, each with a
/// header block of its own (`100 Continue`, a proxy's `200 Connection
/// established`); the first block that is neither belongs to the response,
/// and everything after it is the body — whatever the body looks like.
fn parse_http_output(raw: &[u8]) -> Result<HttpResponse> {
    let mut rest = raw;
    loop {
        if !rest.starts_with(b"HTTP/") {
            bail!("curl returned no HTTP response headers");
        }
        // The blank line that ends the block, whichever line ending comes
        // first.
        let (end, skip) = [(find(rest, b"\r\n\r\n"), 4), (find(rest, b"\n\n"), 2)]
            .into_iter()
            .filter_map(|(at, skip)| at.map(|at| (at, skip)))
            .min()
            .unwrap_or((rest.len(), 0));
        let head = String::from_utf8_lossy(&rest[..end]);
        rest = &rest[end + skip..];

        let mut lines = head.lines();
        let status_line = lines.next().unwrap_or_default();
        let status = status_line
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse::<u16>().ok())
            .context("curl returned a malformed HTTP status line")?;
        let interim = (100..200).contains(&status)
            || status_line
                .to_ascii_lowercase()
                .ends_with("connection established");
        if interim && rest.starts_with(b"HTTP/") {
            continue;
        }
        let etag = lines.find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.trim()
                .eq_ignore_ascii_case("etag")
                .then(|| crate::revision::clean_etag(value))
        });
        return Ok(HttpResponse {
            status,
            etag,
            body: rest.to_vec(),
        });
    }
}

fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

fn rfind(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).rposition(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[allow(unused_imports)]
    use std::path::Path;

    /// `shell` may name a shell that takes the script as an argument of its
    /// own (`busybox sh -c …`), so the command line is split, not assumed.
    #[cfg(unix)]
    fn shell_command(shell: &str, script: &str) -> Command {
        let mut words = shell.split_whitespace();
        let mut cmd = Command::new(words.next().unwrap());
        cmd.args(words).arg("-c").arg(script);
        cmd
    }

    #[cfg(unix)]
    fn run_in(shell: &str, script: &str) -> std::io::Result<std::process::Output> {
        shell_command(shell, script).output()
    }

    #[cfg(unix)]
    fn spawn_in(shell: &str, script: &str) -> std::io::Result<std::process::Child> {
        shell_command(shell, script)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
    }

    /// A path where nothing is answers with its own exit status; everything
    /// else that goes wrong is `cat`'s failure and must stay one. Run for
    /// real, against every shape a remote path can have — the script is
    /// what the remote executes, and a string comparison would not notice
    /// it doing something else.
    #[test]
    #[cfg(unix)]
    fn read_command_tells_a_path_that_holds_nothing_from_a_failing_cat() {
        let dir = tempfile::tempdir().unwrap();
        let at = |name: &str| dir.path().join(name).display().to_string();
        std::fs::write(dir.path().join("tasks.json"), "[]").unwrap();
        std::os::unix::fs::symlink(dir.path().join("gone.json"), dir.path().join("dangling.json"))
            .unwrap();
        std::fs::create_dir(dir.path().join("a dir")).unwrap();

        let run = |path: &str| {
            let fence = fence();
            let out = Command::new("sh")
                .arg("-c")
                .arg(ssh_read_command(path, &fence))
                .output()
                .unwrap();
            let content = between_fences(&out.stdout, &fence)
                .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                .unwrap_or_default();
            (out.status.code(), content)
        };

        assert_eq!(run(&at("tasks.json")), (Some(0), "[]".to_string()));
        // Nothing there, and a directory that could have held it.
        assert_eq!(run(&at("none.json")).0, Some(NO_SUCH_PATH));
        assert_eq!(run(&at("with spaces/none.json")).0, Some(NO_SUCH_PATH));
        assert_eq!(run("relative.json").0, Some(NO_SUCH_PATH));
        // Something is there, and `cat` says what is wrong with it.
        assert_eq!(run(&at("dangling.json")).0, Some(1));
        assert_eq!(run(&at("a dir")).0, Some(1));
        // A quote in the path survives the trip through two shells.
        std::fs::write(dir.path().join("it's here.json"), "[1]").unwrap();
        assert_eq!(run(&at("it's here.json")), (Some(0), "[1]".to_string()));

        let cmd = ssh_read_command("/srv/tasks.json", "--fence--");
        assert!(cmd.starts_with("sh -c "), "{cmd}");
        assert!(!cmd.contains("|| true"));
    }

    /// REVIEW №181: whatever the remote login shell prints of its own — a
    /// banner, an `echo` in an rc file — is not the database. Run for real:
    /// a shell that talks before and after the command, and a file that
    /// contains the fence itself.
    #[test]
    #[cfg(unix)]
    fn a_chatty_remote_shell_stays_out_of_the_file_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tasks.txt");
        let fence = fence();
        std::fs::write(&path, format!("real task\n{fence} in the text\nsecond")).unwrap();

        let out = Command::new("sh")
            .arg("-c")
            .arg(format!(
                "echo 'Welcome to vps!'; {}; echo 'Bye'",
                ssh_read_command(&path.display().to_string(), &fence)
            ))
            .output()
            .unwrap();

        let content = between_fences(&out.stdout, &fence).unwrap();
        assert_eq!(
            String::from_utf8_lossy(&content),
            format!("real task\n{fence} in the text\nsecond")
        );
    }

    /// A remote that never ran the script at all (no `sh`, a shell that
    /// rejected the line) is an error that shows what did come back — not
    /// an empty database.
    #[test]
    fn a_missing_fence_is_an_error_that_shows_what_came_back() {
        let err = between_fences(b"Welcome to vps!\r\n", "--fence--").unwrap_err();
        let text = format!("{err:#}");
        assert!(text.contains("Welcome to vps!"), "{text}");
        assert!(between_fences(b"", "--fence--").is_err());
        assert_eq!(between_fences(b"--fence--[]--fence--", "--fence--").unwrap(), b"[]");
        // A file that is empty, and one that is nothing but a fence.
        assert_eq!(between_fences(b"--fence----fence--", "--fence--").unwrap(), b"");
        assert_eq!(
            between_fences(b"--fence----fence----fence--", "--fence--").unwrap(),
            b"--fence--"
        );
    }

    /// REVIEW №25, №53, №180, and the ssh half of №19: what the write
    /// script does, run for real under every shell that could be the login
    /// shell on the other side. A string comparison would not notice it
    /// doing something else.
    #[test]
    #[cfg(unix)]
    fn the_write_script_replaces_only_what_it_wrote_whole() {
        use std::os::unix::fs::PermissionsExt;

        let shells: Vec<&str> = ["sh", "bash", "dash", "zsh", "ksh", "ash", "busybox sh"]
            .into_iter()
            .filter(|shell| run_in(shell, "exit 0").is_ok_and(|o| o.status.success()))
            .collect();
        assert!(!shells.is_empty(), "no POSIX shell to test with");

        // REVIEW №25: a remote path that starts with a dash is a name, not
        // an option — `mv`/`rm`/`cp` used to refuse it and leave a temp
        // file behind on every single save.
        let dashes = tempfile::tempdir().unwrap();
        let odd = dashes.path().join("-odd.json").display().to_string();
        let mut child = Command::new("sh")
            .arg("-c")
            .arg(ssh_write_command(&odd, 3, true))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child.stdin.take().unwrap().write_all(b"[1]").unwrap();
        let out = child.wait_with_output().unwrap();
        assert!(out.status.success(), "{}", stderr_of(&out));
        assert_eq!(std::fs::read_to_string(&odd).unwrap(), "[1]");
        let leftovers: Vec<_> = std::fs::read_dir(dashes.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.ends_with(".tmp"))
            .collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");
        let fence = fence();
        let read = Command::new("sh")
            .arg("-c")
            .arg(ssh_read_command(&odd, &fence))
            .output()
            .unwrap();
        assert_eq!(between_fences(&read.stdout, &fence).unwrap(), b"[1]");

        for shell in shells {
            let dir = tempfile::tempdir().unwrap();
            let at = |name: &str| dir.path().join(name).display().to_string();
            let write = |path: &str, data: &str, backup: bool| {
                let mut child = spawn_in(shell, &ssh_write_command(path, data.len(), backup))
                    .unwrap();
                child
                    .stdin
                    .take()
                    .unwrap()
                    .write_all(data.as_bytes())
                    .unwrap();
                child.wait_with_output().unwrap()
            };

            // A new database, in a directory that is not there yet.
            let fresh = at("deep/nested/tasks.json");
            assert!(write(&fresh, "[1]", true).status.success(), "{shell}");
            assert_eq!(std::fs::read_to_string(&fresh).unwrap(), "[1]");

            // A save keeps what it replaces, and leaves no temp behind.
            assert!(write(&fresh, "[2]", true).status.success(), "{shell}");
            assert_eq!(std::fs::read_to_string(&fresh).unwrap(), "[2]");
            assert_eq!(
                std::fs::read_to_string(format!("{fresh}.backup")).unwrap(),
                "[1]"
            );
            let mut left: Vec<_> = std::fs::read_dir(dir.path().join("deep/nested"))
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect();
            left.sort();
            assert_eq!(left, ["tasks.json", "tasks.json.backup"], "{shell}");

            // Without the backup flag, no copy is made.
            let plain = at("plain.json");
            write(&plain, "[1]", false);
            write(&plain, "[2]", false);
            assert!(!Path::new(&format!("{plain}.backup")).exists(), "{shell}");

            // REVIEW №53: a stream that ends early replaces nothing, says
            // how much arrived, and takes its temp file with it.
            let short = at("short.json");
            std::fs::write(&short, "[1]").unwrap();
            // Announce 99 bytes and send three: what a rusk killed
            // mid-stream leaves the remote `cat` with.
            let mut child = spawn_in(shell, &ssh_write_command(&short, 99, false)).unwrap();
            child.stdin.take().unwrap().write_all(b"[9]").unwrap();
            let out = child.wait_with_output().unwrap();
            assert!(!out.status.success(), "{shell}");
            assert!(stderr_of(&out).contains("of 99 bytes"), "{shell}: {:?}", stderr_of(&out));
            assert_eq!(std::fs::read_to_string(&short).unwrap(), "[1]");
            let leftovers: Vec<_> = std::fs::read_dir(dir.path())
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .filter(|name| name.ends_with(".tmp"))
                .collect();
            assert!(leftovers.is_empty(), "{shell}: {leftovers:?}");

            // REVIEW №25: a directory in place of the database is named.
            let isdir = at("isdir.json");
            std::fs::create_dir(&isdir).unwrap();
            let out = write(&isdir, "[1]", false);
            assert!(!out.status.success(), "{shell}");
            assert!(stderr_of(&out).contains("is a directory"), "{shell}");
            assert_eq!(std::fs::read_dir(&isdir).unwrap().count(), 0, "{shell}");

            // REVIEW №19 (the ssh half): a symbolic link is followed, not
            // replaced — relative and absolute alike.
            for (link, target) in [("rel.json", "target.json"), ("abs.json", &at("target.json"))] {
                std::fs::write(at("target.json"), "[0]").unwrap();
                let link = at(link);
                let _ = std::fs::remove_file(&link);
                std::os::unix::fs::symlink(target, &link).unwrap();
                assert!(write(&link, "[7]", false).status.success(), "{shell}");
                assert!(std::fs::symlink_metadata(&link).unwrap().is_symlink(), "{shell}");
                assert_eq!(std::fs::read_to_string(at("target.json")).unwrap(), "[7]");
            }

            // A destination nobody may write to fails, and says so.
            let locked = dir.path().join("locked");
            std::fs::create_dir(&locked).unwrap();
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o555)).unwrap();
            let out = write(&locked.join("t.json").display().to_string(), "[1]", false);
            std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
            if !out.status.success() {
                assert!(!stderr_of(&out).is_empty(), "{shell}");
            }
        }
    }

    /// A directory nobody may search cannot say whether the file is there,
    /// so `cat` gets to fail instead of the read passing as "not there".
    #[test]
    #[cfg(unix)]
    fn a_directory_that_cannot_be_searched_is_no_missing_file() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let locked = dir.path().join("locked");
        std::fs::create_dir(&locked).unwrap();
        let path = locked.join("tasks.json").display().to_string();
        std::fs::write(locked.join("tasks.json"), "[]").unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
        let readable_anyway = std::fs::read(locked.join("tasks.json")).is_ok();

        let out = Command::new("sh")
            .arg("-c")
            .arg(ssh_read_command(&path, "--fence--"))
            .output()
            .unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();

        if readable_anyway {
            eprintln!("skipping: running as a user that ignores file modes (root?)");
            return;
        }
        assert_eq!(out.status.code(), Some(1), "must not answer 'not there'");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("ermission"),
            "{:?}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// REVIEW №103: the Bearer token was on curl's command line, readable
    /// by every local user; it goes in a private file now.
    #[test]
    fn a_header_file_is_private_and_goes_away() {
        let file = HeaderFile::new("Authorization: Bearer s3cret").unwrap();
        let path = file.0.clone();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "Authorization: Bearer s3cret\n");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "{mode:o}");
        }
        assert!(file.arg().to_string_lossy().starts_with('@'));
        drop(file);
        assert!(!path.exists());
        assert!(HeaderFile::new("Authorization: Bearer a\nX-Evil: 1").is_err());
    }

    /// Review of R21: a Ctrl+C while curl ran left the file with the token.
    #[test]
    fn a_header_file_left_behind_goes_with_the_next_request() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("rusk-header-0000000000000001");
        let fresh = dir.path().join("rusk-header-0000000000000002");
        let other = dir.path().join("notes.txt");
        for path in [&old, &fresh, &other] {
            std::fs::write(path, "Authorization: Bearer s3cret\n").unwrap();
        }
        let long_ago = std::time::SystemTime::now() - 2 * STALE_HEADER_FILE;
        for path in [&old, &other] {
            std::fs::File::options().write(true).open(path).unwrap().set_modified(long_ago).unwrap();
        }
        HeaderFile::sweep(dir.path());
        assert!(!old.exists());
        assert!(fresh.exists() && other.exists());
    }

    #[test]
    fn curl_output_is_split_into_status_etag_and_body() {
        let res = parse_http_output(
            b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nETag: \"0123abcd\"\r\n\r\n[]",
        )
        .unwrap();
        assert_eq!((res.status, res.etag.as_deref()), (200, Some("0123abcd")));
        assert_eq!(res.body, b"[]");
        assert!(res.is_success());

        // Interim responses come first; HTTP/2 has no reason phrase; header
        // names are case-insensitive and a proxy may weaken the tag.
        let res = parse_http_output(
            b"HTTP/1.1 100 Continue\r\n\r\nHTTP/2 412 \r\netag: W/\"ff\"\r\n\r\n{\"error\":\"changed\"}",
        )
        .unwrap();
        assert_eq!((res.status, res.etag.as_deref()), (412, Some("ff")));
        assert_eq!(res.error_text(), "changed");
        assert!(!res.is_success());

        // No body, no ETag.
        let res = parse_http_output(b"HTTP/1.1 204 No Content\r\n\r\n").unwrap();
        assert_eq!((res.status, res.etag, res.body.len()), (204, None, 0));

        assert!(parse_http_output(b"").is_err());
        assert!(parse_http_output(b"<html>not http</html>").is_err());

        // A proxy tunnel announces itself before the response.
        let res = parse_http_output(
            b"HTTP/1.1 200 Connection established\r\n\r\nHTTP/1.1 200 OK\r\nETag: \"aa\"\r\n\r\n[]",
        )
        .unwrap();
        assert_eq!((res.status, res.etag.as_deref(), res.body.as_slice()), (200, Some("aa"), &b"[]"[..]));

        // The body is never read as headers, whatever it looks like...
        let res = parse_http_output(
            b"HTTP/1.1 200 OK\r\nETag: \"real\"\r\n\r\nHTTP/1.1 200 OK\r\nETag: \"evil\"\r\n\r\n[]",
        )
        .unwrap();
        assert_eq!(res.etag.as_deref(), Some("real"));
        assert!(res.body.starts_with(b"HTTP/1.1 200 OK"));
        // ...and bare line feeds end a header block as well as CRLF does.
        let res = parse_http_output(b"HTTP/1.0 200 OK\nETag: \"lf\"\n\nline\r\n\r\nmore").unwrap();
        assert_eq!(res.etag.as_deref(), Some("lf"));
        assert_eq!(res.body, b"line\r\n\r\nmore");
    }

    #[test]
    fn shell_quoting() {
        assert_eq!(shell_quote("/plain/path"), "'/plain/path'");
        assert_eq!(shell_quote("with'quote"), r"'with'\''quote'");
    }
}
