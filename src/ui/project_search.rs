use std::path::{Path, PathBuf};

use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, Paragraph};
use ratatui::Frame;

use crate::config::Theme;

const MAX_RESULTS: usize = 200;

/// A single line match found during project-wide search.
pub struct SearchResult {
    pub path: PathBuf,
    pub line_number: usize, // 1-indexed
    pub line_content: String,
}

/// Project-wide text search overlay.
pub struct ProjectSearch {
    pub visible: bool,
    pub query: String,
    pub results: Vec<SearchResult>,
    pub selected: usize,
}

impl ProjectSearch {
    pub fn new() -> Self {
        Self {
            visible: false,
            query: String::new(),
            results: Vec::new(),
            selected: 0,
        }
    }

    pub fn show(&mut self, root: &Path) {
        self.visible = true;
        self.query.clear();
        self.results.clear();
        self.selected = 0;
        let _ = root; // no search until user types
    }

    pub fn hide(&mut self) {
        self.visible = false;
    }

    pub fn push_char(&mut self, ch: char, root: &Path) {
        self.query.push(ch);
        self.run_search(root);
    }

    pub fn pop_char(&mut self, root: &Path) {
        self.query.pop();
        self.run_search(root);
    }

    pub fn select_next(&mut self) {
        if !self.results.is_empty() {
            self.selected = (self.selected + 1) % self.results.len();
        }
    }

    pub fn select_previous(&mut self) {
        if !self.results.is_empty() {
            self.selected = if self.selected == 0 {
                self.results.len() - 1
            } else {
                self.selected - 1
            };
        }
    }

    /// Returns the selected result's (path, 0-indexed line) if any.
    pub fn accept(&self) -> Option<(PathBuf, usize)> {
        self.results.get(self.selected).map(|r| (r.path.clone(), r.line_number - 1))
    }

    fn run_search(&mut self, root: &Path) {
        self.results.clear();
        self.selected = 0;
        if self.query.len() < 2 {
            return;
        }
        self.results = search_project(root, &self.query);
    }

    pub fn render(&self, frame: &mut Frame, area: Rect, theme: &Theme) {
        // Centered overlay: 80% width, 70% height
        let overlay_width = (area.width * 4 / 5).max(40).min(area.width);
        let overlay_height = (area.height * 7 / 10).max(10).min(area.height);
        let x = area.x + (area.width.saturating_sub(overlay_width)) / 2;
        let y = area.y + (area.height.saturating_sub(overlay_height)) / 2;
        let overlay_area = Rect::new(x, y, overlay_width, overlay_height);

        // Clear background
        frame.render_widget(Clear, overlay_area);

        let block = Block::default()
            .title(" Project Search ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan))
            .style(
                Style::default()
                    .bg(theme.editor_bg.to_ratatui_color())
                    .fg(theme.editor_fg.to_ratatui_color()),
            );

        let inner = block.inner(overlay_area);
        frame.render_widget(block, overlay_area);

        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(1), Constraint::Min(1)])
            .split(inner);

        // Query input line
        let query_line = format!(" Search: {}▌", self.query);
        let query_widget = Paragraph::new(query_line)
            .style(Style::default().fg(Color::Yellow).bg(Color::Rgb(40, 40, 60)));
        frame.render_widget(query_widget, chunks[0]);

        // Results list
        if self.query.len() < 2 {
            let hint = Paragraph::new(" Type at least 2 characters to search…")
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(hint, chunks[1]);
            return;
        }

        if self.results.is_empty() {
            let hint = Paragraph::new(" No results found.")
                .style(Style::default().fg(Color::DarkGray));
            frame.render_widget(hint, chunks[1]);
            return;
        }

        let items: Vec<ListItem> = self
            .results
            .iter()
            .enumerate()
            .map(|(i, r)| {
                let file_name = r.path.file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("?");
                let label = format!("{}:{}: {}", file_name, r.line_number, r.line_content.trim());
                let style = if i == self.selected {
                    Style::default()
                        .fg(Color::Cyan)
                        .bg(Color::Rgb(40, 60, 80))
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default().fg(theme.editor_fg.to_ratatui_color())
                };
                ListItem::new(Line::from(Span::styled(label, style)))
            })
            .collect();

        let list = List::new(items);
        frame.render_widget(list, chunks[1]);
    }
}

/// Recursively search `root` for lines containing `query` (case-insensitive).
/// Skips hidden directories, `.git`, `target`, `node_modules`, and binary extensions.
fn search_project(root: &Path, query: &str) -> Vec<SearchResult> {
    let query_lower = query.to_lowercase();
    let mut results = Vec::new();
    search_dir(root, root, &query_lower, &mut results);
    results
}

fn search_dir(root: &Path, dir: &Path, query: &str, results: &mut Vec<SearchResult>) {
    if results.len() >= MAX_RESULTS {
        return;
    }

    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        if results.len() >= MAX_RESULTS {
            return;
        }

        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip hidden dirs and known large dirs
        if name_str.starts_with('.') || name_str == "target" || name_str == "node_modules" {
            continue;
        }

        if path.is_dir() {
            search_dir(root, &path, query, results);
        } else if path.is_file() && !is_binary_extension(&path) {
            search_file(&path, query, results);
        }
    }
}

fn search_file(path: &Path, query: &str, results: &mut Vec<SearchResult>) {
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return,
    };

    for (i, line) in content.lines().enumerate() {
        if results.len() >= MAX_RESULTS {
            return;
        }
        if line.to_lowercase().contains(query) {
            results.push(SearchResult {
                path: path.to_path_buf(),
                line_number: i + 1,
                line_content: line.to_string(),
            });
        }
    }
}

fn is_binary_extension(path: &Path) -> bool {
    match path.extension().and_then(|e| e.to_str()) {
        Some(ext) => matches!(
            ext,
            "png" | "jpg" | "jpeg" | "gif" | "bmp" | "ico" | "webp"
            | "pdf" | "zip" | "gz" | "tar" | "tgz" | "bz2" | "xz" | "7z" | "rar"
            | "exe" | "dll" | "so" | "dylib" | "a" | "lib"
            | "wasm" | "bin" | "dat" | "db" | "sqlite" | "sqlite3"
            | "mp3" | "mp4" | "wav" | "ogg" | "flac" | "avi" | "mkv" | "mov"
            | "ttf" | "otf" | "woff" | "woff2"
            | "lock"
        ),
        None => false,
    }
}
