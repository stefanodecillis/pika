use std::path::{Path, PathBuf};

use ratatui::layout::{Constraint, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Cell, Row, Table};
use ratatui::Frame;

use crate::config::Theme;
use crate::input::Action;
use crate::ui::AppCommand;

/// An interactive table viewer/editor for CSV files.
pub struct CsvView {
    pub path: PathBuf,
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub cursor_row: usize,
    pub cursor_col: usize,
    pub scroll_row: usize,
    pub scroll_col: usize,
    pub viewport_height: usize,
    pub viewport_width: usize,
    pub editing: bool,
    pub edit_buffer: String,
    /// Original cell value kept for Undo while editing.
    edit_original: String,
    pub modified: bool,
}

impl CsvView {
    pub fn from_file(path: &Path) -> anyhow::Result<Self> {
        let mut rdr = csv::Reader::from_path(path)?;

        let headers: Vec<String> = rdr
            .headers()?
            .iter()
            .map(|s| s.to_string())
            .collect();

        let mut rows: Vec<Vec<String>> = Vec::new();
        for result in rdr.records() {
            let record = result?;
            let row: Vec<String> = record.iter().map(|s| s.to_string()).collect();
            rows.push(row);
        }

        Ok(Self {
            path: path.to_path_buf(),
            headers,
            rows,
            cursor_row: 0,
            cursor_col: 0,
            scroll_row: 0,
            scroll_col: 0,
            viewport_height: 24,
            viewport_width: 80,
            editing: false,
            edit_buffer: String::new(),
            edit_original: String::new(),
            modified: false,
        })
    }

    pub fn name(&self) -> String {
        self.path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "untitled.csv".to_string())
    }

    pub fn file_path(&self) -> &Path {
        &self.path
    }

    pub fn is_modified(&self) -> bool {
        self.modified
    }

    pub fn save(&mut self) -> anyhow::Result<()> {
        let mut wtr = csv::Writer::from_path(&self.path)?;
        wtr.write_record(&self.headers)?;
        for row in &self.rows {
            wtr.write_record(row)?;
        }
        wtr.flush()?;
        self.modified = false;
        Ok(())
    }

    pub fn update_viewport(&mut self, height: usize, width: usize) {
        self.viewport_height = height;
        self.viewport_width = width;
        self.scroll_cursor_into_view();
    }

    /// Column display widths: min(max(header, data) + 2, 25), at least 4.
    pub fn col_widths(&self) -> Vec<u16> {
        self.headers
            .iter()
            .enumerate()
            .map(|(i, h)| {
                let max_data = self
                    .rows
                    .iter()
                    .filter_map(|row| row.get(i))
                    .map(|s| s.len())
                    .max()
                    .unwrap_or(0);
                let w = h.len().max(max_data) + 2;
                (w.min(25).max(4)) as u16
            })
            .collect()
    }

    fn scroll_cursor_into_view(&mut self) {
        // Vertical scroll
        if self.cursor_row < self.scroll_row {
            self.scroll_row = self.cursor_row;
        }
        // -2 for header row + borders
        let visible_rows = self.viewport_height.saturating_sub(2);
        if self.cursor_row >= self.scroll_row + visible_rows {
            self.scroll_row = self.cursor_row + 1 - visible_rows;
        }

        // Horizontal scroll (by column index)
        if self.cursor_col < self.scroll_col {
            self.scroll_col = self.cursor_col;
        }
        // Estimate visible columns from viewport_width
        let widths = self.col_widths();
        let mut used_width = 0usize;
        let mut visible_cols = 0usize;
        for w in widths.iter().skip(self.scroll_col) {
            used_width += *w as usize;
            if used_width > self.viewport_width {
                break;
            }
            visible_cols += 1;
        }
        if visible_cols == 0 {
            visible_cols = 1;
        }
        if self.cursor_col >= self.scroll_col + visible_cols {
            self.scroll_col = self.cursor_col + 1 - visible_cols;
        }
    }

    fn commit_edit(&mut self) {
        if self.editing {
            if let Some(row) = self.rows.get_mut(self.cursor_row) {
                if let Some(cell) = row.get_mut(self.cursor_col) {
                    let new_val = self.edit_buffer.clone();
                    if *cell != new_val {
                        *cell = new_val;
                        self.modified = true;
                    }
                }
            }
            self.editing = false;
            self.edit_buffer.clear();
            self.edit_original.clear();
        }
    }

    fn cancel_edit(&mut self) {
        if self.editing {
            self.editing = false;
            self.edit_buffer.clear();
            self.edit_original.clear();
        }
    }

    fn start_edit(&mut self, replace_with: Option<char>) {
        let current = self
            .rows
            .get(self.cursor_row)
            .and_then(|r| r.get(self.cursor_col))
            .cloned()
            .unwrap_or_default();
        self.edit_original = current.clone();
        self.editing = true;
        if let Some(ch) = replace_with {
            self.edit_buffer = ch.to_string();
        } else {
            self.edit_buffer = current;
        }
    }

    fn num_cols(&self) -> usize {
        self.headers.len()
    }

    fn num_rows(&self) -> usize {
        self.rows.len()
    }

    /// Truncate a string to fit in `width` chars, appending `…` if needed.
    fn truncate(s: &str, width: usize) -> String {
        if s.len() <= width {
            s.to_string()
        } else if width > 1 {
            format!("{}…", &s[..width - 1])
        } else {
            "…".to_string()
        }
    }

    pub fn handle_action(&mut self, action: &Action) -> AppCommand {
        if self.editing {
            match action {
                Action::InsertChar(ch) => {
                    self.edit_buffer.push(*ch);
                }
                Action::DeleteBackward => {
                    self.edit_buffer.pop();
                }
                Action::DeleteForward => {
                    // Remove first char (simple forward delete)
                    if !self.edit_buffer.is_empty() {
                        self.edit_buffer.remove(0);
                    }
                }
                Action::InsertNewline => {
                    self.commit_edit();
                    // TODO: Implement your preferred behavior here.
                    // Option A (spreadsheet-style): advance cursor to next row after confirming.
                    //   self.cursor_row = (self.cursor_row + 1).min(self.num_rows().saturating_sub(1));
                    // Option B (text-editor-style): stay on the same cell after confirming.
                    //   (do nothing — cursor stays)
                    // Current default: stay on cell (option B). Change to option A if preferred.
                }
                Action::InsertTab => {
                    self.commit_edit();
                    if self.num_cols() > 0 {
                        self.cursor_col = (self.cursor_col + 1) % self.num_cols();
                        if self.cursor_col == 0 && self.cursor_row + 1 < self.num_rows() {
                            self.cursor_row += 1;
                        }
                    }
                    self.scroll_cursor_into_view();
                }
                Action::Undo => {
                    self.cancel_edit();
                }
                _ => {}
            }
            return AppCommand::Nothing;
        }

        // Navigation mode
        match action {
            Action::CursorUp => {
                if self.cursor_row > 0 {
                    self.cursor_row -= 1;
                    self.scroll_cursor_into_view();
                }
            }
            Action::CursorDown => {
                if self.cursor_row + 1 < self.num_rows() {
                    self.cursor_row += 1;
                    self.scroll_cursor_into_view();
                }
            }
            Action::CursorLeft => {
                if self.cursor_col > 0 {
                    self.cursor_col -= 1;
                    self.scroll_cursor_into_view();
                }
            }
            Action::CursorRight => {
                if self.num_cols() > 0 && self.cursor_col + 1 < self.num_cols() {
                    self.cursor_col += 1;
                    self.scroll_cursor_into_view();
                }
            }
            Action::InsertTab => {
                if self.num_cols() > 0 {
                    self.cursor_col = (self.cursor_col + 1) % self.num_cols();
                    if self.cursor_col == 0 && self.cursor_row + 1 < self.num_rows() {
                        self.cursor_row += 1;
                    }
                    self.scroll_cursor_into_view();
                }
            }
            Action::PageUp => {
                let step = self.viewport_height.saturating_sub(2).max(1);
                self.cursor_row = self.cursor_row.saturating_sub(step);
                self.scroll_cursor_into_view();
            }
            Action::PageDown => {
                let step = self.viewport_height.saturating_sub(2).max(1);
                self.cursor_row = (self.cursor_row + step).min(self.num_rows().saturating_sub(1));
                self.scroll_cursor_into_view();
            }
            Action::CursorFileStart => {
                self.cursor_row = 0;
                self.cursor_col = 0;
                self.scroll_cursor_into_view();
            }
            Action::CursorFileEnd => {
                self.cursor_row = self.num_rows().saturating_sub(1);
                self.scroll_cursor_into_view();
            }
            Action::InsertNewline => {
                // Enter in navigation mode starts editing the current cell
                self.start_edit(None);
            }
            Action::InsertChar(ch) => {
                // Typing in navigation mode enters edit mode, preserving existing content
                self.start_edit(None);
                self.edit_buffer.push(*ch);
            }
            Action::DeleteBackward | Action::DeleteForward | Action::DeleteLine => {
                // Clear the cell content
                if let Some(row) = self.rows.get_mut(self.cursor_row) {
                    if let Some(cell) = row.get_mut(self.cursor_col) {
                        if !cell.is_empty() {
                            cell.clear();
                            self.modified = true;
                        }
                    }
                }
            }
            _ => {}
        }

        AppCommand::Nothing
    }

    pub fn render_table(&self, frame: &mut Frame, area: Rect, theme: &Theme, focused: bool) {
        let widths = self.col_widths();
        let visible_cols: Vec<usize> = {
            let mut cols = Vec::new();
            let mut used = 0usize;
            for i in self.scroll_col..self.headers.len() {
                used += widths.get(i).copied().unwrap_or(10) as usize;
                if used > area.width as usize + widths.get(i).copied().unwrap_or(10) as usize {
                    break;
                }
                cols.push(i);
            }
            cols
        };

        let selected_bg = Color::Rgb(60, 90, 150);
        let editing_bg = Color::Rgb(90, 130, 50);
        let header_fg = theme.border_focused_color.to_ratatui_color();
        let normal_fg = theme.editor_fg.to_ratatui_color();

        // Build header row
        let header_cells: Vec<Cell> = visible_cols
            .iter()
            .map(|&ci| {
                let w = widths.get(ci).copied().unwrap_or(10) as usize;
                let text = Self::truncate(&self.headers[ci], w.saturating_sub(1));
                Cell::from(Span::styled(
                    text,
                    Style::default()
                        .fg(header_fg)
                        .add_modifier(Modifier::BOLD),
                ))
            })
            .collect();
        let header_row = Row::new(header_cells).height(1);

        // Build data rows
        let visible_row_start = self.scroll_row;
        let visible_row_end = (self.scroll_row + self.viewport_height).min(self.rows.len());

        let data_rows: Vec<Row> = (visible_row_start..visible_row_end)
            .map(|ri| {
                let row_data = &self.rows[ri];
                let cells: Vec<Cell> = visible_cols
                    .iter()
                    .map(|&ci| {
                        let w = widths.get(ci).copied().unwrap_or(10) as usize;
                        let is_selected = ri == self.cursor_row && ci == self.cursor_col;

                        let content = if is_selected && self.editing {
                            // Show edit buffer with a trailing cursor marker
                            format!("{}|", self.edit_buffer)
                        } else {
                            let raw = row_data.get(ci).map(|s| s.as_str()).unwrap_or("");
                            let truncated = Self::truncate(raw, w.saturating_sub(1));
                            // Pad empty selected cells with spaces so the
                            // background highlight is visible.
                            if is_selected && truncated.is_empty() {
                                " ".repeat(w.saturating_sub(1).max(1))
                            } else {
                                truncated
                            }
                        };

                        let style = if is_selected && self.editing {
                            Style::default().bg(editing_bg).fg(Color::White)
                        } else if is_selected && focused {
                            Style::default().bg(selected_bg).fg(Color::White)
                        } else {
                            Style::default().fg(normal_fg)
                        };

                        Cell::from(Span::styled(content, style))
                    })
                    .collect();
                Row::new(cells).height(1)
            })
            .collect();

        let constraints: Vec<Constraint> = visible_cols
            .iter()
            .map(|&ci| Constraint::Length(widths.get(ci).copied().unwrap_or(10)))
            .collect();

        let border_color = if focused {
            theme.border_focused_color.to_ratatui_color()
        } else {
            theme.border_color.to_ratatui_color()
        };

        // Status line at bottom showing position info
        let status = if self.editing {
            format!(
                " EDIT  row {}/{}, col {}/{} ",
                self.cursor_row + 1,
                self.rows.len(),
                self.cursor_col + 1,
                self.headers.len()
            )
        } else {
            format!(
                " row {}/{}, col {}/{} ",
                self.cursor_row + 1,
                self.rows.len(),
                self.cursor_col + 1,
                self.headers.len()
            )
        };

        let table = Table::new(data_rows, constraints.clone())
            .header(header_row)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border_color))
                    .title(Line::from(vec![
                        Span::styled(
                            format!(" {} ", self.name()),
                            Style::default().fg(header_fg).add_modifier(Modifier::BOLD),
                        ),
                        Span::styled(
                            status,
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]))
                    .style(
                        Style::default()
                            .bg(theme.editor_bg.to_ratatui_color())
                            .fg(normal_fg),
                    ),
            );

        frame.render_widget(table, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_csv(content: &str) -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("test.csv");
        fs::write(&path, content).unwrap();
        (tmp, path)
    }

    #[test]
    fn test_parse_simple_csv() {
        let (_tmp, path) = make_csv("name,age\nAlice,30\nBob,25\n");
        let view = CsvView::from_file(&path).unwrap();
        assert_eq!(view.headers, vec!["name", "age"]);
        assert_eq!(view.rows.len(), 2);
        assert_eq!(view.rows[0], vec!["Alice", "30"]);
    }

    #[test]
    fn test_initial_cursor() {
        let (_tmp, path) = make_csv("a,b\n1,2\n");
        let view = CsvView::from_file(&path).unwrap();
        assert_eq!(view.cursor_row, 0);
        assert_eq!(view.cursor_col, 0);
        assert!(!view.editing);
        assert!(!view.modified);
    }

    #[test]
    fn test_navigate_down_up() {
        let (_tmp, path) = make_csv("a,b\n1,2\n3,4\n");
        let mut view = CsvView::from_file(&path).unwrap();
        view.handle_action(&Action::CursorDown);
        assert_eq!(view.cursor_row, 1);
        view.handle_action(&Action::CursorUp);
        assert_eq!(view.cursor_row, 0);
        // Can't go above 0
        view.handle_action(&Action::CursorUp);
        assert_eq!(view.cursor_row, 0);
    }

    #[test]
    fn test_navigate_right_left() {
        let (_tmp, path) = make_csv("a,b,c\n1,2,3\n");
        let mut view = CsvView::from_file(&path).unwrap();
        view.handle_action(&Action::CursorRight);
        assert_eq!(view.cursor_col, 1);
        view.handle_action(&Action::CursorLeft);
        assert_eq!(view.cursor_col, 0);
        // Can't go left of 0
        view.handle_action(&Action::CursorLeft);
        assert_eq!(view.cursor_col, 0);
    }

    #[test]
    fn test_enter_edit_mode() {
        let (_tmp, path) = make_csv("a,b\nfoo,bar\n");
        let mut view = CsvView::from_file(&path).unwrap();
        view.handle_action(&Action::InsertNewline);
        assert!(view.editing);
        assert_eq!(view.edit_buffer, "foo"); // existing content loaded
    }

    #[test]
    fn test_type_to_start_edit() {
        let (_tmp, path) = make_csv("a,b\nfoo,bar\n");
        let mut view = CsvView::from_file(&path).unwrap();
        view.handle_action(&Action::InsertChar('x'));
        assert!(view.editing);
        assert_eq!(view.edit_buffer, "foox"); // preserves existing content, appends char
    }

    #[test]
    fn test_edit_and_confirm() {
        let (_tmp, path) = make_csv("a,b\nfoo,bar\n");
        let mut view = CsvView::from_file(&path).unwrap();
        view.handle_action(&Action::InsertNewline); // enter edit
        view.handle_action(&Action::InsertChar('z'));
        view.handle_action(&Action::InsertNewline); // confirm
        assert!(!view.editing);
        assert_eq!(view.rows[0][0], "fooz");
        assert!(view.modified);
    }

    #[test]
    fn test_edit_cancel_with_undo() {
        let (_tmp, path) = make_csv("a,b\nfoo,bar\n");
        let mut view = CsvView::from_file(&path).unwrap();
        view.handle_action(&Action::InsertNewline);
        view.handle_action(&Action::InsertChar('x'));
        view.handle_action(&Action::Undo); // cancel
        assert!(!view.editing);
        assert_eq!(view.rows[0][0], "foo"); // unchanged
        assert!(!view.modified);
    }

    #[test]
    fn test_delete_clears_cell() {
        let (_tmp, path) = make_csv("a,b\nfoo,bar\n");
        let mut view = CsvView::from_file(&path).unwrap();
        view.handle_action(&Action::DeleteBackward);
        assert_eq!(view.rows[0][0], "");
        assert!(view.modified);
    }

    #[test]
    fn test_save_roundtrip() {
        let (_tmp, path) = make_csv("a,b\nfoo,bar\n");
        let mut view = CsvView::from_file(&path).unwrap();
        view.rows[0][0] = "baz".to_string();
        view.modified = true;
        view.save().unwrap();
        assert!(!view.modified);
        // Re-read and verify
        let view2 = CsvView::from_file(&path).unwrap();
        assert_eq!(view2.rows[0][0], "baz");
    }

    #[test]
    fn test_col_widths() {
        let (_tmp, path) = make_csv("name,age\nAlice,30\n");
        let view = CsvView::from_file(&path).unwrap();
        let widths = view.col_widths();
        // "name" = 4, "Alice" = 5 → max=5, +2=7
        assert_eq!(widths[0], 7);
        // "age" = 3, "30" = 2 → max=3, +2=5
        assert_eq!(widths[1], 5);
    }

    #[test]
    fn test_tab_moves_to_next_col() {
        let (_tmp, path) = make_csv("a,b,c\n1,2,3\n");
        let mut view = CsvView::from_file(&path).unwrap();
        view.handle_action(&Action::InsertTab);
        assert_eq!(view.cursor_col, 1);
        view.handle_action(&Action::InsertTab);
        assert_eq!(view.cursor_col, 2);
        // Wrap around to col 0, row 0 (only 1 row)
        view.handle_action(&Action::InsertTab);
        assert_eq!(view.cursor_col, 0);
    }

    #[test]
    fn test_page_up_down() {
        let content = "a\n".to_string()
            + &(0..20).map(|i| format!("{}\n", i)).collect::<String>();
        let (_tmp, path) = make_csv(&content);
        let mut view = CsvView::from_file(&path).unwrap();
        view.update_viewport(10, 40);
        view.handle_action(&Action::PageDown);
        assert!(view.cursor_row > 0);
        view.handle_action(&Action::PageUp);
        assert_eq!(view.cursor_row, 0);
    }
}
