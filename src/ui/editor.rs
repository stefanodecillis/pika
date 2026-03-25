use std::path::{Path, PathBuf};

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph};
use ratatui::Frame;

use crate::config::Theme;
use crate::editor::syntax::SyntaxHighlighter;
use crate::input::Action;
use crate::ui::buffer::Buffer;
use crate::ui::csv_view::CsvView;
use crate::ui::status_bar::{StatusBar, StatusInfo};
use crate::ui::tab_bar::TabBar;
use crate::ui::{AppCommand, Component};

/// Content of a single editor tab — either a plain text buffer or a CSV table view.
pub enum TabContent {
    Text(Buffer),
    Csv(CsvView),
}

impl TabContent {
    pub fn name(&self) -> String {
        match self {
            TabContent::Text(b) => b.name(),
            TabContent::Csv(c) => c.name(),
        }
    }

    pub fn is_modified(&self) -> bool {
        match self {
            TabContent::Text(b) => b.is_modified(),
            TabContent::Csv(c) => c.is_modified(),
        }
    }

    pub fn file_path(&self) -> Option<&Path> {
        match self {
            TabContent::Text(b) => b.file_path(),
            TabContent::Csv(c) => Some(c.file_path()),
        }
    }

    pub fn save(&mut self) -> anyhow::Result<()> {
        match self {
            TabContent::Text(b) => b.document.save(),
            TabContent::Csv(c) => c.save(),
        }
    }

    pub fn update_viewport(&mut self, height: usize, width: usize) {
        match self {
            TabContent::Text(b) => b.update_viewport(height, width),
            TabContent::Csv(c) => c.update_viewport(height, width),
        }
    }

    pub fn handle_action(&mut self, action: &Action) -> AppCommand {
        match self {
            TabContent::Text(b) => b.handle_action(action),
            TabContent::Csv(c) => c.handle_action(action),
        }
    }
}

/// The main editor pane containing tabs and buffers.
pub struct EditorPane {
    pub tabs: Vec<TabContent>,
    pub tab_bar: TabBar,
    pub highlighter: SyntaxHighlighter,
    pub theme: Theme,
}

impl EditorPane {
    pub fn new(theme: Theme) -> Self {
        Self {
            tabs: Vec::new(),
            tab_bar: TabBar::new(),
            highlighter: SyntaxHighlighter::new(),
            theme,
        }
    }

    pub fn open_file(&mut self, path: &Path) -> anyhow::Result<()> {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "untitled".to_string());

        // Check if already open
        if let Some(idx) = self.tab_bar.find_tab(&name) {
            self.tab_bar.set_active(idx);
            return Ok(());
        }

        let content = if path.extension().map(|e| e == "csv").unwrap_or(false) {
            TabContent::Csv(CsvView::from_file(path)?)
        } else {
            TabContent::Text(Buffer::from_file(path)?)
        };

        self.tabs.push(content);
        self.tab_bar.add_tab(name, false);
        Ok(())
    }

    /// Returns the active text buffer, if the active tab is a text file.
    pub fn active_buffer(&self) -> Option<&Buffer> {
        match self.tabs.get(self.tab_bar.active) {
            Some(TabContent::Text(b)) => Some(b),
            _ => None,
        }
    }

    /// Returns the active text buffer mutably, if the active tab is a text file.
    pub fn active_buffer_mut(&mut self) -> Option<&mut Buffer> {
        match self.tabs.get_mut(self.tab_bar.active) {
            Some(TabContent::Text(b)) => Some(b),
            _ => None,
        }
    }

    pub fn close_active_tab(&mut self) -> Option<PathBuf> {
        if self.tabs.is_empty() {
            return None;
        }
        let idx = self.tab_bar.active;
        let path = self.tabs[idx].file_path().map(|p| p.to_path_buf());
        self.tabs.remove(idx);
        self.tab_bar.close_tab(idx);
        path
    }

    pub fn save_active_tab(&mut self) -> anyhow::Result<()> {
        let idx = self.tab_bar.active;
        if let Some(tab) = self.tabs.get_mut(idx) {
            tab.save()?;
            let name = tab.name();
            self.tab_bar.update_tab(idx, name, false);
        }
        Ok(())
    }

    pub fn status_info(&self) -> StatusInfo {
        match self.tabs.get(self.tab_bar.active) {
            Some(TabContent::Text(buf)) => StatusInfo {
                file_name: buf.name(),
                language: buf.language_id().to_string(),
                encoding: "UTF-8".to_string(),
                line_ending: "LF".to_string(),
                cursor_line: buf.cursor.position.line,
                cursor_col: buf.cursor.position.col,
                total_lines: buf.document.line_count(),
                modified: buf.is_modified(),
                lsp_status: None,
            },
            Some(TabContent::Csv(csv)) => StatusInfo {
                file_name: csv.name(),
                language: "csv".to_string(),
                encoding: "UTF-8".to_string(),
                line_ending: "LF".to_string(),
                cursor_line: csv.cursor_row,
                cursor_col: csv.cursor_col,
                total_lines: csv.rows.len(),
                modified: csv.is_modified(),
                lsp_status: None,
            },
            None => StatusInfo::default(),
        }
    }

    fn update_tab_modified(&mut self) {
        if let Some(tab) = self.tabs.get(self.tab_bar.active) {
            let modified = tab.is_modified();
            let name = tab.name();
            self.tab_bar.update_tab(self.tab_bar.active, name, modified);
        }
    }

    /// Returns true if the active text buffer has its search bar open.
    pub fn search_active(&self) -> bool {
        self.active_buffer().map(|b| b.search.active).unwrap_or(false)
    }

    /// Returns true if the active text buffer has its replace bar open.
    pub fn replace_active(&self) -> bool {
        self.active_buffer().map(|b| b.search.active && b.search.replace_mode).unwrap_or(false)
    }

    /// Returns true if the active text buffer has its goto-line bar open.
    pub fn goto_line_active(&self) -> bool {
        self.active_buffer().map(|b| b.goto_line.active).unwrap_or(false)
    }

    /// Update the active tab's viewport to match the current terminal size.
    /// Must be called before render.
    pub fn sync_viewport(&mut self, area: Rect) {
        if self.tabs.is_empty() {
            return;
        }
        // Account for: outer border (2), tab bar (1), status bar (1) + optional overlay bars
        let extra_rows =
            self.search_active() as u16 + self.replace_active() as u16 + self.goto_line_active() as u16;
        let height = area.height.saturating_sub(4 + extra_rows) as usize;
        let width = area.width.saturating_sub(2) as usize;
        if let Some(tab) = self.tabs.get_mut(self.tab_bar.active) {
            tab.update_viewport(height, width);
        }
    }

    fn render_welcome(&self, frame: &mut Frame, area: Rect) {
        let welcome = vec![
            Line::from(""),
            Line::from(""),
            Line::from(Span::styled(
                "  ⚡ Pika",
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(ratatui::style::Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Terminal IDE",
                Style::default().fg(Color::Gray),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "  Ctrl+P  Open file finder",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  Ctrl+B  Toggle sidebar",
                Style::default().fg(Color::DarkGray),
            )),
            Line::from(Span::styled(
                "  Ctrl+Q  Quit",
                Style::default().fg(Color::DarkGray),
            )),
        ];

        let paragraph = Paragraph::new(welcome).style(
            Style::default()
                .bg(self.theme.editor_bg.to_ratatui_color())
                .fg(self.theme.editor_fg.to_ratatui_color()),
        );
        frame.render_widget(paragraph, area);
    }
}

impl Component for EditorPane {
    fn handle_action(&mut self, action: &Action) -> AppCommand {
        match action {
            Action::NextTab => {
                self.tab_bar.next_tab();
                AppCommand::Nothing
            }
            Action::PreviousTab => {
                self.tab_bar.previous_tab();
                AppCommand::Nothing
            }
            Action::CloseTab => AppCommand::CloseCurrentTab,
            _ => {
                let cmd = if let Some(tab) = self.tabs.get_mut(self.tab_bar.active) {
                    tab.handle_action(action)
                } else {
                    return AppCommand::Nothing;
                };
                self.update_tab_modified();
                cmd
            }
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, focused: bool) {
        let border_color = if focused {
            self.theme.border_focused_color.to_ratatui_color()
        } else {
            self.theme.border_color.to_ratatui_color()
        };

        if self.tabs.is_empty() {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border_color))
                .style(
                    Style::default()
                        .bg(self.theme.editor_bg.to_ratatui_color())
                        .fg(self.theme.editor_fg.to_ratatui_color()),
                );
            let inner = block.inner(area);
            frame.render_widget(block, area);
            self.render_welcome(frame, inner);
            return;
        }

        match self.tabs.get(self.tab_bar.active) {
            Some(TabContent::Csv(csv)) => {
                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border_color))
                    .style(
                        Style::default()
                            .bg(self.theme.editor_bg.to_ratatui_color())
                            .fg(self.theme.editor_fg.to_ratatui_color()),
                    );
                let inner = block.inner(area);
                frame.render_widget(block, area);

                let layout = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints([Constraint::Length(1), Constraint::Min(1)])
                    .split(inner);

                self.tab_bar.render(frame, layout[0], &self.theme);
                csv.render_table(frame, layout[1], &self.theme, focused);
            }
            Some(TabContent::Text(_)) | None => {
                let block = Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border_color))
                    .style(
                        Style::default()
                            .bg(self.theme.editor_bg.to_ratatui_color())
                            .fg(self.theme.editor_fg.to_ratatui_color()),
                    );

                let inner = block.inner(area);
                frame.render_widget(block, area);

                let search_active = self.search_active();
                let replace_active = self.replace_active();
                let goto_line_active = self.goto_line_active();
                let mut constraints = vec![
                    Constraint::Length(1), // tab bar
                    Constraint::Min(1),    // editor content
                ];
                if search_active {
                    constraints.push(Constraint::Length(1)); // search bar
                }
                if replace_active {
                    constraints.push(Constraint::Length(1)); // replace bar
                }
                if goto_line_active {
                    constraints.push(Constraint::Length(1)); // goto-line bar
                }
                constraints.push(Constraint::Length(1)); // status bar

                let chunks = Layout::default()
                    .direction(Direction::Vertical)
                    .constraints(constraints)
                    .split(inner);

                self.tab_bar.render(frame, chunks[0], &self.theme);

                // Compute bar chunk indices dynamically
                let search_bar_idx = 2;
                let replace_bar_idx = 2 + search_active as usize;
                let goto_bar_idx = 2 + search_active as usize + replace_active as usize;
                let status_idx = 2 + search_active as usize + replace_active as usize + goto_line_active as usize;

                if let Some(TabContent::Text(buf)) = self.tabs.get(self.tab_bar.active) {
                    let editor_area = chunks[1];
                    let lines = buf.build_lines(
                        Some(&self.highlighter),
                        &self.theme,
                        editor_area.width as usize,
                    );

                    let paragraph = Paragraph::new(lines).style(
                        Style::default()
                            .bg(self.theme.editor_bg.to_ratatui_color())
                            .fg(self.theme.editor_fg.to_ratatui_color()),
                    );
                    frame.render_widget(paragraph, editor_area);

                    if focused && !search_active && !goto_line_active {
                        let (cx, cy) = buf.cursor_screen_position();
                        let cursor_x = editor_area.x + cx;
                        let cursor_y = editor_area.y + cy;
                        if cursor_x < editor_area.x + editor_area.width
                            && cursor_y < editor_area.y + editor_area.height
                        {
                            frame.set_cursor_position((cursor_x, cursor_y));
                        }
                    }

                    if search_active {
                        let match_count = buf.search.matches.len();
                        let current = if match_count > 0 { buf.search.current + 1 } else { 0 };
                        let search_text = format!(
                            " Find: {}  [{}/{}]  ↑↓/Enter to navigate • Esc/Ctrl+F to close",
                            buf.search.query, current, match_count
                        );
                        let search_bar = Paragraph::new(search_text).style(
                            Style::default()
                                .fg(Color::White)
                                .bg(Color::Rgb(30, 30, 55)),
                        );
                        frame.render_widget(search_bar, chunks[search_bar_idx]);
                    }

                    if replace_active {
                        let cursor_indicator = if buf.search.replace_field_focused { "▶ " } else { "" };
                        let replace_text = format!(
                            " Replace: {}{cursor_indicator}{}  Tab·Enter·Ctrl+A",
                            if buf.search.replace_field_focused { "" } else { "▶ " },
                            buf.search.replace_query,
                        );
                        let replace_bar = Paragraph::new(replace_text).style(
                            Style::default()
                                .fg(Color::White)
                                .bg(Color::Rgb(55, 20, 55)),
                        );
                        frame.render_widget(replace_bar, chunks[replace_bar_idx]);
                    }

                    if goto_line_active {
                        let line_count = buf.document.line_count();
                        let goto_text = format!(
                            " Go to line: {}  [1–{}]  Enter to jump • Esc to cancel",
                            buf.goto_line.input, line_count
                        );
                        let goto_bar = Paragraph::new(goto_text).style(
                            Style::default()
                                .fg(Color::White)
                                .bg(Color::Rgb(20, 55, 20)),
                        );
                        frame.render_widget(goto_bar, chunks[goto_bar_idx]);
                    }
                }

                let info = self.status_info();
                StatusBar::render(&info, frame, chunks[status_idx], &self.theme);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_file() -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.rs");
        fs::write(&file_path, "fn main() {\n    println!(\"hello\");\n}\n").unwrap();
        (tmp, file_path)
    }

    fn setup_csv_file() -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("data.csv");
        fs::write(&file_path, "name,age\nAlice,30\nBob,25\n").unwrap();
        (tmp, file_path)
    }

    #[test]
    fn test_editor_pane_new() {
        let pane = EditorPane::new(Theme::default());
        assert!(pane.tabs.is_empty());
        assert!(pane.tab_bar.is_empty());
    }

    #[test]
    fn test_open_file() {
        let (tmp, path) = setup_test_file();
        let mut pane = EditorPane::new(Theme::default());
        pane.open_file(&path).unwrap();
        assert_eq!(pane.tabs.len(), 1);
        assert_eq!(pane.tab_bar.len(), 1);
    }

    #[test]
    fn test_open_csv_file() {
        let (_tmp, path) = setup_csv_file();
        let mut pane = EditorPane::new(Theme::default());
        pane.open_file(&path).unwrap();
        assert_eq!(pane.tabs.len(), 1);
        assert!(matches!(pane.tabs[0], TabContent::Csv(_)));
    }

    #[test]
    fn test_open_same_file_twice() {
        let (tmp, path) = setup_test_file();
        let mut pane = EditorPane::new(Theme::default());
        pane.open_file(&path).unwrap();
        pane.open_file(&path).unwrap();
        assert_eq!(pane.tabs.len(), 1); // should not duplicate
    }

    #[test]
    fn test_close_active_tab() {
        let (tmp, path) = setup_test_file();
        let mut pane = EditorPane::new(Theme::default());
        pane.open_file(&path).unwrap();
        let closed = pane.close_active_tab();
        assert!(closed.is_some());
        assert!(pane.tabs.is_empty());
    }

    #[test]
    fn test_active_buffer_text() {
        let (tmp, path) = setup_test_file();
        let mut pane = EditorPane::new(Theme::default());
        assert!(pane.active_buffer().is_none());
        pane.open_file(&path).unwrap();
        assert!(pane.active_buffer().is_some());
    }

    #[test]
    fn test_active_buffer_csv_returns_none() {
        let (_tmp, path) = setup_csv_file();
        let mut pane = EditorPane::new(Theme::default());
        pane.open_file(&path).unwrap();
        // CSV tab → active_buffer() returns None (it's not a text buffer)
        assert!(pane.active_buffer().is_none());
    }

    #[test]
    fn test_next_previous_tab() {
        let tmp = TempDir::new().unwrap();
        let p1 = tmp.path().join("a.rs");
        let p2 = tmp.path().join("b.rs");
        fs::write(&p1, "// a").unwrap();
        fs::write(&p2, "// b").unwrap();

        let mut pane = EditorPane::new(Theme::default());
        pane.open_file(&p1).unwrap();
        pane.open_file(&p2).unwrap();
        assert_eq!(pane.tab_bar.active, 1);

        pane.handle_action(&Action::PreviousTab);
        assert_eq!(pane.tab_bar.active, 0);

        pane.handle_action(&Action::NextTab);
        assert_eq!(pane.tab_bar.active, 1);
    }

    #[test]
    fn test_status_info_text() {
        let (tmp, path) = setup_test_file();
        let mut pane = EditorPane::new(Theme::default());
        pane.open_file(&path).unwrap();
        let info = pane.status_info();
        assert_eq!(info.file_name, "test.rs");
        assert!(!info.modified);
    }

    #[test]
    fn test_status_info_csv() {
        let (_tmp, path) = setup_csv_file();
        let mut pane = EditorPane::new(Theme::default());
        pane.open_file(&path).unwrap();
        let info = pane.status_info();
        assert_eq!(info.file_name, "data.csv");
        assert_eq!(info.language, "csv");
    }

    #[test]
    fn test_save_active_tab_text() {
        let (tmp, path) = setup_test_file();
        let mut pane = EditorPane::new(Theme::default());
        pane.open_file(&path).unwrap();

        pane.handle_action(&Action::InsertChar('X'));
        assert!(pane.tabs[0].is_modified());

        pane.save_active_tab().unwrap();
        assert!(!pane.tabs[0].is_modified());
    }

    #[test]
    fn test_status_info_empty() {
        let pane = EditorPane::new(Theme::default());
        let info = pane.status_info();
        assert_eq!(info.file_name, "");
    }

    #[test]
    fn test_handle_edit_action() {
        let (tmp, path) = setup_test_file();
        let mut pane = EditorPane::new(Theme::default());
        pane.open_file(&path).unwrap();

        let cmd = pane.handle_action(&Action::InsertChar('Z'));
        assert!(matches!(cmd, AppCommand::Nothing));
        if let TabContent::Text(buf) = &pane.tabs[0] {
            assert!(buf.document.line(0).starts_with("Z"));
        }
    }

    #[test]
    fn test_csv_navigation_via_pane() {
        let (_tmp, path) = setup_csv_file();
        let mut pane = EditorPane::new(Theme::default());
        pane.open_file(&path).unwrap();
        pane.handle_action(&Action::CursorDown);
        if let TabContent::Csv(csv) = &pane.tabs[0] {
            assert_eq!(csv.cursor_row, 1);
        }
    }
}
