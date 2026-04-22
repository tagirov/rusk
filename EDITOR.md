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
- [Draft autosave and recovery](#draft-autosave-and-recovery)
- [Task date header](#task-date-header)
- [Output after editing](#output-after-editing)

## Overview

`rusk edit <id>` opens the interactive multi-line editor with the task's current
text preloaded. The editor renders inside an alternate screen and exposes
selection, clipboard, undo/redo, word navigation, and crash-safe autosave.

## Launching

```bash
# Edit task text and due date (first line) interactively.
rusk edit 1

# Edit several tasks in one session (the editor opens once per id).
rusk edit 1,2,3
```

## Visual layout

```
31-12-2025  first line of the task text ●
            second line of the task text
            third line …

                              ^S save · ^G help · Esc cancel
```

- Top rows: the editable text, soft-wrapped to fit the terminal width.
- If the first line starts with a valid due-date token, it is shown in color
  (**green** for today or later, **red** if before today); other rows use
  equivalent-width indent so cursor math stays accurate.
- Footer (last row): hotkey hint that adapts to the terminal width.
- Arrows `↑` / `↓` at the bottom-left appear when the buffer scrolls.
- Status glyph at the bottom-right: `●` (dirty) / `○` (saved).

## Key bindings

### Save, cancel, help

| Key | Action |
|-----|--------|
| `Ctrl+S` | Save and exit. Deletes the autosave draft. |
| `Esc` | Cancel / skip task. Prompts when the buffer is dirty. |
| `Ctrl+G` or `F1` | Show the in-editor help overlay. |
| `Ctrl+C` | Copy selection if any; otherwise abort the program. |

### Navigation

| Key | Action |
|-----|--------|
| `←` / `→` | Move by character. |
| `Ctrl+←` / `Ctrl+→` | Jump by word. |
| `↑` / `↓` | Move between visual (soft-wrapped) rows. |
| `Ctrl+↑` / `Ctrl+↓` | Move by 5 visual rows. |
| `Home` / `End` | Smart Home (first non-space, then col 0) / end of line. |
| `Ctrl+Home` / `Ctrl+End` | Buffer start / end. |
| `PageUp` / `PageDown` | Scroll one page. |
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
| `Ctrl+W`, `Ctrl+Backspace` | Delete word to the left. |
| `Ctrl+Delete` | Delete word to the right. |
| `Ctrl+K` | Kill from cursor to end of line (or join with next line at EOL). |
| `Ctrl+Shift+K` | Delete the whole current line. |
| `Ctrl+U` | Kill from beginning of line to cursor. |

### Clipboard and undo

| Key | Action |
|-----|--------|
| `Ctrl+C` | Copy selection to the system clipboard. |
| `Ctrl+X` | Cut selection to the system clipboard. |
| `Ctrl+V` | Paste from the system clipboard. |
| `Ctrl+Z` | Undo (consecutive single-char inserts collapse into one step). |
| `Ctrl+Y` | Redo. |

The editor uses [`arboard`](https://crates.io/crates/arboard) for the system
clipboard with a process-local fallback when clipboard access is unavailable
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
| Scroll wheel | Scroll the view (cursor follows by 3 visual rows). |

## Dirty-state confirmation

Pressing `Esc` while the buffer differs from the original text (dirty state,
signalled by `●`) shows an overlay `Discard changes? [y/N]`. Answering `y`
discards changes, clears the autosave draft, and cancels the edit. Any other
key returns to the editor.

## Draft autosave and recovery

- The editor writes its buffer to `$RUSK_DB/editor.draft` every ~3 seconds
  while there are unsaved changes. `$RUSK_DB` falls back to `./.rusk/` when not
  set (see the [main README](README.md#database-location)).
- The draft payload is JSON with the task key, timestamp, and text:

  ```json
  { "key": "task-3", "text": "...", "timestamp": "2026-04-21T10:20:30+00:00" }
  ```

- On a clean save (`Ctrl+S`) or confirmed discard (`Esc` → `y`) the draft file
  is deleted.
- When the editor starts and finds a draft whose key matches the task being
  edited, it prompts:

  ```
  Restore unsaved draft for task 3 ? [y/N]:
  ```

  Answering `y` pre-loads the draft instead of the stored text. Any other
  answer deletes the draft and continues with the stored text.
- A crash, `Ctrl+C`, or a terminal kill leaves the draft file in place so the
  next `rusk edit <id>` can offer recovery.

## Task date header

The due date (if any) is **only** the first whitespace-delimited token at the
**very start of the first line**. It may be an absolute date (`DD-MM-YYYY`,
slashes or short year ok), a relative offset from today (`2d`, `2w`, `10d5w`,
`1m3q`, …), or a relative offset from **this task's current due date** using a
leading `+` (`+2w`, `+10d5w`, …; if the task had no due date yet, `+` uses
today, same as `rusk add -d`). Use `_` as the only date token to clear
the deadline. A **recognized** token is **highlighted in color** on that line
(green for today or later, red if before today); text that does not parse as a
date is not colored. The rest of the first line is the task title; following
lines are the body.

`rusk edit <id> -d <date>` (with a value; same relative rules as this first-line
token, including leading `+`) sets the date without opening the TUI. **Bare** `-d` / `--date` (no value) is not supported;
use the TUI to edit the first line. The interactive buffer is still the
authoritative place for a due date: whatever appears as the first token on the
first line (or absent). Full syntax, including relative forms, is in the
in-editor help (`Ctrl+G` / `F1`).

```
31-12-2025 Buy groceries: milk, bread, eggs
           Pick up the cake at 17:00
```

## Output after editing

The editor is intentionally quiet on exit: no task body is echoed back to the
terminal. Only the status line and the task id are printed:

```
Edited task: 3
Task unchanged: 4
Skipped task: 5
```

## Resize handling

The editor listens for `Event::Resize` and re-renders immediately. The view
top is re-clamped so the cursor stays visible in the new geometry.
