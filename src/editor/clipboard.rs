/// System clipboard access with graceful fallback for headless environments.

/// Clipboard wrapper that falls back to an internal buffer when the system
/// clipboard is unavailable (e.g., headless servers, SSH sessions).
pub struct Clipboard {
    system: Option<arboard::Clipboard>,
    fallback: String,
}

impl std::fmt::Debug for Clipboard {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Clipboard")
            .field("system", &self.system.is_some())
            .field("fallback", &self.fallback)
            .finish()
    }
}

impl Clipboard {
    /// Create a new clipboard. Attempts to connect to the system clipboard;
    /// if that fails (e.g. no display server), operations silently fall back
    /// to an internal string buffer.
    pub fn new() -> Self {
        let system = arboard::Clipboard::new().ok();
        Self {
            system,
            fallback: String::new(),
        }
    }

    /// Get text from the clipboard. Tries the system clipboard first, then
    /// falls back to the internal buffer.
    pub fn get_text(&mut self) -> String {
        if let Some(ref mut cb) = self.system {
            match cb.get_text() {
                Ok(text) => return text,
                Err(_) => {} // fall through to fallback
            }
        }
        self.fallback.clone()
    }

    /// Set text on the clipboard. Tries the system clipboard first; if that
    /// fails, stores in the internal fallback buffer.
    pub fn set_text(&mut self, text: &str) {
        self.fallback = text.to_string();
        if let Some(ref mut cb) = self.system {
            let _ = cb.set_text(text.to_string());
        }
    }

    /// Returns `true` if the system clipboard is available.
    pub fn has_system_clipboard(&self) -> bool {
        self.system.is_some()
    }
}

impl Default for Clipboard {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create a clipboard that is guaranteed to use the fallback path.
    fn fallback_clipboard() -> Clipboard {
        Clipboard {
            system: None,
            fallback: String::new(),
        }
    }

    #[test]
    fn new_does_not_panic() {
        // Should succeed on any platform, even CI without a display.
        let _cb = Clipboard::new();
    }

    #[test]
    fn fallback_set_and_get() {
        let mut cb = fallback_clipboard();
        cb.set_text("hello clipboard");
        assert_eq!(cb.get_text(), "hello clipboard");
    }

    #[test]
    fn fallback_empty_initially() {
        let mut cb = fallback_clipboard();
        assert_eq!(cb.get_text(), "");
    }

    #[test]
    fn fallback_overwrite() {
        let mut cb = fallback_clipboard();
        cb.set_text("first");
        cb.set_text("second");
        assert_eq!(cb.get_text(), "second");
    }

    #[test]
    fn fallback_multiline_text() {
        let mut cb = fallback_clipboard();
        let text = "line one\nline two\nline three";
        cb.set_text(text);
        assert_eq!(cb.get_text(), text);
    }

    #[test]
    fn fallback_unicode() {
        let mut cb = fallback_clipboard();
        let text = "日本語 🚀 café";
        cb.set_text(text);
        assert_eq!(cb.get_text(), text);
    }

    #[test]
    fn fallback_empty_string() {
        let mut cb = fallback_clipboard();
        cb.set_text("something");
        cb.set_text("");
        assert_eq!(cb.get_text(), "");
    }

    #[test]
    fn has_system_clipboard_fallback() {
        let cb = fallback_clipboard();
        assert!(!cb.has_system_clipboard());
    }

    #[test]
    fn debug_impl() {
        let cb = fallback_clipboard();
        let debug_str = format!("{:?}", cb);
        assert!(debug_str.contains("Clipboard"));
        assert!(debug_str.contains("system"));
    }

    #[test]
    fn default_trait() {
        // Default should be the same as new() — just make sure it doesn't panic.
        let _cb: Clipboard = Default::default();
    }

    // If we happen to be on a system with a real clipboard, test it.
    // This test is allowed to "pass trivially" on headless CI.
    #[test]
    fn system_clipboard_if_available() {
        let mut cb = Clipboard::new();
        if cb.has_system_clipboard() {
            let unique = format!("pika_test_{}", std::process::id());
            cb.set_text(&unique);
            assert_eq!(cb.get_text(), unique);
        }
    }
}
