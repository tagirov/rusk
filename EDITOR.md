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
  - [The `_` marker](#the-_-marker)
  - [Dates on the command line](#dates-on-the-command-line)
- [Output after editing](#output-after-editing)

## Overview

`rusk edit <id>` opens the interactive multi-line editor with the task's
current text preloaded. The editor renders inside an alternate screen and
offers selection, clipboard, undo/redo, word navigation, and crash-safe
autosave.

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

**Text rows**

- The text is soft-wrapped to the terminal width. The wrap is word-aware:
  rows break at spaces so words stay whole. Only a word longer than the text
  column is split mid-word.
- Width is counted in terminal cells, not characters. CJK and most emoji
  take two cells, combining marks none. A row is only ever cut between
  grapheme clusters, so a flag, a family emoji or a letter with its accent
  stays whole.
- The cursor, vertical movement and mouse clicks use the same measure: a
  click lands in front of the character drawn under it.
- Arrow keys and Backspace/Delete step by character, not by grapheme
  cluster. Inside a multi-character emoji the cursor column does not move
  until the whole cluster is crossed.

**Date header**

- If the first line starts with a valid due-date token, it is shown in
  color: **green** for today or later, **red** if before today.
- The other rows are indented by the same width, so cursor math stays
  accurate.

**What goes into the buffer**

- Tabs become four spaces on the way in (task text, pasted text), so what
  the editor draws is what it saves. A terminal would place a tab on its own
  tab stop, out of step with every other measurement.
- Other control characters (ESC, NUL, BEL, C1) are dropped on the way in. A
  pasted or stored `ESC[31m` would otherwise recolor the screen.
- Opening a task whose text holds a tab or a control character is not an
  edit by itself: saving it untouched leaves the stored text as it was.

**Footer and status**

- The footer is a hotkey hint that adapts to the terminal width: shorter
  wordings on narrow terminals, never overlapped by the status glyph. A
  one-row terminal shows the text only.
- On a wide and tall terminal the footer stays on a fixed row a few rows
  above the bottom. On a narrow or short one it follows the text, a couple of
  rows under its last line, and goes no lower than the last row.
- Arrows `↑` / `↓` at the bottom-left appear when the buffer scrolls.
- The status glyph at the bottom-right is `●` (unsaved changes) or `○`
  (saved).

## Key bindings

### Save, cancel, help

| Key | Action |
|-----|--------|
| `Ctrl+S` | Save and exit. The draft goes once the database holds the text. |
| `Esc` | Cancel / skip task. Prompts when the buffer is dirty. |
| `Ctrl+G` or `F1` | Show the in-editor help overlay. |
| `Ctrl+C` | Copy the selection if there is one (the selection stays). Otherwise abort, keeping the buffer as a draft. |

### Navigation

| Key | Action |
|-----|--------|
| `←` / `→` | Move by character. |
| `Ctrl+←` / `Ctrl+→` | Jump by word: back to the start of the word before the cursor, forward to the start of the next one. |
| `↑` / `↓` | Move between visual (soft-wrapped) rows. |
| `Ctrl+↑` / `Ctrl+↓` | Move by 5 visual rows. |
| `Home` / `End` | Smart Home (first non-space, then column 0) / end of line. |
| `Ctrl+Home` / `Ctrl+End` | Buffer start / end. |
| `PageUp` / `PageDown` | Scroll one page; the cursor moves with the view. |
| `Ctrl+PageUp` / `Ctrl+PageDown` | Buffer start / end. |

### Selection

| Key | Action |
|-----|--------|
| `Shift+←` / `Shift+→` | Extend the selection by character. |
| `Shift+Ctrl+←` / `Shift+Ctrl+→` | Extend the selection by word. |
| `Shift+↑` / `Shift+↓` | Extend the selection vertically. |
| `Shift+Home` / `Shift+End` | Extend to line start / end. |
| `Shift+Ctrl+Home` / `Shift+Ctrl+End` | Extend to buffer start / end. |
| `Ctrl+A` | Select the whole buffer. |

The selection is drawn in reverse video, with colors off (`NO_COLOR`) as
well. An empty line inside it shows one selected cell.

### Editing

| Key | Action |
|-----|--------|
| `Enter` | Insert a newline (splits the line at the cursor; replaces the selection). |
| `Tab` | Insert four spaces (replaces the selection). |
| `Ctrl+R` | Restore the original task text (prefill). |
| `Backspace` | Delete the character to the left, or the selection. |
| `Delete` | Delete the character to the right, or the selection. |
| `Ctrl+W`, `Ctrl+Backspace` | Delete the word to the left, with the spaces between it and the cursor. |
| `Ctrl+Delete` | Delete the word to the right. |
| `Ctrl+K` | Kill from the cursor to the end of the line, or join with the next line at the end. With a selection, delete the selection. |
| `Ctrl+Shift+K` | Delete the whole current line. With a selection, every line it touches (not the line it ends at the start of). See the note on terminals below. |
| `Ctrl+U` | Kill from the beginning of the line to the cursor. With a selection, delete the selection. |

**`Ctrl+Shift+K` and terminals.** The key works where the terminal reports
the `Shift`: kitty does (as `CSI 107;6u`), and so do terminals in xterm's
`CSI u` key format. Elsewhere the key arrives as `Ctrl+K`. In xterm's default
`modifyOtherKeys` format it does nothing.

**Unbound shortcuts.** A `Ctrl` or `Alt` shortcut that is not in these tables
types nothing into the text. On Windows `Ctrl+Alt` is also how `AltGr`
arrives, so there it types what the layout puts on that key. Typing always
ends a selection, even an empty one, so a capital right after `Ctrl+A` on an
empty buffer is kept.

**Non-Latin layouts.** The shortcuts are named by the keys of the Latin
layout. On another layout many terminals send the `Ctrl` code of the Latin
key anyway, and the shortcut works. A terminal that reports the `Ctrl` with
the letter of the layout instead gets one of them: `Ctrl+ы` is `Ctrl+S`, the
save, because `ы` is on the S key in every layout that has it (Russian,
Belarusian, Mongolian). Other letters sit on different keys in different
layouts (`с` is on C in the Russian layout and on S in the Serbian one), so
under `Ctrl` they do nothing. There the Latin layout is the way to the other
shortcuts.

### Clipboard and undo

| Key | Action |
|-----|--------|
| `Ctrl+C` | Copy the selection to the system clipboard. |
| `Ctrl+X` | Cut the selection to the system clipboard. |
| `Ctrl+V` | Paste from the system clipboard. |
| `Ctrl+Z` | Undo. Consecutive single-character inserts collapse into one step. A key that changed nothing, such as `Backspace` at the start, is no step. |
| `Ctrl+Y` | Redo. |

Where a copy goes:

- To the system clipboard through
  [`arboard`](https://crates.io/crates/arboard) (X11, Wayland, Windows,
  macOS).
- Additionally as an OSC 52 escape, so the terminal stores the text itself
  where it supports that (in tmux, with `set-clipboard on`). This keeps the
  copy alive after rusk exits and works over SSH.
- A copy longer than about 75 000 bytes goes to the system clipboard only:
  terminals drop an OSC 52 sequence much longer than that.
- A process-local fallback covers environments where neither is available
  (headless terminals, for one).

### Mouse

| Gesture | Action |
|---------|--------|
| Left click | Move the cursor; start a selection. |
| Left drag | Extend the selection. |
| Double-click | Select the word under the cursor. |
| Triple-click | Select the whole line. |
| `Shift` + click | Extend the existing selection to the click point, where the terminal passes it on (see below). |
| Middle click | Paste from the system clipboard at the click point. |
| Scroll wheel | Scroll the view by 3 visual rows. The cursor and the selection stay where they are (the cursor is hidden while out of sight); the next key brings the view back to it. |

Most terminals (xterm, VTE, kitty, alacritty, foot, iTerm2) keep `Shift` +
click for their own selection while an application reads the mouse, so rusk
never sees it. `Shift+←`/`→` and the other selection keys work everywhere.

## Dirty-state confirmation

Pressing `Esc` while the buffer differs from the original text (dirty state,
signalled by `●`) shows an overlay `Discard changes? [y/N]`.

- `y` or `Y` discards the changes, clears the autosave draft and cancels the
  edit. `Ctrl+Y` (redo) or `Alt+Y` is no answer: it returns to the editor
  like any other key.
- `Ctrl+C` in the overlay aborts the session and keeps the buffer as a
  draft.
- A resize while the question is up draws the editor again for the new size
  and asks again.

## Leaving the editor

However the session ends, the terminal is given back: raw mode off, the
alternate screen left, mouse reporting and bracketed paste turned off, the
cursor shown. That holds:

- for `Ctrl+S` and `Esc`;
- for an error inside the editor;
- for a panic (the message is printed onto the restored screen, not onto the
  one being torn down);
- on unix for `SIGTERM`, `SIGHUP` and `SIGQUIT`. Those also write the buffer
  out as a draft and exit `128 + signal` (143 for `SIGTERM`);
- with the help screen or the `Discard changes?` overlay up. They hand
  control back to the editor's own loop rather than waiting for a key that
  is never coming.

`rusk edit <id>` with no new text needs a terminal, exactly as `rusk add`
with no text does. Into a pipe or a redirect it says so instead of painting
an editor nobody can see. So does a terminal that prints escape sequences
instead of obeying them, `TERM=dumb` (Emacs' `M-x shell`, some CI): pass the
text on the command line there. The one-line `rusk del` question is asked
there as usual.

After `Ctrl+C` the typed text is kept as a draft, and one blank line
separates what rusk says about it from the editor's screen.

## Draft autosave and recovery

A draft is what the editor had in its buffer when it stopped without storing
anything: `Ctrl+C`, a `SIGTERM`, a save the database refused, a crash. It
exists so that the next edit of that task can offer the text back.

**Where.** One file per task, next to the database: `editor-task-3.draft`,
`editor-new-task.draft`. `$RUSK_DB` falls back to `./.rusk/` when not set
(see [STORAGE.md](STORAGE.md#database-location)). For an http(s) or ssh
database there is no local directory to use, so the drafts go to
`$XDG_RUNTIME_DIR/rusk`, or `<temp>/rusk-<uid>` when there is no runtime
directory. That directory is created `0700`, never a `/tmp/rusk` shared with
everyone on the machine.

**When.** Every ~3 seconds while the buffer differs from the stored task, and
once more whenever the editor stops with something unsaved in it.

- Each autosave replaces the previous draft atomically: an interrupted write
  never destroys it.
- The file is readable by its owner only (`0600`).
- If a draft cannot be written, rusk says so once the terminal is back.
- Clearing the buffer is an edit like any other: an empty draft is kept, not
  skipped.

**Payload.** JSON with the task key, the identity of the text the draft was
typed against, a timestamp, and the text:

```json
{ "key": "task-3", "base": "8f1c…", "text": "...", "timestamp": "2026-04-21T10:20:30+00:00" }
```

**Removed when the text is somewhere safer**: once the database holds it, on
a confirmed discard (`Esc` → `y`), and as soon as the buffer is brought back
to the task's own text, since nothing unsaved is left to keep. A `rusk del`
takes the draft of the deleted task with it.

**`Ctrl+S` stores the task right away.** In `rusk edit 1,2,3` each task is
stored as soon as its editor is closed, so a later `Esc` or `Ctrl+C` does not
take the earlier edits with it.

- If the save is refused or fails (another process changed or deleted the
  task while the editor was open, the database cannot be reached), nothing
  is overwritten and **the draft is still there**. Run the command again,
  answer `y`, and merge your text with the task as it is now.
- Where not even the draft could be written (a read-only directory refuses
  the database and the draft alike), the error says so and carries your text
  instead, so it is on the screen rather than lost.
- "Edited task" is printed only once the task is stored.

**Recovery.** When the editor is about to open a task that has a draft, it
says how old the draft is and what is in it:

```
Restore unsaved draft for task 3 (12 minutes ago, “buy oat milk and…”)? [y/N]:
```

- `y` loads the draft into the buffer. The task's stored text stays the
  baseline, so `●` shows at once, `Esc` asks before discarding, and `Ctrl+R`
  goes back to what the task actually holds.
- Any other answer deletes the draft and opens the stored text.
- A draft is pinned to the text it was typed against. One left over from a
  task that has since been deleted is never offered for the **new** task
  that reused its id.
- `rusk add -d <date>` keeps its date when a draft is restored. The draft
  holds text, not a due date, so the date is put back in front unless the
  draft already begins with one. A draft that begins with `_` (no date) gets
  the date in its place, and the words after it stay text.
- A draft that cannot be read is not thrown away. It is kept as
  `editor-task-3.draft.corrupt` and rusk says where it went, because the
  text inside it may still be readable by a human.

## Task date header

The due date, if any, is **only** the first whitespace-delimited token at the
**very start of the first line**. It takes the same forms as `-d` on the
command line:

| Form | Examples | Notes |
|---|---|---|
| Absolute | `31-12-2025`, `11-jan-25`, `1-sept-2026` | `DD-MM-YYYY`, slashes or dots too. A two-digit year is 20xx. The month may be a name. Years run from 1000 to 9999 |
| Words | `today`, `tomorrow` | |
| Relative | `2d`, `2w`, `10d5w`, `1m3q` | an offset from today. The months are added first, in one step, then the days |
| From the task's date | `+2w`, `+10d5w` | an offset from **this task's current due date**. If the task had no due date yet, `+` uses today, same as `rusk add -d` |
| Clear | `_` | as the only date token: removes the deadline |

- A **recognized** token is **highlighted in color** on that line: green for
  today or later, red if before today. Text that does not parse as a date is
  not colored.
- The text after the token is the body. A date alone on the first line is
  fine: the text starts on the next one.
- Empty lines before and after the text are not stored.
- A task due in a year outside 1000–9999 (a file an older rusk wrote; see
  [STORAGE.md](STORAGE.md#what-every-format-is-held-to)) opens with that
  date as its first token, and keeps it when you leave the token as it is.

Full syntax, including the relative forms, is in the in-editor help
(`Ctrl+G` / `F1`).

### The `_` marker

A `_` word right after the date, or right after the `_` that is no date, is
dropped too, and the word after it stays text: `01-01-2027 _ 2d fix` is due
on 1 January with the text `2d fix`.

So a text that starts with a word that reads as a date (`tomorrow call mom`,
`2d fix`) opens as `_ tomorrow call mom` for a task without a due date, and
as `01-01-2027 _ tomorrow call mom` for one with a date. The word stays text
when you:

- save it untouched;
- type a date in front;
- delete the date, or replace it with `_`.

Delete the `_ ` together with its space and the word becomes the due date,
which the coloring shows before you save. A text that starts with a `_` word
gets the mark too: `_ _ _ note`.

### Dates on the command line

`rusk edit <id> -d <date>` (with a value) sets the date without opening the
TUI. The same relative rules apply as for the first-line token, including a
leading `+`.

- **Bare** `-d` / `--date` (no value) is not supported: use the TUI to edit
  the first line.
- The interactive buffer is still the authoritative place for a due date:
  whatever appears as the first token on the first line, or nothing.
- For **`rusk add` with no text**, a leading `+` on a relative form still
  uses **today**, since there is no existing task due date yet. Same as
  `rusk add -d`.

## Output after editing

Each task that was saved, left unchanged or skipped gets a status line with
its id once its editor is closed. When something was saved, the task list
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
