//! `rusk serve`: a tiny_http server, one thread per request.
//!
//! The server holds no task state: every request re-reads the database file,
//! so CLI and web edits always see each other (the atomic writes in
//! `TaskManager::save` guarantee a reader never observes partial data), and
//! every change goes through `TaskManager::update`, so a request and a CLI
//! command saving at the same moment both take effect. Requests are read,
//! checked and answered on threads of their own, so a client that sends its
//! body slowly holds up nobody else; the work on the tasks itself is done
//! by one request at a time ([`with_tasks`]). Clients that send back the
//! `ETag` of the list or of a task as `If-Match` are told (412) when what
//! they are changing is no longer what they have seen. Auth is a token from
//! the config, entered once in a login form and kept in an HttpOnly cookie;
//! `Authorization: Bearer` works for scripting. Without a token the server
//! binds loopback only and answers only requests addressed to a loopback
//! name.

use super::api::{self, ApiResponse};
use crate::{TaskId, TaskManager};
use anyhow::{Result, anyhow, bail};
use std::io::Read;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use tiny_http::{Header, Method, Request, Response, Server};

pub struct ServeOptions {
    pub host: String,
    pub port: u16,
    pub token: Option<String>,
}

/// The largest request body taken. A task list of thousands of tasks is a
/// few MB; a bigger body is refused with 413 rather than cut short and
/// misread as broken JSON (REVIEW №54).
const MAX_BODY_BYTES: u64 = 32 * 1024 * 1024;

/// The largest sign-in form taken: it is read before anybody has signed
/// in, and holds a token, nothing more (review of R20: 32 MiB bodies from
/// anybody, 32 at a time, were a gigabyte of memory).
const AUTH_BODY_BYTES: u64 = 4 * 1024;

/// Held by the request that works on the tasks. Requests run on threads of
/// their own (REVIEW №51), and a backend without a lock of its own — a file
/// over ssh — would lose one of two changes made at the same moment
/// (review of R20); a local file or SQLite would wait on theirs anyway.
static DATABASE: Mutex<()> = Mutex::new(());

/// The body tiny_http reads along with the headers; a longer one comes
/// from the connection as the request reads it.
const PREFETCHED_BODY_BYTES: usize = 1024;

/// An unread body up to this size is read and thrown away by its request,
/// through a small buffer, before the answer; a client that stalls on it
/// holds up only its own thread.
const DISCARDED_BODY_BYTES: usize = 1024 * 1024;

/// Held by the request whose bigger unread body is being thrown away. A
/// request answered without reading its body (401, 413, 404…) takes the
/// rest of it from the connection when it goes, into a buffer as large as
/// that rest (tiny_http's `EqualReader`). One at a time, as when all
/// requests were answered one by one: clients that announce big bodies and
/// send them slowly cannot pile those buffers up (review of R20).
static DRAIN: Mutex<()> = Mutex::new(());

thread_local! {
    /// Whether the request of this thread had its body read whole.
    static BODY_READ: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

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

    /// The session cookie holds the token (see [`token_is_valid`] for why it
    /// needs no encoding). `Secure` when the request came over TLS — to a
    /// reverse proxy that says so in `X-Forwarded-Proto` — so the browser
    /// never sends it over plain HTTP (REVIEW №152).
    fn with_session_cookie(mut self, token: &str, secure: bool) -> Self {
        let secure = if secure { "; Secure" } else { "" };
        self.headers.push((
            "Set-Cookie".to_string(),
            format!("rusk_token={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age=31536000{secure}"),
        ));
        self
    }

    /// `/logout`: the cookie is dropped; the page that asked reloads into
    /// the login page.
    fn logged_out() -> Self {
        let mut reply = Self::html(204, "");
        reply.headers.push((
            "Set-Cookie".to_string(),
            "rusk_token=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0".to_string(),
        ));
        reply
    }
}

/// What a `web_host` value names for binding: an IPv6 literal without its
/// brackets, and `localhost` (in any case) as 127.0.0.1 — one name that
/// resolves to two addresses would otherwise be bound on whichever comes
/// first (`::1`), and 127.0.0.1 is what clients fall back to (REVIEW №112).
fn bind_host(host: &str) -> &str {
    let bare = host
        .strip_prefix('[')
        .and_then(|h| h.strip_suffix(']'))
        .unwrap_or(host);
    // `localhost.`, the name with its root, is the same name.
    let name = bare.strip_suffix('.').unwrap_or(bare);
    if name.eq_ignore_ascii_case("localhost") {
        "127.0.0.1"
    } else {
        bare
    }
}

/// Whether `host` is a loopback address: all of 127.0.0.0/8, `::1` with or
/// without brackets (and 127.0.0.0/8 written as IPv6, `::ffff:127.0.0.1`),
/// `localhost` in any case — not three spellings of it.
fn is_loopback(host: &str) -> bool {
    bind_host(host)
        .parse::<IpAddr>()
        .is_ok_and(|ip| ip.to_canonical().is_loopback())
}

/// Whether a `Host` header (`name[:port]`, `[v6][:port]`) names a loopback
/// address or `localhost`. Anything after the name but a port is no Host.
fn host_header_is_loopback(value: &str) -> bool {
    let is_port = |port: &str| !port.is_empty() && port.bytes().all(|b| b.is_ascii_digit());
    let host = match value.strip_prefix('[') {
        Some(rest) => match rest.split_once(']') {
            Some((v6, "")) => v6,
            Some((v6, tail)) if tail.strip_prefix(':').is_some_and(is_port) => v6,
            _ => return false,
        },
        None => match value.rsplit_once(':') {
            Some((name, port)) if is_port(port) => name,
            Some(_) => return false,
            None => value,
        },
    };
    is_loopback(host)
}

/// Whether `token` can travel as it is in the session cookie and in an
/// `Authorization` header: printable ASCII — spaces inside are fine —
/// without a `;`, which ends a cookie, and without whitespace at its ends,
/// which does not come back. Any other was sent cut short or dropped, and
/// could never sign in (REVIEW №52).
pub fn token_is_valid(token: &str) -> bool {
    !token.is_empty()
        && token.trim() == token
        && token.chars().all(|c| (c.is_ascii_graphic() || c == ' ') && c != ';')
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

/// Every token the request presents: each `rusk_token` cookie and the
/// Bearer token.
fn presented_tokens(request: &Request) -> Vec<&str> {
    let mut tokens: Vec<&str> = header_value(request, "Cookie")
        .map(|cookies| {
            cookies
                .split(';')
                .filter_map(|pair| {
                    let (key, value) = pair.trim().split_once('=')?;
                    (key == "rusk_token").then_some(value)
                })
                .collect()
        })
        .unwrap_or_default();
    // The scheme is a name in any case (RFC 9110 §11.1).
    if let Some((scheme, bearer)) = header_value(request, "Authorization").and_then(|v| v.trim().split_once(' '))
        && scheme.eq_ignore_ascii_case("Bearer")
    {
        tokens.push(bearer.trim());
    }
    tokens
}

/// `%XX` decoded; `+` is a space in a form body, where the browser encodes
/// a `+` itself as `%2B`, but a `+` in a bookmarked address was typed by a
/// human and means `+` (REVIEW №109).
fn percent_decode(s: &str, plus_is_space: bool) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' if plus_is_space => out.push(b' '),
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

/// Value of `name` in an urlencoded form body (`plus_is_space`) or in a
/// query string.
fn form_value(data: &str, name: &str, plus_is_space: bool) -> Option<String> {
    data.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=')?;
        (key == name).then(|| percent_decode(value, plus_is_space))
    })
}

/// The request body, whole: one larger than `limit` is 413, whether it
/// says its length up front or not, and one that ends before the length it
/// announced (the client gave up half-way) is not the request it meant.
fn read_body(request: &mut Request, limit: u64) -> std::result::Result<String, Reply> {
    let too_large = || {
        let size = if limit >= 1 << 20 {
            format!("{} MiB", limit >> 20)
        } else {
            format!("{} KiB", limit >> 10)
        };
        Reply::json_error(413, &format!("request body larger than {size}"))
    };
    let announced = request.body_length();
    if announced.is_some_and(|len| len as u64 > limit) {
        return Err(too_large());
    }
    let mut body = Vec::new();
    request
        .as_reader()
        .take(limit + 1)
        .read_to_end(&mut body)
        .map_err(|e| Reply::json_error(400, &format!("failed to read request body: {e}")))?;
    if body.len() as u64 > limit {
        return Err(too_large());
    }
    if announced.is_some_and(|len| body.len() < len) {
        return Err(Reply::json_error(400, "request body shorter than its Content-Length"));
    }
    BODY_READ.set(true);
    String::from_utf8(body).map_err(|_| Reply::json_error(400, "request body is not UTF-8"))
}

/// Whether the request's `Content-Type` is JSON: the media type compared
/// without regard to case, parameters (`; charset=utf-8`) aside (REVIEW
/// №151).
fn sends_json(request: &Request) -> bool {
    header_value(request, "Content-Type")
        .and_then(|value| value.split(';').next())
        .is_some_and(|media| media.trim().eq_ignore_ascii_case("application/json"))
}

/// Whether the request reached the server over TLS: directly, or through a
/// reverse proxy that says so.
/// Of a chain of proxies the first entry counts: the one the browser
/// talked to (`X-Forwarded-Proto: https, http`; `Forwarded: proto=https`).
fn over_tls(request: &Request) -> bool {
    let https = |proto: &str| proto.trim().trim_matches('"').eq_ignore_ascii_case("https");
    let forwarded = |value: &str| {
        let first = value.split(',').next().unwrap_or_default();
        first
            .split(';')
            .filter_map(|pair| pair.split_once('='))
            .any(|(key, value)| key.trim().eq_ignore_ascii_case("proto") && https(value))
    };
    request.secure()
        || header_value(request, "X-Forwarded-Proto")
            .and_then(|value| value.split(',').next())
            .is_some_and(https)
        || header_value(request, "Forwarded").is_some_and(forwarded)
}

/// Fresh TaskManager per request: reading the database is cheap next to a
/// round trip, and this guarantees CLI writes are always visible.
fn open_tm() -> Result<TaskManager> {
    TaskManager::open()
}

fn task_id_from_path(path: &str) -> Option<TaskId> {
    path.strip_prefix("/api/tasks/")?.parse().ok()
}

/// Whether any token the request presents is `token`: a stale cookie does
/// not hide a valid Bearer token, nor the other way round (REVIEW №197).
fn authorized(request: &Request, token: &str) -> bool {
    presented_tokens(request)
        .into_iter()
        .any(|presented| token_eq(presented, token))
}

/// The servers a request came through, by the ids of its
/// `X-Rusk-Serve` header (see `crate::SERVE_ID`).
fn serves_of(request: &Request) -> Vec<String> {
    header_value(request, crate::SERVE_ID_HEADER)
        .map(|chain| {
            chain
                .split(',')
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Keeps the servers of the request this thread answers in
/// `crate::SERVE_VIA` while it is answered, for the requests it makes.
struct Via;

impl Via {
    fn set(serves: Vec<String>) -> Self {
        crate::SERVE_VIA.with(|via| *via.borrow_mut() = serves);
        Via
    }
}

impl Drop for Via {
    fn drop(&mut self) {
        crate::SERVE_VIA.with(|via| via.borrow_mut().clear());
    }
}

fn route(request: &mut Request, opts: &ServeOptions) -> Reply {
    // A request that has come through this server already: made by it to
    // reach its database, directly or through another server whose
    // database this one is. Answering it would wait for the answer to
    // itself (see `crate::SERVE_ID`). Before anything else, the token too:
    // the answer says nothing of the tasks.
    let serves = serves_of(request);
    if crate::SERVE_ID.get().is_some_and(|id| serves.contains(id)) {
        return Reply::json_error(
            508,
            "this rusk serve is its own database: rusk_db / RUSK_DB names the address it \
             serves on, or that of a server whose database it is — point it at the database \
             file instead",
        );
    }
    let _via = Via::set(serves);
    let url = request.url().to_string();
    let (path, query) = url.split_once('?').unwrap_or((url.as_str(), ""));
    // HEAD is GET without the body (tiny_http leaves it out): uptime checks
    // and proxies probe with it (REVIEW №110).
    let method = match request.method() {
        Method::Head => Method::Get,
        method => method.clone(),
    };
    // One request, one Host (RFC 9112 §3.2): with two, which one the check
    // below would see is anybody's guess.
    if request.headers().iter().filter(|h| h.field.equiv("Host")).count() > 1 {
        return Reply::json_error(400, "more than one Host header");
    }

    match &opts.token {
        Some(token) => {
            // Bookmarkable `/?token=...`: on match, move the token into a
            // cookie and redirect so it leaves the address bar; a wrong one
            // gets the login page, like no token at all (REVIEW №111) —
            // unless the request is signed in anyway, which then goes on to
            // the page.
            if method == Method::Get
                && path == "/"
                && let Some(t) = form_value(query, "token", false)
            {
                return if token_eq(&t, token) {
                    Reply::redirect_home().with_session_cookie(token, over_tls(request))
                } else if authorized(request, token) {
                    Reply::redirect_home()
                } else {
                    Reply::html(401, LOGIN_PAGE)
                };
            }
            // Signing out needs no signing in: a stale cookie goes too.
            if path == "/logout" {
                return logout(request, &method);
            }
            if method == Method::Post && path == "/auth" {
                let body = match read_body(request, AUTH_BODY_BYTES) {
                    Ok(b) => b,
                    Err(reply) => return reply,
                };
                return match form_value(&body, "token", true) {
                    Some(t) if token_eq(&t, token) => {
                        Reply::redirect_home().with_session_cookie(token, over_tls(request))
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
        // Without a token whoever reaches the server may change the tasks;
        // it binds loopback for that reason. A page of another site whose
        // name it has pointed at 127.0.0.1 (DNS rebinding) reaches it too,
        // as a same-origin page, but its requests carry that name in
        // `Host` (REVIEW №113, SECURITY.md M2).
        None => {
            if let Some(host) = header_value(request, "Host")
                && !host_header_is_loopback(host)
            {
                return Reply::json_error(
                    403,
                    "without web_token this server answers only requests addressed to localhost \
                     or a loopback address; set web_token to serve other names",
                );
            }
            if path == "/logout" {
                return logout(request, &method);
            }
        }
    }

    // CSRF guard for state changes: cross-origin forms cannot send
    // application/json without a CORS preflight, and we emit no CORS headers.
    let mutating = matches!(method, Method::Post | Method::Put | Method::Patch);
    if mutating && path.starts_with("/api/") && !sends_json(request) {
        return Reply::json_error(415, "Content-Type must be application/json");
    }

    let if_match = header_value(request, "If-Match").map(str::to_string);

    match (method, path) {
        (Method::Get, "/") => with_tasks(|tm| {
            match super::render_live_page(tm.tasks(), opts.token.is_some()) {
                Ok(html) => Reply::html(200, html),
                Err(e) => Reply::json_error(500, &format!("{e:#}")),
            }
        }),
        (Method::Get, "/api/tasks") => with_tasks(|tm| Reply::from_api(api::list_tasks(tm))),
        (Method::Post, "/api/tasks") => with_body_and_tm(request, |tm, body| {
            api::create_task(tm, body, if_match.as_deref())
        }),
        (Method::Put, "/api/tasks") => with_body_and_tm(request, |tm, body| {
            api::replace_tasks(tm, body, if_match.as_deref())
        }),
        (Method::Delete, "/api/tasks/done") => {
            with_tasks(|tm| Reply::from_api(api::delete_done(tm, if_match.as_deref())))
        }
        (Method::Get, p) if task_id_from_path(p).is_some() => {
            let id = task_id_from_path(p).unwrap();
            with_tasks(|tm| Reply::from_api(api::get_task(tm, id)))
        }
        (Method::Patch, p) if task_id_from_path(p).is_some() => {
            let id = task_id_from_path(p).unwrap();
            with_body_and_tm(request, |tm, body| {
                api::update_task(tm, id, body, if_match.as_deref())
            })
        }
        (Method::Delete, p) if task_id_from_path(p).is_some() => {
            let id = task_id_from_path(p).unwrap();
            with_tasks(|tm| Reply::from_api(api::delete_task(tm, id, if_match.as_deref())))
        }
        (_, p) if p == "/api/tasks" || p == "/api/tasks/done" || task_id_from_path(p).is_some() => {
            Reply::json_error(405, "method not allowed")
        }
        (_, p) if p.starts_with("/api/") => Reply::json_error(404, "not found"),
        _ => Reply::html(404, "<!DOCTYPE html><title>404</title><p>Not found — <a href=\"/\">rusk</a>"),
    }
}

/// Works on the tasks, read afresh, as the one request doing so (see
/// [`DATABASE`]).
fn with_tasks(work: impl FnOnce(&mut TaskManager) -> Reply) -> Reply {
    let _alone = DATABASE.lock().unwrap_or_else(|e| e.into_inner());
    match open_tm() {
        Ok(mut tm) => work(&mut tm),
        Err(e) => Reply::json_error(500, &format!("{e:#}")),
    }
}

/// The body is read before the tasks are taken: a client that sends it
/// slowly holds up nobody.
fn with_body_and_tm(
    request: &mut Request,
    handler: impl FnOnce(&mut TaskManager, &str) -> ApiResponse,
) -> Reply {
    let body = match read_body(request, MAX_BODY_BYTES) {
        Ok(b) => b,
        Err(reply) => return reply,
    };
    with_tasks(|tm| Reply::from_api(handler(tm, &body)))
}

/// `/logout` drops the session cookie. Only a POST that says it is JSON,
/// like every change: a page of another site cannot send one, while it can
/// follow a link or post a form (and a link can be prefetched).
fn logout(request: &Request, method: &Method) -> Reply {
    match method {
        Method::Post if sends_json(request) => Reply::logged_out(),
        Method::Post => Reply::json_error(415, "Content-Type must be application/json"),
        _ => Reply::json_error(405, "method not allowed"),
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
        )
        // No page of another site may show this one in a frame, where a
        // click it lures out lands on a task (REVIEW №198).
        .with_header(Header::from_bytes("X-Frame-Options", "DENY").expect("static header is valid"))
        .with_header(
            Header::from_bytes("Content-Security-Policy", "frame-ancestors 'none'")
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
    if let Some(token) = &opts.token
        && !token_is_valid(token)
    {
        bail!(
            "web_token may hold printable ASCII only (spaces inside are fine), without `;` \
             and without spaces at its ends: it travels in a cookie and an HTTP header as it \
             is. Pick another, e.g. the output of `openssl rand -base64 24`"
        );
    }
    if opts.token.is_none() && !is_loopback(&opts.host) {
        bail!(
            "refusing to serve on {} without authentication: set `web_token` in the config \
             (recommended behind a TLS reverse proxy), or bind 127.0.0.1",
            opts.host
        );
    }

    let _ = crate::SERVE_ID.set(format!("{:016x}", crate::atomic::unique()));
    let host = bind_host(&opts.host);
    let server = match host.parse::<IpAddr>() {
        Ok(ip) => Server::http(SocketAddr::new(ip, opts.port)),
        Err(_) => Server::http(format!("{host}:{}", opts.port)),
    }
    .map_err(|e| anyhow!("failed to bind {}:{}: {e}", opts.host, opts.port))?;

    // The actual address matters with --port 0 (tests parse this line).
    let addr = server
        .server_addr()
        .to_ip()
        .map(|a| a.to_string())
        .unwrap_or_else(|| format!("{}:{}", opts.host, opts.port));
    // The server goes on serving whether or not anybody reads this: an
    // output that fails is no reason to stop it.
    crate::outln!("rusk serve listening on http://{addr}").ok();
    crate::outln!(
        "Database: {}",
        crate::backend::Backend::resolve()
            .map(|b| b.describe())
            .unwrap_or_else(|e| format!("unavailable ({e:#})"))
    )
    .ok();
    crate::outln!(
        "Auth: {}",
        if opts.token.is_some() {
            "token required"
        } else {
            "off (loopback only)"
        }
    )
    .ok();
    crate::outln!("Press Ctrl+C to stop.").ok();

    let opts = Arc::new(opts);
    loop {
        // tiny_http stops taking connections for good after a failed accept
        // (out of file descriptors): that ends the server with an error, so
        // that a supervisor starts it again, not with success (review of
        // R20).
        let request = server
            .recv()
            .map_err(|e| anyhow!("rusk serve stopped taking connections: {e}"))?;
        let opts = Arc::clone(&opts);
        let spawned = std::thread::Builder::new()
            .name("rusk-request".into())
            .spawn(move || {
                let mut request = request;
                let reply = route(&mut request, &opts);
                let unread = request
                    .body_length()
                    .filter(|&len| len > PREFETCHED_BODY_BYTES && !BODY_READ.get());
                match unread {
                    None => respond(request, reply),
                    Some(len) if len <= DISCARDED_BODY_BYTES => {
                        let _ = std::io::copy(
                            &mut request.as_reader().take(len as u64),
                            &mut std::io::sink(),
                        );
                        respond(request, reply);
                    }
                    Some(_) => {
                        let _one = DRAIN.lock().unwrap_or_else(|e| e.into_inner());
                        respond(request, reply);
                    }
                }
            });
        if let Err(e) = spawned {
            crate::errln!("rusk serve: no thread to answer a request: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tiny_http::TestRequest;

    fn header(name: &str, value: &str) -> Header {
        Header::from_bytes(name, value).unwrap()
    }

    fn with_token(token: Option<&str>) -> ServeOptions {
        ServeOptions {
            host: "127.0.0.1".into(),
            port: 0,
            token: token.map(str::to_string),
        }
    }

    fn reply_to(request: TestRequest, opts: &ServeOptions) -> Reply {
        let mut request: Request = request.into();
        route(&mut request, opts)
    }

    fn header_of<'a>(reply: &'a Reply, name: &str) -> Option<&'a str> {
        reply
            .headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// REVIEW №112: the loopback check compared strings.
    #[test]
    fn loopback_is_an_address_not_a_spelling() {
        for host in ["127.0.0.1", "127.0.0.2", "::1", "[::1]", "localhost", "LOCALHOST", "LocalHost", "localhost.", "::ffff:127.0.0.1"] {
            assert!(is_loopback(host), "{host}");
        }
        for host in ["0.0.0.0", "192.168.1.10", "::", "example.com", "localhost.example.com"] {
            assert!(!is_loopback(host), "{host}");
        }
        assert_eq!(bind_host("[::1]"), "::1");
        assert_eq!(bind_host("LOCALHOST"), "127.0.0.1");
        assert_eq!(bind_host("localhost."), "127.0.0.1");
        assert_eq!(bind_host("0.0.0.0"), "0.0.0.0");
    }

    /// REVIEW section 4: a server whose database is its own address waited
    /// on itself for every request. Its own requests say whose they are,
    /// and are answered at once, before the token is looked at.
    #[test]
    fn a_request_of_this_server_itself_is_told_at_once() {
        let _ = crate::SERVE_ID.set("00000000c0ffee00".into());
        let id = crate::SERVE_ID.get().unwrap().clone();
        let own = TestRequest::new().with_path("/api/tasks").with_header(header(crate::SERVE_ID_HEADER, &id));
        let reply = reply_to(own, &with_token(Some("tok")));
        assert_eq!(reply.status, 508);
        assert!(reply.body.contains("its own database"), "{}", reply.body);
        // Another server's is a request like any other.
        let other = TestRequest::new().with_path("/api/tasks").with_header(header(crate::SERVE_ID_HEADER, "ffff"));
        assert_eq!(reply_to(other, &with_token(Some("tok"))).status, 401);
        // One that has come through this server by way of another (review
        // of R28: two servers, each the other's database).
        let round = TestRequest::new()
            .with_path("/api/tasks")
            .with_header(header(crate::SERVE_ID_HEADER, &format!("ffff, {id}")));
        assert_eq!(reply_to(round, &with_token(Some("tok"))).status, 508);
    }

    #[test]
    fn a_host_header_names_loopback_with_or_without_a_port() {
        for value in [
            "localhost", "localhost:7272", "localhost.:7272", "127.0.0.1:7272", "127.1.2.3", "[::1]:7272", "[::1]",
            "LOCALHOST:1", "[::ffff:127.0.0.1]:80",
        ] {
            assert!(host_header_is_loopback(value), "{value}");
        }
        for value in [
            "evil.example", "evil.example:7272", "127.0.0.1.nip.io", "localhost.evil", "[::2]:80", "",
            "[::1]evil", "[::1]:evil", "localhost:abc", "localhost:", "[::1", "::1",
        ] {
            assert!(!host_header_is_loopback(value), "{value}");
        }
    }

    /// REVIEW №52: a token that cannot travel in a cookie or a header as
    /// it is could never sign in. Spaces inside it, quotes, commas travel
    /// (review of R20: a token that worked must not stop the server).
    #[test]
    fn a_token_is_one_a_cookie_and_a_header_carry_as_they_are() {
        for token in ["plain", "ab+cd/ef==", "two words", "a,b", "a\"b", "a\\b", "a-b_c.d~e!#$%&'()*+-./:<=>?@[]^`{|}~"] {
            assert!(token_is_valid(token), "{token}");
        }
        for token in ["", "café", "tok;en", " lead", "trail ", "tab\tinside", "line\nbreak"] {
            assert!(!token_is_valid(token), "{token:?}");
        }
    }

    /// Review of R20: two Host headers leave the check to chance.
    #[test]
    fn a_request_with_two_hosts_is_refused() {
        let twice = TestRequest::new()
            .with_header(header("Host", "localhost"))
            .with_header(header("Host", "evil.example"));
        assert_eq!(reply_to(twice, &with_token(None)).status, 400);
        // No Host at all (HTTP/1.0) is a request to whatever answers.
        assert_eq!(reply_to(TestRequest::new().with_path("/api/nothing"), &with_token(None)).status, 404);
    }

    /// Review of R20: the scheme of `Authorization` is a name in any case.
    #[test]
    fn a_bearer_token_in_any_case() {
        for value in ["Bearer sekret", "bearer sekret", "BEARER  sekret "] {
            let request: Request = TestRequest::new().with_header(header("Authorization", value)).into();
            assert!(authorized(&request, "sekret"), "{value}");
        }
        let request: Request = TestRequest::new().with_header(header("Authorization", "Basic sekret")).into();
        assert!(!authorized(&request, "sekret"));
    }

    /// REVIEW №109: `+` in a bookmarked `?token=` became a space.
    #[test]
    fn plus_is_a_space_only_in_a_form_body() {
        assert_eq!(form_value("token=ab+cd%2Fef%3D%3D", "token", false).unwrap(), "ab+cd/ef==");
        assert_eq!(form_value("token=ab+cd%2B", "token", true).unwrap(), "ab cd+");
        assert_eq!(form_value("x=1&token=%zz", "token", false).unwrap(), "%zz");
    }

    /// REVIEW №113 / SECURITY.md M2: without a token, a request addressed
    /// to another name (DNS rebinding) is refused before anything is read.
    #[test]
    fn without_a_token_only_loopback_names_are_served() {
        let opts = with_token(None);
        let rebound = TestRequest::new()
            .with_path("/api/tasks")
            .with_header(header("Host", "evil.example:7272"));
        assert_eq!(reply_to(rebound, &opts).status, 403);
        let posted = TestRequest::new()
            .with_method(Method::Post)
            .with_path("/api/tasks")
            .with_header(header("Host", "evil.example"))
            .with_header(header("Content-Type", "application/json"))
            .with_body(r#"{"text":"injected"}"#);
        assert_eq!(reply_to(posted, &opts).status, 403);
        // With a token the Host is no concern: the token is.
        let opts = with_token(Some("sekret"));
        let unauthorized = TestRequest::new()
            .with_path("/api/tasks")
            .with_header(header("Host", "tasks.example.com"));
        assert_eq!(reply_to(unauthorized, &opts).status, 401);
    }

    /// REVIEW №197: a stale cookie hid a valid Bearer token.
    #[test]
    fn any_token_presented_that_matches_authorizes() {
        let request: Request = TestRequest::new()
            .with_header(header("Cookie", "theme=dark; rusk_token=oldtoken"))
            .with_header(header("Authorization", "Bearer sekret"))
            .into();
        assert!(authorized(&request, "sekret"));
        let request: Request = TestRequest::new()
            .with_header(header("Cookie", "rusk_token=sekret"))
            .with_header(header("Authorization", "Bearer nope"))
            .into();
        assert!(authorized(&request, "sekret"));
        let request: Request = TestRequest::new()
            .with_header(header("Cookie", "rusk_token=old; rusk_token=older"))
            .into();
        assert!(!authorized(&request, "sekret"));
    }

    /// REVIEW №110, №111: HEAD is answered like GET; a wrong `?token=`
    /// gets the login page, not raw JSON.
    #[test]
    fn head_and_a_wrong_bookmark_get_pages() {
        let opts = with_token(Some("sekret"));
        let head = TestRequest::new().with_method(Method::Head);
        let reply = reply_to(head, &opts);
        assert_eq!((reply.status, reply.content_type), (200, "text/html; charset=utf-8"));
        let wrong = TestRequest::new().with_path("/?token=old");
        let reply = reply_to(wrong, &opts);
        assert_eq!(reply.status, 401);
        assert!(reply.body.contains("<form"), "{}", reply.body);
        let right = TestRequest::new().with_path("/?token=sekret");
        let reply = reply_to(right, &opts);
        assert_eq!(reply.status, 303);
        assert!(header_of(&reply, "Set-Cookie").unwrap().starts_with("rusk_token=sekret;"));
        // Signed in already: an old bookmark goes on to the page.
        let signed_in = TestRequest::new()
            .with_path("/?token=old")
            .with_header(header("Cookie", "rusk_token=sekret"));
        let reply = reply_to(signed_in, &opts);
        assert_eq!(reply.status, 303);
        assert!(header_of(&reply, "Set-Cookie").is_none());
    }

    /// REVIEW №152: the cookie is `Secure` behind TLS, and `/logout` drops it.
    #[test]
    fn the_session_cookie_is_secure_behind_tls_and_can_be_dropped() {
        let opts = with_token(Some("sekret"));
        let plain = reply_to(TestRequest::new().with_path("/?token=sekret"), &opts);
        assert!(!header_of(&plain, "Set-Cookie").unwrap().contains("Secure"));
        let proxied = TestRequest::new()
            .with_path("/?token=sekret")
            .with_header(header("X-Forwarded-Proto", "https"));
        let reply = reply_to(proxied, &opts);
        assert!(header_of(&reply, "Set-Cookie").unwrap().ends_with("; Secure"));
        // Of a chain of proxies the first says it; `Forwarded` too; and a
        // request that came over TLS itself.
        for (name, value) in [
            ("X-Forwarded-Proto", "https, http"),
            ("Forwarded", "for=1.2.3.4;proto=https, for=10.0.0.1;proto=http"),
            ("Forwarded", "proto=\"HTTPS\""),
        ] {
            let proxied = TestRequest::new().with_path("/?token=sekret").with_header(header(name, value));
            let reply = reply_to(proxied, &opts);
            assert!(header_of(&reply, "Set-Cookie").unwrap().ends_with("; Secure"), "{name}: {value}");
        }
        let reply = reply_to(TestRequest::new().with_path("/?token=sekret").with_https(), &opts);
        assert!(header_of(&reply, "Set-Cookie").unwrap().ends_with("; Secure"));
        let proxied = TestRequest::new()
            .with_path("/?token=sekret")
            .with_header(header("X-Forwarded-Proto", "http, https"));
        assert!(!header_of(&reply_to(proxied, &opts), "Set-Cookie").unwrap().contains("Secure"));
    }

    /// REVIEW №152, review of R20: `/logout` drops the cookie — asked for
    /// the way a change is, so that no page of another site can.
    #[test]
    fn logout_is_a_post_that_says_it_is_json() {
        let opts = with_token(Some("sekret"));
        let logout = TestRequest::new()
            .with_method(Method::Post)
            .with_path("/logout")
            .with_header(header("Content-Type", "application/json"))
            .with_body("{}");
        let reply = reply_to(logout, &opts);
        assert_eq!(reply.status, 204);
        assert!(header_of(&reply, "Set-Cookie").unwrap().contains("Max-Age=0"));
        assert_eq!(reply_to(TestRequest::new().with_path("/logout"), &opts).status, 405);
        let form = TestRequest::new()
            .with_method(Method::Post)
            .with_path("/logout")
            .with_header(header("Content-Type", "application/x-www-form-urlencoded"));
        assert_eq!(reply_to(form, &opts).status, 415);
        // Without a token, behind the Host check like everything else.
        let rebound = TestRequest::new()
            .with_method(Method::Post)
            .with_path("/logout")
            .with_header(header("Host", "evil.example"))
            .with_header(header("Content-Type", "application/json"));
        assert_eq!(reply_to(rebound, &with_token(None)).status, 403);
    }

    /// REVIEW №151: the media type is compared without regard to case.
    #[test]
    fn content_type_is_json_in_any_case() {
        for (value, json) in [
            ("application/json", true),
            ("Application/JSON; charset=UTF-8", true),
            (" application/json ;charset=utf-8", true),
            ("text/plain", false),
            ("application/json-seq", false),
        ] {
            let request: Request = TestRequest::new().with_header(header("Content-Type", value)).into();
            assert_eq!(sends_json(&request), json, "{value}");
        }
        let request: Request = TestRequest::new().into();
        assert!(!sends_json(&request));
    }

    /// Review of R20: two requests changed the tasks at the same moment, and
    /// a backend without a lock of its own (a file over ssh) kept one of
    /// the two changes.
    #[test]
    fn one_request_at_a_time_works_on_the_tasks() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static INSIDE: AtomicUsize = AtomicUsize::new(0);
        static MOST: AtomicUsize = AtomicUsize::new(0);
        let threads: Vec<_> = (0..8)
            .map(|_| {
                std::thread::spawn(|| {
                    with_tasks(|_| {
                        let now = INSIDE.fetch_add(1, Ordering::SeqCst) + 1;
                        MOST.fetch_max(now, Ordering::SeqCst);
                        std::thread::sleep(std::time::Duration::from_millis(20));
                        INSIDE.fetch_sub(1, Ordering::SeqCst);
                        Reply::html(200, "")
                    })
                })
            })
            .collect();
        for thread in threads {
            assert_eq!(thread.join().unwrap().status, 200);
        }
        assert_eq!(MOST.load(Ordering::SeqCst), 1);
    }

    /// REVIEW №54: a body over the limit is 413, not cut and misread.
    #[test]
    fn a_body_over_the_limit_is_refused() {
        let request = TestRequest::new()
            .with_method(Method::Put)
            .with_header(header("Content-Length", &(MAX_BODY_BYTES + 1).to_string()));
        let mut request: Request = request.into();
        let reply = read_body(&mut request, MAX_BODY_BYTES).unwrap_err();
        assert_eq!(reply.status, 413);
        let request = TestRequest::new().with_method(Method::Put).with_body("[]");
        let mut request: Request = request.into();
        assert_eq!(read_body(&mut request, MAX_BODY_BYTES).ok().as_deref(), Some("[]"));
    }

    /// Review of R20: the sign-in form is read before anybody signed in,
    /// and took 32 MiB; a body that ends before its announced length was
    /// taken for the whole.
    #[test]
    fn a_sign_in_form_is_small_and_a_body_is_whole() {
        let opts = with_token(Some("sekret"));
        let big = TestRequest::new()
            .with_method(Method::Post)
            .with_path("/auth")
            .with_header(header("Content-Length", &(AUTH_BODY_BYTES + 1).to_string()));
        let reply = reply_to(big, &opts);
        assert_eq!(reply.status, 413);
        assert!(reply.body.contains("4 KiB"), "{}", reply.body);
        let form = TestRequest::new().with_method(Method::Post).with_path("/auth").with_body("token=sekret");
        assert_eq!(reply_to(form, &opts).status, 303);

        let short = TestRequest::new()
            .with_method(Method::Put)
            .with_header(header("Content-Length", "2000"))
            .with_body("[]");
        let mut request: Request = short.into();
        let reply = read_body(&mut request, MAX_BODY_BYTES).unwrap_err();
        assert_eq!(reply.status, 400);
        assert!(reply.body.contains("shorter than its Content-Length"), "{}", reply.body);
    }
}
