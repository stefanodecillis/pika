pub mod buffer;
pub mod command_palette;
pub mod completion;
pub mod confirm_dialog;
pub mod csv_view;
pub mod editor;
pub mod shortcuts_help;
pub mod sidebar;
pub mod status_bar;
pub mod tab_bar;

use ratatui::Frame;
use ratatui::layout::Rect;

use crate::input::Action;

/// A command the UI sends back to the App after handling an action.
#[derive(Debug)]
pub enum AppCommand {
    /// Open a file in the editor
    OpenFile(std::path::PathBuf),
    /// Save the current file
    SaveCurrentFile,
    /// Close the current tab
    CloseCurrentTab,
    /// Quit the application
    Quit,
    /// Toggle sidebar visibility
    ToggleSidebar,
    /// Switch focus to next pane
    FocusNext,
    /// Trigger LSP completion at cursor
    RequestCompletion,
    /// Trigger LSP hover at cursor
    RequestHover,
    /// Trigger LSP go-to-definition
    RequestGotoDefinition,
    /// Trigger LSP find-references
    RequestFindReferences,
    /// Trigger LSP rename
    RequestRename(String),
    /// Trigger LSP code actions
    RequestCodeAction,
    /// Trigger LSP format
    RequestFormat,
    /// Trigger LSP signature help
    RequestSignatureHelp,
    /// File operation: copy
    FileCopy(std::path::PathBuf),
    /// File operation: cut (move)
    FileCut(std::path::PathBuf),
    /// File operation: paste into directory
    FilePaste(std::path::PathBuf),
    /// File operation: delete
    FileDelete(std::path::PathBuf),
    /// File operation: rename
    FileRename { from: std::path::PathBuf, to: String },
    /// File operation: create new file
    FileNew(std::path::PathBuf),
    /// File operation: create new directory
    DirNew(std::path::PathBuf),
    /// Show command palette
    ShowCommandPalette,
    /// Show file finder
    ShowFileFinder,
    /// Project-wide search
    ProjectSearch,
    /// Show keyboard shortcuts help
    ShowShortcuts,
    /// No action needed
    Nothing,
}

/// Trait for all UI components.
pub trait Component {
    /// Handle a user action, return optional command for the app.
    fn handle_action(&mut self, action: &Action) -> AppCommand;

    /// Render into the given Ratatui frame area.
    fn render(&self, frame: &mut Frame, area: Rect, focused: bool);
}
