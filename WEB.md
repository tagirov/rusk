# Web UI and sync

rusk ships a mobile-first web frontend built from a single embedded HTML
template — no node toolchain, no build step. Two commands share it:

- **`rusk gen`** renders a self-contained, read-only `index.html` with your
  tasks embedded. Host it anywhere static files go.
- **`rusk serve`** runs a small built-in HTTP server with the same UI plus a
  JSON API: add, edit, complete, prioritize and delete tasks from a phone.

Theme colors come from the configuration file ([CONFIG.md](CONFIG.md)) as CSS
variables, so the page matches your terminal theme. Light/dark surfaces follow
the system `prefers-color-scheme`.

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

Defaults come from the config (`web_host`, `web_port`, `web_token`); flags
override them. `localhost` (in any case) binds 127.0.0.1, where clients look
for it; an IPv6 address may be given with or without brackets (`[::1]`).
The server re-reads the database on **every request**, so CLI
edits are instantly visible in the browser and vice versa — you can even
`rusk sync push` a new file under a running server.

Each request is read, checked and answered on a thread of its own, so a
client that sends its request body slowly, or never, holds up nobody else;
the work on the tasks is done by one request at a time, so two changes made
at the same moment are both kept, whatever the database. A request body may
be up to 32 MiB (a `PUT` of thousands of tasks is a few MB), the sign-in form
up to 4 KiB; a bigger one is refused with 413, and one that ends before the
length it announced with 400. `HEAD` is answered like `GET`, without the
body, for uptime checks and proxies. If the server cannot take connections
any more (no file descriptors left), it ends with an error, so that a
supervisor (`Restart=on-failure`) starts it again.

`rusk serve` runs on [tiny_http](https://github.com/tiny-http/tiny-http),
which is not made to face hostile clients: every open connection costs a
thread, a request answered without reading its body (a wrong token, a body
too big) takes the rest of that body from the connection before it goes —
the big ones one at a time, so they cannot pile up in memory — and a request
that announces a body larger than the machine could allocate
(`Content-Length: 1000000000000000`) aborts the server. Keep it on loopback
or a trusted network, or put a reverse proxy in front that buffers requests
and limits their size (nginx does both, see below).

### Authentication

With `web_token` set, the browser shows a token form once and keeps the
session in an `HttpOnly` cookie (`SameSite=Strict`, one year; `Secure` when
the request came over TLS — a reverse proxy says so in
`X-Forwarded-Proto: https` or `Forwarded: proto=https`; of a chain of
proxies the first entry counts). **Sign out** in the page header drops the
cookie (a `POST /logout` with `Content-Type: application/json`, like every
change, so that no other site can sign you out). The cookie holds the token
itself: to take the access of a copied cookie away, change `web_token`.
Alternatives:

- `Authorization: Bearer <token>` (the scheme in any case) — for curl and
  scripts. A request is let in when any token it presents matches, so a
  stale cookie does not hide a valid Bearer token.
- `http://host:port/?token=<token>` — bookmarkable on a phone; the token is
  moved into the cookie and stripped from the URL by a redirect. A `+` in
  the address is a `+`; a `%`, `&` or `#` in the token has to be written
  `%25`, `%26`, `%23` (base64 tokens have none). A wrong or old token gets
  the sign-in form — or, when the browser is signed in anyway, the page.

The token travels as it is in a cookie and an HTTP header, so it may hold
printable ASCII only (spaces inside it are fine), without a `;` and without
spaces at its ends; `rusk serve` refuses to start with any other
(`openssl rand -base64 24` makes a good one).

Without a token, rusk **refuses to bind non-loopback addresses** (all of
127.0.0.0/8, `::1`, `localhost`) — a CRUD API over your tasks should not be
world-writable by accident — and answers only requests addressed to
`localhost` or a loopback address (the `Host` header): a web page that points
a name of its own at 127.0.0.1 (DNS rebinding) is refused with 403, and a
request with two `Host` headers with 400. Behind **any** reverse proxy, set
`web_token`: nginx, for one, passes `Host: 127.0.0.1:7272` on by default, and
the whole internet would be "loopback" to a server without a token.

Every page is sent with `X-Frame-Options: DENY` and
`Content-Security-Policy: frame-ancestors 'none'`: no other site can show it
in a frame and lure clicks onto your tasks.

### Deployment on a VPS

`rusk serve` speaks plain HTTP; put a TLS reverse proxy in front for anything
beyond localhost or a trusted LAN. Caddy makes it two lines:

```
tasks.example.com {
    reverse_proxy 127.0.0.1:7272
}
```

nginx equivalent:

```nginx
server {
    listen 443 ssl;
    server_name tasks.example.com;
    # ssl_certificate ...; ssl_certificate_key ...;
    client_max_body_size 32m;   # nginx's own default, 1m, would cap `sync push`
    location / {
        proxy_pass http://127.0.0.1:7272;
        proxy_set_header X-Forwarded-Proto $scheme;   # the cookie gets `Secure`
    }
}
```

A zero-config alternative is [Tailscale](https://tailscale.com): bind
`rusk serve` to the tailnet address and skip the proxy entirely.

The CLI and the web UI can be used side by side: every request, like every
CLI command, applies its change to the database as it is at that moment (a
file database is written under a lock, `tasks.json.lock`), so neither
overwrites the other. The page additionally tells the server which state of
a task it is showing (see `If-Match` below): if that task was changed
elsewhere since — or deleted and its id given to a new task, so row 3 is a
different task by now — the change is refused, the list reloads and you
repeat it on what is actually there. Changes to other tasks do not get in
the way. The automatic `.backup` copy always keeps the previous state.

The edit dialog closes only once the server has taken the change. Until
then what you typed stays in it — the dialog cannot be closed while a Save
or Delete is on its way (a request is given 30 s) — and a lost connection or
a refused change is reported inside the dialog, so Save can simply be
pressed again. When the task was changed elsewhere in the meantime, the
dialog says what is different now (its text above all — it may be another
task under a reused id), takes over the fields you did not touch and keeps
the ones you did; after a moment, Save again applies them on top of the
other change. Save sends only the fields you changed, and nothing at all
when you changed none. Errors outside the dialog show in a banner at the
top, which stays until a change made after it goes through; its Reload
button re-reads the list. If the token stops being accepted (it was changed
in the config), the page shows the sign-in form right away when nothing is
typed on it; otherwise it says so in the banner and leaves reloading to you.

### API

All endpoints return JSON and require the token (when configured) via cookie
or Bearer header. Dates are ISO `YYYY-MM-DD`, in the years 1000–9999. A body
that is not JSON is a `400` "invalid JSON"; one with a value a task can not
take is a `400` that names it (`invalid task: date '2d' is not written
YYYY-MM-DD at line 1 column 25`).

| Method & path | Body | Result |
|---|---|---|
| `GET /api/tasks` | — | full task list |
| `GET /api/tasks/{id}` | — | one task |
| `POST /api/tasks` | `{"text": "...", "date": "2026-08-01" \| null, "after": [1, 2]}` (`date` and `after` optional) | `201` + created task, listed in the place of its id |
| `PATCH /api/tasks/{id}` | any subset of `{text, date, done, priority, after}`; `"date": null` clears, `after` replaces the whole list (`[]` clears; the same ids in another order are no change) | `200` + updated task |
| `DELETE /api/tasks/{id}` | — | `204` |
| `DELETE /api/tasks/done` | — | `{"deleted": n}` |
| `PUT /api/tasks` | full task array | replaces the whole list (used by sync); `{"count": n, "tasks": [...]}`, the list as the server's database holds it |

A task is sent as the JSON format stores it: `id`, `text`, `date`
(`YYYY-MM-DD` or `null`), `done`, `priority` and `after`, the ids of the tasks
it waits for (`--after`; left out when there are none). `after` may name
only other tasks that exist and must not close a loop (a task that waits for
itself, or for one that waits for it); else a `400` says what is wrong. A
text is stored without whitespace at its edges, as the
CLI stores it; a new task takes the lowest free id and goes in front of the
first task with a higher one, so a reused id is not listed at the end.

`PUT` stores the list exactly as sent, so it has to follow the rules every
list read from a database is held to (README, "Notes on the text formats"):
each task has a text and an id of its own (nonzero), and `after` lists only
other tasks of the list, each once. A list that breaks one is refused with
`400` and says what is wrong (`not a valid task list: task 3 has no text`).
A rusk client makes its list follow these rules when it reads it; rusk 0.7.3
and older did not check texts and `after`, so their `rusk sync push` of such
a list fails until the client is updated.

Mutating requests must send `Content-Type: application/json` (CSRF guard;
the media type is compared without regard to case, parameters such as
`; charset=utf-8` aside).
A request that changes nothing (`PATCH` with the values the task already
has) is answered as usual but writes nothing: no `.backup` rotation, no git
commit. Errors come as `{"error": "..."}` with a 4xx/5xx status; the message
includes the cause (`Failed to write the database file '...': ... (os error
28)`), not only the step that failed.

Responses name the revision of what they describe in the `ETag` header: the
list for `/api/tasks`, the task for `/api/tasks/{id}` (also in the answer to
a `POST` or `PATCH`, so requests can be chained). Send a revision back as
`If-Match` to make a changing request conditional: a request on one task
takes the revision of that task (or of the list, which is stricter), a
request on the list (`PUT`, `DELETE /api/tasks/done`, `POST`) the revision
of the list. If it no longer matches — for a task also when the task is
gone — the answer is `412 Precondition Failed` and nothing is written:
re-read and retry. Without `If-Match` a request applies to whatever is
there. Ids are reused, so a client that acts on a list it read a while ago
should always send it.

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

`--force` needs only the side it copies from, so it also replaces a side that
no longer reads at all: a remote file that is not a task list, a local JSON
or other file database that is damaged (kept as `.backup` when `backup` is
on). A SQLite file that is no database is refused, and a server whose own
database is damaged has to be mended there. A side that does read and holds
the same tasks already is not written again (its `.backup` stays), and one
that another writer changes meanwhile is not overwritten.

The remote comes from `sync_remote` in the config (or `RUSK_SYNC_REMOTE`):

```
# over SSH — uses your system ssh binary, keys, agent and ~/.ssh/config
sync_remote = user@vps:/srv/tasks/tasks.json

# over HTTP(S) — talks to the API of a running `rusk serve`
sync_remote = https://tasks.example.com
sync_token = <the web_token of that server>
```

- **SSH remote**: pull is a remote `cat`, fenced so that whatever the remote
  login shell prints of its own — a banner, an `echo` in an rc file — cannot
  end up in the database. Push streams the file to a temp sibling of its own
  (`<path>.<hex>.tmp`), checks that every byte arrived, keeps the previous
  content as `<path>.backup` and `mv`s the new one into place; a transfer
  that ends early leaves the remote file exactly as it was and takes its
  temp file with it. A remote symlink is followed, not replaced, and a
  directory in place of the database is named instead of being filled. A
  `.csv` remote path is written in CSV, and a path ending in `/` means that
  directory's `tasks.json`. Both commands are one POSIX `sh` line with `--`
  before every path, so they survive a remote login shell of any kind (fish,
  csh/tcsh, dash, busybox) and a database whose name starts with a dash.
  Requires `ssh` in PATH; nothing needs to run on
  the server. If `rusk serve` runs there, it picks up pushed files
  immediately (the server re-reads the database per request).
- **HTTP remote**: `GET /api/tasks` and `PUT /api/tasks` via the system
  `curl` 7.55 or newer (needed for TLS without adding heavyweight
  dependencies). The token
  reaches curl in a file only its owner can read, created for the call (in
  `$XDG_RUNTIME_DIR` when there is one, else the temp directory) and removed
  after it — not on its command line, which `ps` shows every user. A Ctrl+C
  while curl runs leaves the file; the next request removes it.
  `https://host/` and `https://host` are the same remote.

### Conflict detection

After every successful sync, rusk stores next to the database
(`tasks.json.sync`) what each side held right after it — each side as its
format stores the list, because a format that cannot hold everything (a
Markdown text with a line that looks like a list item, a leading `!`) stores
less than it was sent; rusk says so when that happens, and the next sync goes
by what the side holds rather than bouncing the difference back. That is
worked out as the list is written, not read back afterwards: a change
another writer makes right after the sync is a change for the next one. A
server says what its database made of a `PUT`. If the state cannot be
written, rusk warns: the next sync will not know about this one. On the next
sync each side is compared with what it held then:

| Situation | `rusk sync` | `push` / `pull` |
|---|---|---|
| local == remote, or neither changed | records base, done | no-op |
| only local changed | pushes | `push` works; `pull` refuses and names `push` |
| only remote changed | pulls | `pull` works; `push` refuses and names `pull` |
| both changed | refuses, names `push --force` / `pull --force` | require `--force` |
| no sync on record, one side has no tasks | seeds that side from the other | the matching one works |
| no sync on record, both have tasks that differ | refuses, names both `--force` ways and why there is no record | require `--force` |
| a fast-forward would empty a side holding tasks | refuses to empty it | require `--force` |

A database file that is not there, and one the user really emptied, both
come to sync as "no tasks". rusk never propagates that automatically:
emptying a side that holds tasks always needs `--force`, and the refusal
says which of the two it is ("the local database at … does not exist" vs
"… has no tasks"), because the answer to "where did my tasks go" differs.
Only a path where nothing exists counts as missing — on an SSH remote the
remote shell reports that with an exit status of its own, so a file that
exists but cannot be read (no permission, a directory in its place) is an
error, never an empty database.

Hashes are computed over the parsed, canonically re-encoded task list, so a
pretty-printed local JSON file, a compact HTTP body and a CSV file with the
same tasks all compare equal.
