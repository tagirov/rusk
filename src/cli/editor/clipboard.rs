//! System clipboard wrapper with a process-local fallback.
//!
//! Copy goes to three places: the process-local buffer (always works), the
//! system clipboard via `arboard`, and the terminal via an OSC 52 escape.
//! The `arboard` handle is kept alive for the editor's lifetime because on
//! X11/Wayland the copied text is served by the owning process and would be
//! lost the moment the handle is dropped. OSC 52 makes the terminal itself
//! store the text, so it survives process exit and works over SSH.

use std::io::Write;

pub(super) struct EditorClipboard {
    internal: String,
    system: Option<arboard::Clipboard>,
}

impl EditorClipboard {
    pub fn new() -> Self {
        Self {
            internal: String::new(),
            system: arboard::Clipboard::new().ok(),
        }
    }

    /// A clipboard that has not reached for the system one yet (tests).
    #[cfg(test)]
    pub fn detached() -> Self {
        Self {
            internal: String::new(),
            system: None,
        }
    }

    fn system(&mut self) -> Option<&mut arboard::Clipboard> {
        if self.system.is_none() {
            self.system = arboard::Clipboard::new().ok();
        }
        self.system.as_mut()
    }

    pub fn copy(&mut self, text: &str) {
        self.internal = text.to_string();
        if let Some(c) = self.system()
            && c.set_text(text.to_string()).is_err()
        {
            // A stale handle (e.g. the display server restarted) is retried
            // from scratch on the next call.
            self.system = None;
        }
        osc52_copy(text);
    }

    pub fn paste(&mut self) -> String {
        if let Some(c) = self.system()
            && let Ok(t) = c.get_text()
        {
            return t;
        }
        self.internal.clone()
    }
}

/// Ask the terminal to place `text` on the system clipboard (OSC 52).
/// Terminals without OSC 52 support silently ignore the sequence.
fn osc52_copy(text: &str) {
    let mut out = std::io::stdout();
    let _ = write!(out, "\x1b]52;c;{}\x07", crate::base64::encode(text.as_bytes()));
    let _ = out.flush();
}
