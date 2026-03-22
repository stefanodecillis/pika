use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};

use crate::input::actions::{Action, FocusArea};
use crate::input::keymap::KeymapSet;

/// Translates raw terminal events into semantic `Action`s using the current keymap.
#[derive(Debug, Clone)]
pub struct InputHandler {
    pub keymap_set: KeymapSet,
}

impl InputHandler {
    pub fn new(keymap_set: KeymapSet) -> Self {
        Self { keymap_set }
    }

    /// Map a crossterm `Event` to a Pika `Action`, given the current focus area.
    ///
    /// Look-up order:
    /// 1. Focus-specific keymap
    /// 2. Global keymap
    /// 3. Fallback rules (character insertion, paste, etc.)
    pub fn handle_event(&self, event: &Event, focus: FocusArea) -> Action {
        match event {
            Event::Key(key_event) => self.handle_key(key_event, focus),
            Event::Paste(text) => self.handle_paste(text),
            Event::Resize(_, _) => Action::None,
            _ => Action::None,
        }
    }

    fn handle_key(&self, key: &KeyEvent, focus: FocusArea) -> Action {
        // Only handle key-press events (not release or repeat) to avoid
        // firing actions twice on platforms that send both press and release.
        if key.kind != KeyEventKind::Press {
            return Action::None;
        }

        let modifiers = key.modifiers;
        let code = key.code;

        // 1. Focus-specific keymap
        let focus_map = self.keymap_set.keymap_for(focus);
        if let Some(action) = focus_map.get(modifiers, code) {
            return action.clone();
        }

        // 2. Global keymap
        if let Some(action) = self.keymap_set.global.get(modifiers, code) {
            return action.clone();
        }

        // 3. Fallback: unmodified (or shift-only) character input
        match focus {
            FocusArea::Editor => self.editor_fallback(modifiers, code),
            FocusArea::CommandPalette => self.palette_fallback(modifiers, code),
            _ => Action::None,
        }
    }

    /// Fallback rules for unbound keys when the editor is focused.
    fn editor_fallback(&self, modifiers: KeyModifiers, code: KeyCode) -> Action {
        // Plain character or shift+character (upper-case letters, symbols)
        if let KeyCode::Char(c) = code {
            if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT {
                return Action::InsertChar(c);
            }
        }
        Action::None
    }

    /// Fallback rules for unbound keys when the command palette is focused.
    fn palette_fallback(&self, modifiers: KeyModifiers, code: KeyCode) -> Action {
        if let KeyCode::Char(c) = code {
            if modifiers.is_empty() || modifiers == KeyModifiers::SHIFT {
                return Action::PaletteInput(c);
            }
        }
        Action::None
    }

    /// Handle a paste event from the terminal.
    fn handle_paste(&self, text: &str) -> Action {
        let text = text.trim();
        if text.is_empty() {
            return Action::None;
        }

        // Try to detect file drop first
        if let Some(action) = Self::try_parse_file_drop(text) {
            return action;
        }

        // Otherwise treat as text paste — carry the full text
        Action::PasteText(text.to_string())
    }

    /// Try to interpret pasted content as a file drop.
    fn try_parse_file_drop(text: &str) -> Option<Action> {
        // Split into lines, handling various separators
        let lines: Vec<&str> = text
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();

        if lines.is_empty() {
            return None;
        }

        let paths: Vec<std::path::PathBuf> = lines
            .iter()
            .filter_map(|l| Self::parse_as_path(l))
            .collect();

        // If all lines resolved to existing paths, it's a file drop
        if paths.len() == lines.len() && !paths.is_empty() {
            Some(Action::FileDrop(paths))
        } else {
            None
        }
    }

    /// Try to interpret a string as a file path.
    fn parse_as_path(s: &str) -> Option<std::path::PathBuf> {
        let s = s.trim();

        // Remove surrounding quotes (some terminals quote paths with spaces)
        let s = s.strip_prefix('"').unwrap_or(s);
        let s = s.strip_suffix('"').unwrap_or(s);
        let s = s.strip_prefix('\'').unwrap_or(s);
        let s = s.strip_suffix('\'').unwrap_or(s);

        // Handle file:// URIs (common from drag-and-drop on macOS)
        let path_str = if let Some(stripped) = s.strip_prefix("file://") {
            Self::url_decode(stripped)
        } else if s.starts_with('~') {
            // Expand tilde
            if let Some(home) = dirs::home_dir() {
                let rest = s.strip_prefix("~/").unwrap_or(&s[1..]);
                home.join(rest).to_string_lossy().to_string()
            } else {
                return None;
            }
        } else if s.starts_with('/') {
            // Unescape backslash-escaped spaces (macOS Finder/terminal)
            s.replace("\\ ", " ")
        } else {
            return None;
        };

        let path = std::path::PathBuf::from(&path_str);
        if path.exists() {
            Some(path)
        } else {
            None
        }
    }

    /// Decode percent-encoded URL characters (e.g. %20 → space).
    fn url_decode(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        let mut chars = s.chars();
        while let Some(ch) = chars.next() {
            if ch == '%' {
                let hex: String = chars.by_ref().take(2).collect();
                if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                    result.push(byte as char);
                } else {
                    result.push('%');
                    result.push_str(&hex);
                }
            } else {
                result.push(ch);
            }
        }
        result
    }
}

impl Default for InputHandler {
    fn default() -> Self {
        Self::new(KeymapSet::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyEventKind, KeyEventState};

    /// Helper to build a press `KeyEvent`.
    fn press(code: KeyCode, modifiers: KeyModifiers) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    /// Helper to build a release `KeyEvent`.
    fn release(code: KeyCode, modifiers: KeyModifiers) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Release,
            state: KeyEventState::NONE,
        })
    }

    fn handler() -> InputHandler {
        InputHandler::default()
    }

    // -- Global keys --

    #[test]
    fn test_ctrl_q_quits_from_any_focus() {
        let h = handler();
        let ev = press(KeyCode::Char('q'), KeyModifiers::CONTROL);
        assert_eq!(h.handle_event(&ev, FocusArea::Editor), Action::Quit);
        assert_eq!(h.handle_event(&ev, FocusArea::Sidebar), Action::Quit);
    }

    #[test]
    fn test_ctrl_s_saves() {
        let h = handler();
        let ev = press(KeyCode::Char('s'), KeyModifiers::CONTROL);
        assert_eq!(h.handle_event(&ev, FocusArea::Editor), Action::SaveFile);
    }

    // -- Editor focus --

    #[test]
    fn test_plain_char_inserts_in_editor() {
        let h = handler();
        let ev = press(KeyCode::Char('x'), KeyModifiers::NONE);
        assert_eq!(h.handle_event(&ev, FocusArea::Editor), Action::InsertChar('x'));
    }

    #[test]
    fn test_shift_char_inserts_uppercase() {
        let h = handler();
        let ev = press(KeyCode::Char('X'), KeyModifiers::SHIFT);
        assert_eq!(h.handle_event(&ev, FocusArea::Editor), Action::InsertChar('X'));
    }

    #[test]
    fn test_arrow_keys_cursor_movement() {
        let h = handler();
        assert_eq!(
            h.handle_event(&press(KeyCode::Up, KeyModifiers::NONE), FocusArea::Editor),
            Action::CursorUp
        );
        assert_eq!(
            h.handle_event(&press(KeyCode::Down, KeyModifiers::NONE), FocusArea::Editor),
            Action::CursorDown
        );
    }

    #[test]
    fn test_ctrl_z_undo() {
        let h = handler();
        let ev = press(KeyCode::Char('z'), KeyModifiers::CONTROL);
        assert_eq!(h.handle_event(&ev, FocusArea::Editor), Action::Undo);
    }

    #[test]
    fn test_enter_inserts_newline() {
        let h = handler();
        let ev = press(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(h.handle_event(&ev, FocusArea::Editor), Action::InsertNewline);
    }

    #[test]
    fn test_backspace_deletes() {
        let h = handler();
        let ev = press(KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!(h.handle_event(&ev, FocusArea::Editor), Action::DeleteBackward);
    }

    #[test]
    fn test_f12_goto_definition() {
        let h = handler();
        let ev = press(KeyCode::F(12), KeyModifiers::NONE);
        assert_eq!(h.handle_event(&ev, FocusArea::Editor), Action::GotoDefinition);
    }

    // -- Focus-specific takes priority over global --

    #[test]
    fn test_focus_specific_overrides_global() {
        // Ctrl+C is Copy in editor, but FileCopy in sidebar
        let h = handler();
        let ev = press(KeyCode::Char('c'), KeyModifiers::CONTROL);
        assert_eq!(h.handle_event(&ev, FocusArea::Editor), Action::Copy);
        assert_eq!(h.handle_event(&ev, FocusArea::Sidebar), Action::FileCopy);
    }

    // -- Sidebar focus --

    #[test]
    fn test_sidebar_enter_opens() {
        let h = handler();
        let ev = press(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(h.handle_event(&ev, FocusArea::Sidebar), Action::TreeOpen);
    }

    #[test]
    fn test_sidebar_arrows_navigate() {
        let h = handler();
        assert_eq!(
            h.handle_event(&press(KeyCode::Up, KeyModifiers::NONE), FocusArea::Sidebar),
            Action::TreeUp
        );
        assert_eq!(
            h.handle_event(&press(KeyCode::Down, KeyModifiers::NONE), FocusArea::Sidebar),
            Action::TreeDown
        );
    }

    #[test]
    fn test_sidebar_n_new_file() {
        let h = handler();
        let ev = press(KeyCode::Char('n'), KeyModifiers::NONE);
        assert_eq!(h.handle_event(&ev, FocusArea::Sidebar), Action::FileNew);
    }

    // -- Completion popup --

    #[test]
    fn test_completion_up_down() {
        let h = handler();
        assert_eq!(
            h.handle_event(&press(KeyCode::Up, KeyModifiers::NONE), FocusArea::CompletionPopup),
            Action::CompletionUp
        );
        assert_eq!(
            h.handle_event(&press(KeyCode::Down, KeyModifiers::NONE), FocusArea::CompletionPopup),
            Action::CompletionDown
        );
    }

    #[test]
    fn test_completion_accept_enter() {
        let h = handler();
        let ev = press(KeyCode::Enter, KeyModifiers::NONE);
        assert_eq!(h.handle_event(&ev, FocusArea::CompletionPopup), Action::CompletionAccept);
    }

    #[test]
    fn test_completion_dismiss_esc() {
        let h = handler();
        let ev = press(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(h.handle_event(&ev, FocusArea::CompletionPopup), Action::CompletionDismiss);
    }

    // -- Command palette --

    #[test]
    fn test_palette_char_input() {
        let h = handler();
        let ev = press(KeyCode::Char('a'), KeyModifiers::NONE);
        assert_eq!(h.handle_event(&ev, FocusArea::CommandPalette), Action::PaletteInput('a'));
    }

    #[test]
    fn test_palette_backspace() {
        let h = handler();
        let ev = press(KeyCode::Backspace, KeyModifiers::NONE);
        assert_eq!(h.handle_event(&ev, FocusArea::CommandPalette), Action::PaletteBackspace);
    }

    #[test]
    fn test_palette_esc_dismisses() {
        let h = handler();
        let ev = press(KeyCode::Esc, KeyModifiers::NONE);
        assert_eq!(h.handle_event(&ev, FocusArea::CommandPalette), Action::PaletteDismiss);
    }

    // -- Paste --

    #[test]
    fn test_paste_text() {
        let h = handler();
        let ev = Event::Paste("hello".into());
        let action = h.handle_event(&ev, FocusArea::Editor);
        // Not a file path, so should be treated as text paste
        assert_eq!(action, Action::PasteText("hello".to_string()));
    }

    #[test]
    fn test_paste_file_paths() {
        // Create real temp files so parse_as_path can verify they exist
        let tmp = tempfile::TempDir::new().unwrap();
        let a = tmp.path().join("a.rs");
        let b = tmp.path().join("b.rs");
        std::fs::write(&a, "").unwrap();
        std::fs::write(&b, "").unwrap();

        let paste_text = format!("{}\n{}", a.display(), b.display());
        let h = handler();
        let ev = Event::Paste(paste_text);
        let action = h.handle_event(&ev, FocusArea::Editor);
        if let Action::FileDrop(paths) = action {
            assert_eq!(paths.len(), 2);
            assert_eq!(paths[0], a);
            assert_eq!(paths[1], b);
        } else {
            panic!("Expected FileDrop, got {:?}", action);
        }
    }

    #[test]
    fn test_paste_empty() {
        let h = handler();
        let ev = Event::Paste(String::new());
        assert_eq!(h.handle_event(&ev, FocusArea::Editor), Action::None);
    }

    // -- Resize --

    #[test]
    fn test_resize_returns_none() {
        let h = handler();
        let ev = Event::Resize(80, 24);
        assert_eq!(h.handle_event(&ev, FocusArea::Editor), Action::None);
    }

    // -- Key release ignored --

    #[test]
    fn test_key_release_ignored() {
        let h = handler();
        let ev = release(KeyCode::Char('a'), KeyModifiers::NONE);
        assert_eq!(h.handle_event(&ev, FocusArea::Editor), Action::None);
    }

    // -- Unbound key returns None --

    #[test]
    fn test_unbound_key_returns_none() {
        let h = handler();
        // F11 is not bound
        let ev = press(KeyCode::F(11), KeyModifiers::NONE);
        assert_eq!(h.handle_event(&ev, FocusArea::Editor), Action::None);
    }

    // -- Default impl --

    #[test]
    fn test_default_handler() {
        let h = InputHandler::default();
        // Just verify it doesn't panic and works
        let ev = press(KeyCode::Char('a'), KeyModifiers::NONE);
        assert_eq!(h.handle_event(&ev, FocusArea::Editor), Action::InsertChar('a'));
    }

    // -- Clone --

    #[test]
    fn test_handler_clone() {
        let h = handler();
        let h2 = h.clone();
        let ev = press(KeyCode::Char('q'), KeyModifiers::CONTROL);
        assert_eq!(h2.handle_event(&ev, FocusArea::Editor), Action::Quit);
    }

    // -- Palette shift char --

    #[test]
    fn test_palette_shift_char() {
        let h = handler();
        let ev = press(KeyCode::Char('A'), KeyModifiers::SHIFT);
        assert_eq!(
            h.handle_event(&ev, FocusArea::CommandPalette),
            Action::PaletteInput('A')
        );
    }

    // -- Sidebar unbound char returns None --

    #[test]
    fn test_sidebar_unbound_char_returns_none() {
        let h = handler();
        // 'z' is not bound in the sidebar and there is no char-fallback for sidebar
        let ev = press(KeyCode::Char('z'), KeyModifiers::NONE);
        assert_eq!(h.handle_event(&ev, FocusArea::Sidebar), Action::None);
    }
}
