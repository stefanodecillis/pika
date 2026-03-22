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
use crate::ui::status_bar::{StatusBar, StatusInfo};
use crate::ui::tab_bar::TabBar;
use crate::ui::{AppCommand, Component};

/// The main editor pane containing tabs and buffers.
pub struct EditorPane {
    pub buffers: Vec<Buffer>,
    pub tab_bar: TabBar,
    pub highlighter: SyntaxHighlighter,
    pub theme: Theme,
}

impl EditorPane {
    pub fn new(theme: Theme) -> Self {
        Self {
            buffers: Vec::new(),
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

        let buffer = Buffer::from_file(path)?;
        self.buffers.push(buffer);
        self.tab_bar.add_tab(name, false);
        Ok(())
    }

    pub fn active_buffer(&self) -> Option<&Buffer> {
        if self.buffers.is_empty() {
            return None;
        }
        self.buffers.get(self.tab_bar.active)
    }

    pub fn active_buffer_mut(&mut self) -> Option<&mut Buffer> {
        if self.buffers.is_empty() {
            return None;
        }
        self.buffers.get_mut(self.tab_bar.active)
    }

    pub fn close_active_tab(&mut self) -> Option<PathBuf> {
        if self.buffers.is_empty() {
            return None;
        }
        let idx = self.tab_bar.active;
        let path = self.buffers[idx].file_path().map(|p| p.to_path_buf());
        self.buffers.remove(idx);
        self.tab_bar.close_tab(idx);
        path
    }

    pub fn save_active_buffer(&mut self) -> anyhow::Result<()> {
        let idx = self.tab_bar.active;
        if let Some(buf) = self.buffers.get_mut(idx) {
            buf.document.save()?;
            let name = buf.name();
            self.tab_bar.update_tab(idx, name, false);
        }
        Ok(())
    }

    pub fn status_info(&self) -> StatusInfo {
        if let Some(buf) = self.active_buffer() {
            StatusInfo {
                file_name: buf.name(),
                language: buf.language_id().to_string(),
                encoding: "UTF-8".to_string(),
                line_ending: "LF".to_string(),
                cursor_line: buf.cursor.position.line,
                cursor_col: buf.cursor.position.col,
                total_lines: buf.document.line_count(),
                modified: buf.is_modified(),
                lsp_status: None, // Will be set by app
            }
        } else {
            StatusInfo::default()
        }
    }

    fn update_tab_modified(&mut self) {
        if let Some(buf) = self.buffers.get(self.tab_bar.active) {
            let modified = buf.is_modified();
            let name = buf.name();
            self.tab_bar.update_tab(self.tab_bar.active, name, modified);
        }
    }

    /// Update the active buffer's viewport to match the current terminal size.
    /// Must be called before render.
    pub fn sync_viewport(&mut self, area: Rect) {
        if self.buffers.is_empty() {
            return;
        }
        // Account for: outer border (2), tab bar (1), status bar (1)
        let editor_height = area.height.saturating_sub(4) as usize;
        let editor_width = area.width.saturating_sub(2) as usize;
        if let Some(buf) = self.buffers.get_mut(self.tab_bar.active) {
            buf.update_viewport(editor_height, editor_width);
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
                let cmd = if let Some(buf) = self.buffers.get_mut(self.tab_bar.active) {
                    buf.handle_action(action)
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

        if self.buffers.is_empty() {
            self.render_welcome(frame, inner);
            return;
        }

        // Layout: tab bar (1 line) + editor content + status bar (1 line)
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1), // tab bar
                Constraint::Min(1),    // editor content
                Constraint::Length(1), // status bar
            ])
            .split(inner);

        // Render tab bar
        self.tab_bar.render(frame, chunks[0], &self.theme);

        // Render active buffer
        if let Some(buf) = self.active_buffer() {
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

            // Set cursor position
            if focused {
                let (cx, cy) = buf.cursor_screen_position();
                let cursor_x = editor_area.x + cx;
                let cursor_y = editor_area.y + cy;
                if cursor_x < editor_area.x + editor_area.width
                    && cursor_y < editor_area.y + editor_area.height
                {
                    frame.set_cursor_position((cursor_x, cursor_y));
                }
            }
        }

        // Render status bar
        let info = self.status_info();
        StatusBar::render(&info, frame, chunks[2], &self.theme);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    fn setup_test_file() -> (TempDir, PathBuf) {
        let tmp = TempDir::new().unwrap();
        let file_path = tmp.path().join("test.rs");
        fs::write(&file_path, "fn main() {\n    println!(\"hello\");\n}\n").unwrap();
        (tmp, file_path)
    }

    #[test]
    fn test_editor_pane_new() {
        let pane = EditorPane::new(Theme::default());
        assert!(pane.buffers.is_empty());
        assert!(pane.tab_bar.is_empty());
    }

    #[test]
    fn test_open_file() {
        let (tmp, path) = setup_test_file();
        let mut pane = EditorPane::new(Theme::default());
        pane.open_file(&path).unwrap();
        assert_eq!(pane.buffers.len(), 1);
        assert_eq!(pane.tab_bar.len(), 1);
    }

    #[test]
    fn test_open_same_file_twice() {
        let (tmp, path) = setup_test_file();
        let mut pane = EditorPane::new(Theme::default());
        pane.open_file(&path).unwrap();
        pane.open_file(&path).unwrap();
        assert_eq!(pane.buffers.len(), 1); // should not duplicate
    }

    #[test]
    fn test_close_active_tab() {
        let (tmp, path) = setup_test_file();
        let mut pane = EditorPane::new(Theme::default());
        pane.open_file(&path).unwrap();
        let closed = pane.close_active_tab();
        assert!(closed.is_some());
        assert!(pane.buffers.is_empty());
    }

    #[test]
    fn test_active_buffer() {
        let (tmp, path) = setup_test_file();
        let mut pane = EditorPane::new(Theme::default());
        assert!(pane.active_buffer().is_none());
        pane.open_file(&path).unwrap();
        assert!(pane.active_buffer().is_some());
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
    fn test_status_info() {
        let (tmp, path) = setup_test_file();
        let mut pane = EditorPane::new(Theme::default());
        pane.open_file(&path).unwrap();
        let info = pane.status_info();
        assert_eq!(info.file_name, "test.rs");
        assert!(!info.modified);
    }

    #[test]
    fn test_save_active_buffer() {
        let (tmp, path) = setup_test_file();
        let mut pane = EditorPane::new(Theme::default());
        pane.open_file(&path).unwrap();

        // Modify the buffer
        pane.handle_action(&Action::InsertChar('X'));
        assert!(pane.active_buffer().unwrap().is_modified());

        // Save
        pane.save_active_buffer().unwrap();
        assert!(!pane.active_buffer().unwrap().is_modified());
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
        assert!(pane.active_buffer().unwrap().document.line(0).starts_with("Z"));
    }
}
