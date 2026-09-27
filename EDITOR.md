<h1 align="center">Rusk Interactive Editor</h1>
<p align="center">TUI multi-line editor for task text</p>

<br />

- [Overview](#overview)
- [Launching](#launching)
- [Visual layout](#visual-layout)
- [Key bindings](#key-bindings)
  - [Save, cancel, help](#save-cancel-help)
  - [Navigation](#navigation)
  - [Selection](#selection)
  - [Editing](#editing)
  - [Clipboard and undo](#clipboard-and-undo)
  - [Mouse](#mouse)
- [Dirty-state confirmation](#dirty-state-confirmation)
- [Leaving the editor](#leaving-the-editor)
- [Draft autosave and recovery](#draft-autosave-and-recovery)
- [Task date header](#task-date-header)
- [Output after editing](#output-after-editing)

## Overview

`rusk edit <id>` opens the interactive multi-line editor with the task's current
text preloaded. The editor renders inside an alternate screen and exposes
selection, clipboard, undo/redo, word navigation, and crash-safe autosave.

## Launching

```bash
# Add a new task: full-screen editor, empty buffer (or restore a `new-task` draft).
rusk add

# Add with a due date on the first line pre-filled (same token rules as in `rusk add -d …`).
rusk add -d 2w

# Edit task text and due date (first line) interactively.
rusk edit 1

# Edit several tasks in one session (the editor opens once per id).
rusk edit 1,2,3
```

## Visual layout

```
31-12-2025 first line of the task text 
second line of the task text
third line …

                              ^S save · ^G help · Esc cancel    ●
```

- Top rows: the editable text, soft-wrapped to fit the terminal width. The
  wrap is word-aware: rows break at spaces so words stay whole; only a word
  longer than the text column is split mid-word. Width is counted in
  terminal cells, not characters - CJK and most emoji take two cells,
  combining marks none - and a row is only ever cut between grapheme
  clusters, so a flag, a family emoji or a letter with its accent stays
  whole. The cursor, vertical movement and mouse clicks use the same
  measure: a click lands in front of the character drawn under it.
- If the first line starts with a valid due-date token, it is shown in color
  (**green** for today or later, **red** if before today); other rows use
  equivalent-width indent so cursor math stays accurate.
- Tabs become four spaces on the way in (task text, pasted text), so what
  the editor draws is what it saves; a terminal would otherwise place a tab
  on its own tab stop, out of step with every other measurement. Other
  control characters (ESC, NUL, BEL, C1) are dropped on the way in: a pasted
  or stored `ESC[31m` would otherwise recolor the screen. Opening a task
  whose text holds a tab or a control character is not an edit by itself:
  saving it untouched leaves the stored text as it was.
- Arrow keys and Backspace/Delete step by character, not by grapheme
  cluster: inside a multi-character emoji the cursor column does not move
  until the whole cluster is crossed.
- Footer (last row): hotkey hint that adapts to the terminal width — shorter
  wordings on narrow terminals, never overlapped by the status glyph; a
  one-row terminal shows the text only.
- Arrows `↑` / `↓` at the bottom-left appear when the buffer scrolls.
- Status glyph at the bottom-right: `●` (dirty) / `○` (saved).

## Key bindings

### Save, cancel, help

| Key | Action |
|-----|--------|
| `Ctrl+S` | Save and exit. The draft goes once the database holds the text. |
| `Esc` | Cancel / skip task. Prompts when the buffer is dirty. |
| `Ctrl+G` or `F1` | Show the in-editor help overlay. |
| `Ctrl+C` | Copy selection if any (the selection stays); otherwise abort, keeping the buffer as a draft. |

### Navigation

| Key | Action |
|-----|--------|
| `←` / `→` | Move by character. |
| `Ctrl+←` / `Ctrl+→` | Jump by word: back to the start of the word before the cursor, forward to the start of the next one. |
| `↑` / `↓` | Move between visual (soft-wrapped) rows. |
| `Ctrl+↑` / `Ctrl+↓` | Move by 5 visual rows. |
| `Home` / `End` | Smart Home (first non-space, then col 0) / end of line. |
| `Ctrl+Home` / `Ctrl+End` | Buffer start / end. |
| `PageUp` / `PageDown` | Scroll one page; the cursor moves with the view. |
| `Ctrl+PageUp` / `Ctrl+PageDown` | Buffer start / end. |

### Selection

| Key | Action |
|-----|--------|
| `Shift+←` / `Shift+→` | Extend selection by character. |
| `Shift+Ctrl+←` / `Shift+Ctrl+→` | Extend selection by word. |
| `Shift+↑` / `Shift+↓` | Extend selection vertically. |
| `Shift+Home` / `Shift+End` | Extend to line start / end. |
| `Shift+Ctrl+Home` / `Shift+Ctrl+End` | Extend to buffer start / end. |
| `Ctrl+A` | Select the whole buffer. |

### Editing

| Key | Action |
|-----|--------|
| `Enter` | Insert newline (splits line at cursor; replaces selection). |
| `Tab` | Insert four spaces (replaces selection). |
| `Ctrl+R` | Restore the original task text (prefill). |
| `Backspace` | Delete left character / selection. |
| `Delete` | Delete right character / selection. |
| `Ctrl+W`, `Ctrl+Backspace` | Delete the word to the left (with the spaces between it and the cursor). |
| `Ctrl+Delete` | Delete word to the right. |
| `Ctrl+K` | Kill from cursor to end of line (or join with next line at EOL). |
| `Ctrl+Shift+K` | Delete the whole current line — where the terminal reports the `Shift` (kitty does, as `CSI 107;6u`; so do terminals in xterm's `CSI u` key format). Elsewhere the key arrives as `Ctrl+K` and kills to the end of the line, or — in xterm's default `modifyOtherKeys` format — does nothing. |
| `Ctrl+U` | Kill from beginning of line to cursor. |

A `Ctrl` or `Alt` shortcut that is not in these tables types nothing into
the text (on Windows `Ctrl+Alt` is also how `AltGr` arrives, so there it
types what the layout puts on that key). Typing always ends a selection,
even an empty one, so a capital right after `Ctrl+A` on an empty buffer is
kept.

### Clipboard and undo

| Key | Action |
|-----|--------|
| `Ctrl+C` | Copy selection to the system clipboard. |
| `Ctrl+X` | Cut selection to the system clipboard. |
| `Ctrl+V` | Paste from the system clipboard. |
| `Ctrl+Z` | Undo (consecutive single-char inserts collapse into one step; a key that changed nothing, such as `Backspace` at the start, is no step). |
| `Ctrl+Y` | Redo. |

Copy targets the system clipboard through
[`arboard`](https://crates.io/crates/arboard) (X11, Wayland, Windows, macOS)
and additionally emits an OSC 52 escape so the terminal stores the text
itself — this keeps the copy alive after rusk exits and works over SSH.
A process-local fallback covers environments where neither is available
(e.g. headless terminals).

### Mouse

| Gesture | Action |
|---------|--------|
| Left click | Move cursor; start selection. |
| Left drag | Extend selection. |
| Double-click | Select word under cursor. |
| Triple-click | Select the whole line. |
| `Shift` + click | Extend existing selection to the click point. |
| Middle click | Paste from the system clipboard at the click point. |
| Scroll wheel | Scroll the view by 3 visual rows. The cursor and the selection stay where they are (the cursor is hidden while out of sight); the next key brings the view back to it. |

## Dirty-state confirmation

Pressing `Esc` while the buffer differs from the original text (dirty state,
signalled by `●`) shows an overlay `Discard changes? [y/N]`. Answering `y`
discards changes, clears the autosave draft, and cancels the edit. `Ctrl+C`
in the overlay aborts the session and keeps the buffer as a draft. Any other
key returns to the editor.

## Leaving the editor

However the session ends, the terminal is given back: raw mode off, the
alternate screen left, mouse reporting and bracketed paste turned off, the
cursor shown. That holds for `Ctrl+S` and `Esc`, for an error inside the
editor, for a panic (the message is printed onto the restored screen, not
onto the one being torn down), and on unix for `SIGTERM`, `SIGHUP` and
`SIGQUIT` — those also write the buffer out as a draft and exit `128 +
signal` (143 for `SIGTERM`).

That holds with the help screen or the `Discard changes?` overlay up, too:
they hand control back to the editor's own loop rather than waiting for a
key that is never coming.

`rusk edit <id>` with no new text needs a terminal, exactly as `rusk add`
with no text does; into a pipe or a redirect it says so instead of painting
an editor nobody can see.

## Draft autosave and recovery

A draft is what the editor had in its buffer when it stopped without storing
anything: `Ctrl+C`, a `SIGTERM`, a save the database refused, a crash. It
exists so that the next edit of that task can offer the text back.

- **Where.** One file per task, next to the database:
  `editor-task-3.draft`, `editor-new-task.draft`. `$RUSK_DB` falls back to
  `./.rusk/` when not set (see the [main README](README.md#database-location)).
  For an http(s) or ssh database there is no local directory to use, so the
  drafts go to `$XDG_RUNTIME_DIR/rusk`, or `<temp>/rusk-<uid>` when there is
  no runtime directory — created `0700`, never a `/tmp/rusk` shared with
  everyone on the machine.
- **When.** Every ~3 seconds while the buffer differs from the stored task,
  and once more whenever the editor stops with something unsaved in it.
  Each autosave replaces the previous draft atomically (an interrupted write
  never destroys it) and the file is readable by its owner only (`0600`). If
  a draft cannot be written, rusk says so once the terminal is back. Clearing
  the buffer is an edit like any other: an empty draft is kept, not skipped.
- **Payload.** JSON with the task key, the identity of the text the draft was
  typed against, a timestamp, and the text:

  ```json
  { "key": "task-3", "base": "8f1c…", "text": "...", "timestamp": "2026-04-21T10:20:30+00:00" }
  ```

- **Removed when the text is somewhere safer**: once the database holds it,
  on a confirmed discard (`Esc` → `y`), and as soon as the buffer is brought
  back to the task's own text — there is nothing unsaved left to keep. A
  `rusk del` takes the draft of the deleted task with it.
- **`Ctrl+S` stores the task right away** — in `rusk edit 1,2,3` each task as
  soon as its editor is closed, so a later `Esc` or `Ctrl+C` does not take
  the earlier edits with it. If the save is refused or fails — another
  process changed or deleted the task while the editor was open, the database
  cannot be reached — nothing is overwritten and **the draft is still there**:
  run the command again, answer `y`, and merge your text with the task as it
  is now. Where not even the draft could be written (a read-only directory
  refuses the database and the draft alike), the error says so and carries
  your text instead, so it is on the screen rather than lost. "Edited task"
  is printed only once the task is stored.
- **Recovery.** When the editor is about to open a task that has a draft, it
  says how old the draft is and what is in it:

  ```
  Restore unsaved draft for task 3 (12 minutes ago, “buy oat milk and…”)? [y/N]:
  ```

  Answering `y` loads the draft into the buffer; the task's stored text stays
  the baseline, so `●` shows at once, `Esc` asks before discarding, and
  `Ctrl+R` goes back to what the task actually holds. Any other answer
  deletes the draft and opens the stored text.
- A draft is pinned to the text it was typed against, so one left over from a
  task that has since been deleted is never offered for the **new** task that
  reused its id.
- `rusk add -d <date>` keeps its date when a draft is restored: the draft
  holds text, not a due date, so the date is put back in front unless the
  draft already begins with one. A draft that begins with `_` (no date) gets
  the date in its place, and the words after it stay text.
- A draft that cannot be read is not thrown away — it is kept as
  `editor-task-3.draft.corrupt` and rusk says where it went, because the text
  inside it may still be readable by a human.

## Task date header

The due date (if any) is **only** the first whitespace-delimited token at the
**very start of the first line**. It may be an absolute date (`DD-MM-YYYY`,
slashes or dots ok, a two-digit year is 20xx, the month may be a name:
`11-jan-25`, `1-sept-2026`; years run from 1000 to 9999), the words
`today` / `tomorrow`, a relative
offset from today (`2d`, `2w`, `10d5w`,
`1m3q`, …; the months are added first, in one step, then the days), or a
relative offset from **this task's current due date** using a
leading `+` (`+2w`, `+10d5w`, …; if the task had no due date yet, `+` uses
today, same as `rusk add -d`). Use `_` as the only date token to clear
the deadline. A **recognized** token is **highlighted in color** on that line
(green for today or later, red if before today); text that does not parse as a
date is not colored. The following text are the body. A date alone on the
first line is fine: the text starts on the next one. Empty lines before and
after the text are not stored. A task due in a year outside 1000–9999 (a
file an older rusk wrote; see README → Database Formats) opens with that date as
its first token, and keeps it when you leave the token as it is.

A `_` word right after the date — or right after the `_` that is no date —
is dropped too, and the word after it stays text: `01-01-2027 _ 2d fix` is
due on 1 January with the text `2d fix`. So a text that starts with a word
that reads as a date (`tomorrow call mom`, `2d fix`) opens as
`_ tomorrow call mom` for a task without a due date, and as
`01-01-2027 _ tomorrow call mom` for one with a date: the word stays text
when you save it untouched, type a date in front, delete the date or replace
it with `_`. Delete the `_ ` together with its space and the word becomes the
due date, which the coloring shows before you save. (A text that starts with
a `_` word gets the mark too: `_ _ _ note`.)

`rusk edit <id> -d <date>` (with a value; same relative rules as this first-line
token, including leading `+`) sets the date without opening the TUI. **Bare** `-d` / `--date` (no value) is not supported;
use the TUI to edit the first line. The interactive buffer is still the
authoritative place for a due date: whatever appears as the first token on the
first line (or absent). For **`rusk add` with no text**, leading `+` on a
relative form still uses **today** (there is no existing task due date yet),
same as `rusk add -d`. Full syntax, including relative forms, is in the
in-editor help (`Ctrl+G` / `F1`).

## Output after editing

Each task that was saved, left unchanged or skipped gets a status line with
its id once its editor is closed, and when something was saved the task list
follows, as after a one-shot `rusk edit <id> <text>`. `Esc` on the last (or
only) task cancels: nothing more is printed, and what was saved before it
stays saved.

```
Edited task: 3
Task unchanged: 4
Skipped task: 5

  #  id    date       task
  ──────────────────────────
  ...
```
