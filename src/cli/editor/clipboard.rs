//! System clipboard wrapper with a process-local fallback.

pub(super) struct EditorClipboard {
    internal: String,
}

impl EditorClipboard {
    pub fn new() -> Self {
        Self { internal: String::new() }
    }

    pub fn copy(&mut self, text: &str) {
        self.internal = text.to_string();
        if let Ok(mut c) = arboard::Clipboard::new() {
            let _ = c.set_text(text.to_string());
        }
    }

    pub fn paste(&self) -> String {
        if let Ok(mut c) = arboard::Clipboard::new()
            && let Ok(t) = c.get_text()
        {
            return t;
        }
        self.internal.clone()
    }
}
