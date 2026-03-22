/// Document model backed by a `ropey::Rope`, with file I/O and text manipulation.
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use ropey::Rope;

use super::cursor::Position;

/// A text document backed by a Rope data structure for efficient editing.
#[derive(Debug)]
pub struct Document {
    rope: Rope,
    pub file_path: Option<PathBuf>,
    pub modified: bool,
    pub language_id: String,
    pub version: i32,
}

impl Document {
    // ── Constructors ────────────────────────────────────────────────

    /// Open a file from disk. The language is detected from the file extension.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let text = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read file: {}", path.display()))?;
        let rope = Rope::from_str(&text);
        let language_id = detect_language_id(path);
        Ok(Self {
            rope,
            file_path: Some(path.to_path_buf()),
            modified: false,
            language_id,
            version: 0,
        })
    }

    /// Create a document from an in-memory string with no associated file path.
    pub fn from_text(text: &str) -> Self {
        Self {
            rope: Rope::from_str(text),
            file_path: None,
            modified: false,
            language_id: "plaintext".to_string(),
            version: 0,
        }
    }

    // ── Persistence ─────────────────────────────────────────────────

    /// Save the document to its current file path. Returns an error if no path is set.
    pub fn save(&mut self) -> Result<()> {
        let path = self
            .file_path
            .clone()
            .context("No file path set for this document")?;
        self.write_to_file(&path)?;
        self.modified = false;
        self.version += 1;
        Ok(())
    }

    /// Save the document to the given path, updating the stored file path and
    /// language id.
    pub fn save_as<P: AsRef<Path>>(&mut self, path: P) -> Result<()> {
        let path = path.as_ref();
        self.write_to_file(path)?;
        self.language_id = detect_language_id(path);
        self.file_path = Some(path.to_path_buf());
        self.modified = false;
        self.version += 1;
        Ok(())
    }

    fn write_to_file(&self, path: &Path) -> Result<()> {
        let text = self.rope.to_string();
        std::fs::write(path, &text)
            .with_context(|| format!("Failed to write file: {}", path.display()))?;
        Ok(())
    }

    // ── Text manipulation ───────────────────────────────────────────

    /// Insert a single character at `pos`.
    pub fn insert_char(&mut self, pos: Position, ch: char) {
        let idx = self.pos_to_char_idx(pos);
        self.rope.insert_char(idx, ch);
        self.modified = true;
    }

    /// Insert a string at `pos`.
    pub fn insert_text(&mut self, pos: Position, text: &str) {
        let idx = self.pos_to_char_idx(pos);
        self.rope.insert(idx, text);
        self.modified = true;
    }

    /// Delete the text between `start` (inclusive) and `end` (exclusive).
    pub fn delete_range(&mut self, start: Position, end: Position) {
        let start_idx = self.pos_to_char_idx(start);
        let end_idx = self.pos_to_char_idx(end);
        if start_idx < end_idx {
            self.rope.remove(start_idx..end_idx);
            self.modified = true;
        }
    }

    // ── Queries ─────────────────────────────────────────────────────

    /// Returns the content of line `n` (0-indexed) as a `String`.
    ///
    /// Returns an empty string if `n` is out of bounds.
    pub fn line(&self, n: usize) -> String {
        if n >= self.rope.len_lines() {
            return String::new();
        }
        self.rope.line(n).to_string()
    }

    /// Total number of lines in the document.
    pub fn line_count(&self) -> usize {
        self.rope.len_lines()
    }

    /// Length of line `n` in characters, excluding the trailing newline.
    ///
    /// Returns 0 if `n` is out of bounds.
    pub fn line_len(&self, n: usize) -> usize {
        if n >= self.rope.len_lines() {
            return 0;
        }
        let line = self.rope.line(n);
        let len = line.len_chars();
        // Strip trailing newline character(s) from the count.
        if len > 0 {
            let last = line.char(len - 1);
            if last == '\n' {
                if len > 1 && line.char(len - 2) == '\r' {
                    len - 2
                } else {
                    len - 1
                }
            } else {
                len
            }
        } else {
            0
        }
    }

    /// Returns the character at the given position, or `None` if out of bounds.
    pub fn char_at(&self, pos: Position) -> Option<char> {
        if pos.line >= self.rope.len_lines() {
            return None;
        }
        let line = self.rope.line(pos.line);
        if pos.col >= line.len_chars() {
            return None;
        }
        Some(line.char(pos.col))
    }

    /// Returns the full document text as a `String`.
    pub fn text(&self) -> String {
        self.rope.to_string()
    }

    /// Returns a reference to the underlying `Rope`.
    pub fn rope(&self) -> &Rope {
        &self.rope
    }

    // ── Internal helpers ────────────────────────────────────────────

    /// Convert a `Position` to a rope char index.
    fn pos_to_char_idx(&self, pos: Position) -> usize {
        let line_start = self.rope.line_to_char(pos.line);
        line_start + pos.col
    }
}

/// Detect a language identifier from a file path's extension.
pub fn detect_language_id(path: &Path) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    match ext.as_str() {
        "rs" => "rust",
        "py" => "python",
        "js" => "javascript",
        "jsx" => "javascriptreact",
        "ts" => "typescript",
        "tsx" => "typescriptreact",
        "c" => "c",
        "cpp" | "cc" | "cxx" => "cpp",
        "h" | "hpp" => "cpp",
        "go" => "go",
        "java" => "java",
        "rb" => "ruby",
        "sh" | "bash" | "zsh" => "shellscript",
        "json" => "json",
        "yaml" | "yml" => "yaml",
        "toml" => "toml",
        "xml" => "xml",
        "html" | "htm" => "html",
        "css" => "css",
        "scss" => "scss",
        "md" | "markdown" => "markdown",
        "sql" => "sql",
        "lua" => "lua",
        "swift" => "swift",
        "kt" | "kts" => "kotlin",
        "zig" => "zig",
        "r" => "r",
        "dart" => "dart",
        "ex" | "exs" => "elixir",
        "erl" | "hrl" => "erlang",
        "hs" => "haskell",
        "ml" | "mli" => "ocaml",
        "txt" | "text" => "plaintext",
        _ => "plaintext",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── from_text / basic queries ───────────────────────────────────

    #[test]
    fn from_text_basic() {
        let doc = Document::from_text("hello\nworld\n");
        assert_eq!(doc.line_count(), 3); // ropey counts trailing empty line
        assert_eq!(doc.line(0), "hello\n");
        assert_eq!(doc.line(1), "world\n");
        assert!(!doc.modified);
        assert_eq!(doc.language_id, "plaintext");
    }

    #[test]
    fn from_text_empty() {
        let doc = Document::from_text("");
        assert_eq!(doc.line_count(), 1); // ropey always has at least 1 line
        assert_eq!(doc.text(), "");
    }

    #[test]
    fn line_out_of_bounds_returns_empty() {
        let doc = Document::from_text("one\ntwo\n");
        assert_eq!(doc.line(999), "");
    }

    #[test]
    fn line_len_excludes_newline() {
        let doc = Document::from_text("abc\nde\n\n");
        assert_eq!(doc.line_len(0), 3); // "abc"
        assert_eq!(doc.line_len(1), 2); // "de"
        assert_eq!(doc.line_len(2), 0); // empty line (just '\n')
    }

    #[test]
    fn line_len_last_line_no_newline() {
        let doc = Document::from_text("abc\ndef");
        assert_eq!(doc.line_len(0), 3);
        assert_eq!(doc.line_len(1), 3); // no trailing newline
    }

    #[test]
    fn line_len_out_of_bounds() {
        let doc = Document::from_text("hi");
        assert_eq!(doc.line_len(100), 0);
    }

    #[test]
    fn char_at_valid() {
        let doc = Document::from_text("abcdef");
        assert_eq!(doc.char_at(Position::new(0, 0)), Some('a'));
        assert_eq!(doc.char_at(Position::new(0, 5)), Some('f'));
    }

    #[test]
    fn char_at_out_of_bounds() {
        let doc = Document::from_text("ab\ncd");
        assert_eq!(doc.char_at(Position::new(0, 10)), None);
        assert_eq!(doc.char_at(Position::new(5, 0)), None);
    }

    #[test]
    fn rope_returns_reference() {
        let doc = Document::from_text("test");
        assert_eq!(doc.rope().len_chars(), 4);
    }

    // ── insert_char ─────────────────────────────────────────────────

    #[test]
    fn insert_char_at_start() {
        let mut doc = Document::from_text("ello");
        doc.insert_char(Position::new(0, 0), 'H');
        assert_eq!(doc.text(), "Hello");
        assert!(doc.modified);
    }

    #[test]
    fn insert_char_at_end() {
        let mut doc = Document::from_text("Hell");
        doc.insert_char(Position::new(0, 4), 'o');
        assert_eq!(doc.text(), "Hello");
    }

    #[test]
    fn insert_char_newline() {
        let mut doc = Document::from_text("ab");
        doc.insert_char(Position::new(0, 1), '\n');
        assert_eq!(doc.text(), "a\nb");
        assert_eq!(doc.line_count(), 2);
    }

    // ── insert_text ─────────────────────────────────────────────────

    #[test]
    fn insert_text_middle() {
        let mut doc = Document::from_text("heo");
        doc.insert_text(Position::new(0, 2), "ll");
        assert_eq!(doc.text(), "hello");
    }

    #[test]
    fn insert_text_multiline() {
        let mut doc = Document::from_text("ac");
        doc.insert_text(Position::new(0, 1), "\nb\n");
        assert_eq!(doc.text(), "a\nb\nc");
        assert_eq!(doc.line_count(), 3);
    }

    // ── delete_range ────────────────────────────────────────────────

    #[test]
    fn delete_range_single_line() {
        let mut doc = Document::from_text("hello world");
        doc.delete_range(Position::new(0, 5), Position::new(0, 11));
        assert_eq!(doc.text(), "hello");
    }

    #[test]
    fn delete_range_multiline() {
        let mut doc = Document::from_text("aaa\nbbb\nccc");
        doc.delete_range(Position::new(0, 1), Position::new(2, 1));
        assert_eq!(doc.text(), "acc");
    }

    #[test]
    fn delete_range_empty_is_noop() {
        let mut doc = Document::from_text("hello");
        doc.delete_range(Position::new(0, 2), Position::new(0, 2));
        assert_eq!(doc.text(), "hello");
        assert!(!doc.modified);
    }

    // ── detect_language_id ──────────────────────────────────────────

    #[test]
    fn detect_language_rust() {
        assert_eq!(detect_language_id(Path::new("main.rs")), "rust");
    }

    #[test]
    fn detect_language_python() {
        assert_eq!(detect_language_id(Path::new("script.py")), "python");
    }

    #[test]
    fn detect_language_javascript() {
        assert_eq!(detect_language_id(Path::new("app.js")), "javascript");
    }

    #[test]
    fn detect_language_typescript() {
        assert_eq!(detect_language_id(Path::new("app.ts")), "typescript");
    }

    #[test]
    fn detect_language_tsx() {
        assert_eq!(
            detect_language_id(Path::new("component.tsx")),
            "typescriptreact"
        );
    }

    #[test]
    fn detect_language_unknown() {
        assert_eq!(detect_language_id(Path::new("data.xyz")), "plaintext");
    }

    #[test]
    fn detect_language_no_extension() {
        assert_eq!(detect_language_id(Path::new("Makefile")), "plaintext");
    }

    #[test]
    fn detect_language_go() {
        assert_eq!(detect_language_id(Path::new("main.go")), "go");
    }

    #[test]
    fn detect_language_shell() {
        assert_eq!(detect_language_id(Path::new("run.sh")), "shellscript");
    }

    // ── File I/O tests (tempfile) ───────────────────────────────────

    #[test]
    fn open_and_read() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.rs");
        std::fs::write(&file_path, "fn main() {}").unwrap();

        let doc = Document::open(&file_path).unwrap();
        assert_eq!(doc.text(), "fn main() {}");
        assert_eq!(doc.language_id, "rust");
        assert!(!doc.modified);
        assert_eq!(doc.file_path.unwrap(), file_path);
    }

    #[test]
    fn open_nonexistent_file_errors() {
        let result = Document::open("/nonexistent/file.txt");
        assert!(result.is_err());
    }

    #[test]
    fn save_to_existing_path() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("output.txt");
        std::fs::write(&file_path, "original").unwrap();

        let mut doc = Document::open(&file_path).unwrap();
        doc.insert_text(Position::new(0, 8), " modified");
        assert!(doc.modified);

        doc.save().unwrap();
        assert!(!doc.modified);
        assert_eq!(doc.version, 1);

        let contents = std::fs::read_to_string(&file_path).unwrap();
        assert_eq!(contents, "original modified");
    }

    #[test]
    fn save_without_path_errors() {
        let mut doc = Document::from_text("no path");
        assert!(doc.save().is_err());
    }

    #[test]
    fn save_as_new_path() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = Document::from_text("hello save_as");
        let target = dir.path().join("new_file.py");

        doc.save_as(&target).unwrap();
        assert_eq!(doc.file_path.as_deref(), Some(target.as_path()));
        assert_eq!(doc.language_id, "python");
        assert!(!doc.modified);
        assert_eq!(doc.version, 1);

        let contents = std::fs::read_to_string(&target).unwrap();
        assert_eq!(contents, "hello save_as");
    }

    #[test]
    fn save_as_updates_language_id() {
        let dir = tempfile::tempdir().unwrap();
        let mut doc = Document::from_text("code");
        assert_eq!(doc.language_id, "plaintext");

        let target = dir.path().join("file.ts");
        doc.save_as(&target).unwrap();
        assert_eq!(doc.language_id, "typescript");
    }

    #[test]
    fn version_increments_on_save() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("v.txt");
        std::fs::write(&path, "v0").unwrap();

        let mut doc = Document::open(&path).unwrap();
        assert_eq!(doc.version, 0);

        doc.insert_char(Position::new(0, 2), '!');
        doc.save().unwrap();
        assert_eq!(doc.version, 1);

        doc.insert_char(Position::new(0, 3), '!');
        doc.save().unwrap();
        assert_eq!(doc.version, 2);
    }

    // ── Unicode handling ────────────────────────────────────────────

    #[test]
    fn unicode_insert_and_query() {
        let mut doc = Document::from_text("cafe");
        doc.insert_char(Position::new(0, 4), '\u{0301}'); // combining accent
        // The text should have 5 chars now
        assert_eq!(doc.rope().len_chars(), 5);
    }

    #[test]
    fn multibyte_chars() {
        let doc = Document::from_text("日本語");
        assert_eq!(doc.line_len(0), 3); // 3 characters
        assert_eq!(doc.char_at(Position::new(0, 0)), Some('日'));
        assert_eq!(doc.char_at(Position::new(0, 2)), Some('語'));
    }
}
