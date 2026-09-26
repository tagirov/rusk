//! Task text on its way to a terminal.
//!
//! A task's text is data from anywhere: a database shared through
//! `rusk serve`, a synced or hand-edited file, a paste. A terminal does not
//! print control characters, it obeys them - ESC starts a sequence that can
//! hide the cursor, retitle the window or repaint the screen, and a C1 CSI
//! (U+009B) does the same in one character. So nothing from a task reaches
//! the terminal raw: what prints it escapes the controls into visible text
//! (like `ls -b`), and the editor keeps them out of its buffer.

use std::borrow::Cow;

/// A character a terminal would act on rather than show: C0 (except the
/// line break, which task text uses for its lines), DEL and C1.
pub fn is_control(c: char) -> bool {
    c.is_control() && c != '\n'
}

/// `s` as it may be printed. Line breaks stay: CRLF and a lone CR become
/// `\n`, the way the editor reads them. A tab becomes a space - it is
/// harmless, but it jumps to a tab stop no layout can measure. Every other
/// control shows as `\xNN`.
pub fn escape(s: &str) -> Cow<'_, str> {
    if !s.chars().any(is_control) {
        return Cow::Borrowed(s);
    }
    let mut out = String::with_capacity(s.len() + 8);
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\r' => {
                chars.next_if_eq(&'\n');
                out.push('\n');
            }
            '\t' => out.push(' '),
            c if is_control(c) => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    Cow::Owned(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_is_borrowed() {
        assert!(matches!(escape("plain\nlines, 日本語"), Cow::Borrowed(_)));
    }

    #[test]
    fn controls_become_visible() {
        assert_eq!(escape("a\x1b[31mb"), "a\\x1b[31mb");
        assert_eq!(escape("\x1b]0;title\x07 text"), "\\x1b]0;title\\x07 text");
        assert_eq!(escape("nul\0del\x7f"), "nul\\x00del\\x7f");
        // C1 CSI is one char but still a control sequence introducer.
        assert_eq!(escape("x\u{9b}2Jy"), "x\\x9b2Jy");
    }

    #[test]
    fn line_breaks_and_tabs_stay_readable() {
        assert_eq!(escape("a\r\nb\rc\nd"), "a\nb\nc\nd");
        assert_eq!(escape("a\tb"), "a b");
    }
}
