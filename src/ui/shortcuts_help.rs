use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};
use ratatui::Frame;

use crate::input::Action;

/// A full-screen overlay showing all keyboard shortcuts.
pub struct ShortcutsHelp {
    pub visible: bool,
    pub scroll: usize,
}

impl ShortcutsHelp {
    pub fn new() -> Self {
        Self {
            visible: false,
            scroll: 0,
        }
    }

    pub fn toggle(&mut self) {
        self.visible = !self.visible;
        self.scroll = 0;
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.scroll = 0;
    }

    pub fn scroll_up(&mut self) {
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        self.scroll += 1;
    }

    pub fn handle_action(&mut self, action: &Action) -> bool {
        match action {
            Action::CursorUp | Action::TreeUp | Action::CompletionUp | Action::PaletteUp => {
                self.scroll_up();
                true
            }
            Action::CursorDown | Action::TreeDown | Action::CompletionDown | Action::PaletteDown => {
                self.scroll_down();
                true
            }
            Action::PageUp => {
                self.scroll = self.scroll.saturating_sub(10);
                true
            }
            Action::PageDown => {
                self.scroll += 10;
                true
            }
            Action::ShowShortcuts
            | Action::FocusNext
            | Action::CompletionDismiss
            | Action::PaletteDismiss => {
                self.hide();
                true
            }
            Action::Quit => {
                self.hide();
                false // let app handle quit
            }
            _ => true, // absorb other actions
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        let width = (area.width - 4).min(72);
        let height = area.height - 2;
        let x = (area.width - width) / 2 + area.x;
        let y = area.y + 1;
        let help_area = Rect::new(x, y, width, height);

        frame.render_widget(Clear, help_area);

        let block = Block::default()
            .title(" Keyboard Shortcuts (Ctrl+H to close) ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .style(Style::default().bg(Color::Rgb(25, 25, 30)));

        let inner = block.inner(help_area);
        frame.render_widget(block, help_area);

        let lines = Self::build_lines();

        // Apply scroll
        let visible_lines: Vec<Line> = lines
            .into_iter()
            .skip(self.scroll)
            .collect();

        let paragraph = Paragraph::new(visible_lines)
            .style(Style::default().fg(Color::Rgb(200, 200, 200)));
        frame.render_widget(paragraph, inner);
    }

    fn build_lines() -> Vec<Line<'static>> {
        let heading = Style::default()
            .fg(Color::Yellow)
            .add_modifier(Modifier::BOLD);
        let key_style = Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD);
        let desc_style = Style::default().fg(Color::Rgb(180, 180, 180));
        let dim = Style::default().fg(Color::Rgb(80, 80, 80));

        let mut lines = Vec::new();

        // Title
        lines.push(Line::from(Span::styled(
            "  ⚡ Pika Keyboard Shortcuts",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )));
        lines.push(Line::from(""));

        // Helper to add a shortcut line
        let shortcut = |key: &'static str, desc: &'static str| -> Line<'static> {
            Line::from(vec![
                Span::styled("    ", desc_style),
                Span::styled(format!("{:<24}", key), key_style),
                Span::styled(desc, desc_style),
            ])
        };

        let separator = || -> Line<'static> {
            Line::from(Span::styled(
                "  ─────────────────────────────────────────────────────",
                dim,
            ))
        };

        // Global
        lines.push(Line::from(Span::styled("  Global", heading)));
        lines.push(separator());
        lines.push(shortcut("Ctrl+B", "Toggle sidebar"));
        lines.push(shortcut("Ctrl+P", "Open file finder / command palette"));
        lines.push(shortcut("Ctrl+S", "Save current file"));
        lines.push(shortcut("Ctrl+W", "Close current tab"));
        lines.push(shortcut("Ctrl+Q", "Quit"));
        lines.push(shortcut("Ctrl+Tab", "Next tab"));
        lines.push(shortcut("Ctrl+H", "Show/hide this help"));
        lines.push(shortcut("Ctrl+Shift+F", "Project-wide search"));
        lines.push(shortcut("Esc", "Switch focus (sidebar ↔ editor)"));
        lines.push(Line::from(""));

        // Editor - Navigation
        lines.push(Line::from(Span::styled("  Editor — Navigation", heading)));
        lines.push(separator());
        lines.push(shortcut("↑ / ↓ / ← / →", "Move cursor"));
        lines.push(shortcut("Ctrl+← / Ctrl+→", "Move by word"));
        lines.push(shortcut("Home", "Go to line start"));
        lines.push(shortcut("End", "Go to line end"));
        lines.push(shortcut("Ctrl+Home", "Go to file start"));
        lines.push(shortcut("Ctrl+End", "Go to file end"));
        lines.push(shortcut("Page Up / Page Down", "Scroll by page"));
        lines.push(shortcut("Ctrl+G", "Go to line number"));
        lines.push(Line::from(""));

        // Editor - Editing
        lines.push(Line::from(Span::styled("  Editor — Editing", heading)));
        lines.push(separator());
        lines.push(shortcut("Ctrl+Z", "Undo"));
        lines.push(shortcut("Ctrl+Y", "Redo"));
        lines.push(shortcut("Ctrl+C", "Copy"));
        lines.push(shortcut("Ctrl+X", "Cut"));
        lines.push(shortcut("Ctrl+V", "Paste"));
        lines.push(shortcut("Ctrl+A", "Select all"));
        lines.push(shortcut("Ctrl+D", "Select next occurrence"));
        lines.push(shortcut("Tab", "Insert tab (spaces)"));
        lines.push(shortcut("Backspace", "Delete backward"));
        lines.push(shortcut("Delete", "Delete forward"));
        lines.push(Line::from(""));

        // Editor - Search
        lines.push(Line::from(Span::styled("  Editor — Search", heading)));
        lines.push(separator());
        lines.push(shortcut("Ctrl+F", "Find in file"));
        lines.push(shortcut("Ctrl+R", "Find and replace"));
        lines.push(Line::from(""));

        // Editor - LSP
        lines.push(Line::from(Span::styled("  Editor — LSP / IntelliSense", heading)));
        lines.push(separator());
        lines.push(shortcut("Ctrl+Space", "Trigger autocomplete"));
        lines.push(shortcut("F12", "Go to definition"));
        lines.push(shortcut("Shift+F12", "Find references"));
        lines.push(shortcut("F2", "Rename symbol"));
        lines.push(shortcut("Ctrl+.", "Code actions / quick fix"));
        lines.push(shortcut("Ctrl+K, Ctrl+I", "Hover info"));
        lines.push(shortcut("Ctrl+Shift+I", "Format document"));
        lines.push(Line::from(""));

        // Sidebar
        lines.push(Line::from(Span::styled("  Sidebar — File Tree", heading)));
        lines.push(separator());
        lines.push(shortcut("↑ / ↓", "Navigate files"));
        lines.push(shortcut("Enter", "Open file / toggle directory"));
        lines.push(shortcut("← / →", "Collapse / expand directory"));
        lines.push(shortcut("Ctrl+C", "Copy file"));
        lines.push(shortcut("Ctrl+X", "Cut file (move)"));
        lines.push(shortcut("Ctrl+V", "Paste file"));
        lines.push(shortcut("Delete", "Delete file (to trash)"));
        lines.push(shortcut("F2", "Rename file"));
        lines.push(shortcut("N", "New file"));
        lines.push(shortcut("Shift+N", "New directory"));
        lines.push(Line::from(""));

        // Drag & Drop
        lines.push(Line::from(Span::styled("  Drag & Drop", heading)));
        lines.push(separator());
        lines.push(Line::from(Span::styled(
            "    Drag files from Finder into the terminal to copy them",
            desc_style,
        )));
        lines.push(Line::from(Span::styled(
            "    into the currently selected directory in the sidebar.",
            desc_style,
        )));
        lines.push(Line::from(""));

        // Completion popup
        lines.push(Line::from(Span::styled("  Autocomplete Popup", heading)));
        lines.push(separator());
        lines.push(shortcut("↑ / ↓", "Navigate suggestions"));
        lines.push(shortcut("Enter / Tab", "Accept suggestion"));
        lines.push(shortcut("Esc", "Dismiss"));
        lines.push(Line::from(""));

        // Command palette
        lines.push(Line::from(Span::styled("  Command Palette", heading)));
        lines.push(separator());
        lines.push(shortcut("↑ / ↓", "Navigate results"));
        lines.push(shortcut("Enter", "Open selected file"));
        lines.push(shortcut("Esc", "Dismiss"));
        lines.push(Line::from(""));

        lines.push(Line::from(Span::styled(
            "  Scroll: ↑/↓ or Page Up/Page Down",
            dim,
        )));

        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let help = ShortcutsHelp::new();
        assert!(!help.visible);
        assert_eq!(help.scroll, 0);
    }

    #[test]
    fn test_toggle() {
        let mut help = ShortcutsHelp::new();
        help.toggle();
        assert!(help.visible);
        help.toggle();
        assert!(!help.visible);
    }

    #[test]
    fn test_scroll() {
        let mut help = ShortcutsHelp::new();
        help.visible = true;
        help.scroll_down();
        assert_eq!(help.scroll, 1);
        help.scroll_down();
        assert_eq!(help.scroll, 2);
        help.scroll_up();
        assert_eq!(help.scroll, 1);
        help.scroll_up();
        help.scroll_up(); // should not go below 0
        assert_eq!(help.scroll, 0);
    }

    #[test]
    fn test_handle_action_esc_hides() {
        let mut help = ShortcutsHelp::new();
        help.visible = true;
        let consumed = help.handle_action(&Action::FocusNext);
        assert!(consumed);
        assert!(!help.visible);
    }

    #[test]
    fn test_handle_action_scroll_down() {
        let mut help = ShortcutsHelp::new();
        help.visible = true;
        help.handle_action(&Action::CursorDown);
        assert_eq!(help.scroll, 1);
    }

    #[test]
    fn test_handle_action_page_down() {
        let mut help = ShortcutsHelp::new();
        help.visible = true;
        help.handle_action(&Action::PageDown);
        assert_eq!(help.scroll, 10);
    }

    #[test]
    fn test_handle_action_quit_doesnt_consume() {
        let mut help = ShortcutsHelp::new();
        help.visible = true;
        let consumed = help.handle_action(&Action::Quit);
        assert!(!consumed); // quit should not be consumed
        assert!(!help.visible);
    }

    #[test]
    fn test_build_lines_not_empty() {
        let lines = ShortcutsHelp::build_lines();
        assert!(lines.len() > 50); // should have many lines
    }

    #[test]
    fn test_hide_resets_scroll() {
        let mut help = ShortcutsHelp::new();
        help.visible = true;
        help.scroll = 15;
        help.hide();
        assert_eq!(help.scroll, 0);
        assert!(!help.visible);
    }
}
