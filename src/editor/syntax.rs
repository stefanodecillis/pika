/// Syntax highlighting powered by syntect.
use syntect::easy::HighlightLines;
use syntect::highlighting::{ThemeSet, Style};
use syntect::parsing::SyntaxSet;
use syntect::util::LinesWithEndings;

/// An RGB foreground color.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighlightStyle {
    pub fg: (u8, u8, u8),
}

impl HighlightStyle {
    pub fn new(r: u8, g: u8, b: u8) -> Self {
        Self { fg: (r, g, b) }
    }

    /// Create from a syntect `Style`.
    fn from_syntect(style: &Style) -> Self {
        Self {
            fg: (style.foreground.r, style.foreground.g, style.foreground.b),
        }
    }
}

/// A span of highlighted text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HighlightedSpan {
    pub style: HighlightStyle,
    pub text: String,
}

impl HighlightedSpan {
    pub fn new(style: HighlightStyle, text: String) -> Self {
        Self { style, text }
    }
}

/// Syntax highlighter backed by syntect.
pub struct SyntaxHighlighter {
    syntax_set: SyntaxSet,
    theme_set: ThemeSet,
}

impl SyntaxHighlighter {
    /// Create a new highlighter with the default syntax and theme sets.
    pub fn new() -> Self {
        Self {
            syntax_set: SyntaxSet::load_defaults_newlines(),
            theme_set: ThemeSet::load_defaults(),
        }
    }

    /// Resolve the best syntect `SyntaxReference` for a given file extension,
    /// with language-specific fallbacks for extensions that may not be present
    /// in the default bundle (e.g. TypeScript → JavaScript).
    fn resolve_syntax(&self, ext: &str) -> &syntect::parsing::SyntaxReference {
        // First try direct lookup by extension.
        if let Some(s) = self.syntax_set.find_syntax_by_extension(ext) {
            return s;
        }
        // Try known language names for extensions that syntect may index by
        // full name rather than by file extension.
        let language_name: Option<&str> = match ext {
            "ts" => Some("TypeScript"),
            "tsx" => Some("TypeScript React"),
            "jsx" => Some("JSX"),
            "mjs" | "cjs" => Some("JavaScript"),
            "toml" => Some("TOML"),
            "yaml" | "yml" => Some("YAML"),
            _ => None,
        };
        if let Some(name) = language_name {
            if let Some(s) = self.syntax_set.find_syntax_by_name(name) {
                return s;
            }
        }
        // Language-specific fallbacks (e.g. TypeScript → JavaScript for basic
        // keyword/string colouring when the TS grammar is not bundled).
        let fallback_ext: Option<&str> = match ext {
            "ts" | "tsx" | "jsx" | "mjs" | "cjs" => Some("js"),
            "toml" => Some("ini"),
            _ => None,
        };
        if let Some(fb) = fallback_ext {
            if let Some(s) = self.syntax_set.find_syntax_by_extension(fb) {
                return s;
            }
        }
        self.syntax_set.find_syntax_plain_text()
    }

    /// Highlight a single line of text using the given syntax name.
    ///
    /// `syntax_name` should be a file extension (e.g. `"rs"`, `"py"`) or a
    /// syntax name recognized by syntect.
    ///
    /// Returns a list of `HighlightedSpan`s. If the syntax is not found, the
    /// entire line is returned as a single span with a default foreground
    /// color.
    pub fn highlight_line(&self, line: &str, syntax_name: &str) -> Vec<HighlightedSpan> {
        let syntax = self.resolve_syntax(syntax_name);

        let theme = self
            .theme_set
            .themes
            .get("base16-ocean.dark")
            .expect("base16-ocean.dark theme must be available");

        let mut highlighter = HighlightLines::new(syntax, theme);

        // syntect expects lines to end with '\n' for proper parsing.
        let line_with_newline;
        let input = if line.ends_with('\n') {
            line
        } else {
            line_with_newline = format!("{}\n", line);
            &line_with_newline
        };

        let ranges = highlighter
            .highlight_line(input, &self.syntax_set)
            .unwrap_or_default();

        ranges
            .into_iter()
            .map(|(style, text)| {
                HighlightedSpan::new(HighlightStyle::from_syntect(&style), text.to_string())
            })
            .collect()
    }

    /// Detect the syntax name (as a file extension) from a file path.
    ///
    /// Returns `"txt"` if the extension is unknown or absent.
    pub fn detect_syntax(&self, file_path: &str) -> String {
        let path = std::path::Path::new(file_path);
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("txt");

        // Verify that syntect knows this extension; fall back to "txt" if not.
        if self.syntax_set.find_syntax_by_extension(ext).is_some() {
            ext.to_string()
        } else {
            "txt".to_string()
        }
    }

    /// Returns a reference to the underlying `SyntaxSet`.
    pub fn syntax_set(&self) -> &SyntaxSet {
        &self.syntax_set
    }

    /// Returns a reference to the underlying `ThemeSet`.
    pub fn theme_set(&self) -> &ThemeSet {
        &self.theme_set
    }
}

impl Default for SyntaxHighlighter {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience function: split text into lines (preserving line endings) and
/// highlight each one.
pub fn highlight_text(
    highlighter: &SyntaxHighlighter,
    text: &str,
    syntax_name: &str,
) -> Vec<Vec<HighlightedSpan>> {
    LinesWithEndings::from(text)
        .map(|line| highlighter.highlight_line(line, syntax_name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_does_not_panic() {
        let _hl = SyntaxHighlighter::new();
    }

    #[test]
    fn highlight_rust_line() {
        let hl = SyntaxHighlighter::new();
        let spans = hl.highlight_line("fn main() {}", "rs");
        assert!(!spans.is_empty());
        // The concatenation of all span texts should match the input (+ newline).
        let combined: String = spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(combined, "fn main() {}\n");
    }

    #[test]
    fn highlight_python_line() {
        let hl = SyntaxHighlighter::new();
        let spans = hl.highlight_line("def hello():", "py");
        assert!(!spans.is_empty());
        let combined: String = spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(combined, "def hello():\n");
    }

    #[test]
    fn highlight_unknown_syntax_returns_single_span() {
        let hl = SyntaxHighlighter::new();
        let spans = hl.highlight_line("random stuff", "nonexistent_syntax_xyz");
        assert!(!spans.is_empty());
        let combined: String = spans.iter().map(|s| s.text.as_str()).collect();
        assert!(combined.contains("random stuff"));
    }

    #[test]
    fn highlight_empty_line() {
        let hl = SyntaxHighlighter::new();
        let spans = hl.highlight_line("", "rs");
        // Even an empty line gets at least the newline appended
        let combined: String = spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(combined, "\n");
    }

    #[test]
    fn highlight_line_with_existing_newline() {
        let hl = SyntaxHighlighter::new();
        let spans = hl.highlight_line("let x = 1;\n", "rs");
        let combined: String = spans.iter().map(|s| s.text.as_str()).collect();
        assert_eq!(combined, "let x = 1;\n");
    }

    #[test]
    fn highlight_style_from_syntect() {
        let style = Style {
            foreground: syntect::highlighting::Color {
                r: 100,
                g: 200,
                b: 50,
                a: 255,
            },
            ..Default::default()
        };
        let hs = HighlightStyle::from_syntect(&style);
        assert_eq!(hs.fg, (100, 200, 50));
    }

    #[test]
    fn highlight_style_new() {
        let hs = HighlightStyle::new(10, 20, 30);
        assert_eq!(hs.fg, (10, 20, 30));
    }

    #[test]
    fn highlighted_span_new() {
        let span = HighlightedSpan::new(HighlightStyle::new(0, 0, 0), "hello".to_string());
        assert_eq!(span.text, "hello");
        assert_eq!(span.style.fg, (0, 0, 0));
    }

    // ── detect_syntax ───────────────────────────────────────────────

    #[test]
    fn detect_syntax_rust() {
        let hl = SyntaxHighlighter::new();
        assert_eq!(hl.detect_syntax("src/main.rs"), "rs");
    }

    #[test]
    fn detect_syntax_python() {
        let hl = SyntaxHighlighter::new();
        assert_eq!(hl.detect_syntax("script.py"), "py");
    }

    #[test]
    fn detect_syntax_javascript() {
        let hl = SyntaxHighlighter::new();
        assert_eq!(hl.detect_syntax("app.js"), "js");
    }

    #[test]
    fn detect_syntax_no_extension() {
        let hl = SyntaxHighlighter::new();
        assert_eq!(hl.detect_syntax("Makefile"), "txt");
    }

    #[test]
    fn detect_syntax_unknown_extension() {
        let hl = SyntaxHighlighter::new();
        assert_eq!(hl.detect_syntax("data.xyzzy"), "txt");
    }

    #[test]
    fn detect_syntax_path_with_dirs() {
        let hl = SyntaxHighlighter::new();
        assert_eq!(hl.detect_syntax("/home/user/project/lib.rs"), "rs");
    }

    // ── highlight_text ──────────────────────────────────────────────

    #[test]
    fn highlight_text_multiline() {
        let hl = SyntaxHighlighter::new();
        let lines = highlight_text(&hl, "fn a() {}\nfn b() {}\n", "rs");
        assert_eq!(lines.len(), 2);
        for line_spans in &lines {
            assert!(!line_spans.is_empty());
        }
    }

    #[test]
    fn highlight_text_single_line_no_trailing_newline() {
        let hl = SyntaxHighlighter::new();
        let lines = highlight_text(&hl, "let x = 1;", "rs");
        assert_eq!(lines.len(), 1);
    }

    // ── Default trait ───────────────────────────────────────────────

    #[test]
    fn default_trait() {
        let _hl: SyntaxHighlighter = Default::default();
    }

    // ── Spans have correct fg colors (not all zeros) ────────────────

    #[test]
    fn rust_keyword_has_nonzero_color() {
        let hl = SyntaxHighlighter::new();
        let spans = hl.highlight_line("fn main() {}", "rs");
        // "fn" is a Rust keyword — it should get a non-default color
        let fn_span = spans.iter().find(|s| s.text.trim() == "fn");
        if let Some(span) = fn_span {
            let (r, g, b) = span.style.fg;
            // At least one channel should be non-zero
            assert!(r > 0 || g > 0 || b > 0);
        }
    }
}
