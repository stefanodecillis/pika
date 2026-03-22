use std::path::{Path, PathBuf};

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};
use ratatui::Frame;

use crate::config::Theme;
use crate::editor::clipboard::Clipboard;
use crate::editor::cursor::{CursorState, Position, Selection};
use crate::editor::document::Document;
use crate::editor::history::{Edit, UndoHistory};
use crate::editor::syntax::SyntaxHighlighter;
use crate::input::Action;
use crate::ui::{AppCommand, Component};

/// A buffer represents a single open file with its editing state.
pub struct Buffer {
    pub document: Document,
    pub cursor: CursorState,
    pub history: UndoHistory,
    pub clipboard: Clipboard,
    pub scroll_offset: usize,
    pub horizontal_scroll: usize,
    viewport_height: usize,
    viewport_width: usize,
}

impl Buffer {
    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let document = Document::open(path)?;
        Ok(Self {
            document,
            cursor: CursorState::new(),
            history: UndoHistory::new(),
            clipboard: Clipboard::new(),
            scroll_offset: 0,
            horizontal_scroll: 0,
            viewport_height: 24,
            viewport_width: 80,
        })
    }

    pub fn from_text(text: &str) -> Self {
        Self {
            document: Document::from_text(text),
            cursor: CursorState::new(),
            history: UndoHistory::new(),
            clipboard: Clipboard::new(),
            scroll_offset: 0,
            horizontal_scroll: 0,
            viewport_height: 24,
            viewport_width: 80,
        }
    }

    pub fn file_path(&self) -> Option<&Path> {
        self.document.file_path.as_deref()
    }

    pub fn name(&self) -> String {
        self.document
            .file_path
            .as_ref()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "untitled".to_string())
    }

    pub fn is_modified(&self) -> bool {
        self.document.modified
    }

    pub fn language_id(&self) -> &str {
        &self.document.language_id
    }

    /// Returns the file extension for syntax highlighting (e.g. "rs", "py").
    fn file_extension(&self) -> String {
        self.document
            .file_path
            .as_ref()
            .and_then(|p| p.extension())
            .and_then(|e| e.to_str())
            .unwrap_or("txt")
            .to_string()
    }

    pub fn update_viewport(&mut self, height: usize, width: usize) {
        self.viewport_height = height;
        self.viewport_width = width;
    }

    pub fn ensure_cursor_visible(&mut self) {
        let line = self.cursor.position.line;
        if line < self.scroll_offset {
            self.scroll_offset = line;
        } else if line >= self.scroll_offset + self.viewport_height {
            self.scroll_offset = line - self.viewport_height + 1;
        }

        let col = self.cursor.position.col;
        let gutter_width = self.gutter_width();
        let visible_width = self.viewport_width.saturating_sub(gutter_width);
        if col < self.horizontal_scroll {
            self.horizontal_scroll = col;
        } else if col >= self.horizontal_scroll + visible_width {
            self.horizontal_scroll = col - visible_width + 1;
        }
    }

    fn gutter_width(&self) -> usize {
        let max_line = self.document.line_count();
        let digits = format!("{}", max_line).len();
        digits + 2 // padding
    }

    // -- Cursor movement --

    pub fn move_cursor_up(&mut self) {
        if self.cursor.position.line > 0 {
            self.cursor.position.line -= 1;
            self.clamp_cursor_col();
        }
        self.cursor.selection = None;
        self.ensure_cursor_visible();
    }

    pub fn move_cursor_down(&mut self) {
        if self.cursor.position.line + 1 < self.document.line_count() {
            self.cursor.position.line += 1;
            self.clamp_cursor_col();
        }
        self.cursor.selection = None;
        self.ensure_cursor_visible();
    }

    pub fn move_cursor_left(&mut self) {
        if self.cursor.position.col > 0 {
            self.cursor.position.col -= 1;
            self.cursor.desired_col = self.cursor.position.col;
        } else if self.cursor.position.line > 0 {
            self.cursor.position.line -= 1;
            self.cursor.position.col = self.document.line_len(self.cursor.position.line);
            self.cursor.desired_col = self.cursor.position.col;
        }
        self.cursor.selection = None;
        self.ensure_cursor_visible();
    }

    pub fn move_cursor_right(&mut self) {
        let line_len = self.document.line_len(self.cursor.position.line);
        if self.cursor.position.col < line_len {
            self.cursor.position.col += 1;
            self.cursor.desired_col = self.cursor.position.col;
        } else if self.cursor.position.line + 1 < self.document.line_count() {
            self.cursor.position.line += 1;
            self.cursor.position.col = 0;
            self.cursor.desired_col = 0;
        }
        self.cursor.selection = None;
        self.ensure_cursor_visible();
    }

    pub fn move_cursor_word_left(&mut self) {
        let line = self.document.line(self.cursor.position.line);
        let col = self.cursor.position.col;

        if col == 0 {
            if self.cursor.position.line > 0 {
                self.cursor.position.line -= 1;
                self.cursor.position.col = self.document.line_len(self.cursor.position.line);
            }
        } else {
            let chars: Vec<char> = line.chars().collect();
            let mut i = col.min(chars.len());
            // Skip whitespace backwards
            while i > 0 && chars[i - 1].is_whitespace() {
                i -= 1;
            }
            // Skip word chars backwards
            while i > 0 && !chars[i - 1].is_whitespace() {
                i -= 1;
            }
            self.cursor.position.col = i;
        }
        self.cursor.desired_col = self.cursor.position.col;
        self.cursor.selection = None;
        self.ensure_cursor_visible();
    }

    pub fn move_cursor_word_right(&mut self) {
        let line = self.document.line(self.cursor.position.line);
        let line_len = self.document.line_len(self.cursor.position.line);
        let col = self.cursor.position.col;

        if col >= line_len {
            if self.cursor.position.line + 1 < self.document.line_count() {
                self.cursor.position.line += 1;
                self.cursor.position.col = 0;
            }
        } else {
            let chars: Vec<char> = line.chars().collect();
            let mut i = col;
            // Skip word chars forward
            while i < chars.len() && !chars[i].is_whitespace() {
                i += 1;
            }
            // Skip whitespace forward
            while i < chars.len() && chars[i].is_whitespace() {
                i += 1;
            }
            self.cursor.position.col = i;
        }
        self.cursor.desired_col = self.cursor.position.col;
        self.cursor.selection = None;
        self.ensure_cursor_visible();
    }

    pub fn move_cursor_line_start(&mut self) {
        self.cursor.position.col = 0;
        self.cursor.desired_col = 0;
        self.cursor.selection = None;
        self.ensure_cursor_visible();
    }

    pub fn move_cursor_line_end(&mut self) {
        self.cursor.position.col = self.document.line_len(self.cursor.position.line);
        self.cursor.desired_col = self.cursor.position.col;
        self.cursor.selection = None;
        self.ensure_cursor_visible();
    }

    pub fn move_cursor_file_start(&mut self) {
        self.cursor.position = Position::new(0, 0);
        self.cursor.desired_col = 0;
        self.cursor.selection = None;
        self.ensure_cursor_visible();
    }

    pub fn move_cursor_file_end(&mut self) {
        let last_line = self.document.line_count().saturating_sub(1);
        self.cursor.position.line = last_line;
        self.cursor.position.col = self.document.line_len(last_line);
        self.cursor.desired_col = self.cursor.position.col;
        self.cursor.selection = None;
        self.ensure_cursor_visible();
    }

    pub fn page_up(&mut self) {
        let jump = self.viewport_height.saturating_sub(2);
        self.cursor.position.line = self.cursor.position.line.saturating_sub(jump);
        self.scroll_offset = self.scroll_offset.saturating_sub(jump);
        self.clamp_cursor_col();
        self.cursor.selection = None;
    }

    pub fn page_down(&mut self) {
        let jump = self.viewport_height.saturating_sub(2);
        let max_line = self.document.line_count().saturating_sub(1);
        self.cursor.position.line = (self.cursor.position.line + jump).min(max_line);
        self.scroll_offset = (self.scroll_offset + jump).min(max_line);
        self.clamp_cursor_col();
        self.cursor.selection = None;
    }

    // -- Selection extension --
    // These work like cursor movement but extend the selection instead of clearing it.

    fn begin_or_extend_selection(&mut self) {
        if self.cursor.selection.is_none() {
            self.cursor.selection = Some(Selection::new(
                self.cursor.position,
                self.cursor.position,
            ));
        }
    }

    fn update_selection_head(&mut self) {
        if let Some(ref mut sel) = self.cursor.selection {
            sel.head = self.cursor.position;
            // Collapse if anchor == head
            if sel.is_empty() {
                self.cursor.selection = None;
            }
        }
    }

    pub fn select_up(&mut self) {
        self.begin_or_extend_selection();
        if self.cursor.position.line > 0 {
            self.cursor.position.line -= 1;
            self.clamp_cursor_col();
        }
        self.update_selection_head();
        self.ensure_cursor_visible();
    }

    pub fn select_down(&mut self) {
        self.begin_or_extend_selection();
        if self.cursor.position.line + 1 < self.document.line_count() {
            self.cursor.position.line += 1;
            self.clamp_cursor_col();
        }
        self.update_selection_head();
        self.ensure_cursor_visible();
    }

    pub fn select_left(&mut self) {
        self.begin_or_extend_selection();
        if self.cursor.position.col > 0 {
            self.cursor.position.col -= 1;
            self.cursor.desired_col = self.cursor.position.col;
        } else if self.cursor.position.line > 0 {
            self.cursor.position.line -= 1;
            self.cursor.position.col = self.document.line_len(self.cursor.position.line);
            self.cursor.desired_col = self.cursor.position.col;
        }
        self.update_selection_head();
        self.ensure_cursor_visible();
    }

    pub fn select_right(&mut self) {
        self.begin_or_extend_selection();
        let line_len = self.document.line_len(self.cursor.position.line);
        if self.cursor.position.col < line_len {
            self.cursor.position.col += 1;
            self.cursor.desired_col = self.cursor.position.col;
        } else if self.cursor.position.line + 1 < self.document.line_count() {
            self.cursor.position.line += 1;
            self.cursor.position.col = 0;
            self.cursor.desired_col = 0;
        }
        self.update_selection_head();
        self.ensure_cursor_visible();
    }

    pub fn select_word_left(&mut self) {
        self.begin_or_extend_selection();
        // Reuse word-left logic without clearing selection
        let line = self.document.line(self.cursor.position.line);
        let col = self.cursor.position.col;
        if col == 0 {
            if self.cursor.position.line > 0 {
                self.cursor.position.line -= 1;
                self.cursor.position.col = self.document.line_len(self.cursor.position.line);
            }
        } else {
            let chars: Vec<char> = line.chars().collect();
            let mut i = col.min(chars.len());
            while i > 0 && chars[i - 1].is_whitespace() { i -= 1; }
            while i > 0 && !chars[i - 1].is_whitespace() { i -= 1; }
            self.cursor.position.col = i;
        }
        self.cursor.desired_col = self.cursor.position.col;
        self.update_selection_head();
        self.ensure_cursor_visible();
    }

    pub fn select_word_right(&mut self) {
        self.begin_or_extend_selection();
        let line = self.document.line(self.cursor.position.line);
        let line_len = self.document.line_len(self.cursor.position.line);
        let col = self.cursor.position.col;
        if col >= line_len {
            if self.cursor.position.line + 1 < self.document.line_count() {
                self.cursor.position.line += 1;
                self.cursor.position.col = 0;
            }
        } else {
            let chars: Vec<char> = line.chars().collect();
            let mut i = col;
            while i < chars.len() && !chars[i].is_whitespace() { i += 1; }
            while i < chars.len() && chars[i].is_whitespace() { i += 1; }
            self.cursor.position.col = i;
        }
        self.cursor.desired_col = self.cursor.position.col;
        self.update_selection_head();
        self.ensure_cursor_visible();
    }

    pub fn select_line_start(&mut self) {
        self.begin_or_extend_selection();
        self.cursor.position.col = 0;
        self.cursor.desired_col = 0;
        self.update_selection_head();
        self.ensure_cursor_visible();
    }

    pub fn select_line_end(&mut self) {
        self.begin_or_extend_selection();
        self.cursor.position.col = self.document.line_len(self.cursor.position.line);
        self.cursor.desired_col = self.cursor.position.col;
        self.update_selection_head();
        self.ensure_cursor_visible();
    }

    // -- Text editing --

    pub fn insert_char(&mut self, ch: char) {
        // If there's a selection, delete it first (replace selection)
        self.delete_selection();
        let pos = self.cursor.position;
        self.document.insert_char(pos, ch);
        self.history.record(Edit::Insert {
            pos: (pos.line, pos.col),
            text: ch.to_string(),
        });
        self.cursor.position.col += 1;
        self.cursor.desired_col = self.cursor.position.col;
        self.ensure_cursor_visible();
    }

    pub fn insert_newline(&mut self) {
        self.delete_selection();
        let pos = self.cursor.position;
        self.document.insert_char(pos, '\n');
        self.history.record(Edit::Insert {
            pos: (pos.line, pos.col),
            text: "\n".to_string(),
        });
        self.cursor.position.line += 1;
        self.cursor.position.col = 0;
        self.cursor.desired_col = 0;
        self.ensure_cursor_visible();
    }

    pub fn insert_tab(&mut self, tab_size: usize) {
        self.delete_selection();
        let spaces: String = " ".repeat(tab_size);
        let pos = self.cursor.position;
        self.document.insert_text(pos, &spaces);
        self.history.record(Edit::Insert {
            pos: (pos.line, pos.col),
            text: spaces,
        });
        self.cursor.position.col += tab_size;
        self.cursor.desired_col = self.cursor.position.col;
        self.ensure_cursor_visible();
    }

    pub fn delete_backward(&mut self) {
        // If there's a selection, just delete it
        if self.cursor.selection.is_some() {
            self.delete_selection();
            self.ensure_cursor_visible();
            return;
        }
        if self.cursor.position.col > 0 {
            let pos = self.cursor.position;
            let del_pos = Position::new(pos.line, pos.col - 1);
            if let Some(ch) = self.document.char_at(del_pos) {
                self.document.delete_range(del_pos, pos);
                self.history.record(Edit::Delete {
                    pos: (del_pos.line, del_pos.col),
                    text: ch.to_string(),
                });
                self.cursor.position.col -= 1;
                self.cursor.desired_col = self.cursor.position.col;
            }
        } else if self.cursor.position.line > 0 {
            let prev_line = self.cursor.position.line - 1;
            let prev_col = self.document.line_len(prev_line);
            let from = Position::new(prev_line, prev_col);
            let to = self.cursor.position;
            self.document.delete_range(from, to);
            self.history.record(Edit::Delete {
                pos: (from.line, from.col),
                text: "\n".to_string(),
            });
            self.cursor.position.line = prev_line;
            self.cursor.position.col = prev_col;
            self.cursor.desired_col = prev_col;
        }
        self.ensure_cursor_visible();
    }

    // -- Clipboard operations --

    pub fn copy_text(&mut self) {
        if let Some(text) = self.get_selected_text() {
            self.clipboard.set_text(&text);
        }
    }

    pub fn cut_text(&mut self) {
        if let Some(text) = self.get_selected_text() {
            self.clipboard.set_text(&text);
            self.delete_selection();
            self.ensure_cursor_visible();
        }
    }

    pub fn paste_text(&mut self) {
        let text = self.clipboard.get_text();
        if text.is_empty() {
            return;
        }
        // Delete selection first if any
        self.delete_selection();
        let pos = self.cursor.position;
        self.document.insert_text(pos, &text);
        self.history.record(Edit::Insert {
            pos: (pos.line, pos.col),
            text: text.clone(),
        });
        // Move cursor to end of pasted text
        let mut end = pos;
        for ch in text.chars() {
            if ch == '\n' {
                end.line += 1;
                end.col = 0;
            } else {
                end.col += 1;
            }
        }
        self.cursor.position = end;
        self.cursor.desired_col = end.col;
        self.cursor.selection = None;
        self.ensure_cursor_visible();
    }

    /// Paste text directly (from terminal paste event, not clipboard).
    pub fn paste_text_content(&mut self, text: &str) {
        if text.is_empty() {
            return;
        }
        self.delete_selection();
        let pos = self.cursor.position;
        self.document.insert_text(pos, text);
        self.history.record(Edit::Insert {
            pos: (pos.line, pos.col),
            text: text.to_string(),
        });
        let mut end = pos;
        for ch in text.chars() {
            if ch == '\n' {
                end.line += 1;
                end.col = 0;
            } else {
                end.col += 1;
            }
        }
        self.cursor.position = end;
        self.cursor.desired_col = end.col;
        self.cursor.selection = None;
        self.ensure_cursor_visible();
    }

    pub fn delete_forward(&mut self) {
        let pos = self.cursor.position;
        let line_len = self.document.line_len(pos.line);
        if pos.col < line_len {
            let end = Position::new(pos.line, pos.col + 1);
            if let Some(ch) = self.document.char_at(pos) {
                self.document.delete_range(pos, end);
                self.history.record(Edit::Delete {
                    pos: (pos.line, pos.col),
                    text: ch.to_string(),
                });
            }
        } else if pos.line + 1 < self.document.line_count() {
            let end = Position::new(pos.line + 1, 0);
            self.document.delete_range(pos, end);
            self.history.record(Edit::Delete {
                pos: (pos.line, pos.col),
                text: "\n".to_string(),
            });
        }
    }

    pub fn select_all(&mut self) {
        let last_line = self.document.line_count().saturating_sub(1);
        let last_col = self.document.line_len(last_line);
        self.cursor.selection = Some(Selection::new(
            Position::new(0, 0),
            Position::new(last_line, last_col),
        ));
        self.cursor.position = Position::new(last_line, last_col);
    }

    pub fn get_selected_text(&self) -> Option<String> {
        let sel = self.cursor.selection.as_ref()?;
        let (start, end) = sel.ordered();
        let text = self.document.text();
        let rope = self.document.rope();

        let start_idx = rope.line_to_char(start.line) + start.col;
        let end_idx = rope.line_to_char(end.line) + end.col;
        Some(rope.slice(start_idx..end_idx).to_string())
    }

    pub fn delete_selection(&mut self) -> Option<String> {
        let sel = self.cursor.selection.take()?;
        let (start, end) = sel.ordered();
        let text = {
            let rope = self.document.rope();
            let start_idx = rope.line_to_char(start.line) + start.col;
            let end_idx = rope.line_to_char(end.line) + end.col;
            rope.slice(start_idx..end_idx).to_string()
        };
        self.document.delete_range(start, end);
        self.history.record(Edit::Delete {
            pos: (start.line, start.col),
            text: text.clone(),
        });
        self.cursor.position = start;
        self.cursor.desired_col = start.col;
        self.ensure_cursor_visible();
        Some(text)
    }

    pub fn undo(&mut self) {
        if let Some(edits) = self.history.undo() {
            // undo() already returns inverted edits — apply them directly
            for edit in edits {
                self.apply_edit(&edit);
            }
            self.cursor.desired_col = self.cursor.position.col;
            self.ensure_cursor_visible();
        }
    }

    pub fn redo(&mut self) {
        if let Some(edits) = self.history.redo() {
            // redo() returns the original edits — apply them directly
            for edit in edits {
                self.apply_edit(&edit);
            }
            self.cursor.desired_col = self.cursor.position.col;
            self.ensure_cursor_visible();
        }
    }

    fn apply_edit(&mut self, edit: &Edit) {
        match edit {
            Edit::Insert { pos, text } => {
                let position = Position::new(pos.0, pos.1);
                self.document.insert_text(position, text);
                // Move cursor to end of inserted text
                let mut end = position;
                for ch in text.chars() {
                    if ch == '\n' {
                        end.line += 1;
                        end.col = 0;
                    } else {
                        end.col += 1;
                    }
                }
                self.cursor.position = end;
            }
            Edit::Delete { pos, text } => {
                let start = Position::new(pos.0, pos.1);
                let mut end = start;
                for ch in text.chars() {
                    if ch == '\n' {
                        end.line += 1;
                        end.col = 0;
                    } else {
                        end.col += 1;
                    }
                }
                self.document.delete_range(start, end);
                self.cursor.position = start;
            }
        }
    }

    fn clamp_cursor_col(&mut self) {
        let line_len = self.document.line_len(self.cursor.position.line);
        self.cursor.position.col = self.cursor.desired_col.min(line_len);
    }

    /// Build styled lines for rendering, with optional syntax highlighting.
    pub fn build_lines(
        &self,
        highlighter: Option<&SyntaxHighlighter>,
        theme: &Theme,
        area_width: usize,
    ) -> Vec<Line<'static>> {
        let gutter_w = self.gutter_width();
        let mut lines = Vec::new();
        let start = self.scroll_offset;
        let end = (start + self.viewport_height).min(self.document.line_count());

        let gutter_fg = theme.editor_line_number_fg.to_ratatui_color();
        let editor_fg = theme.editor_fg.to_ratatui_color();

        // Compute selection range (in document coordinates) if any
        let sel_range = self.cursor.selection.as_ref().map(|s| s.ordered());
        let sel_bg = Color::Rgb(60, 90, 150); // VS Code-like selection blue

        for i in start..end {
            let line_num = format!("{:>width$} ", i + 1, width = gutter_w - 1);
            let is_current = i == self.cursor.position.line;

            let gutter_style = if is_current {
                Style::default().fg(Color::White).add_modifier(Modifier::BOLD)
            } else {
                Style::default().fg(gutter_fg)
            };

            let mut spans = vec![Span::styled(line_num, gutter_style)];

            let raw_line = self.document.line(i);
            let display_line = raw_line.trim_end_matches('\n').trim_end_matches('\r');

            // Apply horizontal scroll
            let visible: String = display_line
                .chars()
                .skip(self.horizontal_scroll)
                .collect();

            // Build base spans (with or without syntax highlighting)
            let base_spans: Vec<(Style, String)> = if let Some(hl) = highlighter {
                // Use file extension for syntect lookup (e.g. "rs", "py", "js")
                let syntax_ext = self.file_extension();
                hl.highlight_line(&visible, &syntax_ext)
                    .into_iter()
                    .map(|s| {
                        let (r, g, b) = s.style.fg;
                        (Style::default().fg(Color::Rgb(r, g, b)), s.text)
                    })
                    .collect()
            } else {
                vec![(Style::default().fg(editor_fg), visible.clone())]
            };

            // Apply selection overlay if this line intersects the selection
            if let Some((sel_start, sel_end)) = &sel_range {
                let line_start_col = self.horizontal_scroll;
                // Selection columns on this line (in visible-text coordinates)
                let sel_col_start = if i == sel_start.line {
                    sel_start.col.saturating_sub(line_start_col)
                } else if i > sel_start.line && i <= sel_end.line {
                    0
                } else {
                    usize::MAX // no selection on this line
                };
                let sel_col_end = if i == sel_end.line {
                    sel_end.col.saturating_sub(line_start_col)
                } else if i >= sel_start.line && i < sel_end.line {
                    visible.len()
                } else {
                    0
                };

                if i >= sel_start.line && i <= sel_end.line && sel_col_start < sel_col_end {
                    // Split base spans at selection boundaries and apply bg
                    let mut col = 0usize;
                    for (style, text) in &base_spans {
                        let span_len = text.chars().count();
                        let span_end = col + span_len;

                        if span_end <= sel_col_start || col >= sel_col_end {
                            // Entirely outside selection
                            spans.push(Span::styled(text.clone(), *style));
                        } else if col >= sel_col_start && span_end <= sel_col_end {
                            // Entirely inside selection
                            spans.push(Span::styled(text.clone(), style.bg(sel_bg)));
                        } else {
                            // Partially selected — split the span
                            let chars: Vec<char> = text.chars().collect();
                            let rel_start = sel_col_start.saturating_sub(col);
                            let rel_end = sel_col_end.saturating_sub(col).min(span_len);

                            if rel_start > 0 {
                                let before: String = chars[..rel_start].iter().collect();
                                spans.push(Span::styled(before, *style));
                            }
                            let selected: String = chars[rel_start..rel_end].iter().collect();
                            spans.push(Span::styled(selected, style.bg(sel_bg)));
                            if rel_end < span_len {
                                let after: String = chars[rel_end..].iter().collect();
                                spans.push(Span::styled(after, *style));
                            }
                        }
                        col = span_end;
                    }
                } else {
                    // No selection on this line
                    for (style, text) in base_spans {
                        spans.push(Span::styled(text, style));
                    }
                }
            } else {
                // No selection at all
                for (style, text) in base_spans {
                    spans.push(Span::styled(text, style));
                }
            }

            lines.push(Line::from(spans));
        }

        lines
    }

    /// Get cursor position relative to the viewport (for setting terminal cursor).
    pub fn cursor_screen_position(&self) -> (u16, u16) {
        let gutter_w = self.gutter_width();
        let x = (self.cursor.position.col - self.horizontal_scroll + gutter_w) as u16;
        let y = (self.cursor.position.line - self.scroll_offset) as u16;
        (x, y)
    }
}

impl Component for Buffer {
    fn handle_action(&mut self, action: &Action) -> AppCommand {
        match action {
            Action::CursorUp => self.move_cursor_up(),
            Action::CursorDown => self.move_cursor_down(),
            Action::CursorLeft => self.move_cursor_left(),
            Action::CursorRight => self.move_cursor_right(),
            Action::CursorWordLeft => self.move_cursor_word_left(),
            Action::CursorWordRight => self.move_cursor_word_right(),
            Action::CursorLineStart => self.move_cursor_line_start(),
            Action::CursorLineEnd => self.move_cursor_line_end(),
            Action::CursorFileStart => self.move_cursor_file_start(),
            Action::CursorFileEnd => self.move_cursor_file_end(),
            Action::PageUp => self.page_up(),
            Action::PageDown => self.page_down(),
            Action::SelectUp => self.select_up(),
            Action::SelectDown => self.select_down(),
            Action::SelectLeft => self.select_left(),
            Action::SelectRight => self.select_right(),
            Action::SelectWordLeft => self.select_word_left(),
            Action::SelectWordRight => self.select_word_right(),
            Action::SelectLineStart => self.select_line_start(),
            Action::SelectLineEnd => self.select_line_end(),
            Action::InsertChar(ch) => self.insert_char(*ch),
            Action::InsertNewline => self.insert_newline(),
            Action::InsertTab => self.insert_tab(4),
            Action::DeleteBackward => self.delete_backward(),
            Action::DeleteForward => self.delete_forward(),
            Action::SelectAll => self.select_all(),
            Action::Undo => self.undo(),
            Action::Redo => self.redo(),
            Action::Copy => self.copy_text(),
            Action::Cut => self.cut_text(),
            Action::Paste => self.paste_text(),
            Action::PasteText(text) => self.paste_text_content(text),
            Action::SaveFile => return AppCommand::SaveCurrentFile,
            Action::TriggerCompletion => return AppCommand::RequestCompletion,
            Action::GotoDefinition => return AppCommand::RequestGotoDefinition,
            Action::FindReferences => return AppCommand::RequestFindReferences,
            Action::CodeAction => return AppCommand::RequestCodeAction,
            Action::HoverInfo => return AppCommand::RequestHover,
            Action::FormatDocument => return AppCommand::RequestFormat,
            Action::SignatureHelp => return AppCommand::RequestSignatureHelp,
            _ => {}
        }
        AppCommand::Nothing
    }

    fn render(&self, frame: &mut Frame, area: Rect, focused: bool) {
        // Buffer rendering is handled by EditorPane which manages viewport
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_buffer() -> Buffer {
        Buffer::from_text("hello world\nfoo bar\nbaz qux\n")
    }

    #[test]
    fn test_buffer_from_text() {
        let buf = test_buffer();
        assert_eq!(buf.cursor.position, Position::new(0, 0));
        assert_eq!(buf.document.line_count(), 4); // trailing newline creates empty last line
    }

    #[test]
    fn test_cursor_movement() {
        let mut buf = test_buffer();
        buf.move_cursor_right();
        assert_eq!(buf.cursor.position, Position::new(0, 1));
        buf.move_cursor_down();
        assert_eq!(buf.cursor.position, Position::new(1, 1));
        buf.move_cursor_left();
        assert_eq!(buf.cursor.position, Position::new(1, 0));
        buf.move_cursor_up();
        assert_eq!(buf.cursor.position, Position::new(0, 0));
    }

    #[test]
    fn test_cursor_line_boundaries() {
        let mut buf = test_buffer();
        buf.move_cursor_line_end();
        assert_eq!(buf.cursor.position.col, 11); // "hello world"
        buf.move_cursor_line_start();
        assert_eq!(buf.cursor.position.col, 0);
    }

    #[test]
    fn test_cursor_word_movement() {
        let mut buf = test_buffer();
        buf.move_cursor_word_right();
        assert_eq!(buf.cursor.position.col, 6); // after "hello "
        buf.move_cursor_word_left();
        assert_eq!(buf.cursor.position.col, 0);
    }

    #[test]
    fn test_insert_char() {
        let mut buf = test_buffer();
        buf.insert_char('X');
        assert_eq!(buf.cursor.position, Position::new(0, 1));
        assert!(buf.document.line(0).starts_with("Xhello"));
    }

    #[test]
    fn test_insert_newline() {
        let mut buf = test_buffer();
        buf.move_cursor_right();
        buf.move_cursor_right();
        buf.insert_newline();
        assert_eq!(buf.cursor.position, Position::new(1, 0));
        assert_eq!(buf.document.line(0).trim_end(), "he");
        assert!(buf.document.line(1).starts_with("llo"));
    }

    #[test]
    fn test_delete_backward() {
        let mut buf = test_buffer();
        buf.move_cursor_right();
        buf.delete_backward();
        assert_eq!(buf.cursor.position, Position::new(0, 0));
        assert!(buf.document.line(0).starts_with("ello"));
    }

    #[test]
    fn test_delete_backward_at_line_start() {
        let mut buf = test_buffer();
        buf.move_cursor_down();
        buf.delete_backward();
        assert_eq!(buf.cursor.position.line, 0);
        // Lines should be joined
    }

    #[test]
    fn test_undo_redo() {
        let mut buf = test_buffer();
        let original = buf.document.text();
        buf.insert_char('Z');
        assert!(buf.document.line(0).starts_with("Z"));
        buf.undo();
        assert_eq!(buf.document.text(), original);
        buf.redo();
        assert!(buf.document.line(0).starts_with("Z"));
    }

    #[test]
    fn test_select_all() {
        let mut buf = test_buffer();
        buf.select_all();
        let selected = buf.get_selected_text();
        assert!(selected.is_some());
        assert_eq!(selected.unwrap(), buf.document.text());
    }

    #[test]
    fn test_page_up_down() {
        let mut buf = Buffer::from_text(&"line\n".repeat(100));
        buf.update_viewport(20, 80);
        buf.page_down();
        assert!(buf.cursor.position.line > 0);
        let pos = buf.cursor.position.line;
        buf.page_up();
        assert!(buf.cursor.position.line < pos);
    }

    #[test]
    fn test_file_start_end() {
        let mut buf = test_buffer();
        buf.move_cursor_file_end();
        assert_eq!(buf.cursor.position.line, buf.document.line_count() - 1);
        buf.move_cursor_file_start();
        assert_eq!(buf.cursor.position, Position::new(0, 0));
    }

    #[test]
    fn test_name_untitled() {
        let buf = Buffer::from_text("hello");
        assert_eq!(buf.name(), "untitled");
    }

    #[test]
    fn test_gutter_width() {
        let buf = Buffer::from_text(&"line\n".repeat(99));
        assert!(buf.gutter_width() >= 4); // "99" + padding
    }

    #[test]
    fn test_cursor_screen_position() {
        let mut buf = test_buffer();
        buf.move_cursor_right();
        buf.move_cursor_right();
        let (x, y) = buf.cursor_screen_position();
        let gutter = buf.gutter_width() as u16;
        assert_eq!(x, gutter + 2);
        assert_eq!(y, 0);
    }

    #[test]
    fn test_scroll_on_cursor_movement() {
        let mut buf = Buffer::from_text(&"line\n".repeat(100));
        buf.update_viewport(10, 80);
        for _ in 0..20 {
            buf.move_cursor_down();
        }
        assert!(buf.scroll_offset > 0);
    }

    #[test]
    fn test_handle_action_returns_command() {
        let mut buf = test_buffer();
        let cmd = buf.handle_action(&Action::SaveFile);
        assert!(matches!(cmd, AppCommand::SaveCurrentFile));
    }

    // -- Selection tests --

    #[test]
    fn test_select_right() {
        let mut buf = test_buffer();
        buf.select_right();
        buf.select_right();
        buf.select_right();
        assert!(buf.cursor.selection.is_some());
        let sel = buf.cursor.selection.unwrap();
        assert_eq!(sel.anchor, Position::new(0, 0));
        assert_eq!(sel.head, Position::new(0, 3));
    }

    #[test]
    fn test_select_left() {
        let mut buf = test_buffer();
        buf.move_cursor_right();
        buf.move_cursor_right();
        buf.move_cursor_right();
        buf.select_left();
        buf.select_left();
        let sel = buf.cursor.selection.unwrap();
        assert_eq!(sel.anchor, Position::new(0, 3));
        assert_eq!(sel.head, Position::new(0, 1));
    }

    #[test]
    fn test_select_down() {
        let mut buf = test_buffer();
        buf.select_down();
        let sel = buf.cursor.selection.unwrap();
        assert_eq!(sel.anchor, Position::new(0, 0));
        assert_eq!(sel.head, Position::new(1, 0));
    }

    #[test]
    fn test_select_up() {
        let mut buf = test_buffer();
        buf.move_cursor_down();
        buf.move_cursor_down();
        buf.select_up();
        let sel = buf.cursor.selection.unwrap();
        assert_eq!(sel.anchor, Position::new(2, 0));
        assert_eq!(sel.head, Position::new(1, 0));
    }

    #[test]
    fn test_select_word_right() {
        let mut buf = test_buffer();
        buf.select_word_right();
        let sel = buf.cursor.selection.unwrap();
        assert_eq!(sel.anchor, Position::new(0, 0));
        assert!(sel.head.col > 0); // should be past "hello"
    }

    #[test]
    fn test_select_line_end() {
        let mut buf = test_buffer();
        buf.select_line_end();
        let sel = buf.cursor.selection.unwrap();
        assert_eq!(sel.anchor, Position::new(0, 0));
        assert_eq!(sel.head.col, 11); // "hello world"
    }

    #[test]
    fn test_select_then_move_clears() {
        let mut buf = test_buffer();
        buf.select_right();
        buf.select_right();
        assert!(buf.cursor.selection.is_some());
        buf.move_cursor_right(); // normal move clears selection
        assert!(buf.cursor.selection.is_none());
    }

    #[test]
    fn test_select_get_text() {
        let mut buf = test_buffer();
        buf.select_right();
        buf.select_right();
        buf.select_right();
        buf.select_right();
        buf.select_right();
        let text = buf.get_selected_text().unwrap();
        assert_eq!(text, "hello");
    }

    #[test]
    fn test_select_collapses_to_none() {
        let mut buf = test_buffer();
        buf.select_right();
        assert!(buf.cursor.selection.is_some());
        buf.select_left(); // back to anchor
        assert!(buf.cursor.selection.is_none()); // collapsed
    }

    #[test]
    fn test_select_multiline() {
        let mut buf = test_buffer();
        buf.select_down();
        buf.select_down();
        let text = buf.get_selected_text().unwrap();
        assert!(text.contains('\n'));
        assert!(text.contains("foo bar"));
    }

    // -- Clipboard tests --

    #[test]
    fn test_copy_paste() {
        let mut buf = test_buffer();
        // Select "hello"
        for _ in 0..5 {
            buf.select_right();
        }
        buf.copy_text();
        // Move to end of line
        buf.move_cursor_line_end();
        buf.paste_text();
        let line = buf.document.line(0);
        assert!(line.contains("hello worldhello"));
    }

    #[test]
    fn test_cut() {
        let mut buf = test_buffer();
        // Select "hello"
        for _ in 0..5 {
            buf.select_right();
        }
        buf.cut_text();
        let line = buf.document.line(0);
        assert!(line.starts_with(" world"));
        // Paste it back
        buf.move_cursor_line_end();
        buf.paste_text();
        let line = buf.document.line(0);
        assert!(line.trim_end() == " worldhello");
    }

    #[test]
    fn test_paste_replaces_selection() {
        let mut buf = test_buffer();
        // Copy "hello"
        for _ in 0..5 {
            buf.select_right();
        }
        buf.copy_text();
        // Clear selection and move to start of " world"
        buf.cursor.selection = None;
        // Now select " world" (6 chars from col 5)
        for _ in 0..6 {
            buf.select_right();
        }
        // Paste "hello" over " world"
        buf.paste_text();
        let line = buf.document.line(0);
        assert_eq!(line.trim_end(), "hellohello");
    }

    #[test]
    fn test_type_replaces_selection() {
        let mut buf = test_buffer();
        for _ in 0..5 {
            buf.select_right();
        }
        buf.insert_char('X');
        let line = buf.document.line(0);
        assert!(line.starts_with("X world"));
    }

    #[test]
    fn test_backspace_deletes_selection() {
        let mut buf = test_buffer();
        for _ in 0..5 {
            buf.select_right();
        }
        buf.delete_backward();
        let line = buf.document.line(0);
        assert!(line.starts_with(" world"));
        assert!(buf.cursor.selection.is_none());
    }

    #[test]
    fn test_paste_multiline() {
        let mut buf = Buffer::from_text("start\nend\n");
        buf.move_cursor_line_end();
        buf.clipboard.set_text("\ninserted line");
        buf.paste_text();
        assert_eq!(buf.cursor.position.line, 1);
        assert!(buf.document.line(1).contains("inserted line"));
    }
}
