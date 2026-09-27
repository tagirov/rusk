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
override them. The server re-reads the database on **every request**, so CLI
edits are instantly visible in the browser and vice versa — you can even
`rusk sync push` a new file under a running server.

### Authentication

With `web_token` set, the browser shows a token form once and keeps the
session in an `HttpOnly` cookie. Alternatives:

- `Authorization: Bearer <token>` — for curl and scripts.
- `http://host:port/?token=<token>` — bookmarkable on a phone; the token is
  moved into the cookie and stripped from the URL by a redirect.

Without a token, rusk **refuses to bind non-loopback addresses** — a CRUD API
over your tasks should not be world-writable by accident.

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
    location / { proxy_pass http://127.0.0.1:7272; }
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
or Bearer header. Dates are ISO `YYYY-MM-DD`.

| Method & path | Body | Result |
|---|---|---|
| `GET /api/tasks` | — | full task list |
| `GET /api/tasks/{id}` | — | one task |
| `POST /api/tasks` | `{"text": "...", "date": "2026-08-01" \| null}` | `201` + created task, listed in the place of its id |
| `PATCH /api/tasks/{id}` | any subset of `{text, date, done, priority}`; `"date": null` clears | `200` + updated task |
| `DELETE /api/tasks/{id}` | — | `204` |
| `DELETE /api/tasks/done` | — | `{"deleted": n}` |
| `PUT /api/tasks` | full task array | replaces the whole list (used by sync) |

A text is stored without whitespace at its edges, as the CLI stores it; a
new task takes the lowest free id and goes in front of the first task with a
higher one, so a reused id is not listed at the end.

`PUT` stores the list exactly as sent, so it has to follow the rules every
list read from a database is held to (README, "Notes on the text formats"):
each task has a text and an id of its own (nonzero), and `after` lists only
other tasks of the list, each once. A list that breaks one is refused with
`400` and says what is wrong (`not a valid task list: task 3 has no text`).
A rusk client makes its list follow these rules when it reads it; rusk 0.7.3
and older did not check texts and `after`, so their `rusk sync push` of such
a list fails until the client is updated.

Mutating requests must send `Content-Type: application/json` (CSRF guard).
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
  `curl` (needed for TLS without adding heavyweight dependencies).

### Conflict detection

After every successful sync, rusk stores a hash of the synced content next to
the database (`tasks.json.sync`). On the next sync it compares local, remote
and that base:

| Situation | `rusk sync` | `push` / `pull` |
|---|---|---|
| local == remote | records base, done | no-op |
| only local changed | pushes | `push` works, `pull` refuses |
| only remote changed | pulls | `pull` works, `push` refuses |
| both changed | refuses with instructions | require `--force` |
| one side has no tasks, the other does | refuses to empty the non-empty side | require `--force` |

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
