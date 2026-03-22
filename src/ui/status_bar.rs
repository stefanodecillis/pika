use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use ratatui::Frame;

use crate::config::Theme;

/// Information displayed in the status bar.
#[derive(Debug, Clone, Default)]
pub struct StatusInfo {
    pub file_name: String,
    pub language: String,
    pub encoding: String,
    pub line_ending: String,
    pub cursor_line: usize,
    pub cursor_col: usize,
    pub total_lines: usize,
    pub modified: bool,
    pub lsp_status: Option<String>,
}

/// The status bar at the bottom of the editor.
pub struct StatusBar;

impl StatusBar {
    pub fn render(info: &StatusInfo, frame: &mut Frame, area: Rect, theme: &Theme) {
        let bg = theme.status_bar_bg.to_ratatui_color();
        let fg = theme.status_bar_fg.to_ratatui_color();
        let style = Style::default().bg(bg).fg(fg);

        let modified_indicator = if info.modified { " [+]" } else { "" };

        let left = format!(
            " {}{}  {}",
            info.file_name, modified_indicator, info.language
        );

        let right = format!(
            "{}  {}  Ln {}, Col {}  {} ",
            info.lsp_status.as_deref().unwrap_or("No LSP"),
            info.encoding,
            info.cursor_line + 1,
            info.cursor_col + 1,
            info.line_ending,
        );

        let padding_len = (area.width as usize)
            .saturating_sub(left.len())
            .saturating_sub(right.len());
        let padding = " ".repeat(padding_len);

        let line = Line::from(vec![
            Span::styled(&left, style.add_modifier(Modifier::BOLD)),
            Span::styled(padding, style),
            Span::styled(&right, style),
        ]);

        let paragraph = Paragraph::new(line).style(style);
        frame.render_widget(paragraph, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_info_default() {
        let info = StatusInfo::default();
        assert_eq!(info.cursor_line, 0);
        assert_eq!(info.cursor_col, 0);
        assert!(!info.modified);
        assert!(info.lsp_status.is_none());
    }

    #[test]
    fn test_status_info_with_values() {
        let info = StatusInfo {
            file_name: "main.rs".to_string(),
            language: "Rust".to_string(),
            encoding: "UTF-8".to_string(),
            line_ending: "LF".to_string(),
            cursor_line: 10,
            cursor_col: 5,
            total_lines: 100,
            modified: true,
            lsp_status: Some("rust-analyzer".to_string()),
        };
        assert_eq!(info.file_name, "main.rs");
        assert!(info.modified);
        assert_eq!(info.lsp_status.as_deref(), Some("rust-analyzer"));
    }

    #[test]
    fn test_status_info_clone() {
        let info = StatusInfo {
            file_name: "test.rs".to_string(),
            language: "Rust".to_string(),
            encoding: "UTF-8".to_string(),
            line_ending: "LF".to_string(),
            cursor_line: 0,
            cursor_col: 0,
            total_lines: 1,
            modified: false,
            lsp_status: None,
        };
        let cloned = info.clone();
        assert_eq!(cloned.file_name, info.file_name);
    }
}
