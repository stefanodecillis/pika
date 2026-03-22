use std::path::PathBuf;

/// Every possible user action in Pika.
/// Input events are mapped to Actions via the keymap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    // -- Global --
    Quit,
    FocusNext,
    ToggleSidebar,
    OpenCommandPalette,
    OpenFileFinder,
    SaveFile,
    CloseTab,
    NextTab,
    PreviousTab,
    ProjectSearch,
    ShowShortcuts,

    // -- Editor: cursor movement --
    CursorUp,
    CursorDown,
    CursorLeft,
    CursorRight,
    CursorWordLeft,
    CursorWordRight,
    CursorLineStart,
    CursorLineEnd,
    CursorFileStart,
    CursorFileEnd,
    PageUp,
    PageDown,
    GoToLine,

    // -- Editor: text editing --
    InsertChar(char),
    InsertNewline,
    InsertTab,
    DeleteBackward,
    DeleteForward,
    DeleteWordBackward,
    DeleteWordForward,
    DeleteLine,

    // -- Editor: selection --
    SelectAll,
    SelectUp,
    SelectDown,
    SelectLeft,
    SelectRight,
    SelectWordLeft,
    SelectWordRight,
    SelectLineStart,
    SelectLineEnd,
    SelectNextOccurrence,

    // -- Editor: clipboard --
    Copy,
    Cut,
    Paste,
    PasteText(String),

    // -- Editor: history --
    Undo,
    Redo,

    // -- Editor: search --
    FindInFile,
    FindAndReplace,

    // -- Editor: LSP --
    TriggerCompletion,
    GotoDefinition,
    FindReferences,
    RenameSymbol,
    CodeAction,
    HoverInfo,
    FormatDocument,
    SignatureHelp,

    // -- Sidebar: navigation --
    TreeUp,
    TreeDown,
    TreeExpand,
    TreeCollapse,
    TreeOpen,

    // -- Sidebar: file operations --
    FileNew,
    DirNew,
    FileDelete,
    FileRename,
    FileCopy,
    FileCut,
    FilePaste,

    // -- Sidebar: drag-and-drop --
    FileDrop(Vec<PathBuf>),

    // -- Completion popup --
    CompletionUp,
    CompletionDown,
    CompletionAccept,
    CompletionDismiss,

    // -- Command palette --
    PaletteUp,
    PaletteDown,
    PaletteAccept,
    PaletteDismiss,
    PaletteInput(char),
    PaletteBackspace,

    // -- No-op --
    None,
}

/// Where keyboard focus currently is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FocusArea {
    Sidebar,
    Editor,
    CommandPalette,
    CompletionPopup,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_clone_eq() {
        let a = Action::InsertChar('x');
        let b = a.clone();
        assert_eq!(a, b);
    }

    #[test]
    fn test_focus_area_copy() {
        let f = FocusArea::Editor;
        let g = f;
        assert_eq!(f, g);
    }

    #[test]
    fn test_file_drop_action() {
        let paths = vec![
            PathBuf::from("/tmp/file1.rs"),
            PathBuf::from("/tmp/file2.rs"),
        ];
        let action = Action::FileDrop(paths.clone());
        if let Action::FileDrop(p) = action {
            assert_eq!(p.len(), 2);
        } else {
            panic!("Expected FileDrop");
        }
    }
}
