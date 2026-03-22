use std::path::{Path, PathBuf};

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState};
use ratatui::Frame;

use crate::config::Theme;
use crate::files::tree::{FileTree, FlatEntry};
use crate::input::Action;
use crate::ui::{AppCommand, Component};

/// Clipboard state for file copy/cut operations.
#[derive(Debug, Clone)]
pub struct FileClipboard {
    pub path: PathBuf,
    pub is_cut: bool,
}

/// The sidebar component showing the file tree.
pub struct Sidebar {
    pub tree: FileTree,
    pub clipboard: Option<FileClipboard>,
    pub visible: bool,
    pub width: u16,
    pub rename_input: Option<String>,
    pub new_file_input: Option<(PathBuf, bool)>, // (parent_dir, is_directory)
    pub input_buffer: String,
}

impl Sidebar {
    pub fn new(root: &Path, width: u16) -> anyhow::Result<Self> {
        let tree = FileTree::from_path(root)?;
        Ok(Self {
            tree,
            clipboard: None,
            visible: true,
            width,
            rename_input: None,
            new_file_input: None,
            input_buffer: String::new(),
        })
    }

    pub fn toggle_visibility(&mut self) {
        self.visible = !self.visible;
    }

    pub fn selected_path(&self) -> Option<PathBuf> {
        self.tree.selected_entry().map(|e| e.path.clone())
    }

    pub fn selected_is_dir(&self) -> bool {
        self.tree.selected_entry().is_some_and(|e| e.is_dir)
    }

    pub fn selected_parent_dir(&self) -> Option<PathBuf> {
        self.tree.selected_entry().map(|e| {
            if e.is_dir {
                e.path.clone()
            } else {
                e.path.parent().unwrap_or(Path::new(".")).to_path_buf()
            }
        })
    }

    pub fn refresh(&mut self) {
        let _ = self.tree.refresh();
    }

    fn handle_input_char(&mut self, ch: char) {
        self.input_buffer.push(ch);
    }

    fn handle_input_backspace(&mut self) {
        self.input_buffer.pop();
    }

    fn cancel_input(&mut self) {
        self.rename_input = None;
        self.new_file_input = None;
        self.input_buffer.clear();
    }

    pub fn render_tree(&self, frame: &mut Frame, area: Rect, focused: bool, theme: &Theme) {
        let border_color = if focused {
            theme.border_focused_color.to_ratatui_color()
        } else {
            theme.border_color.to_ratatui_color()
        };

        let block = Block::default()
            .title(" Explorer ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(border_color))
            .style(
                Style::default()
                    .bg(theme.sidebar_bg.to_ratatui_color())
                    .fg(theme.sidebar_fg.to_ratatui_color()),
            );

        let inner = block.inner(area);
        frame.render_widget(block, area);

        let items: Vec<ListItem> = self
            .tree
            .flattened
            .iter()
            .enumerate()
            .map(|(i, entry)| {
                let indent = "  ".repeat(entry.depth);
                let icon = if entry.is_dir {
                    if entry.expanded { "▼ " } else { "▶ " }
                } else {
                    "  "
                };

                let name = &entry.name;
                let is_cut = self
                    .clipboard
                    .as_ref()
                    .is_some_and(|c| c.is_cut && c.path == entry.path);

                let style = if i == self.tree.selected_index {
                    Style::default()
                        .fg(theme.sidebar_selected_fg.to_ratatui_color())
                        .bg(theme.sidebar_selected_bg.to_ratatui_color())
                        .add_modifier(Modifier::BOLD)
                } else if is_cut {
                    Style::default()
                        .fg(theme.sidebar_fg.to_ratatui_color())
                        .add_modifier(Modifier::DIM)
                } else if entry.is_dir {
                    Style::default()
                        .fg(theme.sidebar_fg.to_ratatui_color())
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.sidebar_fg.to_ratatui_color())
                };

                ListItem::new(Line::from(Span::styled(
                    format!("{}{}{}", indent, icon, name),
                    style,
                )))
            })
            .collect();

        let list = List::new(items);
        frame.render_widget(list, inner);

        // Render input overlay if active
        if self.rename_input.is_some() || self.new_file_input.is_some() {
            let label = if self.rename_input.is_some() {
                "Rename: "
            } else if self.new_file_input.as_ref().is_some_and(|(_, is_dir)| *is_dir) {
                "New dir: "
            } else {
                "New file: "
            };
            let input_line = format!("{}{}_", label, self.input_buffer);
            let input_area = Rect::new(
                inner.x,
                inner.y + inner.height.saturating_sub(1),
                inner.width,
                1,
            );
            let input_widget = ratatui::widgets::Paragraph::new(input_line)
                .style(Style::default().fg(Color::Yellow).bg(Color::DarkGray));
            frame.render_widget(input_widget, input_area);
        }
    }
}

impl Component for Sidebar {
    fn handle_action(&mut self, action: &Action) -> AppCommand {
        // If we're in input mode (rename or new file), handle it specially
        if self.rename_input.is_some() || self.new_file_input.is_some() {
            match action {
                Action::InsertChar(ch) => {
                    self.handle_input_char(*ch);
                    return AppCommand::Nothing;
                }
                Action::DeleteBackward => {
                    self.handle_input_backspace();
                    return AppCommand::Nothing;
                }
                Action::InsertNewline | Action::TreeOpen => {
                    // Confirm input
                    let input = self.input_buffer.clone();
                    if !input.is_empty() {
                        if let Some(_) = self.rename_input.take() {
                            if let Some(path) = self.selected_path() {
                                self.input_buffer.clear();
                                return AppCommand::FileRename {
                                    from: path,
                                    to: input,
                                };
                            }
                        } else if let Some((parent, is_dir)) = self.new_file_input.take() {
                            let new_path = parent.join(&input);
                            self.input_buffer.clear();
                            if is_dir {
                                return AppCommand::DirNew(new_path);
                            } else {
                                return AppCommand::FileNew(new_path);
                            }
                        }
                    }
                    self.cancel_input();
                    return AppCommand::Nothing;
                }
                Action::CompletionDismiss | Action::PaletteDismiss => {
                    self.cancel_input();
                    return AppCommand::Nothing;
                }
                _ => return AppCommand::Nothing,
            }
        }

        match action {
            Action::TreeUp => {
                self.tree.select_previous();
                AppCommand::Nothing
            }
            Action::TreeDown => {
                self.tree.select_next();
                AppCommand::Nothing
            }
            Action::TreeExpand => {
                if self.selected_is_dir() {
                    self.tree.toggle_expand();
                }
                AppCommand::Nothing
            }
            Action::TreeCollapse => {
                if self.selected_is_dir() {
                    self.tree.toggle_expand();
                }
                AppCommand::Nothing
            }
            Action::TreeOpen => {
                if let Some(entry) = self.tree.selected_entry() {
                    if entry.is_dir {
                        self.tree.toggle_expand();
                        AppCommand::Nothing
                    } else {
                        AppCommand::OpenFile(entry.path.clone())
                    }
                } else {
                    AppCommand::Nothing
                }
            }
            Action::FileCopy => {
                if let Some(path) = self.selected_path() {
                    self.clipboard = Some(FileClipboard {
                        path,
                        is_cut: false,
                    });
                }
                AppCommand::Nothing
            }
            Action::FileCut => {
                if let Some(path) = self.selected_path() {
                    self.clipboard = Some(FileClipboard {
                        path,
                        is_cut: true,
                    });
                }
                AppCommand::Nothing
            }
            Action::FilePaste => {
                if let Some(clip) = self.clipboard.take() {
                    if let Some(target_dir) = self.selected_parent_dir() {
                        if clip.is_cut {
                            return AppCommand::FileCut(clip.path);
                        } else {
                            return AppCommand::FileCopy(clip.path);
                        }
                    }
                }
                AppCommand::Nothing
            }
            Action::FileDelete => {
                if let Some(path) = self.selected_path() {
                    AppCommand::FileDelete(path)
                } else {
                    AppCommand::Nothing
                }
            }
            Action::FileRename => {
                if let Some(entry) = self.tree.selected_entry() {
                    self.rename_input = Some(entry.name.clone());
                    self.input_buffer = entry.name.clone();
                }
                AppCommand::Nothing
            }
            Action::FileNew => {
                if let Some(dir) = self.selected_parent_dir() {
                    self.new_file_input = Some((dir, false));
                    self.input_buffer.clear();
                }
                AppCommand::Nothing
            }
            Action::DirNew => {
                if let Some(dir) = self.selected_parent_dir() {
                    self.new_file_input = Some((dir, true));
                    self.input_buffer.clear();
                }
                AppCommand::Nothing
            }
            _ => AppCommand::Nothing,
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, focused: bool) {
        let theme = Theme::default();
        self.render_tree(frame, area, focused, &theme);
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
        fs::write(tmp.path().join("src/main.rs"), "fn main() {}").unwrap();
        fs::write(tmp.path().join("src/lib.rs"), "// lib").unwrap();
        fs::write(tmp.path().join("Cargo.toml"), "[package]").unwrap();
        tmp
    }

    #[test]
    fn test_sidebar_new() {
        let tmp = setup_test_dir();
        let sidebar = Sidebar::new(tmp.path(), 30).unwrap();
        assert!(sidebar.visible);
        assert_eq!(sidebar.width, 30);
    }

    #[test]
    fn test_toggle_visibility() {
        let tmp = setup_test_dir();
        let mut sidebar = Sidebar::new(tmp.path(), 30).unwrap();
        assert!(sidebar.visible);
        sidebar.toggle_visibility();
        assert!(!sidebar.visible);
        sidebar.toggle_visibility();
        assert!(sidebar.visible);
    }

    #[test]
    fn test_navigation() {
        let tmp = setup_test_dir();
        let mut sidebar = Sidebar::new(tmp.path(), 30).unwrap();
        // Expand root to see children
        sidebar.tree.toggle_expand();
        sidebar.tree.select_next();
        assert!(sidebar.selected_path().is_some());
    }

    #[test]
    fn test_file_clipboard() {
        let tmp = setup_test_dir();
        let mut sidebar = Sidebar::new(tmp.path(), 30).unwrap();
        sidebar.tree.toggle_expand();
        sidebar.tree.select_next();

        sidebar.handle_action(&Action::FileCopy);
        assert!(sidebar.clipboard.is_some());
        assert!(!sidebar.clipboard.as_ref().unwrap().is_cut);
    }

    #[test]
    fn test_file_cut() {
        let tmp = setup_test_dir();
        let mut sidebar = Sidebar::new(tmp.path(), 30).unwrap();
        sidebar.tree.toggle_expand();
        sidebar.tree.select_next();

        sidebar.handle_action(&Action::FileCut);
        assert!(sidebar.clipboard.is_some());
        assert!(sidebar.clipboard.as_ref().unwrap().is_cut);
    }

    #[test]
    fn test_tree_open_file() {
        let tmp = setup_test_dir();
        let mut sidebar = Sidebar::new(tmp.path(), 30).unwrap();
        // Root is a dir, expand it
        sidebar.tree.toggle_expand();
        // Navigate to a file
        for _ in 0..3 {
            sidebar.tree.select_next();
        }
        if let Some(entry) = sidebar.tree.selected_entry() {
            if !entry.is_dir {
                let cmd = sidebar.handle_action(&Action::TreeOpen);
                assert!(matches!(cmd, AppCommand::OpenFile(_)));
            }
        }
    }

    #[test]
    fn test_rename_mode() {
        let tmp = setup_test_dir();
        let mut sidebar = Sidebar::new(tmp.path(), 30).unwrap();
        sidebar.tree.toggle_expand();
        sidebar.tree.select_next();

        sidebar.handle_action(&Action::FileRename);
        assert!(sidebar.rename_input.is_some());

        // Type characters
        sidebar.handle_action(&Action::InsertChar('t'));
        sidebar.handle_action(&Action::InsertChar('e'));
        assert!(sidebar.input_buffer.contains("te"));

        // Cancel
        sidebar.handle_action(&Action::CompletionDismiss);
        assert!(sidebar.rename_input.is_none());
        assert!(sidebar.input_buffer.is_empty());
    }

    #[test]
    fn test_new_file_mode() {
        let tmp = setup_test_dir();
        let mut sidebar = Sidebar::new(tmp.path(), 30).unwrap();
        sidebar.tree.toggle_expand();

        sidebar.handle_action(&Action::FileNew);
        assert!(sidebar.new_file_input.is_some());
        assert!(!sidebar.new_file_input.as_ref().unwrap().1); // not a dir
    }

    #[test]
    fn test_new_dir_mode() {
        let tmp = setup_test_dir();
        let mut sidebar = Sidebar::new(tmp.path(), 30).unwrap();
        sidebar.tree.toggle_expand();

        sidebar.handle_action(&Action::DirNew);
        assert!(sidebar.new_file_input.is_some());
        assert!(sidebar.new_file_input.as_ref().unwrap().1); // is a dir
    }

    #[test]
    fn test_delete_returns_command() {
        let tmp = setup_test_dir();
        let mut sidebar = Sidebar::new(tmp.path(), 30).unwrap();
        sidebar.tree.toggle_expand();
        sidebar.tree.select_next();

        let cmd = sidebar.handle_action(&Action::FileDelete);
        if sidebar.selected_path().is_some() {
            assert!(matches!(cmd, AppCommand::FileDelete(_)));
        }
    }
}
