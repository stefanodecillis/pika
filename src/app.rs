use std::path::PathBuf;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::Terminal;
use tokio::sync::mpsc;

use crate::config::Settings;
use crate::events::{AppEvent, BackgroundCommand, FileChangeEvent, LspEvent};
use crate::input::{Action, FocusArea, InputHandler, KeymapSet};
use crate::ui::buffer::Buffer;
use crate::ui::command_palette::CommandPalette;
use crate::ui::editor::TabContent;
use crate::ui::completion::CompletionPopup;
use crate::ui::editor::EditorPane;
use crate::ui::confirm_dialog::{ConfirmAction, ConfirmDialog, ConfirmResult};
use crate::ui::shortcuts_help::ShortcutsHelp;
use crate::ui::sidebar::Sidebar;
use crate::ui::{AppCommand, Component};

/// The main application state.
pub struct App {
    pub root_dir: PathBuf,
    pub settings: Settings,
    pub sidebar: Sidebar,
    pub editor: EditorPane,
    pub command_palette: CommandPalette,
    pub completion: CompletionPopup,
    pub shortcuts_help: ShortcutsHelp,
    pub confirm_dialog: ConfirmDialog,
    pub input_handler: InputHandler,
    pub focus: FocusArea,
    pub running: bool,
    pub event_tx: mpsc::UnboundedSender<AppEvent>,
    pub event_rx: mpsc::UnboundedReceiver<AppEvent>,
    pub bg_tx: mpsc::UnboundedSender<BackgroundCommand>,
}

impl App {
    pub fn new(root_dir: PathBuf) -> Result<Self> {
        let settings = Settings::load().unwrap_or_default();
        let sidebar = Sidebar::new(&root_dir, settings.sidebar_width)?;
        let editor = EditorPane::new(settings.theme.clone());
        let command_palette = CommandPalette::new();
        let completion = CompletionPopup::new();
        let shortcuts_help = ShortcutsHelp::new();
        let confirm_dialog = ConfirmDialog::new();
        let input_handler = InputHandler::new(KeymapSet::default());

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (bg_tx, _bg_rx) = mpsc::unbounded_channel();

        Ok(Self {
            root_dir,
            settings,
            sidebar,
            editor,
            command_palette,
            completion,
            shortcuts_help,
            confirm_dialog,
            input_handler,
            focus: FocusArea::Sidebar,
            running: true,
            event_tx,
            event_rx,
            bg_tx,
        })
    }

    pub fn run(&mut self) -> Result<()> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = std::io::stdout();
        execute!(
            stdout,
            EnterAlternateScreen,
            crossterm::event::EnableBracketedPaste
        )?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;

        // Main loop
        while self.running {
            // Sync viewport to terminal size before rendering
            let term_size = terminal.size()?;
            let term_rect = ratatui::layout::Rect::new(0, 0, term_size.width, term_size.height);
            self.editor.sync_viewport(term_rect);


            // Render
            terminal.draw(|frame| {
                self.render(frame);
            })?;

            // Handle events with a small timeout for responsiveness
            if event::poll(Duration::from_millis(50))? {
                let event = event::read()?;
                self.handle_event(event);
            }

            // Process any pending app events (from background tasks)
            while let Ok(app_event) = self.event_rx.try_recv() {
                self.handle_app_event(app_event);
            }
        }

        // Restore terminal
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            crossterm::event::DisableBracketedPaste,
            LeaveAlternateScreen
        )?;
        terminal.show_cursor()?;

        Ok(())
    }

    fn handle_event(&mut self, event: Event) {
        // Determine the current focus area for input handling
        let focus = if self.command_palette.visible {
            FocusArea::CommandPalette
        } else if self.completion.visible {
            FocusArea::CompletionPopup
        } else {
            self.focus
        };

        let action = self.input_handler.handle_event(&event, focus);
        self.dispatch_action(action);
    }

    fn dispatch_action(&mut self, action: Action) {
        if action == Action::None {
            return;
        }

        // Confirm dialog captures all input when visible
        if self.confirm_dialog.visible {
            match &action {
                Action::CursorLeft | Action::TreeUp | Action::CompletionUp => {
                    self.confirm_dialog.select_previous();
                }
                Action::CursorRight | Action::TreeDown | Action::CompletionDown
                | Action::FocusNext => {
                    self.confirm_dialog.select_next();
                }
                Action::InsertNewline | Action::TreeOpen | Action::CompletionAccept => {
                    let result = self.confirm_dialog.accept();
                    let action = self.confirm_dialog.action.clone();
                    self.confirm_dialog.hide();
                    self.handle_confirm_result(result, action);
                }
                Action::CompletionDismiss | Action::PaletteDismiss => {
                    self.confirm_dialog.hide(); // Cancel
                }
                _ => {} // absorb everything else
            }
            return;
        }

        // Shortcuts help overlay captures most input when visible
        if self.shortcuts_help.visible {
            let consumed = self.shortcuts_help.handle_action(&action);
            if consumed {
                return;
            }
            // If not consumed (e.g. Quit), fall through
        }

        // Handle FileDrop globally — copy files directly
        if let Action::FileDrop(paths) = &action {
            let target_dir = self.sidebar.selected_parent_dir()
                .unwrap_or_else(|| self.root_dir.clone());
            for path in paths {
                if path.exists() {
                    let file_name = path.file_name().unwrap_or_default();
                    let dest = target_dir.join(file_name);
                    if path.is_dir() {
                        // For directories, use a recursive copy
                        let _ = Self::copy_dir_recursive(path, &dest);
                    } else {
                        let _ = std::fs::copy(path, &dest);
                    }
                }
            }
            self.sidebar.refresh();
            return;
        }

        // Handle global actions first
        let command = match &action {
            Action::Quit => AppCommand::Quit,
            Action::ToggleSidebar => AppCommand::ToggleSidebar,
            Action::FocusNext => AppCommand::FocusNext,
            Action::OpenCommandPalette | Action::OpenFileFinder => AppCommand::ShowCommandPalette,
            Action::SaveFile => AppCommand::SaveCurrentFile,
            Action::CloseTab => AppCommand::CloseCurrentTab,
            Action::ShowShortcuts => AppCommand::ShowShortcuts,

            // Command palette actions
            action if self.command_palette.visible => {
                self.command_palette.handle_action(action)
            }

            // Completion popup actions
            action if self.completion.visible => {
                let cmd = self.completion.handle_action(action);
                // If accepting a completion, insert the text
                if matches!(action, Action::CompletionAccept) {
                    if let Some(item) = self.completion.accept() {
                        let text = item.insert_text.clone();
                        self.completion.hide();
                        if let Some(buf) = self.editor.active_buffer_mut() {
                            for ch in text.chars() {
                                buf.insert_char(ch);
                            }
                        }
                    }
                }
                cmd
            }

            // Route to focused component
            _ => match self.focus {
                FocusArea::Sidebar => self.sidebar.handle_action(&action),
                FocusArea::Editor => self.editor.handle_action(&action),
                _ => AppCommand::Nothing,
            },
        };

        self.execute_command(command);
    }

    fn execute_command(&mut self, command: AppCommand) {
        match command {
            AppCommand::Quit => {
                // Check if any tab has unsaved changes
                if let Some(idx) = self.editor.tabs.iter().position(|t| t.is_modified()) {
                    let name = self.editor.tabs[idx].name();
                    self.confirm_dialog.show(name, ConfirmAction::Quit);
                } else {
                    self.running = false;
                }
            }
            AppCommand::ToggleSidebar => {
                self.sidebar.toggle_visibility();
                if !self.sidebar.visible && self.focus == FocusArea::Sidebar {
                    self.focus = FocusArea::Editor;
                }
            }
            AppCommand::FocusNext => {
                self.focus = match self.focus {
                    FocusArea::Sidebar if self.sidebar.visible => FocusArea::Editor,
                    FocusArea::Editor if self.sidebar.visible => FocusArea::Sidebar,
                    _ => self.focus,
                };
            }
            AppCommand::OpenFile(path) => {
                if let Err(e) = self.editor.open_file(&path) {
                    log::error!("Failed to open file: {}", e);
                }
                self.focus = FocusArea::Editor;
            }
            AppCommand::SaveCurrentFile => {
                if let Err(e) = self.editor.save_active_tab() {
                    log::error!("Failed to save file: {}", e);
                }
            }
            AppCommand::CloseCurrentTab => {
                if let Some(tab) = self.editor.tabs.get(self.editor.tab_bar.active) {
                    if tab.is_modified() {
                        let name = tab.name();
                        let idx = self.editor.tab_bar.active;
                        self.confirm_dialog.show(name, ConfirmAction::CloseTab(idx));
                        return;
                    }
                }
                self.editor.close_active_tab();
            }
            AppCommand::ShowCommandPalette => {
                self.command_palette.show(&self.root_dir);
            }
            AppCommand::FileDelete(path) => {
                let name = path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string_lossy().to_string());
                self.confirm_dialog.show(name, ConfirmAction::DeleteFile(path));
            }
            AppCommand::FileRename { from, to } => {
                let new_path = from.parent().unwrap_or(std::path::Path::new(".")).join(&to);
                let _ = std::fs::rename(&from, &new_path);
                self.sidebar.refresh();
            }
            AppCommand::FileNew(path) => {
                let _ = std::fs::write(&path, "");
                self.sidebar.refresh();
            }
            AppCommand::DirNew(path) => {
                let _ = std::fs::create_dir_all(&path);
                self.sidebar.refresh();
            }
            AppCommand::FileCopy(from) => {
                if let Some(target) = self.sidebar.selected_parent_dir() {
                    if let Some(name) = from.file_name() {
                        let dest = target.join(name);
                        if from.is_dir() {
                            let _ = Self::copy_dir_recursive(&from, &dest);
                        } else {
                            let _ = std::fs::copy(&from, &dest);
                        }
                    }
                }
                self.sidebar.refresh();
            }
            AppCommand::FileCut(from) => {
                if let Some(target) = self.sidebar.selected_parent_dir() {
                    if let Some(name) = from.file_name() {
                        let dest = target.join(name);
                        let _ = std::fs::rename(&from, &dest);
                    }
                }
                self.sidebar.refresh();
            }
            AppCommand::FilePaste(_target) => {
                // Paste is handled via FileCopy/FileCut
            }
            AppCommand::RequestCompletion
            | AppCommand::RequestHover
            | AppCommand::RequestGotoDefinition
            | AppCommand::RequestFindReferences
            | AppCommand::RequestCodeAction
            | AppCommand::RequestFormat
            | AppCommand::RequestSignatureHelp
            | AppCommand::RequestRename(_) => {
                // LSP requests will be handled when LSP is connected
            }
            AppCommand::ShowShortcuts => {
                self.shortcuts_help.toggle();
            }
            AppCommand::ShowFileFinder | AppCommand::ProjectSearch => {
                // Will be implemented with more advanced search
            }
            AppCommand::Nothing => {}
        }
    }

    fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
        std::fs::create_dir_all(dst)?;
        for entry in std::fs::read_dir(src)? {
            let entry = entry?;
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            if src_path.is_dir() {
                Self::copy_dir_recursive(&src_path, &dst_path)?;
            } else {
                std::fs::copy(&src_path, &dst_path)?;
            }
        }
        Ok(())
    }

    fn handle_confirm_result(&mut self, result: ConfirmResult, action: Option<ConfirmAction>) {
        let Some(action) = action else { return };
        match result {
            ConfirmResult::Save => {
                match action {
                    ConfirmAction::CloseTab(_idx) => {
                        let _ = self.editor.save_active_tab();
                        self.editor.close_active_tab();
                    }
                    ConfirmAction::Quit => {
                        for tab in &mut self.editor.tabs {
                            if tab.is_modified() {
                                let _ = tab.save();
                            }
                        }
                        self.running = false;
                    }
                    ConfirmAction::DeleteFile(path) => {
                        let _ = trash::delete(&path);
                        self.sidebar.refresh();
                    }
                }
            }
            ConfirmResult::DontSave => {
                match action {
                    ConfirmAction::CloseTab(_idx) => {
                        self.editor.close_active_tab();
                    }
                    ConfirmAction::Quit => {
                        self.running = false;
                    }
                    ConfirmAction::DeleteFile(_) => {
                        // User chose not to delete
                    }
                }
            }
            ConfirmResult::Cancel | ConfirmResult::Pending => {
                // Do nothing — stay in the editor
            }
        }
    }

    fn handle_app_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::FileChanged(change) => {
                self.sidebar.refresh();
                // Handle buffer reloads for modified files
                match change {
                    FileChangeEvent::Modified(path) => {
                        // Find and reload unmodified text buffers
                        for tab in &mut self.editor.tabs {
                            if let TabContent::Text(buf) = tab {
                                if buf.file_path() == Some(path.as_path()) && !buf.is_modified() {
                                    if let Ok(new_buf) = Buffer::from_file(&path) {
                                        *buf = new_buf;
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            AppEvent::Lsp(lsp_event) => match lsp_event {
                LspEvent::Completions(items) => {
                    if let Some(buf) = self.editor.active_buffer() {
                        let (cx, cy) = buf.cursor_screen_position();
                        self.completion.show_from_lsp(items, cx, cy);
                    }
                }
                LspEvent::Diagnostics { uri, diagnostics } => {
                    // Store diagnostics for rendering
                }
                _ => {}
            },
            AppEvent::FileOpComplete(result) => {
                self.sidebar.refresh();
            }
            AppEvent::Tick => {}
            AppEvent::Input(_) => {}
        }
    }

    fn render(&self, frame: &mut ratatui::Frame) {
        let size = frame.area();

        // Main layout: sidebar + editor
        let main_chunks = if self.sidebar.visible {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Length(self.sidebar.width),
                    Constraint::Min(1),
                ])
                .split(size)
        } else {
            Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(1)])
                .split(size)
        };

        // Render sidebar
        if self.sidebar.visible {
            self.sidebar.render(
                frame,
                main_chunks[0],
                self.focus == FocusArea::Sidebar,
            );
        }

        // Render editor
        let editor_area = if self.sidebar.visible {
            main_chunks[1]
        } else {
            main_chunks[0]
        };

        self.editor.render(
            frame,
            editor_area,
            self.focus == FocusArea::Editor,
        );

        // Render overlays
        if self.command_palette.visible {
            self.command_palette
                .render(frame, size, &self.settings.theme);
        }

        if self.completion.visible {
            self.completion.render(frame, editor_area, &self.settings.theme);
        }

        // Shortcuts help overlay (renders on top of everything)
        if self.shortcuts_help.visible {
            self.shortcuts_help.render(frame, size);
        }

        // Confirm dialog (renders on top of everything)
        if self.confirm_dialog.visible {
            self.confirm_dialog.render(frame, size);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    fn setup_test_app() -> (TempDir, App) {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("src")).unwrap();
        fs::write(tmp.path().join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(tmp.path().join("Cargo.toml"), "[package]").unwrap();
        let app = App::new(tmp.path().to_path_buf()).unwrap();
        (tmp, app)
    }

    #[test]
    fn test_app_new() {
        let (tmp, app) = setup_test_app();
        assert!(app.running);
        assert_eq!(app.focus, FocusArea::Sidebar);
        assert!(app.sidebar.visible);
    }

    #[test]
    fn test_toggle_sidebar() {
        let (tmp, mut app) = setup_test_app();
        assert!(app.sidebar.visible);
        app.execute_command(AppCommand::ToggleSidebar);
        assert!(!app.sidebar.visible);
        assert_eq!(app.focus, FocusArea::Editor);
    }

    #[test]
    fn test_focus_cycling() {
        let (tmp, mut app) = setup_test_app();
        assert_eq!(app.focus, FocusArea::Sidebar);
        app.execute_command(AppCommand::FocusNext);
        assert_eq!(app.focus, FocusArea::Editor);
        app.execute_command(AppCommand::FocusNext);
        assert_eq!(app.focus, FocusArea::Sidebar);
    }

    #[test]
    fn test_open_file() {
        let (tmp, mut app) = setup_test_app();
        let path = tmp.path().join("src/main.rs");
        app.execute_command(AppCommand::OpenFile(path));
        assert_eq!(app.editor.tabs.len(), 1);
        assert_eq!(app.focus, FocusArea::Editor);
    }

    #[test]
    fn test_quit() {
        let (tmp, mut app) = setup_test_app();
        assert!(app.running);
        app.execute_command(AppCommand::Quit);
        assert!(!app.running);
    }

    #[test]
    fn test_dispatch_action() {
        let (tmp, mut app) = setup_test_app();
        app.dispatch_action(Action::ToggleSidebar);
        assert!(!app.sidebar.visible);
    }

    #[test]
    fn test_command_palette_show() {
        let (tmp, mut app) = setup_test_app();
        app.execute_command(AppCommand::ShowCommandPalette);
        assert!(app.command_palette.visible);
    }

    #[test]
    fn test_close_tab_empty() {
        let (tmp, mut app) = setup_test_app();
        app.execute_command(AppCommand::CloseCurrentTab);
        assert!(app.editor.tabs.is_empty());
    }

    #[test]
    fn test_save_no_buffer() {
        let (tmp, mut app) = setup_test_app();
        // Should not panic
        app.execute_command(AppCommand::SaveCurrentFile);
    }

    #[test]
    fn test_sidebar_hidden_focus_stays_editor() {
        let (tmp, mut app) = setup_test_app();
        app.focus = FocusArea::Editor;
        app.execute_command(AppCommand::ToggleSidebar);
        assert_eq!(app.focus, FocusArea::Editor);
        // Focus next should not switch to sidebar when hidden
        app.execute_command(AppCommand::FocusNext);
        assert_eq!(app.focus, FocusArea::Editor);
    }

    #[test]
    fn test_dispatch_none_action() {
        let (tmp, mut app) = setup_test_app();
        // Should be a no-op
        app.dispatch_action(Action::None);
        assert!(app.running);
    }
}
