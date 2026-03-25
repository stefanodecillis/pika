use std::collections::HashMap;

use crossterm::event::{KeyCode, KeyModifiers};

use crate::input::actions::{Action, FocusArea};

/// A mapping from (modifiers, key) pairs to actions.
#[derive(Debug, Clone)]
pub struct Keymap {
    pub mappings: HashMap<(KeyModifiers, KeyCode), Action>,
}

impl Keymap {
    pub fn new() -> Self {
        Self {
            mappings: HashMap::new(),
        }
    }

    /// Insert a binding into the map.
    pub fn bind(&mut self, modifiers: KeyModifiers, code: KeyCode, action: Action) {
        self.mappings.insert((modifiers, code), action);
    }

    /// Look up a key event in this map.
    pub fn get(&self, modifiers: KeyModifiers, code: KeyCode) -> Option<&Action> {
        self.mappings.get(&(modifiers, code))
    }
}

/// One `Keymap` per focus area, plus a global map that is always consulted as a fallback.
#[derive(Debug, Clone)]
pub struct KeymapSet {
    pub global: Keymap,
    pub editor: Keymap,
    pub sidebar: Keymap,
    pub command_palette: Keymap,
    pub completion: Keymap,
}

impl KeymapSet {
    /// Return the focus-specific keymap for the given area.
    pub fn keymap_for(&self, focus: FocusArea) -> &Keymap {
        match focus {
            FocusArea::Editor => &self.editor,
            FocusArea::Sidebar => &self.sidebar,
            FocusArea::CommandPalette => &self.command_palette,
            FocusArea::CompletionPopup => &self.completion,
        }
    }
}

impl Default for KeymapSet {
    fn default() -> Self {
        let mut global = Keymap::new();
        let mut editor = Keymap::new();
        let mut sidebar = Keymap::new();
        let mut command_palette = Keymap::new();
        let mut completion = Keymap::new();

        // ======================================================
        // Global bindings (active in every focus area)
        // ======================================================
        global.bind(KeyModifiers::CONTROL, KeyCode::Char('b'), Action::ToggleSidebar);
        global.bind(KeyModifiers::CONTROL, KeyCode::Char('p'), Action::OpenCommandPalette);
        global.bind(KeyModifiers::CONTROL, KeyCode::Char('s'), Action::SaveFile);
        global.bind(KeyModifiers::CONTROL, KeyCode::Char('w'), Action::CloseTab);
        global.bind(KeyModifiers::CONTROL, KeyCode::Char('q'), Action::Quit);
        global.bind(KeyModifiers::ALT, KeyCode::Right, Action::NextTab);
        global.bind(KeyModifiers::ALT, KeyCode::Left, Action::PreviousTab);
        global.bind(KeyModifiers::NONE, KeyCode::Esc, Action::FocusNext);
        global.bind(KeyModifiers::CONTROL, KeyCode::Char('h'), Action::ShowShortcuts);
        global.bind(
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            KeyCode::Char('F'),
            Action::ProjectSearch,
        );

        // ======================================================
        // Editor bindings
        // ======================================================

        // -- Cursor movement --
        editor.bind(KeyModifiers::NONE, KeyCode::Up, Action::CursorUp);
        editor.bind(KeyModifiers::NONE, KeyCode::Down, Action::CursorDown);
        editor.bind(KeyModifiers::NONE, KeyCode::Left, Action::CursorLeft);
        editor.bind(KeyModifiers::NONE, KeyCode::Right, Action::CursorRight);
        editor.bind(KeyModifiers::CONTROL, KeyCode::Left, Action::CursorWordLeft);
        editor.bind(KeyModifiers::CONTROL, KeyCode::Right, Action::CursorWordRight);
        editor.bind(KeyModifiers::NONE, KeyCode::Home, Action::CursorLineStart);
        editor.bind(KeyModifiers::NONE, KeyCode::End, Action::CursorLineEnd);
        editor.bind(KeyModifiers::CONTROL, KeyCode::Home, Action::CursorFileStart);
        editor.bind(KeyModifiers::CONTROL, KeyCode::End, Action::CursorFileEnd);
        editor.bind(KeyModifiers::NONE, KeyCode::PageUp, Action::PageUp);
        editor.bind(KeyModifiers::NONE, KeyCode::PageDown, Action::PageDown);

        // -- Text editing --
        editor.bind(KeyModifiers::NONE, KeyCode::Enter, Action::InsertNewline);
        editor.bind(KeyModifiers::NONE, KeyCode::Tab, Action::InsertTab);
        editor.bind(KeyModifiers::NONE, KeyCode::Backspace, Action::DeleteBackward);
        editor.bind(KeyModifiers::NONE, KeyCode::Delete, Action::DeleteForward);
        editor.bind(KeyModifiers::CONTROL, KeyCode::Backspace, Action::DeleteWordBackward);
        editor.bind(KeyModifiers::CONTROL, KeyCode::Delete, Action::DeleteWordForward);
        editor.bind(
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            KeyCode::Char('K'),
            Action::DeleteLine,
        );

        // -- Selection --
        editor.bind(KeyModifiers::SHIFT, KeyCode::Up, Action::SelectUp);
        editor.bind(KeyModifiers::SHIFT, KeyCode::Down, Action::SelectDown);
        editor.bind(KeyModifiers::SHIFT, KeyCode::Left, Action::SelectLeft);
        editor.bind(KeyModifiers::SHIFT, KeyCode::Right, Action::SelectRight);
        editor.bind(
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            KeyCode::Left,
            Action::SelectWordLeft,
        );
        editor.bind(
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            KeyCode::Right,
            Action::SelectWordRight,
        );
        editor.bind(KeyModifiers::SHIFT, KeyCode::Home, Action::SelectLineStart);
        editor.bind(KeyModifiers::SHIFT, KeyCode::End, Action::SelectLineEnd);
        editor.bind(KeyModifiers::CONTROL, KeyCode::Char('a'), Action::SelectAll);
        editor.bind(KeyModifiers::CONTROL, KeyCode::Char('d'), Action::SelectNextOccurrence);

        // -- Clipboard / History --
        editor.bind(KeyModifiers::CONTROL, KeyCode::Char('z'), Action::Undo);
        editor.bind(KeyModifiers::CONTROL, KeyCode::Char('y'), Action::Redo);
        editor.bind(KeyModifiers::CONTROL, KeyCode::Char('c'), Action::Copy);
        editor.bind(KeyModifiers::CONTROL, KeyCode::Char('x'), Action::Cut);
        editor.bind(KeyModifiers::CONTROL, KeyCode::Char('v'), Action::Paste);

        // -- Search --
        editor.bind(KeyModifiers::CONTROL, KeyCode::Char('f'), Action::FindInFile);
        editor.bind(KeyModifiers::CONTROL, KeyCode::Char('r'), Action::FindAndReplace);
        editor.bind(KeyModifiers::CONTROL, KeyCode::Char('g'), Action::GoToLine);

        // -- LSP --
        editor.bind(KeyModifiers::CONTROL, KeyCode::Char(' '), Action::TriggerCompletion);
        editor.bind(KeyModifiers::NONE, KeyCode::F(12), Action::GotoDefinition);
        editor.bind(KeyModifiers::SHIFT, KeyCode::F(12), Action::FindReferences);
        editor.bind(KeyModifiers::NONE, KeyCode::F(2), Action::RenameSymbol);
        editor.bind(KeyModifiers::CONTROL, KeyCode::Char('.'), Action::CodeAction);

        // ======================================================
        // Sidebar bindings
        // ======================================================
        sidebar.bind(KeyModifiers::NONE, KeyCode::Up, Action::TreeUp);
        sidebar.bind(KeyModifiers::NONE, KeyCode::Down, Action::TreeDown);
        sidebar.bind(KeyModifiers::NONE, KeyCode::Right, Action::TreeExpand);
        sidebar.bind(KeyModifiers::NONE, KeyCode::Left, Action::TreeCollapse);
        sidebar.bind(KeyModifiers::NONE, KeyCode::Enter, Action::TreeOpen);
        sidebar.bind(KeyModifiers::NONE, KeyCode::Delete, Action::FileDelete);
        sidebar.bind(KeyModifiers::NONE, KeyCode::Backspace, Action::FileDelete);
        sidebar.bind(KeyModifiers::NONE, KeyCode::F(2), Action::FileRename);
        sidebar.bind(KeyModifiers::NONE, KeyCode::Char('n'), Action::FileNew);
        sidebar.bind(KeyModifiers::SHIFT, KeyCode::Char('N'), Action::DirNew);
        sidebar.bind(KeyModifiers::CONTROL, KeyCode::Char('c'), Action::FileCopy);
        sidebar.bind(KeyModifiers::CONTROL, KeyCode::Char('x'), Action::FileCut);
        sidebar.bind(KeyModifiers::CONTROL, KeyCode::Char('v'), Action::FilePaste);

        // ======================================================
        // Completion popup bindings
        // ======================================================
        completion.bind(KeyModifiers::NONE, KeyCode::Up, Action::CompletionUp);
        completion.bind(KeyModifiers::NONE, KeyCode::Down, Action::CompletionDown);
        completion.bind(KeyModifiers::NONE, KeyCode::Enter, Action::CompletionAccept);
        completion.bind(KeyModifiers::NONE, KeyCode::Tab, Action::CompletionAccept);
        completion.bind(KeyModifiers::NONE, KeyCode::Esc, Action::CompletionDismiss);

        // ======================================================
        // Command palette bindings
        // ======================================================
        command_palette.bind(KeyModifiers::NONE, KeyCode::Up, Action::PaletteUp);
        command_palette.bind(KeyModifiers::NONE, KeyCode::Down, Action::PaletteDown);
        command_palette.bind(KeyModifiers::NONE, KeyCode::Enter, Action::PaletteAccept);
        command_palette.bind(KeyModifiers::NONE, KeyCode::Esc, Action::PaletteDismiss);
        command_palette.bind(KeyModifiers::NONE, KeyCode::Backspace, Action::PaletteBackspace);

        Self {
            global,
            editor,
            sidebar,
            command_palette,
            completion,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_set() -> KeymapSet {
        KeymapSet::default()
    }

    // -- Global bindings --

    #[test]
    fn test_global_ctrl_q_quits() {
        let set = default_set();
        let action = set.global.get(KeyModifiers::CONTROL, KeyCode::Char('q'));
        assert_eq!(action, Some(&Action::Quit));
    }

    #[test]
    fn test_global_ctrl_s_saves() {
        let set = default_set();
        let action = set.global.get(KeyModifiers::CONTROL, KeyCode::Char('s'));
        assert_eq!(action, Some(&Action::SaveFile));
    }

    #[test]
    fn test_global_ctrl_b_toggles_sidebar() {
        let set = default_set();
        let action = set.global.get(KeyModifiers::CONTROL, KeyCode::Char('b'));
        assert_eq!(action, Some(&Action::ToggleSidebar));
    }

    #[test]
    fn test_global_ctrl_p_command_palette() {
        let set = default_set();
        let action = set.global.get(KeyModifiers::CONTROL, KeyCode::Char('p'));
        assert_eq!(action, Some(&Action::OpenCommandPalette));
    }

    #[test]
    fn test_global_ctrl_w_close_tab() {
        let set = default_set();
        let action = set.global.get(KeyModifiers::CONTROL, KeyCode::Char('w'));
        assert_eq!(action, Some(&Action::CloseTab));
    }

    #[test]
    fn test_global_alt_right_next_tab() {
        let set = default_set();
        let action = set.global.get(KeyModifiers::ALT, KeyCode::Right);
        assert_eq!(action, Some(&Action::NextTab));
    }

    #[test]
    fn test_global_ctrl_shift_f_project_search() {
        let set = default_set();
        let action = set.global.get(
            KeyModifiers::CONTROL | KeyModifiers::SHIFT,
            KeyCode::Char('F'),
        );
        assert_eq!(action, Some(&Action::ProjectSearch));
    }

    // -- Editor bindings --

    #[test]
    fn test_editor_arrow_keys() {
        let set = default_set();
        assert_eq!(set.editor.get(KeyModifiers::NONE, KeyCode::Up), Some(&Action::CursorUp));
        assert_eq!(set.editor.get(KeyModifiers::NONE, KeyCode::Down), Some(&Action::CursorDown));
        assert_eq!(set.editor.get(KeyModifiers::NONE, KeyCode::Left), Some(&Action::CursorLeft));
        assert_eq!(set.editor.get(KeyModifiers::NONE, KeyCode::Right), Some(&Action::CursorRight));
    }

    #[test]
    fn test_editor_ctrl_arrow_word_movement() {
        let set = default_set();
        assert_eq!(
            set.editor.get(KeyModifiers::CONTROL, KeyCode::Left),
            Some(&Action::CursorWordLeft)
        );
        assert_eq!(
            set.editor.get(KeyModifiers::CONTROL, KeyCode::Right),
            Some(&Action::CursorWordRight)
        );
    }

    #[test]
    fn test_editor_home_end() {
        let set = default_set();
        assert_eq!(set.editor.get(KeyModifiers::NONE, KeyCode::Home), Some(&Action::CursorLineStart));
        assert_eq!(set.editor.get(KeyModifiers::NONE, KeyCode::End), Some(&Action::CursorLineEnd));
    }

    #[test]
    fn test_editor_page_up_down() {
        let set = default_set();
        assert_eq!(set.editor.get(KeyModifiers::NONE, KeyCode::PageUp), Some(&Action::PageUp));
        assert_eq!(set.editor.get(KeyModifiers::NONE, KeyCode::PageDown), Some(&Action::PageDown));
    }

    #[test]
    fn test_editor_undo_redo() {
        let set = default_set();
        assert_eq!(set.editor.get(KeyModifiers::CONTROL, KeyCode::Char('z')), Some(&Action::Undo));
        assert_eq!(set.editor.get(KeyModifiers::CONTROL, KeyCode::Char('y')), Some(&Action::Redo));
    }

    #[test]
    fn test_editor_clipboard() {
        let set = default_set();
        assert_eq!(set.editor.get(KeyModifiers::CONTROL, KeyCode::Char('c')), Some(&Action::Copy));
        assert_eq!(set.editor.get(KeyModifiers::CONTROL, KeyCode::Char('x')), Some(&Action::Cut));
        assert_eq!(set.editor.get(KeyModifiers::CONTROL, KeyCode::Char('v')), Some(&Action::Paste));
    }

    #[test]
    fn test_editor_select_all() {
        let set = default_set();
        assert_eq!(
            set.editor.get(KeyModifiers::CONTROL, KeyCode::Char('a')),
            Some(&Action::SelectAll)
        );
    }

    #[test]
    fn test_editor_select_next_occurrence() {
        let set = default_set();
        assert_eq!(
            set.editor.get(KeyModifiers::CONTROL, KeyCode::Char('d')),
            Some(&Action::SelectNextOccurrence)
        );
    }

    #[test]
    fn test_editor_shift_selection() {
        let set = default_set();
        assert_eq!(set.editor.get(KeyModifiers::SHIFT, KeyCode::Up), Some(&Action::SelectUp));
        assert_eq!(set.editor.get(KeyModifiers::SHIFT, KeyCode::Down), Some(&Action::SelectDown));
        assert_eq!(set.editor.get(KeyModifiers::SHIFT, KeyCode::Left), Some(&Action::SelectLeft));
        assert_eq!(set.editor.get(KeyModifiers::SHIFT, KeyCode::Right), Some(&Action::SelectRight));
    }

    #[test]
    fn test_editor_text_editing_keys() {
        let set = default_set();
        assert_eq!(set.editor.get(KeyModifiers::NONE, KeyCode::Enter), Some(&Action::InsertNewline));
        assert_eq!(set.editor.get(KeyModifiers::NONE, KeyCode::Tab), Some(&Action::InsertTab));
        assert_eq!(set.editor.get(KeyModifiers::NONE, KeyCode::Backspace), Some(&Action::DeleteBackward));
        assert_eq!(set.editor.get(KeyModifiers::NONE, KeyCode::Delete), Some(&Action::DeleteForward));
    }

    #[test]
    fn test_editor_find_replace() {
        let set = default_set();
        assert_eq!(
            set.editor.get(KeyModifiers::CONTROL, KeyCode::Char('f')),
            Some(&Action::FindInFile)
        );
        assert_eq!(
            set.editor.get(KeyModifiers::CONTROL, KeyCode::Char('r')),
            Some(&Action::FindAndReplace)
        );
        // Ctrl+H must NOT be in editor map so the global ShowShortcuts binding is reached
        assert_eq!(set.editor.get(KeyModifiers::CONTROL, KeyCode::Char('h')), None);
    }

    #[test]
    fn test_global_alt_left_previous_tab() {
        let set = default_set();
        assert_eq!(
            set.global.get(KeyModifiers::ALT, KeyCode::Left),
            Some(&Action::PreviousTab)
        );
    }

    #[test]
    fn test_editor_goto_line() {
        let set = default_set();
        assert_eq!(
            set.editor.get(KeyModifiers::CONTROL, KeyCode::Char('g')),
            Some(&Action::GoToLine)
        );
    }

    #[test]
    fn test_editor_lsp_bindings() {
        let set = default_set();
        assert_eq!(
            set.editor.get(KeyModifiers::CONTROL, KeyCode::Char(' ')),
            Some(&Action::TriggerCompletion)
        );
        assert_eq!(set.editor.get(KeyModifiers::NONE, KeyCode::F(12)), Some(&Action::GotoDefinition));
        assert_eq!(set.editor.get(KeyModifiers::SHIFT, KeyCode::F(12)), Some(&Action::FindReferences));
        assert_eq!(set.editor.get(KeyModifiers::NONE, KeyCode::F(2)), Some(&Action::RenameSymbol));
        assert_eq!(
            set.editor.get(KeyModifiers::CONTROL, KeyCode::Char('.')),
            Some(&Action::CodeAction)
        );
    }

    // -- Sidebar bindings --

    #[test]
    fn test_sidebar_navigation() {
        let set = default_set();
        assert_eq!(set.sidebar.get(KeyModifiers::NONE, KeyCode::Up), Some(&Action::TreeUp));
        assert_eq!(set.sidebar.get(KeyModifiers::NONE, KeyCode::Down), Some(&Action::TreeDown));
        assert_eq!(set.sidebar.get(KeyModifiers::NONE, KeyCode::Right), Some(&Action::TreeExpand));
        assert_eq!(set.sidebar.get(KeyModifiers::NONE, KeyCode::Left), Some(&Action::TreeCollapse));
        assert_eq!(set.sidebar.get(KeyModifiers::NONE, KeyCode::Enter), Some(&Action::TreeOpen));
    }

    #[test]
    fn test_sidebar_file_operations() {
        let set = default_set();
        assert_eq!(set.sidebar.get(KeyModifiers::NONE, KeyCode::Delete), Some(&Action::FileDelete));
        assert_eq!(set.sidebar.get(KeyModifiers::NONE, KeyCode::F(2)), Some(&Action::FileRename));
        assert_eq!(set.sidebar.get(KeyModifiers::NONE, KeyCode::Char('n')), Some(&Action::FileNew));
        assert_eq!(set.sidebar.get(KeyModifiers::SHIFT, KeyCode::Char('N')), Some(&Action::DirNew));
    }

    #[test]
    fn test_sidebar_clipboard() {
        let set = default_set();
        assert_eq!(set.sidebar.get(KeyModifiers::CONTROL, KeyCode::Char('c')), Some(&Action::FileCopy));
        assert_eq!(set.sidebar.get(KeyModifiers::CONTROL, KeyCode::Char('x')), Some(&Action::FileCut));
        assert_eq!(set.sidebar.get(KeyModifiers::CONTROL, KeyCode::Char('v')), Some(&Action::FilePaste));
    }

    // -- Completion popup bindings --

    #[test]
    fn test_completion_navigation() {
        let set = default_set();
        assert_eq!(set.completion.get(KeyModifiers::NONE, KeyCode::Up), Some(&Action::CompletionUp));
        assert_eq!(set.completion.get(KeyModifiers::NONE, KeyCode::Down), Some(&Action::CompletionDown));
    }

    #[test]
    fn test_completion_accept() {
        let set = default_set();
        assert_eq!(set.completion.get(KeyModifiers::NONE, KeyCode::Enter), Some(&Action::CompletionAccept));
        assert_eq!(set.completion.get(KeyModifiers::NONE, KeyCode::Tab), Some(&Action::CompletionAccept));
    }

    #[test]
    fn test_completion_dismiss() {
        let set = default_set();
        assert_eq!(set.completion.get(KeyModifiers::NONE, KeyCode::Esc), Some(&Action::CompletionDismiss));
    }

    // -- Command palette bindings --

    #[test]
    fn test_palette_navigation() {
        let set = default_set();
        assert_eq!(set.command_palette.get(KeyModifiers::NONE, KeyCode::Up), Some(&Action::PaletteUp));
        assert_eq!(set.command_palette.get(KeyModifiers::NONE, KeyCode::Down), Some(&Action::PaletteDown));
    }

    #[test]
    fn test_palette_accept_dismiss() {
        let set = default_set();
        assert_eq!(set.command_palette.get(KeyModifiers::NONE, KeyCode::Enter), Some(&Action::PaletteAccept));
        assert_eq!(set.command_palette.get(KeyModifiers::NONE, KeyCode::Esc), Some(&Action::PaletteDismiss));
    }

    #[test]
    fn test_palette_backspace() {
        let set = default_set();
        assert_eq!(
            set.command_palette.get(KeyModifiers::NONE, KeyCode::Backspace),
            Some(&Action::PaletteBackspace)
        );
    }

    // -- Misc --

    #[test]
    fn test_keymap_for_returns_correct_map() {
        let set = default_set();
        // Verify that the returned reference points to the expected keymap by
        // checking a binding we know is in that keymap but not in the others.
        let km = set.keymap_for(FocusArea::Editor);
        assert!(km.get(KeyModifiers::NONE, KeyCode::Up).is_some());

        let km = set.keymap_for(FocusArea::Sidebar);
        assert_eq!(km.get(KeyModifiers::NONE, KeyCode::Enter), Some(&Action::TreeOpen));

        let km = set.keymap_for(FocusArea::CompletionPopup);
        assert_eq!(km.get(KeyModifiers::NONE, KeyCode::Esc), Some(&Action::CompletionDismiss));

        let km = set.keymap_for(FocusArea::CommandPalette);
        assert_eq!(km.get(KeyModifiers::NONE, KeyCode::Esc), Some(&Action::PaletteDismiss));
    }

    #[test]
    fn test_keymap_get_missing_returns_none() {
        let set = default_set();
        // F11 is not bound anywhere by default
        assert_eq!(set.global.get(KeyModifiers::NONE, KeyCode::F(11)), None);
        assert_eq!(set.editor.get(KeyModifiers::NONE, KeyCode::F(11)), None);
    }

    #[test]
    fn test_custom_bind_overrides() {
        let mut km = Keymap::new();
        km.bind(KeyModifiers::NONE, KeyCode::Char('a'), Action::Quit);
        assert_eq!(km.get(KeyModifiers::NONE, KeyCode::Char('a')), Some(&Action::Quit));
        // Override it
        km.bind(KeyModifiers::NONE, KeyCode::Char('a'), Action::SaveFile);
        assert_eq!(km.get(KeyModifiers::NONE, KeyCode::Char('a')), Some(&Action::SaveFile));
    }

    #[test]
    fn test_keymap_set_clone() {
        let set = default_set();
        let cloned = set.clone();
        assert_eq!(
            cloned.global.get(KeyModifiers::CONTROL, KeyCode::Char('q')),
            Some(&Action::Quit)
        );
    }
}
