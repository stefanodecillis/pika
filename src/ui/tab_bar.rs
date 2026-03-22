use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Tabs;
use ratatui::Frame;

use crate::config::Theme;

/// Tab bar state — tracks which tabs are open and which is active.
pub struct TabBar {
    pub tabs: Vec<TabInfo>,
    pub active: usize,
}

#[derive(Debug, Clone)]
pub struct TabInfo {
    pub name: String,
    pub modified: bool,
}

impl TabBar {
    pub fn new() -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
        }
    }

    pub fn add_tab(&mut self, name: String, modified: bool) {
        self.tabs.push(TabInfo { name, modified });
        self.active = self.tabs.len() - 1;
    }

    pub fn close_tab(&mut self, index: usize) -> Option<TabInfo> {
        if index >= self.tabs.len() {
            return None;
        }
        let tab = self.tabs.remove(index);
        if self.active >= self.tabs.len() && !self.tabs.is_empty() {
            self.active = self.tabs.len() - 1;
        }
        Some(tab)
    }

    pub fn next_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active = (self.active + 1) % self.tabs.len();
        }
    }

    pub fn previous_tab(&mut self) {
        if !self.tabs.is_empty() {
            self.active = if self.active == 0 {
                self.tabs.len() - 1
            } else {
                self.active - 1
            };
        }
    }

    pub fn set_active(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active = index;
        }
    }

    pub fn update_tab(&mut self, index: usize, name: String, modified: bool) {
        if let Some(tab) = self.tabs.get_mut(index) {
            tab.name = name;
            tab.modified = modified;
        }
    }

    pub fn find_tab(&self, name: &str) -> Option<usize> {
        self.tabs.iter().position(|t| t.name == name)
    }

    pub fn is_empty(&self) -> bool {
        self.tabs.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tabs.len()
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if self.tabs.is_empty() {
            return;
        }

        let titles: Vec<Line> = self
            .tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let indicator = if tab.modified { " ●" } else { "" };
                let title = format!(" {}{} ", tab.name, indicator);
                if i == self.active {
                    Line::from(Span::styled(
                        title,
                        Style::default()
                            .fg(theme.tab_active_fg.to_ratatui_color())
                            .bg(theme.tab_active_bg.to_ratatui_color())
                            .add_modifier(Modifier::BOLD),
                    ))
                } else {
                    Line::from(Span::styled(
                        title,
                        Style::default()
                            .fg(theme.tab_inactive_fg.to_ratatui_color())
                            .bg(theme.tab_inactive_bg.to_ratatui_color()),
                    ))
                }
            })
            .collect();

        let tabs_widget = Tabs::new(titles)
            .style(
                Style::default()
                    .bg(theme.tab_inactive_bg.to_ratatui_color())
                    .fg(theme.tab_inactive_fg.to_ratatui_color()),
            )
            .select(self.active)
            .divider(Span::raw("│"));

        frame.render_widget(tabs_widget, area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_tab_bar() {
        let bar = TabBar::new();
        assert!(bar.is_empty());
        assert_eq!(bar.len(), 0);
    }

    #[test]
    fn test_add_tab() {
        let mut bar = TabBar::new();
        bar.add_tab("main.rs".to_string(), false);
        assert_eq!(bar.len(), 1);
        assert_eq!(bar.active, 0);
        bar.add_tab("app.rs".to_string(), false);
        assert_eq!(bar.len(), 2);
        assert_eq!(bar.active, 1); // new tab becomes active
    }

    #[test]
    fn test_close_tab() {
        let mut bar = TabBar::new();
        bar.add_tab("a.rs".to_string(), false);
        bar.add_tab("b.rs".to_string(), false);
        bar.add_tab("c.rs".to_string(), false);
        bar.set_active(2);

        let closed = bar.close_tab(2);
        assert!(closed.is_some());
        assert_eq!(closed.unwrap().name, "c.rs");
        assert_eq!(bar.active, 1); // clamps to last
    }

    #[test]
    fn test_close_middle_tab() {
        let mut bar = TabBar::new();
        bar.add_tab("a.rs".to_string(), false);
        bar.add_tab("b.rs".to_string(), false);
        bar.add_tab("c.rs".to_string(), false);
        bar.set_active(1);

        bar.close_tab(1);
        assert_eq!(bar.len(), 2);
        assert_eq!(bar.tabs[0].name, "a.rs");
        assert_eq!(bar.tabs[1].name, "c.rs");
    }

    #[test]
    fn test_next_previous_tab() {
        let mut bar = TabBar::new();
        bar.add_tab("a.rs".to_string(), false);
        bar.add_tab("b.rs".to_string(), false);
        bar.add_tab("c.rs".to_string(), false);
        bar.set_active(0);

        bar.next_tab();
        assert_eq!(bar.active, 1);
        bar.next_tab();
        assert_eq!(bar.active, 2);
        bar.next_tab();
        assert_eq!(bar.active, 0); // wraps

        bar.previous_tab();
        assert_eq!(bar.active, 2); // wraps back
    }

    #[test]
    fn test_find_tab() {
        let mut bar = TabBar::new();
        bar.add_tab("main.rs".to_string(), false);
        bar.add_tab("app.rs".to_string(), false);

        assert_eq!(bar.find_tab("app.rs"), Some(1));
        assert_eq!(bar.find_tab("nonexistent.rs"), None);
    }

    #[test]
    fn test_update_tab() {
        let mut bar = TabBar::new();
        bar.add_tab("main.rs".to_string(), false);
        bar.update_tab(0, "main.rs".to_string(), true);
        assert!(bar.tabs[0].modified);
    }

    #[test]
    fn test_close_nonexistent_tab() {
        let mut bar = TabBar::new();
        assert!(bar.close_tab(0).is_none());
    }
}
