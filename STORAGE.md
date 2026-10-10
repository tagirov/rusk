# Storage

Where rusk keeps your tasks and what it does to keep them safe: the database
location, the file formats, remote and git-backed databases, backups,
concurrent writers and `rusk restore`. The [README](README.md#storage) has
the short version of each section.

- [Database location](#database-location)
  - [Debug builds](#debug-builds)
- [Database formats](#database-formats)
  - [SQLite](#sqlite)
  - [CSV](#csv)
  - [Markdown](#markdown)
  - [todo.txt](#todotxt)
  - [iCalendar](#icalendar)
  - [What every format is held to](#what-every-format-is-held-to)
- [Remote and git-backed databases](#remote-and-git-backed-databases)
  - [http(s)](#https)
  - [ssh](#ssh)
  - [git_backend](#git_backend)
- [Data safety](#data-safety)
  - [Automatic backups](#automatic-backups)
  - [Concurrent writers](#concurrent-writers)
  - [Restore](#restore)

## Database location

By default rusk stores tasks in `./.rusk/tasks.json`, relative to the
current directory, so every project has a task list of its own:

```bash
cd ~/projects/website
rusk add Fix responsive layout

cd ~/projects/api
rusk add Add authentication endpoint
```

The `RUSK_DB` environment variable or the `rusk_db` key in the
[configuration file](CONFIG.md) point elsewhere (`RUSK_DB` wins):

```bash
# Use a custom database file
export RUSK_DB="/path/to/your/db.json"

# Use a custom directory (tasks.json will be created inside)
export RUSK_DB="/path/to/your/project/"
```

Every shape the value may take (`~`, relative paths, URLs, ssh locations) is
listed in [CONFIG.md](CONFIG.md#database-location-rusk_db).

### Debug builds

In debug mode (`cargo run` or debug builds) rusk uses a temporary database,
so that your production data is never affected. It lives in a directory of
your own below the temp directory, since the temp directory itself is shared
with every user of the machine:

- Linux/macOS: `$TMPDIR/rusk-<uid>/debug/tasks.json` (usually
  `/tmp/rusk-1000/debug/tasks.json`)
- Windows: `%TEMP%\rusk-<user>\debug\tasks.json` (usually
  `C:\Users\<user>\AppData\Local\Temp\rusk-<user>\debug\tasks.json`)

On Unix `rusk-<uid>` is created so that only you can look inside (mode 700).
If something else is there under that name (a link, a file, a directory of
another user's), rusk refuses to use it and says so: remove it, or point
`TMPDIR` at a directory of your own. On Windows `%TEMP%` is yours already.

In debug mode the `RUSK_DB` environment variable is ignored, and the
database path is printed to the console when the program starts.

## Database formats

The format is chosen by the last extension of the database file name (JSON
by default; `notes.txt.json` is JSON):

```bash
export RUSK_DB="$HOME/tasks/tasks.csv"    # or rusk_db = ~/tasks/tasks.csv in the config
```

| Extension | Format | Interop | Feature |
|---|---|---|---|
| `.json` (or anything else) | pretty JSON | the default | built-in |
| `.csv` | RFC 4180, `id,text,date,done,priority,after` schema | LibreOffice / Excel / Google Sheets | built-in |
| `.md` / `.markdown` | GitHub-style task list | GitHub, Obsidian, any editor | `fmt-markdown` |
| `.txt` | [todo.txt](http://todotxt.org) | todo.sh, Simpletask, … | `fmt-todotxt` |
| `.ndjson` / `.jsonl` | one JSON task per line | git diffs, `grep`, `jq` | `fmt-ndjson` |
| `.ics` | iCalendar VTODO | Thunderbird, Nextcloud Tasks, Apple Reminders | `fmt-ics` |
| `.db` / `.sqlite` / `.sqlite3` | SQLite | concurrent writers, SQL tooling | `backend-sqlite` |

All features except `backend-sqlite` (which bundles a C library) are enabled
by default. Distro builds can trim them (`--no-default-features --features …`).

Backups and atomic writes work the same in every file format
(`tasks.csv.backup`). SQLite gets the same `.backup` copy per save, made by
SQLite itself, so it holds what a WAL holds too.

### SQLite

- Concurrent commands serialize on the database's own write lock. Each one
  re-reads the table inside its transaction when another process wrote
  after it loaded, and applies its change to that (see
  [Concurrent writers](#concurrent-writers)).
- Reading never changes the file: a write-protected database is listed
  fine.
- A database of an older rusk is brought up to date by the first save. That
  save also marks the file as rusk's in its header (`application_id`, and
  the schema in `user_version`).
- A SQLite file of another program is refused rather than written to: one
  with tables but no `tasks` table, or one its header marks as another
  program's. So is one a newer rusk wrote in a later schema.
- Deleted tasks are overwritten in the file (`secure_delete`), and a save
  that frees much of the file compacts it.

### CSV

- Multiline task text, commas and quotes are handled. Dates are ISO
  `YYYY-MM-DD`. The `after` cell takes ids separated by spaces or commas.
- A row typed in with an empty id cell is a new task. Rows of empty cells
  are skipped.
- LibreOffice edits the file in place and keeps ISO dates as they are.
- Google Sheets can import it, but edits made in Sheets have to be exported
  back manually.
- Excel opens it, but its "Save as CSV" may write the dates in the locale's
  format and separate the cells with `;` in locales that use it. The file is
  then not a rusk database any more. The file carries no byte order mark, so
  Excel may show non-ASCII text garbled unless the file is imported rather
  than opened. (Not verified in Excel itself.)

### Markdown

- An item is `- [x] text @2026-07-15 <!-- id:3 -->`. A leading `!` marks
  priority. Continuation lines are indented by two spaces (or a tab).
- Items added by hand without the id comment get the lowest free id on the
  next run.
- Numbered items (`1. [ ] …`) are tasks too. So are the other one-character
  checkbox marks of Obsidian and its themes: `[-]` (cancelled) reads as
  done; `[/]`, `[>]`, `[?]`, `[b]` and the rest as not done. Every task is
  written back as `- [ ]` or `- [x]`.
- Only items at the start of a line count. An indented line is a
  continuation of the task above unless it is a `- [ ]`/`- [x]` item.
- rusk owns the file: headers and prose around the list are dropped on save.
- A text comes back as it was, with two exceptions: a CRLF line break inside
  it comes back as LF, and a first line that is nothing but spaces and a
  date token loses the spaces. A text that looks like the markup does not
  survive either: a first line that starts with `! ` or ends with
  ` @YYYY-MM-DD`, or a later line that is a `- [ ]` item.

### todo.txt

- A line is `x (A) text due:2026-07-15 id:3`. Projects and contexts stay in
  the task text. Newlines are stored as a literal `\n`.
- A word of the text that would read as metadata is written with a `\` in
  front, so the text comes back as it was: `x`, `(B)` or a date as the first
  word, a `due:`/`id:`/`after:` tag anywhere (`\x marks the spot`).
- rusk has one priority flag. Any priority `(A)`–`(Z)` reads as priority and
  is written back as `(A)`.
- Completion and creation dates (`x 2026-07-09 2026-07-01 …`) are dropped.

### iCalendar

- One VTODO per task. Foreign components (VEVENT, VALARM), properties and
  relations are ignored and not preserved.
- `STATUS:COMPLETED` or `CANCELLED` is done. Without a `STATUS`, so is a
  `COMPLETED` time or `PERCENT-COMPLETE:100`.
- A dependency is `RELATED-TO;RELTYPE=DEPENDS-ON` (or `FINISHTOSTART`). A
  `RELATED-TO` without `RELTYPE` is a dependency only when it names a UID of
  the form rusk wrote up to 0.7.3 (`rusk-3@rusk`). Otherwise it is the parent
  of a subtask.
- A save keeps the UID rusk gave each task and, while the task is unchanged,
  its `DTSTAMP`. A calendar client therefore sees only what changed as
  changed.
- A task new to the file gets a UID made of its id and its text
  (`rusk-3@<hash>.rusk`), so a new task that got the id of a deleted one has
  a UID of its own. The exception is one save that does both, as a
  `rusk sync` that brings both changes can: the file knows tasks by id only,
  and the new task is then an edit of the old one.
- A task that came with the UID of another client gets a rusk UID on the
  first save, which pins its id.
- A text has no carriage returns in iCalendar: a CRLF line break comes back
  as LF.

### What every format is held to

Every format can be edited by hand or by another tool. Whatever rusk reads
is held to the same rules before anything uses it:

- **A task needs a text.** An item with neither text nor id (an empty
  `- [ ]`, a todo.txt line of metadata only, a VTODO without SUMMARY) is no
  task: it is skipped with a warning, and the next save leaves it out. A
  task that has an id but no text (`{"id": 2, "text": ""}`) keeps its date,
  flags and dependents and shows as `(no text)`, with a warning, so that
  `rusk edit 2` or `rusk del 2` can take care of it.
- **Every task has an id of its own.** One without an id (or with id 0)
  gets the lowest free id. One that repeats the id of an earlier task gets a
  new id, and a warning names it. `rusk list` shows the ids the next save
  writes.
- **A dependency names a task that exists.** A dependency on a task that
  does not exist is dropped with a warning, so it can never come to mean the
  next task that gets that id. For the same reason `after` can only name
  ids the file already gives: an item added without an id has none until
  the next save. A dependency on the task itself, or one listed twice, is
  dropped too.
- **A due date has a year from 1000 to 9999.** One outside that (a typo an
  older rusk took, `1-1-205`) is read as it is, shown with all its digits
  (`1-jan-0205`) and named in a warning, so that `rusk edit 2 -d <date>` or
  `-d _` can correct it. `rusk serve` does not take a whole list with one
  (`PUT`, as `rusk sync push` sends), as it takes no list a load would have
  to repair.
- In JSON only `text` is required: any other field may be left out or
  `null`.

The file itself changes only with the next save, and the `.backup` keeps
what that save replaced.

## Remote and git-backed databases

The database does not have to be a local file. The *shape* of `rusk_db`
picks the storage backend:

```bash
rusk_db = https://tasks.example.com     # the API of a running `rusk serve` (feature backend-http)
db_token = s3cret                       # its web_token, if set (or RUSK_DB_TOKEN)

rusk_db = alex@vps:/srv/tasks/tasks.md  # a file over ssh (feature backend-ssh)

git_backend = true                      # commit every save of a local file database
                                        # to a git repo in its directory (feature backend-git)
```

### http(s)

A thin client: there is no local copy at all, every command reads and writes
through the server.

- The list comes with a revision (`ETag`) and is saved back with
  `If-Match`, so a command never overwrites what another machine, the web
  UI or a cron script stored in the meantime: it fetches the list again and
  applies its change to that. Both sides need a rusk with this feature; an
  older server is written unconditionally.
- Requires the network and the server to be up. For offline-first use
  `rusk sync` instead.
- A `user:password@` in the URL is basic authentication for a reverse proxy
  that asks for it. The password never shows in what rusk prints: curl gets
  it in a file. curl reads no `~/.curlrc`; proxy settings come from the
  environment and a client certificate from `RUSK_CURL_CONFIG` (see
  [WEB.md](WEB.md#rusk-sync)).
- A `rusk serve` whose `rusk_db` names its own address, or a server whose
  database it is, says so on the first request instead of waiting on
  itself.
- An answer larger than 64 MiB is refused rather than read whole; no task
  list is that big. The same holds over ssh.

### ssh

Reads and writes the remote file over the system `ssh`: keys, agent and
`~/.ssh/config` apply.

- The location is `[user@]host:path` as `scp` reads it. `~/` is the remote
  home directory; IPv6 goes in brackets (`user@[::1]:/srv/tasks.json`).
- The remote extension picks the format. SQLite is local only.
- Writes are atomic (temp + `mv`).
- The file is loaded and saved per command: on flaky links prefer
  `rusk sync`.
- There is no lock on the remote side. After an editor or a prompt the file
  is read again and compared before it is replaced, which leaves a window of
  one round trip.

### git_backend

Gives full history (`git log`, `git revert`) beyond the single `.backup`
copy.

- An existing enclosing repository is used as it is, with git's own identity
  for it (`rusk <rusk@localhost>` stands in only for what git has not got).
  Otherwise a repository is initialized in the database directory with a
  `.gitignore` for the auxiliary files, and rusk says so.
- A repository is never initialized in your home directory, the filesystem
  root or a directory other users can write to (the temp directory). Nor
  when git cannot use a repository it finds above (another owner's, for
  one): that is reported instead.
- The auxiliary files (`.backup`, `.lock`, drafts, …) are kept out of
  `git status` through the repository's `.git/info/exclude`.
- No hook runs: neither a hook script of the repository nor a hook its
  config defines. rusk does not run git at all in a repository other users
  can change, since its config could run anything (a filter,
  `gpg.program`). In your own repository git runs with its config, as
  `git commit` there would.
- A symlinked database is committed where the file is.
- Works on a database file on this machine: not SQLite, not a remote one
  (rusk says so). Uses the system `git`, 2.9 or newer.

## Data safety

### Automatic backups

- Every save first copies the previous state to a `.backup` sibling
  (`tasks.json.backup`), as private as the database itself. `backup = false`
  turns this off.
- Backups are stored in the same directory as your database. That includes
  an ssh database or sync remote, where the copy is made on the other side
  (`rusk restore` cannot reach it, and says where it is).
- Every file is replaced atomically: written to a uniquely named temp file,
  fsynced and renamed into place. A crash, a full disk or a parallel `rusk`
  leaves the old content or the new one, never a truncated file. The file
  mode (e.g. `0600`) and a symlinked database path survive the save.
- An empty (zero-byte) database file is never copied over an existing
  backup.
- A database that cannot be read is never taken for an empty one: no
  permission, a directory in its place, a symlink whose target is gone,
  content that is not UTF-8. The command stops instead of starting from
  zero tasks and saving that over your data. The same over SSH.
- Only a file that is not there at all is a database yet to be created. A
  file that holds nothing loads as zero tasks with a warning, except in the
  formats that write an empty database as an empty file (Markdown,
  todo.txt, NDJSON).
- A UTF-8 BOM in front of the file, as a Windows editor saves it, is not
  corruption.

### Concurrent writers

- Commands running at the same time (two terminals, a cron job,
  `rusk serve`) all take effect. Every change is applied to the database as
  it is at the moment of the save, never to the copy the command loaded
  when it started.
- Saves of a file database take turns on a writer lock, `tasks.json.lock`,
  an empty file that stays next to the database.
- A command that changes nothing writes nothing (no backup rotation
  either).
- `rusk edit` in the editor saves each task as soon as you press Ctrl+S. If
  another process changed or deleted that task while the editor was open,
  nothing is overwritten: the command stops and your text is kept as a
  draft, offered again by the next `rusk edit <id>`. If not even the draft
  can be written (a read-only directory), the text is printed with the
  error instead.
- `rusk del` deletes what you confirmed. A task that was changed, or deleted
  and its id reused, while the prompt was waiting is reported instead.
- Anything else that writes the file (an editor, a sync tool) is noticed as
  well, but without the lock there is a window of a few milliseconds.

### Restore

```bash
rusk restore    # put the automatic backup back in place
```

What it does, in order:

1. Validates the backup file. An empty file, or one that is not a task
   list, is refused.
2. Copies the current database, byte for byte, to `.before_restore`, or to
   `.before_restore.1`, `.2`, … so that an earlier copy is never
   overwritten. A SQLite database that opens is copied by SQLite, WAL
   included.
3. Puts the backup in place of the database: atomically and byte for byte
   (with `git_backend` as a commit of its own); SQLite inside a transaction.

`restore` shows when the backup was saved, and warns when it is much older
than the database (`backup = false`, or the file was edited by hand).

For a database synced with the configured remote it also says what the next
`rusk sync` will do when the restored tasks are not what the last sync left:
send them there, as a change made here. `rusk sync pull --force` takes the
remote's tasks back instead.
