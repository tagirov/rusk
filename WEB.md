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

Known limit: the CLI and the web UI write the whole file last-writer-wins; a
CLI save landing in the same millisecond as a web save can drop the other
one's change. The automatic `.backup` copy always keeps the previous state.

### API

All endpoints return JSON and require the token (when configured) via cookie
or Bearer header. Dates are ISO `YYYY-MM-DD`.

| Method & path | Body | Result |
|---|---|---|
| `GET /api/tasks` | — | full task list |
| `POST /api/tasks` | `{"text": "...", "date": "2026-08-01" \| null}` | `201` + created task |
| `PATCH /api/tasks/{id}` | any subset of `{text, date, done, priority}`; `"date": null` clears | `200` + updated task |
| `DELETE /api/tasks/{id}` | — | `204` |
| `DELETE /api/tasks/done` | — | `{"deleted": n}` |
| `PUT /api/tasks` | full task array | replaces the whole list (used by sync) |

Mutating requests must send `Content-Type: application/json` (CSRF guard).

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

- **SSH remote**: pull is a remote `cat`; push streams the file to
  `<path>.tmp` and `mv`s it into place (atomic replace). A `.csv` remote path
  is written in CSV. Requires `ssh` in PATH; nothing needs to run on the
  server. If `rusk serve` runs there, it picks up pushed files immediately
  (the server re-reads the database per request).
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

Hashes are computed over the parsed, canonically re-encoded task list, so a
pretty-printed local JSON file, a compact HTTP body and a CSV file with the
same tasks all compare equal.
