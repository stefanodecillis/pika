/// Editor engine: cursor, document, history, clipboard, and syntax highlighting.

pub mod clipboard;
pub mod cursor;
pub mod document;
pub mod history;
pub mod syntax;

// Re-export key types for convenient access.
pub use clipboard::Clipboard;
pub use cursor::{CursorState, Position, Selection};
pub use document::Document;
pub use history::{Edit, UndoHistory};
pub use syntax::{HighlightStyle, HighlightedSpan, SyntaxHighlighter};
