use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

/// Given a desired target path, if it already exists, append " (copy)" or
/// " (N)" to the stem until we find a name that does not collide.
fn resolve_conflict(target: &Path) -> PathBuf {
    if !target.exists() {
        return target.to_path_buf();
    }

    let parent = target.parent().unwrap_or(Path::new("."));
    let stem = target
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let ext = target.extension().map(|e| e.to_string_lossy().to_string());

    // First attempt: "<stem> (copy).ext"
    let make_name = |suffix: &str| -> PathBuf {
        match &ext {
            Some(e) => parent.join(format!("{stem}{suffix}.{e}")),
            None => parent.join(format!("{stem}{suffix}")),
        }
    };

    let candidate = make_name(" (copy)");
    if !candidate.exists() {
        return candidate;
    }

    // Subsequent attempts: "<stem> (N).ext"
    for n in 2u32.. {
        let candidate = make_name(&format!(" ({n})"));
        if !candidate.exists() {
            return candidate;
        }
    }

    // Unreachable in practice
    target.to_path_buf()
}

/// Copy a file or directory from `from` to `to`.
///
/// If `to` is an existing directory the file is copied *into* it.
/// Returns the final destination path. Handles name conflicts by appending
/// a suffix.
pub async fn copy_file(from: &Path, to: &Path) -> Result<PathBuf> {
    let dest = if to.is_dir() {
        let name = from
            .file_name()
            .context("source path has no file name")?;
        to.join(name)
    } else {
        to.to_path_buf()
    };

    let dest = resolve_conflict(&dest);

    if from.is_dir() {
        copy_dir_recursive(from, &dest).await?;
    } else {
        if let Some(parent) = dest.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::copy(from, &dest)
            .await
            .with_context(|| format!("failed to copy {} -> {}", from.display(), dest.display()))?;
    }

    Ok(dest)
}

/// Recursively copy a directory.
async fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    tokio::fs::create_dir_all(dst).await?;
    let mut entries = tokio::fs::read_dir(src).await?;
    while let Some(entry) = entries.next_entry().await? {
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            Box::pin(copy_dir_recursive(&src_path, &dst_path)).await?;
        } else {
            tokio::fs::copy(&src_path, &dst_path).await?;
        }
    }
    Ok(())
}

/// Move a file or directory from `from` to `to`.
///
/// If `to` is an existing directory the file is moved *into* it.
/// Returns the final destination path.
pub async fn move_file(from: &Path, to: &Path) -> Result<PathBuf> {
    let dest = if to.is_dir() {
        let name = from
            .file_name()
            .context("source path has no file name")?;
        to.join(name)
    } else {
        to.to_path_buf()
    };

    let dest = resolve_conflict(&dest);

    if let Some(parent) = dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }

    // Try rename first (fast, same-filesystem move)
    match tokio::fs::rename(from, &dest).await {
        Ok(()) => {}
        Err(_) => {
            // Cross-filesystem: copy then remove
            if from.is_dir() {
                copy_dir_recursive(from, &dest).await?;
                tokio::fs::remove_dir_all(from).await?;
            } else {
                tokio::fs::copy(from, &dest).await?;
                tokio::fs::remove_file(from).await?;
            }
        }
    }

    Ok(dest)
}

/// Move a file to the system trash / recycle bin using the `trash` crate.
pub async fn delete_to_trash(path: &Path) -> Result<()> {
    let path = path.to_path_buf();
    tokio::task::spawn_blocking(move || {
        trash::delete(&path)
            .map_err(|e| anyhow::anyhow!("trash delete failed: {e}"))
    })
    .await??;
    Ok(())
}

/// Rename a file or directory within its current parent directory.
///
/// Returns the new path.
pub async fn rename_file(from: &Path, new_name: &str) -> Result<PathBuf> {
    let parent = from
        .parent()
        .context("cannot rename: path has no parent")?;
    let dest = parent.join(new_name);
    let dest = resolve_conflict(&dest);

    tokio::fs::rename(from, &dest)
        .await
        .with_context(|| {
            format!(
                "failed to rename {} -> {}",
                from.display(),
                dest.display()
            )
        })?;

    Ok(dest)
}

/// Create an empty file at the given path. Parent directories are created
/// as needed.
pub async fn create_file(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let path = resolve_conflict(path);
    tokio::fs::write(&path, b"")
        .await
        .with_context(|| format!("failed to create file: {}", path.display()))?;
    Ok(())
}

/// Create a directory at the given path. Parent directories are created as
/// needed.
pub async fn create_dir(path: &Path) -> Result<()> {
    let path = resolve_conflict(path);
    tokio::fs::create_dir_all(&path)
        .await
        .with_context(|| format!("failed to create directory: {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use tokio::fs;

    fn setup() -> TempDir {
        TempDir::new().unwrap()
    }

    // ---------------------------------------------------------------
    // resolve_conflict
    // ---------------------------------------------------------------

    #[test]
    fn test_resolve_conflict_no_conflict() {
        let tmp = setup();
        let target = tmp.path().join("file.txt");
        assert_eq!(resolve_conflict(&target), target);
    }

    #[test]
    fn test_resolve_conflict_first_collision() {
        let tmp = setup();
        let target = tmp.path().join("file.txt");
        std::fs::write(&target, "x").unwrap();

        let resolved = resolve_conflict(&target);
        assert_eq!(resolved, tmp.path().join("file (copy).txt"));
    }

    #[test]
    fn test_resolve_conflict_multiple_collisions() {
        let tmp = setup();
        let target = tmp.path().join("file.txt");
        std::fs::write(&target, "x").unwrap();
        std::fs::write(tmp.path().join("file (copy).txt"), "x").unwrap();

        let resolved = resolve_conflict(&target);
        assert_eq!(resolved, tmp.path().join("file (2).txt"));
    }

    #[test]
    fn test_resolve_conflict_no_extension() {
        let tmp = setup();
        let target = tmp.path().join("README");
        std::fs::write(&target, "x").unwrap();

        let resolved = resolve_conflict(&target);
        assert_eq!(resolved, tmp.path().join("README (copy)"));
    }

    // ---------------------------------------------------------------
    // copy_file
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_copy_file_basic() {
        let tmp = setup();
        let src = tmp.path().join("src.txt");
        fs::write(&src, "hello").await.unwrap();

        let dst = tmp.path().join("dst.txt");
        let result = copy_file(&src, &dst).await.unwrap();

        assert_eq!(result, dst);
        assert_eq!(fs::read_to_string(&dst).await.unwrap(), "hello");
        // Source should still exist
        assert!(src.exists());
    }

    #[tokio::test]
    async fn test_copy_file_into_dir() {
        let tmp = setup();
        let src = tmp.path().join("src.txt");
        fs::write(&src, "data").await.unwrap();

        let dest_dir = tmp.path().join("subdir");
        fs::create_dir(&dest_dir).await.unwrap();

        let result = copy_file(&src, &dest_dir).await.unwrap();
        assert_eq!(result, dest_dir.join("src.txt"));
        assert!(result.exists());
    }

    #[tokio::test]
    async fn test_copy_file_conflict() {
        let tmp = setup();
        let src = tmp.path().join("file.txt");
        fs::write(&src, "original").await.unwrap();

        let dst = tmp.path().join("copy.txt");
        fs::write(&dst, "existing").await.unwrap();

        let result = copy_file(&src, &dst).await.unwrap();
        assert_eq!(result, tmp.path().join("copy (copy).txt"));
        assert_eq!(fs::read_to_string(&result).await.unwrap(), "original");
    }

    #[tokio::test]
    async fn test_copy_directory() {
        let tmp = setup();
        let src_dir = tmp.path().join("mydir");
        fs::create_dir(&src_dir).await.unwrap();
        fs::write(src_dir.join("a.txt"), "a").await.unwrap();
        fs::write(src_dir.join("b.txt"), "b").await.unwrap();

        let dst_dir = tmp.path().join("mydir_copy");
        let result = copy_file(&src_dir, &dst_dir).await.unwrap();
        assert_eq!(result, dst_dir);
        assert!(dst_dir.join("a.txt").exists());
        assert!(dst_dir.join("b.txt").exists());
    }

    // ---------------------------------------------------------------
    // move_file
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_move_file_basic() {
        let tmp = setup();
        let src = tmp.path().join("src.txt");
        fs::write(&src, "move me").await.unwrap();

        let dst = tmp.path().join("dst.txt");
        let result = move_file(&src, &dst).await.unwrap();

        assert_eq!(result, dst);
        assert!(!src.exists());
        assert_eq!(fs::read_to_string(&dst).await.unwrap(), "move me");
    }

    #[tokio::test]
    async fn test_move_file_into_dir() {
        let tmp = setup();
        let src = tmp.path().join("src.txt");
        fs::write(&src, "data").await.unwrap();

        let dest_dir = tmp.path().join("target");
        fs::create_dir(&dest_dir).await.unwrap();

        let result = move_file(&src, &dest_dir).await.unwrap();
        assert_eq!(result, dest_dir.join("src.txt"));
        assert!(!src.exists());
    }

    #[tokio::test]
    async fn test_move_file_conflict() {
        let tmp = setup();
        let src = tmp.path().join("file.txt");
        fs::write(&src, "source").await.unwrap();

        let dst = tmp.path().join("dest.txt");
        fs::write(&dst, "existing").await.unwrap();

        let result = move_file(&src, &dst).await.unwrap();
        assert_eq!(result, tmp.path().join("dest (copy).txt"));
        assert!(!src.exists());
    }

    // ---------------------------------------------------------------
    // delete_to_trash
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_delete_to_trash() {
        let tmp = setup();
        let file = tmp.path().join("trash_me.txt");
        fs::write(&file, "bye").await.unwrap();

        delete_to_trash(&file).await.unwrap();
        assert!(!file.exists());
    }

    #[tokio::test]
    async fn test_delete_to_trash_nonexistent() {
        let tmp = setup();
        let file = tmp.path().join("no_such_file.txt");
        let result = delete_to_trash(&file).await;
        assert!(result.is_err());
    }

    // ---------------------------------------------------------------
    // rename_file
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_rename_file_basic() {
        let tmp = setup();
        let src = tmp.path().join("old.txt");
        fs::write(&src, "content").await.unwrap();

        let result = rename_file(&src, "new.txt").await.unwrap();
        assert_eq!(result, tmp.path().join("new.txt"));
        assert!(!src.exists());
        assert!(result.exists());
    }

    #[tokio::test]
    async fn test_rename_file_conflict() {
        let tmp = setup();
        let src = tmp.path().join("a.txt");
        fs::write(&src, "a").await.unwrap();
        fs::write(tmp.path().join("b.txt"), "b").await.unwrap();

        let result = rename_file(&src, "b.txt").await.unwrap();
        assert_eq!(result, tmp.path().join("b (copy).txt"));
    }

    #[tokio::test]
    async fn test_rename_directory() {
        let tmp = setup();
        let dir = tmp.path().join("old_dir");
        fs::create_dir(&dir).await.unwrap();

        let result = rename_file(&dir, "new_dir").await.unwrap();
        assert_eq!(result, tmp.path().join("new_dir"));
        assert!(!dir.exists());
        assert!(result.exists());
    }

    // ---------------------------------------------------------------
    // create_file
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_create_file_basic() {
        let tmp = setup();
        let path = tmp.path().join("new.txt");
        create_file(&path).await.unwrap();
        assert!(path.exists());
        assert_eq!(fs::read_to_string(&path).await.unwrap(), "");
    }

    #[tokio::test]
    async fn test_create_file_nested() {
        let tmp = setup();
        let path = tmp.path().join("a").join("b").join("file.txt");
        create_file(&path).await.unwrap();
        assert!(path.exists());
    }

    #[tokio::test]
    async fn test_create_file_conflict() {
        let tmp = setup();
        let path = tmp.path().join("exists.txt");
        fs::write(&path, "old").await.unwrap();

        create_file(&path).await.unwrap();
        // Original should be untouched
        assert_eq!(fs::read_to_string(&path).await.unwrap(), "old");
        // Copy should exist
        assert!(tmp.path().join("exists (copy).txt").exists());
    }

    // ---------------------------------------------------------------
    // create_dir
    // ---------------------------------------------------------------

    #[tokio::test]
    async fn test_create_dir_basic() {
        let tmp = setup();
        let path = tmp.path().join("newdir");
        create_dir(&path).await.unwrap();
        assert!(path.is_dir());
    }

    #[tokio::test]
    async fn test_create_dir_nested() {
        let tmp = setup();
        let path = tmp.path().join("a").join("b").join("c");
        create_dir(&path).await.unwrap();
        assert!(path.is_dir());
    }

    #[tokio::test]
    async fn test_create_dir_conflict() {
        let tmp = setup();
        let path = tmp.path().join("mydir");
        fs::create_dir(&path).await.unwrap();

        create_dir(&path).await.unwrap();
        // The original still exists, and a copy was created
        assert!(path.is_dir());
        assert!(tmp.path().join("mydir (copy)").is_dir());
    }
}
