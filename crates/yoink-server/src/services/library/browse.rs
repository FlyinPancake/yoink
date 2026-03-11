use tokio::fs;

use yoink_shared::BrowseEntry;

use crate::{
    error::{AppError, AppResult},
    util::is_audio_extension,
};

/// List the contents of a server-side directory for the path browser UI.
///
/// Returns directories first (sorted alphabetically), then files (sorted
/// alphabetically). Hidden entries (names starting with `.`) are excluded.
pub(crate) async fn browse_path(path: &str) -> AppResult<Vec<BrowseEntry>> {
    let dir = std::path::Path::new(path);

    if !fs::try_exists(dir).await.unwrap_or(false) {
        return Err(AppError::not_found("path", Some(dir.display().to_string())));
    }

    let metadata = fs::metadata(dir).await.map_err(|err| {
        AppError::filesystem("read path metadata", dir.display().to_string(), err)
    })?;

    if !metadata.is_dir() {
        return Err(AppError::validation(
            Some("path"),
            format!("Not a directory: {}", dir.display()),
        ));
    }

    let mut dirs = Vec::new();
    let mut files = Vec::new();

    let mut entries = fs::read_dir(dir)
        .await
        .map_err(|err| AppError::filesystem("read directory", dir.display().to_string(), err))?;

    while let Some(entry) = entries.next_entry().await.map_err(|err| {
        AppError::filesystem("read directory entry", dir.display().to_string(), err)
    })? {
        let name = match entry.file_name().to_str() {
            Some(n) => n.to_string(),
            None => continue, // skip non-UTF8 names
        };

        // Skip hidden entries
        if name.starts_with('.') {
            continue;
        }

        let entry_path = entry.path();
        let entry_meta = match fs::metadata(&entry_path).await {
            Ok(m) => m,
            Err(_) => continue, // skip unreadable entries
        };

        let abs_path = entry_path.to_string_lossy().to_string();

        if entry_meta.is_dir() {
            dirs.push(BrowseEntry {
                name,
                path: abs_path,
                is_dir: true,
                is_audio: false,
            });
        } else {
            let is_audio = entry_path
                .extension()
                .and_then(|e| e.to_str())
                .is_some_and(is_audio_extension);
            files.push(BrowseEntry {
                name,
                path: abs_path,
                is_dir: false,
                is_audio,
            });
        }
    }

    dirs.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    files.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));

    dirs.extend(files);
    Ok(dirs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn browse_nonexistent_path_returns_not_found() {
        let result = browse_path("/tmp/__yoink_test_nonexistent__").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn browse_valid_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path();

        // Create test structure
        tokio::fs::create_dir(dir.join("SubDir")).await.unwrap();
        tokio::fs::write(dir.join("song.flac"), b"fake")
            .await
            .unwrap();
        tokio::fs::write(dir.join("readme.txt"), b"text")
            .await
            .unwrap();
        tokio::fs::write(dir.join(".hidden"), b"hidden")
            .await
            .unwrap();

        let entries = browse_path(dir.to_str().unwrap()).await.unwrap();

        // Hidden file should be excluded
        assert!(entries.iter().all(|e| !e.name.starts_with('.')));

        // Directory should come first
        assert!(entries[0].is_dir);
        assert_eq!(entries[0].name, "SubDir");

        // Audio file should be detected
        let flac = entries.iter().find(|e| e.name == "song.flac").unwrap();
        assert!(flac.is_audio);
        assert!(!flac.is_dir);

        // Non-audio file should not be marked as audio
        let txt = entries.iter().find(|e| e.name == "readme.txt").unwrap();
        assert!(!txt.is_audio);
        assert!(!txt.is_dir);
    }

    #[tokio::test]
    async fn browse_file_path_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        let file_path = tmp.path().join("file.txt");
        tokio::fs::write(&file_path, b"text").await.unwrap();

        let result = browse_path(file_path.to_str().unwrap()).await;
        assert!(result.is_err());
    }
}
