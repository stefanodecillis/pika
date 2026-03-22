use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use notify_debouncer_mini::{new_debouncer, notify::Watcher, DebounceEventResult, Debouncer};

use crate::events::FileChangeEvent;

/// Wraps a debounced file-system watcher that forwards change events over a
/// tokio unbounded channel.
pub struct FileWatcher {
    debouncer: Debouncer<notify::RecommendedWatcher>,
}

impl FileWatcher {
    /// Create a new `FileWatcher`.
    ///
    /// Debounced events are converted to [`FileChangeEvent`] values and sent
    /// through `tx`. The debounce timeout is 100 ms.
    pub fn new(tx: tokio::sync::mpsc::UnboundedSender<FileChangeEvent>) -> Result<Self> {
        let debouncer = new_debouncer(
            Duration::from_millis(100),
            move |result: DebounceEventResult| {
                match result {
                    Ok(events) => {
                        for event in events {
                            // DebouncedEvent only tells us *a path changed*;
                            // we check the filesystem to decide what happened.
                            let path = event.path;
                            let change = if path.exists() {
                                // Could be a create or modify; without
                                // tracking previous state we report Modified
                                // for existing paths.
                                FileChangeEvent::Modified(path)
                            } else {
                                FileChangeEvent::Deleted(path)
                            };
                            let _ = tx.send(change);
                        }
                    }
                    Err(err) => {
                        log::error!("file watcher error: {:?}", err);
                    }
                }
            },
        )
        .context("failed to create debounced file watcher")?;

        Ok(Self { debouncer })
    }

    /// Start watching a path (recursively).
    pub fn watch(&mut self, path: &Path) -> Result<()> {
        self.debouncer
            .watcher()
            .watch(path, notify::RecursiveMode::Recursive)
            .with_context(|| format!("failed to watch path: {}", path.display()))?;
        Ok(())
    }

    /// Stop watching a path.
    pub fn unwatch(&mut self, path: &Path) -> Result<()> {
        self.debouncer
            .watcher()
            .unwatch(path)
            .with_context(|| format!("failed to unwatch path: {}", path.display()))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup() -> (TempDir, tokio::sync::mpsc::UnboundedReceiver<FileChangeEvent>) {
        let tmp = TempDir::new().unwrap();
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        (tmp, rx)
    }

    #[test]
    fn test_watcher_creation() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let watcher = FileWatcher::new(tx);
        assert!(watcher.is_ok());
    }

    #[test]
    fn test_watch_valid_path() {
        let (tmp, _rx) = setup();
        let (tx, _rx2) = tokio::sync::mpsc::unbounded_channel();
        let mut watcher = FileWatcher::new(tx).unwrap();
        assert!(watcher.watch(tmp.path()).is_ok());
    }

    #[test]
    fn test_watch_nonexistent_path() {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        let mut watcher = FileWatcher::new(tx).unwrap();
        let result = watcher.watch(Path::new("/nonexistent/path/12345"));
        assert!(result.is_err());
    }

    #[test]
    fn test_unwatch_after_watch() {
        let (tmp, _rx) = setup();
        let (tx, _rx2) = tokio::sync::mpsc::unbounded_channel();
        let mut watcher = FileWatcher::new(tx).unwrap();
        watcher.watch(tmp.path()).unwrap();
        assert!(watcher.unwatch(tmp.path()).is_ok());
    }

    #[test]
    fn test_unwatch_without_watch() {
        let (tmp, _rx) = setup();
        let (tx, _rx2) = tokio::sync::mpsc::unbounded_channel();
        let mut watcher = FileWatcher::new(tx).unwrap();
        // Unwatching a path that was never watched should error
        let result = watcher.unwatch(tmp.path());
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_watcher_receives_events() {
        let tmp = TempDir::new().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        let mut watcher = FileWatcher::new(tx).unwrap();
        watcher.watch(tmp.path()).unwrap();

        // Write a file to trigger an event
        let file_path = tmp.path().join("test.txt");
        fs::write(&file_path, "hello").unwrap();
        // Canonicalize to resolve symlinks (e.g. /var -> /private/var on macOS)
        let file_path = file_path.canonicalize().unwrap();

        // Wait for the debounced event (100ms debounce + some margin)
        let event = tokio::time::timeout(Duration::from_secs(5), rx.recv()).await;

        assert!(event.is_ok(), "timed out waiting for file change event");
        let event = event.unwrap().unwrap();

        // On macOS, file system events may report the parent directory
        // instead of the exact file path. Accept either.
        let event_path = match &event {
            FileChangeEvent::Modified(p) | FileChangeEvent::Created(p) => p.clone(),
            FileChangeEvent::Deleted(p) => p.clone(),
            FileChangeEvent::Renamed { from, .. } => from.clone(),
        };
        assert!(
            event_path == file_path || file_path.starts_with(&event_path),
            "event path {:?} is not related to {:?}",
            event_path,
            file_path
        );
    }

    #[tokio::test]
    async fn test_watcher_detects_deletion() {
        let tmp = TempDir::new().unwrap();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        let file_path = tmp.path().join("delete_me.txt");
        fs::write(&file_path, "bye").unwrap();
        // Canonicalize before deletion to resolve symlinks (e.g. /var -> /private/var on macOS)
        let file_path = file_path.canonicalize().unwrap();

        let mut watcher = FileWatcher::new(tx).unwrap();
        watcher.watch(tmp.path()).unwrap();

        // Small delay so the watcher is ready
        tokio::time::sleep(Duration::from_millis(50)).await;

        fs::remove_file(&file_path).unwrap();

        let event = tokio::time::timeout(Duration::from_secs(5), rx.recv()).await;
        assert!(event.is_ok(), "timed out waiting for delete event");
        let event = event.unwrap().unwrap();

        // On macOS, file system events may report the parent directory
        // instead of the exact file path. Accept either.
        let event_path = match &event {
            FileChangeEvent::Deleted(p)
            | FileChangeEvent::Modified(p)
            | FileChangeEvent::Created(p) => p.clone(),
            FileChangeEvent::Renamed { from, .. } => from.clone(),
        };
        assert!(
            event_path == file_path || file_path.starts_with(&event_path),
            "event path {:?} is not related to {:?}",
            event_path,
            file_path
        );
    }

    #[test]
    fn test_multiple_watches() {
        let tmp1 = TempDir::new().unwrap();
        let tmp2 = TempDir::new().unwrap();
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();

        let mut watcher = FileWatcher::new(tx).unwrap();
        assert!(watcher.watch(tmp1.path()).is_ok());
        assert!(watcher.watch(tmp2.path()).is_ok());
    }
}
