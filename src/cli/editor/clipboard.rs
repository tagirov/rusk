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

/// Beyond this much encoded text terminals drop an OSC 52 sequence (xterm
/// takes about 100 000 bytes; tmux, where `set-clipboard` lets it through
/// at all, a similar amount), and a multi-megabyte one is a flood of the
/// terminal for nothing (REVIEW section 4). A longer copy goes to the
/// system clipboard only.
const OSC52_LIMIT: usize = 100_000;

/// Ask the terminal to place `text` on the system clipboard (OSC 52).
/// Terminals without OSC 52 support silently ignore the sequence.
fn osc52_copy(text: &str) {
    if let Some(sequence) = osc52_sequence(text) {
        let mut out = std::io::stdout();
        let _ = out.write_all(sequence.as_bytes());
        let _ = out.flush();
    }
}

/// The OSC 52 sequence for `text`, when it is short enough to send.
fn osc52_sequence(text: &str) -> Option<String> {
    let encoded = crate::base64::encode(text.as_bytes());
    (encoded.len() <= OSC52_LIMIT).then(|| format!("\x1b]52;c;{encoded}\x07"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_too_long_for_a_terminal_is_not_sent_to_it() {
        assert_eq!(osc52_sequence("hi").as_deref(), Some("\x1b]52;c;aGk=\x07"));
        assert!(osc52_sequence(&"x".repeat(75_000)).is_some());
        assert!(osc52_sequence(&"x".repeat(75_001)).is_none());
    }
}
