# Web UI and sync

rusk ships a mobile-first web frontend built from a single embedded HTML
template: no node toolchain, no build step. Two commands share it:

- **`rusk gen`** renders a self-contained, read-only `index.html` with your
  tasks embedded. Host it anywhere static files go.
- **`rusk serve`** runs a small built-in HTTP server with the same UI plus a
  JSON API: add, edit, complete, prioritize and delete tasks from a phone.

Theme colors come from the configuration file ([CONFIG.md](CONFIG.md)) as CSS
variables, so the page matches your terminal theme. Light/dark surfaces follow
the system `prefers-color-scheme`.

- [rusk gen](#rusk-gen)
- [rusk serve](#rusk-serve)
  - [Requests](#requests)
  - [Timeouts and limits](#timeouts-and-limits)
  - [HTTP details](#http-details)
  - [The vendored tiny_http](#the-vendored-tiny_http)
  - [Authentication](#authentication)
  - [Deployment on a VPS](#deployment-on-a-vps)
  - [Working from the page](#working-from-the-page)
  - [API](#api)
- [rusk sync](#rusk-sync)
  - [SSH remote](#ssh-remote)
  - [HTTP remote](#http-remote)
  - [Conflict detection](#conflict-detection)

## rusk gen

```bash
rusk gen                       # writes ./index.html
rusk gen -o /var/www/tasks/index.html
rusk gen -o -                  # writes the page to stdout
```

The page is read-only (the file cannot modify your database) and shows its
generation time in the footer. Refresh it with a cron job or a shell alias:

```bash
# regenerate every 15 minutes and publish to a static host
*/15 * * * * cd ~/todo && rusk gen -o /var/www/tasks/index.html
```

## rusk serve

```bash
rusk serve                     # http://127.0.0.1:7272
rusk serve --port 8080
rusk serve --host 0.0.0.0     # requires web_token in the config
```

Defaults come from the config (`web_host`, `web_port`, `web_token`,
`web_timeout`); the flags override the first two. `localhost` (in any case)
binds 127.0.0.1, where clients look for it. An IPv6 address may be given
with or without brackets (`[::1]`).

The server re-reads the database on **every request**, so CLI edits are
instantly visible in the browser and vice versa. You can even
`rusk sync push` a new file under a running server.

### Requests

- Each request is read, checked and answered on a thread of its own, so a
  client that sends its request body slowly holds up no other request.
- The work on the tasks is done by one request at a time, so two changes
  made at the same moment are both kept, whatever the database.
- A request body may be up to 32 MiB (a `PUT` of thousands of tasks is a
  few MB), the sign-in form up to 4 KiB. A bigger one is refused with
  `413`; one that ends before the length it announced gets `400`.
- `HEAD` is answered like `GET`, without the body, for uptime checks and
  proxies.
- If the server cannot take connections any more (no file descriptors
  left), it ends with an error, so that a supervisor (`Restart=on-failure`)
  starts it again.

### Timeouts and limits

A connection may wait a minute (`web_timeout` in the config, in seconds):

- idle, for its next request (browsers keep a few open);
- for the rest of a request's head after its first byte, however slowly the
  bytes come;
- in a request body or an answer, for the socket to move at all.

Past that, a body or an answer has to keep up a kilobyte a second: the time
it spends waiting may come to the minute and a second more per kilobyte
moved. A client that keeps moving is never cut off, however long a big `PUT`
takes. One that sends a byte of a body, or takes a byte of an answer, now and
then is.

A connection that waits longer is closed:

- with `408 Request Timeout` when part of a request had come. A body that
  stops or trickles gets it too, chunked or not, wherever it stops, and the
  request has to be sent again;
- without a word when it was idle, as browsers expect of a keep-alive
  connection.

Up to 256 connections are open at once. Fewer if the process may not open
the file descriptors they take: the limit is raised as far as allowed, and
the startup banner says what it is. The next connection waits in the
listening socket's queue until one closes. The requests of one connection
are taken one at a time, the next after the answer to the one before, so the
requests in flight (a thread each) are bounded by the same limit.

`web_timeout = 0` turns all of this off, as it was before: no timeout, no
pace, no limit on connections. With no timeout to close them, connections a
client dropped without a word would otherwise fill the limit for good.

The limits keep the server up and its threads, memory and file descriptors
bounded whatever clients do. They do not make it fair. A client that keeps
256 connections busy (opening new ones as the old are cut off, heads a byte
at a time, bodies or answers at a kilobyte a second) keeps everybody else
waiting while it does. On loopback, a trusted network or a VPN that does not
matter. Exposed wider, put a reverse proxy in front that buffers requests
and answers and limits them per client (nginx does, see
[Deployment on a VPS](#deployment-on-a-vps)). It is the place for TLS anyway.

### HTTP details

- An answer after which the server closes the connection says so
  (`Connection: close`). That is the `408`; a `400` to a request it cannot
  make sense of (a chunked body it read and found in the wrong format, a
  `Content-Length` that is no number), since where the next request would
  start is not known then; and the answer to a request that said it was the
  last (`Connection: close`, HTTP/1.0 without keep-alive).
- An HTTP/1.0 client that asks to keep the connection is told that it is
  kept (`Connection: keep-alive`).
- A request in an HTTP version the server does not speak is answered `505`
  once, and the connection ends. HTTP/0.9 (`GET /path` with no version) is
  among them. `HTTP/1.2` to `HTTP/1.9` are served as HTTP/1.1.
- Empty lines before a request line are ignored.
- An upgrade the server does not take up (curl proposes h2c on every request
  with `--http2`) is answered as any other request, its body read within its
  headers, and the connection goes on: the next request is read once the
  answer is out.
- A client waiting to be told to send its body (`Expect: 100-continue`) is
  told (`100 Continue`) when the server reads the body, or throws it away: a
  body up to a megabyte it answers without reading. A bigger one, or a
  chunked one, it does not ask for. The answer (401, 413, 415…) says that
  the connection ends, and it does, without waiting for a body the client
  was never told to send.
- A chunked body the server answers without reading (401, 404, 415) is read
  and thrown away after the answer, so that the connection stays in step.
  If it turns out wrong or stops then, the connection ends after an answer
  that could not say so.

### The vendored tiny_http

`rusk serve` runs on [tiny_http](https://github.com/tiny-http/tiny-http)
0.12, vendored under `vendor/` with six changes of rusk's (see
`vendor/README.md`):

- A request answered without reading its body (a wrong token, a body too
  big) takes the rest of that body from the connection before it goes:
  through a small buffer, up to where the connection ends. The crate as
  published took a buffer as large as the announced rest, and a
  `Content-Length: 1000000000000000` aborted the server.
- Every connection of a burst is read. As published, some of six opened at
  once by a browser could hang until another closed.
- The limits above. The crate as published has nothing of them: every open
  connection cost a thread for as long as the client liked, and a client
  could send requests down one connection faster than they were answered, a
  thread each.
- A chunked body that stops is a timeout, not a bad request, and one left
  unread is thrown away.
- An answer says what becomes of the connection.
- A `Connection: upgrade` request's body is read within its headers, and
  the connection goes on when the upgrade is not taken up.

### Authentication

With `web_token` set:

- The browser shows a token form once and keeps the session in an `HttpOnly`
  cookie: `SameSite=Strict`, one year, `Secure` when the request came over
  TLS. A reverse proxy says so in `X-Forwarded-Proto: https` or
  `Forwarded: proto=https`; of a chain of proxies the first entry counts.
- **Sign out** in the page header drops the cookie. It is a `POST /logout`
  with `Content-Type: application/json`, like every change, so that no
  other site can sign you out.
- The cookie holds the token itself. To take the access of a copied cookie
  away, change `web_token`.
- `Authorization: Bearer <token>` (the scheme in any case) is for curl and
  scripts. A request is let in when any token it presents matches, so a
  stale cookie does not hide a valid Bearer token.
- `http://host:port/?token=<token>` is bookmarkable on a phone. The token is
  moved into the cookie and stripped from the URL by a redirect. A `+` in
  the address is a `+`; a `%`, `&` or `#` in the token has to be written
  `%25`, `%26`, `%23` (base64 tokens have none). A wrong or old token gets
  the sign-in form, which says the token was not the one; when the browser
  is signed in anyway, the page.

Signed out:

- `/` is the sign-in form (`200`). Any other page path answers it with
  `401`, and `/api/` paths with a JSON `401`.
- A sign-in posted from a page of another origin is refused like any change
  (`403`, see [API](#api)).
- The pages of the server ask the browser for no icon, so no `/favicon.ico`
  request goes out. A static page from `rusk gen` keeps the icon of the
  site it is put on.

The token travels as it is in a cookie and an HTTP header, so it may hold
printable ASCII only: spaces inside it are fine, but no `;` and no spaces at
its ends. `rusk serve` refuses to start with any other.
`openssl rand -base64 24` makes a good one.

Without a token:

- rusk **refuses to bind non-loopback addresses**. Loopback is all of
  127.0.0.0/8, `::1` and `localhost`. A CRUD API over your tasks should not
  be world-writable by accident.
- It answers only requests addressed to `localhost` or a loopback address
  (the `Host` header). A web page that points a name of its own at
  127.0.0.1 (DNS rebinding) is refused with `403`; a request with two
  `Host` headers with `400`.
- Behind **any** reverse proxy, set `web_token`. nginx, for one, passes
  `Host: 127.0.0.1:7272` on by default, and the whole internet would be
  "loopback" to a server without a token.

Every page is sent with `X-Frame-Options: DENY` and
`Content-Security-Policy: frame-ancestors 'none'`: no other site can show it
in a frame and lure clicks onto your tasks.

### Deployment on a VPS

`rusk serve` speaks plain HTTP. Put a TLS reverse proxy in front for anything
beyond localhost or a trusted LAN. Caddy makes it two lines:

```
tasks.example.com {
    reverse_proxy 127.0.0.1:7272
}
```

nginx equivalent:

```nginx
# In the http block: connections counted per client address.
limit_conn_zone $binary_remote_addr zone=rusk_peers:1m;

server {
    listen 443 ssl;
    server_name tasks.example.com;
    # ssl_certificate ...; ssl_certificate_key ...;
    client_max_body_size 32m;   # nginx's own default, 1m, would cap `sync push`
    client_body_timeout 60s;    # nginx's default: a client that stalls is cut off here
    limit_conn rusk_peers 16;   # no one address takes all of rusk's 256 connections
    location / {
        proxy_pass http://127.0.0.1:7272;
        proxy_set_header X-Forwarded-Proto $scheme;   # the cookie gets `Secure`
    }
}
```

A zero-config alternative is [Tailscale](https://tailscale.com): bind
`rusk serve` to the tailnet address and skip the proxy entirely.

### Working from the page

The CLI and the web UI can be used side by side:

- Every request, like every CLI command, applies its change to the database
  as it is at that moment (a file database is written under a lock,
  `tasks.json.lock`), so neither overwrites the other.
- The page additionally tells the server which state of a task it is
  showing (see `If-Match` in the [API](#api)). If that task was changed
  elsewhere since, or deleted and its id given to a new task (so row 3 is a
  different task by now), the change is refused, the list reloads and you
  repeat it on what is actually there. Changes to other tasks do not get in
  the way.
- The automatic `.backup` copy always keeps the previous state.

The edit dialog:

- It closes only once the server has taken the change. Until then what you
  typed stays in it. The dialog cannot be closed while a Save or Delete is
  on its way (a request is given 30 s).
- A lost connection or a refused change is reported inside the dialog, so
  Save can simply be pressed again.
- When the task was changed elsewhere in the meantime, the dialog says what
  is different now (its text above all: it may be another task under a
  reused id), takes over the fields you did not touch and keeps the ones you
  did. After a moment, Save again applies them on top of the other change.
- Save sends only the fields you changed, and nothing at all when you
  changed none.

Errors outside the dialog show in a banner at the top, which stays until a
change made after it goes through. Its Reload button re-reads the list. If
the token stops being accepted (it was changed in the config), the page shows
the sign-in form right away when nothing is typed on it; otherwise it says so
in the banner and leaves reloading to you.

### API

All endpoints return JSON and require the token (when configured) via cookie
or Bearer header. Dates are ISO `YYYY-MM-DD`, in the years 1000–9999.

| Method & path | Body | Result |
|---|---|---|
| `GET /api/tasks` | — | full task list |
| `GET /api/tasks/{id}` | — | one task |
| `POST /api/tasks` | `{"text": "...", "date": "2026-08-01" \| null, "after": [1, 2]}` (`date` and `after` optional) | `201` + created task, listed in the place of its id |
| `PATCH /api/tasks/{id}` | any subset of `{text, date, done, priority, after}`; `"date": null` clears, `after` replaces the whole list (`[]` clears; the same ids in another order are no change) | `200` + updated task |
| `DELETE /api/tasks/{id}` | — | `204` |
| `DELETE /api/tasks/done` | — | `{"deleted": n}` |
| `PUT /api/tasks` | full task array | replaces the whole list (used by sync); `{"count": n, "tasks": [...]}`, the list as the server's database holds it |

**Tasks.** A task is sent as the JSON format stores it: `id`, `text`, `date`
(`YYYY-MM-DD` or `null`), `done`, `priority` and `after`, the ids of the
tasks it waits for (`--after`; left out when there are none).

- `after` may name only other tasks that exist and must not close a loop (a
  task that waits for itself, or for one that waits for it). Otherwise a
  `400` says what is wrong.
- A text is stored without whitespace at its edges, as the CLI stores it.
- A new task takes the lowest free id and goes in front of the first task
  with a higher one, so a reused id is not listed at the end.
- An id in a path is digits, leading zeros allowed as on the command line:
  `/api/tasks/+1` is `404`, `/api/tasks/01` is task 1.

**`PUT`** stores the list exactly as sent, so it has to follow the rules
every list read from a database is held to
([STORAGE.md](STORAGE.md#what-every-format-is-held-to)): each task has a
text and an id of its own (nonzero), and `after` lists only other tasks of
the list, each once. A list that breaks one is refused with `400` and says
what is wrong (`not a valid task list: task 3 has no text`). A rusk client
makes its list follow these rules when it reads it. rusk 0.7.3 and older did
not check texts and `after`, so their `rusk sync push` of such a list fails
until the client is updated.

**Errors.**

- A body that is not JSON is a `400` "invalid JSON". One with a value a task
  cannot take is a `400` that names it
  (`invalid task: date '2d' is not written YYYY-MM-DD at line 1 column 25`).
- Errors come as `{"error": "..."}` with a 4xx/5xx status. The message
  includes the cause
  (`Failed to write the database file '...': ... (os error 28)`), not only
  the step that failed.
- An error the server meets with its database goes to the client whole: the
  path of the file, up to three lines of it around the fault, what to do
  about it, or what ssh or curl said. The client either holds the token or
  runs on the machine itself. The password of an http URL and the token are
  never in it.

**Cross-site requests.**

- Mutating requests must send `Content-Type: application/json` (a CSRF
  guard). The media type is compared without regard to case, parameters
  such as `; charset=utf-8` aside.
- A change a browser says comes from a page of another origin
  (`Sec-Fetch-Site` other than `same-origin` or `none`) is refused with
  `403`, on any path (`/auth` and `/logout` too). That covers a page of the
  same host on another port, or of a sibling domain, which the browser
  counts as the same site and sends the session cookie from.
- Browsers send `Sec-Fetch-Site` to https and loopback addresses only. Over
  plain HTTP on a LAN or a tailnet, and in a browser that never sends it,
  the Content-Type check alone holds such a page back. Clients that are no
  browser send no such header.

**Requests that change nothing** (`PATCH` with the values the task already
has) are answered as usual but write nothing: no `.backup` rotation, no git
commit.

**Revisions.**

- Responses name the revision of what they describe in the `ETag` header:
  the list for `/api/tasks`, the task for `/api/tasks/{id}` (also in the
  answer to a `POST` or `PATCH`, so requests can be chained).
- Send a revision back as `If-Match` to make a changing request conditional.
  A request on one task takes the revision of that task (or of the list,
  which is stricter). A request on the list (`PUT`,
  `DELETE /api/tasks/done`, `POST`) takes the revision of the list.
- If it no longer matches (for a task also when the task is gone), the
  answer is `412 Precondition Failed` and nothing is written: re-read and
  retry.
- Without `If-Match` a request applies to whatever is there. Ids are reused,
  so a client that acts on a list it read a while ago should always send it.

```bash
etag=$(curl -s -o /dev/null -D - http://127.0.0.1:7272/api/tasks/3 | tr -d '\r' | awk 'tolower($1)=="etag:"{print $2}')
curl -X PATCH -H 'Content-Type: application/json' -H "If-Match: $etag" \
     -d '{"done": true}' http://127.0.0.1:7272/api/tasks/3
```

## rusk sync

Synchronizes the local database with a remote, in whole-file units with
conflict detection (no per-task merging).

```bash
rusk sync              # fast-forward in whichever direction changed
rusk sync push         # upload local tasks (refuses if the remote changed)
rusk sync pull         # download remote tasks (refuses if local changed)
rusk sync push --force # overwrite remote changes
rusk sync pull --force # discard local changes
```

`--force` needs only the side it copies from, so it also replaces a side
that no longer reads at all: a remote file that is not a task list, or a
local JSON or other file database that is damaged (kept as `.backup` when
`backup` is on). A SQLite file that is no database is refused, and a server
whose own database is damaged has to be mended there. A side that does read
and holds the same tasks already is not written again (its `.backup` stays),
and one that another writer changes meanwhile is not overwritten.

The remote comes from `sync_remote` in the config (or `RUSK_SYNC_REMOTE`):

```
# over SSH: uses your system ssh binary, keys, agent and ~/.ssh/config
sync_remote = user@vps:/srv/tasks/tasks.json

# over HTTP(S): talks to the API of a running `rusk serve`
sync_remote = https://tasks.example.com
sync_token = <the web_token of that server>
```

**After `rusk restore`** of a database that is synced with the configured
remote, rusk says what that means when the restored tasks are not what the
last sync left here: the next `rusk sync` takes them for a change made here
and sends them to the remote, unless the remote has changed too.
`rusk sync pull --force` takes the remote's tasks back instead.

### SSH remote

- Pull is a remote `cat`, fenced so that whatever the remote login shell
  prints of its own (a banner, an `echo` in an rc file) cannot end up in the
  database.
- Push streams the file to a temp sibling of its own (`<path>.<hex>.tmp`),
  checks that every byte arrived, keeps the previous content as
  `<path>.backup` and `mv`s the new one into place. A transfer that ends
  early leaves the remote file exactly as it was and takes its temp file
  with it.
- A remote symlink is followed, not replaced. A directory in place of the
  database is named instead of being filled.
- A `.csv` remote path is written in CSV, and a path ending in `/` means
  that directory's `tasks.json`.
- Both commands are one POSIX `sh` line with `--` before every path, so they
  survive a remote login shell of any kind (fish, csh/tcsh, dash, busybox)
  and a database whose name starts with a dash.
- Requires `ssh` in PATH; nothing needs to run on the server. If
  `rusk serve` runs there, it picks up pushed files immediately, since the
  server re-reads the database per request.

### HTTP remote

- `GET /api/tasks` and `PUT /api/tasks` via the system `curl`, 7.55 or
  newer (needed for TLS without adding heavyweight dependencies).
  `https://host/` and `https://host` are the same remote.
- The token reaches curl in a file only its owner can read, created for the
  call (in `$XDG_RUNTIME_DIR` when there is one, else the temp directory)
  and removed after it. Not on its command line, which `ps` shows every
  user. A Ctrl+C while curl runs leaves the file; the next request removes
  it.
- A `user:password@` in the URL is basic authentication (for a reverse proxy
  that asks for it). It reaches curl the same way, in a file, and rusk names
  the remote with the user alone: in what it prints, in the sync state, and
  in an error that quotes a value it refuses. With a token as well, the
  credentials go in `Authorization` for the proxy and the token in the
  session cookie for `rusk serve` behind it.
- curl reads no `~/.curlrc` (an `-o` or `-L` there would change what rusk
  reads back). A proxy is taken from the environment (`https_proxy`,
  `ALL_PROXY`, `NO_PROXY`), a CA bundle from `CURL_CA_BUNDLE` or
  `SSL_CERT_FILE`. Anything else (a client certificate, `resolve`) comes
  from a curl config file named in `RUSK_CURL_CONFIG` (`curl -K`; rusk's
  own options come after it and win).
- curl gives up on a connection that takes more than 30 s, on a transfer
  that moves nothing for 30 s, and on any transfer after 10 minutes. A slow
  one that keeps moving runs to its end within that.
- An answer larger than 64 MiB is refused rather than read whole; no task
  list is that big. Before the transfer when the server announces the
  length, and as it comes otherwise. A file over ssh the same, whatever the
  remote sends.
- A server that is its own database (its `rusk_db` names the address it
  serves on, or that of another server whose database it is) answers `508`
  with a message saying so, at once. Each request a server makes to reach
  its database names the servers it came through (`X-Rusk-Serve`).

### Conflict detection

After every successful sync, rusk stores next to the database
(`tasks.json.sync`) what each side held right after it.

- Each side is recorded as its format stores the list. A format that cannot
  hold everything (a Markdown text with a line that looks like a list item,
  a leading `!`) stores less than it was sent. rusk says so when that
  happens, and the next sync goes by what the side holds rather than
  bouncing the difference back.
- That is worked out as the list is written, not read back afterwards, so a
  change another writer makes right after the sync is a change for the next
  one. A server says what its database made of a `PUT`.
- If the state cannot be written, rusk warns: the next sync will not know
  about this one.

On the next sync each side is compared with what it held then:

| Situation | `rusk sync` | `push` / `pull` |
|---|---|---|
| local == remote, or neither changed | records base, done | no-op |
| only local changed | pushes | `push` works; `pull` refuses and names `push` |
| only remote changed | pulls | `pull` works; `push` refuses and names `pull` |
| both changed | refuses, names `push --force` / `pull --force` | require `--force` |
| no sync on record, one side has no tasks | seeds that side from the other | the matching one works |
| no sync on record, both have tasks that differ | refuses, names both `--force` ways and why there is no record | require `--force` |
| a fast-forward would empty a side holding tasks | refuses to empty it | require `--force` |

Empty and missing sides:

- A database file that is not there, and one the user really emptied, both
  come to sync as "no tasks". rusk never propagates that automatically:
  emptying a side that holds tasks always needs `--force`.
- The refusal says which of the two it is ("the local database at … does
  not exist" vs "… has no tasks"), because the answer to "where did my
  tasks go" differs.
- Only a path where nothing exists counts as missing. On an SSH remote the
  remote shell reports that with an exit status of its own, so a file that
  exists but cannot be read (no permission, a directory in its place) is an
  error, never an empty database.

Hashes are computed over the parsed, canonically re-encoded task list, so a
pretty-printed local JSON file, a compact HTTP body and a CSV file with the
same tasks all compare equal.
