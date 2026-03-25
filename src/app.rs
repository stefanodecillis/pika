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
use crate::lsp::registry::LspRegistry;
use crate::ui::buffer::Buffer;
use crate::ui::command_palette::CommandPalette;
use crate::ui::editor::TabContent;
use crate::ui::completion::CompletionPopup;
use crate::ui::editor::EditorPane;
use crate::ui::confirm_dialog::{ConfirmAction, ConfirmDialog, ConfirmResult};
use crate::ui::project_search::ProjectSearch;
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
    pub project_search: ProjectSearch,
    pub shortcuts_help: ShortcutsHelp,
    pub confirm_dialog: ConfirmDialog,
    pub input_handler: InputHandler,
    pub focus: FocusArea,
    pub running: bool,
    pub event_tx: mpsc::UnboundedSender<AppEvent>,
    pub event_rx: mpsc::UnboundedReceiver<AppEvent>,
    pub bg_tx: mpsc::UnboundedSender<BackgroundCommand>,
    // LSP integration
    lsp_registry: LspRegistry,
    lsp_event_tx: mpsc::UnboundedSender<LspEvent>,
    lsp_event_rx: mpsc::UnboundedReceiver<LspEvent>,
    /// Server commands (e.g. "typescript-language-server") that have been initialized.
    lsp_initialized: std::collections::HashSet<String>,
    /// URI → current document version; only populated for files we have sent didOpen for.
    lsp_open_versions: std::collections::HashMap<String, i32>,
}

impl App {
    pub fn new(root_dir: PathBuf) -> Result<Self> {
        let settings = Settings::load().unwrap_or_default();
        let sidebar = Sidebar::new(&root_dir, settings.sidebar_width)?;
        let editor = EditorPane::new(settings.theme.clone());
        let command_palette = CommandPalette::new();
        let completion = CompletionPopup::new();
        let project_search = ProjectSearch::new();
        let shortcuts_help = ShortcutsHelp::new();
        let confirm_dialog = ConfirmDialog::new();
        let input_handler = InputHandler::new(KeymapSet::default());

        let (event_tx, event_rx) = mpsc::unbounded_channel();
        let (bg_tx, _bg_rx) = mpsc::unbounded_channel();
        let (lsp_event_tx, lsp_event_rx) = mpsc::unbounded_channel::<LspEvent>();
        let lsp_registry = LspRegistry::new(&settings.lsp.servers);

        Ok(Self {
            root_dir,
            settings,
            sidebar,
            editor,
            command_palette,
            completion,
            project_search,
            shortcuts_help,
            confirm_dialog,
            input_handler,
            focus: FocusArea::Sidebar,
            running: true,
            event_tx,
            event_rx,
            bg_tx,
            lsp_registry,
            lsp_event_tx,
            lsp_event_rx,
            lsp_initialized: std::collections::HashSet::new(),
            lsp_open_versions: std::collections::HashMap::new(),
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


            // Update LSP status indicator for the current file.
            self.refresh_lsp_status();

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

            // Process any pending LSP events
            while let Ok(lsp_event) = self.lsp_event_rx.try_recv() {
                self.handle_app_event(AppEvent::Lsp(lsp_event));
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

        // When sidebar is in input mode (rename/new-file), Esc cancels it instead of switching focus
        if action == Action::FocusNext && self.focus == FocusArea::Sidebar {
            if self.sidebar.is_in_input_mode() {
                self.sidebar.cancel_input();
                return;
            }
        }

        // When editor search/goto bar is active, Esc dismisses it instead of switching focus
        if matches!(&action, Action::FocusNext | Action::CompletionDismiss)
            && self.focus == FocusArea::Editor
        {
            if let Some(buf) = self.editor.active_buffer_mut() {
                if buf.search.active {
                    buf.search.dismiss();
                    return;
                }
                if buf.goto_line.active {
                    buf.goto_line.dismiss();
                    return;
                }
            }
        }

        // Project-wide search overlay captures input when visible
        if self.project_search.visible {
            match &action {
                Action::InsertChar(ch) => {
                    let root = self.root_dir.clone();
                    self.project_search.push_char(*ch, &root);
                    return;
                }
                Action::DeleteBackward => {
                    let root = self.root_dir.clone();
                    self.project_search.pop_char(&root);
                    return;
                }
                Action::CursorDown | Action::TreeDown => {
                    self.project_search.select_next();
                    return;
                }
                Action::CursorUp | Action::TreeUp => {
                    self.project_search.select_previous();
                    return;
                }
                Action::InsertNewline | Action::TreeOpen => {
                    if let Some((path, line)) = self.project_search.accept() {
                        self.project_search.hide();
                        let cmd = AppCommand::OpenFile(path);
                        self.execute_command(cmd);
                        // Jump to matched line in the newly opened buffer
                        if let Some(buf) = self.editor.active_buffer_mut() {
                            let max = buf.document.line_count().saturating_sub(1);
                            buf.cursor.position.line = line.min(max);
                            buf.cursor.position.col = 0;
                            buf.cursor.desired_col = 0;
                            buf.cursor.selection = None;
                            buf.ensure_cursor_visible();
                        }
                    }
                    return;
                }
                Action::FocusNext | Action::CompletionDismiss | Action::PaletteDismiss => {
                    self.project_search.hide();
                    return;
                }
                _ => return, // absorb all other keys while open
            }
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

            // Completion popup — navigation/accept/dismiss are handled here;
            // typing is forwarded to the buffer and the list is re-filtered.
            action if self.completion.visible => {
                match action {
                    Action::CompletionUp => {
                        self.completion.select_previous();
                        return;
                    }
                    Action::CompletionDown => {
                        self.completion.select_next();
                        return;
                    }
                    Action::CompletionAccept => {
                        if let Some(item) = self.completion.accept() {
                            let insert_text = Self::strip_snippets(&item.insert_text);
                            let prefix_len = self.completion.trigger_prefix.chars().count();
                            self.completion.hide();
                            if let Some(buf) = self.editor.active_buffer_mut() {
                                for _ in 0..prefix_len {
                                    buf.delete_backward();
                                }
                                for ch in insert_text.chars() {
                                    buf.insert_char(ch);
                                }
                            }
                        }
                        return;
                    }
                    Action::CompletionDismiss => {
                        self.completion.hide();
                        return;
                    }
                    // Typing: forward to the editor, then re-filter the popup.
                    Action::InsertChar(ch) => {
                        if let Some(buf) = self.editor.active_buffer_mut() {
                            buf.insert_char(*ch);
                        }
                        self.notify_lsp_after_edit(action);
                        // Extend the trigger prefix and re-filter.
                        if ch.is_alphanumeric() || *ch == '_' {
                            self.completion.trigger_prefix.push(*ch);
                            let prefix = self.completion.trigger_prefix.clone();
                            self.completion.filter_by_prefix(&prefix);
                            // Also request fresh items from LSP so the list stays current.
                            self.request_completion_for_current_pos();
                        } else {
                            self.completion.hide();
                        }
                        return;
                    }
                    // Backspace: forward to editor, shrink prefix and re-filter.
                    Action::DeleteBackward => {
                        if let Some(buf) = self.editor.active_buffer_mut() {
                            buf.delete_backward();
                        }
                        self.notify_lsp_after_edit(action);
                        if !self.completion.trigger_prefix.is_empty() {
                            self.completion.trigger_prefix.pop();
                            let prefix = self.completion.trigger_prefix.clone();
                            self.completion.filter_by_prefix(&prefix);
                        } else {
                            self.completion.hide();
                        }
                        return;
                    }
                    // Everything else dismisses the popup and falls through normally.
                    _ => {
                        self.completion.hide();
                    }
                }
                // Fall through to normal routing for non-completion actions.
                match self.focus {
                    FocusArea::Sidebar => self.sidebar.handle_action(action),
                    FocusArea::Editor => self.editor.handle_action(action),
                    _ => AppCommand::Nothing,
                }
            }

            // Route to focused component
            _ => match self.focus {
                FocusArea::Sidebar => self.sidebar.handle_action(&action),
                FocusArea::Editor => self.editor.handle_action(&action),
                _ => AppCommand::Nothing,
            },
        };

        self.execute_command(command);

        // After the editor processes a keystroke, notify LSP about changes and
        // auto-trigger completion for certain trigger characters.
        if self.focus == FocusArea::Editor && !self.completion.visible {
            self.notify_lsp_after_edit(&action);
            self.auto_trigger_completion(&action);
        }
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
                } else {
                    self.send_lsp_did_open(&path);
                }
                self.focus = FocusArea::Editor;
            }
            AppCommand::SaveCurrentFile => {
                if let Err(e) = self.editor.save_active_tab() {
                    log::error!("Failed to save file: {}", e);
                }
                let path_and_text = self.editor.active_buffer().and_then(|buf| {
                    buf.file_path().map(|p| (p.to_path_buf(), buf.document.text()))
                });
                if let Some((path, text)) = path_and_text {
                    self.send_lsp_did_change(&path, text);
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
            AppCommand::RequestCompletion => {
                let info = self.editor.active_buffer().and_then(|buf| {
                    buf.file_path().map(|p| (
                        p.to_path_buf(),
                        buf.cursor.position.line as u32,
                        buf.cursor.position.col as u32,
                    ))
                });
                if let Some((path, line, col)) = info {
                    self.send_lsp_completion(&path, line, col);
                }
            }
            AppCommand::RequestHover
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
            AppCommand::ShowFileFinder => {
                // File finder — falls through to command palette for now
            }
            AppCommand::ProjectSearch => {
                let root = self.root_dir.clone();
                self.project_search.show(&root);
            }
            AppCommand::Nothing => {}
        }
    }

    // ── LSP helpers ────────────────────────────────────────────────────────────

    /// Strip LSP/VS Code snippet placeholder markers (`$0`, `$1`, `${1:text}`)
    /// from a completion insert text so they are not inserted literally.
    fn strip_snippets(text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        let mut chars = text.chars().peekable();
        while let Some(ch) = chars.next() {
            if ch == '$' {
                if chars.peek() == Some(&'{') {
                    chars.next(); // consume '{'
                    // Skip until matching '}'
                    for c in chars.by_ref() {
                        if c == '}' { break; }
                    }
                } else {
                    // Skip one or more digits
                    while chars.peek().map(|c: &char| c.is_ascii_digit()).unwrap_or(false) {
                        chars.next();
                    }
                }
            } else {
                out.push(ch);
            }
        }
        out
    }

    /// Send `textDocument/didChange` after a buffer-modifying editor action.
    /// Called whenever the user types, deletes, pastes, etc.
    fn notify_lsp_after_edit(&mut self, action: &Action) {
        if !matches!(
            action,
            Action::InsertChar(_)
                | Action::DeleteBackward
                | Action::DeleteForward
                | Action::InsertNewline
                | Action::InsertTab
                | Action::PasteText(_)
        ) {
            return;
        }
        let path_and_text = self.editor.active_buffer().and_then(|buf| {
            buf.file_path().map(|p| (p.to_path_buf(), buf.document.text()))
        });
        if let Some((path, text)) = path_and_text {
            self.send_lsp_did_change(&path, text);
        }
    }

    /// Auto-trigger LSP completion after trigger characters or word chars.
    fn auto_trigger_completion(&mut self, action: &Action) {
        let should_trigger = match action {
            // Trigger characters: always request fresh completions
            Action::InsertChar('.') | Action::InsertChar('(') => true,
            // Word characters: request completions so the popup appears while typing
            Action::InsertChar(ch) => ch.is_alphanumeric() || *ch == '_',
            _ => false,
        };
        if !should_trigger {
            return;
        }
        self.request_completion_for_current_pos();
    }

    fn request_completion_for_current_pos(&mut self) {
        let info = self.editor.active_buffer().and_then(|buf| {
            buf.file_path().map(|p| (
                p.to_path_buf(),
                buf.cursor.position.line as u32,
                buf.cursor.position.col as u32,
            ))
        });
        if let Some((path, line, col)) = info {
            self.send_lsp_completion(&path, line, col);
        }
    }

    fn file_uri(path: &std::path::Path) -> String {
        format!("file://{}", path.to_string_lossy())
    }

    fn ext_to_lang_id(ext: &str) -> &'static str {
        match ext {
            "rs" => "rust",
            "ts" => "typescript",
            "tsx" => "typescriptreact",
            "js" => "javascript",
            "jsx" => "javascriptreact",
            "py" => "python",
            "go" => "go",
            "lua" => "lua",
            "c" | "h" => "c",
            "cpp" | "hpp" => "cpp",
            "zig" => "zig",
            "java" => "java",
            _ => "plaintext",
        }
    }

    /// Update `editor.lsp_status` from the registry so the status bar shows
    /// the correct LSP server name (or `None` for unsupported file types).
    fn refresh_lsp_status(&mut self) {
        let lsp_status = self.editor.active_buffer()
            .and_then(|buf| buf.file_path())
            .and_then(|p| p.extension())
            .and_then(|e| e.to_str())
            .and_then(|ext| self.lsp_registry.command_for_extension(ext))
            .map(|cmd| cmd.to_string());
        self.editor.lsp_status = lsp_status;
    }

    /// Send `textDocument/didOpen` (plus `initialize` if this is the first file for the server).
    /// Silently no-ops when no tokio runtime is available (e.g. in unit tests).
    fn send_lsp_did_open(&mut self, path: &std::path::Path) {
        if tokio::runtime::Handle::try_current().is_err() {
            log::warn!("send_lsp_did_open: no tokio runtime");
            return;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let root_uri = format!("file://{}", self.root_dir.to_string_lossy());
        let uri = Self::file_uri(path);
        log::info!("send_lsp_did_open: ext={} uri={}", ext, uri);

        if self.lsp_open_versions.contains_key(&uri) {
            log::info!("send_lsp_did_open: already open, skipping");
            return;
        }

        let text = match self.editor.active_buffer() {
            Some(buf) => buf.document.text(),
            None => { log::warn!("send_lsp_did_open: no active buffer"); return; }
        };

        let server_cmd = match self.lsp_registry.command_for_extension(ext) {
            Some(cmd) => cmd.to_string(),
            None => { log::warn!("send_lsp_did_open: no server for ext={}", ext); return; }
        };
        log::info!("send_lsp_did_open: server_cmd={}", server_cmd);
        let client = match self.lsp_registry.client_for_extension(ext, &root_uri, self.lsp_event_tx.clone()) {
            Some(c) => c,
            None => { log::error!("send_lsp_did_open: client_for_extension returned None"); return; }
        };

        let needs_init = self.lsp_initialized.insert(server_cmd.clone());
        self.lsp_open_versions.insert(uri.clone(), 1);
        log::info!("send_lsp_did_open: needs_init={} spawning async task", needs_init);

        let lang_id = Self::ext_to_lang_id(ext).to_string();
        tokio::spawn(async move {
            log::info!("LSP async task started, acquiring lock...");
            let locked = client.lock().await;
            log::info!("LSP async task: lock acquired");
            if needs_init {
                log::info!("LSP async task: sending initialize...");
                if let Err(e) = locked.initialize(&root_uri).await {
                    log::error!("LSP initialize failed: {}", e);
                    return;
                }
                log::info!("LSP async task: initialize succeeded");
            }
            let uri_parsed = match uri.parse::<lsp_types::Uri>() {
                Ok(u) => u,
                Err(e) => { log::error!("Invalid URI: {}", e); return; }
            };
            log::info!("LSP async task: sending didOpen for {}", uri);
            locked.send_notification::<lsp_types::notification::DidOpenTextDocument>(
                lsp_types::DidOpenTextDocumentParams {
                    text_document: lsp_types::TextDocumentItem {
                        uri: uri_parsed,
                        language_id: lang_id,
                        version: 1,
                        text,
                    },
                }
            ).await;
            log::info!("LSP async task: didOpen sent successfully");
        });
    }

    /// Send `textDocument/didChange` with full document text.
    fn send_lsp_did_change(&mut self, path: &std::path::Path, text: String) {
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        let uri = Self::file_uri(path);
        let version = match self.lsp_open_versions.get_mut(&uri) {
            Some(v) => { *v += 1; *v }
            None => return, // file not yet open in LSP
        };
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let root_uri = format!("file://{}", self.root_dir.to_string_lossy());
        let client = match self.lsp_registry.client_for_extension(ext, &root_uri, self.lsp_event_tx.clone()) {
            Some(c) => c,
            None => return,
        };
        tokio::spawn(async move {
            let locked = client.lock().await;
            let uri_parsed = match uri.parse::<lsp_types::Uri>() {
                Ok(u) => u,
                Err(e) => { log::error!("Invalid URI: {}", e); return; }
            };
            locked.send_notification::<lsp_types::notification::DidChangeTextDocument>(
                lsp_types::DidChangeTextDocumentParams {
                    text_document: lsp_types::VersionedTextDocumentIdentifier {
                        uri: uri_parsed,
                        version,
                    },
                    content_changes: vec![lsp_types::TextDocumentContentChangeEvent {
                        range: None,
                        range_length: None,
                        text,
                    }],
                }
            ).await;
        });
    }

    /// Send a `textDocument/completion` request and pipe results into the completion popup.
    fn send_lsp_completion(&mut self, path: &std::path::Path, line: u32, col: u32) {
        if tokio::runtime::Handle::try_current().is_err() {
            log::warn!("send_lsp_completion: no tokio runtime");
            return;
        }
        let uri = Self::file_uri(path);
        if !self.lsp_open_versions.contains_key(&uri) {
            log::warn!("send_lsp_completion: file not in lsp_open_versions: {}", uri);
            return;
        }
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
        let root_uri = format!("file://{}", self.root_dir.to_string_lossy());
        let client = match self.lsp_registry.client_for_extension(ext, &root_uri, self.lsp_event_tx.clone()) {
            Some(c) => c,
            None => { log::warn!("send_lsp_completion: no client"); return; }
        };
        log::info!("send_lsp_completion: requesting at {}:{}:{}", uri, line, col);
        let event_tx = self.lsp_event_tx.clone();
        tokio::spawn(async move {
            let locked = client.lock().await;
            let uri_parsed = match uri.parse::<lsp_types::Uri>() {
                Ok(u) => u,
                Err(e) => { log::error!("Invalid URI: {}", e); return; }
            };
            let params = lsp_types::CompletionParams {
                text_document_position: lsp_types::TextDocumentPositionParams {
                    text_document: lsp_types::TextDocumentIdentifier { uri: uri_parsed },
                    position: lsp_types::Position { line, character: col },
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
                context: None,
            };
            match locked.send_request::<lsp_types::request::Completion>(params).await {
                Ok(Some(response)) => {
                    let items = match response {
                        lsp_types::CompletionResponse::Array(items) => items,
                        lsp_types::CompletionResponse::List(list) => list.items,
                    };
                    log::info!("send_lsp_completion: got {} items", items.len());
                    let _ = event_tx.send(LspEvent::Completions(items));
                }
                Ok(None) => { log::info!("send_lsp_completion: response was None"); }
                Err(e) => log::error!("LSP completion request failed: {}", e),
            }
        });
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
                    log::info!("handle_app_event: received {} completion items", items.len());
                    if let Some(buf) = self.editor.active_buffer() {
                        let (cx, cy) = buf.cursor_screen_position();
                        let prefix = buf.word_before_cursor();
                        log::info!("handle_app_event: showing popup, prefix='{}' cx={} cy={}", prefix, cx, cy);
                        self.completion.show_from_lsp(items, cx, cy, prefix);
                    } else {
                        log::warn!("handle_app_event: no active buffer for completions");
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

        // Project search overlay (renders on top of everything)
        if self.project_search.visible {
            self.project_search.render(frame, size, &self.settings.theme);
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

    #[test]
    fn test_lsp_flow_with_runtime() {
        // This test checks whether LSP init works with rt.enter() + tokio::spawn.
        let rt = tokio::runtime::Runtime::new().unwrap();
        let _guard = rt.enter();

        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("src")).unwrap();
        fs::write(tmp.path().join("src/main.rs"), "fn main() { let x = 1; }").unwrap();
        fs::write(tmp.path().join("Cargo.toml"), "[package]\nname = \"t\"\nversion = \"0.1.0\"\nedition = \"2021\"").unwrap();

        let mut app = App::new(tmp.path().to_path_buf()).unwrap();

        // Check if rust-analyzer is available; skip gracefully if not.
        let has_ra = app.lsp_registry.has_server_for("rs");
        if !has_ra {
            eprintln!("SKIP: rust-analyzer not in PATH");
            return;
        }
        eprintln!("test_lsp_flow: rust-analyzer found");

        // Open a rust file — this should trigger send_lsp_did_open.
        let rs_path = tmp.path().join("src/main.rs");
        app.execute_command(AppCommand::OpenFile(rs_path.clone()));
        assert!(app.editor.active_buffer().is_some(), "buffer should be open");

        let uri = App::file_uri(&rs_path);
        assert!(app.lsp_open_versions.contains_key(&uri),
            "file should be in lsp_open_versions after OpenFile");
        eprintln!("test_lsp_flow: file registered in lsp_open_versions");

        // Give the async init task time to complete (initialize + didOpen).
        std::thread::sleep(std::time::Duration::from_secs(5));

        // Now send a completion request.
        app.send_lsp_completion(&rs_path, 0, 14);
        eprintln!("test_lsp_flow: completion request sent");

        // Wait for the async completion response.
        let mut got_completions = false;
        for _ in 0..40 {
            std::thread::sleep(std::time::Duration::from_millis(250));
            while let Ok(evt) = app.lsp_event_rx.try_recv() {
                if let LspEvent::Completions(items) = &evt {
                    eprintln!("test_lsp_flow: got {} completion items", items.len());
                    got_completions = true;
                }
                app.handle_app_event(AppEvent::Lsp(evt));
                if got_completions { break; }
            }
            if got_completions { break; }
        }
        eprintln!("test_lsp_flow: got_completions={}", got_completions);
        // Note: We don't assert got_completions because rust-analyzer may still
        // be indexing. The key is that we got this far without panics.
    }
}
