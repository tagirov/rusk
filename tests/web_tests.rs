// Integration tests for `rusk serve` and `rusk gen`: raw HTTP over
// TcpStream (no HTTP client dev-dependency), server spawned on port 0.

#![cfg(feature = "web")]

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::Duration;

mod common;

/// Kills the spawned server on drop. Keeps the stdout pipe open for the
/// child's lifetime — closing it early would make the server's own
/// startup `println!` calls fail.
struct ServerGuard {
    child: Child,
    port: u16,
    _stdout: BufReader<std::process::ChildStdout>,
}

impl Drop for ServerGuard {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn spawn_serve(sb: &common::Sandbox, rusk_config: &str, extra_args: &[&str]) -> ServerGuard {
    let mut child = sb
        .cmd()
        .env("RUSK_CONFIG", rusk_config)
        .args(["serve", "--port", "0"])
        .args(extra_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("failed to spawn rusk serve");

    let stdout = child.stdout.take().expect("stdout piped");
    let mut reader = BufReader::new(stdout);
    let mut port = None;
    // Consume the whole startup banner so nothing is left mid-pipe.
    for _ in 0..10 {
        let mut line = String::new();
        if reader.read_line(&mut line).unwrap_or(0) == 0 {
            break;
        }
        if let Some(addr) = line.split("http://").nth(1) {
            port = addr.trim().rsplit(':').next().and_then(|p| p.parse().ok());
        }
        if line.contains("Ctrl+C") {
            break;
        }
    }
    let port = port.expect("server did not print its listen address");
    ServerGuard {
        child,
        port,
        _stdout: reader,
    }
}

/// The body of a chunked response in `raw`, once its last chunk is there.
fn dechunk(raw: &[u8]) -> Option<Vec<u8>> {
    let mut body = Vec::new();
    let mut at = 0;
    loop {
        let line_end = at + raw[at..].windows(2).position(|w| w == b"\r\n")?;
        let size_text = String::from_utf8_lossy(&raw[at..line_end]);
        let size = usize::from_str_radix(size_text.split(';').next()?.trim(), 16).ok()?;
        let data = line_end + 2;
        if size == 0 {
            return Some(body);
        }
        if raw.len() < data + size + 2 {
            return None;
        }
        body.extend_from_slice(&raw[data..data + size]);
        at = data + size + 2;
    }
}

/// Raw HTTP/1.1 client over one keep-alive connection (like a browser).
/// Reads headers, then exactly Content-Length body bytes — or the chunks of
/// a chunked body (tiny_http sends a body over 32 KiB so), put together.
///
/// Deliberately NOT one-connection-per-request: tiny_http's task pool has a
/// lost-wakeup race under rapid connect/close churn that can park a fresh
/// connection for up to 5s; real clients hold a keep-alive connection.
struct Client {
    stream: TcpStream,
}

impl Client {
    fn connect(port: u16) -> Self {
        let stream = TcpStream::connect(("127.0.0.1", port)).expect("connect");
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        Self { stream }
    }

    fn request(&mut self, raw: &str) -> String {
        let first_line = raw.lines().next().unwrap_or("").to_string();
        self.stream.write_all(raw.as_bytes()).unwrap();

        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        let header_end = loop {
            let n = self
                .stream
                .read(&mut chunk)
                .unwrap_or_else(|e| panic!("read response headers for `{first_line}`: {e}"));
            assert!(n > 0, "server closed connection for `{first_line}`");
            buf.extend_from_slice(&chunk[..n]);
            if let Some(pos) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break pos + 4;
            }
        };
        let headers = String::from_utf8_lossy(&buf[..header_end]).to_string();
        let lower = headers.to_ascii_lowercase();
        if lower.lines().any(|l| l.starts_with("transfer-encoding:") && l.contains("chunked")) {
            let mut raw = buf.split_off(header_end);
            let body = loop {
                if let Some(body) = dechunk(&raw) {
                    break body;
                }
                let n = self.stream.read(&mut chunk).expect("read response body");
                assert!(n > 0, "server closed connection in a chunked body of `{first_line}`");
                raw.extend_from_slice(&chunk[..n]);
            };
            return format!("{headers}{}", String::from_utf8_lossy(&body));
        }
        let content_length: usize = lower
            .lines()
            .find_map(|l| l.strip_prefix("content-length:").map(|v| v.trim().parse().unwrap_or(0)))
            .unwrap_or(0);
        while buf.len() < header_end + content_length {
            let n = self.stream.read(&mut chunk).expect("read response body");
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
        }
        String::from_utf8_lossy(&buf).into_owned()
    }

    fn get(&mut self, path: &str, extra_headers: &str) -> String {
        self.request(&format!(
            "GET {path} HTTP/1.1\r\nHost: localhost\r\n{extra_headers}\r\n"
        ))
    }

    fn send(&mut self, method: &str, path: &str, content_type: &str, body: &str) -> String {
        self.request(&format!(
            "{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: {content_type}\r\n\
             Content-Length: {}\r\n\r\n{body}",
            body.len()
        ))
    }

    fn send_json(&mut self, method: &str, path: &str, body: &str) -> String {
        self.send(method, path, "application/json", body)
    }
}

/// The `ETag` response header as sent (quotes included).
fn etag_of(response: &str) -> String {
    response
        .lines()
        .take_while(|line| !line.is_empty())
        .find_map(|line| {
            let (name, value) = line.split_once(':')?;
            name.eq_ignore_ascii_case("etag").then(|| value.trim().to_string())
        })
        .unwrap_or_else(|| panic!("no ETag header in: {response}"))
}

fn status_of(response: &str) -> u16 {
    response
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0)
}

const ONE_TASK_DB: &str =
    r#"[{"id":1,"text":"Web test task","date":null,"done":false,"priority":false}]"#;

#[test]
fn test_serve_full_crud() {
    let sb = common::Sandbox::new();
    sb.write_db(ONE_TASK_DB);
    let server = spawn_serve(&sb, "", &[]);
    let mut client = Client::connect(server.port);

    let res = client.get("/api/tasks", "");
    assert_eq!(status_of(&res), 200);
    assert!(res.contains("Web test task"));

    let res = client.send_json(
        "POST",
        "/api/tasks",
        r#"{"text":"created via API","date":"2026-08-01"}"#,
    );
    assert_eq!(status_of(&res), 201, "{res}");
    assert!(res.contains("created via API"));

    let res = client.send_json("PATCH", "/api/tasks/2", r#"{"done":true}"#);
    assert_eq!(status_of(&res), 200, "{res}");
    assert!(res.contains("\"done\":true"));

    // Whole-list replace (used by `rusk sync push`).
    let res = client.send_json(
        "PUT",
        "/api/tasks",
        r#"[{"id":7,"text":"replaced","date":null,"done":false,"priority":true}]"#,
    );
    assert_eq!(status_of(&res), 200, "{res}");
    let res = client.get("/api/tasks", "");
    assert!(res.contains("replaced") && !res.contains("Web test task"));

    let res = client.request("DELETE /api/tasks/7 HTTP/1.1\r\nHost: localhost\r\n\r\n");
    assert_eq!(status_of(&res), 204, "{res}");

    // CLI writes are visible on the next request (server re-reads the file).
    sb.write_db(ONE_TASK_DB);
    let res = client.get("/api/tasks", "");
    assert!(res.contains("Web test task"));

    // The UI page is served in live mode.
    let res = client.get("/", "");
    assert_eq!(status_of(&res), 200);
    assert!(res.contains(r#"mode: "live""#));

    // CSRF guard: mutations without a JSON content type are rejected.
    let res = client.send("POST", "/api/tasks", "text/plain", r#"{"text":"x"}"#);
    assert_eq!(status_of(&res), 415, "{res}");
}

/// REVIEW №22 (R1): an error body names the cause, not only the step that
/// failed - also where the database cannot even be opened for a request
/// (`server.rs`, before any API handler runs). A directory in place of the
/// database file fails for every user, root included. (A debug binary is
/// held to its test database; a release one takes a directory in `RUSK_DB`
/// for `<dir>/tasks.json`.)
#[test]
#[cfg(debug_assertions)]
fn test_serve_error_bodies_name_the_cause() {
    let sb = common::Sandbox::new();
    std::fs::create_dir(sb.db_path()).unwrap();
    let server = spawn_serve(&sb, "", &[]);
    let mut client = Client::connect(server.port);

    for res in [
        client.get("/api/tasks", ""),
        client.send_json("POST", "/api/tasks", r#"{"text":"x"}"#),
        client.send_json("PATCH", "/api/tasks/1", r#"{"done":true}"#),
    ] {
        assert_eq!(status_of(&res), 500, "{res}");
        assert!(res.contains("Failed to read the database file"), "{res}");
        assert!(res.contains("os error"), "the cause is missing: {res}");
    }
}

/// REVIEW №162 / №156: the list and every task carry their revision as
/// `ETag`; a change sent with `If-Match` is refused (412) when what it is
/// aimed at is no longer what the client has seen, e.g. because a CLI
/// command ran in between.
#[test]
fn test_serve_if_match_refuses_changes_to_what_the_client_has_not_seen() {
    let sb = common::Sandbox::new();
    sb.write_db(
        r#"[{"id":1,"text":"alpha","date":null,"done":false,"priority":false},
            {"id":2,"text":"beta","date":null,"done":false,"priority":false},
            {"id":3,"text":"gamma","date":null,"done":false,"priority":false}]"#,
    );
    let server = spawn_serve(&sb, "", &[]);
    let mut client = Client::connect(server.port);
    let conditional = |client: &mut Client, method: &str, path: &str, etag: &str, body: &str| {
        client.request(&format!(
            "{method} {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\n\
             If-Match: {etag}\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        ))
    };

    let list_seen = etag_of(&client.get("/api/tasks", ""));
    assert!(
        list_seen.starts_with('"') && list_seen.ends_with('"'),
        "an entity tag is quoted: {list_seen}"
    );
    let gamma = client.get("/api/tasks/3", "");
    assert_eq!(status_of(&gamma), 200, "{gamma}");
    assert!(gamma.contains(r#""text":"gamma""#), "{gamma}");
    let gamma_seen = etag_of(&gamma);
    let beta_seen = etag_of(&client.get("/api/tasks/2", ""));
    assert_eq!(status_of(&client.get("/api/tasks/9", "")), 404);

    // Meanwhile, in a terminal: task 3 is deleted and its id reused.
    let cli = |args: &[&str]| {
        let out = sb.cmd().args(args).output().unwrap();
        assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    };
    let del = client.request("DELETE /api/tasks/3 HTTP/1.1\r\nHost: localhost\r\n\r\n");
    assert_eq!(status_of(&del), 204, "{del}");
    cli(&["add", "BRAND NEW important task"]);

    // The stale page deletes "gamma" / saves its form / ticks the box —
    // whether it names the task it saw or the list it saw.
    let form = r#"{"text":"gamma","date":null,"priority":true}"#;
    for seen in [&gamma_seen, &list_seen] {
        let res = conditional(&mut client, "DELETE", "/api/tasks/3", seen, "");
        assert_eq!(status_of(&res), 412, "{res}");
        let res = conditional(&mut client, "PATCH", "/api/tasks/3", seen, form);
        assert_eq!(status_of(&res), 412, "{res}");
    }
    let res = conditional(&mut client, "PUT", "/api/tasks", &list_seen, "[]");
    assert_eq!(status_of(&res), 412, "{res}");
    let db = sb.read_db();
    assert!(db.contains("BRAND NEW important task") && !db.contains("\"priority\": true"), "{db}");

    // Task 2 is still what was seen: changes elsewhere do not refuse it.
    // The answer names the new revision of the task, so a client can chain.
    let res = conditional(&mut client, "PATCH", "/api/tasks/2", &beta_seen, r#"{"done":true}"#);
    assert_eq!(status_of(&res), 200, "{res}");
    let beta_now = etag_of(&res);
    assert_ne!(beta_now, beta_seen);
    assert_eq!(beta_now, etag_of(&client.get("/api/tasks/2", "")));
    let res = conditional(&mut client, "PATCH", "/api/tasks/2", &beta_seen, r#"{"done":false}"#);
    assert_eq!(status_of(&res), 412, "{res}");
    let res = conditional(&mut client, "DELETE", "/api/tasks/2", &beta_now, "");
    assert_eq!(status_of(&res), 204, "{res}");

    // The list revision works for the list: compare-and-swap of a PUT.
    let list_now = etag_of(&client.get("/api/tasks", ""));
    assert_ne!(list_now, list_seen);
    let res = conditional(&mut client, "PUT", "/api/tasks", &list_now, "[]");
    assert_eq!(status_of(&res), 200, "{res}");
    assert_eq!(etag_of(&res), etag_of(&client.get("/api/tasks", "")));

    // No If-Match: applies to whatever is there, as before.
    let res = client.send_json("POST", "/api/tasks", r#"{"text":"unconditional"}"#);
    assert_eq!(status_of(&res), 201, "{res}");
    let res = client.send_json("PATCH", "/api/tasks/1", r#"{"done":true}"#);
    assert_eq!(status_of(&res), 200, "{res}");
}

/// REVIEW №156: the http database backend sends the revision it loaded back
/// with its save, so it cannot overwrite what the web UI or another machine
/// stored in between; `update` — what the commands use — re-reads and
/// applies its change to the current list.
#[test]
#[cfg(feature = "backend-http")]
fn test_http_backend_does_not_overwrite_concurrent_changes() {
    if Command::new("curl").arg("--version").output().is_err() {
        eprintln!("skipping test_http_backend_does_not_overwrite_concurrent_changes: curl not found");
        return;
    }
    let sb = common::Sandbox::new();
    sb.write_db(
        r#"[{"id":1,"text":"server task","date":null,"done":false,"priority":false},
            {"id":2,"text":"second","date":null,"done":false,"priority":false}]"#,
    );
    let server = spawn_serve(&sb, "", &[]);
    let db = rusk::Backend::parse(&format!("http://127.0.0.1:{}", server.port)).unwrap();
    let mut client = Client::connect(server.port);
    let add_from_web_ui = |client: &mut Client, text: &str| {
        let res = client.send_json("POST", "/api/tasks", &format!(r#"{{"text":"{text}"}}"#));
        assert_eq!(status_of(&res), 201, "{res}");
    };

    // `rusk del 1` waits at its prompt while the web UI adds a task.
    let snapshot = db.load().unwrap();
    add_from_web_ui(&mut client, "added during the prompt");

    // Writing the snapshot back is refused...
    let without_first: Vec<rusk::Task> = snapshot.iter().skip(1).cloned().collect();
    let err = db.save(&without_first).unwrap_err();
    assert!(err.is::<rusk::StaleDatabase>(), "{err}");
    assert!(sb.read_db().contains("added during the prompt"));

    // ...and the update deletes task 1 from the list as it is now.
    let tasks = db
        .update(&snapshot, true, &mut |tasks| {
            tasks.retain(|t| t.id != 1);
            Ok(())
        })
        .unwrap()
        .tasks;
    let texts: Vec<&str> = tasks.iter().map(|t| t.text.as_str()).collect();
    assert_eq!(texts, ["second", "added during the prompt"]);
    let on_server = sb.read_db();
    assert!(on_server.contains("added during the prompt") && !on_server.contains("server task"));

    // The backend is current after its own write: the next save goes through.
    db.save(&tasks).unwrap();
    // A failure the server reports comes with its message, not just a code.
    let mut bad = tasks.clone();
    bad.push(bad[0].clone());
    let err = db.save(&bad).unwrap_err().to_string();
    assert!(err.contains("HTTP 400") && err.contains("unique"), "{err}");
}

#[test]
fn test_serve_token_auth() {
    let sb = common::Sandbox::new();
    sb.write_db(ONE_TASK_DB);

    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("cfg");
    fs::write(&cfg_path, "web_token = s3cret\n").unwrap();
    let server = spawn_serve(&sb, cfg_path.to_str().unwrap(), &[]);
    let mut client = Client::connect(server.port);

    let res = client.get("/api/tasks", "");
    assert_eq!(status_of(&res), 401);

    let res = client.get("/", "");
    assert_eq!(status_of(&res), 200);
    assert!(res.contains("Access token"), "login page expected: {res}");

    let res = client.get("/api/tasks", "Authorization: Bearer s3cret\r\n");
    assert_eq!(status_of(&res), 200);
    assert!(res.contains("Web test task"));

    let res = client.get("/api/tasks", "Authorization: Bearer wrong\r\n");
    assert_eq!(status_of(&res), 401);

    // Query token sets the cookie and redirects to a clean URL.
    let res = client.get("/?token=s3cret", "");
    assert_eq!(status_of(&res), 303);
    assert!(res.contains("Set-Cookie: rusk_token=s3cret"));

    let res = client.get("/api/tasks", "Cookie: rusk_token=s3cret\r\n");
    assert_eq!(status_of(&res), 200);
}

#[test]
fn test_serve_refuses_non_loopback_without_token() {
    let sb = common::Sandbox::new();
    let out = sb
        .cmd()
        .args(["serve", "--host", "0.0.0.0", "--port", "0"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("refusing to serve"), "got: {stderr}");
}

#[test]
fn test_gen_static_page() {
    let sb = common::Sandbox::new();
    sb.write_db(
        r#"[{"id":1,"text":"Static page task </script><script>alert(1)</script>","date":"2026-07-10","done":false,"priority":true}]"#,
    );

    let dir = tempfile::tempdir().unwrap();
    let out_path = dir.path().join("index.html");
    let out = sb
        .cmd()
        .args(["gen", "-o", out_path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));

    let html = fs::read_to_string(&out_path).unwrap();
    assert!(html.contains(r#"mode: "static""#));
    assert!(html.contains("Static page task"));
    for marker in ["__MODE__", "__DATA__", "__THEME__", "__GENERATED__"] {
        assert!(!html.contains(marker), "marker {marker} left in output");
    }
    assert!(
        !html.contains("</script><script>alert"),
        "task text must not break out of the script block"
    );
    assert!(html.contains("\\u003c/script>"));
}

/// Full HTTP sync flow against a live `rusk serve`. The local side gets its
/// own sandbox, the server another one.
#[test]
#[cfg(feature = "sync")]
fn test_sync_http_roundtrip() {
    let sb = common::Sandbox::new();
    if Command::new("curl").arg("--version").output().is_err() {
        eprintln!("skipping test_sync_http_roundtrip: curl not found");
        return;
    }
    sb.write_db(ONE_TASK_DB);
    let server = spawn_serve(&sb, "", &[]);
    let remote = format!("http://127.0.0.1:{}", server.port);

    let local = common::Sandbox::with_db(
        r#"[{"id":1,"text":"Local only task","date":null,"done":false,"priority":false}]"#,
    );
    let local_db = local.db_path();
    let sync = |args: &[&str]| {
        local
            .cmd()
            .env("RUSK_SYNC_REMOTE", &remote)
            .args(args)
            .output()
            .unwrap()
    };

    // First contact, both sides differ and are non-empty: refuse to guess.
    let out = sync(&["sync"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--force"), "got: {stderr}");

    // Take the remote state.
    let out = sync(&["sync", "pull", "--force"]);
    assert!(
        out.status.success(),
        "pull failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let local = fs::read_to_string(&local_db).unwrap();
    assert!(local.contains("Web test task") && !local.contains("Local only task"));
    assert!(local_db.with_extension("json.sync").exists());

    // A local edit fast-forwards to the remote automatically.
    let out = sync(&["add", "synced from CLI"]);
    assert!(out.status.success());
    let out = sync(&["sync"]);
    assert!(
        out.status.success(),
        "auto push failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(String::from_utf8_lossy(&out.stdout).contains("Pushed"));
    let mut client = Client::connect(server.port);
    let res = client.get("/api/tasks", "");
    assert!(res.contains("synced from CLI"), "got: {res}");

    // Nothing left to do.
    let out = sync(&["sync"]);
    assert!(out.status.success());
    assert!(String::from_utf8_lossy(&out.stdout).contains("Already in sync"));
}

#[test]
fn test_gen_to_stdout() {
    let sb = common::Sandbox::new();
    sb.write_db(ONE_TASK_DB);
    let out = sb
        .cmd()
        .args(["gen", "-o", "-"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.starts_with("<!DOCTYPE html>"));
    assert!(stdout.contains("Web test task"));
}
