//! Low-level terminal control: enter/leave alt-screen, raw-mode teardown,
//! help overlay, and the "discard changes?" confirmation popup.

use anyhow::{Context, Result};
use colored::*;
use crossterm::{
    ExecutableCommand, QueueableCommand,
    cursor::{Hide, MoveTo, Show},
    event::{
        DisableBracketedPaste, DisableMouseCapture, EnableBracketedPaste, EnableMouseCapture,
        Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
        MouseEventKind, read,
    },
    style::Print,
    terminal::{
        Clear, ClearType, DisableLineWrap, EnableLineWrap, EnterAlternateScreen,
        LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    },
};
use std::io::{self, Write};
use std::sync::atomic::{AtomicBool, Ordering};

struct ShowCursorOnDrop;
impl Drop for ShowCursorOnDrop {
    fn drop(&mut self) {
        let _ = std::io::stdout().execute(Show);
    }
}

/// Whether the terminal is currently the editor's: raw mode on, alternate
/// screen up, mouse reported. Read by the panic hook, which has no other
/// way to know, and cleared by whoever puts the terminal back.
static IN_EDITOR: AtomicBool = AtomicBool::new(false);

/// Puts the terminal back the way it was found. Safe to call when it
/// already is: the flag says whether there is anything to undo, so a second
/// call (a panic during teardown, a Drop after an explicit `finish`) writes
/// nothing.
fn restore(stdout: &mut io::Stdout) {
    if !IN_EDITOR.swap(false, Ordering::SeqCst) {
        return;
    }
    stdout.queue(EnableLineWrap).ok();
    stdout.queue(DisableMouseCapture).ok();
    stdout.queue(DisableBracketedPaste).ok();
    stdout.queue(LeaveAlternateScreen).ok();
    stdout.queue(Show).ok();
    stdout.flush().ok();
    disable_raw_mode().ok();
}

/// The terminal, for as long as the editor has it.
///
/// Teardown is not a line at the end of the happy path. A `?` on a render,
/// a panic on a cursor that landed inside a character, a SIGTERM from the
/// window that was closed — each of them used to leave the shell the user
/// comes back to in raw mode, inside the alternate screen, reporting mouse
/// clicks as garbage. The guard owns all three answers: `Drop` for the
/// error paths, a panic hook for the panics (it has to run *before* the
/// message is printed, or the message is printed onto the screen that is
/// about to be torn down), and, on unix, a signal handler that turns a
/// SIGTERM into one more pass through the editor's own loop.
pub(super) struct TerminalGuard {
    #[cfg(unix)]
    signals: signals::Restore,
}

impl TerminalGuard {
    /// Enter the alternate screen + raw mode with mouse capture and no line wrap.
    pub(super) fn enter(stdout: &mut io::Stdout) -> Result<Self> {
        install_panic_hook();
        #[cfg(unix)]
        let signals = signals::watch();
        enable_raw_mode().context("Failed to enable raw mode")?;
        IN_EDITOR.store(true, Ordering::SeqCst);
        let guard = Self {
            #[cfg(unix)]
            signals,
        };
        stdout.queue(EnterAlternateScreen)?;
        stdout.queue(EnableBracketedPaste)?;
        stdout.queue(EnableMouseCapture)?;
        stdout.queue(DisableLineWrap)?;
        stdout.queue(Clear(ClearType::All))?;
        stdout.flush().ok();
        Ok(guard)
    }

    /// Leave the alternate screen and restore normal terminal state. The
    /// guard has nothing left to do afterwards.
    pub(super) fn finish(&mut self, stdout: &mut io::Stdout) {
        restore(stdout);
        #[cfg(unix)]
        self.signals.undo();
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        self.finish(&mut io::stdout());
    }
}

/// Chains a hook in front of the one already there, once per process. It
/// only acts while [`IN_EDITOR`] says the terminal is the editor's, so it
/// costs nothing to leave installed for the rest of the run.
fn install_panic_hook() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore(&mut io::stdout());
            previous(info);
        }));
    });
}

/// A signal that asked the editor to stop, if one arrived. Taken, not
/// peeked: the caller is the one that acts on it.
#[cfg(unix)]
pub(super) fn caught_signal() -> Option<i32> {
    signals::taken()
}

#[cfg(not(unix))]
pub(super) fn caught_signal() -> Option<i32> {
    None
}

/// Whether such a signal is waiting, without taking it.
#[cfg(unix)]
fn signal_pending() -> bool {
    signals::pending()
}

#[cfg(not(unix))]
fn signal_pending() -> bool {
    false
}

/// The next event — but never past a terminating signal. The editor's own
/// loop is the one place that acts on those (it has the buffer to write
/// out), so an overlay that would otherwise sit in `read()` forever hands
/// control back to it instead. `None` means "a signal is waiting".
///
/// Without this the overlays were worse than before the guard existed: the
/// handler had already replaced the default disposition, so a SIGTERM with
/// the help screen up set a flag nobody read and the process could no
/// longer be killed by anything but SIGKILL.
fn read_event() -> Result<Option<Event>> {
    loop {
        if signal_pending() {
            return Ok(None);
        }
        if crossterm::event::poll(std::time::Duration::from_millis(200))? {
            return Ok(Some(read()?));
        }
    }
}

/// The exit a terminating signal asks for, once the editor has put the
/// terminal back and saved what it could.
pub(super) fn exit_after_signal(signum: i32, note: Option<&str>) -> ! {
    if let Some(note) = note {
        crate::errln!("{note}");
    }
    std::process::exit(128 + signum)
}

/// SIGTERM, SIGHUP and SIGQUIT, caught only while the editor is running.
///
/// The handler does one relaxed store and nothing else — everything that
/// matters (putting the terminal back, writing the draft) needs allocation
/// and locks, which no signal handler may take. The editor's loop wakes up
/// at least twice a second, so it notices within half a second.
#[cfg(unix)]
mod signals {
    use std::sync::atomic::{AtomicI32, Ordering};

    static CAUGHT: AtomicI32 = AtomicI32::new(0);

    const WATCHED: [libc::c_int; 3] = [libc::SIGTERM, libc::SIGHUP, libc::SIGQUIT];

    extern "C" fn catch(signum: libc::c_int) {
        CAUGHT.store(signum, Ordering::Relaxed);
    }

    /// What was in place before the editor took over.
    pub(super) struct Restore([libc::sighandler_t; WATCHED.len()]);

    impl Drop for Restore {
        fn drop(&mut self) {
            self.undo();
        }
    }

    impl Restore {
        pub(super) fn undo(&mut self) {
            for (signum, previous) in WATCHED.iter().zip(self.0) {
                // SAFETY: putting back exactly what `signal` handed out.
                unsafe { libc::signal(*signum, previous) };
            }
            CAUGHT.store(0, Ordering::Relaxed);
        }
    }

    pub(super) fn watch() -> Restore {
        CAUGHT.store(0, Ordering::Relaxed);
        let handler = catch as extern "C" fn(libc::c_int) as libc::sighandler_t;
        let mut previous = [libc::SIG_DFL; WATCHED.len()];
        for (slot, signum) in previous.iter_mut().zip(WATCHED) {
            // SAFETY: the handler only stores into a static atomic.
            *slot = unsafe { libc::signal(signum, handler) };
        }
        Restore(previous)
    }

    pub(super) fn taken() -> Option<i32> {
        match CAUGHT.swap(0, Ordering::Relaxed) {
            0 => None,
            signum => Some(signum),
        }
    }

    pub(super) fn pending() -> bool {
        CAUGHT.load(Ordering::Relaxed) != 0
    }
}

/// A single row in the help overlay body.
enum HelpRow {
    Section(&'static str),
    Pair(&'static str, &'static str),
    Note(&'static str),
    Blank,
}

const HELP_TITLE: &str = "Rusk Interactive Editor";
const HELP_HINT: &str = "press any key to return";

const HELP_ROWS: &[HelpRow] = &[
    HelpRow::Section("Navigation"),
    HelpRow::Pair("← / →", "move by character"),
    HelpRow::Pair("Ctrl+← / Ctrl+→", "jump by word"),
    HelpRow::Pair("↑ / ↓", "move by visual row"),
    HelpRow::Pair("Ctrl+↑ / Ctrl+↓", "move by 5 visual rows"),
    HelpRow::Pair("Home / End", "smart line start / line end"),
    HelpRow::Pair("Ctrl+Home / Ctrl+End", "buffer start / buffer end"),
    HelpRow::Pair("PageUp / PageDown", "scroll one page"),
    HelpRow::Pair("Ctrl+PageUp / Ctrl+PageDown", "buffer start / buffer end"),
    HelpRow::Blank,
    HelpRow::Section("Selection"),
    HelpRow::Pair("Shift + <movement>", "extend selection"),
    HelpRow::Pair("Ctrl+A", "select all"),
    HelpRow::Blank,
    HelpRow::Section("Editing"),
    HelpRow::Pair("Enter", "insert newline"),
    HelpRow::Pair("Tab", "insert four spaces"),
    HelpRow::Pair("Backspace / Delete", "delete char or selection"),
    HelpRow::Pair("Ctrl+W, Ctrl+Backspace", "delete word to the left"),
    HelpRow::Pair("Ctrl+Delete", "delete word to the right"),
    HelpRow::Pair("Ctrl+K", "kill to end of line"),
    HelpRow::Pair("Ctrl+Shift+K", "delete line (if the terminal tells it apart)"),
    HelpRow::Pair("Ctrl+U", "kill to beginning of line"),
    HelpRow::Pair("Ctrl+R", "restore original text"),
    HelpRow::Blank,
    HelpRow::Section("Clipboard & History"),
    HelpRow::Pair("Ctrl+C / Ctrl+X / Ctrl+V", "copy / cut / paste"),
    HelpRow::Pair("Ctrl+Z / Ctrl+Y", "undo / redo"),
    HelpRow::Blank,
    HelpRow::Section("Mouse"),
    HelpRow::Pair("Click / drag", "move cursor / extend selection"),
    HelpRow::Pair("Double-click", "select word"),
    HelpRow::Pair("Triple-click", "select line"),
    HelpRow::Pair("Shift + click", "extend selection to click point"),
    HelpRow::Pair("Middle-click", "paste at cursor"),
    HelpRow::Pair("Wheel", "scroll view"),
    HelpRow::Blank,
    HelpRow::Section("Session"),
    HelpRow::Pair("Ctrl+S", "save and exit"),
    HelpRow::Pair("Esc", "cancel / skip (confirms when dirty)"),
    HelpRow::Pair("Ctrl+G / F1", "show this help"),
    HelpRow::Blank,
    HelpRow::Section("Due date — first token of the first line"),
    HelpRow::Pair("DD-MM-YYYY", "absolute (also `/` or `.`; yy = 20yy)"),
    HelpRow::Pair("", "years run from 1000 to 9999"),
    HelpRow::Pair("11-jan-25", "month by name (short or long)"),
    HelpRow::Pair("today / tomorrow", "plain words work too"),
    HelpRow::Pair("2d 2w 3m 1q 1y", "relative from today (combine: 10d5w)"),
    HelpRow::Pair(
        "+2w, +10d5w",
        "relative to current due date (today if none)",
    ),
    HelpRow::Pair("_ 2d fix", "no date; the text is `2d fix`"),
    HelpRow::Pair("5-1-2027 _ 2d fix", "a `_` after the date keeps `2d` text"),
    HelpRow::Note("Recognized tokens are colored: green = today/future, red = past."),
];

fn pad_right_chars(s: &str, width: usize) -> String {
    let count = s.chars().count();
    if count >= width {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len() + (width - count));
    out.push_str(s);
    for _ in 0..(width - count) {
        out.push(' ');
    }
    out
}

/// Render the in-editor help overlay, wait for a key or click to dismiss it,
/// and then clear the screen so the caller can re-render the editor.
pub(super) fn show_help(stdout: &mut io::Stdout) -> Result<()> {
    /// title + rule + blank + blank before hint + hint
    const CHROME: usize = 5;
    /// Indent applied to every body row so sections visually group the pairs.
    const BODY_INDENT: usize = 2;

    /// Shared overflow/scroll geometry: (max_body, visible_body, max_scroll).
    fn help_scroll_geometry(term_rows: usize, body_len: usize) -> (usize, usize, usize) {
        let max_body = term_rows.saturating_sub(CHROME);
        let overflow = body_len > max_body;
        let visible_body = if overflow { max_body } else { body_len };
        let max_scroll = if overflow && visible_body > 0 {
            body_len.saturating_sub(visible_body)
        } else {
            0
        };
        (max_body, visible_body, max_scroll)
    }

    let body = HELP_ROWS;

    let key_col = body
        .iter()
        .filter_map(|r| match r {
            HelpRow::Pair(k, _) => Some(k.chars().count()),
            _ => None,
        })
        .max()
        .unwrap_or(0);

    let row_visible_width = |r: &HelpRow| -> usize {
        match r {
            HelpRow::Section(s) => s.chars().count(),
            HelpRow::Pair(_, d) => BODY_INDENT + key_col + 2 + d.chars().count(),
            HelpRow::Note(n) => BODY_INDENT + n.chars().count(),
            HelpRow::Blank => 0,
        }
    };
    let body_width = body.iter().map(row_visible_width).max().unwrap_or(0);

    let mut body_scroll: usize = 0;
    let _show_cursor = ShowCursorOnDrop;
    stdout.queue(Hide)?;
    stdout.flush().ok();

    let paint = |stdout: &mut io::Stdout, body_scroll: &mut usize| -> Result<()> {
        let (term_cols_u16, term_rows_u16) = super::view::term_size();
        let term_cols = term_cols_u16 as usize;
        let term_rows = term_rows_u16 as usize;

        let (max_body, visible_body, max_scroll) = help_scroll_geometry(term_rows, body.len());
        let overflow = body.len() > max_body;
        *body_scroll = (*body_scroll).min(max_scroll);

        let start_row = if overflow {
            0
        } else {
            let block_height = CHROME + body.len();
            term_rows.saturating_sub(block_height) / 2
        };
        let body_start_col = term_cols.saturating_sub(body_width) / 2;

        stdout.queue(Clear(ClearType::All))?;

        let title_col = term_cols.saturating_sub(HELP_TITLE.chars().count()) / 2;
        stdout.queue(MoveTo(title_col as u16, start_row as u16))?;
        stdout.queue(Print(HELP_TITLE.bold()))?;

        // Thin underline beneath the title, as wide as the body block.
        let rule_width = body_width.max(HELP_TITLE.chars().count());
        let rule: String = "─".repeat(rule_width);
        let rule_col = term_cols.saturating_sub(rule_width) / 2;
        stdout.queue(MoveTo(rule_col as u16, (start_row + 1) as u16))?;
        stdout.queue(Print(rule.dimmed()))?;

        let first = if overflow { *body_scroll } else { 0 };
        let n = if overflow {
            (body.len().saturating_sub(first)).min(visible_body)
        } else {
            body.len()
        };

        for (i, row) in body.iter().skip(first).take(n).enumerate() {
            let y = (start_row + 3 + i) as u16;
            match row {
                HelpRow::Section(s) => {
                    stdout.queue(MoveTo(body_start_col as u16, y))?;
                    stdout.queue(Print(crate::config::theme().info.paint(s).bold()))?;
                }
                HelpRow::Pair(k, d) => {
                    stdout.queue(MoveTo(body_start_col as u16, y))?;
                    stdout.queue(Print(" ".repeat(BODY_INDENT)))?;
                    let padded = pad_right_chars(k, key_col);
                    stdout.queue(Print(padded.bold()))?;
                    stdout.queue(Print("  "))?;
                    stdout.queue(Print(d.normal()))?;
                }
                HelpRow::Note(text) => {
                    stdout.queue(MoveTo(body_start_col as u16, y))?;
                    stdout.queue(Print(" ".repeat(BODY_INDENT)))?;
                    stdout.queue(Print(text.dimmed().italic()))?;
                }
                HelpRow::Blank => {}
            }
        }

        let last_body_row = if n == 0 {
            start_row + 2
        } else {
            start_row + 3 + (n - 1)
        };
        // At least one blank line between the last help line and the hint.
        let hint_row = last_body_row + 2;
        let hint_col = term_cols.saturating_sub(HELP_HINT.chars().count()) / 2;
        stdout.queue(MoveTo(hint_col as u16, hint_row as u16))?;
        stdout.queue(Print(HELP_HINT.dimmed().italic()))?;
        stdout.flush().ok();
        Ok(())
    };

    paint(stdout, &mut body_scroll)?;

    // Dismiss on any key Press or left mouse button Down; redraw on resize.
    // When content overflows, Up/Down/PageUp/PageDown/Home/End/scroll wheel move the view.
    // Drain queued follow-up events so they never reach the editor loop.
    loop {
        let (_, term_rows_u16) = super::view::term_size();
        let term_rows = term_rows_u16 as usize;
        let (max_body, visible_body, max_scroll) = help_scroll_geometry(term_rows, body.len());
        let page_step = visible_body.max(1);

        let Some(event) = read_event()? else {
            break;
        };
        match event {
            Event::Resize(_, _) => {
                body_scroll = body_scroll.min(max_scroll);
                paint(stdout, &mut body_scroll)?;
            }
            Event::Key(KeyEvent {
                code,
                kind: KeyEventKind::Press,
                ..
            }) if body.len() > max_body => {
                let new_scroll = match code {
                    KeyCode::Up => body_scroll.saturating_sub(1),
                    KeyCode::Down => body_scroll + 1,
                    KeyCode::PageUp => body_scroll.saturating_sub(page_step),
                    KeyCode::PageDown => body_scroll + page_step,
                    KeyCode::Home => 0,
                    KeyCode::End => max_scroll,
                    _ => break,
                };
                body_scroll = new_scroll.min(max_scroll);
                paint(stdout, &mut body_scroll)?;
            }
            Event::Key(KeyEvent {
                kind: KeyEventKind::Press,
                ..
            }) => break,
            Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                ..
            }) => break,
            Event::Mouse(MouseEvent {
                kind: kind @ (MouseEventKind::ScrollUp | MouseEventKind::ScrollDown),
                ..
            }) if body.len() > max_body => {
                let new_scroll = if kind == MouseEventKind::ScrollUp {
                    body_scroll.saturating_sub(3)
                } else {
                    body_scroll + 3
                };
                body_scroll = new_scroll.min(max_scroll);
                paint(stdout, &mut body_scroll)?;
            }
            _ => {}
        }
    }
    while crossterm::event::poll(std::time::Duration::from_millis(0))? {
        let _ = read()?;
    }

    stdout.queue(Clear(ClearType::All))?;
    stdout.flush().ok();
    Ok(())
}

/// What the "Discard changes?" overlay came back with.
#[derive(PartialEq)]
pub(super) enum Discard {
    Yes,
    No,
    /// Ctrl+C: the same abort as in the editor itself, and it takes the
    /// same way out — the buffer is kept as a draft, the terminal is put
    /// back by the guard.
    Abort,
}

/// Overlay "Discard changes? [y/N]". When `dialog_row` is `Some(r)`, the prompt is on row `r`
/// (in the space between the footer and the window bottom). When `None`, it is shown on the last
/// row, replacing the footer.
pub(super) fn confirm_discard(stdout: &mut io::Stdout, dialog_row: Option<u16>) -> Result<Discard> {
    let _show_cursor = ShowCursorOnDrop;
    stdout.queue(Hide)?;
    stdout.flush().ok();
    let (cols_u16, rows_u16) = super::view::term_size();
    let cols = cols_u16 as usize;
    let last = rows_u16.saturating_sub(1);
    // Same style as the CLI confirmation prompts: accent question, dimmed hint.
    let prompt_plain = " Discard changes? [y/N] ";
    let row = dialog_row.unwrap_or(last);
    let col = cols.saturating_sub(prompt_plain.chars().count()) / 2;
    stdout.queue(MoveTo(0, row))?;
    stdout.queue(Clear(ClearType::CurrentLine))?;
    stdout.queue(MoveTo(col as u16, row))?;
    stdout.queue(Print(crate::config::theme().accent.paint(" Discard changes? ")))?;
    stdout.queue(Print("[y/N] ".dimmed()))?;
    stdout.flush().ok();
    loop {
        let Some(event) = read_event()? else {
            // A terminating signal: do not answer for the user. The loop
            // takes it over on the next pass and keeps the buffer.
            return Ok(Discard::No);
        };
        if let Event::Key(KeyEvent {
            code,
            kind,
            modifiers,
            ..
        }) = event
        {
            if kind != KeyEventKind::Press {
                continue;
            }
            match (code, modifiers) {
                (KeyCode::Char('y') | KeyCode::Char('Y'), _) => return Ok(Discard::Yes),
                (KeyCode::Char('c'), KeyModifiers::CONTROL) => return Ok(Discard::Abort),
                _ => return Ok(Discard::No),
            }
        }
    }
}
