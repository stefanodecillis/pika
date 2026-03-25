use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem};
use ratatui::Frame;

use crate::config::Theme;
use crate::input::Action;
use crate::ui::AppCommand;

/// A single completion item.
#[derive(Debug, Clone)]
pub struct CompletionItem {
    pub label: String,
    pub detail: Option<String>,
    pub kind: CompletionKind,
    pub insert_text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionKind {
    Function,
    Variable,
    Keyword,
    Struct,
    Field,
    Module,
    Snippet,
    Other,
}

impl CompletionKind {
    pub fn icon(&self) -> &'static str {
        match self {
            CompletionKind::Function => "fn",
            CompletionKind::Variable => "var",
            CompletionKind::Keyword => "kw",
            CompletionKind::Struct => "st",
            CompletionKind::Field => "fd",
            CompletionKind::Module => "mod",
            CompletionKind::Snippet => "snp",
            CompletionKind::Other => "  ",
        }
    }

    pub fn from_lsp(kind: Option<lsp_types::CompletionItemKind>) -> Self {
        match kind {
            Some(lsp_types::CompletionItemKind::FUNCTION)
            | Some(lsp_types::CompletionItemKind::METHOD) => CompletionKind::Function,
            Some(lsp_types::CompletionItemKind::VARIABLE) => CompletionKind::Variable,
            Some(lsp_types::CompletionItemKind::KEYWORD) => CompletionKind::Keyword,
            Some(lsp_types::CompletionItemKind::STRUCT)
            | Some(lsp_types::CompletionItemKind::CLASS) => CompletionKind::Struct,
            Some(lsp_types::CompletionItemKind::FIELD)
            | Some(lsp_types::CompletionItemKind::PROPERTY) => CompletionKind::Field,
            Some(lsp_types::CompletionItemKind::MODULE) => CompletionKind::Module,
            Some(lsp_types::CompletionItemKind::SNIPPET) => CompletionKind::Snippet,
            _ => CompletionKind::Other,
        }
    }
}

/// The autocomplete popup overlay.
pub struct CompletionPopup {
    pub visible: bool,
    /// Filtered items shown in the popup (subset of full_items).
    pub items: Vec<CompletionItem>,
    /// Unfiltered items from the last LSP response.
    pub full_items: Vec<CompletionItem>,
    pub selected: usize,
    pub cursor_x: u16,
    pub cursor_y: u16,
    /// The word prefix that was already typed when the popup was shown.
    /// Accepted on completion acceptance so the caller can delete it before
    /// inserting the full completion text.
    pub trigger_prefix: String,
}

impl CompletionPopup {
    pub fn new() -> Self {
        Self {
            visible: false,
            items: Vec::new(),
            full_items: Vec::new(),
            selected: 0,
            cursor_x: 0,
            cursor_y: 0,
            trigger_prefix: String::new(),
        }
    }

    pub fn show(&mut self, items: Vec<CompletionItem>, cursor_x: u16, cursor_y: u16) {
        if items.is_empty() {
            self.hide();
            return;
        }
        self.full_items = items.clone();
        self.items = items;
        self.selected = 0;
        self.cursor_x = cursor_x;
        self.cursor_y = cursor_y;
        self.visible = true;
    }

    pub fn show_from_lsp(
        &mut self,
        items: Vec<lsp_types::CompletionItem>,
        cursor_x: u16,
        cursor_y: u16,
        trigger_prefix: String,
    ) {
        let completion_items: Vec<CompletionItem> = items
            .into_iter()
            .map(|item| CompletionItem {
                label: item.label.clone(),
                detail: item.detail.clone(),
                kind: CompletionKind::from_lsp(item.kind),
                insert_text: item.insert_text.unwrap_or(item.label),
            })
            .collect();

        if self.visible {
            // Popup already open: update items and re-filter preserving selection.
            let old_label = self.items.get(self.selected).map(|i| i.label.clone());
            self.full_items = completion_items;
            self.trigger_prefix = trigger_prefix;
            let prefix = self.trigger_prefix.clone();
            self.filter_by_prefix(&prefix);
            // Restore previous selection if the item is still present.
            if let Some(label) = old_label {
                if let Some(idx) = self.items.iter().position(|i| i.label == label) {
                    self.selected = idx;
                }
            }
        } else {
            self.trigger_prefix = trigger_prefix;
            self.show(completion_items, cursor_x, cursor_y);
        }
    }

    /// Re-filter `items` from `full_items` by `prefix` (case-insensitive prefix match).
    /// Hides the popup if no items match.
    pub fn filter_by_prefix(&mut self, prefix: &str) {
        if prefix.is_empty() {
            self.items = self.full_items.clone();
        } else {
            let lower = prefix.to_lowercase();
            self.items = self.full_items
                .iter()
                .filter(|item| item.label.to_lowercase().contains(&lower))
                .cloned()
                .collect();
        }
        if self.items.is_empty() {
            self.visible = false;
        } else {
            self.selected = self.selected.min(self.items.len() - 1);
            self.visible = true;
        }
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.items.clear();
        self.full_items.clear();
        self.selected = 0;
    }

    pub fn select_next(&mut self) {
        if !self.items.is_empty() {
            self.selected = (self.selected + 1) % self.items.len();
        }
    }

    pub fn select_previous(&mut self) {
        if !self.items.is_empty() {
            self.selected = if self.selected == 0 {
                self.items.len() - 1
            } else {
                self.selected - 1
            };
        }
    }

    pub fn accept(&self) -> Option<&CompletionItem> {
        self.items.get(self.selected)
    }

    pub fn handle_action(&mut self, action: &Action) -> AppCommand {
        match action {
            Action::CompletionUp => {
                self.select_previous();
                AppCommand::Nothing
            }
            Action::CompletionDown => {
                self.select_next();
                AppCommand::Nothing
            }
            Action::CompletionAccept => {
                if let Some(_item) = self.accept() {
                    // The app will handle inserting the text
                    AppCommand::Nothing
                } else {
                    AppCommand::Nothing
                }
            }
            Action::CompletionDismiss => {
                self.hide();
                AppCommand::Nothing
            }
            _ => AppCommand::Nothing,
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, _theme: &Theme) {
        if !self.visible || self.items.is_empty() {
            return;
        }

        let max_items = 10.min(self.items.len());
        let max_width = self
            .items
            .iter()
            .map(|i| i.label.len() + i.kind.icon().len() + 4)
            .max()
            .unwrap_or(20)
            .min(50) as u16;

        // Position popup below cursor
        let popup_x = self.cursor_x.min(area.width.saturating_sub(max_width));
        let popup_y = (self.cursor_y + 1).min(area.height.saturating_sub(max_items as u16 + 2));
        let popup_area = Rect::new(
            popup_x,
            popup_y,
            max_width + 2,
            max_items as u16 + 2,
        );

        frame.render_widget(Clear, popup_area);

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::DarkGray))
            .style(Style::default().bg(Color::Rgb(30, 30, 40)));

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        let items: Vec<ListItem> = self
            .items
            .iter()
            .take(max_items)
            .enumerate()
            .map(|(i, item)| {
                let style = if i == self.selected {
                    Style::default()
                        .fg(Color::White)
                        .bg(Color::Rgb(50, 50, 70))
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::Rgb(200, 200, 200))
                };

                let kind_style = Style::default().fg(Color::Rgb(120, 120, 160));

                ListItem::new(Line::from(vec![
                    Span::styled(format!(" {} ", item.kind.icon()), kind_style),
                    Span::styled(&item.label, style),
                ]))
            })
            .collect();

        let list = List::new(items);
        frame.render_widget(list, inner);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_items() -> Vec<CompletionItem> {
        vec![
            CompletionItem {
                label: "println!".to_string(),
                detail: Some("macro".to_string()),
                kind: CompletionKind::Function,
                insert_text: "println!".to_string(),
            },
            CompletionItem {
                label: "pub".to_string(),
                detail: None,
                kind: CompletionKind::Keyword,
                insert_text: "pub".to_string(),
            },
            CompletionItem {
                label: "String".to_string(),
                detail: Some("std::string".to_string()),
                kind: CompletionKind::Struct,
                insert_text: "String".to_string(),
            },
        ]
    }

    #[test]
    fn test_new_popup() {
        let popup = CompletionPopup::new();
        assert!(!popup.visible);
        assert!(popup.items.is_empty());
    }

    #[test]
    fn test_show_popup() {
        let mut popup = CompletionPopup::new();
        popup.show(test_items(), 10, 5);
        assert!(popup.visible);
        assert_eq!(popup.items.len(), 3);
        assert_eq!(popup.selected, 0);
    }

    #[test]
    fn test_show_empty_hides() {
        let mut popup = CompletionPopup::new();
        popup.show(vec![], 10, 5);
        assert!(!popup.visible);
    }

    #[test]
    fn test_hide() {
        let mut popup = CompletionPopup::new();
        popup.show(test_items(), 10, 5);
        popup.hide();
        assert!(!popup.visible);
        assert!(popup.items.is_empty());
    }

    #[test]
    fn test_navigation() {
        let mut popup = CompletionPopup::new();
        popup.show(test_items(), 10, 5);

        popup.select_next();
        assert_eq!(popup.selected, 1);

        popup.select_next();
        assert_eq!(popup.selected, 2);

        popup.select_next();
        assert_eq!(popup.selected, 0); // wraps

        popup.select_previous();
        assert_eq!(popup.selected, 2); // wraps back
    }

    #[test]
    fn test_accept() {
        let mut popup = CompletionPopup::new();
        popup.show(test_items(), 10, 5);
        let item = popup.accept().unwrap();
        assert_eq!(item.label, "println!");
    }

    #[test]
    fn test_accept_empty() {
        let popup = CompletionPopup::new();
        assert!(popup.accept().is_none());
    }

    #[test]
    fn test_completion_kind_icon() {
        assert_eq!(CompletionKind::Function.icon(), "fn");
        assert_eq!(CompletionKind::Variable.icon(), "var");
        assert_eq!(CompletionKind::Struct.icon(), "st");
    }

    #[test]
    fn test_from_lsp_kind() {
        let kind = CompletionKind::from_lsp(Some(lsp_types::CompletionItemKind::FUNCTION));
        assert_eq!(kind, CompletionKind::Function);

        let kind = CompletionKind::from_lsp(None);
        assert_eq!(kind, CompletionKind::Other);
    }

    #[test]
    fn test_handle_dismiss() {
        let mut popup = CompletionPopup::new();
        popup.show(test_items(), 10, 5);
        popup.handle_action(&Action::CompletionDismiss);
        assert!(!popup.visible);
    }

    #[test]
    fn test_show_from_lsp() {
        let mut popup = CompletionPopup::new();
        let lsp_items = vec![
            lsp_types::CompletionItem {
                label: "test_fn".to_string(),
                kind: Some(lsp_types::CompletionItemKind::FUNCTION),
                detail: Some("Test function".to_string()),
                insert_text: Some("test_fn()".to_string()),
                ..Default::default()
            },
        ];
        popup.show_from_lsp(lsp_items, 10, 5, String::new());
        assert!(popup.visible);
        assert_eq!(popup.items[0].label, "test_fn");
        assert_eq!(popup.items[0].insert_text, "test_fn()");
    }
}
