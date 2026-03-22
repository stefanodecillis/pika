use std::path::PathBuf;

/// Handles file drops via bracketed paste in the terminal.
pub struct DropHandler;

impl DropHandler {
    /// Parse content that was received via a bracketed paste event.
    ///
    /// Splits the input by newlines, trims each line, expands a leading `~` to
    /// the user's home directory, and returns only those paths that point to
    /// existing files or directories on disk.
    pub fn parse_dropped_content(content: &str) -> Vec<PathBuf> {
        content
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    return None;
                }

                let expanded = expand_tilde(trimmed);
                let path = PathBuf::from(&expanded);

                if path.exists() {
                    Some(path)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Heuristic to decide whether pasted content looks like file paths
    /// rather than arbitrary text.
    ///
    /// Returns `true` when every non-empty line appears to be a file-system
    /// path (starts with `/`, `~`, or a Windows drive letter like `C:\`) *and*
    /// at least one of those paths exists on disk.
    pub fn is_file_drop(content: &str) -> bool {
        let lines: Vec<&str> = content
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();

        if lines.is_empty() {
            return false;
        }

        // Every line must look like a path
        let all_look_like_paths = lines.iter().all(|line| looks_like_path(line));
        if !all_look_like_paths {
            return false;
        }

        // At least one must actually exist
        lines.iter().any(|line| {
            let expanded = expand_tilde(line);
            PathBuf::from(&expanded).exists()
        })
    }
}

/// Check if a string looks like a file-system path.
fn looks_like_path(s: &str) -> bool {
    if s.starts_with('/') || s.starts_with('~') {
        return true;
    }

    // Windows drive letter: e.g. C:\, D:/
    if s.len() >= 3 {
        let bytes = s.as_bytes();
        if bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && (bytes[2] == b'\\' || bytes[2] == b'/')
        {
            return true;
        }
    }

    false
}

/// Expand a leading `~` to the user's home directory.
fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix('~') {
        if let Some(home) = dirs::home_dir() {
            let rest = rest.strip_prefix('/').unwrap_or(rest);
            return home.join(rest).to_string_lossy().to_string();
        }
    }
    path.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // ---------------------------------------------------------------
    // looks_like_path
    // ---------------------------------------------------------------

    #[test]
    fn test_looks_like_path_absolute_unix() {
        assert!(looks_like_path("/usr/bin/env"));
        assert!(looks_like_path("/tmp/file.txt"));
    }

    #[test]
    fn test_looks_like_path_tilde() {
        assert!(looks_like_path("~/Documents/file.txt"));
        assert!(looks_like_path("~"));
    }

    #[test]
    fn test_looks_like_path_windows() {
        assert!(looks_like_path("C:\\Users\\file.txt"));
        assert!(looks_like_path("D:/folder/file"));
    }

    #[test]
    fn test_looks_like_path_not_a_path() {
        assert!(!looks_like_path("hello world"));
        assert!(!looks_like_path("fn main() {}"));
        assert!(!looks_like_path(""));
        assert!(!looks_like_path("relative/path"));
    }

    // ---------------------------------------------------------------
    // expand_tilde
    // ---------------------------------------------------------------

    #[test]
    fn test_expand_tilde_with_subpath() {
        let expanded = expand_tilde("~/Documents");
        assert!(!expanded.starts_with('~'));
        assert!(expanded.contains("Documents"));
    }

    #[test]
    fn test_expand_tilde_just_tilde() {
        let expanded = expand_tilde("~");
        assert!(!expanded.starts_with('~'));
    }

    #[test]
    fn test_expand_tilde_no_tilde() {
        let input = "/usr/bin/env";
        assert_eq!(expand_tilde(input), input);
    }

    // ---------------------------------------------------------------
    // parse_dropped_content
    // ---------------------------------------------------------------

    #[test]
    fn test_parse_dropped_content_existing_files() {
        let tmp = TempDir::new().unwrap();
        let f1 = tmp.path().join("a.txt");
        let f2 = tmp.path().join("b.txt");
        fs::write(&f1, "a").unwrap();
        fs::write(&f2, "b").unwrap();

        let content = format!("{}\n{}\n", f1.display(), f2.display());
        let paths = DropHandler::parse_dropped_content(&content);

        assert_eq!(paths.len(), 2);
        assert!(paths.contains(&f1));
        assert!(paths.contains(&f2));
    }

    #[test]
    fn test_parse_dropped_content_skips_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let existing = tmp.path().join("real.txt");
        fs::write(&existing, "ok").unwrap();

        let content = format!(
            "{}\n/totally/fake/path.txt\n",
            existing.display()
        );
        let paths = DropHandler::parse_dropped_content(&content);

        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], existing);
    }

    #[test]
    fn test_parse_dropped_content_empty() {
        let paths = DropHandler::parse_dropped_content("");
        assert!(paths.is_empty());
    }

    #[test]
    fn test_parse_dropped_content_whitespace_only() {
        let paths = DropHandler::parse_dropped_content("   \n  \n\n");
        assert!(paths.is_empty());
    }

    #[test]
    fn test_parse_dropped_content_trims_whitespace() {
        let tmp = TempDir::new().unwrap();
        let f = tmp.path().join("file.txt");
        fs::write(&f, "x").unwrap();

        let content = format!("  {}  \n", f.display());
        let paths = DropHandler::parse_dropped_content(&content);
        assert_eq!(paths.len(), 1);
    }

    #[test]
    fn test_parse_dropped_content_directories() {
        let tmp = TempDir::new().unwrap();
        let d = tmp.path().join("subdir");
        fs::create_dir(&d).unwrap();

        let content = format!("{}\n", d.display());
        let paths = DropHandler::parse_dropped_content(&content);
        assert_eq!(paths.len(), 1);
        assert_eq!(paths[0], d);
    }

    // ---------------------------------------------------------------
    // is_file_drop
    // ---------------------------------------------------------------

    #[test]
    fn test_is_file_drop_true() {
        let tmp = TempDir::new().unwrap();
        let f = tmp.path().join("dropped.txt");
        fs::write(&f, "data").unwrap();

        let content = format!("{}", f.display());
        assert!(DropHandler::is_file_drop(&content));
    }

    #[test]
    fn test_is_file_drop_multiple_paths() {
        let tmp = TempDir::new().unwrap();
        let f1 = tmp.path().join("a.txt");
        let f2 = tmp.path().join("b.txt");
        fs::write(&f1, "a").unwrap();
        fs::write(&f2, "b").unwrap();

        let content = format!("{}\n{}", f1.display(), f2.display());
        assert!(DropHandler::is_file_drop(&content));
    }

    #[test]
    fn test_is_file_drop_false_for_code() {
        assert!(!DropHandler::is_file_drop("fn main() { println!(\"hi\"); }"));
    }

    #[test]
    fn test_is_file_drop_false_for_empty() {
        assert!(!DropHandler::is_file_drop(""));
        assert!(!DropHandler::is_file_drop("   \n  "));
    }

    #[test]
    fn test_is_file_drop_false_mixed() {
        let tmp = TempDir::new().unwrap();
        let f = tmp.path().join("real.txt");
        fs::write(&f, "x").unwrap();

        // One line is a path, the other is not
        let content = format!("{}\nsome random text", f.display());
        assert!(!DropHandler::is_file_drop(&content));
    }

    #[test]
    fn test_is_file_drop_false_paths_not_existing() {
        let content = "/this/path/does/not/exist/at/all.txt";
        assert!(!DropHandler::is_file_drop(content));
    }

    #[test]
    fn test_is_file_drop_tilde_path() {
        // ~ itself should exist as the home dir on any system where this
        // test runs.
        assert!(DropHandler::is_file_drop("~"));
    }

    // ---------------------------------------------------------------
    // DropHandler struct
    // ---------------------------------------------------------------

    #[test]
    fn test_drop_handler_is_constructible() {
        let _handler = DropHandler;
    }
}
