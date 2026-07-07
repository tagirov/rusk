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
    let _ = write!(out, "\x1b]52;c;{}\x07", base64(text.as_bytes()));
    let _ = out.flush();
}

fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        out.push(ALPHABET[(n >> 18 & 63) as usize] as char);
        out.push(ALPHABET[(n >> 12 & 63) as usize] as char);
        out.push(if chunk.len() > 1 {
            ALPHABET[(n >> 6 & 63) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            ALPHABET[(n & 63) as usize] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::base64;

    #[test]
    fn base64_matches_reference() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        assert_eq!(base64("héllo".as_bytes()), "aMOpbGxv");
    }
}
