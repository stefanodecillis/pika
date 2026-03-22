use std::path::{Path, PathBuf};

use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::config::Theme;
use crate::input::Action;
use crate::ui::AppCommand;

/// An entry in the command palette results.
#[derive(Debug, Clone)]
pub struct PaletteEntry {
    pub display: String,
    pub path: PathBuf,
    pub score: i64,
}

/// The command palette / file finder overlay.
pub struct CommandPalette {
    pub visible: bool,
    pub input: String,
    pub entries: Vec<PaletteEntry>,
    pub filtered: Vec<PaletteEntry>,
    pub selected: usize,
    matcher: SkimMatcherV2,
}

impl CommandPalette {
    pub fn new() -> Self {
        Self {
            visible: false,
            input: String::new(),
            entries: Vec::new(),
            filtered: Vec::new(),
            selected: 0,
            matcher: SkimMatcherV2::default(),
        }
    }

    pub fn show(&mut self, root: &Path) {
        self.visible = true;
        self.input.clear();
        self.selected = 0;
        self.entries = Self::collect_files(root);
        self.filtered = self.entries.clone();
    }

    pub fn hide(&mut self) {
        self.visible = false;
        self.input.clear();
        self.filtered.clear();
        self.selected = 0;
    }

    pub fn insert_char(&mut self, ch: char) {
        self.input.push(ch);
        self.filter();
    }

    pub fn backspace(&mut self) {
        self.input.pop();
        self.filter();
    }

    pub fn select_next(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = (self.selected + 1) % self.filtered.len();
        }
    }

    pub fn select_previous(&mut self) {
        if !self.filtered.is_empty() {
            self.selected = if self.selected == 0 {
                self.filtered.len() - 1
            } else {
                self.selected - 1
            };
        }
    }

    pub fn accept(&self) -> Option<PathBuf> {
        self.filtered.get(self.selected).map(|e| e.path.clone())
    }

    fn filter(&mut self) {
        if self.input.is_empty() {
            self.filtered = self.entries.clone();
        } else {
            let mut scored: Vec<PaletteEntry> = self
                .entries
                .iter()
                .filter_map(|entry| {
                    self.matcher
                        .fuzzy_match(&entry.display, &self.input)
                        .map(|score| PaletteEntry {
                            display: entry.display.clone(),
                            path: entry.path.clone(),
                            score,
                        })
                })
                .collect();
            scored.sort_by(|a, b| b.score.cmp(&a.score));
            self.filtered = scored;
        }
        self.selected = 0;
    }

    fn collect_files(root: &Path) -> Vec<PaletteEntry> {
        let mut entries = Vec::new();
        Self::walk_dir(root, root, &mut entries, 0);
        entries.sort_by(|a, b| a.display.cmp(&b.display));
        entries
    }

    fn walk_dir(root: &Path, dir: &Path, entries: &mut Vec<PaletteEntry>, depth: usize) {
        if depth > 10 {
            return;
        }
        let Ok(read_dir) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in read_dir.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden files and common noise
            if name.starts_with('.') || name == "node_modules" || name == "target" {
                continue;
            }

            if path.is_dir() {
                Self::walk_dir(root, &path, entries, depth + 1);
            } else {
                let display = path
                    .strip_prefix(root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .to_string();
                entries.push(PaletteEntry {
                    display,
                    path,
                    score: 0,
                });
            }
        }
    }

    pub fn handle_action(&mut self, action: &Action) -> AppCommand {
        match action {
            Action::PaletteInput(ch) => {
                self.insert_char(*ch);
                AppCommand::Nothing
            }
            Action::PaletteBackspace => {
                self.backspace();
                AppCommand::Nothing
            }
            Action::PaletteUp => {
                self.select_previous();
                AppCommand::Nothing
            }
            Action::PaletteDown => {
                self.select_next();
                AppCommand::Nothing
            }
            Action::PaletteAccept => {
                if let Some(path) = self.accept() {
                    self.hide();
                    AppCommand::OpenFile(path)
                } else {
                    AppCommand::Nothing
                }
            }
            Action::PaletteDismiss => {
                self.hide();
                AppCommand::Nothing
            }
            Action::InsertChar(ch) => {
                self.insert_char(*ch);
                AppCommand::Nothing
            }
            Action::DeleteBackward => {
                self.backspace();
                AppCommand::Nothing
            }
            _ => AppCommand::Nothing,
        }
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        if !self.visible {
            return;
        }

        // Center the palette
        let width = (area.width * 60 / 100).max(40).min(area.width - 4);
        let height = 15.min(area.height - 4);
        let x = (area.width - width) / 2 + area.x;
        let y = area.y + 2;
        let palette_area = Rect::new(x, y, width, height);

        // Clear the background
        frame.render_widget(Clear, palette_area);

        let block = Block::default()
            .title(" Open File ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .style(Style::default().bg(Color::Rgb(30, 30, 30)));

        let inner = block.inner(palette_area);
        frame.render_widget(block, palette_area);

        if inner.height < 2 {
            return;
        }

        // Input line
        let input_area = Rect::new(inner.x, inner.y, inner.width, 1);
        let input_text = format!("  {} ", self.input);
        let input = Paragraph::new(input_text)
            .style(Style::default().fg(Color::White).bg(Color::Rgb(40, 40, 40)));
        frame.render_widget(input, input_area);

        // Results
        let results_area = Rect::new(inner.x, inner.y + 1, inner.width, inner.height - 1);
        let max_items = results_area.height as usize;

        let items: Vec<ListItem> = self
            .filtered
            .iter()
            .take(max_items)
            .enumerate()
            .map(|(i, entry)| {
                let style = if i == self.selected {
                    Style::default()
                        .fg(Color::Yellow)
                        .bg(Color::Rgb(50, 50, 50))
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(Color::White)
                };
                ListItem::new(Line::from(Span::styled(
                    format!("  {}", entry.display),
                    style,
                )))
            })
            .collect();

        let list = List::new(items);
        frame.render_widget(list, results_area);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs;

    fn setup_test_dir() -> TempDir {
        let tmp = TempDir::new().unwrap();
        fs::create_dir(tmp.path().join("src")).unwrap();
        fs::write(tmp.path().join("src/main.rs"), "").unwrap();
        fs::write(tmp.path().join("src/app.rs"), "").unwrap();
        fs::write(tmp.path().join("src/lib.rs"), "").unwrap();
        fs::write(tmp.path().join("Cargo.toml"), "").unwrap();
        fs::write(tmp.path().join("README.md"), "").unwrap();
        tmp
    }

    #[test]
    fn test_new_palette() {
        let palette = CommandPalette::new();
        assert!(!palette.visible);
        assert!(palette.input.is_empty());
    }

    #[test]
    fn test_show_collects_files() {
        let tmp = setup_test_dir();
        let mut palette = CommandPalette::new();
        palette.show(tmp.path());
        assert!(palette.visible);
        assert!(!palette.entries.is_empty());
        assert!(palette.entries.len() >= 4); // at least our test files
    }

    #[test]
    fn test_hide() {
        let tmp = setup_test_dir();
        let mut palette = CommandPalette::new();
        palette.show(tmp.path());
        palette.hide();
        assert!(!palette.visible);
        assert!(palette.input.is_empty());
    }

    #[test]
    fn test_fuzzy_filter() {
        let tmp = setup_test_dir();
        let mut palette = CommandPalette::new();
        palette.show(tmp.path());

        palette.insert_char('m');
        palette.insert_char('a');
        palette.insert_char('i');
        palette.insert_char('n');

        assert!(!palette.filtered.is_empty());
        // main.rs should be in results
        assert!(palette.filtered.iter().any(|e| e.display.contains("main")));
    }

    #[test]
    fn test_backspace() {
        let tmp = setup_test_dir();
        let mut palette = CommandPalette::new();
        palette.show(tmp.path());
        let initial_count = palette.filtered.len();

        palette.insert_char('x');
        palette.insert_char('x');
        palette.insert_char('x');
        let narrow_count = palette.filtered.len();

        palette.backspace();
        palette.backspace();
        palette.backspace();
        assert_eq!(palette.filtered.len(), initial_count);
    }

    #[test]
    fn test_navigation() {
        let tmp = setup_test_dir();
        let mut palette = CommandPalette::new();
        palette.show(tmp.path());
        assert_eq!(palette.selected, 0);

        palette.select_next();
        assert_eq!(palette.selected, 1);

        palette.select_previous();
        assert_eq!(palette.selected, 0);

        // Wrap around
        palette.select_previous();
        assert_eq!(palette.selected, palette.filtered.len() - 1);
    }

    #[test]
    fn test_accept() {
        let tmp = setup_test_dir();
        let mut palette = CommandPalette::new();
        palette.show(tmp.path());
        let result = palette.accept();
        assert!(result.is_some());
    }

    #[test]
    fn test_accept_empty() {
        let palette = CommandPalette::new();
        assert!(palette.accept().is_none());
    }

    #[test]
    fn test_handle_action_accept() {
        let tmp = setup_test_dir();
        let mut palette = CommandPalette::new();
        palette.show(tmp.path());

        let cmd = palette.handle_action(&Action::PaletteAccept);
        assert!(matches!(cmd, AppCommand::OpenFile(_)));
        assert!(!palette.visible);
    }

    #[test]
    fn test_handle_action_dismiss() {
        let mut palette = CommandPalette::new();
        palette.visible = true;
        palette.handle_action(&Action::PaletteDismiss);
        assert!(!palette.visible);
    }

    #[test]
    fn test_skips_hidden_files() {
        let tmp = setup_test_dir();
        fs::write(tmp.path().join(".hidden"), "").unwrap();
        fs::create_dir(tmp.path().join(".git")).unwrap();

        let mut palette = CommandPalette::new();
        palette.show(tmp.path());

        assert!(
            !palette.entries.iter().any(|e| e.display.starts_with('.')),
            "hidden files should be filtered"
        );
    }
}
