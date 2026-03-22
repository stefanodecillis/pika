use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph};
use ratatui::Frame;

/// The action that triggered the confirm dialog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfirmAction {
    CloseTab(usize),
    Quit,
    DeleteFile(std::path::PathBuf),
}

/// A confirmation dialog overlay (e.g. "Save before closing?").
pub struct ConfirmDialog {
    pub visible: bool,
    pub action: Option<ConfirmAction>,
    pub file_name: String,
    pub selected: ConfirmChoice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmChoice {
    Save,
    DontSave,
    Cancel,
}

/// What the user decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmResult {
    Save,
    DontSave,
    Cancel,
    Pending,
}

impl ConfirmDialog {
    pub fn new() -> Self {
        Self {
            visible: false,
            action: None,
            file_name: String::new(),
            selected: ConfirmChoice::Save,
        }
    }

    pub fn show(&mut self, file_name: String, action: ConfirmAction) {
        self.visible = true;
        self.file_name = file_name;
        self.action = Some(action);
        self.selected = ConfirmChoice::Save;
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.action = None;
        self.file_name.clear();
    }

    pub fn select_next(&mut self) {
        self.selected = match self.selected {
            ConfirmChoice::Save => ConfirmChoice::DontSave,
            ConfirmChoice::DontSave => ConfirmChoice::Cancel,
            ConfirmChoice::Cancel => ConfirmChoice::Save,
        };
    }

    pub fn select_previous(&mut self) {
        self.selected = match self.selected {
            ConfirmChoice::Save => ConfirmChoice::Cancel,
            ConfirmChoice::DontSave => ConfirmChoice::Save,
            ConfirmChoice::Cancel => ConfirmChoice::DontSave,
        };
    }

    pub fn accept(&self) -> ConfirmResult {
        match self.selected {
            ConfirmChoice::Save => ConfirmResult::Save,
            ConfirmChoice::DontSave => ConfirmResult::DontSave,
            ConfirmChoice::Cancel => ConfirmResult::Cancel,
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect) {
        if !self.visible {
            return;
        }

        let width = 52u16.min(area.width - 4);
        let height = 7u16;
        let x = (area.width - width) / 2 + area.x;
        let y = (area.height - height) / 2 + area.y;
        let dialog_area = Rect::new(x, y, width, height);

        frame.render_widget(Clear, dialog_area);

        let block = Block::default()
            .title(" Save Changes? ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .style(Style::default().bg(Color::Rgb(30, 30, 35)));

        let inner = block.inner(dialog_area);
        frame.render_widget(block, dialog_area);

        let is_delete = matches!(self.action, Some(ConfirmAction::DeleteFile(_)));
        let msg = if is_delete {
            format!("Delete \"{}\"? (moves to trash)", self.file_name)
        } else {
            format!("\"{}\" has unsaved changes.", self.file_name)
        };

        let btn = |label: &str, choice: ConfirmChoice| -> Span<'static> {
            let is_selected = self.selected == choice;
            if is_selected {
                Span::styled(
                    format!(" [{}] ", label),
                    Style::default()
                        .fg(Color::Black)
                        .bg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled(
                    format!("  {}  ", label),
                    Style::default().fg(Color::Rgb(180, 180, 180)),
                )
            }
        };

        let buttons = if is_delete {
            Line::from(vec![
                Span::raw("  "),
                btn("Delete", ConfirmChoice::Save), // Save = confirm action
                Span::raw("  "),
                btn("Cancel", ConfirmChoice::Cancel),
            ])
        } else {
            Line::from(vec![
                Span::raw("  "),
                btn("Save", ConfirmChoice::Save),
                Span::raw("  "),
                btn("Don't Save", ConfirmChoice::DontSave),
                Span::raw("  "),
                btn("Cancel", ConfirmChoice::Cancel),
            ])
        };

        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                format!("  {}", msg),
                Style::default().fg(Color::White),
            )),
            Line::from(""),
            buttons,
        ];

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, inner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new() {
        let dialog = ConfirmDialog::new();
        assert!(!dialog.visible);
        assert_eq!(dialog.selected, ConfirmChoice::Save);
    }

    #[test]
    fn test_show_hide() {
        let mut dialog = ConfirmDialog::new();
        dialog.show("main.rs".to_string(), ConfirmAction::CloseTab(0));
        assert!(dialog.visible);
        assert_eq!(dialog.file_name, "main.rs");
        assert!(matches!(dialog.action, Some(ConfirmAction::CloseTab(0))));

        dialog.hide();
        assert!(!dialog.visible);
        assert!(dialog.action.is_none());
    }

    #[test]
    fn test_navigation() {
        let mut dialog = ConfirmDialog::new();
        dialog.show("test.rs".to_string(), ConfirmAction::Quit);

        assert_eq!(dialog.selected, ConfirmChoice::Save);
        dialog.select_next();
        assert_eq!(dialog.selected, ConfirmChoice::DontSave);
        dialog.select_next();
        assert_eq!(dialog.selected, ConfirmChoice::Cancel);
        dialog.select_next();
        assert_eq!(dialog.selected, ConfirmChoice::Save); // wraps

        dialog.select_previous();
        assert_eq!(dialog.selected, ConfirmChoice::Cancel); // wraps back
    }

    #[test]
    fn test_accept() {
        let mut dialog = ConfirmDialog::new();
        dialog.show("test.rs".to_string(), ConfirmAction::Quit);

        assert_eq!(dialog.accept(), ConfirmResult::Save);
        dialog.select_next();
        assert_eq!(dialog.accept(), ConfirmResult::DontSave);
        dialog.select_next();
        assert_eq!(dialog.accept(), ConfirmResult::Cancel);
    }

    #[test]
    fn test_show_defaults_to_save() {
        let mut dialog = ConfirmDialog::new();
        dialog.selected = ConfirmChoice::Cancel;
        dialog.show("test.rs".to_string(), ConfirmAction::CloseTab(0));
        assert_eq!(dialog.selected, ConfirmChoice::Save); // reset on show
    }
}
