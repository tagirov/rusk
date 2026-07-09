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
            anyhow::anyhow!("`{tool}` not found in PATH (required for this database location)")
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

/// Reads a remote file over ssh; a missing file yields empty output.
pub fn ssh_read_file(target: &str, path: &str) -> Result<Vec<u8>> {
    let quoted = shell_quote(path);
    let mut cmd = Command::new("ssh");
    cmd.arg(target)
        .arg(format!("test -f {quoted} && cat {quoted} || true"));
    run_tool(cmd, None, "ssh")
}

/// Writes a remote file over ssh atomically: stream to a temp sibling, then
/// `mv` into place. Parent directories are created as needed.
pub fn ssh_write_file(target: &str, path: &str, data: &[u8]) -> Result<()> {
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
    run_tool(cmd, Some(data), "ssh")?;
    Ok(())
}

/// One curl call. `method` of `None` is a plain GET; a JSON `body` is sent
/// with the right Content-Type; `token` becomes a Bearer header.
pub fn http_request(
    url: &str,
    method: Option<&str>,
    token: Option<&str>,
    json_body: Option<&[u8]>,
) -> Result<Vec<u8>> {
    let mut cmd = Command::new("curl");
    cmd.args(["-fsS", "--max-time", "30"]);
    if let Some(method) = method {
        cmd.args(["-X", method]);
    }
    if json_body.is_some() {
        cmd.args(["-H", "Content-Type: application/json", "--data-binary", "@-"]);
    }
    if let Some(token) = token {
        cmd.args(["-H", &format!("Authorization: Bearer {token}")]);
    }
    cmd.arg(url);
    run_tool(cmd, json_body, "curl")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quoting() {
        assert_eq!(shell_quote("/plain/path"), "'/plain/path'");
        assert_eq!(shell_quote("with'quote"), r"'with'\''quote'");
    }
}
