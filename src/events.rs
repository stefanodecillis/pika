use std::path::PathBuf;

/// Messages sent from background tasks to the main UI thread.
#[derive(Debug)]
pub enum AppEvent {
    /// Terminal input event from crossterm
    Input(crossterm::event::Event),

    /// File system change detected by watcher
    FileChanged(FileChangeEvent),

    /// LSP server sent a notification or response
    Lsp(LspEvent),

    /// A file operation completed
    FileOpComplete(FileOpResult),

    /// Tick for periodic UI updates (cursor blink, etc.)
    Tick,
}

#[derive(Debug)]
pub enum FileChangeEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Deleted(PathBuf),
    Renamed { from: PathBuf, to: PathBuf },
}

#[derive(Debug)]
pub enum LspEvent {
    /// Diagnostics for a file
    Diagnostics {
        uri: String,
        diagnostics: Vec<lsp_types::Diagnostic>,
    },
    /// Completion response
    Completions(Vec<lsp_types::CompletionItem>),
    /// Hover response
    Hover(Option<lsp_types::Hover>),
    /// Go-to-definition response
    Definition(Option<lsp_types::GotoDefinitionResponse>),
    /// References response
    References(Vec<lsp_types::Location>),
    /// Code actions response
    CodeActions(Vec<lsp_types::CodeActionOrCommand>),
    /// Signature help response
    SignatureHelp(Option<lsp_types::SignatureHelp>),
    /// Server started for a language
    ServerStarted(String),
    /// Server stopped or crashed
    ServerStopped(String),
    /// Formatting response
    Formatting(Vec<lsp_types::TextEdit>),
    /// Rename response
    WorkspaceEdit(Option<lsp_types::WorkspaceEdit>),
}

#[derive(Debug)]
pub enum FileOpResult {
    Copied { from: PathBuf, to: PathBuf },
    Moved { from: PathBuf, to: PathBuf },
    Deleted(PathBuf),
    Renamed { from: PathBuf, to: PathBuf },
    Error(String),
}

/// Commands sent from UI to background tasks.
#[derive(Debug)]
pub enum BackgroundCommand {
    /// Request LSP operation
    Lsp(LspCommand),
    /// Request file operation
    FileOp(FileOpCommand),
    /// Watch a new directory
    WatchDir(PathBuf),
    /// Shutdown all background tasks
    Shutdown,
}

#[derive(Debug)]
pub enum LspCommand {
    Initialize { root_uri: String },
    Completion { uri: String, position: lsp_types::Position },
    Hover { uri: String, position: lsp_types::Position },
    GotoDefinition { uri: String, position: lsp_types::Position },
    References { uri: String, position: lsp_types::Position },
    Rename { uri: String, position: lsp_types::Position, new_name: String },
    CodeAction { uri: String, range: lsp_types::Range },
    Format { uri: String },
    DidOpen { uri: String, language_id: String, text: String },
    DidChange { uri: String, text: String, version: i32 },
    DidClose { uri: String },
    SignatureHelp { uri: String, position: lsp_types::Position },
}

#[derive(Debug)]
pub enum FileOpCommand {
    Copy { from: PathBuf, to: PathBuf },
    Move { from: PathBuf, to: PathBuf },
    Delete(PathBuf),
    Rename { from: PathBuf, to: PathBuf },
    CreateFile(PathBuf),
    CreateDir(PathBuf),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_event_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<AppEvent>();
    }

    #[test]
    fn test_background_command_is_send() {
        fn assert_send<T: Send>() {}
        assert_send::<BackgroundCommand>();
    }

    #[test]
    fn test_file_change_event_variants() {
        let event = FileChangeEvent::Created(PathBuf::from("/test"));
        assert!(matches!(event, FileChangeEvent::Created(_)));

        let event = FileChangeEvent::Renamed {
            from: PathBuf::from("/a"),
            to: PathBuf::from("/b"),
        };
        assert!(matches!(event, FileChangeEvent::Renamed { .. }));
    }
}
