//! `rusk serve`: a single-threaded tiny_http loop.
//!
//! The server holds no task state: every request re-reads the database file,
//! so CLI and web edits always see each other (the atomic writes in
//! `TaskManager::save` guarantee a reader never observes partial data), and
//! every change goes through `TaskManager::update`, so a request and a CLI
//! command saving at the same moment both take effect. Clients that send
//! back the `ETag` of the list or of a task as `If-Match` are told (412)
//! when what they are changing is no longer what they have seen.
//! Auth is a token from the config, entered once in a login form and kept in
//! an HttpOnly cookie; `Authorization: Bearer` works for scripting.

use super::api::{self, ApiResponse};
use crate::{TaskId, TaskManager};
use anyhow::{Context, Result, anyhow, bail};
use std::io::Read;
use tiny_http::{Header, Method, Request, Response, Server};

pub struct ServeOptions {
    pub host: String,
    pub port: u16,
    pub token: Option<String>,
}

const MAX_BODY_BYTES: u64 = 1024 * 1024;

const LOGIN_PAGE: &str = r#"<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>rusk — sign in</title>
<style>
body{margin:0;display:grid;place-items:center;min-height:100vh;background:#111114;color:#e8e8ea;
font:16px/1.4 system-ui,sans-serif}
form{display:flex;flex-direction:column;gap:12px;width:min(90vw,320px)}
h1{font-size:18px;margin:0;text-align:center}
input{background:#1c1c21;color:inherit;border:1px solid #2a2a31;border-radius:10px;padding:12px;font:inherit}
button{background:#ffa500;border:0;border-radius:10px;padding:12px;font:inherit;font-weight:600;cursor:pointer}
@media (prefers-color-scheme:light){body{background:#f5f5f7;color:#1b1b1f}input{background:#fff;border-color:#e2e2e8}}
</style></head><body>
<form method="post" action="/auth">
<h1>rusk</h1>
<input type="password" name="token" placeholder="Access token" autofocus required>
<button type="submit">Sign in</button>
</form></body></html>
"#;

struct Reply {
    status: u16,
    content_type: &'static str,
    body: String,
    headers: Vec<(String, String)>,
}

impl Reply {
    fn html(status: u16, body: impl Into<String>) -> Self {
        Self {
            status,
            content_type: "text/html; charset=utf-8",
            body: body.into(),
            headers: Vec::new(),
        }
    }

    fn from_api(res: ApiResponse) -> Self {
        Self {
            status: res.status,
            content_type: "application/json; charset=utf-8",
            body: res.body,
            // Quoted, as an entity tag has to be.
            headers: res
                .etag
                .map(|etag| ("ETag".to_string(), format!("\"{etag}\"")))
                .into_iter()
                .collect(),
        }
    }

    fn json_error(status: u16, message: &str) -> Self {
        Self::from_api(ApiResponse::error(status, message))
    }

    fn redirect_home() -> Self {
        let mut reply = Self::html(303, "");
        reply
            .headers
            .push(("Location".to_string(), "/".to_string()));
        reply
    }

    fn with_session_cookie(mut self, token: &str) -> Self {
        self.headers.push((
            "Set-Cookie".to_string(),
            format!("rusk_token={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age=31536000"),
        ));
        self
    }
}

fn is_loopback(host: &str) -> bool {
    matches!(host, "127.0.0.1" | "localhost" | "::1")
}

/// Constant-time comparison; overkill for the threat model but free.
fn token_eq(a: &str, b: &str) -> bool {
    a.len() == b.len()
        && a.bytes()
            .zip(b.bytes())
            .fold(0u8, |acc, (x, y)| acc | (x ^ y))
            == 0
}

fn header_value<'a>(request: &'a Request, name: &'static str) -> Option<&'a str> {
    request
        .headers()
        .iter()
        .find(|h| h.field.equiv(name))
        .map(|h| h.value.as_str())
}

fn cookie_token(request: &Request) -> Option<String> {
    let cookies = header_value(request, "Cookie")?;
    cookies.split(';').find_map(|pair| {
        let (key, value) = pair.trim().split_once('=')?;
        (key == "rusk_token").then(|| value.to_string())
    })
}

fn bearer_token(request: &Request) -> Option<String> {
    header_value(request, "Authorization")?
        .strip_prefix("Bearer ")
        .map(|t| t.trim().to_string())
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok();
                match hex.and_then(|h| u8::from_str_radix(h, 16).ok()) {
                    Some(byte) => {
                        out.push(byte);
                        i += 2;
                    }
                    None => out.push(b'%'),
                }
            }
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Value of `name` in a query string or an urlencoded form body.
fn form_value(data: &str, name: &str) -> Option<String> {
    data.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then(|| percent_decode(value))
    })
}

fn read_body(request: &mut Request) -> Result<String> {
    let mut body = String::new();
    request
        .as_reader()
        .take(MAX_BODY_BYTES)
        .read_to_string(&mut body)
        .context("failed to read request body")?;
    Ok(body)
}

/// Fresh TaskManager per request: the database is tiny (≤255 tasks) and this
/// guarantees CLI writes are always visible.
fn open_tm() -> Result<TaskManager> {
    TaskManager::open()
}

fn task_id_from_path(path: &str) -> Option<TaskId> {
    path.strip_prefix("/api/tasks/")?.parse().ok()
}

fn authorized(request: &Request, token: &str) -> bool {
    cookie_token(request)
        .or_else(|| bearer_token(request))
        .is_some_and(|t| token_eq(&t, token))
}

fn route(request: &mut Request, opts: &ServeOptions) -> Reply {
    let url = request.url().to_string();
    let (path, query) = url.split_once('?').unwrap_or((url.as_str(), ""));
    let method = request.method().clone();

    if let Some(token) = &opts.token {
        // Bookmarkable `/?token=...`: on match, move the token into a cookie
        // and redirect so it leaves the address bar.
        if method == Method::Get
            && path == "/"
            && let Some(t) = form_value(query, "token")
        {
            return if token_eq(&t, token) {
                Reply::redirect_home().with_session_cookie(token)
            } else {
                Reply::json_error(401, "invalid token")
            };
        }
        if method == Method::Post && path == "/auth" {
            let body = match read_body(request) {
                Ok(b) => b,
                Err(e) => return Reply::json_error(400, &format!("{e:#}")),
            };
            return match form_value(&body, "token") {
                Some(t) if token_eq(&t, token) => {
                    Reply::redirect_home().with_session_cookie(token)
                }
                _ => Reply::html(401, LOGIN_PAGE),
            };
        }
        if !authorized(request, token) {
            return if path.starts_with("/api/") {
                Reply::json_error(401, "unauthorized")
            } else {
                Reply::html(200, LOGIN_PAGE)
            };
        }
    }

    // CSRF guard for state changes: cross-origin forms cannot send
    // application/json without a CORS preflight, and we emit no CORS headers.
    let mutating = matches!(method, Method::Post | Method::Put | Method::Patch);
    if mutating
        && path.starts_with("/api/")
        && !header_value(request, "Content-Type")
            .unwrap_or("")
            .starts_with("application/json")
    {
        return Reply::json_error(415, "Content-Type must be application/json");
    }

    let if_match = header_value(request, "If-Match").map(str::to_string);

    match (method, path) {
        (Method::Get, "/") => match open_tm().and_then(|tm| super::render_live_page(tm.tasks())) {
            Ok(html) => Reply::html(200, html),
            Err(e) => Reply::json_error(500, &format!("{e:#}")),
        },
        (Method::Get, "/api/tasks") => match open_tm() {
            Ok(tm) => Reply::from_api(api::list_tasks(&tm)),
            Err(e) => Reply::json_error(500, &format!("{e:#}")),
        },
        (Method::Post, "/api/tasks") => with_body_and_tm(request, |tm, body| {
            api::create_task(tm, body, if_match.as_deref())
        }),
        (Method::Put, "/api/tasks") => with_body_and_tm(request, |tm, body| {
            api::replace_tasks(tm, body, if_match.as_deref())
        }),
        (Method::Delete, "/api/tasks/done") => match open_tm() {
            Ok(mut tm) => Reply::from_api(api::delete_done(&mut tm, if_match.as_deref())),
            Err(e) => Reply::json_error(500, &format!("{e:#}")),
        },
        (Method::Get, p) if task_id_from_path(p).is_some() => {
            let id = task_id_from_path(p).unwrap();
            match open_tm() {
                Ok(tm) => Reply::from_api(api::get_task(&tm, id)),
                Err(e) => Reply::json_error(500, &format!("{e:#}")),
            }
        }
        (Method::Patch, p) if task_id_from_path(p).is_some() => {
            let id = task_id_from_path(p).unwrap();
            with_body_and_tm(request, |tm, body| {
                api::update_task(tm, id, body, if_match.as_deref())
            })
        }
        (Method::Delete, p) if task_id_from_path(p).is_some() => {
            let id = task_id_from_path(p).unwrap();
            match open_tm() {
                Ok(mut tm) => Reply::from_api(api::delete_task(&mut tm, id, if_match.as_deref())),
                Err(e) => Reply::json_error(500, &format!("{e:#}")),
            }
        }
        (_, p) if p == "/api/tasks" || p == "/api/tasks/done" || task_id_from_path(p).is_some() => {
            Reply::json_error(405, "method not allowed")
        }
        (_, p) if p.starts_with("/api/") => Reply::json_error(404, "not found"),
        _ => Reply::html(404, "<!DOCTYPE html><title>404</title><p>Not found — <a href=\"/\">rusk</a>"),
    }
}

fn with_body_and_tm(
    request: &mut Request,
    handler: impl FnOnce(&mut TaskManager, &str) -> ApiResponse,
) -> Reply {
    let body = match read_body(request) {
        Ok(b) => b,
        Err(e) => return Reply::json_error(400, &format!("{e:#}")),
    };
    match open_tm() {
        Ok(mut tm) => Reply::from_api(handler(&mut tm, &body)),
        Err(e) => Reply::json_error(500, &format!("{e:#}")),
    }
}

fn respond(request: Request, reply: Reply) {
    let mut response = Response::from_string(reply.body)
        .with_status_code(reply.status)
        .with_header(
            Header::from_bytes("Content-Type", reply.content_type)
                .expect("static header is valid"),
        )
        .with_header(
            Header::from_bytes("Cache-Control", "no-store").expect("static header is valid"),
        )
        .with_header(
            Header::from_bytes("X-Content-Type-Options", "nosniff")
                .expect("static header is valid"),
        );
    for (name, value) in reply.headers {
        if let Ok(header) = Header::from_bytes(name.as_bytes(), value.as_bytes()) {
            response = response.with_header(header);
        }
    }
    let _ = request.respond(response);
}

pub fn run(opts: ServeOptions) -> Result<()> {
    if opts.token.is_none() && !is_loopback(&opts.host) {
        bail!(
            "refusing to serve on {} without authentication: set `web_token` in the config \
             (recommended behind a TLS reverse proxy), or bind 127.0.0.1",
            opts.host
        );
    }

    let server = Server::http(format!("{}:{}", opts.host, opts.port))
        .map_err(|e| anyhow!("failed to bind {}:{}: {e}", opts.host, opts.port))?;

    // The actual address matters with --port 0 (tests parse this line).
    let addr = server
        .server_addr()
        .to_ip()
        .map(|a| a.to_string())
        .unwrap_or_else(|| format!("{}:{}", opts.host, opts.port));
    println!("rusk serve listening on http://{addr}");
    println!(
        "Database: {}",
        crate::backend::Backend::resolve()
            .map(|b| b.describe())
            .unwrap_or_else(|e| format!("unavailable ({e:#})"))
    );
    println!(
        "Auth: {}",
        if opts.token.is_some() {
            "token required"
        } else {
            "off (loopback only)"
        }
    );
    println!("Press Ctrl+C to stop.");

    for mut request in server.incoming_requests() {
        let reply = route(&mut request, &opts);
        respond(request, reply);
    }
    Ok(())
}
