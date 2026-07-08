// Integration tests for `rusk serve` and `rusk gen`: raw HTTP over
// TcpStream (no HTTP client dev-dependency), server spawned on port 0.

#![cfg(feature = "web")]

use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::Duration;

mod common;

// The spawned debug binary always resolves the database to the shared
// rusk_debug path, so serialize tests that touch it.
static DB_MUTEX: Mutex<()> = Mutex::new(());

fn debug_db_path() -> PathBuf {
    std::env::temp_dir().join("rusk_debug").join("tasks.json")
}

fn setup_test_db(tasks_json: &str) {
    let db_path = debug_db_path();
    if let Some(parent) = db_path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&db_path, tasks_json).unwrap();
}

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

fn spawn_serve(rusk_config: &str, extra_args: &[&str]) -> ServerGuard {
    let bin = common::require_rusk_bin().expect("rusk binary not found");
    let mut child = Command::new(bin)
        .env("RUSK_DB", debug_db_path())
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

/// Raw HTTP/1.1 client over one keep-alive connection (like a browser).
/// Reads headers, then exactly Content-Length body bytes.
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
        let content_length: usize = headers
            .lines()
            .find_map(|l| {
                l.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .map(|v| v.trim().parse().unwrap_or(0))
            })
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
    let _guard = DB_MUTEX.lock().unwrap();
    setup_test_db(ONE_TASK_DB);
    let server = spawn_serve("", &[]);
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
    setup_test_db(ONE_TASK_DB);
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

#[test]
fn test_serve_token_auth() {
    let _guard = DB_MUTEX.lock().unwrap();
    setup_test_db(ONE_TASK_DB);

    let dir = tempfile::tempdir().unwrap();
    let cfg_path = dir.path().join("cfg");
    fs::write(&cfg_path, "web_token = s3cret\n").unwrap();
    let server = spawn_serve(cfg_path.to_str().unwrap(), &[]);
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
    let bin = common::require_rusk_bin().expect("rusk binary not found");
    let out = Command::new(bin)
        .env("RUSK_DB", debug_db_path())
        .env("RUSK_CONFIG", "")
        .args(["serve", "--host", "0.0.0.0", "--port", "0"])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("refusing to serve"), "got: {stderr}");
}

#[test]
fn test_gen_static_page() {
    let _guard = DB_MUTEX.lock().unwrap();
    setup_test_db(
        r#"[{"id":1,"text":"Static page task </script><script>alert(1)</script>","date":"2026-07-10","done":false,"priority":true}]"#,
    );

    let dir = tempfile::tempdir().unwrap();
    let out_path = dir.path().join("index.html");
    let bin = common::require_rusk_bin().expect("rusk binary not found");
    let out = Command::new(bin)
        .env("RUSK_DB", debug_db_path())
        .env("RUSK_CONFIG", "")
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

#[test]
fn test_gen_to_stdout() {
    let _guard = DB_MUTEX.lock().unwrap();
    setup_test_db(ONE_TASK_DB);
    let bin = common::require_rusk_bin().expect("rusk binary not found");
    let out = Command::new(bin)
        .env("RUSK_DB", debug_db_path())
        .env("RUSK_CONFIG", "")
        .args(["gen", "-o", "-"])
        .output()
        .unwrap();
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.starts_with("<!DOCTYPE html>"));
    assert!(stdout.contains("Web test task"));
}
