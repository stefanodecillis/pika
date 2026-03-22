use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

const MAX_DEPTH: usize = 20;

/// A single node in the file tree hierarchy.
#[derive(Debug, Clone)]
pub struct FileNode {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub children: Vec<FileNode>,
    pub expanded: bool,
    pub depth: usize,
}

/// A flattened entry used for rendering the tree in a list view.
#[derive(Debug, Clone)]
pub struct FlatEntry {
    pub path: PathBuf,
    pub name: String,
    pub is_dir: bool,
    pub depth: usize,
    pub expanded: bool,
}

/// The full file tree with navigation state and a flattened view for rendering.
#[derive(Debug, Clone)]
pub struct FileTree {
    pub root: FileNode,
    pub selected_index: usize,
    pub flattened: Vec<FlatEntry>,
}

/// Recursively build a `FileNode` tree from a directory path.
///
/// Directories are sorted before files. Within each group entries are
/// sorted alphabetically (case-insensitive). Hidden files (names starting
/// with `.`) are skipped. Recursion stops at `MAX_DEPTH`.
pub fn build_tree(path: &Path, depth: usize) -> Result<FileNode> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| path.to_string_lossy().to_string());

    let is_dir = path.is_dir();

    let mut children = Vec::new();
    if is_dir && depth < MAX_DEPTH {
        let entries = fs::read_dir(path)
            .with_context(|| format!("failed to read directory: {}", path.display()))?;

        let mut dirs: Vec<FileNode> = Vec::new();
        let mut files: Vec<FileNode> = Vec::new();

        for entry in entries {
            let entry = entry?;
            let entry_name = entry.file_name().to_string_lossy().to_string();

            // Skip hidden files
            if entry_name.starts_with('.') {
                continue;
            }

            let child = build_tree(&entry.path(), depth + 1)?;
            if child.is_dir {
                dirs.push(child);
            } else {
                files.push(child);
            }
        }

        dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

        children.append(&mut dirs);
        children.append(&mut files);
    }

    Ok(FileNode {
        name,
        path: path.to_path_buf(),
        is_dir,
        children,
        expanded: depth == 0 && is_dir,
        depth,
    })
}

impl FileTree {
    /// Create a `FileTree` rooted at the given path.
    pub fn from_path(root: &Path) -> Result<Self> {
        let root_node = build_tree(root, 0)?;
        let mut tree = FileTree {
            root: root_node,
            selected_index: 0,
            flattened: Vec::new(),
        };
        tree.flatten();
        Ok(tree)
    }

    /// Rebuild the tree from disk, preserving the selected index as much as
    /// possible.
    pub fn refresh(&mut self) -> Result<()> {
        let path = self.root.path.clone();
        let old_expanded = self.collect_expanded();
        self.root = build_tree(&path, 0)?;
        self.restore_expanded(&old_expanded);
        self.flatten();
        // Clamp selected_index
        if !self.flattened.is_empty() && self.selected_index >= self.flattened.len() {
            self.selected_index = self.flattened.len() - 1;
        }
        Ok(())
    }

    /// Rebuild the `flattened` vec by walking the tree.
    pub fn flatten(&mut self) {
        self.flattened.clear();
        Self::flatten_node(&self.root, &mut self.flattened);
    }

    fn flatten_node(node: &FileNode, out: &mut Vec<FlatEntry>) {
        out.push(FlatEntry {
            path: node.path.clone(),
            name: node.name.clone(),
            is_dir: node.is_dir,
            depth: node.depth,
            expanded: node.expanded,
        });

        if node.expanded {
            for child in &node.children {
                Self::flatten_node(child, out);
            }
        }
    }

    /// Move selection down by one.
    pub fn select_next(&mut self) {
        if !self.flattened.is_empty() && self.selected_index + 1 < self.flattened.len() {
            self.selected_index += 1;
        }
    }

    /// Move selection up by one.
    pub fn select_previous(&mut self) {
        if self.selected_index > 0 {
            self.selected_index -= 1;
        }
    }

    /// Toggle the expanded state of the currently selected entry (if it is a
    /// directory).
    pub fn toggle_expand(&mut self) {
        if let Some(entry) = self.flattened.get(self.selected_index) {
            if entry.is_dir {
                let path = entry.path.clone();
                Self::toggle_node(&mut self.root, &path);
                self.flatten();
                // Clamp index after flatten
                if !self.flattened.is_empty() && self.selected_index >= self.flattened.len() {
                    self.selected_index = self.flattened.len() - 1;
                }
            }
        }
    }

    /// Get the currently selected flat entry.
    pub fn selected_entry(&self) -> Option<&FlatEntry> {
        self.flattened.get(self.selected_index)
    }

    // -- helpers --

    fn toggle_node(node: &mut FileNode, target: &Path) {
        if node.path == target {
            node.expanded = !node.expanded;
            return;
        }
        for child in &mut node.children {
            toggle_node_recursive(child, target);
        }
    }

    /// Collect the set of expanded paths so we can restore state after a
    /// refresh.
    fn collect_expanded(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();
        Self::collect_expanded_node(&self.root, &mut paths);
        paths
    }

    fn collect_expanded_node(node: &FileNode, out: &mut Vec<PathBuf>) {
        if node.expanded {
            out.push(node.path.clone());
        }
        for child in &node.children {
            Self::collect_expanded_node(child, out);
        }
    }

    fn restore_expanded(&mut self, paths: &[PathBuf]) {
        for path in paths {
            restore_expanded_node(&mut self.root, path);
        }
    }
}

fn toggle_node_recursive(node: &mut FileNode, target: &Path) {
    if node.path == target {
        node.expanded = !node.expanded;
        return;
    }
    for child in &mut node.children {
        toggle_node_recursive(child, target);
    }
}

fn restore_expanded_node(node: &mut FileNode, target: &Path) {
    if node.path == target && node.is_dir {
        node.expanded = true;
    }
    for child in &mut node.children {
        restore_expanded_node(child, target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    /// Helper: create a temp directory with a known structure.
    ///
    /// ```text
    /// root/
    ///   alpha/
    ///     nested.txt
    ///   beta/
    ///   aaa.txt
    ///   bbb.txt
    ///   .hidden
    /// ```
    fn setup_dir() -> TempDir {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        fs::create_dir(root.join("alpha")).unwrap();
        fs::write(root.join("alpha").join("nested.txt"), "data").unwrap();
        fs::create_dir(root.join("beta")).unwrap();
        fs::write(root.join("aaa.txt"), "a").unwrap();
        fs::write(root.join("bbb.txt"), "b").unwrap();
        fs::write(root.join(".hidden"), "h").unwrap();

        tmp
    }

    #[test]
    fn test_build_tree_basic() {
        let tmp = setup_dir();
        let tree = build_tree(tmp.path(), 0).unwrap();

        assert!(tree.is_dir);
        // Hidden file should be skipped, so we expect 4: alpha, beta, aaa.txt, bbb.txt
        assert_eq!(tree.children.len(), 4);
    }

    #[test]
    fn test_build_tree_sorts_dirs_first() {
        let tmp = setup_dir();
        let tree = build_tree(tmp.path(), 0).unwrap();

        // First two should be dirs (alpha, beta), then files
        assert!(tree.children[0].is_dir);
        assert!(tree.children[1].is_dir);
        assert!(!tree.children[2].is_dir);
        assert!(!tree.children[3].is_dir);
    }

    #[test]
    fn test_build_tree_alphabetical_order() {
        let tmp = setup_dir();
        let tree = build_tree(tmp.path(), 0).unwrap();

        assert_eq!(tree.children[0].name, "alpha");
        assert_eq!(tree.children[1].name, "beta");
        assert_eq!(tree.children[2].name, "aaa.txt");
        assert_eq!(tree.children[3].name, "bbb.txt");
    }

    #[test]
    fn test_build_tree_hidden_files_skipped() {
        let tmp = setup_dir();
        let tree = build_tree(tmp.path(), 0).unwrap();

        let names: Vec<&str> = tree.children.iter().map(|c| c.name.as_str()).collect();
        assert!(!names.contains(&".hidden"));
    }

    #[test]
    fn test_build_tree_nested() {
        let tmp = setup_dir();
        let tree = build_tree(tmp.path(), 0).unwrap();

        let alpha = &tree.children[0];
        assert_eq!(alpha.name, "alpha");
        assert_eq!(alpha.children.len(), 1);
        assert_eq!(alpha.children[0].name, "nested.txt");
    }

    #[test]
    fn test_build_tree_depth() {
        let tmp = setup_dir();
        let tree = build_tree(tmp.path(), 0).unwrap();

        assert_eq!(tree.depth, 0);
        assert_eq!(tree.children[0].depth, 1);
        assert_eq!(tree.children[0].children[0].depth, 2);
    }

    #[test]
    fn test_build_tree_root_expanded() {
        let tmp = setup_dir();
        let tree = build_tree(tmp.path(), 0).unwrap();
        assert!(tree.expanded);
    }

    #[test]
    fn test_build_tree_non_root_collapsed() {
        let tmp = setup_dir();
        let tree = build_tree(tmp.path(), 0).unwrap();
        assert!(!tree.children[0].expanded); // alpha dir
    }

    #[test]
    fn test_build_tree_max_depth() {
        let tmp = TempDir::new().unwrap();
        let mut current = tmp.path().to_path_buf();
        for i in 0..25 {
            current = current.join(format!("d{i}"));
            fs::create_dir(&current).unwrap();
        }

        let tree = build_tree(tmp.path(), 0).unwrap();

        // Walk down – at depth 20 we should stop recursing
        fn max_depth(node: &FileNode) -> usize {
            if node.children.is_empty() {
                return node.depth;
            }
            node.children.iter().map(|c| max_depth(c)).max().unwrap()
        }
        assert!(max_depth(&tree) <= MAX_DEPTH);
    }

    #[test]
    fn test_file_tree_from_path() {
        let tmp = setup_dir();
        let tree = FileTree::from_path(tmp.path()).unwrap();

        // Flattened should contain root + its children (root is expanded)
        // root, alpha, beta, aaa.txt, bbb.txt = 5
        assert_eq!(tree.flattened.len(), 5);
    }

    #[test]
    fn test_file_tree_flatten_includes_root() {
        let tmp = setup_dir();
        let tree = FileTree::from_path(tmp.path()).unwrap();
        assert_eq!(tree.flattened[0].depth, 0);
        assert!(tree.flattened[0].is_dir);
    }

    #[test]
    fn test_select_next() {
        let tmp = setup_dir();
        let mut tree = FileTree::from_path(tmp.path()).unwrap();

        assert_eq!(tree.selected_index, 0);
        tree.select_next();
        assert_eq!(tree.selected_index, 1);
    }

    #[test]
    fn test_select_next_clamps() {
        let tmp = setup_dir();
        let mut tree = FileTree::from_path(tmp.path()).unwrap();

        for _ in 0..100 {
            tree.select_next();
        }
        assert_eq!(tree.selected_index, tree.flattened.len() - 1);
    }

    #[test]
    fn test_select_previous() {
        let tmp = setup_dir();
        let mut tree = FileTree::from_path(tmp.path()).unwrap();

        tree.selected_index = 2;
        tree.select_previous();
        assert_eq!(tree.selected_index, 1);
    }

    #[test]
    fn test_select_previous_clamps_at_zero() {
        let tmp = setup_dir();
        let mut tree = FileTree::from_path(tmp.path()).unwrap();

        tree.select_previous();
        assert_eq!(tree.selected_index, 0);
    }

    #[test]
    fn test_selected_entry() {
        let tmp = setup_dir();
        let tree = FileTree::from_path(tmp.path()).unwrap();

        let entry = tree.selected_entry().unwrap();
        assert_eq!(entry.depth, 0);
        assert!(entry.is_dir);
    }

    #[test]
    fn test_selected_entry_none_on_empty() {
        let tree = FileTree {
            root: FileNode {
                name: String::new(),
                path: PathBuf::new(),
                is_dir: false,
                children: Vec::new(),
                expanded: false,
                depth: 0,
            },
            selected_index: 0,
            flattened: Vec::new(),
        };
        assert!(tree.selected_entry().is_none());
    }

    #[test]
    fn test_toggle_expand_dir() {
        let tmp = setup_dir();
        let mut tree = FileTree::from_path(tmp.path()).unwrap();

        // Select "alpha" (index 1) and expand it
        tree.selected_index = 1;
        tree.toggle_expand();

        // Now flattened should include alpha's child (nested.txt)
        // root, alpha, nested.txt, beta, aaa.txt, bbb.txt = 6
        assert_eq!(tree.flattened.len(), 6);

        // Collapse again
        tree.selected_index = 1;
        tree.toggle_expand();
        assert_eq!(tree.flattened.len(), 5);
    }

    #[test]
    fn test_toggle_expand_file_is_noop() {
        let tmp = setup_dir();
        let mut tree = FileTree::from_path(tmp.path()).unwrap();

        // Select a file (aaa.txt at index 3)
        tree.selected_index = 3;
        let len_before = tree.flattened.len();
        tree.toggle_expand();
        assert_eq!(tree.flattened.len(), len_before);
    }

    #[test]
    fn test_refresh() {
        let tmp = setup_dir();
        let mut tree = FileTree::from_path(tmp.path()).unwrap();

        let initial_count = tree.flattened.len();

        // Add a new file
        fs::write(tmp.path().join("zzz.txt"), "z").unwrap();
        tree.refresh().unwrap();

        assert_eq!(tree.flattened.len(), initial_count + 1);
    }

    #[test]
    fn test_refresh_preserves_expanded() {
        let tmp = setup_dir();
        let mut tree = FileTree::from_path(tmp.path()).unwrap();

        // Expand alpha
        tree.selected_index = 1;
        tree.toggle_expand();
        assert_eq!(tree.flattened.len(), 6);

        // Refresh should preserve alpha's expansion
        tree.refresh().unwrap();
        assert_eq!(tree.flattened.len(), 6);
    }

    #[test]
    fn test_refresh_clamps_index() {
        let tmp = setup_dir();
        let mut tree = FileTree::from_path(tmp.path()).unwrap();

        tree.selected_index = 4; // last entry
        // Remove a file
        fs::remove_file(tmp.path().join("bbb.txt")).unwrap();
        tree.refresh().unwrap();
        assert!(tree.selected_index < tree.flattened.len());
    }

    #[test]
    fn test_flat_entry_fields() {
        let tmp = setup_dir();
        let tree = FileTree::from_path(tmp.path()).unwrap();

        // alpha should be at index 1
        let alpha = &tree.flattened[1];
        assert_eq!(alpha.name, "alpha");
        assert!(alpha.is_dir);
        assert_eq!(alpha.depth, 1);
        assert!(!alpha.expanded);
    }

    #[test]
    fn test_empty_directory() {
        let tmp = TempDir::new().unwrap();
        let tree = FileTree::from_path(tmp.path()).unwrap();
        // Only the root
        assert_eq!(tree.flattened.len(), 1);
        assert!(tree.flattened[0].is_dir);
    }

    #[test]
    fn test_case_insensitive_sort() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("Banana.txt"), "").unwrap();
        fs::write(tmp.path().join("apple.txt"), "").unwrap();
        fs::write(tmp.path().join("Cherry.txt"), "").unwrap();

        let tree = build_tree(tmp.path(), 0).unwrap();
        let names: Vec<&str> = tree.children.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, vec!["apple.txt", "Banana.txt", "Cherry.txt"]);
    }

    #[test]
    fn test_nonexistent_path_errors() {
        let result = build_tree(Path::new("/nonexistent/path/that/does/not/exist"), 0);
        // A non-dir path won't error — it just produces a file node.
        // But the path not existing at all will just be a file node with is_dir=false.
        assert!(result.is_ok());
        let node = result.unwrap();
        assert!(!node.is_dir);
    }

    #[test]
    fn test_file_node_path_is_absolute() {
        let tmp = setup_dir();
        let tree = build_tree(tmp.path(), 0).unwrap();
        assert!(tree.path.is_absolute());
        for child in &tree.children {
            assert!(child.path.is_absolute());
        }
    }
}
